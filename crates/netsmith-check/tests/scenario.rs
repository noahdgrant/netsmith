//! A board's `scenario` (issue #1258): `remove` and `source` name rails, `sense` a pin or a
//! rail, and the scenario is kept in the model for the solver.

use netsmith_check::*;

const IMPORTS: &str = "import std::sources::Rail\nimport std::parts::passive::Resistor\n";

fn checked(body: &str) -> Checked {
    let src = format!(
        "{IMPORTS}\nboard B {{\n    .usb: Rail\n    .cell: Rail\n    .r: Resistor\n    .usb.pos ~ .r.a\n    .r.b ~ .usb.neg\n{body}\n}}\n"
    );
    check("test.nsm", &src, &Loader::std())
}

fn refused(body: &str) -> String {
    let c = checked(body);
    assert!(!c.is_ok(), "`{body}` was accepted");
    c.diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_scenario_keeps_its_rails_and_sense_pins_in_the_model() {
    let c = checked("    scenario backup { remove .usb; source .cell; sense .r.a; sense .usb }");
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    let [q] = m.scenarios() else {
        panic!("{:?}", m.scenarios())
    };
    assert_eq!(q.name, "backup");
    let written = |ps: &[ScenarioPath]| ps.iter().map(|p| p.written.clone()).collect::<Vec<_>>();
    assert_eq!(written(&q.removed), [".usb"]);
    assert_eq!(written(&q.sources), [".cell"]);
    assert_eq!(written(&q.senses), [".r.a", ".usb"]);
    // A rail that is sensed is read at its `pos`.
    assert_eq!(m.path_of(q.senses[1].inst), "usb.pos");
}

#[test]
fn a_path_that_is_not_a_rail_is_refused_with_what_the_word_names() {
    let m = refused("    scenario s { remove .r; source .cell }");
    assert!(
        m.contains("`.r` is a part (`Resistor`), and `remove` names a supply rail"),
        "{m}"
    );
    let m = refused("    scenario s { remove .usb; source .r.a }");
    assert!(m.contains("`.r.a` is a pin, and `source` names a supply rail"), "{m}");
    let m = refused("    scenario s { remove .usb; source .cell; sense .usb.voltage }");
    assert!(m.contains("`.usb.voltage` is a value, and `sense` names a pin"), "{m}");
}

#[test]
fn a_rail_is_removed_or_a_source_and_named_once() {
    let m = refused("    scenario s { remove .usb; source .usb; source .cell }");
    assert!(m.contains("`.usb` is both removed and a source that stays"), "{m}");
    let m = refused("    scenario s { remove .usb; remove .usb; source .cell }");
    assert!(m.contains("`.usb` is named twice in the scenario"), "{m}");
}

#[test]
fn two_scenarios_of_a_board_have_their_own_names() {
    let m = refused(
        "    scenario s { remove .usb; source .cell }\n    scenario s { remove .cell; source .usb }",
    );
    assert!(m.contains("two scenarios called `s`"), "{m}");
}

#[test]
fn only_a_board_states_a_scenario() {
    let src = format!(
        "{IMPORTS}\nmodule M {{\n    .a: Rail\n    .b: Rail\n    scenario s {{ remove .a; source .b }}\n}}\n"
    );
    let c = check("test.nsm", &src, &Loader::std());
    let m: Vec<String> = c.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert!(
        m.iter().any(|m| m.contains("`scenario` is a statement of a board")),
        "{m:#?}"
    );
}
