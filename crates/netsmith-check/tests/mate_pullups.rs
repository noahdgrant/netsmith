//! The pull-up of an open-drain line in a system (issue #1436): a resistor to a rail on any
//! board the line reaches through the mates takes it high, so the driver's own board needs
//! none of its own.
//!
//! `Opener` and `Listener` are invented for these tests. Neither is a real part.

use netsmith_check::*;

const PARTS: &str = r#"
import std::parts::connector::{Header1x4, Socket1x4}
import std::parts::passive::Resistor
import std::logic::Logic
import std::sources::Rail

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

/// A card whose open-drain driver is on contact 3 of its header, with no pull-up.
board Card {
    .vcc: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u1: Opener
    .u1.vdd ~ .vcc
    .j1: Header1x4
    .vcc.neg ~ .j1.p1
    .u1.out.line ~ .j1.p3
}
"#;

/// A carrier with a listener on contact 3 of each socket in `sockets`, contact 3 of every
/// socket joined, and `pull_up` lines of its own; the card in its first socket and, with
/// `second`, a listener card in the second.
fn kit(pull_up: &str, second: bool) -> Checked {
    let (slot_b, mate_b, board_b) = if second {
        (
            "    .slot_b: Socket1x4\n    .vcc.neg ~ .slot_b.p1\n    .slot_a.p3 ~ .slot_b.p3\n",
            "    .second: Reader\n    .carrier.slot_b.mating ~ .second.j1.mating\n",
            r#"
board Reader {
    .vcc: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u1: Listener
    .u1.vdd ~ .vcc
    .j1: Header1x4
    .vcc.neg ~ .j1.p1
    .u1.in.line ~ .j1.p3
}
"#,
        )
    } else {
        ("", "", "")
    };
    let src = format!(
        r#"{PARTS}
{board_b}
board Carrier {{
    .vcc: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }}
    .u1: Listener
    .u1.vdd ~ .vcc
    .slot_a: Socket1x4
    .vcc.neg ~ .slot_a.p1
    .u1.in.line ~ .slot_a.p3
{slot_b}{pull_up}}}

system Kit {{
    .carrier: Carrier
    .first: Card
    .carrier.slot_a.mating ~ .first.j1.mating
{mate_b}}}
"#
    );
    check("main.nsm", &src, &Loader::std())
}

const PULL_UP: &str = "    .r1: Resistor { .resistance &= 4.7kohm +/- 1% }\n    .vcc.pos ~> .r1 ~> .u1.in.line\n";

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_pull_up_on_the_carrier_takes_the_card_s_open_drain_line_high() {
    let c = kit(PULL_UP, false);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_pull_up_two_mates_away_counts_too() {
    // The line goes from the card through the carrier to the second card; the pull-up is on
    // the carrier, which the card reaches through one mate.
    let c = kit(PULL_UP, true);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn an_open_drain_line_no_board_pulls_up_names_the_nets_it_looked_at() {
    let c = kit("", true);
    let m = messages(&c);
    let missing: Vec<&String> = m.iter().filter(|m| m.contains("no pull-up")).collect();
    assert_eq!(missing.len(), 1, "{m:#?}");
    assert_eq!(
        missing[0],
        "net `net-j1.p3` has an open_drain output (`first.u1.out.line`) and no pull-up, on this board or on the boards its mated connectors lead to (the nets `net-slot_a.p3` of `carrier`, `net-j1.p3` of `second`): nothing takes the line high when it lets go of it; join a resistor from the line to a rail's `pos` on one of the boards, or to a pin that drives the line high"
    );
}

#[test]
fn a_card_alone_behind_its_connector_is_not_judged() {
    // Nothing says where the header leads, so the pull-up may be there (ADR 0090).
    let c = check("main.nsm", PARTS, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn an_open_drain_line_on_no_connector_still_needs_a_pull_up() {
    let src = PARTS.replace(
        "    .u1.out.line ~ .j1.p3\n",
        "    .u2: Listener\n    .u2.vdd ~ .vcc\n    .u1.out.line ~ .u2.in.line\n",
    );
    let c = check("main.nsm", &src, &Loader::std());
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.contains("and no pull-up: nothing")),
        "{m:#?}"
    );
}
