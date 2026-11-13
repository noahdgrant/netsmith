//! A stated package is checked against the part's catalog, and a colour is a variant of
//! `Color` in the catalog as in the source (ADR 0118). The rows here are invented.

use netsmith_check::{Checked, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const LEDS: &str = "lcsc,mpn,name,label,color,forward_voltage,package,library,source\n\
C900401,ZZ-RED-0603,Red 0603,ZZ-RED,Red,1.8V..2.2V,0603,Basic,https://example.invalid/a\n\
C900402,ZZ-GRN-0402,Green 0402,ZZ-GRN,Green,2.0V..2.4V,0402,Extended,https://example.invalid/b\n\
C900403,ZZ-GRN-0603,Green 0603,ZZ-GRN,Green,2.0V..2.4V,0603,Extended,https://example.invalid/c\n";

fn led(statements: &str, rows: &str) -> (Checked, Solution) {
    let source =
        format!("import std::parts::passive::{{Color, Led}}\nboard B {{\n    .led: Led {{ {statements} }}\n}}\n");
    let c = check("t.nsm", &source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let mut catalogs = Catalogs::empty();
    catalogs.add_file("led", "catalogs/led.csv", rows).unwrap();
    let s = solve(&c.boards[0], &catalogs);
    (c, s)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_package_of_the_catalog_picks_a_row_in_it() {
    let (_, s) = led(".color &= Color::Green; .package &= \"0603\"", LEDS);
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert_eq!(s.picks[0].lcsc.as_deref(), Some("C900403"));
}

#[test]
fn a_package_no_row_is_in_is_refused_at_its_statement_with_the_nearest() {
    let (c, s) = led(".color &= Color::Green; .package &= \"603\"", LEDS);
    assert_eq!(
        messages(&s),
        [
            "`led.package` is \"603\", and no row of the `led` catalog is in that package; its packages are \"0402\", \"0603\"; did you mean \"0603\"?"
        ]
    );
    let d = &s.diagnostics[0];
    assert_eq!(
        c.sources.statement(d.source, d.span).as_deref(),
        Some(".package &= \"603\"")
    );
    assert!(s.picks.is_empty(), "{:?}", s.picks);
}

#[test]
fn a_package_far_from_every_row_lists_the_packages_and_names_none() {
    let (_, s) = led(".package &= \"SOT-23\"", LEDS);
    assert_eq!(
        messages(&s),
        [
            "`led.package` is \"SOT-23\", and no row of the `led` catalog is in that package; its packages are \"0402\", \"0603\""
        ]
    );
}

#[test]
fn a_colour_cell_that_is_no_variant_of_color_is_refused() {
    // A project catalog written before `Color` was an enum.
    let rows = LEDS.replace(",Green,2.0V..2.4V,0402,", ",GREEN,2.0V..2.4V,0402,");
    let (_, s) = led(".color &= Color::Green", &rows);
    let all = messages(&s);
    assert_eq!(all.len(), 1, "{all:?}");
    assert!(
        all[0].contains("the `color` of `Green 0402` in the `led` catalog is \"GREEN\", which is not a variant of `Color`"),
        "{all:?}"
    );
}
