//! Tests of the orders, the coarse plan and the stages that follow a try.

use crate::testlib::{fixtures, four_layer_design};

use super::*;

fn led_board() -> Board {
    crate::board_from_design(
        &crate::testlib::led_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap()
}

#[test]
fn the_first_try_keeps_the_first_order_and_later_tries_keep_the_classes_in_turn() {
    let board = led_board();
    let options = RouteOptions::default();
    let mut router = Router::new(&board, &options);
    assert!(router.lines.len() >= 3);
    // The classes most important last in the plain order, so that putting them in turn
    // moves every line.
    let plain = router.orders(true);
    let n = router.lines.len();
    let mut class = vec![Class::Rest; n];
    let order = [
        Class::Escape,
        Class::Decoupling,
        Class::Pair,
        Class::Clock,
        Class::Power,
    ];
    for (k, &l) in plain[0].iter().rev().enumerate() {
        class[l] = order[k.min(order.len() - 1)];
    }
    router.class = class;
    let tries = router.tries(&options);
    assert_eq!(tries[0], plain[0], "the first try is the router's own");
    assert_eq!(
        tries[1],
        router.classed_order(&plain[0]),
        "then the first order by class"
    );
    assert_ne!(tries[1], tries[0]);
    for t in &tries[1..] {
        let classes: Vec<Class> = t.iter().map(|&l| router.class[l]).collect();
        assert!(classes.is_sorted(), "{classes:?}");
    }
    // Inside a class a line keeps the order it had.
    router.class = vec![Class::Rest; n];
    assert_eq!(router.classed_order(&plain[0]), plain[0]);
    // With no classes the tries are the plain ones; with classes on every try, the first
    // is by class too.
    let off = RouteOptions {
        classes: LineClasses::Off,
        ..RouteOptions::default()
    };
    assert_eq!(router.tries(&off)[0], plain[0]);
}

#[test]
fn the_fixed_order_of_the_classes_is_escape_decoupling_pair_clock_power_rest() {
    let fixed = [
        Class::Escape,
        Class::Decoupling,
        Class::Pair,
        Class::Clock,
        Class::Power,
        Class::Rest,
    ];
    assert!(fixed.is_sorted());
    let protected: Vec<Class> = fixed.into_iter().filter(|c| c.protected()).collect();
    assert_eq!(protected, [Class::Decoupling, Class::Pair, Class::Clock]);
}

/// A state of the LED board where only line `a` is routed: a track of its net across the
/// middle of line `b`, of another net.
fn across(router: &Router, a: usize, b: usize) -> State {
    let (from, to) = (router.lines[b].from_at, router.lines[b].to_at);
    let mid = Pt::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = (dx * dx + dy * dy).sqrt();
    let (nx, ny) = (-dy / len, dx / len);
    // On both layers and right across the board, so that no way round is cheaper.
    let track = |layer| Track {
        net: router.lines[a].net.clone(),
        layer,
        from: Pt::new(mid.x - nx * 50.0, mid.y - ny * 50.0),
        to: Pt::new(mid.x + nx * 50.0, mid.y + ny * 50.0),
        width: 0.2,
        mid: None,
    };
    State {
        routed: vec![Routed {
            line: a,
            tracks: vec![track(CopperLayer::Front), track(CopperLayer::Back)],
            vias: Vec::new(),
        }],
        ..State::default()
    }
}

#[test]
fn a_decoupling_line_is_not_ripped_up_for_a_line_of_a_later_class() {
    let board = led_board();
    let mut router = Router::new(&board, &RouteOptions::default());
    // Two lines of different nets, neither to a plane.
    let (a, b) = (0..router.lines.len())
        .flat_map(|a| (0..router.lines.len()).map(move |b| (a, b)))
        .find(|&(a, b)| {
            router.nets[a] != router.nets[b]
                && !router.lines[a].to_plane
                && !router.lines[b].to_plane
        })
        .unwrap();
    let state = across(&router, a, b);
    let mut class = vec![Class::Rest; router.lines.len()];
    class[a] = Class::Decoupling;
    router.class = class.clone();
    let names = |router: &Router| {
        let found = router.contested(b, &state, ROOM_LOOSE).map(|(_, n)| n);
        let chain = router
            .contested_lines(b, &state, ROOM_LOOSE, &[])
            .map(|l| l.iter().map(|&(l, _)| l).collect::<Vec<_>>());
        (found, chain)
    };
    // A try without classes rips up whatever is in the way.
    router.classed.set(false);
    let (found, chain) = names(&router);
    assert!(found.unwrap().contains(&router.nets[a]));
    assert!(chain.unwrap().contains(&a));
    // A try with them never names the decoupling line for a later one.
    router.classed.set(true);
    assert_eq!(router.protected(b, &state), [a]);
    let (found, chain) = names(&router);
    assert!(!found.unwrap_or_default().contains(&router.nets[a]));
    assert!(!chain.unwrap_or_default().contains(&a));
    // A power line keeps its place in the order only: it may be ripped up.
    class[a] = Class::Power;
    router.class = class.clone();
    assert!(router.protected(b, &state).is_empty());
    assert!(names(&router).0.unwrap().contains(&router.nets[a]));
    // And a decoupling line is ripped up for one of its own class or a more important one.
    class[a] = Class::Decoupling;
    class[b] = Class::Escape;
    router.class = class;
    assert!(router.protected(b, &state).is_empty());
}

#[test]
fn a_board_that_routes_on_its_first_try_has_the_same_copper_with_or_without_classes() {
    for design in [crate::testlib::led_design(), four_layer_design()] {
        let route_once = |classes| {
            let mut board = crate::board_from_design(
                &design,
                &netsmith_kicad::Mapping::standard(),
                &fixtures(),
            )
            .unwrap();
            let routing = route_with(
                &mut board,
                RouteOptions {
                    classes,
                    ..RouteOptions::default()
                },
            );
            assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
            assert_eq!(routing.tries, 1);
            (board.tracks, board.vias)
        };
        assert_eq!(
            route_once(LineClasses::Off),
            route_once(LineClasses::FromSecondTry)
        );
    }
}

#[test]
fn the_coarse_plan_gives_each_line_a_corridor_round_its_ends_and_is_the_same_every_time() {
    let board = crate::board_from_design(
        &crate::testlib::led_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap();
    let options = RouteOptions {
        plan: CoarsePlan::EveryTry,
        ..RouteOptions::default()
    };
    let router = Router::new(&board, &options);
    let order = router.orders(false).remove(0);
    let plan = router.make_plan(&order);
    let tiles = &plan.tiles;
    // Every edge holds a track somewhere: the board is not walled.
    assert!(tiles.across_x[FRONT].iter().any(|&c| c > 0));
    assert!(plan.corridors.iter().any(Option::is_some));
    for (line, corridor) in plan.corridors.iter().enumerate() {
        // A line to a plane, or one wider than the narrowest, has none.
        let Some(inside) = corridor else { continue };
        for &(i, j, _) in router.ends[line].0.iter().chain(&router.ends[line].1) {
            let (a, b) = tiles.of(i, j);
            assert!(
                inside[0][b * tiles.nx + a] || inside[1][b * tiles.nx + a],
                "line {line}"
            );
        }
    }
    let again = Router::new(&board, &options).make_plan(&order);
    assert_eq!(plan.corridors, again.corridors);
}

#[test]
fn a_line_of_the_pours_net_or_of_a_diff_pair_has_no_corridor() {
    let mut board = crate::board_from_design(
        &crate::testlib::led_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap();
    let options = RouteOptions {
        plan: CoarsePlan::EveryTry,
        ..RouteOptions::default()
    };
    let router = Router::new(&board, &options);
    let order = router.orders(false).remove(0);
    let plan = router.make_plan(&order);
    let mut steered: Vec<usize> = (0..router.lines.len())
        .filter(|&l| plan.corridors[l].is_some())
        .map(|l| router.nets[l])
        .collect();
    steered.dedup();
    assert!(steered.len() >= 3, "{steered:?}");
    let (poured, paired) = (steered[0], steered[1]);
    board.pours.push(netsmith_pcb::board::BoardPour {
        layer: netsmith_pcb::board::CopperLayer::Back,
        net: board.nets[poured].name.clone(),
        stitch_pitch: None,
    });
    let mut router = Router::new(&board, &options);
    router.partners[paired] = Some(poured);
    let plan = router.make_plan(&order);
    for (l, corridor) in plan.corridors.iter().enumerate() {
        let net = router.nets[l];
        if net == poured || net == paired {
            assert!(corridor.is_none(), "line {l}");
        }
    }
    assert!(
        (0..router.lines.len())
            .any(|l| router.nets[l] == steered[2] && plan.corridors[l].is_some())
    );
}

#[test]
fn a_board_routed_with_the_coarse_plan_is_complete_and_the_same_every_time() {
    let route_once = || {
        let mut board = crate::board_from_design(
            &crate::testlib::led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let routing = route_with(
            &mut board,
            RouteOptions {
                plan: CoarsePlan::EveryTry,
                ..RouteOptions::default()
            },
        );
        assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
        (board.tracks, board.vias)
    };
    assert_eq!(route_once(), route_once());
}

/// The LED board with its two signal nets named `names` and `GND` poured on the back, routed
/// on the front, with each signal line's copper then moved to the back between two vias at
/// its pads, as a try might leave it; and what the pass off the pours makes of it.
fn off_the_pours_of_the_led_board(names: [&str; 2]) -> (Board, Vec<Routed>) {
    let mut design = crate::testlib::led_design();
    design.nets[1].name = names[0].into();
    design.nets[2].name = names[1].into();
    let mut board =
        crate::board_from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures())
            .unwrap();
    board.pours.push(netsmith_pcb::BoardPour {
        layer: CopperLayer::Back,
        net: "GND".into(),
        stitch_pitch: None,
    });
    let router = Router::new(&board, &RouteOptions::default());
    let order = router.orders(false).remove(0);
    let mut state = router.run_stages(&order, true, 1, 1);
    assert!(state.failed.is_empty());
    for r in &mut state.routed {
        let line = &router.lines[r.line];
        if !names.contains(&line.net.as_str()) {
            continue;
        }
        let width = r.tracks[0].width;
        let rules = &board.rules;
        r.tracks = vec![Track {
            net: line.net.clone(),
            layer: CopperLayer::Back,
            from: line.from_at,
            to: line.to_at,
            width,
            mid: None,
        }];
        r.vias = [line.from_at, line.to_at]
            .map(|at| Via {
                net: line.net.clone(),
                at,
                diameter: rules.via_diameter,
                drill: rules.via_drill,
            })
            .to_vec();
    }
    router.off_the_pours(&mut state, 1, 1);
    let routed = state.routed.clone();
    drop(router);
    (board, routed)
}

#[test]
fn a_line_on_another_nets_pour_is_moved_off_it_after_a_complete_try() {
    let (_, routed) = off_the_pours_of_the_led_board(["VCC", "net-led.anode"]);
    for r in routed.iter().filter(|r| r.tracks[0].net != "GND") {
        assert!(
            r.tracks.iter().all(|t| t.layer == CopperLayer::Front) && r.vias.is_empty(),
            "{} is still on the pour: {:?} {:?}",
            r.tracks[0].net,
            r.tracks,
            r.vias
        );
    }
}

#[test]
fn the_nets_of_a_diff_pair_are_not_moved_off_the_pour() {
    let (_, routed) = off_the_pours_of_the_led_board(["USB_P", "USB_N"]);
    for r in routed.iter().filter(|r| r.tracks[0].net != "GND") {
        assert!(
            r.tracks.iter().all(|t| t.layer == CopperLayer::Back) && r.vias.len() == 2,
            "{} moved: {:?}",
            r.tracks[0].net,
            r.tracks
        );
    }
}

/// The line of `net-led.anode` on the LED board, routed in `state`.
fn anode(router: &Router, state: &State) -> Routed {
    state
        .routed
        .iter()
        .find(|r| router.lines[r.line].net == "net-led.anode")
        .unwrap()
        .clone()
}

#[test]
fn a_line_that_changed_layer_to_get_round_copper_no_longer_there_loses_its_vias() {
    let board = led_board();
    let router = Router::new(&board, &RouteOptions::default());
    let order = router.orders(false).remove(0);
    let mut state = router.run_stages(&order, true, 1, 1);
    assert!(state.failed.is_empty());
    // The anode's line dropped to the back and came up again, round copper since moved.
    let line = anode(&router, &state).line;
    let r = state.routed.iter_mut().find(|r| r.line == line).unwrap();
    let (from, to) = (router.lines[line].from_at, router.lines[line].to_at);
    let mid = Pt::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0 + 3.0);
    let via = |at| Via {
        net: "net-led.anode".into(),
        at,
        diameter: board.rules.via_diameter,
        drill: board.rules.via_drill,
    };
    let track = |layer, a, b| Track {
        net: "net-led.anode".into(),
        layer,
        from: a,
        to: b,
        width: r.tracks[0].width,
        mid: None,
    };
    let (a, b) = (Pt::new(from.x, from.y + 3.0), Pt::new(to.x, to.y + 3.0));
    r.tracks = vec![
        track(CopperLayer::Front, from, a),
        track(CopperLayer::Back, a, mid),
        track(CopperLayer::Back, mid, b),
        track(CopperLayer::Front, b, to),
    ];
    r.vias = vec![via(a), via(b)];
    let others: Vec<Routed> = state
        .routed
        .iter()
        .filter(|r| r.line != line)
        .cloned()
        .collect();
    router.fewer_vias(&mut state, 1, 1);
    let after = anode(&router, &state);
    assert!(after.vias.is_empty(), "{:?}", after.vias);
    assert!(
        after.tracks.iter().all(|t| t.layer == CopperLayer::Front),
        "{:?}",
        after.tracks
    );
    // No other line moved.
    for o in others {
        let now = state.routed.iter().find(|r| r.line == o.line).unwrap();
        assert_eq!((&now.tracks, &now.vias), (&o.tracks, &o.vias));
    }
}

/// The LED board routed by hand: a wall of the net `wall` across the line of
/// `net-led.anode` on the front only, and the anode's line under it by a via; and that state
/// after the pass that takes vias out, with the anode's copper before the pass.
fn under_a_wall(wall: &str, poured: bool) -> (Board, State, Routed, Routed) {
    let mut board = led_board();
    if poured {
        board.pours.push(netsmith_pcb::BoardPour {
            layer: CopperLayer::Back,
            net: wall.into(),
            stitch_pitch: None,
        });
    }
    let router = Router::new(&board, &RouteOptions::default());
    let line = |net: &str| (0..router.lines.len()).find(|&l| router.lines[l].net == net);
    let (a, b) = (line(wall).unwrap(), line("net-led.anode").unwrap());
    let mut state = across(&router, a, b);
    state.routed[0]
        .tracks
        .retain(|t| t.layer == CopperLayer::Front);
    router.fine.set(true);
    let (routed, _) = router.try_line(b, &state, &[], ROOM_LOOSE, false).unwrap();
    router.fine.set(false);
    assert!(!routed.vias.is_empty());
    state.routed.push(routed.clone());
    router.fewer_vias(&mut state, 1, 1);
    let after = anode(&router, &state);
    drop(router);
    (board, state, routed, after)
}

#[test]
fn a_line_whose_via_is_needed_keeps_it() {
    // The wall is ground, which the pass never moves (its vias join its pour).
    let (_, _, before, after) = under_a_wall("GND", true);
    assert_eq!((&after.tracks, &after.vias), (&before.tracks, &before.vias));
}

#[test]
fn a_line_in_the_way_is_moved_too_when_the_two_then_have_fewer_vias() {
    let (board, state, _, after) = under_a_wall("VCC", false);
    assert!(after.vias.is_empty(), "{:?}", after.vias);
    // The wall is routed again, shorter and with no via.
    let router = Router::new(&board, &RouteOptions::default());
    let vcc = state
        .routed
        .iter()
        .find(|r| router.lines[r.line].net == "VCC")
        .unwrap();
    let length: f64 = vcc.tracks.iter().map(|t| t.from.distance(t.to)).sum();
    assert!(
        vcc.vias.is_empty() && length < 50.0,
        "{vcc:?}",
        vcc = vcc.tracks
    );
    assert!(state.failed.is_empty() && state.routed.len() == 2);
}

#[test]
fn the_vias_of_planes_pours_and_decoupling_capacitors_are_never_taken_out() {
    // A capacitor's ground via to the pour, and every line of the poured net.
    let board = crate::decouple::tests::decoupled_board();
    let router = Router::new(&board, &RouteOptions::default());
    assert!(router.routed_lines < router.lines.len());
    for l in 0..router.lines.len() {
        let keep = l >= router.routed_lines || router.lines[l].net == "GND";
        assert_eq!(router.may_lose_vias(l), !keep, "{}", router.lines[l]);
    }
    // A line to a plane, and every line of a plane's net.
    let board = crate::board_from_design(
        &four_layer_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap();
    let router = Router::new(&board, &RouteOptions::default());
    assert!(router.lines.iter().any(|l| l.to_plane));
    for l in 0..router.lines.len() {
        let line = &router.lines[l];
        if line.to_plane || line.net == "GND" || line.net == "3V3" {
            assert!(!router.may_lose_vias(l), "{line}");
        }
    }
}

#[test]
fn the_four_layer_board_is_routed_with_a_via_to_the_plane_beside_each_surface_mount_pad() {
    let mut board = crate::board_from_design(
        &four_layer_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap();
    let left = route(&mut board);
    assert!(left.is_empty(), "{:?}", left.lines);
    // Tracks are on the two outer layers only: the type has no other.
    for (refdes, pad, net) in [("D1", "1", "GND"), ("R1", "1", "3V3")] {
        let part = board.part(refdes).unwrap();
        let at = part.pad_centre(part.footprint.pads_numbered(pad).next().unwrap());
        let via = board
            .vias
            .iter()
            .find(|v| v.net == net && v.at.distance(at) < 2.0)
            .unwrap_or_else(|| panic!("no {net} via beside {refdes}.{pad}"));
        // The via is not on the pad, and a track on the pad's layer joins them.
        assert!(via.at.distance(at) > 0.2, "{refdes}.{pad}");
        let touches = |p: Pt| {
            board.tracks.iter().any(|t| {
                t.net == net
                    && t.layer == CopperLayer::Front
                    && (t.from.distance(p) < 1e-6 || t.to.distance(p) < 1e-6)
            })
        };
        assert!(touches(at) && touches(via.at), "{refdes}.{pad}");
    }
    // The through-hole pads are on the planes already, so nothing else has a via.
    assert_eq!(board.vias.iter().filter(|v| v.net == "GND").count(), 1);
    assert_eq!(board.vias.iter().filter(|v| v.net == "3V3").count(), 1);
}
