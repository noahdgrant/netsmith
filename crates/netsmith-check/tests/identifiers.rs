//! Pin functions, colours and packages are checked names, not free text (ADR 0118): a
//! function is a bare name from the part's pin table, a colour a bare variant of the enum
//! `Color`, and a package text that names one of the part's pin tables. A misspelling is an
//! error at its line with the nearest name.
//!
//! `Mcu` and `Ldo` are invented for these tests; they are not real parts.

use netsmith_check::*;

const MCU: &str = r#"
part Mcu {
    pins {
        .pa9  bidirectional "PA9"  "1" fn GPIO USART1_TX
        .pa10 bidirectional "PA10" "2" fn GPIO USART1_RX
    }
}

part Ldo {
    pins "SOT-23-5" {
        .vin  passive "VIN"  "1"
        .vout passive "VOUT" "5"
    }
    pins "VSON-10(2x3)" {
        .vin  passive "VIN"  "1"
        .vout passive "VOUT" "10"
    }
    .package: Package
}
"#;

fn run(statements: &str) -> Checked {
    check(
        "test.nsm",
        &format!(
            "{MCU}\nimport std::parts::passive::{{Led, Resistor}}\n\nboard B {{\n    .u: Mcu\n    .r: Resistor\n    .s: Resistor\n    .u.pa9 ~ .r.a\n    .u.pa10 ~ .s.a\n{statements}\n}}\n"
        ),
        &Loader::std(),
    )
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The source line each diagnostic points at, trimmed.
fn lines(c: &Checked) -> Vec<String> {
    c.diagnostics
        .iter()
        .map(|d| {
            let (line, _) = d.line_col(&c.sources);
            c.sources.get(d.source).text.lines().nth(line - 1).unwrap().trim().to_string()
        })
        .collect()
}

fn text_at(c: &Checked, path: &str) -> Option<String> {
    match c.board("B").unwrap().model.domain_at(path) {
        Some(Domain::Text(t)) => Some(t.clone()),
        _ => None,
    }
}

const BOTH: &str = "    .u.pa9.function &= USART1_TX\n    .u.pa10.function &= USART1_RX\n";

#[test]
fn a_function_is_a_bare_name_from_the_pin_table() {
    let c = run(BOTH);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(text_at(&c, "u.pa9.function").as_deref(), Some("USART1_TX"));
}

#[test]
fn a_quoted_function_is_refused_and_says_what_to_write() {
    let c = run("    .u.pa9.function &= \"USART1_TX\"\n    .u.pa10.function &= USART1_RX\n");
    assert_eq!(
        messages(&c),
        [
            "the function of `u.pa9` is a name from the pin table of its part, not text: write `USART1_TX`, without quotes"
        ]
    );
    assert_eq!(lines(&c), [".u.pa9.function &= \"USART1_TX\""]);
}

#[test]
fn a_misspelt_function_is_refused_at_its_line_with_the_nearest() {
    let c = run("    .u.pa9.function &= USART1_XT\n    .u.pa10.function &= USART1_RX\n");
    assert_eq!(
        messages(&c),
        ["`u.pa9` cannot be `USART1_XT`; its functions are GPIO, USART1_TX; did you mean `USART1_TX`?"]
    );
    assert_eq!(lines(&c), [".u.pa9.function &= USART1_XT"]);
}

#[test]
fn a_function_of_another_pin_says_where_it_is() {
    let c = run("    .u.pa9.function &= USART1_RX\n    .u.pa10.function &= GPIO\n");
    assert_eq!(
        messages(&c),
        ["`u.pa9` cannot be `USART1_RX`; its functions are GPIO, USART1_TX; that function is offered only on `u.pa10`"]
    );
}

#[test]
fn a_module_passes_a_bare_function_on_with_an_equation() {
    let src = format!(
        "{MCU}\nimport std::parts::passive::Resistor\n\nmodule Hub {{\n    .tx: Pin\n    .mcu: Mcu\n    .tx ~ .mcu.pa9\n    .mcu.pa9.function == .tx.function\n}}\n\nboard B {{\n    .hub: Hub\n    .r: Resistor\n    .hub.tx ~ .r.a\n    .hub.tx.function &= USART1_TX\n}}\n"
    );
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_colour_is_a_bare_variant_of_color() {
    let c = run(&format!("{BOTH}    .led: Led {{ .color &= Red }}\n"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(text_at(&c, "led.color").as_deref(), Some("Red"));
    // The spelling with the enum's name is the same value, and `==` reads a bare variant
    // beside a field of the enum as `&=` does.
    let c = check(
        "test.nsm",
        "import std::parts::passive::{Led, Color}\nboard B {\n    .led: Led { .color &= Color::Green }\n    .led.color == Green\n}\n",
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_misspelt_colour_is_refused_at_its_line_with_the_nearest() {
    let c = run(&format!("{BOTH}    .led: Led {{ .color &= Gren }}\n"));
    assert_eq!(
        messages(&c),
        ["`Color` has no variant `Gren`; its variants are `Red`, `Green`, `Blue`, `White` and `Yellow`; did you mean `Green`?"]
    );
    assert_eq!(lines(&c), [".led: Led { .color &= Gren }"]);
}

#[test]
fn a_quoted_colour_is_refused_and_names_the_variant() {
    let c = run(&format!("{BOTH}    .led: Led {{ .color &= \"RED\" }}\n"));
    assert_eq!(
        messages(&c),
        [
            "`led.color` holds a variant of `Color`, but this value is text; a variant of `Color` is a name, not text: write `Red` (the variants are `Red`, `Green`, `Blue`, `White` and `Yellow`)"
        ]
    );
}

#[test]
fn a_bare_name_that_is_no_variant_is_a_member_of_an_enclosing_block() {
    // `shade` is not a variant of `Color`: it is the module's member, as a bare name is
    // anywhere else, and the module's colour is passed to its LED.
    let src = "\
import std::parts::passive::{Led, Color}
module Lamp {
    .shade: Color
    .led: Led { .color &= shade }
}
board B {
    .lamp: Lamp { .shade &= Blue }
}
";
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_package_names_one_of_the_parts_pin_tables() {
    let c = run(&format!("{BOTH}    .ldo: Ldo {{ .package &= \"VSON-10(2x3)\" }}\n"));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_misspelt_package_is_refused_at_its_line_with_the_nearest() {
    let c = run(&format!("{BOTH}    .ldo: Ldo {{ .package &= \"SOT23-5\" }}\n"));
    assert_eq!(
        messages(&c),
        ["`ldo` is a `Ldo` in package `SOT23-5`, which has no pin table; its tables are for `SOT-23-5`, `VSON-10(2x3)`; did you mean \"SOT-23-5\"?"]
    );
    assert_eq!(lines(&c), [".ldo: Ldo { .package &= \"SOT23-5\" }"]);
}
