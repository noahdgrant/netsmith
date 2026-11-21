//! The router's own copper by the quality report's measure (issue #1486, issue #1481): the
//! examples and generated boards, routed, have no defect of any kind `board nets` counts. The
//! router's own tests count the same defects one by one (`netsmith-router`'s `tests/defects.rs`).

use netsmith_fabricate::net_quality::{Counts, Quality};
use netsmith_pcb::Board;

// The placer's synthetic boards and the examples' design files, as the router's tests have them.
#[path = "../../netsmith-placer/tests/common/mod.rs"]
mod placer_common;

fn routed(name: &str, mut board: Board) -> Board {
    assert!(netsmith_router::route(&mut board).is_empty(), "{name}");
    board
}

#[test]
fn the_routed_examples_and_generated_boards_have_no_defect() {
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
            format!("generated board {seed}"),
            placer_common::synthetic_board(seed, 10, 3, 8, 10, 4),
        )
    }));
    for (name, board) in boards {
        let quality = Quality::of_board(&routed(&name, board));
        assert_eq!(quality.totals, Counts::LIMITS, "{name}: {:?}", quality.nets);
    }
}
