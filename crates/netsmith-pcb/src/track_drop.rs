//! The voltage the routed tracks of a power net drop at its current (issue #652).
//!
//! A track is sized for heat before the board is routed (ADR 0036; `netsmith_design::guidelines`
//! says why a drop budget is not part of that sizing), and the voltage it drops depends on the
//! length the router then draws. This measures it on the routed board: each track of the net is
//! a resistance `rho * length / (width * thickness)`, a via or a pad joins what ends in it with
//! none, and the drop of the net is its worst-case current (`Board::net_currents`) through the
//! largest resistance between two of its pads, along the path of least resistance between them.
//! `fab` compares it with a budget, [`DEFAULT_DROP_BUDGET`] of the rail's voltage, and reports a
//! net over it as a guideline (the `track drop` rule).
//!
//! # Decisions (decided by the agent under the owner's delegation, issue #652)
//!
//! - **The whole current between the two pads furthest apart.** Which pad is the source is
//!   not known here, nor how the current divides between the loads, so the drop is the net's
//!   whole current through the largest resistance between any two of its pads. That is an
//!   upper bound on the drop from the source to any load, pessimistic in the same way as the
//!   worst-case current of ADR 0036 (amendment 2), and exact for a net with one source and one
//!   load. The path between two pads is the one of least resistance, which for a tree (the
//!   router's nets are trees) is the only one, and otherwise is no less than the parallel
//!   paths' resistance together, so the figure stays an upper bound.
//! - **The drawn width, copper at 20 °C.** The copper is the process's outer copper
//!   (`stackup.copper_weight`), at the resistivity of annealed copper at 20 °C,
//!   [`COPPER_RESISTIVITY`]. A track warm from its current, or finished narrower than drawn
//!   (the fabricator's width tolerance), drops a few per cent more; the budget is a guide, not
//!   a limit, so neither is added. A track on an inner signal layer is in the inner copper
//!   (`stackup.inner_copper`, issue #833): it counts as the outer track of the same resistance,
//!   its width times the inner copper over the outer, so the narrowest and the widths of a
//!   path are of outer copper.
//! - **Not measured.** A via's barrel (a few milliohms), a pad's copper and a teardrop are
//!   taken as no resistance. A net that is a plane, an outer pour or has a zone of the routes
//!   file is not measured: its copper is an area. Two pads that only a zone joins are not a
//!   pair. Layers are not told apart: tracks of the net that end at one place are joined there,
//!   which the router draws only through a via.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::{Board, PadRef, Pt, Track};

/// The resistivity of annealed copper at 20 °C, in ohm millimetres (1.72e-8 ohm metres: the
/// International Annealed Copper Standard, 100 % IACS).
pub const COPPER_RESISTIVITY: f64 = 1.72e-5;

/// The part of a rail's voltage that its tracks may drop before `fab` says so: 1 %. A choice of
/// this tool's (issue #652), and no published figure.
pub const DEFAULT_DROP_BUDGET: f64 = 0.01;

/// How near two ends of copper must be to be one place, in mm.
const JOIN: f64 = 1e-3;

/// The drop of one net's tracks at its current.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackDrop {
    pub net: String,
    /// The net's worst-case current, in amperes.
    pub current: f64,
    /// The largest resistance between two of its pads, in ohms.
    pub resistance: f64,
    /// The two pads that resistance is between.
    pub from: PadRef,
    pub to: PadRef,
    /// The length of track on the path between them, in mm, and its narrowest track.
    pub length: f64,
    pub narrowest: f64,
    /// Each run of the path as (length, width), in mm: what [`TrackDrop::width_for`] widens.
    pub runs: Vec<(f64, f64)>,
    /// The copper's thickness, in mm.
    pub thickness: f64,
}

impl TrackDrop {
    /// The voltage the path drops at the net's current, in volts.
    pub fn drop(&self) -> f64 {
        self.current * self.resistance
    }

    /// The least width, in steps of 0.01 mm, that every run of the path narrower than it would
    /// need to be widened to so that the path drops no more than `volts`: what a `.min_track` on
    /// the rail would ask. `None` when no width up to 50 mm does.
    pub fn width_for(&self, volts: f64) -> Option<f64> {
        if self.thickness <= 0.0 || self.current <= 0.0 {
            return None;
        }
        let ohms = |w: f64| {
            self.runs
                .iter()
                .map(|&(l, run)| COPPER_RESISTIVITY * l / (run.max(w) * self.thickness))
                .sum::<f64>()
        };
        (1..=5000)
            .map(|k| k as f64 / 100.0)
            .find(|&w| ohms(w) * self.current <= volts * (1.0 + 1e-9))
    }
}

/// The tracks as [`net_drop`] measures them in outer copper `outer` mm thick: a track on an
/// inner layer, in copper `inner` mm thick, is the outer track of the same resistance, its width
/// times `inner / outer` (issue #833).
pub fn in_outer_copper(tracks: &[&Track], outer: f64, inner: f64) -> Vec<Track> {
    tracks
        .iter()
        .map(|&t| {
            let mut t = t.clone();
            if !t.layer.is_outer() && outer > 0.0 {
                t.width *= inner / outer;
            }
            t
        })
        .collect()
}

/// One pad of a net, for [`net_drop`]: what names it and the box its copper spans.
#[derive(Debug, Clone)]
pub struct NetPad {
    pub pad: PadRef,
    pub min: Pt,
    pub max: Pt,
}

/// The largest resistance between two pads of different [`PadRef`]s along the tracks and vias
/// given, which are of one net: `None` when no two of them are joined by a track. `thickness`
/// is the copper's, in mm.
pub fn net_drop(
    net: &str,
    current: f64,
    tracks: &[&Track],
    vias: &[Pt],
    pads: &[NetPad],
    thickness: f64,
) -> Option<TrackDrop> {
    if thickness <= 0.0 {
        return None;
    }
    // The places copper ends: the ends of the tracks, each place once.
    let mut places: Vec<Pt> = Vec::new();
    let mut place = |p: Pt| -> usize {
        match places.iter().position(|q| q.distance(p) <= JOIN) {
            Some(i) => i,
            None => {
                places.push(p);
                places.len() - 1
            }
        }
    };
    let ends: Vec<(usize, usize)> = tracks
        .iter()
        .map(|t| (place(t.from), place(t.to)))
        .collect();
    // An edge: two places, its length and width (a run of track), or a join with none.
    let mut edges: Vec<(usize, usize, f64, f64)> = Vec::new();
    for (t, &(a, b)) in tracks.iter().zip(&ends) {
        // A straight track that another track ends on part way along is split there, so that a
        // branch joins it where it leaves it.
        let mut stops: Vec<(f64, usize)> = vec![(0.0, a), (t.length(), b)];
        if t.mid.is_none() {
            for (i, p) in places.iter().enumerate() {
                if i == a || i == b || t.distance_to(*p) > JOIN {
                    continue;
                }
                stops.push((t.from.distance(*p), i));
            }
        }
        stops.sort_by(|x, y| x.0.total_cmp(&y.0));
        for w in stops.windows(2) {
            edges.push((w[0].1, w[1].1, w[1].0 - w[0].0, t.width));
        }
    }
    // A via joins the ends of copper in it; each pad is a place of its own, joined to the ends
    // in its copper.
    let tracked = places.len();
    for v in vias {
        let ins: Vec<usize> = (0..tracked)
            .filter(|&i| places[i].distance(*v) <= JOIN)
            .collect();
        for w in ins.windows(2) {
            edges.push((w[0], w[1], 0.0, 0.0));
        }
    }
    let mut pad_places = Vec::new();
    for p in pads {
        let at = places.len();
        places.push(Pt::new(
            (p.min.x + p.max.x) / 2.0,
            (p.min.y + p.max.y) / 2.0,
        ));
        for (i, q) in places.iter().enumerate().take(tracked) {
            if p.min.x - JOIN <= q.x
                && q.x <= p.max.x + JOIN
                && p.min.y - JOIN <= q.y
                && q.y <= p.max.y + JOIN
            {
                edges.push((at, i, 0.0, 0.0));
            }
        }
        pad_places.push(at);
    }
    let ohms = |(l, w): (f64, f64)| {
        if w <= 0.0 {
            0.0
        } else {
            COPPER_RESISTIVITY * l / (w * thickness)
        }
    };
    let mut next: Vec<Vec<(usize, usize)>> = vec![Vec::new(); places.len()];
    for (k, &(a, b, _, _)) in edges.iter().enumerate() {
        next[a].push((b, k));
        next[b].push((a, k));
    }
    let mut worst: Option<(f64, usize, usize, Vec<usize>)> = None;
    for (n, &start) in pad_places.iter().enumerate() {
        // The least resistance from this pad to every place, and the edge it came by.
        let mut best = vec![f64::INFINITY; places.len()];
        let mut came: Vec<Option<usize>> = vec![None; places.len()];
        best[start] = 0.0;
        // A resistance is never negative, so its bits order as its value does.
        let mut queue = BinaryHeap::from([Reverse((0u64, start))]);
        while let Some(Reverse((bits, u))) = queue.pop() {
            if bits != best[u].to_bits() {
                continue;
            }
            for &(v, k) in &next[u] {
                let r = best[u] + ohms((edges[k].2, edges[k].3));
                if r < best[v] - 1e-15 {
                    best[v] = r;
                    came[v] = Some(k);
                    queue.push(Reverse((r.to_bits(), v)));
                }
            }
        }
        for (m, &end) in pad_places.iter().enumerate().skip(n + 1) {
            if pads[m].pad == pads[n].pad || !best[end].is_finite() {
                continue;
            }
            // Two pads joined through a track, not only through each other's copper.
            let mut path = Vec::new();
            let mut at = end;
            while let Some(k) = came[at] {
                path.push(k);
                at = if edges[k].0 == at {
                    edges[k].1
                } else {
                    edges[k].0
                };
            }
            if !path.iter().any(|&k| edges[k].3 > 0.0) {
                continue;
            }
            if worst.as_ref().is_none_or(|w| best[end] > w.0 + 1e-15) {
                worst = Some((best[end], n, m, path));
            }
        }
    }
    let (resistance, a, b, path) = worst?;
    let runs: Vec<(f64, f64)> = path
        .iter()
        .map(|&k| (edges[k].2, edges[k].3))
        .filter(|&(_, w)| w > 0.0)
        .collect();
    Some(TrackDrop {
        net: net.to_string(),
        current,
        resistance,
        from: pads[a].pad.clone(),
        to: pads[b].pad.clone(),
        length: runs.iter().map(|r| r.0).sum(),
        narrowest: runs.iter().map(|r| r.1).fold(f64::INFINITY, f64::min),
        runs,
        thickness,
    })
}

impl Board {
    /// The drop of the routed tracks of each net with a known current, by net name, measured
    /// as the header of this module says. A net that is a plane, an outer pour or has a zone
    /// is left out, and so is one with no two pads joined by a track.
    pub fn track_drops(&self) -> Vec<TrackDrop> {
        let thickness = self.process.stackup.copper_weight * netsmith_core::OUNCE_OF_COPPER;
        let mut drops = Vec::new();
        for c in &self.net_currents {
            if c.current <= 0.0
                || self.planes.iter().any(|p| p.net == c.net)
                || self.pours.iter().any(|p| p.net == c.net)
                || self.zones.iter().any(|z| z.net == c.net)
            {
                continue;
            }
            let Some(net) = self.nets.iter().find(|n| n.name == c.net) else {
                continue;
            };
            let inner = self
                .process
                .stackup
                .inner
                .as_ref()
                .map_or(thickness, |i| i.copper);
            let mine: Vec<&Track> = self.tracks.iter().filter(|t| t.net == c.net).collect();
            let owned = in_outer_copper(&mine, thickness, inner);
            let tracks: Vec<&Track> = owned.iter().collect();
            if tracks.is_empty() {
                continue;
            }
            let vias: Vec<Pt> = self
                .vias
                .iter()
                .filter(|v| v.net == c.net)
                .map(|v| v.at)
                .collect();
            let mut pads = Vec::new();
            for r in &net.pads {
                let Some(part) = self.part(&r.refdes) else {
                    continue;
                };
                for p in part.footprint.pads.iter().filter(|p| p.number == r.number) {
                    let b = part.pad_bounds(p);
                    pads.push(NetPad {
                        pad: r.clone(),
                        min: b.min,
                        max: b.max,
                    });
                }
            }
            drops.extend(net_drop(
                &c.net, c.current, &tracks, &vias, &pads, thickness,
            ));
        }
        drops
    }
}
