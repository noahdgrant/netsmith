//! The checker can be stopped (ADR 0062): a token that is cancelled before the call, or while
//! it runs, ends it at the next checkpoint, and a call that is not cancelled gives what it gave
//! without the checkpoints.

mod common;

use std::time::{Duration, Instant};

use netsmith_check::{Loader, check};
use netsmith_core::job::{self, Cancel, Cancelled};

/// More instances than a few milliseconds of checking.
const STAGES: usize = 120;

/// How many times a timing is tried: a busy machine (the whole workspace's tests run at once) only ever makes a
/// delay longer, so the shortest of a few is the one that says how soon the checker looks at its token. A checker
/// that looks too seldom is slow every time and still fails.
const ATTEMPTS: usize = 5;

/// The shortest of up to [`ATTEMPTS`] runs of `once`, which stops trying when one is under `limit`.
fn best_of(limit: Duration, mut once: impl FnMut() -> Duration) -> Duration {
    let mut best = Duration::MAX;
    for _ in 0..ATTEMPTS {
        best = best.min(once());
        if best < limit {
            break;
        }
    }
    best
}

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

    let limit = Duration::from_millis(100);
    let stopped_after = best_of(limit, || {
        let token = Cancel::new();
        token.cancel();
        let started = Instant::now();
        let result = job::catch(|| job::scope(&token, || check("cancel.nsm", &source, &loader)));
        let took = started.elapsed();
        assert!(matches!(result, Err(Cancelled)));
        took
    });
    assert!(
        stopped_after < limit,
        "stopped after {stopped_after:?} at best, a whole check takes {whole_took:?}"
    );
}

#[test]
fn a_check_is_stopped_while_it_runs_within_a_fraction_of_a_second() {
    let source = common::stages::source(STAGES * 4);
    let loader = Loader::std();
    let limit = Duration::from_millis(300);
    let latency = best_of(limit, || {
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
        latency
    });
    assert!(
        latency < limit,
        "stopped {latency:?} after it was asked, at best of {ATTEMPTS}"
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
