//! Whether a load switch's enable can turn it on (issue 1269): something on its net that can make
//! it high (a supply, a host's pin that drives, a passive part), and not a ground, nothing, or
//! only inputs and open-drain outputs. The parts are invented.

use netsmith_check::*;

const PARTS: &str = r#"import std::parts::passive::Resistor
import std::parts::switch::LoadSwitch
import std::sources::Rail

part Zsw provides LoadSwitch {
    pins {
        .in.pos  power "VIN"  "1"
        .in.neg  power "GND"  "2"
        .enable  input "ON"   "3" optional
        .out.pos power "VOUT" "4"
    }
}

part Host {
    .vdd: Rail
    pins {
        .vdd.pos power      "VDD" "1"
        .vdd.neg power      "GND" "2"
        .gpio    output     "IO"  "3"
        .od      open_drain "OD"  "4"
        .sense   input      "IN"  "5"
    }
}
"#;

fn enable(joins: &str) -> Option<EnableOff> {
    let src = format!(
        "{PARTS}\nboard B {{\n    .supply: Rail {{\n        .voltage &= 5V\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n    .sw: Zsw\n    .host: Host\n    .supply ~ .sw.in\n    .supply ~ .host.vdd\n{joins}\n}}\n"
    );
    let c = check("test.nsm", &src, &Loader::std());
    let model = &c.board("B").unwrap_or_else(|| panic!("{}", c.render())).model;
    let sw = model.inst_at("sw").unwrap();
    let (pin, off) = model.load_switch_enable(sw).expect("a load switch");
    assert_eq!(model.path_of(pin), "sw.enable");
    assert_eq!(model.load_switch_can_turn_on(sw), off.is_none());
    off
}

#[test]
fn an_enable_strapped_to_a_supply_or_driven_by_a_host_can_turn_the_switch_on() {
    assert_eq!(enable("    .sw.enable ~ .supply.pos"), None);
    assert_eq!(enable("    .sw.enable ~ .sw.in.pos"), None);
    assert_eq!(enable("    .sw.enable ~ .host.gpio"), None);
}

#[test]
fn an_enable_behind_a_passive_part_can_turn_the_switch_on() {
    // A pull-up to the supply, with an open-drain output that pulls it low.
    assert_eq!(
        enable(
            "    .pull: Resistor { .resistance &= 100kohm +/- 5% }\n    .supply.pos ~> .pull ~> .sw.enable\n    .sw.enable ~ .host.od\n"
        ),
        None
    );
}

#[test]
fn an_enable_on_a_ground_or_alone_or_among_inputs_cannot() {
    assert_eq!(
        enable("    .sw.enable ~ .supply.neg"),
        Some(EnableOff::HeldLow)
    );
    // A pull-down on the ground's own net does not lift it.
    assert_eq!(
        enable(
            "    .pull: Resistor { .resistance &= 100kohm +/- 5% }\n    .sw.enable ~ .supply.neg\n    .supply.pos ~> .pull ~> .supply.neg\n"
        ),
        Some(EnableOff::HeldLow)
    );
    assert_eq!(enable(""), Some(EnableOff::Floating));
    assert_eq!(
        enable("    .sw.enable ~ .host.sense"),
        Some(EnableOff::HeldLow)
    );
    assert_eq!(
        enable("    .sw.enable ~ .host.od"),
        Some(EnableOff::HeldLow)
    );
}
