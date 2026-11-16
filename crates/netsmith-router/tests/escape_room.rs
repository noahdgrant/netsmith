//! The placer's escape room (issue #997) measured where it matters, at the router: how many
//! lines one pass of the router leaves on the tight board placed with the term and without it.

use crate::common::tight_board_weighing;
use crate::guard;

use netsmith_router::{RouteOptions, route_with};

/// Prints, for each seed in `NETSMITH_SEEDS` (`0..8` by default) of the tight board, the lines
/// one pass of the router leaves (no other orders, no rip-up, no negotiation: a fixed amount of
/// work, so the count is the same on every machine) when the placer weighs escape room at
/// `NETSMITH_ESCAPE` (10 by default) and when it does not, with how far the module's courtyard
/// is from each edge: `cargo test -p netsmith-router --test main escape_room::measured --
/// --ignored --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn measured() {
    let range = std::env::var("NETSMITH_SEEDS").unwrap_or_else(|_| "0..8".into());
    let (a, b) = range.split_once("..").expect("a..b");
    let (a, b): (u64, u64) = (a.parse().unwrap(), b.parse().unwrap());
    let weight: f64 = std::env::var("NETSMITH_ESCAPE")
        .ok()
        .and_then(|w| w.parse().ok())
        .unwrap_or(10.0);
    let mut totals = [0usize; 2];
    for seed in a..b {
        let mut line = format!("tight seed {seed}:");
        for (k, w) in [0.0, weight].into_iter().enumerate() {
            let Ok(mut board) =
                std::panic::catch_unwind(|| tight_board_weighing(seed, (27.0, 25.0), 0.0, w))
            else {
                line += &format!(" weight {w}: does not place;");
                continue;
            };
            let q = netsmith_pcb::Quality::of(&board);
            let routing = route_with(
                &mut board,
                RouteOptions {
                    retry_orders: false,
                    rip_up: false,
                    negotiate: false,
                    time_limit: Some(guard::GUARD),
                    ..RouteOptions::default()
                },
            );
            guard::not_hung(&routing);
            let c = board.part("U1").unwrap().courtyard();
            let o = board.outline;
            let on_u1 = routing
                .left
                .lines
                .iter()
                .filter(|l| l.from.refdes == "U1" || l.to.refdes == "U1")
                .count();
            totals[k] += routing.left.len();
            line += &format!(
                " weight {w}: left {} ({on_u1} on U1), wire {:.0} mm, crossings {}, \
                 U1 from the edges {:.1}/{:.1}/{:.1}/{:.1} mm;",
                routing.left.len(),
                q.hpwl,
                q.crossings,
                c.min.x - o.min.x,
                c.min.y - o.min.y,
                o.max.x - c.max.x,
                o.max.y - c.max.y,
            );
        }
        eprintln!("{line}");
    }
    eprintln!(
        "tight: {} lines left without the term, {} with it at {weight}",
        totals[0], totals[1]
    );
}
