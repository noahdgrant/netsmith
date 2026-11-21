//! Negotiated routing (issue #1533): the lines a try leaves after its rip-ups are routed allowing
//! overlaps at a price, the cells fought over get dearer each round, and the overlapping lines are
//! routed again, bounded by counts of rounds and searches. The tests count; `measured` prints the
//! objective of #1517 and the lines left with the negotiation and without it.

use netsmith_pcb::Board;
use netsmith_router::{Negotiation, RouteOptions, Routing, route_with};

use crate::common;
use crate::guard;

/// `board` routed in the first order alone (one try, so what the negotiation did is that try's),
/// with `negotiation`.
fn routed(board: &Board, negotiation: Negotiation) -> (Routing, Board) {
    let mut board = board.clone();
    let routing = route_with(
        &mut board,
        RouteOptions {
            retry_orders: false,
            rip_rounds: 0,
            negotiation,
            time_limit: Some(guard::GUARD),
            ..RouteOptions::default()
        },
    );
    guard::not_hung(&routing);
    (routing, board)
}

/// Mixed seed 7 (`mixed::lines_ripped_up_for_lines_ripped_up_finish_a_board...`), which the
/// rip-ups leave 5 lines of.
fn seed_7() -> Board {
    crate::mixed::mixed_board(7, crate::mixed::AREA, crate::mixed::Mix::DEFAULT)
        .unwrap_or_else(|e| panic!("{e}"))
}

/// The negotiation closes lines of a dense board within its bounds, and the board routes no less
/// completely with it than without it.
#[test]
fn the_negotiation_closes_lines_of_a_dense_board_within_its_counts() {
    let board = seed_7();
    let ((with, _), (without, _)) = std::thread::scope(|s| {
        let without = s.spawn(|| routed(&board, Negotiation::NONE));
        (routed(&board, Negotiation::DEFAULT), without.join().unwrap())
    });
    let n = &with.negotiation;
    assert!(n.kept, "{n:?}");
    println!(
        "with: {} left, {n:?}; without: {} left",
        with.left.len(),
        without.left.len()
    );
    assert!(n.left_before > 0, "the rip-ups left nothing to negotiate");
    assert!(n.left_after < n.left_before, "{n:?}");
    assert!(n.rounds >= 1 && n.rounds <= Negotiation::DEFAULT.rounds, "{n:?}");
    assert!(n.searches >= 2 && n.searches <= Negotiation::DEFAULT.searches, "{n:?}");
    assert!(with.left.len() <= without.left.len());
    assert!(with.left.is_empty(), "{:?}", with.left.lines);
    // Without it, nothing is negotiated and nothing is counted.
    let none = &without.negotiation;
    assert_eq!((none.rounds, none.searches, none.ripped), (0, 0, 0));
}

/// The bounds are counts: one round of at most six searches spends no more, and keeps a state no
/// worse than the one it began with.
#[test]
fn the_negotiation_stops_at_its_counts() {
    let bounds = Negotiation {
        rounds: 1,
        searches: 6,
    };
    let (routing, _) = routed(&seed_7(), bounds);
    let n = &routing.negotiation;
    println!("{n:?}");
    assert_eq!(n.rounds, 1, "{n:?}");
    assert!(n.searches <= bounds.searches, "{n:?}");
    assert!(n.left_after <= n.left_before, "{n:?}");
}

/// The objective of #1517 of a routed board, over the nets no zone pours that have copper
/// (`netsmith_fab::NetTable`, what `board nets` reads): length over straight length, vias, layer
/// changes and the nets over twice their straight length.
fn objective(board: &Board) -> (f64, usize, usize, usize) {
    let art = netsmith_geom::Artwork::parse(&netsmith_pcb::kicad_pcb(board)).unwrap();
    let table = netsmith_fab::NetTable::of(&art, &[], &[]);
    let (mut length, mut straight, mut vias, mut changes, mut over) = (0.0, 0.0, 0, 0, 0);
    for row in &table.nets {
        length += row.length;
        straight += row.straight;
        vias += row.vias;
        changes += row.layer_changes;
        over += usize::from(row.ratio > 2.0);
    }
    let detour = if straight > 0.0 { length / straight } else { 0.0 };
    (detour, vias, changes, over)
}

/// The boards `measured` routes: the examples the router's tests read, and the invented tight
/// and mixed boards, seeds `NETSMITH_SEEDS` (`0..4` by default).
fn measured_boards() -> Vec<(String, Board)> {
    let mut boards: Vec<(String, Board)> = [
        "led",
        "status_indicator",
        "linear_regulator",
        "four_layer",
        "six_layer",
        "ten_layer",
    ]
    .into_iter()
    .map(|e| (e.to_string(), common::example_board(e, &common::fixtures())))
    .collect();
    let range = std::env::var("NETSMITH_SEEDS").unwrap_or_else(|_| "0..4".into());
    let (a, b) = range.split_once("..").expect("a..b");
    let (a, b): (u64, u64) = (a.parse().unwrap(), b.parse().unwrap());
    for seed in a..b {
        boards.push((
            format!("tight {seed}"),
            common::tight_board(seed, (27.0, 25.0)),
        ));
        if let Ok(board) =
            crate::mixed::mixed_board(seed, crate::mixed::AREA, crate::mixed::Mix::DEFAULT)
        {
            boards.push((format!("mixed {seed}"), board));
        }
    }
    boards
}

/// Prints, for each board of [`measured_boards`], the lines left and the objective without the
/// negotiation (`none`: the chain has the lines the rip-ups leave), with it (`negotiated`), and
/// with it and no chain after it (`no chain`, [`RouteOptions::chain`], issue #1566: what the
/// chain closes that the negotiation does not), and the work the negotiation did. One try by default (`NETSMITH_TRIES` for more), so that what is printed is
/// that try's. `cargo test --release -p netsmith-router --test main negotiation::measured --
/// --ignored --nocapture`.
#[test]
#[ignore = "measured: prints the objective and the lines left with the negotiation and without"]
fn measured() {
    let tries = guard::most_tries().or(Some(1));
    println!(
        "{:<18} {:<11} {:>5} {:>7} {:>5} {:>7} {:>5}  negotiation",
        "board", "setting", "left", "detour", "vias", "changes", "over2"
    );
    for (name, board) in measured_boards() {
        for (setting, negotiation, chain) in [
            ("none", Negotiation::NONE, true),
            ("negotiated", Negotiation::DEFAULT, true),
            ("no chain", Negotiation::DEFAULT, false),
        ] {
            let mut b = board.clone();
            let routing = route_with(
                &mut b,
                RouteOptions {
                    negotiation,
                    chain,
                    most_tries: tries,
                    time_limit: Some(guard::GUARD),
                    ..RouteOptions::default()
                },
            );
            guard::not_hung(&routing);
            let (detour, vias, changes, over) = objective(&b);
            let n = &routing.negotiation;
            println!(
                "{name:<18} {setting:<11} {:>5} {detour:>7.3} {vias:>5} {changes:>7} {over:>5}  rounds {} searches {} ripped {} lines {} to {} kept {}",
                routing.left.len(),
                n.rounds,
                n.searches,
                n.ripped,
                n.left_before,
                n.left_after,
                n.kept
            );
        }
    }
}
