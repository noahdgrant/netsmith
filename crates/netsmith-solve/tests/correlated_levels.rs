//! Levels stated as fractions of one rail's voltage are compared at one supply voltage
//! (ADR 0081). `Mic`, `Sum` and `Receiver` are invented parts: a driver that states its levels
//! as fractions of its supply, one that states a high as the supply less a drop, and an input
//! whose thresholds are fractions of its own supply. None is a real part.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::logic::Logic
import std::sources::Rail

/// A driver whose levels are 0.7 and 0.3 of its supply, as a digital microphone's might be.
part Mic {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= 0.7 * .vdd.voltage
    .out.vol &= .vdd.voltage * 0.3
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2"
        .out.line  output "OUT" "3"
    }
}

/// Like `Mic`, but its high is only 0.65 of its supply.
part WeakHigh {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= 0.65 * .vdd.voltage
    .out.vol &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2"
        .out.line  output "OUT" "3"
    }
}

/// Like `Mic`, but its low is up to 0.35 of its supply.
part WeakLow {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= 0.7 * .vdd.voltage
    .out.vol &= 0.35 * .vdd.voltage
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2"
        .out.line  output "OUT" "3"
    }
}

/// A driver whose high is the supply less 0.1 V: a sum, not a fraction.
part Sum {
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

/// Reads a high from 0.7 of its supply and a low up to 0.3 of it.
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

/// Reads a high from 0.6 of its supply and a low up to 0.4 of it.
part Lenient {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 0.6 * .vdd.voltage
    .in.vil &= 0.4 * .vdd.voltage
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .in.line   input "IN"  "3"
    }
}

/// Reads a high from the supply less 1 V: a sum, not a fraction.
part Offset {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= .vdd.voltage - 1V
    .in.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .in.line   input "IN"  "3"
    }
}

/// Reads a high from 0.97 of its supply.
part Strict {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 0.97 * .vdd.voltage
    .in.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .in.line   input "IN"  "3"
    }
}

/// Reads a high from 0.7 of its supply, and narrows the high by a number as well.
part Narrowed {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 0.7 * .vdd.voltage
    .in.vih &= 2.2V..2.4V
    .in.vil &= 0.3 * .vdd.voltage
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
const S3B: &str = "    .s3b: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3B\"\n        .neg.net &= \"GND\"\n    }\n";

fn one_rail(driver: &str, receiver: &str) -> Solution {
    solved(&format!(
        "{S3}    .a: {driver}\n    .b: {receiver}\n    .s3 ~ .a.vdd\n    .s3 ~ .b.vdd\n    .a.out.line ~ .b.in.line\n"
    ))
}

#[test]
fn matching_fractions_of_one_rail_pass_with_nothing_said() {
    let s = one_rail("Mic", "Receiver");
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn a_driver_with_a_higher_high_fraction_than_the_receiver_needs_passes() {
    let s = one_rail("Mic", "Lenient");
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn a_high_fraction_below_the_threshold_fraction_is_a_fault_naming_the_factors() {
    let s = one_rail("WeakHigh", "Receiver");
    assert_eq!(
        faults(&s),
        [
            "net `net-a.out.line`: `a.out.voh` is 0.65 x `a.vdd.voltage` but `b.in.vih` needs 0.7 x `b.vdd.voltage`, the same supply voltage: a high of 0.65 x the supply is not always read as a high, which takes 0.7 x the supply"
        ]
    );
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn a_low_fraction_above_the_threshold_fraction_is_a_fault_naming_the_factors() {
    let s = one_rail("WeakLow", "Receiver");
    assert_eq!(
        faults(&s),
        [
            "net `net-a.out.line`: `a.out.vol` is 0.35 x `a.vdd.voltage` but `b.in.vil` allows 0.3 x `b.vdd.voltage`, the same supply voltage: a low of 0.35 x the supply is not always read as a low, which takes 0.3 x the supply"
        ]
    );
}

#[test]
fn two_different_rails_are_compared_as_independent_ranges() {
    // The same fractions, but each part has a supply of its own: the voltages are not one
    // number, so the high 2.194V..2.426V is checked against a VIH of the same range.
    let s = solved(&format!(
        "{S3}{S3B}    .a: Mic\n    .b: Receiver\n    .s3 ~ .a.vdd\n    .s3b ~ .b.vdd\n    .a.out.line ~ .b.in.line\n"
    ));
    let f = faults(&s);
    assert_eq!(f.len(), 2, "{f:?}");
    assert!(
        f[0].starts_with("net `net-a.out.line`: `a.out.voh` is 2.194V..2.426V but `b.in.vih` needs 2.194V..2.426V: a high of 2.194V is not always read as a high"),
        "{f:?}"
    );
}

#[test]
fn a_sum_is_compared_as_independent_ranges() {
    // `.vdd.voltage - 0.1V` is not a fraction: the ranges 3.035V..3.365V and 3.04V..3.362V
    // are compared as before.
    let s = one_rail("Sum", "Strict");
    let f = faults(&s);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(
        f[0].contains("`a.out.voh` is 3.035V..3.365V but `b.in.vih` needs 3.04V..3.362V: a high of 3.035V is not always read as a high"),
        "{f:?}"
    );
}

#[test]
fn a_fraction_against_a_sum_is_compared_as_independent_ranges() {
    // 2.194V..2.426V against 2.135V..2.465V: the ranges overlap, so it is refused, though at
    // 3.135 V the high 2.1945 V is above the threshold 2.135 V and at 3.465 V it is below
    // 2.465 V.
    let s = one_rail("Mic", "Offset");
    let f = faults(&s);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(
        f[0].contains("`a.out.voh` is 2.194V..2.426V but `b.in.vih` needs 2.135V..2.465V: a high of 2.194V is not always read as a high"),
        "{f:?}"
    );
}

#[test]
fn a_level_that_a_number_also_narrows_is_not_a_fraction() {
    // `vih` is also narrowed to 2.2V..2.4V, so it is not just 0.7 of the supply: the ranges are
    // compared, 2.194V..2.426V against 2.2V..2.4V.
    let s = one_rail("Mic", "Narrowed");
    let f = faults(&s);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(
        f[0].contains("a high of 2.194V is not always read as a high"),
        "{f:?}"
    );
}
