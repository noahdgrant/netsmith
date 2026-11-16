//! The rectangles that cover a region drawn by closed curves (the area a height limit holds):
//! the region's slabs between the heights where it changes, each slab's extent outward.
//!
//! A `Keepout` is a rectangle, so a region that is not one (an L, a round boss, a lid with a
//! hole through it) is several. The region is cut into slabs by horizontal lines: at every
//! horizontal edge of its curves, and, where a slab has an edge that is neither horizontal nor
//! vertical (a circle, a slanted side), at least every `step` mm. Where a slab has only
//! vertical edges its one rectangle is exactly the slab's region; elsewhere each rectangle
//! reaches as far as the region does within its slab, so the rectangles cover the region and
//! over-cover it by less than a slab's height at a curve.

use super::nm;

/// A corner pair: the corner nearest the origin and the opposite one.
pub(super) type Rect = ([f64; 2], [f64; 2]);

/// The most slabs one stretch between two horizontal edges is cut into, whatever `step` says.
const MAX_SLABS: usize = 64;

/// How near two heights are the same one (mm).
const SAME: f64 = 1e-6;

/// The rectangles that cover the region the curves `rings` bound (a point is in it when an odd
/// number of the curves enclose it), within the box `lo`..`hi`.
pub(super) fn rectangles(
    rings: &[&[[f64; 2]]],
    lo: [f64; 2],
    hi: [f64; 2],
    step: f64,
) -> Vec<Rect> {
    let mut ymin = f64::INFINITY;
    let mut ymax = f64::NEG_INFINITY;
    for p in rings.iter().flat_map(|r| r.iter()) {
        ymin = ymin.min(p[1]);
        ymax = ymax.max(p[1]);
    }
    let (ymin, ymax) = (ymin.max(lo[1]), ymax.min(hi[1]));
    if ymin >= ymax {
        return Vec::new();
    }
    // The heights where a horizontal edge lies cut the region into stretches.
    let mut cuts = vec![ymin, ymax];
    for r in rings {
        for i in 0..r.len() {
            let (a, b) = (r[i], r[(i + 1) % r.len()]);
            if (a[1] - b[1]).abs() < SAME && a[1] > ymin && a[1] < ymax {
                cuts.push(a[1]);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < SAME);
    let mut out = Vec::new();
    for w in cuts.windows(2) {
        let (y0, y1) = (w[0], w[1]);
        let slabs = if only_vertical_edges(rings, y0, y1) {
            1
        } else {
            (((y1 - y0) / step).ceil() as usize).clamp(1, MAX_SLABS)
        };
        for k in 0..slabs {
            let s0 = if k == 0 {
                y0
            } else {
                y0 + (y1 - y0) * k as f64 / slabs as f64
            };
            let s1 = if k + 1 == slabs {
                y1
            } else {
                y0 + (y1 - y0) * (k + 1) as f64 / slabs as f64
            };
            for (x0, x1) in extent(rings, s0, s1) {
                let (x0, x1) = (x0.max(lo[0]), x1.min(hi[0]));
                if x0 < x1 {
                    out.push(([nm(x0), nm(s0)], [nm(x1), nm(s1)]));
                }
            }
        }
    }
    out
}

/// Whether every edge that enters the stretch between `y0` and `y1` stands upright (so the
/// region has the same extent all along it).
fn only_vertical_edges(rings: &[&[[f64; 2]]], y0: f64, y1: f64) -> bool {
    rings.iter().all(|r| {
        (0..r.len()).all(|i| {
            let (a, b) = (r[i], r[(i + 1) % r.len()]);
            let (lo, hi) = (a[1].min(b[1]), a[1].max(b[1]));
            hi <= y0 + SAME || lo >= y1 - SAME || (a[0] - b[0]).abs() < SAME
        })
    })
}

/// The stretches of x the region has somewhere in the slab `y0`..`y1`: its crossings with the
/// slab's two sides and with each height a corner stands at inside it, joined where they meet.
fn extent(rings: &[&[[f64; 2]]], y0: f64, y1: f64) -> Vec<(f64, f64)> {
    let inset = ((y1 - y0) / 4.0).min(SAME);
    let mut ys = vec![y0 + inset, y1 - inset];
    for p in rings.iter().flat_map(|r| r.iter()) {
        if p[1] > y0 + inset && p[1] < y1 - inset {
            ys.push(p[1]);
        }
    }
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    let mut spans: Vec<(f64, f64)> = ys.into_iter().flat_map(|y| spans_at(rings, y)).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut joined: Vec<(f64, f64)> = Vec::new();
    for s in spans {
        match joined.last_mut() {
            Some(last) if s.0 <= last.1 + SAME => last.1 = last.1.max(s.1),
            _ => joined.push(s),
        }
    }
    joined
}

/// The stretches of the line at height `y` that are inside the region (the even-odd rule).
fn spans_at(rings: &[&[[f64; 2]]], y: f64) -> Vec<(f64, f64)> {
    let mut xs: Vec<f64> = Vec::new();
    for r in rings {
        for i in 0..r.len() {
            let (a, b) = (r[i], r[(i + 1) % r.len()]);
            if (a[1] > y) != (b[1] > y) {
                xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
    }
    xs.sort_by(f64::total_cmp);
    xs.chunks_exact(2).map(|c| (c[0], c[1])).collect()
}

/// `rects` with every two that have the same x range, and whose y ranges meet or overlap,
/// joined into one: the slabs of a stretch that rounding made alike are one rectangle.
pub(super) fn join(mut rects: Vec<Rect>) -> Vec<Rect> {
    loop {
        let mut merged = false;
        'search: for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                let (a, b) = (rects[i], rects[j]);
                if (a.0[0] - b.0[0]).abs() < SAME
                    && (a.1[0] - b.1[0]).abs() < SAME
                    && a.0[1] <= b.1[1] + SAME
                    && b.0[1] <= a.1[1] + SAME
                {
                    rects[i] = ([a.0[0], a.0[1].min(b.0[1])], [a.1[0], a.1[1].max(b.1[1])]);
                    rects.remove(j);
                    merged = true;
                    break 'search;
                }
            }
        }
        if !merged {
            return rects;
        }
    }
}
