//! A divider behind a part in series, on a net with two unknown currents (issue #18, ADR 0057).
//!
//! The divider's draw needs its input voltage, which needs the drop across the series part,
//! which needs that part's current: what the net carries to the divider and to whatever else
//! is on the net. The solver works a part's current out only from a net of two currents
//! (ADR 0026), so the part's current is never set and every check on the divider waits. The
//! values form a circle that runs through a balance, and it is named like any other open circle.
//! Every value here is invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// A divider (two resistors, a limit on its draw) behind `series`, on a supply of 12 V to 20 V.
/// `drop` is what the parts in series drop. `extra` goes into the board.
fn board(series: &str, chain: &str, drop: &str, extra: &str) -> String {
    format!(
        r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::parts::passive::Fuse
import std::parts::diode::Diode
import std::sources::Rail

module Divider {{
    .vin: Rail
    .mid: Pin
    .max_draw: Current
    .top: Resistor
    .bottom: Resistor
    .vin.pos ~> .top ~> .mid
    .mid ~> .bottom ~> .vin.neg
    .top.current == .vin.voltage / (.top.resistance + .bottom.resistance)
    .bottom.current == .top.current
    .top.current <= .max_draw
}}

board Behind {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        .voltage &= 12V..20V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
{series}
    .after: Rail {{
        .pos.net &= "AFTER"
        .neg.net &= "GND"
    }}
    .supply.neg ~ .after.neg
    .supply.pos ~> {chain} ~> .after.pos
    .after.voltage == .supply.voltage - {drop}
    .divider: Divider {{ .max_draw &= 30uA }}
    .divider.mid.net &= "MID"
    .after ~ .divider.vin{extra}
}}
"#
    )
}

const SERIES_DROP: &str = ".series.drop";
const RESISTOR: &str = "    .series: Resistor { .resistance &= 33ohm +/- 1% }";
const PROTECTION: &str = "    .fuse: Fuse { .rated_current &= 500mA; .resistance &= 0.2ohm }\n    .diode: Diode { .forward_voltage &= 0.5V..0.8V }";

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn names(s: &Solution, fields: &[netsmith_check::FieldId]) -> Vec<String> {
    let mut v: Vec<String> = fields.iter().map(|&f| s.model.path_of_field(f)).collect();
    v.sort();
    v
}

fn checks(s: &Solution) -> Vec<String> {
    s.open_circles
        .iter()
        .flat_map(|c| c.checks.iter())
        .map(|&i| s.model.show_expr(&s.model.relations()[i].lhs))
        .collect()
}

/// The two resistors of the divider that the solver picked, by value.
fn divider_values(s: &Solution) -> Vec<(String, String)> {
    s.picks
        .iter()
        .filter(|p| s.model.path_of(p.part).starts_with("divider."))
        .map(|p| (s.model.path_of(p.part), p.label.clone()))
        .collect()
}

#[test]
fn a_divider_behind_a_part_on_a_net_with_two_unknown_currents_is_an_open_circle() {
    let s = solved(&board(RESISTOR, ".series", SERIES_DROP, ""));
    assert!(
        s.circular.is_empty(),
        "no circle of equations: {:?}",
        s.circular
    );
    assert_eq!(s.open_circles.len(), 1, "{:?}", s.open_circles);
    let c = &s.open_circles[0];
    // The draw limit is one of the checks that wait on it.
    assert!(
        checks(&s).iter().any(|c| c == "divider.top.current"),
        "{:?}",
        checks(&s)
    );
    // The net, its three currents, and the one a range would start.
    assert_eq!(c.nets.len(), 1);
    assert_eq!(c.nets[0].net, "AFTER");
    assert_eq!(
        names(&s, &c.nets[0].unknown),
        ["after.current", "divider.vin.current", "series.current"]
    );
    assert_eq!(names(&s, &c.nets[0].unsolved), ["series.current"]);
    assert!(c.closed);
    // The limit is not applied to the pick: that is what the finding says.
    assert_eq!(
        divider_values(&s),
        [
            ("divider.bottom".to_string(), "330".to_string()),
            ("divider.top".to_string(), "330".to_string())
        ]
    );
}

#[test]
fn a_chain_of_a_fuse_and_a_diode_is_the_same_circle() {
    let s = solved(&board(
        PROTECTION,
        ".fuse ~> .diode",
        ".fuse.drop + .diode.drop",
        "",
    ));
    assert_eq!(s.open_circles.len(), 1, "{:?}", s.open_circles);
    let net = &s.open_circles[0].nets[0];
    let unsolved = names(&s, &net.unsolved);
    assert!(
        unsolved.iter().any(|f| f == "diode.current"),
        "{unsolved:?}"
    );
    assert!(checks(&s).iter().any(|c| c == "divider.top.current"));
}

#[test]
fn a_voltage_start_or_a_range_on_the_part_current_ends_it() {
    for start in [
        "\n    .divider.vin.voltage &= 0V..20V",
        "\n    .series.current &= 0A..3mA",
    ] {
        let s = solved(&board(RESISTOR, ".series", SERIES_DROP, start));
        assert!(s.open_circles.is_empty(), "{start}: {:?}", s.open_circles);
        // The draw is judged now: the pair is picked so that 20 V over it stays under 30 uA.
        let values = divider_values(&s);
        assert_ne!(values[1].1, "330", "{start}: {values:?}");
    }
}

#[test]
fn a_current_range_on_another_term_of_the_net_does_not_end_it() {
    // The outside draw is known but the part's current is a balance of three terms still.
    let s = solved(&board(
        RESISTOR,
        ".series",
        SERIES_DROP,
        "\n    .after.current &= 0A..1mA",
    ));
    assert_eq!(s.open_circles.len(), 1, "{:?}", s.open_circles);
    assert_eq!(
        names(&s, &s.open_circles[0].nets[0].unknown),
        ["divider.vin.current", "series.current"]
    );
}

#[test]
fn nothing_else_on_the_net_makes_it_a_circle_of_equations_that_a_start_ends() {
    // A term that is exactly zero is not a term: the part's current is the divider's.
    let src = board(
        RESISTOR,
        ".series",
        SERIES_DROP,
        "\n    .after.current &= 0A",
    );
    let s = solved(&src);
    assert!(
        s.open_circles.iter().all(|c| c.nets.is_empty()),
        "{:?}",
        s.open_circles
    );
    assert_eq!(s.circular.len(), 1, "{:?}", s.circular);
    let started = board(
        RESISTOR,
        ".series",
        SERIES_DROP,
        "\n    .after.current &= 0A\n    .divider.vin.voltage &= 0V..20V",
    );
    let s = solved(&started);
    assert!(s.open_circles.is_empty(), "{:?}", s.open_circles);
    assert!(s.circular.is_empty());
    assert!(s.deferred.iter().all(|&i| {
        let r = &s.model.relations()[i];
        !s.model.show_expr(&r.lhs).starts_with("divider.top.current")
    }));
}

/// A part in series with two rails on one net: one draws 10 mA to 20 mA, the other `other`.
fn two_loads(other: &str) -> String {
    format!(
        r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

board Two {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        .voltage &= 12V..20V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .series: Resistor {{ .resistance &= 33ohm +/- 1% }}
    .a: Rail {{
        .current &= 10mA..20mA
        .pos.net &= "LOAD"
        .neg.net &= "GND"
    }}
    .b: Rail {{
        .current &= {other}
        .pos.net &= "LOAD"
        .neg.net &= "GND"
    }}
    .a ~ .b
    .supply.neg ~ .a.neg
    .supply.pos ~> .series ~> .a.pos
}}
"#
    )
}

fn value_of(s: &Solution, path: &str) -> String {
    let (f, _) = s
        .model
        .fields()
        .find(|(f, _)| s.model.path_of_field(*f) == path)
        .unwrap();
    format!("{:?}", s.model.field(f).domain)
}

#[test]
fn a_part_current_is_set_from_a_net_whose_other_term_is_zero() {
    // A term that is exactly zero is no term: the net has two, and they are equal.
    let s = solved(&two_loads("0A"));
    let v = value_of(&s, "series.current");
    assert!(v.contains("Quantity") && !v.contains("Unset"), "{v}");
    assert!(value_of(&s, "series.drop").contains("Quantity"));
    // With a range as the other term it is a balance of three, as before: never set.
    let s = solved(&two_loads("0A..1mA"));
    assert!(value_of(&s, "series.current").contains("Unset"));
}

#[test]
fn a_net_with_unknown_currents_and_no_circle_of_equations_is_quiet() {
    // The load's voltage depends on the drop, and nothing depends on the load: a value
    // nothing states, not a circle (the note says so, and nothing more is said).
    let src = r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

board Quiet {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 12V..20V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .series: Resistor { .resistance &= 33ohm +/- 1% }
    .load: Rail {
        .pos.net &= "LOAD"
        .neg.net &= "GND"
    }
    .supply.neg ~ .load.neg
    .supply.pos ~> .series ~> .load.pos
    .load.voltage == .supply.voltage - .series.drop
}
"#;
    let s = solved(src);
    assert!(!s.deferred.is_empty());
    assert!(s.open_circles.is_empty(), "{:?}", s.open_circles);
}
