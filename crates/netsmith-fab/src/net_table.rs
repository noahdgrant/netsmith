//! A per-net table of a routed board (issue #1223): for each net its routed and straight length,
//! its vias, how often it changes layer and on which layers it runs, and for each diff pair the
//! lengths of its two nets, their skew, how much of the pair runs coupled, the gaps it keeps
//! there, and its vias side by side. `netsmith board nets` prints it, the editor's PCB summary
//! shows it, and `board drc` notes the pairs whose skew is over the limit.
//!
//! Read from the board file, as the scorecard and the list of long nets are ([`crate::NetRoutes`],
//! which takes its numbers from here), so a board netsmith routed and one a person routed are
//! measured by the same code, and the numbers are the router's: a track's length is its centre
//! line's (an arc by its chords), as the router's meanders measure it.
//!
//! - The **nets** are those that no zone pours, with a track or a via: a poured net's tracks are
//!   not its connection. Its **straight length** is the shortest tree of straight lines between
//!   the centres of its pads, as for [`crate::NetRoutes`].
//! - A net **changes layer** at a via where its tracks meet on two layers or more (a via of a
//!   pour, or one a track only ends on, changes none).
//! - The **pairs** are the board's diff pairs as the caller gives them (`netsmith_pcb::Board::
//!   diff_pairs`: two nets whose names differ by a suffix pair, a `PairImpedance`, a `DiffPair`
//!   or the lines of a std bus such as `Can`), which a board file does not say. A pair runs
//!   **coupled** where the second net's track is on the same layer within
//!   [`netsmith_geom::COUPLED`] (0.5 mm, centre to centre) of the first's, sampled every
//!   [`SAMPLE`] along the first net, as the scorecard measures a pair; the **gaps** are edge to
//!   edge over that length, the least and the largest, so a pair at one pitch has the two
//!   equal. Two vias of the pair are **side by side** when their copper is no more than
//!   [`VIA_PAIR_GAP`] apart, edge to edge, each via taken once.
//! - A pair's skew is **over** when it is more than the limit the caller gives: the router's
//!   target (`netsmith_router::meander::SKEW_TARGET`, 0.1 mm), which the source cannot state
//!   for a pair yet.

use std::collections::{BTreeMap, BTreeSet};

use netsmith_geom::{Artwork, COUPLED, Pt};

/// The step a pair's first net is sampled at, in mm.
pub const SAMPLE: f64 = 0.05;
/// The most two vias of a pair are apart, copper edge to copper edge, to be side by side, in mm:
/// the scorecard's coupled distance.
pub const VIA_PAIR_GAP: f64 = COUPLED;
/// Points closer than this are one point, in mm.
const SAME: f64 = 1e-3;

/// One routed net's numbers.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NetRow {
    pub net: String,
    /// The length of all the net's tracks, in mm.
    pub length: f64,
    /// The shortest tree of straight lines between the centres of the net's pads, in mm.
    pub straight: f64,
    /// `length` over `straight`; 0 for a net whose pads are on one point.
    pub ratio: f64,
    pub vias: usize,
    /// The vias where the net's tracks meet on two layers or more.
    pub layer_changes: usize,
    /// The copper layers the net's tracks are on, front to back, as the board file names them.
    pub layers: Vec<String>,
}

/// One diff pair's numbers.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PairRow {
    /// The two nets, the positive first.
    pub p: String,
    pub n: String,
    /// Each net's track length, in mm.
    pub length_p: f64,
    pub length_n: f64,
    /// The difference of the two, in mm.
    pub skew: f64,
    /// The skew the pair may have, in mm.
    pub limit: f64,
    /// Whether `skew` is more than `limit`.
    pub over: bool,
    /// The length of `p`'s track that runs coupled, in mm.
    pub coupled: f64,
    /// `coupled` over `length_p`, 0 to 1.
    pub share: f64,
    /// The least and the largest gap, edge to edge, over the coupled length, in mm; `None` when
    /// nothing runs coupled.
    pub gaps: Option<(f64, f64)>,
    pub vias_p: usize,
    pub vias_n: usize,
    /// The vias of `p` with a via of `n` beside them.
    pub via_pairs: usize,
}

/// The table of a board ([`NetTable::of`]).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct NetTable {
    /// The board's name, which the caller sets: empty from [`NetTable::of`].
    pub board: String,
    /// The nets, by name.
    pub nets: Vec<NetRow>,
    /// The pairs, in the order given.
    pub pairs: Vec<PairRow>,
}

/// A straight piece of track: net, layer, its ends and its width.
type Seg = (String, String, Pt, Pt, f64);

/// The distance from `p` to the segment `a`-`b`.
fn to_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    p.dist(Pt::new(a.x + dx * t, a.y + dy * t))
}

/// The nets a zone of the board pours.
fn poured(art: &Artwork) -> Vec<&str> {
    art.zones
        .iter()
        .filter(|z| z.keepout.is_none() && !z.teardrop)
        .filter_map(|z| z.net.as_deref())
        .collect()
}

/// The numbers of each net of the board file no zone pours, with a track or a via, by name.
pub(crate) fn rows(art: &Artwork) -> Vec<NetRow> {
    let poured = poured(art);
    let routed = |net: &str| !net.is_empty() && !poured.contains(&net);
    let segs: Vec<Seg> = crate::score::lines(art)
        .into_iter()
        .filter(|s| routed(&s.0))
        .collect();
    let mut length: BTreeMap<&str, f64> = BTreeMap::new();
    let mut layers: BTreeMap<&str, BTreeSet<usize>> = BTreeMap::new();
    let order = |layer: &str| {
        art.copper
            .iter()
            .position(|c| c == layer)
            .unwrap_or(usize::MAX)
    };
    for (net, layer, a, b, _) in &segs {
        *length.entry(net).or_default() += a.dist(*b);
        layers.entry(net).or_default().insert(order(layer));
    }
    let mut vias: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for v in &art.vias {
        let Some(net) = v.net.as_deref().filter(|n| routed(n)) else {
            continue;
        };
        let met: BTreeSet<&str> = segs
            .iter()
            .filter(|s| s.0 == net)
            .filter(|s| {
                let reach = v.diameter / 2.0 + SAME;
                s.2.dist(v.at) <= reach || s.3.dist(v.at) <= reach
            })
            .map(|s| s.1.as_str())
            .collect();
        let counts = vias.entry(net).or_default();
        counts.0 += 1;
        if met.len() >= 2 {
            counts.1 += 1;
        }
    }
    let mut pads: BTreeMap<&str, Vec<Pt>> = BTreeMap::new();
    for p in &art.pads {
        if let Some(net) = p.net.as_deref().filter(|n| routed(n)) {
            let at = pads.entry(net).or_default();
            if !at.iter().any(|q| q.dist(p.at) <= SAME) {
                at.push(p.at);
            }
        }
    }
    let names: BTreeSet<&str> = length.keys().chain(vias.keys()).copied().collect();
    names
        .into_iter()
        .map(|net| {
            let length = length.get(net).copied().unwrap_or_default();
            let straight = pads.get(net).map_or(0.0, |pts| crate::score::prim(pts));
            let (vias, layer_changes) = vias.get(net).copied().unwrap_or_default();
            NetRow {
                net: net.to_string(),
                length,
                straight,
                ratio: if straight > SAME {
                    length / straight
                } else {
                    0.0
                },
                vias,
                layer_changes,
                layers: layers
                    .get(net)
                    .into_iter()
                    .flatten()
                    .filter_map(|&k| art.copper.get(k).cloned())
                    .collect(),
            }
        })
        .collect()
}

/// The numbers of the pair `p`, `n` from the board's straight pieces of track.
fn pair_row(art: &Artwork, segs: &[Seg], p: &str, n: &str, limit: f64) -> PairRow {
    let length = |net: &str| -> f64 {
        segs.iter()
            .filter(|s| s.0 == net)
            .map(|s| s.2.dist(s.3))
            .sum()
    };
    let (length_p, length_n) = (length(p), length(n));
    let (mut coupled, mut least, mut most) = (0.0, f64::MAX, f64::MIN);
    for (_, layer, a, b, width) in segs.iter().filter(|s| s.0 == p) {
        let piece = a.dist(*b);
        let steps = ((piece / SAMPLE).ceil() as usize).max(1);
        for k in 0..steps {
            let at = *a + (*b - *a) * ((k as f64 + 0.5) / steps as f64);
            let nearest = segs
                .iter()
                .filter(|s| s.0 == n && s.1 == *layer)
                .map(|s| (to_segment(at, s.2, s.3), s.4))
                .min_by(|x, y| x.0.total_cmp(&y.0));
            if let Some((d, w)) = nearest
                && d <= COUPLED
            {
                coupled += piece / steps as f64;
                let gap = d - width / 2.0 - w / 2.0;
                least = least.min(gap);
                most = most.max(gap);
            }
        }
    }
    let vias = |net: &str| -> Vec<(Pt, f64)> {
        art.vias
            .iter()
            .filter(|v| v.net.as_deref() == Some(net))
            .map(|v| (v.at, v.diameter))
            .collect()
    };
    let (vp, vn) = (vias(p), vias(n));
    let mut taken = vec![false; vn.len()];
    let mut via_pairs = 0;
    for (at, d) in &vp {
        let beside = vn
            .iter()
            .enumerate()
            .filter(|(k, (o, od))| !taken[*k] && at.dist(*o) - d / 2.0 - od / 2.0 <= VIA_PAIR_GAP)
            .min_by(|x, y| at.dist(x.1.0).total_cmp(&at.dist(y.1.0)))
            .map(|(k, _)| k);
        if let Some(k) = beside {
            taken[k] = true;
            via_pairs += 1;
        }
    }
    let skew = (length_p - length_n).abs();
    PairRow {
        p: p.to_string(),
        n: n.to_string(),
        length_p,
        length_n,
        skew,
        limit,
        over: skew > limit + 1e-9,
        coupled,
        share: if length_p > 0.0 {
            coupled / length_p
        } else {
            0.0
        },
        gaps: (coupled > 0.0).then_some((least, most)),
        vias_p: vp.len(),
        vias_n: vn.len(),
        via_pairs,
    }
}

impl PairRow {
    /// The note of `board drc` for a pair whose skew is over its limit.
    pub fn over_line(&self) -> String {
        format!(
            "pair skew: {}/{}: {} and {} mm routed, skew {} mm, over the {} mm the router matches a pair to; lengthen the shorter by hand or give the router room for a meander",
            self.p,
            self.n,
            number(self.length_p),
            number(self.length_n),
            number(self.skew),
            number(self.limit),
        )
    }
}

impl NetTable {
    /// The table of a board file, with its diff pairs `pairs` (the positive net first) and the
    /// skew a pair may have, `limit` in mm. A pair whose nets have no track is left out.
    pub fn of(art: &Artwork, pairs: &[(String, String)], limit: f64) -> NetTable {
        let segs = crate::score::lines(art);
        let has = |net: &str| segs.iter().any(|s| s.0 == net);
        NetTable {
            board: String::new(),
            nets: rows(art),
            pairs: pairs
                .iter()
                .filter(|(p, n)| has(p) && has(n))
                .map(|(p, n)| pair_row(art, &segs, p, n, limit))
                .collect(),
        }
    }

    /// The nets longest first, by name on a tie.
    pub fn by_length(&mut self) {
        self.nets
            .sort_by(|a, b| b.length.total_cmp(&a.length).then(a.net.cmp(&b.net)));
    }

    /// The pairs whose skew is over their limit.
    pub fn over(&self) -> impl Iterator<Item = &PairRow> {
        self.pairs.iter().filter(|p| p.over)
    }

    /// One line for each pair whose skew is over its limit, for the notes of `board drc`; none
    /// when every pair is within.
    pub fn over_lines(&self) -> Vec<String> {
        self.over().map(PairRow::over_line).collect()
    }

    /// The table as text for a person: a line for each net, then a line for each pair.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let width = self
            .nets
            .iter()
            .map(|n| n.net.len())
            .max()
            .unwrap_or(3)
            .max(3);
        out.push_str(&format!(
            "{:width$}  {:>8}  {:>8}  {:>5}  {:>4}  {:>7}  layers\n",
            "net", "routed", "straight", "ratio", "vias", "changes"
        ));
        for n in &self.nets {
            out.push_str(&format!(
                "{:width$}  {:>8.2}  {:>8.2}  {:>5}  {:>4}  {:>7}  {}\n",
                n.net,
                n.length,
                n.straight,
                if n.ratio > 0.0 {
                    format!("{:.2}", n.ratio)
                } else {
                    "-".to_string()
                },
                n.vias,
                n.layer_changes,
                n.layers.join(" "),
            ));
        }
        for p in &self.pairs {
            let gaps = match p.gaps {
                Some((least, most)) => format!(", gap {least:.3} to {most:.3} mm"),
                None => String::new(),
            };
            out.push_str(&format!(
                "pair {}/{}: {:.2} and {:.2} mm, skew {:.2} mm ({}the {} mm limit), coupled {:.2} mm ({:.0}%){gaps}, vias {} and {}, {} side by side\n",
                p.p,
                p.n,
                p.length_p,
                p.length_n,
                p.skew,
                if p.over { "over " } else { "within " },
                number(p.limit),
                p.coupled,
                p.share * 100.0,
                p.vias_p,
                p.vias_n,
                p.via_pairs,
            ));
        }
        out
    }
}

/// A number with at most two decimals and no trailing zeros: `3`, `2.45`.
fn number(x: f64) -> String {
    let text = format!("{x:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}
