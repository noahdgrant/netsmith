//! What an empty cell of a catalog row does (ADR 0047, ADR 0055): a row with no value for a
//! column fits no design that says anything about the field, by whatever route, and only a
//! column that a catalog may leave out (the optional ones of the standard kinds) is let
//! through when the design says nothing. Every part and file here is invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn catalogs(text: &str) -> Catalogs {
    let mut c = Catalogs::empty();
    c.add_file("zq", "catalogs/zq.csv", text).unwrap();
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

/// A part with a rail whose output voltage is a column, and a package.
const PART: &str = r#"
import std::sources::Rail

part Zq {
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    pins "SOT-23-5" {
        .vin.pos       passive     "IN"     "1"
        .vin.neg       passive     "GND"    "2"
        .vout.pos      passive     "OUT"    "3"
    }
    .package: Package
    catalog zq { .vout.voltage, .package }
}
"#;

/// The first row has no output voltage and no package; the second has both.
const FILE: &str = "name,package,vout.voltage\n\
bare,,\n\
full,SOT-23-5,3.3V +/- 2%\n";

#[test]
fn a_value_that_reaches_an_empty_cell_through_a_join_excludes_the_row() {
    // `a` is chosen before `r`, so when `a` is chosen the joined output has no value yet.
    // Nothing narrows `a.vout.voltage` and no relation mentions it: only the `~`.
    let src = format!("{PART}\nboard B {{\n    .a: Zq\n    .r: Zq\n    .a.vout ~ .r.vout\n}}\n");
    let s = solved(&src, &catalogs(FILE));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s, "a"), "full");
    assert_eq!(picked(&s, "r"), "full");
}

#[test]
fn a_row_with_an_empty_package_is_never_offered() {
    // Nothing in the design mentions the package, and the part has a pin table for it: a row
    // that does not say which package it is would be picked with no pads to place.
    let src = format!("{PART}\nboard B {{\n    .a: Zq\n}}\n");
    let s = solved(&src, &catalogs(FILE));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s, "a"), "full");
}
