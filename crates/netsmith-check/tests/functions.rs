//! A function with its own kind (ADR 0046): the net rules judge a pin by the kind of the
//! function the board chose, when the function has one.
//!
//! `Mcu` is invented for these tests: a part whose pins can be a GPIO, a UART transmitter or
//! a UART receiver. It is not a real part and is not in the standard library.

use netsmith_check::*;

const MCU: &str = r#"
part Mcu {
    pins {
        .p1 bidirectional "P1" "1" fn GPIO:bidirectional UART_TX:output UART_RX:input
        .p2 bidirectional "P2" "2" fn GPIO:bidirectional UART_TX:output UART_RX:input
        .q1 bidirectional "Q1" "3" fn GPIO UART_TX UART_RX
        .q2 bidirectional "Q2" "4" fn GPIO UART_TX UART_RX
    }
}
"#;

fn run(statements: &str) -> Checked {
    check(
        "test.nsm",
        &format!("{MCU}\nboard B {{\n    .a: Mcu\n    .b: Mcu\n{statements}\n}}\n"),
        &Loader::std(),
    )
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn only(c: &Checked, message: &str) {
    assert_eq!(messages(c), [message], "{}", c.render());
}

#[test]
fn two_pins_chosen_as_a_transmitter_on_one_net_are_two_outputs() {
    let c = run(
        "    .a.p1 ~ .b.p1\n    .a.p1.function &= \"UART_TX\"\n    .b.p1.function &= \"UART_TX\"",
    );
    only(
        &c,
        "net `net-a.p1` has two outputs: `a.p1` (`Mcu` function `UART_TX`, an output), `b.p1` (`Mcu` function `UART_TX`, an output)",
    );
}

#[test]
fn two_pins_chosen_as_a_receiver_on_one_net_have_nothing_to_drive_them() {
    let c = run(
        "    .a.p1 ~ .b.p1\n    .a.p1.function &= \"UART_RX\"\n    .b.p1.function &= \"UART_RX\"",
    );
    only(
        &c,
        "net `net-a.p1` has only inputs (`a.p1` (`Mcu` function `UART_RX`, an input), `b.p1` (`Mcu` function `UART_RX`, an input)): nothing drives it, so it floats; join an output, a bidirectional pin or a rail to it, or a pull resistor, or choose another function for a pin whose function makes it an input",
    );
}

#[test]
fn a_transmitter_receiver_pair_and_a_gpio_on_either_side_agree_with_the_net() {
    // TX to RX, and the pin's own kind (bidirectional) for GPIO, drive or are driven.
    let c = run(
        "    .a.p1 ~ .b.p1\n    .a.p1.function &= \"UART_TX\"\n    .b.p1.function &= \"UART_RX\"\n    .a.p2 ~ .b.p2\n    .a.p2.function &= \"UART_RX\"\n    .b.p2.function &= \"GPIO\"",
    );
    assert!(c.is_ok(), "{}", c.render());
    let c =
        run("    .a.p1 ~ .b.p1\n    .a.p1.function &= \"GPIO\"\n    .b.p1.function &= \"GPIO\"");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_function_written_without_a_kind_has_the_pins_kind() {
    // The same wiring on pins whose functions have no kind of their own: both are
    // bidirectional, so nothing is wrong, as before ADR 0046.
    let c = run(
        "    .a.q1 ~ .b.q1\n    .a.q1.function &= \"UART_TX\"\n    .b.q1.function &= \"UART_TX\"\n    .a.q2 ~ .b.q2\n    .a.q2.function &= \"UART_RX\"\n    .b.q2.function &= \"UART_RX\"",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn the_function_decides_only_for_the_function_chosen() {
    // Another function of the same pin does not carry the kind of the first.
    let c =
        run("    .a.p1 ~ .b.p1\n    .a.p1.function &= \"UART_TX\"\n    .b.p1.function &= \"GPIO\"");
    assert!(c.is_ok(), "{}", c.render());
}
