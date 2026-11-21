//! The router's quality objective (issue #1517): the weights of length, vias and layer changes
//! of the signal nets, the polish pass they turn on, and the 45 degree corners of a mitered
//! board, measured on the examples and generated boards by the objective `board nets` reports
//! (`netsmith_fabricate::net_quality::Objective`). The tests count; `measured` prints the
//! numbers before and after.

use netsmith_fabricate::net_quality::{Counts, Objective, Quality};
use netsmith_pcb::Board;
use netsmith_router::{RouteOptions, Routing, Weights, route_with};

// The placer's synthetic boards and the examples' design files, as `router_quality` has them.
use crate::router_quality::placer_common;

/// The boards: the routed examples and three generated boards.
fn boards() -> Vec<(String, Board)> {
    let mut boards: Vec<(String, Board)> =
        ["led", "status_indicator", "linear_regulator", "four_layer", "six_layer"]
            .into_iter()
            .map(|name| {
                let board = placer_common::example_board(name, &placer_common::fixtures());
                (name.to_string(), board)
            })
            .collect();
    boards.extend((1..=3).map(|seed| {
        (
            format!("generated {seed}"),
            placer_common::synthetic_board(seed, 10, 3, 8, 10, 4),
        )
    }));
    boards
}

/// `board` routed with `weights`: the routing, the board and its objective.
fn routed(name: &str, mut board: Board, weights: Option<Weights>) -> (Routing, Board, Objective) {
    let routing = route_with(
        &mut board,
        RouteOptions {
            weights,
            ..RouteOptions::default()
        },
    );
    assert!(routing.left.is_empty(), "{name} with {weights:?}");
    let table = netsmith_fabricate::net_table::of_board(&board).expect("a table");
    (routing, board, Objective::of_table(&table))
}

/// Vias three times dearer than the router's own.
const DEAR_VIAS: Weights = Weights {
    via: 45.0,
    ..Weights::DEFAULT
};

/// With no weights the router routes at its own costs and polishes nothing; with dear vias every
/// board still routes, with no defect the quality report counts, and with no more vias and layer
/// changes over all the boards than before.
#[test]
fn dear_vias_route_every_board_with_no_more_vias() {
    let (mut own_vias, mut dear_vias) = (0, 0);
    for (name, board) in boards() {
        let (routing, _, own) = routed(&name, board.clone(), None);
        assert_eq!(routing.polished, 0, "{name}: no weights, no polish");
        let (_, weighed, dear) = routed(&name, board, Some(DEAR_VIAS));
        let quality = Quality::of_board(&weighed);
        assert_eq!(quality.totals, Counts::LIMITS, "{name}: {:?}", quality.nets);
        own_vias += own.vias + own.layer_changes;
        dear_vias += dear.vias + dear.layer_changes;
    }
    assert!(
        dear_vias <= own_vias,
        "dear vias: {dear_vias} vias and layer changes against {own_vias}"
    );
}

/// A mitered board (`.corner`, issue #1508) routed with weights has only bends of 45 degrees:
/// no arc, and no corner the design rules check's `track_angle` finds.
#[test]
fn a_mitered_board_with_weights_has_45_degree_corners_only() {
    for (name, mut board) in boards().into_iter().skip(5).take(1) {
        board.corner = netsmith_core::TrackCorner::Mitered;
        let (_, routed, _) = routed(&name, board, Some(DEAR_VIAS));
        assert!(routed.tracks.iter().all(|t| t.mid.is_none()), "{name}");
        let checked = netsmith_fabricate::drc::of_board(&routed, None, false).expect("checked");
        let angles = checked
            .report
            .findings
            .iter()
            .filter(|f| f.rule == netsmith_geom::DrcRule::TrackAngle)
            .count();
        assert_eq!(angles, 0, "{name}");
    }
}

/// The numbers before and after: for each board and each set of weights, the vias, the layer
/// changes, the detour (routed over straight length), the nets over twice their straight length
/// and the lines the polish pass gave cheaper copper.
#[test]
#[ignore = "measured: prints the objective of each board at each set of weights"]
fn measured() {
    let sets: [(&str, Option<Weights>); 4] = [
        ("own", None),
        ("own, polished", Some(Weights::DEFAULT)),
        ("via 45", Some(DEAR_VIAS)),
        (
            "via 45, layer 15",
            Some(Weights {
                layer_change: 15.0,
                ..DEAR_VIAS
            }),
        ),
    ];
    println!(
        "{:<18} {:<18} {:>5} {:>7} {:>7} {:>6} {:>8}",
        "board", "weights", "vias", "changes", "detour", "over2", "polished"
    );
    for (name, board) in boards() {
        for (label, weights) in sets {
            let (routing, _, o) = routed(&name, board.clone(), weights);
            println!(
                "{:<18} {:<18} {:>5} {:>7} {:>7.3} {:>6} {:>8}",
                name, label, o.vias, o.layer_changes, o.detour, o.over_twice, routing.polished
            );
        }
    }
}
