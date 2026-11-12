//! Small STEP files for the reader's tests, written entity by entity with the writer's own
//! numbering (`crate::step::writer`): one for each kind of surface, a unit, an assembly.

use super::geometry::{P3, dot, sub};
use crate::step::writer::{Extrusion, Id, Writer, polygon};

pub const MM: &str = "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))";

pub struct F {
    pub w: Writer,
}

impl F {
    pub fn new() -> F {
        F {
            w: Writer::default(),
        }
    }

    pub fn vertex(&mut self, p: P3) -> Id {
        let q = self.w.point(p);
        self.w.add(format!("VERTEX_POINT('',{q})"))
    }

    pub fn frame(&mut self, o: P3, z: P3, x: P3) -> Id {
        self.w.axis(o, z, x)
    }

    pub fn edge(&mut self, a: Id, b: Id, curve: Id) -> Id {
        self.w.add(format!("EDGE_CURVE('',{a},{b},{curve},.T.)"))
    }

    pub fn line(&mut self, a: P3, b: P3) -> Id {
        let p = self.w.point(a);
        let d = sub(b, a);
        let l = dot(d, d).sqrt();
        let dir = self.w.direction(d.map(|c| c / l));
        let v = self.w.add(format!("VECTOR('',{dir},{l})"));
        self.w.add(format!("LINE('',{p},{v})"))
    }

    pub fn circle(&mut self, o: P3, r: f64) -> Id {
        let a = self.frame(o, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        self.w.add(format!("CIRCLE('',{a},{r:?})"))
    }

    /// A loop of edges, each forward or not.
    pub fn bound(&mut self, edges: &[(Id, bool)], outer: bool) -> Id {
        let oriented: Vec<String> = edges
            .iter()
            .map(|&(e, f)| {
                let id = self.w.add(format!(
                    "ORIENTED_EDGE('',*,*,{e},{})",
                    if f { ".T." } else { ".F." }
                ));
                id.to_string()
            })
            .collect();
        let lp = self
            .w
            .add(format!("EDGE_LOOP('',({}))", oriented.join(",")));
        let kind = if outer {
            "FACE_OUTER_BOUND"
        } else {
            "FACE_BOUND"
        };
        self.w.add(format!("{kind}('',{lp},.T.)"))
    }

    pub fn face(&mut self, bounds: &[Id], surface: Id, sense: bool) -> Id {
        let b: Vec<String> = bounds.iter().map(Id::to_string).collect();
        self.w.add(format!(
            "ADVANCED_FACE('',({}),{surface},{})",
            b.join(","),
            if sense { ".T." } else { ".F." }
        ))
    }

    pub fn solid(&mut self, name: &str, faces: &[Id]) -> Id {
        let f: Vec<String> = faces.iter().map(Id::to_string).collect();
        let shell = self.w.add(format!("CLOSED_SHELL('',({}))", f.join(",")));
        self.w.add(format!("MANIFOLD_SOLID_BREP('{name}',{shell})"))
    }

    /// The file with `items` in one representation whose length unit is `unit` (an instance).
    pub fn file(mut self, items: &[Id], unit: &str) -> String {
        let u = self.w.add(unit);
        let rad = self
            .w
            .add("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))");
        let ctx = self.w.add(format!(
            "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT(({u},{rad}))REPRESENTATION_CONTEXT('',''))"
        ));
        let axis = self.frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let mut all = vec![axis.to_string()];
        all.extend(items.iter().map(Id::to_string));
        self.w.add(format!(
            "ADVANCED_BREP_SHAPE_REPRESENTATION('',({}),{ctx})",
            all.join(",")
        ));
        self.w.file("FILE_DESCRIPTION((''),'2;1');\n")
    }

    /// A cylinder of `r` from z 0 to `h`, its side one face with a seam (as OpenCascade writes it).
    pub fn cylinder(&mut self, r: f64, h: f64) -> Id {
        let (v0, v1) = (self.vertex([r, 0.0, 0.0]), self.vertex([r, 0.0, h]));
        let cb = self.circle([0.0; 3], r);
        let ct = self.circle([0.0, 0.0, h], r);
        let bottom = self.edge(v0, v0, cb);
        let top = self.edge(v1, v1, ct);
        let seam_line = self.line([r, 0.0, 0.0], [r, 0.0, h]);
        let seam = self.edge(v0, v1, seam_line);
        let axis = self.frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let cyl = self.w.add(format!("CYLINDRICAL_SURFACE('',{axis},{r:?})"));
        let side_loop = self.bound(
            &[(bottom, true), (seam, true), (top, false), (seam, false)],
            true,
        );
        let side = self.face(&[side_loop], cyl, true);
        let (bot, tp) = self.caps(bottom, top, h);
        self.solid("cylinder", &[side, bot, tp])
    }

    /// The flat ends of a round solid from z 0 to `h`: their circles' edges `bottom` and `top`.
    pub fn caps(&mut self, bottom: Id, top: Id, h: f64) -> (Id, Id) {
        let a0 = self.frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let p0 = self.w.add(format!("PLANE('',{a0})"));
        let l0 = self.bound(&[(bottom, false)], true);
        let f0 = self.face(&[l0], p0, false);
        let a1 = self.frame([0.0, 0.0, h], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let p1 = self.w.add(format!("PLANE('',{a1})"));
        let l1 = self.bound(&[(top, true)], true);
        let f1 = self.face(&[l1], p1, true);
        (f0, f1)
    }
}

/// A cylinder of radius 2 and height 3 with a seam.
pub fn cylinder() -> String {
    let mut f = F::new();
    let s = f.cylinder(2.0, 3.0);
    f.file(&[s], MM)
}

/// A ball of radius 2 as two half spheres meeting at their equator.
pub fn sphere() -> String {
    let mut f = F::new();
    let r = 2.0;
    let v = f.vertex([r, 0.0, 0.0]);
    let c = f.circle([0.0; 3], r);
    let equator = f.edge(v, v, c);
    let axis = f.frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let sphere = f.w.add(format!("SPHERICAL_SURFACE('',{axis},{r:?})"));
    let up = f.bound(&[(equator, true)], true);
    let down = f.bound(&[(equator, false)], true);
    let top = f.face(&[up], sphere, true);
    let bottom = f.face(&[down], sphere, true);
    let s = f.solid("ball", &[top, bottom]);
    f.file(&[s], MM)
}

/// A cone's side from z 0 (radius `r0`) to `h`, as a frustum (two circles, no seam) or, with
/// `r0` 0, up from its apex.
pub fn cone(r0: f64, h: f64) -> String {
    let mut f = F::new();
    let a = 0.3_f64;
    let r1 = r0 + h * a.tan();
    let axis = f.frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let cone = f.w.add(format!("CONICAL_SURFACE('',{axis},{r0:?},{a:?})"));
    let vt = f.vertex([r1, 0.0, h]);
    let ct = f.circle([0.0, 0.0, h], r1);
    let top = f.edge(vt, vt, ct);
    let solid = if r0 > 0.0 {
        let vb = f.vertex([r0, 0.0, 0.0]);
        let cb = f.circle([0.0; 3], r0);
        let bottom = f.edge(vb, vb, cb);
        let lb = f.bound(&[(bottom, true)], true);
        let lt = f.bound(&[(top, false)], false);
        let side = f.face(&[lb, lt], cone, true);
        let (b, t) = f.caps(bottom, top, h);
        f.solid("frustum", &[side, b, t])
    } else {
        let apex = f.vertex([0.0; 3]);
        let vl = f.w.add(format!("VERTEX_LOOP('',{apex})"));
        let vb = f.w.add(format!("FACE_BOUND('',{vl},.T.)"));
        let lt = f.bound(&[(top, false)], true);
        let side = f.face(&[lt, vb], cone, true);
        let a1 = f.frame([0.0, 0.0, h], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let p1 = f.w.add(format!("PLANE('',{a1})"));
        let l1 = f.bound(&[(top, true)], true);
        let cap = f.face(&[l1], p1, true);
        f.solid("cone", &[side, cap])
    };
    f.file(&[solid], MM)
}

/// A whole torus (major radius 3, minor 1) round (1, 2, 3): one face with no loop.
pub fn torus() -> String {
    let mut f = F::new();
    let axis = f.frame([1.0, 2.0, 3.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let torus = f.w.add(format!("TOROIDAL_SURFACE('',{axis},3.,1.)"));
    let face = f.face(&[], torus, true);
    let s = f.solid("ring", &[face]);
    f.file(&[s], MM)
}

/// The control points of a bicubic patch over 3 x 3 mm, bumped in z.
pub fn patch_points() -> Vec<Vec<P3>> {
    let bump = [
        [0.0, 0.2, 0.1, 0.0],
        [0.3, 1.0, -0.5, 0.2],
        [0.0, 0.6, 0.8, -0.3],
        [0.1, 0.0, 0.2, 0.0],
    ];
    (0..4)
        .map(|i| (0..4).map(|j| [i as f64, j as f64, bump[i][j]]).collect())
        .collect()
}

/// The bicubic patch of [`patch_points`] as one face of an open shell, bounded by its four
/// edges as B-spline curves.
pub fn patch() -> String {
    let mut f = F::new();
    let pts = patch_points();
    let ids: Vec<Vec<Id>> = pts
        .iter()
        .map(|r| r.iter().map(|&p| f.w.point(p)).collect())
        .collect();
    let row = |ids: &[Id]| ids.iter().map(Id::to_string).collect::<Vec<_>>().join(",");
    let grid: Vec<String> = ids.iter().map(|r| format!("({})", row(r))).collect();
    let surface = f.w.add(format!(
        "B_SPLINE_SURFACE_WITH_KNOTS('',3,3,({}),.UNSPECIFIED.,.F.,.F.,.F.,(4,4),(4,4),(0.,1.),(0.,1.),.UNSPECIFIED.)",
        grid.join(",")
    ));
    let curve = |f: &mut F, c: Vec<Id>| {
        f.w.add(format!(
            "B_SPLINE_CURVE_WITH_KNOTS('',3,({}),.UNSPECIFIED.,.F.,.F.,(4,4),(0.,1.),.UNSPECIFIED.)",
            row(&c)
        ))
    };
    let corner = |f: &mut F, i: usize, j: usize| f.vertex(pts[i][j]);
    let (v00, v30, v33, v03) = (
        corner(&mut f, 0, 0),
        corner(&mut f, 3, 0),
        corner(&mut f, 3, 3),
        corner(&mut f, 0, 3),
    );
    let c_v0 = curve(&mut f, (0..4).map(|i| ids[i][0]).collect());
    let c_u1 = curve(&mut f, (0..4).map(|j| ids[3][j]).collect());
    let c_v1 = curve(&mut f, (0..4).map(|i| ids[i][3]).collect());
    let c_u0 = curve(&mut f, (0..4).map(|j| ids[0][j]).collect());
    let e1 = f.edge(v00, v30, c_v0);
    let e2 = f.edge(v30, v33, c_u1);
    let e3 = f.edge(v03, v33, c_v1);
    let e4 = f.edge(v00, v03, c_u0);
    let lp = f.bound(&[(e1, true), (e2, true), (e3, false), (e4, false)], true);
    let face = f.face(&[lp], surface, true);
    let shell = f.w.add(format!("OPEN_SHELL('',({face}))"));
    let model =
        f.w.add(format!("SHELL_BASED_SURFACE_MODEL('patch',({shell}))"));
    f.file(&[model], MM)
}

/// A quarter of a cylinder of radius 2 and height 1 as a rational quadratic B-spline surface,
/// bounded by two rational arcs and two lines.
pub fn rational_quarter() -> String {
    let mut f = F::new();
    let (r, h) = (2.0, 1.0);
    let w = std::f64::consts::FRAC_1_SQRT_2;
    let p = |x: f64, y: f64, z: f64| [x, y, z];
    let grid = [
        [p(r, 0.0, 0.0), p(r, 0.0, h)],
        [p(r, r, 0.0), p(r, r, h)],
        [p(0.0, r, 0.0), p(0.0, r, h)],
    ];
    let ids: Vec<Vec<Id>> = grid
        .iter()
        .map(|row| row.iter().map(|&q| f.w.point(q)).collect())
        .collect();
    let rows: Vec<String> = ids
        .iter()
        .map(|row| {
            format!(
                "({})",
                row.iter().map(Id::to_string).collect::<Vec<_>>().join(",")
            )
        })
        .collect();
    let surface = f.w.add(format!(
        "(BOUNDED_SURFACE()B_SPLINE_SURFACE(2,1,({}),.UNSPECIFIED.,.F.,.F.,.F.)B_SPLINE_SURFACE_WITH_KNOTS((3,3),(2,2),(0.,1.),(0.,1.),.UNSPECIFIED.)GEOMETRIC_REPRESENTATION_ITEM()RATIONAL_B_SPLINE_SURFACE(((1.,1.),({w:?},{w:?}),(1.,1.)))REPRESENTATION_ITEM('')SURFACE())",
        rows.join(",")
    ));
    let arc = |f: &mut F, z: usize| {
        f.w.add(format!(
            "(BOUNDED_CURVE()B_SPLINE_CURVE(2,({},{},{}),.CIRCULAR_ARC.,.F.,.F.)B_SPLINE_CURVE_WITH_KNOTS((3,3),(0.,1.),.UNSPECIFIED.)CURVE()GEOMETRIC_REPRESENTATION_ITEM()RATIONAL_B_SPLINE_CURVE((1.,{w:?},1.))REPRESENTATION_ITEM(''))",
            ids[0][z], ids[1][z], ids[2][z]
        ))
    };
    let (a0, a1) = (arc(&mut f, 0), arc(&mut f, 1));
    let v = [[0, 0], [2, 0], [2, 1], [0, 1]].map(|[i, j]| f.vertex(grid[i][j]));
    let e_bottom = f.edge(v[0], v[1], a0);
    let l_end = f.line(grid[2][0], grid[2][1]);
    let e_end = f.edge(v[1], v[2], l_end);
    let e_top = f.edge(v[3], v[2], a1);
    let l_start = f.line(grid[0][0], grid[0][1]);
    let e_start = f.edge(v[0], v[3], l_start);
    let lp = f.bound(
        &[
            (e_bottom, true),
            (e_end, true),
            (e_top, false),
            (e_start, false),
        ],
        true,
    );
    let face = f.face(&[lp], surface, true);
    let shell = f.w.add(format!("OPEN_SHELL('',({face}))"));
    let model =
        f.w.add(format!("SHELL_BASED_SURFACE_MODEL('quarter',({shell}))"));
    f.file(&[model], MM)
}

pub fn block(f: &mut F, size: [f64; 3]) -> Id {
    let e = Extrusion {
        origin: [0.0; 3],
        u: [1.0, 0.0, 0.0],
        v: [0.0, 1.0, 0.0],
        n: [0.0, 0.0, 1.0],
        loops: vec![polygon(&[
            [0.0, 0.0],
            [size[0], 0.0],
            [size[0], size[1]],
            [0.0, size[1]],
        ])],
        from: 0.0,
        to: size[2],
    }
    .normalised()
    .unwrap();
    f.w.extrusion("block", &e)
}

/// A block of 1 x 0.5 x 0.25 inches, in a file whose unit is the inch.
pub fn inch_block() -> String {
    let mut f = F::new();
    let s = block(&mut f, [1.0, 0.5, 0.25]);
    let mut text = f.file(&[s], "PLACEHOLDER");
    // The inch: a conversion of the millimetre.
    let n = text.matches("\n#").count();
    text = text.replace(
        "PLACEHOLDER",
        &format!(
            "(CONVERSION_BASED_UNIT('INCH',#{})LENGTH_UNIT()NAMED_UNIT(*))",
            n + 1
        ),
    );
    text.replace(
        "ENDSEC;\nEND-ISO",
        &format!(
            "#{}=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#{});\n#{}={MM};\nENDSEC;\nEND-ISO",
            n + 1,
            n + 2,
            n + 2
        ),
    )
}

/// A 2 x 1 x 1 block placed twice in a root: raised to z = 5 by a mapped item, and turned a
/// quarter about z and moved to x = 10 by a transformation.
pub fn assembly() -> String {
    let mut f = F::new();
    let s = block(&mut f, [2.0, 1.0, 1.0]);
    let mm = f.w.add(MM);
    let ctx = f.w.add(format!(
        "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT(({mm}))REPRESENTATION_CONTEXT('',''))"
    ));
    let child_axis = f.frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let child = f.w.add(format!(
        "ADVANCED_BREP_SHAPE_REPRESENTATION('part',({child_axis},{s}),{ctx})"
    ));
    let at = f.frame([10.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
    let map_origin = f.frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let map = f.w.add(format!("REPRESENTATION_MAP({map_origin},{child})"));
    let target = f.frame([0.0, 0.0, 5.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let mapped = f.w.add(format!("MAPPED_ITEM('',{map},{target})"));
    let root_axis = f.frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let root = f.w.add(format!(
        "SHAPE_REPRESENTATION('root',({root_axis},{at},{mapped}),{ctx})"
    ));
    let t = f.w.add(format!(
        "ITEM_DEFINED_TRANSFORMATION('','',{child_axis},{at})"
    ));
    f.w.add(format!(
        "(REPRESENTATION_RELATIONSHIP('','',{child},{root})REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION({t})SHAPE_REPRESENTATION_RELATIONSHIP())"
    ));
    f.w.file("FILE_DESCRIPTION((''),'2;1');\n")
}
