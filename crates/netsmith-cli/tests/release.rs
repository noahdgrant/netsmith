//! The release a project is written for (issue #740): a project whose check fails is told when it
//! was written for another netsmith, and where the changes are listed.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;
use serde_json::Value;

const LED: &str = include_str!("../../../examples/led.nsm");
const MIGRATING: &str = "docs/MIGRATING.md";
const THIS: &str = env!("CARGO_PKG_VERSION");

/// A project of the LED example whose manifest has the lines `release` after the name, and whose board
/// does not build when `broken`.
fn project(label: &str, release: &str, broken: bool) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"led\"\n{release}"),
    )
    .unwrap();
    let source = if broken {
        LED.replacen(".r1: Resistor", ".r1: Resistr", 1)
    } else {
        LED.to_string()
    };
    assert_eq!(source != LED, broken, "the example changed");
    fs::write(root.join("src/main.nsm"), source).unwrap();
    root
}

fn check(cwd: &Path, json: bool) -> std::process::Output {
    let mut args = vec!["board", "check"];
    if json {
        args.extend(["--format", "json"]);
    }
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap()
}

fn stderr(run: &std::process::Output) -> String {
    String::from_utf8_lossy(&run.stderr).into_owned()
}

#[test]
fn a_failed_check_of_an_older_project_points_to_the_migration_guide() {
    let root = project("release-older", "netsmith = \"0.0.1\"\n", true);
    let run = check(&root, false);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    let said = stderr(&run);
    let note = said
        .lines()
        .find(|l| l.contains(MIGRATING))
        .unwrap_or_else(|| panic!("no note of the release: {said}"));
    assert!(note.starts_with("note: the project is written for netsmith 0.0.1"), "{note}");
    assert!(note.contains(&format!("set `netsmith = \"{THIS}\"`")), "{note}");

    // With `--format json` the note is a finding of its own, after the errors.
    let run = check(&root, true);
    assert_eq!(run.status.code(), Some(1));
    let doc: Value = serde_json::from_slice(&run.stdout).unwrap();
    let findings = doc["findings"].as_array().unwrap();
    let last = findings.last().unwrap();
    assert_eq!(last["severity"], "note", "{doc:#}");
    assert_eq!(last["code"], "release", "{doc:#}");
    assert!(last["message"].as_str().unwrap().contains(MIGRATING), "{doc:#}");
}

#[test]
fn a_project_with_no_release_was_made_before_the_key() {
    let root = project("release-none", "", true);
    let said = stderr(&check(&root, false));
    assert!(
        said.contains("does not say which netsmith release the project is written for"),
        "{said}"
    );
    assert!(said.contains(MIGRATING), "{said}");
}

#[test]
fn a_project_of_this_release_or_one_that_builds_gets_no_note() {
    // Written for this netsmith: an error is only an error.
    let root = project("release-this", &format!("netsmith = \"{THIS}\"\n"), true);
    let run = check(&root, false);
    assert_eq!(run.status.code(), Some(1));
    assert!(!stderr(&run).contains(MIGRATING), "{}", stderr(&run));
    // Written for an older one, but nothing failed: nothing to migrate.
    let root = project("release-builds", "netsmith = \"0.0.1\"\n", false);
    let run = check(&root, false);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(!stderr(&run).contains(MIGRATING), "{}", stderr(&run));
}
