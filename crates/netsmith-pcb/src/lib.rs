//! The board model: footprints from a tool-independent `Design`, the board before and after
//! placing, what the placer is told and answers ([`placement`]), the requirements and their
//! checks, a ratsnest, how good a placement is ([`quality`]), and export to KiCad and SVG. The
//! placer is in `netsmith-placer`, the router in `netsmith-router`, and the 3D scene and its
//! glTF, STEP and PNG writers in `netsmith-3d`; all three depend on this crate.
//!
//! ```text
//! Design --Board::unplaced--> Unplaced --(the placer)--> Board --(the router)--> Board (with tracks and vias)
//!                                  |--kicad_pcb / kicad_pro / svg::render
//!                                  |--Ratsnest::of
//! ```
//!
//! # What a fabrication step reads
//!
//! Everything it needs is public data on [`Board`], in mm, with the origin at the sheet's
//! top left and y growing downward, as KiCad has it:
//!
//! - [`Board::parts`], one [`Placed`] per part: `refdes`, `kind`, `value`, `library_id`
//!   (the KiCad footprint, such as `Resistor_SMD:R_0402_1005Metric`), `at` (the footprint
//!   origin), `rotation` (degrees, counterclockwise on screen), `side` ([`Side`]), and
//!   [`Placed::mount`], which says [`Mount::Smd`] or [`Mount::ThroughHole`]. The pads are
//!   `placed.footprint.pads`, and [`Placed::pad_centre`] puts one on the board.
//! - [`Board::nets`]: each net's name and [`PadRef`]s (reference and pad number).
//! - [`Board::outline`]: the board edge, a rectangle.
//! - [`Board::tracks`] and [`Board::vias`]: the routed copper, on the two outer layers.
//! - [`Board::planes`]: the inner layers of a four-layer board, each poured on one net.
//! - [`Board::rules`]: the [`Rules`] the board is made to.
//!
//! A footprint's origin is where KiCad puts it, which is not always the centre of the part
//! as a pick-and-place file wants it; use [`Footprint::courtyard`] for the centre.
//!
//! The modules are private; this root lists the crate's API. A name that would be too general
//! at the root carries what it belongs to (`BodyMaterial`, `copper_of`, `render_svg`).

#![cfg_attr(not(test), warn(unreachable_pub))]

mod board;
mod body3d;
mod constraints;
mod copper;
mod explain;
mod export;
mod footprint;
mod generate;
mod geometry;
mod layout;
mod origin;
mod overhang;
mod placement;
mod polygon;
mod protrusion;
mod quality;
mod ratsnest;
mod routes;
mod rules;
mod shape;
mod stackup;
mod standard_height;
mod svg;
mod validate;

pub use board::{
    Board, BoardCutout, BoardError, BoardKeepout, BoardNet, BoardPour, BoardZone, CopperLayer,
    FixedCopper, Hang, HeightUnchecked, HoleNeighbour, KeepoutArea, Loaded as LoadedBoard, PadRef,
    Placed, PlacementProblem, Plane, ReferenceSide, Side, StandardPass, Teardrop, Track, Trim,
    Unplaced, Via, ZoneIslands, ZonePads, member as board_member, outline_box, rect_distance,
};
pub use body3d::{
    Axis as BodyAxis, Material as BodyMaterial, Primitive as BodyPrimitive, Solid as BodySolid,
    header_of_file, package_of, package_shape, pin_tails,
};
pub use constraints::{
    PlanOffset, Want, axis_of as constraint_axis_of, offset as constraint_offset,
    plan as constraint_plan, quarter as constraint_quarter,
};
pub use copper::{
    Area, Copper, CopperError, LayerPours, Pour, SilkInk, ZoneFill, artwork as copper_artwork,
    copper as copper_of, pours as pours_of, zone_fills,
};
pub use explain::{
    Fault, Precheck, RequirementReport, before_placing, explain as explain_requirements,
};
pub use export::{PATH_PROPERTY, kicad_pcb, kicad_pro};
pub use footprint::{
    Drill, Footprint, FootprintError, FootprintLibrary, Graphic, Layer, Mount, Pad, PadKind,
    PadShape, Shape,
};
pub use generate::{
    generate as generated_footprint, generate_spec as generate_footprint_spec,
    names as generated_footprint_names,
};
pub use geometry::{Pt, Rect};
pub use layout::{
    BoardLayout, Face as LayoutFace, IMPORT_SOURCE, KeepoutPlace, Layout, LayoutError, PartPlace,
    apply_layout, copper_layers, place_options,
};
pub use origin::{ORIGINS_HEAD, PartOrigin, mm as mm_text, origins_text};
pub use overhang::{Edge, Edges, Overhang, clip as clip_to_outline, needs as overhang_needs};
pub use placement::{
    CarriedZone, DESCRIBED, DoesNotFit, Ended, Fixed, HeightZone, ORIGIN, OutsideAt, OverlapAt,
    Oversize, PartState, PlaceOptions, inside as fit_inside, is_mechanical,
    oversize as check_oversize, short as short_mm,
};
pub use polygon::{BoardEdge, Polygon};
pub use protrusion::{Cover, Protrusion, Verdict, judge as judge_protrusion, may_be_covered};
pub use quality::{
    DecouplingDistance, DecouplingPair, PairNet, Quality, crossings, decoupling_pairs,
    segments_cross,
};
pub use ratsnest::{Node as RatNode, Ratline, Ratsnest, nodes_of as ratsnest_nodes_of};
pub use routes::{
    BoardRoutes, NetRoutes, RouteTrack, RouteVia, RouteZone, Routes, RoutesError, RoutesNote,
    VERSION as ROUTES_VERSION, apply_routes, kicad_layer as routes_kicad_layer,
    layer_word as routes_layer_word, pad_hash, record_of, routes_path,
};
pub use rules::Rules;
pub use shape::{Edge as ShapeEdge, RoundRect};
pub use stackup::{
    LayerKind, StackLayer, layers as stackup_layers, round as round_nm, thickness_tolerance,
};
pub use standard_height::StandardHeight;
pub use svg::{render as render_svg, render_view as render_svg_view};
pub use validate::{Subject, validate_pinout};
