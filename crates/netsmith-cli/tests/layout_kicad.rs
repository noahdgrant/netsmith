//! `netsmith layout export` and `layout import` (issue #781): the placed board, not routed, as a
//! KiCad board also when the placement has findings, and the parts moved on it in KiCad brought
//! back to the layout file. The board is the regulator example.

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

/// The JSON a `layout` run that must succeed printed.
fn json(dir: &Path, args: &[&str]) -> serde_json::Value {
    let mut all = vec!["layout"];
    all.extend(args);
    all.extend(["--format", "json"]);
    let run = netsmith(dir, &all);
    assert_eq!(run.status.code(), Some(0), "{}", text(&run.stderr));
    serde_json::from_str(&text(&run.stdout)).unwrap_or_else(|e| panic!("{e}: {}", text(&run.stdout)))
}

/// Two locked capacitors whose courtyards overlap: a placement `fab` refuses.
fn overlapping(dir: &Path) {
    for (path, at) in [("stage.output_cap", "10,10"), ("stage.input_cap", "10.3,10")] {
        let set = netsmith(dir, &["layout", "set", path, "--at", at, "--lock"]);
        assert_eq!(set.status.code(), Some(0), "{}", text(&set.stderr));
    }
}

/// `pcb` with the footprint of `path` moved by `dx` mm and put on the back turned `turn` as the
/// file states it, as a person would in KiCad.
fn moved(pcb: &str, path: &str, dx: f64, turn: f64) -> String {
    let mark = format!("\"netsmith_path\" \"{path}\"");
    let end = pcb.find(&mark).unwrap_or_else(|| panic!("no {path}"));
    let start = pcb[..end].rfind("(footprint ").unwrap();
    let at = start + pcb[start..].find("(at ").unwrap();
    let close = at + pcb[at..].find(')').unwrap();
    let numbers: Vec<f64> = pcb[at + 4..close]
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    let head = pcb[start..at].replacen("(layer \"F.Cu\")", "(layer \"B.Cu\")", 1);
    format!(
        "{}{head}(at {} {} {turn}{}",
        &pcb[..start],
        numbers[0] + dx,
        numbers[1],
        &pcb[close..]
    )
}

#[test]
fn a_board_fab_refuses_is_written_for_kicad_with_its_findings_and_marked() {
    let dir = project("layout-export-findings");
    overlapping(&dir);
    let fab = netsmith(&dir, &["fab", "--out", "fab"]);
    assert_ne!(fab.status.code(), Some(0), "fab packaged overlapping parts");

    let exported = json(&dir, &["export", "--out", "kicad/main.kicad_pcb"]);
    let placement = exported["placement"].as_array().unwrap();
    assert!(
        placement.iter().any(|f| f["kind"] == "overlap"),
        "{exported}"
    );
    assert_eq!(exported["parts"], 5);
    let pcb = fs::read_to_string(dir.join("kicad/main.kicad_pcb")).unwrap();
    assert!(pcb.contains("(comment 1 \"NOT FOR FABRICATION"), "{pcb}");
    assert!(pcb.contains("(gr_text \"NOT FOR FABRICATION"), "{pcb}");
    assert!(!pcb.contains("(segment") && !pcb.contains("(via"), "copper on a placed board");
    assert_eq!(pcb.matches("\"netsmith_path\"").count(), 5);
    assert!(dir.join("kicad/main.kicad_pro").is_file());
    let listed = fs::read_to_string(dir.join("kicad/main.findings.txt")).unwrap();
    assert!(listed.contains("overlap"), "{listed}");
}

#[test]
fn the_parts_moved_in_kicad_come_back_as_entries_and_the_board_is_kept_as_it_was() {
    let dir = project("layout-import");
    overlapping(&dir);
    json(&dir, &["export", "--out", "kicad/main.kicad_pcb"]);
    let file = dir.join("kicad/main.kicad_pcb");

    // The board as written: nothing moved, nothing written.
    let same = json(&dir, &["import", "kicad/main.kicad_pcb"]);
    assert_eq!(same["moved"].as_array().unwrap().len(), 0, "{same}");
    assert_eq!((same["unchanged"].clone(), same["written"].clone()), (5.into(), false.into()));

    // Move one capacitor off the other, onto the back, as a person does in KiCad.
    let pcb = fs::read_to_string(&file).unwrap();
    fs::write(&file, moved(&pcb, "stage.input_cap", 3.0, 270.0)).unwrap();
    let layout = fs::read_to_string(dir.join("main.pcb.toml")).unwrap();
    let dry = json(&dir, &["import", "kicad/main.kicad_pcb", "--dry-run"]);
    assert_eq!(dry["written"], false);
    assert_eq!(fs::read_to_string(dir.join("main.pcb.toml")).unwrap(), layout);

    let imported = json(&dir, &["import", "kicad/main.kicad_pcb"]);
    assert_eq!(imported["written"], true);
    let moved = imported["moved"].as_array().unwrap();
    assert_eq!(moved.len(), 1, "{imported}");
    let m = &moved[0];
    assert_eq!(m["path"], "stage.input_cap");
    assert_eq!(m["from"]["at"], serde_json::json!([10.3, 10.0]));
    // On the back the file states the turn plus a half (netsmith_pcb's export).
    assert_eq!(m["to"], serde_json::json!({"at": [13.3, 10.0], "turn": 90.0, "side": "bottom"}));
    // The entry keeps its lock and source.
    assert_eq!((m["existed"].clone(), m["lock"].clone()), (true.into(), true.into()));
    // The parts the placer placed are held where KiCad has them.
    assert_eq!(imported["held"].as_array().unwrap().len(), 3, "{imported}");

    let shown = json(&dir, &["show"]);
    for part in shown["parts"].as_array().unwrap() {
        assert!(part["entry"]["at"].is_array(), "{part}");
    }
    let check = netsmith(&dir, &["layout", "check"]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check.stdout));

    // The same KiCad board again: the board is as it was left there.
    let again = json(&dir, &["import", "kicad/main.kicad_pcb"]);
    assert_eq!(again["moved"].as_array().unwrap().len(), 0, "{again}");
    assert_eq!(again["written"], false);

    // The capacitors no longer overlap.
    let after = json(&dir, &["export", "--out", "kicad/after.kicad_pcb"]);
    assert_eq!(after["placement"].as_array().unwrap().len(), 0, "{after}");
}

#[test]
fn a_footprint_with_no_path_is_said_and_left_out_and_a_file_that_is_no_board_is_refused() {
    let dir = project("layout-import-other");
    json(&dir, &["export", "--out", "main.kicad_pcb"]);
    let file = dir.join("main.kicad_pcb");
    let pcb = fs::read_to_string(&file).unwrap();
    let without = pcb.replacen("\"netsmith_path\"", "\"other\"", 1);
    fs::write(&file, without).unwrap();
    let imported = json(&dir, &["import", "main.kicad_pcb"]);
    let notes = imported["notes"].to_string();
    assert!(notes.contains("no `netsmith_path` property"), "{imported}");
    assert_eq!(imported["missing"].as_array().unwrap().len(), 1, "{imported}");

    fs::write(&file, "(kicad_sch (version 1))").unwrap();
    let refused = netsmith(&dir, &["layout", "import", "main.kicad_pcb"]);
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused.stdout));
    assert!(text(&refused.stderr).contains("not a KiCad board"), "{}", text(&refused.stderr));
    assert!(!dir.join("main.pcb.toml").exists());
}

#[test]
fn kicad_cli_opens_the_exported_board_with_findings_and_sees_its_parts_where_it_was_placed() {
    // The cross-check of issue #1001: the board `fab` refuses is written, and KiCad's own
    // command line reads it, with the footprints where netsmith put them. Skips without KiCad.
    use netsmith_fab::KicadCli as _;
    let Some(cli) = netsmith_fab::KicadCliExe::locate_runnable() else {
        return;
    };
    let dir = project("layout-export-kicad-cli");
    overlapping(&dir);
    let exported = json(&dir, &["export", "--out", "kicad/main.kicad_pcb"]);
    assert!(
        exported["placement"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["kind"] == "overlap"),
        "{exported}"
    );
    let file = dir.join("kicad/main.kicad_pcb");
    let rows = cli
        .pos(&file)
        .unwrap_or_else(|e| panic!("kicad-cli did not read the exported board: {e}"));
    let pcb = fs::read_to_string(&file).unwrap();
    let placed = netsmith_fabricate::layout_kicad::footprints(&pcb).unwrap();
    assert_eq!(placed.len(), 5);
    // `Ref,Val,Package,PosX,PosY,Rot,Side`: one row for each footprint, at its place (KiCad's
    // position file counts y upwards).
    let mut seen = 0;
    for row in rows.lines().skip(1).filter(|l| !l.trim().is_empty()) {
        let cells: Vec<&str> = row.split(',').map(|c| c.trim_matches('"')).collect();
        let part = placed
            .iter()
            .find(|f| f.reference == cells[0])
            .unwrap_or_else(|| panic!("KiCad lists {} that the board does not have", cells[0]));
        let (x, y): (f64, f64) = (cells[3].parse().unwrap(), cells[4].parse().unwrap());
        assert!(
            (x - part.at[0]).abs() < 0.01 && (y.abs() - part.at[1].abs()).abs() < 0.01,
            "{}: KiCad has ({x}, {y}), the file ({}, {})",
            cells[0],
            part.at[0],
            part.at[1]
        );
        seen += 1;
    }
    assert_eq!(seen, 5, "{rows}");
}
