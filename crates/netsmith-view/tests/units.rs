//! A part whose symbol has several units is drawn as a box for each (ADR 0050): the power unit
//! of a processor module, with its supply pins and twenty grounds in one column, is a box of
//! its own with the decoupling capacitor beside it, and the signal unit has no power row. The
//! drawing is still the netlist (`check`), each pad is in exactly one box, and the metrics count
//! each box as a body.

use crate::common;

use netsmith_core::Design;
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_testkit::TempDir;
use netsmith_view::check::check;
use netsmith_view::geom::Orient;
use netsmith_view::layout::{Layout, PlacedPart};
use netsmith_view::metrics::measure;
use netsmith_view::{Rails, layout_rails, render_svg_by_module_with};

fn pin(kind: &str, x: f64, y: f64, angle: u32, name: &str, number: u32) -> String {
    format!(
        "(pin {kind} line (at {x} {y} {angle}) (length 2.54) \
         (name \"{name}\" (effects (font (size 1.27 1.27)))) \
         (number \"{number}\" (effects (font (size 1.27 1.27)))))"
    )
}

/// `Soc`, a module with two units. Unit 1: the supplies `VDDH`, `VDD`, `VDD` and an `NC` on the
/// left, twenty `GND` pins down the right. Unit 2: thirteen signal pins on the left and thirteen on the
/// right.
fn library(grounds: u32, per_side: u32) -> String {
    let mut power = String::new();
    for (i, name) in ["VDDH", "VDD", "VDD", "NC"].into_iter().enumerate() {
        let kind = if name == "NC" {
            "no_connect"
        } else {
            "power_in"
        };
        power += &pin(kind, -12.7, 10.16 - 2.54 * i as f64, 0, name, 1 + i as u32);
    }
    for i in 0..grounds {
        power += &pin(
            "power_in",
            12.7,
            27.94 - 2.54 * f64::from(i),
            180,
            "GND",
            11 + i,
        );
    }
    let mut signals = String::new();
    for i in 0..per_side {
        signals += &pin(
            "bidirectional",
            -12.7,
            11.43 - 2.54 * f64::from(i),
            0,
            &format!("P0.{i:02}"),
            101 + i,
        );
        signals += &pin(
            "bidirectional",
            12.7,
            11.43 - 2.54 * f64::from(i),
            180,
            &format!("P1.{i:02}"),
            201 + i,
        );
    }
    format!(
        "(kicad_symbol_lib (symbol \"Soc\" (property \"Reference\" \"U\" (at 0 30 0)) \
         (symbol \"Soc_1_1\" (rectangle (start -10.16 30.48) (end 10.16 -22.86) \
         (stroke (width 0.254) (type default)) (fill (type background))) {power}) \
         (symbol \"Soc_2_1\" (rectangle (start -10.16 13.97) (end 10.16 -{}) \
         (stroke (width 0.254) (type default)) (fill (type background))) {signals})))",
        11.43 + 2.54 * f64::from(per_side)
    )
}

fn setup(grounds: u32, per_side: u32) -> (TempDir, Mapping, SymbolLib) {
    let dir = TempDir::new("view-units");
    let file = dir.path().join("units.kicad_sym");
    std::fs::write(&file, library(grounds, per_side)).unwrap();
    let rows = "[[part]]\nkind = \"Soc\"\npackage = \"M\"\nsymbol = \"units:Soc\"\nfootprint = \"units:F\"\n";
    let mapping = Mapping::standard()
        .merged(Mapping::parse(rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let lib = SymbolLib::at(netsmith_testkit::sample_symbols()).with_library("units", file);
    (dir, mapping, lib)
}

/// `U1` with its supplies (the `VDD` pads 2 and 3 on `+3V3`, `VDDH` 1 too), all twenty grounds,
/// two signals to two headers, and the capacitor `C1` between the supply and the ground.
fn board() -> Design {
    let mut gnd: Vec<String> = (11..31).map(|i| format!("U1.{i}")).collect();
    gnd.push("C1.2".into());
    let gnd: Vec<&str> = gnd.iter().map(String::as_str).collect();
    common::design(
        "UnitBoard",
        &[
            ("U1", "Soc", "SOC", Some("M")),
            ("C1", "Capacitor", "10u", Some("0402")),
            ("J1", "Header1x2", "Conn_01x02", None),
            ("J2", "Header1x2", "Conn_01x02", None),
        ],
        &[
            ("+3V3", &["U1.1", "U1.2", "U1.3", "C1.1"]),
            ("GND", &gnd),
            ("SIG0", &["U1.101", "J1.1"]),
            ("SIG1", &["U1.201", "J2.1"]),
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
    mapping: Mapping,
    lib: SymbolLib,
    _dir: TempDir,
}

impl Drawn {
    /// The drawing as the editor serves it.
    fn svg(&self) -> String {
        render_svg_by_module_with(&self.design, &[], &rails(), &self.mapping, &self.lib).unwrap()
    }
}

fn draw(design: Design) -> Drawn {
    draw_with(design, 20, 13)
}

/// The board with a power unit of `grounds` pins and a signal unit of `per_side` on each side.
fn draw_with(design: Design, grounds: u32, per_side: u32) -> Drawn {
    let (dir, mapping, lib) = setup(grounds, per_side);
    let layout = layout_rails(&design, &rails(), &mapping, &lib).unwrap();
    Drawn {
        design,
        layout,
        mapping,
        lib,
        _dir: dir,
    }
}

fn boxes<'a>(d: &'a Drawn, refdes: &str) -> Vec<&'a PlacedPart> {
    let part = d
        .design
        .parts
        .iter()
        .position(|p| p.refdes == refdes)
        .unwrap();
    d.layout.parts.iter().filter(|p| p.part == part).collect()
}

/// The gap between two boxes' extents, as a number to compare.
fn gap(a: &PlacedPart, b: &PlacedPart) -> f64 {
    let dx = (a.extent.min.x - b.extent.max.x)
        .max(b.extent.min.x - a.extent.max.x)
        .max(0.0);
    let dy = (a.extent.min.y - b.extent.max.y)
        .max(b.extent.min.y - a.extent.max.y)
        .max(0.0);
    dx + dy
}

#[test]
fn a_symbol_of_two_units_is_two_boxes_with_the_units_letters() {
    let d = draw(board());
    let u = boxes(&d, "U1");
    assert_eq!(u.len(), 2);
    let letters: Vec<&str> = u.iter().map(|p| p.reference.text.as_str()).collect();
    assert_eq!(
        letters,
        ["U1B", "U1A"],
        "the box with most pins is the part's first"
    );
    // The parts keep their places in the layout; the other unit follows them.
    assert_eq!(d.layout.parts.len(), d.design.parts.len() + 1);
    assert_eq!(d.layout.parts[0].part, 0);
    assert_eq!(d.layout.parts[4].part, 0);
    // A part with one unit is one box.
    assert_eq!(boxes(&d, "C1").len(), 1);
    assert_eq!(boxes(&d, "J1").len(), 1);
    assert_eq!(boxes(&d, "J2").len(), 1);
    // Upright, as the library draws a unit.
    assert!(u.iter().all(|p| p.orient == Orient::IDENTITY));
    // Each pad is in exactly one box, and the boxes have all 50 pads of the symbol.
    let mut pads: Vec<&str> = u
        .iter()
        .flat_map(|p| p.pins.iter().map(|q| q.id.as_str()))
        .collect();
    assert_eq!(pads.len(), 50);
    pads.sort_unstable();
    pads.dedup();
    assert_eq!(pads.len(), 50, "a pad is in two boxes");
}

#[test]
fn the_power_unit_has_the_supplies_and_one_column_of_grounds_and_the_signals_no_power_row() {
    let d = draw(board());
    let u = boxes(&d, "U1");
    let (signals, power) = (u[0], u[1]);
    let at = |p: &PlacedPart, pad: &str| p.pins.iter().find(|q| q.id == pad).unwrap().at;
    // The twenty grounds are one column on the right of the power unit, a step of 2.54 mm apart.
    let first = at(power, "11");
    for (i, pad) in (11..31).enumerate() {
        let q = at(power, &pad.to_string());
        assert!(
            (q.x - first.x).abs() < 1e-6,
            "ground {pad} is not in the column"
        );
        assert!((q.y - first.y - 2.54 * i as f64).abs() < 1e-6);
    }
    // The supplies are on the other side of the same box.
    assert!(at(power, "2").x < first.x - 10.0);
    // No power pin in the signal unit, and no signal pin in the power unit.
    assert!(signals.glyph.pins.iter().all(|g| g.kind == "bidirectional"));
    assert!(power.glyph.pins.iter().all(|g| g.kind != "bidirectional"));
    assert!(signals.pins.iter().all(|q| {
        q.net
            .is_none_or(|n| d.design.nets[n].name.starts_with("SIG"))
    }));
}

#[test]
fn the_decoupling_capacitor_stands_by_the_power_unit_and_not_the_signals() {
    let d = draw(board());
    let u = boxes(&d, "U1");
    let c1 = boxes(&d, "C1")[0];
    assert!(
        gap(c1, u[1]) < gap(c1, u[0]),
        "C1 is {:.1} mm from the power unit and {:.1} mm from the signals",
        gap(c1, u[1]),
        gap(c1, u[0])
    );
    // Its supply and ground have a symbol each, near it.
    let plus = d.design.nets.iter().position(|n| n.name == "+3V3").unwrap();
    assert!(d.layout.symbols.iter().any(|s| s.net == plus
        && (s.at.x - c1.at.x).abs() < 30.0
        && (s.at.y - c1.at.y).abs() < 30.0));
}

#[test]
fn the_drawing_of_two_units_is_the_netlist_and_the_metrics_count_two_bodies() {
    let d = draw(board());
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
    let m = measure(&d.layout);
    assert_eq!(m.total.parts, 5, "U1A, U1B, C1, J1, J2");
    assert_eq!(m.total.units, 2);
    assert!(m.plain().contains("total.units = 2"));
    // Nothing is a satellite or a pair by being a box of a unit.
    assert_eq!(m.total.pair_satellites, 0);
}

#[test]
fn the_same_design_gives_the_same_bytes() {
    let (a, b) = (draw(board()), draw(board()));
    assert_eq!(a.layout, b.layout);
    assert_eq!(a.svg(), b.svg());
}

#[test]
fn the_svg_says_which_unit_a_box_is() {
    let d = draw(board());
    let svg = d.svg();
    assert!(svg.contains("data-ref=\"U1\" data-kind=\"Soc\" data-unit=\"A\""));
    assert!(svg.contains("data-ref=\"U1\" data-kind=\"Soc\" data-unit=\"B\""));
    assert!(svg.contains(", unit A</title>"));
    assert!(!svg.contains("data-ref=\"C1\" data-kind=\"Capacitor\" data-unit"));
}

#[test]
fn a_part_that_uses_a_pad_its_symbol_lacks_stays_one_box() {
    // Pad 999 is on no unit: the symbol cannot draw it, so the part is the box made from what
    // is known of it, not a part with units.
    let mut design = board();
    design.nets[2].pins.push(netsmith_core::PinRef {
        refdes: "U1".into(),
        pin: "999".into(),
    });
    let d = draw(design);
    assert_eq!(boxes(&d, "U1").len(), 1);
    assert_eq!(d.layout.parts.len(), d.design.parts.len());
}

#[test]
fn a_pad_drawn_in_two_boxes_of_one_part_is_found() {
    let d = draw(board());
    let mut l = d.layout.clone();
    // Put the first pin of the box of U1B into the box of U1A as well.
    let pin = l.parts[0].pins[0].clone();
    let glyph_pin = l.parts[0].glyph.pins[0].clone();
    l.parts[4].pins.push(pin);
    l.parts[4].glyph.pins.push(glyph_pin);
    let found = check(&d.design, &l);
    assert!(
        found
            .iter()
            .any(|v| v.kind() == "unit-pin" && v.to_string().contains("U1.101")),
        "{found:?}"
    );
    // And a pad no box draws is the pin-net rule's.
    let mut l = d.layout.clone();
    l.parts[4].pins.pop();
    l.parts[4].glyph.pins.pop();
    let found = check(&d.design, &l);
    assert!(found.iter().any(|v| v.kind() == "pin-net"), "{found:?}");
}

#[test]
fn a_symbol_with_one_unit_is_drawn_as_before() {
    let design = common::led();
    let (mapping, lib) = (
        Mapping::standard(),
        SymbolLib::at(netsmith_testkit::sample_symbols()),
    );
    let l = layout_rails(&design, &Rails::default(), &mapping, &lib).unwrap();
    assert_eq!(l.parts.len(), design.parts.len());
    assert!(l.parts.iter().all(|p| p.glyph.unit.is_none()));
}

#[test]
fn the_supply_of_a_short_power_unit_is_wired_just_above_it_and_not_above_the_tall_signal_unit() {
    // A power unit of twelve grounds beside a signal unit of fifty-eight pins that reaches well
    // above it: the supply's wire and symbol are over the power unit's pins, not over the
    // signals' top edge.
    let mut gnd: Vec<String> = (11..23).map(|i| format!("U1.{i}")).collect();
    gnd.push("C1.2".into());
    let gnd: Vec<&str> = gnd.iter().map(String::as_str).collect();
    let design = common::design(
        "ShortPower",
        &[
            ("U1", "Soc", "SOC", Some("M")),
            ("C1", "Capacitor", "10u", Some("0402")),
            ("J1", "Header1x2", "Conn_01x02", None),
            ("J2", "Header1x2", "Conn_01x02", None),
        ],
        &[
            ("+3V3", &["U1.1", "U1.2", "U1.3", "C1.1"]),
            ("GND", &gnd),
            ("SIG0", &["U1.101", "J1.1"]),
            ("SIG1", &["U1.201", "J2.1"]),
        ],
    );
    let d = draw_with(design, 12, 29);
    let u = boxes(&d, "U1");
    let (signals, power) = (u[0], u[1]);
    assert!(
        signals.extent.min.y < power.extent.min.y - 15.0,
        "the signals reach above the power unit"
    );
    let plus = d.design.nets.iter().position(|n| n.name == "+3V3").unwrap();
    let highest = d
        .layout
        .symbols
        .iter()
        .filter(|s| s.net == plus)
        .map(|s| s.at.y)
        .fold(f64::MAX, f64::min);
    assert!(
        highest > signals.extent.min.y,
        "the symbol at {highest:.1} is above the signal unit's top at {:.1}",
        signals.extent.min.y
    );
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
}

/// A symbol whose second unit has pins a hair off the grid, by different amounts (a library
/// made by hand): its pins would be dead ends of the wires drawn to the grid.
fn crooked_library() -> String {
    let mut first = String::new();
    first += &pin("bidirectional", -12.7, 2.54, 0, "A", 1);
    first += &pin("bidirectional", -12.7, 0.0, 0, "B", 2);
    first += &pin("bidirectional", 12.7, 2.54, 180, "C", 3);
    let mut second = String::new();
    second += &pin("power_in", 10.0, -12.7, 90, "GND", 11);
    second += &pin("power_in", 12.54, -12.7, 90, "GND", 12);
    second += &pin("power_in", 80.0, -12.7, 90, "GND", 13);
    second += &pin("power_in", 10.0, 12.7, 270, "VDD", 14);
    format!(
        "(kicad_symbol_lib (symbol \"Crooked\" (property \"Reference\" \"U\" (at 0 8 0)) \
         (symbol \"Crooked_1_1\" (rectangle (start -10.16 5.08) (end 10.16 -5.08) \
         (stroke (width 0.254) (type default)) (fill (type background))) {first}) \
         (symbol \"Crooked_2_1\" (rectangle (start 5 10.16) (end 90 -10.16) \
         (stroke (width 0.254) (type default)) (fill (type background))) {second})))"
    )
}

#[test]
fn a_unit_whose_pins_are_off_the_grid_by_different_amounts_is_drawn_with_its_pins_on_it() {
    let dir = TempDir::new("view-units-crooked");
    let file = dir.path().join("crooked.kicad_sym");
    std::fs::write(&file, crooked_library()).unwrap();
    let rows = "[[part]]\nkind = \"Crooked\"\npackage = \"M\"\nsymbol = \"crooked:Crooked\"\nfootprint = \"crooked:F\"\n";
    let mapping = Mapping::standard()
        .merged(Mapping::parse(rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let lib = SymbolLib::at(netsmith_testkit::sample_symbols()).with_library("crooked", file);
    let design = common::design(
        "Crooked",
        &[("U1", "Crooked", "X", Some("M"))],
        &[
            ("GND", &["U1.11", "U1.12", "U1.13"]),
            ("+3V3", &["U1.14"]),
            ("S", &["U1.1", "U1.3"]),
        ],
    );
    let l = layout_rails(&design, &rails(), &mapping, &lib).unwrap();
    assert_eq!(l.parts.len(), 2);
    let on = |v: f64| ((v / 1.27).round() * 1.27 - v).abs() < 1e-6;
    for p in &l.parts {
        for q in &p.pins {
            assert!(on(q.at.x) && on(q.at.y), "pin {} at {:?}", q.id, q.at);
        }
    }
    let found = check(&design, &l);
    assert!(found.is_empty(), "{found:?}");
}

// --- The layout file (ADR 0061) -------------------------------------------------------------

/// A file entry for `U1`, put in directly.
fn u1_hints(design: &Design, hint: netsmith_view::PartHint) -> netsmith_view::ResolvedHints {
    let mut h = netsmith_view::ResolvedHints::default();
    h.parts.insert(
        0,
        netsmith_view::ResolvedPart {
            sheet: design.name.clone(),
            path: design.parts[0].path.clone(),
            hint,
            line: 3,
            entry: "[UnitBoard.parts.\"u1\"]".into(),
        },
    );
    h
}

#[test]
fn a_hint_on_a_part_with_units_moves_the_primary_box_and_the_other_units_follow() {
    use netsmith_view::{PartHint, Place, Steps, layout_sheets_hinted_findings};
    let (_dir, mapping, lib) = setup(20, 13);
    let mut design = board();
    for p in &mut design.parts {
        p.path = p.refdes.to_lowercase();
    }
    let draw = |hints: &netsmith_view::ResolvedHints| {
        layout_sheets_hinted_findings(
            &design,
            &[],
            &rails(),
            &netsmith_view::Marks::default(),
            hints,
            &mapping,
            &lib,
        )
        .unwrap()
    };
    let (plain, _) = draw(&netsmith_view::ResolvedHints::default());
    assert_eq!(
        plain.parts.len(),
        design.parts.len() + 1,
        "two boxes for U1"
    );

    // A move: both boxes are carried by the same steps, so they keep the arrangement the
    // drawing gave them.
    let by = |x: i32, y: i32| {
        u1_hints(
            &design,
            PartHint {
                place: Some(Place::Offset(Steps::new(x, y))),
                ..PartHint::default()
            },
        )
    };
    let (moved, found) = draw(&by(0, 50));
    assert!(found.is_empty(), "{found:?}");
    for k in [0, plain.parts.len() - 1] {
        assert_eq!(plain.parts[k].part, 0);
        let (a, b) = (plain.parts[k].at, moved.parts[k].at);
        assert!(
            (b.x - a.x).abs() < 1e-6 && (b.y - a.y - 50.0 * 1.27).abs() < 1e-6,
            "box {k} of U1: {a:?} to {b:?}"
        );
    }
    assert!(check(&design, &moved).is_empty());

    // A turn is the primary box's alone: the other unit stays upright.
    let (turned, found) = draw(&u1_hints(
        &design,
        PartHint {
            rotate: Some(90),
            ..PartHint::default()
        },
    ));
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(turned.parts[0].orient, Orient::turned(false, 1));
    assert_eq!(
        turned.parts[plain.parts.len() - 1].orient,
        Orient::IDENTITY,
        "the other unit is drawn as the library draws it"
    );
    assert!(check(&design, &turned).is_empty());

    // A pin puts the primary box at the place and carries the other unit by the same steps.
    let pin = u1_hints(
        &design,
        PartHint {
            place: Some(Place::Pinned(Steps::new(0, 90))),
            ..PartHint::default()
        },
    );
    let (pinned, found) = draw(&pin);
    assert!(found.is_empty(), "{found:?}");
    let reference = plain
        .parts
        .iter()
        .position(|p| p.at.x.abs() < 1e-9 && p.at.y.abs() < 1e-9)
        .expect("a part at the origin");
    let _ = reference;
    let delta = |k: usize| {
        (
            pinned.parts[k].at.x - plain.parts[k].at.x,
            pinned.parts[k].at.y - plain.parts[k].at.y,
        )
    };
    let (d0, d1) = (delta(0), delta(plain.parts.len() - 1));
    assert!(
        (d0.0 - d1.0).abs() < 1e-6 && (d0.1 - d1.1).abs() < 1e-6,
        "{d0:?} {d1:?}"
    );
    assert!(check(&design, &pinned).is_empty());
}
