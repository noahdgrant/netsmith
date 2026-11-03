//! Rust-style comments, and the examples that use them.

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const STATUS_INDICATOR: &str = include_str!("../../../examples/status_indicator.nsm");

fn names(idents: &[Ident]) -> Vec<&str> {
    idents.iter().map(|i| i.name.as_str()).collect()
}

fn message(src: &str) -> (String, usize, usize) {
    let e = parse(src).unwrap_err();
    let (line, col) = e.line_col(src);
    (e.message, line, col)
}

#[test]
fn plain_comments_are_whitespace() {
    let src = "// line\nboard D { // trailing\n    //// banner\n    .a: R\n    .b: R\n}\n";
    let file = parse(src).unwrap();
    assert_eq!(file.definitions[0].items.len(), 2);
}

#[test]
fn a_block_comment_is_an_error_that_points_at_its_opening() {
    let (msg, line, col) = message("board D {\n    .x &= 1V /* gap */ + 2V\n}\n");
    assert_eq!(msg, "block comments do not exist; use `//`");
    assert_eq!((line, col), (2, 14));

    let (msg, line, col) = message("board D {}\n/* never closed\n");
    assert_eq!(msg, "block comments do not exist; use `//`");
    assert_eq!((line, col), (2, 1));
}

#[test]
fn doc_comments_attach_to_files_definitions_and_declarations() {
    let src = "\
//! The file.
//! Two lines.

/// A part.
///
/// With a blank doc line.
part P {
    //! Inside the part.
    /// A pin.
    .a: Pin
    .b: Pin
}
";
    let file = parse(src).unwrap();
    assert_eq!(file.inner_docs, ["The file.", "Two lines."]);
    let part = &file.definitions[0];
    assert_eq!(part.docs, ["A part.", "", "With a blank doc line."]);
    assert_eq!(part.inner_docs, ["Inside the part."]);
    let Item::Decl(a) = &part.items[0] else {
        panic!()
    };
    assert_eq!(a.docs, ["A pin."]);
    let Item::Decl(b) = &part.items[1] else {
        panic!()
    };
    assert!(b.docs.is_empty());
    assert_eq!(file.to_source(), src);
}

#[test]
fn doc_comments_survive_a_print_parse_round_trip() {
    let once = parse(STATUS_INDICATOR).unwrap().to_source();
    assert_eq!(parse(&once).unwrap().to_source(), once);
    assert!(once.contains("/// A reusable status light indicator"));
    assert!(once.starts_with("//! A small status-light board.\n"));
}

#[test]
fn misplaced_doc_comments_are_errors_that_point_at_the_comment() {
    let (msg, line, col) = message("board D {\n    /// about nothing\n    .x &= 1V\n}\n");
    assert!(msg.contains("only be attached to declarations"), "{msg}");
    assert_eq!((line, col), (2, 5));

    let (msg, line, _) = message("board D {\n    .a: R\n    /// dangling\n}\n");
    assert!(msg.contains("not followed by a declaration"), "{msg}");
    assert_eq!(line, 3);

    let (msg, ..) = message("/// about an import\nimport std::x::Y\n");
    assert!(msg.contains("not followed by a definition"), "{msg}");

    let (msg, ..) = message("board D {\n    .a: R\n    //! too late\n}\n");
    assert!(msg.contains("belong at the very start"), "{msg}");

    let (msg, ..) = message("part P {}\n//! too late for the file\n");
    assert!(msg.contains("belong at the very start"), "{msg}");

    let (msg, ..) = message("board D {\n    .a: R /// trailing\n}\n");
    assert!(msg.contains("found a doc comment"), "{msg}");
}

#[test]
fn the_status_indicator_example_parses() {
    let file = parse(STATUS_INDICATOR).unwrap();
    assert_eq!(file.inner_docs, ["A small status-light board."]);
    assert_eq!(file.imports.len(), 5);

    let circuit = &file.definitions[0];
    assert_eq!(circuit.kind, DefKind::Module);
    assert_eq!(circuit.name.name, "StatusIndicator");
    assert_eq!(
        circuit.docs,
        ["A reusable status light indicator with a logic input and local power filtering."]
    );

    let board = &file.definitions[1];
    assert_eq!(board.kind, DefKind::Board);
    let Item::Decl(process) = &board.items[0] else {
        panic!()
    };
    assert_eq!(names(&process.ty.segments), ["TwoLayer"]);
    let Item::Decl(rail) = &board.items[1] else {
        panic!()
    };
    assert_eq!(
        rail.docs,
        ["The board runs from a 3.3 V rail and exposes one logic input."]
    );
    assert_eq!(names(&rail.ty.segments), ["Rail"]);

    // The two chains inside the circuit are series chains ending at `.power.neg`.
    let chains: Vec<_> = circuit
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Connect(c)
                if c.rest.iter().all(|(op, _)| *op == ConnectOp::Series) && !c.rest.is_empty() =>
            {
                Some(c)
            }
            _ => None,
        })
        .collect();
    assert_eq!(chains.len(), 2);
    assert!(
        chains
            .iter()
            .all(|c| c.rest.last().unwrap().1.to_source() == ".power.neg")
    );
}
