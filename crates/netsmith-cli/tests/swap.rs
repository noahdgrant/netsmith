//! `netsmith board swap` (issue #1094) is the editor's swap (issue #800) on the command line:
//! the alternatives it lists are the editor's for the same part, and applying one writes the
//! source text the editor's `POST /api/swap` writes. Every part, number and file here is
//! invented.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_editor::Backend;
use netsmith_editor::api::{Swap, SwapApply};
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

/// A resistor in series with an LED on 5 V.
const LED: &str = include_str!("../../../examples/led.nsm");

/// A resistor only this project has: the board's 249 ohm has no row, so the part is unresolved,
/// and the nearest rows of the standard catalog are its alternatives.
const OWN_CATALOG: &str = "lcsc,mpn,resistance,max_power,package,library,source\n\
     C9999999,ZZR-316,316ohm +/- 1%,250mW,0402,Extended,https://example.invalid/zzr\n";

fn project(label: &str, resistor: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"own\"\nseries = \"E96\"\n",
    )
    .unwrap();
    netsmith_solve::migrate::write_to(&root, "catalogs/resistor.csv", OWN_CATALOG).unwrap();
    fs::write(
        root.join("src/main.nsm"),
        LED.replace(".r1: Resistor {}", resistor),
    )
    .unwrap();
    root
}

fn swap(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["board", "swap"])
        .args(args)
        .current_dir(cwd)
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap()
}

fn said(run: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

/// The editor's backend on the project, its one board and the designator of `r1`.
fn editor(root: &Path) -> (Workspace, String, String) {
    let editor = Workspace::open(Target::resolve_from(root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let board = analysis.boards[0].name.clone();
    let refdes = editor
        .board(&board)
        .unwrap()
        .parts
        .iter()
        .find(|p| p.path == "r1")
        .unwrap()
        .refdes
        .clone();
    (editor, board, refdes)
}

const STATED: &str = ".r1: Resistor { .resistance &= 249ohm +/- 1% }";

#[test]
fn the_alternatives_are_the_editors_for_the_same_part() {
    let root = project("cli-swap-list", STATED);
    let (editor, board, refdes) = editor(&root);
    let expected = editor.swap(&board, &refdes).unwrap();
    assert!(expected.alternatives.len() > 1, "{expected:?}");
    // By path and by designator, the JSON is the editor's answer.
    for part in ["r1", refdes.as_str()] {
        let run = swap(&root, &[part, "--format", "json"]);
        assert!(run.status.success(), "{}", said(&run));
        let got: Swap = serde_json::from_slice(&run.stdout).unwrap();
        assert_eq!(got, expected);
    }
    // The text lists each alternative's line, check and edit.
    let run = swap(&root, &["r1"]);
    assert!(run.status.success(), "{}", said(&run));
    let text = String::from_utf8_lossy(&run.stdout);
    for (i, alt) in expected.alternatives.iter().enumerate() {
        assert!(text.contains(&format!("  {}. {}: ", i + 1, alt.line)), "{text}");
    }
    assert!(text.contains("passes every check"), "{text}");
    assert!(
        text.contains("writes src/main.nsm:") && text.contains("`249ohm +/- 1%` -> `240ohm +/- 1%`"),
        "{text}"
    );
    // Nothing was written.
    assert_eq!(
        fs::read_to_string(root.join("src/main.nsm")).unwrap(),
        LED.replace(".r1: Resistor {}", STATED)
    );
}

#[test]
fn applying_one_writes_what_the_editors_post_writes() {
    let by_editor = project("cli-swap-editor", STATED);
    let by_cli = project("cli-swap-cli", STATED);
    let (editor, board, refdes) = editor(&by_editor);
    let listed = editor.swap(&board, &refdes).unwrap();
    // Not the first, so that the choice by LCSC number is seen to choose.
    let alternative = &listed.alternatives[1];
    assert!(alternative.edit.is_some(), "{listed:?}");
    let posted = editor
        .swap_apply(&SwapApply {
            board,
            refdes,
            alternative: alternative.name.clone(),
        })
        .unwrap();
    let lcsc = alternative.lcsc.as_deref().unwrap();
    let run = swap(&by_cli, &["r1", "--apply", lcsc, "--format", "json"]);
    assert!(run.status.success(), "{}", said(&run));
    let answer: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(answer["file"], posted.file.as_str(), "{answer}");
    assert_eq!(answer["lock"], serde_json::Value::Null, "{answer}");
    assert_eq!(answer["alternative"]["name"], alternative.name.as_str());
    let written = fs::read_to_string(by_cli.join("src/main.nsm")).unwrap();
    assert_eq!(
        written,
        fs::read_to_string(by_editor.join("src/main.nsm")).unwrap()
    );
    assert_ne!(written, LED.replace(".r1: Resistor {}", STATED));
}

#[test]
fn a_part_with_nothing_to_swap_or_a_name_that_matches_nothing_fails() {
    let root = project(
        "cli-swap-none",
        ".r1: Resistor { .resistance &= 316ohm +/- 1% }",
    );
    // A row stands for it: what the command checked, exit 1.
    let run = swap(&root, &["r1"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    assert!(said(&run).contains("nothing to swap"), "{}", said(&run));
    // No such part: a mistake, exit 2.
    let run = swap(&root, &["r99"]);
    assert_eq!(run.status.code(), Some(2), "{}", said(&run));
    let root = project("cli-swap-unknown", STATED);
    let run = swap(&root, &["r1", "--apply", "C0"]);
    assert_eq!(run.status.code(), Some(2), "{}", said(&run));
    assert!(said(&run).contains("is not an alternative"), "{}", said(&run));
    assert_eq!(
        fs::read_to_string(root.join("src/main.nsm")).unwrap(),
        LED.replace(".r1: Resistor {}", STATED)
    );
}
