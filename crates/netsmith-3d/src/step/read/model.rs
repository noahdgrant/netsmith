//! The boundary representation a STEP file holds, read from its instances: the solids, found
//! through the shape representations and the assemblies that place them; their shells, faces,
//! loops and edges; the curves and surfaces under them; the length unit of each representation
//! in mm; and each face's colour where the file styles it.
//!
//! Everything is read in id order and every placement is applied in the order the file nests
//! them, so the same file gives the same model. An entity the reader does not know, inside a
//! solid, is an error naming it: a model is never read in part.

use std::collections::BTreeMap;

use super::StepError;
use super::geometry::{BSplineCurve, BSplineSurface, Curve, Frame, Knots, P3, Surface, unit};
use super::part21::{Entity, Exchange, Instance, Param};

/// A colour, 0 to 255 a channel.
pub type Rgb = [u8; 3];

/// An edge: from `start` to `end` along `curve`, with the curve's direction when `same_sense`.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub id: u64,
    pub line: u32,
    pub start: P3,
    pub end: P3,
    pub curve: Curve,
    pub same_sense: bool,
}

/// An edge of a loop, run forward (from its start to its end) or backward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Use {
    pub edge: usize,
    pub forward: bool,
}

/// A loop bounding a face.
#[derive(Debug, Clone, PartialEq)]
pub enum Loop {
    Edges(Vec<Use>),
    /// A single point: a cone's apex, a sphere's pole.
    Vertex(P3),
    /// A polygon, its sides straight.
    Poly(Vec<P3>),
}

/// A face: the part of `surface` its loops bound. Its outside is the surface's normal when
/// `same_sense`.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub id: u64,
    pub line: u32,
    pub surface: Surface,
    pub same_sense: bool,
    /// Each loop with the way it is run: as written, or reversed.
    pub bounds: Vec<(Loop, bool)>,
    pub colour: Option<Rgb>,
}

/// A shell: faces, turned over when `reversed`.
#[derive(Debug, Clone, PartialEq)]
pub struct Shell {
    pub id: u64,
    pub faces: Vec<usize>,
    pub closed: bool,
    pub reversed: bool,
    pub colour: Option<Rgb>,
}

/// A solid (or a surface model): its shells, in the file's unit, `mm` mm a unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Solid {
    pub id: u64,
    pub line: u32,
    pub name: String,
    pub shells: Vec<Shell>,
    pub colour: Option<Rgb>,
    /// Millimetres a unit of length of the representation the solid is in.
    pub mm: f64,
}

/// A rigid motion (or a mirror): `p -> m * p + t`, `m` by rows, lengths in mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub m: [[f64; 3]; 3],
    pub t: P3,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0; 3],
    };

    /// The motion that takes a frame's local coordinates to where the frame is.
    fn of(f: &Frame) -> Affine {
        Affine {
            m: [0, 1, 2].map(|r| [f.x[r], f.y[r], f.z[r]]),
            t: f.o,
        }
    }

    /// The motion back, for a frame's (whose `m` is orthonormal).
    fn inverse_rigid(&self) -> Affine {
        let m = [0, 1, 2].map(|r| [0, 1, 2].map(|c| self.m[c][r]));
        let t =
            [0, 1, 2].map(|r| -(m[r][0] * self.t[0] + m[r][1] * self.t[1] + m[r][2] * self.t[2]));
        Affine { m, t }
    }

    pub fn apply(&self, p: P3) -> P3 {
        [0, 1, 2]
            .map(|r| self.m[r][0] * p[0] + self.m[r][1] * p[1] + self.m[r][2] * p[2] + self.t[r])
    }

    /// `self` after `inner`.
    pub fn then_after(&self, inner: &Affine) -> Affine {
        let m = [0, 1, 2]
            .map(|r| [0, 1, 2].map(|c| (0..3).map(|k| self.m[r][k] * inner.m[k][c]).sum()));
        let t = self.apply(inner.t);
        Affine { m, t }
    }
}

/// A solid where the file puts it: `place` maps the solid's points, in mm, to the model's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub solid: usize,
    pub place: Affine,
}

/// The model: every solid once, and each place a solid is drawn at.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Model {
    pub edges: Vec<Edge>,
    pub faces: Vec<Face>,
    pub solids: Vec<Solid>,
    pub placed: Vec<Placed>,
}

/// The units of a representation.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Units {
    mm: f64,
    /// Radians a unit of plane angle.
    radians: f64,
}

const MM: Units = Units {
    mm: 1.0,
    radians: 1.0,
};

/// How deep assemblies may nest.
const MAX_DEPTH: usize = 64;

/// Reads the model of `ex`.
pub fn model(ex: &Exchange) -> Result<Model, StepError> {
    let mut r = Reader {
        ex,
        model: Model::default(),
        edges: BTreeMap::new(),
        faces: BTreeMap::new(),
        solids: BTreeMap::new(),
        colours: colours(ex),
    };
    let found = r.placements()?;
    let found = if found.is_empty() {
        // No representation holds a solid: every solid of the file, where it is.
        let mut all = Vec::new();
        for (&id, inst) in &ex.data {
            if inst.parts.len() == 1 && is_solid(&inst.parts[0].name) {
                all.push((id, MM, Affine::IDENTITY));
            }
        }
        all
    } else {
        found
    };
    for (id, units, place) in found {
        let solid = r.solid(id, units)?;
        r.model.placed.push(Placed { solid, place });
    }
    if r.model.placed.is_empty() {
        return Err(StepError::NoSolid);
    }
    Ok(r.model)
}

fn is_solid(name: &str) -> bool {
    matches!(
        name,
        "MANIFOLD_SOLID_BREP" | "BREP_WITH_VOIDS" | "SHELL_BASED_SURFACE_MODEL" | "FACETED_BREP"
    )
}

/// The colour each styled item is given: an over-riding style wins over a plain one, and a
/// later one over an earlier.
fn colours(ex: &Exchange) -> BTreeMap<u64, Rgb> {
    let mut out = BTreeMap::new();
    for pass in ["STYLED_ITEM", "OVER_RIDING_STYLED_ITEM"] {
        for inst in ex.data.values() {
            let Some(e) = inst.part(pass) else { continue };
            let (Some(styles), Some(item)) = (
                e.params.get(1).and_then(Param::as_list),
                e.params.get(2).and_then(Param::as_ref),
            ) else {
                continue;
            };
            let mut found = None;
            for s in styles {
                if let Some(id) = s.as_ref() {
                    found = found.or_else(|| colour_in(ex, id, 0));
                }
            }
            if let Some(c) = found {
                out.insert(item, c);
            }
        }
    }
    out
}

/// The first surface colour under the style `id`.
fn colour_in(ex: &Exchange, id: u64, depth: usize) -> Option<Rgb> {
    if depth > 16 {
        return None;
    }
    let inst = ex.data.get(&id)?;
    for e in &inst.parts {
        match e.name.as_str() {
            "COLOUR_RGB" => {
                let c = |k: usize| {
                    let x = e.params.get(k)?.as_real()?;
                    Some((x.clamp(0.0, 1.0) * 255.0).round() as u8)
                };
                return Some([c(1)?, c(2)?, c(3)?]);
            }
            "DRAUGHTING_PRE_DEFINED_COLOUR" => {
                return Some(match e.params.first()?.as_str()? {
                    "red" => [255, 0, 0],
                    "green" => [0, 255, 0],
                    "blue" => [0, 0, 255],
                    "yellow" => [255, 255, 0],
                    "magenta" => [255, 0, 255],
                    "cyan" => [0, 255, 255],
                    "black" => [0, 0, 0],
                    "white" => [255, 255, 255],
                    _ => return None,
                });
            }
            // A curve's colour is not a face's.
            "CURVE_STYLE" => return None,
            _ => {}
        }
    }
    let mut refs = Vec::new();
    for e in &inst.parts {
        collect_refs(&e.params, &mut refs);
    }
    refs.into_iter().find_map(|r| colour_in(ex, r, depth + 1))
}

fn collect_refs(params: &[Param], out: &mut Vec<u64>) {
    for p in params {
        match p {
            Param::Ref(r) => out.push(*r),
            Param::List(v) | Param::Typed(_, v) => collect_refs(v, out),
            _ => {}
        }
    }
}

struct Reader<'a> {
    ex: &'a Exchange,
    model: Model,
    edges: BTreeMap<u64, usize>,
    faces: BTreeMap<u64, usize>,
    solids: BTreeMap<u64, usize>,
    colours: BTreeMap<u64, Rgb>,
}

/// What a representation links to: its items, and the representations placed in it.
#[derive(Debug, Default)]
struct Rep {
    items: Vec<u64>,
    context: Option<u64>,
}

impl Reader<'_> {
    fn inst(&self, id: u64, by: u64) -> Result<&Instance, StepError> {
        self.ex.data.get(&id).ok_or_else(|| StepError::Missing {
            id,
            by,
            line: self.line(by),
        })
    }

    fn line(&self, id: u64) -> u32 {
        self.ex.data.get(&id).map_or(0, |i| i.line)
    }

    fn unsupported(&self, id: u64) -> StepError {
        StepError::Unsupported {
            id,
            line: self.line(id),
            entity: self
                .ex
                .data
                .get(&id)
                .map_or_else(String::new, Instance::name),
        }
    }

    fn malformed(&self, id: u64, message: impl Into<String>) -> StepError {
        StepError::Malformed {
            id,
            line: self.line(id),
            entity: self
                .ex
                .data
                .get(&id)
                .map_or_else(String::new, Instance::name),
            message: message.into(),
        }
    }

    /// The simple entity `id`, which `by` refers to.
    fn simple(&self, id: u64, by: u64) -> Result<&Entity, StepError> {
        let inst = self.inst(id, by)?;
        if inst.is_complex() {
            return Err(self.unsupported(id));
        }
        Ok(&inst.parts[0])
    }

    fn param<'p>(&self, id: u64, e: &'p Entity, k: usize) -> Result<&'p Param, StepError> {
        e.params
            .get(k)
            .ok_or_else(|| self.malformed(id, format!("it has no parameter {}", k + 1)))
    }

    fn real(&self, id: u64, e: &Entity, k: usize) -> Result<f64, StepError> {
        self.param(id, e, k)?
            .as_real()
            .filter(|x| x.is_finite())
            .ok_or_else(|| self.malformed(id, format!("parameter {} is not a number", k + 1)))
    }

    fn reference(&self, id: u64, e: &Entity, k: usize) -> Result<u64, StepError> {
        self.param(id, e, k)?
            .as_ref()
            .ok_or_else(|| self.malformed(id, format!("parameter {} is not a reference", k + 1)))
    }

    fn list<'p>(&self, id: u64, e: &'p Entity, k: usize) -> Result<&'p [Param], StepError> {
        self.param(id, e, k)?
            .as_list()
            .ok_or_else(|| self.malformed(id, format!("parameter {} is not a list", k + 1)))
    }

    fn refs(&self, id: u64, e: &Entity, k: usize) -> Result<Vec<u64>, StepError> {
        self.list(id, e, k)?
            .iter()
            .map(|p| {
                p.as_ref().ok_or_else(|| {
                    self.malformed(
                        id,
                        format!("parameter {} holds a value that is not a reference", k + 1),
                    )
                })
            })
            .collect()
    }

    fn boolean(&self, id: u64, e: &Entity, k: usize) -> Result<bool, StepError> {
        self.param(id, e, k)?
            .as_bool()
            .ok_or_else(|| self.malformed(id, format!("parameter {} is not .T. or .F.", k + 1)))
    }

    fn reals(&self, id: u64, list: &[Param]) -> Result<Vec<f64>, StepError> {
        list.iter()
            .map(|p| {
                p.as_real()
                    .filter(|x| x.is_finite())
                    .ok_or_else(|| self.malformed(id, "a list holds a value that is not a number"))
            })
            .collect()
    }

    // ---- Placements: which solids, in which unit, where.

    /// Every solid the shape representations hold, with its representation's units and its
    /// place, in the order of the roots by id and then as each nests its items and children.
    fn placements(&self) -> Result<Vec<(u64, Units, Affine)>, StepError> {
        let ex = self.ex;
        let mut reps: BTreeMap<u64, Rep> = BTreeMap::new();
        for (&id, inst) in &ex.data {
            if !inst.parts.iter().any(|e| {
                e.name.ends_with("SHAPE_REPRESENTATION")
                    && e.name != "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION"
            }) {
                continue;
            }
            // The part with (name, items, context): the entity itself or its REPRESENTATION.
            let Some(e) = inst
                .parts
                .iter()
                .find(|e| e.params.len() == 3 && e.params[1].as_list().is_some())
            else {
                continue;
            };
            reps.insert(
                id,
                Rep {
                    items: e.params[1]
                        .as_list()
                        .unwrap_or(&[])
                        .iter()
                        .filter_map(Param::as_ref)
                        .collect(),
                    context: e.params[2].as_ref(),
                },
            );
        }
        // Links: plain ones join representations into one shape, placed ones put a child in a
        // parent with a transformation.
        let mut plain: Vec<(u64, u64)> = Vec::new();
        let mut placed: Vec<(u64, u64, u64)> = Vec::new();
        for (&id, inst) in &ex.data {
            let rel = inst
                .part("REPRESENTATION_RELATIONSHIP")
                .or_else(|| inst.part("SHAPE_REPRESENTATION_RELATIONSHIP"));
            let Some(rel) = rel else { continue };
            let (Some(a), Some(b)) = (
                rel.params.get(2).and_then(Param::as_ref),
                rel.params.get(3).and_then(Param::as_ref),
            ) else {
                continue;
            };
            if !(reps.contains_key(&a) && reps.contains_key(&b)) {
                continue;
            }
            match inst.part("REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION") {
                Some(t) => {
                    let t = self.reference(id, t, 0)?;
                    placed.push((a, b, t));
                }
                None => plain.push((a, b)),
            }
        }
        // Mapped items place a representation as an item of another.
        let mut mapped: BTreeMap<u64, (u64, u64, u64)> = BTreeMap::new();
        for rep in reps.values() {
            for &item in &rep.items {
                let Some(inst) = ex.data.get(&item) else {
                    continue;
                };
                if let Some(e) = inst.part("MAPPED_ITEM") {
                    let map = self.reference(item, e, 1)?;
                    let target = self.reference(item, e, 2)?;
                    let m = self.simple(map, item)?;
                    if m.name != "REPRESENTATION_MAP" {
                        return Err(self.unsupported(map));
                    }
                    let origin = self.reference(map, m, 0)?;
                    let child = self.reference(map, m, 1)?;
                    mapped.insert(item, (child, origin, target));
                }
            }
        }
        // Shapes: the representations the plain links join, by their least id.
        let mut group: BTreeMap<u64, u64> = reps.keys().map(|&k| (k, k)).collect();
        fn root(g: &BTreeMap<u64, u64>, mut k: u64) -> u64 {
            while g[&k] != k {
                k = g[&k];
            }
            k
        }
        for &(a, b) in &plain {
            let (ra, rb) = (root(&group, a), root(&group, b));
            if ra != rb {
                let (lo, hi) = (ra.min(rb), ra.max(rb));
                group.insert(hi, lo);
            }
        }
        let shape_of: BTreeMap<u64, u64> = reps.keys().map(|&k| (k, root(&group, k))).collect();
        let mut members: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for (&k, &s) in &shape_of {
            members.entry(s).or_default().push(k);
        }
        let mut is_child: BTreeMap<u64, bool> = members.keys().map(|&s| (s, false)).collect();
        for &(child, _, _) in &placed {
            is_child.insert(shape_of[&child], true);
        }
        for &(child, _, _) in mapped.values() {
            if let Some(&s) = shape_of.get(&child) {
                is_child.insert(s, true);
            }
        }
        let mut out = Vec::new();
        for (&shape, &child) in &is_child {
            if !child {
                self.walk(
                    shape,
                    Affine::IDENTITY,
                    0,
                    &Walk {
                        reps: &reps,
                        members: &members,
                        shape_of: &shape_of,
                        placed: &placed,
                        mapped: &mapped,
                    },
                    &mut out,
                )?;
            }
        }
        Ok(out)
    }

    fn walk(
        &self,
        shape: u64,
        place: Affine,
        depth: usize,
        w: &Walk,
        out: &mut Vec<(u64, Units, Affine)>,
    ) -> Result<(), StepError> {
        if depth > MAX_DEPTH {
            return Err(self.malformed(shape, "its assemblies nest too deep, or in a loop"));
        }
        for &rep in &w.members[&shape] {
            let units = self.units(w.reps[&rep].context)?;
            for &item in &w.reps[&rep].items {
                let Some(inst) = self.ex.data.get(&item) else {
                    return Err(StepError::Missing {
                        id: item,
                        by: rep,
                        line: self.line(rep),
                    });
                };
                if inst.parts.len() == 1 && is_solid(&inst.parts[0].name) {
                    out.push((item, units, place));
                } else if let Some(&(child, origin, target)) = w.mapped.get(&item) {
                    let Some(&child_shape) = w.shape_of.get(&child) else {
                        return Err(self.unsupported(child));
                    };
                    let child_units = self.units(w.reps[&child].context)?;
                    let from = Affine::of(&self.frame(origin, item, child_units.mm)?);
                    let to = Affine::of(&self.frame(target, item, units.mm)?);
                    let t = place.then_after(&to.then_after(&from.inverse_rigid()));
                    self.walk(child_shape, t, depth + 1, w, out)?;
                }
            }
            // The children placed in this representation, by the link's order in the file.
            for &(child, parent, transform) in w.placed {
                if parent != rep {
                    continue;
                }
                let child_units = self.units(w.reps[&child].context)?;
                let t = self.transformation(transform, child_units.mm, units.mm)?;
                self.walk(w.shape_of[&child], place.then_after(&t), depth + 1, w, out)?;
            }
        }
        Ok(())
    }

    /// The motion of an `ITEM_DEFINED_TRANSFORMATION`: the child's frame onto the parent's.
    fn transformation(&self, id: u64, child_mm: f64, parent_mm: f64) -> Result<Affine, StepError> {
        let e = self.simple(id, id)?;
        if e.name != "ITEM_DEFINED_TRANSFORMATION" {
            return Err(self.unsupported(id));
        }
        let from = Affine::of(&self.frame(self.reference(id, e, 2)?, id, child_mm)?);
        let to = Affine::of(&self.frame(self.reference(id, e, 3)?, id, parent_mm)?);
        Ok(to.then_after(&from.inverse_rigid()))
    }

    /// The units of a representation context: its length unit in mm and its angle unit in
    /// radians, mm and radians when it states none.
    fn units(&self, context: Option<u64>) -> Result<Units, StepError> {
        let mut u = MM;
        let Some(c) = context else { return Ok(u) };
        let Some(inst) = self.ex.data.get(&c) else {
            return Ok(u);
        };
        let Some(g) = inst.part("GLOBAL_UNIT_ASSIGNED_CONTEXT") else {
            return Ok(u);
        };
        for unit in self.refs(c, g, 0)? {
            let ui = self.inst(unit, c)?;
            if ui.part("LENGTH_UNIT").is_some() {
                u.mm = self.length_unit(unit, 0)?;
            } else if ui.part("PLANE_ANGLE_UNIT").is_some() {
                u.radians = self.angle_unit(unit, 0)?;
            }
        }
        Ok(u)
    }

    fn si_prefix(&self, id: u64, si: &Entity) -> Result<f64, StepError> {
        Ok(match si.params.first().and_then(Param::as_enum) {
            None => 1.0,
            Some("MILLI") => 1e-3,
            Some("CENTI") => 1e-2,
            Some("DECI") => 1e-1,
            Some("MICRO") => 1e-6,
            Some("NANO") => 1e-9,
            Some("KILO") => 1e3,
            Some(other) => {
                return Err(self.malformed(
                    id,
                    format!("netsmith does not read the unit prefix {other}"),
                ));
            }
        })
    }

    /// Millimetres a unit of length `id`.
    fn length_unit(&self, id: u64, depth: usize) -> Result<f64, StepError> {
        if depth > 8 {
            return Err(self.malformed(id, "its unit is defined in a loop"));
        }
        let inst = self.inst(id, id)?;
        if let Some(si) = inst.part("SI_UNIT") {
            if si.params.get(1).and_then(Param::as_enum) != Some("METRE") {
                return Err(self.malformed(id, "a length unit that is not a metre"));
            }
            return Ok(self.si_prefix(id, si)? * 1000.0);
        }
        if let Some(cb) = inst.part("CONVERSION_BASED_UNIT") {
            let (value, base) = self.measure(self.reference(id, cb, 1)?, id)?;
            return Ok(value * self.length_unit(base, depth + 1)?);
        }
        Err(self.unsupported(id))
    }

    /// Radians a unit of plane angle `id`.
    fn angle_unit(&self, id: u64, depth: usize) -> Result<f64, StepError> {
        if depth > 8 {
            return Err(self.malformed(id, "its unit is defined in a loop"));
        }
        let inst = self.inst(id, id)?;
        if let Some(si) = inst.part("SI_UNIT") {
            return self.si_prefix(id, si);
        }
        if let Some(cb) = inst.part("CONVERSION_BASED_UNIT") {
            let (value, base) = self.measure(self.reference(id, cb, 1)?, id)?;
            return Ok(value * self.angle_unit(base, depth + 1)?);
        }
        Err(self.unsupported(id))
    }

    /// A measure with a unit: its value and the unit's id.
    fn measure(&self, id: u64, by: u64) -> Result<(f64, u64), StepError> {
        let inst = self.inst(id, by)?;
        let e = inst
            .parts
            .iter()
            .find(|e| e.params.len() == 2)
            .ok_or_else(|| self.unsupported(id))?;
        Ok((self.real(id, e, 0)?, self.reference(id, e, 1)?))
    }

    // ---- Geometry.

    fn point(&self, id: u64, by: u64) -> Result<P3, StepError> {
        let e = self.simple(id, by)?;
        if e.name != "CARTESIAN_POINT" {
            return Err(self.unsupported(id));
        }
        let c = self.reals(id, self.list(id, e, 1)?)?;
        if c.len() != 3 {
            return Err(self.malformed(id, "a point of a solid has three coordinates"));
        }
        Ok([c[0], c[1], c[2]])
    }

    fn direction(&self, id: u64, by: u64) -> Result<P3, StepError> {
        let e = self.simple(id, by)?;
        if e.name != "DIRECTION" {
            return Err(self.unsupported(id));
        }
        let c = self.reals(id, self.list(id, e, 1)?)?;
        if c.len() != 3 {
            return Err(self.malformed(id, "a direction of a solid has three components"));
        }
        Ok([c[0], c[1], c[2]])
    }

    fn vertex(&self, id: u64, by: u64) -> Result<P3, StepError> {
        let e = self.simple(id, by)?;
        if e.name != "VERTEX_POINT" {
            return Err(self.unsupported(id));
        }
        self.point(self.reference(id, e, 1)?, id)
    }

    /// An `AXIS2_PLACEMENT_3D`, its origin scaled by `mm` (1 to keep the file's unit).
    fn frame(&self, id: u64, by: u64, mm: f64) -> Result<Frame, StepError> {
        let e = self.simple(id, by)?;
        if e.name != "AXIS2_PLACEMENT_3D" {
            return Err(self.unsupported(id));
        }
        let o = self.point(self.reference(id, e, 1)?, id)?;
        let opt = |k: usize| -> Result<Option<P3>, StepError> {
            match self.param(id, e, k)? {
                Param::Null => Ok(None),
                p => {
                    let r = p
                        .as_ref()
                        .ok_or_else(|| self.malformed(id, "a direction is not a reference"))?;
                    Ok(Some(self.direction(r, id)?))
                }
            }
        };
        let (axis, reference) = (opt(2)?, opt(3)?);
        Frame::new(o.map(|c| c * mm), axis, reference)
            .ok_or_else(|| self.malformed(id, "its directions are zero or parallel"))
    }

    fn positive(&self, id: u64, e: &Entity, k: usize) -> Result<f64, StepError> {
        let r = self.real(id, e, k)?;
        if r > 0.0 {
            Ok(r)
        } else {
            Err(self.malformed(id, format!("parameter {} is not above zero", k + 1)))
        }
    }

    fn curve(&self, id: u64, by: u64, depth: usize) -> Result<Curve, StepError> {
        if depth > 8 {
            return Err(self.malformed(id, "its curve is defined in a loop"));
        }
        let inst = self.inst(id, by)?;
        if inst.part("B_SPLINE_CURVE").is_some()
            || (!inst.is_complex()
                && matches!(
                    inst.parts[0].name.as_str(),
                    "B_SPLINE_CURVE_WITH_KNOTS"
                        | "BEZIER_CURVE"
                        | "UNIFORM_CURVE"
                        | "QUASI_UNIFORM_CURVE"
                ))
        {
            return Ok(Curve::BSpline(self.bspline_curve(id, inst)?));
        }
        if inst.is_complex() {
            return Err(self.unsupported(id));
        }
        let e = &inst.parts[0];
        match e.name.as_str() {
            "LINE" => {
                let p = self.point(self.reference(id, e, 1)?, id)?;
                let v = self.reference(id, e, 2)?;
                let ve = self.simple(v, id)?;
                if ve.name != "VECTOR" {
                    return Err(self.unsupported(v));
                }
                let d = self.direction(self.reference(v, ve, 1)?, v)?;
                let d = unit(d).ok_or_else(|| self.malformed(v, "its direction has no length"))?;
                Ok(Curve::Line { p, d })
            }
            "CIRCLE" => Ok(Curve::Circle {
                f: self.frame(self.reference(id, e, 1)?, id, 1.0)?,
                r: self.positive(id, e, 2)?,
            }),
            "ELLIPSE" => Ok(Curve::Ellipse {
                f: self.frame(self.reference(id, e, 1)?, id, 1.0)?,
                a: self.positive(id, e, 2)?,
                b: self.positive(id, e, 3)?,
            }),
            "SURFACE_CURVE" | "SEAM_CURVE" | "INTERSECTION_CURVE" | "TRIMMED_CURVE" => {
                self.curve(self.reference(id, e, 1)?, id, depth + 1)
            }
            "POLYLINE" => {
                let pts = self
                    .refs(id, e, 1)?
                    .into_iter()
                    .map(|p| self.point(p, id))
                    .collect::<Result<Vec<_>, _>>()?;
                if pts.len() < 2 {
                    return Err(self.malformed(id, "a polyline has two points or more"));
                }
                Ok(Curve::Polyline(pts))
            }
            _ => Err(self.unsupported(id)),
        }
    }

    /// The knots of a B-spline: from `mults` and `knots` when written, else as a Bézier, a
    /// uniform or a quasi-uniform one makes them.
    fn knots(
        &self,
        id: u64,
        degree: usize,
        count: usize,
        written: Option<(&[Param], &[Param])>,
        kind: &str,
    ) -> Result<Knots, StepError> {
        let knots = match written {
            Some((mults, values)) => {
                let values = self.reals(id, values)?;
                if mults.len() != values.len() {
                    return Err(self.malformed(id, "its knots and multiplicities differ in number"));
                }
                let mut out = Vec::new();
                for (m, v) in mults.iter().zip(values) {
                    let m = m
                        .as_int()
                        .filter(|&m| (1..=64).contains(&m))
                        .ok_or_else(|| {
                            self.malformed(id, "a knot multiplicity is not a small whole number")
                        })?;
                    out.extend(std::iter::repeat_n(v, m as usize));
                }
                out
            }
            None => {
                let p = degree;
                let n = count;
                match kind {
                    "UNIFORM" => (0..n + p + 1).map(|i| i as f64 - p as f64).collect(),
                    "QUASI_UNIFORM" => {
                        let mut k = vec![0.0; p + 1];
                        k.extend((1..n - p).map(|i| i as f64));
                        k.extend(std::iter::repeat_n((n - p) as f64, p + 1));
                        k
                    }
                    _ => {
                        // Bézier: pieces of degree p sharing their ends.
                        if p == 0 || !(n - 1).is_multiple_of(p) {
                            return Err(
                                self.malformed(id, "a Bézier's points do not make whole pieces")
                            );
                        }
                        let pieces = (n - 1) / p;
                        let mut k = vec![0.0; p + 1];
                        for i in 1..pieces {
                            k.extend(std::iter::repeat_n(i as f64, p));
                        }
                        k.extend(std::iter::repeat_n(pieces as f64, p + 1));
                        k
                    }
                }
            }
        };
        if knots.len() != count + degree + 1 {
            return Err(self.malformed(
                id,
                format!(
                    "{} knots for {count} points of degree {degree}: it needs {}",
                    knots.len(),
                    count + degree + 1
                ),
            ));
        }
        if knots.windows(2).any(|w| w[1] < w[0]) {
            return Err(self.malformed(id, "its knots go down"));
        }
        let k = Knots { degree, knots };
        let (a, b) = k.domain();
        if b <= a {
            return Err(self.malformed(id, "its knots leave no range"));
        }
        Ok(k)
    }

    fn degree(&self, id: u64, p: &Param) -> Result<usize, StepError> {
        p.as_int()
            .filter(|&d| (1..=25).contains(&d))
            .map(|d| d as usize)
            .ok_or_else(|| self.malformed(id, "its degree is not a whole number from 1 to 25"))
    }

    fn weights(&self, id: u64, w: &[Param]) -> Result<Vec<f64>, StepError> {
        let w = self.reals(id, w)?;
        if w.iter().any(|&x| x <= 0.0) {
            return Err(self.malformed(id, "a weight is not above zero"));
        }
        Ok(w)
    }

    fn bspline_curve(&self, id: u64, inst: &Instance) -> Result<BSplineCurve, StepError> {
        // The B_SPLINE_CURVE attributes: (degree, points, form, closed, self-intersect), after
        // the name in a simple instance.
        let (base, kind): (&[Param], &str) = match inst.part("B_SPLINE_CURVE") {
            Some(e) => (&e.params, ""),
            None => (
                inst.parts[0].params.get(1..).unwrap_or(&[]),
                inst.parts[0].name.as_str(),
            ),
        };
        let degree = self.degree(id, base.first().unwrap_or(&Param::Null))?;
        let pts = base
            .get(1)
            .and_then(Param::as_list)
            .ok_or_else(|| self.malformed(id, "it has no list of points"))?;
        let points = pts
            .iter()
            .map(|p| {
                let r = p
                    .as_ref()
                    .ok_or_else(|| self.malformed(id, "a control point is not a reference"))?;
                self.point(r, id)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if points.len() < degree + 1 {
            return Err(self.malformed(id, "it has fewer points than its degree needs"));
        }
        let written = if let Some(k) = inst.part("B_SPLINE_CURVE_WITH_KNOTS") {
            let off = if inst.is_complex() { 0 } else { 6 };
            Some((self.list(id, k, off)?, self.list(id, k, off + 1)?))
        } else {
            None
        };
        let kind = if inst.part("UNIFORM_CURVE").is_some() || kind == "UNIFORM_CURVE" {
            "UNIFORM"
        } else if inst.part("QUASI_UNIFORM_CURVE").is_some() || kind == "QUASI_UNIFORM_CURVE" {
            "QUASI_UNIFORM"
        } else {
            "BEZIER"
        };
        let knots = self.knots(id, degree, points.len(), written, kind)?;
        let weights = match inst.part("RATIONAL_B_SPLINE_CURVE") {
            Some(r) => {
                let w = self.weights(id, self.list(id, r, 0)?)?;
                if w.len() != points.len() {
                    return Err(self.malformed(id, "it has not one weight a point"));
                }
                Some(w)
            }
            None => None,
        };
        Ok(BSplineCurve {
            knots,
            points,
            weights,
        })
    }

    fn bspline_surface(&self, id: u64, inst: &Instance) -> Result<BSplineSurface, StepError> {
        // (u degree, v degree, points, form, u closed, v closed, self-intersect).
        let (base, kind): (&[Param], &str) = match inst.part("B_SPLINE_SURFACE") {
            Some(e) => (&e.params, ""),
            None => (
                inst.parts[0].params.get(1..).unwrap_or(&[]),
                inst.parts[0].name.as_str(),
            ),
        };
        let du = self.degree(id, base.first().unwrap_or(&Param::Null))?;
        let dv = self.degree(id, base.get(1).unwrap_or(&Param::Null))?;
        let rows = base
            .get(2)
            .and_then(Param::as_list)
            .ok_or_else(|| self.malformed(id, "it has no grid of points"))?;
        let mut points = Vec::with_capacity(rows.len());
        for row in rows {
            let row = row
                .as_list()
                .ok_or_else(|| self.malformed(id, "a row of its points is not a list"))?;
            points.push(
                row.iter()
                    .map(|p| {
                        let r = p.as_ref().ok_or_else(|| {
                            self.malformed(id, "a control point is not a reference")
                        })?;
                        self.point(r, id)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        let nu = points.len();
        let nv = points.first().map_or(0, Vec::len);
        if nu < du + 1 || nv < dv + 1 || points.iter().any(|r| r.len() != nv) {
            return Err(self.malformed(
                id,
                "its grid of points is not whole, or too small for its degrees",
            ));
        }
        let (wu, wv) = if let Some(k) = inst.part("B_SPLINE_SURFACE_WITH_KNOTS") {
            let off = if inst.is_complex() { 0 } else { 8 };
            (
                Some((self.list(id, k, off)?, self.list(id, k, off + 2)?)),
                Some((self.list(id, k, off + 1)?, self.list(id, k, off + 3)?)),
            )
        } else {
            (None, None)
        };
        let kind = if inst.part("UNIFORM_SURFACE").is_some() || kind == "UNIFORM_SURFACE" {
            "UNIFORM"
        } else if inst.part("QUASI_UNIFORM_SURFACE").is_some() || kind == "QUASI_UNIFORM_SURFACE" {
            "QUASI_UNIFORM"
        } else {
            "BEZIER"
        };
        let u = self.knots(id, du, nu, wu, kind)?;
        let v = self.knots(id, dv, nv, wv, kind)?;
        let weights = match inst.part("RATIONAL_B_SPLINE_SURFACE") {
            Some(r) => {
                let rows = self.list(id, r, 0)?;
                let w = rows
                    .iter()
                    .map(|row| {
                        let row = row
                            .as_list()
                            .ok_or_else(|| self.malformed(id, "a row of weights is not a list"))?;
                        self.weights(id, row)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if w.len() != nu || w.iter().any(|r| r.len() != nv) {
                    return Err(self.malformed(id, "it has not one weight a point"));
                }
                Some(w)
            }
            None => None,
        };
        Ok(BSplineSurface {
            u,
            v,
            points,
            weights,
        })
    }

    fn surface(&self, id: u64, by: u64, units: Units) -> Result<Surface, StepError> {
        let inst = self.inst(id, by)?;
        if inst.part("B_SPLINE_SURFACE").is_some()
            || (!inst.is_complex()
                && matches!(
                    inst.parts[0].name.as_str(),
                    "B_SPLINE_SURFACE_WITH_KNOTS"
                        | "BEZIER_SURFACE"
                        | "UNIFORM_SURFACE"
                        | "QUASI_UNIFORM_SURFACE"
                ))
        {
            return Ok(Surface::BSpline(self.bspline_surface(id, inst)?));
        }
        if inst.is_complex() {
            return Err(self.unsupported(id));
        }
        let e = &inst.parts[0];
        let frame = || self.frame(self.reference(id, e, 1)?, id, 1.0);
        match e.name.as_str() {
            "PLANE" => Ok(Surface::Plane(frame()?)),
            "CYLINDRICAL_SURFACE" => Ok(Surface::Cylinder {
                f: frame()?,
                r: self.positive(id, e, 2)?,
            }),
            "CONICAL_SURFACE" => {
                let r = self.real(id, e, 2)?;
                let a = self.real(id, e, 3)? * units.radians;
                if r < 0.0 || !(a > 0.0 && a < std::f64::consts::FRAC_PI_2) {
                    return Err(self.malformed(
                        id,
                        "its radius is below zero or its angle is not between 0 and 90 degrees",
                    ));
                }
                let (s, c) = crate::mesh::trig::sin_cos(a);
                Ok(Surface::Cone {
                    f: frame()?,
                    r,
                    tan: s / c,
                })
            }
            "SPHERICAL_SURFACE" => Ok(Surface::Sphere {
                f: frame()?,
                r: self.positive(id, e, 2)?,
            }),
            "TOROIDAL_SURFACE" => Ok(Surface::Torus {
                f: frame()?,
                major: self.positive(id, e, 2)?,
                minor: self.positive(id, e, 3)?,
            }),
            _ => Err(self.unsupported(id)),
        }
    }

    // ---- Topology.

    fn edge(&mut self, id: u64, by: u64) -> Result<usize, StepError> {
        if let Some(&k) = self.edges.get(&id) {
            return Ok(k);
        }
        let e = self.simple(id, by)?;
        if e.name != "EDGE_CURVE" {
            return Err(self.unsupported(id));
        }
        let start = self.vertex(self.reference(id, e, 1)?, id)?;
        let end = self.vertex(self.reference(id, e, 2)?, id)?;
        let curve = self.curve(self.reference(id, e, 3)?, id, 0)?;
        let same_sense = self.boolean(id, e, 4)?;
        let k = self.model.edges.len();
        self.model.edges.push(Edge {
            id,
            line: self.line(id),
            start,
            end,
            curve,
            same_sense,
        });
        self.edges.insert(id, k);
        Ok(k)
    }

    /// An `ORIENTED_EDGE` (perhaps of another): the edge and whether it is run forward.
    fn oriented(&mut self, id: u64, by: u64, depth: usize) -> Result<Use, StepError> {
        if depth > 8 {
            return Err(self.malformed(id, "its edges refer to each other in a loop"));
        }
        let e = self.simple(id, by)?;
        match e.name.as_str() {
            "ORIENTED_EDGE" => {
                let inner = self.reference(id, e, 3)?;
                let forward = self.boolean(id, e, 4)?;
                let u = self.oriented(inner, id, depth + 1)?;
                Ok(Use {
                    edge: u.edge,
                    forward: u.forward == forward,
                })
            }
            "EDGE_CURVE" => Ok(Use {
                edge: self.edge(id, by)?,
                forward: true,
            }),
            _ => Err(self.unsupported(id)),
        }
    }

    fn face_loop(&mut self, id: u64, by: u64) -> Result<Loop, StepError> {
        let e = self.simple(id, by)?.clone();
        match e.name.as_str() {
            "EDGE_LOOP" => {
                let mut uses = Vec::new();
                for r in self.refs(id, &e, 1)? {
                    uses.push(self.oriented(r, id, 0)?);
                }
                if uses.is_empty() {
                    return Err(self.malformed(id, "a loop has no edges"));
                }
                Ok(Loop::Edges(uses))
            }
            "VERTEX_LOOP" => Ok(Loop::Vertex(self.vertex(self.reference(id, &e, 1)?, id)?)),
            "POLY_LOOP" => {
                let pts = self
                    .refs(id, &e, 1)?
                    .into_iter()
                    .map(|p| self.point(p, id))
                    .collect::<Result<Vec<_>, _>>()?;
                if pts.len() < 3 {
                    return Err(self.malformed(id, "a polygon has three points or more"));
                }
                Ok(Loop::Poly(pts))
            }
            _ => Err(self.unsupported(id)),
        }
    }

    fn face(&mut self, id: u64, by: u64, units: Units) -> Result<usize, StepError> {
        if let Some(&k) = self.faces.get(&id) {
            return Ok(k);
        }
        let inst = self.inst(id, by)?;
        let line = inst.line;
        let e = self.simple(id, by)?.clone();
        if !matches!(e.name.as_str(), "ADVANCED_FACE" | "FACE_SURFACE") {
            return Err(self.unsupported(id));
        }
        let surface_id = self.reference(id, &e, 2)?;
        let surface = self.surface(surface_id, id, units)?;
        let same_sense = self.boolean(id, &e, 3)?;
        let mut bounds = Vec::new();
        for b in self.refs(id, &e, 1)? {
            let be = self.simple(b, id)?.clone();
            if !matches!(be.name.as_str(), "FACE_BOUND" | "FACE_OUTER_BOUND") {
                return Err(self.unsupported(b));
            }
            let lp = self.face_loop(self.reference(b, &be, 1)?, b)?;
            bounds.push((lp, self.boolean(b, &be, 2)?));
        }
        let colour = self.colours.get(&id).copied();
        let k = self.model.faces.len();
        self.model.faces.push(Face {
            id,
            line,
            surface,
            same_sense,
            bounds,
            colour,
        });
        self.faces.insert(id, k);
        Ok(k)
    }

    fn shell(&mut self, id: u64, by: u64, units: Units, depth: usize) -> Result<Shell, StepError> {
        if depth > 4 {
            return Err(self.malformed(id, "its shells refer to each other in a loop"));
        }
        let e = self.simple(id, by)?.clone();
        match e.name.as_str() {
            "CLOSED_SHELL" | "OPEN_SHELL" => {
                let mut faces = Vec::new();
                for f in self.refs(id, &e, 1)? {
                    faces.push(self.face(f, id, units)?);
                }
                Ok(Shell {
                    id,
                    faces,
                    closed: e.name == "CLOSED_SHELL",
                    reversed: false,
                    colour: self.colours.get(&id).copied(),
                })
            }
            "ORIENTED_CLOSED_SHELL" | "ORIENTED_OPEN_SHELL" => {
                let inner = self.reference(id, &e, 2)?;
                let forward = self.boolean(id, &e, 3)?;
                let mut s = self.shell(inner, id, units, depth + 1)?;
                s.reversed ^= !forward;
                Ok(s)
            }
            _ => Err(self.unsupported(id)),
        }
    }

    fn solid(&mut self, id: u64, units: Units) -> Result<usize, StepError> {
        if let Some(&k) = self.solids.get(&id) {
            return Ok(k);
        }
        let inst = self.inst(id, id)?;
        let line = inst.line;
        let e = self.simple(id, id)?.clone();
        let mut shells = Vec::new();
        match e.name.as_str() {
            "MANIFOLD_SOLID_BREP" | "FACETED_BREP" => {
                shells.push(self.shell(self.reference(id, &e, 1)?, id, units, 0)?);
            }
            "BREP_WITH_VOIDS" => {
                shells.push(self.shell(self.reference(id, &e, 1)?, id, units, 0)?);
                for v in self.refs(id, &e, 2)? {
                    shells.push(self.shell(v, id, units, 0)?);
                }
            }
            "SHELL_BASED_SURFACE_MODEL" => {
                for s in self.refs(id, &e, 1)? {
                    shells.push(self.shell(s, id, units, 0)?);
                }
            }
            _ => return Err(self.unsupported(id)),
        }
        let name = e
            .params
            .first()
            .and_then(Param::as_str)
            .unwrap_or("")
            .to_string();
        let k = self.model.solids.len();
        self.model.solids.push(Solid {
            id,
            line,
            name,
            shells,
            colour: self.colours.get(&id).copied(),
            mm: units.mm,
        });
        self.solids.insert(id, k);
        Ok(k)
    }
}

/// The links between representations, for [`Reader::walk`].
struct Walk<'a> {
    reps: &'a BTreeMap<u64, Rep>,
    members: &'a BTreeMap<u64, Vec<u64>>,
    shape_of: &'a BTreeMap<u64, u64>,
    placed: &'a [(u64, u64, u64)],
    mapped: &'a BTreeMap<u64, (u64, u64, u64)>,
}
