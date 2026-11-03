//! The copper shape of a pad on the board, and how it is drawn in a Gerber file: a flash of
//! a standard or macro aperture where the shape allows it, a region where it does not (a
//! trapezoid, a chamfered rectangle, a custom pad). The same code draws the pad for copper,
//! for the mask and paste openings (the shape grown or shrunk by a margin) and for the
//! clearance a zone keeps round it.

use crate::artwork::{Pad, PadShape, Primitive};
use crate::gbr::{Ap, Contour, Gerber};
use crate::geom::{self, Arc, Bounds, Pt, circle_polygon, is_right_angle, steps_for};

/// A custom pad's primitive on the board.
#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    Line {
        a: Pt,
        b: Pt,
        width: f64,
    },
    Arc {
        arc: Arc,
        width: f64,
    },
    Circle {
        c: Pt,
        r: f64,
        width: f64,
        fill: bool,
    },
    Poly {
        points: Vec<Pt>,
        width: f64,
        fill: bool,
    },
}

/// A pad's shape on the board.
#[derive(Debug, Clone, PartialEq)]
pub enum Outline {
    Circle {
        c: Pt,
        r: f64,
    },
    /// A rectangle `w` by `h` (before the turn) turned by `angle`.
    Rect {
        c: Pt,
        w: f64,
        h: f64,
        angle: f64,
    },
    Oval {
        c: Pt,
        w: f64,
        h: f64,
        angle: f64,
    },
    RoundRect {
        c: Pt,
        w: f64,
        h: f64,
        r: f64,
        angle: f64,
    },
    /// A convex polygon.
    Poly(Vec<Pt>),
    Custom {
        anchor: Box<Outline>,
        prims: Vec<Prim>,
    },
}

/// The shape of `pad`.
pub fn outline(pad: &Pad) -> Outline {
    let (w, h) = pad.size;
    let (c, angle) = (pad.at, pad.angle);
    let turn = |p: Pt| pad.at + p.rotate(angle);
    match pad.shape {
        PadShape::Circle => Outline::Circle { c, r: w / 2.0 },
        PadShape::Oval if (w - h).abs() < 1e-9 => Outline::Circle { c, r: w / 2.0 },
        PadShape::Oval => Outline::Oval { c, w, h, angle },
        PadShape::Rect | PadShape::RoundRect => {
            let r = if pad.shape == PadShape::RoundRect {
                pad.roundrect_ratio * w.min(h)
            } else {
                0.0
            };
            if pad.chamfer_ratio > 0.0 && pad.chamfer.iter().any(|c| *c) {
                Outline::Poly(chamfered(pad, r))
            } else if r > 1e-9 {
                Outline::RoundRect { c, w, h, r, angle }
            } else {
                Outline::Rect { c, w, h, angle }
            }
        }
        PadShape::Trapezoid => {
            let (hx, hy) = (w / 2.0, h / 2.0);
            let (dx, dy) = (pad.rect_delta.0 / 2.0, pad.rect_delta.1 / 2.0);
            Outline::Poly(
                [
                    Pt::new(-hx - dy, hy + dx),
                    Pt::new(-hx + dy, -hy - dx),
                    Pt::new(hx - dy, -hy + dx),
                    Pt::new(hx + dy, hy - dx),
                ]
                .map(turn)
                .to_vec(),
            )
        }
        PadShape::Custom => {
            let anchor = if pad.anchor_rect {
                Outline::Rect { c, w, h, angle }
            } else {
                Outline::Circle { c, r: w / 2.0 }
            };
            let prims = pad
                .primitives
                .iter()
                .map(|p| match p {
                    Primitive::Line { a, b, width } => Prim::Line {
                        a: turn(*a),
                        b: turn(*b),
                        width: *width,
                    },
                    Primitive::Arc { arc, width } => Prim::Arc {
                        arc: Arc {
                            start: turn(arc.start),
                            end: turn(arc.end),
                            centre: turn(arc.centre),
                            radius: arc.radius,
                            ccw: arc.ccw,
                        },
                        width: *width,
                    },
                    Primitive::Circle {
                        centre,
                        radius,
                        width,
                        fill,
                    } => Prim::Circle {
                        c: turn(*centre),
                        r: *radius,
                        width: *width,
                        fill: *fill,
                    },
                    Primitive::Poly {
                        points,
                        width,
                        fill,
                    } => Prim::Poly {
                        points: points.iter().map(|p| turn(*p)).collect(),
                        width: *width,
                        fill: *fill,
                    },
                })
                .collect();
            Outline::Custom {
                anchor: Box::new(anchor),
                prims,
            }
        }
    }
}

/// A rectangle with chamfered corners and, if `radius` is not zero, rounded others.
fn chamfered(pad: &Pad, radius: f64) -> Vec<Pt> {
    let (w, h) = pad.size;
    let (hx, hy) = (w / 2.0, h / 2.0);
    let cut = pad.chamfer_ratio * w.min(h);
    // Corners in the order top-left, top-right, bottom-right, bottom-left (y down), with the
    // direction to the next corner.
    let corners = [
        (Pt::new(-hx, -hy), pad.chamfer[0]),
        (Pt::new(hx, -hy), pad.chamfer[1]),
        (Pt::new(hx, hy), pad.chamfer[3]),
        (Pt::new(-hx, hy), pad.chamfer[2]),
    ];
    let mut pts = Vec::new();
    for i in 0..4 {
        let (p, chamfer) = corners[i];
        let prev = corners[(i + 3) % 4].0;
        let next = corners[(i + 1) % 4].0;
        let to_prev = unit(prev - p);
        let to_next = unit(next - p);
        if chamfer {
            pts.push(p + to_prev * cut);
            pts.push(p + to_next * cut);
        } else if radius > 1e-9 {
            // A quarter circle about the point `radius` in from both sides.
            let centre = p + to_prev * radius + to_next * radius;
            let a0 = (p + to_prev * radius) - centre;
            let a1 = (p + to_next * radius) - centre;
            let (s, e) = ((-a0.y).atan2(a0.x), (-a1.y).atan2(a1.x));
            let mut d = e - s;
            while d > std::f64::consts::PI {
                d -= std::f64::consts::TAU;
            }
            while d < -std::f64::consts::PI {
                d += std::f64::consts::TAU;
            }
            let steps = steps_for(radius, d.abs());
            for k in 0..=steps {
                let a = s + d * k as f64 / steps as f64;
                pts.push(Pt::new(
                    centre.x + radius * a.cos(),
                    centre.y - radius * a.sin(),
                ));
            }
        } else {
            pts.push(p);
        }
    }
    pts.into_iter()
        .map(|p| pad.at + p.rotate(pad.angle))
        .collect()
}

fn unit(v: Pt) -> Pt {
    let n = v.x.hypot(v.y);
    if n == 0.0 { v } else { v * (1.0 / n) }
}

/// A rounded rectangle (a rectangle if `r` is 0, a stadium if `r` is half the shorter side)
/// as a polygon, turned by `angle` about its centre.
pub fn rounded_rect_polygon(c: Pt, w: f64, h: f64, r: f64, angle: f64) -> Vec<Pt> {
    let r = r.min(w.min(h) / 2.0).max(0.0);
    let (hx, hy) = (w / 2.0 - r, h / 2.0 - r);
    let mut pts = Vec::new();
    if r < 1e-9 {
        pts.extend([
            Pt::new(-hx, -hy),
            Pt::new(hx, -hy),
            Pt::new(hx, hy),
            Pt::new(-hx, hy),
        ]);
    } else {
        let steps = steps_for(r, std::f64::consts::FRAC_PI_2);
        // Corner centres with the angle (screen sense, y down: `(cos, -sin)`) the quarter starts at.
        for (centre, start) in [
            (Pt::new(hx, -hy), 0.0),
            (Pt::new(-hx, -hy), std::f64::consts::FRAC_PI_2),
            (Pt::new(-hx, hy), std::f64::consts::PI),
            (Pt::new(hx, hy), 3.0 * std::f64::consts::FRAC_PI_2),
        ] {
            for k in 0..=steps {
                let a = start + std::f64::consts::FRAC_PI_2 * k as f64 / steps as f64;
                pts.push(Pt::new(centre.x + r * a.cos(), centre.y - r * a.sin()));
            }
        }
    }
    pts.into_iter().map(|p| c + p.rotate(angle)).collect()
}

/// The corners of a polygon in the order where the shoelace sum is positive.
fn positive(poly: &[Pt]) -> Vec<Pt> {
    if geom::area2(poly) >= 0.0 {
        poly.to_vec()
    } else {
        geom::reversed(poly)
    }
}

/// A convex polygon moved outward by `d` with round corners (`d` > 0), or inward (`d` < 0).
/// `None` if nothing is left.
pub fn offset_convex(poly: &[Pt], d: f64) -> Option<Vec<Pt>> {
    let poly = positive(poly);
    let n = poly.len();
    if n < 3 {
        return None;
    }
    if d == 0.0 {
        return Some(poly);
    }
    // Outward normal of the edge i -> i+1 (interior on the left for a positive sum).
    let normal = |i: usize| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        unit(Pt::new(b.y - a.y, -(b.x - a.x)))
    };
    if d < 0.0 {
        let mut out = poly.clone();
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let shift = normal(i) * d;
            out = geom::clip_half_plane(&out, a + shift, b - a);
            if out.len() < 3 {
                return None;
            }
        }
        return Some(out);
    }
    let mut out = Vec::new();
    for (i, vertex) in poly.iter().enumerate() {
        let (n0, n1) = (normal((i + n - 1) % n), normal(i));
        let (a0, a1) = (n0.y.atan2(n0.x), n1.y.atan2(n1.x));
        let delta = (a1 - a0).rem_euclid(std::f64::consts::TAU);
        let steps = steps_for(d, delta);
        for k in 0..=steps {
            let a = a0 + delta * k as f64 / steps as f64;
            out.push(Pt::new(vertex.x + d * a.cos(), vertex.y + d * a.sin()));
        }
    }
    Some(out)
}

impl Outline {
    /// The shape with its edge moved out by `d` (or in, if `d` is negative): the opening a
    /// margin makes, or the clearance a zone keeps. `Ok(None)` if nothing is left.
    pub fn grow(&self, d: f64) -> Result<Option<Outline>, String> {
        if d == 0.0 {
            return Ok(Some(self.clone()));
        }
        Ok(match self {
            Outline::Circle { c, r } => (r + d > 1e-9).then(|| Outline::Circle { c: *c, r: r + d }),
            Outline::Rect { c, w, h, angle } => {
                if w + 2.0 * d <= 1e-9 || h + 2.0 * d <= 1e-9 {
                    None
                } else if d > 0.0 {
                    Some(Outline::RoundRect {
                        c: *c,
                        w: w + 2.0 * d,
                        h: h + 2.0 * d,
                        r: d,
                        angle: *angle,
                    })
                } else {
                    Some(Outline::Rect {
                        c: *c,
                        w: w + 2.0 * d,
                        h: h + 2.0 * d,
                        angle: *angle,
                    })
                }
            }
            Outline::Oval { c, w, h, angle } => {
                let (w, h) = (w + 2.0 * d, h + 2.0 * d);
                (w > 1e-9 && h > 1e-9).then_some(Outline::Oval {
                    c: *c,
                    w,
                    h,
                    angle: *angle,
                })
            }
            Outline::RoundRect { c, w, h, r, angle } => {
                let (w, h) = (w + 2.0 * d, h + 2.0 * d);
                (w > 1e-9 && h > 1e-9).then(|| Outline::RoundRect {
                    c: *c,
                    w,
                    h,
                    r: (r + d).max(0.0),
                    angle: *angle,
                })
            }
            Outline::Poly(points) => offset_convex(points, d).map(Outline::Poly),
            Outline::Custom { anchor, prims } => {
                if d < 0.0 {
                    return Err("a custom pad cannot be shrunk".to_string());
                }
                let anchor = anchor.grow(d)?.map(Box::new);
                let Some(anchor) = anchor else {
                    return Ok(None);
                };
                let prims = prims
                    .iter()
                    .map(|p| match p {
                        Prim::Line { a, b, width } => Prim::Line {
                            a: *a,
                            b: *b,
                            width: width + 2.0 * d,
                        },
                        Prim::Arc { arc, width } => Prim::Arc {
                            arc: *arc,
                            width: width + 2.0 * d,
                        },
                        Prim::Circle { c, r, width, fill } => Prim::Circle {
                            c: *c,
                            r: *r,
                            width: width + 2.0 * d,
                            fill: *fill,
                        },
                        Prim::Poly {
                            points,
                            width,
                            fill,
                        } => Prim::Poly {
                            points: points.clone(),
                            width: width + 2.0 * d,
                            fill: *fill,
                        },
                    })
                    .collect();
                Some(Outline::Custom { anchor, prims })
            }
        })
    }

    /// The centre of the shape's box.
    pub fn bounds(&self) -> Bounds {
        match self {
            Outline::Circle { c, r } => Bounds {
                min: Pt::new(c.x - r, c.y - r),
                max: Pt::new(c.x + r, c.y + r),
            },
            Outline::Rect { c, w, h, angle }
            | Outline::Oval { c, w, h, angle }
            | Outline::RoundRect { c, w, h, angle, .. } => {
                let pts = rounded_rect_polygon(*c, *w, *h, 0.0, *angle);
                Bounds::of(pts).unwrap_or(Bounds { min: *c, max: *c })
            }
            Outline::Poly(p) => Bounds::of(p.iter().copied()).unwrap_or(Bounds {
                min: Pt::default(),
                max: Pt::default(),
            }),
            Outline::Custom { anchor, prims } => {
                let mut b = anchor.bounds();
                for p in prims {
                    let (pts, w): (Vec<Pt>, f64) = match p {
                        Prim::Line { a, b, width } => (vec![*a, *b], *width),
                        Prim::Arc { arc, width } => (arc.points(), *width),
                        Prim::Circle { c, r, width, .. } => (
                            vec![Pt::new(c.x - r, c.y - r), Pt::new(c.x + r, c.y + r)],
                            *width,
                        ),
                        Prim::Poly { points, width, .. } => (points.clone(), *width),
                    };
                    for q in pts {
                        b.add(Pt::new(q.x - w / 2.0, q.y - w / 2.0));
                        b.add(Pt::new(q.x + w / 2.0, q.y + w / 2.0));
                    }
                }
                b
            }
        }
    }

    /// The shape as a convex polygon (cut up if it has curves), or `None` for a custom pad.
    pub fn polygon(&self) -> Option<Vec<Pt>> {
        match self {
            Outline::Circle { c, r } => Some(circle_polygon(*c, *r)),
            Outline::Rect { c, w, h, angle } => Some(rounded_rect_polygon(*c, *w, *h, 0.0, *angle)),
            Outline::Oval { c, w, h, angle } => {
                Some(rounded_rect_polygon(*c, *w, *h, w.min(*h) / 2.0, *angle))
            }
            Outline::RoundRect { c, w, h, r, angle } => {
                Some(rounded_rect_polygon(*c, *w, *h, *r, *angle))
            }
            Outline::Poly(p) => Some(p.clone()),
            Outline::Custom { .. } => None,
        }
    }

    /// Draws the shape as copper (dark, or clear if the file is in clear polarity).
    pub fn draw(&self, g: &mut Gerber, function: Option<&'static str>, attrs: &[String]) {
        match self {
            Outline::Circle { c, r } => g.flash(Ap::Circle(2.0 * r), function, *c, attrs),
            Outline::Rect { c, w, h, angle } => {
                if is_right_angle(*angle) {
                    let quarter = ((angle / 90.0).round() as i64).rem_euclid(2) == 1;
                    let (w, h) = if quarter { (*h, *w) } else { (*w, *h) };
                    g.flash(Ap::Rect(w, h), function, *c, attrs);
                } else {
                    g.flash(
                        Ap::RotRect {
                            w: *w,
                            h: *h,
                            angle: angle.rem_euclid(360.0),
                        },
                        function,
                        *c,
                        attrs,
                    );
                }
            }
            Outline::Oval { c, w, h, angle } => {
                if is_right_angle(*angle) {
                    let quarter = ((angle / 90.0).round() as i64).rem_euclid(2) == 1;
                    let (w, h) = if quarter { (*h, *w) } else { (*w, *h) };
                    g.flash(Ap::Obround(w, h), function, *c, attrs);
                } else {
                    // A thick line between the centres of the round ends, in the file's frame.
                    let (long, short) = (w.max(*h), w.min(*h));
                    let half = (long - short) / 2.0;
                    let along = if w >= h { 0.0 } else { 90.0 };
                    let (s, co) = geom::sin_cos(angle + along);
                    // Screen angle -> file frame (y up): the same counter-clockwise turn.
                    let end = Pt::new(half * co, half * s);
                    g.flash(
                        Ap::HorizOval {
                            width: short,
                            a: Pt::new(-end.x, -end.y),
                            b: end,
                        },
                        function,
                        *c,
                        attrs,
                    );
                }
            }
            Outline::RoundRect { c, w, h, r, angle } => {
                let (hx, hy) = (w / 2.0 - r, h / 2.0 - r);
                let (s, co) = geom::sin_cos(*angle);
                // The corners in the file's frame (y up), turned counter-clockwise.
                let corners = [(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy)]
                    .map(|(x, y)| Pt::new(x * co - y * s, x * s + y * co));
                g.flash(
                    Ap::RoundRect {
                        radius: *r,
                        corners,
                    },
                    function,
                    *c,
                    attrs,
                );
            }
            Outline::Poly(points) => g.region(Contour::polygon(points), function, attrs),
            Outline::Custom { anchor, prims } => {
                anchor.draw(g, function, attrs);
                for p in prims {
                    draw_prim(g, p, function, attrs);
                }
            }
        }
    }
}

/// A primitive of a custom pad as copper.
fn draw_prim(g: &mut Gerber, p: &Prim, function: Option<&'static str>, attrs: &[String]) {
    match p {
        Prim::Line { a, b, width } => {
            if *width > 0.0 {
                if a == b {
                    g.flash(Ap::Circle(*width), function, *a, attrs);
                } else {
                    g.line(*width, function, *a, *b, attrs);
                }
            }
        }
        Prim::Arc { arc, width } => {
            if *width > 0.0 {
                g.arc(*width, function, arc, attrs);
            }
        }
        Prim::Circle { c, r, width, fill } => {
            if *fill {
                g.flash(Ap::Circle(2.0 * r + width), function, *c, attrs);
            } else if *width > 0.0 {
                let start = Pt::new(c.x + r, c.y);
                g.arc(
                    *width,
                    function,
                    &Arc {
                        start,
                        end: start,
                        centre: *c,
                        radius: *r,
                        ccw: true,
                    },
                    attrs,
                );
            }
        }
        Prim::Poly {
            points,
            width,
            fill,
        } => {
            if *fill && points.len() >= 3 {
                g.region(Contour::polygon(points), function, attrs);
            }
            if *width > 0.0 {
                for i in 0..points.len() {
                    let (a, b) = (points[i], points[(i + 1) % points.len()]);
                    if a != b && (i + 1 < points.len() || points.len() > 2) {
                        g.line(*width, function, a, b, attrs);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(shape: PadShape, size: (f64, f64), angle: f64) -> Pad {
        Pad {
            part: "R1".into(),
            number: "1".into(),
            function: None,
            net: None,
            kind: crate::artwork::PadKind::Smd,
            shape,
            at: Pt::new(10.0, 20.0),
            angle,
            size,
            roundrect_ratio: 0.25,
            rect_delta: (0.0, 0.0),
            chamfer_ratio: 0.0,
            chamfer: [false; 4],
            primitives: Vec::new(),
            anchor_rect: false,
            drill: None,
            layers: vec!["F.Cu".into()],
            mask_margin: None,
            paste_margin: None,
            paste_ratio: None,
            zone_connect: None,
            remove_unused_layers: false,
            clearance: None,
        }
    }

    fn text(o: &Outline) -> String {
        let mut g = Gerber::new(Pt::default());
        o.draw(&mut g, None, &[]);
        g.text("Copper,L1,Top", false, "B")
    }

    #[test]
    fn right_angle_rectangles_and_ovals_use_the_standard_apertures() {
        let t = text(&outline(&pad(PadShape::Rect, (1.0, 2.0), 90.0)));
        assert!(t.contains("%ADD10R,2.000000X1.000000*%"), "{t}");
        let t = text(&outline(&pad(PadShape::Oval, (1.0, 2.0), 0.0)));
        assert!(t.contains("%ADD10O,1.000000X2.000000*%"), "{t}");
        let t = text(&outline(&pad(PadShape::Oval, (1.5, 1.5), 0.0)));
        assert!(
            t.contains("%ADD10C,1.500000*%"),
            "an oval that is round is a circle: {t}"
        );
    }

    #[test]
    fn a_turned_rectangle_is_a_macro_with_the_angle_and_a_turned_roundrect_has_turned_corners() {
        let t = text(&outline(&pad(PadShape::Rect, (1.0, 2.0), 30.0)));
        assert!(
            t.contains("%ADD10RotRect,1.000000X2.000000X30.000000*%"),
            "{t}"
        );
        // 0.8 x 0.4 with ratio 0.25 -> radius 0.1; turned by 90 the long side is vertical, so
        // the inner rectangle's corner (-0.3, -0.1) becomes (0.1, -0.3) in the file's frame.
        let t = text(&outline(&pad(PadShape::RoundRect, (0.8, 0.4), 90.0)));
        assert!(t.contains("%ADD10RoundRect,0.100000X0.100000X-0.300000X0.100000X0.300000X-0.100000X0.300000X-0.100000X-0.300000X0*%"), "{t}");
        let t = text(&outline(&pad(PadShape::Oval, (1.0, 3.0), 45.0)));
        assert!(t.contains("HorizOval,1.000000"), "{t}");
    }

    #[test]
    fn growing_makes_the_opening_a_margin_asks_for() {
        let o = outline(&pad(PadShape::Rect, (1.0, 2.0), 0.0));
        // A rectangle grown has round corners of the margin's radius.
        match o.grow(0.05).unwrap().unwrap() {
            Outline::RoundRect { w, h, r, .. } => assert_eq!((w, h, r), (1.1, 2.1, 0.05)),
            other => panic!("{other:?}"),
        }
        assert!(o.grow(-0.6).unwrap().is_none(), "shrunk to nothing");
        let c = outline(&pad(PadShape::Circle, (1.0, 1.0), 0.0));
        assert_eq!(
            c.grow(-0.1).unwrap().unwrap(),
            Outline::Circle {
                c: Pt::new(10.0, 20.0),
                r: 0.4
            }
        );
    }

    #[test]
    fn a_trapezoid_grows_and_shrinks_like_its_polygon() {
        let mut p = pad(PadShape::Trapezoid, (2.0, 1.0), 0.0);
        p.rect_delta = (0.0, 0.4);
        let o = outline(&p);
        let grown = o.grow(0.1).unwrap().unwrap().polygon().unwrap();
        let shrunk = o.grow(-0.1).unwrap().unwrap().polygon().unwrap();
        let area = |p: &[Pt]| geom::area2(p).abs() / 2.0;
        let base = area(&o.polygon().unwrap());
        assert!(area(&grown) > base && area(&shrunk) < base);
    }

    #[test]
    fn a_chamfered_corner_is_cut() {
        let mut p = pad(PadShape::Rect, (2.0, 1.0), 0.0);
        p.chamfer_ratio = 0.25;
        p.chamfer = [true, false, false, false];
        let poly = outline(&p).polygon().unwrap();
        assert_eq!(poly.len(), 5);
        // The top-left corner (9, 19.5) is not a corner any more.
        assert!(poly.iter().all(|q| q.dist(Pt::new(9.0, 19.5)) > 0.2));
    }

    #[test]
    fn a_custom_pad_grows_by_widening_its_strokes_and_cannot_shrink() {
        let mut p = pad(PadShape::Custom, (0.5, 0.5), 0.0);
        p.primitives = vec![Primitive::Line {
            a: Pt::new(0.0, 0.0),
            b: Pt::new(1.0, 0.0),
            width: 0.2,
        }];
        let o = outline(&p);
        match o.grow(0.1).unwrap().unwrap() {
            Outline::Custom { prims, .. } => match &prims[0] {
                Prim::Line { width, .. } => assert!((width - 0.4).abs() < 1e-12),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        assert!(o.grow(-0.1).is_err());
    }
}
