//! The routes file in the editor's PCB view (epic #599, R5): its records are in the scene with
//! what of them is drawn, a record that is not used as written is a finding, and a change from the
//! page (draw or move a track or via, delete, lock, unlock) is made as `netsmith routes` makes it,
//! refused with the reason when it is locked or breaks the rules and with 409 when the file
//! changed, and the view is placed again from the file. Each edit answers with the net's record
//! before and after, and putting it back is the edit's undo.

use std::{
    fs,
    net::{Ipv4Addr, TcpListener},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
};

use netsmith_editor::{
    Backend,
    api::{
        FindingSource, PcbPoint, PcbRoutesEdit, PcbScene, PcbWalk, RoutesChange, RoutesUndo, Severity,
    },
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
        changes: vec![change],
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
            changes: vec![change],
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

/// The pads of `net` on the sheet.
fn pads_of(scene: &PcbScene, net: &str) -> Vec<PcbPoint> {
    scene
        .parts
        .iter()
        .flat_map(|p| &p.pads)
        .filter(|p| p.net.as_deref() == Some(net))
        .map(|p| PcbPoint { x: p.x, y: p.y })
        .collect()
}

fn edit_of(scene: &PcbScene, changes: Vec<RoutesChange>) -> PcbRoutesEdit {
    PcbRoutesEdit {
        board: BOARD.into(),
        stamp: scene.routes_stamp.clone(),
        changes,
    }
}

fn track(net: &str, layer: &str, from: PcbPoint, to: PcbPoint) -> RoutesChange {
    RoutesChange::AddTrack {
        net: net.into(),
        layer: layer.into(),
        width: None,
        from,
        to,
    }
}

/// Where a path from `a` to `b` that goes at 45 degrees first and straight after turns.
fn diagonal(a: PcbPoint, b: PcbPoint) -> PcbPoint {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let m = dx.abs().min(dy.abs());
    PcbPoint {
        x: a.x + m * dx.signum(),
        y: a.y + m * dy.signum(),
    }
}

/// The point `t` of the way from `a` to `b`.
fn along(a: PcbPoint, b: PcbPoint, t: f64) -> PcbPoint {
    PcbPoint {
        x: a.x + t * (b.x - a.x),
        y: a.y + t * (b.y - a.y),
    }
}

/// The undo of an edit: its record put back as it was.
fn undo_of(undo: &RoutesUndo) -> RoutesChange {
    RoutesChange::Restore {
        net: undo.net.clone(),
        before: undo.after.clone(),
        after: undo.before.clone(),
    }
}

fn redo_of(undo: &RoutesUndo) -> RoutesChange {
    RoutesChange::Restore {
        net: undo.net.clone(),
        before: undo.before.clone(),
        after: undo.after.clone(),
    }
}

#[test]
fn a_track_drawn_in_the_view_creates_the_file_and_undo_and_redo_put_it_back() {
    let (root, editor) = project("draw");
    let scene = editor.pcb_scene(BOARD).unwrap();
    assert!(!routes_file(&root).exists());
    let width = scene
        .net_widths
        .iter()
        .find(|w| w.net == "VCC")
        .expect("the scene has VCC's width")
        .width;
    assert!(width > 0.0);
    assert!(scene.via_diameter > scene.via_drill && scene.via_drill > 0.0);
    // Two segments from one VCC pad to the other, on the bottom, by a corner of 45 degrees (the
    // rules refuse a right angle): one edit.
    let pads = pads_of(&scene, "VCC");
    assert_eq!(pads.len(), 2, "{pads:?}");
    let corner = diagonal(pads[0], pads[1]);
    let drawn = editor
        .pcb_routes(&edit_of(
            &scene,
            vec![
                track("VCC", "bottom", pads[0], corner),
                track("VCC", "bottom", corner, pads[1]),
            ],
        ))
        .unwrap_or_else(|e| panic!("{}", e.message));
    assert!(
        drawn
            .said
            .starts_with("added a track to net VCC (2 times); wrote main.routes.toml"),
        "{}",
        drawn.said
    );
    let text = fs::read_to_string(routes_file(&root)).unwrap();
    assert!(text.contains("[LedExample.nets.VCC]\npads = \""), "{text}");
    assert_eq!(text.matches("layer = \"bottom\"").count(), 2, "{text}");
    let record = &drawn.scene.routes[0];
    assert_eq!(record.tracks.len(), 2);
    assert!(record.tracks.iter().all(|t| t.drawn && t.width == width));
    // The step of undo: no record before, the record after.
    assert_eq!(drawn.undo.net, "VCC");
    assert_eq!(drawn.undo.before, None);
    assert_eq!(drawn.undo.after.as_deref(), Some(text.as_str()));

    // Undo: the record goes, and with it the file.
    let undone = editor
        .pcb_routes(&edit_of(&drawn.scene, vec![undo_of(&drawn.undo)]))
        .unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(undone.scene.routes, []);
    assert!(!routes_file(&root).exists());
    // Redo: the same file again.
    let redone = editor
        .pcb_routes(&edit_of(&undone.scene, vec![redo_of(&drawn.undo)]))
        .unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(fs::read_to_string(routes_file(&root)).unwrap(), text);
    assert_eq!(redone.scene.routes[0].tracks, record.tracks);
    // A second redo is refused: the record is not what the step says it was before.
    let twice = editor
        .pcb_routes(&edit_of(&redone.scene, vec![redo_of(&drawn.undo)]))
        .unwrap_err();
    assert_eq!(twice.status, 422, "{}", twice.message);
    assert!(
        twice.message.contains("changed since that edit"),
        "{}",
        twice.message
    );
    assert_eq!(fs::read_to_string(routes_file(&root)).unwrap(), text);
}

#[test]
fn a_track_end_and_a_via_are_moved_and_a_lock_or_the_rules_refuse_it() {
    let (root, editor) = project("move");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let pads = pads_of(&scene, "VCC");
    // A straight path from pad to pad, in two tracks that meet half way.
    let corner = along(pads[0], pads[1], 0.5);
    let scene = editor
        .pcb_routes(&edit_of(
            &scene,
            vec![
                track("VCC", "bottom", pads[0], corner),
                track("VCC", "bottom", corner, pads[1]),
            ],
        ))
        .unwrap_or_else(|e| panic!("{}", e.message))
        .scene;
    // A via where they meet, of the process's size.
    let placed = editor
        .pcb_routes(&edit_of(
            &scene,
            vec![RoutesChange::AddVia {
                net: "VCC".into(),
                at: corner,
            }],
        ))
        .unwrap_or_else(|e| panic!("{}", e.message));
    let via = &placed.scene.routes[0].vias[0];
    assert_eq!(
        (via.diameter, via.drill),
        (scene.via_diameter, scene.via_drill)
    );
    assert!(placed.said.starts_with("added a via to net VCC"));

    // The point dragged along the path: both tracks that meet there move, and the via with
    // them, in one edit.
    let moved_to = along(pads[0], pads[1], 0.4);
    let before = placed.scene.routes[0].clone();
    let moved = editor
        .pcb_routes(&edit_of(
            &placed.scene,
            vec![
                RoutesChange::MoveTrack {
                    net: "VCC".into(),
                    track: 1,
                    from: pads[0],
                    to: moved_to,
                },
                RoutesChange::MoveTrack {
                    net: "VCC".into(),
                    track: 2,
                    from: moved_to,
                    to: pads[1],
                },
                RoutesChange::MoveVia {
                    net: "VCC".into(),
                    via: 1,
                    at: moved_to,
                },
            ],
        ))
        .unwrap_or_else(|e| panic!("{}", e.message));
    let record = &moved.scene.routes[0];
    let at = |p: PcbPoint| (p.x - moved_to.x).abs() < 1e-5 && (p.y - moved_to.y).abs() < 1e-5;
    assert!(at(record.tracks[0].to) && at(record.tracks[1].from) && at(record.vias[0].at));
    assert!(
        moved
            .said
            .starts_with("moved track 1 of net VCC; moved track 2 of net VCC; moved via 1"),
        "{}",
        moved.said
    );
    // One undo puts all three back.
    let undone = editor
        .pcb_routes(&edit_of(&moved.scene, vec![undo_of(&moved.undo)]))
        .unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(undone.scene.routes[0], before);

    // A locked track is not moved, and an undo puts back a lock as it was.
    let locked = editor
        .pcb_routes(&edit_of(&undone.scene, vec![lock("VCC", Some(1), true)]))
        .unwrap();
    let file = fs::read_to_string(routes_file(&root)).unwrap();
    let refused = editor
        .pcb_routes(&edit_of(
            &locked.scene,
            vec![RoutesChange::MoveTrack {
                net: "VCC".into(),
                track: 1,
                from: pads[0],
                to: moved_to,
            }],
        ))
        .unwrap_err();
    assert_eq!(refused.status, 422, "{}", refused.message);
    assert!(
        refused.message.contains("track 1 of net VCC is locked"),
        "{}",
        refused.message
    );
    assert_eq!(fs::read_to_string(routes_file(&root)).unwrap(), file);
    let unlocked = editor
        .pcb_routes(&edit_of(&locked.scene, vec![undo_of(&locked.undo)]))
        .unwrap_or_else(|e| panic!("{}", e.message));
    assert!(!unlocked.scene.routes[0].tracks[0].lock);

    // A via moved onto a GND pad breaks the rules: refused with the check's errors.
    let gnd = pads_of(&unlocked.scene, "GND")[0];
    let file = fs::read_to_string(routes_file(&root)).unwrap();
    let shorted = editor
        .pcb_routes(&edit_of(
            &unlocked.scene,
            vec![RoutesChange::MoveVia {
                net: "VCC".into(),
                via: 1,
                at: gnd,
            }],
        ))
        .unwrap_err();
    assert_eq!(shorted.status, 422, "{}", shorted.message);
    assert!(
        shorted.message.contains("the design rules check finds"),
        "{}",
        shorted.message
    );
    assert_eq!(fs::read_to_string(routes_file(&root)).unwrap(), file);
}

#[test]
fn an_edit_is_of_one_net_and_a_track_is_on_an_outer_layer() {
    let (root, editor) = project("one-net");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let vcc = pads_of(&scene, "VCC")[0];
    let gnd = pads_of(&scene, "GND")[0];
    let near = |p: PcbPoint| PcbPoint { x: p.x, y: p.y + 0.5 };
    let two = editor
        .pcb_routes(&edit_of(
            &scene,
            vec![
                track("VCC", "top", vcc, near(vcc)),
                track("GND", "top", gnd, near(gnd)),
            ],
        ))
        .unwrap_err();
    assert_eq!(two.status, 422, "{}", two.message);
    assert!(two.message.contains("of one net"), "{}", two.message);
    let inner = editor
        .pcb_routes(&edit_of(
            &scene,
            vec![track("VCC", "inner1", vcc, near(vcc))],
        ))
        .unwrap_err();
    assert_eq!(inner.status, 400, "{}", inner.message);
    assert!(!routes_file(&root).exists());
}

fn walk_of(net: &str, layer: &str, from: PcbPoint, to: PcbPoint) -> PcbWalk {
    PcbWalk {
        board: BOARD.into(),
        net: net.into(),
        layer: layer.into(),
        width: None,
        from,
        to,
        heading: None,
        track: Vec::new(),
    }
}

#[test]
fn a_track_is_walked_round_a_part_and_the_corners_it_gives_pass_the_check() {
    let (root, editor) = project("walk");
    let scene = editor.pcb_scene(BOARD).unwrap();
    // A ground track straight through R1's first pad (a VCC pad, on the top layer only) is not
    // free on the top layer, and is on the bottom, where R1 has no copper.
    let r1 = &scene.parts.iter().find(|p| p.refdes == "R1").unwrap().pads;
    let (at_y, x) = (r1[0].y, r1[0].x);
    let (from, to) = (PcbPoint { x: x - 1.5, y: at_y }, PcbPoint { x: x + 1.5, y: at_y });
    let top = editor.pcb_walk(&walk_of("GND", "top", from, to)).unwrap();
    let points = top.points.clone().unwrap_or_else(|| panic!("{top:?}"));
    assert!(points.len() >= 3, "{points:?}");
    assert_eq!((points[0], points[points.len() - 1]), (from, to));
    assert!(top.tried > 0 && top.reason.is_none(), "{top:?}");
    let bottom = editor.pcb_walk(&walk_of("GND", "bottom", from, to)).unwrap();
    assert_eq!(bottom.points, Some(vec![from, to]));
    assert_eq!(bottom.tried, 0);

    // The corners are ordinary track corners: ended, the track passes the same check, and the
    // walk wrote nothing before.
    assert!(!routes_file(&root).exists());
    let changes = points
        .windows(2)
        .map(|w| track("GND", "top", w[0], w[1]))
        .collect();
    let drawn = editor.pcb_routes(&edit_of(&scene, changes)).unwrap();
    assert!(drawn.said.contains("added a track"), "{}", drawn.said);
    // With the track drawn it is copper of the board: another net may not start on it.
    let middle = along(points[0], points[1], 0.5);
    let after = editor
        .pcb_walk(&walk_of("VCC", "top", middle, along(middle, to, 0.5)))
        .unwrap();
    assert_eq!(after.points, None);
    assert!(
        after.reason.as_deref().is_some_and(|r| r.contains("last corner")),
        "{after:?}"
    );
}

#[test]
fn a_walk_that_has_no_way_says_why_and_a_wrong_ask_is_a_400() {
    let (_root, editor) = project("walk-refused");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let vcc = pads_of(&scene, "VCC")[0];
    let gnd = pads_of(&scene, "GND")[0];
    // The end is on a pad of another net: an answer, with the reason, not an error.
    let blocked = editor.pcb_walk(&walk_of("VCC", "top", vcc, gnd)).unwrap();
    assert_eq!(blocked.points, None);
    assert!(
        blocked.reason.as_deref().is_some_and(|r| r.contains("no room")),
        "{blocked:?}"
    );
    let net = editor.pcb_walk(&walk_of("NOPE", "top", vcc, gnd)).unwrap_err();
    assert_eq!(net.status, 400, "{}", net.message);
    let layer = editor.pcb_walk(&walk_of("VCC", "inner1", vcc, gnd)).unwrap_err();
    assert_eq!(layer.status, 400, "{}", layer.message);
}

#[test]
fn a_walk_is_asked_over_the_wire_and_answers_200_with_no_way_too() {
    let (_root, editor) = project("walk-wire");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let vcc = pads_of(&scene, "VCC")[0];
    let gnd = pads_of(&scene, "GND")[0];
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || netsmith_editor::serve(listener, Arc::new(editor)));
    let ask = |to: PcbPoint| {
        http(
            port,
            "POST",
            "/api/pcb/walk",
            "X-Netsmith: 1\r\nContent-Type: application/json\r\nAccept: application/json\r\n",
            &serde_json::to_string(&walk_of("VCC", "top", vcc, to)).unwrap(),
        )
    };
    let (status, answer) = ask(PcbPoint { x: vcc.x, y: vcc.y + 1.0 });
    assert_eq!(status, 200, "{answer}");
    assert!(answer.contains("\"points\":[{"), "{answer}");
    let (status, answer) = ask(gnd);
    assert_eq!(status, 200, "{answer}");
    assert!(answer.contains("\"points\":null"), "{answer}");
    assert!(answer.contains("no room"), "{answer}");
}

/// A walk of `net` with a track `width` wide at `at`: a disc that covers the router's copper
/// round it, whether or not the track fits there (what it crosses is asked all the same).
fn sweep(net: &str, at: PcbPoint, width: f64) -> PcbWalk {
    PcbWalk {
        width: Some(width),
        ..walk_of(net, "top", at, at)
    }
}

#[test]
fn the_routers_copper_gives_way_to_a_walk_and_is_named_when_it_is_crossed() {
    let (_root, editor) = project("walk-router");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let centre = PcbPoint {
        x: scene.outline.x + scene.outline.width / 2.0,
        y: scene.outline.y + scene.outline.height / 2.0,
    };
    let big = scene.outline.width.max(scene.outline.height) * 2.0;
    // Not routed: there is no router copper to cross.
    let before = editor.pcb_walk(&sweep("GND", centre, big)).unwrap();
    assert!(before.crossed.tracks.is_empty() && before.crossed.vias.is_empty());

    let say: netsmith_editor::Say = Arc::new(|_: &str| {});
    let report = editor.route(BOARD, &say).unwrap();
    assert!(report.routed && report.tracks > 0, "{report:?}");
    let after = editor.pcb_walk(&sweep("GND", centre, big)).unwrap();
    let crossed = &after.crossed;
    assert!(!crossed.tracks.is_empty(), "{crossed:?}");
    assert!(
        crossed.tracks.iter().all(|t| t.net != "GND" && t.layer == "top"),
        "copper of the walked net and of the other layer is not crossed: {crossed:?}"
    );
    let bottom = editor
        .pcb_walk(&PcbWalk {
            layer: "bottom".into(),
            ..sweep("VCC", centre, big)
        })
        .unwrap();
    assert!(
        bottom.crossed.tracks.iter().all(|t| t.net != "VCC" && t.layer == "bottom"),
        "{:?}",
        bottom.crossed
    );
    // The router's copper is no obstacle to the way: a straight segment over it takes no search.
    let [a, b] = [pads_of(&scene, "VCC")[0], pads_of(&scene, "VCC")[1]];
    let (c, d) = (along(a, b, 0.3), along(a, b, 0.7));
    let through = editor.pcb_walk(&walk_of("GND", "top", c, d)).unwrap();
    assert_eq!(through.points, Some(vec![c, d]), "{through:?}");
    assert_eq!(through.tried, 0);

    // An edit of the routes file discards the router's copper: nothing is crossed after it.
    let drawn = editor
        .pcb_routes(&edit_of(&editor.pcb_scene(BOARD).unwrap(), vec![track("VCC", "top", a, b)]));
    drawn.unwrap();
    let gone = editor.pcb_walk(&sweep("GND", centre, big)).unwrap();
    assert!(gone.crossed.tracks.is_empty() && gone.crossed.vias.is_empty(), "{gone:?}");
}
