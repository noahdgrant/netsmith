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

    use super::{parsed, project, read, tables};

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
        let parts = tables(&table, "part");
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
            // The footprint is the same as Raspberry Pi's, pad for pad.
            let part = parts
                .iter()
                .find(|p| p["reference"].as_str() == Some(reference))
                .unwrap();
            for pad in part["pads"].as_array().unwrap() {
                let pad = pad.as_table().unwrap();
                let number = pad["number"].as_str().unwrap();
                let mine = footprint.pads_numbered(number).next().unwrap();
                let size: Vec<f64> = pad["size"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_float().unwrap())
                    .collect();
                // Their part is turned by -90 or 180 degrees, so the pad's own size is given
                // in the turned frame and the sizes are compared as a set.
                let mut theirs = size.clone();
                let mut ours = vec![mine.width, mine.height];
                theirs.sort_by(f64::total_cmp);
                ours.sort_by(f64::total_cmp);
                assert_eq!(theirs, ours, "{reference} pad {number}");
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
