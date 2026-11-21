//! Tests of the annealer.

use netsmith_core::{Design, Net, Part, PinRef};

use crate::testlib::fixtures;
use netsmith_pcb::{Board, BoardNet};

use super::*;

#[test]
fn a_shortfall_that_is_real_is_never_shown_as_zero() {
    assert_eq!(short_mm(0.8), "0.80");
    assert_eq!(short_mm(0.05), "0.05");
    assert_eq!(short_mm(0.004), "0.004");
    assert_eq!(short_mm(0.0004), "0.0004");
    // Below what the placement check itself ignores there is nothing to show.
    assert_eq!(short_mm(2e-6), "<0.0001");
    for v in [0.0123, 0.0049, 0.00051, 0.00011] {
        assert!(
            short_mm(v).parse::<f64>().is_ok_and(|shown| shown > 0.0),
            "{v} is shown as {}",
            short_mm(v)
        );
    }
}

#[test]
fn the_random_numbers_and_the_exponential_are_the_same_on_every_machine() {
    // The generator is SplitMix64: the first outputs for a seed are fixed numbers.
    let mut r = Rng::new(0);
    let first = [r.next(), r.next(), r.next()];
    let mut again = Rng::new(0);
    assert_eq!(first, [again.next(), again.next(), again.next()]);
    assert_ne!(first[0], Rng::new(1).next());
    assert!((0..1000).all(|_| (0.0..1.0).contains(&r.unit())));
    assert!((0..1000).all(|_| r.below(7) < 7));
    assert!((0..1000).all(|_| r.spread(5).abs() <= 5));
    assert_eq!(exp_neg(0.0), 1.0);
    assert!((exp_neg(1.0) - (-1.0f64).exp()).abs() < 1e-9);
    assert!((exp_neg(7.3) - (-7.3f64).exp()).abs() < 1e-9);
    assert_eq!(exp_neg(41.0), 0.0);
}

fn resistors(paths: &[&str]) -> Board {
    let parts = paths
        .iter()
        .enumerate()
        .map(|(i, path)| Part {
            provides: Vec::new(),
            declared_kind: netsmith_testkit::std_kind("Resistor")
                .and_then(netsmith_core::PartKind::from_word),
            refdes: format!("R{}", i + 1),
            kind: "Resistor".into(),
            value: "1k".into(),
            package: Some("0603".into()),
            path: (*path).into(),
            pins: Vec::new(),
        })
        .collect();
    let design = Design {
        modules: Vec::new(),
        name: "T".into(),
        process: Some(crate::testlib::test_process()),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts,
        nets: vec![Net {
            name: "N".into(),
            pins: vec![PinRef {
                refdes: "R1".into(),
                pin: "1".into(),
            }],
        }],
    };
    crate::board_from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures()).unwrap()
}

#[test]
fn a_nested_module_is_in_its_outer_modules_group_and_has_none_of_its_own() {
    // `a.inner` is a module inside `a`: its two parts pull with `a`'s, once.
    let mut board = resistors(&["a.inner.x", "a.inner.y", "a.z", "b.x", "b.y"]);
    let fixed = [false; 5];
    let problem = Problem {
        nets: &board.nets.clone(),
        rules: &board.rules.clone(),
        planes: &[],
        signal_layers: 2,
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        face_keepouts: &[],
        pairs: &[],
        overflow: 0.0,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed: 0,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: board.outline,
    };
    let e = Engine::new(&mut board.parts, &problem);
    assert_eq!(e.groups, vec![vec![0, 1, 2], vec![3, 4]]);
}

#[test]
fn a_module_costs_more_the_further_apart_its_parts_are() {
    let mut board = resistors(&["a.x", "a.y", "b.x", "b.y"]);
    let fixed = [false; 4];
    let problem = Problem {
        nets: &board.nets.clone(),
        rules: &board.rules.clone(),
        planes: &[],
        signal_layers: 2,
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        face_keepouts: &[],
        pairs: &[],
        overflow: 0.0,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed: 0,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: board.outline,
    };
    let mut e = Engine::new(&mut board.parts, &problem);
    assert_eq!(e.groups.len(), 2, "one module instance for a and one for b");
    let at = |x: i64, y: i64| Pose {
        x,
        y,
        turn: 0,
        text: 0,
    };
    // a's parts side by side, b's far away, then a's parts apart.
    for (i, (x, y)) in [(0, 0), (60, 0), (800, 0), (860, 0)]
        .into_iter()
        .enumerate()
    {
        e.put(i, at(x, y));
    }
    let together = e.group_cost(0);
    e.put(1, at(600, 400));
    let apart = e.group_cost(0);
    assert!(apart > together + 10.0, "{together} and {apart}");
    // b's part inside the box of a costs, one outside does not.
    e.put(2, at(30, 0));
    let inside = e.group_cost(0);
    e.put(2, at(900, 0));
    e.put(1, at(60, 0));
    let outside = e.group_cost(0);
    assert!(inside > outside, "{inside} and {outside}");
}

#[test]
fn parts_that_stay_are_never_moved_and_the_others_end_clear_of_them() {
    let mut board = resistors(&["", "", "", "", "", ""]);
    let before: Vec<Pt> = board.parts.iter().map(|p| p.at).collect();
    let fixed = [true, true, false, false, false, false];
    let problem = Problem {
        nets: &board.nets.clone(),
        rules: &board.rules.clone(),
        planes: &[],
        signal_layers: 2,
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        face_keepouts: &[],
        pairs: &[],
        overflow: 0.0,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed: 3,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: board.outline,
    };
    let outcome = anneal(&mut board.parts, &problem, 1.0);
    assert!(outcome.feasible);
    assert_eq!(board.parts[0].at, before[0]);
    assert_eq!(board.parts[1].at, before[1]);
    assert_eq!(board.check_placement(), vec![]);
}

#[test]
fn two_parts_that_hang_over_the_same_edge_may_overlap_beyond_it() {
    // ADR 0045, section 10 (i): the placement check counts a part that may hang over an
    // edge as its courtyard clipped to the outline, so the annealer must too. R1 stands
    // 0.5 mm inside the bottom edge and R2 wholly below it, 0.2 mm under it; their whole
    // courtyards overlap, but R2 has no room on the board.
    let mut board = resistors(&["", ""]);
    let outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(20.0, 20.0));
    let fixed = [false; 2];
    let mut bottom = Edges::default();
    bottom.set(netsmith_pcb::Edge::Bottom);
    let run = |board: &mut Board, overhangs: &[Edges]| {
        let problem = Problem {
            nets: &board.nets.clone(),
            rules: &board.rules.clone(),
            planes: &[],
            signal_layers: 2,
            region: outline,
            corner_radius: 0.0,
            fixed: &fixed,
            keepouts: &[],
            zones: &[],
            carried: &[],
            face_keepouts: &[],
            pairs: &[],
            overflow: 0.0,
            escape_room: 0.0,
            pin_distances: &[],
            rf_nets: &[],
            net_currents: &[],
            routing: 0.0,
            cutouts: &[],
            open_outside: &[],
            pours: false,
            seed: 0,
            wants: &[],
            offsets: &[],
            overhangs,
            outline,
        };
        let mut e = Engine::new(&mut board.parts, &problem);
        let court = e.shape(0).court;
        let pose = |top: f64| Pose {
            x: (10.0 / UNIT).round() as i64,
            y: ((top - court.min.y) / UNIT).round() as i64,
            turn: 0,
            text: 0,
        };
        e.put(0, pose(20.0 - 0.5));
        e.put(1, pose(20.0 + 0.2));
        let (a, b) = (e.court(0), e.court(1));
        assert!(a.overlaps(&b), "the whole courtyards overlap: {a:?} {b:?}");
        assert!(b.min.y >= 20.0, "R2 is below the edge: {b:?}");
        e.pair(0, 1)
    };
    let strict = run(&mut board, &[]);
    assert!(
        strict.hard > 0.0,
        "unstated, the overlap is hard: {strict:?}"
    );
    let stated = run(&mut board, &[bottom, bottom]);
    assert!(
        stated.hard == 0.0,
        "stated on both, the overlap beyond the edge is free: {stated:?}"
    );
    // One part stating it is enough for it to take only the room it has on the board.
    let one = run(&mut board, &[Edges::default(), bottom]);
    assert!(one.hard == 0.0, "{one:?}");
}
#[test]
fn a_part_that_stays_off_the_grid_is_measured_where_the_check_measures_it() {
    // #457: a part the requirements pin at x 97.405 mm (off the placer's 0.05 mm grid)
    // and in no relation was measured at the grid's nearest point, 0.005 mm off, so the
    // placer put a neighbour against a courtyard the check found 0.005 mm wider.
    let mut board = resistors(&["", ""]);
    board.parts[0].at = Pt::new(10.025, 7.0);
    board.parts[0].rotation = 90.0;
    let outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(20.0, 20.0));
    let fixed = [true, false];
    let problem = Problem {
        nets: &board.nets.clone(),
        rules: &board.rules.clone(),
        planes: &[],
        signal_layers: 2,
        region: outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        face_keepouts: &[],
        pairs: &[],
        overflow: 0.0,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed: 0,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline,
    };
    let checked = board.parts[0].courtyard();
    let e = Engine::new(&mut board.parts, &problem);
    let placer = e.court(0);
    for (a, b) in [
        (placer.min.x, checked.min.x),
        (placer.max.x, checked.max.x),
        (placer.min.y, checked.min.y),
        (placer.max.y, checked.max.y),
    ] {
        assert!(
            (a - b).abs() < 1e-9,
            "the placer's courtyard {placer:?} is the check's {checked:?}"
        );
    }
}

/// A 30 by 10 mm board with a zone over its left 20 mm and a free strip 10 mm wide
/// beside it. `R1` is 14 mm tall with a body 6 by 4 mm; its courtyard goes 4.5 mm past
/// the body on one side (an edge connector's shell), so that the whole courtyard needs
/// 10.5 mm and the strip, less the board's 2 mm wall, has 8 mm. `R2` to `R4` are small.
/// Returns the board and the zone.
fn lid_beside_a_strip() -> (Board, Rect) {
    let mut board = resistors(&["", "", "", ""]);
    let body = Rect::new(Pt::new(-3.0, -2.0), Pt::new(3.0, 2.0));
    let tall = &mut board.parts[0];
    tall.footprint.body = body;
    tall.footprint.body_is_courtyard = false;
    tall.footprint
        .set_courtyard(Rect::new(Pt::new(-7.5, -2.5), Pt::new(3.5, 2.5)));
    tall.height = Some(14.0);
    for small in &mut board.parts[1..] {
        small.height = Some(1.0);
    }
    board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(30.0, 10.0));
    (board, Rect::new(Pt::new(0.0, 0.0), Pt::new(20.0, 10.0)))
}

fn run_beside_strip(
    board: &mut Board,
    zones: &[netsmith_pcb::HeightZone],
    keepouts: &[Rect],
) -> bool {
    let fixed = [false; 4];
    let problem = Problem {
        nets: &board.nets.clone(),
        rules: &board.rules.clone(),
        planes: &[],
        signal_layers: 2,
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts,
        zones,
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed: 0,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: board.outline,
        carried: &[],
        face_keepouts: &[],
        pairs: &[],
        overflow: 0.0,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
    };
    anneal(&mut board.parts, &problem, 1.0).feasible
}

fn lid(rect: Rect) -> netsmith_pcb::HeightZone {
    netsmith_pcb::HeightZone {
        name: "lid".into(),
        rect,
        front: true,
        back: false,
        limit: 10.0,
    }
}

#[test]
fn a_part_is_kept_out_of_a_height_limit_by_its_body_and_a_margin_not_by_its_courtyard() {
    // Owner decision 2026-11-04 (issue #29, ADR 0045 section 11). The courtyard is 10.5 mm
    // wide and the strip beside the zone has 8 mm inside the wall: the body fits, the
    // courtyard does not.
    let (mut board, zone) = lid_beside_a_strip();
    assert!(run_beside_strip(&mut board, &[lid(zone)], &[]));
    let tall = &board.parts[0];
    let body = tall.body();
    assert!(
        body.min.x >= zone.max.x + HEIGHT_ZONE_MARGIN - 1e-6,
        "the body is at x = {:.2} mm, {:.2} mm from the zone",
        body.min.x,
        body.min.x - zone.max.x
    );
    assert!(
        tall.courtyard().min.x < zone.max.x,
        "the courtyard goes into the zone, where only the body is kept out"
    );
    // What the check measures: no part of the body is in the zone.
    assert!(!body.overlaps(&zone));
    for small in &board.parts[1..] {
        assert!(!small.courtyard().overlaps(&tall.courtyard()));
    }
    // The same inputs give the same board.
    let (mut again, zone) = lid_beside_a_strip();
    assert!(run_beside_strip(&mut again, &[lid(zone)], &[]));
    let poses =
        |b: &Board| -> Vec<(Pt, f64)> { b.parts.iter().map(|p| (p.at, p.rotation)).collect() };
    assert_eq!(poses(&board), poses(&again));
}

#[test]
fn a_part_that_draws_no_fabrication_outline_is_kept_out_by_its_courtyard() {
    // Its courtyard is its body, as the check reads it: no margin is added to that.
    let (mut board, zone) = lid_beside_a_strip();
    let court = board.parts[0].footprint.courtyard;
    board.parts[0].footprint.body = court;
    board.parts[0].footprint.body_is_courtyard = true;
    assert!(!run_beside_strip(&mut board, &[lid(zone)], &[]));
}

#[test]
fn a_keepout_for_parts_still_keeps_the_whole_courtyard_out() {
    let (mut board, zone) = lid_beside_a_strip();
    assert!(!run_beside_strip(&mut board, &[], &[zone]));
}

/// Four resistors on a 30 by 10 mm board, all on `side`, and a keepout for the parts on one
/// face over the whole board (issue #767): whether the run ends with every
/// courtyard clear.
fn run_with_a_face_keepout(side: netsmith_pcb::Side, front: bool, back: bool) -> bool {
    let mut board = resistors(&["", "", "", ""]);
    board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(30.0, 10.0));
    for part in &mut board.parts {
        part.side = side;
    }
    let keepout = netsmith_pcb::FaceKeepout {
        name: "under_antenna".into(),
        rect: Rect::new(Pt::new(0.0, 0.0), Pt::new(30.0, 10.0)),
        front,
        back,
    };
    let fixed = [false; 4];
    let problem = Problem {
        nets: &board.nets.clone(),
        rules: &board.rules.clone(),
        planes: &[],
        signal_layers: 2,
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed: 0,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: board.outline,
        carried: &[],
        face_keepouts: &[keepout],
        pairs: &[],
        overflow: 0.0,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
    };
    anneal(&mut board.parts, &problem, 1.0).feasible
}

#[test]
fn a_keepout_for_the_parts_on_one_face_keeps_out_only_the_parts_on_that_face() {
    use netsmith_pcb::Side;
    // On the face it keeps out the keepout leaves the parts no room, on the other it leaves
    // them all of it.
    assert!(!run_with_a_face_keepout(Side::Front, true, false));
    assert!(run_with_a_face_keepout(Side::Back, true, false));
    assert!(!run_with_a_face_keepout(Side::Back, false, true));
    assert!(run_with_a_face_keepout(Side::Front, false, true));
}

/// Four resistors on a 26 by 12 mm board, the first (`a`) carrying a keepout of
/// 12 by 10 mm in its own frame, from 2 mm right of its origin; the others are named `b`,
/// `c` and `d`. Returns the board and the zone, as the annealer is told.
fn carrier_with_a_zone(parts: bool, limit: Option<f64>) -> (Board, netsmith_pcb::CarriedZone) {
    let mut board = resistors(&["a", "b", "c", "d"]);
    board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(26.0, 12.0));
    let zone = netsmith_pcb::CarriedZone {
        name: "shadow".into(),
        part: "a".into(),
        local: Rect::new(Pt::new(2.0, -5.0), Pt::new(14.0, 5.0)),
        parts,
        front: true,
        back: true,
        limit,
        on_carrier_face: false,
    };
    board.keepouts.push(netsmith_pcb::BoardKeepout {
        name: zone.name.clone(),
        part: Some("a".into()),
        at: Some(zone.local.min),
        size: (12.0, 10.0),
        layers: vec!["top".into(), "bottom".into()],
        tracks: false,
        vias: false,
        copper: false,
        parts,
        max_height: limit,
        on_carrier_face: false,
        polygon: None,
    });
    (board, zone)
}

fn run_with_carried(board: &mut Board, carried: &[netsmith_pcb::CarriedZone]) -> bool {
    let fixed = [false; 4];
    let problem = Problem {
        nets: &board.nets.clone(),
        rules: &board.rules.clone(),
        planes: &[],
        signal_layers: 2,
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried,
        face_keepouts: &[],
        pairs: &[],
        overflow: 0.0,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed: 0,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: board.outline,
    };
    anneal(&mut board.parts, &problem, 1.0).feasible
}

#[test]
fn a_keepout_for_parts_that_a_part_carries_keeps_the_other_parts_out_where_the_part_is() {
    // ADR 0045 section 11 (issue 28): the check measures a carried keepout against every
    // part but its carrier; the annealer must keep the parts out of it too, where the
    // carrier ends up and however it is turned.
    let (mut blind, _) = carrier_with_a_zone(true, None);
    assert!(run_with_carried(&mut blind, &[]));
    assert!(
        blind
            .check_placement()
            .iter()
            .any(|p| matches!(p, netsmith_pcb::PlacementProblem::InKeepout { .. })),
        "without the zone told to the placer a part stands in it, so the test says something"
    );
    let (mut board, zone) = carrier_with_a_zone(true, None);
    assert!(run_with_carried(&mut board, &[zone]));
    assert_eq!(board.check_placement(), vec![]);
}

#[test]
fn a_keepout_that_a_part_on_the_back_carries_is_kept_clear_of_mirrored() {
    let (mut board, zone) = carrier_with_a_zone(true, None);
    board.parts[0].side = Side::Back;
    assert!(run_with_carried(&mut board, &[zone]));
    assert_eq!(board.check_placement(), vec![]);
}

#[test]
fn a_height_limit_that_a_part_carries_keeps_out_the_parts_too_tall_for_it() {
    let tall_in_the_zone = |board: &Board| {
        board
            .check_placement()
            .iter()
            .any(|p| matches!(p, netsmith_pcb::PlacementProblem::TooTall { .. }))
    };
    let (mut blind, _) = carrier_with_a_zone(false, Some(3.0));
    for part in &mut blind.parts[1..] {
        part.height = Some(5.0);
    }
    assert!(run_with_carried(&mut blind, &[]));
    assert!(
        tall_in_the_zone(&blind),
        "without the zone told to the placer a 5 mm part stands in it"
    );
    let (mut board, zone) = carrier_with_a_zone(false, Some(3.0));
    for part in &mut board.parts[1..] {
        part.height = Some(5.0);
    }
    assert!(run_with_carried(&mut board, &[zone]));
    assert!(!tall_in_the_zone(&board), "{:?}", board.check_placement());
    let area = board.keepout_areas()[0].rect;
    for tall in &board.parts[1..] {
        assert!(!tall.body().overlaps(&area), "{} stands clear", tall.refdes);
    }
}

#[test]
fn a_keepout_a_part_carries_is_no_reason_to_refuse_the_part_itself() {
    // The carrier is inside its own keepout and is not counted against it.
    let (mut board, zone) = carrier_with_a_zone(true, None);
    let fixed = [false; 4];
    let carried = [zone];
    let problem = Problem {
        nets: &board.nets.clone(),
        rules: &board.rules.clone(),
        planes: &[],
        signal_layers: 2,
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &carried,
        face_keepouts: &[],
        pairs: &[],
        overflow: 0.0,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed: 0,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: board.outline,
    };
    let e = Engine::new(&mut board.parts, &problem);
    assert!(e.carried_depths(0, 0, 0.0).is_empty());
    assert_eq!(e.carried_cost(0, 0).hard, 0.0);
}

/// A footprint of two courtyard rectangles 2.4 x 4 mm and 22 mm apart, as a plug-in module
/// draws its two socket strips, with a pad in each.
fn two_strips() -> netsmith_pcb::Footprint {
    netsmith_pcb::Footprint::parse(
        "T:Strips",
        r#"(footprint "Strips"
            (pad "1" smd rect (at 0 0) (size 1.5 1.5) (layers "F.Cu" "F.Mask"))
            (pad "2" smd rect (at 22 0) (size 1.5 1.5) (layers "F.Cu" "F.Mask"))
            (fp_rect (start -1.2 -2) (end 1.2 2) (layer "F.CrtYd"))
            (fp_rect (start 20.8 -2) (end 23.2 2) (layer "F.CrtYd")))"#,
    )
    .unwrap()
}

/// A module that stays at x = 5 in a board 29.5 mm wide and 8 mm tall, and four 2.5 x 2.5 mm
/// parts to place: beside the strips there is room for two of them at most (left of the first
/// strip), so the others must stand between the strips.
fn module_with_four_small_parts(shapes: bool) -> Board {
    let mut board = resistors(&["", "", "", "", ""]);
    board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(29.5, 8.0));
    let module = &mut board.parts[0];
    module.footprint = two_strips();
    if !shapes {
        module.footprint.set_courtyard(module.footprint.courtyard);
    }
    module.at = Pt::new(5.0, 4.0);
    for small in &mut board.parts[1..] {
        small
            .footprint
            .set_courtyard(Rect::new(Pt::new(-1.25, -1.25), Pt::new(1.25, 1.25)));
    }
    board
}

fn run_with_module_fixed(board: &mut Board) -> bool {
    let fixed = [true, false, false, false, false];
    let problem = Problem {
        nets: &board.nets.clone(),
        rules: &board.rules.clone(),
        planes: &[],
        signal_layers: 2,
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        face_keepouts: &[],
        pairs: &[],
        overflow: 0.0,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed: 0,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: board.outline,
    };
    anneal(&mut board.parts, &problem, 1.0).feasible
}

#[test]
fn parts_are_placed_between_the_courtyard_shapes_of_a_part() {
    let mut board = module_with_four_small_parts(true);
    assert!(run_with_module_fixed(&mut board));
    assert_eq!(board.check_placement(), vec![]);
    // The module's strips are x 3.8 to 6.2 and 25.8 to 28.2: at least two of the four parts
    // are between them.
    let between = board.parts[1..]
        .iter()
        .filter(|p| p.courtyard().min.x >= 6.2 - 1e-6 && p.courtyard().max.x <= 25.8 + 1e-6)
        .count();
    assert!(between >= 2, "{between} between the strips");
}

#[test]
fn the_box_round_the_two_shapes_leaves_no_room_for_those_parts() {
    // The same board with the courtyard as one box (what a footprint's courtyard was): the
    // four parts do not fit beside it, and the placer says so.
    let mut board = module_with_four_small_parts(false);
    assert!(!run_with_module_fixed(&mut board));
}

/// A QFN of eight pads on each side at `pitch`, pads 0.875 x 0.25 mm, on a body of 5 mm at
/// 0.5 mm (larger at a larger pitch, so that the rows do not meet at the corners), with an
/// exposed pad of 3.1 mm; its courtyard 0.25 mm past the ends of the pads.
fn qfn(pitch: f64) -> netsmith_pcb::Footprint {
    let (length, width) = (0.875, 0.25);
    let centre = 4.0 * pitch + 0.4375;
    let mut text = String::from("(footprint \"QFN\" (attr smd) ");
    for side in 0..4 {
        for k in 0..8 {
            let along = (f64::from(k) - 3.5) * pitch;
            let (x, y, w, h) = match side {
                0 => (-centre, along, length, width),
                1 => (along, centre, width, length),
                2 => (centre, -along, length, width),
                _ => (-along, -centre, width, length),
            };
            text += &format!(
                "(pad \"{}\" smd rect (at {x} {y}) (size {w} {h}) (layers \"F.Cu\" \"F.Mask\")) ",
                side * 8 + k + 1
            );
        }
    }
    let court = centre + length / 2.0 + 0.25;
    text += &format!(
        "(pad \"33\" smd rect (at 0 0) (size 3.1 3.1) (layers \"F.Cu\" \"F.Mask\")) \
         (fp_rect (start {n} {n}) (end {court} {court}) (layer \"F.CrtYd\")))",
        n = -court
    );
    netsmith_pcb::Footprint::parse("T:QFN", &text).unwrap()
}

/// The box round a part's surface-mount pads on the board, grown by the escape ring's width.
fn ring_of(part: &Placed) -> Rect {
    part.footprint
        .pads
        .iter()
        .filter(|p| p.drill.is_none())
        .map(|p| part.pad_bounds(p))
        .reduce(|a, b| a.union(&b))
        .unwrap()
        .expanded(RING)
}

#[test]
fn a_qfn_at_half_a_millimetre_is_fine_pitch_and_one_at_0_65_mm_or_a_resistor_is_not() {
    let mut board = resistors(&["", ""]);
    assert!(!fine_pitch(&board.parts[1]));
    board.parts[0].footprint = qfn(0.5);
    assert!(fine_pitch(&board.parts[0]));
    board.parts[0].footprint = qfn(0.65);
    assert!(!fine_pitch(&board.parts[0]));
}

/// A board of a QFN at 0.5 mm in the middle of a square of `side` mm and `others` resistors,
/// and a problem that keeps the QFN where it is.
fn qfn_board(side: f64, others: usize) -> Board {
    let names = vec![""; others + 1];
    let mut board = resistors(&names);
    board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(side, side));
    board.parts[0].footprint = qfn(0.5);
    board.parts[0].at = Pt::new(side / 2.0, side / 2.0);
    board
}

fn run_with_the_qfn_fixed(board: &mut Board, seed: u64) -> Outcome {
    let mut fixed = vec![false; board.parts.len()];
    fixed[0] = true;
    let problem = Problem {
        nets: &board.nets.clone(),
        rules: &board.rules.clone(),
        planes: &[],
        signal_layers: 2,
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        face_keepouts: &[],
        pairs: &[],
        overflow: 0.0,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: board.outline,
    };
    anneal(&mut board.parts, &problem, 1.0)
}

#[test]
fn a_part_in_the_escape_ring_of_a_fine_pitch_part_is_a_hard_cost_but_its_decoupling_capacitor_is_not()
 {
    let mut board = qfn_board(30.0, 1);
    let fixed = [true, false];
    let problem = Problem {
        nets: &board.nets.clone(),
        rules: &board.rules.clone(),
        planes: &[],
        signal_layers: 2,
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        face_keepouts: &[],
        pairs: &[],
        overflow: 0.0,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed: 0,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: board.outline,
    };
    let ring = ring_of(&board.parts[0]);
    let court = board.parts[0].courtyard();
    let mut e = Engine::new(&mut board.parts, &problem);
    // The resistor's courtyard is 1.65 mm from its origin along x: stood with its left end
    // 0.5 mm past the QFN's courtyard it is in the ring, and 0.5 mm past the ring it is not.
    let reach = e.court(1).max.x - e.pos(1).x;
    let y = (e.pos(0).y / UNIT).round() as i64;
    let at = |x: f64| Pose {
        x: (x / UNIT).round() as i64,
        y,
        turn: 0,
        text: 0,
    };
    e.put(1, at(court.max.x + 0.5 + reach));
    assert!(e.ring_cost(0, 1).hard > 0.0);
    assert_eq!(e.ring_cost(1, 0).hard, 0.0, "a resistor has no ring");
    // As a capacitor that decouples the QFN it may stand there.
    e.near_ok[1] = true;
    e.near_ok[2] = true;
    assert_eq!(e.ring_cost(0, 1).total(), 0.0);
    e.near_ok[1] = false;
    e.near_ok[2] = false;
    // On the other face it is no part of the QFN's face.
    e.models[1].side = Side::Back;
    assert_eq!(e.ring_cost(0, 1).total(), 0.0);
    e.models[1].side = Side::Front;
    e.put(1, at(ring.max.x + 0.5 + reach));
    assert_eq!(e.ring_cost(0, 1).total(), 0.0);
}

#[test]
fn the_placer_leaves_the_escape_ring_of_a_fine_pitch_part_free_of_other_parts() {
    // Eight resistors round a QFN on a 20 mm board, over four seeds: no resistor's courtyard
    // reaches into the ring.
    for seed in 0..4 {
        let mut board = qfn_board(20.0, 8);
        let outcome = run_with_the_qfn_fixed(&mut board, seed);
        assert!(outcome.feasible, "seed {seed}: {:?}", outcome.ended);
        let ring = ring_of(&board.parts[0]);
        let inside = board.parts[1..]
            .iter()
            .filter(|p| penetration(&p.courtyard(), &ring, 0.0) > 1e-6)
            .count();
        assert_eq!(inside, 0, "seed {seed}");
        assert_eq!(board.check_placement(), vec![]);
    }
}

#[test]
fn a_board_with_room_for_a_part_only_in_the_escape_ring_does_not_place_and_says_why() {
    // Beside the ring there is less room to the edge wall than a resistor's narrow side; beside
    // the courtyard there is more, so it is the ring alone that leaves the resistor no place.
    let board = qfn_board(30.0, 1);
    let (ring, court) = (ring_of(&board.parts[0]), board.parts[0].courtyard());
    let resistor = board.parts[1].courtyard();
    let narrow = resistor.width().min(resistor.height());
    let wall = board.rules.edge_wall();
    // The square whose strip beside the ring is half the resistor's narrow side.
    let side = ring.width() + 2.0 * (wall + narrow / 2.0);
    assert!((side - court.width()) / 2.0 - wall > narrow + 0.5);
    let mut board = qfn_board(side, 1);
    let outcome = run_with_the_qfn_fixed(&mut board, 0);
    assert!(!outcome.feasible);
    let why: Vec<&String> = outcome
        .ended
        .outside_parts
        .iter()
        .flat_map(|o| &o.why)
        .collect();
    assert!(why.iter().any(|w| w.contains("escape ring of")), "{why:?}");
}

/// Twelve resistors on a 12 x 12 mm board, each pad on a net with the same pad of the resistor
/// six further on, so that six nets of each pad run across the board; a problem that places
/// them all.
fn crossing_nets() -> Board {
    let mut board = resistors(&[""; 12]);
    board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(12.0, 12.0));
    board.nets = (0..6)
        .flat_map(|k| {
            ["1", "2"].map(|pin| BoardNet {
                name: format!("N{k}_{pin}"),
                pads: [k, k + 6]
                    .iter()
                    .map(|i| netsmith_pcb::PadRef {
                        refdes: format!("R{}", i + 1),
                        number: pin.into(),
                    })
                    .collect(),
            })
        })
        .collect();
    board
}

fn problem_of<'a>(board: &'a Board, fixed: &'a [bool], rules: &'a Rules) -> Problem<'a> {
    problem_weighing(board, fixed, rules, 1.0)
}

fn problem_weighing<'a>(
    board: &'a Board,
    fixed: &'a [bool],
    rules: &'a Rules,
    overflow: f64,
) -> Problem<'a> {
    Problem {
        nets: &board.nets,
        rules,
        planes: &[],
        signal_layers: 2,
        region: board.outline,
        corner_radius: 0.0,
        fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        face_keepouts: &[],
        pairs: &[],
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed: 0,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: board.outline,
        overflow,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
    }
}

#[test]
fn nets_and_pads_crowded_into_one_place_overflow_its_bins_and_spread_out_they_do_not() {
    let board = crossing_nets();
    let mut parts = board.parts.clone();
    let fixed = vec![false; parts.len()];
    let rules = board.rules.clone();
    let problem = problem_of(&board, &fixed, &rules);
    let mut e = Engine::new(&mut parts, &problem);
    let unit = |v: f64| (v / UNIT).round() as i64;
    let put_all = |e: &mut Engine, at: &dyn Fn(usize) -> Pt| {
        for i in 0..12 {
            let p = at(i);
            e.put(
                i,
                Pose {
                    x: unit(p.x),
                    y: unit(p.y),
                    turn: 0,
                    text: 0,
                },
            );
        }
        e.refresh_all();
    };
    // Six resistors at one corner and their pairs at the other: their pads in the same bins,
    // and every net across the same box.
    put_all(&mut e, &|i| {
        if i < 6 {
            Pt::new(2.0, 2.0)
        } else {
            Pt::new(10.0, 10.0)
        }
    });
    let crowded = e.overflow();
    assert!(crowded > 0.0, "{crowded}");
    // Each resistor 3 mm below its pair: the nets are short and take no room past what a bin
    // has.
    put_all(&mut e, &|i| {
        let k = i % 6;
        let y = 2.0 + (k / 3) as f64 * 6.0 + if i < 6 { 0.0 } else { 3.0 };
        Pt::new(2.0 + (k % 3) as f64 * 4.0, y)
    });
    assert_eq!(e.overflow(), 0.0);
    assert_eq!(e.overflow_counted_afresh(), 0.0);
}

#[test]
fn the_overflow_kept_through_moves_is_the_overflow_counted_afresh_and_a_moves_cost_is_its_change() {
    let board = crossing_nets();
    let mut parts = board.parts.clone();
    let fixed = vec![false; parts.len()];
    let rules = board.rules.clone();
    let problem = problem_of(&board, &fixed, &rules);
    let mut e = Engine::new(&mut parts, &problem);
    let mut rng = Rng::new(7);
    let (mut kept, mut undone) = (0, 0);
    for _ in 0..400 {
        let change = e.propose(&mut rng, 60);
        if change.is_empty() {
            continue;
        }
        let set: Vec<usize> = change.iter().map(|&(i, _)| i).collect();
        let mut nets: Vec<usize> = set.iter().flat_map(|&i| e.nets_of[i].clone()).collect();
        nets.sort_unstable();
        nets.dedup();
        // Before the move the parts are where the bins were kept: the change is nothing.
        assert_eq!(e.overflow_with(&nets, &set), e.overflow());
        let old: Vec<Pose> = set.iter().map(|&i| e.poses[i]).collect();
        for &(i, pose) in &change {
            e.put(i, pose);
        }
        let after = e.overflow_with(&nets, &set);
        assert!((after - e.overflow_counted_afresh()).abs() < 1e-9);
        // Keep about half the moves and undo the others, as the annealer does.
        if rng.below(2) == 0 {
            e.refresh(&set);
            kept += 1;
            assert!((e.overflow() - after).abs() < 1e-9);
        } else {
            for (&i, &pose) in set.iter().zip(&old) {
                e.put(i, pose);
            }
            undone += 1;
        }
        assert!((e.overflow() - e.overflow_counted_afresh()).abs() < 1e-9);
    }
    assert!(kept > 50 && undone > 50, "{kept} kept, {undone} undone");
}

/// The overflow of the bins of a placed board, in mm of track.
fn overflow_of(board: &mut Board) -> f64 {
    let fixed = vec![true; board.parts.len()];
    let rules = board.rules.clone();
    let nets = board.nets.clone();
    let probe = Board {
        nets,
        ..board.clone()
    };
    let problem = problem_of(&probe, &fixed, &rules);
    Engine::new(&mut board.parts, &problem).overflow()
}

#[test]
fn weighing_track_overflow_leaves_the_bins_less_overflowed_than_not() {
    // The twelve resistors of `crossing_nets` on their board of 12 x 12 mm, where the placer
    // that does not weigh track overflow leaves some bins overflowed: over four seeds, the one that
    // weighs it (at 5, so that the term is felt on a board this small) leaves less.
    let (mut weighed, mut not) = (0.0, 0.0);
    for seed in 0..4 {
        for (weight, total) in [(5.0, &mut weighed), (0.0, &mut not)] {
            let mut board = crossing_nets();
            let fixed = vec![false; board.parts.len()];
            let rules = board.rules.clone();
            let nets = board.nets.clone();
            let probe = Board {
                nets,
                ..board.clone()
            };
            let problem = Problem {
                seed,
                ..problem_weighing(&probe, &fixed, &rules, weight)
            };
            let outcome = anneal(&mut board.parts, &problem, 1.0);
            assert!(
                outcome.feasible,
                "seed {seed}, weight {weight}: {:?}",
                outcome.ended.summary(3)
            );
            *total += overflow_of(&mut board);
        }
    }
    eprintln!("overflow over four seeds: {weighed:.1} mm weighed, {not:.1} mm not");
    assert!(weighed < not, "{weighed} weighed, {not} not");
}

/// `n` resistors (a multiple of 12) two to a place, six places to a row 6 mm apart, in a
/// region of 30 mm by 6 mm for each row: half of them overlap another at the start, and a
/// board of twice the parts is twice the area. With `away`, every part starts 130 mm right
/// of the region instead.
fn piled(n: usize, away: bool) -> (Board, Rect) {
    let paths = vec![""; n];
    let mut board = resistors(&paths);
    for (k, part) in board.parts.iter_mut().enumerate() {
        let spot = k / 2;
        part.at = Pt::new(
            3.0 + (spot % 6) as f64 * 4.5 + if away { 130.0 } else { 0.0 },
            3.0 + (spot / 6) as f64 * 6.0,
        );
    }
    let region = Rect::new(Pt::new(0.0, 0.0), Pt::new(30.0, (n / 12) as f64 * 6.0));
    (board, region)
}

/// What legalising the piled board of `n` parts did: whether it succeeded, the poses it
/// tried and the pairs of parts it costed.
fn legalise_piled(n: usize, away: bool) -> (bool, u64, u64) {
    let (board, region) = piled(n, away);
    let mut parts = board.parts.clone();
    let fixed = vec![false; n];
    let rules = board.rules.clone();
    let mut problem = problem_of(&board, &fixed, &rules);
    problem.region = region;
    problem.outline = region;
    let mut e = Engine::new(&mut parts, &problem);
    e.pairs_costed.set(0);
    let all = e.legalise();
    (all, e.poses_tried, e.pairs_costed.get())
}

#[test]
fn legalising_costs_each_pose_against_the_parts_near_it_not_against_every_part() {
    // Issue #766: each pose a part was tried at cost a pass over every part, so a crowded
    // board of twice the parts cost about four times the work. The parts a pose is costed
    // against are those its reach meets, about as many on a board of twice the size.
    let (small_ok, small_poses, small_pairs) = legalise_piled(48, false);
    let (big_ok, big_poses, big_pairs) = legalise_piled(96, false);
    assert!(small_ok && big_ok);
    assert!(small_poses > 0 && big_poses > small_poses);
    let (small, big) = (
        small_pairs as f64 / small_poses as f64,
        big_pairs as f64 / big_poses as f64,
    );
    println!("pairs costed for each pose tried: {small:.1} of 48 parts, {big:.1} of 96");
    // A pass over every part would be 47 for each pose, and 95 on the board of 96.
    assert!(small < 36.0, "{small_pairs} pairs for {small_poses} poses");
    assert!(
        big < 1.5 * small,
        "{big_pairs} pairs for {big_poses} poses, against {small_pairs} for {small_poses}"
    );
}

#[test]
fn a_part_that_starts_beyond_the_reach_of_legalising_tries_no_pose() {
    // Issue #766: the rows the annealer starts from may be far outside a fixed outline, and
    // each part there tried every pose round it, none inside the walls.
    let (all, poses, _) = legalise_piled(48, true);
    assert!(!all);
    assert_eq!(poses, 0);
}

/// A QFN of 32 pads at 0.65 mm (busy, and not fine pitch, so it has no escape ring) and eight
/// resistors on a board of `side` mm: each pad of the QFN's first two sides joins a pin of a
/// resistor, on a net of its own.
fn busy_board(side: f64) -> Board {
    let mut board = resistors(&[""; 9]);
    board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(side, side));
    board.parts[0].footprint = qfn(0.65);
    board.nets = (0..16)
        .map(|k| BoardNet {
            name: format!("N{k}"),
            pads: vec![
                netsmith_pcb::PadRef {
                    refdes: "R1".into(),
                    number: (k + 1).to_string(),
                },
                netsmith_pcb::PadRef {
                    refdes: format!("R{}", k % 8 + 2),
                    number: if k < 8 { "1" } else { "2" }.into(),
                },
            ],
        })
        .collect();
    board
}

fn problem_with_escape<'a>(
    board: &'a Board,
    fixed: &'a [bool],
    rules: &'a Rules,
    weight: f64,
) -> Problem<'a> {
    Problem {
        escape_room: weight,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
        ..problem_weighing(board, fixed, rules, 0.0)
    }
}

/// Puts part `i` with its origin at `at`, unturned.
fn put_at(e: &mut Engine, i: usize, at: Pt) {
    let unit = |v: f64| (v / UNIT).round() as i64;
    e.put(
        i,
        Pose {
            x: unit(at.x),
            y: unit(at.y),
            turn: 0,
            text: 0,
        },
    );
}

#[test]
fn a_busy_part_whose_side_faces_the_edge_or_a_part_close_by_falls_short_of_escape_room() {
    let board = busy_board(30.0);
    let mut parts = board.parts.clone();
    let fixed = vec![false; parts.len()];
    let rules = board.rules.clone();
    let problem = problem_with_escape(&board, &fixed, &rules, 1.0);
    let mut e = Engine::new(&mut parts, &problem);
    assert_eq!(e.escapers, vec![0]);
    // The QFN in the middle and the resistors in a row far below it: every pad leaves.
    put_at(&mut e, 0, Pt::new(15.0, 15.0));
    for i in 1..9 {
        put_at(&mut e, i, Pt::new(3.0 * i as f64, 27.0));
    }
    e.refresh_all();
    assert_eq!(e.escape_kept_total(), 0.0);
    // The pads of the side at +x (the first eight, as the part lies) 0.5 mm from the edge:
    // less the copper to edge space, two tracks fit along it, and six pads have no way out.
    let rim = e.rim(0).unwrap().pads;
    assert_eq!(e.rim(0).unwrap().leaving[2].len(), 8);
    put_at(&mut e, 0, Pt::new(30.0 - rim.max.x - 0.5, 15.0));
    e.refresh_all();
    let at_edge = e.escape_kept_total();
    assert_eq!(at_edge, 6.0);
    assert_eq!(e.escape_counted_afresh(), at_edge);
    // In the middle again, with a resistor 0.3 mm in front of its second side (+y): the
    // pads behind the resistor cannot leave, and the others can.
    put_at(&mut e, 0, Pt::new(15.0, 15.0));
    e.refresh_all();
    let rim = e.rim(0).unwrap().pads.translated(e.pos(0));
    let court = e.shape(1).court;
    put_at(&mut e, 1, Pt::new(15.0, rim.max.y + 0.3 - court.min.y));
    e.refresh_all();
    let blocked = e.escape_kept_total();
    assert!(blocked > 0.0 && blocked < 8.0, "{blocked}");
}

#[test]
fn the_escape_shortfall_kept_through_moves_is_the_one_counted_afresh() {
    let board = busy_board(10.0);
    let mut parts = board.parts.clone();
    let fixed = vec![false; parts.len()];
    let rules = board.rules.clone();
    let problem = problem_with_escape(&board, &fixed, &rules, 1.0);
    let mut e = Engine::new(&mut parts, &problem);
    let mut rng = Rng::new(11);
    let (mut kept, mut undone, mut short) = (0, 0, 0);
    for _ in 0..400 {
        let change = e.propose(&mut rng, 60);
        if change.is_empty() {
            continue;
        }
        let set: Vec<usize> = change.iter().map(|&(i, _)| i).collect();
        // Before the move the parts are where the shortfalls were kept.
        assert_eq!(e.escape_with(&set), e.escape_kept_total());
        let old: Vec<Pose> = set.iter().map(|&i| e.poses[i]).collect();
        for &(i, pose) in &change {
            e.put(i, pose);
        }
        let after = e.escape_with(&set);
        assert_eq!(after, e.escape_counted_afresh());
        short += usize::from(after > 0.0);
        if rng.below(2) == 0 {
            e.refresh(&set);
            kept += 1;
            assert_eq!(e.escape_kept_total(), after);
        } else {
            for (&i, &pose) in set.iter().zip(&old) {
                e.put(i, pose);
            }
            undone += 1;
        }
        assert_eq!(e.escape_kept_total(), e.escape_counted_afresh());
    }
    assert!(kept > 50 && undone > 50, "{kept} kept, {undone} undone");
    // The moves tried are not all clear of it: the bookkeeping is tested where it counts.
    assert!(short > 20, "{short} moves fell short");
}

#[test]
fn a_push_takes_along_the_parts_a_busy_part_lands_on_and_no_others() {
    // Issue #1035: the QFN in the middle, a resistor 0.2 mm beyond its +x side, one 0.2 mm
    // beyond its -x side and one far off. Pushed 0.5 mm toward +x, the QFN lands on the first,
    // which goes with it by the same step; it leaves the second behind, and the third is
    // nowhere near.
    let board = busy_board(30.0);
    let mut parts = board.parts.clone();
    let fixed = vec![false; parts.len()];
    let rules = board.rules.clone();
    let problem = problem_with_escape(&board, &fixed, &rules, 1.0);
    let mut e = Engine::new(&mut parts, &problem);
    put_at(&mut e, 0, Pt::new(15.0, 15.0));
    for i in 1..9 {
        put_at(&mut e, i, Pt::new(3.0 * i as f64, 27.0));
    }
    let qfn = e.court(0);
    let r = e.shape(1).court;
    put_at(&mut e, 1, Pt::new(qfn.max.x + 0.2 - r.min.x, 15.0));
    put_at(&mut e, 2, Pt::new(qfn.min.x - 0.2 - r.max.x, 15.0));
    e.refresh_all();
    let (q, r1) = (e.poses[0], e.poses[1]);
    let change = e.push_by(0, 10, 0);
    let moved: Vec<usize> = change.iter().map(|&(i, _)| i).collect();
    assert_eq!(moved, vec![0, 1]);
    assert_eq!((change[0].1.x - q.x, change[0].1.y - q.y), (10, 0));
    assert_eq!((change[1].1.x - r1.x, change[1].1.y - r1.y), (10, 0));
    // The parts are where they were: a push is proposed, not made.
    assert!(e.poses[0] == q && e.poses[1] == r1);
    assert_eq!(e.pushes, 1);
}

#[test]
fn only_a_run_that_weighs_escape_room_pushes() {
    // At weight 0 the moves are the ones from before the push was built (the same random
    // numbers drawn), so a placement that does not weigh escape room is unchanged.
    let board = busy_board(30.0);
    let fixed = vec![false; board.parts.len()];
    let rules = board.rules.clone();
    let pushes = |weight: f64| {
        let mut parts = board.parts.clone();
        let problem = problem_with_escape(&board, &fixed, &rules, weight);
        let mut e = Engine::new(&mut parts, &problem);
        let mut rng = Rng::new(5);
        for _ in 0..1000 {
            e.propose(&mut rng, 60);
        }
        e.pushes
    };
    assert_eq!(pushes(0.0), 0);
    // One move in `PUSH_ONE_IN`, less the few with no step.
    let weighed = pushes(1.0);
    assert!((30..=70).contains(&weighed), "{weighed} pushes");
}

/// The escape shortfall of a placed board's busy parts.
fn escape_of(board: &mut Board) -> f64 {
    let fixed = vec![true; board.parts.len()];
    let rules = board.rules.clone();
    let probe = board.clone();
    let problem = problem_with_escape(&probe, &fixed, &rules, 1.0);
    Engine::new(&mut board.parts, &problem).escape_kept_total()
}

#[test]
fn weighing_escape_room_leaves_a_busy_part_less_short_of_it_than_not() {
    // The QFN and its eight resistors on a board of 11 mm, too small for every part to clear
    // every rule, so that the placer that does not weigh escape room leaves pads with no way
    // out: over four seeds, the one that weighs it leaves fewer (9 against 19 when written).
    let (mut weighed, mut not) = (0.0, 0.0);
    for seed in 0..4 {
        for (weight, total) in [(netsmith_pcb::ESCAPE_ROOM, &mut weighed), (0.0, &mut not)] {
            let mut board = busy_board(11.0);
            let fixed = vec![false; board.parts.len()];
            let rules = board.rules.clone();
            let probe = board.clone();
            let problem = Problem {
                seed,
                ..problem_with_escape(&probe, &fixed, &rules, weight)
            };
            anneal(&mut board.parts, &problem, 1.0);
            *total += escape_of(&mut board);
        }
    }
    eprintln!("escape shortfall over four seeds: {weighed} weighed, {not} not");
    assert!(weighed < not, "{weighed} weighed, {not} not");
}

#[test]
fn a_board_with_inner_signal_layers_overflows_less_than_one_with_the_outer_two_alone() {
    // The resistors of `crossing_nets` crowded into one corner: on four signal layers each bin
    // holds twice the track, so less of it overflows.
    let board = crossing_nets();
    let fixed = vec![false; board.parts.len()];
    let rules = board.rules.clone();
    let overflow_on = |layers: usize| {
        let mut parts = board.parts.clone();
        let problem = Problem {
            signal_layers: layers,
            ..problem_of(&board, &fixed, &rules)
        };
        let mut e = Engine::new(&mut parts, &problem);
        for i in 0..12 {
            put_at(&mut e, i, Pt::new(if i < 6 { 2.0 } else { 10.0 }, 2.0));
        }
        e.refresh_all();
        e.overflow_counted_afresh()
    };
    let (two, four) = (overflow_on(2), overflow_on(4));
    assert!(
        four < two && two > 0.0,
        "{two} mm on two layers, {four} mm on four"
    );
}

/// A problem for a board whose parts are borrowed mutably beside it.
fn problem_beside<'a>(
    nets: &'a [BoardNet],
    region: Rect,
    fixed: &'a [bool],
    rules: &'a Rules,
) -> Problem<'a> {
    Problem {
        nets,
        rules,
        planes: &[],
        signal_layers: 2,
        region,
        corner_radius: 0.0,
        fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        face_keepouts: &[],
        pairs: &[],
        cutouts: &[],
        open_outside: &[],
        pours: false,
        seed: 0,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: region,
        overflow: 1.0,
        escape_room: 0.0,
        pin_distances: &[],
        rf_nets: &[],
        net_currents: &[],
        routing: 0.0,
    }
}

/// A board of `n` resistors on a 20 mm square, each drawing a silkscreen line 2.5 mm to the
/// right of its origin, further out than its courtyard reaches.
fn resistors_with_a_far_silk_line(n: usize) -> Board {
    let names = vec![""; n];
    let mut board = resistors(&names);
    board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(20.0, 20.0));
    for part in &mut board.parts {
        part.footprint.graphics.push(netsmith_pcb::Graphic {
            layer: netsmith_pcb::Layer::Silk,
            shape: netsmith_pcb::Shape::Line {
                a: Pt::new(2.5, -1.0),
                b: Pt::new(2.5, 1.0),
            },
            stroke: 0.12,
            filled: false,
        });
        part.footprint.silk_texts.clear();
    }
    board
}

#[test]
fn silkscreen_beyond_a_courtyard_costs_a_little_where_it_is_nearer_the_edge_than_the_silk_space() {
    let mut board = resistors_with_a_far_silk_line(1);
    let rules = board.rules.clone();
    let fixed = [false];
    let problem = problem_beside(&board.nets, board.outline, &fixed, &rules);
    let mut e = Engine::new(&mut board.parts, &problem);
    let to = |x: f64, y: f64| Pose {
        x: (x / UNIT).round() as i64,
        y: (y / UNIT).round() as i64,
        turn: 0,
        text: 0,
    };
    let gap = rules.silk_to_pad;
    assert_eq!(gap, 0.15);
    // In the middle, nothing. With the line 0.3 mm from the edge (its stroke reaches to
    // 0.24), nothing.
    e.put(0, to(10.0, 10.0));
    assert_eq!(e.silk_edge_cost(0, false, false), 0.0);
    e.put(0, to(17.2, 10.0));
    assert_eq!(e.silk_edge_cost(0, false, false), 0.0);
    // The line 0.1 mm from the edge: its stroke is 0.04 mm from it, 0.11 mm short of the
    // space, and the cost is for that and a little more, as every silk conflict is.
    e.put(0, to(17.4, 10.0));
    let near = e.silk_edge_cost(0, false, false);
    assert!((near - W_SILK_EDGE * (0.11 + 0.05)).abs() < 1e-6, "{near}");
    // Further out it costs more, and it is soft: `single` holds it in `soft`, not `hard`,
    // while the courtyard is still inside its wall.
    e.put(0, to(17.6, 10.0));
    assert!(e.silk_edge_cost(0, false, false) > near);
    let single = e.single(0);
    assert!(single.soft > 0.0 && single.hard == 0.0, "{single:?}");
    // A part that hangs over an edge, or a side the design holds, is not charged.
    assert_eq!(e.silk_edge_cost(0, true, false), 0.0);
    e.models[0].overhangs = true;
    let hangs = e.single(0);
    assert!(hangs.soft < single.soft);
}

#[test]
fn silkscreen_is_kept_off_a_cutout_the_same_way() {
    let mut board = resistors_with_a_far_silk_line(1);
    let rules = board.rules.clone();
    let fixed = [false];
    let cutouts = [Rect::new(Pt::new(11.0, 5.0), Pt::new(14.0, 15.0))];
    let mut problem = problem_beside(&board.nets, board.outline, &fixed, &rules);
    problem.cutouts = &cutouts;
    let mut e = Engine::new(&mut board.parts, &problem);
    let to = |x: f64| Pose {
        x: (x / UNIT).round() as i64,
        y: (10.0 / UNIT).round() as i64,
        turn: 0,
        text: 0,
    };
    // The line at x = 11 - 0.05 is nearer the cutout than the silk space; at 5 it is not.
    e.put(0, to(11.0 - 0.05 - 2.5));
    assert!(e.silk_edge_cost(0, false, false) > 0.0);
    e.put(0, to(5.0));
    assert_eq!(e.silk_edge_cost(0, false, false), 0.0);
}

#[test]
fn the_designator_is_kept_off_a_cutout_as_silkscreen_is() {
    // The check measures the text to the edge of a cutout as to the outline's, so a box of
    // text on a cutout costs, and one `silk_to_pad` away does not.
    let mut board = resistors_with_a_far_silk_line(1);
    board.parts[0].footprint.graphics.clear();
    let rules = board.rules.clone();
    let fixed = [false];
    let cutouts = [Rect::new(Pt::new(11.0, 5.0), Pt::new(14.0, 15.0))];
    let mut problem = problem_beside(&board.nets, board.outline, &fixed, &rules);
    problem.cutouts = &cutouts;
    let mut e = Engine::new(&mut board.parts, &problem);
    let at = |x: f64| Pose {
        x: (x / UNIT).round() as i64,
        y: (10.0 / UNIT).round() as i64,
        turn: 0,
        text: 0,
    };
    e.put(0, at(5.0));
    let text = e.shape(0).text.expect("a resistor has a designator");
    // Moved so that the box's right side is 0.05 mm from the cutout, then 1 mm.
    let reach = text.max.x;
    e.put(0, at(11.0 - 0.05 - reach));
    assert!(e.silk_edge_cost(0, false, false) > 0.0);
    e.put(0, at(11.0 - 1.0 - reach));
    assert_eq!(e.silk_edge_cost(0, false, false), 0.0);
}

#[test]
fn a_connector_pulled_to_an_edge_stops_where_its_silkscreen_is_clear_of_it() {
    // A connector is pulled to the nearest edge, where its courtyard wall alone would leave a
    // frame drawn 1.5 mm round the courtyard over the edge. Over three seeds, the frame ends
    // no nearer than the silk space, and still at the edge: the term moves it in, no further.
    for seed in 0..3 {
        let mut board = resistors_with_a_far_silk_line(1);
        let part = &mut board.parts[0];
        part.declared_kind = Some(netsmith_core::PartKind::Connector);
        part.footprint.graphics.clear();
        let court = part.footprint.courtyard;
        part.footprint.graphics.push(netsmith_pcb::Graphic {
            layer: netsmith_pcb::Layer::Silk,
            shape: netsmith_pcb::Shape::Rect {
                a: Pt::new(court.min.x - 1.5, court.min.y - 1.5),
                b: Pt::new(court.max.x + 1.5, court.max.y + 1.5),
            },
            stroke: 0.12,
            filled: false,
        });
        let rules = board.rules.clone();
        let fixed = [false];
        let mut problem = problem_beside(&board.nets, board.outline, &fixed, &rules);
        problem.seed = seed;
        let out = anneal(&mut board.parts, &problem, 1.0);
        assert!(out.feasible, "seed {seed}");
        let court = board.parts[0].courtyard();
        let to_edge = (court.min.x - board.outline.min.x)
            .min(court.min.y - board.outline.min.y)
            .min(board.outline.max.x - court.max.x)
            .min(board.outline.max.y - court.max.y);
        // The frame is 1.5 mm out, its stroke 0.06 more: the silk space (0.15) beyond that.
        let kept = 1.5 + 0.06 + rules.silk_to_pad;
        assert!(
            to_edge > kept - 1e-6,
            "seed {seed}: {to_edge} mm from the edge"
        );
        assert!(
            to_edge < kept + 1.0,
            "seed {seed}: {to_edge} mm from the edge"
        );
    }
}

/// A QFN at 0.5 mm in the middle of a 20 mm square, fixed, with pad 1 on VDD and its middle
/// pad on GND (both power pins), a capacitor C2 between them that decouples it, and pad
/// `signal` on a signal to resistor R3 (issue #1010).
fn qfn_with_a_capacitor(signal: &str) -> Board {
    let mut board = qfn_board(20.0, 2);
    board.parts[0].power_pads = vec!["1".into(), "33".into()];
    board.parts[1].refdes = "C2".into();
    board.parts[1].kind = "Capacitor".into();
    board.parts[1].declared_kind = Some(netsmith_core::PartKind::Capacitor);
    let net = |name: &str, pads: &[(&str, &str)]| BoardNet {
        name: name.into(),
        pads: pads
            .iter()
            .map(|&(refdes, number)| netsmith_pcb::PadRef {
                refdes: refdes.into(),
                number: number.into(),
            })
            .collect(),
    };
    board.nets = vec![
        net("VDD", &[("R1", "1"), ("C2", "1")]),
        net("GND", &[("R1", "33"), ("C2", "2")]),
        net("SIG", &[("R1", signal), ("R3", "1")]),
    ];
    board
}

#[test]
fn a_decoupling_capacitor_may_stand_by_the_pins_it_decouples_but_not_against_the_others() {
    // Pad 8 is at the far end of pad 1's side.
    let board = qfn_with_a_capacitor("8");
    let fixed = [true, false, false];
    let rules = board.rules.clone();
    let problem = problem_of(&board, &fixed, &rules);
    let pad = |k: &str| {
        let part = &board.parts[0];
        part.pad_bounds(part.footprint.pads.iter().find(|p| p.number == k).unwrap())
    };
    let (one, eight) = (pad("1"), pad("8"));
    let mut parts = board.parts.clone();
    let mut e = Engine::new(&mut parts, &problem);
    assert!(e.near_ok[1], "C2 decouples the QFN");
    // The signal pad is the only one C2 keeps clear of: pad 1 and the middle pad are on its
    // nets, and the QFN's other pads are on none.
    let eight_index = board.parts[0]
        .footprint
        .pads
        .iter()
        .position(|p| p.number == "8")
        .unwrap();
    assert_eq!(e.pin_rooms.get(&(0, 1)), Some(&vec![eight_index]));
    assert_eq!(
        e.pin_rooms.get(&(0, 2)),
        None,
        "R3 is not exempt from the ring"
    );
    // C2 turned upright, its courtyard 0.1 mm off the ends of the pads of the QFN's left side.
    let stand = |e: &mut Engine, y: f64| {
        let upright = |x: f64, y: f64| Pose {
            x: (x / UNIT).round() as i64,
            y: (y / UNIT).round() as i64,
            turn: 1,
            text: 0,
        };
        e.put(1, upright(0.0, 0.0));
        let (court, at) = (e.court(1), e.pos(1));
        let x = at.x + one.min.x - 0.1 - court.max.x;
        let y = at.y + y - (court.min.y + court.max.y) / 2.0;
        e.put(1, upright(x, y));
    };
    // Level with pad 1, the pin it decouples, it is clear of the signal pad's room.
    stand(&mut e, one.centre().y);
    assert!(penetration(&e.court(1), &one, 0.2) > 0.0, "against pad 1");
    assert_eq!(e.ring_cost(0, 1).hard, 0.0);
    // Level with pad 8, the signal's, it is in its room: a hard cost, named in the reason.
    stand(&mut e, eight.centre().y);
    assert!(e.ring_cost(0, 1).hard > 0.0);
    let (rect, _, ring) = e.ring_depth(0, 1, 0.0).unwrap();
    assert!(!ring);
    assert_eq!(rect, eight.expanded(PIN_ROOM));
}

#[test]
fn the_placer_keeps_a_decoupling_capacitor_off_the_fine_pitch_parts_other_pins() {
    // The signal is on pad 2, next to pad 1, where C2 stands without the room (checked).
    // Over four seeds C2 ends by the QFN (closer than its escape ring reaches) but no nearer
    // than PIN_ROOM to the signal's pad.
    for seed in 0..4 {
        let mut board = qfn_with_a_capacitor("2");
        let outcome = run_with_the_qfn_fixed(&mut board, seed);
        assert!(outcome.feasible, "seed {seed}: {:?}", outcome.ended);
        let qfn = &board.parts[0];
        let two = qfn.pad_bounds(qfn.footprint.pads.iter().find(|p| p.number == "2").unwrap());
        let court = board.parts[1].courtyard();
        assert_eq!(
            penetration(&court, &two.expanded(PIN_ROOM), 0.0),
            0.0,
            "seed {seed}: C2 at {:?}",
            board.parts[1].at
        );
        assert!(
            penetration(&court, &ring_of(qfn), 0.0) > 0.0,
            "seed {seed}: C2 is by the QFN"
        );
        assert_eq!(board.check_placement(), vec![]);
    }
}

#[test]
fn a_part_that_may_turn_by_45_degrees_is_offered_all_eight_turns_and_another_four() {
    // Issue #1046: R1's want has a step of 45 degrees and leaves every turn and every place;
    // R2 has no want. The moves the annealer proposes turn R1 to each of the eight steps of
    // 45 degrees, and R2 only to the right angles.
    let board = resistors(&["", ""]);
    let fixed = vec![false; 2];
    let rules = board.rules.clone();
    let anywhere = netsmith_pcb::Reach {
        x: (f64::NEG_INFINITY, f64::INFINITY),
        y: (f64::NEG_INFINITY, f64::INFINITY),
    };
    let wants = vec![
        Some(Want {
            reach: vec![Some(anywhere); 8],
            base: 0.0,
            step: 45.0,
            holds_x: false,
            holds_y: false,
            overhangs: false,
        }),
        None,
    ];
    let mut problem = problem_of(&board, &fixed, &rules);
    problem.wants = &wants;
    let mut parts = board.parts.clone();
    let mut e = Engine::new(&mut parts, &problem);
    assert_eq!((e.models[0].turns(), e.models[1].turns()), (8, 4));
    let mut seen = [
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
    ];
    let mut rng = Rng::new(3);
    for _ in 0..5000 {
        for (i, pose) in e.propose(&mut rng, 20) {
            seen[i].insert(e.models[i].angle(pose.turn) as i64);
        }
    }
    let eighths: Vec<i64> = (0..8).map(|k| k * 45).collect();
    assert_eq!(seen[0].iter().copied().collect::<Vec<_>>(), eighths);
    assert_eq!(
        seen[1].iter().copied().collect::<Vec<_>>(),
        vec![0, 90, 180, 270]
    );
}
