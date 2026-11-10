//! The figures of the parts on an I2C bus, derived by the checker for the pull-up (ADR 0102): the
//! bus capacitance as the sum of the `.capacitance` of each `I2c` interface on the bus and the
//! board's estimate of the traces (`.trace_capacitance`), and the bound of the pull-up's
//! resistance from the sink current of each part. Every part and value here is invented; the
//! expected numbers are worked out by hand and not read from the solver
//! (`Cb = sum of pins + traces`, `tr = 0.8473 * Rp * Cb`, `Rp >= (Vdd - VOL) / IOL`, UM10204
//! Rev. 7.0, section 7.1).
//!
//! The parts: a controller `Zc` (5 pF, 6 mA at 0.4 V), a target `Zt` (8 pF, 3 mA at 0.4 V), a
//! target `Zr` with ranges (7 pF, 2.8 mA to 3 mA at 0.3 V to 0.4 V) and a target `Zn` that states
//! no figure. The supply is 3.3 V +/- 5 %, so at most 3.465 V; the pull-up resistance is stated as
//! 4.7 kohm +/- 5 % (4.935 kohm at most) where the rise time is judged.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::bus::{I2c, I2cPullups, I2cRole}
import std::sources::Rail

/// A controller: 5 pF, 6 mA at 0.4 V.
part Zc {
    .designator &= "U"
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Controller
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    .i2c.capacitance &= 5pF
    .i2c.sink_current &= 6mA
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "VSS" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  bidirectional "SCL" "4"
    }
}

/// A target: 8 pF, 3 mA at 0.4 V.
part Zt {
    .designator &= "U"
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Target
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    .i2c.capacitance &= 8pF
    .i2c.sink_current &= 3mA
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  input         "SCL" "4"
    }
}

/// A target whose datasheet gives ranges: 7 pF, 2.8 mA to 3 mA at 0.3 V to 0.4 V.
part Zr {
    .designator &= "U"
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Target
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.3V..0.4V
    .i2c.capacitance &= 7pF
    .i2c.sink_current &= 2.8mA..3mA
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  input         "SCL" "4"
    }
}

/// A target whose datasheet states no capacitance and no sink current.
part Zn {
    .designator &= "U"
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Target
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  input         "SCL" "4"
    }
}
"#;

/// A board of the controller and the targets named in `others` (`.t1`, `.t2`, ...), on a supply
/// of `rail`, the pull-ups' `resistance` and `extra` as statements of the board.
fn board(others: &[&str], rail: &str, resistance: &str, extra: &str) -> String {
    let mut parts = String::new();
    let mut joins = String::new();
    for (i, t) in others.iter().enumerate() {
        let n = i + 1;
        parts.push_str(&format!("    .t{n}: {t}\n"));
        joins.push_str(&format!("    .s3 ~ .t{n}.vdd\n    .c.i2c ~ .t{n}.i2c\n"));
    }
    format!(
        r#"{PARTS}
board Bus {{
    .s3: Rail {{
        .voltage &= {rail}
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }}
    .c: Zc
{parts}    .pullups: I2cPullups {{
        .resistance &= {resistance}
    }}
    .s3 ~ .c.vdd
    .s3 ~ .pullups.supply
{joins}    .c.i2c ~ .pullups.bus
{extra}}}
"#
    )
}

const RESISTOR: &str = "mpn,resistance,max_power,package,source
R4K7,4.7kohm +/- 1%,100mW,0402,invented
R1K2,1.2kohm +/- 1%,100mW,0402,invented
R1K1,1.1kohm +/- 1%,100mW,0402,invented
R800,800ohm +/- 1%,100mW,0402,invented
R400,400ohm +/- 1%,100mW,0402,invented
";

fn run_on(others: &[&str], rail: &str, resistance: &str, extra: &str) -> Solution {
    let c = check(
        "bus.nsm",
        &board(others, rail, resistance, extra),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    let mut catalogs = Catalogs::empty();
    catalogs
        .add_file("resistor", "catalogs/resistor.csv", RESISTOR)
        .unwrap();
    solve(&c.boards[0], &catalogs)
}

/// The controller and `others` with the traces estimated at 20 pF to 40 pF.
fn run(others: &[&str], extra: &str) -> Solution {
    run_on(
        others,
        "3.3V +/- 5%",
        "4.7kohm +/- 5%",
        &format!("    .pullups.trace_capacitance &= 20pF..40pF\n{extra}"),
    )
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> Option<(f64, f64)> {
    match s.model.domain_at(path)? {
        Domain::Quantity(q) => Some((q.lo(), q.hi())),
        _ => None,
    }
}

/// The relations the solver could not evaluate, as the build lists them.
fn not_evaluated(s: &Solution) -> Vec<String> {
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

/// The notes that name a part or the board (the lines the build prints as not evaluated).
fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated.iter().map(|n| n.message.clone()).collect()
}

/// The notes about a word (`capacitance` or `sink current`).
fn notes_on(s: &Solution, word: &str) -> Vec<String> {
    notes(s).into_iter().filter(|n| n.contains(word)).collect()
}

fn near(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-4 * want.abs(),
        "got {got}, want {want}"
    );
}

/// The rise time of `r` ohms and `c` farads, by hand.
fn rise(r: f64, c: f64) -> f64 {
    0.8473 * r * c
}

/// The least resistance of `(vdd - vol) / iol`, by hand.
fn least(vdd: f64, vol: f64, iol: f64) -> f64 {
    (vdd - vol) / iol
}

#[test]
fn the_hand_figures_are_the_ones_the_header_says() {
    // 5 pF + 8 pF + 20..40 pF is 33..53 pF; 0.8473 * 4935 ohm * 53 pF is 221.6 ns.
    near(rise(4935.0, 53e-12), 221.62e-9);
    near(least(3.465, 0.4, 0.003), 1021.67);
    near(least(3.465, 0.3, 0.0028), 1130.357);
}

/// The sum is the pins of the two parts and the traces' range: 33 pF to 53 pF, and the rise time
/// at the corner (4.935 kohm, 53 pF) is 221.6 ns, with the limits 221.7 ns and 221.5 ns either
/// side. The board states the traces only.
#[test]
fn the_derived_sum_is_the_hand_calculation() {
    let s = run(&["Zt"], "    .pullups.rise_time <= 221.7ns\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
    assert!(notes(&s).is_empty(), "{:#?}", notes(&s));
    let cap = range(&s, "pullups.bus_capacitance").unwrap();
    near(cap.0, 33e-12);
    near(cap.1, 53e-12);
    near(
        range(&s, "pullups.rise_time").unwrap().1,
        rise(4935.0, 53e-12),
    );
    let s = run(&["Zt"], "    .pullups.rise_time <= 221.5ns\n");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("pullups.rise_time") && m[0].contains("221.5ns"),
        "{}",
        m[0]
    );
}

/// Three parts on the bus: 5 + 8 + 7 pF and the traces, 40 pF to 60 pF; the sum is every
/// interface's, wherever it is on the bus.
#[test]
fn every_part_on_the_bus_is_in_the_sum() {
    let s = run(&["Zt", "Zr"], "");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let cap = range(&s, "pullups.bus_capacitance").unwrap();
    near(cap.0, 40e-12);
    near(cap.1, 60e-12);
}

/// A part that states no capacitance leaves the sum unstated, and the build names it. The bound on
/// the rise time is listed as not evaluated and never passes, although the pins that are stated
/// and the traces (5 pF and 40 pF) would pass it.
#[test]
fn a_part_with_no_capacitance_is_named() {
    let s = run(&["Zn"], "    .pullups.rise_time <= 300ns\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(
        not_evaluated(&s)
            .iter()
            .any(|r| r == "pullups.rise_time <= 300ns"),
        "{:#?}",
        not_evaluated(&s)
    );
    assert!(range(&s, "pullups.bus_capacitance").is_none());
    assert_eq!(
        notes_on(&s, "capacitance"),
        [
            "not evaluated: the capacitance of the bus of `pullups` is not derived, so the bounds on it and on its rise time are not checked: `t1.i2c` states no `.capacitance`"
        ]
    );
    // With the part that states its capacitance, the same bound is judged and passes.
    let s = run(&["Zt"], "    .pullups.rise_time <= 300ns\n");
    assert!(messages(&s).is_empty() && notes(&s).is_empty());
}

/// Without the board's estimate of the traces nothing is derived: the pins alone would pass a bound
/// the traces break. The bound is listed, and the note says what to state. A board that states
/// neither a bound nor an estimate is told nothing.
#[test]
fn a_board_with_no_estimate_of_the_traces_is_told_what_to_state() {
    let s = run_on(
        &["Zt"],
        "3.3V +/- 5%",
        "4.7kohm +/- 5%",
        "    .pullups.rise_time <= 300ns\n",
    );
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(range(&s, "pullups.bus_capacitance").is_none());
    assert_eq!(
        notes(&s),
        [
            "not evaluated: the capacitance of the bus of `pullups` is not derived, so the bounds on it and on its rise time are not checked: the board states no estimate of the traces, `.pullups.trace_capacitance &= <range>`"
        ]
    );
    let s = run_on(&["Zt"], "3.3V +/- 5%", "4.7kohm +/- 5%", "");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty() && notes(&s).is_empty());
}

/// A board that states the capacitance itself is judged on its figure, as before, and the sum is
/// not derived for it (an equation against a narrower range the board states would read as a
/// figure that cannot be proved, not as a narrowing): 40 pF to 45 pF gives the rise time at 45 pF,
/// and 60 pF to 120 pF, outside the parts' 33 pF to 53 pF, fails the bound on that figure.
#[test]
fn a_figure_the_board_states_replaces_the_derived_one() {
    let s = run(
        &["Zt"],
        "    .pullups.bus_capacitance &= 40pF..45pF\n    .pullups.rise_time <= 300ns\n",
    );
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let cap = range(&s, "pullups.bus_capacitance").unwrap();
    near(cap.0, 40e-12);
    near(cap.1, 45e-12);
    near(
        range(&s, "pullups.rise_time").unwrap().1,
        rise(4935.0, 45e-12),
    );
    let s = run(
        &["Zt"],
        "    .pullups.bus_capacitance &= 60pF..120pF\n    .pullups.rise_time <= 300ns\n",
    );
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].starts_with("pullups.rise_time"), "{}", m[0]);
}

/// A board that writes the sum itself, as before, is judged on it, and an estimate of the traces
/// the board does not state changes nothing.
#[test]
fn a_sum_the_board_writes_is_as_it_was() {
    let s = run_on(
        &["Zt"],
        "3.3V +/- 5%",
        "4.7kohm +/- 5%",
        "    .pullups.bus_capacitance &= .c.i2c.capacitance + .t1.i2c.capacitance + (20pF..40pF)\n    .pullups.rise_time <= 300ns\n",
    );
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    near(
        range(&s, "pullups.rise_time").unwrap().1,
        rise(4935.0, 53e-12),
    );
}

/// A pull-up with no `I2c` interface of a part on its bus has nothing to sum, even with the
/// estimate of the traces: nothing is derived, and (no bound asked) nothing is said.
#[test]
fn a_bus_with_no_interface_has_nothing_derived() {
    let source = format!(
        r#"{PARTS}
board Bus {{
    .s3: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }}
    .pullups: I2cPullups {{
        .resistance &= 4.7kohm +/- 5%
        .trace_capacitance &= 20pF..40pF
    }}
    .s3 ~ .pullups.supply
}}
"#
    );
    let c = check("bus.nsm", &source, &Loader::std());
    let mut catalogs = Catalogs::empty();
    catalogs
        .add_file("resistor", "catalogs/resistor.csv", RESISTOR)
        .unwrap();
    if c.is_ok() {
        let s = solve(&c.boards[0], &catalogs);
        assert!(range(&s, "pullups.bus_capacitance").is_none());
        assert!(notes(&s).is_empty(), "{:#?}", notes(&s));
    }
}

/// The sink current of each part gives a bound on the pull-up: 3 mA at 0.4 V needs 1021.67 ohm at
/// the greatest supply; 800 ohm fails and the message names the part's own figures. The board
/// states no sink current.
#[test]
fn the_pullup_is_held_to_the_sink_current_of_every_part() {
    let s = run_on(&["Zt"], "3.3V +/- 5%", "800ohm +/- 5%", "");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].starts_with(
            "pullups.resistance must always be at least ((pullups.supply.voltage - t1.i2c.levels.vol) / t1.i2c.sink_current)"
        ),
        "{}",
        m[0]
    );
    let s = run_on(&["Zt"], "3.3V +/- 5%", "4.7kohm +/- 5%", "");
    assert!(messages(&s).is_empty() && not_evaluated(&s).is_empty());
}

/// Two parts on the bus: the controller's 6 mA asks 510.8 ohm and the target's 3 mA asks 1021.7
/// ohm, and the greater stands, whichever part it is. 800 ohm +/- 5 % (760 ohm at the low end)
/// fails for the target alone, not for the controller.
#[test]
fn the_part_that_asks_the_most_stands() {
    near(least(3.465, 0.4, 0.006), 510.83);
    let s = run_on(&["Zt"], "3.3V +/- 5%", "800ohm +/- 5%", "");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("t1.i2c.sink_current"), "{}", m[0]);
    assert!(!m[0].contains(" c.i2c.sink_current"), "{}", m[0]);
}

/// The worst case over ranges is the corner of the part's own figures: the supply at 3.465 V, the
/// low level at 0.3 V and the current at 2.8 mA, 1130.36 ohm. 1131 ohm passes, 1130 ohm fails.
#[test]
fn the_worst_case_over_ranges_is_the_corner() {
    let at = |r: &str| run_on(&["Zr"], "3.3V +/- 5%", r, "");
    let s = at("1131ohm..1500ohm");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
    let s = at("1130ohm..1500ohm");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("t1.i2c.sink_current"), "{}", m[0]);
}

/// A part that states no sink current beside parts that do is named, and the bound is made for
/// the others: with no part that states one, nothing is said.
#[test]
fn a_part_with_no_sink_current_is_named() {
    let s = run_on(&["Zn"], "3.3V +/- 5%", "4.7kohm +/- 5%", "");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert_eq!(
        notes_on(&s, "sink current"),
        [
            "not evaluated: the least pull-up of the bus of `pullups` from the sink current of `t1.i2c`: it states no `.sink_current`, so the bound is made for the other parts of the bus only"
        ]
    );
    // The part that does state one is still held to it.
    let s = run_on(&["Zn"], "3.3V +/- 5%", "400ohm +/- 1%", "");
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
}

/// A board that states the figures itself is judged on them as before: the derivation is not made
/// for it, so a statement says one fault once.
#[test]
fn a_board_that_states_the_figure_is_judged_on_it() {
    let s = run_on(
        &["Zt"],
        "3.3V +/- 5%",
        "800ohm +/- 5%",
        "    .pullups.sink_current &= 3mA\n    .pullups.low_level &= 0.4V\n",
    );
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("pullups.low_level"), "{}", m[0]);
}

/// The messages are byte-stable: the same board gives the same lines twice, in the same order.
#[test]
fn the_messages_are_stable() {
    let extra = "    .pullups.rise_time <= 300ns\n";
    let a = notes(&run(&["Zn", "Zt"], extra));
    let b = notes(&run(&["Zn", "Zt"], extra));
    assert_eq!(a, b);
    assert_eq!(a.len(), 2, "{a:#?}");
    assert!(a.iter().all(|n| n.contains("`t1.i2c`")), "{a:#?}");
}
