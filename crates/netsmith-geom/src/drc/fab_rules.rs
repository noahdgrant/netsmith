//! The rules of a fabricator's capabilities page that a KiCad project file cannot say, which
//! the caller passes in [`Extras`](super::Extras) from the board's process (issue #431). Each is
//! netsmith's own. Those a KiCad custom rule can say (`pad_to_pad`, `hole_to_track`,
//! `pad_hole_to_hole`, `npth_drill`, `slot_width`, `plated_ring`) `fab` also writes into the
//! `.kicad_dru` beside the project (`rules_file.rs`, issue #1185), so KiCad's check judges them
//! and the cross-check compares them; the others it counts and does not compare. A rule the
//! caller does not pass is not checked, so a board read with no process and no rules file (a
//! port, the `drc` example) is checked as KiCad would check it.
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
//! **Rejected:** a reader of a fabricator's KiCad rules file (`.kicad_dru`) as the source of a
//! process: the owner chose the std processes (2026-11-11), and JLCPCB publishes none. The file
//! netsmith writes is read back for its own rules only. One hole clearance for every hole and
//! every copper, as KiCad's single number invites: it refuses land patterns the page does not,
//! or, at 0.28 mm, makes every via wider in effect.

use super::checks::{EPS, Out, pad_copper, ring_width, same_net};
use super::model::{Kind, Model};
use super::shape::{Shape, gap, near_pairs};
use super::{Carrying, Item, Rule};
use crate::artwork::{Pad, PadKind};
use crate::geom::Bounds;

pub(crate) fn run(out: &mut Out<'_>) {
    hole_to_track(out);
    pad_to_pad(out);
    mask_to_track(out);
    holes(out);
    pad_hole_to_hole(out);
    track_current(out);
    track_impedance(out);
    via_current(out);
    track_angle(out);
}

/// A place where tracks end: the net, the layer, and the place to the micrometre.
type Joint = (String, String, (i64, i64));

/// Where exactly two tracks of a net meet on a layer outside the net's pads, the bend: more
/// than 45 degrees (a right angle, or sharper) is a finding; where three or more meet, an acute
/// angle between any two is. An arc meets at its tangent, so an arc tangent to the tracks it
/// joins is no bend at all.
fn track_angle(out: &mut Out<'_>) {
    use crate::artwork::TrackShape;
    use std::collections::HashMap;
    let m = out.m;
    if !m.extras.track_angle {
        return;
    }
    let key = |p: crate::geom::Pt| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
    // The directions out of each place where tracks end, by net and layer, with an item.
    let mut ends: HashMap<Joint, (crate::geom::Pt, Vec<crate::geom::Pt>, usize)> = HashMap::new();
    for (k, c) in m.items.iter().enumerate() {
        let Kind::Track(i) = c.kind else { continue };
        let t = &m.art.tracks[i];
        // Each end of the track and the way the track leaves it: an arc leaves along its
        // tangent, so a tangent arc between two straight tracks is no corner.
        let out_of: Vec<(crate::geom::Pt, crate::geom::Pt)> = match t.shape {
            TrackShape::Line(a, b) => {
                if a.dist(b) < 1e-6 {
                    continue;
                }
                vec![(a, b - a), (b, a - b)]
            }
            TrackShape::Arc(arc) => {
                let (first, last) = arc.tangents();
                vec![(arc.start, first), (arc.end, last)]
            }
        };
        let net = t.net.clone().unwrap_or_default();
        for (at, way) in out_of {
            ends.entry((net.clone(), t.layer.clone(), key(at)))
                .or_insert((at, Vec::new(), k))
                .1
                .push(way);
        }
    }
    let angle = |u: crate::geom::Pt, v: crate::geom::Pt| {
        let dot = u.x * v.x + u.y * v.y;
        let n = (u.x.hypot(u.y) * v.x.hypot(v.y)).max(1e-18);
        (dot / n).clamp(-1.0, 1.0).acos().to_degrees()
    };
    let mut found: Vec<(Joint, f64)> = Vec::new();
    for (place, (at, out_dirs, _)) in &ends {
        if out_dirs.len() < 2 {
            continue;
        }
        let (net, layer, _) = place;
        // Two tracks of the net that run out the same way overlap: one direction, not a corner.
        let mut out_dirs: Vec<crate::geom::Pt> = out_dirs.clone();
        let unit = |p: &crate::geom::Pt| {
            let n = p.x.hypot(p.y).max(1e-18);
            (
                (p.x / n * 1e4).round() as i64,
                (p.y / n * 1e4).round() as i64,
            )
        };
        out_dirs.sort_by_key(unit);
        out_dirs.dedup_by_key(|p| unit(p));
        if out_dirs.len() < 2 {
            continue;
        }
        let dot = Shape::disc(*at, 0.0);
        // Inside the copper of a pad or a via of the net, a joint is no corner.
        let in_pad = m.items.iter().any(|c| {
            matches!(c.kind, Kind::Pad(_) | Kind::Via(_))
                && c.net.as_deref() == Some(net.as_str())
                && c.layers.iter().any(|&l| m.layer_name(l) == layer)
                && c.shapes.iter().any(|s| gap(&dot, s).d == 0.0)
        });
        if in_pad {
            continue;
        }
        let least = (0..out_dirs.len())
            .flat_map(|i| (i + 1..out_dirs.len()).map(move |j| (i, j)))
            .map(|(i, j)| angle(out_dirs[i], out_dirs[j]))
            .fold(180.0, f64::min);
        // Two tracks meet at 135 degrees at a 45 degree bend; three may meet at right angles (a
        // tee), but never at an acute angle.
        let limit = if out_dirs.len() == 2 { 134.0 } else { 89.0 };
        if least < limit {
            found.push((place.clone(), least));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    for (place, least) in found {
        let (at, _, k) = &ends[&place];
        let item = m.items[*k].item.clone();
        out.push(
            Rule::TrackAngle,
            format!(
                "two tracks of net {} meet at {least:.0} degrees on {}; a track bends by 45 degrees at most, and never to a right or an acute angle",
                place.0, place.1
            ),
            *at,
            Some(&place.1),
            vec![item],
        );
    }
}

/// A via of a net that needs more than one where it changes layer, in a cluster of touching
/// vias of the net smaller than that.
fn via_current(out: &mut Out<'_>) {
    let m = out.m;
    if m.extras.via_counts.is_empty() {
        return;
    }
    let vias = &m.art.vias;
    let mut seen = vec![false; vias.len()];
    for start in 0..vias.len() {
        if seen[start] {
            continue;
        }
        let Some(net) = vias[start].net.as_deref() else {
            continue;
        };
        let Some(&(_, needs)) = m.extras.via_counts.iter().find(|(n, _)| n == net) else {
            continue;
        };
        let mut cluster = vec![start];
        seen[start] = true;
        let mut k = 0;
        while k < cluster.len() {
            let v = &vias[cluster[k]];
            for (j, w) in vias.iter().enumerate() {
                if !seen[j]
                    && w.net.as_deref() == Some(net)
                    && w.at.dist(v.at) <= (w.diameter + v.diameter) / 2.0 + EPS
                {
                    seen[j] = true;
                    cluster.push(j);
                }
            }
            k += 1;
        }
        // A cluster that only feeds branches needs what they draw (issue #1113).
        let items: Vec<usize> = cluster.iter().map(|&v| m.via_item[v]).collect();
        let through = m.extras.sizing.as_ref().and_then(|sizing| {
            super::branches::current_through(m, net, &items).map(|t| (sizing, t))
        });
        let (current, from, needed) = match &through {
            Some((sizing, t)) => (t.current, branch_source(t), sizing.vias(t.current)),
            None => (
                net_current(m, net).map_or(0.0, |c| c.current),
                net_source(m, net),
                needs,
            ),
        };
        if cluster.len() >= needed {
            continue;
        }
        let item = m.items[m.via_item[start]].item.clone();
        let sizing = m.extras.sizing.as_ref();
        let held = match (current > 0.0, sizing) {
            (true, Some(s)) => format!(
                "{}, and a via of the process (a {:.2} mm hole plated {:.0} \u{3bc}m) carries {} at a {} K rise (IPC-2221, the inner-layer constant), so {needed} side by side are needed",
                carries(through.is_some(), net, current, &from),
                s.via_drill,
                s.via_plating * 1000.0,
                amps(crate::ipc2221::via_current(
                    s.via_drill,
                    s.via_plating,
                    s.rise
                )),
                s.rise
            ),
            _ => format!("its net's current needs {needed}"),
        };
        out.push_carrying(
            Rule::ViaCurrent,
            format!(
                "{} stands with {} via(s) of its net side by side; {held}",
                item.what,
                cluster.len()
            ),
            vias[start].at,
            None,
            vec![item],
            Some(Carrying {
                current: (current > 0.0).then_some(current),
                current_from: from,
                vias: Some(cluster.len()),
                needed_vias: Some(needed),
                rise: sizing.map(|s| s.rise),
                tolerance: sizing.map(|s| s.tolerance),
                ..Carrying::default()
            }),
        );
    }
}

/// A track narrower than its net's current needs on its layer, unless it lies within the
/// neck-down reach of one of the net's pads.
fn track_current(out: &mut Out<'_>) {
    let m = out.m;
    let x = m.extras;
    if x.least_widths.is_empty() && x.inner_least_widths.is_empty() {
        return;
    }
    for (k, c) in m.items.iter().enumerate() {
        let Kind::Track(i) = c.kind else { continue };
        let t = &m.art.tracks[i];
        let Some(net) = t.net.as_deref() else {
            continue;
        };
        let inner = t.layer.starts_with("In");
        let Some(least) = x.least_width(net, inner) else {
            continue;
        };
        if t.width >= least - EPS / 100.0 {
            continue;
        }
        if near_pads(m, c, net) {
            continue;
        }
        // A track that only feeds branches needs what they draw (issue #1113).
        let through = x
            .sizing
            .as_ref()
            .and_then(|sizing| super::branches::current_through(m, net, &[k]).map(|t| (sizing, t)));
        let (current, from, needed) = match &through {
            Some((sizing, t)) => (
                Some(t.current),
                branch_source(t),
                sizing.least_width_on(t.current, inner),
            ),
            None => (
                net_current(m, net).map(|c| c.current),
                net_source(m, net),
                least,
            ),
        };
        if t.width >= needed - EPS / 100.0 {
            continue;
        }
        let sizing = x.sizing.as_ref();
        // The copper the width was sized for: the inner layers' where the track is on one and
        // the board has inner signal copper.
        let (copper, layer_word) = match sizing {
            Some(s) => match s.inner_copper.filter(|_| inner) {
                Some(c) => (Some(c), "inner"),
                None => (Some(s.copper), "outer"),
            },
            None => (None, "outer"),
        };
        let basis = match (sizing, copper) {
            (Some(s), Some(c)) => format!(
                "IPC-2221, {:.0} \u{3bc}m {layer_word} copper, a {} K temperature rise, the fabricator's {:.0}% width tolerance",
                c * 1000.0,
                s.rise,
                s.tolerance * 100.0
            ),
            _ => "IPC-2221 at the fabricator's width tolerance".to_string(),
        };
        let held = match current {
            Some(i) => format!("{}, which needs", carries(through.is_some(), net, i, &from)),
            None => "its net's current needs".to_string(),
        };
        out.push_carrying(
            Rule::TrackCurrent,
            format!(
                "{} is {:.4} mm wide; {held} {needed:.4} mm ({basis}), and only within {:.2} mm of the pads of a part on the net may a track be narrower: widen it to {needed:.2} mm or more, or lower the current",
                c.item.what, t.width, x.neck_reach
            ),
            c.item.at,
            Some(&t.layer),
            vec![c.item.clone()],
            Some(Carrying {
                current,
                current_from: from,
                width: Some(t.width),
                needed_width: Some(needed),
                copper,
                rise: sizing.map(|s| s.rise),
                tolerance: sizing.map(|s| s.tolerance),
                ..Carrying::default()
            }),
        );
    }
}

/// What the net carries (issue 1373): its worst-case current and where it comes from, where the
/// design stage said.
fn net_current<'a>(m: &Model<'a>, net: &str) -> Option<&'a super::NetCurrent> {
    m.extras.net_currents.iter().find(|c| c.net == net)
}

/// Where the net's current comes from, in words; empty when it is not known.
fn net_source(m: &Model<'_>, net: &str) -> String {
    net_current(m, net)
        .map(|c| c.source.clone())
        .unwrap_or_default()
}

/// Where the current of copper that only feeds branches comes from: the pins that state it.
fn branch_source(t: &super::branches::Through) -> String {
    let pins: Vec<String> = t.pins.iter().map(|p| format!("`{p}`")).collect();
    format!(
        "the most the pins {} carry, by their `.current` or what the model finds through them",
        pins.join(", ")
    )
}

/// The start of the sentence that says what current the copper is held to: its net's, or, for
/// copper that only feeds the branches of the net, theirs.
fn carries(branch: bool, net: &str, current: f64, from: &str) -> String {
    let because = if from.is_empty() {
        String::new()
    } else {
        format!(" ({from})")
    };
    if branch {
        format!(
            "it only feeds branches of net `{net}` and carries at most {}{because}",
            amps(current)
        )
    } else {
        format!("its net `{net}` carries {}{because}", amps(current))
    }
}

/// A current in a message: `500 mA`, `2.5 A`.
fn amps(x: f64) -> String {
    let text = |v: f64| {
        let t = format!("{v:.3}");
        t.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    if x < 1.0 {
        format!("{} mA", text(x * 1e3))
    } else {
        format!("{} A", text(x))
    }
}

/// Whether every point of track item `c` of `net` lies within the neck-down reach of the box
/// round the centres of the pads of a part that has a pad on the net.
fn near_pads(m: &Model<'_>, c: &super::model::Copper, net: &str) -> bool {
    let x = m.extras;
    let parts: Vec<&str> = m
        .art
        .pads
        .iter()
        .filter(|p| p.net.as_deref() == Some(net))
        .map(|p| p.part.as_str())
        .collect();
    let near_pad = |p: crate::geom::Pt| {
        parts.iter().any(|&part| {
            let Some(b) = Bounds::of(m.art.pads.iter().filter(|q| q.part == part).map(|q| q.at))
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
    (0..=steps).all(|k| near_pad(a + (b - a) * (k as f64 / steps as f64)))
}

/// A track of a net with an impedance target that is not the width that gives the target on its
/// layer (issue #1025), unless it lies within the neck-down reach of one of the net's pads.
fn track_impedance(out: &mut Out<'_>) {
    let m = out.m;
    let x = m.extras;
    if x.target_widths.is_empty() {
        return;
    }
    for c in &m.items {
        let Kind::Track(i) = c.kind else { continue };
        let t = &m.art.tracks[i];
        let Some(net) = t.net.as_deref() else {
            continue;
        };
        let Some(&(_, _, want)) = x
            .target_widths
            .iter()
            .find(|(n, l, _)| n == net && *l == t.layer)
        else {
            continue;
        };
        if (t.width - want).abs() <= EPS / 100.0 || near_pads(m, c, net) {
            continue;
        }
        out.push(
            Rule::TrackImpedance,
            format!(
                "{} is {:.4} mm wide; its net's impedance target needs {:.4} mm on {}, and only within {:.2} mm of the pads of a part on the net may a track be another width",
                c.item.what, t.width, want, t.layer, x.neck_reach
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
        x.hole_to_track_of(plated_pad)
    };
    let holes: Vec<usize> = (0..m.holes.len())
        .filter(|&h| limit(&m.holes[h]).is_some())
        .collect();
    let tracks: Vec<usize> = (0..m.items.len())
        .filter(|&i| m.items[i].kind.is_track_like())
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
        // A blind or a buried via's hole is not on a layer it does not reach (issue #1015).
        if let Some((a, b)) = h.span
            && !c.layers.iter().any(|&l| a <= l && l <= b)
        {
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
                m.items[i].kind.is_track_like() && m.items[i].layers.contains(&copper_layer)
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
    for (index, c) in m.items.iter().enumerate() {
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
            let ring = ring_width(&pad_copper(m, index), &hole.shape);
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
