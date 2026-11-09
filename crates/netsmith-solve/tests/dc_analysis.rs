//! The DC analysis of the resistive islands (ADR 0076, phase 2, stage 2a).
//!
//! The pass solves the network of the picked parts (resistors, a fuse, a diode, a board's own
//! supplies, and what a part draws as a current source) over every corner of its ranges, and
//! writes `.current` and `.drop` where nothing states them. The relations that waited are then
//! judged with numbers. A stated range is a budget that the circuit has to fit. Every part and
//! value here is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn board(body: &str) -> String {
    format!(
        r#"import std::parts::passive::{{Resistor, Fuse, Led, Inductor}}
import std::parts::diode::Diode
import std::sources::Rail

part Mcu {{
    .vdd: Rail
}}

part Supplier {{
    .out: Rail
}}

board B {{
    .supply: Rail {{
        .voltage &= {SUPPLY}
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
{body}
}}
"#
    )
}

const SUPPLY: &str = "9V..16V";

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn domain(s: &Solution, path: &str) -> Domain {
    s.model
        .domain_at(path)
        .unwrap_or_else(|| panic!("no field {path}"))
        .clone()
}

/// The range of a field in the model, as (lo, hi), or `None` while it has no value.
fn range(s: &Solution, path: &str) -> Option<(f64, f64)> {
    match domain(s, path) {
        Domain::Quantity(q) => Some((q.lo(), q.hi())),
        _ => None,
    }
}

fn near(got: (f64, f64), want: (f64, f64)) {
    let ok = |a: f64, b: f64| (a - b).abs() <= 1e-6 * b.abs().max(1e-9);
    assert!(
        ok(got.0, want.0) && ok(got.1, want.1),
        "got {got:?}, want {want:?}"
    );
}

fn waiting(s: &Solution) -> Vec<String> {
    s.deferred
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
        .collect()
}

/// A divider off a rail, with an LED on its middle: the top resistor carries what the bottom
/// one takes and what the LED draws. The equations cannot say it (the middle net has two
/// currents that nothing states, ADR 0026); the network can.
#[test]
fn a_boundary_with_a_stated_current_is_a_load_on_a_divider() {
    let src = board(
        "    .top: Resistor { .resistance &= 1kohm +/- 1% }\n    .bottom: Resistor { .resistance &= 2kohm +/- 1% }\n    .led: Led { .current &= 1mA..2mA; .forward_voltage &= 1.8V..2.4V }\n    .supply.pos ~ .top.a\n    .top.b ~ .bottom.a\n    .top.b ~ .led.anode\n    .bottom.b ~ .supply.neg\n    .led.cathode ~ .supply.neg\n",
    )
    .replace(SUPPLY, "5V");
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.dc.solved, 1, "{:?}", s.dc);
    // The middle is `(5 / R1 - I) / (1 / R1 + 1 / R2)`, and the top current `(5 - mid) / R1`.
    let top = |r1: f64, r2: f64, led: f64| {
        let mid = (5.0 / r1 - led) / (1.0 / r1 + 1.0 / r2);
        (5.0 - mid) / r1
    };
    let mut all = Vec::new();
    for r1 in [990.0, 1010.0] {
        for r2 in [1980.0, 2020.0] {
            for led in [0.001, 0.002] {
                all.push(top(r1, r2, led));
            }
        }
    }
    let lo = all.iter().cloned().fold(f64::MAX, f64::min);
    let hi = all.iter().cloned().fold(f64::MIN, f64::max);
    let (got_lo, got_hi) = range(&s, "top.current").expect("top.current has a value");
    assert!(
        (got_lo - lo).abs() < 1e-8 + 1e-6 * lo,
        "{got_lo} against {lo}"
    );
    assert!(
        (got_hi - hi).abs() < 1e-8 + 1e-6 * hi,
        "{got_hi} against {hi}"
    );
    // The relations on the resistors are judged with numbers: nothing waits for a current.
    assert!(
        waiting(&s)
            .iter()
            .all(|w| !w.contains("top.") && !w.contains("bottom.")),
        "{:?}",
        waiting(&s)
    );
    assert!(range(&s, "bottom.current").is_some());
    assert!(range(&s, "bottom.drop").is_some());
}

/// A fuse, a diode and a load: the chain's current is the load's, the fuse's and the diode's
/// ratings are judged, and the drop of each is the current through its resistance or its forward
/// voltage. Without the pass the three relations of the fuse and the diode wait (nothing states
/// the current: the net has the rail's and the load's). The supply states its peak, which the fuse
/// takes (ADR 0086), so its rated voltage is judged too.
#[test]
fn a_fuse_and_a_diode_in_front_of_a_load_are_judged_with_numbers() {
    let src = board(
        "    .supply.peak &= 24V\n    .fuse: Fuse { .rated_current &= 3A }\n    .diode: Diode { .forward_voltage &= 0.5V..0.8V; .max_forward_current &= 1A; .max_reverse_voltage &= 40V }\n    .mcu: Mcu { .vdd.current &= 50mA..100mA }\n    .aux: Mcu { .vdd.current &= 50mA..200mA }\n    .supply.pos ~> .fuse ~> .diode ~> .mcu.vdd.pos\n    .diode.cathode ~ .aux.vdd.pos\n    .supply.neg ~ .mcu.vdd.neg\n    .supply.neg ~ .aux.vdd.neg\n",
    );
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.dc.solved, 1, "{:?}", s.dc);
    near(range(&s, "fuse.current").unwrap(), (0.1, 0.3));
    near(range(&s, "diode.current").unwrap(), (0.1, 0.3));
    // The fuse drops its current through its resistance: the picked row's cold resistance is
    // 32 mohm (the 3 A, 125 V row, the one that interrupts the supply's 24 V peak), so
    // `0.1 A * 0.032 ohm` to `0.3 A * 0.032 ohm`.
    let (lo, hi) = range(&s, "fuse.drop").unwrap();
    assert!(
        (lo - 0.0032).abs() < 1e-6 && (hi - 0.0096).abs() < 1e-6,
        "{lo} {hi}"
    );
    assert!(
        waiting(&s)
            .iter()
            .all(|w| !w.contains("fuse.") && (!w.contains("diode.") || !w.contains("current"))),
        "{:?}",
        waiting(&s)
    );
}

/// A diode that cannot block: its anode is 9 V or more above its cathode with a load that always
/// draws behind it, so the state is refused and the current is the load's. The diode's current
/// is never negative.
#[test]
fn a_diode_keeps_a_current_of_at_least_zero() {
    let src = board(
        "    .diode: Diode { .forward_voltage &= 0.5V..0.8V; .max_forward_current &= 1A; .max_reverse_voltage &= 40V }\n    .mcu: Mcu { .vdd.current &= 0mA..300mA }\n    .supply.pos ~> .diode ~> .mcu.vdd.pos\n    .supply.neg ~ .mcu.vdd.neg\n",
    );
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let (lo, hi) = range(&s, "diode.current").unwrap();
    assert!(lo >= 0.0, "{lo}");
    assert!((hi - 0.3).abs() < 1e-6, "{hi}");
}

/// A range the design states for a part is a budget (ADR 0038): the circuit has to fit inside
/// it, and when it does not the error names the states that give what it needs.
#[test]
fn a_stated_current_that_the_circuit_exceeds_is_an_error_with_its_states() {
    let src = board(
        "    .fuse: Fuse { .rated_current &= 1A; .current &= 0A..100mA }\n    .mcu: Mcu { .vdd.current &= 50mA..300mA }\n    .supply.pos ~> .fuse ~> .mcu.vdd.pos\n    .supply.neg ~ .mcu.vdd.neg\n",
    );
    let s = solved(&src);
    let found: Vec<String> = messages(&s)
        .into_iter()
        .filter(|m| m.contains("the DC analysis"))
        .collect();
    assert_eq!(found.len(), 1, "{:?}", messages(&s));
    let m = &found[0];
    assert!(
        m.starts_with("`fuse.current` is stated 0A..100mA, but the DC analysis of the circuit around it finds 50mA..300mA"),
        "{m}"
    );
    assert!(m.contains("at most 300mA at the high end with"), "{m}");
    // The states that matter are named, and no others: the load's current is the load's,
    // whatever the supply and the fuse are, so neither is named.
    assert!(
        m.ends_with("at most 300mA at the high end with mcu.vdd at 300mA"),
        "{m}"
    );
    // The stated range is left as written.
    near(range(&s, "fuse.current").unwrap(), (0.0, 0.1));
}

/// A stated range that holds the circuit's is no finding, and is left as stated.
#[test]
fn a_stated_current_that_holds_the_circuit_is_left_alone() {
    let src = board(
        "    .fuse: Fuse { .rated_current &= 1A; .current &= 0A..1A }\n    .mcu: Mcu { .vdd.current &= 50mA..300mA }\n    .supply.pos ~> .fuse ~> .mcu.vdd.pos\n    .supply.neg ~ .mcu.vdd.neg\n",
    );
    let s = solved(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    near(range(&s, "fuse.current").unwrap(), (0.0, 1.0));
}

/// A rating the circuit exceeds is an error that carries the states (ADR 0076, section 1).
#[test]
fn a_rating_the_circuit_exceeds_names_the_states_that_give_the_current() {
    let src = board(
        "    .fuse: Fuse { .rated_current &= 1A }\n    .mcu: Mcu { .vdd.current &= 550mA..750mA }\n    .aux: Mcu { .vdd.current &= 550mA..750mA }\n    .supply.pos ~> .fuse ~> .mcu.vdd.pos\n    .fuse.b ~ .aux.vdd.pos\n    .supply.neg ~ .mcu.vdd.neg\n    .supply.neg ~ .aux.vdd.neg\n",
    );
    let s = solved(&src);
    let m: Vec<String> = messages(&s)
        .into_iter()
        .filter(|m| m.contains("rated_current"))
        .collect();
    assert_eq!(m.len(), 1, "{:?}", messages(&s));
    assert!(
        m[0].contains("the DC analysis puts `fuse.current` at"),
        "{}",
        m[0]
    );
    assert!(m[0].contains("its high end with"), "{}", m[0]);
}

/// An island with more combinations than the limit is a note that names its parts, and is left
/// to the bound from Ohm's law: nothing is written.
#[test]
fn an_island_too_large_to_cut_is_a_note_and_writes_nothing() {
    let n = 18;
    let mut body = String::new();
    for k in 0..n {
        body.push_str(&format!(
            "    .r{k}: Resistor {{ .resistance &= 1kohm +/- 1% }}\n"
        ));
    }
    let chain: Vec<String> = (0..n).map(|k| format!(".r{k}")).collect();
    body.push_str(&format!(
        "    .supply.pos ~> {} ~> .supply.neg\n",
        chain.join(" ~> ")
    ));
    let s = solved(&board(&body).replace(SUPPLY, "5V +/- 1%"));
    assert_eq!(s.dc.too_large, 1, "{:?}", s.dc);
    assert_eq!(s.dc.solved, 0);
    let notes: Vec<&str> = s
        .not_evaluated
        .iter()
        .map(|n| n.message.as_str())
        .filter(|m| m.contains("the DC analysis left"))
        .collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].contains("`r0`") && notes[0].contains("more than the limit of"),
        "{}",
        notes[0]
    );
    assert!(
        notes[0].contains("cut it at a node whose voltage a rail holds"),
        "{}",
        notes[0]
    );
    assert_eq!(s.dc.written, 0);
}

/// A part that can give current holds its net's voltage, which this stage does not model: a
/// resistor on such a net is left as it was.
#[test]
fn a_net_an_output_rail_feeds_is_not_solved() {
    let src = board(
        "    .src: Supplier { .out.current &= -50mA..-10mA }\n    .pull: Resistor { .resistance &= 10kohm +/- 1% }\n    .src.out.pos ~> .pull ~> .src.out.neg\n    .supply.neg ~ .src.out.neg\n",
    );
    let s = solved(&src);
    assert_eq!(s.dc.solved, 0, "{:?}", s.dc);
    assert_eq!(s.dc.written, 0);
}

/// A part with a `bridge` and neither an element nor a current is a boundary: the network stops
/// there, and what depends on it waits, as it did.
#[test]
fn a_boundary_without_a_current_leaves_the_island_alone() {
    let src = board(
        "    .r1: Resistor { .resistance &= 100ohm +/- 1% }\n    .lamp: Led\n    .supply.pos ~> .r1 ~> .lamp ~> .supply.neg\n",
    );
    let s = solved(&src);
    assert_eq!(s.dc.solved, 0, "{:?}", s.dc);
    assert_eq!(range(&s, "r1.current"), None);
}

/// The pass is deterministic: the same design gives the same ranges and the same messages.
#[test]
fn the_same_design_gives_the_same_numbers() {
    let src = board(
        "    .fuse: Fuse { .rated_current &= 1A }\n    .diode: Diode { .forward_voltage &= 0.5V..0.8V; .max_forward_current &= 1A; .max_reverse_voltage &= 40V }\n    .mcu: Mcu { .vdd.current &= 100mA..300mA }\n    .supply.pos ~> .fuse ~> .diode ~> .mcu.vdd.pos\n    .supply.neg ~ .mcu.vdd.neg\n",
    );
    let a = solved(&src);
    let b = solved(&src);
    for path in ["fuse.current", "fuse.drop", "diode.current", "diode.drop"] {
        assert_eq!(domain(&a, path), domain(&b, path), "{path}");
    }
    assert_eq!(messages(&a), messages(&b));
    assert_eq!(a.dc, b.dc);
}
