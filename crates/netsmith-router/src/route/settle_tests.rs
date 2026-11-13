//! Tests of settling a path, the memo and the soft search.

use crate::testlib::{fixtures, four_layer_design};

use super::search_tests::{find_soft, open_grid};
use super::*;

/// A track of the net `3V3` over the first pad of the net `GND`, `gap` mm from its top
/// edge, `width` wide, settled against that pad alone.
fn settle_beside_a_foreign_pad(gap: f64, width: f64) -> (Option<Track>, f64) {
    let board = crate::board_from_design(
        &four_layer_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap();
    let router = Router::new(&board, &RouteOptions::default());
    let part = board.part("D1").unwrap();
    let pad = part.footprint.pads_numbered("1").next().unwrap();
    let bounds = part.pad_bounds(pad);
    let net = board.nets.iter().position(|n| n.name == "3V3").unwrap();
    let pad_only: Vec<&Obstacle> = router
        .fixed
        .list
        .iter()
        .filter(|o| matches!(o.form, Form::Rect(r) if r == bounds))
        .collect();
    assert_eq!(pad_only.len(), 1, "the pad is one obstacle");
    let y = bounds.min.y - gap - width / 2.0;
    let track = Track {
        net: "3V3".into(),
        layer: CopperLayer::Front,
        from: Pt::new(bounds.min.x, y),
        to: Pt::new(bounds.max.x, y),
        width,
        mid: None,
    };
    let routed = Routed {
        line: 0,
        tracks: vec![track],
        vias: Vec::new(),
    };
    let settled = router.settled(routed, net, &pad_only, true);
    (
        settled.ok().map(|r| r.tracks[0].clone()),
        board.rules.min_track_width,
    )
}

fn a_track(net: &str, from: Pt, to: Pt) -> Track {
    Track {
        net: net.into(),
        layer: CopperLayer::Front,
        from,
        to,
        width: 0.2,
        mid: None,
    }
}

#[test]
fn the_print_of_a_window_changes_with_the_copper_near_it_only() {
    let board = crate::board_from_design(
        &four_layer_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap();
    let router = Router::new(&board, &RouteOptions::default());
    let window = Window {
        i0: 0,
        j0: 0,
        nx: 5,
        ny: 5,
    };
    let corner = router.grid.centre(0, 0);
    let far = router.grid.centre(router.grid.nx - 1, router.grid.ny - 1);
    assert!(
        far.distance(corner) > 2.0 * router.print_reach + 1.0,
        "a far corner"
    );
    // Line 0 is of one net and the line `other` of another.
    let net = router.nets[0];
    let other = (0..router.lines.len())
        .find(|&l| router.nets[l] != net)
        .unwrap();
    let (mine, theirs) = (router.lines[0].net.clone(), router.lines[other].net.clone());
    let routed = |line: usize, net: &str, at: Pt| Routed {
        line,
        tracks: vec![a_track(net, at, at + Pt::new(0.5, 0.0))],
        vias: Vec::new(),
    };
    let near = routed(other, &theirs, corner);
    let print = |others: &[&Routed], soft: bool, locked: &[usize]| {
        router.print(window, net, others, soft, locked)
    };
    let bare = print(&[], false, &[]);
    // The net's own tracks, copper far away, and the order of the copper are not in it.
    assert_eq!(print(&[&routed(0, &mine, corner)], false, &[]), bare);
    assert_eq!(print(&[&routed(other, &theirs, far)], false, &[]), bare);
    let far_too = routed(other, &theirs, far);
    assert_eq!(
        print(&[&near, &far_too], false, &[]),
        print(&[&far_too, &near], false, &[])
    );
    // Another net's track near the window is, and whether a soft search may cross it.
    let hard = print(&[&near], false, &[]);
    assert_ne!(hard, bare);
    assert_ne!(print(&[&near], true, &[]), hard);
    assert_eq!(print(&[&near], true, &[other]), hard);
    // So is what the negotiation has learnt about the window's cells.
    let cells = router.grid.nx * router.grid.ny;
    *router.history.borrow_mut() = [vec![0; cells], vec![0; cells]];
    let learnt = print(&[], false, &[]);
    router.learn(&[(1, 1, FRONT)]);
    assert_ne!(print(&[], false, &[]), learnt);
}

#[test]
fn a_search_whose_print_is_unchanged_finds_what_it_found_before() {
    let board = crate::board_from_design(
        &four_layer_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap();
    let router = Router::new(&board, &RouteOptions::default());
    let state = State::default();
    let copper =
        |found: Option<(Routed, Vec<Spot>)>| found.map(|(r, path)| (r.tracks, r.vias, path));
    let first = copper(router.try_line(0, &state, &[], ROOM_TIGHT, false));
    assert!(first.is_some());
    let remembered = router.memo.borrow().len();
    assert!(remembered > 0);
    let again = copper(router.try_line(0, &state, &[], ROOM_TIGHT, false));
    assert_eq!(
        router.memo.borrow().len(),
        remembered,
        "nothing searched again"
    );
    assert_eq!(again, first);
    router.memo.borrow_mut().clear();
    assert_eq!(
        copper(router.try_line(0, &state, &[], ROOM_TIGHT, false)),
        first
    );
}

/// Whether a line with `mine` joins cleanly a line of its net with `theirs`, all on the
/// front, away from the net's pads.
fn joins(mine: &[(Pt, Pt)], theirs: &[(Pt, Pt)]) -> bool {
    let board = crate::board_from_design(
        &four_layer_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap();
    let router = Router::new(&board, &RouteOptions::default());
    let net = board.nets[router.nets[0]].name.clone();
    let routed = |tracks: &[(Pt, Pt)]| Routed {
        line: 0,
        tracks: tracks
            .iter()
            .map(|&(from, to)| Track {
                net: net.clone(),
                layer: CopperLayer::Front,
                from,
                to,
                width: 0.1,
                mid: None,
            })
            .collect(),
        vias: Vec::new(),
    };
    let other = routed(theirs);
    router.joins_cleanly(routed(mine), &[&other], true).is_ok()
}

/// A point far off the board, where no pad of any net is.
fn off(x: f64, y: f64) -> Pt {
    Pt::new(-50.0 + x, -50.0 + y)
}

#[test]
fn a_line_that_meets_another_of_its_net_at_45_degrees_is_refused() {
    // Issue #476: the other line ends at the point coming along x; this one ends there
    // coming along the diagonal, on the same side, which leaves an acute pocket.
    let theirs = [(off(1.0, 0.0), off(0.0, 0.0))];
    let mine = [(off(1.0, 1.0), off(0.0, 0.0))];
    assert!(!joins(&mine, &theirs));
    // A crossing at the point: both lines run on through it.
    let theirs = [
        (off(-1.0, 0.0), off(0.0, 0.0)),
        (off(0.0, 0.0), off(1.0, 0.0)),
    ];
    let mine = [
        (off(-1.0, -1.0), off(0.0, 0.0)),
        (off(0.0, 0.0), off(1.0, 1.0)),
    ];
    assert!(!joins(&mine, &theirs));
}

#[test]
fn a_line_that_goes_on_from_another_by_a_bend_of_45_degrees_or_meets_it_square_is_kept() {
    // On from the other line's end by a bend of 45 degrees.
    let theirs = [(off(-1.0, 0.0), off(0.0, 0.0))];
    let mine = [(off(0.0, 0.0), off(1.0, 1.0))];
    assert!(joins(&mine, &theirs));
    // A tee: square into a line that runs on through the point.
    let theirs = [
        (off(-1.0, 0.0), off(0.0, 0.0)),
        (off(0.0, 0.0), off(1.0, 0.0)),
    ];
    let mine = [(off(0.0, 1.0), off(0.0, 0.0))];
    assert!(joins(&mine, &theirs));
    // Over the other line the same way: one direction, not a corner.
    let theirs = [(off(0.0, 0.0), off(1.0, 0.0))];
    let mine = [(off(0.0, 0.0), off(2.0, 0.0))];
    assert!(joins(&mine, &theirs));
    // Lines that do not meet at an end.
    let theirs = [(off(0.0, 0.0), off(1.0, 0.0))];
    let mine = [(off(0.0, 1.0), off(1.0, 2.0))];
    assert!(joins(&mine, &theirs));
}

#[test]
fn a_wide_track_nearer_a_pad_than_the_rules_allow_is_drawn_at_the_narrowest_width() {
    // 0.05 mm from the pad at 0.6 mm; at the narrowest width the same centre line is
    // far enough away.
    let (track, narrowest) = settle_beside_a_foreign_pad(0.05, 0.6);
    let track = track.expect("a narrow track fits");
    assert_eq!(track.width, narrowest);
}

#[test]
fn a_wide_track_that_keeps_the_rules_clear_is_left_wide() {
    let (track, _) = settle_beside_a_foreign_pad(0.3, 0.6);
    assert_eq!(track.expect("it fits").width, 0.6);
}

#[test]
fn a_track_over_a_pad_of_another_net_is_refused_whatever_its_width() {
    let (track, _) = settle_beside_a_foreign_pad(-0.4, 0.6);
    assert_eq!(track, None);
}

#[test]
fn a_via_to_a_plane_keeps_the_rules_clear_of_other_copper() {
    let mut board = crate::board_from_design(
        &four_layer_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap();
    route(&mut board);
    let rules = board.rules.clone();
    for via in &board.vias {
        // Off every pad of another net by the clearance.
        for part in &board.parts {
            for pad in &part.footprint.pads {
                let own = board
                    .net_of(&part.refdes, &pad.number)
                    .is_some_and(|n| n.name == via.net);
                if own {
                    continue;
                }
                let b = part.pad_bounds(pad);
                let dx = (b.min.x - via.at.x).max(via.at.x - b.max.x).max(0.0);
                let dy = (b.min.y - via.at.y).max(via.at.y - b.max.y).max(0.0);
                assert!(
                    dx.hypot(dy) - via.diameter / 2.0 >= rules.clearance - 1e-9,
                    "{} via at {:?} too near {}.{}",
                    via.net,
                    via.at,
                    part.refdes,
                    pad.number
                );
            }
        }
    }
}

#[test]
fn a_via_keeps_the_masks_least_web_from_an_opening_the_footprint_draws_in_the_mask() {
    let mut board = crate::board_from_design(
        &four_layer_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap();
    // An opening in the mask the library draws beside R1, where its plane via would go.
    board
        .parts
        .iter_mut()
        .find(|p| p.refdes == "R1")
        .unwrap()
        .rotation = 0.0;
    let r1 = board.part("R1").unwrap().courtyard();
    let opening = Rect::new(
        Pt::new(r1.max.x + 0.2, r1.min.y - 1.0),
        Pt::new(r1.max.x + 2.2, r1.max.y + 1.0),
    );
    let r1 = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
    let local = opening.translated(Pt::new(-r1.at.x, -r1.at.y));
    r1.footprint.mask_openings.push(local);
    route(&mut board);
    let rules = board.rules.clone();
    let reach = rules.via_diameter / 2.0 + rules.mask_bridge;
    for via in &board.vias {
        let form = Form::Rect(opening);
        assert!(
            form.distance(via.at) >= reach - 1e-9,
            "{} via at {:?} is {:.3} mm from the opening, under {reach}",
            via.net,
            via.at,
            form.distance(via.at)
        );
    }
    assert!(!board.vias.is_empty());
}

/// Copper of another net across the whole grid on both layers: a wall two cells thick at
/// columns `i` and `i + 1`, except for the rows in `gap`.
fn soft_wall(grid: &Grid, masks: &mut Masks, i: usize, gap: std::ops::Range<usize>) {
    let cells = masks.window.cells();
    let mut soft = Soft {
        track: [vec![false; cells], vec![false; cells]],
        via: vec![false; cells],
    };
    for j in (0..grid.ny).filter(|j| !gap.contains(j)) {
        for i in [i, i + 1] {
            let at = masks.at(i, j);
            soft.track[FRONT][at] = true;
            soft.track[BACK][at] = true;
            soft.via[at] = true;
        }
    }
    masks.soft = Some(soft);
}

#[test]
fn a_soft_search_crosses_copper_that_a_hard_one_cannot() {
    let (grid, mut masks) = open_grid();
    soft_wall(&grid, &mut masks, 15, 0..0);
    // Hard masks do not see the wall; the soft ones price it, and the path goes through
    // because no way round exists.
    let path = find_soft(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
    assert_eq!(path.len(), 31);
    assert!(path.contains(&(15, 5, FRONT)));
}

#[test]
fn a_soft_search_goes_round_copper_by_a_gap_when_the_gap_is_not_far() {
    let (grid, mut masks) = open_grid();
    // The wall is open at rows 8 and 9, three rows from the line: the detour costs 160
    // (six steps and four turns), crossing the wall's two cells 200 (SOFT, 100 each).
    soft_wall(&grid, &mut masks, 15, 8..10);
    let path = find_soft(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
    assert!(path.contains(&(15, 8, FRONT)), "{path:?}");
    assert!(!path.contains(&(15, 5, FRONT)));
}

#[test]
fn a_soft_path_does_not_stop_at_a_gap_that_costs_more_than_the_wall() {
    let (grid, mut masks) = open_grid();
    // Open only at row 9, eight rows from the line, whose detour costs 260: the path
    // crosses the wall.
    soft_wall(&grid, &mut masks, 15, 9..10);
    let path = find_soft(&masks, &[(0, 1, FRONT)], Goal::Spots(&[(30, 1, FRONT)])).unwrap();
    assert!(path.contains(&(15, 1, FRONT)), "{path:?}");
}

#[test]
fn masks_made_from_the_cached_fixed_copper_leave_a_nets_own_pads_open() {
    let board = crate::board_from_design(
        &four_layer_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap();
    let router = Router::new(&board, &RouteOptions::default());
    let window = Window::full(&router.grid);
    let reach = Reach {
        room: ROOM_LOOSE,
        width: board.rules.track_width,
    };
    // For every net, a cell on its own pad is open to it, and shut to every other net.
    for (n, net) in board.nets.iter().enumerate() {
        let own = net.pads.first().expect("a net has pads");
        let part = board.part(&own.refdes).unwrap();
        let pad = part.footprint.pads_numbered(&own.number).next().unwrap();
        let (i, j) = router.grid.cell(part.pad_centre(pad));
        let (front, _) = part.pad_layers(pad);
        let layer = if front { FRONT } else { BACK };
        let mine = router.masks(window, n, (&[], &[]), (&[], &[]), reach);
        assert!(
            !mine.track[layer][mine.at(i, j)],
            "{}: its own pad {own:?} is shut to it",
            net.name
        );
        let other = (n + 1) % board.nets.len();
        {
            let theirs = router.masks(window, other, (&[], &[]), (&[], &[]), reach);
            assert!(
                theirs.track[layer][theirs.at(i, j)],
                "{}: the pad of {} is open to {}",
                net.name,
                net.name,
                board.nets[other].name
            );
        }
    }
}

#[test]
fn distances_are_to_the_edge_of_the_form() {
    let rect = Form::Rect(Rect::new(Pt::new(0.0, 0.0), Pt::new(2.0, 1.0)));
    assert_eq!(rect.distance(Pt::new(1.0, 0.5)), 0.0);
    assert_eq!(rect.distance(Pt::new(3.0, 0.5)), 1.0);
    assert_eq!(rect.distance(Pt::new(5.0, 5.0)), 5.0);
    let track = Form::Segment(Pt::new(0.0, 0.0), Pt::new(4.0, 0.0), 0.1);
    assert!((track.distance(Pt::new(2.0, 1.0)) - 0.9).abs() < 1e-12);
    assert!((track.distance(Pt::new(6.0, 0.0)) - 1.9).abs() < 1e-12);
    let via = Form::Circle(Pt::new(0.0, 0.0), 0.3);
    assert!((via.distance(Pt::new(1.0, 0.0)) - 0.7).abs() < 1e-12);
}
