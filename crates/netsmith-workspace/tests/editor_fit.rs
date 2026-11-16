//! Where one part fits, for the editor's PCB view (issue #1002): `GET /api/pcb/fit` answers
//! what `netsmith layout fit` says, for the placement the scene draws, and places nothing for
//! the question once the scene has been drawn.

use std::fs;

use netsmith_editor::Backend;
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const BOARD: &str = "LedExample";

/// A project with `examples/led.nsm` (and its pick lock) as its one top-level file.
fn led_project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-fit-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"fit\"\n").unwrap();
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
fn a_part_of_the_scene_fits_where_the_scene_has_it() {
    let (_root, editor) = led_project("scene");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let part = &scene.parts[0];
    let fit = editor.pcb_fit(BOARD, &part.refdes, None, 0.5, None).unwrap();
    assert_eq!(fit.board, BOARD);
    assert_eq!(fit.layout_stamp, scene.layout_stamp);
    assert_eq!((fit.part.refdes.as_str(), fit.part.path.as_str()), (part.refdes.as_str(), part.path.as_str()));
    assert!(fit.near.part.is_none() && fit.step == 0.5);
    // The placed board has no fault about the part: it fits where it is, and the first pose
    // (the nearest place) fits too.
    assert!(fit.now.fits && fit.now.messages.is_empty(), "{fit:?}");
    assert!(fit.poses[0].fits, "{fit:?}");
    assert!(fit.poses.iter().any(|p| p.side == "top") && fit.poses.iter().any(|p| p.side == "bottom"));
    // One face when asked.
    let top = editor.pcb_fit(BOARD, &part.refdes, None, 0.5, Some("top")).unwrap();
    assert!(top.poses.iter().all(|p| p.side == "top"), "{top:?}");
}

#[test]
fn near_a_part_or_a_point_the_places_are_measured_from_it() {
    let (_root, editor) = led_project("near");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let (a, b) = (&scene.parts[0], &scene.parts[1]);
    let fit = editor.pcb_fit(BOARD, &a.refdes, Some(&b.path), 0.25, None).unwrap();
    assert_eq!(fit.near.part.as_ref().unwrap().refdes, b.refdes);
    assert!(fit.poses[0].fits, "{fit:?}");
    // The nearest place to a point is nearer that point than the part is now, or the same.
    let point = editor.pcb_fit(BOARD, &a.refdes, Some("1.5,2"), 0.5, None).unwrap();
    assert_eq!((point.near.x, point.near.y), (1.5, 2.0));
    assert!(point.near.part.is_none());
    // The query leaves the placement as it was: the scene is the same afterwards.
    assert_eq!(editor.pcb_scene(BOARD).unwrap().layout_stamp, scene.layout_stamp);
}

#[test]
fn what_is_wrong_with_the_query_is_a_bad_request_with_the_reason() {
    let (_root, editor) = led_project("wrong");
    let e = editor.pcb_fit(BOARD, "nothing", None, 0.5, None).unwrap_err();
    assert_eq!(e.status, 400);
    assert!(e.message.contains("no part `nothing`"), "{e:?}");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let refdes = &scene.parts[0].refdes;
    let e = editor.pcb_fit(BOARD, refdes, Some("nothing"), 0.5, None).unwrap_err();
    assert_eq!(e.status, 400);
    let e = editor.pcb_fit(BOARD, refdes, Some("1,x"), 0.5, None).unwrap_err();
    assert!(e.status == 400 && e.message.contains("`near`"), "{e:?}");
    assert_eq!(editor.pcb_fit(BOARD, refdes, None, 0.0, None).unwrap_err().status, 400);
    assert_eq!(editor.pcb_fit("Nope", refdes, None, 0.5, None).unwrap_err().status, 404);
}
