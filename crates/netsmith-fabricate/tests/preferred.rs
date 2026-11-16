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
    let (_root, p) = project(MANIFEST, Some("lcsc,note\nC900001,on the shelf\nC900002,\n"));
    let catalogs = p.catalogs().unwrap();
    assert_eq!(catalogs.preferred().map(|l| l.len()), Some(2));
}

#[test]
fn a_preferred_file_that_is_not_well_formed_stops_the_catalogs_with_its_line() {
    let (_root, p) = project(MANIFEST, Some("lcsc\nC900001\nC900001\n"));
    assert_eq!(
        p.catalogs().unwrap_err(),
        "preferred.csv:3: `C900001` is listed twice; a part is listed once"
    );
}
