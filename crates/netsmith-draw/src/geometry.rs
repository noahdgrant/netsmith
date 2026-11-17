//! The geometry the reader and the rasterizer share: affine transforms, arcs and circles as
//! cubic curves, curves as lines, dashes, and a line's outline as polygons.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use crate::{Cap, Path, Pt, Seg};

/// An affine transform, SVG's `matrix(a b c d e f)`: `(x, y)` goes to
/// `(a x + c y + e, b x + d y + f)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Affine(pub [f64; 6]);

impl Affine {
    pub const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn translate(x: f64, y: f64) -> Affine {
        Affine([1.0, 0.0, 0.0, 1.0, x, y])
    }

    pub fn scale(x: f64, y: f64) -> Affine {
        Affine([x, 0.0, 0.0, y, 0.0, 0.0])
    }

    /// A turn by `degrees`, clockwise on a y-down page.
    pub fn rotate(degrees: f64) -> Affine {
        let (s, c) = degrees.to_radians().sin_cos();
        Affine([c, s, -s, c, 0.0, 0.0])
    }

    /// `self` after `then`: a point is moved by `self` first.
    pub fn then(self, then: Affine) -> Affine {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = then.0;
        Affine([
            a2 * a + c2 * b,
            b2 * a + d2 * b,
            a2 * c + c2 * d,
            b2 * c + d2 * d,
            a2 * e + c2 * f + e2,
            b2 * e + d2 * f + f2,
        ])
    }

    pub fn apply(&self, p: Pt) -> Pt {
        let [a, b, c, d, e, f] = self.0;
        Pt::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f)
    }

    /// How much a length grows, on average over directions: the square root of the area's.
    pub fn scale_factor(&self) -> f64 {
        let [a, b, c, d, ..] = self.0;
        (a * d - b * c).abs().sqrt()
    }

    /// The turn of the x axis, in degrees clockwise on a y-down page.
    pub fn angle(&self) -> f64 {
        let [a, b, ..] = self.0;
        b.atan2(a).to_degrees()
    }

    pub fn path(&self, path: &Path) -> Path {
        Path {
            segs: path
                .segs
                .iter()
                .map(|s| match *s {
                    Seg::Move(p) => Seg::Move(self.apply(p)),
                    Seg::Line(p) => Seg::Line(self.apply(p)),
                    Seg::Cubic(a, b, c) => Seg::Cubic(self.apply(a), self.apply(b), self.apply(c)),
                    Seg::Close => Seg::Close,
                })
                .collect(),
        }
    }
}

/// The cubic curves of an elliptical arc about `centre` with radii `rx`, `ry`, its axes turned
/// by `phi` (radians), from angle `start` through `sweep` (radians, positive clockwise on a
/// y-down page): one curve for each quarter turn or less.
fn arc_cubics(centre: Pt, rx: f64, ry: f64, phi: f64, start: f64, sweep: f64) -> Vec<Seg> {
    let pieces = (sweep.abs() / FRAC_PI_2 - 1e-9).ceil().max(1.0) as usize;
    let step = sweep / pieces as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let (sp, cp) = phi.sin_cos();
    let at = |t: f64| {
        let (s, c) = t.sin_cos();
        let (x, y) = (rx * c, ry * s);
        Pt::new(centre.x + x * cp - y * sp, centre.y + x * sp + y * cp)
    };
    let tangent = |t: f64| {
        let (s, c) = t.sin_cos();
        let (x, y) = (-rx * s, ry * c);
        (x * cp - y * sp, x * sp + y * cp)
    };
    let mut out = Vec::with_capacity(pieces);
    for i in 0..pieces {
        let (t0, t1) = (start + step * i as f64, start + step * (i + 1) as f64);
        let (p0, p1) = (at(t0), at(t1));
        let (d0, d1) = (tangent(t0), tangent(t1));
        out.push(Seg::Cubic(
            Pt::new(p0.x + k * d0.0, p0.y + k * d0.1),
            Pt::new(p1.x - k * d1.0, p1.y - k * d1.1),
            p1,
        ));
    }
    out
}

/// SVG's arc command from `from`: radii, the turn of the x axis in degrees, the two flags and
/// the end (the SVG specification's appendix on implementing arcs). A radius of zero is a line.
pub(crate) fn svg_arc(
    from: Pt,
    rx: f64,
    ry: f64,
    x_axis_degrees: f64,
    large: bool,
    sweep: bool,
    to: Pt,
) -> Vec<Seg> {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-12 || ry < 1e-12 || (from.x - to.x).abs() + (from.y - to.y).abs() < 1e-12 {
        return vec![Seg::Line(to)];
    }
    let phi = x_axis_degrees.to_radians();
    let (sp, cp) = phi.sin_cos();
    let (dx, dy) = ((from.x - to.x) / 2.0, (from.y - to.y) / 2.0);
    let (x1, y1) = (cp * dx + sp * dy, -sp * dx + cp * dy);
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = (num / den).max(0.0).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let (cx1, cy1) = (coef * rx * y1 / ry, -coef * ry * x1 / rx);
    let centre = Pt::new(
        cp * cx1 - sp * cy1 + (from.x + to.x) / 2.0,
        sp * cx1 + cp * cy1 + (from.y + to.y) / 2.0,
    );
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let (ux, uy) = ((x1 - cx1) / rx, (y1 - cy1) / ry);
    let (vx, vy) = ((-x1 - cx1) / rx, (-y1 - cy1) / ry);
    let start = angle(1.0, 0.0, ux, uy);
    let mut delta = angle(ux, uy, vx, vy);
    if !sweep && delta > 0.0 {
        delta -= TAU;
    } else if sweep && delta < 0.0 {
        delta += TAU;
    }
    let mut segs = arc_cubics(centre, rx, ry, phi, start, delta);
    // The last point is exactly the end the command gives.
    if let Some(Seg::Cubic(_, _, end)) = segs.last_mut() {
        *end = to;
    }
    segs
}

/// An ellipse about `c` as a closed path of four curves, clockwise from its right end.
pub(crate) fn ellipse(c: Pt, rx: f64, ry: f64) -> Path {
    let mut segs = vec![Seg::Move(Pt::new(c.x + rx, c.y))];
    segs.extend(arc_cubics(c, rx, ry, 0.0, 0.0, TAU));
    segs.push(Seg::Close);
    Path { segs }
}

/// A rectangle with corners rounded by `rx`, `ry` (zero for square corners).
pub(crate) fn rounded_rect(x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) -> Path {
    let (rx, ry) = (rx.min(w / 2.0).max(0.0), ry.min(h / 2.0).max(0.0));
    if rx < 1e-12 || ry < 1e-12 {
        return Path {
            segs: vec![
                Seg::Move(Pt::new(x, y)),
                Seg::Line(Pt::new(x + w, y)),
                Seg::Line(Pt::new(x + w, y + h)),
                Seg::Line(Pt::new(x, y + h)),
                Seg::Close,
            ],
        };
    }
    let mut segs = vec![Seg::Move(Pt::new(x + rx, y))];
    segs.push(Seg::Line(Pt::new(x + w - rx, y)));
    segs.extend(arc_cubics(
        Pt::new(x + w - rx, y + ry),
        rx,
        ry,
        0.0,
        -FRAC_PI_2,
        FRAC_PI_2,
    ));
    segs.push(Seg::Line(Pt::new(x + w, y + h - ry)));
    segs.extend(arc_cubics(
        Pt::new(x + w - rx, y + h - ry),
        rx,
        ry,
        0.0,
        0.0,
        FRAC_PI_2,
    ));
    segs.push(Seg::Line(Pt::new(x + rx, y + h)));
    segs.extend(arc_cubics(
        Pt::new(x + rx, y + h - ry),
        rx,
        ry,
        0.0,
        FRAC_PI_2,
        FRAC_PI_2,
    ));
    segs.push(Seg::Line(Pt::new(x, y + ry)));
    segs.extend(arc_cubics(
        Pt::new(x + rx, y + ry),
        rx,
        ry,
        0.0,
        PI,
        FRAC_PI_2,
    ));
    segs.push(Seg::Close);
    Path { segs }
}

/// See [`Path::flatten`].
pub(crate) fn flatten(path: &Path, tolerance: f64) -> Vec<(Vec<Pt>, bool)> {
    let mut out = Vec::new();
    let mut current: Vec<Pt> = Vec::new();
    // Where a subpath that goes on after a close starts again: where the closed one began.
    let mut restart: Option<Pt> = None;
    for seg in &path.segs {
        match *seg {
            Seg::Move(p) => {
                if !current.is_empty() {
                    out.push((std::mem::take(&mut current), false));
                }
                current.push(p);
                restart = None;
            }
            Seg::Line(p) => {
                if current.is_empty() {
                    current.extend(restart.take());
                }
                current.push(p);
            }
            Seg::Cubic(a, b, c) => {
                if current.is_empty() {
                    current.extend(restart.take());
                }
                let p0 = *current.last().unwrap_or(&a);
                if current.is_empty() {
                    current.push(p0);
                }
                let span = dist(p0, a) + dist(a, b) + dist(b, c);
                let pieces = ((span / tolerance.max(1e-9)).sqrt().ceil() as usize).clamp(1, 256);
                for i in 1..=pieces {
                    let t = i as f64 / pieces as f64;
                    current.push(bezier(p0, a, b, c, t));
                }
            }
            Seg::Close => {
                if !current.is_empty() {
                    restart = current.first().copied();
                    out.push((std::mem::take(&mut current), true));
                }
            }
        }
    }
    if !current.is_empty() {
        out.push((current, false));
    }
    out
}

fn bezier(p0: Pt, p1: Pt, p2: Pt, p3: Pt, t: f64) -> Pt {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    Pt::new(
        a * p0.x + b * p1.x + c * p2.x + d * p3.x,
        a * p0.y + b * p1.y + c * p2.y + d * p3.y,
    )
}

pub(crate) fn dist(a: Pt, b: Pt) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

/// A polyline cut into its dashes: the pieces drawn of `dash` (dash, gap, dash, ...), from
/// its start.
pub(crate) fn dashes(points: &[Pt], dash: &[f64]) -> Vec<Vec<Pt>> {
    let total: f64 = dash.iter().sum();
    if dash.is_empty() || total <= 1e-9 || dash.iter().any(|d| *d < 0.0) {
        return vec![points.to_vec()];
    }
    let mut out = Vec::new();
    let (mut index, mut left, mut on) = (0usize, dash[0], true);
    let mut piece: Vec<Pt> = points.first().map(|p| vec![*p]).unwrap_or_default();
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let len = dist(a, b);
        let mut done = 0.0;
        while len - done > left {
            done += left;
            let t = done / len;
            let p = Pt::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            if on {
                piece.push(p);
                out.push(std::mem::take(&mut piece));
            } else {
                piece = vec![p];
            }
            on = !on;
            index = (index + 1) % dash.len();
            left = dash[index];
        }
        left -= len - done;
        if on {
            piece.push(b);
        }
    }
    if on && piece.len() > 1 {
        out.push(piece);
    }
    out
}

/// The polygons whose union is a line of `width` along `points` (closed or not), all turning
/// the same way so that their union under the non-zero rule has no holes: a quadrilateral for
/// each piece, a disc at each corner (a round join; the rasterizer draws every join round) and
/// the ends as `cap` says.
pub(crate) fn outline(
    points: &[Pt],
    closed: bool,
    width: f64,
    cap: Cap,
    tolerance: f64,
) -> Vec<Vec<Pt>> {
    let r = width / 2.0;
    let mut polys = Vec::new();
    if r <= 0.0 || points.is_empty() {
        return polys;
    }
    let mut pts: Vec<Pt> = Vec::with_capacity(points.len() + 1);
    for &p in points {
        if pts.last().is_none_or(|q| dist(*q, p) > 1e-12) {
            pts.push(p);
        }
    }
    if closed && pts.len() > 2 && dist(pts[0], *pts.last().expect("points")) > 1e-12 {
        pts.push(pts[0]);
    }
    if pts.len() == 1 {
        // A dot: drawn only with round or square ends, as SVG does.
        let p = pts[0];
        match cap {
            Cap::Round => polys.push(disc(p, r, tolerance)),
            Cap::Square => polys.push(vec![
                Pt::new(p.x - r, p.y - r),
                Pt::new(p.x + r, p.y - r),
                Pt::new(p.x + r, p.y + r),
                Pt::new(p.x - r, p.y + r),
            ]),
            Cap::Butt => {}
        }
        return polys.into_iter().map(clockwise).collect();
    }
    let last = pts.len() - 2;
    for (i, pair) in pts.windows(2).enumerate() {
        let (mut a, mut b) = (pair[0], pair[1]);
        let len = dist(a, b);
        let (ux, uy) = ((b.x - a.x) / len, (b.y - a.y) / len);
        if !closed && cap == Cap::Square {
            if i == 0 {
                a = Pt::new(a.x - ux * r, a.y - uy * r);
            }
            if i == last {
                b = Pt::new(b.x + ux * r, b.y + uy * r);
            }
        }
        let (nx, ny) = (-uy * r, ux * r);
        polys.push(vec![
            Pt::new(a.x + nx, a.y + ny),
            Pt::new(b.x + nx, b.y + ny),
            Pt::new(b.x - nx, b.y - ny),
            Pt::new(a.x - nx, a.y - ny),
        ]);
    }
    // The joins, and round ends.
    for (i, &p) in pts.iter().enumerate() {
        let end = i == 0 || i == pts.len() - 1;
        if !end || closed || cap == Cap::Round {
            polys.push(disc(p, r, tolerance));
        }
    }
    polys.into_iter().map(clockwise).collect()
}

/// A circle as a polygon, close to within `tolerance`.
pub(crate) fn disc(c: Pt, r: f64, tolerance: f64) -> Vec<Pt> {
    let n =
        ((PI / (1.0 - tolerance.min(r) / r).clamp(-1.0, 1.0).acos()).ceil() as usize).clamp(8, 256);
    (0..n)
        .map(|k| {
            let a = TAU * k as f64 / n as f64;
            Pt::new(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

/// Twice the signed area: positive for a polygon that turns clockwise on a y-down page.
pub(crate) fn signed_area(poly: &[Pt]) -> f64 {
    let mut s = 0.0;
    for (i, a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        s += a.x * b.y - b.x * a.y;
    }
    s
}

fn clockwise(mut poly: Vec<Pt>) -> Vec<Pt> {
    if signed_area(&poly) < 0.0 {
        poly.reverse();
    }
    poly
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_circle_arc_ends_where_it_says_and_bulges_the_right_way() {
        // From (0,0) to (2,0) with radius 1, sweep 1: clockwise on a y-down page, through (1,-1).
        let segs = svg_arc(
            Pt::new(0.0, 0.0),
            1.0,
            1.0,
            0.0,
            false,
            true,
            Pt::new(2.0, 0.0),
        );
        let path = Path {
            segs: std::iter::once(Seg::Move(Pt::new(0.0, 0.0)))
                .chain(segs)
                .collect(),
        };
        let (points, _) = &path.flatten(0.001)[0];
        let top = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        assert!((top + 1.0).abs() < 0.01, "{top}");
        let end = points.last().unwrap();
        assert!((end.x - 2.0).abs() < 1e-9 && end.y.abs() < 1e-9);
    }

    #[test]
    fn dashes_alternate() {
        let line = [Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)];
        let d = dashes(&line, &[2.0, 1.0]);
        assert_eq!(d.len(), 4);
        assert!((d[1][0].x - 3.0).abs() < 1e-9 && (d[1][1].x - 5.0).abs() < 1e-9);
    }

    #[test]
    fn rotate_turns_clockwise_on_the_page() {
        let p = Affine::rotate(90.0).apply(Pt::new(1.0, 0.0));
        assert!(p.x.abs() < 1e-12 && (p.y - 1.0).abs() < 1e-12);
    }
}
