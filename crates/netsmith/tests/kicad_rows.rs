//! Every row of `std/kicad.toml` against the libraries it names (ADR 0016, decision 8):
//! the pinout of the kind in that package, as the standard library's `.nsm` source gives
//! it, must agree with the symbol and the footprint, in pad numbers, pin names and
//! electrical types. The stock libraries are read when KiCad is installed; our own
//! sample libraries cover the kinds they hold.

use netsmith::Libraries;
use netsmith_check::{Loader, PinKind as LangKind, check};
use netsmith_kicad::{PartPin, PinKind, Pinout};
use netsmith_pcb::{Subject, validate_pinout};

/// The module of the standard library that defines `kind`.
const MODULES: [&str; 7] = [
    "std::parts::passive",
    "std::parts::diode",
    "std::parts::transistor",
    "std::parts::regulator",
    "std::parts::logic",
    "std::parts::connector",
    "std::parts::mechanical",
];

/// The pinout of `kind` in `package`, from the pin tables of the source.
fn pinout_of(kind: &str, package: &str) -> Pinout {
    for module in MODULES {
        // In a module: a board declares its mounting holes and fiducials in its physical file.
        let src = format!(
            "import {module}::{kind}\nmodule M {{\n    .x: {kind}\n}}\nboard B {{\n    .m: M\n}}\n"
        );
        let checked = check("test.nsm", &src, &Loader::std());
        // A required pin left open is a fault of the board, not of the kind: the board
        // still has a model.
        let Some(board) = checked.board("B") else {
            continue;
        };
        let model = &board.model;
        let Some(x) = model.inst_at("m.x") else {
            continue;
        };
        let Some(table) = model.table_for(x, package) else {
            return Pinout::default();
        };
        return Pinout {
            pins: table
                .rows
                .iter()
                .map(|r| PartPin {
                    name: model
                        .path_of(r.pin)
                        .strip_prefix("m.x.")
                        .unwrap_or_default()
                        .to_string(),
                    datasheet: r.name.clone(),
                    kind: match r.kind {
                        LangKind::Power => PinKind::Power,
                        LangKind::Input => PinKind::Input,
                        LangKind::Output => PinKind::Output,
                        LangKind::Bidirectional => PinKind::Bidirectional,
                        LangKind::Passive => PinKind::Passive,
                        LangKind::NoConnect => PinKind::NoConnect,
                        LangKind::TriState => PinKind::TriState,
                    },
                    pads: r.pads.clone(),
                    function: None,
                })
                .collect(),
        };
    }
    panic!("no module of the standard library defines `{kind}`");
}

/// Every problem of every row whose symbol the libraries hold (all of them, if
/// `skip_missing` is false).
fn problems(libraries: &Libraries, skip_missing: bool) -> (usize, Vec<String>) {
    let mut checked = 0;
    let mut problems = Vec::new();
    for row in libraries.mapping.rows() {
        let mapped = libraries
            .mapping
            .map(&row.kind, Some(&row.package))
            .unwrap();
        let (symbol, footprint) = match (
            libraries.symbols.symbol(&mapped.lib_id),
            libraries.footprints.load(&mapped.footprint),
        ) {
            (Ok(s), Ok(f)) => (s, f),
            _ if skip_missing => continue,
            (Err(e), _) => {
                problems.push(format!("{}/{}: {e}", row.kind, row.package));
                continue;
            }
            (_, Err(e)) => {
                problems.push(format!("{}/{}: {e}", row.kind, row.package));
                continue;
            }
        };
        checked += 1;
        let subject = Subject {
            refdes: "U1",
            kind: &row.kind,
            package: Some(&row.package),
        };
        problems.extend(validate_pinout(
            &subject,
            &pinout_of(&row.kind, &row.package),
            &mapped,
            &symbol,
            &footprint,
        ));
    }
    (checked, problems)
}

#[test]
fn every_row_agrees_with_our_sample_libraries_where_they_hold_it() {
    let (checked, problems) = problems(&Libraries::sample(), true);
    assert!(checked >= 12, "only {checked} rows were checked");
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn every_row_of_the_standard_mapping_agrees_with_the_installed_kicad_libraries() {
    let libraries = Libraries::standard();
    if !libraries.symbols.root().is_dir() || !libraries.footprints.root().is_dir() {
        eprintln!("skipped: no KiCad libraries");
        return;
    }
    let (checked, problems) = problems(&libraries, false);
    assert!(problems.is_empty(), "{problems:#?}");
    assert_eq!(checked, libraries.mapping.rows().len());
}
