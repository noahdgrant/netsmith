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

/// The width and height a PNG's header gives, after checking its signature.
fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    assert_eq!(&bytes[12..16], b"IHDR");
    let be = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    (be(16), be(20))
}

#[test]
fn route_with_stages_draws_a_png_of_each_stage_and_the_crowding_the_same_each_run() {
    let examples = repo().join("examples");
    let dir = TempDir::new("route-stages");
    let saved = dir.join("saved");
    ok(&netsmith(
        &examples,
        &[
            "fab",
            "led.nsm",
            "--out",
            saved.to_str().unwrap(),
            "--locked",
            "--save-placed",
        ],
    ));
    let placed = saved.join("LedExample/LedExample.placed.json");
    let route = |out: &Path, more: &[&str]| {
        let mut args = vec![
            "route",
            placed.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ];
        args.extend_from_slice(more);
        let run = netsmith(&examples, &args);
        ok(&run);
        String::from_utf8_lossy(&run.stdout).into_owned()
    };
    let (plain, a, b) = (dir.join("plain"), dir.join("a"), dir.join("b"));
    route(&plain, &[]);
    let said = route(&a, &["--stages"]);
    route(&b, &["--stages"]);
    // No picture without the flag.
    assert!(!plain.join("crowding.png").exists(), "{:?}", names(&plain));
    // The pictures change no copper.
    let pcb = |d: &Path| std::fs::read_to_string(d.join("LedExample.kicad_pcb")).unwrap();
    assert_eq!(pcb(&a), pcb(&plain));
    let pictures: Vec<String> = names(&a)
        .into_iter()
        .filter(|n| n.ends_with(".png"))
        .collect();
    assert!(
        pictures.contains(&"stage-1-first-pass.png".to_string())
            && pictures.contains(&"stage-4-final.png".to_string())
            && pictures.contains(&"crowding.png".to_string()),
        "{pictures:?}"
    );
    for name in &pictures {
        assert!(said.contains(name.as_str()), "{name} not in {said}");
        let bytes = std::fs::read(a.join(name)).unwrap();
        let (w, h) = png_size(&bytes);
        assert_eq!(w.max(h), 1600, "{name}: {w} x {h}");
        assert!(w.min(h) > 100, "{name}: {w} x {h}");
        assert_eq!(
            bytes,
            std::fs::read(b.join(name)).unwrap(),
            "{name} differs"
        );
    }
    // A later run without the flag takes the pictures away.
    route(&a, &[]);
    assert!(
        !names(&a).iter().any(|n| n.ends_with(".png")),
        "{:?}",
        names(&a)
    );
}

#[test]
fn route_with_one_try_makes_one_and_takes_a_turn_cost() {
    let examples = repo().join("examples");
    let dir = TempDir::new("route-tries");
    let saved = dir.join("saved");
    ok(&netsmith(
        &examples,
        &[
            "fab",
            "led.nsm",
            "--out",
            saved.to_str().unwrap(),
            "--locked",
            "--save-placed",
        ],
    ));
    let placed = saved.join("LedExample/LedExample.placed.json");
    let out = dir.join("one");
    let run = netsmith(
        &examples,
        &[
            "route",
            placed.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--tries",
            "1",
            "--route-timeout",
            "0",
            "--turn",
            "26",
        ],
    );
    ok(&run);
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(said.contains("(1 tries)"), "{said}");
}
