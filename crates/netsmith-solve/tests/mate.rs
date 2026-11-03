//! What a mate asks of the solver (ADR 0040): the voltages of the rails it carries agree, and
//! the contacts carry the current through them, as far as the connectors are rated.

use netsmith_check::{Checked, Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve_system};

const EXAMPLE: &str = include_str!("../../../examples/carrier_and_cards.nsm");

fn run(src: &str) -> (Checked, Solution) {
    let c = check("mate.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    let s = solve_system(&c.systems[0], &Catalogs::jlcpcb());
    (c, s)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// A carrier with a two-contact socket and a card with a two-contact header: contact 1 is the
/// ground and contact 2 the supply, so each carries the whole current of the rail.
fn pair(carrier_draw: &str, card_draw: &str, socket: &str, header: &str, imports: &str) -> String {
    format!(
        "\
import std::sources::Rail
{imports}

board Carrier {{
    .cards: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
        .current &= {carrier_draw}
    }}
    .slot: {socket}
    .cards.neg ~ .slot.p1
    .cards.pos ~ .slot.p2
}}

board Card {{
    .supply: Rail {{
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
        .current &= {card_draw}
    }}
    .j: {header}
    .supply.neg ~ .j.p1
    .supply.pos ~ .j.p2
}}

system Pair {{
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j.mating
}}
"
    )
}

fn rated(carrier_draw: &str, card_draw: &str) -> String {
    pair(
        carrier_draw,
        card_draw,
        "Socket1x2",
        "Header1x2",
        "import std::parts::connector::{Header1x2, Socket1x2}",
    )
}

#[test]
fn the_example_solves_and_every_connector_is_rated_from_its_datasheet() {
    let (_, s) = run(EXAMPLE);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    // The three amperes of the datasheet, for the sockets and for the headers.
    for part in ["carrier.slot_a", "carrier.slot_b", "sensor.j1", "serial.j1"] {
        match s.model.domain_at(&format!("{part}.mating.pin_current")) {
            Some(Domain::Quantity(q)) => assert_eq!((q.lo(), q.hi()), (3.0, 3.0), "{part}"),
            other => panic!("{part}: {other:?}"),
        }
    }
    // The solver checked the current against the contacts: nothing is left unevaluated.
    assert!(
        s.deferred.is_empty(),
        "{:?}",
        s.deferred
            .iter()
            .map(|&i| s.model.show_expr(&s.model.relations()[i].lhs))
            .collect::<Vec<_>>()
    );
    // The socket and the header are picked from the `header` catalog, and are not on the BOM
    // (they are soldered by hand) but are sourced.
    let picked: Vec<&str> = s
        .picks
        .iter()
        .filter(|p| p.catalog == "header")
        .map(|p| p.lcsc.as_deref().unwrap())
        .collect();
    assert_eq!(
        picked,
        ["C42431852", "C42431852", "C42431810", "C42431810"],
        "{picked:?}"
    );
}

#[test]
fn rails_that_a_mate_joins_must_agree_in_voltage_and_the_message_names_the_ports() {
    let src = rated("0A..100mA", "-50mA..-10mA").replacen(
        "        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n        .current &= -50mA..-10mA",
        "        .voltage &= 5V\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n        .current &= -50mA..-10mA",
        1,
    );
    assert!(src.contains(".voltage &= 5V"));
    let (_, s) = run(&src);
    let msgs = messages(&s);
    // The first message is the disagreement; the connectors that cannot be rated while it
    // stands are blamed after it, as any picked part is.
    let m = &msgs[0];
    assert!(
        m.contains("connected values disagree")
            && m.contains("`carrier.cards.voltage` is 3.135V..3.465V")
            && m.contains("`card.supply.voltage` is 5V"),
        "{m}"
    );
}

#[test]
fn contacts_that_carry_less_than_the_rail_are_refused_with_the_numbers() {
    // One supply contact and one ground contact, 3 A each, against a rail that is built for 4 A.
    let (_, s) = run(&rated("0A..4A", "-4A..-3.5A"));
    let msgs = messages(&s);
    // Both connectors are short, and each says so with its contacts and its rating.
    assert_eq!(msgs.len(), 2, "{msgs:#?}");
    for (part, who) in [("card.j", "header"), ("carrier.slot", "socket")] {
        let m = msgs
            .iter()
            .find(|m| m.contains(&format!("no `header` candidate satisfies `{part}`")))
            .unwrap_or_else(|| panic!("{part}: {msgs:#?}"));
        assert!(
            m.contains(&format!(
                "(1 * {part}.mating.pin_current) must always be at least carrier.cards.current"
            )) && m.contains("but 3A is not always above 0A..4A")
                && m.contains(who),
            "{m}"
        );
    }
    // Within 3 A it passes: the contacts are checked in both directions and nothing is left.
    let (_, s) = run(&rated("0A..2.5A", "-2.5A..-2A"));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(s.deferred.is_empty());
}

#[test]
fn a_connector_with_no_rating_leaves_the_contact_check_unevaluated_and_says_so() {
    // A connector the library has no rating for: no catalog, so no `pin_current`.
    let unrated = "
part Unrated1x2 {
    pins {
        .p1 passive \"P1\" \"1\"
        .p2 passive \"P2\" \"2\"
    }
    .mating: Mating {
        .kind &= \"pin_header\"
        .gender &= \"header\"
        .pitch &= 2.54mm
        .rows &= 1
        .positions &= 2
    }
}
";
    let src = pair(
        "0A..4A",
        "-4A..-3.5A",
        "Socket1x2",
        "Unrated1x2",
        "import std::parts::connector::{Mating, Socket1x2}",
    ) + unrated;
    let (_, s) = run(&src);
    // The socket is rated at 3 A and 4 A is too much for it, whatever the header is.
    assert!(!s.diagnostics.is_empty());
    // Within the socket's rating, the header's check waits for a rating it does not have.
    let src = pair(
        "0A..2.5A",
        "-2.5A..-2A",
        "Socket1x2",
        "Unrated1x2",
        "import std::parts::connector::{Mating, Socket1x2}",
    ) + unrated;
    let (_, s) = run(&src);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    let waiting: Vec<String> = s
        .deferred
        .iter()
        .map(|&i| {
            let r = &s.model.relations()[i];
            format!(
                "{} {} {}",
                s.model.show_expr(&r.lhs),
                r.op.symbol(),
                s.model.show_expr(&r.rhs)
            )
        })
        .collect();
    assert!(
        waiting
            .iter()
            .all(|w| w.contains("card.j.mating.pin_current")),
        "{waiting:#?}"
    );
    assert!(!waiting.is_empty());
}

#[test]
fn many_contacts_share_the_current_and_the_ground_return_is_counted_once_for_all_rails() {
    // Four supply contacts on a 1x10 socket carry what one could not.
    let (_, s) = run(EXAMPLE);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    // Two supply contacts (2 and 9) and two ground contacts (1 and 10) at 3 A each: 6 A, so
    // the rail may be up to 6 A at the card: 0.5 A is far inside, and 7 A is refused.
    let busy = EXAMPLE
        .replacen(".current &= -30mA..-5mA", ".current &= -7A..-6.5A", 1)
        .replacen(".current &= 0mA..500mA", ".current &= 0A..8A", 1)
        .replace(".fuse: Fuse\n", ".fuse: Fuse { .rated_current &= 10A }\n");
    let (_, s) = run(&busy);
    let msgs = messages(&s);
    assert!(
        msgs.iter()
            .any(|m| m.contains("(2 * ") && m.contains("pin_current")),
        "{msgs:#?}"
    );
}
