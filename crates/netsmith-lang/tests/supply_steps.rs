//! An input level in rows by the supply (`0.65 * .vcc.voltage at 1.1V..1.95V, 2.0V at
//! 3.0V..3.6V`, issues #1305 and #1307): what the parser reads after `&=`, and that it prints and
//! formats back.

use netsmith_lang::ast::*;
use netsmith_lang::{format, parse};

fn constraint(src: &str) -> Constraint {
    let file = parse(src).unwrap_or_else(|e| panic!("{}\n{src}", e.message));
    match file.definitions[0].items[0].clone() {
        Item::Constraint(c) => c,
        other => panic!("expected a constraint, got {other:?}"),
    }
}

#[test]
fn rows_after_a_narrowing_are_each_value_and_the_supply_it_holds_at() {
    let c = constraint(
        "part P {\n    .a.vih &= 0.65 * .vcc.voltage at 1.1V..1.95V, 2.0V at 3.0V..3.6V\n}\n",
    );
    assert!(c.at.is_none());
    let Expr::SupplySteps {
        rows, interpolated, ..
    } = &c.rhs
    else {
        panic!("not rows: {:?}", c.rhs);
    };
    let read: Vec<(String, String)> = rows
        .iter()
        .map(|r| (r.value.to_source(), r.at.to_source()))
        .collect();
    assert_eq!(
        read,
        [
            ("0.65 * .vcc.voltage".to_string(), "1.1V..1.95V".to_string()),
            ("2.0V".to_string(), "3.0V..3.6V".to_string())
        ]
    );
    assert!(!interpolated);
    assert_eq!(
        c.rhs.to_source(),
        "0.65 * .vcc.voltage at 1.1V..1.95V, 2.0V at 3.0V..3.6V"
    );
}

#[test]
fn interpolated_after_the_last_row_says_the_level_is_linear_between_rows() {
    let c = constraint("part P {\n    .a.vih &= 1.0V at 1.65V, 1.4V at 2.3V interpolated\n}\n");
    let Expr::SupplySteps {
        rows, interpolated, ..
    } = &c.rhs
    else {
        panic!("not rows: {:?}", c.rhs);
    };
    assert_eq!(rows.len(), 2);
    assert!(interpolated);
    assert_eq!(c.rhs.to_source(), "1.0V at 1.65V, 1.4V at 2.3V interpolated");
}

#[test]
fn one_row_alone_is_the_statements_at() {
    // The same text as a test current: the checker reads it by the level and the unit.
    let c = constraint("part P {\n    .a.vih &= 2.0V at 3.0V..3.6V\n}\n");
    assert!(matches!(c.rhs, Expr::Number(..)));
    assert_eq!(c.at.map(|a| a.to_source()).as_deref(), Some("3.0V..3.6V"));
    // `interpolated` after one row still makes it rows, for the checker to refuse.
    let c = constraint("part P {\n    .a.vih &= 2.0V at 3.0V interpolated\n}\n");
    assert!(matches!(c.rhs, Expr::SupplySteps { ref rows, interpolated: true, .. } if rows.len() == 1));
}

#[test]
fn a_row_after_a_comma_needs_its_at() {
    let src = "part P {\n    .a.vih &= 0.65 * .vcc.voltage at 1.1V..1.95V, 2.0V\n}\n";
    let e = parse(src).unwrap_err();
    assert!(
        e.message
            .contains("`at` and the supply the row holds at, such as `2.0V at 3.0V..3.6V`"),
        "{}",
        e.message
    );
}

#[test]
fn rows_format_to_themselves() {
    let src = "part P {\n    .a.vih &= 0.65 * .vcc.voltage at 1.1V..1.95V, 2.0V at 3.0V..3.6V interpolated\n}\n";
    assert_eq!(format(src).as_deref(), Ok(src));
    let loose = "part P {\n    .a.vih &= 0.65 * .vcc.voltage at 1.1V..1.95V ,2.0V  at 3.0V..3.6V  interpolated\n}\n";
    assert_eq!(format(loose).as_deref(), Ok(src));
}
