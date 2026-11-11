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

/// The board's interface parts: the USB Type-C receptacles (J1, J2), the buttons (SW1, SW2), the
/// headers (U2) and the LEDs (D1 to D4). The first four kinds' pads must equal the reference
/// board's (the parity rule): the footprints are the project's own, in
/// `libs/easyduino_must_match.pretty`, compared pad for pad with `reference/placements.toml`.
mod must_match_parts {
    use std::collections::{BTreeMap, BTreeSet};

    use netsmith::project::Target;
    use netsmith::{Libraries, build_with};
    use netsmith_kicad::install::Search;
    use netsmith_kicad::testlib;
    use netsmith_pcb::{Footprint, PadKind, PadShape};

    use super::{number, parsed, project, read, tables};

    /// A small board of our own, never the project's: one of each part, so that the parts, the
    /// catalogs and the `kicad.toml` rows build together. The receptacle's power contacts are on
    /// a rail, which makes the board's checks and the pinout validation run.
    const FIXTURE: &str = r#"
import src::devkitc_headers::Esp32S3DevKitCHeaders
import src::shouhan_type_c_16pin::ShouhanTypeC16Pin
import src::xunpu_ts_1088::XunpuTs1088
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

board InterfaceBare {
    .usb_power: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VBUS"
        .neg.net &= "GND"
    }
    .usb: ShouhanTypeC16Pin { .mpn &= "TYPE-C 16PIN 2MD(073)" }
    .button: XunpuTs1088 { .mpn &= "TS-1088-AR02016" }
    .headers: Esp32S3DevKitCHeaders
    .led: Led {
        .color &= "BLUE"
        .package &= "0402"
        .forward_voltage &= 2.6V..3.2V
        .current &= 3mA..10mA
    }
    .r: Resistor {}

    .usb_power.pos ~ .usb.vbus
    .usb_power.neg ~ .usb.gnd
    .usb.shield ~ .usb.gnd
    .button.a ~ .usb.cc1
    .button.b ~ .usb_power.neg
    .headers.v3v3 ~ .usb_power.pos
    .headers.gnd ~ .usb_power.neg
    .headers.gpio4 ~ .usb.dp
    .led.current == (.usb_power.voltage - .led.forward_voltage) / .r.resistance
    .usb_power.pos ~> .r ~> .led ~> .usb_power.neg
}
"#;

    /// A machine with no KiCad install, and the project's own `kicad.toml` and libraries.
    fn libraries() -> Libraries {
        let nowhere = testlib::TempDir::new("easyduino-no-kicad");
        Libraries::for_project_by(Some(&project()), &Search::under(nowhere.path()))
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn built() -> netsmith::Built {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        build_with(
            "interface.nsm",
            FIXTURE,
            &target.loader().unwrap(),
            &target.catalogs().unwrap(),
            &libraries(),
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .remove(0)
    }

    fn part_of<'a>(built: &'a netsmith::Built, kind: &str) -> &'a netsmith_core::Part {
        built
            .design
            .parts
            .iter()
            .find(|p| p.kind == kind)
            .unwrap_or_else(|| panic!("no {kind}"))
    }

    fn footprint(libraries: &Libraries, kind: &str, package: &str) -> Footprint {
        let mapped = libraries.mapping.map(kind, Some(package)).unwrap();
        libraries
            .footprints
            .load(&mapped.footprint)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// The three footprints the reference's must-match parts need: reference designators, kind,
    /// package.
    const PARTS: [(&str, &str, &str); 5] = [
        ("J1", "ShouhanTypeC16Pin", "SMD"),
        ("J2", "ShouhanTypeC16Pin", "SMD"),
        ("SW1", "XunpuTs1088", "SMD,4x3mm"),
        ("SW2", "XunpuTs1088", "SMD,4x3mm"),
        ("U2", "Esp32S3DevKitCHeaders", "2x22,P=2.54mm"),
    ];

    fn shape_word(pad: &netsmith_pcb::Pad) -> &'static str {
        if pad.extent.is_some() {
            return "custom";
        }
        match pad.shape {
            PadShape::Rect => "rect",
            PadShape::RoundRect => "roundrect",
            PadShape::Oval => "oval",
            PadShape::Circle => "circle",
        }
    }

    /// Every difference between the pads of `footprint` and those of the reference's `reference`
    /// footprint, with the parity rule's tolerance of 0.05 mm on centre, size and drill: number,
    /// centre in the footprint's own frame, size, drill, shape, kind and angle. Empty when they
    /// are the same.
    fn pad_differences(footprint: &Footprint, table: &toml::Table, reference: &str) -> Vec<String> {
        const TOLERANCE: f64 = 0.05;
        let part = *tables(table, "part")
            .iter()
            .find(|p| p["reference"].as_str() == Some(reference))
            .unwrap_or_else(|| panic!("no part {reference}"));
        let rotation = number(part, "rotation");
        let wanted = part["pads"].as_array().unwrap();
        let mut differences = Vec::new();
        if footprint.pads.len() != wanted.len() {
            differences.push(format!(
                "{reference}: {} pads, the reference has {}",
                footprint.pads.len(),
                wanted.len()
            ));
        }
        let as_f = |v: &toml::Value| v.as_float().or_else(|| v.as_integer().map(|i| i as f64));
        let mut used = vec![false; footprint.pads.len()];
        for pad in wanted {
            let pad = pad.as_table().unwrap();
            let (n, x, y) = (
                pad["number"].as_str().unwrap(),
                number(pad, "x"),
                number(pad, "y"),
            );
            let found = footprint.pads.iter().enumerate().find(|(i, own)| {
                !used[*i]
                    && own.number == n
                    && (own.at.x - x).abs() <= TOLERANCE
                    && (own.at.y - y).abs() <= TOLERANCE
            });
            let Some((i, own)) = found else {
                differences.push(format!(
                    "{reference} pad {n:?}: none at ({x}, {y}) within {TOLERANCE} mm; pads of that number are at {:?}",
                    footprint
                        .pads
                        .iter()
                        .filter(|p| p.number == n)
                        .map(|p| (p.at.x, p.at.y))
                        .collect::<Vec<_>>()
                ));
                continue;
            };
            used[i] = true;
            let at = format!("{reference} pad {n:?} at ({x}, {y})");
            let size = pad["size"].as_array().unwrap();
            let (w, h) = (as_f(&size[0]).unwrap(), as_f(&size[1]).unwrap());
            if (own.width - w).abs() > TOLERANCE || (own.height - h).abs() > TOLERANCE {
                differences.push(format!(
                    "{at}: size {} x {}, the reference's {w} x {h}",
                    own.width, own.height
                ));
            }
            let want_shape = pad["shape"].as_str().unwrap();
            if shape_word(own) != want_shape {
                differences.push(format!(
                    "{at}: shape {}, the reference's {want_shape}",
                    shape_word(own)
                ));
            }
            let want_kind = match pad["type"].as_str().unwrap() {
                "smd" => PadKind::Smd,
                "thru_hole" => PadKind::ThruHole,
                "np_thru_hole" => PadKind::NpThruHole,
                other => panic!("{at}: type {other}"),
            };
            if own.kind != want_kind {
                differences.push(format!(
                    "{at}: kind {:?}, the reference's {want_kind:?}",
                    own.kind
                ));
            }
            // The file's pad angle includes the footprint's rotation; a turn of 180 degrees
            // leaves a rectangle, a circle or an oval as it was.
            let turn =
                pad.get("angle").map_or(0.0, |_| number(pad, "angle")) - rotation - own.angle;
            let off = turn.rem_euclid(180.0).min(180.0 - turn.rem_euclid(180.0));
            if off > 1e-6 {
                differences.push(format!(
                    "{at}: angle {} differs from the reference's",
                    own.angle
                ));
            }
            let want_drill = if let Some(slot) = pad.get("drill_slot") {
                let slot = slot.as_array().unwrap();
                Some((as_f(&slot[0]).unwrap(), as_f(&slot[1]).unwrap()))
            } else if pad.contains_key("drill") {
                Some((number(pad, "drill"), number(pad, "drill")))
            } else {
                None
            };
            match (own.drill, want_drill) {
                (None, None) => {}
                (Some(d), Some((a, b)))
                    if (d.width - a).abs() <= TOLERANCE && (d.height - b).abs() <= TOLERANCE => {}
                (d, w) => differences.push(format!(
                    "{at}: drill {:?}, the reference's {w:?}",
                    d.map(|d| (d.width, d.height))
                )),
            }
        }
        differences
    }

    #[test]
    fn every_part_checks_and_the_fixture_builds_with_no_kicad() {
        // The build ran `validate_pinout` on each part (a pad of the pin table that the
        // footprint lacks, or a footprint pad that no pin names, stops it) and wrote the
        // schematic with symbols drawn from the pin tables.
        let built = built();
        assert!(!built.schematic.is_empty());
        let mapping = netsmith_kicad::Mapping::for_project(Some(&project())).unwrap();
        for (kind, package, footprint) in [
            (
                "ShouhanTypeC16Pin",
                "SMD",
                "easyduino_must_match:USB_C_Receptacle_Shouhan_TYPE-C-16PIN-2MD-073",
            ),
            (
                "XunpuTs1088",
                "SMD,4x3mm",
                "easyduino_must_match:SW_XUNPU_TS-1088-AR02016",
            ),
            (
                "Esp32S3DevKitCHeaders",
                "2x22,P=2.54mm",
                "easyduino_must_match:PinHeader_2x22_P2.54mm_Espressif_ESP32-S3-DevKitC-1",
            ),
        ] {
            let mapped = mapping.map(kind, Some(package)).unwrap();
            assert_eq!(mapped.footprint, footprint, "{kind}");
            assert_eq!(mapped.lib_id, None, "{kind}: drawn from the pin table");
            let file = footprint.trim_start_matches("easyduino_must_match:");
            assert!(
                project()
                    .join("libs/easyduino_must_match.pretty")
                    .join(format!("{file}.kicad_mod"))
                    .is_file(),
                "{file}"
            );
        }
        // The LED is the standard one, in the 0402 package of the standard rows.
        let led = part_of(&built, "Led");
        assert_eq!(led.package.as_deref(), Some("0402"));
        assert!(led.refdes.starts_with('D'));
    }

    #[test]
    fn the_receptacle_and_the_headers_are_connectors_with_the_pins_of_their_pads() {
        let built = built();
        for (kind, refdes, pins, pads) in [
            ("ShouhanTypeC16Pin", 'J', 9, 17),
            ("Esp32S3DevKitCHeaders", 'J', 40, 44),
        ] {
            let part = part_of(&built, kind);
            assert!(part.is_connector(), "{kind} provides Connector");
            assert!(part.refdes.starts_with(refdes), "{kind}: {}", part.refdes);
            assert_eq!(part.pins.len(), pins, "{kind}: pins");
            let numbers: Vec<&String> = part.pins.iter().flat_map(|p| &p.pads).collect();
            assert_eq!(
                numbers.iter().collect::<BTreeSet<_>>().len(),
                numbers.len(),
                "{kind}: a pad is on one pin only"
            );
            assert_eq!(numbers.len(), pads, "{kind}: pads");
        }
        let usb = part_of(&built, "ShouhanTypeC16Pin");
        let pads = |name: &str| -> Vec<&str> {
            usb.pins
                .iter()
                .find(|p| p.datasheet == name)
                .unwrap_or_else(|| panic!("no pin {name}"))
                .pads
                .iter()
                .map(String::as_str)
                .collect()
        };
        // The contacts the receptacle joins inside are one pin; the shell is one pin whose pad
        // number the footprint repeats four times.
        assert_eq!(pads("VBUS"), ["A4", "A9", "B4", "B9"]);
        assert_eq!(pads("GND"), ["A1", "A12", "B1", "B12"]);
        assert_eq!(pads("D+"), ["A6", "B6"]);
        assert_eq!(pads("D-"), ["A7", "B7"]);
        assert_eq!(pads("CC1"), ["A5"]);
        assert_eq!(pads("CC2"), ["B5"]);
        assert_eq!(pads("SBU1"), ["A8"]);
        assert_eq!(pads("SBU2"), ["B8"]);
        assert_eq!(pads("SHIELD"), ["S1"]);
        let headers = part_of(&built, "Esp32S3DevKitCHeaders");
        let on = |name: &str| -> Vec<&str> {
            headers
                .pins
                .iter()
                .find(|p| p.datasheet == name)
                .unwrap()
                .pads
                .iter()
                .map(String::as_str)
                .collect()
        };
        assert_eq!(on("3V3"), ["1", "2"]);
        assert_eq!(on("GND"), ["22", "23", "24", "44"]);
        assert_eq!(on("5V"), ["21"]);
        assert_eq!(on("GPIO0"), ["31"]);
    }

    #[test]
    fn the_buttons_have_two_pins_on_pads_1_and_2_and_a_part_number_to_order_by() {
        let built = built();
        let button = part_of(&built, "XunpuTs1088");
        assert!(button.refdes.starts_with("SW"), "{}", button.refdes);
        let mut pads: Vec<_> = button.pins.iter().map(|p| p.pads.clone()).collect();
        pads.sort();
        assert_eq!(pads, [["1"], ["2"]]);
        let sourced = built
            .sourcing
            .iter()
            .find(|s| s.kind == "XunpuTs1088")
            .unwrap();
        assert_eq!(sourced.lcsc.as_deref(), Some("C720477"));
        let receptacle = built
            .sourcing
            .iter()
            .find(|s| s.kind == "ShouhanTypeC16Pin")
            .unwrap();
        assert_eq!(receptacle.lcsc.as_deref(), Some("C2765186"));
    }

    #[test]
    fn the_headers_are_soldered_by_hand_and_have_no_lcsc_number() {
        let built = built();
        let headers = built
            .sourcing
            .iter()
            .find(|s| s.kind == "Esp32S3DevKitCHeaders")
            .unwrap();
        assert_eq!(headers.lcsc, None);
        assert_eq!(headers.assembly, netsmith_design::Assembly::Hand);
    }

    #[test]
    fn the_led_is_the_blue_0402_of_the_boards_bill_of_materials() {
        let built = built();
        let led = built
            .sourcing
            .iter()
            .find(|s| s.kind == "Led")
            .expect("the LED is in the bill of materials");
        assert_eq!(led.lcsc.as_deref(), Some("C434447"), "{led:?}");
    }

    /// The reference's must-match parts, with the number of pads each has.
    #[test]
    fn the_pads_of_each_must_match_footprint_are_those_of_the_reference_board() {
        let libraries = libraries();
        let table = parsed(&read("reference/placements.toml"));
        let mut differences = Vec::new();
        for (reference, kind, package) in PARTS {
            let footprint = footprint(&libraries, kind, package);
            differences.extend(pad_differences(&footprint, &table, reference));
        }
        assert!(
            differences.is_empty(),
            "the footprints differ from the reference's pads:\n{}",
            differences.join("\n")
        );
        // The numbers: the receptacle 16 contacts, the shell and the two unnumbered pegs, the
        // button 2, the headers 1 to 44.
        let usb = footprint(&libraries, "ShouhanTypeC16Pin", "SMD");
        assert_eq!(usb.pads.len(), 22);
        assert_eq!(usb.pad_numbers().len(), 18, "16 contacts, S1 and no number");
        let switch = footprint(&libraries, "XunpuTs1088", "SMD,4x3mm");
        assert_eq!(switch.pads.len(), 2);
        let headers = footprint(&libraries, "Esp32S3DevKitCHeaders", "2x22,P=2.54mm");
        assert_eq!(headers.pads.len(), 44);
        let expected: BTreeSet<String> = (1..=44).map(|n| n.to_string()).collect();
        assert_eq!(
            headers
                .pad_numbers()
                .into_iter()
                .map(String::from)
                .collect::<BTreeSet<_>>(),
            expected
        );
    }

    #[test]
    fn the_comparison_of_pads_says_what_differs() {
        // Not vacuous: a pad moved by 0.1 mm, one of the wrong size and a slot of another
        // length are each named.
        let libraries = libraries();
        let table = parsed(&read("reference/placements.toml"));
        let mut usb = footprint(&libraries, "ShouhanTypeC16Pin", "SMD");
        usb.pads[0].at.x += 0.1;
        usb.pads[1].width = 0.8;
        let slot = usb.pads.iter_mut().find(|p| p.number == "S1").unwrap();
        slot.drill = Some(netsmith_pcb::Drill {
            width: 0.6,
            height: 1.0,
        });
        let differences = pad_differences(&usb, &table, "J1");
        assert_eq!(differences.len(), 3, "{differences:?}");
        assert!(differences[0].contains("none at"), "{differences:?}");
        assert!(differences[1].contains("size 0.8"), "{differences:?}");
        assert!(differences[2].contains("drill"), "{differences:?}");
        // A header pad turned a quarter is a different pad.
        let mut headers = footprint(&libraries, "Esp32S3DevKitCHeaders", "2x22,P=2.54mm");
        headers.pads[0].angle = 0.0;
        let differences = pad_differences(&headers, &table, "U2");
        assert_eq!(differences.len(), 1, "{differences:?}");
        assert!(differences[0].contains("angle"), "{differences:?}");
    }

    /// What the reference's net on a pad is called by the part's pin, for each must-match part.
    fn pin_for_net(reference: &str, net: &str) -> String {
        let net = net.trim_start_matches('/');
        match (reference, net) {
            (_, "GND") => "GND".into(),
            ("U2", "+3.3V") => "3V3".into(),
            ("U2", "VBUS") => "5V".into(),
            ("U2", "USB2_D-") => "GPIO19".into(),
            ("U2", "USB2_D+") => "GPIO20".into(),
            ("U2", n) => n.into(),
            (_, n) if n.starts_with("VBUS") => "VBUS".into(),
            (_, n) if n.starts_with("CC1_") => "CC1".into(),
            (_, n) if n.starts_with("CC2_") => "CC2".into(),
            (_, n) if n.ends_with("_D+") => "D+".into(),
            (_, n) if n.ends_with("_D-") => "D-".into(),
            (_, n) if n.starts_with("unconnected-(") && n.contains("-SBU1-") => "SBU1".into(),
            (_, n) if n.starts_with("unconnected-(") && n.contains("-SBU2-") => "SBU2".into(),
            (r, n) => panic!("{r}: net {n}"),
        }
    }

    #[test]
    fn each_pad_of_the_reference_is_on_the_pin_that_carries_its_net() {
        // The reference's nets, read from its table, against the pin each pad belongs to: the
        // receptacle's contacts, the shell's `S1` on ground and the header pins that the
        // Espressif guide names (the net is the pin's name).
        let built = built();
        let table = parsed(&read("reference/placements.toml"));
        for (reference, kind) in [
            ("J1", "ShouhanTypeC16Pin"),
            ("J2", "ShouhanTypeC16Pin"),
            ("U2", "Esp32S3DevKitCHeaders"),
        ] {
            let part = part_of(&built, kind);
            let pin_of_pad: BTreeMap<&str, &str> = part
                .pins
                .iter()
                .flat_map(|p| p.pads.iter().map(|n| (n.as_str(), p.datasheet.as_str())))
                .collect();
            let pads = tables(&table, "part")
                .into_iter()
                .find(|p| p["reference"].as_str() == Some(reference))
                .unwrap()["pads"]
                .as_array()
                .unwrap()
                .clone();
            let mut checked = 0;
            for pad in pads {
                let pad = pad.as_table().unwrap();
                let (n, net) = (pad["number"].as_str().unwrap(), pad.get("net"));
                let Some(net) = net.and_then(|n| n.as_str()) else {
                    assert_eq!(n, "", "{reference}: only the pegs have no net");
                    continue;
                };
                let want = if n == "S1" {
                    "SHIELD".to_string()
                } else {
                    pin_for_net(reference, net)
                };
                assert_eq!(
                    pin_of_pad.get(n).copied(),
                    Some(want.as_str()),
                    "{reference} pad {n} is on {net}"
                );
                checked += 1;
            }
            assert_eq!(
                checked,
                if reference == "U2" { 44 } else { 20 },
                "{reference}"
            );
        }
        // The buttons: pad 1 on ground and pad 2 on the signal, as the reference has them.
        for (reference, signal) in [("SW1", "/EN"), ("SW2", "/GPIO0")] {
            let pads = tables(&table, "part")
                .into_iter()
                .find(|p| p["reference"].as_str() == Some(reference))
                .unwrap()["pads"]
                .as_array()
                .unwrap()
                .clone();
            let net = |n: &str| {
                pads.iter()
                    .map(|p| p.as_table().unwrap())
                    .find(|p| p["number"].as_str() == Some(n))
                    .unwrap()["net"]
                    .as_str()
                    .unwrap()
                    .to_string()
            };
            assert_eq!((net("1").as_str(), net("2").as_str()), ("GND", signal));
        }
    }

    #[test]
    fn the_header_footprint_does_not_draw_the_boards_outline() {
        // The reference draws the outline in U2's footprint; the physical file draws ours.
        for entry in std::fs::read_dir(project().join("libs/easyduino_must_match.pretty")).unwrap()
        {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(!text.contains("Edge.Cuts"), "{}", path.display());
        }
    }

    #[test]
    fn the_catalog_rows_name_the_parts_and_where_they_were_read() {
        for (file, lcsc, mpn, page, library) in [
            (
                "catalogs/shouhan_type_c_16pin.csv",
                "C2765186",
                "\"TYPE-C 16PIN 2MD(073)\"",
                "https://jlcpcb.com/partdetail/C2765186",
                Some("Extended"),
            ),
            (
                "catalogs/xunpu_ts_1088.csv",
                "C720477",
                "TS-1088-AR02016",
                "https://jlcpcb.com/partdetail/C720477",
                None,
            ),
            (
                "catalogs/led.csv",
                "C434447",
                "SZYY0402B",
                "https://jlcpcb.com/partdetail/C434447",
                Some("Extended"),
            ),
        ] {
            let rows = read(file);
            let header = rows.lines().next().unwrap();
            assert_eq!(rows.lines().count(), 2, "{file}: one row");
            let row = rows.lines().nth(1).unwrap();
            assert!(row.starts_with(&format!("{lcsc},")), "{file}: {row}");
            assert!(
                row.contains(mpn.trim_matches('"')),
                "{file}: the part number"
            );
            assert!(row.contains(page), "{file}: the page it was read from");
            assert!(
                row.contains(&format!("https://www.lcsc.com/product-detail/{lcsc}.html")),
                "{file}: LCSC's page"
            );
            assert!(row.contains("2026-11-10"), "{file}: the day it was read");
            assert!(
                row.contains("reference/Easyduino_ESP32S3_BOM.csv"),
                "{file}: the reference's own number"
            );
            if let Some(library) = library {
                assert!(
                    row.contains(&format!(",{library},")),
                    "{file}: library type"
                );
            } else {
                assert!(
                    row.contains("does not say Basic or Extended"),
                    "{file}: no library type was read"
                );
            }
            // Price and stock are not recorded.
            let columns: Vec<&str> = header.split(',').collect();
            if let (Some(p), Some(s)) = (
                columns.iter().position(|c| *c == "price"),
                columns.iter().position(|c| *c == "stock"),
            ) {
                let cells = csv_cells(row);
                assert_eq!(cells[p], "", "{file}: price");
                assert_eq!(cells[s], "", "{file}: stock");
            }
        }
        let target = Target::resolve_from(&project(), None).unwrap();
        assert!(target.catalogs().is_ok());
    }

    /// The cells of one line of a CSV file (quotes around a cell with commas).
    fn csv_cells(line: &str) -> Vec<String> {
        let (mut cells, mut cell, mut quoted) = (Vec::new(), String::new(), false);
        for c in line.chars() {
            match (c, quoted) {
                ('"', _) => quoted = !quoted,
                (',', false) => cells.push(std::mem::take(&mut cell)),
                (c, _) => cell.push(c),
            }
        }
        cells.push(cell);
        cells
    }
}
