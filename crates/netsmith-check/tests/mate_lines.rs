//! A line through several mates (issue #1426): a carrier with no part of its own on a net, which
//! only passes the line from one card's mate to another's, is judged as one line, so the driver
//! may be on any board the line reaches.
//!
//! `Talker`, `Listener` and `Opener` are invented for these tests. None is a real part.

use netsmith_check::*;

const PARTS: &str = r#"
import std::parts::connector::{Header1x4, Socket1x4}
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

/// Pulls one line low.
part Opener {
    .vdd: Rail
    .out: Logic
    pins {
        .vdd.pos   power      "VDD" "1"
        .vdd.neg   power      "GND" "2"
        .out.line  open_drain "OUT" "3"
    }
}

/// Passes contact 3 of one socket to contact 3 of the other, with nothing of its own on it.
board Carrier {
    .vcc: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .slot_a: Socket1x4
    .slot_b: Socket1x4
    .vcc.neg ~ .slot_a.p1
    .vcc.neg ~ .slot_b.p1
    .slot_a.p3 ~ .slot_b.p3
}
"#;

/// A card whose part `part` (one of the parts above) has its line on contact 3 of its header.
fn card(name: &str, part: &str, line: &str) -> String {
    format!(
        r#"
board {name} {{
    .vcc: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }}
    .u1: {part}
    .u1.vdd ~ .vcc
    .j1: Header1x4
    .vcc.neg ~ .j1.p1
    .u1.{line} ~ .j1.p3
}}
"#
    )
}

/// The carrier with card `A` in one socket and card `B` in the other.
fn kit(a: (&str, &str), b: (&str, &str)) -> Checked {
    let src = format!(
        r#"{PARTS}
{}
{}
system Kit {{
    .carrier: Carrier
    .first: A
    .second: B
    .carrier.slot_a.mating ~ .first.j1.mating
    .carrier.slot_b.mating ~ .second.j1.mating
}}
"#,
        card("A", a.0, a.1),
        card("B", b.0, b.1)
    );
    check("main.nsm", &src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_line_the_carrier_only_passes_is_driven_by_the_card_that_drives_it() {
    let c = kit(("Talker", "out.line"), ("Listener", "in.line"));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_line_no_board_drives_is_refused_and_says_it_looked_past_the_mate() {
    let c = kit(("Listener", "in.line"), ("Listener", "in.line"));
    let m = messages(&c);
    assert_eq!(m.len(), 2, "one for each mate's contact 3: {m:#?}");
    for message in &m {
        assert!(
            message.starts_with("contact 3 of the mated connectors has an input and nothing that drives it on any board the line reaches through the mates"),
            "{message}"
        );
    }
}

#[test]
fn two_outputs_that_only_the_carrier_joins_are_refused_once() {
    let c = kit(("Talker", "out.line"), ("Talker", "out.line"));
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert_eq!(
        m[0],
        "the mates join the nets `net-slot_a.p3` of `carrier`, `net-j1.p3` of `first` and `net-j1.p3` of `second` into one line, which is an output on two boards, and two outputs cannot share a net: `first.u1.out.line`, `second.u1.out.line`"
    );
}

#[test]
fn a_push_pull_output_and_an_open_drain_one_through_the_carrier_are_refused() {
    // The open-drain card has no pull-up, which its own board refuses as well.
    let c = kit(("Talker", "out.line"), ("Opener", "out.line"));
    let m = messages(&c);
    let line = m
        .iter()
        .filter(|m| m.starts_with("the mates join"))
        .collect::<Vec<_>>();
    assert_eq!(line.len(), 1, "{m:#?}");
    assert!(
        line[0].contains("is a push-pull output on one board and an open_drain output on another"),
        "{}",
        line[0]
    );
}
