//! `netsmith layout move` (issue #1627): one part's entry moved by an offset, or turned (issue
//! #1704), refused when that makes courtyards overlap or leaves the outline, with the ends of the
//! routes file that miss the part's pads after it. The board is the invented one of `layout.rs`, a 40 by 24 mm
//! board with a resistor R1 at (20, 12) and an LED D1 at (30, 12).

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
}
";

const LAYOUT: &str = "\
version = 1

[Widget.parts.led]
at = [30.0, 12.0]
rotation = 0
side = \"top\"

# The resistor, placed by hand.
[Widget.parts.r1]
at = [20.0, 12.0]
rotation = 0
side = \"top\"
";

/// R1's pads are 0.54 mm wide, at x 19.49 (VCC) and 20.51 (the LED's anode), y 12. Track 1
/// ends at the anode pad's centre and track 2 just inside its left edge, a via sits on the VCC
/// pad: a move of 0.1 mm to the right leaves track 2's end off the pad and the others on theirs.
const ROUTES: &str = "\
version = 1

[Widget.nets.\"net-led.anode\"]
pads = \"x\"

[[Widget.nets.\"net-led.anode\".tracks]]
layer = \"top\"
width = 0.2
from = [20.51, 12.0]
to = [25.0, 12.0]

[[Widget.nets.\"net-led.anode\".tracks]]
layer = \"top\"
width = 0.1
from = [20.25, 14.0]
to = [20.25, 12.0]

[Widget.nets.VCC]
pads = \"x\"

[[Widget.nets.VCC.vias]]
at = [19.49, 12.0]
diameter = 0.6
drill = 0.3
";

fn project(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"widget\"\ntop_level = [\"main.nsm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("main.nsm"), BOARD).unwrap();
    fs::write(dir.join("main.physical.nsm"), PHYSICAL).unwrap();
    fs::write(dir.join("main.pcb.toml"), LAYOUT).unwrap();
    fs::write(dir.join("main.routes.toml"), ROUTES).unwrap();
    dir
}

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["layout", "move"])
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn layout_file(dir: &Path) -> String {
    fs::read_to_string(dir.join("main.pcb.toml")).unwrap()
}

fn json(run: &Output) -> serde_json::Value {
    serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&run.stdout)))
}

#[test]
fn a_move_of_a_tenth_of_a_mm_changes_the_entry_and_lists_the_ends_that_miss_their_pad() {
    let dir = project("layout-move");
    let run = netsmith(&dir, &["r1", "0.1,0"]);
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        run.status.code(),
        Some(0),
        "{out}{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        out.contains("move `r1` (R1) by 0.1, 0 mm: from 20, 12 to 20.1, 12"),
        "{out}"
    );
    assert!(
        out.contains("net `net-led.anode`: track 2's end at 20.25, 12 misses R1.2"),
        "{out}"
    );
    assert!(out.contains("via 1 at 19.49, 12 on R1.1"), "{out}");
    assert!(out.contains("wrote "), "{out}");
    let file = layout_file(&dir);
    assert!(file.contains("at = [20.1, 12.0]"), "{file}");
    // The other entry, and the comment, are kept; the routes file is not touched.
    assert!(file.contains("at = [30.0, 12.0]"), "{file}");
    assert!(file.contains("# The resistor, placed by hand."), "{file}");
    assert_eq!(
        fs::read_to_string(dir.join("main.routes.toml")).unwrap(),
        ROUTES
    );

    // The same in JSON, by the designator, with the places in the board file's frame: the
    // offset is a distance and stays.
    let run = netsmith(&dir, &["R1", "-0.1,0", "--format", "json", "--frame", "board-file"]);
    assert_eq!(run.status.code(), Some(0));
    let doc = json(&run);
    assert_eq!(doc["frame"], "board-file");
    assert_eq!(doc["by"], serde_json::json!([-0.1, 0.0]));
    assert_eq!(doc["from"], serde_json::json!([40.1, 32.0]));
    assert_eq!(doc["to"], serde_json::json!([40.0, 32.0]));
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["written"], true);
    assert_eq!(doc["missed"], serde_json::json!([]));
    let still: Vec<&str> = doc["still_on"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["shape"].as_str().unwrap())
        .collect();
    assert_eq!(still, ["via 1", "track 1"], "{doc}");
    assert!(layout_file(&dir).contains("at = [20.0, 12.0]"));
}

#[test]
fn a_move_into_another_courtyard_is_refused_naming_the_overlap() {
    let dir = project("layout-move-overlap");
    let before = layout_file(&dir);
    let run = netsmith(&dir, &["r1", "9,0"]);
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(1), "{out}");
    assert!(out.contains("refused: move `r1` (R1) by 9, 0 mm"), "{out}");
    assert!(
        out.contains("the courtyards of `led` (D1) and `r1` (R1) overlap"),
        "{out}"
    );
    assert!(out.contains("not written: the move is refused"), "{out}");
    assert_eq!(layout_file(&dir), before);

    // Past the outline, in JSON: the faults are `layout check`'s `placement` findings.
    let run = netsmith(&dir, &["r1", "0,-11.8", "--format", "json"]);
    assert_eq!(run.status.code(), Some(1));
    let doc = json(&run);
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["written"], false);
    let kinds: Vec<&str> = doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["fault"]["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"outside_outline"), "{doc}");
    assert_eq!(layout_file(&dir), before);
}

#[test]
fn a_turn_of_180_in_place_changes_the_rotation_and_lists_the_ends_that_miss_their_pad() {
    let dir = project("layout-turn");
    let run = netsmith(&dir, &["r1", "--turn", "180"]);
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        run.status.code(),
        Some(0),
        "{out}{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        out.contains("move `r1` (R1) by 0, 0 mm: from 20, 12 to 20, 12"),
        "{out}"
    );
    assert!(
        out.contains("turn by 180 degrees counterclockwise: rotation 0 to 180"),
        "{out}"
    );
    // The pads have swapped: every end that was on one misses it.
    for end in [
        "net `net-led.anode`: track 1's end at 20.51, 12 misses R1.2",
        "net `net-led.anode`: track 2's end at 20.25, 12 misses R1.2",
        "net `VCC`: via 1 at 19.49, 12 misses R1.1",
    ] {
        assert!(out.contains(end), "{end}: {out}");
    }
    assert!(!out.contains("still on their pad"), "{out}");
    let file = layout_file(&dir);
    assert_eq!(file.matches("rotation = 180.0").count(), 1, "{file}");
    assert!(file.contains("at = [20.0, 12.0]"), "{file}");
    assert!(file.contains("at = [30.0, 12.0]"), "{file}");
    assert!(file.contains("# The resistor, placed by hand."), "{file}");
    assert_eq!(
        fs::read_to_string(dir.join("main.routes.toml")).unwrap(),
        ROUTES
    );

    // The turn is relative and wraps: 270 more on 180 is 90, with an offset too, in JSON and in
    // the board file's frame, which moves the places and not the turns.
    let run = netsmith(
        &dir,
        &[
            "R1", "0,1", "--turn", "270", "--format", "json", "--frame", "board-file",
        ],
    );
    let doc = json(&run);
    assert_eq!(run.status.code(), Some(0), "{doc}");
    assert_eq!(doc["turn_by"], 270.0);
    assert_eq!(doc["turn_from"], 180.0);
    assert_eq!(doc["turn_to"], 90.0);
    assert_eq!(doc["by"], serde_json::json!([0.0, 1.0]));
    assert_eq!(doc["to"], serde_json::json!([40.0, 33.0]));
    assert_eq!(doc["written"], true);
    let file = layout_file(&dir);
    assert!(file.contains("rotation = 90.0"), "{file}");
    assert!(file.contains("at = [20.0, 13.0]"), "{file}");
}

/// R1 with D1 close below it: they are clear of each other as they are, and R1 turned by a
/// quarter turn reaches into D1's courtyard.
#[test]
fn a_turn_into_an_overlap_is_refused_naming_it() {
    let dir = project("layout-turn-overlap");
    let near = LAYOUT.replace("at = [30.0, 12.0]", "at = [20.0, 13.5]");
    fs::write(dir.join("main.pcb.toml"), &near).unwrap();
    // The same board is allowed to turn end for end, which takes no more room.
    let run = netsmith(&dir, &["r1", "--turn", "180", "--dry-run"]);
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(0), "{out}");

    let run = netsmith(&dir, &["r1", "--turn", "90"]);
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(1), "{out}");
    assert!(
        out.contains("the courtyards of `led` (D1) and `r1` (R1) overlap"),
        "{out}"
    );
    assert!(out.contains("not written: the move is refused"), "{out}");
    assert_eq!(layout_file(&dir), near);
}

#[test]
fn dry_run_writes_nothing_and_says_the_same() {
    let dir = project("layout-move-dry");
    let before = layout_file(&dir);
    let run = netsmith(&dir, &["r1", "0.1,0", "--dry-run"]);
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(0), "{out}");
    assert!(out.contains("track 2's end at 20.25, 12 misses R1.2"), "{out}");
    assert!(out.contains("not written (--dry-run)"), "{out}");
    assert_eq!(layout_file(&dir), before);

    let run = netsmith(&dir, &["r1", "0.1,0", "--dry-run", "--format", "json"]);
    let doc = json(&run);
    assert_eq!(doc["written"], false);
    assert_eq!(doc["turn_by"], 0.0);
    assert_eq!(doc["missed"][0]["pad"], "R1.2");
    assert_eq!(layout_file(&dir), before);

    let run = netsmith(&dir, &["r1", "--turn", "180", "--dry-run"]);
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(0), "{out}");
    assert!(out.contains("rotation 0 to 180"), "{out}");
    assert!(out.contains("not written (--dry-run)"), "{out}");
    assert_eq!(layout_file(&dir), before);
}

#[test]
fn a_part_with_no_entry_is_refused() {
    let dir = project("layout-move-no-entry");
    let run = netsmith(&dir, &["power_in", "1,0"]);
    let err = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "{err}");
    assert!(err.contains("has no entry"), "{err}");
    assert!(err.contains("netsmith layout set power_in --at X,Y"), "{err}");
}
