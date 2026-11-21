//! A few nets routed round the copper a board has (issue #1195): what `netsmith routes route`
//! asks of the router for one net, or one connection of it, and for each net it ripped up.
//!
//! [`route_nets`] routes the nets `nets` on a board as placed with the routes file's copper on
//! it (its fixed copper, none of the router's) and every other net locked
//! ([`netsmith_pcb::FixedCopper::locked`]), so the router adds copper to those nets alone and
//! goes round everything else: pads, holes, keepouts, the edge, and the copper of every other
//! net. It is the whole-board router ([`route_with`]) with its orders, rip-up, negotiation and
//! rounds of rip-up among the lines of `nets` (the other nets have none of the router's copper to
//! rip), and none of what is drawn on a finished board: no teardrops, meanders, shared vias,
//! pour pads or stitching, which the routes file does not keep and `fab` draws again. The vias a
//! net's current needs side by side and the vias a datasheet asks for in a pad are kept: they are
//! the net's copper.
//!
//! **Corners** (issue #1508). The bends of the copper laid are rounded with arcs as `fab` rounds
//! the router's ([`crate::arcs::round_bends`], each where it keeps the rules' space), because
//! `fab` never rounds the copper of a routes file: a net that is mitered ([`Board::mitered`]: the
//! board's `.corner`, which `routes route --corner` sets for one run, or a `TrackPath`'s) keeps
//! its 45 degree corners.
//!
//! **Bounded work.** The router's own bounds, all counts ([`RouteOptions::rip_rounds`] and the
//! orders it tries, [`One::tries`]); there is no time limit.
//!
//! [`narrowest`] measures the room of a way found: the widest track of its net that each piece
//! of its tracks has room for, with every space of the design rules check ([`crate::walk`]'s
//! room), and the narrowest of them.

use std::collections::BTreeSet;

use netsmith_pcb::{Board, CopperLayer, Pt, Ratline, Track, Via};

use crate::route::{CoarsePlan, LineClasses, LineReport, RouteOptions, route_with};
use crate::walk::Ground;

/// The nets routed round the copper there ([`route_nets`]).
#[derive(Debug, Clone, Default)]
pub struct One {
    /// The copper the router laid for the nets, in the board's places.
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
    /// The connections of the nets still open on the board with that copper.
    pub left: Vec<Ratline>,
    /// Why each of the router's lines of the nets it left is left: what walls it in.
    pub why: Vec<LineReport>,
    /// How many orders of the lines the router tried.
    pub tries: usize,
}

impl One {
    /// Whether every connection of the nets is made.
    pub fn complete(&self) -> bool {
        self.left.is_empty()
    }
}

/// Route `nets` on `board` (placed, with the routes file's copper as its fixed copper and none of
/// the router's) with every other net locked: the module's header.
pub fn route_nets(board: &Board, nets: &BTreeSet<String>) -> One {
    route_nets_weighed(board, nets, None)
}

/// [`route_nets`], with the weights of the quality objective ([`RouteOptions::weights`],
/// issue #1517); `None` routes at the router's own costs.
pub fn route_nets_weighed(
    board: &Board,
    nets: &BTreeSet<String>,
    weights: Option<crate::route::Weights>,
) -> One {
    if nets.is_empty() {
        return One::default();
    }
    let mut b = board.clone();
    b.tracks.truncate(b.fixed.tracks.min(b.tracks.len()));
    b.vias.truncate(b.fixed.vias.min(b.vias.len()));
    for net in &board.nets {
        if !nets.contains(&net.name) {
            b.fixed.locked.insert(net.name.clone());
        }
    }
    let (fixed_tracks, fixed_vias) = (b.fixed.tracks, b.fixed.vias);
    let routing = route_with(
        &mut b,
        RouteOptions {
            plan: CoarsePlan::Off,
            classes: LineClasses::Off,
            teardrops: false,
            arcs: crate::arcs::ARC_WIDTHS,
            meanders: false,
            share_vias: false,
            pour_pads: false,
            stitch: false,
            weights,
            ..RouteOptions::default()
        },
    );
    One {
        tracks: b.tracks[fixed_tracks.min(b.tracks.len())..]
            .iter()
            .filter(|t| nets.contains(&t.net))
            .cloned()
            .collect(),
        vias: b.vias[fixed_vias.min(b.vias.len())..]
            .iter()
            .filter(|v| nets.contains(&v.net))
            .cloned()
            .collect(),
        left: routing
            .left
            .lines
            .into_iter()
            .filter(|l| nets.contains(&l.net))
            .collect(),
        why: routing
            .why
            .into_iter()
            .filter(|r| nets.contains(&r.line.net))
            .collect(),
        tries: routing.tries,
    }
}

/// The longest piece of a track that [`narrowest`] measures as one, in mm.
pub const PIECE: f64 = 1.0;
/// How much wider than its own width [`narrowest`] looks for room for a track, in mm: a way
/// with this much room or more is said to have this much.
pub const MOST_ROOM: f64 = 2.0;
/// How many times [`narrowest`] halves the width it looks for on each piece: to 2 mm / 2^8, under
/// 0.01 mm.
pub const HALVINGS: usize = 8;

/// The narrowest place of a way ([`narrowest`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Narrowest {
    pub layer: CopperLayer,
    /// The middle of the piece of track where the way is narrowest, in the board's places.
    pub at: Pt,
    /// The width of that track.
    pub width: f64,
    /// The widest track of its net the piece has room for there, keeping every space of the
    /// design rules check: the room the way has there (at most `width` +
    /// [`MOST_ROOM`]).
    pub fits: f64,
}

/// The narrowest place of the tracks `tracks` of `net` on `board` (the module's header): each
/// track is cut into pieces of at most [`PIECE`] mm, and the piece that has room for the
/// narrowest track is named. The work is bounded by the pieces and [`HALVINGS`], never timed.
/// `None` for no track, or a board whose copper cannot be read.
pub fn narrowest(board: &Board, net: &str, tracks: &[Track]) -> Option<Narrowest> {
    let ground = Ground::of(board).ok()?;
    let walker = ground.walker();
    let mut best: Option<Narrowest> = None;
    for t in tracks.iter().filter(|t| t.net == net) {
        let line = t.centre_line();
        for w in line.windows(2) {
            let (a, b) = (w[0], w[1]);
            let pieces = (a.distance(b) / PIECE).ceil().max(1.0) as usize;
            for k in 0..pieces {
                let along = |f: f64| Pt::new(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f);
                let (p, q) = (
                    along(k as f64 / pieces as f64),
                    along((k + 1) as f64 / pieces as f64),
                );
                let fits = |width: f64| walker.free(net, t.layer, width, p, q) == Some(true);
                let room = if !fits(t.width) {
                    t.width
                } else if fits(t.width + MOST_ROOM) {
                    t.width + MOST_ROOM
                } else {
                    let (mut lo, mut hi) = (t.width, t.width + MOST_ROOM);
                    for _ in 0..HALVINGS {
                        let mid = (lo + hi) / 2.0;
                        if fits(mid) {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    lo
                };
                if best.as_ref().is_none_or(|c| room < c.fits) {
                    best = Some(Narrowest {
                        layer: t.layer,
                        at: along((k as f64 + 0.5) / pieces as f64),
                        width: t.width,
                        fits: room,
                    });
                }
            }
        }
    }
    best
}
