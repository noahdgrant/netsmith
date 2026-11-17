//! Tracks on inner signal layers (issue #833, ADR 0111): a line whose outer layers are walled off
//! crosses on the process's inner signal layer, through a via down and a via up, and the board
//! passes netsmith's own design rules check; with the same layer a plane, the wall cannot be
//! crossed. A board with no signal layer routes on its outer layers only, as before.

use netsmith_core::{Design, Net, Part, PinRef};
use netsmith_pcb::{Board, BoardKeepout, CopperLayer, Pt, Ratsnest, Rect, kicad_pcb};
use netsmith_router::{RouteOptions, route_with};

use crate::common;

/// Two 0402 resistors 10 mm apart, joined by `S` (their other pads on GND, which the plane on
/// the first inner layer joins, and on 3V3), and a wall of keepout between them, from x = 10.6
/// to 11.2 mm on the sheet, that no track may cross on either outer layer, though a via may stand
/// anywhere.
pub(crate) fn walled(process: netsmith_core::Process) -> Board {
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
    let places = [("R1", 6.0, 7.0), ("R2", 16.0, 7.0)];
    let design = Design {
        modules: Vec::new(),
        name: "InnerLayers".into(),
        process: Some(process),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts: places.iter().map(|(r, ..)| part(r)).collect(),
        nets: vec![
            net("GND", &[("R1", "1")]),
            net("3V3", &[("R2", "2")]),
            net("S", &[("R1", "2"), ("R2", "1")]),
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
    board.outline = Rect::new(Pt::new(3.0, 3.0), Pt::new(19.0, 11.0));
    board.keepouts.push(BoardKeepout {
        name: "wall".into(),
        part: None,
        at: Some(Pt::new(7.6, 0.0)),
        size: (0.6, 8.0),
        layers: vec!["top".into(), "bottom".into()],
        tracks: true,
        vias: false,
        copper: false,
        parts: false,
        max_height: None,
    });
    board
}

/// The errors of netsmith's own design rules check (ADR 0037), with no KiCad.
fn own_findings(board: &Board) -> Vec<String> {
    let report = netsmith_geom::drc_check(
        &kicad_pcb(board),
        Some(&netsmith_pcb::kicad_pro(board)),
        &Default::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    report.errors().map(|f| f.to_string()).collect()
}

#[test]
fn a_line_walled_off_on_the_outer_layers_crosses_on_the_inner_signal_layer() {
    let mut board = walled(common::four_layer_signal_process());
    assert_eq!(board.signal_layers(), vec![CopperLayer::Inner(2)]);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert_eq!(Ratsnest::of(&board).len(), 0);
    let inner: Vec<_> = board
        .tracks
        .iter()
        .filter(|t| t.layer == CopperLayer::Inner(2))
        .collect();
    assert!(
        !inner.is_empty() && inner.iter().all(|t| t.net == "S"),
        "{:?}",
        board.tracks
    );
    // The inner track runs under the wall, and a via of S stands on each side of it.
    assert!(
        inner
            .iter()
            .any(|t| t.from.x.min(t.to.x) < 10.6 && t.from.x.max(t.to.x) > 11.2),
        "{inner:?}"
    );
    let s_vias: Vec<Pt> = board
        .vias
        .iter()
        .filter(|v| v.net == "S")
        .map(|v| v.at)
        .collect();
    assert!(s_vias.iter().any(|v| v.x < 10.6) && s_vias.iter().any(|v| v.x > 11.2));
    // No inner track is on the plane's layer, and none on an outer layer crosses the wall.
    assert!(board.tracks.iter().all(|t| t.layer != CopperLayer::Inner(1)));
    assert!(board.tracks.iter().filter(|t| t.layer.is_outer()).all(|t| {
        let (lo, hi) = (t.from.x.min(t.to.x), t.from.x.max(t.to.x));
        hi <= 10.6 || lo >= 11.2
    }));
    // The board file has the inner track on `In2.Cu`, and netsmith's check passes it.
    assert!(kicad_pcb(&board).contains("(layer \"In2.Cu\")"));
    assert_eq!(own_findings(&board), Vec::<String>::new());
}

#[test]
fn with_the_same_layer_a_plane_the_wall_cannot_be_crossed() {
    let mut board = walled(common::four_layer_process());
    assert!(board.signal_layers().is_empty());
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(
        routing.left.lines.iter().any(|l| l.net == "S"),
        "{:?}",
        board.tracks
    );
    assert!(board.tracks.iter().all(|t| t.layer.is_outer()));
}

/// A current on `S` whose least widths are 0.3 mm outside and `inner` inside.
fn with_current(board: &mut Board, inner: Option<f64>) {
    board.net_widths.push(("S".into(), 0.3));
    board.net_currents.push(netsmith_core::NetCurrent {
        net: "S".into(),
        current: 1.0,
        least_width: 0.3,
        inner_least_width: inner,
        vias: 1,
    });
}

#[test]
fn a_net_with_a_known_current_crosses_inside_at_the_width_its_current_needs_there() {
    // Its outer width is sized for the outer copper (ADR 0036) and its inner width for the inner
    // copper (issue #833): the track under the wall is drawn at the inner width, rounded up to
    // the 0.01 mm a width is written in, and the check holds it to that width.
    let mut board = walled(common::four_layer_signal_process());
    with_current(&mut board, Some(0.444));
    assert!((board.track_width_on("S", CopperLayer::Inner(2)) - 0.45).abs() < 1e-9);
    assert!((board.track_width_on("S", CopperLayer::Front) - 0.3).abs() < 1e-9);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    let inner: Vec<_> = board
        .tracks
        .iter()
        .filter(|t| t.layer == CopperLayer::Inner(2))
        .collect();
    assert!(
        inner
            .iter()
            .any(|t| t.from.x.min(t.to.x) < 10.6 && t.from.x.max(t.to.x) > 11.2),
        "{inner:?}"
    );
    assert!(
        inner.iter().all(|t| (t.width - 0.45).abs() < 1e-9),
        "{inner:?}"
    );
    assert!(
        board
            .tracks
            .iter()
            .filter(|t| t.net == "S" && t.layer.is_outer())
            .all(|t| t.width <= 0.3 + 1e-9),
        "{:?}",
        board.tracks
    );
    let extras = netsmith_geom::DrcExtras {
        least_widths: board.least_widths(),
        inner_least_widths: board.inner_least_widths(),
        neck_reach: netsmith_router::route::NECK_REACH,
        ..Default::default()
    };
    let report = netsmith_geom::drc_check(
        &kicad_pcb(&board),
        Some(&netsmith_pcb::kicad_pro(&board)),
        &extras,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let errors: Vec<String> = report.errors().map(|f| f.to_string()).collect();
    assert_eq!(errors, Vec::<String>::new());
}

#[test]
fn a_net_whose_design_gives_no_inner_width_keeps_to_the_outer_layers() {
    // A design written before an inner width was sized: its width is for the outer copper only,
    // which an inner layer's thinner copper would not carry, so the wall stays closed to it.
    let mut board = walled(common::four_layer_signal_process());
    with_current(&mut board, None);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(
        routing.left.lines.iter().any(|l| l.net == "S"),
        "{:?}",
        board.tracks
    );
    assert!(board.tracks.iter().all(|t| t.layer.is_outer()));
}

#[test]
fn the_coarse_plan_crosses_the_wall_on_the_inner_signal_layer() {
    // Issue #1015: the plan is of every layer of the stack, so a line it steers may plan its
    // way under the wall, as the search finds it.
    let mut board = walled(common::four_layer_signal_process());
    let routing = route_with(
        &mut board,
        RouteOptions {
            plan: netsmith_router::route::CoarsePlan::EveryTry,
            ..RouteOptions::default()
        },
    );
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert!(
        board
            .tracks
            .iter()
            .any(|t| t.net == "S" && t.layer == CopperLayer::Inner(2)),
        "{:?}",
        board.tracks
    );
}
