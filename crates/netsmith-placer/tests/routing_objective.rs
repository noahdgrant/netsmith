//! The placement objective for routing (issue #1534, `PlaceOptions::routing_objective`): off by
//! default and inert on a board with nothing it weighs; on a board with a power chain, a memory
//! on a bus and parts on the back, it leaves fewer parts under the fine-pitch package's escape
//! ring and in the bus's corridor, and the power chain no longer. The tests count
//! (`netsmith_placer::routing_counts`); the numbers before and after routing are printed by
//! `placement_for_routing::measured` of netsmith-fabricate.

use netsmith_pcb::{Board, PlaceOptions, ROUTING_OBJECTIVE};
use netsmith_placer::{RoutingCounts, routing_counts};

use crate::common::{routing_board, synthetic_board_placed};

fn places(board: &Board) -> Vec<(String, f64, f64, f64, bool)> {
    board
        .parts
        .iter()
        .map(|p| {
            (
                p.refdes.clone(),
                p.at.x,
                p.at.y,
                p.rotation,
                p.side == netsmith_pcb::Side::Back,
            )
        })
        .collect()
}

/// The default leaves the objective out: the board is the one placed with a weight of 0.
#[test]
fn the_objective_is_off_by_default() {
    assert_eq!(PlaceOptions::default().routing_objective, 0.0);
    let default = routing_board(1, 6, PlaceOptions::default().routing_objective);
    let off = routing_board(1, 6, 0.0);
    assert_eq!(places(&default), places(&off));
}

/// On a board with no current stated, no bus and every part on one face, the objective costs
/// nothing anywhere, so the run draws the same moves and ends in the same place.
#[test]
fn a_board_with_nothing_it_weighs_is_placed_as_before() {
    let with = |objective: f64| {
        synthetic_board_placed(
            2,
            10,
            3,
            8,
            10,
            4,
            &PlaceOptions {
                routing_objective: objective,
                ..PlaceOptions::default()
            },
        )
    };
    let (off, on) = (with(0.0), with(ROUTING_OBJECTIVE));
    let counts = routing_counts(&on);
    assert_eq!((counts.buses, counts.under_escape, counts.chain_faces), (0, 0, 0));
    assert_eq!(places(&off), places(&on));
}

/// With the objective fewer parts stand under U1's escape ring on the back or in the memory's
/// corridor than without it, on every seed and by more than half in all (the costs are soft, so
/// a part pulled hard enough may stay), the power chain keeps to one face, and its nets are no
/// longer in all.
#[test]
fn the_objective_clears_the_escape_and_the_corridor_and_shortens_the_chain() {
    let (mut off_total, mut on_total) = (RoutingCounts::default(), RoutingCounts::default());
    for seed in 1..=3 {
        let off = routing_counts(&routing_board(seed, 8, 0.0));
        let on = routing_counts(&routing_board(seed, 8, ROUTING_OBJECTIVE));
        println!("seed {seed}: without {off:?}\n        with    {on:?}");
        assert_eq!((off.buses, on.buses), (1, 1), "seed {seed}: the memory's bus");
        assert!(
            on.under_escape + on.in_corridors <= off.under_escape + off.in_corridors,
            "seed {seed}: with {on:?}, without {off:?}"
        );
        assert_eq!(on.chain_faces, 1, "seed {seed}: {on:?}");
        for (total, counts) in [(&mut off_total, &off), (&mut on_total, &on)] {
            total.under_escape += counts.under_escape;
            total.in_corridors += counts.in_corridors;
            total.chain_length += counts.chain_length;
        }
    }
    let (off, on) = (
        off_total.under_escape + off_total.in_corridors,
        on_total.under_escape + on_total.in_corridors,
    );
    assert!(off > 0, "without the objective something stands in the way: {off_total:?}");
    assert!(2 * on < off, "in the way: {on} with, {off} without");
    assert!(
        on_total.chain_length <= off_total.chain_length,
        "the chain: {} mm with, {} mm without",
        on_total.chain_length,
        off_total.chain_length
    );
}
