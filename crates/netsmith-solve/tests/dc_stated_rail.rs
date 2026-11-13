//! A voltage the design states on a rail that the DC analysis solves is checked against the
//! analysis (ADR 0076, amendment 17; issues #284 and #285).
//!
//! A stated range that does not enclose the range the analysis found is an error that names the
//! rail, the stated range, the analysis's range and the corner. A range that encloses it is
//! accepted and left as stated (a start window above the real range stays valid). Not checked: a
//! rail past a regulator's output, a rail on a net no source reaches, an island the analysis
//! leaves open, and the supply's own range (a source's voltage is an input). Every part and
//! number is invented, but the standard resistor, LED and linear regulator.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> Option<(f64, f64)> {
    match s.model.domain_at(path)? {
        Domain::Quantity(q) => Some((q.lo(), q.hi())),
        _ => None,
    }
}

/// Issue #284: a 3 V to 5.5 V supply through 1 ohm into a linear regulator whose input the module
/// `Stage` joins to its own port. The circuit puts the input at 2.989 V to 5.5 V (the least is
/// the supply less the drop of the 10 mA the regulator passes through the 1 ohm).
fn stage(window: &str, supply: &str) -> String {
    format!(
        r#"import std::sources::Rail
import std::parts::passive::Resistor
import std::parts::regulator::LinearRegulator
import std::parts::connector::Header1x2

module Stage {{
    .vin: Rail
    .vout: Rail
    .reg: LinearRegulator
    .vin ~ .reg.vin
    .reg.vout ~ .vout
}}

board B {{
    .supply: Rail {{ .voltage &= {supply}
        .pos.net &= "VIN"
        .neg.net &= "GND" }}
    .r: Resistor {{ .resistance &= 1ohm +/- 1% }}
    .stage: Stage {{ .vout.voltage &= 3.3V +/- 5%
{window}    }}
    .load: Rail {{ .pos.net &= "V33"
        .neg.net &= "GND"
        .current &= 0mA..10mA }}
    .supply.pos ~> .r ~> .stage.vin.pos
    .supply.neg ~ .stage.vin.neg
    .stage.vout ~ .load
    .p: Header1x2
    .supply.pos ~ .p.p1
    .supply.neg ~ .p.p2
}}
"#
    )
}

/// The seed is on the module's port and the regulator's own input is the other member of the
/// class: the circuit is below the seed, and the regulator is in dropout.
#[test]
fn a_seed_on_a_module_port_the_regulator_input_is_joined_to_is_checked() {
    let s = solved(&stage("        .vin.voltage &= 4.2V..5.5V\n", "3V..5.5V"));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].starts_with(
            "`stage.vin.voltage` is stated 4.2V..5.5V, but the DC analysis of the circuit around it finds 2.98"
        ),
        "{m:?}"
    );
    assert!(
        m[0].contains("at least 2.99V at the low end with supply at 3V"),
        "{m:?}"
    );
}

/// The same seed on the regulator itself (control B of the issue) says the same.
#[test]
fn a_seed_on_the_regulator_is_checked_as_before() {
    let src = stage("", "3V..5.5V").replace(
        ".reg: LinearRegulator\n",
        ".reg: LinearRegulator { .vin.voltage &= 4.2V..5.5V }\n",
    );
    let m = messages(&solved(&src));
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].starts_with("`stage.reg.vin.voltage` is stated 4.2V..5.5V, but the DC analysis"),
        "{m:?}"
    );
}

/// A window that encloses the range is a start, not a claim that fails: it is accepted and left
/// as stated, though the circuit is tighter. The supply here gives 5.2 V to 5.5 V, and the
/// regulator needs 4.145 V at the most.
#[test]
fn a_window_above_the_real_range_stays_valid_and_as_stated() {
    let s = solved(&stage("        .vin.voltage &= 4.5V..6V\n", "5.2V..5.5V"));
    assert_eq!(messages(&s), Vec::<String>::new());
    assert_eq!(range(&s, "stage.vin.voltage"), Some((4.5, 6.0)));
    assert_eq!(range(&s, "stage.reg.vin.voltage"), Some((4.5, 6.0)));
}

/// No statement on the rail, nothing to check: the analysis writes its range, as before.
#[test]
fn an_unstated_rail_takes_the_analysis_range() {
    let s = solved(&stage("", "5V..5.5V"));
    assert_eq!(messages(&s), Vec::<String>::new());
    let (lo, hi) = range(&s, "stage.vin.voltage").unwrap();
    assert!(
        (lo - 4.99).abs() < 1e-2 && (hi - 5.5).abs() < 1e-6,
        "{lo} {hi}"
    );
}

/// Issue #285: 12 V less 10 mA to 20 mA through 99 to 101 ohm is 9.98 V to 11.01 V at the
/// rail of a load that the design says takes 1.7 V to 3.6 V.
fn claim(extra: &str, stated: &str) -> String {
    format!(
        r#"import std::sources::Rail
import std::parts::passive::Resistor
import std::parts::connector::Header1x2

/// A part that takes a stated current and states no `dc`: a load of the analysis.
part Sink {{
    pins {{
        .a passive "A" "1"
        .b passive "B" "2"
    }}
    .current: Current
    bridge .a ~> .b
}}

module Load {{
    .vin: Rail
    .sink: Sink {{ .current &= 10mA..20mA }}
    .vin.pos ~> .sink ~> .vin.neg
}}

board B {{
    .supply: Rail {{
        .voltage &= 12V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .r: Resistor {{ .resistance &= 100ohm +/- 1% }}
    .load: Load {{ {stated} }}
    .supply.pos ~> .r ~> .load.vin.pos
    .supply.neg ~ .load.vin.neg
{extra}    .p: Header1x2
    .supply.pos ~ .p.p1
    .supply.neg ~ .p.p2
}}
"#
    )
}

#[test]
fn a_stated_rail_down_a_chain_is_checked_against_the_analysis() {
    let m = messages(&solved(&claim("", ".vin.voltage &= 1.7V..3.6V")));
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].starts_with(
            "`load.vin.voltage` is stated 1.7V..3.6V, but the DC analysis of the circuit around it finds 9.9"
        ),
        "{m:?}"
    );
    assert!(m[0].contains("at most 11.0"), "{m:?}");
    assert!(m[0].contains("at the high end with r at 99Ω"), "{m:?}");
    // The low end of the circuit (9.98 V) is above the stated 1.7 V, so only the high end is named.
    assert!(!m[0].contains("at the low end"), "{m:?}");
}

/// A stated claim and the relation that says the same give the same verdict: both fail here, and
/// both pass for a claim the circuit keeps.
#[test]
fn a_stated_claim_and_the_equivalent_relation_agree() {
    let stated = messages(&solved(&claim("", ".vin.voltage &= 1.7V..3.6V")));
    let relation = messages(&solved(&claim(
        "    .load.vin.voltage <= 3.6V\n    .load.vin.voltage >= 1.7V\n",
        "",
    )));
    assert!(
        !stated.is_empty() && !relation.is_empty(),
        "{stated:?} {relation:?}"
    );
    let held_stated = messages(&solved(&claim("", ".vin.voltage &= 9V..12V")));
    let held_relation = messages(&solved(&claim(
        "    .load.vin.voltage <= 12V\n    .load.vin.voltage >= 9V\n",
        "",
    )));
    assert_eq!(held_stated, Vec::<String>::new());
    assert_eq!(held_relation, Vec::<String>::new());
}

/// A stated range that encloses the circuit's is accepted and left as stated.
#[test]
fn an_enclosing_rail_is_left_as_stated() {
    let s = solved(&claim("", ".vin.voltage &= 9V..12V"));
    assert_eq!(messages(&s), Vec::<String>::new());
    assert_eq!(range(&s, "load.vin.voltage"), Some((9.0, 12.0)));
}

/// A range that overlaps the circuit's and does not enclose it is not enough: the circuit goes
/// outside at one end.
#[test]
fn a_range_that_only_overlaps_is_an_error() {
    let m = messages(&solved(&claim("", ".vin.voltage &= 10.5V..12V")));
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(m[0].contains("at least 9.9"), "{m:?}");
    assert!(m[0].contains("at the low end"), "{m:?}");
}

/// A node that something the analysis does not model sets (a connector's pin) leaves the island
/// open: nothing is solved there and nothing is checked.
#[test]
fn an_open_island_is_not_checked() {
    let s = solved(&claim(
        "    .tap: Header1x2\n    .tap.p1 ~ .load.vin.pos\n    .tap.p2 ~ .load.vin.neg\n",
        ".vin.voltage &= 1.7V..3.6V",
    ));
    assert_eq!(messages(&s), Vec::<String>::new());
}

/// A rail past a regulator's output is the regulator's: the part holds its node, and a window the
/// design states for it is not the circuit's to check (amendment 5, choice 3). A resistor after a
/// 3.3 V regulator drops a little, below the narrow window stated past it on a module's port. (A
/// `Rail` of the board with a stated voltage is a source and is never checked either.)
#[test]
fn a_rail_past_a_regulators_output_is_not_checked() {
    let src = r#"import std::sources::Rail
import std::parts::passive::Resistor
import std::parts::regulator::LinearRegulator
import std::parts::connector::Header1x2

module Tap {
    .vin: Rail
}

board B {
    .supply: Rail { .voltage &= 5V..5.5V
        .pos.net &= "VIN"
        .neg.net &= "GND" }
    .reg: LinearRegulator { .vout.voltage &= 3.3V +/- 5% }
    .r: Resistor { .resistance &= 10ohm +/- 1% }
    .tap: Tap { .vin.voltage &= 3.28V..3.32V }
    .load: Rail { .pos.net &= "V33B"
        .neg.net &= "GND"
        .current &= 0mA..10mA }
    .supply.pos ~> .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .reg.vout.pos ~> .r ~> .tap.vin.pos
    .reg.vout.neg ~ .tap.vin.neg
    .tap.vin ~ .load
    .p: Header1x2
    .supply.pos ~ .p.p1
    .supply.neg ~ .p.p2
}
"#;
    let s = solved(src);
    assert_eq!(messages(&s), Vec::<String>::new());
    assert_eq!(range(&s, "tap.vin.voltage"), Some((3.28, 3.32)));
}

/// The input of a regulator past another regulator's output is the same: the window a design
/// states on the rail that joins the two (the real boards' buck stage, whose output rail sits a
/// little below the node its IC holds) is not checked against the node past the winding.
#[test]
fn the_input_of_a_regulator_past_a_regulators_output_is_not_checked() {
    let src = r#"import std::sources::Rail
import std::parts::regulator::LinearRegulator
import std::parts::connector::Header1x2

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

part Sw {
    .vin: Rail
    .vout: Rail
    .efficiency: Number
    .min_input: Voltage
    .vin.neg ~ .vout.neg
    .vin.voltage >= .min_input
    dc switching_regulator .vin .vout .efficiency
}

module Tap {
    .vin: Rail
}

board B {
    .supply: Rail { .voltage &= 9V..16V
        .pos.net &= "VIN"
        .neg.net &= "GND" }
    .ic: Sw { .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V +/- 1% }
    .link: Link { .resistance &= 0.1ohm; .rated_current &= 2A }
    .tap: Tap { .vin.voltage &= 4.96V..5.04V }
    .ldo: LinearRegulator { .vout.voltage &= 3.3V +/- 5% }
    .load: Rail { .pos.net &= "V33"
        .neg.net &= "GND"
        .current &= 0mA..10mA }
    .supply.pos ~> .ic.vin.pos
    .supply.neg ~ .ic.vin.neg
    .ic.vout.pos ~> .link ~> .tap.vin.pos
    .ic.vout.neg ~ .tap.vin.neg
    .tap.vin ~ .ldo.vin
    .ldo.vout ~ .load
    .p: Header1x2
    .supply.pos ~ .p.p1
    .supply.neg ~ .p.p2
}
"#;
    let s = solved(src);
    assert_eq!(messages(&s), Vec::<String>::new());
    assert_eq!(range(&s, "tap.vin.voltage"), Some((4.96, 5.04)));
}

/// The supply's own range is an input: a source states 5 V to 5.5 V and nothing compares the
/// circuit with it, since the source holds the node. A regulator joined straight to the supply
/// has its input at the supply's range.
#[test]
fn the_supplys_own_range_is_never_checked() {
    let src = stage("", "5V..5.5V")
        .replace(
            ".supply.pos ~> .r ~> .stage.vin.pos",
            ".supply.pos ~ .stage.vin.pos",
        )
        .replace("    .r: Resistor { .resistance &= 1ohm +/- 1% }\n", "");
    let s = solved(&src);
    assert_eq!(messages(&s), Vec::<String>::new());
}
