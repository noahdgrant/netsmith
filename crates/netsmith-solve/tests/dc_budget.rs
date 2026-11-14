//! A source rail's own `.current` as a budget (ADR 0076, amendment 13).
//!
//! A supply that states its `.current` states what it gives, as a negative number (ADR 0014: a
//! rail's current is what is drawn at its `pos`): `.supply.current &= -1A..0A` is a supply of at
//! most 1 A. The loads that the DC analysis finds on its net must not draw more at any corner of
//! their ranges. A supply that states nothing has no budget. Every part and number is invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"import std::sources::Rail

part Link {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .rated_current: Current
    #[role(current)]
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    .current <= .rated_current
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

part Sw {
    .vin: Rail
    .vout: Rail
    .efficiency: Number
    .min_input: Voltage
    .vin.neg ~ .vout.neg
    .vin.voltage >= .min_input
    dc switching_regulator .vin .vout .efficiency
}
"#;

fn solved(body: &str) -> Solution {
    let c = check(
        "test.nsm",
        &format!("{PARTS}\nboard B {{\n{body}}}\n"),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn supply(stated: &str) -> String {
    format!(
        "    .supply: Rail {{\n        .voltage &= 9V..16V\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n{stated}    }}\n"
    )
}

fn load(name: &str, net: &str, current: &str) -> String {
    format!(
        "    .{name}: Rail {{\n        .pos.net &= \"{net}\"\n        .neg.net &= \"GND\"\n        .current &= {current}\n    }}\n"
    )
}

/// A 5 V output at 2 A and 90 % is 11.1 W: from 16 V that is 0.69 A at the regulator and from 9 V
/// 1.23 A, a little more at the supply with the link's drop in front of it. Over the corners the
/// loads draw 0.69 A to 1.25 A.
fn tree(stated: &str, extra: &str) -> String {
    format!(
        "{}    .link: Link {{ .resistance &= 0.1ohm; .rated_current &= 5A }}\n    .reg: Sw {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V }}\n{}{extra}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply(stated),
        load("out", "V5", "2A")
    )
}

/// Even the least the loads draw, at the corner that asks least (16 V), is above the 500 mA.
#[test]
fn loads_that_cannot_draw_less_than_the_supply_states_are_a_finding_with_the_corner() {
    let s = solved(&tree("        .current &= -500mA..0A\n", ""));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].starts_with(
            "`supply.current` is stated -500mA..0A, so the supply `supply` gives at most 500mA, but the DC analysis finds the loads on its net drawing at least 69"
        ),
        "{m:?}"
    );
    assert!(m[0].contains(" and up to 1.2"), "{m:?}");
    assert!(m[0].ends_with(", the least with supply at 16V"), "{m:?}");
}

/// ADR 0038's rule for two stated ranges is that they must be able to meet: the loads draw 0.69 A
/// to 1.25 A over the corners and the supply gives 1 A, so there is a corner where it is enough.
/// This is the reading taken (choice 1 of amendment 13): the high end alone would be a finding.
#[test]
fn a_budget_that_meets_the_range_of_the_loads_is_no_finding() {
    let s = solved(&tree("        .current &= -1A..0A\n", ""));
    assert_eq!(messages(&s), Vec::<String>::new());
    let s = solved(&tree("        .current &= -2A..0A\n", ""));
    assert_eq!(messages(&s), Vec::<String>::new());
}

#[test]
fn a_supply_that_states_no_current_has_no_budget() {
    let s = solved(&tree("", ""));
    assert_eq!(messages(&s), Vec::<String>::new());
}

/// A load straight on the supply's net adds to the regulator's: 0.35 A to 0.62 A for a 1 A load
/// and 0.5 A beside it is 0.85 A to 1.12 A, over 0.8 A at the least; neither alone is.
#[test]
fn a_load_beside_the_regulator_is_in_the_sum() {
    let body = |stated: &str, side: &str| {
        format!(
            "{}    .link: Link {{ .resistance &= 0.1ohm; .rated_current &= 5A }}\n    .reg: Sw {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V }}\n{}{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
            supply(stated),
            load("out", "V5", "1A"),
            side
        )
    };
    let m = messages(&solved(&body(
        "        .current &= -0.8A..0A\n",
        &load("side", "VIN", "0.5A"),
    )));
    // The balance of the net, which has the load beside it stated, says its own word about the
    // link; the finding of the supply is the analysis's.
    assert!(
        m.iter().any(|m| m.contains(
            "gives at most 800mA, but the DC analysis finds the loads on its net drawing at least 8"
        ) && m.ends_with(", summed over the 2 networks on it")),
        "{m:?}"
    );
    // Without the load beside it the regulator alone draws at least 0.35 A, and no finding is made.
    assert_eq!(
        messages(&solved(&body("        .current &= -0.8A..0A\n", ""))),
        Vec::<String>::new()
    );
}

/// A range with no negative end gives nothing to compare with: a positive current is what a rail
/// draws, and the balance judges it (ADR 0038).
#[test]
fn a_stated_current_that_cannot_supply_is_not_a_budget() {
    let s = solved(&tree("        .current &= 0A..2A\n", ""));
    assert!(
        messages(&s).iter().all(|m| !m.contains("gives at most")),
        "{:?}",
        messages(&s)
    );
}
