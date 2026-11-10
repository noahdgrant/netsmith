//! The rise time of an I2C bus against its capacitance, a closed-form check in std
//! (`.rise_time` and `.bus_capacitance` of `I2cPullups`, `.capacitance` of `I2c`,
//! `docs/research/closed-form-checks.md`, check 5). Every part and value here is invented; the
//! expected numbers are worked out by hand from `tr = 0.8473 * Rp * Cb` and not read from the
//! solver. The constant is the I2C-bus specification's (UM10204 Rev. 7.0, section 7.1).
//!
//! The board: a controller (pin capacitance 5 pF) and a target (8 pF) on 3.3 V +/- 5 % with a
//! pull-up pair stated as 4.7 kohm +/- 5 %, for which the catalog has one row, 4.7 kohm +/- 1 %.
//! The traces and the connector are the board's estimate, 20 pF to 40 pF.
//!
//! The worst case is the highest resistance and the highest capacitance. The resistance is the
//! range the board states, whose high end is `4.7 kohm * 1.05 = 4.935 kohm` (the picked resistor
//! stays within it, so the statement bounds it), and `Cb = 5 + 8 + 40 = 53 pF`, so
//! `tr = 0.8473 * 4935 * 53e-12 = 221.62 ns`. The supply does not enter: both points are
//! fractions of it.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::bus::{I2c, I2cPullups, I2cRole}
import std::sources::Rail

/// A controller that states the capacitance of its pin.
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
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "VSS" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  bidirectional "SCL" "4"
    }
}

/// A target that states another capacitance.
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
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  input         "SCL" "4"
    }
}

/// A target whose datasheet states no capacitance.
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

/// A board of the controller and `second`, with `extra` as statements of the board and `rail`
/// as the supply's voltage.
fn board(second: &str, rail: &str, extra: &str) -> String {
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
        .resistance &= 4.7kohm +/- 5%
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

const RESISTOR: &str = "mpn,resistance,max_power,package,source
R4K7,4.7kohm +/- 1%,100mW,0402,invented
";

fn run_with(second: &str, rail: &str, extra: &str) -> Solution {
    let c = check("bus.nsm", &board(second, rail, extra), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let mut catalogs = Catalogs::empty();
    catalogs
        .add_file("resistor", "catalogs/resistor.csv", RESISTOR)
        .unwrap();
    solve(&c.boards[0], &catalogs)
}

/// The two parts that state a capacitance, on 3.3 V +/- 5 %, and the board's own 20 pF to 40 pF
/// for the traces, with `limits` as more statements.
fn run(limits: &str) -> Solution {
    run_with(
        "Zt",
        "3.3V +/- 5%",
        &format!(
            "    .pullups.bus_capacitance &= .c.i2c.capacitance + .t.i2c.capacitance + (20pF..40pF)\n{limits}"
        ),
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

/// The rise time of `r` ohms and `c` farads, by hand: the 30 % to 70 % time of an RC charge.
fn rise(r: f64, c: f64) -> f64 {
    0.8473 * r * c
}

#[test]
fn the_hand_figures_are_the_ones_the_header_says() {
    // ln(7/3) is the constant, and 0.8473 * 4747 ohm * 53 pF is 221.6 ns.
    assert!(((7.0f64 / 3.0).ln() - 0.8473).abs() < 1e-4);
    near(rise(4935.0, 53e-12), 221.62e-9);
}

/// A bound above the worst case passes, and the worst case is the corner (the resistor's high
/// end, the traces' 40 pF), not the nominal.
#[test]
fn a_rise_time_inside_the_limit_passes_and_the_worst_case_is_the_corner() {
    let s = run("    .pullups.rise_time <= 300ns\n    .pullups.bus_capacitance <= 400pF\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
    let cap = range(&s, "pullups.bus_capacitance").unwrap();
    near(cap.0, 33e-12);
    near(cap.1, 53e-12);
    let tr = range(&s, "pullups.rise_time").unwrap();
    near(tr.1, rise(4935.0, 53e-12));
    // The nominal figures (4.7 kohm, 43 pF, 171 ns) are well short of the worst.
    assert!(rise(4700.0, 43e-12) < 0.8 * tr.1, "{tr:?}");
}

/// A limit under the worst case fails with the numbers: the rise time's whole range (the figures are
/// rounded outwards, 221.62 ns shows as 221.7 ns) and the limit.
#[test]
fn a_rise_time_over_the_limit_fails_and_names_the_numbers() {
    let s = run("    .pullups.rise_time <= 200ns\n");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("pullups.rise_time") && m[0].contains("200ns") && m[0].contains("..221.7ns"),
        "{}",
        m[0]
    );
}

/// The limit is exact: just above the worst case passes, just below fails.
#[test]
fn the_limit_is_exact_at_the_worst_case() {
    let worst = rise(4935.0, 53e-12);
    assert!((worst - 221.62e-9).abs() < 0.05e-9, "{worst}");
    let s = run("    .pullups.rise_time <= 221.7ns\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let s = run("    .pullups.rise_time <= 221.5ns\n");
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
}

/// The supply does not enter: 30 % and 70 % are fractions of it, so a wide range of the rail and
/// another voltage give the same worst case.
#[test]
fn the_supply_does_not_change_the_rise_time() {
    let extra = "    .pullups.bus_capacitance &= .c.i2c.capacitance + .t.i2c.capacitance + (20pF..40pF)\n    .pullups.rise_time <= 300ns\n";
    for rail in ["3.3V +/- 5%", "3V..3.6V", "5V +/- 10%"] {
        let s = run_with("Zt", rail, extra);
        assert!(messages(&s).is_empty(), "{rail}: {:#?}", messages(&s));
        near(
            range(&s, "pullups.rise_time").unwrap().1,
            rise(4935.0, 53e-12),
        );
    }
}

/// A bound on the capacitance itself (the specification's 400 pF) is judged on the sum.
#[test]
fn a_capacitance_over_its_limit_fails() {
    let s = run("    .pullups.bus_capacitance <= 50pF\n");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("pullups.bus_capacitance") && m[0].contains("50pF"),
        "{}",
        m[0]
    );
}

/// A part that states no pin capacitance leaves the sum without a value, so the board's bound is
/// listed as not evaluated and is never a pass, although the two pins that are stated, 5 pF and
/// the traces' 40 pF, would pass it.
#[test]
fn a_part_with_no_capacitance_leaves_the_check_not_evaluated() {
    let s = run_with(
        "Zn",
        "3.3V +/- 5%",
        "    .pullups.bus_capacitance &= .c.i2c.capacitance + .t.i2c.capacitance + (20pF..40pF)\n    .pullups.rise_time <= 300ns\n",
    );
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let n = not_evaluated(&s);
    assert!(
        n.iter().any(|r| r == "pullups.rise_time <= 300ns"),
        "{n:#?}"
    );
    assert!(range(&s, "pullups.rise_time").is_none());
    assert!(range(&s, "pullups.bus_capacitance").is_none());
}

/// A board that states the whole capacitance itself (no part's figure) is judged as well.
#[test]
fn a_capacitance_the_board_states_is_enough() {
    let s = run_with(
        "Zn",
        "3.3V +/- 5%",
        "    .pullups.bus_capacitance &= 80pF..120pF\n    .pullups.rise_time <= 600ns\n",
    );
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    near(
        range(&s, "pullups.rise_time").unwrap().1,
        rise(4935.0, 120e-12),
    );
    let s = run_with(
        "Zn",
        "3.3V +/- 5%",
        "    .pullups.bus_capacitance &= 80pF..120pF\n    .pullups.rise_time <= 500ns\n",
    );
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
}

/// A board that states no capacitance and no bound is as it was: nothing is listed.
#[test]
fn a_board_with_neither_is_as_it_was() {
    let s = run_with("Zt", "3.3V +/- 5%", "");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
}
