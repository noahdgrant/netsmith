//! A relation that reads the end of a range (`min(.f)`, `max(.f)`, ADR 0098) is never used to
//! refuse a block of catalog rows without trying them, and never gives a bound to another
//! field: the hull of a block puts a point from the block's edge in the column, and that point
//! is not an enclosure of what each row says. It is still verified on the final values.
//! Every part and number here is invented.

use netsmith_check::{Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Candidate, Catalog, Catalogs, Solution, Value, solve};

fn row(name: &str, volts: f64) -> Candidate {
    Candidate {
        name: name.into(),
        label: name.into(),
        columns: vec![(
            "vz".into(),
            Value::Quantity(Quantity::literal(volts, Some("V")).unwrap().quantity),
        )],
        lcsc: None,
        mpn: None,
        library: None,
        assembly: None,
        height: None,
        temp_min: None,
        temp_max: None,
        generic: None,
    }
}

/// Twenty rows of 3 V and 10 V in turn, so that every block has both.
fn mixed() -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "vzs".into(),
        candidates: (0..20)
            .map(|i| {
                if i % 2 == 0 {
                    row(&format!("LOW{i}"), 3.0)
                } else {
                    row(&format!("HIGH{i}"), 10.0)
                }
            })
            .collect(),
    });
    c
}

fn board(relation: &str) -> String {
    format!(
        "part Vz {{\n    .vz: Voltage\n    .limit: Voltage\n    {relation}\n    pins {{\n        \
         .a passive \"A\" \"1\"\n        .b passive \"K\" \"2\"\n    }}\n    catalog vzs {{ .vz }}\n}}\n\
         board B {{\n    .p: Vz {{ .limit &= 5V }}\n}}\n"
    )
}

fn run(src: &str, catalogs: Catalogs) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &catalogs)
}

fn picked(s: &Solution) -> Vec<String> {
    s.picks.iter().map(|p| p.candidate.clone()).collect()
}

#[test]
fn a_block_with_a_row_that_meets_an_end_form_is_not_refused_by_the_hull() {
    // The least of the block's `vz` is 3 V, the greatest 10 V, so the hull's end fails or holds
    // for the whole block while single rows differ. With the audit on, a skipped row that a trial
    // accepts stops the solver.
    for (relation, wanted) in [
        (".limit <= min(.vz)", "HIGH1"),
        ("min(.vz) >= .limit", "HIGH1"),
        ("max(.vz) <= .limit", "LOW0"),
        (".limit >= max(.vz)", "LOW0"),
        // The forms the standard Zener rule uses are safe, and keep working.
        ("min(.limit) <= max(.vz)", "HIGH1"),
        ("max(.vz) >= min(.limit)", "HIGH1"),
    ] {
        let reference = run(&board(relation), mixed().with_narrowing(false));
        let narrowed = run(
            &board(relation),
            mixed().with_narrowing(true).with_narrowing_audit(true),
        );
        assert_eq!(picked(&reference), vec![wanted], "`{relation}` (reference)");
        assert_eq!(picked(&narrowed), vec![wanted], "`{relation}`");
        assert!(
            narrowed.diagnostics.is_empty(),
            "`{relation}`: {:?}",
            narrowed.diagnostics
        );
    }
}

#[test]
fn a_catalog_with_no_row_that_meets_an_end_relation_still_says_so() {
    let s = run(
        &board(".limit <= min(.vz)").replace("5V", "11V"),
        mixed().with_narrowing(true).with_narrowing_audit(true),
    );
    assert!(s.picks.is_empty(), "{:?}", picked(&s));
    assert!(!s.diagnostics.is_empty());
}

/// A generic capacitor (no row has 130 nF) whose rating is bounded by `bound`, beside a rail whose
/// voltage is stated as a range of 5 V to 14 V and narrowed by a resistor's pick (the resistor's
/// rows give 5 V to 6 V across a 1 mA).
fn rating_board(bound: &str) -> String {
    format!(
        "import std::parts::passive::{{Capacitor, Resistor}}\n\
         board B {{\n    .v: Voltage\n    .v &= 5V..14V\n    \
         .r: Resistor {{ .resistance &= 4.9kohm..6.1kohm }}\n    .v == .r.resistance * 1mA\n    \
         .c: Capacitor {{ .capacitance &= 130nF +/- 10% }}\n    {bound}\n}}\n"
    )
}

fn caps() -> Catalogs {
    let mut c = Catalogs::empty().with_series(Some(netsmith_solve::Series::E24));
    c.add_file(
        "capacitor",
        "catalogs/capacitor.csv",
        "lcsc,mpn,name,label,capacitance,effective_capacitance,max_voltage,package,library,source\n\
         C900101,ZZC-100N-16,100nF 16V,100n,100nF +/- 10%,derate capacitance by bias: 0:1 16:0.5,16V,0402,Basic,https://example.invalid/a\n",
    )
    .unwrap();
    c.add_file(
        "resistor",
        "catalogs/resistor.csv",
        "lcsc,mpn,name,label,resistance,max_power,package,library,source\n\
         C900201,ZZR-5K6,5.6k,5.6k,5.6kohm +/- 1%,62.5mW,0402,Basic,https://example.invalid/r\n",
    )
    .unwrap();
    c
}

fn need(s: &Solution) -> String {
    let c = s
        .picks
        .iter()
        .find(|p| p.generic.is_some())
        .expect("a generic pick");
    c.generic.as_ref().unwrap().describe()
}

#[test]
fn a_rating_is_not_bounded_by_the_end_of_a_range_that_a_later_pick_narrows() {
    // Read as a whole field the bound is taken as it always was (14 V, the range's top when the
    // capacitor is resolved). `max(.v)` read at that time is the same 14 V, but the resistor's
    // pick narrows `v` to about 5.6 V afterwards, so the end it read is stale: it gives no bound.
    let whole = run(&rating_board(".c.max_voltage >= .v"), caps());
    assert!(need(&whole).contains("at least 14 V"), "{}", need(&whole));
    let s = run(&rating_board(".c.max_voltage >= max(.v)"), caps());
    assert!(!need(&s).contains("14 V"), "{}", need(&s));
    assert!(!need(&s).contains("rated voltage"), "{}", need(&s));
    let narrowed = match s.model.domain_at("v") {
        Some(netsmith_check::Domain::Quantity(q)) => q.hi(),
        other => panic!("{other:?}"),
    };
    assert!(narrowed < 7.0, "v is {narrowed}");
    // The relation is not judged on the value it never gave: it waits for the rating.
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
}

/// Resistors of 330 ohm and 22 kohm, and a module that holds two of them to one member.
fn lamp(bound: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\n\
         module Lamp {{\n    .balance: Resistance\n    .top: Resistor\n    \
         .top.resistance &= .balance\n}}\n\
         board B {{\n    .floor: Resistance\n    .floor &= 5kohm..20kohm\n    .lamp: Lamp\n    {bound}\n}}\n"
    )
}

fn resistors() -> Catalogs {
    let mut c = Catalogs::empty();
    c.add_file(
        "resistor",
        "catalogs/resistor.csv",
        "lcsc,mpn,name,label,resistance,max_power,package,library,source\n\
         C900301,ZZR-330,330,330,330ohm +/- 1%,1kW,0402,Basic,https://example.invalid/a\n\
         C900302,ZZR-22K,22k,22k,22kohm +/- 1%,1kW,0402,Basic,https://example.invalid/b\n",
    )
    .unwrap();
    c
}

#[test]
fn a_bound_that_reads_an_end_is_not_carried_to_the_part_the_member_is_held_to() {
    // `.lamp.balance >= .floor` steers the resistor to the 22 kohm row.
    let whole = run(&lamp(".lamp.balance >= .floor"), resistors());
    assert_eq!(picked(&whole), vec!["22k"], "{:?}", whole.diagnostics);
    // The same bound on the end of the floor is not carried to the part: the first row is
    // taken. The member has no value of its own (a pick does not flow back to it), so the
    // relation waits and is listed as not evaluated.
    let s = run(&lamp(".lamp.balance >= max(.floor)"), resistors());
    assert_eq!(picked(&s), vec!["330"]);
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let waiting = run(&lamp(""), resistors());
    assert_eq!(s.deferred.len(), waiting.deferred.len() + 1);
}
