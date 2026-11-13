//! Ear clipping of a simple polygon with holes, after the approach of Mapbox's `earcut`
//! (ISC licence; the algorithm, not its code): each hole is joined to the outer ring by a
//! bridge (two coincident edges) to make one ring, then ears are cut from that ring. A grid of
//! the points makes the test "is any point inside this ear" look only near the ear, so a board
//! with hundreds of round holes takes milliseconds, not seconds.
//!
//! The input is a simple polygon (no edge crosses another) whose holes lie inside it and do not
//! cross each other. A ring that breaks the rule still ends: when no ear is found, the most
//! convex corner is cut anyway, so the result has the right number of triangles but may
//! overlap. Every step is plain `f64` arithmetic in a fixed order: the same points give the same
//! triangles on every machine.

/// One point of the ring being cut.
#[derive(Clone, Copy)]
struct Node {
    /// The index of the point in the caller's list (outer ring first, then each hole).
    i: usize,
    x: f64,
    y: f64,
    prev: usize,
    next: usize,
    removed: bool,
}

struct Ring {
    nodes: Vec<Node>,
    grid: Grid,
    /// Whether a point on a straight run stays a corner (`triangulate_keeping`).
    keep_straight: bool,
}

/// Twice the signed area of the triangle `p q r`: positive when it turns left (counterclockwise).
fn orient(p: &Node, q: &Node, r: &Node) -> f64 {
    (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x)
}

fn equals(a: &Node, b: &Node) -> bool {
    a.x == b.x && a.y == b.y
}

/// Whether `(px, py)` is in the counterclockwise triangle `a b c` or on its edge.
fn in_ccw_triangle(a: (f64, f64), b: (f64, f64), c: (f64, f64), p: (f64, f64)) -> bool {
    let cross =
        |u: (f64, f64), v: (f64, f64)| (v.0 - u.0) * (p.1 - u.1) - (v.1 - u.1) * (p.0 - u.0);
    cross(a, b) >= 0.0 && cross(b, c) >= 0.0 && cross(c, a) >= 0.0
}

/// Whether `p` is in the triangle `a b c` of either turn, or on its edge.
fn in_triangle(a: (f64, f64), b: (f64, f64), c: (f64, f64), p: (f64, f64)) -> bool {
    in_ccw_triangle(a, b, c, p) || in_ccw_triangle(a, c, b, p)
}

/// A uniform grid over the points' box: the nodes in each cell, removed ones included.
struct Grid {
    x0: f64,
    y0: f64,
    inv: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<usize>>,
}

impl Grid {
    fn new(points: &[(f64, f64)]) -> Grid {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in points {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let n = points.len().max(1) as f64;
        let w = (x1 - x0).max(1e-9);
        let h = (y1 - y0).max(1e-9);
        // About one point a cell.
        let cell = (w * h / n).sqrt().max(w.max(h) / 1024.0);
        let nx = ((w / cell) as usize).clamp(1, 1024);
        let ny = ((h / cell) as usize).clamp(1, 1024);
        Grid {
            x0,
            y0,
            inv: 1.0 / cell,
            nx,
            ny,
            cells: vec![Vec::new(); nx * ny],
        }
    }

    fn cell(&self, x: f64, y: f64) -> (usize, usize) {
        let cx = ((x - self.x0) * self.inv).max(0.0) as usize;
        let cy = ((y - self.y0) * self.inv).max(0.0) as usize;
        (cx.min(self.nx - 1), cy.min(self.ny - 1))
    }

    fn insert(&mut self, node: usize, x: f64, y: f64) {
        let (cx, cy) = self.cell(x, y);
        self.cells[cy * self.nx + cx].push(node);
    }
}

impl Ring {
    fn push(&mut self, i: usize, x: f64, y: f64, last: Option<usize>) -> usize {
        let n = self.nodes.len();
        let mut node = Node {
            i,
            x,
            y,
            prev: n,
            next: n,
            removed: false,
        };
        if let Some(l) = last {
            let after = self.nodes[l].next;
            node.prev = l;
            node.next = after;
            self.nodes[after].prev = n;
            self.nodes[l].next = n;
        }
        self.nodes.push(node);
        self.grid.insert(n, x, y);
        n
    }

    fn remove(&mut self, p: usize) {
        let Node { prev, next, .. } = self.nodes[p];
        self.nodes[prev].next = next;
        self.nodes[next].prev = prev;
        self.nodes[p].removed = true;
    }

    fn o(&self, a: usize, b: usize, c: usize) -> f64 {
        orient(&self.nodes[a], &self.nodes[b], &self.nodes[c])
    }

    fn xy(&self, a: usize) -> (f64, f64) {
        (self.nodes[a].x, self.nodes[a].y)
    }

    /// Links the points `range` as a ring turning counterclockwise (`ccw`) or clockwise, and
    /// returns one of its nodes, or `None` when fewer than three are left.
    fn link(
        &mut self,
        points: &[(f64, f64)],
        range: std::ops::Range<usize>,
        ccw: bool,
    ) -> Option<usize> {
        let ring = &points[range.clone()];
        let mut area = 0.0;
        for k in 0..ring.len() {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            area += (b.0 - a.0) * (b.1 + a.1);
        }
        // `area` is minus twice the signed area.
        let forward = (area < 0.0) == ccw;
        let mut last = None;
        let order: Vec<usize> = if forward {
            range.collect()
        } else {
            range.rev().collect()
        };
        for i in order {
            let (x, y) = points[i];
            last = Some(self.push(i, x, y, last));
        }
        let last = last?;
        let first = self.nodes[last].next;
        if equals(&self.nodes[last], &self.nodes[first]) && last != first {
            self.remove(last);
            return self.filter(first, None);
        }
        self.filter(last, None)
    }

    /// Removes repeated points and points on a straight line from `start` round to `end`, and
    /// returns a node still in the ring, or `None` when the ring is gone.
    fn filter(&mut self, start: usize, end: Option<usize>) -> Option<usize> {
        let mut end = end.unwrap_or(start);
        let mut p = start;
        loop {
            let Node { prev, next, .. } = self.nodes[p];
            if next == p {
                return None;
            }
            if equals(&self.nodes[p], &self.nodes[next])
                || (!self.keep_straight && self.o(prev, p, next) == 0.0)
            {
                self.remove(p);
                p = prev;
                end = prev;
                if self.nodes[p].next == p || self.nodes[self.nodes[p].next].next == p {
                    return None;
                }
                continue;
            }
            p = next;
            if p == end {
                return Some(end);
            }
        }
    }

    fn locally_inside(&self, a: usize, b: usize) -> bool {
        let Node { prev, next, .. } = self.nodes[a];
        if self.o(prev, a, next) > 0.0 {
            self.o(a, b, next) <= 0.0 && self.o(a, prev, b) <= 0.0
        } else {
            self.o(a, b, prev) > 0.0 || self.o(a, next, b) > 0.0
        }
    }

    fn sector_contains_sector(&self, m: usize, p: usize) -> bool {
        let (mp, mn) = (self.nodes[m].prev, self.nodes[m].next);
        let (pp, pn) = (self.nodes[p].prev, self.nodes[p].next);
        self.o(mp, m, pp) > 0.0 && self.o(pn, m, mn) > 0.0
    }

    /// The node of the outer ring to join the hole's leftmost node `hole` to.
    fn bridge(&self, hole: usize, outer: usize) -> Option<usize> {
        let (hx, hy) = self.xy(hole);
        let mut p = outer;
        let mut qx = f64::NEG_INFINITY;
        let mut m = None;
        loop {
            let next = self.nodes[p].next;
            if equals(&self.nodes[hole], &self.nodes[p]) {
                return Some(p);
            }
            let (px, py) = self.xy(p);
            let (nx, ny) = self.xy(next);
            // An edge of the counterclockwise outer ring that goes down, crossed by a ray from
            // the hole to the left.
            if hy <= py && hy >= ny && ny != py {
                let x = px + (hy - py) * (nx - px) / (ny - py);
                if x <= hx && x > qx {
                    qx = x;
                    m = Some(if px < nx { p } else { next });
                    if x == hx {
                        return m;
                    }
                }
            }
            p = next;
            if p == outer {
                break;
            }
        }
        let mut m = m?;
        let stop = m;
        let (mx, my) = self.xy(m);
        let mut tan_min = f64::INFINITY;
        p = m;
        loop {
            let (px, py) = self.xy(p);
            let (ax, cx) = if hy < my { (hx, qx) } else { (qx, hx) };
            if hx >= px
                && px >= mx
                && hx != px
                && in_triangle((ax, hy), (mx, my), (cx, hy), (px, py))
            {
                let tan = (hy - py).abs() / (hx - px);
                let (bx, _) = self.xy(m);
                if self.locally_inside(p, hole)
                    && (tan < tan_min
                        || (tan == tan_min
                            && (px > bx || (px == bx && self.sector_contains_sector(m, p)))))
                {
                    m = p;
                    tan_min = tan;
                }
            }
            p = self.nodes[p].next;
            if p == stop {
                break;
            }
        }
        Some(m)
    }

    /// Joins `a` and `b` by two coincident edges, which splits a ring in two or joins two
    /// rings in one, and returns the copy of `b`.
    fn split(&mut self, a: usize, b: usize) -> usize {
        let an = self.nodes[a].next;
        let bp = self.nodes[b].prev;
        let (ai, ax, ay) = (self.nodes[a].i, self.nodes[a].x, self.nodes[a].y);
        let (bi, bx, by) = (self.nodes[b].i, self.nodes[b].x, self.nodes[b].y);
        let a2 = self.push(ai, ax, ay, None);
        let b2 = self.push(bi, bx, by, None);
        self.nodes[a].next = b;
        self.nodes[b].prev = a;
        self.nodes[a2].next = an;
        self.nodes[an].prev = a2;
        self.nodes[b2].next = a2;
        self.nodes[a2].prev = b2;
        self.nodes[bp].next = b2;
        self.nodes[b2].prev = bp;
        b2
    }

    fn is_ear(&self, ear: usize) -> bool {
        let Node {
            prev: a, next: c, ..
        } = self.nodes[ear];
        let b = ear;
        if self.o(a, b, c) <= 0.0 {
            return false;
        }
        let (pa, pb, pc) = (self.xy(a), self.xy(b), self.xy(c));
        let (x0, x1) = (pa.0.min(pb.0).min(pc.0), pa.0.max(pb.0).max(pc.0));
        let (y0, y1) = (pa.1.min(pb.1).min(pc.1), pa.1.max(pb.1).max(pc.1));
        let (cx0, cy0) = self.grid.cell(x0, y0);
        let (cx1, cy1) = self.grid.cell(x1, y1);
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                for &p in &self.grid.cells[cy * self.grid.nx + cx] {
                    let node = &self.nodes[p];
                    if node.removed || p == a || p == b || p == c {
                        continue;
                    }
                    let pp = (node.x, node.y);
                    if pp.0 >= x0
                        && pp.0 <= x1
                        && pp.1 >= y0
                        && pp.1 <= y1
                        && pp != pa
                        && in_ccw_triangle(pa, pb, pc, pp)
                        && self.o(node.prev, p, node.next) <= 0.0
                    {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn cut(&mut self, start: usize, out: &mut Vec<[usize; 3]>) {
        let mut ear = start;
        let mut stop = ear;
        let mut pass = 0;
        while self.nodes[ear].prev != self.nodes[ear].next {
            let Node { prev, next, .. } = self.nodes[ear];
            if self.is_ear(ear) {
                out.push([self.nodes[prev].i, self.nodes[ear].i, self.nodes[next].i]);
                self.remove(ear);
                ear = self.nodes[next].next;
                stop = ear;
                pass = 0;
                continue;
            }
            ear = next;
            if ear == stop {
                if pass == 0 {
                    // Straight runs and repeats left by earlier cuts can hide every ear.
                    match self.filter(ear, None) {
                        Some(e) => ear = e,
                        None => return,
                    }
                    stop = ear;
                    pass = 1;
                } else {
                    // No ear: the ring is not simple. Cut the most convex corner so the loop
                    // ends, with a triangle that may overlap another.
                    let mut best = ear;
                    let mut best_o = f64::NEG_INFINITY;
                    let mut p = ear;
                    loop {
                        let Node { prev, next, .. } = self.nodes[p];
                        let o = self.o(prev, p, next);
                        if o > best_o {
                            best_o = o;
                            best = p;
                        }
                        p = next;
                        if p == ear {
                            break;
                        }
                    }
                    let Node { prev, next, .. } = self.nodes[best];
                    // A corner on a straight run would be a triangle with no area: it is
                    // dropped instead.
                    if best_o > 0.0 || !self.keep_straight {
                        out.push([self.nodes[prev].i, self.nodes[best].i, self.nodes[next].i]);
                    }
                    self.remove(best);
                    ear = next;
                    stop = ear;
                    pass = 0;
                }
            }
        }
    }
}

/// The triangles of the polygon `outer` with `holes`, as indices into the points of `outer`
/// followed by those of each hole in turn, each triangle counterclockwise (seen from +z). The
/// rings may turn either way and need not repeat their first point.
pub(crate) fn triangulate(outer: &[[f64; 2]], holes: &[Vec<[f64; 2]>]) -> Vec<[usize; 3]> {
    cut(outer, holes, false)
}

/// As [`triangulate`], but every point stays a corner of some triangle, also one on a straight
/// run between its neighbours: a side shared with another face keeps all its points, so the two
/// meet with no crack. Only a point that repeats its neighbour is dropped.
pub(crate) fn triangulate_keeping(outer: &[[f64; 2]], holes: &[Vec<[f64; 2]>]) -> Vec<[usize; 3]> {
    cut(outer, holes, true)
}

fn cut(outer: &[[f64; 2]], holes: &[Vec<[f64; 2]>], keep_straight: bool) -> Vec<[usize; 3]> {
    let mut points: Vec<(f64, f64)> = outer.iter().map(|p| (p[0], p[1])).collect();
    let mut ranges = Vec::with_capacity(1 + holes.len());
    ranges.push(0..outer.len());
    for h in holes {
        let s = points.len();
        points.extend(h.iter().map(|p| (p[0], p[1])));
        ranges.push(s..points.len());
    }
    let mut ring = Ring {
        nodes: Vec::with_capacity(points.len() * 3 / 2),
        grid: Grid::new(&points),
        keep_straight,
    };
    let mut out = Vec::new();
    let Some(mut start) = ring.link(&points, ranges[0].clone(), true) else {
        return out;
    };
    // Each hole's leftmost node, the holes from left to right.
    let mut lefts = Vec::new();
    for r in &ranges[1..] {
        let Some(h) = ring.link(&points, r.clone(), false) else {
            continue;
        };
        let mut left = h;
        let mut p = h;
        loop {
            let (n, l) = (&ring.nodes[p], &ring.nodes[left]);
            if n.x < l.x || (n.x == l.x && n.y < l.y) {
                left = p;
            }
            p = ring.nodes[p].next;
            if p == h {
                break;
            }
        }
        lefts.push(left);
    }
    lefts.sort_by(|&a, &b| {
        let (na, nb) = (&ring.nodes[a], &ring.nodes[b]);
        na.x.total_cmp(&nb.x)
            .then(na.y.total_cmp(&nb.y))
            .then(na.i.cmp(&nb.i))
    });
    for hole in lefts {
        let Some(bridge) = ring.bridge(hole, start) else {
            continue;
        };
        let back = ring.split(bridge, hole);
        let _ = ring.filter(back, Some(ring.nodes[back].next));
        match ring.filter(bridge, Some(ring.nodes[bridge].next)) {
            Some(s) => start = s,
            None => return out,
        }
    }
    ring.cut(start, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(points: &[[f64; 2]], tris: &[[usize; 3]]) -> f64 {
        tris.iter()
            .map(|&[a, b, c]| {
                let (a, b, c) = (points[a], points[b], points[c]);
                0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
            })
            .sum()
    }

    fn all(outer: &[[f64; 2]], holes: &[Vec<[f64; 2]>]) -> Vec<[f64; 2]> {
        let mut v = outer.to_vec();
        for h in holes {
            v.extend_from_slice(h);
        }
        v
    }

    #[test]
    fn keeping_leaves_every_point_of_a_straight_run_a_corner() {
        // A rectangle with three more points along its bottom and one in a hole's side.
        let outer = [
            [0.0, 0.0],
            [1.0, 0.0],
            [2.0, 0.0],
            [3.0, 0.0],
            [4.0, 0.0],
            [4.0, 2.0],
            [0.0, 2.0],
        ];
        let hole = vec![[1.0, 0.5], [1.0, 1.0], [1.0, 1.5], [3.0, 1.5], [3.0, 0.5]];
        let pts = all(&outer, std::slice::from_ref(&hole));
        let dropped = triangulate(&outer, std::slice::from_ref(&hole));
        let kept = triangulate_keeping(&outer, std::slice::from_ref(&hole));
        let used = |t: &[[usize; 3]]| {
            let mut u: Vec<usize> = t.iter().flatten().copied().collect();
            u.sort_unstable();
            u.dedup();
            u.len()
        };
        assert!(used(&dropped) < pts.len());
        assert_eq!(used(&kept), pts.len());
        assert!((area(&pts, &kept) - (8.0 - 2.0)).abs() < 1e-12);
        // Each side of the rings is a side of one triangle, so a neighbour meets it whole.
        for (ring, start) in [(&outer[..], 0), (&hole[..], outer.len())] {
            for k in 0..ring.len() {
                let (a, b) = (start + k, start + (k + 1) % ring.len());
                let n = kept
                    .iter()
                    .filter(|t| {
                        (0..3).any(|j| {
                            let (p, q) = (t[j], t[(j + 1) % 3]);
                            (p, q) == (a, b) || (p, q) == (b, a)
                        })
                    })
                    .count();
                assert_eq!(n, 1, "side {a}-{b}");
            }
        }
    }

    #[test]
    fn a_square_is_two_counterclockwise_triangles_either_way_round() {
        let sq = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        let t = triangulate(&sq, &[]);
        assert_eq!(t.len(), 2);
        assert_eq!(area(&sq, &t), 4.0);
        let mut rev = sq;
        rev.reverse();
        let t = triangulate(&rev, &[]);
        assert_eq!(area(&rev, &t), 4.0);
    }

    #[test]
    fn a_concave_polygon_keeps_its_area() {
        // An L, with a repeated point and a point on a straight edge.
        let l = [
            [0.0, 0.0],
            [3.0, 0.0],
            [3.0, 1.0],
            [1.0, 1.0],
            [1.0, 1.0],
            [1.0, 3.0],
            [0.0, 3.0],
            [0.0, 1.5],
        ];
        let t = triangulate(&l, &[]);
        assert!((area(&l, &t) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn many_holes_in_a_board_keep_its_area() {
        let outer = super::super::rounded_rect([0.0, 0.0], [100.0, 60.0], 3.0, 8);
        let mut holes = Vec::new();
        for i in 0..20 {
            for j in 0..12 {
                holes.push(super::super::circle(
                    [4.0 + 4.6 * f64::from(i), 4.0 + 4.6 * f64::from(j)],
                    1.0,
                    16,
                ));
            }
        }
        let t = triangulate(&outer, &holes);
        let pts = all(&outer, &holes);
        let ring_area = |r: &[[f64; 2]]| {
            let mut a = 0.0;
            for k in 0..r.len() {
                let (p, q) = (r[k], r[(k + 1) % r.len()]);
                a += p[0] * q[1] - q[0] * p[1];
            }
            0.5 * a
        };
        let want = ring_area(&outer) - holes.iter().map(|h| ring_area(h)).sum::<f64>();
        assert!(
            (area(&pts, &t) - want).abs() < 1e-6,
            "{} vs {want}",
            area(&pts, &t)
        );
        // Every triangle turns counterclockwise.
        for &[a, b, c] in &t {
            let (a, b, c) = (pts[a], pts[b], pts[c]);
            assert!((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) >= 0.0);
        }
    }

    #[test]
    fn too_few_points_give_nothing() {
        assert!(triangulate(&[[0.0, 0.0], [1.0, 0.0]], &[]).is_empty());
        assert!(triangulate(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]], &[]).is_empty());
    }
}
