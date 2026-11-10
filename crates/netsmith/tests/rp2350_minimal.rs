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

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use netsmith::project::Target;
use netsmith::reference_table::placements_toml;
use netsmith::{Libraries, build_with, outcome_with};
use netsmith_core::PinKind;
use netsmith_kicad::testlib;
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_pcb::FootprintLibrary;

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
