//! Unit tests of the numeric core: islands worked out by hand (the first four are the
//! prototype's, ported to the solver's own types).

use super::net::{
    Draw, Element, Error, Held, Island, Kind, MAX_COMBINATIONS, Range, Which, check_interior,
};

fn close(got: f64, want: f64, rel: f64) {
    let slack = rel * want.abs().max(1e-12);
    assert!(
        (got - want).abs() <= slack,
        "got {got}, want {want} (within {slack})"
    );
}

fn range_is(r: Range, lo: f64, hi: f64) {
    close(r.lo, lo, 1e-6);
    close(r.hi, hi, 1e-6);
}

fn tol(nominal: f64, fraction: f64) -> Range {
    Range::new(nominal * (1.0 - fraction), nominal * (1.0 + fraction))
}

fn held(name: &str, v: Range) -> Held {
    Held {
        name: name.to_string(),
        v,
    }
}

fn resistor(name: &str, a: usize, b: usize, r: Range) -> Element {
    Element {
        name: name.to_string(),
        kind: Kind::Resistor { a, b, r },
    }
}

fn diode(name: &str, anode: usize, cathode: usize, vf: Range) -> Element {
    Element {
        name: name.to_string(),
        kind: Kind::Diode { anode, cathode, vf },
    }
}

fn load(name: &str, from: usize, to: usize, i: Range) -> Element {
    Element {
        name: name.to_string(),
        kind: Kind::Current { from, to, i },
    }
}

/// A 10 V source over two 1 kohm +/- 1 % resistors. The middle is `10 * R2 / (R1 + R2)`, from
/// `10 * 0.99 / 2.00 = 4.95` to `10 * 1.01 / 2.00 = 5.05`. Interval arithmetic counts `R2`
/// twice and gives `4.90..5.10`.
#[test]
fn a_divider_has_the_range_the_circuit_has_and_not_the_one_interval_arithmetic_gives() {
    // Nodes: 0 = MID (free), 1 = IN (held 10 V), 2 = GND (held 0).
    let island = Island {
        free: 1,
        held: vec![
            held("supply", Range::point(10.0)),
            held("ground", Range::point(0.0)),
        ],
        elements: vec![
            resistor("r1", 1, 0, tol(1e3, 0.01)),
            resistor("r2", 0, 2, tol(1e3, 0.01)),
        ],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    range_is(report.range(Which::NodeVoltage(0)), 4.95, 5.05);
    range_is(
        report.range(Which::Current(0)),
        10.0 / 2020.0,
        10.0 / 1980.0,
    );
    range_is(report.range(Which::Drop(1)), 4.95, 5.05);
    assert_eq!(report.combinations, 4);
    assert_eq!(report.consistent, 4);
    let naive_lo = 10.0 * 0.99 / (1.01 + 1.01);
    assert!(naive_lo < 4.95 - 0.04);
    assert_eq!(check_interior(&island, &report, 500), 0);
    // The corner that made the high end of the middle: R1 at its least, R2 at its largest.
    let by = report.produced_by(Which::NodeVoltage(0), true);
    assert_eq!(by, "r1 at 990Ω, r2 at 1.01kΩ");
}

/// Series resistors from a source of 12 V to ground, 100 ohm and 220 ohm (1 %), and a load
/// that draws 10 mA to 20 mA from the middle: the current in the first is the load's plus
/// what the second takes. With the load at 20 mA, the first at 99 ohm and the second at
/// 222.2 ohm the middle is `(12 / 99 + (-0.02)) / (1/99 + 1/222.2)`.
#[test]
fn series_resistors_and_a_load_give_the_current_of_each() {
    let island = Island {
        free: 1,
        held: vec![
            held("supply", Range::point(12.0)),
            held("ground", Range::point(0.0)),
        ],
        elements: vec![
            resistor("r1", 1, 0, tol(100.0, 0.01)),
            resistor("r2", 0, 2, tol(220.0, 0.01)),
            load("load", 0, 2, Range::new(0.010, 0.020)),
        ],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    // The first resistor's current: the middle `V` is `(12 / R1 - I) / (1/R1 + 1/R2)`, and the
    // current in R1 is `(12 - V) / R1`. It is the largest with the load at 20 mA and R1 least.
    let i1 = |r1: f64, r2: f64, load: f64| {
        let v = (12.0 / r1 - load) / (1.0 / r1 + 1.0 / r2);
        (12.0 - v) / r1
    };
    let want_hi = [99.0, 101.0]
        .into_iter()
        .flat_map(|r1| {
            [217.8, 222.2]
                .into_iter()
                .flat_map(move |r2| [0.010, 0.020].into_iter().map(move |l| i1(r1, r2, l)))
        })
        .fold(f64::MIN, f64::max);
    close(report.range(Which::Current(0)).hi, want_hi, 1e-9);
    // The current in the second is the first's less the load's.
    let want_lo = [99.0, 101.0]
        .into_iter()
        .flat_map(|r1| {
            [217.8, 222.2]
                .into_iter()
                .flat_map(move |r2| [0.010, 0.020].into_iter().map(move |l| i1(r1, r2, l) - l))
        })
        .fold(f64::MAX, f64::min);
    close(report.range(Which::Current(1)).lo, want_lo, 1e-9);
    assert_eq!(check_interior(&island, &report, 500), 0);
}

/// A pull-down of 10 kohm +/- 1 % from a 3.3 V +/- 3 % rail to ground: the current is
/// `3.201 / 10100` to `3.399 / 9900`, the drop is the rail's.
#[test]
fn a_pulldown_on_a_rail_carries_the_rail_over_its_resistance() {
    let island = Island {
        free: 0,
        held: vec![
            held("vdd", Range::new(3.201, 3.399)),
            held("ground", Range::point(0.0)),
        ],
        elements: vec![resistor("pd", 0, 1, tol(1e4, 0.01))],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    range_is(
        report.range(Which::Current(0)),
        3.201 / 10100.0,
        3.399 / 9900.0,
    );
    range_is(report.range(Which::Drop(0)), 3.201, 3.399);
    let hi = report.produced_by(Which::Current(0), true);
    assert_eq!(hi, "vdd at 3.399V, pd at 9.9kΩ");
}

/// A source of 9 V to 16 V, a fuse of 32 mohm, a diode of 0 V to 0.8 V and a 10 ohm load. The
/// load sees `Vin - 0.032 I - Vf`: at 16 V and `Vf = 0` it is `16 * 10 / 10.032 = 15.949 V`, at
/// 9 V and `Vf = 0.8` it is `(9 - 0.8) * 10 / 10.032 = 8.1738 V`. A supply of -14 V blocks the
/// diode: nothing flows, the load is at 0 V and the conducting states are refused.
#[test]
fn a_fuse_and_a_diode_drop_in_a_chain_and_the_diode_blocks_a_reversed_supply() {
    let build = |source: Range| Island {
        // 0 = AFTER_FUSE, 1 = SAFE, 2 = VBATT (held), 3 = GND (held).
        free: 2,
        held: vec![held("batt", source), held("ground", Range::point(0.0))],
        elements: vec![
            resistor("fuse", 2, 0, Range::point(0.032)),
            diode("d", 0, 1, Range::new(0.0, 0.8)),
            resistor("load", 1, 3, Range::point(10.0)),
        ],
    };
    let report = build(Range::new(9.0, 16.0))
        .analyze(MAX_COMBINATIONS)
        .unwrap();
    let safe = report.range(Which::NodeVoltage(1));
    close(safe.hi, 16.0 * 10.0 / 10.032, 1e-9);
    close(safe.lo, (9.0 - 0.8) * 10.0 / 10.032, 1e-9);
    let i = report.range(Which::Current(0));
    close(i.hi, 16.0 / 10.032, 1e-9);
    // The diode cannot block here: its anode is above its drop with the load behind it.
    close(i.lo, (9.0 - 0.8) / 10.032, 1e-9);
    let by = report.produced_by(Which::Current(0), true);
    assert!(
        by.contains("batt at 16V") && by.contains("d conducting at 0V"),
        "{by}"
    );
    // The diode's own current is never negative.
    assert!(report.range(Which::Current(1)).lo >= 0.0);

    let reversed = build(Range::point(-14.0))
        .analyze(MAX_COMBINATIONS)
        .unwrap();
    close(reversed.range(Which::NodeVoltage(1)).hi, 0.0, 1.0);
    assert_eq!(reversed.range(Which::Current(1)), Range::point(0.0));
    assert!(reversed.consistent < reversed.combinations);
}

/// Only the states that are the circuit's are counted: with the source at 12 V and a forward
/// drop of 0.7 V across a 100 ohm load, the diode conducts and cannot be blocking.
#[test]
fn a_diode_with_its_anode_above_its_drop_cannot_block() {
    let island = Island {
        free: 1,
        held: vec![
            held("supply", Range::point(12.0)),
            held("ground", Range::point(0.0)),
        ],
        elements: vec![
            diode("d", 1, 0, Range::point(0.7)),
            resistor("load", 0, 2, Range::point(100.0)),
        ],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    assert_eq!(report.consistent, 1);
    range_is(report.range(Which::Current(0)), 0.113, 0.113);
    range_is(report.range(Which::NodeVoltage(0)), 11.3, 11.3);
}

#[test]
fn too_many_combinations_is_an_error_and_not_a_wait() {
    // A chain of 20 resistors of 1 % is 2^20 combinations, which is more than the limit.
    let mut elements = Vec::new();
    for k in 0..20 {
        let a = if k == 0 { 19 } else { k - 1 };
        elements.push(resistor(&format!("r{k}"), a, k, tol(1000.0, 0.01)));
    }
    let island = Island {
        free: 19,
        held: vec![
            held("supply", Range::point(1.0)),
            held("g", Range::point(0.0)),
        ],
        elements,
    };
    assert_eq!(island.combinations(), 1 << 20);
    match island.analyze(MAX_COMBINATIONS) {
        Err(Error::TooManyCombinations {
            combinations,
            limit,
        }) => {
            assert_eq!(combinations, 1 << 20);
            // The limit is the work's: less than the combinations' for an island this size.
            assert!(limit <= MAX_COMBINATIONS && limit > 1 << 12, "{limit}");
        }
        other => panic!("{:?}", other.map(|r| r.combinations)),
    }
}

/// A diode feeding a load that always draws current cannot block: the node behind it would fall
/// without limit, which no blocking diode allows. Only the two conducting states are counted.
#[test]
fn a_diode_in_front_of_a_load_that_always_draws_cannot_block() {
    let island = Island {
        free: 1,
        held: vec![
            held("supply", Range::point(12.0)),
            held("ground", Range::point(0.0)),
        ],
        elements: vec![
            diode("d", 1, 0, Range::new(0.6, 0.8)),
            load("load", 0, 2, Range::new(0.1, 0.2)),
        ],
    };
    assert_eq!(island.combinations(), 6);
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    assert_eq!(report.consistent, 4);
    range_is(report.range(Which::NodeVoltage(0)), 11.2, 11.4);
    range_is(report.range(Which::Current(0)), 0.1, 0.2);
}

fn switching(name: &str, from: usize, to: usize, p: Range, eff: Range) -> Element {
    Element {
        name: name.to_string(),
        kind: Kind::Regulator {
            from,
            to,
            draw: Draw::Switching { p, eff },
        },
    }
}

fn linear(name: &str, from: usize, to: usize, i: Range) -> Element {
    Element {
        name: name.to_string(),
        kind: Kind::Regulator {
            from,
            to,
            draw: Draw::Linear { i },
        },
    }
}

/// The prototype's: a 12 V source, 0.5 ohm in series and a regulator (5 V out at 1 A, 90 %).
/// It draws `I = Pout / (eff * Vin)` with `Vin = 12 - 0.5 I`: `0.9 I (12 - 0.5 I) = 5`, so
/// `0.45 I^2 - 10.8 I + 5 = 0`. Without the drop, `I` would be 5 / (0.9 * 12) = 0.46296 A.
#[test]
fn a_regulator_draws_its_power_through_the_drops_before_it() {
    // Nodes: 0 = VIN (free), 1 = source (held 12 V), 2 = ground (held 0).
    let island = Island {
        free: 1,
        held: vec![
            held("src", Range::point(12.0)),
            held("gnd", Range::point(0.0)),
        ],
        elements: vec![
            resistor("r", 1, 0, Range::point(0.5)),
            switching("reg", 0, 2, Range::point(5.0), Range::point(0.9)),
        ],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    let i = (10.8 - (10.8f64 * 10.8 - 4.0 * 0.45 * 5.0).sqrt()) / (2.0 * 0.45);
    close(i, 0.47226, 1e-4);
    range_is(report.range(Which::Current(1)), i, i);
    range_is(report.range(Which::Current(0)), i, i);
    // The regulator's input voltage is the drop of its element.
    range_is(report.range(Which::Drop(1)), 12.0 - 0.5 * i, 12.0 - 0.5 * i);
    range_is(report.range(Which::HeldDraw(0)), i, i);
    assert!(i > 5.0 / (0.9 * 12.0) + 0.009);
    assert!(report.collapses.is_empty());
}

/// Two regulators in a row, the second the load of the first: a 3.3 V stage (85 %) with a 0.5 A
/// load asks `3.3 * 0.5 / (0.85 * 5)` = 0.38824 A of the 5 V output, and the 5 V stage (90 %)
/// from the source through 0.5 ohm asks `0.9 I (12 - 0.5 I) = 5 * 0.38824`. The islands are
/// solved the output side first, and what the output gives is the next island's load.
#[test]
fn a_regulator_behind_a_regulator_is_the_load_of_the_first() {
    // Behind: the 5 V output (held) and the second regulator's input on it.
    let behind = Island {
        free: 0,
        held: vec![
            held("v5", Range::point(5.0)),
            held("gnd", Range::point(0.0)),
        ],
        elements: vec![switching(
            "b",
            0,
            1,
            Range::point(3.3 * 0.5),
            Range::point(0.85),
        )],
    };
    let ib = 3.3 * 0.5 / (0.85 * 5.0);
    let report = behind.analyze(MAX_COMBINATIONS).unwrap();
    range_is(report.range(Which::Current(0)), ib, ib);
    let load = report.range(Which::HeldDraw(0));
    range_is(load, ib, ib);
    let front = Island {
        free: 1,
        held: vec![
            held("src", Range::point(12.0)),
            held("gnd", Range::point(0.0)),
        ],
        elements: vec![
            resistor("r", 1, 0, Range::point(0.5)),
            switching("a", 0, 2, Range::point(5.0 * load.hi), Range::point(0.9)),
        ],
    };
    let report = front.analyze(MAX_COMBINATIONS).unwrap();
    let pout = 5.0 * ib;
    let ia = (10.8 - (10.8f64 * 10.8 - 4.0 * 0.45 * pout).sqrt()) / (2.0 * 0.45);
    range_is(report.range(Which::Current(1)), ia, ia);
}

/// The source cannot give it: 12 V through 10 ohm gives at most `12^2 / (4 * 10) = 3.6 W`, and
/// the regulator asks for `5 / 0.9 = 5.56 W`. There is no operating point, and the report says
/// which regulator and the voltage its input fell to.
#[test]
fn a_regulator_asking_for_more_than_the_source_can_give_collapses() {
    let island = Island {
        free: 1,
        held: vec![
            held("src", Range::point(12.0)),
            held("gnd", Range::point(0.0)),
        ],
        elements: vec![
            resistor("r", 1, 0, Range::point(10.0)),
            switching("reg", 0, 2, Range::point(5.0), Range::point(0.9)),
        ],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    assert_eq!(report.consistent, 0);
    assert_eq!(report.collapses.len(), 1);
    assert_eq!(report.collapses[0].element, 1);
    assert!(report.collapses[0].vin < 1e-3, "{:?}", report.collapses);
}

/// A supply of 10 V to 16 V through 10 ohm and a regulator that asks 5.56 W: at 16 V the source
/// can give `16^2 / 40 = 6.4 W`, at 10 V only 2.5 W. One corner collapses and the other is the
/// circuit's: the range is that of the corner that holds, and the collapse is reported with the
/// states that gave it.
#[test]
fn only_the_corners_that_collapse_are_reported() {
    let island = Island {
        free: 1,
        held: vec![
            held("src", Range::new(10.0, 16.0)),
            held("gnd", Range::point(0.0)),
        ],
        elements: vec![
            resistor("r", 1, 0, Range::point(10.0)),
            switching("reg", 0, 2, Range::point(5.0), Range::point(0.9)),
        ],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    assert_eq!(report.combinations, 2);
    assert_eq!(report.consistent, 1);
    assert_eq!(report.collapses.len(), 1);
    assert_eq!(
        report.describe(report.collapses[0].combination),
        "src at 10V"
    );
    // At 16 V: `0.9 I (16 - 10 I) = 5` is `9 I^2 - 14.4 I + 5 = 0`.
    let i = (14.4 - (14.4f64 * 14.4 - 4.0 * 9.0 * 5.0).sqrt()) / 18.0;
    range_is(report.range(Which::Current(0)), i, i);
}

/// A diode that blocks in front of a regulator leaves it no input: that is a state that is not
/// the circuit's, refused, and not a finding. The diode that conducts is solved: `Vin = 12 - 0.5
/// - 1.5 I`, and `0.9 I (11.5 - 1.5 I) = 5`.
#[test]
fn a_blocking_diode_in_front_of_a_regulator_is_refused_and_not_a_collapse() {
    // Nodes: 0 = after the diode (free), 1 = source, 2 = ground.
    let island = Island {
        free: 1,
        held: vec![
            held("src", Range::point(12.0)),
            held("gnd", Range::point(0.0)),
        ],
        elements: vec![
            diode("d", 1, 0, Range::point(0.5)),
            switching("reg", 0, 2, Range::point(5.0), Range::point(0.9)),
        ],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    assert!(report.collapses.is_empty(), "{:?}", report.collapses);
    assert_eq!(report.consistent, 1);
    let i = 5.0 / (0.9 * 11.5);
    range_is(report.range(Which::Current(0)), i, i);
}

/// A linear regulator draws what its output supplies: its current is the range of the load,
/// whatever its input. 12 V through 1 ohm and a load of 0.1 A to 0.2 A: the input is
/// `12 - I` and the chain carries the load's current.
#[test]
fn a_linear_regulator_passes_its_output_current() {
    let island = Island {
        free: 1,
        held: vec![
            held("src", Range::point(12.0)),
            held("gnd", Range::point(0.0)),
        ],
        elements: vec![
            resistor("r", 1, 0, Range::point(1.0)),
            linear("reg", 0, 2, Range::new(0.1, 0.2)),
        ],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    range_is(report.range(Which::Current(0)), 0.1, 0.2);
    range_is(report.range(Which::Drop(1)), 11.8, 11.9);
}

/// The interior check holds for a switching regulator: a point inside the ranges is inside the
/// hull of the corners (the input current is monotone in the power, the efficiency and the
/// supply).
#[test]
fn the_corners_of_a_regulator_island_cover_its_interior() {
    let island = Island {
        free: 1,
        held: vec![
            held("src", Range::new(9.0, 16.0)),
            held("gnd", Range::point(0.0)),
        ],
        elements: vec![
            resistor("r", 1, 0, tol(0.1, 0.05)),
            switching("reg", 0, 2, Range::new(2.0, 4.0), Range::new(0.85, 0.95)),
        ],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    assert_eq!(report.combinations, 16);
    assert_eq!(check_interior(&island, &report, 500), 0);
}

/// A regulator in an island of 15 free nodes is limited by the rounds of its fixed point too: the
/// limit counts them, so no island takes longer than one without a regulator.
#[test]
fn an_island_with_a_switching_regulator_has_a_smaller_limit() {
    let mut elements = Vec::new();
    // 12 resistors of 1 % in a chain from the source: 13 nodes' worth of combinations.
    for k in 0..12 {
        elements.push(resistor(
            &format!("r{k}"),
            if k == 0 { 1 } else { k - 1 },
            k,
            tol(0.1, 0.01),
        ));
    }
    elements.push(switching(
        "reg",
        11,
        2,
        Range::new(2.0, 4.0),
        Range::new(0.85, 0.95),
    ));
    let island = Island {
        free: 12,
        held: vec![
            held("src", Range::new(9.0, 16.0)),
            held("gnd", Range::point(0.0)),
        ],
        elements,
    };
    assert!(matches!(
        island.analyze(MAX_COMBINATIONS),
        Err(Error::TooManyCombinations { .. })
    ));
}

fn pin(
    name: &str,
    node: usize,
    high: Option<(usize, Range)>,
    low: Option<(usize, Range)>,
    z: bool,
) -> Element {
    Element {
        name: name.to_string(),
        kind: Kind::Pin { node, high, low, z },
    }
}

fn mosfet(
    name: &str,
    (gate, drain, source): (usize, usize, usize),
    p_channel: bool,
    vth: Range,
    ron: Range,
) -> Element {
    Element {
        name: name.to_string(),
        kind: Kind::Mosfet {
            gate,
            drain,
            source,
            p_channel,
            vth,
            ron,
        },
    }
}

/// A pin that drives a 10 kohm +/- 1 % pull-down on a 3.3 V supply: high through up to 100 ohm,
/// low through up to 50 ohm, or high impedance. The current is the supply over the pull-down and
/// the pin's output resistance when it is high, `3.3 / (9.9 kohm + 1 mohm)` at the most, and zero
/// when it is low or lets go (the node is at ground in both).
#[test]
fn a_pin_drives_a_pulldown_in_each_of_its_states() {
    // Nodes: 0 = the line (free), 1 = vdd (held 3.3 V), 2 = ground.
    let island = Island {
        free: 1,
        held: vec![
            held("vdd", Range::point(3.3)),
            held("ground", Range::point(0.0)),
        ],
        elements: vec![
            pin(
                "mcu.out",
                0,
                Some((1, Range::new(1e-3, 100.0))),
                Some((2, Range::new(1e-3, 50.0))),
                true,
            ),
            resistor("pd", 0, 2, tol(1e4, 0.01)),
        ],
    };
    // High 2 + low 2 + high impedance 1, times the two ends of the pull-down.
    assert_eq!(island.combinations(), 10);
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    assert_eq!(report.consistent, 10);
    // The pull-down's current runs from the line to ground.
    range_is(report.range(Which::Current(1)), 0.0, 3.3 / (9900.0 + 1e-3));
    // The line is `3.3 * R / (R + roh)` at the most (1 mohm), and at ground when low or let go.
    range_is(
        report.range(Which::NodeVoltage(0)),
        0.0,
        3.3 * 9900.0 / (9900.0 + 1e-3),
    );
    // The least high, at the greatest resistance of the pin and the least of the pull-down.
    let states = report.produced_by(Which::Current(1), true);
    assert!(states.contains("mcu.out high through"), "{states}");
    assert_eq!(check_interior(&island, &report, 500), 0);
}

/// An N-channel switch on a 12 V load of 100 ohm, its gate driven by a pin from a 3.3 V supply
/// with a 10 kohm pull-down. The threshold is 1 V to 2.5 V and the channel 50 mohm to 100 mohm.
/// With the pin high the gate is at about 3.3 V, above the greatest threshold: on, and off is
/// refused. Low, or let go (the pull-down holds it at ground), the gate is below the least
/// threshold: off, and on is refused.
#[test]
fn a_gate_driven_by_a_pin_turns_the_load_on_only_when_the_pin_is_high() {
    // Nodes: 0 = gate, 1 = drain (free); 2 = 12 V, 3 = 3.3 V, 4 = ground (held).
    let island = Island {
        free: 2,
        held: vec![
            held("supply", Range::point(12.0)),
            held("vdd", Range::point(3.3)),
            held("ground", Range::point(0.0)),
        ],
        elements: vec![
            pin(
                "mcu.gpio",
                0,
                Some((3, Range::new(1e-3, 100.0))),
                Some((4, Range::new(1e-3, 50.0))),
                true,
            ),
            resistor("pd", 0, 4, tol(1e4, 0.01)),
            mosfet(
                "q",
                (0, 1, 4),
                false,
                Range::new(1.0, 2.5),
                Range::new(0.05, 0.1),
            ),
            resistor("load", 2, 1, Range::point(100.0)),
        ],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    // High: 2 pin states x 2 pull-down x on at 2 channel values = 8. Low and let go: 3 pin
    // states x 2 pull-down x off = 6. The other 16 of the 30 are refused.
    assert_eq!(report.combinations, 30);
    assert_eq!(report.consistent, 14);
    // The load's current is zero when off, `12 / (100 + 0.1)` at the most when on.
    range_is(report.range(Which::Current(3)), 0.0, 12.0 / 100.05);
    // The channel carries the same current, and drops 12 V when off.
    range_is(report.range(Which::Current(2)), 0.0, 12.0 / 100.05);
    close(report.range(Which::Drop(2)).hi, 12.0, 1e-9);
    // The most the load takes is with the pin high and the channel at its least resistance.
    let states = report.produced_by(Which::Current(3), true);
    assert!(states.contains("q on at 50mΩ"), "{states}");
    assert!(states.contains("mcu.gpio high through"), "{states}");
    assert_eq!(check_interior(&island, &report, 500), 0);
}

/// A pin that can let go of a gate that nothing else holds leaves the gate floating, and a
/// floating gate is no state the circuit sets: the switch may be on or off.
#[test]
fn a_gate_that_nothing_holds_may_be_either_way() {
    // Nodes: 0 = gate, 1 = drain (free); 2 = 12 V, 3 = 3.3 V, 4 = ground (held).
    let island = Island {
        free: 2,
        held: vec![
            held("supply", Range::point(12.0)),
            held("vdd", Range::point(3.3)),
            held("ground", Range::point(0.0)),
        ],
        elements: vec![
            pin(
                "mcu.gpio",
                0,
                Some((3, Range::point(100.0))),
                Some((4, Range::point(50.0))),
                true,
            ),
            mosfet(
                "q",
                (0, 1, 4),
                false,
                Range::new(1.0, 2.5),
                Range::point(0.1),
            ),
            resistor("load", 2, 1, Range::point(100.0)),
        ],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    // High (on only), low (off only), let go (on and off): 1 + 1 + 2 of the 3 x 2 = 6.
    assert_eq!(report.combinations, 6);
    assert_eq!(report.consistent, 4);
    range_is(report.range(Which::Current(2)), 0.0, 12.0 / 100.1);
    assert!(
        report
            .produced_by(Which::Current(2), true)
            .contains("mcu.gpio"),
        "the let go state is one of those that turn it on and off"
    );
}

/// A P-channel switch from a 12 V supply to a 100 ohm load. The drive is the source above the
/// gate. A gate at ground gives 12 V: on. A gate at the supply gives 0 V: off.
#[test]
fn a_p_channel_switch_is_on_for_a_gate_below_its_source() {
    let build = |gate: f64| Island {
        free: 1,
        // Nodes: 0 = drain (free); 1 = 12 V (the source), 2 = gate, 3 = ground.
        held: vec![
            held("supply", Range::point(12.0)),
            held("gate", Range::point(gate)),
            held("ground", Range::point(0.0)),
        ],
        elements: vec![
            mosfet(
                "q",
                (2, 0, 1),
                true,
                Range::new(1.0, 2.5),
                Range::new(0.05, 0.1),
            ),
            resistor("load", 0, 3, Range::point(100.0)),
        ],
    };
    let on = build(0.0).analyze(MAX_COMBINATIONS).unwrap();
    // On at 50 mohm and at 100 mohm; off is refused.
    assert_eq!(on.consistent, 2);
    // The channel's own direction is drain to source: the load takes the current from the source.
    range_is(on.range(Which::Current(0)), -12.0 / 100.05, -12.0 / 100.1);
    let off = build(12.0).analyze(MAX_COMBINATIONS).unwrap();
    assert_eq!(off.consistent, 1);
    range_is(off.range(Which::Current(0)), 0.0, 0.0);
}

/// A gate inside the threshold's range keeps both states: 1.5 V is above the least threshold
/// (on is possible) and below the greatest (off is possible).
#[test]
fn a_gate_inside_the_threshold_range_keeps_both_states() {
    let island = Island {
        free: 1,
        // Nodes: 0 = drain; 1 = 12 V, 2 = gate, 3 = ground (the source).
        held: vec![
            held("supply", Range::point(12.0)),
            held("gate", Range::point(1.5)),
            held("ground", Range::point(0.0)),
        ],
        elements: vec![
            mosfet(
                "q",
                (2, 0, 3),
                false,
                Range::new(1.0, 2.5),
                Range::point(0.1),
            ),
            resistor("load", 1, 0, Range::point(100.0)),
        ],
    };
    let report = island.analyze(MAX_COMBINATIONS).unwrap();
    assert_eq!(report.consistent, 2);
    range_is(report.range(Which::Current(0)), 0.0, 12.0 / 100.1);
}
