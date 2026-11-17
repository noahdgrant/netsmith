//! The rounds of rip-up and re-route of whole nets (issue #1088) on whole boards, and what the
//! report says of a line walled in. Every check counts lines and work, never time.

use crate::common;

use netsmith_pcb::{Board, Pt, Rect};
use netsmith_router::{RIP_MOST, RouteOptions, Stage, route_with};

use common::hard_boards;

/// One try with the rip-up of the nets in a line's way and nothing after it but the rounds:
/// what the rounds add to the stages before them, alone.
fn rip_up_then(rounds: usize) -> RouteOptions {
    RouteOptions {
        retry_orders: false,
        negotiate: false,
        rip_rounds: rounds,
        ..RouteOptions::default()
    }
}

#[test]
fn a_board_the_stages_before_route_is_untouched_and_the_rounds_never_leave_more() {
    for (n, board) in hard_boards(45).into_iter().enumerate() {
        let (mut without, mut with) = (board.clone(), board);
        let plain = route_with(&mut without, rip_up_then(0));
        let rounds = route_with(&mut with, rip_up_then(netsmith_router::RIP_ROUNDS));
        assert!(
            rounds.left.len() <= plain.left.len(),
            "board {n}: {} left with the rounds, {} without",
            rounds.left.len(),
            plain.left.len()
        );
        // A board the stages before route has the same copper: the rounds do not run.
        if plain.left.is_empty() {
            assert_eq!(without, with, "board {n}");
            assert_eq!(rounds.rip_rounds, Default::default(), "board {n}");
        }
        assert!(
            rounds.rip_rounds.ripped.values().all(|&r| r <= RIP_MOST),
            "board {n}: {:?}",
            rounds.rip_rounds
        );
    }
}

#[test]
fn the_rounds_are_off_with_zero_rounds_and_count_their_work() {
    // Board 44 leaves a line after the first pass that the stages after it route.
    let boards = hard_boards(45);
    let mut off = boards[44].clone();
    assert_eq!(route_with(&mut off, rip_up_then(0)).rip_rounds, Default::default());
    for (n, board) in boards.into_iter().enumerate() {
        let routing = route_with(&mut board.clone(), rip_up_then(2));
        let r = &routing.rip_rounds;
        assert!(r.rounds <= 2, "board {n}: {r:?}");
        assert!(r.left_after <= r.left_before, "board {n}: {r:?}");
        if r.rounds > 0 {
            assert!(r.searches > 0, "board {n}: {r:?}");
        }
    }
}

/// Three resistors in a row on an outline too thin to pass between them and the edge, and too
/// thin for a via; the first and last are on net A, and R2's pads are in its way.
fn stranded_board() -> Board {
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
fn a_line_walled_in_by_pads_and_the_edge_names_them() {
    let mut board = stranded_board();
    let routing = route_with(&mut board, RouteOptions::default());
    assert_eq!(routing.why.len(), 1, "{:?}", routing.why);
    let why = &routing.why[0];
    assert_eq!(why.line.net, "A");
    // No net's copper is in its way: R2's pads and the edge are.
    assert!(why.blockers.is_empty(), "{why:?}");
    assert!(why.edge, "{why:?}");
    // R2's pad and the other pad of the part the walled end is on.
    assert!(why.pads.iter().any(|p| p.refdes == "R2"), "{why:?}");
    assert!(
        why.pads
            .iter()
            .all(|p| ["R1", "R2", "R3"].contains(&p.refdes.as_str())),
        "{why:?}"
    );
    assert!(why.vias.is_empty(), "{why:?}");
    // The rounds found nothing to rip: the stage it last failed in is the chain's.
    assert_eq!(why.stage, Stage::Chain);
    let json = serde_json::to_value(&routing.why).unwrap();
    assert_eq!(json[0]["edge"], true);
    assert!(json[0]["pads"].as_array().is_some_and(|p| !p.is_empty()));
}

#[test]
fn past_the_time_limit_the_report_still_names_what_walls_a_line_in() {
    let routing = route_with(
        &mut stranded_board(),
        RouteOptions {
            time_limit: Some(std::time::Duration::ZERO),
            ..RouteOptions::default()
        },
    );
    assert!(routing.stopped);
    let walled = routing
        .why
        .iter()
        .find(|w| w.line.net == "A")
        .expect("line A is left");
    assert_eq!(walled.stage, Stage::OutOfTime);
    assert!(walled.edge && !walled.pads.is_empty(), "{walled:?}");
}
