//! Walking a new track round the copper of a board (issue #726, the first slice of push and
//! shove): the query the editor's track tool asks while a track is drawn.
//!
//! [`route`](crate::route) routes a whole board. This answers two small questions about one
//! segment of a track of a net, on a layer, at a width, against the board as it is (its pads,
//! the copper of the other nets, holes, keepouts, the board edge):
//!
//! - [`Walker::free`]: whether a straight segment keeps every space the design rules check asks
//!   of a track.
//! - [`Walker::path`]: the way from one point to another that does, as a few corners of at most
//!   45 degrees: the straight segment where it is free, else a segment with one bend of 45
//!   degrees, else a search over a lattice of 45-degree moves through the obstacles in a bounded
//!   area, which finds the way round a pad or a track or says there is none.
//!
//! **One clearance model.** The spaces are those of [`netsmith_geom::Room`], the room the
//! meanders ([`crate::meander`]) and the teardrops ([`crate::teardrop`]) are drawn in: the
//! design rules check's own (the larger of the two nets' classes and a pad's own, the hole
//! spaces, the copper to edge space, the solder mask opening, keepouts), measured on exact
//! shapes with a micrometre to spare, so a segment this calls free is one the check does not
//! find. The router's grid ([`crate::route`]) keeps 0.05 mm and half a cell more than the rules
//! round every obstacle, because it routes between cells; a person drawing a track wants the
//! rules' own space, and the same model as the check means nothing is offered that the check
//! then refuses. There is no second clearance model here: the grid's obstacle list is not used.
//!
//! **What is an obstacle.** The board as given: the router's copper on it is whatever the
//! caller put there (the editor gives the board as placed with the copper of the routes file,
//! which is what the check reads when the track is ended, and not the router's own copper, which
//! an edit discards). Copper of the walked net is never an obstacle to it.
//!
//! **The router's copper** gives way ([`Ground::crossed`], the second slice of #726): it is
//! never an obstacle, since an edit of the routes file discards it; [`Ground::crossed`] says
//! which of it the track drawn would cross, so the editor can show it as copper that will be
//! routed again.
//!
//! **The search.** Corners are at 45 degrees: a lattice [`LATTICE`] (0.1 mm, the router's cell)
//! apart anchored at the start, moves in eight directions that turn by at most 45 degrees at
//! a bend (the check's `track_angle` refuses more), A* with the router's costs (a step 10, a
//! diagonal 14, a bend [`BEND`] 25). A lattice node outside the board's edge is closed, and so is
//! one outside the box of the two ends widened by [`REACH`]. The search ends at a node within
//! two cells of the end that a segment or one 45-degree bend joins to it. The work is counted,
//! not timed: [`Walked::tried`] is the number of states the search took off its queue, at most
//! [`MOST_TRIED`], so a board that has no way round says so after a bounded effort.
//!
//! **Shove** ([`crate::shove`], the third slice of #726) is the other half: it moves the unlocked
//! copper of the routes file out of the way of a track, and the walkaround goes round what stays.
//!
//! **Not done:** a track never changes layer and does not look for the shortest way over the whole
//! board: the area is the ends' box widened by [`REACH`].

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use netsmith_geom::{Artwork, DrcExtras, DrcRules, DrcShape, Pt as FPt, Room};
use netsmith_pcb::{Board, CopperLayer, Pt, Track, Via};

/// The distance between the lattice's nodes, in mm: the router's cell.
pub const LATTICE: f64 = 0.1;
/// How far round the box of the two ends the search may go, in mm: the router's first window.
pub const REACH: f64 = 4.0;
/// The most states a search takes off its queue before it gives up.
pub const MOST_TRIED: usize = 30_000;
/// What a bend of 45 degrees costs, in tenths of a lattice step: the router's turn cost.
const BEND: i64 = 25;
const STEP: i64 = 10;
const DIAGONAL: i64 = 14;
/// How near the end, in mm, a node must be for the search to try to join it to the end.
const NEAR_END: f64 = 2.0 * LATTICE;
/// The most a corner may turn, in degrees, with a little for rounding.
const MOST_TURN: f64 = 45.0 + 1e-3;

/// What a walk is checked against: the board's copper as the design rules check reads it. Built
/// once for a board ([`Ground::of`]) and asked as often as needed ([`Ground::walker`]).
pub struct Ground {
    pub(crate) art: Artwork,
    pub(crate) rules: DrcRules,
    pub(crate) extras: DrcExtras,
    /// The board's edge, for the nodes the lattice may not use.
    edge: netsmith_pcb::BoardEdge,
}

impl Ground {
    /// The ground of `board`, or why its copper cannot be read.
    pub fn of(board: &Board) -> Result<Ground, String> {
        let art = netsmith_pcb::copper_artwork(board).map_err(|e| e.to_string())?;
        let rules = DrcRules::from_project(&netsmith_pcb::kicad_pro(board))
            .map_err(|e| format!("the design rules: {e}"))?;
        Ok(Ground {
            art,
            rules,
            extras: crate::teardrop::extras(board),
            edge: board.edge(),
        })
    }

    /// The copper of the router that a track drawn through `corners` crosses (issue #726, the
    /// second slice): the tracks and vias of `tracks` and `vias`, copper of other nets than
    /// `net`, that keep every space on this ground as they are but not once the track is there.
    /// An edit of the routes file throws the router's copper away, so none of it is an obstacle
    /// to [`Walker::path`]; this says which of it the way found would have been in, for the
    /// editor to show it giving way. Indexes into the two slices, in their order. A track is
    /// asked on its own layer and a via (a through via) on the track's. The track is `width`
    /// wide on `layer`; a layer the board does not have, or fewer than one corner, crosses
    /// nothing.
    pub fn crossed(
        &self,
        net: &str,
        layer: CopperLayer,
        width: f64,
        corners: &[Pt],
        tracks: &[Track],
        vias: &[Via],
    ) -> Crossed {
        let room = Room::new(&self.art, &self.rules, &self.extras);
        let Some(layer_ix) = room.layer(layer.name()) else {
            return Crossed::default();
        };
        if corners.is_empty() {
            return Crossed::default();
        }
        let track_shape = |t: &Track| {
            DrcShape::stroke(
                t.centre_line().into_iter().map(fpt).collect(),
                t.width / 2.0,
            )
        };
        let via_shape = |v: &Via| DrcShape::disc(fpt(v.at), v.diameter / 2.0);
        // The copper that keeps its spaces as it is: what is judged once the track is kept.
        let track_ix: Vec<usize> = (0..tracks.len())
            .filter(|&i| {
                let t = &tracks[i];
                t.net != net
                    && t.layer == layer
                    && room.fits(&track_shape(t), &t.net, layer_ix, false)
            })
            .collect();
        let via_ix: Vec<usize> = (0..vias.len())
            .filter(|&i| {
                let v = &vias[i];
                v.net != net && room.fits(&via_shape(v), &v.net, layer_ix, false)
            })
            .collect();
        for w in corners.windows(2) {
            room.keep(net, layer_ix, segment(w[0], w[1], width));
        }
        if corners.len() == 1 {
            room.keep(net, layer_ix, segment(corners[0], corners[0], width));
        }
        Crossed {
            tracks: track_ix
                .into_iter()
                .filter(|&i| !room.fits(&track_shape(&tracks[i]), &tracks[i].net, layer_ix, false))
                .collect(),
            vias: via_ix
                .into_iter()
                .filter(|&i| !room.fits(&via_shape(&vias[i]), &vias[i].net, layer_ix, false))
                .collect(),
        }
    }

    /// The questions about this ground.
    pub fn walker(&self) -> Walker<'_> {
        Walker {
            room: Room::new(&self.art, &self.rules, &self.extras),
            edge: &self.edge,
        }
    }
}

/// The router's copper a walked track crosses ([`Ground::crossed`]): indexes of the tracks and
/// of the vias that were asked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Crossed {
    pub tracks: Vec<usize>,
    pub vias: Vec<usize>,
}

/// One track to walk: its net, layer and width, and where it goes. `from` is its last corner,
/// `to` where it is to end; `heading` is the direction of the segment that arrives at `from`, if
/// there is one, which the first segment may turn from by 45 degrees at most.
#[derive(Debug, Clone, Copy)]
pub struct Walk<'a> {
    pub net: &'a str,
    pub layer: CopperLayer,
    pub width: f64,
    pub from: Pt,
    pub to: Pt,
    pub heading: Option<Pt>,
}

/// A way found: the corners from `from` to `to`, both included, to the nanometre the routes file
/// keeps; and the work it took, in states taken off the search's queue (0 where the straight
/// segment, or one bend, was free).
#[derive(Debug, Clone, PartialEq)]
pub struct Walked {
    pub points: Vec<Pt>,
    pub tried: usize,
}

/// Why no way was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// A track of this width at the start keeps no space from the copper round it.
    Start,
    /// A track of this width at the end keeps no space from the copper round it: the end is on
    /// or too near a pad, another net's copper, a keepout or the board edge.
    End,
    /// The layer is not a copper layer of the board.
    Layer,
    /// Every place the search could reach in the area was tried and none joins the end.
    WalledIn { tried: usize },
    /// The search took [`MOST_TRIED`] states and found nothing: no way was found in the area.
    Gave { tried: usize },
}

impl Blocked {
    /// What was tried, in states taken off the queue.
    pub fn tried(&self) -> usize {
        match self {
            Blocked::WalledIn { tried } | Blocked::Gave { tried } => *tried,
            _ => 0,
        }
    }
}

impl std::fmt::Display for Blocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Blocked::Start => write!(
                f,
                "the track's last corner has no room: it is too near other copper or the board edge for a track of this width"
            ),
            Blocked::End => write!(
                f,
                "no room for a track there: that place is on or too near a pad, other copper, a keepout or the board edge"
            ),
            Blocked::Layer => write!(f, "the board has no such copper layer"),
            Blocked::WalledIn { .. } => write!(
                f,
                "no way round: the copper, keepouts and the board edge round the place close it in"
            ),
            Blocked::Gave { .. } => write!(
                f,
                "no way round was found within {REACH} mm of the track: it would have to go farther"
            ),
        }
    }
}

/// The questions about a [`Ground`].
pub struct Walker<'a> {
    room: Room<'a>,
    edge: &'a netsmith_pcb::BoardEdge,
}

pub(crate) fn fpt(p: Pt) -> FPt {
    FPt::new(p.x, p.y)
}

/// The copper of a straight track of `width` from `a` to `b` (a disc where they are the same).
pub(crate) fn segment(a: Pt, b: Pt, width: f64) -> DrcShape {
    if a.distance(b) < 1e-9 {
        DrcShape::disc(fpt(a), width / 2.0)
    } else {
        DrcShape::stroke(vec![fpt(a), fpt(b)], width / 2.0)
    }
}

/// The angle between two directions, in degrees.
pub(crate) fn angle_between(a: Pt, b: Pt) -> f64 {
    let dot = a.x * b.x + a.y * b.y;
    let n = (a.x.hypot(a.y) * b.x.hypot(b.y)).max(1e-18);
    (dot / n).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The eight directions of the lattice, each the one before turned by 45 degrees.
const DIRS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

fn dir_vector(d: u8) -> Pt {
    let (i, j) = DIRS[d as usize];
    Pt::new(i as f64, j as f64)
}

fn round(p: Pt) -> Pt {
    let r = |v: f64| (v * 1e6).round() / 1e6;
    Pt::new(r(p.x), r(p.y))
}

/// A state of the search: a node of the lattice and the direction it was reached in (8: none,
/// the start).
type State = (i32, i32, u8);

impl Walker<'_> {
    /// Whether a straight track of `net` on `layer` at `width` from `a` to `b` keeps every space
    /// the design rules check asks. `None` for a layer the board does not have.
    pub fn free(&self, net: &str, layer: CopperLayer, width: f64, a: Pt, b: Pt) -> Option<bool> {
        let layer = self.room.layer(layer.name())?;
        Some(self.fits(net, layer, width, a, b))
    }

    fn fits(&self, net: &str, layer: usize, width: f64, a: Pt, b: Pt) -> bool {
        self.room.fits(&segment(a, b, width), net, layer, false)
    }

    /// The way for a track from `walk.from` to `walk.to`, or why there is none. See the module's
    /// header for what it tries, in this order: the straight segment, one bend of 45 degrees,
    /// the search.
    pub fn path(&self, walk: &Walk<'_>) -> Result<Walked, Blocked> {
        let Some(layer) = self.room.layer(walk.layer.name()) else {
            return Err(Blocked::Layer);
        };
        let ask = Ask {
            net: walk.net,
            layer,
            width: walk.width,
        };
        if !self.fits(ask.net, layer, ask.width, walk.from, walk.from) {
            return Err(Blocked::Start);
        }
        if !self.fits(ask.net, layer, ask.width, walk.to, walk.to) {
            return Err(Blocked::End);
        }
        let done = |points: Vec<Pt>, tried: usize| Walked {
            points: points.into_iter().map(round).collect(),
            tried,
        };
        if let Some(points) = self.join(&ask, walk.from, walk.to, walk.heading) {
            return Ok(done(points, 0));
        }
        let (points, tried) = self.search(&ask, walk)?;
        Ok(done(self.tidy(&ask, points, walk.heading), tried))
    }

    /// The path without the corners it can do without: where the segment from the corner before
    /// to the corner after is free and turns by at most 45 degrees at both ends, the corner
    /// goes. (The lattice leaves short jogs, where the end is a few cells from a node.)
    fn tidy(&self, ask: &Ask<'_>, mut points: Vec<Pt>, heading: Option<Pt>) -> Vec<Pt> {
        let mut i = 1;
        while i + 1 < points.len() {
            let (a, c) = (points[i - 1], points[i + 1]);
            let before = if i >= 2 {
                Some(a - points[i - 2])
            } else {
                heading
            };
            let straight = a.distance(c) > 1e-9;
            let turns_in = before.is_none_or(|h| angle_between(h, c - a) <= MOST_TURN);
            let turns_out = points
                .get(i + 2)
                .is_none_or(|&d| angle_between(c - a, d - c) <= MOST_TURN);
            if straight && turns_in && turns_out && self.fits(ask.net, ask.layer, ask.width, a, c) {
                points.remove(i);
                i = i.saturating_sub(1).max(1);
            } else {
                i += 1;
            }
        }
        points
    }

    /// The straight segment, or the way of one bend of 45 degrees, from `a` to `b` when it is
    /// free and the first segment turns from `heading` by at most 45 degrees.
    fn join(&self, ask: &Ask<'_>, a: Pt, b: Pt, heading: Option<Pt>) -> Option<Vec<Pt>> {
        let d = b - a;
        if d.x.hypot(d.y) < 1e-9 {
            return Some(vec![a]);
        }
        let turns_ok = |to: Pt| heading.is_none_or(|h| angle_between(h, to - a) <= MOST_TURN);
        let free = |p: Pt, q: Pt| self.fits(ask.net, ask.layer, ask.width, p, q);
        if turns_ok(b) && free(a, b) {
            return Some(vec![a, b]);
        }
        // One bend: the diagonal run and the run along an axis, in either order. On an axis or
        // a diagonal already, there is no bend to make.
        let (ax, ay) = (d.x.abs(), d.y.abs());
        if ax < 1e-9 || ay < 1e-9 || (ax - ay).abs() < 1e-9 {
            return None;
        }
        let diagonal = Pt::new(d.x.signum() * ax.min(ay), d.y.signum() * ax.min(ay));
        let diagonal_first = a + diagonal;
        let axis_first = b - diagonal;
        for bend in [diagonal_first, axis_first] {
            if turns_ok(bend) && free(a, bend) && free(bend, b) {
                return Some(vec![a, bend, b]);
            }
        }
        None
    }

    /// The lattice search: the corners of a way and the states it took, or why none.
    fn search(&self, ask: &Ask<'_>, walk: &Walk<'_>) -> Result<(Vec<Pt>, usize), Blocked> {
        let (a, b) = (walk.from, walk.to);
        let at = |i: i32, j: i32| Pt::new(a.x + i as f64 * LATTICE, a.y + j as f64 * LATTICE);
        let (x0, x1) = (a.x.min(b.x) - REACH, a.x.max(b.x) + REACH);
        let (y0, y1) = (a.y.min(b.y) - REACH, a.y.max(b.y) + REACH);
        // The least the rest of the way costs: the octile distance, in tenths of a step.
        let rest = |p: Pt| {
            let (dx, dy) = (
                (b.x - p.x).abs() / LATTICE * STEP as f64,
                (b.y - p.y).abs() / LATTICE * STEP as f64,
            );
            (dx.max(dy) + (DIAGONAL - STEP) as f64 / STEP as f64 * dx.min(dy)) as i64
        };
        let start: State = (0, 0, 8);
        let mut best: HashMap<State, i64> = HashMap::from([(start, 0)]);
        let mut parent: HashMap<State, State> = HashMap::new();
        let mut queue: BinaryHeap<Reverse<(i64, i64, State)>> = BinaryHeap::new();
        queue.push(Reverse((rest(a), rest(a), start)));
        // Whether the segment between two nodes is free, by the node it starts at and the
        // direction (a segment is asked from both its ends).
        let mut edges: HashMap<(i32, i32, u8), bool> = HashMap::new();
        let mut tried = 0usize;
        while let Some(Reverse((f, h, state))) = queue.pop() {
            let g = f - h;
            let (i, j, came) = state;
            // A queue entry that a cheaper path to the same state has replaced.
            if best[&state] < g {
                continue;
            }
            tried += 1;
            if tried > MOST_TRIED {
                return Err(Blocked::Gave { tried: tried - 1 });
            }
            let here = at(i, j);
            if came != 8 && here.distance(b) <= NEAR_END {
                let heading = Some(dir_vector(came));
                if let Some(tail) = self.join(ask, here, b, heading) {
                    let mut points = lattice_corners(&parent, state, &at);
                    points.pop();
                    points.extend(tail);
                    return Ok((points, tried));
                }
            }
            for (e, (di, dj)) in DIRS.iter().enumerate() {
                let e = e as u8;
                let allowed = if came == 8 {
                    walk.heading
                        .is_none_or(|h| angle_between(h, dir_vector(e)) <= MOST_TURN)
                } else {
                    let turn = (e as i32 - came as i32).rem_euclid(8);
                    turn <= 1 || turn == 7
                };
                if !allowed {
                    continue;
                }
                let (ni, nj) = (i + di, j + dj);
                let there = at(ni, nj);
                if there.x < x0
                    || there.x > x1
                    || there.y < y0
                    || there.y > y1
                    || !self.edge.contains(there)
                {
                    continue;
                }
                // One entry for each segment, whichever end it is asked from.
                let key = if (ni, nj) > (i, j) {
                    (i, j, e)
                } else {
                    (ni, nj, (e + 4) % 8)
                };
                let free = *edges
                    .entry(key)
                    .or_insert_with(|| self.fits(ask.net, ask.layer, ask.width, here, there));
                if !free {
                    continue;
                }
                let step = if di * dj == 0 { STEP } else { DIAGONAL };
                let bend = if came != 8 && e != came { BEND } else { 0 };
                let cost = g + step + bend;
                let next: State = (ni, nj, e);
                if best.get(&next).is_none_or(|&old| cost < old) {
                    best.insert(next, cost);
                    parent.insert(next, state);
                    let h = rest(there);
                    queue.push(Reverse((cost + h, h, next)));
                }
            }
        }
        Err(Blocked::WalledIn { tried })
    }
}

/// What a walk asks: the net, the layer's index in the room, and the width.
struct Ask<'a> {
    net: &'a str,
    layer: usize,
    width: f64,
}

/// The corners of the lattice path that ends at `state`, from the start: the nodes where the
/// direction changes, and both ends.
fn lattice_corners(
    parent: &HashMap<State, State>,
    state: State,
    at: &impl Fn(i32, i32) -> Pt,
) -> Vec<Pt> {
    let mut chain = vec![state];
    let mut s = state;
    while let Some(&p) = parent.get(&s) {
        chain.push(p);
        s = p;
    }
    chain.reverse();
    let mut points = vec![at(chain[0].0, chain[0].1)];
    for k in 1..chain.len() {
        let last = k + 1 == chain.len();
        if last || chain[k + 1].2 != chain[k].2 {
            points.push(at(chain[k].0, chain[k].1));
        }
    }
    points
}
