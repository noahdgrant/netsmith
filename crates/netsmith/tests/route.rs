//! `netsmith fab --save-placed` writes the board as placed before routing, and `netsmith route`
//! routes that file alone to the copper `fab` drew.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use netsmith_kicad::testlib::TempDir;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `netsmith <args>` in `dir`, with KiCad switched off: neither command needs it.
fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn ok(run: &Output) {
    assert!(
        run.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

/// The copper lines of a `.kicad_pcb`: its tracks and vias.
fn copper(pcb: &str) -> Vec<&str> {
    pcb.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("(segment") || l.starts_with("(via"))
        .collect()
}

/// The files of a folder, by name.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn route_of_the_saved_placed_board_draws_the_copper_fab_drew() {
    let examples = repo().join("examples");
    let dir = TempDir::new("route-placed");
    let (saved, plain, routed) = (dir.join("saved"), dir.join("plain"), dir.join("routed"));
    let run = netsmith(
        &examples,
        &[
            "fab",
            "led.nsm",
            "--out",
            saved.to_str().unwrap(),
            "--locked",
            "--save-placed",
        ],
    );
    ok(&run);
    let package = saved.join("LedExample");
    let placed = package.join("LedExample.placed.json");
    assert!(placed.exists(), "{:?}", names(&package));

    // Without the flag the package is the same, less the placed board.
    ok(&netsmith(
        &examples,
        &[
            "fab",
            "led.nsm",
            "--out",
            plain.to_str().unwrap(),
            "--locked",
        ],
    ));
    let mut with = names(&package);
    with.retain(|n| n != "LedExample.placed.json");
    assert_eq!(with, names(&plain.join("LedExample")));

    let run = netsmith(
        &examples,
        &[
            "route",
            placed.to_str().unwrap(),
            "--out",
            routed.to_str().unwrap(),
        ],
    );
    ok(&run);
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(said.contains("0 connections left"), "{said}");
    assert!(said.contains("tracks") && said.contains("vias"), "{said}");
    for file in [
        "LedExample.kicad_pcb",
        "LedExample.kicad_pro",
        "LedExample.svg",
    ] {
        assert!(routed.join(file).exists(), "{file}: {:?}", names(&routed));
    }
    assert!(!routed.join("routing-failure.txt").exists());
    let by_fab = std::fs::read_to_string(package.join("LedExample.kicad_pcb")).unwrap();
    let by_route = std::fs::read_to_string(routed.join("LedExample.kicad_pcb")).unwrap();
    assert!(!copper(&by_fab).is_empty());
    assert_eq!(copper(&by_route), copper(&by_fab));
    assert_eq!(by_route, by_fab, "the whole board file is the same");

    // A run without the flag removes the placed board an earlier one saved.
    ok(&netsmith(
        &examples,
        &[
            "fab",
            "led.nsm",
            "--out",
            saved.to_str().unwrap(),
            "--locked",
        ],
    ));
    assert!(!placed.exists(), "{:?}", names(&package));
}

#[test]
fn route_refuses_a_file_it_cannot_read_with_exit_code_two() {
    let dir = TempDir::new("route-bad-file");
    std::fs::write(dir.join("old.placed.json"), r#"{"format": 0, "board": {}}"#).unwrap();
    let run = netsmith(&dir, &["route", "old.placed.json"]);
    assert_eq!(run.status.code(), Some(2));
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(said.contains("--save-placed"), "{said}");
    let run = netsmith(&dir, &["route", "missing.placed.json"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(!dir.join("out").exists(), "nothing is written");
}
