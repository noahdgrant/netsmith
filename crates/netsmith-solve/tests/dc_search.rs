//! A pick is refused on a result of the DC analysis (ADR 0076, amendment 10): the voltage a
//! candidate causes. A Zener clamp and its series resistor are picked together; the resistor's
//! drop and the Zener's current decide the voltage of the node behind the resistor, and the
//! board does not write that as an equation. Every number is invented, except the standard
//! resistor and Zener, whose catalogs are the repository's.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"import std::parts::diode::Zener
import std::parts::passive::{Capacitor, Resistor}
import std::sources::Rail

part Plug {
    .supply: Rail
    dc source .supply
    pins {
        .supply.pos power "V" "1"
        .supply.neg power "G" "2"
    }
}
"#;

/// The clamp sheet of a board that runs from a battery: a series resistor and a Zener to the
/// ground in front of a load that needs `4.5V`. The resistor is no smaller than the Zener's own
/// test current allows at the supply's peak, and what it drops at the load's current is what
/// the load's voltage has to afford. `{supply}` is the range of the battery and `{equation}` the
/// hand-written drop, or nothing.
fn board(supply: &str, equation: &str) -> String {
    board_with(supply, equation, "")
}

/// The same with more lines in the board.
fn board_with(supply: &str, equation: &str, more: &str) -> String {
    format!(
        "{PARTS}
board B {{
    .plug: Plug {{ .supply.voltage &= {supply}; .supply.peak &= 58V; .supply.pos.net &= \"VIN\"; .supply.neg.net &= \"GND\" }}
    .out: Rail {{
        .pos.net &= \"OUT\"
        .neg.net &= \"GND\"
        .current &= 0A..200uA
    }}
    .plug.supply.neg ~ .out.neg
    .r: Resistor
    .clamp: Zener
    .plug.supply.pos ~> .r ~> .out.pos
    .out.pos ~> .clamp ~> .out.neg
    .clamp.leakage_test_voltage >= .out.voltage
    .clamp.current &= 0A .. .clamp.leakage_current
    {equation}
    .clamp.zener_voltage >= 36V
    .surge_current: Current
    .surge_current == (.plug.supply.peak - .clamp.zener_voltage) / .r.resistance
    .surge_current <= .clamp.test_current
    .out.voltage >= 4.5V
    {more}
}}
"
    )
}

fn solved(src: &str) -> Solution {
    let c = check("search.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> Option<(f64, f64)> {
    match s
        .model
        .domain_at(path)
        .unwrap_or_else(|| panic!("no field {path}"))
    {
        Domain::Quantity(q) => Some((q.lo(), q.hi())),
        _ => None,
    }
}

fn picks(s: &Solution) -> Vec<(String, String)> {
    s.picks
        .iter()
        .map(|p| (s.model.path_of(p.part), p.candidate.clone()))
        .collect()
}

const EQUATION: &str = ".out.voltage == .plug.supply.voltage - .r.drop";

#[test]
fn a_pair_that_leaves_the_load_under_its_minimum_is_refused_at_the_pick() {
    // At 6 V the smallest resistor the Zener's test current allows drops 1.5 V and more at the
    // load's 200 uA, which leaves the load under 4.5 V: no pair of candidates is right, and the
    // pick says so, with no equation of the drop on the board.
    let s = solved(&board("6V..16V", ""));
    let m = messages(&s);
    assert!(
        m.iter().any(|m| m
            .contains("no combination of candidates satisfies `clamp` and `r` together")
            || m.contains("no combination of candidates satisfies `r` and `clamp` together")),
        "{m:#?}"
    );
}

#[test]
fn the_pick_message_is_the_one_the_equation_gave() {
    let with = messages(&solved(&board("6V..16V", EQUATION)));
    let without = messages(&solved(&board("6V..16V", "")));
    let head = |m: &[String]| {
        m.iter()
            .find(|m| m.starts_with("no combination of candidates"))
            .map(|m| m.split(": the closest").next().unwrap().to_string())
    };
    assert!(head(&with).is_some(), "{with:#?}");
    assert_eq!(head(&with), head(&without), "{without:#?}");
}

#[test]
fn the_picks_and_the_node_are_the_same_with_the_equation_and_without_it() {
    let with = solved(&board("9V..16V", EQUATION));
    let without = solved(&board("9V..16V", ""));
    assert!(messages(&with).is_empty(), "{:#?}", messages(&with));
    assert!(messages(&without).is_empty(), "{:#?}", messages(&without));
    assert_eq!(picks(&with), picks(&without));
    // The node is the supply less the resistor's drop at what the load and the Zener draw.
    let node = range(&without, "out.voltage").unwrap();
    assert!(node.0 > 4.5 && node.1 > 15.0 && node.1 < 16.001, "{node:?}");
    assert!(range(&with, "out.voltage").unwrap().0 > 4.5);
}

/// A capacitor across the node reads its voltage (its bias), so it is picked with the pair; the
/// failure is the pair's and not the capacitor's.
const HOLD: &str = ".hold: Capacitor { .capacitance >= 1uF }\n.out.pos ~> .hold ~> .out.neg";

#[test]
fn a_part_that_reads_the_node_does_not_make_the_failure_its_own() {
    let s = solved(&board_with("6V..16V", "", HOLD));
    let m = messages(&s);
    let found = m
        .iter()
        .find(|m| m.starts_with("no combination of candidates"))
        .unwrap_or_else(|| panic!("{m:#?}"));
    assert!(
        found.contains("satisfies `clamp` and `r` together")
            || found.contains("satisfies `r` and `clamp` together"),
        "{found}"
    );
    assert!(!found.contains("`hold`"), "{found}");
}

#[test]
fn the_capacitor_is_picked_on_the_node_the_analysis_gave() {
    let with = solved(&board_with("9V..16V", EQUATION, HOLD));
    let without = solved(&board_with("9V..16V", "", HOLD));
    assert!(messages(&with).is_empty(), "{:#?}", messages(&with));
    assert!(messages(&without).is_empty(), "{:#?}", messages(&without));
    assert_eq!(picks(&with), picks(&without));
}

#[test]
fn a_node_the_board_states_is_not_the_analysis() {
    // The board states the voltage behind the resistor as a window of its own: nothing is
    // written for it, and the pair is judged on the window as ever.
    let s = solved(&board_with("6V..16V", ".out.voltage &= 5V..5.5V", ""));
    assert!(
        messages(&s)
            .iter()
            .all(|m| !m.starts_with("no combination of candidates")),
        "{:#?}",
        messages(&s)
    );
}
