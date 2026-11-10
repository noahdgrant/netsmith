//! `examples/rp2350_minimal/`: the home of the port of Raspberry Pi's RP2350A Minimal board, and
//! the reference table (`reference/placements.toml`) that a parity checker will read. The table
//! is read from Raspberry Pi's board file by `cargo run -p netsmith --example reference_table`
//! (`netsmith::reference_table`); these tests run that reading on a small board of our own, and
//! prove that the committed table is what the program prints for the committed board file.

use std::fs;
use std::path::PathBuf;

use netsmith::outcome_with;
use netsmith::project::Target;
use netsmith::reference_table::placements_toml;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/rp2350_minimal")
}

const REFERENCE: &str = "RPI-RP2350A-MINIMAL_R4-S1.kicad_pcb";

fn read(path: &str) -> String {
    fs::read_to_string(project().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A board of three footprints, written the way KiCad 10 writes them: a 20 x 10 mm outline with
/// 2 mm corners (four lines, four arcs), a connector turned by -90 degrees with a plated slot,
/// an unconnected pad and a pad with an old-style `(net 3 "name")`, and a mounting hole turned
/// by 90 degrees whose pad is off its centre.
const SYNTHETIC: &str = r#"(kicad_pcb
	(version 20260206)
	(generator "pcbnew")
	(generator_version "10.0")
	(title_block (title "Tiny") (date "2026-01-02") (rev "A"))
	(gr_line (start 12 0) (end 28 0) (layer "Edge.Cuts"))
	(gr_arc (start 28 0) (mid 29.414214 0.585786) (end 30 2) (layer "Edge.Cuts"))
	(gr_line (start 30 2) (end 30 8) (layer "Edge.Cuts"))
	(gr_arc (start 30 8) (mid 29.414214 9.414214) (end 28 10) (layer "Edge.Cuts"))
	(gr_line (start 28 10) (end 12 10) (layer "Edge.Cuts"))
	(gr_arc (start 12 10) (mid 10.585786 9.414214) (end 10 8) (layer "Edge.Cuts"))
	(gr_line (start 10 8) (end 10 2) (layer "Edge.Cuts"))
	(gr_arc (start 10 2) (mid 10.585786 0.585786) (end 12 0) (layer "Edge.Cuts"))
	(gr_line (start 0 0) (end 500 500) (layer "F.SilkS"))
	(footprint "Lib:Conn"
		(layer "F.Cu")
		(at 20 5 -90)
		(property "Reference" "J1" (at 0 0 0) (layer "F.SilkS"))
		(pad "1" smd rect (at -1.5 2 270) (size 0.4 1.75) (layers "F.Cu") (net "VBUS") (pintype "power_out"))
		(pad "2" thru_hole oval (at 3 0) (size 1.1 1.8) (drill oval 0.7 1.2) (layers "*.Cu") (net 3 "GND"))
		(pad "3" smd rect (at 0 0) (size 1 1) (layers "F.Cu"))
	)
	(footprint "Lib:Cap"
		(layer "B.Cu")
		(at 15.5 4.25)
		(property "Reference" "C10" (at 0 0 0) (layer "F.SilkS"))
		(pad "1" smd rect (at -0.5 0) (size 0.6 0.6) (layers "B.Cu") (net "/a\"b"))
	)
	(footprint "Lib:Cap"
		(layer "F.Cu")
		(at 15.5 6.25)
		(property "Reference" "C9" (at 0 0 0) (layer "F.SilkS"))
	)
	(footprint "Lib:Hole"
		(layer "F.Cu")
		(at 12 8 90)
		(property "Reference" "MH1" (at 0 0 0) (layer "F.SilkS"))
		(pad "" np_thru_hole circle (at 1 0) (size 2.5 2.5) (drill 2.5) (layers "*.Cu"))
	)
)"#;

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
fn the_table_of_a_small_board_has_its_outline_holes_parts_and_pads() {
    let text = placements_toml(SYNTHETIC, "tiny.kicad_pcb").unwrap_or_else(|e| panic!("{e}"));
    let table = parsed(&text);

    let source = table["source"].as_table().unwrap();
    assert_eq!(source["file"].as_str(), Some("tiny.kicad_pcb"));
    assert_eq!(source["rev"].as_str(), Some("A"));
    assert_eq!(source["footprints"].as_integer(), Some(4));

    // The silkscreen line is not the outline; the arcs' extent is the bounding box.
    let outline = table["outline"].as_table().unwrap();
    assert_eq!(outline["must_match"].as_bool(), Some(true));
    for (key, value) in [
        ("min_x", 10.0),
        ("min_y", 0.0),
        ("max_x", 30.0),
        ("max_y", 10.0),
        ("width", 20.0),
        ("height", 10.0),
        ("corner_radius", 2.0),
    ] {
        assert!(
            (number(outline, key) - value).abs() < 1e-5,
            "{key} is {}, not {value}",
            number(outline, key)
        );
    }
    assert_eq!(outline["lines"].as_integer(), Some(4));
    assert_eq!(outline["arcs"].as_integer(), Some(4));

    // The hole's pad is 1 mm along the part's x, and the part is turned 90 degrees
    // (counter-clockwise on the screen, y down): it lands 1 mm above the part's position.
    let holes = tables(&table, "hole");
    assert_eq!(holes.len(), 1);
    assert_eq!(holes[0]["reference"].as_str(), Some("MH1"));
    assert!((number(holes[0], "x") - 12.0).abs() < 1e-9);
    assert!((number(holes[0], "y") - 7.0).abs() < 1e-9);
    assert!((number(holes[0], "drill") - 2.5).abs() < 1e-9);

    // Sorted by reference, letters first and numbers as numbers: C9 before C10.
    let parts = tables(&table, "part");
    let order: Vec<_> = parts
        .iter()
        .map(|p| p["reference"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["C9", "C10", "J1", "MH1"]);
    let must: Vec<_> = parts
        .iter()
        .map(|p| p["must_match"].as_bool().unwrap())
        .collect();
    assert_eq!(must, [false, false, true, true]);

    let c10 = parts[1];
    assert_eq!(c10["layer"].as_str(), Some("B.Cu"));
    assert_eq!(number(c10, "rotation"), 0.0);
    let pads = c10["pads"].as_array().unwrap();
    assert_eq!(pads[0].as_table().unwrap()["net"].as_str(), Some("/a\"b"));
    assert_eq!(parts[0]["pads"].as_array().unwrap().len(), 0);

    let j1 = parts[2];
    assert_eq!(j1["footprint"].as_str(), Some("Lib:Conn"));
    assert_eq!((number(j1, "x"), number(j1, "y")), (20.0, 5.0));
    assert_eq!(number(j1, "rotation"), -90.0);
    let pads: Vec<_> = j1["pads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_table().unwrap())
        .collect();
    assert_eq!(pads.len(), 3);
    assert_eq!(pads[0]["number"].as_str(), Some("1"));
    assert_eq!(pads[0]["type"].as_str(), Some("smd"));
    assert_eq!(pads[0]["shape"].as_str(), Some("rect"));
    assert_eq!((number(pads[0], "x"), number(pads[0], "y")), (-1.5, 2.0));
    assert_eq!(number(pads[0], "angle"), 270.0);
    assert_eq!(pads[0]["size"].as_array().unwrap().len(), 2);
    assert_eq!(pads[0]["net"].as_str(), Some("VBUS"));
    assert!(!pads[0].contains_key("drill") && !pads[0].contains_key("drill_slot"));
    let slot: Vec<_> = pads[1]["drill_slot"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_float().unwrap())
        .collect();
    assert_eq!(slot, [0.7, 1.2]);
    assert_eq!(pads[1]["net"].as_str(), Some("GND"));
    assert!(!pads[1].contains_key("angle"));
    assert!(!pads[2].contains_key("net"));
}

#[test]
fn the_table_says_what_it_cannot_read() {
    let wrong = |pcb: &str| placements_toml(pcb, "x.kicad_pcb").unwrap_err();
    assert!(wrong("(kicad_sch)").contains("not a kicad_pcb"));
    assert!(wrong("(kicad_pcb)").contains("no Edge.Cuts"));
    let poly = "(kicad_pcb (gr_poly (layer \"Edge.Cuts\")))";
    assert!(wrong(poly).contains("gr_poly"));
    let twice = SYNTHETIC.replace("\"C9\"", "\"C10\"");
    assert!(wrong(&twice).contains("two footprints are called C10"));
}

#[test]
fn the_committed_table_is_what_the_program_prints_for_the_committed_board_file() {
    let printed = placements_toml(&read(&format!("reference/{REFERENCE}")), REFERENCE)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        printed == read("reference/placements.toml"),
        "reference/placements.toml is not what the program prints: run `cargo run -p netsmith \
         --example reference_table -- examples/rp2350_minimal/reference/{REFERENCE} \
         > examples/rp2350_minimal/reference/placements.toml`"
    );
}

#[test]
fn the_table_holds_the_facts_of_raspberry_pis_board() {
    let table = parsed(&read("reference/placements.toml"));
    assert_eq!(table["source"]["footprints"].as_integer(), Some(43));

    let outline = table["outline"].as_table().unwrap();
    for (key, value) in [
        ("min_x", 74.0),
        ("min_y", 70.0),
        ("max_x", 126.0),
        ("max_y", 121.0),
        ("corner_radius", 3.0),
    ] {
        assert!((number(outline, key) - value).abs() < 1e-3, "{key}");
    }

    let holes: Vec<_> = tables(&table, "hole")
        .into_iter()
        .map(|h| (number(h, "x"), number(h, "y"), number(h, "drill")))
        .collect();
    assert_eq!(holes.len(), 4);
    for want in [
        (86.03, 74.0),
        (113.97, 74.0),
        (113.97, 117.18),
        (86.03, 117.18),
    ] {
        assert!(
            holes
                .iter()
                .any(|h| (h.0 - want.0).abs() < 1e-6 && (h.1 - want.1).abs() < 1e-6 && h.2 == 2.75),
            "no hole at {want:?}"
        );
    }

    let parts = tables(&table, "part");
    assert_eq!(parts.len(), 43);
    let find = |reference: &str| {
        *parts
            .iter()
            .find(|p| p["reference"].as_str() == Some(reference))
            .unwrap_or_else(|| panic!("no part {reference}"))
    };
    for (reference, x, y, rotation, pads) in [
        ("J1", 100.0, 71.7, 180.0, 15),
        ("J2", 77.14, 95.59, -90.0, 36),
        ("J3", 122.86, 95.59, -90.0, 36),
        ("J4", 107.6, 115.375, 0.0, 5),
        ("SW1", 91.2, 75.2, -90.0, 4),
        ("SW2", 111.8, 107.475, 180.0, 4),
        ("U1", 100.0, 100.0, 0.0, 70),
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
        assert_eq!(part["pads"].as_array().unwrap().len(), pads, "{reference}");
    }
    let must: Vec<_> = parts
        .iter()
        .filter(|p| p["must_match"].as_bool() == Some(true))
        .map(|p| p["reference"].as_str().unwrap())
        .collect();
    assert_eq!(
        must,
        [
            "J1", "J2", "J3", "J4", "MH1", "MH2", "MH3", "MH4", "SW1", "SW2"
        ]
    );
}

#[test]
fn the_license_is_raspberry_pis_and_keeps_its_notice() {
    let license = read("LICENSE");
    assert!(license.contains("Copyright (c) 2026 Raspberry Pi Ltd"));
    assert!(license.contains("= MIT License ="));
    assert!(license.contains(
        "The above copyright notice and this permission notice shall be included in all \
             copies or substantial portions of the Software."
    ));
    assert!(read("README.md").contains("https://pip.raspberrypi.com/documents/RP-010328-CA"));
}

#[test]
fn the_placeholder_board_checks_and_solves() {
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
    assert_eq!(names, ["Rp2350Minimal"]);
}
