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

    .rail.pos ~ .u1.v3v3
    .rail.neg ~ .u1.gnd
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
