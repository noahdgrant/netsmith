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
    .low: SamtecTsw11807LD
    .high: SamtecTsw11807LD

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
fn the_header_is_soldered_by_hand_and_has_no_lcsc_number_the_others_have_theirs() {
    let built = built_connectors();
    let sourced = |kind: &str| {
        built
            .sourcing
            .iter()
            .filter(|s| s.kind == kind)
            .collect::<Vec<_>>()
    };
    for header in sourced("SamtecTsw11807LD") {
        assert_eq!(header.lcsc, None);
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
    // The header has no JLCPCB part: no catalog and no number.
    assert!(!project().join("catalogs/samtec_tsw_118.csv").exists());
    let target = Target::resolve_from(&project(), None).unwrap();
    assert!(target.catalogs().is_ok());
}
