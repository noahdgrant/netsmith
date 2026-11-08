//! Unit tests of the numeric core: islands worked out by hand (the first four are the
//! prototype's, ported to the solver's own types).

use super::net::{
    Element, Error, Held, Island, Kind, MAX_COMBINATIONS, Range, Which, check_interior,
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
