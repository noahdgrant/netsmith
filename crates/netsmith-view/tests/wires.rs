//! The drawing joins what a net joins: wires between the pins of a net on one sheet, power
//! symbols for rails (decided by the design, not by names), names that do not run into each
//! other, and one box for a symbol of several units. The board is invented
//! (`tests/common/bench.rs`).

mod common;

use common::bench::{Bench, bench};
use netsmith_view::geom::{Pt, Rect};
use netsmith_view::layout::{Layout, PowerKind, Rails, layout_rails};
use netsmith_view::sheets::layout_sheets_rails;
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
fn a_net_between_sheets_is_wired_between_its_pins_on_each() {
    // BATT_DIV has all its pins on one sheet; BATT_ADC has two there and one on another
    // sheet. Both are joined by wires where the pins are, and BATT_ADC says where it goes.
    let b = bench();
    let l = lay(&b);
    for (net, pins) in [
        ("BATT_DIV", vec![("R1", "2"), ("R2", "1"), ("U1", "3")]),
        ("BATT_ADC", vec![("R3", "2"), ("C1", "1")]),
    ] {
        let n = net_index(&b, net);
        let comps = components(&l, n);
        let all: Vec<Pt> = pins.iter().map(|(r, p)| pin_at(&l, &b, r, p)).collect();
        assert!(
            comps.iter().any(|c| all.iter().all(|&p| touches(c, p))),
            "{net}: its pins are not on one wire: {comps:?}"
        );
    }
    let adc = net_index(&b, "BATT_ADC");
    assert!(l.labels.iter().any(|x| x.net == adc), "the net's name");
}

#[test]
fn a_pin_that_leaves_away_from_its_trunk_is_wired_and_not_just_named() {
    // R1 is stacked with the rail above it, so its pin on BATT_DIV points down while the
    // trunk of BATT_DIV is above: its wire goes round the column.
    let b = bench();
    let l = lay(&b);
    let n = net_index(&b, "BATT_DIV");
    let r1 = pin_at(&l, &b, "R1", "2");
    let r2 = pin_at(&l, &b, "R2", "1");
    let comp = components(&l, n)
        .into_iter()
        .find(|c| touches(c, r1))
        .expect("R1's pin is on a wire");
    assert!(touches(&comp, r2), "R1 and R2 share a wire");
    let leaves = l.parts[part_index(&b, "R1")]
        .pins
        .iter()
        .find(|p| p.id == "2")
        .unwrap();
    assert!(leaves.escape.is_vertical());
    // The wire from the pin is longer than a stub with a name.
    let reach = l
        .wires
        .iter()
        .filter(|w| w.net == n && w.points.iter().any(|&p| near(p, r1)))
        .map(|w| w.points.len())
        .max()
        .unwrap();
    assert!(reach >= 4, "{reach}");
}

#[test]
fn every_pin_on_a_rail_has_a_wire_to_a_power_symbol() {
    let b = bench();
    let l = lay(&b);
    for (rail, kind) in [
        ("GND", PowerKind::Ground),
        ("+3V3", PowerKind::Supply),
        ("+3V8", PowerKind::Supply),
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
    // And a net that is none of them has no symbol.
    let adc = net_index(&b, "BATT_ADC");
    assert!(l.symbols.iter().all(|s| s.net != adc));
}

#[test]
fn a_run_of_pins_on_one_rail_shares_a_bus_and_one_symbol() {
    // The patch antenna has nine ground pins in a row: one symbol for them, one for the
    // connector's ground.
    let b = bench();
    let l = lay(&b);
    let gnd = net_index(&b, "GND");
    let sheet = l.sheets.iter().find(|s| s.name == "gnss").unwrap();
    let here: Vec<_> = l
        .symbols
        .iter()
        .filter(|s| s.net == gnd && sheet.frame.min.x <= s.at.x && s.at.x <= sheet.frame.max.x)
        .filter(|s| sheet.frame.min.y <= s.at.y && s.at.y <= sheet.frame.max.y)
        .collect();
    assert_eq!(here.len(), 2, "{here:?}");
    // The microcontroller's 24 grounds are on one side of one part: one symbol.
    let sheet = l.sheets.iter().find(|s| s.name == "mcu").unwrap();
    let mcu: Vec<_> = l
        .symbols
        .iter()
        .filter(|s| s.net == gnd)
        .filter(|s| sheet.frame.min.y <= s.at.y && s.at.y <= sheet.frame.max.y)
        .filter(|s| sheet.frame.min.x <= s.at.x && s.at.x <= sheet.frame.max.x)
        .collect();
    assert_eq!(
        mcu.len(),
        3,
        "the box's grounds and two capacitors: {mcu:?}"
    );
}

#[test]
fn rails_come_from_the_design_and_not_from_names() {
    // `GND` is an ordinary net here and `RET` is the rail's return.
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
fn the_svg_draws_the_symbols_with_their_nets_and_tooltips() {
    let b = bench();
    let svg =
        render_svg_by_module_with(&b.design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap();
    assert!(svg.contains("class=\"power ground\" data-net=\"GND\""));
    assert!(svg.contains("class=\"power supply\" data-net=\"+3V3\""));
    assert!(svg.contains("<title>GND (ground)</title>"));
    assert!(svg.contains("<title>+3V3 (supply)</title>"));
}

#[test]
fn no_two_names_are_written_over_each_other() {
    let b = bench();
    for l in [lay(&b), plain(&b)] {
        for sheet in &l.sheets {
            let inside = |r: &Rect| {
                sheet.frame.min.x <= r.min.x
                    && r.max.x <= sheet.frame.max.x
                    && sheet.frame.min.y <= r.min.y
                    && r.max.y <= sheet.frame.max.y
            };
            let mut names: Vec<(String, Rect)> = l
                .labels
                .iter()
                .map(|x| (x.text.text.clone(), x.text.extent()))
                .chain(
                    l.symbols
                        .iter()
                        .filter_map(|s| s.label.as_ref())
                        .map(|t| (t.text.clone(), t.extent())),
                )
                .filter(|(_, r)| inside(r))
                .collect();
            for p in sheet.parts.iter().map(|&i| &l.parts[i]) {
                names.push((
                    format!("{} reference", b.design.parts[p.part].refdes),
                    p.reference.extent(),
                ));
            }
            for (i, (an, a)) in names.iter().enumerate() {
                for (bn, bx) in &names[i + 1..] {
                    assert!(!overlap(a, bx), "{} : `{an}` over `{bn}`", sheet.name);
                }
            }
        }
        // And no part's pin names are over each other, in the way it is turned.
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
fn a_run_of_pins_on_one_net_that_is_only_named_has_one_name() {
    // With no rails told, the ten pins of ground on the antenna are a net too wide for a
    // trunk: they are named, and the nine in a row share a bar and one name.
    let b = bench();
    let l = plain(&b);
    let gnd = net_index(&b, "GND");
    let sheet = l.sheets.iter().find(|s| s.name == "gnss").unwrap();
    let names = l
        .labels
        .iter()
        .filter(|x| x.net == gnd)
        .filter(|x| sheet.frame.min.x <= x.text.at.x && x.text.at.x <= sheet.frame.max.x)
        .filter(|x| sheet.frame.min.y <= x.text.at.y && x.text.at.y <= sheet.frame.max.y)
        .count();
    assert_eq!(
        names, 2,
        "one for the antenna's nine, one for the connector"
    );
}

#[test]
fn a_name_longer_than_the_limit_is_cut_and_keeps_its_tooltip() {
    let b = bench();
    let long = "a_net_with_a_very_long_name_indeed_over_the_limit";
    let mut design = b.design.clone();
    design.nets.push(netsmith_core::Net {
        name: long.into(),
        pins: vec![netsmith_core::PinRef {
            refdes: "U5".into(),
            pin: "6".into(),
        }],
    });
    let l = layout_sheets_rails(&design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap();
    let n = design.nets.len() - 1;
    let label = l.labels.iter().find(|x| x.net == n).expect("a name");
    assert_eq!(label.text.text.chars().count(), 24);
    assert!(label.text.text.ends_with('…'));
    let svg = render_svg_by_module_with(&design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap();
    assert!(svg.contains(&format!("<title>{long}</title>")));
}

#[test]
fn names_on_pins_side_by_side_are_not_written_over_each_other() {
    // Three pins of the antenna, next to each other, each on a net of its own that goes
    // nowhere here: three long names that would be written over one another.
    let b = bench();
    let mut design = b.design.clone();
    for (i, pad) in ["3", "4", "5"].iter().enumerate() {
        for net in &mut design.nets {
            net.pins.retain(|p| !(p.refdes == "U2" && p.pin == *pad));
        }
        design.nets.push(netsmith_core::Net {
            name: format!("SIGNAL_NUMBER_{i}_OF_THREE"),
            pins: vec![netsmith_core::PinRef {
                refdes: "U2".into(),
                pin: (*pad).into(),
            }],
        });
    }
    let l = layout_sheets_rails(&design, &b.groups, &b.rails, &b.mapping, &b.lib).unwrap();
    let names: Vec<_> = l
        .labels
        .iter()
        .filter(|x| x.text.text.starts_with("SIGNAL_NUMBER"))
        .collect();
    assert_eq!(names.len(), 3);
    for (i, a) in names.iter().enumerate() {
        for c in &names[i + 1..] {
            assert!(!overlap(&a.text.extent(), &c.text.extent()));
        }
    }
}

#[test]
fn a_symbol_of_two_units_and_ninety_seven_pins_is_one_part_in_labelled_groups() {
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
    for heading in [
        "Mcu, 97 pins",
        "Power (6)",
        "Ground (24)",
        "P0 (16)",
        "Not connected (3)",
    ] {
        assert!(texts.contains(&heading), "{heading} in {texts:?}");
    }
    // The pins are in the order of the symbol within a group, one row each, none over another.
    let y = |pad: &str| g.pin(pad).unwrap().at.1;
    assert!(y("17") > y("18") && y("18") > y("19"));
    assert!(!g.texts_collide(u5.orient));
    // The text is large enough to read: the names are as high as the nets' names.
    assert!(g.pin_text >= 1.27);
    // It stays upright: names are written across.
    assert!(u5.orient.dir(g.pin("1").unwrap().toward_body).vector().0 != 0.0);
}

#[test]
fn a_part_with_only_rails_is_drawn_with_wires_to_symbols() {
    let b = bench();
    let l = lay(&b);
    // R6 is between +3V3 and ground and nothing else.
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
fn the_reference_and_value_of_a_part_stay_clear_of_the_wires_to_its_symbols() {
    // The header's pin 1 is on the supply and pin 2 on ground: their wires leave on the side
    // where the texts of the header would go.
    let mut d = common::led();
    let j1 = d.parts.iter_mut().find(|p| p.refdes == "J1").unwrap();
    j1.kind = "Socket1x2".into();
    j1.value = "Conn_01x02_Socket".into();
    let mut rails = Rails::default();
    rails.ground.insert("GND".into());
    rails.positive.insert("VCC".into());
    let lib = netsmith_kicad::SymbolLib::at(netsmith_kicad::testlib::sample_symbols());
    let l = layout_rails(&d, &rails, &netsmith_kicad::Mapping::standard(), &lib).unwrap();
    assert!(!l.symbols.is_empty());
    for p in &l.parts {
        let texts = std::iter::once(&p.reference).chain(p.value.as_ref());
        for t in texts {
            let r = t.extent();
            for w in &l.wires {
                for s in w.points.windows(2) {
                    let mut seg = Rect::around(s[0]);
                    seg.include(s[1]);
                    assert!(
                        !overlap(&r, &seg),
                        "{} `{}` is over a wire of {}",
                        d.parts[p.part].refdes,
                        t.text,
                        d.nets[w.net].name
                    );
                }
            }
            for s in &l.symbols {
                assert!(!overlap(&r, &s.extent()), "`{}` is over a symbol", t.text);
            }
        }
    }
}
