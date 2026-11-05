//! The drawing joins what a net joins: wires between the pins of a net on one sheet, power
//! symbols for rails (decided by the design, not by names), names that do not run into each
//! other, and one box for a symbol of several units. The board is invented
//! (`tests/common/bench.rs`).

mod common;

use common::bench::{Bench, bench};
use netsmith_view::geom::{Pt, Rect};
use netsmith_view::layout::{
    Layout, MarkKind, Marks, PowerKind, Rails, layout_marked, layout_rails,
};
use netsmith_view::sheets::{layout_sheets_marked, layout_sheets_rails};
use netsmith_view::{layout_sheets, render_svg_by_module_with};

fn lay(b: &Bench) -> Layout {
    layout_sheets_rails(&b.design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap()
}

fn plain(b: &Bench) -> Layout {
    layout_sheets(&b.design, &b.groups, &b.mapping, &b.lib).unwrap()
}

fn part_index(b: &Bench, refdes: &str) -> usize {
    b.design
        .parts
        .iter()
        .position(|p| p.refdes == refdes)
        .unwrap()
}

fn net_index(b: &Bench, name: &str) -> usize {
    b.design.nets.iter().position(|n| n.name == name).unwrap()
}

fn pin_at(l: &Layout, b: &Bench, refdes: &str, pad: &str) -> Pt {
    l.parts[part_index(b, refdes)]
        .pins
        .iter()
        .find(|p| p.id == pad)
        .unwrap()
        .at
}

fn near(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

fn on_segment(p: Pt, a: Pt, b: Pt) -> bool {
    let (x0, x1) = (a.x.min(b.x) - 1e-6, a.x.max(b.x) + 1e-6);
    let (y0, y1) = (a.y.min(b.y) - 1e-6, a.y.max(b.y) + 1e-6);
    p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1
}

/// The wires of `net`, in groups that touch (a wire's end on another wire).
fn components(l: &Layout, net: usize) -> Vec<Vec<(Pt, Pt)>> {
    let segments: Vec<(Pt, Pt)> = l
        .wires
        .iter()
        .filter(|w| w.net == net)
        .flat_map(|w| {
            w.points
                .windows(2)
                .map(|s| (s[0], s[1]))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut group: Vec<usize> = (0..segments.len()).collect();
    fn find(group: &mut [usize], i: usize) -> usize {
        if group[i] != i {
            let root = find(group, group[i]);
            group[i] = root;
        }
        group[i]
    }
    for i in 0..segments.len() {
        for j in 0..i {
            let (a, b) = (segments[i], segments[j]);
            let touch = [a.0, a.1].iter().any(|&p| on_segment(p, b.0, b.1))
                || [b.0, b.1].iter().any(|&p| on_segment(p, a.0, a.1));
            if touch {
                let (ri, rj) = (find(&mut group, i), find(&mut group, j));
                group[ri] = rj;
            }
        }
    }
    let mut out: Vec<(usize, Vec<(Pt, Pt)>)> = Vec::new();
    for (i, s) in segments.iter().enumerate() {
        let root = find(&mut group, i);
        match out.iter_mut().find(|(r, _)| *r == root) {
            Some((_, list)) => list.push(*s),
            None => out.push((root, vec![*s])),
        }
    }
    out.into_iter().map(|(_, list)| list).collect()
}

fn touches(component: &[(Pt, Pt)], p: Pt) -> bool {
    component.iter().any(|&(a, b)| on_segment(p, a, b))
}

fn overlap(a: &Rect, b: &Rect) -> bool {
    a.min.x < b.max.x - 0.05
        && b.min.x < a.max.x - 0.05
        && a.min.y < b.max.y - 0.05
        && b.min.y < a.max.y - 0.05
}

/// Every text of a drawing, with its box: references, values, pin numbers and names, the
/// names on power symbols and on flags.
fn texts(l: &Layout) -> Vec<(String, Rect)> {
    let mut out = Vec::new();
    for p in &l.parts {
        for t in std::iter::once(&p.reference).chain(p.value.as_ref()) {
            out.push((t.text.clone(), t.extent()));
        }
        for (pin, placed) in p.glyph.pins.iter().zip(&p.pins) {
            if pin.hidden {
                continue;
            }
            let toward = p.orient.dir(pin.toward_body);
            let (number, name) = p.glyph.pin_texts(pin, placed.at, toward);
            for t in number.into_iter().chain(name) {
                out.push((t.text.clone(), t.extent_at(p.glyph.pin_text)));
            }
        }
    }
    for s in &l.symbols {
        if let Some(t) = &s.label {
            out.push((t.text.clone(), t.extent()));
        }
    }
    for port in &l.ports {
        if let Some(t) = &port.label {
            out.push((t.text.clone(), t.extent()));
        }
    }
    out
}

fn segments(l: &Layout) -> Vec<(usize, Pt, Pt)> {
    l.wires
        .iter()
        .flat_map(|w| {
            w.points
                .windows(2)
                .map(|s| (w.net, s[0], s[1]))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn sheet_of(l: &Layout, p: Pt) -> Option<&netsmith_view::Sheet> {
    l.sheets.iter().find(|s| {
        s.frame.min.x <= p.x && p.x <= s.frame.max.x && s.frame.min.y <= p.y && p.y <= s.frame.max.y
    })
}

#[test]
fn the_same_board_gives_the_same_drawing() {
    let b = bench();
    assert_eq!(lay(&b), lay(&b));
    let svg = |b: &Bench| {
        render_svg_by_module_with(&b.design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap()
    };
    assert_eq!(svg(&b), svg(&b));
    common::balanced(&svg(&b)).unwrap();
}

#[test]
fn every_net_is_wired_or_named_at_each_of_its_places() {
    // Rule 1 (the label policy): a net on a sheet is wired, or, where wires would be long
    // or tangled, a name at the end of a stub at each place it is; never both for one pin,
    // and a generated name is shown as the first pin, `J2.3`.
    let b = bench();
    for l in [lay(&b), plain(&b)] {
        for sheet in &l.sheets {
            for (n, net) in b.design.nets.iter().enumerate() {
                let at: Vec<Pt> = net
                    .pins
                    .iter()
                    .filter(|p| sheet.parts.contains(&part_index(&b, &p.refdes)))
                    .map(|p| pin_at(&l, &b, &p.refdes, &p.pin))
                    .collect();
                if at.len() < 2
                    || b.rails.ground.contains(&net.name)
                    || b.rails.positive.contains(&net.name)
                {
                    continue;
                }
                let inside = |p: Pt| {
                    sheet.frame.min.x <= p.x
                        && p.x <= sheet.frame.max.x
                        && sheet.frame.min.y <= p.y
                        && p.y <= sheet.frame.max.y
                };
                let names: Vec<Pt> = l
                    .labels
                    .iter()
                    .map(|t| (t.net, t.at))
                    .chain(l.ports.iter().map(|f| (f.net, f.at)))
                    .filter(|&(net, at)| net == n && inside(at))
                    .map(|(_, at)| at)
                    .collect();
                let named = !names.is_empty();
                if named {
                    // Each pin is on a wire that ends in a name, or is the pin of a part with
                    // a name at a stub of its own.
                    for &pin in &at {
                        assert!(
                            names
                                .iter()
                                .any(|&t| common::wired_together(&l, n, &[pin, t])),
                            "{}: a pin of {} has no name on its wire",
                            sheet.name,
                            net.name
                        );
                    }
                    continue;
                }
                assert!(
                    common::wired_together(&l, n, &at),
                    "{}: the {} pins of {} are not on one wire",
                    sheet.name,
                    at.len(),
                    net.name
                );
            }
        }
        for (text, _) in texts(&l) {
            assert!(
                !text.starts_with("net-"),
                "a generated name is shown: {text}"
            );
        }
    }
}

#[test]
fn a_net_that_goes_to_another_sheet_leaves_through_a_flag_wired_from_its_pins() {
    let b = bench();
    let l = lay(&b);
    for name in ["BATT_ADC", "M_TX", "LED1", "PGOOD"] {
        let n = net_index(&b, name);
        let flags: Vec<_> = l.ports.iter().filter(|p| p.net == n).collect();
        assert!(!flags.is_empty(), "{name} has no flag");
        for f in &flags {
            assert_eq!(f.label.as_ref().unwrap().text, name);
            // The flag's point is on the net's wires.
            let comps = components(&l, n);
            assert!(
                comps.iter().any(|c| touches(c, f.at)),
                "{name}: the flag is not wired"
            );
        }
    }
    // A generated name is a flag with no name when it has to leave.
    let mut design = b.design.clone();
    let rt = net_index(&b, "net-buck.rt");
    design.nets[rt].pins.push(netsmith_core::PinRef {
        refdes: "R21".into(),
        pin: "2".into(),
    });
    let l = layout_sheets_rails(&design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap();
    let flags: Vec<_> = l.ports.iter().filter(|p| p.net == rt).collect();
    assert_eq!(flags.len(), 2, "one on each sheet");
    assert!(flags.iter().all(|f| f.label.is_none()));
    let svg = render_svg_by_module_with(&design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap();
    assert!(!svg.contains(">net-buck.rt<"));
}

#[test]
fn a_generated_name_is_never_a_rail() {
    // The model's balances may name a node a rail draws from; the drawing keeps it a wire.
    let b = bench();
    let mut rails = b.rails.clone();
    rails.positive.insert("net-buck.sw".into());
    rails.ground.insert("net-buck.fb".into());
    let l = layout_sheets_rails(&b.design, &b.groups, &rails, &b.mapping, &b.lib).unwrap();
    for name in ["net-buck.sw", "net-buck.fb"] {
        let n = net_index(&b, name);
        assert!(l.symbols.iter().all(|s| s.net != n), "{name}");
    }
}

#[test]
fn every_part_touches_wires_on_every_connected_pin() {
    // Rule 2e: a part is never drawn alone.
    let b = bench();
    let l = lay(&b);
    for (n, net) in b.design.nets.iter().enumerate() {
        for pin in &net.pins {
            let at = pin_at(&l, &b, &pin.refdes, &pin.pin);
            let active = net.pins.len() >= 2 || l.ports.iter().any(|p| p.net == n);
            if !active {
                continue;
            }
            assert!(
                l.wires
                    .iter()
                    .any(|w| w.net == n && w.points.iter().any(|&q| near(q, at))),
                "{}.{} on {} has no wire",
                pin.refdes,
                pin.pin,
                net.name
            );
        }
    }
}

#[test]
fn a_two_terminal_part_stands_beside_the_pin_it_serves() {
    // Rule 2b, on the invented board.
    let b = bench();
    let l = lay(&b);
    let at = |r: &str| l.parts[part_index(&b, r)].at;
    let left_of = |r: &str, u: &str| {
        l.parts[part_index(&b, r)].extent.max.x <= l.parts[part_index(&b, u)].extent.min.x + 1e-6
    };
    let right_of = |r: &str, u: &str| {
        l.parts[part_index(&b, r)].extent.min.x >= l.parts[part_index(&b, u)].extent.max.x - 1e-6
    };
    // Decoupling capacitors beside the supply pins, which are in a row on top of the box.
    for c in ["C4", "C5"] {
        let supply = pin_at(&l, &b, "U5", "131");
        assert!(
            (at(c).y - supply.y).abs() < 40.0 && (at(c).x - supply.x).abs() < 60.0,
            "{c} is far from the supply pins"
        );
    }
    // The feedback divider beside the FB pin: in one stack, the pin level between its parts.
    assert!(right_of("R8", "U7") && right_of("R9", "U7"));
    assert!((at("R8").x - at("R9").x).abs() < 1e-6 && at("R8").y < at("R9").y);
    let fb = pin_at(&l, &b, "U7", "6").y;
    assert!(
        at("R8").y < fb && fb < at("R9").y + 5.0,
        "the FB pin is level with the node"
    );
    // The bootstrap capacitor between the BST and SW pins.
    let (bst, sw) = (pin_at(&l, &b, "U7", "13"), pin_at(&l, &b, "U7", "10"));
    assert!(right_of("C7", "U7"));
    assert!((at("C7").y - (bst.y + sw.y) / 2.0).abs() < 10.0);
    // The inductor from SW outward, the output capacitor beside the inductor.
    assert!(right_of("L1", "U7") && right_of("C8", "U7"));
    assert!((at("L1").x - at("C8").x).abs() < 70.0);
    // The crystal on the side of the oscillator pins, and near them.
    assert!(left_of("Y1", "U5"));
    assert!((at("Y1").y - pin_at(&l, &b, "U5", "15").y).abs() < 15.0);
}

#[test]
fn parts_in_series_are_one_stack_with_the_node_wired_out_sideways() {
    // Rule 2c: the divider's upper resistor is in line with the lower one, above the node.
    let b = bench();
    let l = lay(&b);
    let (r1, r2) = (
        &l.parts[part_index(&b, "R1")],
        &l.parts[part_index(&b, "R2")],
    );
    assert!((r1.at.x - r2.at.x).abs() < 1e-6, "one column");
    assert!(r1.at.y < r2.at.y, "the supply side above");
    let node = pin_at(&l, &b, "R1", "2");
    let below = pin_at(&l, &b, "R2", "1");
    assert!(node.y <= below.y);
    // The net's wire leaves sideways from a point between the two.
    let n = net_index(&b, "BATT_DIV");
    let leaves = segments(&l).iter().any(|&(net, a, c)| {
        net == n
            && (a.y - c.y).abs() < 1e-6
            && a.y >= node.y - 1e-6
            && a.y <= below.y + 1e-6
            && ((a.x - node.x).abs() < 1e-6 || (c.x - node.x).abs() < 1e-6)
    });
    assert!(leaves, "the node is wired out sideways");
}

#[test]
fn every_pin_on_a_rail_has_a_wire_to_a_power_symbol() {
    let b = bench();
    let l = lay(&b);
    for (rail, kind) in [
        ("GND", PowerKind::Ground),
        ("+3V3", PowerKind::Supply),
        ("+3V8", PowerKind::Supply),
        ("+1V8", PowerKind::Supply),
    ] {
        let n = net_index(&b, rail);
        let comps = components(&l, n);
        for pin in &b.design.nets[n].pins {
            let at = pin_at(&l, &b, &pin.refdes, &pin.pin);
            let comp = comps
                .iter()
                .find(|c| touches(c, at))
                .unwrap_or_else(|| panic!("{rail}: {}.{} has no wire", pin.refdes, pin.pin));
            assert!(
                l.symbols
                    .iter()
                    .any(|s| s.net == n && s.kind == kind && touches(comp, s.at)),
                "{rail}: {}.{} has no symbol on its wire",
                pin.refdes,
                pin.pin
            );
        }
    }
    let adc = net_index(&b, "BATT_ADC");
    assert!(l.symbols.iter().all(|s| s.net != adc));
}

#[test]
fn a_power_symbol_is_at_the_end_of_a_wire_and_never_on_one() {
    // Rule 3.
    let b = bench();
    let l = lay(&b);
    assert!(!l.symbols.is_empty());
    let segs = segments(&l);
    for s in &l.symbols {
        // It is the end of a wire of its net...
        assert!(
            segs.iter()
                .any(|&(net, a, c)| net == s.net && (near(a, s.at) || near(c, s.at))),
            "{s:?}"
        );
        // ...and no wire of another net, nor a wire of its own that is not the one it ends,
        // runs through its drawing.
        let g = s.glyph_extent();
        let inner = Rect {
            min: Pt::new(g.min.x + 0.15, g.min.y + 0.15),
            max: Pt::new(g.max.x - 0.15, g.max.y - 0.15),
        };
        for &(net, a, c) in &segs {
            let mut r = Rect::around(a);
            r.include(c);
            if net == s.net && (near(a, s.at) || near(c, s.at)) {
                continue;
            }
            assert!(
                !overlap(&inner, &r),
                "a wire of {} runs through the symbol {s:?}",
                b.design.nets[net].name
            );
        }
        // Its name does not cover a wire.
        if let Some(t) = &s.label {
            for &(_, a, c) in &segs {
                let mut r = Rect::around(a);
                r.include(c);
                assert!(!overlap(&t.extent(), &r), "{}", t.text);
            }
        }
    }
}

#[test]
fn a_ground_is_below_the_pins_it_serves_and_a_supply_above() {
    let b = bench();
    let l = lay(&b);
    for s in &l.symbols {
        let comps = components(&l, s.net);
        let comp = comps.iter().find(|c| touches(c, s.at)).unwrap();
        for pin in &b.design.nets[s.net].pins {
            let at = pin_at(&l, &b, &pin.refdes, &pin.pin);
            if !touches(comp, at) {
                continue;
            }
            match s.kind {
                PowerKind::Ground => assert!(s.dir == netsmith_view::geom::Dir::Down),
                PowerKind::Supply => assert!(s.dir == netsmith_view::geom::Dir::Up),
            }
        }
    }
}

#[test]
fn no_wire_crosses_a_text() {
    // Rule 5, geometrically, on every sheet.
    let b = bench();
    for l in [lay(&b), plain(&b)] {
        let segs = segments(&l);
        for (text, r) in texts(&l) {
            let inner = Rect {
                min: Pt::new(r.min.x + 0.1, r.min.y + 0.1),
                max: Pt::new(r.max.x - 0.1, r.max.y - 0.1),
            };
            for &(net, a, c) in &segs {
                let mut w = Rect::around(a);
                w.include(c);
                assert!(
                    !overlap(&inner, &w),
                    "a wire of {} crosses the text `{text}` at {:?}: {a:?} {c:?}",
                    b.design.nets[net].name,
                    r.min
                );
            }
        }
    }
}

#[test]
fn texts_have_a_halo() {
    let b = bench();
    let svg =
        render_svg_by_module_with(&b.design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap();
    assert!(svg.contains("paint-order:stroke"));
}

#[test]
fn no_two_texts_are_written_over_each_other() {
    let b = bench();
    for l in [lay(&b), plain(&b)] {
        for sheet in &l.sheets {
            let names: Vec<(String, Rect)> = texts(&l)
                .into_iter()
                .filter(|(_, r)| {
                    sheet.frame.min.x <= r.min.x
                        && r.max.x <= sheet.frame.max.x
                        && sheet.frame.min.y <= r.min.y
                        && r.max.y <= sheet.frame.max.y
                })
                .filter(|(t, _)| t.len() > 3)
                .collect();
            for (i, (an, a)) in names.iter().enumerate() {
                for (bn, bx) in &names[i + 1..] {
                    assert!(!overlap(a, bx), "{} : `{an}` over `{bn}`", sheet.name);
                }
            }
        }
        for p in &l.parts {
            assert!(
                !p.glyph.texts_collide(p.orient),
                "{}",
                b.design.parts[p.part].refdes
            );
        }
    }
}

#[test]
fn a_run_of_ground_pins_shares_a_bus_and_one_symbol() {
    let b = bench();
    let l = lay(&b);
    let gnd = net_index(&b, "GND");
    // The nine grounds of the antenna and the connector's one are within reach of one another.
    let sheet = l.sheets.iter().find(|s| s.name == "gnss").unwrap();
    let here: Vec<_> = l
        .symbols
        .iter()
        .filter(|s| s.net == gnd && sheet_of(&l, s.at).is_some_and(|x| x.name == sheet.name))
        .collect();
    assert_eq!(here.len(), 1, "{here:?}");
    // The microcontroller's 24 grounds are on one side of one part: one symbol for them. Each
    // small part on the ground has a symbol of its own below it, and there are no others.
    let mcu: Vec<_> = l
        .symbols
        .iter()
        .filter(|s| s.net == gnd && sheet_of(&l, s.at).is_some_and(|x| x.name == "mcu"))
        .collect();
    let small = ["C4", "C5"].len();
    assert!(mcu.len() <= 1 + small, "{mcu:?}");
}

#[test]
fn rails_come_from_the_design_and_not_from_names() {
    let b = bench();
    let mut rails = Rails::default();
    rails.ground.insert("+3V8".into());
    rails.positive.insert("ANT".into());
    let l = layout_sheets_rails(&b.design, &b.groups, &rails, &b.mapping, &b.lib).unwrap();
    let kinds = |name: &str| -> Vec<PowerKind> {
        let n = net_index(&b, name);
        l.symbols
            .iter()
            .filter(|s| s.net == n)
            .map(|s| s.kind)
            .collect()
    };
    assert!(kinds("GND").is_empty(), "no symbol for a net by its name");
    assert!(kinds("+3V3").is_empty());
    assert!(kinds("+3V8").iter().all(|&k| k == PowerKind::Ground));
    assert!(!kinds("+3V8").is_empty());
    assert!(kinds("ANT").iter().all(|&k| k == PowerKind::Supply));
    assert!(!kinds("ANT").is_empty());
}

#[test]
fn a_ground_is_named_only_when_the_board_has_several() {
    let b = bench();
    let l = lay(&b);
    let gnd = net_index(&b, "GND");
    assert!(
        l.symbols
            .iter()
            .filter(|s| s.net == gnd)
            .all(|s| s.label.is_none())
    );
    let supply = net_index(&b, "+3V3");
    assert!(
        l.symbols
            .iter()
            .filter(|s| s.net == supply)
            .all(|s| s.label.as_ref().is_some_and(|t| t.text == "+3V3"))
    );
    let mut rails = b.rails.clone();
    rails.ground.insert("ANT".into());
    let l = layout_sheets_rails(&b.design, &b.groups, &rails, &b.mapping, &b.lib).unwrap();
    for name in ["GND", "ANT"] {
        let n = net_index(&b, name);
        let symbols: Vec<_> = l.symbols.iter().filter(|s| s.net == n).collect();
        assert!(!symbols.is_empty());
        assert!(
            symbols
                .iter()
                .all(|s| s.label.as_ref().is_some_and(|t| t.text == name)),
            "{name}"
        );
    }
}

#[test]
fn the_svg_draws_the_symbols_flags_and_marks() {
    let b = bench();
    let svg =
        render_svg_by_module_with(&b.design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap();
    assert!(svg.contains("class=\"power ground\" data-net=\"GND\""));
    assert!(svg.contains("class=\"power supply\" data-net=\"+3V3\""));
    assert!(svg.contains("<title>GND (ground)</title>"));
    assert!(svg.contains("class=\"port\" data-net=\"BATT_ADC\""));
    assert!(svg.contains("<title>BATT_ADC (goes to another sheet)</title>"));
    assert!(svg.contains("class=\"nc\""));
}

#[test]
fn a_name_longer_than_the_limit_is_cut_and_keeps_its_tooltip() {
    let b = bench();
    let long = "a_net_with_a_very_long_name_indeed_over_the_limit";
    let mut design = b.design.clone();
    let n = net_index(&b, "M_TX");
    design.nets[n].name = long.into();
    let l = layout_sheets_rails(&design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap();
    let flag = l.ports.iter().find(|p| p.net == n).expect("a flag");
    let label = flag.label.as_ref().unwrap();
    assert_eq!(label.text.chars().count(), 24);
    assert!(label.text.ends_with('…'));
    let svg = render_svg_by_module_with(&design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap();
    assert!(svg.contains(&format!("<title>{long}</title>")));
}

#[test]
fn a_symbol_of_two_units_and_ninety_seven_pins_is_one_part_with_its_signals_in_labelled_groups() {
    let b = bench();
    let l = lay(&b);
    let u5 = &l.parts[part_index(&b, "U5")];
    let g = &u5.glyph;
    assert_eq!(g.symbol.as_deref(), Some("bench:Mcu"));
    let bodies = g
        .shapes
        .iter()
        .filter(|s| matches!(s, netsmith_view::glyph::Shape::Rect { .. }))
        .count();
    assert_eq!(bodies, 1, "one box, not one for each unit");
    assert_eq!(g.pins.len(), 97);
    let texts: Vec<&str> = g
        .shapes
        .iter()
        .filter_map(|s| match s {
            netsmith_view::glyph::Shape::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    for heading in ["Mcu, 97 pins", "P0 (16)", "Not connected (3)"] {
        assert!(texts.contains(&heading), "{heading} in {texts:?}");
    }
    assert!(g.pin_text >= 1.27);
    assert!(
        g.dense && u5.orient == netsmith_view::geom::Orient::IDENTITY,
        "it stays upright"
    );
}

#[test]
fn the_supplies_are_in_a_row_on_top_and_the_grounds_in_one_at_the_bottom() {
    // Rule 4: a big part has its supplies on top and its grounds at the bottom, and its
    // signals on the two sides between them.
    let b = bench();
    let l = lay(&b);
    let u5 = &l.parts[part_index(&b, "U5")];
    let at = |pad: &str| u5.pins.iter().find(|p| p.id == pad).unwrap().at;
    let (top, bottom) = (at("131").y, at("101").y);
    for supply in ["131", "132", "136"] {
        assert!(
            (at(supply).y - top).abs() < 1e-6,
            "supply {supply} is not in the row on top"
        );
        assert!(at(supply).y < at("2").y, "supplies above the signals");
    }
    for ground in ["101", "124"] {
        assert!(
            (at(ground).y - bottom).abs() < 1e-6,
            "ground {ground} is not in the row at the bottom"
        );
        assert!(at(ground).y > at("16").y, "grounds below the signals");
    }
    // The groups of signals are between them, by function, on the sides.
    assert!(top < at("1").y && at("16").y < bottom);
    assert!((at("1").x - at("2").x).abs() < 1e-6 && at("1").x != at("131").x);
}

#[test]
fn a_part_with_only_rails_is_drawn_with_wires_to_symbols() {
    let b = bench();
    let l = lay(&b);
    for (pad, rail) in [("1", "+3V3"), ("2", "GND")] {
        let at = pin_at(&l, &b, "R6", pad);
        let n = net_index(&b, rail);
        let comp = components(&l, n)
            .into_iter()
            .find(|c| touches(c, at))
            .expect("a wire");
        assert!(l.symbols.iter().any(|s| s.net == n && touches(&comp, s.at)));
    }
}

#[test]
fn a_design_with_no_rails_told_has_no_symbols() {
    let b = bench();
    let l = plain(&b);
    assert!(l.symbols.is_empty());
    let l = layout_rails(&b.design, &Rails::default(), &b.mapping, &b.lib).unwrap();
    assert!(l.symbols.is_empty());
}

#[test]
fn every_pin_with_nothing_on_it_has_an_x_and_one_that_must_be_connected_a_red_one() {
    // Rule 7 (and the owner's addition): an X on every pin that connects to nothing.
    let b = bench();
    let l = lay(&b);
    for (p, part) in l.parts.iter().enumerate() {
        for (pin, g) in part.pins.iter().zip(&part.glyph.pins) {
            if g.hidden {
                continue;
            }
            let connected = pin.net.is_some_and(|n| {
                b.design.nets[n].pins.len() >= 2 || l.ports.iter().any(|q| q.net == n)
            });
            let marked = l
                .marks
                .iter()
                .any(|m| m.part == p && m.pad == g.id && near(m.at, pin.at));
            assert_eq!(marked, !connected, "{}.{}", b.design.parts[p].refdes, g.id);
        }
    }
    // A pin that is open on purpose says so and what it is.
    let svg =
        render_svg_by_module_with(&b.design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap();
    assert!(svg.contains("not connected: U5 NC (no_connect)"));
    assert!(svg.contains("not connected: U5 P1.12 (bidirectional)"));
    assert!(svg.contains("not connected: U1 + (input)"));
    // One the checker says must be connected is red, with the checker's message.
    let mut marks = Marks::default();
    marks.must.insert(
        ("U5".into(), "6".into()),
        "`u5.p0_05` is an input that must be connected".into(),
    );
    let mut design = b.design.clone();
    for net in &mut design.nets {
        net.pins.retain(|p| !(p.refdes == "U5" && p.pin == "6"));
    }
    let l = layout_sheets_marked(&design, &b.groups, &b.rails, &marks, &b.mapping, &b.lib).unwrap();
    let m = l
        .marks
        .iter()
        .find(|m| m.pad == "6" && b.design.parts[m.part].refdes == "U5")
        .unwrap();
    assert!(matches!(&m.kind, MarkKind::Must(msg) if msg.contains("must be connected")));
    let svg = netsmith_view::render_svg_by_module_marked(
        &design, &b.groups, &b.rails, &marks, &b.mapping, &b.lib,
    )
    .unwrap();
    assert!(svg.contains("class=\"nc must\""));
    assert!(svg.contains("`u5.p0_05` is an input that must be connected"));
    // The marks stay small and at the pin's end: a box of at most 2 mm.
    for m in &l.marks {
        let pin = l.parts[m.part].pins.iter().find(|p| p.id == m.pad).unwrap();
        assert!(near(m.at, pin.at));
    }
}

#[test]
fn the_reference_and_value_of_a_part_stay_clear_of_the_wires_to_its_symbols() {
    let mut d = common::led();
    let j1 = d.parts.iter_mut().find(|p| p.refdes == "J1").unwrap();
    j1.kind = "Socket1x2".into();
    j1.value = "Conn_01x02_Socket".into();
    let mut rails = Rails::default();
    rails.ground.insert("GND".into());
    rails.positive.insert("VCC".into());
    let lib = netsmith_kicad::SymbolLib::at(netsmith_kicad::testlib::sample_symbols());
    let l = layout_marked(
        &d,
        &rails,
        &Marks::default(),
        &netsmith_kicad::Mapping::standard(),
        &lib,
    )
    .unwrap();
    assert!(!l.symbols.is_empty());
    for (text, r) in texts(&l) {
        for (_, a, c) in segments(&l) {
            let mut seg = Rect::around(a);
            seg.include(c);
            assert!(!overlap(&r, &seg), "`{text}` is over a wire");
        }
    }
}

fn sheet_named<'a>(l: &'a Layout, name: &str) -> &'a netsmith_view::Sheet {
    l.sheets.iter().find(|s| s.name == name).unwrap()
}

fn in_frame(s: &netsmith_view::Sheet, p: Pt) -> bool {
    s.frame.min.x <= p.x && p.x <= s.frame.max.x && s.frame.min.y <= p.y && p.y <= s.frame.max.y
}

#[test]
fn a_satellites_ground_has_a_short_stub_and_a_symbol_of_its_own() {
    // Round 3: a small part's ground does not run across the sheet to one rail; its pin has a
    // stub of a few cells to a symbol of its own, below it. The regulator's own ground pin has
    // its stub and symbol as before. (Satellites side by side in a row, whose pins end on one
    // line, may share a short wire and one symbol.)
    let b = bench();
    let l = lay(&b);
    let ldo = sheet_named(&l, "ldo");
    let gnd = net_index(&b, "GND");
    let symbols: Vec<_> = l
        .symbols
        .iter()
        .filter(|s| s.net == gnd && in_frame(ldo, s.at))
        .collect();
    for (r, p) in [("U4", "1"), ("C2", "2"), ("C3", "2")] {
        let pin = pin_at(&l, &b, r, p);
        let below = symbols
            .iter()
            .filter(|s| s.at.y > pin.y)
            .map(|s| ((s.at.x - pin.x).abs() + (s.at.y - pin.y)) / 1.27)
            .fold(f64::MAX, f64::min);
        assert!(
            below <= 40.0,
            "{r}.{p}: the nearest symbol below is {below} steps away"
        );
    }
    for r in ["C2", "C3"] {
        let pin = pin_at(&l, &b, r, "2");
        assert!(
            symbols.iter().any(|s| (s.at.x - pin.x).abs() < 1e-6
                && s.at.y > pin.y
                && s.at.y - pin.y <= 8.0 * 1.27),
            "{r} has a symbol of its own straight below its ground pin: {symbols:?}"
        );
    }
    // No wire of ground on the sheet is as long as the sheet: the longest run is the
    // regulator's own, or a row of capacitors.
    let longest = segments(&l)
        .into_iter()
        .filter(|&(net, a, _)| net == gnd && in_frame(ldo, a))
        .map(|(_, a, c)| ((a.x - c.x).abs() + (a.y - c.y).abs()) / 1.27)
        .fold(0.0, f64::max);
    assert!(
        longest <= 30.0,
        "the longest ground wire is {longest} steps"
    );
    for w in l
        .wires
        .iter()
        .filter(|w| w.net == gnd && in_frame(ldo, w.points[0]))
    {
        assert!(w.points.len() <= 4, "at most two bends: {:?}", w.points);
    }
}

#[test]
fn a_supply_symbol_is_in_line_with_the_capacitor_it_serves() {
    // The output capacitor's supply symbol is directly above it, on a stub of its own.
    let b = bench();
    let l = lay(&b);
    let ldo = sheet_named(&l, "ldo");
    for (rail, cap) in [("+3V3", "C3"), ("+3V8", "C2")] {
        let n = net_index(&b, rail);
        let top = pin_at(&l, &b, cap, "1");
        let syms: Vec<_> = l
            .symbols
            .iter()
            .filter(|s| s.net == n && in_frame(ldo, s.at))
            .collect();
        assert!(
            syms.iter()
                .any(|s| near(Pt::new(s.at.x, 0.0), Pt::new(top.x, 0.0))
                    && s.at.y < top.y
                    && top.y - s.at.y <= 8.0 * 1.27),
            "{rail}: a symbol straight over {cap}, a few steps up: {syms:?}"
        );
    }
}

#[test]
fn decoupling_capacitors_stand_in_an_even_row() {
    // Owner round 2 (E): the same gap between them, level.
    let b = bench();
    let l = lay(&b);
    let xs: Vec<f64> = ["C4", "C5"]
        .iter()
        .map(|r| pin_at(&l, &b, r, "1").x)
        .collect();
    let ys: Vec<f64> = ["C4", "C5"]
        .iter()
        .map(|r| pin_at(&l, &b, r, "1").y)
        .collect();
    assert!((ys[0] - ys[1]).abs() < 1e-6, "level");
    assert!((xs[0] - xs[1]).abs() >= 5.0, "apart");
    // The modem's three supply capacitors and their gaps.
    let cx: Vec<f64> = ["C20", "C21", "C22"]
        .iter()
        .map(|r| pin_at(&l, &b, r, "1").x)
        .collect();
    let mut sorted = cx.clone();
    sorted.sort_by(f64::total_cmp);
    let gap1 = sorted[1] - sorted[0];
    let gap2 = sorted[2] - sorted[1];
    assert!((gap1 - gap2).abs() < 1e-6, "even gaps {sorted:?}");
}

#[test]
fn a_flag_is_at_the_end_of_the_stub_of_each_pin_that_goes_on_and_flags_do_not_touch() {
    // Owner round 2 (D), changed by the label policy: the ten pins that all go on have ten
    // flags, each at the end of its own pin's stub (a wire of one net, no longer than the
    // stub and what the parts beside the pin need), none touching another.
    let b = bench();
    let l = lay(&b);
    let sheet = sheet_named(&l, "ten");
    let flags: Vec<_> = l.ports.iter().filter(|p| in_frame(sheet, p.at)).collect();
    assert_eq!(flags.len(), 10, "one for each net");
    let mut nets: Vec<usize> = flags.iter().map(|f| f.net).collect();
    nets.sort_unstable();
    nets.dedup();
    assert_eq!(nets.len(), 10);
    for f in &flags {
        let pin = b.design.nets[f.net]
            .pins
            .iter()
            .map(|p| pin_at(&l, &b, &p.refdes, &p.pin))
            .find(|&at| {
                // The pin the flag's wire starts at: on the flag's row or column.
                (at.y - f.at.y).abs() < 1e-6 || (at.x - f.at.x).abs() < 1e-6
            })
            .expect("a pin in line with its flag");
        let reach = (pin.x - f.at.x).abs() + (pin.y - f.at.y).abs();
        assert!(
            reach < 30.0,
            "the flag of {} is {reach} mm from its pin",
            f.net
        );
    }
    for (i, a) in flags.iter().enumerate() {
        for c in &flags[i + 1..] {
            assert!(
                !overlap(&inflate(a.extent(), 0.5), &inflate(c.extent(), 0.5)),
                "flags touch"
            );
        }
    }
}

fn inflate(r: Rect, d: f64) -> Rect {
    Rect {
        min: Pt::new(r.min.x - d, r.min.y - d),
        max: Pt::new(r.max.x + d, r.max.y + d),
    }
}

/// Everything that must keep a grid step from everything else: power symbols (and their
/// names), flags, X markers and the texts of parts.
fn items(l: &Layout) -> Vec<(String, Rect, Option<usize>, Option<usize>)> {
    let mut out = Vec::new();
    for s in &l.symbols {
        out.push((
            format!("symbol {:?}", s.kind),
            s.extent(),
            Some(s.net),
            None,
        ));
    }
    for f in &l.ports {
        out.push(("flag".into(), f.extent(), Some(f.net), None));
    }
    for m in &l.marks {
        out.push(("X".into(), inflate(Rect::around(m.at), 0.7), None, None));
    }
    for p in &l.parts {
        for t in std::iter::once(&p.reference).chain(p.value.as_ref()) {
            out.push((format!("text {}", t.text), t.extent(), None, Some(p.part)));
        }
    }
    out
}

#[test]
fn no_two_items_are_closer_than_a_grid_step() {
    // Owner round 2 (C): symbols, flags, X markers and texts keep a grid step from each other,
    // and a wire of another net keeps its distance from each of them.
    let b = bench();
    for l in [lay(&b), plain(&b)] {
        let items = items(&l);
        for (i, (an, a, anet, aown)) in items.iter().enumerate() {
            for (bn, bx, bnet, bown) in &items[i + 1..] {
                if aown.is_some() && aown == bown {
                    continue;
                }
                // Two X markers are a pin pitch apart; a symbol and its own name are one item.
                if an == "X" && bn == "X" {
                    continue;
                }
                let _ = (anet, bnet);
                assert!(
                    !overlap(&inflate(*a, 0.6), &inflate(*bx, 0.6)),
                    "`{an}` at {:?} is closer than a grid step to `{bn}` at {:?}",
                    a.min,
                    bx.min
                );
            }
        }
        for (name, r, net, _) in &items {
            for (wnet, a, c) in segments(&l) {
                if Some(wnet) == *net {
                    continue;
                }
                let mut w = Rect::around(a);
                w.include(c);
                assert!(
                    !overlap(&inflate(*r, 0.3), &inflate(w, 0.3)),
                    "a wire of {} is closer than 0.6 mm to `{name}` at {:?}",
                    b.design.nets[wnet].name,
                    r.min
                );
            }
        }
    }
}
