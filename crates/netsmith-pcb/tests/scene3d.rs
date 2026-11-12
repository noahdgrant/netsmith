//! The 3D scene of a board (ADR 0105, `netsmith_pcb::scene3d`): an invented 30 x 20 mm board with
//! a 4 x 2 mm box on each face, a second box on the top, and a two-pin through-hole row, each
//! placed by hand. Heights are set on the placed parts, as the design would state them.

use std::collections::BTreeMap;

use crate::common;

use netsmith_core::{Design, Features, Outline, Part};
use netsmith_kicad::testlib::TempDir;
use netsmith_pcb::protrusion::Protrusion;
use netsmith_pcb::scene3d::{HeightProvenance, Pins, Scene};
use netsmith_pcb::{Board, BoardKeepout, FootprintLibrary, PlaceOptions, Pt, Side};

/// A box 4 mm along x and 2 mm along y, its origin at its centre, a pad at each end.
const BOX: &str = "(footprint \"Box\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
     (property \"Reference\" \"REF**\" (at 0 -3 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (property \"Value\" \"Box\" (at 0 3 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (attr smd) \
     (pad \"1\" smd rect (at -1.5 0) (size 0.8 1.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
     (pad \"2\" smd rect (at 1.5 0) (size 0.8 1.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
     (fp_rect (start -2 -1) (end 2 1) (stroke (width 0.1) (type solid)) (fill none) (layer \"F.Fab\")) \
     (fp_rect (start -2.2 -1.2) (end 2.2 1.2) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n";

/// Two plated holes 2.54 mm apart along y.
const ROW: &str = "(footprint \"Row2\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
     (property \"Reference\" \"REF**\" (at 0 -3 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (property \"Value\" \"Row2\" (at 0 3 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (attr through_hole) \
     (pad \"1\" thru_hole circle (at 0 0) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\")) \
     (pad \"2\" thru_hole circle (at 0 2.54) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\")) \
     (fp_rect (start -1.27 -1.27) (end 1.27 3.81) (stroke (width 0.1) (type solid)) (fill none) (layer \"F.Fab\")) \
     (fp_rect (start -1.8 -1.8) (end 1.8 4.3) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n";

fn part(refdes: &str, kind: &str, path: &str) -> Part {
    Part {
        provides: Vec::new(),
        refdes: refdes.into(),
        kind: kind.into(),
        value: kind.into(),
        package: Some(kind.into()),
        path: path.into(),
        pins: Vec::new(),
    }
}

/// The board, placed by the placer and then moved by hand: `a` on the top at (10, 5) from the
/// outline's top-left corner, `b` on the top at (22, 5), `under` on the bottom at (10, 15) and
/// the row on the top at (25, 12), all unturned.
fn board() -> Board {
    let folder = TempDir::new("scene3d");
    std::fs::write(folder.join("Box.kicad_mod"), BOX).unwrap();
    std::fs::write(folder.join("Row2.kicad_mod"), ROW).unwrap();
    let mapping = "[[part]]\nkind = \"Box\"\npackage = \"Box\"\nsymbol = \"t:R\"\nfootprint = \"t:Box\"\n\
                   [[part]]\nkind = \"Row2\"\npackage = \"Row2\"\nsymbol = \"t:R\"\nfootprint = \"t:Row2\"\n";
    let mapping = netsmith_kicad::Mapping::parse(mapping, "scene3d", std::path::Path::new("."))
        .unwrap_or_else(|e| panic!("{e}"));
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints())
        .with_library("t", folder.path());
    let design = Design {
        interfaces: Vec::new(),
        notes: Vec::new(),
        name: "Scene".into(),
        process: Some(common::process()),
        features: Features {
            outline: Some(Outline {
                width: (30.0, 30.0),
                height: (20.0, 20.0),
                corner_radius: 0.0,
            }),
            ..Features::default()
        },
        parts: vec![
            part("U1", "Box", "a"),
            part("U2", "Box", "b"),
            part("U3", "Box", "under"),
            part("J1", "Row2", "row"),
        ],
        nets: Vec::new(),
    };
    let mut board =
        Board::from_design_placed(&design, &mapping, &library, &PlaceOptions::default())
            .unwrap_or_else(|e| panic!("{e}"));
    let origin = board.outline.min;
    for (path, x, y, side, height) in [
        ("a", 10.0, 5.0, Side::Front, Some(2.0)),
        ("b", 22.0, 5.0, Side::Front, Some(3.0)),
        ("under", 10.0, 15.0, Side::Back, Some(1.2)),
        ("row", 25.0, 12.0, Side::Front, None),
    ] {
        let p = board.parts.iter_mut().find(|p| p.path == path).unwrap();
        p.at = origin + Pt::new(x, y);
        p.rotation = 0.0;
        p.side = side;
        p.height = height;
    }
    board
}

/// A height limit at `(x, y)` from the outline's corner, `w` by `h`, on `layers`.
fn limit(name: &str, at: (f64, f64), size: (f64, f64), layers: &[&str], max: f64) -> BoardKeepout {
    BoardKeepout {
        name: name.into(),
        part: None,
        at: Some(Pt::new(at.0, at.1)),
        size,
        layers: layers.iter().map(|l| l.to_string()).collect(),
        tracks: false,
        vias: false,
        copper: false,
        parts: false,
        max_height: Some(max),
        drawn: false,
    }
}

fn scene(board: &Board) -> Scene {
    let provenance = BTreeMap::from([("b".to_string(), HeightProvenance::Catalog)]);
    Scene::of(board, &provenance)
}

#[test]
fn the_board_is_its_outline_and_stackup_with_the_top_face_at_zero() {
    let s = scene(&board());
    assert_eq!((s.board.width, s.board.height), (30.0, 20.0));
    assert_eq!(s.board.thickness, 1.6);
    let first = &s.board.layers[0];
    let last = s.board.layers.last().unwrap();
    assert_eq!((first.name.as_str(), first.z_top), ("F.Mask", 0.0));
    assert_eq!((last.name.as_str(), last.z_bottom), ("B.Mask", -1.6));
    // Each layer starts where the one above it ends.
    for pair in s.board.layers.windows(2) {
        assert_eq!(pair[0].z_bottom, pair[1].z_top, "{pair:?}");
    }
    // The two plated holes of the row, by path and pad, in plan with y up.
    let holes: Vec<_> = s
        .board
        .holes
        .iter()
        .map(|h| (h.from, h.diameter, h.plated))
        .collect();
    assert_eq!(holes.len(), 2);
    assert_eq!((holes[0].0.x, holes[0].0.y), (25.0, 8.0));
    assert_eq!((holes[1].0.x, holes[1].0.y), (25.0, 5.46));
    assert!(holes.iter().all(|h| h.1 == 1.0 && h.2));
}

#[test]
fn a_body_stands_up_from_the_top_face_and_hangs_down_from_the_bottom() {
    let s = scene(&board());
    let a = s.body("a").unwrap();
    assert_eq!(a.z_range(), (Some(0.0), Some(2.0)));
    assert_eq!(a.provenance, HeightProvenance::Stated);
    // At (10, 5) from the top-left, y down: (10, 15) in plan.
    assert_eq!((a.bounds.min.x, a.bounds.min.y), (8.0, 14.0));
    assert_eq!((a.bounds.max.x, a.bounds.max.y), (12.0, 16.0));
    let under = s.body("under").unwrap();
    assert_eq!(under.z_range(), (Some(-2.8), Some(-1.6)));
    assert_eq!(s.body("b").unwrap().provenance, HeightProvenance::Catalog);
    // A part with no height has none: no zero and no guess.
    let row = s.body("row").unwrap();
    assert_eq!(row.z_range(), (Some(0.0), None));
    assert_eq!(row.provenance, HeightProvenance::Unknown);
    let c = s.coverage();
    assert_eq!((c.known, c.total, c.stated, c.catalog), (3, 4, 2, 1));
    assert_eq!(s.tallest(Side::Front).unwrap().path, "b");
    assert_eq!(s.tallest(Side::Back).unwrap().path, "under");
    // The bodies are by path.
    let paths: Vec<&str> = s.bodies.iter().map(|b| b.path.as_str()).collect();
    assert_eq!(paths, ["a", "b", "row", "under"]);
}

#[test]
fn a_turned_part_has_its_turned_corners() {
    let mut board = board();
    board
        .parts
        .iter_mut()
        .find(|p| p.path == "a")
        .unwrap()
        .rotation = 90.0;
    let a = scene(&board).body("a").unwrap().clone();
    // 2 mm along x and 4 mm along y once turned.
    assert_eq!(a.bounds.max.x - a.bounds.min.x, 2.0);
    assert_eq!(a.bounds.max.y - a.bounds.min.y, 4.0);
    assert_eq!(a.rotation, 90.0);
}

#[test]
fn the_pins_below_the_board_are_known_only_where_the_design_states_them() {
    let mut board = board();
    assert!(matches!(
        scene(&board).body("row").unwrap().pins,
        Pins::Unknown { .. }
    ));
    assert_eq!(scene(&board).body("a").unwrap().pins, Pins::None);
    board
        .parts
        .iter_mut()
        .find(|p| p.path == "row")
        .unwrap()
        .protrusion = Some(Protrusion {
        length: 1.5,
        source: "scene.physical.nsm:3".into(),
    });
    match &scene(&board).body("row").unwrap().pins {
        Pins::Known {
            z_top,
            z_bottom,
            length,
            ..
        } => assert_eq!((*z_top, *z_bottom, *length), (-1.6, -3.1, 1.5)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_limit_names_the_tallest_part_under_it_its_margin_and_the_unknown_ones() {
    let mut board = board();
    // Over `a` and the row on the top, and over `under` on the bottom.
    board.keepouts = vec![
        limit("lid", (0.0, 0.0), (30.0, 20.0), &["top"], 2.5),
        limit("floor", (5.0, 12.0), (10.0, 6.0), &["bottom"], 1.0),
    ];
    let s = scene(&board);
    let names: Vec<&str> = s.limits.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["floor", "lid"]);
    let lid = &s.limits[1];
    assert_eq!(lid.faces.len(), 1);
    let top = &lid.faces[0];
    assert_eq!((top.z_bottom, top.z_top), (0.0, 2.5));
    assert_eq!(top.tallest.as_ref().unwrap().path, "b");
    assert_eq!(top.margin, Some(-0.5));
    assert_eq!(top.unknown, ["row"]);
    let floor = &s.limits[0].faces[0];
    assert_eq!(
        (floor.side, floor.z_bottom, floor.z_top),
        (Side::Back, -2.6, -1.6)
    );
    assert_eq!(floor.margin, Some(-0.2));
    // The findings are the placement check's own.
    let tall: Vec<(&str, &str)> = s
        .findings
        .too_tall
        .iter()
        .map(|t| (t.path.as_str(), t.keepout.as_str()))
        .collect();
    assert_eq!(tall, [("b", "lid"), ("under", "floor")]);
    let unchecked: Vec<&str> = s
        .findings
        .unchecked
        .iter()
        .map(|u| u.path.as_str())
        .collect();
    assert_eq!(unchecked, ["row"]);
}

#[test]
fn two_bodies_on_one_face_that_share_space_are_an_overlap_and_faces_apart_are_not() {
    let mut board = board();
    assert!(scene(&board).findings.overlaps.is_empty());
    // `b` half over `a`, on the same face; `under` is right below `a` on the other face.
    let origin = board.outline.min;
    board.parts.iter_mut().find(|p| p.path == "b").unwrap().at = origin + Pt::new(12.0, 5.0);
    let s = scene(&board);
    assert_eq!(s.findings.overlaps.len(), 1, "{:?}", s.findings.overlaps);
    let o = &s.findings.overlaps[0];
    assert_eq!(
        (o.a.as_str(), o.b.as_str(), o.side),
        ("a", "b", Side::Front)
    );
    assert_eq!((o.z_bottom, o.z_top), (0.0, 2.0));
    // A body of unknown height overlaps nothing that can be told.
    board
        .parts
        .iter_mut()
        .find(|p| p.path == "b")
        .unwrap()
        .height = None;
    assert!(scene(&board).findings.overlaps.is_empty());
}

#[test]
fn the_same_board_gives_the_same_scene() {
    assert_eq!(scene(&board()), scene(&board()));
}

/// The volume a closed mesh holds: positive when its triangles face out.
fn volume(m: &netsmith_pcb::mesh::Mesh) -> f64 {
    m.indices
        .chunks(3)
        .map(|t| {
            let [a, b, c] = [0, 1, 2].map(|k| m.positions[t[k] as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

/// The board with `a` (on the top) and `under` (on the bottom) given the generated footprint
/// `id`, turned a quarter, `height` mm tall.
fn shaped(id: &str, height: Option<f64>) -> Board {
    let mut board = board();
    for path in ["a", "under"] {
        let p = board.parts.iter_mut().find(|p| p.path == path).unwrap();
        p.footprint = netsmith_pcb::generate::generate(id).unwrap();
        p.rotation = 90.0;
        p.height = height;
    }
    board
}

#[test]
fn a_shaped_part_lands_where_its_box_was_on_either_face_and_turned() {
    use netsmith_pcb::mesh::{Material, of_scene};
    for (id, h) in [
        ("Resistor_SMD:R_0603_1608Metric", 0.5),
        ("Package_SO:SOIC-8_3.9x4.9mm_P1.27mm", 1.75),
    ] {
        let board = shaped(id, Some(h));
        let s = scene(&board);
        let meshes = of_scene(&s);
        let (left, bottom) = (board.outline.min.x, board.outline.max.y);
        for path in ["a", "under"] {
            let body = s.body(path).unwrap();
            assert!(body.shape.is_some(), "{id} {path}");
            let mine: Vec<_> = meshes.iter().filter(|m| m.id == path).collect();
            assert!(mine.iter().any(|m| m.material == Material::Metal), "{id}");
            for m in &mine {
                assert!(
                    volume(m) > 0.0,
                    "{id} {path}: a {} mesh is inside out",
                    m.material
                );
            }
            let b = netsmith_pcb::mesh::bounds_of(
                &mine.iter().map(|m| (*m).clone()).collect::<Vec<_>>(),
            )
            .unwrap();
            // In plan: inside the body box and the pads, as the scene puts them.
            let placed = board.parts.iter().find(|p| p.path == path).unwrap();
            let mut env = body.bounds;
            for pad in &placed.footprint.pads {
                let r = placed.pad_bounds(pad);
                env.min.x = env.min.x.min(r.min.x - left);
                env.max.x = env.max.x.max(r.max.x - left);
                env.min.y = env.min.y.min(bottom - r.max.y);
                env.max.y = env.max.y.max(bottom - r.min.y);
            }
            let eps = 1e-6;
            assert!(
                b.min[0] >= env.min.x - eps
                    && b.max[0] <= env.max.x + eps
                    && b.min[1] >= env.min.y - eps
                    && b.max[1] <= env.max.y + eps,
                "{id} {path}: {b:?} outside {env:?}"
            );
            // The body box's middle is the shape's: nothing is turned the wrong way.
            let mid = |lo: f64, hi: f64| 0.5 * (lo + hi);
            assert!(
                (mid(b.min[0], b.max[0]) - mid(body.bounds.min.x, body.bounds.max.x)).abs() < 1e-6
            );
            assert!(
                (mid(b.min[1], b.max[1]) - mid(body.bounds.min.y, body.bounds.max.y)).abs() < 1e-6
            );
            // Turned a quarter, the part is longer along y than along x.
            assert!(
                b.max[1] - b.min[1] > b.max[0] - b.min[0],
                "{id} {path}: {b:?}"
            );
            // In z: from the face out to the height, a bottom part hanging down.
            let (lo, hi) = match body.side {
                Side::Back => (-1.6 - h, -1.6),
                _ => (0.0, h),
            };
            assert!(
                (b.min[2] - lo).abs() < 1e-9 && (b.max[2] - hi).abs() < 1e-9,
                "{id} {path}: {b:?}"
            );
            // The pin 1 dot is nearest pad 1, also mirrored on the bottom.
            if let Some(dot) = mine.iter().find(|m| m.material == Material::Mark) {
                let c = dot.bounds().unwrap().centre();
                let near = placed
                    .footprint
                    .pads
                    .iter()
                    .min_by(|p, q| {
                        let d = |pad: &netsmith_pcb::Pad| {
                            let at = placed.pad_centre(pad);
                            (at.x - left - c[0]).hypot(bottom - at.y - c[1])
                        };
                        d(p).total_cmp(&d(q))
                    })
                    .unwrap();
                assert_eq!(near.number, "1", "{id} {path}");
            }
        }
        // The file's footprint has no package record: still its box.
        let box_ = meshes.iter().filter(|m| m.id == "b").collect::<Vec<_>>();
        assert!(s.body("b").unwrap().shape.is_none());
        assert_eq!(box_.len(), 1);
        assert_eq!(box_[0].material, Material::Body);
        assert_eq!(box_[0].triangle_count(), 12);
        // The same board gives the same meshes.
        assert_eq!(meshes, of_scene(&scene(&shaped(id, Some(h)))));
        // To look at: `NETSMITH_RENDER3D_OUT=<folder>` writes the board from above and below.
        if let Some(out) = std::env::var_os("NETSMITH_RENDER3D_OUT") {
            use netsmith_pcb::raster3d::{RenderOptions, View, render_view};
            std::fs::create_dir_all(&out).unwrap();
            let stem = id.split(':').nth(1).unwrap_or(id);
            for view in [View::Iso, View::IsoBottom] {
                let png = render_view(&meshes, view, &RenderOptions::default()).to_png();
                let file = format!("shaped-{stem}-{}.png", view.name());
                std::fs::write(std::path::Path::new(&out).join(file), png).unwrap();
            }
        }
    }
}

#[test]
fn a_shaped_footprint_of_unknown_height_is_still_its_flat_outline() {
    use netsmith_pcb::mesh::{Material, of_scene};
    let s = scene(&shaped("Package_SO:SOIC-8_3.9x4.9mm_P1.27mm", None));
    let meshes = of_scene(&s);
    for path in ["a", "under"] {
        assert!(s.body(path).unwrap().shape.is_none());
        let mine: Vec<_> = meshes.iter().filter(|m| m.id == path).collect();
        assert_eq!(mine.len(), 1, "{path}");
        assert_eq!(mine[0].material, Material::BodyUnknown);
        assert!(!mine[0].closed);
    }
}
