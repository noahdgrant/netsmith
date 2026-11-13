//! `examples/power_manifold_charger/`: every part of the Power Manifold charger module's board
//! that is not one of the three hard ICs (those are in `power_manifold_ics.rs`): the connectors
//! (J2 to J6), the project's own parts (the MOSFET, the regulator, the comparator, the ESD
//! array, the thermistor), the standard kinds' rows that the project adds in its `catalogs/`
//! (capacitors, resistors, inductors, the fuse, the ferrite bead and the TVS diode) and their
//! footprints.
//!
//! The footprints of the reference's must-match parts (J2, J3 and J4) are compared pad for pad
//! with `reference/placements.toml` (number, centre, size, shape, kind, angle and drill, all
//! within 0.05 mm: the parity rule's tolerance). The others are not must-match parts: they
//! build, are sourced and are close to the reference's pads, and these tests say how close.
//!
//! A fixture board of the test's own (never the project's board) builds with the parts and no
//! KiCad install.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use netsmith::project::Target;
use netsmith::{Libraries, build_with};
use netsmith_core::PinKind;
use netsmith_kicad::install::Search;
use netsmith_pcb::{Footprint, PadKind, PadShape};
use netsmith_solve::Catalogs;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/power_manifold_charger")
}

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

/// A small board of our own, never the project's: one of each part, joined to a rail and to
/// headers, so that the parts, the catalogs and the `kicad.toml` rows build together.
const FIXTURE: &str = r#"
import src::antc1608_ntc::Antc1608Ntc
import src::bsc059n04ls6::Bsc059n04ls6
import src::gt_usb_7004::GtUsb7004
import src::lp2985::Lp2985
import src::pcie_x1_edge_fingers::PcieX1EdgeFingers
import src::tlv7022::Tlv7022
import src::usblc6_2sc6::Usblc62sc6
import std::parts::connector::{Header1x8, ProgrammingPads6}
import std::parts::diode::Tvs
import std::parts::mechanical::TestPoint
import std::parts::passive::{Capacitor, FerriteBead, Fuse, Inductor, Resistor}
import std::sources::Rail

board PartsFixture {
    .bus: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VBUS"
        .neg.net &= "GND"
    }
    .rail: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..10mA
    }

    // J4, U6 and D2: the receptacle, its ESD array and its TVS diode.
    .j4: GtUsb7004 { .mpn &= "GT-USB-7004" }
    .j4.vbus ~ .bus
    .j4.shell ~ .bus.neg
    .u6: Usblc62sc6 { .mpn &= "USBLC6-2SC6" }
    .u6.io1 ~ .j4.dp
    .u6.io1 ~ .j4.dm
    .u6.gnd ~ .bus.neg
    .d2: Tvs { .package &= "DFN1610-2" }
    .d2.cathode ~ .bus.pos
    .d2.anode ~ .bus.neg

    // U2: the regulator, its ON/OFF tied to its input.
    .u2: Lp2985 { .mpn &= "LP2985A-33DBVR" }
    .bus ~ .u2.vin
    .u2.on_off ~ .bus.pos
    .u2.vout ~ .rail

    // U4: the comparator, its open-drain outputs on one line with a pull-up.
    .u4: Tlv7022 { .mpn &= "TLV7022DDFR" }
    .cmp_pullup: Resistor { .resistance &= 10kohm +/- 1%; .package &= "0402" }
    .rail.pos ~> .cmp_pullup ~> .u4.out_a
    .rail ~ .u4.supply
    .cmp: Header1x8
    .u4.in_a_neg ~ .cmp.p1
    .u4.in_a_pos ~ .cmp.p2
    .u4.in_b_neg ~ .cmp.p1
    .u4.in_b_pos ~ .cmp.p3
    .u4.out_a ~ .cmp.p4
    .u4.out_b ~ .cmp.p4

    // Q1: the MOSFET, TH1: the thermistor.
    .q1: Bsc059n04ls6 { .mpn &= "BSC059N04LS6" }
    .q1.drain ~ .bus.pos
    .q1.source ~ .cmp.p5
    .q1.gate ~ .cmp.p6
    .th1: Antc1608Ntc { .mpn &= "ANTC1608-103F3380FB" }
    .th1.a ~ .cmp.p7
    .th1.b ~ .bus.neg

    // J2, J3, J1: the programming pads, the card edge and a test pad.
    .j2: ProgrammingPads6
    .j2.p1 ~ .rail.pos
    .j2.p2 ~ .cmp.p7
    .j2.p3 ~ .cmp.p6
    .j2.p4 ~ .cmp.p5
    .j2.p5 ~ .bus.neg
    .j2.p6 ~ .cmp.p8
    .j3: PcieX1EdgeFingers
    .j3.a2 ~ .bus.pos
    .j3.a8 ~ .bus.neg
    .j3.b1 ~ .bus.neg
    .j3.a1 ~ .j3.b17
    .j1: TestPoint { .package &= "D1.5mm" }
    .j1.p1 ~ .cmp.p8
}
"#;

/// The project's own catalog files alone, with no standard row: a generic's pick can only
/// come from a row of the project.
const GENERICS: &str = r#"
import std::parts::diode::Tvs
import std::parts::passive::{Capacitor, Dielectric, FerriteBead, Fuse, Inductor, Resistor}
import std::sources::Rail

board GenericRows {
    .c_fh: Capacitor { .capacitance &= 4.7uF +/- 10%; .package &= "0603" }
    .c_x7r_0805: Capacitor { .capacitance &= 10uF +/- 10%; .package &= "0805"; .max_voltage >= 16V; .dielectric &= Dielectric::X7R }
    .c_x5r_0805: Capacitor { .capacitance &= 10uF +/- 10%; .package &= "0805"; .max_voltage >= 16V; .dielectric &= Dielectric::X5R }
    .c_cog: Capacitor { .capacitance &= 330pF +/- 5%; .package &= "0402" }
    .c_1u: Capacitor { .capacitance &= 1uF +/- 10%; .package &= "0603" }
    .c_2u2: Capacitor { .capacitance &= 2.2uF +/- 10%; .package &= "1206" }
    .c_10u: Capacitor { .capacitance &= 10uF +/- 10%; .package &= "1206" }
    .c_polymer: Capacitor { .capacitance &= 220uF +/- 20%; .package &= "SMD,D8xL9mm" }
    .c_elec: Capacitor { .capacitance &= 100uF +/- 20%; .package &= "SMD,D6.3xL7.7mm" }
    .r_49k9: Resistor { .resistance &= 49.9kohm +/- 1%; .package &= "0402" }
    .r_1: Resistor { .resistance &= 1ohm +/- 1%; .package &= "0402" }
    .r_45k3: Resistor { .resistance &= 45.3kohm +/- 1%; .package &= "0402" }
    .r_56k2: Resistor { .resistance &= 56.2kohm +/- 1%; .package &= "0402" }
    .r_57k6: Resistor { .resistance &= 57.6kohm +/- 1%; .package &= "0402" }
    .r_7m: Resistor { .resistance &= 7mohm +/- 1%; .package &= "1206" }
    .r_10m: Resistor { .resistance &= 10mohm +/- 1%; .package &= "1206" }
    .l1: Inductor { .inductance &= 1uH +/- 20%; .package &= "SMD,7x6.6mm" }
    .l2: Inductor { .inductance &= 5.6uH +/- 20%; .package &= "SMD,11.5x10mm" }
    .f1: Fuse { .package &= "2410" }
    .fb1: FerriteBead { .impedance &= 600ohm +/- 25%; .package &= "0603" }
    .d2: Tvs { .package &= "DFN1610-2" }
}
"#;

/// A machine with no KiCad install, and the project's own `kicad.toml` and libraries.
fn libraries() -> Libraries {
    let nowhere = netsmith_testkit::TempDir::new("power-manifold-parts-no-kicad");
    Libraries::for_project_by(Some(&project()), &Search::under(nowhere.path()))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn target() -> Target {
    Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"))
}

fn built() -> netsmith::Built {
    let target = target();
    build_with(
        "parts.nsm",
        FIXTURE,
        &target.loader().unwrap(),
        &target.catalogs().unwrap(),
        &libraries(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0)
}

/// The project's catalog files and nothing else.
fn project_catalogs() -> Catalogs {
    let project = target().project.expect("the folder is a project");
    let mut catalogs = Catalogs::empty();
    for file in project.catalog_files().unwrap_or_else(|e| panic!("{e}")) {
        let text = fs::read_to_string(&file.path).unwrap();
        catalogs
            .add_file(&file.name, &file.file, &text)
            .unwrap_or_else(|e| panic!("{e}"));
    }
    catalogs
}

fn built_generics() -> netsmith::Built {
    let target = target();
    build_with(
        "generics.nsm",
        GENERICS,
        &target.loader().unwrap(),
        &project_catalogs(),
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
/// footprint, with `tolerance` mm on centre, size and drill: number, centre in the footprint's
/// own frame, size, drill, shape, kind and angle. Empty when they are the same. A `connect` pad
/// of the file (an edge-connector pad: copper on one face) is a surface pad. A footprint that the
/// reference has on the back face (`B.Cu`) is stored as KiCad's flip leaves it: every y negated
/// and every layer on the other face (`fab` writes netsmith's back-face footprints the same way,
/// #553). The project's footprint of it is drawn as before the flip, so y is read negated and a
/// pad's copper layers are read on the other face.
fn pad_differences(
    footprint: &Footprint,
    table: &toml::Table,
    reference: &str,
    tolerance: f64,
) -> Vec<String> {
    let part = *tables(table, "part")
        .iter()
        .find(|p| p["reference"].as_str() == Some(reference))
        .unwrap_or_else(|| panic!("no part {reference}"));
    let rotation = number(part, "rotation");
    let mirror = if part["layer"].as_str() == Some("B.Cu") {
        -1.0
    } else {
        1.0
    };
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
            number(pad, "y") * mirror,
        );
        let found = footprint.pads.iter().enumerate().find(|(i, own)| {
            !used[*i]
                && own.number == n
                && (own.at.x - x).abs() <= tolerance
                && (own.at.y - y).abs() <= tolerance
        });
        let Some((i, own)) = found else {
            differences.push(format!(
                "{reference} pad {n:?}: none at ({x}, {y}) within {tolerance} mm; pads of that number are at {:?}",
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
        if (own.width - w).abs() > tolerance || (own.height - h).abs() > tolerance {
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
            "smd" | "connect" => PadKind::Smd,
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
        // The file's pad angle includes the footprint's rotation, and a flip negates the pad's
        // own; a turn of 180 degrees leaves a rectangle, a circle or an oval as it was.
        let turn =
            pad.get("angle").map_or(0.0, |_| number(pad, "angle")) - rotation - own.angle * mirror;
        let off = turn.rem_euclid(180.0).min(180.0 - turn.rem_euclid(180.0));
        if off > 1e-6 {
            differences.push(format!(
                "{at}: angle {} differs from the reference's",
                own.angle
            ));
        }
        // The copper layers, on the face `fab` puts them on (#537).
        let face = |layer: &str| match layer.split_once('.') {
            Some(("F", rest)) if mirror < 0.0 => format!("B.{rest}"),
            Some(("B", rest)) if mirror < 0.0 => format!("F.{rest}"),
            _ => layer.to_string(),
        };
        let mut own_copper: Vec<String> = own
            .layers
            .iter()
            .filter(|l| l.ends_with(".Cu"))
            .map(|l| face(l))
            .collect();
        let mut want_copper: Vec<String> = pad["layers"]
            .as_array()
            .unwrap_or_else(|| panic!("{at}: the table has no layers"))
            .iter()
            .map(|l| l.as_str().unwrap().to_string())
            .collect();
        own_copper.sort();
        want_copper.sort();
        if own_copper != want_copper {
            differences.push(format!(
                "{at}: copper on {own_copper:?} on the board, the reference's {want_copper:?}"
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
                if (d.width - a).abs() <= tolerance && (d.height - b).abs() <= tolerance => {}
            (d, w) => differences.push(format!(
                "{at}: drill {:?}, the reference's {w:?}",
                d.map(|d| (d.width, d.height))
            )),
        }
    }
    differences
}

/// The pads of each reference part of the table, by designator.
const PARTS_BY_FOOTPRINT: [(&str, &str, &str); 3] = [
    ("J2", "ProgrammingPads6", "TC2030-NL"),
    ("J3", "PcieX1EdgeFingers", "PCIe-x1-edge"),
    ("J4", "GtUsb7004", "SMD"),
];

#[test]
fn the_fixture_builds_with_no_kicad() {
    // The build ran `validate_pinout` on each part (a pad of the pin table that the footprint
    // lacks, or a footprint pad that no pin names, stops it) and wrote the schematic with
    // symbols drawn from the pin tables.
    let built = built();
    assert!(!built.schematic.is_empty());
    for kind in [
        "GtUsb7004",
        "PcieX1EdgeFingers",
        "Bsc059n04ls6",
        "Lp2985",
        "Usblc62sc6",
        "Tlv7022",
        "Antc1608Ntc",
        "ProgrammingPads6",
        "TestPoint",
        "Tvs",
    ] {
        part_of(&built, kind);
    }
}

#[test]
fn every_catalog_row_of_the_parts_is_offered_and_sourced() {
    let built = built();
    for (kind, lcsc) in [
        ("GtUsb7004", "C2686968"),
        ("Bsc059n04ls6", "C534356"),
        ("Lp2985", "C89826"),
        ("Usblc62sc6", "C2687116"),
        ("Tlv7022", "C2862588"),
        ("Antc1608Ntc", "C52155402"),
        ("Tvs", "C1974707"),
    ] {
        let sourced = built
            .sourcing
            .iter()
            .find(|s| s.kind == kind)
            .unwrap_or_else(|| panic!("{kind} is not in the bill of materials"));
        assert_eq!(sourced.lcsc.as_deref(), Some(lcsc), "{kind}: {sourced:?}");
    }
}

#[test]
fn the_card_edge_and_the_test_pad_are_assembled_by_nobody_and_have_no_lcsc_number() {
    let built = built();
    for kind in ["PcieX1EdgeFingers", "ProgrammingPads6", "TestPoint"] {
        let sourced = built.sourcing.iter().find(|s| s.kind == kind);
        if let Some(sourced) = sourced {
            assert_eq!(sourced.lcsc, None, "{kind}");
            assert_eq!(sourced.assembly, netsmith_design::Assembly::None, "{kind}");
        }
    }
}

#[test]
fn the_standard_kinds_take_the_projects_rows_where_the_standard_ones_have_none() {
    let built = built_generics();
    let picked: BTreeSet<String> = built
        .sourcing
        .iter()
        .filter_map(|s| s.lcsc.clone())
        .collect();
    let expected: BTreeSet<String> = [
        "C108342",
        "C95841",
        "C1713",
        "C13533",
        "C5199872",
        "C50254",
        "C89632",
        "C46550469",
        "C3151829",
        "C25897",
        "C25086",
        "C2933099",
        "C54968",
        "C2960799",
        "C7420011",
        "C105362",
        "C112125",
        "C17700183",
        "C142876",
        "C85833",
        "C1974707",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(picked, expected, "the rows the fixture picked");
}

#[test]
fn the_receptacle_and_the_card_edge_are_connectors_with_the_pins_of_their_pads() {
    let built = built();
    let usb = part_of(&built, "GtUsb7004");
    assert!(usb.is_connector());
    assert!(usb.refdes.starts_with('J'), "{}", usb.refdes);
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
    assert_eq!(pads("VBUS"), ["A4", "A9", "B4", "B9"]);
    assert_eq!(pads("GND"), ["A1", "A12", "B1", "B12"]);
    assert_eq!(pads("D+"), ["A6", "B6"]);
    assert_eq!(pads("D-"), ["A7", "B7"]);
    assert_eq!(pads("CC1"), ["A5"]);
    assert_eq!(pads("CC2"), ["B5"]);
    assert_eq!(pads("SBU1"), ["A8"]);
    assert_eq!(pads("SBU2"), ["B8"]);
    assert_eq!(pads("SHELL"), ["S1"]);

    let edge = part_of(&built, "PcieX1EdgeFingers");
    assert!(edge.is_connector());
    assert_eq!(edge.pins.len(), 36, "one pin for each finger");
    let all: BTreeSet<&str> = edge
        .pins
        .iter()
        .flat_map(|p| p.pads.iter().map(String::as_str))
        .collect();
    let want: BTreeSet<String> = ["A", "B"]
        .iter()
        .flat_map(|s| (1..=18).map(move |n| format!("{s}{n}")))
        .collect();
    assert_eq!(
        all,
        want.iter().map(String::as_str).collect::<BTreeSet<_>>(),
        "pads A1 to A18 and B1 to B18"
    );
    assert!(edge.pins.iter().all(|p| p.kind == PinKind::Passive));
}

#[test]
fn the_pins_of_the_parts_are_the_datasheets() {
    let built = built();
    let pins = |kind: &str| -> Vec<(String, Vec<String>)> {
        part_of(&built, kind)
            .pins
            .iter()
            .map(|p| (p.datasheet.clone(), p.pads.clone()))
            .collect()
    };
    let owned = |v: &[(&str, &[&str])]| -> Vec<(String, Vec<String>)> {
        v.iter()
            .map(|(n, p)| (n.to_string(), p.iter().map(|s| s.to_string()).collect()))
            .collect()
    };
    // The MOSFET: pins 1 to 3 the source, 4 the gate, the drain the one pad 5.
    assert_eq!(
        pins("Bsc059n04ls6"),
        owned(&[("G", &["4"]), ("S", &["1", "2", "3"]), ("D", &["5"])])
    );
    // The regulator: VIN 1, GND 2, ON/OFF 3, BYPASS 4, VOUT 5.
    assert_eq!(
        pins("Lp2985"),
        owned(&[
            ("VIN", &["1"]),
            ("GND", &["2"]),
            ("ON/OFF", &["3"]),
            ("BYPASS", &["4"]),
            ("VOUT", &["5"]),
        ])
    );
    // The ESD array: pads 1 and 6 are one line inside and two pins here (the design leaves 6 open);
    // pads 3 and 4 are one pin.
    assert_eq!(
        pins("Usblc62sc6"),
        owned(&[
            ("I/O1", &["1"]),
            ("I/O1", &["6"]),
            ("GND", &["2"]),
            ("I/O2", &["3", "4"]),
            ("VBUS", &["5"]),
        ])
    );
    // The comparator, in the order of the datasheet's pads.
    let mut cmp = pins("Tlv7022");
    cmp.sort_by_key(|(_, pads)| pads[0].parse::<u32>().unwrap());
    assert_eq!(
        cmp,
        owned(&[
            ("OUTA", &["1"]),
            ("INA-", &["2"]),
            ("INA+", &["3"]),
            ("VEE", &["4"]),
            ("INB+", &["5"]),
            ("INB-", &["6"]),
            ("OUTB", &["7"]),
            ("VCC", &["8"]),
        ])
    );
    // The thermistor and the TVS diode: two terminals each.
    let mut ntc = pins("Antc1608Ntc");
    ntc.sort();
    assert_eq!(ntc, owned(&[("A", &["1"]), ("B", &["2"])]));
}

#[test]
fn the_pads_of_each_must_match_footprint_are_those_of_the_reference_board() {
    let libraries = libraries();
    let table = parsed(&read("reference/placements.toml"));
    let mut differences = Vec::new();
    for (reference, kind, package) in PARTS_BY_FOOTPRINT {
        let footprint = footprint(&libraries, kind, package);
        differences.extend(pad_differences(&footprint, &table, reference, 0.05));
    }
    // The test pads are not must-match parts; the standard footprint is the reference's.
    let pad = footprint(&libraries, "TestPoint", "D1.5mm");
    for reference in ["J1", "J5", "J6"] {
        differences.extend(pad_differences(&pad, &table, reference, 0.05));
    }
    assert!(
        differences.is_empty(),
        "the footprints differ from the reference's pads:\n{}",
        differences.join("\n")
    );
    // The numbers: the receptacle 16 contacts, four shell slots and two unnumbered pegs, the
    // card edge 36 fingers, the Tag-Connect pads 6 and 3 unnumbered holes.
    let usb = footprint(&libraries, "GtUsb7004", "SMD");
    assert_eq!(usb.pads.len(), 22);
    assert_eq!(usb.pad_numbers().len(), 18, "16 contacts, S1 and no number");
    let edge = footprint(&libraries, "PcieX1EdgeFingers", "PCIe-x1-edge");
    assert_eq!(edge.pads.len(), 36);
    let tag = footprint(&libraries, "ProgrammingPads6", "TC2030-NL");
    assert_eq!(tag.pads.len(), 9);
}

#[test]
fn the_comparison_of_pads_says_what_differs() {
    // Not vacuous: a pad moved by 0.1 mm, one of the wrong size and a slot of another length
    // are each named.
    let libraries = libraries();
    let table = parsed(&read("reference/placements.toml"));
    let mut usb = footprint(&libraries, "GtUsb7004", "SMD");
    usb.pads[0].at.x += 0.1;
    usb.pads[1].width = 0.8;
    let slot = usb.pads.iter_mut().find(|p| p.number == "S1").unwrap();
    slot.drill = Some(netsmith_pcb::Drill {
        width: 0.6,
        height: 1.0,
    });
    let differences = pad_differences(&usb, &table, "J4", 0.05);
    assert_eq!(differences.len(), 3, "{differences:?}");
    assert!(differences[0].contains("none at"), "{differences:?}");
    assert!(differences[1].contains("size 0.8"), "{differences:?}");
    assert!(differences[2].contains("drill"), "{differences:?}");
}

#[test]
fn every_footprint_of_the_projects_libraries_loads_and_none_draws_the_boards_outline() {
    let libraries = libraries();
    for library in [
        "power_manifold_must_match.pretty",
        "power_manifold_parts.pretty",
    ] {
        let dir = project().join("libs").join(library);
        let mut seen = 0;
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let text = fs::read_to_string(&path).unwrap();
            assert!(
                !text.contains("Edge.Cuts"),
                "{} draws a line of the board's outline",
                path.display()
            );
            let id = format!(
                "{}:{}",
                library.trim_end_matches(".pretty"),
                path.file_stem().unwrap().to_string_lossy()
            );
            let loaded = libraries
                .footprints
                .load(&id)
                .unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(!loaded.pads.is_empty(), "{id} has no pad");
            seen += 1;
        }
        assert!(seen > 0, "{library} is empty");
    }
}

/// The footprints of the parts that are not must-match are written from a maker's land pattern
/// or from the reference board's pads, and are not required to equal them: the largest
/// difference of a numbered pad's centre is bounded here, for each, with the reason.
#[test]
fn the_other_footprints_are_close_to_the_reference_boards_pads() {
    let libraries = libraries();
    let table = parsed(&read("reference/placements.toml"));
    // (reference designator, kind, package, bound in mm). A footprint with a number the
    // reference lacks or the other way round is a difference of its own.
    let cases = [
        // The reference's pads exactly.
        ("C39", "Capacitor", "SMD,D8xL9mm", 0.011),
        ("C6", "Capacitor", "SMD,D6.3xL7.7mm", 0.011),
        ("F1", "Fuse", "2410", 0.011),
        ("L1", "Inductor", "SMD,7x6.6mm", 0.011),
        ("L2", "Inductor", "SMD,11.5x10mm", 0.011),
        // ST's footprint recommendation: pads 0.55 x 0.80 mm at +-0.625 mm, where the
        // reference's are 0.6 x 0.9 mm (turned a quarter) at +-0.52 mm.
        ("D2", "Tvs", "DFN1610-2", 0.11),
        // TI's example board layout: 1.05 x 0.45 mm pads 2.6 mm apart (centres at +-1.3 mm and
        // +-0.325, +-0.975 mm), where the reference's are 1.252 x 0.35 mm oval pads 2.46 mm
        // apart, at +-1.23 mm and +-0.32, +-0.33, +-0.98 mm.
        // The reference's numbered pads exactly (it also has paste-only pads, unnumbered).
        ("U4", "Tlv7022", "SOT-23-THIN-8", 0.075),
        ("Q1", "Bsc059n04ls6", "TDSON-8FL", 0.011),
    ];
    for (reference, kind, package, bound) in cases {
        let footprint = footprint(&libraries, kind, package);
        let part = *tables(&table, "part")
            .iter()
            .find(|p| p["reference"].as_str() == Some(reference))
            .unwrap();
        let rotation = number(part, "rotation");
        let _ = rotation;
        let numbered: Vec<&toml::Table> = part["pads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_table().unwrap())
            .filter(|p| !p["number"].as_str().unwrap().is_empty())
            .collect();
        let own: Vec<_> = footprint
            .pads
            .iter()
            .filter(|p| !p.number.is_empty())
            .collect();
        assert_eq!(own.len(), numbered.len(), "{reference}: numbered pads");
        for pad in numbered {
            let n = pad["number"].as_str().unwrap();
            let (x, y) = (number(pad, "x"), number(pad, "y"));
            let nearest = own
                .iter()
                .filter(|p| p.number == n)
                .map(|p| (p.at.x - x).abs().max((p.at.y - y).abs()))
                .fold(f64::INFINITY, f64::min);
            assert!(
                nearest <= bound,
                "{reference} pad {n}: the nearest of the footprint's is {nearest} mm away (bound {bound})"
            );
        }
    }
}

#[test]
fn the_generator_footprints_are_the_reference_boards_footprints() {
    // Where the project takes a generator's footprint, it is the reference's by name.
    let libraries = libraries();
    for (kind, package, name) in [
        ("Lp2985", "SOT-23-5", "Package_TO_SOT_SMD:SOT-23-5"),
        ("Usblc62sc6", "SOT-23-6", "Package_TO_SOT_SMD:SOT-23-6"),
        ("Antc1608Ntc", "0603", "Resistor_SMD:R_0603_1608Metric"),
        ("Resistor", "1206", "Resistor_SMD:R_1206_3216Metric"),
    ] {
        let mapped = libraries.mapping.map(kind, Some(package)).unwrap();
        assert_eq!(mapped.footprint, name, "{kind}");
    }
}
