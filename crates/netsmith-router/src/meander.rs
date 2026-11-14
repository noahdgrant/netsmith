//! Length-matching meanders for diff pairs (issue #431, S11): the shorter net of a pair is
//! lengthened with a serpentine on one of its straight runs until the two nets' track lengths
//! differ by no more than [`SKEW_TARGET`].
//!
//! **The shape.** A meander is a row of bumps on one side of a straight track, each bump a
//! trapezoid with 45 degree corners: out along a diagonal, a leg straight out, a diagonal to
//! the top, the top, and the same back down to the track. The legs of a bump and of the next are
//! one pitch `g` apart, centre to centre: the track's width and an edge gap of
//! [`SPACING_WIDTHS`] (3) widths or the net's clearance if larger, so that a meander's own turns
//! do not couple to each other. A corner is cut [`CHAMFER`] (a quarter) of the pitch from the
//! square, which leaves the top and the track between two bumps half a pitch long. A bump lower
//! than half a pitch has no leg, only its diagonals and its top, so a small skew takes a low
//! bump. All the bumps of a meander are as high as each other, at most [`MAX_AMPLITUDE`], and the
//! fewest of them that keep under it are tried first (the shortest meander along the track); then
//! more and lower ones, and the place along the run, and the side, away from the partner first.
//!
//! **Where.** On a straight track of the shorter net, the longest first, no nearer than one pitch
//! to its ends, on an interval of it that runs no nearer than the scorecard's coupled distance
//! (`netsmith_geom::COUPLED`, 0.5 mm) to the partner's track on its layer, so the pair's
//! coupled run is left as it was. The meander's copper keeps every space the design rules check
//! asks of a track from the other nets' copper, holes, the edge, mask openings and rule areas
//! (`netsmith_geom::Room`, the teardrops' room), and the pitch's gap from its own net's pads,
//! vias and other tracks. A run that cannot take all the length takes what it can (three
//! quarters, a half, a quarter) and the next run the rest. No other net moves and no connection
//! changes: the meander replaces the middle of one track with a longer path between the same two
//! points.
//!
//! **The target.** [`SKEW_TARGET`] 0.1 mm, the router's grid: a meander is sized to add the
//! whole skew, so a pair that has the room ends within a few µm. USB 2.0 high speed asks for a
//! skew of a few mm at most (100 ps), so 0.1 mm leaves margin everywhere netsmith's pairs go
//! (decided by the implementing agent under the owner's delegation, 2026-11-13). A pair with
//! no room keeps the skew it has, and [`add`] says what each pair was left with.
//!
//! The router draws them (`netsmith_router::route::route_with`) once the board is routed,
//! before the vias beside vias, the stitching of pours, the teardrops and the arcs at bends, so
//! those are drawn round the meander and its turns are rounded; it measures an arc along its
//! curve and draws meanders on straight tracks only, so it can top a pair up after the arcs. The
//! same board gives the same meanders: pairs, runs and places are taken in a fixed order.

use netsmith_geom::{DrcRules, DrcShape, Pt as FPt, Room, shape_gap};

use netsmith_pcb::{Board, Pt, Track};

/// The skew a diff pair is matched to, in mm: a pair whose nets' track lengths differ by more
/// gets a meander on its shorter net.
pub const SKEW_TARGET: f64 = 0.1;
/// The gap between two neighbouring legs of a meander, in track widths (the 3W rule); the
/// net's clearance where that is larger.
pub const SPACING_WIDTHS: f64 = 3.0;
/// The highest a meander's bump stands off its track, centre to centre, in mm.
pub const MAX_AMPLITUDE: f64 = 2.0;
/// How far from the square a meander's corner is cut, as a share of the pitch between legs.
pub const CHAMFER: f64 = 0.25;
/// The shares of the length still wanted that a run is asked for, in turn, when it has no room
/// for all of it.
const SHARES: [f64; 4] = [1.0, 0.75, 0.5, 0.25];
/// The least length a meander adds, in mm: below it, nothing is drawn.
const LEAST: f64 = 0.02;
/// Where along a run a meander is tried, at most: evenly spread over the room it has.
const PLACES: usize = 24;

/// A diff pair and its skew before and after the meanders.
#[derive(Debug, Clone, PartialEq)]
pub struct Matched {
    /// The two nets, in the order of `netsmith_geom::PAIR_SUFFIXES` (`dp` then `dm`).
    pub p: String,
    pub n: String,
    /// The difference of the two nets' track lengths before, in mm.
    pub before: f64,
    /// The same after.
    pub after: f64,
}

/// The diff pairs of the board's routed nets: two nets whose names differ only in a suffix pair
/// of `netsmith_geom::PAIR_SUFFIXES`, in the order of their first net's name.
fn pairs(board: &Board) -> Vec<(String, String)> {
    let mut nets: Vec<&str> = board.tracks.iter().map(|t| t.net.as_str()).collect();
    nets.sort_unstable();
    nets.dedup();
    let mut out = Vec::new();
    for &p in &nets {
        for (sp, sn) in netsmith_geom::PAIR_SUFFIXES {
            let Some(stem) = p.strip_suffix(sp) else {
                continue;
            };
            let n = format!("{stem}{sn}");
            if nets.contains(&n.as_str()) {
                out.push((p.to_string(), n));
                break;
            }
        }
    }
    out
}

fn length(board: &Board, net: &str) -> f64 {
    board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .map(track_length)
        .sum()
}

/// The length of a track's centre line, an arc's along its chords of at most 2 µm error.
fn track_length(t: &Track) -> f64 {
    t.centre_line()
        .windows(2)
        .map(|w| w[0].distance(w[1]))
        .sum()
}

/// The skew of each diff pair of the board, in mm, in the order [`add`] takes them.
pub fn skews(board: &Board) -> Vec<f64> {
    pairs(board)
        .iter()
        .map(|(p, n)| (length(board, p) - length(board, n)).abs())
        .collect()
}

/// Lengthen the shorter net of each diff pair of the routed board with a meander, where it has
/// room, and say what skew each pair is left with.
pub fn add(board: &mut Board) -> Vec<Matched> {
    let mut out = Vec::new();
    for (p, n) in pairs(board) {
        let (lp, ln) = (length(board, &p), length(board, &n));
        let before = (lp - ln).abs();
        let (short, other) = if lp < ln { (&p, &n) } else { (&n, &p) };
        // A net a routes file locks gets no copper (`netsmith_pcb::Routes`).
        if before > SKEW_TARGET && !board.fixed.is_locked(short) {
            lengthen(board, short, other, before);
        }
        let after = (length(board, &p) - length(board, &n)).abs();
        out.push(Matched {
            p,
            n,
            before,
            after,
        });
    }
    out
}

fn fpt(p: Pt) -> FPt {
    FPt::new(p.x, p.y)
}

/// A meander planned on a track: the points that replace the track's middle, from where it
/// leaves the track to where it comes back, and the length it adds.
struct Plan {
    points: Vec<Pt>,
    added: f64,
}

/// Add up to `want` mm of track to `net` with meanders on its straight runs.
fn lengthen(board: &mut Board, net: &str, partner: &str, want: f64) {
    let Ok(art) = netsmith_pcb::copper_artwork(board) else {
        return;
    };
    let rules = DrcRules::from_project(&netsmith_pcb::kicad_pro(board)).unwrap_or_default();
    let extras = crate::teardrop::extras(board);
    let room = Room::new(&art, &rules, &extras);
    // The copper a routes file fixed (the first tracks) is never changed.
    let mut runs: Vec<usize> = (board.fixed.tracks..board.tracks.len())
        .filter(|&i| board.tracks[i].net == net && board.tracks[i].mid.is_none())
        .collect();
    runs.sort_by(|&a, &b| {
        let len = |i: usize| board.tracks[i].from.distance(board.tracks[i].to);
        len(b).total_cmp(&len(a)).then(a.cmp(&b))
    });
    let partner_tracks: Vec<Track> = board
        .tracks
        .iter()
        .filter(|t| t.net == partner)
        .cloned()
        .collect();
    // The meanders drawn so far on this net, which later ones keep their gap from.
    let mut drawn: Vec<(usize, DrcShape)> = Vec::new();
    let mut left = want;
    // What replaces each track that gets a meander.
    let mut replaced: Vec<(usize, Vec<Pt>)> = Vec::new();
    for i in runs {
        if left <= SKEW_TARGET / 2.0 {
            break;
        }
        let track = board.tracks[i].clone();
        let Some(layer) = room.layer(track.layer.name()) else {
            continue;
        };
        let plan = SHARES.iter().find_map(|share| {
            let ask = left * share;
            (ask >= LEAST)
                .then(|| plan(&room, &track, layer, ask, &partner_tracks, &drawn))
                .flatten()
        });
        if let Some(plan) = plan {
            let shape = DrcShape::stroke(
                plan.points.iter().map(|&p| fpt(p)).collect(),
                track.width / 2.0,
            );
            drawn.push((layer, shape));
            left -= plan.added;
            replaced.push((i, plan.points));
        }
    }
    // Each track with a meander becomes the track to the meander, the meander, and the track on.
    replaced.sort_by_key(|r| std::cmp::Reverse(r.0));
    for (i, points) in replaced {
        let t = board.tracks.remove(i);
        let mut path = vec![t.from];
        path.extend(points);
        path.push(t.to);
        let pieces: Vec<Track> = path
            .windows(2)
            .filter(|w| w[0].distance(w[1]) > 1e-9)
            .map(|w| Track::straight(&t.net, t.layer, w[0], w[1], t.width))
            .collect();
        board.tracks.splice(i..i, pieces);
    }
}

/// The least distance from `p` to a track of `tracks` on `layer`, centre line to centre line.
fn distance_to(tracks: &[Track], layer: netsmith_pcb::CopperLayer, p: Pt) -> f64 {
    tracks
        .iter()
        .filter(|t| t.layer == layer)
        .map(|t| t.distance_to(p))
        .fold(f64::INFINITY, f64::min)
}

/// The length one bump of height `a` adds, with legs `g` apart: its two diagonals of each
/// corner and two legs, less the run of track they stand over.
fn bump_adds(a: f64, g: f64) -> f64 {
    let c = (CHAMFER * g).min(a / 2.0);
    2.0 * a - 4.0 * c * (2.0 - std::f64::consts::SQRT_2)
}

/// The height of a bump that adds `len`, the inverse of [`bump_adds`].
fn bump_height(len: f64, g: f64) -> f64 {
    let low = 2.0 * CHAMFER * g;
    if len <= bump_adds(low, g) {
        // No leg: the two diagonals of each side add (2 sqrt 2 - 2) of the height.
        len / (2.0 * std::f64::consts::SQRT_2 - 2.0)
    } else {
        (len + 4.0 * CHAMFER * g * (2.0 - std::f64::consts::SQRT_2)) / 2.0
    }
}

/// The points of `count` bumps of height `a`, legs `g` apart, along `u` from `start` on the
/// side `v`: from the first point that leaves the track to the last that comes back.
fn bumps(start: Pt, u: Pt, v: Pt, count: usize, a: f64, g: f64) -> Vec<Pt> {
    let c = (CHAMFER * g).min(a / 2.0);
    let top = g - 2.0 * CHAMFER * g;
    let at = |x: f64, y: f64| Pt::new(start.x + u.x * x + v.x * y, start.y + u.y * x + v.y * y);
    let mut pts = Vec::new();
    let mut x = 0.0;
    for k in 0..count {
        if k > 0 {
            x += top;
        }
        // A bump with no leg is one diagonal each way, with no joint along it.
        let leg = a - 2.0 * c > 1e-9;
        pts.push(at(x, 0.0));
        if leg {
            pts.push(at(x + c, c));
            pts.push(at(x + c, a - c));
        }
        pts.push(at(x + 2.0 * c, a));
        pts.push(at(x + 2.0 * c + top, a));
        if leg {
            pts.push(at(x + 3.0 * c + top, a - c));
            pts.push(at(x + 3.0 * c + top, c));
        }
        x += 4.0 * c + top;
        pts.push(at(x, 0.0));
    }
    pts
}

/// The length along the track that `count` bumps of height `a` take.
fn span(count: usize, a: f64, g: f64) -> f64 {
    let c = (CHAMFER * g).min(a / 2.0);
    let top = g - 2.0 * CHAMFER * g;
    count as f64 * (4.0 * c + top) + count.saturating_sub(1) as f64 * top
}

/// A meander on `track` that adds `want` mm, where it keeps every space, or `None`.
fn plan(
    room: &Room<'_>,
    track: &Track,
    layer: usize,
    want: f64,
    partner: &[Track],
    drawn: &[(usize, DrcShape)],
) -> Option<Plan> {
    let w = track.width;
    let gap = (SPACING_WIDTHS * w).max(room.clearance(&track.net));
    let g = w + gap;
    let length = track.from.distance(track.to);
    let usable = length - 2.0 * g;
    if usable <= 0.0 {
        return None;
    }
    let u = Pt::new(
        (track.to.x - track.from.x) / length,
        (track.to.y - track.from.y) / length,
    );
    // The side away from the partner first.
    let normal = Pt::new(-u.y, u.x);
    let mid = Pt::new(
        (track.from.x + track.to.x) / 2.0,
        (track.from.y + track.to.y) / 2.0,
    );
    let toward = |v: Pt| distance_to(partner, track.layer, Pt::new(mid.x + v.x, mid.y + v.y));
    let flip = Pt::new(-normal.x, -normal.y);
    let sides = if toward(normal) >= toward(flip) {
        [normal, flip]
    } else {
        [flip, normal]
    };
    let along = |x: f64| Pt::new(track.from.x + u.x * x, track.from.y + u.y * x);
    // Whether the track between `x0` and `x1` runs coupled to the partner.
    let coupled = |x0: f64, x1: f64| {
        let steps = ((x1 - x0) / 0.05).ceil().max(1.0) as usize;
        (0..=steps).any(|k| {
            let p = along(x0 + (x1 - x0) * k as f64 / steps as f64);
            distance_to(partner, track.layer, p) <= netsmith_geom::COUPLED
        })
    };
    let most = (usable / (2.0 * g)).floor() as usize + 1;
    for count in 1..=most {
        let a = bump_height(want / count as f64, g);
        if a > MAX_AMPLITUDE {
            continue;
        }
        let s = span(count, a, g);
        if s > usable {
            break;
        }
        let slack = usable - s;
        let places = ((slack / (g / 2.0)).floor() as usize + 1).min(PLACES);
        // From the middle of the run outwards.
        let mut offsets: Vec<f64> = (0..places)
            .map(|k| {
                if places == 1 {
                    slack / 2.0
                } else {
                    slack * k as f64 / (places - 1) as f64
                }
            })
            .collect();
        offsets.sort_by(|x, y| {
            (x - slack / 2.0)
                .abs()
                .total_cmp(&(y - slack / 2.0).abs())
                .then(x.total_cmp(y))
        });
        for x0 in offsets {
            let x0 = g + x0;
            if coupled(x0, x0 + s) {
                continue;
            }
            for v in sides {
                let points = bumps(along(x0), u, v, count, a, g);
                let path: Vec<FPt> = points.iter().map(|&p| fpt(p)).collect();
                let shape = DrcShape::stroke(path.clone(), w / 2.0);
                if !room.fits(&shape, &track.net, layer, false) {
                    continue;
                }
                // Off the baseline, the bumps keep the pitch's gap from the net's own copper.
                let raised: Vec<FPt> = path
                    .iter()
                    .copied()
                    .filter(|p| {
                        let d = FPt::new(p.x - track.from.x, p.y - track.from.y);
                        (d.x * v.x + d.y * v.y).abs() > 1e-9
                    })
                    .collect();
                let own = DrcShape::stroke(raised, w / 2.0);
                let from = fpt(track.from);
                let to = fpt(track.to);
                let same = |a: FPt, b: FPt| {
                    (a.dist(from) < 1e-4 && b.dist(to) < 1e-4)
                        || (a.dist(to) < 1e-4 && b.dist(from) < 1e-4)
                };
                if room.own_gap(&own, &track.net, layer, same) < gap {
                    continue;
                }
                if drawn
                    .iter()
                    .any(|(l, d)| *l == layer && shape_gap(&own, d).d < gap)
                {
                    continue;
                }
                let added = count as f64 * bump_adds(a, g);
                return Some(Plan { points, added });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_geom::{DrcRule, drc_check_artwork};
    use netsmith_pcb::{BoardNet, CopperLayer, Rect};

    /// The LED board grown to the right, with the nets `usb.dp` and `usb.dm` and no tracks.
    fn board() -> Board {
        let mut board = crate::board_from_design(
            &crate::testlib::led_design(),
            &netsmith_kicad::Mapping::standard(),
            &crate::testlib::fixtures(),
        )
        .unwrap();
        board.outline = Rect {
            min: board.outline.min,
            max: Pt::new(board.outline.max.x + 40.0, board.outline.max.y + 20.0),
        };
        for name in ["usb.dp", "usb.dm"] {
            board.nets.push(BoardNet {
                name: name.into(),
                pads: Vec::new(),
            });
        }
        board.tracks.clear();
        board
    }

    fn track(net: &str, from: (f64, f64), to: (f64, f64), width: f64) -> Track {
        Track::straight(
            net,
            CopperLayer::Front,
            Pt::new(from.0, from.1),
            Pt::new(to.0, to.1),
            width,
        )
    }

    /// A path of tracks of `net` through `points`.
    fn path(board: &mut Board, net: &str, points: &[(f64, f64)]) {
        let w = board.rules.track_width;
        for p in points.windows(2) {
            board.tracks.push(track(net, p[0], p[1], w));
        }
    }

    /// The check's findings on the pair's copper, as rule and message, but for the ends that
    /// touch nothing (these tracks join no pads).
    fn findings(board: &Board) -> Vec<String> {
        let art = netsmith_pcb::copper_artwork(board).unwrap();
        let rules = DrcRules::from_project(&netsmith_pcb::kicad_pro(board)).unwrap();
        let mut extras = crate::teardrop::extras(board);
        extras.track_angle = true;
        drc_check_artwork(&art, &rules, &extras, false)
            .findings
            .iter()
            .filter(|f| f.rule != DrcRule::TrackDangling)
            .filter(|f| {
                f.items
                    .iter()
                    .any(|i| i.net.as_deref().is_some_and(|n| n.starts_with("usb")))
            })
            .map(|f| format!("{:?}: {}", f.rule, f.message))
            .collect()
    }

    /// `usb.dp` straight; `usb.dm` beside it, then out `jog` mm and back on 45 degree jogs,
    /// which makes it 0.83 `jog` longer.
    fn skewed_pair(board: &mut Board, jog: f64) {
        let (x, y) = (board.outline.max.x - 30.0, board.outline.max.y - 10.0);
        let gap = board.rules.track_width + board.rules.clearance + 0.05;
        path(board, "usb.dp", &[(x, y), (x + 20.0, y)]);
        path(
            board,
            "usb.dm",
            &[
                (x, y - gap),
                (x + 5.0, y - gap),
                (x + 5.0 + jog, y - gap - jog),
                (x + 15.0 - jog, y - gap - jog),
                (x + 15.0, y - gap),
                (x + 20.0, y - gap),
            ],
        );
    }

    #[test]
    fn the_shorter_net_of_a_skewed_pair_gets_a_meander_that_matches_it_and_keeps_every_space() {
        let mut board = board();
        skewed_pair(&mut board, 2.0);
        let before = findings(&board);
        let matched = add(&mut board);
        assert_eq!(matched.len(), 1, "{matched:?}");
        let m = &matched[0];
        assert_eq!((m.p.as_str(), m.n.as_str()), ("usb.dp", "usb.dm"));
        assert!(
            (m.before - 4.0 * (2.0f64.sqrt() - 1.0)).abs() < 1e-6,
            "{m:?}"
        );
        assert!(m.after <= SKEW_TARGET, "{m:?}");
        // The meander is on the shorter net, below it (away from its partner), and between the
        // coupled runs.
        let dp: Vec<&Track> = board.tracks.iter().filter(|t| t.net == "usb.dp").collect();
        assert!(dp.len() > 3, "{dp:?}");
        let y = dp[0].from.y;
        assert!(
            dp.iter()
                .all(|t| t.from.y >= y - 1e-9 && t.to.y >= y - 1e-9)
        );
        let x0 = dp[0].from.x;
        assert!(
            dp.iter()
                .filter(|t| (t.from.y - y).abs() > 1e-9 || (t.to.y - y).abs() > 1e-9)
                .all(|t| t.from.x > x0 + 5.5 && t.to.x < x0 + 14.5)
        );
        assert_eq!(findings(&board), before);
    }

    #[test]
    fn a_small_skew_takes_low_bumps_with_no_straight_joint() {
        let mut board = board();
        skewed_pair(&mut board, 0.3);
        let m = &add(&mut board)[0];
        assert!(m.before > SKEW_TARGET && m.after <= SKEW_TARGET, "{m:?}");
        let dp: Vec<&Track> = board.tracks.iter().filter(|t| t.net == "usb.dp").collect();
        assert!(dp.len() > 3, "{dp:?}");
        for w in dp.windows(2) {
            let (a, b) = (w[0].to - w[0].from, w[1].to - w[1].from);
            assert!((a.x * b.y - a.y * b.x).abs() > 1e-9, "{dp:?}");
        }
    }

    #[test]
    fn a_pair_whose_shorter_net_has_no_room_is_left_as_it_was() {
        let mut board = board();
        let (x, y) = (board.outline.max.x - 30.0, board.outline.max.y - 10.0);
        // `usb.dp` straight between two tracks of another net, as near as the rules allow;
        // `usb.dm` 2 mm longer, far off.
        let gap = board.rules.track_width + board.rules.clearance + 0.01;
        path(&mut board, "usb.dp", &[(x, y), (x + 20.0, y)]);
        path(
            &mut board,
            "GND",
            &[(x - 1.0, y + gap), (x + 21.0, y + gap)],
        );
        path(
            &mut board,
            "GND",
            &[(x - 1.0, y - gap), (x + 21.0, y - gap)],
        );
        path(&mut board, "usb.dm", &[(x, y - 6.0), (x + 22.0, y - 6.0)]);
        let tracks = board.tracks.clone();
        let matched = add(&mut board);
        assert_eq!(matched.len(), 1);
        assert!((matched[0].before - 2.0).abs() < 1e-9);
        assert_eq!(matched[0].after, matched[0].before);
        assert_eq!(board.tracks, tracks);
    }
}
