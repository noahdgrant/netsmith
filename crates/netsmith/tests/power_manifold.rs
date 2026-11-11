//! `examples/power_manifold_charger/`: the home of the port of the Power Manifold charger module
//! (the fourth showcase board of epic #308), its licence and its reference table
//! (`reference/placements.toml`), read from the author's board file by
//! `cargo run -p netsmith --example reference_table` (`netsmith::reference_table`).

use std::fs;
use std::path::PathBuf;

use netsmith::outcome_with;
use netsmith::project::Target;
use netsmith::reference_table::{MustMatch, placements_toml_with};
use netsmith_netlist::Netlist;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/power_manifold_charger")
}

const REFERENCE: &str = "charger-module.kicad_pcb";

fn read(path: &str) -> String {
    fs::read_to_string(project().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn parsed(text: &str) -> toml::Table {
    text.parse::<toml::Table>()
        .unwrap_or_else(|e| panic!("the table is not TOML: {e}"))
}

fn tables<'a>(table: &'a toml::Table, key: &str) -> Vec<&'a toml::Table> {
    table[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is not an array"))
        .iter()
        .map(|v| v.as_table().unwrap_or_else(|| panic!("{key}: not a table")))
        .collect()
}

fn number(table: &toml::Table, key: &str) -> f64 {
    match &table[key] {
        toml::Value::Float(f) => *f,
        toml::Value::Integer(i) => *i as f64,
        other => panic!("{key} is {other:?}, not a number"),
    }
}

#[test]
fn the_committed_table_is_what_the_program_prints_for_the_committed_board_file() {
    let must = MustMatch::from_toml(&read("reference/must_match.toml"))
        .unwrap_or_else(|e| panic!("reference/must_match.toml: {e}"));
    let printed = placements_toml_with(&read(&format!("reference/{REFERENCE}")), REFERENCE, &must)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        printed == read("reference/placements.toml"),
        "reference/placements.toml is not what the program prints: run `cargo run -p netsmith \
         --example reference_table -- --must-match reference/must_match.toml \
         reference/{REFERENCE} > reference/placements.toml` from examples/power_manifold_charger"
    );
}

#[test]
fn the_table_holds_the_facts_of_the_charger_board() {
    let table = parsed(&read("reference/placements.toml"));
    assert_eq!(table["source"]["footprints"].as_integer(), Some(90));
    assert_eq!(table["source"]["version"].as_str(), Some("20260206"));
    assert_eq!(table["source"]["rev"].as_str(), Some("v0.16.2"));

    // The outline: four lines on the board for the body (x 63.2 to 156, y 26 to 50) and the
    // lines and the arc of the edge connector's footprint (J3), which draws the card's tab and
    // its key slot to x 54.8.
    let outline = table["outline"].as_table().unwrap();
    for (key, value) in [
        ("min_x", 54.8),
        ("min_y", 26.0),
        ("max_x", 156.0),
        ("max_y", 50.0),
        ("width", 101.2),
        ("height", 24.0),
    ] {
        assert!((number(outline, key) - value).abs() < 1e-3, "{key}");
    }
    assert_eq!(outline["lines"].as_integer(), Some(14));
    assert_eq!(outline["arcs"].as_integer(), Some(1));

    // The non-plated holes: the three of the programming pads (J2) and the two pegs of the
    // USB-C receptacle (J4).
    let holes: Vec<_> = tables(&table, "hole")
        .into_iter()
        .map(|h| (h["reference"].as_str().unwrap(), number(h, "drill")))
        .collect();
    assert_eq!(
        holes,
        [
            ("J2", 0.9906),
            ("J2", 0.9906),
            ("J2", 0.9906),
            ("J4", 0.6),
            ("J4", 0.6)
        ]
    );

    let parts = tables(&table, "part");
    assert_eq!(parts.len(), 90);
    let find = |reference: &str| {
        *parts
            .iter()
            .find(|p| p["reference"].as_str() == Some(reference))
            .unwrap_or_else(|| panic!("no part {reference}"))
    };
    for (reference, x, y, rotation, layer, pads) in [
        ("J2", 137.3, 47.3, 0.0, "F.Cu", 9),
        ("J3", 58.25, 45.65, 90.0, "B.Cu", 36),
        ("J4", 152.13, 38.0, 90.0, "F.Cu", 22),
        ("U1", 125.12, 40.45, -90.0, "F.Cu", 48),
        ("U3", 94.74, 34.6, 90.0, "F.Cu", 26),
    ] {
        let part = find(reference);
        assert_eq!(
            (
                number(part, "x"),
                number(part, "y"),
                number(part, "rotation")
            ),
            (x, y, rotation),
            "{reference}"
        );
        assert_eq!(part["layer"].as_str(), Some(layer), "{reference}");
        assert_eq!(part["pads"].as_array().unwrap().len(), pads, "{reference}");
    }

    // Must match: the outline and the holes (above), the programming pads, the edge connector
    // and the USB-C receptacle.
    let must: Vec<_> = parts
        .iter()
        .filter(|p| p["must_match"].as_bool() == Some(true))
        .map(|p| p["reference"].as_str().unwrap())
        .collect();
    assert_eq!(must, ["J2", "J3", "J4"]);
    assert_eq!(table["outline"]["must_match"].as_bool(), Some(true));
    assert!(
        tables(&table, "hole")
            .iter()
            .all(|h| h["must_match"].as_bool() == Some(true))
    );
}

#[test]
fn the_board_file_reads_as_a_netlist_of_89_parts() {
    let netlist = Netlist::from_kicad_pcb(&read(&format!("reference/{REFERENCE}")))
        .unwrap_or_else(|e| panic!("{e}"));
    // 90 footprints, one of them the HOT_SURFACE marker (`REF**`, no pad).
    assert_eq!(netlist.components.len(), 89);
    // An unconnected pad is on no net.
    assert!(
        netlist
            .nets
            .iter()
            .all(|n| !n.name.starts_with("unconnected-"))
    );
    let net = |name: &str| {
        netlist
            .nets
            .iter()
            .find(|n| n.name == name)
            .unwrap_or_else(|| panic!("no net {name}"))
    };
    // The USB-C receptacle's four data pads (A6, A7, B6, B7) are two nets of two pins, and the
    // supply of the module is the net of the edge connector's 5 V pins.
    assert!(
        net("/USB_DATA")
            .pins
            .iter()
            .any(|(c, p)| netlist.components[*c].refdes == "J4" && p == "A6")
    );
    assert!(
        net("+5V")
            .pins
            .iter()
            .any(|(c, p)| netlist.components[*c].refdes == "J3" && p == "A12")
    );
}

#[test]
fn the_license_is_the_cern_ohl_p_text_with_its_notice() {
    let license = read("LICENSE");
    assert!(license.starts_with("CERN Open Hardware Licence Version 2 - Permissive"));
    assert!(license.contains("1.1 'Licence' means this CERN-OHL-P."));
    assert!(license.contains("The CERN-OHL-P is copyright CERN 2020."));
    // 3.1 and 3.3: copies keep every Notice, and a modified copy says it was modified.
    assert!(license.contains("3.1 You may copy and Convey verbatim copies of Covered Source"));
    assert!(license.contains("b) add a Notice to the modified Covered Source stating that You"));
    let readme = read("README.md");
    assert!(readme.contains("https://github.com/mikesmitty/power-manifold"));
    assert!(readme.contains("CERN-OHL-P-2.0"));
    assert!(readme.contains("dd24000a9ce0cac567da674c473ded4cff34bdbc"));
    assert!(readme.contains("Copyright 2024-2026 Michael Smith"));
}

#[test]
fn the_board_checks_and_solves() {
    let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
    let (loader, catalogs) = (target.loader().unwrap(), target.catalogs().unwrap());
    assert_eq!(target.files.len(), 1);
    let source = &target.files[0];
    let text = fs::read_to_string(&source.path).unwrap();
    let outcome = outcome_with(&source.name, &text, &loader, &catalogs, None)
        .unwrap_or_else(|p| panic!("{}: {}", source.name, p[0].rendered));
    let names: Vec<_> = outcome
        .boards
        .iter()
        .map(|b| b.design.name.as_str())
        .collect();
    assert_eq!(names, ["PowerManifoldCharger"]);
}
