//! The via tool on a board of an HDI process (issue #1071): the scene lists the vias the process
//! makes, a via placed with two layers is that blind or buried via of the process, of its size,
//! written with its layers, and a span the process does not make is refused.

use std::fs;

use netsmith_editor::Backend;
use netsmith_editor::api::{PcbPoint, PcbRoutesEdit, PcbShove, PcbShoveVia, RoutesChange};
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const BOARD: &str = "Dense";

#[test]
fn a_via_placed_with_two_layers_is_the_processs_blind_via_of_that_span() {
    let root = TempDir::new("editor-blind-vias");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"hdi\"\n").unwrap();
    fs::write(
        root.join("src/main.nsm"),
        include_str!("../../netsmith-tools/tests/fixtures/hdi_dense.nsm"),
    )
    .unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    let scene = editor.pcb_scene(BOARD).unwrap();
    // The blind vias of each face, the buried via of the core, then the through via.
    let offers: Vec<(Vec<String>, f64, f64)> = scene
        .via_offers
        .iter()
        .map(|o| (o.layers.clone(), o.diameter, o.drill))
        .collect();
    let words = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
    assert_eq!(
        offers,
        [
            (words("top", "inner1"), 0.25, 0.1),
            (words("inner4", "bottom"), 0.25, 0.1),
            (words("inner1", "inner4"), scene.via_diameter, scene.via_drill),
            (Vec::new(), scene.via_diameter, scene.via_drill),
        ]
    );
    // A corner of the board, 1 mm in, the one farthest from every pad.
    let o = &scene.outline;
    let corners = [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(i, j)| PcbPoint {
        x: o.x + 1.0 + f64::from(i) * (o.width - 2.0),
        y: o.y + 1.0 + f64::from(j) * (o.height - 2.0),
    });
    let pads: Vec<(f64, f64)> = scene
        .parts
        .iter()
        .flat_map(|p| &p.pads)
        .map(|p| (p.x, p.y))
        .collect();
    let room = |c: &PcbPoint| {
        pads.iter()
            .map(|&(x, y)| (c.x - x).hypot(c.y - y))
            .fold(f64::MAX, f64::min)
    };
    let at = corners
        .into_iter()
        .max_by(|a, b| room(a).total_cmp(&room(b)))
        .unwrap();
    let via = |layers: Option<Vec<String>>| PcbRoutesEdit {
        board: BOARD.into(),
        stamp: scene.routes_stamp.clone(),
        changes: vec![RoutesChange::AddVia {
            net: "GND".into(),
            at,
            layers,
        }],
    };
    // The room the via tool asks for is a blind via's: on its own layers.
    let room = editor
        .pcb_shove(&PcbShove {
            board: BOARD.into(),
            net: "GND".into(),
            tracks: Vec::new(),
            vias: vec![PcbShoveVia {
                at,
                diameter: None,
                layers: Some(words("top", "inner1")),
            }],
            moving: Vec::new(),
            shove: false,
        })
        .unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(room.reason, None);
    // No via joins the top to the second inner layer: refused, with the spans there are.
    let refused = editor
        .pcb_routes(&via(Some(words("top", "inner2"))))
        .unwrap_err();
    assert_eq!(refused.status, 422, "{}", refused.message);
    assert!(
        refused
            .message
            .contains("the process makes no via from `top` to `inner2`")
            && refused.message.contains("top,inner1"),
        "{}",
        refused.message
    );
    let placed = editor
        .pcb_routes(&via(Some(words("top", "inner1"))))
        .unwrap_or_else(|e| panic!("{}", e.message));
    assert!(
        placed
            .said
            .starts_with("added a blind via from top to inner1 to net GND"),
        "{}",
        placed.said
    );
    let gnd = placed
        .scene
        .routes
        .iter()
        .find(|n| n.net == "GND")
        .expect("a record of GND");
    let v = &gnd.vias[0];
    assert_eq!(
        (v.layers.clone(), v.diameter, v.drill),
        (words("top", "inner1"), 0.25, 0.1)
    );
    let text = fs::read_to_string(root.join("src/main.routes.toml")).unwrap();
    assert!(text.contains("layers = [\"top\", \"inner1\"]"), "{text}");
}
