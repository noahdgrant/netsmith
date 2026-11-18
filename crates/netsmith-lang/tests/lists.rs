//! Lists of pins whose number is not written, `.lines: [Pin]` (issue #1252): the syntax, the
//! printer and the formatter.

use netsmith_lang::ast::*;
use netsmith_lang::{format, parse};

const LIST: &str = "interface LengthGroup {\n    .lines: [Pin]\n    .tolerance: Length\n}\n";

#[test]
fn a_list_is_a_type_in_brackets_and_prints_back_to_its_source() {
    let file = parse(LIST).unwrap();
    let Item::Decl(d) = &file.definitions[0].items[0] else {
        panic!("{:?}", file.definitions[0].items[0])
    };
    let span = d.list.expect("a list");
    assert_eq!(&LIST[span.start..span.end], "[Pin]");
    assert_eq!(d.ty.to_source(), "Pin");
    assert!(d.count.is_none());
    assert_eq!(file.to_source(), LIST);
    assert_eq!(format(LIST).unwrap(), LIST);
}

#[test]
fn a_list_with_a_written_number_says_that_the_number_is_the_indices_named() {
    let message = parse("interface G {\n    .lines: [Pin; 8]\n}\n")
        .unwrap_err()
        .message;
    assert!(
        message.contains("a list's number is not written") && message.contains("write `[Pin]`"),
        "{message}"
    );
}
