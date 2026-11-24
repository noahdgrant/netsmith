//! The ratsnest: the connections that are still to be routed.
//!
//! Each net's pads are joined by a minimum spanning tree of straight lines between pad
//! centres, which is the shortest set of lines that leaves no pad cut off. Pads that
//! routed copper already joins count as one, so a fully routed board has no ratsnest.
//!
//! A net that is poured as a plane on an inner layer is joined by the plane, so its pads
//! need no lines between them. A through-hole pad and a via of the net reach the plane
//! where they stand; any other pad needs a via to it, and has a line to the plane until
//! one is dropped.
//!
//! An outer pour of a plane's net joins such a pad too, through a via it reaches (issue #834):
//! where a pad of the net is still cut off and the net has a via, the pour is filled as the
//! Gerbers fill it, less the copper narrower than its minimum width that KiCad's fill trims
//! ([`crate::plane_pour_fills`]), and each piece joins what it reaches as a zone's piece does
//! (below). The router drops the via beside a pad where this joins the pad without it. A board
//! whose plane nets have no via (one not routed yet) is not filled, and its pads keep their
//! lines as before; [`Ratsnest::without_pour_fills`] never fills.
//!
//! Pads of one net whose copper lies one on the other, on a layer both have, are one piece with no
//! copper added (the plated holes a QFN footprint draws inside its exposed pad, as KiCad and the
//! design rules check count them): the router draws no track between them.
//!
//! Copper joins where it runs into other copper, as KiCad's connectivity has it (issue #1334): a
//! track's end nearer to a pad (its box) than half the track's width, so an end on the pad's
//! edge, joins it, and a round end that only touches the edge does not (the design rules check
//! says so: [`netsmith_geom::COPPER_JOIN`]); a track over a pad, two tracks of one layer whose copper
//! meets (end to end, or an end on the other's middle, as a hand-routed board has them), and a
//! via on a pad or on a track. An end on another track's middle joins it while it lies nearer
//! to the other's centre line than the two half widths together, whatever the widths; an end that
//! only touches an arc's edge joins it, as KiCad's hit test of an arc has it (issue #1542). A net a
//! routes file locks ([`crate::routes`]) is joined by its
//! pours on the outer layers too: the router adds nothing to it, so the pour is how its pads
//! meet, and a pour joins the pads, tracks and vias of its net on its layer, as the design rules
//! check takes it (its fill is not computed). Other nets are not: the router routes them to
//! every pad as before.
//!
//! A zone of a routes file ([`crate::BoardZone`]) joins what of its net its fill reaches, on
//! every net, locked or not: the fill is computed ([`crate::zone_fills`], the Gerbers' fill, each
//! zone less its clearances round other nets' copper), and each piece of it joins the pads of
//! its net with copper on its layer whose centre it covers or whose box holds a corner of it
//! (a thermal relief's spokes reach the pad's centre; a pad the zone does not join keeps its
//! clearance), the vias whose centre it covers, and the tracks of its layer that run into it. On
//! a layer with such a zone, the design's pour or plane is not drawn where a zone of its own net
//! restates it; under zones of other nets it is drawn round them, and its fill, computed with
//! theirs, joins what it reaches as theirs does (issue #1314). Either way it gives no join of its
//! own there.

use crate::board::rect_distance;
use crate::board::{
    Board, BoardNet, BoardPour, CopperLayer, FixedCopper, PadRef, Placed, Plane, Track, Via,
};
use crate::copper::{Area, ZoneFill};
use crate::geometry::{Pt, Rect};

/// Copper this close to a pad, or to other copper, touches it.
const TOUCH: f64 = 1e-6;

/// Whether copper at `distance` from other copper, reaching `reach` round its centre line, runs
/// into it: KiCad joins it when the distance is less than the reach, strictly (issue #1334).
fn runs_into(distance: f64, reach: f64) -> bool {
    distance < reach - netsmith_geom::COPPER_JOIN
}

/// Whether a track's end at `end`, its copper reaching `reach` round it, is joined to `other`:
/// it lies nearer to the other's centre line than the two reaches together, so the round end
/// runs into the other's copper, whatever the two widths (a T of a narrow track on a wide one),
/// as KiCad 9 joins them. An arc's edge counts as on it, as KiCad's hit test of an arc has it,
/// and a straight track's does not (issue #1542).
fn end_on_track(other: &Track, end: Pt, reach: f64) -> bool {
    let (d, reach) = (other.distance_to(end), reach + other.width / 2.0);
    if other.arc().is_some() {
        d <= reach + netsmith_geom::COPPER_ARC_TOUCH
    } else {
        runs_into(d, reach)
    }
}

/// One line still to be routed.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Ratline {
    pub net: String,
    pub from: PadRef,
    pub to: PadRef,
    pub from_at: Pt,
    pub to_at: Pt,
    /// The line is from a pad to its net's plane, which is not a place on the board: `to`
    /// is `from` and the line has no length.
    pub to_plane: bool,
}

impl std::fmt::Display for Ratline {
    /// `net GND: D1.1 to J1.2`, or `net GND: D1.1 to the plane`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.to_plane {
            write!(f, "net {}: {} to the plane", self.net, self.from)
        } else {
            write!(f, "net {}: {} to {}", self.net, self.from, self.to)
        }
    }
}

impl Ratline {
    pub fn length(&self) -> f64 {
        self.from_at.distance(self.to_at)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ratsnest {
    pub lines: Vec<Ratline>,
}

/// One pad that a net reaches.
pub struct Node {
    pub pad: PadRef,
    pub centre: Pt,
    pub bounds: Rect,
    pub front: bool,
    pub back: bool,
}

/// The pads a net reaches. A net that names a pad number reaches every pad of that
/// footprint with the number, and KiCad does not count them as joined to each other, so
/// each is a node of its own.
pub fn nodes_of(parts: &[Placed], net: &BoardNet) -> Vec<Node> {
    let mut nodes = Vec::new();
    for pad in &net.pads {
        let Some(part) = parts.iter().find(|p| p.refdes == pad.refdes) else {
            continue;
        };
        nodes.extend(part.footprint.pads_numbered(&pad.number).map(|p| {
            let (front, back) = part.pad_layers(p);
            Node {
                pad: pad.clone(),
                centre: part.pad_centre(p),
                bounds: part.pad_bounds(p),
                front,
                back,
            }
        }));
    }
    nodes
}

/// Sets that merge, as a vector of parents.
struct Sets(Vec<usize>);

impl Sets {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }

    fn find(&mut self, i: usize) -> usize {
        let mut root = i;
        while self.0[root] != root {
            root = self.0[root];
        }
        let mut at = i;
        while self.0[at] != root {
            at = std::mem::replace(&mut self.0[at], root);
        }
        root
    }

    fn join(&mut self, a: usize, b: usize) -> bool {
        let (a, b) = (self.find(a), self.find(b));
        if a == b {
            return false;
        }
        self.0[a.max(b)] = a.min(b);
        true
    }
}

fn near(a: Pt, b: Pt) -> bool {
    a.distance(b) < TOUCH
}

/// Whether the copper of two tracks of one layer meets: an end of one lies on the other's
/// copper (a track that ends on another's middle), or their centre lines cross.
fn tracks_touch(a: &Track, b: &Track) -> bool {
    let ends_on = |x: &Track, y: &Track| {
        [x.from, x.to]
            .iter()
            .any(|&e| end_on_track(y, e, x.width / 2.0))
    };
    if ends_on(a, b) || ends_on(b, a) {
        return true;
    }
    let (pa, pb) = (a.centre_line(), b.centre_line());
    pa.windows(2)
        .any(|p| pb.windows(2).any(|q| crosses(p[0], p[1], q[0], q[1])))
}

/// Whether segment `a`-`b` crosses segment `c`-`d` (their insides meet at one point).
fn crosses(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let side = |p: Pt, q: Pt, r: Pt| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (side(a, b, c), side(a, b, d));
    let (d3, d4) = (side(c, d, a), side(c, d, b));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// Whether the pad has copper on the layer: an inner layer holds only a through-hole pad.
fn has_layer(node: &Node, layer: CopperLayer) -> bool {
    match layer {
        CopperLayer::Front => node.front,
        CopperLayer::Back => node.back,
        CopperLayer::Inner(_) => node.front && node.back,
    }
}

/// Whether copper round `at`, reaching `reach` from it (half a track's width, a via's radius),
/// runs into the pad's box on the layer (`None`: on either face), as the ratsnest joins copper to
/// a pad. `netsmith layout move` asks it of the ends of the routes file (issue #1627).
pub fn on_pad(node: &Node, at: Pt, reach: f64, layer: Option<CopperLayer>) -> bool {
    let on_layer = match layer {
        Some(layer) => has_layer(node, layer),
        None => node.front || node.back,
    };
    on_layer && runs_into(rect_distance(&node.bounds, &Rect::new(at, at)), reach)
}

/// Whether the copper of two pads lies one on the other: the centre of one is inside the box of
/// the other. (The boxes alone would join two pads that only meet at their corners.)
fn overlap(a: &Node, b: &Node) -> bool {
    let inside = |p: &Node, q: &Node| {
        p.bounds
            .expanded(TOUCH)
            .contains(&Rect::new(q.centre, q.centre))
    };
    inside(a, b) || inside(b, a)
}

/// Whether the track's centre line runs through the pad (its box) on the track's layer: a
/// track drawn over a pad of its net, as a hand-routed board has it, joins the pad.
fn runs_over(node: &Node, track: &Track) -> bool {
    let on_layer = has_layer(node, track.layer);
    let b = node.bounds.expanded(TOUCH);
    on_layer
        && track
            .centre_line()
            .windows(2)
            .any(|w| segment_meets_box(w[0], w[1], &b))
}

/// Whether segment `a`-`b` has a point inside box `r` (Liang and Barsky's clipping).
fn segment_meets_box(a: Pt, b: Pt, r: &Rect) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, a.x - r.min.x),
        (dx, r.max.x - a.x),
        (-dy, a.y - r.min.y),
        (dy, r.max.y - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                lo = lo.max(t);
            } else {
                hi = hi.min(t);
            }
        }
    }
    lo <= hi
}

impl Ratsnest {
    /// The ratsnest of a board as it is placed and routed. Nets are taken in board order.
    pub fn of(board: &Board) -> Ratsnest {
        let first = Self::without_pour_fills(board);
        // A pad of a plane net still cut off from its plane, on an outer layer poured with its
        // net, may reach it through the pour and a via the pour covers (the module's header).
        let poured = |l: &Ratline| {
            l.to_plane
                && board.vias.iter().any(|v| v.net == l.net)
                && board.pours.iter().any(|p| {
                    p.net == l.net
                        && !board.zoned(p.layer.name())
                        && board.part(&l.from.refdes).is_some_and(|part| {
                            part.footprint.pads_numbered(&l.from.number).any(|pad| {
                                match (p.layer, part.pad_layers(pad)) {
                                    (CopperLayer::Front, (front, _)) => front,
                                    (CopperLayer::Back, (_, back)) => back,
                                    (CopperLayer::Inner(_), (front, back)) => front && back,
                                }
                            })
                        })
                })
        };
        if !first.lines.iter().any(poured) {
            return first;
        }
        // A fill that cannot be made joins nothing.
        let fills = crate::copper::plane_pour_fills(board).unwrap_or_default();
        if fills.is_empty() {
            return first;
        }
        Self::with_fills(board, fills)
    }

    /// The ratsnest of a board with the outer pours of a plane net joining nothing but what
    /// every pour of a locked net joins: what [`Self::of`] was before issue #834, and what the
    /// router starts from, so that it gives every such pad a way to its plane of its own before
    /// it knows where the other copper goes.
    pub fn without_pour_fills(board: &Board) -> Ratsnest {
        Self::with_fills(board, Vec::new())
    }

    /// The ratsnest with `more` fills joining what they reach, beside those of the zones.
    fn with_fills(board: &Board, more: Vec<ZoneFill>) -> Ratsnest {
        // A fill that cannot be made (the Gerbers say why) joins nothing.
        let mut fills = crate::copper::zone_fills(board).unwrap_or_default();
        fills.extend(more);
        let zoned: Vec<&str> = board
            .zones
            .iter()
            .flat_map(|z| z.layers.iter().map(String::as_str))
            .collect();
        // The sense copper the board file carves the pours round (issue #1174) joins no pour
        // or plane.
        let carved = crate::kelvin::sense_carves(board);
        Self::from_parts(
            board.copper_count(),
            &board.parts,
            &board.nets,
            (&board.planes, &board.pours),
            &board.tracks,
            &board.vias,
            &board.fixed,
            (&zoned, &fills),
            &board.branch_currents,
            &carved,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        layers: u32,
        parts: &[Placed],
        nets: &[BoardNet],
        (planes, pours): (&[Plane], &[BoardPour]),
        tracks: &[Track],
        vias: &[Via],
        fixed: &FixedCopper,
        (zoned, fills): (&[&str], &[ZoneFill]),
        branches: &[netsmith_core::BranchCurrent],
        carved: &[crate::kelvin::SenseCarve],
    ) -> Ratsnest {
        let mut lines = Vec::new();
        for net in nets {
            let nodes = nodes_of(parts, net);
            // The pads of the pins that state the current of their branch (issue #1113).
            let branch: Vec<bool> = nodes
                .iter()
                .map(|node| {
                    branches.iter().any(|b| {
                        b.net == net.name
                            && b.pins
                                .iter()
                                .any(|p| p.refdes == node.pad.refdes && p.pin == node.pad.number)
                    })
                })
                .collect();
            let (track_at, tracks): (Vec<usize>, Vec<&Track>) = tracks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.net == net.name)
                .unzip();
            let (via_at, vias): (Vec<usize>, Vec<&Via>) = vias
                .iter()
                .enumerate()
                .filter(|(_, v)| v.net == net.name)
                .unzip();
            // Items (pads, then tracks, then vias) that no pour or plane joins: a sense pin's
            // copper, which the pours are carved round.
            let mut alone = vec![false; nodes.len() + tracks.len() + vias.len()];
            for c in carved.iter().filter(|c| c.net == net.name) {
                for (k, node) in nodes.iter().enumerate() {
                    alone[k] |= c.copper.pads.contains(&node.pad);
                }
                for (k, at) in track_at.iter().enumerate() {
                    alone[nodes.len() + k] |= c.copper.tracks.contains(at);
                }
                for (k, at) in via_at.iter().enumerate() {
                    alone[nodes.len() + tracks.len() + k] |= c.copper.vias.contains(at);
                }
            }
            // The layers of the net's planes, which a via reaches if it has copper on one: every
            // via but a blind or a buried one that ends short of them (issue #1015).
            let plane_layers: Vec<CopperLayer> = planes
                .iter()
                .filter(|p| p.net == net.name && !zoned.contains(&p.layer.as_str()))
                .filter_map(|p| CopperLayer::of_name(&p.layer))
                .collect();
            let plane = !plane_layers.is_empty();
            // A locked net's pours join it (the module's header), and its pads unless the pour
            // joins none (`.pads &= PadConnection::None`, as the design rules check takes it).
            let poured: Vec<(CopperLayer, bool)> = if fixed.is_locked(&net.name) {
                pours
                    .iter()
                    .filter(|p| p.net == net.name && !zoned.contains(&p.layer.name()))
                    .map(|p| (p.layer, p.pads != Some(crate::board::ZonePads::None)))
                    .collect()
            } else {
                Vec::new()
            };
            let pieces: Vec<Piece> = fills
                .iter()
                .filter(|f| f.net == net.name)
                .flat_map(|f| f.areas.iter().filter_map(|a| Piece::of(&f.layer, a)))
                .collect();
            let joined = join_pieces(layers, &nodes, &tracks, &vias, &pieces);
            let reaches_plane: Vec<bool> = vias
                .iter()
                .map(|v| plane_layers.iter().any(|&l| v.on(l, layers)))
                .collect();
            lines.extend(spanning_tree(
                layers,
                &net.name,
                &nodes,
                (&tracks, &vias),
                (plane, &reaches_plane),
                &poured,
                &joined,
                &branch,
                &alone,
            ));
        }
        Ratsnest { lines }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The sum of every line's length, in mm.
    pub fn total_length(&self) -> f64 {
        self.lines
            .iter()
            .map(Ratline::length)
            .fold(0.0, |a, b| a + b)
    }
}

/// A piece of a zone's fill on a layer, with its box.
struct Piece<'a> {
    /// `Some` on an outer layer; `None` on an inner one, which only through-hole pads and vias
    /// reach.
    layer: Option<CopperLayer>,
    /// The layer, inner ones too, if it is a copper layer's name.
    copper: Option<CopperLayer>,
    area: &'a Area,
    bounds: Rect,
}

impl<'a> Piece<'a> {
    fn of(layer: &str, area: &'a Area) -> Option<Piece<'a>> {
        let first = *area.outer.first()?;
        let mut bounds = Rect::new(first, first);
        for p in &area.outer {
            bounds.min.x = bounds.min.x.min(p.x);
            bounds.min.y = bounds.min.y.min(p.y);
            bounds.max.x = bounds.max.x.max(p.x);
            bounds.max.y = bounds.max.y.max(p.y);
        }
        Some(Piece {
            layer: match layer {
                "F.Cu" => Some(CopperLayer::Front),
                "B.Cu" => Some(CopperLayer::Back),
                _ => None,
            },
            copper: CopperLayer::of_name(layer),
            area,
            bounds: bounds.expanded(TOUCH),
        })
    }

    fn covers(&self, p: Pt) -> bool {
        self.bounds.contains(&Rect::new(p, p))
            && inside(&self.area.outer, p)
            && !self.area.holes.iter().any(|h| inside(h, p))
    }

    /// Whether a track of its layer runs into it: a point of its centre line is in it, or the
    /// centre line crosses one of its rings.
    fn meets(&self, track: &Track) -> bool {
        let line = track.centre_line();
        line.iter().any(|&p| self.covers(p))
            || std::iter::once(&self.area.outer)
                .chain(&self.area.holes)
                .any(|ring| {
                    (0..ring.len()).any(|i| {
                        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                        line.windows(2).any(|w| crosses(w[0], w[1], a, b))
                    })
                })
    }
}

/// Whether `p` is inside the ring (the even-odd rule).
fn inside(ring: &[Pt], p: Pt) -> bool {
    let mut odd = false;
    let mut j = ring.len().wrapping_sub(1);
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// What the pieces of a net's zones join, as pairs of items in [`spanning_tree`]'s numbering
/// (pads, then tracks, then vias): for each piece, each item it reaches with the first one.
fn join_pieces(
    layers: u32,
    nodes: &[Node],
    tracks: &[&Track],
    vias: &[&Via],
    pieces: &[Piece],
) -> Vec<(usize, usize)> {
    let (n, t) = (nodes.len(), tracks.len());
    let mut pairs = Vec::new();
    for piece in pieces {
        let mut reached: Vec<usize> = Vec::new();
        for (ni, node) in nodes.iter().enumerate() {
            let on = match piece.layer {
                Some(layer) => has_layer(node, layer),
                None => node.front && node.back,
            };
            let near = node.bounds.expanded(TOUCH);
            if on
                && rect_distance(&near, &piece.bounds) == 0.0
                && (piece.covers(node.centre)
                    || piece
                        .area
                        .outer
                        .iter()
                        .any(|&p| near.contains(&Rect::new(p, p))))
            {
                reached.push(ni);
            }
        }
        // A pad whose copper overlaps that of a pad the piece reaches, on a layer both have, is
        // reached too (a connector's shield pad made of a through-hole oval and a surface
        // rectangle of one number: the thermal relief of one cuts the other's spokes, but their
        // copper touches, as KiCad counts it).
        let direct = reached.clone();
        for (ni, node) in nodes.iter().enumerate() {
            let touches = |&ri: &usize| {
                let other = &nodes[ri];
                ((node.front && other.front) || (node.back && other.back))
                    && rect_distance(&node.bounds, &other.bounds) <= TOUCH
            };
            if !reached.contains(&ni) && direct.iter().any(touches) {
                reached.push(ni);
            }
        }
        for (ti, track) in tracks.iter().enumerate() {
            if piece.layer == Some(track.layer) && piece.meets(track) {
                reached.push(n + ti);
            }
        }
        for (vi, via) in vias.iter().enumerate() {
            if piece.copper.is_none_or(|l| via.on(l, layers)) && piece.covers(via.at) {
                reached.push(n + t + vi);
            }
        }
        if let Some((&first, rest)) = reached.split_first() {
            pairs.extend(rest.iter().map(|&other| (first, other)));
        }
    }
    pairs
}

/// The pairs of a net's items whose copper touches, with no pour or plane: items `0..n` are the
/// pads (`nodes`), then the tracks, then the vias. Pads whose copper lies one on the other, on
/// a layer both have, are one piece (as in KiCad and in the design rules check): the plated
/// holes drawn inside a QFN's exposed pad are the pad's own, and need no track to it or to
/// each other. A blind or a buried via joins copper on its own layers only (issue #1015).
fn copper_joins(
    layers: u32,
    nodes: &[Node],
    tracks: &[&Track],
    vias: &[&Via],
) -> Vec<(usize, usize)> {
    let (n, t) = (nodes.len(), tracks.len());
    let mut out = Vec::new();
    for (i, a) in nodes.iter().enumerate() {
        for (j, b) in nodes.iter().enumerate().skip(i + 1) {
            if (a.front && b.front || a.back && b.back) && overlap(a, b) {
                out.push((i, j));
            }
        }
    }
    for (ti, track) in tracks.iter().enumerate() {
        for (ni, node) in nodes.iter().enumerate() {
            let touches = [track.from, track.to]
                .iter()
                .any(|&end| on_pad(node, end, track.width / 2.0, Some(track.layer)))
                || runs_over(node, track);
            if touches {
                out.push((ni, n + ti));
            }
        }
        for (tj, other) in tracks.iter().enumerate().skip(ti + 1) {
            let meet = track.layer == other.layer
                && ([track.from, track.to]
                    .iter()
                    .any(|&a| [other.from, other.to].iter().any(|&b| near(a, b)))
                    || tracks_touch(track, other));
            if meet {
                out.push((n + ti, n + tj));
            }
        }
    }
    for (vi, via) in vias.iter().enumerate() {
        for (ni, node) in nodes.iter().enumerate() {
            let shares = (node.front && via.on(CopperLayer::Front, layers))
                || (node.back && via.on(CopperLayer::Back, layers))
                || (node.front && node.back);
            if shares && on_pad(node, via.at, via.diameter / 2.0, None) {
                out.push((ni, n + t + vi));
            }
        }
        for (ti, track) in tracks.iter().enumerate() {
            if !via.on(track.layer, layers) {
                continue;
            }
            if near(via.at, track.from)
                || near(via.at, track.to)
                || runs_into(
                    track.distance_to(via.at),
                    (track.width + via.diameter) / 2.0,
                )
            {
                out.push((n + t + vi, n + ti));
            }
        }
    }
    out
}

/// What the copper of a net joins to the pads `from` when the pad `cut` is taken away: a sense
/// pin's copper, without the pad it senses (issue #1148). Pours and planes are not counted: the
/// board file carves them round the sense copper (`crate::kelvin`, issue #1174).
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Reach {
    /// The other pads of the net it joins, by index of `nodes`.
    pub pads: Vec<usize>,
    /// The tracks and the vias it holds, by index of the slices it was given.
    pub tracks: Vec<usize>,
    pub vias: Vec<usize>,
    /// Whether a track or a via of it touches the pad `cut`.
    pub touches_cut: bool,
}

/// [`Reach`] of the pads `from` of a net without its pad `cut`, by index of `nodes`.
pub(crate) fn reach_without(
    layers: u32,
    nodes: &[Node],
    (tracks, vias): (&[&Track], &[&Via]),
    from: &[usize],
    cut: usize,
) -> Reach {
    let (n, t) = (nodes.len(), tracks.len());
    let joins = copper_joins(layers, nodes, tracks, vias);
    let mut sets = Sets::new(n + t + vias.len());
    for &(a, b) in &joins {
        if a != cut && b != cut {
            sets.join(a, b);
        }
    }
    if let Some((&first, rest)) = from.split_first() {
        for &other in rest {
            sets.join(first, other);
        }
    }
    let Some(&first) = from.first() else {
        return Reach::default();
    };
    let root = sets.find(first);
    let mut held = vec![false; n + t + vias.len()];
    for (i, h) in held.iter_mut().enumerate() {
        *h = i != cut && sets.find(i) == root;
    }
    let touches_cut = joins
        .iter()
        .any(|&(a, b)| (a == cut && b >= n && held[b]) || (b == cut && a >= n && held[a]));
    Reach {
        pads: (0..n).filter(|&i| held[i] && !from.contains(&i)).collect(),
        tracks: (0..t).filter(|&i| held[n + i]).collect(),
        vias: (0..vias.len()).filter(|&i| held[n + t + i]).collect(),
        touches_cut,
    }
}

/// Kruskal's algorithm, starting with everything the routed copper already joins (`joined`:
/// pairs of items a zone joins, [`join_pieces`]). Of equal lines, the one between the pads listed
/// first wins.
///
/// A pad of a branch (`branch`, a pin that states the current of its branch, issue #1113) is a
/// leaf: the pads of no branch are joined first, then each branch's pad by its shortest line to
/// one of them, and only last the lines between two branches' pads, which a net with a pad of no
/// branch never needs. So no current but the branch's passes through its line, and the router
/// draws that line for it. A net with no branch is joined as it was.
#[allow(clippy::too_many_arguments)]
fn spanning_tree(
    layers: u32,
    net: &str,
    nodes: &[Node],
    (tracks, vias): (&[&Track], &[&Via]),
    (plane, reaches_plane): (bool, &[bool]),
    poured: &[(CopperLayer, bool)],
    joined: &[(usize, usize)],
    branch: &[bool],
    alone: &[bool],
) -> Vec<Ratline> {
    let (n, t) = (nodes.len(), tracks.len());
    // Items 0..n are pads, then tracks, then vias, then the net's plane, then its pours on the
    // front and the back.
    let plane_item = n + t + vias.len();
    let pour_item = |layer: CopperLayer| plane_item + 1 + usize::from(layer == CopperLayer::Back);
    let mut sets = Sets::new(plane_item + 3);
    for &(a, b) in joined {
        sets.join(a, b);
    }
    for (a, b) in copper_joins(layers, nodes, tracks, vias) {
        sets.join(a, b);
    }
    for (ti, track) in tracks.iter().enumerate() {
        if !alone[n + ti] && poured.iter().any(|&(l, _)| l == track.layer) {
            sets.join(n + ti, pour_item(track.layer));
        }
    }
    for (vi, via) in vias
        .iter()
        .enumerate()
        .filter(|&(vi, _)| !alone[n + t + vi])
    {
        for &(layer, _) in poured.iter().filter(|&&(l, _)| via.on(l, layers)) {
            sets.join(n + t + vi, pour_item(layer));
        }
    }
    for (ni, node) in nodes.iter().enumerate().filter(|&(ni, _)| !alone[ni]) {
        for &(layer, pads) in poured {
            if pads && has_layer(node, layer) {
                sets.join(ni, pour_item(layer));
            }
        }
    }

    if plane {
        // A through-hole pad has copper on every layer, and so has a via.
        for (ni, node) in nodes.iter().enumerate() {
            if node.front && node.back && !alone[ni] {
                sets.join(ni, plane_item);
            }
        }
        for vi in (0..vias.len()).filter(|&vi| reaches_plane[vi] && !alone[n + t + vi]) {
            sets.join(n + t + vi, plane_item);
        }
        // The plane joins every pad that reaches it, so the pads that do not need a line
        // each, and none to one another.
        return nodes
            .iter()
            .enumerate()
            .filter(|&(i, _)| sets.join(i, plane_item))
            .map(|(_, node)| Ratline {
                net: net.to_string(),
                from: node.pad.clone(),
                to: node.pad.clone(),
                from_at: node.centre,
                to_at: node.centre,
                to_plane: true,
            })
            .collect();
    }

    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            pairs.push((nodes[i].centre.distance(nodes[j].centre), i, j));
        }
    }
    // Pads of no branch with each other first, then a branch's pad with one of them, then two
    // branches' pads.
    let rank = |i: usize, j: usize| usize::from(branch[i]) + usize::from(branch[j]);
    pairs.sort_by(|a, b| {
        rank(a.1, a.2)
            .cmp(&rank(b.1, b.2))
            .then(a.0.total_cmp(&b.0))
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
    });
    let mut lines = Vec::new();
    for (_, i, j) in pairs {
        if sets.join(i, j) {
            lines.push(Ratline {
                net: net.to_string(),
                from: nodes[i].pad.clone(),
                to: nodes[j].pad.clone(),
                from_at: nodes[i].centre,
                to_at: nodes[j].centre,
                to_plane: false,
            });
        }
    }
    lines
}

/// A track end or a via of a net that runs into nothing of its net: [`open_ends`].
#[derive(Debug, Clone, PartialEq)]
pub struct OpenEnd {
    /// The track or via, as the index of the list [`open_ends`] was given.
    pub copper: OpenCopper,
    /// The track's end, or the via's centre, on the sheet.
    pub at: Pt,
    /// The pad of the net nearest to it: where the copper most likely ran before the pad moved.
    pub pad: Option<PadRef>,
}

/// What an [`OpenEnd`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenCopper {
    Track(usize),
    Via(usize),
}

/// The ends of `tracks` and the `vias` (indices into [`Board::tracks`] and [`Board::vias`], all
/// of `net`) that run into nothing of the net on the board: no pad, no other track or via, no
/// pour, plane or zone of the net on their layer. A routes file's record whose pads moved keeps
/// its copper, and these are where it is open (issue #1328): the copper that ran to a pad that
/// is no longer there. Copper joins as the ratsnest joins it (the module's header); a pour or a
/// plane of the net on the layer joins anything (its fill is not computed), a zone what its
/// outline holds.
pub fn open_ends(board: &Board, net: &str, tracks: &[usize], vias: &[usize]) -> Vec<OpenEnd> {
    let layers = board.copper_count();
    let Some(board_net) = board.nets.iter().find(|n| n.name == net) else {
        return Vec::new();
    };
    let nodes = nodes_of(&board.parts, board_net);
    let all_tracks: Vec<(usize, &Track)> = board
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.net == net)
        .collect();
    let all_vias: Vec<(usize, &Via)> = board
        .vias
        .iter()
        .enumerate()
        .filter(|(_, v)| v.net == net)
        .collect();
    let poured = |layer: CopperLayer, at: Pt| {
        board.pours.iter().any(|p| p.net == net && p.layer == layer)
            || board
                .planes
                .iter()
                .any(|p| p.net == net && CopperLayer::of_name(&p.layer) == Some(layer))
            || board.zones.iter().any(|z| {
                z.net == net && z.layers.iter().any(|l| l == layer.name()) && inside(&z.outline, at)
            })
    };
    let nearest = |at: Pt| {
        nodes
            .iter()
            .min_by(|a, b| a.centre.distance(at).total_cmp(&b.centre.distance(at)))
            .map(|n| n.pad.clone())
    };
    let mut out = Vec::new();
    for (k, &i) in tracks.iter().enumerate() {
        let track = &board.tracks[i];
        for end in [track.from, track.to] {
            let reach = track.width / 2.0;
            let joined = nodes
                .iter()
                .any(|n| on_pad(n, end, reach, Some(track.layer)))
                || all_tracks.iter().any(|&(j, other)| {
                    j != i
                        && other.layer == track.layer
                        && (near(end, other.from)
                            || near(end, other.to)
                            || end_on_track(other, end, reach))
                })
                || all_vias.iter().any(|&(_, via)| {
                    via.on(track.layer, layers)
                        && (near(via.at, end)
                            || runs_into(via.at.distance(end), reach + via.diameter / 2.0))
                })
                || poured(track.layer, end);
            if !joined {
                out.push(OpenEnd {
                    copper: OpenCopper::Track(k),
                    at: end,
                    pad: nearest(end),
                });
            }
        }
    }
    for (k, &i) in vias.iter().enumerate() {
        let via = &board.vias[i];
        let reach = via.diameter / 2.0;
        let on_layers = |l: CopperLayer| via.on(l, layers);
        let joined = nodes.iter().any(|n| {
            let shares = (n.front && on_layers(CopperLayer::Front))
                || (n.back && on_layers(CopperLayer::Back))
                || (n.front && n.back);
            shares && on_pad(n, via.at, reach, None)
        }) || all_tracks.iter().any(|&(_, t)| {
            on_layers(t.layer)
                && (near(via.at, t.from)
                    || near(via.at, t.to)
                    || runs_into(t.distance_to(via.at), reach + t.width / 2.0))
        }) || all_vias.iter().any(|&(j, other)| {
            j != i && runs_into(other.at.distance(via.at), reach + other.diameter / 2.0)
        }) || [CopperLayer::Front, CopperLayer::Back]
            .into_iter()
            .chain((1..layers.saturating_sub(1)).map(|n| CopperLayer::Inner(n as u8)))
            .any(|l| on_layers(l) && poured(l, via.at));
        if !joined {
            out.push(OpenEnd {
                copper: OpenCopper::Via(k),
                at: via.at,
                pad: nearest(via.at),
            });
        }
    }
    // A piece of the copper that reaches one pad or none ran to a pad no longer there, even
    // where each of its ends lies on its own copper (a short track into a pad, beside the
    // track it leaves): it is open at its end farthest from the pad it reaches, named by the
    // nearest pad of the net that none of the copper reaches. A net a pour, a plane or a zone
    // joins is left to the ends above, as the fill may join the piece.
    if !crate::net_is_poured(board, net) && !nodes.is_empty() {
        let ts: Vec<&Track> = tracks.iter().map(|&i| &board.tracks[i]).collect();
        let vs: Vec<&Via> = vias.iter().map(|&i| &board.vias[i]).collect();
        let (n, t) = (nodes.len(), ts.len());
        let mut sets = Sets::new(n + t + vs.len());
        for (a, b) in copper_joins(layers, &nodes, &ts, &vs) {
            sets.join(a, b);
        }
        let roots: Vec<usize> = (0..n + t + vs.len()).map(|i| sets.find(i)).collect();
        let reached: Vec<bool> = (0..n).map(|i| roots[n..].contains(&roots[i])).collect();
        let mut done: Vec<usize> = Vec::new();
        for first in n..n + t + vs.len() {
            let root = roots[first];
            if done.contains(&root) {
                continue;
            }
            done.push(root);
            let pads: Vec<usize> = (0..n).filter(|&i| roots[i] == root).collect();
            if pads.len() > 1 {
                continue;
            }
            let ends: Vec<(OpenCopper, Pt)> = (n..n + t + vs.len())
                .filter(|&i| roots[i] == root)
                .flat_map(|i| {
                    if i < n + t {
                        let tr = ts[i - n];
                        vec![
                            (OpenCopper::Track(i - n), tr.from),
                            (OpenCopper::Track(i - n), tr.to),
                        ]
                    } else {
                        vec![(OpenCopper::Via(i - n - t), vs[i - n - t].at)]
                    }
                })
                .collect();
            if ends
                .iter()
                .any(|(c, _)| out.iter().any(|o: &OpenEnd| o.copper == *c))
            {
                continue;
            }
            let from = pads.first().map(|&i| nodes[i].centre);
            let far = ends.iter().max_by(|a, b| {
                let d = |p: Pt| from.map_or(0.0, |f| f.distance(p));
                d(a.1).total_cmp(&d(b.1))
            });
            let Some(&(copper, at)) = far else { continue };
            let pad = nodes
                .iter()
                .enumerate()
                .filter(|(i, _)| !reached[*i])
                .min_by(|a, b| a.1.centre.distance(at).total_cmp(&b.1.centre.distance(at)))
                .map(|(_, node)| node.pad.clone())
                .or_else(|| nearest(at));
            out.push(OpenEnd { copper, at, pad });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use netsmith_core::{Design, Net, Part, PinRef};

    use crate::board::{CopperLayer, Track, Via};
    use crate::geometry::Pt;

    use super::*;

    #[test]
    fn the_led_board_has_one_line_per_two_pad_net() {
        let board = crate::board::tests::led_board();
        let rats = Ratsnest::of(&board);
        assert_eq!(rats.len(), 3);
        let nets: Vec<&str> = rats.lines.iter().map(|l| l.net.as_str()).collect();
        assert_eq!(nets, ["GND", "VCC", "net-led.anode"]);
        let direct: f64 = board
            .nets
            .iter()
            .map(|n| {
                let a = &n.pads[0];
                let b = &n.pads[1];
                let at = |p: &PadRef| {
                    let part = board.part(&p.refdes).unwrap();
                    part.pad_centre(part.footprint.pads_numbered(&p.number).next().unwrap())
                };
                at(a).distance(at(b))
            })
            .sum();
        assert!((rats.total_length() - direct).abs() < 1e-9);
        assert!(rats.total_length() > 0.0);
    }

    #[test]
    fn a_track_whose_pad_moved_is_open_at_that_pad_and_one_on_its_pads_is_not() {
        // Issue #1328: a track from pad to pad of a net, then one part moved.
        let mut board = crate::board::tests::led_board();
        let net = board.nets[0].clone();
        let centre = |board: &Board, p: &PadRef| {
            let part = board.part(&p.refdes).unwrap();
            part.pad_centre(part.footprint.pads_numbered(&p.number).next().unwrap())
        };
        let (a, b) = (centre(&board, &net.pads[0]), centre(&board, &net.pads[1]));
        board.tracks.clear();
        board.vias.clear();
        board
            .tracks
            .push(Track::straight(&net.name, CopperLayer::Front, a, b, 0.25));
        assert_eq!(open_ends(&board, &net.name, &[0], &[]), []);

        let moved = net.pads[1].clone();
        let part = board
            .parts
            .iter_mut()
            .find(|p| p.refdes == moved.refdes)
            .unwrap();
        // Across the track, so that the pad does not land on its line.
        let len = a.distance(b);
        part.at = part.at + Pt::new(-(b.y - a.y) / len * 3.0, (b.x - a.x) / len * 3.0);
        let open = open_ends(&board, &net.name, &[0], &[]);
        assert_eq!(
            open,
            [OpenEnd {
                copper: OpenCopper::Track(0),
                at: b,
                pad: Some(moved),
            }]
        );

        // A via at the open end joins that end, but the piece of track and via still reaches
        // one pad only: it is open at its far end, the via, by the pad that moved.
        board.vias.push(Via {
            net: net.name.clone(),
            at: b,
            diameter: 0.6,
            drill: 0.3,
            span: None,
        });
        assert_eq!(
            open_ends(&board, &net.name, &[0], &[0]),
            [OpenEnd {
                copper: OpenCopper::Via(0),
                at: b,
                pad: Some(net.pads[1].clone()),
            }]
        );
    }

    #[test]
    fn a_net_of_n_pads_needs_n_minus_one_lines_and_they_are_the_shortest() {
        // Four resistors in a row on one net: the tree is the chain of neighbours.
        let design = Design {
            modules: Vec::new(),
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: (1..=4)
                .map(|i| Part {
                    provides: Vec::new(),
                    declared_kind: netsmith_testkit::std_kind("Resistor")
                        .and_then(netsmith_core::PartKind::from_word),
                    refdes: format!("R{i}"),
                    kind: "Resistor".into(),
                    value: "1k".into(),
                    package: Some("0402".into()),
                    path: String::new(),
                    pins: Vec::new(),
                })
                .collect(),
            nets: vec![Net {
                name: "N".into(),
                pins: (1..=4)
                    .map(|i| PinRef {
                        refdes: format!("R{i}"),
                        pin: "1".into(),
                    })
                    .collect(),
            }],
        };
        let mut board = crate::board::tests::unplaced_board(&design);
        for (i, p) in board.parts.iter_mut().enumerate() {
            p.at = Pt::new(i as f64 * 10.0, 0.0);
            p.rotation = 0.0;
        }
        let rats = Ratsnest::of(&board);
        assert_eq!(rats.len(), 3);
        assert!((rats.total_length() - 30.0).abs() < 1e-9);

        // Issue #1113: R2's pad states the current of its branch. It is a leaf, joined by one
        // line to the nearest of the others, and R1 and R3 are joined past it.
        board.branch_currents.push(netsmith_core::BranchCurrent {
            net: board.nets[0].name.clone(),
            path: "r2.a".into(),
            pins: vec![PinRef {
                refdes: "R2".into(),
                pin: "1".into(),
            }],
            current: 0.001,
            width: 0.1,
            least_width: 0.01,
            inner_least_width: None,
            vias: 1,
        });
        let rats = Ratsnest::of(&board);
        assert_eq!(rats.len(), 3);
        let at_r2 = |l: &&Ratline| l.from.refdes == "R2" || l.to.refdes == "R2";
        let r2: Vec<&Ratline> = rats.lines.iter().filter(at_r2).collect();
        assert_eq!(r2.len(), 1, "{:?}", rats.lines);
        assert!(board.line_branch(r2[0]).is_some());
        assert!(
            rats.lines
                .iter()
                .any(|l| [&l.from.refdes, &l.to.refdes] == [&"R1".to_string(), &"R3".to_string()]),
            "{:?}",
            rats.lines
        );
        assert!((rats.total_length() - 40.0).abs() < 1e-9);
        let others: Vec<&Ratline> = rats.lines.iter().filter(|l| !at_r2(l)).collect();
        assert!(others.iter().all(|l| board.line_branch(l).is_none()));
    }

    #[test]
    fn pads_that_share_a_number_are_each_a_node() {
        // The regulator's tab and pin 2 are both pad "2". KiCad leaves them unjoined, so a
        // net of that pad number and one resistor needs two lines: the resistor to the
        // nearer of the two, and that one to the other.
        let design = Design {
            modules: Vec::new(),
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: vec![
                Part {
                    provides: Vec::new(),
                    declared_kind: netsmith_testkit::std_kind("LinearRegulator")
                        .and_then(netsmith_core::PartKind::from_word),
                    refdes: "U1".into(),
                    kind: "LinearRegulator".into(),
                    value: "LDO".into(),
                    package: Some("SOT-223".into()),
                    path: String::new(),
                    pins: Vec::new(),
                },
                Part {
                    provides: Vec::new(),
                    declared_kind: netsmith_testkit::std_kind("Resistor")
                        .and_then(netsmith_core::PartKind::from_word),
                    refdes: "R1".into(),
                    kind: "Resistor".into(),
                    value: "1k".into(),
                    package: Some("0402".into()),
                    path: String::new(),
                    pins: Vec::new(),
                },
            ],
            nets: vec![Net {
                name: "N".into(),
                pins: vec![
                    PinRef {
                        refdes: "U1".into(),
                        pin: "2".into(),
                    },
                    PinRef {
                        refdes: "R1".into(),
                        pin: "1".into(),
                    },
                ],
            }],
        };
        let mut board = crate::board::tests::unplaced_board(&design);
        board.parts[0].at = Pt::new(0.0, 0.0);
        board.parts[1].at = Pt::new(10.0, 0.0);
        board.parts[0].rotation = 0.0;
        board.parts[1].rotation = 0.0;
        let rats = Ratsnest::of(&board);
        assert_eq!(rats.len(), 2);
        // The pad and the tab are 6.3 mm apart, and the tab is the nearer to R1.
        let expected = 6.3 + (10.0 - 0.51 - 3.15);
        assert!((rats.total_length() - expected).abs() < 1e-9);
    }

    #[test]
    fn copper_that_joins_two_pads_removes_their_line() {
        let mut board = crate::board::tests::led_board();
        assert_eq!(Ratsnest::of(&board).len(), 3);
        let ends = |board: &Board, refdes: &str, pad: &str| {
            let part = board.part(refdes).unwrap();
            part.pad_centre(part.footprint.pads_numbered(pad).next().unwrap())
        };
        // R1.1 to J1.1, on the front: the VCC net is done, the others are not.
        board.tracks.push(Track {
            net: "VCC".into(),
            layer: CopperLayer::Front,
            from: ends(&board, "R1", "1"),
            to: ends(&board, "J1", "1"),
            width: 0.2,
            mid: None,
        });
        let rats = Ratsnest::of(&board);
        assert_eq!(rats.len(), 2);
        assert!(rats.lines.iter().all(|l| l.net != "VCC"));
        // The same copper on the back misses the surface mount pad.
        board.tracks[0].layer = CopperLayer::Back;
        assert_eq!(Ratsnest::of(&board).len(), 3);
    }

    #[test]
    fn a_via_joins_copper_on_both_layers() {
        let mut board = crate::board::tests::led_board();
        let part = board.part("J1").unwrap();
        let j1_1 = part.pad_centre(part.footprint.pads_numbered("1").next().unwrap());
        let part = board.part("R1").unwrap();
        let r1_1 = part.pad_centre(part.footprint.pads_numbered("1").next().unwrap());
        let via_at = Pt::new(j1_1.x + 4.0, j1_1.y);
        let track = |layer, from, to| Track {
            net: "VCC".into(),
            layer,
            from,
            to,
            width: 0.2,
            mid: None,
        };
        // Back copper from the header's through-hole pad to a via, then front copper on to
        // the resistor.
        board.tracks.push(track(CopperLayer::Back, j1_1, via_at));
        board.tracks.push(track(CopperLayer::Front, via_at, r1_1));
        assert_eq!(Ratsnest::of(&board).len(), 3, "no via yet, so no join");
        board.vias.push(Via {
            net: "VCC".into(),
            at: via_at,
            diameter: 0.6,
            drill: 0.3,
            span: None,
        });
        assert_eq!(Ratsnest::of(&board).len(), 2);
    }

    fn pad_at(board: &Board, refdes: &str, pad: &str) -> Pt {
        let part = board.part(refdes).unwrap();
        part.pad_centre(part.footprint.pads_numbered(pad).next().unwrap())
    }

    #[test]
    fn a_plane_joins_its_nets_through_hole_pads_and_leaves_each_surface_mount_pad_a_line() {
        // GND is D1.1 (surface mount) and J1.2 (through hole); 3V3 is J1.1 and R1.1.
        let board = crate::board::tests::four_layer_board();
        let rats = Ratsnest::of(&board);
        let summary: Vec<(&str, String, bool)> = rats
            .lines
            .iter()
            .map(|l| (l.net.as_str(), l.from.to_string(), l.to_plane))
            .collect();
        assert_eq!(
            summary,
            [
                ("GND", "D1.1".into(), true),
                ("3V3", "R1.1".into(), true),
                ("net-led.anode", "D1.2".into(), false),
            ]
        );
        let plane_line = &rats.lines[0];
        assert_eq!(plane_line.from, plane_line.to);
        assert_eq!(plane_line.length(), 0.0);
        // The same nets on two layers want a line between pads.
        assert_eq!(Ratsnest::of(&crate::board::tests::led_board()).len(), 3);
    }

    #[test]
    fn a_via_beside_the_pad_with_a_track_to_it_joins_the_pad_to_the_plane() {
        let mut board = crate::board::tests::four_layer_board();
        let d1 = pad_at(&board, "D1", "1");
        let via_at = Pt::new(d1.x, d1.y + 1.5);
        let via = |net: &str, at| Via {
            net: net.into(),
            at,
            diameter: 0.45,
            drill: 0.15,
            span: None,
        };
        // A via of the net alone is on the plane but not on the pad.
        board.vias.push(via("GND", via_at));
        assert_eq!(Ratsnest::of(&board).len(), 3);
        board.tracks.push(Track {
            net: "GND".into(),
            layer: CopperLayer::Front,
            from: d1,
            to: via_at,
            width: 0.09,
            mid: None,
        });
        let rats = Ratsnest::of(&board);
        assert_eq!(rats.len(), 2);
        assert!(rats.lines.iter().all(|l| l.net != "GND"));
        // A via of another net does not reach this plane.
        board.tracks.clear();
        board.vias[0].net = "net-led.anode".into();
        assert_eq!(Ratsnest::of(&board).len(), 3);
    }

    /// A pad of R1's pad 1 (the VCC net) that stands in the pad: a plated hole drawn over the
    /// surface mount pad's copper, as a QFN's exposed pad has its vias.
    fn board_with_a_via_pad_in_r1_1() -> Board {
        use crate::footprint::{Drill, Pad, PadKind, PadShape};
        let mut board = crate::board::tests::led_board();
        let part = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
        let pad = part.footprint.pads_numbered("1").next().unwrap().clone();
        part.footprint.pads.push(Pad {
            kind: PadKind::ThruHole,
            shape: PadShape::Circle,
            width: pad.height.min(pad.width) * 0.5,
            height: pad.height.min(pad.width) * 0.5,
            drill: Some(Drill::round(0.1)),
            layers: vec!["*.Cu".into(), "*.Mask".into()],
            corner_ratio: 0.0,
            ..pad
        });
        board
    }

    #[test]
    fn pads_of_one_net_whose_copper_overlaps_are_already_joined() {
        // The via pad lies on the surface mount pad's copper, on the front, and KiCad and the
        // design rules check count them as one piece (`drc::connect`): the router draws no
        // track between them, only the one line that the net needs.
        let board = board_with_a_via_pad_in_r1_1();
        let vcc: Vec<String> = Ratsnest::of(&board)
            .lines
            .iter()
            .filter(|l| l.net == "VCC")
            .map(|l| l.to_string())
            .collect();
        assert_eq!(vcc.len(), 1, "{vcc:?}");
    }

    #[test]
    fn pads_of_one_net_that_do_not_touch_still_need_a_line() {
        // The same pad moved clear of the surface mount pad: two pieces, so two lines.
        let mut board = board_with_a_via_pad_in_r1_1();
        let part = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
        part.footprint.pads.last_mut().unwrap().at.y += 3.0;
        let vcc = Ratsnest::of(&board)
            .lines
            .iter()
            .filter(|l| l.net == "VCC")
            .count();
        assert_eq!(vcc, 2);
    }

    #[test]
    fn a_line_says_what_it_joins() {
        let board = crate::board::tests::four_layer_board();
        let lines: Vec<String> = Ratsnest::of(&board)
            .lines
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            lines,
            [
                "net GND: D1.1 to the plane",
                "net 3V3: R1.1 to the plane",
                "net net-led.anode: D1.2 to R1.2"
            ]
        );
    }

    #[test]
    fn a_via_on_the_pad_joins_it_to_the_plane() {
        let mut board = crate::board::tests::four_layer_board();
        let r1 = pad_at(&board, "R1", "1");
        board.vias.push(Via {
            net: "3V3".into(),
            at: r1,
            diameter: 0.45,
            drill: 0.15,
            span: None,
        });
        assert!(Ratsnest::of(&board).lines.iter().all(|l| l.net != "3V3"));
    }

    #[test]
    fn a_net_of_one_pad_has_no_lines() {
        let design = Design {
            modules: Vec::new(),
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: vec![Part {
                provides: Vec::new(),
                declared_kind: netsmith_testkit::std_kind("Resistor")
                    .and_then(netsmith_core::PartKind::from_word),
                refdes: "R1".into(),
                kind: "Resistor".into(),
                value: "1k".into(),
                package: Some("0402".into()),
                path: String::new(),
                pins: Vec::new(),
            }],
            nets: vec![Net {
                name: "N".into(),
                pins: vec![PinRef {
                    refdes: "R1".into(),
                    pin: "1".into(),
                }],
            }],
        };
        let board = crate::board::tests::unplaced_board(&design);
        assert!(Ratsnest::of(&board).is_empty());
    }

    #[test]
    fn the_ratsnest_is_the_same_every_time() {
        let board = crate::board::tests::led_board();
        assert_eq!(Ratsnest::of(&board), Ratsnest::of(&board));
    }

    /// The two pads' centres of the LED board's net `net`.
    fn pad_centres(board: &Board, net: &str) -> (Pt, Pt) {
        let n = board.nets.iter().find(|n| n.name == net).unwrap();
        let nodes = nodes_of(&board.parts, n);
        (nodes[0].centre, nodes[1].centre)
    }

    #[test]
    fn a_track_that_ends_on_the_middle_of_another_joins_it() {
        let mut board = crate::board::tests::led_board();
        let (a, b) = pad_centres(&board, "VCC");
        // From the first pad to a point beyond the second, off it, and from the second pad to
        // the middle of that track.
        let off = Pt::new(b.x, b.y + 3.0);
        board
            .tracks
            .push(Track::straight("VCC", CopperLayer::Front, a, off, 0.2));
        let middle = Pt::new((a.x + off.x) / 2.0, (a.y + off.y) / 2.0);
        board
            .tracks
            .push(Track::straight("VCC", CopperLayer::Front, b, middle, 0.2));
        assert!(Ratsnest::of(&board).lines.iter().all(|l| l.net != "VCC"));
        // On the other layer it does not.
        board.tracks[1].layer = CopperLayer::Back;
        assert!(Ratsnest::of(&board).lines.iter().any(|l| l.net == "VCC"));
    }

    #[test]
    fn a_tee_of_two_widths_joins_while_the_round_end_runs_into_the_other_and_an_arcs_edge_counts() {
        // Issue #1542: KiCad 9 joins the end of one track to another's middle when it lies
        // nearer to the other's centre line than the two half widths together, whichever is
        // the wider; a straight track's edge does not count, an arc's does.
        let board = crate::board::tests::led_board();
        let (a, b) = pad_centres(&board, "VCC");
        let off = Pt::new(b.x, b.y + 3.0);
        let middle = Pt::new((a.x + off.x) / 2.0, (a.y + off.y) / 2.0);
        let len = a.distance(off);
        // The normal of the run, towards `b`.
        let mut n = Pt::new(-(off.y - a.y) / len, (off.x - a.x) / len);
        if (b.x - middle.x) * n.x + (b.y - middle.y) * n.y < 0.0 {
            n = Pt::new(-n.x, -n.y);
        }
        let open = |run: Track, at: Pt, towards: Pt, d: f64, end_w: f64| {
            let mut board = board.clone();
            board.tracks.push(run);
            let end = Pt::new(at.x + towards.x * d, at.y + towards.y * d);
            board
                .tracks
                .push(Track::straight("VCC", CopperLayer::Front, b, end, end_w));
            Ratsnest::of(&board).lines.iter().any(|l| l.net == "VCC")
        };
        for (run_w, end_w) in [(0.5, 0.2), (0.2, 0.5)] {
            let run = Track::straight("VCC", CopperLayer::Front, a, off, run_w);
            let reach: f64 = (run_w + end_w) / 2.0;
            for d in [0.0, run_w / 2.0, reach - 0.0005] {
                assert!(
                    !open(run.clone(), middle, n, d, end_w),
                    "{run_w}/{end_w} at {d}"
                );
            }
            for d in [reach, reach + 0.005] {
                assert!(
                    open(run.clone(), middle, n, d, end_w),
                    "{run_w}/{end_w} at {d}"
                );
            }
            // An arc that bows away from `b`: an end that only touches its edge is joined.
            let bow = Pt::new(middle.x - n.x * 0.5, middle.y - n.y * 0.5);
            let arc = Track {
                mid: Some(bow),
                ..Track::straight("VCC", CopperLayer::Front, a, off, run_w)
            };
            let geom = arc.arc().unwrap();
            let centre = Pt::new(geom.centre.x, geom.centre.y);
            assert!(
                b.distance(centre) < geom.radius,
                "`b` lies inside the arc's circle"
            );
            let inward = Pt::new(
                (centre.x - bow.x) / geom.radius,
                (centre.y - bow.y) / geom.radius,
            );
            assert!(
                !open(arc.clone(), bow, inward, reach, end_w),
                "{run_w}/{end_w} arc"
            );
            assert!(
                open(arc, bow, inward, reach + 0.0005, end_w),
                "{run_w}/{end_w} arc"
            );
        }
    }

    #[test]
    fn a_track_whose_end_is_on_a_pads_edge_joins_it_and_one_whose_copper_only_touches_does_not() {
        // Issue #1334: KiCad joins a track's end to a pad nearer than half the track's width.
        let board = crate::board::tests::led_board();
        let (a, b) = pad_centres(&board, "VCC");
        let n = board.nets.iter().find(|n| n.name == "VCC").unwrap();
        let far = nodes_of(&board.parts, n)
            .into_iter()
            .find(|node| node.centre == b)
            .unwrap()
            .bounds;
        // The end of a track from `a`, `beyond` past the edge of `b`'s pad that faces `a`.
        let end = |beyond: f64| {
            if a.x < b.x {
                Pt::new(far.min.x - beyond, b.y)
            } else {
                Pt::new(far.max.x + beyond, b.y)
            }
        };
        let width = 0.2;
        let open = |beyond: f64| {
            let mut board = board.clone();
            board.tracks.push(Track::straight(
                "VCC",
                CopperLayer::Front,
                a,
                end(beyond),
                width,
            ));
            Ratsnest::of(&board).lines.iter().any(|l| l.net == "VCC")
        };
        assert!(!open(-0.3), "an end inside the pad");
        assert!(!open(0.0), "an end on the pad's edge");
        assert!(!open(width / 4.0), "a round end that runs into the pad");
        // KiCad does not join copper that only touches, nor copper that misses.
        assert!(open(width / 2.0), "a round end that only touches the edge");
        assert!(
            open(width / 2.0 + 0.005),
            "a round end 5 um short of the edge"
        );
    }

    #[test]
    fn a_track_that_runs_over_a_pad_of_its_net_joins_it() {
        let mut board = crate::board::tests::led_board();
        let (a, b) = pad_centres(&board, "VCC");
        // From beyond the first pad, through it, to the second: KiCad counts the pad joined,
        // as a hand-routed board has it.
        let before = Pt::new(a.x - (b.x - a.x), a.y - (b.y - a.y));
        board
            .tracks
            .push(Track::straight("VCC", CopperLayer::Front, before, b, 0.2));
        assert!(Ratsnest::of(&board).lines.iter().all(|l| l.net != "VCC"));
        // On the other layer it does not.
        board.tracks[0].layer = CopperLayer::Back;
        assert!(Ratsnest::of(&board).lines.iter().any(|l| l.net == "VCC"));
    }

    #[test]
    fn a_locked_nets_pour_joins_its_pads_on_the_pours_layer() {
        let mut board = crate::board::tests::led_board();
        board.pours.push(BoardPour {
            layer: CopperLayer::Front,
            net: "GND".into(),
            stitch_pitch: None,
            thermal_gap: None,
            spoke_width: None,
            clearance: None,
            min_width: None,
            pads: None,
        });
        // Not locked: the router routes it, as before.
        assert!(Ratsnest::of(&board).lines.iter().any(|l| l.net == "GND"));
        board.fixed.locked.insert("GND".into());
        assert!(Ratsnest::of(&board).lines.iter().all(|l| l.net != "GND"));
        // A pour on the back does not reach the pads on the front.
        board.pours[0].layer = CopperLayer::Back;
        assert!(Ratsnest::of(&board).lines.iter().any(|l| l.net == "GND"));
    }

    fn whole_board_zone(board: &Board, net: &str, layer: &str) -> crate::board::BoardZone {
        let r = board.outline.expanded(-0.3);
        crate::board::BoardZone {
            patch: false,
            net: net.into(),
            layers: vec![layer.into()],
            outline: r.corners().to_vec(),
            holes: Vec::new(),
            priority: 0,
            clearance: 0.2,
            min_width: 0.2,
            pads: crate::board::ZonePads::Thermal,
            thermal_gap: 0.3,
            spoke_width: 0.3,
            islands: crate::board::ZoneIslands::Remove,
            island_min_area: None,
        }
    }

    #[test]
    fn a_zone_joins_the_pads_its_fill_reaches_on_its_layer_locked_or_not() {
        let mut board = crate::board::tests::led_board();
        assert!(Ratsnest::of(&board).lines.iter().any(|l| l.net == "GND"));
        board.zones.push(whole_board_zone(&board, "GND", "F.Cu"));
        let lines = Ratsnest::of(&board).lines;
        assert!(lines.iter().all(|l| l.net != "GND"), "{lines:?}");
        assert!(lines.iter().any(|l| l.net == "VCC"));
        // On the back it reaches the connector's through-hole pad, not the LED's surface pad.
        board.zones[0].layers = vec!["B.Cu".into()];
        assert!(Ratsnest::of(&board).lines.iter().any(|l| l.net == "GND"));
        // A zone that does not join pads keeps its clearance from them.
        board.zones[0].layers = vec!["F.Cu".into()];
        board.zones[0].pads = crate::board::ZonePads::None;
        assert!(Ratsnest::of(&board).lines.iter().any(|l| l.net == "GND"));
    }

    #[test]
    fn a_zoned_layer_takes_no_join_from_the_designs_pour() {
        let mut board = crate::board::tests::led_board();
        board.pours.push(BoardPour {
            layer: CopperLayer::Front,
            net: "GND".into(),
            stitch_pitch: None,
            thermal_gap: None,
            spoke_width: None,
            clearance: None,
            min_width: None,
            pads: None,
        });
        board.fixed.locked.insert("GND".into());
        assert!(Ratsnest::of(&board).lines.iter().all(|l| l.net != "GND"));
        // A VCC zone of the whole front has all of it: the GND pour, drawn under it, has no
        // copper, and GND is left.
        board.zones.push(whole_board_zone(&board, "VCC", "F.Cu"));
        let lines = Ratsnest::of(&board).lines;
        assert!(lines.iter().any(|l| l.net == "GND"), "{lines:?}");
        assert!(lines.iter().all(|l| l.net != "VCC"), "{lines:?}");
    }

    #[test]
    fn a_zone_of_another_net_leaves_the_pour_to_join_its_net_round_it() {
        // Issue #1314: a small VCC zone in a corner of the front, away from every pad, leaves
        // the GND pour round it, and its fill joins GND's pads as it did with no zone.
        let mut board = crate::board::tests::led_board();
        board.pours.push(BoardPour {
            layer: CopperLayer::Front,
            net: "GND".into(),
            stitch_pitch: None,
            thermal_gap: None,
            spoke_width: None,
            clearance: None,
            min_width: None,
            pads: None,
        });
        let o = board.outline;
        let corner = |x: f64, y: f64| o.min + Pt::new(x, y);
        let mut zone = whole_board_zone(&board, "VCC", "F.Cu");
        zone.outline = vec![
            corner(0.5, 0.5),
            corner(2.0, 0.5),
            corner(2.0, 2.0),
            corner(0.5, 2.0),
        ];
        board.zones.push(zone);
        let fills = crate::copper::zone_fills(&board).unwrap();
        let gnd: f64 = fills
            .iter()
            .filter(|f| f.net == "GND" && f.layer == "F.Cu")
            .flat_map(|f| f.areas.iter())
            .map(Area::size)
            .sum();
        assert!(
            gnd > 0.5 * o.width() * o.height(),
            "the GND pour: {gnd} mm²"
        );
        let lines = Ratsnest::of(&board).lines;
        assert!(lines.iter().all(|l| l.net != "GND"), "{lines:?}");
        // A GND zone there instead restates the pour: only the corner is GND's, and GND is left.
        board.zones[0].net = "GND".into();
        assert!(board.replaced("F.Cu", "GND"));
        let lines = Ratsnest::of(&board).lines;
        assert!(lines.iter().any(|l| l.net == "GND"), "{lines:?}");
    }
}
