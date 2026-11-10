//! The sink-current lower bound of an I2C pull-up, a closed-form check in std
//! (`.sink_current` and `.low_level` of `I2cPullups`, `.sink_current` of `I2c`,
//! `docs/research/closed-form-checks.md`, check 5). Every part and value here is invented; the
//! expected numbers are worked out by hand from `Rp >= (Vdd - VOL) / IOL` and not read from the
//! solver. The relation is the I2C-bus specification's (UM10204 Rev. 7.0, section 7.1).
//!
//! The board: a controller (sinks 6 mA at 0.4 V) and a target (3 mA) on 3.3 V +/- 5 %. The
//! weakest driver is the target's, so the board states 3 mA and 0.4 V for the pull-up.
//!
//! The worst case is the supply at its greatest and the weakest driver: `3.3 V * 1.05 = 3.465 V`,
//! so `Rp >= (3.465 V - 0.4 V) / 3 mA = 1021.67 ohm`, and the resistance the board states is
//! judged at its low end.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::bus::{I2c, I2cPullups, I2cRole}
import std::sources::Rail

/// A controller whose driver sinks 6 mA at 0.4 V.
part Zc {
    .designator &= "U"
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Controller
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    .i2c.sink_current &= 6mA
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "VSS" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  bidirectional "SCL" "4"
    }
}

/// A target whose driver sinks 3 mA at 0.4 V.
part Zt {
    .designator &= "U"
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Target
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    .i2c.sink_current &= 3mA
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  input         "SCL" "4"
    }
}

/// A target whose datasheet states no sink current.
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

/// A board of the controller and `second` on a supply of `rail`, with the pull-ups' resistance
/// `resistance`, and `extra` as statements of the board.
fn board(second: &str, rail: &str, resistance: &str, extra: &str) -> String {
    format!(
        r#"{PARTS}
board Bus {{
    .s3: Rail {{
        .voltage &= {rail}
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }}
    .c: Zc
    .t: {second}
    .pullups: I2cPullups {{
        .resistance &= {resistance}
    }}
    .s3 ~ .c.vdd
    .s3 ~ .t.vdd
    .s3 ~ .pullups.supply
    .c.i2c ~ .t.i2c
    .c.i2c ~ .pullups.bus
{extra}}}
"#
    )
}

/// One row for each resistance the tests ask for, all +/- 1 %.
const RESISTOR: &str = "mpn,resistance,max_power,package,source
R4K7,4.7kohm +/- 1%,100mW,0402,invented
R1K2,1.2kohm +/- 1%,100mW,0402,invented
R1K1,1.1kohm +/- 1%,100mW,0402,invented
R1K,1kohm +/- 1%,100mW,0402,invented
R900,900ohm +/- 1%,100mW,0402,invented
R800,800ohm +/- 1%,100mW,0402,invented
R700,700ohm +/- 1%,100mW,0402,invented
";

fn run_on(second: &str, rail: &str, resistance: &str, extra: &str) -> Solution {
    let c = check(
        "bus.nsm",
        &board(second, rail, resistance, extra),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    let mut catalogs = Catalogs::empty();
    catalogs
        .add_file("resistor", "catalogs/resistor.csv", RESISTOR)
        .unwrap();
    solve(&c.boards[0], &catalogs)
}

fn run_with(second: &str, resistance: &str, extra: &str) -> Solution {
    run_on(second, "3.3V +/- 5%", resistance, extra)
}

/// The weakest driver's figures, stated by the board: 3 mA at 0.4 V, and `more` statements.
fn run(resistance: &str, more: &str) -> Solution {
    run_with(
        "Zt",
        resistance,
        &format!("    .pullups.sink_current &= 3mA\n    .pullups.low_level &= 0.4V\n{more}"),
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

fn near(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-4 * want.abs(),
        "got {got}, want {want}"
    );
}

/// The least pull-up resistance by hand: the supply's greatest voltage less the low level, over
/// the sink current.
fn least(vdd: f64, vol: f64, iol: f64) -> f64 {
    (vdd - vol) / iol
}

#[test]
fn the_hand_figures_are_the_ones_the_header_says() {
    near(least(3.3 * 1.05, 0.4, 0.003), 1021.67);
    // The weakest of the two drivers sets the figure: the controller's 6 mA gives half.
    near(least(3.465, 0.4, 0.006), 510.83);
}

/// A resistance above the least passes, with nothing listed.
#[test]
fn a_pullup_that_the_weakest_driver_can_pull_down_passes() {
    let s = run("4.7kohm +/- 5%", "");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
    let r = range(&s, "pullups.resistance").unwrap();
    assert!(r.0 > least(3.465, 0.4, 0.003), "{r:?}");
}

/// A resistance below it fails with the real message: the resistance's whole range, and the
/// least it must be, whose top end is the hand figure 1021.67 ohm (the supply's 3.465 V).
#[test]
fn a_pullup_the_weakest_driver_cannot_pull_down_fails_and_names_the_numbers() {
    let s = run("800ohm +/- 5%", "");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert_eq!(
        m[0],
        "pullups.resistance must always be at least ((pullups.supply.voltage - pullups.low_level) / pullups.sink_current), but 760\u{3a9}..840\u{3a9} is not always above 911.6\u{3a9}..1.022k\u{3a9}",
        "{}",
        m[0]
    );
}

/// The worst case over ranges is exact: the supply at its greatest (3.465 V), the low level at its
/// least (0.3 V) and the sink current at its least (2.8 mA), `(3.465 - 0.3) / 0.0028 = 1130.36`
/// ohm. A resistance whose low end is just above passes, just below fails.
#[test]
fn the_worst_case_over_ranges_is_the_corner() {
    let more = "    .pullups.sink_current &= 2.8mA..3mA\n    .pullups.low_level &= 0.3V..0.4V\n";
    near(least(3.3 * 1.05, 0.3, 0.0028), 1130.357);
    // The board states the ranges itself (`run` states 3 mA and 0.4 V).
    let direct = |r: &str| run_with("Zt", r, more);
    let s = direct("1131ohm..1500ohm");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
    let s = direct("1130ohm..1500ohm");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("1.13") && m[0].contains("not always above"),
        "{}",
        m[0]
    );
}

/// The supply at its greatest: a wider rail, 3 V to 3.6 V, asks for `(3.6 - 0.4) / 0.003 =
/// 1066.7` ohm; 1040 ohm, enough on 3.3 V +/- 5 % (1021.7 ohm), is too little.
#[test]
fn the_supply_enters_at_its_greatest() {
    let extra = "    .pullups.sink_current &= 3mA\n    .pullups.low_level &= 0.4V\n";
    near(least(3.6, 0.4, 0.003), 1066.67);
    let s = run_on("Zt", "3.3V +/- 5%", "1040ohm..1500ohm", extra);
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let s = run_on("Zt", "3V..3.6V", "1040ohm..1500ohm", extra);
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
    let s = run_on("Zt", "3V..3.6V", "1067ohm..1500ohm", extra);
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
}

/// A board that states no sink current asks for no bound (`.sink_current` is `optional`, ADR 0074,
/// as `.bus_capacitance` is): nothing is listed, and 800 ohm is not judged. A board that states the
/// current and no low level is listed as not evaluated, and does not pass.
#[test]
fn a_board_with_no_sink_current_asks_for_no_bound_and_one_without_a_level_is_listed() {
    let s = run_with("Zt", "800ohm +/- 5%", "    .pullups.low_level &= 0.4V\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
    assert!(range(&s, "pullups.sink_current").is_none());
    let s = run_with("Zt", "800ohm +/- 5%", "    .pullups.sink_current &= 3mA\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert_eq!(
        not_evaluated(&s),
        [
            "pullups.resistance >= ((pullups.supply.voltage - pullups.low_level) / pullups.sink_current)"
        ]
    );
}

/// A board that takes the figure from a part that states none leaves it without a value: not
/// evaluated, with no pass.
#[test]
fn a_part_that_states_no_sink_current_is_not_evaluated() {
    let s = run_with(
        "Zn",
        "800ohm +/- 5%",
        "    .pullups.sink_current &= .t.i2c.sink_current\n    .pullups.low_level &= .t.i2c.levels.vol\n",
    );
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let n = not_evaluated(&s);
    assert!(
        n.iter()
            .any(|r| r.starts_with("pullups.resistance >=") && r.contains("pullups.sink_current")),
        "{n:#?}"
    );
    assert!(range(&s, "pullups.sink_current").is_none());
    // With the part that states 3 mA, the same statements are judged, and 800 ohm fails.
    let s = run_with(
        "Zt",
        "800ohm +/- 5%",
        "    .pullups.sink_current &= .t.i2c.sink_current\n    .pullups.low_level &= .t.i2c.levels.vol\n",
    );
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
}

/// For a bus of several parts the board states the least figure and holds it to each part: a
/// check `.pullups.sink_current <= .t.i2c.sink_current` per part. A figure above a part's is a
/// failure that names that part. (A check on a part that states no figure is not made and is not
/// listed, since the part's member is `optional`, ADR 0074: the board takes the figure of a part
/// it does not hold to a guard from that part with `&=`, as the test above does.)
#[test]
fn the_figure_is_held_to_every_part() {
    let guards = |c: &str| {
        format!(
            "    .pullups.sink_current &= {c}\n    .pullups.low_level &= 0.4V\n    .pullups.sink_current <= .c.i2c.sink_current\n    .pullups.sink_current <= .t.i2c.sink_current\n    .t.i2c.levels.vol <= .pullups.low_level\n"
        )
    };
    // 3 mA is no more than 6 mA and 3 mA: nothing is said.
    let s = run_with("Zt", "4.7kohm +/- 5%", &guards("3mA"));
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
    // 6 mA is more than the target's 3 mA, so the figure is not the weakest's: the bound would pass
    // at 800 ohm, and the check on the part is what fails, naming it.
    let s = run_with("Zt", "800ohm +/- 5%", &guards("6mA"));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("pullups.sink_current") && m[0].contains("t.i2c.sink_current"),
        "{}",
        m[0]
    );
}

/// A board with neither a figure nor a level is as it was: nothing is listed.
#[test]
fn a_board_with_neither_is_as_it_was() {
    let s = run_with("Zt", "4.7kohm +/- 5%", "");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
}

/// The pull-up window: the sink current bounds the resistance from below and the rise time from
/// above, on one bus. A heavy Fast-mode bus, 400 pF and `tr <= 300 ns`, allows `Rp <= 885.2 ohm`
/// at the high end; a 6 mA driver asks for at least 510.8 ohm, and a 3 mA driver 1021.7 ohm.
fn window(resistance: &str, iol: &str) -> Solution {
    run_with(
        "Zt",
        resistance,
        &format!(
            "    .pullups.sink_current &= {iol}\n    .pullups.low_level &= 0.4V\n    .pullups.bus_capacitance &= 400pF\n    .pullups.rise_time <= 300ns\n"
        ),
    )
}

#[test]
fn a_resistance_inside_the_window_passes_both_bounds() {
    near(300e-9 / (0.8473 * 400e-12), 885.2);
    near(least(3.465, 0.4, 0.006), 510.83);
    let s = window("700ohm +/- 5%", "6mA");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
}

/// With a 3 mA driver the window is empty: the least resistance, 1021.7 ohm, is above the greatest
/// the rise time allows, 885.2 ohm. Whatever the board states fails one bound, and the message
/// says which.
#[test]
fn an_empty_window_fails_a_bound_and_says_which() {
    assert!(least(3.465, 0.4, 0.003) > 300e-9 / (0.8473 * 400e-12));
    // Too low for the driver, low enough for the rise time: the sink bound fails alone.
    let m = messages(&window("800ohm +/- 5%", "3mA"));
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].starts_with("pullups.resistance must always be at least"),
        "{}",
        m[0]
    );
    // Enough for the driver, too high for the rise time: the rise-time bound fails alone.
    let m = messages(&window("1.1kohm +/- 5%", "3mA"));
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].starts_with("pullups.rise_time") && m[0].contains("300ns"),
        "{}",
        m[0]
    );
    // In between, both fail.
    let m = messages(&window("900ohm +/- 5%", "3mA"));
    assert_eq!(m.len(), 2, "{m:#?}");
    assert!(m.iter().any(|x| x.starts_with("pullups.resistance must")));
    assert!(m.iter().any(|x| x.starts_with("pullups.rise_time")));
}
