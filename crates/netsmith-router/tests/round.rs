//! The router's crossed nets routed again round a track being drawn
//! (`netsmith_router::round`, issue 1009): only the crossed nets get copper, none of it is in the
//! new track's way, and the work is one try. Counted, never timed.

use std::collections::BTreeSet;

use netsmith_pcb::{Board, Pt, Track};
use netsmith_router::round::reroute_round;
use netsmith_router::walk::Ground;
use netsmith_router::{RouteOptions, route_with};

use crate::common;

const WIDTH: f64 = 0.2;

#[test]
fn the_crossed_nets_alone_are_routed_again_round_the_new_track_in_one_try() {
    let placed: Board = common::example_board("led", &common::fixtures());
    let mut routed = placed.clone();
    route_with(&mut routed, RouteOptions::default());
    let router = routed.tracks[routed.fixed.tracks..].to_vec();
    let vias = routed.vias[routed.fixed.vias..].to_vec();
    assert!(!router.is_empty());
    let ground = Ground::of(&placed).unwrap();
    let walker = ground.walker();
    // A short track of another net across the middle of a router track, where it is free of the
    // board's own copper (the router's is not on the walk's board).
    let (drawn, net, crossed) = router
        .iter()
        .filter(|t| t.mid.is_none() && t.from.distance(t.to) > 1.5)
        .find_map(|t| {
            let other = placed.nets.iter().find(|n| n.name != t.net)?.name.clone();
            let m = Pt::new((t.from.x + t.to.x) / 2.0, (t.from.y + t.to.y) / 2.0);
            let len = t.from.distance(t.to);
            let (ux, uy) = ((t.to.x - t.from.x) / len, (t.to.y - t.from.y) / len);
            let half = t.width / 2.0 + 0.6;
            let a = Pt::new(m.x - uy * half, m.y + ux * half);
            let b = Pt::new(m.x + uy * half, m.y - ux * half);
            if walker.free(&other, t.layer, WIDTH, a, b) != Some(true) {
                return None;
            }
            let got = ground.crossed(&other, t.layer, WIDTH, &[a, b], &router, &vias);
            let nets: BTreeSet<String> = got
                .tracks
                .iter()
                .map(|&i| router[i].net.clone())
                .chain(got.vias.iter().map(|&i| vias[i].net.clone()))
                .collect();
            (!nets.is_empty())
                .then(|| (Track::straight(&other, t.layer, a, b, WIDTH), other, nets))
        })
        .expect("a router track a short track can cross");
    let round = reroute_round(
        &placed,
        &router,
        &vias,
        std::slice::from_ref(&drawn),
        &[],
        &crossed,
        None,
    );
    assert!(round.tries <= 1, "{round:?}");
    assert!(!round.tracks.is_empty(), "{round:?}");
    assert!(
        round.tracks.iter().all(|t| crossed.contains(&t.net))
            && round.vias.iter().all(|v| crossed.contains(&v.net)),
        "only the crossed nets {crossed:?} get copper: {round:?}"
    );
    // None of the new copper is in the drawn track's way.
    let again = ground.crossed(
        &net,
        drawn.layer,
        WIDTH,
        &[drawn.from, drawn.to],
        &round.tracks,
        &round.vias,
    );
    assert!(again.tracks.is_empty() && again.vias.is_empty(), "{again:?}");
    // No net crossed, nothing to route.
    let none = reroute_round(
        &placed,
        &router,
        &vias,
        &[drawn],
        &[],
        &BTreeSet::new(),
        None,
    );
    assert_eq!((none.tracks.len(), none.tries), (0, 0));
}
