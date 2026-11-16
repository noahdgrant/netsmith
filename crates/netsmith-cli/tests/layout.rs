//! `netsmith layout` (issue #782): `show` says the entry of each part as the placer reads it, `set`
//! writes one entry and keeps the file's other tables, refusing a part the board does not have and
//! a turn that is not 0, 90, 180 or 270, and `check` says what the file has wrong against the
//! board, one line each with the line of the entry. The board is the invented case of
//! `constraints.rs`: a 40 by 24 mm board whose header must sit on the left edge, turned 90.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

const BOARD: &str = "\
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Widget {
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
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
";

const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Widget {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }

    .power_in.place.left &= 1.5mm +/- 0.1mm
    .power_in.place.rotation &= 90
    .led.place.right <= 38mm
}
";

/// A project of the board and its physical file, with `layout` as its layout file if there is one.
fn project(tag: &str, layout: Option<&str>) -> TempDir {
    let dir = TempDir::new(tag);
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"widget\"\ntop_level = [\"main.nsm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("main.nsm"), BOARD).unwrap();
    fs::write(dir.join("main.physical.nsm"), PHYSICAL).unwrap();
    if let Some(layout) = layout {
        fs::write(dir.join("main.pcb.toml"), layout).unwrap();
    }
    dir
}

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .arg("layout")
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

fn layout_file(dir: &Path) -> String {
    fs::read_to_string(dir.join("main.pcb.toml")).unwrap_or_default()
}

const HAND: &str = "\
version = 1

[Widget]
seed = 3

[Widget.parts.led]
at = [30.0, 12.0]
rotation = 0
side = \"top\"
lock = false
source = \"auto\"

[Widget.keepouts.antenna]
at = [30.0, 0.0]
";

#[test]
fn show_says_each_part_and_the_line_of_its_entry() {
    let dir = project("layout-show", Some(HAND));
    let run = netsmith(&dir, &["show"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.contains("led (D1)") || text.contains("led ("), "{text}");
    assert!(
        text.contains("at 30, 12 mm  turn 0  top  source auto  (line 6)"),
        "{text}"
    );
    assert!(
        text.contains("power_in") && text.contains("no entry: the placer places it"),
        "{text}"
    );

    let run = netsmith(&dir, &["show", "led", "--format", "json"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let json: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    assert_eq!(json["board"], "Widget");
    assert_eq!(json["exists"], true);
    let parts = json["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0]["path"], "led");
    assert_eq!(parts[0]["entry"]["at"], serde_json::json!([30.0, 12.0]));
    assert_eq!(parts[0]["entry"]["turn"], 0.0);
    assert_eq!(parts[0]["entry"]["side"], "top");
    assert_eq!(parts[0]["entry"]["lock"], false);
    assert_eq!(parts[0]["entry"]["source"], "auto");
    assert_eq!(parts[0]["entry"]["line"], 6);

    // A part with no entry has none in the data, and the file need not exist.
    let none = project("layout-show-none", None);
    let run = netsmith(&none, &["show", "r1", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    assert_eq!(json["exists"], false);
    assert!(json["parts"][0].get("entry").is_none());
}

#[test]
fn show_refuses_a_path_the_board_does_not_have_with_the_nearest_names() {
    let dir = project("layout-show-wrong", None);
    let run = netsmith(&dir, &["show", "power_n"]);
    assert_eq!(code(&run), 1);
    let said = stderr(&run);
    assert!(
        said.contains("has no part `power_n`") && said.contains("`power_in`"),
        "{said}"
    );
}

#[test]
fn set_writes_one_entry_and_keeps_the_other_tables() {
    let dir = project("layout-set", Some(HAND));
    let run = netsmith(
        &dir,
        &[
            "set", "power_in", "--at", "3,12", "--turn", "90", "--side", "bottom", "--lock",
        ],
    );
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(stdout(&run).starts_with("wrote the entry of `power_in`"), "{}", stdout(&run));
    let file = layout_file(&dir);
    assert!(file.contains("[Widget.parts.power_in]"), "{file}");
    assert!(file.contains("side = \"bottom\""), "{file}");
    assert!(file.contains("lock = true"), "{file}");
    assert!(file.contains("source = \"hand\""), "{file}");
    // The seed, the other part and the keepout are as they were.
    assert!(file.contains("seed = 3"), "{file}");
    assert!(file.contains("[Widget.parts.led]"), "{file}");
    assert!(file.contains("[Widget.keepouts.antenna]"), "{file}");

    // What `show` says is what was written.
    let run = netsmith(&dir, &["show", "power_in", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    let entry = &json["parts"][0]["entry"];
    assert_eq!(entry["at"], serde_json::json!([3.0, 12.0]));
    assert_eq!(entry["turn"], 90.0);
    assert_eq!(entry["side"], "bottom");
    assert_eq!(entry["lock"], true);

    // A change of one field keeps the others, and `--unlock` unlocks.
    let run = netsmith(&dir, &["set", "power_in", "--unlock", "--side", "top"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(stdout(&run).starts_with("changed the entry of `power_in`"));
    let run = netsmith(&dir, &["show", "power_in", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    let entry = &json["parts"][0]["entry"];
    assert_eq!(entry["at"], serde_json::json!([3.0, 12.0]));
    assert_eq!(entry["turn"], 90.0);
    assert_eq!(entry["side"], "top");
    assert_eq!(entry["lock"], false);
}

#[test]
fn set_makes_the_file_when_there_is_none() {
    let dir = project("layout-set-new", None);
    let run = netsmith(&dir, &["set", "r1", "--at", "10,5"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let file = layout_file(&dir);
    assert!(file.starts_with("version = 1\n"), "{file}");
    assert!(file.contains("[Widget.parts.r1]"), "{file}");
}

#[test]
fn set_refuses_what_is_wrong_and_leaves_the_file_as_it_was() {
    let dir = project("layout-set-refused", Some(HAND));
    let before = layout_file(&dir);

    // A part the board does not have: the nearest names are listed.
    let run = netsmith(&dir, &["set", "pwer_in", "--at", "1,1"]);
    assert_eq!(code(&run), 1);
    let said = stderr(&run);
    assert!(
        said.contains("has no part `pwer_in`") && said.contains("`power_in`"),
        "{said}"
    );

    // A turn that is not 0, 90, 180 or 270 is a usage mistake.
    let run = netsmith(&dir, &["set", "led", "--turn", "45"]);
    assert_eq!(code(&run), 2);
    assert!(stderr(&run).contains("0, 90, 180 or 270"), "{}", stderr(&run));

    // A part with no entry has no place to start from.
    let run = netsmith(&dir, &["set", "r1", "--turn", "90"]);
    assert_eq!(code(&run), 1);
    assert!(stderr(&run).contains("--at X,Y"), "{}", stderr(&run));

    // Nothing to change is a usage mistake too.
    assert_eq!(code(&netsmith(&dir, &["set", "led"])), 2);

    assert_eq!(layout_file(&dir), before);
}

#[test]
fn set_says_when_a_locked_entry_breaks_a_requirement() {
    let dir = project("layout-set-warns", None);
    let run = netsmith(&dir, &["set", "power_in", "--at", "30,12", "--lock"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let said = stderr(&run);
    assert!(
        said.contains("warning: `power_in`") && said.contains("is locked in the layout file"),
        "{said}"
    );
}

#[test]
fn check_says_each_thing_wrong_with_the_line_of_its_entry() {
    let file = "\
version = 1

[Widget.parts.ledd]
at = [1.0, 1.0]

[Widget.parts.r1]
at = [10.0, 5.0]
rotation = 45

[Widget.parts.power_in]
at = [30.0, 12.0]
rotation = 0
lock = true
";
    let dir = project("layout-check", Some(file));
    let run = netsmith(&dir, &["check"]);
    assert_eq!(code(&run), 1, "{}{}", stdout(&run), stderr(&run));
    let text = stdout(&run);
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines.iter().any(|l| l.contains("main.pcb.toml:3: error:")
            && l.contains("names no part")
            && l.contains("`led`")),
        "{text}"
    );
    assert!(
        lines.iter().any(|l| l.contains("main.pcb.toml:6: error:")
            && l.contains("`r1`")
            && l.contains("0, 90, 180 or 270")),
        "{text}"
    );
    assert!(
        lines.iter().any(|l| l.contains("main.pcb.toml:10: error:")
            && l.contains("is locked in the layout file")
            && l.contains("`power_in`")),
        "{text}"
    );
    // The part with no entry is a note without a line.
    assert!(
        lines.iter().any(|l| l.contains("main.pcb.toml: note:")
            && l.contains("`led`")
            && l.contains("has no entry")),
        "{text}"
    );

    let run = netsmith(&dir, &["check", "--format", "json"]);
    assert_eq!(code(&run), 1);
    let json: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    assert_eq!(json["ok"], false);
    let found = json["findings"].as_array().unwrap();
    let kinds: Vec<(&str, Option<u64>)> = found
        .iter()
        .map(|f| (f["kind"].as_str().unwrap(), f["line"].as_u64()))
        .collect();
    assert!(kinds.contains(&("no_part", Some(3))), "{kinds:?}");
    assert!(kinds.contains(&("bad_turn", Some(6))), "{kinds:?}");
    assert!(kinds.contains(&("requirement", Some(10))), "{kinds:?}");
    assert!(kinds.contains(&("no_entry", None)), "{kinds:?}");
}

#[test]
fn check_passes_with_notes_for_parts_the_placer_places() {
    // The header locked where its requirements put it: left edge 1.5 mm in, turned 90.
    let dir = project("layout-check-ok", None);
    let run = netsmith(&dir, &["check"]);
    assert_eq!(code(&run), 0, "{}{}", stdout(&run), stderr(&run));
    assert!(stdout(&run).contains("has no entry"), "{}", stdout(&run));

    let file = "version = 1\n\n[Widget.parts.led]\nat = [30.0, 12.0]\n";
    let dir = project("layout-check-ok-entry", Some(file));
    let run = netsmith(&dir, &["check", "--format", "json"]);
    assert_eq!(code(&run), 0, "{}{}", stdout(&run), stderr(&run));
    let json: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    assert_eq!(json["ok"], true);
}
