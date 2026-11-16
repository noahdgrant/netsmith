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
    run_in_with(name, "", physical, layout, args)
}

/// [`run_in`] with `manifest` lines under `[project]`.
fn run_in_with(
    name: &str,
    manifest: &str,
    physical: &str,
    layout: &str,
    args: &[&str],
) -> std::process::Output {
    let root = out(name);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"widget\"\ntop_level = [\"src/main.nsm\"]\n{manifest}\n"),
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

#[test]
fn fab_json_has_the_requirements_and_faults_of_a_lock_that_breaks_a_requirement() {
    // Issue #779: the refusal is the sentence, and the board as the lock leaves it is data: the
    // requirement the lock misses, with its numbers, as for any other miss.
    let layout = "version = 1\n[Widget.parts.r1]\nat = [35.0, 10.0]\nlock = true\n";
    let run = run_in(
        "refused-fab",
        LOCKED_PHYSICAL,
        layout,
        &["fab", "--format", "json", "--out", "fab"],
    );
    assert!(!run.status.success());
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&run.stdout)));
    let finding = &doc["findings"][0];
    assert!(
        finding["message"]
            .as_str()
            .unwrap()
            .contains("is locked in the layout file at (35, 10) mm"),
        "{finding}"
    );
    let miss = finding["requirements"]
        .as_array()
        .and_then(|r| r.iter().find(|r| r["id"] == "r1.place.x"))
        .unwrap_or_else(|| panic!("{finding}"));
    assert!(miss["measured"].as_f64().unwrap() > 30.0, "{miss}");
    assert!(miss["over"].as_f64().unwrap() > 0.0, "{miss}");
}

#[test]
fn fab_json_has_the_part_too_big_for_the_outline_as_data() {
    // Issue #779: what the requirements prove before placing, about one part, is data too.
    let physical = LOCKED_PHYSICAL.replace("40mm", "6mm").replace("24mm", "6mm");
    let run = run_in(
        "too-big-fab",
        &physical,
        "version = 1\n",
        &["fab", "--format", "json", "--out", "fab"],
    );
    assert!(!run.status.success());
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&run.stdout)));
    let finding = &doc["findings"][0];
    let big = finding["conflicts"]
        .as_array()
        .and_then(|c| c.iter().find(|c| c["kind"] == "too_big"))
        .unwrap_or_else(|| panic!("{finding}"));
    assert!(big["parts"][0]["refdes"].is_string(), "{big}");
    assert!(big["turns"][0]["width"].as_f64().unwrap() > 0.0, "{big}");
    assert_eq!(big["outline"]["width"], 6.0, "{big}");
}

/// The layout file of `LOCKED_OVER` with no lock: the placer places the board, then both parts
/// are put where the file says, over each other.
const UNLOCKED_OVER: &str = "version = 1\n[Widget.parts.r1]\nat = [10.0, 10.0]\n[Widget.parts.led]\nat = [10.5, 10.2]\n";

#[test]
fn board3d_ends_with_1_for_a_placed_board_whose_courtyards_overlap() {
    // Issue #780: a placement with an overlap is not a success, as `fab` refuses it.
    let run = run_in(
        "unlocked-over-board3d",
        LOCKED_PHYSICAL,
        UNLOCKED_OVER,
        &["board3d", "--json"],
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "{stderr}");
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {stderr}"));
    assert!(doc["placement_refused"].is_null(), "{doc}");
    assert!(
        doc["placement"].as_array().unwrap().iter().any(|f| f["kind"] == "overlap"),
        "{}",
        doc["placement"]
    );
    assert!(stderr.contains("error: Widget: the placement has"), "{stderr}");
}

#[test]
fn layout_fit_gives_the_nearest_place_clear_of_the_others_and_writes_nothing() {
    let root = out("fit-r1");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"widget\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), BOARD).unwrap();
    fs::write(root.join("src/main.physical.nsm"), LOCKED_PHYSICAL).unwrap();
    fs::write(root.join("src/main.pcb.toml"), UNLOCKED_OVER).unwrap();
    let netsmith = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
            .args(args)
            .current_dir(&root)
            .output()
            .unwrap()
    };
    let run = netsmith(&["layout", "fit", "r1", "--near", "led", "--format", "json"]);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(0), "{stderr}");
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {stderr}"));
    assert_eq!(doc["part"]["path"], "r1", "{doc}");
    assert_eq!(doc["near"]["part"]["path"], "led", "{doc}");
    // Where it is, it overlaps the LED.
    assert_eq!(doc["now"]["fits"], false, "{doc}");
    assert!(
        doc["now"]["placement"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["kind"] == "overlap"),
        "{}",
        doc["now"]
    );
    // The first pose fits, and the requirement `.r1.place.x <= 30mm` holds there.
    let first = &doc["poses"][0];
    assert_eq!(first["fits"], true, "{doc}");
    let x = first["at"][0].as_f64().unwrap();
    assert!(x <= 30.0, "{first}");
    // Put there, `layout check` and `board3d` find nothing about it.
    let at = format!("{},{}", x, first["at"][1].as_f64().unwrap());
    let turn = first["turn"].as_f64().unwrap().to_string();
    let side = first["side"].as_str().unwrap();
    let before = fs::read_to_string(root.join("src/main.pcb.toml")).unwrap();
    assert_eq!(before, UNLOCKED_OVER, "fit writes nothing");
    let set = netsmith(&["layout", "set", "r1", "--at", &at, "--turn", &turn, "--side", side]);
    assert!(set.status.success(), "{}", String::from_utf8_lossy(&set.stderr));
    let again = netsmith(&["layout", "fit", "r1", "--format", "json"]);
    let doc: serde_json::Value = serde_json::from_slice(&again.stdout).unwrap();
    assert_eq!(doc["now"]["fits"], true, "{doc}");
    // A part the board lacks is refused with the nearest names.
    let none = netsmith(&["layout", "fit", "r9"]);
    assert_eq!(none.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&none.stderr).contains("no part `r9`"),
        "{}",
        String::from_utf8_lossy(&none.stderr)
    );
}

#[test]
fn layout_fit_as_laid_out_queries_the_layout_files_board_with_no_placement() {
    // Issue #1002: every part has an entry, so the board is the file's places and nothing is
    // placed; the answer is the one the placed board gives.
    let all = format!("{UNLOCKED_OVER}[Widget.parts.power_in]\nat = [30.0, 12.0]\n");
    let fit = |name: &str, layout: &str, args: &[&str]| {
        let mut full = vec!["layout", "fit", "r1", "--near", "led", "--format", "json"];
        full.extend_from_slice(args);
        run_in(name, LOCKED_PHYSICAL, layout, &full)
    };
    let placed = fit("fit-as-placed", &all, &[]);
    let laid = fit("fit-as-laid-out", &all, &["--as-laid-out"]);
    let stderr = String::from_utf8_lossy(&laid.stderr);
    assert_eq!(laid.status.code(), Some(0), "{stderr}");
    let laid_doc: serde_json::Value = serde_json::from_slice(&laid.stdout).unwrap();
    let placed_doc: serde_json::Value = serde_json::from_slice(&placed.stdout).unwrap();
    assert_eq!(laid_doc, placed_doc);
    assert_eq!(laid_doc["now"]["fits"], false, "r1 overlaps the LED: {laid_doc}");

    // Locked parts are where the file has them, an overlap of two locks included.
    let locked = format!("{LOCKED_OVER}[Widget.parts.power_in]\nat = [30.0, 12.0]\nlock = true\n");
    let laid = fit("fit-locked-as-laid-out", &locked, &["--as-laid-out"]);
    let stderr = String::from_utf8_lossy(&laid.stderr);
    let doc: serde_json::Value = serde_json::from_slice(&laid.stdout)
        .unwrap_or_else(|e| panic!("{e}: {stderr}"));
    assert_eq!(doc["now"]["fits"], false, "{doc}");
    assert!(
        doc["now"]["placement"].as_array().unwrap().iter().any(|f| f["kind"] == "overlap"),
        "{doc}"
    );

    // A part with no entry is named, and the answer is a refusal (1), not a guess.
    let short = fit("fit-short", UNLOCKED_OVER, &["--as-laid-out"]);
    assert_eq!(short.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&short.stderr);
    assert!(stderr.contains("no place for `power_in`"), "{stderr}");

    // The placer's seeds have nothing to do with it: a usage mistake (2).
    let both = fit("fit-both", &all, &["--as-laid-out", "--place-seeds", "2"]);
    assert_eq!(both.status.code(), Some(2));
}

#[test]
fn build_json_has_the_place_findings_with_the_lines_their_windows_and_the_faults_as_data() {
    // Issue #998: what is proved before placing is a finding of code `place` in `build` too,
    // with the lines of the physical file and the window each allows as data.
    let physical = LOCKED_PHYSICAL.replace(
        "    .r1.place.x <= 30mm\n",
        "    .r1.place.x &= 10mm +/- 0.1mm\n    .r1.place.y &= 10mm +/- 0.1mm\n    .led.place.x &= 10mm +/- 0.1mm\n    .led.place.y &= 10mm +/- 0.1mm\n    .led.place.rotation &= 90\n",
    );
    let run = run_in(
        "build-place-overlap",
        &physical,
        "version = 1\n",
        &["build", "--format", "json", "--out", "out"],
    );
    assert!(!run.status.success());
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&run.stdout)));
    let finding = doc["findings"]
        .as_array()
        .and_then(|f| f.iter().find(|f| f["code"] == "place"))
        .unwrap_or_else(|| panic!("{doc}"));
    assert_eq!(finding["severity"], "error", "{finding}");
    let conflict = finding["conflicts"]
        .as_array()
        .and_then(|c| c.iter().find(|c| c["kind"] == "overlap"))
        .unwrap_or_else(|| panic!("{finding}"));
    let lines = conflict["lines"].as_array().unwrap();
    let x = lines
        .iter()
        .find(|l| l["id"] == "r1.place.x")
        .unwrap_or_else(|| panic!("{conflict}"));
    assert_eq!(x["kind"], "requirement", "{x}");
    assert_eq!(x["measure"], "x", "{x}");
    assert_eq!(x["unit"], "mm", "{x}");
    assert_eq!(x["min"], 9.9, "{x}");
    assert_eq!(x["max"], 10.1, "{x}");
    assert!(x["source"].as_str().unwrap().contains("main.physical.nsm:"), "{x}");
    let turn = lines
        .iter()
        .find(|l| l["id"] == "led.place.rotation")
        .unwrap_or_else(|| panic!("{conflict}"));
    assert_eq!(turn["measure"], "rotation", "{turn}");
    assert_eq!(turn["unit"], "degrees", "{turn}");
}

#[test]
fn build_json_has_the_part_too_big_and_a_refused_lock_as_place_findings_with_data() {
    let physical = LOCKED_PHYSICAL.replace("40mm", "6mm").replace("24mm", "6mm");
    let run = run_in(
        "build-place-too-big",
        &physical,
        "version = 1\n",
        &["build", "--format", "json", "--out", "out"],
    );
    assert!(!run.status.success());
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&run.stdout)));
    let findings: Vec<&serde_json::Value> = doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "place")
        .collect();
    assert!(!findings.is_empty(), "{doc}");
    // A fault about one part is a finding of its own with its one conflict.
    let big = findings
        .iter()
        .find(|f| f["conflicts"][0]["kind"] == "too_big")
        .unwrap_or_else(|| panic!("{doc}"));
    assert_eq!(big["conflicts"].as_array().unwrap().len(), 1, "{big}");
    assert_eq!(big["conflicts"][0]["outline"]["width"], 6.0, "{big}");

    // A lock that breaks a requirement: the board as the lock leaves it.
    let layout = "version = 1\n[Widget.parts.r1]\nat = [35.0, 10.0]\nlock = true\n";
    let run = run_in(
        "build-place-lock",
        LOCKED_PHYSICAL,
        layout,
        &["build", "--format", "json", "--out", "out"],
    );
    assert!(!run.status.success());
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&run.stdout)));
    let finding = doc["findings"]
        .as_array()
        .and_then(|f| f.iter().find(|f| f["code"] == "place"))
        .unwrap_or_else(|| panic!("{doc}"));
    let miss = finding["requirements"]
        .as_array()
        .and_then(|r| r.iter().find(|r| r["id"] == "r1.place.x"))
        .unwrap_or_else(|| panic!("{finding}"));
    assert!(miss["over"].as_f64().unwrap() > 0.0, "{miss}");
}

#[test]
fn constraints_warn_makes_the_lines_that_cannot_hold_a_place_warning_with_data() {
    // Issue #998: with `constraints = "warn"` the conflicts are warnings; they keep their data,
    // in `build`.
    let physical = LOCKED_PHYSICAL.replace(
        "    .r1.place.x <= 30mm\n",
        "    .r1.place.x &= 10mm +/- 0.1mm\n    .r1.place.y &= 10mm +/- 0.1mm\n    .led.place.x &= 10mm +/- 0.1mm\n    .led.place.y &= 10mm +/- 0.1mm\n",
    );
    let run = run_in_with(
        "build-place-warn",
        "constraints = \"warn\"",
        &physical,
        "version = 1\n",
        &["build", "--format", "json", "--out", "out"],
    );
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {stderr}"));
    let finding = doc["findings"]
        .as_array()
        .and_then(|f| f.iter().find(|f| f["code"] == "place" && f["severity"] == "warning"))
        .unwrap_or_else(|| panic!("{doc}"));
    assert!(
        finding["conflicts"]
            .as_array()
            .is_some_and(|c| c.iter().any(|c| c["kind"] == "overlap")),
        "{finding}"
    );
}
