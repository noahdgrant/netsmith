//! The search does not choose again the parts that are not the cause of a conflict.
//!
//! `bench` builds a board of `a`, `u01`..`uNN` and `z`, all resistors. The `u` parts are
//! chained to `a` by relations that every candidate satisfies, so they share a group with
//! `a` and `z` but have nothing to do with the one relation nothing can satisfy:
//! `a + z >= 1Mohm`, when no resistor in the catalog is above 1 kohm. Searched in order
//! (`a`, `u01`..`uNN`, `z`), every combination of the `u` parts reaches `z` and fails there
//! for the same reason.

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
        qualification: None,
        generic: None,
    }
}

/// Resistors of 100, 200, ... ohms, `k` of them.
fn catalogs(k: usize) -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: (1..=k).map(|i| resistor(100.0 * i as f64)).collect(),
    });
    c
}

/// `n` chained parts between `a` and `z`; `conflict` says whether `z` can be satisfied.
fn bench(n: usize, conflict: bool) -> String {
    let mut src = String::from("import std::parts::passive::Resistor\n\nboard Bench {\n");
    src.push_str("    .a: Resistor\n");
    for i in 1..=n {
        src.push_str(&format!("    .u{i:02}: Resistor\n"));
    }
    src.push_str("    .z: Resistor\n");
    if n > 0 {
        src.push_str("    .a.resistance + .u01.resistance <= 1Mohm\n");
    }
    for i in 1..n {
        src.push_str(&format!(
            "    .u{i:02}.resistance + .u{:02}.resistance <= 1Mohm\n",
            i + 1
        ));
    }
    let bound = if conflict { "1Mohm" } else { "100ohm" };
    src.push_str(&format!(
        "    .a.resistance + .z.resistance >= {bound}\n}}\n"
    ));
    src
}

/// `bench` with the conflict between `u02` and `z` instead of `a` and `z`.
fn bench_mid(n: usize, relation: &str) -> String {
    let mut src = bench(n, false);
    let at = src.rfind("    .a.resistance + .z").unwrap();
    src.truncate(at);
    src.push_str(&format!("    {relation}\n}}\n"));
    src
}

fn run(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("bench.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn picked(s: &Solution) -> Vec<String> {
    s.picks
        .iter()
        .map(|p| {
            format!(
                "{} = {} (rejected {})",
                s.model.path_of(p.part),
                p.candidate,
                p.rejected
            )
        })
        .collect()
}

fn unresolved(s: &Solution) -> Vec<String> {
    s.unresolved
        .iter()
        .map(|u| s.model.path_of(u.part))
        .collect()
}

// ---- what the search says and chooses -------------------------------
//
// The parts that the failure is made of are named, and only they (`conflict_core.rs`); which
// parts are chosen does not change.

#[test]
fn a_conflict_with_the_first_part_names_the_two_parts() {
    let s = run(&bench(3, true), &catalogs(3));
    assert_eq!(
        messages(&s),
        [
            "no combination of candidates satisfies `a` and `z` together, whatever the other parts of the group are: the closest, a = 300 1% and z = 300 1%, fails because (a.resistance + z.resistance) must always be at least 1MΩ, but 594Ω..606Ω is not always above 1MΩ"
        ]
    );
    assert_eq!(unresolved(&s), ["a", "u01", "u02", "u03", "z"]);
    assert!(s.picks.is_empty());
}

#[test]
fn a_conflict_with_a_middle_part_names_the_two_parts() {
    let s = run(
        &bench_mid(4, ".u02.resistance + .z.resistance >= 1Mohm"),
        &catalogs(3),
    );
    assert_eq!(
        messages(&s),
        [
            "no combination of candidates satisfies `u02` and `z` together, whatever the other parts of the group are: the closest, u02 = 300 1% and z = 300 1%, fails because (u02.resistance + z.resistance) must always be at least 1MΩ, but 594Ω..606Ω is not always above 1MΩ"
        ]
    );
    assert!(s.picks.is_empty());
}

#[test]
fn a_part_with_nothing_that_fits_is_reported_as_it_was() {
    // `z` must be ten times `a`, and no candidate is.
    let s = run(
        &bench_mid(4, ".z.resistance == .a.resistance * 10"),
        &catalogs(3),
    );
    assert_eq!(
        messages(&s),
        ["the `resistor` catalog has nothing for `z`, which needs nothing in particular"]
    );
    assert_eq!(unresolved(&s), ["a", "u01", "u02", "u03", "u04", "z"]);
}

#[test]
fn a_range_that_misses_by_a_hair_is_refused_with_its_numbers() {
    // `a * 3.001` ends at 303.101, a hair past the 303 that `z` can be.
    let s = run(
        &bench_mid(4, ".z.resistance == .a.resistance * 3.001"),
        &catalogs(3),
    );
    assert_eq!(
        messages(&s),
        ["the `resistor` catalog has nothing for `z`, which needs nothing in particular"]
    );
}

/// The range of `a * 3` is 297 to 303 exactly, which is the range of `z`: nothing in the
/// arithmetic rounds, so the range is not pushed past the limit (ADR 0056). It used to be
/// refused, with the same two ranges in the message.
#[test]
fn a_range_that_touches_exactly_is_accepted() {
    let s = run(
        &bench_mid(4, ".z.resistance == .a.resistance * 3"),
        &catalogs(3),
    );
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(unresolved(&s).is_empty());
    assert!(!s.picks.is_empty());
}

#[test]
fn a_design_that_passes_keeps_its_first_solution_in_catalog_order() {
    let s = run(&bench(3, false), &catalogs(3));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(
        picked(&s),
        [
            "a = 100 1% (rejected 0)",
            "u01 = 100 1% (rejected 0)",
            "u02 = 100 1% (rejected 0)",
            "u03 = 100 1% (rejected 0)",
            "z = 100 1% (rejected 0)",
        ]
    );
}

#[test]
fn a_solution_that_needs_a_middle_part_changed_is_still_found() {
    // `u02` and `z` must add up to at least 500 ohm, so `u02` is not the first candidate.
    // The parts between `a` and `u02`, and after it, keep their first.
    let s = run(
        &bench_mid(4, ".u02.resistance + .z.resistance >= 500ohm"),
        &catalogs(3),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(
        picked(&s),
        [
            "a = 100 1% (rejected 0)",
            "u01 = 100 1% (rejected 0)",
            "u02 = 300 1% (rejected 2)",
            "u03 = 100 1% (rejected 0)",
            "u04 = 100 1% (rejected 0)",
            "z = 300 1% (rejected 2)",
        ]
    );
}

#[test]
fn the_same_design_gives_the_same_answer_every_time() {
    let src = bench_mid(4, ".u02.resistance + .z.resistance >= 1Mohm");
    let first = run(&src, &catalogs(3));
    for _ in 0..3 {
        let again = run(&src, &catalogs(3));
        assert_eq!(messages(&first), messages(&again));
        assert_eq!(first.trials, again.trials);
    }
}

// ---- the effort ---------------------------------------------------------------------
//
// Before backjumping, the trials of `bench(n, true)` with three candidates were 39, 120,
// 363, 1092, 3279, 9840 and 29523 for n = 1..7: three times as many for each part added.

#[test]
fn parts_that_are_not_the_cause_are_not_chosen_again() {
    let trials: Vec<usize> = (1..=7)
        .map(|n| run(&bench(n, true), &catalogs(3)).trials)
        .collect();
    // Each added part costs the same few trials, however many there are.
    let steps: Vec<usize> = trials.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(steps.windows(2).all(|w| w[0] == w[1]), "{trials:?}");
    assert!(trials[6] < 100, "{trials:?}");
}

#[test]
fn a_conflict_deep_in_a_long_chain_stays_cheap() {
    // Twelve unrelated parts were 3^12 = 531,441 combinations, past the limit, and the
    // search used to give up with a message that blames none of the parts.
    let s = run(&bench(12, true), &catalogs(3));
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(!msgs[0].contains("gave up"), "{}", msgs[0]);
    assert!(
        msgs[0].contains("(a.resistance + z.resistance) must always be at least 1MΩ"),
        "{}",
        msgs[0]
    );
    assert!(s.trials < 200, "{} trials", s.trials);
}

#[test]
fn a_conflict_with_a_middle_part_costs_no_more_than_the_parts_before_it() {
    // Before: 1092 trials for four parts in between.
    let s = run(
        &bench_mid(4, ".u02.resistance + .z.resistance >= 1Mohm"),
        &catalogs(3),
    );
    assert!(s.trials < 60, "{} trials", s.trials);
}

#[test]
fn a_part_that_is_the_cause_through_an_equation_is_still_chosen_again() {
    // `z` is equal to `u01`, so when `z` and `a` add up to too little, the cause is `u01` as
    // much as `a`, and the search must go back to `u01` and not past it to `a`. Between
    // them are `u02` and `u03`, which are not the cause.
    let s = run(
        &bench_mid(
            3,
            ".z.resistance == .u01.resistance\n    .z.resistance + .a.resistance >= 250ohm",
        ),
        &catalogs(3),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(
        picked(&s),
        [
            "a = 100 1% (rejected 0)",
            "u01 = 200 1% (rejected 1)",
            "u02 = 100 1% (rejected 0)",
            "u03 = 100 1% (rejected 0)",
            "z = 200 1% (rejected 0)",
        ]
    );
}
