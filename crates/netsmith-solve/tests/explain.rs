//! With `Catalogs::with_explain`, each pick says which rows its part passed over and why
//! (issue #744): the rows that do not fit what the design states, and the rows a trial refused
//! with the first relation each failed. Without it nothing is recorded, and the search is the
//! same either way. Every value here is invented.

use netsmith_check::{Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Candidate, Catalog, Catalogs, PassedWhy, Solution, Value, solve};

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
        manufacturer: None,
        mpn: None,
        library: None,
        assembly: None,
        height: None,
        temp_min: None,
        temp_max: None,
        max_voltage_steps: None,
        qualification: None,
        generic: None,
    }
}

/// Rows of 100, 200, 300, 400 and 500 ohms, in that order.
fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: (1..=5).map(|i| resistor(100.0 * i as f64)).collect(),
    });
    c
}

/// `.bound` is refused by a relation on the two rows below 290 ohms; `.stated` states 400 ohms,
/// which the three rows before it do not fit.
const SRC: &str = "import std::parts::passive::Resistor

board Bench {
    .bound: Resistor
    .bound.resistance >= 290ohm
    .stated: Resistor { .resistance &= 400ohm +/- 5% }
}
";

fn run(catalogs: &Catalogs) -> Solution {
    let c = check("explain.nsm", SRC, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn pick<'a>(s: &'a Solution, path: &str) -> &'a netsmith_solve::Pick {
    s.picks
        .iter()
        .find(|p| s.model.path_of(p.part) == path)
        .unwrap_or_else(|| panic!("no pick of `{path}`"))
}

#[test]
fn a_pick_names_the_rows_it_passed_over_and_why() {
    let s = run(&catalogs().with_explain(true));
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);

    let bound = pick(&s, "bound");
    assert_eq!(bound.candidate, "300 1%");
    let rows: Vec<(&str, PassedWhy)> = bound
        .passed
        .iter()
        .map(|p| (p.row.as_str(), p.why))
        .collect();
    assert_eq!(
        rows,
        [("100 1%", PassedWhy::Refused), ("200 1%", PassedWhy::Refused)]
    );
    // The refusal is the relation's, with where it is written.
    assert!(
        bound.passed[0].message.contains("bound.resistance"),
        "{}",
        bound.passed[0].message
    );
    assert!(bound.passed[0].at.is_some());

    let stated = pick(&s, "stated");
    assert_eq!(stated.candidate, "400 1%");
    let rows: Vec<(&str, PassedWhy)> = stated
        .passed
        .iter()
        .map(|p| (p.row.as_str(), p.why))
        .collect();
    assert_eq!(
        rows,
        [
            ("100 1%", PassedWhy::Unfit),
            ("200 1%", PassedWhy::Unfit),
            ("300 1%", PassedWhy::Unfit)
        ]
    );
    let message = &stated.passed[0].message;
    assert!(
        message.contains("`resistance`") && message.contains("`stated.resistance` is 380Ω..420Ω"),
        "{message}"
    );
    // Nothing held either part to its row.
    assert!(bound.held.is_none() && stated.held.is_none());
}

#[test]
fn without_explain_nothing_is_recorded_and_the_search_is_the_same() {
    let plain = run(&catalogs());
    let explained = run(&catalogs().with_explain(true));
    assert!(plain.picks.iter().all(|p| p.passed.is_empty() && p.held.is_none()));
    let rows = |s: &Solution| -> Vec<String> { s.picks.iter().map(|p| p.candidate.clone()).collect() };
    assert_eq!(rows(&plain), rows(&explained));
    // Recording counts as no trial and no work.
    assert_eq!((plain.trials, plain.work), (explained.trials, explained.work));
}

/// Solves `body` as the body of a board of resistors.
fn fails(body: &str, catalogs: &Catalogs) -> Solution {
    let src = format!("import std::parts::passive::Resistor\n\nboard Bench {{\n{body}}}\n");
    let c = check("explain.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn rows(passed: &[netsmith_solve::Passed]) -> Vec<(&str, PassedWhy)> {
    passed.iter().map(|p| (p.row.as_str(), p.why)).collect()
}

#[test]
fn a_part_no_row_works_for_says_what_refused_each_row() {
    let s = fails(
        "    .r: Resistor\n    .r.resistance >= 700ohm\n",
        &catalogs().with_explain(true),
    );
    assert!(!s.diagnostics.is_empty());
    assert!(s.picks.is_empty());
    assert_eq!(s.unpicked.len(), 1, "{:?}", s.unpicked);
    let r = &s.unpicked[0];
    assert_eq!(s.model.path_of(r.part), "r");
    assert_eq!(r.catalog, "resistor");
    assert!(r.with.is_empty());
    assert_eq!(rows(&r.passed).len(), 5);
    assert!(r.passed.iter().all(|p| p.why == PassedWhy::Refused && p.at.is_some()));
    assert!(r.passed[0].message.contains("r.resistance"), "{}", r.passed[0].message);
}

#[test]
fn a_part_with_no_row_that_fits_says_which_columns_do_not() {
    let s = fails(
        "    .r: Resistor { .resistance &= 700ohm +/- 5% }\n",
        &catalogs().with_explain(true),
    );
    assert_eq!(s.unpicked.len(), 1, "{:?}", s.unpicked);
    let r = &s.unpicked[0];
    assert!(r.passed.iter().all(|p| p.why == PassedWhy::Unfit));
    assert_eq!(r.passed.len(), 5);
    assert!(r.passed[0].message.contains("`r.resistance` is"), "{}", r.passed[0].message);
}

#[test]
fn the_parts_of_a_failing_group_each_keep_their_first_turn() {
    let s = fails(
        "    .a: Resistor\n    .b: Resistor\n    .a.resistance + .b.resistance >= 650ohm\n    .a.resistance + .b.resistance <= 690ohm\n",
        &catalogs().with_explain(true),
    );
    assert!(!s.diagnostics.is_empty());
    let paths: Vec<String> = s.unpicked.iter().map(|u| s.model.path_of(u.part)).collect();
    assert_eq!(paths, ["a", "b"]);
    // The first part's rows were each accepted and the second part had nothing after them; the
    // second part's turn is the one made with the first part's first row.
    let (a, b) = (&s.unpicked[0], &s.unpicked[1]);
    assert!(a.with.is_empty() && a.passed.iter().all(|p| p.why == PassedWhy::Later));
    assert_eq!(b.with, ["a = 100 1%"]);
    assert!(b.passed.iter().all(|p| p.why == PassedWhy::Refused));
}

#[test]
fn without_explain_a_failure_keeps_no_record() {
    let s = fails("    .r: Resistor\n    .r.resistance >= 700ohm\n", &catalogs());
    assert!(!s.diagnostics.is_empty());
    assert!(s.unpicked.is_empty());
}
