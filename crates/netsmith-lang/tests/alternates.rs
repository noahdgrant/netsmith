//! `alternate .u: ModuleB` (issue 1312): another part that the footprint of a member declared
//! beside it can take in its place.

use netsmith_lang::ast::*;
use netsmith_lang::{format, parse};

const BOARD: &str = "\
board Tracker {
    .u: ModuleA
    /// Fitted when the first is out of stock.
    alternate .u: ModuleB { .assembly &= AssemblyMode::Hand }
    alternate .u: ModuleC
}
";

#[test]
fn an_alternate_is_a_declaration_after_the_word() {
    let file = parse(BOARD).unwrap();
    let items = &file.definitions[0].items;
    let Item::Alternate(a) = &items[1] else {
        panic!("an alternate, not {:?}", items[1])
    };
    assert_eq!(a.decl.name.segments[0].name, "u");
    assert_eq!(a.decl.ty.segments[0].name, "ModuleB");
    assert_eq!(a.decl.docs, ["Fitted when the first is out of stock."]);
    assert!(a.decl.body.is_some());
    assert_eq!(&BOARD[a.word.start..a.word.end], "alternate");
    assert!(matches!(&items[2], Item::Alternate(c) if c.decl.body.is_none()));
}

#[test]
fn it_prints_back_and_formats_as_written() {
    let printed = parse(BOARD).unwrap().to_source();
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
    assert!(printed.contains("alternate .u: ModuleB {"), "{printed}");
    assert_eq!(format(BOARD).as_deref(), Ok(BOARD));
}

#[test]
fn an_alternate_is_one_member_of_one_part() {
    for (source, at) in [
        ("board B {\n    alternate .u: 2 x ModuleB\n}\n", "2 x"),
        ("board B {\n    alternate .u: ModuleB optional\n}\n", "optional"),
        ("board B {\n    alternate .m.u: ModuleB\n}\n", ".m.u"),
    ] {
        let e = parse(source).unwrap_err();
        assert!(
            e.message.contains("an alternate is one part in the place of one member"),
            "{}",
            e.message
        );
        assert_eq!(&source[e.span.start..e.span.end], at);
    }
    let e = parse("board B {\n    alternate .u ~ .v\n}\n").unwrap_err();
    assert!(
        e.message.contains("`alternate` is followed by a declaration"),
        "{}",
        e.message
    );
}

#[test]
fn alternate_is_still_an_ordinary_name() {
    let file = parse("board B {\n    .alternate: Length\n    .alternate &= 1mm\n}\n").unwrap();
    assert_eq!(file.definitions[0].items.len(), 2);
}
