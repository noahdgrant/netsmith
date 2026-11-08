//! `optional` after the type of a member declaration (ADR 0074).

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const SRC: &str = "interface Limits {\n    /// The largest DC resistance, when the datasheet gives one.\n    .max_dcr: Resistance optional\n    .min_current: Current\n    .mode: Mode optional\n}\n";

fn decls(src: &str) -> Vec<Decl> {
    let file = parse(src).unwrap();
    file.definitions[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Decl(d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_declaration_may_be_marked_optional_after_its_type() {
    let d = decls(SRC);
    assert_eq!(d.len(), 3);
    assert!(d[0].optional.is_some(), "{:?}", d[0]);
    assert!(d[1].optional.is_none(), "{:?}", d[1]);
    assert!(d[2].optional.is_some(), "{:?}", d[2]);
    // The span is the word's.
    let span = d[0].optional.unwrap();
    assert_eq!(&SRC[span.start..span.end], "optional");
}

#[test]
fn an_optional_member_prints_back_to_the_source_it_came_from() {
    let printed = parse(SRC).unwrap().to_source();
    assert_eq!(printed, SRC);
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
}

#[test]
fn optional_is_a_plain_word_elsewhere() {
    // A member, a type, a variable and a path may still be called `optional`.
    let src = "module M {\n    .optional: Voltage\n    .a: optional\n    .optional &= 3V\n}\n";
    let d = decls(src);
    assert_eq!(d.len(), 2);
    assert!(d.iter().all(|d| d.optional.is_none()), "{d:?}");
    assert_eq!(d[1].ty.segments[0].name, "optional");
}

#[test]
fn the_word_is_on_the_line_of_the_type() {
    // On the next line it is a statement of its own, not the modifier.
    let src = "module M {\n    .a: Voltage\n    optional.x &= 3V\n}\n";
    let d = decls(src);
    assert_eq!(d.len(), 1);
    assert!(d[0].optional.is_none());
}
