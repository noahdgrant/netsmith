//! Partial routing (issue #825): after one module is placed again, the copper of the nets it
//! left alone is carried as it was and only the rest is routed. Work is counted in connections
//! left to route, never in time.

use std::collections::BTreeSet;

use netsmith_pcb::{Board, Pt, Ratsnest};
use netsmith_router::carry::{carry_copper, moved_parts, reroute};
use netsmith_router::{RouteOptions, route};

use crate::common;

/// A board written in modules (eight parts to a module) placed and routed, and the same board
/// with one module placed again by a seed that moves a part, and the nets of its parts.
fn routed_and_regenerated() -> (Board, Board, Vec<String>) {
    let placed = common::synthetic_board_structured(
        7,
        24,
        0,
        4,
        6,
        10,
        &netsmith_pcb::PlaceOptions::default(),
    );
    let mut routed = placed.clone();
    let left = route(&mut routed).len();
    println!("the board routed with {left} connections left");
    let module = netsmith_placer::module_paths(&placed)
        .into_iter()
        .find(|m| m.starts_with('m'))
        .expect("a module");
    let (again, nets) = (1..20)
        .find_map(|seed| {
            let mut again = placed.clone();
            let done = netsmith_placer::regenerate(&mut again, &module, &BTreeSet::new(), seed, 3.0)
                .unwrap();
            (again.parts != placed.parts).then_some((again, done.nets))
        })
        .expect("a seed moves a part");
    (routed, again, nets)
}

fn copper_of(board: &Board, net: &str) -> (Vec<String>, Vec<String>) {
    let mut tracks: Vec<String> = board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .map(|t| format!("{t:?}"))
        .collect();
    let mut vias: Vec<String> = board
        .vias
        .iter()
        .filter(|v| v.net == net)
        .map(|v| format!("{v:?}"))
        .collect();
    tracks.sort();
    vias.sort();
    (tracks, vias)
}

#[test]
fn the_copper_of_untouched_nets_is_carried_and_only_the_rest_is_left_to_route() {
    let (routed, again, nets) = routed_and_regenerated();
    let rip: BTreeSet<String> = nets.iter().cloned().collect();
    let mut after = again.clone();
    let carried = carry_copper(&routed, &mut after, &rip);
    // Every net of a part that moved lost its copper; the others kept theirs exactly.
    for net in &nets {
        assert!(!carried.kept.contains(net), "{net} was carried");
    }
    for net in &carried.kept {
        assert_eq!(copper_of(&routed, net), copper_of(&after, net), "{net}");
    }
    assert_eq!(after.fixed.tracks, carried.tracks);
    assert_eq!(after.fixed.vias, carried.vias);
    // What is left to route is the connections of the ripped nets alone.
    let left = Ratsnest::of(&after);
    assert!(
        left.lines.iter().all(|l| carried.ripped.contains(&l.net)),
        "a carried net has a connection left: {:?}",
        left.lines.iter().map(|l| &l.net).collect::<Vec<_>>()
    );
    let whole = Ratsnest::of(&again).lines.len();
    println!(
        "carried {} nets ({} tracks, {} vias); {} of {} connections left to route",
        carried.kept.len(),
        carried.tracks,
        carried.vias,
        left.lines.len(),
        whole
    );
    // The router routes the rest, and the carried copper is still there as it was.
    assert!(!carried.kept.is_empty(), "nothing was carried");
    assert!(left.lines.len() < whole);
    let mut routed_again = again.clone();
    let done = reroute(&routed, &mut routed_again, &rip, RouteOptions::default());
    // The carried copper is the router's again: the board is one to carry from in turn.
    assert_eq!(routed_again.fixed, again.fixed);
    for net in &done.carried.kept {
        assert_eq!(copper_of(&routed, net), copper_of(&routed_again, net), "{net}");
    }
    println!(
        "routed again in {} run(s), {} connections left",
        done.runs,
        done.routing.left.lines.len()
    );
}

#[test]
fn copper_that_a_moved_part_now_covers_is_ripped_up() {
    let placed = common::example_board("linear_regulator", &common::fixtures());
    let mut routed = placed.clone();
    assert_eq!(route(&mut routed).len(), 0);
    // Move one part so that the middle of a track of a net it is not on is under its first pad.
    let (part, track) = routed
        .parts
        .iter()
        .find_map(|p| {
            let on: Vec<&str> = routed
                .nets
                .iter()
                .filter(|n| n.pads.iter().any(|pad| pad.refdes == p.refdes))
                .map(|n| n.name.as_str())
                .collect();
            routed
                .tracks
                .iter()
                .find(|t| !on.contains(&t.net.as_str()) && t.mid.is_none())
                .map(|t| (p.refdes.clone(), t.clone()))
        })
        .expect("a track of another net");
    let mut after = placed.clone();
    let p = after.parts.iter_mut().find(|p| p.refdes == part).unwrap();
    let pad = p.footprint.pads[0].clone();
    let centre = p.pad_centre(&pad);
    let middle = Pt::new(
        (track.from.x + track.to.x) / 2.0,
        (track.from.y + track.to.y) / 2.0,
    );
    p.at = Pt::new(p.at.x + middle.x - centre.x, p.at.y + middle.y - centre.y);
    assert_eq!(moved_parts(&routed, &after).len(), 1);
    let carried = carry_copper(&routed, &mut after, &BTreeSet::new());
    assert!(
        carried.ripped.contains(&track.net),
        "{} was carried under {part}",
        track.net
    );
}

#[test]
fn nothing_moved_carries_everything_and_leaves_nothing_to_route() {
    let placed = common::example_board("linear_regulator", &common::fixtures());
    let mut routed = placed.clone();
    assert_eq!(route(&mut routed).len(), 0);
    let mut after = placed.clone();
    let carried = carry_copper(&routed, &mut after, &BTreeSet::new());
    assert!(carried.ripped.is_empty(), "{:?}", carried.ripped);
    assert_eq!(Ratsnest::of(&after).len(), 0);
}
