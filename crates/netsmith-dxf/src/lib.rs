//! A reader for the outline of a board in an ASCII DXF file (ADR 0045, step C0 piece 3).
//!
//! [`read_outline`] takes the text of a DXF file, from the R12 to the R2018 formats, and
//! returns the board edge as one closed loop of lines and arcs, the loops inside it as cutouts,
//! and circles on a holes layer as mounting holes. It reads `LINE`, `ARC`, `CIRCLE`,
//! `LWPOLYLINE` (closed flag and bulges), `POLYLINE` with `VERTEX`, `SPLINE` and `ELLIPSE`
//! (approximated by chords that stay within [`Options::curve_tolerance`]), and `INSERT` of a
//! block with translation, rotation, scale and mirroring. Text, dimensions, hatches and other
//! entities are not read; the ones on a layer that was used are named in the warnings.
//!
//! Every length in the result is in millimetres, whatever the file's units were, and the
//! frame is the board's: the origin is the top left corner of the outline's bounding box, x
//! grows to the right and y downward (a DXF drawing has y up, so the drawing is mirrored top
//! to bottom; [`Outline::origin`] says where the new origin was in the drawing).
//!
//! What it refuses, with a message that names the numbers: a binary file; a file with no units
//! (`$INSUNITS` is 0 or absent) unless [`Options::units`] says them; layers it cannot choose
//! between (the message lists them with their entity counts); loops with a gap (the gaps are
//! listed), a branch, or a crossing; and an outline that is not exactly one outer loop with
//! cutouts inside it.

mod chain;
mod entities;
mod geom;
mod parse;

use std::collections::BTreeMap;
use std::fmt;

pub use geom::ArcGeom;
use geom::{P, edge_bounds, edge_length};

/// The unit a drawing is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Units {
    Millimetres,
    Centimetres,
    Metres,
    Inches,
}

impl Units {
    /// Millimetres in one of the unit.
    pub fn millimetres(self) -> f64 {
        match self {
            Units::Millimetres => 1.0,
            Units::Centimetres => 10.0,
            Units::Metres => 1000.0,
            Units::Inches => 25.4,
        }
    }

    /// The unit a command-line word names: `mm`, `cm`, `m`, `in` or `inch`.
    pub fn parse(word: &str) -> Option<Units> {
        match word.to_ascii_lowercase().as_str() {
            "mm" => Some(Units::Millimetres),
            "cm" => Some(Units::Centimetres),
            "m" => Some(Units::Metres),
            "in" | "inch" | "inches" => Some(Units::Inches),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Units::Millimetres => "mm",
            Units::Centimetres => "cm",
            Units::Metres => "m",
            Units::Inches => "inch",
        }
    }
}

/// What to read and how.
#[derive(Debug, Clone)]
pub struct Options {
    /// The layers of the board edge and its cutouts. Empty: the one layer with lines, arcs or
    /// polylines that is not a holes layer; if there are several, that is an error listing
    /// them. Layer names are compared without regard to case.
    pub outline_layers: Vec<String>,
    /// The layers whose circles are mounting holes.
    pub holes_layers: Vec<String>,
    /// The units to take the drawing to be in, if the caller states them. A file that states
    /// its own is overruled, with a warning if they differ.
    pub units: Option<Units>,
    /// Ends of two edges closer than this (mm) are one point. Default 0.001.
    pub join_tolerance: f64,
    /// How far (mm) a chord may be from the spline or ellipse it stands for. Default 0.005.
    pub curve_tolerance: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            outline_layers: Vec::new(),
            holes_layers: Vec::new(),
            units: None,
            join_tolerance: 1e-3,
            curve_tolerance: 5e-3,
        }
    }
}

/// A layer of the file and what is drawn on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerInfo {
    pub name: String,
    /// The entities by type, after blocks are inserted: `LINE` 8, `ARC` 4.
    pub counts: BTreeMap<String, usize>,
}

impl LayerInfo {
    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    fn describe(&self) -> String {
        let parts: Vec<String> = self
            .counts
            .iter()
            .map(|(k, n)| format!("{n} {k}"))
            .collect();
        format!("`{}` ({})", self.name, parts.join(", "))
    }

    /// Whether it has anything that can be a board edge, a cutout or a hole.
    fn has_geometry(&self) -> bool {
        self.counts.keys().any(|k| GEOMETRY.contains(&k.as_str()))
    }
}

const GEOMETRY: [&str; 7] = [
    "LINE",
    "ARC",
    "CIRCLE",
    "LWPOLYLINE",
    "POLYLINE",
    "SPLINE",
    "ELLIPSE",
];

/// A gap in a chain of edges that should be a closed loop.
#[derive(Debug, Clone, PartialEq)]
pub struct Gap {
    /// An open end, in millimetres in the drawing's own frame (y up).
    pub at: P,
    /// The nearest other open end, if there is one.
    pub nearest: Option<P>,
    /// The distance to it.
    pub distance: Option<f64>,
}

/// Why a file could not be read.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// Binary DXF.
    Binary,
    /// The text is not DXF.
    Syntax { line: usize, message: String },
    /// Something the reader does not do; the message says what to export instead.
    Unsupported(String),
    /// No units in the file and none given.
    Unitless,
    /// `$INSUNITS` is a unit this does not know.
    UnknownUnits(i32),
    /// A layer that was asked for is not in the file.
    UnknownLayer {
        asked: String,
        layers: Vec<LayerInfo>,
    },
    /// There is more than one layer that could be the outline, and none was named.
    AmbiguousLayer { layers: Vec<LayerInfo> },
    /// Nothing to read on the layers.
    NoGeometry { layers: Vec<LayerInfo> },
    /// Chains of edges that do not close.
    Gaps { gaps: Vec<Gap>, tolerance: f64 },
    /// Three or more edge ends meet at one point.
    Branch { at: P, ends: usize },
    /// A loop crosses or touches itself, or two loops cross or touch.
    Crossing { what: String },
    /// No loop at all.
    NoOutline,
    /// More than one loop that is not inside another.
    SeveralOuterLoops { loops: Vec<String> },
    /// A loop inside a cutout.
    Island { inner: String, within: String },
    /// A loop on a holes layer that is not a circle.
    HoleNotCircle { loop_: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |layers: &[LayerInfo]| {
            layers
                .iter()
                .map(LayerInfo::describe)
                .collect::<Vec<_>>()
                .join("; ")
        };
        match self {
            Error::Binary => write!(
                f,
                "this is a binary DXF file, which is not read; save it as an ASCII DXF (in most CAD tools \
                 \"DXF\" with the ASCII option, or an older version such as R12 or R2000)"
            ),
            Error::Syntax { line, message } => write!(f, "line {line}: {message}"),
            Error::Unsupported(message) => f.write_str(message),
            Error::Unitless => write!(
                f,
                "the file does not say what its units are ($INSUNITS is absent or 0); say them with \
                 --units mm, cm, m or inch"
            ),
            Error::UnknownUnits(code) => write!(
                f,
                "the file's $INSUNITS is {code}, which is not millimetres (4), centimetres (5), metres (6) \
                 or inches (1); say the units with --units, or export in millimetres"
            ),
            Error::UnknownLayer { asked, layers } => write!(
                f,
                "the file has no layer `{asked}`; its layers: {}",
                list(layers)
            ),
            Error::AmbiguousLayer { layers } => write!(
                f,
                "cannot tell which layer is the board outline; choose with --layer NAME (and --holes-layer NAME \
                 for the circles that are mounting holes). The layers: {}",
                list(layers)
            ),
            Error::NoGeometry { layers } => write!(
                f,
                "there are no lines, arcs, polylines or circles on the chosen layers; the layers of the file: {}",
                list(layers)
            ),
            Error::Gaps { gaps, tolerance } => {
                write!(
                    f,
                    "the outline does not close: {} open end{} (ends closer than {tolerance} mm are joined)",
                    gaps.len(),
                    if gaps.len() == 1 { "" } else { "s" }
                )?;
                for g in gaps.iter().take(8) {
                    match (g.nearest, g.distance) {
                        (Some(n), Some(d)) => write!(
                            f,
                            "; the end at ({:.4}, {:.4}) is {d:.4} mm from the end at ({:.4}, {:.4})",
                            g.at.0, g.at.1, n.0, n.1
                        )?,
                        _ => write!(
                            f,
                            "; the end at ({:.4}, {:.4}) has no other end",
                            g.at.0, g.at.1
                        )?,
                    }
                }
                if gaps.len() > 8 {
                    write!(f, "; and {} more", gaps.len() - 8)?;
                }
                write!(f, " (positions in mm in the drawing, y up)")
            }
            Error::Branch { at, ends } => write!(
                f,
                "{ends} edges meet at ({:.4}, {:.4}) (mm in the drawing); a loop passes through a point once",
                at.0, at.1
            ),
            Error::Crossing { what } => f.write_str(what),
            Error::NoOutline => write!(f, "there is no closed loop on the outline layers"),
            Error::SeveralOuterLoops { loops } => write!(
                f,
                "{} closed loops are not inside another, and a board has one outline: {}; \
                 put one board on the layer, or choose another layer",
                loops.len(),
                loops.join("; ")
            ),
            Error::Island { inner, within } => write!(
                f,
                "a loop ({inner}) lies inside a cutout ({within}), which would be an island of board \
                 inside a hole; that is not supported"
            ),
            Error::HoleNotCircle { loop_ } => write!(
                f,
                "a shape on the holes layer is not a circle ({loop_}); holes are circles"
            ),
        }
    }
}

impl std::error::Error for Error {}

/// One corner of a [`Polygon`]: where an edge starts, and the bulge of that edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub x: f64,
    pub y: f64,
    /// The edge from this vertex to the next is straight if 0, and an arc of
    /// `4 atan(bulge)` radians otherwise: positive turns counterclockwise as seen in the
    /// drawing (y up), and so clockwise on the board's screen (y down).
    pub bulge: f64,
}

/// A closed loop of straight edges and arcs.
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    pub vertices: Vec<Vertex>,
}

/// An axis-aligned rectangle with four equal rounded corners.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundedRect {
    /// The corner nearest the origin.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}

impl Polygon {
    /// The edges: start, end and bulge.
    pub fn edges(&self) -> impl Iterator<Item = (P, P, f64)> + '_ {
        let n = self.vertices.len();
        (0..n).map(move |i| {
            let (a, b) = (&self.vertices[i], &self.vertices[(i + 1) % n]);
            ((a.x, a.y), (b.x, b.y), a.bulge)
        })
    }

    /// The least and greatest corners of the loop, exact for arcs.
    pub fn bounds(&self) -> (P, P) {
        let mut min = (f64::INFINITY, f64::INFINITY);
        let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (a, b, bulge) in self.edges() {
            let (lo, hi) = edge_bounds(a, b, bulge);
            min = (min.0.min(lo.0), min.1.min(lo.1));
            max = (max.0.max(hi.0), max.1.max(hi.1));
        }
        (min, max)
    }

    /// The area enclosed, in square millimetres.
    pub fn area(&self) -> f64 {
        chain::shoelace(&self.vertices).abs()
    }

    /// The number of arcs and of straight edges.
    pub fn edge_counts(&self) -> (usize, usize) {
        let arcs = self.vertices.iter().filter(|v| v.bulge != 0.0).count();
        (arcs, self.vertices.len() - arcs)
    }

    /// A short description for messages: bounding box and edges.
    pub fn describe(&self) -> String {
        let (lo, hi) = self.bounds();
        let (arcs, lines) = self.edge_counts();
        format!(
            "{:.3} x {:.3} mm at ({:.3}, {:.3}), {} edges: {lines} straight, {arcs} arcs",
            hi.0 - lo.0,
            hi.1 - lo.1,
            lo.0,
            lo.1,
            self.vertices.len()
        )
    }

    /// The loop as a rectangle with equal rounded corners, if it is one: a rectangle, a
    /// rounded rectangle, a slot (radius half the short side) or a circle (a square with a
    /// radius of half its side). Every edge must lie on the boundary of that shape to within
    /// 2 micrometres, and the edges must be as long as that boundary.
    pub fn as_rounded_rect(&self) -> Option<RoundedRect> {
        const TOL: f64 = 2e-3;
        let (lo, hi) = self.bounds();
        let (w, h) = (hi.0 - lo.0, hi.1 - lo.1);
        if w <= TOL || h <= TOL {
            return None;
        }
        let mut r = 0.0;
        for (a, b, bulge) in self.edges() {
            if let Some(arc) = ArcGeom::of(a, b, bulge) {
                r = arc.radius;
                break;
            }
        }
        if r > w.min(h) / 2.0 + TOL {
            return None;
        }
        let r = r.min(w.min(h) / 2.0);
        let c = ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0);
        let (ix, iy) = (w / 2.0 - r, h / 2.0 - r);
        // The signed distance from the boundary of the rounded rectangle.
        let sdf = |p: P| {
            let (qx, qy) = ((p.0 - c.0).abs() - ix, (p.1 - c.1).abs() - iy);
            qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
        };
        let mut length = 0.0;
        for (a, b, bulge) in self.edges() {
            length += edge_length(a, b, bulge);
            let arc = ArcGeom::of(a, b, bulge);
            for k in 0..=8 {
                let t = f64::from(k) / 8.0;
                let p = match arc {
                    Some(arc) => arc.point_at(t),
                    None => (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t),
                };
                if sdf(p).abs() > TOL {
                    return None;
                }
            }
        }
        let perimeter = 2.0 * (w - 2.0 * r) + 2.0 * (h - 2.0 * r) + std::f64::consts::TAU * r;
        if (length - perimeter).abs() > 5e-3 + 1e-4 * perimeter {
            return None;
        }
        Some(RoundedRect {
            x: lo.0,
            y: lo.1,
            width: w,
            height: h,
            radius: r,
        })
    }

    /// The loop as a circle (its centre and diameter), if it is one.
    pub fn as_circle(&self) -> Option<(P, f64)> {
        let rr = self.as_rounded_rect()?;
        ((rr.width - rr.height).abs() <= 2e-3 && (rr.radius - rr.width / 2.0).abs() <= 2e-3)
            .then(|| ((rr.x + rr.width / 2.0, rr.y + rr.height / 2.0), rr.width))
    }
}

/// A mounting hole: a circle on a holes layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hole {
    pub centre: P,
    pub diameter: f64,
}

/// What a DXF file says of a board.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    /// The board edge. Its vertices run clockwise on the screen (y down).
    pub outer: Polygon,
    /// The loops inside it, by their top left corner (y, then x).
    pub cutouts: Vec<Polygon>,
    /// The circles of the holes layers, by position (y, then x).
    pub holes: Vec<Hole>,
    /// The units the drawing was taken to be in.
    pub units: Units,
    /// Whether the file said so (`$INSUNITS`) and the caller did not overrule it.
    pub units_from_file: bool,
    /// Where the board's origin is in the drawing, in mm: the outline's bounding box's left
    /// edge and top edge (the greatest y). A drawing point `(x, y)` is at
    /// `(x - origin.0, origin.1 - y)` on the board.
    pub origin: P,
    /// The layers of the file with their entity counts.
    pub layers: Vec<LayerInfo>,
    /// The layers used for the outline and for the holes.
    pub outline_layers: Vec<String>,
    pub holes_layers: Vec<String>,
    /// Things worth saying that did not stop the read: entities that were not read, layers
    /// that were not used, units overruled.
    pub warnings: Vec<String>,
}

/// Read the board outline, cutouts and holes from the text of an ASCII DXF file.
pub fn read_outline(text: &str, options: &Options) -> Result<Outline, Error> {
    let dxf = parse::parse(text)?;
    let mut warnings = Vec::new();

    // The units, before anything is measured.
    let from_file = match dxf.insunits {
        None | Some(0) => None,
        Some(1) => Some(Units::Inches),
        Some(4) => Some(Units::Millimetres),
        Some(5) => Some(Units::Centimetres),
        Some(6) => Some(Units::Metres),
        Some(other) => {
            if options.units.is_none() {
                return Err(Error::UnknownUnits(other));
            }
            None
        }
    };
    let (units, units_from_file) = match (options.units, from_file) {
        (Some(given), Some(file)) if given != file => {
            warnings.push(format!(
                "the file says its units are {}, and --units says {}: using {}",
                file.name(),
                given.name(),
                given.name()
            ));
            (given, false)
        }
        (Some(given), Some(_)) => (given, true),
        (Some(given), None) => (given, false),
        (None, Some(file)) => (file, true),
        (None, None) => return Err(Error::Unitless),
    };

    let drawing = entities::expand(&dxf, units.millimetres(), options.curve_tolerance)?;
    let counts = entities::count_by_layer(&drawing);
    let layers: Vec<LayerInfo> = counts
        .into_iter()
        .map(|(name, counts)| LayerInfo { name, counts })
        .collect();

    // Which layers.
    let find = |asked: &str| -> Result<String, Error> {
        layers
            .iter()
            .find(|l| l.name.eq_ignore_ascii_case(asked))
            .map(|l| l.name.clone())
            .ok_or_else(|| Error::UnknownLayer {
                asked: asked.to_string(),
                layers: layers.clone(),
            })
    };
    let holes_layers = options
        .holes_layers
        .iter()
        .map(|l| find(l))
        .collect::<Result<Vec<_>, _>>()?;
    let mut outline_layers = options
        .outline_layers
        .iter()
        .map(|l| find(l))
        .collect::<Result<Vec<_>, _>>()?;
    if outline_layers.is_empty() {
        let candidates: Vec<&LayerInfo> = layers
            .iter()
            .filter(|l| l.has_geometry() && !holes_layers.contains(&l.name))
            .collect();
        match candidates.as_slice() {
            [one] => outline_layers.push(one.name.clone()),
            [] => return Err(Error::NoGeometry { layers }),
            _ => return Err(Error::AmbiguousLayer { layers }),
        }
    }
    if let Some(both) = outline_layers.iter().find(|l| holes_layers.contains(l)) {
        return Err(Error::Unsupported(format!(
            "the layer `{both}` is named as both the outline layer and the holes layer"
        )));
    }
    for layer in &layers {
        let used = outline_layers.contains(&layer.name) || holes_layers.contains(&layer.name);
        if !used && layer.has_geometry() {
            warnings.push(format!("layer {} was not used", layer.describe()));
        }
    }
    let mut skipped: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for (layer, kind) in &drawing.skipped {
        if outline_layers.contains(layer) || holes_layers.contains(layer) {
            *skipped.entry((layer, kind)).or_default() += 1;
        }
    }
    for ((layer, kind), n) in skipped {
        warnings.push(format!("{n} {kind} on layer `{layer}` were not read"));
    }
    warnings.extend(drawing.warnings.iter().cloned());

    let prims_of = |layers: &[String]| {
        let mut segs = Vec::new();
        let mut circles = Vec::new();
        for item in drawing.items.iter().filter(|i| layers.contains(&i.layer)) {
            for p in &item.prims {
                match *p {
                    entities::Prim::Seg(s) => segs.push(s),
                    entities::Prim::Circle { centre, radius } => circles.push((centre, radius)),
                }
            }
        }
        (segs, circles)
    };

    let (segs, circles) = prims_of(&outline_layers);
    if segs.is_empty() && circles.is_empty() {
        return Err(Error::NoGeometry { layers });
    }
    let mut loops = chain::loops(&segs, options.join_tolerance, &mut warnings)?;
    loops.extend(circles.iter().map(|&(c, r)| chain::circle_loop(c, r)));

    // Holes.
    let (hole_segs, hole_circles) = prims_of(&holes_layers);
    let mut holes: Vec<Hole> = hole_circles
        .iter()
        .map(|&(centre, r)| Hole {
            centre,
            diameter: 2.0 * r,
        })
        .collect();
    for l in chain::loops(&hole_segs, options.join_tolerance, &mut warnings)? {
        match l.as_circle() {
            Some((centre, diameter)) => holes.push(Hole { centre, diameter }),
            None => {
                return Err(Error::HoleNotCircle {
                    loop_: l.describe(),
                });
            }
        }
    }

    let (outer, cutouts) = chain::classify(loops, options.join_tolerance)?;

    // To the board's frame: the outline's left and top edges are the origin, y down.
    let (lo, hi) = outer.bounds();
    let origin = (lo.0, hi.1);
    let flip = |poly: &Polygon| {
        let mut vertices: Vec<Vertex> = poly
            .vertices
            .iter()
            .map(|v| Vertex {
                x: v.x - origin.0,
                y: origin.1 - v.y,
                bulge: -v.bulge,
            })
            .collect();
        // Clockwise on the screen: a positive shoelace sum with y down.
        if chain::shoelace(&vertices) < 0.0 {
            // Reverse the loop: each edge now runs the other way with the opposite bulge.
            let n = vertices.len();
            let bulges: Vec<f64> = vertices.iter().map(|v| v.bulge).collect();
            vertices.reverse();
            for (i, v) in vertices.iter_mut().enumerate() {
                // The vertex now at i was at n-1-i; its edge to the next was the edge from
                // the one before it in the old order, reversed.
                v.bulge = -bulges[(2 * n - 2 - i) % n];
            }
        }
        Polygon { vertices }
    };
    let outer = flip(&outer);
    let mut cutouts: Vec<Polygon> = cutouts.iter().map(&flip).collect();
    cutouts.sort_by(|a, b| {
        let (ka, kb) = (a.bounds().0, b.bounds().0);
        (ka.1, ka.0).partial_cmp(&(kb.1, kb.0)).expect("finite")
    });
    let mut holes: Vec<Hole> = holes
        .into_iter()
        .map(|h| Hole {
            centre: (h.centre.0 - origin.0, origin.1 - h.centre.1),
            diameter: h.diameter,
        })
        .collect();
    holes.sort_by(|a, b| {
        (a.centre.1, a.centre.0)
            .partial_cmp(&(b.centre.1, b.centre.0))
            .expect("finite")
    });

    Ok(Outline {
        outer,
        cutouts,
        holes,
        units,
        units_from_file,
        origin,
        layers,
        outline_layers,
        holes_layers,
        warnings,
    })
}
