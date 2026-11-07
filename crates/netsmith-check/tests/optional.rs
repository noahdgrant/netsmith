//! `optional` on a member declaration (ADR 0074): a relation that reads an optional member
//! that nothing states is skipped, and a member that something states is read as any other.

use netsmith_check::*;

/// An interface with one optional limit, and a stage that checks a value against it.
const LIBRARY: &str = "\
interface Limits {
    /// The largest DC resistance, when the datasheet gives one.
    .max_dcr: Resistance optional
    .min_current: Current
}

module Stage {
    .limits: Limits
    .dcr: Resistance
    .dcr <= .limits.max_dcr
}

module Ic {
    .limits: Limits
}
";

fn run(src: &str) -> Checked {
    check("test.nsm", src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The relations of the board `B` that read `needle`, as text.
fn reading(c: &Checked, needle: &str) -> Vec<String> {
    let m = &c.board("B").unwrap().model;
    m.relations()
        .iter()
        .map(|r| {
            format!(
                "{} {} {}",
                m.show_expr(&r.lhs),
                r.op.symbol(),
                m.show_expr(&r.rhs)
            )
        })
        .filter(|t| t.contains(needle))
        .collect()
}

fn board(body: &str) -> String {
    format!("{LIBRARY}\nboard B {{\n    .stage: Stage\n{body}}}\n")
}

#[test]
fn a_relation_that_reads_an_unstated_optional_member_is_skipped() {
    let c = run(&board(""));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(reading(&c, "max_dcr"), Vec::<String>::new());
    // A member that is not optional keeps its relations, stated or not.
    let src = board("").replace(
        "    .max_dcr: Resistance optional\n",
        "    .max_dcr: Resistance\n",
    );
    let c = run(&src);
    assert_eq!(reading(&c, "max_dcr").len(), 1);
}

#[test]
fn a_statement_on_the_member_keeps_the_relation() {
    let c = run(&board("    .stage.limits.max_dcr &= 0.1ohm\n"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        reading(&c, "max_dcr"),
        ["stage.dcr <= stage.limits.max_dcr"]
    );
}

#[test]
fn a_bound_against_numbers_states_the_member_and_a_bound_against_a_field_does_not() {
    // `<= 0.5ohm` is a statement of what the design needs of the value, as for any range.
    let c = run(&board("    .stage.limits.max_dcr <= 0.5ohm\n"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        reading(&c, "stage.dcr <=").len(),
        1,
        "{:?}",
        reading(&c, "dcr")
    );
    // The relation of the stage is itself a bound with the member alone on a side. It is a
    // check, and a statement on the other operand does not state the member either.
    let c = run(&board("    .stage.dcr &= 0.2ohm\n"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(reading(&c, "max_dcr"), Vec::<String>::new());
}

#[test]
fn a_statement_across_a_join_reaches_the_member() {
    // The IC states the limit on its side of the join: the stage's member is stated.
    let ic = "    .ic: Ic\n    .stage.limits ~ .ic.limits\n";
    let c = run(&board(&format!("{ic}    .ic.limits.max_dcr &= 0.1ohm\n")));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(reading(&c, "max_dcr").len(), 1);
    // The join in the other order, and the statement on the stage's side: the same.
    let c = run(&board(
        "    .ic: Ic\n    .ic.limits ~ .stage.limits\n    .ic.limits.max_dcr &= 0.1ohm\n",
    ));
    assert_eq!(reading(&c, "max_dcr").len(), 1);
    // Neither side states it.
    let c = run(&board(ic));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(reading(&c, "max_dcr"), Vec::<String>::new());
}

#[test]
fn a_default_of_a_module_around_the_member_states_it() {
    let src = format!(
        "{LIBRARY}\nmodule Wrapped {{\n    .stage: Stage\n    .stage.limits.max_dcr ?= 0.3ohm\n}}\nboard B {{\n    .w: Wrapped\n}}\n"
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(reading(&c, "max_dcr").len(), 1);
}

#[test]
fn a_member_worked_out_from_an_unstated_optional_member_is_not_stated() {
    let wrapper = |extra: &str| {
        format!(
            "{LIBRARY}\nmodule Wrapped {{\n    .limit: Resistance optional\n    .stage: Stage\n    .stage.limits.max_dcr == .limit\n}}\nboard B {{\n    .w: Wrapped\n{extra}}}\n"
        )
    };
    // The wrapper forwards a limit that nothing states: both are skipped.
    let c = run(&wrapper(""));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(reading(&c, "max_dcr"), Vec::<String>::new());
    assert_eq!(reading(&c, "limit"), Vec::<String>::new());
    // The board states the wrapper's limit: the whole chain is stated.
    let c = run(&wrapper("    .w.limit &= 0.1ohm\n"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        reading(&c, "max_dcr").len(),
        2,
        "{:?}",
        reading(&c, "max_dcr")
    );
}

#[test]
fn optional_guards_the_member_and_no_other_operand() {
    // Another operand that nothing states leaves the relation not evaluated, as before, once
    // the member is stated.
    let c = run(&board("    .stage.limits.max_dcr &= 0.1ohm\n"));
    let m = &c.board("B").unwrap().model;
    assert_eq!(
        m.domain_at("stage.dcr"),
        Some(&Domain::Unset),
        "the stage's own value is not touched"
    );
    assert_eq!(reading(&c, "stage.dcr <=").len(), 1);
}

// ---- where `optional` is refused ---------------------------------------------------------

fn refused(src: &str) -> Vec<String> {
    let c = run(src);
    assert!(!c.is_ok(), "no error for:\n{src}");
    messages(&c)
}

#[test]
fn optional_on_a_pin_is_refused_and_says_where_the_other_word_goes() {
    let m = refused("module M {\n    .p: Pin optional\n}\nboard B {\n    .m: M\n}\n");
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains(
            "`optional` marks a value that a design may leave unstated, but `.p` is a pin"
        ) && m[0].contains("`pins` row"),
        "{}",
        m[0]
    );
}

#[test]
fn optional_on_an_interface_a_part_or_a_module_is_refused() {
    let src = |ty: &str, defs: &str| {
        format!(
            "import std::sources::Rail\n{defs}\nmodule M {{\n    .x: {ty} optional\n}}\nboard B {{\n    .m: M\n}}\n"
        )
    };
    let m = refused(&src("Rail", ""));
    assert!(
        m.iter()
            .any(|m| m
                .contains("but `.x` is an interface (`Rail`); mark the values inside it instead")),
        "{m:#?}"
    );
    let m = refused(&src("Inner", "module Inner {\n    .v: Voltage\n}\n"));
    assert!(
        m.iter()
            .any(|m| m.contains("`.x` is a module (`Inner`); mark the values inside it instead")),
        "{m:#?}"
    );
}

#[test]
fn optional_in_a_board_is_refused() {
    let m = refused("board B {\n    .limit: Resistance optional\n}\n");
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("a board has no user above it") && m[0].contains("`.limit`"),
        "{}",
        m[0]
    );
}

#[test]
fn a_statement_in_the_declaring_definition_is_refused() {
    for (statement, symbol) in [
        (".max_dcr &= 0.1ohm", "&="),
        (".max_dcr ?= 0.1ohm", "?="),
        (".max_dcr == 0.1ohm", "=="),
        (".max_dcr <= 0.1ohm", "<="),
    ] {
        let src = format!(
            "interface Limits {{\n    .max_dcr: Resistance optional\n    {statement}\n}}\nboard B {{\n    .l: Limits\n}}\n"
        );
        let m = refused(&src);
        assert_eq!(m.len(), 1, "{statement}: {m:#?}");
        assert!(
            m[0].starts_with(&format!(
                "`max_dcr` is `optional` (at test.nsm:2) and is stated here with `{symbol}`, in the definition that declares it"
            )),
            "{}",
            m[0]
        );
    }
    // A check against another member is not a statement.
    let src = "interface Limits {\n    .max_dcr: Resistance optional\n    .typical: Resistance\n    .typical <= .max_dcr\n}\nboard B {\n    .l: Limits\n}\n";
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_use_may_state_the_member_in_the_body_of_its_declaration() {
    // The statement is the user's, not the declaring definition's.
    let src = format!(
        "{LIBRARY}\nboard B {{\n    .stage: Stage {{\n        .limits.max_dcr &= 0.1ohm\n    }}\n}}\n"
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(reading(&c, "max_dcr").len(), 1);
}

// ---- a generic's optional member ---------------------------------------------------------

/// A generic with an optional limit and a check against it, and two providers: one states
/// the limit, the other leaves it.
const GENERIC: &str = "\
generic Receptacle {
    .designator &= \"J\"
    pins {
        .signal passive
    }
    /// The highest frequency it is specified for, when it says.
    .max_frequency: Frequency optional
    .needed: Frequency
    .needed <= .max_frequency
}

part Stated provides Receptacle {
    pins {
        .signal passive \"SIGNAL\" \"1\"
    }
    .max_frequency &= 6GHz
}

part Silent provides Receptacle {
    pins {
        .signal passive \"SIGNAL\" \"1\"
    }
}
";

#[test]
fn a_provider_states_the_optional_member_of_its_generic() {
    // A provider is not the definition that declares the member: its statement counts.
    let src = format!(
        "{GENERIC}\nboard B {{\n    .rf: Stated {{ .needed &= 1GHz }}\n    .rf2: Silent {{ .needed &= 1GHz }}\n}}\n"
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    let text = reading(&c, "max_frequency");
    assert_eq!(text, ["rf.needed <= rf.max_frequency"], "{text:?}");
}

#[test]
fn a_statement_in_the_generic_itself_is_refused() {
    let src = GENERIC.replace(
        "    .needed <= .max_frequency\n",
        "    .needed <= .max_frequency\n    .max_frequency ?= 1GHz\n",
    );
    let src = format!("{src}\nboard B {{\n    .rf: Stated\n}}\n");
    let m = refused(&src);
    assert!(
        m.iter().any(
            |m| m.starts_with("`max_frequency` is `optional` (at test.nsm:")
                && m.contains("is stated here with `?=`")
        ),
        "{m:#?}"
    );
}
