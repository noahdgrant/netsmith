//! A bound or a stated window on a value that an equation works out from the parts' columns
//! takes part in choosing the parts (ADR 0057), and the check made after the pick is still
//! the judge. Every value here is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Candidate, Catalog, Catalogs, Solution, Value, solve};

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
        generic: None,
    }
}

fn catalogs(values: &[f64]) -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: values.iter().map(|&v| resistor(v)).collect(),
    });
    c
}

fn run(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("derived.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn picked(s: &Solution) -> Vec<String> {
    s.picks
        .iter()
        .map(|p| format!("{} = {}", s.model.path_of(p.part), p.label))
        .collect()
}

fn range(s: &Solution, path: &str) -> Quantity {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

/// A divider whose ratio is a named value and whose bound is on that name.
fn divider(bound: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor

module Divider {{
    .top: Resistor
    .bottom: Resistor
    .ratio: Number
    .ratio == .bottom.resistance / (.top.resistance + .bottom.resistance)
    {bound}
}}

board B {{
    .divider: Divider
}}
"#
    )
}

const DIVIDER: [f64; 4] = [1_000.0, 10_000.0, 100_000.0, 1_000_000.0];

#[test]
fn a_bound_on_a_named_ratio_takes_part_in_choosing_the_pair() {
    // The first pair in catalog order (1 kohm over 1 kohm) has a ratio of 0.5.
    let s = run(&divider(".ratio <= 0.02"), &catalogs(&DIVIDER));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(s.unresolved.is_empty());
    assert_eq!(
        picked(&s),
        ["divider.bottom = 1000", "divider.top = 100000"]
    );
    assert!(range(&s, "divider.ratio").hi() <= 0.02);
}

#[test]
fn the_same_bound_written_on_the_resistors_picks_the_same_pair() {
    let named = run(&divider(".ratio <= 0.02"), &catalogs(&DIVIDER));
    let inline = run(
        &divider(".bottom.resistance / (.top.resistance + .bottom.resistance) <= 0.02"),
        &catalogs(&DIVIDER),
    );
    assert_eq!(picked(&named), picked(&inline));
}

#[test]
fn a_named_bound_that_no_pair_meets_is_refused_with_the_pair_and_the_numbers() {
    // The smallest ratio the catalog gives is 1 kohm over 1 Mohm + 1 kohm, about 0.001.
    let s = run(&divider(".ratio <= 0.0001"), &catalogs(&DIVIDER));
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:?}");
    assert!(
        text[0].starts_with(
            "no combination of candidates satisfies `divider.bottom` and `divider.top` together"
        ),
        "{text:?}"
    );
    assert!(
        text[0].contains("divider.ratio must always be at most 0.0001"),
        "{text:?}"
    );
    assert!(!s.unresolved.is_empty());
    assert!(s.picks.is_empty());
}

#[test]
fn a_bound_waits_for_the_part_that_has_not_been_chosen_yet() {
    // While `bottom` is chosen, `top` is anything from 500 ohm to 2 Mohm and the ratio is a
    // wide range that the bound refuses; the pair that meets it must still be found.
    let s = run(
        &divider(".top.resistance &= 500ohm..2Mohm\n    .ratio <= 0.02"),
        &catalogs(&DIVIDER),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(
        picked(&s),
        ["divider.bottom = 1000", "divider.top = 100000"]
    );
}

/// A module's own declared value, bounded by the board, and worked out from two picks.
fn quiescent(bound: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor

module Sense {{
    .top: Resistor
    .bottom: Resistor
    .quiescent: Current
    .quiescent == 10V / (.top.resistance + .bottom.resistance)
}}

board B {{
    .sense: Sense
    {bound}
}}
"#
    )
}

#[test]
fn a_bound_on_a_modules_declared_value_takes_part_in_choosing_its_parts() {
    // 10 V over the pair must be at most 150 uA: at least 66.7 kohm in all.
    let s = run(&quiescent(".sense.quiescent <= 150uA"), &catalogs(&DIVIDER));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s), ["sense.bottom = 1000", "sense.top = 100000"]);
    assert!(range(&s, "sense.quiescent").hi() <= 150e-6);
}

#[test]
fn a_bound_on_a_declared_value_that_nothing_meets_is_refused_after_trying_every_pair() {
    let s = run(&quiescent(".sense.quiescent <= 1uA"), &catalogs(&DIVIDER));
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:?}");
    assert!(text[0].contains("sense.quiescent"), "{text:?}");
    assert!(s.picks.is_empty());
}

/// The window of a frequency that a resistor sets through two named values, one a default.
fn frequency(window: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor

module Clock {{
    .rt: Resistor
    .nominal: Frequency
    .tolerance: Number
    .fsw: Frequency
    .nominal == 46.5MHz * 1kohm / (.rt.resistance + 5.2kohm)
    .tolerance &= 0.95..1.05
    .fsw == .nominal * .tolerance
    {window}
}}

board B {{
    .clock: Clock
}}
"#
    )
}

const RT: [f64; 4] = [10_000.0, 20_000.0, 41_200.0, 90_000.0];

#[test]
fn a_default_window_on_a_derived_frequency_takes_part_in_choosing_the_resistor() {
    let s = run(&frequency(".fsw ?= 0.9MHz..1.1MHz"), &catalogs(&RT));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s), ["clock.rt = 41200"]);
}

#[test]
fn a_stated_window_on_a_derived_frequency_takes_part_in_choosing_the_resistor() {
    let s = run(&frequency(".fsw &= 0.9MHz..1.1MHz"), &catalogs(&RT));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s), ["clock.rt = 41200"]);
}

#[test]
fn a_window_that_no_resistor_meets_is_refused_by_the_check() {
    let s = run(
        &frequency(".fsw &= 5MHz..6MHz"),
        &catalogs(&[41_200.0, 90_000.0]),
    );
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:?}");
    assert!(
        text[0].contains("clock.fsw") && text[0].contains("5MHz..6MHz"),
        "{text:?}"
    );
}
