//! A range with an open end (`1V..`, `..5V`) in a relation: it is compared, tightened and
//! read at its ends like any range, and an end it has not is no value. Every number here is
//! invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// A board of rails: `a` is `4V..` and `b` is `..6V`, `c` is 12 V, and `statements` follow.
fn run(statements: &str) -> Solution {
    let src = format!(
        "import std::sources::Rail\nboard B {{\n    .a: Rail {{ .voltage &= 4V.. }}\n    \
         .b: Rail {{ .voltage &= ..6V }}\n    .c: Rail {{ .voltage &= 12V }}\n    {statements}\n}}\n"
    );
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::empty())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn domain(s: &Solution, path: &str) -> String {
    let f = s.model.field_at(path).unwrap();
    match &s.model.field(f).domain {
        Domain::Unset => "unset".into(),
        Domain::Quantity(q) => q.to_string(),
        Domain::Text(t) => t.clone(),
    }
}

#[test]
fn an_open_range_shows_as_written() {
    let s = run("");
    assert_eq!(domain(&s, "a.voltage"), "4V..");
    assert_eq!(domain(&s, "b.voltage"), "..6V");
}

#[test]
fn a_range_open_above_is_not_always_under_a_limit_but_always_over_one() {
    let s = run(".a.voltage <= 20V");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("a.voltage must always be at most 20V") && m[0].contains("4V.."),
        "{m:#?}"
    );
    let s = run(".a.voltage >= 3V");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    // And the other end the same: `..6V` is under 6 V and not over 1 V.
    assert!(messages(&run(".b.voltage <= 6V")).is_empty());
    assert_eq!(messages(&run(".b.voltage >= 1V")).len(), 1);
}

#[test]
fn the_least_of_an_open_range_is_a_value_and_its_open_end_is_not() {
    // `min(.a.voltage)` is 4 V, so it is at most 5 V.
    let s = run("min(.a.voltage) <= 5V");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    // `max(.a.voltage)` has no value: the relation says so and names the end.
    let s = run("max(.a.voltage) >= 5V");
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("max(a.voltage) >= 5V")
            && m[0].contains("`max` of 4V.. is no value")
            && m[0].contains("open end"),
        "{m:#?}"
    );
    // The same for the other side's least and for a centre.
    let s = run("min(.b.voltage) <= 5V");
    assert!(
        messages(&s)
            .iter()
            .any(|m| m.contains("min(b.voltage)") && m.contains("open end")),
        "{:#?}",
        messages(&s)
    );
    let s = run("nominal(.a.voltage) >= 3V");
    assert!(
        messages(&s)
            .iter()
            .any(|m| m.contains("nominal(a.voltage)") && m.contains("open end")),
        "{:#?}",
        messages(&s)
    );
}

#[test]
fn a_field_that_repeats_in_an_expression_with_an_open_end_is_read_as_it_stands() {
    // `a - a` over a closed range is 0 V (the two readings are one value); over an open one the
    // field is read as two independent values and the difference is `..` of nothing: it is
    // refused as not always at most 0 V, not a failure of the solver.
    let s = run("(.a.voltage - .a.voltage) <= 1V");
    let m = messages(&s);
    assert!(m.len() <= 1, "{m:#?}");
    assert!(!m.iter().any(|m| m.contains("NaN") || m.contains("not a finite")), "{m:#?}");
}

#[test]
fn an_equation_carries_an_open_end_across_fields() {
    // `w` is what `a` plus 1 V leaves it: the range stays open above, from 5 V.
    let s = run(".w: Rail {}\n    .w.voltage == .a.voltage + 1V");
    let m = messages(&s);
    assert!(
        !m.iter()
            .any(|m| m.contains("NaN") || m.contains("not a finite")),
        "{m:#?}"
    );
    assert_eq!(domain(&s, "w.voltage"), "5V..", "{m:#?}");
    // And downwards, from `b`'s `..6V`.
    let s = run(".w: Rail {}\n    .w.voltage == .b.voltage - 1V");
    assert_eq!(domain(&s, "w.voltage"), "..5V", "{:#?}", messages(&s));
}

#[test]
fn a_relation_between_open_fields_narrows_them_without_a_number_it_does_not_have() {
    // `a` is `4V..` and `b` is `..6V`: if `a <= b`, `a` is at most 6 V and `b` at least 4 V.
    let s = run(".a.voltage <= .b.voltage");
    let m = messages(&s);
    assert!(
        !m.iter()
            .any(|m| m.contains("NaN") || m.contains("not a finite")),
        "{m:#?}"
    );
    let (a, b) = (domain(&s, "a.voltage"), domain(&s, "b.voltage"));
    assert!(a == "4V..6V" || a == "4V..", "a = {a}");
    assert!(b == "4V..6V" || b == "..6V", "b = {b}");
}

/// The LED example with the resistor stated as `resistance`, solved against the real catalogs.
fn led_pick(resistance: &str) -> Vec<String> {
    let src = format!(
        "import std::parts::passive::{{Resistor, Led}}\nimport std::sources::Rail\nboard L {{\n    \
         .supply: Rail {{ .voltage &= 5V +/- 5% }}\n    .led: Led {{ .current &= 0mA..15mA }}\n    \
         .r1: Resistor {{ .resistance &= {resistance} }}\n    \
         .supply.pos ~> .r1 ~> .led ~> .supply.neg\n}}\n"
    );
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{resistance}: {:?}", messages(&s));
    s.picks.iter().map(|p| p.candidate.clone()).collect()
}

#[test]
fn a_part_is_picked_from_a_requirement_with_an_open_end() {
    let open = led_pick("330ohm..");
    assert_eq!(open.len(), 2);
    assert_eq!(open, led_pick("330ohm..10kohm"));
}
