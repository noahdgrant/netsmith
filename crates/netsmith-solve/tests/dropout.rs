//! Dropout (issue #1259): a regulator's output follows its input less its dropout at the load,
//! so the least input that keeps the output rail at its minimum is that minimum plus the
//! dropout. A linear regulator's dropout is its row's figure or its curve file over
//! `output_current` and the board's temperature (`netsmith_solve::curve`); a buck converter's is
//! the inductor's current times the IC's high-side switch resistance and the inductor's DC
//! resistance. Every part, row and curve here is invented: no standard row has a recorded
//! dropout curve or switch resistance yet (`data/jlcpcb/README.md`). The numbers are worked out
//! by hand in the comments, not read from the solver.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// One invented 3.3 V +/- 2 % regulator (3.234 V to 3.366 V): 0.6 V at its rated 500 mA is the
/// row's single figure.
const LDO: &str = "mpn,vout.voltage,dropout,max_current,max_power,package,source
ZZL-33,3.3V +/- 2%,0.6V,0.5A,2W,SOT-23,https://example.invalid/zzl
";

/// Its dropout over the load at three temperatures.
const LDO_CURVE: &str = "temperature,output_current,dropout
-40degC,0A,0.04V
-40degC,500mA,0.45V
25degC,0A,0.05V
25degC,100mA,0.15V
25degC,500mA,0.5V
125degC,0A,0.08V
125degC,100mA,0.22V
125degC,500mA,0.6V
";

const LDO_CURVE_FILE: &str = "catalogs/curves/ldo/ZZL-33.csv";

fn ldo_catalogs(curve: bool) -> Catalogs {
    let mut c = Catalogs::empty();
    c.add_file("ldo", "catalogs/ldo.csv", LDO).unwrap();
    if curve {
        c.add_curve("ldo", "ZZL-33", LDO_CURVE_FILE, LDO_CURVE)
            .unwrap();
    }
    c
}

/// A supply of `vin` into the standard `LinearRegulator`, whose output feeds a load of `load`,
/// with `rest` after them.
fn ldo_board(vin: &str, load: &str, rest: &str) -> String {
    format!(
        "import std::parts::regulator::LinearRegulator
import std::sources::Rail
import std::env::Environment

board B {{
    .supply: Rail {{ .voltage &= {vin}; .pos.net &= \"VIN\" }}
    .u: LinearRegulator
    .supply ~ .u.vin
    .load: Rail {{ .current &= {load}; .pos.net &= \"V33\" }}
    .u.vout ~ .load
{rest}
}}
"
    )
}

fn solved(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn errors(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> (f64, f64) {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("{path}: {other:?}"),
    }
}

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

const AT_25: &str = "    .environment: Environment { .temperature &= 25degC }";

#[test]
fn the_dropout_is_read_off_the_curve_at_the_load() {
    let s = solved(
        &ldo_board("3.5V..5V", "0mA..100mA", AT_25),
        &ldo_catalogs(true),
    );
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert!(
        close(range(&s, "u.output_current"), (0.0, 0.1)),
        "{:?}",
        range(&s, "u.output_current")
    );
    assert!(
        close(range(&s, "u.dropout"), (0.05, 0.15)),
        "{:?}",
        range(&s, "u.dropout")
    );
}

/// The least input is the output rail's minimum plus the dropout at the load: 3.234 V + 0.15 V
/// = 3.384 V, which a 3.5 V supply holds. At the output's highest set point (3.366 V + 0.15 V =
/// 3.516 V) the regulator is in dropout at 3.5 V, and its output, 3.35 V, is still inside its
/// rail's range.
#[test]
fn the_least_input_is_the_rails_minimum_plus_the_dropout() {
    let s = solved(
        &ldo_board("3.5V..5V", "0mA..100mA", AT_25),
        &ldo_catalogs(true),
    );
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    // With the row's figure alone, 3.234 V + 0.6 V = 3.834 V, the same supply is too low.
    let s = solved(
        &ldo_board("3.5V..5V", "0mA..100mA", AT_25),
        &ldo_catalogs(false),
    );
    let all = errors(&s).join("\n");
    assert!(all.contains("min(u.vout.voltage)"), "{all}");
}

/// A hot board raises the dropout at 100 mA to 0.22 V: the least input is 3.454 V, which a
/// 3.4 V supply is not, while at 25 C (3.384 V) it is.
#[test]
fn a_hot_board_needs_more_input() {
    let s = solved(
        &ldo_board("3.4V..5V", "0mA..100mA", AT_25),
        &ldo_catalogs(true),
    );
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    let s = solved(
        &ldo_board(
            "3.4V..5V",
            "0mA..100mA",
            "    .environment: Environment { .temperature &= 25degC..125degC }",
        ),
        &ldo_catalogs(true),
    );
    let all = errors(&s).join("\n");
    assert!(all.contains("min(u.vout.voltage) + u.dropout"), "{all}");
    assert!(all.contains("3.4V..5V"), "{all}");
}

/// A regulator that feeds another: the second one's least input is the first one's output rail,
/// so the 3.3 V one keeps the 5 V one's minimum. The first is a fixed 3.6 V +/- 1 % (3.564 V
/// to 3.636 V) regulator with no curve; the second needs 3.234 V + 0.15 V at its load.
#[test]
fn a_regulator_behind_another_is_held_to_the_first_ones_minimum() {
    let rows = "mpn,vout.voltage,dropout,max_current,max_power,package,source
ZZL-33,3.3V +/- 2%,0.6V,0.5A,2W,SOT-23,https://example.invalid/zzl
ZZL-36,3.6V +/- 1%,0.3V,0.5A,2W,SOT-23,https://example.invalid/zzl
";
    let mut c = Catalogs::empty();
    c.add_file("ldo", "catalogs/ldo.csv", rows).unwrap();
    c.add_curve("ldo", "ZZL-33", LDO_CURVE_FILE, LDO_CURVE)
        .unwrap();
    let src = "import std::parts::regulator::LinearRegulator
import std::sources::Rail
import std::env::Environment

board B {
    .supply: Rail { .voltage &= 3.9V..5V; .pos.net &= \"VIN\" }
    .first: LinearRegulator { .vout.voltage &= 3.5V..3.7V }
    .second: LinearRegulator { .vout.voltage &= 3.2V..3.4V }
    .supply ~ .first.vin
    .first.vout ~ .second.vin
    .mid: Rail { .current &= 0mA..10mA; .pos.net &= \"V36\" }
    .first.vout ~ .mid
    .load: Rail { .current &= 0mA..100mA; .pos.net &= \"V33\" }
    .second.vout ~ .load
    .environment: Environment { .temperature &= 25degC }
}
";
    let s = solved(src, &c);
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    // The first: 3.564 V + 0.3 V = 3.864 V, under the 3.9 V supply; the second: 3.384 V, under
    // the first's 3.564 V.
    assert!(
        close(range(&s, "second.dropout"), (0.05, 0.15)),
        "{:?}",
        range(&s, "second.dropout")
    );
}

// The buck converter: an IC whose high-side switch is 0.1 ohm and an inductor of 0.085 ohm,
// 1.8 V +/- 3 % out (1.746 V to 1.854 V) at up to 1 A.

fn buck_board(ic_extra: &str, vin: &str) -> String {
    format!(
        r#"import std::parts::regulator::{{Buck, BuckIc}}
import std::sources::Rail

part Zbk3 provides BuckIc {{
    .vin.voltage >= 1.5V
    .vin.voltage <= 20V
    .vref &= 0.8V +/- 1.5%
    .min_input_voltage &= 1.5V
    .max_input_voltage &= 20V
    .max_output_current &= 3A
    .efficiency &= 0.85..0.95
    .min_input_capacitance &= 2.2uF
    .min_output_capacitance &= 10uF
    .fsw &= 1MHz +/- 10%
    .min_inductance &= 1.5uH
{ic_extra}
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)
    dc switching_regulator .vin .sw .efficiency
    pins {{
        .vin.pos       power   "VIN"  "1"
        .vin.neg       power   "GND"  "2"
        .sw.pos        power   "SW"   "3"
        .fb            input   "FB"   "4"
    }}
    .package: Package
    catalog zbk3 {{ .mpn, .package }}
}}

board Converter {{
    .input: Rail {{
        .voltage &= {vin}
        .peak &= 6V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .output: Rail {{
        .pos.net &= "1V8"
        .neg.net &= "GND"
        .current &= 0mA..1A
        .peak &= 1.9V
    }}
    .stage: Buck<Zbk3> {{ .vout.voltage &= 1.8V +/- 3% }}
    .input ~ .stage.vin
    .stage.vout ~ .output
}}
"#
    )
}

fn buck_catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    for (name, text) in [
        (
            "zbk3",
            "mpn,name,label,package,source\nZBK3-A,ZBK3-A invented buck IC,ZBK3,SOT-23-6,invented for a test\n",
        ),
        (
            "resistor",
            "mpn,resistance,max_power,package,source
R20,20kohm +/- 0.1%,100mW,0402,invented
R25,25kohm +/- 0.1%,100mW,0402,invented
",
        ),
        (
            "inductor",
            "mpn,inductance,saturation_current,dc_resistance,package,source
L22,2.2uH +/- 20%,3.6A,0.085ohm,1210,invented
",
        ),
        (
            "capacitor",
            "mpn,capacitance,effective_capacitance,max_voltage,package,dielectric,source
C47U6,47uF +/- 20%,derate capacitance by bias: 0:1 3:0.8 6.3:0.5,6.3V,0805,X5R,invented
C4U35,4.7uF +/- 10%,derate capacitance by bias: 0:1 10:0.8 35:0.3,35V,1206,X7R,invented
",
        ),
    ] {
        c.add_file(name, &format!("catalogs/{name}.csv"), text)
            .unwrap();
    }
    c
}

const HIGH_SIDE: &str = "    .high_side_resistance &= 0.1ohm";

/// The drop is what the inductor carries, the load and the divider's current (20 kohm under
/// 25 kohm across 1.746 V to 1.854 V, about 39 uA to 42 uA), times 0.185 ohm: up to about
/// 0.1850 V, so the least input is about 1.931 V.
#[test]
fn a_bucks_dropout_is_its_current_through_the_switch_and_the_winding() {
    let s = solved(&buck_board(HIGH_SIDE, "2V..5V"), &buck_catalogs());
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    let (lo, hi) = range(&s, "stage.dropout");
    assert!(lo < 1e-4, "{lo}");
    assert!((hi - 1.000_042 * 0.185).abs() < 1e-5, "{hi}");
}

/// A 1.9 V input is above the output's highest set point (1.854 V), which is all the converter
/// asked before, and below what the output's minimum needs at 1 A.
#[test]
fn an_input_inside_the_bucks_dropout_is_refused() {
    let s = solved(&buck_board(HIGH_SIDE, "1.9V..5V"), &buck_catalogs());
    let all = errors(&s).join("\n");
    assert!(all.contains("min(stage.vout.voltage)"), "{all}");
    assert!(all.contains("stage.ic.high_side_resistance"), "{all}");
    assert!(all.contains("1.9V..5V"), "{all}");
}

/// An IC that states no switch resistance has no dropout check, and nothing is left not
/// evaluated for it.
#[test]
fn an_ic_with_no_switch_resistance_has_no_dropout_check() {
    let s = solved(&buck_board("", "1.9V..5V"), &buck_catalogs());
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    // As many relations wait as for the IC that states one, where every one is judged.
    let stated = solved(&buck_board(HIGH_SIDE, "2V..5V"), &buck_catalogs());
    assert_eq!(s.deferred.len(), stated.deferred.len(), "{:?}", s.deferred);
    assert_eq!(s.model.domain_at("stage.dropout"), Some(&Domain::Unset));
}
