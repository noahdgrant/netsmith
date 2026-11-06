//! The solver can be stopped (ADR 0062) and says how far it has got: a token that is cancelled
//! before the call, or while it runs, ends it at the next checkpoint; the parts it has chosen
//! are counted as it goes; and a call that is not cancelled gives what it gave without them.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

use netsmith_check::{Checked, Loader, check};
use netsmith_core::job::{self, Cancel, Cancelled, Work};
use netsmith_solve::{Catalogs, Solution, solve};

#[path = "../../netsmith-check/tests/common/stages.rs"]
mod stages;

fn checked(stages: usize) -> Checked {
    let c = check("cancel.nsm", &stages::source(stages), &Loader::std());
    assert!(c.is_ok(), "the source checks clean:\n{}", c.render());
    c
}

#[test]
fn a_solve_with_a_cancelled_token_stops_at_once() {
    let c = checked(6);
    let catalogs = Catalogs::jlcpcb();
    let started = Instant::now();
    let _ = solve(&c.boards[0], &catalogs);
    let whole = started.elapsed();

    let token = Cancel::new();
    token.cancel();
    let started = Instant::now();
    let result = job::catch(|| job::scope(&token, || solve(&c.boards[0], &catalogs)));
    let stopped_after = started.elapsed();
    assert!(matches!(result, Err(Cancelled)));
    assert!(
        stopped_after < Duration::from_millis(100),
        "stopped after {stopped_after:?}, a whole solve takes {whole:?}"
    );
}

#[test]
fn a_solve_is_stopped_while_it_searches_within_a_fraction_of_a_second() {
    let c = checked(8);
    let catalogs = Catalogs::jlcpcb();
    let token = Cancel::new();
    // Asked from inside, at the fifth part chosen, so that it is certain to be in the search.
    let asked: Rc<Cell<Option<Instant>>> = Rc::default();
    let sink: Rc<dyn Fn(&Work)> = {
        let (token, asked) = (token.clone(), Rc::clone(&asked));
        Rc::new(move |w: &Work| {
            if w.done >= 5 && asked.get().is_none() {
                asked.set(Some(Instant::now()));
                token.cancel();
            }
        })
    };
    let result = job::catch(|| {
        job::reporting(sink, || {
            job::scope(&token, || solve(&c.boards[0], &catalogs))
        })
    });
    let latency = asked
        .get()
        .expect("the search got to its fifth part")
        .elapsed();
    assert!(matches!(result, Err(Cancelled)));
    assert!(
        latency < Duration::from_millis(500),
        "stopped {latency:?} after it was asked"
    );
}

#[test]
fn the_parts_chosen_are_counted_as_the_solve_goes_and_the_total_is_known() {
    let c = checked(3);
    let heard: Rc<RefCell<Vec<Work>>> = Rc::default();
    let sink: Rc<dyn Fn(&Work)> = {
        let heard = Rc::clone(&heard);
        Rc::new(move |w: &Work| heard.borrow_mut().push(w.clone()))
    };
    let solved = job::reporting(sink, || solve(&c.boards[0], &Catalogs::jlcpcb()));
    assert!(solved.diagnostics.is_empty(), "{:?}", solved.diagnostics);
    let all = heard.borrow();
    // Each unit is counted on its own: the parts prepared from their catalogs, the fields scanned
    // for the parts that share relations, then the parts chosen. Each grows, and has its total.
    for unit in ["parts prepared", "parts chosen"] {
        let heard: Vec<&Work> = all.iter().filter(|w| w.unit == unit).collect();
        assert!(heard.len() > 3, "{unit}: {all:?}");
        let total = heard[0].total.expect("the number of parts is known");
        assert!(total >= 10, "a board of three rails has parts: {heard:?}");
        assert!(
            heard.iter().all(|w| w.total == Some(total)),
            "{unit}: {heard:?}"
        );
        assert_eq!(heard[0].done, 0, "{unit} starts at nothing");
        assert!(
            heard.windows(2).all(|p| p[0].done <= p[1].done),
            "{unit} never goes back: {heard:?}"
        );
    }
    let chosen: Vec<&Work> = all.iter().filter(|w| w.unit == "parts chosen").collect();
    assert_eq!(
        chosen.last().unwrap().done,
        chosen[0].total.unwrap(),
        "and ends at the total"
    );
    for w in all.iter() {
        assert!(
            ["parts prepared", "fields scanned", "parts chosen"].contains(&w.unit),
            "{w:?}"
        );
        assert!(
            w.total.is_some(),
            "nothing is reported without a total here: {w:?}"
        );
    }
}

fn picks_of(s: &Solution) -> String {
    format!(
        "{:?} {:?} {:?} {}",
        s.picks, s.unresolved, s.diagnostics, s.trials
    )
}

#[test]
fn a_solve_with_a_token_that_is_not_cancelled_and_a_listener_gives_the_same_solution() {
    let c = checked(3);
    let catalogs = Catalogs::jlcpcb();
    let plain = solve(&c.boards[0], &catalogs);
    let token = Cancel::new();
    let sink: Rc<dyn Fn(&Work)> = Rc::new(|_| {});
    let watched = job::reporting(sink, || {
        job::catch(|| job::scope(&token, || solve(&c.boards[0], &catalogs))).expect("not cancelled")
    });
    assert_eq!(picks_of(&plain), picks_of(&watched));
    assert_eq!(format!("{:?}", plain.model), format!("{:?}", watched.model));
}
