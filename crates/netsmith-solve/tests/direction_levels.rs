//! A bidirectional pin whose direction a strapped control pin fixes (issue #1306): the part
//! states `direction .a.line to .b.line when .dir high`, and the levels check judges only the
//! direction the strap of `.dir` fixes, and both where it cannot tell.
//!
//! `Shift`, `Host` and `Reader` are invented for these tests; none is a real part.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::logic::Logic
import std::sources::Rail

/// Invented: a one-bit translator whose A side drives a weak high (half its supply), so that a
/// high it drives into the host is never read as one.
part Shift {
    .vcca: Rail
    .vccb: Rail
    .vccb.neg ~ .vcca.neg
    .a: Logic
    .a.reference ~ .vcca
    .a.voh &= 0.5 * .vcca.voltage
    .a.vol &= 0.1V
    .a.vih &= 0.7 * .vcca.voltage
    .a.vil &= 0.3 * .vcca.voltage
    .b: Logic
    .b.reference ~ .vccb
    .b.voh &= .vccb.voltage - 0.1V
    .b.vol &= 0.1V
    .b.vih &= 0.7 * .vccb.voltage
    .b.vil &= 0.3 * .vccb.voltage
    direction .a to .b when .dir high
    direction .b to .a when .dir low
    pins {
        .vcca.pos power         "VCCA" "1"
        .vccb.pos power         "VCCB" "2"
        .vcca.neg power         "GND"  "3"
        .a.line   bidirectional "A"    "4"
        .b.line   bidirectional "B"    "5"
        .dir      input         "DIR"  "6"
    }
}

/// Invented: a processor's general-purpose pin, and an output that can drive the translator's
/// control pin.
part Host {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    .io.voh &= .vdd.voltage - 0.1V
    .io.vol &= 0.1V
    .io.vih &= 0.7 * .vdd.voltage
    .io.vil &= 0.3 * .vdd.voltage
    .ctl: Logic
    .ctl.reference ~ .vdd
    .ctl.voh &= .vdd.voltage - 0.1V
    .ctl.vol &= 0.1V
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .io.line  bidirectional "IO"  "3"
        .ctl.line output        "CTL" "4"
    }
}

/// Invented: an input on the B side.
part Reader {
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

const BOARD: &str = r#"    .s18: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
        .neg.net &= "GND"
    }
    .s3: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .host: Host
    .x: Shift
    .r: Reader
    .s18 ~ .host.vdd
    .s18 ~ .x.vcca
    .s3 ~ .x.vccb
    .s3 ~ .r.vdd
    .host.io.line ~ .x.a.line
    .x.b.line ~ .r.in.line
"#;

/// The fault of the A side driving the host: the direction from B to A.
const B_TO_A: &str = "net `net-host.io.line`: `x.a.voh` is 0.5 x `x.vcca.voltage` but `host.io.vih` needs 0.7 x `host.vdd.voltage`";

fn solved(parts: &str, body: &str) -> Solution {
    let c = check(
        "test.nsm",
        &format!("{parts}\nboard B {{\n{BOARD}{body}}}\n"),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn faults(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The only fault is the A side driving the host.
fn judged_b_to_a(s: &Solution) -> bool {
    let f = faults(s);
    f.len() == 1 && f[0].starts_with(B_TO_A)
}

#[test]
fn a_control_strapped_high_fixes_a_to_b_and_the_way_back_is_not_judged() {
    let s = solved(PARTS, "    .x.dir ~ .s18.pos\n");
    assert_eq!(faults(&s), Vec::<String>::new());
}

#[test]
fn a_control_strapped_low_fixes_b_to_a_and_that_direction_is_judged() {
    let s = solved(PARTS, "    .x.dir ~ .s18.neg\n");
    assert!(judged_b_to_a(&s), "{:#?}", faults(&s));
}

#[test]
fn a_control_that_a_pin_drives_leaves_both_directions_judged() {
    let s = solved(PARTS, "    .host.ctl.line ~ .x.dir\n");
    assert!(judged_b_to_a(&s), "{:#?}", faults(&s));
}

#[test]
fn a_strap_at_a_level_no_direction_names_leaves_both_directions_judged() {
    // The part states only the direction while `.dir` is low; the board straps it high.
    let parts = PARTS.replacen("    direction .a to .b when .dir high\n", "", 1);
    assert_ne!(parts, PARTS);
    let s = solved(&parts, "    .x.dir ~ .s18.pos\n");
    assert!(judged_b_to_a(&s), "{:#?}", faults(&s));
}

#[test]
fn a_part_with_no_direction_is_judged_both_ways_as_before() {
    let parts = PARTS
        .replacen("    direction .a to .b when .dir high\n", "", 1)
        .replacen("    direction .b to .a when .dir low\n", "", 1);
    let s = solved(&parts, "    .x.dir ~ .s18.pos\n");
    assert!(judged_b_to_a(&s), "{:#?}", faults(&s));
}
