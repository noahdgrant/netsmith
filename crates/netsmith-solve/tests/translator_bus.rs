//! A MOSFET translator between two open-drain buses (#1045): the levels that cross its channel
//! are judged by the DC analysis, with the drivers of the receiver's own net letting go of it.
//!
//! The circuit is the usual one: an N-channel MOSFET whose gate is on the low supply, its source
//! on the low side's line and its drain on the high side's, each line pulled up to its own
//! supply, an open-drain pin on each. Every part here is invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"import std::parts::passive::Resistor
import std::logic::Logic
import std::sources::Rail

/// An open-drain bus pin: a low of 0.4 V at 3 mA and no high, and it reads a high from 70 % of
/// its supply and a low up to 30 %.
part Od {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    .io.vol &= 0.4V at 3mA
    .io.vih &= 0.7 * .vdd.voltage
    .io.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos   power         "VDD" "1"
        .vdd.neg   power         "GND" "2"
        .io.line   bidirectional "IO"  "3"
    }
}

/// An N-channel MOSFET as a switch, with a body diode of at most `DIODE`.
part Fet {
    pins {
        .gate   input   "G" "1"
        .source passive "S" "2"
        .drain  passive "D" "3"
    }
    .gate_threshold: Voltage
    .on_resistance: Resistance
    .gate_threshold &= THRESHOLD
    .on_resistance &= 0.7ohm..3.5ohm
    .body_diode_voltage: Voltage
    DIODE
    #[role(current)]
    .current: Current
    bridge .drain ~> .source
    dc nmos .gate .drain .source .gate_threshold .on_resistance
}

board B {
    .low: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .high: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "5V"
        .neg.net &= "GND"
    }
    .a: Od
    .b: Od
    .ra: Resistor { .resistance &= 10kohm +/- 1% }
    .rb: Resistor { .resistance &= PULLUP }
    .q: Fet
    .low ~ .a.vdd
    .high ~ .b.vdd
    .low.pos ~> .ra ~> .a.io.line
    .high.pos ~> .rb ~> .b.io.line
    .q.gate ~ .low.pos
    .q.source ~ .a.io.line
    .q.drain ~ .b.io.line
}
"#;

/// The translator with the MOSFET's threshold, its body diode (a statement, or nothing) and the
/// high side's pull-up.
fn translator(threshold: &str, diode: &str, pullup: &str) -> Solution {
    let src = PARTS
        .replace("THRESHOLD", threshold)
        .replace("DIODE", diode)
        .replace("PULLUP", pullup);
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn faults(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated.iter().map(|n| n.message.clone()).collect()
}

/// The notes about what crosses the MOSFET, or reaches a receiver through it.
fn crossing(s: &Solution) -> Vec<String> {
    notes(s)
        .into_iter()
        .filter(|n| n.contains("`q`"))
        .collect()
}

const DIODE: &str = ".body_diode_voltage &= 1.2V";

#[test]
fn a_translator_with_its_body_diode_is_judged_on_both_sides_and_holds() {
    // Each side's high is its own pull-up, with the MOSFET off; a low on the 3.3 V side turns
    // the MOSFET on through its gate, and a low on the 5 V side through its body diode.
    let s = translator("0.8V..1.5V", DIODE, "10kohm +/- 1%");
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(crossing(&s), Vec::<String>::new());
}

#[test]
fn without_its_body_diode_the_low_that_crosses_from_the_drain_is_named() {
    // The 5 V side pulls the drain low. With the channel on, the 3.3 V side follows it; but the
    // analysis also finds the MOSFET off with the source still at its pull-up, a state only the
    // body diode rules out. The other three levels are judged.
    let s = translator("0.8V..1.5V", "", "10kohm +/- 1%");
    assert_eq!(faults(&s), Vec::<String>::new());
    let c = crossing(&s);
    assert_eq!(c.len(), 1, "{c:#?}");
    assert!(
        c[0].starts_with(
            "not evaluated: the low level into `a.io` on net `net-a.io.line` through `q` from net `net-b.io.line` with `b.io` driving low and every driver of net `net-a.io.line` letting go of it: with `q` on, the DC analysis holds the line at "
        ) && c[0].ends_with(
            "whether `q` turns on there rests on its body diode, which the analysis models only when `q.body_diode_voltage` states its forward voltage"
        ),
        "{c:#?}"
    );
}

#[test]
fn a_threshold_too_high_for_the_gate_drive_leaves_the_low_side_off_and_is_refused() {
    // With a threshold of up to 2.5 V, a gate at 3.135 V and a body diode of up to 1.2 V, the
    // MOSFET can stay off with the 3.3 V side up to 1.2 V above the 5 V side's low: not a low.
    let s = translator("1V..2.5V", DIODE, "10kohm +/- 1%");
    let f = faults(&s);
    assert_eq!(f.len(), 1, "{f:#?}");
    assert!(
        f[0].starts_with(
            "net `net-a.io.line`: with `b.io` driving low and every driver of net `net-a.io.line` letting go of it, the DC analysis holds the line at "
        ) && f[0].contains(" through `q` from net `net-b.io.line` but `a.io.vil` allows "),
        "{f:#?}"
    );
}

#[test]
fn a_pull_up_too_strong_for_the_open_drain_pins_gives_lows_that_cross_badly_both_ways() {
    // A 200 ohm pull-up to 5 V against an open-drain pin of up to 133 ohm: a low on either side
    // is above what the other side reads as a low once it has crossed the MOSFET.
    let s = translator("0.8V..1.5V", DIODE, "200ohm +/- 1%");
    let f = faults(&s);
    assert_eq!(f.len(), 2, "{f:#?}");
    for (near, far, driver, vil) in [
        ("net-a.io.line", "net-b.io.line", "b.io", "a.io.vil"),
        ("net-b.io.line", "net-a.io.line", "a.io", "b.io.vil"),
    ] {
        assert!(
            f.iter().any(|m| m.starts_with(&format!(
                "net `{near}`: with `{driver}` driving low and every driver of net `{near}` letting go of it, the DC analysis holds the line at "
            )) && m.contains(&format!(" through `q` from net `{far}` but `{vil}` allows "))),
            "{f:#?}"
        );
    }
}
