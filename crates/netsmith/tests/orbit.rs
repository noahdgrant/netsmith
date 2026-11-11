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

/// ORBIT's connectors, switches, inductor, crystal and LEDs as parts (`src/shouhan_mini_5ptp.nsm`,
/// `src/jst_b2b_ph_sm4_tb.nsm`, `src/xkb_ts_1187a.nsm`, `src/hro_k3_1270.nsm`,
/// `src/everlight_19_217.nsm`, and the standard `Header1x8`, `Crystal` and `Inductor` with the
/// project's own rows and footprints). The must-match ones (USB1, BAT1, J1, J2, SW1, SW2, BOOT0,
/// PWR1) are compared pad for pad with the reference table; the others only have to build and be
/// close. Nothing here needs KiCad.
mod small_parts {
    use std::collections::{BTreeMap, BTreeSet};

    use netsmith::project::Target;
    use netsmith::{Libraries, build_with};
    use netsmith_kicad::install::Search;
    use netsmith_kicad::testlib;
    use netsmith_pcb::{Footprint, PadKind, PadShape};

    use super::{number, parsed, project, read, tables};

    /// A small board of our own, never built by the project: one of each part, so that the
    /// parts, the catalogs and the `kicad.toml` rows build together.
    const FIXTURE: &str = r#"
import src::everlight_19_217::{Everlight19217Bhc, Everlight19217Ghc}
import src::hro_k3_1270::HroK3_1270
import src::jst_b2b_ph_sm4_tb::JstB2bPhSm4Tb
import src::shouhan_mini_5ptp::ShouHanMini5ptp
import src::xkb_ts_1187a::XkbTs1187a
import std::parts::connector::Header1x8
import std::parts::passive::{Crystal, Inductor}
import std::sources::Rail

board SmallPartsFixture {
    .vbus: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VBUS"
        .neg.net &= "GND"
    }
    .usb: ShouHanMini5ptp { .mpn &= "MINI 5PTP" }
    .vbus.pos ~ .usb.vbus
    .vbus.neg ~ .usb.gnd

    .bat: JstB2bPhSm4Tb { .mpn &= "B2B-PH-SM4-TB(LF)(SN)" }
    .vbus.neg ~ .bat.p1
    .power: HroK3_1270 { .mpn &= "K3-1270S-F2" }
    .bat.p2 ~ .power.a
    .boot: HroK3_1270 { .mpn &= "K3-1270S-F2" }
    .power.com ~ .boot.a

    .reset: XkbTs1187a { .mpn &= "TS-1187A-B-A-B" }
    .user: XkbTs1187a { .mpn &= "TS-1187A-B-A-B" }
    .reset.b ~ .vbus.neg
    .user.b ~ .vbus.neg

    .west: Header1x8
    .east: Header1x8
    .west.p1 ~ .vbus.neg
    .east.p1 ~ .vbus.neg

    .xtal: Crystal {
        .package &= "5032"
        .frequency &= 8MHz +/- 30ppm
    }
    .l1: Inductor {
        .package &= "0806"
        .inductance &= 470nH +/- 20%
    }
    .charge: Everlight19217Ghc
    .led2: Everlight19217Bhc
}
"#;

    fn libraries() -> Libraries {
        let nowhere = testlib::TempDir::new("orbit-no-kicad");
        Libraries::for_project_by(Some(&project()), &Search::under(nowhere.path()))
            .unwrap_or_else(|e| panic!("{e}"))
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

    fn part<'a>(built: &'a netsmith::Built, path: &str) -> &'a netsmith_core::Part {
        built
            .design
            .parts
            .iter()
            .find(|p| p.path == path)
            .unwrap_or_else(|| panic!("no part {path}"))
    }

    /// The footprint a kind and package are exported with, loaded as the build loads it.
    fn footprint(libraries: &Libraries, kind: &str, package: &str) -> Footprint {
        let mapped = libraries
            .mapping
            .map(kind, Some(package))
            .unwrap_or_else(|e| panic!("{kind} {package}: {e}"));
        libraries
            .footprints
            .load(&mapped.footprint)
            .unwrap_or_else(|e| panic!("{}: {e}", mapped.footprint))
    }

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

    fn as_f(v: &toml::Value) -> f64 {
        v.as_float()
            .or_else(|| v.as_integer().map(|i| i as f64))
            .unwrap()
    }

    /// Every difference between the pads of `footprint` and those of the reference's
    /// `reference` footprint: number, centre in the footprint's own frame (0.05 mm), size
    /// (0.05 mm), drill (0.05 mm), shape, kind and angle. Empty when they are the same.
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
                "{reference}: {} pads, the reference has {}",
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
                    && (own.at.x - x).abs() <= 0.05
                    && (own.at.y - y).abs() <= 0.05
            });
            let Some((i, own)) = found else {
                differences.push(format!(
                    "{reference} pad {n:?}: none at ({x}, {y}) within 0.05 mm; pads of that number are at {:?}",
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
            let (w, h) = (as_f(&size[0]), as_f(&size[1]));
            // A pad turned by 90 degrees has its size's two numbers the other way round in
            // the file; the table gives the size in the pad's own frame, and so does the
            // footprint, so they are compared as they are.
            if (own.width - w).abs() > 0.05 || (own.height - h).abs() > 0.05 {
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
            // The file's pad angle includes the footprint's rotation; a rectangle, a circle
            // or an oval turned by 180 degrees is as it was.
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
                Some((as_f(&slot[0]), as_f(&slot[1])))
            } else if pad.contains_key("drill") {
                Some((number(pad, "drill"), number(pad, "drill")))
            } else {
                None
            };
            match (own.drill, want_drill) {
                (None, None) => {}
                (Some(d), Some((a, b)))
                    if (d.width - a).abs() <= 0.05 && (d.height - b).abs() <= 0.05 => {}
                (d, w) => differences.push(format!(
                    "{at}: drill {:?}, the reference's {w:?}",
                    d.map(|d| (d.width, d.height))
                )),
            }
        }
        differences
    }

    fn reference_table() -> toml::Table {
        parsed(&read("reference/placements.toml"))
    }

    /// The pads of ORBIT's part with their nets: pad number to nets (a number may repeat).
    fn reference_nets(reference: &str) -> BTreeMap<String, BTreeSet<String>> {
        let table = reference_table();
        let parts = tables(&table, "part");
        let part = parts
            .iter()
            .find(|p| p["reference"].as_str() == Some(reference))
            .unwrap_or_else(|| panic!("the reference has no {reference}"));
        let mut nets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for pad in part["pads"].as_array().unwrap() {
            let pad = pad.as_table().unwrap();
            if let (Some(n), Some(net)) = (
                pad["number"].as_str().filter(|n| !n.is_empty()),
                pad.get("net").and_then(|n| n.as_str()),
            ) {
                nets.entry(n.to_string())
                    .or_default()
                    .insert(net.to_string());
            }
        }
        nets
    }

    /// The parts of the project, as `(reference, kind, package)`, for the pad comparison.
    const MUST_MATCH: [(&str, &str, &str); 8] = [
        ("USB1", "ShouHanMini5ptp", "SMD"),
        ("BAT1", "JstB2bPhSm4Tb", "SMD,P=2mm"),
        ("J1", "Header1x8", "2.54mm"),
        ("J2", "Header1x8", "2.54mm"),
        ("SW1", "XkbTs1187a", "SMD-4P,5.1x5.1mm"),
        ("SW2", "XkbTs1187a", "SMD-4P,5.1x5.1mm"),
        ("BOOT0", "HroK3_1270", "SMD,8.8x2mm"),
        ("PWR1", "HroK3_1270", "SMD,8.8x2mm"),
    ];

    #[test]
    fn the_must_match_footprints_have_the_pads_of_orbits_board() {
        let libraries = libraries();
        let table = reference_table();
        let mut differences = Vec::new();
        for (reference, kind, package) in MUST_MATCH {
            let footprint = footprint(&libraries, kind, package);
            differences.extend(pad_differences(&footprint, &table, reference));
        }
        assert!(
            differences.is_empty(),
            "the footprints differ from ORBIT's pads:\n{}",
            differences.join("\n")
        );
        // Pads counted as the board has them: the locating holes of USB1 and the repeated
        // numbers of the buttons and the battery's tabs are pads too.
        for (reference, kind, package, pads) in [
            ("USB1", "ShouHanMini5ptp", "SMD", 11),
            ("BAT1", "JstB2bPhSm4Tb", "SMD,P=2mm", 4),
            ("J1", "Header1x8", "2.54mm", 8),
            ("SW1", "XkbTs1187a", "SMD-4P,5.1x5.1mm", 4),
            ("BOOT0", "HroK3_1270", "SMD,8.8x2mm", 5),
        ] {
            assert_eq!(
                footprint(&libraries, kind, package).pads.len(),
                pads,
                "{reference}"
            );
        }
    }

    #[test]
    fn the_comparison_of_pads_sees_a_pad_that_moved_or_changed() {
        let libraries = libraries();
        let table = reference_table();
        let good = footprint(&libraries, "HroK3_1270", "SMD,8.8x2mm");
        assert!(pad_differences(&good, &table, "PWR1").is_empty());
        let mut moved = good.clone();
        moved.pads[0].at.x += 0.2;
        let mut swapped = good.clone();
        let (w, h) = (swapped.pads[1].width, swapped.pads[1].height);
        (swapped.pads[1].width, swapped.pads[1].height) = (h, w);
        let mut drilled = good.clone();
        drilled.pads[3].drill = None;
        let mut turned = good.clone();
        turned.pads[2].angle += 90.0;
        for (what, wrong) in [
            ("moved", moved),
            ("swapped", swapped),
            ("drilled", drilled),
            ("turned", turned),
        ] {
            let differences = pad_differences(&wrong, &table, "PWR1");
            assert_eq!(differences.len(), 1, "a pad {what}: {differences:?}");
        }
        let mut short = good.clone();
        short.pads.pop();
        assert!(!pad_differences(&short, &table, "PWR1").is_empty());
    }

    #[test]
    fn the_other_parts_build_and_are_close_to_orbits() {
        let libraries = libraries();
        let table = reference_table();
        // U10: the standard 5032 crystal is the board's own footprint, pad for pad.
        let crystal = footprint(&libraries, "Crystal", "5032");
        assert_eq!(
            pad_differences(&crystal, &table, "U10"),
            Vec::<String>::new()
        );
        // D1 to D3: the standard 0603 LED, pad for pad (D2 and D3 stand turned by 90 degrees,
        // which the footprint's frame does not see).
        let led = footprint(&libraries, "Everlight19217Ghc", "0603");
        for reference in ["D1", "D2", "D3"] {
            assert_eq!(
                pad_differences(&led, &table, reference),
                Vec::<String>::new(),
                "{reference}"
            );
        }
        assert_eq!(
            footprint(&libraries, "Everlight19217Bhc", "0603")
                .pads
                .len(),
            2
        );
        // L1: Murata's recommended pattern, not the board's. Two pads, centres 1.6 mm apart
        // where the board's are 1.72 mm apart (a difference of 0.12 mm).
        let inductor = footprint(&libraries, "Inductor", "0806");
        assert_eq!(inductor.pads.len(), 2);
        let pitch = (inductor.pads[1].at.x - inductor.pads[0].at.x).abs();
        assert!((pitch - 1.6).abs() < 1e-9, "{pitch}");
        assert!((pitch - 1.72).abs() < 0.15);
        for pad in &inductor.pads {
            assert_eq!((pad.width, pad.height), (0.8, 1.8));
        }
    }

    #[test]
    fn the_fixture_builds_with_no_kicad_and_every_part_is_the_one_the_board_names() {
        let built = built();
        assert!(!built.schematic.is_empty());
        let sourced = |path: &str| {
            let refdes = &part(&built, path).refdes;
            built
                .sourcing
                .iter()
                .find(|s| &s.refdes == refdes)
                .unwrap_or_else(|| panic!("no {path} in the sourcing"))
        };
        for (path, lcsc, mpn) in [
            ("usb", "C2681563", "MINI 5PTP"),
            ("bat", "C160352", "B2B-PH-SM4-TB(LF)(SN)"),
            ("power", "C145857", "K3-1270S-F2"),
            ("boot", "C145857", "K3-1270S-F2"),
            ("reset", "C318884", "TS-1187A-B-A-B"),
            ("user", "C318884", "TS-1187A-B-A-B"),
            ("xtal", "C115962", "X50328MSB2GI"),
            ("l1", "C668312", "DFE201612E-R47M=P2"),
            ("charge", "C72043", "19-217/GHC-YR1S2/3T"),
            ("led2", "C72041", "19-217/BHC-ZL1M2RY/3T"),
        ] {
            let row = sourced(path);
            assert_eq!(row.lcsc.as_deref(), Some(lcsc), "{path}");
            assert_eq!(row.mpn.as_deref(), Some(mpn), "{path}");
        }
        // The headers have no number in ORBIT's schematic and are soldered by hand.
        for path in ["west", "east"] {
            assert_eq!(sourced(path).assembly, netsmith_design::Assembly::Hand);
        }
        // Every part has the designator letters of ORBIT's (U10, the crystal, is `Y` here: the
        // standard crystal's letter).
        for (path, letters) in [
            ("usb", "J"),
            ("bat", "J"),
            ("power", "SW"),
            ("reset", "SW"),
            ("l1", "L"),
            ("charge", "D"),
            ("xtal", "Y"),
        ] {
            assert!(
                part(&built, path).refdes.trim_end_matches(char::is_numeric) == letters,
                "{path}: {}",
                part(&built, path).refdes
            );
        }
    }

    #[test]
    fn the_pin_tables_name_every_pad_of_their_footprints_once() {
        let libraries = libraries();
        let built = built();
        for (path, kind, package) in [
            ("usb", "ShouHanMini5ptp", "SMD"),
            ("bat", "JstB2bPhSm4Tb", "SMD,P=2mm"),
            ("power", "HroK3_1270", "SMD,8.8x2mm"),
            ("reset", "XkbTs1187a", "SMD-4P,5.1x5.1mm"),
        ] {
            let pads: Vec<&String> = part(&built, path)
                .pins
                .iter()
                .flat_map(|p| p.pads.iter())
                .collect();
            let unique: BTreeSet<&String> = pads.iter().copied().collect();
            assert_eq!(pads.len(), unique.len(), "{path}: a pad on two pins");
            let footprint = footprint(&libraries, kind, package);
            let footprint_pads: BTreeSet<String> = footprint
                .pad_numbers()
                .into_iter()
                .filter(|n| !n.is_empty())
                .map(str::to_string)
                .collect();
            let table_pads: BTreeSet<String> = unique.iter().map(|s| s.to_string()).collect();
            assert_eq!(table_pads, footprint_pads, "{path}");
        }
        assert!(part(&built, "usb").is_connector());
        assert!(part(&built, "bat").is_connector());
        // USB1's shell is one pin of four pads, BAT1's tabs and the switches' mounting posts
        // are one pin each.
        let pads = |path: &str, pin: &str| -> Vec<String> {
            part(&built, path)
                .pins
                .iter()
                .find(|p| p.datasheet == pin)
                .unwrap_or_else(|| panic!("{path} has no pin {pin}"))
                .pads
                .clone()
        };
        assert_eq!(pads("usb", "SHIELD"), ["6", "7", "8", "9"]);
        assert_eq!(pads("bat", "MP"), ["MP"]);
        assert_eq!(pads("power", "MOUNT"), ["4", "5"]);
        assert_eq!(pads("power", "COM"), ["3"]);
        assert_eq!(pads("reset", "A"), ["1"]);
        assert_eq!(pads("reset", "B"), ["2"]);
    }

    #[test]
    fn orbits_nets_agree_with_the_pins_of_each_part() {
        let nets = |reference: &str, pad: &str| -> BTreeSet<String> {
            reference_nets(reference)
                .get(pad)
                .unwrap_or_else(|| panic!("{reference} pad {pad} has no net"))
                .clone()
        };
        let only = |reference: &str, pad: &str, net: &str| {
            assert_eq!(nets(reference, pad), BTreeSet::from([net.to_string()]));
        };
        // USB1: VBUS, D-, D+, (ID), GND on pads 1 to 5 as the pin table has them.
        only("USB1", "1", "VBUS");
        only("USB1", "2", "/USB_D-");
        only("USB1", "3", "/USB_D+");
        only("USB1", "5", "GND");
        // BAT1: pad 1 is ground, pad 2 the battery's plus (through PWR1).
        only("BAT1", "1", "GND");
        only("BAT1", "2", "Net-(BAT1-Pin_2)");
        // The buttons: the pair of pad 1 and the pair of pad 2 are one net each.
        only("SW1", "1", "NRST");
        only("SW1", "2", "GND");
        only("SW2", "1", "+3.3V");
        only("SW2", "2", "Net-(C18-Pad1)");
        // The slide switches: the common terminal (pad 3) is the MCU's BOOT0 pin on BOOT0 and
        // the +BATT rail on PWR1, with pad 2 at +3.3 V on BOOT0 and pad 1 on the battery's
        // plus on PWR1.
        only("BOOT0", "2", "+3.3V");
        only("BOOT0", "3", "Net-(U16-BOOT0)");
        only("PWR1", "1", "Net-(BAT1-Pin_2)");
        only("PWR1", "3", "+BATT");
        // The headers: pin N is pad N, the board's signals are in the table.
        only("J1", "1", "GND");
        only("J1", "8", "+3.3V");
        only("J2", "8", "PA6");
    }

    #[test]
    fn the_rows_record_their_pages_and_leave_price_and_stock_empty() {
        for (file, lcsc, library) in [
            ("catalogs/usb_mini_b.csv", "C2681563", "Extended"),
            ("catalogs/jst_ph_smt.csv", "C160352", "Extended"),
            ("catalogs/hro_k3_1270.csv", "C145857", "Extended"),
            ("catalogs/xkb_ts_1187a.csv", "C318884", ""),
            ("catalogs/inductor.csv", "C668312", "Extended"),
            ("catalogs/everlight_19_217_ghc.csv", "C72043", "Extended"),
            ("catalogs/everlight_19_217_bhc.csv", "C72041", "Extended"),
        ] {
            let text = read(file);
            assert_eq!(text.lines().count(), 2, "{file}: one row");
            let mut reader = csv_row(&text);
            let mut cell = |name: &str| reader.remove(name).unwrap_or_default();
            assert_eq!(cell("lcsc"), lcsc, "{file}");
            assert_eq!(cell("library"), library, "{file}");
            assert_eq!(cell("price"), "", "{file}: price is not read");
            assert_eq!(cell("stock"), "", "{file}: stock is not read");
            let source = cell("source");
            assert!(
                source.contains(&format!("https://jlcpcb.com/partdetail/{lcsc}")),
                "{file}: the JLCPCB page"
            );
            assert!(
                source.contains("2026-11-10"),
                "{file}: the day the page was read"
            );
            assert!(source.contains("LCSC"), "{file}: LCSC's record");
        }
        let target = Target::resolve_from(&project(), None).unwrap();
        assert!(target.catalogs().is_ok());
    }

    /// The first row of a CSV text as column name to cell (quotes of cells that hold commas).
    fn csv_row(text: &str) -> BTreeMap<String, String> {
        fn split(line: &str) -> Vec<String> {
            let (mut cells, mut cell, mut quoted) = (Vec::new(), String::new(), false);
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                match (c, quoted) {
                    ('"', true) if chars.peek() == Some(&'"') => {
                        cell.push('"');
                        chars.next();
                    }
                    ('"', _) => quoted = !quoted,
                    (',', false) => cells.push(std::mem::take(&mut cell)),
                    (c, _) => cell.push(c),
                }
            }
            cells.push(cell);
            cells
        }
        let mut lines = text.lines();
        let header = split(lines.next().unwrap());
        let row = split(lines.next().unwrap());
        assert_eq!(header.len(), row.len(), "the row has the header's columns");
        header.into_iter().zip(row).collect()
    }
}
