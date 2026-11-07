//! A default (`?=`) gives a field a value unless a `&=` states the field (ADR 0051).

use netsmith_check::*;
use netsmith_lang::ast::CmpOp;

/// A module with a defaulted current and a defaulted supply voltage, and a board that uses
/// it. `{board}` is the board's body after `.lamp: Lamp`.
fn source(module_extra: &str, board: &str) -> String {
    format!(
        "\
import std::sources::Rail
import std::parts::passive::Resistor

module Lamp {{
    .supply: Rail
    .current: Current
    .current ?= 10mA..20mA
    .supply.voltage ?= 5V
{module_extra}}}

board B {{
    .lamp: Lamp
    .src: Rail {{ .voltage &= 3V }}
{board}}}
"
    )
}

fn run(src: &str) -> Checked {
    check("test.nsm", src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(c: &Checked, path: &str) -> (f64, f64) {
    let m = &c.board("B").unwrap().model;
    match m.domain_at(path) {
        Some(Domain::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("`{path}` is {other:?}"),
    }
}

fn is_unset(c: &Checked, path: &str) -> bool {
    c.board("B").unwrap().model.domain_at(path) == Some(&Domain::Unset)
}

#[test]
fn a_default_is_the_value_when_nothing_states_the_field() {
    let c = run(&source("", ""));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(range(&c, "lamp.current"), (0.010, 0.020));
    assert_eq!(range(&c, "lamp.supply.voltage"), (5.0, 5.0));
}

#[test]
fn a_board_statement_replaces_the_default_and_need_not_overlap_it() {
    let c = run(&source("", "    .lamp.current &= 1mA..2mA\n"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(range(&c, "lamp.current"), (0.001, 0.002));
    // The other default is untouched.
    assert_eq!(range(&c, "lamp.supply.voltage"), (5.0, 5.0));
}

#[test]
fn the_order_of_statements_never_matters() {
    // The default after the statement in the module, and the board's statement before and
    // after the declaration: the same field either way.
    let first = run(&source("    .current &= 1mA..2mA\n", ""));
    let c = run(&source("", "    .lamp.current &= 1mA..2mA\n"));
    assert_eq!(range(&first, "lamp.current"), (0.001, 0.002));
    assert_eq!(range(&c, "lamp.current"), (0.001, 0.002));
    let before = "board B {\n    .lamp.current &= 1mA..2mA\n    .lamp: Lamp\n}\n";
    let after = "board B {\n    .lamp: Lamp\n    .lamp.current &= 1mA..2mA\n}\n";
    let module = "module Lamp {\n    .current: Current\n    .current ?= 10mA..20mA\n}\n";
    for board in [before, after] {
        let c = run(&format!("{module}{board}"));
        assert!(c.is_ok(), "{}", c.render());
        assert_eq!(range(&c, "lamp.current"), (0.001, 0.002));
    }
    // The module writes its default after its own statement and before it: the same.
    let late = "module Lamp {\n    .current: Current\n    .current &= 1mA..2mA\n    .current ?= 10mA..20mA\n}\nboard B { .lamp: Lamp }\n";
    let early = "module Lamp {\n    .current: Current\n    .current ?= 10mA..20mA\n    .current &= 1mA..2mA\n}\nboard B { .lamp: Lamp }\n";
    for src in [late, early] {
        let c = run(src);
        assert!(c.is_ok(), "{}", c.render());
        assert_eq!(range(&c, "lamp.current"), (0.001, 0.002));
    }
}

#[test]
fn a_value_reached_through_a_relation_or_a_join_does_not_state_the_field() {
    // `==` reaches `lamp.current` and `~` joins its supply to a rail that states 3 V, and
    // the defaults still apply: the solver then judges them against what reaches them.
    let board = "    .lamp.supply ~ .src\n    .load: Rail { .current &= 5mA }\n    .load.current == .lamp.current\n";
    let c = run(&source("", board));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(range(&c, "lamp.current"), (0.010, 0.020));
    assert_eq!(range(&c, "lamp.supply.voltage"), (5.0, 5.0));
}

#[test]
fn a_statement_with_an_expression_counts_as_stating_the_field() {
    let board = "    .load: Rail { .current &= 5mA }\n    .lamp.current &= .load.current\n";
    let c = run(&source("", board));
    assert!(c.is_ok(), "{}", c.render());
    // Left to the solver as a narrowing, and the default is dropped.
    assert!(is_unset(&c, "lamp.current"));
}

#[test]
fn a_default_may_be_an_expression_and_then_waits_for_the_solver_like_a_narrowing() {
    let src = "\
import std::sources::Rail
import std::parts::passive::Resistor

board B {
    .s: Rail { .voltage &= 5V }
    .r: Resistor { .resistance ?= s.voltage / 1mA }
}
";
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    assert!(is_unset(&c, "r.resistance"));
    let rel = m
        .relations()
        .iter()
        .find(|r| matches!(&r.lhs, RExpr::Field(f) if m.path_of_field(*f) == "r.resistance"))
        .expect("the default waits as a narrowing");
    assert_eq!(rel.op, CmpOp::Narrow);
    // With a statement on the field, there is no such relation.
    let stated = src.replace(
        "    .r: Resistor { .resistance ?= s.voltage / 1mA }",
        "    .r: Resistor { .resistance ?= s.voltage / 1mA; .resistance &= 1kohm }",
    );
    let c = run(&stated);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(range(&c, "r.resistance"), (1000.0, 1000.0));
    assert!(
        !c.board("B")
            .unwrap()
            .model
            .relations()
            .iter()
            .any(|r| matches!(&r.lhs, RExpr::Field(f) if c.board("B").unwrap().model.path_of_field(*f) == "r.resistance"))
    );
}

#[test]
fn a_default_is_a_range_a_tolerance_a_number_or_text() {
    let src = "\
import std::parts::passive::Resistor

part Thing {
    .designator &= \"U\"
    .a: Voltage
    .b: Voltage
    .c: Voltage
    .name: String
    .a ?= 1V..2V
    .b ?= 3V +/- 10%
    .c ?= 4V
    .name ?= \"hello\"
    pins {
        .p passive \"P\" \"1\"
    }
}

board B {
    .t: Thing
}
";
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(range(&c, "t.a"), (1.0, 2.0));
    let (lo, hi) = range(&c, "t.b");
    assert!(
        (lo - 2.7).abs() < 1e-9 && (hi - 3.3).abs() < 1e-9,
        "{lo} {hi}"
    );
    assert_eq!(range(&c, "t.c"), (4.0, 4.0));
    let m = &c.board("B").unwrap().model;
    assert_eq!(m.domain_at("t.name"), Some(&Domain::Text("hello".into())));
}

#[test]
fn two_defaults_for_one_field_name_both_lines() {
    // Both in the module (lines 7 and 9 of the source).
    let src = source("    .current ?= 1mA\n", "");
    let c = run(&src);
    // The module is checked on its own too, which says it as `current`, and as `lamp.current`
    // inside the board.
    let m = messages(&c);
    assert_eq!(m.len(), 2, "{m:#?}");
    assert!(
        m[0].contains("`current` has 2 defaults, at test.nsm:7 and test.nsm:9"),
        "{}",
        m[0]
    );
    assert!(
        m[1].contains("`lamp.current` has 2 defaults, at test.nsm:7 and test.nsm:9"),
        "{}",
        m[1]
    );
    // And no value: neither default is used.
    assert!(is_unset(&c, "lamp.current"));
}

#[test]
fn a_default_in_the_module_and_one_in_the_board_are_two_defaults() {
    let c = run(&source("", "    .lamp.current ?= 1mA\n"));
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("`lamp.current` has 2 defaults, at test.nsm:7 and test.nsm:14"),
        "{}",
        m[0]
    );
    // A statement does not make two defaults one.
    let c = run(&source(
        "",
        "    .lamp.current ?= 1mA\n    .lamp.current &= 1mA\n",
    ));
    assert_eq!(messages(&c).len(), 1, "{:#?}", messages(&c));
}

#[test]
fn a_default_on_a_name_with_no_declaration_is_refused_as_a_narrowing_is() {
    let narrow = messages(&run(&source("", "    .lamp.nope &= 1mA\n")));
    let default = messages(&run(&source("", "    .lamp.nope ?= 1mA\n")));
    assert_eq!(narrow.len(), 1, "{narrow:#?}");
    assert_eq!(narrow, default);
    assert!(default[0].contains("nope"), "{}", default[0]);
}

#[test]
fn a_default_of_the_wrong_kind_or_on_an_instance_is_refused() {
    let m = messages(&run(&source("", "    .lamp.current ?= 5V\n")));
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("`lamp.current` holds A, but this value is V"),
        "{}",
        m[0]
    );
    let m = messages(&run(&source("", "    .lamp ?= 5V\n")));
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("`?=` sets a value, but `.lamp` is a module"),
        "{}",
        m[0]
    );
    let m = messages(&run(&source("", "    3V ?= .lamp.supply.voltage\n")));
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("`?=` needs a field on its left"), "{}", m[0]);
}

#[test]
fn a_default_cannot_name_a_net_in_a_module() {
    let m = messages(&run(&source("    .supply.pos.net ?= \"X\"\n", "")));
    assert!(!m.is_empty(), "{m:#?}");
    assert!(
        m.iter().all(|m| m.contains("only a board names nets")),
        "{m:#?}"
    );
}

#[test]
fn a_module_is_checked_on_its_own_with_its_defaults() {
    // Definitions that no board uses are elaborated alone, and their defaults hold.
    let c = run("module Solo {\n    .x: Voltage\n    .x ?= 3V\n    .x &= 2V..4V\n}\n");
    assert!(c.is_ok(), "{}", c.render());
    // A default that contradicts the module's own narrowing is dropped, not a contradiction.
    let c = run("module Solo {\n    .x: Voltage\n    .x ?= 9V\n    .x &= 2V..4V\n}\n");
    assert!(c.is_ok(), "{}", c.render());
}
