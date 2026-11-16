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
