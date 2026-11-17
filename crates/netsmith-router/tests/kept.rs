//! The router's copper kept on disk (issue #1019): a kept board gives back the board it was
//! routed on, from which the copper of every net still where it was is carried; a keep of other
//! rules or other routes-file copper is refused whole, and a net whose pads moved is routed
//! again. Work is counted in nets carried and ripped up, never in time.

use std::collections::BTreeSet;

use netsmith_pcb::{Board, Ratsnest, Track};
use netsmith_router::carry::carry_copper;
use netsmith_router::kept::{Kept, Stale};
use netsmith_router::route;

use crate::common;

const SOURCE: &str = "src/main.nsm";

fn routed() -> (Board, Board) {
    let placed = common::example_board("linear_regulator", &common::fixtures());
    let mut routed = placed.clone();
    assert_eq!(route(&mut routed).len(), 0);
    (placed, routed)
}

/// The kept board as a file would have it: written and read again.
fn through_a_file(kept: &Kept) -> Kept {
    serde_json::from_str(&serde_json::to_string(kept).unwrap()).unwrap()
}

#[test]
fn a_kept_board_carries_every_net_of_the_same_placement() {
    let (placed, routed) = routed();
    let kept = through_a_file(&Kept::of(&placed, &routed, SOURCE));
    assert!(!kept.is_empty());
    let (before, rip) = kept.before(&placed, SOURCE).unwrap();
    assert!(rip.is_empty(), "{rip:?}");
    let mut after = placed.clone();
    let carried = carry_copper(&before, &mut after, &rip);
    println!("carried {} nets", carried.kept.len());
    assert_eq!(carried.kept.len(), kept.len());
    assert!(carried.ripped.is_empty(), "{:?}", carried.ripped);
    assert_eq!(Ratsnest::of(&after).len(), 0);
    // The same copper, net by net (the kept file holds it by net).
    let sorted = |b: &Board| {
        let mut t: Vec<String> = b.tracks.iter().map(|t| format!("{t:?}")).collect();
        t.sort();
        t
    };
    assert_eq!(sorted(&after), sorted(&routed));
}

#[test]
fn a_moved_part_rips_up_its_nets_and_keeps_the_rest() {
    let (placed, routed) = routed();
    let kept = Kept::of(&placed, &routed, SOURCE);
    let mut moved = placed.clone();
    let part = &mut moved.parts[0];
    part.at.x += 0.5;
    let refdes = part.refdes.clone();
    let (before, rip) = kept.before(&moved, SOURCE).unwrap();
    // The placement it was routed on, not the one now.
    assert_eq!(before.part(&refdes).unwrap().at, placed.part(&refdes).unwrap().at);
    let mut after = moved.clone();
    let carried = carry_copper(&before, &mut after, &rip);
    let on: BTreeSet<&str> = moved
        .nets
        .iter()
        .filter(|n| n.pads.iter().any(|p| p.refdes == refdes))
        .map(|n| n.name.as_str())
        .collect();
    println!("carried {:?}, ripped {:?}", carried.kept, carried.ripped);
    for net in &carried.kept {
        assert!(!on.contains(net.as_str()), "{net} of {refdes} was carried");
    }
    assert!(carried.ripped.iter().any(|n| on.contains(n.as_str())));
    assert_eq!(carried.kept.len() + carried.ripped.len(), kept.len());
}

#[test]
fn a_keep_of_other_rules_other_routes_copper_or_another_source_is_not_used() {
    let (placed, routed) = routed();
    let kept = Kept::of(&placed, &routed, SOURCE);
    let mut rules = placed.clone();
    rules.rules.clearance += 0.05;
    assert_eq!(kept.before(&rules, SOURCE).unwrap_err(), Stale::Rules);
    let mut drawn = placed.clone();
    let net = drawn.nets[0].name.clone();
    let at = drawn.outline.min;
    drawn.tracks.insert(
        0,
        Track::straight(&net, netsmith_pcb::CopperLayer::Front, at, at, 0.2),
    );
    drawn.fixed.tracks += 1;
    assert_eq!(kept.before(&drawn, SOURCE).unwrap_err(), Stale::Routes);
    assert!(matches!(
        kept.before(&placed, "src/other.nsm"),
        Err(Stale::Source { .. })
    ));
}
