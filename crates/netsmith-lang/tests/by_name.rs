//! `.left ~ .right { a = b } except { c }`: the parse and the printed form (ADR 0117).

use netsmith_lang::ast::*;
use netsmith_lang::parse;

fn join_map(src: &str) -> JoinMap {
    let file = parse(&format!("board B {{\n    {src}\n}}\n")).unwrap();
    match &file.definitions[0].items[0] {
        Item::JoinMap(b) => b.clone(),
        other => panic!("not a join with a map: {other:?}"),
    }
}

fn error(src: &str) -> String {
    parse(&format!("board B {{\n    {src}\n}}\n"))
        .unwrap_err()
        .message
}

#[test]
fn the_parts_of_the_statement() {
    let b = join_map(".headers ~ .module { gpio4 = io4; gpio5 = io5 } except { gnd vbus }");
    assert_eq!(b.left.to_source(), ".headers");
    assert_eq!(b.right.to_source(), ".module");
    let renames: Vec<(String, String)> = b
        .renames
        .iter()
        .map(|(a, c)| (a.name.clone(), c.name.clone()))
        .collect();
    assert_eq!(
        renames,
        [
            ("gpio4".to_string(), "io4".to_string()),
            ("gpio5".to_string(), "io5".to_string())
        ]
    );
    let except: Vec<&str> = b.except.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(except, ["gnd", "vbus"]);
}

#[test]
fn the_statement_prints_back_to_its_source() {
    for src in [
        ".headers ~ .module { gpio4 = io4 }",
        ".headers ~ .module except { gnd }",
        ".headers ~ .module { gpio4 = io4; gpio5 = io5 } except { gnd vbus }",
    ] {
        let text = format!("board B {{\n    {src}\n}}\n");
        let printed = parse(&text).unwrap().to_source();
        assert!(printed.contains(src), "{printed}");
        assert_eq!(parse(&printed).unwrap().to_source(), printed);
    }
}

#[test]
fn a_rename_map_may_take_a_line_per_rename() {
    let b = join_map(".h ~ .m {\n        gpio4 = io4\n        gpio5 = io5\n    } except {\n        gnd\n    }");
    assert_eq!(b.renames.len(), 2);
    assert_eq!(b.except.len(), 1);
}

#[test]
fn a_plain_join_of_two_ports_is_a_connection() {
    let file = parse("board B {\n    .headers ~ .module\n}\n").unwrap();
    assert!(matches!(&file.definitions[0].items[0], Item::Connect(_)));
}

#[test]
fn a_map_follows_a_join_of_two_ports_with_one_tilde() {
    let message = "a rename map or `except` follows a join of two ports with one `~`: write `.a ~ .b { x = y }`";
    assert_eq!(error(".a ~ .b ~ .c { x = y }"), message);
    assert_eq!(error(".a ~> .b except { c }"), message);
}

#[test]
fn malformed_maps_are_refused() {
    assert!(error(".a ~ .b { gpio4 io4 }").contains("`=`"));
    assert_eq!(
        error(".a ~ .b { }"),
        "an empty rename map; leave it out when every name matches"
    );
    assert_eq!(
        error(".a ~ .b except { }"),
        "an empty `except`; leave it out when no member is left out"
    );
    assert!(error(".a ~ .b { .gpio4 = io4 }").contains("a rename such as `gpio4 = io4`"));
    assert!(error(".a ~ .b except { .gnd }").contains("the name of a member left out"));
}

#[test]
fn a_doc_comment_is_not_attached_to_it() {
    assert_eq!(
        error("/// The headers.\n    .a ~ .b except { gnd }"),
        "doc comments can only be attached to declarations"
    );
}
