//! Nets drawn as a name at the end of a stub (`netsmith_view::layout`, the label policy): the
//! names join by name in `check`, a label on nothing is floating, the parts that serve a pin
//! stay wired to it, a sheet with many parts is a page and not a strip, and the same design
//! gives the same drawing.

use crate::common;

use common::corpus::corpus;
use common::rowcol::setup;
use netsmith_core::Design;
use netsmith_view::check::{Violation, check};
use netsmith_view::geom::{Dir, Pt};
use netsmith_view::layout::{Anchor, Layout, NetLabel, Text, Wire};
use netsmith_view::layout_sheets;

/// `J1` (a row of `n` pins) and `U1` (`n` pins down its left) with pin `k` of one on a net with
/// pin `n + 1 - k` of the other, so that every wire between them crosses every other; a pull-up
/// `R1` on the net of `U1`'s first pin, and a capacitor `C1` on a net of `U1`'s last pin only.
fn crossing(n: u32) -> (Design, Vec<netsmith_view::Group>) {
    let mut nets: Vec<(String, Vec<String>)> = (1..=n)
        .map(|k| {
            (
                format!("N{k}"),
                vec![format!("J1.{k}"), format!("U1.{}", n + 1 - k)],
            )
        })
        .collect();
    nets[n as usize - 1].1.push("R1.1".into());
    nets.push(("DEC".into(), vec!["U1.9".into(), "C1.1".into()]));
    let refs: Vec<(&str, Vec<&str>)> = nets
        .iter()
        .map(|(name, pins)| (name.as_str(), pins.iter().map(String::as_str).collect()))
        .collect();
    let borrowed: Vec<(&str, &[&str])> = refs.iter().map(|(n, p)| (*n, p.as_slice())).collect();
    let parts = [
        ("J1", "Row", "Row", Some("P")),
        ("U1", "Col", "Col", Some("P")),
        ("R1", "Resistor", "10k", Some("0402")),
        ("C1", "Capacitor", "100n", Some("0402")),
    ];
    let design = common::design("Crossing", &parts, &borrowed);
    (design, Vec::new())
}

fn drawn(n: u32) -> (Design, Layout, netsmith_kicad::testlib::TempDir) {
    let (dir, mapping, lib) = setup(n.max(9));
    let (design, groups) = crossing(n);
    let l = layout_sheets(&design, &groups, &mapping, &lib).unwrap();
    (design, l, dir)
}

fn net(d: &Design, name: &str) -> usize {
    d.nets.iter().position(|n| n.name == name).unwrap()
}

fn pin_at(l: &Layout, d: &Design, refdes: &str, pad: &str) -> Pt {
    let i = d.parts.iter().position(|p| p.refdes == refdes).unwrap();
    l.parts[i].pins.iter().find(|p| p.id == pad).unwrap().at
}

fn kinds<'a>(found: &'a [Violation], kinds: &[&str]) -> Vec<&'a Violation> {
    found.iter().filter(|v| kinds.contains(&v.kind())).collect()
}

fn label(net: usize, at: Pt) -> NetLabel {
    NetLabel {
        net,
        text: Text::new("x", Pt::new(at.x + 0.6, at.y), Anchor::Start),
        at,
        dir: Dir::Right,
    }
}

#[test]
fn nets_that_would_cross_are_named_and_the_drawing_is_still_the_netlist() {
    let (d, l, _dir) = drawn(8);
    assert!(
        !l.labels.is_empty(),
        "eight nets that all cross each other are not eight wires"
    );
    let found = check(&d, &l);
    assert!(found.is_empty(), "{found:?}");
    // A net that is named has a name at each of its two parts, and each is on a stub: a wire
    // of the net ends where the name begins.
    for n in 1..=8 {
        let name = format!("N{n}");
        let ni = net(&d, &name);
        let at: Vec<&NetLabel> = l.labels.iter().filter(|t| t.net == ni).collect();
        if at.is_empty() {
            continue;
        }
        assert!(at.len() >= 2, "{name} is named at one place only");
        for t in at {
            assert!(
                l.wires
                    .iter()
                    .any(|w: &Wire| w.net == ni && w.points.iter().any(|p| near(*p, t.at))),
                "the name of {name} is not at the end of a wire"
            );
        }
    }
}

fn near(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

#[test]
fn a_part_that_serves_a_pin_stays_wired_to_it_and_its_net_is_never_named() {
    let (d, l, _dir) = drawn(8);
    // The capacitor on `DEC` is wired to the pin it serves, and `DEC` has no label.
    let dec = net(&d, "DEC");
    assert!(l.labels.iter().all(|t| t.net != dec), "DEC is named");
    let (c1, u1) = (pin_at(&l, &d, "C1", "1"), pin_at(&l, &d, "U1", "9"));
    assert!(common::wired_together(&l, dec, &[c1, u1]));
    // The pull-up on the net `N1` is wired to the pin of the part it stands beside, though the
    // net itself is named where it goes to the other part.
    let n1 = net(&d, "N8");
    let (r1, u1) = (pin_at(&l, &d, "R1", "1"), pin_at(&l, &d, "U1", "1"));
    assert!(
        common::wired_together(&l, n1, &[r1, u1]),
        "the pull-up is not wired to its pin"
    );
}

#[test]
fn labels_of_one_name_join_the_pieces_of_a_net() {
    let (d, mut l, _dir) = drawn(8);
    // Cut the wire of one net in two, as a drawing that named it would have, and put a name
    // on each piece: no open. Without a name on one piece it is open.
    let named = l.labels.iter().map(|t| t.net).next().unwrap();
    let kept: Vec<Wire> = l.wires.iter().filter(|w| w.net == named).cloned().collect();
    assert!(kept.len() >= 2, "the stubs of {named} are separate wires");
    let at: Vec<Pt> = l
        .labels
        .iter()
        .filter(|t| t.net == named)
        .map(|t| t.at)
        .collect();
    assert!(check(&d, &l).is_empty());
    // Take the labels of the net away: its two stubs are two pieces with nothing to join them.
    let mut bare = l.clone();
    bare.labels.retain(|t| t.net != named);
    let opens = kinds(&check(&d, &bare), &["open"]).len();
    assert_eq!(opens, 1, "a net in two pieces with no name is open");
    // One label back: still open, the other piece has none.
    let mut one = bare.clone();
    one.labels.push(label(named, at[0]));
    assert_eq!(kinds(&check(&d, &one), &["open"]).len(), 1);
    // Both back: the same name on both pieces joins them.
    one.labels.push(label(named, at[1]));
    assert!(kinds(&check(&d, &one), &["open", "short"]).is_empty());
    l.labels.clear();
}

#[test]
fn a_label_that_touches_no_wire_and_no_pin_is_floating() {
    let (d, mut l, _dir) = drawn(8);
    let n = net(&d, "N1");
    l.labels.push(label(n, Pt::new(-300.0, -300.0)));
    let found = check(&d, &l);
    let floating = kinds(&found, &["floating-label"]);
    assert_eq!(floating.len(), 1, "{found:?}");
    assert!(floating[0].to_string().contains("N1"), "{}", floating[0]);
}

#[test]
fn a_sheet_with_many_parts_is_a_page_and_not_a_strip() {
    // Thirty small connectors in a chain, each joined to the next: in one row they would be a
    // strip a metre wide.
    let n = 30;
    let parts: Vec<(String, &str, &str, Option<&str>)> = (1..=n)
        .map(|k| (format!("J{k}"), "Header1x2", "Conn_01x02", None))
        .collect();
    let nets: Vec<(String, Vec<String>)> = (1..n)
        .map(|k| {
            (
                format!("L{k}"),
                vec![format!("J{k}.1"), format!("J{}.2", k + 1)],
            )
        })
        .collect();
    let part_refs: Vec<(&str, &str, &str, Option<&str>)> = parts
        .iter()
        .map(|(r, k, v, p)| (r.as_str(), *k, *v, *p))
        .collect();
    let net_refs: Vec<(&str, Vec<&str>)> = nets
        .iter()
        .map(|(name, pins)| (name.as_str(), pins.iter().map(String::as_str).collect()))
        .collect();
    let borrowed: Vec<(&str, &[&str])> = net_refs.iter().map(|(n, p)| (*n, p.as_slice())).collect();
    let d = common::design("Chain", &part_refs, &borrowed);
    let l = netsmith_view::layout(
        &d,
        &netsmith_kicad::Mapping::standard(),
        &netsmith_kicad::SymbolLib::at(netsmith_kicad::testlib::sample_symbols()),
    )
    .unwrap();
    let aspect = l.bounds.width() / l.bounds.height();
    assert!(
        (0.8..=2.4).contains(&aspect),
        "{:.0} x {:.0} mm is {aspect:.1}:1",
        l.bounds.width(),
        l.bounds.height()
    );
    let found = check(&d, &l);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_small_sheet_is_not_cut_into_rows() {
    // The regulator example has five parts: its two connectors stand level, in one row, and
    // nothing in it is named.
    let case = corpus().remove(2);
    assert_eq!(case.name, "RegulatorExample");
    let l = case.drawn_plain();
    let at = |r: &str| {
        let i = case
            .design
            .parts
            .iter()
            .position(|p| p.refdes == r)
            .unwrap();
        l.parts[i].pins[0].at.y
    };
    assert!(
        (at("J1") - at("J2")).abs() < 12.0,
        "J1 and J2 are in different rows"
    );
    assert!(l.labels.is_empty(), "a small board is wired");
}

#[test]
fn the_same_design_gives_the_same_drawing_with_its_names() {
    let (d, a, _dir) = drawn(8);
    let (_, b, _dir2) = drawn(8);
    assert_eq!(a, b);
    assert!(!a.labels.is_empty());
    let svg = |l: &Layout| format!("{:?}", (&l.labels, &l.ports, &l.wires, &d.name));
    assert_eq!(svg(&a), svg(&b));
}
