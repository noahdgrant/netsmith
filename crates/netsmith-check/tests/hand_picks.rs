//! `.lcsc` and `.mpn` are members of every part (ADR 0122): a board narrows them to hold a part to
//! a catalog row, and a part that declares either declares what it already has: the checker
//! refuses it (issue #696).

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
fn a_part_may_not_declare_them_at_all() {
    for name in ["mpn", "lcsc"] {
        for ty in ["String", "Voltage"] {
            let checked = check(
                "main.nsm",
                &source(&format!(".{name}: {ty}"), ""),
                &Loader::std(),
            );
            assert!(
                checked.render().contains(&format!(
                    "`{name}` is a member every part has, as text: the number of the catalog row it is (ADR 0122); do not declare it, and narrow it with `.{name} &= \"...\"`"
                )),
                ".{name}: {ty}: {}",
                checked.render()
            );
        }
    }
}

#[test]
fn a_generic_may_not_declare_them_either() {
    let checked = check(
        "main.nsm",
        "generic G {\n    pins { }\n    .mpn: String\n}\n",
        &Loader::std(),
    );
    assert!(
        checked.render().contains("`mpn` is a member every part has"),
        "{}",
        checked.render()
    );
}

#[test]
fn narrowing_them_stays_fine() {
    let checked = check(
        "main.nsm",
        &source("", ".mpn &= \"ZZ-1\"; .lcsc &= \"C1\""),
        &Loader::std(),
    );
    assert!(checked.is_ok(), "{}", checked.render());
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
