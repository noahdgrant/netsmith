//! `min(.f)` and `max(.f)` in a relation (ADR 0098): the comparison is made on two points, it
//! waits when a field has no value, and nothing is ever narrowed through it.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// A board of rails with voltages that are ranges, and `statements` after them.
fn board(statements: &str) -> String {
    format!(
        "import std::sources::Rail\nboard B {{\n    .a: Rail {{ .voltage &= 4V..6V }}\n    \
         .b: Rail {{ .voltage &= 4.7V..5.5V }}\n    .c: Rail {{ .voltage &= 12V }}\n    \
         .w: Rail {{ }}\n    {statements}\n}}\n"
    )
}

fn run(statements: &str) -> Solution {
    let src = board(statements);
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::empty())
}

/// The messages that mention a call of `min` or `max`, which are the relation's own.
fn about_ends(s: &Solution) -> Vec<String> {
    s.diagnostics
        .iter()
        .map(|d| d.message.clone())
        .filter(|m| m.contains("min(") || m.contains("max("))
        .collect()
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
fn the_least_of_one_range_is_compared_with_the_greatest_of_another() {
    // `a` is 4 V to 6 V and `b` 4.7 V to 5.5 V. The ordinary comparison needs every value of
    // the left under every value of the right, and refuses; the ends are two numbers, 4 V and
    // 5.5 V, and 4 V is at most 5.5 V.
    let s = run(".a.voltage <= .b.voltage\n    min(.a.voltage) <= max(.b.voltage)");
    let m: Vec<_> = s.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("a.voltage must always be at most b.voltage"),
        "{m:#?}"
    );
    assert!(about_ends(&s).is_empty(), "{m:#?}");
}

#[test]
fn a_rail_surely_above_the_other_fails_and_the_message_names_both_ends() {
    // `c` is 12 V: its least value is above the greatest of `b`, 5.5 V.
    let s = run("min(.c.voltage) <= max(.b.voltage)");
    let m = about_ends(&s);
    assert_eq!(m.len(), 1, "{:#?}", s.diagnostics);
    assert!(
        m[0].contains("min(c.voltage) must be at most max(b.voltage), but 12V is above 5.5V"),
        "{m:#?}"
    );
    // The same numbers the other way round are met.
    let s = run("max(.b.voltage) <= min(.c.voltage)\n    min(.c.voltage) >= max(.b.voltage)");
    assert!(about_ends(&s).is_empty(), "{:#?}", s.diagnostics);
}

#[test]
fn an_end_is_taken_when_the_ends_are_equal() {
    // An end against itself holds with nothing to spare.
    let s = run("min(.a.voltage) <= min(.a.voltage)\n    max(.a.voltage) >= max(.a.voltage)");
    assert!(about_ends(&s).is_empty(), "{:#?}", s.diagnostics);
}

#[test]
fn an_end_has_no_correlation_with_the_same_field_read_whole() {
    // The end is a point, so reading it beside the whole field is the plain comparison of a
    // number with a range, and it never says more than is so for every value.
    let s = run(
        "min(.a.voltage) <= .a.voltage\n    max(.a.voltage) >= .a.voltage\n    \
         min(.a.voltage) <= max(.a.voltage)",
    );
    assert!(s.diagnostics.is_empty(), "{:#?}", s.diagnostics);
    // The greatest is not at most the whole range, which reaches it: refused, since 6 V is
    // above 4 V.
    let s = run("max(.a.voltage) <= .a.voltage");
    assert_eq!(s.diagnostics.len(), 1, "{:#?}", s.diagnostics);
    let s = run("max(.a.voltage) <= min(.a.voltage)");
    assert_eq!(about_ends(&s).len(), 1, "{:#?}", s.diagnostics);
    // A field that is one number has the same end both ways.
    let s = run("max(.c.voltage) <= min(.c.voltage)\n    min(.c.voltage) >= max(.c.voltage)");
    assert!(s.diagnostics.is_empty(), "{:#?}", s.diagnostics);
}

#[test]
fn a_field_with_no_value_makes_the_relation_wait() {
    // `w` has no voltage, so there is no end to read: the relation is listed as not
    // evaluated and is no error. Stated, the same relation is made.
    let without = run("");
    let waiting = run("min(.w.voltage) <= max(.b.voltage)");
    assert!(
        about_ends(&waiting).is_empty(),
        "{:#?}",
        waiting.diagnostics
    );
    assert_eq!(waiting.deferred.len(), without.deferred.len() + 1);
    let listed: Vec<String> = waiting
        .deferred
        .iter()
        .map(|&i| waiting.model.show_expr(&waiting.model.relations()[i].lhs))
        .collect();
    assert!(listed.iter().any(|l| l == "min(w.voltage)"), "{listed:#?}");
    // The right side waits as well.
    let waiting = run("max(.b.voltage) <= max(.w.voltage)");
    assert_eq!(waiting.deferred.len(), without.deferred.len() + 1);
    // With a value it is made, and it fails or passes on the numbers.
    let met = run(".w.voltage &= 5V\n    min(.w.voltage) <= max(.b.voltage)");
    assert_eq!(met.deferred.len(), without.deferred.len());
    assert!(about_ends(&met).is_empty(), "{:#?}", met.diagnostics);
    let missed = run(".w.voltage &= 9V\n    min(.w.voltage) <= max(.b.voltage)");
    assert_eq!(about_ends(&missed).len(), 1, "{:#?}", missed.diagnostics);
}

#[test]
fn no_field_is_narrowed_through_a_relation_that_reads_an_end() {
    // A relation that holds or fails leaves every field as it was stated: the check is made
    // on numbers read from the ranges and gives nothing back to them.
    for statements in [
        "min(.a.voltage) <= max(.b.voltage)",
        "min(.c.voltage) <= max(.b.voltage)",
        "max(.a.voltage) <= min(.c.voltage)",
        "max(.b.voltage) >= min(.a.voltage)",
        "min(.w.voltage) <= 5V",
        "5V >= max(.w.voltage)",
        "max(.a.voltage) >= 5V\n    min(.b.voltage) <= 5V",
    ] {
        let s = run(statements);
        let before = run("");
        for path in ["a.voltage", "b.voltage", "c.voltage", "w.voltage"] {
            assert_eq!(
                domain(&s, path),
                domain(&before, path),
                "`{statements}` changed {path}"
            );
        }
    }
    // The ranges as stated, for the record.
    let s = run("min(.a.voltage) <= max(.b.voltage)");
    assert_eq!(domain(&s, "a.voltage"), "4V..6V");
    assert_eq!(domain(&s, "b.voltage"), "4.7V..5.5V");
    assert_eq!(domain(&s, "w.voltage"), "unset");
}

// ---- nominal(.f) ----------------------------------------------------------------------

/// A board with rails stated with a tolerance, a range with no stated centre and a value.
fn nominal_board(statements: &str) -> String {
    format!(
        "import std::sources::Rail\nboard B {{\n    .n: Rail {{ .voltage &= 3.3V +/- 5% }}\n    \
         .a: Rail {{ .voltage &= 4V..6V }}\n    .c: Rail {{ .voltage &= 12V }}\n    \
         .w: Rail {{ }}\n    {statements}\n}}\n"
    )
}

fn run_nominal(statements: &str) -> Solution {
    let src = nominal_board(statements);
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::empty())
}

fn about_nominal(s: &Solution) -> Vec<String> {
    s.diagnostics
        .iter()
        .map(|d| d.message.clone())
        .filter(|m| m.contains("nominal("))
        .collect()
}

#[test]
fn a_nominal_is_the_value_a_field_is_stated_at_without_its_tolerance() {
    // 3.3 V +/- 5% is 3.135 V to 3.465 V: the plain bound reads the worst case and is refused,
    // the nominal is 3.3 V and meets it. The ends of the range do not average to exactly 3.3 in
    // a float; the comparison is made on the stated value.
    let plain = run_nominal(".n.voltage >= 3.3V");
    assert_eq!(plain.diagnostics.len(), 1, "{:#?}", plain.diagnostics);
    assert!(
        plain.diagnostics[0]
            .message
            .contains("must always be at least 3.3V")
    );
    for statement in [
        "nominal(.n.voltage) >= 3.3V",
        "nominal(.n.voltage) <= 3.3V",
        "3.3V <= nominal(.n.voltage)",
        "nominal(.n.voltage) >= 3.2V",
        "nominal(.n.voltage) <= 3.4V",
    ] {
        let s = run_nominal(statement);
        assert!(
            s.diagnostics.is_empty(),
            "{statement}: {:#?}",
            s.diagnostics
        );
    }
}

#[test]
fn a_nominal_below_the_bound_is_refused_and_the_message_names_nominal() {
    let s = run_nominal("nominal(.n.voltage) >= 3.4V");
    let m = about_nominal(&s);
    assert_eq!(m.len(), 1, "{:#?}", s.diagnostics);
    assert!(
        m[0].contains("nominal(n.voltage) must be at least 3.4V, but 3.3V is below 3.4V"),
        "{m:#?}"
    );
    assert!(m[0].contains("without its tolerance"), "{m:#?}");
    let s = run_nominal("nominal(.n.voltage) <= 3.2V");
    let m = about_nominal(&s);
    assert_eq!(m.len(), 1, "{:#?}", s.diagnostics);
    assert!(
        m[0].contains("nominal(n.voltage) must be at most 3.2V, but 3.3V is above 3.2V"),
        "{m:#?}"
    );
}

#[test]
fn a_range_with_no_stated_centre_has_its_middle_as_nominal() {
    // A range is stored as its two ends, so 4 V to 6 V is read at 5 V, and a value at itself.
    for statement in [
        "nominal(.a.voltage) >= 5V",
        "nominal(.a.voltage) <= 5V",
        "nominal(.c.voltage) >= 12V",
        "nominal(.c.voltage) <= 12V",
    ] {
        let s = run_nominal(statement);
        assert!(
            s.diagnostics.is_empty(),
            "{statement}: {:#?}",
            s.diagnostics
        );
    }
    for statement in [
        "nominal(.a.voltage) >= 5.1V",
        "nominal(.a.voltage) <= 4.9V",
        "nominal(.c.voltage) >= 12.1V",
    ] {
        let s = run_nominal(statement);
        assert_eq!(
            about_nominal(&s).len(),
            1,
            "{statement}: {:#?}",
            s.diagnostics
        );
    }
}

#[test]
fn a_nominal_of_a_field_with_no_value_waits_as_the_ends_do() {
    let without = run_nominal("");
    let waiting = run_nominal("nominal(.w.voltage) >= 3V");
    assert!(
        about_nominal(&waiting).is_empty(),
        "{:#?}",
        waiting.diagnostics
    );
    assert_eq!(waiting.deferred.len(), without.deferred.len() + 1);
    let listed: Vec<String> = waiting
        .deferred
        .iter()
        .map(|&i| waiting.model.show_expr(&waiting.model.relations()[i].lhs))
        .collect();
    assert!(
        listed.iter().any(|l| l == "nominal(w.voltage)"),
        "{listed:#?}"
    );
    // Given a value it is made.
    let met = run_nominal(".w.voltage &= 5V +/- 1%\n    nominal(.w.voltage) >= 5V");
    assert_eq!(met.deferred.len(), without.deferred.len());
    assert!(met.diagnostics.is_empty(), "{:#?}", met.diagnostics);
    let missed = run_nominal(".w.voltage &= 5V +/- 1%\n    nominal(.w.voltage) >= 5.1V");
    assert_eq!(about_nominal(&missed).len(), 1, "{:#?}", missed.diagnostics);
}

#[test]
fn no_field_is_narrowed_through_a_relation_that_reads_a_nominal() {
    let before = run_nominal("");
    for statements in [
        "nominal(.n.voltage) >= 3V",
        "nominal(.n.voltage) >= 4V",
        "nominal(.w.voltage) <= 5V",
        "5V >= nominal(.w.voltage)",
        "nominal(.a.voltage) <= nominal(.c.voltage)",
    ] {
        let s = run_nominal(statements);
        for path in ["n.voltage", "a.voltage", "c.voltage", "w.voltage"] {
            assert_eq!(
                domain(&s, path),
                domain(&before, path),
                "`{statements}` changed {path}"
            );
        }
    }
}
