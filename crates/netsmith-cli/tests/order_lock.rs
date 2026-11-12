//! `fab --order` and the pick lock (ADR 0100, ADR 0035): an order holds the boards of each source
//! file it makes boards from to that file's lock, as `fab` on the file alone does, and `--locked`
//! refuses a change before anything is made. These run the program, with the sample KiCad
//! libraries and no KiCad.

use std::{fs, path::Path, process::Command};

use netsmith_kicad::testlib::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

/// The row the solver picks for `r1` and a row that fits as well that it does not pick.
const SOLVER_ROW: &str = "330Ω 1% 0402 C25104";
const OTHER_ROW: &str = "390Ω 1% 0603 C23151";
const SOLVER_LCSC: &str = "C25104";
const OTHER_LCSC: &str = "C23151";
/// A row that cannot hold the LED's current: 2.2 kohm in 0402 overheats.
const FAILING_ROW: &str = "2.2kΩ 1% 0402 C25879";

const ORDER: &str = "[order.kit.boards]\nLedExample = 4\nLedTwo = 6\n";

/// Two source files, each a board of its own (the second is the first with another name),
/// and an order of both.
fn project(label: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!(
            "[project]\nname = \"kit\"\ntop_level = [\"src/led.nsm\", \"src/led2.nsm\"]\n\n{ORDER}"
        ),
    )
    .unwrap();
    fs::write(root.join("src/led.nsm"), LED).unwrap();
    fs::write(
        root.join("src/led2.nsm"),
        LED.replace("LedExample", "LedTwo"),
    )
    .unwrap();
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

fn order(root: &Path, extra: &[&str]) -> std::process::Output {
    let mut args = vec!["fab", "--order", "kit", "--allow-drc-violations"];
    args.extend_from_slice(extra);
    netsmith(root, &args)
}

fn stderr(run: &std::process::Output) -> String {
    String::from_utf8_lossy(&run.stderr).to_string()
}

fn lock(root: &Path, stem: &str) -> std::path::PathBuf {
    root.join(format!("src/{stem}.lock.toml"))
}

fn bom_of(root: &Path, board: &str) -> String {
    fs::read_to_string(root.join(format!("fab/orders/kit/{board}/bom.csv"))).unwrap()
}

/// Writes the locks of both files (what `build` writes) and holds `r1` of `stem` to `row`.
fn locked_to(root: &Path, stem: &str, row: &str) {
    for file in ["src/led.nsm", "src/led2.nsm"] {
        let run = netsmith(root, &["build", file]);
        assert!(run.status.success(), "{}", stderr(&run));
    }
    let text = fs::read_to_string(lock(root, stem)).unwrap();
    // The numbers are a check, and a row of the same part is the same; leave them out.
    let text = text
        .replace(
            &format!("row = \"{SOLVER_ROW}\""),
            &format!("row = \"{row}\""),
        )
        .replace("lcsc = \"C25104\"\nmpn = \"0402WGF3300TCE\"\n", "");
    fs::write(lock(root, stem), text).unwrap();
}

#[test]
fn an_order_over_two_files_gets_the_rows_each_lock_holds() {
    let root = project("order-lock-rows");
    // Without a lock the solver picks the 0402 row for both boards.
    locked_to(&root, "led", OTHER_ROW);
    locked_to(&root, "led2", OTHER_ROW);
    let run = order(&root, &[]);
    assert!(run.status.success(), "{}", stderr(&run));
    for board in ["LedExample", "LedTwo"] {
        let bom = bom_of(&root, board);
        assert!(
            bom.contains(OTHER_LCSC) && !bom.contains(SOLVER_LCSC),
            "{board}: {bom}"
        );
    }
    // The order says nothing about the locks it kept, and writes no lock.
    assert!(!stderr(&run).contains("note:"), "{}", stderr(&run));
    // `fab` of each file alone gets the rows the order did.
    for file in ["src/led.nsm", "src/led2.nsm"] {
        let alone = netsmith(
            &root,
            &["fab", file, "--allow-drc-violations", "--out", "alone"],
        );
        assert!(alone.status.success(), "{}", stderr(&alone));
    }
    for board in ["LedExample", "LedTwo"] {
        assert_eq!(
            fs::read_to_string(root.join(format!("alone/{board}/bom.csv"))).unwrap(),
            bom_of(&root, board),
            "{board}"
        );
    }
}

#[test]
fn a_board_whose_file_has_no_lock_is_picked_and_locked_as_fab_does() {
    let root = project("order-lock-mixed");
    locked_to(&root, "led", OTHER_ROW);
    fs::remove_file(lock(&root, "led2")).unwrap();
    let run = order(&root, &[]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(bom_of(&root, "LedExample").contains(OTHER_LCSC));
    // The file with no lock is picked freely, and the lock `fab` of that file writes is written.
    let bom = bom_of(&root, "LedTwo");
    assert!(
        bom.contains(SOLVER_LCSC) && !bom.contains(OTHER_LCSC),
        "{bom}"
    );
    let written = fs::read_to_string(lock(&root, "led2")).expect("an absent lock is written");
    let other = project("order-lock-mixed-fab");
    let fab = netsmith(&other, &["fab", "src/led2.nsm", "--allow-drc-violations"]);
    assert!(fab.status.success(), "{}", stderr(&fab));
    assert_eq!(
        written,
        fs::read_to_string(lock(&other, "led2")).unwrap(),
        "the lock of an order is the lock of a fab"
    );
    // And the lock that was there is as it was left.
    assert!(
        fs::read_to_string(lock(&root, "led"))
            .unwrap()
            .contains(OTHER_ROW)
    );
}

#[test]
fn locked_refuses_a_lock_that_does_not_exist_before_anything_is_written() {
    let root = project("order-lock-absent");
    let run = order(&root, &["--locked"]);
    assert!(!run.status.success());
    let said = stderr(&run);
    assert!(
        said.contains("led.lock.toml does not exist")
            && said.contains("led2.lock.toml does not exist")
            && said.contains("--locked says it must not be written"),
        "{said}"
    );
    assert!(!lock(&root, "led").exists() && !lock(&root, "led2").exists());
    assert!(!root.join("fab").exists(), "nothing is made");
}

#[test]
fn locked_refuses_a_stale_lock_and_leaves_every_file_alone() {
    let root = project("order-lock-stale");
    for file in ["src/led.nsm", "src/led2.nsm"] {
        assert!(netsmith(&root, &["build", file]).status.success());
    }
    // The first file's lock is current; the second's has lost the entry of `r1`, as when a
    // part was added to the board after the lock was written.
    let current = fs::read_to_string(lock(&root, "led")).unwrap();
    let text = fs::read_to_string(lock(&root, "led2")).unwrap();
    let stale: String = text
        .split("\n[")
        .enumerate()
        .filter(|(i, part)| *i == 0 || !part.starts_with("LedTwo.parts.\"r1\""))
        .map(|(i, part)| {
            if i == 0 {
                part.to_string()
            } else {
                format!("\n[{part}")
            }
        })
        .collect();
    assert_ne!(stale, text);
    fs::write(lock(&root, "led2"), &stale).unwrap();

    let run = order(&root, &["--locked"]);
    assert!(!run.status.success());
    let said = stderr(&run);
    assert!(
        said.contains("the parts picked differ from")
            && said.contains("led2.lock.toml")
            && said.contains("LedTwo: `r1` is not in the lock")
            && said.contains("--locked says it must not change")
            && !said.contains("led.lock.toml"),
        "{said}"
    );
    assert!(!root.join("fab").exists(), "nothing is made");
    assert_eq!(fs::read_to_string(lock(&root, "led")).unwrap(), current);
    assert_eq!(fs::read_to_string(lock(&root, "led2")).unwrap(), stale);

    // Without `--locked` the same order is made, says so, and does not rewrite the lock.
    let run = order(&root, &[]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("note: the parts picked differ from")
            && stderr(&run).contains("LedTwo: `r1` is not in the lock"),
        "{}",
        stderr(&run)
    );
    assert_eq!(fs::read_to_string(lock(&root, "led2")).unwrap(), stale);
}

#[test]
fn locked_accepts_current_locks_and_two_runs_make_the_same_bytes() {
    let root = project("order-lock-current");
    let first = order(&root, &[]);
    assert!(first.status.success(), "{}", stderr(&first));
    let locks = (
        fs::read_to_string(lock(&root, "led")).unwrap(),
        fs::read_to_string(lock(&root, "led2")).unwrap(),
    );
    let bom = (bom_of(&root, "LedExample"), bom_of(&root, "LedTwo"));
    let summary = fs::read_to_string(root.join("fab/orders/kit/order-bom.csv")).unwrap();
    let again = order(&root, &["--locked"]);
    assert!(again.status.success(), "{}", stderr(&again));
    assert_eq!(
        locks,
        (
            fs::read_to_string(lock(&root, "led")).unwrap(),
            fs::read_to_string(lock(&root, "led2")).unwrap()
        )
    );
    assert_eq!(bom, (bom_of(&root, "LedExample"), bom_of(&root, "LedTwo")));
    assert_eq!(
        summary,
        fs::read_to_string(root.join("fab/orders/kit/order-bom.csv")).unwrap()
    );
}

#[test]
fn a_locked_row_that_fails_stops_the_order_with_the_solvers_message() {
    let root = project("order-lock-fails");
    locked_to(&root, "led2", FAILING_ROW);
    let run = order(&root, &[]);
    assert!(!run.status.success());
    let said = stderr(&run);
    assert!(
        said.contains(&format!("`r1` is locked to the row `{FAILING_ROW}`"))
            && said.contains("no other row was tried"),
        "{said}"
    );
}

#[test]
fn a_file_with_no_board_in_the_order_is_not_read() {
    let root = project("order-lock-unused");
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"kit\"\ntop_level = [\"src/led.nsm\", \"src/led2.nsm\"]\n\n[order.kit.boards]\nLedExample = 4\n",
    )
    .unwrap();
    // The lock of the file the order does not use is not a lock at all.
    fs::write(lock(&root, "led2"), "this is not a lock\n").unwrap();
    let run = order(&root, &["--locked"]);
    // `--locked` still wants the lock of the file it does read.
    assert!(!run.status.success());
    assert!(
        stderr(&run).contains("led.lock.toml does not exist") && !stderr(&run).contains("led2"),
        "{}",
        stderr(&run)
    );
    let run = order(&root, &[]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(lock(&root, "led").exists());
    assert!(root.join("fab/orders/kit/LedExample/bom.csv").is_file());
    assert!(!root.join("fab/orders/kit/LedTwo").exists());
}
