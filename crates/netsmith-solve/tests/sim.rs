//! The voltage classes of a SIM bus against its supply (issue 1280): `std::bus::Sim`, the
//! classes of the host and the card held against each other and against `vcc.voltage` after the
//! solve. The parts are invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// A board with a host that states `host` classes and a card that states `card` (an empty
/// string states none), the supply at `vcc`.
fn source(host: &str, card: &str, vcc: &str) -> String {
    let classes = |c: &str| {
        if c.is_empty() {
            String::new()
        } else {
            format!(".sim.classes &= SimClasses::{c}")
        }
    };
    format!(
        "import std::bus::{{Sim, SimClasses, SimRole}}
import std::sources::Rail

part Radio {{
    .vdd: Rail
    .sim: Sim
    .sim.role &= SimRole::Host
    {}
    .sim.vcc.neg ~ .vdd.neg

    pins {{
        .vdd.pos     power         \"VDD\"      \"1\"
        .vdd.neg     power         \"GND\"      \"2\"
        .sim.vcc.pos power         \"SIM_VDD\"  \"3\"
        .sim.rst     output        \"SIM_RST\"  \"4\"
        .sim.clk     output        \"SIM_CLK\"  \"5\"
        .sim.io      bidirectional \"SIM_DATA\" \"6\"
    }}
}}

part Card {{
    .sim: Sim
    .sim.role &= SimRole::Card
    {}
    .sim.vcc.current &= 0mA..10mA

    pins {{
        .sim.vcc.pos power         \"VCC\" \"1\"
        .sim.vcc.neg power         \"GND\" \"2\"
        .sim.rst     input         \"RST\" \"3\"
        .sim.clk     input         \"CLK\" \"4\"
        .sim.io      bidirectional \"IO\"  \"5\"
    }}
}}

board B {{
    .supply: Rail {{ .voltage &= 3.8V +/- 5%; .pos.net &= \"VBAT\" }}
    .radio: Radio {{ .sim.vcc.voltage &= {vcc} }}
    .card: Card
    .supply ~ .radio.vdd
    .radio.sim ~ .card.sim
}}
",
        classes(host),
        classes(card),
    )
}

fn solved(src: &str) -> Solution {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn sim_errors(s: &Solution) -> Vec<String> {
    s.diagnostics
        .iter()
        .map(|d| d.message.clone())
        .filter(|m| m.contains("SIM"))
        .collect()
}

fn sim_notes(s: &Solution) -> Vec<String> {
    s.not_evaluated
        .iter()
        .map(|n| n.message.clone())
        .filter(|m| m.contains("SIM bus"))
        .collect()
}

#[test]
fn a_supply_within_a_class_both_support_passes() {
    let s = solved(&source("BC", "C", "1.8V +/- 5%"));
    assert!(sim_errors(&s).is_empty(), "{:?}", sim_errors(&s));
    assert!(sim_notes(&s).is_empty(), "{:?}", sim_notes(&s));
}

#[test]
fn a_supply_of_a_class_the_card_does_not_take_is_refused() {
    let s = solved(&source("BC", "C", "3V +/- 5%"));
    let e = sim_errors(&s);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(
        e[0].contains("`radio.sim.vcc` is 2.85V to 3.15V")
            && e[0].contains("host `radio.sim`")
            && e[0].contains("card `card.sim`")
            && e[0].contains("class C, 1.62V to 1.98V"),
        "{}",
        e[0]
    );
}

#[test]
fn a_supply_that_straddles_a_class_end_is_refused() {
    // 1.8 V +/- 15% reaches 2.07 V, above class C's 1.98 V.
    let s = solved(&source("C", "C", "1.8V +/- 15%"));
    assert_eq!(sim_errors(&s).len(), 1, "{:?}", sim_errors(&s));
}

#[test]
fn classes_with_none_in_common_are_refused_with_both_sets() {
    let s = solved(&source("BC", "A", "1.8V +/- 5%"));
    let e = sim_errors(&s);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(
        e[0].contains("supplies classes B and C (`radio.sim.classes`)")
            && e[0].contains("works at class A (`card.sim.classes`)")
            && e[0].contains("share no class"),
        "{}",
        e[0]
    );
}

#[test]
fn a_card_of_several_classes_passes_at_any_it_shares() {
    let s = solved(&source("ABC", "AB", "5V +/- 5%"));
    assert!(sim_errors(&s).is_empty(), "{:?}", sim_errors(&s));
}

#[test]
fn the_classes_are_each_sides_own_and_not_equated() {
    // Were `~` to equate them, `BC` and `C` would be a contradiction.
    let c = check("t.nsm", &source("BC", "C", "1.8V +/- 5%"), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_side_with_no_classes_is_listed_as_not_evaluated() {
    let s = solved(&source("BC", "", "1.8V +/- 5%"));
    assert!(sim_errors(&s).is_empty(), "{:?}", sim_errors(&s));
    let n = sim_notes(&s);
    assert_eq!(n.len(), 1, "{n:?}");
    assert!(n[0].contains("`card.sim.classes` has no value"), "{}", n[0]);
}
