//! The load behind a regulator, before the picks (ADR 0076, amendment 11, issue #273). Every part
//! and number here is invented.
//!
//! The capacitor across the regulator's input reads that rail's voltage, which has no value
//! before the picks: the picks wait on the DC analysis's start, and the start is where the
//! regulator's load is enclosed.
//!
//! A bead and a shunt are picked from a catalog of two rows each, in front of a linear regulator
//! whose rated range is far above what the loads on its output state. The picks in front of it
//! are judged on the current the regulator's output supplies: the board's stated window if it
//! states one, else the sum of what the loads state, else (a load that states nothing) the
//! regulator's rated limit.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"import std::parts::passive::{Capacitor, Resistor}
import std::sources::Rail

/// A bead: a small part and a large one.
generic Bead {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .rated_current: Current
    .dc_resistance: Resistance
    .current: Current
    .drop: Voltage
    .drop == .current * .dc_resistance
    .current <= .rated_current
    bridge .a ~> .b
    dc resistor .a .b .dc_resistance
    .package: Package
    catalog zbead { .rated_current, .dc_resistance, .package }
}

/// A shunt: a small part and a large one, judged on its power.
generic Shunt {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .max_power: Power
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    .drop * .current <= .max_power
    bridge .a ~> .b
    dc resistor .a .b .resistance
    .package: Package
    catalog zshunt { .resistance, .max_power, .package }
}

/// A linear regulator with a rated limit on what its output supplies.
part Ldo {
    .vin: Rail
    .vout: Rail
    .max_current: Current
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current
    -.vout.current <= .max_current
    dc linear_regulator .vin .vout
}

/// A switching regulator: 5 V out, 85 % to 95 %, working from 4.5 V.
part Buck {
    .vin: Rail
    .vout: Rail
    .efficiency: Number
    .min_input: Voltage
    .vin.neg ~ .vout.neg
    .vin.voltage >= .min_input
    .vin.current == -.vout.current * .vout.voltage / (.vin.voltage * .efficiency)
    dc switching_regulator .vin .vout .efficiency
}

/// A bleeder across a rail: a resistor still to be picked, whose draw is the rail's voltage over
/// the resistance the design asks for. Nothing gives its current until the resistor is picked.
module Bleeder {
    .supply: Rail
    .resistance: Resistance
    .resistor: Resistor
    .floor: Resistance
    .resistance == .resistor.resistance
    .resistor.resistance >= .floor
    .resistor.current == .supply.voltage / .resistor.resistance
    .supply.pos ~> .resistor ~> .supply.neg
}

/// A load that states its supply current, or nothing.
part Load {
    .vdd: Rail
}
"#;

const BEADS: &str = "name,rated_current,dc_resistance,package\n\
small,0.2437A,0.3ohm,0603\n\
large,3A,0.05ohm,1206\n";

const SHUNTS: &str = "name,resistance,max_power,package\n\
small,50mohm,0.002968W,0603\n\
large,50mohm,2W,2512\n";

fn catalogs() -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    c.add_file("zbead", "catalogs/zbead.csv", BEADS).unwrap();
    c.add_file("zshunt", "catalogs/zshunt.csv", SHUNTS).unwrap();
    c
}

/// A 9 V to 16 V supply, a bead and a shunt in a chain to a 5 V buck stage, an LDO rated 2 A at
/// 3.3 V on its output, a bleeder on the 3.3 V net (a resistor still to be picked, and tied to the shunt's pick by a relation, so that the shunt is chosen before the current of the bleeder is known) and `loads`
/// on it (one line each, as `(name, supply current)`; `None` states nothing), and `extra`.
///
/// The picks of the bead and the shunt are judged on the current of the supply: the stage takes
/// `Pout / (eff * Vin)`, and before the picks `Vin` is only known to lie in the start's
/// enclosure (8.06 V to 16 V, with the least bead and shunt at their worst), so the current the
/// picks see is a little larger than the circuit's.
fn board(loads: &[(&str, Option<&str>)], extra: &str) -> String {
    let mut body = String::from(
        "    .supply: Rail { .voltage &= 9V..16V; .pos.net &= \"VIN\"; .neg.net &= \"GND\" }\n\
    .bead: Bead\n\
    .shunt: Shunt\n\
    .buck: Buck { .efficiency &= 0.85..0.95; .min_input &= 4.5V; .vout.voltage &= 5V }\n\
    .ldo: Ldo { .vout.voltage &= 3.3V; .vout.pos.net &= \"V33\"; .vout.neg.net &= \"GND\"; .max_current &= 2A }\n\
    .supply.pos ~> .bead ~> .shunt ~> .buck.vin.pos\n\
    .supply.neg ~ .buck.vin.neg\n\
    .buck.vout.pos ~ .ldo.vin.pos\n\
    .buck.vout.neg ~ .ldo.vin.neg\n\
    .hold: Capacitor\n\
    .hold.capacitance >= 1uF\n\
    .hold.effective_capacitance >= 0.5uF\n\
    .buck.vin.pos ~> .hold ~> .buck.vin.neg\n\
    .zbleed: Bleeder { .resistance &= 1kohm +/- 20% }\n\
    .ldo.vout ~ .zbleed.supply\n\
    .zbleed.floor == 1000 * .shunt.resistance\n",
    );
    for (name, current) in loads {
        body.push_str(&format!("    .{name}: Load\n    .ldo.vout ~ .{name}.vdd\n"));
        if let Some(c) = current {
            body.push_str(&format!("    .{name}.vdd.current &= {c}\n"));
        }
    }
    body.push_str(extra);
    format!("{PARTS}\nboard B {{\n{body}}}\n")
}

fn solved(src: &str) -> Solution {
    solved_with(src, &catalogs())
}

fn solved_with(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("loads.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn picked(s: &Solution, path: &str) -> String {
    s.picks
        .iter()
        .find(|p| s.model.path_of(p.part) == path)
        .unwrap_or_else(|| panic!("`{path}` was not picked: {:?}", messages(s)))
        .candidate
        .clone()
}

fn range(s: &Solution, path: &str) -> Option<(f64, f64)> {
    match s
        .model
        .domain_at(path)
        .unwrap_or_else(|| panic!("no field {path}"))
    {
        netsmith_check::Domain::Quantity(q) => Some((q.lo(), q.hi())),
        _ => None,
    }
}

fn picks(s: &Solution) -> (String, String) {
    (picked(s, "bead"), picked(s, "shunt"))
}

/// Three loads of 0 A to 110 mA each: 330 mA at the most.
const STATED: [(&str, Option<&str>); 3] = [
    ("a", Some("0A..110mA")),
    ("b", Some("0A..110mA")),
    ("c", Some("0A..110mA")),
];

#[test]
fn loads_that_state_a_current_give_the_picks_of_a_window_of_the_same_size() {
    let none = solved(&board(&STATED, ""));
    let window = solved(&board(&STATED, "    .ldo.vout.current &= -0.4A..0A\n"));
    assert!(messages(&none).is_empty(), "{:?}", messages(&none));
    assert!(messages(&window).is_empty(), "{:?}", messages(&window));
    let large = ("large".to_string(), "large".to_string());
    // A window of 0.4 A, wider than the 330 mA the loads state, has the parts that carry what
    // the stage draws: the small bead (0.2437 A) and shunt (2.968 mW) are refused.
    assert_eq!(picks(&window), large);
    // The loads state less than the window does, so they give the same picks.
    assert_eq!(picks(&none), large);
}

#[test]
fn a_load_that_states_nothing_leaves_the_regulator_without_a_sum() {
    let loads = [
        ("a", Some("0A..110mA")),
        ("b", Some("0A..110mA")),
        ("c", None),
    ];
    let s = solved(&board(&loads, ""));
    // Nothing is guessed for the load that says nothing: the output's current has no value, as
    // it had, and the regulator's rated limit stays what the start encloses it by.
    assert_eq!(range(&s, "ldo.vout.current"), None);
}

#[test]
fn the_sum_is_the_current_of_the_output_after_the_picks() {
    let s = solved(&board(&STATED, ""));
    let (lo, hi) = range(&s, "ldo.vout.current").expect("a value");
    // The three loads state up to 330 mA, and nothing the bleeder draws is below zero.
    assert!(lo <= -0.33 && lo > -0.34, "{lo}");
    assert!(hi <= 0.0, "{hi}");
}

#[test]
fn the_picks_are_the_same_every_time() {
    let a = solved(&board(&STATED, ""));
    let b = solved(&board(&STATED, ""));
    assert_eq!(picks(&a), picks(&b));
    assert_eq!(messages(&a), messages(&b));
}
