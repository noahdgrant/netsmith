//! A capacitor's `.bias` is the voltage of the rail it sits across (ADR 0013).

use netsmith_check::*;

const HEAD: &str = "import std::parts::passive::{Capacitor, Resistor}\nimport std::sources::Rail\n";

/// Every relation that gives a bias, as `field = rail voltage`.
fn biases(src: &str, board: &str) -> Vec<String> {
    let c = check("t.nsm", &format!("{HEAD}{src}"), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board(board).unwrap().model;
    m.relations()
        .iter()
        .map(|r| format!("{} = {}", m.show_expr(&r.lhs), m.show_expr(&r.rhs)))
        .filter(|s| s.contains(".bias") || s.starts_with("bias"))
        .collect()
}

#[test]
fn a_capacitor_in_a_series_chain_between_a_rails_pins_takes_its_voltage() {
    let b = biases(
        "board B {\n    .rail: Rail\n    .c: Capacitor\n    .rail.pos ~> .c ~> .rail.neg\n}\n",
        "B",
    );
    assert_eq!(b, ["c.bias = rail.voltage"]);
}

#[test]
fn the_direction_of_the_chain_does_not_matter() {
    let b = biases(
        "board B {\n    .rail: Rail\n    .c: Capacitor\n    .rail.neg ~> .c ~> .rail.pos\n}\n",
        "B",
    );
    assert_eq!(b, ["c.bias = rail.voltage"]);
}

#[test]
fn a_capacitor_whose_pins_are_on_a_rails_nets_by_name_takes_its_voltage() {
    let b = biases(
        "board B {\n    .rail: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }\n    .c: Capacitor {\n        .a.net &= \"VIN\"\n        .b.net &= \"GND\"\n    }\n}\n",
        "B",
    );
    assert_eq!(b, ["c.bias = rail.voltage"]);
}

#[test]
fn a_capacitor_joined_to_the_pins_with_plain_connections_takes_its_voltage() {
    let b = biases(
        "board B {\n    .rail: Rail\n    .c: Capacitor\n    .rail.pos ~ .c.a\n    .rail.neg ~ .c.b\n}\n",
        "B",
    );
    assert_eq!(b, ["c.bias = rail.voltage"]);
}

#[test]
fn a_capacitor_inside_a_module_takes_the_voltage_of_the_modules_port() {
    let b = biases(
        "module Stage {\n    .vin: Rail\n    .c: Capacitor\n    .vin.pos ~> .c ~> .vin.neg\n}\nboard B {\n    .supply: Rail\n    .s: Stage\n    .supply ~ .s.vin\n}\n",
        "B",
    );
    // The board's own rail is on the same nets, but it is out of the module's reach: the
    // voltage arrives at the port by the `~` that joins them.
    assert_eq!(b, ["s.c.bias = s.vin.voltage"]);
}

#[test]
fn a_capacitor_beside_a_part_takes_the_voltage_of_the_parts_rail() {
    // A rail of something the capacitor's own module holds is in reach.
    let b = biases(
        "part Chip {\n    .power: Rail\n}\nmodule Stage {\n    .chip: Chip\n    .c: Capacitor\n    .chip.power.pos ~> .c ~> .chip.power.neg\n}\nboard B {\n    .s: Stage\n}\n",
        "B",
    );
    assert_eq!(b, ["s.c.bias = s.chip.power.voltage"]);
}

#[test]
fn rails_on_the_same_nets_give_one_bias() {
    let b = biases(
        "board B {\n    .b: Rail\n    .a: Rail\n    .c: Capacitor\n    .a ~ .b\n    .a.pos ~> .c ~> .a.neg\n}\n",
        "B",
    );
    assert_eq!(b, ["c.bias = a.voltage"]);
}

#[test]
fn a_capacitor_across_no_rail_has_no_derived_bias() {
    for body in [
        // One end on a rail, the other on a resistor.
        "    .rail: Rail\n    .r: Resistor\n    .c: Capacitor\n    .rail.pos ~> .c ~> .r\n    .r.b ~ .rail.neg\n",
        // Both pins on the same net.
        "    .rail: Rail\n    .c: Capacitor\n    .rail.pos ~> .c ~> .rail.pos\n",
        // Not connected to anything.
        "    .rail: Rail\n    .c: Capacitor\n",
    ] {
        let b = biases(&format!("board B {{\n{body}}}\n"), "B");
        assert!(b.is_empty(), "{body}\n{b:?}");
    }
}

#[test]
fn a_rail_inside_another_module_is_out_of_reach() {
    // The capacitor is across the nets of a module's port. It sees the port, not the rail of
    // the part the module keeps inside.
    let b = biases(
        "part Chip {\n    .power: Rail\n}\nmodule Inner {\n    .out: Rail\n    .chip: Chip\n    .chip.power ~ .out\n}\nmodule Outer {\n    .inner: Inner\n    .c: Capacitor\n    .inner.out.pos ~> .c ~> .inner.out.neg\n}\nboard B {\n    .o: Outer\n}\n",
        "B",
    );
    assert_eq!(b, ["o.c.bias = o.inner.out.voltage"]);
}
