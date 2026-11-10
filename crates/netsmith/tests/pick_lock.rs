//! The pick lock (ADR 0100): `build` and `fab` write `<stem>.lock.toml` beside the source when it
//! does not exist, the solver then holds each locked part to its row, and a lock that no longer
//! matches is told, or with `--locked` refused before anything is written.

use std::{fs, path::Path, process::Command};

use netsmith_kicad::testlib::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

/// A second LED with its own resistor: a part the first lock does not have.
const SECOND_LED: &str = "
    .led2: Led {
        .current &= 5mA..15mA
    }
    .r2: Resistor {}
    .led2.current == (.supply.voltage - .led2.forward_voltage) / .r2.resistance
    .supply.pos ~> .r2 ~> .led2 ~> .supply.neg
";

/// The row the solver picks for `r1` (the first that fits) and one it does not pick that fits
/// as well: a 0603 resistor of 390 ohm and the power rating of its package.
const SOLVER_ROW: &str = "330Ω 1% 0402 C25104";
const OTHER_ROW: &str = "390Ω 1% 0603 C23151";
/// A row that cannot hold the LED's current: 2.2 kohm in 0402 overheats.
const FAILING_ROW: &str = "2.2kΩ 1% 0402 C25879";

fn project(label: &str, source: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
    root
}

fn with_second_led() -> String {
    let at = LED.rfind('}').unwrap();
    format!("{}{SECOND_LED}}}\n", &LED[..at])
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

fn lock_file(root: &Path) -> std::path::PathBuf {
    root.join("src/main.lock.toml")
}

fn schematic(root: &Path) -> String {
    fs::read_to_string(root.join("out/LedExample/LedExample.kicad_sch")).unwrap()
}

#[test]
fn build_writes_the_lock_beside_the_source_and_two_runs_write_the_same_bytes() {
    let root = project("pick-lock-write", LED);
    let run = netsmith(&root, &["build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let first =
        fs::read_to_string(lock_file(&root)).expect("the lock is written beside the source");
    for want in [
        "version = 1",
        "[LedExample.parts.\"r1\"]",
        "catalog = \"resistor\"",
        &format!("row = \"{SOLVER_ROW}\""),
        "lcsc = \"C25104\"",
        "mpn = \"0402WGF3300TCE\"",
        "[LedExample.parts.\"led\"]",
    ] {
        assert!(first.contains(want), "{want}: {first}");
    }
    assert!(
        stdout(&run).contains("wrote ") && stdout(&run).contains("src/main.lock.toml\n"),
        "{}",
        stdout(&run)
    );
    // Pick again from nothing: the same bytes.
    fs::remove_file(lock_file(&root)).unwrap();
    let again = netsmith(&root, &["build"]);
    assert!(again.status.success(), "{}", stderr(&again));
    assert_eq!(fs::read_to_string(lock_file(&root)).unwrap(), first);
}

#[test]
fn a_second_build_reads_the_lock_leaves_it_alone_and_makes_the_same_schematic() {
    let root = project("pick-lock-again", LED);
    assert!(netsmith(&root, &["build"]).status.success());
    let lock = fs::read_to_string(lock_file(&root)).unwrap();
    let schematic_before = schematic(&root);
    let run = netsmith(&root, &["build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(!stderr(&run).contains("note:"), "{}", stderr(&run));
    assert!(!stdout(&run).contains("lock.toml"), "{}", stdout(&run));
    assert_eq!(fs::read_to_string(lock_file(&root)).unwrap(), lock);
    assert_eq!(schematic(&root), schematic_before);
    // And `--locked` accepts a lock that matches.
    let locked = netsmith(&root, &["build", "--locked"]);
    assert!(locked.status.success(), "{}", stderr(&locked));
}

#[test]
fn a_locked_row_that_still_passes_is_the_pick_though_the_solver_would_pick_another() {
    let root = project("pick-lock-passes", LED);
    assert!(netsmith(&root, &["build"]).status.success());
    let without = stdout(&netsmith(&root, &["build"]));
    assert!(
        without.contains("R_0402") && !without.contains("R_0603"),
        "{without}"
    );

    let lock = fs::read_to_string(lock_file(&root)).unwrap();
    let held = lock
        .replace(
            &format!("row = \"{SOLVER_ROW}\""),
            &format!("row = \"{OTHER_ROW}\""),
        )
        .replace("lcsc = \"C25104\"\nmpn = \"0402WGF3300TCE\"\n", "");
    fs::write(lock_file(&root), &held).unwrap();
    let run = netsmith(&root, &["build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(stdout(&run).contains("R_0603"), "{}", stdout(&run));
    // The row the lock holds is a row of the same part and the lock is as the person left it,
    // though its numbers were left out.
    assert_eq!(fs::read_to_string(lock_file(&root)).unwrap(), held);
    assert!(!stderr(&run).contains("note:"), "{}", stderr(&run));
}

#[test]
fn a_locked_row_that_fails_is_reported_with_its_numbers_and_no_other_row_is_tried() {
    let root = project("pick-lock-fails", LED);
    assert!(netsmith(&root, &["build"]).status.success());
    let lock = fs::read_to_string(lock_file(&root)).unwrap();
    fs::write(
        lock_file(&root),
        lock.replace(SOLVER_ROW, FAILING_ROW)
            .replace("C25104", "C25879")
            .replace("0402WGF3300TCE", "0402WGF2201TCE"),
    )
    .unwrap();
    fs::remove_dir_all(root.join("out")).unwrap();
    let run = netsmith(&root, &["build"]);
    assert!(!run.status.success());
    let said = stderr(&run);
    // The failure of the relation, with its numbers, as any failure is told.
    assert!(
        said.contains(&format!("r1 = {FAILING_ROW}")) && said.contains("max_power"),
        "{said}"
    );
    // And whose it is: the lock's row was the only one tried.
    assert!(
        said.contains(&format!(
            "`r1` is locked to the row `{FAILING_ROW}` of the `resistor` catalog by"
        )) && said.contains("no other row was tried for it"),
        "{said}"
    );
    assert!(
        !root.join("out").exists(),
        "nothing is written when the picks fail"
    );
}

#[test]
fn a_row_the_catalog_no_longer_has_or_whose_number_changed_is_named() {
    let root = project("pick-lock-gone", LED);
    assert!(netsmith(&root, &["build"]).status.success());
    let lock = fs::read_to_string(lock_file(&root)).unwrap();

    fs::write(lock_file(&root), lock.replace(SOLVER_ROW, "renamed row")).unwrap();
    let run = netsmith(&root, &["build"]);
    assert!(!run.status.success());
    assert!(
        stderr(&run)
            .contains("`r1` is locked to the row `renamed row` of the `resistor` catalog by")
            && stderr(&run).contains("the `resistor` catalog has no row `renamed row` now"),
        "{}",
        stderr(&run)
    );

    fs::write(lock_file(&root), lock.replace("\"C25104\"", "\"C1\"")).unwrap();
    let run = netsmith(&root, &["build"]);
    assert!(!run.status.success());
    assert!(
        stderr(&run).contains("now has the LCSC number C25104")
            && stderr(&run).contains("and the lock has C1"),
        "{}",
        stderr(&run)
    );

    fs::write(
        lock_file(&root),
        lock.replace("catalog = \"resistor\"", "catalog = \"led\""),
    )
    .unwrap();
    let run = netsmith(&root, &["build"]);
    assert!(!run.status.success());
    assert!(
        stderr(&run).contains(
            "the part now picks from the `resistor` catalog, and the lock has it in `led`"
        ),
        "{}",
        stderr(&run)
    );
}

#[test]
fn a_new_part_is_told_and_with_locked_it_is_an_error_before_anything_is_written() {
    let root = project("pick-lock-new", LED);
    assert!(netsmith(&root, &["build"]).status.success());
    let lock = fs::read_to_string(lock_file(&root)).unwrap();
    fs::write(root.join("src/main.nsm"), with_second_led()).unwrap();

    // Without the flag the build goes on and says what is not in the lock; the lock stays.
    let run = netsmith(&root, &["build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let said = stderr(&run);
    assert!(
        said.contains("note: the parts picked differ from")
            && said.contains("LedExample: `r2` is not in the lock (it was picked as `")
            && said.contains("LedExample: `led2` is not in the lock"),
        "{said}"
    );
    assert_eq!(fs::read_to_string(lock_file(&root)).unwrap(), lock);

    // With it nothing is written.
    fs::remove_dir_all(root.join("out")).unwrap();
    let run = netsmith(&root, &["build", "--locked"]);
    assert!(!run.status.success());
    assert!(
        stderr(&run).contains("error: the parts picked differ from")
            && stderr(&run).contains("--locked says it must not change")
            && stderr(&run).contains("`r2` is not in the lock"),
        "{}",
        stderr(&run)
    );
    assert!(!root.join("out").exists());
    assert_eq!(fs::read_to_string(lock_file(&root)).unwrap(), lock);
}

#[test]
fn a_part_that_is_gone_is_told() {
    let root = project("pick-lock-removed", &with_second_led());
    assert!(netsmith(&root, &["build"]).status.success());
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    let run = netsmith(&root, &["build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("`r2` is in the lock as `")
            && stderr(&run).contains("is no longer a part of the board"),
        "{}",
        stderr(&run)
    );
}

#[test]
fn locked_with_no_lock_file_is_an_error_and_writes_nothing() {
    let root = project("pick-lock-none", LED);
    let run = netsmith(&root, &["build", "--locked"]);
    assert!(!run.status.success());
    assert!(
        stderr(&run).contains("does not exist, and --locked says it must not be written"),
        "{}",
        stderr(&run)
    );
    assert!(!lock_file(&root).exists());
    assert!(!root.join("out").exists());
}

#[test]
fn a_file_that_is_not_a_lock_is_refused_naming_it() {
    let root = project("pick-lock-wrong", LED);
    fs::write(lock_file(&root), "version = 7\n").unwrap();
    let run = netsmith(&root, &["build"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(
        stderr(&run).contains("main.lock.toml: version 7 is not a version of the lock"),
        "{}",
        stderr(&run)
    );
    assert!(!root.join("out").exists());
}

#[test]
fn fab_writes_and_reads_the_lock_too() {
    let root = project("pick-lock-fab", LED);
    let run = netsmith(&root, &["fab", "--allow-drc-violations"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let lock = fs::read_to_string(lock_file(&root)).expect("fab writes the lock too");
    assert!(lock.contains("[LedExample.parts.\"r1\"]"), "{lock}");
    // The lock of a build is the lock of a fab: the same picks, the same bytes.
    let other = project("pick-lock-fab-build", LED);
    assert!(netsmith(&other, &["build"]).status.success());
    assert_eq!(fs::read_to_string(lock_file(&other)).unwrap(), lock);

    // `--locked` holds, and a lock tampered with is read.
    let run = netsmith(&root, &["fab", "--allow-drc-violations", "--locked"]);
    assert!(run.status.success(), "{}", stderr(&run));
    fs::write(lock_file(&root), lock.replace(SOLVER_ROW, "renamed row")).unwrap();
    let run = netsmith(&root, &["fab", "--allow-drc-violations"]);
    assert!(!run.status.success());
    assert!(
        stderr(&run).contains("`r1` is locked to the row `renamed row`"),
        "{}",
        stderr(&run)
    );
}
