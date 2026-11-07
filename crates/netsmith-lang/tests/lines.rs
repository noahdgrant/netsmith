//! `lines` in a bus interface (ADR 0064): the parser, the printer, and what the parser refuses.

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const SPI: &str = "\
interface Zspi {
    .role: ZRole
    .sclk: Pin
    .miso: Pin
    .cs: Pin
    lines ZRole::Master limit 1 {
        .sclk output
        .miso input optional
    }
    lines ZRole::Slave {
        .sclk input
        .miso output required
        .cs input single
    }
}
";

const UART: &str = "\
interface Zuart {
    .tx: Pin
    .rx: Pin
    lines limit 2 {
        .tx output crosses .rx
        .rx input
    }
}
";

fn err(src: &str) -> String {
    parse(src).unwrap_err().message
}

fn tables(src: &str) -> Vec<Lines> {
    parse(src).unwrap().definitions[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Lines(l) => Some(l.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn lines_with_a_role_and_a_limit_parse() {
    let t = tables(SPI);
    assert_eq!(t.len(), 2);
    assert_eq!(t[0].role.as_ref().unwrap().to_source(), "ZRole::Master");
    assert_eq!(t[0].limit.map(|(n, _)| n), Some(1));
    assert_eq!(t[0].rows.len(), 2);
    assert_eq!(t[0].rows[0].pin.to_source(), ".sclk");
    assert_eq!(t[0].rows[0].kind, PinKind::Output);
    assert_eq!(
        t[0].rows[1].requirement.map(|(r, _)| r),
        Some(Requirement::Optional)
    );
    assert_eq!(
        t[1].rows[1].requirement.map(|(r, _)| r),
        Some(Requirement::Required)
    );
    assert!(t[1].rows[2].single.is_some());
    assert!(t[1].limit.is_none());
}

#[test]
fn lines_with_no_role_and_a_crossing_parse() {
    let t = tables(UART);
    assert_eq!(t.len(), 1);
    assert!(t[0].role.is_none());
    assert_eq!(t[0].limit.map(|(n, _)| n), Some(2));
    assert_eq!(
        t[0].rows[0].crosses.as_ref().map(|p| p.to_source()),
        Some(".rx".to_string())
    );
    assert!(t[0].rows[1].crosses.is_none());
}

#[test]
fn lines_with_neither_role_nor_limit_parse() {
    let t = tables("interface I {\n    .a: Pin\n    lines {\n        .a input\n    }\n}\n");
    assert_eq!(t.len(), 1);
    assert!(t[0].role.is_none() && t[0].limit.is_none());
}

#[test]
fn the_words_of_a_row_come_in_any_order_and_print_in_one() {
    let src = "interface I {\n    .a: Pin\n    .b: Pin\n    lines {\n        .a output optional crosses .b single\n        .b input\n    }\n}\n";
    let file = parse(src).unwrap();
    let printed = file.to_source();
    assert!(
        printed.contains(".a output single crosses .b optional"),
        "{printed}"
    );
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
}

#[test]
fn lines_print_back_to_the_source_they_came_from() {
    for src in [SPI, UART] {
        let file = parse(src).unwrap();
        let printed = file.to_source();
        assert_eq!(parse(&printed).unwrap().to_source(), printed, "{printed}");
        assert!(printed.contains("lines"), "{printed}");
    }
    let printed = parse(SPI).unwrap().to_source();
    assert!(
        printed.contains("lines ZRole::Master limit 1 {"),
        "{printed}"
    );
    assert!(printed.contains(".miso output required"), "{printed}");
}

#[test]
fn lines_is_a_name_everywhere_else() {
    let file = parse("board B {\n    .lines: Rail\n    lines.pos ~ other.neg\n}\n").unwrap();
    assert!(matches!(&file.definitions[0].items[0], Item::Decl(_)));
    assert!(matches!(&file.definitions[0].items[1], Item::Connect(_)));
}

#[test]
fn a_table_that_is_empty_or_unclosed_says_so() {
    assert!(err("interface I {\n    lines {\n    }\n}\n").contains("`lines` has no rows"));
    assert!(err("interface I {\n    lines {\n        .a input\n").contains("never closed"));
}

#[test]
fn the_words_of_a_row_are_checked() {
    assert!(
        err("interface I {\n    lines {\n        .a tristate\n    }\n}\n")
            .starts_with("unknown pin kind `tristate`")
    );
    assert!(
        err("interface I {\n    lines {\n        .a input single single\n    }\n}\n")
            .contains("says `single` twice")
    );
    assert!(
        err("interface I {\n    lines {\n        .a input optional required\n    }\n}\n")
            .contains("already says whether it must be connected")
    );
    assert!(
        err("interface I {\n    lines {\n        .a output crosses\n    }\n}\n")
            .contains("the line it crosses to")
    );
    assert!(
        err("interface I {\n    lines {\n        a input\n    }\n}\n")
            .contains("a line such as `.sclk`")
    );
}

#[test]
fn a_limit_is_a_whole_number_and_notes_are_not_doc_comments() {
    assert!(
        err("interface I {\n    lines limit 0 {\n        .a input\n    }\n}\n")
            .contains("whole number of at least 1")
    );
    assert!(
        err("interface I {\n    lines {\n        /// no\n        .a input\n    }\n}\n")
            .contains("takes `//` notes")
    );
    assert!(
        err("interface I {\n    lines X::Y limit {\n        .a input\n    }\n}\n")
            .contains("whole number after `limit`")
    );
}

#[test]
fn a_doc_comment_cannot_sit_on_lines() {
    assert!(
        err("interface I {\n    /// no\n    lines {\n        .a input\n    }\n}\n")
            .contains("doc comments can only be attached to declarations")
    );
}
