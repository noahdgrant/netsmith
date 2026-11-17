//! Fewer points for a ring that a filler cut finely (Douglas and Peucker's rule): the pours the
//! Gerbers hold are cut into pieces no further than 2 µm from their curves, tens of thousands of
//! points on a board with hundreds of pads that a drawing never needs. The editor's pours
//! (`netsmith-workspace`, `pours.rs`) and the board's picture (`picture.rs`, issue #1086) draw
//! their rings through [`simplify_ring`].

use crate::geometry::Pt;

/// A coordinate rounded to a micrometre, with no negative zero.
fn micron(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { 0.0 } else { r }
}

/// A closed ring with its points rounded to a micrometre, repeated points dropped, and each point
/// within `tolerance` (mm) of the line between its neighbours left out. The ring keeps its
/// direction; a ring that collapses to fewer than three points comes back with fewer.
pub fn simplify_ring(ring: &[Pt], tolerance: f64) -> Vec<Pt> {
    let mut points: Vec<Pt> = Vec::with_capacity(ring.len());
    for p in ring {
        let p = Pt::new(micron(p.x), micron(p.y));
        if points.last() != Some(&p) {
            points.push(p);
        }
    }
    while points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    if points.len() < 4 {
        // Three points on a line are no area.
        if points.len() == 3 && distance_to_segment(points[0], points[2], points[1]) <= tolerance {
            points.clear();
        }
        return points;
    }
    // A ring has no end to start from: keep the point farthest from the first, and the first,
    // and simplify the two open runs between them.
    let far = (1..points.len())
        .max_by(|&a, &b| {
            dist2(points[0], points[a])
                .partial_cmp(&dist2(points[0], points[b]))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(1);
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[far] = true;
    run(&points, 0, far, &mut keep, tolerance);
    let mut closing = points[far..].to_vec();
    closing.push(points[0]);
    let last = closing.len() - 1;
    let mut closing_keep = vec![false; closing.len()];
    closing_keep[0] = true;
    closing_keep[last] = true;
    run(&closing, 0, last, &mut closing_keep, tolerance);
    for (i, k) in closing_keep.iter().enumerate().take(last).skip(1) {
        keep[far + i] = *k;
    }
    points
        .into_iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(p))
        .collect()
}

/// Douglas and Peucker over `points[from..=to]`, marking in `keep` the points that stay, with an
/// explicit stack: a ring of tens of thousands of points must not recurse that deep.
fn run(points: &[Pt], from: usize, to: usize, keep: &mut [bool], tolerance: f64) {
    let mut stack = vec![(from, to)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (mut at, mut worst) = (a, 0.0);
        for (i, p) in points.iter().enumerate().take(b).skip(a + 1) {
            let d = distance_to_segment(points[a], points[b], *p);
            if d > worst {
                (at, worst) = (i, d);
            }
        }
        if worst > tolerance {
            keep[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
}

fn dist2(a: Pt, b: Pt) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

/// How far `p` is from the segment `a` to `b`.
pub fn distance_to_segment(a: Pt, b: Pt, p: Pt) -> f64 {
    let len2 = dist2(a, b);
    if len2 == 0.0 {
        return dist2(a, p).sqrt();
    }
    let t = (((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / len2).clamp(0.0, 1.0);
    dist2(Pt::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)), p).sqrt()
}
