//! `examples/rp2350_minimal/`: the home of the port of Raspberry Pi's RP2350A Minimal board, and
//! the reference table (`reference/placements.toml`) that a parity checker will read. The table
//! is read from Raspberry Pi's board file by `cargo run -p netsmith --example reference_table`
//! (`netsmith::reference_table`); these tests run that reading on a small board of our own, and
//! prove that the committed table is what the program prints for the committed board file.
//!
//! The MCU, `src/rp2350a.nsm` (the RP2350A, QFN-60), is checked against that table too: the pads
//! of Raspberry Pi's U1 carry net names, and where a net is named like a datasheet pin the pad
//! numbers must agree. Nothing here needs KiCad: the footprint comes from netsmith's generators
//! and the symbol is drawn from the pin table.
//!
//! The connectors (J1 micro-USB, J2 and J3 the 2 x 18 headers, J4 JST-SH) are the project's own
//! parts with footprints in its own library (`libs/netsmith_rp2350.pretty`): their pads are
//! compared with those of Raspberry Pi's board, pad by pad, and the board builds with no KiCad
//! installed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use netsmith::project::Target;
use netsmith::reference_table::placements_toml;
use netsmith::{Libraries, build_with, outcome_with};
use netsmith_core::PinKind;
use netsmith_kicad::install::Search;
use netsmith_kicad::testlib;
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_pcb::{Footprint, FootprintLibrary, PadKind, PadShape};

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
    // J1's two custom pads state the box of their polygons; no other pad has an extent.
    let extents: Vec<(&str, &str)> = parts
        .iter()
        .flat_map(|p| {
            p["pads"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|pad| pad.as_table().unwrap().contains_key("extent"))
                .map(|pad| {
                    (
                        p["reference"].as_str().unwrap(),
                        pad["number"].as_str().unwrap(),
                    )
                })
        })
        .collect();
    assert_eq!(extents, [("J1", "7"), ("J1", "10")]);
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
    assert_eq!(names, ["Rp2350Minimal"]);
}

/// U2 (the onsemi NCP1117ST33T3G, the standard `LinearRegulator` with a row of the project's own
/// `catalogs/ldo.csv`), L1 (the Abracon AOTA-B201610S3R3-101-T, the standard `Inductor` with a row
/// of `catalogs/inductor.csv` and a footprint of the project's library) and SW1 and SW2 (the
/// Wurth 434133025816, the project's `TactileSwitch`, its catalog and its footprint). Nothing
/// here needs KiCad: the regulator's footprint is netsmith's generator, the other two are files
/// of `libs/rp2350_minimal.pretty`, and the symbols are drawn by netsmith.
mod regulator_inductor_switch {
    use std::collections::{BTreeMap, BTreeSet};

    use netsmith::project::Target;
    use netsmith::{Libraries, build_with};
    use netsmith_kicad::{Mapping, SymbolLib, testlib};
    use netsmith_pcb::FootprintLibrary;

    use super::{pad_differences, parsed, project, read, tables};

    /// A small board of our own, never built by the project: a regulator from a 5 V input to
    /// 3.3 V, an inductor between two headers, and a button between two headers. The nets are
    /// those of the parts' use on Raspberry Pi's board in spirit only.
    const FIXTURE: &str = r#"
import src::tactile_switch::TactileSwitch
import std::parts::connector::Header1x2
import std::parts::passive::Inductor
import std::parts::regulator::LinearRegulator
import std::sources::Rail

board Fixture {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VBUS"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..100mA
    }
    .u2: LinearRegulator {
        .package &= "SOT-223"
        .vout.voltage &= 3.2V..3.4V
        .quiescent_current &= 0mA..10mA
    }
    .input ~ .u2.vin
    .u2.vout ~ .output

    .power: Header1x2
    .input.pos ~ .power.p1
    .input.neg ~ .power.p2

    .l1: Inductor {
        .package &= "0806"
        .inductance &= 3.3uH +/- 20%
    }
    .lx: Header1x2
    .out: Header1x2
    .lx.p1 ~ .l1.a
    .l1.b ~ .lx.p2
    .output.pos ~ .out.p1
    .output.neg ~ .out.p2

    .sw1: TactileSwitch { .mpn &= "434133025816" }
    .btn: Header1x2
    .sw1.a ~ .btn.p1
    .sw1.b ~ .btn.p2
}
"#;

    fn libraries() -> Libraries {
        let mapping = Mapping::for_project(Some(&project())).unwrap_or_else(|e| panic!("{e}"));
        Libraries::with_mapping(
            mapping,
            SymbolLib::at(testlib::sample_symbols()),
            FootprintLibrary::at(testlib::sample_footprints()).with_generators(),
        )
    }

    fn built() -> netsmith::Built {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        build_with(
            "fixture.nsm",
            FIXTURE,
            &target.loader().unwrap(),
            &target.catalogs().unwrap(),
            &libraries(),
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .remove(0)
    }

    /// The pads of a part's pins as the design has them: pin name to its pads.
    fn pads_of(built: &netsmith::Built, refdes: &str) -> BTreeMap<String, Vec<String>> {
        let part = built
            .design
            .parts
            .iter()
            .find(|p| p.refdes == refdes)
            .unwrap_or_else(|| panic!("no part {refdes}"));
        part.pins
            .iter()
            .map(|p| (p.name.clone(), p.pads.clone()))
            .collect()
    }

    /// The footprint a part is exported with, loaded as the build loads it.
    fn footprint_of(built: &netsmith::Built, refdes: &str) -> netsmith_pcb::Footprint {
        let part = built
            .design
            .parts
            .iter()
            .find(|p| p.refdes == refdes)
            .unwrap();
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

    fn unique_pads(footprint: &netsmith_pcb::Footprint) -> BTreeSet<String> {
        footprint
            .pad_numbers()
            .into_iter()
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// The pads of Raspberry Pi's part with their nets: pad number to net.
    fn reference_nets(reference: &str) -> BTreeMap<String, String> {
        let table = parsed(&read("reference/placements.toml"));
        let parts = tables(&table, "part");
        let part = parts
            .iter()
            .find(|p| p["reference"].as_str() == Some(reference))
            .unwrap_or_else(|| panic!("the reference has no {reference}"));
        part["pads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_table().unwrap())
            .filter_map(|p| {
                Some((
                    p["number"].as_str()?.to_string(),
                    p.get("net")?.as_str()?.to_string(),
                ))
            })
            .collect()
    }

    #[test]
    fn each_part_parses_and_its_catalog_row_is_the_one_the_board_gets() {
        // The catalogs load with the project's three files beside the standard ones.
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        assert!(target.catalogs().is_ok());
        // The fixture's regulator is its only IC, so it is U1 here (Raspberry Pi's is U2).
        let built = built();
        let sourced = |refdes: &str| {
            built
                .sourcing
                .iter()
                .find(|s| s.refdes == refdes)
                .unwrap_or_else(|| panic!("no {refdes} in the sourcing"))
        };
        for (refdes, lcsc, mpn) in [
            ("U1", "C26537", "NCP1117ST33T3G"),
            ("L1", "C42411119", "AOTA-B201610S3R3-101-T"),
            ("SW1", "C5504987", "434133025816"),
        ] {
            let row = sourced(refdes);
            assert_eq!(row.lcsc.as_deref(), Some(lcsc), "{refdes}");
            assert_eq!(row.mpn.as_deref(), Some(mpn), "{refdes}");
        }
    }

    #[test]
    fn the_rows_record_their_source_and_leave_price_and_stock_empty() {
        for (file, lcsc) in [
            ("catalogs/ldo.csv", "C26537"),
            ("catalogs/inductor.csv", "C42411119"),
            ("catalogs/tactile_switch.csv", "C5504987"),
        ] {
            let text = read(file);
            let header = text.lines().next().unwrap();
            let columns: Vec<&str> = header.split(',').collect();
            // The cells before `source` hold no comma, so they split on the commas.
            let cells: Vec<&str> = text.lines().nth(1).unwrap().split(',').collect();
            let cell = |name: &str| cells[columns.iter().position(|c| *c == name).unwrap()];
            assert_eq!(cell("lcsc"), lcsc, "{file}");
            assert_eq!(cell("price"), "", "{file}: price is not read");
            assert_eq!(cell("stock"), "", "{file}: stock is not read");
            assert_eq!(cell("library"), "Extended", "{file}");
            let source = text.lines().nth(1).unwrap();
            assert!(
                source.contains(&format!("https://jlcpcb.com/partdetail/{lcsc} ")),
                "{file}: the JLCPCB page"
            );
            assert!(
                source.contains("read 2026-11-10"),
                "{file}: the day the page was read"
            );
            assert_eq!(text.lines().count(), 2, "{file}: one row");
        }
    }

    #[test]
    fn the_regulators_pads_are_the_ncp1117s_pins_and_its_tab_is_the_output() {
        let built = built();
        // Datasheet (NCP1117/D Rev. 31, page 1): 1 Adjust/Ground, 2 Output, 3 Input, the
        // heatsink tab is pin 2. The standard table says the same.
        let pads = pads_of(&built, "U1");
        assert_eq!(pads["vin.pos"], ["3"]);
        assert_eq!(pads["vin.neg"], ["1"]);
        assert_eq!(pads["vout.pos"], ["2"]);
        let footprint = footprint_of(&built, "U1");
        assert_eq!(
            footprint.name, "SOT-223-3_TabPin2",
            "the generator's SOT-223 whose tab is numbered like pin 2"
        );
        let all: BTreeSet<String> = pads.values().flatten().cloned().collect();
        assert_eq!(unique_pads(&footprint), all);
        assert_eq!(footprint.pads_numbered("2").count(), 2, "pin 2 and the tab");
    }

    #[test]
    fn raspberry_pis_u2_has_its_input_output_and_ground_where_the_part_has_them() {
        let built = built();
        let pads = pads_of(&built, "U1");
        let reference = reference_nets("U2");
        // Input from USB's VBUS, output the 3.3 V rail, ground.
        assert_eq!(reference[&pads["vin.pos"][0]], "VBUS");
        assert_eq!(reference[&pads["vout.pos"][0]], "+3V3");
        assert_eq!(reference[&pads["vin.neg"][0]], "GND");
        // Raspberry Pi's footprint numbers the tab 4 and puts it on the output net; the
        // generator's footprint numbers the tab 2, so the pad number differs and the net is
        // the same (the datasheet says the tab is pin 2, the output).
        assert_eq!(reference["4"], "+3V3");
        assert_eq!(reference.len(), 4);
    }

    #[test]
    fn the_inductors_pads_are_its_two_pins_and_the_land_pattern_is_abracons() {
        let built = built();
        let pads = pads_of(&built, "L1");
        assert_eq!(pads["a"], ["1"]);
        assert_eq!(pads["b"], ["2"]);
        let footprint = footprint_of(&built, "L1");
        assert_eq!(
            unique_pads(&footprint),
            BTreeSet::from(["1".to_string(), "2".to_string()])
        );
        // Abracon, revision A of 9/13/2024, page 4: two pads 1.00 x 1.60 mm with 1.00 mm
        // between them, so centres 2.00 mm apart.
        let at = |n: &str| footprint.pads_numbered(n).next().unwrap().clone();
        let (one, two) = (at("1"), at("2"));
        for pad in [&one, &two] {
            assert_eq!((pad.width, pad.height), (1.0, 1.6));
            assert_eq!(pad.at.y, 0.0);
        }
        assert!((two.at.x - one.at.x - 2.0).abs() < 1e-9);
    }

    #[test]
    fn raspberry_pis_l1_has_the_output_rail_on_pad_1_and_the_switch_node_on_pad_2() {
        let reference = reference_nets("L1");
        assert_eq!(reference["1"], "+1V1");
        assert_eq!(reference["2"], "/VREG_LX");
        assert_eq!(reference.len(), 2);
        // The part's pins are `a` on pad 1 and `b` on pad 2, so a board that follows the
        // reference joins `.a` to the core rail and `.b` to the regulator's VREG_LX pin.
        let pads = pads_of(&built(), "L1");
        assert_eq!(pads["a"], ["1"]);
        assert_eq!(pads["b"], ["2"]);
    }

    #[test]
    fn the_switchs_pairs_are_the_datasheets_and_its_land_pattern_is_wurths() {
        let built = built();
        // Wurth 434133025816, revision 001.001 (2021-01-25), page 1, schematic: 1 and 2 are
        // joined, 3 and 4 are joined, and the button joins the two pairs.
        let pads = pads_of(&built, "SW1");
        assert_eq!(pads["a"], ["1", "2"]);
        assert_eq!(pads["b"], ["3", "4"]);
        let footprint = footprint_of(&built, "SW1");
        let all: BTreeSet<String> = pads.values().flatten().cloned().collect();
        assert_eq!(unique_pads(&footprint), all);
        // Page 1, recommended land pattern: 5.2 mm over the pads and 3.1 mm between them
        // across, 2.8 mm over the pads and 1.5 mm between them along: pads 1.05 x 0.65 mm
        // with centres at +/-2.075 and +/-1.075 mm; 1 and 2 on the upper row, 1 and 3 on the left.
        for (number, x, y) in [
            ("1", -2.075, -1.075),
            ("2", 2.075, -1.075),
            ("3", -2.075, 1.075),
            ("4", 2.075, 1.075),
        ] {
            let pad = footprint.pads_numbered(number).next().unwrap();
            assert_eq!((pad.width, pad.height), (1.05, 0.65), "pad {number}");
            assert!(
                (pad.at.x - x).abs() < 1e-9 && (pad.at.y - y).abs() < 1e-9,
                "pad {number} is at {:?}",
                pad.at
            );
        }
    }

    #[test]
    fn raspberry_pis_buttons_have_a_net_on_each_pair_and_the_same_pads() {
        let table = parsed(&read("reference/placements.toml"));
        let built = built();
        let pads = pads_of(&built, "SW1");
        let footprint = footprint_of(&built, "SW1");
        for (reference, pair_a, pair_b) in [
            ("SW1", "GND", "/~{USB_BOOT}"),
            ("SW2", "Net-(R4-Pad1)", "/RUN"),
        ] {
            let nets = reference_nets(reference);
            for pad in &pads["a"] {
                assert_eq!(nets[pad], pair_a, "{reference} pad {pad}");
            }
            for pad in &pads["b"] {
                assert_eq!(nets[pad], pair_b, "{reference} pad {pad}");
            }
            // The footprint is the same as Raspberry Pi's, pad for pad: number, centre, size
            // in the footprint's own frame, shape, angle and drill (`pad_differences`).
            let differences = pad_differences(&footprint, &table, reference);
            assert!(differences.is_empty(), "{}", differences.join("\n"));
        }
        // And it can fail: a footprint with every pad turned by 90 degrees, or with the width
        // and height of each swapped, is not Raspberry Pi's, though a sorted set of sizes is.
        let mut turned = footprint.clone();
        for pad in &mut turned.pads {
            pad.angle += 90.0;
        }
        let mut swapped = footprint.clone();
        for pad in &mut swapped.pads {
            std::mem::swap(&mut pad.width, &mut pad.height);
        }
        for (what, wrong) in [("turned", turned), ("swapped", swapped)] {
            for reference in ["SW1", "SW2"] {
                let differences = pad_differences(&wrong, &table, reference);
                assert_eq!(
                    differences.len(),
                    4,
                    "{reference}, pads {what}: {differences:?}"
                );
            }
        }
    }

    #[test]
    fn the_fixture_builds_a_schematic_with_all_three_parts_and_no_kicad_library() {
        let built = built();
        for refdes in ["U1", "L1", "SW1"] {
            assert!(
                built.design.parts.iter().any(|p| p.refdes == refdes),
                "no {refdes}"
            );
        }
        assert!(
            built.schematic.contains("(number \"4\""),
            "the switch's pad 4"
        );
    }
}

/// A small board of our own, never built by the project: the RP2350A with its supplies, to
/// prove that the part, its catalog row and its `kicad.toml` row build together. The rails are
/// the supplies of the datasheet's pins; nothing says they are Raspberry Pi's.
const BARE: &str = r#"
import src::rp2350a::Rp2350A
import std::sources::Rail

board Rp2350aBare {
    .io: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .core: Rail {
        .voltage &= 1.1V +/- 5%
        .pos.net &= "1V1"
        .neg.net &= "GND"
    }
    .u: Rp2350A { .mpn &= "RP2350A" }

    .io.pos ~ .u.iovdd
    .io.pos ~ .u.adc_avdd
    .io.pos ~ .u.vreg_avdd
    .io.pos ~ .u.vreg_vin
    .io.pos ~ .u.usb_otp_vdd
    .io.pos ~ .u.qspi_iovdd
    .io.pos ~ .u.run
    .io.pos ~ .u.swclk
    .core.pos ~ .u.dvdd
    .core.pos ~ .u.vreg_fb
    .io.neg ~ .u.gnd
    .io.neg ~ .u.vreg_pgnd
    .core.neg ~ .io.neg
}
"#;

fn built_bare() -> netsmith::Built {
    let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
    let mapping = Mapping::for_project(Some(&project())).unwrap_or_else(|e| panic!("{e}"));
    let libraries = Libraries::with_mapping(
        mapping,
        SymbolLib::at(testlib::sample_symbols()),
        FootprintLibrary::at(testlib::sample_footprints()).with_generators(),
    );
    build_with(
        "bare.nsm",
        BARE,
        &target.loader().unwrap(),
        &target.catalogs().unwrap(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0)
}

/// The pin table of the part as the design has it: datasheet name to its pads, in pin order.
fn pins_of(built: &netsmith::Built) -> Vec<(String, PinKind, Vec<String>)> {
    let part = built
        .design
        .parts
        .iter()
        .find(|p| p.kind == "Rp2350A")
        .expect("the board has the RP2350A");
    part.pins
        .iter()
        .map(|p| (p.datasheet.clone(), p.kind, p.pads.clone()))
        .collect()
}

/// A net's name as it is compared: without a leading `/` (KiCad's mark of a sheet-level net),
/// in lower case.
fn plain(name: &str) -> String {
    name.trim_start_matches('/').to_ascii_lowercase()
}

#[test]
fn the_part_has_sixty_pads_and_the_exposed_pad_each_once() {
    let pins = pins_of(&built_bare());
    let mut pads: Vec<&str> = pins
        .iter()
        .flat_map(|(_, _, pads)| pads.iter().map(String::as_str))
        .collect();
    assert_eq!(pads.len(), 61, "61 pads, one per entry: {pads:?}");
    pads.sort_unstable();
    pads.dedup();
    assert_eq!(pads.len(), 61, "a pad is given twice");
    let wanted: BTreeSet<String> = (1..=61).map(|n| n.to_string()).collect();
    assert_eq!(
        pads.iter().map(|p| p.to_string()).collect::<BTreeSet<_>>(),
        wanted,
        "the pads are 1 to 60 and 61, the exposed pad"
    );
    // The exposed pad (numbered 61 by the footprint generator, as in Raspberry Pi's board) is
    // the ground, and the two other grounds and the supplies are power pins.
    let ep = pins
        .iter()
        .find(|(_, _, pads)| pads.iter().any(|p| p == "61"))
        .unwrap();
    assert_eq!((ep.0.as_str(), ep.1), ("GND", PinKind::Power));
    for (name, pads) in [
        ("IOVDD", 6),
        ("DVDD", 3),
        ("QSPI_IOVDD", 1),
        ("USB_OTP_VDD", 1),
        ("ADC_AVDD", 1),
        ("VREG_AVDD", 1),
        ("VREG_VIN", 1),
        ("VREG_PGND", 1),
    ] {
        let pin = pins
            .iter()
            .find(|p| p.0 == name)
            .unwrap_or_else(|| panic!("no {name}"));
        assert_eq!((pin.1, pin.2.len()), (PinKind::Power, pads), "{name}");
    }
    let gpios = pins.iter().filter(|p| p.0.starts_with("GPIO")).count();
    assert_eq!(gpios, 30);
    assert!(
        pins.iter()
            .filter(|p| p.0.starts_with("GPIO"))
            .all(|p| p.1 == PinKind::Bidirectional)
    );
}

#[test]
fn the_pads_agree_with_the_nets_of_raspberry_pis_u1_wherever_a_net_is_named_like_a_pin() {
    let pins = pins_of(&built_bare());
    let mut pads_of: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, _, pads) in &pins {
        pads_of
            .entry(plain(name))
            .or_default()
            .extend(pads.iter().cloned());
    }
    // Raspberry Pi ties the regulator's power ground (VREG_PGND, pad 47) to the net `GND`, as
    // the exposed pad is, so that net is two pins of the datasheet.
    let ground: BTreeSet<String> = pads_of["gnd"]
        .union(&pads_of["vreg_pgnd"])
        .cloned()
        .collect();
    pads_of.insert("gnd".to_string(), ground);

    let table = parsed(&read("reference/placements.toml"));
    let parts = tables(&table, "part");
    let u1 = parts
        .iter()
        .find(|p| p["reference"].as_str() == Some("U1"))
        .expect("the reference has U1");
    let mut reference: BTreeMap<String, String> = BTreeMap::new(); // pad -> net
    for pad in u1["pads"].as_array().unwrap() {
        let pad = pad.as_table().unwrap();
        if let Some(net) = pad.get("net") {
            reference.insert(
                pad["number"].as_str().unwrap().to_string(),
                net.as_str().unwrap().to_string(),
            );
        }
    }

    let (mut wrong, mut unmatched, mut matched) = (Vec::new(), Vec::new(), 0);
    for (pad, net) in &reference {
        match pads_of.get(&plain(net)) {
            Some(mine) if mine.contains(pad) => matched += 1,
            Some(mine) => wrong.push(format!("pad {pad}: net `{net}` is pad(s) {mine:?} here")),
            None => unmatched.push(format!("{pad} (`{net}`)")),
        }
    }
    assert!(
        wrong.is_empty(),
        "the reference and the part disagree: {wrong:?}; the pads whose net is named like no \
         pin of the part: {unmatched:?}"
    );
    // Not vacuous: the GPIOs, the QSPI, the crystal, SWCLK, RUN, GND and the regulator's pins
    // that Raspberry Pi names like the datasheet.
    assert!(
        matched >= 40,
        "only {matched} pads matched; unmatched: {unmatched:?}"
    );

    // Raspberry Pi's wiring of the supplies, in the part's own pins: the IO supply (and the
    // QSPI, USB, ADC and regulator-input supplies, all from the same 3V3) is on `+3V3`, the
    // core (DVDD and the regulator's feedback) on `+1V1`.
    let net_pads = |net: &str| -> BTreeSet<String> {
        reference
            .iter()
            .filter(|(_, n)| n.as_str() == net)
            .map(|(p, _)| p.clone())
            .collect()
    };
    let union = |names: &[&str]| -> BTreeSet<String> {
        names
            .iter()
            .flat_map(|n| pads_of[&plain(n)].iter().cloned())
            .collect()
    };
    assert_eq!(
        net_pads("+3V3"),
        union(&["IOVDD", "QSPI_IOVDD", "USB_OTP_VDD", "ADC_AVDD", "VREG_VIN"])
    );
    assert_eq!(net_pads("+1V1"), union(&["DVDD", "VREG_FB"]));
}

#[test]
fn a_board_with_the_part_builds_a_schematic_with_a_symbol_drawn_from_the_pin_table() {
    let built = built_bare();
    // The mapping row of the project passed the pinout validation (a failure stops the
    // build), and the symbol is netsmith's own: no KiCad symbol library is named.
    let name = format!(
        "netsmith:{}",
        netsmith_kicad::generate::symbol_name("Rp2350A", Some("QFN-60-EP(7x7)"))
    );
    assert!(
        built.schematic.contains(&format!("(lib_id \"{name}")),
        "no instance of the symbol `{name}`"
    );
    for (datasheet, _, pads) in pins_of(&built) {
        assert!(
            built.schematic.contains(&format!("(name \"{datasheet}\"")),
            "no pin {datasheet}"
        );
        for pad in pads {
            assert!(
                built.schematic.contains(&format!("(number \"{pad}\"")),
                "no pad {pad}"
            );
        }
    }
    let part = built
        .design
        .parts
        .iter()
        .find(|p| p.kind == "Rp2350A")
        .unwrap();
    assert_eq!(part.package.as_deref(), Some("QFN-60-EP(7x7)"));
}

/// The id of `kicad.toml` is a footprint of netsmith's generators: its land pattern is the
/// datasheet's package in pitch, pad count and exposed pad (RP2350 datasheet, Release 8, Figure
/// 147 on printed page 1324: 0.4 mm pitch, 60 leads, exposed pad D2 x E2 3.4 x 3.4 mm), and
/// differs from the recommended footprint (Figure 148, page 1325) in the size of the lead pads.
#[test]
fn the_footprint_has_the_pitch_the_pad_count_and_the_exposed_pad_of_the_datasheets_package() {
    let mapping = Mapping::for_project(Some(&project())).unwrap();
    let mapped = mapping.map("Rp2350A", Some("QFN-60-EP(7x7)")).unwrap();
    assert_eq!(
        mapped.lib_id, None,
        "the symbol is drawn from the pin table"
    );
    assert_eq!(
        mapped.footprint,
        "Package_DFN_QFN:QFN-60-1EP_7x7mm_P0.4mm_EP3.4x3.4mm"
    );
    let footprint = FootprintLibrary::at(testlib::sample_footprints())
        .with_generators()
        .load(&mapped.footprint)
        .unwrap();
    let numbers = footprint.pad_numbers();
    assert_eq!(numbers.iter().filter(|n| !n.is_empty()).count(), 61);
    let pad = |number: &str| {
        footprint
            .pads
            .iter()
            .find(|p| p.number == number)
            .unwrap_or_else(|| panic!("no pad {number}"))
    };
    let (one, two) = (pad("1"), pad("2"));
    assert!(((two.at.y - one.at.y).abs() - 0.4).abs() < 1e-9, "pitch");
    let ep = pad("61");
    assert_eq!((ep.width, ep.height), (3.4, 3.4));
}

#[test]
fn the_catalog_row_names_the_part_and_its_source() {
    let rows = read("catalogs/rp2350a.csv");
    let row = rows.lines().nth(1).expect("a row");
    assert!(row.starts_with("C42411118,RP2350A,"), "{row}");
    assert!(row.contains("https://jlcpcb.com/partdetail/C42411118"));
    assert!(row.contains("2026-11-10"), "the day the page was read");
    // The standard catalogs do not have it, so the part is the project's.
    let target = Target::resolve_from(&project(), None).unwrap();
    assert!(target.catalogs().is_ok());
}

/// U3 (the QSPI flash, `src/flash.nsm`) and X1 (the 12 MHz crystal, `src/crystal.nsm`) of
/// Raspberry Pi's board: the parts are checked against the pads and the net names of the
/// reference table, and a small board of our own builds with both and no KiCad library.
mod flash_and_crystal {
    use super::{parsed, project, read, tables};
    use std::collections::{BTreeMap, BTreeSet};

    use netsmith::project::Target;
    use netsmith::{Libraries, build_with};
    use netsmith_core::PinKind;
    use netsmith_kicad::testlib;
    use netsmith_kicad::{Mapping, SymbolLib};
    use netsmith_pcb::FootprintLibrary;

    /// A small board of our own, never built by the project: both parts with their supply and
    /// the nets of their pins. The names are the datasheets', nothing says they are Raspberry
    /// Pi's.
    const SMALL: &str = r#"
import src::flash::W25Q128Jv
import src::crystal::Abm8272T3
import std::parts::connector::{Header1x2, Header1x4}
import std::sources::Rail

board FlashAndCrystal {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: W25Q128Jv { .mpn &= "W25Q128JVSIQ" }
    // What the board asks of the crystal is met by the row only if the row's values are read
    // as the datasheet states them: 12 MHz (+/- 60 ppm with the stability), 10 pF, 50 ohm.
    .x: Abm8272T3 {
        .mpn &= "ABM8-272-T3"
        .frequency &= 12MHz +/- 100ppm
        .load_capacitance &= 10pF
        .esr_max <= 60ohm
    }
    .spi_a: Header1x4
    .spi_b: Header1x2
    .xtal: Header1x2

    .supply.pos ~ .u.vcc
    .supply.neg ~ .u.gnd
    .supply.neg ~ .x.gnd_2
    .supply.neg ~ .x.gnd_4
    .u.cs ~ .spi_a.p1
    .u.do_io1 ~ .spi_a.p2
    .u.wp_io2 ~ .spi_a.p3
    .u.di_io0 ~ .spi_a.p4
    .u.clk ~ .spi_b.p1
    .u.hold_io3 ~ .spi_b.p2
    .x.xtal_a ~ .xtal.p1
    .x.xtal_b ~ .xtal.p2
}
"#;

    fn built() -> netsmith::Built {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        let mapping = Mapping::for_project(Some(&project())).unwrap_or_else(|e| panic!("{e}"));
        let libraries = Libraries::with_mapping(
            mapping,
            SymbolLib::at(testlib::sample_symbols()),
            FootprintLibrary::at(testlib::sample_footprints()).with_generators(),
        );
        build_with(
            "small.nsm",
            SMALL,
            &target.loader().unwrap(),
            &target.catalogs().unwrap(),
            &libraries,
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .remove(0)
    }

    /// A part's pins as the design has them: the datasheet's name, kind and pads, in order.
    fn pins_of(built: &netsmith::Built, kind: &str) -> Vec<(String, PinKind, Vec<String>)> {
        built
            .design
            .parts
            .iter()
            .find(|p| p.kind == kind)
            .unwrap_or_else(|| panic!("the board has no {kind}"))
            .pins
            .iter()
            .map(|p| (p.datasheet.clone(), p.kind, p.pads.clone()))
            .collect()
    }

    /// The footprint the project's `kicad.toml` gives a kind, loaded from the generators.
    fn footprint_of(kind: &str, package: &str) -> netsmith_pcb::Footprint {
        let mapping = Mapping::for_project(Some(&project())).unwrap();
        let mapped = mapping
            .map(kind, Some(package))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            mapped.lib_id, None,
            "the symbol is drawn from the pin table"
        );
        FootprintLibrary::at(testlib::sample_footprints())
            .with_generators()
            .load(&mapped.footprint)
            .unwrap_or_else(|e| panic!("{}: {e}", mapped.footprint))
    }

    fn pad_numbers(pins: &[(String, PinKind, Vec<String>)]) -> BTreeSet<String> {
        pins.iter().flat_map(|p| p.2.iter().cloned()).collect()
    }

    /// The pads of a footprint of the reference table, as pad number to net name.
    fn reference_nets(reference: &str) -> BTreeMap<String, String> {
        let table = parsed(&read("reference/placements.toml"));
        let parts = tables(&table, "part");
        let part = parts
            .iter()
            .find(|p| p["reference"].as_str() == Some(reference))
            .unwrap_or_else(|| panic!("the reference has no {reference}"));
        part["pads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pad| {
                let pad = pad.as_table().unwrap();
                (
                    pad["number"].as_str().unwrap().to_string(),
                    pad.get("net")
                        .map(|n| n.as_str().unwrap().to_string())
                        .unwrap_or_default(),
                )
            })
            .collect()
    }

    #[test]
    fn the_flash_has_the_eight_pins_of_the_datasheet() {
        let pins = pins_of(&built(), "W25Q128Jv");
        let table: Vec<(&str, PinKind, &str)> = pins
            .iter()
            .map(|(name, kind, pads)| {
                assert_eq!(pads.len(), 1, "{name}: one pad each");
                (name.as_str(), *kind, pads[0].as_str())
            })
            .collect();
        // Winbond W25Q128JV Revision I, section 3.3 (printed page 5): pad, name and I/O.
        assert_eq!(
            table,
            [
                ("CS#", PinKind::Input, "1"),
                ("DO(IO1)", PinKind::Bidirectional, "2"),
                ("WP#(IO2)", PinKind::Bidirectional, "3"),
                ("GND", PinKind::Power, "4"),
                ("DI(IO0)", PinKind::Bidirectional, "5"),
                ("CLK", PinKind::Input, "6"),
                ("HOLD#/RESET#(IO3)", PinKind::Bidirectional, "7"),
                ("VCC", PinKind::Power, "8"),
            ]
        );
    }

    #[test]
    fn the_crystal_has_four_pins_the_crystal_on_pads_1_and_3_and_ground_on_2_and_4() {
        let pins = pins_of(&built(), "Abm8272T3");
        let table: Vec<(&str, &str)> = pins
            .iter()
            .map(|(name, kind, pads)| {
                assert_eq!(*kind, PinKind::Passive, "{name}");
                assert_eq!(pads.len(), 1, "{name}: one pad each");
                (name.as_str(), pads[0].as_str())
            })
            .collect();
        // Abracon drawing # 456603 Revision B, page 4: the top view has GND on pads 2 and 4.
        assert_eq!(
            table,
            [
                ("XTAL_A", "1"),
                ("GND_2", "2"),
                ("XTAL_B", "3"),
                ("GND_4", "4")
            ]
        );
    }

    #[test]
    fn the_footprints_have_the_pads_of_the_pins_and_no_others() {
        let built = built();
        for (kind, package) in [("W25Q128Jv", "SOIC-8-208mil"), ("Abm8272T3", "SMD3225-4P")] {
            let pins = pins_of(&built, kind);
            let footprint = footprint_of(kind, package);
            let pads: BTreeSet<String> = footprint
                .pad_numbers()
                .into_iter()
                .filter(|n| !n.is_empty())
                .map(str::to_string)
                .collect();
            assert_eq!(pads, pad_numbers(&pins), "{kind}");
            assert_eq!(
                footprint.pads.len(),
                pads.len(),
                "{kind}: a pad number is given twice"
            );
        }
        assert_eq!(footprint_of("W25Q128Jv", "SOIC-8-208mil").pads.len(), 8);
        assert_eq!(footprint_of("Abm8272T3", "SMD3225-4P").pads.len(), 4);
    }

    /// The flash's land pattern is the generator's for the datasheet's package (Winbond W25Q128JV
    /// Revision I, section 10.1, printed page 67: pitch 1.27 mm, 7.7 to 8.1 mm over the leads),
    /// and its pads are Raspberry Pi's in place (at +-3.6 mm, 1.27 mm apart) and differ in size
    /// (1.6 x 0.6 mm here, 1.7 x 0.65 mm there; the datasheet draws no land pattern).
    #[test]
    fn the_flash_footprint_has_the_pitch_and_the_span_of_the_datasheets_package() {
        let footprint = footprint_of("W25Q128Jv", "SOIC-8-208mil");
        let pad = |n: &str| footprint.pads.iter().find(|p| p.number == n).unwrap();
        let pitch = |a: &str, b: &str| (pad(a).at.y - pad(b).at.y).abs();
        for (a, b) in [("1", "2"), ("2", "3"), ("3", "4"), ("5", "6"), ("7", "8")] {
            assert!((pitch(a, b) - 1.27).abs() < 1e-9, "pads {a} and {b}");
        }
        // Pad 1 is a top-left pad and pad 8 the top right of the datasheet's top view.
        assert!(pad("1").at.x < 0.0 && pad("8").at.x > 0.0);
        assert!((pad("1").at.y - pad("8").at.y).abs() < 1e-9);
        assert!(pad("1").at.y < pad("4").at.y);
        let outer = |n: &str| pad(n).at.x.abs() + pad(n).width / 2.0;
        let over_the_leads = outer("1") + outer("8");
        assert!(
            over_the_leads > 7.9,
            "the pads reach beyond the leads' tips (7.7 to 8.1 mm): {over_the_leads}"
        );
        // Raspberry Pi's pads are at the same centres and 1.7 x 0.65 mm where these are
        // 1.6 x 0.6 mm.
        let table = parsed(&read("reference/placements.toml"));
        let parts = tables(&table, "part");
        let u3 = parts
            .iter()
            .find(|p| p["reference"].as_str() == Some("U3"))
            .unwrap();
        for theirs in u3["pads"].as_array().unwrap() {
            let theirs = theirs.as_table().unwrap();
            let mine = pad(theirs["number"].as_str().unwrap());
            assert_eq!(theirs["x"].as_float().unwrap(), mine.at.x);
            assert_eq!(theirs["y"].as_float().unwrap(), mine.at.y);
            let size = theirs["size"].as_array().unwrap();
            assert_eq!(
                (size[0].as_float().unwrap(), size[1].as_float().unwrap()),
                (1.7, 0.65)
            );
            assert_eq!((mine.width, mine.height), (1.6, 0.6));
        }
    }

    /// The crystal's pads hold the terminals of the datasheet's drawing: the terminals (page 4,
    /// bottom view) are 0.9 mm wide with 1.2 mm between them across and 0.8 mm high with 0.7 mm
    /// between them along, in a 3.2 x 2.5 mm body: they are 0.60 to 1.50 mm and 0.35 to 1.15 mm
    /// from the middle. The generator's pads are Raspberry Pi's (1.4 x 1.2 mm at +-1.1, +-0.85).
    #[test]
    fn the_crystal_pads_cover_the_terminals_of_the_drawing_and_are_raspberry_pis() {
        let footprint = footprint_of("Abm8272T3", "SMD3225-4P");
        let reference = {
            let table = parsed(&read("reference/placements.toml"));
            let parts = tables(&table, "part");
            let x1 = parts
                .iter()
                .find(|p| p["reference"].as_str() == Some("X1"))
                .unwrap();
            x1["pads"].as_array().unwrap().clone()
        };
        for pad in &footprint.pads {
            let theirs = reference
                .iter()
                .map(|r| r.as_table().unwrap())
                .find(|r| r["number"].as_str() == Some(pad.number.as_str()))
                .unwrap_or_else(|| panic!("no pad {}", pad.number));
            let get = |k: &str| theirs[k].as_float().unwrap();
            let size = theirs["size"].as_array().unwrap();
            assert!((get("x") - pad.at.x).abs() < 1e-9, "pad {} x", pad.number);
            assert!((get("y") - pad.at.y).abs() < 1e-9, "pad {} y", pad.number);
            assert_eq!(
                (size[0].as_float().unwrap(), size[1].as_float().unwrap()),
                (pad.width, pad.height),
                "pad {} size",
                pad.number
            );
            // The terminal of the drawing on this pad lies inside it.
            let (cx, cy) = (pad.at.x.signum() * 1.05, pad.at.y.signum() * 0.75);
            let (hw, hh) = (0.45, 0.4);
            assert!(pad.at.x - pad.width / 2.0 <= cx - hw + 1e-9);
            assert!(cx + hw <= pad.at.x + pad.width / 2.0 + 1e-9);
            assert!(pad.at.y - pad.height / 2.0 <= cy - hh + 1e-9);
            assert!(cy + hh <= pad.at.y + pad.height / 2.0 + 1e-9);
        }
        // Pad 1 is at the bottom left of the top view (y grows down), 3 at the top right.
        let at = |n: &str| footprint.pads.iter().find(|p| p.number == n).unwrap().at;
        assert!(at("1").x < 0.0 && at("1").y > 0.0);
        assert!(at("2").x > 0.0 && at("2").y > 0.0);
        assert!(at("3").x > 0.0 && at("3").y < 0.0);
        assert!(at("4").x < 0.0 && at("4").y < 0.0);
    }

    #[test]
    fn the_flash_agrees_with_the_nets_of_raspberry_pis_u3() {
        let pins = pins_of(&built(), "W25Q128Jv");
        let reference = reference_nets("U3");
        let plain = |net: &str| net.trim_start_matches('/').to_string();
        let mut seen = BTreeSet::new();
        for (name, _, pads) in &pins {
            let net = plain(&reference[&pads[0]]);
            seen.insert(pads[0].clone());
            // The datasheet's IOn pins carry the MCU's QSPI_SDn, and the others their own names.
            let expected = match name.as_str() {
                "CS#" => "FLASH_SS".to_string(), // not QSPI_SS: R10 joins them (README)
                "CLK" => "QSPI_SCLK".to_string(),
                "GND" => "GND".to_string(),
                "VCC" => "+3V3".to_string(),
                io => {
                    let n = io.rsplit_once("(IO").unwrap().1.trim_end_matches(')');
                    format!("QSPI_SD{n}")
                }
            };
            assert_eq!(net, expected, "pad {} ({name})", pads[0]);
        }
        let all: BTreeSet<String> = reference.keys().cloned().collect();
        assert_eq!(seen, all, "every pad of the reference is a pin here");
        assert!(
            reference.values().all(|n| !n.is_empty()),
            "every pad has a net"
        );
    }

    #[test]
    fn the_crystal_agrees_with_the_nets_of_raspberry_pis_x1() {
        let pins = pins_of(&built(), "Abm8272T3");
        let reference = reference_nets("X1");
        let ground = |pad: &str| reference[pad] == "GND";
        for (name, _, pads) in &pins {
            assert_eq!(
                name.starts_with("GND"),
                ground(&pads[0]),
                "pad {} ({name}) and the net `{}`",
                pads[0],
                reference[&pads[0]]
            );
        }
        // The two crystal pads are on two nets of their own, neither ground.
        let crystal: BTreeSet<&String> = pins
            .iter()
            .filter(|p| p.0.starts_with("XTAL"))
            .map(|p| &reference[&p.2[0]])
            .collect();
        assert_eq!(crystal.len(), 2);
        assert!(crystal.iter().all(|n| !n.is_empty() && *n != "GND"));
        assert_eq!(reference["1"], "/XIN");
    }

    #[test]
    fn a_board_with_both_parts_builds_with_symbols_drawn_from_the_pin_tables() {
        let built = built();
        let drawn = |kind: &str, package: &str| {
            format!(
                "netsmith:{}",
                netsmith_kicad::generate::symbol_name(kind, Some(package))
            )
        };
        for (kind, package) in [("W25Q128Jv", "SOIC-8-208mil"), ("Abm8272T3", "SMD3225-4P")] {
            let name = drawn(kind, package);
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
                        "no pad {pad}"
                    );
                }
            }
        }
        let by_kind = |kind: &str| built.sourcing.iter().find(|s| s.kind == kind).unwrap();
        let flash = by_kind("W25Q128Jv");
        assert_eq!(flash.lcsc.as_deref(), Some("C97521"));
        assert_eq!(flash.mpn.as_deref(), Some("W25Q128JVSIQ"));
        assert_eq!(flash.package.as_deref(), Some("SOIC-8-208mil"));
        assert_eq!(format!("{:?}", flash.library.unwrap()), "Basic");
        let crystal = by_kind("Abm8272T3");
        assert_eq!(crystal.lcsc.as_deref(), Some("C20625731"));
        assert_eq!(crystal.mpn.as_deref(), Some("ABM8-272-T3"));
        assert_eq!(format!("{:?}", crystal.library.unwrap()), "Extended");
        // Raspberry Pi's board calls the crystal X1 (the standard crystal is Y).
        let refdes = |kind: &str| {
            built
                .design
                .parts
                .iter()
                .find(|p| p.kind == kind)
                .unwrap()
                .refdes
                .clone()
        };
        assert_eq!(refdes("Abm8272T3"), "X1");
    }

    #[test]
    fn the_catalog_rows_name_the_parts_and_where_they_were_read() {
        for (file, lcsc, mpn, page) in [
            (
                "catalogs/w25q128jv.csv",
                "C97521",
                "W25Q128JVSIQ",
                "https://jlcpcb.com/partdetail/C97521",
            ),
            (
                "catalogs/abm8_272_t3.csv",
                "C20625731",
                "ABM8-272-T3",
                "https://jlcpcb.com/partdetail/C20625731",
            ),
        ] {
            let rows = read(file);
            let row = rows.lines().nth(1).expect("a row");
            assert_eq!(rows.lines().count(), 2, "{file}: one row");
            assert!(row.starts_with(&format!("{lcsc},{mpn},")), "{row}");
            assert!(row.contains(page), "{file}: the page it was read from");
            assert!(
                row.contains("2026-11-10"),
                "{file}: the day the page was read"
            );
            // Price and stock are not recorded.
            let header = rows.lines().next().unwrap();
            assert!(!header.split(',').any(|c| c == "price" || c == "stock"));
        }
        let target = Target::resolve_from(&project(), None).unwrap();
        assert!(target.catalogs().is_ok());
    }
}

// The connectors: J1, J2, J3 and J4 of Raspberry Pi's board.

/// A small board of our own, never built by the project: one of each connector part, so that
/// the parts, the catalogs and the `kicad.toml` rows build together. The power contacts of the
/// receptacle are on a rail, which makes the board's checks and the pinout validation run.
const CONNECTORS: &str = r#"
import src::amphenol_10103594::Amphenol10103594
import src::jst_sm03b_srss_tb::JstSm03bSrssTb
import src::samtec_tsw_118::SamtecTsw11807LD
import std::sources::Rail

board ConnectorsBare {
    .usb_power: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VBUS"
        .neg.net &= "GND"
    }
    .usb: Amphenol10103594 { .mpn &= "10103594-0001LF" }
    .debug: JstSm03bSrssTb { .mpn &= "SM03B-SRSS-TB(LF)(SN)" }
    .low: SamtecTsw11807LD { .mpn &= "TSW-118-07-L-D" }
    .high: SamtecTsw11807LD { .mpn &= "TSW-118-07-L-D" }

    .usb_power.pos ~ .usb.vbus
    .usb_power.neg ~ .usb.gnd
    .usb_power.neg ~ .debug.p2
}
"#;

/// A machine with no KiCad install, and the project's own `kicad.toml` and libraries.
fn libraries_without_kicad() -> Libraries {
    let nowhere = testlib::TempDir::new("rp2350-no-kicad");
    Libraries::for_project_by(Some(&project()), &Search::under(nowhere.path()))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn built_connectors() -> netsmith::Built {
    let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
    build_with(
        "connectors.nsm",
        CONNECTORS,
        &target.loader().unwrap(),
        &target.catalogs().unwrap(),
        &libraries_without_kicad(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0)
}

/// The part of `kind` in the board, with its reference designator.
fn part_of<'a>(built: &'a netsmith::Built, kind: &str) -> Vec<&'a netsmith_core::Part> {
    built
        .design
        .parts
        .iter()
        .filter(|p| p.kind == kind)
        .collect()
}

#[test]
fn each_connector_part_provides_connector_and_has_the_pins_of_its_pads() {
    let built = built_connectors();
    for (kind, count, pads) in [
        ("Amphenol10103594", 1, (1..=11).collect::<Vec<u32>>()),
        ("JstSm03bSrssTb", 1, (1..=5).collect()),
        ("SamtecTsw11807LD", 2, (1..=36).collect()),
    ] {
        let parts = part_of(&built, kind);
        assert_eq!(parts.len(), count, "{kind}");
        for part in parts {
            assert!(part.is_connector(), "{kind} provides Connector");
            assert!(part.refdes.starts_with('J'), "{}: {}", kind, part.refdes);
            let mut numbers: Vec<u32> = part
                .pins
                .iter()
                .flat_map(|p| p.pads.iter().map(|n| n.parse().unwrap()))
                .collect();
            numbers.sort_unstable();
            assert_eq!(numbers, pads, "{kind}: each pad of the part is on one pin");
        }
    }
    // Pins, as the maker's parts have them: the micro-USB's five contacts and the shell (one
    // pin of six pads, the shell being one piece of metal), the header's 36, the JST's three
    // contacts and the two tabs as one pin.
    let pins = |kind: &str| part_of(&built, kind)[0].pins.len();
    assert_eq!(pins("Amphenol10103594"), 6);
    assert_eq!(pins("SamtecTsw11807LD"), 36);
    assert_eq!(pins("JstSm03bSrssTb"), 4);
    let shield = part_of(&built, "Amphenol10103594")[0]
        .pins
        .iter()
        .find(|p| p.datasheet == "SHIELD")
        .unwrap();
    assert_eq!(shield.pads.len(), 6);
    let mount = part_of(&built, "JstSm03bSrssTb")[0]
        .pins
        .iter()
        .find(|p| p.datasheet == "MOUNT")
        .unwrap();
    assert_eq!(mount.pads, ["4", "5"]);
}

#[test]
fn the_connectors_build_with_no_kicad_and_their_footprints_are_the_projects_own() {
    // The build ran `validate_pinout` on each part (a pad of the pin table that the footprint
    // lacks, or a footprint pad that no pin names, stops it) and wrote the schematic.
    let built = built_connectors();
    assert!(!built.schematic.is_empty());
    let mapping = Mapping::for_project(Some(&project())).unwrap();
    for (kind, package, footprint) in [
        (
            "Amphenol10103594",
            "SMD",
            "netsmith_rp2350:USB_Micro-B_Amphenol_10103594-0001LF",
        ),
        (
            "JstSm03bSrssTb",
            "SMD,P=1mm,Surface Mount,Right Angle",
            "netsmith_rp2350:JST_SM03B-SRSS-TB_1x03-2MP_P1.00mm_Horizontal",
        ),
        (
            "SamtecTsw11807LD",
            "TSW-118-07-L-D",
            "netsmith_rp2350:PinHeader_2x18_P2.54mm_Samtec_TSW-118-07-L-D",
        ),
    ] {
        let mapped = mapping.map(kind, Some(package)).unwrap();
        assert_eq!(mapped.footprint, footprint, "{kind}");
        assert_eq!(mapped.lib_id, None, "{kind}: drawn from the pin table");
        let file = footprint.trim_start_matches("netsmith_rp2350:");
        assert!(
            project()
                .join("libs/netsmith_rp2350.pretty")
                .join(format!("{file}.kicad_mod"))
                .is_file(),
            "{file}"
        );
    }
}

#[test]
fn the_header_is_soldered_by_hand_and_has_its_lcsc_number_like_the_others() {
    let built = built_connectors();
    let sourced = |kind: &str| {
        built
            .sourcing
            .iter()
            .filter(|s| s.kind == kind)
            .collect::<Vec<_>>()
    };
    for header in sourced("SamtecTsw11807LD") {
        assert_eq!(header.lcsc.as_deref(), Some("C6887963"));
        assert_eq!(header.assembly, netsmith_design::Assembly::Hand);
    }
    assert_eq!(
        sourced("Amphenol10103594")[0].lcsc.as_deref(),
        Some("C428495")
    );
    assert_eq!(
        sourced("JstSm03bSrssTb")[0].lcsc.as_deref(),
        Some("C160403")
    );
}

/// The shape word of the reference table for a pad.
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

fn load_footprint(libraries: &Libraries, kind: &str, package: &str) -> Footprint {
    let mapped = libraries.mapping.map(kind, Some(package)).unwrap();
    libraries
        .footprints
        .load(&mapped.footprint)
        .unwrap_or_else(|e| panic!("{e}"))
}

/// Every difference between the pads of `footprint` and those of the reference's `reference`
/// footprint: number, centre in the footprint's own frame (0.01 mm), size (0.05 mm), drill
/// (0.01 mm), shape and kind. Empty when they are the same.
fn pad_differences(footprint: &Footprint, table: &toml::Table, reference: &str) -> Vec<String> {
    let part = *tables(table, "part")
        .iter()
        .find(|p| p["reference"].as_str() == Some(reference))
        .unwrap_or_else(|| panic!("no part {reference}"));
    let rotation = number(part, "rotation");
    let wanted = part["pads"].as_array().unwrap();
    let mut differences = Vec::new();
    if footprint.pads.len() != wanted.len() {
        differences.push(format!(
            "{reference}: {} pads, Raspberry Pi's has {}",
            footprint.pads.len(),
            wanted.len()
        ));
    }
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
                && (own.at.x - x).abs() <= 0.01
                && (own.at.y - y).abs() <= 0.01
        });
        let Some((i, own)) = found else {
            differences.push(format!(
                "{reference} pad {n}: none at ({x}, {y}) within 0.01 mm; pads of that number are at {:?}",
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
        let at = format!("{reference} pad {n} at ({x}, {y})");
        let size = pad["size"].as_array().unwrap();
        let as_f = |v: &toml::Value| v.as_float().or_else(|| v.as_integer().map(|i| i as f64));
        let (w, h) = (as_f(&size[0]).unwrap(), as_f(&size[1]).unwrap());
        if (own.width - w).abs() > 0.05 || (own.height - h).abs() > 0.05 {
            differences.push(format!(
                "{at}: size {} x {}, Raspberry Pi's {w} x {h}",
                own.width, own.height
            ));
        }
        let want_shape = pad["shape"].as_str().unwrap();
        if shape_word(own) != want_shape {
            differences.push(format!(
                "{at}: shape {}, Raspberry Pi's {want_shape}",
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
                "{at}: kind {:?}, Raspberry Pi's {want_kind:?}",
                own.kind
            ));
        }
        // The file's pad angle includes the footprint's rotation. A turn of 180 degrees leaves
        // a rectangle, a circle or an oval as it was (Raspberry Pi's J4 has its pads turned so,
        // in a footprint that is not), but not a custom pad.
        let turn = pad.get("angle").map_or(0.0, |_| number(pad, "angle")) - rotation - own.angle;
        let period = if want_shape == "custom" { 360.0 } else { 180.0 };
        let off = turn
            .rem_euclid(period)
            .min(period - turn.rem_euclid(period));
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
                if (d.width - a).abs() <= 0.01 && (d.height - b).abs() <= 0.01 => {}
            (d, w) => differences.push(format!(
                "{at}: drill {:?}, Raspberry Pi's {w:?}",
                d.map(|d| (d.width, d.height))
            )),
        }
    }
    differences
}

#[test]
fn the_pads_of_each_connector_footprint_are_those_of_raspberry_pis_j1_to_j4() {
    let libraries = libraries_without_kicad();
    let table = parsed(&read("reference/placements.toml"));
    let mut differences = Vec::new();
    for (reference, kind, package, pads) in [
        ("J1", "Amphenol10103594", "SMD", 15),
        ("J2", "SamtecTsw11807LD", "TSW-118-07-L-D", 36),
        ("J3", "SamtecTsw11807LD", "TSW-118-07-L-D", 36),
        (
            "J4",
            "JstSm03bSrssTb",
            "SMD,P=1mm,Surface Mount,Right Angle",
            5,
        ),
    ] {
        let footprint = load_footprint(&libraries, kind, package);
        assert_eq!(
            footprint.pads.len(),
            pads,
            "{reference}: pads of the footprint"
        );
        differences.extend(pad_differences(&footprint, &table, reference));
    }
    assert!(
        differences.is_empty(),
        "the footprints differ from Raspberry Pi's pads:\n{}",
        differences.join("\n")
    );
}

#[test]
fn the_comparison_of_pads_says_what_differs() {
    // Not vacuous: a header of netsmith's generator has other pads (1.778 mm, and the pins run
    // down a column), and the comparison names them.
    let table = parsed(&read("reference/placements.toml"));
    let id = netsmith_kicad::footprint_spec::FootprintSpec::parse(
        "gen:PinHeader(rows=2, pins=18, pitch=2.54mm)",
    )
    .unwrap()
    .id();
    let generated = FootprintLibrary::at(testlib::sample_footprints())
        .with_generators()
        .load(&id)
        .unwrap();
    let differences = pad_differences(&generated, &table, "J2");
    assert!(!differences.is_empty());
    assert!(
        differences.iter().any(|d| d.contains("J2 pad 1: none at")),
        "{differences:?}"
    );
    // And the pad's size and drill are compared too: the project's header with the generator's
    // pad and a larger hole.
    let mut wrong = load_footprint(
        &libraries_without_kicad(),
        "SamtecTsw11807LD",
        "TSW-118-07-L-D",
    );
    wrong.pads[0].width = 1.778;
    wrong.pads[1].drill = Some(netsmith_pcb::Drill {
        width: 1.02,
        height: 1.02,
    });
    let differences = pad_differences(&wrong, &table, "J2");
    assert_eq!(differences.len(), 2, "{differences:?}");
    assert!(differences[0].contains("J2 pad 1 at") && differences[0].contains("size 1.778"));
    assert!(differences[1].contains("J2 pad 2 at") && differences[1].contains("drill"));
}

#[test]
fn the_catalog_rows_of_the_receptacle_and_the_jst_header_have_their_numbers_and_sources() {
    for (file, number, mpn, page) in [
        (
            "catalogs/micro_usb.csv",
            "C428495",
            "10103594-0001LF",
            "https://jlcpcb.com/partdetail/C428495",
        ),
        (
            "catalogs/jst_srss.csv",
            "C160403",
            "\"SM03B-SRSS-TB(LF)(SN)\"",
            "https://jlcpcb.com/partdetail/C160403",
        ),
    ] {
        let rows = read(file);
        let row = rows.lines().nth(1).expect("a row");
        assert_eq!(rows.lines().count(), 2, "{file}: one row");
        assert!(row.starts_with(&format!("{number},")), "{row}");
        assert!(
            row.contains(mpn) || row.contains(mpn.trim_matches('"')),
            "{row}"
        );
        assert!(row.contains(page), "{row}");
        assert!(
            row.contains("2026-11-10"),
            "the day the page was read: {row}"
        );
        assert!(row.contains(",Extended,"), "{row}");
    }
    // The header's row: LCSC's page was read, JLCPCB's returned no library type, so none is
    // recorded (an empty cell, not a guess).
    let rows = read("catalogs/samtec_tsw_118.csv");
    let row = rows.lines().nth(1).expect("a row");
    assert_eq!(rows.lines().count(), 2, "one row");
    assert!(
        row.starts_with("C6887963,TSW-118-07-L-D,TSW-118-07-L-D,,"),
        "{row}"
    );
    assert!(
        row.contains("https://jlcpcb.com/partdetail/C6887963"),
        "{row}"
    );
    assert!(row.contains("https://www.lcsc.com/product-detail/C6887963.html"));
    assert!(row.contains("2026-11-10"), "{row}");
    assert!(
        !row.contains(",Extended,") && !row.contains(",Basic,"),
        "{row}"
    );
    let target = Target::resolve_from(&project(), None).unwrap();
    assert!(target.catalogs().is_ok());
}

/// The board itself, `src/main.nsm`: the schematic of Raspberry Pi's RP2350A Minimal circuit
/// (item S6a of epic #308). The proofs: the nets are Raspberry Pi's (read from the committed
/// `.kicad_pcb` as a file, with no KiCad), the header pin map is fixed, and the board builds
/// with no KiCad install.
mod board {
    use super::*;
    use netsmith_netlist::{Options, Outcome, compare, normalize_value, report};

    use crate::common::port::Port;

    /// The port: `reference/parts.toml` pairs each of Raspberry Pi's 43 parts with a member of the
    /// board. The parts that Raspberry Pi has several of on the same two nets (the eight 100 nF
    /// capacitors on +3V3, the three on +1V1) are interchangeable: which of them is called C12 is
    /// arbitrary, and nothing below depends on it. MH1 to MH4 are the mounting holes, which belong
    /// to the physical file (`.hole1` to `.hole4`) and have no nets. Their footprint numbers U2's
    /// tab 4, the datasheet and the generator's footprint 2: the same pin on the same net (README,
    /// "Differs from Raspberry Pi's board").
    const PORT: Port = Port {
        folder: "rp2350_minimal",
        board: Some(REFERENCE),
        original: "Raspberry Pi",
        swapped: &[],
        renumbered: &[("U2", "4", "2")],
        no_nets: &HOLES,
        own: &[],
    };

    const HOLES: [&str; 4] = ["MH1", "MH2", "MH3", "MH4"];

    /// The mounting holes of the physical file, in the order of `HOLES`.
    const HOLE_PATHS: [&str; 4] = ["hole1", "hole2", "hole3", "hole4"];

    fn built() -> netsmith::Built {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        let source = &target.files[0];
        let text = fs::read_to_string(&source.path).unwrap();
        build_with(
            &source.name,
            &text,
            &target.loader().unwrap(),
            &target.catalogs().unwrap(),
            &libraries_without_kicad(),
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .remove(0)
    }

    /// Every part of the board is one of Raspberry Pi's, once, and every part of theirs is in the
    /// board: the four mounting holes are the physical file's `.hole1` to `.hole4`, in that
    /// order, and are on no net.
    #[test]
    fn the_board_builds_with_no_kicad_and_has_the_parts_of_raspberry_pis_board() {
        let built = built();
        PORT.assert_has_the_parts(&built, 43, 43);
        let parts = PORT.parts();
        let reference = PORT.reference();
        for (hole, path) in HOLES.iter().zip(HOLE_PATHS) {
            assert_eq!(parts.reference_of(path), Some(*hole), "{path}");
            let c = reference.find(hole).unwrap();
            assert!(
                reference.components[c].pins.is_empty(),
                "{hole} is on a net"
            );
        }
    }

    /// The proof of item S7: for every pad of every part of Raspberry Pi's board, the set of
    /// the other pads on its net is the same as on the board's. It is made from the committed
    /// `.kicad_pcb` as a file (`Netlist::from_kicad_pcb`, netsmith's own s-expression reader) and
    /// the design's nets, with no `kicad-cli`: `netsmith compare` reads the same file the same
    /// way, but a `.nsm` side and a `.kicad_pcb` side differ in the footprints' names (theirs are
    /// `RPI_COMPILED:generic_capc1005x50`), so the test compares the wiring alone.
    #[test]
    fn every_pad_has_the_same_pads_on_its_net_as_in_raspberry_pis_board() {
        PORT.assert_same_pads_on_nets(&built(), 200);
    }

    /// The same, by `netsmith-netlist`'s `compare` (what `netsmith compare` runs), with the
    /// designators paired by `reference/parts.toml` and the value and footprint left out of the
    /// comparison (the mounting holes are no part of the circuit: they have no pin and no net):
    /// the verdict is a match, and the report names no net.
    #[test]
    fn netsmith_compare_says_the_nets_are_the_same() {
        let built = built();
        PORT.assert_compare_matches(&built);
        // Without the pairs, the wiring alone still makes the two the same circuit.
        let free = compare(
            &PORT.wiring(&built),
            &PORT.reference_wiring(),
            &Options::default(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(free, Outcome::Match(_)), "{}", report(&free));
    }

    /// The comparison would see a wrong net: it is the comparison above that runs, on a board
    /// whose J2 pad 4 (GPIO0 in Raspberry Pi's board) is said to be on the net of pad 6, and its
    /// answer is the pad.
    #[test]
    fn the_comparison_sees_a_pad_on_the_wrong_net() {
        PORT.assert_sees_a_wrong_net(&built(), ("J2", "4"), ("J2", "6"));
    }

    /// The values of the resistors and capacitors are Raspberry Pi's (`4k7` and `4.7k` are one
    /// value), and the package is the one their footprint says: `1005` (metric) is 0402 and
    /// `2012` is 0805.
    #[test]
    fn the_resistors_and_capacitors_have_raspberry_pis_values_and_packages() {
        let built = built();
        let reference = PORT.reference();
        let mut wrong = Vec::new();
        for (path, name) in PORT
            .parts()
            .iter()
            .filter(|(_, r)| r.starts_with('R') || r.starts_with('C'))
        {
            let part = built.design.parts.iter().find(|p| p.path == path).unwrap();
            let theirs = &reference.components[reference.find(name).unwrap()];
            if normalize_value(&part.value, true) != normalize_value(&theirs.value, true) {
                wrong.push(format!(
                    "{name}: value {} against {}",
                    part.value, theirs.value
                ));
            }
            let package = if theirs.footprint.contains("1005") {
                "0402"
            } else if theirs.footprint.contains("2012") {
                "0805"
            } else {
                panic!("{name}: footprint {}", theirs.footprint)
            };
            if part.package.as_deref() != Some(package) {
                wrong.push(format!(
                    "{name}: package {:?} against {package}",
                    part.package
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// What each header pad carries on Raspberry Pi's board, read from its nets: a power net, a
    /// ground, or the MCU pin that the net reaches (`GPIO4`, `RUN`, `SWCLK`).
    fn reference_signals() -> BTreeMap<(String, String), String> {
        let reference = PORT.reference();
        let mut out = BTreeMap::new();
        for header in ["J2", "J3"] {
            let c = reference.find(header).unwrap();
            for (pad, net) in &reference.components[c].pins {
                let name = reference.nets[*net]
                    .name
                    .trim_start_matches('/')
                    .to_string();
                out.insert((header.to_string(), pad.clone()), name);
            }
        }
        out
    }

    /// The header pin map is Raspberry Pi's, pad by pad, and it is made of named pins: the
    /// MCU's pin that a header pad reaches is the one on the net of that pad in the design.
    #[test]
    fn the_header_pins_carry_raspberry_pis_signals() {
        let built = built();
        let design = &built.design;
        let mcu = design.parts.iter().find(|p| p.path == "mcu").unwrap();
        let mut wrong = Vec::new();
        let expected = reference_signals();
        assert_eq!(expected.len(), 72, "two headers of 36 pads, each on a net");
        for ((header, pad), signal) in &expected {
            let path = if header == "J2" {
                "header_low"
            } else {
                "header_high"
            };
            let part = design.parts.iter().find(|p| p.path == path).unwrap();
            let net = design
                .nets
                .iter()
                .find(|n| {
                    n.pins
                        .iter()
                        .any(|p| p.refdes == part.refdes && p.pin == *pad)
                })
                .unwrap_or_else(|| panic!("{header}.{pad} is on no net"));
            let found = match signal.as_str() {
                "GND" | "+3V3" => net.name.clone(),
                _ => {
                    let pin = net
                        .pins
                        .iter()
                        .filter(|p| p.refdes == mcu.refdes)
                        .filter_map(|p| mcu.pins.iter().find(|q| q.pads.contains(&p.pin)))
                        .map(|q| q.datasheet.clone())
                        .next()
                        .unwrap_or_default();
                    // The datasheet calls the swd data pin SWDIO and Raspberry Pi's net SWD.
                    if pin == "SWDIO" { "SWD".into() } else { pin }
                }
            };
            if found != *signal {
                wrong.push(format!(
                    "{header}.{pad}: {found}, Raspberry Pi's is {signal}"
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// The pin optimizer (ADR 0101) moves only a pin that a request got. The board has no
    /// request, so the optimizer has nothing it may move: its input, the check's `spaces`, is
    /// empty, the pin file has no entry, and `improve` has no slot to try. A board whose
    /// header pad is joined to a request is the opposite, so the test would see a mapping that
    /// the optimizer could move.
    #[test]
    fn the_optimizer_cannot_move_a_header_pad() {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        let source = &target.files[0];
        let text = fs::read_to_string(&source.path).unwrap();
        let (loader, catalogs) = (target.loader().unwrap(), target.catalogs().unwrap());
        let libraries = libraries_without_kicad();
        let outcome =
            netsmith::outcome_with(&source.name, &text, &loader, &catalogs, Some(&libraries))
                .unwrap_or_else(|p| panic!("{}", p[0].rendered));
        let requests = |outcome: &netsmith::Outcome| {
            outcome.pins.spaces.get("Rp2350Minimal").map_or(0, |parts| {
                parts.iter().map(|s| s.requests.len()).sum::<usize>()
            })
        };
        assert_eq!(requests(&outcome), 0, "the board asks for a pin somewhere");
        assert!(
            outcome
                .pins
                .file
                .boards
                .get("Rp2350Minimal")
                .is_none_or(|e| e.is_empty())
        );
        assert!(outcome.pins.unserved.values().all(|u| u.is_empty()));
        assert!(
            !project().join("src/main.pins.toml").exists(),
            "a pin file would keep an answer for a request"
        );
        // The control: the same board with the header's GPIO4 pad joined to a request is one
        // the optimizer may move.
        let asked = text
            .replace(
                "    .header_low.p12 ~ .mcu.gpio4",
                "    .pin_for_header: .mcu.GPIO\n    .header_low.p12 ~ .pin_for_header",
            )
            .replace(
                ".mcu: Rp2350A { .mpn &= \"RP2350A\" }",
                ".mcu: Rp2350A { .mpn &= \"RP2350A\"; .package &= \"QFN-60-EP(7x7)\" }",
            );
        assert_ne!(asked, text);
        let moved =
            netsmith::outcome_with(&source.name, &asked, &loader, &catalogs, Some(&libraries))
                .unwrap_or_else(|p| panic!("{}", p[0].rendered));
        assert_eq!(requests(&moved), 1);
    }

    /// GPIO26 to GPIO29 are GPIO or an ADC input, and the board chooses `GPIO` on all four: the
    /// part marks `GPIO` as a class (`fn GPIO class`), so one function may be chosen on four
    /// pins (a function that no row marks is refused on the second pin: `netsmith-check`'s
    /// `class_marker` tests).
    #[test]
    fn the_four_adc_pins_all_choose_gpio_because_the_part_marks_it_a_class() {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        let source = &target.files[0];
        let text = fs::read_to_string(&source.path).unwrap();
        for n in 26..=29 {
            assert!(
                text.contains(&format!(".mcu.gpio{n}.function &= \"GPIO\"")),
                "gpio{n}"
            );
        }
        let part = read("src/rp2350a.nsm");
        let rows: Vec<&str> = part
            .lines()
            .filter(|l| l.trim_start().starts_with(".gpio"))
            .collect();
        assert_eq!(rows.len(), 30);
        assert!(
            rows.iter().all(|l| l.contains("fn GPIO class")),
            "every GPIO row"
        );
        let (loader, catalogs) = (target.loader().unwrap(), target.catalogs().unwrap());
        let libraries = libraries_without_kicad();
        netsmith::outcome_with(&source.name, &text, &loader, &catalogs, Some(&libraries))
            .unwrap_or_else(|p| panic!("{}", p[0].rendered));
    }

    #[test]
    fn the_electrical_rules_check_of_kicad_passes_on_the_schematic() {
        let built = built();
        let Some(found) = crate::erc::erc_built(&built) else {
            return;
        };
        assert_eq!(
            crate::erc::with_the_machines_libraries(found),
            Vec::<String>::new()
        );
    }
}
