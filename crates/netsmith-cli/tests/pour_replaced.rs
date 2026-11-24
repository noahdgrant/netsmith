//! `pour_replaced` (issue #1612): a zone of the routes file of the net of the process's pour on
//! its layer replaces the pour (issue #1314), and `board drc` and `fab-notes.txt` say so in a
//! note; a board with no such zone has no note.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

/// A project folder with the four-layer example: ground pours on both outer layers.
fn project(name: &str) -> TempDir {
    let dir = TempDir::new(name);
    for (file, text) in [
        ("main.nsm", include_str!("../../../examples/outline_and_pours.nsm")),
        (
            "main.physical.nsm",
            include_str!("../../../examples/outline_and_pours.physical.nsm"),
        ),
        (
            "main.pcb.toml",
            include_str!("../../../examples/outline_and_pours.pcb.toml"),
        ),
        (
            "main.lock.toml",
            include_str!("../../../examples/outline_and_pours.lock.toml"),
        ),
    ] {
        fs::write(dir.join(file), text).unwrap();
    }
    dir
}

/// A routes file with a ground zone on the top layer, where the process pours ground.
fn add_zone(dir: &Path) {
    fs::write(
        dir.join("main.routes.toml"),
        r#"version = 1

[OutlineAndPours.nets.GND]
pads = "x"

[[OutlineAndPours.nets.GND.zones]]
layers = ["top"]
priority = 1
clearance = 0.2
min_width = 0.25
pads = "solid"
thermal_gap = 0.5
spoke_width = 0.5
islands = "remove"
outline = [[1, 1], [11, 1], [11, 6], [1, 6]]
"#,
    )
    .unwrap();
}

fn netsmith(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap()
}

fn notes(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("note [pour_replaced]"))
        .collect()
}

#[test]
fn a_zone_that_replaces_the_pour_is_a_note_of_board_drc_in_text_and_json() {
    let dir = project("pour-replaced-drc");
    add_zone(&dir);
    let run = netsmith(&dir, &["board", "drc", "main.nsm"]);
    let said = String::from_utf8_lossy(&run.stdout).into_owned();
    let found = notes(&said);
    assert_eq!(found.len(), 1, "{said}");
    for word in [
        "pour of net GND on F.Cu",
        "replaced there by the routes file's zone at 1,1,10,5",
        "the pour is not drawn",
    ] {
        assert!(found[0].contains(word), "{}", found[0]);
    }
    // A note fails nothing: the exit code is that of the board's own findings.
    let run = netsmith(&dir, &["board", "drc", "main.nsm", "--format", "json"]);
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let note: Vec<&serde_json::Value> = doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["rule"] == "pour_replaced")
        .collect();
    assert_eq!(note.len(), 1, "{doc}");
    assert_eq!(note[0]["severity"], "note");
    assert_eq!(note[0]["nets"], serde_json::json!(["GND"]));
    assert_eq!(note[0]["place"]["layer"], "F.Cu");
}

#[test]
fn a_board_with_no_replacing_zone_has_no_note() {
    let dir = project("pour-replaced-none");
    let run = netsmith(&dir, &["board", "drc", "main.nsm"]);
    let said = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(notes(&said).is_empty(), "{said}");
    // A zone of another net cuts the pour and replaces nothing (issue #1314).
    add_zone(&dir);
    let routes = fs::read_to_string(dir.join("main.routes.toml"))
        .unwrap()
        .replace("nets.GND", "nets.3V3");
    fs::write(dir.join("main.routes.toml"), routes).unwrap();
    let run = netsmith(&dir, &["board", "drc", "main.nsm"]);
    let said = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(notes(&said).is_empty(), "{said}");
}

#[test]
fn fab_lists_the_note_in_fab_notes() {
    let dir = project("pour-replaced-fab");
    add_zone(&dir);
    let run = netsmith(
        &dir,
        &["board", "fab", "main.nsm", "--for-review", "--out", "out"],
    );
    let said = String::from_utf8_lossy(&run.stderr).into_owned();
    let notes_file = fs::read_to_string(dir.join("out/OutlineAndPours/fab-notes.txt"))
        .unwrap_or_else(|e| panic!("{e}: {said}"));
    assert_eq!(notes(&notes_file).len(), 1, "{notes_file}");
    assert!(notes_file.contains("design rules notes:"), "{notes_file}");
}
