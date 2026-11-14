//! A board's `defaults { ... }` block (issue #625): read, and printed back to its source.

use netsmith_lang::ast::{CmpOp, Item};
use netsmith_lang::parse;

const SRC: &str = "board B {\n    defaults {\n        Resistor.package ?= \"0402\"\n        Capacitor.package ?= \"0603\"\n    }\n}\n";

#[test]
fn a_defaults_block_holds_its_lines_as_statements() {
    let file = parse(SRC).unwrap();
    let [Item::Defaults(block)] = file.definitions[0].items.as_slice() else {
        panic!("{:?}", file.definitions[0].items);
    };
    assert_eq!(block.items.len(), 2);
    let Item::Constraint(c) = &block.items[0] else {
        panic!("{:?}", block.items[0]);
    };
    assert_eq!(c.op, CmpOp::Default);
    assert_eq!(c.lhs.to_source(), "Resistor.package");
}

#[test]
fn a_defaults_block_prints_back_to_its_source() {
    let printed = parse(SRC).unwrap().to_source();
    assert!(
        printed.contains("    defaults {\n        Resistor.package ?= \"0402\"\n        Capacitor.package ?= \"0603\"\n    }\n"),
        "{printed}"
    );
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
}

#[test]
fn defaults_is_a_keyword_only_before_a_brace() {
    // A member may still be called `defaults`.
    let file = parse("board B {\n    .defaults: Number\n    .defaults &= 2\n}\n").unwrap();
    assert!(matches!(file.definitions[0].items[0], Item::Decl(_)));
}

#[test]
fn a_doc_comment_on_a_defaults_block_is_refused() {
    let err = parse("board B {\n    /// no\n    defaults { Resistor.package ?= \"0402\" }\n}\n").unwrap_err();
    assert!(err.to_string().contains("doc comments can only be attached to declarations"), "{err}");
}
