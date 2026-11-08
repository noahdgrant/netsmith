//! A line that states a swing and no `voh` or `vol` gets its unloaded output levels from the
//! swing: the high is its top and the low its bottom (ADR 0081, amendment). A level the line
//! states always wins, and a line that states no swing is judged as before.
//!
//! The parts are invented for these tests.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::logic::Logic
import std::parts::passive::Resistor
import std::sources::Rail

/// A module on a battery whose output swings to an internal 1.8 V rail that has no pad, and
/// states no level.
part Modem {
    .vbat: Rail
    .status: Logic
    .status.reference ~ .vbat
    .status.swing &= 0V..1.8V
    pins {
        .vbat.pos     power  "VBAT"   "1"
        .vbat.neg     power  "GND"    "2"
        .status.line  output "STATUS" "3"
    }
}

/// Like `Modem`, but it states the high it reaches, 1.5 V, as a datasheet does.
part ModemWithHigh {
    .vbat: Rail
    .status: Logic
    .status.reference ~ .vbat
    .status.swing &= 0V..1.8V
    .status.voh &= 1.5V
    pins {
        .vbat.pos     power  "VBAT"   "1"
        .vbat.neg     power  "GND"    "2"
        .status.line  output "STATUS" "3"
    }
}

/// An open-drain output: it states its low and a swing, and no high.
part OpenDrain {
    .vbat: Rail
    .status: Logic
    .status.reference ~ .vbat
    .status.swing &= 0V..1.8V
    .status.vol &= 0.2V
    pins {
        .vbat.pos     power  "VBAT"   "1"
        .vbat.neg     power  "GND"    "2"
        .status.line  output "STATUS" "3"
    }
}

/// A module that states no swing and no level.
part Mute {
    .vbat: Rail
    .status: Logic
    .status.reference ~ .vbat
    pins {
        .vbat.pos     power  "VBAT"   "1"
        .vbat.neg     power  "GND"    "2"
        .status.line  output "STATUS" "3"
    }
}

/// Reads a high from 1.2 V and a low up to 0.4 V.
part Sensor {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 1.2V
    .in.vil &= 0.4V
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}

/// Reads a high only from 1.9 V.
part Demanding {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 1.9V
    .in.vil &= 0.4V
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}

/// Reads a low only below -0.1 V.
part Negative {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 1.2V
    .in.vil &= -0.1V
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}
"#;

fn solved(driver: &str, receiver: &str) -> Solution {
    let c = check(
        "test.nsm",
        &format!(
            "{PARTS}\nboard B {{\n    .battery: Rail {{\n        .voltage &= 3.8V +/- 5%\n        .pos.net &= \"VBAT\"\n        .neg.net &= \"GND\"\n    }}\n    .io: Rail {{\n        .voltage &= 1.8V +/- 5%\n        .pos.net &= \"1V8\"\n        .neg.net &= \"GND\"\n    }}\n    .m: {driver}\n    .s: {receiver}\n    .battery ~ .m.vbat\n    .io ~ .s.vdd\n    .m.status.line ~ .s.in.line\n}}\n"
        ),
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
fn a_swing_with_no_levels_is_checked_against_a_receiver_whose_thresholds_fit() {
    let s = solved("Modem", "Sensor");
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn a_high_below_what_the_receiver_needs_is_a_fault_that_names_the_swing() {
    let s = solved("Modem", "Demanding");
    assert_eq!(
        faults(&s),
        [
            "net `net-m.status.line`: `m.status.voh` (not stated: the top of its swing, `m.status.swing`) is 1.8V but `s.in.vih` needs 1.9V: a high of 1.8V is not always read as a high, which takes 1.9V"
        ]
    );
}

#[test]
fn a_low_above_what_the_receiver_allows_is_a_fault_that_names_the_swing() {
    let s = solved("Modem", "Negative");
    assert_eq!(
        faults(&s),
        [
            "net `net-m.status.line`: `m.status.vol` (not stated: the bottom of its swing, `m.status.swing`) is 0V but `s.in.vil` allows -100mV: a low of 0V is not always read as a low, which takes -100mV"
        ]
    );
}

#[test]
fn a_level_the_line_states_wins_over_the_swing() {
    // The swing's top is 1.8 V, which `Demanding` would need; the stated 1.5 V is what counts,
    // and it fails against 1.9 V under its own name. The low still comes from the swing.
    let s = solved("ModemWithHigh", "Demanding");
    assert_eq!(
        faults(&s),
        [
            "net `net-m.status.line`: `m.status.voh` is 1.5V but `s.in.vih` needs 1.9V: a high of 1.5V is not always read as a high, which takes 1.9V"
        ]
    );
    // A stated high that fits leaves a clean board, and the low is read from the swing.
    let s = solved("ModemWithHigh", "Sensor");
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn a_line_that_states_no_swing_is_judged_as_before() {
    let s = solved("Mute", "Sensor");
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(
        notes(&s),
        [
            "not evaluated: the high level from `m.status` to `s.in` on net `net-m.status.line`: `m.status.voh` has no value",
            "not evaluated: the low level from `m.status` to `s.in` on net `net-m.status.line`: `m.status.vol` has no value",
        ]
    );
}

#[test]
fn an_open_drain_line_still_takes_its_high_from_its_pull_up() {
    // The line states a low and a swing and no high: ADR 0075 applies, so the high is what the
    // pull-up holds the line at (1.71 V to 1.89 V), not the top of the swing.
    let c = check(
        "test.nsm",
        &format!(
            "{PARTS}\nboard B {{\n    .battery: Rail {{\n        .voltage &= 3.8V +/- 5%\n        .pos.net &= \"VBAT\"\n        .neg.net &= \"GND\"\n    }}\n    .io: Rail {{\n        .voltage &= 1.8V +/- 5%\n        .pos.net &= \"1V8\"\n        .neg.net &= \"GND\"\n    }}\n    .m: OpenDrain\n    .s: Demanding\n    .r: Resistor {{ .resistance &= 4.7kohm +/- 5% }}\n    .battery ~ .m.vbat\n    .io ~ .s.vdd\n    .io.pos ~> .r ~> .m.status.line\n    .m.status.line ~ .s.in.line\n}}\n"
        ),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    let f = faults(&s);
    assert!(
        f.iter()
            .any(|m| m.contains("leaves the high to the pull-up")
                && m.contains("holds the line at 1.71V..1.89V")),
        "{f:#?}"
    );
    assert!(
        f.iter().all(|m| !m.contains("the top of its swing")),
        "{f:#?}"
    );
}
