//! Regulators in the DC analysis (ADR 0076, amendment 3).
//!
//! A regulator holds its output net at its stated voltage, so what is behind it is an island of
//! its own, and its input draws what its output asks: a linear one what it supplies, a switching
//! one `Pout / (eff * Vin)`, which depends on the voltage its own current lowers through the
//! parts in front of it. The pass solves the output side first, takes its range as the
//! regulator's load, and solves the input side with the fixed point. Every part and number here
//! is invented; the closed forms are worked out in the tests.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// The invented parts: a link (the shape of a fuse, with a stated resistance), a linear regulator
/// and a switching one, each stating its element and the least input the part works from.
const PARTS: &str = r#"import std::parts::diode::Diode
import std::sources::Rail

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

part Ldo {
    .vin: Rail
    .vout: Rail
    .min_input: Voltage
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current
    .vin.voltage >= .min_input
    dc linear_regulator .vin .vout
}

/// A linear regulator that states its own quiescent current (ADR 0076, amendment 9).
part Ldoq {
    .vin: Rail
    .vout: Rail
    .min_input: Voltage
    .iq: Current
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current + .iq
    .vin.voltage >= .min_input
    dc linear_regulator .vin .vout .iq
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

/// A switching regulator that states its own quiescent current (ADR 0096).
part Swq {
    .vin: Rail
    .vout: Rail
    .efficiency: Number
    .min_input: Voltage
    .iq: Current
    .vin.neg ~ .vout.neg
    .vin.voltage >= .min_input
    dc switching_regulator .vin .vout .efficiency .iq
}

part Mcu {
    .vdd: Rail
}
"#;

fn board(body: &str) -> String {
    format!("{PARTS}\nboard B {{\n{body}\n}}\n")
}

fn supply(v: &str) -> String {
    format!(
        "    .supply: Rail {{\n        .voltage &= {v}\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n"
    )
}

fn load(name: &str, net: &str, current: &str) -> String {
    format!(
        "    .{name}: Rail {{\n        .pos.net &= \"{net}\"\n        .neg.net &= \"GND\"\n        .current &= {current}\n    }}\n"
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

/// Within a part in 1e6 and a pico of each end: the pass widens a range by a billionth.
fn near(got: (f64, f64), want: (f64, f64)) {
    let ok = |a: f64, b: f64| (a - b).abs() <= 2e-6 * b.abs() + 1e-9;
    assert!(
        ok(got.0, want.0) && ok(got.1, want.1),
        "got {got:?}, want {want:?}"
    );
}

/// A switching regulator after a link, with its load stated at the output. 12 V, 0.5 ohm, 5 V out
/// at 1 A, 90 %: `0.9 I (12 - 0.5 I) = 5`, so `0.45 I^2 - 10.8 I + 5 = 0`. Without the drop the
/// current would be `5 / (0.9 * 12)` = 0.46296 A, and the regulator's input is not 12 V.
#[test]
fn a_switching_regulator_draws_its_power_through_the_link_before_it() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    .reg: Sw {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V }}\n{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply("12V"),
        load("out", "V5", "1A")
    ));
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.dc.regulators, 1, "{:?}", s.dc);
    let i = (10.8 - (10.8f64 * 10.8 - 4.0 * 0.45 * 5.0).sqrt()) / (2.0 * 0.45);
    assert!((i - 0.47226).abs() < 1e-5, "{i}");
    near(range(&s, "link.current").unwrap(), (i, i));
    near(range(&s, "reg.vin.current").unwrap(), (i, i));
    near(range(&s, "link.drop").unwrap(), (0.5 * i, 0.5 * i));
    near(
        range(&s, "reg.vin.voltage").unwrap(),
        (12.0 - 0.5 * i, 12.0 - 0.5 * i),
    );
    // The relation of the link that waited for a current is judged.
    assert!(s.deferred.is_empty() || messages(&s).is_empty());
}

/// A linear regulator passes its output current: the link carries the load's 0.1 A to 0.2 A,
/// and the input is `12 - 0.5 I`.
#[test]
fn a_linear_regulator_passes_the_current_of_its_load() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    .reg: Ldo {{ .min_input &= 4.5V; .vout.voltage &= 3.3V }}\n{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply("12V"),
        load("out", "V33", "100mA..200mA")
    ));
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    near(range(&s, "link.current").unwrap(), (0.1, 0.2));
    near(range(&s, "reg.vin.voltage").unwrap(), (11.9, 11.95));
}

/// A linear regulator's quiescent current is drawn at its input besides the load: the link
/// carries `0.1 A + 50 uA` to `0.2 A + 140 uA`, and the regulator's `.vin.current` is the same.
#[test]
fn a_linear_regulators_quiescent_current_adds_to_its_input() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    .reg: Ldoq {{ .min_input &= 4.5V; .vout.voltage &= 3.3V; .iq &= 50uA..140uA }}\n{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply("12V"),
        load("out", "V33", "100mA..200mA")
    ));
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let (lo, hi) = (0.1 + 50e-6, 0.2 + 140e-6);
    near(range(&s, "link.current").unwrap(), (lo, hi));
    near(range(&s, "reg.vin.current").unwrap(), (lo, hi));
    near(
        range(&s, "reg.vin.voltage").unwrap(),
        (12.0 - 0.5 * hi, 12.0 - 0.5 * lo),
    );
    // Without a quiescent current to name, the same board has the load alone, as before.
    let src = src
        .replace(".reg: Ldoq", ".reg: Ldo")
        .replace("; .iq &= 50uA..140uA", "");
    let s = solved(&src);
    near(range(&s, "link.current").unwrap(), (0.1, 0.2));
}

/// The quiescent current of a linear regulator is a load of the regulator before it: B (3.3 V,
/// 0.5 A, 140 uA) behind A (switching, 5 V, 90 %), so A's output supplies `0.5 A + 140 uA`, which
/// is `5 * 0.50014 / 0.9` of its input: `0.9 I (12 - 0.5 I) = 2.5007`. Nothing is stated at A's
/// output: the island of the 3.3 V loads behind B is solved, and the regulators in front of it.
#[test]
fn the_quiescent_current_of_the_regulator_behind_is_the_load_of_the_one_before() {
    let src = board(&format!(
        "{}    .front: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    .a: Sw {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V }}\n    .b: Ldoq {{ .min_input &= 3.6V; .vout.voltage &= 3.3V; .iq &= 140uA }}\n{}    .supply.pos ~> .front ~> .a.vin.pos\n    .supply.neg ~ .a.vin.neg\n    .a.vout ~ .b.vin\n    .b.vout ~ .out\n",
        supply("12V"),
        load("out", "V33", "0.5A")
    ));
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.dc.regulators, 2, "{:?}", s.dc);
    near(range(&s, "b.vin.current").unwrap(), (0.50014, 0.50014));
    let p = 5.0 * 0.50014 / 0.9;
    let ia = (12.0 - (144.0f64 - 2.0 * p).sqrt()) / 1.0;
    near(range(&s, "front.current").unwrap(), (ia, ia));
    near(
        range(&s, "a.vin.voltage").unwrap(),
        (12.0 - 0.5 * ia, 12.0 - 0.5 * ia),
    );
}

/// A quiescent current that the part leaves open (no value at all) is a number nothing can model
/// with: the regulator is left out of the analysis, and nothing is guessed.
#[test]
fn an_open_quiescent_current_leaves_the_regulator_unmodelled() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    .reg: Ldoq {{ .min_input &= 4.5V; .vout.voltage &= 3.3V }}\n{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply("12V"),
        load("out", "V33", "100mA..200mA")
    ));
    let s = solved(&src);
    assert_eq!(s.dc.regulators, 0, "{:?}", s.dc);
}

/// The input current of the switching regulator at `iq` of quiescent current: `I = P / (eff *
/// (12 - 0.5 I)) + iq` for the link of 0.5 ohm from 12 V, by iteration.
fn switching_input(p: f64, eff: f64, iq: f64) -> f64 {
    let mut i = 0.0;
    for _ in 0..200 {
        i = p / (eff * (12.0 - 0.5 * i)) + iq;
    }
    i
}

/// A switching regulator's quiescent current is drawn at its input besides what the output asks
/// (ADR 0096): the ends add, so `0 A` to `10 uA` on top of `Pout / (eff * Vin)`, at the voltage
/// the link before it leaves.
#[test]
fn a_switching_regulators_quiescent_current_adds_to_its_input() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    .reg: Swq {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V; .iq &= 0A..10uA }}\n{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply("12V"),
        load("out", "V5", "1A")
    ));
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.dc.regulators, 1, "{:?}", s.dc);
    let (lo, hi) = (
        switching_input(5.0, 0.9, 0.0),
        switching_input(5.0, 0.9, 10e-6),
    );
    assert!(hi - lo > 9.9e-6, "{lo} {hi}");
    near(range(&s, "link.current").unwrap(), (lo, hi));
    near(range(&s, "reg.vin.current").unwrap(), (lo, hi));
}

/// With nothing loaded the converter still draws its own current from its input: the whole of the
/// current is the quiescent one. Before ADR 0096 the DC analysis proved it drew 0 A.
#[test]
fn an_unloaded_switching_regulator_draws_its_quiescent_current() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    .reg: Swq {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V; .iq &= 0A..10uA }}\n{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply("12V"),
        load("out", "V5", "0A")
    ));
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.dc.regulators, 1, "{:?}", s.dc);
    near(range(&s, "link.current").unwrap(), (0.0, 10e-6));
    near(range(&s, "reg.vin.current").unwrap(), (0.0, 10e-6));
}

/// A quiescent current that the part leaves open is a number nothing can model with: the
/// regulator is left out of the analysis, as a linear one is.
#[test]
fn an_open_quiescent_current_leaves_the_switching_regulator_unmodelled() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    .reg: Swq {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V }}\n{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply("12V"),
        load("out", "V5", "1A")
    ));
    let s = solved(&src);
    assert_eq!(s.dc.regulators, 0, "{:?}", s.dc);
}

/// A regulator behind a regulator: B (linear, 3.3 V, 0.5 A) is fed from A's 5 V output through
/// 0.2 ohm, so B's input is `5 - 0.2 * 0.5` = 4.9 V and the 0.5 A is A's load, which asks
/// `5 * 0.5 / 0.9` = 2.778 W of its own input: `0.9 I (12 - 0.5 I) = 2.5`.
#[test]
fn a_regulator_behind_a_regulator_is_the_load_of_the_first() {
    let src = board(&format!(
        "{}    .front: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    .between: Link {{ .resistance &= 0.2ohm; .rated_current &= 2A }}\n    .a: Sw {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V }}\n    .b: Ldo {{ .min_input &= 3.6V; .vout.voltage &= 3.3V }}\n{}    .supply.pos ~> .front ~> .a.vin.pos\n    .supply.neg ~ .a.vin.neg\n    .a.vout.pos ~> .between ~> .b.vin.pos\n    .a.vout.neg ~ .b.vin.neg\n    .b.vout ~ .out\n",
        supply("12V"),
        load("out", "V33", "0.5A")
    ));
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.dc.regulators, 2, "{:?}", s.dc);
    near(range(&s, "between.current").unwrap(), (0.5, 0.5));
    near(range(&s, "b.vin.voltage").unwrap(), (4.9, 4.9));
    // A's output is held at 5 V, and what it gives is B's input current.
    let ia = (10.8 - (10.8f64 * 10.8 - 4.0 * 0.45 * 2.5).sqrt()) / (2.0 * 0.45);
    near(range(&s, "front.current").unwrap(), (ia, ia));
    near(
        range(&s, "a.vin.voltage").unwrap(),
        (12.0 - 0.5 * ia, 12.0 - 0.5 * ia),
    );
}

/// The source cannot give what the load asks: 12 V through 10 ohm gives at most
/// `12^2 / (4 * 10) = 3.6 W`, and the regulator's output asks `5 / 0.9 = 5.56 W` of its input.
/// That is an error that names the regulator, the voltage its input fell to and the states.
#[test]
fn a_source_that_cannot_give_what_the_load_asks_is_an_error() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 10ohm; .rated_current &= 2A }}\n    .reg: Sw {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V }}\n{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply("12V"),
        load("out", "V5", "1A")
    ));
    let s = solved(&src);
    let m: Vec<String> = messages(&s)
        .into_iter()
        .filter(|m| m.contains("cannot give"))
        .collect();
    assert_eq!(m.len(), 1, "{:?}", messages(&s));
    assert_eq!(
        m[0],
        "the source cannot give what `reg` asks (5W at its output, efficiency 90 %): its input falls to 0V and no operating point exists"
    );
    assert_eq!(s.dc.collapsed, 1);
    // Nothing is written for an island with no operating point.
    assert_eq!(range(&s, "link.current"), None);
}

/// Only the corners that collapse are named: the supply from 10 V to 16 V through 10 ohm can give
/// 6.4 W at the top and 2.5 W at the bottom, against the 5.56 W asked.
#[test]
fn the_states_of_a_collapse_name_the_corner_that_gives_it() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 10ohm; .rated_current &= 2A }}\n    .reg: Sw {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V }}\n{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply("10V..16V"),
        load("out", "V5", "1A")
    ));
    let s = solved(&src);
    let m: Vec<String> = messages(&s)
        .into_iter()
        .filter(|m| m.contains("cannot give"))
        .collect();
    assert_eq!(m.len(), 1, "{:?}", messages(&s));
    assert!(
        m[0].ends_with(": its input falls to 0V and no operating point exists, with supply at 10V"),
        "{}",
        m[0]
    );
    // The corner that holds is solved: at 16 V `0.9 I (16 - 10 I) = 5`.
    let i = (14.4 - (14.4f64 * 14.4 - 4.0 * 9.0 * 5.0).sqrt()) / 18.0;
    let (_, hi) = range(&s, "link.current").unwrap();
    assert!((hi - i).abs() < 1e-6, "{hi} against {i}");
}

/// An input below the regulator's minimum is an error: the link takes 1 V of the 5 V supply at
/// the 1 A the linear regulator passes, so its input is 4 V against the 4.5 V the part works from.
/// The relation the part states for it (`.vin.voltage >= .min_input`) is judged with the number
/// the analysis gives.
#[test]
fn an_input_below_the_minimum_of_the_regulator_is_an_error() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 1ohm; .rated_current &= 2A }}\n    .reg: Ldo {{ .min_input &= 4.5V; .vout.voltage &= 3.3V }}\n{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply("5V"),
        load("out", "V33", "1A")
    ));
    let s = solved(&src);
    let m = messages(&s);
    assert!(
        m.iter()
            .any(|m| m.contains("reg.vin.voltage") && m.contains("4.5V")),
        "{m:?}"
    );
    near(range(&s, "reg.vin.voltage").unwrap(), (4.0, 4.0));
}

/// A boundary on the output with a stated current (question 5 of the ADR): the regulator's
/// output supplies the 1 mA to 2 mA of an LED, and a linear regulator's input draws it.
#[test]
fn a_boundary_with_a_stated_current_on_the_output_is_the_regulators_load() {
    let src = format!(
        "import std::parts::passive::Led\n{}",
        board(&format!(
            "{}    .reg: Ldo {{ .min_input &= 4.5V; .vout.voltage &= 3.3V }}\n    .led: Led {{ .current &= 1mA..2mA; .forward_voltage &= 1.8V..2.4V }}\n    .supply.pos ~ .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout.pos ~> .led ~> .reg.vout.neg\n",
            supply("12V")
        ))
    );
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    near(range(&s, "reg.vin.current").unwrap(), (0.001, 0.002));
}

/// A regulator whose output has no load that anyone can say (a pin that nothing models on the
/// net and no stated current) leaves the island in front of it alone, as before.
#[test]
fn an_output_with_no_load_that_can_be_said_leaves_the_input_alone() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    .reg: Sw {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V }}\n    .unknown: Sink\n    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout.pos ~ .unknown.p\n    .reg.vout.neg ~ .supply.neg\n    .unknown.g ~ .supply.neg\n",
        supply("12V")
    ))
    .replace(
        "part Mcu {",
        "part Sink {\n    pins {\n        .p output \"P\" \"1\"\n        .g output \"G\" \"2\"\n    }\n}\n\npart Mcu {",
    );
    let s = solved(&src);
    assert_eq!(range(&s, "link.current"), None);
    assert_eq!(s.dc.regulators, 0, "{:?}", s.dc);
}

/// A board rail on a regulator's output that states its own voltage is a load, not a second
/// source: the regulator holds the net, and the rail's `.current` is what it supplies.
#[test]
fn a_board_rail_on_the_output_that_states_its_voltage_is_a_load() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    .reg: Sw {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V }}\n    .out: Rail {{\n        .voltage &= 5V\n        .pos.net &= \"V5\"\n        .neg.net &= \"GND\"\n        .current &= 1A\n    }}\n    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply("12V")
    ));
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let i = (10.8 - (10.8f64 * 10.8 - 4.0 * 0.45 * 5.0).sqrt()) / (2.0 * 0.45);
    near(range(&s, "link.current").unwrap(), (i, i));
}

/// The same design gives the same numbers and messages.
#[test]
fn the_regulator_pass_is_deterministic() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 0.5ohm +/- 1%; .rated_current &= 2A }}\n    .reg: Sw {{ .efficiency &= 0.85..0.95; .min_input &= 4.5V; .vout.voltage &= 5V +/- 2% }}\n{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n",
        supply("9V..16V"),
        load("out", "V5", "0.5A..1A")
    ));
    let a = solved(&src);
    let b = solved(&src);
    for p in [
        "link.current",
        "link.drop",
        "reg.vin.current",
        "reg.vin.voltage",
    ] {
        assert_eq!(a.model.domain_at(p), b.model.domain_at(p), "{p}");
    }
    assert_eq!(messages(&a), messages(&b));
    assert_eq!(a.dc, b.dc);
    assert!(messages(&a).is_empty(), "{:?}", messages(&a));
}

// ---- an invented power tree ---------------------------------------------------------------

/// A fuse, a diode, a shunt and a bead in a chain from a battery of 9 V to 16 V, and behind them
/// a 5 V stage (85 % to 95 %) with a 0.1 A to 0.3 A load and a linear regulator of 3.3 V on its
/// output (20 mA to 50 mA), a 3.8 V stage (80 % to 90 %) with 0.2 A to 0.4 A, and a 10 mA to
/// 20 mA load on the chain itself. `hand` adds what a board writes by hand today: the equation
/// of the chain's voltage and a seed for the input of the first stage.
fn power_tree(hand: bool) -> String {
    let hand_written = if hand {
        "    .buck5.vin.voltage == .supply.voltage - .fuse.drop - .diode.drop - .shunt.drop - .bead.drop\n    .buck5.vin.voltage &= 7V..16V\n"
    } else {
        ""
    };
    board(&format!(
        "{}\
    .fuse: Link {{ .resistance &= 30mohm; .rated_current &= 3A }}\n\
    .diode: Diode {{ .forward_voltage &= 0.5V..0.8V; .max_forward_current &= 3A; .max_reverse_voltage &= 40V }}\n\
    .shunt: Link {{ .resistance &= 50mohm; .rated_current &= 3A }}\n\
    .bead: Link {{ .resistance &= 0ohm..70mohm; .rated_current &= 2.5A }}\n\
    .buck5: Sw {{ .efficiency &= 0.85..0.95; .min_input &= 4.5V; .vout.voltage &= 5V }}\n\
    .buck38: Sw {{ .efficiency &= 0.8..0.9; .min_input &= 4.5V; .vout.voltage &= 3.8V }}\n\
    .ldo: Ldo {{ .min_input &= 3.6V; .vout.voltage &= 3.3V }}\n\
    .mcu: Mcu {{ .vdd.current &= 10mA..20mA }}\n\
{}{}{}\
    .supply.pos ~> .fuse ~> .diode ~> .shunt ~> .bead ~> .buck5.vin.pos\n\
    .bead.b ~ .buck38.vin.pos\n\
    .bead.b ~ .mcu.vdd.pos\n\
    .supply.neg ~ .buck5.vin.neg\n\
    .supply.neg ~ .buck38.vin.neg\n\
    .supply.neg ~ .mcu.vdd.neg\n\
    .buck5.vout ~ .out5\n\
    .buck5.vout.pos ~ .ldo.vin.pos\n\
    .buck5.vout.neg ~ .ldo.vin.neg\n\
    .ldo.vout ~ .out33\n\
    .buck38.vout ~ .out38\n\
{hand_written}",
        supply("9V..16V"),
        load("out5", "V5", "0.1A..0.3A"),
        load("out33", "V33", "20mA..50mA"),
        load("out38", "V38", "0.2A..0.4A"),
    ))
}

/// The chain's current and the input voltage of the stages worked out here, apart from the
/// solver: every corner of the battery, the diode's drop, the bead, the powers and the
/// efficiencies is a fixed point `I = P(Vin) + mcu` with `Vin = Vsrc - vf - I * R`, solved by
/// iterating from zero. Returns the range of the current and of the input voltage.
fn tree_by_hand() -> ((f64, f64), (f64, f64)) {
    let (mut i_lo, mut i_hi) = (f64::MAX, f64::MIN);
    let (mut v_lo, mut v_hi) = (f64::MAX, f64::MIN);
    for vsrc in [9.0, 16.0] {
        for vf in [0.5, 0.8] {
            for bead in [1e-3, 0.07] {
                // The 5 V stage carries its own 0.1 A to 0.3 A and the linear regulator's input
                // current, the 3.3 V load's 20 mA to 50 mA: 0.12 A to 0.35 A.
                for iout5 in [0.12, 0.35] {
                    for e5 in [0.85, 0.95] {
                        for iout38 in [0.2, 0.4] {
                            for e38 in [0.8, 0.9] {
                                for mcu in [0.01, 0.02] {
                                    let r = 0.03 + 0.05 + bead;
                                    let mut i = 0.0;
                                    for _ in 0..200 {
                                        let vin = vsrc - vf - r * i;
                                        i = 5.0 * iout5 / (e5 * vin)
                                            + 3.8 * iout38 / (e38 * vin)
                                            + mcu;
                                    }
                                    let vin = vsrc - vf - r * i;
                                    i_lo = i_lo.min(i);
                                    i_hi = i_hi.max(i);
                                    v_lo = v_lo.min(vin);
                                    v_hi = v_hi.max(vin);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    ((i_lo, i_hi), (v_lo, v_hi))
}

#[test]
fn the_power_tree_is_solved_and_agrees_with_a_calculation_made_apart() {
    let s = solved(&power_tree(false));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.dc.regulators, 3, "{:?}", s.dc);
    let (i, v) = tree_by_hand();
    // The chain carries one current, and every part of it takes the solved range.
    for p in [
        "fuse.current",
        "diode.current",
        "shunt.current",
        "bead.current",
    ] {
        near(range(&s, p).unwrap(), i);
    }
    // The input of the two stages on the chain is the battery less the drops at that current.
    for p in ["buck5.vin.voltage", "buck38.vin.voltage"] {
        near(range(&s, p).unwrap(), v);
    }
    // Each drop is the current through its resistance, and the diode's is its own: 5 mohm is
    // `0.105 A * 30 mohm`, 15.2 mV is `0.507 A * 30 mohm`.
    near(range(&s, "fuse.drop").unwrap(), (i.0 * 0.03, i.1 * 0.03));
    near(range(&s, "shunt.drop").unwrap(), (i.0 * 0.05, i.1 * 0.05));
    near(range(&s, "diode.drop").unwrap(), (0.5, 0.8));
    // The linear regulator is on the 5 V stage's output: its input is that output, and it
    // draws what its load does.
    near(range(&s, "ldo.vin.current").unwrap(), (0.02, 0.05));
    near(range(&s, "ldo.vin.voltage").unwrap(), (5.0, 5.0));
    // The chain cannot give less than 8.12 V at the stages: a seed that starts at 7 V (issue
    // #153) is 1.1 V too low, and the analysis says so.
    assert!(range(&s, "buck5.vin.voltage").unwrap().0 > 8.1);
}

/// The board that states the chain's voltage by hand and seeds the input of the first stage, as
/// the real boards do today (issue #153), builds without a message and has the same numbers:
/// the equation is evaluated on the drops the analysis wrote, and the seed holds the solved
/// range. Both are redundant now, and are left in.
#[test]
fn the_power_tree_with_the_hand_written_equations_agrees() {
    let by_hand = solved(&power_tree(true));
    let solved_alone = solved(&power_tree(false));
    assert!(messages(&by_hand).is_empty(), "{:?}", messages(&by_hand));
    for p in [
        "fuse.current",
        "diode.current",
        "shunt.current",
        "bead.current",
    ] {
        near(
            range(&by_hand, p).unwrap(),
            range(&solved_alone, p).unwrap(),
        );
    }
    // The seed `7V..16V` and the equation narrow the input to what the analysis says, to a
    // part in a million: the solved range is inside both.
    let hand = range(&by_hand, "buck5.vin.voltage").unwrap();
    let alone = range(&solved_alone, "buck5.vin.voltage").unwrap();
    near(hand, alone);
    assert!(hand.0 >= 7.0 && hand.1 <= 16.0);
}

/// A pull-up to a line that only an input reads, on a regulator's output: the pin draws nothing
/// (ADR 0027), so the DC analysis, which now sees the net behind the regulator, solves it to no
/// current at all. A driver that may pull the line low has to be on the net for the pull-up to
/// carry up to the supply over its resistance (stage 2a, choice 3 of the pull request).
#[test]
fn a_pull_up_to_an_input_pin_behind_a_regulator_carries_nothing() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    .reg: Ldo {{ .min_input &= 4.5V; .vout.voltage &= 3.3V }}\n    .pull: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .reader: Reader\n    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout.pos ~> .pull ~> .reader.p\n    .reg.vout.neg ~ .supply.neg\n    .reader.vdd.pos ~ .reg.vout.pos\n    .reader.vdd.neg ~ .supply.neg\n",
        supply("12V")
    ))
    .replace(
        "part Mcu {",
        "part Reader {\n    .vdd: Rail\n    .vdd.current &= 1mA..2mA\n    pins {\n        .vdd.pos power \"VDD\" \"1\"\n        .vdd.neg power \"GND\" \"2\"\n        .p input \"P\" \"3\"\n    }\n}\n\npart Mcu {",
    )
    .replace(
        "import std::sources::Rail",
        "import std::sources::Rail\nimport std::parts::passive::Resistor",
    );
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let (lo, hi) = range(&s, "pull.current").unwrap();
    assert!(lo.abs() < 1e-11 && hi.abs() < 1e-11, "{lo} {hi}");
    // What the regulator supplies is the reader's supply current, 1 mA to 2 mA.
    near(range(&s, "link.current").unwrap(), (0.001, 0.002));
}

/// What flows into a net, over the corners (ADR 0076, amendment 8). A 9 V to 16 V supply behind a
/// 0.5 ohm link feeds a switching regulator (5 W at 90 %) and a resistor of 10 ohm. The regulator
/// draws most at 9 V and the resistor at 16 V, so the sum of each at its largest is more than the
/// net ever carries: at 16 V the link carries 1.92 A, and 0.677 A + 1.56 A is 2.24 A.
#[test]
fn the_current_into_a_net_is_the_most_over_the_corners_and_not_every_load_at_its_largest() {
    let src = board(&format!(
        "{}    .link: Link {{ .resistance &= 0.5ohm; .rated_current &= 5A }}\n    .rl: Link {{ .resistance &= 10ohm; .rated_current &= 5A }}\n    .reg: Sw {{ .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V }}\n{}    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vin.pos ~> .rl ~> .supply.neg\n    .reg.vout ~ .out\n",
        supply("9V..16V"),
        load("out", "V5", "1A")
    ));
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let total = |v: f64| {
        let mut i = 0.0;
        for _ in 0..200 {
            let vx = v - 0.5 * i;
            i = 5.0 / (0.9 * vx) + vx / 10.0;
        }
        i
    };
    let want = total(9.0).max(total(16.0));
    let net = s
        .model
        .nets()
        .iter()
        .find(|n| n.terminals.iter().any(|&p| s.model.path_of(p) == "rl.a"))
        .expect("the net of rl.a");
    let found = s
        .net_currents
        .iter()
        .find(|c| net.pins.contains(&c.pin))
        .expect("the analysis solved the net");
    assert!(
        (found.current - want).abs() <= 2e-6 * want + 1e-9,
        "{} vs {want}",
        found.current
    );
    let plain = range(&s, "reg.vin.current").unwrap().1 + range(&s, "rl.current").unwrap().1;
    assert!(plain > want * 1.1, "{plain} should be well above {want}");
}
