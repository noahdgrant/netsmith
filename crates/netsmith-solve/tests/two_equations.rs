//! A value that two equations state is held to the window the board stated for it, whatever
//! range the part's own member has (ADR 0065, section 10). An invented oscillator: the frequency
//! is `K / (rt + c)` times a tolerance, the board states a window for it, and the resistor that
//! sets it is picked. Every value here is invented.

use netsmith_check::{Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Candidate, Catalog, Catalogs, Solution, Value, solve};

fn resistor(ohms: f64) -> Candidate {
    let label = format!("{ohms}");
    let center = Quantity::literal(ohms, Some("ohm")).unwrap().quantity;
    let one_percent = Quantity::literal(1.0, Some("%")).unwrap().quantity;
    Candidate {
        name: format!("{label} 1%"),
        label,
        columns: vec![
            (
                "resistance".into(),
                Value::Quantity(Quantity::tolerance(&center, &one_percent, true).unwrap()),
            ),
            ("package".into(), Value::Text("0402".into())),
            (
                "max_power".into(),
                Value::Quantity(Quantity::literal(1.0, Some("kW")).unwrap().quantity),
            ),
        ],
        lcsc: None,
        mpn: None,
        library: None,
        assembly: None,
        height: None,
        temp_min: None,
        temp_max: None,
        qualification: None,
        generic: None,
    }
}

fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: [12_000.0, 15_000.0, 22_000.0, 47_000.0, 100_000.0]
            .iter()
            .map(|&v| resistor(v))
            .collect(),
    });
    c
}

fn run(src: &str) -> Solution {
    let c = check("main.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], &catalogs())
}

/// The oscillator module. `range` is the line that gives the member `rt_resistance` a range, or
/// nothing; `window` is what the board states for `.timer.fsw`.
fn source(range: &str, window: &str, timer_lines: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail

module Oscillator {{
    .rt_resistance: Resistance
{range}
    .fsw_nominal: Frequency
    .fsw_nominal == 46.5MHz * 1kohm / (.rt_resistance + 5.2kohm)
    .fsw_tolerance: Number
    .fsw_tolerance &= 0.925..1.075
    .fsw: Frequency
    .fsw == .fsw_nominal * .fsw_tolerance
}}

module Timer {{
    .fsw: Frequency
    .supply: Rail
    .osc: Oscillator
    .timing: Resistor
    .supply.pos ~> .timing ~> .supply.neg
    .osc.rt_resistance == .timing.resistance
    .osc.fsw == .fsw
{timer_lines}
}}

board Probe {{
    .supply: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }}
    .timer: Timer
    .timer.supply ~ .supply
    .timer.fsw &= {window}
}}
"#
    )
}

const RANGE: &str = "    .rt_resistance &= 10kohm..230kohm";

fn picked(s: &Solution) -> Vec<String> {
    s.picks
        .iter()
        .map(|p| format!("{} = {}", s.model.path_of(p.part), p.label))
        .collect()
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_window_on_the_frequency_picks_the_resistor_that_meets_it() {
    // 47 kohm gives 0.82 MHz to 0.96 MHz with both tolerances; 10 kohm gives 3 MHz.
    for range in [RANGE, ""] {
        let s = run(&source(range, "0.8MHz..1.2MHz", ""));
        assert!(messages(&s).is_empty(), "{range:?}: {:?}", messages(&s));
        assert_eq!(picked(&s), ["timer.timing = 47000"], "{range:?}");
    }
}

#[test]
fn the_range_on_the_member_does_not_change_the_result() {
    // Statement order and the member's range are not what picks the part.
    let with = run(&source(RANGE, "1.7MHz..2.6MHz", ""));
    let without = run(&source("", "1.7MHz..2.6MHz", ""));
    assert_eq!(picked(&with), picked(&without));
    assert_eq!(picked(&with), ["timer.timing = 15000"]);
    assert_eq!(messages(&with), messages(&without));
}

#[test]
fn no_row_in_the_window_fails_with_the_numbers_whether_or_not_the_member_has_a_range() {
    // 0.2 MHz needs about 230 kohm and the catalog stops at 100 kohm.
    for range in [RANGE, ""] {
        let s = run(&source(range, "0.2MHz..0.22MHz", ""));
        let m = messages(&s);
        assert!(
            m.iter().any(
                |m| m.contains("timer.timing") && m.contains("must stay within 200kHz..220kHz")
            ),
            "{range:?}: {m:#?}"
        );
        assert!(s.picks.is_empty(), "{range:?}: {:?}", picked(&s));
    }
}

#[test]
fn a_window_that_a_row_only_partly_meets_is_refused_too() {
    // 47 kohm gives 0.82 to 0.96 MHz: the window 0.9 to 1 MHz holds only for some values.
    for range in [RANGE, ""] {
        let s = run(&source(range, "0.9MHz..1MHz", ""));
        assert!(
            messages(&s).iter().any(|m| m.contains("timer.timing")),
            "{range:?}: {:?}",
            messages(&s)
        );
        assert!(s.picks.is_empty(), "{range:?}: {:?}", picked(&s));
    }
}

#[test]
fn a_resistor_the_board_states_is_judged_against_the_window() {
    // The resistor is stated, so there is no pick left to blame: its 15 kohm gives about
    // 2.3 MHz, and the window says 0.9 MHz to 1.1 MHz.
    let lines = "    .timing.resistance &= 15kohm +/- 1%";
    for range in [RANGE, ""] {
        let s = run(&source(range, "0.9MHz..1.1MHz", lines));
        assert!(
            messages(&s)
                .iter()
                .any(|m| m.contains("900kHz..1.1MHz") && m.contains("2.1")),
            "{range:?}: {:?}",
            messages(&s)
        );
    }
}

#[test]
fn a_stated_resistor_that_meets_the_window_passes() {
    let lines = "    .timing.resistance &= 47kohm +/- 1%";
    for range in [RANGE, ""] {
        let s = run(&source(range, "0.8MHz..1.2MHz", lines));
        assert!(messages(&s).is_empty(), "{range:?}: {:?}", messages(&s));
    }
}

#[test]
fn two_equations_that_no_pick_changes_keep_the_old_rule() {
    // No part is picked and nothing here is fed by a pick: `.x` is what its first equation says
    // (`1V..3V`) and the second equation's `1.5V..2V` lies inside it, so the board builds, as it
    // did before section 9. The equation that comes later is not made to narrow the value that
    // the earlier one gave (ADR 0065, section 9.1, "Not done").
    let src = r#"board P1 {
    .a: Voltage
    .b: Voltage
    .x: Voltage
    .a &= 1V..3V
    .b &= 1.5V..2V
    .x == .a
    .x == .b
}
"#;
    let s = run(src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(s.picks.is_empty());
}
