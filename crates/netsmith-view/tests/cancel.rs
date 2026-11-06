//! The schematic's layout can be stopped (ADR 0062), and with a token that is not cancelled it
//! draws the same bytes as without one.

mod common;

use std::time::{Duration, Instant};

use netsmith_core::job::{self, Cancel, Cancelled};
use netsmith_view::{Marks, render_svg_by_module_marked};

#[test]
fn a_drawing_with_a_cancelled_token_stops_at_once_and_with_another_is_the_same_drawing() {
    let b = common::bench::bench();
    let draw = || {
        render_svg_by_module_marked(
            &b.design,
            &b.groups,
            &b.rails,
            &Marks::default(),
            &b.mapping,
            &b.lib,
        )
        .expect("draws")
    };
    let started = Instant::now();
    let plain = draw();
    let whole = started.elapsed();

    let token = Cancel::new();
    let watched = job::catch(|| job::scope(&token, draw)).expect("not cancelled");
    assert!(
        watched == plain,
        "a token that is not cancelled changes no byte of the drawing"
    );

    token.cancel();
    let started = Instant::now();
    let result = job::catch(|| job::scope(&token, draw));
    let stopped_after = started.elapsed();
    assert!(matches!(result, Err(Cancelled)));
    assert!(
        stopped_after < Duration::from_millis(100),
        "stopped after {stopped_after:?}, a whole drawing takes {whole:?}"
    );
}

#[test]
fn a_drawing_is_stopped_while_it_runs() {
    let b = common::bench::bench();
    let token = Cancel::new();
    let asked = std::sync::Arc::new(std::sync::Mutex::new(None));
    let started = Instant::now();
    // Counted in sheets: asked from inside, at the second sheet.
    let sink: std::rc::Rc<dyn Fn(&job::Work)> = {
        let (token, asked) = (token.clone(), std::sync::Arc::clone(&asked));
        std::rc::Rc::new(move |w: &job::Work| {
            if w.unit == "sheets drawn" && w.done >= 1 && asked.lock().unwrap().is_none() {
                *asked.lock().unwrap() = Some(Instant::now());
                token.cancel();
            }
        })
    };
    let result = job::catch(|| {
        job::reporting(sink, || {
            job::scope(&token, || {
                render_svg_by_module_marked(
                    &b.design,
                    &b.groups,
                    &b.rails,
                    &Marks::default(),
                    &b.mapping,
                    &b.lib,
                )
            })
        })
    });
    let asked = asked
        .lock()
        .unwrap()
        .expect("the drawing reached its second sheet");
    assert!(matches!(result, Err(Cancelled)));
    assert!(
        asked.elapsed() < Duration::from_millis(500),
        "stopped {:?} after it was asked (the whole call took {:?})",
        asked.elapsed(),
        started.elapsed()
    );
}

#[test]
fn the_sheets_drawn_are_counted_with_their_total() {
    let b = common::bench::bench();
    let heard = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink: std::rc::Rc<dyn Fn(&job::Work)> = {
        let heard = std::rc::Rc::clone(&heard);
        std::rc::Rc::new(move |w: &job::Work| heard.borrow_mut().push((w.done, w.total)))
    };
    job::reporting(sink, || {
        render_svg_by_module_marked(
            &b.design,
            &b.groups,
            &b.rails,
            &Marks::default(),
            &b.mapping,
            &b.lib,
        )
        .expect("draws")
    });
    let heard = heard.borrow();
    let total = heard.first().and_then(|(_, t)| *t).expect("a total");
    assert!(total > 1, "the bench is drawn on several sheets: {heard:?}");
    assert!(heard.iter().all(|(_, t)| *t == Some(total)));
    assert!(heard.windows(2).all(|p| p[0].0 < p[1].0), "{heard:?}");
}
