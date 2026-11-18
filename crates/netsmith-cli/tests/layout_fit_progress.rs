//! `netsmith layout fit` says how far the placing has got and, when the layout file has a place
//! for every part, that `--as-laid-out` answers at once (issue #1282). The board is the invented
//! one of `layout.rs`: a 40 by 24 mm board of a header, an LED and a resistor.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;
use serde_json::Value;

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
}
";

/// The note a placed `layout fit` starts with when `--as-laid-out` would answer at once.
const NOTE: &str = "note: the layout file has a place for every part";

fn project(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"widget\"\ntop_level = [\"main.nsm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("main.nsm"), BOARD).unwrap();
    fs::write(dir.join("main.physical.nsm"), PHYSICAL).unwrap();
    dir
}

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    let mut full = vec!["layout"];
    full.extend_from_slice(args);
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(full)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn stderr(run: &Output) -> String {
    String::from_utf8_lossy(&run.stderr).into_owned()
}

/// The lines of stderr that are JSON objects.
fn events(run: &Output) -> Vec<Value> {
    stderr(run)
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[test]
fn a_placed_fit_says_how_far_it_has_got_as_json_lines() {
    // No layout file: the placer places every part.
    let dir = project("fit-progress-json");
    let run = netsmith(&dir, &["fit", "r1", "--progress", "json"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let events = events(&run);
    let placing: Vec<&Value> = events.iter().filter(|e| e["stage"] == "placing").collect();
    assert!(!placing.is_empty(), "{}", stderr(&run));
    for e in &placing {
        assert_eq!(e["board"], "Widget", "{e}");
        assert!(e["unit"].is_string() && e["elapsedMs"].is_u64(), "{e}");
        assert!(
            e["message"].as_str().is_some_and(|m| m.starts_with("Widget: placing: ")),
            "{e}"
        );
    }
    // Every line of the placing is JSON: no text line is mixed in.
    assert!(
        stderr(&run).lines().all(|l| serde_json::from_str::<Value>(l).is_ok()),
        "{}",
        stderr(&run)
    );
    // The text run says the same in text.
    let run = netsmith(&dir, &["fit", "r1"]);
    assert!(stderr(&run).contains("Widget: placing: "), "{}", stderr(&run));
    // A part with no entry: there is nothing to say about `--as-laid-out`.
    assert!(!stderr(&run).contains(NOTE), "{}", stderr(&run));
}

#[test]
fn with_json_on_stdout_the_progress_is_printed_only_when_asked_for_as_json() {
    let dir = project("fit-progress-format");
    let run = netsmith(&dir, &["fit", "r1", "--format", "json"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    serde_json::from_slice::<Value>(&run.stdout).expect("one JSON document on stdout");
    assert!(!stderr(&run).contains("placing"), "{}", stderr(&run));
    let run = netsmith(&dir, &["fit", "r1", "--format", "json", "--progress", "json"]);
    serde_json::from_slice::<Value>(&run.stdout).expect("one JSON document on stdout");
    assert!(events(&run).iter().any(|e| e["stage"] == "placing"), "{}", stderr(&run));
}

#[test]
fn a_layout_file_with_a_place_for_every_part_says_as_laid_out_answers_at_once() {
    let dir = project("fit-progress-note");
    let capture = netsmith(&dir, &["capture"]);
    assert!(capture.status.success(), "{}", stderr(&capture));
    // Placed: the note comes before the placer's first line, and the board is still placed.
    let run = netsmith(&dir, &["fit", "r1"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let said = stderr(&run);
    let note = said.find(NOTE).unwrap_or_else(|| panic!("{said}"));
    assert!(said.contains("`--as-laid-out` answers at once"), "{said}");
    let placing = said.find("Widget: placing: ").unwrap_or_else(|| panic!("{said}"));
    assert!(note < placing, "{said}");
    // The note is there with JSON on stdout too, and as text beside JSON lines.
    let run = netsmith(&dir, &["fit", "r1", "--format", "json"]);
    assert!(stderr(&run).contains(NOTE), "{}", stderr(&run));
    serde_json::from_slice::<Value>(&run.stdout).expect("one JSON document on stdout");
    let run = netsmith(&dir, &["fit", "r1", "--progress", "json"]);
    assert!(stderr(&run).contains(NOTE), "{}", stderr(&run));
    // With the option itself it is not said, and nothing is placed.
    let run = netsmith(&dir, &["fit", "r1", "--as-laid-out"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(!stderr(&run).contains(NOTE), "{}", stderr(&run));
    assert!(!stderr(&run).contains("placing"), "{}", stderr(&run));
}

#[test]
fn a_part_without_an_entry_keeps_the_default_and_says_nothing_about_as_laid_out() {
    let dir = project("fit-progress-partial");
    let capture = netsmith(&dir, &["capture"]);
    assert!(capture.status.success(), "{}", stderr(&capture));
    // Take one part's entry out of the file: the placer is the only way to know its place.
    let file = dir.join("main.pcb.toml");
    let text = fs::read_to_string(&file).unwrap();
    let kept: String = text
        .split("\n[")
        .filter(|table| !table.starts_with("Widget.parts.r1]"))
        .collect::<Vec<_>>()
        .join("\n[");
    assert_ne!(kept, text, "r1 had an entry to take out:\n{text}");
    fs::write(&file, kept).unwrap();
    let run = netsmith(&dir, &["fit", "r1"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(!stderr(&run).contains(NOTE), "{}", stderr(&run));
    assert!(stderr(&run).contains("Widget: placing: "), "{}", stderr(&run));
}
