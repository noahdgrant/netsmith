//! A module's own rail and the rail of a part inside it that `~` joins are one current
//! (ADR 0077). Every part and module here is invented.

use netsmith_check::*;

const PARTS: &str = "\
import std::parts::passive::{Capacitor, Resistor}
import std::sources::Rail
import std::logic::Logic

part Sensor {
    .designator &= \"U\"
    .vs: Rail
    .vs.current &= 0A..1.1mA
}

part Flash {
    .designator &= \"U\"
    .vcc: Rail
    .vcc.current &= 0A..10mA
}

part TwoSupplies {
    .designator &= \"U\"
    .core: Rail
    .io: Rail
}
";

const BOARD: &str = "
board B {
    .supply: Rail {
        .voltage &= 3.3V +/- 3%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }
    .m: M
    .supply ~ .m.vdd
}
";

/// The model of a board `B` that has one module `M` with the given body.
fn with_module(body: &str) -> Model {
    let src = format!("{PARTS}\nmodule M {{\n    .vdd: Rail\n{body}}}\n{BOARD}");
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    c.board("B").expect("the board is there").model.clone()
}

/// The signed terms of the balance of module `m` on its supply net, as `path` or `-path`.
fn terms(m: &Model) -> Vec<String> {
    let b = m
        .balances()
        .iter()
        .find(|b| m.path_of(b.scope) == "m" && b.net == "3V3")
        .expect("the module has a balance on the supply net");
    b.draws
        .iter()
        .map(|d| {
            let sign = if d.flipped { "-" } else { "" };
            format!("{sign}{}", m.path_of_field(d.field))
        })
        .collect()
}

const ALIAS: [&str; 2] = ["m.sensor.vs.current", "-m.vdd.current"];

/// A model of board `B` with the given modules and module `M` as the board's `.m`.
fn with_source(modules: &str) -> Model {
    let src = format!("{PARTS}\n{modules}\n{BOARD}");
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    c.board("B").expect("the board is there").model.clone()
}

/// The balance of `scope` on the supply net keeps these terms besides the two of an alias.
fn keeps(m: &Model, extra: &[&str]) {
    let all = terms(m);
    for e in extra {
        assert!(all.iter().any(|t| t == e), "`{e}` is not in {all:?}");
    }
}

#[test]
fn a_module_rail_joined_to_the_rail_of_its_part_has_the_part_s_current() {
    let m = with_module(
        "    .sensor: Sensor\n    .bypass: Capacitor\n    .vdd ~ .sensor.vs\n    .vdd.pos ~> .bypass ~> .vdd.neg\n",
    );
    assert_eq!(terms(&m), ALIAS);
}

#[test]
fn the_side_the_statement_names_first_does_not_matter() {
    let m = with_module("    .sensor: Sensor\n    .sensor.vs ~ .vdd\n");
    assert_eq!(terms(&m), ALIAS);
}

#[test]
fn the_reference_rail_of_a_logic_line_carries_nothing_and_is_not_a_term() {
    // The module's own line and the part's reference rails are on the net with no current
    // stated: without the alias the balance would have three unknown currents.
    let m = with_module(
        "    .sensor: Sensor\n    .alert: Logic\n    .vdd ~ .sensor.vs\n    .alert.reference ~ .vdd\n",
    );
    assert_eq!(terms(&m), ALIAS);
}

#[test]
fn a_module_rail_that_two_parts_are_joined_to_is_the_current_of_neither() {
    let m = with_module(
        "    .sensor: Sensor\n    .flash: Flash\n    .vdd ~ .sensor.vs\n    .vdd ~ .flash.vcc\n",
    );
    keeps(
        &m,
        &[
            "m.sensor.vs.current",
            "m.flash.vcc.current",
            "-m.vdd.current",
        ],
    );
}

#[test]
fn two_rails_of_one_part_on_the_module_rail_are_not_each_its_current() {
    let m = with_module("    .chip: TwoSupplies\n    .vdd ~ .chip.core\n    .vdd ~ .chip.io\n");
    keeps(
        &m,
        &["m.chip.core.current", "m.chip.io.current", "-m.vdd.current"],
    );
}

#[test]
fn a_chain_that_starts_on_the_net_is_a_draw_the_part_does_not_have() {
    let m = with_module(
        "    .sensor: Sensor\n    .pull: Resistor\n    .alert: Logic\n    .vdd ~ .sensor.vs\n    .alert.reference ~ .vdd\n    .vdd.pos ~> .pull ~> .vdd.neg\n",
    );
    keeps(&m, &["m.pull.current", "-m.alert.reference.current"]);
}

#[test]
fn a_second_part_on_the_net_is_a_draw_the_module_rail_shares() {
    // The flash is joined to the sensor's rail, not to the module's: the module's rail carries
    // both, and the sensor's current is only a part of it.
    let m = with_module(
        "    .sensor: Sensor\n    .flash: Flash\n    .alert: Logic\n    .vdd ~ .sensor.vs\n    .sensor.vs ~ .flash.vcc\n    .alert.reference ~ .vdd\n",
    );
    keeps(&m, &["m.flash.vcc.current", "-m.alert.reference.current"]);
}

#[test]
fn a_rail_the_module_hands_on_is_drawn_from_and_stays_a_term() {
    let m = with_source(
        "module M {\n    .vdd: Rail\n    .vdd_out: Rail\n    .sensor: Sensor\n    .vdd ~ .sensor.vs\n    .vdd ~ .vdd_out\n}",
    );
    keeps(
        &m,
        &[
            "m.sensor.vs.current",
            "-m.vdd.current",
            "-m.vdd_out.current",
        ],
    );
}

#[test]
fn a_module_rail_joined_to_the_port_of_an_inner_module_is_left_to_the_balance() {
    let m = with_source(
        "module Inner {\n    .vdd: Rail\n    .sensor: Sensor\n    .vdd ~ .sensor.vs\n}\nmodule M {\n    .vdd: Rail\n    .inner: Inner\n    .alert: Logic\n    .vdd ~ .inner.vdd\n    .alert.reference ~ .vdd\n}",
    );
    // The reference stays a term in `m`: the inner module is not a part. In `m.inner` the
    // module's rail is the sensor's.
    keeps(
        &m,
        &[
            "m.inner.vdd.current",
            "-m.vdd.current",
            "-m.alert.reference.current",
        ],
    );
    let inner = m
        .balances()
        .iter()
        .find(|b| m.path_of(b.scope) == "m.inner")
        .expect("the inner module has a balance");
    assert_eq!(inner.draws.len(), 2);
}

#[test]
fn a_board_has_no_interface_to_alias() {
    let src = format!(
        "{PARTS}\nboard B {{\n    .supply: Rail {{\n        .pos.net &= \"V\"\n        .neg.net &= \"GND\"\n    }}\n    .sensor: Sensor\n    .supply ~ .sensor.vs\n}}\n"
    );
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    let b = m
        .balances()
        .iter()
        .find(|b| b.net == "V")
        .expect("a balance");
    assert_eq!(b.draws.len(), 2, "{:?}", b.draws);
}
