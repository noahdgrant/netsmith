//! `netsmith routes route --region X,Y,W,H` (issue #1641): the unlocked copper inside a box of the
//! board ripped up and every connection it carried routed again by the router's negotiation, with
//! the copper outside the box as it is.
//!
//! **What is ripped.** The box is in the routes file's places (mm from the outline's top left
//! corner, y down). Every unlocked track of a record that is not locked and is current (its pads
//! have not moved) is cut where its centre line crosses the box's edge: the pieces inside are
//! removed and those outside kept, an arc as arcs of the same circle. An unlocked via whose
//! centre is inside is removed. Locked shapes, the copper of locked records, records written for
//! pads that have moved since, and zones are never touched.
//!
//! **What is routed.** The nets one of whose connections the cut opens (its ratsnest has more
//! lines than before), and the nets with a connection open before whose straight line crosses
//! the box, are routed together by the whole-board router ([`one::route_nets_weighed`]: its
//! orders, rip-ups, negotiation and rounds of rip-up among their lines, every other net held as
//! it is), with the copper left as fixed obstacles. The router joins pads, so each end the cut
//! left on an outer layer, of a net one of whose connections the cut opened, is given to it as a pad of one point, the track's width round, on the
//! track's layer, which the copper outside reaches ([`terminals`]): the line the router routes
//! then runs between the two cut ends of a connection, inside or near the box, not from pad to
//! pad round the board. These pads are the router's only: the copper it laid is checked on the
//! board as it is. A cut end on an inner layer gets none, and its connection is routed from its
//! pads.
//!
//! **Joins.** Where the router's copper meets a piece the cut left at its end and turns by more
//! than 45 degrees there (a bend the design rules check refuses outside a pad), the corner is cut
//! back on both tracks and replaced by short straight tracks that each turn by 45 degrees at most
//! ([`smooth`]), within the corner, so the space to other copper only grows.
//!
//! **Dead copper.** A piece the cut left of a net one of whose connections the cut opened, whose
//! end the new copper does not reach, runs into nothing (a piece of copper that was dead before,
//! of a net whose connections the cut opened none of, is kept); it is removed, and then each unlocked track or via of its net that this leaves open,
//! back to where the copper still joins something ([`prune`]). A removal that would open a
//! connection is not made.
//!
//! **What is kept.** The change is written only when no net has more connections open than it had
//! before the cut, and either fewer are open in all or the copper of the nets that changed costs
//! no more by the weights (length and vias, [`netsmith_router::Weights::measure`]) than before.
//! A region that cannot be closed is refused and names the connections left open; one that costs
//! more says both totals. Either way nothing is written. The records that change are put in the
//! file as one edit ([`crate::routes::edit_all`]), so a result that adds a finding of the design
//! rules check is refused too.
//!
//! **Bounded work, counted.** One run of the router, with its own bounds (all counts); the
//! pruning makes at most one check of the board for each track and via of the nets that changed.

use std::collections::{BTreeMap, BTreeSet};

use netsmith_pcb::{
    Board, CopperLayer, NetRoutes, PadRef, Pt, Ratline, Ratsnest, Rect, RouteTrack, RouteVia,
    Routes,
};
use netsmith_router::one;

use crate::route_net::{Total, add, total, um};
use crate::routes::{Edit, Refusal, edit_all, with_routes};

/// A piece shorter than this, in mm, is no copper: a cut that falls this near a track's end
/// leaves nothing outside.
const SLIVER: f64 = 0.001;

/// What to rip and route: the box, in the routes file's places, and the weights.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionAsk {
    /// `[x, y, w, h]`, mm from the outline's top left corner, y down.
    pub rect: [f64; 4],
    /// The weights of the router's quality objective; `None` is the router's own costs.
    pub weights: Option<netsmith_router::Weights>,
}

/// The box of a region, in the routes file's places.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Region {
    pub min: [f64; 2],
    pub max: [f64; 2],
}

impl Region {
    fn of(rect: [f64; 4]) -> Region {
        Region {
            min: [rect[0], rect[1]],
            max: [rect[0] + rect[2], rect[1] + rect[3]],
        }
    }

    /// Whether `p` is strictly inside: a point on the edge is outside.
    fn holds(&self, p: [f64; 2]) -> bool {
        p[0] > self.min[0] && p[0] < self.max[0] && p[1] > self.min[1] && p[1] < self.max[1]
    }

    /// Whether the straight line from `a` to `b` runs through the box.
    fn crossed_by(&self, a: [f64; 2], b: [f64; 2]) -> bool {
        if self.holds(a) || self.holds(b) {
            return true;
        }
        let mut s = segment_cuts(a, b, self);
        s.extend([0.0, 1.0]);
        s.sort_by(f64::total_cmp);
        s.windows(2).any(|w| {
            let m = (w[0] + w[1]) / 2.0;
            self.holds([a[0] + m * (b[0] - a[0]), a[1] + m * (b[1] - a[1])])
        })
    }
}

/// What `routes route --region` did, or would do: the module's header.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RegionRouted {
    pub board: String,
    pub region: Region,
    /// Each record whose copper the region changes.
    pub nets: Vec<RegionNet>,
    /// The connections the cut opened, and those open before that run through the box.
    pub opened: Vec<String>,
    pub through: Vec<String>,
    /// The cut ends given to the router as pads ([`terminals`]).
    pub cut_ends: usize,
    /// The connections the cut opened that are still open after the router: what keeps the
    /// region from being closed.
    pub left: Vec<String>,
    /// What the copper of the nets that changed costs by the weights, before and after.
    pub total: Option<Total>,
    /// Why nothing is (or would be) written, when the region cannot be closed.
    pub refused: Option<String>,
    /// The orders of the lines the router tried, and the checks the pruning made.
    pub tries: usize,
    pub checks: usize,
    /// The routes file with the change, when it is kept.
    #[serde(skip)]
    pub routes: Option<Routes>,
}

/// What the region did to one record ([`RegionRouted::nets`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RegionNet {
    pub net: String,
    /// Tracks cut at the edge (their outside part kept), tracks wholly inside and vias inside
    /// removed.
    pub cut_tracks: usize,
    pub removed_tracks: usize,
    pub removed_vias: usize,
    /// The tracks and vias the router laid.
    pub tracks: Vec<RouteTrack>,
    pub vias: Vec<RouteVia>,
    /// The tracks and vias that ran into nothing after it and were removed ([`prune`]).
    pub pruned_tracks: usize,
    pub pruned_vias: usize,
}

/// An end the cut left: where the copper outside meets the box.
#[derive(Debug, Clone, PartialEq)]
struct CutEnd {
    net: String,
    layer: CopperLayer,
    width: f64,
    at: [f64; 2],
}

/// Rip up `ask`'s box of `routes` on `board` and route what it carried again: the module's
/// header. The error is a failure to read the board or the file.
pub fn route_region(
    board: &Board,
    routes: &Routes,
    ask: &RegionAsk,
) -> Result<RegionRouted, String> {
    let region = Region::of(ask.rect);
    let mut out = RegionRouted {
        board: board.name.clone(),
        region,
        nets: Vec::new(),
        opened: Vec::new(),
        through: Vec::new(),
        cut_ends: 0,
        left: Vec::new(),
        total: None,
        refused: None,
        tries: 0,
        checks: 0,
        routes: None,
    };
    if !(ask.rect[2] > 0.0 && ask.rect[3] > 0.0) {
        out.refused = Some(format!(
            "the region {} by {} mm has no area: give it a width and a height above 0",
            ask.rect[2], ask.rect[3]
        ));
        return Ok(out);
    }
    let (real, _) = with_routes(board, routes)?;
    let before = Ratsnest::of(&real);
    let (cut, ends, counts) = cut(board, routes, &region);
    let (cut_board, _) = with_routes(board, &cut)?;
    let after_cut = Ratsnest::of(&cut_board);
    let per_net = |r: &Ratsnest| {
        let mut m: BTreeMap<String, usize> = BTreeMap::new();
        for l in &r.lines {
            *m.entry(l.net.clone()).or_default() += 1;
        }
        m
    };
    let (open_before, open_cut) = (per_net(&before), per_net(&after_cut));
    let said: BTreeSet<String> = before.lines.iter().map(ToString::to_string).collect();
    let mut routed: BTreeSet<String> = BTreeSet::new();
    let mut cut_open: BTreeSet<String> = BTreeSet::new();
    for (net, &n) in &open_cut {
        if n > open_before.get(net).copied().unwrap_or(0) {
            routed.insert(net.clone());
            cut_open.insert(net.clone());
            let lines: Vec<&Ratline> = after_cut.lines.iter().filter(|l| &l.net == net).collect();
            let new: Vec<String> = lines
                .iter()
                .map(ToString::to_string)
                .filter(|s| !said.contains(s))
                .collect();
            out.opened.extend(if new.is_empty() {
                lines.iter().map(ToString::to_string).collect()
            } else {
                new
            });
        }
    }
    let file_at = |p: Pt| {
        let q = p - board.outline.min;
        [q.x, q.y]
    };
    for l in &before.lines {
        if !l.to_plane
            && !board.fixed.is_locked(&l.net)
            && !real.fixed.is_locked(&l.net)
            && region.crossed_by(file_at(l.from_at), file_at(l.to_at))
        {
            routed.insert(l.net.clone());
            out.through.push(l.to_string());
        }
    }
    let changed: BTreeSet<String> = counts
        .keys()
        .cloned()
        .chain(routed.iter().cloned())
        .collect();
    if changed.is_empty() {
        return Ok(out);
    }
    // The router, with the cut ends as pads of one point.
    let mut state = cut.clone();
    let mut laid: BTreeMap<String, (Vec<RouteTrack>, Vec<RouteVia>)> = BTreeMap::new();
    if !routed.is_empty() {
        let ends: Vec<&CutEnd> = ends.iter().filter(|e| cut_open.contains(&e.net)).collect();
        let (guided, given) = terminals(board, &cut_board, &ends);
        out.cut_ends = given;
        let way = one::route_nets_weighed(&guided, &routed, ask.weights);
        out.tries = way.tries;
        let had = state.clone();
        for net in &routed {
            add(board, &mut state, net, &way);
        }
        for net in &routed {
            let record = |r: &Routes| {
                r.boards
                    .get(&board.name)
                    .and_then(|t| t.nets.get(net))
                    .cloned()
                    .unwrap_or_default()
            };
            let (was, now) = (record(&had), record(&state));
            laid.insert(
                net.clone(),
                (
                    now.tracks[was.tracks.len().min(now.tracks.len())..].to_vec(),
                    now.vias[was.vias.len().min(now.vias.len())..].to_vec(),
                ),
            );
        }
    }
    smooth(&mut state, board, &cut_open, &ends, &laid);
    let pruned = prune(board, &mut state, &cut_open, &ends, &mut out.checks)?;
    let (after_board, _) = with_routes(board, &state)?;
    let after = Ratsnest::of(&after_board);
    let open_after = per_net(&after);
    let worse: BTreeSet<&String> = open_after
        .iter()
        .filter(|(n, k)| **k > open_before.get(*n).copied().unwrap_or(0))
        .map(|(n, _)| n)
        .collect();
    out.left = after
        .lines
        .iter()
        .filter(|l| worse.contains(&l.net))
        .map(ToString::to_string)
        .collect();
    for net in &changed {
        let (c, r, v) = counts.get(net).copied().unwrap_or((0, 0, 0));
        let (tracks, vias) = laid.remove(net).unwrap_or_default();
        let (pt, pv) = pruned.get(net).copied().unwrap_or((0, 0));
        out.nets.push(RegionNet {
            net: net.clone(),
            cut_tracks: c,
            removed_tracks: r,
            removed_vias: v,
            tracks,
            vias,
            pruned_tracks: pt,
            pruned_vias: pv,
        });
    }
    if !out.left.is_empty() {
        out.refused = Some(format!(
            "the region cannot be closed: {} connection(s) its copper carried found no way back ({}), so nothing is written",
            out.left.len(),
            out.left.join("; ")
        ));
        return Ok(out);
    }
    let w = ask.weights.unwrap_or_default();
    let (cost_before, cost_after) = (
        total(board, routes, &changed, &w),
        total(board, &state, &changed, &w),
    );
    let fewer = after.lines.len() < before.lines.len();
    let kept = fewer || cost_after <= cost_before + 1e-9;
    out.total = Some(Total {
        before: (cost_before * 1000.0).round() / 1000.0,
        after: (cost_after * 1000.0).round() / 1000.0,
        kept,
    });
    if !kept {
        return Ok(out);
    }
    let record = |r: &Routes, net: &str| {
        r.boards
            .get(&board.name)
            .and_then(|t| t.nets.get(net))
            .cloned()
    };
    let edits: Vec<Edit> = changed
        .iter()
        .filter(|n| record(routes, n) != record(&state, n))
        .map(|n| Edit::Restore {
            net: n.clone(),
            before: record(routes, n),
            after: record(&state, n),
        })
        .collect();
    if edits.is_empty() {
        return Ok(out);
    }
    let mut checked = routes.clone();
    match edit_all(board, &mut checked, &edits) {
        Ok(_) => out.routes = Some(checked),
        Err(Refusal::Refused(e)) => out.refused = Some(e),
        Err(Refusal::Failed(e)) => return Err(e),
    }
    Ok(out)
}

/// `routes` with the box's unlocked copper cut away (the module's header), the ends the cut
/// left, and for each net that lost copper the tracks cut, the tracks removed and the vias
/// removed.
#[allow(clippy::type_complexity)]
fn cut(
    board: &Board,
    routes: &Routes,
    region: &Region,
) -> (Routes, Vec<CutEnd>, BTreeMap<String, (usize, usize, usize)>) {
    let mut out = routes.clone();
    let mut ends = Vec::new();
    let mut counts: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    let Some(table) = out.boards.get_mut(&board.name) else {
        return (out, ends, counts);
    };
    for (net, record) in table.nets.iter_mut() {
        if record.lock || Some(&record.pads) != netsmith_pcb::pad_hash(board, net).as_ref() {
            continue;
        }
        let mut tracks = Vec::new();
        let (mut c, mut r, mut v) = (0, 0, 0);
        for t in &record.tracks {
            if t.lock {
                tracks.push(t.clone());
                continue;
            }
            let pieces = pieces(t, region);
            if pieces.iter().all(|p| !p.inside) {
                tracks.push(t.clone());
                continue;
            }
            let outside: Vec<&Piece> = pieces.iter().filter(|p| !p.inside).collect();
            if outside.is_empty() {
                r += 1;
            } else {
                c += 1;
            }
            for p in pieces.iter().filter(|p| p.inside) {
                for end in [p.track.from, p.track.to] {
                    if !region.holds(end) {
                        ends.push(CutEnd {
                            net: net.clone(),
                            layer: t.layer,
                            width: t.width,
                            at: end,
                        });
                    }
                }
            }
            tracks.extend(outside.into_iter().map(|p| p.track.clone()));
        }
        let vias: Vec<RouteVia> = record
            .vias
            .iter()
            .filter(|via| {
                let gone = !via.lock && region.holds(via.at);
                v += usize::from(gone);
                !gone
            })
            .cloned()
            .collect();
        if c + r + v > 0 {
            record.tracks = tracks;
            record.vias = vias;
            counts.insert(net.clone(), (c, r, v));
        }
    }
    table.nets.retain(|_, r| r.lock || r.shapes() > 0);
    if table.nets.is_empty() {
        out.boards.remove(&board.name);
    }
    (out, ends, counts)
}

/// A piece of a track between two crossings of the box's edge.
struct Piece {
    track: RouteTrack,
    inside: bool,
}

/// The pieces of `t` between the places its centre line crosses the box's edge, each with
/// whether it is inside; a piece outside shorter than [`SLIVER`] is left out.
fn pieces(t: &RouteTrack, region: &Region) -> Vec<Piece> {
    let curve = Curve::of(t);
    let mut s: Vec<f64> = match &curve {
        Curve::Line(a, b) => segment_cuts(*a, *b, region),
        Curve::Arc { .. } => arc_cuts(&curve, region),
    };
    s.extend([0.0, 1.0]);
    s.sort_by(f64::total_cmp);
    s.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    let mut out: Vec<Piece> = Vec::new();
    for w in s.windows(2) {
        let (a, b) = (w[0], w[1]);
        let inside = region.holds(curve.at((a + b) / 2.0));
        let length = curve.length() * (b - a);
        if !inside && length < SLIVER {
            continue;
        }
        let p = |s: f64| {
            if s == 0.0 {
                t.from
            } else if s == 1.0 {
                t.to
            } else {
                let q = curve.at(s);
                [um(q[0]), um(q[1])]
            }
        };
        let mid = match curve {
            Curve::Arc { .. } => Some({
                let q = curve.at((a + b) / 2.0);
                [um(q[0]), um(q[1])]
            }),
            Curve::Line(..) => None,
        };
        match out.last_mut() {
            Some(last) if last.inside == inside && mid.is_none() => last.track.to = p(b),
            _ => out.push(Piece {
                track: RouteTrack {
                    from: p(a),
                    to: p(b),
                    mid,
                    ..t.clone()
                },
                inside,
            }),
        }
    }
    out
}

/// A track's centre line, from its start (0) to its end (1).
enum Curve {
    Line([f64; 2], [f64; 2]),
    Arc {
        centre: [f64; 2],
        radius: f64,
        start: f64,
        sweep: f64,
    },
}

impl Curve {
    fn of(t: &RouteTrack) -> Curve {
        let line = Curve::Line(t.from, t.to);
        let Some(m) = t.mid else {
            return line;
        };
        let (a, b) = (t.from, t.to);
        let d = 2.0 * (a[0] * (m[1] - b[1]) + m[0] * (b[1] - a[1]) + b[0] * (a[1] - m[1]));
        if d.abs() < 1e-12 {
            return line;
        }
        let sq = |p: [f64; 2]| p[0] * p[0] + p[1] * p[1];
        let cx = (sq(a) * (m[1] - b[1]) + sq(m) * (b[1] - a[1]) + sq(b) * (a[1] - m[1])) / d;
        let cy = (sq(a) * (b[0] - m[0]) + sq(m) * (a[0] - b[0]) + sq(b) * (m[0] - a[0])) / d;
        let angle = |p: [f64; 2]| (p[1] - cy).atan2(p[0] - cx);
        let tau = std::f64::consts::TAU;
        let (ta, tm, tb) = (angle(a), angle(m), angle(b));
        let (dm, db) = ((tm - ta).rem_euclid(tau), (tb - ta).rem_euclid(tau));
        let sweep = if dm < db { db } else { db - tau };
        Curve::Arc {
            centre: [cx, cy],
            radius: ((a[0] - cx).powi(2) + (a[1] - cy).powi(2)).sqrt(),
            start: ta,
            sweep,
        }
    }

    fn at(&self, s: f64) -> [f64; 2] {
        match *self {
            Curve::Line(a, b) => [a[0] + s * (b[0] - a[0]), a[1] + s * (b[1] - a[1])],
            Curve::Arc {
                centre,
                radius,
                start,
                sweep,
            } => {
                let t = start + s * sweep;
                [centre[0] + radius * t.cos(), centre[1] + radius * t.sin()]
            }
        }
    }

    fn length(&self) -> f64 {
        match *self {
            Curve::Line(a, b) => ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt(),
            Curve::Arc { radius, sweep, .. } => radius * sweep.abs(),
        }
    }
}

/// Where, from 0 at `a` to 1 at `b`, the line crosses the lines of the box's four sides.
fn segment_cuts(a: [f64; 2], b: [f64; 2], region: &Region) -> Vec<f64> {
    let mut s = Vec::new();
    for k in 0..2 {
        let d = b[k] - a[k];
        if d.abs() < 1e-12 {
            continue;
        }
        for edge in [region.min[k], region.max[k]] {
            let t = (edge - a[k]) / d;
            if t > 0.0 && t < 1.0 {
                s.push(t);
            }
        }
    }
    s
}

/// Where, from 0 at its start to 1 at its end, an arc crosses the lines of the box's sides.
fn arc_cuts(curve: &Curve, region: &Region) -> Vec<f64> {
    let Curve::Arc {
        centre,
        radius,
        start,
        sweep,
    } = *curve
    else {
        return Vec::new();
    };
    let tau = std::f64::consts::TAU;
    let along = |t: f64| {
        if sweep > 0.0 {
            (t - start).rem_euclid(tau) / sweep
        } else {
            (start - t).rem_euclid(tau) / -sweep
        }
    };
    let mut s = Vec::new();
    for k in 0..2 {
        for edge in [region.min[k], region.max[k]] {
            let q = (edge - centre[k]) / radius;
            if q.abs() > 1.0 {
                continue;
            }
            let angles = if k == 0 {
                let t = q.acos();
                [t, -t]
            } else {
                let t = q.asin();
                [t, std::f64::consts::PI - t]
            };
            s.extend(
                angles
                    .into_iter()
                    .map(along)
                    .filter(|&x| x > 0.0 && x < 1.0),
            );
        }
    }
    s
}

/// The board `cut` (the routes file's copper on it, less the box's) with a pad of one point at
/// each of `ends` on an outer layer, added to its net (the module's header), and how many were
/// added. A pad is a part of its own with one round surface-mount pad the track's width across,
/// on the track's face, with no mask opening and no courtyard to speak of.
fn terminals(board: &Board, cut: &Board, ends: &[&CutEnd]) -> (Board, usize) {
    let mut guided = cut.clone();
    let Some(template) = board.parts.first() else {
        return (guided, 0);
    };
    let mut seen: Vec<(String, CopperLayer, [i64; 2])> = Vec::new();
    let mut given = 0;
    for end in ends {
        let side = match end.layer {
            CopperLayer::Front => netsmith_pcb::Side::Front,
            CopperLayer::Back => netsmith_pcb::Side::Back,
            _ => continue,
        };
        let key = (
            end.net.clone(),
            end.layer,
            [
                (end.at[0] * 1e6).round() as i64,
                (end.at[1] * 1e6).round() as i64,
            ],
        );
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let Some(net) = guided.nets.iter_mut().find(|n| n.name == end.net) else {
            continue;
        };
        given += 1;
        let refdes = format!("~cut{given}");
        let half = end.width / 2.0;
        let r = Rect::new(Pt::new(-half, -half), Pt::new(half, half));
        let mut part = template.clone();
        let fp = &mut part.footprint;
        fp.name = "region-cut".to_string();
        fp.pads = vec![netsmith_pcb::Pad {
            number: "1".to_string(),
            kind: netsmith_pcb::PadKind::Smd,
            shape: netsmith_pcb::PadShape::Circle,
            at: Pt::new(0.0, 0.0),
            angle: 0.0,
            width: end.width,
            height: end.width,
            drill: None,
            layers: vec!["F.Cu".to_string()],
            corner_ratio: 0.0,
            extent: None,
            clearance: None,
            mask_margin: None,
        }];
        fp.graphics.clear();
        fp.courtyard = r;
        fp.courtyard_shapes = vec![r];
        fp.body = r;
        part.refdes = refdes.clone();
        part.path = refdes.clone();
        part.kind = String::new();
        part.provides.clear();
        part.declared_kind = None;
        part.value = String::new();
        part.library_id = String::new();
        part.power_pads.clear();
        part.at = board.outline.min + Pt::new(end.at[0], end.at[1]);
        part.rotation = 0.0;
        part.side = side;
        part.designator = None;
        part.layout.clear();
        part.apart.clear();
        part.contacts.clear();
        part.routing_margin = 0.0;
        part.soft_margin = 0.0;
        part.symbol = None;
        net.pads.push(PadRef {
            refdes: refdes.clone(),
            number: "1".to_string(),
        });
        guided.parts.push(part);
    }
    (guided, given)
}

/// The corners where the router's copper of `nets` (`laid`) meets a piece the cut left at one of
/// `ends`, on its layer, cut back and turned in steps of 45 degrees at most (the module's
/// header). A join with more than one track of either kind, or with a via, is left as it is.
fn smooth(
    routes: &mut Routes,
    board: &Board,
    nets: &BTreeSet<String>,
    ends: &[CutEnd],
    laid: &BTreeMap<String, (Vec<RouteTrack>, Vec<RouteVia>)>,
) {
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6;
    let Some(table) = routes.boards.get_mut(&board.name) else {
        return;
    };
    for end in ends.iter().filter(|e| nets.contains(&e.net)) {
        let (Some(record), Some((new, new_vias))) =
            (table.nets.get_mut(&end.net), laid.get(&end.net))
        else {
            continue;
        };
        if new_vias.iter().any(|v| near(v.at, end.at))
            || record.vias.iter().any(|v| near(v.at, end.at))
        {
            continue;
        }
        let at: Vec<usize> = record
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.layer == end.layer && (near(t.from, end.at) || near(t.to, end.at)))
            .map(|(k, _)| k)
            .collect();
        let (fresh, old): (Vec<usize>, Vec<usize>) = at
            .into_iter()
            .partition(|&k| new.contains(&record.tracks[k]));
        let ([n], [o]) = (fresh.as_slice(), old.as_slice()) else {
            continue;
        };
        let (n, o) = (*n, *o);
        if record.tracks[n].mid.is_some() || record.tracks[o].mid.is_some() {
            continue;
        }
        // The far ends, the stub's `a` and the new track's `b`, and the corner `p`.
        let far = |t: &RouteTrack| if near(t.from, end.at) { t.to } else { t.from };
        let (a, b, p) = (far(&record.tracks[o]), far(&record.tracks[n]), end.at);
        let unit = |q: [f64; 2], r: [f64; 2]| {
            let (dx, dy) = (r[0] - q[0], r[1] - q[1]);
            let l = (dx * dx + dy * dy).sqrt();
            ([dx / l, dy / l], l)
        };
        let ((u, la), (v, lb)) = (unit(a, p), unit(p, b));
        let turn = (u[0] * v[0] + u[1] * v[1]).clamp(-1.0, 1.0).acos();
        let step = std::f64::consts::FRAC_PI_4;
        if turn <= step + 1e-6
            || turn > std::f64::consts::PI - 1e-3
            || !la.is_finite()
            || !lb.is_finite()
        {
            continue;
        }
        // Chords of a circle tangent to both tracks, each turning by turn / k <= 45 degrees,
        // the first and last by half that.
        let c = 0.4 * la.min(lb);
        let (s, e) = (
            [p[0] - u[0] * c, p[1] - u[1] * c],
            [p[0] + v[0] * c, p[1] + v[1] * c],
        );
        let k = (turn / step).ceil().max(1.0) as usize;
        let start = u[1].atan2(u[0]);
        let sign = if u[0] * v[1] - u[1] * v[0] >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let r = c / (turn / 2.0).tan();
        let centre = [s[0] - sign * u[1] * r, s[1] + sign * u[0] * r];
        let point = |j: usize| -> [f64; 2] {
            if j == 0 {
                return [um(s[0]), um(s[1])];
            }
            if j == k {
                return [um(e[0]), um(e[1])];
            }
            let t = start - sign * std::f64::consts::FRAC_PI_2 + sign * turn * j as f64 / k as f64;
            [um(centre[0] + r * t.cos()), um(centre[1] + r * t.sin())]
        };
        let base = record.tracks[o].clone();
        let mut chords: Vec<RouteTrack> = (0..k)
            .map(|j| RouteTrack {
                from: point(j),
                to: point(j + 1),
                mid: None,
                lock: false,
                ..base.clone()
            })
            .collect();
        let (s, e) = (point(0), point(k));
        let t = &mut record.tracks[o];
        if near(t.from, p) {
            t.from = s;
        } else {
            t.to = s;
        }
        let t = &mut record.tracks[n];
        if near(t.from, p) {
            t.from = e;
        } else {
            t.to = e;
        }
        record.tracks.append(&mut chords);
    }
}

/// Remove the dead copper of `nets` in `routes` (the module's header): from each cut end of
/// `ends`, the unlocked tracks and vias that run into nothing of their net, one at a time, back
/// to where the copper joins something; a removal that opens a connection is put back. Each
/// check of the board is counted in `checks`. The tracks and vias removed, by net.
fn prune(
    board: &Board,
    routes: &mut Routes,
    nets: &BTreeSet<String>,
    ends: &[CutEnd],
    checks: &mut usize,
) -> Result<BTreeMap<String, (usize, usize)>, String> {
    let mut removed: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let open_count = |r: &Routes| -> Result<usize, String> {
        Ok(Ratsnest::of(&with_routes(board, r)?.0).lines.len())
    };
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6;
    for net in nets {
        let budget = routes
            .boards
            .get(&board.name)
            .and_then(|t| t.nets.get(net))
            .map_or(0, NetRoutes::shapes);
        let mut frontier: Vec<[f64; 2]> = ends
            .iter()
            .filter(|e| &e.net == net)
            .map(|e| e.at)
            .collect();
        let mut spent = 0;
        while let Some(at) = frontier.pop() {
            if spent >= budget {
                break;
            }
            let Some(record) = routes.boards.get(&board.name).and_then(|t| t.nets.get(net)) else {
                break;
            };
            let (real, _) = with_routes(board, routes)?;
            let track_index = |t: &RouteTrack| {
                let on = t.on(board, net);
                real.tracks.iter().position(|x| *x == on)
            };
            let via_index = |v: &RouteVia| {
                let on = v.on(board, net);
                real.vias.iter().position(|x| *x == on)
            };
            let mut dead: Option<(bool, usize)> = None;
            for (k, t) in record.tracks.iter().enumerate() {
                if t.lock || !(near(t.from, at) || near(t.to, at)) {
                    continue;
                }
                let Some(i) = track_index(t) else { continue };
                let open = netsmith_pcb::open_ends(&real, net, &[i], &[]);
                if open.iter().any(|e| {
                    let q = e.at - board.outline.min;
                    near([q.x, q.y], at)
                }) {
                    dead = Some((true, k));
                    break;
                }
            }
            if dead.is_none() {
                for (k, v) in record.vias.iter().enumerate() {
                    if v.lock || !near(v.at, at) {
                        continue;
                    }
                    let Some(i) = via_index(v) else { continue };
                    if !netsmith_pcb::open_ends(&real, net, &[], &[i]).is_empty() {
                        dead = Some((false, k));
                        break;
                    }
                }
            }
            let Some((is_track, k)) = dead else { continue };
            spent += 1;
            *checks += 1;
            let before = open_count(routes)?;
            let mut trial = routes.clone();
            let r = trial
                .boards
                .get_mut(&board.name)
                .and_then(|t| t.nets.get_mut(net))
                .expect("the record is there");
            let next = if is_track {
                let t = r.tracks.remove(k);
                if near(t.from, at) { t.to } else { t.from }
            } else {
                r.vias.remove(k);
                at
            };
            if r.shapes() == 0
                && !r.lock
                && let Some(t) = trial.boards.get_mut(&board.name)
            {
                t.nets.remove(net);
            }
            *checks += 1;
            if open_count(&trial)? > before {
                continue;
            }
            *routes = trial;
            let e = removed.entry(net.clone()).or_default();
            if is_track {
                e.0 += 1;
            } else {
                e.1 += 1;
            }
            frontier.push(at);
            frontier.push(next);
        }
    }
    Ok(removed)
}

impl RegionRouted {
    /// The text for a person.
    pub fn text(&self) -> String {
        self.text_moved([0.0, 0.0])
    }

    /// [`RegionRouted::text`] with its places moved by `by`, mm (into another frame than the
    /// routes file's).
    pub fn text_moved(&self, by: [f64; 2]) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let p = |q: [f64; 2]| format!("({}, {})", um(q[0] + by[0]) + 0.0, um(q[1] + by[1]) + 0.0);
        let region = format!("region {} to {}", p(self.region.min), p(self.region.max));
        if self.nets.is_empty() && self.refused.is_none() {
            let _ = writeln!(
                s,
                "{region}: no unlocked copper inside and no open connection through it: nothing to route"
            );
            return s;
        }
        if self.nets.is_empty() {
            return s;
        }
        let (c, r, v) = self.nets.iter().fold((0, 0, 0), |(c, r, v), n| {
            (c + n.cut_tracks, r + n.removed_tracks, v + n.removed_vias)
        });
        let _ = writeln!(
            s,
            "{region}: {c} track(s) cut at its edge, {r} track(s) and {v} via(s) inside removed; {} connection(s) opened, {} open before run through it, {} cut end(s) given to the router ({} order(s) tried)",
            self.opened.len(),
            self.through.len(),
            self.cut_ends,
            self.tries
        );
        for n in &self.nets {
            let mut parts = Vec::new();
            if n.cut_tracks + n.removed_tracks + n.removed_vias > 0 {
                parts.push(format!(
                    "{} cut, {} track(s) and {} via(s) removed",
                    n.cut_tracks, n.removed_tracks, n.removed_vias
                ));
            }
            if !n.tracks.is_empty() || !n.vias.is_empty() {
                parts.push(format!(
                    "routed with {} track(s) and {} via(s)",
                    n.tracks.len(),
                    n.vias.len()
                ));
            }
            if n.pruned_tracks + n.pruned_vias > 0 {
                parts.push(format!(
                    "{} dead track(s) and {} via(s) taken off",
                    n.pruned_tracks, n.pruned_vias
                ));
            }
            let _ = writeln!(s, "net {}: {}", n.net, parts.join("; "));
        }
        for l in &self.left {
            let _ = writeln!(s, "{l}: still open");
        }
        if let Some(t) = &self.total {
            let _ = writeln!(
                s,
                "laid again, the copper of these nets costs {} mm of track by the weights where it cost {} mm: {}",
                t.after,
                t.before,
                if t.kept {
                    "kept".to_string()
                } else {
                    "it costs more, so nothing is written".to_string()
                }
            );
        }
        let _ = writeln!(s, "{} check(s) of dead copper", self.checks);
        s
    }
}
