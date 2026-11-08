//! `unused .processor.usb`: a board states that an interface is not used (ADR 0083).

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const SRC: &str =
    "board B {\n    .processor: Mcu\n    unused .processor.usb\n    unused .modem.usb;\n}\n";

fn unused(src: &str) -> Vec<String> {
    let file = parse(src).unwrap();
    file.definitions[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Unused(u) => Some(u.path.to_source()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_statement_names_one_interface_by_its_path() {
    assert_eq!(unused(SRC), [".processor.usb", ".modem.usb"]);
}

#[test]
fn it_prints_back_to_the_source_it_came_from() {
    let printed = parse(SRC).unwrap().to_source();
    assert!(printed.contains("    unused .processor.usb\n"), "{printed}");
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
    assert_eq!(unused(&printed), [".processor.usb", ".modem.usb"]);
}

#[test]
fn it_is_a_plain_word_where_it_does_not_start_a_statement() {
    // A member called `unused` is still a member.
    let file =
        parse("board B {\n    .unused: Rail\n    .unused.voltage &= 5V\n    unused.x ~ .y\n}\n")
            .unwrap();
    assert!(
        file.definitions[0]
            .items
            .iter()
            .all(|i| !matches!(i, Item::Unused(_)))
    );
}

#[test]
fn it_names_nothing_but_the_interface() {
    let e = parse("board B {\n    unused .a.usb ~ .b.usb\n}\n").unwrap_err();
    assert!(e.message.contains("`unused` names one interface"), "{e:?}");
    let e = parse("board B {\n    unused\n}\n").unwrap_err();
    assert!(!e.message.is_empty());
}

#[test]
fn a_doc_comment_cannot_be_attached_to_it() {
    let e = parse("board B {\n    /// no\n    unused .a.usb\n}\n").unwrap_err();
    assert!(e.message.contains("doc comments"), "{e:?}");
}
