//! The board's copper areas in the editor's PCB summary tab (issue #1085): `GET /api/pcb/copper`
//! answers what `netsmith board copper` prints, from the source and the routes file with no
//! placing, and counts the teardrop fills only once the board has been routed here.

use std::fs;
use std::sync::{Arc, Mutex};

use netsmith_editor::{Backend, Say};
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const POURS: &str = "OutlineAndPours";

/// A project with `examples/led.nsm` (two layers, no pour) as its one top-level file.
fn led_project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-copper-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"copper\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), include_str!("../../../examples/led.nsm")).unwrap();
    fs::write(
        root.join("src/main.lock.toml"),
        include_str!("../../../examples/led.lock.toml"),
    )
    .unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

/// The four-layer example: ground pours on both outer layers, ground and 3.3 V planes.
fn pours_project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-copper-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"copper\"\n").unwrap();
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
        fs::write(root.join("src").join(file), text).unwrap();
    }
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

#[test]
fn a_board_with_pours_and_planes_says_each_layer_and_its_fields() {
    let (_root, editor) = pours_project("layers");
    let copper = editor.pcb_copper(POURS).unwrap();
    assert_eq!(copper.board, POURS);
    let rows: Vec<(&str, &str, Option<&str>)> = copper
        .layers
        .iter()
        .map(|l| (l.layer.as_str(), l.kind.as_str(), l.net.as_deref()))
        .collect();
    assert_eq!(
        rows,
        [
            ("top", "pour", Some("GND")),
            ("inner1", "plane", Some("GND")),
            ("inner2", "plane", Some("3V3")),
            ("bottom", "pour", Some("GND")),
        ]
    );
    assert_eq!(copper.layers[0].pour.as_ref().unwrap().stitch_pitch, Some(8.0));
    assert!(copper.layers[1].pour.is_none());
    assert!(copper.zones.is_empty());
    assert!(!copper.teardrops.counted && copper.teardrops.total == 0);
    assert_eq!(editor.pcb_copper("Nope").unwrap_err().status, 404);
}

#[test]
fn a_board_with_no_pour_has_signal_layers_and_a_hand_zone_is_counted_apart() {
    let (root, editor) = led_project("zone");
    let led = "LedExample";
    let copper = editor.pcb_copper(led).unwrap();
    let kinds: Vec<&str> = copper.layers.iter().map(|l| l.kind.as_str()).collect();
    assert_eq!(kinds, ["signal", "signal"]);
    // A zone of the routes file, read when the board is asked about again.
    fs::write(
        root.join("src/main.routes.toml"),
        r#"version = 1

[LedExample.nets.GND]
pads = "x"

[[LedExample.nets.GND.zones]]
layers = ["bottom"]
priority = 0
clearance = 0.2
min_width = 0.25
pads = "thermal"
thermal_gap = 0.5
spoke_width = 0.5
islands = "remove"
outline = [[0, 0], [4, 0], [4, 3], [0, 3]]
"#,
    )
    .unwrap();
    let copper = editor.pcb_copper(led).unwrap();
    assert_eq!(copper.zones.len(), 1);
    assert_eq!((copper.zones[0].net.as_str(), copper.zones[0].area), ("GND", 12.0));
    let bottom = &copper.layers[1];
    assert_eq!((bottom.layer.as_str(), bottom.kind.as_str(), bottom.zones), ("bottom", "signal", 1));
    // No pour of the process is replaced by it, and no teardrop is counted.
    assert!(!bottom.replaced_by_zones && !copper.teardrops.counted);
}

#[test]
fn the_teardrop_fills_are_counted_once_the_board_is_routed_here() {
    let (_root, editor) = led_project("teardrops");
    let led = "LedExample";
    assert!(!editor.pcb_copper(led).unwrap().teardrops.counted);
    let log = Arc::new(Mutex::new(Vec::new()));
    let sink = log.clone();
    let say: Say = Arc::new(move |m: &str| sink.lock().unwrap().push(m.to_string()));
    editor.route(led, &say).unwrap();
    let copper = editor.pcb_copper(led).unwrap();
    assert!(copper.teardrops.counted, "{copper:?}");
    assert_eq!(
        copper.teardrops.total,
        copper.teardrops.layers.iter().map(|l| l.count).sum::<u32>()
    );
    // Teardrops are no pour and no hand zone.
    assert!(copper.zones.is_empty() && copper.layers.iter().all(|l| l.kind == "signal"));
}
