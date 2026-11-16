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

// ---- a kind's line in the physical file's `defaults` (issue #881) ----

#[test]
fn a_physical_files_defaults_offers_silk_to_every_part_of_a_kind() {
    let c = run(
        "board B {\n    .j: Header1x2\n    .b: Block\n}\n",
        "import std::parts::connector::Header1x2\n\nphysical for B {\n    defaults {\n        Header1x2.silk ?= \"hide\"\n    }\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    assert_eq!(m.domain_at("j.silk"), Some(&Domain::Text("hide".into())));
    assert_eq!(m.domain_at("b.j.silk"), Some(&Domain::Text("hide".into())));
    assert_eq!(
        m.domain_at("b.silk"),
        Some(&Domain::Unset),
        "the module states nothing"
    );
}

#[test]
fn a_parts_own_statement_and_its_own_text_beat_the_line_of_its_kind() {
    let c = run(
        "board B {\n    .j: Header1x2\n    .b: Block\n}\n",
        "import std::parts::connector::Header1x2\n\nphysical for B {\n    defaults {\n        Header1x2.silk ?= \"hide\"\n    }\n    .j.silk &= \"show\"\n    .b.j.silk_text &= \"UART\"\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    assert_eq!(m.domain_at("j.silk"), Some(&Domain::Text("show".into())));
    assert_eq!(
        m.domain_at("b.j.silk"),
        Some(&Domain::Unset),
        "a text of its own is not hidden"
    );
}

#[test]
fn a_line_that_reaches_nothing_is_a_note() {
    let c = run(
        "board B {\n    .j: Header1x2\n}\n",
        "import std::parts::connector::Header1x2\n\nphysical for B {\n    defaults {\n        Header1x2.silk ?= \"hide\"\n    }\n    .j.silk_text &= \"UART\"\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let notes: Vec<&str> = c.boards[0]
        .model
        .default_notes
        .iter()
        .map(|n| n.message.as_str())
        .collect();
    assert_eq!(
        notes,
        ["the default for `Header1x2.silk` reaches nothing: every `Header1x2` on the board states `.silk` or `.silk_text` itself"]
    );
}

#[test]
fn a_physical_files_defaults_offers_nothing_but_silk_and_only_show_or_hide() {
    let c = run(
        "board B {\n    .j: Header1x2\n}\n",
        "import std::parts::connector::Header1x2\n\nphysical for B {\n    defaults {\n        Header1x2.package ?= \"0402\"\n        Header1x2.silk_text ?= \"J\"\n        Header1x2.silk ?= \"off\"\n    }\n}\n",
    );
    let found = messages(&c);
    assert_eq!(found.len(), 3, "{found:?}");
    assert!(
        found[0].starts_with("a physical file's `defaults` offers only what the silkscreen prints")
            && found[0].contains("`Header1x2.package` is a value of the circuit"),
        "{found:?}"
    );
    assert!(
        found[1].starts_with("a text on the silkscreen belongs to one part"),
        "{found:?}"
    );
    assert!(
        found[2].contains("`Header1x2.silk` is \"show\" or \"hide\""),
        "{found:?}"
    );
}

#[test]
fn a_circuit_sources_defaults_does_not_offer_silk() {
    let c = run(
        "board B {\n    .j: Header1x2\n    defaults {\n        Header1x2.silk ?= \"hide\"\n    }\n}\n",
        "physical for B {\n}\n",
    );
    let found = messages(&c);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("`Header1x2.silk` is what the silkscreen prints")
            && found[0].contains("a `defaults` block of `physical for B`"),
        "{found:?}"
    );
}

#[test]
fn a_modules_physical_file_holds_no_defaults() {
    let mut loader = Loader::std();
    loader.insert(
        "src::block",
        "src/block.nsm",
        "import std::parts::connector::Header1x2\n\nmodule Block {\n    physical \"block.physical.nsm\"\n    .j: Header1x2\n}\n",
    );
    loader.attach_physical(
        "src/block.nsm",
        "src/block.physical.nsm",
        "import std::parts::connector::Header1x2\n\nphysical for Block {\n    defaults {\n        Header1x2.silk ?= \"hide\"\n    }\n}\n",
    );
    let c = check(
        "main.nsm",
        "import src::block::Block\n\nboard B {\n    .b: Block\n}\n",
        &loader,
    );
    let found = messages(&c);
    assert!(
        found
            .iter()
            .any(|m| m.starts_with("a module's physical file holds no `defaults`")),
        "{found:?}"
    );
}

#[test]
fn a_line_for_a_module_kind_offers_the_modules_own_silk() {
    let mut loader = Loader::std();
    loader.insert(
        "src::block",
        "src/block.nsm",
        "import std::parts::connector::Header1x2\n\nmodule Block {\n    .j: Header1x2\n}\n",
    );
    loader.attach_physical(
        "main.nsm",
        "main.physical.nsm",
        "import src::block::Block\n\nphysical for B {\n    defaults {\n        Block.silk ?= \"hide\"\n    }\n}\n",
    );
    let c = check(
        "main.nsm",
        "import src::block::Block\n\nboard B {\n    .b: Block\n    .c: Block\n}\n",
        &loader,
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    for path in ["b.silk", "c.silk"] {
        assert_eq!(m.domain_at(path), Some(&Domain::Text("hide".into())), "{path}");
    }
}
