//! A pick prefers a candidate for which every bound that waits for a later part holds over the
//! whole range its value has, and falls back to one for which a bound can only possibly hold,
//! in catalog order within each tier (ADR 0065, 9.6). Every part and number here is invented.

use netsmith_check::{Loader, check};
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
        qualification: None,
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
    let c = check("tiers.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn picked(s: &Solution) -> Vec<String> {
    s.picks
        .iter()
        .map(|p| format!("{} = {}", s.model.path_of(p.part), p.label))
        .collect()
}

fn errors(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// `.a` is picked first (path order) and `.twice` is twice its resistance. The bound
/// `.twice <= .z.resistance` names no field of `.a`, and `.z`, which feeds its right side, is
/// chosen after `.a`, so the bound waits. `.z` is stated to be 200 ohm to 1 kohm.
fn board() -> String {
    r#"import std::parts::passive::Resistor

board B {
    .a: Resistor
    .z: Resistor
    .twice: Resistance
    .twice == .a.resistance * 2
    .z.resistance &= 200ohm..1kohm
    .twice <= .z.resistance
}
"#
    .to_string()
}

#[test]
fn a_candidate_for_which_the_waiting_bound_holds_over_the_whole_range_is_taken_first() {
    // 300 ohm doubled is 600 ohm: it can be at most what `.z` is (200 to 1000 ohm), for some
    // values of `.z` and not for all. 50 ohm doubled is 100 ohm, below all of them. The old
    // order took 300 ohm, the first that could hold; the 50 ohm row holds whatever `.z` is.
    let s = run(&board(), &catalogs(&[300.0, 50.0, 800.0]));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert_eq!(picked(&s), ["a = 50", "z = 300"]);
}

#[test]
fn candidates_in_a_tier_keep_catalog_order() {
    // 60 and 70 ohm both hold over the whole range; 300 ohm only for some values.
    let s = run(&board(), &catalogs(&[300.0, 70.0, 60.0, 800.0]));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert_eq!(picked(&s), ["a = 70", "z = 300"]);
}

#[test]
fn a_candidate_for_which_the_bound_can_only_possibly_hold_is_still_accepted_when_no_other_exists() {
    // Both rows double to 600 ohm and 700 ohm: neither is below every value of `.z`, each can be
    // met by some. The first in catalog order is taken, and `.z` is picked to meet it.
    let s = run(&board(), &catalogs(&[300.0, 350.0, 800.0]));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert_eq!(picked(&s), ["a = 300", "z = 800"]);
}

#[test]
fn a_bound_no_value_can_meet_still_refuses_a_candidate_in_both_tiers() {
    // 800 ohm doubled is above 1 kohm, the top of what `.z` may be.
    let s = run(&board(), &catalogs(&[800.0, 900.0]));
    let text = errors(&s);
    assert_eq!(text.len(), 1, "{text:?}");
    assert!(text[0].contains("is always above 200Ω..1kΩ"), "{text:?}");
}
