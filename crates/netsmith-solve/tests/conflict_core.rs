//! A search that cannot succeed says which parts it cannot succeed for, and stops when it has
//! learned it (ADR 0065, 9.7). `board` chains `a`, `u01`..`uNN` and `z` through one sum, which
//! puts every part in the same group and the same class of values: the sum reads them all. The
//! conflict is between a few of them, through a bound that reads only those. A bound that no
//! value of the candidates' ranges can meet is refused before a trial, by the hulls of the parts
//! it reads (`ahead.rs`, issue #851); a bound that some value meets but no choice meets over its
//! whole range (a window narrower than the spread of the sum) is left to the search. Every part
//! and number here is invented.

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
        sku: None,
        manufacturer: None,
        mpn: None,
        library: None,
        channel: None,
        assembly: None,
        height: None,
        temp_min: None,
        temp_max: None,
        max_voltage_steps: None,
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

/// `a`, `m`, `u01`..`u{n}` and `z`, all one sum, and the bounds `bounds` between some of them.
fn board(n: usize, bounds: &str) -> String {
    let names: Vec<String> = ["a".to_string(), "m".to_string()]
        .into_iter()
        .chain((1..=n).map(|i| format!("u{i:02}")))
        .chain(["z".to_string()])
        .collect();
    let mut src = String::from("import std::parts::passive::Resistor\n\nboard Big {\n");
    for name in &names {
        src.push_str(&format!("    .{name}: Resistor\n"));
    }
    src.push_str("    .total: Resistance\n");
    let sum: Vec<String> = names.iter().map(|n| format!(".{n}.resistance")).collect();
    src.push_str(&format!("    .total == {}\n", sum.join(" + ")));
    src.push_str(bounds);
    src.push_str("}\n");
    src
}

fn run(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("core.nsm", src, &Loader::std());
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
                p.label,
                p.rejected
            )
        })
        .collect()
}

#[test]
fn a_pair_that_cannot_be_met_is_named_among_many_parts_that_share_a_sum() {
    // 14 parts of 8 candidates: 8^14 combinations, far past the limit, and the sum joins them
    // all. No two resistors of the catalog add up to 1 Mohm.
    let s = run(
        &board(12, "    .a.resistance + .z.resistance >= 1Mohm\n"),
        &catalogs(8),
    );
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:#?}");
    assert!(!text[0].contains("gave up"), "{}", text[0]);
    assert_eq!(
        text[0],
        "no combination of candidates satisfies `a` and `z` together: whichever candidates they take, (a.resistance + z.resistance) must be at least 1MΩ, but 198Ω..1.616kΩ is always below 1MΩ"
    );
    assert_eq!(s.trials, 0);
    assert!(s.picks.is_empty());
}

#[test]
fn a_pair_whose_window_no_choice_fits_is_named_among_many_parts_that_share_a_sum() {
    // Some value of the pair's range is in the window, so nothing is refused before the pair is
    // chosen, but the sum of two 1 % resistors spreads over more than the window: every choice
    // of `z` is refused for the choice of `a`, and the search goes back to `a` over the twelve
    // parts between them.
    let s = run(
        &board(
            12,
            "    .a.resistance + .z.resistance >= 1001ohm\n    .a.resistance + .z.resistance <= 1002ohm\n",
        ),
        &catalogs(8),
    );
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:#?}");
    assert!(
        text[0].starts_with(
            "no combination of candidates satisfies `a` and `z` together, whatever the other parts of the group are: the closest, "
        ),
        "{}",
        text[0]
    );
    assert!(s.trials < 2_000, "{} trials", s.trials);
    assert!(s.picks.is_empty());
}

#[test]
fn three_parts_that_cannot_be_met_together_are_named() {
    let s = run(
        &board(
            10,
            "    .a.resistance + .m.resistance + .z.resistance >= 1Mohm\n",
        ),
        &catalogs(8),
    );
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:#?}");
    assert_eq!(
        text[0],
        "no combination of candidates satisfies `a` and `m` and `z` together: whichever candidates they take, ((a.resistance + m.resistance) + z.resistance) must be at least 1MΩ, but 297Ω..2.424kΩ is always below 1MΩ"
    );
    assert_eq!(s.trials, 0);
}

#[test]
fn a_pair_that_can_be_met_by_a_later_candidate_keeps_the_first_solution_in_catalog_order() {
    // Only 800 and 800 add up to 1.5 kohm with the least at that: the parts that are no part of
    // the pair keep their first candidate, and the pair's are the first that works.
    let s = run(
        &board(12, "    .a.resistance + .z.resistance >= 1.5kohm\n"),
        &catalogs(8),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    let picks = picked(&s);
    assert_eq!(picks.len(), 15);
    assert_eq!(picks[0], "a = 800 (rejected 7)");
    assert_eq!(picks[1], "m = 100 (rejected 0)");
    assert_eq!(picks[14], "z = 800 (rejected 7)");
    assert!(
        picks[2..14]
            .iter()
            .all(|p| p.ends_with("= 100 (rejected 0)") || p.contains("= 100 ("))
    );
    assert!(s.trials < 2_000, "{} trials", s.trials);
}

#[test]
fn a_failure_that_every_part_takes_part_in_names_every_part() {
    // The bound reads the sum, which every part feeds, so the failure is the whole group's.
    let s = run(&board(2, "    .total >= 1Mohm\n"), &catalogs(3));
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:#?}");
    assert_eq!(
        text[0],
        "no combination of candidates satisfies `a` and `m` and `u01` and `u02` and `z` together: whichever candidates they take, total must be at least 1MΩ, but 495Ω..1.515kΩ is always below 1MΩ"
    );
}

#[test]
fn a_window_that_every_part_feeds_names_every_part_and_the_closest() {
    // The window is inside what the sum can be, so the search runs, and every part feeds it.
    let s = run(
        &board(2, "    .total >= 501ohm\n    .total <= 502ohm\n"),
        &catalogs(3),
    );
    let text = messages(&s);
    assert_eq!(text.len(), 1, "{text:#?}");
    assert!(
        text[0].starts_with(
            "no combination of candidates satisfies `a` and `m` and `u01` and `u02` and `z` together: the closest, "
        ),
        "{}",
        text[0]
    );
    assert!(!text[0].contains("whatever the other parts"), "{}", text[0]);
}

#[test]
fn a_chain_that_no_choice_can_reach_is_refused_before_a_trial() {
    // Nine parts of eight candidates whose total is at most 7.3 kohm: 8^9 combinations, which the
    // search tried until its limit (250000) when a bound was judged only once every part it
    // reads was chosen. The hulls of the candidates show that none reaches 100 Mohm.
    let s = run(&board(6, "    .total >= 100Mohm\n"), &catalogs(8));
    assert_eq!(
        messages(&s),
        [
            "no combination of candidates satisfies `a` and `m` and `u01` and `u02` and `u03` and `u04` and `u05` and `u06` and `z` together: whichever candidates they take, total must be at least 100MΩ, but 891Ω..7.272kΩ is always below 100MΩ"
        ]
    );
    assert_eq!(s.trials, 0);
}

#[test]
fn a_failure_that_every_part_feeds_still_gives_up_at_the_limit_and_names_a_part_to_state() {
    // A window of 1 ohm in the middle of what the nine parts can add up to: every choice of the
    // first eight leaves a sum that some choice of the last reaches, so nothing is refused before
    // the last part, and every choice of it fails, so nothing is learned that cuts the 8^9
    // combinations. The limit is the safety net (issue #177), and its message names the part
    // with the most candidates to state a value for (issue #851). The limit is lowered so that the
    // test ends in a moment; `search_budget.rs` has the default.
    let catalogs = catalogs(8).with_search_budget(netsmith_solve::SearchBudget {
        trials: 20_000,
        work: u64::MAX,
    });
    let s = run(
        &board(6, "    .total >= 4000ohm\n    .total <= 4001ohm\n"),
        &catalogs,
    );
    assert_eq!(
        messages(&s),
        [
            "gave up after trying 20000 combinations of candidates for `a` and `m` and `u01` and `u02` and `u03` and `u04` and `u05` and `u06` and `z`; a bound reads values of all of them, so no part could be blamed alone. State a value for `a`: it has the most candidates that fit (8), so a value for it leaves the fewest combinations to try. Or narrow a window that a bound on them reads, or split the group so that fewer parts share a bound"
        ]
    );
    assert_eq!(s.trials, 20_000);
}
