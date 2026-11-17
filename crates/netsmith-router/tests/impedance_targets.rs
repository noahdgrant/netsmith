//! Nets with an impedance target (issue #1025): the router draws a net with a `TrackImpedance`
//! at the width that gives it on each layer, and the two nets of a `PairImpedance` at the pair's
//! width and never nearer than its spacing; netsmith's design rules
//! check holds the tracks to the width.

use netsmith_core::{Design, ImpedanceTarget, Net, Part, PinRef};
use netsmith_pcb::{Board, CopperLayer, Pt, Rect, kicad_pcb};
use netsmith_router::{RouteOptions, route_with};

use crate::common;
use crate::inner_layers::walled;

/// A target on `nets`: single-ended with no spacing, a pair with one.
fn target(nets: &[&str], ohms: f64, spacing: Option<f64>) -> ImpedanceTarget {
    ImpedanceTarget {
        path: "z".into(),
        nets: nets.iter().map(|n| n.to_string()).collect(),
        ohms,
        spacing,
    }
}

/// netsmith's own design rules check of the board as `fab` gives it the targets' widths: its
/// errors and its `track_impedance` findings.
fn findings(board: &Board) -> Vec<String> {
    let extras = netsmith_geom::DrcExtras {
        target_widths: board.target_track_widths(),
        neck_reach: netsmith_router::route::NECK_REACH,
        ..Default::default()
    };
    let report = netsmith_geom::drc_check(
        &kicad_pcb(board),
        Some(&netsmith_pcb::kicad_pro(board)),
        &extras,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    report
        .findings
        .iter()
        .filter(|f| {
            f.severity == netsmith_geom::DrcSeverity::Error
                || f.rule == netsmith_geom::DrcRule::TrackImpedance
        })
        .map(|f| f.to_string())
        .collect()
}

#[test]
fn a_net_with_a_target_is_drawn_at_the_width_that_gives_it_on_each_layer() {
    // The walled board: `S` crosses the wall on the inner signal layer, a stripline, which is
    // narrower for 50 ohm than the microstrip outside.
    let mut board = walled(common::four_layer_signal_process());
    board.set_impedance_targets(vec![target(&["S"], 50.0, None)]);
    let front = board.target_width("S", CopperLayer::Front).unwrap();
    let inner = board.target_width("S", CopperLayer::Inner(2)).unwrap();
    assert!(inner < front - 1e-9, "{front} mm outside, {inner} mm inside");
    assert_eq!(board.track_width_on("S", CopperLayer::Front), front);
    assert_eq!(board.track_width_on("S", CopperLayer::Inner(2)), inner);
    assert_eq!(board.impedance_findings(), Vec::<String>::new());
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    println!("{} tries", routing.tries);
    let s: Vec<_> = board.tracks.iter().filter(|t| t.net == "S").collect();
    assert!(s.iter().any(|t| t.layer == CopperLayer::Inner(2)), "{s:?}");
    for t in &s {
        let want = board.track_width_on("S", t.layer);
        assert!((t.width - want).abs() < 1e-9, "{t:?} is not {want} mm wide");
    }
    assert_eq!(findings(&board), Vec::<String>::new());
    // Each layer's track is within JLCPCB's test tolerance of the target.
    for (layer, width) in [("F.Cu", front), ("In2.Cu", inner)] {
        let z = netsmith_pcb::impedance_at(&board.process.stackup, layer, width, None).unwrap();
        assert!((z - 50.0).abs() < 5.0, "{layer}: {width} mm is {z} ohm");
    }
}

#[test]
fn the_check_names_a_track_of_a_target_net_drawn_at_another_width() {
    let mut board = walled(common::four_layer_signal_process());
    board.set_impedance_targets(vec![target(&["S"], 50.0, None)]);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    // The longest track of S, made 0.1 mm wider than its target.
    let k = (0..board.tracks.len())
        .filter(|&k| board.tracks[k].net == "S")
        .max_by(|&a, &b| board.tracks[a].length().total_cmp(&board.tracks[b].length()))
        .unwrap();
    board.tracks[k].width += 0.1;
    let found = findings(&board);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("its net's impedance target needs"), "{found:?}");
}

/// Four 0402 resistors: `A` from R1 to R2 and `B` from R3 to R4, the second two `apart` mm
/// below the first and `out` mm further out to each side, on a four-layer board with a signal
/// layer.
fn pair_board(apart: f64, out: f64) -> Board {
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
    let places = [
        ("R1", 6.0, 6.0),
        ("R2", 18.0, 6.0),
        ("R3", 6.0 - out, 6.0 + apart),
        ("R4", 18.0 + out, 6.0 + apart),
    ];
    let design = Design {
        modules: Vec::new(),
        name: "Pair".into(),
        process: Some(common::four_layer_signal_process()),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts: places.iter().map(|(r, ..)| part(r)).collect(),
        nets: vec![
            net("GND", &[("R1", "1"), ("R3", "1")]),
            net("N", &[("R2", "2"), ("R4", "2")]),
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
    for (placed, (_, x, y)) in board.parts.iter_mut().zip(places) {
        placed.at = Pt::new(x, y);
        placed.rotation = 0.0;
        placed.reference_side = netsmith_pcb::ReferenceSide::default();
    }
    board.outline = Rect::new(Pt::new(1.0, 3.0), Pt::new(23.0, 11.0));
    board
}

#[test]
fn a_pair_with_a_target_is_drawn_at_its_width_and_never_nearer_than_its_spacing() {
    // `A` and `B` say by no name that they are a pair: the target says it. Their parts are 1.2
    // mm apart, which the courtyards of 0402 parts leave no nearer; the router does not jog them
    // together (issue #1025 leaves that to a coupled route), and the notes say how much of the
    // pair keeps a gap that gives its impedance.
    let mut board = pair_board(1.2, 0.0);
    board.set_impedance_targets(vec![target(&["A", "B"], 90.0, Some(0.2))]);
    let width = board.target_width("A", CopperLayer::Front).unwrap();
    assert_eq!(board.target_width("B", CopperLayer::Front), Some(width));
    assert!(board.clearance("A") >= 0.2 && board.clearance("B") >= 0.2);
    assert_eq!(board.impedance_findings(), Vec::<String>::new());
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    println!("{} tries", routing.tries);
    let tracks = |net: &str| -> Vec<netsmith_pcb::Track> {
        board
            .tracks
            .iter()
            .filter(|t| t.net == net)
            .cloned()
            .collect()
    };
    let (a, b) = (tracks("A"), tracks("B"));
    for t in a.iter().chain(&b) {
        let want = board.track_width_on(&t.net, t.layer);
        assert!((t.width - want).abs() < 1e-9, "{t:?} is not {want} mm wide");
    }
    // No piece of A comes nearer B than the spacing, edge to edge.
    for t in &a {
        for o in b.iter().filter(|o| o.layer == t.layer) {
            for p in t.centre_line() {
                let gap = o.distance_to(p) - o.width / 2.0 - t.width / 2.0;
                assert!(gap >= 0.2 - 1e-6, "{t:?} is {gap} mm from {o:?}");
            }
        }
    }
    let couplings = board.pair_couplings();
    assert_eq!(couplings.len(), 1);
    println!("{:?}", couplings[0]);
    assert!(
        board
            .process_extras()
            .iter()
            .any(|l| l.starts_with("track impedance pair: nets `A` and `B` (z): ")),
        "{:?}",
        board.process_extras()
    );
    assert_eq!(findings(&board), Vec::<String>::new());
    // The project file gives the pair's class its gap and width.
    let pro = netsmith_pcb::kicad_pro(&board);
    assert!(pro.contains("\"diff_pair_gap\": 0.2"), "{pro}");
    assert!(pro.contains(&format!("\"diff_pair_width\": {width}")), "{pro}");
}
