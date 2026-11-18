//! A control input strapped to a rail or to ground is at a known level (issue #1304): a pin
//! joined with no part between to the positive pin of a rail is judged at the rail's voltage
//! range against its `vih`, and one on a ground at 0 V against its `vil`, with no note naming the
//! passives on the net.
//!
//! `Transceiver` and `Driver` are invented for these tests; neither is a real part.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::logic::{Levels, Logic}
import std::parts::passive::Resistor
import std::sources::Rail

/// Invented: a radio transceiver whose mode pins are strapped by the board, one table of
/// levels for its I/O pins, and a power-down input with levels of its own.
part Transceiver {
    .vdd: Rail
    .levels: Levels
    .levels.reference ~ .vdd
    .levels.voh &= .vdd.voltage - 0.4V
    .levels.vol &= 0.4V
    .levels.vih &= 0.7 * .vdd.voltage
    .levels.vil &= 0.3 * .vdd.voltage
    .pd: Logic
    .pd.reference ~ .vdd
    .pd.reference.current &= 0A
    .pd.vih &= 0.9 * .vdd.voltage
    pins {
        .vdd.pos  power  "VDD"  "1"
        .vdd.neg  power  "GND"  "2"
        .mode0    input  "M0"   "3" optional
        .mode1    input  "M1"   "4" optional
        .pd.line  input  "PD"   "5" optional
        .irq      output "IRQ"  "6"
    }
}

/// Invented: drives within 0.1 V of its supply.
part Driver {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= .vdd.voltage - 0.1V
    .out.vol &= 0.1V
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}
"#;

const S3: &str = "    .s3: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }\n";
const S18: &str = "    .s18: Rail {\n        .voltage &= 1.8V +/- 5%\n        .pos.net &= \"1V8\"\n        .neg.net &= \"GND\"\n    }\n";

/// Passives on the ground and the supply whose far ends a driver drives: a pull-down and a
/// pull-up on the driver's line, which made every strapped pin a note that named them.
const PASSIVES: &str = "    .a: Driver\n    .s3 ~ .a.vdd\n    .down: Resistor { .resistance &= 10kohm +/- 1% }\n    .up: Resistor { .resistance &= 10kohm +/- 1% }\n    .a.out.line ~> .down ~> .s3.neg\n    .s3.pos ~> .up ~> .a.out.line\n";

fn solved(body: &str) -> Solution {
    let c = check(
        "test.nsm",
        &format!("{PARTS}\nboard B {{\n{body}}}\n"),
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

#[test]
fn inputs_strapped_to_the_supply_and_to_ground_are_judged_with_no_note() {
    let s = solved(&format!(
        "{S3}{PASSIVES}    .u: Transceiver\n    .s3 ~ .u.vdd\n    .u.mode0 ~ .s3.neg\n    .u.mode1 ~ .s3.pos\n    .u.pd.line ~ .s3.pos\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn an_input_strapped_to_a_rail_below_its_high_is_a_fault_with_the_numbers() {
    // A 1.8 V rail is not a high for an input that reads one from 70 % of 3.3 V.
    let s = solved(&format!(
        "{S3}{S18}{PASSIVES}    .u: Transceiver\n    .s3 ~ .u.vdd\n    .u.mode1 ~ .s18.pos\n"
    ));
    assert_eq!(
        faults(&s),
        [
            "net `1V8`: rail `s18` holds the line at 1.71V..1.89V but `u.levels.vih` needs 2.194V..2.426V: a high of 1.71V is not always read as a high, which takes 2.426V"
        ]
    );
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn an_input_strapped_to_ground_that_states_no_low_is_a_note() {
    // `pd` states `vih` only: strapped to ground, its low is not known.
    let s = solved(&format!(
        "{S3}{PASSIVES}    .u: Transceiver\n    .s3 ~ .u.vdd\n    .u.pd.line ~ .s3.neg\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(
        notes(&s),
        ["not evaluated: the low level into `u.pd` on net `GND`: it is a ground, and `u.pd.vil` has no value"]
    );
}

#[test]
fn a_ground_that_a_resistor_joins_to_the_supply_stays_at_zero_volts_for_an_absolute_maximum() {
    // A resistor straight from the supply to ground does not lift the ground to the supply.
    let parts = PARTS.replacen(
        ".mode0    input  \"M0\"   \"3\" optional",
        ".mode0    input  \"M0\"   \"3\" optional abs_max 2V",
        1,
    );
    assert_ne!(parts, PARTS);
    let c = check(
        "test.nsm",
        &format!(
            "{parts}\nboard B {{\n{S3}    .u: Transceiver\n    .s3 ~ .u.vdd\n    .r: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .s3.pos ~> .r ~> .s3.neg\n    .u.mode0 ~ .s3.neg\n}}\n"
        ),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}
