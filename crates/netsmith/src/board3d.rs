//! `netsmith board3d`: the board's _Scene_ (ADR 0105, `netsmith_3d::scene3d`) as numbers, for an
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

use netsmith_3d::scene3d::{Body, HeightProvenance, HoleOwner, Pins, Plan, PlanBox, Scene};
use netsmith_pcb::{Board, Side};
use serde::Serialize;

use crate::Built;
use crate::heights::CatalogHeight;

/// The version of the JSON report's shape: a field removed or changed in meaning raises it; a
/// field added does not.
pub const SCHEMA: u32 = 2;

/// The frame, as the report states it for a reader that has not read this module.
pub const FRAME: &str = "millimetres, right-handed, z up; the origin is the outline's bottom-left corner on the board's top face (z = 0), x right, y up toward the outline's top edge, the bottom face at z = -thickness; a layout-file point (x, y) (from the top-left corner, y down) is (x, outline height - y) here";

/// The scene of a placed board, with where each height came from as `heights.rs` says, and
/// each part whose STEP model was found on disk ([`crate::models::locate`]) drawn from it (ADR
/// 0108). A model that cannot be read is a warning in `scene.warnings`, and its part keeps its
/// generated shape. The board's case models are read, drawn and checked (ADR 0109).
pub fn scene(built: &Built, board: &Board) -> Scene {
    // The case models, read and checked where the design has them: drawn see-through, and
    // their findings are the scene's.
    crate::case_fit::with_case(&built.design, scene_without_case(built, board))
}

/// [`scene`] with no case model: a board as a member of a system's scene (ADR 0110), where a
/// case round one board is neither drawn nor checked (a case round a system is not modelled).
pub fn scene_without_case(built: &Built, board: &Board) -> Scene {
    let mut scene = Scene::of(
        board,
        &crate::heights::provenance(&built.design, &built.catalog_heights),
    );
    let (uses, unread) = crate::models::uses(&built.design);
    scene.draw_models(&uses);
    scene.warnings.splice(0..0, unread);
    scene
}

/// The scene of a placed board as a `.glb` file (`netsmith_3d::gltf`), labelled as the report
/// labels it: what `board3d --glb` writes and the editor's `GET /api/pcb/3d` serves.
pub fn glb(built: &Built, board: &Board) -> Vec<u8> {
    netsmith_3d::gltf::glb(&scene(built, board), &built.label())
}

/// A placed board as a STEP file (`netsmith_pcb::step`), labelled as the report
/// labels it: what `board3d --step` writes.
pub fn step(built: &Built, board: &Board) -> Vec<u8> {
    netsmith_3d::step::step(&scene(built, board), &built.label()).into_bytes()
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
    pub standard_passes: Vec<StandardPassReport>,
    pub coverage: CoverageReport,
    pub tallest: TallestReport,
    /// What finding and drawing the parts' STEP models found (ADR 0108): a file that is not
    /// there or cannot be read (the part keeps its generated shape), a model whose top is
    /// above its part's height. The command prints each as a `warning:` line too.
    pub warnings: Vec<String>,
    /// The board's case models (ADR 0109), by name: each file, whether it was read, and its
    /// solids.
    pub case_models: Vec<CaseModelReport>,
    /// What the fit check found, in the check's order (`netsmith_3d::case::check`).
    pub case_findings: Vec<CaseFindingReport>,
}

/// A case model of the board (ADR 0109).
#[derive(Debug, Serialize)]
pub struct CaseModelReport {
    pub name: String,
    /// As the design states it, relative to the physical file.
    pub file: String,
    /// `read`, `missing` (the file is not there) or `unreadable` (netsmith's reader cannot read
    /// it).
    pub status: &'static str,
    /// Of the file's bytes, in hex; `null` when it was not read.
    pub sha256: Option<String>,
    /// The placement, in the scene's frame: degrees about x, then y, then z, then mm.
    pub rotation: [f64; 3],
    pub offset: [f64; 3],
    /// The check's figures, in mm (`.clearance`, `.reach`, `.boss_gap`).
    pub clearance: f64,
    pub reach: f64,
    pub boss_gap: f64,
    pub triangles: usize,
    pub solids: Vec<CaseSolidReport>,
}

#[derive(Debug, Serialize)]
pub struct CaseSolidReport {
    pub name: String,
    pub triangles: usize,
    /// Whether its shell is closed: one that is not gets no test of a body wholly inside it.
    pub closed: bool,
    pub min: [f64; 3],
    pub max: [f64; 3],
}

/// One finding of the fit check.
#[derive(Debug, Serialize)]
pub struct CaseFindingReport {
    /// `case_collision`, `case_clearance`, `case_reach`, `case_boss` or `case_unchecked`.
    pub kind: &'static str,
    /// `error` (stops `fab`, never waived) or `warning`.
    pub severity: &'static str,
    /// Whether it is an error when constraints are strict (`case_unchecked`).
    pub strict_error: bool,
    /// The case model (several joined by `, ` for a `case_boss`).
    pub case: String,
    /// The case solid; empty when it is about no one solid.
    pub solid: String,
    /// `part`, `board`, `solid` or `model`.
    pub subject: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refdes: Option<String>,
    /// Where to look, in the scene's frame.
    pub at: [f64; 3],
    /// How far the case is inside the body or the board (`case_collision`, a standard height's
    /// `case_unchecked`); `null` there for one wholly inside a solid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<Option<f64>>,
    /// The gap to the surface (`case_clearance`) or the most a solid may be from a hole's ring
    /// (`case_boss`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<f64>,
    /// The figure it is measured against (`.clearance`, `.reach`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    /// How far out from a connector's face the case material is (`case_reach`, blocked), or how
    /// far inside the wall's outer face the face stands (`case_reach`, too deep).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
    /// The board edge a `case_reach` connector stands at.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<&'static str>,
    pub message: String,
}

/// The STEP model a part is drawn from (ADR 0108).
#[derive(Debug, Serialize)]
pub struct ModelReport {
    /// The file as the design states it.
    pub file: String,
    pub sha256: String,
    /// How far the model reaches out of the part's face, in mm. A drawing, never the part's
    /// height.
    pub top: f64,
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
    /// The package standard and the drawing the height is from, for a `standard` height (ADR
    /// 0106): `JEDEC MS-012, drawing TI D0008A`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard: Option<String>,
    /// The pins below the far face, for a part with plated holes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pins: Option<PinsReport>,
    /// The STEP model the part is drawn from, for a part that names one and whose file was
    /// read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelReport>,
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
    /// The known heights that are a package standard's maximum (ADR 0106), counted in `known`.
    pub standard: usize,
}

/// A part that fits a height limit only by its package standard's height (ADR 0106): a pass,
/// but not on a figure of the part.
#[derive(Debug, Serialize)]
pub struct StandardPassReport {
    pub path: String,
    pub refdes: String,
    pub limit_name: String,
    pub face: &'static str,
    /// The standard's maximum, in mm.
    pub height: f64,
    pub limit: f64,
    /// `JEDEC MS-012, drawing TI D0008A`.
    pub standard: String,
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

fn tallest(t: Option<netsmith_3d::scene3d::Tallest>, side: Option<Side>) -> Option<TallestPart> {
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
                standard: b.standard.map(|s| s.cited()),
                pins,
                model: b.model.as_ref().map(|m| ModelReport {
                    file: m.file.clone(),
                    sha256: m.sha256.clone(),
                    top: m.top,
                }),
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
    // A part of standard height under a limit that its maximum fits: a pass to name as one. One
    // over the limit is in `findings.unchecked`, as the board's check reports it.
    let mut standard_passes = Vec::new();
    for l in &scene.limits {
        for f in &l.faces {
            for b in scene
                .bodies
                .iter()
                .filter(|b| f.under.contains(&b.path) && b.provenance == HeightProvenance::Standard)
            {
                if let (Some(h), Some(s)) = (b.height, b.standard)
                    && h <= l.max_height + 0.0005
                {
                    standard_passes.push(StandardPassReport {
                        path: b.path.clone(),
                        refdes: b.refdes.clone(),
                        limit_name: l.name.clone(),
                        face: face(f.side),
                        height: h,
                        limit: l.max_height,
                        standard: s.cited(),
                    });
                }
            }
        }
    }
    let coverage = scene.coverage();
    let fit = &scene.case;
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
        standard_passes,
        coverage: CoverageReport {
            known: coverage.known,
            total: coverage.total,
            stated: coverage.stated,
            catalog: coverage.catalog,
            standard: coverage.standard,
        },
        tallest: TallestReport {
            top: tallest(scene.tallest(Side::Front), None),
            bottom: tallest(scene.tallest(Side::Back), None),
        },
        warnings: scene.warnings.clone(),
        case_models: case_models_report(built, fit),
        case_findings: fit.findings.iter().map(case_finding_report).collect(),
    })
}

/// Each case model of the design, read or not.
fn case_models_report(built: &Built, fit: &netsmith_3d::case::CaseScene) -> Vec<CaseModelReport> {
    use netsmith_3d::case::Why;
    built
        .design
        .features
        .case_models
        .iter()
        .map(|c| {
            let read = fit.models.iter().find(|p| p.name == c.name);
            let status = match read {
                Some(_) => "read",
                None if fit
                    .findings
                    .iter()
                    .any(|f| f.case == c.name && matches!(f.why, Why::Unreadable { .. })) =>
                {
                    "unreadable"
                }
                None => "missing",
            };
            CaseModelReport {
                name: c.name.clone(),
                file: c.file.clone(),
                status,
                sha256: read.map(|p| p.sha256.clone()),
                rotation: c.rotation,
                offset: c.offset,
                clearance: c.figures.clearance,
                reach: c.figures.reach,
                boss_gap: c.figures.boss_gap,
                triangles: read.map_or(0, |p| p.triangle_count()),
                solids: read
                    .map(|p| {
                        p.solids
                            .iter()
                            .map(|s| CaseSolidReport {
                                name: s.name.clone(),
                                triangles: s.triangles.len(),
                                closed: s.closed,
                                min: s.bounds.min,
                                max: s.bounds.max,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect()
}

fn case_finding_report(f: &netsmith_3d::case::FitFinding) -> CaseFindingReport {
    use netsmith_3d::case::{Subject, Weight, Why};
    let (path, refdes, subject) = match &f.subject {
        Subject::Part { path, refdes } => (Some(path.clone()), Some(refdes.clone()), "part"),
        Subject::Board => (None, None, "board"),
        Subject::Solid => (None, None, "solid"),
        Subject::Model => (None, None, "model"),
    };
    let (mut depth, mut gap, mut limit, mut distance, mut edge) = (None, None, None, None, None);
    match &f.why {
        Why::Collision { depth: d, .. } | Why::Standard { depth: d, .. } => depth = Some(*d),
        Why::Clearance { gap: g, least } => {
            gap = Some(*g);
            limit = Some(*least);
        }
        Why::ReachBlocked {
            edge: e,
            distance: d,
        } => {
            distance = Some(*d);
            edge = Some(*e);
        }
        Why::ReachDeep {
            edge: e,
            depth: d,
            most,
        } => {
            distance = Some(*d);
            limit = Some(*most);
            edge = Some(*e);
        }
        Why::Boss { gap: g } => gap = Some(*g),
        _ => {}
    }
    CaseFindingReport {
        kind: f.kind.word(),
        severity: f.severity(false),
        strict_error: f.weight() == Weight::Unchecked,
        case: f.case.clone(),
        solid: f.solid.clone(),
        subject,
        path,
        refdes,
        at: f.at,
        depth,
        gap,
        limit,
        distance,
        edge,
        message: f.to_string(),
    }
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
            "heights: {} of {} parts known ({} stated, {} from catalog rows, {} the package standard's maximum)\n",
            c.known, c.total, c.stated, c.catalog, c.standard
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
        for p in &self.standard_passes {
            text += &format!(
                "fits by its package standard: {} (`{}`) at most {:.2} mm ({}) under `{}`, which allows {:.2} mm on the {} face\n",
                p.refdes, p.path, p.height, p.standard, p.limit_name, p.limit, p.face
            );
        }
        for u in &self.unknown_heights {
            text += &format!(
                "unknown height: {} (`{}`): {}\n",
                u.refdes, u.path, u.reason
            );
        }
        for p in &self.parts {
            if let Some(m) = &p.model {
                text += &format!(
                    "drawn from a STEP model: {} (`{}`) from `{}`, {:.2} mm out of its face\n",
                    p.refdes, p.path, m.file, m.top
                );
            }
        }
        for c in &self.case_models {
            text += &match &c.sha256 {
                Some(_) => format!(
                    "case model `{}` (`{}`): {} solid(s), {} triangles\n",
                    c.name,
                    c.file,
                    c.solids.len(),
                    c.triangles
                ),
                None => format!("case model `{}` (`{}`): {}\n", c.name, c.file, c.status),
            };
        }
        for f in &self.case_findings {
            text += &format!("{}: {}: {}\n", f.severity, f.kind, f.message);
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
    run_with(
        name,
        text,
        loader,
        catalogs,
        libraries,
        layout,
        board,
        crate::layout::DEFAULT_PLACE_SEEDS,
        None,
    )
}

/// [`run`] with the number of seeds the placer tries (`place_seeds` in `netsmith.toml`) and
/// where the target's files are, to find the parts' STEP models (ADR 0108).
#[allow(clippy::too_many_arguments)]
fn run_with(
    name: &str,
    text: &str,
    loader: &netsmith_check::Loader,
    catalogs: &netsmith_solve::Catalogs,
    libraries: &crate::Libraries,
    layout: Option<&std::path::Path>,
    board: Option<&str>,
    seeds: u32,
    models: Option<&crate::models::Places>,
) -> Result<(Report, Scene), String> {
    placed(
        name,
        text,
        loader,
        catalogs,
        libraries,
        layout,
        board,
        seeds,
        models,
        |built, board, found| {
            let (sourcing, _) = crate::resolve::resolve_generics(&built.sourcing, catalogs);
            let mut scene = scene(built, board);
            scene.warnings.splice(0..0, found);
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
        crate::layout::DEFAULT_PLACE_SEEDS,
        None,
        |built, board, found| (scene_with(built, board, found), built.label()),
    )
}

/// [`scene`] with the warnings of finding its models ([`crate::models::locate`]) first.
fn scene_with(built: &Built, board: &Board, found: Vec<String>) -> Scene {
    let mut scene = scene(built, board);
    scene.warnings.splice(0..0, found);
    scene
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
    /// How many seeds the placer tries (`place_seeds` in `netsmith.toml`).
    pub seeds: u32,
    /// Where the target's files are, to find the STEP models its parts are drawn from (ADR
    /// 0108): a path outside the project is an error, a file that is not there a warning in
    /// the scene. `None` finds none, and every part is drawn with its generated shape.
    pub models: Option<crate::models::Places>,
}

impl Placed<'_> {
    /// [`run`]: the report and the scene.
    pub fn run(&self) -> Result<(Report, Scene), String> {
        run_with(
            self.name,
            self.text,
            self.loader,
            self.catalogs,
            self.libraries,
            Some(&self.layout),
            self.board,
            self.seeds,
            self.models.as_ref(),
        )
    }

    /// [`scene_and_label`]: the scene and the board's label.
    pub fn scene_and_label(&self) -> Result<(Scene, String), String> {
        placed(
            self.name,
            self.text,
            self.loader,
            self.catalogs,
            self.libraries,
            Some(&self.layout),
            self.board,
            self.seeds,
            self.models.as_ref(),
            |built, board, found| (scene_with(built, board, found), built.label()),
        )
    }
}

/// What `board3d` made of what `--board` names: a board's report and scene, or a system's.
pub enum Made {
    Board(Box<(Report, Scene)>),
    System(
        Box<(
            crate::system3d::SystemReport,
            netsmith_3d::system::SystemScene,
        )>,
    ),
}

impl Placed<'_> {
    /// [`run`], or for the name of a system its system scene and report (ADR 0110), each member
    /// placed as `--board System/member` places it.
    pub fn run_any(&self) -> Result<Made, String> {
        let catalogs = self.catalogs;
        picked(
            self.name,
            self.text,
            self.loader,
            self.catalogs,
            self.libraries,
            Some(&self.layout),
            self.board,
            true,
            self.seeds,
            self.models.as_ref(),
            |built, board, found| {
                let (sourcing, _) = crate::resolve::resolve_generics(&built.sourcing, catalogs);
                let scene = scene_with(built, board, found);
                report(built, &scene, &sourcing)
                    .map(|r| Made::Board(Box::new((r, scene))))
                    .map_err(|e| format!("error: {}: {e}\n", built.label()))
            },
            |solved, members, found| {
                let system = system_with(solved, members, found);
                let mut reports = Vec::new();
                for ((built, _), member) in members.iter().zip(&system.members) {
                    let (sourcing, _) = crate::resolve::resolve_generics(&built.sourcing, catalogs);
                    reports.push(
                        report(built, &member.scene, &sourcing)
                            .map_err(|e| format!("error: {}: {e}\n", built.label()))?,
                    );
                }
                let report = crate::system3d::report(&system, reports);
                Ok(Made::System(Box::new((report, system))))
            },
        )
        .and_then(|made| made)
    }

    /// [`Placed::scene_and_label`], or for the name of a system its system scene and name.
    pub fn any_scene(&self) -> Result<AnyScene, String> {
        picked(
            self.name,
            self.text,
            self.loader,
            self.catalogs,
            self.libraries,
            Some(&self.layout),
            self.board,
            true,
            self.seeds,
            self.models.as_ref(),
            |built, board, found| {
                AnyScene::Board(Box::new(scene_with(built, board, found)), built.label())
            },
            |solved, members, found| {
                AnyScene::System(Box::new(system_with(solved, members, found)))
            },
        )
    }
}

/// The system scene of `members`, each member's parts drawn from their STEP models as a
/// board's are, with the warnings of finding each member's models (`found`, by its label)
/// first among its scene's.
fn system_with(
    solved: &crate::SolvedSystem,
    members: &[(&Built, &Board)],
    found: &BTreeMap<String, Vec<String>>,
) -> netsmith_3d::system::SystemScene {
    let mut system = crate::system3d::scene(solved, members);
    for ((built, _), member) in members.iter().zip(&mut system.members) {
        if let Some(found) = found.get(&built.label()) {
            member.scene.warnings.splice(0..0, found.iter().cloned());
        }
    }
    system
}

/// A board's scene with its label, or a system's scene.
pub enum AnyScene {
    Board(Box<Scene>, String),
    System(Box<netsmith_3d::system::SystemScene>),
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
    seeds: u32,
    models: Option<&crate::models::Places>,
    then: impl FnOnce(&Built, &Board, Vec<String>) -> T,
) -> Result<T, String> {
    let then = std::cell::Cell::new(Some(then));
    picked(
        name,
        text,
        loader,
        catalogs,
        libraries,
        layout,
        board,
        false,
        seeds,
        models,
        |built, board, found| (then.take().expect("called once"))(built, board, found),
        |_, _, _| unreachable!("systems are not asked for"),
    )
}

/// `board` or `system` of what `want` picks: a board, built and placed, or with `systems` the
/// name of a system, each of its members built and placed in the order it declares them (ADR
/// 0110). With `models`, the STEP models of the boards' parts are found on disk first (ADR
/// 0108): a path outside the project of a board it builds is the error, and the warnings of a
/// file that is not there are given to `board`, or to `system` by each member's label.
#[allow(clippy::too_many_arguments)]
fn picked<T>(
    name: &str,
    text: &str,
    loader: &netsmith_check::Loader,
    catalogs: &netsmith_solve::Catalogs,
    libraries: &crate::Libraries,
    layout: Option<&std::path::Path>,
    want: Option<&str>,
    systems: bool,
    seeds: u32,
    models: Option<&crate::models::Places>,
    board: impl FnOnce(&Built, &Board, Vec<String>) -> T,
    system: impl FnOnce(&crate::SolvedSystem, &[(&Built, &Board)], &BTreeMap<String, Vec<String>>) -> T,
) -> Result<T, String> {
    let mut outcome = crate::outcome_with(name, text, loader, catalogs, Some(libraries))
        .map_err(|problems| crate::render_problems(&problems))?;
    // The STEP models the parts are drawn from, found on disk (ADR 0108), by board label.
    let mut found: BTreeMap<String, crate::models::Located> = BTreeMap::new();
    if let Some(places) = models {
        for b in &mut outcome.boards {
            found.insert(b.label(), crate::models::locate(&mut b.design, places));
        }
    }
    let located = |label: &str| -> Result<Vec<String>, String> {
        match found.get(label) {
            Some(l) => match l.errors.first() {
                Some(e) => Err(format!("error: {label}: {e}\n")),
                None => Ok(l.warnings.clone()),
            },
            None => Ok(Vec::new()),
        }
    };
    let names: Vec<String> = outcome
        .boards
        .iter()
        .map(|b| b.design.name.clone())
        .collect();
    let mut labels: Vec<String> = outcome.boards.iter().map(Built::label).collect();
    if systems {
        labels.extend(outcome.systems.iter().map(|s| s.name.clone()));
    }
    let read_layout = |label: &str| -> Result<_, String> {
        match layout {
            Some(file) => crate::layout::read(file, &names)
                .map_err(|what| format!("error: {label}: {what}\n")),
            None => Ok(None),
        }
    };
    let place = |built: &Built, layout: Option<&(netsmith_pcb::Layout, String)>| {
        let fail = |what: String| format!("error: {}: {what}\n", built.label());
        crate::layout::place(
            &built.design,
            layout,
            libraries,
            catalogs.constraints_warn(),
            seeds,
        )
        .map(|placed| placed.board)
        .map_err(|e| fail(e.message()))
    };
    if systems
        && let Some(want) = want
        && let Some(solved) = outcome.systems.iter().find(|s| s.name == want)
    {
        let layout = read_layout(want)?;
        let members = crate::system3d::members(solved, &outcome.boards);
        let mut placed = Vec::new();
        let mut warnings = BTreeMap::new();
        for built in &members {
            warnings.insert(built.label(), located(&built.label())?);
            placed.push(place(built, layout.as_ref())?);
        }
        let pairs: Vec<(&Built, &Board)> = members.iter().copied().zip(&placed).collect();
        return Ok(system(solved, &pairs, &warnings));
    }
    let built = match want {
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
    let warnings = located(&built.label())?;
    let layout = read_layout(&built.label())?;
    let placed = place(built, layout.as_ref())?;
    Ok(board(built, &placed, warnings))
}
