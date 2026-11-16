//! What a long call shares with whoever started it (ADR 0062): a way to be stopped, and a way
//! to say how far it has got.
//!
//! The checker, the solver, the design stage, the schematic's layout and the placer are single calls of
//! seconds to minutes with a result at the end. Stopping one must not mean a `Result` in
//! hundreds of functions, so the one who starts it installs a [`Cancel`] for the thread
//! ([`scope`]), the loops of the call ask [`checkpoint`] now and then, and a checkpoint that
//! finds the token set ends the call by unwinding with a [`Cancelled`] payload
//! (`std::panic::resume_unwind`, which does not run the panic hook, so nothing is printed). The
//! starter catches it with [`catch`] and drops what the call had made; a panic of any other
//! kind goes on up as it would have. The crates that use it keep no state outside the call
//! (no cache, no lock, no thread), so what an unwind passes is dropped and nothing is left half
//! done.
//!
//! A checkpoint outside any scope costs one thread-local read; inside one, a relaxed atomic add
//! (the count of checkpoints reached, which tests read) and a load. The results of a call that is not cancelled are the same bytes as without the
//! checkpoints: they only read.
//!
//! **Why the call is stopped by unwinding and not by a `Result`.** The long calls are `check`,
//! `solve`, the design stage and the layout, each a tree of hundreds of functions that return
//! values, not results. A `Result` through them would change every signature in five crates
//! and every test, to carry one case that stops the call and drops everything it made. The
//! unwind does exactly that, and drops it correctly because the crates keep no state outside the
//! call: no cache, no lock, no thread, no global (the only interior mutability is a `RefCell` or
//! a `Cell` local to one function's work). It needs `panic = "unwind"`, the default; the release
//! profile does not change it. The editor's `analysis::contained`, which turns a panic of the
//! analysis into a problem, passes a cancellation on. The token is a thread-local and not a
//! `&Cancel` parameter, for the same reason: `solve(board, catalogs)` is called from `fab`,
//! `build`, the editor, the examples and a hundred tests, and none of them has a token to pass.
//! The thread-local is set by one scope at the top of a job and nothing else reads it.
//!
//! **Where the checkpoints are, and what they cost.** One for each unit of work of about a
//! millisecond or more: each trial of the solver's search, each sweep of propagation and each
//! relation or balance it visits, each part and net of the design stage and of the pin checks,
//! each instance the checker makes, each sheet, pass and block of the layout and each of its
//! rule checks, each move of the placer and each ring of its legalising pass, and each test of
//! the router for its time. The ones that
//! matter are in the search; the others are there so that no stage is a place a cancel waits at.
//! Measured in a debug build on generated boards of the `big` fixture's shape
//! (`scripts/editor-verify-js/fixtures/big.mjs` with more rails; a throwaway timer):
//!
//! | Stage | 8 rails (89 parts) | 24 rails | 40 rails |
//! |---|---|---|---|
//! | check (file and modules) | 14 ms | 39 ms | 71 ms |
//! | solve | 2.6 s | 19.0 s | 51.8 s |
//! | design stage (with its export) | 5 ms | 23 ms | 68 ms |
//! | draw | 0.31 s | 0.44 s | 0.59 s |
//!
//! The solve is more than 99 % of the time that is not drawing, and inside it picking parts is
//! all of it: the 24-rail board has one group of 120 parts that share relations, and the search
//! over that group takes 18.3 s of the 19.0 (the coupling and the grouping 0.9 s, propagation,
//! verification and the level checks together under 0.1 s). Each trial of the search copies the
//! model and propagates, so the cost grows with the square of the board. A board with generic
//! parts adds the alternatives `recheck_alternatives` solves again, each a whole solve. A call
//! whose token is not cancelled gives the same bytes (the corpus drawings, the metrics and the
//! solver's picks are compared byte for byte in the `cancel.rs` tests of the crates).
//!
//! **One token for each job, and a job never stops another.** A scope replaces the token of the
//! scope around it. The analysis of the live project runs under a token of its own, which the
//! backend's `cancel_analysis` sets; it is not the token of the request that happened to
//! trigger it. A compare, a build, a fab and an order each run under the token of their
//! request, and the analysis a compare makes of a ref runs under the compare's.
//!
//! **Progress.** The solver counts what it can count as it goes, each count growing with a known
//! total: **parts prepared** (read from their catalogs, the first seconds of a big board),
//! **fields scanned** (finding which parts share relations, which takes seconds on a big board
//! and is done twice) and then **parts chosen**, of the parts to choose in the board (`41 of 118
//! parts chosen`). Measured on the 40-rail board in a debug build: the first line at 0 s, 0 of
//! 441 parts prepared; the search begins at 6 s and then counts about ten parts a second, slower
//! as the model grows. The search backs up, so the count is the most parts chosen at once, which
//! only grows: it stalls while the search backs up and does not go back. The groups of parts that
//! share relations are searched one after the other and the count runs over all of them. The
//! re-solves for generic parts (ADR 0047), made when the bill of materials or `fab` shows an
//! unresolved part and not when the board is analysed, report **alternative parts re-solved**
//! with no total, because the number of alternatives is not known before they are made; the
//! solve those make of the board does not report its own count, so the count of the board's
//! solve is not told again from zero. The drawing counts **sheets drawn**. A line is sent at most
//! once in 100 ms, except the first and the last of a step. No percentage is made up from them,
//! and none is shown.
//!
//! **Limits.** A call is stopped at its next checkpoint, so the latency is the longest stretch
//! between two: a trial of the search, and for a solver that is slower per trial than the board
//! measured here, longer. Nothing inside a single `try_candidate`, a single sheet's wire routing
//! or a single placer move is interruptible, and a cancel that arrives while the analysis is
//! writing its result (after its last checkpoint) is too late and is not applied: the analysis is
//! published whole, and `cancelled` in the answer of the route was true for a job that ended
//! anyway. The drawing of a sheet of a real board can be long between two checkpoints (the
//! layout's inner routing has none); its latency is not measured on a board of that kind.
//!
//! **A limit that keeps what was made** (issue #764). A cancel drops what the call made; a
//! [`Limit`] ends it early and keeps it, as the router's Stop keeps its copper. The one who
//! starts the call installs it for the thread ([`limited`]); the call asks [`spent`] before it
//! begins another try and begins none once it is true, and a loop inside a try asks
//! [`time_up`] now and then and ends with what it has; nothing unwinds. A limit is a
//! time, a number of tries, or both, whichever comes first: a try is what the call says it is
//! (the placer: one run of the annealer in a fixed outline) and is counted by [`spend_try`]. A
//! limit of tries gives the same result on every machine; a limit of time does not, and a call
//! that ends within it gives the same bytes as with none. The placer is its only user: `fab`
//! and `board3d` run each board's placing under `--place-budget` (`place_budget` in
//! `netsmith.toml`).
//!
//! **What a step has found.** A [`Work`] may carry a `detail`, a sentence a person reads, for a
//! step whose count alone does not say whether it is going well: the placer says, after each
//! run, how many parts were still out and which. A sink that shows counts at a slower rate
//! shows a line with a detail always.

use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// A request to stop, shared by whoever may ask and the call that is asked. Cloning shares it.
///
/// It also counts the checkpoints that the calls under it have reached ([`Cancel::checkpoints`]),
/// which is how a test says how far a call went after a cancel: in checkpoints, which are the
/// same on every machine, and not in milliseconds, which are not.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<Shared>);

#[derive(Debug, Default)]
struct Shared {
    asked: AtomicBool,
    /// How many checkpoints under this token have been reached (the one that stopped the call
    /// included).
    reached: AtomicU64,
    /// The checkpoint that cancels the token when it is reached, 0 for none ([`Cancel::at_checkpoint`]).
    cancel_at: AtomicU64,
}

impl Cancel {
    pub fn new() -> Cancel {
        Cancel::default()
    }

    /// A token that cancels itself at the `n`th checkpoint a call under it reaches (`n` of at
    /// least 1), and that checkpoint stops the call: the call has reached `n` checkpoints when
    /// it ends, never more. For tests, which ask for a cancel at a known place of the work
    /// whatever the load of the machine, and not after a time.
    pub fn at_checkpoint(n: u64) -> Cancel {
        assert!(n >= 1, "checkpoints are counted from 1");
        let token = Cancel::new();
        token.0.cancel_at.store(n, Ordering::Relaxed);
        token
    }

    /// Ask the call that watches this to stop at its next checkpoint. It stays asked.
    pub fn cancel(&self) {
        self.0.asked.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.asked.load(Ordering::Relaxed)
    }

    /// How many checkpoints the calls under this token have reached, the one that stopped a
    /// call included. A call that is stopped by a token cancelled before it began has reached
    /// one.
    pub fn checkpoints(&self) -> u64 {
        self.0.reached.load(Ordering::Relaxed)
    }

    /// Counts one checkpoint reached, and says whether the call is to stop.
    fn reach(&self) -> bool {
        let n = self.0.reached.fetch_add(1, Ordering::Relaxed) + 1;
        if n == self.0.cancel_at.load(Ordering::Relaxed) {
            self.cancel();
        }
        self.is_cancelled()
    }
}

/// The payload a cancelled call unwinds with. [`catch`] tells it from a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

thread_local! {
    static CURRENT: RefCell<Option<Cancel>> = const { RefCell::new(None) };
    static WATCHED: Cell<bool> = const { Cell::new(false) };
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
    static LIMIT: RefCell<Option<Running>> = const { RefCell::new(None) };
}

/// Puts the thread's token back to what it was, however the scope ends.
struct Restore(Option<Cancel>);

impl Drop for Restore {
    fn drop(&mut self) {
        let previous = self.0.take();
        WATCHED.with(|w| w.set(previous.is_some()));
        CURRENT.with(|c| *c.borrow_mut() = previous);
    }
}

/// Runs `work` with `token` as the thread's: a [`checkpoint`] in it unwinds once `token` is
/// cancelled. It replaces the token of an enclosing scope for as long as it runs (a job that
/// must not be stopped by its starter's token starts its own), and puts it back after, also if
/// `work` unwinds.
pub fn scope<R>(token: &Cancel, work: impl FnOnce() -> R) -> R {
    let previous = CURRENT.with(|c| c.borrow_mut().replace(token.clone()));
    WATCHED.with(|w| w.set(true));
    let _restore = Restore(previous);
    work()
}

/// Runs `work` with no token, so that a checkpoint in it never stops it: for the part of a job
/// that must finish once it has begun (writing the files of a package).
pub fn uninterruptible<R>(work: impl FnOnce() -> R) -> R {
    let previous = CURRENT.with(|c| c.borrow_mut().take());
    WATCHED.with(|w| w.set(false));
    let _restore = Restore(previous);
    work()
}

/// Whether the thread's token has been cancelled, for a loop that can stop cleanly and
/// without unwinding.
#[inline]
pub fn is_cancelled() -> bool {
    WATCHED.with(Cell::get)
        && CURRENT.with(|c| c.borrow().as_ref().is_some_and(Cancel::is_cancelled))
}

/// A place where a call may be stopped: ends it with [`Cancelled`] if the thread's token is
/// cancelled, else does nothing. Put it in a loop at a grain of a millisecond or so; it is no
/// cost to put one in each pass of a loop of that size. The token counts it
/// ([`Cancel::checkpoints`]).
#[inline]
pub fn checkpoint() {
    if WATCHED.with(Cell::get) && CURRENT.with(|c| c.borrow().as_ref().is_some_and(Cancel::reach)) {
        resume_unwind(Box::new(Cancelled));
    }
}

/// Runs `work` and gives its result, or [`Cancelled`] if it ended in a checkpoint. A panic of
/// any other kind is not caught: it unwinds on from here.
pub fn catch<R>(work: impl FnOnce() -> R) -> Result<R, Cancelled> {
    catch_unwind(AssertUnwindSafe(work)).map_err(|payload| {
        if payload.is::<Cancelled>() {
            Cancelled
        } else {
            resume_unwind(payload)
        }
    })
}

/// Whether a panic's payload is a cancellation, for code that catches panics itself and must
/// not take a cancellation for a bug.
pub fn is_cancellation(payload: &(dyn std::any::Any + Send)) -> bool {
    payload.is::<Cancelled>()
}

/// How far a call has got through a unit of work that it can count as it goes: `done` of
/// `total` `unit`s. `total` is `None` when it is not known; nothing is estimated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    /// What is counted, in the plural: `parts chosen`.
    pub unit: &'static str,
    pub done: u64,
    pub total: Option<u64>,
    /// What the step has found so far, as a sentence, when the count alone does not say it
    /// ([`report_detail`]).
    pub detail: Option<String>,
}

type Sink = Rc<dyn Fn(&Work)>;

/// Tells `sink` what [`report`] is told on this thread while `work` runs.
pub fn reporting<R>(sink: Rc<dyn Fn(&Work)>, work: impl FnOnce() -> R) -> R {
    struct Put(Option<Sink>);
    impl Drop for Put {
        fn drop(&mut self) {
            let previous = self.0.take();
            SINK.with(|s| *s.borrow_mut() = previous);
        }
    }
    let previous = SINK.with(|s| s.borrow_mut().replace(sink));
    let _put = Put(previous);
    work()
}

/// Says how far the call has got. A call reports only what it counts and what grows: at most
/// once for each step of the count.
pub fn report(unit: &'static str, done: u64, total: Option<u64>) {
    tell(|| Work {
        unit,
        done,
        total,
        detail: None,
    });
}

/// [`report`], with what the step has found so far: a sentence a person reads. The sentence
/// is made only when a sink listens.
pub fn report_detail(
    unit: &'static str,
    done: u64,
    total: Option<u64>,
    detail: impl FnOnce() -> String,
) {
    tell(|| Work {
        unit,
        done,
        total,
        detail: Some(detail()),
    });
}

fn tell(work: impl FnOnce() -> Work) {
    SINK.with(|s| {
        // The sink is cloned out so that it may itself call `report` or `reporting`.
        let sink = s.borrow().clone();
        if let Some(sink) = sink {
            sink(&work());
        }
    });
}

/// How much a call may do before it ends with what it has: a time, a number of tries, or both,
/// whichever comes first. `Limit::default()` is no limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Limit {
    pub time: Option<Duration>,
    pub tries: Option<u64>,
}

impl Limit {
    /// Whether it limits anything.
    pub fn is_none(&self) -> bool {
        self.time.is_none() && self.tries.is_none()
    }
}

impl std::fmt::Display for Limit {
    /// `120 s`, `5 tries`, `120 s or 5 tries`, or `no limit`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let time = self.time.map(|t| {
            let secs = t.as_secs_f64();
            if secs.fract() == 0.0 {
                format!("{secs:.0} s")
            } else {
                format!("{secs:.1} s")
            }
        });
        let tries = self
            .tries
            .map(|n| format!("{n} {}", if n == 1 { "try" } else { "tries" }));
        match (time, tries) {
            (Some(t), Some(n)) => write!(f, "{t} or {n}"),
            (Some(t), None) => f.write_str(&t),
            (None, Some(n)) => f.write_str(&n),
            (None, None) => f.write_str("no limit"),
        }
    }
}

/// A [`Limit`] being spent.
struct Running {
    deadline: Option<Instant>,
    tries: Option<u64>,
    used: Cell<u64>,
}

/// Runs `work` with `limit` as the thread's: [`spent`] in it is true once the limit is used up.
/// It replaces the limit of an enclosing call while it runs and puts it back after, also if
/// `work` unwinds. The time is counted from here.
pub fn limited<R>(limit: Limit, work: impl FnOnce() -> R) -> R {
    struct Put(Option<Running>);
    impl Drop for Put {
        fn drop(&mut self) {
            let previous = self.0.take();
            LIMIT.with(|l| *l.borrow_mut() = previous);
        }
    }
    let running = (!limit.is_none()).then(|| Running {
        deadline: limit.time.map(|t| Instant::now() + t),
        tries: limit.tries,
        used: Cell::new(0),
    });
    let previous = LIMIT.with(|l| std::mem::replace(&mut *l.borrow_mut(), running));
    let _put = Put(previous);
    work()
}

/// Whether the thread's limit is used up: its time has passed, or every try it allows has
/// begun. False with no limit. A call asks it before it begins another try, and begins none
/// when it is true; a try that has begun is ended early only by [`time_up`].
pub fn spent() -> bool {
    LIMIT.with(|l| {
        l.borrow().as_ref().is_some_and(|r| {
            r.deadline.is_some_and(|d| Instant::now() >= d)
                || r.tries.is_some_and(|n| r.used.get() >= n)
        })
    })
}

/// Whether the time of the thread's limit has passed. False with no limit or no time. A loop
/// inside a try that can end with what it has asks this and ends.
pub fn time_up() -> bool {
    LIMIT.with(|l| {
        l.borrow()
            .as_ref()
            .is_some_and(|r| r.deadline.is_some_and(|d| Instant::now() >= d))
    })
}

/// Counts one try begun under the thread's limit. Nothing with no limit.
pub fn spend_try() {
    LIMIT.with(|l| {
        if let Some(r) = l.borrow().as_ref() {
            r.used.set(r.used.get() + 1);
        }
    });
}

/// How many tries have begun under the thread's limit, 0 with none.
pub fn tries_spent() -> u64 {
    LIMIT.with(|l| l.borrow().as_ref().map_or(0, |r| r.used.get()))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn a_checkpoint_outside_any_scope_does_nothing() {
        checkpoint();
        assert!(!is_cancelled());
    }

    #[test]
    fn a_checkpoint_in_a_scope_stops_the_call_once_the_token_is_cancelled() {
        let token = Cancel::new();
        let mut passes = 0;
        let result = catch(|| {
            scope(&token, || {
                loop {
                    checkpoint();
                    passes += 1;
                    if passes == 4 {
                        token.cancel();
                    }
                }
            })
        });
        assert_eq!(result, Err(Cancelled));
        assert_eq!(
            passes, 4,
            "the pass that cancelled finished, the next stopped"
        );
        assert!(
            !is_cancelled(),
            "the token of the scope is not the thread's any more"
        );
    }

    #[test]
    fn the_token_counts_the_checkpoints_reached_and_a_token_at_a_checkpoint_stops_there() {
        let token = Cancel::new();
        scope(&token, || (0..7).for_each(|_| checkpoint()));
        assert_eq!(token.checkpoints(), 7);
        // Outside a scope, and in an uninterruptible part, nothing is counted.
        checkpoint();
        scope(&token, || uninterruptible(checkpoint));
        assert_eq!(token.checkpoints(), 7);

        let token = Cancel::at_checkpoint(3);
        let passes = Cell::new(0);
        let result = catch(|| {
            scope(&token, || {
                loop {
                    checkpoint();
                    passes.set(passes.get() + 1);
                }
            })
        });
        assert_eq!(result, Err(Cancelled));
        assert_eq!(passes.get(), 2, "the third checkpoint stops the call");
        assert_eq!(token.checkpoints(), 3);

        // A token cancelled before the call stops it at its first checkpoint.
        let token = Cancel::new();
        token.cancel();
        assert_eq!(catch(|| scope(&token, checkpoint)), Err(Cancelled));
        assert_eq!(token.checkpoints(), 1);
    }

    #[test]
    fn a_token_that_is_not_cancelled_changes_nothing() {
        let token = Cancel::new();
        let result = catch(|| scope(&token, || (0..1000).inspect(|_| checkpoint()).sum::<u32>()));
        assert_eq!(result, Ok(499_500));
    }

    #[test]
    fn a_real_panic_is_not_taken_for_a_cancellation() {
        let caught = std::panic::catch_unwind(|| {
            // Keep the test's output quiet: the hook is not what is tested.
            let _ = catch(|| -> () { panic!("a bug") });
        });
        let payload = caught.expect_err("the panic goes on up");
        assert!(!is_cancellation(&*payload));
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"a bug"));
    }

    #[test]
    fn a_cancellation_does_not_run_the_panic_hook() {
        // `resume_unwind` is silent; this is what the editor relies on to print nothing.
        let hook_ran = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&hook_ran);
        let previous = std::panic::take_hook();
        // Only this thread's panics count: the other tests run beside this one and may panic.
        let mine = std::thread::current().id();
        std::panic::set_hook(Box::new(move |_| {
            if std::thread::current().id() == mine {
                seen.store(true, Ordering::SeqCst);
            }
        }));
        let token = Cancel::new();
        token.cancel();
        let result = catch(|| scope(&token, checkpoint));
        std::panic::set_hook(previous);
        assert_eq!(result, Err(Cancelled));
        assert!(!hook_ran.load(Ordering::SeqCst));
    }

    #[test]
    fn a_scope_puts_the_enclosing_token_back_also_when_it_unwinds() {
        let outer = Cancel::new();
        let inner = Cancel::new();
        inner.cancel();
        scope(&outer, || {
            assert!(catch(|| scope(&inner, checkpoint)).is_err());
            // Back in the outer scope, whose token is not cancelled.
            checkpoint();
            outer.cancel();
            assert!(is_cancelled());
        });
        assert!(!is_cancelled());
    }

    #[test]
    fn an_uninterruptible_part_is_not_stopped_and_the_token_comes_back() {
        let token = Cancel::new();
        token.cancel();
        let result = catch(|| {
            scope(&token, || {
                uninterruptible(|| {
                    checkpoint();
                    assert!(!is_cancelled());
                });
                checkpoint();
            })
        });
        assert_eq!(
            result,
            Err(Cancelled),
            "the checkpoint after it still stops"
        );
    }

    #[test]
    fn the_token_of_one_thread_does_not_stop_another() {
        let token = Cancel::new();
        token.cancel();
        scope(&token, || {
            std::thread::spawn(|| {
                checkpoint();
                assert!(!is_cancelled());
            })
            .join()
            .unwrap();
        });
    }

    #[test]
    fn a_limit_of_tries_is_spent_when_every_try_has_begun_and_only_inside_it() {
        assert!(!spent());
        let limit = Limit {
            time: None,
            tries: Some(2),
        };
        limited(limit, || {
            assert!(!spent());
            spend_try();
            assert!(!spent());
            spend_try();
            assert!(spent());
            assert!(!time_up(), "a limit of tries has no time");
            assert_eq!(tries_spent(), 2);
            // An inner limit replaces it while it runs.
            limited(Limit::default(), || {
                assert!(!spent());
                spend_try();
            });
            assert!(spent());
        });
        assert!(!spent());
        spend_try();
        assert_eq!(tries_spent(), 0);
    }

    #[test]
    fn a_limit_of_no_time_is_spent_at_once() {
        let limit = Limit {
            time: Some(Duration::ZERO),
            tries: None,
        };
        assert!(limited(limit, spent));
        assert!(limited(limit, time_up));
        assert_eq!(limit.to_string(), "0 s");
        assert_eq!(
            Limit {
                time: Some(Duration::from_secs(120)),
                tries: Some(1)
            }
            .to_string(),
            "120 s or 1 try"
        );
        assert_eq!(Limit::default().to_string(), "no limit");
    }

    #[test]
    fn work_is_reported_to_the_sink_of_the_thread_and_only_while_it_runs() {
        let heard = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&heard);
        report("parts chosen", 0, Some(1));
        reporting(
            Rc::new(move |w: &Work| sink.borrow_mut().push((w.unit, w.done, w.total))),
            || {
                report("parts chosen", 1, Some(3));
                report("rails settled", 2, None);
                report_detail("placer runs", 1, None, || "placed".to_string());
            },
        );
        report("parts chosen", 3, Some(3));
        assert_eq!(
            *heard.borrow(),
            [
                ("parts chosen", 1, Some(3)),
                ("rails settled", 2, None),
                ("placer runs", 1, None)
            ]
        );
    }
}
