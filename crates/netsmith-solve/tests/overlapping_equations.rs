//! A value that two equations state, both worked out from picks, takes what the two equations
//! share even when neither range lies inside the other (issue #852), also with a window among
//! them, on the field or on a field joined to it (issue #991), and the window still holds each
//! equation. Two invented resistors feed one value `.x`; a check on `.x` holds for the overlap of
//! the two equations and for neither alone. Every value here is invented.

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
        height: None,
        temp_min: None,
        temp_max: None,
        max_voltage_steps: None,
        qualification: None,
        generic: None,
    }
}

fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: [10_000.0, 15_000.0, 22_000.0]
            .iter()
            .map(|&v| resistor(v))
            .collect(),
    });
    c
}

fn run(src: &str) -> Solution {
    let c = check("main.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], &catalogs())
}

/// Both resistors are 15 kohm at 1 %: `.r1.resistance` is 14.85 to 15.15 kohm and
/// `.r2.resistance * 0.995` is 14.78 to 15.07 kohm, so `.x` is 14.85 to 15.07 kohm. `first` and
/// `second` are the two equations in the order they are written; `checks` the lines that read
/// `.x`.
fn source(first: &str, second: &str, checks: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail

board Probe {{
    .supply: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }}
    .r1: Resistor
    .r2: Resistor
    .supply.pos ~> .r1 ~> .supply.neg
    .supply.pos ~> .r2 ~> .supply.neg
    .r1.resistance &= 14kohm..16kohm
    .r2.resistance &= 14kohm..16kohm
    .x: Resistance
    .y: Resistance
{first}
{second}
    .y == .x * 2
{checks}
}}
"#
    )
}

const BY_R1: &str = "    .x == .r1.resistance * 1";
const BY_R2: &str = "    .x == .r2.resistance * 0.995";

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> (f64, f64) {
    let (id, _) = s
        .model
        .fields()
        .find(|(id, _)| s.model.path_of_field(*id) == path)
        .unwrap_or_else(|| panic!("no field {path}"));
    match &s.model.field(id).domain {
        netsmith_check::Domain::Quantity(q) => (q.lo(), q.hi()),
        other => panic!("{path} is {other:?}"),
    }
}

#[test]
fn the_value_is_what_both_equations_allow_in_either_order() {
    for (first, second) in [(BY_R1, BY_R2), (BY_R2, BY_R1)] {
        let s = run(&source(first, second, ""));
        assert!(messages(&s).is_empty(), "{first:?}: {:?}", messages(&s));
        let (lo, hi) = range(&s, "x");
        // 15 kohm - 1 % and 0.995 * (15 kohm + 1 %), but for the rounding of the arithmetic.
        assert!((lo - 14_850.0).abs() < 1e-6, "{first:?}: {lo}");
        assert!((hi - 0.995 * 15_150.0).abs() < 1e-6, "{first:?}: {hi}");
        // What is worked out from `.x` follows it.
        let (lo, hi) = range(&s, "y");
        assert!((lo - 29_700.0).abs() < 1e-6 && (hi - 2.0 * 0.995 * 15_150.0).abs() < 1e-6);
    }
}

#[test]
fn a_check_that_holds_only_for_the_overlap_passes() {
    // Read from `.r1` alone, `.y` reaches 30.3 kohm; from `.r2` alone, it falls to 29.55 kohm.
    // Together it is 29.7 to 30.15 kohm, inside both bounds.
    let checks = "    .y >= 29.6kohm\n    .y <= 30.2kohm";
    for (first, second) in [(BY_R1, BY_R2), (BY_R2, BY_R1)] {
        let s = run(&source(first, second, checks));
        assert!(messages(&s).is_empty(), "{first:?}: {:?}", messages(&s));
        assert_eq!(s.picks.len(), 2, "{first:?}");
    }
}

#[test]
fn equations_that_share_no_value_still_fail() {
    // 10 kohm and 0.995 * 15 kohm share nothing: no overlap to narrow to, and the build says
    // they can never be equal.
    let first = "    .x == .r1.resistance * 1\n    .r1.resistance &= 9kohm..11kohm";
    let s = run(&source(first, BY_R2, "").replace(
        "    .r1.resistance &= 14kohm..16kohm\n",
        "",
    ));
    assert!(
        messages(&s)
            .iter()
            .any(|m| m.contains("`x`") && m.contains("can never be equal")),
        "{:?}",
        messages(&s)
    );
}

/// Every order of `lines`, so that a result that depends on the order the equations are written
/// in shows.
fn orders(lines: &[&'static str]) -> Vec<Vec<&'static str>> {
    if lines.len() <= 1 {
        return vec![lines.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..lines.len() {
        let mut rest = lines.to_vec();
        let first = rest.remove(i);
        for mut tail in orders(&rest) {
            tail.insert(0, first);
            out.push(tail);
        }
    }
    out
}

/// A window the board states for `.x`, around the two equations' overlap (issue #991): 14.7 to
/// 15.3 kohm holds both 14.85 to 15.15 kohm and 14.78 to 15.07 kohm.
const WINDOW: &str = "    .x == 14.7kohm..15.3kohm";

#[test]
fn with_a_window_the_value_is_what_both_equations_allow_in_every_order() {
    for order in orders(&[WINDOW, BY_R1, BY_R2]) {
        let s = run(&source(&order.join("\n"), "", ""));
        assert!(messages(&s).is_empty(), "{order:?}: {:?}", messages(&s));
        let (lo, hi) = range(&s, "x");
        assert!((lo - 14_850.0).abs() < 1e-6, "{order:?}: {lo}");
        assert!((hi - 0.995 * 15_150.0).abs() < 1e-6, "{order:?}: {hi}");
    }
}

#[test]
fn with_a_window_a_check_that_holds_only_for_the_overlap_passes() {
    let checks = "    .y >= 29.6kohm\n    .y <= 30.2kohm";
    for order in orders(&[WINDOW, BY_R1, BY_R2]) {
        let s = run(&source(&order.join("\n"), "", checks));
        assert!(messages(&s).is_empty(), "{order:?}: {:?}", messages(&s));
        assert_eq!(s.picks.len(), 2, "{order:?}");
    }
}

#[test]
fn a_window_that_one_equation_only_partly_meets_still_fails() {
    // 14.9 to 15.3 kohm holds the overlap, 14.85 to 15.07 kohm, only from 14.9 kohm on, and
    // `.r1.resistance` reaches down to 14.85 kohm: the window is not met, whichever order the
    // lines are in, and the overlap does not hide it.
    let window = "    .x == 14.9kohm..15.3kohm";
    for order in orders(&[window, BY_R1, BY_R2]) {
        let s = run(&source(&order.join("\n"), "", ""));
        assert!(
            messages(&s)
                .iter()
                .any(|m| m.contains("`x` must stay within")),
            "{order:?}: {:?}",
            messages(&s)
        );
    }
}

/// The same two equations on the voltage of a rail `.q`, and the window on the voltage of a
/// rail `.p` joined to it: a window stated through a join holds the value as one on the field
/// does. 1 mA through each resistor: `.q.voltage` is 14.85 to 15.07 V.
fn joined(window: &str) -> String {
    r#"import std::parts::passive::Resistor
import std::sources::Rail

board Probe {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .r1: Resistor
    .r2: Resistor
    .supply.pos ~> .r1 ~> .supply.neg
    .supply.pos ~> .r2 ~> .supply.neg
    .r1.resistance &= 14kohm..16kohm
    .r2.resistance &= 14kohm..16kohm
    .p: Rail
    .q: Rail
    .p ~ .q
WINDOW
    .q.voltage == .r1.resistance * 1mA
    .q.voltage == .r2.resistance * 0.995mA
}
"#
    .replace("WINDOW", window)
}

#[test]
fn a_window_through_a_join_narrows_to_the_overlap() {
    let s = run(&joined("    .p.voltage == 14.7V..15.3V"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let (lo, hi) = range(&s, "q.voltage");
    assert!((lo - 14.85).abs() < 1e-9, "{lo}");
    assert!((hi - 0.995 * 15.15).abs() < 1e-9, "{hi}");
}

#[test]
fn a_window_through_a_join_that_is_only_partly_met_fails() {
    let s = run(&joined("    .p.voltage == 14.9V..15.3V"));
    assert!(
        messages(&s)
            .iter()
            .any(|m| m.contains("must stay within") && m.contains("r1.resistance")),
        "{:?}",
        messages(&s)
    );
}
