//! Every row of `std/kicad.toml` against the libraries it names (ADR 0016, decision 8):
//! the pinout of the kind in that package, as the standard library's `.nsm` source gives
//! it, must agree with the symbol and the footprint, in pad numbers, pin names and
//! electrical types. The stock libraries are read when KiCad is installed; our own
//! sample libraries cover the kinds they hold.

use netsmith_pipeline::Libraries;
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
pub(crate) fn pinout_of(kind: &str, package: &str) -> Pinout {
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
                        LangKind::OpenDrain => PinKind::OpenDrain,
                    },
                    pads: r.pads.clone(),
                    function: None,
                    power_flow: None,
                })
                .collect(),
        };
    }
    panic!("no module of the standard library defines `{kind}`");
}

/// The part a row stands for, with the pin table of its kind in its package: what the symbol
/// of a row with no `symbol` is drawn from.
pub(crate) fn part_of(kind: &str, package: &str) -> netsmith_core::Part {
    netsmith_core::Part {
        refdes: "U1".into(),
        kind: kind.into(),
        provides: Vec::new(),
        value: String::new(),
        package: Some(package.into()),
        path: "u1".into(),
        pins: pinout_of(kind, package).pins,
    }
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
            libraries
                .symbols
                .symbol_of(&part_of(&row.kind, &row.package), &mapped),
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

// ---- netsmith's own footprints and symbols (std/libs/, ADR 0087) ----

/// The rows that name the standard library's own symbol or footprint.
fn own_rows(libraries: &Libraries) -> Vec<&netsmith_kicad::Row> {
    libraries
        .mapping
        .rows()
        .iter()
        .filter(|r| {
            netsmith_kicad::is_library_id(&r.footprint)
                || r.symbol
                    .as_deref()
                    .is_some_and(netsmith_kicad::is_library_id)
        })
        .collect()
}

/// A library that reads nothing from a folder: whatever it finds is in the binary.
fn bare() -> Libraries {
    Libraries::with_mapping(
        netsmith_kicad::Mapping::standard(),
        netsmith_kicad::SymbolLib::at("/nonexistent"),
        netsmith_pcb::FootprintLibrary::at("/nonexistent"),
    )
}

/// The symbol of a row with no KiCad install, and the mapping to check it with: the library's
/// if the row names netsmith's own or none, and else (a row that still names a KiCad symbol,
/// while its footprint is netsmith's own) the box drawn from the part's pin table, which
/// has the datasheet's pin names, so the mapping's renames of KiCad's pins are left out and
/// the footprint's pads are what is checked against the pins. The row's KiCad symbol is
/// checked against the pin table by `problems` where KiCad is installed.
fn symbol_without_kicad(
    libraries: &Libraries,
    row: &netsmith_kicad::Row,
    mapped: &netsmith_kicad::Mapped,
) -> (
    std::sync::Arc<netsmith_kicad::Symbol>,
    netsmith_kicad::Mapped,
) {
    let part = part_of(&row.kind, &row.package);
    let own = row
        .symbol
        .as_deref()
        .is_none_or(netsmith_kicad::is_library_id);
    if own {
        let symbol = libraries
            .symbols
            .symbol_of(&part, mapped)
            .unwrap_or_else(|e| panic!("{}/{}: {e}", row.kind, row.package));
        (symbol, mapped.clone())
    } else {
        let symbol = netsmith_kicad::generated_symbol(&part, &mapped.footprint)
            .unwrap_or_else(|| panic!("{}/{} has no pin table", row.kind, row.package));
        let plain = netsmith_kicad::Mapped {
            pin_names: Default::default(),
            ..mapped.clone()
        };
        (std::sync::Arc::new(symbol), plain)
    }
}

#[test]
fn every_footprint_netsmith_ships_parses_and_agrees_with_the_pin_table_of_its_part() {
    let libraries = bare();
    let rows = own_rows(&libraries);
    assert!(!rows.is_empty());
    let mut checked = 0;
    for row in rows {
        let mapped = libraries
            .mapping
            .map(&row.kind, Some(&row.package))
            .unwrap();
        let (symbol, mapped) = symbol_without_kicad(&libraries, row, &mapped);
        let footprint = libraries
            .footprints
            .load(&mapped.footprint)
            .unwrap_or_else(|e| panic!("{}/{}: {e}", row.kind, row.package));
        let subject = Subject {
            refdes: "U1",
            kind: &row.kind,
            package: Some(&row.package),
        };
        let problems = validate_pinout(
            &subject,
            &pinout_of(&row.kind, &row.package),
            &mapped,
            &symbol,
            &footprint,
        );
        assert!(problems.is_empty(), "{problems:#?}");
        checked += 1;
    }
    assert!(checked >= 1);
}

/// ADR 0097, step 3: every footprint of the standard mapping is generated or one of netsmith's
/// own files, so a library that reads nothing from a folder answers all of them.
#[test]
fn every_footprint_of_the_standard_mapping_is_answered_with_no_kicad_footprint_library() {
    let libraries = Libraries::with_mapping(
        netsmith_kicad::Mapping::standard(),
        netsmith_kicad::SymbolLib::at("/nonexistent").with_generators(),
        netsmith_pcb::FootprintLibrary::at("/nonexistent").with_generators(),
    );
    let mut missing = Vec::new();
    for row in libraries.mapping.rows() {
        if let Err(e) = libraries.footprints.load(&row.footprint) {
            missing.push(format!("{}/{}: {e}", row.kind, row.package));
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

#[test]
fn every_file_of_the_footprint_library_is_named_by_a_row_so_that_it_is_checked() {
    let libraries = bare();
    let named: Vec<&str> = libraries
        .mapping
        .rows()
        .iter()
        .filter_map(|r| r.footprint.strip_prefix("netsmith:"))
        .collect();
    for name in netsmith_kicad::stdlib_footprint_names() {
        assert!(
            named.contains(&name),
            "std/libs/netsmith.pretty/{name}.kicad_mod is in no row of std/kicad.toml, so no \
             pin table is checked against its pads"
        );
    }
}

#[test]
fn a_footprint_whose_pad_disagrees_with_the_pin_table_is_refused_by_name() {
    // The datasheet's pad 22 is SW; a pinout that calls it something the footprint lacks, or a
    // footprint with a pad that the pinout does not name, is a message that names both.
    let libraries = bare();
    let row = own_rows(&libraries)
        .into_iter()
        .find(|r| r.footprint.contains("UDC-QFN-18"))
        .unwrap();
    let mapped = libraries
        .mapping
        .map(&row.kind, Some(&row.package))
        .unwrap();
    let (symbol, mapped) = symbol_without_kicad(&libraries, row, &mapped);
    let footprint = libraries.footprints.load(&mapped.footprint).unwrap();
    let mut pinout = pinout_of(&row.kind, &row.package);
    // The pin table with pad 22 taken away from SW.
    for pin in &mut pinout.pins {
        pin.pads.retain(|p| p != "22");
    }
    let subject = Subject {
        refdes: "U1",
        kind: &row.kind,
        package: Some(&row.package),
    };
    let problems = validate_pinout(&subject, &pinout, &mapped, &symbol, &footprint);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("has pad 22, which no pin of the part has")),
        "{problems:#?}"
    );
}

// ---- symbols drawn from pin tables (ADR 0097, step 1) ----

#[test]
fn every_ic_row_of_the_standard_mapping_names_no_symbol_and_is_drawn_from_its_pin_table_with_no_kicad()
 {
    let libraries = bare();
    let mut checked = Vec::new();
    for row in libraries
        .mapping
        .rows()
        .iter()
        .filter(|r| r.symbol.is_none())
    {
        let part = part_of(&row.kind, &row.package);
        assert!(
            !part.pins.is_empty(),
            "{}/{} names no symbol and has no pin table",
            row.kind,
            row.package
        );
        let mapped = libraries
            .mapping
            .map(&row.kind, Some(&row.package))
            .unwrap();
        // Neither a library folder nor an install is read: the symbol is made here.
        let symbol = libraries.symbols.symbol_of(&part, &mapped).unwrap();
        let mut drawn: Vec<(String, String)> = symbol
            .pins()
            .iter()
            .map(|p| (p.number.clone(), p.name.clone()))
            .collect();
        let mut table: Vec<(String, String)> = part
            .pins
            .iter()
            .flat_map(|p| p.pads.iter().map(|pad| (pad.clone(), p.datasheet.clone())))
            .collect();
        drawn.sort();
        table.sort();
        assert_eq!(drawn, table, "{}/{}", row.kind, row.package);
        checked.push(row.kind.as_str());
    }
    for kind in ["Lt8641", "LevelTranslator", "Sn74lvc1g04"] {
        assert!(checked.contains(&kind), "{kind} still names a symbol");
    }
}

#[test]
fn a_row_with_no_symbol_for_a_part_with_no_pin_table_says_so() {
    let libraries = bare();
    let mapped = libraries.mapping.map("Lt8641", Some("UDC20(18)")).unwrap();
    let mut part = part_of("Lt8641", "UDC20(18)");
    part.pins.clear();
    let e = libraries
        .symbols
        .symbol_of(&part, &mapped)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("no `symbol`") && e.contains("`Lt8641`") && e.contains("no pin table"),
        "{e}"
    );
}
