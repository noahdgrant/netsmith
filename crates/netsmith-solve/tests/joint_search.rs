//! The search of parts that depend on each other says at once which part nothing can serve,
//! and parts that only read the same stated rail are searched apart (ADR 0057). Every value
//! here is invented.

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

fn catalogs(k: usize) -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: (1..=k).map(|i| resistor(100.0 * i as f64)).collect(),
    });
    c
}

fn run(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("joint.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn unresolved(s: &Solution) -> Vec<String> {
    s.unresolved
        .iter()
        .map(|u| s.model.path_of(u.part))
        .collect()
}

/// `n` resistors `u01`.. and `z`, all in one equation (`total`), so that one value joins
/// them whatever the bounds say, and `bound` on `z` alone.
fn tied(n: usize, bound: &str) -> String {
    let mut src = String::from("import std::parts::passive::Resistor\n\nboard Bench {\n");
    let names: Vec<String> = (1..=n).map(|i| format!("u{i:02}")).collect();
    for name in &names {
        src.push_str(&format!("    .{name}: Resistor\n"));
    }
    src.push_str("    .z: Resistor\n    .total: Resistance\n");
    let terms: Vec<String> = names
        .iter()
        .map(|n| format!(".{n}.resistance"))
        .chain(["    .z.resistance".trim().to_string()])
        .collect();
    src.push_str(&format!("    .total == {}\n", terms.join(" + ")));
    src.push_str(&format!("    {bound}\n}}\n"));
    src
}

#[test]
fn a_part_no_candidate_can_serve_is_named_at_once_however_many_parts_are_tied_to_it() {
    // Twelve parts tied by one equation, and a bound on `z` that no resistor of the catalog
    // meets: the combinations of the twelve are 6^12, and none of them can help.
    let s = run(&tied(12, ".z.resistance >= 1Mohm"), &catalogs(6));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].starts_with(
            "no `resistor` candidate satisfies `z`: the closest of the 6 that fit, 600 1%,"
        ),
        "{}",
        m[0]
    );
    assert!(
        m[0].contains("z.resistance must always be at least 1MΩ"),
        "{}",
        m[0]
    );
    assert!(!m[0].contains("gave up"), "{}", m[0]);
    assert!(s.trials < 100, "{} trials", s.trials);
    assert!(s.picks.is_empty());
    assert!(unresolved(&s).contains(&"z".to_string()));
}

#[test]
fn a_bound_the_other_parts_can_help_is_not_blamed_on_one_part() {
    // `z` and `u01` together: neither is the cause alone, so the search runs as it did and the
    // message names the two parts that the failure is made of, and not `u02`.
    let s = run(
        &tied(2, ".u01.resistance + .z.resistance >= 1Mohm"),
        &catalogs(3),
    );
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].starts_with("no combination of candidates satisfies `u01` and `z` together"),
        "{}",
        m[0]
    );
}

#[test]
fn a_bound_that_a_late_candidate_meets_still_finds_the_solution() {
    // Only the 300 ohm, the last of the catalog for `z`, is over 250 ohm: the part is not hopeless, and the first
    // solution in catalog order is found.
    let s = run(&tied(5, ".z.resistance >= 250ohm"), &catalogs(3));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks.len(), 6);
    let z = s
        .picks
        .iter()
        .find(|p| s.model.path_of(p.part) == "z")
        .unwrap();
    assert_eq!(z.label, "300");
    let first = s
        .picks
        .iter()
        .find(|p| s.model.path_of(p.part) == "u01")
        .unwrap();
    assert_eq!(first.label, "100");
}

/// Two taps on one stated rail. A tap is a resistor with a supply pin of its own, joined to
/// the rail: the rail's voltage is one value of both, and a check on either reads it.
fn across_a_rail(a: &str, b: &str) -> String {
    format!(
        r#"import std::sources::Rail

part Tap {{
    .designator &= "R"
    pins {{
        .vin.pos passive "A" "1"
        .vin.neg passive "B" "2"
    }}
    .vin: Rail
    .resistance: Resistance
    .max_power: Power
    .package: Package
    catalog resistor {{ .resistance, .max_power, .package }}
}}

board Bench {{
    .rail: Rail {{
        .voltage &= 5V +/- 1%
        .pos.net &= "V5"
        .neg.net &= "GND"
    }}
    .a: Tap
    .b: Tap
    .rail ~ .a.vin
    .rail ~ .b.vin
    .rail.voltage / .a.resistance <= {a}
    .rail.voltage / .b.resistance <= {b}
}}
"#
    )
}

#[test]
fn parts_that_only_read_the_same_stated_rail_are_searched_apart() {
    // Every resistor of the catalog draws more than 1 mA from 5 V, so `a` has nothing. `b`
    // reads the same rail and nothing else of `a`'s, so it is chosen all the same.
    let s = run(&across_a_rail("1mA", "100mA"), &catalogs(3));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("`a`"), "{}", m[0]);
    assert_eq!(unresolved(&s), ["a"]);
    let b = s
        .picks
        .iter()
        .find(|p| s.model.path_of(p.part) == "b")
        .expect("b is chosen");
    assert_eq!(b.label, "100");
}

#[test]
fn parts_that_share_a_value_a_pick_changes_stay_together() {
    // A check on both of them at once is one search: neither is chosen alone.
    let src = across_a_rail("1mA", "100mA").replace(
        "    .rail.voltage / .b.resistance <= 100mA\n",
        "    .rail.voltage / (.a.resistance + .b.resistance) <= 100mA\n",
    );
    let s = run(&src, &catalogs(3));
    assert_eq!(unresolved(&s), ["a", "b"]);
    assert!(s.picks.is_empty());
}

#[test]
fn both_are_chosen_when_both_can_be_and_the_picks_are_listed_in_path_order() {
    let s = run(&across_a_rail("100mA", "100mA"), &catalogs(3));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let picked: Vec<String> = s
        .picks
        .iter()
        .map(|p| format!("{} = {}", s.model.path_of(p.part), p.label))
        .collect();
    assert_eq!(picked, ["a = 100", "b = 100"]);
}
