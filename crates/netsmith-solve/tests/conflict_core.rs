//! A search that cannot succeed says which parts it cannot succeed for, and stops when it has
//! learned it (ADR 0065, 9.7). `board` chains `a`, `u01`..`uNN` and `z` through one sum, which
//! puts every part in the same group and the same class of values: the sum reads them all. The
//! conflict is between a few of them, through a bound that reads only those. Every part and
//! number here is invented.

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
    assert!(
        text[0].starts_with(
            "no combination of candidates satisfies `a` and `z` together, whatever the other parts of the group are: the closest, a = 800 1% and z = 800 1%, fails because (a.resistance + z.resistance) must always be at least 1MΩ"
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
    assert!(
        text[0].starts_with(
            "no combination of candidates satisfies `a` and `m` and `z` together, whatever the other parts of the group are: the closest, a = 800 1% and m = 800 1% and z = 800 1%, fails because "
        ),
        "{}",
        text[0]
    );
    assert!(s.trials < 20_000, "{} trials", s.trials);
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
fn a_failure_that_every_part_feeds_still_gives_up_at_the_limit_with_the_same_message() {
    // The bound reads the sum and every candidate of the last part fails it for the choices
    // before it, so nothing is learned that cuts the 8^9 combinations: the limit is the safety
    // net, and its message names the parts and says what to change (issue #177).
    let s = run(&board(6, "    .total >= 100Mohm\n"), &catalogs(8));
    assert_eq!(
        messages(&s),
        [
            "gave up after trying 250000 combinations of candidates for `a` and `m` and `u01` and `u02` and `u03` and `u04` and `u05` and `u06` and `z`; a bound reads values of all of them, so no part could be blamed alone. State a value for one of them, narrow a window that a bound on them reads, or split the group so that fewer parts share a bound"
        ]
    );
    assert_eq!(s.trials, 250_000);
}
