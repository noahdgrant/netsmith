//! `netsmith board copper` (issue #1085): the planes, pours, hand zones and teardrop fills of a
//! board, from the source and the routes file, with no placing and no routing.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

/// A project folder with the four-layer example: ground pours on both outer layers, ground and
/// 3.3 V planes.
fn pours_project(name: &str) -> TempDir {
    let dir = TempDir::new(name);
    for (file, text) in [
        ("main.nsm", include_str!("../../../examples/outline_and_pours.nsm")),
        (
            "main.physical.nsm",
            include_str!("../../../examples/outline_and_pours.physical.nsm"),
        ),
        (
            "main.pcb.toml",
            include_str!("../../../examples/outline_and_pours.pcb.toml"),
        ),
        (
            "main.lock.toml",
            include_str!("../../../examples/outline_and_pours.lock.toml"),
        ),
    ] {
        fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn copper(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["board", "copper"])
        .args(args)
        .current_dir(cwd)
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap()
}

fn json(cwd: &Path, args: &[&str]) -> serde_json::Value {
    let run = copper(cwd, &[args, &["--format", "json"]].concat());
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    // One JSON document on stdout, and nothing else there.
    serde_json::from_slice(&run.stdout).unwrap()
}

fn layer<'a>(doc: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    doc["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["layer"] == name)
        .unwrap_or_else(|| panic!("no layer {name} in {doc}"))
}

#[test]
fn a_board_with_outer_pours_and_inner_planes_says_each_layer_and_its_fields() {
    let dir = pours_project("copper-pours");
    let run = copper(&dir, &["main.nsm"]);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(said.starts_with("OutlineAndPours: 4 copper layers\n"), "{said}");
    assert!(said.contains("top     pour on GND: stitch pitch 8 mm"), "{said}");
    assert!(said.contains("inner1  plane on GND\n"), "{said}");
    assert!(said.contains("inner2  plane on 3V3\n"), "{said}");
    assert!(said.contains("hand zones (routes file): 0\n"), "{said}");
    assert!(said.contains("teardrop fills: not counted"), "{said}");
    let doc = json(&dir, &["main.nsm"]);
    assert_eq!(doc["board"], "OutlineAndPours");
    for outer in ["top", "bottom"] {
        let l = layer(&doc, outer);
        assert_eq!((&l["kind"], &l["net"]), (&"pour".into(), &"GND".into()), "{l}");
        assert_eq!(l["pour"]["stitch_pitch"], 8.0);
        // A field the design does not state is null.
        assert!(l["pour"]["clearance"].is_null() && l["pour"]["pads"].is_null(), "{l}");
        assert_eq!(l["replaced_by_zones"], false);
    }
    assert_eq!(layer(&doc, "inner1")["kind"], "plane");
    assert_eq!(layer(&doc, "inner2")["net"], "3V3");
    assert!(layer(&doc, "inner1")["pour"].is_null());
    assert_eq!(doc["zones"], serde_json::json!([]));
    assert_eq!(doc["teardrops"]["counted"], false);
    // It writes nothing.
    let mut files: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(files, ["main.lock.toml", "main.nsm", "main.pcb.toml", "main.physical.nsm"]);
}

#[test]
fn a_board_with_no_pour_has_signal_layers() {
    let dir = TempDir::new("copper-none");
    fs::write(dir.join("led.nsm"), include_str!("../../../examples/led.nsm")).unwrap();
    fs::write(
        dir.join("led.lock.toml"),
        include_str!("../../../examples/led.lock.toml"),
    )
    .unwrap();
    let doc = json(&dir, &["led.nsm"]);
    let kinds: Vec<(&str, &str)> = doc["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["layer"].as_str().unwrap(), l["kind"].as_str().unwrap()))
        .collect();
    assert_eq!(kinds, [("top", "signal"), ("bottom", "signal")]);
}

#[test]
fn a_hand_zone_of_the_routes_file_is_counted_apart_and_replaces_the_pour_of_its_layer() {
    let dir = pours_project("copper-zone");
    fs::write(
        dir.join("main.routes.toml"),
        r#"version = 1

[OutlineAndPours.nets.GND]
pads = "x"

[[OutlineAndPours.nets.GND.zones]]
layers = ["top"]
priority = 1
clearance = 0.2
min_width = 0.25
pads = "solid"
thermal_gap = 0.5
spoke_width = 0.5
islands = "remove"
outline = [[1, 1], [11, 1], [11, 6], [1, 6]]
lock = true
"#,
    )
    .unwrap();
    let doc = json(&dir, &["main.nsm"]);
    let zones = doc["zones"].as_array().unwrap();
    assert_eq!(zones.len(), 1, "{doc}");
    assert_eq!(zones[0]["net"], "GND");
    assert_eq!(zones[0]["layers"], serde_json::json!(["top"]));
    assert_eq!(zones[0]["area"], 50.0);
    assert_eq!((&zones[0]["pads"], &zones[0]["locked"]), (&"solid".into(), &true.into()));
    // The zone is on the top layer, whose pour of the process is then not drawn; the bottom pour
    // and the planes are as they were.
    let top = layer(&doc, "top");
    assert_eq!((&top["kind"], &top["zones"], &top["replaced_by_zones"]), (&"pour".into(), &1.into(), &true.into()));
    assert_eq!(layer(&doc, "bottom")["replaced_by_zones"], false);
    assert_eq!(layer(&doc, "inner1")["zones"], 0);
    let said = String::from_utf8_lossy(&copper(&dir, &["main.nsm"]).stdout).into_owned();
    assert!(said.contains("(not drawn: 1 zone(s) of the routes file replace it)"), "{said}");
    assert!(said.contains("hand zones (routes file): 1\n  GND on top: 50 mm2, priority 1"), "{said}");
}

#[test]
fn a_routes_file_that_cannot_be_read_is_named() {
    let dir = pours_project("copper-bad-routes");
    fs::write(dir.join("main.routes.toml"), "version = 1\n[[[").unwrap();
    let run = copper(&dir, &["main.nsm"]);
    assert_eq!(run.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("main.routes.toml"),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
