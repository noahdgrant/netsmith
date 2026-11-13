//! The board's _Scene_ as a STEP file (ISO 10303-21, AP214 `AUTOMOTIVE_DESIGN`, ADR 0107): the
//! board and its parts as solids for a mechanical CAD tool. [`step`] writes it from the same
//! [`Scene`] the `.glb` is written from (`crate::gltf`); `docs/BOARD3D.md` describes the file
//! for a reader of it. [`read`] goes the other way: netsmith's own reader of a STEP model a
//! project names for a part (ADR 0108).
//!
//! # The contract
//!
//! - **Millimetres, Z up, the scene's frame** (`crate::scene3d`): the outline's bottom-left
//!   corner on the board's top face is the origin, the top face is `z = 0`, the bottom face
//!   `z = -thickness`. Every solid is written where it is: each part's placement is the
//!   identity.
//! - **An assembly.** The root product is the board's label; its children, in order, are
//!   `board` (the board solid) and one product per part with a body, by path: the product's id is
//!   the part's path, its name the designator, and its description the part's kind and where its
//!   height came from (`Resistor; height catalog`).
//! - **Exact solids.** Each solid is a closed shell of planes and cylinders: the
//!   board is its outline with its rounded corners, its cutouts and every drilled hole (round,
//!   or a slot) through the board's chosen thickness. A part's _Package shape_ is its solids
//!   ([`netsmith_pcb::body3d`]: boxes, prisms, cylinders), named `body`, `metal` or `mark`, each with
//!   the 3D view's colour; a part with no package record is its body box from its face to its
//!   far end, named `body`. A BGA's balls are the only solids made of flat facets, the polygons
//!   of the 3D view's balls. Pins that stand out of the far face by a stated length are a box
//!   named `pins`.
//! - **A part drawn from its STEP model** (ADR 0108) holds the model's solids, copied from its
//!   file ([`copy`]): one child product per solid, in the model's own units, placed by an
//!   `ITEM_DEFINED_TRANSFORMATION` where the scene draws it; the part's description names the
//!   file and its SHA-256.
//! - **No height is invented** (ADR 0094, ADR 0105). A part of unknown height has no solid: it
//!   is its body's outline as a closed polyline on its face (a wireframe, orange), and its
//!   description says `height unknown: outline only`.
//! - **What it leaves out**: the copper, the mask, the silkscreen and the height limits, which
//!   the `.glb` draws; a hole's plating.
//! - **Deterministic**: the same scene gives the same bytes. The header's time stamp is
//!   `1970-01-01T00:00:00`, the entities are numbered in the order they are made, a point or
//!   a direction is written once, and a real is written to ten places.

mod copy;
pub mod read;
mod writer;

#[cfg(test)]
mod tests;

pub use writer::{Extrusion, Loop, Polyhedron, Segment};
use writer::{Id, Writer, cross, dot, list, polygon, real, string};

use crate::mesh::package::{BALL_RINGS, ROUND_SEGMENTS, settled};
use crate::mesh::trig::sin_cos_turns;
use crate::raster3d::Theme;
use crate::scene3d::{BoardSolid, Body, DrawnModel, Pins, PlacedShape, Plan, Scene};
use crate::system::SystemScene;
use netsmith_pcb::body3d::{Axis, Material as Made, Primitive};
use netsmith_pcb::stackup::LayerKind;

/// What a solid of the file is coloured as: the 3D view's light palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Colour {
    Board,
    Substrate,
    Body,
    Metal,
    Mark,
    Unknown,
}

impl Colour {
    fn rgb(self) -> [u8; 3] {
        use crate::mesh::Material as M;
        let m = match self {
            Colour::Board => M::MaskTop,
            Colour::Substrate => M::Substrate,
            Colour::Body => M::Body,
            Colour::Metal => M::Metal,
            Colour::Mark => M::Mark,
            Colour::Unknown => M::BodyUnknown,
        };
        let [r, g, b, _] = Theme::Light.palette().colour(m);
        [r, g, b]
    }
}

/// The file being written: the entities, the shared context and the styles made so far.
struct File {
    w: Writer,
    /// The geometric context every representation is in.
    context: Id,
    product_context: Id,
    definition_context: Id,
    /// The style of each colour, made when first used.
    styles: Vec<(Colour, bool, Id)>,
    /// Every styled item, for the presentation at the end.
    styled: Vec<Id>,
    /// The units of the context: mm, radians, steradians.
    units: [Id; 3],
    /// The context of a STEP model's solids, by its length unit in mm and its angle unit in
    /// radians (the bits of each), made when first used.
    model_contexts: Vec<((u64, u64), Id)>,
}

impl File {
    fn new() -> File {
        let mut w = Writer::default();
        let app = w.add("APPLICATION_CONTEXT('automotive design')");
        w.add(format!(
            "APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,{app})"
        ));
        let mm = w.add("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))");
        let rad = w.add("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))");
        let sr = w.add("(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT())");
        let tol = w.add(format!(
            "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-06),{mm},'distance_accuracy_value','confusion accuracy')"
        ));
        let context = w.add(format!(
            "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT(({tol}))GLOBAL_UNIT_ASSIGNED_CONTEXT(({mm},{rad},{sr}))REPRESENTATION_CONTEXT('Context3D','3D context, millimetres'))"
        ));
        let product_context = w.add(format!("PRODUCT_CONTEXT('',{app},'mechanical')"));
        let definition_context = w.add(format!(
            "PRODUCT_DEFINITION_CONTEXT('part definition',{app},'design')"
        ));
        File {
            w,
            context,
            product_context,
            definition_context,
            styles: Vec::new(),
            styled: Vec::new(),
            units: [mm, rad, sr],
            model_contexts: Vec::new(),
        }
    }

    /// The representation context of a model's solids whose unit of length is `mm` mm and of
    /// angle `radians` radians: the file's own when they are mm and radians, else one with
    /// those units, so that the copied entities keep the numbers the model has.
    fn model_context(&mut self, mm: f64, radians: f64) -> Id {
        if mm == 1.0 && radians == 1.0 {
            return self.context;
        }
        let key = (mm.to_bits(), radians.to_bits());
        if let Some(&(_, id)) = self.model_contexts.iter().find(|(k, _)| *k == key) {
            return id;
        }
        let [mm_unit, rad, sr] = self.units;
        let w = &mut self.w;
        let length = if mm == 1.0 {
            mm_unit
        } else {
            let m = w.add(format!(
                "LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE({}),{mm_unit})",
                copy::exact(mm)
            ));
            let d = w.add("DIMENSIONAL_EXPONENTS(1.,0.,0.,0.,0.,0.,0.)");
            w.add(format!(
                "(CONVERSION_BASED_UNIT('unit',{m})LENGTH_UNIT()NAMED_UNIT({d}))"
            ))
        };
        let angle = if radians == 1.0 {
            rad
        } else {
            let m = w.add(format!(
                "PLANE_ANGLE_MEASURE_WITH_UNIT(PLANE_ANGLE_MEASURE({}),{rad})",
                copy::exact(radians)
            ));
            let d = w.add("DIMENSIONAL_EXPONENTS(0.,0.,0.,0.,0.,0.,0.)");
            w.add(format!(
                "(CONVERSION_BASED_UNIT('unit',{m})NAMED_UNIT({d})PLANE_ANGLE_UNIT())"
            ))
        };
        let tol = w.add(format!(
            "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE({}),{length},'distance_accuracy_value','confusion accuracy')",
            copy::exact(1e-6 / mm)
        ));
        let id = w.add(format!(
            "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT(({tol}))GLOBAL_UNIT_ASSIGNED_CONTEXT(({length},{angle},{sr}))REPRESENTATION_CONTEXT('Context3D','3D context, the units of a STEP model'))"
        ));
        self.model_contexts.push((key, id));
        id
    }

    /// A product and its shape: `(definition, shape definition)`.
    fn product(&mut self, id: &str, name: &str, description: &str) -> (Id, Id) {
        let w = &mut self.w;
        let p = w.add(format!(
            "PRODUCT({},{},{},({}))",
            string(id),
            string(name),
            string(description),
            self.product_context
        ));
        w.add(format!("PRODUCT_RELATED_PRODUCT_CATEGORY('part',$,({p}))"));
        let formation = w.add(format!("PRODUCT_DEFINITION_FORMATION('','',{p})"));
        let definition = w.add(format!(
            "PRODUCT_DEFINITION('design','',{formation},{})",
            self.definition_context
        ));
        let shape = w.add(format!("PRODUCT_DEFINITION_SHAPE('','',{definition})"));
        (definition, shape)
    }

    /// The style of `colour`, for a surface or (`curve`) a curve.
    fn style(&mut self, colour: Colour, curve: bool) -> Id {
        if let Some(&(_, _, id)) = self
            .styles
            .iter()
            .find(|(c, k, _)| *c == colour && *k == curve)
        {
            return id;
        }
        let [r, g, b] = colour.rgb();
        let c = |v: u8| real((f64::from(v) / 255.0 * 10_000.0).round() / 10_000.0);
        let w = &mut self.w;
        let rgb = w.add(format!("COLOUR_RGB('',{},{},{})", c(r), c(g), c(b)));
        let style = if curve {
            let font = w.add("DRAUGHTING_PRE_DEFINED_CURVE_FONT('continuous')");
            w.add(format!(
                "CURVE_STYLE('',{font},POSITIVE_LENGTH_MEASURE(0.1),{rgb})"
            ))
        } else {
            let fill = w.add(format!("FILL_AREA_STYLE_COLOUR('',{rgb})"));
            let area = w.add(format!("FILL_AREA_STYLE('',({fill}))"));
            let surface = w.add(format!("SURFACE_STYLE_FILL_AREA({area})"));
            let side = w.add(format!("SURFACE_SIDE_STYLE('',({surface}))"));
            w.add(format!("SURFACE_STYLE_USAGE(.BOTH.,{side})"))
        };
        let id = w.add(format!("PRESENTATION_STYLE_ASSIGNMENT(({style}))"));
        self.styles.push((colour, curve, id));
        id
    }

    fn paint(&mut self, item: Id, colour: Colour, curve: bool) {
        let style = self.style(colour, curve);
        let s = self.w.add(format!("STYLED_ITEM('color',({style}),{item})"));
        self.styled.push(s);
    }

    /// Puts the representation `rep` of the product with shape definition `child_shape` into
    /// the root: `child_axis` is the placement in `rep`, `root_axis` the root's.
    #[allow(clippy::too_many_arguments)]
    fn occurrence(
        &mut self,
        id: &str,
        name: &str,
        root: Id,
        root_rep: Id,
        root_axis: Id,
        child: Id,
        child_shape: Id,
        rep: Id,
        child_axis: Id,
    ) {
        self.w.add(format!(
            "SHAPE_DEFINITION_REPRESENTATION({child_shape},{rep})"
        ));
        self.link(id, name, root, root_rep, root_axis, child, rep, child_axis);
    }

    /// [`File::occurrence`] of a child whose shape is already defined: a product placed more
    /// than once.
    #[allow(clippy::too_many_arguments)]
    fn link(
        &mut self,
        id: &str,
        name: &str,
        root: Id,
        root_rep: Id,
        root_axis: Id,
        child: Id,
        rep: Id,
        child_axis: Id,
    ) {
        let w = &mut self.w;
        let nauo = w.add(format!(
            "NEXT_ASSEMBLY_USAGE_OCCURRENCE({},{},'',{root},{child},$)",
            string(id),
            string(name)
        ));
        let shape = w.add(format!("PRODUCT_DEFINITION_SHAPE('','',{nauo})"));
        let t = w.add(format!(
            "ITEM_DEFINED_TRANSFORMATION('','',{child_axis},{root_axis})"
        ));
        let rel = w.add(format!(
            "(REPRESENTATION_RELATIONSHIP('','',{rep},{root_rep})REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION({t})SHAPE_REPRESENTATION_RELATIONSHIP())"
        ));
        w.add(format!(
            "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION({rel},{shape})"
        ));
    }

    fn origin(&mut self) -> Id {
        self.w.axis([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])
    }
}

/// The scene as a STEP file, for the board labelled `board` (its name, or `system/member`).
pub fn step(scene: &Scene, board: &str) -> String {
    let mut f = File::new();
    board_product(&mut f, scene, board, "", true);
    finish(
        f,
        &format!("the board {board} and its parts, from netsmith"),
        board,
    )
}

/// The _System scene_ as a STEP file (ADR 0110, ADR 0107's assembly a level deeper): the root
/// product is the system; its children, in the order the system declares them, are one product
/// per member, each the board's assembly as [`step`] writes it (its id `System/member`, its
/// parts' ids `member/path`), placed by an `ITEM_DEFINED_TRANSFORMATION` that is the member's
/// motion.
pub fn system_step(system: &SystemScene) -> String {
    let mut f = File::new();
    let name = &system.name;
    let (root, root_shape) = f.product(name, name, "the system's boards, from netsmith");
    let root_axis = f.origin();
    let ctx = f.context;
    let root_rep = f.w.add(format!(
        "SHAPE_REPRESENTATION({},({root_axis}),{ctx})",
        string(name)
    ));
    f.w.add(format!(
        "SHAPE_DEFINITION_REPRESENTATION({root_shape},{root_rep})"
    ));
    for m in &system.members {
        let label = format!("{name}/{}", m.name);
        let made = board_product(&mut f, &m.scene, &label, &format!("{}/", m.name), false);
        let l = m.motion.linear();
        let column = |c: usize| [l[0][c], l[1][c], l[2][c]];
        let placed = f.w.axis(m.motion.offset, column(2), column(0));
        f.occurrence(
            &label, &m.name, root, root_rep, placed, made.root, made.shape, made.rep, made.axis,
        );
    }
    finish(
        f,
        &format!("the system {name} and its boards, from netsmith"),
        name,
    )
}

/// The file of `f`, its header describing it as `description` and naming it `name`.
fn finish(f: File, description: &str, name: &str) -> String {
    let mut f = f;
    if !f.styled.is_empty() {
        let styled = list(&f.styled);
        let ctx = f.context;
        f.w.add(format!(
            "MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION('',{styled},{ctx})"
        ));
    }
    let header = format!(
        "FILE_DESCRIPTION(({}),'2;1');\nFILE_NAME({},'1970-01-01T00:00:00',(''),(''),'netsmith','netsmith','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN {{ 1 0 10303 214 1 1 1 1 }}'));\n",
        string(description),
        string(name)
    );
    f.w.file(&header)
}

/// A board's assembly product, made in `f`: its definition, shape definition, representation and
/// that representation's axis. With `tie` its shape definition is tied to its representation (a
/// file of the board alone); without, a system's occurrence of it ties them. Every child
/// product's id starts with `prefix`.
struct BoardProduct {
    root: Id,
    shape: Id,
    rep: Id,
    axis: Id,
}

fn board_product(
    f: &mut File,
    scene: &Scene,
    board: &str,
    prefix: &str,
    tie: bool,
) -> BoardProduct {
    let (root, root_shape) = f.product(board, board, "the board and its parts, from netsmith");
    let root_axis = f.origin();
    let ctx = f.context;
    let root_rep = f.w.add(format!(
        "SHAPE_REPRESENTATION({},({root_axis}),{ctx})",
        string(board)
    ));
    if tie {
        f.w.add(format!(
            "SHAPE_DEFINITION_REPRESENTATION({root_shape},{root_rep})"
        ));
    }

    // The board.
    if let Some(e) = board_solid(&scene.board) {
        let colour = if scene.board.layers.iter().any(|l| l.kind == LayerKind::Mask) {
            Colour::Board
        } else {
            Colour::Substrate
        };
        let id = format!("{prefix}board");
        let (child, shape) = f.product(&id, "board", "the board, its outline, cutouts and holes");
        let axis = f.origin();
        let brep = f.w.extrusion("board", &e);
        f.paint(brep, colour, false);
        let rep = f.w.add(format!(
            "ADVANCED_BREP_SHAPE_REPRESENTATION('board',({axis},{brep}),{ctx})"
        ));
        f.occurrence(
            &id, "board", root, root_rep, root_axis, child, shape, rep, axis,
        );
    }

    // The parts, by path.
    for b in &scene.bodies {
        if let Some(model) = b.model.as_ref().filter(|m| m.exact) {
            model_part(f, b, model, prefix, (root, root_rep, root_axis));
            continue;
        }
        let solids = part_solids(b);
        let known = !solids.is_empty();
        let description = if b.z_far.is_some() {
            format!("{}; height {}", b.kind, b.provenance.word())
        } else {
            format!("{}; height unknown: outline only", b.kind)
        };
        let id = format!("{prefix}{}", b.path);
        let (child, shape) = f.product(&id, &b.refdes, &description);
        let axis = f.origin();
        let rep = if known {
            let mut items = vec![axis];
            for (name, colour, solid) in &solids {
                let brep = match solid {
                    Solid::Extrusion(e) => f.w.extrusion(name, e),
                    Solid::Polyhedron(p) => f.w.polyhedron(name, p),
                };
                f.paint(brep, *colour, false);
                items.push(brep);
            }
            f.w.add(format!(
                "ADVANCED_BREP_SHAPE_REPRESENTATION({},{},{ctx})",
                string(&b.refdes),
                list(&items)
            ))
        } else {
            let mut points: Vec<Id> = b
                .corners
                .iter()
                .map(|c| f.w.point([c.x, c.y, b.z_face]))
                .collect();
            points.push(points[0]);
            let line = f.w.add(format!("POLYLINE('outline',{})", list(&points)));
            let set = f.w.add(format!("GEOMETRIC_CURVE_SET('outline',({line}))"));
            f.paint(set, Colour::Unknown, true);
            f.w.add(format!(
                "GEOMETRICALLY_BOUNDED_WIREFRAME_SHAPE_REPRESENTATION({},({axis},{set}),{ctx})",
                string(&b.refdes)
            ))
        };
        f.occurrence(
            &id, &b.refdes, root, root_rep, root_axis, child, shape, rep, axis,
        );
    }
    BoardProduct {
        root,
        shape: root_shape,
        rep: root_rep,
        axis: root_axis,
    }
}

/// A part drawn from its STEP model (ADR 0108): its product holds one child product per solid
/// of the model, the solid's entities copied from the model's file ([`copy`]) in the model's own
/// units, each placed where the scene draws it by an `ITEM_DEFINED_TRANSFORMATION`. The
/// product's description names the file and its SHA-256. Pins that stand out of the far face
/// by a stated length are a box named `pins` in the part's own representation, as for any part.
fn model_part(f: &mut File, b: &Body, model: &DrawnModel, prefix: &str, under: (Id, Id, Id)) {
    let (root, root_rep, root_axis) = under;
    let id = format!("{prefix}{}", b.path);
    let height = match b.height {
        Some(_) => b.provenance.word(),
        None => "unknown",
    };
    let description = format!(
        "{}; height {height}; drawn from {} (sha256 {})",
        b.kind, model.file, model.sha256
    );
    let (part, part_shape) = f.product(&id, &b.refdes, &description);
    let axis = f.origin();
    let read = &model.read;
    // Each solid once: its product, shape and representation, and the origin it is placed by.
    let mut copies: Vec<Option<(Id, Id, Id)>> = vec![None; read.model.solids.len()];
    let mut places: Vec<(Id, (Id, Id, Id))> = Vec::new();
    for placed in &read.model.placed {
        let solid = &read.model.solids[placed.solid];
        let copied = match copies[placed.solid] {
            Some(c) => c,
            None => {
                let (ids, styles) = copy::closure(&read.exchange, solid.id);
                let map = copy::copy(&read.exchange, &ids, &mut f.w);
                f.styled.extend(styles.iter().filter_map(|s| map.get(s)));
                let new = map[&solid.id];
                let name = if solid.name.is_empty() {
                    format!("#{}", solid.id)
                } else {
                    solid.name.clone()
                };
                let (def, shape) = f.product(
                    &format!("{id}:{name}"),
                    &name,
                    &format!("a solid of {}", model.file),
                );
                let ctx = f.model_context(solid.mm, solid.radians);
                let origin = f.origin();
                let kind = read
                    .exchange
                    .data
                    .get(&solid.id)
                    .map_or_else(String::new, |i| i.name());
                let rep = match kind.as_str() {
                    "MANIFOLD_SOLID_BREP" | "BREP_WITH_VOIDS" => {
                        "ADVANCED_BREP_SHAPE_REPRESENTATION"
                    }
                    "FACETED_BREP" => "FACETED_BREP_SHAPE_REPRESENTATION",
                    _ => "SHAPE_REPRESENTATION",
                };
                let rep =
                    f.w.add(format!("{rep}({},({origin},{new}),{ctx})", string(&name)));
                f.w.add(format!("SHAPE_DEFINITION_REPRESENTATION({shape},{rep})"));
                let c = (def, rep, origin);
                copies[placed.solid] = Some(c);
                c
            }
        };
        // The solid's frame, in mm, where the scene draws it: the file's place, then the
        // part's (a rigid motion: `DrawnModel::exact`).
        let motion = model.to_scene.then_after(&placed.place);
        let col = |c: usize| [motion.m[0][c], motion.m[1][c], motion.m[2][c]];
        let at = f.w.axis(motion.t, col(2), col(0));
        places.push((at, copied));
    }
    let mut items = vec![axis];
    items.extend(places.iter().map(|(at, _)| *at));
    let ctx = f.context;
    if let Some(pins) = pins_solid(b) {
        let brep = f.w.extrusion("pins", &pins);
        f.paint(brep, Colour::Metal, false);
        items.push(brep);
    }
    let rep = f.w.add(format!(
        "ADVANCED_BREP_SHAPE_REPRESENTATION({},{},{ctx})",
        string(&b.refdes),
        list(&items)
    ));
    f.occurrence(
        &id, &b.refdes, root, root_rep, root_axis, part, part_shape, rep, axis,
    );
    for (k, (at, (def, child_rep, origin))) in places.into_iter().enumerate() {
        f.link(
            &format!("{id}:{}", k + 1),
            &b.refdes,
            part,
            rep,
            at,
            def,
            child_rep,
            origin,
        );
    }
}

/// The box of a part's pins out of the far face, where their length is stated.
fn pins_solid(b: &Body) -> Option<Extrusion> {
    let Pins::Known {
        at,
        z_top,
        z_bottom,
        ..
    } = &b.pins
    else {
        return None;
    };
    Extrusion {
        origin: [0.0; 3],
        u: X,
        v: Y,
        n: Z,
        loops: vec![rounded(xy(at.min), xy(at.max), 0.0)],
        from: *z_bottom,
        to: *z_top,
    }
    .normalised()
}

/// A solid of a part.
#[derive(Debug, Clone, PartialEq)]
enum Solid {
    Extrusion(Extrusion),
    Polyhedron(Polyhedron),
}

const X: [f64; 3] = [1.0, 0.0, 0.0];
const Y: [f64; 3] = [0.0, 1.0, 0.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];

/// A loop round the box from `min` to `max` with corners of `radius`, counterclockwise.
fn rounded(min: [f64; 2], max: [f64; 2], radius: f64) -> Loop {
    let (w, h) = (max[0] - min[0], max[1] - min[1]);
    let r = radius.min(0.5 * w).min(0.5 * h);
    if r <= 1e-9 {
        return polygon(&[min, [max[0], min[1]], max, [min[0], max[1]]]);
    }
    let (x0, y0, x1, y1) = (min[0], min[1], max[0], max[1]);
    let (wide, tall) = (w - 2.0 * r > 1e-9, h - 2.0 * r > 1e-9);
    let mut out = Vec::new();
    let mut side = |on: bool, start: [f64; 2], arc: [f64; 2], centre: [f64; 2]| {
        if on {
            out.push(Segment::Line { start });
        }
        out.push(Segment::Arc {
            start: arc,
            centre,
            ccw: true,
        });
    };
    side(wide, [x0 + r, y0], [x1 - r, y0], [x1 - r, y0 + r]);
    side(tall, [x1, y0 + r], [x1, y1 - r], [x1 - r, y1 - r]);
    side(wide, [x1 - r, y1], [x0 + r, y1], [x0 + r, y1 - r]);
    side(tall, [x0, y1 - r], [x0, y0 + r], [x0 + r, y0 + r]);
    out
}

/// The loop round a slot of `diameter` with its axis from `a` to `b` (a circle when they meet),
/// counterclockwise.
fn slot(a: [f64; 2], b: [f64; 2], diameter: f64) -> Loop {
    let r = 0.5 * diameter;
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    if len <= 1e-9 {
        return vec![Segment::Arc {
            start: [a[0] + r, a[1]],
            centre: a,
            ccw: true,
        }];
    }
    let right = [dy / len * r, -dx / len * r];
    let at = |c: [f64; 2], s: f64| [c[0] + s * right[0], c[1] + s * right[1]];
    vec![
        Segment::Line { start: at(a, 1.0) },
        Segment::Arc {
            start: at(b, 1.0),
            centre: b,
            ccw: true,
        },
        Segment::Line { start: at(b, -1.0) },
        Segment::Arc {
            start: at(a, -1.0),
            centre: a,
            ccw: true,
        },
    ]
}

fn xy(p: Plan) -> [f64; 2] {
    [p.x, p.y]
}

/// The board through its chosen thickness, with its cutouts and drilled holes.
fn board_solid(solid: &BoardSolid) -> Option<Extrusion> {
    let mut loops = vec![rounded(
        [0.0, 0.0],
        [solid.width, solid.height],
        solid.corner_radius,
    )];
    for c in &solid.cutouts {
        loops.push(rounded(xy(c.at.min), xy(c.at.max), c.radius));
    }
    for h in &solid.holes {
        loops.push(slot(xy(h.from), xy(h.to), h.diameter));
    }
    Extrusion {
        origin: [0.0; 3],
        u: X,
        v: Y,
        n: Z,
        loops,
        from: -solid.thickness,
        to: 0.0,
    }
    .normalised()
}

/// The solids of a part, each with its name and colour: none when its height is unknown.
fn part_solids(b: &Body) -> Vec<(&'static str, Colour, Solid)> {
    let mut out = Vec::new();
    let Some(far) = b.z_far else {
        return out;
    };
    match &b.shape {
        Some(shape) => {
            for k in 0..shape.solids.len() {
                let (name, colour) = match shape.solids[k].material {
                    Made::Body => ("body", Colour::Body),
                    Made::Metal | Made::Ball => ("metal", Colour::Metal),
                    Made::Mark => ("mark", Colour::Mark),
                };
                if let Some(s) = placed(shape, &settled(&shape.solids, k)) {
                    out.push((name, colour, s));
                }
            }
        }
        None => {
            let ring: Vec<[f64; 2]> = b.corners.iter().map(|&c| xy(c)).collect();
            let e = Extrusion {
                origin: [0.0; 3],
                u: X,
                v: Y,
                n: Z,
                loops: vec![polygon(&ring)],
                from: b.z_face,
                to: far,
            };
            out.extend(
                e.normalised()
                    .map(|e| ("body", Colour::Body, Solid::Extrusion(e))),
            );
        }
    }
    out.extend(pins_solid(b).map(|e| ("pins", Colour::Metal, Solid::Extrusion(e))));
    out
}

/// A direction of the footprint's frame in the scene's.
fn turn(shape: &PlacedShape, d: [f64; 3]) -> [f64; 3] {
    let l = shape.place.linear();
    [dot(l[0], d), dot(l[1], d), dot(l[2], d)]
}

/// A primitive of a package shape, in the footprint's frame, where the part is.
fn placed(shape: &PlacedShape, p: &Primitive) -> Option<Solid> {
    let place = &shape.place;
    let origin = [place.origin.x, place.origin.y, place.z_face];
    let extrusion = |u, v, n, loops, from, to| {
        Extrusion {
            origin,
            u: turn(shape, u),
            v: turn(shape, v),
            n: turn(shape, n),
            loops,
            from,
            to,
        }
        .normalised()
        .map(Solid::Extrusion)
    };
    match p {
        Primitive::Box { min, max } => {
            if !(0..3).all(|k| min[k] < max[k]) {
                return None;
            }
            let ring = rounded([min[0], min[1]], [max[0], max[1]], 0.0);
            extrusion(X, Y, Z, vec![ring], min[2], max[2])
        }
        Primitive::Prism {
            axis,
            profile,
            from,
            to,
        } => {
            // The profile's (a, b) and the length along the axis, as `body3d` states them.
            let (u, v, n) = match axis {
                Axis::Z => (X, Y, Z),
                Axis::Y => (X, Z, Y),
                Axis::X => (Y, Z, X),
            };
            extrusion(u, v, n, vec![polygon(profile)], *from, *to)
        }
        Primitive::Cylinder {
            centre,
            radius,
            z0,
            z1,
        } => {
            if *radius <= 0.0 {
                return None;
            }
            let ring = vec![Segment::Arc {
                start: [centre.x + radius, centre.y],
                centre: [centre.x, centre.y],
                ccw: true,
            }];
            extrusion(X, Y, Z, vec![ring], *z0, *z1)
        }
        Primitive::Spheroid {
            centre,
            radius,
            z0,
            z1,
        } => {
            if !(*radius > 0.0 && z1 > z0) {
                return None;
            }
            let ball = spheroid([centre.x, centre.y], *radius, *z0, *z1);
            let l = place.linear();
            let det = dot(l[0], cross(l[1], l[2]));
            let points = ball.points.iter().map(|&q| place.point(q)).collect();
            let faces = ball
                .faces
                .into_iter()
                .map(|mut f| {
                    if det < 0.0 {
                        f.reverse();
                    }
                    f
                })
                .collect();
            Some(Solid::Polyhedron(Polyhedron { points, faces }))
        }
    }
}

/// A ball as the 3D view's mesh draws it (`Mesh::spheroid`): a pole at each end and rings of
/// [`ROUND_SEGMENTS`] corners between, as triangles at the poles and flat quads between rings.
fn spheroid(centre: [f64; 2], radius: f64, z0: f64, z1: f64) -> Polyhedron {
    let (n, rings) = (ROUND_SEGMENTS, BALL_RINGS);
    let (c, zc) = (0.5 * (z1 - z0), 0.5 * (z0 + z1));
    let mut points = vec![[centre[0], centre[1], z1]];
    for k in 1..rings {
        let (st, ct) = sin_cos_turns(k as f64 / (2 * rings) as f64);
        for j in 0..n {
            let (sp, cp) = sin_cos_turns(j as f64 / n as f64);
            points.push([
                centre[0] + radius * st * cp,
                centre[1] + radius * st * sp,
                zc + c * ct,
            ]);
        }
    }
    let bottom = points.len();
    points.push([centre[0], centre[1], z0]);
    let at = |ring: usize, j: usize| 1 + ring * n + j % n;
    let mut faces = Vec::new();
    for j in 0..n {
        faces.push(vec![0, at(0, j), at(0, j + 1)]);
        for r in 0..rings - 2 {
            faces.push(vec![at(r, j), at(r + 1, j), at(r + 1, j + 1), at(r, j + 1)]);
        }
        faces.push(vec![at(rings - 2, j), bottom, at(rings - 2, j + 1)]);
    }
    Polyhedron { points, faces }
}
