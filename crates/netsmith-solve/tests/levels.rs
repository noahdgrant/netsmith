//! Logic levels and absolute maximums (ADR 0046), checked after the rails are solved.
//!
//! `Driver`, `Receiver` and `Pad` are invented for these tests: parts with a supply and one
//! `Logic` line whose levels are what a datasheet might say. None is a real part and none is
//! in the standard library.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::logic::Logic
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail

/// Drives a line to within 0.1 V of its supply and to 0.1 V at the bottom.
part Driver {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= .vdd.voltage - 0.1V
    .out.vol &= 0.1V
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2"
        .out.line  output "OUT" "3"
    }
}

/// Drives a line and states no level at all.
part Silent {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2"
        .out.line  output "OUT" "3"
    }
}

/// Drives a line to within 0.1 V of its supply, but only down to 1 V at the bottom.
part Weak {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= .vdd.voltage - 0.1V
    .out.vol &= 1V
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2"
        .out.line  output "OUT" "3"
    }
}

/// Reads a high from 70 % of its supply and a low up to 30 %.
part Receiver {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 0.7 * .vdd.voltage
    .in.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .in.line   input "IN"  "3"
    }
}

/// Reads a high from 2 V and a low up to 0.8 V, and survives -0.5 V to 3.6 V.
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

/// A receiver that states no levels.
part Bare {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .in.line   input "IN"  "3"
    }
}
"#;

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

const S3: &str = "    .s3: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }\n";
const S5: &str = "    .s5: Rail {\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"VCC5\"\n        .neg.net &= \"GND\"\n    }\n";

#[test]
fn a_driver_and_a_receiver_that_agree_pass_with_nothing_said() {
    let s = solved(&format!(
        "{S3}    .a: Driver\n    .b: Receiver\n    .s3 ~ .a.vdd\n    .s3 ~ .b.vdd\n    .a.out.line ~ .b.in.line\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn a_high_that_is_not_always_read_as_a_high_is_a_fault_with_the_numbers() {
    // A 3.3 V driver into an input that wants 70 % of 5 V.
    let s = solved(&format!(
        "{S3}{S5}    .a: Driver\n    .b: Receiver\n    .s3 ~ .a.vdd\n    .s5 ~ .b.vdd\n    .a.out.line ~ .b.in.line\n"
    ));
    assert_eq!(
        faults(&s),
        [
            "net `net-a.out.line`: `a.out.voh` is 3.035V..3.365V but `b.in.vih` needs 3.325V..3.675V: a high of 3.035V is not always read as a high, which takes 3.675V"
        ]
    );
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn a_low_that_is_not_always_read_as_a_low_is_a_fault_with_the_numbers() {
    // The input's low threshold is 0.8 V; this driver's low is up to 1 V.
    let s = solved(&format!(
        "{S3}    .a: Weak\n    .b: Pad\n    .s3 ~ .a.vdd\n    .s3 ~ .b.vdd\n    .a.out.line ~ .b.in.line\n"
    ));
    let f = faults(&s);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(
        f[0].contains("`a.out.vol` is 1V but `b.in.vil` allows 800mV: a low of 1V is not always read as a low, which takes 800mV"),
        "{f:?}"
    );
}

#[test]
fn grounds_that_are_not_the_same_net_are_not_evaluated() {
    let s = solved(&format!(
        "{S3}    .s5: Rail {{\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"VCC5\"\n        .neg.net &= \"GND5\"\n    }}\n    .a: Driver\n    .b: Receiver\n    .s3 ~ .a.vdd\n    .s5 ~ .b.vdd\n    .a.out.line ~ .b.in.line\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(
        notes(&s),
        [
            "not evaluated: the logic levels of `a.out` against `b.in` on net `net-a.out.line`: their grounds differ (`a.out.reference.neg` is on net `GND`, `b.in.reference.neg` on net `GND5`)"
        ]
    );
}

#[test]
fn a_level_with_no_value_is_not_evaluated_and_never_passes() {
    let s = solved(&format!(
        "{S3}    .a: Driver\n    .b: Bare\n    .s3 ~ .a.vdd\n    .s3 ~ .b.vdd\n    .a.out.line ~ .b.in.line\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(
        notes(&s),
        [
            "not evaluated: the high level from `a.out` to `b.in` on net `net-a.out.line`: `b.in.vih` has no value",
            "not evaluated: the low level from `a.out` to `b.in` on net `net-a.out.line`: `b.in.vil` has no value",
        ]
    );
}

#[test]
fn two_pins_that_state_no_level_at_all_are_not_looked_at() {
    // Both lines are `Logic`, neither states a level: as before levels existed.
    let s = solved(&format!(
        "{S3}    .a: Bare\n    .b: Bare\n    .c: Silent\n    .s3 ~ .a.vdd\n    .s3 ~ .b.vdd\n    .s3 ~ .c.vdd\n    .c.out.line ~ .a.in.line\n    .c.out.line ~ .b.in.line\n"
    ));
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn the_voltage_of_a_net_is_held_against_the_absolute_maximum_of_a_pin_on_it() {
    // Within 3.6 V: 3.3 V +5 % = 3.465 V.
    let s = solved(&format!(
        "{S3}    .a: Driver\n    .b: Pad\n    .s3 ~ .a.vdd\n    .s3 ~ .b.vdd\n    .a.out.line ~ .b.in.line\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
    // A 5 V driver into the same pad swings to 5.25 V.
    let s = solved(&format!(
        "{S3}{S5}    .a: Driver\n    .b: Pad\n    .s5 ~ .a.vdd\n    .s3 ~ .b.vdd\n    .a.out.line ~ .b.in.line\n"
    ));
    assert_eq!(
        faults(&s),
        [
            "net `net-a.out.line` reaches 5.25V above its ground `GND` (`a.out.line`, a line of `a.out.reference`) but `b.in.line.abs_max` is 3.6V: it must not exceed 3.6V"
        ]
    );
}

#[test]
fn a_resistor_to_a_rail_pulls_the_net_to_it() {
    let s = solved(&format!(
        "{S3}{S5}    .b: Pad\n    .r: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .s3 ~ .b.vdd\n    .s5.pos ~> .r ~> .b.in.line\n    .s5.neg ~ .s3.neg\n"
    ));
    let f = faults(&s);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(
        f[0].contains("reaches 5.25V above its ground `GND` (a resistor to rail `s5`) but `b.in.line.abs_max` is 3.6V"),
        "{f:?}"
    );
}

#[test]
fn a_net_nothing_sets_the_voltage_of_is_not_evaluated() {
    let s = solved(&format!(
        "{S3}    .b: Pad\n    .s3 ~ .b.vdd\n    .j: Header1x2\n    .b.in.line ~ .j.p1\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert!(
        notes(&s)
            .iter()
            .all(|n| n.contains("nothing sets its voltage")),
        "{:?}",
        notes(&s)
    );
    assert!(!notes(&s).is_empty());
}

#[test]
fn joining_two_lines_as_interfaces_does_not_equate_their_levels() {
    // `voh` of one and `vih` of the other are different numbers; the join shares the rail.
    let s = solved(&format!(
        "{S3}    .a: Driver\n    .b: Receiver\n    .s3 ~ .a.vdd\n    .s3 ~ .b.vdd\n    .a.out ~ .b.in\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}
