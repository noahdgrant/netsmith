//! `examples/easyduino_esp32s3/`: the home of the port of the Easyduino ESP32-S3 board (the
//! second showcase board of epic #308), its licence and its reference table
//! (`reference/placements.toml`), read from the author's board file by
//! `cargo run -p netsmith --example reference_table` (`netsmith::reference_table`).
//!
//! This is the first board whose file draws its outline inside a footprint, has four copper
//! layers and has parts that carry several pads of one number: the first tests run the reading
//! on small boards of our own that have each of those, and the rest prove what the committed
//! table and the placeholder hold.

use std::fs;
use std::path::PathBuf;

use netsmith::outcome_with;
use netsmith::project::Target;
use netsmith::reference_table::{MustMatch, placements_toml, placements_toml_with, read_board};
use netsmith_netlist::Netlist;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/easyduino_esp32s3")
}

const REFERENCE: &str = "Easyduino_ESP32S3.kicad_pcb";

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

fn outline_of(pcb: &str) -> toml::Table {
    let text = placements_toml(pcb, "tiny.kicad_pcb").unwrap_or_else(|e| panic!("{e}"));
    parsed(&text)["outline"].as_table().unwrap().clone()
}

fn assert_box(outline: &toml::Table, want: [f64; 4]) {
    for (key, value) in ["min_x", "min_y", "max_x", "max_y"].iter().zip(want) {
        assert!(
            (number(outline, key) - value).abs() < 1e-5,
            "{key} is {}, not {value}",
            number(outline, key)
        );
    }
}

/// The outline drawn by a footprint, as the Easyduino's board file has it (a development board's
/// footprint with its own `Edge.Cuts`): four `fp_line`s of a 10 x 20 mm rectangle in the
/// footprint's frame, the footprint turned by 90 degrees. With KiCad's rotation (counter-
/// clockwise on the screen, y down) a footprint point (x, y) lands at (y, -x) from the position,
/// so the rectangle spans x 100 to 120 and y 40 to 50.
const FOOTPRINT_OUTLINE: &str = r#"(kicad_pcb
	(version 20260206)
	(footprint "Lib:Devkit"
		(layer "F.Cu")
		(at 100 50 90)
		(property "Reference" "U2" (at 0 0 0) (layer "F.SilkS"))
		(fp_line (start 0 0) (end 10 0) (stroke (width 0.12) (type solid)) (layer "Edge.Cuts"))
		(fp_line (start 10 0) (end 10 20) (stroke (width 0.12) (type solid)) (layer "Edge.Cuts"))
		(fp_line (start 10 20) (end 0 20) (stroke (width 0.12) (type solid)) (layer "Edge.Cuts"))
		(fp_line (start 0 20) (end 0 0) (stroke (width 0.12) (type solid)) (layer "Edge.Cuts"))
		(fp_line (start -5 -5) (end 50 50) (stroke (width 0.12) (type solid)) (layer "F.SilkS"))
		(pad "1" thru_hole oval (at 1 1 270) (size 1.2 2) (drill 0.8) (layers "*.Cu" "*.Mask") (net "A"))
	)
)"#;

#[test]
fn an_outline_drawn_by_a_footprint_is_read_in_the_boards_frame() {
    let outline = outline_of(FOOTPRINT_OUTLINE);
    assert_box(&outline, [100.0, 40.0, 120.0, 50.0]);
    assert_eq!(outline["lines"].as_integer(), Some(4));
    assert_eq!(outline["arcs"].as_integer(), Some(0));
}

#[test]
fn the_outline_of_a_board_joins_what_the_board_and_its_footprints_draw() {
    // The board draws the bottom edge; a footprint at (20, 0) draws the other three.
    let pcb = r#"(kicad_pcb
	(gr_line (start 0 10) (end 30 10) (stroke (width 0.1) (type solid)) (layer "Edge.Cuts"))
	(footprint "Lib:Frame"
		(layer "F.Cu")
		(at 20 0)
		(property "Reference" "U2" (at 0 0 0) (layer "F.SilkS"))
		(fp_line (start -20 10) (end -20 0) (layer "Edge.Cuts"))
		(fp_line (start -20 0) (end 10 0) (layer "Edge.Cuts"))
		(fp_line (start 10 0) (end 10 10) (layer "Edge.Cuts"))
	)
)"#;
    let outline = outline_of(pcb);
    assert_box(&outline, [0.0, 0.0, 30.0, 10.0]);
    assert_eq!(outline["lines"].as_integer(), Some(4));
}

#[test]
fn a_footprints_rectangle_arc_and_polygon_on_the_edge_are_read() {
    // fp_rect: two corners; the footprint is turned by 90 degrees: (x, y) -> (y, -x).
    let rect = r#"(kicad_pcb (footprint "L:F" (layer "F.Cu") (at 100 50 90)
		(property "Reference" "U2" (at 0 0 0) (layer "F.SilkS"))
		(fp_rect (start 0 0) (end 10 20) (stroke (width 0.1) (type solid)) (fill no) (layer "Edge.Cuts"))))"#;
    let outline = outline_of(rect);
    assert_box(&outline, [100.0, 40.0, 120.0, 50.0]);
    assert_eq!(outline["lines"].as_integer(), Some(4));

    // fp_arc: a half circle of radius 5 round (0, 0) from (5, 0) through (0, 5) to (-5, 0),
    // in a footprint at (10, 10) not turned: it spans x 5 to 15 and y 10 to 15.
    let arc = r#"(kicad_pcb (footprint "L:F" (layer "F.Cu") (at 10 10)
		(property "Reference" "U2" (at 0 0 0) (layer "F.SilkS"))
		(fp_arc (start 5 0) (mid 0 5) (end -5 0) (stroke (width 0.1) (type solid)) (layer "Edge.Cuts"))))"#;
    let outline = outline_of(arc);
    assert_box(&outline, [5.0, 10.0, 15.0, 15.0]);
    assert_eq!(outline["arcs"].as_integer(), Some(1));
    assert_eq!(number(&outline, "corner_radius"), 5.0);

    // fp_poly and gr_poly: a triangle, three edges.
    let poly = r#"(kicad_pcb (footprint "L:F" (layer "F.Cu") (at 10 10)
		(property "Reference" "U2" (at 0 0 0) (layer "F.SilkS"))
		(fp_poly (pts (xy 0 0) (xy 10 0) (xy 0 6)) (stroke (width 0.1) (type solid)) (fill no) (layer "Edge.Cuts"))))"#;
    let outline = outline_of(poly);
    assert_box(&outline, [10.0, 10.0, 20.0, 16.0]);
    assert_eq!(outline["lines"].as_integer(), Some(3));
    let poly = r#"(kicad_pcb (gr_poly (pts (xy 1 2) (xy 11 2) (xy 11 8) (xy 1 8))
		(stroke (width 0.1) (type solid)) (fill no) (layer "Edge.Cuts")))"#;
    let outline = outline_of(poly);
    assert_box(&outline, [1.0, 2.0, 11.0, 8.0]);
    assert_eq!(outline["lines"].as_integer(), Some(4));
}

#[test]
fn an_edge_shape_that_is_not_read_is_named_with_the_footprint_that_draws_it() {
    let curve = r#"(kicad_pcb (footprint "L:F" (layer "F.Cu") (at 0 0)
		(property "Reference" "U2" (at 0 0 0) (layer "F.SilkS"))
		(fp_curve (pts (xy 0 0) (xy 1 1) (xy 2 1) (xy 3 0)) (layer "Edge.Cuts"))))"#;
    let e = placements_toml(curve, "x.kicad_pcb").unwrap_err();
    assert!(e.contains("fp_curve") && e.contains("U2"), "{e}");
    let circle = r#"(kicad_pcb (gr_circle (center 5 5) (end 7 5) (layer "Edge.Cuts")))"#;
    let e = placements_toml(circle, "x.kicad_pcb").unwrap_err();
    assert!(e.contains("gr_circle"), "{e}");
    let arc_in_poly = r#"(kicad_pcb (gr_poly (pts (xy 0 0) (arc (start 1 0) (mid 2 1) (end 1 2)))
		(layer "Edge.Cuts")))"#;
    let e = placements_toml(arc_in_poly, "x.kicad_pcb").unwrap_err();
    assert!(e.contains("gr_poly"), "{e}");
}

#[test]
fn a_pad_of_net_zero_has_no_net() {
    // Some writers state an unconnected pad as `(net 0 "")`: that is no net, as for a pad with no
    // `(net ...)` at all.
    let pcb = r#"(kicad_pcb
	(gr_rect (start 0 0) (end 10 10) (stroke (width 0.1) (type solid)) (layer "Edge.Cuts"))
	(footprint "L:F" (layer "F.Cu") (at 5 5)
		(property "Reference" "R1" (at 0 0 0) (layer "F.SilkS"))
		(pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu") (net 0 ""))
		(pad "2" smd rect (at 1 0) (size 1 1) (layers "F.Cu") (net 2 "N"))))"#;
    let text = placements_toml(pcb, "x.kicad_pcb").unwrap_or_else(|e| panic!("{e}"));
    let table = parsed(&text);
    let pads = tables(&table, "part")[0]["pads"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!pads[0].as_table().unwrap().contains_key("net"), "{text}");
    assert_eq!(pads[1].as_table().unwrap()["net"].as_str(), Some("N"));
}

#[test]
fn a_must_match_list_says_which_footprints_a_copy_must_reproduce() {
    let must = MustMatch::from_toml("parts = [\"U2\", \"J1\"]\n").unwrap_or_else(|e| panic!("{e}"));
    assert!(must.contains("U2") && must.contains("J1"));
    assert!(!must.contains("J12") && !must.contains("U1"));

    let pcb = r#"(kicad_pcb
	(gr_rect (start 0 0) (end 10 10) (stroke (width 0.1) (type solid)) (layer "Edge.Cuts"))
	(footprint "L:F" (layer "F.Cu") (at 1 1) (property "Reference" "J1" (at 0 0 0) (layer "F.SilkS")))
	(footprint "L:F" (layer "F.Cu") (at 1 1) (property "Reference" "JP1" (at 0 0 0) (layer "F.SilkS")))
	(footprint "L:F" (layer "F.Cu") (at 1 1) (property "Reference" "U1" (at 0 0 0) (layer "F.SilkS")))
	(footprint "L:F" (layer "F.Cu") (at 1 1) (property "Reference" "U2" (at 0 0 0) (layer "F.SilkS"))))"#;
    let text = placements_toml_with(pcb, "x.kicad_pcb", &must).unwrap_or_else(|e| panic!("{e}"));
    let table = parsed(&text);
    let flags: Vec<_> = tables(&table, "part")
        .iter()
        .map(|p| {
            (
                p["reference"].as_str().unwrap(),
                p["must_match"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        flags,
        [("J1", true), ("JP1", false), ("U1", false), ("U2", true)]
    );

    // The default is Raspberry Pi's list, so their table does not change.
    let default = placements_toml(pcb, "x.kicad_pcb").unwrap();
    assert!(default.contains("J1 to J4, SW1, SW2"));
    assert_eq!(
        default,
        placements_toml_with(pcb, "x.kicad_pcb", &MustMatch::default()).unwrap()
    );
}

#[test]
fn a_must_match_list_that_names_a_missing_footprint_is_refused() {
    // A listed designator the board does not have is a mistake, not a part that matches nothing.
    let pcb = r#"(kicad_pcb
	(gr_rect (start 0 0) (end 10 10) (stroke (width 0.1) (type solid)) (layer "Edge.Cuts"))
	(footprint "L:F" (layer "F.Cu") (at 1 1) (property "Reference" "J1" (at 0 0 0) (layer "F.SilkS"))))"#;
    let must = MustMatch::from_toml("parts = [\"J1\", \"J9\"]").unwrap();
    let e = placements_toml_with(pcb, "x.kicad_pcb", &must).unwrap_err();
    assert!(e.contains("J9"), "{e}");
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
         reference/{REFERENCE} > reference/placements.toml` from examples/easyduino_esp32s3"
    );
}

#[test]
fn the_table_holds_the_facts_of_the_easyduino_board() {
    let table = parsed(&read("reference/placements.toml"));
    assert_eq!(table["source"]["footprints"].as_integer(), Some(35));
    assert_eq!(table["source"]["version"].as_str(), Some("20260206"));

    // The outline is drawn by U2, the footprint of the development board's headers.
    let outline = table["outline"].as_table().unwrap();
    for (key, value) in [
        ("min_x", 127.105),
        ("min_y", 58.6514),
        ("max_x", 152.965),
        ("max_y", 122.5486),
        ("width", 25.86),
        ("height", 63.8972),
    ] {
        assert!((number(outline, key) - value).abs() < 1e-3, "{key}");
    }
    assert_eq!(outline["lines"].as_integer(), Some(4));
    assert_eq!(outline["arcs"].as_integer(), Some(0));

    // The only non-plated holes are the two pegs of each USB-C receptacle.
    let holes: Vec<_> = tables(&table, "hole")
        .into_iter()
        .map(|h| (h["reference"].as_str().unwrap(), number(h, "drill")))
        .collect();
    assert_eq!(
        holes,
        [("J1", 0.65), ("J1", 0.65), ("J2", 0.65), ("J2", 0.65)]
    );

    let parts = tables(&table, "part");
    assert_eq!(parts.len(), 35);
    let find = |reference: &str| {
        *parts
            .iter()
            .find(|p| p["reference"].as_str() == Some(reference))
            .unwrap_or_else(|| panic!("no part {reference}"))
    };
    for (reference, x, y, rotation, pads) in [
        ("J1", 133.08, 119.925, 0.0, 22),
        ("J2", 146.95, 119.925, 0.0, 22),
        ("SW1", 136.3, 93.2, 0.0, 2),
        ("SW2", 143.9, 93.2, 180.0, 2),
        ("U1", 140.07, 68.5, 0.0, 49),
        ("U2", 128.605, 59.75136, 0.0, 44),
        ("U3", 134.2, 107.1, 90.0, 38),
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

    // Must match: the outline (above), the receptacles, the buttons and the headers (U2).
    // Not required: the module, the bridge, the regulator, the transistors, the LEDs, the
    // passives and the routing.
    let must: Vec<_> = parts
        .iter()
        .filter(|p| p["must_match"].as_bool() == Some(true))
        .map(|p| p["reference"].as_str().unwrap())
        .collect();
    assert_eq!(must, ["J1", "J2", "SW1", "SW2", "U2"]);
    assert_eq!(table["outline"]["must_match"].as_bool(), Some(true));
    assert!(
        tables(&table, "hole")
            .iter()
            .all(|h| h["must_match"].as_bool() == Some(true))
    );

    // The reader returns the same outline size from the board file.
    let board = read_board(&read(&format!("reference/{REFERENCE}")), REFERENCE).unwrap();
    assert!((board.outline.max_x - board.outline.min_x - 25.86).abs() < 1e-3);
}

/// Every pad of every footprint is in the table, and a pad number several pads share (the
/// module's ground pad is nine pads, a receptacle's shield is four, the regulator's tab is
/// one more `2`) is not lost or merged.
#[test]
fn pads_that_share_a_number_are_all_in_the_table() {
    let table = parsed(&read("reference/placements.toml"));
    let parts = tables(&table, "part");
    let count = |reference: &str, number: &str| {
        let part = parts
            .iter()
            .find(|p| p["reference"].as_str() == Some(reference));
        part.unwrap()["pads"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p.as_table().unwrap()["number"].as_str() == Some(number))
            .count()
    };
    assert_eq!(count("U1", "41"), 9);
    assert_eq!(count("J1", "S1"), 4);
    assert_eq!(count("J2", "S1"), 4);
    assert_eq!(count("U5", "2"), 2);
    // The two pegs of a receptacle have no number.
    assert_eq!(count("J1", ""), 2);
}

#[test]
fn the_board_file_reads_as_a_netlist_of_35_parts() {
    let netlist = Netlist::from_kicad_pcb(&read(&format!("reference/{REFERENCE}")))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(netlist.components.len(), 35);
    let net = |name: &str| {
        netlist
            .nets
            .iter()
            .find(|n| n.name == name)
            .unwrap_or_else(|| panic!("no net {name}"))
    };
    // A pad number that several pads share is one pin.
    let gnd = net("GND");
    let on = |reference: &str, pin: &str| {
        gnd.pins
            .iter()
            .filter(|(c, p)| netlist.components[*c].refdes == reference && p == pin)
            .count()
    };
    assert_eq!(on("U1", "41"), 1);
    assert_eq!(on("J1", "S1"), 1);
    // An unconnected pad is on no net.
    assert!(
        netlist
            .nets
            .iter()
            .all(|n| !n.name.starts_with("unconnected-"))
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
    assert!(readme.contains("https://github.com/Hanqaqa/Easyduino"));
    assert!(readme.contains("CERN-OHL-P-2.0"));
    assert!(readme.contains("53b14b66d64f25c55f88971c1c1b51656126bd30"));
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
    assert_eq!(names, ["EasyduinoEsp32s3"]);
}
