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
