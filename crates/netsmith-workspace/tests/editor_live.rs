//! The PCB view watches the placer and the router (issue #764): the scene's placing tells its
//! progress and where it has the parts as it works, a route shows its copper as it is laid, and
//! Cancel stops a placing with nothing kept.

use std::{
    fs,
    sync::{Arc, Mutex},
};

use netsmith_editor::{Backend, Say, Show, api::PcbLive};
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const BOARD: &str = "LedExample";

/// A project with `examples/led.nsm` (and its pick lock) as its one top-level file.
fn led_project(name: &str) -> (TempDir, Arc<Workspace>) {
    let root = TempDir::new(&format!("editor-live-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"live\"\n").unwrap();
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
    (root, Arc::new(editor))
}

/// A `say` and a `show` that keep what they are told.
fn watchers() -> (Say, Arc<Mutex<Vec<String>>>, Show, Arc<Mutex<Vec<PcbLive>>>) {
    let said = Arc::new(Mutex::new(Vec::new()));
    let shown = Arc::new(Mutex::new(Vec::new()));
    let say: Say = {
        let said = Arc::clone(&said);
        Arc::new(move |m: &str| said.lock().unwrap().push(m.to_string()))
    };
    let show: Show = {
        let shown = Arc::clone(&shown);
        Arc::new(move |l: &PcbLive| shown.lock().unwrap().push(l.clone()))
    };
    (say, said, show, shown)
}

#[test]
fn the_scene_shows_the_parts_where_the_placer_has_them_while_it_places() {
    let (_root, editor) = led_project("place");
    let (say, _said, show, shown) = watchers();
    let scene = editor.pcb_scene_watched(BOARD, &say, &show).unwrap();
    let shown = shown.lock().unwrap();
    // The first glimpse is always passed on; the others at most five a second.
    assert!(!shown.is_empty());
    for live in shown.iter() {
        assert_eq!((live.board.as_str(), live.task.as_str()), (BOARD, "place"));
        assert_eq!(live.parts.len(), scene.parts.len());
        assert!(live.region.is_some() && live.tracks.is_empty());
        let mut names: Vec<&str> = live.parts.iter().map(|p| p.refdes.as_str()).collect();
        names.sort_unstable();
        let mut placed: Vec<&str> = scene.parts.iter().map(|p| p.refdes.as_str()).collect();
        placed.sort_unstable();
        assert_eq!(names, placed);
    }
    // The scene is kept: asked again, it is not placed again and shows nothing.
    let (say, _said, show, again) = watchers();
    editor.pcb_scene_watched(BOARD, &say, &show).unwrap();
    assert!(again.lock().unwrap().is_empty());
}

#[test]
fn a_route_shows_the_copper_the_router_lays() {
    let (_root, editor) = led_project("route");
    let (say, said, show, shown) = watchers();
    let report = editor.route_watched(BOARD, &say, &show).unwrap();
    assert!(report.routed && report.tracks > 0);
    let shown = shown.lock().unwrap();
    let routing: Vec<&PcbLive> = shown.iter().filter(|l| l.task == "route").collect();
    assert!(!routing.is_empty(), "{shown:?}");
    // The end of each try is always shown, with the copper it laid.
    assert!(routing.last().is_some_and(|l| !l.tracks.is_empty()));
    let layers = editor.pcb_scene(BOARD).unwrap().track_layers;
    for live in &routing {
        assert!(live.parts.is_empty() && live.region.is_none());
        assert!(live.tracks.iter().all(|t| layers.contains(&t.layer) && t.width > 0.0));
    }
    // The route places the board first, and that placing is watched too.
    assert!(shown.iter().any(|l| l.task == "place"));
    assert!(said.lock().unwrap().iter().any(|m| m.contains("routing")));
}

#[test]
fn cancel_stops_the_placing_that_runs_with_nothing_kept_and_not_the_next() {
    let (_root, editor) = led_project("cancel");
    let (say, _said, _, _) = watchers();
    // Cancelled from inside the placing, at its first glimpse: a place in the work that is the
    // same on every machine.
    let asked = Arc::new(Mutex::new(0));
    let show: Show = {
        let (editor, asked) = (Arc::clone(&editor), Arc::clone(&asked));
        Arc::new(move |_: &PcbLive| {
            *asked.lock().unwrap() += 1;
            editor.cancel_placing();
        })
    };
    let error = editor
        .pcb_scene_watched(BOARD, &say, &show)
        .expect_err("the placing was cancelled");
    assert!(error.is_cancelled(), "{error:?}");
    assert_eq!(
        *asked.lock().unwrap(),
        1,
        "the placing stopped at its next checkpoint, before another glimpse"
    );
    // Nothing was kept, and a cancel stops no placing that begins after it: the scene is placed.
    let (say, _said, show, shown) = watchers();
    let scene = editor.pcb_scene_watched(BOARD, &say, &show).unwrap();
    assert!(!scene.parts.is_empty());
    assert!(!shown.lock().unwrap().is_empty(), "placed again, not kept");
}
