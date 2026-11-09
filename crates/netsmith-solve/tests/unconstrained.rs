//! A pick of a part whose value nothing limits is reported (ADR 0065, stage 1a), and so is a
//! value declared in a module or board that a check waits for and nothing states (stage 1b).
//! Every value here is invented.

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
        generic: None,
    }
}

fn capacitor(name: &str, effective_uf: f64) -> Candidate {
    let q =
        |x: f64, unit: &str| Value::Quantity(Quantity::literal(x, Some(unit)).unwrap().quantity);
    Candidate {
        name: name.into(),
        label: name.into(),
        columns: vec![
            ("capacitance".into(), q(effective_uf, "uF")),
            ("effective_capacitance".into(), q(effective_uf, "uF")),
            ("max_voltage".into(), q(25.0, "V")),
            ("package".into(), Value::Text("0402".into())),
            ("dielectric".into(), Value::Text("X7R".into())),
        ],
        lcsc: None,
        mpn: None,
        library: None,
        assembly: None,
        height: None,
        generic: None,
    }
}

fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: [330.0, 10_000.0, 22_000.0, 47_000.0]
            .iter()
            .map(|&v| resistor(v))
            .collect(),
    });
    c.insert(Catalog {
        name: "capacitor".into(),
        candidates: vec![capacitor("half", 0.5), capacitor("two", 2.0)],
    });
    c
}

fn run(src: &str) -> Solution {
    let c = check("main.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], &catalogs())
}

/// The paths of the parts reported as unconstrained.
fn unconstrained(s: &Solution) -> Vec<String> {
    s.unconstrained
        .iter()
        .map(|u| s.model.path_of(u.part))
        .collect()
}

fn unstated(s: &Solution) -> Vec<Vec<String>> {
    s.unstated
        .iter()
        .map(|u| u.fields.iter().map(|&f| s.model.path_of_field(f)).collect())
        .collect()
}

const RAIL: &str =
    r#".supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }"#;

/// A module whose two resistors are held to a member, in a board that states `lines`.
fn lamp(lines: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail

module Lamp {{
    .supply: Rail
    .balance_resistance: Resistance
    .top: Resistor
    .bottom: Resistor
    .top.resistance &= .balance_resistance
    .bottom.resistance &= .balance_resistance
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
}}

board B {{
    {RAIL}
    .lamp: Lamp
    .supply ~ .lamp.supply
{lines}
}}
"#
    )
}

fn single(lines: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail

board B {{
    {RAIL}
    .pull: Resistor
    .supply.pos ~> .pull ~> .supply.neg
{lines}
}}
"#
    )
}

#[test]
fn a_resistor_that_nothing_limits_is_reported_with_the_row_that_was_taken() {
    // Its own power check (`drop * current <= max_power`) is the part's, and does not limit it.
    let s = run(&single(""));
    assert_eq!(unconstrained(&s), ["pull"]);
    let u = &s.unconstrained[0];
    assert_eq!(u.column, "resistance");
    assert_eq!(u.row, "330 1%");
    assert!(u.held_by.is_none());
    assert!(unstated(&s).is_empty());
}

#[test]
fn a_stated_value_a_range_or_a_tolerance_limits_the_pick() {
    for line in [
        "    .pull.resistance &= 4.7kohm +/- 5%",
        "    .pull.resistance &= 10kohm..100kohm",
        "    .pull.resistance &= 330ohm",
    ] {
        let s = run(&single(line));
        assert!(
            s.unconstrained.is_empty(),
            "{line}: {:?}",
            unconstrained(&s)
        );
    }
}

#[test]
fn a_range_of_the_whole_domain_is_the_way_to_say_any() {
    // The finding cannot tell a wide range from a narrow one, and the range is the documented
    // spelling of "any" (ADR 0065).
    let s = run(&single("    .pull.resistance &= 0ohm..10Mohm"));
    assert!(s.unconstrained.is_empty());
}

#[test]
fn two_resistors_held_to_one_member_that_nothing_states_name_the_member() {
    let s = run(&lamp(""));
    assert_eq!(unconstrained(&s), ["lamp.bottom", "lamp.top"]);
    for u in &s.unconstrained {
        let held = u.held_by.as_ref().expect("held to the member");
        assert_eq!(
            s.model.path_of_field(held.member),
            "lamp.balance_resistance"
        );
        let (op, _, _) = held.link.expect("a statement holds it");
        assert_eq!(op, netsmith_lang::ast::CmpOp::Narrow);
    }
}

#[test]
fn a_member_with_a_range_is_silent() {
    let s = run(&lamp("    .lamp.balance_resistance &= 10kohm..100kohm"));
    assert!(s.unconstrained.is_empty(), "{:?}", unconstrained(&s));
    assert!(s.unstated.is_empty());
}

#[test]
fn a_bound_on_the_member_limits_both_picks_through_stage_0() {
    let s = run(&lamp("    .lamp.balance_resistance >= 20kohm"));
    assert!(s.unconstrained.is_empty(), "{:?}", unconstrained(&s));
    // The member is bounded and not valued; the bound was judged on the parts, so it is not a
    // check that waits for a value.
    assert!(s.unstated.is_empty(), "{:?}", unstated(&s));
}

#[test]
fn a_default_that_applied_is_a_statement_of_the_library() {
    let src = lamp("").replace(
        "    .balance_resistance: Resistance\n",
        "    .balance_resistance: Resistance\n    .balance_resistance ?= 4.7kohm +/- 5%\n",
    );
    let s = run(&src);
    assert!(s.unconstrained.is_empty(), "{:?}", unconstrained(&s));
    // A board that states its own value replaces the default, and is silent as well.
    let s = run(&lamp("    .lamp.balance_resistance &= 22kohm +/- 5%"));
    assert!(s.unconstrained.is_empty());
}

#[test]
fn a_check_of_a_value_the_pick_decides_limits_it() {
    // The LED's current is stated, and the equation holds it to what the resistor lets through.
    let src = format!(
        r#"import std::parts::passive::{{Resistor, Led}}
import std::sources::Rail

board B {{
    {RAIL}
    .led: Led {{ .current &= 5mA..15mA; .forward_voltage &= 2V +/- 5% }}
    .r1: Resistor
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}}
"#
    );
    let s = run(&src);
    assert!(s.unconstrained.is_empty(), "{:?}", unconstrained(&s));
}

#[test]
fn a_bound_on_a_value_the_pick_decides_limits_it() {
    let s = run(&single("    .pull.drop <= 3V"));
    // `drop` waits for the current, which nothing states, so the bound cannot be judged while
    // the part is picked and does not limit the pick. The DC analysis solves the resistor across
    // the supply after the pick and judges the bound then, so the pick is not reported: the
    // value it took is held to the bound.
    assert!(s.unconstrained.is_empty(), "{:?}", unconstrained(&s));
    // A bound that can be judged does: the supply's voltage is stated and the drop is worked
    // out from the current that the board states for the resistor.
    let s = run(&single(
        "    .pull.current &= 1mA..2mA\n    .pull.drop <= 3V",
    ));
    assert!(s.unconstrained.is_empty(), "{:?}", unconstrained(&s));
}

#[test]
fn a_bound_on_a_rating_does_not_count() {
    let s = run(&single("    .pull.max_power >= 50mW"));
    assert_eq!(unconstrained(&s), ["pull"]);
}

#[test]
fn a_bound_that_cannot_be_judged_does_not_limit_the_pick() {
    // The bound reads a member that nothing states, so it is not evaluated. The pick is
    // reported, and so is the member.
    let src = format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail

module Gauge {{
    .supply: Rail
    .limit: Voltage
    .r: Resistor
    .r.drop <= .limit
    .supply.pos ~> .r ~> .supply.neg
}}

board B {{
    {RAIL}
    .gauge: Gauge
    .supply ~ .gauge.supply
}}
"#
    );
    let s = run(&src);
    assert_eq!(unconstrained(&s), ["gauge.r"]);
    assert_eq!(unstated(&s), [["gauge.limit"]]);
}

#[test]
fn a_pick_limited_only_by_a_check_in_an_open_circle_is_left_to_the_circle() {
    // The load's voltage is a value of a circle that nothing starts, and a bound on it waits. The
    // resistor is reported by the open circle finding that names the bound, not twice.
    let src = r#"import std::parts::passive::Resistor
import std::sources::Rail

board Circle {
    .supply: Rail {
        .voltage &= 9V..16V
        .pos.net &= "VBATT"
        .neg.net &= "GND"
    }
    .r1: Resistor
    .load: Rail {
        .pos.net &= "VLOAD"
        .neg.net &= "GND"
    }
    .supply.neg ~ .load.neg
    .supply.pos ~> .r1 ~> .load.pos
    .load.voltage == .supply.voltage - .r1.drop
    .load.current == 0.5W / .load.voltage
    .load.voltage >= 3V
}
"#;
    let s = run(src);
    assert!(!s.open_circles.is_empty(), "the circle is open");
    assert!(s.unconstrained.is_empty(), "{:?}", unconstrained(&s));
    // Without the bound only the part's own rating waits on the circle, which is the library's
    // and limits nothing: the resistor is reported.
    let s = run(&src.replace("    .load.voltage >= 3V\n", ""));
    assert_eq!(unconstrained(&s), ["r1"]);
}

#[test]
fn a_pick_that_failed_is_not_reported() {
    // Nothing is picked, so there is no row that was taken.
    let s = run(&single("    .pull.resistance >= 10Mohm"));
    assert!(!s.diagnostics.is_empty());
    assert!(s.unconstrained.is_empty());
}

#[test]
fn each_resistor_that_is_alone_is_a_finding_of_its_own() {
    let src = format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail

board B {{
    {RAIL}
    .a: Resistor
    .b: Resistor
    .supply.pos ~> .a ~> .b ~> .supply.neg
}}
"#
    );
    let s = run(&src);
    assert_eq!(unconstrained(&s), ["a", "b"]);
    assert!(s.unconstrained.iter().all(|u| u.held_by.is_none()));
}

#[test]
fn a_capacitor_is_judged_by_its_marked_and_its_effective_value() {
    let board = |lines: &str| {
        format!(
            r#"import std::parts::passive::Capacitor
import std::sources::Rail

board B {{
    {RAIL}
    .c: Capacitor
    .supply.pos ~> .c ~> .supply.neg
{lines}
}}
"#
        )
    };
    let s = run(&board(""));
    assert_eq!(unconstrained(&s), ["c"]);
    assert_eq!(s.unconstrained[0].column, "capacitance");
    for line in [
        "    .c.capacitance &= 100nF +/- 10%",
        "    .c.effective_capacitance >= 1uF",
    ] {
        let s = run(&board(line));
        assert!(
            s.unconstrained.is_empty(),
            "{line}: {:?}",
            unconstrained(&s)
        );
    }
}

// ---- stage 1b ----------------------------------------------------------------------

const RECEIVER: &str = r#"
module Receiver {
    .line: Pin
    .vih: Voltage
    .vil: Voltage
    .driver_high: Voltage
    .driver_high >= .vih
    .driver_high <= 5.5V
    .vil &= 0.3V
}
"#;

#[test]
fn values_that_a_check_waits_for_are_one_finding_with_the_checks() {
    let s = run(&format!(
        "{RECEIVER}\nboard Probe {{\n    .receiver: Receiver\n}}\n"
    ));
    assert_eq!(unstated(&s), [["receiver.vih", "receiver.driver_high"]]);
    assert_eq!(s.unstated[0].checks.len(), 2);
    // `vil` is stated, and nothing reads it as a check.
    assert!(s.unconstrained.is_empty());
}

#[test]
fn stating_the_values_ends_the_finding() {
    let s = run(&format!(
        "{RECEIVER}\nboard Probe {{\n    .receiver: Receiver\n    .receiver.vih &= 2V\n    .receiver.driver_high &= 3.3V +/- 5%\n}}\n"
    ));
    assert!(s.unstated.is_empty(), "{:?}", unstated(&s));
}

#[test]
fn a_member_that_nothing_reads_is_silent() {
    let s = run(r#"module Quiet {
    .spare: Voltage
    .note: String
}

board Probe {
    .quiet: Quiet
}
"#);
    assert!(s.unstated.is_empty());
}

#[test]
fn a_wrapper_that_forwards_names_its_own_member_and_not_the_inner_one() {
    let s = run(&format!(
        r#"{RECEIVER}
module Wrapper {{
    .dtr_high: Voltage
    .inner: Receiver
    .inner.driver_high == .dtr_high
    .inner.vih &= 2V
}}

board Probe {{
    .wrapper: Wrapper
}}
"#
    ));
    assert_eq!(unstated(&s), [["wrapper.dtr_high"]]);
}

#[test]
fn what_a_rail_a_part_or_an_interface_leaves_open_is_not_reported() {
    // A rail's voltage is stated or joined by the board, a part's unset datasheet value is the
    // library's gap, and neither is declared by a module or a board.
    let s = run(r#"import std::sources::Rail

part Gadget {
    .designator &= "U"
    pins {
        .a passive "A" "1"
    }
    .vf: Voltage
    .vf >= 1V
}

board Probe {
    .supply: Rail { .pos.net &= "VCC"; .neg.net &= "GND" }
    .supply.voltage >= 3V
    .g: Gadget
}
"#);
    assert!(s.unstated.is_empty(), "{:?}", unstated(&s));
}

#[test]
fn a_member_that_an_unconstrained_pick_names_is_named_once() {
    // `balance_resistance` is read by the links that wait for it and holds two picks: the pick
    // finding names it, and the value finding does not.
    let s = run(&lamp(""));
    assert_eq!(s.unconstrained.len(), 2);
    assert!(s.unstated.is_empty(), "{:?}", unstated(&s));
}
