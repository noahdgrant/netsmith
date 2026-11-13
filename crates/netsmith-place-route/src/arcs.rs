//! Arcs at a track's bends (issue #431, step 11): where two straight tracks of a net meet at a bend
//! of 45 degrees or less, the corner is replaced by an arc tangent to both, so the copper has no
//! corner at all. The router's search and its copper are as before; this runs on the finished
//! board, after the parallel and the stitching vias and the teardrops, and keeps an arc only where
//! it passes the same exact clearance check as the router's clean-up ([`Router::settled`] in
//! `route.rs`). A bend that no arc fits keeps its corner, which `track_angle` allows.
//!
//! **The radius.** For each bend the largest radius up to [`ARC_WIDTHS`] times the track's width is
//! tried first, then three quarters of it, and so on down to one width; the first that keeps the
//! rules' space from every other net's copper, the pads, the holes, the keepouts, the teardrops and
//! the board edge is drawn. It runs after the teardrops and keeps them as they are: an arc never
//! cuts into the part of a track a teardrop lies along. An arc takes at most half of each straight
//! track it joins, so two arcs at the two ends of one track never meet, and the short diagonal of a
//! cut right angle (0.05 to 0.3 mm, `CHAMFERS` in `route.rs`) takes two small arcs. Only a bend
//! where exactly two straight tracks of one width end is rounded, outside the net's pads and vias,
//! and where no other track of the net joins the corner it takes away, so an arc never breaks a
//! join.
//!
//! An arc moves copper towards the inside of its bend (by about 0.08 of the radius at its
//! middle) and away from the outside, which is why it is checked; it is checked as chords no
//! further than [`ARC_ERROR`] from it, with that kept as extra space, which also covers the
//! design rules check's own chords. A bend whose tracks run at the rules' space from another
//! net's copper (within [`ARC_ERROR`] of it) keeps its corner.
//!
//! **Decided** (the agent, under the owner's delegation of 2026-11-11, measured 2026-11-13):
//! arcs are on for every board netsmith routes ([`RouteOptions::arcs`]), with a radius of at
//! most [`ARC_WIDTHS`] widths. The measurements are in the guide's "How tracks bend".
//!
//! [`Router::settled`]: crate::route
//! [`RouteOptions::arcs`]: crate::RouteOptions::arcs

use std::collections::HashMap;

use netsmith_fab::geom::ARC_ERROR;
use netsmith_pcb::board::{Board, CopperLayer, Track};
use netsmith_pcb::geometry::{Pt, Rect};

use crate::route::{EXACT_SLACK, Form, MARGIN, Obstacle, initial_obstacles};

/// The largest radius of an arc at a bend, in widths of its track.
pub const ARC_WIDTHS: f64 = 4.0;

/// Each radius tried after the first is this much of the one before.
const SHRINK: f64 = 0.75;

/// The bends rounded: a turn of more than this, in degrees, and of at most [`MOST_TURN`].
const LEAST_TURN: f64 = 1.0;
const MOST_TURN: f64 = 45.5;

/// A place where tracks end: the net, whether on the back, and the place to 0.1 µm.
type Joint<'a> = (&'a str, bool, (i64, i64));

/// Round the bends of `board`'s tracks with arcs of radius at most `widths` track widths (the
/// module's header). Returns how many arcs were drawn.
pub fn round_bends(board: &mut Board, widths: f64) -> usize {
    if widths <= 0.0 || board.tracks.is_empty() {
        return 0;
    }
    let rules = &board.rules;
    let net_of: HashMap<&str, usize> = board
        .nets
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name.as_str(), i))
        .collect();
    let net_index = |name: &str| net_of.get(name).copied();
    let fixed = initial_obstacles(board).list;
    let clearance = rules.clearance + MARGIN;
    let layers_of = |layer: CopperLayer| [layer == CopperLayer::Front, layer == CopperLayer::Back];
    // The copper of each track as obstacles, kept as the tracks change; the vias.
    let copper = |t: &Track| -> Vec<Obstacle> {
        let net = net_index(&t.net);
        let line = t.centre_line();
        // An arc's chords are within `ARC_ERROR` of it.
        let half = t.width / 2.0 + if t.mid.is_some() { ARC_ERROR } else { 0.0 };
        line.windows(2)
            .map(|w| Obstacle {
                net,
                layers: layers_of(t.layer),
                form: Form::Segment(w[0], w[1], half),
                clearance,
            })
            .collect()
    };
    let mut tracks: Vec<Vec<Obstacle>> = board.tracks.iter().map(copper).collect();
    // The vias, and the teardrops as their edges: an arc starts on a track of its own net, so it
    // reaches another net's teardrop only across an edge.
    let vias: Vec<Obstacle> = board
        .vias
        .iter()
        .map(|v| Obstacle {
            net: net_index(&v.net),
            layers: [true, true],
            form: Form::Circle(v.at, v.diameter / 2.0),
            clearance,
        })
        .chain(board.teardrops.iter().flat_map(|d| {
            let net = net_index(&d.net);
            let layers = layers_of(d.layer);
            let n = d.points.len();
            (0..n).map(move |k| Obstacle {
                net,
                layers,
                form: Form::Segment(d.points[k], d.points[(k + 1) % n], 0.0),
                clearance,
            })
        }))
        .collect();
    // How much of each track, from each end, a teardrop of its net lies along: an arc at the
    // other end leaves that much straight.
    let teardrop_on = |t: &Track, from: Pt| -> f64 {
        let length = t.from.distance(t.to);
        if length < 1e-9 {
            return 0.0;
        }
        let u = Pt::new((t.to.x - t.from.x) / length, (t.to.y - t.from.y) / length);
        board
            .teardrops
            .iter()
            .filter(|d| d.net == t.net && d.layer == t.layer)
            .flat_map(|d| d.points.iter())
            .filter(|&&p| t.distance_to(p) <= t.width / 2.0 + 1e-3)
            .map(|&p| {
                let along = (p.x - t.from.x) * u.x + (p.y - t.from.y) * u.y;
                if from == t.from {
                    along
                } else {
                    length - along
                }
            })
            .fold(0.0, f64::max)
    };

    // The ends of the straight tracks, by net, layer and place.
    let key = |p: Pt| ((p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64);
    let mut ends: HashMap<Joint, Vec<(usize, bool)>> = HashMap::new();
    for (i, t) in board.tracks.iter().enumerate() {
        for (at, is_to) in [(t.from, false), (t.to, true)] {
            ends.entry((t.net.as_str(), t.layer == CopperLayer::Back, key(at)))
                .or_default()
                .push((i, is_to));
        }
    }
    let mut bends: Vec<[(usize, bool); 2]> = ends
        .into_values()
        .filter_map(|e| match e.as_slice() {
            &[a, b] if a.0 != b.0 => Some([a, b]),
            _ => None,
        })
        .collect();
    // The same board gives the same copper.
    bends.sort();
    let lengths: Vec<f64> = board.tracks.iter().map(Track::length).collect();
    let mut drawn = 0;
    for [(a, a_to), (b, b_to)] in bends {
        let (ta, tb) = (&board.tracks[a], &board.tracks[b]);
        if ta.mid.is_some() || tb.mid.is_some() || (ta.width - tb.width).abs() > 1e-9 {
            continue;
        }
        let end = |t: &Track, to: bool| if to { (t.to, t.from) } else { (t.from, t.to) };
        let ((corner, a_far), (_, b_far)) = (end(ta, a_to), end(tb, b_to));
        let (u, v) = (a_far - corner, b_far - corner);
        let (la, lb) = (u.x.hypot(u.y), v.x.hypot(v.y));
        if la < 1e-6 || lb < 1e-6 {
            continue;
        }
        let (u, v) = (Pt::new(u.x / la, u.y / la), Pt::new(v.x / lb, v.y / lb));
        let between = (u.x * v.x + u.y * v.y).clamp(-1.0, 1.0).acos();
        let turn = std::f64::consts::PI - between;
        if turn.to_degrees() <= LEAST_TURN || turn.to_degrees() > MOST_TURN {
            continue;
        }
        let net = net_index(&ta.net);
        let layer = usize::from(ta.layer == CopperLayer::Back);
        let width = ta.width;
        let half_turn = (turn / 2.0).tan();
        // At most half of each track, less a few micrometres, so the arcs at its two ends stay
        // apart and neither is taken for a join on the other's corner.
        // Nor into a teardrop at a track's other end.
        let room = |t: &Track, far: Pt, length: f64| {
            (length / 2.0).min(length - teardrop_on(t, far)) - 0.005
        };
        let reach = room(ta, a_far, lengths[a])
            .min(room(tb, b_far, lengths[b]))
            .min(widths * width * half_turn);
        // The corner the arc takes away: no other track of the net may join it there, nor a pad
        // or a via of the net cover it, so the arc breaks no join.
        let (pa, pb) = (along(corner, u, reach), along(corner, v, reach));
        let own_near = |o: &Obstacle| {
            let touch = match o.form {
                Form::Segment(c, d, _) => Form::Segment(c, d, 0.0),
                form => form,
            };
            o.net == net
                && o.layers[layer]
                && [pa, pb]
                    .iter()
                    .any(|&p| touch.distance_to_line(corner, p) <= 1e-3)
        };
        if fixed.iter().chain(&vias).any(own_near)
            || tracks
                .iter()
                .enumerate()
                .any(|(k, obs)| k != a && k != b && obs.iter().any(own_near))
        {
            continue;
        }
        let others = |o: &&Obstacle| o.net != net && o.layers[layer];
        let mut radius = reach / half_turn;
        let mut found = None;
        while radius >= width - 1e-9 {
            let arc = Bend::new(corner, u, v, turn, radius);
            if arc.clear(
                width,
                fixed.iter().chain(&vias).filter(others),
                tracks.iter().flatten().filter(others),
            ) {
                found = Some(arc);
                break;
            }
            radius *= SHRINK;
        }
        let Some(arc) = found else { continue };
        for (k, to, at) in [(a, a_to, arc.from), (b, b_to, arc.to)] {
            let t = &mut board.tracks[k];
            if to {
                t.to = at;
            } else {
                t.from = at;
            }
            tracks[k] = copper(t);
        }
        let track = Track {
            mid: Some(arc.mid),
            ..Track::straight(
                &board.tracks[a].net,
                board.tracks[a].layer,
                arc.from,
                arc.to,
                width,
            )
        };
        tracks.push(copper(&track));
        board.tracks.push(track);
        drawn += 1;
    }
    drawn
}

/// The point `by` along the direction `d` from `p`.
fn along(p: Pt, d: Pt, by: f64) -> Pt {
    Pt::new(p.x + d.x * by, p.y + d.y * by)
}

/// An arc tangent to the two tracks of a bend.
struct Bend {
    from: Pt,
    mid: Pt,
    to: Pt,
    centre: Pt,
    radius: f64,
}

impl Bend {
    /// The arc of `radius` at `corner`, between the track that leaves it along `u` and the one
    /// that leaves along `v` (unit directions), which turn by `turn` radians.
    fn new(corner: Pt, u: Pt, v: Pt, turn: f64, radius: f64) -> Bend {
        let reach = radius * (turn / 2.0).tan();
        let bisector = u + v;
        let n = bisector.x.hypot(bisector.y);
        let bisector = Pt::new(bisector.x / n, bisector.y / n);
        let centre = along(corner, bisector, radius / (turn / 2.0).cos());
        Bend {
            from: along(corner, u, reach),
            mid: along(centre, bisector, -radius),
            to: along(corner, v, reach),
            centre,
            radius,
        }
    }

    /// The arc's centre line as chords no further than [`ARC_ERROR`] from it.
    fn chords(&self) -> Vec<Pt> {
        let angle = |p: Pt| (p.y - self.centre.y).atan2(p.x - self.centre.x);
        let a0 = angle(self.from);
        let tau = std::f64::consts::TAU;
        let mut sweep = (angle(self.to) - a0).rem_euclid(tau);
        if sweep > std::f64::consts::PI {
            sweep -= tau;
        }
        let steps = netsmith_fab::geom::steps_for(self.radius, sweep.abs());
        let mut out = vec![self.from];
        for k in 1..steps {
            let a = a0 + sweep * k as f64 / steps as f64;
            out.push(Pt::new(
                self.centre.x + self.radius * a.cos(),
                self.centre.y + self.radius * a.sin(),
            ));
        }
        out.push(self.to);
        out
    }

    /// Whether the arc's copper, `width` wide, keeps the rules' space from every obstacle.
    fn clear<'a>(
        &self,
        width: f64,
        fixed: impl Iterator<Item = &'a Obstacle>,
        copper: impl Iterator<Item = &'a Obstacle>,
    ) -> bool {
        let chords = self.chords();
        let Some(reach) = Rect::around(chords.iter().copied()) else {
            return true;
        };
        // The chords are inside the arc by up to `ARC_ERROR`: nearer than the arc to what is
        // inside it, and no nearer than the design rules check's own chords to what is outside.
        let extra = width / 2.0 + ARC_ERROR;
        fixed.chain(copper).all(|o| {
            let apart = o.clearance - MARGIN - EXACT_SLACK;
            !reach
                .expanded(extra + o.clearance)
                .overlaps(&o.form.bounds())
                || chords
                    .windows(2)
                    .all(|w| o.form.distance_to_line(w[0], w[1]) - extra >= apart)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The LED board with its parts taken away: an open board with nets `GND` and `VCC`.
    fn open_board() -> Board {
        let mut board = crate::board_from_design(
            &crate::testlib::led_design(),
            &netsmith_kicad::Mapping::standard(),
            &crate::testlib::fixtures(),
        )
        .unwrap();
        board.parts.clear();
        board
    }

    /// A track of `net` on the front from `a` through `b` to `c`, as two straight tracks.
    fn bent(board: &mut Board, net: &str, [a, b, c]: [Pt; 3], width: f64) {
        for (from, to) in [(a, b), (b, c)] {
            board
                .tracks
                .push(Track::straight(net, CopperLayer::Front, from, to, width));
        }
    }

    /// Its middle, and the three points of a 45 degree bend there: in from 3 mm to the left,
    /// out 3 mm down and to the right.
    fn bend_at(board: &Board) -> [Pt; 3] {
        let c = board.outline.centre();
        let d = 3.0 * std::f64::consts::FRAC_1_SQRT_2;
        [Pt::new(c.x - 3.0, c.y), c, Pt::new(c.x + d, c.y + d)]
    }

    #[test]
    fn a_bend_gets_a_tangent_arc_of_the_largest_radius() {
        let mut board = open_board();
        let width = 0.2;
        let points = bend_at(&board);
        bent(&mut board, "GND", points, width);
        assert_eq!(round_bends(&mut board, ARC_WIDTHS), 1);
        assert_eq!(board.tracks.len(), 3);
        let arc = board.tracks.iter().find_map(Track::arc).expect("an arc");
        assert!(
            (arc.radius - ARC_WIDTHS * width).abs() < 1e-6,
            "{}",
            arc.radius
        );
        // It replaces as much of each straight track as the radius needs, and joins their new
        // ends.
        let cut = ARC_WIDTHS * width * std::f64::consts::FRAC_PI_8.tan();
        let corner = points[1];
        for t in &board.tracks[..2] {
            let near = if t.from.distance(corner) < t.to.distance(corner) {
                t.from
            } else {
                t.to
            };
            assert!((near.distance(corner) - cut).abs() < 1e-9);
        }
        let ends = [board.tracks[2].from, board.tracks[2].to];
        assert!(ends.contains(&board.tracks[0].to) && ends.contains(&board.tracks[1].from));
        // Off: no arc.
        let mut board = open_board();
        bent(&mut board, "GND", points, width);
        assert_eq!(round_bends(&mut board, 0.0), 0);
        assert!(board.tracks.iter().all(|t| t.mid.is_none()));
    }

    #[test]
    fn a_short_track_between_two_bends_takes_two_smaller_arcs() {
        let mut board = open_board();
        let c = board.outline.centre();
        // A right angle cut by a diagonal of 0.3 mm: two bends of 45 degrees.
        let d = 0.3 * std::f64::consts::FRAC_1_SQRT_2;
        for (a, b) in [
            (Pt::new(c.x - 3.0, c.y), c),
            (c, Pt::new(c.x + d, c.y + d)),
            (Pt::new(c.x + d, c.y + d), Pt::new(c.x + d, c.y + 3.0)),
        ] {
            board
                .tracks
                .push(Track::straight("GND", CopperLayer::Front, a, b, 0.2));
        }
        assert_eq!(round_bends(&mut board, ARC_WIDTHS), 2);
        // The diagonal keeps a piece: each arc takes at most half of it.
        assert!(board.tracks[1].length() > 1e-6);
    }

    #[test]
    fn a_bend_beside_another_net_at_the_rules_space_keeps_its_corner() {
        let mut board = open_board();
        let width = 0.2;
        let points = bend_at(&board);
        bent(&mut board, "GND", points, width);
        // A track of VCC along the outside of the bend's first track, at the rules' space and a
        // micrometre, on past the corner: the arc leaves the first track where VCC is.
        let space = board.rules.clearance + width + 0.001;
        let [a, corner, _] = points;
        board.tracks.push(Track::straight(
            "VCC",
            CopperLayer::Front,
            Pt::new(a.x, a.y - space),
            Pt::new(corner.x + 1.0, corner.y - space),
            width,
        ));
        assert_eq!(round_bends(&mut board, ARC_WIDTHS), 0);
        assert!(board.tracks.iter().all(|t| t.mid.is_none()));
        // A little further away, the bend takes an arc.
        board.tracks[2].from.y -= 0.01;
        board.tracks[2].to.y -= 0.01;
        assert_eq!(round_bends(&mut board, ARC_WIDTHS), 1);
    }

    #[test]
    fn an_arc_leaves_straight_the_part_of_a_track_a_teardrop_lies_along() {
        let mut board = open_board();
        let points = bend_at(&board);
        bent(&mut board, "GND", points, 0.2);
        // A teardrop on the first track, from its far end to 0.1 mm short of the corner.
        let [a, corner, _] = points;
        let tail = Pt::new(corner.x - 0.1, corner.y);
        board.teardrops.push(netsmith_pcb::board::Teardrop {
            net: "GND".into(),
            layer: CopperLayer::Front,
            points: vec![
                Pt::new(a.x, a.y - 0.3),
                Pt::new(tail.x, tail.y - 0.1),
                Pt::new(tail.x, tail.y + 0.1),
                Pt::new(a.x, a.y + 0.3),
            ],
        });
        assert_eq!(round_bends(&mut board, ARC_WIDTHS), 1);
        // The arc is smaller than four widths, and starts beyond the teardrop's tail.
        let arc = board.tracks.iter().find_map(Track::arc).unwrap();
        assert!(
            arc.radius < ARC_WIDTHS * 0.2 && arc.radius >= 0.2,
            "{}",
            arc.radius
        );
        assert!(board.tracks[0].to.x >= tail.x, "{:?}", board.tracks[0].to);
    }

    #[test]
    fn a_bend_that_another_track_of_its_net_joins_keeps_its_corner() {
        let mut board = open_board();
        let points = bend_at(&board);
        bent(&mut board, "GND", points, 0.2);
        // A branch of the net that ends on the first track near the corner.
        let corner = points[1];
        board.tracks.push(Track::straight(
            "GND",
            CopperLayer::Front,
            Pt::new(corner.x - 0.1, corner.y),
            Pt::new(corner.x - 0.1, corner.y - 2.0),
            0.2,
        ));
        assert_eq!(round_bends(&mut board, ARC_WIDTHS), 0);
    }

    #[test]
    fn an_arc_is_tangent_to_both_tracks_of_its_bend() {
        // A bend of 45 degrees at the origin: in from the left, out down and to the right.
        let d = std::f64::consts::FRAC_1_SQRT_2;
        let (u, v) = (Pt::new(-1.0, 0.0), Pt::new(d, d));
        let arc = Bend::new(Pt::new(0.0, 0.0), u, v, std::f64::consts::FRAC_PI_4, 1.0);
        for p in [arc.from, arc.mid, arc.to] {
            assert!((p.distance(arc.centre) - 1.0).abs() < 1e-9);
        }
        // Tangent: the radius to each end is square to its track.
        let r = arc.from - arc.centre;
        assert!((r.x * u.x + r.y * u.y).abs() < 1e-9);
        let r = arc.to - arc.centre;
        assert!((r.x * v.x + r.y * v.y).abs() < 1e-9);
        let chords = arc.chords();
        assert!(chords.len() > 2);
        assert!(
            chords
                .iter()
                .all(|p| (p.distance(arc.centre) - 1.0).abs() < 1e-9)
        );
    }
}
