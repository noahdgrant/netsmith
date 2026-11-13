//! The cost of the nets, the crossings, the decoupling pairs and the groups.

use super::*;

impl<'a> Engine<'a> {
    pub(super) fn net_cost(&self, n: usize) -> f64 {
        let net = &self.nets[n];
        let (mut lo, mut hi) = (Pt::new(f64::MAX, f64::MAX), Pt::new(f64::MIN, f64::MIN));
        for &(i, k) in &net.pads {
            let at = self.pad_at(i, k);
            lo = Pt::new(lo.x.min(at.x), lo.y.min(at.y));
            hi = Pt::new(hi.x.max(at.x), hi.y.max(at.y));
        }
        W_WIRE * net.weight * ((hi.x - lo.x) + (hi.y - lo.y))
    }

    /// The ratsnest lines of a net as the parts are now: the shortest tree over its pad
    /// centres (Prim's algorithm, on squared distances so that nothing is rounded).
    pub(super) fn net_lines(&self, n: usize) -> Vec<(Pt, Pt)> {
        let pts: Vec<Pt> = self.nets[n]
            .pads
            .iter()
            .map(|&(i, k)| self.pad_at(i, k))
            .collect();
        let mut lines = Vec::with_capacity(pts.len().saturating_sub(1));
        let mut reach: Vec<(f64, usize)> = pts[1.min(pts.len())..]
            .iter()
            .map(|p| (dist2(*p, pts[0]), 0))
            .collect();
        let mut left: Vec<usize> = (1..pts.len()).collect();
        while !left.is_empty() {
            let mut best = 0;
            for k in 1..left.len() {
                if reach[k].0 < reach[best].0 {
                    best = k;
                }
            }
            let (_, from) = reach.swap_remove(best);
            let added = left.swap_remove(best);
            lines.push((pts[from], pts[added]));
            for (k, &other) in left.iter().enumerate() {
                let d = dist2(pts[other], pts[added]);
                if d < reach[k].0 {
                    reach[k] = (d, added);
                }
            }
        }
        lines
    }

    /// Bring the lines kept for a net up to date.
    pub(super) fn refresh_net(&mut self, n: usize) {
        if !self.nets[n].lines {
            return;
        }
        let lines = self.net_lines(n);
        self.line_box[n] = Rect::around(lines.iter().flat_map(|&(a, b)| [a, b]));
        self.lines[n] = lines;
    }

    pub(super) fn refresh(&mut self, set: &[usize]) {
        for &i in set {
            for k in 0..self.nets_of[i].len() {
                let n = self.nets_of[i][k];
                self.refresh_net(n);
            }
        }
    }

    pub(super) fn refresh_all(&mut self) {
        for n in 0..self.nets.len() {
            self.refresh_net(n);
        }
    }

    /// How many lines of the nets in `touched` (as the parts are now) cross a line of another
    /// net, the lines of the nets not in `touched` being those kept. A net's lines are
    /// counted against each other net's once.
    pub(super) fn crossings(&self, touched: &[usize], stamp: u32) -> usize {
        let news: Vec<(Lines, Option<Rect>)> = touched
            .iter()
            .map(|&n| {
                let lines = self.net_lines(n);
                let b = Rect::around(lines.iter().flat_map(|&(a, b)| [a, b]));
                (lines, b)
            })
            .collect();
        let mut count = 0;
        let across = |a: &[(Pt, Pt)], b: &[(Pt, Pt)]| {
            let mut n = 0;
            for &(p, q) in a {
                for &(r, s) in b {
                    if netsmith_pcb::segments_cross(p, q, r, s) {
                        n += 1;
                    }
                }
            }
            n
        };
        for (x, (lines, bx)) in news.iter().enumerate() {
            let Some(bx) = bx else { continue };
            for u in 0..self.nets.len() {
                if !self.nets[u].lines || self.net_mark[u] == stamp {
                    continue;
                }
                if self.line_box[u].is_some_and(|bu| bu.overlaps(bx)) {
                    count += across(lines, &self.lines[u]);
                }
            }
            for (other, by) in &news[x + 1..] {
                if by.is_some_and(|by| by.overlaps(bx)) {
                    count += across(lines, other);
                }
            }
        }
        count
    }

    pub(super) fn dec_cost(&self, d: usize) -> f64 {
        let dec = &self.decs[d];
        let mut total = 0.0;
        for (cap_pads, pins) in &dec.nets {
            let mut nearest = f64::MAX;
            for &c in cap_pads {
                let at = self.pad_at(dec.cap, c);
                for &(i, k) in pins {
                    nearest = nearest.min(dist(at, self.pad_at(i, k)));
                }
            }
            if nearest < f64::MAX {
                total += nearest;
            }
        }
        W_DECOUPLING * total
    }

    /// The box round the centres of a module instance's parts.
    pub(super) fn group_box(&self, g: usize) -> Rect {
        let (mut lo, mut hi) = (Pt::new(f64::MAX, f64::MAX), Pt::new(f64::MIN, f64::MIN));
        for &i in &self.groups[g] {
            let c = self.court(i).centre();
            lo = Pt::new(lo.x.min(c.x), lo.y.min(c.y));
            hi = Pt::new(hi.x.max(c.x), hi.y.max(c.y));
        }
        Rect::new(lo, hi)
    }

    /// What a module instance costs: the half perimeter of its box, and each part of another
    /// instance whose centre is inside the box.
    pub(super) fn group_cost(&self, g: usize) -> f64 {
        let b = self.group_box(g);
        let mut cost = W_COHESION * (b.width() + b.height());
        for j in 0..self.models.len() {
            if !self.in_group[g][j] {
                cost += self.intrusion(&b, j);
            }
        }
        cost
    }
}
