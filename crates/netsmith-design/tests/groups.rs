//! Groups of identical members, `N x Kind` (ADR 0119), in the design: one part for each
//! member, numbered in index order, each in parallel between the ends of its chain.

use netsmith_check::{Loader, check};
use netsmith_core::Design;
use netsmith_design::{design_board, natural_path_order};
use netsmith_solve::{Catalogs, solve};

fn designed(src: &str) -> Design {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    design_board(&c.boards[0], &s).unwrap().design
}

const BOARD: &str = "\
import std::parts::passive::{Capacitor, Resistor}
import std::sources::Rail

board B {
    .v: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }
    .first: Capacitor { .capacitance &= 1uF +/- 10% }
    .v.pos ~> .first ~> .v.neg
    .decoupling: 12 x Capacitor { .capacitance &= 100nF +/- 10%; .package &= \"0402\" }
    .decoupling[3].package &= \"0402\" // the group's body and the member's agree
    .v.pos ~> .decoupling ~> .v.neg
    .v.pos ~> 2 x Capacitor { .capacitance &= 10uF +/- 20% } ~> .v.neg
    .r: Resistor { .resistance &= 10kohm +/- 1% }
    .v.pos ~> .r ~> .v.neg
}
";

#[test]
fn each_member_is_a_part_numbered_in_index_order_by_its_path() {
    let d = designed(BOARD);
    let mut by_number: Vec<(usize, &str)> = d
        .parts
        .iter()
        .filter(|p| p.refdes.starts_with('C'))
        .map(|p| (p.refdes[1..].parse().unwrap(), p.path.as_str()))
        .collect();
    by_number.sort();
    let paths: Vec<&str> = by_number.iter().map(|(_, p)| *p).collect();
    // By path (issue #634), and an index as a number: `decoupling[2]` before `decoupling[10]`.
    let mut want: Vec<String> = (0..2)
        .map(|i| format!("capacitor_v_pos_v_neg[{i}]"))
        .collect();
    want.extend((0..12).map(|i| format!("decoupling[{i}]")));
    want.push("first".to_string());
    assert_eq!(paths, want);
    // The design's parts are in path order, and an index sorts as a number.
    let listed: Vec<&str> = d.parts.iter().map(|p| p.path.as_str()).collect();
    let i2 = listed.iter().position(|p| *p == "decoupling[2]").unwrap();
    let i10 = listed.iter().position(|p| *p == "decoupling[10]").unwrap();
    assert!(i2 < i10, "{listed:?}");
}

#[test]
fn each_member_stands_between_the_ends_of_the_chain() {
    let d = designed(BOARD);
    let net = |name: &str| -> Vec<String> {
        let n = d.nets.iter().find(|n| n.name == name).unwrap();
        n.pins.iter().map(|p| format!("{}.{}", p.refdes, p.pin)).collect()
    };
    let (supply, ground) = (net("3V3"), net("GND"));
    for part in d.parts.iter().filter(|p| p.kind == "Capacitor") {
        assert!(supply.contains(&format!("{}.1", part.refdes)), "{supply:?}");
        assert!(ground.contains(&format!("{}.2", part.refdes)), "{ground:?}");
    }
    assert_eq!(d.nets.len(), 2, "{:?}", d.nets);
}

#[test]
fn one_member_may_be_narrowed_further_than_the_group() {
    let src = BOARD
        .replace(
            "12 x Capacitor { .capacitance &= 100nF +/- 10%; .package &= \"0402\" }",
            "12 x Capacitor { .capacitance &= 100nF +/- 10% }",
        )
        .replace(
            ".decoupling[3].package &= \"0402\"",
            ".decoupling[3].package &= \"0603\"",
        );
    assert_ne!(src, BOARD);
    let d = designed(&src);
    let package = |path: &str| {
        d.parts
            .iter()
            .find(|p| p.path == path)
            .unwrap()
            .package
            .clone()
            .unwrap()
    };
    assert_eq!(package("decoupling[3]"), "0603");
    assert_ne!(package("decoupling[2]"), "0603");
}

#[test]
fn an_index_is_compared_as_a_number() {
    use std::cmp::Ordering::{Greater, Less};
    assert_eq!(natural_path_order("decoupling[2]", "decoupling[10]"), Less);
    assert_eq!(natural_path_order("decoupling[10].a", "decoupling[9].a"), Greater);
    assert_eq!(natural_path_order("decoupling", "decoupling[0]"), Less);
    assert_eq!(natural_path_order("m.caps[1]", "m.caps[01]"), Less);
}
