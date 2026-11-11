//! `examples/easyduino_esp32s3/`: the home of the port of the Easyduino ESP32-S3 board (the
//! second showcase board of epic #308), its licence and its reference table
//! (`reference/placements.toml`), read from the author's board file by
//! `cargo run -p netsmith --example reference_table` (`netsmith::reference_table`).
//!
//! This is the first board whose file draws its outline inside a footprint, has four copper
//! layers and has parts that carry several pads of one number: the first tests run the reading
//! on small boards of our own that have each of those, and the rest prove what the committed
//! table and the board hold.

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

    .usb_power ~ .usb.vbus
    .usb.shield ~ .usb.vbus.neg
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

/// The module, the USB-UART bridge, the regulator, the auto-reset transistors and the Schottky
/// diodes (U1, U3, U5, Q1, Q2, D2, D3): the parts of `src/esp32_s3_wroom_1.nsm`,
/// `src/cp2102n.nsm`, `src/ss8050.nsm`, the project's catalogs and the rows of `kicad.toml`.
/// None is a must-match part of the parity checker: they build, are sourced and are close to the
/// reference's pads, and these tests say how close.
mod ics {
    use super::{number, parsed, project, read, tables};
    use std::collections::{BTreeMap, BTreeSet};

    use netsmith::project::Target;
    use netsmith::{Libraries, build_with};
    use netsmith_core::PinKind;
    use netsmith_kicad::install::Search;
    use netsmith_kicad::{Mapping, testlib};
    use netsmith_pcb::Footprint;

    /// A small board of our own, never built by the project: every part of this module with the
    /// pins joined to headers, a supply and a rail. The nets and the names are the datasheets',
    /// nothing says they are Easyduino's.
    const FIXTURE: &str = r#"
import src::cp2102n::Cp2102n
import src::esp32_s3_wroom_1::Esp32S3Wroom1
import src::ss8050::Ss8050
import std::parts::connector::{Header1x2, Header1x4, Header1x8}
import std::parts::diode::Schottky
import std::parts::regulator::LinearRegulator
import std::sources::Rail

board IcsFixture {
    .usb: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VBUS"
        .neg.net &= "GND"
    }
    .rail: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..100mA
    }
    .ldo: LinearRegulator {
        .package &= "SOT-223"
        .vout.voltage &= 3.2V..3.4V
        .quiescent_current &= 0mA..11mA
    }
    .usb ~ .ldo.vin
    .ldo.vout ~ .rail

    .u1: Esp32S3Wroom1 { .mpn &= "ESP32-S3-WROOM-1-N4" }
    .u3: Cp2102n { .mpn &= "CP2102N-A02-GQFN28" }
    .q1: Ss8050 { .mpn &= "SS8050" }
    .q2: Ss8050 { .mpn &= "SS8050" }
    .d2: Schottky { .package &= "SMD1006" }

    .rail ~ .u1.vdd
    .usb.pos ~ .u3.vbus
    .usb.pos ~ .u3.vregin
    .rail.pos ~ .u3.vdd
    .rail.neg ~ .u3.gnd

    .module: Header1x8
    .u1.en ~ .module.p1
    .u1.io0 ~ .module.p2
    .u1.rxd0 ~ .u3.txd
    .u1.txd0 ~ .u3.rxd
    .u1.io19 ~ .module.p3
    .u1.io20 ~ .module.p4

    .bridge: Header1x8
    .u3.dp ~ .bridge.p1
    .u3.dm ~ .bridge.p2
    .u3.rts ~ .q1.collector
    .u3.dtr ~ .q2.collector
    .u3.rstb ~ .bridge.p3
    .u3.dcd ~ .bridge.p4
    .u3.cts ~ .bridge.p5
    .u3.dsr ~ .bridge.p6

    .reset: Header1x4
    .q1.base ~ .reset.p1
    .q1.emitter ~ .reset.p2
    .q2.base ~ .reset.p3
    .q2.emitter ~ .reset.p4

    .diode: Header1x2
    .d2.cathode ~ .usb.pos
    .d2.anode ~ .diode.p1
    .usb.neg ~ .diode.p2
}
"#;

    /// A machine with no KiCad install, and the project's own `kicad.toml` and libraries.
    fn libraries() -> Libraries {
        let nowhere = testlib::TempDir::new("easyduino-ics-no-kicad");
        Libraries::for_project_by(Some(&project()), &Search::under(nowhere.path()))
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn built() -> netsmith::Built {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        build_with(
            "ics.nsm",
            FIXTURE,
            &target.loader().unwrap(),
            &target.catalogs().unwrap(),
            &libraries(),
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .remove(0)
    }

    /// A part's pins as the design has them: the datasheet's name, the kind and the pads, in
    /// order.
    fn pins_of(built: &netsmith::Built, kind: &str) -> Vec<(String, PinKind, Vec<String>)> {
        part_of(built, kind)
            .pins
            .iter()
            .map(|p| (p.datasheet.clone(), p.kind, p.pads.clone()))
            .collect()
    }

    fn part_of<'a>(built: &'a netsmith::Built, kind: &str) -> &'a netsmith_core::Part {
        built
            .design
            .parts
            .iter()
            .find(|p| p.kind == kind)
            .unwrap_or_else(|| panic!("the board has no {kind}"))
    }

    /// The footprint a part is exported with, loaded as the build loads it.
    fn footprint_of(built: &netsmith::Built, kind: &str) -> Footprint {
        let part = part_of(built, kind);
        let libraries = libraries();
        let mapped = libraries
            .mapping
            .map(&part.kind, part.package.as_deref())
            .unwrap_or_else(|e| panic!("{e}"));
        libraries
            .footprints
            .load(&mapped.footprint)
            .unwrap_or_else(|e| panic!("{}: {e}", mapped.footprint))
    }

    fn pad_numbers(pins: &[(String, PinKind, Vec<String>)]) -> BTreeSet<String> {
        pins.iter().flat_map(|p| p.2.iter().cloned()).collect()
    }

    fn numbered(footprint: &Footprint) -> BTreeSet<String> {
        footprint
            .pad_numbers()
            .into_iter()
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn reference_part(reference: &str) -> toml::Table {
        let table = parsed(&read("reference/placements.toml"));
        tables(&table, "part")
            .into_iter()
            .find(|p| p["reference"].as_str() == Some(reference))
            .unwrap_or_else(|| panic!("the reference has no {reference}"))
            .clone()
    }

    /// The reference's pads of one footprint as (number, x, y, width, height, net).
    #[allow(clippy::type_complexity)]
    fn reference_pads(reference: &str) -> Vec<(String, f64, f64, f64, f64, Option<String>)> {
        let part = reference_part(reference);
        let rotation = number(&part, "rotation");
        part["pads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let p = p.as_table().unwrap();
                let size = p["size"].as_array().unwrap();
                let f = |v: &toml::Value| v.as_float().or_else(|| v.as_integer().map(|i| i as f64));
                let (mut w, mut h) = (f(&size[0]).unwrap(), f(&size[1]).unwrap());
                // The file's pad angle includes the footprint's rotation: a pad that is a
                // quarter turn from the footprint's own frame has its size the other way round.
                let turn = p.get("angle").map_or(0.0, |_| number(p, "angle")) - rotation;
                if (turn.rem_euclid(180.0) - 90.0).abs() < 1e-6 {
                    std::mem::swap(&mut w, &mut h);
                }
                (
                    p["number"].as_str().unwrap().to_string(),
                    number(p, "x"),
                    number(p, "y"),
                    w,
                    h,
                    p.get("net").map(|n| n.as_str().unwrap().to_string()),
                )
            })
            .collect()
    }

    /// How far the numbered pads of `footprint` are from the reference's: the largest distance
    /// between the centres of the pads of one number, and the largest difference of a side.
    /// Every numbered pad of the reference must have a pad of the same number (each used once),
    /// and the other way round.
    fn closeness(footprint: &Footprint, reference: &str) -> (f64, f64) {
        let wanted: Vec<_> = reference_pads(reference)
            .into_iter()
            .filter(|p| !p.0.is_empty())
            .collect();
        let own: Vec<_> = footprint
            .pads
            .iter()
            .filter(|p| !p.number.is_empty())
            .collect();
        assert_eq!(
            own.len(),
            wanted.len(),
            "{reference}: the number of numbered pads"
        );
        let mut used = vec![false; own.len()];
        let (mut far, mut size) = (0.0f64, 0.0f64);
        for (n, x, y, w, h, _) in &wanted {
            // The nearest unused pad of the number.
            let (i, d) = own
                .iter()
                .enumerate()
                .filter(|(i, p)| !used[*i] && &p.number == n)
                .map(|(i, p)| (i, ((p.at.x - x).powi(2) + (p.at.y - y).powi(2)).sqrt()))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap_or_else(|| panic!("{reference}: no pad {n} left"));
            used[i] = true;
            far = far.max(d);
            size = size
                .max((own[i].width - w).abs())
                .max((own[i].height - h).abs());
        }
        (far, size)
    }

    #[test]
    fn the_fixture_builds_with_no_kicad_and_every_part_is_sourced() {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        assert!(target.catalogs().is_ok(), "the project's catalogs load");
        let built = built();
        // The build ran `validate_pinout` on each part (a pad of the pin table that the
        // footprint lacks, or a footprint pad that no pin names, stops it).
        assert!(!built.schematic.is_empty());
        let by_kind = |kind: &str| {
            built
                .sourcing
                .iter()
                .find(|s| s.kind == kind)
                .unwrap_or_else(|| panic!("no sourcing row of {kind}"))
        };
        for (kind, lcsc, mpn, package, library) in [
            (
                "Esp32S3Wroom1",
                "C2913197",
                "ESP32-S3-WROOM-1-N4",
                "SMD, 25.5x18mm",
                "Extended",
            ),
            (
                "Cp2102n",
                "C1550553",
                "CP2102N-A02-GQFN28",
                "QFN-28-EP(5x5)",
                "Extended",
            ),
            ("Ss8050", "C916392", "SS8050", "SOT-23", "Extended"),
            (
                "LinearRegulator",
                "C6186",
                "AMS1117-3.3",
                "SOT-223",
                "Basic",
            ),
            (
                "Schottky",
                "C2837790",
                "RB161QS-40T18R",
                "SMD1006",
                "Extended",
            ),
        ] {
            let row = by_kind(kind);
            assert_eq!(row.lcsc.as_deref(), Some(lcsc), "{kind}");
            assert_eq!(row.mpn.as_deref(), Some(mpn), "{kind}");
            assert_eq!(row.package.as_deref(), Some(package), "{kind}");
            assert_eq!(format!("{:?}", row.library.unwrap()), library, "{kind}");
        }
    }

    #[test]
    fn the_symbols_are_drawn_from_the_pin_tables() {
        let built = built();
        for (kind, package) in [
            ("Esp32S3Wroom1", "SMD, 25.5x18mm"),
            ("Cp2102n", "QFN-28-EP(5x5)"),
            ("Ss8050", "SOT-23"),
        ] {
            let mapped = libraries().mapping.map(kind, Some(package)).unwrap();
            assert_eq!(mapped.lib_id, None, "{kind}: no KiCad symbol is named");
            let name = format!(
                "netsmith:{}",
                netsmith_kicad::generate::symbol_name(kind, Some(package))
            );
            assert!(
                built.schematic.contains(&format!("(lib_id \"{name}")),
                "no instance of the symbol `{name}`"
            );
            for (datasheet, _, pads) in pins_of(&built, kind) {
                assert!(
                    built.schematic.contains(&format!("(name \"{datasheet}\"")),
                    "no pin {datasheet}"
                );
                for pad in pads {
                    assert!(
                        built.schematic.contains(&format!("(number \"{pad}\"")),
                        "{kind}: no pad {pad}"
                    );
                }
            }
        }
    }

    // ---- U1, the module ----

    #[test]
    fn the_module_has_the_forty_edge_pads_and_the_ground_pad_of_the_datasheet() {
        let built = built();
        let pins = pins_of(&built, "Esp32S3Wroom1");
        let want: BTreeSet<String> = (1..=41).map(|n| n.to_string()).collect();
        assert_eq!(pad_numbers(&pins), want, "pads 1 to 41, each in one pin");
        let total: usize = pins.iter().map(|p| p.2.len()).sum();
        assert_eq!(total, 41, "no pad is on two pins");
        let pin = |name: &str| {
            pins.iter()
                .find(|p| p.0 == name)
                .unwrap_or_else(|| panic!("{name}"))
        };
        // Table 3-1: GND on pads 1 and 40 and the EPAD 41, 3V3 on 2, EN on 3 (an input).
        assert_eq!(pin("GND").2, ["1", "40", "41"]);
        assert_eq!(pin("GND").1, PinKind::Power);
        assert_eq!(
            (pin("3V3").1, pin("3V3").2.clone()),
            (PinKind::Power, vec!["2".to_string()])
        );
        assert_eq!(
            (pin("EN").1, pin("EN").2.clone()),
            (PinKind::Input, vec!["3".to_string()])
        );
        // The 36 signal pads are the table's "I/O/T" and are bidirectional here.
        let signals: Vec<_> = pins
            .iter()
            .filter(|p| !["GND", "3V3", "EN"].contains(&p.0.as_str()))
            .collect();
        assert_eq!(signals.len(), 36);
        assert!(signals.iter().all(|p| p.1 == PinKind::Bidirectional));
        // Pad numbers and names at the corners and along the sides of the table.
        for (name, pad) in [
            ("IO4", "4"),
            ("IO8", "12"),
            ("IO19", "13"),
            ("IO20", "14"),
            ("IO3", "15"),
            ("IO14", "22"),
            ("IO45", "26"),
            ("IO0", "27"),
            ("IO35", "28"),
            ("IO42", "35"),
            ("RXD0", "36"),
            ("TXD0", "37"),
            ("IO2", "38"),
            ("IO1", "39"),
        ] {
            assert_eq!(pin(name).2, [pad], "{name}");
        }
    }

    #[test]
    fn the_module_footprint_is_the_land_pattern_of_the_datasheet_and_of_the_reference() {
        let built = built();
        let footprint = footprint_of(&built, "Esp32S3Wroom1");
        assert_eq!(footprint.name, "ESP32-S3-WROOM-1_18x25.5mm_P1.27mm");
        assert_eq!(
            footprint.pads.len(),
            49,
            "40 edge pads and the nine squares of pad 41"
        );
        assert_eq!(footprint.pads_numbered("41").count(), 9);
        assert_eq!(
            numbered(&footprint),
            pad_numbers(&pins_of(&built, "Esp32S3Wroom1"))
        );
        // Datasheet v1.8, Figure 11-1 (page 45): 1.5 x 0.9 mm edge pads on a 1.27 mm pitch,
        // 17.5 mm between the two columns' centres, 13.97 mm along the bottom row (11 pitches),
        // nine 0.9 mm squares on a 1.4 mm pitch.
        let pad = |n: &str| footprint.pads.iter().find(|p| p.number == n).unwrap();
        for n in 1..=40 {
            let p = pad(&n.to_string());
            let (long, short) = if (15..=26).contains(&n) {
                (p.height, p.width)
            } else {
                (p.width, p.height)
            };
            assert_eq!((long, short), (1.5, 0.9), "pad {n}");
        }
        assert!((pad("40").at.x - pad("1").at.x - 17.5).abs() < 1e-9);
        assert!(
            (pad("14").at.y - pad("1").at.y - 16.51).abs() < 1e-9,
            "13 x 1.27"
        );
        assert!(
            (pad("26").at.x - pad("15").at.x - 13.97).abs() < 1e-9,
            "11 x 1.27"
        );
        assert!(
            (pad("27").at.y - pad("14").at.y).abs() < 1e-9,
            "the right column starts level with the left's end"
        );
        let squares: Vec<_> = footprint.pads_numbered("41").collect();
        assert!(squares.iter().all(|p| (p.width, p.height) == (0.9, 0.9)));
        let xs: BTreeSet<i64> = squares
            .iter()
            .map(|p| (p.at.x * 1000.0).round() as i64)
            .collect();
        let ys: BTreeSet<i64> = squares
            .iter()
            .map(|p| (p.at.y * 1000.0).round() as i64)
            .collect();
        assert_eq!((xs.len(), ys.len()), (3, 3));
        // The reference's pads: the same numbers, centres and sizes to the 0.01 mm of its file.
        let (far, size) = closeness(&footprint, "U1");
        assert!(
            far <= 0.01,
            "the farthest pad centre is {far} mm from the reference's"
        );
        assert!(size <= 0.01, "the largest size difference is {size} mm");
    }

    /// The pin that the reference's net of a module pad names: a net `/GPIOn` is the pad `IOn`,
    /// except GPIO43 and GPIO44, which are the pads `TXD0` and `RXD0` (Table 3-1), and the
    /// module's own USB pair is GPIO19 and GPIO20 (`/USB2_D-`, `/USB2_D+`).
    fn module_pin_of(net: &str) -> Option<String> {
        let net = net.trim_start_matches('/');
        Some(match net {
            "GND" => "GND".into(),
            "+3.3V" => "3V3".into(),
            "EN" => "EN".into(),
            "GPIO44" => "RXD0".into(),
            "GPIO43" => "TXD0".into(),
            "USB2_D-" => "IO19".into(),
            "USB2_D+" => "IO20".into(),
            other => format!("IO{}", other.strip_prefix("GPIO")?),
        })
    }

    #[test]
    fn the_module_pads_agree_with_the_nets_of_the_reference_u1() {
        let built = built();
        let pins = pins_of(&built, "Esp32S3Wroom1");
        let mut checked = BTreeSet::new();
        for (number, _, _, _, _, net) in reference_pads("U1") {
            let net = net.expect("every pad of U1 is on a net");
            let name = module_pin_of(&net).unwrap_or_else(|| panic!("net {net} of pad {number}"));
            let pin = pins
                .iter()
                .find(|p| p.0 == name)
                .unwrap_or_else(|| panic!("no pin {name}"));
            assert!(
                pin.2.contains(&number),
                "pad {number} is on {net}, which is the pin {name}"
            );
            checked.insert(number);
        }
        assert_eq!(
            checked.len(),
            41,
            "every pad number of the table was compared"
        );
    }

    // ---- U3, the bridge ----

    #[test]
    fn the_bridge_has_the_twenty_eight_pins_and_the_centre_pad_of_the_datasheet() {
        let built = built();
        let pins = pins_of(&built, "Cp2102n");
        let want: BTreeSet<String> = (1..=29).map(|n| n.to_string()).collect();
        assert_eq!(pad_numbers(&pins), want);
        let kind_of = |pad: &str| {
            pins.iter()
                .find(|p| p.2.iter().any(|x| x == pad))
                .unwrap()
                .1
        };
        // Table 5.1: the supplies and the grounds are power, the modem inputs are inputs, the
        // outputs outputs, pad 10 is a no-connect.
        for pad in ["3", "29", "6", "7"] {
            assert_eq!(kind_of(pad), PinKind::Power, "pad {pad}");
        }
        for pad in ["1", "8", "23", "25", "27"] {
            assert_eq!(kind_of(pad), PinKind::Input, "pad {pad}");
        }
        for pad in ["11", "12", "13", "14", "15", "24", "26", "28"] {
            assert_eq!(kind_of(pad), PinKind::Output, "pad {pad}");
        }
        for pad in ["2", "4", "5", "9", "16", "17", "18", "19", "20", "21", "22"] {
            assert_eq!(kind_of(pad), PinKind::Bidirectional, "pad {pad}");
        }
        assert_eq!(kind_of("10"), PinKind::NoConnect);
        let ground = pins.iter().find(|p| p.0 == "GND").unwrap();
        assert_eq!(ground.2, ["3", "29"], "pad 3 and the centre pad");
    }

    /// A pin name as it is compared with the reference's net names: the datasheet's overbar
    /// (`~{...}` in KiCad) and the `b` that this table writes for it removed, and the two
    /// functions of a shared pad in any order.
    fn name_parts(name: &str) -> BTreeSet<String> {
        name.replace("{slash}", "/")
            .replace(['~', '{', '}'], "")
            .split('/')
            .map(|p| match p.strip_suffix('b') {
                // The table's `b` for an overbar: `RSTb`, `SUSPENDb`.
                Some(stem) if !stem.is_empty() && stem.chars().all(|c| c.is_ascii_uppercase()) => {
                    stem.to_string()
                }
                _ => p.to_string(),
            })
            .collect()
    }

    #[test]
    fn the_bridge_agrees_with_the_nets_of_the_reference_u3() {
        let built = built();
        let pins = pins_of(&built, "Cp2102n");
        let pin_of_pad = |pad: &str| pins.iter().find(|p| p.2.iter().any(|x| x == pad)).unwrap();
        let mut named = 0;
        let mut open = 0;
        for (number, _, _, _, _, net) in reference_pads("U3") {
            if number.is_empty() {
                continue; // the nine paste-only squares of the exposed pad
            }
            let net = net.expect("a pad of U3 is on a net");
            let pin = pin_of_pad(&number);
            if let Some(rest) = net.strip_prefix("unconnected-(U3-") {
                // `unconnected-(U3-<name>-Pad<n>)`: the name KiCad's symbol gave the pin.
                let name = rest.rsplit_once("-Pad").unwrap().0;
                assert_eq!(name_parts(name), name_parts(&pin.0), "pad {number}: {net}");
                open += 1;
            } else {
                let want = match net.as_str() {
                    "GND" => "GND",
                    "/USB1_D+" => "D+",
                    "/USB1_D-" => "D-",
                    "VBUS" if number == "7" => "VREGIN",
                    "VBUS" => "VBUS",
                    "Net-(U3-VDD)" => "VDD",
                    "/~{RST}" => "RSTb",
                    "/~{SUSPEND}" => "SUSPENDb",
                    "/~{RTS}" => "RTS",
                    "/~{DTR}" => "DTR",
                    // The module's TXD0 (GPIO43) is the bridge's receiver and RXD0 (GPIO44)
                    // its transmitter.
                    "/GPIO43" => "RXD",
                    "/GPIO44" => "TXD",
                    other => panic!("pad {number}: net {other}"),
                };
                assert_eq!(pin.0, want, "pad {number} is on {net}");
                named += 1;
            }
        }
        assert_eq!(
            (named, open),
            (13, 16),
            "13 pads on named nets, 16 pins left open"
        );
    }

    #[test]
    fn the_bridge_footprint_is_the_generators_and_says_how_it_differs_from_the_datasheet() {
        let built = built();
        let footprint = footprint_of(&built, "Cp2102n");
        assert_eq!(footprint.name, "QFN-28-1EP_5x5mm_P0.5mm_EP3.35x3.35mm");
        assert_eq!(
            numbered(&footprint),
            pad_numbers(&pins_of(&built, "Cp2102n"))
        );
        let pad = |n: &str| footprint.pads.iter().find(|p| p.number == n).unwrap();
        // Silicon Labs' land pattern (Table 6.2, page 35): pitch E 0.50, the centre pad
        // X2 x Y2 = 3.35 x 3.35. The pitch, the pad count and the centre pad agree.
        assert!((pad("2").at.y - pad("1").at.y).abs() - 0.5 < 1e-9);
        assert_eq!((pad("29").width, pad("29").height), (3.35, 3.35));
        // The lead pads differ: the generator's are 0.25 x 0.80 mm with centres 4.90 mm apart,
        // the datasheet's 0.30 x 0.95 mm (X1, Y1) with centres 4.80 mm apart (C1, C2).
        let (one, fifteen) = (pad("1"), pad("15"));
        assert_eq!((one.width, one.height), (0.8, 0.25));
        assert!(((fifteen.at.x - one.at.x).abs() - 4.9).abs() < 1e-9);
        // The reference's numbered pads are the generator's.
        let (far, size) = closeness(&footprint, "U3");
        assert!(far <= 0.01, "{far}");
        assert!(size <= 0.01, "{size}");
    }

    // ---- U5, the regulator ----

    #[test]
    fn the_regulator_pads_are_the_ams1117s_pins_and_its_tab_is_the_output() {
        let built = built();
        let part = part_of(&built, "LinearRegulator");
        let pads: BTreeMap<_, _> = part
            .pins
            .iter()
            .map(|p| (p.name.clone(), p.pads.clone()))
            .collect();
        // AMS1117 datasheet, page 1: 1 Ground/Adjust, 2 VOUT, 3 VIN, the tab is the output.
        assert_eq!(pads["vin.pos"], ["3"]);
        assert_eq!(pads["vin.neg"], ["1"]);
        assert_eq!(pads["vout.pos"], ["2"]);
        let footprint = footprint_of(&built, "LinearRegulator");
        assert_eq!(footprint.name, "SOT-223-3_TabPin2");
        assert_eq!(footprint.pads_numbered("2").count(), 2, "pin 2 and the tab");
        let nets: BTreeMap<_, _> = reference_pads("U5")
            .into_iter()
            .map(|p| (p.0, p.5.unwrap()))
            .collect();
        assert_eq!(nets["1"], "GND");
        assert_eq!(nets["2"], "+3.3V");
        assert_eq!(nets["3"], "VBUS");
        let (far, size) = closeness(&footprint, "U5");
        assert!(far <= 0.01, "{far}");
        assert!(size <= 0.01, "{size}");
    }

    #[test]
    fn the_regulator_row_has_the_datasheets_output_dropout_and_power() {
        let text = read("catalogs/ldo.csv");
        assert_eq!(text.lines().count(), 2);
        let row = text.lines().nth(1).unwrap();
        for figure in [
            "C6186,AMS1117-3.3,",
            ",3.201V..3.399V,1.3V,0.8A,1.2W,SOT-223,-40degC,125degC,Basic,",
        ] {
            assert!(row.contains(figure), "{figure}");
        }
        // Where each figure was read.
        for place in ["page 2", "page 3", "note 2", "boldface"] {
            assert!(row.contains(place), "{place}");
        }
    }

    // ---- Q1 and Q2, the transistors ----

    #[test]
    fn the_transistor_has_the_base_on_pad_1_the_emitter_on_2_and_the_collector_on_3() {
        let built = built();
        let pins = pins_of(&built, "Ss8050");
        assert_eq!(
            pins,
            [
                ("B".to_string(), PinKind::Passive, vec!["1".to_string()]),
                ("E".to_string(), PinKind::Passive, vec!["2".to_string()]),
                ("C".to_string(), PinKind::Passive, vec!["3".to_string()]),
            ]
        );
        let footprint = footprint_of(&built, "Ss8050");
        assert_eq!(footprint.name, "SOT-23");
        assert_eq!(numbered(&footprint), pad_numbers(&pins));
    }

    #[test]
    fn the_reference_has_its_signal_for_en_on_the_pad_that_is_the_emitter() {
        // Easyduino's Q1 has /EN on pad 2 and /~{RTS} on pad 3, and Q2 /GPIO0 on pad 2 and
        // /~{DTR} on pad 3. With the SS8050's pad 2 the emitter and pad 3 the collector (the
        // datasheet, page 1), the lines that the reset circuit pulls low are on the emitters.
        // Its symbol, `Q_NPN_BCE`, calls pad 2 the collector. This is a fact about the
        // reference, recorded in the README: the port follows the pads.
        for (reference, pad2, pad3) in [("Q1", "/EN", "/~{RTS}"), ("Q2", "/GPIO0", "/~{DTR}")] {
            let nets: BTreeMap<_, _> = reference_pads(reference)
                .into_iter()
                .map(|p| (p.0, p.5.unwrap()))
                .collect();
            assert_eq!(
                (nets["2"].as_str(), nets["3"].as_str()),
                (pad2, pad3),
                "{reference}"
            );
            assert!(nets["1"].starts_with("Net-(Q"), "{reference}: the base");
        }
    }

    #[test]
    fn the_transistor_footprint_is_the_generators_sot_23_and_the_reference_has_the_wide_one() {
        let built = built();
        let footprint = footprint_of(&built, "Ss8050");
        let (far, size) = closeness(&footprint, "Q1");
        // Easyduino's SOT-23W has its pads 1.0 x 0.7 mm at +-1.2 mm; the generator's SOT-23
        // pads are 1.475 x 0.6 mm at +-0.9375 mm, and the datasheet's soldering footprint
        // (page 4) has pads 0.80 x 0.90 mm with the single pad 2.00 mm from the other two's line. Pad numbers and sides agree.
        assert!(
            far < 0.27,
            "the farthest pad centre is {far} mm from the SOT-23W's"
        );
        assert!(size < 0.48, "{size}");
        let pad = |n: &str| footprint.pads.iter().find(|p| p.number == n).unwrap();
        // The generator's pads: 1.475 x 0.6 mm at +-0.9375 mm (1.875 mm between the columns).
        assert_eq!((pad("1").width, pad("1").height), (1.475, 0.6));
        assert!((pad("3").at.x - pad("1").at.x - 1.875).abs() < 1e-9);
        assert!(pad("1").at.x < 0.0 && pad("2").at.x < 0.0 && pad("3").at.x > 0.0);
        assert!(pad("1").at.y < 0.0 && pad("2").at.y > 0.0);
    }

    // ---- D2 and D3, the Schottky diodes ----

    #[test]
    fn the_diode_has_the_cathode_on_pad_1_and_the_anode_on_pad_2_like_the_reference() {
        let built = built();
        let part = part_of(&built, "Schottky");
        let pads: BTreeMap<_, _> = part
            .pins
            .iter()
            .map(|p| (p.name.clone(), p.pads.clone()))
            .collect();
        assert_eq!(pads["cathode"], ["1"]);
        assert_eq!(pads["anode"], ["2"]);
        for (reference, anode) in [("D2", "/VBUS1"), ("D3", "/VBUS2")] {
            let nets: BTreeMap<_, _> = reference_pads(reference)
                .into_iter()
                .map(|p| (p.0, p.5.unwrap()))
                .collect();
            assert_eq!(nets["1"], "VBUS", "{reference}: the cathode is on VBUS");
            assert_eq!(nets["2"], anode, "{reference}");
        }
    }

    #[test]
    fn the_diode_footprint_is_rohms_land_and_says_how_far_it_is_from_the_reference() {
        let built = built();
        let footprint = footprint_of(&built, "Schottky");
        assert_eq!(footprint.name, "D_ROHM_SMD1006");
        assert_eq!(
            numbered(&footprint),
            BTreeSet::from(["1".to_string(), "2".to_string()])
        );
        // ROHM RB161QS-40, Rev.001 (2016.02), page 1, Land Size Figure: pads 0.6 wide and 0.35
        // long, 0.65 mm between their centres.
        let pad = |n: &str| footprint.pads.iter().find(|p| p.number == n).unwrap();
        assert_eq!((pad("1").width, pad("1").height), (0.35, 0.6));
        assert_eq!((pad("2").width, pad("2").height), (0.35, 0.6));
        assert!((pad("2").at.x - pad("1").at.x - 0.65).abs() < 1e-9);
        assert!(
            pad("1").at.x < 0.0,
            "the cathode on the left, as KiCad's diodes"
        );
        // Easyduino's pads are the generator's 0402 diode: 0.59 x 0.64 mm, 0.97 mm apart. The
        // pad centres are 0.16 mm nearer in ROHM's land, and each pad is 0.24 mm shorter.
        let reference = reference_part("D2");
        let ref_pads = reference["pads"].as_array().unwrap();
        let distance = |p: &toml::Value| number(p.as_table().unwrap(), "x").abs();
        assert!(ref_pads.iter().all(|p| (distance(p) - 0.485).abs() < 1e-9));
        assert!((0.485 - 0.325f64 - 0.16).abs() < 1e-9);
    }

    // ---- the catalogs and the library ----

    #[test]
    fn the_rows_record_their_source_and_leave_price_and_stock_empty() {
        for (file, lcsc, mpn) in [
            (
                "catalogs/esp32_s3_wroom_1.csv",
                "C2913197",
                "ESP32-S3-WROOM-1-N4",
            ),
            ("catalogs/cp2102n.csv", "C1550553", "CP2102N-A02-GQFN28"),
            ("catalogs/cp2102n.csv", "C964632", "CP2102N-A02-GQFN28R"),
            ("catalogs/ss8050.csv", "C916392", "SS8050"),
            ("catalogs/ldo.csv", "C6186", "AMS1117-3.3"),
            ("catalogs/schottky.csv", "C2837790", "RB161QS-40T18R"),
        ] {
            let text = read(file);
            let header: Vec<&str> = text.lines().next().unwrap().split(',').collect();
            let row = text
                .lines()
                .find(|l| l.starts_with(&format!("{lcsc},{mpn},")))
                .unwrap_or_else(|| panic!("{file}: no row {lcsc} {mpn}"));
            assert!(
                row.contains(&format!("https://jlcpcb.com/partdetail/{lcsc} ")),
                "{file}: the JLCPCB page"
            );
            assert!(
                row.contains(&format!("https://www.lcsc.com/product-detail/{lcsc}.html")),
                "{file}: the LCSC page"
            );
            assert!(
                row.contains("2026-11-10"),
                "{file}: the day the pages were read"
            );
            assert!(
                row.contains("library type"),
                "{file}: the library type as the page says"
            );
            assert!(
                !header.contains(&"price") || {
                    // Where there is a price column, it is empty: the cells before `source` hold
                    // no comma.
                    let cells: Vec<&str> = row.split(',').collect();
                    let at = |name: &str| cells[header.iter().position(|c| *c == name).unwrap()];
                    at("price").is_empty() && at("stock").is_empty()
                }
            );
        }
        // The reference's number for the bridge is the older CP2102, and the row says so.
        let bridge = read("catalogs/cp2102n.csv");
        assert!(bridge.contains("C6568"));
        assert!(bridge.contains("CP2102-GMR"));
    }

    #[test]
    fn the_projects_own_footprints_are_in_the_library_and_the_rows_name_them() {
        let mapping = Mapping::for_project(Some(&project())).unwrap();
        for (kind, package, footprint) in [
            (
                "Esp32S3Wroom1",
                "SMD, 25.5x18mm",
                "easyduino_ics:ESP32-S3-WROOM-1_18x25.5mm_P1.27mm",
            ),
            ("Schottky", "SMD1006", "easyduino_ics:D_ROHM_SMD1006"),
        ] {
            let mapped = mapping.map(kind, Some(package)).unwrap();
            assert_eq!(mapped.footprint, footprint, "{kind}");
            let file = footprint.trim_start_matches("easyduino_ics:");
            assert!(
                project()
                    .join("libs/easyduino_ics.pretty")
                    .join(format!("{file}.kicad_mod"))
                    .is_file(),
                "{file}"
            );
        }
        // The two generated footprints are not in the library: the generators answer them.
        for (kind, package, footprint) in [
            (
                "Cp2102n",
                "QFN-28-EP(5x5)",
                "Package_DFN_QFN:QFN-28-1EP_5x5mm_P0.5mm_EP3.35x3.35mm",
            ),
            ("Ss8050", "SOT-23", "Package_TO_SOT_SMD:SOT-23"),
        ] {
            assert_eq!(
                mapping.map(kind, Some(package)).unwrap().footprint,
                footprint
            );
        }
    }
}

/// The board itself, `src/main.nsm`: the schematic of Easyduino's ESP32-S3 circuit (item B3c of
/// epic #308). The proofs: the nets are Easyduino's (read from the committed `.kicad_pcb` as a
/// file, with no KiCad), the pin map of the headers, the receptacles and the buttons is
/// Easyduino's and made of named pins, and the board builds with no KiCad install.
///
/// Pads that share a number are one pad here, as `Netlist::from_kicad_pcb` reads them and as
/// the design's netlist has them (a pin is one entry however many pads it has): the nine pads
/// 41 of the module are the pin `41`, the four shield pads `S1` of each receptacle the pin `S1`,
/// and the regulator's output and tab (two pads 2) the pin `2`.
mod board {
    use std::collections::{BTreeMap, BTreeSet};

    use netsmith::compare::netlist_of_design;
    use netsmith::project::Target;
    use netsmith::{Libraries, build_with};
    use netsmith_kicad::install::Search;
    use netsmith_kicad::{Mapping, testlib};
    use netsmith_netlist::{Identity, Netlist, Options, Outcome, compare, normalize_value, report};

    use super::{REFERENCE, project, read};

    /// Where each part of Easyduino's board is in the netsmith board (the path of its member),
    /// by Easyduino's reference designator. The two receptacles' parts are inside the modules
    /// `usb_uart` (J1, with R2 and R3) and `usb_native` (J2, with R7 and R8), the auto-reset
    /// pair in `auto_reset`. The three 22 uF tantalum capacitors, the three 100 nF on EN and
    /// GPIO0 and the other 100 nF and 10 kohm parts are named by what they do, so each pair is
    /// one-to-one (which of C4 and C8 is which is arbitrary: both are 100 nF from EN to ground).
    const PARTS: &[(&str, &str)] = &[
        ("U1", "module"),
        ("U2", "headers"),
        ("U3", "bridge"),
        ("U5", "regulator"),
        ("J1", "usb_uart.connector"),
        ("J2", "usb_native.connector"),
        ("SW1", "reset"),
        ("SW2", "boot"),
        ("Q1", "auto_reset.q_en"),
        ("Q2", "auto_reset.q_boot"),
        ("D1", "power_led"),
        ("D2", "usb_uart_diode"),
        ("D3", "usb_native_diode"),
        ("D4", "user_led"),
        ("R1", "en_pullup.resistor"),
        ("R2", "usb_uart.cc1_pulldown"),
        ("R3", "usb_uart.cc2_pulldown"),
        ("R4", "auto_reset.r_dtr"),
        ("R5", "auto_reset.r_rts"),
        ("R6", "power_led_resistor"),
        ("R7", "usb_native.cc1_pulldown"),
        ("R8", "usb_native.cc2_pulldown"),
        ("R9", "user_led_resistor"),
        ("R10", "suspend_pulldown.resistor"),
        ("R11", "bridge_reset_pullup.resistor"),
        ("C1", "vbus_1u"),
        ("C2", "vbus_100n"),
        ("C3", "bridge_vdd_cap"),
        ("C4", "en_100n_a"),
        ("C5", "v3v3_100n"),
        ("C6", "vbus_22u"),
        ("C7", "v3v3_22u_a"),
        ("C8", "en_100n_b"),
        ("C9", "boot_100n"),
        ("C10", "v3v3_22u_b"),
    ];

    /// The two resistors whose pads 1 and 2 are exchanged against Easyduino's: a resistor has
    /// no polarity, and netsmith's chain and `PullUp` enter a resistor at pad 1 (README,
    /// "Differences from the reference").
    const SWAPPED: [&str; 2] = ["R6", "R11"];

    fn libraries() -> Libraries {
        let nowhere = testlib::TempDir::new("easyduino-board-no-kicad");
        Libraries::for_project_by(Some(&project()), &Search::under(nowhere.path()))
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn built() -> netsmith::Built {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        let source = &target.files[0];
        let text = std::fs::read_to_string(&source.path).unwrap();
        build_with(
            &source.name,
            &text,
            &target.loader().unwrap(),
            &target.catalogs().unwrap(),
            &libraries(),
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .remove(0)
    }

    fn reference() -> Netlist {
        Netlist::from_kicad_pcb(&read(&format!("reference/{REFERENCE}")))
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// The design's reference designator for each of Easyduino's, by designator.
    fn designators(built: &netsmith::Built) -> BTreeMap<String, String> {
        PARTS
            .iter()
            .map(|(reference, path)| {
                let part = built
                    .design
                    .parts
                    .iter()
                    .find(|p| p.path == *path)
                    .unwrap_or_else(|| panic!("{reference}: the board has no part `{path}`"));
                (part.refdes.clone(), (*reference).to_string())
            })
            .collect()
    }

    type Pad = (String, String);

    /// For each pad that is on a net with another pad: the other pads of the net, in Easyduino's
    /// designators (`to_reference` maps a designator of this netlist). `swapped` names the parts
    /// whose pads 1 and 2 are read exchanged.
    fn neighbours(
        netlist: &Netlist,
        to_reference: &BTreeMap<String, String>,
        swapped: &[&str],
    ) -> BTreeMap<Pad, BTreeSet<Pad>> {
        let mut out = BTreeMap::new();
        for net in &netlist.nets {
            let pads: BTreeSet<Pad> = net
                .pins
                .iter()
                .map(|(c, pin)| {
                    let refdes = &netlist.components[*c].refdes;
                    let refdes = to_reference.get(refdes).unwrap_or(refdes).clone();
                    let pin = match (swapped.contains(&refdes.as_str()), pin.as_str()) {
                        (true, "1") => "2",
                        (true, "2") => "1",
                        (_, other) => other,
                    };
                    (refdes, pin.to_string())
                })
                .collect();
            let pads: Vec<Pad> = pads.into_iter().collect();
            if pads.len() < 2 {
                continue;
            }
            for pad in &pads {
                let others = pads.iter().filter(|p| *p != pad).cloned().collect();
                out.insert(pad.clone(), others);
            }
        }
        out
    }

    /// The same netlist with the value and footprint of every component blanked: only the
    /// wiring is left to compare. `swapped` names the parts whose pads 1 and 2 are exchanged.
    fn wiring_only(netlist: &Netlist, swapped: &[&str]) -> Netlist {
        let components = netlist
            .components
            .iter()
            .map(|c| (c.refdes.clone(), String::new(), String::new()))
            .collect();
        let nets = netlist
            .nets
            .iter()
            .map(|n| {
                let pins = n
                    .pins
                    .iter()
                    .map(|(c, pin)| {
                        let refdes = netlist.components[*c].refdes.clone();
                        let pin = match (swapped.contains(&refdes.as_str()), pin.as_str()) {
                            (true, "1") => "2".to_string(),
                            (true, "2") => "1".to_string(),
                            (_, other) => other.to_string(),
                        };
                        (refdes, pin)
                    })
                    .collect();
                (n.name.clone(), pins)
            })
            .collect();
        Netlist::from_parts(components, nets).unwrap_or_else(|e| panic!("{e}"))
    }

    fn design_netlist(built: &netsmith::Built) -> Netlist {
        let mapping = Mapping::for_project(Some(&project())).unwrap_or_else(|e| panic!("{e}"));
        netlist_of_design(&built.design, &mapping).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn the_board_builds_with_no_kicad_and_has_the_35_parts_of_easyduinos_board() {
        let built = built();
        assert_eq!(designators(&built).len(), PARTS.len());
        let own: Vec<&str> = built
            .design
            .parts
            .iter()
            .map(|p| p.path.as_str())
            .filter(|path| !PARTS.iter().any(|(_, part)| part == path))
            .collect();
        assert_eq!(own, Vec::<&str>::new(), "a part that is not Easyduino's");
        let reference = reference();
        let theirs: BTreeSet<&str> = reference
            .components
            .iter()
            .map(|c| c.refdes.as_str())
            .collect();
        let ours: BTreeSet<&str> = PARTS.iter().map(|(r, _)| *r).collect();
        assert_eq!(theirs.len(), 35);
        assert_eq!(theirs, ours);
        assert!(built.schematic.starts_with("(kicad_sch"));
    }

    /// Every pad whose other pads on its net are not the same on both sides, one line each.
    fn net_differences(
        ours: &BTreeMap<Pad, BTreeSet<Pad>>,
        theirs: &BTreeMap<Pad, BTreeSet<Pad>>,
    ) -> Vec<String> {
        let mut differences = Vec::new();
        let pads: BTreeSet<&Pad> = ours.keys().chain(theirs.keys()).collect();
        let empty = BTreeSet::new();
        for pad in pads {
            let (a, b) = (
                ours.get(pad).unwrap_or(&empty),
                theirs.get(pad).unwrap_or(&empty),
            );
            if a != b {
                differences.push(format!(
                    "{}.{}: board has {:?}, Easyduino has {:?}",
                    pad.0,
                    pad.1,
                    a.difference(b).collect::<Vec<_>>(),
                    b.difference(a).collect::<Vec<_>>()
                ));
            }
        }
        differences
    }

    /// The proof: for every pad of every part of Easyduino's board, the set of the other pads
    /// on its net is the same as on the board's. It is made from the committed `.kicad_pcb` as a
    /// file (`Netlist::from_kicad_pcb`) and the design's nets, with no `kicad-cli`. The pads of
    /// R6 and R11 are read exchanged on the reference's side (see `SWAPPED`). The 20 pads on
    /// nets of one pad (`unconnected-(...)`: 16 pins of U3 and the receptacles' SBU1 and SBU2)
    /// are on no net with another pad, and so have no entry on either side: the design leaves
    /// the same pins open, and `the_open_pins_are_the_ones_the_reference_leaves_open` says which.
    #[test]
    fn every_pad_has_the_same_pads_on_its_net_as_in_easyduinos_board() {
        let built = built();
        let to_reference = designators(&built);
        let ours = neighbours(&design_netlist(&built), &to_reference, &[]);
        let theirs = neighbours(&reference(), &BTreeMap::new(), &SWAPPED);
        let differences = net_differences(&ours, &theirs);
        assert!(differences.is_empty(), "{}", differences.join("\n"));
        // It is a real comparison: both sides have many pads on nets.
        assert!(
            theirs.len() > 100 && ours.len() == theirs.len(),
            "{} {}",
            ours.len(),
            theirs.len()
        );
    }

    /// Without the exchange, the two differ at exactly the pads of R6 and R11 and the pads on
    /// their nets: the exchange is real, and bounded to those two resistors.
    #[test]
    fn only_r6_and_r11_have_their_pads_exchanged() {
        let built = built();
        let to_reference = designators(&built);
        let ours = neighbours(&design_netlist(&built), &to_reference, &[]);
        let theirs = neighbours(&reference(), &BTreeMap::new(), &[]);
        let differences = net_differences(&ours, &theirs);
        let parts: BTreeSet<&str> = differences
            .iter()
            .map(|d| d.split('.').next().unwrap())
            .collect();
        assert_eq!(
            parts,
            BTreeSet::from([
                "D1", "R6", "R11", "U3", "C1", "C2", "C6", "D2", "D3", "U5", "U2"
            ])
            .into_iter()
            .filter(|p| differences.iter().any(|d| d.starts_with(&format!("{p}."))))
            .collect::<BTreeSet<_>>(),
            "{differences:?}"
        );
        // Every difference is on a net of R6 or R11: D1's anode, VBUS, or the bridge's reset.
        assert!(!differences.is_empty());
        assert!(
            differences
                .iter()
                .any(|d| d.starts_with("R6.1") || d.starts_with("R6.2"))
        );
        assert!(
            differences
                .iter()
                .any(|d| d.starts_with("R11.1") || d.starts_with("R11.2"))
        );
    }

    /// The same, by `netsmith-netlist`'s `compare` (what `netsmith compare` runs), with the
    /// designators paired by the table above and the value and footprint left out of the
    /// comparison.
    #[test]
    fn netsmith_compare_says_the_nets_are_the_same() {
        let built = built();
        let to_reference = designators(&built);
        let a = wiring_only(&design_netlist(&built), &[]);
        let b = wiring_only(&reference(), &SWAPPED);
        let pairs = to_reference
            .iter()
            .map(|(ours, theirs)| (ours.clone(), theirs.clone()))
            .collect();
        let options = Options {
            identity: Identity::Map(pairs),
            ..Options::default()
        };
        let outcome = compare(&a, &b, &options).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(outcome, Outcome::Match(_)), "{}", report(&outcome));
    }

    /// The comparison would see a wrong net: it is the comparison above that runs, on a board
    /// whose U2 pad 4 (GPIO4) is on another net, and its answer is the pad.
    #[test]
    fn the_comparison_sees_a_pad_on_the_wrong_net() {
        let built = built();
        let to_reference = designators(&built);
        let mut ours = neighbours(&design_netlist(&built), &to_reference, &[]);
        let theirs = neighbours(&reference(), &BTreeMap::new(), &SWAPPED);
        assert_eq!(net_differences(&ours, &theirs), Vec::<String>::new());
        // U2 pad 4 carries GPIO4; say it carried the net of pad 5.
        let key = ("U2".to_string(), "4".to_string());
        let wrong = ours
            .get(&("U2".to_string(), "5".to_string()))
            .unwrap()
            .clone();
        assert_ne!(ours[&key], wrong, "the wrong net must be another one");
        ours.insert(key, wrong);
        let differences = net_differences(&ours, &theirs);
        assert!(
            differences.iter().any(|d| d.starts_with("U2.4: board has")),
            "{differences:?}"
        );
        // A pad that is on a net on one side only is a difference as well.
        let mut ours = neighbours(&design_netlist(&built), &to_reference, &[]);
        ours.remove(&("U2".to_string(), "4".to_string()));
        assert!(
            net_differences(&ours, &theirs)
                .iter()
                .any(|d| d.starts_with("U2.4: ")),
        );
    }

    /// The values of the resistors and capacitors are Easyduino's (`5.1k` and `5k1` are one
    /// value) and the package is the one their footprint says: `1005` (metric) is 0402 and
    /// the tantalum `CP_EIA-3216` is the case A of EIA 3216-18, `CASE-A-3216-18`.
    #[test]
    fn the_resistors_and_capacitors_have_easyduinos_values_and_packages() {
        let built = built();
        let reference = reference();
        let mut wrong = Vec::new();
        let mut checked = 0;
        for (name, path) in PARTS
            .iter()
            .filter(|(r, _)| r.starts_with('R') || r.starts_with('C'))
        {
            let part = built.design.parts.iter().find(|p| p.path == *path).unwrap();
            let theirs = &reference.components[reference.find(name).unwrap()];
            if normalize_value(&part.value, true) != normalize_value(&theirs.value, true) {
                wrong.push(format!(
                    "{name}: value {} against {}",
                    part.value, theirs.value
                ));
            }
            let package = if theirs.footprint.contains("1005") {
                "0402"
            } else if theirs.footprint.contains("CP_EIA-3216") {
                "CASE-A-3216-18"
            } else {
                panic!("{name}: footprint {}", theirs.footprint)
            };
            if part.package.as_deref() != Some(package) {
                wrong.push(format!(
                    "{name}: package {:?} against {package}",
                    part.package
                ));
            }
            checked += 1;
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
        assert_eq!(checked, 21, "11 resistors and 10 capacitors");
    }

    /// Every part of the board is sourced with the LCSC number of Easyduino's bill of
    /// materials, but for the bridge's (the BOM's C6568 is the older CP2102, README) and the
    /// headers (hand-soldered, the BOM's `-`).
    #[test]
    fn the_parts_have_the_lcsc_numbers_of_easyduinos_bill_of_materials() {
        let built = built();
        let to_reference = designators(&built);
        let bom = read("reference/Easyduino_ESP32S3_BOM.csv");
        let mut theirs = BTreeMap::new();
        for line in bom.lines().skip(1) {
            let cells: Vec<&str> = line.split(',').map(|c| c.trim_matches('"')).collect();
            theirs.insert(cells[0].to_string(), cells[4].to_string());
        }
        assert_eq!(theirs.len(), 35);
        let mut wrong = Vec::new();
        for s in &built.sourcing {
            let reference = &to_reference[&s.refdes];
            let bom_number = &theirs[reference];
            match reference.as_str() {
                "U2" => assert_eq!(s.lcsc, None),
                "U3" => assert_eq!(
                    (s.lcsc.as_deref(), bom_number.as_str()),
                    (Some("C1550553"), "C6568")
                ),
                _ => {
                    if s.lcsc.as_deref() != Some(bom_number.as_str()) {
                        wrong.push(format!("{reference}: {:?} against {bom_number}", s.lcsc));
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// The net a pad of a part is on, as the signal it carries in Easyduino's terms: the
    /// design's net name for a rail or ground, else the other pads on it, in Easyduino's
    /// designators.
    fn others_on_net(
        built: &netsmith::Built,
        to_reference: &BTreeMap<String, String>,
        path: &str,
        pad: &str,
    ) -> (String, BTreeSet<Pad>) {
        let part = built.design.parts.iter().find(|p| p.path == path).unwrap();
        let net = built
            .design
            .nets
            .iter()
            .find(|n| {
                n.pins
                    .iter()
                    .any(|p| p.refdes == part.refdes && p.pin == pad)
            })
            .unwrap_or_else(|| panic!("{path}.{pad} is on no net"));
        let others = net
            .pins
            .iter()
            .filter(|p| !(p.refdes == part.refdes && p.pin == pad))
            .map(|p| (to_reference[&p.refdes].clone(), p.pin.clone()))
            .collect();
        (net.name.clone(), others)
    }

    /// The signal a pad of the module carries, by the pin of the module's table: the datasheet's
    /// name of the pin that the net reaches (`IO4`, `RXD0`, `EN`).
    fn module_pin_on(built: &netsmith::Built, net_name: &str) -> Option<String> {
        let module = built
            .design
            .parts
            .iter()
            .find(|p| p.path == "module")
            .unwrap();
        let net = built.design.nets.iter().find(|n| n.name == net_name)?;
        net.pins
            .iter()
            .filter(|p| p.refdes == module.refdes)
            .filter_map(|p| module.pins.iter().find(|q| q.pads.contains(&p.pin)))
            .map(|q| q.datasheet.clone())
            .next()
    }

    /// What each header pad carries on Easyduino's board, from its net name: a rail, ground, or
    /// the module's GPIO (`/GPIO44` is the module's pad `RXD0`, `/GPIO43` its `TXD0`, and
    /// `/USB2_D-` and `/USB2_D+` its `IO19` and `IO20`).
    fn reference_signals() -> BTreeMap<String, String> {
        let reference = reference();
        let c = reference.find("U2").unwrap();
        let mut out = BTreeMap::new();
        for (pad, net) in &reference.components[c].pins {
            let name = reference.nets[*net]
                .name
                .trim_start_matches('/')
                .to_string();
            let signal = match name.as_str() {
                "GND" | "+3.3V" | "VBUS" | "EN" => name,
                "GPIO44" => "RXD0".to_string(),
                "GPIO43" => "TXD0".to_string(),
                "USB2_D-" => "IO19".to_string(),
                "USB2_D+" => "IO20".to_string(),
                other => format!("IO{}", other.strip_prefix("GPIO").unwrap()),
            };
            out.insert(pad.clone(), signal);
        }
        out
    }

    /// The headers' pin map is Easyduino's, pad by pad, and it is made of named pins: the
    /// module's pin that a header pad reaches is the one on the net of that pad in the design.
    #[test]
    fn the_header_pins_carry_easyduinos_signals() {
        let built = built();
        let design = &built.design;
        let headers = design.parts.iter().find(|p| p.path == "headers").unwrap();
        let expected = reference_signals();
        assert_eq!(expected.len(), 44, "two headers of 22 pads, each on a net");
        let mut wrong = Vec::new();
        for (pad, signal) in &expected {
            let net = design
                .nets
                .iter()
                .find(|n| {
                    n.pins
                        .iter()
                        .any(|p| p.refdes == headers.refdes && p.pin == *pad)
                })
                .unwrap_or_else(|| panic!("U2.{pad} is on no net"));
            let found = match signal.as_str() {
                "GND" | "+3.3V" | "VBUS" => net.name.clone(),
                _ => module_pin_on(&built, &net.name).unwrap_or_default(),
            };
            if found != *signal {
                wrong.push(format!("U2.{pad}: {found}, Easyduino's is {signal}"));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
        // Every header pin is joined to a named pin, not to a request: the source has no `.GPIO`.
        let source = read("src/main.nsm");
        assert!(
            !source.contains(": .module."),
            "a request for a pin of the module"
        );
        assert!(
            built
                .design
                .parts
                .iter()
                .find(|p| p.path == "module")
                .unwrap()
                .pins
                .iter()
                .all(|p| p.function.is_none())
        );
    }

    /// The receptacles carry Easyduino's signals on each contact: VBUS on the net behind the
    /// receptacle's diode, ground on the ground and the shield, each CC on its 5.1 kohm resistor
    /// to ground, the data pair on the bridge (J1) or the module and the headers (J2), and the
    /// sideband contacts alone.
    #[test]
    fn the_receptacles_carry_easyduinos_signals() {
        let built = built();
        let to_reference = designators(&built);
        // (receptacle, path, cc1 resistor, cc2 resistor, vbus net, diode, d+ partner, d- partner)
        let receptacles = [
            (
                "J1",
                "usb_uart.connector",
                "R2",
                "R3",
                "VBUS1",
                "D2",
                ("U3", "4"),
                ("U3", "5"),
            ),
            (
                "J2",
                "usb_native.connector",
                "R7",
                "R8",
                "VBUS2",
                "D3",
                ("U1", "14"),
                ("U1", "13"),
            ),
        ];
        let reference = reference();
        let mut checked = 0;
        for (j, path, cc1, cc2, vbus, diode, dp, dm) in receptacles {
            // Every pad of the reference's receptacle is on the design's net the same way.
            let c = reference.find(j).unwrap();
            let mut seen = BTreeSet::new();
            for (pad, net) in &reference.components[c].pins {
                seen.insert(pad.clone());
                let theirs = reference.nets[*net].name.as_str();
                let (name, others) = others_on_net(&built, &to_reference, path, pad);
                let has =
                    |part: &str, pin: &str| others.contains(&(part.to_string(), pin.to_string()));
                let ok = match pad.as_str() {
                    "A1" | "A12" | "B1" | "B12" | "S1" => name == "GND" && theirs == "GND",
                    "A4" | "A9" | "B4" | "B9" => {
                        name == vbus && theirs == format!("/{vbus}") && has(diode, "2")
                    }
                    "A5" => has(cc1, "1") && others.len() == 1,
                    "B5" => has(cc2, "1") && others.len() == 1,
                    "A6" | "B6" => has(dp.0, dp.1),
                    "A7" | "B7" => has(dm.0, dm.1),
                    other => panic!("{j}: pad {other}"),
                };
                assert!(
                    ok,
                    "{j}.{pad}: design net {name} {others:?}, Easyduino's {theirs}"
                );
                checked += 1;
            }
            // The reader leaves a pad that is alone on its net out of a part's pins: the
            // sideband contacts A8 and B8 (`the_open_pins_are_the_ones_the_reference_leaves_open`).
            assert_eq!(
                seen.len(),
                15,
                "{j}: 14 contacts on nets, and the shield S1"
            );
        }
        assert_eq!(checked, 30, "15 contacts on nets for each receptacle");
    }

    /// The buttons: SW1 (RESET) has ground on pad 1 and EN on pad 2, SW2 (BOOT) ground on pad 1
    /// and GPIO0 on pad 2, as on Easyduino's board.
    #[test]
    fn the_buttons_have_easyduinos_nets_on_their_pads() {
        let built = built();
        let to_reference = designators(&built);
        let reference = reference();
        for (button, path, signal, module_pad) in [
            ("SW1", "reset", "/EN", "3"),
            ("SW2", "boot", "/GPIO0", "27"),
        ] {
            let c = reference.find(button).unwrap();
            let nets: BTreeMap<&str, &str> = reference.components[c]
                .pins
                .iter()
                .map(|(pad, net)| (pad.as_str(), reference.nets[*net].name.as_str()))
                .collect();
            assert_eq!(nets["1"], "GND");
            assert_eq!(nets["2"], signal);
            let (ground, _) = others_on_net(&built, &to_reference, path, "1");
            assert_eq!(ground, "GND");
            let (_, others) = others_on_net(&built, &to_reference, path, "2");
            assert!(
                others.contains(&("U1".to_string(), module_pad.to_string())),
                "{button}: {others:?}"
            );
        }
    }

    /// The pins that the reference leaves open (20 pads on nets named `unconnected-(...)`, read
    /// from the committed table) are the pads that no net of the design names: 16 pins of the
    /// bridge and the receptacles' SBU1 and SBU2 (pads A8 and B8), and no other pad of the
    /// board is open. (The netlist reader drops a net of one pad, so the proof of the nets above
    /// says nothing about them.)
    #[test]
    fn the_open_pins_are_the_ones_the_reference_leaves_open() {
        let built = built();
        let to_reference = designators(&built);
        let table = super::parsed(&read("reference/placements.toml"));
        let mut theirs: BTreeSet<Pad> = BTreeSet::new();
        for part in super::tables(&table, "part") {
            let reference = part["reference"].as_str().unwrap();
            for pad in part["pads"].as_array().unwrap() {
                let pad = pad.as_table().unwrap();
                let net = pad.get("net").and_then(|n| n.as_str()).unwrap_or("");
                if net.starts_with("unconnected-(") {
                    theirs.insert((
                        reference.to_string(),
                        pad["number"].as_str().unwrap().to_string(),
                    ));
                }
            }
        }
        assert_eq!(theirs.len(), 20, "{theirs:?}");
        let ours: BTreeSet<Pad> = {
            let design = &built.design;
            let mut on_net: BTreeSet<Pad> = BTreeSet::new();
            for net in &design.nets {
                for p in &net.pins {
                    on_net.insert((to_reference[&p.refdes].clone(), p.pin.clone()));
                }
            }
            // Every pad of a part of the design that no net names.
            let mut open = BTreeSet::new();
            for part in &design.parts {
                for pin in &part.pins {
                    for pad in &pin.pads {
                        let key = (to_reference[&part.refdes].clone(), pad.clone());
                        if !on_net.contains(&key) {
                            open.insert(key);
                        }
                    }
                }
            }
            open
        };
        // The design's netlist has a pin once however many pads it has; the pads of the open
        // set are the pins' pads, and a pin on a net has all its pads on it.
        assert_eq!(ours, theirs, "ours {ours:?}");
    }

    /// What the solver does and does not check on the power path (README, "What the solver
    /// checks"): the regulator's output current (500 mA, the reference's figure from USB) is
    /// held to the AMS1117's rating and the output rail to the module's supply window, and the
    /// dropout from USB through a Schottky diode is NOT evaluated, because the bridge's supply
    /// pins and its reset line (a pull-up to VBUS) are not modelled, so the rail's voltage has
    /// no value. This pins that gap: when the bridge states a supply current and the dropout
    /// is evaluated, this test fails and the README's paragraph is to be rewritten.
    #[test]
    fn the_power_path_is_checked_as_far_as_the_parts_state_and_the_rest_is_listed() {
        let built = built();
        let listed = |needle: &str| built.notes.iter().any(|n| n.contains(needle));
        // Checked (not listed): the supply window of the module, the regulator's current.
        assert!(!listed("module.vdd"), "{:?}", built.notes);
        assert!(!listed(
            "not evaluated: -regulator.vout.current <= regulator.max_current"
        ));
        // Not evaluated: the dropout and the regulator's power.
        assert!(listed(
            "not evaluated: regulator.vin.voltage >= (regulator.vout.voltage + regulator.dropout)"
        ));
        assert!(listed(
            "the balance on net `VBUS` is incomplete: `bridge.vbus`, `bridge.vregin` are not modelled"
        ));
    }

    /// The module's supply window is a real check: a regulator that gives 3.7 V is refused.
    #[test]
    fn a_regulator_that_gives_more_than_the_modules_window_is_refused() {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        let source = &target.files[0];
        let text = std::fs::read_to_string(&source.path).unwrap();
        let wrong = text.replace(
            "    .module.vdd ~ .v3v3\n",
            "    .module.vdd ~ .v3v3\n    .v3v3.voltage &= 3.7V +/- 1%\n",
        );
        assert_ne!(wrong, text);
        let result = build_with(
            &source.name,
            &wrong,
            &target.loader().unwrap(),
            &target.catalogs().unwrap(),
            &libraries(),
        );
        let message = match result {
            Ok(_) => panic!("a 3.7 V rail was accepted"),
            Err(e) => format!("{e:?}"),
        };
        assert!(
            message.contains("3.6V") || message.contains("vdd") || message.contains("v3v3"),
            "{message}"
        );
    }

    #[test]
    fn the_electrical_rules_check_of_kicad_passes_on_the_schematic() {
        let built = built();
        let Some(found) = crate::erc::erc(&built.design.name, &built.schematic) else {
            return;
        };
        assert_eq!(
            crate::erc::with_the_machines_libraries(found),
            Vec::<String>::new()
        );
    }

    /// The pin optimizer (ADR 0101) moves only a pin that a request got. The board has none:
    /// the check's `spaces` for it are empty, the pin file has no entry and there is none on
    /// disk. The module's table has no function rows (a pin is one name), so a header pad
    /// could not even be joined to a request for "any GPIO".
    #[test]
    fn the_optimizer_cannot_move_a_header_pad() {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        let source = &target.files[0];
        let text = std::fs::read_to_string(&source.path).unwrap();
        let (loader, catalogs) = (target.loader().unwrap(), target.catalogs().unwrap());
        let libraries = libraries();
        let outcome =
            netsmith::outcome_with(&source.name, &text, &loader, &catalogs, Some(&libraries))
                .unwrap_or_else(|p| panic!("{}", p[0].rendered));
        let requests = outcome
            .pins
            .spaces
            .get("EasyduinoEsp32s3")
            .map_or(0, |parts| {
                parts.iter().map(|s| s.requests.len()).sum::<usize>()
            });
        assert_eq!(requests, 0, "the board asks for a pin somewhere");
        assert!(
            outcome
                .pins
                .file
                .boards
                .get("EasyduinoEsp32s3")
                .is_none_or(|e| e.is_empty())
        );
        assert!(outcome.pins.unserved.values().all(|u| u.is_empty()));
        assert!(
            !project().join("src/main.pins.toml").exists(),
            "a pin file would keep an answer for a request"
        );
    }
}
