//! `.a ?~ .b`: the parse and the printed form of a default join (ADR 0130).

use netsmith_lang::ast::*;
use netsmith_lang::parse;

fn wrap(src: &str) -> String {
    format!("part P {{\n    {src}\n}}\n")
}

#[test]
fn a_default_join_is_a_connection_with_the_default_operator() {
    let file = parse(&wrap(".bias ?~ .vin.neg")).unwrap();
    let Item::Connect(c) = &file.definitions[0].items[0] else {
        panic!("not a connection");
    };
    assert_eq!(c.first.to_source(), ".bias");
    assert_eq!(c.rest.len(), 1);
    assert_eq!(c.rest[0].0, ConnectOp::Default);
    assert_eq!(c.rest[0].1.to_source(), ".vin.neg");
}

#[test]
fn a_default_join_prints_back_to_its_source() {
    let printed = parse(&wrap(".bias ?~ .vin.neg")).unwrap().to_source();
    assert!(printed.contains(".bias ?~ .vin.neg"), "{printed}");
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
}

#[test]
fn a_default_join_is_not_part_of_a_chain() {
    for src in [
        ".a ?~ .b ~ .c",
        ".a ~ .b ?~ .c",
        ".a ?~ .b ~> .c",
        ".a ?~ .b ?~ .c",
    ] {
        let e = parse(&wrap(src)).unwrap_err();
        assert!(
            e.message
                .contains("`?~` joins exactly two pins and is not part of a chain"),
            "{src}: {}",
            e.message
        );
    }
}
