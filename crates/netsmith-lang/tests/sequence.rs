//! The power-up order of a board, `sequence { .a.vout; then 6ms; .b.vout; after .e }`
//! (issue #1267): the syntax, the printer and the formatter.

use netsmith_lang::ast::*;
use netsmith_lang::{format, parse};

const BOARD: &str = "board B {\n    sequence {\n        .a.vout\n        then 6ms\n        .b.vout\n        after .radio.en\n        then\n        .c\n    }\n}\n";

#[test]
fn a_sequence_is_events_parted_by_then_and_prints_back_to_its_source() {
    let file = parse(BOARD).unwrap();
    let Item::Sequence(q) = &file.definitions[0].items[0] else {
        panic!("{:?}", file.definitions[0].items[0])
    };
    let words: Vec<String> = q
        .steps
        .iter()
        .map(|s| match s {
            SequenceStep::Event(p) => p.to_source(),
            SequenceStep::Then { delay: Some((n, _)), .. } => format!("then {}", n.text),
            SequenceStep::Then { delay: None, .. } => "then".into(),
            SequenceStep::After { event, .. } => format!("after {}", event.to_source()),
        })
        .collect();
    assert_eq!(
        words,
        [".a.vout", "then 6ms", ".b.vout", "after .radio.en", "then", ".c"]
    );
    assert_eq!(file.to_source(), BOARD);
    assert_eq!(format(BOARD).unwrap(), BOARD);
}

#[test]
fn the_statements_of_a_sequence_may_be_parted_by_semicolons_and_are_laid_out_one_to_a_line() {
    let one_line = "board B {\n    sequence { .a.vout; then 6ms; .b.vout; after .radio.en; then; .c }\n}\n";
    assert_eq!(parse(one_line).unwrap().to_source(), BOARD);
    assert_eq!(format(one_line).unwrap(), BOARD);
}

fn refused(body: &str) -> String {
    parse(&format!("board B {{\n    sequence {{ {body} }}\n}}\n"))
        .unwrap_err()
        .message
}

#[test]
fn a_sequence_out_of_shape_is_refused_with_its_shape() {
    for (body, says) in [
        (".a.vout; .b.vout", "parted by `then`"),
        ("then 6ms; .b.vout", "`then` follows an event"),
        (".a.vout; then", "`then` is followed by an event"),
        (".a.vout", "two events or more"),
        ("after .e", "`after` follows an event"),
    ] {
        let message = refused(body);
        assert!(
            message.contains(says) && message.contains("sequence {"),
            "`{body}`: {message}"
        );
    }
}
