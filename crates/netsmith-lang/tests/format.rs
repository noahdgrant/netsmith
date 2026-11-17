//! The formatter (issue #1103): the layout of each construct, the comments it keeps, and that the
//! standard library is formatted, comes back from another layout, and is formatted again unchanged.
//! The examples are checked by `scripts/check-example-fmt.sh` in CI, and the guide's by its test.

use std::fs;
use std::path::{Path, PathBuf};

use netsmith_lang::{FormatError, format, parse};

/// `src` formatted, which must also be formatted already.
fn fmt(src: &str) -> String {
    let out = format(src).unwrap_or_else(|e| panic!("{e}\n{src}"));
    assert_eq!(format(&out).as_deref(), Ok(out.as_str()), "not idempotent:\n{out}");
    out
}

/// `src` is the formatter's layout: it formats to itself.
fn kept(src: &str) {
    assert_eq!(fmt(src), src);
}

fn std_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "nsm") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std"),
        &mut out,
    );
    out.sort();
    assert!(out.len() >= 10, "found {} files in std/", out.len());
    out
}

/// `src` laid out otherwise with the same meaning: each indent changed, each space between tokens
/// doubled (outside strings and comments), a blank line after each `{` that ends a line, and each
/// blank line doubled.
fn unformatted(src: &str) -> String {
    let mut out = String::new();
    for (n, line) in src.lines().enumerate() {
        let body = line.trim_start();
        out.push_str(&"  ".repeat(n % 3));
        let mut in_string = false;
        let mut chars = body.chars().peekable();
        while let Some(c) = chars.next() {
            if !in_string && c == '/' && chars.peek() == Some(&'/') {
                out.push(c);
                out.extend(chars.by_ref());
                break;
            }
            if c == '"' {
                in_string = !in_string;
            }
            out.push(c);
            if c == ' ' && !in_string {
                out.push(' ');
            }
        }
        out.push('\n');
        if body.is_empty() || (body.ends_with('{') && !body.starts_with("//")) {
            out.push('\n');
        }
    }
    out
}

#[test]
fn the_standard_library_is_formatted_and_comes_back_from_another_layout() {
    for path in std_files() {
        let src = fs::read_to_string(&path).unwrap();
        let name = path.display();
        let out = format(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            out == src,
            "{name} is not formatted: run `netsmith project fmt` on it"
        );
        let messy = unformatted(&src);
        assert_ne!(messy, src);
        assert_eq!(
            format(&messy).as_deref(),
            Ok(src.as_str()),
            "{name} in another layout does not format back"
        );
    }
}

/// `src` with every block that holds something written over lines: a line end after each `{`
/// and before each `}` that are not `{}`, and in place of each `; ` (outside strings and
/// comments). Its line breaks are not the formatter's, which joins them again where they fit.
fn exploded(src: &str) -> String {
    let mut out = String::new();
    for line in src.lines() {
        let mut in_string = false;
        let mut chars = line.chars().peekable();
        let mut prev = ' ';
        while let Some(c) = chars.next() {
            if !in_string && c == '/' && chars.peek() == Some(&'/') {
                out.push(c);
                out.extend(chars.by_ref());
                break;
            }
            if c == '"' {
                in_string = !in_string;
            }
            if !in_string && c == '{' && chars.peek().is_some_and(|n| *n != '}') {
                out.push_str("{\n");
            } else if !in_string && c == '}' && prev != '{' {
                out.push_str("\n}");
            } else if !in_string && c == ';' {
                out.push('\n');
            } else {
                out.push(c);
            }
            prev = c;
        }
        out.push('\n');
    }
    out
}

#[test]
fn the_standard_library_comes_back_from_a_layout_with_every_block_over_lines() {
    for path in std_files() {
        let src = fs::read_to_string(&path).unwrap();
        let name = path.display();
        let messy = exploded(&src);
        assert_eq!(
            format(&messy).as_deref(),
            Ok(src.as_str()),
            "{name} with every block over lines does not format back"
        );
    }
}

#[test]
fn indent_spaces_and_one_line_blocks() {
    assert_eq!(
        fmt("import std::sources::Rail\nboard B{\n\t.v:Rail{.voltage&=3.3V+/-5%;.pos.net&=\"3V3\"}\n  .v.pos~.v.neg\n}"),
        "import std::sources::Rail\n\nboard B {\n    .v: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= \"3V3\" }\n    .v.pos ~ .v.neg\n}\n"
    );
    // `±` is `+/-`, a sign touches its number, a range has no spaces, a row range has them.
    assert_eq!(
        fmt("board B {\n    .t &= -40C .. 85C\n    .x &= 2V ± 1% at -5mA\n    .y == .a.b - .c\n}\n"),
        "board B {\n    .t &= -40C..85C\n    .x &= 2V +/- 1% at -5mA\n    .y == .a.b - .c\n}\n"
    );
    // An open range keeps a space before the `}` of its block.
    assert_eq!(
        fmt("board B {\n    .r: Rail {\n        .min_track &= 0.4mm..\n    }\n}\n"),
        "board B {\n    .r: Rail { .min_track &= 0.4mm.. }\n}\n"
    );
    kept("board B {\n    .c: 4 x Capacitor { .capacitance &= 100nF +/- 10% }\n    .c[3].package &= \"0603\"\n    .r: Buck<Lt8641> {}\n}\n");
    kept("#[series(E24, E96)]\npart P {\n    #[role(bias)]\n    .r: Resistor\n}\n");
}

#[test]
fn a_block_over_several_lines_has_a_statement_to_a_line() {
    assert_eq!(
        fmt("board B { .a: R\n    .b: R; .c: R;\n    .d: R }\n"),
        "board B {\n    .a: R\n    .b: R\n    .c: R\n    .d: R\n}\n"
    );
    // The body of a definition or of a section is over lines, also when written on one.
    assert_eq!(
        fmt("part P { .a: R; support { .c: C } }\n"),
        "part P {\n    .a: R\n    support {\n        .c: C\n    }\n}\n"
    );
    kept("interface I {}\n");
}

#[test]
fn a_statement_or_block_over_several_lines_is_joined_when_it_fits() {
    assert_eq!(
        fmt("board B {\n    .r: R {\n        .x &= 1V\n\n        .y &= 2V\n    }  // the r\n}\n"),
        "board B {\n    .r: R { .x &= 1V; .y &= 2V }  // the r\n}\n"
    );
    // A connection, a relation, a polygon, an import's names, a catalog's columns and a
    // peripheral's functions.
    assert_eq!(
        fmt("import std::parts::passive::{\n    Resistor,\n    Led,\n}\nboard B {\n    .a ~>\n        .r ~>\n        .b\n    .x &=\n        3V +/-\n        5%\n}\n"),
        "import std::parts::passive::{Led, Resistor}\n\nboard B {\n    .a ~> .r ~> .b\n    .x &= 3V +/- 5%\n}\n"
    );
    assert_eq!(
        fmt("part P {\n    catalog c by .mpn {\n        .mpn,\n        .package\n    }\n    peripheral U as Uart {\n        U0TXD:output as tx\n        U0RXD:input as rx\n    }\n}\n"),
        "part P {\n    catalog c by .mpn { .mpn, .package }\n    peripheral U as Uart { U0TXD:output as tx; U0RXD:input as rx }\n}\n"
    );
    assert_eq!(
        fmt("physical for B {\n    case {\n        .outline: Outline {\n            polygon (0mm, 0mm) (25mm, 0mm)\n                (25mm, 15mm)\n        }\n    }\n}\n"),
        "physical for B {\n    case {\n        .outline: Outline { polygon (0mm, 0mm) (25mm, 0mm) (25mm, 15mm) }\n    }\n}\n"
    );
    // Blocks in a block: the outer one does not fit, and the inner ones are joined.
    let wide = "board B {\n    .reg: BuckConverter {\n        .vout: Rail {\n            .voltage &= 3.3V +/- 5%\n        }\n        .input_capacitor.capacitance &= 22uF +/- 10%\n        .output_capacitor.capacitance &= 47uF\n    }\n}\n";
    assert_eq!(
        fmt(wide),
        "board B {\n    .reg: BuckConverter {\n        .vout: Rail { .voltage &= 3.3V +/- 5% }\n        .input_capacitor.capacitance &= 22uF +/- 10%\n        .output_capacitor.capacitance &= 47uF\n    }\n}\n"
    );
}

#[test]
fn a_comment_doc_comment_or_attribute_keeps_its_block_broken() {
    kept("board B {\n    .r: R {\n        // the value\n        .x &= 1V\n    }\n}\n");
    kept("board B {\n    .r: R {\n        .x &= 1V  // the value\n        .y &= 2V\n    }\n}\n");
    kept("board B {\n    .r: R {\n        /// The value.\n        .x: S\n    }\n}\n");
    kept("board B {\n    .r: R {\n        #[role(bias)]\n        .x: R\n    }\n}\n");
    // Inside a statement that is not joined, what can be is.
    assert_eq!(
        fmt("board B {\n    .r: R {\n        // the value\n        .x: S {\n            .y &= 2V\n        }\n    }\n}\n"),
        "board B {\n    .r: R {\n        // the value\n        .x: S { .y &= 2V }\n    }\n}\n"
    );
}

#[test]
fn a_long_n_x_kind_statement_breaks_in_its_block() {
    let long = "board B {\n    .bulk: 2 x Capacitor { .capacitance &= 22uF +/- 10%; .package &= \"CASE-A-3216-18\"; .max_voltage >= 6.3V }\n}\n";
    assert_eq!(
        fmt(long),
        "board B {\n    .bulk: 2 x Capacitor {\n        .capacitance &= 22uF +/- 10%\n        .package &= \"CASE-A-3216-18\"\n        .max_voltage >= 6.3V\n    }\n}\n"
    );
}

#[test]
fn a_long_connection_breaks_after_each_operator() {
    let src = "board B {\n    .usb_connector.vbus.pos ~> .input_protection_fuse ~> .reverse_polarity_protection_diode ~> .bulk.pos\n}\n";
    assert_eq!(
        fmt(src),
        "board B {\n    .usb_connector.vbus.pos ~>\n        .input_protection_fuse ~>\n        .reverse_polarity_protection_diode ~>\n        .bulk.pos\n}\n"
    );
}

#[test]
fn a_long_relation_breaks_after_its_operator_and_then_in_its_value() {
    let src = "board B {\n    .regulator_output_capacitor.max_voltage >= .regulator.output_rail.voltage * 1.5 + .design_margins\n}\n";
    assert_eq!(
        fmt(src),
        "board B {\n    .regulator_output_capacitor.max_voltage >=\n        .regulator.output_rail.voltage * 1.5 + .design_margins\n}\n"
    );
    let value = format!(
        "board B {{\n    .x &= {} + {} + {}\n}}\n",
        ".a_very_long_member_name.with_a_long_field",
        ".another_long_member_name.with_another_long_field",
        ".a_third_long_member_name.with_a_third_long_field",
    );
    assert_eq!(
        fmt(&value),
        "board B {\n    .x &=\n        .a_very_long_member_name.with_a_long_field +\n        .another_long_member_name.with_another_long_field +\n        .a_third_long_member_name.with_a_third_long_field\n}\n"
    );
}

#[test]
fn a_long_polygon_breaks_before_a_corner() {
    let src = "physical for B {\n    case {\n        .outline: Outline {\n            polygon (0mm, 0mm) (25mm, 0mm) round 2mm (25mm, 15mm) (40mm, 15mm) (40mm, 30mm) arc through (20mm, 34mm) (0mm, 30mm) round 3mm\n        }\n    }\n}\n";
    assert_eq!(
        fmt(src),
        "physical for B {\n    case {\n        .outline: Outline {\n            polygon (0mm, 0mm) (25mm, 0mm) round 2mm (25mm, 15mm) (40mm, 15mm)\n                (40mm, 30mm) arc through (20mm, 34mm) (0mm, 30mm) round 3mm\n        }\n    }\n}\n"
    );
}

#[test]
fn pin_tables_are_aligned_in_columns() {
    let src = "part P {\n    pins \"SOIC-8\" {\n        //  pin kind name pads\n        .vin.pos power \"VIN\" \"1\" \"8\"\n        .en input \"EN\" \"2\" abs_max 6V\n        .fb  input  \"FB\"  \"3\"  // the feedback\n\n        .pa9 bidirectional \"PA9\" \"30\" fn GPIO USART1_TX:output\n    }\n    lines limit 2 {\n        .tx output crosses .rx\n        .rx   input\n    }\n}\n";
    assert_eq!(
        fmt(src),
        "part P {\n    pins \"SOIC-8\" {\n        //  pin kind name pads\n        .vin.pos power \"VIN\" \"1\" \"8\"\n        .en      input \"EN\"  \"2\" abs_max 6V\n        .fb      input \"FB\"  \"3\"  // the feedback\n\n        .pa9 bidirectional \"PA9\" \"30\" fn GPIO USART1_TX:output\n    }\n    lines limit 2 {\n        .tx output crosses .rx\n        .rx input\n    }\n}\n"
    );
}

#[test]
fn comments_are_kept_where_they_are() {
    kept(
        "//! The file.

// A note on the import.
import std::sources::Rail  // the rail

// Before the board.
/// The board.
board B {
    //! Inside the board.

    // Alone on a line.
    .v: Rail  // at the end of a line
    .w: Rail {
        // The only line of a block.
    }
    // Last in the block.
}
",
    );
    assert_eq!(
        fmt("board B {\n.v: Rail   // two spaces before me\n        // indented as the block\n}\n"),
        "board B {\n    .v: Rail  // two spaces before me\n    // indented as the block\n}\n"
    );
    // A `////` is plain, and keeps its text.
    kept("board B {\n    //// a banner ////\n    .r: R\n}\n");
}

#[test]
fn blank_lines_are_one_at_most_and_between_definitions() {
    assert_eq!(
        fmt("//! Doc.\nimport a::B\npart P {\n\n\n    .x: B\n\n}\npart Q {}\n\n\n\nboard R {}\n"),
        "//! Doc.\n\nimport a::B\n\npart P {\n    .x: B\n}\n\npart Q {}\n\nboard R {}\n"
    );
}

#[test]
fn imports_are_sorted_as_the_editor_sorts_them_with_their_comments() {
    assert_eq!(
        fmt("//! Doc.\nimport std::sources::Rail\n// the parts\nimport std::parts::passive::{Resistor, Led, Capacitor}\nimport src::power::Bypassed\n\nimport shared::Thing\nimport std::bus::Uart\n\nboard B {}\n"),
        "//! Doc.\n\nimport std::bus::Uart\n// the parts\nimport std::parts::passive::{Capacitor, Led, Resistor}\nimport std::sources::Rail\n\nimport shared::Thing\n\nimport src::power::Bypassed\n\nboard B {}\n"
    );
}

#[test]
fn a_function_kind_touches_its_name_and_a_member_type_does_not() {
    assert_eq!(
        fmt("part P {\n    .a:R\n    peripheral U as Uart {\n        U0TXD:output as tx\n    }\n}\nmodule M<Ic:BuckIc> {}\n"),
        "part P {\n    .a: R\n    peripheral U as Uart { U0TXD:output as tx }\n}\n\nmodule M<Ic: BuckIc> {}\n"
    );
}

#[test]
fn a_text_that_does_not_parse_is_refused() {
    let Err(FormatError::Parse(e)) = format("board B {\n    .r: R /* no */\n}\n") else {
        panic!("a block comment parses");
    };
    assert_eq!(e.message, "block comments do not exist; use `//`");
    // An empty file is formatted as one.
    assert_eq!(format("\n\n").as_deref(), Ok(""));
}

#[test]
fn the_formatted_text_is_the_same_program() {
    let src = "import std::sources::Rail\nboard B {\n  .v: Rail { .voltage &= 3.3V +/- 5% }\n  .v.pos ~ .v.neg\n}\n";
    let a = parse(src).unwrap().to_source();
    let b = parse(&fmt(src)).unwrap().to_source();
    assert_eq!(a, b);
}
