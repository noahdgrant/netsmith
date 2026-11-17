//! The section of an invented case (`fixtures::case_box`) at the board's plane and below it.

use super::*;
use crate::case::{CaseSolid, place};
use crate::step::read::{case_box_step, case_lid_step, case_shaped_step, read};
use netsmith_core::CaseFigures;

fn case_box() -> PlacedCase {
    let mesh = read(case_box_step().as_bytes()).expect("the fixture is STEP");
    place(
        "shell",
        "case_box.step",
        "",
        CaseFigures::default(),
        &mesh,
        [0.0; 3],
        [0.0; 3],
    )
}

fn bounds(r: &Ring) -> ([f64; 2], [f64; 2]) {
    let mut lo = [f64::MAX; 2];
    let mut hi = [f64::MIN; 2];
    for p in &r.points {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

fn by_depth(rings: &[Ring], depth: usize) -> Vec<&Ring> {
    rings.iter().filter(|r| r.depth == depth).collect()
}

#[test]
fn the_board_plane_cuts_the_wall_its_pocket_and_what_stands_through_the_board() {
    let rings = case_box().section(-0.8).unwrap();
    assert_eq!(
        rings
            .iter()
            .map(|r| (r.solid.as_str(), r.depth))
            .collect::<Vec<_>>(),
        [("wall", 0), ("wall", 1), ("pin", 2), ("block", 2),]
    );
    let (lo, hi) = bounds(&rings[0]);
    assert_eq!((lo, hi), ([-5.0, -5.0], [65.0, 45.0]));
    let pocket = &rings[1];
    let (lo, hi) = bounds(pocket);
    assert_eq!((lo, hi), ([0.0, 0.0], [60.0, 40.0]));
    // 60 x 40, less the four corners (2 mm) and the rib (4 x 6).
    let want = 2400.0 - 4.0 * (4.0 - std::f64::consts::PI) - 24.0;
    assert!(
        (area(&pocket.points) - want).abs() < 0.1,
        "{}",
        area(&pocket.points)
    );
    // Every ring turns counterclockwise.
    assert!(rings.iter().all(|r| area(&r.points) > 0.0));
    let (lo, hi) = bounds(&rings[3]);
    assert_eq!((lo, hi), ([41.0, 18.0], [49.0, 22.0]));
}

#[test]
fn a_plane_below_the_board_cuts_the_standoffs_and_the_holes_in_them() {
    let rings = case_box().section(-4.0).unwrap();
    let holes = by_depth(&rings, 3);
    assert_eq!(holes.len(), 2);
    let centre = |r: &Ring| {
        let (lo, hi) = bounds(r);
        [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, hi[0] - lo[0]]
    };
    let mut found: Vec<[f64; 3]> = holes.iter().map(|r| centre(r)).collect();
    found.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let close = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 0.02);
    assert!(close(found[0], [6.0, 6.0, 3.2]), "{found:?}");
    assert!(close(found[1], [54.0, 34.0, 2.5]), "{found:?}");
    // The standoffs themselves are posts in the pocket, as the pin and the block are.
    assert_eq!(by_depth(&rings, 2).len(), 4);
}

#[test]
fn a_plane_above_everything_has_no_section() {
    assert_eq!(case_box().section(20.0).unwrap(), []);
}

#[test]
fn a_solid_that_is_not_closed_is_an_error_that_says_where() {
    // A pyramid on a square base, missing one side face.
    let apex = [0.0, 0.0, 10.0];
    let (a, b, c, d) = (
        [-5.0, -5.0, 0.0],
        [5.0, -5.0, 0.0],
        [5.0, 5.0, 0.0],
        [-5.0, 5.0, 0.0],
    );
    let solid = CaseSolid::new(
        "tent",
        vec![[a, b, apex], [b, c, apex], [c, d, apex]],
        false,
    );
    let case = PlacedCase {
        name: "shell".into(),
        file: String::new(),
        sha256: String::new(),
        figures: CaseFigures::default(),
        checked: true,
        colour: None,
        solids: vec![solid],
    };
    let e = case.section(5.0).unwrap_err();
    let text = e.to_string();
    assert!(
        text.contains("`tent`") && text.contains("z = 5 mm"),
        "{text}"
    );
    assert!(text.contains("loose end"), "{text}");
}

/// How finely a slope or a curve is followed in these tests (mm).
const STEP: f64 = 0.5;

/// The file the tests of `netsmith import case model` (here and in `netsmith-tools`) read.
const COMMITTED: &str = include_str!("../../../tests/models/case_box.step");

#[test]
fn the_committed_model_is_what_the_fixture_writes() {
    assert_eq!(
        COMMITTED,
        case_box_step(),
        "run `cargo test -p netsmith-3d --lib write_case_box -- --ignored` to write it again"
    );
}

#[test]
#[ignore = "writes tests/models/case_box.step"]
fn write_case_box() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/models/case_box.step");
    std::fs::write(path, case_box_step()).unwrap();
}

/// The file the tests of `netsmith import case model --limits` read.
const COMMITTED_LID: &str = include_str!("../../../tests/models/case_lid.step");

#[test]
fn the_committed_lid_model_is_what_the_fixture_writes() {
    assert_eq!(
        COMMITTED_LID,
        case_lid_step(),
        "run `cargo test -p netsmith-3d --lib write_case_lid -- --ignored` to write it again"
    );
}

#[test]
#[ignore = "writes tests/models/case_lid.step"]
fn write_case_lid() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/models/case_lid.step");
    std::fs::write(path, case_lid_step()).unwrap();
}

fn case_lid() -> PlacedCase {
    let mesh = read(case_lid_step().as_bytes()).expect("the fixture is STEP");
    place(
        "shell",
        "case_lid.step",
        "",
        CaseFigures::default(),
        &mesh,
        [0.0; 3],
        [0.0; 3],
    )
}

#[test]
fn a_lid_a_rib_and_a_boss_are_limits_and_what_stands_through_the_board_is_not() {
    let case = case_lid();
    let rings = case.section(-0.8).unwrap();
    let pocket = rings.iter().find(|r| r.depth == 1).unwrap();
    let c = case.ceilings(&pocket.points, STEP);
    assert!(c.skipped.is_empty(), "{:?}", c.skipped);
    let zones: Vec<(f64, [f64; 2], [f64; 2], &str)> = c
        .zones
        .iter()
        .map(|z| (z.height, z.lo, z.hi, z.solid.as_str()))
        .collect();
    // The block stands from the board up to the lid: it is no limit. The rib (4 mm), the boss
    // (7 mm, a circle of 8 mm: slabs of at most a step, all inside the box of the circle) and
    // the lid (10 mm, over the whole pocket) are.
    let boss: Vec<_> = zones.iter().filter(|z| z.3 == "boss").collect();
    assert!((8..=16).contains(&boss.len()), "{boss:?}");
    assert!(boss.iter().all(|z| z.0 == 7.0));
    let area: f64 = boss
        .iter()
        .map(|z| (z.2[0] - z.1[0]) * (z.2[1] - z.1[1]))
        .sum();
    let circle = std::f64::consts::PI * 16.0;
    assert!(area > circle && area < 64.0 * 0.9, "{area}");
    let others: Vec<_> = zones.iter().filter(|z| z.3 != "boss").collect();
    assert_eq!(
        others,
        [
            &(4.0, [20.0, 0.0], [26.0, 40.0], "rib"),
            &(10.0, [0.0, 0.0], [60.0, 40.0], "lid"),
        ]
    );
}

#[test]
fn a_case_with_no_underside_above_the_board_has_no_limit() {
    let case = case_box();
    let rings = case.section(-0.8).unwrap();
    let pocket = rings.iter().find(|r| r.depth == 1).unwrap();
    let c = case.ceilings(&pocket.points, STEP);
    assert_eq!(c.zones, []);
    assert!(c.skipped.is_empty());
}

#[test]
fn a_solid_that_is_not_closed_at_a_plane_skips_that_plane_only() {
    // The lid's rib is cut open at its own plane: the others stay.
    let mut case = case_lid();
    let rings = case.section(-0.8).unwrap();
    let pocket = rings.iter().find(|r| r.depth == 1).unwrap().points.clone();
    let rib = case.solids.iter_mut().find(|s| s.name == "rib").unwrap();
    // Take away the rib's first side wall (the triangles that stand on x = 20).
    rib.triangles
        .retain(|t| !t.iter().all(|v| (v[0] - 20.0).abs() < 1e-9));
    let c = case.ceilings(&pocket, STEP);
    assert!(!c.skipped.is_empty());
    assert!(c.zones.iter().all(|z| z.solid != "rib"), "{:?}", c.zones);
    assert!(c.zones.iter().any(|z| z.solid == "lid"));
}

/// The file the tests of the shaped limits read.
const COMMITTED_SHAPED: &str = include_str!("../../../tests/models/case_shaped.step");

#[test]
fn the_committed_shaped_model_is_what_the_fixture_writes() {
    assert_eq!(
        COMMITTED_SHAPED,
        case_shaped_step(),
        "run `cargo test -p netsmith-3d --lib write_case_shaped -- --ignored` to write it again"
    );
}

#[test]
#[ignore = "writes tests/models/case_shaped.step"]
fn write_case_shaped() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/models/case_shaped.step");
    std::fs::write(path, case_shaped_step()).unwrap();
}

fn case_shaped() -> (PlacedCase, Vec<[f64; 2]>) {
    let mesh = read(case_shaped_step().as_bytes()).expect("the fixture is STEP");
    let case = place(
        "shell",
        "case_shaped.step",
        "",
        CaseFigures::default(),
        &mesh,
        [0.0; 3],
        [0.0; 3],
    );
    let rings = case.section(-0.8).unwrap();
    let pocket = rings.iter().find(|r| r.depth == 1).unwrap().points.clone();
    (case, pocket)
}

fn rects(c: &Ceilings, solid: &str) -> Vec<(f64, [f64; 2], [f64; 2])> {
    c.zones
        .iter()
        .filter(|z| z.solid == solid)
        .map(|z| (z.height, z.lo, z.hi))
        .collect()
}

#[test]
fn an_l_shaped_underside_is_two_rectangles_that_cover_exactly_it() {
    let (case, pocket) = case_shaped();
    let c = case.ceilings(&pocket, STEP);
    assert!(c.skipped.is_empty(), "{:?}", c.skipped);
    // The bracket's underside is at 9 mm: the L is a wide slab and a tall one, not its box
    // (5, 5) to (35, 35).
    assert_eq!(
        rects(&c, "bracket"),
        [
            (9.0, [5.0, 5.0], [35.0, 15.0]),
            (9.0, [5.0, 15.0], [15.0, 35.0]),
        ]
    );
}

#[test]
fn a_round_underside_is_slabs_inside_its_box_and_covering_the_circle() {
    let (case, pocket) = case_shaped();
    let c = case.ceilings_below(&pocket, 1.6, 2.0);
    let pad = rects(&c, "pad");
    // A circle of 8 mm: slabs of 2 mm (the two in the middle, as wide as the circle, are one
    // rectangle); the top of the pad is 3 mm under the bottom face.
    assert_eq!(pad.len(), 3, "{pad:?}");
    assert!(pad.iter().all(|z| z.0 == 3.0));
    let area: f64 = pad
        .iter()
        .map(|z| (z.2[0] - z.1[0]) * (z.2[1] - z.1[1]))
        .sum();
    assert!(area > std::f64::consts::PI * 16.0 && area < 64.0, "{area}");
    // Every point of the circle is in a slab.
    for k in 0..360 {
        let a = f64::from(k).to_radians();
        let p = [30.0 + 3.99 * a.cos(), 10.0 + 3.99 * a.sin()];
        assert!(
            pad.iter()
                .any(|z| (z.1[0]..=z.2[0]).contains(&p[0]) && (z.1[1]..=z.2[1]).contains(&p[1])),
            "{p:?} is not covered"
        );
    }
}

#[test]
fn a_slope_is_limited_by_its_steps_from_the_lowest_end_up() {
    let (case, pocket) = case_shaped();
    // The ramp's underside is 14 - (x - 40) * 6 / 16 mm: steps of 2 mm from 8 to 14.
    let c = case.ceilings(&pocket, 2.0);
    let ramp = rects(&c, "ramp");
    assert_eq!(ramp.len(), 3, "{ramp:?}");
    for (z, lo, hi) in &ramp {
        assert!(c.zones.iter().any(|zone| zone.height == *z && zone.sloped));
        assert_eq!((lo[1], hi[1], hi[0]), (5.0, 35.0, 56.0));
    }
    // Over every point of the ramp the lowest limit is no higher than the underside, and
    // lower by less than a step and the rounding of the box: parts are never let into the
    // ramp, and not kept far from it.
    for k in 0..=160 {
        let x = 40.0 + f64::from(k) * 0.1;
        let underside = 14.0 - (x - 40.0) * 6.0 / 16.0;
        let limit = c
            .zones
            .iter()
            .filter(|z| (z.lo[0]..=z.hi[0]).contains(&x) && (z.lo[1]..=z.hi[1]).contains(&20.0))
            .map(|z| z.height)
            .fold(f64::INFINITY, f64::min);
        assert!(
            limit <= underside + 1e-9,
            "x = {x}: {limit} over {underside}"
        );
        assert!(
            limit >= underside - 2.0 - 0.1,
            "x = {x}: {limit} under {underside}"
        );
    }
    let heights: Vec<f64> = ramp.iter().map(|r| r.0).collect();
    assert_eq!(heights, [8.0, 10.0, 12.0]);
    // The lid, the L and the ramp are all there, lowest first.
    let all: Vec<f64> = c.zones.iter().map(|z| z.height).collect();
    assert!(all.windows(2).all(|w| w[0] <= w[1]), "{all:?}");
    // A finer step gives more, lower-error steps; a flat face is never a slope.
    let fine = case.ceilings(&pocket, 0.5);
    assert_eq!(rects(&fine, "ramp").len(), 12);
    assert!(
        c.zones
            .iter()
            .filter(|z| !z.sloped)
            .all(|z| z.solid != "ramp")
    );
}

#[test]
fn the_floor_the_pad_and_the_post_limit_the_parts_on_the_bottom_face() {
    let (case, pocket) = case_shaped();
    let c = case.ceilings_below(&pocket, 1.6, 0.5);
    assert!(c.skipped.is_empty(), "{:?}", c.skipped);
    // The post stands from the floor to the board: it is no limit. The pad (3 mm under the
    // bottom face) and the floor (6.4 mm, over the whole pocket) are.
    let solids: Vec<&str> = c.zones.iter().map(|z| z.solid.as_str()).collect();
    assert!(!solids.contains(&"post"), "{solids:?}");
    assert!(solids.contains(&"pad"));
    assert_eq!(rects(&c, "floor"), [(6.4, [0.0, 0.0], [60.0, 40.0])]);
    let heights: Vec<f64> = c.zones.iter().map(|z| z.height).collect();
    assert!(heights.windows(2).all(|w| w[0] <= w[1]), "{heights:?}");
    // The lid's side is not here: nothing above the board is a limit for its bottom face.
    assert!(!solids.contains(&"lid") && !solids.contains(&"bracket"));
}

#[test]
fn a_face_with_a_draft_is_no_underside() {
    // A block hanging from the lid whose sides lean out by 2 degrees as they go up: their
    // faces point a little down, and limit nothing; its bottom face (9 mm) does.
    let (mut case, pocket) = case_shaped();
    let e = 5.0 * 2.0_f64.to_radians().tan();
    let b = |x: f64, y: f64| [x, y, 9.0];
    let t = |x: f64, y: f64| {
        [
            x + e * (x - 25.0).signum(),
            y + e * (y - 25.0).signum(),
            14.0,
        ]
    };
    let (b0, b1, b2, b3) = (b(20.0, 20.0), b(30.0, 20.0), b(30.0, 30.0), b(20.0, 30.0));
    let (t0, t1, t2, t3) = (t(20.0, 20.0), t(30.0, 20.0), t(30.0, 30.0), t(20.0, 30.0));
    let solid = CaseSolid::new(
        "frustum",
        vec![
            [b0, b2, b1],
            [b0, b3, b2],
            [t0, t1, t2],
            [t0, t2, t3],
            [b0, b1, t1],
            [b0, t1, t0],
            [b1, b2, t2],
            [b1, t2, t1],
            [b2, b3, t3],
            [b2, t3, t2],
            [b3, b0, t0],
            [b3, t0, t3],
        ],
        true,
    );
    case.solids.push(solid);
    let c = case.ceilings(&pocket, 0.5);
    let frustum = rects(&c, "frustum");
    assert_eq!(frustum.len(), 1, "{frustum:?}");
    assert_eq!(frustum[0].0, 9.0);
    assert!(
        c.zones
            .iter()
            .filter(|z| z.solid == "frustum")
            .all(|z| !z.sloped)
    );
}

#[test]
fn the_mirror_shows_the_board_from_below() {
    let (case, _) = case_shaped();
    let m = case.mirrored(1.6);
    // The floor's top (z = -8) is 6.4 mm over the mirrored board's top face, and every
    // triangle still faces out: the section of the mirror is a closed curve of the floor.
    let rings = m.section(6.405).unwrap();
    assert!(rings.iter().any(|r| r.solid == "floor"), "{rings:?}");
    assert_eq!(
        m.section(-0.8).unwrap().len(),
        case.section(-0.8).unwrap().len()
    );
}
