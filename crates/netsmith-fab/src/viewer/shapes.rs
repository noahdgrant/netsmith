//! A file's objects combined into what it draws: polygons with holes.
//!
//! Each object (a flash, a stroke, a region) is first made one shape by the rule of its
//! primitives: a region's contours by the even-odd rule, an aperture's primitives in order, one
//! with its exposure off taking away from the ones before it in the same aperture. Then the
//! objects are combined in file order: a dark object adds, a clear one takes away from what is
//! dark before it. Runs of one polarity are gathered first, so a file of pads with a few
//! clearances is a few polygon operations, not one for each object (as `netsmith_geom::Polygons`
//! does for the writer).

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;

use crate::raster::{Item, Rule};

type Ring = Vec<[f64; 2]>;

/// A polygon with holes, in the files' frame (mm, y up). `outer` turns counter-clockwise
/// (its shoelace sum is positive) and each hole the other way.
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub outer: Vec<(f64, f64)>,
    pub holes: Vec<Vec<(f64, f64)>>,
}

impl Area {
    /// Its area in mm², the holes taken away.
    pub fn size(&self) -> f64 {
        let a = |p: &[(f64, f64)]| (sum(p) / 2.0).abs();
        a(&self.outer) - self.holes.iter().map(|h| a(h)).sum::<f64>()
    }
}

/// Twice the signed area (positive counter-clockwise, y up).
fn sum(p: &[(f64, f64)]) -> f64 {
    (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum()
}

fn ring_sum(p: &Ring) -> f64 {
    (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

fn rings(polys: &[Vec<(f64, f64)>]) -> Vec<Ring> {
    polys
        .iter()
        .filter(|p| p.len() >= 3)
        .map(|p| p.iter().map(|q| [q.0, q.1]).collect())
        .collect()
}

fn flat(shapes: Vec<Vec<Ring>>) -> Vec<Ring> {
    shapes.into_iter().flatten().collect()
}

/// One primitive's polygons as rings that the non-zero rule fills the same: outer rings
/// positive, holes negative.
fn normal(polys: &[Vec<(f64, f64)>], rule: Rule) -> Vec<Ring> {
    let rs = rings(polys);
    if rs.is_empty() {
        return rs;
    }
    // The common case, a flash or a stroke of one turn: its rings turned positive.
    if rule == Rule::NonZero {
        let signs: Vec<f64> = rs.iter().map(ring_sum).collect();
        if signs.iter().all(|s| *s > 0.0) {
            return rs;
        }
        if signs.iter().all(|s| *s < 0.0) {
            return rs
                .into_iter()
                .map(|mut r| {
                    r.reverse();
                    r
                })
                .collect();
        }
    }
    let fill = match rule {
        Rule::NonZero => FillRule::NonZero,
        Rule::EvenOdd => FillRule::EvenOdd,
    };
    flat(rs.simplify_shape(fill))
}

/// One object as rings (outer positive, holes negative).
fn object(item: &Item) -> Vec<Ring> {
    if let [p] = item.prims.as_slice() {
        return if p.on {
            normal(&p.polys, p.rule)
        } else {
            Vec::new()
        };
    }
    let mut shape: Vec<Ring> = Vec::new();
    for p in &item.prims {
        let r = normal(&p.polys, p.rule);
        if p.on {
            shape.extend(r);
        } else if !shape.is_empty() && !r.is_empty() {
            shape = flat(shape.overlay(&r, OverlayRule::Difference, FillRule::NonZero));
        }
    }
    shape
}

/// What the objects draw, combined in order as their polarity says.
pub(crate) fn combine(items: &[Item]) -> Vec<Area> {
    let mut dark: Vec<Ring> = Vec::new();
    let mut clear: Vec<Ring> = Vec::new();
    for item in items {
        let r = object(item);
        if item.dark {
            if !clear.is_empty() {
                dark = flat(dark.overlay(&clear, OverlayRule::Difference, FillRule::NonZero));
                clear.clear();
            }
            dark.extend(r);
        } else if !dark.is_empty() {
            clear.extend(r);
        }
    }
    if !clear.is_empty() {
        dark = flat(dark.overlay(&clear, OverlayRule::Difference, FillRule::NonZero));
    }
    let none: Vec<Ring> = Vec::new();
    dark.overlay(&none, OverlayRule::Union, FillRule::NonZero)
        .into_iter()
        .filter_map(|shape| {
            let mut it = shape
                .into_iter()
                .map(|r| r.into_iter().map(|[x, y]| (x, y)).collect::<Vec<_>>());
            let outer = turned(it.next()?, true);
            let holes = it.map(|h| turned(h, false)).collect();
            Some(Area { outer, holes })
        })
        .collect()
}

fn turned(mut ring: Vec<(f64, f64)>, positive: bool) -> Vec<(f64, f64)> {
    if (sum(&ring) > 0.0) != positive {
        ring.reverse();
    }
    ring
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::read_gerber;

    fn areas(body: &str) -> Vec<Area> {
        let text = format!("%FSLAX24Y24*%\n%MOMM*%\n{body}M02*\n");
        let mut items = Vec::new();
        read_gerber(&text, 100.0, &mut |i| items.push(i.clone())).unwrap();
        combine(&items)
    }

    fn total(a: &[Area]) -> f64 {
        a.iter().map(Area::size).sum()
    }

    #[test]
    fn a_clear_object_cuts_a_hole_that_is_a_hole_of_the_area() {
        let a = areas("%ADD10R,4X4*%\n%ADD11C,2*%\nD10*\nX0Y0D03*\n%LPC*%\nD11*\nX0Y0D03*\n");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].holes.len(), 1);
        let want = 16.0 - std::f64::consts::PI;
        assert!((total(&a) - want).abs() < 1e-3, "{}", total(&a));
    }

    #[test]
    fn dark_after_clear_is_drawn_over_the_hole() {
        let a = areas(
            "%ADD10R,4X4*%\n%ADD11C,2*%\n%ADD12R,1X1*%\nD10*\nX0Y0D03*\n%LPC*%\nD11*\nX0Y0D03*\n%LPD*%\nD12*\nX0Y0D03*\n",
        );
        let want = 16.0 - std::f64::consts::PI + 1.0;
        assert!((total(&a) - want).abs() < 1e-3, "{}", total(&a));
    }

    #[test]
    fn a_region_is_filled_even_odd_and_overlaps_join() {
        // A square with a square contour inside it: a frame.
        let a = areas(
            "G36*\nX0Y0D02*\nX40000Y0D01*\nX40000Y40000D01*\nX0Y40000D01*\nX0Y0D01*\nX10000Y10000D02*\nX30000Y10000D01*\nX30000Y30000D01*\nX10000Y30000D01*\nX10000Y10000D01*\nG37*\n%ADD10R,1X1*%\nD10*\nX0Y0D03*\n",
        );
        // 16 - 4, and the pad's three quarters outside the frame.
        assert!((total(&a) - (12.0 + 0.75)).abs() < 1e-6, "{}", total(&a));
    }
}
