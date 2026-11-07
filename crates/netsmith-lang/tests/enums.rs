//! `enum` declarations and `Enum::Variant` values (ADR 0054).

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const SOURCE: &str = "\
/// How a lamp is driven.
enum Drive {
    /// Always on.
    Steady,
    Dimmed
    Blinking,
}

board B {
    .drive: Drive
    .drive &= Drive::Dimmed
    .drive == Drive::Steady
}
";

fn constraints(file: &File) -> Vec<&Constraint> {
    file.definitions
        .iter()
        .flat_map(|d| &d.items)
        .filter_map(|i| match i {
            Item::Constraint(c) => Some(c),
            _ => None,
        })
        .collect()
}

#[test]
fn an_enum_has_a_name_and_variants_in_the_order_written() {
    let file = parse(SOURCE).unwrap();
    let def = &file.definitions[0];
    assert_eq!(def.kind, DefKind::Enum);
    assert_eq!(def.name.name, "Drive");
    assert_eq!(def.docs, ["How a lamp is driven."]);
    let names: Vec<&str> = def.variants.iter().map(|v| v.name.name.as_str()).collect();
    assert_eq!(names, ["Steady", "Dimmed", "Blinking"]);
    assert_eq!(def.variants[0].docs, ["Always on."]);
    assert!(def.items.is_empty());
}

#[test]
fn a_variant_is_an_expression_with_its_enum() {
    let file = parse(SOURCE).unwrap();
    let all = constraints(&file);
    match &all[0].rhs {
        Expr::Variant { ty, variant, .. } => {
            assert_eq!(ty.segments.len(), 1);
            assert_eq!(ty.segments[0].name, "Drive");
            assert_eq!(variant.name, "Dimmed");
        }
        other => panic!("expected a variant, got {other:?}"),
    }
    assert_eq!(all[1].op, CmpOp::EqEq);
}

#[test]
fn a_qualified_enum_keeps_its_path() {
    let file = parse("board B {\n    .x &= std::prelude::AssemblyMode::Hand\n}\n").unwrap();
    match &constraints(&file)[0].rhs {
        Expr::Variant { ty, variant, .. } => {
            let path: Vec<&str> = ty.segments.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(path, ["std", "prelude", "AssemblyMode"]);
            assert_eq!(variant.name, "Hand");
        }
        other => panic!("expected a variant, got {other:?}"),
    }
}

#[test]
fn an_enum_prints_back_to_source_that_parses_to_the_same_tree() {
    let file = parse(SOURCE).unwrap();
    let printed = file.to_source();
    let again = parse(&printed).unwrap();
    assert_eq!(again.to_source(), printed);
    assert_eq!(again.definitions[0].variants.len(), 3);
    assert!(printed.contains("Drive::Dimmed"), "{printed}");
}

#[test]
fn a_variant_written_twice_is_refused() {
    let e = parse("enum E {\n    A,\n    B,\n    A,\n}\n").unwrap_err();
    assert_eq!(e.message, "the variant `A` is written twice");
}

#[test]
fn an_enum_body_holds_names_and_nothing_else() {
    let e = parse("enum E {\n    .a: Pin\n}\n").unwrap_err();
    assert!(
        e.message.starts_with("expected the name of a variant"),
        "{}",
        e.message
    );
    let e = parse("enum E {\n    A B\n}\n").unwrap_err();
    assert!(e.message.contains("after a variant"), "{}", e.message);
}

#[test]
fn an_empty_enum_parses_and_an_unclosed_one_is_refused() {
    let file = parse("enum E {}\n").unwrap();
    assert!(file.definitions[0].variants.is_empty());
    let e = parse("enum E {\n    A,\n").unwrap_err();
    assert_eq!(e.message, "this `{` is never closed");
}
