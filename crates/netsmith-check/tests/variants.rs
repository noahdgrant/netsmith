//! A part's `.variants` table and a design's `.needs` (issue 1311): the part lists its order codes
//! and their features, the design states what it needs, and the part's `.mpn` is the one code
//! that has every need. A code without a needed feature is refused.

use netsmith_check::{Domain, Loader, check};

const MODEM: &str = r#"
part Modem {
    .variants {
        "MDM-100-G" GNSS LTE
        "MDM-100" LTE
        "MDM-100-W" WIFI
    }
    pins { .a passive "A" "1" }
}
"#;

fn board(body: &str) -> String {
    format!("{MODEM}\nboard B {{\n{body}\n}}\n")
}

fn mpn_of(body: &str) -> Option<Domain> {
    let checked = check("main.nsm", &board(body), &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let b = checked.standalone_boards().next().unwrap();
    b.model.domain_at("u.mpn").cloned()
}

#[test]
fn a_need_leaves_the_one_code_that_has_it() {
    assert_eq!(
        mpn_of(".u: Modem\n.u.needs &= GNSS"),
        Some(Domain::Text("MDM-100-G".into()))
    );
    assert_eq!(
        mpn_of(".u: Modem { .needs &= WIFI }"),
        Some(Domain::Text("MDM-100-W".into()))
    );
}

#[test]
fn two_needs_need_both_features() {
    assert_eq!(
        mpn_of(".u: Modem\n.u.needs &= LTE\n.u.needs &= GNSS"),
        Some(Domain::Text("MDM-100-G".into()))
    );
}

#[test]
fn a_named_code_that_has_every_need_is_kept() {
    assert_eq!(
        mpn_of(".u: Modem { .mpn &= \"MDM-100-G\" }\n.u.needs &= LTE"),
        Some(Domain::Text("MDM-100-G".into()))
    );
}

fn errors(body: &str) -> String {
    let checked = check("main.nsm", &board(body), &Loader::std());
    assert!(!checked.is_ok(), "{body} was accepted");
    checked.render()
}

#[test]
fn a_named_code_without_a_need_is_refused() {
    let out = errors(".u: Modem { .mpn &= \"MDM-100\" }\n.u.needs &= GNSS");
    assert!(
        out.contains("`u.mpn` is `MDM-100`, which has no `GNSS`, and the design needs `GNSS` (main.nsm:13); the order codes that have them are `MDM-100-G`"),
        "{out}"
    );
}

#[test]
fn a_named_code_the_table_lacks_is_refused() {
    let out = errors(".u: Modem { .mpn &= \"MDM-200\" }");
    assert!(
        out.contains("`u.mpn` is `MDM-200`, which is not an order code of `Modem`; its order codes are `MDM-100-G`, `MDM-100` and `MDM-100-W`"),
        "{out}"
    );
}

#[test]
fn several_codes_left_ask_for_a_need_or_a_code() {
    let out = errors(".u: Modem\n.u.needs &= LTE");
    assert!(
        out.contains("`u` is a `Modem`, which is sold as `MDM-100-G` (GNSS, LTE) and `MDM-100` (LTE); state what the design needs of it (`.u.needs &= GNSS`, one statement for each feature) or name the order code (`.u.mpn &= \"MDM-100-G\"`)"),
        "{out}"
    );
}

#[test]
fn needs_no_code_meets_are_refused() {
    let out = errors(".u: Modem\n.u.needs &= GNSS\n.u.needs &= WIFI");
    assert!(
        out.contains("no order code of `Modem` has `GNSS` and `WIFI` together, which the design needs of `u`: `MDM-100-G` (GNSS, LTE), `MDM-100` (LTE) and `MDM-100-W` (WIFI)"),
        "{out}"
    );
}

#[test]
fn a_feature_the_table_does_not_write_is_refused() {
    let out = errors(".u: Modem { .mpn &= \"MDM-100\" }\n.u.needs &= GNNS");
    assert!(
        out.contains("`GNNS` is not a feature of `Modem` (`u`); the features of `Modem` are `GNSS`, `LTE` and `WIFI`; did you mean `GNSS`?"),
        "{out}"
    );
}

#[test]
fn only_narrowing_states_a_need() {
    let out = errors(".u: Modem { .mpn &= \"MDM-100\" }\n.u.needs ?= LTE");
    assert!(
        out.contains("`?=` does not state a need: a design states what it needs of `u` with `&=`"),
        "{out}"
    );
    let out = errors(".u: Modem { .mpn &= \"MDM-100\" }\n.u.needs &= \"LTE\"");
    assert!(
        out.contains("`.needs` takes the name of a feature, written bare"),
        "{out}"
    );
}

#[test]
fn a_need_reaches_a_part_inside_a_module() {
    let src = format!(
        "{MODEM}\nmodule Radio {{\n    .u: Modem\n    .u.needs &= LTE\n}}\nboard B {{\n    .radio: Radio\n    .radio.u.needs &= GNSS\n}}\n"
    );
    let checked = check("main.nsm", &src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let b = checked.standalone_boards().next().unwrap();
    assert_eq!(
        b.model.domain_at("radio.u.mpn"),
        Some(&Domain::Text("MDM-100-G".into()))
    );
}

#[test]
fn a_part_alone_is_not_asked_for_a_code() {
    let checked = check("main.nsm", MODEM, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
}

#[test]
fn a_part_with_one_code_needs_nothing() {
    let src = r#"
part Single {
    .variants {
        "ONE-1" GNSS
    }
    pins { .a passive "A" "1" }
}

board B {
    .u: Single
}
"#;
    let checked = check("main.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let b = checked.standalone_boards().next().unwrap();
    assert_eq!(
        b.model.domain_at("u.mpn"),
        Some(&Domain::Text("ONE-1".into()))
    );
}

#[test]
fn the_table_is_a_parts_own() {
    let src = r#"
board B {
    .variants {
        "X" A
    }
}
"#;
    let checked = check("main.nsm", src, &Loader::std());
    assert!(
        checked
            .render()
            .contains("`.variants` lists the order codes of a part, in the part's own definition"),
        "{}",
        checked.render()
    );
}

#[test]
fn a_part_with_the_table_does_not_declare_needs() {
    let src = r#"
part P {
    .needs: String
    .variants {
        "X" A
    }
    pins { .a passive "A" "1" }
}
"#;
    let checked = check("main.nsm", src, &Loader::std());
    assert!(
        checked
            .render()
            .contains("a part that states `.variants` has `.needs`"),
        "{}",
        checked.render()
    );
}

#[test]
fn a_code_twice_is_refused() {
    let src = r#"
part P {
    .variants {
        "X" A
        "X" B
    }
    pins { .a passive "A" "1" }
}
"#;
    let checked = check("main.nsm", src, &Loader::std());
    assert!(
        checked
            .render()
            .contains("the order code `X` is in this `.variants` table twice"),
        "{}",
        checked.render()
    );
}

#[test]
fn a_part_without_the_table_has_no_needs() {
    let src = r#"
part P {
    pins { .a passive "A" "1" }
}

board B {
    .u: P
    .u.needs &= GNSS
}
"#;
    let checked = check("main.nsm", src, &Loader::std());
    assert!(!checked.is_ok(), "{}", checked.render());
}
