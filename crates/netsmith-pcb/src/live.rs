//! What the placer and the router have made so far, shown while they run (issue #764): the
//! editor's PCB view draws the parts where the placer has them as it works, and the tracks as the
//! router lays them.
//!
//! It is `netsmith_core::job`'s progress again, for pictures instead of counts: the one who
//! starts the work installs a sink for the thread ([`watching_glimpses`]), and the placer and the
//! router hand it a [`Glimpse`] now and then ([`show_glimpse`]). A glimpse is made only when a sink
//! watches ([`glimpses_watched`]), and making one only reads: the placement and the routing are
//! the same bytes with a sink and without one. The placer shows the parts at the end of each
//! stage of its annealer and at the end of a run; the router shows its copper as it reports its
//! progress and at the end of each try. How often a glimpse is sent on is the sink's business
//! (the editor's sends at most a few a second).

use std::{cell::RefCell, rc::Rc};

use crate::{Rect, Track, Via};

/// Where the work stands, as a picture.
#[derive(Debug, Clone, PartialEq)]
pub enum Glimpse {
    /// The parts where the placer has them now, in the region it places them in. The placer
    /// may later move the whole placement (a board with no outline is moved to the origin), so
    /// a glimpse is drawn on its own and not over a board placed before.
    Placing {
        /// The box the parts are placed in.
        region: Rect,
        parts: Vec<GlimpsePart>,
    },
    /// The copper the router has laid so far on the board it was given, besides the copper the
    /// board had.
    Routing { tracks: Vec<Track>, vias: Vec<Via> },
}

/// A part in a [`Glimpse::Placing`].
#[derive(Debug, Clone, PartialEq)]
pub struct GlimpsePart {
    pub refdes: String,
    /// The box round its courtyard where it is.
    pub court: Rect,
    /// Whether it is on the back face.
    pub back: bool,
}

type Sink = Rc<dyn Fn(&Glimpse)>;

thread_local! {
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
}

/// Tells `sink` what [`show_glimpse`] is given on this thread while `work` runs, and puts the
/// sink before it back after, also if `work` unwinds.
pub fn watching_glimpses<R>(sink: Rc<dyn Fn(&Glimpse)>, work: impl FnOnce() -> R) -> R {
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

/// Whether a sink watches this thread, for work that would clone its state only to show it.
pub fn glimpses_watched() -> bool {
    SINK.with(|s| s.borrow().is_some())
}

/// Hands the sink of the thread the glimpse `make` makes; `make` is called only when one
/// watches.
pub fn show_glimpse(make: impl FnOnce() -> Glimpse) {
    // The sink is cloned out so that it may itself start work that shows glimpses.
    let sink = SINK.with(|s| s.borrow().clone());
    if let Some(sink) = sink {
        sink(&make());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pt;

    #[test]
    fn a_glimpse_reaches_the_sink_of_the_thread_and_is_made_only_while_one_watches() {
        let made = std::cell::Cell::new(0);
        let make = || {
            made.set(made.get() + 1);
            Glimpse::Routing {
                tracks: Vec::new(),
                vias: Vec::new(),
            }
        };
        show_glimpse(make);
        assert_eq!(made.get(), 0, "nobody watches, nothing is made");
        assert!(!glimpses_watched());
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&seen);
        watching_glimpses(
            Rc::new(move |g: &Glimpse| sink.borrow_mut().push(g.clone())),
            || {
                assert!(glimpses_watched());
                show_glimpse(make);
                show_glimpse(|| Glimpse::Placing {
                    region: Rect::new(Pt::new(0.0, 0.0), Pt::new(10.0, 5.0)),
                    parts: vec![GlimpsePart {
                        refdes: "R1".into(),
                        court: Rect::new(Pt::new(1.0, 1.0), Pt::new(3.0, 2.0)),
                        back: false,
                    }],
                });
            },
        );
        assert!(!glimpses_watched(), "the sink goes when the work ends");
        assert_eq!(made.get(), 1);
        assert_eq!(seen.borrow().len(), 2);
        // Another thread has its own sink, none here.
        watching_glimpses(Rc::new(|_: &Glimpse| {}), || {
            std::thread::spawn(|| assert!(!glimpses_watched()))
                .join()
                .unwrap();
        });
    }
}
