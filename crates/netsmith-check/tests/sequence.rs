//! A board's `sequence` (issue #1267): the events it names are rails and pins, its delays are
//! times, and the orders it states are kept in the model for the solver.

use netsmith_check::*;

const IMPORTS: &str = "import std::sources::Rail\nimport std::parts::passive::Resistor\n";

fn checked(body: &str) -> Checked {
    let src = format!(
        "{IMPORTS}\nboard B {{\n    .a: Rail\n    .b: Rail\n    .r: Resistor\n    .a.pos ~ .r.a\n{body}\n}}\n"
    );
    check("test.nsm", &src, &Loader::std())
}

fn refused(body: &str) -> String {
    let c = checked(body);
    assert!(!c.is_ok(), "`{body}` was accepted");
    c.diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_orders_of_a_sequence_are_in_the_model() {
    let c = checked("    sequence { .a; then 6ms; .b; after .r.a }");
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    let orders: Vec<(String, String, f64)> = m
        .sequence()
        .iter()
        .map(|o| (o.earlier.written.clone(), o.later.written.clone(), o.delay))
        .collect();
    assert_eq!(
        orders,
        [
            (".a".to_string(), ".b".to_string(), 6e-3),
            (".r.a".to_string(), ".b".to_string(), 0.0),
        ]
    );
    assert_eq!(m.sequence()[0].earlier.kind, EventKind::Rail);
    assert_eq!(m.sequence()[1].earlier.kind, EventKind::Pin);
}

#[test]
fn an_event_that_is_neither_a_rail_nor_a_pin_is_refused_with_what_to_write() {
    let m = refused("    sequence { .a; then; .r }");
    assert!(m.contains("`.r` is a part (`Resistor`)") && m.contains("a pin that goes high"), "{m}");
    let m = refused("    sequence { .a; then; .a.voltage }");
    assert!(m.contains("`.a.voltage` is a value"), "{m}");
}

#[test]
fn a_delay_is_a_time_of_more_than_nothing() {
    let m = refused("    sequence { .a; then 6V; .b }");
    assert!(m.contains("a time, such as `then 6ms`, and `6V` is not one"), "{m}");
    let m = refused("    sequence { .a; then 0ms; .b }");
    assert!(m.contains("write `then` alone"), "{m}");
}

#[test]
fn an_event_cannot_come_after_itself() {
    let m = refused("    sequence { .a; then 1ms; .a }");
    assert!(m.contains("`.a` cannot come after itself"), "{m}");
}

#[test]
fn only_a_board_states_a_sequence() {
    let src = format!(
        "{IMPORTS}\nmodule M {{\n    .a: Rail\n    .b: Rail\n    sequence {{ .a; then; .b }}\n}}\n"
    );
    let c = check("test.nsm", &src, &Loader::std());
    let m: Vec<String> = c.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert!(
        m.iter().any(|m| m.contains("`sequence` is a statement of a board")),
        "{m:#?}"
    );
}
