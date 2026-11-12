//! `netsmith port skeleton` (#452) over the reference boards of the four ports: one member for
//! each footprint, one join for each net, the original's net names, and a parts file that
//! `PortParts` reads back. The joins are read back from the source and are the original's nets.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use netsmith::port::PortParts;
use netsmith::port_skeleton::skeleton;
use netsmith_kicad::testlib::TempDir;
use netsmith_lang::ast::{DefKind, Item};
use netsmith_netlist::Netlist;

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

const BOARDS: &[&str] = &[
    "easyduino_esp32s3/reference/Easyduino_ESP32S3.kicad_pcb",
    "orbit/reference/ORBIT.kicad_pcb",
    "rp2350_minimal/reference/RPI-RP2350A-MINIMAL_R4-S1.kicad_pcb",
    "power_manifold_charger/reference/charger-module.kicad_pcb",
];

#[test]
fn the_skeleton_of_each_ports_original_has_its_parts_and_its_nets() {
    for board in BOARDS {
        let path = examples().join(board);
        let text = fs::read_to_string(&path).unwrap();
        let original = Netlist::from_kicad_pcb(&text).unwrap();
        let src = TempDir::new("port-skeleton-src");
        let s = skeleton(&text, board, "Port", &src).unwrap();

        // A member for each footprint, and a parts file that names each designator once.
        assert_eq!(s.members, original.components.len(), "{board}");
        let parts = PortParts::parse(&s.parts, "parts.toml").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parts.len(), original.components.len(), "{board}");
        for c in &original.components {
            assert!(parts.path_of(&c.refdes).is_some(), "{board}: {}", c.refdes);
        }
        // With no part files, every member that is not a standard part is a gap.
        assert_eq!(s.standard + s.gaps.len(), s.members, "{board}");
        // The RP2350 board's passives have IPC footprint names (`generic_resc1005x35`), which
        // the skeleton's table does not read: each is a gap.
        assert_eq!(
            s.standard == 0,
            board.starts_with("rp2350"),
            "{board}: {}",
            s.standard
        );

        // A join for each net of two pins or more, and a name on each net the original names.
        let joined: Vec<_> = original.nets.iter().filter(|n| n.pins.len() > 1).collect();
        assert_eq!(s.joins, joined.len(), "{board}");
        let own_names = original
            .nets
            .iter()
            .filter(|n| !n.name.starts_with("Net-(") && !n.name.starts_with("unconnected-("))
            .count();
        assert_eq!(s.named, own_names, "{board}");

        // The source parses, and its joins are the original's nets, pad for pad.
        let file = netsmith_lang::parse(&s.source)
            .unwrap_or_else(|e| panic!("{board}: {e:?}\n{}", s.source));
        let def = file
            .definitions
            .iter()
            .find(|d| d.kind == DefKind::Board)
            .unwrap();
        let declared = def
            .items
            .iter()
            .filter(|i| matches!(i, Item::Decl(_)))
            .count();
        assert_eq!(declared, s.members, "{board}");
        let mut nets = BTreeSet::new();
        let mut names = BTreeMap::new();
        for item in &def.items {
            // The original's pads of a pin the source names: `.u1.pa14_boot0`, or with
            // `.net` after it.
            let pads = |p: &netsmith_lang::ast::Path| -> Vec<(String, String)> {
                let names: Vec<&str> = p.segments.iter().map(|s| s.name.as_str()).collect();
                let pin = match names.last() {
                    Some(&"net") if matches!(item, Item::Constraint(_)) => {
                        &names[1..names.len() - 1]
                    }
                    _ => &names[1..],
                };
                let refdes = parts.reference_of(names[0]).unwrap().to_string();
                let key = (names[0].to_string(), pin.join("."));
                s.pads[&key]
                    .iter()
                    .map(|pad| (refdes.clone(), pad.clone()))
                    .collect()
            };
            match item {
                Item::Connect(c) => {
                    let net: BTreeSet<_> = std::iter::once(&c.first)
                        .chain(c.rest.iter().map(|(_, p)| p))
                        .flat_map(pads)
                        .collect();
                    nets.insert(net);
                }
                Item::Constraint(c) => {
                    let netsmith_lang::ast::Expr::Path(p) = &c.lhs else {
                        panic!("{board}: a constraint that is not a net name");
                    };
                    let netsmith_lang::ast::Expr::Str(name, _) = &c.rhs else {
                        panic!("{board}: a net name that is not text");
                    };
                    names.insert(pads(p)[0].clone(), name.clone());
                }
                _ => {}
            }
        }
        let expected: BTreeSet<BTreeSet<(String, String)>> = joined
            .iter()
            .map(|n| {
                n.pins
                    .iter()
                    .map(|(c, pad)| (original.components[*c].refdes.clone(), pad.clone()))
                    .collect()
            })
            .collect();
        assert_eq!(nets, expected, "{board}");
        assert_eq!(names.len(), own_names, "{board}");
        for (pin, name) in &names {
            let i = original.find(&pin.0).unwrap();
            let net = original.net_of(i, &pin.1).unwrap();
            assert_eq!(&original.nets[net].name, name, "{board}");
        }
        // Every R, C and L the skeleton made a standard part carries the TODO on its line.
        let lines: Vec<&str> = s.source.lines().collect();
        for (line, refdes) in &s.tolerances {
            let text = lines[line - 1];
            assert!(
                text.contains(&format!(".{}: ", refdes.to_lowercase()))
                    && text.ends_with("// TODO tolerance: the original states none"),
                "{board}: line {line}: {text}"
            );
        }
        let todo = lines
            .iter()
            .filter(|l| l.contains("TODO tolerance:"))
            .count();
        assert_eq!(todo, s.tolerances.len(), "{board}");
    }
}

#[test]
fn a_gaps_pins_are_named_as_part_from_symbol_names_them() {
    let skeleton_of = |board: &str| {
        let text = fs::read_to_string(examples().join(board)).unwrap();
        let src = TempDir::new("port-skeleton-names");
        skeleton(&text, board, "Port", &src).unwrap().source
    };
    // The charger's microcontroller, whose board names carry the pad: `VDD/VDDA_6`, `VREF+_5`.
    let charger = skeleton_of(BOARDS[3]);
    for pin in [".u1.vdd_vdda_6", ".u1.vref_5"] {
        assert!(
            charger.contains(pin),
            "no `{pin}` in the charger's skeleton"
        );
    }
    // The Easyduino's USB bridge: `~{RST}_9` has an overbar.
    let easyduino = skeleton_of(BOARDS[0]);
    assert!(
        easyduino.contains(".u3.rst_9_n"),
        "no `.u3.rst_9_n` in the Easyduino's skeleton"
    );
}

/// A board of a resistor and a part with a file of its own, and a part with none.
const SMALL: &str = r#"(kicad_pcb (version 20240108)
  (footprint "Resistor_SMD:R_0603_1608Metric"
    (property "Reference" "R1") (property "Value" "4k7")
    (pad "1" smd rect (net 1 "VCC")) (pad "2" smd rect (net 2 "Net-(R1-Pad2)")))
  (footprint "Package_TO_SOT_SMD:SOT-23"
    (property "Reference" "U1") (property "Value" "ABC1")
    (pad "1" smd rect (net 2 "Net-(R1-Pad2)")) (pad "2" smd rect (net 3 "GND")) (pad "3" smd rect (net 1 "VCC")))
  (footprint "Package_TO_SOT_SMD:SOT-23"
    (property "Reference" "U2") (property "Value" "XYZ-9")
    (pad "1" smd rect (net 3 "GND") (pinfunction "~{EN}"))
    (pad "2" smd rect (net 1 "VCC") (pinfunction "IN+"))
    (pad "3" smd rect (net 1 "VCC") (pinfunction "IN_POS"))
    (pad "4" smd rect (net 1 "VCC") (pinfunction "IN+"))
    (pad "5" smd rect (net 3 "GND") (pinfunction "IN+"))
    (pad "6" smd rect (net 3 "GND")))
)"#;

#[test]
fn a_part_file_in_the_project_names_the_pins_and_a_footprint_without_one_is_a_gap() {
    let src = TempDir::new("port-skeleton-own");
    fs::write(
        src.join("abc1.nsm"),
        "part Abc1 {\n    pins {\n        .out output \"OUT\" \"1\"\n        .gnd power \"GND\" \"2\"\n        .vin power \"VIN\" \"3\"\n    }\n}\n",
    )
    .unwrap();
    let s = skeleton(SMALL, "small.kicad_pcb", "Small", &src).unwrap();
    let text = &s.source;
    assert!(text.contains("import src::abc1::Abc1\n"), "{text}");
    assert!(
        text.contains("import src::xyz9::Xyz9  // GAP: no part file, src/xyz9.nsm"),
        "{text}"
    );
    assert!(
        text.contains("import std::parts::passive::Resistor\n"),
        "{text}"
    );
    assert!(
        text.contains(".r1: Resistor { .resistance &= 4.7kohm +/- 1%; .package &= \"0603\" }"),
        "{text}"
    );
    assert!(
        text.contains("    // U1: ABC1, Package_TO_SOT_SMD:SOT-23\n    .u1: Abc1\n"),
        "{text}"
    );
    assert!(
        text.contains("// GAP: no part file; write `part Xyz9` in src/xyz9.nsm\n    .u2: Xyz9"),
        "{text}"
    );
    assert!(
        text.contains(
            "    // VCC\n    .r1.a ~ .u1.vin ~ .u2.in_pos ~ .u2.in_pos_2\n    .r1.a.net &= \"VCC\"\n"
        ),
        "{text}"
    );
    // A gap's pins are the original's names; two names that make one path, or one name on two
    // nets, are numbered; a pad with no name is its number.
    assert!(
        text.contains("    // GND\n    .u1.gnd ~ .u2.en_n ~ .u2.in_pos_3 ~ .u2.'6'\n"),
        "{text}"
    );
    assert_eq!(s.tolerances, [(22, "R1".to_string())], "{text}");
    assert!(
        text.lines()
            .nth(21)
            .unwrap()
            .ends_with("}  // TODO tolerance: the original states none"),
        "{text}"
    );
    // A net KiCad named has its join and no name line.
    assert!(
        text.contains("    // Net-(R1-Pad2)\n    .r1.b ~ .u1.out\n\n"),
        "{text}"
    );
    assert!(!text.contains("&= \"Net-("), "{text}");
    assert_eq!(s.gaps, ["U2"]);
    assert_eq!((s.members, s.standard, s.joins, s.named), (3, 1, 3, 2));
}
