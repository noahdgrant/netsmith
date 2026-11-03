//! From the entities of a DXF file to line, arc and circle primitives in millimetres, in the
//! drawing's own frame (y up): blocks are inserted with their transforms, and splines and
//! ellipses are approximated by chords.

use std::collections::BTreeMap;
use std::f64::consts::TAU;

use crate::Error;
use crate::geom::{P, Xf, dist};
use crate::parse::{Dxf, Raw, number_of};

/// A straight or curved edge from `a` to `b`; see [`crate::geom`] for the bulge.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Seg {
    pub a: P,
    pub b: P,
    pub bulge: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Prim {
    Seg(Seg),
    Circle { centre: P, radius: f64 },
}

/// What one entity (after inserting blocks) drew.
#[derive(Debug)]
pub(crate) struct Item {
    pub layer: String,
    pub kind: &'static str,
    pub prims: Vec<Prim>,
}

/// Everything the entities drew, and what was not read.
#[derive(Debug, Default)]
pub(crate) struct Drawing {
    pub items: Vec<Item>,
    /// `(layer, entity type)` of every entity of a kind that is not read.
    pub skipped: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

const MAX_DEPTH: usize = 16;

pub(crate) fn expand(dxf: &Dxf, scale: f64, curve_tol: f64) -> Result<Drawing, Error> {
    let mut ctx = Ctx {
        dxf,
        curve_tol,
        drawing: Drawing::default(),
        stack: Vec::new(),
    };
    ctx.entities(&dxf.entities, &Xf::scale(scale, scale), None)?;
    Ok(ctx.drawing)
}

struct Ctx<'a> {
    dxf: &'a Dxf,
    curve_tol: f64,
    drawing: Drawing,
    stack: Vec<String>,
}

/// The mirror that an extrusion direction of (0, 0, -1) makes of the entity's own frame:
/// `x` becomes `-x`. Any other direction that is not (0, 0, 1) is not supported.
fn extrusion(raw: &Raw) -> Result<Xf, Error> {
    let n = (
        raw.number_or(210, 0.0)?,
        raw.number_or(220, 0.0)?,
        raw.number_or(230, 1.0)?,
    );
    if n.0.abs() < 1e-9 && n.1.abs() < 1e-9 {
        if n.2 > 0.0 {
            return Ok(Xf::IDENTITY);
        }
        if n.2 < 0.0 {
            return Ok(Xf::scale(-1.0, 1.0));
        }
    }
    Err(Error::Unsupported(format!(
        "{} on line {} is drawn on a plane that is not parallel to the XY plane (extrusion direction {}, {}, {}); \
         export the drawing flat",
        raw.kind, raw.line, n.0, n.1, n.2
    )))
}

fn xy(raw: &Raw, cx: i32, cy: i32) -> Result<P, Error> {
    Ok((raw.number_or(cx, 0.0)?, raw.number_or(cy, 0.0)?))
}

impl Ctx<'_> {
    fn entities(&mut self, list: &[Raw], xf: &Xf, inherit: Option<&str>) -> Result<(), Error> {
        let mut i = 0;
        while i < list.len() {
            let raw = &list[i];
            i += 1;
            // Paper space (group 67 = 1) is a layout sheet, not the model.
            if raw.number(67)? == Some(1.0) {
                continue;
            }
            let mut layer = raw.text(8).unwrap_or("0").to_string();
            if layer == "0"
                && let Some(l) = inherit
            {
                layer = l.to_string();
            }
            match raw.kind.as_str() {
                "LINE" => {
                    let a = xf.apply(xy(raw, 10, 20)?);
                    let b = xf.apply(xy(raw, 11, 21)?);
                    self.push(layer, "LINE", vec![Prim::Seg(Seg { a, b, bulge: 0.0 })]);
                }
                "ARC" => {
                    let prims = self.arc(raw, xf)?;
                    self.push(layer, "ARC", prims);
                }
                "CIRCLE" => {
                    let prims = self.circle(raw, xf)?;
                    self.push(layer, "CIRCLE", prims);
                }
                "LWPOLYLINE" => {
                    let prims = self.lwpolyline(raw, xf)?;
                    self.push(layer, "LWPOLYLINE", prims);
                }
                "POLYLINE" => {
                    // The vertices follow, up to SEQEND.
                    let start = i;
                    while i < list.len() && list[i].kind != "SEQEND" {
                        i += 1;
                    }
                    let vertices = &list[start..i];
                    if i < list.len() {
                        i += 1; // SEQEND
                    }
                    let prims = self.polyline(raw, vertices, xf)?;
                    self.push(layer, "POLYLINE", prims);
                }
                "SPLINE" => {
                    let prims = self.spline(raw, xf)?;
                    self.push(layer, "SPLINE", prims);
                }
                "ELLIPSE" => {
                    let prims = self.ellipse(raw, xf)?;
                    self.push(layer, "ELLIPSE", prims);
                }
                "INSERT" => self.insert(raw, xf, &layer)?,
                "VERTEX" | "SEQEND" | "ATTRIB" => {}
                other => self.drawing.skipped.push((layer, other.to_string())),
            }
        }
        Ok(())
    }

    fn push(&mut self, layer: String, kind: &'static str, prims: Vec<Prim>) {
        self.drawing.items.push(Item { layer, kind, prims });
    }

    fn need_similarity(&self, raw: &Raw, xf: &Xf) -> Result<(), Error> {
        if xf.is_similarity() {
            Ok(())
        } else {
            Err(Error::Unsupported(format!(
                "{} on line {} is stretched by a block insert with different scales in x and y, \
                 which turns it into an ellipse; insert it with one scale, or explode the block in the CAD tool",
                raw.kind, raw.line
            )))
        }
    }

    /// A segment from `a` to `b` with `bulge`, in the entity's own frame, moved by `xf`.
    fn seg(xf: &Xf, a: P, b: P, bulge: f64) -> Prim {
        let flip = if xf.det() < 0.0 { -1.0 } else { 1.0 };
        Prim::Seg(Seg {
            a: xf.apply(a),
            b: xf.apply(b),
            bulge: bulge * flip,
        })
    }

    fn arc(&mut self, raw: &Raw, xf: &Xf) -> Result<Vec<Prim>, Error> {
        let xf = xf.after(&extrusion(raw)?);
        let c = xy(raw, 10, 20)?;
        let r = raw.number_or(40, 0.0)?;
        let s = raw.number_or(50, 0.0)?.to_radians();
        let e = raw.number_or(51, 360.0)?.to_radians();
        if r <= 0.0 {
            self.drawing.warnings.push(format!(
                "ARC on line {} has no radius and is ignored",
                raw.line
            ));
            return Ok(Vec::new());
        }
        self.need_similarity(raw, &xf)?;
        let sweep = (e - s).rem_euclid(TAU);
        if sweep < 1e-9 || TAU - sweep < 1e-9 {
            return Ok(vec![Prim::Circle {
                centre: xf.apply(c),
                radius: r * xf.length_scale(),
            }]);
        }
        let a = (c.0 + r * s.cos(), c.1 + r * s.sin());
        let b = (c.0 + r * e.cos(), c.1 + r * e.sin());
        Ok(vec![Self::seg(&xf, a, b, (sweep / 4.0).tan())])
    }

    fn circle(&mut self, raw: &Raw, xf: &Xf) -> Result<Vec<Prim>, Error> {
        let xf = xf.after(&extrusion(raw)?);
        let c = xy(raw, 10, 20)?;
        let r = raw.number_or(40, 0.0)?;
        if r <= 0.0 {
            self.drawing.warnings.push(format!(
                "CIRCLE on line {} has no radius and is ignored",
                raw.line
            ));
            return Ok(Vec::new());
        }
        self.need_similarity(raw, &xf)?;
        Ok(vec![Prim::Circle {
            centre: xf.apply(c),
            radius: r * xf.length_scale(),
        }])
    }

    fn lwpolyline(&mut self, raw: &Raw, xf: &Xf) -> Result<Vec<Prim>, Error> {
        let xf = xf.after(&extrusion(raw)?);
        let closed = raw.number_or(70, 0.0)? as i64 & 1 == 1;
        // Each vertex starts at a code 10; its y and bulge follow it.
        let mut vertices: Vec<(P, f64)> = Vec::new();
        for g in &raw.groups {
            match g.code {
                10 => vertices.push(((number_of(g)?, 0.0), 0.0)),
                20 => {
                    if let Some(v) = vertices.last_mut() {
                        v.0.1 = number_of(g)?;
                    }
                }
                42 => {
                    if let Some(v) = vertices.last_mut() {
                        v.1 = number_of(g)?;
                    }
                }
                _ => {}
            }
        }
        Ok(self.chain(raw, &xf, &vertices, closed))
    }

    /// The edges through `vertices`, each with the bulge of the vertex it starts at.
    fn chain(&mut self, raw: &Raw, xf: &Xf, vertices: &[(P, f64)], closed: bool) -> Vec<Prim> {
        let mut out = Vec::new();
        let n = vertices.len();
        if n < 2 {
            self.drawing.warnings.push(format!(
                "{} on line {} has {n} vertices and is ignored",
                raw.kind, raw.line
            ));
            return out;
        }
        let edges = if closed { n } else { n - 1 };
        for i in 0..edges {
            let (a, bulge) = vertices[i];
            let (b, _) = vertices[(i + 1) % n];
            if bulge != 0.0 && !xf.is_similarity() {
                self.drawing.warnings.push(format!(
                    "{} on line {} has arcs and is stretched unevenly by a block insert; its arcs are kept as chords",
                    raw.kind, raw.line
                ));
                out.push(Self::seg(xf, a, b, 0.0));
            } else {
                out.push(Self::seg(xf, a, b, bulge));
            }
        }
        out
    }

    fn polyline(&mut self, raw: &Raw, vertices: &[Raw], xf: &Xf) -> Result<Vec<Prim>, Error> {
        let flags = raw.number_or(70, 0.0)? as i64;
        if flags & (16 | 64) != 0 {
            self.drawing.warnings.push(format!(
                "POLYLINE on line {} is a mesh, not an outline, and is ignored",
                raw.line
            ));
            return Ok(Vec::new());
        }
        let xf = xf.after(&extrusion(raw)?);
        let mut points: Vec<(P, f64)> = Vec::new();
        for v in vertices.iter().filter(|v| v.kind == "VERTEX") {
            // 16: the control point of a spline frame, which the curve does not pass through.
            if v.number_or(70, 0.0)? as i64 & 16 != 0 {
                continue;
            }
            points.push((xy(v, 10, 20)?, v.number_or(42, 0.0)?));
        }
        Ok(self.chain(raw, &xf, &points, flags & 1 == 1))
    }

    fn ellipse(&mut self, raw: &Raw, xf: &Xf) -> Result<Vec<Prim>, Error> {
        if raw.number_or(230, 1.0)? < 0.0 {
            return Err(Error::Unsupported(format!(
                "ELLIPSE on line {} has a downward normal; export the drawing flat",
                raw.line
            )));
        }
        let c = xy(raw, 10, 20)?;
        let major = xy(raw, 11, 21)?;
        let ratio = raw.number_or(40, 1.0)?;
        let start = raw.number_or(41, 0.0)?;
        let mut end = raw.number_or(42, TAU)?;
        if end <= start {
            end += TAU;
        }
        let minor = (-major.1 * ratio, major.0 * ratio);
        let rmax = major.0.hypot(major.1);
        let at = |t: f64| {
            xf.apply((
                c.0 + major.0 * t.cos() + minor.0 * t.sin(),
                c.1 + major.1 * t.cos() + minor.1 * t.sin(),
            ))
        };
        // A chord of parameter step dt on a curve of radius at most rmax*scale sags by at
        // most r (1 - cos(dt / 2)).
        let r = rmax * xf.length_scale().max(1e-12);
        let dt = if self.curve_tol >= r {
            TAU / 8.0
        } else {
            2.0 * (1.0 - self.curve_tol / r).acos()
        };
        let n = (((end - start) / dt).ceil() as usize).clamp(8, 100_000);
        let mut prims = Vec::with_capacity(n);
        let mut prev = at(start);
        for k in 1..=n {
            let p = at(start + (end - start) * k as f64 / n as f64);
            prims.push(Prim::Seg(Seg {
                a: prev,
                b: p,
                bulge: 0.0,
            }));
            prev = p;
        }
        Ok(prims)
    }

    fn spline(&mut self, raw: &Raw, xf: &Xf) -> Result<Vec<Prim>, Error> {
        let flags = raw.number_or(70, 0.0)? as i64;
        let degree = raw.number_or(71, 3.0)? as usize;
        if flags & 2 != 0 {
            return Err(Error::Unsupported(format!(
                "SPLINE on line {} is periodic; export lines and arcs, or a non-periodic spline",
                raw.line
            )));
        }
        let knots = raw.numbers(40)?;
        let weights = raw.numbers(41)?;
        let xs = raw.numbers(10)?;
        let ys = raw.numbers(20)?;
        let n = xs.len().min(ys.len());
        if n == 0 {
            return Err(Error::Unsupported(format!(
                "SPLINE on line {} has no control points (only fit points); export lines and arcs",
                raw.line
            )));
        }
        if degree < 1 || n <= degree || knots.len() != n + degree + 1 {
            return Err(Error::Syntax {
                line: raw.line,
                message: format!(
                    "SPLINE has {n} control points, degree {degree} and {} knots; it needs more points than the degree and {} knots",
                    knots.len(),
                    n + degree + 1
                ),
            });
        }
        if !weights.is_empty() && weights.len() != n {
            return Err(Error::Syntax {
                line: raw.line,
                message: format!(
                    "SPLINE has {n} control points and {} weights",
                    weights.len()
                ),
            });
        }
        let w = |i: usize| weights.get(i).copied().unwrap_or(1.0);
        let control: Vec<(f64, f64, f64)> =
            (0..n).map(|i| (xs[i] * w(i), ys[i] * w(i), w(i))).collect();
        let eval = |u: f64| -> P {
            let k = (degree..n)
                .rev()
                .find(|&k| knots[k] <= u && knots[k] < knots[k + 1])
                .unwrap_or(degree);
            let mut d: Vec<(f64, f64, f64)> =
                (0..=degree).map(|j| control[j + k - degree]).collect();
            for r in 1..=degree {
                for j in (r..=degree).rev() {
                    let i = j + k - degree;
                    let den = knots[i + degree + 1 - r] - knots[i];
                    let alpha = if den == 0.0 {
                        0.0
                    } else {
                        (u - knots[i]) / den
                    };
                    d[j] = (
                        (1.0 - alpha) * d[j - 1].0 + alpha * d[j].0,
                        (1.0 - alpha) * d[j - 1].1 + alpha * d[j].1,
                        (1.0 - alpha) * d[j - 1].2 + alpha * d[j].2,
                    );
                }
            }
            let h = d[degree];
            xf.apply((h.0 / h.2, h.1 / h.2))
        };
        let tol = self.curve_tol;
        let mut points: Vec<P> = vec![eval(knots[degree])];
        for k in degree..n {
            let (u0, u1) = (knots[k], knots[k + 1]);
            if u1 <= u0 {
                continue;
            }
            let pieces = 8;
            for s in 0..pieces {
                let (a, b) = (
                    u0 + (u1 - u0) * f64::from(s) / f64::from(pieces),
                    u0 + (u1 - u0) * f64::from(s + 1) / f64::from(pieces),
                );
                let pa = *points.last().expect("started with a point");
                refine(&eval, a, b, pa, eval(b), tol, 0, &mut points);
            }
        }
        let prims: Vec<Prim> = points
            .windows(2)
            .map(|w| {
                Prim::Seg(Seg {
                    a: w[0],
                    b: w[1],
                    bulge: 0.0,
                })
            })
            .collect();
        Ok(prims)
    }

    fn insert(&mut self, raw: &Raw, xf: &Xf, layer: &str) -> Result<(), Error> {
        let name = raw.text(2).unwrap_or("");
        let key = name.to_ascii_uppercase();
        let Some(block) = self.dxf.blocks.get(&key) else {
            return Err(Error::Syntax {
                line: raw.line,
                message: format!("INSERT of the block `{name}`, which the file does not define"),
            });
        };
        if self.stack.len() >= MAX_DEPTH || self.stack.contains(&key) {
            return Err(Error::Unsupported(format!(
                "INSERT of the block `{name}` on line {} contains itself (or is nested over {MAX_DEPTH} deep)",
                raw.line
            )));
        }
        let ocs = xf.after(&extrusion(raw)?);
        let ins = xy(raw, 10, 20)?;
        let sx = raw.number_or(41, 1.0)?;
        let sy = raw.number_or(42, 1.0)?;
        let angle = raw.number_or(50, 0.0)?.to_radians();
        let cols = (raw.number_or(70, 1.0)? as usize).max(1);
        let rows = (raw.number_or(71, 1.0)? as usize).max(1);
        let (col_gap, row_gap) = (raw.number_or(44, 0.0)?, raw.number_or(45, 0.0)?);
        let rot = Xf::rotate(angle);
        let local = rot
            .after(&Xf::scale(sx, sy))
            .after(&Xf::translate(-block.base.0, -block.base.1));
        self.stack.push(key);
        for col in 0..cols {
            for row in 0..rows {
                let off = rot.apply((col as f64 * col_gap, row as f64 * row_gap));
                let at = Xf::translate(ins.0 + off.0, ins.1 + off.1);
                let total = ocs.after(&at).after(&local);
                self.entities(&block.entities, &total, Some(layer))?;
            }
        }
        self.stack.pop();
        Ok(())
    }
}

/// Add the points of the curve between parameters `a` and `b` (whose points are `pa` and
/// `pb`, `pa` already in `out`) so that the chords stay within `tol`.
#[allow(clippy::too_many_arguments)]
fn refine(
    eval: &dyn Fn(f64) -> P,
    a: f64,
    b: f64,
    pa: P,
    pb: P,
    tol: f64,
    depth: u32,
    out: &mut Vec<P>,
) {
    let chord_dist = |p: P| {
        let l = dist(pa, pb);
        if l == 0.0 {
            dist(p, pa)
        } else {
            ((pb.0 - pa.0) * (pa.1 - p.1) - (pa.0 - p.0) * (pb.1 - pa.1)).abs() / l
        }
    };
    let (q1, qm, q3) = (
        eval(a + (b - a) * 0.25),
        eval((a + b) / 2.0),
        eval(a + (b - a) * 0.75),
    );
    let off = chord_dist(q1).max(chord_dist(qm)).max(chord_dist(q3));
    if off <= tol * 0.5 || depth >= 14 {
        out.push(pb);
        return;
    }
    let m = (a + b) / 2.0;
    refine(eval, a, m, pa, qm, tol, depth + 1, out);
    refine(eval, m, b, qm, pb, tol, depth + 1, out);
}

/// Count the entities of each type on each layer.
pub(crate) fn count_by_layer(drawing: &Drawing) -> BTreeMap<String, BTreeMap<String, usize>> {
    let mut layers: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for item in &drawing.items {
        *layers
            .entry(item.layer.clone())
            .or_default()
            .entry(item.kind.to_string())
            .or_default() += 1;
    }
    for (layer, kind) in &drawing.skipped {
        *layers
            .entry(layer.clone())
            .or_default()
            .entry(kind.clone())
            .or_default() += 1;
    }
    layers
}
