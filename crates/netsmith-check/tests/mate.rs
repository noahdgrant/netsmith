//! Connectors mated in a system (ADR 0040): the checks of a mate, what it derives, the grounds
//! it joins, and the port joins it refuses.

use netsmith_check::*;

const EXAMPLE: &str = include_str!("../../../examples/carrier_and_cards.nsm");

fn run(src: &str) -> Checked {
    check("mate.nsm", src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The example with one piece of it changed.
fn changed(from: &str, to: &str) -> String {
    assert!(EXAMPLE.contains(from), "`{from}` is not in the example");
    EXAMPLE.replacen(from, to, 1)
}

/// The one error `src` has, which must mention each needle.
fn refused(src: &str, needles: &[&str]) {
    let c = run(src);
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 1, "expected one error: {msgs:#?}");
    for needle in needles {
        assert!(
            msgs[0].contains(needle),
            "`{needle}` is not in: {}",
            msgs[0]
        );
    }
}

fn system(c: &Checked) -> &System {
    c.system("CarrierAndCards").expect("the system")
}

#[test]
fn the_example_mates_two_cards_and_reads_every_contact() {
    let c = run(EXAMPLE);
    assert!(c.is_ok(), "{}", c.render());
    let m = &system(&c).model;
    assert_eq!(m.mates().len(), 2);
    let role = |mate: usize, n: usize| {
        let c = &m.mates()[mate].contacts[n - 1];
        (c.a.role, c.b.role)
    };
    // The first socket carries the SPI bus and power; contacts 7 and 8 are open on both sides.
    assert_eq!(role(0, 1), (Role::Ground, Role::Ground));
    assert_eq!(role(0, 2), (Role::Power, Role::Power));
    assert_eq!(role(0, 3), (Role::Signal, Role::Signal));
    assert_eq!(role(0, 7), (Role::Open, Role::Open));
    assert_eq!(role(0, 9), (Role::Power, Role::Power));
    assert_eq!(role(0, 10), (Role::Ground, Role::Ground));
    // The second carries the UART.
    assert_eq!(role(1, 7), (Role::Signal, Role::Signal));
    assert_eq!(role(1, 3), (Role::Open, Role::Open));
    // The nets are each board's own.
    let contact = &m.mates()[0].contacts[0];
    assert_eq!(contact.a.net.as_deref(), Some("GND"));
    assert_eq!(contact.b.net.as_deref(), Some("GND"));
    assert_eq!(m.mates()[0].contacts[1].b.net.as_deref(), Some("3V3_CARD"));
}

#[test]
fn a_mate_derives_the_join_of_the_ports_it_carries() {
    let c = run(EXAMPLE);
    let m = &system(&c).model;
    let joins: Vec<String> = m
        .joins()
        .iter()
        .map(|&(a, b)| format!("{} ~ {}", m.path_of(a), m.path_of(b)))
        .collect();
    // The carrier's `supply` is on another net (a fuse is between it and `cards`), so it is
    // not carried by the mate and is not joined.
    assert_eq!(
        joins,
        [
            "carrier.cards ~ sensor.supply",
            "carrier.cards ~ serial.supply"
        ]
    );
    assert!(!m.equalities().is_empty(), "the voltages are equated");
    // Joined ports are balanced in the system, as an explicit join would.
    assert!(
        m.balances().iter().any(|b| b.net.contains("carrier.cards")
            && b.net.contains("sensor.supply")
            && b.net.contains("serial.supply")),
        "{:#?}",
        m.balances().iter().map(|b| &b.net).collect::<Vec<_>>()
    );
    for mate in m.mates() {
        assert_eq!(mate.ports.len(), 1);
    }
}

#[test]
fn each_board_keeps_its_ground_and_the_system_reports_one_ground_group() {
    let c = run(EXAMPLE);
    let m = &system(&c).model;
    // Three boards, three `GND` nets, none of them merged.
    let gnd = m.nets().iter().filter(|n| n.name == "GND").count();
    assert_eq!(gnd, 3);
    let groups = m.ground_groups();
    assert_eq!(groups.len(), 1, "{groups:#?}");
    let nets: Vec<String> = groups[0]
        .nets
        .iter()
        .map(|n| format!("{}.{}", m.path_of(n.board), n.net))
        .collect();
    assert_eq!(nets, ["carrier.GND", "sensor.GND", "serial.GND"]);
    // Each mate puts its two ground contacts (1 and 10) in the group.
    assert_eq!(groups[0].contacts, [(0, 2), (1, 2)]);
}

#[test]
fn a_header_goes_with_a_socket_and_never_with_a_header() {
    refused(
        &changed(".slot_a: Socket1x10", ".slot_a: Header1x10"),
        &[
            "`.carrier.slot_a` and `.sensor.j1` are both headers",
            "a header mates with a socket",
        ],
    );
    let two_sockets = changed(
        ".j1: Header1x10\n    .supply.neg",
        ".j1: Socket1x10\n    .supply.neg",
    );
    refused(
        &two_sockets,
        &[
            "`.carrier.slot_a` and `.sensor.j1` are both sockets",
            "a socket mates with a header",
        ],
    );
}

#[test]
fn the_two_connectors_need_as_many_positions_as_each_other() {
    let eight = changed(
        ".j1: Header1x10\n    .supply.neg",
        ".j1: Header1x8\n    .supply.neg",
    )
    .replace(
        "    .supply.pos ~ .j1.p9\n    .supply.neg ~ .j1.p10\n}\n\n/// A card with a UART",
        "}\n\n/// A card with a UART",
    )
    .replace(
        "import std::parts::connector::{Header1x10, Socket1x10}",
        "import std::parts::connector::{Header1x10, Header1x8, Socket1x10}",
    );
    refused(
        &eight,
        &[
            "`.carrier.slot_a` (Socket1x10) has 10 positions and `.sensor.j1` (Header1x8) has 8",
            "contact N to contact N",
        ],
    );
}

const FINE_HEADER: &str = "
import std::parts::connector::Mating

/// A header at 2 mm, as an invented part.
part Header1x10Fine {
    pins {
        .p1 passive \"P1\" \"1\"
        .p2 passive \"P2\" \"2\"
        .p3 passive \"P3\" \"3\"
        .p4 passive \"P4\" \"4\"
        .p5 passive \"P5\" \"5\"
        .p6 passive \"P6\" \"6\"
        .p7 passive \"P7\" \"7\"
        .p8 passive \"P8\" \"8\"
        .p9 passive \"P9\" \"9\"
        .p10 passive \"P10\" \"10\"
    }
    .mating: Mating {
        .kind &= \"pin_header\"
        .gender &= \"header\"
        .pitch &= 2mm
        .rows &= 1
        .positions &= 10
    }
}
";

#[test]
fn the_pitch_must_be_the_same() {
    let src = changed(
        ".j1: Header1x10\n    .supply.neg",
        ".j1: Header1x10Fine\n    .supply.neg",
    ) + FINE_HEADER;
    refused(
        &src,
        &[
            "`.carrier.slot_a` (Socket1x10) has a pitch of 2.54mm and `.sensor.j1` (Header1x10Fine) has 2mm",
            "do not fit together",
        ],
    );
}

#[test]
fn a_connector_that_states_nothing_of_its_mating_cannot_be_checked() {
    let src = EXAMPLE.replacen(".j1: Header1x10", ".j1: Header1x10Silent", 1)
        + "
part Header1x10Silent {
    pins {
        .p1 passive \"P1\" \"1\"
        .p2 passive \"P2\" \"2\"
        .p3 passive \"P3\" \"3\"
        .p4 passive \"P4\" \"4\"
        .p5 passive \"P5\" \"5\"
        .p6 passive \"P6\" \"6\"
        .p7 passive \"P7\" \"7\"
        .p8 passive \"P8\" \"8\"
        .p9 passive \"P9\" \"9\"
        .p10 passive \"P10\" \"10\"
    }
    .mating: std::parts::connector::Mating
}
";
    // `Mating` has to be imported by name, so the qualified form is refused first; use the
    // import instead.
    let src = src.replace(".mating: std::parts::connector::Mating", ".mating: Mating") + "\n";
    let src = src.replacen(
        "import std::sources::Rail",
        "import std::sources::Rail\nimport std::parts::connector::Mating",
        1,
    );
    let c = run(&src);
    let msgs = messages(&c);
    assert!(
        msgs.iter()
            .any(|m| m.contains("does not state its `.mating.kind`")),
        "{msgs:#?}"
    );
}

#[test]
fn two_outputs_on_one_contact_and_an_input_nobody_drives_are_refused() {
    // The serial card's `tx` on contact 7, where the controller's `tx` is: two outputs. Its
    // `rx` on 8, where the controller has its `rx`: an input with nothing to drive it.
    let src = changed(
        "    .chip.uart.rx ~ .j1.p7\n    .chip.uart.tx ~ .j1.p8",
        "    .chip.uart.tx ~ .j1.p7\n    .chip.uart.rx ~ .j1.p8",
    );
    let c = run(&src);
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 2, "{msgs:#?}");
    let two_outputs = msgs
        .iter()
        .find(|m| m.contains("contact 7"))
        .expect("contact 7");
    for needle in [
        "is an output on both boards",
        "`.carrier.slot_b` pin 7 is on net `net-mcu.uart.tx`",
        "(output `carrier.mcu.uart.tx`)",
        "`.serial.j1` pin 7",
        "(output `serial.chip.uart.tx`)",
        "its pin 7 would meet pin 4",
    ] {
        assert!(
            two_outputs.contains(needle),
            "`{needle}` is not in: {two_outputs}"
        );
    }
    let undriven = msgs
        .iter()
        .find(|m| m.contains("contact 8"))
        .expect("contact 8");
    assert!(
        undriven.contains("has an input and nothing that drives it on either board")
            && undriven.contains("(input `carrier.mcu.uart.rx`)")
            && undriven.contains("(input `serial.chip.uart.rx`)"),
        "{undriven}"
    );
}

#[test]
fn an_output_on_one_board_and_a_tri_state_output_on_the_other_are_refused() {
    // The serial card's `tx` on contact 7, where the controller's `tx` is, now `tri_state`: a
    // line that several parts drive needs every driver `tri_state`.
    let src = changed(
        "    .chip.uart.rx ~ .j1.p7\n    .chip.uart.tx ~ .j1.p8",
        "    .chip.uart.tx ~ .j1.p7\n    .chip.uart.rx ~ .j1.p8",
    );
    let src = src.replacen(
        ".uart.tx  output \"TX\"   \"3\"",
        ".uart.tx  tri_state \"TX\" \"3\"",
        1,
    );
    let c = run(&src);
    let msgs = messages(&c);
    let seven = msgs
        .iter()
        .find(|m| m.contains("contact 7"))
        .unwrap_or_else(|| panic!("{msgs:#?}"));
    assert!(
        seven.contains("is an output on one board and a tri_state output on the other")
            && seven.contains("(tri_state `carrier.mcu.uart.tx`)")
            && seven.contains("(output `serial.chip.uart.tx`)"),
        "{seven}"
    );
}

#[test]
fn an_open_drain_output_on_one_board_and_a_push_pull_output_on_the_other_are_refused() {
    // The controller's `tx` is open drain, and the serial card's `tx` on contact 7 drives the
    // line high (#460).
    let src = changed(
        "    .chip.uart.rx ~ .j1.p7\n    .chip.uart.tx ~ .j1.p8",
        "    .chip.uart.tx ~ .j1.p7\n    .chip.uart.rx ~ .j1.p8",
    );
    let src = src.replacen(
        ".uart.tx  output \"TX\"   \"3\"",
        ".uart.tx  open_drain \"TX\" \"3\"",
        1,
    );
    let c = run(&src);
    let msgs = messages(&c);
    let seven = msgs
        .iter()
        .find(|m| m.contains("contact 7"))
        .unwrap_or_else(|| panic!("{msgs:#?}"));
    assert!(
        seven.contains("is a push-pull output on one board and an open_drain output on the other")
            && seven.contains("(open_drain `carrier.mcu.uart.tx`)")
            && seven.contains("(output `serial.chip.uart.tx`)"),
        "{seven}"
    );
}

#[test]
fn ground_against_power_or_a_signal_is_a_short() {
    // Swap the ground and the supply on the sensor card's header: contact 1 is ground on the
    // carrier and power on the card.
    let src = changed(
        "    .supply.neg ~ .j1.p1\n    .supply.pos ~ .j1.p2\n    .sensor.spi.sclk",
        "    .supply.pos ~ .j1.p1\n    .supply.neg ~ .j1.p2\n    .sensor.spi.sclk",
    );
    let c = run(&src);
    let msgs = messages(&c);
    let first = msgs
        .iter()
        .find(|m| m.contains("contact 1 of the mated connectors"))
        .expect("contact 1");
    assert!(
        first.contains("is ground on one board and power on the other")
            && first.contains("`.carrier.slot_a` pin 1 is on net `GND`, ground")
            && first.contains("`.sensor.j1` pin 1 is on net `3V3_CARD`, power")
            && first.contains("its pin 1 would meet pin 10"),
        "{first}"
    );
    // A signal against power.
    let src = changed(
        "    .sensor.spi.sclk ~ .j1.p3\n",
        "    .supply.pos ~ .j1.p3\n",
    );
    let c = run(&src);
    assert!(
        messages(&c).iter().any(|m| m.contains("contact 3")
            && m.contains("is a signal on one board and power on the other")),
        "{:#?}",
        messages(&c)
    );
}

#[test]
fn a_mate_that_carries_power_must_carry_a_ground() {
    // The sensor card leaves both ground contacts off its header.
    let src = changed(
        "    .supply.neg ~ .j1.p1\n    .supply.pos ~ .j1.p2\n    .sensor",
        "    .supply.pos ~ .j1.p2\n    .sensor",
    )
    .replacen("    .supply.neg ~ .j1.p10\n", "", 1);
    refused(
        &src,
        &[
            "the mate of `.carrier.slot_a` and `.sensor.j1` carries power or signals",
            "but no ground",
            "the two boards share no ground",
        ],
    );
}

#[test]
fn a_connector_goes_to_one_mate_and_not_to_the_same_board() {
    let src = changed(
        "    .carrier.slot_b.mating ~ .serial.j1.mating\n",
        "    .carrier.slot_b.mating ~ .serial.j1.mating\n    .carrier.slot_a.mating ~ .serial.j1.mating\n",
    );
    refused(&src, &["`.carrier.slot_a` is already mated"]);
    let src = changed(
        "    .carrier.slot_b.mating ~ .serial.j1.mating\n",
        "    .carrier.slot_b.mating ~ .carrier.slot_a.mating\n",
    );
    refused(&src, &["are on the same board"]);
}

#[test]
fn a_system_reaches_a_connectors_mating_and_nothing_else_of_it() {
    refused(
        &changed(
            "    .carrier.slot_a.mating ~ .sensor.j1.mating\n",
            "    .carrier.slot_a.mating ~ .sensor.j1.mating\n    .carrier.slot_b.p1 ~ .serial.supply\n",
        ),
        &[
            "`p1` is inside `slot_b`, a `Socket1x10`",
            "a system can reach only the `.mating` of a connector",
        ],
    );
    // The rest of a board is still its own business.
    refused(
        &changed(
            "    .carrier.slot_a.mating ~ .sensor.j1.mating\n",
            "    .carrier.slot_a.mating ~ .sensor.j1.mating\n    .carrier.fuse.rated_current &= 1A\n",
        ),
        &[
            "`fuse` is inside board `Carrier`",
            "and the `.mating` of a connector",
        ],
    );
    // A `.mating` joins only another `.mating`.
    refused(
        &changed(
            "    .carrier.slot_a.mating ~ .sensor.j1.mating\n",
            "    .carrier.slot_a.mating ~ .sensor.supply\n",
        ),
        &["`.sensor.supply` is not the `.mating` of a connector"],
    );
}

#[test]
fn a_port_join_that_a_mate_makes_is_refused_once_and_names_the_mate() {
    let src = changed(
        "    .carrier.slot_a.mating ~ .sensor.j1.mating\n",
        "    .carrier.slot_a.mating ~ .sensor.j1.mating\n    .carrier.cards ~ .sensor.supply\n",
    );
    refused(
        &src,
        &[
            "`.carrier.cards` and `.sensor.supply` are already joined by the mate of `.carrier.slot_a` and `.sensor.j1`",
            "at contacts 1, 2, 9, 10",
            "remove this one",
        ],
    );
    // A port with some of its pins on the mate is the mate's as well: the carrier's `supply` has
    // its ground on contacts 1 and 10.
    let src = changed(
        "    .carrier.slot_a.mating ~ .sensor.j1.mating\n",
        "    .carrier.slot_a.mating ~ .sensor.j1.mating\n    .carrier.supply ~ .sensor.supply\n",
    );
    refused(&src, &["at contacts 1, 10"]);
}

#[test]
fn two_ports_of_a_board_on_the_same_contacts_leave_the_mate_unable_to_choose() {
    // Without the fuse the carrier's `supply` and `cards` are one net.
    let src = changed(
        "    .supply.pos ~> .fuse ~> .cards.pos\n",
        "    .supply.pos ~ .cards.pos\n",
    )
    .replace("    .fuse: Fuse\n", "")
    .replace(
        "    .cards.voltage == .supply.voltage - .fuse.drop\n",
        "    .cards.voltage == .supply.voltage\n",
    )
    .replace("\"3V3_CARDS\"", "\"3V3\"");
    let c = run(&src);
    let msgs = messages(&c);
    assert!(
        msgs.iter()
            .any(|m| m.contains("`.carrier.cards`, `.carrier.supply`")
                && m.contains("the mate cannot say which of them is joined")),
        "{msgs:#?}"
    );
}

// ---- turned end for end -----------------------------------------------------------------

/// A carrier and a card that mate through four contacts: ground, two signals, ground. The
/// pinout reads the same from either end, and the two signals would swap. `aux` is a port on
/// each board that no connector carries.
fn two_signals(keyed: &str, system: &str) -> String {
    format!(
        "\
import std::parts::connector::{{Header1x4, Socket1x4}}
import std::sources::Rail

part Out {{
    pins {{ .y output \"Y\" \"1\" }}
}}

part In {{
    pins {{ .a input \"A\" \"1\" }}
}}

board Carrier {{
    .gnd: Rail {{ .neg.net &= \"GND\" }}
    .aux: Rail {{ .current &= 0mA..10mA }}
    .first: Out
    .second: Out
    .slot: Socket1x4 {{ {keyed} }}
    .gnd.neg ~ .slot.p1
    .first.y ~ .slot.p2
    .second.y ~ .slot.p3
    .gnd.neg ~ .slot.p4
}}

board Card {{
    .gnd: Rail {{ .neg.net &= \"GND\" }}
    .aux: Rail {{ .current &= -5mA..-1mA }}
    .first: In
    .second: In
    .j: Header1x4
    .gnd.neg ~ .j.p1
    .first.a ~ .j.p2
    .second.a ~ .j.p3
    .gnd.neg ~ .j.p4
}}

system Pair {{
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j.mating
{system}}}
"
    )
}

#[test]
fn a_symmetric_pinout_is_insertable_the_wrong_way_round_unless_a_connector_is_keyed() {
    // Contacts 1 and 4 are ground on both boards and 2 and 3 are outputs to inputs, so turned end
    // for end every contact still works, and the two signals trade places.
    let c = run(&two_signals("", ""));
    assert!(c.is_ok(), "{}", c.render());
    let mate = &c.system("Pair").unwrap().model.mates()[0];
    let why = mate.reversible.as_deref().expect("the mate is reversible");
    for needle in [
        "`.carrier.slot` and `.card.j`",
        "turned end for end",
        "contact 2 (net-first.y on `.carrier.slot`, net-first.a on `.card.j`)",
        "contact 3 (net-second.y on `.carrier.slot`, net-j.p3 on `.card.j`)",
        "would swap places",
        "`.mating.keyed`",
    ] {
        assert!(why.contains(needle), "`{needle}` is not in: {why}");
    }
    // Keyed on either connector, there is nothing to say.
    let c = run(&two_signals(".mating.keyed &= \"polarised\"", ""));
    assert!(c.is_ok(), "{}", c.render());
    assert!(
        c.system("Pair").unwrap().model.mates()[0]
            .reversible
            .is_none()
    );
    // The example's pinout is not symmetric: turned round, an input would face nothing.
    let c = run(EXAMPLE);
    assert!(
        system(&c)
            .model
            .mates()
            .iter()
            .all(|m| m.reversible.is_none())
    );
}

#[test]
fn a_pinout_that_reads_the_same_turned_round_changes_nothing_and_says_nothing() {
    // Ground on all four contacts: turned round, every contact meets the same net.
    let src = two_signals("", "")
        .replace(".first.y ~ .slot.p2", ".gnd.neg ~ .slot.p2")
        .replace(".second.y ~ .slot.p3", ".gnd.neg ~ .slot.p3")
        .replace(".first.a ~ .j.p2", ".gnd.neg ~ .j.p2")
        .replace(".second.a ~ .j.p3", ".gnd.neg ~ .j.p3")
        .replace("    .first: Out\n    .second: Out\n", "")
        .replace("    .first: In\n    .second: In\n", "");
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    assert!(
        c.system("Pair").unwrap().model.mates()[0]
            .reversible
            .is_none()
    );
}

#[test]
fn a_port_that_no_connector_carries_is_still_joined_with_a_port_join() {
    let c = run(&two_signals("", "    .carrier.aux ~ .card.aux\n"));
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.system("Pair").unwrap().model;
    assert_eq!(
        m.joins().len(),
        1,
        "the explicit join of `aux`, and none from the mate"
    );
    assert!(m.mates()[0].ports.is_empty());
}
