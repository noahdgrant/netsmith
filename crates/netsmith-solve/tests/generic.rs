//! Generic resistors (ADR 0047): the solver may pick an E-series value that no catalog row
//! has, and a separate step resolves it to a row. All parts here are invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Library, Series, Solution, resolve, solve};

/// A divider-less design: one resistor in series with an LED-like load is not needed; a
/// plain resistor with a stated value is enough to choose a value.
fn board(resistance: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\nboard B {{\n    .r: Resistor {{ .resistance &= {resistance} }}\n}}\n"
    )
}

fn solved(source: &str, catalogs: &Catalogs) -> Solution {
    let c = check("t.nsm", source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], catalogs)
}

/// Invented rows: 330 ohm Basic, 330 ohm Extended (first in the file), 360 ohm Basic, and
/// a 0603 330 ohm. Real-looking numbers are avoided: the numbers are made up.
const ROWS: &str = "lcsc,mpn,name,label,resistance,max_power,package,library,source\n\
C900002,ZZR-330-E,330Ω Extended,330,330ohm +/- 1%,62.5mW,0402,Extended,https://example.invalid/a\n\
C900001,ZZR-330-B,330Ω Basic,330,330ohm +/- 1%,62.5mW,0402,Basic,https://example.invalid/b\n\
C900003,ZZR-360-B,360Ω Basic,360,360ohm +/- 1%,62.5mW,0402,Basic,https://example.invalid/c\n\
C900004,ZZR-330-6,330Ω 0603,330,330ohm +/- 1%,100mW,0603,Basic,https://example.invalid/d\n";

fn catalogs(series: Option<Series>) -> Catalogs {
    let mut c = Catalogs::empty().with_series(series);
    c.add_file("resistor", "catalogs/resistor.csv", ROWS)
        .unwrap();
    c
}

#[test]
fn without_a_series_a_value_with_no_row_blocks_the_build_as_before() {
    let s = solved(&board("340ohm +/- 1%"), &catalogs(None));
    assert!(s.picks.is_empty());
    assert_eq!(s.diagnostics.len(), 1, "{:?}", s.diagnostics);
}

#[test]
fn with_a_series_the_solver_picks_a_generic_part_for_a_value_no_row_has() {
    // 340 is not in E24 (330 and 360 are), so ask for what E24 has and the rows do not.
    let s = solved(&board("390ohm +/- 1%"), &catalogs(Some(Series::E24)));
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert_eq!(s.picks.len(), 1);
    let pick = &s.picks[0];
    assert!(pick.lcsc.is_none() && pick.mpn.is_none());
    let generic = pick.generic.as_ref().expect("a generic part");
    assert_eq!(generic.series(), Some(Series::E24));
    assert_eq!(
        (generic.nominal(), generic.package.as_str()),
        (Some(390.0), "0402")
    );
    assert_eq!(pick.label, "390");
    assert_eq!(generic.describe(), "390 ohm 1% 0402 62.5 mW (E24)");
}

#[test]
fn a_value_that_has_a_row_still_gets_the_row_and_the_rows_first_rule_is_unchanged() {
    let s = solved(&board("330ohm +/- 1%"), &catalogs(Some(Series::E24)));
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert_eq!(s.picks[0].lcsc.as_deref(), Some("C900002"));
    assert!(s.picks[0].generic.is_none());
}

#[test]
fn a_value_outside_the_series_is_still_refused() {
    // No E24 value, and no row, lies between 335 and 345 ohm.
    let s = solved(&board("335ohm..345ohm"), &catalogs(Some(Series::E24)));
    assert!(s.picks.is_empty());
    assert_eq!(s.diagnostics.len(), 1);
}

#[test]
fn e96_offers_values_e24_does_not() {
    let want = board("2.21kohm +/- 1%");
    assert!(solved(&want, &catalogs(Some(Series::E24))).picks.is_empty());
    let s = solved(&want, &catalogs(Some(Series::E96)));
    assert_eq!(s.picks[0].generic.as_ref().unwrap().nominal(), Some(2210.0));
}

#[test]
fn a_catalog_that_has_no_generic_parts_gets_none() {
    use netsmith_check::FieldKind;
    use netsmith_quantity::Dimension;
    let mut c = Catalogs::empty().with_series(Some(Series::E24));
    c.add_file("fuse", "catalogs/fuse.csv", "name,rated_current\nzz,1A\n")
        .unwrap();
    let kinds = [(
        "rated_current".to_string(),
        FieldKind::Quantity(Dimension::CURRENT),
    )];
    let got = c.resolve("fuse", &kinds, &[]).unwrap().unwrap();
    assert_eq!(got.candidates.len(), 1);
    assert!(got.candidates[0].generic.is_none());
}

// ---- resolution --------------------------------------------------------------------

fn generic_of(source: &str, series: Series) -> netsmith_solve::Generic {
    let s = solved(source, &catalogs(Some(series)));
    s.picks[0].generic.clone().expect("a generic part")
}

/// Change the value and the power rating of a generic resistor.
fn set_resistor(g: &mut netsmith_solve::Generic, ohms: f64, watts: f64) {
    match &mut g.spec {
        netsmith_solve::Spec::Resistor {
            nominal, max_power, ..
        } => {
            *nominal = ohms;
            *max_power = watts;
        }
        other => panic!("not a resistor: {other:?}"),
    }
}

fn never(_: &str) -> Option<u64> {
    None
}

#[test]
fn a_generic_part_resolves_to_a_row_that_is_at_least_as_good_basic_first() {
    // A generic 330 ohm 1% 0402 is what the rows also have; E24 offers 330 but a row is
    // found first by the solver, so build the generic directly.
    let mut g = generic_of(&board("390ohm +/- 1%"), Series::E24);
    set_resistor(&mut g, 330.0, 0.0625);
    let got = resolve(&g, &catalogs(Some(Series::E24)), &never).unwrap();
    // Two 0402 rows have the value; the Basic one is preferred though it is second.
    assert_eq!(got.lcsc.as_deref(), Some("C900001"));
    assert_eq!(got.library, Some(Library::Basic));
}

#[test]
fn a_package_and_a_power_rating_are_matched_too() {
    let mut g = generic_of(&board("390ohm +/- 1%"), Series::E24);
    set_resistor(&mut g, 330.0, 0.1);
    g.package = "0603".into();
    assert_eq!(
        resolve(&g, &catalogs(None), &never)
            .unwrap()
            .lcsc
            .as_deref(),
        Some("C900004")
    );
    // More power than the 0603 row has: nothing fits.
    set_resistor(&mut g, 330.0, 0.125);
    assert!(resolve(&g, &catalogs(None), &never).is_err());
}

#[test]
fn a_row_the_provider_says_has_no_stock_is_passed_over_and_an_unknown_stock_is_not() {
    let mut g = generic_of(&board("390ohm +/- 1%"), Series::E24);
    set_resistor(&mut g, 330.0, 0.0625);
    let c = catalogs(None);
    let sold_out = |lcsc: &str| (lcsc == "C900001").then_some(0);
    let got = resolve(&g, &c, &sold_out).unwrap();
    assert_eq!(got.lcsc.as_deref(), Some("C900002"));
    assert_eq!(got.passed_over, ["330Ω Basic has no stock"]);
    // Unknown (None) never excludes a row.
    assert_eq!(
        resolve(&g, &c, &never).unwrap().lcsc.as_deref(),
        Some("C900001")
    );
    // Every fitting row sold out: unresolved, and the message says why.
    let none_left = |_: &str| Some(0);
    let e = resolve(&g, &c, &none_left).unwrap_err();
    assert!(e.contains("Rows that fit were passed over"), "{e}");
}

#[test]
fn an_unresolved_part_names_what_is_wanted_and_the_nearest_rows() {
    let g = generic_of(&board("390ohm +/- 1%"), Series::E24);
    let e = resolve(&g, &catalogs(Some(Series::E24)), &never).unwrap_err();
    assert!(
        e.starts_with(
            "unresolved: no row of the `resistor` catalog can be ordered for 390 ohm 1% 0402 62.5 mW (E24)."
        ),
        "{e}"
    );
    // 360 is the nearest value (-7.7%), then the two 330s, Basic first; same package first.
    assert!(
        e.contains("Nearest in the catalog: 360Ω Basic (-7.7%, Basic), 330Ω Basic (-15.4%, Basic), 330Ω Extended (-15.4%, Extended)."),
        "{e}"
    );
    assert!(
        e.contains("Change the value in the design and build again"),
        "{e}"
    );
}
