//! Nets routed on one layer with no via (issue #1274): a net with a stated impedance of one
//! track (an RF net) and a net the physical file says `route one_layer` of. The router keeps
//! such a net on the outer layer all its pads have copper on, gives it no via, and routes it
//! first; `route any_layer` lets an RF net change layers as any net does.

use netsmith_core::{ImpedanceTarget, NetRoute};
use netsmith_pcb::{Board, CopperLayer};
use netsmith_router::{RouteOptions, route_with};

use crate::common;
use crate::inner_layers::walled;

fn rf(board: &mut Board) {
    board.set_impedance_targets(vec![ImpedanceTarget {
        path: "rf_line".into(),
        nets: vec!["S".into()],
        ohms: 50.0,
        spacing: None,
        skew: None,
    }]);
}

/// The walled board with a gap of 2 mm under the wall, on the outer layers too.
fn gapped() -> Board {
    let mut board = walled(common::four_layer_signal_process());
    let wall = board.keepouts.iter_mut().find(|k| k.name == "wall").unwrap();
    wall.size = (0.6, 6.0);
    board
}

/// The layers of the tracks of `S`, and its vias.
fn copper_of_s(board: &Board) -> (Vec<CopperLayer>, usize) {
    let mut layers: Vec<CopperLayer> = board
        .tracks
        .iter()
        .filter(|t| t.net == "S")
        .map(|t| t.layer)
        .collect();
    layers.sort_by_key(|l| l.name());
    layers.dedup();
    let vias = board.vias.iter().filter(|v| v.net == "S").count();
    (layers, vias)
}

#[test]
fn an_rf_net_is_routed_on_one_layer_with_no_via() {
    let mut board = gapped();
    rf(&mut board);
    assert_eq!(board.one_layer_nets(), ["S"]);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert_eq!(copper_of_s(&board), (vec![CopperLayer::Front], 0));
    // It goes round the wall, through the gap under it.
    assert!(
        board
            .tracks
            .iter()
            .filter(|t| t.net == "S")
            .any(|t| t.to.y.max(t.from.y) > board.outline.min.y + 6.0),
        "{:?}",
        board.tracks
    );
}

#[test]
fn a_net_with_no_way_on_one_layer_is_left_and_given_no_via() {
    // The wall closes both outer layers; only the inner layer crosses it, which a via reaches.
    let mut board = walled(common::four_layer_signal_process());
    rf(&mut board);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(
        routing.left.lines.iter().any(|l| l.net == "S"),
        "{:?}",
        routing.left.lines
    );
    assert_eq!(copper_of_s(&board).1, 0, "{:?}", board.vias);
    assert!(
        board
            .tracks
            .iter()
            .filter(|t| t.net == "S")
            .all(|t| t.layer == CopperLayer::Front),
        "{:?}",
        board.tracks
    );
}

#[test]
fn route_any_layer_lets_an_rf_net_change_layers() {
    let mut board = walled(common::four_layer_signal_process());
    rf(&mut board);
    board.net_routes = vec![NetRoute {
        net: "S".into(),
        pin: "r1.b".into(),
        one_layer: false,
        source: "t.physical.nsm:3".into(),
    }];
    assert!(board.one_layer_nets().is_empty());
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    let (layers, vias) = copper_of_s(&board);
    assert!(layers.contains(&CopperLayer::Inner(2)), "{layers:?}");
    assert!(vias >= 2, "{vias} vias");
}

#[test]
fn route_one_layer_holds_a_net_with_no_impedance_to_one_layer() {
    let mut board = walled(common::four_layer_signal_process());
    board.net_routes = vec![NetRoute {
        net: "S".into(),
        pin: "r1.b".into(),
        one_layer: true,
        source: "t.physical.nsm:3".into(),
    }];
    assert_eq!(board.one_layer_nets(), ["S"]);
    assert!(board.rf_nets().is_empty());
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.lines.iter().any(|l| l.net == "S"));
    assert_eq!(copper_of_s(&board).1, 0);
}
