//! Requirements on where parts go (ADR 0045), from the source to the package: the placer
//! keeps to them, the layout file keeps the solution, the check measures them on the board
//! file with every number, and a violation stops `fab` unless the project says `warn`.
//! The numbers are an invented case drawing.

use netsmith_testkit::TempDir;
use std::fs;

use netsmith_fab::KicadCliExe;

/// A 40 by 24 mm board whose supply header must sit on the left edge, turned, centred along
/// it, and whose LED must stay inside the right of the board.
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

/// The case drawing of `BOARD`, in its physical file (ADR 0048).
const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Widget {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }

    // Case drawing, slot A: the header's body is 1.5 mm in from the left edge, centred.
    .power_in.place.left &= 1.5mm +/- 0.1mm
    .power_in.place.y >= .outline.height / 2 - 0.1mm
    .power_in.place.y <= .outline.height / 2 + 0.1mm
    .power_in.place.rotation &= 90
    .led.place.right <= 38mm
    .led.place.top >= 3mm
}
";

fn out(name: &str) -> TempDir {
    TempDir::unused(&format!("constraints-test-{name}"))
}

fn have_libraries() -> bool {
    KicadCliExe::locate_runnable().is_some()
}

/// A layout file that moves the header by hand, without locking it.
const MOVED: &str = "version = 1\n[Widget.parts.power_in]\nat = [30.0, 5.0]\n";

/// A project whose manifest says `manifest` (a line, or none) and whose layout file moves the
/// header out of its window, run with `args`.
fn run_fab(name: &str, manifest: &str, args: &[&str]) -> std::process::Output {
    let root = out(name);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"widget\"\ntop_level = [\"src/main.nsm\"]\n{manifest}\n"),
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), BOARD).unwrap();
    fs::write(root.join("src/main.physical.nsm"), PHYSICAL).unwrap();
    fs::write(root.join("src/main.pcb.toml"), MOVED).unwrap();
    std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .arg("fab")
        .args(args)
        .args(["--out", "fab"])
        .current_dir(&root)
        .output()
        .unwrap()
}

#[test]
fn the_command_line_over_rules_the_manifest_both_ways() {
    if !have_libraries() {
        return;
    }
    let said = |run: &std::process::Output| String::from_utf8_lossy(&run.stderr).to_string();
    // The manifest says nothing: an error, as by default.
    let run = run_fab("cli-default", "", &[]);
    assert!(!run.status.success(), "{}", said(&run));
    assert!(said(&run).contains("error [constraint] constraint power_in.place.left"));
    // `--constraints warn` is a warning for one run.
    let run = run_fab("cli-warn", "", &["--constraints", "warn"]);
    assert!(run.status.success(), "{}", said(&run));
    assert!(said(&run).contains("warning [constraint] constraint power_in.place.left"));
    // The manifest says warn: a warning; `--constraints error` makes it an error again.
    let warn = "constraints = \"warn\"";
    let run = run_fab("manifest-warn", warn, &[]);
    assert!(run.status.success(), "{}", said(&run));
    let run = run_fab("manifest-warn-forced", warn, &["--constraints", "error"]);
    assert!(!run.status.success(), "{}", said(&run));
    assert!(said(&run).contains("error [constraint] constraint power_in.place.left"));
    // The manifest says error: `--constraints warn` wins.
    let run = run_fab(
        "manifest-error",
        "constraints = \"error\"",
        &["--constraints", "warn"],
    );
    assert!(run.status.success(), "{}", said(&run));
    // `--allow-drc-violations` does not help.
    let run = run_fab("cli-allow", "", &["--allow-drc-violations"]);
    assert!(!run.status.success(), "{}", said(&run));
    assert!(said(&run).contains("does not waive them"));
}

/// The requirement ids of the findings of a `--format json` run that carry requirements.
fn requirements_of(run: &std::process::Output) -> Vec<serde_json::Value> {
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}",
            String::from_utf8_lossy(&run.stdout)
        )
    });
    doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["requirements"].as_array().cloned().unwrap_or_default())
        .collect()
}

#[test]
fn the_json_report_has_each_requirement_not_met_with_its_numbers() {
    if !have_libraries() {
        return;
    }
    // An error: the finding of the failed run lists the requirements (issue #816).
    let run = run_fab("json-error", "", &["--format", "json"]);
    assert!(!run.status.success());
    let all = requirements_of(&run);
    let left = all
        .iter()
        .find(|r| r["id"] == "power_in.place.left")
        .unwrap_or_else(|| panic!("{all:?}"));
    assert_eq!(left["measure"], "left");
    assert_eq!(left["unit"], "mm");
    assert_eq!(left["parts"], serde_json::json!(["J1"]));
    assert_eq!(left["min"], 1.4);
    assert_eq!(left["max"], 1.6);
    assert!(left["measured"].as_f64().unwrap() > 20.0, "{left}");
    assert!(left["over"].as_f64().unwrap() > 20.0, "{left}");
    assert!(
        left["source"].as_str().unwrap().contains("main.physical.nsm:"),
        "{left}"
    );
    // A warning: each is a finding of its own, with its numbers.
    let run = run_fab("json-warn", "", &["--format", "json", "--constraints", "warn"]);
    assert!(run.status.success());
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let warning = doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| {
            f["severity"] == "warning"
                && f["requirements"]
                    .as_array()
                    .is_some_and(|r| r.iter().any(|r| r["id"] == "power_in.place.left"))
        })
        .unwrap_or_else(|| panic!("{doc}"));
    assert_eq!(warning["code"], "fab");
    assert!(
        warning["message"]
            .as_str()
            .unwrap()
            .contains("constraint power_in.place.left"),
        "{warning}"
    );
    // And it is not also a line of the notes.
    let notes = doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| {
            f["severity"] == "note"
                && f["message"]
                    .as_str()
                    .unwrap()
                    .contains("constraint power_in.place.left")
        })
        .count();
    assert_eq!(notes, 0, "{doc}");
}

/// A case drawing of `BOARD` with one line, and a layout file that locks the LED and the
/// resistor over each other.
const LOCKED_PHYSICAL: &str = "\
import std::pcb::Outline

physical for Widget {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }
    .r1.place.x <= 30mm
}
";
const LOCKED_OVER: &str = "version = 1\n[Widget.parts.r1]\nat = [10.0, 10.0]\nlock = true\n[Widget.parts.led]\nat = [10.5, 10.2]\nlock = true\n";

/// A project of `BOARD` with `physical` and the layout file `layout`, run with `args`.
fn run_in(name: &str, physical: &str, layout: &str, args: &[&str]) -> std::process::Output {
    let root = out(name);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"widget\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), BOARD).unwrap();
    fs::write(root.join("src/main.physical.nsm"), physical).unwrap();
    fs::write(root.join("src/main.pcb.toml"), layout).unwrap();
    std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(&root)
        .output()
        .unwrap()
}

#[test]
fn locks_that_overlap_are_conflicts_and_faults_as_data_in_fab_and_board3d() {
    // Issue #779: an agent that moves parts to clear a conflict reads the parts, the boxes
    // and the locks, not the sentences.
    let run = run_in(
        "locked-over-fab",
        LOCKED_PHYSICAL,
        LOCKED_OVER,
        &["fab", "--format", "json", "--out", "fab"],
    );
    assert!(!run.status.success());
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&run.stdout)));
    let finding = &doc["findings"][0];
    let conflict = finding["conflicts"]
        .as_array()
        .and_then(|c| c.iter().find(|c| c["kind"] == "overlap"))
        .unwrap_or_else(|| panic!("{finding}"));
    let locks: Vec<&str> = conflict["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["kind"] == "lock")
        .map(|l| l["part"]["refdes"].as_str().unwrap())
        .collect();
    assert_eq!(locks, ["D1", "R1"], "{conflict}");
    assert!(conflict["overlap"]["width"].as_f64().unwrap() > 0.0, "{conflict}");
    assert!(conflict["parts"][0]["courtyard"]["x_min"].is_number(), "{conflict}");
    let fault = finding["placement"]
        .as_array()
        .and_then(|p| {
            p.iter().find(|f| {
                f["kind"] == "overlap"
                    && f["parts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|p| p["refdes"] == "R1")
            })
        })
        .unwrap_or_else(|| panic!("{finding}"));
    assert!(fault["area"].as_f64().unwrap() > 0.0, "{fault}");

    // `board3d --json` has the same lists for the board it places.
    let run = run_in(
        "locked-over-board3d",
        LOCKED_PHYSICAL,
        LOCKED_OVER,
        &["board3d", "--json"],
    );
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&run.stderr)));
    assert!(
        doc["conflicts"].as_array().unwrap().iter().any(|c| c["kind"] == "overlap"),
        "{}",
        doc["conflicts"]
    );
    assert!(
        doc["placement"].as_array().unwrap().iter().any(|f| f["kind"] == "overlap"),
        "{}",
        doc["placement"]
    );
}

#[test]
fn board3d_reports_a_lock_that_breaks_a_requirement_with_the_layout_and_ends_with_1() {
    let layout = "version = 1\n[Widget.parts.r1]\nat = [35.0, 10.0]\nlock = true\n";
    let run = run_in(
        "refused-board3d",
        LOCKED_PHYSICAL,
        layout,
        &["board3d", "--json"],
    );
    assert_eq!(run.status.code(), Some(1), "{}", String::from_utf8_lossy(&run.stderr));
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&run.stderr)));
    let refused = doc["placement_refused"].as_str().unwrap_or_else(|| panic!("{doc}"));
    assert!(
        refused.contains("is locked in the layout file at (35, 10) mm"),
        "{refused}"
    );
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("error: Widget: "),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
