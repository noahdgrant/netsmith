//! The placer and the router can be stopped by the token of the job that runs them (ADR 0062),
//! which is how a `fab` is cancelled. (The router's own flag, which the PCB view's "stop" sets,
//! is another thing: it stops the router and keeps what it routed. See `routing.rs`.)
//!
//! How soon a cancel is heard is counted in checkpoints (`Cancel::checkpoints`), never in time,
//! so that a busy runner cannot fail these. The router's progress, which a job's sink hears, is here too.

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

#[test]
fn a_cancel_in_the_middle_of_a_routing_stops_it_at_that_checkpoint_and_keeps_nothing() {
    let board = hard_boards(1).remove(0);
    let token = Cancel::new();
    let mut whole_board = board.clone();
    job::catch(|| job::scope(&token, || route_with(&mut whole_board, RouteOptions::default())))
        .expect("not cancelled");
    let whole = token.checkpoints();
    assert!(whole > 4, "a whole routing reaches {whole} checkpoints");

    // Cancelled half way through the router's work, at a known checkpoint whatever the machine.
    let half = whole / 2;
    let token = Cancel::at_checkpoint(half);
    let mut cancelled_board = board.clone();
    let result = job::catch(|| {
        job::scope(&token, || {
            route_with(&mut cancelled_board, RouteOptions::default())
        })
    });
    assert!(matches!(result, Err(Cancelled)));
    assert_eq!(
        token.checkpoints(),
        half,
        "the router went on past the checkpoint that cancelled it (a whole routing reaches {whole})"
    );
    assert!(cancelled_board.tracks.is_empty() && cancelled_board.vias.is_empty());
}

/// The router tells a job's sink how far it has got (issue #764): `connections routed` of the
/// lines it routes, each with a sentence, and what each try ended with. Listening changes
/// nothing it routes.
#[test]
fn the_router_reports_its_progress_to_the_job_and_routes_the_same_board_while_it_does() {
    use std::{cell::RefCell, rc::Rc};
    let board = hard_boards(1).remove(0);
    let mut quiet = board.clone();
    let alone = route_with(&mut quiet, RouteOptions::default());

    let heard = Rc::new(RefCell::new(Vec::<job::Work>::new()));
    let sink = {
        let heard = Rc::clone(&heard);
        Rc::new(move |w: &job::Work| heard.borrow_mut().push(w.clone()))
    };
    let mut listened = board.clone();
    let routing = job::reporting(sink, || {
        route_with(&mut listened, RouteOptions::default())
    });
    assert_eq!(routing, alone);
    assert_eq!(listened.tracks, quiet.tracks);
    assert_eq!(listened.vias, quiet.vias);

    let heard = heard.borrow();
    let total = heard[0].total.expect("the router knows how many lines it routes");
    assert!(total > 0);
    assert!(heard.iter().all(|w| w.unit == "connections routed"
        && w.total == Some(total)
        && w.done <= total
        && w.detail.is_some()));
    let details: Vec<&str> = heard.iter().filter_map(|w| w.detail.as_deref()).collect();
    assert!(
        details[0].starts_with(&format!("{total} connections on a ")),
        "{details:?}"
    );
    // One line for each try, always told: the try, what it routed and its copper.
    let ended: Vec<&&str> = details.iter().filter(|d| d.contains(": ended with ")).collect();
    assert_eq!(ended.len(), routing.tries, "{details:?}");
    assert!(
        ended[0].starts_with("try 1 of ") && ended[0].ends_with(" vias so far"),
        "{ended:?}"
    );
    let last = heard
        .iter()
        .rev()
        .find(|w| w.detail.as_deref().is_some_and(|d| d.contains(": ended with ")))
        .unwrap();
    if routing.left.is_empty() {
        assert_eq!(last.done, total, "{:?}", last.detail);
    }
}
