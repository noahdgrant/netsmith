//! `netsmith layout` (issue #782): `show` says the entry of each part as the placer reads it, `set`
//! writes one entry and keeps the file's other tables, refusing a part the board does not have and
//! a turn that is not from 0 up to 360, and `check` says what the file has wrong against the
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

    // A turn that is not from 0 up to 360 is a usage mistake.
    let run = netsmith(&dir, &["set", "led", "--turn", "360"]);
    assert_eq!(code(&run), 2);
    assert!(stderr(&run).contains("from 0 up to 360"), "{}", stderr(&run));

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
rotation = 450

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
            && l.contains("from 0 up to 360")),
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

/// The led placed by hand, and nothing else in the file.
const PLACED: &str = "\
version = 1

[Widget.parts.led]
at = [30.0, 12.0]
rotation = 0
side = \"top\"
";

#[test]
fn summary_sums_up_the_placed_board_as_one_line_of_json() {
    let dir = project("layout-summary", Some(PLACED));
    let run = netsmith(&dir, &["summary", "--format", "json", "--near", "led", "--within", "50"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let text = stdout(&run);
    assert_eq!(text.lines().count(), 1, "{text}");
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["board"], "Widget");
    assert_eq!(json["width"], 40.0);
    assert_eq!(json["height"], 24.0);
    assert_eq!(json["parts"], 3);
    assert!(json["totals"]["hpwl"].as_f64().unwrap() > 0.0, "{json}");
    let faces = json["faces"].as_array().unwrap();
    assert_eq!(faces[0]["face"], "top");
    assert_eq!(faces[1]["face"], "bottom");
    assert_eq!(faces[0]["parts"], 3);
    let usable = faces[0]["usable"].as_f64().unwrap();
    let courtyards = faces[0]["courtyards"].as_f64().unwrap();
    assert!(usable > 0.0 && usable < 40.0 * 24.0, "{json}");
    assert!(courtyards > 0.0 && courtyards < usable, "{json}");
    // The bottom face is empty: its largest free rectangle is most of the board.
    let free = &faces[1]["largest_free"][0];
    let width = free["max"][0].as_f64().unwrap() - free["min"][0].as_f64().unwrap();
    assert!(width > 30.0, "{json}");
    // The nets of two pads or more, each with its longest ratsnest line.
    let nets = json["nets"].as_array().unwrap();
    assert!(nets.iter().any(|n| n["name"] == "VCC"), "{json}");
    assert!(nets.iter().all(|n| n["pads"].as_u64().unwrap() >= 2), "{json}");
    // The led's neighbours, nearest first: the two other parts.
    let near = &json["neighbours"][0];
    assert_eq!(near["path"], "led");
    let gaps: Vec<f64> = near["near"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["gap"].as_f64().unwrap())
        .collect();
    assert_eq!(gaps.len(), 2, "{json}");
    assert!(gaps[0] <= gaps[1], "{gaps:?}");
    assert!(json["findings"].as_array().unwrap().is_empty(), "{json}");

    // For a person, a line per face.
    let run = netsmith(&dir, &["summary"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.starts_with("Widget: 40 x 24 mm, 3 parts"), "{text}");
    assert!(text.contains("\ntop: 3 parts, courtyards"), "{text}");

    // A part the board does not have is refused with the nearest names.
    let run = netsmith(&dir, &["summary", "--near", "power_n"]);
    assert_eq!(code(&run), 1);
    assert!(stderr(&run).contains("no part `power_n`"), "{}", stderr(&run));
}

#[test]
fn summary_gives_the_distance_between_two_pads() {
    let dir = project("layout-summary-pads", Some(PLACED));
    let run = netsmith(
        &dir,
        &["summary", "--format", "json", "--between", "r1.1", "r1.2", "--between", "power_in.1", "led.1"],
    );
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let json: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    let pads = json["pads"].as_array().unwrap();
    assert_eq!(pads.len(), 2, "{json}");
    // The two pads of one resistor: a small distance, and copper that does not touch.
    let r1 = &pads[0];
    assert_eq!(r1["from"].as_str().unwrap().split('.').nth(1), Some("1"), "{json}");
    let distance = r1["distance"].as_f64().unwrap();
    assert!(distance > 0.5 && distance < 5.0, "{json}");
    assert!(r1["gap"].as_f64().unwrap() < distance, "{json}");
    // The header and the led are far apart on this board, and each pad says its net.
    let far = pads[1]["distance"].as_f64().unwrap();
    assert!(far > 15.0, "{json}");
    assert_eq!(pads[1]["dx"].as_f64().unwrap().signum(), 1.0, "{json}");
    // Without the flag the document has no `pads`.
    let run = netsmith(&dir, &["summary", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    assert!(json.get("pads").is_none(), "{json}");
    // For a person, a line for each pair.
    let run = netsmith(&dir, &["summary", "--between", "r1.1", "r1.2"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(stdout(&run).contains("pads R"), "{}", stdout(&run));
    assert!(stdout(&run).contains("mm between centres"), "{}", stdout(&run));
    // A pad the part has none of is refused with the pads it has; so is a name that is no pad.
    let run = netsmith(&dir, &["summary", "--between", "r1.9", "r1.1"]);
    assert_eq!(code(&run), 1);
    assert!(stderr(&run).contains("has no pad `9`"), "{}", stderr(&run));
    let run = netsmith(&dir, &["summary", "--between", "r1", "r1.1"]);
    assert_eq!(code(&run), 1);
    assert!(stderr(&run).contains("is not a pad"), "{}", stderr(&run));
}

const ALL_PLACED: &str = "\
version = 1

[Widget.parts.led]
at = [30.0, 12.0]
rotation = 0
side = \"top\"

[Widget.parts.r1]
at = [20.0, 12.0]
rotation = 90
side = \"bottom\"

[Widget.parts.power_in]
at = [3.0, 12.0]
rotation = 90
side = \"top\"
";

fn parts_json(dir: &Path, args: &[&str]) -> serde_json::Value {
    let mut all = vec!["parts", "--format", "json"];
    all.extend_from_slice(args);
    let run = netsmith(dir, &all);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let text = stdout(&run);
    assert_eq!(text.lines().count(), 1, "{text}");
    serde_json::from_str(&text).unwrap()
}

#[test]
fn parts_gives_each_part_its_frame_courtyard_and_pads_as_one_line_of_json() {
    let dir = project("layout-parts", Some(ALL_PLACED));
    let json = parts_json(&dir, &["--as-laid-out"]);
    assert_eq!(json["board"], "Widget");
    assert_eq!(json["width"], 40.0);
    assert_eq!(json["height"], 24.0);
    assert_eq!(json["placement_refused"], serde_json::Value::Null);
    let parts = json["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 3, "{json}");
    let r1 = parts.iter().find(|p| p["path"] == "r1").expect("r1");
    // The frame is the layout file's: the entry's place, turn and face, mirrored on the bottom.
    assert_eq!(r1["frame"]["at"], serde_json::json!([20.0, 12.0]));
    assert_eq!(r1["frame"]["turn"], 90.0);
    assert_eq!(r1["frame"]["side"], "bottom");
    assert_eq!(r1["frame"]["mirrored"], true);
    // Its courtyard is a list of shapes, on the board and in the footprint's frame, and the
    // frame's size is the box round them.
    let shapes = r1["courtyards"].as_array().unwrap();
    assert!(!shapes.is_empty() && shapes.len() == r1["local_courtyards"].as_array().unwrap().len());
    let (min, max) = (&r1["courtyard"]["min"], &r1["courtyard"]["max"]);
    let width = max[0].as_f64().unwrap() - min[0].as_f64().unwrap();
    assert!((r1["frame"]["width"].as_f64().unwrap() - width).abs() < 0.011, "{r1}");
    // Two pads, each with a net, a size and a place that follows the frame's rule: the turn is a
    // quarter, so a footprint's x offset becomes a y offset (and the mirror flips its sign).
    let pads = r1["pads"].as_array().unwrap();
    assert_eq!(pads.len(), 2, "{r1}");
    for pad in pads {
        assert!(pad["net"].is_string(), "{pad}");
        assert!(pad["width"].as_f64().unwrap() > 0.0 && pad["height"].as_f64().unwrap() > 0.0);
        assert_eq!(pad["kind"], "smd");
        assert_eq!(pad["copper"], serde_json::json!(["bottom"]));
        let (lx, ly) = (pad["local"][0].as_f64().unwrap(), pad["local"][1].as_f64().unwrap());
        // Mirror x, then turn 90 degrees counterclockwise on screen (y down).
        let want = [20.0 + ly, 12.0 + lx];
        let at = [pad["at"][0].as_f64().unwrap(), pad["at"][1].as_f64().unwrap()];
        assert!((at[0] - want[0]).abs() < 0.002 && (at[1] - want[1]).abs() < 0.002, "{pad} {want:?}");
    }
    // The led's pad is on the board's nets: the same net as the header's.
    let nets: Vec<&str> = parts
        .iter()
        .flat_map(|p| p["pads"].as_array().unwrap())
        .filter_map(|pad| pad["net"].as_str())
        .collect();
    assert!(nets.contains(&"VCC") && nets.contains(&"GND"), "{nets:?}");
}

#[test]
fn parts_picks_parts_by_path_designator_or_module_and_refuses_a_name_the_board_lacks() {
    let dir = project("layout-parts-pick", Some(ALL_PLACED));
    let json = parts_json(&dir, &["led", "--as-laid-out"]);
    assert_eq!(json["parts"].as_array().unwrap().len(), 1, "{json}");
    let refdes = json["parts"][0]["refdes"].as_str().unwrap().to_string();
    let json = parts_json(&dir, &[&refdes, "r1"]);
    assert_eq!(json["parts"].as_array().unwrap().len(), 2, "{json}");
    let run = netsmith(&dir, &["parts", "power_n"]);
    assert_eq!(code(&run), 1);
    assert!(stderr(&run).contains("no part `power_n`"), "{}", stderr(&run));
    // For a person: a line for each part and each pad.
    let run = netsmith(&dir, &["parts", "r1", "--as-laid-out"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.starts_with("Widget: 40 x 24 mm, 1 parts"), "{text}");
    assert!(text.contains("at 20,12 turn 90 bottom (mirrored)"), "{text}");
    assert!(text.contains("  pad 1 smd "), "{text}");
}

#[test]
fn parts_is_refused_as_laid_out_for_a_part_with_no_entry() {
    let dir = project("layout-parts-open", Some(PLACED));
    let run = netsmith(&dir, &["parts", "--as-laid-out"]);
    assert_eq!(code(&run), 1);
    assert!(!stderr(&run).is_empty());
    let run = netsmith(&dir, &["parts", "--as-laid-out", "--place-seeds", "2"]);
    assert_eq!(code(&run), 2);
}

#[test]
fn parts_gives_the_data_with_the_findings_beside_it_when_the_layout_overlaps() {
    // The led and the resistor locked on one place: `fab` refuses the board, and `layout parts`
    // gives the geometry, which is what is needed to fix it, with the findings.
    let overlapping = ALL_PLACED.replace("at = [20.0, 12.0]", "at = [30.0, 12.0]");
    let overlapping = overlapping.replace("side = \"bottom\"", "side = \"top\"\nlock = true");
    let overlapping = overlapping.replacen(
        "side = \"top\"\n\n[Widget.parts.r1]",
        "side = \"top\"\nlock = true\n\n[Widget.parts.r1]",
        1,
    );
    let dir = project("layout-parts-findings", Some(&overlapping));
    let json = parts_json(&dir, &["--as-laid-out"]);
    assert_eq!(json["parts"].as_array().unwrap().len(), 3, "{json}");
    let placement = json["placement"].as_array().unwrap();
    assert!(placement.iter().any(|f| f["kind"] == "overlap"), "{json}");
    let run = netsmith(&dir, &["parts", "--as-laid-out"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(stdout(&run).contains("finding: "), "{}", stdout(&run));
}

#[test]
fn compare_says_which_parts_moved_between_two_layout_files() {
    let dir = project("layout-compare", Some(PLACED));
    fs::write(
        dir.join("before.pcb.toml"),
        PLACED.replace("at = [30.0, 12.0]", "at = [26.0, 9.0]"),
    )
    .unwrap();
    let run = netsmith(&dir, &["compare", "before.pcb.toml", "--format", "json"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let json: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    assert_eq!(json["board"], "Widget");
    let moved = json["moved"].as_array().unwrap();
    let led = moved
        .iter()
        .find(|m| m["path"] == "led")
        .unwrap_or_else(|| panic!("{json}"));
    assert_eq!(led["from"]["x"], 26.0);
    assert_eq!(led["from"]["y"], 9.0);
    assert_eq!(led["to"]["x"], 30.0);
    assert_eq!(led["to"]["y"], 12.0);
    assert_eq!(led["distance"], 5.0);
    assert!(json["before"]["hpwl"].as_f64().is_some(), "{json}");
    assert!(json["appeared"].as_array().is_some(), "{json}");

    // The same file on both sides: nothing moved.
    let run = netsmith(&dir, &["compare", "main.pcb.toml", "main.pcb.toml"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(stdout(&run).starts_with("Widget: 0 parts moved"), "{}", stdout(&run));

    // A layout file that is not there cannot be compared.
    let run = netsmith(&dir, &["compare", "nothing.pcb.toml"]);
    assert_eq!(code(&run), 2);
    assert!(stderr(&run).contains("nothing.pcb.toml"), "{}", stderr(&run));
}
