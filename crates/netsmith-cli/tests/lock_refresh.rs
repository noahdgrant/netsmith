//! `netsmith lock --update` and `lock --check` (ADR 0100): the pick lock is refreshed on purpose,
//! with a diff of what changed, and `--check` says whether it is current without writing.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

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

/// An invented regulator whose rows are in the project's own catalog file, picked by what the
/// board needs and not by name: the first row that fits is the pick, so a catalog edit can move it.
const ZZ9: &str = r#"import std::sources::Rail

part Zz9 {
    .vin:  Rail
    .vout: Rail
    .vin.neg ~ .vout.neg

    .dropout:     Voltage
    .max_current: Current
    .vin.voltage >= .vout.voltage + .dropout
    .vin.current == -.vout.current
    -.vout.current <= .max_current

    pins "SOT-23-5" {
        .vin.pos       power       "IN"     "1"
        .vin.neg       power       "GND"    "2"
        .en            input       "EN"     "3"
        .nc            no_connect  "NC"     "4"
        .vout.pos      power       "OUT"    "5"
    }

    .package: Package
    catalog zz9 { .vout.voltage, .dropout, .max_current, .package }
}
"#;

const ZZ9_BOARD: &str = r#"import std::sources::Rail
import src::ics::Zz9

board Zz9Board {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .u: Zz9 { .vout.voltage &= 3.0V..3.6V }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.en ~ .u.vin.pos
}
"#;

const ZZ9_HEADER: &str =
    "lcsc,mpn,vout.voltage,dropout,max_current,package,library,price,stock,date,source\n";
/// Invented rows: not real parts.
const ROW_S: &str = "C9001,ZZ9-33-S,3.3V +/- 2%,0.3V,0.5A,SOT-23-5,Basic,0.10,1000,2026-10-31,https://example.invalid/zz9-33-s.pdf\n";
const ROW_A: &str = "C9002,ZZ9-33-A,3.3V +/- 1%,0.2V,0.5A,SOT-23-5,Basic,0.10,1000,2026-10-31,https://example.invalid/zz9-33-a.pdf\n";
/// The same part as `ROW_S`, which cannot supply the board's 50 mA.
const ROW_S_WEAK: &str = "C9001,ZZ9-33-S,3.3V +/- 2%,0.3V,1mA,SOT-23-5,Basic,0.10,1000,2026-10-31,https://example.invalid/zz9-33-s.pdf\n";

fn led_project(label: &str, source: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
    root
}

fn zz9_project(label: &str, rows: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"zz\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/ics.nsm"), ZZ9).unwrap();
    fs::write(root.join("src/main.nsm"), ZZ9_BOARD).unwrap();
    set_rows(&root, rows);
    root
}

fn set_rows(root: &Path, rows: &str) {
    fs::write(root.join("catalogs/zz9.csv"), format!("{ZZ9_HEADER}{rows}")).unwrap();
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

fn lock_file(root: &Path) -> std::path::PathBuf {
    root.join("src/main.lock.toml")
}

fn read_lock(root: &Path) -> String {
    fs::read_to_string(lock_file(root)).expect("the lock file exists")
}

/// The diff lines of a run's stdout: the indented ones.
fn diff_lines(run: &std::process::Output) -> Vec<String> {
    stdout(run)
        .lines()
        .filter_map(|l| l.strip_prefix("  "))
        .map(str::to_string)
        .collect()
}

#[test]
fn update_with_no_lock_writes_the_lock_build_would_write_and_says_what_it_holds() {
    let root = led_project("lock-update-new", LED);
    let run = netsmith(&root, &["lock", "--update"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(stderr(&run).is_empty(), "{}", stderr(&run));
    let out = stdout(&run);
    assert!(
        out.starts_with("wrote ") && out.lines().next().unwrap().ends_with("src/main.lock.toml"),
        "{out}"
    );
    let lines = diff_lines(&run);
    assert!(
        lines.contains(
            &"LedExample: `r1`: added as `330Ω 1% 0402 C25104` in the `resistor` catalog (lcsc C25104, mpn 0402WGF3300TCE)"
                .to_string()
        ),
        "{lines:?}"
    );
    // The same bytes as the lock of a build, which writes it when it is absent.
    let other = led_project("lock-update-new-build", LED);
    assert!(netsmith(&other, &["build"]).status.success());
    assert_eq!(read_lock(&root), read_lock(&other));
}

#[test]
fn a_second_update_prints_nothing_and_leaves_the_bytes() {
    let root = led_project("lock-update-twice", LED);
    assert!(netsmith(&root, &["lock", "--update"]).status.success());
    let first = read_lock(&root);
    let modified = fs::metadata(lock_file(&root)).unwrap().modified().unwrap();
    for _ in 0..2 {
        let run = netsmith(&root, &["lock", "--update"]);
        assert!(run.status.success(), "{}", stderr(&run));
        assert!(
            stdout(&run).is_empty() && stderr(&run).is_empty(),
            "{}{}",
            stdout(&run),
            stderr(&run)
        );
        assert_eq!(read_lock(&root), first);
    }
    // A lock that is current is not even written again.
    assert_eq!(
        fs::metadata(lock_file(&root)).unwrap().modified().unwrap(),
        modified
    );
}

#[test]
fn check_with_no_lock_fails_and_says_so_and_writes_nothing() {
    let root = led_project("lock-check-none", LED);
    let run = netsmith(&root, &["lock", "--check"]);
    assert_eq!(run.status.code(), Some(1));
    assert!(stdout(&run).is_empty(), "{}", stdout(&run));
    assert!(
        stderr(&run).contains("src/main.lock.toml does not exist: run `netsmith lock --update`"),
        "{}",
        stderr(&run)
    );
    assert!(!lock_file(&root).exists());
}

#[test]
fn check_passes_on_a_current_lock_in_silence() {
    let root = led_project("lock-check-current", LED);
    assert!(netsmith(&root, &["build"]).status.success());
    let run = netsmith(&root, &["lock", "--check"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(
        stdout(&run).is_empty() && stderr(&run).is_empty(),
        "{}{}",
        stdout(&run),
        stderr(&run)
    );
}

#[test]
fn a_catalog_edit_that_moves_a_pick_is_a_stale_lock_and_update_changes_the_pick() {
    let root = zz9_project("lock-update-catalog", ROW_S);
    assert!(netsmith(&root, &["lock", "--update"]).status.success());
    assert!(
        read_lock(&root).contains("row = \"ZZ9-33-S\"")
            && read_lock(&root).contains("lcsc = \"C9001\""),
        "{}",
        read_lock(&root)
    );
    assert!(netsmith(&root, &["lock", "--check"]).status.success());

    // A row the catalog gains before the locked one is what the solver picks now. The locked row
    // still passes, so nothing else says the lock is old: `--check` does.
    set_rows(&root, &format!("{ROW_A}{ROW_S}"));
    let before = read_lock(&root);
    let check = netsmith(&root, &["lock", "--check"]);
    assert_eq!(check.status.code(), Some(1));
    let moved = "Zz9Board: `u`: `ZZ9-33-S` (lcsc C9001, mpn ZZ9-33-S) -> `ZZ9-33-A` (lcsc C9002, mpn ZZ9-33-A)";
    assert_eq!(diff_lines(&check), [moved], "{}", stdout(&check));
    assert!(
        stdout(&check).starts_with(&format!("{} is not current\n", lock_file(&root).display()))
            && stderr(&check).contains("is not current: run `netsmith lock --update`"),
        "{}{}",
        stdout(&check),
        stderr(&check)
    );
    // The locked row passes, so the solver has nothing to report about it.
    assert!(!stderr(&check).contains("note:"), "{}", stderr(&check));
    assert_eq!(read_lock(&root), before, "--check writes nothing");

    let update = netsmith(&root, &["lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert!(
        stdout(&update).starts_with("updated ") && diff_lines(&update) == [moved],
        "{}",
        stdout(&update)
    );
    let after = read_lock(&root);
    assert!(
        after.contains("row = \"ZZ9-33-A\"") && !after.contains("ZZ9-33-S"),
        "{after}"
    );
    assert!(netsmith(&root, &["lock", "--check"]).status.success());
}

#[test]
fn a_row_that_no_longer_passes_or_is_gone_is_reported_in_the_solvers_words_and_replaced() {
    for (label, rows) in [
        ("fails", format!("{ROW_S_WEAK}{ROW_A}")),
        ("gone", ROW_A.to_string()),
    ] {
        let root = zz9_project(&format!("lock-update-{label}"), ROW_S);
        assert!(netsmith(&root, &["lock", "--update"]).status.success());
        set_rows(&root, &rows);

        let check = netsmith(&root, &["lock", "--check"]);
        assert_eq!(check.status.code(), Some(1), "{label}");
        let said = stderr(&check);
        assert!(
            said.contains("note: the rows ")
                && said.contains("holds that no longer pass, in the solver's words")
                && said.contains("`u` is locked to the row `ZZ9-33-S` of the `zz9` catalog by"),
            "{label}: {said}"
        );
        if label == "fails" {
            // The relation that fails and its numbers, as `build` says it.
            assert!(
                said.contains("max_current") && said.contains("1mA"),
                "{said}"
            );
        } else {
            assert!(
                said.contains("the `zz9` catalog has no row `ZZ9-33-S` now"),
                "{said}"
            );
        }
        assert!(
            diff_lines(&check)
                == [
                    "Zz9Board: `u`: `ZZ9-33-S` (lcsc C9001, mpn ZZ9-33-S) -> `ZZ9-33-A` (lcsc C9002, mpn ZZ9-33-A)"
                ],
            "{label}: {}",
            stdout(&check)
        );

        let update = netsmith(&root, &["lock", "--update"]);
        assert!(update.status.success(), "{label}: {}", stderr(&update));
        assert!(
            stderr(&update).contains("`u` is locked to the row `ZZ9-33-S`")
                && stderr(&update).contains("(replaced with the rows above)"),
            "{label}: {}",
            stderr(&update)
        );
        assert!(read_lock(&root).contains("row = \"ZZ9-33-A\""));
        assert!(netsmith(&root, &["lock", "--check"]).status.success());
    }
}

#[test]
fn a_part_new_to_the_design_is_added_and_the_rows_of_the_others_stay() {
    let root = led_project("lock-update-added", LED);
    assert!(netsmith(&root, &["lock", "--update"]).status.success());
    let before = read_lock(&root);
    fs::write(root.join("src/main.nsm"), with_second_led()).unwrap();

    let check = netsmith(&root, &["lock", "--check"]);
    assert_eq!(check.status.code(), Some(1));
    let found = diff_lines(&check);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found[0].starts_with("LedExample: `led2`: added as ")
            && found[1].starts_with("LedExample: `r2`: added as "),
        "{found:?}"
    );
    assert_eq!(read_lock(&root), before);

    let update = netsmith(&root, &["lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert_eq!(diff_lines(&update), found);
    let after = read_lock(&root);
    assert!(
        after.contains("[LedExample.parts.\"r2\"]")
            && after.contains("[LedExample.parts.\"led2\"]"),
        "{after}"
    );
    // What was locked is as it was: the new lock is the old one with two entries more.
    let kept = |text: &str| -> Vec<String> {
        text.split("\n\n")
            .filter(|t| t.contains("\"r1\"") || t.contains("\"led\"]"))
            .map(|t| t.trim_end().to_string())
            .collect()
    };
    assert_eq!(kept(&after), kept(&before));
    assert!(netsmith(&root, &["lock", "--check"]).status.success());
}

#[test]
fn a_part_the_design_no_longer_has_is_dropped_with_a_line() {
    let root = led_project("lock-update-dropped", &with_second_led());
    assert!(netsmith(&root, &["lock", "--update"]).status.success());
    assert!(read_lock(&root).contains("\"r2\""));
    fs::write(root.join("src/main.nsm"), LED).unwrap();

    let check = netsmith(&root, &["lock", "--check"]);
    assert_eq!(check.status.code(), Some(1));
    let found = diff_lines(&check);
    assert_eq!(found.len(), 2, "{found:?}");
    for (line, path) in found.iter().zip(["led2", "r2"]) {
        assert!(
            line.starts_with(&format!("LedExample: `{path}`: dropped (it was `"))
                && line.ends_with("and is no longer a part of the board)"),
            "{line}"
        );
    }

    let update = netsmith(&root, &["lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert_eq!(diff_lines(&update), found);
    assert!(!read_lock(&root).contains("r2"), "{}", read_lock(&root));
    assert!(netsmith(&root, &["lock", "--check"]).status.success());
    // It is the lock a fresh build of the smaller board writes.
    let fresh = led_project("lock-update-dropped-fresh", LED);
    assert!(netsmith(&fresh, &["build"]).status.success());
    assert_eq!(read_lock(&root), read_lock(&fresh));
}

#[test]
fn the_diff_and_the_file_are_the_same_text_in_every_run() {
    let run_in = |label: &str| {
        let root = zz9_project(label, ROW_S);
        assert!(netsmith(&root, &["lock", "--update"]).status.success());
        set_rows(&root, &format!("{ROW_A}{ROW_S}"));
        let check = netsmith(&root, &["lock", "--check"]);
        let update = netsmith(&root, &["lock", "--update"]);
        // The folder's name is in the file's name; the rest is the text to compare.
        let name = |text: String| text.replace(&root.display().to_string(), "<project>");
        (
            name(stdout(&check)),
            name(stdout(&update)),
            read_lock(&root),
        )
    };
    let (check_a, update_a, file_a) = run_in("lock-determinism-a");
    let (check_b, update_b, file_b) = run_in("lock-determinism-b");
    assert_eq!(check_a, check_b);
    assert_eq!(update_a, update_b);
    assert_eq!(file_a, file_b);
    assert!(check_a.contains("-> `ZZ9-33-A`"), "{check_a}");
}

#[test]
fn a_named_file_is_the_only_one_refreshed() {
    let root = zz9_project("lock-update-file", ROW_S);
    // A second top-level file with a board of its own.
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"zz\"\ntop_level = [\"src/main.nsm\", \"src/other.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/other.nsm"), LED).unwrap();
    let run = netsmith(&root, &["lock", "--update", "src/other.nsm"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(root.join("src/other.lock.toml").is_file());
    assert!(!lock_file(&root).exists());
    // With no file, every top-level file of the project.
    let run = netsmith(&root, &["lock", "--update"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(lock_file(&root).is_file());
    assert!(netsmith(&root, &["lock", "--check"]).status.success());
}

#[test]
fn a_lock_that_is_not_a_lock_or_a_design_that_does_not_build_is_exit_two() {
    let root = led_project("lock-update-wrong", LED);
    fs::write(lock_file(&root), "version = 7\n").unwrap();
    for mode in ["--update", "--check"] {
        let run = netsmith(&root, &["lock", mode]);
        assert_eq!(run.status.code(), Some(2), "{mode}");
        assert!(
            stderr(&run).contains("main.lock.toml: version 7 is not a version of the lock"),
            "{}",
            stderr(&run)
        );
    }
    assert_eq!(read_lock(&root), "version = 7\n");

    fs::remove_file(lock_file(&root)).unwrap();
    fs::write(root.join("src/main.nsm"), "board B { .x: Nothing }\n").unwrap();
    let run = netsmith(&root, &["lock", "--update"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(stderr(&run).contains("error"), "{}", stderr(&run));
    assert!(!lock_file(&root).exists());
}

#[test]
fn lock_needs_exactly_one_of_update_and_check() {
    let root = led_project("lock-flags", LED);
    for (args, said) in [
        (vec!["lock"], "<--update|--check>"),
        (vec!["lock", "--update", "--check"], "cannot be used with"),
        (vec!["lock", "--locked"], "unexpected argument '--locked'"),
        (vec!["build", "--update"], "unexpected argument '--update'"),
    ] {
        let run = netsmith(&root, &args);
        assert_eq!(run.status.code(), Some(2), "{args:?}");
        assert!(stderr(&run).contains(said), "{args:?}: {}", stderr(&run));
    }
    // Both flags may be said twice, and `--help` lists the command.
    assert!(
        netsmith(&root, &["lock", "--check", "--check"])
            .status
            .code()
            == Some(1)
    );
    let help = stdout(&netsmith(&root, &["lock", "--help"]));
    assert!(
        help.contains("netsmith lock <--update|--check> [FILE.NSM]")
            && help.contains("lock --update"),
        "{help}"
    );
    assert!(!lock_file(&root).exists());
}
