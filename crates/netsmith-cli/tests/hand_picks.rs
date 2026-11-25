//! A hand pick end to end (ADR 0122, ADR 0143): `.mpn &= "..."` in the source holds a part to that row,
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

/// The LED board with `r1` held to the row with the manufacturer part number `mpn`.
fn hand_picked(mpn: &str) -> String {
    LED.replace(
        ".r1: Resistor {}",
        &format!(".r1: Resistor {{ .mpn &= \"{mpn}\" }}"),
    )
}

/// The manufacturer part numbers of the rows of `OTHER_ROW` and of a 2.2 kohm 0402 resistor.
const OTHER_MPN: &str = "0603WAF3900T5E";
const HOT_MPN: &str = "0402WGF2201TCE";

#[test]
fn a_hand_pick_is_the_pick_and_the_lock_records_it() {
    let root = project("hand-pick", &hand_picked(OTHER_MPN));
    let run = netsmith(&root, &["board", "build"]);
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
    assert!(netsmith(&root, &["board", "build"]).status.success());
    assert!(lock(&root).contains(SOLVER_ROW));
    fs::write(root.join("src/main.nsm"), hand_picked(OTHER_MPN)).unwrap();
    // The build takes the hand pick and says that the lock differs, which it leaves alone.
    let run = netsmith(&root, &["board", "build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(stdout(&run).contains("R_0603"), "{}", stdout(&run));
    assert!(
        stderr(&run).contains(&format!(
            "`r1` is locked to `{SOLVER_ROW}` and was picked as `{OTHER_ROW}`"
        )),
        "{}",
        stderr(&run)
    );
    assert!(!netsmith(&root, &["board", "build", "--locked"]).status.success());
    // `project lock --check` says the lock is not current, and `--update` writes the hand-picked row.
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    let update = netsmith(&root, &["project", "lock", "--update"]);
    assert!(update.status.success(), "{}", stderr(&update));
    assert!(lock(&root).contains(OTHER_ROW), "{}", lock(&root));
    assert!(netsmith(&root, &["project", "lock", "--check"]).status.success());
    assert!(netsmith(&root, &["board", "build", "--locked"]).status.success());
}

/// Where `source`, the project's `src/main.nsm`, has `text`: `src/main.nsm:<line>`.
fn place(source: &str, text: &str) -> String {
    let line = source.lines().position(|l| l.contains(text)).unwrap();
    format!("src/main.nsm:{}", line + 1)
}

/// The lines of a run's output about the row of the part at `path` of the LED board (not the
/// line about its footprint).
fn lines_of(said: &str, path: &str) -> Vec<String> {
    let head = format!("  LedExample: `{path}`: ");
    said.lines()
        .filter_map(|l| l.strip_prefix(&head))
        .filter(|l| !l.starts_with("footprint"))
        .map(str::to_string)
        .collect()
}

#[test]
fn the_lock_diff_says_a_row_was_taken_for_a_hand_pick_and_where_the_source_states_it() {
    let root = project("hand-pick-diff", LED);
    assert!(netsmith(&root, &["board", "build"]).status.success());
    let source = hand_picked(OTHER_MPN);
    fs::write(root.join("src/main.nsm"), &source).unwrap();
    let note = format!(
        "; taken for the hand pick in the source: `.mpn &= \"{OTHER_MPN}\"` at {}",
        place(&source, ".mpn &=")
    );
    for (mode, code) in [("--check", 1), ("--update", 0)] {
        let run = netsmith(&root, &["project", "lock", mode]);
        assert_eq!(run.status.code(), Some(code), "{}", stderr(&run));
        let said = stdout(&run);
        let lines = lines_of(&said, "r1");
        assert!(
            lines.len() == 1
                && lines[0].starts_with(&format!("`{SOLVER_ROW}` "))
                && lines[0].contains(&format!(" -> `{OTHER_ROW}` "))
                && lines[0].ends_with(&note),
            "{mode}: {said}"
        );
    }
    // The same line in the JSON document, for a part the lock had no entry for.
    fs::remove_file(root.join("src/main.lock.toml")).unwrap();
    let run = netsmith(&root, &["project", "lock", "--update", "--format", "json"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let document: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    let changes: Vec<&str> = document["files"][0]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    // Only the hand-picked part's line says it.
    assert!(changes.len() > 1, "{changes:?}");
    for line in &changes {
        assert_eq!(
            line.contains("hand pick"),
            line.starts_with("LedExample: `r1`: added as "),
            "{changes:?}"
        );
    }
    assert!(changes.iter().any(|l| l.ends_with(&note)), "{changes:?}");
    // The pick removed, the part takes the solver's row, and the line says nothing new.
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    let run = netsmith(&root, &["project", "lock", "--update"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let said = stdout(&run);
    let lines = lines_of(&said, "r1");
    assert!(
        lines.len() == 1 && lines[0].contains(&format!(" -> `{SOLVER_ROW}` ")),
        "{said}"
    );
    assert!(!said.contains("hand pick"), "{said}");
}

#[test]
fn the_note_of_a_hand_pick_names_the_manufacturer_and_its_own_place() {
    let source = LED.replace(
        ".r1: Resistor {}",
        ".r1: Resistor {\n        .mpn &= \"ZZR-316\"\n        .manufacturer &= \"Zeta\"\n    }",
    );
    let root = project("hand-pick-maker", &source);
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(
        root.join("catalogs/resistor.csv"),
        "manufacturer,mpn,resistance,max_power,package,price,stock,date,source\n\
         Zeta,ZZR-316,316ohm +/- 1%,250mW,0402,0.01,10,2026-10-31,https://example.invalid/zzr\n",
    )
    .unwrap();
    let run = netsmith(&root, &["project", "lock", "--update"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let said = stdout(&run);
    let note = format!(
        "; taken for the hand pick in the source: `.mpn &= \"ZZR-316\"` at {} and `.manufacturer &= \"Zeta\"` at {}",
        place(&source, ".mpn &="),
        place(&source, ".manufacturer &=")
    );
    let lines = lines_of(&said, "r1");
    assert!(lines.len() == 1 && lines[0].ends_with(&note), "{said}");
}

/// A regulator sold as two order codes, with a keyed catalog, and a board that needs a feature
/// of one of them.
const CODES_PART: &str = r#"import std::sources::Rail

part Zz9 {
    .variants {
        "ZZ9-33-S" PLAIN
        "ZZ9-33-A" AUTOMOTIVE
    }
    .vout: Rail
    pins "SOT-23-5" {
        .vout.pos      power       "OUT"    "5"
        .vout.neg      power       "GND"    "2"
        .nc            no_connect  "NC"     "4"
    }
    .package: Package
    catalog zz9 by .mpn { .mpn, .package }
}
"#;

const CODES_BOARD: &str = r#"import std::sources::Rail
import src::ics::Zz9

board Zz9Board {
    .u: Zz9
    .u.needs &= AUTOMOTIVE
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .u.vout ~ .output
}
"#;

#[test]
fn the_note_of_an_order_code_names_the_needs_that_left_it() {
    let root = project("hand-pick-order-code", CODES_BOARD);
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"zz\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/ics.nsm"), CODES_PART).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(
        root.join("catalogs/zz9.csv"),
        "mpn,package,price,stock,date,source\n\
         ZZ9-33-S,SOT-23-5,0.10,1000,2026-10-31,https://example.invalid/zz9-33-s.pdf\n\
         ZZ9-33-A,SOT-23-5,0.20,1000,2026-10-31,https://example.invalid/zz9-33-a.pdf\n",
    )
    .unwrap();
    let run = netsmith(&root, &["project", "lock", "--update"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let said = stdout(&run);
    let note = format!(
        "; taken for the hand pick in the source: the order code `ZZ9-33-A`, which the design's `.needs` at {} leave",
        place(CODES_BOARD, ".needs &=")
    );
    assert!(
        said.lines()
            .any(|l| l.starts_with("  Zz9Board: `u`: added as ") && l.ends_with(&note)),
        "{said}"
    );
}

#[test]
fn a_hand_picked_row_that_fails_stops_the_build_at_the_narrowing() {
    // 2.2 kohm in 0402 overheats at the LED's current.
    let source = hand_picked(HOT_MPN);
    let root = project("hand-pick-fails", &source);
    let run = netsmith(&root, &["board", "build"]);
    assert!(!run.status.success());
    let said = stderr(&run);
    assert!(said.contains("max_power"), "{said}");
    // The second message points at the narrowing, wherever the board puts it.
    let (line, text) = source
        .lines()
        .enumerate()
        .find(|(_, l)| l.contains(".mpn &="))
        .unwrap();
    let at = format!("main.nsm:{}:{}", line + 1, text.find(".mpn").unwrap() + 1);
    assert!(
        said.contains(&format!("`r1` is held by `.mpn &= \"{HOT_MPN}\"` to one row of the `resistor` catalog, and no other row was tried for it: if the failure above is about it, change the design so that the row fits, or change the narrowing\n   --> src/{at}")),
        "{said}"
    );
    // `project lock --check` cannot tell, and says why in the solver's words.
    let check = netsmith(&root, &["project", "lock", "--check"]);
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
    for args in [&["board", "build"][..], &["project", "lock", "--check"], &["project", "lock", "--update"]] {
        let run = netsmith(&root, args);
        assert_eq!(run.status.code(), Some(2), "{args:?}: {}", stderr(&run));
        assert!(
            stderr(&run).contains("main.picks.toml: netsmith no longer reads a picks file (ADR 0122)")
                && stderr(&run).contains(&format!("  in `board LedExample`:\n    .r1.mpn &= \"{OTHER_MPN}\"")),
            "{args:?}: {}",
            stderr(&run)
        );
    }
    assert!(!root.join("src/main.lock.toml").exists());
    // Moved into the source and deleted, the entry is the same pick.
    fs::write(root.join("src/main.nsm"), hand_picked(OTHER_MPN)).unwrap();
    fs::remove_file(root.join("src/main.picks.toml")).unwrap();
    let run = netsmith(&root, &["board", "build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(lock(&root).contains(OTHER_ROW), "{}", lock(&root));
}

#[test]
fn a_hand_pick_by_the_lcsc_number_stops_the_build_with_the_mpn_and_migrate_offers_rewrites_it() {
    let source = LED.replace(".r1: Resistor {}", ".r1: Resistor { .lcsc &= \"C23151\" }");
    let root = project("hand-pick-lcsc", &source);
    let run = netsmith(&root, &["board", "build"]);
    assert!(!run.status.success());
    assert!(
        stderr(&run).contains(&format!("`.lcsc &= \"C23151\"` on `r1`: a hand pick is by the manufacturer's part number now, and a supplier's number is an offer (ADR 0143): write `.mpn &= \"{OTHER_MPN}\"` in its place")),
        "{}",
        stderr(&run)
    );
    // The command rewrites it, and then the build takes the row it named.
    let check = netsmith(&root, &["project", "migrate-offers", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    assert_eq!(fs::read_to_string(root.join("src/main.nsm")).unwrap(), source);
    let moved = netsmith(&root, &["project", "migrate-offers"]);
    assert!(moved.status.success(), "{}", stderr(&moved));
    assert!(
        stdout(&moved).contains("src/main.nsm\n  line ") && stdout(&moved).contains(&format!("-> `.mpn &= \"{OTHER_MPN}\"`")),
        "{}",
        stdout(&moved)
    );
    assert_eq!(
        fs::read_to_string(root.join("src/main.nsm")).unwrap(),
        hand_picked(OTHER_MPN)
    );
    let run = netsmith(&root, &["board", "build"]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(lock(&root).contains(OTHER_ROW), "{}", lock(&root));
    // A second run has nothing to move.
    let again = netsmith(&root, &["project", "migrate-offers", "--check"]);
    assert_eq!(again.status.code(), Some(0), "{}", stderr(&again));
}
