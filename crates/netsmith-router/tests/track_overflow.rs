//! The placer's track overflow (issue #820) measured where it matters, at the router: how many
//! lines the router's first pass leaves on boards placed with the term and without it.

use crate::common::tight_board_weighing_overflow;
use crate::guard;
use crate::mixed::{AREA, Mix, mixed_board_weighing_overflow};

use netsmith_router::{RouteOptions, route_with};

/// The lines one pass of the router leaves: no other orders, no rip-up, no negotiation, a fixed
/// amount of work, so that the count is the same on every machine.
fn left_by_one_pass(board: &mut netsmith_pcb::Board) -> usize {
    let routing = route_with(
        board,
        RouteOptions {
            retry_orders: false,
            rip_up: false,
            negotiate: false,
            time_limit: Some(guard::GUARD),
            ..RouteOptions::default()
        },
    );
    guard::not_hung(&routing);
    routing.left.len()
}

/// Prints, for each seed in `NETSMITH_SEEDS` (`0..8` by default) of the tight board and the mixed
/// board, the lines one pass of the router leaves when the placer weighs track overflow at
/// `NETSMITH_OVERFLOW` (1 by default) and when it does not, with the wire and the crossings:
/// `NETSMITH_SEEDS=0..8 cargo test -p netsmith-router --test main track_overflow::measured --
/// --ignored --nocapture`. `NETSMITH_BOARDS=tight` or `mixed` measures one of them.
#[test]
#[ignore = "a measurement, not a check"]
fn measured() {
    let range = std::env::var("NETSMITH_SEEDS").unwrap_or_else(|_| "0..8".into());
    let (a, b) = range.split_once("..").expect("a..b");
    let (a, b): (u64, u64) = (a.parse().unwrap(), b.parse().unwrap());
    let weight: f64 = std::env::var("NETSMITH_OVERFLOW")
        .ok()
        .and_then(|w| w.parse().ok())
        .unwrap_or(1.0);
    let boards = std::env::var("NETSMITH_BOARDS").unwrap_or_else(|_| "tight,mixed".into());
    for name in boards.split(',') {
        let mut totals = [0usize; 2];
        for seed in a..b {
            let mut line = format!("{name} seed {seed}:");
            for (k, w) in [0.0, weight].into_iter().enumerate() {
                let board = match name {
                    "tight" => {
                        std::panic::catch_unwind(|| tight_board_weighing_overflow(seed, (27.0, 25.0), w))
                            .ok()
                    }
                    _ => mixed_board_weighing_overflow(seed, AREA, Mix::DEFAULT, w).ok(),
                };
                let Some(mut board) = board else {
                    line += &format!(" weight {w}: does not place;");
                    continue;
                };
                let q = netsmith_pcb::Quality::of(&board);
                let left = left_by_one_pass(&mut board);
                totals[k] += left;
                line += &format!(
                    " weight {w}: left {left}, wire {:.0} mm, crossings {};",
                    q.hpwl, q.crossings
                );
            }
            eprintln!("{line}");
        }
        eprintln!(
            "{name}: {} lines left without the term, {} with it at {weight}",
            totals[0], totals[1]
        );
    }
}
