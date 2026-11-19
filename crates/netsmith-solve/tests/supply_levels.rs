//! Input levels in rows by the supply (issues #1305 and #1307): the solver reads the rows at the
//! voltage of the level's reference rail and writes the level from the rows it reaches; a range
//! that spans two rows is judged against the worse, and a supply between two rows takes a level
//! only when the part says it is `interpolated` there.
//!
//! `Rx`, `Schmitt` and `Driver` are invented for these tests; none is a real part.

use netsmith_check::{Domain, Loader, Member, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::logic::Logic
import std::sources::Rail

/// Invented: an input whose thresholds are a fraction of its supply from 1.1 V to 1.95 V and
/// fixed from 3.0 V to 3.6 V, with no row between.
part Rx {
    .vcc: Rail
    .a: Logic
    .a.reference ~ .vcc
    .a.reference.current &= 0A
    .a.vih &= 0.65 * .vcc.voltage at 1.1V..1.95V, 2.0V at 3.0V..3.6V
    .a.vil &= 0.35 * .vcc.voltage at 1.1V..1.95V, 0.8V at 3.0V..3.6V
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
        .a.line  input "A"   "3"
    }
}

/// Invented: rows that meet at 2.7 V, so a supply across 2.7 V reaches both.
part Touching {
    .vcc: Rail
    .a: Logic
    .a.reference ~ .vcc
    .a.reference.current &= 0A
    .a.vih &= 0.7 * .vcc.voltage at 1.65V..2.7V, 2.0V at 2.7V..3.6V
    .a.vil &= 0.4V
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
        .a.line  input "A"   "3"
    }
}

/// Invented: a Schmitt input whose guaranteed high threshold is given at 1.65 V and at 2.3 V,
/// and which the datasheet says is linear in the supply between them.
part Schmitt {
    .vcc: Rail
    .a: Logic
    .a.reference ~ .vcc
    .a.reference.current &= 0A
    .a.vih &= 1.0V at 1.65V, 1.4V at 2.3V interpolated
    .a.vil &= 0.4V
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
        .a.line  input "A"   "3"
    }
}

/// Invented: drives within 0.1 V of its supply.
part Driver {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.reference.current &= 0A
    .out.voh &= .vdd.voltage - 0.1V
    .out.vol &= 0.1V
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}
"#;

/// A board with the driver on `driver` volts and the receiver `kind` on `receiver` volts.
fn solved_with(parts: &str, kind: &str, driver: &str, receiver: &str) -> Solution {
    let body = format!(
        "    .sd: Rail {{ .voltage &= {driver}; .pos.net &= \"VD\"; .neg.net &= \"GND\" }}\n    .sr: Rail {{ .voltage &= {receiver}; .pos.net &= \"VR\"; .neg.net &= \"GND\" }}\n    .d: Driver\n    .sd ~ .d.vdd\n    .r: {kind}\n    .sr ~ .r.vcc\n    .d.out.line ~ .r.a.line\n"
    );
    let c = check(
        "test.nsm",
        &format!("{parts}\nboard B {{\n{body}}}\n"),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn solved(kind: &str, driver: &str, receiver: &str) -> Solution {
    solved_with(PARTS, kind, driver, receiver)
}

fn faults(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated.iter().map(|n| n.message.clone()).collect()
}

/// The solved range of a level, rounded to the microvolt.
fn level(s: &Solution, path: &str) -> Option<(f64, f64)> {
    let round = |v: f64| (v * 1e6).round() / 1e6;
    match s.model.lookup(path) {
        Some(Member::Field(f)) => match s.model.field(f).domain {
            Domain::Quantity(q) => Some((round(q.lo()), round(q.hi()))),
            _ => None,
        },
        other => panic!("`{path}` is not a field: {other:?}"),
    }
}

#[test]
fn the_row_of_the_supply_gives_the_level_with_the_supply_in_it() {
    let s = solved("Rx", "1.8V +/- 5%", "1.8V +/- 5%");
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
    // 0.65 and 0.35 of 1.71 V..1.89 V.
    assert_eq!(level(&s, "r.a.vih"), Some((1.1115, 1.2285)));
    assert_eq!(level(&s, "r.a.vil"), Some((0.5985, 0.6615)));
}

#[test]
fn a_fixed_row_is_judged_with_its_numbers() {
    // On 3.3 V the input needs 2.0 V, and a 1.8 V driver's high is at most 1.79 V.
    let s = solved("Rx", "1.8V +/- 5%", "3.3V +/- 5%");
    assert_eq!(level(&s, "r.a.vih"), Some((2.0, 2.0)));
    assert_eq!(
        faults(&s),
        [
            "net `net-d.out.line`: `d.out.voh` is 1.61V..1.79V but `r.a.vih` needs 2V: a high of 1.61V is not always read as a high, which takes 2V"
        ]
    );
}

#[test]
fn a_supply_between_two_rows_has_no_level_and_says_why() {
    let s = solved("Rx", "2.5V", "2.5V");
    assert_eq!(level(&s, "r.a.vih"), None);
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(
        notes(&s),
        [
            "not evaluated: the high level from `d.out` to `r.a` on net `net-d.out.line`: `r.a.vih` has no value: `r.a.vih` is stated in rows by the supply, and no row gives it at 2.5V of the 2.5V of `r.a.reference.voltage` (between two rows the datasheet guarantees nothing; a part whose datasheet says the level is linear there states `interpolated` after the last row)",
            "not evaluated: the low level from `d.out` to `r.a` on net `net-d.out.line`: `r.a.vil` has no value: `r.a.vil` is stated in rows by the supply, and no row gives it at 2.5V of the 2.5V of `r.a.reference.voltage` (between two rows the datasheet guarantees nothing; a part whose datasheet says the level is linear there states `interpolated` after the last row)",
        ]
    );
}

#[test]
fn a_supply_beyond_the_last_row_has_no_level() {
    let s = solved("Rx", "3.3V", "3.3V..5V");
    assert_eq!(level(&s, "r.a.vih"), None);
    assert!(
        notes(&s).iter().any(|n| n.contains(
            "`r.a.vih` is stated in rows by the supply, and no row gives it at 3.6V..5V of the 3.3V..5V of `r.a.reference.voltage`"
        )),
        "{:?}",
        notes(&s)
    );
}

#[test]
fn a_supply_across_two_rows_is_judged_against_the_worse() {
    // 2.5 V..3.0 V reaches 0.7 x 2.5 V..2.7 V (at most 1.89 V) and the fixed 2.0 V: a driver
    // whose high is 1.9 V is enough for the first row and not for the second.
    let s = solved("Touching", "2V", "2.5V..3V");
    assert_eq!(level(&s, "r.a.vih"), Some((1.75, 2.0)));
    assert_eq!(
        faults(&s),
        [
            "net `net-d.out.line`: `d.out.voh` is 1.9V but `r.a.vih` needs 1.75V..2V: a high of 1.9V is not always read as a high, which takes 2V"
        ]
    );
}

#[test]
fn between_two_rows_an_interpolated_level_is_on_the_line_between_them() {
    // 1.0 V at 1.65 V and 1.4 V at 2.3 V: at 1.71 V..1.89 V, 1.0369 V..1.1477 V.
    let s = solved("Schmitt", "1.8V +/- 5%", "1.8V +/- 5%");
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
    assert_eq!(level(&s, "r.a.vih"), Some((1.036923, 1.147692)));
}

#[test]
fn without_interpolated_a_level_between_two_points_is_not_given() {
    let parts = PARTS.replacen(
        "1.0V at 1.65V, 1.4V at 2.3V interpolated",
        "1.0V at 1.65V, 1.4V at 2.3V",
        1,
    );
    assert_ne!(parts, PARTS);
    let s = solved_with(&parts, "Schmitt", "1.8V +/- 5%", "1.8V +/- 5%");
    assert_eq!(level(&s, "r.a.vih"), None);
    assert!(
        notes(&s).iter().any(|n| n.contains(
            "no row gives it at 1.71V..1.89V of the 1.71V..1.89V of `r.a.reference.voltage` (between two rows"
        )),
        "{:?}",
        notes(&s)
    );
}

#[test]
fn rows_that_disagree_with_another_statement_of_the_level_are_a_fault() {
    let parts = PARTS.replacen(
        "    .a.vih &= 0.65 * .vcc.voltage at 1.1V..1.95V, 2.0V at 3.0V..3.6V\n",
        "    .a.vih &= 0.65 * .vcc.voltage at 1.1V..1.95V, 2.0V at 3.0V..3.6V\n    .a.vih &= 1.5V..1.8V\n",
        1,
    );
    assert_ne!(parts, PARTS);
    let s = solved_with(&parts, "Rx", "3.3V", "3.3V");
    assert!(
        faults(&s).iter().any(|f| f.contains(
            "`r.a.vih` is 2V by its rows at the supply its reference rail has, outside the 1.5V..1.8V it is also stated to be"
        )),
        "{:?}",
        faults(&s)
    );
}
