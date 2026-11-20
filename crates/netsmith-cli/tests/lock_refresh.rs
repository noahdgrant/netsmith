//! `netsmith project lock --update` and `project lock --check` (ADR 0100): the pick lock is refreshed on purpose,
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
const ROW_S: &str = "C9900001,ZZ9-33-S,3.3V +/- 2%,0.3V,0.5A,SOT-23-5,Basic,0.10,1000,2026-10-31,https://example.invalid/zz9-33-s.pdf\n";
const ROW_A: &str = "C9900002,ZZ9-33-A,3.3V +/- 1%,0.2V,0.5A,SOT-23-5,Basic,0.10,1000,2026-10-31,https://example.invalid/zz9-33-a.pdf\n";
/// The same part as `ROW_S`, which cannot supply the board's 50 mA.
const ROW_S_WEAK: &str = "C9900001,ZZ9-33-S,3.3V +/- 2%,0.3V,1mA,SOT-23-5,Basic,0.10,1000,2026-10-31,https://example.invalid/zz9-33-s.pdf\n";

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
    netsmith_solve::migrate::write_to(root, "catalogs/zz9.csv", &format!("{ZZ9_HEADER}{rows}")).unwrap();
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
    let run = netsmith(&root, &["project", "lock", "--update"]);
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
            &"LedExample: `r1`: added as `330Ω 1% 0402 C25104` in the `resistor` catalog (mpn 0402WGF3300TCE, jlcpcb-assembly C25104, lcsc-retail C25104)"
                .to_string()
        ),
        "{lines:?}"
    );
    // The same bytes as the lock of a build, which writes it when it is absent.
    let other = led_project("lock-update-new-build", LED);
    assert!(netsmith(&other, &["board", "build"]).status.success());
    assert_eq!(read_lock(&root), read_lock(&other));
}

#[test]
fn a_second_update_prints_nothing_and_leaves_the_bytes() {
    let root = led_project("lock-update-twice", LED);
    assert!(netsmith(&root, &["project", "lock", "--update"]).status.success());
    let first = read_lock(&root);
    let modified = fs::metadata(lock_file(&root)).unwrap().modified().unwrap();
    for _ in 0..2 {
        let run = netsmith(&root, &["project", "lock", "--update"]);
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
    let run = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(run.status.code(), Some(1));
    assert!(stdout(&run).is_empty(), "{}", stdout(&run));
    assert!(
        stderr(&run).contains("src/main.lock.toml does not exist: run `netsmith project lock --update`"),
        "{}",
        stderr(&run)
    );
    assert!(!lock_file(&root).exists());
}

#[test]
fn check_passes_on_a_current_lock_in_silence() {
    let root = led_project("lock-check-current", LED);
    assert!(netsmith(&root, &["board", "build"]).status.success());
    let run = netsmith(&root, &["project", "lock", "--check"]);
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
    assert!(netsmith(&root, &["project", "lock", "--update"]).status.success());
    assert!(
        read_lock(&root).contains("row = \"ZZ9-33-S\"")
            && read_lock(&root).contains("sku = { jlcpcb-assembly = \"C9900001\""),
        "{}",
        read_lock(&root)
    );
    assert!(netsmith(&root, &["project", "lock", "--check"]).status.success());

    // A row the catalog gains before the locked one is what the solver picks now. The locked row
    // still passes, so nothing else says the lock is old: `--check` does.
    set_rows(&root, &format!("{ROW_A}{ROW_S}"));
    let before = read_lock(&root);
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(1));
    let moved = "Zz9Board: `u`: `ZZ9-33-S` (mpn ZZ9-33-S, jlcpcb-assembly C9900001, lcsc-retail C9900001) -> `ZZ9-33-A` (mpn ZZ9-33-A, jlcpcb-assembly C9900002, lcsc-retail C9900002)";
    assert_eq!(diff_lines(&check), [moved], "{}", stdout(&check));
    assert!(
        stdout(&check).starts_with(&format!("{} is not current\n", lock_file(&root).display()))
            && stderr(&check).contains("is not current: run `netsmith project lock --update`"),
        "{}{}",
        stdout(&check),
        stderr(&check)
    );
    // The locked row passes, so the solver has nothing to report about it.
    assert!(!stderr(&check).contains("note:"), "{}", stderr(&check));
    assert_eq!(read_lock(&root), before, "--check writes nothing");

    let update = netsmith(&root, &["project", "lock", "--update"]);
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
    assert!(netsmith(&root, &["project", "lock", "--check"]).status.success());
}

#[test]
fn a_row_that_no_longer_passes_or_is_gone_is_reported_in_the_solvers_words_and_replaced() {
    for (label, rows) in [
        ("fails", format!("{ROW_S_WEAK}{ROW_A}")),
        ("gone", ROW_A.to_string()),
    ] {
        let root = zz9_project(&format!("lock-update-{label}"), ROW_S);
        assert!(netsmith(&root, &["project", "lock", "--update"]).status.success());
        set_rows(&root, &rows);

        let check = netsmith(&root, &["project", "lock", "--check"]);
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
                    "Zz9Board: `u`: `ZZ9-33-S` (mpn ZZ9-33-S, jlcpcb-assembly C9900001, lcsc-retail C9900001) -> `ZZ9-33-A` (mpn ZZ9-33-A, jlcpcb-assembly C9900002, lcsc-retail C9900002)"
                ],
            "{label}: {}",
            stdout(&check)
        );

        let update = netsmith(&root, &["project", "lock", "--update"]);
        assert!(update.status.success(), "{label}: {}", stderr(&update));
        assert!(
            stderr(&update).contains("`u` is locked to the row `ZZ9-33-S`")
                && stderr(&update).contains("(replaced with the rows above)"),
            "{label}: {}",
            stderr(&update)
        );
        assert!(read_lock(&root).contains("row = \"ZZ9-33-A\""));
        assert!(netsmith(&root, &["project", "lock", "--check"]).status.success());
    }
}

#[test]
fn a_part_new_to_the_design_is_added_and_the_rows_of_the_others_stay() {
    let root = led_project("lock-update-added", LED);
    assert!(netsmith(&root, &["project", "lock", "--update"]).status.success());
    let before = read_lock(&root);
    fs::write(root.join("src/main.nsm"), with_second_led()).unwrap();

    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(1));
    let found = diff_lines(&check);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found[0].starts_with("LedExample: `led2`: added as ")
            && found[1].starts_with("LedExample: `r2`: added as "),
        "{found:?}"
    );
    assert_eq!(read_lock(&root), before);

    let update = netsmith(&root, &["project", "lock", "--update"]);
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
    assert!(netsmith(&root, &["project", "lock", "--check"]).status.success());
}

#[test]
fn a_part_the_design_no_longer_has_is_dropped_with_a_line() {
    let root = led_project("lock-update-dropped", &with_second_led());
    assert!(netsmith(&root, &["project", "lock", "--update"]).status.success());
    assert!(read_lock(&root).contains("\"r2\""));
    fs::write(root.join("src/main.nsm"), LED).unwrap();

    let check = netsmith(&root, &["project", "lock", "--check"]);
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

    let update = netsmith(&root, &["project", "lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert_eq!(diff_lines(&update), found);
    assert!(!read_lock(&root).contains("r2"), "{}", read_lock(&root));
    assert!(netsmith(&root, &["project", "lock", "--check"]).status.success());
    // It is the lock a fresh build of the smaller board writes.
    let fresh = led_project("lock-update-dropped-fresh", LED);
    assert!(netsmith(&fresh, &["board", "build"]).status.success());
    assert_eq!(read_lock(&root), read_lock(&fresh));
}

#[test]
fn the_diff_and_the_file_are_the_same_text_in_every_run() {
    let run_in = |label: &str| {
        let root = zz9_project(label, ROW_S);
        assert!(netsmith(&root, &["project", "lock", "--update"]).status.success());
        set_rows(&root, &format!("{ROW_A}{ROW_S}"));
        let check = netsmith(&root, &["project", "lock", "--check"]);
        let update = netsmith(&root, &["project", "lock", "--update"]);
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
    let run = netsmith(&root, &["project", "lock", "--update", "src/other.nsm"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(root.join("src/other.lock.toml").is_file());
    assert!(!lock_file(&root).exists());
    // With no file, every top-level file of the project.
    let run = netsmith(&root, &["project", "lock", "--update"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(lock_file(&root).is_file());
    assert!(netsmith(&root, &["project", "lock", "--check"]).status.success());
}

#[test]
fn a_lock_that_is_not_a_lock_or_a_design_that_does_not_build_is_exit_two() {
    let root = led_project("lock-update-wrong", LED);
    fs::write(lock_file(&root), "version = 7\n").unwrap();
    for mode in ["--update", "--check"] {
        let run = netsmith(&root, &["project", "lock", mode]);
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
    let run = netsmith(&root, &["project", "lock", "--update"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(stderr(&run).contains("error"), "{}", stderr(&run));
    assert!(!lock_file(&root).exists());
}

#[test]
fn lock_needs_exactly_one_of_update_and_check() {
    let root = led_project("lock-flags", LED);
    for (args, said) in [
        (vec!["project", "lock"], "<--update|--check>"),
        (vec!["project", "lock", "--update", "--check"], "cannot be used with"),
        (vec!["project", "lock", "--locked"], "unexpected argument '--locked'"),
        (vec!["board", "build", "--update"], "unexpected argument '--update'"),
    ] {
        let run = netsmith(&root, &args);
        assert_eq!(run.status.code(), Some(2), "{args:?}");
        assert!(stderr(&run).contains(said), "{args:?}: {}", stderr(&run));
    }
    // Both flags may be said twice, and `--help` lists the command.
    assert!(
        netsmith(&root, &["project", "lock", "--check", "--check"])
            .status
            .code()
            == Some(1)
    );
    let help = stdout(&netsmith(&root, &["project", "lock", "--help"]));
    assert!(
        help.contains("netsmith project lock [OPTIONS] <--update|--check> [FILE.NSM]")
            && help.contains("lock --update"),
        "{help}"
    );
    assert!(!lock_file(&root).exists());
}

/// Two regulators, `u` and `v`, each on a rail of its own, picked from the same catalog.
const ZZ9_TWO: &str = r#"import std::sources::Rail
import src::ics::Zz9

board Zz9Two {
    .in_u: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN_U"
        .neg.net &= "GND"
    }
    .in_v: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN_V"
        .neg.net &= "GND"
    }
    .u: Zz9 { .vout.voltage &= 3.0V..3.6V }
    .v: Zz9 { .vout.voltage &= 3.0V..3.6V }
    .out_u: Rail {
        .pos.net &= "3V3_U"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .out_v: Rail {
        .pos.net &= "3V3_V"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .in_u ~ .u.vin
    .u.vout ~ .out_u
    .u.en ~ .u.vin.pos
    .in_v ~ .v.vin
    .v.vout ~ .out_v
    .v.en ~ .v.vin.pos
}
"#;

/// The row named in the lock for the part `path` of `Zz9Two`, or `None`.
fn locked_row(root: &Path, path: &str) -> Option<String> {
    let text = read_lock(root);
    let at = text.find(&format!("[Zz9Two.parts.\"{path}\"]"))?;
    let row = text[at..].lines().find(|l| l.starts_with("row = "))?;
    Some(row.trim_start_matches("row = ").trim_matches('"').to_string())
}

/// A project with `Zz9Two`, locked on `ROW_S`, whose catalog then gains `ROW_A` before it: both
/// parts are due to move.
fn two_regulators_due_to_move(label: &str) -> TempDir {
    let root = zz9_project(label, ROW_S);
    fs::write(root.join("src/main.nsm"), ZZ9_TWO).unwrap();
    let run = netsmith(&root, &["project", "lock", "--update"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert_eq!(locked_row(&root, "u").as_deref(), Some("ZZ9-33-S"));
    assert_eq!(locked_row(&root, "v").as_deref(), Some("ZZ9-33-S"));
    set_rows(&root, &format!("{ROW_A}{ROW_S}"));
    root
}

#[test]
fn update_part_takes_a_new_pick_for_the_named_parts_only() {
    let root = two_regulators_due_to_move("lock-update-part");
    let run = netsmith(&root, &["project", "lock", "--update", "--part", "v"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert_eq!(
        diff_lines(&run),
        ["Zz9Two: `v`: `ZZ9-33-S` (mpn ZZ9-33-S, jlcpcb-assembly C9900001, lcsc-retail C9900001) -> `ZZ9-33-A` (mpn ZZ9-33-A, jlcpcb-assembly C9900002, lcsc-retail C9900002)"],
        "{}",
        stdout(&run)
    );
    assert_eq!(locked_row(&root, "u").as_deref(), Some("ZZ9-33-S"));
    assert_eq!(locked_row(&root, "v").as_deref(), Some("ZZ9-33-A"));
    // `u` is still due: the lock is not current, and `--check` says it.
    assert_eq!(
        netsmith(&root, &["project", "lock", "--check"]).status.code(),
        Some(1)
    );
}

#[test]
fn update_except_takes_a_new_pick_for_every_part_but_the_named_ones() {
    let root = two_regulators_due_to_move("lock-update-except");
    let run = netsmith(&root, &["project", "lock", "--update", "--except", "v"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert_eq!(diff_lines(&run).len(), 1, "{}", stdout(&run));
    assert_eq!(locked_row(&root, "u").as_deref(), Some("ZZ9-33-A"));
    assert_eq!(locked_row(&root, "v").as_deref(), Some("ZZ9-33-S"));
    // The names may be a list, and an update of all the rest is then the same as `--update`.
    let run = netsmith(&root, &["project", "lock", "--update", "--except", "u,v"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert_eq!(stdout(&run), "");
    assert_eq!(locked_row(&root, "v").as_deref(), Some("ZZ9-33-S"));
    assert!(netsmith(&root, &["project", "lock", "--update"]).status.success());
    assert_eq!(locked_row(&root, "v").as_deref(), Some("ZZ9-33-A"));
    assert!(netsmith(&root, &["project", "lock", "--check"]).status.success());
}

#[test]
fn a_part_named_that_no_board_has_is_exit_two_and_writes_nothing() {
    let root = two_regulators_due_to_move("lock-update-unknown");
    let before = read_lock(&root);
    for flag in ["--part", "--except"] {
        let run = netsmith(&root, &["project", "lock", "--update", flag, "u,w"]);
        assert_eq!(run.status.code(), Some(2), "{flag}");
        assert!(
            stderr(&run).contains("no board has the part `w`; the parts there are: `u`, `v`"),
            "{}",
            stderr(&run)
        );
        assert_eq!(read_lock(&root), before);
    }
}

#[test]
fn a_held_row_that_no_longer_passes_stops_a_partial_update_and_says_how_to_move_it() {
    let root = two_regulators_due_to_move("lock-update-part-failing");
    // `u` is held to `ZZ9-33-S`, which can no longer supply its 50 mA.
    set_rows(&root, &format!("{ROW_A}{ROW_S_WEAK}"));
    let before = read_lock(&root);
    let run = netsmith(&root, &["project", "lock", "--update", "--part", "v"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(
        stderr(&run).contains("name them to take a new pick"),
        "{}",
        stderr(&run)
    );
    assert_eq!(read_lock(&root), before);
    // Naming it moves it.
    let run = netsmith(&root, &["project", "lock", "--update", "--part", "u,v"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert_eq!(locked_row(&root, "u").as_deref(), Some("ZZ9-33-A"));
}

#[test]
fn part_needs_update_and_is_not_said_with_except() {
    let root = led_project("lock-part-flags", LED);
    for (args, said) in [
        (vec!["project", "lock", "--check", "--part", "r"], "--update"),
        (
            vec!["project", "lock", "--update", "--part", "r", "--except", "led"],
            "cannot be used with",
        ),
    ] {
        let run = netsmith(&root, &args);
        assert_eq!(run.status.code(), Some(2), "{args:?}");
        assert!(stderr(&run).contains(said), "{args:?}: {}", stderr(&run));
    }
}

fn set_lock_key(root: &Path, value: &str) {
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"led\"\ntop_level = [\"src/main.nsm\"]\nlock = \"{value}\"\n"),
    )
    .unwrap();
}

#[test]
fn lock_top_level_gives_a_lock_to_the_top_level_files_and_not_to_a_file_named_beside_them() {
    let root = led_project("lock-key-top-level", LED);
    set_lock_key(&root, "top_level");
    fs::write(root.join("src/part_test.nsm"), LED).unwrap();
    // A file named on the command line that the manifest does not list: no lock, read or written.
    let run = netsmith(&root, &["board", "build", "src/part_test.nsm"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(!root.join("src/part_test.lock.toml").exists());
    // A stale one beside it is not read either: a bad file does not stop the build.
    fs::write(root.join("src/part_test.lock.toml"), "version = 7\n").unwrap();
    let run = netsmith(&root, &["board", "build", "--locked", "src/part_test.nsm"]);
    assert!(run.status.success(), "{}", stderr(&run));
    // `lock` skips it; the top-level file has its lock.
    let run = netsmith(&root, &["project", "lock", "--update", "src/part_test.nsm"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert_eq!(stdout(&run), "");
    let run = netsmith(&root, &["board", "build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(lock_file(&root).is_file());
    assert!(netsmith(&root, &["project", "lock", "--check"]).status.success());
}

#[test]
fn lock_none_writes_no_lock_and_lock_check_has_nothing_to_find() {
    let root = led_project("lock-key-none", LED);
    set_lock_key(&root, "none");
    let run = netsmith(&root, &["board", "build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(!lock_file(&root).exists());
    let run = netsmith(&root, &["board", "build", "--locked"]);
    assert!(run.status.success(), "{}", stderr(&run));
    for mode in ["--update", "--check"] {
        let run = netsmith(&root, &["project", "lock", mode]);
        assert!(run.status.success(), "{mode}: {}", stderr(&run));
        assert_eq!(stdout(&run), "");
    }
    assert!(!lock_file(&root).exists());
}

#[test]
fn lock_all_is_what_a_project_has_with_no_key_and_another_word_is_refused() {
    let root = led_project("lock-key-all", LED);
    set_lock_key(&root, "all");
    assert!(netsmith(&root, &["board", "build"]).status.success());
    assert!(lock_file(&root).is_file());
    set_lock_key(&root, "boards");
    let run = netsmith(&root, &["board", "build"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(
        stderr(&run)
            .contains("lock = \"boards\" is not one of \"all\", \"top_level\" or \"none\""),
        "{}",
        stderr(&run)
    );
}

fn json(run: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not one JSON document ({e}): {}",
            String::from_utf8_lossy(&run.stdout)
        )
    })
}

#[test]
fn lock_prints_one_json_document_with_the_diff_the_text_has_and_the_same_exit_codes() {
    let root = led_project("lock-json", LED);
    // No lock yet: `--check` fails, the text's "does not exist".
    let check = netsmith(&root, &["project", "lock", "--check", "--format", "json"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    let doc = json(&check);
    assert_eq!(doc["command"], "lock");
    assert_eq!(doc["mode"], "check");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["files"][0]["status"], "missing");
    assert!(doc["files"][0]["file"].as_str().unwrap().ends_with("main.lock.toml"));
    assert!(!root.join("src/main.lock.toml").exists());

    // `--update` writes it, and says which parts it has, as the text's indented lines.
    let update = netsmith(&root, &["project", "lock", "--update", "--format", "json"]);
    assert_eq!(update.status.code(), Some(0), "{}", stderr(&update));
    let doc = json(&update);
    assert_eq!((doc["mode"].as_str(), doc["ok"].as_bool()), (Some("update"), Some(true)));
    assert_eq!(doc["files"][0]["status"], "wrote");
    let changes = doc["files"][0]["changes"].as_array().unwrap();
    assert!(!changes.is_empty());
    assert!(changes.iter().any(|c| c.as_str().unwrap().contains("r1")), "{changes:?}");
    assert!(root.join("src/main.lock.toml").exists());
    // The text prints the same lines.
    let text = std::fs::read_to_string(root.join("src/main.lock.toml")).unwrap();
    assert!(text.contains("r1"));

    // Now it is current: nothing to change.
    let check = netsmith(&root, &["project", "lock", "--check", "--format", "json"]);
    assert_eq!(check.status.code(), Some(0));
    let doc = json(&check);
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["files"][0]["status"], "current");
    assert_eq!(doc["files"][0]["changes"], serde_json::json!([]));

    // A part the lock lacks: not current, with the part in its changes.
    fs::write(root.join("src/main.nsm"), with_second_led()).unwrap();
    let check = netsmith(&root, &["project", "lock", "--check", "--format", "json"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    let doc = json(&check);
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["files"][0]["status"], "not_current");
    assert!(
        doc["files"][0]["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap().contains("r2")),
        "{doc}"
    );
    // The text's error is on stderr, and the document alone on stdout.
    assert!(stderr(&check).contains("is not current"));

    // A mistake in the run is exit code 2 with nothing on stdout.
    let bad = netsmith(&root, &["project", "lock", "--check", "--part", "r1", "--format", "json"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(bad.stdout.is_empty());
}

/// A project's own resistor row for the part the lock of `LED` holds (0402WGF3300TCE, C25104),
/// as a project that had the part before the built-in catalog did would write it, under a name
/// of its own (issue #1119).
const OWN_C25104: &str = "name,mpn,resistance,max_power,package,source\nACME-R330,0402WGF3300TCE,330ohm +/- 5%,62.5mW,0402,https://example.invalid/acme-r330\n";

/// The row name the lock of `LED` has for `r1`, and the lock with the name the project's row had.
fn lock_of_the_project_row(root: &Path) -> (String, String) {
    assert!(
        netsmith(root, &["project", "lock", "--update"])
            .status
            .success()
    );
    let lock = read_lock(root);
    let built_in = "330Ω 1% 0402 C25104";
    assert!(lock.contains(&format!("row = \"{built_in}\"")), "{lock}");
    (
        lock.clone(),
        lock.replace(
            &format!("row = \"{built_in}\""),
            "row = \"ACME-R330\"",
        ),
    )
}

#[test]
fn a_project_row_of_a_built_in_part_is_refused_naming_the_row_to_use_and_what_differs() {
    let root = led_project("lock-built-in-twin", LED);
    fs::create_dir_all(root.join("catalogs")).unwrap();
    netsmith_solve::migrate::write_to(&root, "catalogs/resistor.csv", OWN_C25104).unwrap();
    let run = netsmith(&root, &["board", "build"]);
    assert!(!run.status.success());
    let said = stderr(&run);
    for want in [
        "catalogs/resistor.csv:2: the mpn `0402WGF3300TCE` is already a row at data/jlcpcb/catalog/resistor.csv:",
        "a row of the built-in catalog (`330Ω 1% 0402 C25104`)",
        "they differ in name (`ACME-R330` here, `330Ω 1% 0402 C25104` built in)",
        "resistance (`330ohm +/- 5%` here,",
        "remove the row at catalogs/resistor.csv:2 from the project's file",
        "`row = \"ACME-R330\"` for the part takes `row = \"330Ω 1% 0402 C25104\"`",
        "`netsmith project lock --update`",
    ] {
        assert!(said.contains(want), "wanted {want}\n{said}");
    }
}

#[test]
fn a_lock_row_whose_part_the_built_in_catalog_has_under_another_name_is_told_and_update_repairs_it() {
    let root = led_project("lock-built-in-moved", LED);
    let (before, with_project_name) = lock_of_the_project_row(&root);
    fs::write(lock_file(&root), &with_project_name).unwrap();

    // The build names the row to use and the command that sets it.
    let build = netsmith(&root, &["board", "build"]);
    assert!(!build.status.success());
    let said = stderr(&build);
    assert!(
        said.contains("the `resistor` catalog has no row `ACME-R330` now; the row with the manufacturer part number 0402WGF3300TCE is `330Ω 1% 0402 C25104` now, and `netsmith project lock --update` sets the lock to it"),
        "{said}"
    );

    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    assert_eq!(
        diff_lines(&check),
        ["LedExample: `r1`: `ACME-R330` (mpn 0402WGF3300TCE, jlcpcb-assembly C25104, lcsc-retail C25104) -> `330Ω 1% 0402 C25104` (mpn 0402WGF3300TCE, jlcpcb-assembly C25104, lcsc-retail C25104)"],
        "{}",
        stdout(&check)
    );
    assert_eq!(read_lock(&root), with_project_name, "--check writes nothing");

    let update = netsmith(&root, &["project", "lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert_eq!(read_lock(&root), before, "the lock is as it was, with nothing else changed");
    assert!(netsmith(&root, &["board", "build", "--locked"]).status.success());
}

#[test]
fn update_part_rewrites_a_row_that_moved_to_the_built_in_one_whether_it_is_named_or_not() {
    let source = with_second_led();
    let root = led_project("lock-built-in-moved-part", &source);
    let (before, with_project_name) = lock_of_the_project_row(&root);
    // Both resistors have the project's name; only `r2` is named, and `r1` is rewritten, not re-picked.
    fs::write(lock_file(&root), &with_project_name).unwrap();
    let update = netsmith(&root, &["project", "lock", "--update", "--part", "r2"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert_eq!(read_lock(&root), before);
}

/// Adds `held = "reason"` to the lock's entry of the part `path` of `Zz9Two`.
fn hold(root: &Path, path: &str, reason: &str) {
    let text = read_lock(root);
    let header = format!("[Zz9Two.parts.\"{path}\"]\n");
    let at = text.find(&header).expect("the part is in the lock") + header.len();
    let held = format!("{}held = \"{reason}\"\n{}", &text[..at], &text[at..]);
    fs::write(lock_file(root), held).unwrap();
}

const WHY: &str = "the qualified row, kept for the first article";

#[test]
fn check_accepts_a_pick_held_on_purpose_and_lists_it_with_its_reason() {
    let root = two_regulators_due_to_move("lock-check-held");
    // Without a hold both parts are due to move, as before.
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(1));
    assert_eq!(diff_lines(&check).len(), 2, "{}", stdout(&check));

    hold(&root, "u", WHY);
    let before = read_lock(&root);
    // `v` is still due, so the lock is not current, and the diff says nothing of `u`.
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    let listed = "Zz9Two: `u`: `ZZ9-33-S` in the `zz9` catalog (mpn ZZ9-33-S, jlcpcb-assembly C9900001, lcsc-retail C9900001): ";
    assert!(
        diff_lines(&check)
            .iter()
            .all(|l| !l.starts_with("Zz9Two: `u`: `ZZ9-33-S` (")),
        "{}",
        stdout(&check)
    );
    assert_eq!(
        stdout(&check).matches(&format!("{listed}{WHY}")).count(),
        1,
        "{}",
        stdout(&check)
    );
    assert_eq!(read_lock(&root), before, "--check writes nothing");

    // `--update` moves `v`, and keeps `u` on its row with its key.
    let update = netsmith(&root, &["project", "lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert_eq!(locked_row(&root, "u").as_deref(), Some("ZZ9-33-S"));
    assert_eq!(locked_row(&root, "v").as_deref(), Some("ZZ9-33-A"));
    assert_eq!(
        read_lock(&root).matches(&format!("held = \"{WHY}\"")).count(),
        1,
        "{}",
        read_lock(&root)
    );
    let moved = |run: &std::process::Output| {
        diff_lines(run).into_iter().filter(|l| l.contains(" -> ")).count()
    };
    assert_eq!(moved(&update), 1, "{}", stdout(&update));

    // Now the lock is current: `--check` passes and says what it holds on purpose, once.
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert!(check.status.success(), "{}", stderr(&check));
    assert!(!stdout(&check).contains("not current"), "{}", stdout(&check));
    assert_eq!(
        stdout(&check).matches(&format!("{listed}{WHY}")).count(),
        1,
        "{}",
        stdout(&check)
    );
    let doc = json(&netsmith(&root, &["project", "lock", "--check", "--format", "json"]));
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["files"][0]["status"], "current");
    assert_eq!(
        doc["files"][0]["held"],
        serde_json::json!([{
            "board": "Zz9Two",
            "part": "u",
            "row": "`ZZ9-33-S` in the `zz9` catalog (mpn ZZ9-33-S, jlcpcb-assembly C9900001, lcsc-retail C9900001)",
            "reason": WHY,
        }])
    );
    assert!(doc["files"][0].get("not_judged").is_none());

    // A second update changes no byte.
    let kept = read_lock(&root);
    assert!(netsmith(&root, &["project", "lock", "--update"]).status.success());
    assert_eq!(read_lock(&root), kept);

    // Naming the part takes a new pick and drops the key.
    // `v` is held to the row too, as a part not named: both are named to move.
    let run = netsmith(&root, &["project", "lock", "--update", "--part", "u,v"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert_eq!(locked_row(&root, "u").as_deref(), Some("ZZ9-33-A"));
    assert!(!read_lock(&root).contains(WHY), "{}", read_lock(&root));
    assert_eq!(diff_lines(&run).len(), 1, "{}", stdout(&run));
    assert!(
        diff_lines(&run)[0].ends_with(&format!("(no longer held: {WHY})")),
        "{}",
        stdout(&run)
    );
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert!(check.status.success(), "{}", stderr(&check));
    assert!(!stdout(&check).contains("held in"), "{}", stdout(&check));
}

#[test]
fn update_except_and_part_leave_a_held_row_on_its_row_unless_it_is_named() {
    let root = two_regulators_due_to_move("lock-update-held-except");
    hold(&root, "u", WHY);
    // `--update --except v` would take a new pick for `u`, but `u` is held.
    let run = netsmith(&root, &["project", "lock", "--update", "--except", "v"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert_eq!(locked_row(&root, "u").as_deref(), Some("ZZ9-33-S"));
    assert_eq!(locked_row(&root, "v").as_deref(), Some("ZZ9-33-S"));
    assert!(read_lock(&root).contains(WHY), "{}", read_lock(&root));
}

#[test]
fn a_held_row_that_no_longer_passes_is_an_error_that_says_how_to_release_it() {
    let root = two_regulators_due_to_move("lock-check-held-failing");
    hold(&root, "u", WHY);
    set_rows(&root, &format!("{ROW_A}{ROW_S_WEAK}"));
    let before = read_lock(&root);
    for mode in ["--check", "--update"] {
        let run = netsmith(&root, &["project", "lock", mode]);
        assert_eq!(run.status.code(), Some(2), "{mode}");
        assert!(
            stderr(&run).contains("held on purpose") && stderr(&run).contains("remove its `held` line"),
            "{}",
            stderr(&run)
        );
        assert_eq!(read_lock(&root), before);
    }
    // `v` is held to the row too, as a part not named: both are named to move.
    let run = netsmith(&root, &["project", "lock", "--update", "--part", "u,v"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert_eq!(locked_row(&root, "u").as_deref(), Some("ZZ9-33-A"));
    assert!(!read_lock(&root).contains(WHY), "{}", read_lock(&root));
}

#[test]
fn check_except_judges_every_part_but_the_named_ones_and_lists_them() {
    let root = two_regulators_due_to_move("lock-check-except");
    let before = read_lock(&root);
    // `u` is not judged, `v` is due.
    let check = netsmith(&root, &["project", "lock", "--check", "--except", "u"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    let diff = diff_lines(&check);
    assert!(
        diff.iter().any(|l| l.starts_with("Zz9Two: `v`: `ZZ9-33-S` (")),
        "{}",
        stdout(&check)
    );
    assert!(
        !diff.iter().any(|l| l.starts_with("Zz9Two: `u`: `ZZ9-33-S` (")),
        "{}",
        stdout(&check)
    );
    assert!(stdout(&check).contains("not judged in "), "{}", stdout(&check));
    // Both not judged: the lock passes, and both are listed, in the text and the JSON.
    let check = netsmith(&root, &["project", "lock", "--check", "--except", "u,v"]);
    assert!(check.status.success(), "{}", stderr(&check));
    let text = stdout(&check);
    assert!(
        text.contains("Zz9Two: `u`: `ZZ9-33-S` in the `zz9` catalog (mpn ZZ9-33-S, jlcpcb-assembly C9900001, lcsc-retail C9900001)\n")
            && text.contains("Zz9Two: `v`: "),
        "{text}"
    );
    let doc = json(&netsmith(
        &root,
        &["project", "lock", "--check", "--except", "u,v", "--format", "json"],
    ));
    assert_eq!(doc["ok"], true);
    let skipped = doc["files"][0]["not_judged"].as_array().unwrap();
    assert_eq!(
        skipped.iter().map(|k| k["part"].as_str().unwrap()).collect::<Vec<_>>(),
        ["u", "v"]
    );
    assert!(doc["files"][0].get("held").is_none());
    // A part no board has is an error, as for `--update`.
    let unknown = netsmith(&root, &["project", "lock", "--check", "--except", "w"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(
        stderr(&unknown).contains("no board has the part `w`"),
        "{}",
        stderr(&unknown)
    );
    assert_eq!(read_lock(&root), before, "--check writes nothing");
}

#[test]
fn a_lock_of_version_1_builds_as_before_and_check_calls_it_not_current_until_update() {
    let root = led_project("lock-version-1", LED);
    assert!(netsmith(&root, &["board", "build"]).status.success());
    // The lock an older netsmith wrote: version 1, with no manufacturer.
    let v2 = read_lock(&root);
    assert!(v2.contains("\nversion = 2\n"), "{v2}");
    let v1 = v2.replace("\nversion = 2\n", "\nversion = 1\n");
    fs::write(lock_file(&root), &v1).unwrap();
    // A build still reads it and holds the parts to it, also with --locked, and says it is old.
    let build = netsmith(&root, &["board", "build", "--locked"]);
    assert!(build.status.success(), "{}", stderr(&build));
    assert!(
        stderr(&build).contains("main.lock.toml is of an older format of the lock, which records no manufacturer or no sku of each channel (ADR 0143); it holds the parts as before, and `netsmith project lock --update` writes it as version 2"),
        "{}",
        stderr(&build)
    );
    assert_eq!(read_lock(&root), v1, "a build never rewrites the lock");
    // `--check` calls it not current, with the line that says why.
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    assert_eq!(
        diff_lines(&check),
        ["version 1 -> 2: the file is of version 1 of the lock, and version 2 records each part's manufacturer and mpn, and the sku of its offer for each supply channel in place of its lcsc (ADR 0143)"]
    );
    // `--update` writes it as version 2, with the same rows, and it is then current.
    let update = netsmith(&root, &["project", "lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert_eq!(read_lock(&root), v2);
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
}

#[test]
fn the_lock_records_the_manufacturer_a_row_names() {
    let root = zz9_project("lock-manufacturer", "");
    fs::write(
        root.join("catalogs/zz9.csv"),
        "manufacturer,mpn,vout.voltage,dropout,max_current,package,source\n\
         Acme Analog,ZZ9-33,3.3V +/- 2%,0.3V,0.5A,SOT-23-5,https://example.invalid/acme.pdf\n\
         Zenith Semi,ZZ9-33,3.3V +/- 1%,0.2V,0.5A,SOT-23-5,https://example.invalid/zenith.pdf\n",
    )
    .unwrap();
    let update = netsmith(&root, &["project", "lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    let lock = read_lock(&root);
    assert!(
        lock.contains("[Zz9Board.parts.\"u\"]\ncatalog = \"zz9\"\nrow = \"ZZ9-33\"\nmanufacturer = \"Acme Analog\"\nmpn = \"ZZ9-33\"\n"),
        "{lock}"
    );
    // The lock holds the part to Acme's row though Zenith's shares its name, and stays current.
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    // Held to Zenith's row by hand, the lock is not current, and the diff names both makers.
    fs::write(lock_file(&root), lock.replace("Acme Analog", "Zenith Semi")).unwrap();
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    assert_eq!(
        diff_lines(&check),
        ["Zz9Board: `u`: `ZZ9-33` (manufacturer Zenith Semi, mpn ZZ9-33) -> `ZZ9-33` (manufacturer Acme Analog, mpn ZZ9-33)"]
    );
}
