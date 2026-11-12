//! `examples/power_manifold_charger/`: the home of the port of the Power Manifold charger module
//! (the fourth showcase board of epic #308), its licence and its reference table
//! (`reference/placements.toml`), read from the author's board file by
//! `cargo run -p netsmith --example reference_table` (`netsmith::reference_table`).

use std::fs;
use std::path::PathBuf;

use netsmith::outcome_with;
use netsmith::project::Target;
use netsmith::reference_table::{MustMatch, placements_toml_with};
use netsmith_netlist::Netlist;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/power_manifold_charger")
}

const REFERENCE: &str = "charger-module.kicad_pcb";

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
         reference/{REFERENCE} > reference/placements.toml` from examples/power_manifold_charger"
    );
}

#[test]
fn the_table_holds_the_facts_of_the_charger_board() {
    let table = parsed(&read("reference/placements.toml"));
    assert_eq!(table["source"]["footprints"].as_integer(), Some(90));
    assert_eq!(table["source"]["version"].as_str(), Some("20260206"));
    assert_eq!(table["source"]["rev"].as_str(), Some("v0.16.2"));

    // The outline: four lines on the board for the body (x 63.2 to 156, y 26 to 50) and the
    // lines and the arc of the edge connector's footprint (J3), which draws the card's tab and
    // its key slot to x 54.8.
    let outline = table["outline"].as_table().unwrap();
    for (key, value) in [
        ("min_x", 54.8),
        ("min_y", 26.0),
        ("max_x", 156.0),
        ("max_y", 50.0),
        ("width", 101.2),
        ("height", 24.0),
    ] {
        assert!((number(outline, key) - value).abs() < 1e-3, "{key}");
    }
    assert_eq!(outline["lines"].as_integer(), Some(14));
    assert_eq!(outline["arcs"].as_integer(), Some(1));

    // The non-plated holes: the three of the programming pads (J2) and the two pegs of the
    // USB-C receptacle (J4).
    let holes: Vec<_> = tables(&table, "hole")
        .into_iter()
        .map(|h| (h["reference"].as_str().unwrap(), number(h, "drill")))
        .collect();
    assert_eq!(
        holes,
        [
            ("J2", 0.9906),
            ("J2", 0.9906),
            ("J2", 0.9906),
            ("J4", 0.6),
            ("J4", 0.6)
        ]
    );

    let parts = tables(&table, "part");
    assert_eq!(parts.len(), 90);
    let find = |reference: &str| {
        *parts
            .iter()
            .find(|p| p["reference"].as_str() == Some(reference))
            .unwrap_or_else(|| panic!("no part {reference}"))
    };
    for (reference, x, y, rotation, layer, pads) in [
        ("J2", 137.3, 47.3, 0.0, "F.Cu", 9),
        ("J3", 58.25, 45.65, 90.0, "B.Cu", 36),
        ("J4", 152.13, 38.0, 90.0, "F.Cu", 22),
        ("U1", 125.12, 40.45, -90.0, "F.Cu", 48),
        ("U3", 94.74, 34.6, 90.0, "F.Cu", 26),
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
        assert_eq!(part["layer"].as_str(), Some(layer), "{reference}");
        assert_eq!(part["pads"].as_array().unwrap().len(), pads, "{reference}");
    }

    // Must match: the outline and the holes (above), the programming pads, the edge connector
    // and the USB-C receptacle.
    let must: Vec<_> = parts
        .iter()
        .filter(|p| p["must_match"].as_bool() == Some(true))
        .map(|p| p["reference"].as_str().unwrap())
        .collect();
    assert_eq!(must, ["J2", "J3", "J4"]);
    assert_eq!(table["outline"]["must_match"].as_bool(), Some(true));
    assert!(
        tables(&table, "hole")
            .iter()
            .all(|h| h["must_match"].as_bool() == Some(true))
    );
}

#[test]
fn the_board_file_reads_as_a_netlist_of_89_parts() {
    let netlist = Netlist::from_kicad_pcb(&read(&format!("reference/{REFERENCE}")))
        .unwrap_or_else(|e| panic!("{e}"));
    // 90 footprints, one of them the HOT_SURFACE marker (`REF**`, no pad).
    assert_eq!(netlist.components.len(), 89);
    // An unconnected pad is on no net.
    assert!(
        netlist
            .nets
            .iter()
            .all(|n| !n.name.starts_with("unconnected-"))
    );
    let net = |name: &str| {
        netlist
            .nets
            .iter()
            .find(|n| n.name == name)
            .unwrap_or_else(|| panic!("no net {name}"))
    };
    // The USB-C receptacle's four data pads (A6, A7, B6, B7) are two nets of two pins, and the
    // supply of the module is the net of the edge connector's 5 V pins.
    assert!(
        net("/USB_DATA")
            .pins
            .iter()
            .any(|(c, p)| netlist.components[*c].refdes == "J4" && p == "A6")
    );
    assert!(
        net("+5V")
            .pins
            .iter()
            .any(|(c, p)| netlist.components[*c].refdes == "J3" && p == "A12")
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
    assert!(readme.contains("https://github.com/mikesmitty/power-manifold"));
    assert!(readme.contains("CERN-OHL-P-2.0"));
    assert!(readme.contains("dd24000a9ce0cac567da674c473ded4cff34bdbc"));
    assert!(readme.contains("Copyright 2024-2026 Michael Smith"));
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
    assert_eq!(names, ["PowerManifoldCharger"]);
}

/// The nets of the port against the charger module's board file, and what the solver checks.
mod board {
    use std::collections::{BTreeMap, BTreeSet};

    use netsmith::project::Target;
    use netsmith::{Libraries, build_with};
    use netsmith_kicad::install::Search;
    use netsmith_kicad::testlib;
    use netsmith_netlist::normalize_value;

    use crate::common::port::{Pad, Port};

    use super::{REFERENCE, project, read};

    /// The port: `reference/parts.toml` pairs each of the charger module's 89 parts with a member
    /// of the board. The members are named by what the part does, so each pair is one to one.
    const PORT: Port = Port {
        folder: "power_manifold_charger",
        board: REFERENCE,
        original: "the charger board",
        swapped: &[],
        renumbered: &[],
        no_nets: &[],
        own: &[],
    };

    fn libraries() -> Libraries {
        let nowhere = testlib::TempDir::new("power-manifold-board-no-kicad");
        Libraries::for_project_by(Some(&project()), &Search::under(nowhere.path()))
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn built() -> netsmith::Built {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        let source = &target.files[0];
        // The rows of the picks file are the design's, as for `netsmith build` (#462).
        let catalogs = netsmith::picks::holding(&source.path, &target.catalogs().unwrap())
            .unwrap_or_else(|e| panic!("{e}"));
        build_with(
            &source.name,
            &read("src/main.nsm"),
            &target.loader().unwrap(),
            &catalogs,
            &libraries(),
        )
        .map(|mut boards| boards.remove(0))
        .unwrap_or_else(|e| panic!("{e:?}"))
    }

    #[test]
    fn the_board_builds_with_no_kicad_and_has_the_89_parts_of_the_charger_board() {
        PORT.assert_has_the_parts(&built(), 89, 89);
    }

    /// Pads that share a number are one pin in both netlists.
    #[test]
    fn every_pad_has_the_same_pads_on_its_net_as_in_the_charger_board() {
        PORT.assert_same_pads_on_nets(&built(), 200);
    }

    #[test]
    fn netsmith_compare_says_the_nets_are_the_same() {
        PORT.assert_compare_matches(&built());
    }

    /// The comparison would see a wrong net: it is the comparison above that runs, on a board
    /// whose U1 pad 13 (PA2, the USART's TX) is said to be on the net of pad 14, and its answer
    /// is the pad.
    #[test]
    fn the_comparison_sees_a_pad_on_the_wrong_net() {
        PORT.assert_sees_a_wrong_net(&built(), ("U1", "13"), ("U1", "14"));
    }

    #[test]
    fn the_resistors_and_capacitors_have_the_charger_boards_values_and_packages() {
        let built = built();
        let reference = PORT.reference();
        let mut wrong = Vec::new();
        let mut checked = 0;
        for (path, name) in PORT
            .parts()
            .iter()
            .filter(|(_, r)| r.starts_with('R') || r.starts_with('C'))
        {
            let part = built.design.parts.iter().find(|p| p.path == path).unwrap();
            let theirs = &reference.components[reference.find(name).unwrap()];
            // "1R" is 1 ohm, "R007" 7 milliohm and "R010" 10 milliohm in their value field.
            let value = match theirs.value.as_str() {
                "R007" => "7m".to_string(),
                "R010" => "10m".to_string(),
                "1R" => "1".to_string(),
                other => other.to_string(),
            };
            if normalize_value(&part.value, true) != normalize_value(&value, true) {
                wrong.push(format!(
                    "{name}: value {} against {}",
                    part.value, theirs.value
                ));
            }
            let package = [
                ("0402", "0402"),
                ("0603", "0603"),
                ("0805", "0805"),
                ("1206", "1206"),
                ("CP_Elec_6.3x7.7", "SMD,D6.3xL7.7mm"),
                ("CP_Elec_8x10", "SMD,D8xL9mm"),
            ]
            .iter()
            .find(|(fp, _)| theirs.footprint.contains(fp))
            .unwrap_or_else(|| panic!("{name}: footprint {}", theirs.footprint))
            .1;
            if part.package.as_deref() != Some(package) {
                wrong.push(format!(
                    "{name}: package {:?} against {package}",
                    part.package
                ));
            }
            checked += 1;
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
        assert_eq!(checked, 65, "27 resistors and 38 capacitors");
    }

    /// The pads that the reference leaves open (its `unconnected-(...)` nets) are the pads that no
    /// net of the design names.
    #[test]
    fn the_open_pins_are_the_ones_the_reference_leaves_open() {
        let built = built();
        let to_reference = PORT.designators(&built);
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
        assert_eq!(theirs.len(), 26, "{theirs:?}");
        let design = &built.design;
        let mut on_net: BTreeSet<Pad> = BTreeSet::new();
        for net in &design.nets {
            for p in &net.pins {
                on_net.insert((to_reference[&p.refdes].clone(), p.pin.clone()));
            }
        }
        let mut ours = BTreeSet::new();
        for part in &design.parts {
            for pin in &part.pins {
                for pad in &pin.pads {
                    let key = (to_reference[&part.refdes].clone(), pad.clone());
                    if !on_net.contains(&key) {
                        ours.insert(key);
                    }
                }
            }
        }
        assert_eq!(ours, theirs, "ours {ours:?}");
    }

    /// The LCSC number of each footprint of the reference board, read from its `LCSC` property
    /// (the board file is the evidence: the design has no separate bill of materials).
    fn bom_numbers() -> BTreeMap<String, String> {
        let text = read(&format!("reference/{REFERENCE}"));
        let mut out = BTreeMap::new();
        for block in text.split("\n\t(footprint ").skip(1) {
            let property = |name: &str| {
                let key = format!("(property \"{name}\" \"");
                block.find(&key).map(|i| {
                    let rest = &block[i + key.len()..];
                    rest[..rest.find('"').unwrap()].to_string()
                })
            };
            if let (Some(reference), Some(lcsc)) = (property("Reference"), property("LCSC"))
                && !lcsc.is_empty()
            {
                out.insert(reference, lcsc);
            }
        }
        out
    }

    /// The parts whose pick is another row than the board's LCSC number: none. The design states
    /// a value, a package and what the rail needs, not a part number, and the solver picks the
    /// first row that fits; for 19 ceramics that is another row than the board's, and
    /// `src/main.picks.toml` holds them to the board's numbers (#462).
    const OTHER_ROW: &[&str] = &[];

    /// Every part of the board that has a number is sourced with that LCSC number.
    #[test]
    fn the_parts_have_the_lcsc_numbers_of_the_charger_boards_footprints() {
        let built = built();
        let to_reference = PORT.designators(&built);
        let theirs = bom_numbers();
        assert_eq!(theirs.len(), 84, "84 parts carry an LCSC property");
        let mut other = Vec::new();
        for s in &built.sourcing {
            let reference = &to_reference[&s.refdes];
            if let Some(number) = theirs.get(reference)
                && s.lcsc.as_deref() != Some(number.as_str())
            {
                other.push(reference.as_str());
            }
        }
        other.sort_by_key(|r| r[1..].parse::<u32>().unwrap_or(0));
        assert_eq!(other, OTHER_ROW);
    }

    #[test]
    fn the_electrical_rules_check_of_kicad_passes_on_the_schematic() {
        let built = built();
        let Some(found) = crate::erc::erc(&built.design.name, &built.schematic) else {
            return;
        };
        // The design ties PA9 and PA10 of the microcontroller (U1 pads 29 and 32) to ground. The
        // port's pins are the datasheet's (bidirectional), which the project's ERC pin map accepts
        // on a net with a power flag, as netsmith's pin rules do (#461).
        assert_eq!(
            crate::erc::with_the_machines_libraries(found),
            Vec::<String>::new()
        );
    }

    /// The pin optimizer (ADR 0101) moves only a pin that a request got. The board has none:
    /// every pin of the microcontroller and of the edge connector is joined to the pin the
    /// design joins it to, and the check's `spaces` for the board are empty.
    #[test]
    fn the_optimizer_cannot_move_a_pin() {
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
            .get("PowerManifoldCharger")
            .map_or(0, |parts| {
                parts.iter().map(|s| s.requests.len()).sum::<usize>()
            });
        assert_eq!(requests, 0, "the board asks for a pin somewhere");
        assert!(
            outcome
                .pins
                .file
                .boards
                .get("PowerManifoldCharger")
                .is_none_or(|e| e.is_empty())
        );
    }

    /// What the solver does and does not check on the power path (README, "What the solver
    /// checks"). The microcontroller's and the comparator's supply windows are checked against
    /// the 3.3 V rail, the protector's against the 5 V rail, and the converter's input against
    /// the slot's; the converter, the protector, the MOSFETs' gate drive and the comparators'
    /// thresholds are not modelled, so what they do is listed as not evaluated. This pins the
    /// listed ones: when a part states what is missing, this fails and the README is to be
    /// rewritten.
    #[test]
    fn the_power_path_is_checked_as_far_as_the_parts_state_and_the_rest_is_listed() {
        let built = built();
        let listed = |needle: &str| built.notes.iter().any(|n| n.contains(needle));
        // Checked, so not listed: the supply windows of the chips that sit on a stated rail.
        assert!(!listed("mcu.vdd.voltage"), "{:?}", built.notes);
        assert!(!listed("comparator.supply.voltage"), "{:?}", built.notes);
        assert!(!listed("protector.vcc.voltage"), "{:?}", built.notes);
        // Not evaluated: the converter's input window, the regulator's drop and the MOSFETs.
        assert!(listed("not evaluated: converter.vin.voltage >= 2.7V"));
        assert!(listed(
            "not evaluated: q_high.gate_source_voltage >= q_high.gate_threshold"
        ));
        assert!(listed(
            "the balance on net `VOUT_CONV` is incomplete: `converter.vout` is not modelled"
        ));
    }
}
