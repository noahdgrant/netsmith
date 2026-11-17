//! `acid_trap` (issue #838): a sharp notch in the copper of one net, where a track meets a pad
//! or another track at an acute angle. The etchant pools in the narrow wedge outside the angle
//! and etches it longer than the open copper round it, so the notch can eat into the track.
//! netsmith's rule: KiCad has none, and the cross-check counts its findings and does not compare
//! them.
//!
//! **Where it looks.** Three joints, each on one copper layer and of one net:
//!
//! - a track that leaves a pad or via: the angle between the track and the edge where the
//!   track's centre line crosses it. For a straight edge (a rectangle's or a polygon's side, an
//!   oval's straight side, the straight part of a rounded rectangle's side, a custom pad's
//!   straight side) it is the angle to the side. A round edge (a round pad, a via, an oval's end,
//!   a rounded corner, a custom pad's circle or arc) turns away from the track and the wedge
//!   opens within the pad's radius (the routed examples leave round pads and rounded corners a
//!   little off square, at 79 to 86 degrees, where no etchant is trapped), so it is judged only
//!   where the board asks: the process field `min_round_track_angle` (`Extras::round_track_angle`)
//!   is the least angle between the track's centre line and the tangent of the round edge where
//!   the track leaves it (90 along the radius, 0 along the edge). No fabricator publishes
//!   one, and unstated, as every process leaves it, round copper is not judged. A custom pad is
//!   judged at the edge of its anchor and primitives the track leaves: a straight side of its
//!   anchor, of one of its polygons or of one of its lines (a curve is the straight pieces the
//!   board reader cuts it into); its circles and arcs, and the ends of its lines, are round.
//! - a track that ends on the middle of another track (a tee): the angle between the two. A
//!   track that lies along the other overlaps it and is no tee.
//! - tracks that end at one place outside a pad: the least angle between any two of them.
//!   `track_angle` judges these with a stricter limit when the process asks (`fab` asks for the
//!   boards netsmith routes), so then this rule leaves them to it.
//!
//! **The threshold.** An angle under 90 degrees, by more than the degree of tolerance
//! `track_angle` gives a tee, is a trap: "acute" is the whole threshold. It is geometry and the
//! design guides' word for an acid trap, not a fabricator's number: JLCPCB's capabilities page
//! names no acid trap rule and no angle, so no process field holds one. The finding is a warning,
//! as `silk_over_copper` is (ADR 0037), until a real order shows it is a limit; a project file's
//! `rule_severities` may make it an error or ignore it.
//!
//! **What fills the wedge.** A wedge is no trap when other copper of the net fills it: a
//! teardrop, another track, a pad or via, or a pour of the net on the layer (its outline, or the
//! fill the file holds), since the pour's fill closes round a track of its own net. The check
//! looks at one point inside the wedge, just beyond its corner.

use super::Rule;
use super::checks::Out;
use super::model::{Copper, Kind, Model};
use super::shape::{Shape, gap};
use crate::artwork::TrackShape;
use crate::geom::{self, Pt};
use crate::pads::{self, Outline, Prim};

/// The tolerance on a right angle, in degrees: an angle under `90 - TOLERANCE` is acute. The
/// tolerance `track_angle` gives a tee (its 89 degrees).
const TOLERANCE: f64 = 1.0;

/// Two directions nearer than this (degrees) are one: tracks that overlap, not a corner.
const OVERLAP: f64 = 0.5;

/// How near a point must be to a track's centre line, or two ends to each other, to meet (mm).
const MEET: f64 = 0.001;

/// How far into the wedge, past its corner, the check looks for copper that fills it (mm).
const PROBE: f64 = 0.02;

pub(crate) fn run(out: &mut Out<'_>) {
    let m = out.m;
    if m.rules.severity(Rule::AcidTrap).is_none() {
        return;
    }
    let mut found: Vec<Trap> = Vec::new();
    for (k, c) in m.items.iter().enumerate() {
        let Kind::Track(i) = c.kind else { continue };
        let t = &m.art.tracks[i];
        let Some(&layer) = c.layers.first() else {
            continue;
        };
        for (end, way) in ends(&t.shape) {
            if let Some(trap) = leaves_pad(m, k, layer, end, way, t.width) {
                found.push(trap);
            }
            if let Some(trap) = tee(m, k, layer, end, way, t.width) {
                found.push(trap);
            }
        }
    }
    if !m.extras.track_angle {
        found.extend(corners(m));
    }
    found.sort_by(|a, b| {
        (a.at.x, a.at.y, a.track)
            .partial_cmp(&(b.at.x, b.at.y, b.track))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for trap in found {
        let layer = m.layer_name(trap.layer).to_string();
        let mut items = vec![m.items[trap.track].item.clone()];
        items.extend(trap.other.map(|o| m.items[o].item.clone()));
        let message = match trap.round {
            Some(least) => format!(
                "{} at {:.0} degrees to the tangent of its round edge on {layer}, under the {least:.0} degrees `.process.min_round_track_angle` asks, where etchant is trapped; leave it nearer the radius, or fill it (a teardrop)",
                trap.what, trap.angle
            ),
            None => format!(
                "{} at {:.0} degrees on {layer}, an acute angle that traps etchant; make it a right angle or wider, or fill it (a teardrop)",
                trap.what, trap.angle
            ),
        };
        out.push(Rule::AcidTrap, message, trap.at, Some(&layer), items);
    }
}

struct Trap {
    /// What meets what: "a track leaves a pad".
    what: &'static str,
    angle: f64,
    at: Pt,
    layer: usize,
    /// The track, and the pad, via or track it meets (indices into the model's items).
    track: usize,
    other: Option<usize>,
    /// The process's `min_round_track_angle` the angle is under, for a round edge.
    round: Option<f64>,
}

/// Each end of a track and the unit direction in which the track leaves it.
fn ends(shape: &TrackShape) -> Vec<(Pt, Pt)> {
    match shape {
        TrackShape::Line(a, b) => {
            if a.dist(*b) < 1e-6 {
                return Vec::new();
            }
            vec![(*a, unit(*b - *a)), (*b, unit(*a - *b))]
        }
        TrackShape::Arc(arc) => {
            let (first, last) = arc.tangents();
            vec![(arc.start, unit(first)), (arc.end, unit(last))]
        }
    }
}

fn unit(p: Pt) -> Pt {
    let n = p.x.hypot(p.y).max(1e-18);
    p * (1.0 / n)
}

fn dot(a: Pt, b: Pt) -> f64 {
    a.x * b.x + a.y * b.y
}

/// The angle between two unit directions, in degrees.
fn angle(u: Pt, v: Pt) -> f64 {
    dot(u, v).clamp(-1.0, 1.0).acos().to_degrees()
}

fn on_layer(c: &Copper, layer: usize) -> bool {
    c.layers.contains(&layer)
}

fn same_net(c: &Copper, net: &Option<String>) -> bool {
    net.is_some() && c.net == *net
}

fn covers(c: &Copper, p: Pt) -> bool {
    let dot = Shape::disc(p, 0.0);
    c.bounds.expanded(MEET).overlaps(&dot.bounds) && c.shapes.iter().any(|s| gap(&dot, s).d == 0.0)
}

/// Whether copper of the net on the layer other than the track `track` covers `p`: a pad, a
/// via, a teardrop, another track, or a pour.
fn filled(m: &Model<'_>, track: usize, net: &Option<String>, layer: usize, p: Pt) -> bool {
    m.items
        .iter()
        .enumerate()
        .any(|(k, c)| k != track && same_net(c, net) && on_layer(c, layer) && covers(c, p))
        || m.pours.iter().any(|z| {
            z.layer == layer
                && z.net == *net
                && z.polygons.iter().any(|poly| geom::contains(poly, p))
        })
}

/// A track that ends in a pad or via of its net and leaves its copper at an acute angle to its
/// edge.
fn leaves_pad(m: &Model<'_>, k: usize, layer: usize, end: Pt, way: Pt, width: f64) -> Option<Trap> {
    let c = &m.items[k];
    let length = c.length;
    let (pad, shapes) = m.items.iter().enumerate().find_map(|(j, p)| {
        if !matches!(p.kind, Kind::Pad(_) | Kind::Via(_))
            || !same_net(p, &c.net)
            || !on_layer(p, layer)
            || !covers(p, end)
        {
            return None;
        }
        Some((j, &p.shapes))
    })?;
    // Where the centre line leaves the pad (all its shapes: a custom pad is its anchor and its
    // primitives): a step out of the pad, then halving. An arc is followed along its tangent,
    // near enough over the size of a pad.
    let inside = |t: f64| {
        let dot = Shape::disc(end + way * t, 0.0);
        shapes.iter().any(|s| gap(&dot, s).d == 0.0)
    };
    let step = 0.01_f64.min(length / 4.0).max(1e-4);
    let mut t = 0.0;
    while inside(t + step) {
        t += step;
        if t + step > length {
            return None;
        }
    }
    let (mut lo, mut hi) = (t, t + step);
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        if inside(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let exit = end + way * hi;
    // A straight edge makes a notch at any acute angle; a round one (a round pad or via, an
    // oval's end, a rounded corner) turns away from the track, and the wedge opens within the
    // pad's radius, so it is judged only against the angle the board states, if it does.
    let (normal, round) = match edge_normal_at(m, pad, exit + way * 1e-3)? {
        Edge::Straight(normal) => (normal, None),
        Edge::Round(normal) => (normal, Some(m.extras.round_track_angle?)),
    };
    let off = angle(way, normal);
    let acute = 90.0 - off;
    if acute >= round.unwrap_or(90.0 - TOLERANCE) {
        return None;
    }
    // The wedge is on the side the track leans to, between the track's edge and the pad's.
    let along = unit(way - normal * dot(way, normal));
    let corner = exit + along * (width / 2.0 / off.to_radians().cos().max(1e-6));
    let probe = corner + unit(way + along) * PROBE;
    if filled(m, k, &c.net, layer, probe) {
        return None;
    }
    let what = if matches!(m.items[pad].kind, Kind::Via(_)) {
        "a track leaves a via"
    } else {
        "a track leaves a pad"
    };
    Some(Trap {
        what,
        angle: acute,
        at: exit,
        layer,
        track: k,
        other: Some(pad),
        round,
    })
}

/// The edge of a pad or via where a track leaves it, with its outward normal.
enum Edge {
    /// A straight side.
    Straight(Pt),
    /// A round edge: a circle, an arc, an oval's end, a rounded corner. The normal points away
    /// from the centre of the curve.
    Round(Pt),
}

/// The edge of a pad's or via's copper near `p`, a point just outside it, with its outward
/// normal: a side of a rectangle, a polygon or an oval, or the straight part of a rounded
/// rectangle's side; of a custom pad, such a side of its anchor, of one of its polygons or of
/// one of its lines, all `Straight`. A round pad, a via, an oval's end, a rounded corner, the
/// ends of a custom pad's lines, its circles and arcs are `Round`. `None` at a sharp corner of
/// a polygon, where there is no one normal.
fn edge_normal_at(m: &Model<'_>, item: usize, p: Pt) -> Option<Edge> {
    let shapes = &m.items[item].shapes;
    // The shape whose edge the track leaves: the one nearest `p`.
    let nearest = (0..shapes.len()).min_by(|&a, &b| {
        let d = |k: usize| gap(&Shape::disc(p, 0.0), &shapes[k]).d;
        d(a).total_cmp(&d(b))
    })?;
    let Kind::Pad(i) = m.items[item].kind else {
        // A via is a disc.
        return path_edge(&shapes[nearest], p);
    };
    match pads::outline(&m.art.pads[i]) {
        Outline::Custom { anchor, prims } => {
            let anchors = super::model::outline_shapes(&anchor).len();
            if nearest < anchors {
                return outline_edge(&anchor, &shapes[nearest], p);
            }
            match prims.get(nearest - anchors)? {
                Prim::Line { .. } | Prim::Poly { .. } => path_edge(&shapes[nearest], p),
                Prim::Arc { .. } | Prim::Circle { .. } => {
                    path_edge(&shapes[nearest], p).map(|e| match e {
                        Edge::Straight(n) | Edge::Round(n) => Edge::Round(n),
                    })
                }
            }
        }
        outline => outline_edge(&outline, &shapes[nearest], p),
    }
}

/// The edge of a pad's `outline` (drawn as `shape`) near `p`.
fn outline_edge(outline: &Outline, shape: &Shape, p: Pt) -> Option<Edge> {
    match *outline {
        Outline::Rect { .. } | Outline::Poly(_) | Outline::Oval { .. } => path_edge(shape, p),
        Outline::RoundRect { c, w, h, r, angle } => {
            let local = (p - c).rotate(-angle);
            let (corner, straight) = if local.y.abs() <= h / 2.0 - r {
                (Pt::new(local.x.signum(), 0.0), true)
            } else if local.x.abs() <= w / 2.0 - r {
                (Pt::new(0.0, local.y.signum()), true)
            } else if r > MEET {
                // The centre of the corner's circle.
                (
                    Pt::new(
                        local.x.signum() * (w / 2.0 - r),
                        local.y.signum() * (h / 2.0 - r),
                    ),
                    false,
                )
            } else {
                return None;
            };
            Some(if straight {
                Edge::Straight(corner.rotate(angle))
            } else {
                Edge::Round(unit(local - corner).rotate(angle))
            })
        }
        Outline::Circle { c, .. } => Some(Edge::Round(unit(p - c))),
        Outline::Custom { .. } => None,
    }
}

/// The outward edge of a shape near `p`: the normal of a straight piece of its path (grown by
/// the shape's radius, its side is straight too), `Straight`; round, where the shape has a
/// radius, near one of the path's points; `None` at a point of a path with no radius, a sharp
/// corner.
fn path_edge(shape: &Shape, p: Pt) -> Option<Edge> {
    let (near, straight) = nearest_on_path(shape, p);
    if p.dist(near) <= shape.r {
        return None;
    }
    let normal = unit(p - near);
    if straight {
        Some(Edge::Straight(normal))
    } else if shape.r > MEET {
        Some(Edge::Round(normal))
    } else {
        None
    }
}

/// The point of a shape's path (before it is grown by its radius) nearest `p`, and whether it
/// lies inside one of the path's pieces rather than at one of its points.
fn nearest_on_path(shape: &Shape, p: Pt) -> (Pt, bool) {
    let n = shape.pts.len();
    let pieces = match n {
        0 | 1 => 0,
        _ if shape.closed && n > 2 => n,
        _ => n - 1,
    };
    let mut best = (shape.pts.first().copied().unwrap_or(p), false);
    let mut least = best.0.dist(p);
    for i in 0..pieces {
        let (a, b) = (shape.pts[i], shape.pts[(i + 1) % n]);
        let ab = b - a;
        let len2 = dot(ab, ab);
        if len2 < 1e-18 {
            continue;
        }
        let t = (dot(p - a, ab) / len2).clamp(0.0, 1.0);
        let q = a + ab * t;
        let d = q.dist(p);
        if d < least {
            let inner = t * len2.sqrt() > MEET && (1.0 - t) * len2.sqrt() > MEET;
            least = d;
            best = (q, inner);
        }
    }
    best
}

/// A track that ends on the middle of another track of its net at an acute angle to it.
fn tee(m: &Model<'_>, k: usize, layer: usize, end: Pt, way: Pt, width: f64) -> Option<Trap> {
    let c = &m.items[k];
    for (j, o) in m.items.iter().enumerate() {
        let Kind::Track(i) = o.kind else { continue };
        if j == k || !same_net(o, &c.net) || !on_layer(o, layer) {
            continue;
        }
        if !o
            .bounds
            .expanded(MEET)
            .overlaps(&Shape::disc(end, 0.0).bounds)
        {
            continue;
        }
        let other = &m.art.tracks[i];
        let (along, start, stop) = match &other.shape {
            TrackShape::Line(a, b) => (unit(*b - *a), *a, *b),
            TrackShape::Arc(arc) => {
                let r = end - arc.centre;
                (unit(Pt::new(-r.y, r.x)), arc.start, arc.end)
            }
        };
        if end.dist(start) <= MEET || end.dist(stop) <= MEET {
            continue;
        }
        let line = match &other.shape {
            TrackShape::Line(a, b) => Shape::capsule(*a, *b, 0.0),
            TrackShape::Arc(arc) => Shape::stroke(arc.points(), 0.0),
        };
        if gap(&Shape::disc(end, 0.0), &line).d > MEET {
            continue;
        }
        let a = angle(way, along);
        let (acute, side) = if a <= 90.0 {
            (a, along)
        } else {
            (180.0 - a, along * -1.0)
        };
        // A track that runs along the other overlaps it: no corner.
        if acute < OVERLAP || acute >= 90.0 - TOLERANCE {
            continue;
        }
        let half = (acute / 2.0).to_radians().sin().max(1e-6);
        let probe = end + unit(way + side) * (width.max(other.width) / 2.0 / half + PROBE);
        if in_pad(m, &c.net, layer, end) || filled(m, k, &c.net, layer, probe) {
            continue;
        }
        return Some(Trap {
            what: "a track meets another track",
            angle: acute,
            at: end,
            layer,
            track: k,
            other: Some(j),
            round: None,
        });
    }
    None
}

/// Whether `p` is inside a pad or via of the net on the layer, where a joint is no corner.
fn in_pad(m: &Model<'_>, net: &Option<String>, layer: usize, p: Pt) -> bool {
    m.items.iter().any(|c| {
        matches!(c.kind, Kind::Pad(_) | Kind::Via(_))
            && same_net(c, net)
            && on_layer(c, layer)
            && covers(c, p)
    })
}

/// Tracks of a net that end at one place outside its pads, two of them at an acute angle.
fn corners(m: &Model<'_>) -> Vec<Trap> {
    use std::collections::BTreeMap;
    let key = |p: Pt| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
    // By net, layer and place: the place, and each track's direction out of it and width.
    type Ends = BTreeMap<(String, usize, (i64, i64)), (Pt, Vec<(Pt, f64, usize)>)>;
    let mut at: Ends = BTreeMap::new();
    for (k, c) in m.items.iter().enumerate() {
        let Kind::Track(i) = c.kind else { continue };
        let (Some(net), Some(&layer)) = (c.net.clone(), c.layers.first()) else {
            continue;
        };
        let t = &m.art.tracks[i];
        for (end, way) in ends(&t.shape) {
            at.entry((net.clone(), layer, key(end)))
                .or_insert((end, Vec::new()))
                .1
                .push((way, t.width, k));
        }
    }
    let mut found = Vec::new();
    for ((net, layer, _), (place, ways)) in at {
        if ways.len() < 2 {
            continue;
        }
        let net = Some(net);
        let mut least: Option<(f64, usize, usize)> = None;
        for i in 0..ways.len() {
            for j in i + 1..ways.len() {
                let a = angle(ways[i].0, ways[j].0);
                // Two tracks that run out the same way overlap: no corner.
                if a < OVERLAP {
                    continue;
                }
                if least.is_none_or(|(l, _, _)| a < l) {
                    least = Some((a, i, j));
                }
            }
        }
        let Some((a, i, j)) = least else { continue };
        if a >= 90.0 - TOLERANCE || in_pad(m, &net, layer, place) {
            continue;
        }
        let half = (a / 2.0).to_radians().sin().max(1e-6);
        let reach = ways[i].1.max(ways[j].1) / 2.0 / half + PROBE;
        let probe = place + unit(ways[i].0 + ways[j].0) * reach;
        let others = |p: Pt| {
            m.items.iter().enumerate().any(|(k, c)| {
                k != ways[i].2
                    && k != ways[j].2
                    && same_net(c, &net)
                    && on_layer(c, layer)
                    && covers(c, p)
            }) || m.pours.iter().any(|z| {
                z.layer == layer
                    && z.net == net
                    && z.polygons.iter().any(|poly| geom::contains(poly, p))
            })
        };
        if others(probe) {
            continue;
        }
        found.push(Trap {
            what: "two tracks meet",
            angle: a,
            at: place,
            layer,
            track: ways[i].2,
            other: Some(ways[j].2),
            round: None,
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::super::{Extras, Report, Rule, Severity, check};

    /// A 20 mm board with a SIG net, a GND net and `items`.
    fn board(items: &str, extras: &Extras) -> Report {
        let pcb = format!(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (net 0 "") (net 1 "SIG") (net 2 "GND")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            {items})"#
        );
        check(&pcb, None, extras).unwrap()
    }

    fn traps_with(items: &str, extras: &Extras) -> Vec<String> {
        board(items, extras)
            .findings
            .iter()
            .filter(|f| f.rule == Rule::AcidTrap)
            .map(|f| {
                assert_eq!(f.severity, Severity::Warning);
                assert_eq!(f.layer.as_deref(), Some("F.Cu"));
                f.with_items()
            })
            .collect()
    }

    fn traps(items: &str) -> Vec<String> {
        traps_with(items, &Extras::default())
    }

    /// A 1.5 x 1 mm SIG pad at (10, 10).
    const PAD: &str = r#"(footprint "L:R" (layer "F.Cu") (at 10 10)
        (property "Reference" "R1")
        (pad "1" smd rect (at 0 0) (size 1.5 1) (layers "F.Cu") (net 1 "SIG")))"#;

    fn segment(a: (f64, f64), b: (f64, f64), net: u32) -> String {
        format!(
            r#"(segment (start {} {}) (end {} {}) (width 0.2) (layer "F.Cu") (net {net}))"#,
            a.0, a.1, b.0, b.1
        )
    }

    #[test]
    fn a_track_that_leaves_a_pad_square_is_no_trap() {
        // Out of the right side, straight; out of the corner, on the diagonal.
        let square = format!("{PAD} {}", segment((10.0, 10.0), (13.0, 10.0), 1));
        assert_eq!(traps(&square), Vec::<String>::new());
        let diagonal = format!("{PAD} {}", segment((10.75, 10.5), (12.75, 12.5), 1));
        assert_eq!(traps(&diagonal), Vec::<String>::new());
    }

    #[test]
    fn a_track_that_leaves_round_copper_off_its_centre_is_no_trap() {
        // A 1.7 mm round pad and a via, each left by a track 0.3 mm off its centre.
        let round = r#"(footprint "L:J" (layer "F.Cu") (at 10 10)
            (property "Reference" "J1")
            (pad "1" thru_hole circle (at 0 0) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask") (net 1 "SIG")))"#;
        let off = format!("{round} {}", segment((10.0, 10.3), (14.0, 10.3), 1));
        assert_eq!(traps(&off), Vec::<String>::new());
        let via = format!(
            r#"(via (at 5 5) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1)) {}"#,
            segment((5.0, 5.1), (8.0, 5.1), 1)
        );
        assert_eq!(traps(&via), Vec::<String>::new());
        // A rounded rectangle left through its rounded corner, and on the slant through the
        // straight part of its side, which is a trap.
        let rounded = r#"(footprint "L:C" (layer "F.Cu") (at 10 10)
            (property "Reference" "C1")
            (pad "1" smd roundrect (at 0 0) (size 1.5 1) (layers "F.Cu") (roundrect_rratio 0.25) (net 1 "SIG")))"#;
        let corner = format!("{rounded} {}", segment((10.6, 9.7), (14.0, 9.7), 1));
        assert_eq!(traps(&corner), Vec::<String>::new());
        let slant = format!("{rounded} {}", segment((10.0, 10.0), (13.0, 10.6), 1));
        assert_eq!(traps(&slant).len(), 1, "{:?}", traps(&slant));
    }

    /// The extras of a process that states `least` as its `min_round_track_angle`.
    fn round_limit(least: f64) -> Extras {
        Extras {
            round_track_angle: Some(least),
            ..Extras::default()
        }
    }

    /// A track that leaves round copper is judged against the angle the process states, between
    /// the track and the tangent of the edge where it leaves (90 along the radius), and not at all
    /// where none is stated.
    #[test]
    fn a_track_that_leaves_round_copper_is_judged_by_the_angle_the_process_states() {
        // (copper, the track, its angle to the edge's tangent): a track 0.3 mm off the centre of
        // a round pad of radius 0.85 leaves at 90 - asin(0.3 / 0.85) = 69.3 degrees.
        let round = r#"(footprint "L:J" (layer "F.Cu") (at 10 10)
            (property "Reference" "J1")
            (pad "1" thru_hole circle (at 0 0) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask") (net 1 "SIG")))"#;
        let via = r#"(via (at 10 10) (size 1.7) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))"#;
        // An oval's end, a radius of 0.5 mm: 0.35 mm off the axis is 90 - asin(0.7) = 45.6.
        let oval = r#"(footprint "L:O" (layer "F.Cu") (at 10 10)
            (property "Reference" "O1")
            (pad "1" smd oval (at 0 0) (size 2 1) (layers "F.Cu") (net 1 "SIG")))"#;
        // A rounded corner of radius 0.25 mm, the track 0.05 mm off the corner's centre: 78.5.
        let corner = r#"(footprint "L:C" (layer "F.Cu") (at 10 10)
            (property "Reference" "C1")
            (pad "1" smd roundrect (at 0 0) (size 1.5 1) (layers "F.Cu") (roundrect_rratio 0.25) (net 1 "SIG")))"#;
        for (copper, from, to, angle) in [
            (round, (10.0, 10.3), (14.0, 10.3), 69.3),
            (via, (10.0, 10.3), (14.0, 10.3), 69.3),
            (oval, (10.0, 10.35), (14.0, 10.35), 45.6),
            (corner, (10.6, 9.7), (14.0, 9.7), 78.5),
            // The custom pad's circle, a radius of 0.6 mm: 0.3 mm off its centre, 60 degrees.
            (CUSTOM, (9.0, 10.3), (5.0, 10.3), 60.0),
        ] {
            let items = format!("{copper} {}", segment(from, to, 1));
            assert_eq!(traps(&items), Vec::<String>::new(), "unstated: {items}");
            let under = traps_with(&items, &round_limit(angle + 1.0));
            assert_eq!(under.len(), 1, "{angle}: {under:?} for {items}");
            assert!(
                under[0].contains("to the tangent of its round edge on F.Cu")
                    && under[0].contains("min_round_track_angle"),
                "{}",
                under[0]
            );
            assert!(
                under[0].contains(&format!("at {angle:.0} degrees")),
                "{}",
                under[0]
            );
            let over = traps_with(&items, &round_limit(angle - 1.0));
            assert_eq!(over, Vec::<String>::new(), "{angle}: {items}");
        }
    }

    #[test]
    fn a_track_that_leaves_round_copper_along_its_radius_is_no_trap() {
        let round = r#"(footprint "L:J" (layer "F.Cu") (at 10 10)
            (property "Reference" "J1")
            (pad "1" thru_hole circle (at 0 0) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask") (net 1 "SIG")))"#;
        let radial = format!("{round} {}", segment((10.0, 10.0), (14.0, 10.0), 1));
        // Even a limit of 90, the most, only asks for the radius itself.
        assert_eq!(
            traps_with(&radial, &round_limit(89.0)),
            Vec::<String>::new()
        );
        // The straight edges are judged as before, with or without a limit.
        let square = format!("{PAD} {}", segment((10.0, 10.0), (13.0, 10.0), 1));
        assert_eq!(
            traps_with(&square, &round_limit(89.0)),
            Vec::<String>::new()
        );
    }

    #[test]
    fn copper_that_fills_the_wedge_at_round_copper_takes_the_trap_away() {
        let round = r#"(footprint "L:J" (layer "F.Cu") (at 10 10)
            (property "Reference" "J1")
            (pad "1" thru_hole circle (at 0 0) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask") (net 1 "SIG")))"#;
        let track = segment((10.0, 10.7), (14.0, 10.7), 1);
        let pour = r#"(zone (net 1) (net_name "SIG") (layer "F.Cu") (connect_pads (clearance 0.3)) (min_thickness 0.25)
            (fill yes) (polygon (pts (xy 5 5) (xy 15 5) (xy 15 15) (xy 5 15))))"#;
        let bare = format!("{round} {track}");
        assert_eq!(traps_with(&bare, &round_limit(80.0)).len(), 1);
        assert_eq!(
            traps_with(&format!("{bare} {pour}"), &round_limit(80.0)),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_track_that_leaves_a_pad_side_on_the_slant_is_an_acid_trap() {
        // From the centre at 30 degrees off the side's normal: 60 degrees to the side.
        let (dx, dy) = (
            3.0 * 30f64.to_radians().cos(),
            3.0 * 30f64.to_radians().sin(),
        );
        let slant = format!("{PAD} {}", segment((10.0, 10.0), (10.0 + dx, 10.0 + dy), 1));
        let found = traps(&slant);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("a track leaves a pad at 60 degrees on F.Cu"),
            "{}",
            found[0]
        );
        assert!(found[0].contains("SMD pad 1 [SIG] of R1"), "{}", found[0]);
    }

    #[test]
    fn a_teardrop_or_a_pour_that_fills_the_wedge_takes_the_trap_away() {
        let (dx, dy) = (
            3.0 * 30f64.to_radians().cos(),
            3.0 * 30f64.to_radians().sin(),
        );
        let track = segment((10.0, 10.0), (10.0 + dx, 10.0 + dy), 1);
        let pour = r#"(zone (net 1) (net_name "SIG") (layer "F.Cu") (connect_pads (clearance 0.3)) (min_thickness 0.25)
            (fill yes) (polygon (pts (xy 5 5) (xy 15 5) (xy 15 15) (xy 5 15))))"#;
        assert_eq!(
            traps(&format!("{PAD} {track} {pour}")),
            Vec::<String>::new()
        );
    }

    /// A custom SIG pad at (10, 10): a 0.6 mm round anchor, a 2 x 1 mm polygon to the right of
    /// it and a 1.2 mm circle to the left.
    const CUSTOM: &str = r#"(footprint "L:U" (layer "F.Cu") (at 10 10)
        (property "Reference" "U1")
        (pad "1" smd custom (at 0 0) (size 0.6 0.6) (layers "F.Cu") (net 1 "SIG")
          (options (anchor circle))
          (primitives
            (gr_poly (pts (xy 0 -0.5) (xy 2 -0.5) (xy 2 0.5) (xy 0 0.5)) (width 0) (fill yes))
            (gr_circle (center -1 0) (end -0.4 0) (width 0) (fill yes)))))"#;

    #[test]
    fn a_track_that_leaves_a_custom_pads_straight_side_on_the_slant_is_an_acid_trap() {
        // Out of the polygon's right side, square: no trap.
        let square = format!("{CUSTOM} {}", segment((11.0, 10.0), (14.0, 10.0), 1));
        assert_eq!(traps(&square), Vec::<String>::new());
        // Out of the polygon's right side at 60 degrees to it.
        let (dx, dy) = (
            3.0 * 30f64.to_radians().cos(),
            3.0 * 30f64.to_radians().sin(),
        );
        let slant = format!(
            "{CUSTOM} {}",
            segment((11.5, 10.0), (11.5 + dx, 10.0 + dy), 1)
        );
        let found = traps(&slant);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("a track leaves a pad at 60 degrees on F.Cu")
                && found[0].contains("SMD pad 1 [SIG] of U1"),
            "{}",
            found[0]
        );
        // Out of the circle, 0.3 mm off its centre: a round edge, no trap.
        let round = format!("{CUSTOM} {}", segment((9.0, 10.3), (5.0, 10.3), 1));
        assert_eq!(traps(&round), Vec::<String>::new());
    }

    #[test]
    fn a_tee_at_an_acute_angle_is_an_acid_trap_and_a_right_tee_is_not() {
        let run = segment((5.0, 10.0), (15.0, 10.0), 1);
        let right = segment((10.0, 10.0), (10.0, 13.0), 1);
        assert_eq!(traps(&format!("{run} {right}")), Vec::<String>::new());
        // A track that ends on the run and lies along it overlaps it: no corner.
        let along = segment((10.0, 10.0), (12.0, 10.0), 1);
        assert_eq!(traps(&format!("{run} {along}")), Vec::<String>::new());
        let slant = segment((10.0, 10.0), (13.0, 13.0), 1);
        let found = traps(&format!("{run} {slant}"));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("a track meets another track at 45 degrees on F.Cu"),
            "{}",
            found[0]
        );
    }

    #[test]
    fn tracks_that_end_at_an_acute_angle_are_an_acid_trap_unless_track_angle_judges_them() {
        let v = format!(
            "{} {}",
            segment((10.0, 10.0), (14.0, 10.0), 1),
            segment((10.0, 10.0), (13.0, 7.0), 1)
        );
        let found = traps(&v);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("two tracks meet at 45 degrees"),
            "{}",
            found[0]
        );
        let strict = Extras {
            track_angle: true,
            ..Extras::default()
        };
        assert_eq!(traps_with(&v, &strict), Vec::<String>::new());
        // A right angle is no trap.
        let l = format!(
            "{} {}",
            segment((10.0, 10.0), (14.0, 10.0), 1),
            segment((10.0, 10.0), (10.0, 6.0), 1)
        );
        assert_eq!(traps(&l), Vec::<String>::new());
    }

    #[test]
    fn a_project_file_can_make_an_acid_trap_an_error_or_ignore_it() {
        let run = segment((5.0, 10.0), (15.0, 10.0), 1);
        let slant = segment((10.0, 10.0), (13.0, 13.0), 1);
        let pcb = format!(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (net 0 "") (net 1 "SIG")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            {run} {slant})"#
        );
        let pro = |s: &str| {
            format!(
                r#"{{"board": {{"design_settings": {{"rule_severities": {{"acid_trap": "{s}"}}}}}}}}"#
            )
        };
        let error = check(&pcb, Some(&pro("error")), &Extras::default()).unwrap();
        assert_eq!(
            error
                .findings
                .iter()
                .filter(|f| f.rule == Rule::AcidTrap && f.severity == Severity::Error)
                .count(),
            1
        );
        let ignored = check(&pcb, Some(&pro("ignore")), &Extras::default()).unwrap();
        assert!(ignored.findings.iter().all(|f| f.rule != Rule::AcidTrap));
    }
}
