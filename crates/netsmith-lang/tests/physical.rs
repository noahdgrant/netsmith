//! `physical for Board { ... }` and `case { ... }` (ADR 0048, "Syntax decisions" 1 and 7).

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const TRELLIS: &str = "\
//! The case of Trellis.

import std::pcb::{Cutout, Outline}

/// Case drawing TC-12.
physical for Trellis {
    case {
        .outline: Outline {
            .width &= 44mm
            .height &= 72mm
        }
        .inlet.plug.place.bottom >= .outline.height
    }
    .radio.row_a.place.left &= 8mm +/- 0.2mm
    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm
    .supply.place.side &= \"bottom\"
}

physical for Other {}
";

#[test]
fn a_physical_file_holds_units_named_for_boards() {
    let file = parse(TRELLIS).unwrap();
    assert!(file.definitions.is_empty());
    assert_eq!(file.physicals.len(), 2);
    let unit = &file.physicals[0];
    assert_eq!(unit.board.name, "Trellis");
    assert_eq!(unit.docs, ["Case drawing TC-12."]);
    assert_eq!(unit.items.len(), 4);
    assert_eq!(file.physicals[1].board.name, "Other");
    assert!(file.physicals[1].items.is_empty());
}

#[test]
fn the_case_section_nests_and_holds_statements() {
    let file = parse(TRELLIS).unwrap();
    let Item::Case(case) = &file.physicals[0].items[0] else {
        panic!("a case section")
    };
    assert_eq!(case.items.len(), 2);
    assert!(matches!(case.items[0], Item::Decl(_)));
    assert!(matches!(case.items[1], Item::Constraint(_)));
}

#[test]
fn a_physical_file_prints_back_to_source_that_parses_to_the_same_tree() {
    let file = parse(TRELLIS).unwrap();
    let printed = file.to_source();
    assert!(printed.contains("physical for Trellis {"), "{printed}");
    assert!(printed.contains("case {"), "{printed}");
    let again = parse(&printed).unwrap();
    assert_eq!(again.to_source(), printed);
    assert_eq!(again.physicals.len(), 2);
}

#[test]
fn a_unit_needs_the_board_name_and_a_brace() {
    let e = parse("physical for {}").unwrap_err();
    assert!(e.message.contains("the name of the board"), "{}", e.message);
    let e = parse("physical for Trellis\n").unwrap_err();
    assert!(e.message.contains("`{`"), "{}", e.message);
}

#[test]
fn physical_is_still_an_ordinary_name_elsewhere() {
    // Only `physical for` at the top level starts a unit.
    let file = parse("board B {\n    .physical: Length\n}\n").unwrap();
    assert_eq!(file.definitions.len(), 1);
    assert!(file.physicals.is_empty());
}

#[test]
fn an_unknown_top_level_word_lists_the_physical_unit() {
    let e = parse("physics for Trellis {}").unwrap_err();
    assert!(e.message.contains("`physical for`"), "{}", e.message);
}
