//! The reroute pass (issue #1538): a net that went round its neighbour because the neighbour
//! was laid first is laid again with it, itself first, and the result is kept only when the
//! total by the weights gets shorter with every connection still made. The tests count; the
//! `measured` test prints the totals.

use netsmith_core::{Design, Net, NetRoute, Part, PinRef};
use netsmith_pcb::{Board, Pt, Rect};
use netsmith_router::{Reroute, RouteOptions, Routing, Weights, route_with};

use crate::common;

/// Four 0402 resistors on one layer each net (`route one_layer`), so no via crosses: net `A`
/// joins R1 and R2 across the board, about 16 mm from (13, 10) to (29, 10), and net `B` joins
/// R3 and R4, about 10 mm from (15, 5) to (15, 15), across `A`'s straight way 2 mm from its
/// left end. The router lays the shorter `B` first, straight, and `A` must go round its end;
/// laid after `A`, `B` goes round `A`'s near end at little cost.
fn crossing() -> Board {
    let part = |refdes: &str| Part {
        provides: Vec::new(),
        declared_kind: netsmith_testkit::std_kind("Resistor")
            .and_then(netsmith_core::PartKind::from_word),
        refdes: refdes.into(),
        kind: "Resistor".into(),
        value: "1k".into(),
        package: Some("0402".into()),
        path: String::new(),
        pins: Vec::new(),
    };
    let net = |name: &str, pads: &[(&str, &str)]| Net {
        name: name.into(),
        pins: pads
            .iter()
            .map(|(r, p)| PinRef {
                refdes: (*r).into(),
                pin: (*p).into(),
            })
            .collect(),
    };
    // Each resistor's joined pad is where the header says; its other pad is on no line.
    let places = [
        ("R1", 12.5, 10.0, 0.0),
        ("R2", 29.5, 10.0, 0.0),
        ("R3", 15.0, 4.5, 90.0),
        ("R4", 15.0, 15.5, 90.0),
    ];
    let design = Design {
        modules: Vec::new(),
        name: "Crossing".into(),
        process: Some(common::process()),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts: places.iter().map(|(r, ..)| part(r)).collect(),
        nets: vec![
            net("A", &[("R1", "2"), ("R2", "1")]),
            net("B", &[("R3", "2"), ("R4", "1")]),
        ],
    };
    let mut board = netsmith_placer::board_from_design(
        &design,
        &netsmith_kicad::Mapping::standard(),
        &common::fixtures(),
    )
    .unwrap();
    for (placed, (_, x, y, rotation)) in board.parts.iter_mut().zip(places) {
        placed.at = Pt::new(x, y);
        placed.rotation = rotation;
        placed.reference_side = netsmith_pcb::ReferenceSide::default();
    }
    board.outline = Rect::new(Pt::new(5.0, 0.0), Pt::new(34.0, 20.0));
    board.net_routes = ["A", "B"]
        .into_iter()
        .map(|n| NetRoute {
            net: n.into(),
            pin: format!("{}.x", n.to_lowercase()),
            one_layer: true,
            source: "t.physical.nsm:1".into(),
        })
        .collect();
    board
}

/// The length of the tracks of `net`.
fn length(board: &Board, net: &str) -> f64 {
    board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .map(|t| t.from.distance(t.to))
        .sum()
}

/// `board` routed with the router's own weights stated and `reroute`.
fn routed(mut board: Board, reroute: Reroute) -> (Routing, Board) {
    let routing = route_with(
        &mut board,
        RouteOptions {
            weights: Some(Weights::DEFAULT),
            reroute,
            ..RouteOptions::default()
        },
    );
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    (routing, board)
}

/// Without the pass, the net laid second goes the long way; with it, that net is laid again
/// first with its neighbour, the total is shorter, every connection is made, and the pass is
/// reported with its gain.
#[test]
fn a_net_that_went_round_its_neighbour_is_laid_again_first() {
    let (none, plain) = routed(crossing(), Reroute::NONE);
    assert!(none.rerouted.is_empty());
    assert_eq!(none.reroute_tried, 0);
    let (routing, board) = routed(crossing(), Reroute::DEFAULT);
    let total = |b: &Board| length(b, "A") + length(b, "B");
    println!(
        "without: A {:.1} B {:.1}; with: A {:.1} B {:.1}; {:?}",
        length(&plain, "A"),
        length(&plain, "B"),
        length(&board, "A"),
        length(&board, "B"),
        routing.rerouted
    );
    assert_eq!(routing.rerouted.len(), 1, "{:?}", routing.rerouted);
    let r = &routing.rerouted[0];
    assert!(r.gain() > 0.5, "{r:?}");
    assert_eq!(r.with.len(), 1, "{r:?}");
    assert!(total(&board) < total(&plain), "{} {}", total(&board), total(&plain));
    // Bounded by counts: at most `nets` tries a round, for at most `rounds` rounds.
    let bound = Reroute::DEFAULT.nets * Reroute::DEFAULT.rounds;
    assert!(routing.reroute_tried >= 1 && routing.reroute_tried <= bound);
}

/// With no weights stated, the pass does not run and the board routes as before.
#[test]
fn no_weights_no_reroute() {
    let mut board = crossing();
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty());
    assert!(routing.rerouted.is_empty());
    assert_eq!(routing.reroute_tried, 0);
}

/// Bounded: one net a round for one round tries one net at most.
#[test]
fn the_pass_tries_no_more_nets_than_its_bounds() {
    let (routing, _) = routed(
        crossing(),
        Reroute {
            nets: 1,
            rounds: 1,
        },
    );
    assert!(routing.reroute_tried <= 1, "{}", routing.reroute_tried);
}

/// The totals with and without the pass on generated boards: the nets tried and kept, and the
/// length of the signal copper.
#[test]
#[ignore = "measured: prints the totals of the reroute pass on generated boards"]
fn measured() {
    println!(
        "{:<14} {:>10} {:>10} {:>6} {:>5}",
        "board", "without", "with", "tried", "kept"
    );
    let mut boards: Vec<(String, Board)> = vec![("crossing".into(), crossing())];
    boards.extend((1..=3).map(|seed| {
        (
            format!("generated {seed}"),
            common::synthetic_board(seed, 10, 3, 8, 10, 4),
        )
    }));
    for (name, board) in boards {
        let all = |b: &Board| b.tracks.iter().map(|t| t.from.distance(t.to)).sum::<f64>();
        let (_, plain) = routed(board.clone(), Reroute::NONE);
        let (routing, with) = routed(board, Reroute::DEFAULT);
        println!(
            "{:<14} {:>10.1} {:>10.1} {:>6} {:>5}",
            name,
            all(&plain),
            all(&with),
            routing.reroute_tried,
            routing.rerouted.len()
        );
    }
}
