//! The pick lock records each part's designator (issue #1653): a part keeps it when parts are
//! added, a new part takes the number after the highest of its letters the lock records, and
//! `project lock --update --renumber` numbers the parts by path again.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

/// A second LED with its resistor, named to sort before `led` and `r1`.
const BEFORE: &str = "
    .indicator: Led { .current &= 5mA..15mA }
    .ballast: Resistor {}
    .indicator.current == (.supply.voltage - .indicator.forward_voltage) / .ballast.resistance
    .supply.pos ~> .ballast ~> .indicator ~> .supply.neg
";

/// A second LED with its resistor, named to sort after `led` and `r1`.
const AFTER: &str = "
    .led2: Led { .current &= 5mA..15mA }
    .r2: Resistor {}
    .led2.current == (.supply.voltage - .led2.forward_voltage) / .r2.resistance
    .supply.pos ~> .r2 ~> .led2 ~> .supply.neg
";

fn with(extra: &str) -> String {
    let at = LED.rfind('}').unwrap();
    format!("{}{extra}}}\n", &LED[..at])
}

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

fn lock_file(root: &Path) -> std::path::PathBuf {
    root.join("src/main.lock.toml")
}

fn read_lock(root: &Path) -> String {
    fs::read_to_string(lock_file(root)).unwrap()
}

/// The designator of each part of the board a build makes, by path, in the design's order.
fn designators(root: &Path, extra: &[&str]) -> Vec<(String, String)> {
    let mut args = vec!["board", "build", "--emit-design", "design.json"];
    args.extend_from_slice(extra);
    let run = netsmith(root, &args);
    assert!(run.status.success(), "{}", stderr(&run));
    let text = fs::read_to_string(root.join("design.json")).unwrap();
    let designs: serde_json::Value = serde_json::from_str(&text).unwrap();
    designs[0]["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["path"].as_str().unwrap().to_string(),
                p["refdes"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(p, d)| (p.to_string(), d.to_string()))
        .collect()
}

#[test]
fn with_no_record_the_parts_are_numbered_by_path_and_the_first_build_records_them() {
    let root = project("designators-no-record", &with(BEFORE));
    // No lock yet: by path, as before the lock recorded designators.
    let found = designators(&root, &[]);
    assert_eq!(
        found,
        pairs(&[
            ("ballast", "R1"),
            ("indicator", "D1"),
            ("led", "D2"),
            ("power_in", "J1"),
            ("r1", "R2"),
        ])
    );
    let lock = read_lock(&root);
    assert!(
        lock.contains(
            "\n[LedExample.designators]\n\"ballast\" = \"R1\"\n\"indicator\" = \"D1\"\n\"led\" = \"D2\"\n\"power_in\" = \"J1\"\n\"r1\" = \"R2\"\n"
        ),
        "{lock}"
    );
    // A lock with no record numbers by path too, and `--locked` does not call it a change.
    let without: String = lock[..lock.find("\n[LedExample.designators]").unwrap() + 1].to_string();
    fs::write(lock_file(&root), &without).unwrap();
    assert_eq!(designators(&root, &["--locked"]), found);
    // `--check` says the record is missing, and `--update` writes it.
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    assert!(
        stdout(&check).contains("LedExample: designators recorded for 5 parts, as they are numbered now"),
        "{}",
        stdout(&check)
    );
    let update = netsmith(&root, &["project", "lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert_eq!(read_lock(&root), lock);
}

#[test]
fn added_parts_keep_the_recorded_designators_and_take_the_numbers_after_them() {
    let root = project("designators-added", LED);
    assert_eq!(
        designators(&root, &[]),
        pairs(&[("led", "D1"), ("power_in", "J1"), ("r1", "R1")])
    );
    let lock = read_lock(&root);
    fs::write(root.join("src/main.nsm"), with(BEFORE)).unwrap();
    // The new parts sort first and take the numbers after the recorded ones.
    let found = designators(&root, &[]);
    assert_eq!(
        found,
        pairs(&[
            ("ballast", "R2"),
            ("indicator", "D2"),
            ("led", "D1"),
            ("power_in", "J1"),
            ("r1", "R1"),
        ])
    );
    // The build leaves the lock as it was; the update records the new parts as numbered.
    assert_eq!(read_lock(&root), lock);
    let update = netsmith(&root, &["project", "lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    let lock = read_lock(&root);
    assert!(
        lock.contains("\"ballast\" = \"R2\"") && lock.contains("\"r1\" = \"R1\""),
        "{lock}"
    );
    assert_eq!(designators(&root, &["--locked"]), found);

    // `--renumber` numbers every part by path again.
    let renumber = netsmith(&root, &["project", "lock", "--update", "--renumber"]);
    assert!(renumber.status.success(), "{}", stderr(&renumber));
    let said = stdout(&renumber);
    assert!(
        said.contains("LedExample: `ballast`: designator R2 -> R1")
            && said.contains("LedExample: `r1`: designator R1 -> R2")
            && said.contains("LedExample: `led`: designator D1 -> D2"),
        "{said}"
    );
    assert_eq!(
        designators(&root, &["--locked"]),
        pairs(&[
            ("ballast", "R1"),
            ("indicator", "D1"),
            ("led", "D2"),
            ("power_in", "J1"),
            ("r1", "R2"),
        ])
    );
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert!(check.status.success(), "{}", stderr(&check));
}

#[test]
fn a_removed_parts_number_is_not_taken_again_while_the_lock_records_it() {
    let root = project("designators-removed", &with(BEFORE));
    assert!(netsmith(&root, &["board", "build"]).status.success());
    // `ballast` (R1) and `indicator` (D1) go, `r2` and `led2` come.
    fs::write(root.join("src/main.nsm"), with(AFTER)).unwrap();
    let found = designators(&root, &[]);
    assert_eq!(
        found,
        pairs(&[
            ("led", "D2"),
            ("led2", "D3"),
            ("power_in", "J1"),
            ("r1", "R2"),
            ("r2", "R3"),
        ])
    );
    // `--locked` calls a part the lock has no designator for, and an entry of a part that is
    // gone, a change.
    let locked = netsmith(&root, &["board", "build", "--locked"]);
    assert!(!locked.status.success());
    // `--update` drops the entries of the parts that are gone and keeps the numbers.
    let update = netsmith(&root, &["project", "lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    let lock = read_lock(&root);
    assert!(!lock.contains("\"ballast\""), "{lock}");
    assert_eq!(designators(&root, &["--locked"]), found);
}

#[test]
fn a_part_with_no_row_is_numbered_by_the_lock_too_and_told_when_it_is_new() {
    // A mounting hole picks no catalog row: its designator is the only thing the lock has of it.
    let root = project("designators-hole", LED);
    assert!(netsmith(&root, &["board", "build"]).status.success());
    fs::write(
        root.join("src/main.physical.nsm"),
        "import std::parts::mechanical::MountingHole\n\nphysical for LedExample {\n    case {\n        .hole: MountingHole\n    }\n}\n",
    )
    .unwrap();
    let run = netsmith(&root, &["board", "build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("LedExample: `hole` has no designator in the lock (it is numbered H1)"),
        "{}",
        stderr(&run)
    );
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    assert!(
        stdout(&check).contains("LedExample: `hole`: numbered H1"),
        "{}",
        stdout(&check)
    );
}

#[test]
fn two_parts_the_lock_gives_one_designator_are_an_error_naming_both() {
    let root = project("designators-duplicate", &with(BEFORE));
    assert!(netsmith(&root, &["board", "build"]).status.success());
    let lock = read_lock(&root).replace("\"r1\" = \"R2\"", "\"r1\" = \"R1\"");
    fs::write(lock_file(&root), &lock).unwrap();
    let run = netsmith(&root, &["board", "build"]);
    assert!(!run.status.success());
    assert!(
        stderr(&run).contains(
            "the pick lock gives `ballast` and `r1` the same designator R1, and a designator names one part"
        ),
        "{}",
        stderr(&run)
    );
    // A plain update cannot keep both; `--renumber` numbers them by path.
    let update = netsmith(&root, &["project", "lock", "--update"]);
    assert_eq!(update.status.code(), Some(2), "{}", stderr(&update));
    let renumber = netsmith(&root, &["project", "lock", "--update", "--renumber"]);
    assert!(renumber.status.success(), "{}", stderr(&renumber));
    assert!(read_lock(&root).contains("\"r1\" = \"R2\""));
    // A designator that is not one is refused with its place in the file.
    let wrong = read_lock(&root).replace("\"r1\" = \"R2\"", "\"r1\" = \"R02\"");
    fs::write(lock_file(&root), &wrong).unwrap();
    let run = netsmith(&root, &["board", "build"]);
    assert!(
        stderr(&run).contains("[LedExample.designators] `r1` is \"R02\", which is not a designator"),
        "{}",
        stderr(&run)
    );
}
