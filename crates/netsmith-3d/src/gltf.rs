//! The board's _Scene_ as glTF 2.0 binary (`.glb`, ADR 0105): the one file the editor's 3D view
//! displays, `netsmith board3d --glb` writes and an agent or another tool reads. [`glb`] writes
//! it from a [`Scene`] and its meshes ([`crate::mesh::pieces`], the same triangles the
//! rasterizer draws). `docs/BOARD3D.md` describes the file for a reader of it.
//!
//! # The contract (schema 1)
//!
//! A viewer is written against this, so a change to it that removes a field or changes what one
//! means raises [`SCHEMA`]; a field added does not.
//!
//! - **Units and axes are glTF's**: metres, right-handed, Y up. A point `(x, y, z)` of the scene
//!   (mm, Z up, the top face at `z = 0`, [`crate::scene3d`]) is written as
//!   `(x, z, -y) * 0.001`, in the vertex data itself: no node has a transform (but a member
//!   of a system's file, [`system_glb`], whose `matrix` is its motion). The board's
//!   top face is `y = 0`, its outline's bottom-left corner the origin, its top edge toward `-z`.
//!   The numbers in the `extras` are in the scene's frame and in mm, as `board3d --json` has
//!   them (their names end in `_mm`).
//! - **Deterministic**: the same scene gives the same bytes. Nodes, meshes, materials,
//!   accessors and buffer views are in a fixed order; a coordinate is the `f64` of the scene
//!   converted with `as f32`; every accessor of positions has its `min` and `max`;
//!   `asset.generator` is `netsmith`, with no version and no time.
//! - **Nodes**, each with `extras.netsmith.kind`. The scene's one root is `board`; its children,
//!   in order:
//!   - `board` (the root): `{kind: "board", schema, board, thickness_mm, stack_thickness_mm,
//!     width_mm, height_mm, layers: [{name, kind, thickness_mm, z_top_mm, z_bottom_mm}],
//!     coverage: {known, total, standard}, findings: [{code, message, parts: [path...], limit?}]}`.
//!     `board` is the board's label (`system/member` for a board of a system). `layers` is the
//!     stackup from the top face down, as `board3d --json` has it in `board.layers` (`kind` is
//!     `mask`, `copper`, `core` or `prepreg`), for a viewer that draws the stackup (a section,
//!     the layers pulled apart); added in schema 1. A finding's `code` is `too_tall` (a part
//!     over a height limit; `limit` names it), `height_unchecked` (a part of unknown height
//!     under one) or `body_overlap` (two bodies that fill the same space); `face` says on which
//!     face. A case model's fit (ADR 0109) adds `case_collision`, `case_clearance`,
//!     `case_reach`, `case_boss` and `case_unchecked`, each with `severity` (`error` or
//!     `warning`), `case` (the model), `solid` (the node `case:<name>:<solid>` it is about, or
//!     `null`) and `at_mm` (where to look, in the scene's frame); added without a new schema.
//!   - `substrate`: `{kind: "substrate"}`, the laminate as its mesh, with children `mask:top`
//!     and `mask:bottom` (`{kind: "mask", face}`, where the stackup has them: the mask with its
//!     openings cut out, material `mask-top` or `mask-bottom` over laminate and
//!     `mask-over-copper` over copper), one node per plated hole, `hole:<refdes>.<pad>` or
//!     `via:<net>` (`{kind: "hole", path, refdes, pad}` or `{kind: "hole", net}`), its barrel as
//!     the mesh, and last `silk:top` and `silk:bottom` (`{kind: "silk", face}`, material
//!     `mark`), the silkscreen off the mask's openings, when the copper can be drawn.
//!   - one node per part with a body, by path, named by its designator: `{kind: "part", path,
//!     refdes, side: "top" | "bottom", provenance: "stated" | "catalog" | "standard" |
//!     "unknown", standard, height_mm, z_bottom_mm, z_top_mm, part_kind, category, margin_mm,
//!     margin_limit}`. A `standard` height is the package standard's maximum (ADR 0106), and
//!     `standard` names the standard and the drawing (`JEDEC MS-012, drawing TI D0008A`; `null`
//!     for the other provenances). Both were added without a new schema: a field and a word were
//!     added and none changed meaning. The root's `coverage.standard` counts those heights, which
//!     `known` includes. A known height is a solid (material `body`); an unknown one is the
//!     body's flat outline (material `body-unknown`) and `height_mm` is `null`, as is the far end of the body
//!     (`z_top_mm` on the top face, `z_bottom_mm` on the bottom): never a guessed height (ADR
//!     0094). `part_kind` is the part's kind as the design names it (`Resistor`, `Header1x2`);
//!     `category` is what sort of part it is ([`crate::scene3d::PartCategory`]: `resistor`,
//!     `capacitor`, `inductor`, `diode`, `transistor`, `crystal`, `fuse`, `ic`, `connector`,
//!     `mechanical` or `other`). `margin_mm` is the least margin of the part to a height limit
//!     over it on its face (the limit less the part's height, negative when too tall) and
//!     `margin_limit` names that limit; both are `null` when no limit is over the part or its
//!     height is unknown. (These four were added in schema 1.) `model` is `{file, sha256,
//!     top_mm}` for a part drawn from its STEP model (ADR 0108), else `null`: added without a
//!     new schema.
//!   - `limits`: one child per face of each height limit, `limit:<name>:<face>`:
//!     `{kind: "height_limit", name, face, max_height_mm, tallest: path | null, margin_mm:
//!     number | null, violated, unknown: [path...], under: [path...]}`, its volume as the
//!     mesh; `under` is every part under it on that face, known height or not (added in
//!     schema 1).
//!   - `protrusions`: one child per part whose pins stand out of the far face by a stated
//!     length, `pins:<refdes>`: `{kind: "protrusion", path, length_mm}`.
//!   - One group per copper layer, top face down, `copper:<layer>`: `{kind: "copper_layer",
//!     layer, z_top_mm, z_bottom_mm}`, with a child per net on it, `copper:<layer>:<net>`
//!     (`copper:<layer>:(no net)` first, for the copper with no net): `{kind: "copper", layer,
//!     net: name | null}`, the net's copper as the mesh (material `copper` under the mask,
//!     `finish` where it is open). None when the copper cannot be drawn.
//!
//!   - One group per case model (ADR 0109), `case:<name>`: `{kind: "case", name, file,
//!     sha256, triangles}`, with a child per solid of the file, `case:<name>:<solid>`
//!     (`{kind: "case_solid", case, solid, closed, triangles}`), its triangles as the mesh,
//!     material `case` (see-through). Added without a new schema.
//!
//!   A node whose shape has no area has no mesh, and is still there.
//! - **Materials**, by name, PBR metallic-roughness, in the order of [`Material::ALL`]:
//!   `substrate`, `mask-top`, `mask-bottom`, `copper`, `body`, `body-unknown`, `metal`, `mark`,
//!   `limit`, `limit-violated` (these two `alphaMode: BLEND`), `finish` and `mask-over-copper`
//!   (added at the end in schema 1, so no index moved), then `prepreg` and `case` (a case
//!   model's solids, ADR 0109, `BLEND`), added at the end the same way. Their colours are the
//!   rasterizer's light palette ([`Theme::Light`]), turned into linear light as glTF wants
//!   them. The materials of open surfaces (a barrel, a flat outline, a limit) are
//!   double-sided.
//!   A part drawn from its STEP model (ADR 0108) has the model's triangles as its mesh: a
//!   piece the file colours has a material of its own after these (after `case`), `model:#rrggbb`,
//!   in the order the colours are first used; a piece it does not colour is `body`.
//! - No extension is used or required.

use serde_json::{Map, Value, json};

use crate::mesh::{BodyPieces, Material, Mesh, NO_NET, pieces};
use crate::raster3d::Theme;
use crate::scene3d::{HoleOwner, Pins, Scene};
use crate::system::{Finding, Motion, Place, SystemScene};
use netsmith_pcb::Side;

/// The version of the file's contract (the module's header).
pub(crate) const SCHEMA: u32 = 1;

/// The scene's frame (mm, Z up) to glTF's (m, Y up).
fn position(p: [f64; 3]) -> [f32; 3] {
    [p[0] * 0.001, p[2] * 0.001, -p[1] * 0.001].map(f32_of)
}

/// A direction turned as [`position`] turns a point.
fn direction(n: [f64; 3]) -> [f32; 3] {
    [n[0], n[2], -n[1]].map(f32_of)
}

/// `v as f32`, with no negative zero.
fn f32_of(v: f64) -> f32 {
    let f = v as f32;
    if f == 0.0 { 0.0 } else { f }
}

const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;
const TRIANGLES: u32 = 4;

/// The JSON and binary chunks as they are filled.
#[derive(Default)]
struct Builder {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
    meshes: Vec<Value>,
    nodes: Vec<Value>,
    /// The colours of the STEP models' pieces (ADR 0108), in the order they are first used:
    /// each a material after [`Material::ALL`]'s.
    colours: Vec<[u8; 3]>,
}

/// A node before it is numbered.
struct Node {
    name: String,
    mesh: Option<usize>,
    /// The node's transform, column by column, as glTF writes it; `None` for none.
    matrix: Option<[f64; 16]>,
    extras: Value,
    children: Vec<Node>,
}

impl Node {
    fn new(name: impl Into<String>, extras: Value) -> Node {
        Node {
            name: name.into(),
            mesh: None,
            matrix: None,
            extras,
            children: Vec::new(),
        }
    }
}

impl Builder {
    /// A buffer view of `bytes`, aligned to four bytes.
    fn view(&mut self, bytes: &[u8], target: u32) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let offset = self.bin.len();
        self.bin.extend_from_slice(bytes);
        self.views.push(json!({
            "buffer": 0,
            "byteOffset": offset,
            "byteLength": bytes.len(),
            "target": target,
        }));
        self.views.len() - 1
    }

    fn vec3(&mut self, values: &[[f32; 3]], bounds: bool) -> usize {
        let bytes: Vec<u8> = values
            .iter()
            .flat_map(|v| v.iter().flat_map(|c| c.to_le_bytes()))
            .collect();
        let view = self.view(&bytes, ARRAY_BUFFER);
        let mut accessor = json!({
            "bufferView": view,
            "componentType": FLOAT,
            "count": values.len(),
            "type": "VEC3",
        });
        if bounds {
            let mut min = values[0];
            let mut max = values[0];
            for v in values {
                for k in 0..3 {
                    min[k] = min[k].min(v[k]);
                    max[k] = max[k].max(v[k]);
                }
            }
            accessor["min"] = json!(min.map(f64::from));
            accessor["max"] = json!(max.map(f64::from));
        }
        self.accessors.push(accessor);
        self.accessors.len() - 1
    }

    fn indices(&mut self, indices: &[u32]) -> usize {
        let bytes: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        let view = self.view(&bytes, ELEMENT_ARRAY_BUFFER);
        self.accessors.push(json!({
            "bufferView": view,
            "componentType": UNSIGNED_INT,
            "count": indices.len(),
            "type": "SCALAR",
        }));
        self.accessors.len() - 1
    }

    /// A glTF mesh of `pieces`, one primitive each; `None` when none has a triangle.
    fn mesh(&mut self, name: &str, pieces: &[&Mesh]) -> Option<usize> {
        let mut primitives = Vec::new();
        for m in pieces.iter().filter(|m| !m.indices.is_empty()) {
            let positions: Vec<[f32; 3]> = m.positions.iter().map(|&p| position(p)).collect();
            let mut attributes = Map::new();
            attributes.insert("POSITION".into(), json!(self.vec3(&positions, true)));
            if let Some(normals) = &m.normals
                && normals.len() == m.positions.len()
            {
                let normals: Vec<[f32; 3]> = normals.iter().map(|&n| direction(n)).collect();
                attributes.insert("NORMAL".into(), json!(self.vec3(&normals, false)));
            }
            let indices = self.indices(&m.indices);
            let material = match m.colour {
                Some(c) => {
                    let k = match self.colours.iter().position(|&x| x == c) {
                        Some(k) => k,
                        None => {
                            self.colours.push(c);
                            self.colours.len() - 1
                        }
                    };
                    Material::ALL.len() + k
                }
                None => material_index(m.material),
            };
            primitives.push(json!({
                "attributes": attributes,
                "indices": indices,
                "material": material,
                "mode": TRIANGLES,
            }));
        }
        if primitives.is_empty() {
            return None;
        }
        self.meshes
            .push(json!({ "name": name, "primitives": primitives }));
        Some(self.meshes.len() - 1)
    }

    /// Numbers `node` and its children depth first, the parent before them; returns its index.
    fn node(&mut self, node: Node) -> usize {
        let at = self.nodes.len();
        self.nodes.push(Value::Null);
        let children: Vec<usize> = node.children.into_iter().map(|c| self.node(c)).collect();
        let mut out = json!({
            "name": node.name,
            "extras": { "netsmith": node.extras },
        });
        if let Some(mesh) = node.mesh {
            out["mesh"] = json!(mesh);
        }
        if let Some(matrix) = node.matrix {
            out["matrix"] = json!(matrix);
        }
        if !children.is_empty() {
            out["children"] = json!(children);
        }
        self.nodes[at] = out;
        at
    }
}

fn material_index(m: Material) -> usize {
    Material::ALL.iter().position(|&x| x == m).unwrap_or(0)
}

/// An sRGB byte as linear light, to four places (so that the platform's `powf` cannot change a
/// byte of the file).
fn linear(c: u8) -> f64 {
    let s = f64::from(c) / 255.0;
    let l = if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    };
    (l * 10_000.0).round() / 10_000.0
}

/// The thirteen materials of [`Material::ALL`], then one per colour of a STEP model, named
/// `model:#rrggbb`, opaque, as the `body` material is but for its colour.
fn materials(colours: &[[u8; 3]]) -> Vec<Value> {
    let palette = Theme::Light.palette();
    let model = colours.iter().map(|&[r, g, b]| {
        json!({
            "name": format!("model:#{r:02x}{g:02x}{b:02x}"),
            "pbrMetallicRoughness": {
                "baseColorFactor": [linear(r), linear(g), linear(b), 1.0],
                "metallicFactor": 0.0,
                "roughnessFactor": 0.8,
            },
        })
    });
    Material::ALL
        .iter()
        .map(|&m| {
            let [r, g, b, a] = palette.colour(m);
            let (metallic, roughness) = match m {
                Material::Copper => (1.0, 0.35),
                Material::Finish => (1.0, 0.3),
                Material::Metal => (1.0, 0.4),
                Material::MaskTop | Material::MaskBottom | Material::MaskOverCopper => (0.0, 0.45),
                _ => (0.0, 0.8),
            };
            let alpha = (f64::from(a) / 255.0 * 10_000.0).round() / 10_000.0;
            let mut out = json!({
                "name": m.name(),
                "pbrMetallicRoughness": {
                    "baseColorFactor": [linear(r), linear(g), linear(b), alpha],
                    "metallicFactor": metallic,
                    "roughnessFactor": roughness,
                },
            });
            if m.is_translucent() {
                out["alphaMode"] = json!("BLEND");
            }
            if matches!(
                m,
                Material::Copper
                    | Material::BodyUnknown
                    | Material::Mark
                    | Material::Limit
                    | Material::LimitViolated
                    | Material::Case
            ) {
                out["doubleSided"] = json!(true);
            }
            out
        })
        .chain(model)
        .collect()
}

fn face(side: Side) -> &'static str {
    side.word()
}

/// The 3D findings of the scene, as the root's `extras` lists them.
fn findings(scene: &Scene) -> Vec<Value> {
    let f = &scene.findings;
    let mut out = Vec::new();
    for t in &f.too_tall {
        out.push(json!({
            "code": "too_tall",
            "message": format!(
                "{} (`{}`) stands {:.2} mm tall on the {} face, and the limit `{}` allows {:.2} mm there",
                t.refdes, t.path, t.height, face(t.side), t.keepout, t.limit
            ),
            "parts": [t.path],
            "limit": t.keepout,
            "face": face(t.side),
        }));
    }
    for u in &f.unchecked {
        out.push(json!({
            "code": "height_unchecked",
            "message": format!(
                "{} (`{}`) has no known height, so the limit `{}` ({:.2} mm on the {} face) is not checked over it",
                u.refdes, u.path, u.keepout, u.limit, face(u.side)
            ),
            "parts": [u.path],
            "limit": u.keepout,
            "face": face(u.side),
        }));
    }
    for o in &f.overlaps {
        out.push(json!({
            "code": "body_overlap",
            "message": format!(
                "{} and {} fill the same space on the {} face, z {:.2} to {:.2} mm",
                o.a_refdes, o.b_refdes, face(o.side), o.z_bottom, o.z_top
            ),
            "parts": [o.a, o.b],
            "face": face(o.side),
        }));
    }
    // The fit in the case models (ADR 0109).
    for c in &scene.case.findings {
        let solid = (!c.solid.is_empty()).then(|| format!("case:{}:{}", c.case, c.solid));
        out.push(json!({
            "code": c.kind.word(),
            "message": c.to_string(),
            "parts": c.path().into_iter().collect::<Vec<_>>(),
            "severity": c.severity(false),
            "case": c.case,
            "solid": solid,
            "at_mm": c.at,
        }));
    }
    out
}

/// The least margin of a part of known height to the height limits over it on its face, and the
/// limit's name: `None` when no limit is over it or its height is unknown.
fn margin_of<'a>(scene: &'a Scene, body: &crate::scene3d::Body) -> Option<(f64, &'a str)> {
    let height = body.height?;
    scene
        .limits
        .iter()
        .flat_map(|l| l.faces.iter().map(move |f| (l, f)))
        .filter(|(_, f)| f.side == body.side && f.under.contains(&body.path))
        .map(|(l, _)| {
            let m = l.max_height - height;
            ((m * 1e6).round() / 1e6, l.name.as_str())
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

/// The scene as a `.glb` file, for the board labelled `board` (its name, or `system/member`).
pub fn glb(scene: &Scene, board: &str) -> Vec<u8> {
    let mut b = Builder::default();
    let root = board_node(&mut b, scene, board, None);
    finish(b, root, board)
}

/// The `board` node of `scene` and its children, their meshes added to `b`. For a member of a
/// system (`member`), every node's and mesh's name starts with `member/` and the board and each
/// part say which member they are of; with none the names are the board's own.
fn board_node(b: &mut Builder, scene: &Scene, board: &str, member: Option<&str>) -> Node {
    let prefix = member.map(|m| format!("{m}/")).unwrap_or_default();
    let named = |name: &str| format!("{prefix}{name}");
    let pieces = pieces(scene);
    let solid = &scene.board;
    let coverage = scene.coverage();

    // The substrate, its masks and its barrels.
    let mut substrate = Node::new(named("substrate"), json!({ "kind": "substrate" }));
    let laminate: Vec<&Mesh> = pieces.board.iter().collect();
    substrate.mesh = b.mesh(&named("substrate"), &laminate);
    for m in &pieces.masks {
        let side = face(m.side);
        let name = named(&format!("mask:{side}"));
        let mut node = Node::new(&name, json!({ "kind": "mask", "face": side }));
        let parts: Vec<&Mesh> = m.bare.iter().chain(&m.over_copper).collect();
        node.mesh = b.mesh(&name, &parts);
        substrate.children.push(node);
    }
    for (h, wall) in solid.holes.iter().zip(&pieces.walls) {
        if !h.plated {
            continue;
        }
        let (name, extras) = match &h.owner {
            HoleOwner::Pad { path, refdes, pad } => (
                named(&format!("hole:{refdes}.{pad}")),
                json!({ "kind": "hole", "path": path, "refdes": refdes, "pad": pad }),
            ),
            HoleOwner::Via { net } => (
                named(&format!("via:{net}")),
                json!({ "kind": "hole", "net": net }),
            ),
        };
        let mut node = Node::new(&name, extras);
        node.mesh = wall.as_ref().and_then(|w| b.mesh(&name, &[w]));
        substrate.children.push(node);
    }
    for s in &pieces.silk {
        let side = face(s.side);
        let name = named(&format!("silk:{side}"));
        let mut node = Node::new(&name, json!({ "kind": "silk", "face": side }));
        node.mesh = s.mesh.as_ref().and_then(|m| b.mesh(&name, &[m]));
        substrate.children.push(node);
    }

    let mut root = Node::new(
        named("board"),
        json!({
            "kind": "board",
            "schema": SCHEMA,
            "board": board,
            "thickness_mm": solid.thickness,
            "stack_thickness_mm": solid.stack_thickness,
            "width_mm": solid.width,
            "height_mm": solid.height,
            "layers": solid.layers.iter().map(|l| json!({
                "name": l.name,
                "kind": l.kind.word(),
                "thickness_mm": l.thickness,
                "z_top_mm": l.z_top,
                "z_bottom_mm": l.z_bottom,
            })).collect::<Vec<_>>(),
            "coverage": { "known": coverage.known, "total": coverage.total, "standard": coverage.standard },
            "findings": findings(scene),
        }),
    );
    root.children.push(substrate);

    // The parts.
    let empty = BodyPieces::default();
    for (i, body) in scene.bodies.iter().enumerate() {
        let piece = pieces.bodies.get(i).unwrap_or(&empty);
        let (z_bottom, z_top) = body.z_range();
        let margin = margin_of(scene, body);
        let name = named(&body.refdes);
        let mut node = Node::new(
            &name,
            json!({
                "kind": "part",
                "path": body.path,
                "refdes": body.refdes,
                "side": face(body.side),
                "provenance": body.provenance.word(),
                "standard": body.standard.map(|s| s.cited()),
                "height_mm": body.height,
                "z_bottom_mm": z_bottom,
                "z_top_mm": z_top,
                "part_kind": body.kind,
                "category": body.category.word(),
                "margin_mm": margin.map(|(m, _)| m),
                "margin_limit": margin.map(|(_, name)| name),
                "model": body.model.as_ref().map(|m| json!({
                    "file": m.file,
                    "sha256": m.sha256,
                    "top_mm": m.top,
                })),
            }),
        );
        if let Some(member) = member {
            node.extras["member"] = json!(member);
        }
        node.mesh = b.mesh(&name, &piece.body.iter().collect::<Vec<_>>());
        root.children.push(node);
    }

    // The height limits, one node per face.
    let mut limits = Node::new(named("limits"), json!({ "kind": "limits" }));
    for (i, limit) in scene.limits.iter().enumerate() {
        for (k, f) in limit.faces.iter().enumerate() {
            let violated = scene
                .findings
                .too_tall
                .iter()
                .any(|t| t.keepout == limit.name && t.side == f.side);
            let name = named(&format!("limit:{}:{}", limit.name, face(f.side)));
            let mut node = Node::new(
                &name,
                json!({
                    "kind": "height_limit",
                    "name": limit.name,
                    "face": face(f.side),
                    "max_height_mm": limit.max_height,
                    "tallest": f.tallest.as_ref().map(|t| t.path.clone()),
                    "margin_mm": f.margin,
                    "violated": violated,
                    "unknown": f.unknown,
                    "under": f.under,
                }),
            );
            node.mesh = pieces
                .limits
                .get(i)
                .and_then(|faces| faces.get(k))
                .and_then(Option::as_ref)
                .and_then(|m| b.mesh(&name, &[m]));
            limits.children.push(node);
        }
    }
    root.children.push(limits);

    // The pins out of the far face, where their length is stated.
    let mut protrusions = Node::new(named("protrusions"), json!({ "kind": "protrusions" }));
    for (i, body) in scene.bodies.iter().enumerate() {
        let Pins::Known { length, .. } = &body.pins else {
            continue;
        };
        let name = named(&format!("pins:{}", body.refdes));
        let mut node = Node::new(
            &name,
            json!({ "kind": "protrusion", "path": body.path, "length_mm": length }),
        );
        node.mesh = pieces
            .bodies
            .get(i)
            .and_then(|p| p.pins.as_ref())
            .and_then(|m| b.mesh(&name, &[m]));
        protrusions.children.push(node);
    }
    root.children.push(protrusions);

    // The copper, a group per layer and a node per net on it.
    if let Ok(copper) = &scene.copper {
        for (l, layer) in copper.layers.iter().zip(&pieces.copper) {
            let group_name = named(&format!("copper:{}", l.name));
            let mut group = Node::new(
                &group_name,
                json!({
                    "kind": "copper_layer",
                    "layer": l.name,
                    "z_top_mm": l.z_top,
                    "z_bottom_mm": l.z_bottom,
                }),
            );
            for n in &layer.nets {
                let label = n.net.as_deref().unwrap_or(NO_NET);
                let name = named(&format!("copper:{}:{label}", l.name));
                let mut node = Node::new(
                    &name,
                    json!({ "kind": "copper", "layer": l.name, "net": n.net }),
                );
                let parts: Vec<&Mesh> = n.copper.iter().chain(&n.finish).collect();
                node.mesh = b.mesh(&name, &parts);
                group.children.push(node);
            }
            root.children.push(group);
        }
    }

    // The case models (ADR 0109): a group per model, a node per solid.
    for (model, meshes) in scene.case.models.iter().zip(&pieces.cases) {
        let group_name = named(&format!("case:{}", model.name));
        let mut group = Node::new(
            &group_name,
            json!({
                "kind": "case",
                "name": model.name,
                "file": model.file,
                "sha256": model.sha256,
                "triangles": model.triangle_count(),
            }),
        );
        let mut drawn = meshes.iter();
        for solid in &model.solids {
            let name = named(&format!("case:{}:{}", model.name, solid.name));
            let mut node = Node::new(
                &name,
                json!({
                    "kind": "case_solid",
                    "case": model.name,
                    "solid": solid.name,
                    "closed": solid.closed,
                    "triangles": solid.triangles.len(),
                }),
            );
            if !solid.triangles.is_empty() {
                node.mesh = drawn.next().and_then(|m| b.mesh(&name, &[m]));
            }
            group.children.push(node);
        }
        root.children.push(group);
    }

    if let Some(member) = member {
        root.extras["member"] = json!(member);
    }
    root
}

/// The _System scene_ as a `.glb` file (ADR 0110): the one root `system`, and under it one node
/// per member, named by the member, that carries the member's motion as its `matrix` (in glTF's
/// frame) and holds that board's `board` subtree as [`glb`] writes it, every name starting with
/// `member/` (`led/J1`).
pub fn system_glb(system: &SystemScene) -> Vec<u8> {
    let mut b = Builder::default();
    let mut root = Node::new(
        "system",
        json!({
            "kind": "system",
            "schema": SCHEMA,
            "system": system.name,
            "base": system.members.first().map(|m| m.name.clone()),
            "mates": system.mates.iter().map(|m| json!({
                "a": m.input.a,
                "b": m.input.b,
                "a_member": m.input.a_member,
                "b_member": m.input.b_member,
                "contacts": m.contacts,
                "mated_height_mm": m.input.mated_height.map(|(lo, hi)| json!({ "min": lo, "max": hi })),
                "residual_mm": m.residual,
                "gap_mm": m.gap,
            })).collect::<Vec<_>>(),
            "findings": system.findings.iter().map(system_finding).collect::<Vec<_>>(),
        }),
    );
    for m in &system.members {
        let label = format!("{}/{}", system.name, m.name);
        let (placed, mate, apart) = match &m.place {
            Place::Base => (true, None, None),
            Place::Mated { mate } => (true, Some(*mate), None),
            Place::Apart { reason } => (false, None, Some(reason.clone())),
        };
        let mut node = Node::new(
            &m.name,
            json!({
                "kind": "member",
                "member": m.name,
                "board": label,
                "placed": placed,
                "base": m.place == Place::Base,
                "mate": mate,
                "apart": apart,
                "turn_deg": m.motion.turn,
                "flip": m.motion.flip,
                "offset_mm": m.motion.offset,
            }),
        );
        node.matrix = Some(gltf_matrix(&m.motion));
        node.children
            .push(board_node(&mut b, &m.scene, &label, Some(&m.name)));
        root.children.push(node);
    }
    finish(b, root, &system.name)
}

/// A finding of the system scene, as the `system` root lists it.
fn system_finding(f: &Finding) -> Value {
    json!({
        "code": f.code.word(),
        "severity": f.code.severity(),
        "message": f.message,
        "members": f.members,
        "parts": f.parts,
        "mate": f.mate.as_ref().map(|(a, b)| json!([a, b])),
        "z_bottom_mm": f.z.map(|z| z.0),
        "z_top_mm": f.z.map(|z| z.1),
        "distance_mm": f.distance,
    })
}

/// A member's motion (mm, Z up) as a glTF node's matrix (m, Y up), column by column: the same
/// motion seen through [`position`], `(x, y, z) -> (x, z, -y) * 0.001`.
fn gltf_matrix(motion: &Motion) -> [f64; 16] {
    // The scene's axes in glTF's: P, and its inverse, the transpose.
    let p = [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]];
    let l = motion.linear();
    let mul = |a: [[f64; 3]; 3], b: [[f64; 3]; 3]| {
        [0, 1, 2].map(|r| [0, 1, 2].map(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum::<f64>()))
    };
    let pt = [0, 1, 2].map(|r| [0, 1, 2].map(|c| p[c][r]));
    let g = mul(mul(p, l), pt);
    let o = motion.offset;
    let t = [0, 1, 2].map(|r| (0..3).map(|k| p[r][k] * o[k]).sum::<f64>() * 0.001);
    let clean = |v: f64| {
        let f = f64::from(f32_of(v));
        if f == 0.0 { 0.0 } else { f }
    };
    let mut out = [0.0; 16];
    for c in 0..3 {
        for r in 0..3 {
            out[c * 4 + r] = clean(g[r][c]);
        }
    }
    for r in 0..3 {
        out[12 + r] = clean(t[r]);
    }
    out[15] = 1.0;
    out
}

/// The file of `b` with the one root `root`, the scene named `name`.
fn finish(mut b: Builder, root: Node, name: &str) -> Vec<u8> {
    let root = b.node(root);
    let mut doc = json!({
        "asset": { "version": "2.0", "generator": "netsmith" },
        "scene": 0,
        "scenes": [{ "name": name, "nodes": [root] }],
        "nodes": b.nodes,
        "materials": materials(&b.colours),
    });
    if !b.meshes.is_empty() {
        doc["meshes"] = json!(b.meshes);
        doc["accessors"] = json!(b.accessors);
        doc["bufferViews"] = json!(b.views);
        doc["buffers"] = json!([{ "byteLength": b.bin.len() }]);
    }
    container(&doc, b.bin)
}

/// The GLB container: the header, the JSON chunk padded with spaces, the binary chunk padded
/// with zeros.
fn container(doc: &Value, mut bin: Vec<u8>) -> Vec<u8> {
    let mut text = serde_json::to_vec(doc).expect("the document is plain data");
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let bin_chunk = if bin.is_empty() { 0 } else { 8 + bin.len() };
    let total = 12 + 8 + text.len() + bin_chunk;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&text);
    if !bin.is_empty() {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
    }
    out
}

/// The JSON chunk of a `.glb` file, for a test or a tool that reads the ids: `None` if the
/// bytes are not a GLB file.
pub fn json_chunk(glb: &[u8]) -> Option<Value> {
    let word = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(glb.get(at..at + 4)?.try_into().ok()?))
    };
    if glb.get(0..4)? != b"glTF" || word(4)? != 2 || glb.get(16..20)? != b"JSON" {
        return None;
    }
    let len = word(12)? as usize;
    serde_json::from_slice(glb.get(20..20 + len)?).ok()
}

/// The binary chunk of a `.glb` file, `None` if it has none.
#[cfg(test)]
pub(crate) fn bin_chunk(glb: &[u8]) -> Option<&[u8]> {
    let word = |at: usize| -> Option<usize> {
        Some(u32::from_le_bytes(glb.get(at..at + 4)?.try_into().ok()?) as usize)
    };
    let at = 20 + word(12)?;
    let len = word(at)?;
    if glb.get(at + 4..at + 8)? != b"BIN\0" {
        return None;
    }
    glb.get(at + 8..at + 8 + len)
}

#[cfg(test)]
mod tests;
