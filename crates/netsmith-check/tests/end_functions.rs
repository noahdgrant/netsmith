//! `min(.f)` and `max(.f)` (ADR 0098): what the checker accepts, what it refuses, and what
//! each refusal says.

use netsmith_check::*;

const IMPORTS: &str = "import std::sources::Rail\n";

fn run(statement: &str) -> Checked {
    let src = format!(
        "{IMPORTS}board B {{\n    .a: Rail {{ .voltage &= 4V..6V }}\n    .b: Rail {{ .voltage &= 4.7V..5.5V }}\n    \
         .t: Rail {{ .voltage &= 3V..4V }}\n    {statement}\n}}\n"
    );
    check("test.nsm", &src, &Loader::std())
}

fn messages(statement: &str) -> Vec<String> {
    run(statement)
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

fn one_error(statement: &str, needle: &str) {
    let m = messages(statement);
    assert_eq!(m.len(), 1, "{statement}: {m:#?}");
    assert!(m[0].contains(needle), "`{}` does not say `{needle}`", m[0]);
}

#[test]
fn the_ends_of_two_fields_are_compared_in_a_relation() {
    for statement in [
        "min(.a.voltage) <= max(.b.voltage)",
        "max(.b.voltage) >= min(.a.voltage)",
        "min(.a.voltage) + 1V <= max(.b.voltage) * 2",
        "min(.a.voltage) <= 5V",
        "3V >= max(.t.voltage) - 1V",
    ] {
        let c = run(statement);
        assert!(c.is_ok(), "{statement}: {}", c.render());
        // The relation is in the model, with the field it reads, and it is not a statement of
        // any field: nothing is stated by it.
        let model = &c.standalone_boards().next().unwrap().model;
        let relation = model.relations().last().unwrap();
        let mut fields = Vec::new();
        relation.lhs.fields(&mut fields);
        relation.rhs.fields(&mut fields);
        assert!(!fields.is_empty(), "{statement}");
    }
}

#[test]
fn an_end_has_the_quantity_kind_of_its_field() {
    // A voltage's end is a voltage: it cannot be compared with a current.
    one_error(
        "min(.a.voltage) <= 5mA",
        "the two sides do not match: left is",
    );
    // And it adds with a voltage, not with a current.
    one_error("min(.a.voltage) + 1mA <= 5V", "cannot add");
}

#[test]
fn the_call_is_shown_as_written_in_the_model() {
    let c = run("min(.a.voltage) <= max(.b.voltage)");
    let model = &c.standalone_boards().next().unwrap().model;
    let relation = model.relations().last().unwrap();
    assert_eq!(model.show_expr(&relation.lhs), "min(a.voltage)");
    assert_eq!(model.show_expr(&relation.rhs), "max(b.voltage)");
}

#[test]
fn another_count_of_arguments_is_refused() {
    one_error(
        "min(.a.voltage, .b.voltage) <= 5V",
        "`min` takes one field, such as `min(.bias)`, but this has 2 arguments",
    );
    one_error(
        "min() <= 5V",
        "`min` takes one field, such as `min(.bias)`, but this has no argument",
    );
    let m = messages("max(.a.voltage, .b.voltage) >= 1V");
    assert!(m[0].contains("not the larger of several values"), "{m:#?}");
}

#[test]
fn an_argument_that_is_not_a_field_is_refused() {
    one_error(
        "min(.a.voltage + 1V) <= 5V",
        "the argument of `min` is a field, such as `min(.bias)`, not `.a.voltage + 1V`",
    );
    one_error("max(5V) >= 1V", "the argument of `max` is a field");
    one_error(
        "max((.a.voltage)) >= 1V",
        "the argument of `max` is a field",
    );
}

#[test]
fn an_argument_that_is_text_is_refused() {
    one_error(
        "min(.a.pos.net) <= 5V",
        "`min` reads the end of a number's range, but `.a.pos.net` is text",
    );
}

#[test]
fn a_name_that_is_not_a_function_is_refused() {
    one_error(
        "abs(.a.voltage) <= 5V",
        "`abs` is not a function; the functions are `min(.field)` and `max(.field)`",
    );
}

#[test]
fn an_unknown_field_is_named() {
    one_error("min(.a.nothing) <= 5V", "nothing");
}

#[test]
fn the_ends_are_read_only_in_a_less_or_greater_statement() {
    // Nothing is worked out from the end of a range: `==`, `&=` and `?=` would give a field a
    // value from it, so they refuse it, and so does anything that is not a relation.
    let said = "`max(.b.voltage)` can only be used in a `<=` or `>=` statement";
    one_error(".a.voltage == max(.b.voltage)", said);
    one_error(".a.voltage &= max(.b.voltage)", said);
    one_error(".a.voltage ?= max(.b.voltage)", said);
    one_error("max(.b.voltage) == .a.voltage", said);
}

#[test]
fn nominal_is_read_in_a_less_or_greater_statement_like_the_ends() {
    for statement in [
        "nominal(.a.voltage) >= 2V",
        "3V <= nominal(.b.voltage)",
        "nominal(.a.voltage) + 1V <= nominal(.b.voltage) * 2",
        "nominal(.a.voltage) <= max(.b.voltage)",
    ] {
        let c = run(statement);
        assert!(c.is_ok(), "{statement}: {}", c.render());
    }
    let c = run("nominal(.a.voltage) >= 2V");
    let model = &c.standalone_boards().next().unwrap().model;
    let relation = model.relations().last().unwrap();
    assert_eq!(model.show_expr(&relation.lhs), "nominal(a.voltage)");
    let mut fields = Vec::new();
    relation.lhs.fields(&mut fields);
    assert_eq!(fields.len(), 1);
}

#[test]
fn nominal_is_refused_where_a_value_would_be_taken_from_it() {
    // A nominal is a point read from a range: `==`, `&=` and `?=` would give a field a value
    // from it.
    let said = "`nominal(.b.voltage)` can only be used in a `<=` or `>=` statement: it reads the centre of a range";
    one_error(".a.voltage == nominal(.b.voltage)", said);
    one_error(".a.voltage &= nominal(.b.voltage)", said);
    one_error(".a.voltage ?= nominal(.b.voltage)", said);
    one_error("nominal(.b.voltage) == .a.voltage", said);
}

#[test]
fn nominal_takes_one_field() {
    one_error(
        "nominal(.a.voltage, .b.voltage) >= 1V",
        "`nominal` takes one field, such as `nominal(.resistance)`, but this has 2 arguments",
    );
    one_error("nominal() >= 1V", "`nominal` takes one field");
    one_error(
        "nominal(.a.voltage + 1V) >= 1V",
        "the argument of `nominal` is a field, such as `nominal(.bias)`, not `.a.voltage + 1V`",
    );
    one_error("nominal(5V) >= 1V", "the argument of `nominal` is a field");
    one_error(
        "nominal(.a.pos.net) >= 1V",
        "`nominal` reads the centre of a number's range, but `.a.pos.net` is text",
    );
    one_error("nominal(.a.nothing) >= 1V", "nothing");
    one_error(
        "nominal(.a.voltage) >= 5mA",
        "the two sides do not match: left is",
    );
}

#[test]
fn the_list_of_functions_in_the_refusal_names_nominal() {
    one_error(
        "abs(.a.voltage) <= 5V",
        "the functions are `min(.field)` and `max(.field)`, the least and the greatest value of a field's range, and `nominal(.field)`, its centre",
    );
}
