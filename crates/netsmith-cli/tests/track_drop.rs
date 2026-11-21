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
    let dir = TempDir::new("track-drop-alone");
    fs::write(dir.join("feed.nsm"), FEED).unwrap();
    fs::write(dir.join("feed.physical.nsm"), PHYSICAL).unwrap();
    fs::write(dir.join("feed.pcb.toml"), LAYOUT).unwrap();
    // Route the rail into the routes file beside the source, so that every command reads the
    // same copper.
    ok(&netsmith(&dir, &["routes", "route", "3V3", "--file", "feed.nsm"]));
    assert!(dir.join("feed.routes.toml").exists());

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
