//! The router's crossed nets routed again round a track being drawn (issue 1009, the slice "2b"
//! of push and shove): a preview, for the editor, of what routing again would lay round the new
//! copper.
//!
//! An edit of the routes file throws the router's copper away ([`crate::walk`]'s header), so the
//! track tool does not go round it: [`crate::walk::Ground::crossed`] names the router's tracks and
//! vias the track would cross. [`reroute_round`] shows where those nets would go instead: it
//! routes them again on the board as the routes file has it, with the new copper, and with the
//! router's copper of every other net, all of it fixed. Every net but the crossed ones is locked
//! ([`netsmith_pcb::FixedCopper::locked`]), so the router adds copper to the crossed nets alone.
//!
//! **Bounded work.** One order of the lines (`most_tries: Some(1)`), rip-up of the crossed nets
//! among themselves but no negotiation and no rounds of rip-up of whole nets, no teardrops, arcs, meanders, shared vias or pour pads:
//! the preview is the shape of the way, not the finished copper. The work is counted
//! ([`Round::tries`]); a cancel (`cancel`) stops it as it stops a route, keeping what it has.
//!
//! **A preview only.** Nothing is written: the commit of the track still discards the router's
//! copper, and Place and route lays it again.

use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;

use netsmith_pcb::{Board, Track, Via};

use crate::route::{CoarsePlan, LineClasses, RouteOptions, route_with};

/// The crossed nets routed round the new copper ([`reroute_round`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Round {
    /// The copper the router laid for the crossed nets.
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
    /// The connections of the crossed nets it could not make.
    pub left: usize,
    /// How many times the router routed the board (at most one).
    pub tries: usize,
    /// Stopped by the cancel before it was done.
    pub stopped: bool,
}

/// Route the nets `nets` again on `board` (as placed, with the routes file's copper as its fixed
/// copper, none of the router's) with `new_tracks` and `new_vias` (the track being drawn) and the
/// router's copper (`router_tracks`, `router_vias`) of every other net fixed: the module's header.
pub fn reroute_round(
    board: &Board,
    router_tracks: &[Track],
    router_vias: &[Via],
    new_tracks: &[Track],
    new_vias: &[Via],
    nets: &BTreeSet<String>,
    cancel: Option<&'static AtomicBool>,
) -> Round {
    if nets.is_empty() {
        return Round::default();
    }
    let mut b = board.clone();
    let kept_tracks: Vec<Track> = router_tracks
        .iter()
        .filter(|t| !nets.contains(&t.net))
        .chain(new_tracks)
        .cloned()
        .collect();
    let kept_vias: Vec<Via> = router_vias
        .iter()
        .filter(|v| !nets.contains(&v.net))
        .chain(new_vias)
        .cloned()
        .collect();
    let at_tracks = b.fixed.tracks.min(b.tracks.len());
    let at_vias = b.fixed.vias.min(b.vias.len());
    b.tracks.truncate(at_tracks);
    b.vias.truncate(at_vias);
    b.fixed.tracks += kept_tracks.len();
    b.fixed.vias += kept_vias.len();
    b.tracks.extend(kept_tracks);
    b.vias.extend(kept_vias);
    for net in &board.nets {
        if !nets.contains(&net.name) {
            b.fixed.locked.insert(net.name.clone());
        }
    }
    let (fixed_tracks, fixed_vias) = (b.fixed.tracks, b.fixed.vias);
    let routing = route_with(
        &mut b,
        RouteOptions {
            retry_orders: false,
            rip_up: true,
            negotiate: false,
            cancel,
            most_tries: Some(1),
            plan: CoarsePlan::Off,
            classes: LineClasses::Off,
            teardrops: false,
            arcs: 0.0,
            meanders: false,
            share_vias: false,
            pour_pads: false,
            rip_rounds: 0,
            ..RouteOptions::default()
        },
    );
    Round {
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
            .iter()
            .filter(|l| nets.contains(&l.net))
            .count(),
        tries: routing.tries,
        stopped: routing.stopped,
    }
}
