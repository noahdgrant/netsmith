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
/// of `crates/netsmith-pipeline/tests/build.rs::an_incomplete_balance_is_noted...` states its stage's
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
    #[role(current)]
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
    #[role(current)]
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

/// Issue #710: the note asks for the diode's current, and a diode that states it is judged with
/// it. On a regulator's output the regulator's load stays what its own `.current` states (a
/// buck's catch diode moves the load's own current into the switch node), which the balance of
/// its net makes `out`'s 100 mA to 200 mA and the diode's up to 100 mA: the network in front of
/// it is solved with that, and there is no note.
#[test]
fn a_diode_on_a_regulator_output_that_states_its_current_is_not_noted() {
    let src = board(ORED).replace(
        ".max_reverse_voltage &= 40V }",
        ".max_reverse_voltage &= 40V; .current &= 0A..100mA }",
    );
    let s = solved(&src);
    let m = notes(&s);
    assert!(
        !m.iter().any(|m| m.contains("does not model the current")),
        "{m:?}"
    );
    assert!(m.is_empty(), "{m:?}");
    let (lo, hi) = range(&s, "link.current").expect("the network in front of `reg` is solved");
    assert!(
        (lo - 0.1).abs() < 1e-9 && (hi - 0.3).abs() < 1e-9,
        "{lo}..{hi}"
    );
}

/// A diode between two sources that no regulator holds: two board rails.
fn two_sources(diode: &str, budget: &str) -> String {
    board(&format!(
        "    .alt: Rail {{
        .voltage &= 5V
        .pos.net &= \"V5\"
        .neg.net &= \"GND\"
    }}
    .d: Diode {{ .forward_voltage &= 0.5V..0.8V; .max_forward_current &= 3A; .max_reverse_voltage &= 40V{diode} }}
    .supply.pos ~> .d ~> .alt.pos
    .alt.neg ~ .supply.neg
"
    ))
    .replace("        .voltage &= 12V\n", &format!("        .voltage &= 12V\n{budget}"))
}

/// Issue #710: a diode between two board rails that states no current is noted, and one that
/// states it is not.
#[test]
fn a_diode_between_two_sources_is_noted_only_when_it_states_no_current() {
    let s = solved(&two_sources("", ""));
    let m = notes(&s);
    assert!(
        m.iter().any(|m| m.contains(
            "the DC analysis does not model the current of `d`: it joins `VIN` and `V5`"
        ) && !m.contains("not a sum")),
        "{m:?}"
    );
    let s = solved(&two_sources("; .current &= 0A..100mA", ""));
    assert!(notes(&s).is_empty(), "{:?}", notes(&s));
}

/// The stated current is what the diode draws from the supply it is on: a supply that gives at
/// most 500 mA is short of a diode that moves 600 mA to 1 A, and has enough for one that moves
/// at most 100 mA.
#[test]
fn a_diode_between_two_sources_draws_its_stated_current_from_the_supply() {
    let budget = "        .current &= -500mA..0A\n";
    let s = solved(&two_sources("; .current &= 600mA..1A", budget));
    let m: Vec<String> = s.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert!(
        m.iter().any(|m| m.contains("the currents on net `VIN` do not add up")
            && m.contains("`d.current` is 600mA..1A")),
        "{m:?}"
    );
    let s = solved(&two_sources("; .current &= 0A..100mA", budget));
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert!(notes(&s).is_empty(), "{:?}", notes(&s));
}
