//! Lists of pins whose number is not written, `.lines: [Pin]` (issue #1252): a member is made
//! for each index a statement names, and what the checker refuses.

use netsmith_check::*;

fn run(src: &str) -> Checked {
    check("t.nsm", src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

const GROUP: &str = "import std::sources::LengthGroup\n";

#[test]
fn a_list_has_a_member_for_each_index_named_and_no_other() {
    let c = run(&format!(
        "{GROUP}board B {{\n    .lane: LengthGroup {{ .lines[0].net &= \"D0\" }}\n    .lane.lines[3].net &= \"D3\"\n}}\n"
    ));
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    assert_eq!(
        m.domain_at("lane.lines[0].net"),
        Some(&Domain::Text("D0".into()))
    );
    assert_eq!(
        m.domain_at("lane.lines[3].net"),
        Some(&Domain::Text("D3".into()))
    );
    assert!(m.field_at("lane.lines[1].net").is_none());
}

#[test]
fn a_list_named_with_no_index_says_to_name_each_by_its_index() {
    let c = run(&format!(
        "{GROUP}board B {{\n    .lane: LengthGroup {{}}\n    .lane.lines.net &= \"D0\"\n}}\n"
    ));
    assert_eq!(
        messages(&c),
        ["`.lines` is a list of pins, not one pin: name each by its index, `.lines[0]`, `.lines[1]`, ..., and the list has as many as the indices named"]
    );
}

#[test]
fn only_an_interface_declares_a_list_and_only_of_pins() {
    let board = run("board B {\n    .lines: [Pin]\n}\n");
    assert!(
        messages(&board)
            .iter()
            .any(|m| m.contains("only an interface declares a list (`.lines: [Pin]`)")),
        "{}",
        board.render()
    );
    let values = run("interface G {\n    .widths: [Length]\n}\n");
    assert!(
        messages(&values)
            .iter()
            .any(|m| m.contains("a list is of pins, `.widths: [Pin]`, and `Length` is not `Pin`")),
        "{}",
        values.render()
    );
    let block = run("interface G {\n    .lines: [Pin] { .net &= \"D0\" }\n}\n");
    assert!(
        messages(&block)
            .iter()
            .any(|m| m.contains("a list of pins has no block and no `optional`")),
        "{}",
        block.render()
    );
}
