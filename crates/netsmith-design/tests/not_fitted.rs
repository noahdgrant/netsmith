//! A passive that is not fitted until it is tuned (issue #1273): `.fitted &= false` and
//! `.tune &= <range>`. No catalog row unless the value is stated, the value judged over the
//! range, assembled by nobody, marked DNP in the design.

use netsmith_check::{Loader, check};
use netsmith_design::{Assembly, design_board};
use netsmith_solve::{Catalogs, Solution, solve};

fn source(shunt: &str) -> String {
    format!(
        r#"import std::parts::passive::{{Capacitor, Resistor}}
import std::sources::Rail

board Match {{
    .supply: Rail {{ .voltage &= 3.3V +/- 5%; .pos.net &= "VCC" }}
    .feed: Resistor {{ .resistance &= 1kohm +/- 1% }}
    .load: Resistor {{ .resistance &= 1kohm +/- 1% }}
    .supply.pos ~> .feed ~> .load ~> .supply.neg
    .shunt: Capacitor {{ {shunt} }}
    .shunt.a ~ .feed.b
    .shunt.b ~ .supply.neg
}}
"#
    )
}

fn solved(shunt: &str) -> (netsmith_check::Checked, Solution) {
    let c = check("t.nsm", &source(shunt), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    (c, s)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn picked(s: &Solution, path: &str) -> Option<String> {
    let id = s.model.inst_at(path).expect("the part is there");
    s.picks
        .iter()
        .find(|p| p.part == id)
        .map(|p| p.candidate.clone())
}

#[test]
fn a_part_not_fitted_that_states_no_value_is_given_no_row_and_takes_its_range() {
    let (c, s) = solved(r#".fitted &= false; .tune &= 1pF..10pF; .package &= "0402""#);
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert_eq!(picked(&s, "shunt"), None);
    assert!(picked(&s, "feed").is_some(), "the fitted parts are picked");
    let d = design_board(&c.boards[0], &s).unwrap_or_else(|e| panic!("{e:#?}"));
    let i = d.design.parts.iter().position(|p| p.path == "shunt").unwrap();
    let shunt = &d.sourcing[i];
    assert_eq!(shunt.assembly, Assembly::None);
    assert_eq!(shunt.value, "1pF..10pF");
    assert_eq!(shunt.lcsc, None);
    assert_eq!(d.design.parts[i].value, "DNP 1pF..10pF");
    let dnp = shunt.not_fitted.as_ref().expect("marked not fitted");
    assert_eq!(dnp.tune.as_deref(), Some("1pF..10pF"));
    assert_eq!(d.design.features.not_fitted, vec![dnp.clone()]);
    assert!(d.design.features.is_not_fitted(&shunt.refdes));
}

#[test]
fn a_part_not_fitted_that_states_its_value_is_picked_and_still_not_fitted() {
    let (c, s) = solved(".fitted &= false; .tune &= 10nF..1uF; .capacitance &= 100nF +/- 10%");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(picked(&s, "shunt").is_some(), "the stated value is picked");
    let d = design_board(&c.boards[0], &s).unwrap_or_else(|e| panic!("{e:#?}"));
    let i = d.design.parts.iter().position(|p| p.path == "shunt").unwrap();
    assert_eq!(d.sourcing[i].assembly, Assembly::None);
    assert!(d.sourcing[i].not_fitted.is_some());
    assert_eq!(d.design.parts[i].value, "DNP 100n");
}

#[test]
fn a_stated_value_outside_the_range_is_refused_with_both() {
    let (_, s) = solved(".fitted &= false; .tune &= 10nF..1uF; .capacitance &= 4.7uF +/- 10%");
    let all = messages(&s).join("\n");
    assert!(
        all.contains("shunt.capacitance must be at most max(shunt.tune)") && all.contains("1uF"),
        "{all}"
    );
}

#[test]
fn a_part_that_is_fitted_and_states_a_range_is_picked_inside_it() {
    let (c, s) = solved(".tune &= 90nF..110nF");
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let row = picked(&s, "shunt").expect("a fitted part is picked");
    assert!(row.starts_with("100nF"), "{row}");
    let d = design_board(&c.boards[0], &s).unwrap_or_else(|e| panic!("{e:#?}"));
    assert!(d.design.features.not_fitted.is_empty());
    assert!(d.sourcing.iter().all(|s| s.not_fitted.is_none()));
}

#[test]
fn a_part_not_fitted_with_no_value_and_no_package_is_refused_with_what_to_write() {
    let c = check(
        "t.nsm",
        &source(".fitted &= false; .tune &= 1pF..10pF"),
        &Loader::std(),
    );
    let all = c.render();
    assert!(
        all.contains("`shunt` is not fitted and states no `.capacitance`")
            && all.contains(".shunt.package &= \"0402\""),
        "{all}"
    );
}

#[test]
fn true_and_false_are_written_bare() {
    let c = check(
        "t.nsm",
        &source(r#".fitted &= Bool::false; .package &= "0402""#),
        &Loader::std(),
    );
    assert!(
        c.render()
            .contains("`Bool::false` is written `false`: `true` and `false` need no `Bool::`"),
        "{}",
        c.render()
    );
    let c = check(
        "t.nsm",
        &source(r#".fitted &= false; .fitted &= true; .package &= "0402""#),
        &Loader::std(),
    );
    assert!(
        c.render()
            .contains("contradiction on `shunt.fitted`: it is already false, which does not overlap true"),
        "{}",
        c.render()
    );
}
