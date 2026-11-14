//! The DC analysis of logic pins and MOSFETs as switches (ADR 0076, amendment 4).
//!
//! A pin that states `voh` and `vol` with the currents they are measured at drives its line high,
//! low, or (where it can let go) high impedance, through the output resistance those give. A
//! MOSFET is on or off, by what its gate says. Every part and value here is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"import std::parts::passive::Resistor
import std::logic::Logic
import std::sources::Rail

/// A controller with one output that states its levels as a datasheet does: the high is the
/// supply less 0.4 V at 2 mA (200 ohm at the most), the low 0.4 V at 2 mA.
part Mcu {
    .vdd: Rail
    .gpio: Logic
    .gpio.reference ~ .vdd
    .gpio.voh &= .vdd.voltage - 0.4V at 2mA
    .gpio.vol &= 0.4V at 2mA
    pins {
        .vdd.pos    power  "VDD" "1"
        .vdd.neg    power  "GND" "2"
        .gpio.line  output "IO"  "3"
    }
}

/// The same pin on a part that can let go of its line.
part Hub {
    .vdd: Rail
    .gpio: Logic
    .gpio.reference ~ .vdd
    .gpio.voh &= .vdd.voltage - 0.4V at 2mA
    .gpio.vol &= 0.4V at 2mA
    pins {
        .vdd.pos    power     "VDD" "1"
        .vdd.neg    power     "GND" "2"
        .gpio.line  tri_state "IO"  "3"
    }
}

/// An output that states its high and no test current, and its low the same.
part Vague {
    .vdd: Rail
    .gpio: Logic
    .gpio.reference ~ .vdd
    .gpio.voh &= .vdd.voltage - 0.4V
    .gpio.vol &= 0.4V
    pins {
        .vdd.pos    power  "VDD" "1"
        .vdd.neg    power  "GND" "2"
        .gpio.line  output "IO"  "3"
    }
}

/// An output that states no level at all.
part Mute {
    .vdd: Rail
    .gpio: Logic
    .gpio.reference ~ .vdd
    pins {
        .vdd.pos    power  "VDD" "1"
        .vdd.neg    power  "GND" "2"
        .gpio.line  output "IO"  "3"
    }
}

/// An N-channel switch: the channel between drain and source is open below the threshold.
part Switch1 {
    pins {
        .gate   input   "G" "1"
        .source passive "S" "2"
        .drain  passive "D" "3"
    }
    .gate_threshold: Voltage
    .on_resistance: Resistance
    #[role(current)]
    .current: Current
    bridge .drain ~> .source
    dc nmos .gate .drain .source .gate_threshold .on_resistance
}

board B {
    .main: Rail {
        .voltage &= 12V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .logic: Rail {
        .voltage &= 3.3V +/- 3%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
BODY
}
"#;

fn board(body: &str) -> String {
    PARTS.replace("BODY", body)
}

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> Option<(f64, f64)> {
    match s
        .model
        .domain_at(path)
        .unwrap_or_else(|| panic!("no field {path}"))
    {
        Domain::Quantity(q) => Some((q.lo(), q.hi())),
        _ => None,
    }
}

fn near(got: f64, want: f64, rel: f64) {
    assert!(
        (got - want).abs() <= rel * want.abs().max(1e-9) + 2e-12,
        "got {got}, want {want}"
    );
}

const PULLDOWN: &str = "    .mcu: Mcu\n    .pd: Resistor { .resistance &= 10kohm +/- 1% }\n    .logic ~ .mcu.vdd\n    .mcu.gpio.line ~ .pd.a\n    .pd.b ~ .logic.neg\n";

/// A pin with a 10 kohm +/- 1 % pull-down. The pull-down carries the supply over the
/// resistance when the pin is high: `3.399 V / (9.9 kohm + 1 mohm)` at the most, and nothing when
/// the pin is low. Nothing states the current, and the equations cannot say it.
#[test]
fn a_pull_down_on_a_pin_carries_what_the_pin_drives_through_it() {
    let s = solved(&board(PULLDOWN));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.dc.pins, 1, "{:?}", s.dc);
    let (lo, hi) = range(&s, "pd.current").expect("pd.current has a value");
    near(hi, 3.399 / (9900.0 + 1e-3), 1e-6);
    assert!(lo.abs() < 1e-9, "{lo}");
    let (_, drop_hi) = range(&s, "pd.drop").expect("pd.drop has a value");
    near(drop_hi, 3.399 * 9900.0 / (9900.0 + 1e-3), 1e-6);
}

/// The output resistance is the datasheet's: `(VDD - VOH) / IOH` is 0.4 V / 2 mA = 200 ohm,
/// and not the 1.1 V the supply's range makes of the lowest high. With a pull-down of 100 ohm
/// the difference is the current.
#[test]
fn the_output_resistance_is_the_stated_drop_over_its_test_current() {
    let src = board(
        "    .mcu: Mcu\n    .pd: Resistor { .resistance &= 100ohm }\n    .logic ~ .mcu.vdd\n    .mcu.gpio.line ~ .pd.a\n    .pd.b ~ .logic.neg\n",
    );
    let s = solved(&src);
    let (_, hi) = range(&s, "pd.current").expect("pd.current has a value");
    // The most current is at the highest supply and the least output resistance, 1 mohm.
    near(hi, 3.399 / (100.0 + 1e-3), 1e-6);
    // The least high: the supply's least with the pin's greatest resistance gives the
    // smallest current when high, which the range does not show (it starts at zero); the largest
    // output resistance is seen in the states of the end that is not the greatest.
    let (lo, _) = range(&s, "pd.current").unwrap();
    assert!(lo.abs() < 1e-9);
}

/// A pin that states its levels without a current has no output resistance to derive: the net
/// stays open and the pull-down is left to the bound, as before.
#[test]
fn a_pin_with_no_test_current_is_a_boundary() {
    let src = board(&PULLDOWN.replace("Mcu", "Vague"));
    let s = solved(&src);
    assert_eq!(s.dc.pins, 0, "{:?}", s.dc);
}

#[test]
fn a_pin_with_no_levels_is_a_boundary() {
    let src = board(&PULLDOWN.replace("Mcu", "Mute"));
    let s = solved(&src);
    assert_eq!(s.dc.pins, 0, "{:?}", s.dc);
}

/// Two pins that can drive one net may fight, and which pairs are driven together is not
/// stated: neither is modelled.
#[test]
fn two_drivers_on_one_net_are_not_modelled() {
    let src = board(&format!(
        "{}    .other: Hub\n    .logic ~ .other.vdd\n    .other.gpio.line ~ .pd.a\n",
        PULLDOWN.replace("Mcu", "Hub")
    ));
    let s = solved(&src);
    assert_eq!(s.dc.pins, 0, "{:?}", s.dc);
}

/// A stated current is a budget: the pull-down is allowed 100 uA, and the circuit needs up to
/// 343 uA with the pin high. The error names the pin's state.
#[test]
fn a_stated_budget_that_the_pin_exceeds_is_an_error_naming_its_state() {
    let src = board(&PULLDOWN.replace(
        "{ .resistance &= 10kohm +/- 1% }",
        "{ .resistance &= 10kohm +/- 1%; .current &= 0A..100uA }",
    ));
    let s = solved(&src);
    let all = messages(&s);
    assert!(
        all.iter().any(|m| m.contains("`pd.current` is stated")
            && m.contains("DC analysis")
            && m.contains("mcu.gpio.line high through")),
        "{all:?}"
    );
}

const SWITCHED: &str = "    .mcu: Mcu\n    .q: Switch1 { .gate_threshold &= 1V..2.5V; .on_resistance &= 50mohm..100mohm }\n    .load: Resistor { .resistance &= 10kohm +/- 1% }\n    .logic ~ .mcu.vdd\n    .mcu.gpio.line ~ .q.gate\n    .main.pos ~ .load.a\n    .load.b ~ .q.drain\n    .q.source ~ .main.neg\n";

/// A switch whose gate a pin drives, off a 12 V supply into a 10 kohm load. With the pin high
/// the gate is above the greatest threshold: on. With it low the gate is below the least: off.
/// The load's current is from zero to `12 V / (9.9 kohm + 50 mohm)`, and the channel carries the
/// same.
#[test]
fn a_switch_driven_by_a_pin_carries_the_load_when_the_pin_is_high() {
    let s = solved(&board(SWITCHED));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.dc.switches, 1, "{:?}", s.dc);
    assert_eq!(s.dc.pins, 1, "{:?}", s.dc);
    let want = 12.0 / (9900.0 + 0.05);
    let (lo, hi) = range(&s, "load.current").expect("load.current has a value");
    near(hi, want, 1e-6);
    assert!(lo.abs() < 1e-9, "{lo}");
    let (qlo, qhi) = range(&s, "q.current").expect("q.current has a value");
    near(qhi, want, 1e-6);
    assert!(qlo.abs() < 1e-9, "{qlo}");
}

/// The same switch with its gate joined to the ground: the gate is below the least threshold, so
/// only off is the circuit's state, and nothing flows.
#[test]
fn a_gate_at_ground_keeps_the_switch_off() {
    let src = board(&SWITCHED.replace(".mcu.gpio.line ~ .q.gate", ".logic.neg ~ .q.gate"));
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.dc.switches, 1, "{:?}", s.dc);
    let (lo, hi) = range(&s, "load.current").expect("load.current has a value");
    assert!(lo.abs() < 1e-9 && hi.abs() < 1e-9, "{lo} {hi}");
}

/// A gate on the supply: above the greatest threshold, so on and never off.
#[test]
fn a_gate_on_the_supply_keeps_the_switch_on() {
    let src = board(&SWITCHED.replace(".mcu.gpio.line ~ .q.gate", ".logic.pos ~ .q.gate"));
    let s = solved(&src);
    let (lo, hi) = range(&s, "load.current").expect("load.current has a value");
    near(lo, 12.0 / (10100.0 + 0.1), 1e-6);
    near(hi, 12.0 / (9900.0 + 0.05), 1e-6);
}

/// A tri-state pin can let go of the gate, and a gate that nothing else holds is neither on nor
/// off: both stay possible, and the pin has one more state than an output has.
#[test]
fn a_gate_that_a_tri_state_pin_lets_go_of_may_be_either_way() {
    let output = solved(&board(SWITCHED));
    let tri = solved(&board(&SWITCHED.replace("Mcu", "Hub")));
    assert_eq!(tri.dc.switches, 1, "{:?}", tri.dc);
    // The supply's two ends, the pin's states (an output is high or low, each at two output
    // resistances; a tri-state pin lets go too), the switch's three and the load's two.
    assert_eq!(output.dc.combinations, 2 * 4 * 3 * 2, "{:?}", output.dc);
    assert_eq!(tri.dc.combinations, 2 * 5 * 3 * 2, "{:?}", tri.dc);
    let want = 12.0 / (9900.0 + 0.05);
    let (lo, hi) = range(&tri, "load.current").expect("load.current has a value");
    near(hi, want, 1e-6);
    assert!(lo.abs() < 1e-9, "{lo}");
}

/// A switch whose gate net has a pin nothing models is a boundary. A stated current is what it
/// carries, as before this stage.
#[test]
fn a_switch_with_an_unmodelled_gate_is_a_boundary_with_its_stated_current() {
    let src = board(&SWITCHED.replace("Mcu", "Mute").replace(
        "{ .gate_threshold",
        "{ .current &= 1mA..2mA; .gate_threshold",
    ));
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.dc.switches, 0, "{:?}", s.dc);
    let (lo, hi) = range(&s, "load.current").expect("load.current has a value");
    near(lo, 1e-3, 1e-6);
    near(hi, 2e-3, 1e-6);
}
