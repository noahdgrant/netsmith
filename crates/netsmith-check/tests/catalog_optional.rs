//! `optional` after a column of a `catalog` statement (ADR 0078): the checker records which
//! columns the part marks, and refuses a mark on a column that is not one of the part's
//! fields and on the key of the catalog.

use netsmith_check::*;

fn run(src: &str) -> Checked {
    check("test.nsm", src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The columns of the first catalog in the board, and the ones marked `optional`, by path.
fn columns(c: &Checked, part: &str) -> (Vec<String>, Vec<String>) {
    let m = &c.boards[0].model;
    let id = m.inst_at(part).expect("the part is placed");
    let cat = m.instance(id).catalog.as_ref().expect("it has a catalog");
    let prefix = format!("{}.", m.path_of(id));
    let name = |&f: &FieldId| {
        let p = m.path_of_field(f);
        p.strip_prefix(&prefix).unwrap_or(&p).to_string()
    };
    (
        cat.fields.iter().map(name).collect(),
        cat.optional.iter().map(name).collect(),
    )
}

fn part(statement: &str) -> String {
    format!(
        "part Zp {{
    .a: Voltage
    .b: Current
    .package: Package
    pins \"SOT-23-5\" {{
        .p passive \"P\" \"1\"
    }}
    {statement}
}}
board B {{
    .x: Zp
}}
"
    )
}

#[test]
fn a_part_marks_the_columns_it_lists_as_optional() {
    let c = run(&part("catalog zp { .a, .b optional, .package }"));
    assert!(c.is_ok(), "{}", c.render());
    let (all, optional) = columns(&c, "x");
    assert_eq!(all, ["a", "b", "package"]);
    assert_eq!(optional, ["b"]);

    let c = run(&part("catalog zp { .a, .b, .package }"));
    assert!(c.is_ok(), "{}", c.render());
    assert!(columns(&c, "x").1.is_empty());
}

#[test]
fn a_column_may_be_marked_over_several_lines_and_with_a_key() {
    let c = run(&part(
        "catalog zp by .package {\n        .a optional,\n        .package,\n        .b optional\n    }",
    ));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(columns(&c, "x").1, ["a", "b"]);
}

#[test]
fn the_standard_tvs_diode_marks_its_breakdown_voltage() {
    let c = run("import std::parts::diode::Tvs\nboard B {\n    .x: Tvs\n}\n");
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(columns(&c, "x").1, ["breakdown_voltage"]);
}

#[test]
fn the_standard_catalogs_mark_the_columns_that_were_optional_before_the_marker() {
    for (generic, module, column) in [
        ("Resistor", "passive", "temperature_coefficient"),
        ("Capacitor", "passive", "dielectric"),
        ("Inductor", "passive", "rms_current"),
    ] {
        let c = run(&format!(
            "import std::parts::{module}::{generic}\nboard B {{\n    .x: {generic}\n}}\n"
        ));
        assert!(c.is_ok(), "{}", c.render());
        assert_eq!(columns(&c, "x").1, [column], "{generic}");
    }
}

#[test]
fn a_column_marked_optional_that_the_part_does_not_have_is_refused() {
    let c = run(&part("catalog zp { .a, .package, .missing optional }"));
    let m = messages(&c);
    assert!(!c.is_ok());
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("missing"), "{}", m[0]);
}

#[test]
fn the_key_of_a_catalog_cannot_be_optional() {
    let c = run(&part("catalog zp by .package { .a, .package optional }"));
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("`.package` is the key of `catalog zp`")
            && m[0].contains("cannot be `optional`"),
        "{}",
        m[0]
    );
}
