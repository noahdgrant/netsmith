//! The DC analysis says when it cannot judge (issue #289, items 1 to 4).
//!
//! A check that cannot run says so: it never passes silently. Each test builds a small board
//! and reads what the pass did with it. Every part and number is invented, but the standard
//! resistor, diode and linear regulator.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

/// The notes of the build: what it did not evaluate and why.
fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated.iter().map(|n| n.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> Option<(f64, f64)> {
    match s.model.domain_at(path)? {
        Domain::Quantity(q) => Some((q.lo(), q.hi())),
        _ => None,
    }
}

/// Item 1. The supply's range is stated on the port of a module that the board's rail is joined
/// to, and not on the board's own rail.
fn module_side(stated_on_board: bool) -> String {
    let (board_side, module_side) = if stated_on_board {
        (".voltage &= 5V..5.5V;", "")
    } else {
        ("", ".vin.voltage &= 5V..5.5V")
    };
    format!(
        r#"import std::sources::Rail
import std::parts::passive::Resistor

module Front {{
    .vin: Rail
    {module_side}
}}

board B {{
    .supply: Rail {{
        {board_side}
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .front: Front
    .r: Resistor {{ .resistance &= 1ohm +/- 1% }}
    .load: Rail {{
        .pos.net &= "V1"
        .neg.net &= "GND"
        .current &= 0mA..10mA
    }}
    .supply ~ .front.vin
    .supply.pos ~> .r ~> .load.pos
    .supply.neg ~ .load.neg
}}
"#
    )
}

/// Reproduced: with the range stated on the module's port the pass finds no source, extracts
/// nothing and says nothing. This pins that behaviour and changes none: a range on a module's
/// port is also how a design seeds a node that a circuit sets (ADR 0076 amendment 17; the board
/// of `crates/netsmith/tests/build.rs::an_incomplete_balance_is_noted...` states its stage's
/// output that way), so a note on every such rail would be noise on boards that mean it, and
/// whether the rail is a source or a seed is the language's to say (owner question in the
/// pull request that added this test).
#[test]
fn a_supply_stated_on_a_module_port_is_not_a_source() {
    let on_board = solved(&module_side(true));
    assert!(on_board.dc.solved > 0, "{:?}", on_board.dc);
    let on_module = solved(&module_side(false));
    assert_eq!(on_module.dc.islands, 0, "{:?}", on_module.dc);
    assert_eq!(on_module.dc.solved, 0, "{:?}", on_module.dc);
    assert!(notes(&on_module).is_empty(), "{:?}", notes(&on_module));
}

const PARTS: &str = r#"import std::parts::diode::Diode
import std::parts::passive::Resistor
import std::sources::Rail

part Link {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .rated_current: Current
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    .current <= .rated_current
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

part Ldo {
    .vin: Rail
    .vout: Rail
    .min_input: Voltage
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current
    .vin.voltage >= .min_input
    dc linear_regulator .vin .vout
}

/// An N-channel switch: the channel between drain and source is open below the threshold.
part Switch1 {
    pins {
        .gate   input   "G" "1"
        .source passive "S" "2"
        .drain  passive "D" "3"
    }
    .gate_threshold: Voltage
    .on_resistance: Resistance
    .current: Current
    bridge .drain ~> .source
    dc nmos .gate .drain .source .gate_threshold .on_resistance
}
"#;

fn board(body: &str) -> String {
    format!(
        "{PARTS}\nboard B {{\n    .supply: Rail {{\n        .voltage &= 12V\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n{body}}}\n"
    )
}

/// Item 2: an N-channel source follower. The gate is on the supply and the source is on a load:
/// on is refused because the source rises to the gate (no gate drive is left), off is refused
/// because the gate is above the threshold. No combination is the circuit's.
const FOLLOWER: &str =
    "    .q: Switch1 { .gate_threshold &= 1V..2.5V; .on_resistance &= 50mohm..100mohm }
    .load: Resistor { .resistance &= 10kohm +/- 1% }
    .supply.pos ~ .q.gate
    .supply.pos ~ .q.drain
    .q.source ~ .load.a
    .load.b ~ .supply.neg
";

/// Reproduced: the island is extracted, no combination is its circuit's, nothing is written and
/// nothing is said. It now says which parts and why.
#[test]
fn an_island_with_no_operating_point_is_named() {
    let s = solved(&board(FOLLOWER));
    assert_eq!(s.dc.islands, 1, "{:?}", s.dc);
    assert_eq!(s.dc.solved, 0, "{:?}", s.dc);
    let m = notes(&s);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("the DC analysis found no operating point for the network of")
            && m[0].contains("`q`")
            && m[0].contains("`load`")
            && m[0].contains("gate"),
        "{m:?}"
    );
}

/// Item 2: a regulator whose output net is pushed from a higher supply through a resistor takes
/// current in at its output, which is no regulator's load: the network in front of it is left
/// alone.
const PUSHED: &str = "    .link: Link { .resistance &= 0.5ohm; .rated_current &= 2A }
    .reg: Ldo { .min_input &= 4.5V; .vout.voltage &= 3.3V }
    .pull: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .link ~> .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .reg.vout.pos.net &= \"V33\"
    .reg.vout.neg ~ .supply.neg
    .supply.pos ~ .pull.a
    .reg.vout.pos ~ .pull.b
";

/// Reproduced: the network in front of the regulator is dropped (`link.current` has no value)
/// and nothing says so. It now names the network, the regulator and the current taken in.
#[test]
fn an_island_left_alone_for_a_regulator_pushed_back_is_named() {
    let s = solved(&board(PUSHED));
    assert_eq!(range(&s, "link.current"), None);
    let m = notes(&s);
    let left: Vec<&String> = m
        .iter()
        .filter(|m| m.contains("the DC analysis left the network of"))
        .collect();
    assert_eq!(left.len(), 1, "{m:?}");
    assert!(
        left[0].contains("`link`")
            && left[0].contains("`reg.vout`")
            && left[0].contains("take 8.7")
            && left[0].contains("in, which no regulator supplies"),
        "{m:?}"
    );
}

/// A regulator whose output has a pin nothing models and states no current is left alone too,
/// and says so.
#[test]
fn a_regulator_with_no_known_load_names_the_island_it_leaves() {
    let src = board(
        "    .link: Link { .resistance &= 0.5ohm; .rated_current &= 2A }
    .reg: Ldo { .min_input &= 4.5V; .vout.voltage &= 3.3V }
    .unknown: Sink
    .supply.pos ~> .link ~> .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .reg.vout.pos ~ .unknown.p
    .reg.vout.neg ~ .supply.neg
    .unknown.g ~ .supply.neg
",
    )
    .replace(
        "part Link {",
        "part Sink {\n    pins {\n        .p bidirectional \"P\" \"1\"\n        .g bidirectional \"G\" \"2\"\n    }\n}\n\npart Link {",
    );
    let s = solved(&src);
    assert_eq!(range(&s, "link.current"), None);
    let m = notes(&s);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("the DC analysis left the network of `reg`, `link` alone: what the output of `reg.vout` supplies is not known")
            && m[0].contains("`reg.vout.current` states no range"),
        "{m:?}"
    );
}

/// Item 4: a diode that joins a regulator's output to another source.
const ORED: &str = "    .link: Link { .resistance &= 0.5ohm; .rated_current &= 2A }
    .reg: Ldo { .min_input &= 4.5V; .vout.voltage &= 3.3V }
    .out: Rail {
        .pos.net &= \"V33\"
        .neg.net &= \"GND\"
        .current &= 100mA..200mA
    }
    .alt: Rail {
        .voltage &= 2V
        .pos.net &= \"V2\"
        .neg.net &= \"GND\"
    }
    .d: Diode { .forward_voltage &= 0.5V..0.8V; .max_forward_current &= 3A; .max_reverse_voltage &= 40V }
    .supply.pos ~> .link ~> .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .reg.vout ~ .out
    .out.pos ~> .d ~> .alt.pos
    .alt.neg ~ .supply.neg
";

/// Reproduced: the diode is dropped, its pins count as accounted and nothing says so. Now the
/// pass names it, and the regulator's output is no longer a net whose loads are all summed.
#[test]
fn a_diode_between_two_held_nets_is_named() {
    let s = solved(&board(ORED));
    let m = notes(&s);
    let named: Vec<&String> = m
        .iter()
        .filter(|m| m.contains("the DC analysis does not model the current of `d`"))
        .collect();
    assert_eq!(named.len(), 1, "{m:?}");
    assert!(
        named[0].contains("joins `V33` and `V2`")
            && named[0].contains("state the current it carries (`d.current &= 0A..100mA`)")
            && named[0].contains("what its own `.current` states, not a sum"),
        "{m:?}"
    );
}

/// With no current stated on the output, the diode's pins were counted as accounted and the
/// regulator's load was a sum of nothing: the input drew zero and `link.current` was a point at
/// zero. The output is no complete net now, and the network in front of the regulator is left
/// alone and named.
#[test]
fn a_regulator_output_with_a_dropped_diode_has_no_load_of_zero() {
    let src = board(ORED).replace("        .current &= 100mA..200mA\n", "");
    let s = solved(&src);
    assert_eq!(range(&s, "link.current"), None);
    let m = notes(&s);
    assert!(
        m.iter()
            .any(|m| m.contains("the DC analysis left the network of `reg`, `link` alone")),
        "{m:?}"
    );
}

/// A diode between a regulator's output and a net the regulator does not hold: unchanged.
#[test]
fn a_diode_in_series_with_a_load_is_still_modelled() {
    let s = solved(&board(
        "    .reg: Ldo { .min_input &= 4.5V; .vout.voltage &= 3.3V }
    .d: Diode { .forward_voltage &= 0.5V..0.8V; .max_forward_current &= 3A; .max_reverse_voltage &= 40V }
    .load: Resistor { .resistance &= 330ohm +/- 1% }
    .supply ~ .reg.vin
    .reg.vout.pos.net &= \"V33\"
    .reg.vout.pos ~> .d ~> .load.a
    .load.b ~ .supply.neg
",
    ));
    let m = notes(&s);
    assert!(
        !m.iter().any(|m| m.contains("does not model the current")),
        "{m:?}"
    );
}
