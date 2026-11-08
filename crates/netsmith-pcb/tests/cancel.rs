//! The placer and the router can be stopped by the token of the job that runs them (ADR 0062),
//! which is how a `fab` is cancelled. (The router's own flag, which the PCB view's "stop" sets,
//! is another thing: it stops the router and keeps what it routed. See `routing.rs`.)

use crate::common;

use std::time::{Duration, Instant};

use netsmith_core::job::{self, Cancel, Cancelled};
use netsmith_pcb::{RouteOptions, route_with};

use common::{hard_boards, synthetic_board};

#[test]
fn the_placer_stops_at_once_when_the_token_of_its_job_is_cancelled() {
    let token = Cancel::new();
    token.cancel();
    let started = Instant::now();
    let result = job::catch(|| job::scope(&token, || synthetic_board(7, 52, 17, 20, 72, 36)));
    let stopped_after = started.elapsed();
    assert!(matches!(result, Err(Cancelled)));
    assert!(
        stopped_after < Duration::from_millis(500),
        "stopped after {stopped_after:?}"
    );
}

#[test]
fn the_router_stops_and_leaves_the_board_as_it_was_when_the_token_of_its_job_is_cancelled() {
    let board = hard_boards(1).remove(0);
    let token = Cancel::new();
    // A token that is not cancelled changes nothing: the router does its work.
    let mut free_board = board.clone();
    let free = job::catch(|| {
        job::scope(&token, || {
            route_with(&mut free_board, RouteOptions::default())
        })
    })
    .expect("not cancelled");
    assert!(!free.stopped);

    token.cancel();
    let mut cancelled_board = board.clone();
    let started = Instant::now();
    let result = job::catch(|| {
        job::scope(&token, || {
            route_with(&mut cancelled_board, RouteOptions::default())
        })
    });
    assert!(matches!(result, Err(Cancelled)));
    assert!(started.elapsed() < Duration::from_millis(500));
    // Nothing routed was kept: the job is dropped, not stopped early.
    assert!(cancelled_board.tracks.is_empty());
}
