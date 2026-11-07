//! What parts add to a net's current (ADR 0014): the first part of a chain is drawn from
//! the net the chain starts on, and the last gives it back where the chain ends.

use netsmith_check::*;

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");
const REGULATOR: &str = include_str!("../../../examples/linear_regulator.nsm");
const STATUS: &str = include_str!("../../../examples/status_indicator.nsm");

const IMPORTS: &str = "\
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led, Capacitor}
import std::sources::Rail
import std::logic::Logic
";

fn model(name: &str, src: &str, board: &str) -> Model {
    let c = check(name, src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    c.board(board).expect("the board is there").model.clone()
}

/// A board with a supply and whatever else `body` says.
fn board(body: &str) -> Model {
    let src = format!(
        "{IMPORTS}\nboard B {{\n    .supply: Rail {{\n        .pos.net &= \"V\"\n        .neg.net &= \"GND\"\n    }}\n{body}}}\n"
    );
    model("test.nsm", &src, "B")
}

/// The signed terms of the balance on a net, as `path` or `-path`.
fn terms(m: &Model, scope: &str, net: &str) -> Option<Vec<String>> {
    let b = m
        .balances()
        .iter()
        .find(|b| m.path_of(b.scope) == scope && b.net == net)?;
    Some(
        b.draws
            .iter()
            .map(|d| {
                let sign = if d.flipped { "-" } else { "" };
                format!("{sign}{}", m.path_of_field(d.field))
            })
            .collect(),
    )
}

fn equated(m: &Model, a: &str, b: &str) -> bool {
    m.equalities()
        .iter()
        .any(|&(x, y)| (m.path_of_field(x), m.path_of_field(y)) == (a.into(), b.into()))
}

#[test]
fn a_chain_is_drawn_from_the_net_it_starts_on_and_given_back_where_it_ends() {
    let m =
        board("    .r: Resistor\n    .led: Led\n    .supply.pos ~> .r ~> .led ~> .supply.neg\n");
    // The first part is drawn from the supply net, the last gives it back on the return net.
    assert_eq!(terms(&m, "", "V").unwrap(), ["r.current", "supply.current"]);
    assert_eq!(
        terms(&m, "", "GND").unwrap(),
        ["-led.current", "-supply.current"]
    );
    let returns = |net: &str| {
        m.balances()
            .iter()
            .find(|b| b.net == net)
            .map(|b| b.returns)
    };
    assert_eq!(returns("V"), Some(false));
    assert_eq!(returns("GND"), Some(true));
    // A series chain passes one current.
    assert!(equated(&m, "r.current", "led.current"));
}

#[test]
fn the_current_a_chain_draws_is_not_stated_by_the_board_in_two_boards() {
    let c = check("two_boards.nsm", TWO_BOARDS, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("LedBoard").unwrap().model;
    assert!(
        !TWO_BOARDS.contains(".input.current ="),
        "no by-hand current"
    );
    assert_eq!(
        terms(m, "", "3V3").unwrap(),
        ["input.current", "r1.current"]
    );
    assert_eq!(
        terms(m, "", "GND").unwrap(),
        ["-input.current", "-led.current"]
    );
}

#[test]
fn a_capacitor_draws_nothing_at_dc() {
    let m = model("regulator.nsm", REGULATOR, "RegulatorExample");
    let all: Vec<String> = m
        .balances()
        .iter()
        .flat_map(|b| terms(&m, &m.path_of(b.scope), &b.net).unwrap())
        .collect();
    assert!(!all.iter().any(|t| t.contains("cap")), "{all:?}");
    // The decoupling chains have no current to count and add no equality: only the four joins of rails, which tie a voltage, a peak
    // voltage and a track width each.
    assert_eq!(m.equalities().len(), 12);
    // The same on a board of its own: the chain has a part, and no balance gets a term.
    let m = board("    .bypass: Capacitor\n    .supply.pos ~> .bypass ~> .supply.neg\n");
    assert_eq!(terms(&m, "", "V").unwrap(), ["supply.current"]);
    assert!(terms(&m, "", "GND").is_none());
}

#[test]
fn a_chain_that_starts_on_a_signal_is_left_out_at_both_ends() {
    let m = model("status.nsm", STATUS, "IndicatorExample");
    let mentioned = m.balances().iter().any(|b| {
        terms(&m, &m.path_of(b.scope), &b.net)
            .unwrap()
            .iter()
            .any(|t| t.contains("current_limit") || t.contains("led"))
    });
    assert!(!mentioned);
    // The two parts still pass one current.
    assert!(equated(
        &m,
        "status_indicator.current_limit.current",
        "status_indicator.led.current"
    ));
}

#[test]
fn a_tapped_junction_does_not_equate_and_leaves_the_return_unchecked() {
    // A divider with something on its middle: the two resistors need not pass one current,
    // so only the start is drawn from the supply and the return net is not checked.
    let m = board(
        "    .r1: Resistor\n    .r2: Resistor\n    .load: Resistor\n    .supply.pos ~> .r1 ~> .r2 ~> .supply.neg\n    .r1.b ~ .load.a\n    .load.b ~ .supply.neg\n",
    );
    assert_eq!(
        terms(&m, "", "V").unwrap(),
        ["r1.current", "supply.current"]
    );
    assert!(terms(&m, "", "GND").is_none());
    assert!(!equated(&m, "r1.current", "r2.current"));
}

#[test]
fn a_chain_of_which_only_the_start_passes_a_current_is_partial() {
    let m =
        board("    .r: Resistor\n    .c: Capacitor\n    .supply.pos ~> .r ~> .c ~> .supply.neg\n");
    assert_eq!(terms(&m, "", "V").unwrap(), ["r.current", "supply.current"]);
    assert!(terms(&m, "", "GND").is_none());
}

#[test]
fn a_part_with_a_rail_of_its_own_draws_through_it() {
    // The general way for a part to say it draws: an interface of the kind that carries a
    // current. It counts as any rail on a board does, with no chain.
    let src = format!(
        "{IMPORTS}\npart Mcu {{\n    .vdd: Rail\n}}\n\nboard B {{\n    .supply: Rail {{\n        .pos.net &= \"V\"\n        .neg.net &= \"GND\"\n    }}\n    .mcu: Mcu {{\n        .vdd.current &= 10mA..30mA\n    }}\n    .supply ~ .mcu.vdd\n}}\n"
    );
    let m = model("test.nsm", &src, "B");
    assert_eq!(
        terms(&m, "", "V").unwrap(),
        ["mcu.vdd.current", "supply.current"]
    );
}

// ---- a balance is complete or it is not used for parts (ADR 0027) ---------------------

/// The pins on a net that its balance in `scope` does not account for, by path.
fn unmodelled(m: &Model, scope: &str, net: &str) -> Vec<String> {
    let b = m
        .balances()
        .iter()
        .find(|b| m.path_of(b.scope) == scope && b.net == net)
        .unwrap_or_else(|| panic!("no balance for `{net}` in `{scope}`"));
    b.unmodelled.iter().map(|&p| m.path_of(p)).collect()
}

#[test]
fn a_net_with_only_terms_and_parts_that_draw_nothing_is_complete() {
    // The chain's parts are terms; the capacitor and the header have no current by their
    // models (every pin passive, no `current`, no rail).
    let m = board(
        "    .r: Resistor\n    .led: Led\n    .c: Capacitor\n    .h: Header1x2\n    \
         .supply.pos ~> .r ~> .led ~> .supply.neg\n    \
         .supply.pos ~> .c ~> .supply.neg\n    .supply.pos ~ .h.p1\n    .supply.neg ~ .h.p2\n",
    );
    assert!(unmodelled(&m, "", "V").is_empty());
    assert!(unmodelled(&m, "", "GND").is_empty());
}

#[test]
fn a_pin_that_draws_something_unmodelled_makes_the_balance_incomplete() {
    // A resistor joined with `~` and not in a chain, and an IC's signal pin: nothing says
    // what they draw from the net.
    let m = board(
        "    .r: Resistor\n    .led: Led\n    .loose: Resistor\n    .supply.pos ~> .r ~> .led ~> .supply.neg\n    \
         .supply.pos ~ .loose.a\n",
    );
    assert_eq!(unmodelled(&m, "", "V"), ["loose.a"]);
    assert!(unmodelled(&m, "", "GND").is_empty());
    let b = m.balances().iter().find(|b| b.net == "V").unwrap();
    assert!(!b.is_complete());
}

/// Two ICs invented for the tests. `Ic` takes its supply through a rail and has an enable
/// pin and an unused pin; `NoRail` has an enable pin and no rail to say where it is
/// supplied.
const ICS: &str = r#"
part Ic {
    .vdd: Rail
    pins {
        .vdd.pos   power       "VDD" "1"
        .vdd.neg   power       "GND" "2"
        .en        input       "EN"  "3" optional
        .nc        no_connect  "NC"  "4"
    }
}

part NoRail {
    pins {
        .en        input       "EN"  "1" optional
    }
}
"#;

fn board_with_ics(body: &str) -> Model {
    let src = format!(
        "{IMPORTS}{ICS}\nboard B {{\n    .supply: Rail {{\n        .pos.net &= \"V\"\n        .neg.net &= \"GND\"\n    }}\n{body}}}\n"
    );
    model("test.nsm", &src, "B")
}

#[test]
fn an_input_pin_of_a_part_with_a_rail_draws_nothing() {
    // The enable pin is tied to the net the resistor feeds. The IC's supply is its rail, so
    // the pin's leakage belongs to it and is ignored (ADR 0027); the unused pin is nothing.
    let m = board_with_ics(
        "    .r: Resistor\n    .ic: Ic\n    .supply.pos ~> .r ~> .ic.vdd.pos\n    \
         .ic.vdd.neg ~ .supply.neg\n    .ic.en ~ .ic.vdd.pos\n",
    );
    assert!(unmodelled(&m, "", "V").is_empty());
    assert_eq!(
        unmodelled(&m, "", "GND"),
        Vec::<String>::new(),
        "the ground net is complete too"
    );
    assert!(m.balances().iter().all(|b| b.is_complete()));
}

#[test]
fn an_input_pin_of_a_part_with_no_rail_stays_unmodelled() {
    // Nothing says where the part is supplied, so what its input draws is not known.
    let m = board_with_ics(
        "    .r: Resistor\n    .ic: Ic\n    .other: NoRail\n    .supply.pos ~> .r ~> .ic.vdd.pos\n    \
         .ic.vdd.neg ~ .supply.neg\n    .other.en ~ .ic.vdd.pos\n",
    );
    let net = m
        .net_of(m.inst_at("ic.vdd.pos").unwrap())
        .unwrap()
        .name
        .clone();
    assert_eq!(unmodelled(&m, "", &net), ["other.en"]);
}

#[test]
fn a_module_port_stands_for_what_is_outside_and_a_child_module_for_what_is_inside() {
    // `l.b` and `r_top.a` are in the stage's scope; the board's load is the port's business.
    let src = "\
import std::parts::passive::{Inductor, Resistor}
import std::sources::Rail

module Stage {
    .vin: Rail
    .vout: Rail
    .l: Inductor
    .r_top: Resistor
    .r_bottom: Resistor
    .vin.pos ~ .l.a
    .l.b ~ .vout.pos
    .vin.neg ~ .vout.neg
    .vout.pos ~> .r_top ~> .r_bottom ~> .vout.neg
}

board B {
    .input: Rail { .pos.net &= \"VIN\"\n .neg.net &= \"GND\" }
    .stage: Stage
    .output: Rail { .pos.net &= \"VOUT\"\n .neg.net &= \"GND\" }
    .input ~ .stage.vin
    .stage.vout ~ .output
}
";
    let m = model("test.nsm", src, "B");
    // Inside the stage: the divider's first pin is a term, the inductor's is not.
    assert_eq!(unmodelled(&m, "stage", "VOUT"), ["stage.l.b"]);
    assert_eq!(unmodelled(&m, "stage", "VIN"), ["stage.l.a"]);
    // On the board the stage is a child with a rail on each net: it accounts for them.
    assert!(unmodelled(&m, "", "VOUT").is_empty());
    assert!(unmodelled(&m, "", "VIN").is_empty());
}

#[test]
fn the_examples_are_complete_except_where_a_logic_pin_pulls_an_led_down() {
    for (name, src) in [
        ("two_boards.nsm", TWO_BOARDS),
        ("linear_regulator.nsm", REGULATOR),
    ] {
        let c = check(name, src, &Loader::std());
        assert!(c.is_ok(), "{}", c.render());
        for b in &c.boards {
            for balance in b.model.balances() {
                assert!(
                    balance.is_complete(),
                    "`{}` in `{name}`: {:?}",
                    balance.net,
                    balance.unmodelled
                );
            }
        }
    }
    // The status indicator's LED goes to an enable line, whose driver is not modelled: its
    // return net is the only one that is not complete, and the LED's cathode is the pin.
    let m = model("status_indicator.nsm", STATUS, "IndicatorExample");
    let incomplete: Vec<_> = m
        .balances()
        .iter()
        .filter(|b| !b.is_complete())
        .map(|b| (m.path_of(b.scope), b.returns))
        .collect();
    assert!(
        incomplete.iter().all(|(_, returns)| *returns),
        "{incomplete:?}"
    );
}
