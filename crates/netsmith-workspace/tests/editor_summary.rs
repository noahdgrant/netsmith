//! The placed board summed up for the editor's PCB view (issue #1003): `GET /api/pcb/summary`
//! answers what `netsmith layout summary` prints, for the placement the scene draws, with the
//! neighbours of the parts asked about and the distance between two pads.

use std::fs;

use netsmith_editor::Backend;
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const BOARD: &str = "LedExample";

/// A project with `examples/led.nsm` (and its pick lock) as its one top-level file.
fn led_project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-summary-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"summary\"\n").unwrap();
    fs::write(
        root.join("src/main.nsm"),
        include_str!("../../../examples/led.nsm"),
    )
    .unwrap();
    fs::write(
        root.join("src/main.lock.toml"),
        include_str!("../../../examples/led.lock.toml"),
    )
    .unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

#[test]
fn the_summary_is_of_the_placement_the_scene_draws() {
    let (_root, editor) = led_project("scene");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let summary = editor.pcb_summary(BOARD, &[], 2.0, &[]).unwrap();
    assert_eq!(summary.board, BOARD);
    assert_eq!(summary.layout_stamp, scene.layout_stamp);
    assert_eq!(summary.parts as usize, scene.parts.len());
    assert_eq!(summary.faces.len(), 2);
    assert_eq!(summary.faces[0].face, "top");
    assert!(summary.totals.hpwl > 0.0 && summary.faces[0].free > 0.0, "{summary:?}");
    assert!(!summary.nets.is_empty() && summary.nets.iter().all(|n| n.pads >= 2));
    // Without the parameters there are no neighbours and no pads.
    assert!(summary.neighbours.is_empty() && summary.pads.is_empty());
}

#[test]
fn neighbours_and_the_distance_between_two_pads_are_asked_for() {
    let (_root, editor) = led_project("asked");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let part = &scene.parts[0];
    let other = &scene.parts[1];
    let pad = |p: &netsmith_editor::api::PcbPart| p.pads[0].clone();
    let (a, b) = (pad(part), pad(other));
    let summary = editor
        .pcb_summary(
            BOARD,
            std::slice::from_ref(&part.refdes),
            500.0,
            &[(
                format!("{}.{}", part.refdes, a.number),
                format!("{}.{}", other.path, b.number),
            )],
        )
        .unwrap();
    let near = &summary.neighbours[0];
    assert_eq!(near.part, part.refdes);
    assert_eq!(near.near.len(), scene.parts.len() - 1);
    // The distance is the one between the pads the scene draws, to the hundredth of a mm.
    let d = &summary.pads[0];
    let expected = (a.x - b.x).hypot(a.y - b.y);
    assert!((d.distance - expected).abs() < 0.02, "{d:?} against {expected}");
    assert_eq!(d.from, format!("{}.{}", part.refdes, a.number));
    assert_eq!(d.to, format!("{}.{}", other.refdes, b.number));
    assert!(d.gap <= d.distance);
}

#[test]
fn a_part_or_pad_the_board_has_none_of_is_a_bad_request_with_the_reason() {
    let (_root, editor) = led_project("wrong");
    let near = ["nothing".to_string()];
    let e = editor.pcb_summary(BOARD, &near, 2.0, &[]).unwrap_err();
    assert_eq!(e.status, 400);
    assert!(e.message.contains("no part `nothing`"), "{e:?}");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let refdes = &scene.parts[0].refdes;
    let e = editor
        .pcb_summary(BOARD, &[], 2.0, &[(format!("{refdes}.999"), format!("{refdes}.1"))])
        .unwrap_err();
    assert_eq!(e.status, 400);
    assert!(e.message.contains("has no pad `999`"), "{e:?}");
    assert_eq!(editor.pcb_summary("Nope", &[], 2.0, &[]).unwrap_err().status, 404);
}
