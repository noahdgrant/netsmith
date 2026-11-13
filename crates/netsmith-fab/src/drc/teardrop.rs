//! Teardrops (issue #431, S11): copper that widens a track where it meets a pad or a via, so
//! that a drill that wanders or an etch that undercuts the joint does not cut the track off.
//!
//! **What is drawn.** For each track that ends inside a pad or a via of its net, the place where
//! the track leaves the copper (its exit) is found by following the track and the tracks that
//! carry it on, and a four-cornered polygon is drawn there: two corners inside the pad, `width`
//! apart across the track, and two on the track's edges `length` beyond the exit. The width is
//! [`WIDTH_RATIO`] (90 %) of the pad's or via's copper across the track, at most [`MAX_WIDTH`];
//! the length is [`LENGTH_RATIO`] (half) of it, at most [`MAX_LENGTH`] and no longer than the
//! straight run of track beyond the exit. These are KiCad's own defaults for a round pad
//! (`best_length_ratio` 0.5, `max_length` 1 mm, `max_width` 2 mm), with the width a little under
//! KiCad's 100 % so the corners stay inside the copper. The sides are straight (KiCad's default,
//! no curve points). A teardrop no wider than 1.2 times the track adds nothing and is not drawn.
//!
//! **Only where it fits.** A teardrop is kept only if it keeps every space the check
//! ([`super::check_artwork`]) asks of a track: the clearance to copper of other nets (the larger
//! of the two nets' classes, a pad's own), the hole clearance and the process's hole-to-track
//! spaces to holes not of its net, the copper-to-edge space, the space from a track to a solder
//! mask opening of another net, and no rule area that forbids tracks. A teardrop on a
//! surface-mount pad also keeps the process's space between surface-mount pads from the pads of
//! other nets, so that it does not close the gap of a fine-pitch part. Each rule is met with
//! [`MARGIN`] to spare, and the teardrops already kept count as copper. A track whose exit lies
//! inside another pad or via of its net (a via in a pad) gets none there.
//!
//! **Why netsmith draws them, and how they are written.** KiCad 9 keeps teardrop settings on pads
//! and vias, but its teardrops are copper only once its own filler has made them as zones, and
//! `build`, `fab` and the editor need no KiCad (ADR 0097): netsmith's Gerber writer and its check
//! would have to make KiCad's copper themselves. So netsmith draws the polygon, here, with the
//! check's own geometry, and the board file holds it in KiCad 9's form for a teardrop: a zone of
//! the track's net on the track's layer, marked `(attr (teardrop (type padvia)))`, with the
//! polygon as its outline and as its fill. The Gerber writer draws the fill as copper, a pour of
//! another net keeps its clearance from it, and the check sees it as copper of its net. KiCad
//! reads the same zone as a teardrop. (Decided by the implementing agent under the owner's
//! delegation, 2026-11-13.)
//!
//! The same board gives the same teardrops: the pads and vias are taken in the file's order, and
//! the tracks in theirs.

use std::collections::HashMap;

use super::model::{Copper, Kind, Model};
use super::shape::{Shape, gap, touches_area};
use super::{Extras, Rules};
use crate::artwork::{Artwork, PadKind, TrackShape};
use crate::geom::{Bounds, Pt};

/// The length of a teardrop beyond the pad, as a share of the pad's copper across the track.
pub const LENGTH_RATIO: f64 = 0.5;
/// The longest teardrop, in mm.
pub const MAX_LENGTH: f64 = 1.0;
/// The width of a teardrop at the pad, as a share of the pad's copper across the track.
pub const WIDTH_RATIO: f64 = 0.9;
/// The widest teardrop, in mm.
pub const MAX_WIDTH: f64 = 2.0;
/// How much more than each rule's space a teardrop keeps, in mm.
pub const MARGIN: f64 = 0.001;
/// How far inside the pad's copper a teardrop's corners are, in mm.
const INSIDE: f64 = 0.005;

/// A teardrop: a polygon of copper on a layer, on a net.
#[derive(Debug, Clone, PartialEq)]
pub struct Teardrop {
    pub net: String,
    /// The copper layer, KiCad's name (`F.Cu`).
    pub layer: String,
    pub polygon: Vec<Pt>,
}

/// How far inside the shapes a point is: the distance to the nearest edge, negative outside.
fn depth(shapes: &[Shape], p: Pt) -> f64 {
    let dot = Shape::disc(p, 0.0);
    shapes
        .iter()
        .map(|s| {
            if s.filled && s.pts.len() >= 3 {
                let edge = gap(&Shape::ring(s.pts.clone(), 0.0), &dot).d;
                if crate::geom::contains(&s.pts, p) {
                    edge + s.r
                } else {
                    s.r - edge
                }
            } else {
                s.r - gap(&Shape::stroke(s.pts.clone(), 0.0), &dot).d
            }
        })
        .fold(f64::MIN, f64::max)
}

fn unit(p: Pt) -> Pt {
    let n = p.x.hypot(p.y);
    if n < 1e-12 { p } else { p * (1.0 / n) }
}

fn key(p: Pt) -> (i64, i64) {
    ((p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64)
}

/// The width of the copper across the direction `v` through `c`: how far it reaches each way.
fn across(shapes: &[Shape], c: Pt, v: Pt) -> f64 {
    let reach = |dir: Pt| {
        let (mut lo, mut hi) = (0.0, 0.0);
        let mut step = 0.05;
        while depth(shapes, c + dir * (hi + step)) >= 0.0 && hi < 20.0 {
            hi += step;
            step *= 2.0;
        }
        hi += step;
        for _ in 0..30 {
            let mid = (lo + hi) / 2.0;
            if depth(shapes, c + dir * mid) >= 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    };
    reach(v) + reach(v * -1.0)
}

/// What the planner checks a teardrop against.
struct Fit<'m, 'a> {
    m: &'m Model<'a>,
    /// The solder mask openings of the pads of each face, with the pad's net.
    openings: [Vec<(Option<String>, Vec<Shape>, Bounds)>; 2],
    /// The teardrops kept so far, with their net, layer and shape.
    kept: Vec<(String, usize, Shape)>,
}

impl Fit<'_, '_> {
    /// Whether a teardrop of `net` on `layer` for `owner` keeps every space.
    fn fits(&self, poly: &Shape, net: &str, layer: usize, owner: &Copper) -> bool {
        let m = self.m;
        let r = m.rules;
        let x = m.extras;
        let mine = Some(net.to_string());
        let near = |b: &Bounds, d: f64| poly.bounds.expanded(d + MARGIN).overlaps(b);
        let smd_owner = matches!(owner.kind, Kind::Pad(p)
            if matches!(m.art.pads[p].kind, PadKind::Smd | PadKind::Connect));
        let own_class = r.net_clearance(Some(net));
        let reach = m.max_gap() + x.pad_to_pad.unwrap_or(0.0) + x.mask_to_track.unwrap_or(0.0);
        for c in &m.items {
            if c.net == mine || !c.layers.contains(&layer) || !near(&c.bounds, reach) {
                continue;
            }
            let mut need = own_class
                .max(r.net_clearance(c.net.as_deref()))
                .max(c.own_clearance);
            if smd_owner
                && let Kind::Pad(p) = c.kind
                && matches!(m.art.pads[p].kind, PadKind::Smd | PadKind::Connect)
            {
                need = need.max(x.pad_to_pad.unwrap_or(0.0));
            }
            if c.shapes.iter().any(|s| gap(poly, s).d < need + MARGIN) {
                return false;
            }
        }
        for (n, l, s) in &self.kept {
            if n != net && *l == layer {
                let need = own_class.max(r.net_clearance(Some(n)));
                if near(&s.bounds, need) && gap(poly, s).d < need + MARGIN {
                    return false;
                }
            }
        }
        for h in &m.holes {
            if h.item.net == mine || !near(&h.shape.bounds, reach) {
                continue;
            }
            let plated_pad = h.plated
                && h.owner
                    .is_some_and(|o| matches!(m.items[o].kind, Kind::Pad(_)));
            let to_track = if plated_pad {
                x.plated_hole_clearance
            } else {
                x.hole_to_track
            };
            let need = r.hole_clearance.max(to_track.unwrap_or(0.0));
            if gap(poly, &h.shape).d < need + MARGIN {
                return false;
            }
        }
        for (s, _) in &m.edge {
            if near(&s.bounds, r.copper_edge) && gap(poly, s).d < r.copper_edge + MARGIN {
                return false;
            }
        }
        let last = m.layers.len().saturating_sub(1);
        let face = if layer == 0 {
            Some(0)
        } else if layer == last {
            Some(1)
        } else {
            None
        };
        if let Some(face) = face {
            let need = x.mask_to_track.unwrap_or(0.0);
            for (n, shapes, b) in &self.openings[face] {
                if n.as_deref() == Some(net) || !near(b, need) {
                    continue;
                }
                if shapes.iter().any(|s| gap(poly, s).d < need + MARGIN) {
                    return false;
                }
            }
        }
        let layer_name = &m.layers[layer];
        for z in m.art.zones.iter().filter(|z| {
            z.owner.is_empty()
                && z.keepout.as_ref().is_some_and(|k| k.tracks)
                && z.layers.iter().any(|l| l == layer_name)
        }) {
            if z.outlines
                .iter()
                .any(|o| o.len() >= 3 && touches_area(poly, o))
            {
                return false;
            }
        }
        true
    }
}

/// The teardrops a board can have, for the board as the check reads it, to its rules.
pub fn teardrops(art: &Artwork, rules: &Rules, extras: &Extras) -> Vec<Teardrop> {
    let m = Model::new(art, rules, extras, false);
    let opening = |front: bool| {
        art.pads
            .iter()
            .filter_map(|p| {
                let s = m.mask_opening(p, front);
                (!s.is_empty()).then(|| (p.net.clone(), s.clone(), Model::bounds_of(&s)))
            })
            .collect::<Vec<_>>()
    };
    let mut fit = Fit {
        m: &m,
        openings: [opening(true), opening(false)],
        kept: Vec::new(),
    };
    // The straight tracks that end at each place, by net and layer.
    let mut ends: HashMap<(&str, &str, (i64, i64)), Vec<usize>> = HashMap::new();
    for (i, t) in art.tracks.iter().enumerate() {
        let (Some(net), TrackShape::Line(a, b)) = (t.net.as_deref(), &t.shape) else {
            continue;
        };
        if a.dist(*b) < 1e-6 {
            continue;
        }
        for p in [a, b] {
            ends.entry((net, t.layer.as_str(), key(*p)))
                .or_default()
                .push(i);
        }
    }
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<(usize, usize, (i64, i64))> = Default::default();
    for (k, c) in m.items.iter().enumerate() {
        if !matches!(c.kind, Kind::Pad(_) | Kind::Via(_)) {
            continue;
        }
        let Some(net) = c.net.as_deref() else {
            continue;
        };
        for &layer in &c.layers {
            let layer_name = m.layers[layer].as_str();
            for (i, t) in art.tracks.iter().enumerate() {
                if t.layer != layer_name || t.net.as_deref() != Some(net) {
                    continue;
                }
                let TrackShape::Line(a, b) = t.shape else {
                    continue;
                };
                if !c
                    .bounds
                    .expanded(1e-6)
                    .overlaps(&Bounds::of([a, b]).unwrap())
                {
                    continue;
                }
                for (start, other) in [(a, b), (b, a)] {
                    if depth(&c.shapes, start) < 0.0 || depth(&c.shapes, other) >= 0.0 {
                        continue;
                    }
                    let Some(d) = plan_one(&m, &ends, c, k, net, layer, i, start) else {
                        continue;
                    };
                    if !seen.insert((k, layer, key(d.exit))) {
                        continue;
                    }
                    let poly = Shape::polygon(d.polygon.clone(), 0.0);
                    if fit.fits(&poly, net, layer, c) {
                        fit.kept.push((net.to_string(), layer, poly));
                        out.push(Teardrop {
                            net: net.to_string(),
                            layer: layer_name.to_string(),
                            polygon: d.polygon,
                        });
                    }
                }
            }
        }
    }
    out
}

struct Planned {
    exit: Pt,
    polygon: Vec<Pt>,
}

/// The teardrop of the track `track` that starts at `start` inside the copper `c`, before it
/// is checked against anything else.
#[allow(clippy::too_many_arguments)]
fn plan_one(
    m: &Model<'_>,
    ends: &HashMap<(&str, &str, (i64, i64)), Vec<usize>>,
    c: &Copper,
    k: usize,
    net: &str,
    layer: usize,
    track: usize,
    start: Pt,
) -> Option<Planned> {
    let art = m.art;
    let layer_name = m.layers[layer].as_str();
    let w = art.tracks[track].width;
    // The track and the straight tracks that carry it on, with no branch, as a path.
    let mut path = vec![start];
    let (mut at, mut prev) = (start, track);
    for _ in 0..32 {
        let TrackShape::Line(a, b) = art.tracks[prev].shape else {
            break;
        };
        let next = if key(a) == key(at) { b } else { a };
        path.push(next);
        at = next;
        let run: f64 = path.windows(2).map(|s| s[0].dist(s[1])).sum();
        if run > 2.0 * MAX_LENGTH + 10.0 {
            break;
        }
        let on: Vec<usize> = ends
            .get(&(net, layer_name, key(at)))
            .map(|v| v.iter().copied().filter(|&j| j != prev).collect())
            .unwrap_or_default();
        match on[..] {
            [j] if (art.tracks[j].width - w).abs() < 1e-6 => prev = j,
            _ => break,
        }
    }
    // Where the path leaves the copper.
    let seg = (0..path.len() - 1).find(|&s| depth(&c.shapes, path[s + 1]) < 0.0)?;
    let (p, q) = (path[seg], path[seg + 1]);
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if depth(&c.shapes, p + (q - p) * mid) >= 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let exit = p + (q - p) * hi;
    let u = unit(q - p);
    let v = Pt::new(-u.y, u.x);
    // The exit inside other copper of the net: a via in a pad, a pad over a pad.
    let inside_other = m.items.iter().enumerate().any(|(j, o)| {
        j != k
            && matches!(o.kind, Kind::Pad(_) | Kind::Via(_))
            && o.net.as_deref() == Some(net)
            && o.layers.contains(&layer)
            && depth(&o.shapes, exit) >= 0.0
    });
    if inside_other {
        return None;
    }
    // The straight run beyond the exit.
    let mut run = exit.dist(q);
    for s in seg + 1..path.len() - 1 {
        let d = unit(path[s + 1] - path[s]);
        if (d.x - u.x).abs() > 1e-6 || (d.y - u.y).abs() > 1e-6 {
            break;
        }
        run += path[s].dist(path[s + 1]);
    }
    let size = across(&c.shapes, c.item.at, v);
    let width = (WIDTH_RATIO * size).min(MAX_WIDTH);
    if width < 1.2 * w {
        return None;
    }
    let length = (LENGTH_RATIO * size).min(MAX_LENGTH).min(run);
    if length < w.max(0.1) {
        return None;
    }
    // The corners inside the copper: as near the exit as both lie inside.
    let half = width / 2.0;
    let base = (0..=400).map(|i| i as f64 * 0.005).find(|&d| {
        let b = exit - u * d;
        depth(&c.shapes, b + v * half) >= INSIDE
            && depth(&c.shapes, b - v * half) >= INSIDE
            && depth(&c.shapes, b) >= INSIDE
    })?;
    let b = exit - u * base;
    let t = exit + u * length;
    let polygon = vec![
        b + v * half,
        t + v * (w / 2.0),
        t - v * (w / 2.0),
        b - v * half,
    ];
    Some(Planned { exit, polygon })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(extra: &str) -> Artwork {
        let src = format!(
            r#"(kicad_pcb (version 20241229) (generator "test")
  (layers (0 "F.Cu" signal) (31 "B.Cu" signal))
  (net 0 "") (net 1 "A") (net 2 "B")
  (gr_rect (start 0 0) (end 20 20) (layer "Edge.Cuts") (width 0.05))
  (via (at 10 10) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
  (segment (start 10 10) (end 14 10) (width 0.2) (layer "F.Cu") (net 1))
  {extra})"#
        );
        Artwork::parse(&src).unwrap()
    }

    fn rules() -> Rules {
        Rules {
            default_clearance: 0.15,
            copper_edge: 0.3,
            ..Rules::default()
        }
    }

    #[test]
    fn a_track_into_a_via_gets_a_teardrop_of_the_via_s_size_on_its_layer_and_net() {
        let drops = teardrops(&board(""), &rules(), &Extras::default());
        assert_eq!(drops.len(), 1, "{drops:?}");
        let d = &drops[0];
        assert_eq!((d.net.as_str(), d.layer.as_str()), ("A", "F.Cu"));
        let b = Bounds::of(d.polygon.iter().copied()).unwrap();
        // 90 % of the via's 0.6 mm across, and half of it beyond the via's edge at 10.3.
        assert!((b.max.y - b.min.y - 0.54).abs() < 1e-3, "{b:?}");
        assert!((b.max.x - 10.6).abs() < 1e-3, "{b:?}");
        assert!(b.min.x > 10.0 && b.min.x < 10.3, "{b:?}");
    }

    #[test]
    fn a_teardrop_that_would_come_too_near_another_net_s_pad_is_not_drawn() {
        let pad = r#"(footprint "x" (layer "F.Cu") (at 10.45 10.42)
            (pad "1" smd rect (at 0 0) (size 0.2 0.2) (layers "F.Cu") (net 2 "B")))"#;
        let art = board(pad);
        assert!(teardrops(&art, &rules(), &Extras::default()).is_empty());
        // The same pad farther off leaves room.
        let far = pad.replace("10.42", "10.8");
        assert_eq!(
            teardrops(&board(&far), &rules(), &Extras::default()).len(),
            1
        );
    }

    #[test]
    fn a_board_with_teardrops_passes_the_check() {
        let art = board("");
        let drops = teardrops(&art, &rules(), &Extras::default());
        let zones: String = drops
            .iter()
            .map(|d| {
                let pts: String = d
                    .polygon
                    .iter()
                    .map(|p| format!("(xy {} {})", p.x, p.y))
                    .collect();
                format!(
                    r#"(zone (net 1) (net_name "A") (layer "{}") (attr (teardrop (type padvia)))
                      (connect_pads yes (clearance 0)) (polygon (pts {pts}))
                      (filled_polygon (layer "{}") (pts {pts})))"#,
                    d.layer, d.layer
                )
            })
            .collect();
        let with = board(&zones);
        assert!(with.zones[0].teardrop);
        let report = super::super::check_artwork(&with, &rules(), &Extras::default(), false);
        let errors: Vec<_> = report
            .errors()
            .filter(|f| f.rule != super::super::Rule::TrackDangling)
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
        // And the check sees the teardrop's copper: a track of another net over it is a short.
        let short = format!(
            "{zones}(segment (start 10.45 9.5) (end 10.45 10.5) (width 0.1) (layer \"F.Cu\") (net 2))"
        );
        let report =
            super::super::check_artwork(&board(&short), &rules(), &Extras::default(), false);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.items.iter().any(|i| i.what.starts_with("Teardrop"))),
            "{:?}",
            report.findings
        );
    }
}
