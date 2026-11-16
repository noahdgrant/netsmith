//! A range with an open end, `1V..` and `..5V`: what the checker makes of one, what a
//! narrowing leaves, and what each refusal says. Every value here is invented.

use netsmith_check::*;
use netsmith_quantity::Quantity;

fn run(statements: &str) -> Checked {
    let src = format!(
        "import std::sources::Rail\nboard B {{\n    .a: Rail {{}}\n    .b: Rail {{}}\n    {statements}\n}}\n"
    );
    check("test.nsm", &src, &Loader::std())
}

fn messages(statements: &str) -> Vec<String> {
    run(statements)
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

fn voltage(statements: &str, path: &str) -> Quantity {
    let c = run(statements);
    assert!(c.is_ok(), "{statements}: {}", c.render());
    let m = &c.standalone_boards().next().unwrap().model;
    match &m.field(m.field_at(path).expect("the field")).domain {
        Domain::Quantity(q) => *q,
        other => panic!("{path} is {other:?}"),
    }
}

#[test]
fn an_open_end_is_stated_and_narrowed_like_a_closed_one() {
    let q = voltage(".a.voltage &= 3V..", "a.voltage");
    assert!(q.is_open_above() && !q.is_open_below());
    assert_eq!(q.lo(), 3.0);
    let q = voltage(".a.voltage &= ..5V", "a.voltage");
    assert!(q.is_open_below());
    assert_eq!(q.hi(), 5.0);
    // Two open ends meet in a closed range, or in a narrower open one.
    let q = voltage(".a.voltage &= 3V..\n    .a.voltage &= ..5V", "a.voltage");
    assert_eq!((q.lo(), q.hi()), (3.0, 5.0));
    let q = voltage(".a.voltage &= 3V..\n    .a.voltage &= 4V..", "a.voltage");
    assert!(q.is_open_above());
    assert_eq!(q.lo(), 4.0);
    // A range with an open end may also be a closed range's narrowing.
    let q = voltage(".a.voltage &= 3V..6V\n    .a.voltage &= 4V..", "a.voltage");
    assert_eq!((q.lo(), q.hi()), (4.0, 6.0));
}

#[test]
fn an_open_end_has_the_kind_of_its_number() {
    let m = messages(".a.voltage &= 3mA..");
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("holds V, but this value is A"), "{}", m[0]);
}

#[test]
fn a_range_with_no_end_at_all_is_refused_at_the_parser() {
    let e = netsmith_lang::parse("board B {\n    .a &= ..\n}\n").unwrap_err();
    assert!(e.message.contains("a value on either side of `..`"), "{}", e.message);
}

#[test]
fn a_narrowing_that_misses_an_open_range_is_a_contradiction_that_prints_it() {
    let m = messages(".a.voltage &= 3V..\n    .a.voltage &= ..2V");
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("contradiction on `a.voltage`")
            && m[0].contains("3V..")
            && m[0].contains("..2V"),
        "{}",
        m[0]
    );
}

#[test]
fn arithmetic_on_an_open_range_keeps_it_open() {
    let q = voltage(".a.voltage &= (3V..) + 1V", "a.voltage");
    assert!(q.is_open_above());
    assert_eq!(q.lo(), 4.0);
    let q = voltage(".a.voltage &= 10V - (3V..)", "a.voltage");
    assert!(q.is_open_below());
    assert_eq!(q.hi(), 7.0);
}

#[test]
fn a_tolerance_needs_both_ends() {
    let m = messages(".a.voltage &= (3V..) +/- 5%");
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("cannot give a tolerance to a range with an open end"),
        "{}",
        m[0]
    );
}

#[test]
fn an_open_end_may_hold_a_value_at_its_other_end() {
    // `.b.voltage..` has a value at its low end: the range waits for it and is open above.
    let c = run(".a.voltage &= .b.voltage..\n    .b.voltage &= 3V");
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.standalone_boards().next().unwrap().model;
    let relation = m.relations().last().unwrap();
    assert!(matches!(&relation.rhs, RExpr::Range(Some(_), None)));
    assert_eq!(m.show_expr(&relation.rhs), "(b.voltage..)");
    let mut fields = Vec::new();
    relation.rhs.fields(&mut fields);
    assert_eq!(fields.len(), 1);
}

#[test]
fn an_open_range_has_one_kind_and_arithmetic_checks_it() {
    let m = messages(".a.voltage &= .b.current..");
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("holds V, but this value is A"), "{}", m[0]);
    let m = messages(".a.voltage &= (3V..) + 1mA");
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("cannot add V and A"), "{}", m[0]);
}
