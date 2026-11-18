//! Two nets the design states are a diff pair with no impedance (a `std::sources::DiffPair`, or
//! the lines of a `std::bus::Can`, issue #1193): their names say nothing, and the router routes
//! them as a pair, side by side, with their lengths matched by a meander and their vias alike.
//! A pair's own `.skew` is the limit it is matched to, and the nets of a group matched in length
//! are lengthened towards its longest (issue #1234).

use netsmith_core::{LengthGroup, NetPair};
use netsmith_pcb::Board;
use netsmith_router::meander::SKEW_TARGET;
use netsmith_router::{RouteOptions, route_with};

use crate::impedance_targets::pair_board;

/// The length of a net's tracks and its vias.
fn length_and_vias(board: &Board, net: &str) -> (f64, usize) {
    let length = board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .map(|t| t.length())
        .sum();
    (length, board.vias.iter().filter(|v| v.net == net).count())
}

/// `A` from R1 to R2 and `B` from R3 to R4, the second two 3 mm below the first and 2 mm
/// further out to each side, so `B` is about 4 mm longer.
fn skewed() -> Board {
    pair_board(3.0, 2.0)
}

#[test]
fn two_plainly_named_nets_stated_a_pair_are_routed_with_their_lengths_matched() {
    let mut board = skewed();
    board.pairs = vec![NetPair {
        path: "pair".into(),
        p: "A".into(),
        n: "B".into(),
        skew: None,
    }];
    assert_eq!(board.diff_pairs(), [("A".to_string(), "B".to_string())]);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert_eq!(routing.pairs.len(), 1, "{:?}", routing.pairs);
    let matched = &routing.pairs[0];
    println!("{matched:?}");
    assert_eq!((matched.p.as_str(), matched.n.as_str()), ("A", "B"));
    assert!(matched.before > 1.0, "{matched:?}");
    assert!(matched.after <= SKEW_TARGET, "{matched:?}");
    let ((a, a_vias), (b, b_vias)) = (length_and_vias(&board, "A"), length_and_vias(&board, "B"));
    assert!((a - b).abs() <= SKEW_TARGET, "A {a} mm, B {b} mm");
    assert_eq!(a_vias, b_vias);
}

#[test]
fn the_same_nets_stated_no_pair_keep_their_skew() {
    let mut board = skewed();
    assert!(board.diff_pairs().is_empty());
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert!(routing.pairs.is_empty(), "{:?}", routing.pairs);
    let ((a, _), (b, _)) = (length_and_vias(&board, "A"), length_and_vias(&board, "B"));
    assert!(b - a > 1.0, "A {a} mm, B {b} mm");
}

/// `A` and `B` of [`skewed`] stated a pair whose skew may be `skew` mm.
fn pair_with_skew(skew: f64) -> Board {
    let mut board = skewed();
    board.pairs = vec![NetPair {
        path: "pair".into(),
        p: "A".into(),
        n: "B".into(),
        skew: Some(skew),
    }];
    board
}

#[test]
fn a_pair_is_matched_to_its_own_skew() {
    let mut board = pair_with_skew(0.03);
    assert_eq!(board.pair_skew("B", "A"), 0.03);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    let matched = &routing.pairs[0];
    println!("{matched:?}");
    assert_eq!(matched.limit, 0.03);
    assert!(matched.before > 1.0, "{matched:?}");
    assert!(matched.after <= 0.03, "{matched:?}");
}

#[test]
fn a_pair_whose_skew_is_within_its_own_limit_gets_no_meander() {
    let mut board = pair_with_skew(20.0);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    let matched = &routing.pairs[0];
    println!("{matched:?}");
    // No meander matches it: it keeps its skew, less what the rounded bends take of it.
    assert!(matched.before > 1.0, "{matched:?}");
    assert!(
        matched.after > 1.0 && (matched.before - matched.after).abs() < 0.2,
        "{matched:?}"
    );
}

#[test]
fn the_shorter_nets_of_a_length_group_are_lengthened_to_the_longest() {
    let mut board = skewed();
    board.length_groups = vec![LengthGroup {
        path: "lanes".into(),
        nets: vec!["A".into(), "B".into()],
        tolerance: 0.2,
    }];
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert!(routing.pairs.is_empty(), "{:?}", routing.pairs);
    assert_eq!(routing.groups.len(), 1, "{:?}", routing.groups);
    let group = &routing.groups[0];
    println!("{group:?}");
    assert_eq!(group.nets, ["A", "B"]);
    assert!(group.before > 1.0, "{group:?}");
    assert!(group.after <= 0.2, "{group:?}");
    let ((a, _), (b, _)) = (length_and_vias(&board, "A"), length_and_vias(&board, "B"));
    assert!((a - b).abs() <= 0.2, "A {a} mm, B {b} mm");
    // `board nets` measures the group as the router does.
    let art = netsmith_geom::Artwork::parse(&netsmith_pcb::kicad_pcb(&board)).unwrap();
    let table = netsmith_fab::NetTable::of(&art, &[], &board.length_groups);
    println!("{}", table.text());
    let row = &table.groups[0];
    assert!(!row.over && (row.spread - group.after).abs() < 1e-3, "{row:?}");
}
