//! A project's preferred parts, `preferred.csv` beside `netsmith.toml` (issue #802): the
//! project reads it into its catalogs and names it in a problem. It is not in a catalog folder
//! (the root cannot be one), so it is never read as a catalog.

use netsmith_fabricate::project::{PREFERRED, Project};
use netsmith_testkit::TempDir;
use std::fs;

fn project(manifest: &str, preferred: Option<&str>) -> (TempDir, Project) {
    let root = TempDir::new("preferred");
    fs::write(root.join("netsmith.toml"), manifest).unwrap();
    if let Some(text) = preferred {
        fs::write(root.join(PREFERRED), text).unwrap();
    }
    let project = Project::load(root.path()).unwrap();
    (root, project)
}

const MANIFEST: &str = "[project]\nname = \"invented\"\n";

#[test]
fn a_project_with_no_preferred_file_has_no_list() {
    let (_root, p) = project(MANIFEST, None);
    assert!(p.preferred().unwrap().is_none());
    assert!(p.catalogs().unwrap().preferred().is_none());
}

#[test]
fn the_preferred_file_travels_with_the_projects_catalogs() {
    let (_root, p) = project(
        MANIFEST,
        Some("mpn,note\nZZR-1,on the shelf\nZZR-2,\n"),
    );
    let catalogs = p.catalogs().unwrap();
    assert_eq!(catalogs.preferred().map(|l| l.len()), Some(2));
}

#[test]
fn a_preferred_file_that_is_not_well_formed_stops_the_catalogs_with_its_line() {
    let (_root, p) = project(MANIFEST, Some("mpn\nZZR-1\nZZR-1\n"));
    assert_eq!(
        p.catalogs().unwrap_err(),
        "preferred.csv:3: `ZZR-1` is listed twice; a part is listed once"
    );
}

#[test]
fn a_preferred_file_that_names_parts_by_lcsc_is_refused_with_the_mpn_of_each_number() {
    // The number is the sku of a standard offer: the message says its part, and where.
    let (_root, p) = project(MANIFEST, Some("lcsc\nC25104\n"));
    let e = p.catalogs().unwrap_err();
    assert!(
        e.starts_with("preferred.csv:1: the column `lcsc` has gone from a preferred-parts file (ADR 0143)"),
        "{e}"
    );
    assert!(
        e.ends_with("; line 2: `C25104` is `0402WGF3300TCE` (data/offers/lcsc.csv:LINE)".replace("LINE", &line_of("C25104")).as_str()),
        "{e}"
    );
}

/// The line of the standard offer of `sku`.
fn line_of(sku: &str) -> String {
    let text = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/offers/lcsc.csv")).unwrap();
    let at = text
        .lines()
        .position(|l| l.contains(&format!(",{sku},")))
        .expect("a standard offer");
    (at + 1).to_string()
}
