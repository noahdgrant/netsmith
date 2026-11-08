//! The islands of `tests/islands/`: circuits of a real 12 V automotive board, read from the
//! export of `netsmith build`, each checked against a closed form worked out here, apart from
//! the solver.

use netsmith_dc::{Range, Report, analyze, text::parse};

fn run(text: &str) -> Report {
    analyze(&parse(text).expect("the island parses")).expect("the island has an operating point")
}

fn close(got: f64, want: f64, rel: f64) {
    let slack = rel * want.abs();
    assert!(
        (got - want).abs() <= slack,
        "got {got}, want {want} (within {slack})"
    );
}

const GATE: &str = include_str!("islands/gate_pulldown.cir");
const DTR: &str = include_str!("islands/dtr_pulldown.cir");
const STATUS: &str = include_str!("islands/status_pulldown.cir");
const GNSS: &str = include_str!("islands/gnss_divider.cir");
const SUPERCAP: &str = include_str!("islands/supercap_balance.cir");
const BATTERY: &str = include_str!("islands/battery_divider.cir");
const SERIES: &str = include_str!("islands/series_resistors.cir");
const CHAIN: &str = include_str!("islands/input_chain.cir");

/// 10 kohm +/- 1 % from a 3.333 V pin: at most `3.333 / 9900` = 336.7 uA and 1.122 mW, of the
/// 100 mW of a 0603 resistor. The switch is on when the pin is high (the gate is 3.17 V or more
/// against a threshold of at most 2.5 V) and off otherwise: half of the 96 combinations are
/// refused, as the states they choose are not the circuit's.
#[test]
fn the_gate_pulldown_carries_a_third_of_a_milliamp_and_the_switch_follows_the_pin() {
    let r = run(GATE);
    close(r.get("i(r8)").unwrap().hi, 3.333 / 9900.0, 1e-4);
    close(r.get("p(r8)").unwrap().hi, 3.333 * 3.333 / 9900.0, 1e-4);
    assert!(
        r.get("p(r8)").unwrap().hi < 0.1 / 50.0,
        "a 2 % use of the rating"
    );
    assert_eq!(r.combinations, 96);
    assert_eq!(r.inconsistent, 48);
    // The switch's current is the assumed pull-up's: 1.89 V over 100 kohm and 7 ohm.
    close(r.get("i(q1)").unwrap().hi, 1.89 / 100_007.0, 1e-4);
}

#[test]
fn the_100k_pulldown_carries_34_microamps() {
    let r = run(DTR);
    close(r.get("i(r7)").unwrap().hi, 3.333 / 99_000.0, 1e-4);
    close(r.get("p(r7)").unwrap().hi, 3.333 * 3.333 / 99_000.0, 1e-4);
    let by = r.produced_by("i(r7)", true).unwrap();
    assert!(by.contains("pin: high through 0ohm"), "{by}");
}

#[test]
fn the_status_pulldown_carries_under_two_microamps() {
    let r = run(STATUS);
    close(r.get("i(r10)").unwrap().hi, 1.89 / 990_000.0, 1e-4);
}

/// The divider's middle with the pin high at the top of its rail and the modem's input leaking
/// to its supply: Thevenin by hand,
/// `(Vt / R5 + Vdd / Rz) / (1 / R5 + 1 / R4 + 1 / Rz)`, with R5 low and R4 high.
#[test]
fn the_gnss_divider_middle_is_a_thevenin_worked_by_hand() {
    let r = run(GNSS);
    let (vt, r5, r4, rz) = (3.333, 19_800.0, 15_150.0, 3.3e6);
    let want = (vt / r5 + vt / rz) / (1.0 / r5 + 1.0 / r4 + 1.0 / rz);
    close(r.get("v(RXD)").unwrap().hi, want, 1e-4);
    // The divider's current is the largest at the same rail with R4 low: 3.333 / 34.65 kohm.
    close(
        r.get("i(r5)").unwrap().hi,
        3.333 / (19_800.0 + 14_850.0),
        5e-3,
    );
    // The modem's input (1.8 V) is above this by a margin that the checks of the board need:
    // the middle stays under 1.8 V.
    assert!(r.get("v(RXD)").unwrap().hi < 1.8);
}

/// The balancing divider: the top resistor carries `a`, the bottom one `a + l1 - l2`, the 10
/// ohm `a + l1`; the middle is `V - a R27`. All 32 corners worked out here. The cell across
/// the top resistor takes `a R27` at most: more than half the rail, by the imbalance of the cells'
/// leakage, which is what the island is for.
#[test]
fn the_supercap_balance_and_the_cell_voltage_are_worked_by_hand() {
    let r = run(SUPERCAP);
    let (mut mid, mut top_current, mut top_cell) = (Range::EMPTY, Range::EMPTY, Range::EMPTY);
    for v in [5.0, 5.5] {
        for r27 in [42_570.0, 43_430.0] {
            for r26 in [42_570.0, 43_430.0] {
                for l1 in [0.0, 6e-6] {
                    for l2 in [0.0, 6e-6] {
                        // The sense divider and the pin on the same rail do not touch these nodes.
                        let a = (v - (l1 - l2) * r26 - l1 * 10.0) / (r27 + r26 + 10.0);
                        mid.include(v - a * r27);
                        top_current.include(a);
                        top_cell.include(a * r27);
                    }
                }
            }
        }
    }
    close(r.get("v(MID)").unwrap().lo, mid.lo, 1e-4);
    close(r.get("v(MID)").unwrap().hi, mid.hi, 1e-4);
    close(r.get("i(r27)").unwrap().hi, top_current.hi, 1e-4);
    close(r.get("i(r27)").unwrap().lo, top_current.lo, 1e-4);
    // What the netlist says and the board's rule of thumb does not: with the largest leakage
    // on one cell only, the other cell takes 2.9 V of the 5.5 V.
    assert!(top_cell.hi > 2.9 && top_cell.hi < 2.92, "{top_cell}");
    // The sense divider: 1 Mohm over 1 Mohm from the rail, 3.3 uA at the most.
    assert!(r.get("i(r30)").unwrap().hi < 3.2e-6);
}

#[test]
fn the_battery_divider_current_and_power_are_the_ones_of_ohms_law() {
    let r = run(BATTERY);
    close(r.get("i(r2)").unwrap().hi, 16.0 / (1.98e6 + 99_000.0), 1e-3);
    close(
        r.get("p(r2)").unwrap().hi,
        16.0 * 16.0 * 1.98e6 / (2.079e6 * 2.079e6),
        1e-3,
    );
}

#[test]
fn the_series_resistors_into_inputs_dissipate_nothing_that_matters() {
    let r = run(SERIES);
    assert!(
        r.get("p(r3)").unwrap().hi < 1e-7,
        "{}",
        r.get("p(r3)").unwrap()
    );
    assert!(r.get("p(r16)").unwrap().hi < 1e-9);
    assert!(r.get("i(r3)").unwrap().abs_max() < 1.1e-6);
}

/// The power tree. Worked out by hand for the corner that draws the most: the battery at 9 V,
/// the diode at 0.8 V, the fuse 32 mohm, the shunt 50.5 mohm and the bead 70 mohm in series
/// (0.1525 ohm); the 3.8 V stage at 3.914 V and 80 % supplying 1.2 A and the LDO's 0.3 A and
/// 140 uA; its input is the output of the 5.3 V stage with the balancing divider on it
/// (5.5 V over 85.14 kohm); the 5.3 V stage at 91.5 %. `0.915 I (8.2 - 0.1525 I) = P` is a
/// quadratic, and the stage's own 20 uA is added.
#[test]
fn the_input_chain_current_is_what_the_regulators_power_asks_through_the_drops() {
    let r = run(CHAIN);
    assert!(r.collapsed.is_empty());
    let p3: f64 = 3.914 * (1.2 + 0.3 + 140e-6);
    let p5: f64 = p3 / 0.8 + 5.5 * (5.5 / (2.0 * 42_570.0 + 10.0));
    let (a, b, c): (f64, f64, f64) = (0.915 * 0.1525, 0.915 * 8.2, p5);
    let i = (b - (b * b - 4.0 * a * c).sqrt()) / (2.0 * a) + 20e-6;
    close(r.get("i(f1)").unwrap().hi, i, 2e-3);
    close(r.get("p(d2)").unwrap().hi, 0.8 * i, 2e-3);
    // The hand-written equations with the seed `vin.voltage &= 7V..16V` bound it by
    // 5.5 * 1.47 / (7 * 0.915) = 1.26 A: the seed's 7 V is below the 8.05 V the chain can give.
    let seeded = 5.5 * 1.468 / (7.0 * 0.915);
    assert!(seeded > 1.2 * r.get("i(f1)").unwrap().hi);
    close(r.get("v(VIN5)").unwrap().lo, 8.2 - 0.1525 * i, 5e-3);
    assert!(r.get("v(VIN5)").unwrap().lo > 7.0);
    // The same current is in every part of the chain (the bead's differs only by what GMIN
    // takes, which is picoamperes).
    for part in ["i(d2)", "i(r15)", "i(fb1)", "i(reg5v3.in)"] {
        close(r.get(part).unwrap().hi, r.get("i(f1)").unwrap().hi, 1e-6);
    }
    // The fuse's 3 A rating and the diode's 3 A: a third of each.
    assert!(r.get("i(f1)").unwrap().hi < 1.0);
}
