//! The start-up time of a buck stage against a requirement, a closed-form check in std
//! (`.soft_start_time` of `BuckIc`, forwarded by `Buck`; `docs/research/closed-form-checks.md`,
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
{ic}    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)
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

const TIME: &str = "    .soft_start_time &= 2ms..4ms\n";

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
        !n.iter().any(|r| r.contains("ic.soft_start_time")),
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
// `Lt8641` has `.css: Capacitance` and the relation
// `.soft_start_time == .css * 0.8V / .tr_ss_current` (Rev. C: TR/SS Source Current 1.2 uA
// to 2.6 uA, p.3; the output tracks the TR/SS voltage from 0 V to 0.8 V, p.19). The current is a
// range, so the time is one, and the smallest current gives the longest time.
//
// The capacitor is the part's own, in its `support` section (ADR 0115): 10 nF +/- 10 % by
// default, which states `.css`. A board that places the IC through `Buck<Lt8641>` may narrow it
// (ADR 0116); these boards do not, so they judge the default capacitor: 9 nF to 11 nF, 9 nF * 0.8 V /
// 2.6 uA = 2.769 ms to 11 nF * 0.8 V / 1.2 uA = 7.333 ms. Before, the board placed the IC and
// its capacitor itself and stated `.u.css &= 10nF` exactly.

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
        r#"import std::parts::regulator::{{Buck, Lt8641}}
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
    .stage: Buck<Lt8641> {{ .vout.voltage &= 3.3V +/- 4% }}

    .input ~ .stage.vin
    .stage.vout ~ .output
{extra}}}
"#
    )
}

fn lt_catalogs() -> Catalogs {
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file("lt8641", "catalogs/lt8641.csv", LT_ROWS)
        .unwrap();
    catalogs
        .add_file("inductor", "catalogs/inductor.csv", LT_INDUCTORS)
        .unwrap();
    catalogs
}

fn lt_run(extra: &str) -> Solution {
    let c = check("lt8641_soft_start.nsm", &lt_board(extra), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &lt_catalogs())
}

/// The time of the default capacitor at its two ends, as the header works it out.
const LT_SHORTEST: f64 = 9e-9 * 0.8 / 2.6e-6;
const LT_LONGEST: f64 = 11e-9 * 0.8 / 1.2e-6;

/// The capacitor on TR/SS gives the time from the datasheet's figures, the largest current
/// (2.6 uA) the shortest time and the smallest (1.2 uA) the longest. A requirement of 8 ms holds,
/// and the stage's member is the whole range, the IC's.
#[test]
fn the_time_follows_the_capacitor_on_tr_ss_and_the_orientation_is_the_currents() {
    let s = lt_run("    .stage.soft_start_time <= 8ms\n");
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
    near(t.0, LT_SHORTEST);
    near(t.1, LT_LONGEST);
    assert!(
        t.0 < 4.211e-3 && 4.211e-3 < t.1,
        "{t:?}: the typical 1.9 uA at 10 nF is inside"
    );
    let u = range(&s, "stage.ic.soft_start_time").unwrap();
    near(u.0, t.0);
    near(u.1, t.1);
}

/// The worst case is judged: 5 ms is above the typical 4.2 ms and the best case 2.8 ms, and the
/// 1.2 uA end at 11 nF, 7.333 ms, misses it. The message names the range and the bound.
#[test]
fn a_requirement_the_worst_case_current_misses_fails_and_names_the_figures() {
    let s = lt_run("    .stage.soft_start_time <= 5ms\n");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].starts_with(
            "stage.soft_start_time must always be at most 5ms, but 2.769ms..7.334ms is not always below 5ms"
        ),
        "{}",
        m[0]
    );
}

/// The bound is exact at the worst-case end (8.8 nC / 1.2 uA is 7.3333 ms), and a lower bound is
/// judged at the other end (7.2 nC / 2.6 uA, the best case, is 2.7692 ms).
#[test]
fn the_bound_is_exact_at_the_ends_of_the_range() {
    let s = lt_run("    .stage.soft_start_time <= 7.34ms\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let s = lt_run("    .stage.soft_start_time <= 7.33ms\n");
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
    let s = lt_run("    .stage.soft_start_time >= 2.76ms\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let s = lt_run("    .stage.soft_start_time >= 2.77ms\n");
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
}

/// The capacitor's tolerance widens the range at both ends: 10 nF +/- 10% is 9 nF to 11 nF, so
/// the time is wider than that of an exact 10 nF, 8 nC / 2.6 uA to 8 nC / 1.2 uA.
#[test]
fn the_capacitors_tolerance_widens_the_time() {
    let s = lt_run("    .stage.soft_start_time <= 8ms\n");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let css = range(&s, "stage.ic.css").unwrap();
    near(css.0, 9e-9);
    near(css.1, 11e-9);
    let t = range(&s, "stage.soft_start_time").unwrap();
    assert!(
        t.0 < 8e-9 / 2.6e-6 && t.1 > 8e-9 / 1.2e-6,
        "{t:?} is not wider than an exact 10 nF's"
    );
}

/// A board that places the IC alone, not through `Buck`, reaches its support capacitor (a part
/// has no private members; ADR 0115 leaves this open) and states another value, which states
/// `.css` through the support section's equation as the default does.
#[test]
fn a_board_that_places_the_ic_alone_can_state_its_soft_start_capacitor() {
    let src = r#"import std::parts::regulator::Lt8641
import std::sources::Rail

board Alone {
    .input: Rail {
        .voltage &= 9V..16V
        .peak &= 36V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .voltage &= 3.3V +/- 4%
        .pos.net &= "VOUT"
        .neg.net &= "GND"
        .current &= 0A..2A
    }
    .u: Lt8641
    .input ~ .u.vin
    .u.sw ~ .output
    .u.fb ~ .output.pos
    .u.soft_start.capacitance &= 22nF +/- 10%
    .u.soft_start_time <= 20ms
}
"#;
    let c = check("lt8641_alone.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &lt_catalogs());
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let t = range(&s, "u.soft_start_time").unwrap();
    near(t.0, 22e-9 * 0.9 * 0.8 / 2.6e-6);
    near(t.1, 22e-9 * 1.1 * 0.8 / 1.2e-6);
}

/// The support section always states `.css`, so a bound on the stage is always judged, never
/// listed as not evaluated: before, a board that placed no capacitor left it unstated (an IC that
/// states no time at all is judged so above, with `Zbk2`).
#[test]
fn a_requirement_is_always_judged_with_the_support_capacitor() {
    let s = lt_run("    .stage.soft_start_time <= 5ms\n");
    assert_eq!(messages(&s).len(), 1, "{:#?}", messages(&s));
    let n = not_evaluated(&s);
    assert!(
        !n.iter()
            .any(|r| r.contains("soft_start") || r.contains("css")),
        "{n:#?}"
    );
}

/// A board that states no requirement is as it was: nothing is said, and a bound that holds
/// lists nothing more and moves no pick.
#[test]
fn a_board_that_states_no_requirement_is_as_it_was() {
    let without = lt_run("");
    let with = lt_run("    .stage.soft_start_time <= 8ms\n");
    for s in [&without, &with] {
        assert!(messages(s).is_empty(), "{:#?}", messages(s));
    }
    // The list of what is not evaluated is the same, with a requirement stated or not.
    assert_eq!(not_evaluated(&with), not_evaluated(&without));
    for path in [
        "stage.inductor.inductance",
        "stage.output_cap.capacitance",
        "stage.output_count",
        "stage.input_count",
        "stage.ripple",
        "stage.ic.soft_start.capacitance",
    ] {
        assert_eq!(
            range(&with, path),
            range(&without, path),
            "{path} moved with a stated requirement"
        );
    }
}
