//! Pair satellites and local rails: a crystal across two adjacent pins of a processor stands
//! beside both, with two short wires that cross nothing and carry no name; a decoupling
//! capacitor on a supply pin has its own short stub with a supply symbol above it and a ground
//! symbol below, instead of a rail across the sheet. The metrics `pair_crossings`, `rail_span`
//! and `satellite_wire` say so in numbers.

mod common;

use netsmith_core::Design;
use netsmith_kicad::testlib::{self, TempDir};
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_view::check::check;
use netsmith_view::geom::{Dir, Pt, Rect};
use netsmith_view::layout::{Layout, PowerKind, PowerSymbol, Wire};
use netsmith_view::metrics::measure;
use netsmith_view::{Rails, layout_rails, render_svg};

const STEP: f64 = 1.27;

fn pin(kind: &str, x: f64, y: f64, angle: u32, name: &str, number: u32) -> String {
    format!(
        "(pin {kind} line (at {x} {y} {angle}) (length 2.54) \
         (name \"{name}\" (effects (font (size 1.27 1.27)))) \
         (number \"{number}\" (effects (font (size 1.27 1.27)))))"
    )
}

/// `Cpu`: eight pins down the left, the crystal's `XL1` and `XL2` the first two, then six
/// others; the supply on the top, the ground on the bottom. `Crystal`: two pins at the ends of
/// a short body.
fn library() -> String {
    let mut cpu = String::new();
    cpu += &pin("bidirectional", -12.7, 8.89, 0, "XL1", 1);
    cpu += &pin("bidirectional", -12.7, 6.35, 0, "XL2", 2);
    for i in 0..6u32 {
        cpu += &pin(
            "bidirectional",
            -12.7,
            3.81 - 2.54 * f64::from(i),
            0,
            &format!("P{i}"),
            3 + i,
        );
    }
    cpu += &pin("power_in", 0.0, 15.24, 270, "VDD", 10);
    cpu += &pin("power_in", 0.0, -15.24, 90, "GND", 11);
    format!(
        "(kicad_symbol_lib (symbol \"Cpu\" (property \"Reference\" \"U\" (at 0 17 0)) \
         (symbol \"Cpu_0_1\" (rectangle (start -10.16 12.7) (end 10.16 -12.7) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Cpu_1_1\" {cpu})) \
         (symbol \"Crystal\" (property \"Reference\" \"Y\" (at 2 0 0)) \
         (symbol \"Crystal_0_1\" (rectangle (start -1.0 2.54) (end 1.0 -2.54) \
         (stroke (width 0.254) (type default)) (fill (type none)))) \
         (symbol \"Crystal_1_1\" {} {})))",
        pin("passive", 0.0, 3.81, 270, "~", 1),
        pin("passive", 0.0, -3.81, 90, "~", 2)
    )
}

fn setup() -> (TempDir, Mapping, SymbolLib) {
    let dir = TempDir::new("view-pairs");
    let file = dir.path().join("pairs.kicad_sym");
    std::fs::write(&file, library()).unwrap();
    let rows = "[[part]]\nkind = \"Cpu\"\npackage = \"P\"\nsymbol = \"pairs:Cpu\"\nfootprint = \"pairs:F\"\n\
                [[part]]\nkind = \"Crystal\"\npackage = \"XTAL\"\nsymbol = \"pairs:Crystal\"\nfootprint = \"pairs:F\"\n";
    let mapping = Mapping::standard()
        .merged(Mapping::parse(rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let lib = SymbolLib::at(testlib::sample_symbols()).with_library("pairs", file);
    (dir, mapping, lib)
}

/// The processor `U1`, the crystal `Y1` across its `XL1` and `XL2` and the capacitor `C1`
/// across its supply and ground pins.
fn board() -> Design {
    common::design(
        "CrystalBoard",
        &[
            ("U1", "Cpu", "MCU", Some("P")),
            ("Y1", "Crystal", "32k768", Some("XTAL")),
            ("C1", "Capacitor", "10u", Some("0402")),
        ],
        &[
            ("XL1", &["U1.1", "Y1.1"]),
            ("XL2", &["U1.2", "Y1.2"]),
            ("+3V3", &["U1.10", "C1.1"]),
            ("GND", &["U1.11", "C1.2"]),
        ],
    )
}

fn rails() -> Rails {
    let mut r = Rails::default();
    r.ground.insert("GND".into());
    r.positive.insert("+3V3".into());
    r
}

struct Drawn {
    design: Design,
    layout: Layout,
    _dir: TempDir,
}

fn draw(rails: &Rails) -> Drawn {
    let (dir, mapping, lib) = setup();
    let design = board();
    let layout = layout_rails(&design, rails, &mapping, &lib).unwrap();
    Drawn {
        design,
        layout,
        _dir: dir,
    }
}

fn net(d: &Design, name: &str) -> usize {
    d.nets.iter().position(|n| n.name == name).unwrap()
}

fn part(d: &Design, refdes: &str) -> usize {
    d.parts.iter().position(|p| p.refdes == refdes).unwrap()
}

fn pin_at(l: &Layout, d: &Design, refdes: &str, pad: &str) -> Pt {
    l.parts[part(d, refdes)]
        .pins
        .iter()
        .find(|p| p.id == pad)
        .unwrap()
        .at
}

fn steps(a: Pt, b: Pt) -> f64 {
    ((a.x - b.x).abs() + (a.y - b.y).abs()) / STEP
}

#[test]
fn the_drawing_keeps_every_rule_with_and_without_rails() {
    for rails in [rails(), Rails::default()] {
        let d = draw(&rails);
        let found = check(&d.design, &d.layout);
        assert!(found.is_empty(), "{found:?}");
    }
}

#[test]
fn a_crystal_across_two_adjacent_pins_stands_beside_both_with_short_wires() {
    let d = draw(&rails());
    // Each crystal pin is within a few steps of the processor pin it serves.
    for (crystal_pad, cpu_pad) in [("1", "1"), ("2", "2")] {
        let (a, b) = (
            pin_at(&d.layout, &d.design, "Y1", crystal_pad),
            pin_at(&d.layout, &d.design, "U1", cpu_pad),
        );
        assert!(
            steps(a, b) <= 16.0,
            "Y1.{crystal_pad} is {} steps from U1.{cpu_pad}",
            steps(a, b)
        );
    }
    // Each wire has at most two corners, and the whole drawing none that is not needed: the
    // two nets are a few runs of wire each.
    for name in ["XL1", "XL2"] {
        let n = net(&d.design, name);
        let wires: Vec<&Wire> = d.layout.wires.iter().filter(|w| w.net == n).collect();
        let corners: usize = wires.iter().map(|w| w.points.len().saturating_sub(2)).sum();
        assert!(corners <= 2, "{name} has {corners} corners: {wires:?}");
        // Neither is named: no label, no flag.
        assert!(d.layout.labels.iter().all(|l| l.net != n));
        assert!(d.layout.ports.iter().all(|p| p.net != n));
    }
    let m = &measure(&d.layout).sheets[0];
    assert_eq!(m.pair_satellites, 1);
    assert_eq!(m.pair_crossings, 0);
    assert_eq!(m.crossings, 0);
    // The longest wire from a satellite to its pin: the crystal's, or the capacitor's to its
    // symbols, a few steps.
    assert!(m.satellite_wire <= 16.0, "{}", m.satellite_wire);
}

#[test]
fn the_crystal_is_turned_so_that_its_wires_do_not_cross_each_other() {
    // Pin 1 of the crystal is on XL1, the upper pin of the processor, pin 2 on XL2: the pins
    // of the crystal are in the same order as the processor's, from top to bottom.
    let d = draw(&rails());
    let y = |pad| pin_at(&d.layout, &d.design, "Y1", pad);
    let u = |pad| pin_at(&d.layout, &d.design, "U1", pad);
    let upper_first = u("1").y < u("2").y;
    let crystal_first = y("1").y < y("2").y;
    let same_axis = (y("1").x - y("2").x).abs() < 1e-6;
    // A crystal standing on end keeps the order; one lying down is above both pins and its
    // pins run in the opposite order of the pins' x, which `pair_crossings` measures.
    if same_axis {
        assert_eq!(upper_first, crystal_first);
    }
    assert_eq!(measure(&d.layout).sheets[0].pair_crossings, 0);
}

#[test]
fn a_wire_laid_across_the_crystals_wire_is_a_pair_crossing() {
    let d = draw(&rails());
    let mut l = d.layout.clone();
    let xl1 = net(&d.design, "XL1");
    // A long straight run of XL1's wire, and a wire of another net across its middle.
    let (a, b) = l
        .wires
        .iter()
        .filter(|w| w.net == xl1)
        .flat_map(|w| w.points.windows(2).map(|p| (p[0], p[1])))
        .max_by(|p, q| steps(p.0, p.1).total_cmp(&steps(q.0, q.1)))
        .unwrap();
    let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let across = if (a.x - b.x).abs() < 1e-6 {
        vec![Pt::new(mid.x - 2.0, mid.y), Pt::new(mid.x + 2.0, mid.y)]
    } else {
        vec![Pt::new(mid.x, mid.y - 2.0), Pt::new(mid.x, mid.y + 2.0)]
    };
    l.wires.push(Wire {
        net: net(&d.design, "+3V3"),
        points: across,
    });
    assert_eq!(measure(&l).sheets[0].pair_crossings, 1);
}

#[test]
fn a_decoupling_capacitor_has_a_supply_symbol_above_it_and_a_ground_symbol_below() {
    let d = draw(&rails());
    let c1 = &d.layout.parts[part(&d.design, "C1")];
    let (plus, minus) = (net(&d.design, "+3V3"), net(&d.design, "GND"));
    let pin_of = |n: usize| c1.pins.iter().find(|p| p.net == Some(n)).unwrap().at;
    // The symbol of the net nearest the pin, among the symbols that are not on the processor's
    // pin.
    let nearest = |n: usize, kind: PowerKind| -> &PowerSymbol {
        let at = pin_of(n);
        d.layout
            .symbols
            .iter()
            .filter(|s| s.net == n && s.kind == kind)
            .min_by(|p, q| steps(p.at, at).total_cmp(&steps(q.at, at)))
            .expect("a symbol")
    };
    let up = nearest(plus, PowerKind::Supply);
    let down = nearest(minus, PowerKind::Ground);
    assert!(up.at.y < pin_of(plus).y, "the supply symbol is above");
    assert!(down.at.y > pin_of(minus).y, "the ground symbol is below");
    assert!(
        steps(up.at, pin_of(plus)) <= 8.0,
        "{} steps",
        steps(up.at, pin_of(plus))
    );
    assert!(
        steps(down.at, pin_of(minus)) <= 8.0,
        "{} steps",
        steps(down.at, pin_of(minus))
    );
    // Both are the capacitor's own: the processor's supply pin has a symbol of its own too, so
    // the supply has two, and so has the ground.
    for (n, kind) in [(plus, PowerKind::Supply), (minus, PowerKind::Ground)] {
        let count = d
            .layout
            .symbols
            .iter()
            .filter(|s| s.net == n && s.kind == kind)
            .count();
        assert_eq!(count, 2, "symbols of {n}");
    }
    // No run of wire of either rail is long: the capacitor's rails are stubs, and the
    // processor's pins are one or two steps apart.
    let m = &measure(&d.layout).sheets[0];
    assert!(m.rail_span <= 12.0, "rail span {}", m.rail_span);
}

#[test]
fn rail_span_is_the_longest_run_of_a_net_that_ends_in_a_power_symbol() {
    // Two wires of net 0 that meet on one line are one run of 10 steps, a corner turns it,
    // and a run of another net, which has no symbol, is not counted however long.
    let wire = |net: usize, pts: &[(f64, f64)]| Wire {
        net,
        points: pts.iter().map(|&(x, y)| Pt::new(x, y)).collect(),
    };
    let l = Layout {
        parts: Vec::new(),
        symbols: vec![PowerSymbol {
            net: 0,
            kind: PowerKind::Supply,
            at: Pt::new(0.0, 0.0),
            dir: Dir::Up,
            label: None,
        }],
        ports: Vec::new(),
        marks: Vec::new(),
        sheets: Vec::new(),
        wires: vec![
            wire(0, &[(0.0, 10.0), (6.35, 10.0)]),
            wire(0, &[(6.35, 10.0), (12.7, 10.0), (12.7, 13.81)]),
            wire(1, &[(0.0, 20.0), (50.0, 20.0)]),
        ],
        junctions: Vec::new(),
        labels: Vec::new(),
        buses: Vec::new(),
        bounds: Rect {
            min: Pt::new(-5.0, -5.0),
            max: Pt::new(60.0, 30.0),
        },
    };
    let m = &measure(&l).sheets[0];
    assert!((m.rail_span - 10.0).abs() < 1e-6, "{}", m.rail_span);
}

#[test]
fn the_same_board_is_drawn_to_the_same_bytes() {
    let (a, b) = (draw(&rails()), draw(&rails()));
    assert_eq!(a.layout, b.layout);
    let svg = || {
        let (_dir, mapping, lib) = setup();
        render_svg(&board(), &mapping, &lib).unwrap()
    };
    assert_eq!(svg(), svg());
}
