//! Isolated grounds (issue #793): `.isolated` of a rail keeps its ground apart from the board's
//! other grounds, on the board and through the mates of a system.

use netsmith_check::*;

fn run(src: &str) -> Checked {
    check("isolation.nsm", src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
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

/// A carrier with a main rail on `GND` and an isolated rail on `iso_ground`, each on a socket
/// of its own, and a card whose two headers meet them: its `supply` on `GND`, its `field` on
/// `field_ground`.
fn carrier(iso_ground: &str, field_ground: &str) -> String {
    format!(
        "\
import std::parts::connector::{{Header1x2, Socket1x2}}
import std::sources::Rail

board Carrier {{
    .main: Rail {{ .neg.net &= \"GND\" }}
    .iso: Rail {{
        .isolated &= \"isolated DC-DC\"
        {iso_ground}
    }}
    .slot_a: Socket1x2
    .slot_b: Socket1x2
    .main.neg ~ .slot_a.p1
    .main.pos ~ .slot_a.p2
    .iso.neg ~ .slot_b.p1
    .iso.pos ~ .slot_b.p2
}}

board Card {{
    .supply: Rail {{ .neg.net &= \"GND\" }}
    .field: Rail {{ .neg.net &= \"{field_ground}\" }}
    .j1: Header1x2
    .j2: Header1x2
    .supply.neg ~ .j1.p1
    .supply.pos ~ .j1.p2
    .field.neg ~ .j2.p1
    .field.pos ~ .j2.p2
}}

system Pair {{
    .carrier: Carrier
    .card: Card
    .carrier.slot_a.mating ~ .card.j1.mating
    .carrier.slot_b.mating ~ .card.j2.mating
}}
"
    )
}

const ISO_GND: &str = ".neg.net &= \"ISO_GND\"";

#[test]
fn two_grounds_of_a_card_keep_the_carriers_isolated_ground_apart() {
    let c = run(&carrier(ISO_GND, "FIELD_GND"));
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.system("Pair").expect("the system").model;
    let groups: Vec<Vec<String>> = m
        .ground_groups()
        .iter()
        .map(|g| {
            g.nets
                .iter()
                .map(|n| format!("{}.{}", m.path_of(n.board), n.net))
                .collect()
        })
        .collect();
    assert_eq!(
        groups,
        [
            vec!["card.FIELD_GND", "carrier.ISO_GND"],
            vec!["card.GND", "carrier.GND"],
        ]
    );
    // Both rails are joined to the card's, each through its own mate.
    let joins: Vec<String> = m
        .joins()
        .iter()
        .map(|&(a, b)| format!("{} ~ {}", m.path_of(a), m.path_of(b)))
        .collect();
    assert_eq!(
        joins,
        ["carrier.main ~ card.supply", "carrier.iso ~ card.field"]
    );
}

#[test]
fn a_card_whose_one_ground_meets_both_sides_of_the_barrier_is_refused() {
    refused(
        &carrier(ISO_GND, "GND"),
        &[
            "the mates of `.carrier.slot_a` and `.card.j1`, `.carrier.slot_b` and `.card.j2`",
            "join `carrier.ISO_GND`, the ground of the isolated rail `.carrier.iso` (isolated DC-DC), to `carrier.GND`",
            "card.GND + carrier.GND + carrier.ISO_GND",
            "an isolated ground meets no other ground of its board",
        ],
    );
}

#[test]
fn an_isolated_rail_on_the_ground_of_another_rail_is_refused() {
    // No ground statement: the isolated rail takes the `GND` default with the main rail.
    refused(
        &carrier("", "FIELD_GND"),
        &[
            "`.iso` is isolated (isolated DC-DC) but its ground is net `GND`, the ground of `.main` too",
            "state its net (`.neg.net &= \"ISO_GND\"`)",
        ],
    );
}

#[test]
fn none_is_the_same_as_unset() {
    // Two rails of a board on one ground, neither isolated, are an ordinary board.
    let src = carrier(ISO_GND, "FIELD_GND")
        .replacen("\"isolated DC-DC\"", "\"none\"", 1)
        .replacen(ISO_GND, "", 1);
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_rail_joined_to_the_isolated_one_shares_its_ground() {
    // A module's rail joined to the isolated rail is on its ground and is no second domain.
    let c = run("\
import std::parts::passive::Resistor
import std::sources::Rail

module Load {
    .supply: Rail
    .r: Resistor
    .supply.pos ~ .r.a
    .supply.neg ~ .r.b
}

board Iso {
    .main: Rail { .neg.net &= \"GND\" }
    .iso: Rail {
        .isolated &= \"digital isolator\"
        .neg.net &= \"ISO_GND\"
    }
    .other: Rail
    .load: Load
    .iso ~ .load.supply
    .iso ~ .other
    .r: Resistor
    .main.pos ~ .r.a
    .main.neg ~ .r.b
}
");
    assert!(c.is_ok(), "{}", c.render());
}
