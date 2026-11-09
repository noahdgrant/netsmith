//! A `~>` chain carries the voltage of the DC analysis (ADR 0076, amendment 5).
//!
//! The analysis solves the network of the picked parts, so the rail at the end of a chain has the
//! voltage the chain leaves it: the source's, less what each part in series drops at the current
//! it carries. Every part and number here is invented; the closed forms are worked out in the
//! tests.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// The invented parts: a link (the shape of a fuse, with a stated resistance), a connector that
/// gives a supply, and a switching regulator.
const PARTS: &str = r#"import std::sources::Rail

part Link {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .rated_current: Current
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    .current <= .rated_current
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

part Plug {
    .supply: Rail
    dc source .supply
    pins {
        .supply.pos power "V" "1"
        .supply.neg power "G" "2"
    }
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

module Protect {
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    .fuse: Link {
        .resistance &= 0.1ohm
        .rated_current &= 2A
    }
    .bead: Link {
        .resistance &= 0.2ohm
        .rated_current &= 2A
    }
    .vin.pos ~> .fuse ~> .bead ~> .vout.pos
}
"#;

fn board(body: &str) -> String {
    format!("{PARTS}\nboard B {{\n{body}\n}}\n")
}

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> Option<(f64, f64)> {
    match s
        .model
        .domain_at(path)
        .unwrap_or_else(|| panic!("no field {path}"))
    {
        Domain::Quantity(q) => Some((q.lo(), q.hi())),
        _ => None,
    }
}

fn near(got: (f64, f64), want: (f64, f64)) {
    let ok = |a: f64, b: f64| (a - b).abs() <= 2e-6 * b.abs() + 1e-9;
    assert!(
        ok(got.0, want.0) && ok(got.1, want.1),
        "got {got:?}, want {want:?}"
    );
}

/// A supply from a connector, a fuse (0.1 ohm) and a bead (0.2 ohm) in a chain, and a load of
/// 0.5 A to 1 A at the end. The rail at the end of the chain has the supply's voltage less the
/// drops, and the board states none of it.
#[test]
fn a_chain_carries_the_voltage_less_the_drops_of_the_parts_in_it() {
    let src = board(
        "    .plug: Plug { .supply.voltage &= 9V..16V; .supply.pos.net &= \"VIN\"; .supply.neg.net &= \"GND\" }
    .p: Protect
    .plug.supply ~ .p.vin
    .load: Rail {
        .pos.net &= \"OUT\"
        .neg.net &= \"GND\"
        .current &= 500mA..1A
    }
    .p.vout ~ .load
",
    );
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(s.dc.rails >= 1, "{:?}", s.dc);
    // 9 V less 1 A through 0.3 ohm at the least, 16 V less 0.5 A through 0.3 ohm at the most.
    near(
        range(&s, "p.vout.voltage").unwrap(),
        (9.0 - 0.3, 16.0 - 0.15),
    );
    near(range(&s, "load.voltage").unwrap(), (9.0 - 0.3, 16.0 - 0.15));
}

fn chain_board(extra: &str) -> String {
    board(&format!(
        "    .plug: Plug {{ .supply.voltage &= 9V..16V; .supply.pos.net &= \"VIN\"; .supply.neg.net &= \"GND\" }}
    .p: Protect
    .plug.supply ~ .p.vin
    .load: Rail {{
        .pos.net &= \"OUT\"
        .neg.net &= \"GND\"
        .current &= 500mA..1A
    }}
    .p.vout ~ .load
{extra}"
    ))
}

/// A relation on the carried voltage is judged with it, and the failure says what the analysis
/// put there and which states give its low end.
#[test]
fn a_relation_on_the_carried_voltage_is_judged_with_it() {
    let s = solved(&chain_board("    .p.vout.voltage >= 8.8V\n"));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("p.vout.voltage") && m[0].contains("8.7V"),
        "{}",
        m[0]
    );
    assert!(
        m[0].contains("the DC analysis puts `p.vout.voltage` at 8.7V"),
        "{}",
        m[0]
    );
    // The same board with a bound the circuit meets builds without a message.
    let s = solved(&chain_board("    .p.vout.voltage >= 8.6V\n"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
}

/// A voltage the board states is the board's: the analysis does not narrow it. It is checked: a
/// range that encloses the circuit's (here 0 V to 20 V) is accepted and kept (ADR 0076,
/// amendment 17; `dc_stated_rail.rs` has the ones that do not enclose).
#[test]
fn a_stated_voltage_is_left_as_stated() {
    let s = solved(&chain_board("    .p.vout.voltage &= 0V..20V\n"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    near(range(&s, "p.vout.voltage").unwrap(), (0.0, 20.0));
    // Joined to it by `~`, the load's rail is the stated range too.
    near(range(&s, "load.voltage").unwrap(), (0.0, 20.0));
    // The chain's own drops are still written.
    assert!(range(&s, "p.fuse.drop").is_some());
}

/// A supply the board does not state a voltage for is no source: the analysis does not guess
/// one, and the net stays open.
#[test]
fn a_connector_with_no_stated_voltage_is_not_a_source() {
    let src = board(
        "    .plug: Plug { .supply.pos.net &= \"VIN\"; .supply.neg.net &= \"GND\" }
    .p: Protect
    .plug.supply ~ .p.vin
    .p.vout ~ .load
    .load: Rail {
        .pos.net &= \"OUT\"
        .neg.net &= \"GND\"
        .current &= 500mA..1A
    }
",
    );
    let s = solved(&src);
    assert_eq!(s.dc.islands, 0, "{:?}", s.dc);
    assert!(range(&s, "p.vout.voltage").is_none());
}

/// A node that no supply reaches (a backup input tied to the ground through a resistor, with
/// nothing to charge it) has no voltage the circuit can say: the load on it would put it below
/// the ground. No rail is given one, and the relation that reads it is not made.
#[test]
fn a_node_no_supply_reaches_is_given_no_voltage() {
    let s = solved(&chain_board(
        "    .tie: Link { .resistance &= 10kohm }
    .backup: Rail {
        .pos.net &= \"BACKUP\"
        .neg.net &= \"GND\"
        .current &= 700nA..800nA
    }
    .backup.pos ~> .tie ~> .backup.neg
    .backup.voltage >= 1.3V
",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(range(&s, "backup.voltage").is_none());
    // The supplied chain on the same board is solved as before.
    assert!(range(&s, "p.vout.voltage").is_some());
}

/// A capacitor across the input of a regulator behind a link: the pick reads the capacitor's
/// bias, which is the voltage of the rail behind the link, which the link's drop (and so the
/// regulator's input current, and so the bias) sets. The board states no window to start it.
fn start_board(extra: &str) -> String {
    board(&format!(
        "    .plug: Plug {{ .supply.voltage &= 4V..5.5V; .supply.pos.net &= \"VIN\"; .supply.neg.net &= \"GND\" }}
    .link: Link {{ .resistance &= 0.2ohm; .rated_current &= 3A }}
    .reg: Sw {{ .efficiency &= 0.9; .min_input &= 3V; .vout.voltage &= 3.3V }}
    .load: Rail {{
        .pos.net &= \"V33\"
        .neg.net &= \"GND\"
        .current &= 1A
    }}
    .cin: Capacitor
    .plug.supply.pos ~> .link ~> .reg.vin.pos
    .plug.supply.neg ~ .reg.vin.neg
    .reg.vin.pos ~> .cin ~> .reg.vin.neg
    .reg.vout ~ .load
    .cin.effective_capacitance >= 4uF
{extra}"
    ))
}

#[test]
fn a_circle_a_pick_waits_on_starts_from_the_networks_bounds() {
    let src = start_board("").replace(
        "import std::sources::Rail",
        "import std::sources::Rail\nimport std::parts::passive::Capacitor",
    );
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(s.deferred.is_empty(), "{:?}", s.deferred);
    // The regulator's input at a supply `v`: `0.9 I (v - 0.2 I) = 3.3`, so
    // `0.18 I^2 - 0.9 v I + 3.3 = 0`, the smaller root.
    let at = |v: f64| {
        let i = (0.9 * v - ((0.9 * v).powi(2) - 4.0 * 0.18 * 3.3).sqrt()) / (2.0 * 0.18);
        v - 0.2 * i
    };
    near(range(&s, "reg.vin.voltage").unwrap(), (at(4.0), at(5.5)));
    let bias = range(&s, "cin.bias").unwrap();
    assert!(
        bias.0 <= at(4.0) + 1e-6 && bias.1 >= at(5.5) - 1e-6,
        "{bias:?}"
    );
}

/// Two sheets in a chain, each with the hand-written equation of its drop (ADR 0076, amendment
/// 5, choice 8). The monitor's shunt is written from the output to the input, so its drop is
/// negative: the equation adds it. The load is declared before the sheets, so the analysis marks
/// the load's voltage as its own and the sheet's output is only joined to it.
fn two_sheets(clamp_equation: &str, monitor_equation: &str) -> String {
    let parts = PARTS.replace(
        "module Protect",
        &format!(
            r#"module Clamp {{
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    .fuse: Link {{
        .resistance &= 0.1ohm
        .rated_current &= 2A
    }}
    .vin.pos ~> .fuse ~> .vout.pos
    {clamp_equation}
}}

module Monitor {{
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    .shunt: Link {{
        .resistance &= 0.05ohm
        .rated_current &= 2A
    }}
    .vout.pos ~> .shunt ~> .vin.pos
    {monitor_equation}
}}

module Protect"#
        ),
    );
    format!(
        "{parts}\nboard B {{
    .load: Rail {{ .pos.net &= \"OUT\"; .neg.net &= \"GND\"; .current &= 500mA..1A }}
    .m: Monitor
    .a: Clamp
    .plug: Plug {{ .supply.voltage &= 9V..16V; .supply.pos.net &= \"VIN\"; .supply.neg.net &= \"GND\" }}
    .plug.supply ~ .a.vin
    .a.vout ~ .m.vin
    .m.vout ~ .load
}}\n"
    )
}

/// One of the two equations deleted and the other kept is as valid as both or neither: the
/// analysis's ranges are widened outward by a billionth and a nanovolt, and the sum of two such
/// ranges sticks out of the voltage's own by that, which is no fault of the equation. The
/// message of the failure was `must stay within 8.85V..15.93V, but (...) works out to
/// 8.85V..15.93V`.
#[test]
fn a_hand_written_drop_stays_valid_when_the_equation_of_a_neighbour_is_deleted() {
    let clamp = ".vout.voltage == .vin.voltage - .fuse.drop";
    let monitor = ".vout.voltage == .vin.voltage + .shunt.drop";
    for (clamp, monitor) in [("", ""), (clamp, ""), ("", monitor), (clamp, monitor)] {
        let s = solved(&two_sheets(clamp, monitor));
        assert!(
            messages(&s).is_empty(),
            "clamp `{clamp}`, monitor `{monitor}`: {:?}",
            messages(&s)
        );
        // 9 V less 1 A through 0.15 ohm at the least, 16 V less 0.5 A through 0.15 ohm at the
        // most (the fuse 0.1 ohm and the shunt 0.05 ohm), whichever equation is kept.
        near(
            range(&s, "m.vout.voltage").unwrap(),
            (9.0 - 0.15, 16.0 - 0.075),
        );
    }
    // An equation that shares no value with the circuit's is still a fault.
    let s = solved(&two_sheets("", ".vout.voltage == .vin.voltage + 20V"));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(m[0].contains("m.vout.voltage"), "{}", m[0]);
}
