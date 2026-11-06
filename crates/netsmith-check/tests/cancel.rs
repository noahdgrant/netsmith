//! The checker can be stopped (ADR 0062): a token that is cancelled before the call, or while
//! it runs, ends it at the next checkpoint, and a call that is not cancelled gives what it gave
//! without the checkpoints.

mod common;

use std::time::{Duration, Instant};

use netsmith_check::{Loader, check};
use netsmith_core::job::{self, Cancel, Cancelled};

/// More instances than a few milliseconds of checking.
const STAGES: usize = 120;

#[test]
fn a_check_with_a_cancelled_token_stops_at_once() {
    let source = common::stages::source(STAGES);
    let loader = Loader::std();
    let started = Instant::now();
    let whole = check("cancel.nsm", &source, &loader);
    let whole_took = started.elapsed();
    assert!(
        whole.is_ok(),
        "the source checks clean:\n{}",
        whole.render()
    );

    let token = Cancel::new();
    token.cancel();
    let started = Instant::now();
    let result = job::catch(|| job::scope(&token, || check("cancel.nsm", &source, &loader)));
    let stopped_after = started.elapsed();
    assert!(matches!(result, Err(Cancelled)));
    assert!(
        stopped_after < Duration::from_millis(100),
        "stopped after {stopped_after:?}, a whole check takes {whole_took:?}"
    );
}

#[test]
fn a_check_is_stopped_while_it_runs_within_a_fraction_of_a_second() {
    let source = common::stages::source(STAGES * 4);
    let loader = Loader::std();
    let token = Cancel::new();
    let asked = std::sync::Arc::new(std::sync::Mutex::new(None));
    let timer = {
        let (token, asked) = (token.clone(), std::sync::Arc::clone(&asked));
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(15));
            *asked.lock().unwrap() = Some(Instant::now());
            token.cancel();
        })
    };
    let result = job::catch(|| job::scope(&token, || check("cancel.nsm", &source, &loader)));
    let latency = asked.lock().unwrap().expect("asked").elapsed();
    timer.join().unwrap();
    assert!(
        matches!(result, Err(Cancelled)),
        "the check ended before it was asked to"
    );
    assert!(
        latency < Duration::from_millis(300),
        "stopped {latency:?} after it was asked"
    );
}

#[test]
fn a_check_whose_token_is_not_cancelled_is_the_check_without_a_token() {
    let source = common::stages::source(8);
    let loader = Loader::std();
    let plain = check("cancel.nsm", &source, &loader);
    let token = Cancel::new();
    let watched = job::catch(|| job::scope(&token, || check("cancel.nsm", &source, &loader)))
        .expect("not cancelled");
    assert_eq!(plain.render(), watched.render());
    assert_eq!(plain.boards.len(), watched.boards.len());
    let model =
        |c: &netsmith_check::Checked| format!("{:?}", c.boards[0].model.instances().count());
    assert_eq!(model(&plain), model(&watched));
}
