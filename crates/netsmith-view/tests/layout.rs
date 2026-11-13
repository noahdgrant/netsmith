//! Layout: where parts and pins land, and that every net is drawn.

use crate::common;

use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_view::geom::{Dir, Pt};
use netsmith_view::layout::{Layout, layout};

fn lay(d: &netsmith_core::Design) -> Layout {
    layout(
        d,
        &Mapping::standard(),
        &SymbolLib::at(netsmith_testkit::sample_symbols()),
    )
    .unwrap()
}

fn part<'a>(
    l: &'a Layout,
    d: &netsmith_core::Design,
    refdes: &str,
) -> &'a netsmith_view::layout::PlacedPart {
    let i = d.parts.iter().position(|p| p.refdes == refdes).unwrap();
    &l.parts[i]
}

fn pin_at(l: &Layout, d: &netsmith_core::Design, refdes: &str, pad: &str) -> Pt {
    part(l, d, refdes)
        .pins
        .iter()
        .find(|p| p.id == pad)
        .unwrap()
        .at
}

fn near(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

#[test]
fn layout_is_deterministic() {
    for d in common::all() {
        assert_eq!(lay(&d), lay(&d), "{}", d.name);
    }
}

#[test]
fn pins_sit_where_the_symbol_says_with_y_flipped() {
    // A lone resistor keeps the library's orientation: pin 1 is above the origin in the
    // symbol (y up), so it is above the origin on the sheet (y down) too, which means a
    // smaller sheet y.
    let d = common::design(
        "One",
        &[("R1", "Resistor", "1k", Some("0402"))],
        &[("A", &["R1.1"]), ("B", &["R1.2"])],
    );
    let l = lay(&d);
    let r = &l.parts[0];
    let (p1, p2) = (pin_at(&l, &d, "R1", "1"), pin_at(&l, &d, "R1", "2"));
    assert!(
        near(Pt::new(p1.x, p1.y), Pt::new(r.at.x, r.at.y - 3.81)),
        "{p1:?} {:?}",
        r.at
    );
    assert!(near(Pt::new(p2.x, p2.y), Pt::new(r.at.x, r.at.y + 3.81)));
}

#[test]
fn every_pin_and_origin_is_on_the_grid() {
    for d in common::all() {
        let l = lay(&d);
        for p in &l.parts {
            for pin in &p.pins {
                for v in [pin.at.x, pin.at.y] {
                    let steps = v / 1.27;
                    assert!(
                        (steps - steps.round()).abs() < 1e-6,
                        "{}: {:?}",
                        d.name,
                        pin
                    );
                }
            }
        }
    }
}

#[test]
fn wires_are_horizontal_or_vertical() {
    for d in common::all() {
        for w in lay(&d).wires {
            for s in w.points.windows(2) {
                assert!(
                    (s[0].x - s[1].x).abs() < 1e-6 || (s[0].y - s[1].y).abs() < 1e-6,
                    "{}: slanted wire {:?}",
                    d.name,
                    w
                );
            }
        }
    }
}

#[test]
fn every_pin_of_every_net_is_the_end_of_a_wire_of_that_net() {
    for d in common::all() {
        let l = lay(&d);
        for (n, net) in d.nets.iter().enumerate() {
            for pin in &net.pins {
                let at = pin_at(&l, &d, &pin.refdes, &pin.pin);
                let reached = l.wires.iter().any(|w| {
                    w.net == n && (near(w.points[0], at) || near(*w.points.last().unwrap(), at))
                });
                assert!(
                    reached,
                    "{}: {}.{} on {} has no wire",
                    d.name, pin.refdes, pin.pin, net.name
                );
            }
        }
    }
}

#[test]
fn parts_do_not_overlap() {
    for d in common::all() {
        let l = lay(&d);
        for (i, a) in l.parts.iter().enumerate() {
            for b in &l.parts[i + 1..] {
                let apart = a.extent.max.x <= b.extent.min.x
                    || b.extent.max.x <= a.extent.min.x
                    || a.extent.max.y <= b.extent.min.y
                    || b.extent.max.y <= a.extent.min.y;
                assert!(
                    apart,
                    "{}: {:?} overlaps {:?}",
                    d.name, a.reference.text, b.reference.text
                );
            }
        }
    }
}

#[test]
fn everything_is_inside_the_bounds() {
    for d in common::all() {
        let l = lay(&d);
        for p in &l.parts {
            assert!(p.extent.min.x >= l.bounds.min.x && p.extent.max.x <= l.bounds.max.x);
            assert!(p.extent.min.y >= l.bounds.min.y && p.extent.max.y <= l.bounds.max.y);
        }
        for w in &l.wires {
            for p in &w.points {
                assert!(p.x >= l.bounds.min.x && p.x <= l.bounds.max.x);
                assert!(p.y >= l.bounds.min.y && p.y <= l.bounds.max.y);
            }
        }
    }
}

#[test]
fn on_a_small_board_every_net_is_wired_and_none_is_named() {
    for d in common::all() {
        let l = lay(&d);
        assert!(
            l.labels.is_empty(),
            "{}: a small board has short wires, no names",
            d.name
        );
        for (n, net) in d.nets.iter().enumerate() {
            if net.pins.len() < 2 {
                continue;
            }
            let at: Vec<Pt> = net
                .pins
                .iter()
                .map(|p| pin_at(&l, &d, &p.refdes, &p.pin))
                .collect();
            assert!(
                common::wired_together(&l, n, &at),
                "{}: the pins of {} are not on one wire",
                d.name,
                net.name
            );
        }
    }
}

#[test]
fn series_parts_stack_in_one_column_with_the_higher_net_on_top() {
    let d = common::led();
    let l = lay(&d);
    let (r1, d1) = (pin_at(&l, &d, "R1", "2"), pin_at(&l, &d, "D1", "2"));
    // R1's second pin and the LED's anode are one net, joined by a straight vertical wire.
    assert!((r1.x - d1.x).abs() < 1e-6);
    assert!(
        r1.y < d1.y,
        "the resistor, on the supply side, is above the LED"
    );
    let (r1_top, led_cathode) = (pin_at(&l, &d, "R1", "1"), pin_at(&l, &d, "D1", "1"));
    assert!(r1_top.y < led_cathode.y);
    let joined = l.wires.iter().any(|w| {
        w.net == 2 && w.points.len() == 2 && near(w.points[0], r1) && near(w.points[1], d1)
            || w.net == 2 && w.points.len() == 2 && near(w.points[1], r1) && near(w.points[0], d1)
    });
    assert!(joined, "one straight wire joins them");
}

#[test]
fn the_signal_runs_left_to_right_through_the_regulator() {
    let d = common::regulator();
    let l = lay(&d);
    let x = |r: &str| part(&l, &d, r).at.x;
    assert!(x("J1") < x("C1") && x("C1") < x("U1") && x("U1") < x("C2") && x("C2") < x("J2"));
}

#[test]
fn a_ground_symbol_is_below_its_pins_and_a_supply_symbol_above() {
    use netsmith_view::layout::{PowerKind, Rails, layout_rails};
    let d = common::regulator();
    let mut rails = Rails::default();
    rails.ground.insert("GND".into());
    rails.positive.insert("VIN".into());
    rails.positive.insert("3V3".into());
    let l = layout_rails(
        &d,
        &rails,
        &Mapping::standard(),
        &SymbolLib::at(netsmith_testkit::sample_symbols()),
    )
    .unwrap();
    assert!(!l.symbols.is_empty());
    // A net may have several symbols (a pin that has one of its own, a rail's end), each pointing
    // away from the parts: down for a ground, up for a supply. The outermost of a net's symbols
    // is beyond all its pins.
    for s in &l.symbols {
        let want = match s.kind {
            PowerKind::Ground => Dir::Down,
            PowerKind::Supply => Dir::Up,
        };
        assert_eq!(s.dir, want, "{s:?}");
    }
    for net in 0..d.nets.len() {
        let symbols: Vec<_> = l.symbols.iter().filter(|s| s.net == net).collect();
        let Some(first) = symbols.first() else {
            continue;
        };
        let pins: Vec<Pt> = d.nets[net]
            .pins
            .iter()
            .map(|p| pin_at(&l, &d, &p.refdes, &p.pin))
            .collect();
        match first.kind {
            PowerKind::Ground => {
                let lowest = symbols.iter().map(|s| s.at.y).fold(f64::MIN, f64::max);
                assert!(pins.iter().all(|p| p.y <= lowest + 1e-6), "{symbols:?}");
            }
            PowerKind::Supply => {
                let highest = symbols.iter().map(|s| s.at.y).fold(f64::MAX, f64::min);
                assert!(pins.iter().all(|p| p.y >= highest - 1e-6), "{symbols:?}");
            }
        }
    }
}

#[test]
fn a_wire_that_meets_the_middle_of_another_gets_a_junction() {
    let d = common::regulator();
    let l = lay(&d);
    let gnd = d.nets.iter().position(|n| n.name == "GND").unwrap();
    assert!(l.junctions.iter().any(|j| j.net == gnd));
    // Two pins joined end to end need none.
    let led = common::led();
    assert!(
        lay(&led)
            .junctions
            .iter()
            .all(|j| led.nets[j.net].name != "net-led.anode")
    );
}

#[test]
fn an_input_pin_is_pointed_toward_the_signal_source() {
    use netsmith_view::geom::Dir;
    let d = common::regulator();
    let l = lay(&d);
    let u1 = part(&l, &d, "U1");
    let escape = |pad: &str| u1.pins.iter().find(|p| p.id == pad).unwrap().escape;
    assert_eq!(escape("3"), Dir::Left, "VI faces the input side");
    assert_eq!(escape("2"), Dir::Right, "VO faces the output side");
    assert_eq!(escape("1"), Dir::Down, "ground faces down");
}

#[test]
fn a_part_with_no_symbol_is_a_box_with_its_pins() {
    let d = common::design(
        "Odd",
        &[
            ("X1", "Gadget", "42", None),
            ("R1", "Resistor", "1k", Some("0402")),
        ],
        &[("A", &["X1.1", "R1.1"]), ("B", &["X1.2", "X1.10", "R1.2"])],
    );
    let l = lay(&d);
    let x = &l.parts[0];
    assert!(x.glyph.symbol.is_none());
    assert_eq!(
        x.pins.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["1", "2", "10"]
    );
    assert!(x.glyph.missing.as_deref().unwrap().contains("Gadget"));
}

#[test]
fn a_net_with_many_pins_is_wired_all_the_same() {
    let mut parts = vec![];
    let names: Vec<String> = (1..=10).map(|i| format!("R{i}")).collect();
    for n in &names {
        parts.push((n.as_str(), "Resistor", "1k", Some("0402")));
    }
    let pins1: Vec<String> = names.iter().map(|n| format!("{n}.1")).collect();
    let pins2: Vec<String> = names.iter().map(|n| format!("{n}.2")).collect();
    let (p1, p2): (Vec<&str>, Vec<&str>) = (
        pins1.iter().map(String::as_str).collect(),
        pins2.iter().map(String::as_str).collect(),
    );
    let d = common::design("Big", &parts, &[("BUS", &p1), ("GND", &p2)]);
    let l = lay(&d);
    let bus = d.nets.iter().position(|n| n.name == "BUS").unwrap();
    assert!(
        l.labels.is_empty(),
        "ten parts on one net are wired, not named"
    );
    let at: Vec<Pt> = pins1
        .iter()
        .map(|p| {
            let (r, pad) = p.split_once('.').unwrap();
            pin_at(&l, &d, r, pad)
        })
        .collect();
    assert!(common::wired_together(&l, bus, &at));
}

#[test]
fn a_design_with_no_parts_still_lays_out() {
    let d = common::design("Empty", &[], &[]);
    let l = lay(&d);
    assert!(l.parts.is_empty() && l.wires.is_empty());
}

#[test]
fn a_broken_design_is_an_error() {
    let d = common::design(
        "Bad",
        &[("R1", "Resistor", "1k", Some("0402"))],
        &[("A", &["R9.1"])],
    );
    assert!(
        layout(
            &d,
            &Mapping::standard(),
            &SymbolLib::at(netsmith_testkit::sample_symbols())
        )
        .is_err()
    );
}
