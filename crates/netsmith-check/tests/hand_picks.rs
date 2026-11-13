//! `.lcsc` and `.mpn` are members of every part (ADR 0121): a board narrows them to hold a part to
//! a catalog row, and a part that declares `.mpn: String` declares what it already has.

use netsmith_check::{Domain, Loader, check};

const PART: &str = r#"
part Zz {
    .designator &= "U"
    pins { .a passive "A" "1" }
    DECL
}

board B {
    .u: Zz { NARROW }
}
"#;

fn source(decl: &str, narrow: &str) -> String {
    PART.replace("DECL", decl).replace("NARROW", narrow)
}

#[test]
fn every_part_has_lcsc_and_mpn_as_text() {
    let checked = check(
        "main.nsm",
        &source("", ".lcsc &= \"C307331\"; .mpn &= \"ZZ-1\""),
        &Loader::std(),
    );
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().unwrap();
    assert_eq!(
        board.model.domain_at("u.lcsc"),
        Some(&Domain::Text("C307331".into()))
    );
    assert_eq!(
        board.model.domain_at("u.mpn"),
        Some(&Domain::Text("ZZ-1".into()))
    );
}

#[test]
fn a_part_may_declare_them_as_text() {
    for decl in [".mpn: String", ".lcsc: String"] {
        let checked = check(
            "main.nsm",
            &source(decl, ".mpn &= \"ZZ-1\""),
            &Loader::std(),
        );
        assert!(checked.is_ok(), "{decl}: {}", checked.render());
    }
}

#[test]
fn a_part_may_not_declare_them_as_anything_else() {
    let checked = check("main.nsm", &source(".lcsc: Voltage", ""), &Loader::std());
    assert!(
        checked.render().contains(
            "`lcsc` is a member every part has, as text: the number of the catalog row it is (ADR 0121); declare it as `.lcsc: String` or not at all, and narrow it with `.lcsc &= \"...\"`"
        ),
        "{}",
        checked.render()
    );
}

#[test]
fn two_hand_picks_of_one_part_that_differ_contradict() {
    let checked = check(
        "main.nsm",
        &source("", ".lcsc &= \"C1\"")
            .replace("    .u: Zz {", "    .u.lcsc &= \"C2\"\n    .u: Zz {"),
        &Loader::std(),
    );
    assert!(
        checked
            .render()
            .contains("contradiction on `u.lcsc`: it is already \"C"),
        "{}",
        checked.render()
    );
}
