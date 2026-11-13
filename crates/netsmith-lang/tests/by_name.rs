//! `.left ~ .right by name { a = b } except { c }`: the parse and the printed form (ADR 0117).

use netsmith_lang::ast::*;
use netsmith_lang::parse;

fn by_name(src: &str) -> ByName {
    let file = parse(&format!("board B {{\n    {src}\n}}\n")).unwrap();
    match &file.definitions[0].items[0] {
        Item::ByName(b) => b.clone(),
        other => panic!("not a `by name`: {other:?}"),
    }
}

fn error(src: &str) -> String {
    parse(&format!("board B {{\n    {src}\n}}\n"))
        .unwrap_err()
        .message
}

#[test]
fn the_parts_of_the_statement() {
    let b = by_name(".headers ~ .module by name { gpio4 = io4; gpio5 = io5 } except { gnd vbus }");
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
        ".headers ~ .module by name",
        ".a.bus ~ .b.bus by name",
        ".headers ~ .module by name { gpio4 = io4 }",
        ".headers ~ .module by name except { gnd }",
        ".headers ~ .module by name { gpio4 = io4; gpio5 = io5 } except { gnd vbus }",
    ] {
        let text = format!("board B {{\n    {src}\n}}\n");
        let printed = parse(&text).unwrap().to_source();
        assert!(printed.contains(src), "{printed}");
        assert_eq!(parse(&printed).unwrap().to_source(), printed);
    }
}

#[test]
fn a_rename_map_may_take_a_line_per_rename() {
    let b = by_name(".h ~ .m by name {\n        gpio4 = io4\n        gpio5 = io5\n    } except {\n        gnd\n    }");
    assert_eq!(b.renames.len(), 2);
    assert_eq!(b.except.len(), 1);
}

#[test]
fn the_statement_joins_two_ports_with_one_tilde() {
    assert_eq!(
        error(".a ~ .b ~ .c by name"),
        "`by name` joins two ports with one `~`: write `.a ~ .b by name`"
    );
    assert_eq!(
        error(".a ~> .b by name"),
        "`by name` joins two ports with one `~`: write `.a ~ .b by name`"
    );
}

#[test]
fn malformed_maps_are_refused() {
    assert!(error(".a ~ .b by names").contains("`name` after `by`"));
    assert!(error(".a ~ .b by name { gpio4 io4 }").contains("`=`"));
    assert_eq!(
        error(".a ~ .b by name { }"),
        "an empty rename map; write `by name` alone when every name matches"
    );
    assert_eq!(
        error(".a ~ .b by name except { }"),
        "an empty `except`; leave it out when no member is left out"
    );
    assert!(error(".a ~ .b by name { .gpio4 = io4 }").contains("a rename such as `gpio4 = io4`"));
    assert!(error(".a ~ .b by name except { .gnd }").contains("the name of a member left out"));
}

#[test]
fn a_doc_comment_is_not_attached_to_it() {
    assert_eq!(
        error("/// The headers.\n    .a ~ .b by name"),
        "doc comments can only be attached to declarations"
    );
}
