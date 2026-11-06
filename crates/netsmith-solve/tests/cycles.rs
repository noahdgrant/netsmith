//! Values that depend on each other in a cycle while none has a value: the solver cannot
//! start, and the build names the group instead of leaving only a list of relations that
//! were not evaluated.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// A constant-power load (0.5 W) behind a series resistor: the load's voltage is the
/// supply's minus the drop, and the drop needs the current, which needs the voltage.
fn board(extra: &str, load: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board B {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        .voltage &= 9V..16V
        .pos.net &= "VBATT"
        .neg.net &= "GND"
    }}
    .r1: Resistor {{ .resistance &= 10ohm +/- 1% }}
    .load: Rail {{
        .pos.net &= "VLOAD"
        .neg.net &= "GND"
        {extra}
    }}
    .supply.neg ~ .load.neg
    .supply.pos ~> .r1 ~> .load.pos
    {load}
}}
"#
    )
}

const CYCLE: &str =
    ".load.voltage == .supply.voltage - .r1.drop\n    .load.current == 0.5W / .load.voltage";

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn groups(s: &Solution) -> Vec<Vec<String>> {
    s.circular
        .iter()
        .map(|g| g.iter().map(|&f| s.model.path_of_field(f)).collect())
        .collect()
}

#[test]
fn a_cycle_of_unset_values_is_named_once() {
    let s = solved(&board("", CYCLE));
    let g = groups(&s);
    assert_eq!(g.len(), 1, "{g:?}");
    for field in ["load.voltage", "load.current", "r1.drop", "r1.current"] {
        assert!(g[0].iter().any(|p| p == field), "{field} is in {:?}", g[0]);
    }
    assert!(s.diagnostics.is_empty(), "a note only: {:?}", s.diagnostics);
    assert!(
        !s.deferred.is_empty(),
        "the relations are still not evaluated"
    );
}

#[test]
fn a_range_on_a_field_of_the_cycle_starts_the_solver() {
    let s = solved(&board(
        "",
        &format!("{CYCLE}\n    .load.voltage &= 5V..16V"),
    ));
    assert!(groups(&s).is_empty(), "{:?}", groups(&s));
    assert!(s.deferred.is_empty(), "{:?}", s.deferred);
}

#[test]
fn a_value_nothing_states_is_no_cycle() {
    // The current is not stated and nothing works it out, but nothing depends on itself.
    let s = solved(&board("", ".load.voltage == .supply.voltage - .r1.drop"));
    assert!(!s.deferred.is_empty());
    assert!(groups(&s).is_empty(), "{:?}", groups(&s));
}

#[test]
fn a_cycle_that_also_waits_on_a_value_nothing_states_is_left_alone() {
    // A range on one field of the cycle would not start it: `.aux.voltage` has no value.
    let src = board("", CYCLE)
        .replace(
            ".load.voltage == .supply.voltage - .r1.drop",
            ".load.voltage == .supply.voltage - .r1.drop - .aux.voltage",
        )
        .replace(
            "    .supply.neg ~ .load.neg",
            "    .aux: Rail {\n        .pos.net &= \"AUX\"\n        .neg.net &= \"GND\"\n    }\n    .supply.neg ~ .load.neg",
        );
    let s = solved(&src);
    assert!(!s.deferred.is_empty());
    assert!(groups(&s).is_empty(), "{:?}", groups(&s));
}

#[test]
fn a_design_with_everything_stated_has_none() {
    let s = solved(&board(
        ".current &= 10mA..20mA",
        ".load.voltage == .supply.voltage - .r1.drop",
    ));
    assert!(groups(&s).is_empty(), "{:?}", groups(&s));
}

// ---- the circles that stayed open, with a check they left unmade (ADR 0057) ----------------

/// Fields, closed, waiting on, checks.
type Open = (Vec<String>, bool, Vec<String>, Vec<String>);

fn open(s: &Solution) -> Vec<Open> {
    let show = |fields: &[netsmith_check::FieldId]| -> Vec<String> {
        fields.iter().map(|&f| s.model.path_of_field(f)).collect()
    };
    s.open_circles
        .iter()
        .map(|c| {
            let checks = c
                .checks
                .iter()
                .map(|&i| s.model.show_expr(&s.model.relations()[i].lhs))
                .collect();
            (show(&c.fields), c.closed, show(&c.waiting), checks)
        })
        .collect()
}

#[test]
fn a_circle_with_a_bound_that_reads_it_is_open_with_the_bound_named() {
    let s = solved(&board("", CYCLE));
    let o = open(&s);
    assert_eq!(o.len(), 1, "{o:?}");
    assert!(o[0].1, "closed: a range on a field starts it");
    assert!(o[0].2.is_empty());
    // The resistor's power rating is the bound that reads the drop and the current.
    assert_eq!(o[0].3, ["(r1.drop * r1.current)"]);
}

#[test]
fn a_circle_that_waits_on_an_unstated_value_is_open_and_names_it() {
    let src = board("", CYCLE)
        .replace(
            ".load.voltage == .supply.voltage - .r1.drop",
            ".load.voltage == .supply.voltage - .r1.drop - .aux.voltage",
        )
        .replace(
            "    .supply.neg ~ .load.neg",
            "    .aux: Rail {\n        .pos.net &= \"AUX\"\n        .neg.net &= \"GND\"\n    }\n    .supply.neg ~ .load.neg",
        );
    let s = solved(&src);
    let o = open(&s);
    assert_eq!(o.len(), 1, "{o:?}");
    assert!(!o[0].1);
    assert_eq!(o[0].2, ["aux.voltage"]);
    // The closed-groups list is what it was: this one is not in it.
    assert!(s.circular.is_empty());
}

#[test]
fn a_started_circle_and_a_design_with_nothing_unevaluated_have_no_open_circle() {
    let s = solved(&board(
        "",
        &format!("{CYCLE}\n    .load.voltage &= 5V..16V"),
    ));
    assert!(s.open_circles.is_empty(), "{:?}", open(&s));
    let s = solved(&board(
        ".current &= 10mA..20mA",
        ".load.voltage == .supply.voltage - .r1.drop",
    ));
    assert!(s.open_circles.is_empty());
}
