//! Booleans of [`Polygon`]s for the meshes: the copper of a net joined into one piece, the mask
//! with its openings cut out, the silkscreen kept to the mask. It is the overlay the Gerbers' pen
//! uses (`i_overlay`, in `netsmith-fab`), which works in integers on a grid fitted to the
//! input's box, so the same polygons give the same result on every machine.
//!
//! The input rings may turn either way; each is turned so that an outer ring winds once round
//! what it holds and a hole takes that away (the non-zero rule), so overlapping polygons join.
//! The result's polygons do not overlap and each is simple, with its holes in it; slivers
//! smaller than [`MIN_AREA`] are dropped.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;

use super::{Polygon, ring_area};

type Ring = Vec<[f64; 2]>;

/// A piece of a result smaller than this, in mm² (a micrometre square), is left out: the sliver
/// where two outlines that meet were cut into straight pieces a little differently.
pub const MIN_AREA: f64 = 1e-6;

/// The rings of `polygons`, each outer ring counterclockwise and each hole clockwise.
fn contours(polygons: &[Polygon]) -> Vec<Ring> {
    let mut out = Vec::new();
    for p in polygons {
        for (k, r) in std::iter::once(&p.outer).chain(&p.holes).enumerate() {
            if r.len() < 3 {
                continue;
            }
            let a = ring_area(r);
            if a == 0.0 || !a.is_finite() {
                continue;
            }
            let mut r = r.clone();
            if (a > 0.0) != (k == 0) {
                r.reverse();
            }
            out.push(r);
        }
    }
    out
}

fn run(a: &[Polygon], b: &[Polygon], rule: OverlayRule) -> Vec<Polygon> {
    let (sa, sb) = (contours(a), contours(b));
    if sa.is_empty() && !matches!(rule, OverlayRule::Union) {
        return Vec::new();
    }
    if sa.is_empty() && sb.is_empty() {
        return Vec::new();
    }
    let shapes = sa.overlay(&sb, rule, FillRule::NonZero);
    shapes
        .into_iter()
        .filter_map(|shape| {
            let mut rings = shape.into_iter();
            let outer = rings.next()?;
            Some(Polygon {
                outer,
                holes: rings.collect(),
            })
        })
        .filter(|p| p.area() > MIN_AREA)
        .collect()
}

/// Everything that is in one of `polygons`.
pub fn union(polygons: &[Polygon]) -> Vec<Polygon> {
    run(polygons, &[], OverlayRule::Union)
}

/// What is in `a` and not in `b`.
pub fn difference(a: &[Polygon], b: &[Polygon]) -> Vec<Polygon> {
    if b.is_empty() {
        return union(a);
    }
    run(a, b, OverlayRule::Difference)
}

/// What is in both `a` and `b`.
pub fn intersection(a: &[Polygon], b: &[Polygon]) -> Vec<Polygon> {
    if b.is_empty() {
        return Vec::new();
    }
    run(a, b, OverlayRule::Intersect)
}

/// The area of `polygons`, each counted once (they must not overlap, as a boolean's result does
/// not).
pub fn area(polygons: &[Polygon]) -> f64 {
    polygons.iter().map(Polygon::area).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{circle, rounded_rect};

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon {
        Polygon::new(rounded_rect([x0, y0], [x1, y1], 0.0, 0))
    }

    #[test]
    fn overlapping_squares_join_and_cut() {
        let a = square(0.0, 0.0, 2.0, 2.0);
        let b = square(1.0, 1.0, 3.0, 3.0);
        assert!((area(&union(&[a.clone(), b.clone()])) - 7.0).abs() < 1e-9);
        assert!(
            (area(&difference(
                std::slice::from_ref(&a),
                std::slice::from_ref(&b)
            )) - 3.0)
                .abs()
                < 1e-9
        );
        assert!(
            (area(&intersection(
                std::slice::from_ref(&a),
                std::slice::from_ref(&b)
            )) - 1.0)
                .abs()
                < 1e-9
        );
        assert_eq!(union(&[a.clone(), b.clone()]).len(), 1);
        // The rings may turn either way.
        let mut r = b.clone();
        r.outer.reverse();
        assert!((area(&union(&[a, r])) - 7.0).abs() < 1e-9);
    }

    #[test]
    fn a_hole_stays_a_hole_unless_something_fills_it() {
        let ring = square(0.0, 0.0, 4.0, 4.0).with_hole(circle([2.0, 2.0], 1.0, 32));
        let u = union(std::slice::from_ref(&ring));
        assert_eq!((u.len(), u[0].holes.len()), (1, 1));
        let filled = union(&[ring.clone(), Polygon::new(circle([2.0, 2.0], 1.5, 32))]);
        assert_eq!((filled.len(), filled[0].holes.len()), (1, 0));
        let cut = difference(&[square(-1.0, -1.0, 5.0, 5.0)], &[ring]);
        // The frame round the ring, and the disc in its hole.
        assert_eq!(cut.len(), 2);
    }

    #[test]
    fn nothing_in_gives_nothing_out() {
        assert!(union(&[]).is_empty());
        assert!(difference(&[], &[square(0.0, 0.0, 1.0, 1.0)]).is_empty());
        assert!(intersection(&[square(0.0, 0.0, 1.0, 1.0)], &[]).is_empty());
    }
}
