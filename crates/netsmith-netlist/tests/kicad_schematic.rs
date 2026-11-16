//! A `.kicad_sch` read from its files (#855), with no KiCad: small schematics written here for
//! each of KiCad's rules the reader follows, and the schematics of the ports in `examples/`
//! against the boards KiCad made from them, pin for pin by designator. CI's `kicad` job also
//! holds the reader to KiCad's own netlist export (`netsmith-tools`, `tests/compare.rs`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use netsmith_netlist::{Identity, Netlist, Options, Outcome, compare, report};
use netsmith_testkit::TempDir;

/// A two-pin symbol whose pin 1 is off its axis, so a turn and a mirror move it apart, a power
/// symbol, and a symbol of two units with a pin common to both.
const LIBRARY: &str = r#"(lib_symbols
  (symbol "T:T" (symbol "T_1_1"
    (pin passive line (at 1.27 3.81 270) (length 1) (name "~") (number "1"))
    (pin passive line (at 0 -3.81 90) (length 1) (name "~") (number "2"))))
  (symbol "power:GND" (power) (symbol "GND_0_1"
    (pin power_in line (at 0 0 270) (length 0) (hide yes) (name "GND") (number "1"))))
  (symbol "U:Dual"
    (symbol "Dual_0_1" (pin power_in line (at 5.08 0 0) (length 1) (name "VCC") (number "3")))
    (symbol "Dual_1_1" (pin input line (at 0 0 0) (length 1) (name "A") (number "1")))
    (symbol "Dual_2_1" (pin input line (at 0 0 0) (length 1) (name "B") (number "2")))))"#;

/// The UUIDs of the root file and of the two sheet symbols that place the child (KiCad reads only
/// a UUID as one), and an instance path below them.
const ROOT: &str = "00000000-0000-4000-8000-000000000001";
const S1: &str = "00000000-0000-4000-8000-0000000000a1";
const S2: &str = "00000000-0000-4000-8000-0000000000a2";
const CHILD: &str = "00000000-0000-4000-8000-0000000000c1";

/// The instance path of the root sheet.
fn root() -> String {
    format!("/{ROOT}")
}

fn file(uuid: &str, body: &str) -> String {
    file_of(20250114, uuid, body)
}

fn file_of(version: u32, uuid: &str, body: &str) -> String {
    format!("(kicad_sch (version {version}) (generator \"eeschema\") (uuid \"{uuid}\")\n{LIBRARY}\n{body})\n")
}

/// A placed symbol: its library symbol, where and how it is turned (`(mirror y)` and the like in
/// `extra`), its value, and its designator and unit in each sheet instance path.
fn symbol(lib: &str, at: (f64, f64, f64), extra: &str, value: &str, refs: &[(String, &str, u32)]) -> String {
    let instances: String = refs
        .iter()
        .map(|(path, r, unit)| format!("(path \"{path}\" (reference \"{r}\") (unit {unit}))"))
        .collect();
    let (r, unit) = (refs[0].1, refs[0].2);
    // A UUID of its own: the designator's bytes and the unit.
    let id: String = r.bytes().chain([unit as u8]).map(|b| format!("{b:02x}")).collect();
    let uuid = format!("00000000-0000-4000-9000-{id:0>12}");
    format!(
        "(symbol (lib_id \"{lib}\") (at {} {} {}) {extra} (unit {unit}) (uuid \"{uuid}\")
           (property \"Reference\" \"{r}\" (at 0 0 0)) (property \"Value\" \"{value}\" (at 0 0 0))
           (property \"Footprint\" \"F:{lib}\" (at 0 0 0))
           (instances (project \"p\" {instances})))\n",
        at.0, at.1, at.2
    )
}

fn part(r: &str, at: (f64, f64)) -> String {
    symbol("T:T", (at.0, at.1, 0.0), "", "1k", &[(root(), r, 1)])
}

fn label(kind: &str, name: &str, at: (f64, f64)) -> String {
    format!("({kind} \"{name}\" (at {} {} 0))\n", at.0, at.1)
}

fn wire(a: (f64, f64), b: (f64, f64)) -> String {
    format!("(wire (pts (xy {} {}) (xy {} {})))\n", a.0, a.1, b.0, b.1)
}

/// The nets of two pins or more, each as its `R1.1` pins.
fn nets(n: &Netlist) -> BTreeSet<BTreeSet<String>> {
    n.nets
        .iter()
        .filter(|net| net.pins.len() > 1)
        .map(|net| {
            net.pins
                .iter()
                .map(|(c, p)| format!("{}.{p}", n.components[*c].refdes))
                .collect()
        })
        .collect()
}

fn expect(sets: &[&[&str]]) -> BTreeSet<BTreeSet<String>> {
    sets.iter()
        .map(|s| s.iter().map(|p| p.to_string()).collect())
        .collect()
}

fn net_name<'n>(n: &'n Netlist, pin: &str) -> &'n str {
    let (r, p) = pin.split_once('.').unwrap();
    let c = n.find(r).unwrap();
    &n.nets[n.net_of(c, p).unwrap()].name
}

fn read(dir: &Path, files: &[(&str, String)]) -> Result<Netlist, String> {
    for (name, text) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    Netlist::from_kicad_schematic(&dir.join(files[0].0))
}

#[test]
fn a_turned_or_mirrored_symbol_has_its_pins_where_kicad_draws_them() {
    // Pin 1 of `T` is at (1.27, 3.81) in the library, y up; on the sheet y is down.
    let cases = [
        (0.0, "", (1.27, -3.81)),
        (90.0, "", (-3.81, -1.27)),
        (180.0, "", (-1.27, 3.81)),
        (270.0, "", (3.81, 1.27)),
        (0.0, "(mirror x)", (1.27, 3.81)),
        (0.0, "(mirror y)", (-1.27, -3.81)),
        (90.0, "(mirror y)", (3.81, -1.27)),
        (90.0, "(mirror x)", (-3.81, 1.27)),
    ];
    for (angle, mirror, (dx, dy)) in cases {
        let dir = TempDir::new("kicad-sch-turn");
        let body = [
            symbol("T:T", (100.0, 100.0, angle), mirror, "1k", &[(root(), "R1", 1)]),
            // J1 is not turned: its pin 1 is at (51.27, 46.19).
            part("J1", (50.0, 50.0)),
            label("label", "P", (51.27, 46.19)),
            label("label", "P", (100.0 + dx, 100.0 + dy)),
        ]
        .concat();
        let n = read(&dir, &[("turn.kicad_sch", file(ROOT, &body))]).unwrap();
        assert_eq!(
            nets(&n),
            expect(&[&["J1.1", "R1.1"]]),
            "at {angle}° {mirror}: pin 1 is not at ({dx}, {dy})"
        );
    }
}

#[test]
fn a_label_joins_a_wire_anywhere_along_it_and_a_pin_only_at_its_ends() {
    let dir = TempDir::new("kicad-sch-wire");
    // T at (x, y): pin 1 at (x + 1.27, y - 3.81), pin 2 at (x, y + 3.81).
    let body = [
        wire((0.0, 0.0), (20.0, 0.0)),
        part("J2", (-1.27, 3.81)), // pin 1 at the wire's start
        part("J3", (18.73, 3.81)), // pin 1 at its end
        part("J1", (8.73, 3.81)),  // pin 1 on the wire, between its ends
        label("label", "L", (5.0, 0.0)),
        part("J4", (48.73, 3.81)),
        label("label", "L", (50.0, 0.0)),
        label("global_label", "G", (-1.27, 7.62)),
        label("global_label", "G", (18.73, 7.62)),
    ]
    .concat();
    let n = read(&dir, &[("wire.kicad_sch", file(ROOT, &body))]).unwrap();
    assert_eq!(
        nets(&n),
        expect(&[&["J2.1", "J3.1", "J4.1"], &["J2.2", "J3.2"]])
    );
    assert_eq!(n.net_of(n.find("J1").unwrap(), "1"), None, "J1.1 is joined to nothing");
    assert_eq!(net_name(&n, "J2.1"), "/L");
    assert_eq!(net_name(&n, "J2.2"), "G");
}

#[test]
fn a_sheet_in_another_folder_placed_twice_is_two_instances_joined_by_its_pins_and_global_names() {
    let dir = TempDir::new("kicad-sch-sheets");
    // A sheet symbol at (10, y), 10 by 10, with its pin on its right edge.
    let sheet = |uuid: &str, name: &str, pin_at: (f64, f64)| {
        format!(
            "(sheet (at 10 {}) (size 10 10) (uuid \"{uuid}\")
               (property \"Sheetname\" \"{name}\" (at 0 0 0))
               (property \"Sheetfile\" \"sub/child.kicad_sch\" (at 0 0 0))
               (pin \"IN\" input (at {} {} 0)))\n",
            pin_at.1 - 2.0,
            pin_at.0,
            pin_at.1
        )
    };
    let root = [
        sheet(S1, "A", (20.0, 12.0)),
        sheet(S2, "B", (20.0, 32.0)),
        label("label", "N1", (20.0, 12.0)),
        label("label", "N1", (20.0, 32.0)),
        part("J1", (50.0, 50.0)),
        label("label", "N1", (51.27, 46.19)),
        symbol("power:GND", (50.0, 53.81, 0.0), "", "GND", &[(root(), "#PWR1", 1)]),
    ]
    .concat();
    let (p1, p2) = (format!("/{ROOT}/{S1}"), format!("/{ROOT}/{S2}"));
    let both = |a: &'static str, b: &'static str| [(p1.clone(), a, 1u32), (p2.clone(), b, 1)];
    let child = [
        symbol("T:T", (30.0, 30.0, 0.0), "", "1k", &both("R1", "R2")),
        label("hierarchical_label", "IN", (31.27, 26.19)),
        label("label", "X", (30.0, 33.81)),
        symbol("T:T", (60.0, 60.0, 0.0), "", "1k", &both("Q1", "Q2")),
        label("label", "X", (61.27, 56.19)),
        symbol("power:GND", (60.0, 63.81, 0.0), "", "GND", &both("#PWR2", "#PWR3")),
    ]
    .concat();
    let n = read(
        &dir,
        &[
            ("top/root.kicad_sch", file(ROOT, &root)),
            ("top/sub/child.kicad_sch", file(CHILD, &child)),
        ],
    )
    .unwrap();
    assert_eq!(
        nets(&n),
        expect(&[
            &["J1.1", "R1.1", "R2.1"],
            &["R1.2", "Q1.1"],
            &["R2.2", "Q2.1"],
            &["J1.2", "Q1.2", "Q2.2"],
        ])
    );
    assert_eq!(net_name(&n, "J1.1"), "/N1");
    assert_eq!(net_name(&n, "R1.2"), "/A/X");
    assert_eq!(net_name(&n, "R2.2"), "/B/X");
    assert_eq!(net_name(&n, "Q1.2"), "GND");
    assert_eq!(n.components.len(), 5, "the power symbols are not parts");
}

#[test]
fn each_unit_of_a_symbol_brings_its_own_pins_and_the_pins_common_to_all() {
    let dir = TempDir::new("kicad-sch-units");
    let body = [
        symbol("U:Dual", (0.0, 0.0, 0.0), "", "dual", &[(root(), "U1", 1)]),
        symbol("U:Dual", (20.0, 0.0, 0.0), "", "dual", &[(root(), "U1", 2)]),
        part("J1", (-1.27, 3.81)),
        label("label", "A", (0.0, 0.0)),
        label("label", "A", (20.0, 0.0)),
        // The common pin 3 is at (5.08, 0) of each unit: joined at the first, open at the second.
        wire((5.08, 0.0), (5.08, 10.0)),
        part("J5", (3.81, 13.81)),
        label("label", "V", (5.08, 10.0)),
    ]
    .concat();
    let n = read(&dir, &[("units.kicad_sch", file(ROOT, &body))]).unwrap();
    assert_eq!(n.components.iter().filter(|c| c.refdes == "U1").count(), 1);
    assert_eq!(
        nets(&n),
        expect(&[&["J1.1", "U1.1", "U1.2"], &["J5.1", "U1.3"]])
    );
}

#[test]
fn the_designator_of_a_kicad_6_schematic_is_in_its_root_symbol_instances() {
    let dir = TempDir::new("kicad-sch-6");
    let body = "(symbol (lib_id \"T:T\") (at 0 0 0) (unit 1) (uuid \"00000000-0000-4000-8000-0000000000e1\")
        (property \"Reference\" \"R?\" (at 0 0 0)) (property \"Value\" \"\" (at 0 0 0))
        (property \"Footprint\" \"\" (at 0 0 0)))
      (symbol_instances (path \"/00000000-0000-4000-8000-0000000000e1\" (reference \"R7\") (unit 1) (value \"10k\") (footprint \"F:R\")))";
    let n = read(&dir, &[("six.kicad_sch", file_of(20211123, ROOT, body))]).unwrap();
    let r = &n.components[0];
    assert_eq!((r.refdes.as_str(), r.value.as_str(), r.footprint.as_str()), ("R7", "10k", "F:R"));
}

#[test]
fn a_missing_sheet_a_bus_and_a_file_that_is_no_schematic_are_named() {
    let dir = TempDir::new("kicad-sch-errors");
    let missing = "(sheet (at 0 0) (size 1 1) (uuid \"s\") (property \"Sheetname\" \"Power\" (at 0 0 0))
        (property \"Sheetfile\" \"power.kicad_sch\" (at 0 0 0)))";
    let e = read(&dir, &[("missing.kicad_sch", file(ROOT, missing))]).unwrap_err();
    assert!(e.contains("missing.kicad_sch") && e.contains("`Power`") && e.contains("power.kicad_sch"), "{e}");
    let bus = "(bus (pts (xy 0 0) (xy 10 0)))";
    let e = read(&dir, &[("bus.kicad_sch", file(ROOT, bus))]).unwrap_err();
    assert!(e.contains("bus.kicad_sch") && e.contains("a bus"), "{e}");
    let e = read(&dir, &[("board.kicad_sch", "(kicad_pcb)".to_string())]).unwrap_err();
    assert!(e.contains("not a KiCad schematic"), "{e}");
    let looped = "(sheet (at 0 0) (size 1 1) (uuid \"s\") (property \"Sheetname\" \"Me\" (at 0 0 0))
        (property \"Sheetfile\" \"loop.kicad_sch\" (at 0 0 0)))";
    let e = read(&dir, &[("loop.kicad_sch", file(ROOT, looped))]).unwrap_err();
    assert!(e.contains("places itself"), "{e}");
}

fn example(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(path)
}

/// The schematic and the board KiCad made from it are the same circuit, part for part by
/// designator, once the values the board has from elsewhere (`(designator, board's value)`)
/// are given.
fn same_as_its_board(sch: &str, pcb: &str, board_values: &[(&str, &str)]) {
    let mut schematic =
        Netlist::from_kicad_schematic(&example(sch)).unwrap_or_else(|e| panic!("{e}"));
    for (r, value) in board_values {
        let c = schematic.find(r).unwrap_or_else(|| panic!("{sch} has no {r}"));
        schematic.components[c].value = value.to_string();
    }
    let text = std::fs::read_to_string(example(pcb)).unwrap();
    let board = Netlist::from_kicad_pcb(&text).unwrap_or_else(|e| panic!("{e}"));
    let options = Options {
        identity: Identity::ByRefdes,
        ..Options::default()
    };
    match compare(&schematic, &board, &options).unwrap() {
        Outcome::Match(_) => {}
        other => panic!("{sch} and {pcb} differ: {}", report(&other)),
    }
}

#[test]
fn the_schematic_of_orbit_is_its_board() {
    // KiCad 7's format, with power symbols named by their pins. U17's value is `~` in the
    // schematic (and in KiCad's netlist of it) and the part's name on the board.
    same_as_its_board(
        "orbit/reference/ORBIT.kicad_sch",
        "orbit/reference/ORBIT.kicad_pcb",
        &[("U17", "AT24C128C")],
    );
}

#[test]
fn the_schematic_of_easyduino_and_its_sheet_is_its_board() {
    same_as_its_board(
        "easyduino_esp32s3/reference/Easyduino_ESP32S3.kicad_sch",
        "easyduino_esp32s3/reference/Easyduino_ESP32S3.kicad_pcb",
        &[],
    );
}

#[test]
fn the_schematic_of_the_charger_is_its_board() {
    same_as_its_board(
        "power_manifold_charger/reference/charger-module.kicad_sch",
        "power_manifold_charger/reference/charger-module.kicad_pcb",
        &[],
    );
}
