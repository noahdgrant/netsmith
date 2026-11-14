//! Small boards of the kind a real 12 V automotive board has, each checked in the DC analysis
//! against a closed form worked out in the test, apart from the solver (ADR 0076).
//!
//! They carry over the hand-checked islands of the prototype that the analysis was first written
//! as: a gate's pull-down on a processor pin, a pull-down on a 1.8 V output, a resistor divider from a 3.3 V pin to a 1.8 V input, a supercapacitor's balancing
//! divider with the leakage of its cells, a battery divider and series resistors into inputs.
//! Every part here is invented and every number is a datasheet's kind of number: a
//! test states what it assumes in the comment above the assertion, and the expected value is
//! the one the comment works out.
//!
//! The analysis solves a pin's output resistance down to a milliohm and not to zero, so a range
//! that the hand calculation ends at 0 ohm is compared to a part in a million.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"import std::parts::passive::Resistor
import std::logic::Logic
import std::sources::Rail

/// A resistance to pick no row for: the shape of a fuse, a shunt or a bead with a stated value.
part Link {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .rated_current: Current
    #[role(current)]
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    .current <= .rated_current
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

/// A current that a part draws between two nets (a cell's leakage across its terminals): a
/// boundary with a stated current, which the analysis takes as a current source.
part Leak {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    #[role(current)]
    .current: Current
    bridge .a ~> .b
}

/// A pin that can drive its line high, low or let go of it. A board states the output
/// resistances it assumes (`.gpio.roh`, `.gpio.rol`).
part Hub {
    .vdd: Rail
    .gpio: Logic
    .gpio.reference ~ .vdd
    .gpio.voh &= .vdd.voltage - 0.4V
    .gpio.vol &= 0.4V
    pins {
        .vdd.pos    power     "VDD" "1"
        .vdd.neg    power     "GND" "2"
        .gpio.line  tri_state "IO"  "3"
    }
}

/// The same pin on a part that drives always, high or low.
part Out {
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

/// An input that never drives: it leaks 1 uA at most, as the datasheet's `Ii`.
part Pad {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    .io.ii &= 1uA
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .io.line   input "IN"  "3"
    }
}

/// An input that states nothing about what it draws, as a buffer's does at the precision of the
/// analysis: it draws nothing.
part Quiet {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .io.line   input "IN"  "3"
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
BODY
}
"#;

/// The 3.3 V rail of the processor, 2 % under and 1 % over, and the 1.8 V rail of the modem,
/// 5 % either way.
const RAILS: &str = "    .logic: Rail { .voltage &= 3.234V..3.333V; .pos.net &= \"3V3\"; .neg.net &= \"GND\" }\n    .modem: Rail { .voltage &= 1.71V..1.89V; .pos.net &= \"1V8\"; .neg.net &= \"GND\" }\n";

fn solved(body: &str) -> Solution {
    let src = PARTS.replace("BODY", body);
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
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

/// `got` is within `rel` of `want`, and within two picoamperes of it where `want` is near zero.
fn close(what: &str, got: f64, want: f64, rel: f64) {
    let slack = rel * want.abs() + 2e-12;
    assert!(
        (got - want).abs() <= slack,
        "{what}: got {got}, want {want} (within {slack})"
    );
}

fn close_range(what: &str, got: (f64, f64), want: (f64, f64), rel: f64) {
    close(&format!("{what}, least"), got.0, want.0, rel);
    close(&format!("{what}, greatest"), got.1, want.1, rel);
}

/// A 10 V source across two 1 kohm +/- 1 % resistors. The middle is `10 * R2 / (R1 + R2)`, from
/// `10 * 0.99 / 2.00 = 4.95` (R2 at its least, R1 at its greatest: `10 * 990 / (1010 + 990)`) to
/// `10 * 1.01 / 2.00 = 5.05`. Interval arithmetic would give `10 * [0.99, 1.01]` over the sum of
/// two such intervals, which is 4.901 to 5.101: it counts `R2` twice, once in the numerator and
/// once in the sum. The current is 10 V over the sum, 1.98 kohm to 2.02 kohm.
#[test]
fn a_divider_has_the_range_the_circuit_has_and_not_the_one_interval_arithmetic_gives() {
    let s = solved(
        "    .supply: Rail { .voltage &= 10V; .pos.net &= \"IN\"; .neg.net &= \"GND\" }\n    .r1: Resistor { .resistance &= 1kohm +/- 1% }\n    .r2: Resistor { .resistance &= 1kohm +/- 1% }\n    .supply.pos ~ .r1.a\n    .r1.b ~ .r2.a\n    .r2.b ~ .supply.neg\n",
    );
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let middle = range(&s, "r2.drop");
    close_range("the middle", middle, (4.95, 5.05), 1e-6);
    close_range(
        "the current",
        range(&s, "r1.current"),
        (10.0 / 2020.0, 10.0 / 1980.0),
        1e-6,
    );
    // What interval arithmetic gives is wider by 0.049 V at each end.
    let (naive_lo, naive_hi) = (10.0 * 0.99 / (1.01 + 1.01), 10.0 * 1.01 / (0.99 + 0.99));
    assert!(middle.0 > naive_lo + 0.04 && middle.1 < naive_hi - 0.04);
}

/// The gate network of a low-side switch: a 10 kohm +/- 1 % pull-down on a processor pin that
/// drives an N-channel MOSFET (threshold 1 V to 2.5 V, 7 ohm on) with a 100 kohm pull-up to a
/// 1.8 V rail (1.71 V to 1.89 V) on its drain. The pin's output resistances are 0 to 200 ohm
/// either way, and it can let go. The rail is 3.234 V to 3.333 V.
///
/// - the pull-down carries the rail over the resistor and the output resistance when the pin is
///   high, at most `3.333 / 9900` = 336.7 uA (the rail at its greatest, the resistor at its
///   least, no output resistance), and nothing when the pin is low or lets go (both of its ends
///   are at the ground);
/// - the pin high puts the gate at least `3.234 * 9900 / (9900 + 200)` = 3.170 V above the
///   source, over the greatest threshold: the switch is on and the pull-up carries
///   `1.89 / (100 000 + 7)` at most; with the pin low or let go the gate is at the ground, under
///   the least threshold: off, and nothing flows. The switch's current is the pull-up's.
#[test]
fn the_gate_pulldown_carries_a_third_of_a_milliamp_and_the_switch_follows_the_pin() {
    let s = solved(&format!(
        "{RAILS}    .mcu: Hub {{ .gpio.roh &= 0ohm..200ohm; .gpio.rol &= 0ohm..200ohm }}\n    .r8: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .q1: Switch1 {{ .gate_threshold &= 1V..2.5V; .on_resistance &= 7ohm }}\n    .pu: Resistor {{ .resistance &= 100kohm }}\n    .logic ~ .mcu.vdd\n    .mcu.gpio.line ~ .r8.a\n    .r8.b ~ .logic.neg\n    .mcu.gpio.line ~ .q1.gate\n    .q1.source ~ .logic.neg\n    .modem.pos ~ .pu.a\n    .pu.b ~ .q1.drain\n"
    ));
    assert_eq!(s.dc.switches, 1, "{:?}", s.dc);
    assert_eq!(s.dc.pins, 1, "{:?}", s.dc);
    close_range(
        "the pull-down's current",
        range(&s, "r8.current"),
        (0.0, 3.333 / 9900.0),
        1e-5,
    );
    let want = 1.89 / 100_007.0;
    close_range(
        "the switch's current",
        range(&s, "q1.current"),
        (0.0, want),
        1e-5,
    );
    close_range(
        "the pull-up's current",
        range(&s, "pu.current"),
        (0.0, want),
        1e-5,
    );
}

/// A 1 Mohm +/- 1 % pull-down on the status output of a modem on a 1.8 V rail (1.71 V to
/// 1.89 V) whose pin drives high or low through 0 to 100 ohm or lets go, read by a buffer input
/// that draws nothing. The pull-down carries the most with the pin high at the top of its rail
/// through no resistance and the resistor at its least: `1.89 / 990 000` = 1.909 uA, and nothing
/// when the pin is low or lets go.
#[test]
fn the_status_pulldown_carries_under_two_microamps() {
    let s = solved(&format!(
        "{RAILS}    .pin: Hub {{ .gpio.roh &= 0ohm..100ohm; .gpio.rol &= 0ohm..100ohm }}\n    .r10: Resistor {{ .resistance &= 1Mohm +/- 1% }}\n    .buffer: Quiet\n    .modem ~ .pin.vdd\n    .logic ~ .buffer.vdd\n    .pin.gpio.line ~ .r10.a\n    .pin.gpio.line ~ .buffer.io.line\n    .r10.b ~ .logic.neg\n"
    ));
    assert_eq!(s.dc.pins, 1, "{:?}", s.dc);
    close_range(
        "the pull-down's current",
        range(&s, "r10.current"),
        (0.0, 1.89 / 990_000.0),
        1e-5,
    );
}

const STATUS_LINE: &str = "    .r10: Resistor { .resistance &= 1Mohm +/- 1% }\n    .modem ~ .pin.vdd\n    .pin.gpio.line ~ .r10.a\n    .pin.gpio.line ~ .buffer.io.line\n    .r10.b ~ .logic.neg\n";

fn status_line_current(pin: &str, buffer_on: &str) -> (Solution, (f64, f64)) {
    let s = solved(&format!(
        "{RAILS}    .pin: {pin} {{ .gpio.roh &= 0ohm..100ohm; .gpio.rol &= 0ohm..100ohm }}\n    .buffer: Pad\n    .{buffer_on} ~ .buffer.vdd\n{STATUS_LINE}"
    ));
    let current = range(&s, "r10.current");
    (s, current)
}

/// The status line of the test above with a receiver that states its leakage (`.io.ii &= 1uA`)
/// on the line, as a buffer's datasheet does: the same pull-down current, `1.89 / 990 000` =
/// 1.909 uA at the most, and nothing when the pin is low or lets go. The pin is high at the top
/// of its rail through no resistance and the resistor is at its least. The input's leak is `Ii`
/// over the supply's greatest, 3.333 Mohm to its own 3.3 V rail or to the ground; the pin holds
/// the line to within its output resistance of its rail, so the leak changes the pull-down's
/// current by under a part in a million (the leak to the 3.3 V rail is
/// `(3.333 - 1.89) / 3.333e6` = 0.43 uA through the pin, not through the resistor).
#[test]
fn a_tri_state_pin_beside_a_leaking_input_carries_the_pulldown() {
    let (s, current) = status_line_current("Hub", "logic");
    assert_eq!(s.dc.pins, 2, "{:?}", s.dc);
    assert_eq!(s.dc.islands, 1, "{:?}", s.dc);
    close_range(
        "the pull-down's current",
        current,
        (0.0, 1.89 / 990_000.0),
        1e-5,
    );
}

/// An `output` pin is the same board with no high impedance state: high or low, so the same
/// range (the pull-down carries nothing with the pin low).
#[test]
fn an_output_pin_beside_a_leaking_input_carries_the_pulldown() {
    let (s, current) = status_line_current("Out", "logic");
    assert_eq!(s.dc.pins, 2, "{:?}", s.dc);
    close_range(
        "the pull-down's current",
        current,
        (0.0, 1.89 / 990_000.0),
        1e-5,
    );
}

/// The input on the pin's own 1.8 V rail leaks `1.89 / 1 uA` = 1.89 Mohm to either end of it:
/// the same range.
#[test]
fn a_leaking_input_on_the_pins_own_rail_changes_nothing_either() {
    let (s, current) = status_line_current("Hub", "modem");
    assert_eq!(s.dc.pins, 2, "{:?}", s.dc);
    close_range(
        "the pull-down's current",
        current,
        (0.0, 1.89 / 990_000.0),
        1e-5,
    );
}

/// Two pins that can drive the line still fight, and a receiver on it does not change that: no
/// pin of the line is modelled, and the pull-down's current is not a number the analysis writes.
#[test]
fn two_drivers_and_a_leaking_input_on_one_line_are_still_not_modelled() {
    let s = solved(&format!(
        "{RAILS}    .pin: Hub {{ .gpio.roh &= 0ohm..100ohm; .gpio.rol &= 0ohm..100ohm }}\n    .other: Hub {{ .gpio.roh &= 0ohm..100ohm; .gpio.rol &= 0ohm..100ohm }}\n    .buffer: Pad\n    .modem ~ .other.vdd\n    .logic ~ .buffer.vdd\n    .other.gpio.line ~ .r10.a\n{STATUS_LINE}"
    ));
    assert_eq!(s.dc.pins, 0, "{:?}", s.dc);
    assert!(
        !matches!(
            s.model.domain_at("r10.current"),
            Some(Domain::Quantity(q)) if q.hi() < 1e-3
        ),
        "{:?}",
        s.model.domain_at("r10.current")
    );
}

/// A resistor divider that brings a 3.3 V processor pin (3.234 V to 3.333 V, output resistance 0
/// to 200 ohm) down to a 1.8 V modem input (1.71 V to 1.89 V) that leaks 1 uA at most: 20 kohm
/// +/- 1 % on top and 15 kohm +/- 1 % below. The input is 1.89 Mohm to either of its supplies
/// (the rail's greatest over the leak).
///
/// The middle with the pin high at the top of its rail, the input leaking to its supply at the
/// top of that, R5 low and R4 high, is a Thevenin by hand:
/// `(Vt / R5 + Vm / Rz) / (1 / R5 + 1 / R4 + 1 / Rz)` with `Vt = 3.333`, `Vm = 1.89`,
/// `R5 = 19 800`, `R4 = 15 150` and `Rz = 1.89e6`. The divider's current is the greatest with
/// R5 and R4 low and the input leaking to the ground, in parallel with R4: `Vt / (R5 + R4 || Rz)`.
#[test]
fn the_gnss_divider_middle_is_a_thevenin_worked_by_hand() {
    let s = solved(&format!(
        "{RAILS}    .tx: Hub {{ .gpio.roh &= 0ohm..200ohm; .gpio.rol &= 0ohm..200ohm }}\n    .r5: Resistor {{ .resistance &= 20kohm +/- 1% }}\n    .r4: Resistor {{ .resistance &= 15kohm +/- 1% }}\n    .rx: Pad\n    .logic ~ .tx.vdd\n    .modem ~ .rx.vdd\n    .tx.gpio.line ~ .r5.a\n    .r5.b ~ .r4.a\n    .r5.b ~ .rx.io.line\n    .r4.b ~ .logic.neg\n"
    ));
    assert_eq!(s.dc.pins, 2, "{:?}", s.dc);
    let (vt, vm, r5, r4, rz) = (3.333, 1.89, 19_800.0, 15_150.0, 1.89e6);
    let middle = (vt / r5 + vm / rz) / (1.0 / r5 + 1.0 / r4 + 1.0 / rz);
    close("the middle", range(&s, "r4.drop").1, middle, 1e-5);
    let (r4_low, rz_low) = (14_850.0, 1.89e6);
    let current = vt / (r5 + r4_low * rz_low / (r4_low + rz_low));
    close(
        "the divider's current",
        range(&s, "r5.current").1,
        current,
        1e-5,
    );
    // The middle stays under the least supply of the input it feeds.
    assert!(range(&s, "r4.drop").1 < 1.71);
}

/// A 10 kohm +/- 1 % pull-down on an input that only leaks: 1 uA at most at the greatest supply
/// of the rail (3.201 V to 3.399 V), which is 3.399 Mohm to either supply. Leaking to the ground
/// nothing flows in the resistor; leaking to the supply the current is `Vdd / (R + Rz)`, at most
/// `3.399 / (9900 + 3.399e6)` = 0.9971 uA, and the pin is at most `3.399 * 10 100 / (3.399e6 +
/// 10 100)` = 10.09 mV (the resistor at its greatest).
#[test]
fn a_pulldown_on_an_input_that_only_leaks_holds_the_pin_at_millivolts() {
    let s = solved(
        "    .logic: Rail { .voltage &= 3.3V +/- 3%; .pos.net &= \"3V3\"; .neg.net &= \"GND\" }\n    .pad: Pad\n    .pd: Resistor { .resistance &= 10kohm +/- 1% }\n    .logic ~ .pad.vdd\n    .pad.io.line ~ .pd.a\n    .pd.b ~ .logic.neg\n",
    );
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let rz = 3.399e6;
    close(
        "the current",
        range(&s, "pd.current").1,
        3.399 / (9900.0 + rz),
        1e-5,
    );
    close(
        "the pin",
        range(&s, "pd.drop").1,
        3.399 * 10_100.0 / (rz + 10_100.0),
        1e-5,
    );
    assert!(range(&s, "pd.current").0.abs() < 2e-12);
}

/// The balancing divider of a two-cell supercapacitor with a charge-limit resistor: 5.0 V to
/// 5.5 V across 43 kohm +/- 1 % twice and 10 ohm, each cell leaking 0 to 6 uA (`l1` across the top
/// cell, `l2` across the bottom one; the capacitor is an open circuit at DC). Kirchhoff by hand:
/// the top resistor carries `a`, the bottom one `a + l1 - l2`, the 10 ohm `a + l1`, so
/// `V = a R27 + (a + l1 - l2) R26 + (a + l1) 10`, and the top cell takes `a R27` of the rail.
/// All 32 corners are worked out here and the hull is the expected range.
///
/// The cell across the top resistor takes the most with the largest leakage on one cell only,
/// more than half of the rail: 2.9 V of the 5.5 V, which a rule of thumb of "half the rail" does
/// not say.
///
/// The 1 Mohm +/- 1 % sense divider on the same rail reads an input that leaks to the ground,
/// 3.333 Mohm in parallel with the lower resistor: at the most `5.5 / (990 000 + 990 000 || 3.333e6)`.
#[test]
fn the_supercap_balance_and_the_cell_voltage_are_worked_by_hand() {
    let s = solved(
        "    .rail: Rail { .voltage &= 5V..5.5V; .pos.net &= \"5V\"; .neg.net &= \"GND\" }\n    .logic: Rail { .voltage &= 3.234V..3.333V; .pos.net &= \"3V3\"; .neg.net &= \"GND\" }\n    .r27: Resistor { .resistance &= 43kohm +/- 1% }\n    .r26: Resistor { .resistance &= 43kohm +/- 1% }\n    .r28: Link { .resistance &= 10ohm; .rated_current &= 1A }\n    .leak_top: Leak { .current &= 0A..6uA }\n    .leak_bot: Leak { .current &= 0A..6uA }\n    .r30: Resistor { .resistance &= 1Mohm +/- 1% }\n    .r29: Resistor { .resistance &= 1Mohm +/- 1% }\n    .adc: Pad\n    .rail.pos ~ .r27.a\n    .r27.b ~ .r26.a\n    .r26.b ~ .r28.a\n    .r28.b ~ .rail.neg\n    .rail.pos ~ .leak_top.a\n    .leak_top.b ~ .r27.b\n    .r27.b ~ .leak_bot.a\n    .leak_bot.b ~ .r26.b\n    .rail.pos ~ .r30.a\n    .r30.b ~ .r29.a\n    .r30.b ~ .adc.io.line\n    .r29.b ~ .rail.neg\n    .logic ~ .adc.vdd\n",
    );
    let (mut top_current, mut mid_current, mut bottom_current, mut top_cell) =
        (Hull::new(), Hull::new(), Hull::new(), Hull::new());
    for v in [5.0, 5.5] {
        for r27 in [42_570.0, 43_430.0] {
            for r26 in [42_570.0, 43_430.0] {
                for l1 in [0.0, 6e-6] {
                    for l2 in [0.0, 6e-6] {
                        let a = (v - (l1 - l2) * r26 - l1 * 10.0) / (r27 + r26 + 10.0);
                        top_current.include(a);
                        mid_current.include(a + l1 - l2);
                        bottom_current.include(a + l1);
                        top_cell.include(a * r27);
                    }
                }
            }
        }
    }
    close_range("r27", range(&s, "r27.current"), top_current.pair(), 1e-5);
    close_range("r26", range(&s, "r26.current"), mid_current.pair(), 1e-5);
    close_range("r28", range(&s, "r28.current"), bottom_current.pair(), 1e-5);
    close_range("the top cell", range(&s, "r27.drop"), top_cell.pair(), 1e-5);
    assert!(top_cell.hi > 2.9 && top_cell.hi < 2.92, "{}", top_cell.hi);
    // The sense divider: at most 3.3 uA, the current with the input leaking to the ground.
    let sense = 5.5 / (990_000.0 + 990_000.0 * 3.333e6 / (990_000.0 + 3.333e6));
    close("the sense divider", range(&s, "r30.current").1, sense, 1e-5);
}

/// The least and the greatest of the numbers put in.
struct Hull {
    lo: f64,
    hi: f64,
}

impl Hull {
    fn new() -> Hull {
        Hull {
            lo: f64::MAX,
            hi: f64::MIN,
        }
    }
    fn include(&mut self, x: f64) {
        self.lo = self.lo.min(x);
        self.hi = self.hi.max(x);
    }
    fn pair(&self) -> (f64, f64) {
        (self.lo, self.hi)
    }
}

/// The battery-voltage divider, 2 Mohm +/- 1 % over 100 kohm +/- 1 %, behind the protection: 8.1
/// V (9 V less 0.032 ohm at the largest current and 0.8 V) to 16 V at the node it hangs on. The
/// buffer that reads its middle draws nothing.
///
/// - the current is the supply over the sum: at most `16 / (1.98e6 + 99 000)` and at least
///   `8.1 / (2.02e6 + 101 000)`;
/// - the middle is `V R1 / (R1 + R2)`: at most `16 * 101 000 / (101 000 + 1.98e6)` (R1 at its
///   greatest, R2 at its least), at least `8.1 * 99 000 / (99 000 + 2.02e6)`.
#[test]
fn the_battery_divider_current_and_voltage_are_the_ones_of_ohms_law() {
    let s = solved(
        "    .logic: Rail { .voltage &= 3.234V..3.333V; .pos.net &= \"3V3\"; .neg.net &= \"GND\" }\n    .batt: Rail { .voltage &= 8.1V..16V; .pos.net &= \"SAFE\"; .neg.net &= \"GND\" }\n    .r2: Resistor { .resistance &= 2Mohm +/- 1% }\n    .r1: Resistor { .resistance &= 100kohm +/- 1% }\n    .buffer: Quiet\n    .logic ~ .buffer.vdd\n    .batt.pos ~ .r2.a\n    .r2.b ~ .r1.a\n    .r2.b ~ .buffer.io.line\n    .r1.b ~ .batt.neg\n",
    );
    close_range(
        "the current",
        range(&s, "r2.current"),
        (8.1 / (2.02e6 + 101_000.0), 16.0 / (1.98e6 + 99_000.0)),
        1e-5,
    );
    close_range(
        "the middle",
        range(&s, "r1.drop"),
        (
            8.1 * 99_000.0 / (99_000.0 + 2.02e6),
            16.0 * 101_000.0 / (101_000.0 + 1.98e6),
        ),
        1e-5,
    );
}

/// Two series resistors into inputs that leak 1 uA at most (3.333 Mohm to either supply of the
/// 3.234 V to 3.333 V rail): 10 kohm +/- 1 % after a buffer whose output is 0 V to 3.333 V, and
/// 100 ohm +/- 1 % after a microphone's output that drives high or low through 0 to 200 ohm.
/// What flows is the leak, `V / (R + Rz)`:
///
/// - the 10 kohm at most `3.333 / (9900 + 3.333e6)` = 0.997 uA, either way (the buffer high and
///   the input leaking to the ground, or the buffer low and the input leaking to the supply);
/// - the 100 ohm at most `3.333 / (99 + 3.333e6)`, the microphone high through no resistance
///   and the input leaking to the ground, or low and leaking to the supply.
///
/// So the dissipation of either is the leak squared over the resistance, under 0.1 uW.
#[test]
fn the_series_resistors_into_inputs_carry_only_the_leak_of_the_input() {
    let s = solved(&format!(
        "{RAILS}    .buffer: Rail {{ .voltage &= 0V..3.333V; .pos.net &= \"BUF\"; .neg.net &= \"GND\" }}\n    .r3: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .adc_in: Pad\n    .mic: Out {{ .gpio.roh &= 0ohm..200ohm; .gpio.rol &= 0ohm..200ohm }}\n    .r16: Resistor {{ .resistance &= 100ohm +/- 1% }}\n    .pdm_in: Pad\n    .logic ~ .adc_in.vdd\n    .logic ~ .mic.vdd\n    .logic ~ .pdm_in.vdd\n    .buffer.pos ~ .r3.a\n    .r3.b ~ .adc_in.io.line\n    .mic.gpio.line ~ .r16.a\n    .r16.b ~ .pdm_in.io.line\n    .buffer.neg ~ .logic.neg\n"
    ));
    let rz = 3.333e6;
    let r3 = 3.333 / (9900.0 + rz);
    close_range("r3", range(&s, "r3.current"), (-r3, r3), 1e-5);
    let r16 = 3.333 / (99.0 + rz);
    close_range("r16", range(&s, "r16.current"), (-r16, r16), 1e-5);
    // The power of the resistor at that current.
    assert!(r3 * r3 * 10_100.0 < 1e-7 && r16 * r16 * 101.0 < 1e-9);
}
