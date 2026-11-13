//! `fn GPIO class` in a pin row: the function is a class of the part, which the board may choose
//! on any number of pins, as it may a function that a `peripheral ... on` names. A function that
//! no row marks stays on one pin of a part.
//!
//! `Zmcu5` is invented for these tests; it is not a real part and is not in the standard library.

use netsmith_check::*;

/// Four pins that list `GPIO`, two of which also list an analog input; every row that lists
/// `GPIO` marks it.
const ZMCU5: &str = r#"
part Zmcu5 {
    pins {
        .p00 bidirectional "P0.00" "1" fn GPIO class
        .p01 bidirectional "P0.01" "2" fn GPIO class
        .p02 bidirectional "P0.02" "3" fn GPIO class AIN0:input
        .p03 bidirectional "P0.03" "4" fn GPIO class AIN1:input
    }
}

/// The same part with no marker.
part Zmcu5Plain {
    pins {
        .p00 bidirectional "P0.00" "1" fn GPIO
        .p01 bidirectional "P0.01" "2" fn GPIO
        .p02 bidirectional "P0.02" "3" fn GPIO AIN0:input
        .p03 bidirectional "P0.03" "4" fn GPIO AIN1:input
    }
}

/// Something with a pin for each of the part's, to put a net on.
part Probe {
    pins {
        .a00 passive "A00" "1"
        .a01 passive "A01" "2"
        .a02 passive "A02" "3"
        .a03 passive "A03" "4"
    }
}
"#;

fn run_part(part: &str, statements: &str) -> Checked {
    check(
        "main.nsm",
        &format!("{ZMCU5}\nboard B {{\n    .u: {part}\n    .probe: Probe\n{statements}\n}}\n"),
        &Loader::std(),
    )
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn only(c: &Checked, message: &str) {
    assert_eq!(messages(c), vec![message.to_string()], "{}", c.render());
}

/// A part on its own, for the mistakes in its table.
fn part_messages(body: &str) -> Vec<String> {
    messages(&check(
        "main.nsm",
        &format!("part P {{\n{body}\n}}\n"),
        &Loader::std(),
    ))
}

const GPIO_ON_FOUR: &str = "    .u.p00 ~ .probe.a00\n    .u.p01 ~ .probe.a01\n    .u.p02 ~ .probe.a02\n    .u.p03 ~ .probe.a03\n    .u.p00.function &= GPIO\n    .u.p01.function &= GPIO\n    .u.p02.function &= GPIO\n    .u.p03.function &= GPIO";

#[test]
fn a_marked_function_may_be_chosen_on_every_pin_that_lists_it() {
    let c = run_part("Zmcu5", GPIO_ON_FOUR);
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    for pin in ["u.p00", "u.p01", "u.p02", "u.p03"] {
        let id = m.inst_at(pin).unwrap();
        assert_eq!(m.function_of(id).as_deref(), Some("GPIO"), "{pin}");
    }
}

#[test]
fn a_function_that_is_not_marked_is_still_on_one_pin() {
    let c = run_part("Zmcu5Plain", GPIO_ON_FOUR);
    only(
        &c,
        "function `GPIO` is chosen on both `u.p00` and `u.p01` and `u.p02` and `u.p03`",
    );
}

#[test]
fn a_connected_pin_with_a_class_and_another_function_needs_one_chosen() {
    let c = run_part("Zmcu5", "    .u.p02 ~ .probe.a02");
    only(
        &c,
        "`u.p02` has several functions (GPIO, AIN0) and is connected: choose one with `.u.p02.function &= GPIO`",
    );
    // Either function is accepted, the other pins keep GPIO.
    let c = run_part(
        "Zmcu5",
        "    .u.p02 ~ .probe.a02\n    .u.p03 ~ .probe.a03\n    .u.p02.function &= AIN0\n    .u.p03.function &= GPIO",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_request_for_a_marked_function_and_a_pin_chosen_by_hand_agree() {
    // `p00` is chosen by hand as GPIO; the request goes round it and gets another pin that
    // offers GPIO, with no error from the class rule.
    let c = run_part(
        "Zmcu5",
        "    .u.p00 ~ .probe.a00\n    .u.p00.function &= GPIO\n    .status: .u.GPIO\n    .status ~ .probe.a01",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    let got: Vec<(String, String)> = m
        .pin_assignments
        .iter()
        .map(|a| (a.key.clone(), m.path_below(a.pin, a.part)))
        .collect();
    assert_eq!(got, vec![("status".to_string(), "p01".to_string())]);
    assert_eq!(
        m.function_of(m.inst_at("u.p01").unwrap()).as_deref(),
        Some("GPIO")
    );
}

#[test]
fn two_requests_for_a_marked_function_get_two_pins() {
    let c = run_part(
        "Zmcu5",
        "    .a: .u.GPIO\n    .b: .u.GPIO\n    .a ~ .probe.a00\n    .b ~ .probe.a01",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    let mut got: Vec<String> = m
        .pin_assignments
        .iter()
        .map(|a| m.path_below(a.pin, a.part))
        .collect();
    got.sort();
    assert_eq!(got, ["p00", "p01"]);
}

#[test]
fn a_peripheral_on_a_function_and_a_marker_mean_the_same() {
    // The same refusal-free reading, whichever way the part says that `GPIO` is a class.
    let by_peripheral = r#"
part Zmcu6 {
    peripheral UARTE0 on GPIO { UARTE0_TX:output UARTE0_RX:input }
    pins {
        .p00 bidirectional "P0.00" "1" fn GPIO
        .p01 bidirectional "P0.01" "2" fn GPIO
    }
}
part Probe2 {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
}
board B {
    .u: Zmcu6
    .probe: Probe2
    .u.p00 ~ .probe.a
    .u.p01 ~ .probe.b
    .u.p00.function &= GPIO
    .u.p01.function &= GPIO
}
"#;
    let c = check("main.nsm", by_peripheral, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_row_that_lists_a_marked_function_without_the_marker_is_refused() {
    let m = part_messages(
        "    pins {\n        .a bidirectional \"A\" \"1\" fn GPIO class\n        .b bidirectional \"B\" \"2\" fn GPIO\n    }",
    );
    assert_eq!(
        m,
        vec!["`.b` lists `GPIO` without `class`, which `.a` marks as a class; a class is a fact of the function, so write `fn GPIO class` on every row that lists it".to_string()]
    );
}

#[test]
fn a_marker_on_what_a_peripheral_is_on_is_refused() {
    let m = part_messages(
        "    peripheral UARTE0 on GPIO { UARTE0_TX:output }\n    pins {\n        .a bidirectional \"A\" \"1\" fn GPIO class\n    }",
    );
    assert_eq!(
        m,
        vec!["`.a` marks `GPIO` as a class, which `peripheral UARTE0 on GPIO` already makes it; remove `class` from the row".to_string()]
    );
}

#[test]
fn a_marker_on_a_line_of_a_peripheral_is_refused() {
    let m = part_messages(
        "    peripheral SPIM0 { SPIM0_SCK:output }\n    pins {\n        .a bidirectional \"A\" \"1\" fn SPIM0_SCK class\n    }",
    );
    assert_eq!(
        m,
        vec!["`.a` marks `SPIM0_SCK` as a class, but it is a line of `peripheral SPIM0`, which is on one pin at a time; a class is a function that no peripheral lists".to_string()]
    );
}
