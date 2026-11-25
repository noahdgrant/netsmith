//! `layout fit`, `layout move` and `layout set` judge the height limits the project states as
//! `board fab` judges them (issue #1727), and `layout fit --near PART.PAD --count N` gives the
//! nearest legal places to a pad, ranked (issue #1728).
//!
//! The board is invented: four resistors in a 30 by 20 mm outline, one of them (`tall`) 2 mm
//! tall, under a lid that leaves 1.2 mm over the left half of the top face.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;
use serde_json::Value;

const BOARD: &str = "\
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Lidded {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .rail: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .r1: Resistor { .resistance &= 15kohm +/- 1% }
    .r2: Resistor { .resistance &= 15kohm +/- 1% }
    .r3: Resistor { .resistance &= 15kohm +/- 1% }
    .tall: Resistor { .resistance &= 15kohm +/- 1% }
    .rail.pos ~> .r1 ~> .rail.neg
    .rail.pos ~> .r2 ~> .rail.neg
    .rail.pos ~> .r3 ~> .rail.neg
    .rail.pos ~> .tall ~> .rail.neg
    .tall.height &= 2mm
}
";

const PHYSICAL: &str = "\
import std::pcb::{Keepout, Outline}

physical for Lidded {
    case {
        .outline: Outline {
            .width &= 30mm
            .height &= 20mm
        }
    }
    // The lid leaves 1.2 mm over the left half of the top face.
    .lid: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 15mm
        .height &= 20mm
        .layers &= \"top\"
        .max_height &= 1.2mm
    }
    .tall.place.side &= \"top\"
}
";

/// Every part locked: `r1` under the lid, `r2` and `r3` beside it, `tall` on the right half.
const LAYOUT: &str = "\
version = 1

[Lidded]
seed = 0

[Lidded.parts.r1]
at = [5.0, 10.0]
rotation = 0.0
side = \"top\"
lock = true
source = \"hand\"

[Lidded.parts.r2]
at = [25.0, 4.0]
rotation = 0.0
side = \"top\"
lock = true
source = \"hand\"

[Lidded.parts.r3]
at = [25.0, 16.0]
rotation = 0.0
side = \"top\"
lock = true
source = \"hand\"

[Lidded.parts.tall]
at = [17.0, 10.0]
rotation = 0.0
side = \"top\"
lock = true
source = \"hand\"
";

fn project(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"lidded\"\ntop_level = [\"main.nsm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("main.nsm"), BOARD).unwrap();
    fs::write(dir.join("main.physical.nsm"), PHYSICAL).unwrap();
    fs::write(dir.join("main.pcb.toml"), LAYOUT).unwrap();
    dir
}

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn said(run: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

fn json(dir: &Path, args: &[&str]) -> Value {
    let run = netsmith(dir, args);
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| panic!("{args:?}: {e}: {}", said(&run)))
}

/// `fab`'s sentence for `tall` 2 mm tall under the lid.
const TOO_TALL: &str = "`tall` (R4) stands 2.00 mm tall on the top face, and the keepout `lid` allows 1.20 mm there (0.80 mm too tall)";

/// Writes `pose` (a place of `layout fit`) as the text says to, and gives `layout check`'s
/// findings about `tall`, with the file put back as it was.
fn check_after_set(dir: &Path, pose: &Value) -> Vec<Value> {
    let before = fs::read_to_string(dir.join("main.pcb.toml")).unwrap();
    let at = format!(
        "{},{}",
        pose["at"][0].as_f64().unwrap(),
        pose["at"][1].as_f64().unwrap()
    );
    let turn = pose["turn"].as_f64().unwrap().to_string();
    let side = pose["side"].as_str().unwrap();
    let set = netsmith(
        dir,
        &["layout", "set", "tall", "--at", &at, "--turn", &turn, "--side", side, "--lock"],
    );
    assert!(set.status.success(), "{}", said(&set));
    let doc = json(dir, &["layout", "check", "--format", "json"]);
    fs::write(dir.join("main.pcb.toml"), before).unwrap();
    doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["path"] == "tall" && f["severity"] != "note")
        .cloned()
        .collect()
}

#[test]
fn a_part_too_tall_for_a_height_limit_is_offered_no_place_under_it_and_is_told_why() {
    let dir = project("heights-fit");
    // Asked for the nearest place to a point under the lid, every place it offers is clear of
    // it: the part's body is right of x = 15 mm.
    let doc = json(
        &dir,
        &["layout", "fit", "tall", "--near", "3,10", "--as-laid-out", "--format", "json"],
    );
    let offered: Vec<&Value> = doc["poses"].as_array().unwrap().iter().filter(|p| p["fits"] == true).collect();
    assert!(!offered.is_empty(), "{doc}");
    for pose in &offered {
        assert!(pose["distance"].as_f64().unwrap() > 11.0, "{pose}");
        assert_eq!(check_after_set(&dir, pose), Vec::<Value>::new(), "{pose}");
        // The places nearer that were refused, by why, with `fab`'s sentence for each.
        let refusals = pose["refusals"].as_array().unwrap();
        let tall = refusals
            .iter()
            .find(|r| r["kind"] == "too_tall")
            .unwrap_or_else(|| panic!("{pose}"));
        assert!(tall["places"].as_u64().unwrap() > 0, "{pose}");
        assert!(tall["text"].as_str().unwrap().contains(TOO_TALL), "{pose}");
    }
    let run = netsmith(&dir, &["layout", "fit", "tall", "--near", "3,10", "--as-laid-out"]);
    assert!(run.status.success(), "{}", said(&run));
    let text = said(&run);
    assert!(text.contains("too tall"), "{text}");
    assert!(text.contains(TOO_TALL), "{text}");
}

#[test]
fn a_move_under_a_height_limit_is_refused_and_a_set_there_warns_in_fabs_words() {
    let dir = project("heights-move");
    let before = fs::read_to_string(dir.join("main.pcb.toml")).unwrap();
    // Left by 10 mm: under the lid.
    let run = netsmith(&dir, &["layout", "move", "tall", "-10,0"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    assert!(said(&run).contains(TOO_TALL), "{}", said(&run));
    assert_eq!(fs::read_to_string(dir.join("main.pcb.toml")).unwrap(), before);
    // A turn there too.
    let run = netsmith(&dir, &["layout", "move", "tall", "-10,0", "--turn", "90"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    assert!(said(&run).contains("stands 2.00 mm tall on the top face, and the keepout `lid` allows 1.20 mm there"), "{}", said(&run));
    // Set there, it is written, with the same sentence as a warning.
    let run = netsmith(&dir, &["layout", "set", "tall", "--at", "7,10"]);
    assert!(run.status.success(), "{}", said(&run));
    let warned = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(warned.contains(&format!("warning: {TOO_TALL}")), "{warned}");
    // And `fab` says it in the same words.
    let fab = netsmith(&dir, &["board", "fab", "main.nsm", "--out", "fab"]);
    assert_ne!(fab.status.code(), Some(0), "{}", said(&fab));
    assert!(said(&fab).contains(TOO_TALL), "{}", said(&fab));
}

#[test]
fn fit_near_a_pad_gives_ranked_legal_places_with_their_distance_and_turn() {
    let dir = project("heights-near-pad");
    let doc = json(
        &dir,
        &[
            "layout", "fit", "tall", "--near", "R1.1", "--count", "4", "--as-laid-out", "--format", "json",
        ],
    );
    assert_eq!(doc["near"]["pad"], "R1.1", "{doc}");
    let ranked = doc["ranked"].as_array().unwrap();
    assert_eq!(ranked.len(), 4, "{doc}");
    let mut last = 0.0;
    for c in ranked {
        let d = c["distance"].as_f64().unwrap();
        assert!(d >= last, "not nearest first: {doc}");
        last = d;
        assert_eq!(c["fits"], true, "{c}");
        assert!(c["turn"].is_number() && c["side"] == "top", "{c}");
        // None under the lid, none on a courtyard: `layout check` finds nothing about it.
        assert_eq!(check_after_set(&dir, c), Vec::<Value>::new(), "{c}");
    }
    // Nothing is written.
    assert_eq!(fs::read_to_string(dir.join("main.pcb.toml")).unwrap(), LAYOUT);
    // The text lists them, nearest first, each with the line that writes it.
    let run = netsmith(&dir, &["layout", "fit", "tall", "--near", "R1.1", "--count", "2", "--as-laid-out"]);
    assert!(run.status.success(), "{}", said(&run));
    let text = String::from_utf8_lossy(&run.stdout).into_owned();
    let lines: Vec<&str> = text.lines().filter(|l| l.trim_start().starts_with("place ")).collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(lines.iter().all(|l| l.contains("netsmith layout set tall --at")), "{text}");
    // A pad the part does not have is refused with its pads.
    let run = netsmith(&dir, &["layout", "fit", "tall", "--near", "R1.9", "--as-laid-out"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    assert!(said(&run).contains("has no pad `9`"), "{}", said(&run));
}
