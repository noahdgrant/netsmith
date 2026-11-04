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

// ---- what the search says and chooses does not change -------------------------------
//
// The expected text below is what the search printed before it backjumped.

#[test]
fn a_conflict_with_the_first_part_is_reported_as_it_was() {
    let s = run(&bench(3, true), &catalogs(3));
    assert_eq!(
        messages(&s),
        [
            "no combination of candidates satisfies `a` and `u01` and `u02` and `u03` and `z` together: the closest, a = 300 1% and u01 = 100 1% and u02 = 100 1% and u03 = 100 1% and z = 300 1%, fails because (a.resistance + z.resistance) must always be at least 1MΩ, but 594Ω..606Ω is not always above 1MΩ"
        ]
    );
    assert_eq!(unresolved(&s), ["a", "u01", "u02", "u03", "z"]);
    assert!(s.picks.is_empty());
}

#[test]
fn a_conflict_with_a_middle_part_is_reported_as_it_was() {
    let s = run(
        &bench_mid(4, ".u02.resistance + .z.resistance >= 1Mohm"),
        &catalogs(3),
    );
    assert_eq!(
        messages(&s),
        [
            "no combination of candidates satisfies `a` and `u01` and `u02` and `u03` and `u04` and `z` together: the closest, a = 100 1% and u01 = 100 1% and u02 = 300 1% and u03 = 100 1% and u04 = 100 1% and z = 300 1%, fails because (u02.resistance + z.resistance) must always be at least 1MΩ, but 594Ω..606Ω is not always above 1MΩ"
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
fn a_range_that_just_misses_is_reported_as_it_was() {
    let s = run(
        &bench_mid(4, ".z.resistance == .a.resistance * 3"),
        &catalogs(3),
    );
    assert_eq!(
        messages(&s),
        [
            "no combination of candidates satisfies `a` and `u01` and `u02` and `u03` and `u04` and `z` together: the closest, a = 100 1% and u01 = 100 1% and u02 = 100 1% and u03 = 100 1% and u04 = 100 1% and z = 300 1%, fails because `z.resistance` must stay within 297Ω..303Ω, but (a.resistance * 3) works out to 297Ω..303Ω"
        ]
    );
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
