//! Tests of the annealer.

use netsmith_core::{Design, Net, Part, PinRef};

use crate::testlib::fixtures;
use netsmith_pcb::Board;

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
            class: netsmith_testkit::std_class("Resistor")
                .and_then(netsmith_core::PartClass::from_word),
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
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        cutouts: &[],
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
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        cutouts: &[],
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
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        cutouts: &[],
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
            region: outline,
            corner_radius: 0.0,
            fixed: &fixed,
            keepouts: &[],
            zones: &[],
            carried: &[],
            cutouts: &[],
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
        region: outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &[],
        cutouts: &[],
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
    tall.footprint.courtyard = Rect::new(Pt::new(-7.5, -2.5), Pt::new(3.5, 2.5));
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
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts,
        zones,
        cutouts: &[],
        pours: false,
        seed: 0,
        wants: &[],
        offsets: &[],
        overhangs: &[],
        outline: board.outline,
        carried: &[],
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
        drawn: false,
    });
    (board, zone)
}

fn run_with_carried(board: &mut Board, carried: &[netsmith_pcb::CarriedZone]) -> bool {
    let fixed = [false; 4];
    let problem = Problem {
        nets: &board.nets.clone(),
        rules: &board.rules.clone(),
        planes: &[],
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried,
        cutouts: &[],
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
        region: board.outline,
        corner_radius: 0.0,
        fixed: &fixed,
        keepouts: &[],
        zones: &[],
        carried: &carried,
        cutouts: &[],
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
