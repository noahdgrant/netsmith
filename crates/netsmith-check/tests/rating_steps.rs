//! A part's `.max_voltage` in steps over temperature (issue #1257): the statement narrows the
//! member to its greatest step and records the steps, and every other use of steps is refused
//! with a message that names the member and shows the form.

use netsmith_check::*;

const PART: &str = "/// A supercapacitor.\npart Supercap {\n    .designator &= \"C\"\n    pins {\n        .a passive \"+\" \"1\"\n        .b passive \"-\" \"2\"\n    }\n    #[role(bias)]\n    .bias: Voltage\n    .max_voltage: Voltage\n    .other: Voltage\n    bridge .a ~> .b\n}\n";

fn checked(board: &str) -> Checked {
    check("t.nsm", &format!("{PART}{board}"), &Loader::std())
}

fn model(board: &str) -> Model {
    let c = checked(board);
    assert!(c.is_ok(), "{}", c.render());
    c.board("B").unwrap().model.clone()
}

fn refused(board: &str) -> String {
    let c = checked(board);
    assert!(!c.is_ok(), "this should not check:\n{board}");
    c.render()
}

fn part(m: &Model, name: &str) -> InstId {
    match m.member(m.root, name) {
        Some(Member::Inst(id)) => id,
        other => panic!("`{name}` is not a member: {other:?}"),
    }
}

#[test]
fn steps_narrow_the_rating_to_the_greatest_step_and_are_recorded() {
    let m = model(
        "board B {\n    .c: Supercap {}\n    .c.max_voltage &= 5.5V to 70degC, 4.2V to 85degC\n}\n",
    );
    let c = part(&m, "c");
    let steps = m.voltage_steps_of(c).expect("the steps are recorded");
    assert_eq!(steps.steps.steps, [(5.5, 343.15), (4.2, 358.15)]);
    assert_eq!(m.path_of_field(steps.field), "c.max_voltage");
    match &m.field(steps.field).domain {
        Domain::Quantity(q) => assert_eq!((q.lo(), q.hi()), (5.5, 5.5)),
        other => panic!("the rating is {other}"),
    }
}

#[test]
fn a_definition_states_its_steps_for_every_instance() {
    let src = "board B {\n    .c: Supercap { .max_voltage &= 2.7V to 65degC, 2.3V to 85degC }\n    .d: Supercap {}\n}\n";
    let m = model(src);
    assert!(m.voltage_steps_of(part(&m, "c")).is_some());
    assert!(m.voltage_steps_of(part(&m, "d")).is_none());
}

#[test]
fn only_a_parts_max_voltage_takes_steps() {
    let e = refused("board B {\n    .c: Supercap {}\n    .c.other &= 5.5V to 70degC\n}\n");
    assert!(
        e.contains("`.c.other` cannot be stated in steps over temperature: only a part's maximum voltage, its `.max_voltage`, is, as `.max_voltage &= 5.5V to 70degC, 4.2V to 85degC`"),
        "{e}"
    );
}

#[test]
fn steps_are_stated_with_a_narrowing_alone() {
    let e = refused("board B {\n    .c: Supercap {}\n    .c.max_voltage ?= 5.5V to 70degC\n}\n");
    assert!(
        e.contains("steps over temperature are stated with `&=`, not `?=`"),
        "{e}"
    );
    let e = refused("board B {\n    .c: Supercap {}\n    .c.max_voltage >= 5.5V to 70degC\n}\n");
    assert!(
        e.contains("steps over temperature are a part's maximum voltage, stated with `&=` on its own"),
        "{e}"
    );
}

#[test]
fn each_step_is_one_voltage_to_one_temperature_rising() {
    let e = refused("board B {\n    .c: Supercap {}\n    .c.max_voltage &= 5.5V to 70C\n}\n");
    assert!(
        e.contains("`70C` is a value in C (`C` alone is the coulomb: a temperature is written in `degC` or `K`), but here a step takes a temperature such as `85degC`"),
        "{e}"
    );
    let e = refused("board B {\n    .c: Supercap {}\n    .c.max_voltage &= 5V..6V to 70degC\n}\n");
    assert!(
        e.contains("a step is one number, as the datasheet prints it, not the range"),
        "{e}"
    );
    let e = refused(
        "board B {\n    .c: Supercap {}\n    .c.max_voltage &= 5.5V to 85degC, 4.2V to 70degC\n}\n",
    );
    assert!(
        e.contains("the temperatures of the steps rise from the first to the last, but `70degC` is not above `358.15 K (85degC)` before it"),
        "{e}"
    );
    let e = refused("board B {\n    .c: Supercap {}\n    .c.max_voltage &= .c.other to 70degC\n}\n");
    assert!(
        e.contains("a step is the number the datasheet prints, a voltage such as `4.2V`, not `.c.other`"),
        "{e}"
    );
}

#[test]
fn a_part_has_one_table_of_its_rating() {
    let e = refused(
        "board B {\n    .c: Supercap { .max_voltage &= 5.5V to 70degC }\n    .c.max_voltage &= 5.5V to 70degC\n}\n",
    );
    assert!(
        e.contains("is already stated in steps over temperature (at t.nsm:"),
        "{e}"
    );
}
