//! `direction .a to .b when .dir high` in a part (issue #1306): what it parses to, how it
//! prints, and what the parser says about one that is wrong.

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const SHIFT: &str = "\
part Shift {
    direction .a to .b when .dir high
    direction .b.line to .a.line when .dir low
}
";

fn items(src: &str) -> Vec<Item> {
    parse(src).unwrap().definitions[0].items.clone()
}

#[test]
fn a_direction_has_its_two_sides_its_control_pin_and_its_level() {
    let items = items(SHIFT);
    let Item::Direction(d) = &items[0] else {
        panic!("{items:#?}")
    };
    assert_eq!(
        (
            d.from.to_source(),
            d.to.to_source(),
            d.control.to_source(),
            d.high
        ),
        (".a".into(), ".b".into(), ".dir".into(), true)
    );
    let Item::Direction(d) = &items[1] else {
        panic!("{items:#?}")
    };
    assert_eq!(
        (d.from.to_source(), d.to.to_source(), d.high),
        (".b.line".into(), ".a.line".into(), false)
    );
}

#[test]
fn a_direction_prints_and_parses_back_to_the_same_tree() {
    let printed = parse(SHIFT).unwrap().to_source();
    assert!(
        printed.contains("    direction .a to .b when .dir high\n"),
        "{printed}"
    );
    assert!(
        printed.contains("    direction .b.line to .a.line when .dir low\n"),
        "{printed}"
    );
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
}

#[test]
fn a_direction_with_no_level_names_the_two_words() {
    let e = parse("part P {\n    direction .a to .b when .dir\n}\n").unwrap_err();
    assert!(
        e.message
            .starts_with("expected `high` or `low`, the level of the control pin, in `direction .a to .b when .dir high`"),
        "{}",
        e.message
    );
}

#[test]
fn a_direction_with_no_to_says_what_it_needs() {
    let e = parse("part P {\n    direction .a .b when .dir high\n}\n").unwrap_err();
    assert!(
        e.message.starts_with(
            "expected `to` and the pin the signal goes to in `direction .a to .b when .dir high`"
        ),
        "{}",
        e.message
    );
}

#[test]
fn a_member_called_direction_is_still_a_member() {
    // Only `direction` followed by a pin starts the statement.
    let items = items("part P {\n    .direction: Voltage\n}\n");
    assert!(matches!(items[0], Item::Decl(_)), "{items:#?}");
}
