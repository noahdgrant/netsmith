//! `netsmith routes route --region` (issue #1641, `netsmith_fabricate::route_region`): a knot of
//! hand-drawn copper across a box that blocks a connection is ripped and the connection routed
//! through it, the knot's copper outside the box kept; a connection cut at the box's edge is
//! joined again; a region that cannot be closed writes nothing and names the connection; locked
//! copper is never touched. The board is the LED example, as `route_net`'s tests have it.

use netsmith_fabricate::route_net::{Ask, route_net};
use netsmith_fabricate::route_region::{RegionAsk, route_region};
use netsmith_fabricate::routes::{drc_errors, with_routes};
use netsmith_pcb::{Board, CopperLayer, NetRoutes, Pt, Ratsnest, RouteTrack, Routes, pad_hash};
use netsmith_pipeline::Libraries;
use netsmith_solve::Catalogs;

const LED: &str = include_str!("../../../examples/led.nsm");

fn board() -> Board {
    let libraries = Libraries::sample();
    let built = netsmith_pipeline::build_with(
        "led.nsm",
        LED,
        &netsmith_check::Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    netsmith_placer::board_from_design(
        &built[0].design,
        &libraries.mapping,
        &libraries.footprints,
    )
    .unwrap()
}

fn pads_of(board: &Board, net: &str) -> Vec<Pt> {
    let n = board.nets.iter().find(|n| n.name == net).unwrap();
    n.pads
        .iter()
        .flat_map(|p| {
            let part = board.part(&p.refdes).unwrap();
            part.footprint
                .pads_numbered(&p.number)
                .map(|pad| part.pad_centre(pad))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The net of two pads farthest apart along x, another net for the knot, and the x half way
/// between the net's pads, in the routes file's places.
fn nets(board: &Board) -> (String, String, f64) {
    let mut two: Vec<(f64, String)> = board
        .nets
        .iter()
        .filter(|n| n.pads.len() == 2)
        .map(|n| {
            let p = pads_of(board, &n.name);
            ((p[0].x - p[1].x).abs(), n.name.clone())
        })
        .collect();
    two.sort_by(|a, b| a.0.total_cmp(&b.0));
    let target = two.last().unwrap().1.clone();
    let knot = if target == "GND" { "VCC" } else { "GND" };
    let p = pads_of(board, &target);
    let x = (p[0].x + p[1].x) / 2.0 - board.outline.min.x;
    (target, knot.to_string(), x)
}

/// A wall of `net`'s tracks at `x` from the top edge to the bottom on both layers.
fn wall(board: &Board, net: &str, x: f64, lock: bool) -> Vec<RouteTrack> {
    let edge = board.rules.copper_to_edge + 0.2;
    let h = board.outline.height();
    let width = board.track_width(net);
    [CopperLayer::Front, CopperLayer::Back]
        .into_iter()
        .map(|layer| RouteTrack {
            layer,
            width,
            from: [x, edge],
            mid: None,
            to: [x, h - edge],
            lock,
        })
        .collect()
}

fn with_record(routes: &mut Routes, board: &Board, net: &str, tracks: Vec<RouteTrack>, lock: bool) {
    routes.boards.entry(board.name.clone()).or_default().nets.insert(
        net.to_string(),
        NetRoutes {
            pads: pad_hash(board, net).unwrap(),
            lock,
            tracks,
            vias: Vec::new(),
            zones: Vec::new(),
        },
    );
}

fn open(board: &Board, routes: &Routes, net: &str) -> usize {
    let (b, _) = with_routes(board, routes).unwrap();
    Ratsnest::of(&b).lines.iter().filter(|l| l.net == net).count()
}

/// A band round `x`, most of the board's height.
fn band(board: &Board, x: f64, top: f64) -> [f64; 4] {
    let h = board.outline.height();
    [x - 1.0, h * top, 2.0, h * (1.0 - 2.0 * top)]
}

fn ask(rect: [f64; 4]) -> RegionAsk {
    RegionAsk {
        rect,
        weights: None,
    }
}

#[test]
fn a_knot_that_blocks_a_connection_is_ripped_and_the_connection_routed_through_it() {
    let board = board();
    let (target, knot, x) = nets(&board);
    let mut routes = Routes::default();
    with_record(&mut routes, &board, &knot, wall(&board, &knot, x, false), false);
    assert!(drc_errors(&with_routes(&board, &routes).unwrap().0).unwrap().is_empty());
    let blocked = route_net(&board, &routes, &Ask::of(&board, &target, &[]).unwrap()).unwrap();
    assert!(blocked.refused.is_some(), "{}", blocked.text());

    let said = route_region(&board, &routes, &ask(band(&board, x, 0.15))).unwrap();
    assert_eq!(said.refused, None, "{}", said.text());
    assert!(said.through.iter().any(|l| l.contains(&target)), "{}", said.text());
    let after = said.routes.clone().expect("written");
    assert_eq!(open(&board, &after, &target), 0, "{}", said.text());
    let errors = drc_errors(&with_routes(&board, &after).unwrap().0).unwrap();
    assert!(errors.is_empty(), "{errors:#?}");
    // The knot's copper outside the box is kept: a piece on each layer from each edge to the
    // box (it was dead copper before the cut, so none of it is taken off).
    let kept = &after.boards[&board.name].nets[&knot].tracks;
    let rect = band(&board, x, 0.15);
    let edge = board.rules.copper_to_edge + 0.2;
    let h = board.outline.height();
    for layer in [CopperLayer::Front, CopperLayer::Back] {
        for (from, to) in [(edge, rect[1]), (rect[1] + rect[3], h - edge)] {
            assert!(
                kept.iter().any(|t| t.layer == layer
                    && (t.from[0] - x).abs() < 1e-6
                    && (t.to[0] - x).abs() < 1e-6
                    && ((t.from[1] - from).abs() < 1e-3 || (t.from[1] - to).abs() < 1e-3)),
                "no piece of the knot from {from} to {to} on {layer:?}: {kept:?}"
            );
        }
    }
    let knot_said = said.nets.iter().find(|n| n.net == knot).unwrap();
    assert_eq!(knot_said.cut_tracks, 2, "{}", said.text());
    assert!(said.text().contains("kept"), "{}", said.text());
}

#[test]
fn a_connection_cut_at_the_edge_is_joined_again_between_its_cut_ends() {
    let board = board();
    let (target, _, x) = nets(&board);
    let first = route_net(&board, &Routes::default(), &Ask::of(&board, &target, &[]).unwrap())
        .unwrap();
    let routes = first.routes.clone().expect("the net routes");
    let said = route_region(&board, &routes, &ask(band(&board, x, 0.0))).unwrap();
    assert_eq!(said.refused, None, "{}", said.text());
    assert!(said.opened.iter().any(|l| l.contains(&target)), "{}", said.text());
    assert!(said.cut_ends >= 1, "{}", said.text());
    let total = said.total.clone().expect("a total");
    if let Some(after) = &said.routes {
        // Kept: it costs no more, or it closes more connections than were open (the open
        // connection through the box is routed too).
        assert!(total.kept, "{}", said.text());
        assert_eq!(open(&board, after, &target), 0, "{}", said.text());
        let errors = drc_errors(&with_routes(&board, after).unwrap().0).unwrap();
        assert!(errors.is_empty(), "{errors:#?}");
    } else {
        assert!(!total.kept, "{}", said.text());
        assert!(said.text().contains("costs more"), "{}", said.text());
    }
}

#[test]
fn a_region_that_cannot_be_closed_writes_nothing_and_names_the_connection() {
    let board = board();
    let (target, knot, x) = nets(&board);
    let first = route_net(&board, &Routes::default(), &Ask::of(&board, &target, &[]).unwrap())
        .unwrap();
    let mut routes = first.routes.clone().expect("the net routes");
    // A locked wall across the board, on both layers, where the net's copper crosses it.
    with_record(&mut routes, &board, &knot, wall(&board, &knot, x, true), false);
    let said = route_region(&board, &routes, &ask(band(&board, x, 0.0))).unwrap();
    let refused = said.refused.clone().expect("refused");
    assert!(said.routes.is_none(), "{}", said.text());
    assert!(refused.contains("cannot be closed"), "{refused}");
    assert!(said.left.iter().any(|l| l.contains(&target)), "{}", said.text());
    assert!(refused.contains(&format!("net {target}")), "{refused}");
    // The locked wall is not cut.
    assert!(said.nets.iter().all(|n| n.net != knot || n.cut_tracks + n.removed_tracks == 0));
}

#[test]
fn locked_copper_inside_the_box_is_never_touched() {
    let board = board();
    let (_, knot, x) = nets(&board);
    let mut routes = Routes::default();
    with_record(&mut routes, &board, &knot, wall(&board, &knot, x, true), false);
    let said = route_region(&board, &routes, &ask(band(&board, x, 0.15))).unwrap();
    assert!(said.nets.iter().all(|n| n.cut_tracks + n.removed_tracks + n.removed_vias == 0));
    // The router may add copper to the knot's net (one of its connections runs through the
    // box), but its locked tracks are all there as they were.
    if let Some(after) = &said.routes {
        let kept = &after.boards[&board.name].nets[&knot].tracks;
        for t in &routes.boards[&board.name].nets[&knot].tracks {
            assert!(kept.contains(t), "{t:?} is gone: {kept:?}");
        }
    }
    // A locked record, its shapes unlocked, is not touched either.
    let mut routes = Routes::default();
    with_record(&mut routes, &board, &knot, wall(&board, &knot, x, false), true);
    let said = route_region(&board, &routes, &ask(band(&board, x, 0.15))).unwrap();
    assert!(said.nets.iter().all(|n| n.cut_tracks + n.removed_tracks == 0), "{}", said.text());
}

#[test]
fn a_box_with_no_area_is_refused() {
    let board = board();
    let said = route_region(&board, &Routes::default(), &ask([1.0, 1.0, 0.0, 2.0])).unwrap();
    assert!(said.refused.unwrap().contains("no area"));
}
