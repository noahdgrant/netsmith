//! A circle of values that no range starts is started from the supply's range when the part's
//! own range holds no other value that the equations allow (ADR 0053), and left alone, with
//! its checks not evaluated, when it cannot be shown. Every value here is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, Solution, solve};

/// A constant-power load (0.5 W) behind a series resistor `{r}`, on a supply of `{supply}`.
/// The load's voltage is the supply's less the drop, the drop needs the current, and the
/// current needs the voltage. `{range}` is what the load says about the voltage it works at,
/// and `{extra}` is more of the board.
fn board(supply: &str, r: &str, range: &str, extra: &str) -> String {
    format!(
        r#"import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

/// A series element with a resistance and no rating, so that no catalog decides the board.
part Link {{
    .designator &= "R"
    pins {{
        .a passive "A" "1"
        .b passive "B" "2"
    }}
    .resistance: Resistance
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    bridge .a ~> .b
}}

board B {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        {supply}
        .pos.net &= "VBATT"
        .neg.net &= "GND"
    }}
    .r1: Link {{ .resistance &= {r} }}
    .load: Rail {{
        .pos.net &= "VLOAD"
        .neg.net &= "GND"
    }}
    .supply.neg ~ .load.neg
    .supply.pos ~> .r1 ~> .load.pos
    .load.voltage == .supply.voltage - .r1.drop
    .load.current == 0.5W / .load.voltage
{range}{extra}}}
"#
    )
}

const SUPPLY: &str = ".voltage &= 9V..16V";
const RANGE: &str = "    .load.voltage >= 4V\n    .load.voltage <= 30V\n";

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn range(s: &Solution, path: &str) -> Quantity {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

/// The voltage the load works at when the supply is `s` volts, the resistance `r` ohms and
/// the load `p` watts, on the branch that carries the least current: the larger root of
/// `v * v - s * v + r * p = 0`.
fn upper_root(s: f64, r: f64, p: f64) -> f64 {
    (s + (s * s - 4.0 * r * p).sqrt()) / 2.0
}

#[test]
fn a_circle_is_started_from_the_supplys_range_and_worked_out_to_the_end() {
    let s = solved(&board(SUPPLY, "10ohm +/- 1%", RANGE, ""));
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert!(s.circular.is_empty(), "{:?}", s.circular);
    assert!(s.deferred.is_empty(), "{:?}", s.deferred);
    assert_eq!(s.seeded.len(), 1);
    let seed = &s.seeded[0];
    assert_eq!(s.model.path_of_field(seed.field), "load.voltage");
    let from: Vec<String> = seed
        .from
        .iter()
        .map(|&f| s.model.path_of_field(f))
        .collect();
    assert_eq!(from, ["supply.voltage"]);
    assert_eq!((seed.within.lo(), seed.within.hi()), (4.0, 30.0));

    // The worst case is 9 V through 10.1 ohm: 8.399 V and 59.5 mA, and the start was not
    // allowed to be tighter than that.
    let worst = upper_root(9.0, 10.1, 0.5);
    let v = range(&s, "load.voltage");
    assert!(
        v.lo() <= worst && worst - v.lo() < 1e-4,
        "{v}, the worst case is {worst}"
    );
    // And at 16 V through 9.9 ohm it is 15.69 V.
    let best = upper_root(16.0, 9.9, 0.5);
    assert!(
        v.hi() >= best && v.hi() - best < 1e-4,
        "{v}, the best case is {best}"
    );
    let i = range(&s, "load.current");
    assert!(i.lo() <= 0.5 / best && i.lo() > 0.5 / best * 0.999, "{i}");
    assert!(i.hi() >= 0.5 / worst && i.hi() < 0.5 / worst * 1.001, "{i}");
}

#[test]
fn what_the_circle_holds_contains_every_working_point_of_the_ranges() {
    let s = solved(&board(SUPPLY, "10ohm +/- 1%", RANGE, ""));
    let v = range(&s, "load.voltage");
    let i = range(&s, "load.current");
    let drop = range(&s, "r1.drop");
    for supply in (0..=14).map(|k| 9.0 + 0.5 * f64::from(k)) {
        for r in [9.9, 10.0, 10.1] {
            let at = upper_root(supply, r, 0.5);
            assert!(v.lo() <= at && at <= v.hi(), "{at} is not in {v}");
            let amps = 0.5 / at;
            assert!(i.lo() <= amps && amps <= i.hi(), "{amps} A is not in {i}");
            assert!(drop.lo() <= amps * r && amps * r <= drop.hi(), "{drop}");
        }
    }
}

#[test]
fn a_check_that_needed_the_circle_is_made_with_what_was_found() {
    // 8 V is below the working point at 9 V (8.399 V), so the load always has more.
    let pass = solved(&board(
        SUPPLY,
        "10ohm +/- 1%",
        RANGE,
        "    .load.voltage >= 8V\n",
    ));
    assert!(pass.diagnostics.is_empty(), "{:?}", pass.diagnostics);
    assert!(pass.deferred.is_empty());
    // 8.5 V is not: at 9 V in and 10.1 ohm the load gets 8.399 V, and the message says so.
    let fail = solved(&board(
        SUPPLY,
        "10ohm +/- 1%",
        RANGE,
        "    .load.voltage >= 8.5V\n",
    ));
    let m: Vec<String> = fail.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("load.voltage must always be at least 8.5V")
            && m[0].contains("is not always above 8.5V"),
        "{}",
        m[0]
    );
}

#[test]
fn a_supply_that_states_no_range_leaves_the_circle_as_it_was() {
    // Nothing has a value to start from: the relations are not evaluated (and the supply's
    // voltage, which nothing states, is no circle to name).
    let s = solved(&board("", "10ohm +/- 1%", RANGE, ""));
    assert!(s.seeded.is_empty());
    assert!(!s.deferred.is_empty());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
}

#[test]
fn a_load_that_states_no_range_of_its_own_leaves_the_circle_as_it_was() {
    // The start would be a guess about where the part stops working: the board states a range.
    let s = solved(&board(SUPPLY, "10ohm +/- 1%", "", ""));
    assert!(s.seeded.is_empty());
    assert!(!s.circular.is_empty());
    assert!(!s.deferred.is_empty());
}

#[test]
fn a_second_working_point_inside_the_parts_range_is_no_reason_to_start_from_the_first() {
    // 36.4 ohm behind 9 V: the load can sit at 5.9 V and at 3.1 V, both with the power it
    // asks for. With 3 V as the least the load works at, the second is in its range and the
    // circle is not started: no check passes on the strength of the first.
    let r = "36ohm +/- 1%";
    let s = solved(&board(
        SUPPLY,
        r,
        "    .load.voltage >= 3V\n    .load.voltage <= 30V\n",
        "    .load.voltage >= 5V\n",
    ));
    assert!(s.seeded.is_empty(), "{:?}", s.seeded);
    assert!(!s.circular.is_empty());
    assert!(!s.deferred.is_empty(), "the check is not evaluated");
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
}

#[test]
fn a_second_working_point_below_the_parts_range_is_not_one_the_part_can_be_in() {
    // The same circuit with 4 V as the least the load works at: 3.1 V is not a state of
    // the load, and the circle is started from the first.
    let r = "36ohm +/- 1%";
    let s = solved(&board(
        SUPPLY,
        r,
        "    .load.voltage >= 4V\n    .load.voltage <= 30V\n",
        "    .load.voltage >= 5V\n",
    ));
    assert_eq!(s.seeded.len(), 1, "{:?}", s.circular);
    assert!(s.circular.is_empty());
    let left: Vec<String> = s
        .deferred
        .iter()
        .map(|&i| s.model.show_expr(&s.model.relations()[i].lhs))
        .collect();
    assert!(s.deferred.is_empty(), "{left:?}");
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let worst = upper_root(9.0, 36.36, 0.5);
    let v = range(&s, "load.voltage");
    assert!(
        v.lo() <= worst && worst - v.lo() < 1e-4,
        "{v} against {worst}"
    );
}

#[test]
fn a_range_the_board_states_on_the_circle_is_used_as_before() {
    let s = solved(&board(
        SUPPLY,
        "10ohm +/- 1%",
        RANGE,
        "    .load.voltage &= 4V..16V\n",
    ));
    assert!(s.seeded.is_empty());
    assert!(s.circular.is_empty());
    assert!(s.deferred.is_empty());
}

#[test]
fn a_supply_the_load_can_leave_no_working_point_for_is_not_started() {
    // 100 ohm cannot give 0.5 W at 9 V: there is no working point, and nothing is claimed.
    let s = solved(&board(SUPPLY, "100ohm +/- 1%", RANGE, ""));
    assert!(s.seeded.is_empty());
    assert!(!s.deferred.is_empty());
}

#[test]
fn a_loose_range_the_board_states_stays_loose_and_sound() {
    // The board states the whole range the load works in as the start (the way a seed had
    // to be written before). Where the first pass reaches below it the equation cannot
    // narrow it, and the answer stays as wide as it was stated: sound, and not tight.
    let s = solved(&board(
        SUPPLY,
        "36ohm +/- 1%",
        "    .load.voltage >= 3V\n    .load.voltage <= 30V\n",
        "    .load.voltage &= 3V..30V\n",
    ));
    assert!(s.seeded.is_empty());
    assert!(s.circular.is_empty());
    let v = range(&s, "load.voltage");
    assert_eq!((v.lo(), v.hi()), (3.0, 30.0), "{v}");
    // The current is the one worked out at the bottom of that range, 0.5 W at 3 V.
    let i = range(&s, "load.current");
    assert!(i.hi() >= 0.5 / 3.0 - 1e-9, "{i}");
    // The equation is held against the range as stated, and the check says it cannot be shown.
    let m: Vec<String> = s.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("`load.voltage` must stay within 3V..30V") && m[0].contains("works out to"),
        "{}",
        m[0]
    );
}
