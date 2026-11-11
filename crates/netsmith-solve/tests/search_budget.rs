//! A search of coupled parts that runs away ends with an answer (issue #177): after a count of
//! trials, or a count of work, whichever it reaches first, and the message names the parts it
//! was choosing together, how many combinations it tried, and what to change. Both are counts
//! and not seconds, so the answer is the same on every machine. The work is what a trial costs
//! on this board: a larger board is given up on after fewer trials. Every part and number here
//! is invented; each test lowers the budget so that it runs in a moment.

use netsmith_check::{Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Candidate, Catalog, Catalogs, SearchBudget, Solution, Value, solve};

fn resistor(ohms: f64) -> Candidate {
    let label = format!("{ohms}");
    let center = Quantity::literal(ohms, Some("ohm")).unwrap().quantity;
    let one_percent = Quantity::literal(1.0, Some("%")).unwrap().quantity;
    Candidate {
        name: format!("{label} 1%"),
        label,
        columns: vec![
            (
                "resistance".into(),
                Value::Quantity(Quantity::tolerance(&center, &one_percent, true).unwrap()),
            ),
            ("package".into(), Value::Text("0402".into())),
            (
                "max_power".into(),
                Value::Quantity(Quantity::literal(1.0, Some("kW")).unwrap().quantity),
            ),
        ],
        lcsc: None,
        mpn: None,
        library: None,
        assembly: None,
        height: None,
        temp_min: None,
        temp_max: None,
        qualification: None,
        generic: None,
    }
}

/// Resistors of 100, 200, ... 800 ohms, with the budget `budget`.
fn catalogs(budget: SearchBudget) -> Catalogs {
    let mut c = Catalogs::empty().with_search_budget(budget);
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: (1..=8).map(|i| resistor(100.0 * i as f64)).collect(),
    });
    c
}

/// `n` resistors `u01`.. in one sum that no choice of them reaches (800 ohms at most each),
/// and `others` resistors that read nothing of it, each with a bound of its own: parts that
/// make the board larger without being part of the search.
fn board(n: usize, others: usize) -> String {
    let names: Vec<String> = (1..=n).map(|i| format!("u{i:02}")).collect();
    let mut src = String::from("import std::parts::passive::Resistor\n\nboard Big {\n");
    for name in &names {
        src.push_str(&format!("    .{name}: Resistor\n"));
    }
    src.push_str("    .total: Resistance\n");
    let sum: Vec<String> = names.iter().map(|n| format!(".{n}.resistance")).collect();
    src.push_str(&format!("    .total == {}\n", sum.join(" + ")));
    src.push_str("    .total >= 100Mohm\n");
    for i in 0..others {
        src.push_str(&format!(
            "    .x{i}: Resistor\n    .y{i}: Voltage\n    .y{i} &= 3V\n    .x{i}.resistance >= .y{i} / 10mA\n    .x{i}.resistance <= .y{i} / 1mA\n"
        ));
    }
    src.push_str("}\n");
    src
}

fn run(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("budget.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

const ADVICE: &str = "a bound reads values of all of them, so no part could be blamed alone. State a value for one of them, narrow a window that a bound on them reads, or split the group so that fewer parts share a bound";

#[test]
fn the_default_budget_is_the_limit_a_build_has() {
    assert_eq!(
        SearchBudget::default(),
        SearchBudget {
            trials: 250_000,
            work: 1_000_000_000
        }
    );
    assert_eq!(Catalogs::empty().search_budget(), SearchBudget::default());
}

#[test]
fn a_search_that_tries_every_combination_ends_at_its_trials_and_names_the_parts() {
    let budget = SearchBudget {
        trials: 300,
        work: u64::MAX,
    };
    let s = run(&board(4, 0), &catalogs(budget));
    assert_eq!(
        messages(&s),
        [format!(
            "gave up after trying 300 combinations of candidates for `u01` and `u02` and `u03` and `u04`; {ADVICE}"
        )]
    );
    assert_eq!(s.trials, 300);
    assert!(s.picks.is_empty());
    assert_eq!(s.unresolved.len(), 4);
}

#[test]
fn a_search_that_does_more_work_than_it_is_given_ends_and_says_it_was_the_work() {
    // Plenty of trials, little work: the board has parts of its own, and every pass of the
    // propagation reads all of their relations.
    let budget = SearchBudget {
        trials: usize::MAX,
        work: 200_000,
    };
    let s = run(&board(4, 20), &catalogs(budget));
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:#?}");
    assert!(
        text[0].starts_with(
            "gave up after trying "
        ) && text[0].contains(" combinations of candidates for `u01` and `u02` and `u03` and `u04` (the search stopped for the work it had done, which grows with the size of the board, and not for the number of combinations it had tried); "),
        "{}",
        text[0]
    );
    assert!(text[0].ends_with(ADVICE), "{}", text[0]);
    assert!(s.work >= 200_000, "{}", s.work);
    assert!(s.trials > 0 && s.trials < 250_000, "{} trials", s.trials);
}

#[test]
fn the_same_work_buys_fewer_trials_on_a_larger_board() {
    let budget = SearchBudget {
        trials: usize::MAX,
        work: 400_000,
    };
    let small = run(&board(4, 0), &catalogs(budget));
    let large = run(&board(4, 40), &catalogs(budget));
    assert_eq!(messages(&small).len(), 1);
    assert_eq!(messages(&large).len(), 1);
    assert!(
        large.trials * 2 < small.trials,
        "{} trials on the small board, {} on the large",
        small.trials,
        large.trials
    );
}

#[test]
fn the_end_of_a_search_does_not_depend_on_the_machine() {
    let budget = SearchBudget {
        trials: usize::MAX,
        work: 300_000,
    };
    let a = run(&board(4, 10), &catalogs(budget));
    let b = run(&board(4, 10), &catalogs(budget));
    assert_eq!((a.trials, a.work), (b.trials, b.work));
    assert_eq!(messages(&a), messages(&b));
}

#[test]
fn a_group_of_many_parts_names_the_first_and_counts_the_rest() {
    let budget = SearchBudget {
        trials: 50,
        work: u64::MAX,
    };
    let s = run(&board(15, 0), &catalogs(budget));
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:#?}");
    assert!(
        text[0].starts_with(
            "gave up after trying 50 combinations of candidates for `u01` and `u02` and `u03` and `u04` and `u05` and `u06` and `u07` and `u08` and `u09` and `u10` and `u11` and `u12` and 3 more; "
        ),
        "{}",
        text[0]
    );
}

#[test]
fn a_search_inside_its_budget_is_not_touched_by_it() {
    // The parts that no bound ties together are searched apart and each is found at once.
    let budget = SearchBudget {
        trials: 300,
        work: 200_000,
    };
    let src = "import std::parts::passive::Resistor\n\nboard Small {\n    .a: Resistor\n    .b: Resistor\n    .a.resistance >= 250ohm\n    .b.resistance >= 450ohm\n}\n";
    let s = run(src, &catalogs(budget));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks.len(), 2);
}
