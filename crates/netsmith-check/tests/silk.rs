//! What the silkscreen prints (`.silk`, `.silk_text`) is physical intent: the board's
//! physical file states it, and a board's circuit source does not (ADR 0048).

use netsmith_check::*;

const HEAD: &str = "\
import std::parts::connector::Header1x2

module Block {
    .j: Header1x2
}

";

fn run(body: &str, physical: &str) -> Checked {
    let loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", physical);
    check("t.nsm", &format!("{HEAD}{body}"), &loader)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_part_a_module_and_the_board_each_have_a_silk_and_a_part_has_a_text() {
    let c = run(
        "board B {\n    .j: Header1x2\n    .b: Block\n}\n",
        "physical for B {\n    .silk &= \"hide\"\n    .b.silk &= \"show\"\n    .j.silk &= \"hide\"\n    .b.j.silk_text &= \"UART\"\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    assert_eq!(m.domain_at("silk"), Some(&Domain::Text("hide".into())));
    assert_eq!(m.domain_at("b.silk"), Some(&Domain::Text("show".into())));
    assert_eq!(m.domain_at("b.j.silk_text"), Some(&Domain::Text("UART".into())));
}

#[test]
fn a_module_has_no_text_of_its_own() {
    let c = run(
        "board B {\n    .b: Block\n}\n",
        "physical for B {\n    .b.silk_text &= \"UART\"\n}\n",
    );
    let found = messages(&c);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("silk_text"), "{found:?}");
}

#[test]
fn a_boards_circuit_source_does_not_state_what_the_silkscreen_prints() {
    let c = run(
        "board B {\n    .j: Header1x2\n    .j.silk &= \"hide\"\n    .j.silk_text &= \"UART\"\n    .silk &= \"hide\"\n}\n",
        "physical for B {\n}\n",
    );
    let found = messages(&c);
    assert_eq!(found.len(), 3, "{found:?}");
    for (message, path) in found.iter().zip(["j.silk", "j.silk_text", "silk"]) {
        assert!(
            message.starts_with(&format!(
                "`{path}` is what the silkscreen prints, which is physical intent"
            )) && message.contains("put it in `physical for B` in `t.physical.nsm`"),
            "{message}"
        );
    }
}

#[test]
fn a_physical_file_may_not_state_other_things_about_a_part_by_the_same_gate() {
    let c = run(
        "board B {\n    .j: Header1x2\n}\n",
        "physical for B {\n    .j.mpn &= \"X\"\n}\n",
    );
    assert!(
        messages(&c)[0].contains("what its silkscreen prints (`.silk`, `.silk_text`)"),
        "{:?}",
        messages(&c)
    );
}
