//! The routes file in the editor's PCB view (epic #599, R5): its records are in the scene with
//! what of them is drawn, a record that is not used as written is a finding, and a change from the
//! page (delete, lock, unlock) is made as `netsmith routes` makes it, refused with the reason when
//! it is locked and with 409 when the file changed, and the view is placed again from the file.

use std::{
    fs,
    net::{Ipv4Addr, TcpListener},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
};

use netsmith_editor::{
    Backend,
    api::{FindingSource, PcbRoutesEdit, PcbScene, RoutesChange, Severity},
};
use netsmith_fabricate::project::Target;
use netsmith_pcb::{LayoutFace, RouteTrack, Routes};
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

use super::editor::http;

const BOARD: &str = "LedExample";

/// A project with `examples/led.nsm` (and its pick lock) as its one top-level file.
fn project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-routes-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"routes\"\n").unwrap();
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

fn routes_file(root: &Path) -> PathBuf {
    root.join("src/main.routes.toml")
}

fn change(scene: &PcbScene, change: RoutesChange) -> PcbRoutesEdit {
    PcbRoutesEdit {
        board: BOARD.into(),
        stamp: scene.routes_stamp.clone(),
        change,
    }
}

fn lock(net: &str, track: Option<u32>, on: bool) -> RoutesChange {
    RoutesChange::Lock {
        net: net.into(),
        track,
        via: None,
        lock: on,
    }
}

fn delete(net: &str, track: Option<u32>) -> RoutesChange {
    RoutesChange::Delete {
        net: net.into(),
        track,
        via: None,
    }
}

/// A pad of `net` on the board, in the routes file's frame (mm from the outline's corner).
fn pad_of(scene: &PcbScene, net: &str) -> [f64; 2] {
    let pad = scene
        .parts
        .iter()
        .flat_map(|p| &p.pads)
        .find(|p| p.net.as_deref() == Some(net))
        .unwrap_or_else(|| panic!("no pad of {net}"));
    [pad.x - scene.origin_x, pad.y - scene.origin_y]
}

/// The routes file with `edit` made to it by hand, as another program would.
fn rewrite(root: &Path, edit: impl FnOnce(&mut Routes)) {
    let file = routes_file(root);
    let mut routes = Routes::parse(&fs::read_to_string(&file).unwrap(), "routes").unwrap();
    edit(&mut routes);
    fs::write(&file, routes.to_text()).unwrap();
}

#[test]
fn the_view_locks_unlocks_and_deletes_as_the_routes_commands_do() {
    let (root, editor) = project("edit");
    let scene = editor.pcb_scene(BOARD).unwrap();
    assert_eq!(scene.routes, []);
    assert_eq!(scene.routes_file, "main.routes.toml");
    assert!(!routes_file(&root).exists());

    // Lock a net with no record: the file is written with the net's pads as they are.
    let locked = editor
        .pcb_routes(&change(&scene, lock("VCC", None, true)))
        .unwrap();
    assert!(locked.said.starts_with("locked net VCC"), "{}", locked.said);
    assert!(locked.said.ends_with("wrote main.routes.toml"), "{}", locked.said);
    let text = fs::read_to_string(routes_file(&root)).unwrap();
    assert!(text.contains("[LedExample.nets.VCC]\npads = \""), "{text}");
    assert!(text.contains("lock = true"), "{text}");
    let net = &locked.scene.routes[0];
    assert_eq!((net.net.as_str(), net.lock, net.current), ("VCC", true, true));
    assert_ne!(locked.scene.routes_stamp, scene.routes_stamp);

    // A change made from what the page read before is refused: the file changed.
    let stale = editor
        .pcb_routes(&change(&scene, lock("VCC", None, false)))
        .unwrap_err();
    assert_eq!(stale.status, 409, "{}", stale.message);

    // Another program adds a track to the record: the view reads it, drawn on the board.
    let from = pad_of(&locked.scene, "VCC");
    let to = [from[0] + 1.0, from[1]];
    rewrite(&root, |routes| {
        let record = routes
            .boards
            .get_mut(BOARD)
            .unwrap()
            .nets
            .get_mut("VCC")
            .unwrap();
        record.tracks.push(RouteTrack {
            layer: LayoutFace::Top,
            width: 0.3,
            from,
            mid: None,
            to,
            lock: false,
        });
    });
    let scene = editor.pcb_scene(BOARD).unwrap();
    let track = &scene.routes[0].tracks[0];
    assert!(track.drawn && !track.lock);
    assert!((track.from.x - (scene.origin_x + from[0])).abs() < 1e-9);
    assert!((track.to.y - (scene.origin_y + to[1])).abs() < 1e-9);
    assert_eq!(track.layer, "top");

    // Lock the track: deleting it is refused with the reason, and the file is as it was.
    let scene = editor
        .pcb_routes(&change(&scene, lock("VCC", Some(1), true)))
        .unwrap()
        .scene;
    assert!(scene.routes[0].tracks[0].lock);
    let before = fs::read_to_string(routes_file(&root)).unwrap();
    let refused = editor
        .pcb_routes(&change(&scene, delete("VCC", Some(1))))
        .unwrap_err();
    assert_eq!(refused.status, 422, "{}", refused.message);
    assert!(
        refused.message.contains("track 1 of net VCC is locked"),
        "{}",
        refused.message
    );
    assert_eq!(fs::read_to_string(routes_file(&root)).unwrap(), before);
    // A shape the record does not have is refused too.
    let missing = editor
        .pcb_routes(&change(&scene, delete("VCC", Some(4))))
        .unwrap_err();
    assert_eq!(missing.status, 422);
    assert!(
        missing.message.contains("net VCC has no track 4"),
        "{}",
        missing.message
    );

    // Unlocked, it is deleted; then the net unlocked, the empty record goes, and so does the file.
    let scene = editor
        .pcb_routes(&change(&scene, lock("VCC", Some(1), false)))
        .unwrap()
        .scene;
    let deleted = editor
        .pcb_routes(&change(&scene, delete("VCC", Some(1))))
        .unwrap();
    assert!(deleted.said.starts_with("deleted track 1 of net VCC"));
    assert_eq!(deleted.scene.routes[0].tracks, []);
    let gone = editor
        .pcb_routes(&change(&deleted.scene, lock("VCC", None, false)))
        .unwrap();
    assert_eq!(gone.scene.routes, []);
    assert!(!routes_file(&root).exists());
}

#[test]
fn a_record_not_used_as_written_is_a_warning_in_the_findings() {
    let (root, editor) = project("stale");
    let scene = editor.pcb_scene(BOARD).unwrap();
    editor
        .pcb_routes(&change(&scene, lock("GND", None, true)))
        .unwrap();
    assert!(
        editor
            .pcb_findings(BOARD)
            .unwrap()
            .findings
            .iter()
            .all(|f| f.source != FindingSource::Routes),
        "a record written now is current"
    );
    let at = pad_of(&scene, "GND");
    rewrite(&root, |routes| {
        let table = routes.boards.get_mut(BOARD).unwrap();
        let gnd = table.nets.get_mut("GND").unwrap();
        // As if the pads moved since the record was written.
        gnd.pads = "0000000000000000".into();
        gnd.tracks.push(RouteTrack {
            layer: LayoutFace::Bottom,
            width: 0.25,
            from: at,
            mid: None,
            to: [at[0], at[1] + 1.0],
            lock: true,
        });
        gnd.tracks.push(RouteTrack {
            layer: LayoutFace::Bottom,
            width: 0.25,
            from: at,
            mid: None,
            to: [at[0], at[1] - 1.0],
            lock: false,
        });
        let mut gone = gnd.clone();
        gone.lock = false;
        table.nets.insert("RENAMED".into(), gone);
    });
    let scene = editor.pcb_scene(BOARD).unwrap();
    let gnd = scene.routes.iter().find(|n| n.net == "GND").unwrap();
    assert!(!gnd.current);
    // The locked track is kept and drawn, the unlocked one dropped.
    assert_eq!(
        gnd.tracks.iter().map(|t| t.drawn).collect::<Vec<_>>(),
        [true, false]
    );
    let renamed = scene.routes.iter().find(|n| n.net == "RENAMED").unwrap();
    assert!(renamed.tracks.iter().all(|t| !t.drawn));

    let found = editor.pcb_findings(BOARD).unwrap().findings;
    let routes: Vec<_> = found
        .iter()
        .filter(|f| f.source == FindingSource::Routes)
        .collect();
    assert_eq!(routes.len(), 2, "{found:#?}");
    assert!(routes.iter().all(|f| f.severity == Severity::Warning));
    let moved = routes
        .iter()
        .find(|f| f.rule == "routes_pads_moved")
        .unwrap();
    assert!(
        moved.message.starts_with("main.routes.toml: the pads of net `GND` are not where"),
        "{}",
        moved.message
    );
    assert!(!moved.parts.is_empty(), "it names the parts of the net's pads");
    let no_net = routes.iter().find(|f| f.rule == "routes_no_net").unwrap();
    assert!(
        no_net.message.contains("net `RENAMED`, which the board does not have"),
        "{}",
        no_net.message
    );

    // Locking the net again writes the record with its pads as they are: no warning is left for it.
    let scene = editor
        .pcb_routes(&change(&scene, lock("GND", None, true)))
        .unwrap()
        .scene;
    assert!(scene.routes.iter().find(|n| n.net == "GND").unwrap().current);
    let found = editor.pcb_findings(BOARD).unwrap().findings;
    assert!(
        !found.iter().any(|f| f.rule == "routes_pads_moved"),
        "{found:#?}"
    );
}

#[test]
fn a_refused_change_is_a_422_with_the_reason_over_the_wire() {
    let (_root, editor) = project("wire");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || netsmith_editor::serve(listener, Arc::new(editor)));
    let body = |change: RoutesChange| {
        serde_json::to_string(&PcbRoutesEdit {
            board: BOARD.into(),
            stamp: scene.routes_stamp.clone(),
            change,
        })
        .unwrap()
    };
    let (status, answer) = http(
        port,
        "POST",
        "/api/pcb/routes",
        "X-Netsmith: 1\r\nContent-Type: application/json\r\nAccept: application/json\r\n",
        &body(delete("NOPE", None)),
    );
    assert_eq!(status, 422, "{answer}");
    assert!(answer.contains("has no net `NOPE`"), "{answer}");
    assert!(answer.contains("routes_refused"), "{answer}");
    let (status, answer) = http(
        port,
        "POST",
        "/api/pcb/routes",
        "X-Netsmith: 1\r\nContent-Type: application/json\r\n",
        &body(lock("VCC", None, true)),
    );
    assert_eq!(status, 200, "{answer}");
    assert!(answer.contains("\"said\":\"locked net VCC"), "{answer}");
}
