//! The placement objective for routing (issue #1534, `PlaceOptions::routing_objective`),
//! measured: each board placed without and with it and routed, by the router's objective that
//! `board nets` reports (vias, layer changes, detour, nets over twice their straight length)
//! and the counts of the placement objective (`netsmith_placer::routing_counts`). The tests that
//! assert counts are the placer's (`tests/routing_objective.rs`).
//!
//! `cargo test -p netsmith-fabricate --test main placement_for_routing::measured --
//! --ignored --nocapture` prints the table; its numbers are in the header of the placer's
//! `anneal/routing.rs`.

use netsmith_fabricate::net_quality::Objective;
use netsmith_pcb::{Board, PlaceOptions, ROUTING_OBJECTIVE};
use netsmith_placer::{RoutingCounts, routing_counts};

use crate::router_quality::placer_common;

/// The example `name` placed with the objective at `weight`.
fn example(name: &str, weight: f64) -> Board {
    netsmith_placer::board_from_design_placed(
        &placer_common::example_design(name),
        &netsmith_kicad::Mapping::standard(),
        &placer_common::fixtures(),
        &PlaceOptions {
            routing_objective: weight,
            ..PlaceOptions::default()
        },
    )
    .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The boards, each placed at `weight`: the routed examples and three generated boards with a
/// power chain, a memory on a bus and parts on the back.
fn boards(weight: f64) -> Vec<(String, Board)> {
    let mut boards: Vec<(String, Board)> =
        ["led", "status_indicator", "linear_regulator", "four_layer", "six_layer"]
            .into_iter()
            .map(|name| (name.to_string(), example(name, weight)))
            .collect();
    boards.extend((1..=3).map(|seed| {
        (
            format!("generated {seed}"),
            placer_common::routing_board(seed, 8, weight),
        )
    }));
    boards
}

/// `board` routed: the lines left, and the router's objective.
fn routed(mut board: Board) -> (usize, Objective) {
    let left = netsmith_router::route(&mut board).len();
    let table = netsmith_fabricate::net_table::of_board(&board).expect("a table");
    (left, Objective::of_table(&table))
}

/// The numbers before and after, for each board: the placement's counts, the lines the router
/// left, the vias, the layer changes, the detour (routed over straight length) and the nets over
/// twice their straight length.
#[test]
#[ignore = "measured: prints the router's objective of each board placed without and with the objective"]
fn measured() {
    println!(
        "{:<18} {:<8} {:>5} {:>8} {:>7} {:>5} {:>4} {:>7} {:>7} {:>5}",
        "board",
        "placed",
        "under",
        "corridor",
        "chain",
        "left",
        "vias",
        "changes",
        "detour",
        "over2"
    );
    let label = |w: f64| if w > 0.0 { "with" } else { "without" };
    for (off, on) in boards(0.0).into_iter().zip(boards(ROUTING_OBJECTIVE)) {
        for (weight, (name, board)) in [(0.0, off), (ROUTING_OBJECTIVE, on)] {
            let RoutingCounts {
                under_escape,
                in_corridors,
                chain_length,
                ..
            } = routing_counts(&board);
            let (left, o) = routed(board);
            println!(
                "{:<18} {:<8} {:>5} {:>8} {:>7.1} {:>5} {:>4} {:>7} {:>7.3} {:>5}",
                name,
                label(weight),
                under_escape,
                in_corridors,
                chain_length,
                left,
                o.vias,
                o.layer_changes,
                o.detour,
                o.over_twice
            );
        }
    }
}
