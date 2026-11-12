//! The picks file (#462): `<stem>.picks.toml` beside the source holds the catalog rows a person
//! chose for parts. The solver holds each part it names to that row, before the lock, and judges
//! the row by everything the design says; the lock records the chosen row; `lock --update` keeps
//! it; an entry whose part is gone is stale and is told as a stale lock entry is (ADR 0100).

use std::{fs, path::Path, process::Command};

use netsmith_kicad::testlib::TempDir;

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

fn stdout(run: &std::process::Output) -> String {
    String::from_utf8_lossy(&run.stdout).to_string()
}

fn lock(root: &Path) -> String {
    fs::read_to_string(root.join("src/main.lock.toml")).unwrap()
}

/// Writes the picks file with one entry for each `(part, catalog, lcsc)`.
fn picks(root: &Path, entries: &[(&str, &str, &str)]) {
    let mut text = String::from("version = 1\n");
    for (part, catalog, lcsc) in entries {
        text.push_str(&format!(
            "\n[LedExample.parts.\"{part}\"]\ncatalog = \"{catalog}\"\nlcsc = \"{lcsc}\"\n"
        ));
    }
    fs::write(root.join("src/main.picks.toml"), text).unwrap();
}

#[test]
fn a_chosen_row_is_the_pick_and_the_lock_records_it() {
    let root = project("picks-chosen", LED);
    picks(&root, &[("r1", "resistor", "C23151")]);
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
fn a_chosen_row_comes_before_the_lock_and_lock_update_keeps_it() {
    let root = project("picks-over-lock", LED);
    assert!(netsmith(&root, &["build"]).status.success());
    assert!(lock(&root).contains(SOLVER_ROW));
    picks(&root, &[("r1", "resistor", "C23151")]);
    // The build takes the chosen row and says that the lock differs, which it leaves alone.
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
    // `lock --check` says the lock is not current, and `--update` writes the chosen row.
    let check = netsmith(&root, &["lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    let update = netsmith(&root, &["lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert!(lock(&root).contains(OTHER_ROW), "{}", lock(&root));
    let check = netsmith(&root, &["lock", "--check"]);
    assert!(check.status.success(), "{}", stderr(&check));
    assert!(netsmith(&root, &["build", "--locked"]).status.success());
}

#[test]
fn a_chosen_row_that_fails_is_reported_and_no_other_row_is_tried() {
    let root = project("picks-fails", LED);
    // 2.2 kohm in 0402 overheats at the LED's current.
    picks(&root, &[("r1", "resistor", "C25879")]);
    let run = netsmith(&root, &["build"]);
    assert!(!run.status.success());
    let said = stderr(&run);
    assert!(said.contains("max_power"), "{said}");
    assert!(
        said.contains("`r1` is picked as the row with lcsc C25879 of the `resistor` catalog by")
            && said.contains("main.picks.toml, and no other row was tried for it"),
        "{said}"
    );
    // `lock --check` cannot tell, and says why in the solver's words.
    let check = netsmith(&root, &["lock", "--check"]);
    assert_eq!(check.status.code(), Some(2), "{}", stderr(&check));
    assert!(stderr(&check).contains("max_power"), "{}", stderr(&check));
}

#[test]
fn a_number_the_catalog_does_not_have_or_another_catalog_is_named() {
    let root = project("picks-no-row", LED);
    picks(&root, &[("r1", "resistor", "C0000000")]);
    let run = netsmith(&root, &["build"]);
    assert!(!run.status.success());
    assert!(
        stderr(&run).contains("`r1` is picked as the row with lcsc C0000000 of the `resistor` catalog by")
            && stderr(&run).contains("and the `resistor` catalog has no row with lcsc C0000000: no other row was tried for it"),
        "{}",
        stderr(&run)
    );
    picks(&root, &[("r1", "capacitor", "C25104")]);
    let run = netsmith(&root, &["build"]);
    assert!(!run.status.success());
    assert!(
        stderr(&run).contains("the part picks from the `resistor` catalog, and")
            && stderr(&run).contains("names `capacitor`"),
        "{}",
        stderr(&run)
    );
}

#[test]
fn a_stale_entry_is_a_note_and_with_locked_or_lock_check_an_error() {
    let root = project("picks-stale", LED);
    picks(&root, &[("gone", "resistor", "C25104")]);
    let run = netsmith(&root, &["build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let line = "LedExample: `gone` is in";
    assert!(
        stderr(&run).contains("note: ")
            && stderr(&run).contains(line)
            && stderr(&run).contains("is no longer a part of the board that picks a row"),
        "{}",
        stderr(&run)
    );
    let locked = netsmith(&root, &["build", "--locked"]);
    assert!(!locked.status.success());
    assert!(
        stderr(&locked).contains("--locked says nothing may be stale")
            && stderr(&locked).contains(line),
        "{}",
        stderr(&locked)
    );
    let check = netsmith(&root, &["lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    assert!(stderr(&check).contains(line), "{}", stderr(&check));
    // `--update` says so too, and leaves the picks file, which is a person's, as it was.
    let before = fs::read_to_string(root.join("src/main.picks.toml")).unwrap();
    let update = netsmith(&root, &["lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert!(stderr(&update).contains(line), "{}", stderr(&update));
    assert_eq!(
        fs::read_to_string(root.join("src/main.picks.toml")).unwrap(),
        before
    );
}

#[test]
fn a_file_that_is_not_a_picks_file_is_refused_naming_it() {
    let root = project("picks-wrong", LED);
    fs::write(
        root.join("src/main.picks.toml"),
        "version = 1\n[LedExample.parts.\"r1\"]\ncatalog = \"resistor\"\n",
    )
    .unwrap();
    let run = netsmith(&root, &["build"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("main.picks.toml: [LedExample.parts.\"r1\"] names the row by `lcsc` or by `mpn`: it has neither"),
        "{}",
        stderr(&run)
    );
}
