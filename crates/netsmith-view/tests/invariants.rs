//! The rules every drawing keeps whatever it looks like (`netsmith_view::check`): rebuilt
//! from the drawing alone it is the design's netlist, no wire runs along another net's wire,
//! no wire goes through a symbol's body, and nothing is drawn that serves nothing (no wire
//! end that touches nothing, no double line, no hook, no dot without three arms, no three
//! arms without a dot) and no two texts overlap. Run on every board the tests draw, with and
//! without rails told. Each rule is also shown to catch a fault put into a drawing.

mod common;

use common::corpus::corpus;
use netsmith_core::Design;
use netsmith_view::check::{Violation, check};
use netsmith_view::geom::Pt;
use netsmith_view::layout::{Layout, Wire};

fn of_kind<'a>(found: &'a [Violation], kinds: &[&str]) -> Vec<&'a Violation> {
    found.iter().filter(|v| kinds.contains(&v.kind())).collect()
}

/// Draws every board, both ways, and fails with every violation of `kinds`.
fn hold(kinds: &[&str]) {
    let mut failures = Vec::new();
    for case in corpus() {
        for (how, layout) in [("rails", case.drawn()), ("plain", case.drawn_plain())] {
            let found = check(&case.design, &layout);
            for v in of_kind(&found, kinds) {
                failures.push(format!("{} ({how}): {v}", case.name));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const NETLIST: &[&str] = &[
    "short",
    "open",
    "extra-pin",
    "pin-net",
    "floating-wire",
    "floating-flag",
    "floating-label",
];

#[test]
fn the_drawing_is_the_netlist_on_every_board() {
    // Rebuilt from wire ends, joins, junctions, power symbols, flags and labels alone: no short
    // (two nets in one piece), no open (a net in two pieces with nothing to join them by
    // name), no pin that is in the design's net and not drawn so, no stray wire.
    hold(NETLIST);
}

#[test]
fn no_wire_runs_along_another_nets_wire() {
    hold(&["wire-overlap"]);
}

// Fixed in stage 2 (2026-11-05): the engine laid a wire through a capacitor's plates. Smallest
// case, `supplies(1)` drawn with no rails told: a vertical run of the net `sense1` went one
// grid step beside the pin line of `CC1`, across its 4.06 mm wide plate. The test library's
// capacitor is one plate 0.25 mm thick that lies between two rows of the router's grid, so
// no node was inside it and the wire went between the nodes. Seen too on the bench board
// (`M_CTS` through `C20`, `GND` through `C4`, `C5`, `C25`, `+1V8` through `C8`).
#[test]
fn no_wire_goes_through_a_symbol() {
    hold(&["wire-through-body"]);
}

#[test]
fn the_smallest_case_of_a_wire_through_a_symbol() {
    let (design, groups) = common::supplies(1);
    let mapping = netsmith_kicad::Mapping::standard();
    let lib = netsmith_kicad::SymbolLib::at(netsmith_kicad::testlib::sample_symbols());
    let l = netsmith_view::layout_sheets(&design, &groups, &mapping, &lib).unwrap();
    let found = check(&design, &l);
    assert!(found.is_empty(), "{found:?}");
}

// --- The rules catch what they are for -------------------------------------------------

fn regulator() -> (Design, Layout) {
    let case = corpus().remove(2);
    assert_eq!(case.name, "RegulatorExample");
    let layout = case.drawn_plain();
    (case.design, layout)
}

fn net(d: &Design, name: &str) -> usize {
    d.nets.iter().position(|n| n.name == name).unwrap()
}

fn pin_at(l: &Layout, d: &Design, refdes: &str, pad: &str) -> Pt {
    let i = d.parts.iter().position(|p| p.refdes == refdes).unwrap();
    l.parts[i].pins.iter().find(|p| p.id == pad).unwrap().at
}

#[test]
fn a_clean_drawing_has_no_violation() {
    let (d, l) = regulator();
    let found = check(&d, &l);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_wire_between_two_nets_is_a_short() {
    let (d, mut l) = regulator();
    let (a, b) = (pin_at(&l, &d, "J2", "1"), pin_at(&l, &d, "J1", "1"));
    l.wires.push(Wire {
        net: net(&d, "3V3"),
        points: vec![a, Pt::new(a.x, b.y), b],
    });
    let found = check(&d, &l);
    let shorts = of_kind(&found, &["short"]);
    assert!(!shorts.is_empty(), "{found:?}");
    let text = shorts[0].to_string();
    assert!(text.contains("3V3") && text.contains("VIN"), "{text}");
}

#[test]
fn a_net_with_a_wire_taken_out_is_open() {
    let (d, mut l) = regulator();
    let gnd = net(&d, "GND");
    l.wires.retain(|w| w.net != gnd);
    let found = check(&d, &l);
    let opens = of_kind(&found, &["open"]);
    assert_eq!(opens.len(), 1, "{found:?}");
    assert!(opens[0].to_string().contains("GND"), "{}", opens[0]);
}

#[test]
fn a_flag_joins_the_pieces_of_a_net_by_name() {
    // Cut a net into two pieces and put a flag of the net on each: that is not an open.
    let (d, mut l) = regulator();
    let vin = net(&d, "VIN");
    let kept: Vec<Wire> = l.wires.iter().filter(|w| w.net == vin).cloned().collect();
    assert!(kept.len() >= 2, "VIN has {} wires", kept.len());
    l.wires
        .retain(|w| w.net != vin || std::ptr::eq(w, &kept[0]) || *w == kept[0]);
    let before = check(&d, &l);
    assert!(!of_kind(&before, &["open"]).is_empty(), "{before:?}");
    for w in &kept[1..] {
        l.ports.push(netsmith_view::Port {
            net: vin,
            at: w.points[0],
            dir: netsmith_view::geom::Dir::Up,
            length: 3.0,
            label: None,
        });
    }
    // The flags sit on the loose ends, but the first wire is still alone: still open.
    l.wires.extend(kept[1..].iter().cloned());
    let after = check(&d, &l);
    assert!(
        of_kind(&after, &["open", "short"]).is_empty(),
        "{:?}",
        of_kind(&after, &["open", "short"])
    );
}

#[test]
fn a_pin_taken_out_of_its_net_but_still_wired_is_an_extra_pin() {
    let (mut d, l) = regulator();
    let gnd = net(&d, "GND");
    d.nets[gnd]
        .pins
        .retain(|p| !(p.refdes == "C1" && p.pin == "2"));
    let found = check(&d, &l);
    assert!(!of_kind(&found, &["extra-pin"]).is_empty(), "{found:?}");
    assert!(!of_kind(&found, &["pin-net"]).is_empty(), "{found:?}");
}

#[test]
fn a_second_wire_laid_along_another_net_is_found() {
    let (d, mut l) = regulator();
    let gnd = net(&d, "GND");
    let mut copy = l.wires.iter().find(|w| w.net == gnd).unwrap().clone();
    copy.net = net(&d, "VIN");
    l.wires.push(copy);
    let found = check(&d, &l);
    assert!(!of_kind(&found, &["wire-overlap"]).is_empty(), "{found:?}");
    assert!(!of_kind(&found, &["short"]).is_empty(), "{found:?}");
}

#[test]
fn a_wire_across_a_body_is_found() {
    let (d, mut l) = regulator();
    let u1 = d.parts.iter().position(|p| p.refdes == "U1").unwrap();
    let at = l.parts[u1].at;
    // Far from every pin: only the body is crossed.
    l.wires.push(Wire {
        net: net(&d, "GND"),
        points: vec![Pt::new(at.x - 30.0, at.y), Pt::new(at.x + 30.0, at.y)],
    });
    let found = check(&d, &l);
    let through = of_kind(&found, &["wire-through-body"]);
    assert!(
        through.iter().any(|v| v.to_string().contains("body of U1")),
        "{found:?}"
    );
}

#[test]
fn a_flag_on_nothing_is_floating() {
    let (d, mut l) = regulator();
    l.ports.push(netsmith_view::Port {
        net: net(&d, "GND"),
        at: Pt::new(-200.0, -200.0),
        dir: netsmith_view::geom::Dir::Right,
        length: 3.0,
        label: None,
    });
    let found = check(&d, &l);
    assert_eq!(of_kind(&found, &["floating-flag"]).len(), 1, "{found:?}");
    assert!(found[0].to_string().contains("GND"), "{}", found[0]);
}

#[test]
fn a_wire_on_nothing_is_floating() {
    let (d, mut l) = regulator();
    l.wires.push(Wire {
        net: net(&d, "GND"),
        points: vec![Pt::new(-200.0, -200.0), Pt::new(-190.0, -200.0)],
    });
    let found = check(&d, &l);
    assert_eq!(of_kind(&found, &["floating-wire"]).len(), 1, "{found:?}");
}

// --- Nothing is drawn that serves nothing ------------------------------------------------

const STRAY: &[&str] = &[
    "dead-end",
    "redundant-wire",
    "zero-length-wire",
    "u-turn",
    "unused-junction",
    "missing-junction",
];

// Found in stage 4 (2026-11-05) on the private board of the owner's complaint: the rail of
// `+3V3` was drawn from its symbol to the farthest pin it might serve, and the pins were then
// joined to each other by a bus, so the rail ran 75 px past its last tap and ended in mid-air.
// The engine now takes every end that touches nothing off its wires when it has routed them.
#[test]
fn no_wire_ends_in_mid_air_on_any_board() {
    hold(&["dead-end"]);
}

#[test]
fn no_wire_lies_along_another_wire_of_its_net() {
    hold(&["redundant-wire", "zero-length-wire"]);
}

#[test]
fn no_wire_hooks_back_on_itself() {
    hold(&["u-turn"]);
}

#[test]
fn a_dot_is_where_three_arms_meet_and_only_there() {
    hold(&["unused-junction", "missing-junction"]);
}

#[test]
fn no_wire_runs_along_a_pin_or_hugs_a_symbol() {
    hold(&["wire-along-pin", "wire-hugs-symbol"]);
}

#[test]
fn no_two_texts_overlap_and_no_wire_runs_through_one() {
    hold(&["text-overlap", "wire-over-text"]);
}

#[test]
fn nothing_that_serves_nothing_is_in_a_clean_drawing() {
    let (d, l) = regulator();
    assert!(of_kind(&check(&d, &l), STRAY).is_empty());
}

fn first_wire_of(l: &Layout, net: usize) -> usize {
    l.wires.iter().position(|w| w.net == net).unwrap()
}

#[test]
fn a_wire_that_goes_on_past_its_last_use_has_a_dead_end() {
    // The wire of GND that ends at a pin is made to go on 5 mm past it: the old end is a pin
    // and the new one touches nothing.
    let (d, mut l) = regulator();
    let gnd = net(&d, "GND");
    let i = first_wire_of(&l, gnd);
    let pts = &mut l.wires[i].points;
    let (a, b) = (pts[pts.len() - 2], pts[pts.len() - 1]);
    let len = (b.x - a.x).abs() + (b.y - a.y).abs();
    let end = Pt::new(
        b.x + (b.x - a.x) / len * 5.08,
        b.y + (b.y - a.y) / len * 5.08,
    );
    pts.push(end);
    let found = check(&d, &l);
    let dead = of_kind(&found, &["dead-end"]);
    assert_eq!(dead.len(), 1, "{found:?}");
    let text = dead[0].to_string();
    assert!(
        text.contains("GND") && text.contains("touches nothing"),
        "{text}"
    );
}

#[test]
fn a_second_wire_over_the_same_run_is_redundant() {
    let (d, mut l) = regulator();
    let i = first_wire_of(&l, net(&d, "GND"));
    let copy = l.wires[i].clone();
    l.wires.push(copy);
    let found = check(&d, &l);
    let doubled = of_kind(&found, &["redundant-wire"]);
    assert!(!doubled.is_empty(), "{found:?}");
    assert!(doubled[0].to_string().contains("GND"), "{}", doubled[0]);
}

#[test]
fn a_wire_that_is_a_point_is_zero_length() {
    let (d, mut l) = regulator();
    let at = pin_at(&l, &d, "J1", "1");
    l.wires.push(Wire {
        net: net(&d, "VIN"),
        points: vec![at, at],
    });
    let found = check(&d, &l);
    assert_eq!(of_kind(&found, &["zero-length-wire"]).len(), 1, "{found:?}");
}

#[test]
fn a_wire_that_leaves_a_pin_and_comes_back_beside_itself_is_a_hook() {
    let (d, mut l) = regulator();
    let a = pin_at(&l, &d, "J1", "1");
    l.wires.push(Wire {
        net: net(&d, "VIN"),
        points: vec![
            a,
            Pt::new(a.x, a.y - 2.54),
            Pt::new(a.x + 1.27, a.y - 2.54),
            Pt::new(a.x + 1.27, a.y),
        ],
    });
    let found = check(&d, &l);
    let hooks = of_kind(&found, &["u-turn"]);
    assert_eq!(hooks.len(), 1, "{found:?}");
    assert!(hooks[0].to_string().contains("VIN"), "{}", hooks[0]);
}

#[test]
fn a_dot_on_nothing_or_on_a_straight_wire_is_unused() {
    let (d, mut l) = regulator();
    let gnd = net(&d, "GND");
    let i = first_wire_of(&l, gnd);
    let (a, b) = (l.wires[i].points[0], l.wires[i].points[1]);
    l.junctions.push(netsmith_view::layout::Junction {
        net: gnd,
        at: Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
    });
    l.junctions.push(netsmith_view::layout::Junction {
        net: gnd,
        at: Pt::new(-200.0, -200.0),
    });
    let found = check(&d, &l);
    assert_eq!(of_kind(&found, &["unused-junction"]).len(), 2, "{found:?}");
}

#[test]
fn a_dot_taken_off_a_tee_is_missing() {
    let (d, mut l) = regulator();
    assert!(!l.junctions.is_empty(), "the regulator has a dot");
    let gone = l.junctions.remove(0);
    let found = check(&d, &l);
    let missing = of_kind(&found, &["missing-junction"]);
    assert_eq!(missing.len(), 1, "{found:?}");
    assert!(
        missing[0].to_string().contains(&d.nets[gone.net].name),
        "{}",
        missing[0]
    );
}

#[test]
fn a_wire_along_a_pins_line_is_found() {
    let (d, mut l) = regulator();
    let u1 = d.parts.iter().position(|p| p.refdes == "U1").unwrap();
    let part = &l.parts[u1];
    let (g, placed) = (&part.glyph.pins[0], &part.pins[0]);
    let inner = placed.at.step(part.orient.dir(g.toward_body), g.length);
    l.wires.push(Wire {
        net: net(&d, "GND"),
        points: vec![placed.at, inner],
    });
    let found = check(&d, &l);
    assert!(
        !of_kind(&found, &["wire-along-pin"]).is_empty(),
        "{found:?}"
    );
}

#[test]
fn a_wire_close_beside_a_body_hugs_it() {
    let (d, mut l) = regulator();
    let u1 = d.parts.iter().position(|p| p.refdes == "U1").unwrap();
    let part = &l.parts[u1];
    let b = part.glyph.body().unwrap();
    let corner = |x: f64, y: f64| {
        let p = part.orient.apply((x, y));
        Pt::new(p.x + part.at.x, p.y + part.at.y)
    };
    let (p, q) = (corner(b.min.x, b.min.y), corner(b.max.x, b.max.y));
    let (left, right) = (p.x.min(q.x), p.x.max(q.x));
    let top = p.y.min(q.y);
    l.wires.push(Wire {
        net: net(&d, "GND"),
        points: vec![Pt::new(left, top - 0.3), Pt::new(right, top - 0.3)],
    });
    let found = check(&d, &l);
    let hugs = of_kind(&found, &["wire-hugs-symbol"]);
    assert!(
        hugs.iter().any(|v| v.to_string().contains("U1")),
        "{found:?}"
    );
}

#[test]
fn two_texts_written_over_each_other_are_found() {
    let (d, mut l) = regulator();
    let at = l.parts[0].reference.at;
    l.parts[1].reference.at = at;
    let found = check(&d, &l);
    assert!(!of_kind(&found, &["text-overlap"]).is_empty(), "{found:?}");
}

#[test]
fn a_wire_through_a_text_is_found() {
    let (d, mut l) = regulator();
    let r = l.parts[0].reference.extent();
    let y = (r.min.y + r.max.y) / 2.0;
    l.wires.push(Wire {
        net: net(&d, "GND"),
        points: vec![Pt::new(r.min.x - 3.0, y), Pt::new(r.max.x + 3.0, y)],
    });
    let found = check(&d, &l);
    assert!(
        !of_kind(&found, &["wire-over-text"]).is_empty(),
        "{found:?}"
    );
}

/// Not a check: `NETSMITH_DUMP_SVG=<folder>` writes the picture of every board of the corpus,
/// drawn as the editor draws it (`<name>-rails.svg`) and with no rails told
/// (`<name>-plain.svg`), to look at when a rule above fails.
#[test]
fn dump_the_corpus_when_asked() {
    let Ok(dir) = std::env::var("NETSMITH_DUMP_SVG") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    for case in corpus() {
        for (how, rails) in [("rails", case.rails.clone()), ("plain", Default::default())] {
            let svg = netsmith_view::render_svg_by_module_with(
                &case.design,
                &case.groups,
                &rails,
                &case.mapping,
                &case.lib,
            )
            .unwrap();
            std::fs::write(format!("{dir}/{}-{how}.svg", case.name), svg).unwrap();
        }
    }
}
