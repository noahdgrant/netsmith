//! Lines and arcs found again in a section's chords.

use super::*;
use std::f64::consts::TAU;

/// The corners of a circle of radius `r` at `c`, chorded within 0.0025 mm (a quarter of the reader's, so that a corner added by a triangulation's diagonal is inside the circle by about the reader's 0.01),
/// counterclockwise, from angle `from` for `sweep` radians.
fn chords(c: P, r: f64, from: f64, sweep: f64) -> Vec<[f64; 2]> {
    let step = 2.0 * (1.0 - 0.0025 / r).acos();
    let n = (sweep.abs() / step).ceil() as usize;
    (0..=n)
        .map(|k| {
            let a = from + sweep * k as f64 / n as f64;
            [c.0 + r * a.cos(), c.1 + r * a.sin()]
        })
        .collect()
}

/// A rectangle from the origin, `w` x `h`, with corners of radius `r` (counterclockwise), the
/// straight edges with a corner every few millimetres as a triangulated wall has.
fn rounded(w: f64, h: f64, r: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let quarter = std::f64::consts::FRAC_PI_2;
    let straight = |out: &mut Vec<[f64; 2]>, a: [f64; 2], b: [f64; 2]| {
        for k in 0..3 {
            let t = f64::from(k) / 3.0;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    };
    straight(&mut out, [r, 0.0], [w - r, 0.0]);
    out.extend(chords((w - r, r), r, -quarter, quarter));
    straight(&mut out, [w, r], [w, h - r]);
    out.extend(chords((w - r, h - r), r, 0.0, quarter));
    straight(&mut out, [w - r, h], [r, h]);
    out.extend(chords((r, h - r), r, quarter, quarter));
    straight(&mut out, [0.0, h - r], [0.0, r]);
    out.extend(chords((r, r), r, 2.0 * quarter, quarter));
    out.dedup_by(|a, b| dist(pt(*a), pt(*b)) < 1e-9);
    out
}

fn arcs_and_lines(p: &Polygon) -> (usize, usize) {
    p.edge_counts()
}

#[test]
fn a_rounded_rectangle_comes_back_as_four_lines_and_four_arcs() {
    let p = polygon_of_ring(&rounded(60.0, 40.0, 2.0));
    assert_eq!(arcs_and_lines(&p), (4, 4), "{p:?}");
    let rr = p.as_rounded_rect().expect("a rounded rectangle");
    assert!((rr.width - 60.0).abs() < 0.01 && (rr.height - 40.0).abs() < 0.01);
    assert!((rr.radius - 2.0).abs() < 0.01, "{rr:?}");
    // The straight edges' extra corners are gone.
    assert_eq!(p.vertices.len(), 8);
}

#[test]
fn the_seam_of_the_ring_is_not_in_an_edge() {
    // The same ring started in the middle of an arc and of a straight edge.
    let ring = rounded(30.0, 20.0, 3.0);
    for shift in [1, 5, 11, ring.len() / 2, ring.len() - 2] {
        let mut turned = ring.clone();
        turned.rotate_left(shift);
        let p = polygon_of_ring(&turned);
        assert_eq!(arcs_and_lines(&p), (4, 4), "shift {shift}: {p:?}");
    }
}

#[test]
fn a_circle_comes_back_as_a_circle_whichever_way_it_runs() {
    let ring = chords((10.0, 5.0), 1.6, 0.3, TAU);
    let ring = &ring[..ring.len() - 1];
    let (c, d) = polygon_of_ring(ring).as_circle().expect("a circle");
    assert!(
        dist(c, (10.0, 5.0)) < 0.002 && (d - 3.2).abs() < 0.002,
        "{c:?} {d}"
    );
    let mut backwards = ring.to_vec();
    backwards.reverse();
    assert!(polygon_of_ring(&backwards).as_circle().is_some());
}

#[test]
fn a_polygon_with_corners_far_apart_stays_straight_edges() {
    // A regular octagon of radius 10: its corners are on a circle, and its sides are no chords
    // of the reader's.
    let ring: Vec<[f64; 2]> = (0..8)
        .map(|k| {
            let a = TAU * f64::from(k) / 8.0;
            [10.0 * a.cos(), 10.0 * a.sin()]
        })
        .collect();
    let p = polygon_of_ring(&ring);
    assert_eq!(arcs_and_lines(&p), (0, 8), "{p:?}");
    // And a triangle, and an L.
    assert_eq!(
        arcs_and_lines(&polygon_of_ring(&[[0.0, 0.0], [4.0, 0.0], [0.0, 3.0]])),
        (0, 3)
    );
    let l = polygon_of_ring(&[
        [0.0, 0.0],
        [10.0, 0.0],
        [10.0, 4.0],
        [4.0, 4.0],
        [4.0, 10.0],
        [0.0, 10.0],
    ]);
    assert_eq!(arcs_and_lines(&l), (0, 6));
}

#[test]
fn a_notch_in_a_rounded_rectangle_is_a_polygon_with_its_arcs() {
    // The pocket of the fixture: a rib standing 6 mm out of the bottom edge.
    let mut ring = rounded(60.0, 40.0, 2.0);
    let at = ring.iter().position(|p| p[0] > 29.0).unwrap();
    // Replace the points of the bottom edge between x 28 and 32 with the rib's three sides.
    let tail = ring.split_off(at);
    let after: Vec<[f64; 2]> = tail
        .into_iter()
        .skip_while(|p| p[1] == 0.0 && p[0] < 32.0)
        .collect();
    ring.extend([[28.0, 0.0], [28.0, 6.0], [32.0, 6.0], [32.0, 0.0]]);
    ring.extend(after);
    let p = polygon_of_ring(&ring);
    assert!(p.as_rounded_rect().is_none());
    assert_eq!(arcs_and_lines(&p), (4, 8), "{p:?}");
}

/// `ring` with a corner added after each, a third of the way to the one after next, where
/// `noisy` says: the section of a cylinder's side triangulated with diagonals between corners
/// that are not neighbours is inside its circle by up to the reader's chord tolerance.
fn with_diagonal_points(ring: &[[f64; 2]], noisy: impl Fn([f64; 2]) -> bool) -> Vec<[f64; 2]> {
    let n = ring.len();
    let mut out = Vec::new();
    for k in 0..n {
        let (a, c) = (ring[k], ring[(k + 2) % n]);
        out.push(a);
        if noisy(a) && noisy(c) && noisy(ring[(k + 1) % n]) {
            out.push([a[0] + (c[0] - a[0]) * 0.35, a[1] + (c[1] - a[1]) * 0.35]);
        }
    }
    out
}

#[test]
fn corners_on_the_diagonals_of_a_circle_do_not_hide_it() {
    let ring = chords((15.0, 20.0), 1.5, 0.0, TAU);
    let ring = with_diagonal_points(&ring[..ring.len() - 1], |_| true);
    let (c, d) = polygon_of_ring(&ring).as_circle().expect("a circle");
    assert!(
        dist(c, (15.0, 20.0)) < 0.002 && (d - 3.0).abs() < 0.004,
        "{c:?} {d}"
    );
}

#[test]
fn corners_on_the_diagonals_of_a_rounded_rectangle_do_not_hide_its_corners() {
    let corner = |p: [f64; 2]| (p[0] < 2.0 || p[0] > 58.0) && (p[1] < 2.0 || p[1] > 38.0);
    let ring = with_diagonal_points(&rounded(60.0, 40.0, 2.0), corner);
    let p = polygon_of_ring(&ring);
    assert_eq!(arcs_and_lines(&p), (4, 4), "{p:?}");
    let rr = p.as_rounded_rect().expect("a rounded rectangle");
    assert!((rr.radius - 2.0).abs() < 0.01, "{rr:?}");
}
