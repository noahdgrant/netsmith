//! The track-drop guideline alone (issue #1540): `board nets` lists each power net's drop on the
//! copper the board has, and `board drc` notes a net over the budget with the message `board
//! fab` warns with, for the same copper.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

/// 2 A from a 3.3 V supply's header to a load's header, 55 mm apart: 1.7 % of 3.3 V.
const FEED: &str = "\
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Feed {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .load: Rail {
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
        .current &= 0A..2A
    }
    .source: Rail {
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
        .voltage &= 3.3V
        .current &= -2A..0A
    }
    .supply: Header1x2
    .out: Header1x2
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}
";

const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Feed {
    case {
        .outline: Outline {
            .width &= 80mm
            .height &= 20mm
        }
    }
}
";

const LAYOUT: &str = "\
version = 1

[Feed.parts.supply]
at = [10.0, 10.0]

[Feed.parts.out]
at = [65.0, 10.0]
";

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

/// The feed board in a folder of its own, its rail routed into the routes file beside the
/// source, so that every command reads the same copper.
fn routed(name: &str) -> TempDir {
    let dir = TempDir::new(name);
    fs::write(dir.join("feed.nsm"), FEED).unwrap();
    fs::write(dir.join("feed.physical.nsm"), PHYSICAL).unwrap();
    fs::write(dir.join("feed.pcb.toml"), LAYOUT).unwrap();
    ok(&netsmith(&dir, &["routes", "route", "3V3", "--file", "feed.nsm"]));
    assert!(dir.join("feed.routes.toml").exists());
    dir
}

fn ok(run: &Output) -> String {
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(run.status.success(), "{said}");
    said
}

/// The message after `track drop guideline: ` on the line of `text` that has it.
fn message(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| l.contains("track drop guideline: "))
        .unwrap_or_else(|| panic!("no track drop in:\n{text}"));
    line.split_once("track drop guideline: ").unwrap().1.to_string()
}

#[test]
fn board_nets_and_board_drc_measure_the_drop_fab_warns_of() {
    let dir = routed("track-drop-alone");

    // `board nets` lists the drop, in text and JSON, over the budget.
    let said = ok(&netsmith(&dir, &["board", "nets", "feed.nsm"]));
    let line = said
        .lines()
        .find(|l| l.trim_start().starts_with("3V3: "))
        .unwrap_or_else(|| panic!("no drop line in:\n{said}"));
    assert!(said.contains("track drop (the budget is 1% of the rail's voltage):"), "{said}");
    assert!(line.contains(" mV of 33 mV (") && line.contains(", over: "), "{line}");
    let run = netsmith(&dir, &["board", "nets", "feed.nsm", "--format", "json"]);
    ok(&run);
    let json: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let drops = json["track_drops"].as_array().unwrap();
    let rail = drops.iter().find(|d| d["net"] == "3V3").unwrap();
    println!("{rail}");
    assert_eq!(rail["over"], true, "{rail}");
    assert_eq!(rail["whole_current"], true, "{rail}");
    assert!((rail["current"].as_f64().unwrap() - 2.0).abs() < 1e-9, "{rail}");
    assert!((rail["allowed"].as_f64().unwrap() - 0.033).abs() < 1e-9, "{rail}");
    let drop = rail["drop"].as_f64().unwrap();
    assert!(drop > 0.033 && rail["share"].as_f64().unwrap() > 1.0, "{rail}");
    assert!(rail["width_for"].as_f64().unwrap() > 0.98, "{rail}");
    // The path's pieces, the one that drops most first, each with the width it needs (issue
    // #1610).
    let path = rail["path"].as_array().unwrap();
    assert!(!path.is_empty(), "{rail}");
    let drops_of: Vec<f64> = path.iter().map(|p| p["drop"].as_f64().unwrap()).collect();
    assert!(drops_of.windows(2).all(|w| w[0] >= w[1]), "{rail}");
    let summed: f64 = drops_of.iter().sum();
    assert!((summed - drop).abs() < 1e-9, "{rail}");
    assert!(path[0]["needs"].as_f64().unwrap() > path[0]["width"].as_f64().unwrap(), "{rail}");
    assert!(said.contains("    #") && said.contains(" mV ("), "{said}");
    // A ground has no voltage of its own.
    assert!(!drops.iter().any(|d| d["net"] == "GND"), "{drops:?}");

    // `board drc` notes it with fab's message, and it does not fail the check for it.
    let drc = netsmith(&dir, &["board", "drc", "feed.nsm"]);
    let checked = format!(
        "{}{}",
        String::from_utf8_lossy(&drc.stdout),
        String::from_utf8_lossy(&drc.stderr)
    );
    let noted = message(&checked);
    assert!(
        checked.contains("Feed: note: track drop guideline: net `3V3` drops up to "),
        "{checked}"
    );
    let run = netsmith(&dir, &["board", "drc", "feed.nsm", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let over = json["boards"][0]["track_drops"].as_array().unwrap();
    assert_eq!(over.len(), 1, "{json}");
    assert_eq!(over[0]["drop"].as_f64(), Some(drop), "{json}");
    assert_eq!(over[0]["path"], rail["path"], "{json}");
    assert!(checked.contains("\n  #1 track ") || checked.contains("\n  #2 track "), "{checked}");

    // `board fab` on the same copper warns with the same message.
    let fab = netsmith(
        &dir,
        &["board", "fab", "feed.nsm", "--reuse-routes", "--out", "out"],
    );
    let made = ok(&fab);
    let notes = fs::read_to_string(dir.join("out").join("Feed").join("fab-notes.txt"))
        .unwrap_or_else(|e| panic!("{e}: {made}"));
    assert_eq!(message(&notes), noted, "fab:\n{notes}\ndrc:\n{checked}");
}

/// What a run printed on stdout, with its exit code.
fn printed(dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let run = netsmith(dir, args);
    (
        run.status.code(),
        String::from_utf8_lossy(&run.stdout).into_owned(),
    )
}

fn json_of(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// `--at-current` (issue #1694): the rail at half its design current shows both drops with
/// their shares of the budget, and nothing else of the output changes.
#[test]
fn a_rail_at_half_its_current_shows_both_drops_and_the_rest_is_unchanged() {
    let dir = routed("track-drop-at-current");
    for command in ["nets", "drc"] {
        let plain = ["board", command, "feed.nsm"];
        let half = ["board", command, "feed.nsm", "--at-current", "3V3=1A"];

        // In text, the flag adds its lines and changes no other.
        let (code, before) = printed(&dir, &plain);
        let (with_code, after) = printed(&dir, &half);
        assert_eq!(code, with_code, "{after}");
        assert!(!before.contains("another current"), "{before}");
        let added: Vec<&str> = after
            .lines()
            .filter(|l| !before.lines().any(|b| b == *l))
            .collect();
        let line = added
            .iter()
            .find(|l| l.contains("3V3: design 2 A: "))
            .unwrap_or_else(|| panic!("no line at 1 A in:\n{after}"));
        println!("{line}");
        assert!(line.contains("; at 1 A: "), "{line}");
        assert_eq!(line.matches("% of the budget").count(), 2, "{line}");
        assert!(line.contains("; budget 33 mV, J"), "{line}");
        assert!(added.iter().all(|l| l.contains("another current") || l == line), "{added:?}");
        let rest: Vec<&str> = after.lines().filter(|l| !added.contains(l)).collect();
        assert_eq!(rest, before.lines().collect::<Vec<_>>());

        // In JSON, one list more, and the rest as it was.
        let (_, before) = printed(&dir, &[&plain[..], &["--format", "json"]].concat());
        let (_, after) = printed(&dir, &[&half[..], &["--format", "json"]].concat());
        let before = json_of(&before);
        let mut after = json_of(&after);
        let holder = if command == "nets" {
            &mut after
        } else {
            &mut after["boards"][0]
        };
        let rows = holder
            .as_object_mut()
            .unwrap()
            .remove("track_drops_at_current")
            .expect("the list of the flag");
        assert_eq!(after, before);
        let rows = rows.as_array().unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        let row = &rows[0];
        println!("{row}");
        assert_eq!(row["net"], "3V3", "{row}");
        let number = |v: &serde_json::Value| v.as_f64().unwrap();
        let close = |a: f64, b: f64| assert!((a - b).abs() < 1e-9, "{a} is not {b}: {row}");
        let (design, alternative) = (&row["design"], &row["alternative"]);
        close(number(&row["allowed"]), 0.033);
        close(number(&design["current"]), 2.0);
        close(number(&alternative["current"]), 1.0);
        close(number(&alternative["drop"]), number(&design["drop"]) / 2.0);
        close(number(&alternative["share"]), number(&design["share"]) / 2.0);
        close(number(&design["share"]), number(&design["drop"]) / 0.033);
        assert_eq!(design["over"], true, "{row}");
        assert_eq!(alternative["over"], false, "{row}");
    }

    // A net that has no drop to give, and one the board does not have, stop the command.
    for (command, flag, said) in [
        ("nets", "GND=1", "net `GND` has no track drop"),
        ("drc", "GND=1", "net `GND` has no track drop"),
        ("nets", "5V=1", "the board has no net `5V`"),
        ("drc", "5V=1", "the board has no net `5V`"),
    ] {
        let run = netsmith(&dir, &["board", command, "feed.nsm", "--at-current", flag]);
        let err = String::from_utf8_lossy(&run.stderr);
        assert_eq!(run.status.code(), Some(2), "{command} {flag}: {err}");
        assert!(err.contains(said), "{command} {flag}: {err}");
    }
}
