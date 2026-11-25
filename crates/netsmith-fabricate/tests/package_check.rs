//! A catalog row's package against the footprint it names (issue #1658), with the standard
//! mapping and the generated footprints only: no KiCad install is looked for.

use std::path::Path;

use netsmith_fabricate::package_check::{Context, Found, catalog_kinds, check_files};
use netsmith_kicad::Mapping;
use netsmith_pcb::FootprintLibrary;

/// The standard library's catalogs and kinds, the given mapping and the footprints netsmith
/// generates.
fn context(mapping: Mapping) -> Context {
    let loader = netsmith_check::Loader::std();
    Context::with(
        catalog_kinds(loader.texts().map(|(_, text)| text)),
        mapping,
        FootprintLibrary::at(Path::new("no-kicad-here")).with_generators(),
    )
}

/// The standard mapping with a project's row that gives a kind's `SOT-23` the SOT-323 footprint.
fn sot_323_as_sot_23() -> Mapping {
    let project = Mapping::parse(
        r#"
[[part]]
kind = "Ldo2"
package = "SOT-23"
footprint = "Package_TO_SOT_SMD:SOT-323_SC-70"
"#,
        "kicad.toml",
        Path::new("."),
    )
    .unwrap();
    Mapping::standard().merged(project).unwrap()
}

fn file(catalog: &str, text: &str) -> (String, String, String) {
    (
        catalog.to_string(),
        format!("catalogs/{catalog}.csv"),
        text.to_string(),
    )
}

#[test]
fn a_sot_23_row_whose_footprint_is_the_sot_323_is_reported_and_a_correct_row_is_not() {
    let mut kinds = catalog_kinds(netsmith_check::Loader::std().texts().map(|(_, t)| t));
    kinds.entry("ldo2".into()).or_default().insert("Ldo2".into());
    let context = Context::with(
        kinds,
        sot_323_as_sot_23(),
        FootprintLibrary::at(Path::new("no-kicad-here")).with_generators(),
    );
    let report = check_files(
        &context,
        &[
            file(
                "ldo2",
                "mpn,package,source\nWRONG-1,SOT-23,a datasheet\n",
            ),
            // The standard `ldo` catalog's kind maps SOT-23 to the SOT-23 footprint.
            file("ldo", "mpn,package,source\nRIGHT-1,SOT-23,a datasheet\n"),
        ],
    )
    .unwrap();
    let wrong: Vec<_> = report.mismatches().collect();
    assert_eq!(wrong.len(), 1, "{report:?}");
    assert_eq!((wrong[0].mpn.as_str(), wrong[0].line), ("WRONG-1", 2));
    let text = netsmith_fabricate::package_check::render(&report);
    // Both values and the source.
    assert!(
        text.contains("catalogs/ldo2.csv:2: WRONG-1 (ldo2): the footprint `Package_TO_SOT_SMD:SOT-323_SC-70` draws a body 1.35 x 2.2 mm"),
        "{text}"
    );
    assert!(text.contains("1.2-1.4 x 2.8-3 mm"), "{text}");
    assert!(text.contains("JSCJ 2N7002 datasheet"), "{text}");
    assert!(text.contains("kicad.toml line"), "{text}");
    let right = report.rows.iter().find(|r| r.mpn == "RIGHT-1").unwrap();
    assert!(
        matches!(&right.found, Found::Measured(m) if m.iter().all(|m| m.problems.is_empty() && m.unread.is_none())),
        "{right:?}"
    );
}

#[test]
fn every_built_in_row_fits_its_footprint() {
    let context = context(Mapping::standard());
    let files: Vec<_> = netsmith_solve::built_in_files()
        .iter()
        .map(|(name, shown, text)| (name.to_string(), shown.to_string(), text.to_string()))
        .collect();
    let report = check_files(&context, &files).unwrap();
    let text = netsmith_fabricate::package_check::render(&report);
    assert_eq!(report.mismatches().count(), 0, "{text}");
    // Every footprint of a known package was read and measured, so the pass is not empty.
    assert!(report.unread().is_empty(), "{text}");
    assert!(report.no_footprint().is_empty(), "{text}");
    assert!(report.measured() > 300, "{text}");
    println!("{text}");
}

#[test]
fn a_record_whose_description_names_another_package_is_reported() {
    let context = context(Mapping::standard());
    let found = context.check_record(
        "n_mosfet",
        Some("SOT-23"),
        &["MOSFET N-CH 60V 115mA SOT323"],
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`SOT-23`") && found[0].contains("`SOT-323`"), "{found:?}");
    assert!(
        context
            .check_record(
                "n_mosfet",
                Some("SOT-23"),
                &["N-Channel 60V 115mA 0.225W Surface Mount SOT-23"]
            )
            .is_empty()
    );
}
