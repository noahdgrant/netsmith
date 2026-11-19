//! An input level in rows by the supply (issues #1305 and #1307): the statement records the rows
//! and does not narrow the level, and every other use of rows is refused with a message that
//! names the level and shows the form.

use netsmith_check::*;

const PART: &str = "import std::logic::Logic\nimport std::sources::Rail\n\n/// A buffer.\npart Buf {\n    .designator &= \"U\"\n    .vcc: Rail\n    .a: Logic\n    .a.reference ~ .vcc\n    .a.reference.current &= 0A\n    .other: Voltage\n    pins {\n        .vcc.neg power \"GND\" \"1\"\n        .vcc.pos power \"VCC\" \"2\"\n        .a.line  input \"A\" \"3\"\n    }\n}\n";

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

fn board(statement: &str) -> String {
    format!(
        "board B {{\n    .p: Rail {{ .voltage &= 3.3V; .pos.net &= \"VCC\" }}\n    .u: Buf {{}}\n    .u.vcc ~ .p\n    .u.a.line ~ .p.pos\n    {statement}\n}}\n"
    )
}

#[test]
fn rows_are_recorded_and_leave_the_level_unset() {
    let m = model(&board(
        ".u.a.vih &= 0.65 * .u.vcc.voltage at 1.1V..1.95V, 2.0V at 3.0V..3.6V",
    ));
    let [steps] = m.supply_steps() else {
        panic!("one table: {:?}", m.supply_steps());
    };
    assert_eq!(m.path_of_field(steps.field), "u.a.vih");
    let ends: Vec<(f64, f64)> = steps.rows.iter().map(|r| (r.lo, r.hi)).collect();
    assert_eq!(ends, [(1.1, 1.95), (3.0, 3.6)]);
    assert!(!steps.interpolated);
    assert!(matches!(steps.rows[1].value, RExpr::Const(_)));
    assert!(matches!(m.field(steps.field).domain, Domain::Unset));
    assert!(m.supply_steps_of(steps.field).is_some());
}

#[test]
fn one_row_with_a_supply_is_a_table_of_one_row() {
    let m = model(&board(".u.a.vil &= 0.8V at 3.0V..3.6V"));
    let [steps] = m.supply_steps() else {
        panic!("one table: {:?}", m.supply_steps());
    };
    assert_eq!(m.path_of_field(steps.field), "u.a.vil");
    assert_eq!((steps.rows[0].lo, steps.rows[0].hi), (3.0, 3.6));
    // A point is a row too, and `interpolated` is kept.
    let m = model(&board(".u.a.vih &= 1.0V at 1.65V, 1.4V at 2.3V interpolated"));
    let steps = &m.supply_steps()[0];
    assert!(steps.interpolated);
    assert_eq!((steps.rows[0].lo, steps.rows[0].hi), (1.65, 1.65));
}

#[test]
fn only_an_input_level_takes_rows() {
    let e = refused(&board(".u.other &= 2V at 3V..3.6V, 2.2V at 4.5V..5.5V"));
    assert!(
        e.contains("`.other` cannot be stated in rows by the supply: only an input level, the `vih` or `vil` of a `Logic` or of `Levels`, is, as `.vih &= 0.65 * .vcc.voltage at 1.1V..1.95V, 2.0V at 3.0V..3.6V`"),
        "{e}"
    );
    let e = refused(&board(".u.a.voh &= 2.4V at 3V..3.6V, 3.8V at 4.5V..5.5V"));
    assert!(
        e.contains("`.a.voh` cannot be stated in rows by the supply: an output level is stated at its load (`at 2mA`)"),
        "{e}"
    );
}

#[test]
fn rows_are_stated_with_a_narrowing_alone() {
    let e = refused(&board(".u.a.vih ?= 2V at 3V..3.6V, 2.2V at 4.5V..5.5V"));
    assert!(
        e.contains("rows by the supply are stated with `&=`, not `?=`"),
        "{e}"
    );
    let e = refused(&board(".u.a.vih >= 2V at 3V..3.6V, 2.2V at 4.5V..5.5V"));
    assert!(
        e.contains("rows by the supply are an input level, stated with `&=` on its own"),
        "{e}"
    );
}

#[test]
fn each_row_is_a_voltage_at_a_supply_rising() {
    let e = refused(&board(".u.a.vih &= 2V at 3V..3.6V, 1.2V at 1.65V..1.95V"));
    assert!(
        e.contains("the supplies of the rows rise from the first to the last and do not overlap, but `1.65V..1.95V` starts below 3.6V where the row before it ends"),
        "{e}"
    );
    let e = refused(&board(".u.a.vih &= 2mA at 3V..3.6V, 2.2V at 4.5V..5.5V"));
    assert!(
        e.contains("but a row of `.a.vih` is a voltage, such as `2.0V` or `0.65 * .vcc.voltage`"),
        "{e}"
    );
    let e = refused(&board(".u.a.vih &= 2V at 3mA"));
    assert!(
        e.contains("an input level is not measured at a load: `at` after `vih` or `vil` takes the supply the level holds at"),
        "{e}"
    );
    let e = refused(&board(".u.a.vih &= 2V at .u.vcc.voltage, 2.2V at 4.5V..5.5V"));
    assert!(
        e.contains("the supply of a row is the number the datasheet prints, such as `3.0V..3.6V`, not `.u.vcc.voltage`"),
        "{e}"
    );
}

#[test]
fn interpolated_needs_two_rows() {
    let e = refused(&board(".u.a.vih &= 2V at 3V..3.6V interpolated"));
    assert!(
        e.contains("`interpolated` says that `.a.vih` is linear in the supply between two rows, and there is one row"),
        "{e}"
    );
}

#[test]
fn a_level_has_one_table() {
    let e = refused(&board(
        ".u.a.vih &= 2V at 3V..3.6V\n    .u.a.vih &= 2.2V at 4.5V..5.5V",
    ));
    assert!(
        e.contains("`.a.vih` is already stated in rows by the supply (at"),
        "{e}"
    );
}
