//! A `~>` chain that starts on a logic pin, or runs between two rails, sets its series current
//! before the pick (ADR 0127, after ADR 0113).
//!
//! A pin that drives the start high is at least its `voh` (at its test current) and at most its
//! supply; one that sinks the end low is at most its `vol`. A chain from one rail's `pos` to
//! another's has the difference of the two voltages across it. The pin's current is at most the
//! test current its level is measured at, and a window on the chain's current is judged with the
//! pin driving the chain: with the pin in its other state the chain is off.

use netsmith_check::{Checked, Domain, Loader, RExpr, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, Solution, solve};

/// A part with one push-pull output whose levels are stated at a test current.
const MCU: &str = r#"part Mcu {
    .designator &= "U"
    .vdd: Rail
    .gpio: Logic
    .gpio.reference ~ .vdd
    .gpio.voh &= .vdd.voltage - 0.4V at 8mA
    .gpio.vol &= 0.4V at 8mA
    pins {
        .vdd.pos    power  "VDD" "1"
        .vdd.neg    power  "GND" "2"
        .gpio.line  output "IO"  "3"
    }
}
"#;

/// A GPIO that drives an LED through a resistor to ground, with no relation in the board.
fn gpio_board(window: &str) -> String {
    format!(
        r#"import std::parts::passive::{{Resistor, Led}}
import std::logic::Logic
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer
{MCU}
board G {{
    .process: TwoLayer
    .supply: Rail {{ .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }}
    .mcu: Mcu
    .supply ~ .mcu.vdd
    .led: Led {{ .current &= {window} }}
    .r1: Resistor {{}}
    .mcu.gpio.line ~> .r1 ~> .led ~> .supply.neg
}}
"#
    )
}

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

/// The relation the checker wrote for the chain, as the board would write it.
fn written(c: &Checked) -> Vec<String> {
    c.boards[0]
        .model
        .relations()
        .iter()
        .filter_map(|r| r.written.as_ref().filter(|w| !w.chain.is_empty()))
        .map(|w| w.text.clone())
        .collect()
}

#[test]
fn a_gpio_led_picks_its_resistor_from_the_window() {
    let (c, s) = run(&gpio_board("2mA..8mA"));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert_eq!(
        written(&c),
        [
            ".led.current == ((.mcu.gpio.voh .. .mcu.gpio.reference.voltage) - .led.forward_voltage) / .r1.resistance"
        ]
    );
    // The worst case: the pin as low as its `voh` with the LED at its greatest drop, and as high
    // as its supply with the least drop.
    let r = q(&s, "r1.resistance");
    let vf = q(&s, "led.forward_voltage");
    let lo = (q(&s, "mcu.gpio.voh").lo() - vf.hi()) / r.hi();
    let hi = (q(&s, "supply.voltage").hi() - vf.lo()) / r.lo();
    assert!(
        lo >= 2e-3 && hi <= 8e-3,
        "the LED carries {lo}..{hi} A through {r}, outside 2 mA..8 mA"
    );
    assert!(s.unconstrained.is_empty(), "{:?}", s.unconstrained.len());
}

#[test]
fn a_window_above_the_pins_test_current_is_refused_at_the_level() {
    let (_, s) = run(&gpio_board("10mA..15mA"));
    let found: Vec<_> = s
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("at most 8mA"))
        .collect();
    assert!(!found.is_empty(), "{:?}", errors(&s));
    assert!(
        found
            .iter()
            .all(|d| d.message.contains("led.current")),
        "{:?}",
        errors(&s)
    );
}

#[test]
fn a_level_with_no_test_current_leaves_the_chain_free_and_is_named() {
    let src = gpio_board("2mA..8mA")
        .replace("- 0.4V at 8mA", "- 0.4V")
        .replace(".led: Led { .current &= 2mA..8mA }", ".led: Led");
    let (c, s) = run(&src);
    assert!(written(&c).is_empty(), "{:?}", written(&c));
    let chain = &c.boards[0].model.chains()[0];
    assert_eq!(
        chain.unloaded_level.map(|f| c.boards[0].model.path_of_field(f)),
        Some("mcu.gpio.voh".to_string())
    );
    let named: Vec<_> = s
        .unconstrained
        .iter()
        .filter_map(|u| u.unloaded_level.map(|f| s.model.path_of_field(f)))
        .collect();
    assert!(named.contains(&"mcu.gpio.voh".to_string()), "{named:?}");
}

#[test]
fn a_pin_that_sinks_the_chain_is_at_most_its_vol() {
    let src = gpio_board("2mA..8mA").replace(
        ".mcu.gpio.line ~> .r1 ~> .led ~> .supply.neg",
        ".supply.pos ~> .r1 ~> .led ~> .mcu.gpio.line",
    );
    let (c, s) = run(&src);
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert_eq!(
        written(&c),
        [".led.current == (.supply.voltage - (0V .. .mcu.gpio.vol) - .led.forward_voltage) / .r1.resistance"]
    );
    let r = q(&s, "r1.resistance");
    let vf = q(&s, "led.forward_voltage");
    let v = q(&s, "supply.voltage");
    let lo = (v.lo() - q(&s, "mcu.gpio.vol").hi() - vf.hi()) / r.hi();
    let hi = (v.hi() - vf.lo()) / r.lo();
    assert!(lo >= 2e-3 && hi <= 8e-3, "{lo}..{hi} A through {r}");
}

/// A chain from one rail's `pos` to another's that holds its voltage (a connector's supply, a
/// `dc source`): the difference of the two is across it.
fn two_rails(low_rail: &str) -> String {
    format!(
        r#"import std::parts::passive::{{Resistor, Led}}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

part Plug {{
    .designator &= "J"
    .supply: Rail
    dc source .supply
    pins {{
        .supply.pos power "V" "1"
        .supply.neg power "G" "2"
    }}
}}

board T {{
    .process: TwoLayer
    .high: Rail {{ .voltage &= 12V +/- 5%; .pos.net &= "V12"; .neg.net &= "GND" }}
    .low: Plug {{ .supply.voltage &= {low_rail}; .supply.pos.net &= "V5"; .supply.neg.net &= "GND" }}
    .led: Led {{ .current &= 5mA..15mA }}
    .r1: Resistor {{}}
    .high.pos ~> .r1 ~> .led ~> .low.supply.pos
}}
"#
    )
}

#[test]
fn a_chain_between_two_rails_has_their_difference_across_it() {
    let (c, s) = run(&two_rails("5V +/- 5%"));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert_eq!(
        written(&c),
        [".led.current == (.high.voltage - .low.supply.voltage - .led.forward_voltage) / .r1.resistance"]
    );
    let r = q(&s, "r1.resistance");
    let vf = q(&s, "led.forward_voltage");
    let (a, b) = (q(&s, "high.voltage"), q(&s, "low.supply.voltage"));
    let lo = (a.lo() - b.hi() - vf.hi()) / r.hi();
    let hi = (a.hi() - b.lo() - vf.lo()) / r.lo();
    assert!(lo >= 5e-3 && hi <= 15e-3, "{lo}..{hi} A through {r}");
}

#[test]
fn a_corner_where_the_current_would_go_negative_is_refused() {
    // 12 V less 11 V leaves 0.4 V to 1.6 V across the chain, below any LED's forward voltage at
    // some corner: the LED's `.current >= 0mA` refuses it, as for a chain across one rail.
    let (_, s) = run(&two_rails("11V +/- 1%"));
    assert!(!errors(&s).is_empty(), "the LED cannot light at every corner");
}

#[test]
fn the_chain_records_the_pin_that_drives_it() {
    let (c, _) = run(&gpio_board("2mA..8mA"));
    let m = &c.boards[0].model;
    let chain = &m.chains()[0];
    let pins: Vec<_> = chain.pins.iter().map(|p| (m.path_of(p.pin), p.high)).collect();
    assert_eq!(pins, [("mcu.gpio.line".to_string(), true)]);
    // The pin's limit is a relation of the chain's current, at the level's own statement.
    let limit = m
        .relations()
        .iter()
        .find(|r| matches!(r.rhs, RExpr::Const(q) if q.hi() == 8e-3))
        .expect("the pin's limit");
    assert!(matches!(&limit.lhs, RExpr::Field(f) if m.path_of_field(*f) == "led.current"));
}

#[test]
fn a_boards_own_rail_at_the_end_is_a_load_and_ends_no_law() {
    // A board's rail at the chain's end, even with a voltage, is the load the chain feeds (the
    // window of voltage it takes): its voltage is what the chain leaves it, and the chain says
    // no current.
    let src = two_rails("5V +/- 5%")
        .replace(
            ".low: Plug { .supply.voltage &= 5V +/- 5%; .supply.pos.net &= \"V5\"; .supply.neg.net &= \"GND\" }",
            ".low: Rail { .voltage &= 5V +/- 5%; .pos.net &= \"V5\"; .neg.net &= \"GND\" }",
        )
        .replace(".low.supply.pos", ".low.pos");
    assert!(src.contains(".low: Rail"), "{src}");
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    assert!(written(&c).is_empty(), "{:?}", written(&c));
}
