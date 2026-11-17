//! The coarse plan on a dense board with inner signal layers (issue #1015, part 2): the mixed
//! board ([`crate::mixed::mixed_board_on`]) on JLCPCB's `SixLayer` with GND and 3V3 planes
//! outside two signal layers, in a smaller outline than the four-layer board's, so that the
//! outer layers are crowded and the inner ones carry tracks. A measurement only: `plan.rs`'s
//! header holds the numbers.

use std::time::{Duration, Instant};

use netsmith_pcb::{Board, CopperLayer};
use netsmith_router::route::CoarsePlan;
use netsmith_router::{RouteOptions, route_with};

use crate::common;
use crate::mixed::{Mix, mixed_board_on};

/// The outline of the dense board: two thirds of the four-layer board's area.
const DENSE: (f64, f64) = (30.0, 45.0);

/// The mixed board of `seed` on six layers, placed in `area`.
pub fn dense_board(seed: u64, area: (f64, f64)) -> Option<Board> {
    mixed_board_on(seed, area, Mix::DEFAULT, 0.0, common::six_layer_signal_process()).ok()
}

/// The length of a board's tracks on the outer layers and on the inner ones, in mm.
fn lengths(board: &Board) -> (f64, f64) {
    let of = |inner: bool| {
        board
            .tracks
            .iter()
            .filter(|t| t.layer.is_outer() != inner)
            .map(|t| t.length())
            .sum::<f64>()
    };
    (of(false), of(true))
}

/// Prints, for each seed in `NETSMITH_SEEDS` (`0..6` by default) and each plan in
/// `NETSMITH_PLANS` (`off,every,later`), how the router ended on the dense board: lines left,
/// tries, vias, the track on the outer and the inner layers, and the time (printed, never
/// asserted). `NETSMITH_AREA=30x45` changes the outline.
#[test]
#[ignore = "a measurement, not a check"]
fn measured() {
    let range = std::env::var("NETSMITH_SEEDS").unwrap_or_else(|_| "0..6".into());
    let (a, b) = range.split_once("..").expect("a..b");
    let (a, b): (u64, u64) = (a.parse().unwrap(), b.parse().unwrap());
    let area = std::env::var("NETSMITH_AREA")
        .ok()
        .and_then(|a| {
            let (w, h) = a.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or(DENSE);
    let plans = std::env::var("NETSMITH_PLANS").unwrap_or_else(|_| "off,every,later".into());
    for seed in a..b {
        let Some(placed) = dense_board(seed, area) else {
            eprintln!("seed {seed}: does not place in {area:?}");
            continue;
        };
        assert!(placed.signal_layers().contains(&CopperLayer::Inner(2)));
        for plan in plans.split(',') {
            let mode = match plan {
                "off" => CoarsePlan::Off,
                "every" => CoarsePlan::EveryTry,
                "later" => CoarsePlan::FromSecondTry,
                other => panic!("a plan is off, every or later, not {other}"),
            };
            let mut board = placed.clone();
            let started = Instant::now();
            let routing = route_with(
                &mut board,
                RouteOptions {
                    time_limit: Some(Duration::from_secs(300)),
                    plan: mode,
                    ..RouteOptions::default()
                },
            );
            let took = started.elapsed();
            let (outer, inner) = lengths(&board);
            eprintln!(
                "seed {seed} plan {plan}: left {} lines, tries {}, vias {}, track {outer:.0} mm outside and {inner:.0} mm inside, stopped {}, in {took:.1?}",
                routing.left.len(),
                routing.tries,
                board.vias.len(),
                routing.stopped,
            );
            let mut nets: Vec<&str> = routing.left.lines.iter().map(|l| l.net.as_str()).collect();
            nets.sort_unstable();
            nets.dedup();
            if !nets.is_empty() {
                eprintln!("    left on {}", nets.join(", "));
            }
        }
    }
}
