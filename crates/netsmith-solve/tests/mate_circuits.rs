//! The circuit of a line across a system mate (issue #1427): the DC analysis solves the nets
//! that mated contacts join as one node, and the checks of a pin with no levels and of a level
//! through a series part or a divider follow the line from one board into the other.
//!
//! `Driver`, `Receiver`, `Drain` and `PlainIn` are invented for these tests: parts with a
//! supply and one pin whose levels are what a datasheet might say. None is a real part.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve_system};

const PARTS: &str = r#"
import std::logic::Logic
import std::parts::connector::{Header1x4, Socket1x4}
import std::parts::passive::Resistor
import std::sources::Rail

/// Drives a line to 70 % of its supply and to 0.1 V at the bottom, at no stated load.
part Driver {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= 0.7 * .vdd.voltage
    .out.vol &= 0.1V
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2"
        .out.line  output "OUT" "3"
    }
}

/// Pulls a line low, 0.4 V at 3 mA, and leaves its high to the circuit around it.
part Drain {
    .vdd: Rail
    .io: Logic
    .io.reference ~ .vdd
    .io.vol &= 0.4V at 3mA
    pins {
        .vdd.pos   power      "VDD" "1"
        .vdd.neg   power      "GND" "2"
        .io.line   open_drain "IO"  "3"
    }
}

/// Reads a high from 70 % of its supply and a low up to 30 %.
part Receiver {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 0.7 * .vdd.voltage
    .in.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .in.line   input "IN"  "3"
    }
}

/// An enable input whose datasheet gives no levels.
part PlainIn {
    .vdd: Rail
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .en        input "EN"  "3"
    }
}
"#;

fn run(src: &str) -> Solution {
    let src = format!("{PARTS}\n{src}");
    let c = check("main.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve_system(&c.systems[0], &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated.iter().map(|n| n.message.clone()).collect()
}

/// A carrier and a card, each with a rail of its own (`carrier_volts` and `card_volts`), the
/// grounds mated on contact 1 and the line on contact 3: `carrier` is what the carrier has on
/// `.line` (its net at `.slot.p3`), `card` what the card has on `.line` (at `.j1.p3`).
fn kit(carrier_volts: &str, carrier: &str, card_volts: &str, card: &str) -> String {
    format!(
        r#"
board Carrier {{
    .vcc: Rail {{
        .voltage &= {carrier_volts}
        .pos.net &= "VCC_A"
        .neg.net &= "GND"
    }}
    .slot: Socket1x4
    .vcc.neg ~ .slot.p1
{carrier}}}

board Card {{
    .vcc: Rail {{
        .voltage &= {card_volts}
        .pos.net &= "VCC_B"
        .neg.net &= "GND"
    }}
    .j1: Header1x4
    .vcc.neg ~ .j1.p1
{card}}}

system Kit {{
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}}
"#
    )
}

const MATED: &str = "the nets `net-slot.p3` of `carrier` and `net-j1.p3` of `card`, mated at `carrier.slot` pin 3 against `card.j1` pin 3";

#[test]
fn the_dc_analysis_solves_an_open_drain_line_across_the_mate() {
    // The card pulls the line up to 3.3 V through 4.7 kohm and lets go of it with an
    // open-drain pin, the carrier pulls it down through 10 kohm: a divider of two boards, at
    // least 3.3 V x 0.95 x 9.9 / (9.9 + 4.747) = 2.12 V, under the receiver's 70 % of 3.3 V.
    // The pull-up alone cannot judge it (a resistor to ground is on the line too); the analysis
    // of both boards does.
    let carrier = "    .u1: Receiver\n    .u1.vdd ~ .vcc\n    .down: Resistor { .resistance &= 10kohm +/- 1% }\n    .u1.in.line ~> .down ~> .vcc.neg\n    .u1.in.line ~ .slot.p3\n";
    let card = "    .u2: Drain\n    .u2.vdd ~ .vcc\n    .up: Resistor { .resistance &= 4.7kohm +/- 1% }\n    .vcc.pos ~> .up ~> .u2.io.line\n    .u2.io.line ~ .j1.p3\n";
    let s = run(&kit("3.3V +/- 5%", carrier, "3.3V +/- 5%", card));
    let m = messages(&s);
    let fault = m
        .iter()
        .find(|m| m.contains("carrier.u1.in.vih"))
        .unwrap_or_else(|| panic!("a fault of the high across the mate: {m:#?}\n{:#?}", notes(&s)));
    // Named from the driver's board, as every pair across a mate is.
    assert!(
        fault.starts_with(
            "the nets `net-j1.p3` of `card` and `net-down.a` of `carrier`, mated at `card.j1` pin 3 against `carrier.slot` pin 3: the DC analysis holds the line at 2.118V"
        ),
        "the analysis's high: {fault}"
    );
    assert!(
        !notes(&s)
            .iter()
            .any(|n| n.contains("carrier.u1.in") && n.contains("high level")),
        "{:#?}",
        notes(&s)
    );
    // With no pull-down on the carrier the line goes to the card's rail: a high.
    let carrier = "    .u1: Receiver\n    .u1.vdd ~ .vcc\n    .u1.in.line ~ .slot.p3\n";
    let s = run(&kit("3.3V +/- 5%", carrier, "3.3V +/- 5%", card));
    assert!(
        !messages(&s).iter().any(|m| m.contains("carrier.u1.in.vih")),
        "{:#?}",
        messages(&s)
    );
}

#[test]
fn a_plain_input_across_the_mate_is_named() {
    let carrier = "    .u1: Driver\n    .u1.vdd ~ .vcc\n    .u1.out.line ~ .slot.p3\n";
    let card = "    .q: PlainIn\n    .q.vdd ~ .vcc\n    .q.en ~ .j1.p3\n";
    let s = run(&kit("3.3V +/- 5%", carrier, "3.3V +/- 5%", card));
    let want = format!(
        "not evaluated: the logic levels from `carrier.u1.out` to `card.q.en` on {MATED}: `card.q.en` reads it and is not the `line` of a `Logic` or a pin of an interface or a part with `levels`, so the levels it needs are not known"
    );
    assert!(notes(&s).contains(&want), "{:#?}", notes(&s));
}

#[test]
fn a_level_through_a_series_resistor_on_the_far_board_is_judged_across_the_mate() {
    // The carrier's 3.3 V driver into a 5 V receiver on the card through 100 ohm on the card:
    // nothing draws from the receiver's net, so it sees the driver's own high, 70 % of 3.3 V,
    // under 70 % of 5 V.
    let carrier = "    .u1: Driver\n    .u1.vdd ~ .vcc\n    .u1.out.line ~ .slot.p3\n";
    let card = "    .u2: Receiver\n    .u2.vdd ~ .vcc\n    .r: Resistor { .resistance &= 100ohm +/- 1% }\n    .j1.p3 ~> .r ~> .u2.in.line\n";
    let s = run(&kit("3.3V +/- 5%", carrier, "5V +/- 5%", card));
    let m = messages(&s);
    let fault = m
        .iter()
        .find(|m| m.contains("card.u2.in.vih"))
        .unwrap_or_else(|| panic!("the high through the resistor: {m:#?}\n{:#?}", notes(&s)));
    assert!(
        fault.starts_with(&format!("{MATED}: `carrier.u1.out.voh`")),
        "{fault}"
    );
    assert!(
        !notes(&s).iter().any(|n| n.contains("comes through")),
        "{:#?}",
        notes(&s)
    );
    // A 5 V driver into the 3.3 V receiver: the high is read as one, and nothing is left
    // unjudged.
    let s = run(&kit("5V +/- 5%", carrier, "3.3V +/- 5%", card));
    assert!(
        !messages(&s).iter().any(|m| m.contains("card.u2.in")),
        "{:#?}",
        messages(&s)
    );
    assert!(
        !notes(&s).iter().any(|n| n.contains("card.u2.in")),
        "{:#?}",
        notes(&s)
    );
    // The resistor on the carrier, between its driver and the socket (issue #1440): the mate
    // check lets it through and the levels check judges the same high across the mate.
    let carrier = "    .u1: Driver\n    .u1.vdd ~ .vcc\n    .r: Resistor { .resistance &= 100ohm +/- 1% }\n    .u1.out.line ~> .r ~> .slot.p3\n";
    let card = "    .u2: Receiver\n    .u2.vdd ~ .vcc\n    .u2.in.line ~ .j1.p3\n";
    let s = run(&kit("3.3V +/- 5%", carrier, "5V +/- 5%", card));
    let m = messages(&s);
    assert!(
        m.iter().any(|m| m.contains("card.u2.in.vih") && m.contains("`carrier.u1.out.voh`")),
        "the high through the carrier's resistor: {m:#?}\n{:#?}",
        notes(&s)
    );
    let s = run(&kit("5V +/- 5%", carrier, "3.3V +/- 5%", card));
    assert!(
        !messages(&s).iter().any(|m| m.contains("card.u2.in")),
        "{:#?}",
        messages(&s)
    );
    assert!(
        !notes(&s).iter().any(|n| n.contains("card.u2.in")),
        "{:#?}",
        notes(&s)
    );
}

#[test]
fn a_divider_on_the_card_from_a_driver_on_the_carrier_is_judged_at_its_levels() {
    // 10 kohm into 10 kohm to the card's ground: the high is about half the driver's.
    let carrier = "    .u1: Driver\n    .u1.vdd ~ .vcc\n    .u1.out.line ~ .slot.p3\n";
    let card = "    .u2: Receiver\n    .u2.vdd ~ .vcc\n    .r: Resistor { .resistance &= 10kohm +/- 1% }\n    .p: Resistor { .resistance &= 10kohm +/- 1% }\n    .j1.p3 ~> .r ~> .u2.in.line\n    .u2.in.line ~> .p ~> .vcc.neg\n";
    let s = run(&kit("3.3V +/- 5%", carrier, "3.3V +/- 5%", card));
    let m = messages(&s);
    let fault = m
        .iter()
        .find(|m| m.contains("card.u2.in.vih"))
        .unwrap_or_else(|| panic!("the divider's high: {m:#?}\n{:#?}", notes(&s)));
    assert!(
        fault.contains(&format!(
            ": with `carrier.u1.out` driving high, the divider of `card.r` from {MATED} and `card.p` to ground `GND` holds the line at "
        )),
        "{fault}"
    );
}
