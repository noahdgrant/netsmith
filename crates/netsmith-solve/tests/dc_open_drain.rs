//! An open-drain line in the DC analysis, and the levels check that reads its node voltage
//! (ADR 0076, amendment 8). A pin that states a low level at a test current and no high is open
//! drain by ADR 0075's inference: low through the resistance the level gives, or high
//! impedance. Every part here is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"import std::parts::passive::Resistor
import std::logic::Logic
import std::sources::Rail

/// An open-drain driver: its datasheet states a low of 0.4 V at 3 mA and no high.
part Od {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    .io.vol &= 0.4V at 3mA
    pins {
        .vdd.pos   power         "VDD" "1"
        .vdd.neg   power         "GND" "2"
        .io.line   bidirectional "IO"  "3"
    }
}

/// The same pin with no test current for its low: the analysis cannot give it a resistance.
part OdBare {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    .io.vol &= 0.4V
    pins {
        .vdd.pos   power         "VDD" "1"
        .vdd.neg   power         "GND" "2"
        .io.line   bidirectional "IO"  "3"
    }
}

/// A push-pull driver on its own supply, whose levels carry their test currents.
part Push {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    .io.voh &= .vdd.voltage - 0.4V at 2mA
    .io.vol &= 0.4V at 2mA
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2"
        .io.line   output "IO"  "3"
    }
}

/// A receiver that reads a high from 70 % of its supply.
part Pad {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    .io.vih &= 0.7 * .vdd.voltage
    .io.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .io.line   input "IN"  "3"
    }
}

/// An open-drain driver with a datasheet leakage of 1 uA.
part OdLeaky {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    .io.vol &= 0.4V at 3mA
    .io.ii &= 1uA
    pins {
        .vdd.pos   power         "VDD" "1"
        .vdd.neg   power         "GND" "2"
        .io.line   bidirectional "IO"  "3"
    }
}

/// A receiver with a leakage of 1 uA that reads a high from 93 % of its supply.
part PadLeaky {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    .io.vih &= 0.93 * .vdd.voltage
    .io.vil &= 0.3 * .vdd.voltage
    .io.ii &= 1uA
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .io.line   input "IN"  "3"
    }
}

/// The same receiver with no leakage stated.
part PadHigh {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    .io.vih &= 0.93 * .vdd.voltage
    .io.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .io.line   input "IN"  "3"
    }
}

/// An output that states its output resistances, and its levels with no test current.
part Pp {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    .io.voh &= .vdd.voltage - 0.4V
    .io.vol &= 0.4V
    .io.roh &= 50ohm..60ohm
    .io.rol &= 10ohm..20ohm
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2"
        .io.line   output "IO"  "3"
    }
}

board B {
    .logic: Rail {
        .voltage &= 3.3V +/- 3%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
BODY
}
"#;

fn solved(body: &str) -> Solution {
    let c = check("test.nsm", &PARTS.replace("BODY", body), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn faults(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated.iter().map(|n| n.message.clone()).collect()
}

fn has_value(s: &Solution, path: &str) -> bool {
    matches!(s.model.domain_at(path), Some(Domain::Quantity(_)))
}

const SUPPLIES: &str = "    .od: Od\n    .pad: Pad\n    .logic ~ .od.vdd\n    .logic ~ .pad.vdd\n    .od.io.line ~ .pad.io.line\n";

/// The line is pulled up to the rail through a resistor that has a second resistor to the
/// ground: a divider. The levels check used to find no pull-up in it and say that the high
/// has no value. The analysis solves it: with the pin let go, the line is `3.3 V * R2 / (R1 +
/// R2)` over the corners, 2.164 V at the least, under what the pad needs (70 % of 3.3 V).
#[test]
fn a_pulled_up_line_with_a_divider_is_judged_on_its_node_voltage() {
    let s = solved(&format!(
        "{SUPPLIES}    .up: Resistor {{ .resistance &= 4.7kohm +/- 1% }}\n    .down: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .up.a ~ .logic.pos\n    .up.b ~ .od.io.line\n    .down.a ~ .od.io.line\n    .down.b ~ .logic.neg\n"
    ));
    let f = faults(&s);
    let lo = 3.3 * 0.97 * 9900.0 / (4747.0 + 9900.0);
    let hi = 3.3 * 1.03 * 10100.0 / (4653.0 + 10100.0);
    assert_eq!(f.len(), 1, "{f:#?}");
    assert!(
        f[0].starts_with("net `")
            && f[0].contains("the DC analysis holds the line at 2.16")
            && f[0].contains("with every driver of it high impedance")
            && f[0].contains("`pad.io.vih` needs"),
        "{}\nwanted a high from {lo} V to {hi} V",
        f[0]
    );
    assert!(f[0].contains(&format!("a high of {:.3}V", lo)), "{}", f[0]);
    // No part is written from a pin that was only inferred open drain.
    assert!(!has_value(&s, "up.current"));
    assert!(!has_value(&s, "down.current"));
}

/// A plain pull-up to a rail passes, as it did before: the high is the rail's range, which the
/// node voltage with nothing drawn from the resistor is.
#[test]
fn a_plain_pull_up_holds_the_line_at_the_rail_and_passes() {
    let s = solved(&format!(
        "{SUPPLIES}    .up: Resistor {{ .resistance &= 4.7kohm +/- 5% }}\n    .up.a ~ .logic.pos\n    .up.b ~ .od.io.line\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
    assert!(!has_value(&s, "up.current"));
}

/// Several open-drain pins on a line are a wired AND, not a fight: they are modelled together,
/// and the divider is judged as it is with one.
#[test]
fn several_open_drain_pins_on_one_line_are_modelled_together() {
    let s = solved(&format!(
        "{SUPPLIES}    .od2: Od\n    .logic ~ .od2.vdd\n    .od2.io.line ~ .pad.io.line\n    .up: Resistor {{ .resistance &= 4.7kohm +/- 1% }}\n    .down: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .up.a ~ .logic.pos\n    .up.b ~ .od.io.line\n    .down.a ~ .od.io.line\n    .down.b ~ .logic.neg\n"
    ));
    let f = faults(&s);
    assert!(
        f.iter()
            .any(|m| m.contains("the DC analysis holds the line at 2.16")),
        "{f:#?}"
    );
}

/// A pin that can drive the line high next to an open-drain one may fight it: the line is not
/// modelled, and the levels check reads the pull-up as before (the divider is a note).
#[test]
fn a_push_pull_driver_on_the_line_leaves_it_to_the_old_reading() {
    let s = solved(&format!(
        "{SUPPLIES}    .push: Push\n    .logic ~ .push.vdd\n    .push.io.line ~ .pad.io.line\n    .up: Resistor {{ .resistance &= 4.7kohm +/- 1% }}\n    .down: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .up.a ~ .logic.pos\n    .up.b ~ .od.io.line\n    .down.a ~ .od.io.line\n    .down.b ~ .logic.neg\n"
    ));
    assert!(
        faults(&s)
            .iter()
            .all(|m| !m.contains("the DC analysis holds")),
        "{:#?}",
        faults(&s)
    );
}

/// A low with no test current has no resistance to derive: the pin is a boundary, the line is
/// not solved, and the divider is the note it was.
#[test]
fn an_open_drain_pin_with_no_test_current_is_a_boundary() {
    let s = solved(&format!(
        "{}    .up: Resistor {{ .resistance &= 4.7kohm +/- 1% }}\n    .down: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .up.a ~ .logic.pos\n    .up.b ~ .od.io.line\n    .down.a ~ .od.io.line\n    .down.b ~ .logic.neg\n",
        SUPPLIES.replace("Od\n", "OdBare\n")
    ));
    assert!(
        faults(&s)
            .iter()
            .all(|m| !m.contains("the DC analysis holds")),
        "{:#?}",
        faults(&s)
    );
    assert!(
        notes(&s)
            .iter()
            .any(|n| n.contains("`od.io.voh` has no value")),
        "{:#?}",
        notes(&s)
    );
}

fn range(s: &Solution, path: &str) -> (f64, f64) {
    match s
        .model
        .domain_at(path)
        .unwrap_or_else(|| panic!("no field {path}"))
    {
        Domain::Quantity(q) => (q.lo(), q.hi()),
        other => panic!("{path} has no value: {other:?}"),
    }
}

const PULL_100K: &str = "    .up: Resistor { .resistance &= 100kohm +/- 1% }\n    .up.a ~ .logic.pos\n    .up.b ~ .od.io.line\n";

/// A pull-up of 100 kohm on a line with two pins that leak 1 uA at the supply's greatest, 3.399
/// V: each is 3.399 Mohm to the ground, 1.7 Mohm together. The high is `3.201 V * 1.6995 Mohm /
/// (101 kohm + 1.6995 Mohm)` = 3.021 V at the least, under the 93 % of the supply that the
/// receiver needs (3.161 V at the most); without the leakage the high is the rail's, 3.201 V, and
/// passes.
#[test]
fn the_leakage_of_the_pins_on_a_line_lowers_its_high() {
    let body = |od: &str, pad: &str| {
        format!(
            "    .od: {od}\n    .pad: {pad}\n    .logic ~ .od.vdd\n    .logic ~ .pad.vdd\n    .od.io.line ~ .pad.io.line\n{PULL_100K}"
        )
    };
    let s = solved(&body("Od", "PadHigh"));
    assert_eq!(faults(&s), Vec::<String>::new());
    let s = solved(&body("OdLeaky", "PadLeaky"));
    let f = faults(&s);
    assert_eq!(f.len(), 1, "{f:#?}");
    let lo = 3.3 * 0.97 * 1.6995e6 / (101_000.0 + 1.6995e6);
    assert!(
        f[0].contains("the DC analysis holds the line at 3.02")
            && f[0].contains(&format!("a high of {:.3}V", lo)),
        "{}\nwanted {lo}",
        f[0]
    );
}

/// An input that states its leakage draws it: a pull-down of 100 kohm under a pad that leaks to
/// the supply carries the supply over the pull-down and the leak, and not nothing.
#[test]
fn an_input_that_states_its_leakage_draws_it() {
    let s = solved(
        "    .pad: PadLeaky\n    .pd: Resistor { .resistance &= 100kohm +/- 1% }\n    .logic ~ .pad.vdd\n    .pad.io.line ~ .pd.a\n    .pd.b ~ .logic.neg\n",
    );
    assert_eq!(faults(&s), Vec::<String>::new());
    let (_, hi) = range(&s, "pd.current");
    let want = 3.399 / (99_000.0 + 3.399e6);
    assert!((hi - want).abs() <= 1e-6 * want + 2e-12, "{hi} vs {want}");
}

/// A part that states `.roh` and `.rol` has the resistances it gives, with no test current on
/// its levels: the pull-down carries the supply over the least resistances.
#[test]
fn stated_output_resistances_stand_in_for_the_test_currents() {
    let s = solved(
        "    .pp: Pp\n    .pd: Resistor { .resistance &= 10kohm +/- 1% }\n    .logic ~ .pp.vdd\n    .pp.io.line ~ .pd.a\n    .pd.b ~ .logic.neg\n",
    );
    assert_eq!(faults(&s), Vec::<String>::new());
    let (_, hi) = range(&s, "pd.current");
    let want = 3.399 / (9_900.0 + 50.0);
    assert!((hi - want).abs() <= 1e-6 * want + 2e-12, "{hi} vs {want}");
}
