//! A grid router: orthogonal wires that go round parts and texts, cross other nets only at
//! right angles, and are as short as the cost below makes them. Pure and deterministic: ties
//! are broken by the order the directions are tried in.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

use crate::geom::{Dir, Pt, Rect};

/// A node of the grid: column and row.
pub type Node = (i32, i32);

/// Nobody owns the node on that axis.
pub const NONE: u32 = u32::MAX;

const STEP_COST: u32 = 10;
const BEND_COST: u32 = 60;
const CROSS_COST: u32 = 45;
const DIRS: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

fn vector(d: Dir) -> (i32, i32) {
    match d {
        Dir::Up => (0, -1),
        Dir::Down => (0, 1),
        Dir::Left => (-1, 0),
        Dir::Right => (1, 0),
    }
}

/// 0 for a horizontal move, 1 for a vertical one.
fn axis(d: Dir) -> usize {
    usize::from(d.is_vertical())
}

fn dir_index(d: Dir) -> usize {
    DIRS.iter().position(|&x| x == d).expect("a direction")
}

#[derive(Clone)]
pub struct Grid {
    pub w: i32,
    pub h: i32,
    pub x0: f64,
    pub y0: f64,
    pub step: f64,
    blocked: Vec<bool>,
    /// For each axis (horizontal, vertical), whether the step from the node to the next one
    /// along it crosses a body that lies between the two nodes (a stroke thinner than a
    /// step falls between nodes, so blocking nodes does not stop a wire through it).
    cut: [Vec<bool>; 2],
    /// For each axis (horizontal, vertical), the net whose wire runs through or ends at the
    /// node along it.
    own: [Vec<u32>; 2],
}

impl Grid {
    /// A grid over `bounds`, with nodes `step` apart on multiples of `step`.
    pub fn new(bounds: Rect, step: f64) -> Grid {
        let x0 = (bounds.min.x / step).floor() * step;
        let y0 = (bounds.min.y / step).floor() * step;
        let w = ((bounds.max.x - x0) / step).ceil() as i32 + 1;
        let h = ((bounds.max.y - y0) / step).ceil() as i32 + 1;
        let n = (w * h) as usize;
        Grid {
            w,
            h,
            x0,
            y0,
            step,
            blocked: vec![false; n],
            cut: [vec![false; n], vec![false; n]],
            own: [vec![NONE; n], vec![NONE; n]],
        }
    }

    pub fn node(&self, p: Pt) -> Node {
        (
            ((p.x - self.x0) / self.step).round() as i32,
            ((p.y - self.y0) / self.step).round() as i32,
        )
    }

    pub fn pt(&self, n: Node) -> Pt {
        Pt::new(
            self.x0 + f64::from(n.0) * self.step,
            self.y0 + f64::from(n.1) * self.step,
        )
    }

    pub fn inside(&self, n: Node) -> bool {
        n.0 >= 0 && n.1 >= 0 && n.0 < self.w && n.1 < self.h
    }

    fn idx(&self, n: Node) -> usize {
        (n.1 * self.w + n.0) as usize
    }

    /// Blocks every node that lies in `r`, edges included.
    pub fn block_rect(&mut self, r: Rect) {
        let eps = 1e-6;
        let lo = self.node(Pt::new(r.min.x + eps, r.min.y + eps));
        let hi = self.node(Pt::new(r.max.x - eps, r.max.y - eps));
        // `node` rounds; widen by one and test the real position.
        for j in (lo.1 - 1).max(0)..=(hi.1 + 1).min(self.h - 1) {
            for i in (lo.0 - 1).max(0)..=(hi.0 + 1).min(self.w - 1) {
                let p = self.pt((i, j));
                if p.x >= r.min.x - eps
                    && p.x <= r.max.x + eps
                    && p.y >= r.min.y - eps
                    && p.y <= r.max.y + eps
                {
                    let k = self.idx((i, j));
                    self.blocked[k] = true;
                }
            }
        }
    }

    /// Blocks every node that lies in `r`, edges included, and every step between two
    /// nodes that goes through `r`, which a body thinner than a step needs.
    pub fn block_body(&mut self, r: Rect) {
        self.block_rect(r);
        let eps = 1e-6;
        let lo = self.node(Pt::new(r.min.x, r.min.y));
        let hi = self.node(Pt::new(r.max.x, r.max.y));
        for j in (lo.1 - 2).max(0)..=(hi.1 + 2).min(self.h - 1) {
            for i in (lo.0 - 2).max(0)..=(hi.0 + 2).min(self.w - 1) {
                let (a, k) = (self.pt((i, j)), self.idx((i, j)));
                let (right, down) = (self.pt((i + 1, j)), self.pt((i, j + 1)));
                // A step crosses the box when it starts and ends outside it and passes
                // through it between. A step with an end in the box is the business of the
                // blocked nodes: a pin's own node may stand in the box and its wire leaves.
                let inside = |p: Pt| {
                    p.x >= r.min.x - eps
                        && p.x <= r.max.x + eps
                        && p.y >= r.min.y - eps
                        && p.y <= r.max.y + eps
                };
                if !inside(a)
                    && !inside(right)
                    && a.y >= r.min.y - eps
                    && a.y <= r.max.y + eps
                    && a.x < r.min.x
                    && right.x > r.max.x
                {
                    self.cut[0][k] = true;
                }
                if !inside(a)
                    && !inside(down)
                    && a.x >= r.min.x - eps
                    && a.x <= r.max.x + eps
                    && a.y < r.min.y
                    && down.y > r.max.y
                {
                    self.cut[1][k] = true;
                }
            }
        }
    }

    /// Whether the step from `n` one node along `dir` crosses a body.
    pub fn step_is_cut(&self, n: Node, dir: Dir) -> bool {
        let (dx, dy) = vector(dir);
        // A step is stored at its left or upper node.
        let from = if dx < 0 || dy < 0 {
            (n.0 + dx, n.1 + dy)
        } else {
            n
        };
        self.inside(from) && self.cut[axis(dir)][self.idx(from)]
    }

    pub fn block(&mut self, n: Node) {
        if self.inside(n) {
            let k = self.idx(n);
            self.blocked[k] = true;
        }
    }

    pub fn unblock(&mut self, n: Node) {
        if self.inside(n) {
            let k = self.idx(n);
            self.blocked[k] = false;
        }
    }

    pub fn is_blocked(&self, n: Node) -> bool {
        !self.inside(n) || self.blocked[self.idx(n)]
    }

    pub fn owner(&self, n: Node, axis: usize) -> u32 {
        if self.inside(n) {
            self.own[axis][self.idx(n)]
        } else {
            NONE
        }
    }

    fn claim(&mut self, n: Node, axis: usize, net: u32) {
        let k = self.idx(n);
        self.own[axis][k] = net;
    }

    /// Whether nothing is on the node, on either axis, and it is not blocked.
    pub fn is_free(&self, n: Node) -> bool {
        !self.is_blocked(n) && self.owner(n, 0) == NONE && self.owner(n, 1) == NONE
    }

    /// Whether nothing but a wire of `net` is on the node, and it is not blocked.
    pub fn free_for(&self, n: Node, net: u32) -> bool {
        !self.is_blocked(n)
            && [0, 1]
                .into_iter()
                .all(|ax| matches!(self.owner(n, ax), NONE) || self.owner(n, ax) == net)
    }

    /// Whether `net` may have a wire along `axis` through the node.
    fn usable(&self, n: Node, ax: usize, net: u32) -> bool {
        if self.is_blocked(n) {
            return false;
        }
        let o = self.own[ax][self.idx(n)];
        o == NONE || o == net
    }

    /// Marks a straight wire of `net` from `a` to `b` (on one row or one column) as taken,
    /// its ends on both axes.
    pub fn claim_wire(&mut self, a: Node, b: Node, net: u32) {
        let (dx, dy) = ((b.0 - a.0).signum(), (b.1 - a.1).signum());
        let mut n = a;
        loop {
            let end = n == a || n == b;
            if end {
                self.claim(n, 0, net);
                self.claim(n, 1, net);
            } else {
                self.claim(n, usize::from(dx == 0), net);
            }
            if n == b {
                break;
            }
            n = (n.0 + dx, n.1 + dy);
        }
    }

    /// Takes the nodes of a routed path for `net`.
    pub fn commit(&mut self, path: &[Node], net: u32) {
        for (i, &n) in path.iter().enumerate() {
            if i == 0 || i + 1 == path.len() {
                self.claim(n, 0, net);
                self.claim(n, 1, net);
                continue;
            }
            let a = (n.0 - path[i - 1].0, n.1 - path[i - 1].1);
            let b = (path[i + 1].0 - n.0, path[i + 1].1 - n.1);
            if a == b {
                self.claim(n, usize::from(a.0 == 0), net);
            } else {
                self.claim(n, 0, net);
                self.claim(n, 1, net);
            }
        }
    }

    /// Whether the `len` nodes after `from` along `dir` are free (nothing on them).
    pub fn run_is_free(&self, from: Node, dir: Dir, len: i32) -> bool {
        let (dx, dy) = vector(dir);
        (1..=len).all(|k| {
            let before = (from.0 + dx * (k - 1), from.1 + dy * (k - 1));
            self.is_free((from.0 + dx * k, from.1 + dy * k)) && !self.step_is_cut(before, dir)
        })
    }

    /// The cheapest path for `net` from `start` (entered going `heading`) to any node of
    /// `targets`, as the nodes along it, or `None`. Searches a window round the two first
    /// and the whole grid if that fails.
    pub fn route(
        &self,
        net: u32,
        start: Node,
        heading: Dir,
        targets: &HashSet<Node>,
    ) -> Option<Vec<Node>> {
        if targets.contains(&start) {
            return Some(vec![start]);
        }
        let mut lo = start;
        let mut hi = start;
        for t in targets {
            lo = (lo.0.min(t.0), lo.1.min(t.1));
            hi = (hi.0.max(t.0), hi.1.max(t.1));
        }
        for margin in [10, 40, i32::MAX / 4] {
            let win = (
                (lo.0.saturating_sub(margin)).max(0),
                (lo.1.saturating_sub(margin)).max(0),
                (hi.0.saturating_add(margin)).min(self.w - 1),
                (hi.1.saturating_add(margin)).min(self.h - 1),
            );
            if let Some(p) = self.search(net, start, heading, targets, win) {
                return Some(p);
            }
            if win == (0, 0, self.w - 1, self.h - 1) {
                break;
            }
        }
        None
    }

    fn search(
        &self,
        net: u32,
        start: Node,
        heading: Dir,
        targets: &HashSet<Node>,
        win: (i32, i32, i32, i32),
    ) -> Option<Vec<Node>> {
        let (wi, wj) = (win.2 - win.0 + 1, win.3 - win.1 + 1);
        let cells = (wi * wj) as usize;
        let slot = |n: Node, d: usize| -> usize {
            (((n.1 - win.1) * wi + (n.0 - win.0)) as usize) * 4 + d
        };
        let mut dist = vec![u32::MAX; cells * 4];
        let mut from = vec![u32::MAX; cells * 4];
        let mut heap: BinaryHeap<Reverse<(u32, u32, u32)>> = BinaryHeap::new();
        let d0 = dir_index(heading);
        dist[slot(start, d0)] = 0;
        heap.push(Reverse((0, 0, slot(start, d0) as u32)));
        let mut seq = 0u32;
        let mut found: Option<usize> = None;
        // Where a slot is, from its index.
        let unslot = |s: usize| -> (Node, usize) {
            let d = s % 4;
            let c = s / 4;
            ((win.0 + (c as i32 % wi), win.1 + (c as i32 / wi)), d)
        };
        while let Some(Reverse((cost, _, s))) = heap.pop() {
            let s = s as usize;
            if cost > dist[s] {
                continue;
            }
            let (n, d) = unslot(s);
            if n != start && targets.contains(&n) {
                found = Some(s);
                break;
            }
            let heading = DIRS[d];
            for (nd, &dir) in DIRS.iter().enumerate() {
                if dir == heading.opposite() {
                    continue;
                }
                let turning = dir != heading;
                // A turn at `n` takes both axes there.
                if turning
                    && !(self.usable(n, axis(heading), net) && self.usable(n, axis(dir), net))
                {
                    continue;
                }
                let (dx, dy) = vector(dir);
                let m = (n.0 + dx, n.1 + dy);
                if m.0 < win.0 || m.1 < win.1 || m.0 > win.2 || m.1 > win.3 {
                    continue;
                }
                if !self.usable(m, axis(dir), net) || self.step_is_cut(n, dir) {
                    continue;
                }
                let mut c = cost + STEP_COST;
                if turning {
                    c += BEND_COST;
                }
                // Crossing another net's wire at right angles.
                let o = self.own[1 - axis(dir)][self.idx(m)];
                if o != NONE && o != net {
                    c += CROSS_COST;
                }
                let t = slot(m, nd);
                if c < dist[t] {
                    dist[t] = c;
                    from[t] = s as u32;
                    seq += 1;
                    heap.push(Reverse((c, seq, t as u32)));
                }
            }
        }
        let mut s = found?;
        let mut path = vec![unslot(s).0];
        while from[s] != u32::MAX {
            s = from[s] as usize;
            path.push(unslot(s).0);
        }
        path.reverse();
        Some(path)
    }
}

/// The corners of a path of nodes: its points with the ones between that lie on a line left
/// out.
pub fn corners(path: &[Node]) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::new();
    for &n in path {
        if out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            if (b.0 - a.0).signum() == (n.0 - b.0).signum()
                && (b.1 - a.1).signum() == (n.1 - b.1).signum()
                && ((b.0 - a.0 == 0) == (n.0 - b.0 == 0))
            {
                out.pop();
            }
        }
        out.push(n);
    }
    out
}
