//! `.status: .u.GPIO`: a declaration whose type is a capability of a part asks for a pin.

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const SRC: &str = "board B {\n    .status: .u.GPIO\n    .tx: .u.UARTE1_TX\n    .plain: Pin\n}\n";

fn decls(src: &str) -> Vec<(String, String, bool)> {
    let file = parse(src).unwrap();
    file.definitions[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Decl(d) => Some((
                d.name.to_source(),
                d.ty.segments
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
                d.ty.of_member,
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn a_request_has_a_member_path_for_its_type() {
    assert_eq!(
        decls(SRC),
        [
            (".status".to_string(), "u.GPIO".to_string(), true),
            (".tx".to_string(), "u.UARTE1_TX".to_string(), true),
            (".plain".to_string(), "Pin".to_string(), false),
        ]
    );
}

#[test]
fn it_prints_back_to_the_source_it_came_from() {
    let printed = parse(SRC).unwrap().to_source();
    assert!(printed.contains("    .status: .u.GPIO\n"), "{printed}");
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
    assert_eq!(decls(&printed), decls(SRC));
}

#[test]
fn a_request_names_a_part_and_a_capability() {
    let e = parse("board B {\n    .status: .GPIO\n}\n").unwrap_err();
    assert!(
        e.message.contains("names a part and a capability"),
        "{}",
        e.message
    );
}
