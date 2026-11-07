//! `peripheral` and `shared` (ADR 0062): what the parser reads, how the printer writes it
//! back, and what it says about a statement that is wrong. Every part here is invented.

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const ZMCU: &str = "\
part Zmcu {
    peripheral UARTE0 { UARTE0_TX:output UARTE0_RX:input }
    peripheral SPIM0 {
        SPIM0_SCK:output
        SPIM0_MOSI:output
        SPIM0_MISO:input
    }
    shared SERIAL0 { UARTE0 SPIM0 }
}
";

fn err(src: &str) -> (String, usize, usize) {
    let e = parse(src).unwrap_err();
    let (line, col) = e.line_col(src);
    (e.message, line, col)
}

fn in_part(statement: &str) -> String {
    format!("part P {{\n    {statement}\n}}\n")
}

#[test]
fn a_peripheral_lists_functions_with_their_kinds() {
    let file = parse(ZMCU).unwrap();
    let items = &file.definitions[0].items;
    let Item::Peripheral(uart) = &items[0] else {
        panic!("{items:?}")
    };
    assert_eq!(uart.name.name, "UARTE0");
    let listed: Vec<(&str, Option<PinKind>)> = uart
        .functions
        .iter()
        .map(|f| (f.name.name.as_str(), f.kind.map(|(k, _)| k)))
        .collect();
    assert_eq!(
        listed,
        [
            ("UARTE0_TX", Some(PinKind::Output)),
            ("UARTE0_RX", Some(PinKind::Input))
        ]
    );
    // Functions may be on several lines, with or without a kind.
    let Item::Peripheral(spi) = &items[1] else {
        panic!("{items:?}")
    };
    assert_eq!(spi.functions.len(), 3);
    let file = parse(&in_part("peripheral Plain { A B:passive C }")).unwrap();
    let Item::Peripheral(p) = &file.definitions[0].items[0] else {
        panic!()
    };
    let kinds: Vec<Option<PinKind>> = p.functions.iter().map(|f| f.kind.map(|(k, _)| k)).collect();
    assert_eq!(kinds, [None, Some(PinKind::Passive), None]);
}

#[test]
fn a_shared_resource_lists_its_members() {
    let file = parse(ZMCU).unwrap();
    let Item::Shared(s) = &file.definitions[0].items[2] else {
        panic!()
    };
    assert_eq!(s.name.name, "SERIAL0");
    let members: Vec<String> = s
        .members
        .iter()
        .map(|m| match m {
            SharedMember::Name(n) => n.name.clone(),
            SharedMember::Pin(p) => p.to_source(),
        })
        .collect();
    assert_eq!(members, ["UARTE0", "SPIM0"]);
}

#[test]
fn both_print_and_parse_back_to_the_same_tree() {
    let file = parse(ZMCU).unwrap();
    let printed = file.to_source();
    assert!(
        printed.contains("    peripheral UARTE0 { UARTE0_TX:output UARTE0_RX:input }\n"),
        "{printed}"
    );
    assert!(
        printed.contains(
            "    peripheral SPIM0 { SPIM0_SCK:output SPIM0_MOSI:output SPIM0_MISO:input }\n"
        ),
        "{printed}"
    );
    assert!(
        printed.contains("    shared SERIAL0 { UARTE0 SPIM0 }\n"),
        "{printed}"
    );
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
}

#[test]
fn statements_end_at_a_newline_or_a_semicolon() {
    let file = parse(&in_part(
        "peripheral A { A_TX }; peripheral B { B_TX }; shared S { A B }",
    ))
    .unwrap();
    assert_eq!(file.definitions[0].items.len(), 3);
    let file = parse(&in_part("peripheral A { A_TX; A_RX }")).unwrap();
    let Item::Peripheral(p) = &file.definitions[0].items[0] else {
        panic!()
    };
    assert_eq!(p.functions.len(), 2);
}

#[test]
fn the_words_are_still_names_where_a_name_is_meant() {
    // Keywords only at the start of a statement, and only with a name after them.
    let file = parse(
        "board B {\n    .peripheral: Rail\n    .shared: Rail\n    peripheral.pos ~ shared.neg\n}\n",
    )
    .unwrap();
    assert!(matches!(&file.definitions[0].items[2], Item::Connect(_)));
}

#[test]
fn a_doc_comment_does_not_attach_to_either() {
    for stmt in ["peripheral A { A_TX }", "shared S { A }"] {
        let (msg, ..) = err(&format!("part P {{\n    /// note\n    {stmt}\n}}\n"));
        assert_eq!(
            msg, "doc comments can only be attached to declarations",
            "{stmt}"
        );
    }
}

#[test]
fn a_peripheral_needs_functions_and_a_shared_resource_members() {
    let (msg, line, col) = err(&in_part("peripheral UARTE0 { }"));
    assert_eq!(
        msg,
        "`peripheral UARTE0` has no functions; list at least one in the braces, such as `UARTE0_TX:output`"
    );
    assert_eq!((line, col), (2, 16));
    let (msg, ..) = err(&in_part("shared SERIAL0 { }"));
    assert_eq!(
        msg,
        "`shared SERIAL0` has no members; list the peripherals, functions or pins that share it in braces"
    );
}

#[test]
fn braces_are_needed_and_must_close() {
    let (msg, ..) = err(&in_part("peripheral UARTE0 UARTE0_TX"));
    assert!(
        msg.starts_with("expected `{` or `on` after the name of the peripheral"),
        "{msg}"
    );
    let (msg, ..) = err(&in_part("shared SERIAL0 UARTE0"));
    assert!(
        msg.starts_with("expected `{` or `limit` after the name of the shared resource"),
        "{msg}"
    );
    let (msg, line, _) = err("part P {\n    peripheral A { A_TX\n");
    assert_eq!(msg, "this `{` is never closed");
    assert_eq!(line, 2);
}

#[test]
fn a_function_is_a_plain_name_with_its_kind_against_it() {
    let (msg, ..) = err(&in_part("peripheral A { \"A_TX\" }"));
    assert_eq!(
        msg,
        "a function is a plain name such as UARTE0_TX, without quotes"
    );
    let (msg, ..) = err(&in_part("peripheral A { A_TX:tristate }"));
    assert!(msg.starts_with("unknown pin kind `tristate`"), "{msg}");
    let (msg, ..) = err(&in_part("peripheral A { A_TX :output }"));
    assert!(
        msg.starts_with("write the kind of function `A_TX` against its name"),
        "{msg}"
    );
    let (msg, ..) = err(&in_part("peripheral A { A_TX /// a note\n }"));
    assert_eq!(msg, "a peripheral takes `//` notes, not doc comments");
}

// ---- `on CLASS` (stage 2) ----

#[test]
fn a_peripheral_may_go_on_a_class() {
    let file = parse(&in_part(
        "peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }",
    ))
    .unwrap();
    let Item::Peripheral(p) = &file.definitions[0].items[0] else {
        panic!()
    };
    assert_eq!(p.on.as_ref().map(|c| c.name.as_str()), Some("APP_GPIO"));
    assert_eq!(p.functions.len(), 2);
    // Without `on`, none.
    let file = parse(&in_part("peripheral HSSPI { HSSPI_SCK:output }")).unwrap();
    let Item::Peripheral(p) = &file.definitions[0].items[0] else {
        panic!()
    };
    assert!(p.on.is_none());
}

#[test]
fn on_prints_back_and_is_a_plain_name_elsewhere() {
    let file = parse(&in_part(
        "peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }",
    ))
    .unwrap();
    let printed = file.to_source();
    assert!(
        printed
            .contains("    peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }\n"),
        "{printed}"
    );
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
    // A peripheral or a function may be called `on`, and a class too.
    parse(&in_part("peripheral on { on_tx }")).unwrap();
    parse(&in_part("peripheral A on on { A_TX }")).unwrap();
}

#[test]
fn on_needs_a_function_and_braces() {
    let (msg, ..) = err(&in_part("peripheral UARTE0 on { UARTE0_TX }"));
    assert!(
        msg.starts_with("expected a function after `on`, such as `on APP_GPIO`"),
        "{msg}"
    );
    let (msg, ..) = err(&in_part("peripheral UARTE0 on APP_GPIO UARTE0_TX"));
    assert!(
        msg.starts_with("expected `{` after the function that the peripheral is on"),
        "{msg}"
    );
    let (msg, ..) = err(&in_part("peripheral UARTE0 on \"APP_GPIO\" { UARTE0_TX }"));
    assert!(msg.starts_with("expected a function after `on`"), "{msg}");
}

// ---- `limit N`, and function and pin members (stage 3) ----

#[test]
fn a_shared_resource_may_state_a_limit_and_list_functions_and_pins() {
    let file = parse(&in_part(
        "shared UARTS limit 2 { UARTE0 UARTE1 UARTE2_TX .p10 .vdd.pos }",
    ))
    .unwrap();
    let Item::Shared(s) = &file.definitions[0].items[0] else {
        panic!()
    };
    assert_eq!(s.limit.map(|(n, _)| n), Some(2));
    let shown: Vec<String> = s
        .members
        .iter()
        .map(|m| match m {
            SharedMember::Name(n) => format!("name {}", n.name),
            SharedMember::Pin(p) => format!("pin {}", p.to_source()),
        })
        .collect();
    assert_eq!(
        shown,
        [
            "name UARTE0",
            "name UARTE1",
            "name UARTE2_TX",
            "pin .p10",
            "pin .vdd.pos"
        ]
    );
    // No limit is none.
    let file = parse(&in_part("shared S { A B }")).unwrap();
    let Item::Shared(s) = &file.definitions[0].items[0] else {
        panic!()
    };
    assert!(s.limit.is_none());
}

#[test]
fn the_limit_and_the_members_print_back() {
    let src = "part P {\n    shared UARTS limit 2 { UARTE0 UARTE1 .p10 }\n    shared REFERENCE { .p10 .p11 }\n}\n";
    let printed = parse(src).unwrap().to_source();
    assert_eq!(printed, src);
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
}

#[test]
fn a_limit_is_a_whole_number_of_at_least_one() {
    for (text, shown) in [("0", "0"), ("1.5", "1.5"), ("-2", "-2")] {
        let (msg, ..) = err(&in_part(&format!("shared S limit {text} {{ A B }}")));
        assert!(
            msg.starts_with("a limit is a whole number of at least 1, such as `limit 2`")
                || msg.starts_with("expected a whole number after `limit`"),
            "{shown}: {msg}"
        );
    }
    let (msg, ..) = err(&in_part("shared S limit 2V { A B C }"));
    assert_eq!(
        msg,
        "a limit is a count, a whole number with no unit, such as `limit 2`; `2V` has one"
    );
    let (msg, ..) = err(&in_part("shared S limit { A B }"));
    assert!(
        msg.starts_with("expected a whole number after `limit`, such as `limit 2`"),
        "{msg}"
    );
    let (msg, ..) = err(&in_part("shared S limit 2 A B"));
    assert!(msg.starts_with("expected `{` after the limit"), "{msg}");
}

#[test]
fn limit_is_a_plain_name_elsewhere() {
    // A shared resource or a peripheral may be called `limit`, and a function too.
    parse(&in_part("shared limit { A B }")).unwrap();
    parse(&in_part("peripheral limit { limit_tx }")).unwrap();
    parse(&in_part("shared S limit 2 { limit A B }")).unwrap();
}
