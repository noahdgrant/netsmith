//! `netsmith schematic` (issue #1093): `show` says the board's sheets and each entry of the
//! schematic layout file, `set` writes one entry and keeps the file's other tables and comments,
//! refusing a name the board does not have, `check` says which entries name nothing or which the
//! drawing could not honour with the line of each, and `arrange` says where the drawing sets the
//! boxes of a big board's sheets and keeps them. The board is an invented one of nine instances
//! of a two-part module (18 parts, so it is drawn by module), and a small one of three parts
//! (drawn as one sheet).

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

const CELLS: &str = "abcdefghi";

fn many() -> String {
    let mut text = String::from(
        "\
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

module Cell {
    .supply: Rail
    .led: Led {
        .current &= 5mA..15mA
    }
    .r1: Resistor {}
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

board Many {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
",
    );
    for cell in CELLS.chars() {
        text.push_str(&format!("    .{cell}: Cell\n    .supply ~ .{cell}.supply\n"));
    }
    text.push_str("}\n");
    text
}

const SMALL: &str = "\
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Small {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .led: Led {
        .current &= 5mA..15mA
    }
    .r1: Resistor {}
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
";

/// A project of `source` with `file` as its schematic layout file if there is one.
fn project(tag: &str, source: &str, file: Option<&str>) -> TempDir {
    let dir = TempDir::new(tag);
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"main.nsm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("main.nsm"), source).unwrap();
    if let Some(file) = file {
        fs::write(dir.join("main.sch.toml"), file).unwrap();
    }
    dir
}

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .arg("schematic")
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn stdout(run: &Output) -> String {
    String::from_utf8_lossy(&run.stdout).into_owned()
}

fn stderr(run: &Output) -> String {
    String::from_utf8_lossy(&run.stderr).into_owned()
}

fn code(run: &Output) -> i32 {
    run.status.code().unwrap()
}

fn json(run: &Output) -> serde_json::Value {
    serde_json::from_str(&stdout(run)).unwrap_or_else(|e| panic!("{e}: {}", stdout(run)))
}

fn file(dir: &Path) -> String {
    fs::read_to_string(dir.join("main.sch.toml")).unwrap_or_default()
}

const HAND: &str = "\
# Notes of the person who arranged this.
version = 1

# a goes where b is drawn without it
[Many.sheets.a]
at = [95.25, 0] # keep this too

[Many.sheets.b.parts.r1]
dx = 2.54
dy = 0

[Other.parts.x]
dx = 1.27
dy = 0
";

#[test]
fn show_says_the_sheets_and_each_entry_with_its_line() {
    let dir = project("sch-show", &many(), Some(HAND));
    let run = netsmith(&dir, &["show"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.contains("the board is drawn by module"), "{text}");
    assert!(text.contains("a: 2 parts, box at 95.25, 0"), "{text}");
    assert!(text.contains("placed"), "{text}");
    assert!(text.contains("main.sch.toml:6: sheet a  at 95.25, 0"), "{text}");
    assert!(
        text.contains("main.sch.toml:8: part b.r1  dx 2.54 dy 0"),
        "{text}"
    );

    let run = netsmith(&dir, &["show", "b", "--format", "json"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let shown = json(&run);
    assert_eq!(shown["board"], "Many");
    assert_eq!(shown["exists"], true);
    assert_eq!(shown["by_module"], true);
    assert_eq!(shown["sheets"].as_array().unwrap().len(), 1);
    let entries = shown["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["kind"], "part");
    assert_eq!(entries[0]["path"], "b.r1");
    assert_eq!(entries[0]["sheet"], "b");
    assert_eq!(entries[0]["dx"], 2.54);
    assert_eq!(entries[0]["line"], 8);
    assert_eq!(entries[0]["used"], true);
}

#[test]
fn set_then_show_round_trips_and_keeps_comments_and_other_tables() {
    let dir = project("sch-set", &many(), Some(HAND));
    let run = netsmith(
        &dir,
        &["set", "c.led", "--by", "2.54,-1.27", "--rotate", "90", "--mirror"],
    );
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(stdout(&run).contains("part c.led: dx 2.54 dy -1.27  rotate 90  mirror true"));
    // A second set changes one field and keeps the others.
    let run = netsmith(&dir, &["set", "c.led", "--rotate", "180"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let run = netsmith(&dir, &["set", "d", "--at", "190.5,0"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));

    let text = file(&dir);
    assert!(text.contains("# Notes of the person who arranged this."), "{text}");
    assert!(text.contains("# a goes where b is drawn without it"), "{text}");
    assert!(text.contains("at = [95.25, 0] # keep this too"), "{text}");
    assert!(text.contains("[Other.parts.x]\ndx = 1.27"), "{text}");
    assert!(text.contains("[Many.sheets.b.parts.r1]\ndx = 2.54"), "{text}");

    let run = netsmith(&dir, &["show", "c.led", "d", "--format", "json"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let shown = json(&run);
    let entries = shown["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "{shown}");
    let led = entries.iter().find(|e| e["kind"] == "part").unwrap();
    assert_eq!(led["dx"], 2.54);
    assert_eq!(led["dy"], -1.27);
    assert_eq!(led["rotate"], 180);
    assert_eq!(led["mirror"], true);
    let sheet = entries.iter().find(|e| e["kind"] == "sheet").unwrap();
    assert_eq!(sheet["at"], serde_json::json!([190.5, 0.0]));

    // `--clear` removes an entry, and a sheet's `at`.
    let run = netsmith(&dir, &["set", "c.led", "--clear"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let run = netsmith(&dir, &["set", "d", "--clear"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let text = file(&dir);
    assert!(!text.contains("c.led") && !text.contains("sheets.c") && !text.contains("sheets.d"));
    assert!(text.contains("# Notes of the person who arranged this."), "{text}");
}

#[test]
fn set_makes_the_file_and_a_set_that_changes_nothing_leaves_it_as_it_is() {
    let dir = project("sch-new", &many(), None);
    assert!(!dir.join("main.sch.toml").exists());
    let run = netsmith(&dir, &["set", "a.r1", "--by", "1.27,0"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert_eq!(
        file(&dir),
        "version = 1\n\n[Many.sheets.a.parts.r1]\ndx = 1.27\ndy = 0\n"
    );
    // The same again: the file is not written, and says so.
    fs::write(
        dir.join("main.sch.toml"),
        "# spaced   by hand\nversion = 1\n[Many.sheets.a.parts.r1]\ndx=1.27\ndy=0\n",
    )
    .unwrap();
    let before = file(&dir);
    let run = netsmith(&dir, &["set", "a.r1", "--by", "1.27,0"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(stdout(&run).contains("the file is as it was"));
    assert_eq!(file(&dir), before);
}

#[test]
fn a_bad_name_or_value_is_refused_with_the_file_untouched() {
    let dir = project("sch-refuse", &many(), Some(HAND));
    let before = file(&dir);
    for (args, said) in [
        (&["set", "a.nope", "--by", "0,0"][..], "has no part `a.nope`"),
        (&["set", "zzz", "--at", "0,0"], "has no sheet `zzz`"),
        (&["set", "a.r1", "--by", "1,0"], "is not on the schematic's 1.27 mm grid"),
        (&["set", "a.r1", "--rotate", "45"], "0, 90, 180 or 270"),
        (&["set", "a.r1"], "nothing to set"),
        (&["set", "a.r1", "--at", "0,0"], "has no sheet `a.r1`"),
        (&["set", "a", "--by", "0,0"], "has no part `a`"),
        (&["set", "a.r1", "--clear", "--rotate", "90"], "goes with no other flag"),
        (&["set", "a.r1", "--by", "5081.27,0"], "typo"),
        (&["show", "a.nope"], "has no sheet or part `a.nope`"),
    ] {
        let run = netsmith(&dir, args);
        assert_eq!(code(&run), 1, "{args:?}: {}", stderr(&run));
        assert!(stderr(&run).contains(said), "{args:?}: {}", stderr(&run));
        assert_eq!(file(&dir), before, "{args:?}");
    }
    // The nearest names are listed.
    let run = netsmith(&dir, &["set", "a.led2", "--by", "0,0"]);
    assert!(stderr(&run).contains("`a.led`"), "{}", stderr(&run));
    // Two flags that exclude each other are a usage mistake.
    assert_eq!(code(&netsmith(&dir, &["set", "a.r1", "--by", "0,0", "--pin", "0,0"])), 2);
    assert_eq!(file(&dir), before);
    // With no file, a refusal does not make one.
    let dir = project("sch-refuse-new", &many(), None);
    assert_eq!(code(&netsmith(&dir, &["set", "a.nope", "--by", "0,0"])), 1);
    assert!(!dir.join("main.sch.toml").exists());
}

#[test]
fn a_file_with_a_fault_is_refused_by_set_and_named_by_check() {
    let text = "version = 1\n\n[Many.sheets.a.parts.r1]\ndx = 1.3\ndy = 0\n";
    let dir = project("sch-fault", &many(), Some(text));
    for args in [&["set", "a.led", "--by", "0,0"][..], &["show"], &["arrange"]] {
        let run = netsmith(&dir, args);
        assert_eq!(code(&run), 1, "{args:?}");
        assert!(stderr(&run).contains("main.sch.toml:4:"), "{args:?}: {}", stderr(&run));
        assert!(stderr(&run).contains("fix the line first"), "{}", stderr(&run));
    }
    assert_eq!(file(&dir), text);
    let run = netsmith(&dir, &["check", "--format", "json"]);
    assert_eq!(code(&run), 1);
    let checked = json(&run);
    assert_eq!(checked["ok"], false);
    assert_eq!(checked["findings"][0]["kind"], "invalid");
    assert_eq!(checked["findings"][0]["line"], 4);
}

#[test]
fn check_says_the_entries_that_name_nothing_with_their_lines() {
    let text = "\
version = 1

[Many.sheets.a.parts.gone]
dx = 1.27
dy = 0

[Many.sheets.nowhere.parts.r1]
dx = 1.27
dy = 0

[Many.sheets.b.parts.r1]
dx = 1.27
dy = 0

[Many.parts.\"a.r1\"]
dx = 1.27
dy = 0

[Other.parts.x]
dx = 1.27
dy = 0
";
    let dir = project("sch-check", &many(), Some(text));
    let run = netsmith(&dir, &["check"]);
    assert_eq!(code(&run), 1, "{}", stderr(&run));
    let said = stdout(&run);
    assert!(said.contains("main.sch.toml:3: "), "{said}");
    assert!(said.contains("no part `gone`"), "{said}");
    assert!(said.contains("main.sch.toml:7: "), "{said}");
    assert!(said.contains("no sheet `nowhere`"), "{said}");
    assert!(said.contains("main.sch.toml:15: "), "{said}");
    assert!(said.contains("main.sch.toml:19: "), "{said}");
    assert!(said.contains("no board `Other`"), "{said}");
    // The entry that matches is not named.
    assert!(!said.contains("main.sch.toml:11:"), "{said}");

    let run = netsmith(&dir, &["check", "--format", "json"]);
    assert_eq!(code(&run), 1);
    let checked = json(&run);
    assert_eq!(checked["ok"], false);
    let kinds: Vec<&str> = checked["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["stale_entry", "stale_sheet", "wrong_sheet", "unknown_board"]
    );

    // And `show` marks them ignored.
    let run = netsmith(&dir, &["show", "a", "b", "--format", "json"]);
    let shown = json(&run);
    let ignored: Vec<&serde_json::Value> = shown["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["used"] == false)
        .collect();
    assert_eq!(ignored.len(), 1, "{shown}");
    assert!(ignored[0]["message"].as_str().unwrap().contains("no part `gone`"));

    // A file that matches everything is ok, and so is no file.
    let dir = project(
        "sch-check-ok",
        &many(),
        Some("version = 1\n\n[Many.sheets.b.parts.r1]\ndx = 1.27\ndy = 0\n"),
    );
    let run = netsmith(&dir, &["check"]);
    assert_eq!(code(&run), 0, "{}{}", stdout(&run), stderr(&run));
    let dir = project("sch-check-none", &many(), None);
    assert_eq!(code(&netsmith(&dir, &["check"])), 0);
}

#[test]
fn check_says_an_entry_the_drawing_could_not_honour_and_set_warns_of_it() {
    // Two sheets placed at one place: the second box would lie on the first.
    let dir = project("sch-overlap", &many(), None);
    let run = netsmith(&dir, &["set", "a", "--at", "0,0"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(stderr(&run).is_empty(), "{}", stderr(&run));
    let run = netsmith(&dir, &["set", "b", "--at", "0,0"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(
        stderr(&run).contains("would lie on the box of the sheet `a`"),
        "{}",
        stderr(&run)
    );
    let run = netsmith(&dir, &["check", "--format", "json"]);
    assert_eq!(code(&run), 1);
    let checked = json(&run);
    assert_eq!(checked["findings"][0]["kind"], "unusable");
    assert_eq!(checked["findings"][0]["line"], 7);
    let run = netsmith(&dir, &["show", "--format", "json"]);
    let shown = json(&run);
    let b = shown["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "b")
        .unwrap();
    assert_eq!(b["placed"], false, "{shown}");
}

#[test]
fn arrange_keeps_the_arrangement_and_reset_drops_the_places() {
    let dir = project("sch-arrange", &many(), None);
    let run = netsmith(&dir, &["arrange", "--format", "json"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let arranged = json(&run);
    assert_eq!(arranged["written"], false);
    assert!(!dir.join("main.sch.toml").exists());
    let sheets = arranged["sheets"].as_array().unwrap();
    assert_eq!(sheets.len(), 9);
    assert!(sheets.iter().all(|s| s["source"] == "arranged"));
    let places = |value: &serde_json::Value| -> Vec<(String, serde_json::Value)> {
        value["sheets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| (s["name"].as_str().unwrap().to_string(), s["at"].clone()))
            .collect()
    };
    let by_connections = places(&arranged);

    // Keep it: an `at` for each sheet, and the drawing is the same.
    let run = netsmith(&dir, &["arrange", "--write", "--format", "json"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert_eq!(json(&run)["written"], true);
    let text = file(&dir);
    assert_eq!(text.matches("\nat = [").count(), 9, "{text}");
    assert_eq!(netsmith(&dir, &["check"]).status.code(), Some(0));
    let run = netsmith(&dir, &["arrange", "--format", "json"]);
    let kept = json(&run);
    assert_eq!(places(&kept), by_connections);
    assert!(kept["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["source"] == "file"));
    // Written again, the file is as it was.
    let run = netsmith(&dir, &["arrange", "--write", "--format", "json"]);
    assert_eq!(json(&run)["written"], false);
    assert_eq!(file(&dir), text);

    // A sheet moved by hand is kept, and the others are set out round it.
    let run = netsmith(&dir, &["set", "a", "--at", "381,0"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let run = netsmith(&dir, &["arrange", "--format", "json"]);
    let moved = places(&json(&run));
    assert_eq!(moved[0], ("a".to_string(), serde_json::json!([381.0, 0.0])));
    // `--reset` drops the places and gives the arrangement by connections again, written too.
    let run = netsmith(&dir, &["arrange", "--reset", "--write", "--format", "json"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let again = json(&run);
    assert_eq!(again["reset"], true);
    assert_eq!(places(&again), by_connections);
    assert_eq!(file(&dir), text);
}

#[test]
fn a_board_drawn_as_one_sheet_has_nothing_to_arrange_and_no_place_for_a_sheet() {
    let dir = project("sch-small", SMALL, None);
    let run = netsmith(&dir, &["arrange"]);
    assert_eq!(code(&run), 1);
    assert!(
        stderr(&run).contains("is drawn as one sheet"),
        "{}",
        stderr(&run)
    );
    let run = netsmith(&dir, &["show"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(stdout(&run).contains("drawn as one sheet"), "{}", stdout(&run));
    assert!(stdout(&run).contains("Small (the board's own): 2 parts"));
    // A part's entry is written and used; a place for the one sheet is written and said.
    let run = netsmith(&dir, &["set", "r1", "--by", "2.54,0"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let run = netsmith(&dir, &["set", "Small", "--at", "0,0"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(
        stderr(&run).contains("the board is drawn as one sheet"),
        "{}",
        stderr(&run)
    );
    assert_eq!(code(&netsmith(&dir, &["check"])), 1);
}
