//! The placer and the router can be stopped by the token of the job that runs them (ADR 0062),
//! which is how a `fab` is cancelled. (The router's own flag, which the PCB view's "stop" sets,
//! is another thing: it stops the router and keeps what it routed. See `routing.rs`.)
//!
//! How soon a cancel is heard is counted in checkpoints (`Cancel::checkpoints`), never in time,
//! so that a busy runner cannot fail these.

use crate::common;

use netsmith_core::job::{self, Cancel, Cancelled};
use netsmith_router::{RouteOptions, route_with};

use common::{hard_boards, synthetic_board};

#[test]
fn the_placer_stops_at_once_when_the_token_of_its_job_is_cancelled() {
    let token = Cancel::new();
    token.cancel();
    let result = job::catch(|| job::scope(&token, || synthetic_board(7, 52, 17, 20, 72, 36)));
    assert!(matches!(result, Err(Cancelled)));
    assert_eq!(
        token.checkpoints(),
        1,
        "the placer went past a checkpoint of a token that was cancelled"
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
    let whole = token.checkpoints();
    assert!(whole > 1, "a whole routing reaches {whole} checkpoints");

    let token = Cancel::new();
    token.cancel();
    let mut cancelled_board = board.clone();
    let result = job::catch(|| {
        job::scope(&token, || {
            route_with(&mut cancelled_board, RouteOptions::default())
        })
    });
    assert!(matches!(result, Err(Cancelled)));
    assert_eq!(
        token.checkpoints(),
        1,
        "the router went past a checkpoint of a token that was cancelled (a whole routing reaches {whole})"
    );
    // Nothing routed was kept: the job is dropped, not stopped early.
    assert!(cancelled_board.tracks.is_empty());
}
