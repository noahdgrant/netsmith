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

// ---- through a use of a generic that declares `.needs` (issue 1402) ----------------------

/// A generic that wants order codes declares `.needs: String optional`; `Modem` fills it with
/// its table, and `Basic`, first in the order of candidates, has no table.
const RADIO: &str = r#"
generic Radio {
    pins { .a passive }
    .needs: String optional
}

part Basic provides Radio {
    pins { .a passive "A" "1" }
}

part Modem provides Radio {
    .variants {
        "MDM-100-G" GNSS LTE
        "MDM-100" LTE
    }
    pins { .a passive "A" "1" }
}
"#;

fn use_board(body: &str, provider: Option<&str>) -> netsmith_check::Checked {
    let src = format!("{RADIO}\nboard B {{\n{body}\n}}\n");
    let mut assignment = netsmith_check::Assignment::new();
    if let Some(p) = provider {
        assignment.insert(
            "B".to_string(),
            [("u".to_string(), p.to_string())].into_iter().collect(),
        );
    }
    netsmith_check::check_with("main.nsm", &src, &Loader::std(), &assignment)
}

#[test]
fn a_need_through_a_use_narrows_the_providers_code() {
    let checked = use_board(".u: Radio\n.u.needs &= GNSS", Some("main::Modem"));
    assert!(checked.is_ok(), "{}", checked.render());
    let b = checked.standalone_boards().next().unwrap();
    assert_eq!(
        b.model.domain_at("u.mpn"),
        Some(&Domain::Text("MDM-100-G".into()))
    );
    let checked = use_board(".u: Radio { .needs &= LTE\n.mpn &= \"MDM-100\" }", Some("main::Modem"));
    assert!(checked.is_ok(), "{}", checked.render());
}

#[test]
fn a_need_a_provider_without_a_table_cannot_meet_is_a_provider_conflict() {
    let checked = use_board(".u: Radio\n.u.needs &= GNSS", None);
    let out = checked.render();
    assert!(
        out.contains("`u` is a `Basic`, which states no `.variants`: it is one order code with no feature, and the design needs `GNSS` of it"),
        "{out}"
    );
    // Kept apart, so the build tries `Modem` before it stops on it (ADR 0113).
    let b = checked.standalone_boards().next().unwrap();
    assert_eq!(b.model.provider_conflicts.len(), 1, "{out}");
    assert_eq!(checked.diagnostics.len(), 1, "{out}");
}

#[test]
fn a_providers_code_without_the_need_is_a_provider_conflict() {
    let checked = use_board(".u: Radio\n.u.needs &= WIFI", Some("main::Modem"));
    let out = checked.render();
    assert!(out.contains("`WIFI` is not a feature of `Modem` (`u`)"), "{out}");
    // Every refusal is the provider's: another may have the feature.
    let b = checked.standalone_boards().next().unwrap();
    assert_eq!(b.model.provider_conflicts.len(), checked.diagnostics.len(), "{out}");
    let checked = use_board(".u: Radio", Some("main::Modem"));
    let out = checked.render();
    assert!(out.contains("`u` is a `Modem`, which is sold as"), "{out}");
    let b = checked.standalone_boards().next().unwrap();
    assert_eq!(b.model.provider_conflicts.len(), 1, "{out}");
}

#[test]
fn a_use_of_a_generic_without_needs_does_not_reach_it() {
    let src = r#"
generic Plain {
    pins { .a passive }
}

part Modem provides Plain {
    .variants {
        "MDM-100-G" GNSS
    }
    pins { .a passive "A" "1" }
}

board B {
    .u: Plain
    .u.needs &= GNSS
}
"#;
    let checked = check("main.nsm", src, &Loader::std());
    assert!(
        checked
            .render()
            .contains("`needs` is not declared by `Plain`"),
        "{}",
        checked.render()
    );
}

#[test]
fn a_generic_declares_needs_as_optional_text() {
    let src = r#"
generic Radio {
    pins { .a passive }
    .needs: String
}

part Modem provides Radio {
    .variants {
        "MDM-100-G" GNSS
    }
    pins { .a passive "A" "1" }
}
"#;
    let checked = check("main.nsm", src, &Loader::std());
    assert!(
        checked
            .render()
            .contains("declare it `.needs: String optional` in `Radio`"),
        "{}",
        checked.render()
    );
}
