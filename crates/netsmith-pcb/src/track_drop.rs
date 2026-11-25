//! The voltage the routed tracks of a power net drop at its current (issue #652).
//!
//! A track is sized for heat before the board is routed (ADR 0036; `netsmith_design::guidelines`
//! says why a drop budget is not part of that sizing), and the voltage it drops depends on the
//! length the router then draws. This measures it on the routed board: each track of the net is
//! a resistance `rho * length / (width * thickness)`, a via or a pad joins what ends in it with
//! none, and the drop of the net is the most any of its pads sees from a source pad, each run of
//! the path at the current the pins past it draw (issue #1541). `fab` compares it with a budget,
//! [`DEFAULT_DROP_BUDGET`] of the rail's voltage, and reports a net over it as a guideline (the
//! `track drop` rule); `board nets` and `board drc` print it too (issue #1540).
//!
//! # Decisions (decided by the agent under the owner's delegation, issues #652 and #1541)
//!
//! - **Each run at what the pins past it draw.** A pin whose current is known draws that: one
//!   that states the current of its branch (issue #1113), or one whose current the model knows
//!   below its net's (the design's `pin_currents`, issue #1333: a divider's resistor carries the
//!   voltage across it over its resistance, from the DC analysis, and an input none). A pin
//!   whose current is not known (a connector's, a regulator's output, a capacitor's) may draw
//!   the net's whole current, its worst-case current (`Board::net_currents`), and may be the
//!   source: which pad is the source is not known here, so each pad of such a pin is tried as
//!   one (every pad, when all the pins' currents are known). From a source the current takes
//!   the paths of least resistance to every place, a tree, and each run of that tree carries
//!   the sum of what the pins beyond it draw, each pin once however many pads it has, and never
//!   more than the net's current. The drop to a pad is the sum along its path of each run's
//!   resistance times its current, and the net's drop is the largest over every source and
//!   pad. A net of pins of no known current drops its whole current through the largest
//!   resistance between two of its pads, as before issue #1541: an upper bound on the drop from
//!   the source to any load, pessimistic in the same way as the worst-case current of ADR 0036
//!   (amendment 2), and exact for a net with one source and one load. The router's nets are
//!   trees, where the tree of least resistance is the copper; where copper closes a loop, all
//!   the current is taken along the one path, more than the parallel paths share, so the
//!   figure stays an upper bound.
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
//! - **A branch draws its own current** (issues #1113 and #1541). The pad of a pin that states
//!   the current of its branch, or whose current the model knows, draws that current and is
//!   never a source while a pin of no known current is on the net: the net's whole current
//!   through a divider's thin track would be a drop that never happens.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::{Board, CopperLayer, PadRef, Pt, Track};

/// The resistivity of annealed copper at 20 °C, in ohm millimetres (1.72e-8 ohm metres: the
/// International Annealed Copper Standard, 100 % IACS).
pub const COPPER_RESISTIVITY: f64 = 1.72e-5;

/// The part of a rail's voltage that its tracks may drop before `fab` says so: 1 %. A choice of
/// this tool's (issue #652), and no published figure.
pub const DEFAULT_DROP_BUDGET: f64 = 0.01;

/// How near two ends of copper must be to be one place, in mm.
const JOIN: f64 = 1e-3;

/// How near its budget a net's drop must be for its path to be listed piece by piece: half of
/// it (issue #1610). A choice of this tool's: a net far within its budget needs no widening.
pub const NEAR_DROP_BUDGET: f64 = 0.5;

/// One run of a [`TrackDrop`]'s path: a length of track at one width, and the current it
/// carries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropRun {
    /// In mm.
    pub length: f64,
    /// In mm, of outer copper.
    pub width: f64,
    /// In amperes: what the pins past it draw, at most the net's current.
    pub current: f64,
    /// Where the run starts and ends, from the source towards the load (issue #1610).
    pub from: Pt,
    pub to: Pt,
    /// The run's track and its layer: an index into the tracks the path was measured from (for
    /// [`Board::track_drops`], into [`Board::tracks`]), and its drawn width in mm, which differs
    /// from `width` on an inner layer.
    pub track: Option<usize>,
    pub layer: Option<CopperLayer>,
    pub drawn: f64,
    /// The via the path changes layer in before this run, if it does: its place.
    pub via_before: Option<Pt>,
}

impl DropRun {
    /// The run's resistance at `width` in copper `thickness` mm thick, in ohms.
    fn ohms(&self, width: f64, thickness: f64) -> f64 {
        COPPER_RESISTIVITY * self.length / (width * thickness)
    }
}

/// The drop of one net's tracks at its current.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackDrop {
    pub net: String,
    /// The most current a run of the path carries, in amperes: the net's worst-case current
    /// where the pins past the run include one of no known current.
    pub current: f64,
    /// The resistance of the path, in ohms.
    pub resistance: f64,
    /// The two pads of the path: the source (a pad of a pin of no known current) and the load.
    pub from: PadRef,
    pub to: PadRef,
    /// The length of track on the path between them, in mm, and its narrowest track.
    pub length: f64,
    pub narrowest: f64,
    /// Each run of the path: what [`TrackDrop::width_for`] widens.
    pub runs: Vec<DropRun>,
    /// The copper's thickness, in mm.
    pub thickness: f64,
}

impl TrackDrop {
    /// The voltage the path drops, in volts: each run's resistance times its current.
    pub fn drop(&self) -> f64 {
        self.drop_at(0.0)
    }

    /// Whether every run carries the net's whole current: a path with no pin of a known current
    /// past any of its runs.
    pub fn whole_current(&self) -> bool {
        self.runs
            .iter()
            .all(|r| r.current >= self.current * (1.0 - 1e-9))
    }

    /// The drop with every run narrower than `width` widened to it.
    fn drop_at(&self, width: f64) -> f64 {
        if self.thickness <= 0.0 {
            return 0.0;
        }
        self.runs
            .iter()
            .map(|r| r.current * r.ohms(r.width.max(width), self.thickness))
            .sum()
    }

    /// The least width, in steps of 0.01 mm, that every run of the path narrower than it would
    /// need to be widened to so that the path drops no more than `volts`: what a `.min_track` on
    /// the rail would ask. `None` when no width up to 50 mm does.
    pub fn width_for(&self, volts: f64) -> Option<f64> {
        if self.thickness <= 0.0 || self.current <= 0.0 {
            return None;
        }
        (1..=5000)
            .map(|k| k as f64 / 100.0)
            .find(|&w| self.drop_at(w) <= volts * (1.0 + 1e-9))
    }

    /// The pieces of the path (issue #1610): each track (the runs of one track next to each
    /// other on the path taken together) and each via, from the source to the load, with what
    /// each drops and, when the path drops more than `volts`, the drawn width each track needs
    /// so that it drops no more: [`TrackDrop::width_for`], the one width every narrower run is
    /// widened to, in the drawn copper of the track's layer. A via drops nothing here (the
    /// header of this module says why). [`DropPiece::worst_first`] orders them by their drop.
    pub fn pieces(&self, volts: f64) -> Vec<DropPiece> {
        let floor = if self.drop() > volts * (1.0 + 1e-9) {
            self.width_for(volts)
        } else {
            None
        };
        let mut pieces: Vec<DropPiece> = Vec::new();
        for r in &self.runs {
            if let Some(at) = r.via_before {
                pieces.push(DropPiece {
                    step: pieces.len(),
                    via: Some(at),
                    track: None,
                    layer: None,
                    from: at,
                    to: at,
                    length: 0.0,
                    width: 0.0,
                    current: r.current,
                    drop: 0.0,
                    needs: None,
                    unreachable: false,
                });
            }
            let drop = if self.thickness > 0.0 {
                r.current * r.ohms(r.width, self.thickness)
            } else {
                0.0
            };
            let scale = if r.width > 0.0 {
                r.drawn / r.width
            } else {
                1.0
            };
            if let Some(last) = pieces.last_mut()
                && last.via.is_none()
                && last.track.is_some()
                && last.track == r.track
            {
                last.to = r.to;
                last.length += r.length;
                last.current = last.current.max(r.current);
                last.drop += drop;
                continue;
            }
            let (needs, unreachable) = match (floor, self.drop() > volts * (1.0 + 1e-9)) {
                (Some(w), _) if r.width < w - 1e-9 => (Some(round_up(w * scale)), false),
                (None, true) => (None, true),
                _ => (None, false),
            };
            pieces.push(DropPiece {
                step: pieces.len(),
                via: None,
                track: r.track,
                layer: r.layer,
                from: r.from,
                to: r.to,
                length: r.length,
                width: r.drawn,
                current: r.current,
                drop,
                needs,
                unreachable,
            });
        }
        pieces
    }
}

/// A width rounded up to the next 0.01 mm, as [`TrackDrop::width_for`] steps.
fn round_up(w: f64) -> f64 {
    (w * 100.0 - 1e-6).ceil() / 100.0
}

/// One piece of a [`TrackDrop`]'s path (issue #1610): a track, or a via the path changes layer
/// in, with its share of the drop and the width it needs. What `board nets` and `board drc` list
/// for a net near its budget, and what a tool that widens the path reads.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DropPiece {
    /// Its place on the path, from the source: 0 is the first.
    pub step: usize,
    /// A via's place, for a via; `None` for a track.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<Pt>,
    /// The track: its index in the tracks the path was measured from (for
    /// [`Board::track_drops`], into [`Board::tracks`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "layer_name")]
    pub layer: Option<CopperLayer>,
    /// Where it starts and ends, from the source towards the load, in the board's frame.
    pub from: Pt,
    pub to: Pt,
    /// In mm, drawn: on an inner layer, the width of the inner copper.
    pub length: f64,
    pub width: f64,
    /// The most current it carries, in amperes.
    pub current: f64,
    /// What it drops, in volts.
    pub drop: f64,
    /// The drawn width in mm it needs for the path to drop no more than the budget, when it is
    /// narrower than that and the path is over: `None` for a piece wide enough, or a path within.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs: Option<f64>,
    /// Whether no width up to 50 mm brings the path within the budget.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unreachable: bool,
}

fn layer_name<S: serde::Serializer>(l: &Option<CopperLayer>, s: S) -> Result<S::Ok, S::Error> {
    match l {
        Some(l) => s.serialize_str(l.name()),
        None => s.serialize_none(),
    }
}

impl DropPiece {
    /// The pieces in the order of their drop, the most first, and by their place on the path
    /// where two drop the same.
    pub fn worst_first(mut pieces: Vec<DropPiece>) -> Vec<DropPiece> {
        pieces.sort_by(|a, b| b.drop.total_cmp(&a.drop).then(a.step.cmp(&b.step)));
        pieces
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

/// What a pin of a [`NetPad`] draws, when it is known (issue #1541): a pin that states the
/// current of its branch, or one whose current the model knows (a divider's resistor, an input).
#[derive(Debug, Clone, PartialEq)]
pub struct PinDraw {
    /// The pin's path in the design: its pads, however many, draw this once.
    pub pin: String,
    /// In amperes.
    pub current: f64,
}

/// One pad of a net, for [`net_drop`]: what names it, the box its copper spans, and what its
/// pin draws, when it is known.
#[derive(Debug, Clone)]
pub struct NetPad {
    pub pad: PadRef,
    pub min: Pt,
    pub max: Pt,
    pub draws: Option<PinDraw>,
}

impl NetPad {
    /// The pin the pad is of: its path when its current is known, else the pad's own name.
    fn pin(&self) -> String {
        match &self.draws {
            Some(d) => d.pin.clone(),
            None => format!("{}.{}", self.pad.refdes, self.pad.number),
        }
    }
}

/// The largest drop from a source pad to another pad along the tracks and vias given, which
/// are of one net whose worst-case current is `current`, as the header of this module says:
/// `None` when no two pads of different pins are joined by a track. `thickness` is the
/// copper's, in mm.
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
    // An edge: two places, its length and width (a run of track), or a join with none; and
    // what it is, a run of the track of that index, a via at that place or a pad's copper.
    let mut edges: Vec<(usize, usize, f64, f64)> = Vec::new();
    let mut kinds: Vec<Edge> = Vec::new();
    for (n, (t, &(a, b))) in tracks.iter().zip(&ends).enumerate() {
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
            kinds.push(Edge::Track(n));
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
            kinds.push(Edge::Via);
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
                kinds.push(Edge::Pad);
            }
        }
        pad_places.push(at);
    }
    // The pins, each once, and what each draws: a pin of no known current may draw the net's.
    let mut pins: Vec<(String, f64)> = Vec::new();
    let pin_of: Vec<usize> = pads
        .iter()
        .map(|p| {
            let name = p.pin();
            match pins.iter().position(|(n, _)| *n == name) {
                Some(i) => i,
                None => {
                    pins.push((name, p.draws.as_ref().map_or(current, |d| d.current)));
                    pins.len() - 1
                }
            }
        })
        .collect();
    // The sources: the pads of a pin of no known current, or every pad when all are known.
    let sources: Vec<usize> = match (0..pads.len())
        .filter(|&n| pads[n].draws.is_none())
        .collect::<Vec<_>>()
    {
        none if none.is_empty() => (0..pads.len()).collect(),
        some => some,
    };
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
    let other = |k: usize, at: usize| {
        if edges[k].0 == at {
            edges[k].1
        } else {
            edges[k].0
        }
    };
    let mut worst: Option<(f64, usize, usize, Vec<DropRun>)> = None;
    for &n in &sources {
        let start = pad_places[n];
        // The least resistance from this pad to every place, and the edge it came by: the
        // tree the current from this source takes.
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
        // What each place passes on: the sum of what the pins past it draw, each pin once and
        // the source's own not at all, at most the net's current.
        let mut load = vec![0.0; places.len()];
        let mut marked: Vec<Vec<usize>> = vec![Vec::new(); places.len()];
        for (m, &end) in pad_places.iter().enumerate() {
            let pin = pin_of[m];
            if pin == pin_of[n] || !best[end].is_finite() {
                continue;
            }
            let mut at = end;
            loop {
                if marked[at].contains(&pin) {
                    break;
                }
                marked[at].push(pin);
                load[at] += pins[pin].1;
                match came[at] {
                    Some(k) => at = other(k, at),
                    None => break,
                }
            }
        }
        for (m, &end) in pad_places.iter().enumerate() {
            if pin_of[m] == pin_of[n] || !best[end].is_finite() {
                continue;
            }
            // The path back to the source, each run at what the place past it passes on.
            let mut runs: Vec<DropRun> = Vec::new();
            let mut volts = 0.0;
            let mut at = end;
            while let Some(k) = came[at] {
                let (length, width) = (edges[k].2, edges[k].3);
                let back = other(k, at);
                match kinds[k] {
                    Edge::Track(t) if width > 0.0 => {
                        let amps = load[at].min(current);
                        volts += amps * ohms((length, width));
                        runs.push(DropRun {
                            length,
                            width,
                            current: amps,
                            from: places[back],
                            to: places[at],
                            track: Some(t),
                            layer: Some(tracks[t].layer),
                            drawn: width,
                            via_before: None,
                        });
                    }
                    _ => {}
                }
                at = back;
            }
            // Two pads joined through a track, not only through each other's copper.
            if runs.is_empty() {
                continue;
            }
            runs.reverse();
            // A via the path changes layer in, between two runs on two layers.
            for i in 1..runs.len() {
                if runs[i - 1].layer != runs[i].layer {
                    runs[i].via_before = vias
                        .iter()
                        .copied()
                        .find(|v| v.distance(runs[i].from) <= JOIN);
                }
            }
            if worst.as_ref().is_none_or(|w| volts > w.0 + 1e-15) {
                worst = Some((volts, n, m, runs));
            }
        }
    }
    let (_, a, b, runs) = worst?;
    Some(TrackDrop {
        net: net.to_string(),
        current: runs.iter().map(|r| r.current).fold(0.0, f64::max),
        resistance: runs.iter().map(|r| r.ohms(r.width, thickness)).sum(),
        from: pads[a].pad.clone(),
        to: pads[b].pad.clone(),
        length: runs.iter().map(|r| r.length).sum(),
        narrowest: runs.iter().map(|r| r.width).fold(f64::INFINITY, f64::min),
        runs,
        thickness,
    })
}

/// What an edge of [`net_drop`]'s graph is.
#[derive(Clone, Copy)]
enum Edge {
    Track(usize),
    Via,
    Pad,
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
            let index: Vec<usize> = (0..self.tracks.len())
                .filter(|&i| self.tracks[i].net == c.net)
                .collect();
            let mine: Vec<&Track> = index.iter().map(|&i| &self.tracks[i]).collect();
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
                let draws = self.draw_of(&c.net, r);
                for p in part.footprint.pads_numbered(&r.number) {
                    let b = part.pad_bounds(p);
                    pads.push(NetPad {
                        pad: r.clone(),
                        min: b.min,
                        max: b.max,
                        draws: draws.clone(),
                    });
                }
            }
            drops.extend(
                net_drop(&c.net, c.current, &tracks, &vias, &pads, thickness).map(|mut d| {
                    // The runs name the board's tracks, at their drawn widths.
                    for r in &mut d.runs {
                        if let Some(t) = r.track {
                            r.track = Some(index[t]);
                            r.drawn = self.tracks[index[t]].width;
                        }
                    }
                    d
                }),
            );
        }
        drops
    }

    /// What the pin with `pad` on `net` draws, when it is known (issue #1541): the current its
    /// branch states (issue #1113), or the one the model knows for it (`pin_currents`, issue
    /// #1333: a divider's resistor carries the voltage across it over its resistance, an input
    /// none).
    pub fn draw_of(&self, net: &str, pad: &PadRef) -> Option<PinDraw> {
        if let Some(b) = self.branch_of(net, pad) {
            return Some(PinDraw {
                pin: b.path.clone(),
                current: b.current,
            });
        }
        self.pin_currents
            .iter()
            .find(|l| {
                l.net == net
                    && l.pins
                        .iter()
                        .any(|p| p.refdes == pad.refdes && p.pin == pad.number)
            })
            .map(|l| PinDraw {
                pin: l.path.clone(),
                current: l.current,
            })
    }
}
