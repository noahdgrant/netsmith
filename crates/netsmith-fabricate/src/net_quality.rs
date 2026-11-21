//! The quality of a board's routed copper (issue #1481): for each net, the defects a person sees
//! by eye on a board that passes the design rules check with every connection made, counted, the
//! worst nets first. `netsmith board nets` prints it as its `quality` section (and fails with
//! `--gate` when a count is over its limit), and `board drc` notes the totals. The router's and
//! `routes tidy`'s own checks of a routed board count by it, so the measure is one.
//!
//! The copper is the board's tracks and vias as they stand (what `view pcb` draws), of the nets
//! no zone, pour or plane pours: a poured net's tracks and vias are joined by its fill, which
//! these counts do not see, as `board nets` leaves those nets out of its table. The kinds:
//!
//! - **crossings**: two straight tracks of one net on one layer whose centre lines cross away
//!   from the ends of both, a loop of the net's own copper;
//! - **duplicates**: the second ways between the same two places of a net, as `routes tidy`
//!   finds them (`routes_tidy`'s duplicate ways: a way is a run of tracks and the vias it changes
//!   layer by, between a pad or a place where other than two tracks meet);
//! - **unneeded**: tracks and vias no pad needs: a track or a via that ends in nothing (a dead
//!   stub, a dangling via, a dead branch of several tracks, each track and via of it), a track
//!   from a pad's or a via's centre to a point still inside it (a nudge) and a track whose copper
//!   lies inside, or overlaps along its line, another track of its net and layer, as `routes
//!   tidy` finds the last two;
//! - **short**: straight tracks shorter than [`crate::routes_tidy::TINY`] of their widths, the
//!   tracks `routes tidy` makes one of when several are in a row;
//! - **via pairs**: two vias of one net side by side, each via taken once, the nearest first:
//!   their copper no more than the group gap apart edge to edge (`Rules::group_gap`, the
//!   process's least clearance unless the board states `.process.via_group_gap`, issue #1505),
//!   the rule by which `via_current` and the router group vias. A group of vias side by side
//!   (through others that are) no larger than the net's current needs is wanted copper and no
//!   defect.
//!
//! Each track and via is counted once: as unneeded first, then in a duplicate way only when the
//! way has a track that is not unneeded (a nudge from a pad's centre back into the pad is a way
//! of its own to tidy's detector too), and as short only when it is in neither.
//!
//! After the objective, a line for each power net ([`PowerNet`], issue #1532): its layers, its
//! layer changes and its via groups, against the vias side by side its current needs.
//!
//! The limits of `--gate` are [`Counts::LIMITS`] unless a `--max-<kind>` flag gives another: 0
//! of each, the zero-defect check, and the report states the limits it was judged by.

use std::collections::{BTreeMap, BTreeSet};

use netsmith_pcb::{Board, Pt, Track};
use serde::Serialize;

use crate::routes_tidy::{Kind, TINY, shape_defects};

/// How many defects of each kind, of a net or of the board.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub crossings: usize,
    pub duplicates: usize,
    pub unneeded: usize,
    pub short: usize,
    pub via_pairs: usize,
}

impl Counts {
    /// The limits of `board nets --gate` when no `--max-<kind>` flag gives one: none of each.
    pub const LIMITS: Counts = Counts {
        crossings: 0,
        duplicates: 0,
        unneeded: 0,
        short: 0,
        via_pairs: 0,
    };

    pub fn total(&self) -> usize {
        self.crossings + self.duplicates + self.unneeded + self.short + self.via_pairs
    }

    /// Each kind by its name (as JSON and the `--max-<kind>` flags have it) and its count.
    pub fn kinds(&self) -> [(&'static str, usize); 5] {
        [
            ("crossings", self.crossings),
            ("duplicates", self.duplicates),
            ("unneeded", self.unneeded),
            ("short", self.short),
            ("via_pairs", self.via_pairs),
        ]
    }

    fn add(&mut self, o: &Counts) {
        self.crossings += o.crossings;
        self.duplicates += o.duplicates;
        self.unneeded += o.unneeded;
        self.short += o.short;
        self.via_pairs += o.via_pairs;
    }

    /// The counts said in words: "2 own-net crossing(s), 1 duplicate way(s), ...".
    pub fn words(&self) -> String {
        format!(
            "{} own-net crossing(s), {} duplicate way(s), {} unneeded track(s) and via(s), {} short track(s), {} via pair(s) side by side",
            self.crossings, self.duplicates, self.unneeded, self.short, self.via_pairs
        )
    }
}

/// The counts of one net.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetQuality {
    pub net: String,
    #[serde(flatten)]
    pub counts: Counts,
}

/// The quality of a board's copper: the totals and each net with a defect, the worst first.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Quality {
    pub totals: Counts,
    /// The nets with at least one defect, the most defects first, then by name.
    pub nets: Vec<NetQuality>,
    /// What the router's quality objective weighs, over the nets of the table (issue #1517):
    /// `None` until [`Quality::with_objective`] gives it the table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objective: Option<Objective>,
    /// The power nets' layer changes and via groups ([`PowerNet`], issue #1532).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub power: Vec<PowerNet>,
}

/// A power net's copper (issue #1532): a net with a known current (`Board::net_currents`) that
/// needs more than one via side by side where it changes layer, with track, and that no zone,
/// pour or plane fills. The router keeps such a net to one layer where it can, and changes its
/// layer through a group of vias sized for its current; this says how well it did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PowerNet {
    pub net: String,
    /// The net's current, in amperes, and how many vias side by side carry it.
    pub current: f64,
    pub vias_needed: usize,
    /// How many copper layers its tracks are on.
    pub layers: usize,
    pub vias: usize,
    /// Its vias in groups side by side: copper no more than the group gap apart, through others
    /// that are, as `via_current` groups them (`Rules::group_gap`, issue #1505).
    pub groups: usize,
    /// The groups its tracks meet on two layers or more: each a change of layer.
    pub layer_changes: usize,
    /// The groups with fewer vias than its current needs.
    pub short_groups: usize,
}

impl PowerNet {
    /// The power nets of `board`, by name.
    pub fn of_board(board: &Board) -> Vec<PowerNet> {
        let poured = |net: &str| {
            board.zones.iter().any(|z| z.net == net)
                || board.pours.iter().any(|p| p.net == net)
                || board.planes.iter().any(|p| p.net == net)
        };
        let gap = board.rules.group_gap();
        let mut out: Vec<PowerNet> = board
            .net_currents
            .iter()
            .filter(|c| c.vias > 1 && !poured(&c.net))
            .filter_map(|c| {
                let tracks: Vec<&Track> = board.tracks.iter().filter(|t| t.net == c.net).collect();
                if tracks.is_empty() {
                    return None;
                }
                let layers: BTreeSet<&str> = tracks.iter().map(|t| t.layer.word()).collect();
                let vias: Vec<(Pt, f64)> = board
                    .vias
                    .iter()
                    .filter(|v| v.net == c.net)
                    .map(|v| (v.at, v.diameter))
                    .collect();
                let groups = via_groups(&vias, gap);
                let layer_changes = groups
                    .iter()
                    .filter(|group| {
                        let met: BTreeSet<&str> = tracks
                            .iter()
                            .filter(|t| {
                                group.iter().any(|&g| {
                                    let (at, d) = vias[g];
                                    let reach = d / 2.0 + NEAR;
                                    t.from.distance(at) <= reach || t.to.distance(at) <= reach
                                })
                            })
                            .map(|t| t.layer.word())
                            .collect();
                        met.len() >= 2
                    })
                    .count();
                Some(PowerNet {
                    net: c.net.clone(),
                    current: c.current,
                    vias_needed: c.vias,
                    layers: layers.len(),
                    vias: vias.len(),
                    groups: groups.len(),
                    layer_changes,
                    short_groups: groups.iter().filter(|g| g.len() < c.vias).count(),
                })
            })
            .collect();
        out.sort_by(|a, b| a.net.cmp(&b.net));
        out
    }

    /// The net as a line of text for a person.
    pub fn text(&self) -> String {
        let short = if self.short_groups > 0 {
            format!(", {} group(s) short", self.short_groups)
        } else {
            String::new()
        };
        format!(
            "power net {}: {:.2} A, on {} layer(s), {} layer change(s), {} via(s) in {} group(s) of the {} side by side its current needs{short}\n",
            self.net,
            self.current,
            self.layers,
            self.layer_changes,
            self.vias,
            self.groups,
            self.vias_needed
        )
    }
}

/// How long a net may be against its straight length before the objective names it: twice.
pub const DETOUR_LIMIT: f64 = 2.0;

/// How many of the nets over [`DETOUR_LIMIT`] the objective names, the longest ratio first.
pub const DETOUR_NAMED: usize = 5;

/// What the router's quality objective weighs (issue #1517), over the routed nets of the board's
/// table (`netsmith_fab::NetTable`: the nets no zone pours, with a track or a via): their length
/// and straight length together and the ratio of the two (the detour), their vias and layer
/// changes (a via where the net's tracks meet on two layers or more), and the nets routed over
/// [`DETOUR_LIMIT`] times their straight length.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Objective {
    /// The nets' routed length and straight length, in mm, and the one over the other.
    pub length: f64,
    pub straight: f64,
    pub detour: f64,
    pub vias: usize,
    pub layer_changes: usize,
    /// How many nets are over [`DETOUR_LIMIT`] times their straight length.
    pub over_twice: usize,
    /// The first [`DETOUR_NAMED`] of them, the longest ratio first.
    pub worst: Vec<Detour>,
}

/// A net routed long against its straight length.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Detour {
    pub net: String,
    pub ratio: f64,
    pub vias: usize,
    pub layer_changes: usize,
}

impl Objective {
    /// The objective of the nets of `table`.
    pub fn of_table(table: &netsmith_fab::NetTable) -> Objective {
        let mut o = Objective::default();
        let mut long = Vec::new();
        for row in &table.nets {
            o.length += row.length;
            o.straight += row.straight;
            o.vias += row.vias;
            o.layer_changes += row.layer_changes;
            if row.ratio > DETOUR_LIMIT {
                long.push(Detour {
                    net: row.net.clone(),
                    ratio: row.ratio,
                    vias: row.vias,
                    layer_changes: row.layer_changes,
                });
            }
        }
        o.detour = if o.straight > 0.0 {
            o.length / o.straight
        } else {
            0.0
        };
        long.sort_by(|a, b| b.ratio.total_cmp(&a.ratio).then_with(|| a.net.cmp(&b.net)));
        o.over_twice = long.len();
        long.truncate(DETOUR_NAMED);
        o.worst = long;
        o
    }

    /// The objective as a line of text for a person.
    pub fn text(&self) -> String {
        let mut out = format!(
            "objective: {:.1} mm of track over a straight {:.1} mm ({:.2}x), {} via(s), {} layer change(s), {} net(s) over {DETOUR_LIMIT} times their straight length",
            self.length, self.straight, self.detour, self.vias, self.layer_changes, self.over_twice
        );
        if !self.worst.is_empty() {
            let named: Vec<String> = self
                .worst
                .iter()
                .map(|d| format!("{} {:.2}x", d.net, d.ratio))
                .collect();
            out.push_str(&format!(" ({})", named.join(", ")));
        }
        out.push('\n');
        out
    }
}

/// A kind whose total is over its limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Over {
    pub kind: &'static str,
    pub count: usize,
    pub limit: usize,
}

impl Quality {
    /// The quality of `board`'s copper as it stands.
    pub fn of_board(board: &Board) -> Quality {
        let poured: BTreeSet<&str> = board
            .zones
            .iter()
            .map(|z| z.net.as_str())
            .chain(board.pours.iter().map(|p| p.net.as_str()))
            .chain(board.planes.iter().map(|p| p.net.as_str()))
            .collect();
        let mut by: BTreeMap<String, Counts> = BTreeMap::new();
        // Each track and via is counted once: as unneeded copper first, then in a duplicate
        // way only when the way has a track that is not unneeded (a nudge from a pad's centre is
        // a way of its own too), then as short only when it is in neither.
        let defects = shape_defects(board);
        let mut gone_wires = BTreeSet::new();
        let mut gone_holes = BTreeSet::new();
        for d in defects.iter().filter(|d| d.kind != Kind::Duplicate) {
            if poured.contains(d.net.as_str()) {
                continue;
            }
            let fresh = d.wires.iter().filter(|&&i| gone_wires.insert(i)).count()
                + d.holes.iter().filter(|&&i| gone_holes.insert(i)).count();
            if fresh > 0 {
                by.entry(d.net.clone()).or_default().unneeded += fresh;
            }
        }
        let mut dup_wires = BTreeSet::new();
        for d in defects.iter().filter(|d| d.kind == Kind::Duplicate) {
            if !poured.contains(d.net.as_str()) && d.wires.iter().any(|i| !gone_wires.contains(i)) {
                by.entry(d.net.clone()).or_default().duplicates += 1;
                dup_wires.extend(d.wires.iter().copied());
            }
        }
        gone_wires.extend(dup_wires);
        let mut tracks: BTreeMap<(&str, &'static str), Vec<&Track>> = BTreeMap::new();
        for (i, t) in board.tracks.iter().enumerate() {
            if poured.contains(t.net.as_str()) || t.net.is_empty() {
                continue;
            }
            if t.mid.is_none() && !gone_wires.contains(&i) && t.from.distance(t.to) < TINY * t.width
            {
                by.entry(t.net.clone()).or_default().short += 1;
            }
            tracks.entry((&t.net, t.layer.word())).or_default().push(t);
        }
        for ((net, _), list) in &tracks {
            let n = crossings(list);
            if n > 0 {
                by.entry(net.to_string()).or_default().crossings += n;
            }
        }
        let mut vias: BTreeMap<&str, Vec<(Pt, f64)>> = BTreeMap::new();
        for v in &board.vias {
            if !poured.contains(v.net.as_str()) && !v.net.is_empty() {
                vias.entry(&v.net).or_default().push((v.at, v.diameter));
            }
        }
        let gap = board.rules.group_gap();
        for (net, list) in &vias {
            let needs = board
                .net_currents
                .iter()
                .find(|c| c.net == *net)
                .map_or(1, |c| c.vias);
            let n = via_pairs(list, gap, needs);
            if n > 0 {
                by.entry(net.to_string()).or_default().via_pairs += n;
            }
        }
        let mut totals = Counts::default();
        let mut nets: Vec<NetQuality> = by
            .into_iter()
            .filter(|(_, c)| c.total() > 0)
            .map(|(net, counts)| {
                totals.add(&counts);
                NetQuality { net, counts }
            })
            .collect();
        nets.sort_by(|a, b| {
            b.counts
                .total()
                .cmp(&a.counts.total())
                .then_with(|| a.net.cmp(&b.net))
        });
        Quality {
            totals,
            nets,
            objective: None,
            power: PowerNet::of_board(board),
        }
    }

    /// The quality with the objective of the nets of `table` (the board's net table).
    pub fn with_objective(self, table: &netsmith_fab::NetTable) -> Quality {
        Quality {
            objective: Some(Objective::of_table(table)),
            ..self
        }
    }

    /// The kinds whose total is over its limit in `limits`.
    pub fn over(&self, limits: &Counts) -> Vec<Over> {
        self.totals
            .kinds()
            .into_iter()
            .zip(limits.kinds())
            .filter(|((_, count), (_, limit))| count > limit)
            .map(|((kind, count), (_, limit))| Over { kind, count, limit })
            .collect()
    }

    /// The section as text for a person: a line of the totals, then a line for each net with
    /// a defect, the worst first.
    pub fn text(&self) -> String {
        let mut out = format!("quality: {}\n", self.totals.words());
        if let Some(o) = &self.objective {
            out.push_str(&o.text());
        }
        for p in &self.power {
            out.push_str(&p.text());
        }
        if self.nets.is_empty() {
            return out;
        }
        let width = self
            .nets
            .iter()
            .map(|n| n.net.len())
            .max()
            .unwrap_or(3)
            .max(3);
        out.push_str(&format!(
            "  {:width$}  {:>9}  {:>10}  {:>8}  {:>5}  {:>9}\n",
            "net", "crossings", "duplicates", "unneeded", "short", "via pairs"
        ));
        for n in &self.nets {
            let c = &n.counts;
            out.push_str(&format!(
                "  {:width$}  {:>9}  {:>10}  {:>8}  {:>5}  {:>9}\n",
                n.net, c.crossings, c.duplicates, c.unneeded, c.short, c.via_pairs
            ));
        }
        out
    }

    /// The note of `board drc`: the totals, when there is a defect.
    pub fn note(&self) -> Option<String> {
        (self.totals.total() > 0).then(|| {
            let worst: Vec<&str> = self.nets.iter().take(3).map(|n| n.net.as_str()).collect();
            format!(
                "routing quality: {} (worst: {}; `board nets` lists each net)",
                self.totals.words(),
                worst.join(", ")
            )
        })
    }
}

/// Points nearer than this are one, in mm.
const NEAR: f64 = 1e-3;

/// How many pairs of the straight tracks `list` (of one net and layer) cross away from their
/// ends.
fn crossings(list: &[&Track]) -> usize {
    let straight: Vec<&&Track> = list.iter().filter(|t| t.mid.is_none()).collect();
    let mut n = 0;
    for (i, a) in straight.iter().enumerate() {
        for b in &straight[i + 1..] {
            if let Some(p) = crossing(a.from, a.to, b.from, b.to)
                && [a.from, a.to, b.from, b.to]
                    .iter()
                    .all(|e| e.distance(p) >= NEAR)
            {
                n += 1;
            }
        }
    }
    n
}

/// Where the segments `a0`-`a1` and `b0`-`b1` cross, when they do at one point.
fn crossing(a0: Pt, a1: Pt, b0: Pt, b1: Pt) -> Option<Pt> {
    let (r, s) = (a1 - a0, b1 - b0);
    let d = r.x * s.y - r.y * s.x;
    if d.abs() < 1e-12 {
        return None;
    }
    let w = b0 - a0;
    let t = (w.x * s.y - w.y * s.x) / d;
    let u = (w.x * r.y - w.y * r.x) / d;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u))
        .then(|| Pt::new(a0.x + r.x * t, a0.y + r.y * t))
}

/// The vias `list` (of one net, each its centre and diameter) in groups side by side: their
/// copper no more than `gap` apart, edge to edge, through others that are. Each group is its
/// vias' indices into `list`.
fn via_groups(list: &[(Pt, f64)], gap: f64) -> Vec<Vec<usize>> {
    let beside = |i: usize, k: usize| {
        let ((a, d), (b, e)) = (list[i], list[k]);
        a.distance(b) - d / 2.0 - e / 2.0 <= gap + 1e-9
    };
    let mut seen = vec![false; list.len()];
    let mut groups = Vec::new();
    for start in 0..list.len() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut group = vec![start];
        let mut k = 0;
        while k < group.len() {
            let at = group[k];
            for (j, seen) in seen.iter_mut().enumerate() {
                if !*seen && beside(at, j) {
                    *seen = true;
                    group.push(j);
                }
            }
            k += 1;
        }
        groups.push(group);
    }
    groups
}

/// How many pairs of the vias `list` (of one net, each its centre and diameter) stand side by
/// side, their copper no more than `gap` apart, each via taken once, the nearest first; the
/// vias of a group side by side (through others that are) of no more than `needs` vias, what
/// the net's current needs, are left out.
fn via_pairs(list: &[(Pt, f64)], gap: f64, needs: usize) -> usize {
    let beside = |i: usize, k: usize| {
        let ((a, d), (b, e)) = (list[i], list[k]);
        a.distance(b) - d / 2.0 - e / 2.0 <= gap + 1e-9
    };
    // The vias of a group the current needs count as taken.
    let mut taken = vec![false; list.len()];
    for group in via_groups(list, gap) {
        if group.len() <= needs.max(1) && group.len() > 1 {
            for g in group {
                taken[g] = true;
            }
        }
    }
    let mut n = 0;
    for i in 0..list.len() {
        if taken[i] {
            continue;
        }
        let at = list[i].0;
        let beside = (i + 1..list.len())
            .filter(|&k| !taken[k] && beside(i, k))
            .min_by(|&x, &y| at.distance(list[x].0).total_cmp(&at.distance(list[y].0)));
        if let Some(k) = beside {
            taken[i] = true;
            taken[k] = true;
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_tracks_that_cross_away_from_their_ends_are_a_crossing_and_a_tee_is_not() {
        let t = |a: (f64, f64), b: (f64, f64)| {
            Track::straight(
                "n",
                netsmith_pcb::CopperLayer::Front,
                Pt::new(a.0, a.1),
                Pt::new(b.0, b.1),
                0.2,
            )
        };
        let (a, b) = (t((0.0, 0.0), (2.0, 2.0)), t((0.0, 2.0), (2.0, 0.0)));
        assert_eq!(crossings(&[&a, &b]), 1);
        // One ends on the middle of the other: a tee, which joins and loops nothing.
        let c = t((1.0, 1.0), (1.0, 3.0));
        assert_eq!(crossings(&[&a, &c]), 0);
        // End to end: a bend.
        let d = t((2.0, 2.0), (4.0, 2.0));
        assert_eq!(crossings(&[&a, &d]), 0);
    }

    #[test]
    fn vias_side_by_side_are_counted_once_each() {
        let v = |x: f64| (Pt::new(x, 0.0), 0.6);
        // Copper 0.2 mm apart, then 0.4 mm on, then 2 mm on, judged by a 0.3 mm gap.
        assert_eq!(via_pairs(&[v(0.0), v(0.8), v(1.8), v(4.4)], 0.3, 1), 1);
        assert_eq!(via_pairs(&[v(0.0), v(0.8), v(1.6), v(2.4)], 0.3, 1), 2);
        assert_eq!(via_pairs(&[v(0.0), v(3.0)], 0.3, 1), 0);
    }

    /// The gap is the group rule's (issue #1505): copper 0.15 mm apart is a pair at the
    /// least clearance of 0.15 mm and none at 0.1 mm; and a group the net's current needs is
    /// wanted, one larger than it is not.
    #[test]
    fn via_pairs_follow_the_group_gap_and_leave_out_a_group_the_current_needs() {
        let v = |x: f64| (Pt::new(x, 0.0), 0.45);
        let row = [v(0.0), v(0.6)];
        assert_eq!(via_pairs(&row, 0.15, 1), 1);
        assert_eq!(via_pairs(&row, 0.1, 1), 0);
        assert_eq!(via_pairs(&row, 0.15, 2), 0);
        let three = [v(0.0), v(0.6), v(1.2)];
        assert_eq!(via_pairs(&three, 0.15, 3), 0);
        assert_eq!(via_pairs(&three, 0.15, 2), 1);
    }

    #[test]
    fn the_gate_names_each_kind_over_its_limit() {
        let q = Quality {
            totals: Counts {
                crossings: 2,
                short: 1,
                ..Counts::default()
            },
            nets: Vec::new(),
            objective: None,
            power: Vec::new(),
        };
        let over = q.over(&Counts::LIMITS);
        assert_eq!(
            over.iter().map(|o| (o.kind, o.count)).collect::<Vec<_>>(),
            [("crossings", 2), ("short", 1)]
        );
        let over = q.over(&Counts {
            crossings: 2,
            ..Counts::LIMITS
        });
        assert_eq!(over.len(), 1);
    }
}
