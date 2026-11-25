//! The editor lists the warning of a project written for another minor release than this netsmith
//! among its problems, once (issue #1657).

use std::fs;

use netsmith_editor::Backend;
use netsmith_editor::api::{Analysis, Severity};
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const LED: &str = include_str!("../../../examples/led.nsm");
const WARNED: &str = "the project is written for netsmith";

fn project(label: &str, written_for: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"led\"\nnetsmith = \"{written_for}\"\n"),
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    (root, editor)
}

fn warned(analysis: &Analysis) -> usize {
    analysis
        .problems
        .iter()
        .filter(|p| p.message.contains(WARNED))
        .count()
}

#[test]
fn the_problems_start_with_the_warning_of_another_release() {
    let (_root, editor) = project("editor-release-older", "0.0.1");
    let analysis = editor.analysis();
    assert_eq!(warned(&analysis), 1, "{:?}", analysis.problems);
    let first = &analysis.problems[0];
    assert!(first.message.starts_with(WARNED), "{first:?}");
    assert_eq!(first.severity, Severity::Warning);
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let counted = analysis
        .problems
        .iter()
        .filter(|p| p.severity == Severity::Warning)
        .count();
    assert_eq!(analysis.warnings as usize, counted);
    // Of typed text too, once.
    let checked = editor
        .check("src/main.nsm", &format!("{LED}\n// typed\n"))
        .unwrap();
    assert_eq!(warned(&checked), 1, "{:?}", checked.problems);
}

#[test]
fn this_release_has_no_warning() {
    let (_root, editor) = project("editor-release-this", env!("CARGO_PKG_VERSION"));
    assert_eq!(warned(&editor.analysis()), 0);
}
