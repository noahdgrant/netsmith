//! The absolute maximum of a pin, judged against the voltage the DC analysis solved on its net
//! (ADR 0076, amendment 12). Every part and number here is invented.
//!
//! The check of ADR 0046 reads a net's worst case from the rails on it and the resistors one step
//! from them: a divider's middle is the rail it hangs from, and a net behind two resistors is
//! not followed. A net that the analysis solved has the voltage its whole circuit makes of it.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::logic::Logic
import std::parts::passive::Resistor
import std::sources::Rail

/// Reads a high from 2 V and survives -0.5 V to 3.6 V.
part Pad {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 2V
    .in.vil &= 0.8V
    .in.line.abs_min &= -0.5V
    .in.line.abs_max &= 3.6V
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .in.line   input "IN"  "3"
    }
}
"#;

const RAILS: &str = "    .s3: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }\n    .s5: Rail {\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"VCC5\"\n        .neg.net &= \"GND\"\n    }\n    .s5.neg ~ .s3.neg\n    .s3 ~ .b.vdd\n";

fn solved(body: &str) -> Solution {
    let c = check(
        "test.nsm",
        &format!("{PARTS}\nboard B {{\n{RAILS}{body}}}\n"),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn faults(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated.iter().map(|n| n.message.clone()).collect()
}

fn divider(top: &str, bottom: &str) -> String {
    format!(
        "    .b: Pad\n    .rt: Resistor {{ .resistance &= {top} +/- 1% }}\n    .rb: Resistor {{ .resistance &= {bottom} +/- 1% }}\n    .s5.pos ~> .rt ~> .b.in.line\n    .b.in.line ~> .rb ~> .s5.neg\n"
    )
}

/// 10 kohm over 20 kohm from 5.25 V gives at most `5.25 * 20.2 / (20.2 + 9.9)` = 3.523 V, inside
/// the 3.6 V the pin survives. The step from the rail alone said 5.25 V, and was a fault.
#[test]
fn a_divider_is_judged_by_the_voltage_of_its_middle() {
    let s = solved(&divider("10kohm", "20kohm"));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}

/// 10 kohm over 30 kohm: at most `5.25 * 30.3 / (30.3 + 9.9)` = 3.957 V, over the 3.6 V.
#[test]
fn a_divider_that_goes_over_the_maximum_is_a_fault_with_the_analysis_voltage() {
    let s = solved(&divider("10kohm", "30kohm"));
    assert_eq!(
        faults(&s),
        [
            "net `net-b.in.line` reaches 3.957V above its ground `GND` (the DC analysis of the network it is in) but `b.in.line.abs_max` is 3.6V: it must not exceed 3.6V"
        ]
    );
}

/// A net behind two resistors: the step from the rail the check made stopped at the first, found
/// the pull-down to the ground and said 0 V. The circuit has the pin at 5.25 V less the drops:
/// with 39 kohm below, at most `5.25 * 39.39 / (39.39 + 19.8)` = 3.494 V, inside; with 68 kohm
/// below `5.25 * 68.68 / (68.68 + 19.8)` = 4.075 V, over.
#[test]
fn a_net_behind_two_resistors_has_the_voltage_the_circuit_gives_it() {
    let chain = |bottom: &str| {
        format!(
            "    .b: Pad\n    .r1: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .r2: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .rb: Resistor {{ .resistance &= {bottom} +/- 1% }}\n    .s5.pos ~> .r1 ~> .r2 ~> .b.in.line\n    .b.in.line ~> .rb ~> .s5.neg\n"
        )
    };
    let s = solved(&chain("39kohm"));
    assert_eq!(faults(&s), Vec::<String>::new());
    let s = solved(&chain("68kohm"));
    assert_eq!(
        faults(&s),
        [
            "net `net-b.in.line` reaches 4.075V above its ground `GND` (the DC analysis of the network it is in) but `b.in.line.abs_max` is 3.6V: it must not exceed 3.6V"
        ]
    );
}
