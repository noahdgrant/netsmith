//! A placed board written to JSON and read back is the same board, and routes to the same
//! copper (`netsmith fab --save-placed` and `netsmith route`).

use netsmith_pcb::{Board, kicad_pcb};
use netsmith_router::route;

use crate::common;

fn round_trip(board: &Board) -> Board {
    let text = serde_json::to_string(board).unwrap();
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", board.name))
}

#[test]
fn a_placed_board_read_back_is_equal_and_routes_to_the_same_copper() {
    let mut boards: Vec<Board> = ["led", "status_indicator", "linear_regulator"]
        .iter()
        .map(|name| common::example_board(name, &common::fixtures()))
        .collect();
    boards.extend(common::hard_boards(2));
    // A footprint the generator made carries its package's shape (for the 3D view).
    let mut generated = common::hard_boards(1).remove(0);
    for part in &mut generated.parts {
        part.footprint =
            netsmith_pcb::generated_footprint("Resistor_SMD:R_0402_1005Metric").unwrap();
    }
    boards.push(generated);
    assert!(
        boards
            .iter()
            .flat_map(|b| &b.parts)
            .any(|p| p.footprint.package.is_some()),
        "one board has a generated footprint"
    );
    for board in boards {
        let mut read = round_trip(&board);
        assert_eq!(read, board, "{}", board.name);
        let mut original = board.clone();
        let left = route(&mut original);
        assert_eq!(route(&mut read).lines, left.lines, "{}", board.name);
        assert!(!original.tracks.is_empty(), "{}", board.name);
        assert_eq!(read.tracks, original.tracks, "{}", board.name);
        assert_eq!(read.vias, original.vias, "{}", board.name);
        assert_eq!(kicad_pcb(&read), kicad_pcb(&original), "{}", board.name);
        // A routed board reads back too.
        assert_eq!(round_trip(&original), original, "{}", board.name);
    }
}

#[test]
fn a_four_layer_board_with_planes_reads_back_equal() {
    let mut board = common::hard_boards(1).remove(0);
    board.process = common::four_layer_process();
    board.rules = netsmith_pcb::Rules::for_process(&board.process).unwrap();
    board.planes = vec![
        netsmith_pcb::Plane {
            layer: "In1.Cu".into(),
            net: "N0".into(),
        },
        netsmith_pcb::Plane {
            layer: "In2.Cu".into(),
            net: "N1".into(),
        },
    ];
    assert_eq!(round_trip(&board), board);
}
