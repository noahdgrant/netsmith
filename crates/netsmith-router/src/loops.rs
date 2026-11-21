//! No connection drawn twice (issue #1486): the router routes a net line by line, each from pad
//! to pad, and a line may run where another of its net already runs, or reach a pad or a via the
//! net's copper reaches already. The net's copper then has a loop: two ways between the same two
//! places. Once the board is routed, each loop of a net's new copper loses a piece: the longest
//! piece on a loop first, with what of the line it was on goes nowhere after (a dead end), where
//! that leaves every place where tracks meet outside a pad or a via bent by 45 degrees at most
//! (and three or more at right angles or wider), as `track_angle` asks, and no track ending
//! there alone, on the middle of another, which KiCad's check finds unconnected. Taking copper away never
//! comes nearer other copper, and a piece on a loop joins nothing the rest does not.
//!
//! A net's copper is a graph: its pads and vias are places (a track that ends anywhere in one
//! joins it), and so is each other end of a track; a track is cut where another of its net ends
//! on it, and its pieces that are kept in a row are one track again, so only the ends of tracks
//! are judged as `track_angle` judges them. Copper the board had before it was routed (a routes
//! file's) stays as it is.

use std::collections::HashMap;

use netsmith_pcb::{Board, CopperLayer, Pt, Rect, Track};

/// Ends nearer than this are one place.
const SAME: f64 = 1e-4;

/// Copper a track end may join: a pad's box and layers, or a via.
enum Place {
    Pad(Rect, bool, bool),
    Via(Pt, f64),
}

impl Place {
    fn holds(&self, p: Pt, layer: CopperLayer) -> bool {
        match self {
            Place::Pad(r, front, back) => {
                let on = match layer {
                    CopperLayer::Front => *front,
                    CopperLayer::Back => *back,
                    CopperLayer::Inner(_) => *front && *back,
                };
                on && r.expanded(SAME).contains(&Rect::new(p, p))
            }
            Place::Via(c, r) => c.distance(p) <= r + SAME,
        }
    }
}

/// A piece of a track: the track, its ends and the places they are.
#[derive(Clone)]
struct Piece {
    track: usize,
    from: Pt,
    to: Pt,
    a: usize,
    b: usize,
    /// Whether the router drew it, so it may go.
    new: bool,
}

/// Whether `p` lies on the straight track `t` between its ends.
fn inside(p: Pt, t: &Track) -> bool {
    let (dx, dy) = (t.to.x - t.from.x, t.to.y - t.from.y);
    let length = dx.hypot(dy);
    if t.mid.is_some() || length < SAME {
        return false;
    }
    let along = ((p.x - t.from.x) * dx + (p.y - t.from.y) * dy) / length;
    let across = ((p.x - t.from.x) * dy - (p.y - t.from.y) * dx).abs() / length;
    across < SAME && along > SAME && along < length - SAME
}

/// The way a track leaves its end `at`: along its line, or an arc's tangent there.
fn leaving(t: &Track, at: Pt) -> Option<(f64, f64)> {
    let towards = match (t.mid, at.distance(t.from) < at.distance(t.to)) {
        (Some(m), _) => m,
        (None, true) => t.to,
        (None, false) => t.from,
    };
    let (dx, dy) = (towards.x - at.x, towards.y - at.y);
    let n = dx.hypot(dy);
    (n > 1e-9).then_some((dx / n, dy / n))
}

/// Take a piece off each loop of each net's copper drawn from track `first` on (the module's
/// header), but of the tracks in `keep` (a pad's track to its own via, which a pour may take
/// off whole later, `crate::pour_pads`). Returns how many pieces were taken off.
pub(crate) fn drop_second_ways(board: &mut Board, first: usize, keep: &[&Track]) -> usize {
    let mut nets: Vec<String> = board.tracks[first..]
        .iter()
        .map(|t| t.net.clone())
        .collect();
    nets.sort_unstable();
    nets.dedup();
    let mut dropped = 0;
    for net in nets {
        dropped += drop_in(board, &net, first, keep);
    }
    dropped
}

fn drop_in(board: &mut Board, net: &str, first: usize, keep: &[&Track]) -> usize {
    let mut places: Vec<Place> = Vec::new();
    for part in &board.parts {
        for pad in &part.footprint.pads {
            if board
                .net_of(&part.refdes, &pad.number)
                .is_none_or(|n| n.name != net)
            {
                continue;
            }
            let (front, back) = part.pad_layers(pad);
            places.push(Place::Pad(part.pad_bounds(pad), front, back));
        }
    }
    for v in board.vias.iter().filter(|v| v.net == net) {
        places.push(Place::Via(v.at, v.diameter / 2.0));
    }
    let fixed = first.max(board.fixed.tracks);
    let tracks: Vec<usize> = (0..board.tracks.len())
        .filter(|&k| board.tracks[k].net == net)
        .collect();
    // The places: the net's pads and vias, then the other ends of its tracks, by layer.
    let mut free: Vec<(CopperLayer, Pt)> = Vec::new();
    let mut place_of = |p: Pt, layer: CopperLayer| -> usize {
        if let Some(k) = places.iter().position(|q| q.holds(p, layer)) {
            return k;
        }
        let k = free
            .iter()
            .position(|(l, q)| *l == layer && q.distance(p) < SAME)
            .unwrap_or_else(|| {
                free.push((layer, p));
                free.len() - 1
            });
        places.len() + k
    };
    let mut pieces: Vec<Piece> = Vec::new();
    for &k in &tracks {
        let t = &board.tracks[k];
        let mut cuts: Vec<Pt> = tracks
            .iter()
            .filter(|&&j| j != k && board.tracks[j].layer == t.layer)
            .flat_map(|&j| [board.tracks[j].from, board.tracks[j].to])
            .filter(|&p| inside(p, t))
            .collect();
        cuts.sort_by(|p, q| t.from.distance(*p).total_cmp(&t.from.distance(*q)));
        cuts.dedup_by(|p, q| p.distance(*q) < SAME);
        let mut points = vec![t.from];
        points.extend(cuts);
        points.push(t.to);
        for w in points.windows(2) {
            let (a, b) = (place_of(w[0], t.layer), place_of(w[1], t.layer));
            pieces.push(Piece {
                track: k,
                from: w[0],
                to: w[1],
                a,
                b,
                new: k >= fixed && !board.fixed.is_locked(net) && !keep.contains(&t),
            });
        }
    }
    let count = places.len() + free.len();
    let is_free = |n: usize| n >= places.len();
    let mut kept = vec![true; pieces.len()];
    // Whether `x` reaches `y` over the kept pieces.
    let joined = |kept: &[bool], x: usize, y: usize| {
        let mut seen = vec![false; count];
        let mut todo = vec![x];
        seen[x] = true;
        while let Some(n) = todo.pop() {
            if n == y {
                return true;
            }
            for (k, p) in pieces.iter().enumerate() {
                if !kept[k] {
                    continue;
                }
                let m = if p.a == n {
                    p.b
                } else if p.b == n {
                    p.a
                } else {
                    continue;
                };
                if !seen[m] {
                    seen[m] = true;
                    todo.push(m);
                }
            }
        }
        false
    };
    let degree = |kept: &[bool], n: usize| {
        pieces
            .iter()
            .enumerate()
            .filter(|(k, p)| kept[*k] && (p.a == n) != (p.b == n))
            .count()
    };
    // The bends at a free place, as `track_angle` judges them.
    let bends_well = |kept: &[bool], n: usize| {
        let (layer, at) = free[n - places.len()];
        let mut ways: Vec<(f64, f64)> = Vec::new();
        let mut ends = 0;
        for (k, p) in pieces.iter().enumerate() {
            if !kept[k] || (p.a != n && p.b != n) {
                continue;
            }
            // Two kept pieces of one track that meet here are one track, through it.
            let through = pieces
                .iter()
                .enumerate()
                .any(|(j, o)| j != k && kept[j] && o.track == p.track && (o.a == n || o.b == n));
            if through {
                continue;
            }
            let t = &board.tracks[p.track];
            let piece = Track {
                from: p.from,
                to: p.to,
                ..t.clone()
            };
            if t.layer != layer {
                continue;
            }
            ends += 1;
            if let Some(u) = leaving(&piece, at)
                && !ways
                    .iter()
                    .any(|v| (v.0 - u.0).abs() < 1e-3 && (v.1 - u.1).abs() < 1e-3)
            {
                ways.push(u);
            }
        }
        // A track that ends here alone, on another or on nothing, joins nothing: KiCad's check
        // finds its end unconnected.
        if ends == 1 {
            return false;
        }
        if ways.len() < 2 {
            return true;
        }
        let least = (0..ways.len())
            .flat_map(|i| (i + 1..ways.len()).map(move |j| (i, j)))
            .map(|(i, j)| {
                let dot = ways[i].0 * ways[j].0 + ways[i].1 * ways[j].1;
                dot.clamp(-1.0, 1.0).acos().to_degrees()
            })
            .fold(180.0, f64::min);
        least >= if ways.len() == 2 { 134.0 } else { 89.0 }
    };
    let mut order: Vec<usize> = (0..pieces.len()).filter(|&k| pieces[k].new).collect();
    order.sort_by(|&x, &y| {
        let length = |k: usize| pieces[k].from.distance(pieces[k].to);
        length(y).total_cmp(&length(x)).then(x.cmp(&y))
    });
    let mut dropped = 0;
    for k in order {
        if !kept[k] || pieces[k].a == pieces[k].b {
            continue;
        }
        let mut trial = kept.clone();
        trial[k] = false;
        if !joined(&trial, pieces[k].a, pieces[k].b) {
            continue;
        }
        // What goes nowhere now: a free place with one piece left, while it is new.
        let mut touched = vec![pieces[k].a, pieces[k].b];
        let mut ends: Vec<usize> = touched.clone();
        while let Some(n) = ends.pop() {
            if !is_free(n) || degree(&trial, n) != 1 {
                continue;
            }
            let Some(j) =
                (0..pieces.len()).find(|&j| trial[j] && (pieces[j].a == n || pieces[j].b == n))
            else {
                continue;
            };
            if !pieces[j].new {
                continue;
            }
            trial[j] = false;
            let other = if pieces[j].a == n {
                pieces[j].b
            } else {
                pieces[j].a
            };
            touched.push(other);
            ends.push(other);
        }
        if touched
            .iter()
            .all(|&n| !is_free(n) || degree(&trial, n) == 0 || bends_well(&trial, n))
        {
            dropped += kept.iter().zip(&trial).filter(|(a, b)| **a && !**b).count();
            kept = trial;
        }
    }
    if dropped == 0 {
        return 0;
    }
    // Each track as the pieces of it that are kept, those in a row as one.
    let mut by_track: HashMap<usize, Vec<(Pt, Pt)>> = HashMap::new();
    for (k, p) in pieces.iter().enumerate() {
        let runs = by_track.entry(p.track).or_default();
        if !kept[k] {
            continue;
        }
        match runs.last_mut() {
            Some(last) if last.1.distance(p.from) < SAME => last.1 = p.to,
            _ => runs.push((p.from, p.to)),
        }
    }
    let mut out: Vec<Track> = Vec::with_capacity(board.tracks.len());
    for (k, t) in board.tracks.iter().enumerate() {
        match by_track.get(&k) {
            None => out.push(t.clone()),
            Some(runs) => {
                let whole = runs.len() == 1
                    && runs[0].0.distance(t.from) < SAME
                    && runs[0].1.distance(t.to) < SAME;
                if whole {
                    out.push(t.clone());
                } else {
                    out.extend(runs.iter().map(|&(from, to)| Track {
                        from,
                        to,
                        ..t.clone()
                    }));
                }
            }
        }
    }
    board.tracks = out;
    dropped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testlib::{fixtures, led_design};

    fn routed_led() -> Board {
        let mut board = crate::board_from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        assert!(crate::route(&mut board).is_empty());
        board
    }

    #[test]
    fn a_connection_drawn_twice_loses_its_second_way() {
        let mut board = routed_led();
        let before = board.tracks.clone();
        let longest = (0..before.len())
            .max_by(|&a, &b| before[a].length().total_cmp(&before[b].length()))
            .unwrap();
        board.tracks.push(before[longest].clone());
        assert_eq!(drop_second_ways(&mut board, 0, &[]), 1);
        assert_eq!(board.tracks.len(), before.len());
        assert!(netsmith_pcb::Ratsnest::of(&board).is_empty());
    }

    #[test]
    fn copper_from_before_the_routing_is_kept() {
        let mut board = routed_led();
        let first = board.tracks.len();
        assert_eq!(drop_second_ways(&mut board, first, &[]), 0);
        assert_eq!(board.tracks.len(), first);
    }
}
