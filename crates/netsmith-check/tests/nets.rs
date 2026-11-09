//! Rules of a whole net: inputs with nothing to drive them, and an I2C bus with no pull-up.
//!
//! The parts are invented for these tests.

use netsmith_check::*;

const PARTS: &str = r#"
import std::bus::{I2c, I2cPullups, Uart}
import std::parts::passive::Resistor
import std::sources::Rail

part Uart32 {
    .vdd: Rail
    .uart: Uart

    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "VSS" "2"
        .uart.tx   output "TX"  "3"
        .uart.rx   input  "RX"  "4"
    }
}

/// An IC with an enable input and one bidirectional pin.
part Switchable {
    pins {
        .en     input         "EN"  "1"
        .io     bidirectional "IO"  "2"
        .sense  input         "SNS" "3" optional
    }
}

part Sensor {
    .vdd: Rail
    .i2c: I2c

    pins {
        .vdd.pos   power         "VDD" "1"
        .vdd.neg   power         "GND" "2"
        .i2c.sda   bidirectional "SDA" "3"
        .i2c.scl   bidirectional "SCL" "4"
    }
}

module Wrapped {
    .rx: Pin
    .chip: Switchable
    .rx ~ .chip.en
}
"#;

fn run(body: &str) -> Checked {
    check("test.nsm", &format!("{PARTS}\n{body}"), &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn only(c: &Checked, message: &str) {
    assert_eq!(messages(c), [message], "{}", c.render());
}

const SUPPLY: &str = "    .supply: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }\n";

#[test]
fn a_net_of_only_inputs_has_nothing_to_drive_it() {
    let c = run(&format!(
        "board B {{\n{SUPPLY}    .a: Uart32\n    .b: Uart32\n    .supply ~ .a.vdd\n    .supply ~ .b.vdd\n    .a.uart.tx ~ .b.uart.rx\n    .b.uart.tx ~ .a.uart.rx\n    .c: Switchable\n    .d: Switchable\n    .c.en ~ .d.en\n}}\n"
    ));
    only(
        &c,
        "net `net-c.en` has only inputs (`c.en`, `d.en`): nothing drives it, so it floats; join an output, a bidirectional pin or a rail to it, or a pull resistor",
    );
}

#[test]
fn uart_receive_lines_joined_together_are_found_with_the_net_and_the_pins() {
    // `rx` to `rx` and `tx` to `tx`: since ADR 0064 the line rule says so, once for each net,
    // and the rules of a net (only inputs, two outputs) leave those nets to it.
    let c = run(&format!(
        "board B {{\n{SUPPLY}    .a: Uart32\n    .b: Uart32\n    .supply ~ .a.vdd\n    .supply ~ .b.vdd\n    .a.uart.rx ~ .b.uart.rx\n    .a.uart.tx ~ .b.uart.tx\n}}\n"
    ));
    let m = messages(&c);
    assert_eq!(m.len(), 2, "{m:?}");
    assert!(
        m.contains(&"net `net-a.uart.rx` joins `rx` to `rx` (`a.uart.rx`, `b.uart.rx`): `rx` crosses `tx` in a `Uart`, so it is joined to the other end's `tx`; write `.a.uart.rx ~ .b.uart.tx`, or join the two interfaces with `.a.uart ~ .b.uart`, which crosses `tx` to `rx` and `rts` to `cts`".to_string()),
        "{m:?}"
    );
    assert!(
        m.iter()
            .all(|m| !m.contains("only inputs") && !m.contains("two outputs")),
        "{m:?}"
    );
}

#[test]
fn an_output_a_bidirectional_pin_a_rail_or_a_resistor_drives_the_net() {
    // A bidirectional pin, a rail's pos, and a pull-up resistor each drive one.
    let c = run(&format!(
        "board B {{\n{SUPPLY}    .c: Switchable\n    .d: Switchable\n    .e: Switchable\n    .r: Resistor\n    .c.en ~ .d.io\n    .d.en ~ .supply.pos\n    .supply.pos ~> .r ~> .e.en\n    .c.sense ~ .e.sense\n    .e.io ~ .c.sense\n}}\n"
    ));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_port_the_board_brings_in_from_outside_drives_the_net() {
    // A module's port that the board leaves open has nothing on it.
    let c = run("board B {\n    .w: Wrapped\n    .x: Switchable\n    .w.rx ~ .x.en\n}\n");
    only(
        &c,
        "net `net-w.chip.en` has only inputs (`w.chip.en`, `x.en`): nothing drives it, so it floats; join an output, a bidirectional pin or a rail to it, or a pull resistor",
    );
}

#[test]
fn a_lone_input_is_the_business_of_the_must_be_connected_rule() {
    let c = run("board B {\n    .c: Switchable\n    .d: Switchable\n    .c.io ~ .d.io\n}\n");
    let m = messages(&c);
    assert_eq!(m.len(), 2, "{m:?}");
    assert!(
        m.iter()
            .all(|m| m.contains("is an input that must be connected"))
    );
}

// ---- I2C pull-ups ------------------------------------------------------------------

fn i2c_board(extra: &str) -> Checked {
    run(&format!(
        "board B {{\n{SUPPLY}    .a: Sensor\n    .b: Sensor\n    .supply ~ .a.vdd\n    .supply ~ .b.vdd\n    .a.i2c ~ .b.i2c\n{extra}}}\n"
    ))
}

/// `i2c_board` with `imports` before the board.
fn i2c_board_importing(imports: &str, extra: &str) -> Checked {
    run(&format!(
        "{imports}\nboard B {{\n{SUPPLY}    .a: Sensor\n    .b: Sensor\n    .supply ~ .a.vdd\n    .supply ~ .b.vdd\n    .a.i2c ~ .b.i2c\n{extra}}}\n"
    ))
}

const SDA: &str = "net `net-a.i2c.sda` carries the sda line of an I2C bus (`a.i2c.sda`, `b.i2c.sda`) and has no pull-up: no resistor joins it to the positive pin of a rail; add `std::bus::I2cPullups` to the bus, or a resistor from a rail's `pos` to the line";
const SCL: &str = "net `net-a.i2c.scl` carries the scl line of an I2C bus (`a.i2c.scl`, `b.i2c.scl`) and has no pull-up: no resistor joins it to the positive pin of a rail; add `std::bus::I2cPullups` to the bus, or a resistor from a rail's `pos` to the line";

#[test]
fn an_i2c_bus_with_no_pullups_is_refused_for_each_line() {
    let c = i2c_board("");
    assert_eq!(messages(&c), [SCL, SDA], "{}", c.render());
}

#[test]
fn the_i2c_pullups_module_satisfies_the_bus() {
    let c = i2c_board(
        "    .pullups: I2cPullups { .resistance &= 4.7kohm +/- 5% }\n    .supply ~ .pullups.supply\n    .a.i2c ~ .pullups.bus\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn one_pullup_resistor_on_a_line_is_enough_for_that_line() {
    let c = i2c_board(
        "    .r: Resistor { .resistance &= 4.7kohm +/- 5% }\n    .supply.pos ~> .r ~> .a.i2c.sda\n",
    );
    assert_eq!(messages(&c), [SCL], "{}", c.render());
}

#[test]
fn a_chain_of_resistors_to_the_rail_is_a_pullup_too() {
    let c = i2c_board(
        "    .r1: Resistor { .resistance &= 1kohm +/- 5% }\n    .r2: Resistor { .resistance &= 3.3kohm +/- 5% }\n    .r3: Resistor { .resistance &= 4.7kohm +/- 5% }\n    .supply.pos ~> .r1 ~> .r2 ~> .a.i2c.sda\n    .supply.pos ~> .r3 ~> .a.i2c.scl\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_pulldown_or_a_line_tied_to_the_rail_is_not_a_pullup() {
    // The resistor goes to the negative pin; the other line is wired to the rail itself.
    let c = i2c_board(
        "    .r: Resistor { .resistance &= 4.7kohm +/- 5% }\n    .a.i2c.sda ~> .r ~> .supply.neg\n    .a.i2c.scl ~ .supply.pos\n",
    );
    let m = messages(&c);
    assert_eq!(m.len(), 2, "{}", c.render());
    assert!(
        m[0].starts_with("net `3V3` carries the scl line"),
        "{}",
        m[0]
    );
    assert_eq!(m[1], SDA);
}

#[test]
fn a_bus_that_leaves_through_a_connector_is_pulled_up_on_the_other_side() {
    let c = run(&format!(
        "import std::parts::connector::Header1x4\nboard B {{\n{SUPPLY}    .a: Sensor\n    .supply ~ .a.vdd\n    .j: Header1x4\n    .a.i2c.sda ~ .j.p1\n    .a.i2c.scl ~ .j.p2\n}}\n"
    ));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_test_point_on_the_line_does_not_hide_a_missing_pullup() {
    let c = run(&format!(
        "import std::parts::mechanical::TestPoint\nboard B {{\n{SUPPLY}    .a: Sensor\n    .b: Sensor\n    .supply ~ .a.vdd\n    .supply ~ .b.vdd\n    .a.i2c ~ .b.i2c\n    .tp: TestPoint\n    .tp.p1 ~ .a.i2c.sda\n}}\n"
    ));
    assert_eq!(messages(&c), [SCL, SDA], "{}", c.render());
}

#[test]
fn a_bus_port_of_the_board_is_not_judged() {
    let c = run(&format!(
        "board B {{\n{SUPPLY}    .bus: I2c\n    .a: Sensor\n    .supply ~ .a.vdd\n    .a.i2c ~ .bus\n}}\n"
    ));
    assert!(c.is_ok(), "{}", c.render());
}

// ---- what counts as a pull-up (ADR 0044, ADR 0090) -----------------------------------

#[test]
fn a_pullup_module_a_parallel_pair_and_a_divider_are_pullups_too() {
    // `PullUp` is a resistor from `supply` to `line`; two resistors in parallel are two paths to
    // the rail; and a resistor to a node of a divider (a second resistor to ground on it) is a
    // path through a network, found hop by hop.
    let c = i2c_board_importing(
        "import std::bus::PullUp",
        "    .pu: PullUp { .resistance &= 4.7kohm +/- 5% }\n    .supply ~ .pu.supply\n    .a.i2c.sda ~ .pu.line\n    \
         .r1: Resistor { .resistance &= 10kohm +/- 5% }\n    .r2: Resistor { .resistance &= 10kohm +/- 5% }\n    \
         .supply.pos ~> .r1 ~> .a.i2c.scl\n    .supply.pos ~> .r2 ~> .a.i2c.scl\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let c = i2c_board(
        "    .r1: Resistor { .resistance &= 10kohm +/- 5% }\n    .r2: Resistor { .resistance &= 4.7kohm +/- 5% }\n    \
         .r3: Resistor { .resistance &= 4.7kohm +/- 5% }\n    .r4: Resistor { .resistance &= 100kohm +/- 5% }\n    \
         .supply.pos ~> .r1 ~> .r2 ~> .a.i2c.sda\n    .r1.b ~> .r4 ~> .supply.neg\n    .supply.pos ~> .r3 ~> .a.i2c.scl\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_resistor_to_a_net_that_is_not_the_rail_is_not_a_pullup() {
    // The far end of the resistor is on a net with no rail on it, and that net has nothing
    // else that holds it up.
    let c = i2c_board_importing(
        "import std::parts::mechanical::TestPoint",
        "    .r1: Resistor { .resistance &= 4.7kohm +/- 5% }\n    .r2: Resistor { .resistance &= 4.7kohm +/- 5% }\n    \
         .tp: TestPoint\n    .tp.p1 ~> .r1 ~> .a.i2c.sda\n    .tp.p1 ~> .r2 ~> .a.i2c.scl\n",
    );
    assert_eq!(messages(&c), [SCL, SDA], "{}", c.render());
}

#[test]
fn an_input_with_a_pull_resistor_a_pullup_module_or_a_connector_pin_is_not_undriven() {
    // The undriven-input rule treats every passive pin as a driver, so a pull-up built of a
    // resistor, of `PullUp`, or a connector pin that leaves the board are all accepted.
    for extra in [
        "    .r: Resistor { .resistance &= 10kohm +/- 5% }\n    .supply.pos ~> .r ~> .c.en\n    .c.en ~ .d.en\n",
        "    .pu: PullUp { .resistance &= 10kohm +/- 5% }\n    .supply ~ .pu.supply\n    .pu.line ~ .c.en\n    .c.en ~ .d.en\n",
        "    .j: Header1x2\n    .j.p1 ~ .c.en\n    .c.en ~ .d.en\n",
    ] {
        let c = run(&format!(
            "import std::bus::PullUp\nimport std::parts::connector::Header1x2\nboard B {{\n{SUPPLY}    .c: Switchable\n    .d: Switchable\n{extra}}}\n"
        ));
        assert!(c.is_ok(), "{extra}\n{}", c.render());
    }
}

// ---- an I2C bus that crosses a mate (ADR 0090) ----------------------------------------

/// A carrier and a card that a system mates, each with a sensor on the bus. Contact 1 is ground,
/// 2 power, 3 `sda` and 4 `scl`. `carrier` and `card` are added to the boards.
fn mated(carrier: &str, card: &str) -> Checked {
    run(&format!(
        "import std::parts::connector::{{Header1x4, Socket1x4}}\n\
         board Carrier {{\n{SUPPLY}    .a: Sensor\n    .supply ~ .a.vdd\n    .j: Socket1x4\n    \
         .supply.neg ~ .j.p1\n    .supply.pos ~ .j.p2\n    .a.i2c.sda ~ .j.p3\n    .a.i2c.scl ~ .j.p4\n{carrier}}}\n\
         board Card {{\n{SUPPLY}    .b: Sensor\n    .supply ~ .b.vdd\n    .h: Header1x4\n    \
         .supply.neg ~ .h.p1\n    .supply.pos ~ .h.p2\n    .b.i2c.sda ~ .h.p3\n    .b.i2c.scl ~ .h.p4\n{card}}}\n\
         system S {{\n    .carrier: Carrier\n    .card: Card\n    .carrier.j.mating ~ .card.h.mating\n}}\n"
    ))
}

const PULL_PAIR: &str = "    .pullups: I2cPullups { .resistance &= 4.7kohm +/- 5% }\n    .supply ~ .pullups.supply\n    .a.i2c ~ .pullups.bus\n";

#[test]
fn a_bus_through_a_mate_with_no_pullup_on_either_board_is_refused_once_per_line() {
    let c = mated("", "");
    let m = messages(&c);
    assert_eq!(m.len(), 2, "{}", c.render());
    for (message, line) in m.iter().zip(["scl", "sda"]) {
        assert!(
            message.contains(&format!("carries the {line} line of an I2C bus"))
                && message.contains("`carrier.a.i2c.")
                && message.contains("`card.b.i2c.")
                && message.contains("on this board or on the boards its mated connectors lead to")
                && message.contains("`carrier.j`")
                && message.contains("`card.h`"),
            "{message}"
        );
    }
}

#[test]
fn a_pullup_on_either_side_of_a_mate_satisfies_the_bus() {
    // On the carrier, as `I2cPullups`.
    let on_carrier = mated(PULL_PAIR, "");
    assert!(on_carrier.is_ok(), "{}", on_carrier.render());
    // On the card, as resistors to its rail.
    let on_card = mated(
        "",
        "    .r1: Resistor { .resistance &= 4.7kohm +/- 5% }\n    .r2: Resistor { .resistance &= 4.7kohm +/- 5% }\n    \
         .supply.pos ~> .r1 ~> .b.i2c.sda\n    .supply.pos ~> .r2 ~> .b.i2c.scl\n",
    );
    assert!(on_card.is_ok(), "{}", on_card.render());
    // One line pulled up on each side: each line has its own.
    let split = mated(
        "    .r1: Resistor { .resistance &= 4.7kohm +/- 5% }\n    .supply.pos ~> .r1 ~> .a.i2c.sda\n",
        "    .r2: Resistor { .resistance &= 4.7kohm +/- 5% }\n    .supply.pos ~> .r2 ~> .b.i2c.scl\n",
    );
    assert!(split.is_ok(), "{}", split.render());
}

#[test]
fn a_line_that_also_leaves_by_a_connector_that_is_not_mated_is_not_judged() {
    // A second connector on the carrier's line is not mated in the system: the line goes
    // somewhere the system does not show, and may be pulled up there.
    let c = mated(
        "    .j2: Header1x4\n    .a.i2c.sda ~ .j2.p3\n    .a.i2c.scl ~ .j2.p4\n    .supply.neg ~ .j2.p1\n    .supply.pos ~ .j2.p2\n",
        "",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_board_alone_is_not_judged_behind_its_connector() {
    // Checked as itself, with no system around it, the carrier cannot see the other side.
    let c = run(&format!(
        "import std::parts::connector::Socket1x4\nboard Carrier {{\n{SUPPLY}    .a: Sensor\n    .supply ~ .a.vdd\n    .j: Socket1x4\n    \
         .supply.neg ~ .j.p1\n    .supply.pos ~ .j.p2\n    .a.i2c.sda ~ .j.p3\n    .a.i2c.scl ~ .j.p4\n}}\n"
    ));
    assert!(c.is_ok(), "{}", c.render());
}
