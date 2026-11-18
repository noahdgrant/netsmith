//! A part states the logic levels of its plain pins once, as its `.levels` (issue #1190): every
//! plain pin of a kind the levels check reads is judged by them, and a pin of an interface (a
//! `Logic` line) keeps its own.
//!
//! `Sensor`, `Driver` and `Receiver` are invented for these tests; none is a real part.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::logic::{Levels, Logic}
import std::sources::Rail

/// Invented: one table of levels for its I/O pins, and a reset input with levels of its own.
part Sensor {
    .vdd: Rail
    .levels: Levels
    .levels.reference ~ .vdd
    .levels.voh &= .vdd.voltage - 0.4V
    .levels.vol &= 0.4V
    .levels.vih &= 0.7 * .vdd.voltage
    .levels.vil &= 0.3 * .vdd.voltage
    .rst: Logic
    .rst.reference ~ .vdd
    .rst.reference.current &= 0A
    .rst.vih &= 0.9 * .vdd.voltage
    .rst.vil &= 0.1 * .vdd.voltage
    pins {
        .vdd.pos  power   "VDD"  "1"
        .vdd.neg  power   "GND"  "2"
        .dout     output  "DOUT" "3"
        .din      input   "DIN"  "4" optional abs_max 2.1V
        .rst.line input   "RST"  "5" optional
        .ana      passive "ANA"  "6"
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

/// Invented: reads a high from 70 % of its supply and a low up to 30 %.
part Receiver {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 0.7 * .vdd.voltage
    .in.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos power "VDD" "1"
        .vdd.neg power "GND" "2"
        .in.line input "IN"  "3"
    }
}
"#;

const S3: &str = "    .s3: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }\n";
const S18: &str = "    .s18: Rail {\n        .voltage &= 1.8V +/- 5%\n        .pos.net &= \"1V8\"\n        .neg.net &= \"GND\"\n    }\n";

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
fn a_plain_output_takes_the_levels_of_its_part_and_is_judged_with_no_note() {
    let s = solved(&format!(
        "{S3}    .s: Sensor\n    .b: Receiver\n    .s3 ~ .s.vdd\n    .s3 ~ .b.vdd\n    .s.dout ~ .b.in.line\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn a_plain_output_whose_part_s_high_is_not_read_as_a_high_is_a_fault_with_the_numbers() {
    let s = solved(&format!(
        "{S3}{S18}    .s: Sensor\n    .b: Receiver\n    .s18 ~ .s.vdd\n    .s3 ~ .b.vdd\n    .s.dout ~ .b.in.line\n"
    ));
    assert_eq!(
        faults(&s),
        [
            "net `net-b.in.line`: `s.levels.voh` is 1.31V..1.49V but `b.in.vih` needs 2.194V..2.426V: a high of 1.31V is not always read as a high, which takes 2.426V"
        ]
    );
}

#[test]
fn a_plain_input_takes_the_levels_of_its_part_and_its_limit_is_measured_from_their_ground() {
    // Both on 1.8 V: the high is read as a high and the line stays within 2.1 V.
    let s = solved(&format!(
        "{S18}    .s: Sensor\n    .a: Driver\n    .s18 ~ .s.vdd\n    .s18 ~ .a.vdd\n    .a.out.line ~ .s.din\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
    // A driver on 3.3 V puts 3.465 V on the input, measured from the ground of the part's
    // levels.
    let s = solved(&format!(
        "{S3}{S18}    .s: Sensor\n    .a: Driver\n    .s18 ~ .s.vdd\n    .s3 ~ .a.vdd\n    .a.out.line ~ .s.din\n"
    ));
    assert_eq!(
        faults(&s),
        [
            "net `net-a.out.line` reaches 3.465V above its ground `GND` (`a.out.line`, a line of `a.out.reference`) but `s.din.abs_max` is 2.1V: it must not exceed 2.1V"
        ]
    );
}

#[test]
fn a_logic_line_of_the_part_keeps_its_own_levels() {
    // The reset input reads a high from 90 % of its supply, which a high of 3.035 V is not at
    // 3.465 V; the part's levels (70 %) would have passed it.
    let s = solved(&format!(
        "{S3}    .s: Sensor\n    .a: Driver\n    .s3 ~ .s.vdd\n    .s3 ~ .a.vdd\n    .a.out.line ~ .s.rst.line\n"
    ));
    let f = faults(&s);
    assert_eq!(f.len(), 1, "{f:#?}");
    assert!(f[0].contains("`s.rst.vih`"), "{f:#?}");
}

#[test]
fn a_passive_pin_takes_no_levels_from_its_part() {
    // A passive pin is no driver and no receiver: nothing is judged and nothing is said.
    let s = solved(&format!(
        "{S3}    .s: Sensor\n    .a: Driver\n    .s3 ~ .s.vdd\n    .s3 ~ .a.vdd\n    .a.out.line ~ .s.ana\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}
