//! `netsmith layout regenerate` (issue #825): one module placed again with the rest held, the
//! proposal printed, and written to the layout file only with `--write`. The board is the
//! regulator example, whose `stage` module has three parts.

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

/// The proposal of `seed` as JSON, after a run that must succeed.
fn proposal(dir: &Path, extra: &[&str]) -> serde_json::Value {
    let mut args = vec!["regenerate", "stage", "--format", "json"];
    args.extend_from_slice(extra);
    let run = netsmith(dir, &args);
    assert_eq!(run.status.code(), Some(0), "{}", text(&run.stderr));
    serde_json::from_str(&text(&run.stdout)).unwrap_or_else(|e| panic!("{e}: {}", text(&run.stdout)))
}

#[test]
fn regenerate_proposes_without_writing_and_writes_with_write() {
    let dir = project("layout-regenerate");
    // A seed that moves a part (a seed may find the arrangement that is there).
    let (seed, json) = (0..12)
        .map(|seed| (seed, proposal(&dir, &["--seed", &seed.to_string()])))
        .find(|(_, json)| {
            json["parts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["was"]["at"] != p["after"]["at"] || p["was"]["rotation"] != p["after"]["rotation"])
        })
        .expect("a seed moves a part");
    assert_eq!(json["module"], "stage");
    assert_eq!(json["written"], false);
    let parts: Vec<&str> = json["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["path"].as_str().unwrap())
        .collect();
    assert_eq!(parts.len(), 3, "{json}");
    assert!(parts.iter().all(|p| p.starts_with("stage.")), "{parts:?}");
    assert!(!json["nets"].as_array().unwrap().is_empty(), "{json}");
    assert!(json["comparison"]["before"]["hpwl"].as_f64().is_some(), "{json}");
    assert!(!dir.join("main.pcb.toml").exists(), "a proposal wrote the layout file");

    // The same seed again is the same proposal.
    let again = proposal(&dir, &["--seed", &seed.to_string()]);
    assert_eq!(again["parts"], json["parts"]);

    // Written: an entry for each part placed again, unlocked, `source = "regen"`.
    let written = proposal(&dir, &["--seed", &seed.to_string(), "--write"]);
    assert_eq!(written["written"], true);
    let file = fs::read_to_string(dir.join("main.pcb.toml")).unwrap();
    for part in &parts {
        assert!(file.contains(&format!("[RegulatorExample.parts.\"{part}\"]")) || file.contains(&format!("[RegulatorExample.parts.{part}]")), "{part} has no entry:\n{file}");
    }
    assert_eq!(file.matches("source = \"regen\"").count(), 3, "{file}");
    // The board placed with the file has the parts where the proposal put them.
    let show = netsmith(&dir, &["show", "stage", "--format", "json"]);
    assert_eq!(show.status.code(), Some(0), "{}", text(&show.stderr));
    let shown: serde_json::Value = serde_json::from_str(&text(&show.stdout)).unwrap();
    for p in json["parts"].as_array().unwrap() {
        let entry = shown["parts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["path"] == p["path"])
            .unwrap();
        assert_eq!(entry["entry"]["at"], p["after"]["at"], "{}", p["path"]);
    }
    // A second regeneration of the written board moves from those places.
    let next = proposal(&dir, &["--seed", &(seed + 1).to_string()]);
    for p in next["parts"].as_array().unwrap() {
        let first = json["parts"].as_array().unwrap().iter().find(|q| q["path"] == p["path"]).unwrap();
        assert_eq!(p["was"]["at"], first["after"]["at"], "{}", p["path"]);
    }
}

#[test]
fn regenerate_refuses_a_module_the_board_lacks_or_holds_whole() {
    let dir = project("layout-regenerate-refused");
    let run = netsmith(&dir, &["regenerate", "stag"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run.stderr));
    assert!(text(&run.stderr).contains("no part in `stag`; its modules include `stage`"), "{}", text(&run.stderr));
    assert!(!dir.join("main.pcb.toml").exists());

    // Every part of the module locked: nothing to place.
    for part in ["stage.input_cap", "stage.output_cap", "stage.regulator"] {
        let set = netsmith(&dir, &["set", part, "--at", "1,1", "--lock"]);
        assert_eq!(set.status.code(), Some(0), "{}", text(&set.stderr));
    }
    let before = fs::read_to_string(dir.join("main.pcb.toml")).unwrap();
    let run = netsmith(&dir, &["regenerate", "stage", "--write"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run.stderr));
    assert!(text(&run.stderr).contains("every part of `stage` is held"), "{}", text(&run.stderr));
    assert_eq!(fs::read_to_string(dir.join("main.pcb.toml")).unwrap(), before);
}
