//! A resistor's `temperature_coefficient` is a catalog column that no standard row states. A
//! design that does not mention it picks rows as before; a design that bounds it is offered
//! only the rows that state it. Every part here is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn board(resistor: &str) -> String {
    format!(
        "import std::parts::passive::Resistor
import std::sources::Rail

board B {{
    .supply: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }}
    .r1: Resistor {{ {resistor} }}
    .supply.pos ~> .r1 ~> .supply.neg
}}
"
    )
}

fn with_rows(rows: &str) -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    c.add_file("resistor", "catalogs/resistor.csv", rows)
        .unwrap();
    c
}

fn solved(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// Two invented 100 ohm rows: one states 50 ppm/K, one states nothing.
const ROWS: &str = "mpn,resistance,max_power,package,temperature_coefficient,library,source
ZZR-STATED,100ohm +/- 1%,62.5mW,0402,0.00005,Extended,https://example.invalid/a
ZZR-BLANK,100ohm +/- 1%,62.5mW,0402,,Extended,https://example.invalid/b
";

#[test]
fn a_design_that_does_not_mention_it_is_offered_every_row_as_before() {
    // The first fitting standard row, with the column empty in all of them.
    let s = solved(&board(".resistance &= 100ohm +/- 1%"), &Catalogs::jlcpcb());
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks.len(), 1);
    assert!(
        s.picks[0].lcsc.is_some(),
        "a standard row, not a made-up one"
    );
    // The field stays unset: nothing says what the picked row's coefficient is.
    assert_eq!(
        s.model.domain_at("r1.temperature_coefficient"),
        Some(&Domain::Unset)
    );
}

#[test]
fn a_design_that_bounds_it_gets_only_a_row_that_states_it() {
    let c = with_rows(ROWS);
    let s = solved(
        &board(".resistance &= 100ohm +/- 1%\n .temperature_coefficient <= 0.0001"),
        &c,
    );
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZR-STATED");
    match s.model.domain_at("r1.temperature_coefficient") {
        Some(Domain::Quantity(q)) => assert_eq!((q.lo(), q.hi()), (0.00005, 0.00005)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_bound_the_row_does_not_meet_is_refused_with_the_numbers() {
    let c = with_rows(ROWS);
    let s = solved(
        &board(".resistance &= 100ohm +/- 1%\n .temperature_coefficient <= 0.00002"),
        &c,
    );
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("r1.temperature_coefficient")
            && m[0].contains("0.00002")
            && m[0].contains("0.00005"),
        "{}",
        m[0]
    );
}

#[test]
fn with_only_standard_rows_a_bound_has_nothing_to_pick_and_says_why() {
    let s = solved(
        &board(".resistance &= 100ohm +/- 1%\n .temperature_coefficient <= 0.0001"),
        &Catalogs::jlcpcb(),
    );
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("the `resistor` catalog has nothing for `r1`")
            && m[0].contains("219 for `temperature_coefficient`"),
        "{}",
        m[0]
    );
    // A made-up generic part is no way round it: it states no coefficient either.
    assert!(s.picks.is_empty());
}

#[test]
fn a_message_about_something_else_does_not_blame_the_empty_column() {
    // No 100 ohm 0402 row has this much power rating, and the design never mentions the
    // coefficient, so the empty cells are not why.
    let s = solved(
        &board(".resistance &= 100ohm +/- 1%\n .max_power >= 100W"),
        &Catalogs::jlcpcb(),
    );
    let m = messages(&s);
    assert!(!m.is_empty());
    assert!(
        m.iter().all(|m| !m.contains("temperature_coefficient")),
        "{m:#?}"
    );
}

#[test]
fn a_file_may_leave_the_column_out_and_only_that_column() {
    // A project file written before the column existed is read as empty there.
    let old = "mpn,resistance,max_power,package,library,source
ZZR-OLD,7.77ohm +/- 1%,62.5mW,0402,Extended,https://example.invalid/c
";
    let c = with_rows(old);
    let s = solved(&board(".resistance &= 7.77ohm +/- 1%"), &c);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZR-OLD");
    // Any other column the part lists stays required.
    let no_power = "mpn,resistance,package,library,source
ZZR-NO,8.88ohm +/- 1%,0402,Extended,https://example.invalid/d
";
    let c = with_rows(no_power);
    let s = solved(&board(".resistance &= 8.88ohm +/- 1%"), &c);
    let m = messages(&s);
    assert!(
        m.iter()
            .any(|m| m.contains("there is no `max_power` column")),
        "{m:#?}"
    );
}
