//! What the silkscreen prints for a part (`.silk`, `.silk_text`, stated in the physical
//! file): the design lists the parts that do not print their designator, and the most
//! specific statement decides.

use netsmith_check::{Loader, check};
use netsmith_core::{PartSilk, SilkPrint};
use netsmith_design::{Designed, design_board};
use netsmith_solve::{Catalogs, solve};

const SOURCE: &str = "\
import std::parts::connector::{Header1x2, Header1x4}

module Block {
    .j: Header1x2
    .k: Header1x2
}

module Outer {
    .inner: Block
    .m: Header1x4
}

board B {
    .a: Header1x2
    .b: Header1x4
    .block: Block
    .outer: Outer
}
";

/// The design of `B` with `physical` as its physical file, or the messages that stop it.
fn designed(physical: &str) -> Result<Designed, Vec<String>> {
    let loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", physical);
    let c = check("t.nsm", SOURCE, &loader);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    design_board(&c.boards[0], &s)
}

fn silks(physical: &str) -> Vec<PartSilk> {
    designed(&format!(
        "import std::parts::connector::{{Connector, Header1x2}}\n\nphysical for B {{\n{physical}\n}}\n"
    ))
        .unwrap_or_else(|e| panic!("{e:?}"))
        .design
        .features
        .silks
}

fn nothing(path: &str) -> PartSilk {
    PartSilk {
        path: path.into(),
        print: SilkPrint::Nothing,
    }
}

fn text(path: &str, text: &str) -> PartSilk {
    PartSilk {
        path: path.into(),
        print: SilkPrint::Text(text.into()),
    }
}

#[test]
fn a_board_that_states_nothing_prints_every_designator() {
    assert_eq!(silks(""), []);
}

#[test]
fn a_part_hides_its_designator_and_the_others_keep_theirs() {
    assert_eq!(silks("    .a.silk &= \"hide\""), [nothing("a")]);
}

#[test]
fn the_board_hides_every_designator_at_any_depth() {
    let found = silks("    .silk &= \"hide\"");
    let paths: Vec<&str> = found.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "a",
            "b",
            "block.j",
            "block.k",
            "outer.inner.j",
            "outer.inner.k",
            "outer.m"
        ]
    );
    assert!(found.iter().all(|s| s.print == SilkPrint::Nothing));
}

#[test]
fn a_module_hides_the_designators_of_every_part_in_it() {
    let found = silks("    .outer.silk &= \"hide\"");
    let paths: Vec<&str> = found.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(
        paths,
        ["outer.inner.j", "outer.inner.k", "outer.m"],
        "the parts of the module, at any depth, and no others"
    );
}

#[test]
fn the_most_specific_statement_wins() {
    // The board hides all, a module shows its own, and a part of that module hides again.
    let found = silks(
        "    .silk &= \"hide\"\n    .outer.silk &= \"show\"\n    .outer.inner.silk &= \"hide\"\n    .outer.inner.k.silk &= \"show\"",
    );
    let paths: Vec<&str> = found.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(
        paths,
        ["a", "b", "block.j", "block.k", "outer.inner.j"],
        "outer.m shows (its module), outer.inner.k shows (itself), outer.inner.j hides (its module)"
    );
}

#[test]
fn a_part_prints_the_text_it_is_given_in_place_of_its_designator() {
    let found = silks("    .silk &= \"hide\"\n    .a.silk_text &= \"5V\"\n    .block.j.silk_text &= \"J1 UART\"");
    assert!(found.contains(&text("a", "5V")), "{found:?}");
    assert!(found.contains(&text("block.j", "J1 UART")), "{found:?}");
    assert!(found.contains(&nothing("b")), "{found:?}");
}

#[test]
fn a_text_with_nothing_said_of_the_designator_is_listed_alone() {
    assert_eq!(silks("    .b.silk_text &= \"TX\""), [text("b", "TX")]);
}

#[test]
fn a_word_other_than_show_or_hide_is_refused() {
    let e = designed("physical for B {\n    .a.silk &= \"none\"\n    .silk &= \"off\"\n}\n").unwrap_err();
    assert!(
        e.iter().any(|m| m.contains("`a` has `.silk` \"none\"") && m.contains("\"show\" or \"hide\"")),
        "{e:?}"
    );
    assert!(
        e.iter().any(|m| m.contains("the board has `.silk` \"off\"")),
        "{e:?}"
    );
}

#[test]
fn an_empty_text_is_refused_and_so_is_a_text_on_a_hidden_designator() {
    let e = designed("physical for B {\n    .a.silk_text &= \"\"\n}\n").unwrap_err();
    assert!(
        e.iter().any(|m| m.contains("`a.silk_text` is empty") && m.contains(".a.silk &= \"hide\"")),
        "{e:?}"
    );
    let e = designed("physical for B {\n    .a.silk_text &= \"5V\"\n    .a.silk &= \"hide\"\n}\n").unwrap_err();
    assert!(
        e.iter().any(|m| m.contains("`a` hides its silkscreen text") && m.contains("\"5V\"")),
        "{e:?}"
    );
}

// ---- a line for every part of a kind, in the physical file's `defaults` (issue #881) ----

fn paths(found: &[PartSilk]) -> Vec<&str> {
    found.iter().map(|s| s.path.as_str()).collect()
}

#[test]
fn a_kind_hides_the_designators_of_every_part_of_it_and_the_rest_keep_theirs() {
    let found = silks("    defaults {\n        Header1x2.silk ?= \"hide\"\n    }");
    assert_eq!(
        paths(&found),
        ["a", "block.j", "block.k", "outer.inner.j", "outer.inner.k"],
        "every Header1x2 at any depth, and neither Header1x4"
    );
    assert!(found.iter().all(|s| s.print == SilkPrint::Nothing));
}

#[test]
fn a_generic_reaches_every_part_that_provides_it() {
    let found = silks("    defaults {\n        Connector.silk ?= \"hide\"\n    }");
    assert_eq!(found.len(), 7, "{found:?}");
}

#[test]
fn a_parts_own_statement_beats_its_kind_and_its_kind_beats_its_module_and_the_board() {
    // The board and a module hide, the kind shows, and one part of the kind hides itself.
    let found = silks(
        "    defaults {\n        Header1x2.silk ?= \"show\"\n    }\n    .silk &= \"hide\"\n    .outer.silk &= \"hide\"\n    .block.k.silk &= \"hide\"",
    );
    assert_eq!(
        paths(&found),
        ["b", "block.k", "outer.m"],
        "the Header1x4s follow the board and the module; block.k hides itself; the other Header1x2s show"
    );
}

#[test]
fn a_part_with_a_text_of_its_own_prints_it_whatever_its_kind_says() {
    let found = silks(
        "    defaults {\n        Header1x2.silk ?= \"hide\"\n    }\n    .a.silk_text &= \"5V\"",
    );
    assert!(found.contains(&text("a", "5V")), "{found:?}");
    assert!(found.contains(&nothing("block.j")), "{found:?}");
}

