//! Two nets the design states are a diff pair with no impedance (a `std::sources::DiffPair`, or
//! the lines of a `std::bus::Can`, issue #1193): their names say nothing, and the router routes
//! them as a pair, side by side, with their lengths matched by a meander and their vias alike.

use netsmith_core::NetPair;
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
