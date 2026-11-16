//! The section of an invented case (`fixtures::case_box`) at the board's plane and below it.

use super::*;
use crate::case::{CaseSolid, place};
use crate::step::read::{case_box_step, case_lid_step, read};
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

/// The file the tests of `netsmith case import` (here and in `netsmith-tools`) read.
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

/// The file the tests of `netsmith case import --limits` read.
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
    let c = case.ceilings(&pocket.points);
    assert!(c.skipped.is_empty(), "{:?}", c.skipped);
    let zones: Vec<(f64, [f64; 2], [f64; 2], &str)> = c
        .zones
        .iter()
        .map(|z| (z.height, z.lo, z.hi, z.solid.as_str()))
        .collect();
    // The block stands from the board up to the lid: it is no limit. The rib (4 mm), the boss
    // (7 mm; the box of its circle) and the lid (10 mm, over the whole pocket) are.
    assert_eq!(
        zones,
        vec![
            (4.0, [20.0, 0.0], [26.0, 40.0], "rib"),
            (7.0, [41.0, 26.0], [49.0, 34.0], "boss"),
            (10.0, [0.0, 0.0], [60.0, 40.0], "lid"),
        ]
    );
}

#[test]
fn a_case_with_no_underside_above_the_board_has_no_limit() {
    let case = case_box();
    let rings = case.section(-0.8).unwrap();
    let pocket = rings.iter().find(|r| r.depth == 1).unwrap();
    let c = case.ceilings(&pocket.points);
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
    let c = case.ceilings(&pocket);
    assert!(!c.skipped.is_empty());
    assert!(c.zones.iter().all(|z| z.solid != "rib"), "{:?}", c.zones);
    assert!(c.zones.iter().any(|z| z.solid == "lid"));
}

#[test]
fn dump_pin() {
    let rings = case_box().section(-0.8).unwrap();
    for p in &rings[2].points {
        let r = ((p[0] - 15.0).powi(2) + (p[1] - 20.0).powi(2)).sqrt();
        println!("{:.4} {:.4} r={:.5}", p[0], p[1], r);
    }
}
