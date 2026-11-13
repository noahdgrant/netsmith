//! The ripple of a buck stage's output voltage, a closed-form check in std (`.output_ripple`
//! of `Buck`, `docs/research/closed-form-checks.md`, check 3). Every part and value here is
//! invented; the expected numbers are worked out below by hand from
//! `dI / (8 * fsw * n * C) + dI * ESR / n` and not read from the solver.
//!
//! The board: an input of 9 V to 16 V (peak 30 V), 1.8 V +/- 3 % out at up to 1 A, an IC at
//! 1 MHz +/- 10 %, one inductor (2.2 uH +/- 20 %) and an output capacitor of 47 uF +/- 20 %
//! that keeps `1 - 0.2 * v / 3` of its value at `v` volts (`0:1 3:0.8`).
//!
//! The worst case of every term, each of which grows as it moves in the same direction:
//!
//! - the ripple current `dI = vout * (1 - vout / vin) / (L * fsw)` is largest at the highest
//!   input (16 V) and output (1.854 V, the lower the ratio `vout / vin` the larger, and
//!   `vout * (1 - vout / vin)` grows with `vout` below half of `vin`), the lowest inductance
//!   (1.76 uH) and the lowest frequency (0.9 MHz): `1.854 * (1 - 1.854 / 16) / (1.76e-6 * 0.9e6)`;
//! - the capacitance is smallest at the lowest marked value (37.6 uF) at the highest bias
//!   (1.854 V, which keeps `1 - 0.2 * 1.854 / 3 = 0.8764` of it): 32.9526 uF;
//! - the ESR is the highest of the range the board states.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn board(extra: &str) -> String {
    format!(
        r#"import std::parts::regulator::{{Buck, BuckIc}}
import std::sources::Rail

part Zbk2 provides BuckIc {{
    .vin.voltage >= 3V
    .vin.voltage <= 65V
    .vref &= 0.8V +/- 1.5%
    .min_input_voltage &= 3V
    .max_input_voltage &= 65V
    .max_output_current &= 3A
    .efficiency &= 0.85..0.95
    .min_input_capacitance &= 2.2uF
    .min_output_capacitance &= 10uF
    .fsw &= 1MHz +/- 10%
    .min_inductance &= 1.5uH
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)
    dc switching_regulator .vin .sw .efficiency
    pins {{
        .vin.pos       power   "VIN"  "1"
        .vin.neg       power   "GND"  "2"
        .sw.pos        power   "SW"   "3"
        .fb            input   "FB"   "4"
    }}
    .mpn: String
    .package: Package
    catalog zbk2 {{ .mpn, .package }}
}}

board Converter {{
    .input: Rail {{
        .voltage &= 9V..16V
        .peak &= 30V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .output: Rail {{
        .pos.net &= "1V8"
        .neg.net &= "GND"
        .current &= 0mA..1A
        .peak &= 1.9V
    }}
    .stage: Buck<Zbk2> {{
        .vout.voltage &= 1.8V +/- 3%
{extra}    }}
    .input ~ .stage.vin
    .stage.vout ~ .output
}}
"#
    )
}

const IC: &str = "mpn,name,label,package,source
ZBK2-A,ZBK2-A invented buck IC,ZBK2,SOT-23-6,invented for a test
";

const RESISTOR: &str = "mpn,resistance,max_power,package,source
R20,20kohm +/- 0.1%,100mW,0402,invented
R25,25kohm +/- 0.1%,100mW,0402,invented
R100,100kohm +/- 0.1%,100mW,0402,invented
";

const INDUCTOR: &str = "mpn,inductance,saturation_current,dc_resistance,package,source
L22,2.2uH +/- 20%,3.6A,0.085ohm,1210,invented
";

const CAPACITOR: &str =
    "mpn,capacitance,effective_capacitance,max_voltage,package,dielectric,source
C47U6,47uF +/- 20%,derate capacitance by bias: 0:1 3:0.8 6.3:0.5,6.3V,0805,X5R,invented
C4U35,4.7uF +/- 10%,derate capacitance by bias: 0:1 10:0.8 35:0.3,35V,1206,X7R,invented
";

fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    for (name, text) in [
        ("zbk2", IC),
        ("resistor", RESISTOR),
        ("inductor", INDUCTOR),
        ("capacitor", CAPACITOR),
    ] {
        c.add_file(name, &format!("catalogs/{name}.csv"), text)
            .unwrap();
    }
    c
}

fn run(extra: &str) -> Solution {
    let c = check("ripple.nsm", &board(extra), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &catalogs())
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

/// The ripple current at its worst, by hand.
fn ripple_current() -> f64 {
    let vout = 1.8 * 1.03;
    vout * (1.0 - vout / 16.0) / (2.2e-6 * 0.8 * 0.9e6)
}

/// The output voltage ripple at its worst for a bank of `n` capacitors of `esr` ohms each, by
/// hand.
fn output_ripple(n: f64, esr: f64) -> f64 {
    let c = 47e-6 * 0.8 * (1.0 - 0.2 * (1.8 * 1.03) / 3.0);
    let di = ripple_current();
    di / (8.0 * 0.9e6 * n * c) + di * esr / n
}

#[test]
fn the_hand_figures_are_the_ones_the_header_says() {
    // 1.6392 V.A / 1.584 = 1.0348 A, 32.9526 uF, and 4.36 mV + 5.17 mV = 9.54 mV.
    near(ripple_current(), 1.034828);
    near(output_ripple(1.0, 5e-3), 4.3617e-3 + 5.1741e-3);
}

/// A board that states the ESR and a limit above the worst case passes, and the worst case is
/// the corner (the ESR's high end, the lowest inductance, frequency and capacitance, the highest
/// input), not the nominal.
#[test]
fn a_ripple_inside_the_limit_passes_and_the_worst_case_is_the_corner() {
    let s = run(
        "        .output_count &= 1\n        .output_esr &= 3mohm..5mohm\n        .output_ripple <= 12mV\n",
    );
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
    let ripple = range(&s, "stage.output_ripple").unwrap();
    near(ripple.1, output_ripple(1.0, 5e-3));
    // The nominal figures (1 MHz, 2.2 uH, 47 uF at 1.8 V, 4 mohm, 12 V) are far from it.
    let nominal =
        (1.8 * (1.0 - 1.8 / 12.0) / (2.2e-6 * 1e6)) * (1.0 / (8.0 * 1e6 * 47e-6 * 0.88) + 4e-3);
    assert!(nominal < 0.7 * ripple.1, "{nominal} against {}", ripple.1);
}

/// A limit under the worst case fails with the numbers: the ripple's whole range and the limit. The
/// capacitor is the part the ripple judges (its capacitance is in it), so the message is the
/// search's: the closest candidate and why it fails.
#[test]
fn a_ripple_over_the_limit_fails_and_names_the_numbers() {
    let s = run(
        "        .output_count &= 1\n        .output_esr &= 3mohm..5mohm\n        .output_ripple <= 9mV\n",
    );
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].starts_with("no `capacitor` candidate satisfies `stage.output_cap`: the closest of the 2 that fit, 1 x C47U6, fails because stage.output_ripple must always be at most 9mV, but ")
            && m[0].ends_with("is not always below 9mV"),
        "{}",
        m[0]
    );
    assert!(
        m[0].contains("..9.536mV"),
        "the worst case is 9.5358 mV: {}",
        m[0]
    );
}

/// The limit is exact: just above the worst case passes, just below fails.
#[test]
fn the_limit_is_exact_at_the_worst_case() {
    let worst = output_ripple(1.0, 5e-3);
    assert!((worst - 9.5358e-3).abs() < 1e-6, "{worst}");
    let s = run(
        "        .output_count &= 1\n        .output_esr &= 3mohm..5mohm\n        .output_ripple <= 9.54mV\n",
    );
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let s = run(
        "        .output_count &= 1\n        .output_esr &= 3mohm..5mohm\n        .output_ripple <= 9.53mV\n",
    );
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
}

/// Two capacitors share the ripple current: each term divides by the count.
#[test]
fn a_bank_of_two_halves_both_terms() {
    let s = run(
        "        .output_count &= 2\n        .output_esr &= 5mohm\n        .output_ripple <= 5mV\n",
    );
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    near(
        range(&s, "stage.output_ripple").unwrap().1,
        output_ripple(2.0, 5e-3),
    );
    assert!(output_ripple(2.0, 5e-3) < 5e-3 && output_ripple(1.0, 5e-3) > 5e-3);
    // The same limit on a single capacitor fails.
    let s = run(
        "        .output_count &= 1\n        .output_esr &= 5mohm\n        .output_ripple <= 5mV\n",
    );
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
}

/// A board that states a limit and no ESR is not judged: the ripple has no value, and the limit is
/// listed as not evaluated, never passed. The capacitor term alone, 4.36 mV, would pass the 5 mV
/// limit and is not used for it.
#[test]
fn a_limit_with_no_esr_is_not_evaluated_and_never_passes() {
    let s = run("        .output_count &= 1\n        .output_ripple <= 5mV\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let n = not_evaluated(&s);
    assert!(
        n.iter().any(|r| r == "stage.output_ripple <= 5mV"),
        "{n:#?}"
    );
    assert!(range(&s, "stage.output_ripple").is_none());
    // The relation that reads the ESR was taken out (ADR 0074), not left to be listed.
    assert!(!n.iter().any(|r| r.contains("output_esr")), "{n:#?}");
}

/// A board that states neither the ESR nor a limit is as it was: nothing is listed.
#[test]
fn a_board_with_neither_is_as_it_was() {
    let s = run("");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
}
