//! An `enum` is a type with a fixed list of variants (ADR 0054). Every part has a built-in
//! `.assembly` of the enum `AssemblyMode`, which every file knows without an import.

use netsmith_check::*;

fn run(src: &str) -> Checked {
    check("test.nsm", src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn text_at(c: &Checked, board: &str, path: &str) -> Option<String> {
    match c.board(board).unwrap().model.domain_at(path) {
        Some(Domain::Text(t)) => Some(t.clone()),
        _ => None,
    }
}

/// An invented enum, a module that holds one and a board that states it.
fn lamp(board_body: &str) -> String {
    format!(
        "\
enum Drive {{
    Steady,
    Dimmed,
    Blinking,
}}

module Lamp {{
    .drive: Drive
}}

board B {{
    .lamp: Lamp
{board_body}}}
"
    )
}

#[test]
fn a_member_of_an_enum_type_takes_a_variant() {
    let c = run(&lamp("    .lamp.drive &= Drive::Dimmed\n"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(text_at(&c, "B", "lamp.drive").as_deref(), Some("Dimmed"));
}

#[test]
fn a_variant_can_be_a_default_and_a_board_replaces_it() {
    let src = "\
enum Drive { Steady, Dimmed }
module Lamp {
    .drive: Drive
    .drive ?= Drive::Steady
}
board A { .lamp: Lamp }
board B {
    .lamp: Lamp
    .lamp.drive &= Drive::Dimmed
}
";
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(text_at(&c, "A", "lamp.drive").as_deref(), Some("Steady"));
    assert_eq!(text_at(&c, "B", "lamp.drive").as_deref(), Some("Dimmed"));
}

#[test]
fn two_different_variants_for_one_field_contradict_each_other() {
    let c = run(&lamp(
        "    .lamp.drive &= Drive::Dimmed\n    .lamp.drive &= Drive::Steady\n",
    ));
    assert_eq!(
        messages(&c),
        [
            "contradiction on `lamp.drive`: it is already Drive::Dimmed, which does not overlap Drive::Steady"
        ]
    );
}

#[test]
fn a_variant_the_enum_lacks_is_refused_with_the_variants_and_the_nearest() {
    let c = run(&lamp("    .lamp.drive &= Drive::Dimed\n"));
    assert_eq!(
        messages(&c),
        [
            "`Drive` has no variant `Dimed`; its variants are `Drive::Steady`, `Drive::Dimmed` and `Drive::Blinking`; did you mean `Drive::Dimmed`?"
        ]
    );
    // Nothing is near: the list is the whole hint.
    let c = run(&lamp("    .lamp.drive &= Drive::Off\n"));
    assert_eq!(
        messages(&c),
        [
            "`Drive` has no variant `Off`; its variants are `Drive::Steady`, `Drive::Dimmed` and `Drive::Blinking`"
        ]
    );
}

#[test]
fn a_variant_in_the_wrong_case_names_the_right_one() {
    let c = run(&lamp("    .lamp.drive &= Drive::dimmed\n"));
    assert_eq!(
        messages(&c),
        [
            "`Drive` has no variant `dimmed`; its variants are `Drive::Steady`, `Drive::Dimmed` and `Drive::Blinking`; did you mean `Drive::Dimmed`?"
        ]
    );
}

#[test]
fn text_where_a_variant_belongs_says_which_variant_it_was_meant_to_be() {
    let c = run(&lamp("    .lamp.drive &= \"dimmed\"\n"));
    assert_eq!(
        messages(&c),
        [
            "`lamp.drive` holds a variant of `Drive`, but this value is text; a variant of `Drive` is not written as text: write `Drive::Dimmed` (the variants are `Drive::Steady`, `Drive::Dimmed` and `Drive::Blinking`)"
        ]
    );
    let c = run(&lamp("    .lamp.drive &= \"flashing\"\n"));
    assert_eq!(
        messages(&c),
        [
            "`lamp.drive` holds a variant of `Drive`, but this value is text; a variant of `Drive` is not written as text, and \"flashing\" is none of them: the variants are `Drive::Steady`, `Drive::Dimmed` and `Drive::Blinking`"
        ]
    );
}

#[test]
fn a_variant_of_another_enum_or_a_number_does_not_fit() {
    let src = "\
enum Drive { Steady, Dimmed }
enum Colour { Red, Green }
module Lamp { .drive: Drive }
board B {
    .lamp: Lamp
    .lamp.drive &= Colour::Red
}
";
    let c = run(src);
    assert_eq!(
        messages(&c),
        ["`lamp.drive` holds a variant of `Drive`, but this value is a variant of `Colour`"]
    );
    let c = run(&lamp("    .lamp.drive &= 5V\n"));
    assert_eq!(
        messages(&c),
        ["`lamp.drive` holds a variant of `Drive`, but this value is V"]
    );
}

#[test]
fn a_text_field_does_not_take_a_variant() {
    let src = "\
enum Drive { Steady, Dimmed }
module Lamp { .label: String }
board B {
    .lamp: Lamp
    .lamp.label &= Drive::Steady
}
";
    let c = run(src);
    assert_eq!(
        messages(&c),
        ["`lamp.label` holds text, but this value is a variant of `Drive`"]
    );
}

#[test]
fn variants_compare_with_equals_equals_and_nothing_else() {
    let ok = run(&lamp(
        "    .lamp.drive &= Drive::Dimmed\n    .lamp.drive == Drive::Dimmed\n",
    ));
    assert!(ok.is_ok(), "{}", ok.render());
    let c = run(&lamp("    .lamp.drive >= Drive::Dimmed\n"));
    assert_eq!(
        messages(&c),
        ["`>=` compares numbers; variants are compared with `==` only"]
    );
    let c = run(&lamp("    .lamp.drive == \"Dimmed\"\n"));
    assert_eq!(
        messages(&c),
        [
            "the two sides do not match: left is a variant of `Drive`, right is text; a variant of `Drive` is not written as text: write `Drive::Dimmed` (the variants are `Drive::Steady`, `Drive::Dimmed` and `Drive::Blinking`)"
        ]
    );
}

#[test]
fn a_name_that_is_not_an_enum_is_not_a_variant() {
    let src = "\
import std::parts::passive::Resistor
module Lamp { .label: String }
board B {
    .lamp: Lamp
    .lamp.label &= Resistor::Hand
}
";
    let c = run(src);
    assert_eq!(
        messages(&c),
        ["`Resistor` is a part, not an enum, so `Resistor::Hand` is not a value"]
    );
    let c = run(&lamp("    .lamp.drive &= Missing::Dimmed\n"));
    assert_eq!(messages(&c), ["unknown type `Missing`"]);
    let c = run(&lamp("    .lamp.drive &= String::Dimmed\n"));
    assert_eq!(
        messages(&c),
        ["`String` is not an enum, so `String::Dimmed` is not a value"]
    );
}

#[test]
fn an_enum_in_another_file_is_imported_like_a_definition() {
    let mut loader = Loader::std();
    loader.insert(
        "src::drive",
        "src/drive.nsm",
        "enum Drive { Steady, Dimmed }\n",
    );
    let src = "\
import src::drive::Drive
module Lamp { .drive: Drive }
board B {
    .lamp: Lamp
    .lamp.drive &= Drive::Dimmed
}
";
    let c = check("test.nsm", src, &loader);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(text_at(&c, "B", "lamp.drive").as_deref(), Some("Dimmed"));
    // Without the import the name is unknown.
    let c = check(
        "test.nsm",
        &src.replace("import src::drive::Drive\n", ""),
        &loader,
    );
    assert!(
        messages(&c).iter().any(|m| m == "unknown type `Drive`"),
        "{:?}",
        messages(&c)
    );
}

#[test]
fn an_enum_is_declared_once_in_a_file() {
    let c = run("enum E { A }\nenum E { B }\nboard B {}\n");
    assert_eq!(messages(&c), ["`E` is defined twice in this file"]);
}

#[test]
fn a_system_holds_boards_not_enum_values() {
    let src = "\
enum E { A }
board Inner {}
system S {
    .x: E
    .b: Inner
}
";
    let c = run(src);
    assert!(
        messages(&c)
            .iter()
            .any(|m| m.contains("a system holds boards, but `E` is not one")),
        "{:?}",
        messages(&c)
    );
}

// ---- AssemblyMode, the built-in type of `.assembly` ----------------------------

#[test]
fn every_part_has_an_assembly_that_takes_a_variant_with_no_import() {
    let src = "\
import std::parts::passive::Resistor
board B {
    .r1: Resistor { .assembly &= AssemblyMode::Hand }
}
";
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(text_at(&c, "B", "r1.assembly").as_deref(), Some("Hand"));
}

#[test]
fn a_wrong_assembly_variant_lists_the_three() {
    let src = "\
import std::parts::passive::Resistor
board B {
    .r1: Resistor { .assembly &= AssemblyMode::Hnad }
}
";
    let c = run(src);
    assert_eq!(
        messages(&c),
        [
            "`AssemblyMode` has no variant `Hnad`; its variants are `AssemblyMode::Fab`, `AssemblyMode::Hand`, `AssemblyMode::None` and `AssemblyMode::Consigned`; did you mean `AssemblyMode::Hand`?"
        ]
    );
}

#[test]
fn the_quoted_words_of_earlier_versions_are_refused_with_the_variant_to_write() {
    for (word, variant) in [("hand", "Hand"), ("fab", "Fab"), ("none", "None")] {
        let src = format!(
            "import std::parts::passive::Resistor\nboard B {{\n    .r1: Resistor {{ .assembly &= \"{word}\" }}\n}}\n"
        );
        let c = run(&src);
        let all = messages(&c);
        assert_eq!(all.len(), 1, "{all:?}");
        assert!(
            all[0].starts_with(
                "`r1.assembly` holds a variant of `AssemblyMode`, but this value is text;"
            ) && all[0].contains(&format!("write `AssemblyMode::{variant}`")),
            "{all:?}"
        );
    }
    let src = "\
import std::parts::passive::Resistor
board B {
    .r1: Resistor { .assembly &= \"solder\" }
}
";
    let all = messages(&run(src));
    assert!(
        all[0].contains("\"solder\" is none of them: the variants are `AssemblyMode::Fab`, `AssemblyMode::Hand`, `AssemblyMode::None` and `AssemblyMode::Consigned`"),
        "{all:?}"
    );
}

#[test]
fn a_module_offers_assembly_mode_and_the_board_states_it() {
    let src = "\
import std::parts::connector::Header1x2
module Block {
    .assembly_mode: AssemblyMode
    .j: Header1x2
    .j.assembly &= .assembly_mode
}
board B {
    .block: Block
    .block.assembly_mode &= AssemblyMode::Fab
}
";
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        text_at(&c, "B", "block.assembly_mode").as_deref(),
        Some("Fab")
    );
}

#[test]
fn a_string_offered_to_a_part_is_refused_with_the_type_to_use() {
    let src = "\
import std::parts::connector::Header1x2
module Block {
    .assembly_mode: String
    .j: Header1x2
    .j.assembly &= .assembly_mode
}
board B {
    .block: Block
}
";
    let c = run(src);
    assert_eq!(
        messages(&c),
        [
            // The module is checked on its own, and again where the board uses it.
            "`j.assembly` holds a variant of `AssemblyMode`, but this value is text",
            "`block.j.assembly` holds a variant of `AssemblyMode`, but this value is text"
        ]
    );
}

#[test]
fn assembly_cannot_be_declared_again() {
    let src = "\
part Odd {
    .assembly: AssemblyMode
}
board B {}
";
    let c = run(src);
    assert!(
        messages(&c)
            .iter()
            .any(|m| m.contains("`assembly` is already declared")),
        "{:?}",
        messages(&c)
    );
}

#[test]
fn the_prelude_is_listed_with_the_standard_library_and_can_be_imported() {
    assert!(
        Loader::std()
            .modules()
            .iter()
            .any(|(key, file, _)| *key == "std::prelude" && *file == "std/prelude.nsm")
    );
    let src = "\
import std::prelude::AssemblyMode
import std::parts::passive::Resistor
board B {
    .r1: Resistor { .assembly &= AssemblyMode::None }
}
";
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_loader_with_no_standard_library_still_knows_the_prelude() {
    let src = "\
part Odd {
    .assembly &= AssemblyMode::Hand
}
board B {
    .odd: Odd
}
";
    let c = check("test.nsm", src, &Loader::empty());
    assert!(c.is_ok(), "{}", c.render());
}
