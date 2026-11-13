//! The checker can be stopped (ADR 0062): a token that is cancelled before the call, or while
//! it runs, ends it at the next checkpoint, and a call that is not cancelled gives what it gave
//! without the checkpoints.
//!
//! These tests count and time nothing: how soon a cancel is heard is the number of checkpoints
//! the checker reaches after it (`Cancel::checkpoints`), which is the same on a busy runner and
//! on a fast one. The cancel is asked from inside the call, at a checkpoint (`Cancel::at_checkpoint`).

use crate::common;

use netsmith_check::{Loader, check};
use netsmith_core::job::{self, Cancel, Cancelled};

/// More instances than a few checkpoints.
const STAGES: usize = 120;

/// How many checkpoints a whole check of `source` reaches.
fn whole_check(source: &str, loader: &Loader) -> u64 {
    let token = Cancel::new();
    let whole = job::catch(|| job::scope(&token, || check("cancel.nsm", source, loader)))
        .expect("not cancelled");
    assert!(
        whole.is_ok(),
        "the source checks clean:\n{}",
        whole.render()
    );
    token.checkpoints()
}

#[test]
fn a_check_with_a_cancelled_token_stops_at_its_first_checkpoint() {
    let source = common::stages::source(STAGES);
    let loader = Loader::std();
    let whole = whole_check(&source, &loader);
    // The checker looks at its token for each instance it makes, so a cancel is heard within
    // one instance's work.
    assert!(
        whole >= STAGES as u64,
        "a whole check reaches {whole} checkpoints for {STAGES} instances"
    );

    let token = Cancel::new();
    token.cancel();
    let result = job::catch(|| job::scope(&token, || check("cancel.nsm", &source, &loader)));
    assert!(matches!(result, Err(Cancelled)));
    assert_eq!(
        token.checkpoints(),
        1,
        "the check went past a checkpoint of a token that was cancelled (a whole check reaches {whole})"
    );
}

#[test]
fn a_check_is_stopped_while_it_runs_at_the_next_checkpoint() {
    let source = common::stages::source(STAGES * 4);
    let loader = Loader::std();
    let whole = whole_check(&source, &loader);
    // Asked at a checkpoint in the middle of the check.
    let asked_at = whole / 2;
    let token = Cancel::at_checkpoint(asked_at);
    let result = job::catch(|| job::scope(&token, || check("cancel.nsm", &source, &loader)));
    assert!(
        matches!(result, Err(Cancelled)),
        "the check ended before it was asked to"
    );
    assert_eq!(
        token.checkpoints(),
        asked_at,
        "the check went on past the checkpoint it was asked at (a whole check reaches {whole})"
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
