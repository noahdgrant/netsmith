//! The schematic's layout can be stopped (ADR 0062), and with a token that is not cancelled it
//! draws the same bytes as without one. How soon a cancel is heard is counted in checkpoints and
//! sheets, never in time, so that a busy runner cannot fail it.

use crate::common;

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
    let plain = draw();

    let token = Cancel::new();
    let watched = job::catch(|| job::scope(&token, draw)).expect("not cancelled");
    assert!(
        watched == plain,
        "a token that is not cancelled changes no byte of the drawing"
    );
    let whole = token.checkpoints();

    let token = Cancel::new();
    token.cancel();
    let result = job::catch(|| job::scope(&token, draw));
    assert!(matches!(result, Err(Cancelled)));
    assert_eq!(
        token.checkpoints(),
        1,
        "the drawing went past a checkpoint of a token that was cancelled (a whole drawing reaches {whole})"
    );
}

/// Asked from inside, when the second sheet begins (the same place whatever the load): the
/// drawing reports no sheet after it, and ends cancelled.
#[test]
fn a_drawing_is_stopped_while_it_runs_before_it_begins_another_sheet() {
    let b = common::bench::bench();
    let token = Cancel::new();
    // The sheets reported, and how many of them came after the cancel was asked.
    let heard = std::rc::Rc::new(std::cell::RefCell::new((0u64, 0usize)));
    let sink: std::rc::Rc<dyn Fn(&job::Work)> = {
        let (token, heard) = (token.clone(), std::rc::Rc::clone(&heard));
        std::rc::Rc::new(move |w: &job::Work| {
            if w.unit != "sheets drawn" {
                return;
            }
            if token.is_cancelled() {
                heard.borrow_mut().1 += 1;
            } else if w.done >= 1 {
                heard.borrow_mut().0 = w.done;
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
    let (asked_at, after) = *heard.borrow();
    assert_eq!(asked_at, 1, "the drawing reached its second sheet");
    assert!(matches!(result, Err(Cancelled)));
    assert_eq!(
        after, 0,
        "the drawing began {after} more sheet(s) after it was asked to stop"
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
