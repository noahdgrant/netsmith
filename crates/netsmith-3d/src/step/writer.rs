//! The text of a STEP file (ISO 10303-21): numbered entity instances, written in the order they
//! are made, and the boundary representation of the solids the scene is made of.
//!
//! Every solid is a closed shell of `ADVANCED_FACE`s on elementary surfaces: a [`Extrusion`]
//! (a profile of lines and circular arcs with holes, swept along a straight line) has two planar
//! end faces and one side face per segment of its profile, planar for a line and cylindrical for
//! an arc; a [`Polyhedron`] has one planar face per polygon. Each edge is one `EDGE_CURVE` used by
//! its two faces in opposite senses, so a reader sews nothing.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A reference to an entity instance, `#n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Id(pub usize);

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// A list of references, `(#1,#2)`.
pub fn list(ids: &[Id]) -> String {
    let mut s = String::from("(");
    for (k, id) in ids.iter().enumerate() {
        if k > 0 {
            s.push(',');
        }
        let _ = write!(s, "{id}");
    }
    s.push(')');
    s
}

/// A real as STEP writes one: always with a point (`1.`, `-0.25`), to ten places (1e-10 mm for a
/// length), with no trailing zeros and never `-0.`.
pub fn real(x: f64) -> String {
    let mut s = format!("{x:.10}");
    while s.ends_with('0') {
        s.pop();
    }
    if s == "-0." {
        s = "0.".into();
    }
    s
}

/// A string as STEP writes one: in single quotes, a quote doubled, and every character outside
/// printable ASCII written as `\X2\hhhh\X0\` (UTF-16 code units), so the file is plain ASCII.
pub fn string(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        match c {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                out.push_str("\\X2\\");
                for u in c.encode_utf16(&mut units) {
                    let _ = write!(out, "{u:04X}");
                }
                out.push_str("\\X0\\");
            }
        }
    }
    out.push('\'');
    out
}

/// The entity instances of a file's `DATA` section.
#[derive(Debug, Default)]
pub struct Writer {
    lines: Vec<String>,
    /// Points and directions already written, by their text, so that each is written once.
    shared: BTreeMap<String, Id>,
}

impl Writer {
    /// Adds an instance (`ENTITY(...)`, or a complex one in parentheses) and returns its id.
    pub fn add(&mut self, body: impl Into<String>) -> Id {
        self.lines.push(body.into());
        Id(self.lines.len())
    }

    fn shared(&mut self, body: String) -> Id {
        if let Some(&id) = self.shared.get(&body) {
            return id;
        }
        let id = self.add(body.clone());
        self.shared.insert(body, id);
        id
    }

    pub fn point(&mut self, p: [f64; 3]) -> Id {
        self.shared(format!(
            "CARTESIAN_POINT('',({},{},{}))",
            real(p[0]),
            real(p[1]),
            real(p[2])
        ))
    }

    pub fn direction(&mut self, d: [f64; 3]) -> Id {
        self.shared(format!(
            "DIRECTION('',({},{},{}))",
            real(d[0]),
            real(d[1]),
            real(d[2])
        ))
    }

    /// A placement at `at` with its Z along `axis` and its X along `x`.
    pub fn axis(&mut self, at: [f64; 3], axis: [f64; 3], x: [f64; 3]) -> Id {
        let (p, z, x) = (self.point(at), self.direction(axis), self.direction(x));
        self.add(format!("AXIS2_PLACEMENT_3D('',{p},{z},{x})"))
    }

    /// The file, with `header` (the lines between `HEADER;` and `ENDSEC;`).
    pub fn file(&self, header: &str) -> String {
        let mut out = String::new();
        out.push_str("ISO-10303-21;\nHEADER;\n");
        out.push_str(header);
        out.push_str("ENDSEC;\nDATA;\n");
        for (k, line) in self.lines.iter().enumerate() {
            let _ = writeln!(out, "#{}={};", k + 1, line);
        }
        out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
        out
    }
}

/// A segment of a profile, from its start to the next segment's start (the first one's, for
/// the last): a straight line, or an arc round `centre`, counterclockwise when `ccw`. An arc
/// that is the only segment of its loop is a full circle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    Line {
        start: [f64; 2],
    },
    Arc {
        start: [f64; 2],
        centre: [f64; 2],
        ccw: bool,
    },
}

impl Segment {
    pub fn start(&self) -> [f64; 2] {
        match *self {
            Segment::Line { start } | Segment::Arc { start, .. } => start,
        }
    }
}

/// A closed loop of segments.
pub type Loop = Vec<Segment>;

/// Points along a loop, enough to tell which way it turns: each start, and three points inside
/// each arc.
fn samples(l: &Loop) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for (k, s) in l.iter().enumerate() {
        out.push(s.start());
        if let Segment::Arc { start, centre, ccw } = *s {
            let end = l[(k + 1) % l.len()].start();
            let a0 = (start[1] - centre[1]).atan2(start[0] - centre[0]);
            let mut a1 = (end[1] - centre[1]).atan2(end[0] - centre[0]);
            let tau = std::f64::consts::TAU;
            if ccw {
                while a1 <= a0 {
                    a1 += tau;
                }
            } else {
                while a1 >= a0 {
                    a1 -= tau;
                }
            }
            let r = (start[0] - centre[0]).hypot(start[1] - centre[1]);
            for q in 1..4 {
                let a = a0 + (a1 - a0) * f64::from(q) / 4.0;
                out.push([centre[0] + r * a.cos(), centre[1] + r * a.sin()]);
            }
        }
    }
    out
}

/// Twice the signed area a loop holds: positive when it turns counterclockwise. Only its sign is
/// used, so the arcs' samples (with the platform's `atan2`) cannot change the file.
pub fn signed_area(l: &Loop) -> f64 {
    let p = samples(l);
    (0..p.len())
        .map(|k| {
            let (a, b) = (p[k], p[(k + 1) % p.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

/// The loop run the other way.
pub fn reversed(l: &Loop) -> Loop {
    let n = l.len();
    (0..n)
        .map(|k| {
            // The segment from start k+1 back to start k.
            let seg = l[n - 1 - k];
            let end = l[(2 * n - k) % n].start();
            match seg {
                Segment::Line { .. } => Segment::Line { start: end },
                Segment::Arc { centre, ccw, .. } => Segment::Arc {
                    start: end,
                    centre,
                    ccw: !ccw,
                },
            }
        })
        .collect()
}

/// A polygon as a loop of lines, with the points that repeat their neighbour dropped.
pub fn polygon(points: &[[f64; 2]]) -> Loop {
    let mut out: Loop = Vec::with_capacity(points.len());
    for &p in points {
        if out.last().is_none_or(|s| s.start() != p) {
            out.push(Segment::Line { start: p });
        }
    }
    while out.len() > 1 && out[0].start() == out[out.len() - 1].start() {
        out.pop();
    }
    out
}

/// A profile swept along a straight line: the points `origin + a * u + b * v + t * n` for each
/// `(a, b)` in the region `loops` bound (the first the outside, the rest holes) and `t` from
/// `from` to `to`. `u`, `v` and `n` are unit vectors, at right angles.
#[derive(Debug, Clone, PartialEq)]
pub struct Extrusion {
    pub origin: [f64; 3],
    pub u: [f64; 3],
    pub v: [f64; 3],
    pub n: [f64; 3],
    pub loops: Vec<Loop>,
    pub from: f64,
    pub to: f64,
}

/// A closed solid of planar polygons, each listed counterclockwise seen from outside.
#[derive(Debug, Clone, PartialEq)]
pub struct Polyhedron {
    pub points: Vec<[f64; 3]>,
    pub faces: Vec<Vec<usize>>,
}

pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn neg(a: [f64; 3]) -> [f64; 3] {
    [-a[0], -a[1], -a[2]]
}

fn unit(a: [f64; 3]) -> [f64; 3] {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

impl Extrusion {
    /// The same solid with `u`, `v`, `n` right-handed, `from < to`, the outside loop
    /// counterclockwise and the holes clockwise, as [`Writer::extrusion`] needs it. `None` when it
    /// has no volume.
    pub fn normalised(mut self) -> Option<Extrusion> {
        if dot(cross(self.u, self.v), self.n) < 0.0 {
            self.n = neg(self.n);
            (self.from, self.to) = (-self.to, -self.from);
        }
        if self.from > self.to {
            (self.from, self.to) = (self.to, self.from);
        }
        let ok = |x: f64| x.is_finite();
        if !(self.to - self.from > 1e-9 && ok(self.from) && ok(self.to)) {
            return None;
        }
        self.loops
            .retain(|l| !l.is_empty() && signed_area(l).abs() > 1e-12);
        if self.loops.is_empty() {
            return None;
        }
        for (k, l) in self.loops.iter_mut().enumerate() {
            if (signed_area(l) > 0.0) != (k == 0) {
                *l = reversed(l);
            }
        }
        Some(self)
    }

    fn at(&self, p: [f64; 2], t: f64) -> [f64; 3] {
        std::array::from_fn(|i| {
            self.origin[i] + p[0] * self.u[i] + p[1] * self.v[i] + t * self.n[i]
        })
    }

    fn dir(&self, d: [f64; 2]) -> [f64; 3] {
        unit([
            d[0] * self.u[0] + d[1] * self.v[0],
            d[0] * self.u[1] + d[1] * self.v[1],
            d[0] * self.u[2] + d[1] * self.v[2],
        ])
    }
}

impl Writer {
    /// A curve of segment `s` (ending at `end`) at height `t` of `e`, and whether the edge from its
    /// start to `end` runs with the curve.
    fn curve(&mut self, e: &Extrusion, s: Segment, end: [f64; 2], t: f64) -> (Id, bool) {
        match s {
            Segment::Line { start } => {
                let p = self.point(e.at(start, t));
                let d = self.direction(e.dir([end[0] - start[0], end[1] - start[1]]));
                let len = (end[0] - start[0]).hypot(end[1] - start[1]);
                let v = self.add(format!("VECTOR('',{d},{})", real(len)));
                (self.add(format!("LINE('',{p},{v})")), true)
            }
            Segment::Arc { start, centre, ccw } => {
                let r = (start[0] - centre[0]).hypot(start[1] - centre[1]);
                let x = e.dir([start[0] - centre[0], start[1] - centre[1]]);
                let a = self.axis(e.at(centre, t), e.n, x);
                (self.add(format!("CIRCLE('',{a},{})", real(r))), ccw)
            }
        }
    }

    /// The solid of `e` (normalised) as a `MANIFOLD_SOLID_BREP` named `name`.
    pub fn extrusion(&mut self, name: &str, e: &Extrusion) -> Id {
        let (from, to) = (e.from, e.to);
        let mut faces = Vec::new();
        let mut tops: Vec<Vec<Id>> = Vec::new();
        let mut bottoms: Vec<Vec<Id>> = Vec::new();
        for l in &e.loops {
            let n = l.len();
            let starts: Vec<[f64; 2]> = l.iter().map(Segment::start).collect();
            let vb: Vec<Id> = starts
                .iter()
                .map(|&p| {
                    let pt = self.point(e.at(p, from));
                    self.add(format!("VERTEX_POINT('',{pt})"))
                })
                .collect();
            let vt: Vec<Id> = starts
                .iter()
                .map(|&p| {
                    let pt = self.point(e.at(p, to));
                    self.add(format!("VERTEX_POINT('',{pt})"))
                })
                .collect();
            let mut bottom = Vec::with_capacity(n);
            let mut top = Vec::with_capacity(n);
            for k in 0..n {
                let end = starts[(k + 1) % n];
                for (t, verts, out) in [(from, &vb, &mut bottom), (to, &vt, &mut top)] {
                    let (c, sense) = self.curve(e, l[k], end, t);
                    let (a, b) = (verts[k], verts[(k + 1) % n]);
                    out.push(self.add(format!(
                        "EDGE_CURVE('',{a},{b},{c},{})",
                        if sense { ".T." } else { ".F." }
                    )));
                }
            }
            let up = self.direction(e.n);
            let len = to - from;
            let verticals: Vec<Id> = (0..n)
                .map(|k| {
                    let p = self.point(e.at(starts[k], from));
                    let v = self.add(format!("VECTOR('',{up},{})", real(len)));
                    let line = self.add(format!("LINE('',{p},{v})"));
                    self.add(format!("EDGE_CURVE('',{},{},{line},.T.)", vb[k], vt[k]))
                })
                .collect();
            for k in 0..n {
                let end = starts[(k + 1) % n];
                let (surface, same) = match l[k] {
                    Segment::Line { start } => {
                        let d = e.dir([end[0] - start[0], end[1] - start[1]]);
                        let a = self.axis(e.at(start, from), cross(d, e.n), d);
                        (self.add(format!("PLANE('',{a})")), true)
                    }
                    Segment::Arc { start, centre, ccw } => {
                        let r = (start[0] - centre[0]).hypot(start[1] - centre[1]);
                        let x = e.dir([start[0] - centre[0], start[1] - centre[1]]);
                        let a = self.axis(e.at(centre, from), e.n, x);
                        (
                            self.add(format!("CYLINDRICAL_SURFACE('',{a},{})", real(r))),
                            ccw,
                        )
                    }
                };
                let edges = [
                    self.add(format!("ORIENTED_EDGE('',*,*,{},.T.)", bottom[k])),
                    self.add(format!(
                        "ORIENTED_EDGE('',*,*,{},.T.)",
                        verticals[(k + 1) % n]
                    )),
                    self.add(format!("ORIENTED_EDGE('',*,*,{},.F.)", top[k])),
                    self.add(format!("ORIENTED_EDGE('',*,*,{},.F.)", verticals[k])),
                ];
                let lp = self.add(format!("EDGE_LOOP('',{})", list(&edges)));
                let bound = self.add(format!("FACE_OUTER_BOUND('',{lp},.T.)"));
                faces.push(self.add(format!(
                    "ADVANCED_FACE('',({bound}),{surface},{})",
                    if same { ".T." } else { ".F." }
                )));
            }
            tops.push(top);
            bottoms.push(bottom);
        }
        // The two ends: the top runs each loop forward, the bottom backward.
        for (t, loops, forward) in [(to, &tops, true), (from, &bottoms, false)] {
            let a = self.axis(e.at([0.0, 0.0], t), e.n, e.u);
            let plane = self.add(format!("PLANE('',{a})"));
            let mut bounds = Vec::new();
            for (k, edges) in loops.iter().enumerate() {
                let oriented: Vec<Id> = if forward {
                    edges
                        .iter()
                        .map(|c| self.add(format!("ORIENTED_EDGE('',*,*,{c},.T.)")))
                        .collect()
                } else {
                    edges
                        .iter()
                        .rev()
                        .map(|c| self.add(format!("ORIENTED_EDGE('',*,*,{c},.F.)")))
                        .collect()
                };
                let lp = self.add(format!("EDGE_LOOP('',{})", list(&oriented)));
                let kind = if k == 0 {
                    "FACE_OUTER_BOUND"
                } else {
                    "FACE_BOUND"
                };
                bounds.push(self.add(format!("{kind}('',{lp},.T.)")));
            }
            faces.push(self.add(format!(
                "ADVANCED_FACE('',{},{plane},{})",
                list(&bounds),
                if forward { ".T." } else { ".F." }
            )));
        }
        let shell = self.add(format!("CLOSED_SHELL('',{})", list(&faces)));
        self.add(format!("MANIFOLD_SOLID_BREP({},{shell})", string(name)))
    }

    /// The solid of `p` as a `MANIFOLD_SOLID_BREP` named `name`.
    pub fn polyhedron(&mut self, name: &str, p: &Polyhedron) -> Id {
        let verts: Vec<Id> = p
            .points
            .iter()
            .map(|&q| {
                let pt = self.point(q);
                self.add(format!("VERTEX_POINT('',{pt})"))
            })
            .collect();
        let mut edges: BTreeMap<(usize, usize), Id> = BTreeMap::new();
        let mut faces = Vec::new();
        for f in &p.faces {
            let n = f.len();
            let mut oriented = Vec::with_capacity(n);
            for k in 0..n {
                let (a, b) = (f[k], f[(k + 1) % n]);
                let key = (a.min(b), a.max(b));
                let edge = match edges.get(&key) {
                    Some(&e) => e,
                    None => {
                        let (s, t) = (p.points[key.0], p.points[key.1]);
                        let d = [t[0] - s[0], t[1] - s[1], t[2] - s[2]];
                        let ps = self.point(s);
                        let dir = self.direction(unit(d));
                        let v = self.add(format!("VECTOR('',{dir},{})", real(dot(d, d).sqrt())));
                        let line = self.add(format!("LINE('',{ps},{v})"));
                        let e = self.add(format!(
                            "EDGE_CURVE('',{},{},{line},.T.)",
                            verts[key.0], verts[key.1]
                        ));
                        edges.insert(key, e);
                        e
                    }
                };
                let sense = if a < b { ".T." } else { ".F." };
                oriented.push(self.add(format!("ORIENTED_EDGE('',*,*,{edge},{sense})")));
            }
            // Newell's normal of the polygon.
            let mut normal = [0.0; 3];
            for k in 0..n {
                let (a, b) = (p.points[f[k]], p.points[f[(k + 1) % n]]);
                normal[0] += (a[1] - b[1]) * (a[2] + b[2]);
                normal[1] += (a[2] - b[2]) * (a[0] + b[0]);
                normal[2] += (a[0] - b[0]) * (a[1] + b[1]);
            }
            let normal = unit(normal);
            let (a, b) = (p.points[f[0]], p.points[f[1]]);
            let x = unit([b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
            let x = unit(cross(cross(normal, x), normal));
            let axis = self.axis(a, normal, x);
            let plane = self.add(format!("PLANE('',{axis})"));
            let lp = self.add(format!("EDGE_LOOP('',{})", list(&oriented)));
            let bound = self.add(format!("FACE_OUTER_BOUND('',{lp},.T.)"));
            faces.push(self.add(format!("ADVANCED_FACE('',({bound}),{plane},.T.)")));
        }
        let shell = self.add(format!("CLOSED_SHELL('',{})", list(&faces)));
        self.add(format!("MANIFOLD_SOLID_BREP({},{shell})", string(name)))
    }
}
