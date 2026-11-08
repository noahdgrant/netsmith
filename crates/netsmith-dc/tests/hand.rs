//! The solver against circuits worked out by hand.

use netsmith_dc::{
    Kind, Logic, LogicState, Netlist, Range, analyze, check_interior, corners::Error, text::parse,
};

fn close(got: f64, want: f64, rel: f64) {
    let slack = rel * want.abs().max(1e-12);
    assert!(
        (got - want).abs() <= slack,
        "got {got}, want {want} (within {slack})"
    );
}

fn near_zero(r: Range) {
    assert!(r.lo.abs() < 1e-9 && r.hi.abs() < 1e-9, "{r} is not zero");
}

fn range_is(r: Range, lo: f64, hi: f64) {
    close(r.lo, lo, 1e-6);
    close(r.hi, hi, 1e-6);
}

/// A 10 V source across two 1 kohm +/- 1 % resistors. The middle is `10 * R2 / (R1 + R2)`, from
/// `10 * 0.99 / 2.00 = 4.95` to `10 * 1.01 / 2.00 = 5.05`. Interval arithmetic gives
/// `10 * [0.99, 1.01] / ([0.99, 1.01] + [0.99, 1.01])`, which is `4.901..5.101`: it counts
/// `R2` twice, once in the numerator and once in the sum.
#[test]
fn a_divider_has_the_range_the_circuit_has_and_not_the_one_interval_arithmetic_gives() {
    let mut nl = Netlist::new();
    nl.vsource("v", "IN", "GND", Range::point(10.0));
    nl.resistor("r1", "IN", "MID", Range::tol(1e3, 0.01));
    nl.resistor("r2", "MID", "GND", Range::tol(1e3, 0.01));
    let report = analyze(&nl).unwrap();

    range_is(report.get("v(MID)").unwrap(), 4.95, 5.05);
    // The current is 10 V over the sum, 1.98 kohm to 2.02 kohm.
    range_is(report.get("i(r1)").unwrap(), 10.0 / 2020.0, 10.0 / 1980.0);
    // The source delivers what the resistors take.
    range_is(report.get("i(v)").unwrap(), 10.0 / 2020.0, 10.0 / 1980.0);
    assert_eq!(report.combinations, 4);

    let naive_lo = 10.0 * 0.99 / (1.01 + 1.01);
    let naive_hi = 10.0 * 1.01 / (0.99 + 0.99);
    assert!(naive_lo < 4.95 - 0.04 && naive_hi > 5.05 + 0.04);
    let (outside, solved) = check_interior(&nl, &report, 500);
    assert!(outside.is_empty(), "{outside:?}");
    assert_eq!(solved, 500);
}

/// Power in a resistor is `V^2 R1 / (R1 + R2)^2`, which is largest at `R1 = R2`. With `R1` from
/// 500 ohm to 1500 ohm and `R2` at 1000 ohm that is inside the box, and the corners (22.2 mW and
/// 24 mW) miss the 25 mW of the circuit. The interior check says so: this is the limit of a
/// corner enumeration, and the proposal says how to handle it.
#[test]
fn a_quantity_that_is_not_monotone_is_caught_by_the_interior_check() {
    let mut nl = Netlist::new();
    nl.vsource("v", "IN", "GND", Range::point(10.0));
    nl.resistor("r1", "IN", "MID", Range::new(500.0, 1500.0));
    nl.resistor("r2", "MID", "GND", Range::point(1000.0));
    let report = analyze(&nl).unwrap();
    let p = report.get("p(r1)").unwrap();
    close(p.hi, 100.0 * 1500.0 / (2500.0 * 2500.0), 1e-9);
    let (outside, _) = check_interior(&nl, &report, 400);
    assert!(
        outside.iter().any(|(l, _)| l == "p(r1)"),
        "the interior check should find p(r1) above its corners: {outside:?}"
    );
    // The voltage and the current are monotone, and nothing of them is outside.
    assert!(!outside.iter().any(|(l, _)| l == "v(MID)" || l == "i(r1)"));
}

fn pulldown(states: &str) -> Netlist {
    parse(&format!(
        "V vdd VDD GND 3.3+-3%\n\
         L pin PIN VDD GND states={states} roh=0..200 rol=0..100 rz=3.3M\n\
         R pd  PIN GND 10k+-1%\n"
    ))
    .unwrap()
}

/// A pull-down of 10 kohm +/- 1 % on a logic pin of a 3.3 V +/- 3 % rail (3.201 V to 3.399 V),
/// whose high is the rail through 0 to 200 ohm, whose low is the ground through 0 to 100 ohm,
/// and whose input leakage is 1 uA at 3.3 V (3.3 Mohm to either rail). In each state:
///
/// - high: the pin and the resistor divide the rail, so the current is `Vdd / (R + roh)`: from
///   3.201 / (10100 + 200) = 310.8 uA (rail at its least, R at its largest, 200 ohm) to
///   3.399 / 9900 = 343.3 uA (rail at its largest, R at its least, no output resistance);
/// - low: both ends are at ground, and nothing flows;
/// - high impedance: leaking to the ground, nothing flows in R; leaking to the supply, the
///   current is `Vdd / (R + 3.3 Mohm)`: up to 3.399 / (9900 + 3.3e6) = 1.027 uA, and the pin is
///   at most 3.399 * 10100 / (3.3e6 + 10100) = 10.4 mV.
#[test]
fn a_pulldown_on_a_logic_pin_in_each_state() {
    let high = analyze(&pulldown("H")).unwrap();
    range_is(
        high.get("i(pd)").unwrap(),
        3.201 / (10100.0 + 200.0),
        3.399 / 9900.0,
    );
    let low = analyze(&pulldown("L")).unwrap();
    near_zero(low.get("i(pd)").unwrap());
    let z = analyze(&pulldown("Z")).unwrap();
    range_is(z.get("i(pd)").unwrap(), 0.0, 3.399 / (9900.0 + 3.3e6));
    range_is(
        z.get("v(PIN)").unwrap(),
        0.0,
        3.399 * 10_100.0 / (3.3e6 + 10_100.0),
    );
}

/// All three states together are the hull of the three, and the report names the state that
/// made each end: the high end of the current is the rail at its top and the pin high through
/// no resistance, and the low end is nothing flowing.
#[test]
fn the_states_together_are_their_hull_and_the_report_names_the_state() {
    let nl = pulldown("HLZ");
    let report = analyze(&nl).unwrap();
    let i = report.get("i(pd)").unwrap();
    close(i.hi, 3.399 / 9900.0, 1e-6);
    near_zero(Range::point(i.lo));
    let hi = report.produced_by("i(pd)", true).unwrap();
    assert!(hi.contains("pin: high through 0ohm"), "{hi}");
    assert!(
        hi.contains("pd: 9.9kohm") && hi.contains("vdd: 3.399V"),
        "{hi}"
    );
    // The rail's 2 corners, the pin's 6 (high 2, low 2, Z 2) and the resistor's 2.
    assert_eq!(report.combinations, 24);
    // The power in the resistor is the largest with the pin high at no resistance.
    let p = report.get("p(pd)").unwrap();
    close(p.hi, 3.399 * 3.399 / 9900.0, 1e-6);
}

/// A source of 9 V to 16 V, a fuse of 32 mohm, a diode of 0 V to 0.8 V and a 10 ohm load.
/// The load sees `Vin - 0.032 I - Vf`, and `I = V / 10`. At 16 V and `Vf = 0`: the load is
/// `16 * 10 / 10.032 = 15.9490 V`. At 9 V and `Vf = 0.8` the load current is `I = (9 - 0.8) / 10.032`
/// = 0.81738 A, so 8.1738 V. The same circuit with the source at -14 V blocks: the diode is off,
/// nothing flows, the load is at 0 V and the diode holds 14 V.
#[test]
fn a_fuse_and_a_diode_drop_in_a_chain_and_the_diode_blocks_a_reversed_supply() {
    let build = |source: Range| {
        let mut nl = Netlist::new();
        nl.vsource("batt", "VBATT", "GND", source);
        nl.resistor("fuse", "VBATT", "AFTER_FUSE", Range::point(0.032));
        nl.diode("d", "AFTER_FUSE", "SAFE", Range::new(0.0, 0.8));
        nl.resistor("load", "SAFE", "GND", Range::point(10.0));
        nl
    };
    let report = analyze(&build(Range::new(9.0, 16.0))).unwrap();
    let load = report.get("v(SAFE)").unwrap();
    close(load.hi, 16.0 * 10.0 / 10.032, 1e-9);
    close(load.lo, (9.0 - 0.8) * 10.0 / 10.032, 1e-9);
    let i = report.get("i(fuse)").unwrap();
    close(i.lo, (9.0 - 0.8) / 10.032, 1e-9);
    close(i.hi, 16.0 / 10.032, 1e-9);
    // The fuse's drop and power.
    close(
        report.get("p(fuse)").unwrap().hi,
        0.032 * (16.0 / 10.032f64).powi(2),
        1e-9,
    );
    // The diode's power is the drop times the current.
    close(
        report.get("p(d)").unwrap().hi,
        0.8 * (16.0 - 0.8) / 10.032,
        1e-9,
    );
    // Which supply voltage made the largest current, and which diode drop.
    let by = report.produced_by("i(fuse)", true).unwrap();
    assert!(
        by.contains("batt: 16V") && by.contains("conducting at 0V"),
        "{by}"
    );

    let reversed = analyze(&build(Range::new(-14.0, -14.0))).unwrap();
    near_zero(reversed.get("v(SAFE)").unwrap());
    near_zero(reversed.get("i(d)").unwrap());
    assert!(
        reversed.inconsistent > 0,
        "the conducting states must be refused"
    );
}

/// A 12 V source, 0.5 ohm in series, a regulator (5 V out, 90 % efficient) and a 1 A load at its
/// output. The regulator draws `I = Pout / (eff * Vin)` with `Vin = 12 - 0.5 I`:
/// `0.9 I (12 - 0.5 I) = 5`, so `0.45 I^2 - 10.8 I + 5 = 0`, `I = 0.47226 A`, `Vin = 11.7639 V`.
/// Without the drop, `I` would be 5 / (0.9 * 12) = 0.46296 A.
#[test]
fn a_regulator_draws_its_power_through_the_drops_before_it() {
    let nl = parse(
        "V  src  VIN0 GND 12\n\
         R  r    VIN0 VIN 0.5\n\
         REG reg VIN VOUT GND vout=5 eff=0.9\n\
         I  load VOUT GND 1\n",
    )
    .unwrap();
    let report = analyze(&nl).unwrap();
    let i = (10.8 - (10.8f64 * 10.8 - 4.0 * 0.45 * 5.0).sqrt()) / (2.0 * 0.45);
    close(i, 0.47226, 1e-4);
    range_is(report.get("i(reg.in)").unwrap(), i, i);
    range_is(
        report.get("v(VIN)").unwrap(),
        12.0 - 0.5 * i,
        12.0 - 0.5 * i,
    );
    range_is(report.get("i(r)").unwrap(), i, i);
    // The loss is Vin * Iin - Pout.
    let loss = (12.0 - 0.5 * i) * i - 5.0;
    range_is(report.get("p(reg)").unwrap(), loss, loss);
    assert!(i > 5.0 / (0.9 * 12.0) + 0.009);
    // The fixed point settles in a few steps.
    assert!(report.max_iterations < 20, "{}", report.max_iterations);
}

/// Two regulators in a row: a 5 V stage (90 %) from the source through 0.5 ohm, and a 3.3 V stage
/// (85 %) on its output with a 0.5 A load. The second draws `3.3 * 0.5 / (0.85 * 5)` = 0.38824 A
/// at 5 V, which is the load of the first.
#[test]
fn a_regulator_behind_a_regulator_is_the_load_of_the_first() {
    let nl = parse(
        "V  src  VIN0 GND 12\n\
         R  r    VIN0 VIN 0.5\n\
         REG a   VIN V5 GND vout=5 eff=0.9\n\
         REG b   V5 V33 GND vout=3.3 eff=0.85\n\
         I  load V33 GND 0.5\n",
    )
    .unwrap();
    let report = analyze(&nl).unwrap();
    let ib = 3.3 * 0.5 / (0.85 * 5.0);
    range_is(report.get("i(b.in)").unwrap(), ib, ib);
    let pout = 5.0 * ib;
    // 0.9 I (12 - 0.5 I) = pout.
    let ia = (10.8 - (10.8f64 * 10.8 - 4.0 * 0.45 * pout).sqrt()) / (2.0 * 0.45);
    range_is(report.get("i(a.in)").unwrap(), ia, ia);
}

/// The source cannot give it: 12 V through 10 ohm gives at most `12^2 / (4 * 10) = 3.6 W`, and
/// the regulator asks for `5 / 0.9 = 5.56 W`. The analysis has no operating point and says so.
#[test]
fn a_regulator_asking_for_more_than_the_source_can_give_collapses() {
    let nl = parse(
        "V  src  VIN0 GND 12\n\
         R  r    VIN0 VIN 10\n\
         REG reg VIN VOUT GND vout=5 eff=0.9\n\
         I  load VOUT GND 1\n",
    )
    .unwrap();
    match analyze(&nl) {
        Err(Error::NoOperatingPoint(why)) => {
            assert!(
                why.contains("cannot give") || why.contains("settle"),
                "{why}"
            )
        }
        other => panic!(
            "expected a collapse, got {:?}",
            other.map(|r| r.combinations)
        ),
    }
}

/// A linear regulator draws what it supplies plus its quiescent current, whatever its input.
#[test]
fn a_linear_regulator_passes_its_output_current_and_its_quiescent_current() {
    let nl = parse(
        "V  src  VIN GND 4.5..5.5\n\
         REG ldo VIN VOUT GND vout=3.3+-2% linear iq=50u\n\
         I  load VOUT GND 0..0.3\n",
    )
    .unwrap();
    let report = analyze(&nl).unwrap();
    range_is(report.get("i(ldo.in)").unwrap(), 50e-6, 0.3 + 50e-6);
    // What it dissipates: (Vin - Vout) * I + Vin * Iq, at most (5.5 - 3.234) * 0.3 + 5.5 * 50 uA.
    close(
        report.get("p(ldo)").unwrap().hi,
        (5.5 - 3.3 * 0.98) * 0.3 + 5.5 * 50e-6,
        1e-6,
    );
}

/// An N-channel switch: 2 ohm on, threshold 1 V to 2.5 V, a 100 ohm load from 5 V. With the gate
/// driven low (to the ground through 0 to 100 ohm, with the 10 kohm pull-down in parallel) the
/// channel is off whatever the threshold: nothing flows and the drain is at 5 V. With the gate high
/// (the 3.234 V to 3.333 V rail through 0 to 200 ohm, against the pull-down) the gate is at least
/// 3.234 * 9900 / 10100 = 3.17 V, above 2.5 V: it is on, and `5 / 102` = 49 mA flows.
#[test]
fn a_mosfet_is_off_for_a_low_gate_and_on_for_a_high_one() {
    let net = |states: &str| {
        parse(&format!(
            "V rail VCC GND 5\n\
             V gv   GV  GND 3.234..3.333\n\
             R load VCC D 100\n\
             M q    G D GND n vth=1..2.5 ron=2\n\
             L drv  G GV GND states={states} roh=0..200 rol=0..100 rz=3.3M\n\
             R pd   G GND 10k+-1%\n"
        ))
        .unwrap()
    };
    let low = analyze(&net("L")).unwrap();
    range_is(low.get("i(q)").unwrap(), 0.0, 0.0);
    range_is(low.get("v(D)").unwrap(), 5.0, 5.0);
    assert!(
        low.inconsistent > 0,
        "the on state must be refused for a low gate"
    );

    let high = analyze(&net("H")).unwrap();
    range_is(high.get("i(q)").unwrap(), 5.0 / 102.0, 5.0 / 102.0);
    let vg = high.get("v(G)").unwrap();
    assert!(vg.lo > 3.16 && vg.lo < 3.18, "{vg}");
    close(
        high.get("p(q)").unwrap().hi,
        (5.0f64 / 102.0).powi(2) * 2.0,
        1e-9,
    );
    assert!(
        high.inconsistent > 0,
        "the off state must be refused for a high gate"
    );

    // Left high impedance, the pull-down holds the gate low and the switch is off.
    let z = analyze(&net("Z")).unwrap();
    range_is(z.get("i(q)").unwrap(), 0.0, 0.0);
}

/// A gate-source voltage inside the threshold's range could be either: both states are kept.
#[test]
fn a_gate_inside_the_threshold_range_keeps_both_states() {
    let nl = parse(
        "V rail VCC GND 5\n\
         V gate G GND 1.8\n\
         R load VCC D 100\n\
         M q    G D GND n vth=1..2.5 ron=2\n",
    )
    .unwrap();
    let report = analyze(&nl).unwrap();
    range_is(report.get("i(q)").unwrap(), 0.0, 5.0 / 102.0);
}

/// An open-drain bus: a 4.7 kohm +/- 5 % pull-up to 3.3 V +/- 3 % and three devices that pull
/// low (to the ground through 0 to 25 ohm) or leave the line (input leakage 1 uA at 3.3 V: 3.3 Mohm
/// to a rail). With all of them off, the line is at the supply (3.399 V) when they leak to it and
/// at `Vcc * 1.1M / (1.1M + R)` when all three leak to the ground (the three 3.3 Mohm in parallel
/// are 1.1 Mohm): 3.201 * 1.1e6 / (1.1e6 + 4935) = 3.1865 V at the least. That is the line's
/// high, and it comes from the pull-up, the rail and the leakage, as ADR 0075 says; here it is
/// a node voltage and not a rule. With one device pulling low through 25 ohm the line is at most
/// 3.399 * 25 / (4465 + 25) = 18.9 mV, and the pull-up carries 3.399 / 4465 = 761 uA when
/// the three pull low through no resistance.
#[test]
fn an_open_drain_line_gets_its_high_from_its_pullup() {
    let mut nl = Netlist::new();
    nl.vsource("vcc", "VCC", "GND", Range::tol(3.3, 0.03));
    nl.resistor("pullup", "VCC", "SDA", Range::tol(4700.0, 0.05));
    for n in 0..3 {
        let (node, vdd, gnd) = (nl.node("SDA"), nl.node("VCC"), nl.node("GND"));
        nl.add(
            &format!("dev{n}"),
            Kind::Logic(Logic {
                node,
                vdd,
                gnd,
                states: vec![LogicState::Low, LogicState::Z],
                roh: Range::point(0.0),
                rol: Range::new(0.0, 25.0),
                rz: 3.3e6,
            }),
        );
    }
    let report = analyze(&nl).unwrap();
    let v = report.get("v(SDA)").unwrap();
    close(v.hi, 3.3 * 1.03, 1e-6);
    let produced = report.produced_by("v(SDA)", true).unwrap();
    assert!(produced.contains("high impedance"), "{produced}");
    assert!(v.lo.abs() < 1e-6, "{v}");
    let i = report.get("i(pullup)").unwrap();
    close(i.hi, 3.3 * 1.03 / (4700.0 * 0.95), 1e-4);

    // Only the bus's high: every device off, leaving the pull-up and the leakage.
    let mut off = nl.clone();
    for e in off.elements.iter_mut() {
        if let Kind::Logic(l) = &mut e.kind {
            l.states = vec![LogicState::Z];
        }
    }
    let high = analyze(&off).unwrap().get("v(SDA)").unwrap();
    close(high.lo, 3.3 * 0.97 * 1.1e6 / (1.1e6 + 4700.0 * 1.05), 1e-6);
    close(high.hi, 3.3 * 1.03, 1e-6);
    // Only one device low, through its 25 ohm: the line's low, against the pull-up's least.
    let mut one = nl.clone();
    for (n, e) in one.elements.iter_mut().enumerate() {
        if let Kind::Logic(l) = &mut e.kind {
            l.states = if n == 2 {
                vec![LogicState::Low]
            } else {
                vec![LogicState::Z]
            };
        }
    }
    let low = analyze(&one).unwrap().get("v(SDA)").unwrap();
    assert!(low.hi < 0.0189 + 1e-4 && low.hi > 0.017, "{low}");
}

/// The supercap's balancing divider with the cells' leakage: 5.0 V to 5.5 V across 43 kohm +/-
/// 1 % twice and 10 ohm, each cell leaking 0 to 6 uA (`l1` across the top cell, `l2` across the
/// bottom one). Kirchhoff by hand: the top resistor carries `a`, the bottom one `a + l1 - l2`, the
/// 10 ohm `a + l1`, so `V = a R27 + (a + l1 - l2) R26 + (a + l1) 10` and the middle is `V - a R27`.
/// The test works that out for all 32 corners and compares the hull with the solver's.
#[test]
fn a_balancing_divider_with_cell_leakage() {
    let nl = parse(
        "V  rail  TOP GND 5..5.5\n\
         R  r27   TOP MID 43k+-1%\n\
         R  r26   MID BOT 43k+-1%\n\
         R  r28   BOT GND 10\n\
         I  leak1 TOP MID 0..6u\n\
         I  leak2 MID BOT 0..6u\n",
    )
    .unwrap();
    let report = analyze(&nl).unwrap();

    let (mut mid, mut top_current) = (Range::EMPTY, Range::EMPTY);
    for v in [5.0, 5.5] {
        for r27 in [42_570.0, 43_430.0] {
            for r26 in [42_570.0, 43_430.0] {
                for l1 in [0.0, 6e-6] {
                    for l2 in [0.0, 6e-6] {
                        let a = (v - (l1 - l2) * r26 - l1 * 10.0) / (r27 + r26 + 10.0);
                        mid.include(v - a * r27);
                        top_current.include(a);
                    }
                }
            }
        }
    }
    range_is(report.get("v(MID)").unwrap(), mid.lo, mid.hi);
    range_is(
        report.get("i(r27)").unwrap(),
        top_current.lo,
        top_current.hi,
    );
    // The numbers the comment promises: about 55 uA to 68 uA, and the middle within 0.3 V of
    // half the supply.
    assert!(
        top_current.lo > 54e-6 && top_current.hi < 68e-6,
        "{top_current}"
    );
    assert!(mid.hi < 2.75 + 0.3 && mid.lo > 2.5 - 0.3, "{mid}");
    let (outside, _) = check_interior(&nl, &report, 400);
    assert!(outside.is_empty(), "{outside:?}");
}

#[test]
fn too_many_combinations_is_an_error_that_says_what_to_do() {
    let mut nl = Netlist::new();
    nl.vsource("v", "N0", "GND", Range::point(1.0));
    for k in 0..30 {
        nl.resistor(
            &format!("r{k}"),
            &format!("N{k}"),
            &format!("N{}", k + 1),
            Range::tol(1000.0, 0.01),
        );
    }
    nl.resistor("end", "N30", "GND", Range::tol(1000.0, 0.01));
    match analyze(&nl) {
        Err(e @ Error::TooManyCombinations { .. }) => {
            assert!(e.to_string().contains("cut the island"), "{e}")
        }
        other => panic!("{:?}", other.map(|r| r.combinations)),
    }
}

#[test]
fn a_node_nothing_drives_is_at_zero_and_not_singular() {
    let nl = parse(
        "V v  A GND 5\n\
         R r  A B 1k\n\
         R f  C D 1k\n",
    )
    .unwrap();
    let report = analyze(&nl).unwrap();
    // B is joined to A through r and goes nowhere: it is at A's voltage.
    range_is(report.get("v(B)").unwrap(), 5.0, 5.0);
    near_zero(report.get("i(r)").unwrap());
    // C and D float together and are at 0.
    near_zero(report.get("v(C)").unwrap());
}
