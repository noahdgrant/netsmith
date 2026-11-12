//! `netsmith board3d`: the board's _Scene_ (ADR 0105, `netsmith_pcb::scene3d`) as numbers, for an
//! agent or a person: the board's stackup, each part's body box with its Z and _Height
//! provenance_, each height limit with the tallest part under it and the margin, bodies that
//! overlap in 3D, the parts of unknown height with the reason, and the _Height coverage_; and a
//! summary of the copper (per layer its Z, its pads, tracks, vias and pours, and each net's area),
//! not its polygons.
//!
//! The board is placed as `fab` and the editor place it ([`crate::layout::place`], the layout
//! file respected); nothing is routed, written or asked of KiCad. The JSON report is the same
//! bytes for the same source: every list is in a fixed order and every length is rounded to a
//! nanometre. Its fields are described in `docs/BOARD3D.md`; [`SCHEMA`] counts its versions.

use std::collections::BTreeMap;

use netsmith_pcb::scene3d::{Body, HeightProvenance, HoleOwner, Pins, Plan, PlanBox, Scene};
use netsmith_pcb::{Board, Side};
use serde::Serialize;

use crate::Built;
use crate::heights::CatalogHeight;

/// The version of the JSON report's shape: a field removed or changed in meaning raises it; a
/// field added does not.
pub const SCHEMA: u32 = 2;

/// The frame, as the report states it for a reader that has not read this module.
pub const FRAME: &str = "millimetres, right-handed, z up; the origin is the outline's bottom-left corner on the board's top face (z = 0), x right, y up toward the outline's top edge, the bottom face at z = -thickness; a layout-file point (x, y) (from the top-left corner, y down) is (x, outline height - y) here";

/// The scene of a placed board, with where each height came from as `heights.rs` says.
pub fn scene(built: &Built, board: &Board) -> Scene {
    Scene::of(
        board,
        &crate::heights::provenance(&built.design, &built.catalog_heights),
    )
}

/// The scene of a placed board as a `.glb` file (`netsmith_pcb::gltf`), labelled as the report
/// labels it: what `board3d --glb` writes and the editor's `GET /api/pcb/3d` serves.
pub fn glb(built: &Built, board: &Board) -> Vec<u8> {
    netsmith_pcb::gltf::glb(&scene(built, board), &built.label())
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u32,
    pub units: &'static str,
    pub frame: &'static str,
    pub board: BoardReport,
    pub copper: CopperReport,
    pub parts: Vec<PartReport>,
    pub height_limits: Vec<LimitReport>,
    pub too_tall: Vec<TooTallReport>,
    pub overlaps: Vec<OverlapReport>,
    pub unknown_heights: Vec<UnknownReport>,
    pub coverage: CoverageReport,
    pub tallest: TallestReport,
}

#[derive(Debug, Serialize)]
pub struct BoardReport {
    /// The board's label: its name, or `system/member` for a board of a system.
    pub name: String,
    pub outline: OutlineReport,
    pub thickness: f64,
    pub stack_thickness: f64,
    pub layers: Vec<LayerReport>,
    pub cutouts: Vec<CutoutReport>,
    pub holes: Vec<HoleReport>,
}

/// The copper in numbers: no polygons.
#[derive(Debug, Serialize)]
pub struct CopperReport {
    /// From the top face down.
    pub layers: Vec<CopperLayerReport>,
    /// The vias of the board: each is a barrel through it with a ring on every copper layer.
    pub vias: usize,
    /// How many openings of the solder mask each face has.
    pub mask_openings: MaskReport,
}

#[derive(Debug, Serialize)]
pub struct MaskReport {
    pub top: usize,
    pub bottom: usize,
}

#[derive(Debug, Serialize)]
pub struct CopperLayerReport {
    pub name: String,
    pub z_top: f64,
    pub z_bottom: f64,
    pub thickness: f64,
    pub pads: usize,
    pub tracks: usize,
    /// The vias, which have a ring on every layer.
    pub vias: usize,
    /// The polygons of pours (outer layers) and planes (inner layers).
    pub pours: usize,
    /// The copper of each net on the layer in mm², to 1e-4 mm².
    pub nets: Vec<NetArea>,
}

#[derive(Debug, Serialize)]
pub struct NetArea {
    /// `null` for copper that belongs to no net.
    pub net: Option<String>,
    pub area: f64,
}

#[derive(Debug, Serialize)]
pub struct OutlineReport {
    pub width: f64,
    pub height: f64,
    pub corner_radius: f64,
}

#[derive(Debug, Serialize)]
pub struct LayerReport {
    pub name: String,
    pub kind: &'static str,
    pub thickness: f64,
    pub z_top: f64,
    pub z_bottom: f64,
}

#[derive(Debug, Serialize)]
pub struct CutoutReport {
    pub name: String,
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub radius: f64,
}

#[derive(Debug, Serialize)]
pub struct HoleReport {
    /// `pad` or `via`.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refdes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pad: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<String>,
    pub from: [f64; 2],
    pub to: [f64; 2],
    pub diameter: f64,
    pub plated: bool,
}

#[derive(Debug, Serialize)]
pub struct PartReport {
    pub path: String,
    pub refdes: String,
    pub kind: String,
    pub side: &'static str,
    /// The footprint's origin and turn as the layout file writes them.
    pub layout: LayoutPlace,
    pub body: BodyBox,
    pub z_bottom: Option<f64>,
    pub z_top: Option<f64>,
    pub height: Option<f64>,
    pub provenance: &'static str,
    /// The catalog row the height is from, for a `catalog` height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_row: Option<String>,
    /// The pins below the far face, for a part with plated holes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pins: Option<PinsReport>,
}

#[derive(Debug, Serialize)]
pub struct LayoutPlace {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
}

#[derive(Debug, Serialize)]
pub struct BodyBox {
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub corners: [[f64; 2]; 4],
    /// `fab` (the footprint's fabrication outline) or `courtyard`.
    pub from: &'static str,
}

#[derive(Debug, Serialize)]
pub struct PinsReport {
    pub min: [f64; 2],
    pub max: [f64; 2],
    /// How far they stand out of the far face; `null` when the design states no protrusion.
    pub protrusion: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub z_top: Option<f64>,
    pub z_bottom: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct LimitReport {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carrier: Option<String>,
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub max_height: f64,
    pub faces: Vec<&'static str>,
    /// The tallest part under it on any face, and the least margin.
    pub tallest: Option<TallestPart>,
    pub margin: Option<f64>,
    pub per_face: Vec<LimitFaceReport>,
}

#[derive(Debug, Serialize)]
pub struct LimitFaceReport {
    pub face: &'static str,
    pub z_bottom: f64,
    pub z_top: f64,
    pub tallest: Option<TallestPart>,
    pub margin: Option<f64>,
    /// The parts of unknown height under it on this face, by path.
    pub unknown: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TallestPart {
    pub path: String,
    pub refdes: String,
    pub height: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct TooTallReport {
    pub path: String,
    pub refdes: String,
    pub limit_name: String,
    pub face: &'static str,
    pub height: f64,
    pub limit: f64,
}

#[derive(Debug, Serialize)]
pub struct OverlapReport {
    pub a: String,
    pub b: String,
    pub a_refdes: String,
    pub b_refdes: String,
    pub face: &'static str,
    pub z_bottom: f64,
    pub z_top: f64,
}

#[derive(Debug, Serialize)]
pub struct UnknownReport {
    pub path: String,
    pub refdes: String,
    pub reason: String,
    /// The height limits it stands under, whose check is left unchecked by it.
    pub under_limits: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CoverageReport {
    pub known: usize,
    pub total: usize,
    pub stated: usize,
    pub catalog: usize,
}

#[derive(Debug, Serialize)]
pub struct TallestReport {
    pub top: Option<TallestPart>,
    pub bottom: Option<TallestPart>,
}

fn xy(p: Plan) -> [f64; 2] {
    [p.x, p.y]
}

fn face(side: Side) -> &'static str {
    side.word()
}

fn tallest(t: Option<netsmith_pcb::scene3d::Tallest>, side: Option<Side>) -> Option<TallestPart> {
    t.map(|t| TallestPart {
        path: t.path,
        refdes: t.refdes,
        height: t.height,
        face: side.map(face),
    })
}

/// Why a part has no height: what the design and its catalog row lack.
fn reason(body: &Body, built: &Built, rows: &BTreeMap<&str, String>) -> String {
    match rows.get(body.path.as_str()) {
        Some(row) => format!(
            "the design states no `.height` and the `height` cell of its catalog row {row} is empty (state it: `.{}.height &= <mm>`, from the datasheet)",
            body.path
        ),
        None if built.design.parts.iter().any(|p| p.path == body.path) => format!(
            "the design states no `.height` and the part has no catalog row to take one from (state it: `.{}.height &= <mm>`, from the datasheet)",
            body.path
        ),
        None => "the design states no `.height`".to_string(),
    }
}

/// The report of `scene`, the scene of `built`'s placed board. `sourcing` is the board's sourcing
/// with its generic parts resolved to rows, as `fab` orders them, to name a row with no height.
pub fn report(
    built: &Built,
    scene: &Scene,
    sourcing: &[netsmith_design::Sourced],
) -> Result<Report, String> {
    let copper = copper_report(scene)?;
    let rows: BTreeMap<&str, String> = built
        .design
        .parts
        .iter()
        .zip(sourcing)
        .filter_map(|(p, s)| {
            let row = CatalogHeight {
                refdes: p.refdes.clone(),
                path: p.path.clone(),
                height: 0.0,
                lcsc: s.lcsc.clone(),
                mpn: s.mpn.clone(),
            };
            (s.lcsc.is_some() || s.mpn.is_some()).then(|| (p.path.as_str(), row.row()))
        })
        .collect();
    let catalog: BTreeMap<&str, String> = built
        .catalog_heights
        .iter()
        .map(|c| (c.path.as_str(), c.row()))
        .collect();
    let solid = &scene.board;
    let board = BoardReport {
        name: built.label(),
        outline: OutlineReport {
            width: solid.width,
            height: solid.height,
            corner_radius: solid.corner_radius,
        },
        thickness: solid.thickness,
        stack_thickness: solid.stack_thickness,
        layers: solid
            .layers
            .iter()
            .map(|l| LayerReport {
                name: l.name.clone(),
                kind: l.kind.word(),
                thickness: l.thickness,
                z_top: l.z_top,
                z_bottom: l.z_bottom,
            })
            .collect(),
        cutouts: solid
            .cutouts
            .iter()
            .map(|c| CutoutReport {
                name: c.name.clone(),
                min: xy(c.at.min),
                max: xy(c.at.max),
                radius: c.radius,
            })
            .collect(),
        holes: solid
            .holes
            .iter()
            .map(|h| {
                let (kind, path, refdes, pad, net) = match &h.owner {
                    HoleOwner::Pad { path, refdes, pad } => (
                        "pad",
                        Some(path.clone()),
                        Some(refdes.clone()),
                        Some(pad.clone()),
                        None,
                    ),
                    HoleOwner::Via { net } => ("via", None, None, None, Some(net.clone())),
                };
                HoleReport {
                    kind,
                    path,
                    refdes,
                    pad,
                    net,
                    from: xy(h.from),
                    to: xy(h.to),
                    diameter: h.diameter,
                    plated: h.plated,
                }
            })
            .collect(),
    };
    let parts = scene
        .bodies
        .iter()
        .map(|b| {
            let (z_bottom, z_top) = b.z_range();
            let pins = match &b.pins {
                Pins::None => None,
                Pins::Unknown { at } => Some(pins_report(at, None, None, None, None)),
                Pins::Known {
                    at,
                    length,
                    source,
                    z_top,
                    z_bottom,
                } => Some(pins_report(
                    at,
                    Some(*length),
                    Some(source.clone()),
                    Some(*z_top),
                    Some(*z_bottom),
                )),
            };
            PartReport {
                path: b.path.clone(),
                refdes: b.refdes.clone(),
                kind: b.kind.clone(),
                side: face(b.side),
                layout: LayoutPlace {
                    x: b.layout_at.0,
                    y: b.layout_at.1,
                    rotation: b.rotation,
                },
                body: BodyBox {
                    min: xy(b.bounds.min),
                    max: xy(b.bounds.max),
                    corners: b.corners.map(xy),
                    from: if b.body_is_courtyard {
                        "courtyard"
                    } else {
                        "fab"
                    },
                },
                z_bottom,
                z_top,
                height: b.height,
                provenance: b.provenance.word(),
                catalog_row: (b.provenance == HeightProvenance::Catalog)
                    .then(|| catalog.get(b.path.as_str()).cloned())
                    .flatten(),
                pins,
            }
        })
        .collect();
    let height_limits = scene
        .limits
        .iter()
        .map(|l| {
            let per_face: Vec<LimitFaceReport> = l
                .faces
                .iter()
                .map(|f| LimitFaceReport {
                    face: face(f.side),
                    z_bottom: f.z_bottom,
                    z_top: f.z_top,
                    tallest: tallest(f.tallest.clone(), Some(f.side)),
                    margin: f.margin,
                    unknown: f.unknown.clone(),
                })
                .collect();
            // The tallest on any face is the one with the least margin; the first face wins a tie.
            let worst = per_face
                .iter()
                .filter(|f| f.margin.is_some())
                .min_by(|a, b| {
                    a.margin
                        .unwrap_or_default()
                        .total_cmp(&b.margin.unwrap_or_default())
                });
            LimitReport {
                name: l.name.clone(),
                carrier: l.carrier.clone(),
                min: xy(l.at.min),
                max: xy(l.at.max),
                max_height: l.max_height,
                faces: l.faces.iter().map(|f| face(f.side)).collect(),
                tallest: worst.and_then(|f| f.tallest.clone()),
                margin: worst.and_then(|f| f.margin),
                per_face,
            }
        })
        .collect();
    let findings = &scene.findings;
    let unknown_heights = scene
        .bodies
        .iter()
        .filter(|b| b.provenance == HeightProvenance::Unknown)
        .map(|b| {
            let mut under: Vec<String> = findings
                .unchecked
                .iter()
                .filter(|u| u.path == b.path)
                .map(|u| u.keepout.clone())
                .collect();
            under.dedup();
            UnknownReport {
                path: b.path.clone(),
                refdes: b.refdes.clone(),
                reason: reason(b, built, &rows),
                under_limits: under,
            }
        })
        .collect();
    let coverage = scene.coverage();
    Ok(Report {
        schema: SCHEMA,
        units: "mm",
        frame: FRAME,
        board,
        copper,
        parts,
        height_limits,
        too_tall: findings
            .too_tall
            .iter()
            .map(|t| TooTallReport {
                path: t.path.clone(),
                refdes: t.refdes.clone(),
                limit_name: t.keepout.clone(),
                face: face(t.side),
                height: t.height,
                limit: t.limit,
            })
            .collect(),
        overlaps: findings
            .overlaps
            .iter()
            .map(|o| OverlapReport {
                a: o.a.clone(),
                b: o.b.clone(),
                a_refdes: o.a_refdes.clone(),
                b_refdes: o.b_refdes.clone(),
                face: face(o.side),
                z_bottom: o.z_bottom,
                z_top: o.z_top,
            })
            .collect(),
        unknown_heights,
        coverage: CoverageReport {
            known: coverage.known,
            total: coverage.total,
            stated: coverage.stated,
            catalog: coverage.catalog,
        },
        tallest: TallestReport {
            top: tallest(scene.tallest(Side::Front), None),
            bottom: tallest(scene.tallest(Side::Back), None),
        },
    })
}

/// An area to 1e-4 mm², with no negative zero.
fn area(mm2: f64) -> f64 {
    let r = (mm2 * 1e4).round() / 1e4;
    if r == 0.0 { 0.0 } else { r }
}

fn copper_report(scene: &Scene) -> Result<CopperReport, String> {
    let copper = scene.copper.as_ref().map_err(|e| e.to_string())?;
    let openings = |side: Side| {
        copper
            .mask
            .iter()
            .filter(|m| m.side == side)
            .map(|m| m.areas.len())
            .sum()
    };
    Ok(CopperReport {
        layers: copper
            .layers
            .iter()
            .map(|l| CopperLayerReport {
                name: l.name.clone(),
                z_top: l.z_top,
                z_bottom: l.z_bottom,
                thickness: l.thickness,
                pads: l.pads.len(),
                tracks: l.tracks.len(),
                vias: copper.vias.len(),
                pours: l.pours.len(),
                nets: l
                    .nets()
                    .into_iter()
                    .map(|net| NetArea {
                        net: net.map(str::to_string),
                        area: area(l.area_of(net)),
                    })
                    .collect(),
            })
            .collect(),
        vias: copper.vias.len(),
        mask_openings: MaskReport {
            top: openings(Side::Front),
            bottom: openings(Side::Back),
        },
    })
}

fn pins_report(
    at: &PlanBox,
    protrusion: Option<f64>,
    source: Option<String>,
    z_top: Option<f64>,
    z_bottom: Option<f64>,
) -> PinsReport {
    PinsReport {
        min: xy(at.min),
        max: xy(at.max),
        protrusion,
        source,
        z_top,
        z_bottom,
    }
}

impl Report {
    /// The report as JSON, pretty-printed, with a final newline.
    pub fn json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("the report is plain data");
        text.push('\n');
        text
    }

    /// A short summary for a person.
    pub fn summary(&self) -> String {
        let b = &self.board;
        let copper = b.layers.iter().filter(|l| l.kind == "copper").count();
        let mut text = format!(
            "{}: {} x {} mm, {} mm thick, {copper} copper layers\n",
            b.name, b.outline.width, b.outline.height, b.thickness
        );
        let c = &self.coverage;
        text += &format!(
            "heights: {} of {} parts known ({} stated, {} from catalog rows)\n",
            c.known, c.total, c.stated, c.catalog
        );
        for (word, t) in [("top", &self.tallest.top), ("bottom", &self.tallest.bottom)] {
            if let Some(t) = t {
                text += &format!(
                    "tallest on the {word} face: {} (`{}`) {:.2} mm\n",
                    t.refdes, t.path, t.height
                );
            }
        }
        for l in &self.height_limits {
            text += &format!(
                "limit `{}` ({}, {:.2} mm): ",
                l.name,
                l.faces.join(" and "),
                l.max_height
            );
            text += &match (&l.tallest, l.margin) {
                (Some(t), Some(m)) => format!(
                    "tallest {} {:.2} mm, margin {:.2} mm",
                    t.refdes, t.height, m
                ),
                _ => "no part of known height under it".to_string(),
            };
            let unknown: Vec<&str> = l
                .per_face
                .iter()
                .flat_map(|f| f.unknown.iter().map(String::as_str))
                .collect();
            if !unknown.is_empty() {
                text += &format!("; unknown height: `{}`", unknown.join("`, `"));
            }
            text.push('\n');
        }
        for t in &self.too_tall {
            text += &format!(
                "too tall: {} (`{}`) is {:.2} mm under `{}`, which allows {:.2} mm on the {} face\n",
                t.refdes, t.path, t.height, t.limit_name, t.limit, t.face
            );
        }
        for o in &self.overlaps {
            text += &format!(
                "overlap: {} and {} share space on the {} face, z {:.2} to {:.2} mm\n",
                o.a_refdes, o.b_refdes, o.face, o.z_bottom, o.z_top
            );
        }
        for u in &self.unknown_heights {
            text += &format!(
                "unknown height: {} (`{}`): {}\n",
                u.refdes, u.path, u.reason
            );
        }
        text
    }
}

/// The report of one board of the source `text` (named `name`), placed with the layout file
/// `layout` if it exists, and the scene it was made from (for the `.glb` file). `board` picks it
/// by name or label (`system/member`); with none, the source must hold one board. The error is
/// the message to print.
pub fn run(
    name: &str,
    text: &str,
    loader: &netsmith_check::Loader,
    catalogs: &netsmith_solve::Catalogs,
    libraries: &crate::Libraries,
    layout: Option<&std::path::Path>,
    board: Option<&str>,
) -> Result<(Report, Scene), String> {
    placed(
        name,
        text,
        loader,
        catalogs,
        libraries,
        layout,
        board,
        |built, board| {
            let (sourcing, _) = crate::resolve::resolve_generics(&built.sourcing, catalogs);
            let scene = scene(built, board);
            match report(built, &scene, &sourcing) {
                Ok(report) => Ok((report, scene)),
                Err(e) => Err(format!("error: {}: {e}\n", built.label())),
            }
        },
    )
    .and_then(|made| made)
}

/// The scene of one board, picked and placed as [`run`] does: what the rasterizer and the
/// `.glb` writer draw.
pub fn scene_of(
    name: &str,
    text: &str,
    loader: &netsmith_check::Loader,
    catalogs: &netsmith_solve::Catalogs,
    libraries: &crate::Libraries,
    layout: Option<&std::path::Path>,
    board: Option<&str>,
) -> Result<Scene, String> {
    scene_and_label(name, text, loader, catalogs, libraries, layout, board).map(|(scene, _)| scene)
}

/// [`scene_of`] with the board's label (its name, or `system/member`), which `render` names its
/// file from.
pub fn scene_and_label(
    name: &str,
    text: &str,
    loader: &netsmith_check::Loader,
    catalogs: &netsmith_solve::Catalogs,
    libraries: &crate::Libraries,
    layout: Option<&std::path::Path>,
    board: Option<&str>,
) -> Result<(Scene, String), String> {
    placed(
        name,
        text,
        loader,
        catalogs,
        libraries,
        layout,
        board,
        |built, board| (scene(built, board), built.label()),
    )
}

/// One source file and what builds and places its board: the arguments of [`run`] and
/// [`scene_and_label`], held together so `board3d` and `render` pick, build and place the board
/// by the one path.
pub struct Placed<'a> {
    pub name: &'a str,
    pub text: &'a str,
    pub loader: &'a netsmith_check::Loader,
    pub catalogs: &'a netsmith_solve::Catalogs,
    pub libraries: &'a crate::Libraries,
    /// The layout file beside the source (read if it exists).
    pub layout: std::path::PathBuf,
    /// The board to pick, by name or label.
    pub board: Option<&'a str>,
}

impl Placed<'_> {
    /// [`run`]: the report and the scene.
    pub fn run(&self) -> Result<(Report, Scene), String> {
        run(
            self.name,
            self.text,
            self.loader,
            self.catalogs,
            self.libraries,
            Some(&self.layout),
            self.board,
        )
    }

    /// [`scene_and_label`]: the scene and the board's label.
    pub fn scene_and_label(&self) -> Result<(Scene, String), String> {
        scene_and_label(
            self.name,
            self.text,
            self.loader,
            self.catalogs,
            self.libraries,
            Some(&self.layout),
            self.board,
        )
    }
}

/// `then` of the board [`run`] picks, built and placed.
#[allow(clippy::too_many_arguments)]
fn placed<T>(
    name: &str,
    text: &str,
    loader: &netsmith_check::Loader,
    catalogs: &netsmith_solve::Catalogs,
    libraries: &crate::Libraries,
    layout: Option<&std::path::Path>,
    board: Option<&str>,
    then: impl FnOnce(&Built, &Board) -> T,
) -> Result<T, String> {
    let outcome = crate::outcome_with(name, text, loader, catalogs, Some(libraries))
        .map_err(|problems| crate::render_problems(&problems))?;
    let names: Vec<String> = outcome
        .boards
        .iter()
        .map(|b| b.design.name.clone())
        .collect();
    let labels: Vec<String> = outcome.boards.iter().map(Built::label).collect();
    let built = match board {
        Some(want) => outcome
            .boards
            .iter()
            .find(|b| b.label() == want || (b.system.is_none() && b.design.name == want))
            .ok_or_else(|| {
                format!(
                    "error: {name} has no board `{want}`; its boards are: {}\n",
                    labels.join(", ")
                )
            })?,
        None => match outcome.boards.as_slice() {
            [one] => one,
            [] => return Err(format!("error: {name} has no board\n")),
            _ => {
                return Err(format!(
                    "error: {name} has {} boards; say which with `--board <name>`: {}\n",
                    labels.len(),
                    labels.join(", ")
                ));
            }
        },
    };
    let fail = |what: String| format!("error: {}: {what}\n", built.label());
    let layout = match layout {
        Some(file) => crate::layout::read(file, &names).map_err(fail)?,
        None => None,
    };
    let (board, _) = crate::layout::place(
        &built.design,
        layout.as_ref(),
        libraries,
        catalogs.constraints_warn(),
    )
    .map_err(|e| match e {
        crate::layout::PlaceFailure::Board(e) => fail(e.to_string()),
        crate::layout::PlaceFailure::Layout(e) => fail(e),
    })?;
    Ok(then(built, &board))
}
