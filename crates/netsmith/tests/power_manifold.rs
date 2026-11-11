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

    use netsmith::compare::netlist_of_design;
    use netsmith::project::Target;
    use netsmith::{Libraries, build_with};
    use netsmith_kicad::install::Search;
    use netsmith_kicad::{Mapping, testlib};
    use netsmith_netlist::{Identity, Netlist, Options, Outcome, compare, normalize_value, report};

    use super::{REFERENCE, project, read};

    /// Where each part of the charger module's board is in the netsmith board (the path of its
    /// member), by the design's reference designator. The members are named by what the part
    /// does, so each pair is one to one.
    const PARTS: &[(&str, &str)] = &[
        ("C1", "v3v3_4u7"),
        ("C2", "v3v3_100n_a"),
        ("C3", "v3v3_100n_b"),
        ("C4", "v3v3_100n_c"),
        ("C5", "reset_100n"),
        ("C6", "emi_vin_100u"),
        ("C7", "v5_4u7"),
        ("C8", "emi_vin_10u_a"),
        ("C9", "emi_vin_10u_b"),
        ("C10", "emi_vin_100n"),
        ("C11", "ldo_bypass"),
        ("C12", "v3v3_10u"),
        ("C13", "dither_10n"),
        ("C14", "ntc_converter_100n"),
        ("C15", "emi_vin_1u"),
        ("C16", "ntc_plug_100n"),
        ("C18", "vout_sense_1n"),
        ("C19", "vbus_sense_10n"),
        ("C20", "boot1_cap"),
        ("C21", "boot2_cap"),
        ("C22", "cc1_330p"),
        ("C23", "vcc_10u"),
        ("C24", "cc2_330p"),
        ("C25", "comp_cap_series"),
        ("C26", "iana_100n"),
        ("C27", "cbias_100n"),
        ("C28", "vcc_100n"),
        ("C29", "comp_cap"),
        ("C30", "v3v3_100n_d"),
        ("C31", "gate_47p"),
        ("C32", "vout_conv_10u_a"),
        ("C33", "vout_conv_10u_b"),
        ("C34", "v5_1u"),
        ("C35", "v5_100n"),
        ("C36", "vout_conv_10u_c"),
        ("C37", "vout_conv_10u_d"),
        ("C38", "vbus_2u2"),
        ("C39", "vout_conv_220u"),
        ("D1", "led"),
        ("D2", "port_tvs"),
        ("F1", "fuse"),
        ("FB1", "vcc_bead"),
        ("J1", "tp_tx"),
        ("J2", "swd"),
        ("J3", "slot"),
        ("J4", "usb"),
        ("J5", "tp_gnd"),
        ("J6", "tp_rx"),
        ("L1", "input_inductor"),
        ("L2", "power_inductor"),
        ("Q1", "q_high"),
        ("Q2", "q_low"),
        ("Q3", "q_port_a"),
        ("Q4", "q_port_b"),
        ("R1", "en_pulldown"),
        ("R2", "ovp_ref_pulldown"),
        ("R3", "sda_pullup"),
        ("R4", "scl_pullup"),
        ("R5", "usb_sda_pullup"),
        ("R6", "usb_scl_pullup"),
        ("R7", "led_resistor"),
        ("R8", "tcpp_en_series"),
        ("R9", "tcpp_en_pulldown"),
        ("R10", "fault_pullup"),
        ("R11", "fsw_resistor"),
        ("R12", "ntc_converter_pullup"),
        ("R13", "flag_pullup"),
        ("R14", "gate_high_resistor"),
        ("R16", "ntc_plug_pullup"),
        ("R17", "vout_divider_top"),
        ("R18", "vout_divider_bottom"),
        ("R19", "vbus_divider_top"),
        ("R20", "vbus_divider_bottom"),
        ("R21", "gate_low_resistor"),
        ("R22", "mode_resistor"),
        ("R23", "ilim_resistor"),
        ("R24", "comp_resistor"),
        ("R25", "ovp_ceil_top"),
        ("R26", "ovp_ceil_bottom"),
        ("R27", "port_shunt"),
        ("R28", "output_shunt"),
        ("TH1", "ntc_converter"),
        ("TH2", "ntc_plug"),
        ("U1", "mcu"),
        ("U2", "ldo"),
        ("U3", "converter"),
        ("U4", "comparator"),
        ("U5", "protector"),
        ("U6", "usb_esd"),
    ];

    fn libraries() -> Libraries {
        let nowhere = testlib::TempDir::new("power-manifold-board-no-kicad");
        Libraries::for_project_by(Some(&project()), &Search::under(nowhere.path()))
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn built() -> netsmith::Built {
        let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
        let source = &target.files[0];
        build_with(
            &source.name,
            &read("src/main.nsm"),
            &target.loader().unwrap(),
            &target.catalogs().unwrap(),
            &libraries(),
        )
        .map(|mut boards| boards.remove(0))
        .unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn reference() -> Netlist {
        Netlist::from_kicad_pcb(&read(&format!("reference/{REFERENCE}")))
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// The design's reference designator for each of the design's own, by designator.
    fn designators(built: &netsmith::Built) -> BTreeMap<String, String> {
        PARTS
            .iter()
            .map(|(reference, path)| {
                let part = built
                    .design
                    .parts
                    .iter()
                    .find(|p| p.path == *path)
                    .unwrap_or_else(|| panic!("{reference}: the board has no part `{path}`"));
                (part.refdes.clone(), (*reference).to_string())
            })
            .collect()
    }

    type Pad = (String, String);

    /// For each pad that is on a net with another pad: the other pads of the net, in the
    /// design's designators (`to_reference` maps a designator of this netlist).
    fn neighbours(
        netlist: &Netlist,
        to_reference: &BTreeMap<String, String>,
    ) -> BTreeMap<Pad, BTreeSet<Pad>> {
        let mut out = BTreeMap::new();
        for net in &netlist.nets {
            let pads: BTreeSet<Pad> = net
                .pins
                .iter()
                .map(|(c, pin)| {
                    let refdes = &netlist.components[*c].refdes;
                    let refdes = to_reference.get(refdes).unwrap_or(refdes).clone();
                    (refdes, pin.clone())
                })
                .collect();
            let pads: Vec<Pad> = pads.into_iter().collect();
            if pads.len() < 2 {
                continue;
            }
            for pad in &pads {
                let others = pads.iter().filter(|p| *p != pad).cloned().collect();
                out.insert(pad.clone(), others);
            }
        }
        out
    }

    /// The same netlist with the value and footprint of every component blanked: only the
    /// wiring is left to compare.
    fn wiring_only(netlist: &Netlist) -> Netlist {
        let components = netlist
            .components
            .iter()
            .map(|c| (c.refdes.clone(), String::new(), String::new()))
            .collect();
        let nets = netlist
            .nets
            .iter()
            .map(|n| {
                let pins = n
                    .pins
                    .iter()
                    .map(|(c, pin)| (netlist.components[*c].refdes.clone(), pin.clone()))
                    .collect();
                (n.name.clone(), pins)
            })
            .collect();
        Netlist::from_parts(components, nets).unwrap_or_else(|e| panic!("{e}"))
    }

    fn design_netlist(built: &netsmith::Built) -> Netlist {
        let mapping = Mapping::for_project(Some(&project())).unwrap_or_else(|e| panic!("{e}"));
        netlist_of_design(&built.design, &mapping).unwrap_or_else(|e| panic!("{e}"))
    }

    /// What differs between two neighbour maps, one line per pad.
    fn net_differences(
        ours: &BTreeMap<Pad, BTreeSet<Pad>>,
        theirs: &BTreeMap<Pad, BTreeSet<Pad>>,
    ) -> Vec<String> {
        let show = |s: &BTreeSet<Pad>| {
            s.iter()
                .map(|(r, p)| format!("{r}.{p}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let mut out = Vec::new();
        for pad in ours.keys().chain(theirs.keys()).collect::<BTreeSet<_>>() {
            match (ours.get(pad), theirs.get(pad)) {
                (Some(a), Some(b)) if a == b => {}
                (a, b) => out.push(format!(
                    "{}.{}: board has [{}], the reference [{}]",
                    pad.0,
                    pad.1,
                    a.map(show).unwrap_or_default(),
                    b.map(show).unwrap_or_default()
                )),
            }
        }
        out
    }

    #[test]
    fn the_board_builds_with_no_kicad_and_has_the_89_parts_of_the_charger_board() {
        let built = built();
        assert_eq!(designators(&built).len(), PARTS.len());
        let own: Vec<&str> = built
            .design
            .parts
            .iter()
            .map(|p| p.path.as_str())
            .filter(|path| !PARTS.iter().any(|(_, part)| part == path))
            .collect();
        assert_eq!(own, Vec::<&str>::new(), "a part that is not the charger's");
        let reference = reference();
        let theirs: BTreeSet<&str> = reference
            .components
            .iter()
            .map(|c| c.refdes.as_str())
            .collect();
        let ours: BTreeSet<&str> = PARTS.iter().map(|(r, _)| *r).collect();
        assert_eq!(theirs.len(), 89);
        assert_eq!(theirs, ours);
        assert!(built.schematic.starts_with("(kicad_sch"));
    }

    /// Pads that share a number are one pin in both netlists.
    #[test]
    fn every_pad_has_the_same_pads_on_its_net_as_in_the_charger_board() {
        let built = built();
        let to_reference = designators(&built);
        let ours = neighbours(&design_netlist(&built), &to_reference);
        let theirs = neighbours(&reference(), &BTreeMap::new());
        assert_eq!(net_differences(&ours, &theirs), Vec::<String>::new());
    }

    #[test]
    fn netsmith_compare_says_the_nets_are_the_same() {
        let built = built();
        let to_reference = designators(&built);
        let a = wiring_only(&design_netlist(&built));
        let b = wiring_only(&reference());
        let pairs = to_reference
            .iter()
            .map(|(ours, theirs)| (ours.clone(), theirs.clone()))
            .collect();
        let options = Options {
            identity: Identity::Map(pairs),
            ..Options::default()
        };
        let outcome = compare(&a, &b, &options).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(outcome, Outcome::Match(_)), "{}", report(&outcome));
    }

    /// The comparison would see a wrong net: it is the comparison above that runs, on a board
    /// whose U1 pad 13 (PA2, the USART's TX) is on another net, and its answer is the pad.
    #[test]
    fn the_comparison_sees_a_pad_on_the_wrong_net() {
        let built = built();
        let to_reference = designators(&built);
        let mut ours = neighbours(&design_netlist(&built), &to_reference);
        let theirs = neighbours(&reference(), &BTreeMap::new());
        let key = ("U1".to_string(), "13".to_string());
        let wrong = ours
            .get(&("U1".to_string(), "14".to_string()))
            .unwrap()
            .clone();
        assert_ne!(ours[&key], wrong, "the wrong net must be another one");
        ours.insert(key, wrong);
        let differences = net_differences(&ours, &theirs);
        assert!(
            differences
                .iter()
                .any(|d| d.starts_with("U1.13: board has")),
            "{differences:?}"
        );
        // A pad that is on a net on one side only is a difference as well.
        let mut ours = neighbours(&design_netlist(&built), &to_reference);
        ours.remove(&("U1".to_string(), "14".to_string()));
        let differences = net_differences(&ours, &theirs);
        assert!(
            differences
                .iter()
                .any(|d| d.starts_with("U1.14: board has []")),
            "{differences:?}"
        );
    }

    #[test]
    fn the_resistors_and_capacitors_have_the_charger_boards_values_and_packages() {
        let built = built();
        let reference = reference();
        let mut wrong = Vec::new();
        let mut checked = 0;
        for (name, path) in PARTS
            .iter()
            .filter(|(r, _)| r.starts_with('R') || r.starts_with('C'))
        {
            let part = built.design.parts.iter().find(|p| p.path == *path).unwrap();
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
        let to_reference = designators(&built);
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

    /// The parts whose pick is another row than the board's LCSC number. The design states a
    /// value, a package and what the rail needs, not a part number, and the solver picks the
    /// first row that fits: for a 100 nF, 1 uF, 4.7 uF, 2.2 uF or 10 uF ceramic the board's
    /// number is a higher-voltage or other-dielectric row than the one picked (README,
    /// "Differences from the reference"). Every other part has the board's number.
    const OTHER_ROW: &[&str] = &[
        "C1", "C2", "C3", "C4", "C5", "C7", "C10", "C14", "C15", "C16", "C20", "C21", "C23", "C27",
        "C28", "C30", "C34", "C35", "C38",
    ];

    /// Every part of the board that has a number is sourced with that LCSC number, but the
    /// ceramics of `OTHER_ROW`.
    #[test]
    fn the_parts_have_the_lcsc_numbers_of_the_charger_boards_footprints() {
        let built = built();
        let to_reference = designators(&built);
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
        // The design ties PA9 and PA10 of the microcontroller (U1 pads 29 and 32) to ground, and
        // types those two pins "unspecified" in its symbol. The port's pins are the datasheet's
        // (bidirectional), and KiCad's rules refuse a bidirectional pin on a net with a power
        // flag: these two findings, and no other (README, "Differences from the reference").
        assert_eq!(
            crate::erc::with_the_machines_libraries(found),
            vec!["pin_to_pin: Pins of type Bidirectional and Power output are connected"; 2]
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

    /// `reference/designators.toml`, which the parity checker reads, pairs each of the design's
    /// designators with the one `fab` writes for the same part, and so must say what the
    /// design's own designators are (`PARTS` pairs the parts by what they are: the member
    /// path). The design's designators come from the kind of the part and the order of its
    /// declaration, so a part added to `src/main.nsm` ahead of another renumbers it, and this
    /// fails until the map is written again; the message prints the map as it must be.
    #[test]
    fn the_parity_checkers_designator_map_is_the_designs_own() {
        let built = built();
        let map = netsmith::parity::DesignatorMap::parse(&read("reference/designators.toml"))
            .unwrap_or_else(|e| panic!("{e}"));
        let mut expected: Vec<(String, String)> = designators(&built)
            .into_iter()
            .map(|(refdes, reference)| (reference, refdes))
            .collect();
        let wrong: Vec<String> = expected
            .iter()
            .filter(|(reference, refdes)| map.get(reference) != Some(refdes.as_str()))
            .map(|(reference, refdes)| {
                format!(
                    "{reference}: the map says {:?}, the design has {refdes}",
                    map.get(reference)
                )
            })
            .collect();
        expected.sort_by_key(|(r, _)| {
            let digits: String = r.chars().filter(char::is_ascii_digit).collect();
            (
                r.chars().filter(|c| c.is_alphabetic()).collect::<String>(),
                digits.parse::<u32>().unwrap_or(0),
            )
        });
        let lines: String = expected
            .iter()
            .map(|(reference, refdes)| format!("{reference} = \"{refdes}\"\n"))
            .collect();
        assert!(
            wrong.is_empty(),
            "{}\nthe map as it must be:\n{lines}",
            wrong.join("\n")
        );
        // All 89 parts of the reference are paired, and the map has nothing else.
        assert_eq!(map.len(), PARTS.len());
        assert_eq!(map.len(), 89);
        for reference in ["J2", "J3", "J4"] {
            assert!(map.get(reference).is_some(), "{reference} is not mapped");
        }
    }
}
