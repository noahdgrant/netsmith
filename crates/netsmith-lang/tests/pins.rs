//! The pin table (`pins`), `tie`, and what the parser says about a row that is wrong
//! (ADR 0016).

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const LDO: &str = "\
part Ldo9 {
    pins \"DFN-8-EP\" {
        //  pin        kind        name     pads
        .vout.pos      power       \"OUT\"    \"1\" \"2\"
        .sense         input       \"SENSE\"  \"3\"
        .nc            no_connect  \"NC\"     \"5\"
        .pa2           bidirectional  \"PA2\"  \"12\"  fn GPIO UART1_TX SPI1_SCK
        .boot0         input       \"BOOT0\"  \"31\"  optional
        .tx            output      \"TX\"     \"8\"   required
    }
    pins {
        .a passive \"A\" \"1\"
    }
    tie .sense ~ .vout.pos
}
";

fn err(src: &str) -> (String, usize, usize) {
    let e = parse(src).unwrap_err();
    let (line, col) = e.line_col(src);
    (e.message, line, col)
}

fn row(row: &str) -> String {
    format!("part P {{\n    pins \"X\" {{\n        {row}\n    }}\n}}\n")
}

fn row_error(row_text: &str) -> (String, usize, usize) {
    err(&row(row_text))
}

#[test]
fn a_pin_table_has_one_row_per_pin() {
    let file = parse(LDO).unwrap();
    let items = &file.definitions[0].items;
    let Item::Pins(table) = &items[0] else {
        panic!("{items:?}")
    };
    assert_eq!(
        table.package.as_ref().map(|p| p.0.as_str()),
        Some("DFN-8-EP")
    );
    assert_eq!(table.rows.len(), 6);

    let out = &table.rows[0];
    assert_eq!(out.pin.to_source(), ".vout.pos");
    assert_eq!(out.kind, PinKind::Power);
    assert_eq!(out.name, "OUT");
    assert_eq!(
        out.pads.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(),
        ["1", "2"]
    );
    assert!(out.functions.is_empty() && out.requirement.is_none());

    let pa2 = &table.rows[3];
    assert_eq!(pa2.kind, PinKind::Bidirectional);
    assert_eq!(
        pa2.functions
            .iter()
            .map(|f| f.name.name.as_str())
            .collect::<Vec<_>>(),
        ["GPIO", "UART1_TX", "SPI1_SCK"]
    );
    assert_eq!(
        table.rows[4].requirement.map(|r| r.0),
        Some(Requirement::Optional)
    );
    assert_eq!(
        table.rows[5].requirement.map(|r| r.0),
        Some(Requirement::Required)
    );

    let Item::Pins(unnamed) = &items[1] else {
        panic!()
    };
    assert!(unnamed.package.is_none());
    let Item::Tie(tie) = &items[2] else { panic!() };
    assert_eq!(
        (tie.a.to_source().as_str(), tie.b.to_source().as_str()),
        (".sense", ".vout.pos")
    );
}

#[test]
fn a_pin_table_prints_and_parses_back_to_the_same_tree() {
    let file = parse(LDO).unwrap();
    let printed = file.to_source();
    assert!(
        printed.contains("        .pa2 bidirectional \"PA2\" \"12\" fn GPIO UART1_TX SPI1_SCK\n"),
        "{printed}"
    );
    assert!(
        printed.contains("    tie .sense ~ .vout.pos\n"),
        "{printed}"
    );
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
}

#[test]
fn rows_end_at_a_newline_or_a_semicolon() {
    let file = parse("part P {\n    pins { .a passive \"A\" \"1\"; .b passive \"B\" \"2\" }\n}\n")
        .unwrap();
    let Item::Pins(table) = &file.definitions[0].items[0] else {
        panic!()
    };
    assert_eq!(table.rows.len(), 2);
}

#[test]
fn pins_and_tie_are_still_names_where_a_name_is_meant() {
    // Keywords only at the start of a statement, and only with what follows them.
    let file = parse("board B {\n    .pins: Rail\n    pins.pos ~ tie.neg\n}\n").unwrap();
    assert!(matches!(&file.definitions[0].items[1], Item::Connect(_)));
}

#[test]
fn an_unknown_pin_kind_lists_the_kinds() {
    let (msg, line, col) = row_error(".a tristate \"A\" \"1\"");
    assert_eq!(
        msg,
        "unknown pin kind `tristate`; the kinds are power, input, output, bidirectional, passive and no_connect"
    );
    assert_eq!((line, col), (3, 12));
}

#[test]
fn a_row_that_stops_early_says_what_is_missing() {
    let (msg, ..) = row_error(".a");
    assert!(
        msg.starts_with("expected a pin kind (power, input, output, bidirectional, passive and no_connect) after `.a`, found end of line"),
        "{msg}"
    );
    let (msg, ..) = row_error(".a passive");
    assert!(
        msg.contains(
            "the datasheet name of `.a` as a quoted string such as \"SENSE\", found end of line"
        ),
        "{msg}"
    );
    let (msg, ..) = row_error(".a passive \"A\"");
    assert!(
        msg.contains("a pad of `.a` as a quoted string such as \"3\", found end of line"),
        "{msg}"
    );
}

#[test]
fn a_name_must_be_quoted_and_so_must_a_pad() {
    let (msg, ..) = row_error(".a passive A \"1\"");
    assert!(
        msg.contains("the datasheet name of `.a` as a quoted string"),
        "{msg}"
    );
    assert!(msg.ends_with("found `A`"), "{msg}");

    let (msg, line, col) = row_error(".a passive \"A\" 1");
    assert_eq!(msg, "a pad is text: write \"1\" with quotes, not `1`");
    assert_eq!((line, col), (3, 24));

    let (msg, ..) = row_error(".a passive \"A\" \"1\" 2");
    assert_eq!(msg, "a pad is text: write \"2\" with quotes, not `2`");

    let (msg, ..) = row_error(".a passive \"A\" P1");
    assert!(msg.contains("a pad of `.a` as a quoted string"), "{msg}");
}

#[test]
fn a_row_starts_with_a_pin() {
    let (msg, ..) = row_error("a passive \"A\" \"1\"");
    assert_eq!(
        msg,
        "expected a pin such as `.sense` at the start of a row, found `a`"
    );
}

#[test]
fn functions_are_plain_names_and_fn_needs_one() {
    let (msg, ..) = row_error(".a bidirectional \"A\" \"1\" fn");
    assert!(
        msg.starts_with("expected a function name after `fn`, such as `fn GPIO UART1_TX`"),
        "{msg}"
    );
    let (msg, ..) = row_error(".a bidirectional \"A\" \"1\" fn \"GPIO\"");
    assert_eq!(
        msg,
        "a function is a plain name such as UART1_TX, without quotes"
    );
    let (msg, ..) = row_error(".a bidirectional \"A\" \"1\" fn optional");
    assert!(
        msg.starts_with("expected a function name after `fn`"),
        "{msg}"
    );
}

#[test]
fn optional_and_required_close_a_row() {
    let (msg, ..) = row_error(".a input \"A\" \"1\" optional required");
    assert!(
        msg.contains("end of line or `;`, found `required`"),
        "{msg}"
    );
    let (msg, ..) = row_error(".a input \"A\" \"1\" optional fn GPIO");
    assert!(msg.contains("end of line or `;`, found `fn`"), "{msg}");
}

#[test]
fn a_pin_table_needs_braces_and_a_quoted_package() {
    let (msg, ..) = err("part P {\n    pins \"X\"\n}\n");
    assert!(msg.contains("`{` after the package name"), "{msg}");
    let (msg, ..) = err("part P {\n    pins \"\" { }\n}\n");
    assert_eq!(msg, "a package name cannot be empty");
    let (msg, ..) = err("part P {\n    pins \"X\" {\n        .a passive \"A\" \"1\"\n");
    assert!(msg.contains("never closed"), "{msg}");
    let (msg, ..) = err(
        "part P {\n    pins \"X\" {\n        /// note\n        .a passive \"A\" \"1\"\n    }\n}\n",
    );
    assert_eq!(msg, "a pin table takes `//` notes, not doc comments");
}

#[test]
fn a_tie_joins_exactly_two_pins() {
    let (msg, ..) = err("part P {\n    tie .a\n}\n");
    assert!(msg.contains("`~` between the two pins of a `tie`"), "{msg}");
    let (msg, ..) = err("part P {\n    tie .a ~ .b ~ .c\n}\n");
    assert_eq!(msg, "a `tie` joins exactly two pins: `tie .a ~ .b`");
    let (msg, ..) = err("part P {\n    tie .a ~> .b\n}\n");
    assert!(msg.contains("`~` between the two pins of a `tie`"), "{msg}");
}

#[test]
fn doc_comments_do_not_attach_to_pins_or_ties() {
    for stmt in ["pins { .a passive \"A\" \"1\" }", "tie .a ~ .b"] {
        let (msg, ..) = err(&format!("part P {{\n    /// note\n    {stmt}\n}}\n"));
        assert_eq!(
            msg, "doc comments can only be attached to declarations",
            "{stmt}"
        );
    }
}

#[test]
fn a_catalog_may_name_the_column_that_picks_a_row() {
    let src = "part P {\n    catalog mcu by .mpn { .mpn, .package }\n}\n";
    let file = parse(src).unwrap();
    let Item::Catalog(c) = &file.definitions[0].items[0] else {
        panic!()
    };
    assert_eq!(
        c.key.as_ref().map(|k| k.to_source()).as_deref(),
        Some(".mpn")
    );
    assert_eq!(file.to_source(), src);

    let plain = parse("part P {\n    catalog r { .a }\n}\n").unwrap();
    let Item::Catalog(c) = &plain.definitions[0].items[0] else {
        panic!()
    };
    assert!(c.key.is_none());

    let (msg, ..) = err("part P {\n    catalog mcu by { .a }\n}\n");
    assert!(msg.starts_with("expected a name, found `{`"), "{msg}");
    let (msg, ..) = err("part P {\n    catalog mcu .mpn { .a }\n}\n");
    assert!(msg.contains("`{` or `by` after the catalog name"), "{msg}");
}

#[test]
fn a_function_may_carry_its_own_kind() {
    let src = row(
        ".pa2 bidirectional \"PA2\" \"12\" fn GPIO:bidirectional UART1_TX:output UART1_RX:input SPI1_SCK optional",
    );
    let file = parse(&src).unwrap();
    let Item::Pins(table) = &file.definitions[0].items[0] else {
        panic!()
    };
    let row = &table.rows[0];
    let got: Vec<(&str, Option<PinKind>)> = row
        .functions
        .iter()
        .map(|f| (f.name.name.as_str(), f.kind.map(|k| k.0)))
        .collect();
    assert_eq!(
        got,
        [
            ("GPIO", Some(PinKind::Bidirectional)),
            ("UART1_TX", Some(PinKind::Output)),
            ("UART1_RX", Some(PinKind::Input)),
            ("SPI1_SCK", None),
        ]
    );
    assert_eq!(row.requirement.map(|r| r.0), Some(Requirement::Optional));
    // It prints back to the source it came from.
    let printed = file.to_source();
    assert!(
        printed.contains("fn GPIO:bidirectional UART1_TX:output UART1_RX:input SPI1_SCK optional"),
        "{printed}"
    );
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
}

#[test]
fn the_kind_of_a_function_must_be_a_kind_and_touch_its_name() {
    let (msg, line, col) = row_error(".a bidirectional \"A\" \"1\" fn GPIO:tristate");
    assert_eq!(
        msg,
        "unknown pin kind `tristate`; the kinds are power, input, output, bidirectional, passive and no_connect"
    );
    assert_eq!((line, col), (3, 42));
    let (msg, ..) = row_error(".a bidirectional \"A\" \"1\" fn GPIO:");
    assert!(
        msg.contains("a pin kind (power, input, output, bidirectional, passive and no_connect) right after `GPIO:`"),
        "{msg}"
    );
    let (msg, ..) = row_error(".a bidirectional \"A\" \"1\" fn GPIO :output");
    assert_eq!(
        msg,
        "write the kind of function `GPIO` against its name, as `GPIO:output` (no spaces)"
    );
    let (msg, ..) = row_error(".a bidirectional \"A\" \"1\" fn GPIO: output");
    assert!(msg.contains("right after `GPIO:`"), "{msg}");
}
