//! `#[name]` and `#[name(arg, ...)]` above a definition or a member declaration (ADR 0123).
//! The parser takes any name; the checker knows the attributes.

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const CAPACITOR: &str = "\
/// A capacitor.
#[kind(capacitor)]
#[series(E24, E96)]
generic Capacitor {
    /// The voltage across it.
    #[role(bias)]
    .bias: Voltage
}
";

fn only(src: &str) -> Definition {
    parse(src).unwrap().definitions.remove(0)
}

fn err(src: &str) -> String {
    parse(src).unwrap_err().message
}

fn words(attr: &Attribute) -> (String, Vec<String>) {
    (
        attr.name.name.clone(),
        attr.args
            .iter()
            .map(|a| match a {
                AttrArg::Name(i) => i.name.clone(),
                AttrArg::Text { text, .. } => format!("{text:?}"),
            })
            .collect(),
    )
}

#[test]
fn a_definition_and_a_member_carry_their_attributes_in_order() {
    let def = only(CAPACITOR);
    assert_eq!(def.docs, [" A capacitor.".trim_start().to_string()]);
    let attrs: Vec<_> = def.attrs.iter().map(words).collect();
    assert_eq!(
        attrs,
        [
            ("kind".to_string(), vec!["capacitor".to_string()]),
            ("series".to_string(), vec!["E24".to_string(), "E96".to_string()]),
        ]
    );
    let Item::Decl(bias) = &def.items[0] else {
        panic!("not a declaration")
    };
    assert_eq!(bias.docs.len(), 1);
    assert_eq!(
        bias.attrs.iter().map(words).collect::<Vec<_>>(),
        [("role".to_string(), vec!["bias".to_string()])]
    );
}

#[test]
fn an_attribute_takes_no_arguments_names_or_texts() {
    let def = only("#[bank]\n#[value(\"LED\")]\npart P {\n}\n");
    let attrs: Vec<_> = def.attrs.iter().map(words).collect();
    assert_eq!(
        attrs,
        [
            ("bank".to_string(), Vec::new()),
            ("value".to_string(), vec!["\"LED\"".to_string()]),
        ]
    );
}

#[test]
fn attributes_print_back_to_their_source() {
    let file = parse(CAPACITOR).unwrap();
    let printed = file.to_source();
    assert!(
        printed.contains("#[kind(capacitor)]\n#[series(E24, E96)]\ngeneric Capacitor {"),
        "{printed}"
    );
    assert!(
        printed.contains("    #[role(bias)]\n    .bias: Voltage"),
        "{printed}"
    );
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
    let text = parse("#[value(\"a \\\"b\\\"\")]\npart P {\n}\n").unwrap();
    let def = &parse(&text.to_source()).unwrap().definitions[0];
    assert_eq!(words(&def.attrs[0]), words(&text.definitions[0].attrs[0]));
}

#[test]
fn an_attribute_on_a_statement_is_refused() {
    assert_eq!(
        err("part P {\n    #[role(bias)]\n    .a ~ .b\n}\n"),
        "an attribute belongs on a member declaration (`#[role(bias)] .bias: Voltage`), not on a statement"
    );
}

#[test]
fn an_attribute_with_nothing_after_it_is_refused() {
    assert_eq!(
        err("part P {\n    #[role(bias)]\n}\n"),
        "this attribute is not followed by a declaration"
    );
    assert_eq!(
        err("#[bank]\n"),
        "an attribute belongs on a definition or a member declaration, and nothing follows this one"
    );
    assert_eq!(
        err("#[bank]\nimport std::sources::Rail\n"),
        "an attribute belongs on a definition or a member declaration, and nothing follows this one"
    );
}

#[test]
fn an_attribute_that_is_not_closed_is_refused() {
    assert!(err("#[series(E24\ngeneric G {\n}\n").contains("`,` or `)`"));
    assert!(err("#[bank\ngeneric G {\n}\n").contains("`]` to close the attribute"));
    assert!(err("#[3]\ngeneric G {\n}\n").contains("the name of an attribute"));
}
