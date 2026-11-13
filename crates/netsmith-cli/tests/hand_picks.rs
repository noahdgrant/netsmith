//! A hand pick end to end (ADR 0121): `.lcsc &= "..."` in the source holds a part to that row,
//! before the pick lock, and the lock records it; a picks file beside the source, which netsmith
//! no longer reads, stops every command that solves with how to move its entries into the source.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

/// The row the solver picks for `r1`, and one it does not pick that fits as well (as in
/// `pick_lock.rs`).
const SOLVER_ROW: &str = "330Ω 1% 0402 C25104";
const OTHER_ROW: &str = "390Ω 1% 0603 C23151";

fn project(label: &str, source: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
    root
}

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_KICAD_SYMBOLS", netsmith_testkit::sample_symbols())
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_testkit::sample_footprints(),
        )
        .output()
        .unwrap()
}

fn stderr(run: &std::process::Output) -> String {
    String::from_utf8_lossy(&run.stderr).to_string()
}

fn stdout(run: &std::process::Output) -> String {
    String::from_utf8_lossy(&run.stdout).to_string()
}

fn lock(root: &Path) -> String {
    fs::read_to_string(root.join("src/main.lock.toml")).unwrap()
}

/// The LED board with `r1` held to the row with the LCSC number `lcsc`.
fn hand_picked(lcsc: &str) -> String {
    LED.replace(
        ".r1: Resistor {}",
        &format!(".r1: Resistor {{ .lcsc &= \"{lcsc}\" }}"),
    )
}

#[test]
fn a_hand_pick_is_the_pick_and_the_lock_records_it() {
    let root = project("hand-pick", &hand_picked("C23151"));
    let run = netsmith(&root, &["build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(stdout(&run).contains("R_0603"), "{}", stdout(&run));
    assert!(
        lock(&root).contains(&format!("row = \"{OTHER_ROW}\"")),
        "{}",
        lock(&root)
    );
    assert!(!stderr(&run).contains("note:"), "{}", stderr(&run));
}

#[test]
fn a_hand_pick_comes_before_the_lock_and_lock_update_records_it() {
    let root = project("hand-pick-over-lock", LED);
    assert!(netsmith(&root, &["build"]).status.success());
    assert!(lock(&root).contains(SOLVER_ROW));
    fs::write(root.join("src/main.nsm"), hand_picked("C23151")).unwrap();
    // The build takes the hand pick and says that the lock differs, which it leaves alone.
    let run = netsmith(&root, &["build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(stdout(&run).contains("R_0603"), "{}", stdout(&run));
    assert!(
        stderr(&run).contains(&format!(
            "`r1` is locked to `{SOLVER_ROW}` and was picked as `{OTHER_ROW}`"
        )),
        "{}",
        stderr(&run)
    );
    assert!(!netsmith(&root, &["build", "--locked"]).status.success());
    // `lock --check` says the lock is not current, and `--update` writes the hand-picked row.
    let check = netsmith(&root, &["lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    let update = netsmith(&root, &["lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert!(lock(&root).contains(OTHER_ROW), "{}", lock(&root));
    assert!(netsmith(&root, &["lock", "--check"]).status.success());
    assert!(netsmith(&root, &["build", "--locked"]).status.success());
}

#[test]
fn a_hand_picked_row_that_fails_stops_the_build_at_the_narrowing() {
    // 2.2 kohm in 0402 overheats at the LED's current.
    let root = project("hand-pick-fails", &hand_picked("C25879"));
    let run = netsmith(&root, &["build"]);
    assert!(!run.status.success());
    let said = stderr(&run);
    assert!(said.contains("max_power"), "{said}");
    assert!(
        said.contains("`r1` is held by `.lcsc &= \"C25879\"` to one row of the `resistor` catalog, and no other row was tried for it")
            && said.contains("main.nsm:22:21"),
        "{said}"
    );
    // `lock --check` cannot tell, and says why in the solver's words.
    let check = netsmith(&root, &["lock", "--check"]);
    assert_eq!(check.status.code(), Some(2), "{}", stderr(&check));
    assert!(stderr(&check).contains("max_power"), "{}", stderr(&check));
}

#[test]
fn a_picks_file_is_refused_with_the_narrowings_to_write() {
    let root = project("picks-file-refused", LED);
    fs::write(
        root.join("src/main.picks.toml"),
        "version = 1\n[LedExample.parts.\"r1\"]\ncatalog = \"resistor\"\nlcsc = \"C23151\"\n",
    )
    .unwrap();
    for args in [&["build"][..], &["lock", "--check"], &["lock", "--update"]] {
        let run = netsmith(&root, args);
        assert_eq!(run.status.code(), Some(2), "{args:?}: {}", stderr(&run));
        assert!(
            stderr(&run).contains("main.picks.toml: netsmith no longer reads a picks file (ADR 0121)")
                && stderr(&run).contains("  in `board LedExample`:\n    .r1.lcsc &= \"C23151\""),
            "{args:?}: {}",
            stderr(&run)
        );
    }
    assert!(!root.join("src/main.lock.toml").exists());
    // Moved into the source and deleted, the entry is the same pick.
    fs::write(root.join("src/main.nsm"), hand_picked("C23151")).unwrap();
    fs::remove_file(root.join("src/main.picks.toml")).unwrap();
    let run = netsmith(&root, &["build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(lock(&root).contains(OTHER_ROW), "{}", lock(&root));
}
