//! The `within` and `route` statements of a physical file (issue #1274): a part within a
//! distance of a pin, and a net routed on one layer. Every refusal names the statement.

use netsmith_check::*;

const HEAD: &str = "\
import std::parts::connector::Header1x2

module Radio {
    .rf: Pin
    .ant: Header1x2
    .ant.p1 ~ .rf
}

board B {
    .radio: Radio
    .j: Header1x2
    .k: Header1x2
    .j.p1 ~ .radio.rf
}
";

fn physical(body: &str) -> Checked {
    let text = format!("physical for B {{\n{body}\n}}\n");
    let loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", &text);
    check("t.nsm", HEAD, &loader)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_within_statement_names_a_part_a_pin_and_a_distance() {
    let c = physical("    .j within 8mm of .radio.ant.p1\n    .k.p2 within 3mm of .radio.ant.p2");
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    let found: Vec<(String, String, String, f64)> = m
        .pin_distances
        .iter()
        .map(|d| {
            (
                m.path_of(d.subject),
                m.path_of(d.subject_part),
                m.path_of(d.pin),
                d.within,
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("j".into(), "j".into(), "radio.ant.p1".into(), 8.0),
            ("k.p2".into(), "k".into(), "radio.ant.p2".into(), 3.0),
        ]
    );
}

#[test]
fn a_route_statement_names_the_net_by_a_pin() {
    let c = physical("    .radio.ant.p1 route one_layer\n    .k.p1 route any_layer");
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    let found: Vec<(String, bool)> = m
        .net_routes
        .iter()
        .map(|r| (m.path_of(r.pin), r.one_layer))
        .collect();
    assert_eq!(
        found,
        [("radio.ant.p1".into(), true), ("k.p1".into(), false)]
    );
}

#[test]
fn a_within_statement_refuses_what_is_not_a_part_and_a_pin() {
    let c = physical("    .radio within 8mm of .j.p1");
    assert_eq!(
        messages(&c),
        ["`.radio within 8mm of .j.p1` is about `.radio`, which is a module; the statement is about a part, or a pin of a part"]
    );
    let c = physical("    .j within 8mm of .radio.ant");
    assert_eq!(
        messages(&c),
        ["`.radio.ant` is a part, and `.j within 8mm of .radio.ant` measures to a pin of a part, such as `.radio.ant`"]
    );
    let c = physical("    .j within 8V of .radio.ant.p1");
    assert_eq!(
        messages(&c),
        ["`within` of `.j within 8V of .radio.ant.p1` is one length above zero, such as `2mm`, and `8V` is not"]
    );
    let c = physical("    .j within 8mm of .j.p2");
    assert_eq!(
        messages(&c),
        ["`.j within 8mm of .j.p2` measures a part to its own pin; name another part, or a pin of another part"]
    );
}

#[test]
fn a_route_statement_refuses_what_is_not_a_pin() {
    let c = physical("    .radio.ant route one_layer");
    assert_eq!(
        messages(&c),
        ["`.radio.ant route one_layer` names the net by a pin on it, and `.radio.ant` is a part"]
    );
}

#[test]
fn both_statements_belong_in_a_physical_file() {
    let source = HEAD.replace(
        "    .j.p1 ~ .radio.rf\n",
        "    .j.p1 ~ .radio.rf\n    .j within 8mm of .radio.ant.p1\n    .j.p1 route one_layer\n",
    );
    let c = check("t.nsm", &source, &Loader::std());
    assert_eq!(
        messages(&c),
        [
            "`.j within 8mm of .radio.ant.p1` is a requirement on where a part goes, which is physical intent (ADR 0048); put it in the board's physical file (`physical for Board { ... }`)",
            "`.j.p1 route one_layer` says how a net's copper runs, which is physical intent (ADR 0048); put it in the board's physical file (`physical for Board { ... }`)",
        ]
    );
}
