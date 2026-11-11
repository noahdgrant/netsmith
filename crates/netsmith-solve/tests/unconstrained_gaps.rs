//! Two gaps in the unconstrained pick finding (ADR 0065, section 9). A stated load on a net
//! limits a pick only if the pick's value changes whether the budget holds, and a check that
//! holds whatever the size of the values is limits their ratio and not their size. Every value
//! here is invented.

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

fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: [1.0, 330.0, 4_700.0, 10_000.0, 22_000.0, 47_000.0, 100_000.0]
            .iter()
            .map(|&v| resistor(v))
            .collect(),
    });
    c
}

fn run(src: &str) -> Solution {
    let c = check("main.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], &catalogs())
}

fn unconstrained(s: &Solution) -> Vec<String> {
    let mut paths: Vec<String> = s
        .unconstrained
        .iter()
        .map(|u| s.model.path_of(u.part))
        .collect();
    paths.sort();
    paths
}

const RAIL: &str =
    r#".supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }"#;

/// A switch whose pull-up's resistance is held to a member, with a load of its own on the same
/// supply whose current the module states (the pull-up does not feed it).
const SWITCH: &str = r#"
module Switch {
    .supply: Rail
    .load: Rail
    .pullup_resistance: Resistance
    .pass: Resistor { .resistance &= 1ohm +/- 1% }
    .pullup: Resistor
    .pullup.resistance &= .pullup_resistance
    .share: Number
    .share &= 0..1
    .pullup.current == .share * .supply.voltage / .pullup.resistance
    .load.current &= -1mA..0mA
    .supply.pos ~> .pass ~> .load.pos
    .load.neg ~ .supply.neg
    .supply.pos ~> .pullup ~> .supply.neg
    .pass.current == -.load.current
}
"#;

fn switch(lines: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail
{SWITCH}
board B {{
    {RAIL}
    .sw: Switch
    .sw.supply ~ .supply
{lines}
}}
"#
    )
}

#[test]
fn a_stated_load_that_the_pull_up_does_not_feed_is_no_limit_on_it() {
    let s = run(&switch(""));
    assert_eq!(unconstrained(&s), ["sw.pullup"]);
    let u = &s.unconstrained[0];
    // The first row of the catalog that fits.
    assert_eq!(u.row, "1 1%");
    assert_eq!(
        s.model
            .path_of_field(u.held_by.as_ref().expect("held to a member").member),
        "sw.pullup_resistance"
    );
    assert!(u.ratio.is_empty());
}

#[test]
fn stating_the_resistance_ends_that_finding() {
    let s = run(&switch("    .sw.pullup_resistance &= 4.7kohm +/- 5%"));
    assert!(s.unconstrained.is_empty(), "{:?}", unconstrained(&s));
}

/// A divider of two resistors whose output voltage is bounded, in a board that states `lines`.
fn divider(module_lines: &str, lines: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail

module Divider {{
    .supply: Rail
    .total_resistance: Resistance
    .top: Resistor
    .bottom: Resistor
    .total_resistance == .top.resistance + .bottom.resistance
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
    .out: Voltage
    .out == .supply.voltage * .bottom.resistance / .total_resistance
    .out >= 1.5V
    .out <= 1.8V
{module_lines}
}}

board B {{
    {RAIL}
    .divider: Divider
    .divider.supply ~ .supply
{lines}
}}
"#
    )
}

#[test]
fn a_ratio_limits_the_ratio_and_not_the_size_of_two_resistors() {
    let s = run(&divider("", ""));
    assert_eq!(unconstrained(&s), ["divider.bottom", "divider.top"]);
    for u in &s.unconstrained {
        assert_eq!(
            u.ratio.len(),
            2,
            "the two bounds on `.out` are the ratio checks"
        );
    }
}

#[test]
fn a_range_on_the_total_limits_the_size() {
    let s = run(&divider(
        "",
        "    .divider.total_resistance &= 10kohm..200kohm",
    ));
    assert!(s.unconstrained.is_empty(), "{:?}", unconstrained(&s));
}

#[test]
fn a_range_on_one_resistor_ties_the_other_to_it() {
    // The other resistor's size follows from the ratio, so it is limited too.
    let s = run(&divider("    .bottom.resistance &= 10kohm..100kohm", ""));
    assert!(s.unconstrained.is_empty(), "{:?}", unconstrained(&s));
}

#[test]
fn a_check_on_the_current_limits_the_size() {
    let s = run(&divider(
        "    .draw: Current\n    .draw == .supply.voltage / .total_resistance\n    .draw <= 200uA\n",
        "",
    ));
    assert!(s.unconstrained.is_empty(), "{:?}", unconstrained(&s));
}

#[test]
fn a_divider_in_a_module_is_found_from_a_board_that_forgot_both() {
    // Both are one finding in the driver; here, the solver names both with the same checks.
    let s = run(&divider("", ""));
    let checks: Vec<_> = s.unconstrained.iter().map(|u| u.ratio.clone()).collect();
    assert_eq!(checks[0], checks[1]);
}
