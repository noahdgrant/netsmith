//! The rules of a fabricator's capabilities page that a KiCad project file cannot say, which
//! the caller passes in [`Extras`](super::Extras) from the board's process (issue #431). Each is
//! netsmith's own: the cross-check counts its findings and does not compare them. A rule the
//! caller does not pass is not checked, so a board read with no process (a port, the `drc`
//! example) is checked as KiCad would check it.
//!
//! **The hole rules are to tracks.** The page writes "via hole to track", "NPTH to track" and
//! "PTH to track", and a footprint's own pads may sit nearer its holes: the USB-C receptacle of
//! the Easyduino has GND pads 0.175 mm from its non-plated pegs, its maker's land pattern.
//! KiCad's one `min_hole_clearance` would hold pads to the figure too, so the project file keeps
//! the copper clearance there and `hole_to_track` measures tracks only. Two holes are 0.45 mm
//! apart when one is a pad's ("pad hole-to-hole") and 0.2 mm when both are vias' ("via
//! hole-to-hole"); the project file's `min_hole_to_hole` is the vias' figure, so KiCad's check
//! sees it, and `pad_hole_to_hole` adds the pad figure.
//!
//! **Rejected:** a reader of a fabricator's KiCad rules file (`.kicad_dru`): the owner chose the
//! std processes (2026-11-11), and JLCPCB publishes none. One hole clearance for every hole and
//! every copper, as KiCad's single number invites: it refuses land patterns the page does not,
//! or, at 0.28 mm, makes every via wider in effect.

use super::checks::{EPS, Out, ring_width, same_net};
use super::model::{Kind, Model};
use super::shape::{Shape, gap, near_pairs};
use super::{Item, Rule};
use crate::artwork::{Pad, PadKind};
use crate::geom::Bounds;

pub(crate) fn run(out: &mut Out<'_>) {
    hole_to_track(out);
    pad_to_pad(out);
    mask_to_track(out);
    holes(out);
    pad_hole_to_hole(out);
    track_current(out);
}

/// A track narrower than its net's current needs, unless it lies within the neck-down reach
/// of one of the net's pads.
fn track_current(out: &mut Out<'_>) {
    let m = out.m;
    let x = m.extras;
    if x.least_widths.is_empty() {
        return;
    }
    for c in &m.items {
        let Kind::Track(i) = c.kind else { continue };
        let t = &m.art.tracks[i];
        let Some(net) = t.net.as_deref() else {
            continue;
        };
        let Some(&(_, least)) = x.least_widths.iter().find(|(n, _)| n == net) else {
            continue;
        };
        if t.width >= least - EPS / 100.0 {
            continue;
        }
        // Near the pads of a part that has a pad on the net: within the reach of the box
        // round the centres of that part's pads.
        let parts: Vec<&str> = m
            .art
            .pads
            .iter()
            .filter(|p| p.net.as_deref() == Some(net))
            .map(|p| p.part.as_str())
            .collect();
        let near_pad = |p: crate::geom::Pt| {
            parts.iter().any(|&part| {
                let Some(b) =
                    Bounds::of(m.art.pads.iter().filter(|q| q.part == part).map(|q| q.at))
                else {
                    return false;
                };
                let b = b.expanded(x.neck_reach + EPS);
                b.min.x <= p.x && p.x <= b.max.x && b.min.y <= p.y && p.y <= b.max.y
            })
        };
        // Every point of the track near one of the net's pads: sampled each 0.05 mm.
        let (a, b) = (c.anchors[0], c.anchors[c.anchors.len() - 1]);
        let steps = ((a.dist(b) / 0.05).ceil() as usize).max(1);
        if (0..=steps).all(|k| near_pad(a + (b - a) * (k as f64 / steps as f64))) {
            continue;
        }
        out.push(
            Rule::TrackCurrent,
            format!(
                "{} is {:.4} mm wide; its net's current needs {:.4} mm (IPC-2221 at the fabricator's width tolerance), and only within {:.2} mm of the pads of a part on the net may a track be narrower",
                c.item.what, t.width, least, x.neck_reach
            ),
            c.item.at,
            Some(&t.layer),
            vec![c.item.clone()],
        );
    }
}

/// Two holes, at least one of them a pad's, nearer than the process allows between pad holes.
fn pad_hole_to_hole(out: &mut Out<'_>) {
    let m = out.m;
    let Some(least_gap) = m.extras.pad_hole_to_hole else {
        return;
    };
    let is_pad = |h: &super::model::Hole| {
        h.owner
            .is_some_and(|o| matches!(m.items[o].kind, Kind::Pad(_)))
            || h.owner.is_none()
    };
    let boxes: Vec<Bounds> = m.holes.iter().map(|h| h.shape.bounds).collect();
    for (i, j) in near_pairs(&boxes, least_gap) {
        let (a, b) = (&m.holes[i], &m.holes[j]);
        if (a.owner.is_some() && a.owner == b.owner) || !(is_pad(a) || is_pad(b)) {
            continue;
        }
        let g = gap(&a.shape, &b.shape);
        if g.d < least_gap - EPS {
            out.push(
                Rule::PadHoleToHole,
                format!(
                    "holes are {:.4} mm apart ({} and {}); the process asks {:.4} mm when one is a pad's",
                    g.d, a.item.what, b.item.what, least_gap
                ),
                g.middle(),
                None,
                vec![a.item.clone(), b.item.clone()],
            );
        }
    }
}

fn pad_item(p: &Pad) -> Item {
    Item {
        what: format!(
            "Pad {}{} of {}",
            p.number,
            p.net
                .as_ref()
                .map(|n| format!(" [{n}]"))
                .unwrap_or_default(),
            p.part
        ),
        at: p.at,
        part: Some(p.part.clone()),
        pad: Some(p.number.clone()),
        net: p.net.clone(),
    }
}

/// The least of the gaps between two sets of shapes.
fn least(a: &[Shape], b: &[Shape]) -> Option<super::shape::Gap> {
    let mut best: Option<super::shape::Gap> = None;
    for sa in a {
        for sb in b {
            let g = gap(sa, sb);
            if best.is_none_or(|x| g.d < x.d) {
                best = Some(g);
            }
        }
    }
    best
}

/// A track of another net nearer a hole than the process allows: the page's "via hole to
/// track" and "NPTH to track" (`hole_to_track`), and "PTH to track" for the hole of a plated
/// pad (`plated_hole_clearance`). Only tracks: a pad beside a hole of its own footprint is the
/// footprint's, and the copper clearance judges it.
fn hole_to_track(out: &mut Out<'_>) {
    let m = out.m;
    let x = m.extras;
    if x.hole_to_track.is_none() && x.plated_hole_clearance.is_none() {
        return;
    }
    let limit = |h: &super::model::Hole| {
        let plated_pad = h.plated
            && h.owner
                .is_some_and(|o| matches!(m.items[o].kind, Kind::Pad(_)));
        if plated_pad {
            x.plated_hole_clearance
        } else {
            x.hole_to_track
        }
    };
    let holes: Vec<usize> = (0..m.holes.len())
        .filter(|&h| limit(&m.holes[h]).is_some())
        .collect();
    let tracks: Vec<usize> = (0..m.items.len())
        .filter(|&i| matches!(m.items[i].kind, Kind::Track(_)))
        .collect();
    let most = x
        .hole_to_track
        .unwrap_or(0.0)
        .max(x.plated_hole_clearance.unwrap_or(0.0));
    let mut boxes: Vec<Bounds> = holes.iter().map(|&h| m.holes[h].shape.bounds).collect();
    boxes.extend(tracks.iter().map(|&t| m.items[t].bounds));
    let n = holes.len();
    for (i, j) in near_pairs(&boxes, most) {
        if i >= n || j < n {
            continue;
        }
        let (h, c) = (&m.holes[holes[i]], &m.items[tracks[j - n]]);
        if h.item.net.is_some() && h.item.net == c.net {
            continue;
        }
        let Some(least_gap) = limit(h) else { continue };
        let Some(g) = least(&c.shapes, std::slice::from_ref(&h.shape)) else {
            continue;
        };
        if g.d < least_gap - EPS {
            out.push(
                Rule::HoleToTrack,
                format!(
                    "{:.4} mm between {} and the hole of {}; the process asks {:.4} mm from such a hole to a track",
                    g.d, c.item.what, h.item.what, least_gap
                ),
                g.middle(),
                None,
                vec![c.item.clone(), h.item.clone()],
            );
        }
    }
}

/// Surface-mount pads of different nets nearer than the process allows.
fn pad_to_pad(out: &mut Out<'_>) {
    let m = out.m;
    let Some(least_gap) = m.extras.pad_to_pad else {
        return;
    };
    let pads: Vec<usize> = (0..m.items.len())
        .filter(|&i| match m.items[i].kind {
            Kind::Pad(p) => matches!(m.art.pads[p].kind, PadKind::Smd | PadKind::Connect),
            _ => false,
        })
        .collect();
    let boxes: Vec<Bounds> = pads.iter().map(|&p| m.items[p].bounds).collect();
    for (i, j) in near_pairs(&boxes, least_gap) {
        let (a, b) = (&m.items[pads[i]], &m.items[pads[j]]);
        if same_net(&a.net, &b.net) || !a.layers.iter().any(|l| b.layers.contains(l)) {
            continue;
        }
        let Some(g) = least(&a.shapes, &b.shapes) else {
            continue;
        };
        // Pads that touch or overlap are the clearance check's (a short), not this one's.
        if g.d > 0.0 && g.d < least_gap - EPS {
            out.push(
                Rule::PadToPad,
                format!(
                    "{:.4} mm between {} and {}; the process asks {:.4} mm between surface-mount pads of different nets",
                    g.d, a.item.what, b.item.what, least_gap
                ),
                g.middle(),
                None,
                vec![a.item.clone(), b.item.clone()],
            );
        }
    }
}

/// A track of another net nearer a pad's solder mask opening than the process allows: the
/// mask may not cover it.
fn mask_to_track(out: &mut Out<'_>) {
    let m = out.m;
    let Some(least_gap) = m.extras.mask_to_track else {
        return;
    };
    let back = m.layers.len().saturating_sub(1);
    for (front, copper_layer, mask) in [(true, 0, "F.Mask"), (false, back, "B.Mask")] {
        let openings: Vec<(&Pad, Vec<Shape>)> = m
            .art
            .pads
            .iter()
            .map(|p| (p, m.mask_opening(p, front)))
            .filter(|(_, s)| !s.is_empty())
            .collect();
        let tracks: Vec<usize> = (0..m.items.len())
            .filter(|&i| {
                matches!(m.items[i].kind, Kind::Track(_))
                    && m.items[i].layers.contains(&copper_layer)
            })
            .collect();
        let mut boxes: Vec<Bounds> = openings.iter().map(|(_, s)| Model::bounds_of(s)).collect();
        boxes.extend(tracks.iter().map(|&t| m.items[t].bounds));
        let n = openings.len();
        for (i, j) in near_pairs(&boxes, least_gap) {
            if i >= n || j < n {
                continue;
            }
            let ((pad, opening), c) = (&openings[i], &m.items[tracks[j - n]]);
            if same_net(&pad.net, &c.net) {
                continue;
            }
            let Some(g) = least(&c.shapes, opening) else {
                continue;
            };
            if g.d < least_gap - EPS {
                out.push(
                    Rule::MaskToTrack,
                    format!(
                        "{:.4} mm between {} and the solder mask opening of pad {} of {}; the process asks {:.4} mm",
                        g.d, c.item.what, pad.number, pad.part, least_gap
                    ),
                    g.middle(),
                    Some(mask),
                    vec![c.item.clone(), pad_item(pad)],
                );
            }
        }
    }
}

/// The holes of pads: the copper round a plated one, the width of a slot, the size of a
/// non-plated hole.
fn holes(out: &mut Out<'_>) {
    let m = out.m;
    let x = m.extras;
    // A hole with no copper round it is no copper item: its size is read from the hole.
    for h in m.holes.iter().filter(|h| h.owner.is_none() && !h.plated) {
        let slot = h.shape.pts.len() == 2;
        let (limit, what, rule) = if slot {
            (x.npth_slot, "non-plated slot", Rule::SlotWidth)
        } else {
            (x.npth_drill, "non-plated hole", Rule::NpthDrill)
        };
        if let Some(limit) = limit
            && h.diameter < limit - EPS / 100.0
        {
            out.push(
                rule,
                format!(
                    "{what} of {} is {:.4} mm wide; the process makes it at least {limit:.4} mm",
                    h.item.what, h.diameter
                ),
                h.at,
                None,
                vec![h.item.clone()],
            );
        }
    }
    for c in &m.items {
        let Kind::Pad(p) = c.kind else { continue };
        let pad = &m.art.pads[p];
        let Some(d) = &pad.drill else { continue };
        let narrow = d.width.min(d.length);
        let slot = d.is_slot();
        let plated = pad.kind == PadKind::Thru;
        let (limit, what) = match (plated, slot) {
            (true, true) => (x.plated_slot, "plated slot"),
            (false, true) => (x.npth_slot, "non-plated slot"),
            (false, false) => (x.npth_drill, "non-plated hole"),
            (true, false) => (None, ""),
        };
        if let Some(limit) = limit
            && narrow < limit - EPS / 100.0
        {
            let rule = if slot {
                Rule::SlotWidth
            } else {
                Rule::NpthDrill
            };
            out.push(
                rule,
                format!(
                    "{what} of {} is {narrow:.4} mm wide; the process makes it at least {limit:.4} mm",
                    c.item.what
                ),
                pad.at,
                None,
                vec![c.item.clone()],
            );
        }
        if plated
            && let Some(limit) = x.pad_ring
            && let Some(hole) = m
                .holes
                .iter()
                .find(|h| h.owner.is_some_and(|o| m.items[o].kind == c.kind))
        {
            let ring = ring_width(&c.shapes, &hole.shape);
            if ring < limit - EPS / 100.0 {
                out.push(
                    Rule::PlatedRing,
                    format!(
                        "{} has {ring:.4} mm of copper round its {:.4} mm hole; the process leaves at least {limit:.4} mm round a plated hole",
                        c.item.what, hole.diameter
                    ),
                    pad.at,
                    None,
                    vec![c.item.clone()],
                );
            }
        }
    }
}
