//! No connection drawn twice (issue #1486): the router routes a net line by line, each from pad
//! to pad, and a line may run where another of its net already runs, or reach a pad or a via the
//! net's copper reaches already. The net's copper then has a loop: two ways between the same two
//! places. Once the board is routed, each loop of a net's new copper loses a piece: the longest
//! piece on a loop first, with what of the line it was on goes nowhere after (a dead end), where
//! that leaves every place where tracks meet outside a pad or a via bent by 45 degrees at most
//! (and three or more at right angles or wider), as `track_angle` asks, and no track ending
//! there alone, on the middle of another, which KiCad's check finds unconnected.
//! The copper a removal leaves is judged as the check judges it ([`faults`]): a place on a
//! via's edge is no place inside it, so a removal that adds a fault there is not made (issue
//! #1518). Taking copper away never
//! comes nearer other copper, and a piece on a loop joins nothing the rest does not.
//! Before the loops, a piece that lies on another, end to end the same, goes (the narrower, or
//! of two as wide the one of the track cut into fewer pieces, else the other), where what it
//! leaves bends as the rest must: two tracks of a net that overlap on one line leave no copy of
//! a track behind (issue #1539) and no bend too sharp (issue #1568).
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
    dropped + drop_exact_copies(board, first, keep)
}

/// A track by its net, layer and two ends, the lesser end first.
type CopyKey = (String, CopperLayer, (i64, i64), (i64, i64));

/// Each straight track drawn from track `first` on (but those in `keep`) that is a copy of
/// another of its net and layer, the same two ends and as wide or narrower, taken off (issue
/// #1568; of two as wide, the first stays, drawn from its lesser end, issue #1572): taking two
/// pieces off a loop may leave what is left of one track the same as another.
/// The other has the same ends, going the same ways, so taking the copy off changes no copper
/// and no bend. Returns how many were taken off.
fn drop_exact_copies(board: &mut Board, first: usize, keep: &[&Track]) -> usize {
    let fixed = first.max(board.fixed.tracks);
    let key = |t: &Track| {
        let q = |p: Pt| ((p.x / SAME).round() as i64, (p.y / SAME).round() as i64);
        let (a, b) = (q(t.from), q(t.to));
        (t.net.clone(), t.layer, a.min(b), a.max(b))
    };
    let mut seen: HashMap<CopyKey, usize> = HashMap::new();
    let mut gone = vec![false; board.tracks.len()];
    let mut canonical: Vec<usize> = Vec::new();
    for k in 0..board.tracks.len() {
        let t = &board.tracks[k];
        if t.mid.is_some() {
            continue;
        }
        let may_go = |k: usize| {
            let t = &board.tracks[k];
            k >= fixed && !board.fixed.is_locked(&t.net) && !keep.contains(&t)
        };
        match seen.get(&key(t)).copied() {
            None => {
                seen.insert(key(t), k);
            }
            Some(j) => {
                let wider = t.width > board.tracks[j].width + 1e-9;
                let (goes, stays) = if wider { (j, k) } else { (k, j) };
                if may_go(goes) {
                    gone[goes] = true;
                    seen.insert(key(t), stays);
                    if !wider {
                        canonical.push(stays);
                    }
                }
            }
        }
    }
    // Of two as wide, the first stays, drawn from its lesser end: which of the two copies was
    // drawn which way (from the order the lines were routed in) changes neither the track left
    // nor where it is, so neither the arcs drawn after it (issue #1572).
    for k in canonical {
        let t = &mut board.tracks[k];
        let q = |p: Pt| ((p.x / SAME).round() as i64, (p.y / SAME).round() as i64);
        if q(t.to) < q(t.from) {
            std::mem::swap(&mut t.from, &mut t.to);
        }
    }
    let count = gone.iter().filter(|g| **g).count();
    if count > 0 {
        let mut k = 0;
        board.tracks.retain(|_| {
            k += 1;
            !gone[k - 1]
        });
    }
    count
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
    // A net with no loop has nothing to take off: most nets, and each is looked at once. (Two
    // pieces that lie one on the other are a loop too.)
    let mut sets: Vec<usize> = (0..count).collect();
    let mut looped = false;
    for p in &pieces {
        let (a, b) = (find(&mut sets, p.a), find(&mut sets, p.b));
        if a == b {
            looped = true;
        } else {
            sets[a] = b;
        }
    }
    if !looped {
        return 0;
    }
    // The pieces at each place.
    let mut by_place: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (k, p) in pieces.iter().enumerate() {
        by_place[p.a].push(k);
        if p.b != p.a {
            by_place[p.b].push(k);
        }
    }
    // Whether `x` reaches `y` over the kept pieces.
    let joined = |kept: &[bool], x: usize, y: usize| {
        let mut seen = vec![false; count];
        let mut todo = vec![x];
        seen[x] = true;
        while let Some(n) = todo.pop() {
            if n == y {
                return true;
            }
            for &k in &by_place[n] {
                let p = &pieces[k];
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
        by_place[n]
            .iter()
            .filter(|&&k| kept[k] && (pieces[k].a == n) != (pieces[k].b == n))
            .count()
    };
    // The bends at a free place, as `track_angle` judges them.
    let bends_well = |kept: &[bool], n: usize| {
        let (layer, at) = free[n - places.len()];
        let mut ways: Vec<(f64, f64)> = Vec::new();
        let mut ends = 0;
        for &k in &by_place[n] {
            let p = &pieces[k];
            if !kept[k] {
                continue;
            }
            // Two kept pieces of one track that meet here are one track, through it.
            let through = by_place[n]
                .iter()
                .any(|&j| j != k && kept[j] && pieces[j].track == p.track);
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
    let mut faulty = faults(
        &rebuilt(&board.tracks, &pieces, &kept, &tracks),
        net,
        &places,
    );
    // First each piece that lies on another (issue #1539), the one [`copies`] names first, then
    // the other, where what it leaves bends as `track_angle` asks: the copper is the same, but
    // the track that loses it may then end where the other bends (issue #1568).
    for choices in copies(board, &pieces) {
        for goes in choices {
            if !pieces[goes].new || choices.iter().any(|&k| !kept[k]) {
                continue;
            }
            let mut trial = kept.clone();
            trial[goes] = false;
            let bends = [pieces[goes].a, pieces[goes].b]
                .iter()
                .all(|&n| !is_free(n) || degree(&trial, n) == 0 || bends_well(&trial, n));
            if !bends {
                continue;
            }
            let left = faults(
                &rebuilt(&board.tracks, &pieces, &trial, &tracks),
                net,
                &places,
            );
            if left > faulty {
                continue;
            }
            faulty = left;
            dropped += 1;
            kept = trial;
            break;
        }
    }
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
            let Some(j) = by_place[n].iter().copied().find(|&j| trial[j]) else {
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
            // The copper it leaves, judged where its tracks end as the check judges it: in a
            // pad or a via only when clearly inside.
            let left = faults(
                &rebuilt(&board.tracks, &pieces, &trial, &tracks),
                net,
                &places,
            );
            if left > faulty {
                continue;
            }
            faulty = left;
            dropped += kept.iter().zip(&trial).filter(|(a, b)| **a && !**b).count();
            kept = trial;
        }
    }
    if dropped == 0 {
        return 0;
    }
    let all: Vec<usize> = (0..board.tracks.len()).collect();
    board.tracks = rebuilt(&board.tracks, &pieces, &kept, &all);
    dropped
}

/// Each two pieces that lie one on the other on their layer, end to end the same (two tracks of
/// the net that overlap on one line are cut into such pieces where they lie one on the other),
/// the one to take off first (issue #1539): its copper is the other's, so taking it off changes
/// no copper and leaves no copy of a track. Of two, the narrower; of two as wide, the one of the
/// track cut into fewer pieces (a track wholly on another goes whole), then the later.
fn copies(board: &Board, pieces: &[Piece]) -> Vec<[usize; 2]> {
    let mut by_places: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (k, p) in pieces.iter().enumerate() {
        by_places
            .entry((p.a.min(p.b), p.a.max(p.b)))
            .or_default()
            .push(k);
    }
    let mut cut_into: HashMap<usize, usize> = HashMap::new();
    for p in pieces {
        *cut_into.entry(p.track).or_default() += 1;
    }
    let mut out = Vec::new();
    let mut groups: Vec<&Vec<usize>> = by_places.values().filter(|g| g.len() > 1).collect();
    groups.sort_unstable();
    for group in groups {
        for (i, &x) in group.iter().enumerate() {
            for &y in &group[i + 1..] {
                let (px, py) = (&pieces[x], &pieces[y]);
                let (tx, ty) = (&board.tracks[px.track], &board.tracks[py.track]);
                let one = (px.from.distance(py.from) < SAME && px.to.distance(py.to) < SAME)
                    || (px.from.distance(py.to) < SAME && px.to.distance(py.from) < SAME);
                if !one
                    || tx.layer != ty.layer
                    || tx.mid.is_some()
                    || ty.mid.is_some()
                    || px.track == py.track
                {
                    continue;
                }
                let first = if (tx.width - ty.width).abs() > 1e-9 {
                    if tx.width < ty.width { x } else { y }
                } else if cut_into[&px.track] != cut_into[&py.track] {
                    if cut_into[&px.track] < cut_into[&py.track] {
                        x
                    } else {
                        y
                    }
                } else {
                    y
                };
                let wider = (tx.width - ty.width).abs() > 1e-9;
                // The wider of two is never taken off: the narrower's copper is not all of it.
                if wider {
                    out.push([first, first]);
                } else {
                    out.push([first, if first == x { y } else { x }]);
                }
            }
        }
    }
    out
}

fn find(sets: &mut [usize], mut a: usize) -> usize {
    while sets[a] != a {
        sets[a] = sets[sets[a]];
        a = sets[a];
    }
    a
}

/// The tracks of `tracks` numbered in `only`, each track of `pieces` as the pieces of it that
/// are `kept`, those in a row as one.
fn rebuilt(tracks: &[Track], pieces: &[Piece], kept: &[bool], only: &[usize]) -> Vec<Track> {
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
    let mut out: Vec<Track> = Vec::with_capacity(only.len());
    for &k in only {
        let t = &tracks[k];
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
    out
}

/// How many places where tracks of `net` end, outside its pads and vias, `track_angle` finds a
/// fault at: two that bend by more than 45 degrees, or three or more at an acute angle. A place
/// is in a pad or a via only when it is clearly inside: a pad's box less what a round pad of its
/// size leaves out at a corner, a via less a micrometre, as the design rules check finds it.
fn faults(tracks: &[Track], net: &str, places: &[Place]) -> usize {
    type Ways = (Pt, Vec<(f64, f64)>);
    let mut ends: HashMap<(CopperLayer, (i64, i64)), Ways> = HashMap::new();
    for t in tracks.iter().filter(|t| t.net == net) {
        for at in [t.from, t.to] {
            let Some(u) = leaving(t, at) else { continue };
            let key = ((at.x * 1e3).round() as i64, (at.y * 1e3).round() as i64);
            let entry = ends.entry((t.layer, key)).or_insert((at, Vec::new()));
            if !entry
                .1
                .iter()
                .any(|v| (v.0 - u.0).abs() < 1e-3 && (v.1 - u.1).abs() < 1e-3)
            {
                entry.1.push(u);
            }
        }
    }
    let inside = |p: Pt, layer: CopperLayer| {
        places.iter().any(|q| match q {
            Place::Pad(r, front, back) => {
                let inset =
                    r.width().min(r.height()) / 2.0 * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
                Place::Pad(r.expanded(-inset - SAME), *front, *back).holds(p, layer)
            }
            Place::Via(c, r) => c.distance(p) <= r - 1e-3,
        })
    };
    ends.iter()
        .filter(|((layer, _), (at, ways))| {
            if ways.len() < 2 || inside(*at, *layer) {
                return false;
            }
            let least = (0..ways.len())
                .flat_map(|i| (i + 1..ways.len()).map(move |j| (i, j)))
                .map(|(i, j)| {
                    let dot = ways[i].0 * ways[j].0 + ways[i].1 * ways[j].1;
                    dot.clamp(-1.0, 1.0).acos().to_degrees()
                })
                .fold(180.0, f64::min);
            least < if ways.len() == 2 { 134.0 } else { 89.0 }
        })
        .count()
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
    fn a_track_that_lies_on_half_of_another_goes_and_leaves_no_copy() {
        // Issue #1539: the two were cut into pieces that lie one on the other, and the piece of
        // the longer track was taken off, leaving its other half and a copy of the shorter.
        let mut board = routed_led();
        let before = board.tracks.clone();
        let longest = (0..before.len())
            .filter(|&k| before[k].mid.is_none())
            .max_by(|&a, &b| before[a].length().total_cmp(&before[b].length()))
            .unwrap();
        let t = &before[longest];
        let half = Pt::new((t.from.x + t.to.x) / 2.0, (t.from.y + t.to.y) / 2.0);
        board.tracks.push(Track {
            to: half,
            ..t.clone()
        });
        assert_eq!(drop_second_ways(&mut board, 0, &[]), 1);
        assert_eq!(board.tracks, before);
    }

    #[test]
    fn copper_from_before_the_routing_is_kept() {
        let mut board = routed_led();
        let first = board.tracks.len();
        assert_eq!(drop_second_ways(&mut board, first, &[]), 0);
        assert_eq!(board.tracks.len(), first);
    }
}
