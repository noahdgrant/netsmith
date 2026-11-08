//! How far the router gets on boards that are hard to route.

use crate::common;

use netsmith_pcb::{Board, Pt, Ratsnest, Rect, RouteOptions, route, route_with};

use common::{edge_gap, hard_boards};

const FIRST_ONLY: RouteOptions = RouteOptions {
    retry_orders: false,
    rip_up: false,
    negotiate: false,
    time_limit: None,
    progress: None,
    cancel: None,
};
const RETRY_ONLY: RouteOptions = RouteOptions {
    retry_orders: true,
    rip_up: false,
    negotiate: false,
    time_limit: None,
    progress: None,
    cancel: None,
};
const RIP_UP_ONLY: RouteOptions = RouteOptions {
    retry_orders: false,
    rip_up: true,
    negotiate: false,
    time_limit: None,
    progress: None,
    cancel: None,
};

const NEGOTIATE_ONLY: RouteOptions = RouteOptions {
    retry_orders: false,
    rip_up: true,
    negotiate: true,
    time_limit: None,
    progress: None,
    cancel: None,
};

fn left(board: &Board, options: RouteOptions) -> usize {
    route_with(&mut board.clone(), options).left.len()
}

#[test]
fn every_hard_board_is_placed_validly() {
    for (n, board) in hard_boards(40).iter().enumerate() {
        assert_eq!(board.check_placement(), vec![], "board {n}");
        assert_eq!(Ratsnest::of(board).len(), 12, "board {n}: 4 nets of 4 pads");
    }
}

#[test]
fn the_first_pass_alone_strands_lines_that_the_full_router_routes() {
    let boards = hard_boards(40);
    // Board 11 needs rip-up, board 8 needs either, and board 32 needs both.
    assert_eq!(left(&boards[11], FIRST_ONLY), 1);
    assert_eq!(left(&boards[11], RETRY_ONLY), 1);
    assert_eq!(left(&boards[11], RIP_UP_ONLY), 0);
    assert_eq!(left(&boards[8], FIRST_ONLY), 1);
    assert_eq!(
        left(&boards[8], RETRY_ONLY) + left(&boards[8], RIP_UP_ONLY),
        0
    );
    assert_eq!(left(&boards[32], FIRST_ONLY), 1);
    assert_eq!(left(&boards[32], RETRY_ONLY), 1);
    assert_eq!(left(&boards[32], RIP_UP_ONLY), 1);
    for n in [11, 8, 32] {
        assert_eq!(left(&boards[n], RouteOptions::default()), 0, "board {n}");
    }
}

#[test]
fn negotiating_for_the_lines_left_resolves_nets_that_block_each_other() {
    let boards = hard_boards(40);
    // Ripping up the nets in the way of the last line, one try, strands it; negotiating
    // for it, with the cells the lines fought over made dearer, routes everything, in the
    // first order, and the copper keeps the rules.
    assert_eq!(left(&boards[32], RIP_UP_ONLY), 1);
    let mut board = boards[32].clone();
    let routing = route_with(&mut board, NEGOTIATE_ONLY);
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert_eq!(routing.tries, 1);
    assert!(Ratsnest::of(&board).is_empty());
    assert!(edge_gap(&board) >= board.rules.copper_to_edge - 1e-9);
    // It never makes a board worse than ripping up alone does.
    for (n, board) in boards.iter().enumerate() {
        assert!(
            left(board, NEGOTIATE_ONLY) <= left(board, RIP_UP_ONLY),
            "board {n}"
        );
    }
}

#[test]
fn the_full_router_completes_every_hard_board_and_does_the_same_each_time() {
    let mut first_pass_failures = 0;
    for (n, board) in hard_boards(40).into_iter().enumerate() {
        first_pass_failures += usize::from(left(&board, FIRST_ONLY) > 0);
        let (mut a, mut b) = (board.clone(), board);
        assert!(route(&mut a).is_empty(), "board {n} left lines unrouted");
        route(&mut b);
        assert_eq!(a, b, "board {n}");
    }
    assert!(
        first_pass_failures >= 5,
        "the boards are not hard enough: only {first_pass_failures} fail the first pass"
    );
}

#[test]
fn routed_hard_boards_keep_all_copper_off_the_edge() {
    for (n, mut board) in hard_boards(40).into_iter().enumerate() {
        route(&mut board);
        let gap = edge_gap(&board);
        assert!(
            gap >= board.rules.copper_to_edge - 1e-9,
            "board {n}: copper is {gap:.3} mm from the edge"
        );
    }
}

#[test]
fn a_board_that_cannot_be_routed_is_returned_with_what_is_left() {
    // Three resistors in a row on an outline too thin to pass between them and the edge,
    // and too thin for a via. The first and last are on one net; the middle one's pad is in
    // the way.
    let mut board = hard_boards(1).remove(0);
    board.parts.truncate(3);
    board.nets.clear();
    for (name, pads) in [
        ("A", [("R1", "2"), ("R3", "1")]),
        ("B", [("R2", "1"), ("R2", "2")]),
    ] {
        board.nets.push(netsmith_pcb::BoardNet {
            name: name.into(),
            pads: pads
                .iter()
                .map(|(r, p)| netsmith_pcb::PadRef {
                    refdes: (*r).into(),
                    number: (*p).into(),
                })
                .collect(),
        });
    }
    for (k, p) in board.parts.iter_mut().enumerate() {
        p.at = Pt::new(5.0 + k as f64 * 2.4, 5.0);
        p.rotation = 0.0;
    }
    board.outline = Rect::new(Pt::new(2.0, 4.2), Pt::new(13.0, 5.8));
    let rest = route(&mut board);
    assert_eq!(rest.len(), 1, "{:?}", rest.lines);
    assert_eq!(rest.lines[0].net, "A");
}

#[test]
fn a_cancel_flag_stops_the_router_like_the_time_limit() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static CANCEL: AtomicBool = AtomicBool::new(false);
    let board = hard_boards(1).remove(0);
    // Not set: the router does its work.
    let free = route_with(
        &mut board.clone(),
        RouteOptions {
            cancel: Some(&CANCEL),
            ..RouteOptions::default()
        },
    );
    assert!(!free.stopped);
    // Set before it starts: it stops at once, with what it has, and says so.
    CANCEL.store(true, Ordering::Relaxed);
    let mut stopped_board = board.clone();
    let stopped = route_with(
        &mut stopped_board,
        RouteOptions {
            cancel: Some(&CANCEL),
            ..RouteOptions::default()
        },
    );
    CANCEL.store(false, Ordering::Relaxed);
    assert!(stopped.stopped);
    assert!(stopped.left.len() >= free.left.len());
    assert!(stopped_board.tracks.is_empty());
}
