//! The start-up time of a buck stage against a requirement, a closed-form check in std
//! (`.soft_start_time` of `Buck`, forwarded by `BuckStage`; `docs/research/closed-form-checks.md`,
//! check 4, option A of ADR 0076). Every part and figure here is invented: the IC `Zbk2` has an
//! internal ramp that its invented datasheet gives as 2 ms to 4 ms, and the board is the one of
//! `buck_ripple.rs`.
//!
//! A board states a requirement as a bound on the stage's member (`.soft_start_time <= 5ms`).
//! The IC's member is optional (ADR 0074): while nothing bounds it, the stage's relation is
//! skipped and the board is as it was; a bound on an IC that states no time is listed as not
//! evaluated, never passed.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn board(ic: &str, extra: &str) -> String {
    format!(
        r#"import std::parts::regulator::{{Buck, BuckStage}}
import std::sources::Rail

part Zbk2 {{
    .vin: Rail
    .vin.voltage >= 3V
    .vin.voltage <= 65V
    .buck: Buck {{
        .vref &= 0.8V +/- 1.5%
        .min_input_voltage &= 3V
        .max_input_voltage &= 65V
        .max_output_current &= 3A
        .efficiency &= 0.85..0.95
        .min_input_capacitance &= 2.2uF
        .min_output_capacitance &= 10uF
        .fsw &= 1MHz +/- 10%
        .min_inductance &= 1.5uH
{ic}    }}
    .buck.sw.neg ~ .vin.neg
    .vin.current == -.buck.sw.current * .buck.sw.voltage / (.vin.voltage * .buck.efficiency)
    dc switching_regulator .vin .buck.sw .buck.efficiency
    pins {{
        .vin.pos       power   "VIN"  "1"
        .vin.neg       power   "GND"  "2"
        .buck.sw.pos   power   "SW"   "3"
        .buck.fb       input   "FB"   "4"
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
    .u: Zbk2
    .stage: BuckStage {{
        .vout.voltage &= 1.8V +/- 3%
{extra}    }}
    .input ~ .stage.vin
    .input ~ .u.vin
    .stage.vout ~ .output
    .stage.buck ~ .u.buck
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

fn run(ic: &str, extra: &str) -> Solution {
    let c = check("soft_start.nsm", &board(ic, extra), &Loader::std());
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

const TIME: &str = "        .soft_start_time &= 2ms..4ms\n";

fn near(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-9 + 1e-6 * want.abs(),
        "got {got}, want {want}"
    );
}

/// A requirement the IC's whole span meets passes, and the stage's member is the IC's span.
#[test]
fn a_requirement_the_time_meets_passes() {
    let s = run(TIME, "        .soft_start_time <= 5ms\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:#?}", not_evaluated(&s));
    let t = range(&s, "stage.soft_start_time").unwrap();
    near(t.0, 2e-3);
    near(t.1, 4e-3);
}

/// A lower bound is the same member the other way (a ramp slow enough to keep the inrush down).
#[test]
fn a_lower_bound_is_held_the_same_way() {
    let s = run(TIME, "        .soft_start_time >= 1ms\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let s = run(TIME, "        .soft_start_time >= 3ms\n");
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
}

/// A requirement the time does not meet fails, with the figures: the span and the bound. The
/// worst case of the span is judged, so 3 ms is missed by the 4 ms end.
#[test]
fn a_requirement_the_time_does_not_meet_fails_and_names_the_figures() {
    let s = run(TIME, "        .soft_start_time <= 3ms\n");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert_eq!(
        m[0],
        "stage.soft_start_time must always be at most 3ms, but 2ms..4ms is not always below 3ms",
        "{}",
        m[0]
    );
}

/// The bound is exact at the end of the span: at 4 ms it holds, a little under it does not.
#[test]
fn the_bound_is_exact_at_the_end_of_the_span() {
    let s = run(TIME, "        .soft_start_time <= 4ms\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let s = run(TIME, "        .soft_start_time <= 3.99ms\n");
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
}

/// An IC that states no time is not judged: the stage's member has no value and the requirement
/// is listed as not evaluated, never passed.
#[test]
fn a_requirement_with_no_time_is_not_evaluated_and_never_passes() {
    let s = run("", "        .soft_start_time <= 5ms\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let n = not_evaluated(&s);
    assert!(
        n.iter().any(|r| r == "stage.soft_start_time <= 5ms"),
        "{n:#?}"
    );
    assert!(range(&s, "stage.soft_start_time").is_none());
    // The relation that forwards the IC's member was taken out (ADR 0074), not left to be listed.
    assert!(
        !n.iter().any(|r| r.contains("buck.soft_start_time")),
        "{n:#?}"
    );
}

/// A board that states no requirement is as it was, whether or not the IC states a time:
/// nothing is listed, nothing is said, and the picks do not move.
#[test]
fn a_board_with_no_requirement_is_as_it_was() {
    let with = run(TIME, "");
    let without = run("", "");
    for s in [&with, &without] {
        assert!(messages(s).is_empty(), "{:#?}", messages(s));
        assert!(not_evaluated(s).is_empty(), "{:#?}", not_evaluated(s));
    }
    for path in [
        "stage.inductor.inductance",
        "stage.output_cap.capacitance",
        "stage.output_count",
        "stage.input_count",
        "stage.ripple",
    ] {
        assert_eq!(
            range(&with, path),
            range(&without, path),
            "{path} moved with a stated time"
        );
    }
}

// ---- the LT8641: the time follows the capacitor on TR/SS (item CF2b of epic #308) ----
//
// `Lt8641` has `.css: Capacitance optional` and the relation
// `.buck.soft_start_time == .css * 0.8V / .tr_ss_current` (Rev. C: TR/SS Source Current 1.2 uA
// to 2.6 uA, p.3; the output tracks the TR/SS voltage from 0 V to 0.8 V, p.19). The current is a
// range, so the time is one, and the smallest current gives the longest time.

/// A project's rows for the `lt8641` catalog (the standard file has none), and an inductor of
/// a DC resistance the part accepts (the standard ones have more than its 0.04 ohm).
const LT_ROWS: &str = "mpn,package,library,source
LT8641IUDC#WPBF,UDC20(18),Extended,invented for a test
";

const LT_INDUCTORS: &str =
    "mpn,name,inductance,saturation_current,dc_resistance,package,library,source
ZL-3R3,ZL-3R3 3.3uH 4.5A inductor 1210,3.3uH +/- 20%,4.5A,0.03ohm,1210,Extended,invented
";

fn lt_board(extra: &str) -> String {
    format!(
        r#"import std::parts::passive::{{Capacitor, Resistor}}
import std::parts::regulator::{{BuckStage, Lt8641}}
import std::sources::Rail

board Converter {{
    .input: Rail {{
        .voltage &= 9V..16V
        .peak &= 36V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .output: Rail {{
        .pos.net &= "VOUT"
        .neg.net &= "GND"
        .current &= 0A..2A
    }}
    .u: Lt8641 {{ .mpn &= "LT8641IUDC#WPBF" }}
    .stage: BuckStage {{ .vout.voltage &= 3.3V +/- 4% }}
    .rt: Resistor {{ .resistance &= 47kohm +/- 1% }}
    .boost: Capacitor {{ .capacitance &= 100nF +/- 20% }}
    .intvcc_cap: Capacitor {{ .capacitance &= 1uF +/- 20% }}

    .input ~ .stage.vin
    .input ~ .u.vin
    .stage.vout ~ .output
    .stage.buck ~ .u.buck
    .u.en_uv ~ .input.pos
    .u.bias ~ .output.pos
    .u.bias_voltage == .output.voltage
    .u.sync_mode ~ .input.neg
    .u.rt ~> .rt ~> .input.neg
    .u.rt_resistance == .rt.resistance
    .u.bst ~> .boost ~> .u.buck.sw.pos
    .u.intvcc ~> .intvcc_cap ~> .input.neg
{extra}}}
"#
    )
}

fn lt_run(extra: &str) -> Solution {
    let c = check("lt8641_soft_start.nsm", &lt_board(extra), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file("lt8641", "catalogs/lt8641.csv", LT_ROWS)
        .unwrap();
    catalogs
        .add_file("inductor", "catalogs/inductor.csv", LT_INDUCTORS)
        .unwrap();
    solve(&c.boards[0], &catalogs)
}

/// A capacitor of 10 nF on TR/SS gives the time from the datasheet's figures: 10 nF * 0.8 V is
/// 8 nC, over 2.6 uA (the largest current, the shortest time) and over 1.2 uA (the smallest, the
/// longest). A requirement of 7 ms holds, and the stage's member is the whole range.
#[test]
fn the_time_follows_the_capacitor_on_tr_ss_and_the_orientation_is_the_currents() {
    let s = lt_run("    .u.css &= 10nF\n    .stage.soft_start_time <= 7ms\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    // The board's other relations that the solver cannot evaluate are not this one's.
    let n = not_evaluated(&s);
    assert!(
        !n.iter()
            .any(|r| r.contains("soft_start") || r.contains("css")),
        "{n:#?}"
    );
    let t = range(&s, "stage.soft_start_time").unwrap();
    // The smallest current gives the largest time, the largest current the smallest.
    near(t.0, 8e-9 / 2.6e-6);
    near(t.1, 8e-9 / 1.2e-6);
    assert!(
        t.0 < 4.211e-3 && 4.211e-3 < t.1,
        "{t:?}: the typical 1.9 uA is inside"
    );
    let u = range(&s, "u.buck.soft_start_time").unwrap();
    near(u.0, t.0);
    near(u.1, t.1);
}

/// The worst case is judged: 5 ms is above the typical 4.2 ms and the best case 3.1 ms, and the
/// 1.2 uA end, 6.667 ms, misses it. The message names the range and the bound.
#[test]
fn a_requirement_the_worst_case_current_misses_fails_and_names_the_figures() {
    let s = lt_run("    .u.css &= 10nF\n    .stage.soft_start_time <= 5ms\n");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].starts_with(
            "stage.soft_start_time must always be at most 5ms, but 3.076ms..6.667ms is not always below 5ms"
        ),
        "{}",
        m[0]
    );
}

/// The bound is exact at the worst-case end (8 nC / 1.2 uA is 6.6667 ms), and a lower bound is
/// judged at the other end (the best case, 3.077 ms, is under 3.1 ms).
#[test]
fn the_bound_is_exact_at_the_ends_of_the_range() {
    let s = lt_run("    .u.css &= 10nF\n    .stage.soft_start_time <= 6.67ms\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let s = lt_run("    .u.css &= 10nF\n    .stage.soft_start_time <= 6.66ms\n");
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
    let s = lt_run("    .u.css &= 10nF\n    .stage.soft_start_time >= 3.07ms\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let s = lt_run("    .u.css &= 10nF\n    .stage.soft_start_time >= 3.1ms\n");
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
}

/// A capacitor of a tolerance widens the range at both ends: 10 nF +/- 10% is 9 nF to 11 nF,
/// so 9 nF * 0.8 V / 2.6 uA to 11 nF * 0.8 V / 1.2 uA.
#[test]
fn the_capacitors_tolerance_widens_the_time() {
    let s = lt_run("    .u.css &= 10nF +/- 10%\n    .stage.soft_start_time <= 8ms\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let t = range(&s, "stage.soft_start_time").unwrap();
    near(t.0, 9e-9 * 0.8 / 2.6e-6);
    near(t.1, 11e-9 * 0.8 / 1.2e-6);
}

/// The capacitor a board puts on TR/SS states `.css` by an equation, as it does for RT.
#[test]
fn the_capacitor_of_the_board_can_state_css() {
    let s = lt_run(
        "    .soft_start: Capacitor { .capacitance &= 22nF +/- 10% }\n    \
         .u.tr_ss ~> .soft_start ~> .input.neg\n    .u.css == .soft_start.capacitance\n    \
         .stage.soft_start_time <= 20ms\n",
    );
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let t = range(&s, "stage.soft_start_time").unwrap();
    near(t.0, 22e-9 * 0.9 * 0.8 / 2.6e-6);
    near(t.1, 22e-9 * 1.1 * 0.8 / 1.2e-6);
}

/// With `.css` unstated nothing changes: a bound is listed as not evaluated and never passes,
/// and the relation that computes the time is skipped, not listed.
#[test]
fn a_requirement_with_no_capacitor_is_not_evaluated_and_never_passes() {
    let s = lt_run("    .stage.soft_start_time <= 5ms\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let n = not_evaluated(&s);
    assert!(
        n.iter().any(|r| r == "stage.soft_start_time <= 5ms"),
        "{n:#?}"
    );
    assert!(range(&s, "stage.soft_start_time").is_none());
    assert!(!n.iter().any(|r| r.contains("css")), "{n:#?}");
}

/// A board that states neither a capacitor nor a requirement is as it was: nothing is said,
/// nothing is listed, and the picks do not move with a stated capacitor.
#[test]
fn a_board_that_states_no_requirement_is_as_it_was() {
    let without = lt_run("");
    let with = lt_run("    .u.css &= 10nF\n");
    for s in [&without, &with] {
        assert!(messages(s).is_empty(), "{:#?}", messages(s));
    }
    // The list of what is not evaluated is the same, with a capacitor stated or not.
    assert_eq!(not_evaluated(&with), not_evaluated(&without));
    for path in [
        "stage.inductor.inductance",
        "stage.output_cap.capacitance",
        "stage.output_count",
        "stage.input_count",
        "stage.ripple",
    ] {
        assert_eq!(
            range(&with, path),
            range(&without, path),
            "{path} moved with a stated capacitor"
        );
    }
}
