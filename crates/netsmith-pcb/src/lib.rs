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
mod conflict;
mod constraints;
mod copper;
mod copper_areas;
mod explain;
mod export;
mod fit_report;
mod footprint;
mod generate;
mod geometry;
mod impedance;
mod laid_out;
mod layout;
pub mod layout_notes;
mod live;
mod miss;
mod modules;
mod origin;
mod overhang;
mod part_fit;
mod part_geometry;
mod part_layout;
mod picture;
mod placement;
mod polygon;
mod protrusion;
mod quality;
mod ratsnest;
mod routes;
mod rules;
mod shape;
mod simplify;
mod stackup;
mod standard_height;
mod summary;
mod svg;
mod track_drop;
mod validate;

pub use board::{
    Board, BoardCutout, BoardError, BoardKeepout, BoardNet, BoardPour, BoardZone, CopperLayer,
    FixedCopper, Hang, HeightUnchecked, HoleNeighbour, KeepoutArea, Loaded as LoadedBoard, PadRef,
    PairCoupling, Placed, PlacementProblem, Plane, ReferenceSide, Side, Silk, StandardPass,
    Teardrop, Track, Trim, Unplaced, Via, ViaOffer, ViaSpan, ZoneIslands, ZonePads,
    member as board_member, outline_box, rect_distance, span_depth, via_offers,
};
pub use body3d::{
    Axis as BodyAxis, Material as BodyMaterial, Primitive as BodyPrimitive, Solid as BodySolid,
    header_of_file, package_of, package_shape, pin_tails,
};
pub use conflict::{
    Conflict, ConflictArea, ConflictLine, ConflictOutline, EdgePast, PartBox, PlacementFault, Span,
    Turn,
};
pub use constraints::{
    PartWindow, PlanOffset, RIGHT, Reach, Want, axis_of as constraint_axis_of,
    locked_breaks as constraint_locked_breaks, offset as constraint_offset, part_windows,
    plan as constraint_plan, quarter as constraint_quarter,
};
pub use copper::{
    Area, Copper, CopperError, LayerPours, Pour, SilkInk, ZoneFill, artwork as copper_artwork,
    copper as copper_of, plane_pour_fills, pours as pours_of, zone_fills,
};
pub use copper_areas::{
    AreaKind, AreaLayer, AreaPour, AreaTeardropLayer, AreaTeardrops, AreaZone, CopperAreas,
};
pub use explain::{
    Fault, Precheck, RequirementReport, before_placing, explain as explain_requirements,
};
pub use export::{PATH_PROPERTY, kicad_pcb, kicad_pro};
pub use fit_report::{FitReport, PartRef, Size};
pub use footprint::{
    Drill, Footprint, FootprintError, FootprintLibrary, Graphic, Layer, Mount, Pad, PadKind,
    PadShape, Shape, SilkText,
};
pub use generate::{
    HOLE_DRILL, generate as generated_footprint, generate_spec as generate_footprint_spec,
    names as generated_footprint_names,
};
pub use geometry::{Pt, Rect, TURNED_STRIP, TURNED_STRIPS, is_right_angle};
pub use impedance::{
    Line as ImpedanceLine, OutOfRange, Pair as ImpedancePair, TEST_TOLERANCE, TargetLayer,
    TargetWidth, TrackDielectric, coated_coupled_microstrip, coated_microstrip, coupled_microstrip,
    coupled_stripline, impedance_at, impedance_notes, microstrip, stripline, target_layers,
    target_notes, target_widths, width_for,
};
pub use layout::{
    BoardLayout, Face as LayoutFace, IMPORT_SOURCE, KeepoutPlace, Layout, LayoutError, PartPlace,
    Waiver, apply_layout, check_drawn_keepout, copper_layers, keepout_declaration, place_options,
};
pub use live::{Glimpse, GlimpsePart, glimpses_watched, show_glimpse, watching_glimpses};
pub use miss::RequirementMiss;
pub use modules::{
    MODULE_COLOURS, MODULE_COLOURS_DARK, MODULE_COLOURS_LIGHT, in_module, module_colour,
    module_paths_of, rgb_of,
};
pub use origin::{ORIGINS_HEAD, PartOrigin, mm as mm_text, origins_text};
pub use overhang::{
    Edge, Edges, Overhang, clip as clip_to_outline, needs as overhang_needs,
    reached as overhang_reached,
};
pub use part_fit::{FIT_STEP, FitNear, FitPlace, FitTarget, PartFit};
pub use part_geometry::{PadGeometry, PartFrame, PartGeometries, PartGeometry};
pub use part_layout::{
    LayoutFinding, Limit, LoopPair, Measure, NearPair, PartGuideline, away_pairs, edge_parts,
    layout_findings, loop_pairs, measures, near_findings, near_pairs, pins_of,
};
pub use picture::{
    BACKGROUND as PICTURE_BACKGROUND, COPPER as PICTURE_COPPER, Layers as PictureLayers, Lit,
    MARGIN as PICTURE_MARGIN, Marker, Picture as PictureOptions, picture as picture_svg,
    whole as picture_whole,
};
pub use placement::{
    CarriedZone, DESCRIBED, DoesNotFit, ESCAPE_ROOM, Ended, FaceKeepout, Fixed, HeightZone, ORIGIN,
    OutsideAt, OverlapAt, Oversize, PartState, PlaceOptions, inside as fit_inside, is_mechanical,
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
pub use rules::{HDI_SIX_BUILD_UP, NetClass, Rules};
pub use shape::{Edge as ShapeEdge, RoundRect};
pub use simplify::{distance_to_segment, simplify_ring};
pub use stackup::{
    LayerKind, StackLayer, layers as stackup_layers, round as round_nm, thickness_tolerance,
};
pub use standard_height::StandardHeight;
pub use summary::{
    DecouplingSummary, FaceSummary, GuidelineSummary, ModuleSummary, Moved, Neighbour, Neighbours,
    NetSummary, Place, PlacementComparison, RatsnestLine, Summary, SummaryBox, Totals,
};
pub use svg::{render as render_svg, render_view as render_svg_view};
pub use track_drop::{
    COPPER_RESISTIVITY, DEFAULT_DROP_BUDGET, NetPad, TrackDrop, in_outer_copper,
    net_drop as track_drop_of,
};
pub use validate::{Subject, validate_pinout};
