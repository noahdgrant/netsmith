//! `netsmith layout capture` (issue #828): the placement written as the layout file, an entry
//! for every part and the seed. The board is the regulator example.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

const BOARD: &str = include_str!("../../../examples/linear_regulator.nsm");

fn project(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"regulator\"\ntop_level = [\"main.nsm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("main.nsm"), BOARD).unwrap();
    dir
}

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .arg("layout")
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The JSON a run that must succeed printed.
fn json(dir: &Path, args: &[&str]) -> serde_json::Value {
    let run = netsmith(dir, args);
    assert_eq!(run.status.code(), Some(0), "{}", text(&run.stderr));
    serde_json::from_str(&text(&run.stdout)).unwrap_or_else(|e| panic!("{e}: {}", text(&run.stdout)))
}

/// Each part's path with its entry, as `layout show` says.
fn shown(dir: &Path) -> Vec<(String, serde_json::Value)> {
    json(dir, &["show", "--format", "json"])["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["path"].as_str().unwrap().to_string(), p["entry"].clone()))
        .collect()
}

#[test]
fn a_dry_run_says_what_would_be_written_and_writes_nothing() {
    let dir = project("layout-capture-dry");
    let captured = json(&dir, &["capture", "--dry-run", "--format", "json"]);
    assert_eq!(captured["written"], false);
    assert_eq!(captured["board"], "RegulatorExample");
    let parts = shown(&dir).len();
    assert_eq!(captured["parts"], parts, "{captured}");
    assert_eq!(captured["added"].as_array().unwrap().len(), parts);
    assert_eq!(captured["kept"], 0);
    assert!(captured["seed_was"].is_null(), "{captured}");
    assert!(!dir.join("main.pcb.toml").exists(), "a dry run wrote the layout file");
}

#[test]
fn capture_writes_an_entry_for_every_part_and_the_seed() {
    let dir = project("layout-capture");
    assert!(shown(&dir).iter().all(|(_, entry)| entry.is_null()));
    let captured = json(&dir, &["capture", "--format", "json"]);
    assert_eq!(captured["written"], true);
    let file = fs::read_to_string(dir.join("main.pcb.toml")).unwrap();
    assert!(file.contains("seed = "), "{file}");
    let entries = shown(&dir);
    assert!(!entries.is_empty());
    for (path, entry) in &entries {
        assert!(entry["at"].is_array(), "{path} has no entry:\n{file}");
        assert_eq!(entry["source"], "auto", "{path}");
        assert_eq!(entry["lock"], false, "{path}");
    }
    // The file is the placement: the board it gives is the one `fab` places with it.
    let check = netsmith(&dir, &["board", "check", "--format", "json"]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check.stdout));

    // A second capture changes nothing: every part has its entry, byte for byte.
    let again = json(&dir, &["capture", "--format", "json"]);
    assert_eq!(again["added"].as_array().unwrap().len(), 0, "{again}");
    assert_eq!(again["kept"], captured["parts"]);
    assert_eq!(again["seed"], captured["seed"]);
    assert_eq!(fs::read_to_string(dir.join("main.pcb.toml")).unwrap(), file);
}

#[test]
fn an_entry_a_person_wrote_keeps_its_lock_its_source_and_its_comment() {
    let dir = project("layout-capture-kept");
    let set = netsmith(&dir, &["set", "stage.output_cap", "--at", "20,12", "--lock"]);
    assert_eq!(set.status.code(), Some(0), "{}", text(&set.stderr));
    let path = dir.join("main.pcb.toml");
    let file = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("# placed by hand, kept\n{file}")).unwrap();

    let captured = json(&dir, &["capture", "--format", "json"]);
    assert_eq!(captured["kept"], 1, "{captured}");
    let entries = shown(&dir);
    let cap = &entries.iter().find(|(p, _)| p == "stage.output_cap").unwrap().1;
    assert_eq!(cap["at"], serde_json::json!([20.0, 12.0]));
    assert_eq!((cap["lock"].clone(), cap["source"].clone()), (true.into(), "hand".into()));
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.starts_with("# placed by hand, kept\n"), "{written}");
    assert_eq!(written.matches("source = \"hand\"").count(), 1, "{written}");
}

#[test]
fn a_file_with_an_error_is_refused_and_left_as_it_was() {
    let dir = project("layout-capture-refused");
    let path = dir.join("main.pcb.toml");
    let bad = "version = 1\n\n[RegulatorExample.parts.nothing]\nat = [1.0, 2.0]\n";
    fs::write(&path, bad).unwrap();
    let run = netsmith(&dir, &["capture"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run.stderr));
    assert!(text(&run.stderr).contains("nothing"), "{}", text(&run.stderr));
    assert_eq!(fs::read_to_string(&path).unwrap(), bad);
}
