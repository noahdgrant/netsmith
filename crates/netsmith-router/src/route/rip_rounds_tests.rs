//! Tests of the rounds of rip-up and re-route of whole nets (`rip_rounds.rs`, issue #1088) and of
//! the walls the report names (`walls.rs`). They count work (rounds, searches, rips), never time.

use super::stage_tests::{across, led_board};
use super::*;

/// Two lines of different nets of the LED board, neither to a plane.
fn two_lines(router: &Router) -> (usize, usize) {
    (0..router.lines.len())
        .flat_map(|a| (0..router.lines.len()).map(move |b| (a, b)))
        .find(|&(a, b)| {
            router.nets[a] != router.nets[b]
                && !router.lines[a].to_plane
                && !router.lines[b].to_plane
        })
        .unwrap()
}

#[test]
fn a_line_whose_only_way_crosses_another_nets_track_is_routed_and_that_net_is_routed_again() {
    let board = led_board();
    let router = Router::new(&board, &RouteOptions::default());
    let (a, b) = two_lines(&router);
    // Line `a`'s copper runs right across the board on both layers, through line `b`'s way.
    let mut state = across(&router, a, b);
    state.failed = vec![b];
    assert!(router.try_line(b, &state, &[], ROOM_TIGHT, false).is_none());
    let before = state.routed[0].tracks.clone();
    router.stage.set(Stage::RipRounds);
    router.rip_rounds(&mut state, 1, 1);
    assert!(state.failed.is_empty(), "{:?}", state.failed);
    let mut lines: Vec<usize> = state.routed.iter().map(|r| r.line).collect();
    lines.sort_unstable();
    assert_eq!(lines, {
        let mut both = vec![a, b];
        both.sort_unstable();
        both
    });
    // Net `a` was ripped up whole and routed again on a way of its own.
    let again = state.routed.iter().find(|r| r.line == a).unwrap();
    assert_ne!(again.tracks, before);
    let ripped = router.ripped.take();
    assert_eq!(ripped.rounds, 1);
    assert_eq!(
        ripped.ripped.into_iter().collect::<Vec<_>>(),
        [(board.nets[router.nets[a]].name.clone(), 1)]
    );
    assert_eq!((ripped.left_before, ripped.left_after), (1, 0));
    // One soft search, then the line and the ripped line at the usual room.
    assert!(ripped.searches >= 3, "{}", ripped.searches);
    assert!(ripped.searches <= RIP_SEARCHES);
}

#[test]
fn a_net_ripped_up_as_often_as_it_may_be_is_hard_to_the_rounds() {
    let board = led_board();
    let router = Router::new(&board, &RouteOptions::default());
    let (a, b) = two_lines(&router);
    let state = across(&router, a, b);
    // Free, the soft search crosses net `a`; with `a`'s line kept hard (as the rounds keep a net
    // ripped up `RIP_MOST` times), no path crosses only other copper, so nothing is ripped.
    let free = router.contested_locked(b, &state, ROOM_TIGHT, &[]);
    assert!(free.unwrap().1.contains(&router.nets[a]));
    assert!(
        router
            .contested_locked(b, &state, ROOM_TIGHT, &[a])
            .is_none()
    );
}

#[test]
fn copper_of_the_routes_file_is_never_ripped_and_a_locked_net_gets_none() {
    let mut board = led_board();
    let (fixed, net_a, net_b) = {
        let router = Router::new(&board, &RouteOptions::default());
        let (a, b) = two_lines(&router);
        // The same track across the board, but the routes file's: fixed copper.
        let state = across(&router, a, b);
        (
            state.routed[0].tracks.clone(),
            board.nets[router.nets[a]].name.clone(),
            board.nets[router.nets[b]].name.clone(),
        )
    };
    board.tracks.splice(0..0, fixed.clone());
    board.fixed.tracks += fixed.len();
    for locked in [false, true] {
        let mut board = board.clone();
        if locked {
            board.fixed.locked.insert(net_a.clone());
        }
        let routing = route_with(&mut board, RouteOptions::default());
        // The fixed copper is where it was, and the rounds ripped nothing of net `a`.
        assert_eq!(board.tracks[..fixed.len()], fixed[..]);
        assert!(
            !routing.rip_rounds.ripped.contains_key(&net_a),
            "{:?}",
            routing.rip_rounds
        );
        // Line `b` stays open, and the report names net `a`, whose fixed copper walls it in.
        let why = routing
            .why
            .iter()
            .find(|w| w.line.net == net_b)
            .expect("line b is left");
        assert!(why.blockers.contains(&net_a), "{why:?}");
        if locked {
            // The router laid no copper of the locked net.
            assert!(board.tracks[fixed.len()..].iter().all(|t| t.net != net_a));
            assert!(board.vias.iter().all(|v| v.net != net_a));
        }
    }
}
