//! The board's _Scene_ (ADR 0105): one three-dimensional description of a placed board, built
//! from netsmith's own data, that `netsmith board3d` reports as numbers and that the editor's 3D
//! view, the `.glb` writer and the rasterizer will draw. [`Scene::of`] is a pure function of the
//! [`Board`] and the parts' _Height provenance_; it reads no file and needs no KiCad.
//!
//! # The frame
//!
//! Millimetres, right-handed, Z up:
//!
//! - the origin is the outline's bottom-left corner (as the layout file and the editor draw the
//!   board, with its top edge up), on the board's **top face**;
//! - x runs right along the outline's bottom edge, y up toward its top edge, z up out of the top
//!   face, so the top face is `z = 0` and the bottom face `z = -thickness`;
//! - a point of the layout file (mm from the outline's top-left corner, y down) is
//!   `(x, outline height - y)` here; a turn is the same number of degrees, counterclockwise
//!   seen from above, as the layout file has it.
//!
//! A part on the top face stands from `z = 0` up to its height; a part on the bottom face hangs
//! from `z = -thickness` down by its height. A face is the outside of its solder mask, which is
//! what a height is measured from (the glossary's _Catalog height_).
//!
//! # What it holds
//!
//! - The board solid: the rounded outline, the cutouts, every drilled hole (pads and vias,
//!   plated or not) and the layers of [`netsmith_pcb::stackup::layers`], the list the KiCad board file's
//!   stackup is written from, each at its Z.
//! - One [`Body`] per part with a body (a label has none): the body box (the footprint's
//!   `F.Fab` drawing, else its courtyard, as a placement requirement and the height check measure
//!   it, ADR 0045) turned and mirrored with the part, from its face to its height. A part whose
//!   height is unknown has none: it is never given zero or a guess (ADR 0094, ADR 0105). A
//!   part with no height of its own whose footprint has a _Standard height_ stands to the
//!   package standard's maximum, labelled `standard` (ADR 0106).
//!   Where the footprint has a package record and the height is known, the body also carries
//!   its _Package shape_ ([`PlacedShape`]): the solids of [`netsmith_pcb::body3d::package_shape`] in the
//!   footprint's frame and the map that puts them where the box is, mirrored and hanging down
//!   for a part on the bottom face. Only the meshes draw it: the height limits, the overlaps and
//!   every report measure the body box.
//! - A part's STEP model, where the design names one and its caller has read it
//!   ([`Scene::draw_models`], ADR 0108): its triangles placed in KiCad's frame for a model
//!   ([`model_frame`]), drawn in place of the package shape. A drawing only, like the shape.
//! - The pins of a through-hole part below the far face, where the design states their
//!   protrusion (ADR 0045 section 15); unknown otherwise.
//! - The height limits ([`HeightLimit`]): every placed keepout with `.max_height`, per face it
//!   limits, with the tallest part under it and the margin.
//! - [`Findings`] that are 3D facts: the parts too tall for a limit (the placement check's own
//!   `TooTall`, not measured again here), the parts of unknown height under a limit (the board's
//!   `heights_unchecked`), and bodies that overlap in 3D (two bodies on one face whose turned
//!   boxes share area in plan and whose heights both start at the face).
//!
//! - The copper ([`SceneCopper`]): per copper layer, at that layer's Z of the stackup, the pads,
//!   the tracks (each a straight piece with round ends), the planes and the pours as polygons
//!   with holes; the vias as barrels from the top copper to the bottom copper; and the
//!   openings of the solder mask and the silkscreen of each face. It is the output of
//!   [`netsmith_pcb::copper`], the copper of the Gerbers, moved into this frame, not computed again. Its ids are the net
//!   names of [`Board::nets`], and a pad also has its part's path and number. A board whose
//!   copper cannot be drawn (its board file would not plot either) has the reason in place of
//!   the copper, and the rest of the scene is as it is.
//!
//! **Rings.** A polygon's outer ring runs counterclockwise seen from above (a positive
//! shoelace sum in x and y up) and each of its holes clockwise. The copper's own rings run the
//! other way in the board's y-down frame; [`Frame::ring`] turns them with the flip.
//!
//! Everything is in a fixed order: bodies by part path, holes by part path then pad order then
//! vias in the board's order, limits by name, findings by path; copper layers from the top
//! face down, a layer's pads by part path then the footprint's pad order, and its tracks,
//! pours and the vias in the order of [`netsmith_pcb::copper`] (the board's).

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::mesh::Mesh;
use crate::mesh::trig::sin_cos_turns;
use crate::step::read::model::Affine;
use crate::step::read::{CHORD_TOLERANCE, ReadModel};
use netsmith_pcb::board::{Board, PlacementProblem, Side};
use netsmith_pcb::body3d::Solid;
use netsmith_pcb::copper::{self, CopperError};
use netsmith_pcb::footprint::{Drill, PadKind};
use netsmith_pcb::geometry::{Pt, Rect};
use netsmith_pcb::stackup::{LayerKind, StackLayer};

/// Where a part's height came from (the glossary's _Height provenance_), strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HeightProvenance {
    /// The source or the physical file states `.height`.
    Stated,
    /// The `height` cell of the catalog row the part was picked as (ADR 0094).
    Catalog,
    /// Neither, and the footprint has a _Standard height_: the greatest its package standard
    /// allows (ADR 0106, [`netsmith_pcb::standard_height`]), not a figure of the part.
    Standard,
    /// None of these: the part has no height.
    Unknown,
}

impl HeightProvenance {
    /// `stated`, `catalog`, `standard` or `unknown`.
    pub fn word(self) -> &'static str {
        match self {
            HeightProvenance::Stated => "stated",
            HeightProvenance::Catalog => "catalog",
            HeightProvenance::Standard => "standard",
            HeightProvenance::Unknown => "unknown",
        }
    }
}

/// What sort of part a body is, for the 3D view's colouring by kind: from the standard generic
/// the part provides (ADR 0068), else from the designator letters its kind states (ADR 0046).
/// A connector is one only by the generic `Connector`, never by its letters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PartCategory {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    Transistor,
    Crystal,
    Fuse,
    Ic,
    Connector,
    Mechanical,
    Other,
}

impl PartCategory {
    /// `resistor`, `capacitor`, `inductor`, `diode`, `transistor`, `crystal`, `fuse`, `ic`,
    /// `connector`, `mechanical` or `other`.
    pub fn word(self) -> &'static str {
        match self {
            PartCategory::Resistor => "resistor",
            PartCategory::Capacitor => "capacitor",
            PartCategory::Inductor => "inductor",
            PartCategory::Diode => "diode",
            PartCategory::Transistor => "transistor",
            PartCategory::Crystal => "crystal",
            PartCategory::Fuse => "fuse",
            PartCategory::Ic => "ic",
            PartCategory::Connector => "connector",
            PartCategory::Mechanical => "mechanical",
            PartCategory::Other => "other",
        }
    }

    /// The category of a part of kind `kind` that provides `provides`, designated `refdes`.
    pub fn of(kind: &str, provides: &[String], refdes: &str) -> PartCategory {
        let is = |g: &str| kind == g || provides.iter().any(|p| p == g);
        let by_generic = [
            ("Connector", PartCategory::Connector),
            ("Resistor", PartCategory::Resistor),
            ("Capacitor", PartCategory::Capacitor),
            ("Inductor", PartCategory::Inductor),
            ("FerriteBead", PartCategory::Inductor),
            ("Led", PartCategory::Diode),
            ("Schottky", PartCategory::Diode),
            ("Zener", PartCategory::Diode),
            ("Tvs", PartCategory::Diode),
            ("TvsBidirectional", PartCategory::Diode),
            ("NMosfet", PartCategory::Transistor),
            ("PMosfet", PartCategory::Transistor),
            ("Crystal", PartCategory::Crystal),
            ("Fuse", PartCategory::Fuse),
            ("LinearRegulator", PartCategory::Ic),
            ("LevelTranslator", PartCategory::Ic),
        ];
        if let Some((_, c)) = by_generic.iter().find(|(g, _)| is(g)) {
            return *c;
        }
        match refdes.trim_end_matches(|c: char| c.is_ascii_digit()) {
            "R" | "RN" => PartCategory::Resistor,
            "C" => PartCategory::Capacitor,
            "L" | "FB" => PartCategory::Inductor,
            "D" | "LED" => PartCategory::Diode,
            "Q" => PartCategory::Transistor,
            "Y" | "X" => PartCategory::Crystal,
            "F" => PartCategory::Fuse,
            "U" | "IC" => PartCategory::Ic,
            "H" | "MH" | "FID" | "TP" => PartCategory::Mechanical,
            _ => PartCategory::Other,
        }
    }
}

/// A point in the plan of the scene: x right, y up, in mm from the outline's bottom-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub x: f64,
    pub y: f64,
}

/// An axis-aligned box in plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanBox {
    pub min: Plan,
    pub max: Plan,
}

/// The board as its stackup makes it.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardSolid {
    pub name: String,
    /// The outline's size, in plan from `(0, 0)` to `(width, height)`.
    pub width: f64,
    pub height: f64,
    pub corner_radius: f64,
    /// A polygon outline's edge as a ring counterclockwise seen from above (issue #57); empty
    /// for the rectangle of `width`, `height` and `corner_radius`.
    pub outline: Vec<Plan>,
    /// The thickness the board chose, which the board house is asked for: the bottom face is at
    /// `-thickness`.
    pub thickness: f64,
    /// The sum of the layers' thicknesses, the process's own figures: it may differ from
    /// `thickness` by the process's rounding (a four-layer board).
    pub stack_thickness: f64,
    /// From the top face down.
    pub layers: Vec<SceneLayer>,
    pub cutouts: Vec<SceneCutout>,
    pub holes: Vec<Hole>,
}

/// A layer of the stackup at its height.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneLayer {
    pub name: String,
    pub kind: LayerKind,
    pub thickness: f64,
    pub z_top: f64,
    pub z_bottom: f64,
}

/// A rectangular hole through the board, with rounded corners.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneCutout {
    pub name: String,
    pub at: PlanBox,
    pub radius: f64,
}

/// What a drilled hole belongs to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum HoleOwner {
    /// A pad of a part: its path, designator and pad number (empty for a mounting hole's).
    Pad {
        path: String,
        refdes: String,
        pad: String,
    },
    /// A via, by its net.
    Via { net: String },
}

/// A drilled hole, through the whole board: a round hole has both ends at its centre, a slot
/// has the two ends of its axis.
#[derive(Debug, Clone, PartialEq)]
pub struct Hole {
    pub owner: HoleOwner,
    pub from: Plan,
    pub to: Plan,
    pub diameter: f64,
    pub plated: bool,
}

/// The pins of a through-hole part where they come out of the far face.
#[derive(Debug, Clone, PartialEq)]
pub enum Pins {
    /// The part has no plated hole.
    None,
    /// The design states no protrusion: how far the pins stand out is unknown, not zero.
    Unknown { at: PlanBox },
    /// They stand out by `length` (the lines in `source` state it), from `z_top` to `z_bottom`.
    Known {
        at: PlanBox,
        length: f64,
        source: String,
        z_top: f64,
        z_bottom: f64,
    },
}

/// A part's body in the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    /// The part's path in the design: the id a layout file, a finding and the scene share.
    pub path: String,
    pub refdes: String,
    pub kind: String,
    /// What sort of part it is, for colouring by kind.
    pub category: PartCategory,
    pub side: Side,
    /// The footprint's origin and turn as the layout file writes them (from the outline's
    /// top-left corner, y down), to find the part in it.
    pub layout_at: (f64, f64),
    pub rotation: f64,
    /// The body box's four corners in plan, turned with the part.
    pub corners: [Plan; 4],
    /// The box that holds them, which the height check measures.
    pub bounds: PlanBox,
    /// Whether the box is the courtyard, because the footprint draws no fabrication outline.
    pub body_is_courtyard: bool,
    /// The face it stands on: `0` for the top face, `-thickness` for the bottom.
    pub z_face: f64,
    /// The far end of the body: above the top face or below the bottom. `None` when the height
    /// is unknown.
    pub z_far: Option<f64>,
    pub height: Option<f64>,
    pub provenance: HeightProvenance,
    /// The package standard the height is from, for a `standard` height (ADR 0106).
    pub standard: Option<netsmith_pcb::standard_height::StandardHeight>,
    pub pins: Pins,
    /// The package shape, drawn in place of the box. `None` when the footprint has no package
    /// record (a footprint read from a file, ADR 0105) or the height is unknown: the box, or the
    /// flat outline, is drawn instead.
    pub shape: Option<PlacedShape>,
    /// Where the footprint's frame is in the scene's.
    pub place: Placement,
    /// The STEP model the part is drawn from in place of its shape (ADR 0108), once
    /// [`Scene::draw_models`] has placed it. A drawing only: the height, the provenance, the
    /// limits and the overlaps never read it.
    pub model: Option<DrawnModel>,
}

/// A part's STEP model as read, for [`Scene::draw_models`] to place: what the caller that reads
/// the file found (`netsmith::models`).
#[derive(Debug, Clone)]
pub struct ModelUse {
    /// The part's path.
    pub path: String,
    /// The file as the design states it (`models/usb4105.step`).
    pub file: String,
    /// The SHA-256 of the file's bytes, in lower-case hex.
    pub sha256: String,
    /// `.model.x`, `.y`, `.z`, in mm.
    pub offset: [f64; 3],
    /// `.model.rx`, `.ry`, `.rz`, in degrees.
    pub rotation: [f64; 3],
    pub read: Arc<ReadModel>,
}

/// A part's STEP model where it is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnModel {
    /// The file as the design states it.
    pub file: String,
    pub sha256: String,
    /// How far the model reaches out of the part's face, in mm: up from the top face, down
    /// from the bottom one.
    pub top: f64,
    /// The map from the model's frame (mm, as the file places its solids) to the scene's:
    /// [`model_frame`] of the part's placement and the model's rotation and offset. A rigid
    /// motion, never a mirror.
    pub to_scene: Affine,
    /// The model's triangles in the scene's frame, one mesh per piece of [`ReadModel::mesh`],
    /// id the part's path, material [`crate::mesh::Material::Body`] and the file's colour where
    /// it has one.
    pub meshes: Vec<Mesh>,
    /// Whether the STEP export can copy the model's solids exactly: every place the file puts
    /// a solid at is a rigid motion. A file that mirrors one is exported with the part's
    /// generated shape.
    pub exact: bool,
    pub read: Arc<ReadModel>,
}

/// A part's _Package shape_ where it is on the board: its solids in the footprint's own frame
/// ([`netsmith_pcb::body3d`]: y down, z up from the face the part is on) and the map to the scene's.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedShape {
    pub solids: Vec<Solid>,
    pub place: Placement,
}

/// The map from a footprint's frame to the scene's: the footprint's `(x, y, z)` is
/// `origin + x * ex + y * ey` in plan and `z_face + out * z` up. `ex` and `ey` are unit vectors
/// that hold the part's turn, the flip of y and the mirror of a bottom part; `out` is 1 for a
/// part on the top face and -1 for one on the bottom, which hangs down from it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub origin: Plan,
    pub ex: Plan,
    pub ey: Plan,
    pub z_face: f64,
    pub out: f64,
}

impl Placement {
    /// A point of the footprint's frame in the scene's.
    pub fn point(&self, p: [f64; 3]) -> [f64; 3] {
        [
            self.origin.x + p[0] * self.ex.x + p[1] * self.ey.x,
            self.origin.y + p[0] * self.ex.y + p[1] * self.ey.y,
            self.z_face + self.out * p[2],
        ]
    }

    /// The linear part of the map, by rows: what a direction (a normal) is turned by.
    pub fn linear(&self) -> [[f64; 3]; 3] {
        [
            [self.ex.x, self.ey.x, 0.0],
            [self.ex.y, self.ey.y, 0.0],
            [0.0, 0.0, self.out],
        ]
    }
}

impl Body {
    /// The lowest and the highest Z of the body, each `None` where the unknown height leaves it.
    pub fn z_range(&self) -> (Option<f64>, Option<f64>) {
        match self.side {
            Side::Back => (self.z_far, Some(self.z_face)),
            _ => (Some(self.z_face), self.z_far),
        }
    }
}

/// The tallest part under a limit, or on a face.
#[derive(Debug, Clone, PartialEq)]
pub struct Tallest {
    pub path: String,
    pub refdes: String,
    pub height: f64,
}

/// One face that a height limit limits.
#[derive(Debug, Clone, PartialEq)]
pub struct LimitFace {
    pub side: Side,
    /// The volume parts may fill: from the face out to the limit.
    pub z_bottom: f64,
    pub z_top: f64,
    /// The tallest part of known height whose body is under the limit on this face.
    pub tallest: Option<Tallest>,
    /// The limit less that part's height: negative when it is too tall.
    pub margin: Option<f64>,
    /// The paths of the parts under it on this face whose height is unknown.
    pub unknown: Vec<String>,
    /// The paths of every part under it on this face, known height or not, in the order of the
    /// scene's bodies.
    pub under: Vec<String>,
}

/// A keepout with a height limit, as a volume over each face it limits.
#[derive(Debug, Clone, PartialEq)]
pub struct HeightLimit {
    pub name: String,
    /// The path of the part that carries it, if one does (its zone is not measured against
    /// that part).
    pub carrier: Option<String>,
    pub at: PlanBox,
    pub max_height: f64,
    /// Top first. Empty for a limit on inner layers only, which limits no face.
    pub faces: Vec<LimitFace>,
}

/// A part taller than a limit it stands under: the placement check's `TooTall`.
#[derive(Debug, Clone, PartialEq)]
pub struct TooTall {
    pub path: String,
    pub refdes: String,
    pub keepout: String,
    pub height: f64,
    pub limit: f64,
    pub side: Side,
}

/// A part of unknown height under a limit: the board's `heights_unchecked`.
#[derive(Debug, Clone, PartialEq)]
pub struct Unchecked {
    pub path: String,
    pub refdes: String,
    pub keepout: String,
    pub limit: f64,
    pub side: Side,
}

/// Two bodies on one face that fill the same space.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlap {
    /// The two paths, the smaller first.
    pub a: String,
    pub b: String,
    pub a_refdes: String,
    pub b_refdes: String,
    pub side: Side,
    /// The Z range both fill: from the face out to the shorter body's height.
    pub z_bottom: f64,
    pub z_top: f64,
}

/// What the scene shows that is wrong or unknown in 3D.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Findings {
    pub too_tall: Vec<TooTall>,
    pub unchecked: Vec<Unchecked>,
    pub overlaps: Vec<Overlap>,
}

/// How many bodies have a known height (the glossary's _Height coverage_).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coverage {
    /// Every known height: stated, catalog and standard.
    pub known: usize,
    pub total: usize,
    pub stated: usize,
    pub catalog: usize,
    /// The heights that are a package standard's, counted as known and named apart (ADR 0106).
    pub standard: usize,
}

/// A polygon with holes in the plan of the scene. `outer` runs counterclockwise and each hole
/// clockwise, seen from above.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanArea {
    pub outer: Vec<Plan>,
    pub holes: Vec<Vec<Plan>>,
}

impl PlanArea {
    /// The area in mm², the holes taken away.
    pub fn size(&self) -> f64 {
        let ring = |r: &[Plan]| {
            let sum: f64 = (0..r.len())
                .map(|i| {
                    let (a, b) = (r[i], r[(i + 1) % r.len()]);
                    a.x * b.y - b.x * a.y
                })
                .sum();
            sum.abs() / 2.0
        };
        ring(&self.outer) - self.holes.iter().map(|h| ring(h)).sum::<f64>()
    }
}

/// The copper of the board in the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneCopper {
    /// From the top face down: the front copper, the inner layers, the back copper.
    pub layers: Vec<CopperLayer>,
    /// In the board's order.
    pub vias: Vec<Via>,
    /// The front's mask openings, then the back's.
    pub mask: Vec<MaskFace>,
    /// The front's silkscreen, then the back's.
    pub silk: Vec<SilkFace>,
}

/// The copper of one layer, between that layer's `z_top` and `z_bottom` of the stackup.
#[derive(Debug, Clone, PartialEq)]
pub struct CopperLayer {
    /// `F.Cu`, `In1.Cu`, ..., `B.Cu`.
    pub name: String,
    pub thickness: f64,
    pub z_top: f64,
    pub z_bottom: f64,
    pub pads: Vec<CopperPad>,
    pub tracks: Vec<CopperTrack>,
    /// The pours (outer layers) and planes (inner layers) as filled for the Gerbers, with the
    /// clearances round other nets' copper cut out of them.
    pub pours: Vec<CopperPour>,
    /// What footprints draw on the layer that is not a pad (no net).
    pub drawings: Vec<PlanArea>,
}

impl CopperLayer {
    /// How much copper the net has on the layer in mm²: its pads, tracks and pours added up,
    /// each polygon by itself (a pad on a pour of its net is in both) and with no drill taken
    /// away. A via's ring is not in it. `None` is the copper with no net.
    pub fn area_of(&self, net: Option<&str>) -> f64 {
        let size = |areas: &[PlanArea]| areas.iter().map(PlanArea::size).sum::<f64>();
        let pads: f64 = self
            .pads
            .iter()
            .filter(|p| p.net.as_deref() == net)
            .map(|p| size(&p.areas))
            .sum();
        let tracks: f64 = self
            .tracks
            .iter()
            .filter(|t| t.net.as_deref() == net)
            .map(|t| t.area.size())
            .sum();
        let pours: f64 = self
            .pours
            .iter()
            .filter(|p| p.net.as_deref() == net)
            .map(|p| p.area.size())
            .sum();
        pads + tracks + pours
    }

    /// The nets that have copper on the layer, in order, the copper with no net first.
    pub fn nets(&self) -> Vec<Option<&str>> {
        let mut nets: Vec<Option<&str>> = self
            .pads
            .iter()
            .map(|p| p.net.as_deref())
            .chain(self.tracks.iter().map(|t| t.net.as_deref()))
            .chain(self.pours.iter().map(|p| p.net.as_deref()))
            .collect();
        nets.sort();
        nets.dedup();
        nets
    }
}

/// A pad's copper on a layer.
#[derive(Debug, Clone, PartialEq)]
pub struct CopperPad {
    /// The part's path, designator and the pad's number: the pad's id.
    pub part: String,
    pub refdes: String,
    pub number: String,
    pub net: Option<String>,
    pub kind: PadKind,
    /// The pad's centre.
    pub at: Plan,
    /// The pad's turn, in degrees, counterclockwise seen from above, as the board file has it.
    pub angle: f64,
    /// The pad's size before its turn.
    pub size: (f64, f64),
    pub drill: Option<Drill>,
    /// The copper's shape: one polygon for most pads.
    pub areas: Vec<PlanArea>,
}

/// A straight piece of track.
#[derive(Debug, Clone, PartialEq)]
pub struct CopperTrack {
    pub net: Option<String>,
    pub from: Plan,
    pub to: Plan,
    pub width: f64,
    /// The copper: the piece with round ends.
    pub area: PlanArea,
}

/// The pour or plane of one net: one piece of its copper.
#[derive(Debug, Clone, PartialEq)]
pub struct CopperPour {
    pub net: Option<String>,
    pub area: PlanArea,
}

/// A via: a plated barrel through the board, with a ring of copper on every copper layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Via {
    pub net: Option<String>,
    pub at: Plan,
    /// The ring's outer diameter.
    pub diameter: f64,
    pub drill: f64,
    /// The barrel runs from the top of the front copper to the bottom of the back copper.
    pub z_top: f64,
    pub z_bottom: f64,
}

/// Where the solder mask of a face is open.
#[derive(Debug, Clone, PartialEq)]
pub struct MaskFace {
    pub side: Side,
    /// `F.Mask` or `B.Mask`, with the layer's own Z.
    pub layer: String,
    pub z_top: f64,
    pub z_bottom: f64,
    pub areas: Vec<PlanArea>,
}

/// The silkscreen of a face: the ink of its legend Gerber ([`netsmith_pcb::copper::SilkInk`]), as
/// printed on the outside of the face's mask (`z`, the mask layer's outer side of the stackup;
/// the outer copper's where the stackup has no mask).
#[derive(Debug, Clone, PartialEq)]
pub struct SilkFace {
    pub side: Side,
    /// `F.SilkS` or `B.SilkS`.
    pub layer: String,
    pub z: f64,
    pub areas: Vec<PlanArea>,
}

/// The scene of one placed board.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub board: BoardSolid,
    /// The copper, or why it cannot be drawn.
    pub copper: Result<SceneCopper, CopperError>,
    /// By path.
    pub bodies: Vec<Body>,
    /// By name.
    pub limits: Vec<HeightLimit>,
    pub findings: Findings,
    /// What drawing the parts' STEP models found ([`Scene::draw_models`]): a model whose top is
    /// above its part's height, a model the STEP export cannot copy. Warnings, for the command
    /// that built the scene to print. Empty from [`Scene::of`].
    pub warnings: Vec<String>,
    /// The board's case models and their fit (ADR 0109, [`crate::case`]): empty unless the
    /// caller that reads the files adds them ([`Scene::with_case`]).
    pub case: crate::case::CaseScene,
}

/// How far two boxes must share before they count as overlapping, in mm: what the placement
/// check's `Rect::overlaps` allows.
const TOLERANCE: f64 = 1e-6;

impl Scene {
    /// The scene of `board`. `provenance` says where each part's height came from, by path
    /// (`netsmith::heights::provenance`); a part it does not name is `stated` if it has a height
    /// and `unknown` if not. A part with no height is `unknown` whatever it says.
    pub fn of(board: &Board, provenance: &BTreeMap<String, HeightProvenance>) -> Scene {
        let frame = Frame::of(board);
        let thickness = board
            .process
            .stackup
            .chosen_thickness()
            .expect("a board is only made once its thickness is chosen");
        let solid = solid(board, &frame, thickness);
        let copper = copper::copper(board).and_then(|c| scene_copper(&c, &solid.layers, &frame));
        let mut parts: Vec<&netsmith_pcb::board::Placed> =
            board.parts.iter().filter(|p| p.kind != "Label").collect();
        parts.sort_by(|a, b| a.path.cmp(&b.path));
        let bodies: Vec<Body> = parts
            .iter()
            .map(|p| body(p, &frame, thickness, provenance))
            .collect();
        let limits = limits(board, &frame, thickness, &bodies);
        let findings = findings(board, &bodies);
        Scene {
            board: solid,
            copper,
            bodies,
            limits,
            findings,
            warnings: Vec::new(),
            case: crate::case::CaseScene::default(),
        }
    }

    /// Draws each part of `uses` from its STEP model, in place of its generated shape (ADR
    /// 0108): the model's triangles are put at the footprint's origin, turned by the model's
    /// rotation and moved by its offset ([`model_frame`]), and turned and mirrored with the
    /// part. Nothing that is measured changes. A model whose top is above the part's known
    /// height by more than the reader's chord tolerance is a warning naming both figures; a use
    /// whose part has no body (a label) is left out.
    pub fn draw_models(&mut self, uses: &[ModelUse]) {
        for u in uses {
            let Some(body) = self.bodies.iter_mut().find(|b| b.path == u.path) else {
                continue;
            };
            let to_scene = model_frame(&body.place, u.offset, u.rotation);
            let mut meshes = Vec::new();
            let mut top = f64::MIN;
            for piece in &u.read.mesh.pieces {
                let mut m = piece.mesh.clone();
                m.transform(to_scene.m, to_scene.t);
                for p in &mut m.positions {
                    *p = p.map(clean);
                    top = top.max(body.place.out * (p[2] - body.z_face));
                }
                m.id = body.path.clone();
                m.colour = piece.colour;
                meshes.push(m);
            }
            let top = if top == f64::MIN { 0.0 } else { clean(top) };
            if let Some(h) = body.height
                && top > h + CHORD_TOLERANCE
            {
                // Rounded up to the hundredth, so that the height it suggests holds the model.
                let suggest = (top * 100.0 - 1e-6).ceil() / 100.0;
                self.warnings.push(format!(
                    "the model of `{}` (`{}`) stands {top:.3} mm out of its face, above the part's height of {h:.3} mm ({}); the model is drawn and never gives a height, so if the model is right state `.{}.height &= {suggest:.2}mm`",
                    u.path,
                    u.file,
                    body.provenance.word(),
                    u.path,
                ));
            }
            let exact = u.read.model.placed.iter().all(|p| rigid(&p.place.m));
            if !exact {
                self.warnings.push(format!(
                    "the model of `{}` (`{}`) places a solid in a mirror, which a STEP assembly cannot hold, so the STEP export draws `{}` with its generated shape",
                    u.path, u.file, u.path
                ));
            }
            body.model = Some(DrawnModel {
                file: u.file.clone(),
                sha256: u.sha256.clone(),
                top,
                to_scene,
                meshes,
                exact,
                read: u.read.clone(),
            });
        }
    }

    /// The scene with its case models placed and checked (ADR 0109): `models` are drawn
    /// see-through, and the scene's case findings are `unread` (what the caller found of the
    /// files it could not read) and then the fit check's.
    pub fn with_case(
        mut self,
        models: Vec<crate::case::PlacedCase>,
        unread: Vec<crate::case::FitFinding>,
    ) -> Scene {
        let mut findings = unread;
        findings.extend(crate::case::check(&self.board, &self.bodies, &models));
        self.case = crate::case::CaseScene { models, findings };
        self
    }

    pub fn body(&self, path: &str) -> Option<&Body> {
        self.bodies.iter().find(|b| b.path == path)
    }

    /// How many bodies have a known height, and where it came from.
    pub fn coverage(&self) -> Coverage {
        let count = |p| self.bodies.iter().filter(|b| b.provenance == p).count();
        let (stated, catalog, standard) = (
            count(HeightProvenance::Stated),
            count(HeightProvenance::Catalog),
            count(HeightProvenance::Standard),
        );
        Coverage {
            known: stated + catalog + standard,
            total: self.bodies.len(),
            stated,
            catalog,
            standard,
        }
    }

    /// The tallest part of known height on a face, the first by path among equals.
    pub fn tallest(&self, side: Side) -> Option<Tallest> {
        tallest_of(self.bodies.iter().filter(|b| b.side == side))
    }
}

/// The centre of each pad of the part at `path` on `board`, in the plan of the board's scene,
/// with its number, in the footprint's order: what the system scene lines two mated connectors
/// up by (ADR 0110). Empty when the board has no part at `path`.
pub fn pad_centres(board: &Board, path: &str) -> Vec<(String, Plan)> {
    let frame = Frame::of(board);
    board
        .parts
        .iter()
        .find(|p| p.path == path)
        .map(|p| {
            p.footprint
                .pads
                .iter()
                .map(|pad| (pad.number.clone(), frame.point(p.to_board(pad.at))))
                .collect()
        })
        .unwrap_or_default()
}

fn tallest_of<'a>(bodies: impl Iterator<Item = &'a Body>) -> Option<Tallest> {
    let mut best: Option<&Body> = None;
    for b in bodies {
        if let Some(h) = b.height
            && best.is_none_or(|t| h > t.height.unwrap_or(f64::MIN))
        {
            best = Some(b);
        }
    }
    best.map(|b| Tallest {
        path: b.path.clone(),
        refdes: b.refdes.clone(),
        height: b.height.unwrap_or_default(),
    })
}

/// The board's sheet frame (y down, origin at the sheet's corner) to the scene's plan.
struct Frame {
    left: f64,
    top: f64,
    bottom: f64,
}

impl Frame {
    fn of(board: &Board) -> Frame {
        Frame {
            left: board.outline.min.x,
            top: board.outline.min.y,
            bottom: board.outline.max.y,
        }
    }

    fn point(&self, p: Pt) -> Plan {
        Plan {
            x: clean(p.x - self.left),
            y: clean(self.bottom - p.y),
        }
    }

    /// A ring of the board's frame in the scene's: y flips, so the ring is turned round to keep
    /// the outer rings counterclockwise and the holes clockwise seen from above.
    fn ring(&self, ring: &[Pt]) -> Vec<Plan> {
        ring.iter().rev().map(|p| self.point(*p)).collect()
    }

    fn area(&self, a: &copper::Area) -> PlanArea {
        PlanArea {
            outer: self.ring(&a.outer),
            holes: a.holes.iter().map(|h| self.ring(h)).collect(),
        }
    }

    fn areas(&self, areas: &[copper::Area]) -> Vec<PlanArea> {
        areas.iter().map(|a| self.area(a)).collect()
    }

    fn rect(&self, r: Rect) -> PlanBox {
        let (a, b) = (self.point(r.min), self.point(r.max));
        PlanBox {
            min: Plan {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
            },
            max: Plan {
                x: a.x.max(b.x),
                y: a.y.max(b.y),
            },
        }
    }
}

/// A length to a nanometre, with no negative zero, so a report prints the same bytes on every
/// machine.
fn clean(v: f64) -> f64 {
    let r = netsmith_pcb::stackup::round(v);
    if r == 0.0 { 0.0 } else { r }
}

fn solid(board: &Board, frame: &Frame, thickness: f64) -> BoardSolid {
    let stack: Vec<StackLayer> = netsmith_pcb::stackup::layers(board);
    let mut z = 0.0;
    let layers = stack
        .into_iter()
        .map(|l| {
            let top = z;
            z = clean(z - l.thickness);
            SceneLayer {
                name: l.name,
                kind: l.kind,
                thickness: l.thickness,
                z_top: top,
                z_bottom: z,
            }
        })
        .collect();
    let cutouts = board
        .cutout_shapes()
        .into_iter()
        .map(|(c, shape)| SceneCutout {
            name: c.name.clone(),
            at: frame.rect(shape.rect),
            radius: c.radius,
        })
        .collect::<Vec<_>>();
    let mut parts: Vec<&netsmith_pcb::board::Placed> = board.parts.iter().collect();
    parts.sort_by(|a, b| a.path.cmp(&b.path));
    let mut holes = Vec::new();
    for p in parts {
        for pad in &p.footprint.pads {
            let Some((from, to, radius)) = p.hole(pad) else {
                continue;
            };
            holes.push(Hole {
                owner: HoleOwner::Pad {
                    path: p.path.clone(),
                    refdes: p.refdes.clone(),
                    pad: pad.number.clone(),
                },
                from: frame.point(from),
                to: frame.point(to),
                diameter: clean(2.0 * radius),
                plated: pad.kind == PadKind::ThruHole,
            });
        }
    }
    holes.extend(board.vias.iter().map(|v| Hole {
        owner: HoleOwner::Via { net: v.net.clone() },
        from: frame.point(v.at),
        to: frame.point(v.at),
        diameter: v.drill,
        plated: true,
    }));
    BoardSolid {
        name: board.name.clone(),
        width: clean(board.outline.width()),
        height: clean(board.outline.height()),
        corner_radius: board.corner_radius,
        // Clockwise on screen with y down is counterclockwise from above with y up.
        outline: board
            .polygon
            .as_ref()
            .map(|p| p.segments().iter().map(|s| frame.point(s.0)).collect())
            .unwrap_or_default(),
        thickness,
        stack_thickness: clean(-z),
        layers,
        cutouts,
        holes,
    }
}

/// The copper of `c` in the scene: each layer at its Z of the stackup `layers`.
fn scene_copper(
    c: &copper::Copper,
    layers: &[SceneLayer],
    frame: &Frame,
) -> Result<SceneCopper, CopperError> {
    let z_of = |name: &str| {
        layers
            .iter()
            .find(|l| l.name == name)
            .ok_or_else(|| CopperError(format!("the stackup has no layer `{name}`")))
    };
    let mut out = Vec::new();
    for l in &c.layers {
        let z = z_of(&l.layer)?;
        let mut pads: Vec<CopperPad> = l
            .pads
            .iter()
            .map(|p| CopperPad {
                part: p.part.clone(),
                refdes: p.refdes.clone(),
                number: p.number.clone(),
                net: p.net.clone(),
                kind: p.kind,
                at: frame.point(p.at),
                angle: p.angle,
                size: p.size,
                drill: p.drill,
                areas: frame.areas(&p.areas),
            })
            .collect();
        pads.sort_by(|a, b| a.part.cmp(&b.part));
        let tracks = l
            .tracks
            .iter()
            .map(|t| CopperTrack {
                net: t.net.clone(),
                from: frame.point(t.from),
                to: frame.point(t.to),
                width: t.width,
                area: frame.area(&t.outline()),
            })
            .collect();
        out.push(CopperLayer {
            name: l.layer.clone(),
            thickness: z.thickness,
            z_top: z.z_top,
            z_bottom: z.z_bottom,
            pads,
            tracks,
            pours: l
                .pours
                .iter()
                // The fills the file holds (teardrops) are copper of their net, as a pour is.
                .chain(&l.fills)
                .map(|p| CopperPour {
                    net: p.net.clone(),
                    area: frame.area(&p.area),
                })
                .collect(),
            drawings: frame.areas(&l.drawings),
        });
    }
    let (top, bottom) = match (out.first(), out.last()) {
        (Some(t), Some(b)) => (t.z_top, b.z_bottom),
        _ => (0.0, 0.0),
    };
    let vias = c
        .vias
        .iter()
        .map(|v| Via {
            net: v.net.clone(),
            at: frame.point(v.at),
            diameter: v.diameter,
            drill: v.drill,
            z_top: top,
            z_bottom: bottom,
        })
        .collect();
    let mut mask = Vec::new();
    for m in &c.mask {
        let z = z_of(&m.layer)?;
        mask.push(MaskFace {
            side: if m.layer == "B.Mask" {
                Side::Back
            } else {
                Side::Front
            },
            layer: m.layer.clone(),
            z_top: z.z_top,
            z_bottom: z.z_bottom,
            areas: frame.areas(&m.areas),
        });
    }
    let mut silk = Vec::new();
    for ink in &c.silk {
        let side = if ink.layer == "B.SilkS" {
            Side::Back
        } else {
            Side::Front
        };
        let (mask, copper) = if side == Side::Back {
            ("B.Mask", out.last())
        } else {
            ("F.Mask", out.first())
        };
        let z = match (layers.iter().find(|l| l.name == mask), copper) {
            (Some(l), _) if side == Side::Back => l.z_bottom,
            (Some(l), _) => l.z_top,
            (None, Some(c)) if side == Side::Back => c.z_bottom,
            (None, Some(c)) => c.z_top,
            (None, None) => 0.0,
        };
        silk.push(SilkFace {
            side,
            layer: ink.layer.clone(),
            z,
            areas: frame.areas(&ink.areas),
        });
    }
    Ok(SceneCopper {
        layers: out,
        vias,
        mask,
        silk,
    })
}

fn body(
    p: &netsmith_pcb::board::Placed,
    frame: &Frame,
    thickness: f64,
    provenance: &BTreeMap<String, HeightProvenance>,
) -> Body {
    let local = p.footprint.body;
    let corners = [
        local.min,
        Pt::new(local.max.x, local.min.y),
        local.max,
        Pt::new(local.min.x, local.max.y),
    ]
    .map(|c| frame.point(p.to_board(c)));
    // The part's own height wins; a standard height only stands in where there is none (ADR
    // 0106).
    let standard = p.standard_height().copied();
    let height = p.height.or(standard.map(|s| s.max));
    let provenance = match (p.height, standard) {
        (None, None) => HeightProvenance::Unknown,
        (None, Some(_)) => HeightProvenance::Standard,
        (Some(_), _) => match provenance.get(&p.path) {
            Some(HeightProvenance::Catalog) => HeightProvenance::Catalog,
            _ => HeightProvenance::Stated,
        },
    };
    let (z_face, out) = match p.side {
        Side::Back => (-thickness, -1.0),
        _ => (0.0, 1.0),
    };
    let plated: Vec<Rect> = p
        .footprint
        .pads
        .iter()
        .filter(|pad| pad.kind == PadKind::ThruHole)
        .map(|pad| p.pad_bounds(pad))
        .collect();
    let pins = match plated.iter().copied().reduce(|a, b| a.union(&b)) {
        None => Pins::None,
        Some(r) => {
            let at = frame.rect(r);
            match &p.protrusion {
                None => Pins::Unknown { at },
                Some(t) => {
                    // The pins come out of the face opposite the part.
                    let (near, toward) = match p.side {
                        Side::Back => (0.0, 1.0),
                        _ => (-thickness, -1.0),
                    };
                    let far = clean(near + toward * t.length);
                    Pins::Known {
                        at,
                        length: t.length,
                        source: t.source.clone(),
                        z_top: near.max(far),
                        z_bottom: near.min(far),
                    }
                }
            }
        }
    };
    let place = placement(p, frame, z_face, out);
    let shape = (p.footprint.package.is_some() && height.is_some())
        .then(|| netsmith_pcb::body3d::package_shape(&p.footprint, height))
        .filter(|s| s.shaped && !s.solids.is_empty())
        .map(|s| PlacedShape {
            solids: s.solids,
            place,
        });
    Body {
        path: p.path.clone(),
        refdes: p.refdes.clone(),
        kind: p.kind.clone(),
        category: PartCategory::of(&p.kind, &p.provides, &p.refdes),
        side: p.side,
        layout_at: (clean(p.at.x - frame.left), clean(p.at.y - frame.top)),
        rotation: p.rotation,
        corners,
        bounds: frame.rect(p.body()),
        body_is_courtyard: p.footprint.body_is_courtyard,
        z_face,
        z_far: height.map(|h| clean(z_face + out * h)),
        height,
        provenance,
        standard,
        pins,
        shape,
        place,
        model: None,
    }
}

/// The map from a STEP model's frame to the scene's, for a part placed by `place` (ADR 0108).
///
/// The model's frame is KiCad's for a footprint's model, so that the numbers of a
/// `(model ... (offset (xyz x y z)) (rotate (xyz rx ry rz)))` line can be copied into
/// `.model.x` ... `.model.rz`: x along the footprint's x, **y up the footprint as the
/// footprint editor shows it** (the footprint's own y runs down, so a model's y is the
/// footprint's -y), z out of the part's face. A point `p` of the model is turned by
/// `Rz(-rz) * Ry(-ry) * Rx(-rx)` (about x first; each angle turns clockwise seen from the
/// axis's positive end, as KiCad turns a model), then moved by the offset, then put on the
/// board as the footprint is: turned with the part, and for a part on the bottom face mirrored
/// and hanging down. KiCad's 3D viewer and STEP exporter do the same (`render_3d_opengl.cpp`,
/// `step_pcb_model.cpp`: "aOrientation is applied -Z*-Y*-X, b. aOffset is applied", the y of
/// the footprint's position negated and the offset's not). The map is a rigid motion on
/// both faces: the y flip and the bottom face's mirror and its z flip cancel out.
pub fn model_frame(place: &Placement, offset: [f64; 3], rotation: [f64; 3]) -> Affine {
    let turn = |deg: f64| sin_cos_turns(-deg / 360.0);
    let (sx, cx) = turn(rotation[0]);
    let (sy, cy) = turn(rotation[1]);
    let (sz, cz) = turn(rotation[2]);
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    let r = mul(rz, mul(ry, rx));
    // The footprint's frame of the model's: y flipped.
    let flip = [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]];
    let lf = mul(place.linear(), flip);
    let o = [0, 1, 2].map(|k| (0..3).map(|c| lf[k][c] * offset[c]).sum::<f64>());
    Affine {
        m: mul(lf, r),
        t: [
            place.origin.x + o[0],
            place.origin.y + o[1],
            place.z_face + o[2],
        ],
    }
}

fn mul(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    [0, 1, 2].map(|r| [0, 1, 2].map(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}

/// Whether `m` is a rotation: its columns of unit length and at right angles, and no mirror.
fn rigid(m: &[[f64; 3]; 3]) -> bool {
    let col = |c: usize| [m[0][c], m[1][c], m[2][c]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (x, y, z) = (col(0), col(1), col(2));
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let near = |v: f64, want: f64| (v - want).abs() < 1e-9;
    near(dot(x, x), 1.0)
        && near(dot(y, y), 1.0)
        && near(dot(z, z), 1.0)
        && near(dot(x, y), 0.0)
        && near(dot(y, z), 0.0)
        && near(dot(x, z), 0.0)
        && det > 0.0
}

/// Where the footprint's frame of `p` is in the scene: its origin, and its axes as
/// [`netsmith_pcb::board::Placed::to_board`] turns and mirrors them, with the sheet's y flipped.
fn placement(p: &netsmith_pcb::board::Placed, frame: &Frame, z_face: f64, out: f64) -> Placement {
    // `to_board` less the move, without the rounding of adding and taking away `p.at`.
    let axis = |local: Pt| {
        let local = if p.mirrored() {
            Pt::new(-local.x, local.y)
        } else {
            local
        };
        let d = local.rotate(p.rotation);
        Plan { x: d.x, y: -d.y }
    };
    Placement {
        origin: frame.point(p.at),
        ex: axis(Pt::new(1.0, 0.0)),
        ey: axis(Pt::new(0.0, 1.0)),
        z_face,
        out,
    }
}

fn limits(board: &Board, frame: &Frame, thickness: f64, bodies: &[Body]) -> Vec<HeightLimit> {
    let parts: BTreeMap<&str, &netsmith_pcb::board::Placed> =
        board.parts.iter().map(|p| (p.path.as_str(), p)).collect();
    let mut limits: Vec<HeightLimit> = board
        .keepout_areas()
        .into_iter()
        .filter_map(|k| {
            let max_height = k.max_height?;
            let faces = [Side::Front, Side::Back]
                .into_iter()
                .filter(|side| k.limits_height_on(*side))
                .map(|side| {
                    // The parts under it on this face, as the height check finds them.
                    let under: Vec<&Body> = bodies
                        .iter()
                        .filter(|b| b.side == side && k.carrier.as_deref() != Some(&b.path))
                        .filter(|b| parts[b.path.as_str()].body().overlaps(&k.rect))
                        .collect();
                    let tallest = tallest_of(under.iter().copied());
                    let (z_bottom, z_top) = match side {
                        Side::Back => (clean(-thickness - max_height), -thickness),
                        _ => (0.0, max_height),
                    };
                    LimitFace {
                        side,
                        z_bottom,
                        z_top,
                        margin: tallest.as_ref().map(|t| clean(max_height - t.height)),
                        tallest,
                        unknown: under
                            .iter()
                            .filter(|b| b.height.is_none())
                            .map(|b| b.path.clone())
                            .collect(),
                        under: under.iter().map(|b| b.path.clone()).collect(),
                    }
                })
                .collect();
            Some(HeightLimit {
                name: k.name,
                carrier: k.carrier,
                at: frame.rect(k.rect),
                max_height,
                faces,
            })
        })
        .collect();
    limits.sort_by(|a, b| a.name.cmp(&b.name));
    limits
}

fn findings(board: &Board, bodies: &[Body]) -> Findings {
    let mut too_tall: Vec<TooTall> = board
        .check_placement()
        .into_iter()
        .filter_map(|p| match p {
            PlacementProblem::TooTall {
                refdes,
                path,
                keepout,
                height,
                limit,
                side,
                ..
            } => Some(TooTall {
                path,
                refdes,
                keepout,
                height,
                limit,
                side,
            }),
            _ => None,
        })
        .collect();
    too_tall.sort_by(|a, b| (&a.path, &a.keepout).cmp(&(&b.path, &b.keepout)));
    let mut unchecked: Vec<Unchecked> = board
        .heights_unchecked()
        .into_iter()
        .map(|u| Unchecked {
            path: u.path,
            refdes: u.refdes,
            keepout: u.keepout,
            limit: u.limit,
            side: u.side,
        })
        .collect();
    unchecked.sort_by(|a, b| (&a.path, &a.keepout).cmp(&(&b.path, &b.keepout)));
    Findings {
        too_tall,
        unchecked,
        overlaps: overlaps(bodies),
    }
}

/// The pairs of bodies on one face, both of known height above zero, whose turned boxes share
/// area in plan. Both stand from the face, so they share the space up to the shorter one.
fn overlaps(bodies: &[Body]) -> Vec<Overlap> {
    let mut found = Vec::new();
    for (i, a) in bodies.iter().enumerate() {
        for b in &bodies[i + 1..] {
            let (Some(ha), Some(hb)) = (a.height, b.height) else {
                continue;
            };
            if a.side != b.side || ha <= TOLERANCE || hb <= TOLERANCE {
                continue;
            }
            if !boxes_overlap(&a.bounds, &b.bounds) || !turned_overlap(&a.corners, &b.corners) {
                continue;
            }
            let reach = ha.min(hb);
            let (z_bottom, z_top) = match a.side {
                Side::Back => (clean(a.z_face - reach), a.z_face),
                _ => (a.z_face, clean(a.z_face + reach)),
            };
            found.push(Overlap {
                a: a.path.clone(),
                b: b.path.clone(),
                a_refdes: a.refdes.clone(),
                b_refdes: b.refdes.clone(),
                side: a.side,
                z_bottom,
                z_top,
            });
        }
    }
    found
}

fn boxes_overlap(a: &PlanBox, b: &PlanBox) -> bool {
    a.min.x < b.max.x - TOLERANCE
        && b.min.x < a.max.x - TOLERANCE
        && a.min.y < b.max.y - TOLERANCE
        && b.min.y < a.max.y - TOLERANCE
}

/// Whether two convex quadrilaterals share area: no edge of either separates them by more than
/// the tolerance (the separating axis test).
fn turned_overlap(a: &[Plan; 4], b: &[Plan; 4]) -> bool {
    let separated_along = |poly: &[Plan; 4]| {
        (0..4).any(|i| {
            let (p, q) = (poly[i], poly[(i + 1) % 4]);
            let (nx, ny) = (q.y - p.y, p.x - q.x);
            let len = (nx * nx + ny * ny).sqrt();
            if len == 0.0 {
                return false;
            }
            let project = |s: &[Plan; 4]| {
                s.iter()
                    .map(|c| (c.x * nx + c.y * ny) / len)
                    .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
            };
            let ((alo, ahi), (blo, bhi)) = (project(a), project(b));
            ahi <= blo + TOLERANCE || bhi <= alo + TOLERANCE
        })
    };
    !separated_along(a) && !separated_along(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, size: f64) -> [Plan; 4] {
        [
            Plan { x, y },
            Plan { x: x + size, y },
            Plan {
                x: x + size,
                y: y + size,
            },
            Plan { x, y: y + size },
        ]
    }

    #[test]
    fn boxes_that_only_touch_do_not_overlap_and_boxes_that_share_area_do() {
        assert!(!turned_overlap(
            &square(0.0, 0.0, 1.0),
            &square(1.0, 0.0, 1.0)
        ));
        assert!(turned_overlap(
            &square(0.0, 0.0, 1.0),
            &square(0.5, 0.5, 1.0)
        ));
    }

    #[test]
    fn a_diamond_beside_a_square_corner_is_apart_though_their_bounds_meet() {
        // A square turned 45 degrees round (2, 2), reaching to (2 - 0.71, 2): its bounds overlap
        // the unit square at the origin's, but its edge does not reach the square's corner.
        let r = std::f64::consts::FRAC_1_SQRT_2;
        let diamond = [
            Plan { x: 2.0 - r, y: 2.0 },
            Plan { x: 2.0, y: 2.0 - r },
            Plan { x: 2.0 + r, y: 2.0 },
            Plan { x: 2.0, y: 2.0 + r },
        ];
        let square = square(0.0, 0.0, 1.5);
        assert!(!turned_overlap(&square, &diamond));
    }

    #[test]
    fn a_length_is_cleaned_of_negative_zero_and_float_noise() {
        assert_eq!(clean(-0.0).to_string(), "0");
        assert_eq!(clean(0.1 + 0.2), 0.3);
    }
}
