//! `netsmith project lint` (issue #1104): the findings of a project, the `[lint]` table of its
//! manifest, `--strict`, `--format json` and the exit codes. The rules are tested in
//! `crates/netsmith-lint/tests`.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;
use serde_json::Value;

/// A board with one unused import and a board name against the guide.
const SOURCE: &str = r#"import std::parts::passive::{Resistor, Capacitor}
import std::sources::Rail

board pull_board {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .r: Resistor { .resistance &= 10kohm +/- 5% }
    .supply.pos ~> .r ~> .supply.neg
}
"#;

/// A project of `SOURCE` with `table` after the `[project]` table of its manifest.
fn project(label: &str, table: &str, source: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"pull\"\n\n{table}"),
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
    root
}

fn lint(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["project", "lint"])
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap();
    (
        run.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&run.stdout).into_owned(),
        String::from_utf8_lossy(&run.stderr).into_owned(),
    )
}

#[test]
fn findings_are_warnings_and_strict_makes_them_errors() {
    let root = project("lint-warn", "", SOURCE);
    let (code, _, err) = lint(&root, &[]);
    assert_eq!(code, 0, "{err}");
    assert!(
        err.contains("warning: src/main.nsm:1:40: lint.unused_import: `Capacitor` is imported"),
        "{err}"
    );
    assert!(
        err.contains("warning: src/main.nsm:4:7: lint.naming: `pull_board` is a board"),
        "{err}"
    );
    assert!(err.contains("lint: 2 findings in 1 file"), "{err}");
    let (code, _, err) = lint(&root, &["--strict"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("error: src/main.nsm:1:40: lint.unused_import"), "{err}");
}

#[test]
fn json_has_the_fields_of_check() {
    let root = project("lint-json", "", SOURCE);
    let (code, out, err) = lint(&root, &["--format", "json"]);
    assert_eq!(code, 0, "{err}");
    let report: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(report["command"], "project lint");
    assert_eq!(report["ok"], true);
    let first = &report["findings"][0];
    assert_eq!(first["severity"], "warning");
    assert_eq!(first["code"], "lint.unused_import");
    assert_eq!(first["file"], "src/main.nsm");
    assert_eq!(first["line"], 1);
    assert_eq!(first["column"], 40);
    assert!(first["message"].as_str().unwrap().contains("`Capacitor`"));
}

#[test]
fn the_manifest_turns_a_rule_off_and_makes_findings_errors() {
    let root = project(
        "lint-table",
        "[lint]\noff = [\"unused_import\"]\nfindings = \"error\"\n",
        SOURCE,
    );
    let (code, _, err) = lint(&root, &[]);
    assert_eq!(code, 1, "{err}");
    assert!(!err.contains("lint.unused_import"), "{err}");
    assert!(err.contains("error: src/main.nsm:4:7: lint.naming"), "{err}");
    let clean = SOURCE.replace("board pull_board", "board PullBoard");
    let root = project("lint-clean", "[lint]\noff = [\"unused_import\"]\nfindings = \"error\"\n", &clean);
    let (code, _, err) = lint(&root, &[]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("lint: no findings"), "{err}");
}

#[test]
fn a_rule_that_does_not_exist_and_a_source_that_does_not_check_cannot_run() {
    let root = project("lint-unknown", "[lint]\noff = [\"namign\"]\n", SOURCE);
    let (code, _, err) = lint(&root, &[]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("[lint] off names `namign`, which is not a rule"), "{err}");
    let root = project(
        "lint-broken",
        "",
        &SOURCE.replace(".r: Resistor", ".r: Resistr"),
    );
    let (code, _, err) = lint(&root, &[]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("src/main.nsm does not check (1 error)"), "{err}");
}

#[test]
fn the_linter_writes_nothing() {
    let root = project("lint-writes", "", SOURCE);
    let before: Vec<_> = fs::read_dir(&*root).unwrap().map(|e| e.unwrap().path()).collect();
    let (code, _, err) = lint(&root, &["--strict"]);
    assert_eq!(code, 1, "{err}");
    let after: Vec<_> = fs::read_dir(&*root).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(before.len(), after.len());
    assert_eq!(fs::read_to_string(root.join("src/main.nsm")).unwrap(), SOURCE);
}
