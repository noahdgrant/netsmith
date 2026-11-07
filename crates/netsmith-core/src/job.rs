//! What a long call shares with whoever started it (ADR 0062): a way to be stopped, and a way
//! to say how far it has got.
//!
//! The checker, the solver, lowering, the schematic's layout and the placer are single calls of
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
//! A checkpoint outside any scope costs one thread-local read; inside one, one more relaxed
//! atomic load. The results of a call that is not cancelled are the same bytes as without the
//! checkpoints: they only read.

use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// A request to stop, shared by whoever may ask and the call that is asked. Cloning shares it.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Cancel {
        Cancel::default()
    }

    /// Ask the call that watches this to stop at its next checkpoint. It stays asked.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
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
/// cost to put one in each pass of a loop of that size.
#[inline]
pub fn checkpoint() {
    if is_cancelled() {
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
    SINK.with(|s| {
        // The sink is cloned out so that it may itself call `report` or `reporting`.
        let sink = s.borrow().clone();
        if let Some(sink) = sink {
            sink(&Work { unit, done, total });
        }
    });
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
    fn work_is_reported_to_the_sink_of_the_thread_and_only_while_it_runs() {
        let heard = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&heard);
        report("parts chosen", 0, Some(1));
        reporting(
            Rc::new(move |w: &Work| sink.borrow_mut().push((w.unit, w.done, w.total))),
            || {
                report("parts chosen", 1, Some(3));
                report("rails settled", 2, None);
            },
        );
        report("parts chosen", 3, Some(3));
        assert_eq!(
            *heard.borrow(),
            [("parts chosen", 1, Some(3)), ("rails settled", 2, None)]
        );
    }
}
