//! The spine of a regulator: the power path through an IC is one row from left to right, the
//! parts in series (a fuse, a diode, the inductor) stand on it, each turned along it, the
//! capacitors and the divider hang from the node they serve with a ground symbol of their own,
//! and the bootstrap capacitor stays beside its two pins, crossed by nothing. The metrics
//! `spine_bends` and `detached_chain` say so in numbers.

mod common;

use netsmith_core::Design;
use netsmith_kicad::testlib::{self, TempDir};
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_view::check::check;
use netsmith_view::geom::Pt;
use netsmith_view::layout::{Layout, PowerKind};
use netsmith_view::metrics::measure;
use netsmith_view::{Rails, layout_rails};

const STEP: f64 = 1.27;

fn pin(kind: &str, x: f64, y: f64, angle: u32, name: &str, number: u32) -> String {
    format!(
        "(pin {kind} line (at {x} {y} {angle}) (length 2.54) \
         (name \"{name}\" (effects (font (size 1.27 1.27)))) \
         (number \"{number}\" (effects (font (size 1.27 1.27)))))"
    )
}

/// `Reg`: the input and the enable on the left, the bootstrap, the switching pin and the
/// feedback on the right, the ground below. `Series2`: two pins at the ends of a short body.
fn library() -> String {
    let mut reg = String::new();
    reg += &pin("power_in", -12.7, 7.62, 0, "VIN", 1);
    reg += &pin("input", -12.7, 5.08, 0, "EN", 2);
    reg += &pin("passive", 12.7, 7.62, 180, "BST", 3);
    reg += &pin("output", 12.7, 5.08, 180, "SW", 4);
    reg += &pin("input", 12.7, 2.54, 180, "FB", 5);
    reg += &pin("power_in", 0.0, -12.7, 90, "GND", 6);
    format!(
        "(kicad_symbol_lib (symbol \"Reg\" (property \"Reference\" \"U\" (at 0 12 0)) \
         (symbol \"Reg_0_1\" (rectangle (start -10.16 10.16) (end 10.16 -10.16) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Reg_1_1\" {reg})) \
         (symbol \"Series2\" (property \"Reference\" \"X\" (at 2 0 0)) \
         (symbol \"Series2_0_1\" (rectangle (start -1.0 2.54) (end 1.0 -2.54) \
         (stroke (width 0.254) (type default)) (fill (type none)))) \
         (symbol \"Series2_1_1\" {} {})))",
        pin("passive", 0.0, 3.81, 270, "~", 1),
        pin("passive", 0.0, -3.81, 90, "~", 2)
    )
}

fn setup() -> (TempDir, Mapping, SymbolLib) {
    let dir = TempDir::new("view-spine");
    let file = dir.path().join("spine.kicad_sym");
    std::fs::write(&file, library()).unwrap();
    let mut rows = String::new();
    for (kind, package, symbol) in [
        ("Reg", "R", "Reg"),
        ("SpFuse", "P", "Series2"),
        ("SpDiode", "P", "Series2"),
        ("SpCoil", "P", "Series2"),
    ] {
        rows += &format!(
            "[[part]]\nkind = \"{kind}\"\npackage = \"{package}\"\nsymbol = \"spine:{symbol}\"\nfootprint = \"spine:F\"\n"
        );
    }
    let mapping = Mapping::standard()
        .merged(Mapping::parse(&rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let lib = SymbolLib::at(testlib::sample_symbols()).with_library("spine", file);
    (dir, mapping, lib)
}

/// A connector `J1`, a fuse `F1` and a diode `D1` to the IC `U1`; its switching pin through the
/// inductor `L1` to the output; input capacitors `C1`, `C2`, output capacitors `C4`, `C5`, the
/// bootstrap capacitor `C3`, and the divider `R1`, `R2` to the feedback pin.
fn buck() -> Design {
    common::design(
        "Buck",
        &[
            ("J1", "Header1x2", "Conn_01x02", None),
            ("F1", "SpFuse", "3A", Some("P")),
            ("D1", "SpDiode", "SS34", Some("P")),
            ("U1", "Reg", "BUCK", Some("R")),
            ("L1", "SpCoil", "4u7", Some("P")),
            ("C1", "Capacitor", "10u", Some("0402")),
            ("C2", "Capacitor", "100n", Some("0402")),
            ("C3", "Capacitor", "100n", Some("0402")),
            ("C4", "Capacitor", "22u", Some("0402")),
            ("C5", "Capacitor", "22u", Some("0402")),
            ("R1", "Resistor", "100k", Some("0402")),
            ("R2", "Resistor", "10k", Some("0402")),
        ],
        &[
            ("VIN", &["J1.1", "F1.1"]),
            ("FUSED", &["F1.2", "D1.1"]),
            ("VPROT", &["D1.2", "C1.1", "C2.1", "U1.1", "U1.2"]),
            ("BST", &["U1.3", "C3.2"]),
            ("SW", &["U1.4", "L1.1", "C3.1"]),
            ("VOUT", &["L1.2", "C4.1", "C5.1", "R1.1"]),
            ("FB", &["U1.5", "R1.2", "R2.1"]),
            (
                "GND",
                &["J1.2", "U1.6", "C1.2", "C2.2", "C4.2", "C5.2", "R2.2"],
            ),
        ],
    )
}

/// The IC with a supply on its input and an output pin that is on a rail itself, and two
/// capacitors: an LDO, with no part in series.
fn ldo() -> Design {
    common::design(
        "Ldo",
        &[
            ("U1", "Reg", "LDO", Some("R")),
            ("C1", "Capacitor", "1u", Some("0402")),
            ("C2", "Capacitor", "1u", Some("0402")),
        ],
        &[
            ("VIN", &["U1.1", "U1.2", "C1.1"]),
            ("VOUT", &["U1.4", "U1.5", "C2.1"]),
            ("GND", &["U1.6", "C1.2", "C2.2"]),
        ],
    )
}

/// An IC that reads a battery through a divider: a part from its input pin to a rail, in
/// series, that is no way out of a regulator.
fn sensor() -> Design {
    common::design(
        "Sense",
        &[
            ("U1", "Reg", "ADC", Some("R")),
            ("R1", "Resistor", "100k", Some("0402")),
            ("R2", "Resistor", "10k", Some("0402")),
            ("C1", "Capacitor", "1u", Some("0402")),
        ],
        &[
            ("VCC", &["U1.1", "C1.1"]),
            ("VBAT", &["R1.1"]),
            ("SENSE", &["U1.5", "R1.2", "R2.1"]),
            ("GND", &["U1.6", "C1.2", "R2.2"]),
        ],
    )
}

fn rails(names: &[&str]) -> Rails {
    let mut r = Rails::default();
    r.ground.insert("GND".into());
    for n in names {
        r.positive.insert((*n).into());
    }
    r
}

fn buck_rails() -> Rails {
    rails(&["VIN", "VOUT"])
}

struct Drawn {
    design: Design,
    layout: Layout,
    _dir: TempDir,
}

fn draw(design: Design, rails: &Rails) -> Drawn {
    let (dir, mapping, lib) = setup();
    let layout = layout_rails(&design, rails, &mapping, &lib).unwrap();
    Drawn {
        design,
        layout,
        _dir: dir,
    }
}

fn part(d: &Design, refdes: &str) -> usize {
    d.parts.iter().position(|p| p.refdes == refdes).unwrap()
}

fn pin_at(d: &Drawn, refdes: &str, pad: &str) -> Pt {
    d.layout.parts[part(&d.design, refdes)]
        .pins
        .iter()
        .find(|p| p.id == pad)
        .unwrap()
        .at
}

/// Both pins of a two-pin part are on one horizontal line: its y, or `None`.
fn row_of(d: &Drawn, refdes: &str) -> Option<f64> {
    let (a, b) = (pin_at(d, refdes, "1"), pin_at(d, refdes, "2"));
    ((a.y - b.y).abs() < 1e-6).then_some(a.y)
}

#[test]
fn a_buck_is_drawn_as_one_row_from_the_input_to_the_inductor() {
    let d = draw(buck(), &buck_rails());
    assert_eq!(d.layout.spines.len(), 1, "{:?}", d.layout.spines);
    let mark = &d.layout.spines[0];
    let names = |c: &[usize]| -> Vec<String> {
        c.iter()
            .map(|&p| d.design.parts[p].refdes.clone())
            .collect()
    };
    assert_eq!(names(&mark.chains[0]), ["D1", "F1"], "{mark:?}");
    assert_eq!(names(&mark.chains[1]), ["L1"], "{mark:?}");

    // The input chain is on the line of the IC's input pin, the inductor on that of its switch.
    let vin = pin_at(&d, "U1", "1");
    let sw = pin_at(&d, "U1", "4");
    for r in ["D1", "F1"] {
        assert_eq!(row_of(&d, r), Some(vin.y), "{r} is not on the input row");
    }
    assert_eq!(row_of(&d, "L1"), Some(sw.y), "L1 is not on the switch row");
    // In the order of the signal: the connector, the fuse, the diode, the IC, the inductor.
    let x = |r: &str| pin_at(&d, r, "1").x;
    assert!(x("J1") < x("F1") && x("F1") < x("D1") && x("D1") < vin.x);
    assert!(sw.x < x("L1"));

    let m = &measure(&d.layout).sheets[0];
    assert_eq!(m.spine_bends, 0);
    assert_eq!(m.detached_chain, 0);
    assert_eq!(m.crossings, 0, "{m:?}");
    assert_eq!(m.pair_crossings, 0);
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_capacitors_hang_from_the_row_each_with_a_ground_symbol_below() {
    let d = draw(buck(), &buck_rails());
    let rows = [
        ("C1", pin_at(&d, "U1", "1").y),
        ("C2", pin_at(&d, "U1", "1").y),
        ("C4", pin_at(&d, "U1", "4").y),
        ("C5", pin_at(&d, "U1", "4").y),
    ];
    let grounds: Vec<Pt> = d
        .layout
        .symbols
        .iter()
        .filter(|s| s.kind == PowerKind::Ground)
        .map(|s| s.at)
        .collect();
    for (c, row) in rows {
        let top = pin_at(&d, c, "1");
        let bottom = pin_at(&d, c, "2");
        // The pin on the node is two steps below the row, straight under it, and the other
        // pin points down.
        assert!(
            (top.y - row - 2.0 * STEP).abs() < 1e-6,
            "{c}: its pin is {} steps under the row",
            (top.y - row) / STEP
        );
        assert!(bottom.y > top.y);
        // A ground symbol is near: below the pin, within a dozen steps down.
        assert!(
            grounds
                .iter()
                .any(|g| g.y > bottom.y && (g.y - bottom.y) / STEP <= 12.0),
            "{c} has no ground symbol under it"
        );
    }
}

#[test]
fn the_bootstrap_capacitor_stays_beside_its_two_pins_and_nothing_crosses_it() {
    let d = draw(buck(), &buck_rails());
    let m = &measure(&d.layout).sheets[0];
    assert_eq!(m.pair_satellites, 1);
    assert_eq!(m.pair_crossings, 0);
    for (pad, ic_pad) in [("1", "4"), ("2", "3")] {
        let (a, b) = (pin_at(&d, "C3", pad), pin_at(&d, "U1", ic_pad));
        let steps = ((a.x - b.x).abs() + (a.y - b.y).abs()) / STEP;
        assert!(steps <= 24.0, "C3.{pad} is {steps} steps from U1.{ic_pad}");
    }
    // No name on its nets.
    for name in ["BST", "SW"] {
        let n = d.design.nets.iter().position(|n| n.name == name).unwrap();
        assert!(d.layout.labels.iter().all(|l| l.net != n));
        assert!(d.layout.ports.iter().all(|p| p.net != n));
    }
}

#[test]
fn the_divider_hangs_from_the_output_node_with_its_middle_wired_to_the_feedback_pin() {
    let d = draw(buck(), &buck_rails());
    let row = pin_at(&d, "U1", "4").y;
    let (r1, r2) = (pin_at(&d, "R1", "1"), pin_at(&d, "R2", "1"));
    assert!(
        (r1.y - row - 2.0 * STEP).abs() < 1e-6,
        "{}",
        (r1.y - row) / STEP
    );
    // The second resistor is under the first, in its column.
    assert!(r2.y > r1.y && (r2.x - r1.x).abs() < 1e-6);
    // The feedback net is a wire from the pin to the divider, with no name.
    let fb = d.design.nets.iter().position(|n| n.name == "FB").unwrap();
    assert!(d.layout.wires.iter().any(|w| w.net == fb));
    assert!(d.layout.labels.iter().all(|l| l.net != fb));
}

#[test]
fn an_ldo_has_both_its_capacitors_hanging_from_the_row_of_a_pin() {
    let d = draw(ldo(), &rails(&["VIN", "VOUT"]));
    assert_eq!(d.layout.spines.len(), 1);
    assert!(d.layout.spines[0].chains.is_empty());
    for (c, ic_pad) in [("C1", "1"), ("C2", "4")] {
        let row = pin_at(&d, "U1", ic_pad).y;
        let top = pin_at(&d, c, "1");
        assert!((top.y - row - 2.0 * STEP).abs() < 1e-6, "{c}");
    }
    let m = &measure(&d.layout).sheets[0];
    assert_eq!((m.crossings, m.spine_bends, m.detached_chain), (0, 0, 0));
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_part_from_an_input_pin_to_a_rail_is_no_spine_and_nothing_is_found_without_rails() {
    let d = draw(sensor(), &rails(&["VCC", "VBAT"]));
    assert!(d.layout.spines.is_empty(), "{:?}", d.layout.spines);
    // The same buck with no rails told is drawn as it was: no spine.
    let d = draw(buck(), &Rails::default());
    assert!(d.layout.spines.is_empty());
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_spine_is_drawn_the_same_each_time() {
    let a = draw(buck(), &buck_rails());
    let b = draw(buck(), &buck_rails());
    assert_eq!(a.layout, b.layout);
}

#[test]
fn dump_the_boards_when_asked() {
    // `NETSMITH_DUMP_SVG=<folder>` writes the pictures, to look at them.
    let Some(dir) = std::env::var_os("NETSMITH_DUMP_SVG") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (_dir, mapping, lib) = setup();
    for (name, design, r) in [
        ("spine-buck", buck(), buck_rails()),
        ("spine-ldo", ldo(), rails(&["VIN", "VOUT"])),
    ] {
        let svg = netsmith_view::render_svg_with(&design, &r, &mapping, &lib).unwrap();
        std::fs::write(dir.join(format!("{name}.svg")), svg).unwrap();
    }
}
