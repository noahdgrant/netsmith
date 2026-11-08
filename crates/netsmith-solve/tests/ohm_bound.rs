//! The Ohm bound, pass-only (ADR 0026, amendment; ADR 0036, amendment 4).
//!
//! A resistor whose current nothing states waits: its `drop == current * resistance` and
//! `drop * current <= max_power` are not evaluated. When the voltage at each end lies in a
//! known range, `|I| <= max |Va - Vb| / R` and the power `(Va - Vb)^2 / R` are bounds, and a
//! check that the bound proves is made. One it does not prove stays not evaluated: the bound
//! does not know that a pin draws nothing, so it must never turn into a failure. Every part
//! and value here is invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::logic::Logic
import std::parts::connector::Header1x2
import std::parts::passive::Capacitor
import std::parts::passive::Resistor
import std::sources::Rail

/// Drives a line within 0.1 V of its supply and to 0.1 V at the bottom.
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

/// Reads a line. Its input draws nothing the model states.
part Receiver {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .in.line   input "IN"  "3"
    }
}

/// A pin that drives a line and says nothing of how far: no `Logic`, and no supply of its own.
part Opaque {
    pins {
        .out output "OUT" "1"
    }
}

/// Like `Opaque`, but the part has a supply of 1.8 V and the pin is on it.
part Gpio {
    .vdd: Rail
    pins {
        .vdd.pos power  "VDD" "1"
        .vdd.neg power  "GND" "2"
        .out     output "OUT" "3"
    }
}
"#;

const S3: &str = "    .s3: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }\n";

fn solved(body: &str) -> Solution {
    let c = check(
        "test.nsm",
        &format!("{PARTS}\nboard B {{\n{body}}}\n"),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

/// What waits, as the notes of a build print it.
fn waiting(s: &Solution) -> Vec<String> {
    s.deferred
        .iter()
        .map(|&i| {
            let r = &s.model.relations()[i];
            format!(
                "{} {} {}",
                s.model.show_expr(&r.lhs),
                r.op.symbol(),
                s.model.show_expr(&r.rhs)
            )
        })
        .collect()
}

fn faults(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn bound_of(s: &Solution, part: &str) -> Option<f64> {
    s.current_bounds
        .iter()
        .find(|b| s.model.path_of(b.part) == part)
        .map(|b| b.current)
}

/// The driver `a` on the 3.3 V rail, a `r` between its line and the ground, and a receiver.
fn pull_down(resistance: &str) -> String {
    format!(
        "{S3}    .a: Driver\n    .b: Receiver\n    .r: Resistor {{ .resistance &= {resistance} }}\n    .s3 ~ .a.vdd\n    .s3 ~ .b.vdd\n    .a.out.line ~ .b.in.line\n    .r.a ~ .a.out.line\n    .r.b ~ .s3.neg\n"
    )
}

#[test]
fn a_pull_down_on_a_logic_line_is_evaluated_and_nothing_is_said() {
    let s = solved(&pull_down("10kohm +/- 1%"));
    assert_eq!(faults(&s), Vec::<String>::new());
    // Both of the resistor's relations were waiting on its current, and the bound proves both.
    assert_eq!(waiting(&s), Vec::<String>::new());
    // The bound is `3.465 V / 9.9 kohm`: the top of the rail's range over the least resistance.
    let bound = bound_of(&s, "r").expect("a bound");
    assert!((bound - 3.465 / 9900.0).abs() < 1e-9, "{bound}");
}

#[test]
fn the_bound_is_not_the_current_of_the_part() {
    let s = solved(&pull_down("10kohm +/- 1%"));
    let current = s.model.lookup("r.current").expect("a current");
    let netsmith_check::Member::Field(f) = current else {
        panic!("not a field")
    };
    assert_eq!(s.model.field(f).domain, netsmith_check::Domain::Unset);
}

#[test]
fn a_series_resistor_whose_bound_is_over_its_rating_stays_not_evaluated() {
    // 100 ohm between a driver and a receiver: both ends are anywhere from 0 V to 3.465 V, so
    // the bound is 35 mA and 120 mW, over what a small resistor is rated for, though the real
    // current is the input's, almost nothing. The power check is left not evaluated, and
    // nothing says it failed.
    let s = solved(&format!(
        "{S3}    .a: Driver\n    .b: Receiver\n    .r: Resistor {{ .resistance &= 100ohm +/- 1% }}\n    .s3 ~ .a.vdd\n    .s3 ~ .b.vdd\n    .a.out.line ~ .r.a\n    .r.b ~ .b.in.line\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(waiting(&s), ["(r.drop * r.current) <= r.max_power"]);
    assert!(bound_of(&s, "r").is_some());
}

#[test]
fn a_divider_is_bounded_by_the_range_of_the_line_over_each_resistor() {
    // The middle of the divider has no voltage of its own; the line's range is the bound for
    // both resistors, looser than the real current and enough for a resistor of 100 mW.
    let s = solved(&format!(
        "{S3}    .a: Driver\n    .b: Receiver\n    .top: Resistor {{ .resistance &= 20kohm +/- 1% }}\n    .bottom: Resistor {{ .resistance &= 15kohm +/- 1% }}\n    .s3 ~ .a.vdd\n    .s3 ~ .b.vdd\n    .a.out.line ~ .top.a\n    .top.b ~ .bottom.a\n    .top.b ~ .b.in.line\n    .bottom.b ~ .s3.neg\n"
    ));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(waiting(&s), Vec::<String>::new());
    assert!(bound_of(&s, "top").is_some() && bound_of(&s, "bottom").is_some());
}

#[test]
fn a_resistor_across_a_wide_rail_is_failed_by_the_dc_analysis_and_not_by_the_bound() {
    // 3.465 V over 1 ohm is 3.5 A and 12 W: far over its rating. The bound is pass-only and
    // would leave it waiting; the DC analysis solves the circuit (a rail across one resistor),
    // so the check is made and fails, with the states that give the current (ADR 0076).
    let s = solved(&format!(
        "{S3}    .r: Resistor {{ .resistance &= 1ohm +/- 1% }}\n    .r.a ~ .s3.pos\n    .r.b ~ .s3.neg\n"
    ));
    let faults = faults(&s);
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert!(
        faults[0].contains("(r.drop * r.current) must always be at most r.max_power")
            && faults[0].contains("the DC analysis puts `r.current` at")
            && faults[0].contains("its high end with s3 at 3.465V, r at 990mΩ"),
        "{}",
        faults[0]
    );
    assert!(waiting(&s).is_empty(), "{:?}", waiting(&s));
}

#[test]
fn a_net_whose_voltage_nothing_bounds_gives_no_bound() {
    // A pin that drives the line and says nothing of how far, on a part with no supply.
    let s = solved(&format!(
        "{S3}    .o: Opaque\n    .r: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .r.a ~ .o.out\n    .r.b ~ .s3.neg\n"
    ));
    assert_eq!(bound_of(&s, "r"), None);
    assert_eq!(
        waiting(&s),
        [
            "r.drop == (r.current * r.resistance)",
            "(r.drop * r.current) <= r.max_power"
        ]
    );
}

#[test]
fn a_connector_on_the_net_gives_no_bound() {
    // What is plugged into a header can put any voltage on the line.
    let s = solved(&format!(
        "{S3}    .a: Driver\n    .j: Header1x2\n    .r: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .s3 ~ .a.vdd\n    .a.out.line ~ .r.a\n    .a.out.line ~ .j.p1\n    .r.b ~ .s3.neg\n"
    ));
    assert_eq!(bound_of(&s, "r"), None);
    assert_eq!(waiting(&s).len(), 2, "{:?}", waiting(&s));
}

#[test]
fn a_pin_that_drives_is_held_to_the_supplies_of_its_own_part() {
    let s = solved(&format!(
        "{S3}    .g: Gpio\n    .r: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .s3 ~ .g.vdd\n    .r.a ~ .g.out\n    .r.b ~ .s3.neg\n"
    ));
    assert_eq!(waiting(&s), Vec::<String>::new());
    let bound = bound_of(&s, "r").expect("a bound");
    assert!((bound - 3.465 / 9900.0).abs() < 1e-9, "{bound}");
}

#[test]
fn a_capacitor_on_the_net_passes_nothing_and_changes_no_bound() {
    let s = solved(&format!(
        "{S3}    .a: Driver\n    .c: Capacitor {{ .capacitance &= 100nF }}\n    .r: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .s3 ~ .a.vdd\n    .a.out.line ~ .r.a\n    .a.out.line ~ .c.a\n    .r.b ~ .s3.neg\n    .c.b ~ .s3.neg\n"
    ));
    assert!(bound_of(&s, "r").is_some());
}

#[test]
fn a_stated_current_is_left_alone() {
    let s = solved(&format!(
        "{S3}    .a: Driver\n    .r: Resistor {{ .resistance &= 10kohm +/- 1%; .current &= 100uA }}\n    .s3 ~ .a.vdd\n    .a.out.line ~ .r.a\n    .r.b ~ .s3.neg\n"
    ));
    assert_eq!(bound_of(&s, "r"), None);
}
