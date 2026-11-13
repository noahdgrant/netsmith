//! How far the router gets on boards that are hard to route.

use crate::common;

use netsmith_pcb::{Board, Pt, Ratsnest, Rect};
use netsmith_place_route::{Costs, RouteOptions, SnapshotStage, Stage, route, route_with};

use common::{edge_gap, hard_boards};

const FIRST_ONLY: RouteOptions = RouteOptions {
    retry_orders: false,
    rip_up: false,
    negotiate: false,
    time_limit: None,
    progress: None,
    cancel: None,
    costs: Costs::DEFAULT,
    snapshots: false,
    most_tries: None,
    plan: netsmith_place_route::CoarsePlan::Off,
    plan_cost: None,
    classes: netsmith_place_route::LineClasses::FromSecondTry,
    teardrops: true,
    arcs: netsmith_place_route::arcs::ARC_WIDTHS,
};
const RETRY_ONLY: RouteOptions = RouteOptions {
    retry_orders: true,
    rip_up: false,
    negotiate: false,
    time_limit: None,
    progress: None,
    cancel: None,
    costs: Costs::DEFAULT,
    snapshots: false,
    most_tries: None,
    plan: netsmith_place_route::CoarsePlan::Off,
    plan_cost: None,
    classes: netsmith_place_route::LineClasses::FromSecondTry,
    teardrops: true,
    arcs: netsmith_place_route::arcs::ARC_WIDTHS,
};
const RIP_UP_ONLY: RouteOptions = RouteOptions {
    retry_orders: false,
    rip_up: true,
    negotiate: false,
    time_limit: None,
    progress: None,
    cancel: None,
    costs: Costs::DEFAULT,
    snapshots: false,
    most_tries: None,
    plan: netsmith_place_route::CoarsePlan::Off,
    plan_cost: None,
    classes: netsmith_place_route::LineClasses::FromSecondTry,
    teardrops: true,
    arcs: netsmith_place_route::arcs::ARC_WIDTHS,
};

const NEGOTIATE_ONLY: RouteOptions = RouteOptions {
    retry_orders: false,
    rip_up: true,
    negotiate: true,
    time_limit: None,
    progress: None,
    cancel: None,
    costs: Costs::DEFAULT,
    snapshots: false,
    most_tries: None,
    plan: netsmith_place_route::CoarsePlan::Off,
    plan_cost: None,
    classes: netsmith_place_route::LineClasses::FromSecondTry,
    teardrops: true,
    arcs: netsmith_place_route::arcs::ARC_WIDTHS,
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
    let boards = hard_boards(45);
    // Board 44 needs rip-up and board 8 needs either. (Boards 11 and 32 needed rip-up, and
    // 32 needed both, until the first pass searched in eight directions, issue #415: it routes
    // them alone now, and no board of the first 300 needs both.)
    assert_eq!(left(&boards[44], FIRST_ONLY), 1);
    assert_eq!(left(&boards[44], RETRY_ONLY), 1);
    assert_eq!(left(&boards[44], RIP_UP_ONLY), 0);
    assert_eq!(left(&boards[8], FIRST_ONLY), 1);
    assert_eq!(
        left(&boards[8], RETRY_ONLY) + left(&boards[8], RIP_UP_ONLY),
        0
    );
    for n in [44, 8] {
        assert_eq!(left(&boards[n], RouteOptions::default()), 0, "board {n}");
    }
}

#[test]
fn negotiating_for_the_lines_left_resolves_nets_that_block_each_other() {
    let boards = hard_boards(40);
    // Every hard board routes completely in the first order with negotiation, and the copper
    // keeps the rules. (Board 32 was one that ripping up alone stranded until the first pass
    // searched in eight directions, issue #415; a board that ripping up strands and negotiation
    // finishes is `mixed::lines_ripped_up_for_lines_ripped_up_finish_a_board...`, seed 7.)
    for (n, board) in boards.iter().enumerate() {
        let mut board = board.clone();
        let routing = route_with(&mut board, NEGOTIATE_ONLY);
        assert!(
            routing.left.is_empty(),
            "board {n}: {:?}",
            routing.left.lines
        );
        assert_eq!(routing.tries, 1);
        assert!(Ratsnest::of(&board).is_empty());
        assert!(edge_gap(&board) >= board.rules.copper_to_edge - 1e-9);
    }
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

/// Three resistors in a row on an outline too thin to pass between them and the edge, and too
/// thin for a via. The first and last are on one net; the middle one's pad is in the way.
fn stranded_board() -> Board {
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
    board
}

#[test]
fn a_board_that_cannot_be_routed_is_returned_with_what_is_left() {
    let mut board = stranded_board();
    let rest = route(&mut board);
    assert_eq!(rest.len(), 1, "{:?}", rest.lines);
    assert_eq!(rest.lines[0].net, "A");
}

#[test]
fn each_line_left_says_the_stage_it_failed_in_and_the_nets_in_its_way() {
    let mut board = stranded_board();
    let routing = route_with(&mut board, RouteOptions::default());
    assert_eq!(routing.why.len(), 1, "{:?}", routing.why);
    let why = &routing.why[0];
    assert_eq!(why.line.net, "A");
    // Every stage tried it; the last is the chain of rip-ups. The pads and the edge alone wall
    // it in, so no net's copper is in its way.
    assert_eq!(why.stage, Stage::Chain);
    assert!(why.blockers.is_empty(), "{why:?}");
    let json = serde_json::to_value(&routing.why).unwrap();
    assert_eq!(json[0]["stage"], "chain");
    assert_eq!(json[0]["line"]["from"]["refdes"], "R1");
    // Hard board 44 with only the first pass leaves a line that a rip-up routes: another net's
    // copper is in its way, and it failed in the first pass.
    let mut board = hard_boards(45).remove(44);
    let first = route_with(&mut board, FIRST_ONLY);
    assert_eq!(first.why.len(), 1, "{:?}", first.why);
    assert_eq!(first.why[0].stage, Stage::FirstPass);
    assert!(!first.why[0].blockers.is_empty(), "{:?}", first.why);
    assert!(
        first.why[0]
            .blockers
            .iter()
            .all(|b| b != &first.why[0].line.net && board.nets.iter().any(|n| &n.name == b)),
        "{:?}",
        first.why
    );
    // Out of time from the start: every line is out of time, and nothing is searched for.
    let none = route_with(
        &mut stranded_board(),
        RouteOptions {
            time_limit: Some(std::time::Duration::ZERO),
            ..RouteOptions::default()
        },
    );
    assert!(!none.why.is_empty());
    assert!(none.why.iter().all(|w| w.stage == Stage::OutOfTime));
    // A board that routes completely leaves nothing to explain.
    let mut done = hard_boards(1).remove(0);
    assert!(
        route_with(&mut done, RouteOptions::default())
            .why
            .is_empty()
    );
}

#[test]
fn the_crowding_map_counts_where_soft_paths_went_and_covers_the_board() {
    let boards = hard_boards(45);
    // Board 44 needs a rip-up, which looks for the nets in a line's way by a soft path.
    let mut board = boards[44].clone();
    let routing = route_with(&mut board, RouteOptions::default());
    let crowding = &routing.crowding;
    assert_eq!(crowding.cell, 1.0);
    assert_eq!(crowding.origin, board.outline.min);
    assert_eq!(crowding.counts.len(), crowding.nx * crowding.ny);
    assert!(crowding.nx as f64 >= board.outline.width());
    assert!(crowding.ny as f64 >= board.outline.height());
    assert!(crowding.counts.iter().any(|&c| c > 0), "{crowding:?}");
    // The first pass alone searches for no nets in the way.
    let first = route_with(&mut boards[44].clone(), FIRST_ONLY);
    assert!(first.crowding.counts.iter().all(|&c| c == 0));
}

#[test]
fn snapshots_keep_each_stage_of_the_try_that_won_and_change_no_copper() {
    let boards = hard_boards(45);
    let (mut plain, mut kept) = (boards[44].clone(), boards[44].clone());
    let without = route_with(&mut plain, RouteOptions::default());
    assert!(without.stages.is_empty());
    let with = route_with(
        &mut kept,
        RouteOptions {
            snapshots: true,
            ..RouteOptions::default()
        },
    );
    assert_eq!(plain, kept, "snapshots changed the copper");
    let stages: Vec<SnapshotStage> = with.stages.iter().map(|s| s.after).collect();
    assert_eq!(
        stages,
        [
            SnapshotStage::FirstPass,
            SnapshotStage::RipUp,
            SnapshotStage::Final
        ]
    );
    // The first pass left a line (the board needs a rip-up), the end none, and the end's copper
    // is the board's.
    assert_eq!(with.stages[0].left.len(), 1);
    assert!(with.stages[2].left.is_empty());
    assert_eq!(with.stages[2].tracks, kept.tracks);
}

#[test]
fn the_default_costs_are_the_routers_own_and_another_turn_cost_is_used() {
    assert_eq!(Costs::default(), Costs::DEFAULT);
    assert_eq!(Costs::DEFAULT.turn, 25);
    let board = hard_boards(1).remove(0);
    let (mut a, mut b, mut c) = (board.clone(), board.clone(), board);
    route(&mut a);
    route_with(
        &mut b,
        RouteOptions {
            costs: Costs { turn: 25 },
            ..RouteOptions::default()
        },
    );
    assert_eq!(a, b);
    // Bends dearer than a pair of vias: the board is routed with other copper.
    route_with(
        &mut c,
        RouteOptions {
            costs: Costs { turn: 1000 },
            ..RouteOptions::default()
        },
    );
    assert_ne!(a.tracks, c.tracks);
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

#[test]
fn most_tries_stops_the_router_after_that_many_orders() {
    // Without rip-up the hard boards need more than one order; find one the router tries again.
    let board = hard_boards(45)
        .into_iter()
        .find(|b| route_with(&mut b.clone(), RETRY_ONLY).tries > 1)
        .expect("a hard board that ripping up alone does not finish in one order");
    let once = |most| {
        route_with(
            &mut board.clone(),
            RouteOptions {
                most_tries: Some(most),
                ..RETRY_ONLY
            },
        )
        .tries
    };
    assert_eq!(once(1), 1);
    // None is at least one.
    assert_eq!(once(0), 1);
    assert!(once(2) <= 2);
}

/// The share of the track of `net` within 0.5 mm of `other`'s on the same layer, centre line to
/// centre line, as the route scorecard counts a pair coupled.
fn coupled(board: &Board, net: &str, other: &str) -> f64 {
    let theirs: Vec<&netsmith_pcb::Track> =
        board.tracks.iter().filter(|t| t.net == other).collect();
    let (mut length, mut near) = (0.0, 0.0);
    for t in board.tracks.iter().filter(|t| t.net == net) {
        let l = t.from.distance(t.to);
        length += l;
        let n = (l / 0.05).ceil().max(1.0) as usize;
        for k in 0..n {
            let f = (k as f64 + 0.5) / n as f64;
            let p = Pt::new(
                t.from.x + (t.to.x - t.from.x) * f,
                t.from.y + (t.to.y - t.from.y) * f,
            );
            let to_line = |a: Pt, b: Pt| {
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let len2 = dx * dx + dy * dy;
                let u = if len2 == 0.0 {
                    0.0
                } else {
                    (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
                };
                p.distance(Pt::new(a.x + u * dx, a.y + u * dy))
            };
            if theirs
                .iter()
                .any(|o| o.layer == t.layer && to_line(o.from, o.to) <= 0.5 + 1e-9)
            {
                near += l / n as f64;
            }
        }
    }
    if length > 0.0 { near / length } else { 0.0 }
}

#[test]
fn two_nets_named_as_a_diff_pair_route_as_before_and_are_no_less_coupled() {
    // The same board with two of its nets named as a pair (`.p` and `.n`) and not: only the
    // router's pass for pairs differs, which never leaves a line and keeps new copper only
    // where more of it runs coupled.
    for (n, board) in hard_boards(12).into_iter().enumerate() {
        let mut plain = board.clone();
        for net in &mut plain.nets {
            net.name = format!("{}.a", net.name);
        }
        let mut paired = board.clone();
        paired.nets[0].name = "P.p".into();
        paired.nets[1].name = "P.n".into();
        let plain_left = route(&mut plain).len();
        let mut again = paired.clone();
        assert_eq!(route(&mut paired).len(), plain_left, "board {n}");
        route(&mut again);
        assert_eq!(paired, again, "board {n} is routed the same each time");
        let before = coupled(&plain, "N0.a", "N1.a") + coupled(&plain, "N1.a", "N0.a");
        let after = coupled(&paired, "P.p", "P.n") + coupled(&paired, "P.n", "P.p");
        assert!(
            after >= before - 1e-9,
            "board {n}: coupled {before} -> {after}"
        );
    }
}
