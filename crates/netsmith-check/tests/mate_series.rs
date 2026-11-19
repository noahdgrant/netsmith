//! An input across a mate whose driver is behind a series part on the other board (issue
//! #1440): the carrier's driver reaches its socket's contact through a resistor in a `~>`
//! chain, and the card reads the line on the matching header contact. The mate check follows
//! the part's `bridge` to the driver, as the net rules of one board do, and leaves the levels
//! to the solver.
//!
//! `Talker` and `Listener` are invented for these tests. Neither is a real part.

use netsmith_check::*;

const PARTS: &str = r#"
import std::parts::connector::{Header1x4, Socket1x4}
import std::parts::passive::Resistor
import std::logic::Logic
import std::sources::Rail

/// Drives one line.
part Talker {
    .vdd: Rail
    .out: Logic
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2"
        .out.line  output "OUT" "3"
    }
}

/// Reads one line.
part Listener {
    .vdd: Rail
    .in: Logic
    pins {
        .vdd.pos   power "VDD" "1"
        .vdd.neg   power "GND" "2"
        .in.line   input "IN"  "3"
    }
}

/// Reads contact 3 of its header.
board Card {
    .vcc: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u2: Listener
    .u2.vdd ~ .vcc
    .j1: Header1x4
    .vcc.neg ~ .j1.p1
    .u2.in.line ~ .j1.p3
}
"#;

/// A carrier whose `carrier` lines put something behind contact 3 of its socket, mated to the
/// card.
fn kit(carrier: &str) -> Checked {
    let src = format!(
        r#"{PARTS}
board Carrier {{
    .vcc: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }}
    .slot: Socket1x4
    .vcc.neg ~ .slot.p1
    .r: Resistor {{ .resistance &= 100ohm +/- 1% }}
{carrier}}}

system Kit {{
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}}
"#
    );
    check("main.nsm", &src, &Loader::std())
}

#[test]
fn a_driver_behind_a_series_resistor_on_the_far_board_drives_the_input() {
    let c = kit("    .u1: Talker\n    .u1.vdd ~ .vcc\n    .u1.out.line ~> .r ~> .slot.p3\n");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_pull_resistor_to_a_rail_on_the_far_board_drives_the_input() {
    let c = kit("    .vcc.pos ~> .r ~> .slot.p3\n");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_series_resistor_to_nothing_that_drives_leaves_the_input_undriven() {
    let c = kit(
        "    .u1: Listener\n    .u1.vdd ~ .vcc\n    .u1.in.line ~> .r ~> .slot.p3\n",
    );
    let m: Vec<String> = c.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert!(
        m.iter().any(|m| m.starts_with(
            "contact 3 of the mated connectors has an input and nothing that drives it on either board"
        )),
        "{m:#?}"
    );
}
