//! A `~>` chain sets its series current before the pick (ADR 0113).
//!
//! A chain from a rail's `pos` to the same rail's `neg`, through parts that each state a
//! `dc resistor` or a `dc diode` and with no tap between them, passes one current, and
//! Kirchhoff's voltage law along it says what that current is: the rail's voltage less the
//! diodes' forward voltages, over the resistances. The checker writes that relation, so an LED's
//! current window picks its resistor with no equation in the board.

use netsmith_check::{Checked, Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, Solution, solve};

/// The board of issue #621: no relation in it, only the chain.
const BOARD: &str = r#"import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer
board E2 {
    .process: TwoLayer
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .led: Led { .current &= 5mA..15mA }
    .r1: Resistor {}
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
"#;

fn run(src: &str) -> (Checked, Solution) {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    (c, s)
}

fn errors(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn q(s: &Solution, path: &str) -> Quantity {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

/// The LED's current over every corner of the picked parts and the supply.
fn worst_case_current(s: &Solution) -> Quantity {
    q(s, "supply.voltage")
        .sub(&q(s, "led.forward_voltage"))
        .unwrap()
        .div(&q(s, "r1.resistance"))
        .unwrap()
}

#[test]
fn the_led_current_window_picks_the_resistor() {
    let (_, s) = run(BOARD);
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    let current = worst_case_current(&s);
    assert!(
        current.lo() >= 5e-3 && current.hi() <= 15e-3,
        "the LED carries {current}, outside 5 mA..15 mA"
    );
    let r = q(&s, "r1.resistance");
    assert!(r.lo() > 10.0, "a {r} resistor is the pick of issue #621");
}

#[test]
fn the_resistor_pick_is_not_an_unconstrained_pick() {
    let (_, s) = run(BOARD);
    let names: Vec<String> = s
        .unconstrained
        .iter()
        .map(|u| s.model.path_of(u.part))
        .collect();
    assert!(names.is_empty(), "unconstrained: {names:?}");
}

#[test]
fn a_hand_written_relation_that_agrees_is_allowed() {
    let src = BOARD.replace(
        "    .supply.pos ~>",
        "    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance\n    .supply.pos ~>",
    );
    let (_, alone) = run(BOARD);
    let (_, both) = run(&src);
    assert!(errors(&both).is_empty(), "{:?}", errors(&both));
    let label = |s: &Solution| -> Vec<(String, String)> {
        s.picks
            .iter()
            .map(|p| (s.model.path_of(p.part), p.label.clone()))
            .collect()
    };
    assert_eq!(label(&alone), label(&both));
}

#[test]
fn a_hand_written_relation_that_disagrees_is_refused() {
    // Twice the resistance the chain has: no resistor gives the LED one current both ways.
    let src = BOARD
        .replace(
            "    .supply.pos ~>",
            "    .led.current == (.supply.voltage - .led.forward_voltage) / (2 * .r1.resistance)\n    .supply.pos ~>",
        )
        .replace(".r1: Resistor {}", ".r1: Resistor { .resistance &= 100ohm +/- 1% }");
    let (_, s) = run(&src);
    assert!(
        !errors(&s).is_empty(),
        "two currents for one LED should not both hold"
    );
}

#[test]
fn a_chain_with_two_resistors_shares_the_drop_between_them() {
    let src = BOARD
        .replace(
            ".r1: Resistor {}",
            ".r1: Resistor {}\n    .r2: Resistor { .resistance &= 100ohm +/- 1% }",
        )
        .replace(".r1 ~> .led", ".r1 ~> .r2 ~> .led");
    let (_, s) = run(&src);
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    let current = q(&s, "supply.voltage")
        .sub(&q(&s, "led.forward_voltage"))
        .unwrap()
        .div(&q(&s, "r1.resistance").add(&q(&s, "r2.resistance")).unwrap())
        .unwrap();
    assert!(
        current.lo() >= 5e-3 && current.hi() <= 15e-3,
        "the LED carries {current}"
    );
}

/// A part with a bridge and a current, and no `dc` statement.
const UNMODELLED: &str = r#"import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

part Lamp {
    .designator &= "DS"
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .current: Current
    bridge .a ~> .b
}

board E3 {
    .process: TwoLayer
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .lamp: Lamp { .current &= 5mA..15mA }
    .r1: Resistor {}
    .supply.pos ~> .r1 ~> .lamp ~> .supply.neg
}
"#;

#[test]
fn a_part_with_no_dc_element_leaves_the_current_free_and_is_named() {
    let (_, s) = run(UNMODELLED);
    let found: Vec<(String, Option<String>)> = s
        .unconstrained
        .iter()
        .map(|u| {
            (
                s.model.path_of(u.part),
                u.stopped_by.map(|p| s.model.path_of(p)),
            )
        })
        .collect();
    assert_eq!(found, [("r1".to_string(), Some("lamp".to_string()))]);
}

/// An LED whose window comes through a member (`.led.current &= .current`) and a board that also
/// writes the chain's equation itself. Counted as two equations of one current, the two only had
/// to share a value and the window was lost: a 1 kohm resistor put 2.3 mA to 3.5 mA through an
/// LED asked for 2 mA to 3 mA, with nothing said. The design's equation narrows the current, and
/// the chain's is checked against it.
const LINKED: &str = r#"import std::parts::passive::{Resistor, Led}
import std::sources::Rail

module Lamp {
    .supply: Rail
    .current: Current
    .current ?= 2mA..3mA
    .led: Led
    .r1: Resistor
    .led.current &= .current
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

board Dim {
    .input: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .lamp: Lamp
    .input ~ .lamp.supply
}
"#;

#[test]
fn a_window_through_a_member_holds_with_the_designs_own_equation_too() {
    for src in [
        LINKED.to_string(),
        LINKED.replace(
            "    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance\n",
            "",
        ),
    ] {
        let (_, s) = run(&src);
        assert!(errors(&s).is_empty(), "{:?}", errors(&s));
        let current = q(&s, "lamp.supply.voltage")
            .sub(&q(&s, "lamp.led.forward_voltage"))
            .unwrap()
            .div(&q(&s, "lamp.r1.resistance"))
            .unwrap();
        assert!(
            current.lo() >= 2e-3 && current.hi() <= 3e-3,
            "the LED carries {current}, outside 2 mA..3 mA"
        );
    }
}
