//! A part with a `.count` is a bank of identical parts in parallel (ADR 0052): the solver
//! chooses the row and the whole number of parts together, the fewest that hold every check.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// A 5 V supply across a `CapacitorBank` `bulk` that states `{bulk}` and `{extra}`.
fn board(bulk: &str, extra: &str) -> String {
    board_of("CapacitorBank", bulk, extra)
}

/// The same across a part of the standard kind `kind`.
fn board_of(kind: &str, bulk: &str, extra: &str) -> String {
    format!(
        r#"import std::parts::passive::{{Capacitor, CapacitorBank}}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board B {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        .voltage &= 5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .bulk: {kind} {{ {bulk} }}
    .supply.pos ~> .bulk ~> .supply.neg
{extra}}}
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

fn number(s: &Solution, path: &str) -> f64 {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) if q.lo() == q.hi() => q.lo(),
        other => panic!("`{path}` is {other:?}"),
    }
}

const NEED: &str =
    "    .bulk.count * .bulk.effective_capacitance >= 60uF\n    .bulk.max_voltage >= 6V\n";

#[test]
fn a_requirement_no_single_capacitor_meets_is_met_by_a_bank() {
    let s = solved(&board(".count &= 1..8", NEED));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let n = number(&s, "bulk.count");
    assert!(n > 1.0, "one capacitor cannot give 60 uF at 5 V, not {n}");
    // The count is the fewest: the row picked with one less would not be enough.
    let eff = match s.model.domain_at("bulk.effective_capacitance") {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("{other:?}"),
    };
    assert!(n * eff.lo() >= 60e-6 - 1e-12, "{n} x {eff}");
    assert!((n - 1.0) * eff.lo() < 60e-6, "{n} x {eff}");
    // The rating is per capacitor.
    assert!(number(&s, "bulk.max_voltage") >= 6.0);
    let pick = &s.picks[0];
    assert_eq!(pick.count as f64, n);
}

#[test]
fn a_bank_of_one_picks_what_a_part_with_no_count_picks() {
    let alone = solved(&board_of(
        "Capacitor",
        "",
        "    .bulk.effective_capacitance >= 4uF\n",
    ));
    let bank = solved(&board(
        ".count &= 1..8",
        "    .bulk.count * .bulk.effective_capacitance >= 4uF\n",
    ));
    assert!(messages(&alone).is_empty() && messages(&bank).is_empty());
    assert_eq!(alone.picks[0].candidate, bank.picks[0].candidate);
    assert_eq!(alone.picks[0].count, 1);
    assert_eq!(bank.picks[0].count, 1);
    assert_eq!(number(&bank, "bulk.count"), 1.0);
}

#[test]
fn a_bank_with_no_count_stated_is_chosen_from_one_to_eight_fewest_first() {
    // The bank's own default is `1..8` (ADR 0123); nothing asks for more than one capacitor.
    let s = solved(&board("", "    .bulk.effective_capacitance >= 4uF\n"));
    assert_eq!(s.picks[0].count, 1);
    assert_eq!(number(&s, "bulk.count"), 1.0);
}

/// The same board as `board`, asking for the bank's total and not for the count times a value.
const NEED_TOTAL: &str =
    "    .bulk.total_effective_capacitance >= 60uF\n    .bulk.max_voltage >= 6V\n";

#[test]
fn a_board_that_narrows_the_total_gets_a_count_picked() {
    let by_total = solved(&board("", NEED_TOTAL));
    assert!(messages(&by_total).is_empty(), "{:?}", messages(&by_total));
    let n = number(&by_total, "bulk.count");
    assert!(n > 1.0, "one capacitor cannot give 60 uF at 5 V, not {n}");
    // The count is the fewest, and the total is what the circuit keeps: the same pick as the
    // relation written out with the count.
    let total = match by_total.model.domain_at("bulk.total_effective_capacitance") {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("{other:?}"),
    };
    assert!(total.lo() >= 60e-6 - 1e-12, "{total}");
    let by_count = solved(&board("", NEED));
    assert_eq!(by_total.picks[0].candidate, by_count.picks[0].candidate);
    assert_eq!(by_total.picks[0].count, by_count.picks[0].count);
}

#[test]
fn the_marked_total_is_the_count_times_the_marked_value() {
    // 66 uF of marked capacitance from 22 uF parts is three of them, whatever they keep at 5 V.
    let s = solved(&board(
        ".capacitance &= 22uF +/- 20%",
        "    .bulk.total_capacitance >= 66uF\n    .bulk.max_voltage >= 6V\n",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(number(&s, "bulk.count") >= 3.0);
}

#[test]
fn an_impossible_total_is_refused_at_its_line() {
    let src = board(
        ".count &= 1..2",
        "    .bulk.total_effective_capacitance >= 10mF\n",
    );
    let s = solved(&src);
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("no `capacitor` candidate satisfies `bulk`")
            && m[0].contains("bulk.total_effective_capacitance must always be at least 10mF")
            && m[0].contains("is not always above 10mF"),
        "{}",
        m[0]
    );
    // The line it points at is the board's statement of the total.
    let at = s.diagnostics[0].span;
    assert_eq!(
        &src[at.start..at.end],
        ".bulk.total_effective_capacitance >= 10mF"
    );
}

/// A count a module passes through its interface: a board cannot write a whole number as the
/// `.count` of its own capacitor (issue #685), but it states `.held` of a module that does.
fn held(count: u32) -> String {
    format!(
        r#"import std::parts::passive::CapacitorBank
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

module Held {{
    .rail: Rail
    .held: Number
    .bulk: CapacitorBank
    .bulk.count == .held
    .bulk.count * .bulk.effective_capacitance >= 60uF
    .bulk.max_voltage >= 6V
    .rail.pos ~> .bulk ~> .rail.neg
}}

board B {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        .voltage &= 5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .h: Held {{ .held &= {count} }}
    .h.rail ~ .supply
}}
"#
    )
}

#[test]
fn a_stated_count_is_the_count_and_the_checks_hold_it() {
    let s = solved(&held(8));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(number(&s, "h.bulk.count"), 8.0);
    assert_eq!(s.picks[0].count, 8);
    // Two are not enough with any row at 5 V: the message says what was tried.
    let s = solved(&held(2));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("no `capacitor` candidate satisfies `h.bulk`")
            && m[0].contains("2 x ")
            && m[0].contains(
                "(h.bulk.count * h.bulk.effective_capacitance) must always be at least 60uF"
            ),
        "{}",
        m[0]
    );
}

#[test]
fn a_range_with_no_whole_number_in_it_is_refused_naming_the_count() {
    let s = solved(&board(".count &= 2.2..2.8", NEED));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("the `capacitor` catalog has nothing for `bulk`")
            && m[0].contains("count within 2.2..2.8"),
        "{}",
        m[0]
    );
}

#[test]
fn a_bank_limited_to_fewer_than_it_needs_says_so_with_the_numbers() {
    let s = solved(&board(".count &= 1..2", NEED));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("no `capacitor` candidate satisfies `bulk`")
            && m[0].contains("is not always above 60uF"),
        "{}",
        m[0]
    );
}

#[test]
fn a_rating_that_no_count_changes_is_not_tried_at_every_count() {
    // 500 V is out of every row's reach, whatever the count: each row fails once, for the
    // rating, and a count of eight does not make the search eight times as long.
    let extra = "    .bulk.max_voltage >= 500V\n";
    let alone = solved(&board("", extra));
    let bank = solved(&board(".count &= 1..8", extra));
    assert_eq!(messages(&alone).len(), 1);
    assert_eq!(messages(&bank).len(), 1);
    assert_eq!(bank.trials, alone.trials, "{} trials", bank.trials);
}

#[test]
fn a_relation_that_reads_only_counts_does_not_hide_a_count_from_the_search() {
    // Two banks and a board value that is their counts' sum, which must be at least 5.
    // `b` has at most 3, so `a` at one fails whatever `b` does: every row of `b` fails at
    // every count, for a relation that reads `.total` and the counts and no column of `a`.
    // It does depend on `a`'s choice of count, so `a` at two must still be tried (a false
    // refusal otherwise).
    let src = r#"import std::parts::passive::CapacitorBank
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board B {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .a: CapacitorBank { .count &= 1..8 }
    .b: CapacitorBank { .count &= 1..3 }
    .supply.pos ~> .a ~> .supply.neg
    .supply.pos ~> .b ~> .supply.neg
    .total: Number
    .total == .a.count + .b.count
    .total >= 5
    .a.count * .a.effective_capacitance >= 2uF
    .b.count * .b.effective_capacitance >= 2uF
    .a.max_voltage >= 6V
    .b.max_voltage >= 6V
}
"#;
    let s = solved(src);
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let (a, b) = (number(&s, "a.count"), number(&s, "b.count"));
    assert!(a + b >= 5.0, "a = {a}, b = {b}");
    // The fewest for `a` comes first: two, with `b` at its most.
    assert_eq!((a, b), (2.0, 3.0));
}
