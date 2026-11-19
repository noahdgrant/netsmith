//! Logic levels across a mate (issue #1323): a driver on one board against a receiver on the
//! other, the nets paired by the mate's contacts, each part's levels from the supply of its own
//! board, and a fault that names both ends.
//!
//! `Driver`, `Receiver`, `Opener` and `Pad` are invented for these tests: parts with a supply
//! and one `Logic` line whose levels are what a datasheet might say. None is a real part.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve_system};

const PARTS: &str = r#"
import std::logic::Logic
import std::parts::connector::{Header1x4, Socket1x4}
import std::parts::passive::Resistor
import std::sources::Rail

/// Drives a line to 70 % of its supply and to 0.1 V at the bottom.
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

/// Pulls a line low and leaves its high to a pull-up.
part Opener {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.vol &= 0.2V
    pins {
        .vdd.pos   power      "VDD" "1"
        .vdd.neg   power      "GND" "2"
        .out.line  open_drain "OUT" "3"
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

/// Reads a high from 2 V and a low up to 0.8 V.
part Pad {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 2V
    .in.vil &= 0.8V
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .in.line   input "IN"  "3"
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

/// A carrier whose driver is on contact 3 of its socket, and a card whose receiver (`part`)
/// is on contact 3 of its header. Contact 1 is ground on both. The card's supply is `card`:
/// with `joined` it is the carrier's rail, carried on contact 2, and otherwise a rail of its
/// own at that voltage.
fn kit(card: &str, part: &str, joined: bool) -> String {
    let (card_rail, power) = if joined {
        (String::new(), "    .vcc.pos ~ .j1.p2\n")
    } else {
        (format!("        .voltage &= {card}\n"), "")
    };
    let carrier_power = if joined {
        "    .vcc.pos ~ .slot.p2\n"
    } else {
        ""
    };
    format!(
        r#"
board Carrier {{
    .vcc: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }}
    .u1: Driver
    .u1.vdd ~ .vcc
    .slot: Socket1x4
    .vcc.neg ~ .slot.p1
{carrier_power}    .u1.out.line ~ .slot.p3
}}

board Card {{
    .vcc: Rail {{
{card_rail}        .pos.net &= "VCC"
        .neg.net &= "GND"
    }}
    .u2: {part}
    .u2.vdd ~ .vcc
    .j1: Header1x4
    .vcc.neg ~ .j1.p1
{power}    .u2.in.line ~ .j1.p3
}}

system Kit {{
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}}
"#
    )
}

#[test]
fn a_driver_on_one_board_against_a_receiver_on_another_is_judged_and_names_both_ends() {
    // 70 % of 3.3 V is at most 2.43 V; 70 % of 5 V is at least 3.33 V.
    let s = run(&kit("5V +/- 5%", "Receiver", false));
    let m = messages(&s);
    let fault = m
        .iter()
        .find(|m| m.contains("card.u2.in.vih"))
        .unwrap_or_else(|| panic!("a fault of the high across the mate: {m:#?}"));
    assert!(
        fault.starts_with(
            "the nets `net-slot.p3` of `carrier` and `net-j1.p3` of `card`, mated at `carrier.slot` pin 3 against `card.j1` pin 3: `carrier.u1.out.voh`"
        ),
        "{fault}"
    );
    assert!(fault.contains("is not always read as a high"), "{fault}");
    let d = s
        .diagnostics
        .iter()
        .find(|d| d.message == *fault)
        .expect("the fault");
    let src = format!("{PARTS}\n{}", kit("5V +/- 5%", "Receiver", false));
    assert_eq!(
        &src[d.span.start..d.span.end],
        ".carrier.slot.mating ~ .card.j1.mating",
        "the fault is at the mate"
    );
}

#[test]
fn a_receiver_whose_supply_the_driver_meets_is_not_a_fault() {
    // 70 % of 3.3 V +/- 5 % is at least 2.19 V; 2 V is a high.
    let s = run(&kit("3.3V +/- 5%", "Pad", false));
    let m = messages(&s);
    assert!(
        !m.iter().any(|m| m.contains("mated at")),
        "no level fault across the mate: {m:#?}"
    );
    assert!(
        !notes(&s).iter().any(|n| n.contains("mated at")),
        "{:#?}",
        notes(&s)
    );
}

#[test]
fn fractions_of_one_supply_that_the_mate_carries_are_compared_at_one_voltage() {
    // The card is powered from the carrier through contact 2: 0.7 x the supply against
    // 0.7 x the same supply holds at every voltage.
    let s = run(&kit("", "Receiver", true));
    let m = messages(&s);
    assert!(
        !m.iter().any(|m| m.contains("mated at")),
        "one supply across the mate: {m:#?}"
    );
    // Two rails of their own at the same voltage are two voltages: compared as two ranges.
    let s = run(&kit("3.3V +/- 5%", "Receiver", false));
    let m = messages(&s);
    assert!(
        m.iter()
            .any(|m| m.contains("mated at") && m.contains("card.u2.in.vih")),
        "two supplies across the mate: {m:#?}"
    );
}

#[test]
fn an_open_drain_line_takes_its_high_from_its_pull_up_across_the_mate() {
    // The card pulls its open-drain line up to 1.8 V; the carrier's receiver needs 2 V.
    let src = r#"
board Carrier {
    .vcc: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u1: Pad
    .u1.vdd ~ .vcc
    .slot: Socket1x4
    .vcc.neg ~ .slot.p1
    .u1.in.line ~ .slot.p3
}

board Card {
    .vcc: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
        .neg.net &= "GND"
    }
    .u2: Opener
    .u2.vdd ~ .vcc
    .r1: Resistor { .resistance &= 4.7kohm +/- 1% }
    .j1: Header1x4
    .vcc.neg ~ .j1.p1
    .vcc.pos ~> .r1 ~> .u2.out.line
    .u2.out.line ~ .j1.p3
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}
"#;
    let s = run(src);
    let m = messages(&s);
    let fault = m
        .iter()
        .find(|m| m.contains("mated at") && m.contains("pull-up `card.r1`"))
        .unwrap_or_else(|| panic!("the pull-up's high across the mate: {m:#?}"));
    assert!(
        fault.contains("`carrier.u1.in.vih` needs 2V"),
        "the receiver on the carrier: {fault}"
    );
}

#[test]
fn a_line_through_a_carrier_to_a_second_card_names_every_contact() {
    let src = r#"
board Carrier {
    .vcc: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    // The carrier has nothing of its own on the line: it only passes it from one card to the
    // other (issue #1426).
    .slot_a: Socket1x4
    .slot_b: Socket1x4
    .vcc.neg ~ .slot_a.p1
    .vcc.neg ~ .slot_b.p1
    .slot_a.p3 ~ .slot_b.p3
}

board Talker {
    .vcc: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u1: Driver
    .u1.vdd ~ .vcc
    .j1: Header1x4
    .vcc.neg ~ .j1.p1
    .u1.out.line ~ .j1.p3
}

board Listener {
    .vcc: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "5V"
        .neg.net &= "GND"
    }
    .u2: Receiver
    .u2.vdd ~ .vcc
    .j1: Header1x4
    .vcc.neg ~ .j1.p1
    .u2.in.line ~ .j1.p3
}

system Kit {
    .carrier: Carrier
    .talker: Talker
    .listener: Listener
    .carrier.slot_a.mating ~ .talker.j1.mating
    .carrier.slot_b.mating ~ .listener.j1.mating
}
"#;
    let s = run(src);
    let m = messages(&s);
    let fault = m
        .iter()
        .find(|m| m.contains("talker.u1.out.voh") && m.contains("listener.u2.in.vih"))
        .unwrap_or_else(|| panic!("a fault through the carrier: {m:#?}"));
    assert!(
        fault.starts_with(
            "the nets `net-j1.p3` of `talker`, `net-slot_a.p3` of `carrier` and `net-j1.p3` of `listener`, mated at `talker.j1` pin 3 against `carrier.slot_a` pin 3 and `carrier.slot_b` pin 3 against `listener.j1` pin 3:"
        ),
        "{fault}"
    );
}
