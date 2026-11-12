//! The route scorecard: numbers that say how good a board's routing is, read from the board
//! file (`.kicad_pcb`, with the `.kicad_pro` beside it), so that a board netsmith routed and a
//! board a person routed in KiCad are measured by the same code (issue #431, step S0).
//!
//! Nothing here is a check: a board is not refused for its score. The numbers are what the
//! router's changes are judged by, before and after, on the example boards and their
//! references (`scripts/route-scorecard`).
//!
//! What is measured, and how:
//!
//! - **Connections left and errors**: netsmith's own design rules check ([`crate::drc`]) on the
//!   file, its `unconnected_items` findings and its errors.
//! - **Track length**, in all and on each copper layer, and the length of the **routed nets**:
//!   the nets that no zone pours, the ones whose pads tracks must join. Their **spanning
//!   length** is the shortest tree of straight lines between the centres of each one's pads,
//!   a lower bound that no routing beats; the ratio of the two is how much longer than the
//!   least the routing is.
//! - **Vias**.
//! - **Bends**: where exactly two straight tracks of one net and layer meet, outside every pad,
//!   the angle between them: straight (no bend), 135 degrees (a 45 degree bend), 90 degrees, acute,
//!   or another angle. Where three or more meet (a tee), an acute angle between any two is
//!   counted as acute. **Off-angle** tracks run at an angle that is not a multiple of 45 degrees.
//! - **Narrow copper**: track of a net drawn narrower than the width the project file's net
//!   class gives the net (the width its current needs, ADR 0036), by net, in mm of track.
//! - **Pours**: for each zone on an outer layer, the length of other nets' track inside it, and
//!   the pieces its copper falls into: a raster of 0.1 mm cells inside the zone and the board,
//!   less the zone's clearance round the copper of other nets and round holes, joined side to
//!   side, counting pieces of at least [`PIECE_LEAST`]. Thermal reliefs are not drawn, so a pad
//!   of the net joins the piece round it.
//! - **Decoupling**: for each capacitor of two pads with one pad on a poured net and the other on
//!   a net that a part of at least [`IC_PADS`] pads is on, the length of track from the
//!   capacitor's pad to the nearest pad of that part on the net, along the copper (tracks joined
//!   end to end, through vias and pads). `None` when no track joins them.
//! - **Diff pairs**: nets whose names differ only in a suffix pair ([`PAIR_SUFFIXES`]), the
//!   track length of each, the difference (skew), and the share of the first net's track that
//!   has the second's within [`COUPLED`] on the same layer.

use std::collections::{BTreeMap, BinaryHeap, HashMap};
use std::fmt::Write as _;

use crate::artwork::{Artwork, PadKind, TrackShape};
use crate::drc::{self, Extras, Rule, Rules, shape};
use crate::geom::{self, Pt};
use crate::pads;

/// The side of a cell of the pour raster, in mm.
const CELL: f64 = 0.1;
/// The least area of a pour's piece that is counted, in mm². Smaller ones are slivers.
pub const PIECE_LEAST: f64 = 1.0;
/// The fewest pads of a part that a decoupling capacitor is counted as decoupling.
pub const IC_PADS: usize = 8;
/// How near, centre line to centre line, the other net of a diff pair runs for a stretch of
/// track to count as coupled, in mm.
pub const COUPLED: f64 = 0.5;
/// The suffixes that make two nets a diff pair: the net that ends in the first and the one that
/// ends in the second, with the same name before them.
pub const PAIR_SUFFIXES: [(&str, &str); 6] = [
    ("dp", "dm"),
    ("DP", "DM"),
    ("D+", "D-"),
    ("_P", "_N"),
    ("+", "-"),
    ("_p", "_n"),
];
/// Points closer than this are one point, in mm.
const SAME: f64 = 1e-3;

/// A straight piece of track: net, layer, its ends and its width.
type Seg = (String, String, Pt, Pt, f64);
/// The directions out of each place where tracks meet, by net, layer and place.
type Joints = HashMap<(String, String, (i64, i64)), (Pt, Vec<Pt>)>;

/// How the bends of a board's tracks are turned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Bends {
    pub straight: usize,
    pub deg45: usize,
    pub deg90: usize,
    pub acute: usize,
    pub other: usize,
}

/// A bend that is not 45 degrees, for a person to look at: the net, the layer, where, and the
/// angle between the two tracks (180 is straight on).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Corner {
    pub net: String,
    pub layer: String,
    pub x: f64,
    pub y: f64,
    pub angle: f64,
}

/// One pour's numbers.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PourScore {
    pub layer: String,
    pub net: String,
    /// Track of other nets inside the zone, in mm.
    pub foreign_track: f64,
    /// Pieces of at least [`PIECE_LEAST`].
    pub pieces: usize,
    /// The largest piece's share of the copper of all pieces, 0 to 1.
    pub largest: f64,
}

/// One decoupling capacitor's connection to the part it serves.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Decoupling {
    pub cap: String,
    pub part: String,
    pub net: String,
    /// Track from the capacitor's pad to the part's nearest pad on the net, in mm.
    pub length: Option<f64>,
}

/// One diff pair's numbers.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PairScore {
    pub p: String,
    pub n: String,
    pub length_p: f64,
    pub length_n: f64,
    pub skew: f64,
    /// The share of `p`'s track with `n`'s within [`COUPLED`], 0 to 1.
    pub coupled: f64,
}

/// What a board's routing measures.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Scorecard {
    pub unconnected: usize,
    pub drc_errors: usize,
    pub track_length: f64,
    pub length_by_layer: BTreeMap<String, f64>,
    /// Track of the nets no zone pours.
    pub routed_length: f64,
    /// The shortest tree between the pads of each net no zone pours.
    pub spanning_length: f64,
    pub vias: usize,
    pub bends: Bends,
    /// Where the bends are that are 90 degrees, acute or another angle.
    pub corners: Vec<Corner>,
    pub off_angle: usize,
    /// The track widths drawn, in mm, each once.
    pub widths: Vec<f64>,
    /// mm of track narrower than its net class's width, by net.
    pub narrow: BTreeMap<String, f64>,
    pub pours: Vec<PourScore>,
    pub decoupling: Vec<Decoupling>,
    pub pairs: Vec<PairScore>,
}

impl Scorecard {
    /// The scorecard of a board file, with its project file if there is one.
    pub fn of(pcb: &str, project: Option<&str>) -> Result<Scorecard, String> {
        let art = Artwork::parse(pcb).map_err(|e| e.to_string())?;
        let rules = match project {
            Some(p) => Rules::from_project(p)?,
            None => Rules::default(),
        };
        let report = drc::check(pcb, project, &Extras::default()).map_err(|e| e.to_string())?;
        let unconnected = report
            .findings
            .iter()
            .filter(|f| f.rule == Rule::UnconnectedItems)
            .count();
        let drc_errors = report.errors().count();
        let classes = class_widths(project);
        Ok(score(&art, &rules, &classes, unconnected, drc_errors))
    }

    /// The ratio of the routed nets' track to their spanning length.
    pub fn detour(&self) -> f64 {
        if self.spanning_length > 0.0 {
            self.routed_length / self.spanning_length
        } else {
            0.0
        }
    }

    /// Every number on one line each, for a person.
    pub fn text(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "connections left    {}", self.unconnected);
        let _ = writeln!(s, "drc errors          {}", self.drc_errors);
        let _ = writeln!(s, "track               {:.0} mm", self.track_length);
        for (layer, len) in &self.length_by_layer {
            let _ = writeln!(s, "  on {layer:<16} {len:.0} mm");
        }
        let _ = writeln!(
            s,
            "routed nets         {:.0} mm over a spanning {:.0} mm ({:.2}x)",
            self.routed_length,
            self.spanning_length,
            self.detour()
        );
        let _ = writeln!(s, "vias                {}", self.vias);
        let b = self.bends;
        let _ = writeln!(
            s,
            "bends               45: {}, 90: {}, acute: {}, other: {} (straight joins {})",
            b.deg45, b.deg90, b.acute, b.other, b.straight
        );
        for c in &self.corners {
            let _ = writeln!(
                s,
                "  corner of {:.0} degrees: net {} on {} at ({:.3}, {:.3})",
                c.angle, c.net, c.layer, c.x, c.y
            );
        }
        let _ = writeln!(s, "off-angle tracks    {}", self.off_angle);
        let widths: Vec<String> = self.widths.iter().map(|w| format!("{w}")).collect();
        let _ = writeln!(s, "widths              {}", widths.join(", "));
        let narrow: f64 = self.narrow.values().sum::<f64>() + 0.0;
        let _ = writeln!(
            s,
            "narrow copper       {narrow:.1} mm on {} nets",
            self.narrow.len()
        );
        for p in &self.pours {
            let _ = writeln!(
                s,
                "pour {} on {}: {:.0} mm of other nets' track, {} pieces (largest {:.0}%)",
                p.net,
                p.layer,
                p.foreign_track,
                p.pieces,
                p.largest * 100.0
            );
        }
        let joined: Vec<f64> = self.decoupling.iter().filter_map(|d| d.length).collect();
        let mean = if joined.is_empty() {
            0.0
        } else {
            joined.iter().sum::<f64>() / joined.len() as f64
        };
        let _ = writeln!(
            s,
            "decoupling          {} capacitors, {} joined by track, mean {mean:.1} mm, max {:.1} mm",
            self.decoupling.len(),
            joined.len(),
            joined.iter().copied().fold(0.0, f64::max)
        );
        for p in &self.pairs {
            let _ = writeln!(
                s,
                "pair {}/{}: {:.1} and {:.1} mm, skew {:.2} mm, coupled {:.0}%",
                p.p,
                p.n,
                p.length_p,
                p.length_n,
                p.skew,
                p.coupled * 100.0
            );
        }
        s
    }
}

/// The track width of each net class the project names, as (net patterns, width), with the
/// default class's width under the pattern `*` last.
fn class_widths(project: Option<&str>) -> Vec<(Vec<String>, f64)> {
    let Some(json) = project.and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok()) else {
        return Vec::new();
    };
    let settings = &json["net_settings"];
    let mut out: Vec<(String, Vec<String>, f64)> = Vec::new();
    let mut default = None;
    for c in settings["classes"].as_array().into_iter().flatten() {
        let (Some(name), Some(width)) = (c["name"].as_str(), c["track_width"].as_f64()) else {
            continue;
        };
        if name == "Default" {
            default = Some(width);
        } else {
            out.push((name.to_string(), Vec::new(), width));
        }
    }
    for p in settings["netclass_patterns"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let (Some(class), Some(pattern)) = (p["netclass"].as_str(), p["pattern"].as_str()) else {
            continue;
        };
        if let Some(c) = out.iter_mut().find(|c| c.0 == class) {
            c.1.push(pattern.to_string());
        }
    }
    let mut widths: Vec<(Vec<String>, f64)> = out.into_iter().map(|(_, p, w)| (p, w)).collect();
    if let Some(w) = default {
        widths.push((vec!["*".to_string()], w));
    }
    widths
}

/// The width a net's class gives it, if any: the first class whose pattern matches.
fn class_width(classes: &[(Vec<String>, f64)], net: &str) -> Option<f64> {
    classes
        .iter()
        .find(|(patterns, _)| patterns.iter().any(|p| drc::matches_pattern(p, net)))
        .map(|(_, w)| *w)
}

/// A point as a key, to the micrometre.
fn key(p: Pt) -> (i64, i64) {
    ((p.x / SAME).round() as i64, (p.y / SAME).round() as i64)
}

/// The straight tracks of the board as (net, layer, a, b, width), arcs as their chords' points.
fn lines(art: &Artwork) -> Vec<Seg> {
    let mut out = Vec::new();
    for t in &art.tracks {
        let net = t.net.clone().unwrap_or_default();
        match &t.shape {
            TrackShape::Line(a, b) => out.push((net, t.layer.clone(), *a, *b, t.width)),
            TrackShape::Arc(arc) => {
                for w in arc.points().windows(2) {
                    out.push((net.clone(), t.layer.clone(), w[0], w[1], t.width));
                }
            }
        }
    }
    out
}

fn score(
    art: &Artwork,
    rules: &Rules,
    classes: &[(Vec<String>, f64)],
    unconnected: usize,
    drc_errors: usize,
) -> Scorecard {
    let tracks = lines(art);
    let poured: Vec<&str> = art
        .zones
        .iter()
        .filter(|z| z.keepout.is_none())
        .filter_map(|z| z.net.as_deref())
        .collect();
    let mut length_by_layer = BTreeMap::new();
    let mut track_length = 0.0;
    let mut routed_length = 0.0;
    let mut narrow: BTreeMap<String, f64> = BTreeMap::new();
    let mut widths: Vec<f64> = Vec::new();
    let mut off_angle = 0;
    for (net, layer, a, b, width) in &tracks {
        let len = a.dist(*b);
        track_length += len;
        *length_by_layer.entry(layer.clone()).or_insert(0.0) += len;
        if !poured.contains(&net.as_str()) {
            routed_length += len;
        }
        if let Some(w) = class_width(classes, net)
            && *width < w - 1e-6
        {
            *narrow.entry(net.clone()).or_insert(0.0) += len;
        }
        let w = (width * 1e4).round() / 1e4;
        if !widths.contains(&w) {
            widths.push(w);
        }
        if len > SAME {
            let deg = (b.y - a.y).atan2(b.x - a.x).to_degrees().rem_euclid(45.0);
            if deg.min(45.0 - deg) > 0.5 {
                off_angle += 1;
            }
        }
    }
    widths.sort_by(f64::total_cmp);

    // The pad shapes of each net, for "inside a pad".
    let pad_shapes: Vec<(Option<&str>, &crate::artwork::Pad, Vec<shape::Shape>)> = art
        .pads
        .iter()
        .map(|p| (p.net.as_deref(), p, drc::outline_shapes(&pads::outline(p))))
        .collect();
    // Inside the copper of a pad or a via of the net: a joint there is no corner.
    let in_pad = |p: Pt, net: &str, layer: &str| {
        let dot = shape::Shape::disc(p, 0.0);
        art.vias
            .iter()
            .any(|v| v.net.as_deref() == Some(net) && v.at.dist(p) <= v.diameter / 2.0)
            || pad_shapes.iter().any(|(n, pad, shapes)| {
                *n == Some(net)
                    && pad.layers.iter().any(|l| l == layer || l == "*.Cu")
                    && shapes.iter().any(|s| shape::gap(&dot, s).d == 0.0)
            })
    };

    let (bends, corners) = bends(&tracks, &in_pad);
    let spanning_length = spanning(art, &poured);
    let pours = pours(art, rules, &tracks);
    let decoupling = decoupling(art, &tracks, &poured, &pad_shapes);
    let pairs = pairs(&tracks);
    Scorecard {
        unconnected,
        drc_errors,
        track_length,
        length_by_layer,
        routed_length,
        spanning_length,
        vias: art.vias.len(),
        bends,
        corners,
        off_angle,
        widths,
        narrow,
        pours,
        decoupling,
        pairs,
    }
}

/// The interior angle, in degrees, between two directions out of one point.
fn angle(u: Pt, v: Pt) -> f64 {
    let dot = u.x * v.x + u.y * v.y;
    let n = (u.x.hypot(u.y) * v.x.hypot(v.y)).max(1e-18);
    (dot / n).clamp(-1.0, 1.0).acos().to_degrees()
}

fn bends(tracks: &[Seg], in_pad: &dyn Fn(Pt, &str, &str) -> bool) -> (Bends, Vec<Corner>) {
    // The directions out of each joint, by net and layer.
    let mut joints: Joints = HashMap::new();
    for (net, layer, a, b, _) in tracks {
        if a.dist(*b) <= SAME {
            continue;
        }
        for (at, to) in [(*a, *b), (*b, *a)] {
            joints
                .entry((net.clone(), layer.clone(), key(at)))
                .or_insert((at, Vec::new()))
                .1
                .push(to - at);
        }
    }
    let mut bends = Bends::default();
    let mut corners = Vec::new();
    let mut corner = |net: &str, layer: &str, at: Pt, angle: f64| {
        corners.push(Corner {
            net: net.to_string(),
            layer: layer.to_string(),
            x: at.x,
            y: at.y,
            angle,
        })
    };
    for ((net, layer, _), (at, out)) in &joints {
        // Tracks of the net that run out the same way overlap: one direction, not a bend.
        let unit = |p: &Pt| {
            let n = p.x.hypot(p.y).max(1e-18);
            (
                (p.x / n * 1e4).round() as i64,
                (p.y / n * 1e4).round() as i64,
            )
        };
        let mut out = out.clone();
        out.sort_by_key(unit);
        out.dedup_by_key(|p| unit(p));
        if out.len() < 2 || in_pad(*at, net, layer) {
            continue;
        }
        if out.len() == 2 {
            let a = angle(out[0], out[1]);
            if a >= 179.0 {
                bends.straight += 1;
            } else if (134.0..=136.0).contains(&a) {
                bends.deg45 += 1;
            } else if (89.0..=91.0).contains(&a) {
                bends.deg90 += 1;
                corner(net, layer, *at, a);
            } else if a < 89.0 {
                bends.acute += 1;
                corner(net, layer, *at, a);
            } else {
                bends.other += 1;
                corner(net, layer, *at, a);
            }
        } else {
            let least = (0..out.len())
                .flat_map(|i| (i + 1..out.len()).map(move |j| (i, j)))
                .map(|(i, j)| angle(out[i], out[j]))
                .fold(180.0, f64::min);
            if least < 89.0 {
                bends.acute += 1;
                corner(net, layer, *at, least);
            }
        }
    }
    corners.sort_by(|a, b| {
        (&a.net, &a.layer)
            .cmp(&(&b.net, &b.layer))
            .then(a.x.total_cmp(&b.x))
    });
    (bends, corners)
}

/// The shortest tree of straight lines between the centres of each unpoured net's pads.
fn spanning(art: &Artwork, poured: &[&str]) -> f64 {
    let mut by_net: BTreeMap<&str, Vec<Pt>> = BTreeMap::new();
    for p in &art.pads {
        if let Some(net) = p.net.as_deref()
            && !net.is_empty()
            && !poured.contains(&net)
        {
            let at = by_net.entry(net).or_default();
            if !at.iter().any(|q| q.dist(p.at) <= SAME) {
                at.push(p.at);
            }
        }
    }
    by_net.values().map(|pts| prim(pts)).sum()
}

/// The length of the minimum spanning tree of `pts` (Prim's, O(n²)).
fn prim(pts: &[Pt]) -> f64 {
    if pts.len() < 2 {
        return 0.0;
    }
    let mut best = vec![f64::MAX; pts.len()];
    let mut done = vec![false; pts.len()];
    best[0] = 0.0;
    let mut total = 0.0;
    for _ in 0..pts.len() {
        let (k, d) = best
            .iter()
            .enumerate()
            .filter(|(k, _)| !done[*k])
            .min_by(|a, b| a.1.total_cmp(b.1))
            .map(|(k, d)| (k, *d))
            .expect("a point is left");
        done[k] = true;
        total += d;
        for j in 0..pts.len() {
            if !done[j] {
                best[j] = best[j].min(pts[k].dist(pts[j]));
            }
        }
    }
    total
}

fn pours(art: &Artwork, rules: &Rules, tracks: &[Seg]) -> Vec<PourScore> {
    let outer: Vec<&str> = [art.copper.first(), art.copper.last()]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect();
    let board = crate::edge::outline(art).ok();
    let mut out = Vec::new();
    for zone in art.zones.iter().filter(|z| z.keepout.is_none()) {
        let Some(net) = zone.net.as_deref() else {
            continue;
        };
        for layer in zone.layers.iter().filter(|l| outer.contains(&l.as_str())) {
            let inside = |p: Pt| zone.outlines.iter().any(|o| geom::contains(o, p));
            let foreign_track: f64 = tracks
                .iter()
                .filter(|t| t.1 == *layer && t.0 != net)
                .map(|(_, _, a, b, _)| {
                    let steps = ((a.dist(*b) / CELL).ceil() as usize).max(1);
                    let piece = a.dist(*b) / steps as f64;
                    (0..steps)
                        .filter(|k| inside(*a + (*b - *a) * ((*k as f64 + 0.5) / steps as f64)))
                        .count() as f64
                        * piece
                })
                .sum();
            let (pieces, largest) = pour_pieces(art, rules, zone, net, layer, board.as_ref());
            out.push(PourScore {
                layer: layer.clone(),
                net: net.to_string(),
                foreign_track,
                pieces,
                largest,
            });
        }
    }
    out
}

/// The pieces a pour's copper falls into, and the largest one's share.
fn pour_pieces(
    art: &Artwork,
    rules: &Rules,
    zone: &crate::artwork::Zone,
    net: &str,
    layer: &str,
    board: Option<&crate::edge::Outline>,
) -> (usize, f64) {
    let Some(bounds) = geom::Bounds::of(zone.outlines.iter().flatten().copied()) else {
        return (0, 0.0);
    };
    let nx = ((bounds.max.x - bounds.min.x) / CELL).ceil() as usize + 1;
    let ny = ((bounds.max.y - bounds.min.y) / CELL).ceil() as usize + 1;
    let centre = |i: usize, j: usize| {
        Pt::new(
            bounds.min.x + (i as f64 + 0.5) * CELL,
            bounds.min.y + (j as f64 + 0.5) * CELL,
        )
    };
    let edge = rules.copper_edge.max(zone.clearance);
    let edge_shapes: Vec<shape::Shape> = board
        .map(|b| {
            std::iter::once(&b.outer)
                .chain(&b.cutouts)
                .map(|p| shape::Shape::ring(p.clone(), 0.0))
                .collect()
        })
        .unwrap_or_default();
    let mut open = vec![false; nx * ny];
    for j in 0..ny {
        for i in 0..nx {
            let p = centre(i, j);
            let in_board = board.is_none_or(|b| {
                geom::contains(&b.outer, p) && !b.cutouts.iter().any(|c| geom::contains(c, p))
            });
            open[j * nx + i] = in_board
                && zone.outlines.iter().any(|o| geom::contains(o, p))
                && edge_shapes
                    .iter()
                    .all(|s| shape::gap(&shape::Shape::disc(p, 0.0), s).d >= edge);
        }
    }
    // The copper of other nets on the layer, and holes, each with the zone's clearance.
    let clearance = zone.clearance.max(rules.clearance);
    let mut blocks: Vec<shape::Shape> = Vec::new();
    for pad in &art.pads {
        let on = art.pad_copper_on(pad, layer);
        if on && pad.net.as_deref() != Some(net) {
            blocks.extend(drc::outline_shapes(&pads::outline(pad)));
        }
        if let Some(d) = &pad.drill
            && (pad.kind == PadKind::Npth || pad.net.as_deref() != Some(net))
        {
            blocks.push(shape::Shape::disc(pad.at, d.width.max(d.length) / 2.0));
        }
    }
    for v in art.vias.iter().filter(|v| v.net.as_deref() != Some(net)) {
        blocks.push(shape::Shape::disc(v.at, v.diameter / 2.0));
    }
    for t in &art.tracks {
        if t.layer != layer || t.net.as_deref() == Some(net) {
            continue;
        }
        match &t.shape {
            TrackShape::Line(a, b) => blocks.push(shape::Shape::capsule(*a, *b, t.width / 2.0)),
            TrackShape::Arc(arc) => blocks.push(shape::Shape::stroke(arc.points(), t.width / 2.0)),
        }
    }
    for s in &blocks {
        let b = s.bounds.expanded(clearance + CELL);
        let i0 = (((b.min.x - bounds.min.x) / CELL).floor().max(0.0)) as usize;
        let j0 = (((b.min.y - bounds.min.y) / CELL).floor().max(0.0)) as usize;
        let i1 = ((((b.max.x - bounds.min.x) / CELL).ceil()) as usize).min(nx - 1);
        let j1 = ((((b.max.y - bounds.min.y) / CELL).ceil()) as usize).min(ny - 1);
        for j in j0..=j1 {
            for i in i0..=i1 {
                let k = j * nx + i;
                if open[k] && shape::gap(&shape::Shape::disc(centre(i, j), 0.0), s).d < clearance {
                    open[k] = false;
                }
            }
        }
    }
    // Pieces, side to side.
    let least = (PIECE_LEAST / (CELL * CELL)).round() as usize;
    let mut seen = vec![false; nx * ny];
    let mut sizes = Vec::new();
    for start in 0..nx * ny {
        if !open[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let mut size = 0;
        while let Some(k) = stack.pop() {
            size += 1;
            let (i, j) = (k % nx, k / nx);
            let mut push = |n: usize| {
                if open[n] && !seen[n] {
                    seen[n] = true;
                    stack.push(n);
                }
            };
            if i > 0 {
                push(k - 1);
            }
            if i + 1 < nx {
                push(k + 1);
            }
            if j > 0 {
                push(k - nx);
            }
            if j + 1 < ny {
                push(k + nx);
            }
        }
        if size >= least {
            sizes.push(size);
        }
    }
    let total: usize = sizes.iter().sum();
    let largest = sizes.iter().copied().max().unwrap_or(0);
    (
        sizes.len(),
        if total > 0 {
            largest as f64 / total as f64
        } else {
            0.0
        },
    )
}

/// The part a pad belongs to, by name.
fn part_pads(art: &Artwork) -> BTreeMap<&str, usize> {
    let mut count = BTreeMap::new();
    for p in &art.pads {
        *count.entry(p.part.as_str()).or_insert(0) += 1;
    }
    count
}

fn decoupling(
    art: &Artwork,
    tracks: &[Seg],
    poured: &[&str],
    pad_shapes: &[(Option<&str>, &crate::artwork::Pad, Vec<shape::Shape>)],
) -> Vec<Decoupling> {
    let counts = part_pads(art);
    let is_cap = |part: &str| {
        part.strip_prefix('C')
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
    };
    let mut out = Vec::new();
    for (&part, &n) in &counts {
        if n != 2 || !is_cap(part) {
            continue;
        }
        let pads: Vec<&crate::artwork::Pad> = art.pads.iter().filter(|p| p.part == part).collect();
        let nets: Vec<Option<&str>> = pads.iter().map(|p| p.net.as_deref()).collect();
        let (Some(a), Some(b)) = (nets[0], nets[1]) else {
            continue;
        };
        let (supply_pad, net) = match (poured.contains(&a), poured.contains(&b)) {
            (false, true) => (pads[0], a),
            (true, false) => (pads[1], b),
            _ => continue,
        };
        // The part with the most pads on the supply net, of at least IC_PADS.
        let Some(ic) = art
            .pads
            .iter()
            .filter(|p| p.net.as_deref() == Some(net) && p.part != part)
            .map(|p| p.part.as_str())
            .filter(|p| counts.get(p).copied().unwrap_or(0) >= IC_PADS)
            .max_by_key(|p| (counts[p], std::cmp::Reverse(*p)))
        else {
            continue;
        };
        let targets: Vec<usize> = pad_shapes
            .iter()
            .enumerate()
            .filter(|(_, (n, p, _))| *n == Some(net) && p.part == ic)
            .map(|(k, _)| k)
            .collect();
        let from = pad_shapes
            .iter()
            .position(|(_, p, _)| std::ptr::eq(*p, supply_pad))
            .expect("the pad is on the board");
        out.push(Decoupling {
            cap: part.to_string(),
            part: ic.to_string(),
            net: net.to_string(),
            length: copper_path(art, tracks, pad_shapes, net, from, &targets),
        });
    }
    out
}

/// The length of track from pad `from` to the nearest of `to` along the copper of `net`.
fn copper_path(
    art: &Artwork,
    tracks: &[Seg],
    pad_shapes: &[(Option<&str>, &crate::artwork::Pad, Vec<shape::Shape>)],
    net: &str,
    from: usize,
    to: &[usize],
) -> Option<f64> {
    // Nodes: the net's pads, its vias, then the ends of its tracks.
    let pads: Vec<usize> = (0..pad_shapes.len())
        .filter(|&k| pad_shapes[k].0 == Some(net))
        .collect();
    let vias: Vec<Pt> = art
        .vias
        .iter()
        .filter(|v| v.net.as_deref() == Some(net))
        .map(|v| v.at)
        .collect();
    let own: Vec<&Seg> = tracks.iter().filter(|t| t.0 == net).collect();
    let mut ends: HashMap<(i64, i64), usize> = HashMap::new();
    let mut points: Vec<Pt> = Vec::new();
    let base = pads.len() + vias.len();
    let mut node = |p: Pt| -> usize {
        *ends.entry(key(p)).or_insert_with(|| {
            points.push(p);
            base + points.len() - 1
        })
    };
    let segs: Vec<(usize, usize, f64)> = own
        .iter()
        .map(|(_, _, a, b, _)| (node(*a), node(*b), a.dist(*b)))
        .collect();
    let count = base + points.len();
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); count];
    let mut join = |a: usize, b: usize, w: f64| {
        adj[a].push((b, w));
        adj[b].push((a, w));
    };
    for &(a, b, w) in &segs {
        join(a, b, w);
    }
    for (k, p) in points.iter().enumerate() {
        let dot = shape::Shape::disc(*p, 0.0);
        for (i, &pad) in pads.iter().enumerate() {
            if pad_shapes[pad]
                .2
                .iter()
                .any(|s| shape::gap(&dot, s).d == 0.0)
            {
                join(base + k, i, 0.0);
            }
        }
        for (i, v) in vias.iter().enumerate() {
            let radius = art
                .vias
                .iter()
                .find(|x| x.at.dist(*v) <= SAME)
                .map_or(0.0, |x| x.diameter / 2.0);
            if p.dist(*v) <= radius + SAME {
                join(base + k, pads.len() + i, 0.0);
            }
        }
    }
    let start = pads.iter().position(|&p| p == from)?;
    let goal: Vec<usize> = to
        .iter()
        .filter_map(|t| pads.iter().position(|p| p == t))
        .collect();
    // Dijkstra, in micrometres so the queue orders whole numbers.
    let mut dist = vec![u64::MAX; count];
    let mut queue = BinaryHeap::new();
    dist[start] = 0;
    queue.push(std::cmp::Reverse((0u64, start)));
    while let Some(std::cmp::Reverse((d, n))) = queue.pop() {
        if d > dist[n] {
            continue;
        }
        if goal.contains(&n) {
            return Some(d as f64 / 1000.0);
        }
        for &(m, w) in &adj[n] {
            let nd = d + (w * 1000.0).round() as u64;
            if nd < dist[m] {
                dist[m] = nd;
                queue.push(std::cmp::Reverse((nd, m)));
            }
        }
    }
    None
}

fn pairs(tracks: &[Seg]) -> Vec<PairScore> {
    let mut nets: Vec<&str> = tracks.iter().map(|t| t.0.as_str()).collect();
    nets.sort_unstable();
    nets.dedup();
    let length = |net: &str| -> f64 {
        tracks
            .iter()
            .filter(|t| t.0 == net)
            .map(|t| t.2.dist(t.3))
            .sum()
    };
    let mut out = Vec::new();
    for &p in &nets {
        for (sp, sn) in PAIR_SUFFIXES {
            let Some(stem) = p.strip_suffix(sp) else {
                continue;
            };
            let n = format!("{stem}{sn}");
            if !nets.contains(&n.as_str()) {
                continue;
            }
            let (length_p, length_n) = (length(p), length(&n));
            // Sample p's track and look for n's on the same layer.
            let (mut near, mut all) = (0usize, 0usize);
            for (_, layer, a, b, _) in tracks.iter().filter(|t| t.0 == p) {
                let steps = ((a.dist(*b) / CELL).ceil() as usize).max(1);
                for k in 0..steps {
                    let at = *a + (*b - *a) * ((k as f64 + 0.5) / steps as f64);
                    all += 1;
                    let dot = shape::Shape::disc(at, 0.0);
                    if tracks.iter().any(|t| {
                        t.0 == n
                            && t.1 == *layer
                            && shape::gap(&dot, &shape::Shape::capsule(t.2, t.3, 0.0)).d <= COUPLED
                    }) {
                        near += 1;
                    }
                }
            }
            out.push(PairScore {
                p: p.to_string(),
                n,
                length_p,
                length_n,
                skew: (length_p - length_n).abs(),
                coupled: if all > 0 {
                    near as f64 / all as f64
                } else {
                    0.0
                },
            });
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A board of 20 x 10 mm with two pads per net and the tracks given, as KiCad text.
    fn board(body: &str) -> String {
        format!(
            r#"(kicad_pcb (version 20240108) (generator "test")
  (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (44 "Edge.Cuts" user))
  (net 0 "") (net 1 "A") (net 2 "GND")
  (gr_rect (start 0 0) (end 20 10) (layer "Edge.Cuts") (stroke (width 0.05)))
  (footprint "R" (layer "F.Cu") (at 2 2)
    (fp_text reference "R1" (at 0 0) (layer "F.SilkS"))
    (pad "1" smd rect (at 0 0) (size 0.6 0.6) (layers "F.Cu") (net 1 "A")))
  (footprint "R" (layer "F.Cu") (at 12 6)
    (fp_text reference "R2" (at 0 0) (layer "F.SilkS"))
    (pad "1" smd rect (at 0 0) (size 0.6 0.6) (layers "F.Cu") (net 1 "A")))
  {body}
)"#
        )
    }

    fn seg(a: (f64, f64), b: (f64, f64), w: f64) -> String {
        format!(
            "(segment (start {} {}) (end {} {}) (width {w}) (layer \"F.Cu\") (net 1))\n",
            a.0, a.1, b.0, b.1
        )
    }

    fn card(text: &str) -> Scorecard {
        Scorecard::of(text, None).unwrap()
    }

    #[test]
    fn a_right_angle_bend_is_counted_as_90_degrees() {
        let s = card(&board(
            &(seg((2.0, 2.0), (12.0, 2.0), 0.2) + &seg((12.0, 2.0), (12.0, 6.0), 0.2)),
        ));
        assert_eq!(s.bends.deg90, 1);
        assert_eq!(s.bends.deg45, 0);
        assert_eq!(s.off_angle, 0);
        assert_eq!(s.unconnected, 0);
        assert!((s.track_length - 14.0).abs() < 1e-9);
    }

    #[test]
    fn two_45_degree_bends_are_counted_as_such() {
        // Right 6, then diagonal 4 by 4, then right: from (2,2) to (12,6).
        let s = card(&board(
            &(seg((2.0, 2.0), (6.0, 2.0), 0.2)
                + &seg((6.0, 2.0), (10.0, 6.0), 0.2)
                + &seg((10.0, 6.0), (12.0, 6.0), 0.2)),
        ));
        assert_eq!(
            s.bends,
            Bends {
                deg45: 2,
                ..Bends::default()
            }
        );
        // The straight line between the pads is the spanning length.
        assert!((s.spanning_length - 116f64.sqrt()).abs() < 1e-9);
        assert!(s.detour() > 1.0);
    }

    #[test]
    fn an_acute_bend_and_an_off_angle_track_are_counted() {
        let s = card(&board(
            &(seg((2.0, 2.0), (8.0, 2.0), 0.2)
                + &seg((8.0, 2.0), (4.0, 4.0), 0.2)
                + &seg((4.0, 4.0), (12.0, 6.0), 0.2)),
        ));
        assert_eq!(s.bends.acute, 2);
        assert_eq!(s.off_angle, 2);
    }

    #[test]
    fn a_bend_inside_a_pad_is_not_a_corner() {
        // A stub that turns at the pad's centre.
        let s = card(&board(
            &(seg((2.2, 2.0), (2.0, 2.0), 0.2)
                + &seg((2.0, 2.0), (2.0, 2.2), 0.2)
                + &seg((2.0, 2.2), (12.0, 6.0), 0.2)),
        ));
        assert_eq!(s.bends.deg90, 0, "{:?}", s.bends);
    }

    #[test]
    fn a_track_narrower_than_its_class_is_narrow_copper() {
        let pro = r#"{"net_settings": {"classes": [
            {"name": "Default", "track_width": 0.1},
            {"name": "Track_0.5mm", "track_width": 0.5, "priority": 0}],
          "netclass_patterns": [{"netclass": "Track_0.5mm", "pattern": "A"}]}}"#;
        let text =
            board(&(seg((2.0, 2.0), (12.0, 2.0), 0.5) + &seg((12.0, 2.0), (12.0, 6.0), 0.1)));
        let s = Scorecard::of(&text, Some(pro)).unwrap();
        assert_eq!(s.narrow.len(), 1);
        assert!((s.narrow["A"] - 4.0).abs() < 1e-9);
        assert_eq!(s.widths, vec![0.1, 0.5]);
    }

    #[test]
    fn a_track_across_a_pour_cuts_it_in_two_pieces() {
        let zone = r#"(zone (net 2) (net_name "GND") (layer "F.Cu") (hatch edge 0.5)
            (connect_pads (clearance 0.2)) (min_thickness 0.2) (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5))
            (polygon (pts (xy 0 0) (xy 20 0) (xy 20 10) (xy 0 10))))"#;
        // A track from the top edge to the bottom edge, over the whole height.
        let across = "(segment (start 7 0.3) (end 7 9.7) (width 0.2) (layer \"F.Cu\") (net 1))\n";
        let whole = card(&board(zone));
        let cut = card(&board(&format!("{zone}\n{across}")));
        assert_eq!(whole.pours.len(), 1);
        assert_eq!(whole.pours[0].pieces, 1, "{:?}", whole.pours);
        assert!(whole.pours[0].foreign_track == 0.0);
        assert_eq!(cut.pours[0].pieces, 2, "{:?}", cut.pours);
        assert!((cut.pours[0].foreign_track - 9.4).abs() < 0.11);
    }

    #[test]
    fn a_diff_pair_is_found_by_its_names_and_its_skew_measured() {
        let text = r#"(kicad_pcb (version 20240108) (generator "test")
  (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (44 "Edge.Cuts" user))
  (net 0 "") (net 1 "usb.dp") (net 2 "usb.dm")
  (gr_rect (start 0 0) (end 20 10) (layer "Edge.Cuts") (stroke (width 0.05)))
  (segment (start 2 2) (end 12 2) (width 0.2) (layer "F.Cu") (net 1))
  (segment (start 2 2.4) (end 14 2.4) (width 0.2) (layer "F.Cu") (net 2))
)"#;
        let s = card(text);
        assert_eq!(s.pairs.len(), 1);
        let p = &s.pairs[0];
        assert_eq!((p.p.as_str(), p.n.as_str()), ("usb.dp", "usb.dm"));
        assert!((p.skew - 2.0).abs() < 1e-9);
        assert!((p.coupled - 1.0).abs() < 1e-9);
    }
}
