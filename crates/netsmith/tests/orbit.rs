//! `examples/orbit/`: the home of the port of ORBIT, an STM32F030 board with USB, battery
//! charging and a buck-boost regulator by the author of
//! <https://github.com/mohamedboubaker/Battery-Powered-STM32-Board-With-USB> (MIT). This is the
//! scaffold: the licence, the placeholder board, and the reference table
//! (`reference/placements.toml`) that a parity checker will read, with the list of parts that
//! must match (`reference/must_match.toml`).
//!
//! ORBIT's files are KiCad 7's (format 20221018), the second kind of file the reference reader
//! met after the RP2350A Minimal's KiCad 10. Four things in them were new: the reference of a
//! footprint is an `(fp_text reference ...)` and not a `(property "Reference" ...)`; the mounting
//! holes are plated `MountingHole_*_Pad_Via` footprints, with no `np_thru_hole` pad; the page
//! has no title block; and the board's must-match list is its own. A test per fix, on a small
//! board in KiCad 7's spelling and on the real file.

use std::fs;
use std::path::PathBuf;

use netsmith::outcome_with;
use netsmith::project::Target;
use netsmith::reference_table::{MustMatch, placements_toml, placements_toml_with, read_board};
use netsmith_netlist::Netlist;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/orbit")
}

const REFERENCE: &str = "ORBIT.kicad_pcb";

fn read(path: &str) -> String {
    fs::read_to_string(project().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A board of KiCad 7's spelling (format 20221018): a `(net N "name")` table, the reference in
/// an `fp_text`, a plated mounting hole with a ring of vias numbered like its pad, an outline
/// of four lines in KiCad 7's `(stroke ...)` form, and no title block.
const KICAD7: &str = r#"(kicad_pcb (version 20221018) (generator pcbnew)
  (net 0 "")
  (net 1 "GND")
  (net 2 "SWDIO")
  (footprint "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical" (layer "F.Cu")
    (tstamp 01bd2248-19cb-464a-abbd-b864ad6aeb60)
    (at 12.5 8)
    (attr through_hole)
    (fp_text reference "J1" (at 0 -2) (layer "F.SilkS")
      (effects (font (size 1 1) (thickness 0.15))) (tstamp 48bb38de-9248-41d0-abae-6acc0059361e))
    (fp_text value "West" (at 0 6) (layer "F.Fab")
      (effects (font (size 1 1) (thickness 0.15))) (tstamp 32e56cef-7781-43ca-b690-ea726c51562c))
    (pad "1" thru_hole rect (at 0 0) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask") (net 1 "GND"))
    (pad "2" thru_hole oval (at 0 2.54) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask") (net 2 "SWDIO"))
  )
  (footprint "MountingHole:MountingHole_2.5mm_Pad_Via" (layer "F.Cu")
    (at 20 5)
    (attr exclude_from_pos_files)
    (fp_text reference "H1" (at 0 -3.5) (layer "F.SilkS") hide (effects (font (size 1 1) (thickness 0.15))))
    (fp_text value "MountingHole" (at 0 3.5) (layer "F.Fab") (effects (font (size 1 1) (thickness 0.15))))
    (pad "1" thru_hole circle (at -1.875 0) (size 0.8 0.8) (drill 0.5) (layers "*.Cu" "*.Mask"))
    (pad "1" thru_hole circle (at 0 0) (size 5 5) (drill 2.5) (layers "*.Cu" "*.Mask"))
    (pad "1" thru_hole circle (at 1.875 0) (size 0.8 0.8) (drill 0.5) (layers "*.Cu" "*.Mask"))
  )
  (footprint "Connector_JST:Plug" (layer "F.Cu")
    (at 25 5)
    (fp_text reference "BAT1" (at 0 0) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))))
    (pad "1" thru_hole circle (at 0 0) (size 3 3) (drill 1.5) (layers "*.Cu") (net 1 "GND"))
  )
  (footprint "LOGO" (layer "F.Cu")
    (at 15 5)
    (attr board_only exclude_from_pos_files exclude_from_bom)
    (fp_text reference "O" (at 0 0) (layer "F.SilkS") hide (effects (font (size 1 1) (thickness 0.15))))
  )
  (gr_line (start 10 0) (end 30 0)
    (stroke (width 0.1) (type default)) (layer "Edge.Cuts") (tstamp 0c9fa499-9e75-4411-8cc5-d4a825a84cd9))
  (gr_line (start 30 0) (end 30 10)
    (stroke (width 0.1) (type default)) (layer "Edge.Cuts") (tstamp 4ffe455d-911c-4d71-8091-40ee7abce61a))
  (gr_line (start 30 10) (end 10 10)
    (stroke (width 0.1) (type default)) (layer "Edge.Cuts") (tstamp 60d5dcc1-a9d0-4c07-af41-7267cecc2505))
  (gr_line (start 10 10) (end 10 0)
    (stroke (width 0.1) (type default)) (layer "Edge.Cuts") (tstamp 6891b7b8-e345-42ed-a72e-919e2a05c56f))
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

fn must_match(table: &toml::Table) -> Vec<&str> {
    tables(table, "part")
        .into_iter()
        .filter(|p| p["must_match"].as_bool() == Some(true))
        .map(|p| p["reference"].as_str().unwrap())
        .collect()
}

#[test]
fn a_kicad_7_footprints_reference_is_its_fp_text() {
    // Before the fix: "the footprint Connector_PinHeader_...: has no Reference property".
    let text = placements_toml(KICAD7, "k7.kicad_pcb").unwrap_or_else(|e| panic!("{e}"));
    let table = parsed(&text);
    assert_eq!(table["source"]["version"].as_str(), Some("20221018"));
    assert!(!table["source"].as_table().unwrap().contains_key("title"));
    let refs: Vec<_> = tables(&table, "part")
        .iter()
        .map(|p| p["reference"].as_str().unwrap())
        .collect();
    assert_eq!(refs, ["BAT1", "H1", "J1", "O"]);
    let j1 = *tables(&table, "part")
        .iter()
        .find(|p| p["reference"].as_str() == Some("J1"))
        .unwrap();
    let pads = j1["pads"].as_array().unwrap();
    assert_eq!(pads[1]["net"].as_str(), Some("SWDIO"), "(net 2 \"SWDIO\")");
    // The outline is read from KiCad 7's `(stroke (width ..) ..) (layer "Edge.Cuts")` lines.
    assert_eq!(number(table["outline"].as_table().unwrap(), "width"), 20.0);
}

#[test]
fn a_kicad_7_board_gives_its_netlist_with_the_references_and_nets_of_its_pads() {
    // Before the fix: no footprint had a reference, so the netlist had no component at all.
    let netlist = Netlist::from_kicad_pcb(KICAD7).unwrap_or_else(|e| panic!("{e}"));
    let refs: Vec<_> = netlist
        .components
        .iter()
        .map(|c| c.refdes.as_str())
        .collect();
    assert_eq!(refs, ["J1", "H1", "BAT1"], "the logo is board_only");
    assert_eq!(netlist.components[0].value, "West");
    let swdio = netlist.nets.iter().find(|n| n.name == "SWDIO").unwrap();
    assert_eq!(swdio.pins, [(0, "2".to_string())]);
    let gnd = netlist.nets.iter().find(|n| n.name == "GND").unwrap();
    assert_eq!(gnd.pins.len(), 2, "J1.1 and BAT1.1");
}

#[test]
fn a_mounting_hole_footprint_with_a_plated_pad_is_one_plated_hole() {
    // Before the fix: the table had no hole at all for this board.
    let table = parsed(&placements_toml(KICAD7, "k7.kicad_pcb").unwrap());
    let holes = tables(&table, "hole");
    assert_eq!(holes.len(), 1, "the ring of vias is not holes: {holes:?}");
    assert_eq!(holes[0]["reference"].as_str(), Some("H1"));
    assert_eq!(
        (number(holes[0], "x"), number(holes[0], "y")),
        (20.0, 5.0),
        "the place of the pad with the largest drill"
    );
    assert_eq!(number(holes[0], "drill"), 2.5);
    assert_eq!(holes[0]["plated"].as_bool(), Some(true));
    // A plated pad of another footprint (BAT1's, J1's) is never a hole.
    let board = read_board(KICAD7, "k7.kicad_pcb").unwrap();
    assert_eq!(board.holes.len(), 1);
    assert!(board.holes[0].plated);
}

#[test]
fn a_non_plated_hole_has_no_plated_key_and_the_default_table_is_unchanged() {
    let mut pcb = KICAD7.replace(
        "MountingHole:MountingHole_2.5mm_Pad_Via",
        "MountingHole:MountingHole_2.5mm",
    );
    pcb = pcb.replace(
        r#"(pad "1" thru_hole circle (at 0 0) (size 5 5) (drill 2.5) (layers "*.Cu" "*.Mask"))"#,
        r#"(pad "" np_thru_hole circle (at 0 0) (size 2.5 2.5) (drill 2.5) (layers "*.Cu"))"#,
    );
    let table = placements_toml(&pcb, "k7.kicad_pcb").unwrap();
    let holes = tables(&parsed(&table), "hole").len();
    // The np pad is the hole, and the plated vias of the ring are left alone.
    assert_eq!(holes, 1);
    assert!(
        !table.contains("plated = "),
        "the RP2350A Minimal's table has no `plated`"
    );
}

#[test]
fn the_must_match_list_marks_the_listed_parts_and_the_default_list_is_the_rp2350s() {
    let listed = MustMatch::from_toml("parts = [\"J1\", \"O\"]").unwrap();
    let table = placements_toml_with(KICAD7, "k7.kicad_pcb", &listed).unwrap();
    let table = parsed(&table);
    assert_eq!(must_match(&table), ["J1", "O"]);
    assert!(
        tables(&table, "hole")
            .iter()
            .all(|h| h["must_match"].as_bool() == Some(true)),
        "every hole is must-match"
    );
    // The default list is Raspberry Pi's (J1 to J4, SW1, SW2, MH1 to MH4): of this board, J1
    // (and it is not held against the board: J2 is not there).
    let default = parsed(&placements_toml(KICAD7, "k7.kicad_pcb").unwrap());
    assert_eq!(must_match(&default), ["J1"]);
    // The header's last paragraph is the old one for the default and says "list" for a list.
    assert!(
        placements_toml(KICAD7, "k")
            .unwrap()
            .contains("J1 to J4, SW1, SW2")
    );
    assert!(
        placements_toml_with(KICAD7, "k", &listed)
            .unwrap()
            .contains("must-match list")
    );
}

#[test]
fn a_bad_must_match_list_is_an_error_that_names_the_fault() {
    let wrong = |text: &str| MustMatch::from_toml(text).unwrap_err();
    assert!(wrong("parts = []").contains("empty"));
    assert!(wrong("parts = [\"J1\", \"J1\"]").contains("J1 twice"));
    assert!(wrong("part = [\"J1\"]").contains("the key part"));
    assert!(wrong("name = 1").contains("the key name"));
    assert!(wrong("parts = [1]").contains("not a text"));
    assert!(wrong("x").contains("must-match list"));
    // A designator the board lacks must not pass silently: a typo drops a part.
    let typo = MustMatch::from_toml("parts = [\"J1\", \"J9\"]").unwrap();
    let e = placements_toml_with(KICAD7, "k7.kicad_pcb", &typo).unwrap_err();
    assert!(e.contains("J9") && e.contains("does not have"), "{e}");
}

#[test]
fn the_committed_table_is_what_the_program_prints_for_the_committed_board_file() {
    let must =
        MustMatch::from_toml(&read("reference/must_match.toml")).unwrap_or_else(|e| panic!("{e}"));
    let printed = placements_toml_with(&read(&format!("reference/{REFERENCE}")), REFERENCE, &must)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        printed == read("reference/placements.toml"),
        "reference/placements.toml is not what the program prints: run `cargo run -p netsmith \
         --example reference_table -- --must-match examples/orbit/reference/must_match.toml \
         examples/orbit/reference/{REFERENCE} > examples/orbit/reference/placements.toml`"
    );
}

#[test]
fn the_table_holds_the_facts_of_orbits_board() {
    let table = parsed(&read("reference/placements.toml"));
    assert_eq!(table["source"]["footprints"].as_integer(), Some(53));
    assert_eq!(table["source"]["version"].as_str(), Some("20221018"));

    let outline = table["outline"].as_table().unwrap();
    for (key, value) in [
        ("min_x", 124.0),
        ("min_y", 70.0),
        ("max_x", 174.0),
        ("max_y", 120.0),
        ("width", 50.0),
        ("height", 50.0),
        ("corner_radius", 5.0),
    ] {
        assert!((number(outline, key) - value).abs() < 1e-3, "{key}");
    }
    assert_eq!(outline["lines"].as_integer(), Some(4));
    assert_eq!(outline["arcs"].as_integer(), Some(4));

    // Four plated mounting holes at the corners (H3 is the M2 one, 2.2 mm), and the two
    // locating pegs of the mini USB connector.
    let holes: Vec<_> = tables(&table, "hole")
        .into_iter()
        .map(|h| {
            (
                h["reference"].as_str().unwrap(),
                number(h, "x"),
                number(h, "y"),
                number(h, "drill"),
                h.get("plated").and_then(toml::Value::as_bool) == Some(true),
            )
        })
        .collect();
    assert_eq!(
        holes,
        [
            ("H1", 170.0, 74.0, 2.5, true),
            ("H2", 128.0, 74.0, 2.5, true),
            ("H3", 128.0, 116.0, 2.2, true),
            ("H4", 170.0, 116.0, 2.5, true),
            ("USB1", 151.2, 74.9, 0.9, false),
            ("USB1", 146.8, 74.9, 0.9, false),
        ]
    );

    let parts = tables(&table, "part");
    assert_eq!(parts.len(), 53);
    let find = |reference: &str| {
        *parts
            .iter()
            .find(|p| p["reference"].as_str() == Some(reference))
            .unwrap_or_else(|| panic!("no part {reference}"))
    };
    for (reference, x, y, rotation, pads) in [
        ("USB1", 149.0, 75.0, 180.0, 11),
        ("BAT1", 161.0, 75.5, 0.0, 4),
        ("J1", 126.5, 92.0, 0.0, 8),
        ("J2", 171.5, 92.0, 0.0, 8),
        ("SW1", 161.5, 114.5, 0.0, 4),
        ("SW2", 136.5, 114.5, 180.0, 4),
        ("BOOT0", 127.125, 82.5, 180.0, 5),
        ("PWR1", 170.875, 82.5, 0.0, 5),
        ("U16", 149.0, 97.5, 90.0, 48),
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

    // The headers' pin-to-signal mapping is the net of each pad.
    let signals = |reference: &str| -> Vec<String> {
        find(reference)["pads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["net"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        signals("J1"),
        [
            "GND", "SWDIO", "SWDCLK", "NRST", "PC14", "PA2", "PA3", "+3.3V"
        ]
    );
    assert_eq!(
        signals("J2"),
        ["GND", "PB11", "PB10", "PB2", "PB1", "PB0", "PA7", "PA6"]
    );

    // The owner's rule: the connectors, the switches and buttons and the mounting holes must
    // match; the ICs, the crystal, the inductor, the LEDs and the passives do not.
    assert_eq!(
        must_match(&table),
        [
            "BAT1", "BOOT0", "H1", "H2", "H3", "H4", "J1", "J2", "PWR1", "SW1", "SW2", "USB1"
        ]
    );
}

#[test]
fn orbits_board_file_gives_a_netlist_with_every_footprint_but_the_logo() {
    // 53 footprints, the logo (`board_only`) left out: before the `fp_text` fix, none.
    let netlist = Netlist::from_kicad_pcb(&read(&format!("reference/{REFERENCE}")))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(netlist.components.len(), 52);
    let index = |refdes: &str| {
        netlist
            .components
            .iter()
            .position(|c| c.refdes == refdes)
            .unwrap_or_else(|| panic!("no {refdes}"))
    };
    let net_name = |refdes: &str, pad: &str| {
        let net = netlist.net_of(index(refdes), pad).expect("pad has no net");
        netlist.nets[net].name.as_str()
    };
    assert_eq!(net_name("J1", "2"), "SWDIO");
    assert_eq!(net_name("J2", "6"), "PB0");
    assert_eq!(net_name("USB1", "1"), "VBUS");
    assert_eq!(net_name("SW1", "1"), "NRST");
    assert_eq!(netlist.components[index("U16")].value, "STM32F030C8Tx");
}

#[test]
fn the_license_is_the_mit_text_of_orbits_author_and_keeps_its_notice() {
    let license = read("LICENSE");
    assert!(license.starts_with("MIT License\n\nCopyright (c) 2023 mohamedboubaker\n"));
    assert!(license.contains("Permission is hereby granted, free of charge"));
    assert!(license.contains(
        "The above copyright notice and this permission notice shall be included in all\n\
         copies or substantial portions of the Software."
    ));
    let readme = read("README.md");
    assert!(
        readme.contains("https://github.com/mohamedboubaker/Battery-Powered-STM32-Board-With-USB")
    );
    assert!(readme.contains("a33a81c9199174a9116dfb75a76a126d2341667e"));
    assert!(readme.contains("MIT"));
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
    assert_eq!(names, ["Orbit"]);
}

/// The five ICs of ORBIT as parts: the charger (U1), the buck-boost converter (U15), the
/// microcontroller (U16), the EEPROM (U17) and the USB-UART bridge (U22), each with its pin
/// table, footprint and catalog row, and what the reference board says about them.
mod ics {
    use super::{parsed, project, read, tables};
    use std::collections::{BTreeMap, BTreeSet};

    use netsmith::project::Target;
    use netsmith::{Libraries, build_with};
    use netsmith_core::PinKind;
    use netsmith_kicad::{Mapping, SymbolLib, testlib};
    use netsmith_pcb::{Footprint, FootprintLibrary};

    /// A small board of our own, never built by the project: the five parts with their
    /// supplies and the nets of their pins. The names are the datasheets', nothing says they
    /// are ORBIT's.
    const FIXTURE: &str = r#"
import src::mcp73831::Mcp73831
import src::tps63802::Tps63802
import src::stm32f030c8::Stm32F030C8
import src::at24c128c::At24C128C
import src::cp2102::Cp2102
import std::parts::connector::{Header1x2, Header1x4}
import std::parts::passive::Resistor
import std::sources::Rail

board Ics {
    .vbus: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VBUS"
        .neg.net &= "GND"
    }
    .battery: Rail {
        .voltage &= 3.0V..4.2V
        .pos.net &= "BATT"
        .neg.net &= "GND"
    }
    .rail: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }

    // The charger: 10 kohm sets about 100 mA.
    .charger: Mcp73831 { .mpn &= "MCP73831T-2ACI/OT" }
    .r_prog: Resistor { .resistance &= 10kohm +/- 1% }
    .charger.rprog == .r_prog.resistance
    .vbus.pos ~ .charger.vdd
    .battery.pos ~ .charger.vbat
    .vbus.neg ~ .charger.vss
    .charger.prog ~ .r_prog.a
    .r_prog.b ~ .vbus.neg
    .status: Header1x2
    .charger.stat ~ .status.p1
    .vbus.neg ~ .status.p2

    // The converter: the inductor and the output are on headers.
    .converter: Tps63802 { .mpn &= "TPS63802DLAR" }
    .battery.pos ~ .converter.vin
    .battery.pos ~ .converter.en
    .battery.neg ~ .converter.gnd
    .battery.neg ~ .converter.agnd
    .battery.neg ~ .converter.mode
    .rail.pos ~ .converter.vout
    .converter.fb ~ .rail.pos
    .coil: Header1x2
    .converter.l1 ~ .coil.p1
    .converter.l2 ~ .coil.p2
    .good: Header1x2
    .converter.pg ~ .good.p1
    .rail.neg ~ .good.p2

    // The microcontroller, the EEPROM and the bridge.
    .mcu: Stm32F030C8 { .mpn &= "STM32F030C8T6" }
    .eeprom: At24C128C { .mpn &= "AT24C128C-SSHM-T" }
    .bridge: Cp2102 { .mpn &= "CP2102-GMR" }
    .rail.pos ~ .mcu.vdd
    .rail.pos ~ .mcu.vdda
    .rail.neg ~ .mcu.vss
    .rail.neg ~ .mcu.vssa
    .boot: Header1x2
    .mcu.boot0 ~ .boot.p1
    .rail.neg ~ .boot.p2

    .rail.pos ~ .eeprom.vcc
    .rail.neg ~ .eeprom.gnd
    .rail.pos ~ .eeprom.a0
    .rail.pos ~ .eeprom.a1
    .rail.neg ~ .eeprom.a2
    .rail.neg ~ .eeprom.wp
    .mcu.pb6.function &= "I2C1_SCL"
    .mcu.pb7.function &= "I2C1_SDA"
    .mcu.pb6 ~ .eeprom.scl
    .mcu.pb7 ~ .eeprom.sda

    .vbus.pos ~ .bridge.regin
    .vbus.pos ~ .bridge.vbus
    .rail.pos ~ .bridge.vdd
    .vbus.neg ~ .bridge.gnd
    .usb: Header1x2
    .bridge.d_plus ~ .usb.p1
    .bridge.d_minus ~ .usb.p2
    .mcu.pa9.function &= "USART1_TX"
    .mcu.pa10.function &= "USART1_RX"
    .mcu.pa9 ~ .bridge.rxd
    .mcu.pa10 ~ .bridge.txd
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

    fn build(source: &str) -> Result<netsmith::Built, String> {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        build_with(
            "ics.nsm",
            source,
            &target.loader().unwrap(),
            &target.catalogs().unwrap(),
            &libraries(),
        )
        .map(|mut boards| boards.remove(0))
        .map_err(|e| e.to_string())
    }

    fn built() -> netsmith::Built {
        build(FIXTURE).unwrap_or_else(|e| panic!("{e}"))
    }

    /// The five parts: kind, package, the reference board's designator.
    const PARTS: [(&str, &str, &str); 5] = [
        ("Mcp73831", "SOT-23-5", "U1"),
        ("Tps63802", "VSON-10(2x3)", "U15"),
        ("Stm32F030C8", "LQFP-48(7x7)", "U16"),
        ("At24C128C", "SOIC-8", "U17"),
        ("Cp2102", "WQFN-28-EP(5x5)", "U22"),
    ];

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

    /// The footprint the project's `kicad.toml` gives a kind, loaded as the build loads it.
    fn footprint_of(kind: &str, package: &str) -> Footprint {
        let libraries = libraries();
        let mapped = libraries
            .mapping
            .map(kind, Some(package))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            mapped.lib_id, None,
            "the symbol is drawn from the pin table"
        );
        libraries
            .footprints
            .load(&mapped.footprint)
            .unwrap_or_else(|e| panic!("{}: {e}", mapped.footprint))
    }

    fn numbered(footprint: &Footprint) -> BTreeSet<String> {
        footprint
            .pad_numbers()
            .into_iter()
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn pad<'a>(footprint: &'a Footprint, number: &str) -> &'a netsmith_pcb::Pad {
        footprint
            .pads
            .iter()
            .find(|p| p.number == number)
            .unwrap_or_else(|| panic!("no pad {number}"))
    }

    /// A pad of the reference board's footprint: its number, centre and size as the pad lies in
    /// the footprint (the table's `size` is as the file writes it before the pad's own turn,
    /// and `angle` includes the footprint's rotation, so a pad turned a quarter against its
    /// footprint has its width and height swapped), and its net.
    struct Theirs {
        number: String,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        net: String,
    }

    fn reference(reference: &str) -> Vec<Theirs> {
        let table = parsed(&read("reference/placements.toml"));
        let parts = tables(&table, "part");
        let part = parts
            .iter()
            .find(|p| p["reference"].as_str() == Some(reference))
            .unwrap_or_else(|| panic!("the reference has no {reference}"));
        let rotation = part
            .get("rotation")
            .map(|r| {
                r.as_float()
                    .or_else(|| r.as_integer().map(|i| i as f64))
                    .unwrap()
            })
            .unwrap_or(0.0);
        part["pads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_table().unwrap())
            .map(|p| {
                let get = |k: &str| match &p[k] {
                    toml::Value::Float(f) => *f,
                    toml::Value::Integer(i) => *i as f64,
                    other => panic!("{k} is {other:?}"),
                };
                let angle = p.get("angle").map(|_| get("angle")).unwrap_or(0.0);
                let relative = (angle - rotation).rem_euclid(180.0);
                let size = p["size"].as_array().unwrap();
                let (w, h) = (
                    size[0]
                        .as_float()
                        .unwrap_or_else(|| size[0].as_integer().unwrap() as f64),
                    size[1]
                        .as_float()
                        .unwrap_or_else(|| size[1].as_integer().unwrap() as f64),
                );
                let (width, height) = if (relative - 90.0).abs() < 1e-6 {
                    (h, w)
                } else {
                    (w, h)
                };
                Theirs {
                    number: p["number"].as_str().unwrap().to_string(),
                    x: get("x"),
                    y: get("y"),
                    width,
                    height,
                    net: p
                        .get("net")
                        .map(|n| n.as_str().unwrap().to_string())
                        .unwrap_or_default(),
                }
            })
            .collect()
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn the_fixture_builds_with_all_five_parts_and_no_kicad() {
        let built = built();
        for (kind, _, refdes) in PARTS {
            assert!(
                built.design.parts.iter().any(|p| p.kind == kind),
                "no {kind}, which ORBIT calls {refdes}"
            );
        }
        // Every part's symbol is drawn from its pin table (the part's pins are in the schematic).
        for (kind, package, _) in PARTS {
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
                    "{kind}: no pin {datasheet}"
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

    #[test]
    fn each_part_has_the_pads_of_its_datasheet_each_once() {
        let built = built();
        for (kind, pads_wanted) in [
            ("Mcp73831", 5),
            ("Tps63802", 10),
            ("Stm32F030C8", 48),
            ("At24C128C", 8),
            // 28 leads and the exposed pad, pad 29.
            ("Cp2102", 29),
        ] {
            let mut pads: Vec<String> = pins_of(&built, kind)
                .into_iter()
                .flat_map(|(_, _, pads)| pads)
                .collect();
            assert_eq!(pads.len(), pads_wanted, "{kind}: one entry for each pad");
            pads.sort_unstable();
            pads.dedup();
            assert_eq!(pads.len(), pads_wanted, "{kind}: a pad is given twice");
            let wanted: BTreeSet<String> = (1..=pads_wanted).map(|n| n.to_string()).collect();
            assert_eq!(pads.into_iter().collect::<BTreeSet<_>>(), wanted, "{kind}");
        }
    }

    /// The supply and ground pins are `power`, as the datasheets' pin tables have them.
    #[test]
    fn the_supply_and_ground_pins_are_power_pins_on_the_datasheets_pads() {
        let built = built();
        let power = |kind: &str| -> BTreeMap<String, Vec<String>> {
            pins_of(&built, kind)
                .into_iter()
                .filter(|(_, k, _)| *k == PinKind::Power)
                .map(|(name, _, pads)| (name, pads))
                .collect()
        };
        let own = |list: &[(&str, &[&str])]| -> BTreeMap<String, Vec<String>> {
            list.iter()
                .map(|(n, p)| (n.to_string(), p.iter().map(|s| s.to_string()).collect()))
                .collect()
        };
        assert_eq!(
            power("Mcp73831"),
            own(&[("VSS", &["2"]), ("VBAT", &["3"]), ("VDD", &["4"])])
        );
        assert_eq!(
            power("Tps63802"),
            own(&[
                ("AGND", &["3"]),
                ("VOUT", &["6"]),
                ("GND", &["8"]),
                ("VIN", &["10"])
            ])
        );
        assert_eq!(
            power("Stm32F030C8"),
            own(&[
                ("VDD", &["1", "24", "48"]),
                ("VSSA", &["8"]),
                ("VDDA", &["9"]),
                ("VSS", &["23", "47"])
            ])
        );
        assert_eq!(power("At24C128C"), own(&[("GND", &["4"]), ("VCC", &["8"])]));
        assert_eq!(
            power("Cp2102"),
            own(&[("GND", &["3", "29"]), ("VDD", &["6"]), ("REGIN", &["7"])])
        );
        // The other kinds the datasheets give, pin by pin.
        let kind_of = |part: &str, name: &str| -> PinKind {
            pins_of(&built, part)
                .into_iter()
                .find(|(n, _, _)| n == name)
                .unwrap_or_else(|| panic!("{part} has no pin {name}"))
                .1
        };
        // STAT: "a tri-state logic output on the MCP73831" (DS20001984G, section 3.3, page 11).
        assert_eq!(kind_of("Mcp73831", "STAT"), PinKind::TriState);
        assert_eq!(kind_of("Mcp73831", "PROG"), PinKind::Input);
        assert_eq!(kind_of("Tps63802", "PG"), PinKind::Output);
        assert_eq!(kind_of("Tps63802", "EN"), PinKind::Input);
        assert_eq!(kind_of("Tps63802", "L1"), PinKind::Passive);
        assert_eq!(kind_of("Stm32F030C8", "BOOT0"), PinKind::Input);
        assert_eq!(kind_of("Stm32F030C8", "NRST"), PinKind::Bidirectional);
        assert_eq!(kind_of("Stm32F030C8", "PA9"), PinKind::Bidirectional);
        assert_eq!(kind_of("At24C128C", "SDA"), PinKind::Bidirectional);
        assert_eq!(kind_of("At24C128C", "SCL"), PinKind::Input);
        assert_eq!(kind_of("Cp2102", "TXD"), PinKind::Output);
        assert_eq!(kind_of("Cp2102", "RXD"), PinKind::Input);
        assert_eq!(kind_of("Cp2102", "D+"), PinKind::Bidirectional);
        assert_eq!(kind_of("Cp2102", "NC"), PinKind::NoConnect);
    }

    /// What ORBIT's board has on the pads of a part: the nets that are named like a pin or
    /// are a supply (the table is `pin name`, `net`), every pad that is not in the table is
    /// on a net with no name of its own (`Net-(...)` or `unconnected-(...)`), and the
    /// pins and the pads are the same set.
    fn check_nets(kind: &str, refdes: &str, named: &[(&str, &str)]) {
        let built = built();
        let pins = pins_of(&built, kind);
        let theirs: BTreeMap<String, String> = reference(refdes)
            .into_iter()
            .map(|p| (p.number, p.net))
            .collect();
        let mut seen = BTreeSet::new();
        for (name, _, pads) in &pins {
            for pad in pads {
                let net = theirs
                    .get(pad)
                    .unwrap_or_else(|| panic!("{refdes} has no pad {pad} ({name})"));
                seen.insert(pad.clone());
                match named.iter().find(|(n, _)| n == name) {
                    Some((_, expected)) => {
                        assert_eq!(net, expected, "{refdes} pad {pad} ({name})")
                    }
                    None => assert!(
                        net.starts_with("unconnected-(") || net.starts_with("Net-("),
                        "{refdes} pad {pad} ({name}) is on the named net `{net}`: add it to the table"
                    ),
                }
            }
        }
        assert_eq!(
            seen,
            theirs.keys().cloned().collect::<BTreeSet<_>>(),
            "{refdes}: the pads of the board are the pads of the part"
        );
        // A net that is named after a pin (`Net-(U15-FB)`, `unconnected-(U22-DCD-Pad1)`) names
        // the pin of the pad the net is on.
        let normal = |name: &str| {
            name.replace("{slash}", "/")
                .replace(['~', '{', '}'], "")
                .to_ascii_lowercase()
        };
        for (name, _, pads) in &pins {
            for pad in pads {
                let net = &theirs[pad];
                let named_after = net
                    .strip_prefix(&format!("Net-({refdes}-"))
                    .and_then(|r| r.strip_suffix(')'))
                    .map(str::to_string)
                    .or_else(|| {
                        net.strip_prefix(&format!("unconnected-({refdes}-"))
                            .and_then(|r| r.rsplit_once("-Pad").map(|(n, _)| n.to_string()))
                    });
                if let Some(after) = named_after {
                    let after = normal(&after);
                    let after = after.split('@').next().unwrap();
                    let mine = normal(name);
                    assert!(
                        mine == after
                            || mine.starts_with(after)
                            || (after.starts_with("nc") && mine.starts_with("nc")),
                        "{refdes} pad {pad}: the net `{net}` names `{after}`, the part's pin is `{name}`"
                    );
                }
            }
        }
    }

    #[test]
    fn the_charger_agrees_with_the_nets_of_orbits_u1() {
        check_nets(
            "Mcp73831",
            "U1",
            &[("VSS", "GND"), ("VBAT", "+BATT"), ("VDD", "VBUS")],
        );
    }

    #[test]
    fn the_converter_agrees_with_the_nets_of_orbits_u15() {
        check_nets(
            "Tps63802",
            "U15",
            &[
                ("EN", "+BATT"),
                ("MODE", "GND"),
                ("AGND", "GND"),
                ("VOUT", "+3.3V"),
                ("GND", "GND"),
                ("VIN", "+BATT"),
            ],
        );
    }

    #[test]
    fn the_microcontroller_agrees_with_the_nets_of_orbits_u16() {
        // Every pin whose net is named like it, from the board file: the supplies, the
        // reset, the header pins `PA2` to `PB11`, the debug port, the UART, the I2C bus, the
        // 8 MHz crystal and the boot pin.
        let mut named: Vec<(&str, &str)> = vec![
            ("VDD", "+3.3V"),
            ("VDDA", "+3.3V"),
            ("VSS", "GND"),
            ("VSSA", "GND"),
            ("NRST", "NRST"),
            ("BOOT0", "Net-(U16-BOOT0)"),
            ("PA9", "USART1_TX"),
            ("PA10", "USART1_RX"),
            ("PA13", "SWDIO"),
            ("PA14", "SWDCLK"),
            ("PB6", "I2C1_SCLK"),
            ("PB7", "I2C1_SDA"),
            ("PF0-OSC_IN", "/HSE_IN"),
            ("PF1-OSC_OUT", "/HSE_OUT"),
            ("PC14-OSC32_IN", "PC14"),
            ("PC15-OSC32_OUT", "USER_Button"),
            ("PA4", "LED1"),
            ("PA5", "LED2"),
        ];
        for pin in [
            "PA2", "PA3", "PA6", "PA7", "PB0", "PB1", "PB2", "PB10", "PB11",
        ] {
            named.push((pin, pin));
        }
        check_nets("Stm32F030C8", "U16", &named);
    }

    #[test]
    fn the_eeprom_agrees_with_the_nets_of_orbits_u17() {
        check_nets(
            "At24C128C",
            "U17",
            &[
                ("A0", "+3.3V"),
                ("A1", "+3.3V"),
                ("A2", "GND"),
                ("GND", "GND"),
                ("SDA", "I2C1_SDA"),
                ("SCL", "I2C1_SCLK"),
                ("WP", "GND"),
                ("VCC", "+3.3V"),
            ],
        );
    }

    #[test]
    fn the_bridge_agrees_with_the_nets_of_orbits_u22() {
        // Its RXD is the line the MCU sends on, and its TXD the one it reads.
        check_nets(
            "Cp2102",
            "U22",
            &[
                ("GND", "GND"),
                ("D+", "/USB_D+"),
                ("D-", "/USB_D-"),
                ("VDD", "Net-(U22-VDD)"),
                ("REGIN", "VBUS"),
                ("VBUS", "VBUS"),
                ("RXD", "USART1_TX"),
                ("TXD", "USART1_RX"),
            ],
        );
    }

    #[test]
    fn the_footprints_have_the_pads_of_the_pins_and_no_others() {
        let built = built();
        for (kind, package, _) in PARTS {
            let footprint = footprint_of(kind, package);
            let pads = numbered(&footprint);
            let wanted: BTreeSet<String> = pins_of(&built, kind)
                .into_iter()
                .flat_map(|(_, _, pads)| pads)
                .collect();
            assert_eq!(pads, wanted, "{kind}");
            // Only the unnumbered paste openings of the bridge share no number.
            let unnumbered = footprint
                .pads
                .iter()
                .filter(|p| p.number.is_empty())
                .count();
            assert_eq!(
                footprint.pads.len() - unnumbered,
                pads.len(),
                "{kind}: a pad number is given twice"
            );
        }
    }

    /// The charger's pads are the generator's `SOT-23-5`, which are ORBIT's pad for pad, and
    /// cover the drawing's leads (Microchip DS20001984G, drawing C04-091B, page 22: pitch
    /// 0.95, 2.20 to 3.20 over the leads, foot 0.35 to 0.80). The datasheet draws no land
    /// pattern for the SOT-23.
    #[test]
    fn the_chargers_footprint_is_the_generators_and_orbits_and_covers_the_leads() {
        let footprint = footprint_of("Mcp73831", "SOT-23-5");
        for theirs in reference("U1") {
            let mine = pad(&footprint, &theirs.number);
            assert!(
                near(mine.at.x, theirs.x) && near(mine.at.y, theirs.y),
                "{}",
                theirs.number
            );
            assert!(
                near(mine.width, theirs.width) && near(mine.height, theirs.height),
                "pad {} is {} x {}",
                theirs.number,
                mine.width,
                mine.height
            );
        }
        assert!(near(
            (pad(&footprint, "1").at.y - pad(&footprint, "2").at.y).abs(),
            0.95
        ));
        assert!(near(
            (pad(&footprint, "2").at.y - pad(&footprint, "3").at.y).abs(),
            0.95
        ));
        // Pads 1 to 3 on one side, 4 and 5 on the other, pad 5 across from pad 1.
        assert!(pad(&footprint, "1").at.x < 0.0 && pad(&footprint, "3").at.x < 0.0);
        assert!(pad(&footprint, "4").at.x > 0.0 && pad(&footprint, "5").at.x > 0.0);
        assert!(near(pad(&footprint, "5").at.y, pad(&footprint, "1").at.y));
        // The leads' tips are at most 3.20 / 2 from the middle, and the shortest foot, 0.35,
        // reaches in to 1.25: the pad starts before that and ends beyond the tip.
        let p = pad(&footprint, "1");
        let (inner, outer) = (p.at.x.abs() - p.width / 2.0, p.at.x.abs() + p.width / 2.0);
        assert!(outer >= 1.6 && inner <= 1.6 - 0.35, "{inner} to {outer}");
    }

    /// The converter's pads are TI's land pattern example (drawing DLA0010A, SLVSEU9D, the
    /// "Example Board Layout": five 0.60 x 0.25 mm pads 0.90 mm left of the package's centre
    /// line, four 0.90 x 0.25 mm pads 0.75 mm to the right, one 1.30 x 0.25 mm pad at 0.55 mm,
    /// all at 0.50 mm pitch), and ORBIT's footprint has the same numbers, centres and sizes
    /// (its pads are rounded rectangles where these are rectangles).
    #[test]
    fn the_converters_footprint_is_tis_land_pattern_and_orbits() {
        let footprint = footprint_of("Tps63802", "VSON-10(2x3)");
        for n in 1..=5 {
            let p = pad(&footprint, &n.to_string());
            assert!(
                near(p.at.x, -0.9) && near(p.at.y, -1.0 + 0.5 * (n - 1) as f64),
                "pad {n}"
            );
            assert!(near(p.width, 0.6) && near(p.height, 0.25), "pad {n}");
        }
        for (n, y) in [(10, -1.0), (9, -0.5), (7, 0.5), (6, 1.0)] {
            let p = pad(&footprint, &n.to_string());
            assert!(near(p.at.x, 0.75) && near(p.at.y, y), "pad {n}");
            assert!(near(p.width, 0.9) && near(p.height, 0.25), "pad {n}");
        }
        let long = pad(&footprint, "8");
        assert!(near(long.at.x, 0.55) && near(long.at.y, 0.0));
        assert!(near(long.width, 1.3) && near(long.height, 0.25));
        for theirs in reference("U15") {
            let mine = pad(&footprint, &theirs.number);
            assert!(
                near(mine.at.x, theirs.x)
                    && near(mine.at.y, theirs.y)
                    && near(mine.width, theirs.width)
                    && near(mine.height, theirs.height),
                "pad {} differs from ORBIT's",
                theirs.number
            );
        }
    }

    /// The microcontroller's pads are the generator's `LQFP-48_7x7mm_P0.5mm`, which are ORBIT's
    /// pad for pad. ST's recommended footprint (DocID024849 Rev 2, Figure 31, page 86) is
    /// 1.20 x 0.30 mm pads at 0.50 mm pitch whose centres are (9.70 - 1.20) / 2 = 4.25 mm from
    /// the middle: each lies inside the generator's pad (1.475 x 0.30 mm at 4.1625 mm).
    #[test]
    fn the_microcontrollers_footprint_is_the_generators_and_orbits_and_holds_sts_pads() {
        let footprint = footprint_of("Stm32F030C8", "LQFP-48(7x7)");
        assert_eq!(footprint.pads.len(), 48);
        for theirs in reference("U16") {
            let mine = pad(&footprint, &theirs.number);
            assert!(
                near(mine.at.x, theirs.x)
                    && near(mine.at.y, theirs.y)
                    && near(mine.width, theirs.width)
                    && near(mine.height, theirs.height),
                "pad {} differs from ORBIT's",
                theirs.number
            );
        }
        for p in &footprint.pads {
            // ST's pad on the same side, along the same axis.
            let (along, across, long, wide) = if p.width > p.height {
                (p.at.y, p.at.x.abs(), p.width, p.height)
            } else {
                (p.at.x, p.at.y.abs(), p.height, p.width)
            };
            let _ = along;
            assert!(near(wide, 0.30), "pad {} is 0.30 mm wide as ST's", p.number);
            assert!(
                across - long / 2.0 <= 4.25 - 0.6 + 1e-9
                    && across + long / 2.0 >= 4.25 + 0.6 - 1e-9,
                "pad {} does not hold ST's (3.65 to 4.85 mm from the middle): {} to {}",
                p.number,
                across - long / 2.0,
                across + long / 2.0
            );
        }
        // The pitch is the datasheet's 0.50 along each side.
        let (p1, p2) = (pad(&footprint, "1"), pad(&footprint, "2"));
        assert!(near((p1.at.y - p2.at.y).abs(), 0.5));
    }

    /// The EEPROM's pads are the generator's 8-lead SOIC (3.9 x 4.9 mm body, 1.27 mm pitch).
    /// Microchip's land pattern (DS20006110A, drawing C04-2057-SN Rev B, page 27) is pads of
    /// 1.55 x 0.60 mm at 5.40 mm between the two rows' centres, so 2.70 mm from the middle:
    /// the width is the same, the outer edges are 0.025 mm apart and the generator's are
    /// longer to the inside (1.95 x 0.60 mm at 2.475 mm). ORBIT's pads are 1.9812 x 0.5588 mm
    /// at 2.4638 mm.
    #[test]
    fn the_eeproms_footprint_is_the_generators_and_is_close_to_microchips_land_pattern() {
        let footprint = footprint_of("At24C128C", "SOIC-8");
        for (n, y) in [("1", -1.905), ("2", -0.635), ("3", 0.635), ("4", 1.905)] {
            let p = pad(&footprint, n);
            assert!(near(p.at.y, y) && p.at.x < 0.0, "pad {n}");
            let q = pad(&footprint, &(9 - n.parse::<i32>().unwrap()).to_string());
            // Pad 8 is across from pad 1: the numbers go down the left side and up the right.
            assert!(
                near(q.at.y, y) && q.at.x > 0.0,
                "pad {}",
                9 - n.parse::<i32>().unwrap()
            );
        }
        for p in &footprint.pads {
            assert!(
                near(p.height, 0.6),
                "pad {} is 0.60 mm wide as Microchip's",
                p.number
            );
            let outer = p.at.x.abs() + p.width / 2.0;
            let microchip_outer = 2.70 + 1.55 / 2.0;
            assert!(
                (outer - microchip_outer).abs() <= 0.03,
                "pad {}: outer edge {outer}, Microchip's {microchip_outer}",
                p.number
            );
            assert!(
                p.at.x.abs() - p.width / 2.0 <= 2.70 - 1.55 / 2.0,
                "pad {}",
                p.number
            );
        }
        // ORBIT's pads: the same pitch and number, other centres and sizes.
        let theirs = reference("U17");
        assert_eq!(theirs.len(), 8);
        for t in &theirs {
            assert!(near(t.x.abs(), 2.4638) && near(t.width, 1.9812) && near(t.height, 0.5588));
            assert!(
                near(t.y, pad(&footprint, &t.number).at.y),
                "pad {}",
                t.number
            );
        }
    }

    /// The bridge's pads are Silicon Labs' land pattern (CP2102/9 Rev. 1.6, Figure 4 and
    /// Table 11, page 14): 0.50 mm pitch, 4.80 mm between the pad columns' centres, pads 0.25 x
    /// 0.90 mm, a centre pad 3.25 x 3.25 mm (the table gives 3.20 to 3.30) with the nine
    /// 0.90 mm paste openings of note 8 on a 1.1 mm pitch. ORBIT's pads are 0.9 x 0.254 mm
    /// but 2.59 mm from the middle, 0.19 mm further out, and its centre pad is 3.1 x 3.1 mm.
    #[test]
    fn the_bridges_footprint_is_silicon_labs_land_pattern_and_orbits_pads_are_further_out() {
        let footprint = footprint_of("Cp2102", "WQFN-28-EP(5x5)");
        let leads: Vec<_> = footprint
            .pads
            .iter()
            .filter(|p| !p.number.is_empty() && p.number != "29")
            .collect();
        assert_eq!(leads.len(), 28);
        for p in &leads {
            let (across, along, long, wide) = if p.width > p.height {
                (p.at.x.abs(), p.at.y, p.width, p.height)
            } else {
                (p.at.y.abs(), p.at.x, p.height, p.width)
            };
            assert!(near(across, 2.4), "pad {}: C = 4.80 mm", p.number);
            assert!(near(long, 0.9) && near(wide, 0.25), "pad {}", p.number);
            assert!(
                near((along / 0.5).round() * 0.5, along) && along.abs() <= 1.5,
                "pad {}",
                p.number
            );
        }
        // Pad 1 at the top of the left side, 8 at the left of the bottom, 15 at the bottom of
        // the right side and 22 at the right of the top (Figure 2, counter-clockwise).
        let at = |n: &str| pad(&footprint, n).at;
        assert!(at("1").x < 0.0 && near(at("1").y, -1.5));
        assert!(near(at("8").x, -1.5) && at("8").y > 0.0);
        assert!(at("15").x > 0.0 && near(at("15").y, 1.5));
        assert!(near(at("22").x, 1.5) && at("22").y < 0.0);
        let centre = pad(&footprint, "29");
        assert!(near(centre.at.x, 0.0) && near(centre.at.y, 0.0));
        assert!(near(centre.width, 3.25) && near(centre.height, 3.25));
        let openings: Vec<_> = footprint
            .pads
            .iter()
            .filter(|p| p.number.is_empty())
            .collect();
        assert_eq!(openings.len(), 9);
        for o in &openings {
            assert!(near(o.width, 0.9) && near(o.height, 0.9));
            assert_eq!(o.layers, ["F.Paste"]);
            assert!(near((o.at.x / 1.1).round() * 1.1, o.at.x));
        }
        // ORBIT's footprint, pad for pad.
        for t in reference("U22") {
            let mine = pad(&footprint, &t.number);
            if t.number == "29" {
                assert!(near(t.width, 3.1) && near(t.height, 3.1));
                continue;
            }
            assert!(
                (t.x.abs() - mine.at.x.abs()).abs() < 1e-9
                    || (t.y.abs() - mine.at.y.abs()).abs() < 1e-9,
                "pad {}",
                t.number
            );
            let outwards = t.x.abs().max(t.y.abs()) - mine.at.x.abs().max(mine.at.y.abs());
            assert!(near(outwards, 0.19), "pad {}: {outwards}", t.number);
            assert!(
                near(t.width.min(t.height), 0.254) && near(t.width.max(t.height), 0.9),
                "pad {}",
                t.number
            );
        }
    }

    #[test]
    fn the_footprints_in_the_project_library_are_files_of_the_project() {
        let toml = read("kicad.toml");
        assert!(toml.contains("footprints = \"libs/orbit_ics.pretty\""));
        for file in [
            "libs/orbit_ics.pretty/VSON-10-HR_2x3mm_P0.5mm_TI_DLA0010A.kicad_mod",
            "libs/orbit_ics.pretty/QFN-28-1EP_5x5mm_P0.5mm_EP3.25x3.25mm_SiliconLabs_CP2102.kicad_mod",
        ] {
            let text = read(file);
            assert!(text.contains("(generator \"netsmith\")"), "{file}");
            assert!(text.contains("(descr \""), "{file}");
        }
    }

    #[test]
    fn the_catalog_rows_name_the_parts_and_where_they_were_read() {
        for (file, lcsc, mpn) in [
            ("catalogs/mcp73831.csv", "C424093", "MCP73831T-2ACI/OT"),
            ("catalogs/tps63802.csv", "C2845237", "TPS63802DLAR"),
            ("catalogs/stm32f030c8.csv", "C23922", "STM32F030C8T6"),
            ("catalogs/at24c128c.csv", "C6478", "AT24C128C-SSHM-T"),
            ("catalogs/cp2102.csv", "C6568", "CP2102-GMR"),
        ] {
            let rows = read(file);
            let row = rows.lines().nth(1).expect("a row");
            assert_eq!(rows.lines().count(), 2, "{file}: one row");
            assert!(row.starts_with(&format!("{lcsc},{mpn},")), "{row}");
            for page in [
                format!("https://jlcpcb.com/partdetail/{lcsc}"),
                format!("https://www.lcsc.com/product-detail/{lcsc}.html"),
            ] {
                assert!(
                    row.contains(&page),
                    "{file}: the page it was read from: {page}"
                );
            }
            assert!(
                row.contains("2026-11-10"),
                "{file}: the day the pages were read"
            );
            assert!(row.contains("library type Extended"), "{file}");
            // Price and stock are not recorded.
            let header = rows.lines().next().unwrap();
            assert!(
                !header.split(',').any(|c| c == "price" || c == "stock"),
                "{file}"
            );
        }
        let target = Target::resolve_from(&project(), None).unwrap();
        assert!(target.catalogs().is_ok());
    }

    #[test]
    fn the_board_gets_the_catalog_rows_the_parts_were_asked_for() {
        let built = built();
        for (kind, lcsc, mpn, package) in [
            ("Mcp73831", "C424093", "MCP73831T-2ACI/OT", "SOT-23-5"),
            ("Tps63802", "C2845237", "TPS63802DLAR", "VSON-10(2x3)"),
            ("Stm32F030C8", "C23922", "STM32F030C8T6", "LQFP-48(7x7)"),
            ("At24C128C", "C6478", "AT24C128C-SSHM-T", "SOIC-8"),
            ("Cp2102", "C6568", "CP2102-GMR", "WQFN-28-EP(5x5)"),
        ] {
            let s = built
                .sourcing
                .iter()
                .find(|s| s.kind == kind)
                .unwrap_or_else(|| panic!("no sourcing for {kind}"));
            assert_eq!(s.lcsc.as_deref(), Some(lcsc), "{kind}");
            assert_eq!(s.mpn.as_deref(), Some(mpn), "{kind}");
            assert_eq!(s.package.as_deref(), Some(package), "{kind}");
            assert_eq!(format!("{:?}", s.library.unwrap()), "Extended", "{kind}");
        }
    }

    /// The charger's charge current is the datasheet's `1000 V / RPROG` (section 5.1.2, page
    /// 15) and its programming resistor stays in the datasheet's range of 2 to 67 kohm (page
    /// 4): 10 kohm is 100 mA.
    #[test]
    fn the_chargers_current_is_one_thousand_volts_over_the_programming_resistor() {
        let with = |extra: &str| {
            build(&FIXTURE.replacen(
                "    // The charger",
                &format!("    {extra}\n    // The charger"),
                1,
            ))
        };
        with(".charger.charge_current >= 90mA\n    .charger.charge_current <= 110mA")
            .unwrap_or_else(|e| panic!("{e}"));
        let error = with(".charger.charge_current >= 200mA")
            .err()
            .expect("100 mA is not 200 mA");
        assert!(error.contains("charge_current"), "{error}");
        // 1 kohm is outside the datasheet's range.
        let low = build(&FIXTURE.replace("10kohm +/- 1%", "1kohm +/- 1%"))
            .err()
            .expect("1 kohm is below the 2 kohm the datasheet allows");
        assert!(low.contains("rprog"), "{low}");
    }

    /// The converter's feedback reference, 500 mV +/- 1 % (SLVSEU9D, page 6), feeds the std
    /// feedback divider: two resistors that make 3.3 V +/- 5 % from it.
    #[test]
    fn the_converters_reference_feeds_the_standard_feedback_divider() {
        let source = FIXTURE
            .replace(
                "import std::sources::Rail",
                "import std::parts::regulator::FeedbackDivider\nimport std::sources::Rail",
            )
            .replace(
                "    .converter.fb ~ .rail.pos\n",
                "    .divider: FeedbackDivider { .vout &= 3.3V +/- 5% }\n    .divider.vref == .converter.vref\n    .divider.top ~ .rail.pos\n    .divider.bottom ~ .rail.neg\n    .divider.fb ~ .converter.fb\n",
            );
        build(&source).unwrap_or_else(|e| panic!("{e}"));
    }
}
