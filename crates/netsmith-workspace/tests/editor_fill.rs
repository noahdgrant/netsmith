//! How the fill of the board's pours joins each pad, in the editor's PCB summary tab (issue
//! #1187): `GET /api/pcb/fill` answers what `netsmith board fill` prints, of the board as the PCB
//! view has it placed (and routed), and its starved thermal reliefs are the design rules check's
//! findings of that board (which the editor runs once the board is routed).

use std::fs;
use std::sync::Arc;

use netsmith_editor::{Backend, Say};
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const POURS: &str = "OutlineAndPours";

/// The four-layer example (ground pours on both outer layers, ground and 3.3 V planes), its top
/// pour's clearance raised to 1.5 mm so that it cuts the spokes of ground pads beside other nets'.
fn starving_project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-fill-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"fill\"\n").unwrap();
    let source = include_str!("../../../examples/outline_and_pours.nsm").replace(
        "        .top.stitch_pitch &= 0mm..8mm\n",
        "        .top.stitch_pitch &= 0mm..8mm\n        .top.clearance &= 1.5mm\n",
    );
    assert!(source.contains(".top.clearance"), "the example's pour moved");
    for (file, text) in [
        ("main.nsm", source.as_str()),
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
fn the_fill_of_the_placed_board_names_the_pads_the_check_finds_starved() {
    let (_root, editor) = starving_project("starved");
    let fill = editor.pcb_fill(POURS).unwrap();
    assert_eq!(fill.board, POURS);
    assert!(!fill.routed);
    let fills: Vec<(&str, &str, &str)> = fill
        .fills
        .iter()
        .map(|f| (f.layer.as_str(), f.source.as_str(), f.net.as_str()))
        .collect();
    assert_eq!(
        fills,
        [
            ("top", "pour", "GND"),
            ("inner1", "plane", "GND"),
            ("inner2", "plane", "3V3"),
            ("bottom", "pour", "GND"),
        ]
    );
    let mut starved: Vec<String> = fill.starved.iter().map(|s| s.part.clone()).collect();
    starved.sort();
    assert!(!starved.is_empty(), "{fill:?}");
    assert_eq!(fill.counts.starved as usize, starved.len());

    // Routed here, the board is checked, and its fill asked again.
    let say: Say = Arc::new(|_: &str| {});
    editor.route(POURS, &say).unwrap();
    let fill = editor.pcb_fill(POURS).unwrap();
    assert!(fill.routed);
    let mut starved: Vec<String> = fill.starved.iter().map(|s| s.part.clone()).collect();
    starved.sort();
    let findings = editor.pcb_findings(POURS).unwrap();
    assert!(findings.routed);
    let mut found: Vec<String> = findings
        .findings
        .iter()
        .filter(|f| f.rule == "starved_thermal")
        .flat_map(|f| f.parts.clone())
        .collect();
    found.sort();
    assert_eq!(starved, found, "{fill:?}");
    assert_eq!(editor.pcb_fill("Nope").unwrap_err().status, 404);
}

/// A buck converter on a two-layer board poured with ground on both faces: the IC's one ground
/// pad hangs on a thermal relief of the top pour.
#[test]
fn the_fill_warns_of_a_switching_regulators_ground_pad_on_a_relief() {
    let root = TempDir::new("editor-fill-regulator");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"fill\"\n").unwrap();
    fs::write(
        root.join("src/main.nsm"),
        include_str!("../../../scripts/editor-verify-js/fixtures/buck_pours.nsm"),
    )
    .unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));

    let fill = editor.pcb_fill("BuckPours").unwrap();
    let [ground] = fill.regulator_grounds.as_slice() else {
        panic!("one ground pad of the regulator is on a relief: {fill:?}");
    };
    assert_eq!(
        (
            ground.path.as_str(),
            ground.layer.as_str(),
            ground.net.as_str(),
            ground.pad.as_str(),
            ground.join.as_str(),
        ),
        ("reg.ic", "top", "GND", "2", "thermal")
    );
    assert_eq!(ground.stated_by, None);
    // It is the pad of that part in the top pour, with the spokes the pour's list says.
    let pad = fill
        .fills
        .iter()
        .filter(|f| f.layer == ground.layer && f.net == ground.net)
        .flat_map(|f| &f.pads)
        .find(|p| p.part == ground.part && p.pad == ground.pad)
        .unwrap();
    assert_eq!(
        (ground.spokes, ground.joined_spokes, ground.at, ground.starved),
        (pad.spokes, pad.joined_spokes, pad.at, pad.starved)
    );
    // The page reads it by its camelCase names.
    let json = serde_json::to_value(&fill).unwrap();
    assert_eq!(
        json["regulatorGrounds"][0]["joinedSpokes"].as_u64(),
        pad.joined_spokes.map(u64::from),
        "{json}"
    );
}
