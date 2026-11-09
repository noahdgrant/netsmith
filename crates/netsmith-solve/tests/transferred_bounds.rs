//! A bound on a value that a part's field is held to reaches the part (ADR 0065, stage 0): the
//! pick honours it, a part that is not a pick is refused with the numbers, and nothing the
//! design did not write is added to the model that comes back. Every value here is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Candidate, Catalog, Catalogs, Solution, Value, solve};

fn resistor(ohms: f64, percent: f64) -> Candidate {
    let label = format!("{ohms}");
    let center = Quantity::literal(ohms, Some("ohm")).unwrap().quantity;
    let tolerance = Quantity::literal(percent, Some("%")).unwrap().quantity;
    Candidate {
        name: format!("{label} {percent}%"),
        label,
        columns: vec![
            (
                "resistance".into(),
                Value::Quantity(Quantity::tolerance(&center, &tolerance, true).unwrap()),
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

fn catalogs(values: &[f64]) -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: values.iter().map(|&v| resistor(v, 1.0)).collect(),
    });
    c
}

const ROWS: [f64; 4] = [330.0, 10_000.0, 22_000.0, 47_000.0];

fn run(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("main.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn picked(s: &Solution) -> Vec<String> {
    s.picks
        .iter()
        .map(|p| format!("{} = {}", s.model.path_of(p.part), p.label))
        .collect()
}

fn range(s: &Solution, path: &str) -> Quantity {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

/// A module whose resistors are held to a member, and a board that states `board_lines`.
fn lamp(board_lines: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor

module Lamp {{
    .balance_resistance: Resistance
    .top: Resistor
    .bottom: Resistor
    .top.resistance &= .balance_resistance
    .bottom.resistance &= .balance_resistance
}}

board B {{
    .lamp: Lamp
{board_lines}
}}
"#
    )
}

#[test]
fn a_lower_bound_on_the_member_steers_the_pick_of_both_resistors() {
    // Without the bound the first row, 330 ohm, is taken twice.
    let plain = run(&lamp(""), &catalogs(&ROWS));
    assert_eq!(picked(&plain), ["lamp.bottom = 330", "lamp.top = 330"]);

    let s = run(
        &lamp("    .lamp.balance_resistance >= 20kohm"),
        &catalogs(&ROWS),
    );
    assert_eq!(messages(&s), Vec::<String>::new());
    assert_eq!(picked(&s), ["lamp.bottom = 22000", "lamp.top = 22000"]);
}

#[test]
fn an_upper_bound_on_the_member_steers_the_pick_too() {
    // The first row that is at most 20 kohm for sure (tolerance included) is 330 ohm, so use
    // a catalog that starts above the bound's reach.
    let rows = [47_000.0, 22_000.0, 10_000.0];
    let s = run(
        &lamp("    .lamp.balance_resistance <= 20kohm"),
        &catalogs(&rows),
    );
    assert_eq!(messages(&s), Vec::<String>::new());
    assert_eq!(picked(&s), ["lamp.bottom = 10000", "lamp.top = 10000"]);
}

#[test]
fn a_bound_with_the_member_on_the_right_of_the_comparison_is_the_same_bound() {
    let s = run(
        &lamp("    20kohm <= .lamp.balance_resistance"),
        &catalogs(&ROWS),
    );
    assert_eq!(messages(&s), Vec::<String>::new());
    assert_eq!(picked(&s), ["lamp.bottom = 22000", "lamp.top = 22000"]);
    let s = run(
        &lamp("    20kohm >= .lamp.balance_resistance"),
        &catalogs(&[47_000.0, 10_000.0]),
    );
    assert_eq!(picked(&s), ["lamp.bottom = 10000", "lamp.top = 10000"]);
}

#[test]
fn the_bound_passes_through_a_chain_of_members() {
    let src = r#"import std::parts::passive::Resistor

module Inner {
    .value: Resistance
    .r: Resistor
    .r.resistance &= .value
}

module Outer {
    .value: Resistance
    .inner: Inner
    .inner.value &= .value
}

board B {
    .outer: Outer
    .outer.value >= 20kohm
}
"#;
    let s = run(src, &catalogs(&ROWS));
    assert_eq!(messages(&s), Vec::<String>::new());
    assert_eq!(picked(&s), ["outer.inner.r = 22000"]);
}

#[test]
fn two_parts_on_one_member_honour_it_whatever_order_they_are_declared_in() {
    let swapped = r#"import std::parts::passive::Resistor

module Lamp {
    .balance_resistance: Resistance
    .bottom: Resistor
    .top: Resistor
    .bottom.resistance &= .balance_resistance
    .top.resistance &= .balance_resistance
}

board B {
    .lamp: Lamp
    .lamp.balance_resistance >= 20kohm
}
"#;
    let a = run(
        &lamp("    .lamp.balance_resistance >= 20kohm"),
        &catalogs(&ROWS),
    );
    let b = run(swapped, &catalogs(&ROWS));
    assert_eq!(picked(&a), picked(&b));
    assert_eq!(picked(&a), ["lamp.bottom = 22000", "lamp.top = 22000"]);
}

#[test]
fn a_bound_that_no_row_meets_is_refused_at_the_pick_with_both_statements() {
    let s = run(
        &lamp("    .lamp.balance_resistance >= 1Mohm"),
        &catalogs(&ROWS),
    );
    let text = messages(&s);
    // Each resistor is a part of its own that no row serves.
    assert_eq!(text.len(), 2, "{text:?}");
    let m = text
        .iter()
        .find(|m| m.contains("`lamp.top`"))
        .expect("the message names the part");
    assert!(
        m.starts_with(
            "no `resistor` candidate satisfies `lamp.top`: the closest of the 4 that fit, 47000 1%, fails because lamp.top.resistance must always be at least 1MΩ, but 46.53kΩ..47.47kΩ is not always above 1MΩ: "
        ),
        "{m}"
    );
    assert!(
        m.contains("it is held to `lamp.balance_resistance` (`.top.resistance &= .balance_resistance`, main.nsm:7)"),
        "{m}"
    );
    assert!(
        m.ends_with("and `.lamp.balance_resistance >= 1Mohm` (main.nsm:13) bounds it"),
        "{m}"
    );
    assert_eq!(s.unresolved.len(), 2);
}

/// A part with no catalog, so nothing is picked for it: its value is the design's alone. The
/// module holds it to a member as well.
fn fixed(board_lines: &str) -> String {
    format!(
        r#"part Shunt {{
    .resistance: Resistance
    .resistance &= 330ohm +/- 1%
}}

module Fixed {{
    .value: Resistance
    .r: Shunt
    .r.resistance &= .value
}}

board B {{
    .fixed: Fixed
{board_lines}
}}
"#
    )
}

#[test]
fn a_part_that_is_not_a_pick_is_refused_after_the_picks_with_the_numbers() {
    let s = run(&fixed("    .fixed.value >= 20kohm"), &catalogs(&ROWS));
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:?}");
    assert_eq!(
        text[0],
        "fixed.r.resistance must always be at least 20kΩ, but 326.7Ω..333.3Ω is not always above 20kΩ: it is held to `fixed.value` (`.r.resistance &= .value`, main.nsm:9), and `.fixed.value >= 20kohm` (main.nsm:14) bounds it"
    );
    // The same bound that the design holds the member to on its own ends in no message at all.
    let s = run(&fixed("    .fixed.value >= 300ohm"), &catalogs(&ROWS));
    assert_eq!(messages(&s), Vec::<String>::new());
}

#[test]
fn a_part_with_a_stated_value_and_a_row_is_refused_where_it_is_picked_as_any_bound_would() {
    // A catalog part with a stated value is a pick of the one row that stands for it. The bound
    // refuses it at the pick, and the check after says what the stated value is: the two
    // messages a bound written on the part itself gives as well.
    let module = |bound: &str| {
        format!(
            r#"import std::parts::passive::Resistor

module Fixed {{
    .value: Resistance
    .r: Resistor {{ .resistance &= 330ohm +/- 1% }}
    .r.resistance &= .value
    {bound}
}}

board B {{
    .fixed: Fixed
    .fixed.value >= 20kohm
}}
"#
        )
    };
    let transferred = run(&module(""), &catalogs(&ROWS));
    let direct = run(&module(".r.resistance >= 20kohm"), &catalogs(&ROWS));
    for s in [&transferred, &direct] {
        let text = messages(s);
        assert!(
            text[0].starts_with("no `resistor` candidate satisfies `fixed.r`"),
            "{text:?}"
        );
        assert!(
            text.iter()
                .any(|m| m.starts_with("fixed.r.resistance must always be at least 20kΩ")),
            "{text:?}"
        );
    }
    assert_eq!(messages(&transferred).len(), 2);
}

#[test]
fn a_member_with_a_stated_range_is_judged_as_before_with_no_second_message() {
    // The range is the domain path: it reaches both parts by propagation. The bound fails on the
    // range as it always did, once, and the solver's own bound adds nothing.
    let s = run(
        &lamp(
            "    .lamp.balance_resistance &= 10kohm..100kohm\n    .lamp.balance_resistance >= 20kohm",
        ),
        &catalogs(&ROWS),
    );
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:?}");
    assert!(
        text[0].starts_with("lamp.balance_resistance must always be at least 20kΩ, but 10kΩ..100kΩ is not always above 20kΩ"),
        "{}",
        text[0]
    );
    assert!(!text[0].contains("bounds it"), "{}", text[0]);
}

#[test]
fn the_other_direction_of_adr_0057_is_unchanged() {
    // The member on the left of `==`: the pick sets the member and the bound judges it.
    let src = r#"import std::parts::passive::Resistor

module Lamp {
    .balance_resistance: Resistance
    .top: Resistor
    .balance_resistance == .top.resistance
}

board B {
    .lamp: Lamp
    .lamp.balance_resistance >= 20kohm
}
"#;
    let s = run(src, &catalogs(&ROWS));
    assert_eq!(messages(&s), Vec::<String>::new());
    assert_eq!(picked(&s), ["lamp.top = 22000"]);
}

#[test]
fn the_solvers_own_bounds_are_not_in_the_model_and_not_among_the_lines_that_wait() {
    let s = run(
        &lamp("    .lamp.balance_resistance >= 20kohm"),
        &catalogs(&ROWS),
    );
    // The design wrote the two links and the bound; the solver's two are gone.
    let c = check(
        "main.nsm",
        &lamp("    .lamp.balance_resistance >= 20kohm"),
        &Loader::std(),
    );
    assert_eq!(
        s.model.relations().len(),
        c.boards[0].model.relations().len(),
        "the solved model has the design's relations and no others"
    );
    // The member is left unset (the bound is not a value), so the design's bound and the links
    // wait, as they did: they are the lines that say the member was never stated.
    assert!(
        s.deferred
            .iter()
            .all(|&i| i < c.boards[0].model.relations().len()),
        "{:?}",
        s.deferred
    );
    let bound = c.boards[0]
        .model
        .relations()
        .iter()
        .position(|r| r.op == netsmith_lang::ast::CmpOp::Ge)
        .expect("the bound is a relation");
    assert!(s.deferred.contains(&bound), "the design's own bound waits");
}

#[test]
fn a_bound_the_pick_already_meets_changes_nothing() {
    // Soundness: the first row meets the bound, so the picks are the ones of the run without it.
    let plain = run(&lamp(""), &catalogs(&ROWS));
    let bounded = run(
        &lamp("    .lamp.balance_resistance >= 100ohm"),
        &catalogs(&ROWS),
    );
    assert_eq!(picked(&plain), picked(&bounded));
    assert_eq!(messages(&bounded), Vec::<String>::new());
    // The model holds the same values, row for row.
    for path in ["lamp.top.resistance", "lamp.bottom.resistance"] {
        assert_eq!(range(&plain, path), range(&bounded, path));
    }
}

#[test]
fn tolerance_counts_for_the_transferred_bound() {
    // 20 kohm +/- 1% is not always at least 20 kohm; the next row is.
    let s = run(
        &lamp("    .lamp.balance_resistance >= 20kohm"),
        &catalogs(&[20_000.0, 22_000.0]),
    );
    assert_eq!(picked(&s), ["lamp.bottom = 22000", "lamp.top = 22000"]);
}

#[test]
fn a_bound_on_a_member_held_by_a_range_of_the_part_is_not_judged_on_the_part() {
    // The part states its own range, which is a claim; `==` puts the member inside what the part
    // allows and not the other way round, so a bound on the member says nothing about the part's
    // range and the solver adds none.
    let src = r#"import std::parts::passive::Resistor

module Lamp {
    .value: Resistance
    .top: Resistor { .resistance &= 1kohm..100kohm }
    .top.resistance == .value
}

board B {
    .lamp: Lamp
    .lamp.value >= 20kohm
}
"#;
    let s = run(src, &catalogs(&ROWS));
    // The first row in the part's own range is taken; nothing was refused for the bound.
    assert_eq!(picked(&s), ["lamp.top = 10000"]);
}

/// A capacitor row whose marked and effective values are plain numbers.
fn cap(name: &str, effective_uf: f64) -> Candidate {
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

#[test]
fn a_row_that_differs_only_in_a_value_nothing_else_reads_is_not_refused_with_its_twin() {
    // `hopeless` tries each row of a part alone, and a row that the design cannot tell from a
    // refused one is not tried again. A bound on a column alone (`a.effective_capacitance >=
    // 1uF`) names no other field, so the column is not among those the design uses, and the 2 uF
    // row looked like its 0.5 uF twin (same rating): the part was called hopeless and the build
    // failed, though the 2 uF row meets the bound. It matters for a group of two parts, and a
    // bound that reaches a part (ADR 0065) can make one.
    let src = r#"import std::parts::passive::Capacitor

board B {
    .a: Capacitor
    .b: Capacitor
    .a.max_voltage == .b.max_voltage
    .a.effective_capacitance >= 1uF
}
"#;
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "capacitor".into(),
        candidates: vec![cap("half", 0.5), cap("two", 2.0)],
    });
    let s = run(src, &c);
    assert_eq!(messages(&s), Vec::<String>::new());
    assert_eq!(picked(&s), ["a = two", "b = half"]);
}
