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

/// Issue #1488: the order after a try is the nets not put back first, then the rest; an order
/// tried is not tried again, and with no order left there is no next try.
#[test]
fn the_next_order_puts_the_nets_not_put_back_first_and_never_repeats() {
    use netsmith_fabricate::route_net::next_order;
    let v = |s: &[&str]| s.iter().map(ToString::to_string).collect::<Vec<_>>();
    let order = v(&["A", "B", "C"]);
    let tried = vec![order.clone()];
    assert_eq!(next_order(&order, &v(&["C"]), &tried), Some(v(&["C", "A", "B"])));
    // A net ripped in turn that was not put back goes first too.
    assert_eq!(next_order(&order, &v(&["D"]), &tried), Some(v(&["D", "A", "B", "C"])));
    // The first net failed again: that order is the one tried, so the order turns by one.
    assert_eq!(next_order(&order, &v(&["A"]), &tried), Some(v(&["B", "C", "A"])));
    let all = vec![order.clone(), v(&["B", "C", "A"]), v(&["C", "A", "B"])];
    assert_eq!(next_order(&order, &v(&["A"]), &all), None);
    // One net has one order.
    assert_eq!(next_order(&v(&["A"]), &v(&["A"]), &[v(&["A"])]), None);
}

/// Tracks of `net` from the top edge to the bottom on both layers at `x` on the board, in the
/// routes file's places.
fn wall_at(board: &Board, net: &str, x: f64) -> Vec<RouteTrack> {
    let x = x - board.outline.min.x;
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
            lock: false,
        })
        .collect()
}

/// Issue #1488: a ripped net is put back only by ripping a third net in turn. The net asked
/// for (the one whose pads are nearest along x) is walled off by the copper of a net that is
/// joined (its pads are on both sides), and that net's pads are walled apart by the third net's
/// tracks, which leave the asked net's pads and the third net's own on one side. At
/// `--rip-depth 0` the ripped net may not rip, the one order there is is tried once and the
/// refusal names the depth; at the default depth it rips the third net at depth 2 and every net
/// is put back.
#[test]
fn a_ripped_net_rips_in_turn_to_the_depth_asked_and_the_tries_are_counted() {
    let board = board();
    let span = |n: &str| {
        let p = pads_of(&board, n);
        let xs = p.iter().map(|p| p.x);
        (xs.clone().fold(f64::MAX, f64::min), xs.fold(f64::MIN, f64::max))
    };
    let mut names: Vec<String> = board.nets.iter().map(|n| n.name.clone()).collect();
    names.sort_by(|a, b| {
        let w = |n: &str| span(n).1 - span(n).0;
        w(a).total_cmp(&w(b))
    });
    let target = names[0].clone();
    let (t0, t1) = span(&target);
    let wall = names
        .iter()
        .find(|n| span(n).0 < t0 && span(n).1 > t1)
        .expect("a net whose pads are on both sides of the asked net's")
        .clone();
    let third = names
        .iter()
        .find(|n| **n != target && **n != wall)
        .unwrap()
        .clone();
    let (w0, _) = span(&wall);
    let x = (w0 + t0) / 2.0;
    assert!(span(&third).0 > x, "the third net's pads are on one side of its wall");

    // The wall's net joined, then its wall between the asked net's pads, then the third
    // net's wall between the wall's net's pads.
    let joined = route_net(&board, &Routes::default(), &ask(&board, &wall, &[])).unwrap();
    let mut routes = joined
        .routes
        .clone()
        .unwrap_or_else(|| panic!("{}", joined.text()));
    let table = &mut routes.boards.get_mut(&board.name).unwrap().nets;
    table
        .get_mut(&wall)
        .unwrap()
        .tracks
        .extend(wall_at(&board, &wall, (t0 + t1) / 2.0));
    table.insert(
        third.clone(),
        NetRoutes {
            pads: pad_hash(&board, &third).unwrap(),
            tracks: wall_at(&board, &third, x),
            ..NetRoutes::default()
        },
    );
    assert_eq!(open(&board, &routes, &wall), 0);

    let mut shallow = ask(&board, &target, &[&wall, &third]);
    shallow.depth = 0;
    let said = route_net(&board, &routes, &shallow).unwrap();
    assert!(said.routes.is_none(), "{}", said.text());
    assert_eq!(said.in_the_way, vec![wall.clone()], "{}", said.text());
    assert_eq!(said.tries.len(), 1, "one order of one net: {}", said.text());
    assert_eq!(said.tries[0].depth_stopped, vec![wall.clone()], "{}", said.text());
    assert_eq!(said.tries[0].failed, vec![wall.clone()], "{}", said.text());
    let why = said.refused.clone().unwrap();
    assert!(why.contains("`--rip-depth 1`"), "{why}");
    assert!(said.runs <= netsmith_fabricate::route_net::MOST_RUNS * shallow.tries);

    let said = route_net(&board, &routes, &ask(&board, &target, &[&wall, &third])).unwrap();
    assert_eq!(said.refused, None, "{}", said.text());
    assert_eq!(said.tries.len(), 1, "{}", said.text());
    let rips = &said.tries[0].ripped;
    assert_eq!(rips[0].net, wall);
    assert_eq!((rips[0].by.as_str(), rips[0].depth), (target.as_str(), 1));
    let deeper = rips
        .iter()
        .find(|r| r.net == third)
        .unwrap_or_else(|| panic!("{rips:?}"));
    assert_eq!((deeper.by.as_str(), deeper.depth), (wall.as_str(), 2));
    let after = said.routes.clone().unwrap();
    for n in [&target, &wall, &third] {
        assert_eq!(open(&board, &after, n), 0, "{n}: {}", said.text());
    }
    let text = said.text();
    assert!(text.contains(&format!("{third} (by {wall}, depth 2)")), "{text}");
    assert!(text.contains("every ripped net put back"), "{text}");
}

/// The copper `routes route` lays takes the board's corner style (issue #1508): rounded, the
/// default, its bends are arcs where they keep the rules' space, as `fab` rounds the router's
/// (it never rounds a routes file's); mitered (`.corner`, or `--corner mitered` for one run),
/// every bend keeps its 45 degree corner. Either way the check finds nothing.
#[test]
fn the_copper_laid_takes_the_boards_corner_style() {
    let rounded = board();
    let (target, _) = nets(&rounded);
    let routes = Routes::default();
    let arcs = |board: &Board| -> usize {
        let said = route_net(board, &routes, &ask(board, &target, &[])).unwrap();
        assert_eq!(said.refused, None, "{}", said.text());
        let after = said.routes.unwrap();
        let errors = drc_errors(&with_routes(board, &after).unwrap().0).unwrap();
        assert!(errors.is_empty(), "{errors:#?}");
        assert_eq!(open(board, &after, &target), 0);
        after.boards[&board.name].nets[&target]
            .tracks
            .iter()
            .filter(|t| t.mid.is_some())
            .count()
    };
    assert!(arcs(&rounded) > 0, "a rounded board's bends are arcs");
    let mut mitered = rounded.clone();
    mitered.corner = netsmith_core::TrackCorner::Mitered;
    assert_eq!(arcs(&mitered), 0, "a mitered board's bends keep their corners");
}

/// `--reroute` (issue #1538): a net that went round a wall of another net's copper, with a gap
/// at the bottom, is laid again. With the wall kept it finds the same long way: not shorter, so
/// nothing is written. With the wall's net ripped, the net is laid first, straight, the wall's
/// net after it, and the total by the weights is shorter: the change is written with every
/// connection made.
#[test]
fn reroute_lays_a_net_and_the_nets_ripped_again_and_keeps_it_only_when_shorter() {
    let board = board();
    let (target, wall) = nets(&board);
    let mut routes = walled(&board, &target, &wall, false);
    let h = board.outline.height();
    for t in &mut routes.boards.get_mut(&board.name).unwrap().nets.get_mut(&wall).unwrap().tracks {
        t.to[1] = h * 0.7;
    }
    let first = route_net(&board, &routes, &ask(&board, &target, &[])).unwrap();
    assert_eq!(first.refused, None, "{}", first.text());
    let looped = first.routes.clone().expect("the net goes round the wall");
    assert_eq!(open(&board, &looped, &target), 0);

    let alone = Ask {
        reroute: true,
        ..ask(&board, &target, &[])
    };
    let said = route_net(&board, &looped, &alone).unwrap();
    let total = said.rerouted.clone().expect("a total");
    assert!(!total.kept, "{}", said.text());
    assert!(said.routes.is_none(), "{}", said.text());
    assert!(said.text().contains("not shorter"), "{}", said.text());

    let with_wall = Ask {
        reroute: true,
        ..ask(&board, &target, &[&wall])
    };
    let said = route_net(&board, &looped, &with_wall).unwrap();
    let total = said.rerouted.clone().expect("a total");
    assert!(total.kept, "{}", said.text());
    assert!(total.after < total.before - netsmith_fabricate::route_net::REROUTE_GAIN);
    assert_eq!(said.ripped, vec![wall.clone()]);
    let after = said.routes.clone().expect("written");
    assert_eq!(open(&board, &after, &target), 0, "{}", said.text());
    assert!(
        open(&board, &after, &wall) <= open(&board, &looped, &wall),
        "{}",
        said.text()
    );
    assert!(said.runs <= netsmith_fabricate::route_net::MOST_RUNS * with_wall.tries);

    // One connection is not laid again alone.
    let pads: Vec<String> = board.nets.iter().find(|n| n.name == target).unwrap().pads.iter().map(ToString::to_string).collect();
    let connection = Ask {
        reroute: true,
        ..ask(&board, &format!("{target}:{}-{}", pads[0], pads[1]), &[])
    };
    let said = route_net(&board, &looped, &connection).unwrap();
    assert!(said.refused.unwrap().contains("whole"));
}

/// Issue #1642: `--via` routes a net through a via on the far side of another net's locked
/// tracks (a wall from the top edge down past the net's pads, on both layers): the via is added,
/// both pads are joined through it, and nothing the check finds is new. Asked again, the via is
/// the routes file's and the net is joined already. A via on the wall is refused, naming its net.
#[test]
fn a_net_is_routed_through_a_via_on_the_far_side_of_an_obstacle() {
    use netsmith_fabricate::route_net::ViaAsked;
    let board = board();
    let (target, wall) = nets(&board);
    let p = pads_of(&board, &target);
    let min = board.outline.min;
    let x = (p[0].x + p[1].x) / 2.0 - min.x;
    let edge = board.rules.copper_to_edge + 0.2;
    let below = (p[0].y.max(p[1].y) - min.y + 1.5).min(board.outline.height() - edge - 2.0);
    let width = board.track_width(&wall);
    let track = |layer| RouteTrack {
        layer,
        width,
        from: [x, edge],
        mid: None,
        to: [x, below],
        lock: true,
    };
    let mut routes = Routes::default();
    routes.boards.entry(board.name.clone()).or_default().nets.insert(
        wall.clone(),
        NetRoutes {
            pads: pad_hash(&board, &wall).unwrap(),
            lock: false,
            tracks: vec![track(CopperLayer::Front), track(CopperLayer::Back)],
            vias: Vec::new(),
            zones: Vec::new(),
        },
    );
    // The far side of the wall from the first pad, near the top edge.
    let side = if p[0].x < p[1].x { 1.5 } else { -1.5 };
    let at = [x + side, edge + 1.0];
    let mut asked = ask(&board, &target, &[]);
    asked.through = vec![ViaAsked { at, layers: None }];
    let said = route_net(&board, &routes, &asked).unwrap();
    assert_eq!(said.refused, None, "{}", said.text());
    let after = said.routes.clone().unwrap();
    assert_eq!(open(&board, &after, &target), 0, "{}", said.text());
    let errors = drc_errors(&with_routes(&board, &after).unwrap().0).unwrap();
    assert!(errors.is_empty(), "{errors:#?}");
    assert_eq!(said.through.len(), 1);
    assert!(said.through[0].added);
    assert_eq!(said.through[0].pads.len(), 2, "{}", said.text());
    // Each pad was a run of its own, and the via joins the net's tracks.
    assert_eq!(said.runs, 2, "{}", said.text());
    let record = &after.boards[&board.name].nets[&target];
    let via = record
        .vias
        .iter()
        .find(|v| (v.at[0] - at[0]).hypot(v.at[1] - at[1]) < 1e-6)
        .expect("the via is in the record");
    let ends = record
        .tracks
        .iter()
        .flat_map(|t| [t.from, t.to])
        .filter(|e| (e[0] - at[0]).hypot(e[1] - at[1]) < via.diameter / 2.0)
        .count();
    assert!(ends >= 2, "tracks end in the via from both pads: {record:#?}");
    // The wall is untouched.
    assert_eq!(after.boards[&board.name].nets[&wall], routes.boards[&board.name].nets[&wall]);
    let text = said.text();
    assert!(text.contains("added: pad(s)"), "{text}");

    // Asked again: the via is the routes file's, and the net is joined through it already.
    let again = route_net(&board, &after, &asked).unwrap();
    assert!(again.joined, "{}", again.text());
    assert!(!again.through[0].added);
    assert_eq!(again.runs, 0);

    // A via on the wall is refused, and what blocks it is named.
    asked.through = vec![ViaAsked {
        at: [x, edge + 1.0],
        layers: None,
    }];
    let said = route_net(&board, &routes, &asked).unwrap();
    let why = said.refused.clone().expect("a via on another net's track");
    assert!(why.contains("refused") && why.contains(&wall), "{why}");
    assert!(said.routes.is_none());
    assert_eq!(said.runs, 0);
}
