use super::*;
use crate::scene3d::{
    BoardSolid, Body, Findings, HeightLimit, HeightProvenance, Hole, LimitFace, Plan, PlanBox,
    SceneLayer, Tallest, TooTall,
};
use crate::stackup::LayerKind;

fn plan(x: f64, y: f64) -> Plan {
    Plan { x, y }
}

fn body(path: &str, refdes: &str, min: Plan, max: Plan, height: Option<f64>) -> Body {
    Body {
        path: path.into(),
        refdes: refdes.into(),
        kind: "Thing".into(),
        side: Side::Front,
        layout_at: (0.0, 0.0),
        rotation: 0.0,
        corners: [min, plan(max.x, min.y), max, plan(min.x, max.y)],
        bounds: PlanBox { min, max },
        body_is_courtyard: false,
        z_face: 0.0,
        z_far: height,
        height,
        provenance: if height.is_some() {
            HeightProvenance::Stated
        } else {
            HeightProvenance::Unknown
        },
        pins: Pins::None,
        shape: None,
    }
}

fn layer(name: &str, kind: LayerKind, z_top: f64, z_bottom: f64) -> SceneLayer {
    SceneLayer {
        name: name.into(),
        kind,
        // Rounded to a nanometre, as the scene rounds its lengths.
        thickness: ((z_top - z_bottom) * 1e6).round() / 1e6,
        z_top,
        z_bottom,
    }
}

/// A 40 x 20 mm board with a box `U1` from (10, 5) to (14, 8), 2 mm tall, a part `J1` of unknown
/// height, a via, and a lid that `U1` is too tall for.
fn scene() -> Scene {
    let mut tall = body("u1", "U1", plan(10.0, 5.0), plan(14.0, 8.0), Some(2.0));
    tall.pins = Pins::Known {
        at: PlanBox {
            min: plan(10.5, 5.5),
            max: plan(13.5, 7.5),
        },
        length: 1.5,
        source: "a line".into(),
        z_top: -1.6,
        z_bottom: -3.1,
    };
    Scene {
        board: BoardSolid {
            name: "Demo".into(),
            width: 40.0,
            height: 20.0,
            corner_radius: 0.0,
            thickness: 1.6,
            stack_thickness: 1.6,
            layers: vec![
                layer("F.Mask", LayerKind::Mask, 0.0, -0.01),
                layer("F.Cu", LayerKind::Copper, -0.01, -0.045),
                layer("core", LayerKind::Core, -0.045, -1.555),
                layer("B.Cu", LayerKind::Copper, -1.555, -1.59),
                layer("B.Mask", LayerKind::Mask, -1.59, -1.6),
            ],
            cutouts: Vec::new(),
            holes: vec![Hole {
                owner: HoleOwner::Via { net: "GND".into() },
                from: plan(30.0, 10.0),
                to: plan(30.0, 10.0),
                diameter: 0.4,
                plated: true,
            }],
        },
        // The glTF has no copper yet.
        copper: Ok(crate::scene3d::SceneCopper {
            layers: Vec::new(),
            vias: Vec::new(),
            mask: Vec::new(),
            silk: Vec::new(),
        }),
        bodies: vec![
            body("j1", "J1", plan(20.0, 5.0), plan(24.0, 9.0), None),
            tall,
        ],
        limits: vec![HeightLimit {
            name: "lid".into(),
            carrier: None,
            at: PlanBox {
                min: plan(0.0, 0.0),
                max: plan(20.0, 20.0),
            },
            max_height: 1.0,
            faces: vec![LimitFace {
                side: Side::Front,
                z_bottom: 0.0,
                z_top: 1.0,
                tallest: Some(Tallest {
                    path: "u1".into(),
                    refdes: "U1".into(),
                    height: 2.0,
                }),
                margin: Some(-1.0),
                unknown: Vec::new(),
            }],
        }],
        findings: Findings {
            too_tall: vec![TooTall {
                path: "u1".into(),
                refdes: "U1".into(),
                keepout: "lid".into(),
                height: 2.0,
                limit: 1.0,
                side: Side::Front,
            }],
            ..Findings::default()
        },
    }
}

fn node<'a>(doc: &'a Value, name: &str) -> &'a Value {
    doc["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == name)
        .unwrap_or_else(|| panic!("no node `{name}`"))
}

fn index_of(doc: &Value, name: &str) -> u64 {
    doc["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|n| n["name"] == name)
        .unwrap() as u64
}

fn children(doc: &Value, name: &str) -> Vec<String> {
    node(doc, name)["children"]
        .as_array()
        .map(|c| {
            c.iter()
                .map(|i| {
                    doc["nodes"][i.as_u64().unwrap() as usize]["name"]
                        .as_str()
                        .unwrap()
                        .into()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The positions of the first primitive of a node's mesh, as the file holds them.
fn positions(glb: &[u8], doc: &Value, name: &str) -> Vec<[f32; 3]> {
    let mesh = node(doc, name)["mesh"].as_u64().unwrap() as usize;
    let accessor = &doc["accessors"][doc["meshes"][mesh]["primitives"][0]["attributes"]["POSITION"]
        .as_u64()
        .unwrap() as usize];
    let view = &doc["bufferViews"][accessor["bufferView"].as_u64().unwrap() as usize];
    let bin = bin_chunk(glb).unwrap();
    let at = view["byteOffset"].as_u64().unwrap() as usize;
    let len = view["byteLength"].as_u64().unwrap() as usize;
    let floats: Vec<f32> = bin[at..at + len]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    floats.as_chunks::<3>().0.to_vec()
}

fn material_of(doc: &Value, name: &str) -> String {
    let mesh = node(doc, name)["mesh"].as_u64().unwrap() as usize;
    let m = doc["meshes"][mesh]["primitives"][0]["material"]
        .as_u64()
        .unwrap() as usize;
    doc["materials"][m]["name"].as_str().unwrap().into()
}

#[test]
fn the_nodes_carry_the_ids_of_the_contract() {
    let glb = glb(&scene(), "Demo");
    let doc = json_chunk(&glb).unwrap();
    assert_eq!(doc["asset"]["generator"], "netsmith");
    assert_eq!(doc["asset"]["version"], "2.0");
    assert!(doc.get("extensionsUsed").is_none() && doc.get("extensionsRequired").is_none());
    assert_eq!(doc["scenes"][0]["nodes"], json!([0]));
    let root = &node(&doc, "board")["extras"]["netsmith"];
    assert_eq!(root["kind"], "board");
    assert_eq!(root["schema"], 1);
    assert_eq!(root["board"], "Demo");
    assert_eq!(root["thickness_mm"], 1.6);
    assert_eq!(root["width_mm"], 40.0);
    // The stackup, top face down, as the scene has it.
    let layers = root["layers"].as_array().unwrap();
    let want = &scene().board.layers;
    assert_eq!(layers.len(), want.len());
    for (got, l) in layers.iter().zip(want) {
        assert_eq!(
            *got,
            json!({
                "name": l.name, "kind": l.kind.word(), "thickness_mm": l.thickness,
                "z_top_mm": l.z_top, "z_bottom_mm": l.z_bottom,
            })
        );
    }
    assert_eq!(layers[0]["kind"], "mask");
    assert_eq!(root["coverage"], json!({ "known": 1, "total": 2 }));
    assert_eq!(root["findings"][0]["code"], "too_tall");
    assert_eq!(root["findings"][0]["parts"], json!(["u1"]));
    assert_eq!(root["findings"][0]["limit"], "lid");
    assert_eq!(
        children(&doc, "board"),
        ["substrate", "J1", "U1", "limits", "protrusions"]
    );
    assert_eq!(
        children(&doc, "substrate"),
        ["mask:top", "mask:bottom", "via:GND"]
    );
    assert_eq!(
        node(&doc, "via:GND")["extras"]["netsmith"],
        json!({ "kind": "hole", "net": "GND" })
    );
    let u1 = &node(&doc, "U1")["extras"]["netsmith"];
    assert_eq!(
        *u1,
        json!({
            "kind": "part", "path": "u1", "refdes": "U1", "side": "top", "provenance": "stated",
            "height_mm": 2.0, "z_bottom_mm": 0.0, "z_top_mm": 2.0,
        })
    );
    assert_eq!(material_of(&doc, "U1"), "body");
    let lid = &node(&doc, "limit:lid:top")["extras"]["netsmith"];
    assert_eq!(lid["kind"], "height_limit");
    assert_eq!(lid["tallest"], "u1");
    assert_eq!(lid["margin_mm"], -1.0);
    assert_eq!(lid["violated"], true);
    assert_eq!(material_of(&doc, "limit:lid:top"), "limit-violated");
    assert_eq!(
        node(&doc, "pins:U1")["extras"]["netsmith"],
        json!({ "kind": "protrusion", "path": "u1", "length_mm": 1.5 })
    );
    let names: Vec<&str> = doc["materials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, Material::ALL.map(Material::name));
    assert_eq!(doc["materials"][8]["alphaMode"], "BLEND");
    assert_eq!(doc["materials"][9]["alphaMode"], "BLEND");
    // The children are numbered after their parent.
    assert!(index_of(&doc, "U1") > index_of(&doc, "board"));
}

#[test]
fn a_part_of_unknown_height_is_flat_and_says_so() {
    let glb = glb(&scene(), "Demo");
    let doc = json_chunk(&glb).unwrap();
    let j1 = &node(&doc, "J1")["extras"]["netsmith"];
    assert_eq!(j1["provenance"], "unknown");
    assert!(j1["height_mm"].is_null() && j1["z_top_mm"].is_null());
    assert_eq!(j1["z_bottom_mm"], 0.0);
    assert_eq!(material_of(&doc, "J1"), "body-unknown");
    let ys: Vec<f32> = positions(&glb, &doc, "J1").iter().map(|p| p[1]).collect();
    let lift = (crate::mesh::UNKNOWN_LIFT * 0.001) as f32;
    assert!(ys.iter().all(|&y| y == lift), "flat at the lift: {ys:?}");
}

#[test]
fn a_box_lands_where_gltf_puts_it() {
    let glb = glb(&scene(), "Demo");
    let doc = json_chunk(&glb).unwrap();
    let ps = positions(&glb, &doc, "U1");
    let (mut min, mut max) = (ps[0], ps[0]);
    for p in &ps {
        for k in 0..3 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }
    // Scene x 10..14 mm, y 5..8 mm, z 0..2 mm: glTF x = x, y = z, z = -y, in metres.
    assert_eq!(min, [0.010, 0.0, -0.008]);
    assert_eq!(max, [0.014, 0.002, -0.005]);
    // The accessor states the same bounds (read back as f32: the reader's parser may land a
    // digit off in f64).
    let mesh = node(&doc, "U1")["mesh"].as_u64().unwrap() as usize;
    let a = &doc["accessors"][doc["meshes"][mesh]["primitives"][0]["attributes"]["POSITION"]
        .as_u64()
        .unwrap() as usize];
    let read = |key: &str| -> Vec<f32> {
        a[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect()
    };
    assert_eq!(read("min"), min);
    assert_eq!(read("max"), max);
}

#[test]
fn the_same_scene_gives_the_same_bytes_and_a_valid_container() {
    let a = glb(&scene(), "Demo");
    assert_eq!(a, glb(&scene(), "Demo"));
    assert_eq!(&a[0..4], b"glTF");
    assert_eq!(
        u32::from_le_bytes(a[8..12].try_into().unwrap()) as usize,
        a.len()
    );
    assert_eq!(a.len() % 4, 0);
    let doc = json_chunk(&a).unwrap();
    assert_eq!(
        doc["buffers"][0]["byteLength"].as_u64().unwrap() as usize,
        bin_chunk(&a).unwrap().len()
    );
    // Every accessor reads inside the binary chunk, with the size its type says.
    let bin = bin_chunk(&a).unwrap();
    for accessor in doc["accessors"].as_array().unwrap() {
        let view = &doc["bufferViews"][accessor["bufferView"].as_u64().unwrap() as usize];
        let (at, len) = (
            view["byteOffset"].as_u64().unwrap() as usize,
            view["byteLength"].as_u64().unwrap() as usize,
        );
        assert!(at % 4 == 0 && at + len <= bin.len());
        let each = if accessor["type"] == "VEC3" { 12 } else { 4 };
        assert_eq!(accessor["count"].as_u64().unwrap() as usize * each, len);
    }
}

#[test]
fn the_colours_are_the_light_palette_in_linear_light() {
    assert_eq!(linear(0), 0.0);
    assert_eq!(linear(255), 1.0);
    let doc = json_chunk(&glb(&scene(), "Demo")).unwrap();
    let [r, ..] = Theme::Light.palette().colour(Material::Copper);
    assert_eq!(
        doc["materials"][3]["pbrMetallicRoughness"]["baseColorFactor"][0],
        linear(r)
    );
}
