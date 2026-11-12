//! Structured and quiet output for agents (#455): `--format json`, `--notes`, `check`, `fab
//! --quiet`, `compare --format json` and the `parity` command.

use std::{fs, path::Path, process::Command};

use netsmith_kicad::testlib::TempDir;
use serde_json::Value;

const LED: &str = include_str!("../../../examples/led.nsm");

fn examples() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// A copy of the `two_buck` example, a project whose board has notes and a guideline warning.
fn two_buck(label: &str) -> TempDir {
    let root = TempDir::new(label);
    for file in [
        "netsmith.toml",
        "kicad.toml",
        "two_buck.nsm",
        "two_buck.lock.toml",
    ] {
        fs::copy(examples().join("two_buck").join(file), root.join(file)).unwrap();
    }
    root
}

fn led(label: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    root
}

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env(
            "NETSMITH_KICAD_SYMBOLS",
            netsmith_kicad::testlib::sample_symbols(),
        )
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_kicad::testlib::sample_footprints(),
        )
        .output()
        .unwrap()
}

fn stderr(run: &std::process::Output) -> String {
    String::from_utf8_lossy(&run.stderr).to_string()
}

/// The whole of standard output, which must be one JSON document.
fn json(run: &std::process::Output) -> Value {
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not one JSON document ({e}):\n{}",
            String::from_utf8_lossy(&run.stdout)
        )
    })
}

fn note_lines(text: &str) -> Vec<&str> {
    text.lines().filter(|l| l.starts_with("note: ")).collect()
}

#[test]
fn check_writes_nothing_and_prints_the_findings_only() {
    let root = two_buck("output-check");
    let lock = fs::read(root.join("two_buck.lock.toml")).unwrap();
    let run = netsmith(&root, &["check"]);
    assert!(run.status.success(), "{}", stderr(&run));
    // No bill of materials, no `wrote`.
    assert!(
        run.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    assert!(stderr(&run).contains("warning: TwoBuck: unconstrained pick guideline"));
    assert!(!root.join("out").exists());
    assert_eq!(fs::read(root.join("two_buck.lock.toml")).unwrap(), lock);
    // A check with no lock beside it writes none either.
    fs::remove_file(root.join("two_buck.lock.toml")).unwrap();
    assert!(netsmith(&root, &["check"]).status.success());
    assert!(!root.join("two_buck.lock.toml").exists());
}

#[test]
fn a_board_that_does_not_build_is_exit_one_and_a_finding_with_its_stage_and_place() {
    let root = led("output-check-error");
    let broken = LED.replacen(".r1: Resistor", ".r1: Resistr", 1);
    assert_ne!(broken, LED);
    fs::write(root.join("src/main.nsm"), broken).unwrap();
    let run = netsmith(&root, &["check"]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert!(stderr(&run).contains("error:"), "{}", stderr(&run));
    let run = netsmith(&root, &["check", "--format", "json"]);
    assert_eq!(run.status.code(), Some(1));
    let doc = json(&run);
    assert_eq!(doc["command"], "check");
    assert_eq!(doc["ok"], false);
    let error = &doc["findings"][0];
    assert_eq!(error["severity"], "error", "{doc:#}");
    assert_eq!(error["code"], "check", "{doc:#}");
    assert!(
        error["file"].as_str().unwrap().ends_with("main.nsm"),
        "{doc:#}"
    );
    assert!(error["line"].as_u64().unwrap() > 0, "{doc:#}");
    assert!(
        error["message"].as_str().unwrap().contains("Resistr"),
        "{doc:#}"
    );
}

#[test]
fn notes_summary_counts_the_notes_by_cause_and_none_leaves_them_out() {
    let root = two_buck("output-notes");
    let all = stderr(&netsmith(&root, &["check", "--notes", "all"]));
    let listed = note_lines(&all).len();
    assert!(listed > 1, "{all}");
    let summary = stderr(&netsmith(&root, &["check", "--notes", "summary"]));
    let lines = note_lines(&summary);
    assert_eq!(lines.len(), 1, "{summary}");
    assert!(
        lines[0].starts_with(&format!("note: TwoBuck: {listed} notes: "))
            && lines[0].contains("not_stated")
            && lines[0].ends_with("(`--notes all` lists them)"),
        "{summary}"
    );
    let none = stderr(&netsmith(&root, &["check", "--notes", "none"]));
    assert!(note_lines(&none).is_empty(), "{none}");
    // The warnings stay whatever `--notes` says.
    assert!(none.contains("warning: TwoBuck:"), "{none}");
}

#[test]
fn json_lists_the_notes_with_their_cause_or_counts_them() {
    let root = two_buck("output-json-notes");
    let listed = note_lines(&stderr(&netsmith(&root, &["check"]))).len();
    let doc = json(&netsmith(&root, &["check", "--format", "json"]));
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["boards"][0]["board"], "TwoBuck");
    let findings = doc["findings"].as_array().unwrap();
    let notes: Vec<&Value> = findings
        .iter()
        .filter(|f| f["severity"] == "note")
        .collect();
    assert_eq!(notes.len(), listed, "{doc:#}");
    assert_eq!(doc["notes"]["total"], listed);
    for note in &notes {
        assert!(
            note["code"].as_str().unwrap().starts_with("note."),
            "{note}"
        );
        assert_eq!(note["board"], "TwoBuck");
    }
    let warning = findings
        .iter()
        .find(|f| f["severity"] == "warning")
        .unwrap();
    assert_eq!(warning["code"], "guideline");
    assert_eq!(warning["path"], "buck_5v.stage.divider.rtop");
    // `summary` keeps the counts and leaves the notes out of the findings.
    let doc = json(&netsmith(
        &root,
        &["check", "--format", "json", "--notes", "summary"],
    ));
    assert_eq!(doc["notes"]["total"], listed);
    assert!(
        doc["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["severity"] != "note")
    );
}

#[test]
fn build_as_json_says_what_it_wrote_and_prints_nothing_else_on_stdout() {
    let root = led("output-build-json");
    let run = netsmith(&root, &["build", "--format", "json"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let doc = json(&run);
    assert_eq!(doc["command"], "build");
    assert_eq!(doc["boards"][0]["board"], "LedExample");
    let dir = doc["boards"][0]["dir"].as_str().unwrap();
    assert!(
        Path::new(dir).join("LedExample.kicad_sch").is_file(),
        "{dir}"
    );
    assert!(root.join("src/main.lock.toml").is_file());
}

#[test]
fn fab_quiet_prints_the_result_and_not_the_stages() {
    let root = led("output-fab-quiet");
    let loud = netsmith(&root, &["fab"]);
    assert!(loud.status.success(), "{}", stderr(&loud));
    let quiet = netsmith(&root, &["fab", "--quiet"]);
    assert!(quiet.status.success(), "{}", stderr(&quiet));
    assert!(String::from_utf8_lossy(&quiet.stdout).contains("wrote "));
    // Every line of a quiet run's stderr is a finding or the notes before ordering, and it
    // has fewer than a loud run's.
    let said = stderr(&quiet);
    assert!(
        said.lines().count() < stderr(&loud).lines().count(),
        "{said}\n---\n{}",
        stderr(&loud)
    );
    assert!(!said.contains("routing"), "{said}");
    let doc = json(&netsmith(&root, &["fab", "--format", "json"]));
    assert_eq!(doc["command"], "fab");
    assert_eq!(doc["ok"], true);
    assert!(
        Path::new(doc["boards"][0]["dir"].as_str().unwrap())
            .join("fab-notes.txt")
            .is_file(),
        "{doc:#}"
    );
}

#[test]
fn compare_as_json_says_match_and_the_pairs() {
    let root = two_buck("output-compare");
    let run = netsmith(
        &root,
        &[
            "compare",
            "two_buck.nsm",
            "two_buck.nsm",
            "--format",
            "json",
        ],
    );
    assert!(run.status.success(), "{}", stderr(&run));
    let doc = json(&run);
    assert_eq!(doc["command"], "compare");
    assert_eq!(doc["verdict"], "match");
    assert!(!doc["components"].as_array().unwrap().is_empty());
    assert_eq!(doc["a"]["path"], "two_buck.nsm");
}

#[test]
fn parity_is_a_command_with_a_json_report() {
    // The charger's reference board against itself, with a map that pairs each part with itself.
    let reference = examples().join("power_manifold_charger/reference");
    let dir = TempDir::new("output-parity");
    let map: String = fs::read_to_string(reference.join("designators.toml"))
        .unwrap()
        .lines()
        .filter_map(|l| l.split_once(" = "))
        .filter(|(k, _)| !k.starts_with('#'))
        .map(|(k, _)| format!("{k} = \"{k}\"\n"))
        .collect();
    fs::write(dir.join("identity.toml"), map).unwrap();
    let pcb = reference.join("charger-module.kicad_pcb");
    let table = reference.join("placements.toml");
    let args = |format: &'static str| {
        vec![
            "parity".to_string(),
            pcb.display().to_string(),
            "--table".into(),
            table.display().to_string(),
            "--map".into(),
            "identity.toml".into(),
            "--format".into(),
            format.into(),
        ]
    };
    let text = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args("text"))
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(text.status.success(), "{}", stderr(&text));
    assert!(String::from_utf8_lossy(&text.stdout).contains("must-match: "));
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args("json"))
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(run.status.success(), "{}", stderr(&run));
    let doc = json(&run);
    assert_eq!(doc["command"], "parity");
    assert_eq!(doc["passed"], true);
    assert_eq!(doc["must_match"]["failed"], 0);
    assert!(doc["must_match"]["ok"].as_u64().unwrap() > 0);
    let item = &doc["items"][0];
    for key in [
        "id",
        "field",
        "subject",
        "must_match",
        "verdict",
        "tolerance",
    ] {
        assert!(item.get(key).is_some(), "{key}: {item}");
    }
    // A file that is not there is exit 2.
    let missing = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args([
            "parity",
            "nowhere.kicad_pcb",
            "--table",
            "x.toml",
            "--map",
            "identity.toml",
        ])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));
}
