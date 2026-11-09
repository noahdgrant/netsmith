//! A part is picked after the parts whose picks decide what it needs, never by the order of
//! the parts' names: the same board builds, with the same parts, whatever its parts are
//! called.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// A capacitor on the rail behind a Schottky diode: the rail's voltage is the supply's minus
/// the forward voltage of the diode that the solver picks, and the capacitor's effective
/// capacitance needs that voltage as its bias.
fn board(diode: &str, cap: &str, tie: bool) -> String {
    let tie = if tie {
        format!("    .{diode}.max_reverse_voltage >= .{cap}.max_voltage\n")
    } else {
        String::new()
    };
    format!(
        r#"import std::parts::diode::Schottky
import std::parts::passive::Capacitor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board B {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        .voltage &= 5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .load: Rail {{
        .pos.net &= "VLOAD"
        .neg.net &= "GND"
        .current &= 0mA..100mA
    }}
    .supply.neg ~ .load.neg
    .{diode}: Schottky {{}}
    .supply.pos ~> .{diode} ~> .load.pos
    .load.voltage == .supply.voltage - .{diode}.drop
    .{cap}: Capacitor {{ .effective_capacitance >= 1uF }}
    .load.pos ~> .{cap} ~> .load.neg
{tie}}}
"#
    )
}

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// What was picked for the diode and for the capacitor, whatever they are called.
fn picks(s: &Solution) -> (String, String) {
    let of = |catalog: &str| {
        let found: Vec<&str> = s
            .picks
            .iter()
            .filter(|p| p.catalog == catalog)
            .map(|p| p.candidate.as_str())
            .collect();
        assert_eq!(found.len(), 1, "{catalog}: {found:?}");
        found[0].to_string()
    };
    (of("schottky"), of("capacitor"))
}

#[test]
fn a_part_behind_a_picked_diode_is_picked_after_it_whatever_the_names() {
    let diode_first = solved(&board("a_diode", "z_cap", false));
    assert_eq!(messages(&diode_first), Vec::<String>::new());
    let cap_first = solved(&board("z_diode", "a_cap", false));
    assert_eq!(messages(&cap_first), Vec::<String>::new());
    assert_eq!(picks(&diode_first), picks(&cap_first));
}

#[test]
fn the_same_holds_when_the_two_parts_share_a_relation() {
    let diode_first = solved(&board("a_diode", "z_cap", true));
    assert_eq!(messages(&diode_first), Vec::<String>::new());
    let cap_first = solved(&board("z_diode", "a_cap", true));
    assert_eq!(messages(&cap_first), Vec::<String>::new());
    assert_eq!(picks(&diode_first), picks(&cap_first));
}

#[test]
fn the_picks_come_in_the_order_of_the_paths() {
    let s = solved(&board("z_diode", "a_cap", false));
    let order: Vec<String> = s.picks.iter().map(|p| s.model.path_of(p.part)).collect();
    assert_eq!(order, ["a_cap", "z_diode"]);
}

#[test]
fn a_bias_that_no_pick_sets_is_still_the_designs_to_state() {
    // The capacitor is behind a resistor, across no rail, and a diode elsewhere on the
    // board feeds nothing it reads: the message is the one it always was.
    let src = r#"import std::parts::diode::Schottky
import std::parts::passive::{Capacitor, Resistor}
import std::sources::Rail

board B {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .other: Rail {
        .voltage &= 5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
        .current &= 0mA..100mA
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .a_filter: Capacitor { .effective_capacitance >= 1uF }
    .z_diode: Schottky {}
    .supply.pos ~> .r1 ~> .a_filter ~> .supply.neg
    .other.neg ~> .z_diode ~> .other.pos
}
"#;
    let s = solved(src);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("`a_filter.bias` is not known")
            && msgs[0].contains("`a_filter.effective_capacitance`"),
        "{}",
        msgs[0]
    );
}
