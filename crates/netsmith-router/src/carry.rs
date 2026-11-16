//! Partial routing (ADR 0030, issue #825): after a change to a placement, keep the router's
//! copper of every net the change did not touch and route only the rest.
//!
//! [`carry_copper`] puts on a board placed again (with its routes file's copper, and none of
//! the router's) the copper the router drew on the same board before the change, net by net. A
//! net's copper is **carried** when its pads are where they were ([`netsmith_pcb::pad_hash`]),
//! the caller does not name it, the routes file does not lock it, and none of its tracks or vias
//! comes nearer than the board's clearance to a pad of another net on a part that moved. Every
//! other net is **ripped up**: none of its copper is carried, and the router routes it again. A
//! part moved when its place, turn or face differs. When the routes file's copper is not the same
//! on the two boards, nothing is carried: the router's copper was drawn round the old.
//!
//! Carried copper is fixed copper ([`netsmith_pcb::Board::fixed`]): it goes after the routes
//! file's, the router keeps it as an obstacle of its net and never moves or removes it, and
//! routes only the connections it leaves. Teardrops are not carried; the teardrop pass draws
//! them on the finished board.
//!
//! [`reroute`] carries and routes. If connections are left, the nets whose carried copper
//! crosses the box round the parts that moved (before and after, grown by
//! [`CROSSING_MARGIN`]) are ripped up too and the board is routed again from the carried copper
//! less theirs; the result that leaves fewer connections is kept, and
//! [`Rerouted::also`] names the nets of the second run.

use std::collections::BTreeSet;

use netsmith_pcb::{Board, CopperLayer, Placed, Pt, Rect, Track, Via, pad_hash};

use crate::route::{RouteOptions, Routing, route_with};

/// How far round the parts that moved the second run of [`reroute`] rips up crossing copper, in
/// mm.
pub const CROSSING_MARGIN: f64 = 3.0;

/// What [`carry_copper`] did, by net name, each sorted.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Carried {
    /// The nets whose router copper was carried.
    pub kept: Vec<String>,
    /// The nets that had router copper and lost it.
    pub ripped: Vec<String>,
    /// How many tracks and vias were carried.
    pub tracks: usize,
    pub vias: usize,
}

/// What [`reroute`] did.
#[derive(Debug)]
pub struct Rerouted {
    pub routing: Routing,
    pub carried: Carried,
    /// The nets ripped up in a second run because the first left connections and their copper
    /// crossed the parts that moved; empty when there was no second run or it was not kept.
    pub also: Vec<String>,
    /// How many times the router ran (1 or 2).
    pub runs: usize,
}

/// The parts of `after` whose place, turn or face differs from `before`'s, by reference
/// designator.
pub fn moved_parts<'a>(before: &Board, after: &'a Board) -> Vec<&'a Placed> {
    after
        .parts
        .iter()
        .filter(|p| {
            before.part(&p.refdes).is_none_or(|q| {
                (q.at.x - p.at.x).abs() > 1e-6
                    || (q.at.y - p.at.y).abs() > 1e-6
                    || (q.rotation - p.rotation).abs() > 1e-6
                    || q.side != p.side
            })
        })
        .collect()
}

/// Put the router's copper of `before` on `after` for every net the change left alone, and
/// none for the nets in `rip` (the module's header).
pub fn carry_copper(before: &Board, after: &mut Board, rip: &BTreeSet<String>) -> Carried {
    let tracks = &before.tracks[before.fixed.tracks.min(before.tracks.len())..];
    let vias = &before.vias[before.fixed.vias.min(before.vias.len())..];
    let nets: BTreeSet<&str> = tracks
        .iter()
        .map(|t| t.net.as_str())
        .chain(vias.iter().map(|v| v.net.as_str()))
        .collect();
    // The routes file's copper changed (a track drawn, a record dropped): the router's copper
    // was drawn round other fixed copper and may now cross it, so none of it is carried.
    let same_fixed = before.tracks[..before.fixed.tracks.min(before.tracks.len())]
        == after.tracks[..after.fixed.tracks.min(after.tracks.len())]
        && before.vias[..before.fixed.vias.min(before.vias.len())]
            == after.vias[..after.fixed.vias.min(after.vias.len())]
        && before.fixed.locked == after.fixed.locked
        && before.zones == after.zones;
    let pads = moved_pads(after, &moved_parts(before, after));
    let clearance = after.rules.clearance;
    let mut carried = Carried::default();
    let mut new_tracks: Vec<Track> = Vec::new();
    let mut new_vias: Vec<Via> = Vec::new();
    for net in nets {
        let own_tracks: Vec<&Track> = tracks.iter().filter(|t| t.net == net).collect();
        let own_vias: Vec<&Via> = vias.iter().filter(|v| v.net == net).collect();
        let keep = same_fixed
            && !rip.contains(net)
            && !after.fixed.is_locked(net)
            && pad_hash(before, net).is_some()
            && pad_hash(before, net) == pad_hash(after, net)
            && !own_tracks
                .iter()
                .any(|t| track_clashes(t, &pads, clearance))
            && !own_vias.iter().any(|v| via_clashes(v, &pads, clearance));
        if keep {
            carried.kept.push(net.to_string());
            carried.tracks += own_tracks.len();
            carried.vias += own_vias.len();
            new_tracks.extend(own_tracks.into_iter().cloned());
            new_vias.extend(own_vias.into_iter().cloned());
        } else {
            carried.ripped.push(net.to_string());
        }
    }
    // After the routes file's copper, which is first, and before anything else.
    let at_tracks = after.fixed.tracks.min(after.tracks.len());
    let at_vias = after.fixed.vias.min(after.vias.len());
    after.fixed.tracks += new_tracks.len();
    after.fixed.vias += new_vias.len();
    after.tracks.splice(at_tracks..at_tracks, new_tracks);
    after.vias.splice(at_vias..at_vias, new_vias);
    carried
}

/// Carry the copper of `before` onto `after` ([`carry_copper`]) and route what is left, with a
/// second run when connections are left (the module's header).
pub fn reroute(
    before: &Board,
    after: &mut Board,
    rip: &BTreeSet<String>,
    options: RouteOptions,
) -> Rerouted {
    let start = after.clone();
    let carried = carry_copper(before, after, rip);
    let routing = route_with(after, options);
    if routing.left.lines.is_empty()
        || options
            .cancel
            .is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed))
    {
        return Rerouted {
            routing,
            carried,
            also: Vec::new(),
            runs: 1,
        };
    }
    let moved = moved_parts(before, &start);
    let Some(area) = moved
        .iter()
        .map(|p| p.courtyard())
        .chain(
            moved
                .iter()
                .filter_map(|p| before.part(&p.refdes).map(Placed::courtyard)),
        )
        .reduce(|a, b| a.union(&b))
        .map(|r| r.expanded(CROSSING_MARGIN))
    else {
        return Rerouted {
            routing,
            carried,
            also: Vec::new(),
            runs: 1,
        };
    };
    let crossing: BTreeSet<String> = carried
        .kept
        .iter()
        .filter(|net| {
            let tracks = &before.tracks[before.fixed.tracks.min(before.tracks.len())..];
            let vias = &before.vias[before.fixed.vias.min(before.vias.len())..];
            tracks
                .iter()
                .filter(|t| &t.net == *net)
                .any(|t| segment_meets(t.from, t.to, &area))
                || vias
                    .iter()
                    .filter(|v| &v.net == *net)
                    .any(|v| holds(&area, v.at))
        })
        .cloned()
        .collect();
    if crossing.is_empty() {
        return Rerouted {
            routing,
            carried,
            also: Vec::new(),
            runs: 1,
        };
    }
    let mut second = start;
    let rip_more: BTreeSet<String> = rip.union(&crossing).cloned().collect();
    let carried_more = carry_copper(before, &mut second, &rip_more);
    let routing_more = route_with(&mut second, options);
    if routing_more.left.lines.len() < routing.left.lines.len() {
        *after = second;
        Rerouted {
            routing: routing_more,
            carried: carried_more,
            also: crossing.into_iter().collect(),
            runs: 2,
        }
    } else {
        Rerouted {
            routing,
            carried,
            also: Vec::new(),
            runs: 2,
        }
    }
}

/// A copper pad (or a hole) of a part that moved: its box on the board, the faces it is on, and
/// its net, if it has one.
struct MovedPad {
    rect: Rect,
    front: bool,
    back: bool,
    net: Option<String>,
}

/// The pads of `moved`, with their nets as `board` has them.
fn moved_pads(board: &Board, moved: &[&Placed]) -> Vec<MovedPad> {
    let mut pads = Vec::new();
    for part in moved {
        for pad in &part.footprint.pads {
            let (front, back) = part.pad_layers(pad);
            // A hole with no copper is in the way on both faces.
            let (front, back) = if !front && !back {
                (true, true)
            } else {
                (front, back)
            };
            let net = board
                .nets
                .iter()
                .find(|n| {
                    n.pads
                        .iter()
                        .any(|p| p.refdes == part.refdes && p.number == pad.number)
                })
                .map(|n| n.name.clone());
            pads.push(MovedPad {
                rect: part.pad_bounds(pad),
                front,
                back,
                net,
            });
        }
    }
    pads
}

fn track_clashes(track: &Track, pads: &[MovedPad], clearance: f64) -> bool {
    // An inner track meets only a through-hole pad, which has copper on every layer.
    let on = |pad: &MovedPad| match track.layer {
        CopperLayer::Front => pad.front,
        CopperLayer::Back => pad.back,
        CopperLayer::Inner(_) => pad.front && pad.back,
    };
    let reach = track.width / 2.0 + clearance;
    let pieces: Vec<(Pt, Pt)> = match track.mid {
        Some(mid) => vec![(track.from, mid), (mid, track.to)],
        None => vec![(track.from, track.to)],
    };
    pads.iter().any(|pad| {
        on(pad) && pad.net.as_deref() != Some(track.net.as_str()) && {
            let grown = pad.rect.expanded(reach);
            pieces.iter().any(|(a, b)| segment_meets(*a, *b, &grown))
        }
    })
}

fn via_clashes(via: &Via, pads: &[MovedPad], clearance: f64) -> bool {
    let reach = via.diameter / 2.0 + clearance;
    pads.iter().any(|pad| {
        pad.net.as_deref() != Some(via.net.as_str()) && holds(&pad.rect.expanded(reach), via.at)
    })
}

/// Whether the point `p` is in the box `r`, its edges included.
fn holds(r: &Rect, p: Pt) -> bool {
    p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y
}

/// Whether the segment from `a` to `b` meets the box `r` (Liang and Barsky's clipping).
fn segment_meets(a: Pt, b: Pt, r: &Rect) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
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
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}
