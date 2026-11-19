//! Load switches in the DC analysis (issue 1269).
//!
//! A part that provides `std::parts::switch::LoadSwitch` states `dc load_switch`: while its enable
//! can turn it on it is its on resistance between the positive nets of its two rails, so its
//! output is the input less the drop and what the output's loads draw is drawn through the input,
//! from whatever holds it. Every part and number here is invented; the closed forms are worked out
//! in the tests.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"import std::parts::switch::{LoadSwitch, PmosLoadSwitch}
import std::sources::Rail

part Link {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    #[role(current)]
    .current: Current
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

/// An invented load switch IC.
part Zsw provides LoadSwitch {
    .max_current &= 2A
    pins {
        .in.pos  power "VIN"  "1"
        .in.neg  power "GND"  "2"
        .enable  input "ON"   "3"
        .out.pos power "VOUT" "4"
    }
}

part Ldo {
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current
    dc linear_regulator .vin .vout
}
"#;

fn board(body: &str) -> String {
    format!(
        "{PARTS}\nboard B {{\n    .supply: Rail {{\n        .voltage &= 5V\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n{body}\n}}\n"
    )
}

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
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

/// 5 V through a switch of 0.1 ohm into 10 ohm to ground: `5 / 10.1` A, and the output is
/// `5 * 10 / 10.1`. Only the analysis knows the resistor's current (the resistor states no
/// relation of its voltage), so the numbers are the analysis's, through the switch.
#[test]
fn a_load_switch_that_is_on_drops_its_on_resistance_times_the_load() {
    let src = board(
        "    .sw: Zsw { .on_resistance &= 0.1ohm }\n    .load: Link { .resistance &= 10ohm }\n    .supply ~ .sw.in\n    .sw.enable ~ .supply.pos\n    .sw.out.pos ~> .load ~> .sw.out.neg\n",
    );
    let s = solved(&src);
    let i = 5.0 / 10.1;
    near(range(&s, "load.current").unwrap(), (i, i));
    near(range(&s, "sw.in.current").unwrap(), (i, i));
    near(range(&s, "sw.out.voltage").unwrap(), (5.0 - 0.1 * i, 5.0 - 0.1 * i));
}

/// The input draws the load and the quiescent current, and a resistance in front of the switch
/// carries both: 1 A stated at the output and 0 to 100 uA of its own, through 0.5 ohm.
#[test]
fn a_load_switch_passes_its_load_and_its_quiescent_current_to_its_input() {
    let src = board(
        "    .front: Link { .resistance &= 0.5ohm }\n    .sw: Zsw { .on_resistance &= 0.1ohm; .quiescent_current &= 0A..100uA }\n    .out: Rail {\n        .pos.net &= \"VSW\"\n        .neg.net &= \"GND\"\n        .current &= 1A\n    }\n    .supply.pos ~> .front ~> .sw.in.pos\n    .supply.neg ~ .sw.in.neg\n    .sw.enable ~ .supply.pos\n    .sw.out ~ .out\n",
    );
    let s = solved(&src);
    near(range(&s, "front.current").unwrap(), (1.0, 1.0001));
    near(range(&s, "sw.in.current").unwrap(), (1.0, 1.0001));
    near(range(&s, "sw.in.voltage").unwrap(), (5.0 - 0.5 * 1.0001, 4.5));
    near(
        range(&s, "sw.out.voltage").unwrap(),
        (5.0 - 0.5 * 1.0001 - 0.1, 4.4),
    );
}

/// A switch behind a regulator is a load of the regulator: the regulator's input carries what
/// the resistor behind the switch draws, `3.3 / 33.1` A.
#[test]
fn the_load_behind_a_load_switch_is_the_load_of_the_regulator_in_front() {
    let src = board(
        "    .ldo: Ldo { .vout.voltage &= 3.3V }\n    .sw: Zsw { .on_resistance &= 0.1ohm }\n    .load: Link { .resistance &= 33ohm }\n    .supply ~ .ldo.vin\n    .ldo.vout ~ .sw.in\n    .sw.enable ~ .ldo.vout.pos\n    .sw.out.pos ~> .load ~> .sw.out.neg\n",
    );
    let s = solved(&src);
    assert_eq!(s.dc.regulators, 1, "{:?}", s.dc);
    let i = 3.3 / 33.1;
    near(range(&s, "load.current").unwrap(), (i, i));
    near(range(&s, "ldo.vin.current").unwrap(), (i, i));
}

/// A switch whose enable is on the ground can never be on: the analysis leaves what is behind it
/// alone, since the part's relations are a switch that is on, and a note says why.
#[test]
fn a_load_switch_whose_enable_is_grounded_is_left_out_with_a_note() {
    let on = board(
        "    .sw: Zsw { .on_resistance &= 0.1ohm }\n    .load: Link { .resistance &= 10ohm }\n    .supply ~ .sw.in\n    .sw.enable ~ .supply.pos\n    .sw.out.pos ~> .load ~> .sw.out.neg\n",
    );
    let off = on.replace(".sw.enable ~ .supply.pos", ".sw.enable ~ .supply.neg");
    let s = solved(&off);
    let messages: Vec<&str> = s.diagnostics.iter().map(|d| d.message.as_str()).collect();
    let notes: Vec<&str> = s.not_evaluated.iter().map(|n| n.message.as_str()).collect();
    assert!(
        notes.iter().any(|m| m.contains(
            "the DC analysis does not work out what is behind the load switch `sw`: its enable `sw.enable` is held low"
        )),
        "{notes:#?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("must stay within")),
        "{messages:#?}"
    );
    let sw = s
        .model
        .instances()
        .find(|(_, i)| i.name == "sw")
        .map(|(id, _)| id)
        .unwrap();
    assert!(!s.model.load_switch_can_turn_on(sw));
    assert_eq!(range(&s, "load.current"), None);
}

/// The P-channel use: the solver picks the MOSFET, the analysis models it as a switch, since a
/// resistor to ground sets its gate, and the input draws what the 10 ohm behind it does,
/// `5 / (10 + ron)`.
#[test]
fn a_p_channel_load_switch_passes_its_load_to_its_input() {
    let src = board(
        "    .sw: PmosLoadSwitch\n    .pull: Link { .resistance &= 10kohm }\n    .load: Link { .resistance &= 10ohm }\n    .supply ~ .sw.in\n    .sw.enable ~> .pull ~> .supply.neg\n    .sw.out.pos ~> .load ~> .sw.out.neg\n",
    );
    let s = solved(&src);
    let messages: Vec<&str> = s.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(messages.is_empty(), "{messages:#?}");
    let (lo, hi) = range(&s, "sw.on_resistance").unwrap();
    near(range(&s, "load.current").unwrap(), (5.0 / (10.0 + hi), 5.0 / (10.0 + lo)));
    near(
        range(&s, "sw.in.current").unwrap(),
        range(&s, "load.current").unwrap(),
    );
}
