//! A board's `defaults` block offers a value to every part or module of a kind on the board
//! that states nothing on the field (issue #625, ADR 0128).

use netsmith_check::*;

fn run(src: &str) -> Checked {
    check("test.nsm", src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn text(c: &Checked, path: &str) -> Option<String> {
    match c.board("B").unwrap().model.domain_at(path) {
        Some(Domain::Text(t)) => Some(t.clone()),
        Some(Domain::Unset) => None,
        other => panic!("`{path}` is {other:?}"),
    }
}

fn notes(c: &Checked) -> Vec<String> {
    c.board("B")
        .unwrap()
        .model
        .default_notes
        .iter()
        .map(|n| n.message.clone())
        .collect()
}

/// `{defaults}` inside a `defaults` block of board `B`, with `{body}` after it.
fn board(defaults: &str, body: &str) -> String {
    format!(
        "\
import std::parts::passive::{{Resistor, Capacitor, CapacitorBank, Led}}
import std::parts::regulator::FeedbackDivider
import std::sources::Rail

module Stage {{
    .r: Resistor
    .c: Capacitor
}}

board B {{
    defaults {{
{defaults}
    }}
{body}
}}
"
    )
}

#[test]
fn a_line_reaches_the_parts_of_its_kind_on_the_board_and_inside_its_modules() {
    let c = run(&board(
        "        Resistor.package ?= \"0402\"",
        "    .r1: Resistor\n    .stage: Stage\n    .c1: Capacitor",
    ));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(text(&c, "r1.package").as_deref(), Some("0402"));
    assert_eq!(text(&c, "stage.r.package").as_deref(), Some("0402"));
    // A capacitor is not a resistor.
    assert_eq!(text(&c, "c1.package"), None);
    assert_eq!(text(&c, "stage.c.package"), None);
}

#[test]
fn a_narrowing_of_the_field_anywhere_replaces_the_line() {
    let module = "module Fixed {\n    .r: Resistor\n    .r.package &= \"0603\"\n}\n";
    let src = board(
        "        Resistor.package ?= \"0402\"",
        "    .r1: Resistor { .package &= \"0805\" }\n    .r2: Resistor\n    .stage: Stage\n    .stage.r.package &= \"1206\"\n    .fixed: Fixed",
    )
    .replacen("board B", &format!("{module}board B"), 1);
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(text(&c, "r1.package").as_deref(), Some("0805"));
    assert_eq!(text(&c, "r2.package").as_deref(), Some("0402"));
    assert_eq!(text(&c, "stage.r.package").as_deref(), Some("1206"));
    assert_eq!(text(&c, "fixed.r.package").as_deref(), Some("0603"));
}

#[test]
fn a_default_on_the_field_itself_wins_over_the_line() {
    // The module's own default and a default on a member of the board are closer to the
    // field than a line about every resistor: they win, and two defaults is no error here.
    let module = "module Soft {\n    .r: Resistor\n    .r.package ?= \"0603\"\n}\n";
    let part = "part Big provides Resistor {\n    .package ?= \"2512\"\n    pins {\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"2\"\n    }\n}\n";
    let src = board(
        "        Resistor.package ?= \"0402\"",
        "    .soft: Soft\n    .r1: Resistor { .package ?= \"0805\" }\n    .big: Big\n    .r2: Resistor",
    )
    .replacen("board B", &format!("{module}{part}board B"), 1);
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(text(&c, "soft.r.package").as_deref(), Some("0603"));
    assert_eq!(text(&c, "r1.package").as_deref(), Some("0805"));
    assert_eq!(text(&c, "big.package").as_deref(), Some("2512"));
    assert_eq!(text(&c, "r2.package").as_deref(), Some("0402"));
}

#[test]
fn a_statement_that_reaches_the_field_through_an_equation_also_replaces_the_line() {
    // `FeedbackDivider` ties both resistors to its `.package` with `==`. A board that states the
    // divider's package must not meet the line on the resistors, which would contradict it.
    let c = run(&board(
        "        Resistor.package ?= \"0402\"",
        "    .fb: FeedbackDivider { .package &= \"0603\" }\n    .free: FeedbackDivider",
    ));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(text(&c, "fb.rtop.package"), None);
    assert_eq!(text(&c, "fb.package").as_deref(), Some("0603"));
    // With nothing stated, the line reaches both resistors of the other divider.
    assert_eq!(text(&c, "free.rtop.package").as_deref(), Some("0402"));
    assert_eq!(text(&c, "free.rbottom.package").as_deref(), Some("0402"));
}

#[test]
fn a_line_may_name_a_generic_a_part_or_a_module() {
    let part = "part Thin provides Resistor {\n    pins {\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"2\"\n    }\n}\npart Wide provides Resistor {\n    pins {\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"2\"\n    }\n}\n";
    let src = board(
        "        Thin.package ?= \"0201\"\n        FeedbackDivider.package ?= \"0603\"\n        Led.package ?= \"0805\"",
        "    .thin: Thin\n    .wide: Wide\n    .fb: FeedbackDivider\n    .led: Led",
    )
    .replacen("board B", &format!("{part}board B"), 1);
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    // A part's name reaches that part only.
    assert_eq!(text(&c, "thin.package").as_deref(), Some("0201"));
    assert_eq!(text(&c, "wide.package"), None);
    // A module's name reaches the module's own value, which its equations carry on.
    assert_eq!(text(&c, "fb.package").as_deref(), Some("0603"));
    assert_eq!(text(&c, "led.package").as_deref(), Some("0805"));
}

#[test]
fn a_generic_reaches_a_project_part_that_provides_it() {
    let part = "part Thin provides Resistor {\n    pins {\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"2\"\n    }\n}\n";
    let src = board("        Resistor.package ?= \"0402\"", "    .thin: Thin")
        .replacen("board B", &format!("{part}board B"), 1);
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(text(&c, "thin.package").as_deref(), Some("0402"));
}

#[test]
fn a_bank_is_its_own_kind_and_its_line_is_the_package_of_each_member() {
    let c = run(&board(
        "        Capacitor.package ?= \"0402\"\n        CapacitorBank.package ?= \"0805\"",
        "    .c1: Capacitor\n    .bulk: CapacitorBank\n    .group: 3 x Capacitor",
    ));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(text(&c, "c1.package").as_deref(), Some("0402"));
    assert_eq!(text(&c, "bulk.package").as_deref(), Some("0805"));
    // Each member of a group is a `Capacitor`.
    for k in 0..3 {
        assert_eq!(
            text(&c, &format!("group[{k}].package")).as_deref(),
            Some("0402")
        );
    }
}

#[test]
fn the_field_records_that_a_line_of_the_board_gave_it() {
    let c = run(&board(
        "        Resistor.package ?= \"0402\"",
        "    .r1: Resistor\n    .r2: Resistor { .package ?= \"0603\" }\n    .r3: Resistor { .package &= \"0805\" }",
    ));
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    let from = |p: &str| m.field(m.field_at(p).unwrap()).default_from;
    assert_eq!(from("r1.package"), Some(DefaultFrom::Board));
    assert_eq!(from("r2.package"), Some(DefaultFrom::Field));
    assert_eq!(from("r3.package"), None);
    // The value's statement is the line, which a message about the package points at.
    let f = m.field(m.field_at("r1.package").unwrap());
    let (source, span) = *f.narrowed_at.last().unwrap();
    let line = c.sources.statement(source, span).unwrap();
    assert_eq!(line, "Resistor.package ?= \"0402\"");
}

#[test]
fn the_order_of_the_block_and_the_members_never_matters() {
    let before = run(&board("        Resistor.package ?= \"0402\"", "    .r1: Resistor"));
    let after = run(
        "import std::parts::passive::Resistor\nboard B {\n    .r1: Resistor\n    defaults { Resistor.package ?= \"0402\" }\n}\n",
    );
    for c in [before, after] {
        assert!(c.is_ok(), "{}", c.render());
        assert_eq!(text(&c, "r1.package").as_deref(), Some("0402"));
    }
}

#[test]
fn a_line_that_reaches_nothing_is_a_note_and_not_an_error() {
    let c = run(&board(
        "        Led.package ?= \"0603\"\n        Capacitor.package ?= \"0402\"",
        "    .r1: Resistor\n    .c1: Capacitor { .package &= \"0805\" }",
    ));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        notes(&c),
        [
            "the default for `Led.package` reaches nothing: the board has no `Led`",
            "the default for `Capacitor.package` reaches nothing: every `Capacitor` on the board states `.package` itself",
        ]
    );
}

#[test]
fn a_kind_that_is_not_in_scope_is_refused_with_the_nearest() {
    let c = run(&board("        Resistr.package ?= \"0402\"", "    .r1: Resistor"));
    assert_eq!(
        messages(&c),
        ["no part, module or generic `Resistr` is in scope; did you mean `Resistor`?"]
    );
}

#[test]
fn a_field_the_kind_lacks_is_refused_with_the_nearest() {
    let c = run(&board("        Resistor.pakage ?= \"0402\"", "    .r1: Resistor"));
    assert_eq!(
        messages(&c),
        ["`Resistor` declares no value `pakage`; did you mean `package`?"]
    );
    // A pin is not a value.
    let c = run(&board("        Resistor.a ?= \"0402\"", "    .r1: Resistor"));
    assert_eq!(c.diagnostics.len(), 1, "{}", c.render());
    assert!(
        messages(&c)[0].starts_with("`Resistor` declares no value `a`"),
        "{}",
        c.render()
    );
}

#[test]
fn an_interface_is_no_kind_a_line_names() {
    let c = run(&board("        Rail.voltage ?= 5V", "    .r1: Resistor"));
    assert_eq!(
        messages(&c),
        ["`Rail` is an interface; a line of `defaults` names a part, a module or a generic"]
    );
}

#[test]
fn the_value_is_checked_against_the_field() {
    let c = run(&board("        Resistor.package ?= 5V", "    .r1: Resistor"));
    assert_eq!(
        messages(&c),
        ["`Resistor.package` holds text, but this value is V"]
    );
    let c = run(&board(
        "        Resistor.resistance ?= .r1.resistance",
        "    .r1: Resistor",
    ));
    assert_eq!(
        messages(&c),
        ["a line of `defaults` offers a value, such as `\"0402\"` or `25V..50V`, not one read from a field"]
    );
}

#[test]
fn a_line_is_a_kind_a_field_and_a_default() {
    let cases = [
        (
            "        Resistor.package &= \"0402\"",
            "a line of `defaults` offers a value with `?=`, such as `Resistor.package ?= \"0402\"`, not `&=`: what a board requires of one part it states on that part",
        ),
        (
            "        .r1.package ?= \"0402\"",
            "`.r1.package` is a member of the board; a line of `defaults` names a kind, such as `Resistor.package ?= \"0402\"`, and a member is stated in the board's body",
        ),
        (
            "        Resistor.place.side ?= \"top\"",
            "a line of `defaults` names a kind and one field it declares, such as `Resistor.package ?= \"0402\"`; `Resistor.place.side` is not that",
        ),
        (
            "        .r2: Resistor",
            "a `defaults` block holds only lines that name a kind, one of its fields and a value, such as `Resistor.package ?= \"0402\"`",
        ),
    ];
    for (line, message) in cases {
        let c = run(&board(line, "    .r1: Resistor"));
        assert_eq!(messages(&c), [message], "{line}");
    }
}

#[test]
fn one_kind_and_field_has_one_line() {
    let c = run(&board(
        "        Resistor.package ?= \"0402\"\n        Resistor.package ?= \"0603\"",
        "    .r1: Resistor",
    ));
    assert_eq!(
        messages(&c),
        ["`Resistor.package` already has a default in this block, at test.nsm:12: keep one line"]
    );
}

#[test]
fn two_lines_that_reach_one_field_are_refused() {
    // A line for the generic and one for a part that provides it both reach the part.
    let part = "part Thin provides Resistor {\n    pins {\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"2\"\n    }\n}\n";
    let src = board(
        "        Resistor.package ?= \"0402\"\n        Thin.package ?= \"0201\"",
        "    .thin: Thin",
    )
    .replacen("board B", &format!("{part}board B"), 1);
    let c = run(&src);
    assert_eq!(c.diagnostics.len(), 1, "{}", c.render());
    let m = &messages(&c)[0];
    assert!(
        m.starts_with("`thin.package` is reached by two lines of `defaults`, this one and the one at test.nsm:"),
        "{m}"
    );
    assert!(m.ends_with("a field takes one default, so state it on the part with `&=`"), "{m}");
}

#[test]
fn only_a_board_holds_defaults() {
    let c = run(
        "import std::parts::passive::Resistor\nmodule M {\n    .r: Resistor\n    defaults { Resistor.package ?= \"0402\" }\n}\nboard B { .m: M }\n",
    );
    assert_eq!(
        messages(&c),
        ["only a board holds `defaults`: it offers a value to every part or module of a kind on the board; a module or a part states its own default with `?=` on its member"]
    );
}

#[test]
fn a_package_no_pin_table_has_is_refused_once_at_the_line() {
    // Two parts the line reaches, one message, at the line (ADR 0118's check).
    let src = "\
part Twin {
    pins \"SOT-23\" {
        .a passive \"A\" \"1\"
    }
    pins \"SOT-223\" {
        .a passive \"A\" \"2\"
    }
    .package: Package
}
board B {
    defaults {
        Twin.package ?= \"SOT23\"
    }
    .x: Twin
    .y: Twin
}
";
    let c = run(src);
    assert_eq!(
        messages(&c),
        ["`Twin.package ?= \"SOT23\"` in the board's `defaults` puts a `Twin` in package `SOT23`, which has no pin table; its tables are for `SOT-23`, `SOT-223`; did you mean \"SOT-23\"?"]
    );
    let d = &c.diagnostics[0];
    assert_eq!(
        c.sources.statement(d.source, d.span).as_deref(),
        Some("Twin.package ?= \"SOT23\"")
    );
}
