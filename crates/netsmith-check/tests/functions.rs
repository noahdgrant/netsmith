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

// ---- A module passes a function choice on (ADR 0071, stage 0) ----

/// An invented part with two pins that can each be a GPIO, a bus clock or a transmitter.
const ZMCU: &str = r#"
part Zmcu {
    pins {
        .p04 bidirectional "P04" "1" fn GPIO:bidirectional BUS_SCL:bidirectional UART_TX:output
        .p05 bidirectional "P05" "2" fn GPIO:bidirectional BUS_SDA:bidirectional UART_TX:output
    }
}

/// A module that exports a pin and passes the board's choice on to the part.
module Hub {
    .scl: Pin
    .mcu: Zmcu
    .scl ~ .mcu.p04
    .mcu.p04.function == .scl.function
}

/// The same, without the equation: the choice of `.scl` goes nowhere.
module Bare {
    .scl: Pin
    .mcu: Zmcu
    .scl ~ .mcu.p04
}

/// A pin joined to two pins of the part, with no equation.
module Fork {
    .scl: Pin
    .mcu: Zmcu
    .scl ~ .mcu.p04
    .scl ~ .mcu.p05
}

/// A module in a module: it forwards `.scl` to the pin of the `Hub` inside it.
module Outer {
    .scl: Pin
    .hub: Hub
    .scl ~ .hub.scl
    .hub.scl.function == .scl.function
}

/// The same, with no equation at this level.
module Loose {
    .scl: Pin
    .hub: Hub
    .scl ~ .hub.scl
}
"#;

fn zmcu(body: &str) -> Checked {
    check(
        "test.nsm",
        &format!("{ZMCU}\nboard B {{\n{body}\n}}\n"),
        &Loader::std(),
    )
}

/// The text of the source that a diagnostic points at.
fn pointed_at(c: &Checked, i: usize) -> String {
    let d = &c.diagnostics[i];
    let text = &c.sources.get(d.source).text;
    text[d.span.start..d.span.end].to_string()
}

#[test]
fn a_function_a_module_forwards_is_read_by_the_part_and_its_net_rules() {
    let c = zmcu("    .hub: Hub\n    .hub.scl.function &= \"BUS_SCL\"");
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    let pin = model.inst_at("hub.mcu.p04").unwrap();
    assert_eq!(model.function_of(pin).as_deref(), Some("BUS_SCL"));

    // The kind of the forwarded function counts on the net: two transmitters on one net.
    let c = zmcu(
        "    .a: Hub\n    .b: Hub\n    .a.scl ~ .b.scl\n    .a.scl.function &= \"UART_TX\"\n    .b.scl.function &= \"UART_TX\"",
    );
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.contains("has two outputs")
            && m.contains("`a.mcu.p04` (`Zmcu` function `UART_TX`, an output)")
            && m.contains("`b.mcu.p04` (`Zmcu` function `UART_TX`, an output)")),
        "{m:#?}"
    );
}

#[test]
fn a_function_stated_on_a_module_pin_that_nothing_reads_is_an_error_at_the_boards_line() {
    let c = zmcu("    .bare: Bare\n    .bare.scl.function &= \"BUS_SCL\"");
    let m = messages(&c);
    // The part's own pin still has several functions to choose from, as it did, and the
    // statement of the board says why its choice does not reach the part.
    assert_eq!(
        m,
        [
            "`bare.mcu.p04` has several functions (GPIO, BUS_SCL, UART_TX) and is connected: choose one with `.mcu.p04.function &= \"GPIO\"`",
            "`bare.scl.function` is stated, but `bare.scl` is a pin of module `Bare` and nothing passes it to a pin of a part, so it has no effect; inside `Bare`, write `.mcu.p04.function == .scl.function`",
        ],
        "{}",
        c.render()
    );
    assert_eq!(pointed_at(&c, 1), ".bare.scl.function &= \"BUS_SCL\"");

    // More than one pin of the part on the net: the hint leaves the place to fill.
    let c = zmcu("    .fork: Fork\n    .fork.scl.function &= \"BUS_SCL\"");
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.ends_with("inside `Fork`, write `.<part>.<pin>.function == .scl.function`")),
        "{m:#?}"
    );
}

#[test]
fn a_function_forwarded_through_two_modules_reaches_the_part() {
    let c = zmcu("    .outer: Outer\n    .outer.scl.function &= \"BUS_SCL\"");
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    let pin = model.inst_at("outer.hub.mcu.p04").unwrap();
    assert_eq!(model.function_of(pin).as_deref(), Some("BUS_SCL"));

    // A break in the chain is found at the outer pin the board wrote, and names the part's pin
    // two modules down.
    let c = zmcu("    .loose: Loose\n    .loose.scl.function &= \"BUS_SCL\"");
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.starts_with(
            "`loose.scl.function` is stated, but `loose.scl` is a pin of module `Loose` and nothing passes it to a pin of a part"
        ) && m.ends_with("write `.hub.mcu.p04.function == .scl.function`")),
        "{m:#?}"
    );
}

#[test]
fn a_wrong_forwarded_function_points_at_the_boards_line() {
    let c = zmcu("    .hub: Hub\n    .hub.scl.function &= \"NOPE\"");
    assert_eq!(
        messages(&c),
        ["`hub.mcu.p04` cannot be `NOPE`; its functions are GPIO, BUS_SCL, UART_TX"],
        "{}",
        c.render()
    );
    assert_eq!(pointed_at(&c, 0), ".hub.scl.function &= \"NOPE\"");
}

#[test]
fn the_hint_for_a_forwarded_pin_names_the_modules_pin_the_board_can_write() {
    // Connected inside the module and not chosen: the part's pin has three functions.
    let c = zmcu("    .hub: Hub");
    assert_eq!(
        messages(&c),
        [
            "`hub.mcu.p04` has several functions (GPIO, BUS_SCL, UART_TX) and is connected: choose one with `.hub.scl.function &= \"GPIO\"`"
        ],
        "{}",
        c.render()
    );
    assert_eq!(pointed_at(&c, 0), ".hub: Hub");
    // Two modules deep, it is the outer pin.
    let c = zmcu("    .outer: Outer");
    assert_eq!(
        messages(&c),
        [
            "`outer.hub.mcu.p04` has several functions (GPIO, BUS_SCL, UART_TX) and is connected: choose one with `.outer.scl.function &= \"GPIO\"`"
        ],
        "{}",
        c.render()
    );
    // Not forwarded, the hint is the part's own pin, as the module's author writes it, as before.
    let c = zmcu("    .bare: Bare");
    assert_eq!(
        messages(&c),
        [
            "`bare.mcu.p04` has several functions (GPIO, BUS_SCL, UART_TX) and is connected: choose one with `.mcu.p04.function &= \"GPIO\"`"
        ],
        "{}",
        c.render()
    );
}

#[test]
fn a_function_stated_on_an_alias_that_the_table_has_no_row_for_is_an_error_too() {
    let src = r#"
part Dual {
    .q: Pin
    .q ~ .p1
    pins {
        .p1 bidirectional "P1" "1" fn GPIO UART_TX
    }
}

board B {
    .d: Dual
    .d.q.function &= "GPIO"
}
"#;
    let c = check("test.nsm", src, &Loader::std());
    assert_eq!(
        messages(&c),
        [
            "`d.q.function` is stated, but `d.q` has no row in the pin table of `Dual`, so it has no effect; the function of a pin is chosen on a pin of the table"
        ],
        "{}",
        c.render()
    );
    assert_eq!(pointed_at(&c, 0), ".d.q.function &= \"GPIO\"");
}
