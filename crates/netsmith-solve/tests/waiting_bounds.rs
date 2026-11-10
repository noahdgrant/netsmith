//! A bound that waits for a part still to be chosen refuses a candidate that no value of the
//! ranges can meet (ADR 0065): the parts chosen later only narrow the ranges. Without it the
//! pick that sets the value is made as if the bound were not there. Every value here is invented.

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

/// A bleeder: a divider from `vout` to ground, whose draw is the quiescent current of the
/// stage, read against the input voltage. The input is a window the board states, and an
/// equation says what it is behind a series resistor and a current that nothing states, so
/// the equation is not evaluated and the series resistor, which feeds the input, is chosen
/// after the divider (path order) without anything telling the bound its choice.
fn bleeder(bound: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor

module Stage {{
    .top: Resistor
    .bottom: Resistor
    .vref: Voltage
    .vout: Voltage
    .vin: Voltage
    .draw: Current
    .quiescent: Current
    .vref &= 1V
    .vout &= 4.9V..5.1V
    .vout == .vref * (1 + .top.resistance / .bottom.resistance)
    .draw == (.vout - .vref) / .top.resistance
    .quiescent == .draw * .vout / (.vin * 0.9)
}}

board B {{
    .a_stage: Stage
    .supply: Voltage
    .load: Current
    .series: Resistor
    .supply &= 12V
    .a_stage.vin &= 10V..12V
    .a_stage.vin == .supply - .series.resistance * .load
    {bound}
}}
"#
    )
}

const BLEEDER: [f64; 7] = [
    1_000.0,
    4_000.0,
    10_000.0,
    40_000.0,
    100_000.0,
    400_000.0,
    1_000_000.0,
];

#[test]
fn a_bound_that_waits_for_a_later_part_still_reaches_the_pick_it_can_limit() {
    // The first pair that fits the output, 1 kohm under 4 kohm, draws 1 mA. a 10 kohm bottom under a 40 kohm top
    // draws 100 uA, whose quiescent current is at least 44 uA at the top of the input window:
    // over the bound whatever the series resistor turns out to be.
    let s = run(&bleeder(".a_stage.quiescent <= 40uA"), &catalogs(&BLEEDER));
    assert_eq!(
        picked(&s)[..2],
        ["a_stage.bottom = 100000", "a_stage.top = 400000"]
    );
    assert!(range(&s, "a_stage.quiescent").hi() <= 40e-6);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
}

#[test]
fn a_bound_that_does_not_wait_is_judged_as_before() {
    // The draw reads nothing that the series resistor feeds, so its bound never waited.
    let s = run(&bleeder(".a_stage.draw <= 20uA"), &catalogs(&BLEEDER));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(
        picked(&s)[..2],
        ["a_stage.bottom = 100000", "a_stage.top = 400000"]
    );
}

#[test]
fn a_bound_that_a_later_value_may_still_meet_does_not_refuse_the_pick() {
    // a 10 kohm bottom under a 40 kohm top gives 55.6 uA at the bottom of the window (10 V) and 46.3 uA at the
    // top (12 V): a bound of 55 uA could hold for a narrower window, so the candidate stays and
    // the check afterwards is the judge. It cannot judge here, where nothing narrows the window,
    // and says so.
    let s = run(&bleeder(".a_stage.quiescent <= 55uA"), &catalogs(&BLEEDER));
    assert_eq!(
        picked(&s)[..2],
        ["a_stage.bottom = 10000", "a_stage.top = 40000"]
    );
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:?}");
    assert!(
        text[0].contains("a_stage.quiescent must always be at most 55uA"),
        "{text:?}"
    );
}

#[test]
fn a_bound_that_no_pair_can_meet_is_refused_by_the_search() {
    let s = run(&bleeder(".a_stage.quiescent <= 0.1uA"), &catalogs(&BLEEDER));
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:?}");
    assert!(
        text[0].starts_with(
            "no combination of candidates satisfies `a_stage.bottom` and `a_stage.top` together"
        ),
        "{text:?}"
    );
    assert!(
        s.picks
            .iter()
            .all(|p| !s.model.path_of(p.part).starts_with("a_stage"))
    );
}
