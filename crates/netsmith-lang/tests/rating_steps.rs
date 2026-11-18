//! A value in steps over temperature (`5.5V to 70degC, 4.2V to 85degC`, issue #1257): what the
//! parser reads after `&=` and in a catalog cell, and that it prints and formats back.

use netsmith_lang::ast::*;
use netsmith_lang::{format, parse, parse_expr};

fn rhs(src: &str) -> Expr {
    let file = parse(src).unwrap_or_else(|e| panic!("{}\n{src}", e.message));
    match file.definitions[0].items[0].clone() {
        Item::Constraint(c) => c.rhs,
        other => panic!("expected a constraint, got {other:?}"),
    }
}

#[test]
fn steps_after_a_narrowing_are_each_value_and_the_temperature_it_holds_to() {
    let e = rhs("part P {\n    .max_voltage &= 5.5V to 70degC, 4.2V to 85degC\n}\n");
    let Expr::Steps { steps, .. } = &e else {
        panic!("not steps: {e:?}");
    };
    let read: Vec<(String, String)> = steps
        .iter()
        .map(|s| (s.value.to_source(), s.to.to_source()))
        .collect();
    assert_eq!(
        read,
        [
            ("5.5V".to_string(), "70degC".to_string()),
            ("4.2V".to_string(), "85degC".to_string())
        ]
    );
    assert_eq!(e.to_source(), "5.5V to 70degC, 4.2V to 85degC");
    // One step is steps too, and a value without `to` is a value.
    assert!(matches!(
        parse_expr("5V to 85degC"),
        Ok(Expr::Steps { steps, .. }) if steps.len() == 1
    ));
    assert!(matches!(parse_expr("5V"), Ok(Expr::Number(..))));
}

#[test]
fn a_catalog_cell_reads_steps_as_a_source_does() {
    let e = parse_expr("5.5V to 70degC, 4.2V to 85degC").unwrap();
    assert_eq!(e.to_source(), "5.5V to 70degC, 4.2V to 85degC");
}

#[test]
fn a_step_after_a_comma_needs_its_to() {
    let src = "part P {\n    .max_voltage &= 5.5V to 70degC, 4.2V\n}\n";
    let e = parse(src).unwrap_err();
    assert!(
        e.message
            .contains("`to` and the temperature the step holds up to, such as `4.2V to 85degC`"),
        "{}",
        e.message
    );
}

#[test]
fn steps_format_to_themselves() {
    let src = "part P {\n    .max_voltage: Voltage\n    .max_voltage &= 5.5V to 70degC, 4.2V to 85degC\n}\n";
    assert_eq!(format(src).as_deref(), Ok(src));
    let loose = "part P {\n    .max_voltage: Voltage\n    .max_voltage &= 5.5V  to  70degC ,4.2V to 85degC\n}\n";
    assert_eq!(format(loose).as_deref(), Ok(src));
}
