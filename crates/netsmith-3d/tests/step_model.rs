//! A part drawn from its STEP model (ADR 0108, `Scene::draw_models`): where the model lands,
//! checked against the way KiCad places a footprint's model, its colours, and the warning for a
//! model taller than its part.
//!
//! `models/blocks.step` was written by hand for these tests (netsmith's own, MIT): a dark block
//! 3 x 1 x 0.5 mm from the model's origin along +x and +y, and a red block 0.5 x 0.5 x 0.1 mm on
//! its top near the origin, so that a turn, a mirror and the sign of y all show.

use std::sync::Arc;

use netsmith_3d::scene3d::{ModelUse, Scene};
use netsmith_3d::step::read::{ReadModel, load};
use netsmith_pcb::{Board, Pt, Side};

use crate::scene3d::{board, scene};

const BLOCKS: &[u8] = include_bytes!("models/blocks.step");

fn blocks() -> Arc<ReadModel> {
    Arc::new(load(BLOCKS).unwrap_or_else(|e| panic!("{e}")))
}

fn model_use(path: &str, offset: [f64; 3], rotation: [f64; 3]) -> ModelUse {
    ModelUse {
        path: path.into(),
        file: "models/blocks.step".into(),
        sha256: "0".repeat(64),
        offset,
        rotation,
        read: blocks(),
    }
}

type M3 = [[f64; 3]; 3];

fn mul(a: M3, b: M3) -> M3 {
    [0, 1, 2].map(|r| [0, 1, 2].map(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}

fn apply(m: M3, p: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|r| (0..3).map(|k| m[r][k] * p[k]).sum())
}

/// A turn by `deg` degrees about an axis, counterclockwise seen from its positive end.
fn rot(axis: usize, deg: f64) -> M3 {
    let (s, c) = deg.to_radians().sin_cos();
    match axis {
        0 => [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]],
        1 => [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]],
        _ => [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
    }
}

/// Where KiCad puts the point `p` of a footprint's model, written out from KiCad's STEP
/// exporter (`STEP_PCB_MODEL::getModelLocation`, `pcbnew/exporters/step/step_pcb_model.cpp`,
/// KiCad 7 to 9; its 3D viewer, `render_3d_opengl.cpp`, applies the same order):
///
/// "a. aOrientation is applied -Z*-Y*-X; b. aOffset is applied, Top ? add thickness to the Z
/// offset; c. Bottom ? Rotate on X axis (in contrast to most ECAD which mirror on X), then rotate
/// on +Z, Top ? rotate on -Z; d. aPosition is applied. Note: Y axis is inverted in KiCad": the
/// position is `(x, -y)` and the offset is used as it is, so the model's y runs up the
/// footprint, against the board file's y.
///
/// The footprint's `(at x y angle)` is in the board file's frame (mm, y down); the result is in
/// KiCad's 3D frame (y up), with the board's top face at z = 0 and its bottom at
/// `-thickness`.
fn kicad(
    at: Pt,
    angle: f64,
    bottom: bool,
    thickness: f64,
    offset: [f64; 3],
    rotate: [f64; 3],
    p: [f64; 3],
) -> [f64; 3] {
    let orient = mul(
        rot(2, -rotate[2]),
        mul(rot(1, -rotate[1]), rot(0, -rotate[0])),
    );
    let q = apply(orient, p);
    let mut q = [q[0] + offset[0], q[1] + offset[1], q[2] + offset[2]];
    if bottom {
        // The offset grows by the board's bottom (here -thickness taken away), then the turn
        // about x puts the model under it.
        q[2] += thickness;
        q = apply(rot(0, 180.0), q);
    }
    let q = apply(rot(2, angle), q);
    [q[0] + at.x, q[1] - at.y, q[2]]
}

/// A point of KiCad's 3D frame in the scene's: the same x and z, and y from the outline's
/// bottom edge (KiCad's y is the board file's negated).
fn kicad_to_scene(board: &Board, k: [f64; 3]) -> [f64; 3] {
    [k[0] - board.outline.min.x, k[1] + board.outline.max.y, k[2]]
}

/// A footprint pad's place as KiCad has it for a footprint on the bottom face: the library's
/// pads mirrored in y (KiCad's `FOOTPRINT::Flip` flips its pads top to bottom about its
/// position, and a flip left to right adds a turn of 180 degrees), turned by its angle.
fn kicad_pad(at: Pt, angle: f64, pad: Pt) -> Pt {
    at + Pt::new(pad.x, -pad.y).rotate(angle)
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < 1e-6)
}

/// The model of `path` drawn with `offset` and `rotation` on a board where `path` is turned by
/// `angle` degrees.
fn drawn(path: &str, angle: f64, offset: [f64; 3], rotation: [f64; 3]) -> (Board, Scene) {
    let mut b = board();
    b.parts
        .iter_mut()
        .find(|p| p.path == path)
        .unwrap()
        .rotation = angle;
    let mut s = scene(&b);
    s.draw_models(&[model_use(path, offset, rotation)]);
    (b, s)
}

#[test]
fn a_model_lands_where_kicad_puts_it_on_the_top_face() {
    let (offset, rotation, angle) = ([1.0, 2.0, 0.3], [10.0, 20.0, 90.0], 30.0);
    let (b, s) = drawn("a", angle, offset, rotation);
    let part = b.parts.iter().find(|p| p.path == "a").unwrap();
    assert_eq!(part.side, Side::Front);
    let model = s.body("a").unwrap().model.as_ref().unwrap();
    let read = blocks();
    for (piece, mesh) in read.mesh.pieces.iter().zip(&model.meshes) {
        for (p, q) in piece.mesh.positions.iter().zip(&mesh.positions) {
            let want = kicad_to_scene(&b, kicad(part.at, angle, false, 1.6, offset, rotation, *p));
            assert!(
                close(*q, want),
                "{p:?} is drawn at {q:?}, KiCad has {want:?}"
            );
        }
    }
}

#[test]
fn a_model_lands_where_kicad_puts_it_on_the_bottom_face_by_the_same_pads() {
    // KiCad turns a footprint on the bottom face by 180 degrees more than netsmith does for the
    // same pads: KiCad mirrors the library's pads in y, netsmith in x (`Placed::to_board`).
    let (offset, rotation, angle) = ([1.0, 2.0, 0.3], [10.0, 20.0, 90.0], 30.0);
    let (b, s) = drawn("under", angle, offset, rotation);
    let part = b.parts.iter().find(|p| p.path == "under").unwrap();
    assert_eq!(part.side, Side::Back);
    let kicad_angle = angle + 180.0;
    for pad in &part.footprint.pads {
        let (ours, theirs) = (
            part.to_board(pad.at),
            kicad_pad(part.at, kicad_angle, pad.at),
        );
        assert!((ours.x - theirs.x).abs() < 1e-9 && (ours.y - theirs.y).abs() < 1e-9);
    }
    let model = s.body("under").unwrap().model.as_ref().unwrap();
    let read = blocks();
    let mut reach = f64::MIN;
    for (piece, mesh) in read.mesh.pieces.iter().zip(&model.meshes) {
        for (p, q) in piece.mesh.positions.iter().zip(&mesh.positions) {
            let want = kicad_to_scene(
                &b,
                kicad(part.at, kicad_angle, true, 1.6, offset, rotation, *p),
            );
            assert!(
                close(*q, want),
                "{p:?} is drawn at {q:?}, KiCad has {want:?}"
            );
            reach = reach.max(-1.6 - want[2]);
        }
    }
    // Its top is how far it hangs down from the bottom face.
    assert!((model.top - reach).abs() < 1e-6, "{} {reach}", model.top);
    assert!(
        model
            .meshes
            .iter()
            .flat_map(|m| &m.positions)
            .all(|p| p[2] < -1.6)
    );
}

#[test]
fn the_y_of_a_model_runs_up_the_footprint() {
    // Unturned, no rotation: the model's +y (its 1 mm side) is toward the outline's top edge,
    // which is the scene's +y and the board file's -y.
    let (b, s) = drawn("a", 0.0, [0.0, 0.5, 0.0], [0.0; 3]);
    let part = b.parts.iter().find(|p| p.path == "a").unwrap();
    let origin = [
        part.at.x - b.outline.min.x,
        b.outline.max.y - part.at.y,
        0.0,
    ];
    let model = s.body("a").unwrap().model.as_ref().unwrap();
    let bounds = netsmith_3d::mesh::bounds_of(&model.meshes).unwrap();
    assert!(close(bounds.min, [origin[0], origin[1] + 0.5, 0.0]));
    assert!(close(bounds.max, [origin[0] + 3.0, origin[1] + 1.5, 0.6]));
}

#[test]
fn a_model_keeps_its_colours_and_takes_its_parts_id() {
    let (_, s) = drawn("a", 0.0, [0.0; 3], [0.0; 3]);
    let model = s.body("a").unwrap().model.as_ref().unwrap();
    let colours: Vec<Option<[u8; 3]>> = model.meshes.iter().map(|m| m.colour).collect();
    assert_eq!(colours, [Some([51, 51, 56]), Some([217, 26, 26])]);
    assert!(model.meshes.iter().all(|m| m.id == "a" && m.closed));
    assert!(model.exact);
    assert_eq!(model.top, 0.6);
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
}

#[test]
fn a_model_above_its_parts_height_is_a_warning_and_changes_no_height() {
    // `a` is 2 mm tall; sunk 1.7 mm up, the model's top is 2.3 mm.
    let (_, s) = drawn("a", 0.0, [0.0, 0.0, 1.7], [0.0; 3]);
    let a = s.body("a").unwrap();
    assert_eq!(a.height, Some(2.0));
    assert_eq!(a.z_far, Some(2.0));
    assert_eq!(s.warnings.len(), 1, "{:?}", s.warnings);
    let w = &s.warnings[0];
    assert!(w.contains("2.300 mm out of its face"), "{w}");
    assert!(w.contains("the part's height of 2.000 mm (stated)"), "{w}");
    assert!(w.contains("`.a.height &= 2.30mm`"), "{w}");
    // Within the chord tolerance it is not.
    let (_, s) = drawn("a", 0.0, [0.0, 0.0, 1.405], [0.0; 3]);
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
}

#[test]
fn the_same_model_is_drawn_to_the_same_bits() {
    let (_, a) = drawn("under", 30.0, [1.0, 2.0, 0.3], [10.0, 20.0, 90.0]);
    let (_, b) = drawn("under", 30.0, [1.0, 2.0, 0.3], [10.0, 20.0, 90.0]);
    assert_eq!(a, b);
}

#[test]
fn the_meshes_of_a_part_drawn_from_its_model_are_the_models() {
    let (_, s) = drawn("a", 30.0, [1.0, 2.0, 0.3], [0.0, 0.0, 45.0]);
    let pieces = netsmith_3d::mesh::pieces(&s);
    let i = s.bodies.iter().position(|b| b.path == "a").unwrap();
    let model = s.body("a").unwrap().model.as_ref().unwrap();
    assert_eq!(pieces.bodies[i].body, model.meshes);
}

#[test]
fn the_glb_draws_a_model_in_its_own_colours() {
    let (_, s) = drawn("a", 0.0, [0.0; 3], [0.0; 3]);
    let glb = netsmith_3d::gltf::glb(&s, "Scene");
    assert_eq!(glb, netsmith_3d::gltf::glb(&s, "Scene"));
    let doc = netsmith_3d::gltf::json_chunk(&glb).unwrap();
    let materials: Vec<&str> = doc["materials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    // After the fixed materials (`case` the last of them, ADR 0109), in the order first used.
    let fixed = netsmith_3d::mesh::Material::ALL.len();
    assert_eq!(materials[fixed - 1], "case");
    assert_eq!(materials[fixed..], ["model:#333338", "model:#d91a1a"]);
    let node = doc["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "U1")
        .unwrap();
    let mesh = &doc["meshes"][node["mesh"].as_u64().unwrap() as usize];
    let used: Vec<u64> = mesh["primitives"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["material"].as_u64().unwrap())
        .collect();
    assert_eq!(used, [fixed as u64, fixed as u64 + 1]);
}

#[test]
fn the_step_export_copies_the_models_solids_exactly_where_the_scene_draws_them() {
    let (_, s) = drawn("under", 30.0, [1.0, 2.0, 0.3], [10.0, 20.0, 90.0]);
    let text = netsmith_3d::step::step(&s, "Scene");
    assert_eq!(text, netsmith_3d::step::step(&s, "Scene"));
    assert!(
        text.contains("drawn from models/blocks.step (sha256 0000"),
        "the part's description names the file and its SHA-256"
    );
    assert!(text.contains("PRODUCT('under:body','body','a solid of models/blocks.step'"));
    // Read back, the copied solids are where the scene draws the model, to the reader's
    // tolerance (the copy is exact; the placement is written to ten places).
    let back = netsmith_3d::step::read::read(text.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    let model = s.body("under").unwrap().model.as_ref().unwrap();
    for (name, drawn) in ["body", "mark"].iter().zip(&model.meshes) {
        let b = drawn.bounds().unwrap();
        let found = back.pieces.iter().filter(|p| p.mesh.id == *name).any(|p| {
            let a = p.mesh.bounds().unwrap();
            close(a.min, b.min) && close(a.max, b.max)
        });
        assert!(found, "no solid `{name}` read back at {b:?}");
    }
    // The model's colours come with it.
    assert!(text.contains("COLOUR_RGB('',0.85,0.1,0.1)"));
}

#[test]
fn a_model_in_inches_is_copied_with_its_own_unit() {
    let text = String::from_utf8(BLOCKS.to_vec()).unwrap().replace(
        "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))",
        "(CONVERSION_BASED_UNIT('INCH',#9001)LENGTH_UNIT()NAMED_UNIT(*))",
    );
    let text = text.replace(
        "ENDSEC;\nEND-ISO",
        "#9000=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n#9001=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#9000);\nENDSEC;\nEND-ISO",
    );
    let read = Arc::new(load(text.as_bytes()).unwrap_or_else(|e| panic!("{e}")));
    let mut s = scene(&board());
    s.draw_models(&[ModelUse {
        read,
        ..model_use("a", [0.0; 3], [0.0; 3])
    }]);
    let model = s.body("a").unwrap().model.as_ref().unwrap();
    assert!((model.top - 0.6 * 25.4).abs() < 1e-6, "{}", model.top);
    let out = netsmith_3d::step::step(&s, "Scene");
    assert!(out.contains("LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),"));
    let back = netsmith_3d::step::read::read(out.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    let body = back.pieces.iter().find(|p| p.mesh.id == "body").unwrap();
    let b = body.mesh.bounds().unwrap();
    assert!((b.max[2] - 0.5 * 25.4).abs() < 1e-6, "{b:?}");
}
