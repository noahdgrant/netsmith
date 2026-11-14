//! The pours and zones in the editor's 2D PCB view (epic #599): `GET /api/pcb/pours` answers with the
//! fill of the board's pours, planes and routes-file zones, as the Gerbers and the 3D view have
//! it, by copper layer and net, as paths the page draws under the tracks and pads.

use std::{fs, path::Path};

use netsmith_editor::{
    Backend,
    api::{PcbPours, PcbRoutesEdit, RoutesChange},
};
use netsmith_fabricate::project::Target;
use netsmith_pcb::{Routes, ZoneIslands, ZonePads};
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const BOARD: &str = "LedExample";

/// A project with `examples/led.nsm` (and its pick lock) as its one top-level file.
fn led_project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-pours-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"pours\"\n").unwrap();
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

/// The four-layer example: ground on both outer layers and on the first inner layer, 3.3 V on the
/// second.
fn pours_project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-pours-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"pours\"\n").unwrap();
    fs::write(
        root.join("src/main.nsm"),
        include_str!("../../../examples/outline_and_pours.nsm"),
    )
    .unwrap();
    fs::write(
        root.join("src/main.physical.nsm"),
        include_str!("../../../examples/outline_and_pours.physical.nsm"),
    )
    .unwrap();
    fs::write(
        root.join("src/main.pcb.toml"),
        include_str!("../../../examples/outline_and_pours.pcb.toml"),
    )
    .unwrap();
    fs::write(
        root.join("src/main.lock.toml"),
        include_str!("../../../examples/outline_and_pours.lock.toml"),
    )
    .unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

/// The rings of a path of `M`, `L` and `Z`, as lists of points.
fn rings(d: &str) -> Vec<Vec<(f64, f64)>> {
    d.split('Z')
        .filter(|s| !s.is_empty())
        .map(|ring| {
            ring.split(['M', 'L'])
                .filter(|s| !s.is_empty())
                .map(|p| {
                    let (x, y) = p.split_once(' ').unwrap_or_else(|| panic!("a point: {p}"));
                    (x.parse().unwrap(), y.parse().unwrap())
                })
                .collect()
        })
        .collect()
}

/// Twice the signed area of a ring: positive when it runs clockwise on the screen.
fn shoelace(ring: &[(f64, f64)]) -> f64 {
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum()
}

/// The area the path fills, the holes taken away.
fn filled(d: &str) -> f64 {
    rings(d).iter().map(|r| shoelace(r) / 2.0).sum()
}

fn layer<'a>(pours: &'a PcbPours, name: &str) -> &'a netsmith_editor::api::PcbPourLayer {
    pours
        .layers
        .iter()
        .find(|l| l.layer == name)
        .unwrap_or_else(|| panic!("no fill on {name}: {:?}", pours.layers.iter().map(|l| &l.layer).collect::<Vec<_>>()))
}

#[test]
fn a_board_with_no_pour_has_no_fill() {
    let (_root, editor) = led_project("none");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let pours = editor.pcb_pours(BOARD).unwrap();
    assert_eq!(pours.board, BOARD);
    assert_eq!(pours.layers, []);
    assert_eq!(pours.error, None);
    // It says which placement it filled, so the page can tell a stale answer.
    assert_eq!(pours.layout_stamp, scene.layout_stamp);
    assert_eq!(pours.routes_stamp, scene.routes_stamp);
    assert_eq!(pours.routed, scene.routed);
    assert_eq!(editor.pcb_pours("Nope").unwrap_err().status, 404);
}

#[test]
fn the_pours_and_planes_of_a_board_are_filled_by_layer_and_net() {
    let (_root, editor) = pours_project("planes");
    let scene = editor.pcb_scene("OutlineAndPours").unwrap();
    let pours = editor.pcb_pours("OutlineAndPours").unwrap();
    assert_eq!(pours.error, None);
    assert_eq!(
        pours.layers.iter().map(|l| l.layer.as_str()).collect::<Vec<_>>(),
        ["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"],
        "front to back"
    );
    let net = |name: &str, net: &str| {
        layer(&pours, name)
            .nets
            .iter()
            .find(|n| n.net.as_deref() == Some(net))
            .unwrap_or_else(|| panic!("no {net} on {name}"))
    };
    let outline = scene.outline;
    for (name, of) in [("F.Cu", "GND"), ("In1.Cu", "GND"), ("In2.Cu", "3V3"), ("B.Cu", "GND")] {
        let d = &net(name, of).d;
        let rs = rings(d);
        assert!(!rs.is_empty(), "{name}");
        // On the sheet, inside the outline.
        for (x, y) in rs.iter().flatten() {
            assert!(
                *x >= outline.x - 1e-6
                    && *x <= outline.x + outline.width + 1e-6
                    && *y >= outline.y - 1e-6
                    && *y <= outline.y + outline.height + 1e-6,
                "{name}: ({x}, {y}) is off the board"
            );
        }
        // The outlines run clockwise on the screen and the holes the other way, so the path
        // fills by the non-zero rule: its area is that of the outlines less the holes.
        let area = filled(d);
        assert!(area > 0.0 && area < outline.width * outline.height, "{name}: {area}");
    }
    // A pour is cut round other nets' pads and tracks: the ground pour on the top layer has
    // holes, a plane with no pad of another net near it far fewer.
    let top = rings(&net("F.Cu", "GND").d);
    assert!(top.iter().any(|r| shoelace(r) < 0.0), "the top pour has holes");
    // Nothing but the fill itself is in the answer: no pads, tracks or vias.
    assert!(pours.layers.iter().all(|l| l.nets.iter().all(|n| !n.d.is_empty())));
}

/// A zone of `net` on the top layer in the routes file, in its frame (mm from the outline's
/// corner), written the way another program would.
fn write_zone(root: &Path, net: &str, outline: &[[f64; 2]], holes: Vec<Vec<[f64; 2]>>) {
    let file = root.join("src/main.routes.toml");
    let mut routes = Routes::parse(&fs::read_to_string(&file).unwrap(), "routes").unwrap();
    let record = routes
        .boards
        .get_mut(BOARD)
        .unwrap()
        .nets
        .get_mut(net)
        .unwrap();
    record.zones.push(netsmith_pcb::RouteZone {
        layers: vec!["top".into()],
        priority: 0,
        clearance: 0.2,
        min_width: 0.15,
        pads: ZonePads::default(),
        thermal_gap: 0.3,
        spoke_width: 0.3,
        islands: ZoneIslands::default(),
        island_min_area: None,
        outline: outline.to_vec(),
        holes,
        lock: false,
    });
    fs::write(&file, routes.to_text()).unwrap();
}

#[test]
fn a_zone_of_the_routes_file_is_filled_where_it_is_and_with_its_holes() {
    let (root, editor) = led_project("zone");
    let scene = editor.pcb_scene(BOARD).unwrap();
    // A record for the supply net (the file is written with its pads as they are), then a zone in it.
    let edit = PcbRoutesEdit {
        board: BOARD.into(),
        stamp: scene.routes_stamp.clone(),
        changes: vec![RoutesChange::Lock {
            net: "VCC".into(),
            track: None,
            via: None,
            lock: true,
        }],
    };
    editor.pcb_routes(&edit).unwrap();
    let (w, h) = (scene.outline.width, scene.outline.height);
    let (x0, y0, x1, y1) = (0.5, 0.5, w - 0.5, h - 0.5);
    write_zone(
        &root,
        "VCC",
        &[[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
        vec![vec![[1.0, 1.0], [1.0, 1.4], [1.4, 1.4], [1.4, 1.0]]],
    );
    // The view reads the file the next time the scene is made again.
    let scene = editor.pcb_scene(BOARD).unwrap();
    let pours = editor.pcb_pours(BOARD).unwrap();
    assert_eq!(pours.error, None);
    assert_eq!(pours.routes_stamp, scene.routes_stamp);
    assert_eq!(
        pours.layers.iter().map(|l| l.layer.as_str()).collect::<Vec<_>>(),
        ["F.Cu"],
        "only the layer the zone is on has a fill"
    );
    let vcc = layer(&pours, "F.Cu")
        .nets
        .iter()
        .find(|n| n.net.as_deref() == Some("VCC"))
        .expect("the supply net's fill");
    let (ox, oy) = (scene.origin_x, scene.origin_y);
    let rs = rings(&vcc.d);
    assert!(!rs.is_empty());
    for (x, y) in rs.iter().flatten() {
        assert!(
            *x >= ox + x0 - 1e-6 && *x <= ox + x1 + 1e-6 && *y >= oy + y0 - 1e-6 && *y <= oy + y1 + 1e-6,
            "({x}, {y}) is outside the zone"
        );
    }
    // The fill is the zone less its hole and the clearances round the other nets' pads: less than
    // the zone's own area, and more than half of it on a board this empty.
    let zone = (x1 - x0) * (y1 - y0) - 0.16;
    let area = filled(&vcc.d);
    assert!(area < zone && area > zone / 2.0, "{area} of a zone of {zone}");
    // The hole is a ring of its own, the other way round.
    assert!(rs.iter().any(|r| shoelace(r) < 0.0), "the zone's hole is a hole of the fill");
}
