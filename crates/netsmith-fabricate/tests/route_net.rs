//! `netsmith routes route` (issue #1195, `netsmith_fabricate::route_net`): a net whose only way
//! is through another net's track names that net, `--rip` routes both with no connection open
//! and no new finding, a locked net or shape is never ripped, and one connection is routed alone.
//! The board is the LED example with a wall of another net's tracks across it, on both layers,
//! between the two pads of a net.

use std::collections::BTreeSet;

use netsmith_fabricate::route_net::{Ask, route_net};
use netsmith_fabricate::routes::{Refusal, drc_errors, with_routes};
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

/// The centre of each pad of `net`.
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

/// The net of two pads on two parts whose pads are farthest apart along x, and the other net
/// of the board that will be the wall.
fn nets(board: &Board) -> (String, String) {
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
    let wall = if target == "GND" { "VCC" } else { "GND" };
    (target, wall.to_string())
}

/// The routes file with a wall of `wall`'s tracks from the top edge to the bottom on both
/// layers, half way between the pads of `target`, and the wall's `x` in the file's places.
fn walled(board: &Board, target: &str, wall: &str, lock: bool) -> Routes {
    let p = pads_of(board, target);
    let x = (p[0].x + p[1].x) / 2.0 - board.outline.min.x;
    let edge = board.rules.copper_to_edge + 0.2;
    let h = board.outline.height();
    let width = board.track_width(wall);
    let track = |layer| RouteTrack {
        layer,
        width,
        from: [x, edge],
        mid: None,
        to: [x, h - edge],
        lock,
    };
    let mut routes = Routes::default();
    routes.boards.entry(board.name.clone()).or_default().nets.insert(
        wall.to_string(),
        NetRoutes {
            pads: pad_hash(board, wall).unwrap(),
            lock: false,
            tracks: vec![track(CopperLayer::Front), track(CopperLayer::Back)],
            vias: Vec::new(),
            zones: Vec::new(),
        },
    );
    routes
}

fn open(board: &Board, routes: &Routes, net: &str) -> usize {
    let (b, _) = with_routes(board, routes).unwrap();
    Ratsnest::of(&b).lines.iter().filter(|l| l.net == net).count()
}

fn ask(board: &Board, target: &str, rip: &[&str]) -> Ask {
    let rip: Vec<String> = rip.iter().map(ToString::to_string).collect();
    Ask::of(board, target, &rip).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_net_whose_way_crosses_another_nets_track_names_it_and_rip_routes_both() {
    let board = board();
    let (target, wall) = nets(&board);
    let routes = walled(&board, &target, &wall, false);
    // The wall is a fair start: it adds no error of the check, and it walls the net in.
    assert!(
        drc_errors(&with_routes(&board, &routes).unwrap().0)
            .unwrap()
            .is_empty(),
        "the wall itself breaks a rule"
    );

    let said = route_net(&board, &routes, &ask(&board, &target, &[])).unwrap();
    assert_eq!(said.in_the_way, vec![wall.clone()], "{}", said.text());
    assert!(said.refused.as_ref().unwrap().contains("--rip"), "{said:?}");
    assert!(said.routes.is_none());

    let said = route_net(&board, &routes, &ask(&board, &target, &[&wall])).unwrap();
    assert_eq!(said.refused, None, "{}", said.text());
    assert_eq!(said.ripped, vec![wall.clone()]);
    let after = said.routes.clone().unwrap();
    assert_eq!(open(&board, &after, &target), 0, "{}", said.text());
    assert_eq!(open(&board, &after, &wall), 0, "{}", said.text());
    let errors = drc_errors(&with_routes(&board, &after).unwrap().0).unwrap();
    assert!(errors.is_empty(), "{errors:#?}");
    // The wall's two tracks went, and the copper of both nets is the file's, unlocked.
    assert_eq!(said.nets[0].net, target);
    assert!(!said.nets[0].tracks.is_empty());
    assert_eq!(said.nets[1].removed_tracks, 2);
    let table = &after.boards[&board.name].nets;
    assert!(table[&target].tracks.iter().all(|t| !t.lock));
    assert!(said.runs >= 2 && said.runs <= netsmith_fabricate::route_net::MOST_RUNS);
    let c = said.narrowest.clone().unwrap();
    assert!(c.fits >= c.width, "{c:?}");
    let text = said.text();
    assert!(text.contains(&format!("in the way of net {target}")), "{text}");
    assert!(text.contains("the narrowest place of its way"), "{text}");
}

/// Issue #1315: a net a pour joins is never suggested for `--rip`, asking to rip it is refused
/// with what is true (it has no copper to rip), and no run of the router is spent taking its
/// copper away.
#[test]
fn a_poured_net_is_neither_suggested_nor_ripped_and_costs_no_run() {
    let mut board = board();
    let (target, wall) = nets(&board);
    let routes = walled(&board, &target, &wall, false);
    let plain = route_net(&board, &routes, &ask(&board, &target, &[])).unwrap();
    assert_eq!(plain.in_the_way, vec![wall.clone()], "{}", plain.text());

    board.pours.push(netsmith_pcb::BoardPour {
        layer: CopperLayer::Front,
        net: wall.clone(),
        stitch_pitch: None,
        thermal_gap: None,
        spoke_width: None,
        clearance: None,
        min_width: None,
        pads: None,
    });
    let said = route_net(&board, &routes, &ask(&board, &target, &[])).unwrap();
    let text = said.refused.clone().unwrap();
    assert!(said.in_the_way.is_empty(), "{said:?}");
    assert!(!text.contains("--rip"), "{text}");
    // One run to route the net, none to look for nets to rip.
    assert_eq!(said.runs, 1, "{said:?}");
    assert!(plain.runs > said.runs, "{} runs without a pour", plain.runs);

    let rip = vec![wall.clone()];
    match Ask::of(&board, &target, &rip) {
        Err(Refusal::Refused(m)) => {
            assert!(m.contains(&wall) && m.contains("no copper to rip"), "{m}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_locked_net_or_shape_is_never_ripped() {
    let board = board();
    let (target, wall) = nets(&board);
    // The wall's net locked.
    let mut routes = walled(&board, &target, &wall, false);
    routes.boards.get_mut(&board.name).unwrap().nets.get_mut(&wall).unwrap().lock = true;
    let said = route_net(&board, &routes, &ask(&board, &target, &[&wall])).unwrap();
    assert!(said.refused.is_some(), "{}", said.text());
    assert!(said.routes.is_none());
    assert!(said.ripped.is_empty());
    assert!(
        said.walls.iter().any(|w| w.nets.contains(&wall)),
        "the locked net is named in the wall: {said:?}"
    );
    // The wall's tracks locked in an unlocked record.
    let routes = walled(&board, &target, &wall, true);
    let said = route_net(&board, &routes, &ask(&board, &target, &[&wall])).unwrap();
    assert!(said.refused.is_some(), "{}", said.text());
    assert!(said.routes.is_none());
    // The net asked for, locked, gets no copper.
    let mut routes = Routes::default();
    routes.boards.entry(board.name.clone()).or_default().nets.insert(
        target.clone(),
        NetRoutes {
            pads: pad_hash(&board, &target).unwrap(),
            lock: true,
            ..NetRoutes::default()
        },
    );
    let said = route_net(&board, &routes, &ask(&board, &target, &[])).unwrap();
    assert!(said.refused.unwrap().contains("locked"));
    assert_eq!(said.runs, 0);
}

#[test]
fn one_connection_is_routed_alone_and_then_is_joined() {
    let board = board();
    let n = board.nets.iter().find(|n| n.name == "GND").unwrap();
    assert!(n.pads.len() >= 2);
    let (a, b) = (&n.pads[0], &n.pads[1]);
    let target = format!("GND:{a}-{b}");
    let asked = ask(&board, &target, &[]);
    assert_eq!(asked.connection, Some([a.clone(), b.clone()]));
    let said = route_net(&board, &Routes::default(), &asked).unwrap();
    assert_eq!(said.refused, None, "{}", said.text());
    assert_eq!(said.connection, Some(format!("{a} to {b}")));
    let after = said.routes.unwrap();
    assert!(after.boards[&board.name].nets.contains_key("GND"));
    let again = route_net(&board, &after, &asked).unwrap();
    assert!(again.joined, "{}", again.text());
    assert!(again.routes.is_none());
    assert_eq!(again.runs, 0);
}

#[test]
fn an_ask_names_a_net_and_two_of_its_pads() {
    let board = board();
    let refused = |target: &str, rip: &[&str]| {
        let rip: Vec<String> = rip.iter().map(ToString::to_string).collect();
        match Ask::of(&board, target, &rip) {
            Err(Refusal::Refused(e)) => e,
            other => panic!("{target}: {other:?}"),
        }
    };
    assert!(refused("NOPE", &[]).contains("no net `NOPE`"));
    assert!(refused("GND:X9.1-X9.2", &[]).contains("not two pads of net GND"));
    assert!(refused("GND", &["GND"]).contains("the net to route"));
    assert!(refused("GND", &["NOPE"]).contains("no net `NOPE`"));
    let asked = ask(&board, "GND", &["VCC"]);
    assert_eq!(asked.connection, None);
    assert_eq!(asked.rip, BTreeSet::from(["VCC".to_string()]));
}
