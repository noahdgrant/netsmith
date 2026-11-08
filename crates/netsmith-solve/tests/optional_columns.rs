//! A project's own part declares an optional catalog column in its `catalog` statement
//! (`.rating optional`, ADR 0078): a file may leave the column out, and a row may leave the
//! cell empty and is still offered to a design that says nothing about the field. A column
//! that is not marked is as it was: an empty cell takes the row out, and a file must have the
//! column. Every part and file here is invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn catalogs(text: &str) -> Catalogs {
    let mut c = Catalogs::empty();
    c.add_file("zo", "catalogs/zo.csv", text).unwrap();
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

fn picked(s: &Solution, path: &str) -> String {
    s.picks
        .iter()
        .find(|p| s.model.path_of(p.part) == path)
        .unwrap_or_else(|| panic!("`{path}` was not picked: {:?}", messages(s)))
        .candidate
        .clone()
}

/// A part with three columns: a voltage and a package that a row must state, and a rating
/// that the `catalog` statement marks optional.
fn part(marker: &str) -> String {
    format!(
        r#"
import std::sources::Rail

part Zo {{
    .vin: Rail
    .rating: Current
    pins "SOT-23-5" {{
        .vin.pos       passive     "IN"     "1"
        .vin.neg       passive     "GND"    "2"
    }}
    .package: Package
    catalog zo {{ .vin.voltage, .package, .rating{marker} }}
}}
"#
    )
}

/// The first row leaves the rating empty, the second states it.
const FILE: &str = "name,package,vin.voltage,rating\n\
unrated,SOT-23-5,3.3V,\n\
rated,SOT-23-5,3.3V,2A\n";

/// A file written before the column existed.
const FILE_WITHOUT_THE_COLUMN: &str = "name,package,vin.voltage\n\
older,SOT-23-5,3.3V\n";

fn board(body: &str) -> String {
    format!("\nboard B {{\n    .a: Zo\n{body}}}\n")
}

#[test]
fn a_row_with_an_empty_optional_cell_is_offered_to_a_design_that_says_nothing_about_it() {
    let src = format!("{}{}", part(" optional"), board(""));
    let s = solved(&src, &catalogs(FILE));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    // The first row in the file's order is the empty one.
    assert_eq!(picked(&s, "a"), "unrated");
}

#[test]
fn a_design_that_states_the_optional_column_is_offered_only_the_rows_that_record_it() {
    let src = format!("{}{}", part(" optional"), board("    .a.rating >= 1A\n"));
    let s = solved(&src, &catalogs(FILE));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s, "a"), "rated");
}

#[test]
fn a_file_may_leave_an_optional_column_out() {
    let src = format!("{}{}", part(" optional"), board(""));
    let s = solved(&src, &catalogs(FILE_WITHOUT_THE_COLUMN));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s, "a"), "older");
    // A design that states it gets nothing from such a file, and the message counts the rows.
    let src = format!("{}{}", part(" optional"), board("    .a.rating >= 1A\n"));
    let s = solved(&src, &catalogs(FILE_WITHOUT_THE_COLUMN));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("the `zo` catalog has nothing for `a`") && m[0].contains("1 for `rating`"),
        "{}",
        m[0]
    );
}

#[test]
fn a_column_that_is_not_marked_takes_a_row_with_an_empty_cell_out() {
    let src = format!("{}{}", part(""), board(""));
    let s = solved(&src, &catalogs(FILE));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s, "a"), "rated");
}

#[test]
fn a_column_that_is_not_marked_must_be_in_the_file() {
    let src = format!("{}{}", part(""), board(""));
    let s = solved(&src, &catalogs(FILE_WITHOUT_THE_COLUMN));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("there is no `rating` column, which `catalog zo` lists"),
        "{}",
        m[0]
    );
}

#[test]
fn only_the_marked_column_may_be_empty() {
    // The voltage is not marked: a row with no voltage is never offered, whatever the rating.
    let file = "name,package,vin.voltage,rating\n\
bare,SOT-23-5,,\n\
full,SOT-23-5,3.3V,\n";
    let src = format!("{}{}", part(" optional"), board(""));
    let s = solved(&src, &catalogs(file));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s, "a"), "full");
}
