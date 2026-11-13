//! `#[series(...)]` on a project's own generic (ADR 0123): the solver makes it the values of the
//! series, and the project's `series` key chooses among the series it offers. All parts here
//! are invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Series, Solution, resolve, solve};

/// A thermistor of the project's own, offering `series`, with a board that asks for `want`.
fn board(series: &str, want: &str) -> String {
    format!(
        "{series}
generic Thermistor {{
    .designator &= \"RT\"
    pins {{
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }}
    .resistance: Resistance
    .beta: Number
    catalog thermistor {{ .resistance, .beta }}
}}
board B {{
    .t: Thermistor {{
        .resistance &= {want}
        .beta &= 3950
    }}
}}
"
    )
}

fn solved(source: &str, catalogs: &Catalogs) -> Solution {
    let c = check("t.nsm", source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], catalogs)
}

/// One invented row, 10 kohm.
const ROWS: &str = "lcsc,mpn,name,label,resistance,beta,source\n\
C990001,ZZT-10K,10k thermistor,10k,10kohm +/- 1%,3950,https://example.invalid/t\n";

fn catalogs(series: Option<Series>, rows: bool) -> Catalogs {
    let mut c = Catalogs::empty().with_series(series);
    if rows {
        c.add_file("thermistor", "catalogs/thermistor.csv", ROWS)
            .unwrap();
    }
    c
}

#[test]
fn a_generic_that_declares_a_series_gets_the_values_of_the_series() {
    let s = solved(
        &board("#[series(E24)]", "4.4kohm..5kohm"),
        &catalogs(Some(Series::E24), true),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let pick = &s.picks[0];
    assert!(pick.lcsc.is_none());
    let generic = pick.generic.as_ref().expect("a value the solver made");
    assert_eq!(generic.series(), Some(Series::E24));
    assert_eq!(generic.nominal(), Some(4700.0));
    assert_eq!(pick.label, "4.7k");
    // E24 is made at 5% (IEC 60063), and the design's other columns are what it states.
    assert_eq!(generic.describe(), "4.7 kΩ 5% (E24), beta 3950..3950");
}

#[test]
fn a_row_comes_first_and_a_generated_value_resolves_to_a_row_that_fits() {
    let s = solved(
        &board("#[series(E24)]", "9kohm..11kohm"),
        &catalogs(Some(Series::E24), true),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert_eq!(s.picks[0].lcsc.as_deref(), Some("C990001"));
    // A generated 10 k at 5% resolves to the 1% row: as good or better, the same beta.
    let s = solved(
        &board("#[series(E24)]", "9.4kohm..10.6kohm"),
        &catalogs(Some(Series::E24), false),
    );
    let generic = s.picks[0].generic.clone().expect("generated");
    let found = resolve(&generic, &catalogs(Some(Series::E24), true), &|_| None);
    assert_eq!(found.unwrap().lcsc.as_deref(), Some("C990001"));
}

#[test]
fn a_catalog_with_no_file_is_the_generated_values_alone() {
    let s = solved(
        &board("#[series(E12)]", "1.9kohm..2.5kohm"),
        &catalogs(Some(Series::E12), false),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert_eq!(s.picks[0].label, "2.2k");
}

#[test]
fn a_generic_without_a_series_gets_no_generated_values() {
    let s = solved(&board("", "4.4kohm..5kohm"), &catalogs(Some(Series::E24), true));
    assert!(s.picks.is_empty());
    assert!(!s.diagnostics.is_empty());
}

#[test]
fn the_project_series_chooses_among_the_offered_ones() {
    // A project on E96 gives a generic that offers E12 and E24 its E24 values: 4.75 k at 1%,
    // which is E96, would fit this window, and 4.7 k at 5% does not.
    let s = solved(
        &board("#[series(E12, E24)]", "4.5kohm..4.99kohm"),
        &catalogs(Some(Series::E96), false),
    );
    assert!(s.picks.is_empty(), "{:?}", s.picks.first().map(|p| &p.label));
    let s = solved(
        &board("#[series(E12, E24)]", "4.4kohm..5kohm"),
        &catalogs(Some(Series::E96), false),
    );
    assert_eq!(s.picks[0].generic.as_ref().unwrap().series(), Some(Series::E24));
    // A project that states no series gets no generated values, whatever the generic offers.
    let s = solved(&board("#[series(E24)]", "4.4kohm..5kohm"), &catalogs(None, false));
    assert!(s.picks.is_empty());
}

#[test]
fn a_series_is_offered_only_when_the_project_s_is_no_coarser() {
    assert_eq!(Series::E96.within(&[Series::E12, Series::E24]), Some(Series::E24));
    assert_eq!(Series::E24.within(&[Series::E24, Series::E96]), Some(Series::E24));
    assert_eq!(Series::E12.within(&[Series::E24, Series::E96]), None);
    assert_eq!(Series::E24.within(&[]), None);
}
