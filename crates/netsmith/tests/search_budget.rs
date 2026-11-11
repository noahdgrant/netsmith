//! A build whose search of coupled parts runs away ends with an error that names the parts and
//! says what to change, and does not print a part list for a board it could not build (issue
//! #177). The budget is lowered for the test so that it ends in a moment. Every part and number
//! here is invented.

use netsmith::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_quantity::Quantity;
use netsmith_solve::{Candidate, Catalog, Catalogs, SearchBudget, Value};

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

fn catalogs(budget: SearchBudget) -> Catalogs {
    let mut c = Catalogs::empty().with_search_budget(budget);
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: (1..=8).map(|i| resistor(100.0 * i as f64)).collect(),
    });
    c
}

/// Four resistors in one sum that none of their combinations reaches.
const BOARD: &str = "import std::parts::passive::Resistor

board Stack {
    .a: Resistor
    .b: Resistor
    .c: Resistor
    .d: Resistor
    .total: Resistance
    .total == .a.resistance + .b.resistance + .c.resistance + .d.resistance
    .total >= 100Mohm
}
";

fn build(budget: SearchBudget) -> Result<Vec<netsmith::Built>, String> {
    build_with(
        "stack.nsm",
        BOARD,
        &Loader::std(),
        &catalogs(budget),
        &Libraries::sample(),
    )
}

#[test]
fn a_runaway_search_is_an_error_that_names_the_parts_and_what_to_change() {
    let error = build(SearchBudget {
        trials: 300,
        work: u64::MAX,
    })
    .map(|_| ())
    .expect_err("no combination reaches 100 Mohm");
    assert!(
        error.contains("gave up after trying 300 combinations of candidates for `a` and `b` and `c` and `d`; a bound reads values of all of them"),
        "{error}"
    );
    assert!(
        error.contains("State a value for one of them, narrow a window that a bound on them reads, or split the group"),
        "{error}"
    );
    assert!(error.starts_with("error: "), "{error}");
    assert!(error.contains("stack.nsm:"), "{error}");
}

#[test]
fn a_search_that_ran_out_of_work_says_so_in_the_build_too() {
    let error = build(SearchBudget {
        trials: usize::MAX,
        work: 5_000,
    })
    .map(|_| ())
    .expect_err("no combination reaches 100 Mohm");
    assert!(
        error.contains("combinations of candidates for `a` and `b` and `c` and `d` (the search stopped for the work it had done, which grows with the size of the board"),
        "{error}"
    );
}
