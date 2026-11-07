//! The parts a board gets do not depend on what its instances are called (ADR 0065, section
//! 11). Two modules read each other's values through bounds, and the solver picks parts group by
//! group in path order. The group that came first was picked while the bound waited for the
//! other, so its first fit could leave nothing for the other, and the board built or failed by
//! the name of an instance. Every value here is invented.

use netsmith_check::{Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Candidate, Catalog, Catalogs, Solution, Value, solve};

fn resistor(ohms: f64) -> Candidate {
    let label = format!("{ohms}");
    let center = Quantity::literal(ohms, Some("ohm")).unwrap().quantity;
    let one_percent = Quantity::literal(1.0, Some("%")).unwrap().quantity;
    Candidate {
        name: format!("{label} 1%"),
        label,
        columns: vec![
            (
                "resistance".into(),
                Value::Quantity(Quantity::tolerance(&center, &one_percent, true).unwrap()),
            ),
            ("package".into(), Value::Text("0402".into())),
            (
                "max_power".into(),
                Value::Quantity(Quantity::literal(1.0, Some("kW")).unwrap().quantity),
            ),
        ],
        lcsc: None,
        mpn: None,
        library: None,
        assembly: None,
        generic: None,
    }
}

fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: [330.0, 1_000.0, 10_000.0, 100_000.0, 1_000_000.0]
            .iter()
            .map(|&v| resistor(v))
            .collect(),
    });
    c
}

/// A module that divides a cell's voltage down (its member `sense` is the tap, as in a module
/// that names its own pin like the instance a board gives it), and a reference module; the board
/// ties the tap's voltage to a fraction of the reference's. `tap` and `reference` are the names
/// of the two instances.
fn source(tap: &str, reference: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail

module Reference {{
    .supply: Rail
    .out: Voltage
    .top: Resistor
    .bottom: Resistor
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
    .out == .supply.voltage * .bottom.resistance / (.top.resistance + .bottom.resistance)
    .out >= 2.8V
    .out <= 3.4V
    .top.resistance &= 1kohm..100kohm
}}

module Tap {{
    .supply: Rail
    .sense: Pin
    .cell_max_voltage: Voltage
    .max_draw: Current
    .sense_at_cell_max: Voltage
    .draw_at_cell_max: Current
    .sense_top: Resistor
    .sense_bottom: Resistor
    .supply.pos ~> .sense_top ~> .sense_bottom ~> .supply.neg
    .sense_top.b ~ .sense
    .sense_at_cell_max == .cell_max_voltage * .sense_bottom.resistance / (.sense_top.resistance + .sense_bottom.resistance)
    .draw_at_cell_max == .cell_max_voltage / (.sense_top.resistance + .sense_bottom.resistance)
    .draw_at_cell_max <= .max_draw
}}

board B {{
    .supply: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }}
    .{tap}: Tap
    .{tap}.supply ~ .supply
    .{tap}.cell_max_voltage &= 5.4V..5.5V
    .{tap}.max_draw &= 10uA
    .{tap}.sense_at_cell_max <= .{reference}.out
    .{tap}.sense_at_cell_max >= 0.75 * .{reference}.out
    .{reference}: Reference
    .{reference}.supply ~ .supply
}}
"#
    )
}

fn run(src: &str) -> Solution {
    let c = check("main.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], &catalogs())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The picks by the part's name inside its instance (`sense_top`), so two names compare.
fn by_role(s: &Solution, tap: &str, reference: &str) -> Vec<String> {
    let mut picks: Vec<String> = s
        .picks
        .iter()
        .map(|p| {
            let path = s.model.path_of(p.part);
            let path = path
                .strip_prefix(&format!("{tap}."))
                .map(|r| format!("tap.{r}"))
                .or_else(|| {
                    path.strip_prefix(&format!("{reference}."))
                        .map(|r| format!("reference.{r}"))
                })
                .unwrap_or(path);
            format!("{path} = {}", p.label)
        })
        .collect();
    picks.sort();
    picks
}

const NAMES: [(&str, &str); 6] = [
    ("a", "b"),
    ("b", "a"),
    ("sense", "ref"),
    ("sense", "zref"),
    ("asense", "ref"),
    ("supercap_sense", "ref"),
];

#[test]
fn the_same_board_gets_the_same_parts_whatever_its_instances_are_called() {
    let mut results: Vec<(String, Vec<String>)> = Vec::new();
    for (tap, reference) in NAMES {
        let s = run(&source(tap, reference));
        assert!(
            messages(&s).is_empty(),
            "{tap} / {reference}: {:?}",
            messages(&s)
        );
        results.push((format!("{tap} / {reference}"), by_role(&s, tap, reference)));
    }
    assert_eq!(results[0].1.len(), 4, "{results:?}");
    for (names, picks) in &results {
        assert_eq!(picks, &results[0].1, "{names} against {}", results[0].0);
    }
}

#[test]
fn a_board_for_which_no_order_works_keeps_the_first_failure() {
    // The reference cannot meet its own window: nothing the other module does helps, and the
    // message is the one the search gave, naming the reference's parts.
    let src = source("a", "b").replace(".out >= 2.8V", ".out >= 3.6V");
    let s = run(&src);
    let m = messages(&s);
    assert!(
        m.iter()
            .any(|m| m.contains("`b.bottom`") || m.contains("b.top")),
        "{m:#?}"
    );
}

#[test]
fn a_retry_that_fails_at_a_later_group_leaves_the_first_failure_whole() {
    // The reference cannot meet the tap's bound (the tap's own bound, `>= 4.5V`, which no row of
    // the reference's divider can meet) and the retry, with the reference first, fails at the
    // tap, which is a later group of the retry. The message and the parts without a pick are
    // those of the first attempt: the reference's, as the tap was searched first and fit.
    let src = source("a", "b").replace(
        "    .b: Reference",
        "    .a.sense_at_cell_max >= 4.5V\n    .b: Reference",
    );
    let s = run(&src);
    let m = messages(&s);
    assert!(
        m.iter()
            .any(|m| m.contains("b.bottom") && m.contains("b.top") && m.contains("b.out")),
        "{m:#?}"
    );
    assert!(
        !m.iter().any(|m| m.contains("a.sense_bottom")),
        "the message of the second attempt stands: {m:#?}"
    );
    let unresolved: Vec<String> = s
        .unresolved
        .iter()
        .map(|u| s.model.path_of(u.part))
        .collect();
    for part in ["b.top", "b.bottom"] {
        assert!(unresolved.contains(&part.to_string()), "{unresolved:?}");
    }
    for p in &s.picks {
        let path = s.model.path_of(p.part);
        assert!(
            !unresolved.contains(&path),
            "{path} has a pick and is listed as unresolved: {unresolved:?}"
        );
    }
}
