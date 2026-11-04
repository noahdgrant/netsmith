//! The `row` statement of a physical file (ADR 0048, "Syntax decisions" 11): it lowers to
//! the relations the equation form states, and every refusal names the statement.

use netsmith_check::*;

const HEAD: &str = "\
import std::parts::connector::Header1x2
import std::parts::passive::Resistor

module Pads {
    .tp1: Header1x2
    .tp2: Header1x2
    .tp3: Header1x2
    .tp4: Header1x2
    .tp9: Header1x2
    .tp10: Header1x2
    .tp11: Header1x2
    .tp01: Header1x2
    .tp02: Header1x2
    .tp03: Header1x2
    .gap1: Header1x2
    .gap3: Header1x2
    .tpa: Header1x2
    .inner: Resistor {}
}

module Other {
    .tp5: Header1x2
}

board B {
    .pads: Pads
    .other: Other
    .p1: Header1x2
    .p2: Header1x2
    .p3: Header1x2
}
";

const PHYS_HEAD: &str = "import std::parts::connector::Header1x2\n";

fn physical(body: &str) -> Checked {
    let text = format!("{PHYS_HEAD}physical for B {{\n{body}\n}}\n");
    let loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", &text);
    check("t.nsm", HEAD, &loader)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The relations the physical file states, as text, in sorted order.
fn relations(c: &Checked) -> Vec<String> {
    let m = &c.boards[0].model;
    let mut found: Vec<String> = m
        .relations()
        .iter()
        .filter(|r| c.sources.get(r.source).name == "t.physical.nsm")
        .map(|r| {
            format!(
                "{} {} {}",
                m.show_expr(&r.lhs),
                r.op.symbol(),
                m.show_expr(&r.rhs)
            )
        })
        .collect();
    found.sort();
    found
}

#[test]
fn a_row_lowers_to_the_relations_of_the_equation_form() {
    let row = physical("    row .pads.tp1 .. .pads.tp4 along x pitch 2.54mm +/- 0.05mm");
    assert!(row.is_ok(), "{}", row.render());
    let mut lines = String::new();
    for (a, b) in [(1, 2), (2, 3), (3, 4)] {
        lines.push_str(&format!(
            "    .pads.tp{b}.place.x - .pads.tp{a}.place.x == 2.54mm +/- 0.05mm\n    .pads.tp{b}.place.y == .pads.tp{a}.place.y\n"
        ));
    }
    let equations = physical(&lines);
    assert!(equations.is_ok(), "{}", equations.render());
    assert_eq!(relations(&row).len(), 6);
    assert_eq!(relations(&row), relations(&equations));
}

#[test]
fn a_row_along_y_holds_x_in_common() {
    let c = physical("    row .pads.tp1 .. .pads.tp3 along y pitch 5mm");
    assert!(c.is_ok(), "{}", c.render());
    let r = relations(&c);
    assert_eq!(r.len(), 4, "{r:?}");
    assert!(
        r.iter()
            .any(|l| l.contains("pads.tp2.place.y - pads.tp1.place.y")),
        "{r:?}"
    );
    assert!(
        r.iter()
            .any(|l| l.starts_with("pads.tp3.place.x") && l.contains("== pads.tp2.place.x")),
        "{r:?}"
    );
}

#[test]
fn a_row_states_each_relation_at_the_line_of_the_row_statement() {
    let c = physical("\n    row .pads.tp1 .. .pads.tp3 along x pitch 2.54mm");
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    let spans: Vec<_> = m
        .relations()
        .iter()
        .filter(|r| c.sources.get(r.source).name == "t.physical.nsm")
        .map(|r| r.span)
        .collect();
    assert_eq!(spans.len(), 4);
    assert!(spans.iter().all(|s| *s == spans[0]), "{spans:?}");
}

#[test]
fn a_row_runs_in_natural_order_through_ten() {
    let c = physical("    row .pads.tp9 .. .pads.tp11 along x pitch 2.54mm");
    assert!(c.is_ok(), "{}", c.render());
    let r = relations(&c);
    assert!(
        r.iter()
            .any(|l| l.contains("pads.tp10.place.x - pads.tp9.place.x")),
        "{r:?}"
    );
    assert!(
        r.iter()
            .any(|l| l.contains("pads.tp11.place.x - pads.tp10.place.x")),
        "{r:?}"
    );
}

#[test]
fn a_zero_padded_run_keeps_the_width_of_its_first_name() {
    let c = physical("    row .pads.tp01 .. .pads.tp03 along x pitch 2.54mm");
    assert!(c.is_ok(), "{}", c.render());
    let r = relations(&c);
    assert!(
        r.iter()
            .any(|l| l.contains("pads.tp02.place.x - pads.tp01.place.x")),
        "{r:?}"
    );
}

#[test]
fn a_row_of_the_boards_own_parts_has_no_prefix() {
    let c = physical("    row .p1 .. .p3 along x pitch 2.54mm");
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(relations(&c).len(), 4);
}

#[test]
fn a_gap_names_the_missing_part() {
    let c = physical("    row .pads.gap1 .. .pads.gap3 along x pitch 2.54mm");
    assert_eq!(
        messages(&c),
        [
            "`row .pads.gap1 .. .pads.gap3` has a gap: `pads` has no member `gap2`, and a row is every number from the first to the last"
        ]
    );
}

#[test]
fn a_gap_in_a_long_run_names_the_first_missing_part() {
    let c = physical(
        "    row .pads.tp1 .. .pads.tp4 along x pitch 2.54mm\n    row .pads.tp9 .. .pads.tp11 along x pitch 2.54mm\n    row .pads.tp1 .. .pads.tp11 along x pitch 2.54mm",
    );
    assert_eq!(
        messages(&c),
        [
            "`row .pads.tp1 .. .pads.tp11` has a gap: `pads` has no member `tp5`, and a row is every number from the first to the last"
        ]
    );
}

#[test]
fn a_missing_end_is_named() {
    let c = physical("    row .pads.tp1 .. .pads.tp12 along x pitch 2.54mm");
    assert_eq!(
        messages(&c),
        ["`row .pads.tp1 .. .pads.tp12` names `tp12`, and `pads` has no member of that name"]
    );
}

#[test]
fn a_first_number_above_the_last_is_refused() {
    let c = physical("    row .pads.tp4 .. .pads.tp1 along x pitch 2.54mm");
    assert_eq!(
        messages(&c),
        [
            "`row .pads.tp4 .. .pads.tp1` starts at 4 and ends at 1; write the lower number first (`tp1 .. tp4`)"
        ]
    );
}

#[test]
fn a_run_of_one_part_is_refused() {
    let c = physical("    row .pads.tp2 .. .pads.tp2 along x pitch 2.54mm");
    assert_eq!(
        messages(&c),
        [
            "`row .pads.tp2 .. .pads.tp2` is one part; a row is two or more parts, so a pitch has something to hold"
        ]
    );
}

#[test]
fn an_end_with_no_number_is_refused() {
    let c = physical("    row .pads.tp1 .. .pads.tpa along x pitch 2.54mm");
    assert_eq!(
        messages(&c),
        [
            "`row .pads.tp1 .. .pads.tpa` needs a numbered run of parts, and `tpa` does not end in a number (`tp1 .. tp12`)"
        ]
    );
}

#[test]
fn ends_with_other_names_are_refused() {
    let c = physical("    row .pads.tp1 .. .pads.gap3 along x pitch 2.54mm");
    assert_eq!(
        messages(&c),
        [
            "`row .pads.tp1 .. .pads.gap3` joins `tp1` and `gap3`, which do not share a name before the number; a row is a run such as `tp1 .. tp12`"
        ]
    );
}

#[test]
fn parts_of_different_modules_are_refused() {
    let c = physical("    row .pads.tp4 .. .other.tp5 along x pitch 2.54mm");
    assert_eq!(
        messages(&c),
        [
            "`row .pads.tp4 .. .other.tp5` joins parts of different modules: `tp4` is in `pads` and `tp5` is in `other`; a row is the run of one module's parts"
        ]
    );
}

#[test]
fn a_module_in_the_run_is_not_a_part() {
    let text = "\
import std::parts::connector::Header1x2
module Cell {}
board M {
    .c1: Cell
    .c2: Cell
}
";
    let phys = "physical for M {\n    row .c1 .. .c2 along x pitch 2mm\n}\n";
    let loader = Loader::std().with_physical("m.nsm", "m.physical.nsm", phys);
    let c = check("m.nsm", text, &loader);
    assert_eq!(
        messages(&c),
        [
            "`row .c1 .. .c2` includes `c1`, which is a module; a row is of parts (a relation is between the places of parts, not of a module's box)"
        ]
    );
}

#[test]
fn a_pitch_that_is_not_a_length_is_one_message() {
    let c = physical("    row .pads.tp1 .. .pads.tp4 along x pitch 5V");
    assert_eq!(
        messages(&c),
        ["the pitch of `row .pads.tp1 .. .pads.tp4` is a length such as `2.54mm`, and `5V` is not"]
    );
}

#[test]
fn a_row_in_the_circuit_source_says_it_belongs_in_a_physical_file() {
    let text = "import std::parts::connector::Header1x2\nboard B {\n    .p1: Header1x2\n    .p2: Header1x2\n    row .p1 .. .p2 along x pitch 2mm\n}\n";
    let c = check("t.nsm", text, &Loader::std());
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].starts_with("a `row` statement belongs in a physical file"),
        "{}",
        m[0]
    );
}
