//! Plane geometry for the fabrication writers: points, turns, arcs cut into short lines,
//! and the few polygon operations the zone fill needs. All lengths are millimetres in
//! KiCad's board frame (x to the right, y down), where a positive angle turns a point
//! counter-clockwise as seen on the screen.

/// The most a cut-up arc may differ from the true arc, in mm.
pub const ARC_ERROR: f64 = 0.002;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Turned about the origin by `degrees`, KiCad's way (`y` points down, so a positive
    /// angle is counter-clockwise on screen). Right angles are exact.
    pub fn rotate(self, degrees: f64) -> Pt {
        let (s, c) = sin_cos(degrees);
        Pt::new(self.x * c + self.y * s, -self.x * s + self.y * c)
    }

    pub fn dist(self, o: Pt) -> f64 {
        (self.x - o.x).hypot(self.y - o.y)
    }
}

impl std::ops::Add for Pt {
    type Output = Pt;
    fn add(self, o: Pt) -> Pt {
        Pt::new(self.x + o.x, self.y + o.y)
    }
}

impl std::ops::Sub for Pt {
    type Output = Pt;
    fn sub(self, o: Pt) -> Pt {
        Pt::new(self.x - o.x, self.y - o.y)
    }
}

impl std::ops::Mul<f64> for Pt {
    type Output = Pt;
    fn mul(self, k: f64) -> Pt {
        Pt::new(self.x * k, self.y * k)
    }
}

/// Sine and cosine of an angle in degrees, exact at multiples of 90.
pub fn sin_cos(degrees: f64) -> (f64, f64) {
    let d = degrees.rem_euclid(360.0);
    if d == 0.0 {
        (0.0, 1.0)
    } else if d == 90.0 {
        (1.0, 0.0)
    } else if d == 180.0 {
        (0.0, -1.0)
    } else if d == 270.0 {
        (-1.0, 0.0)
    } else {
        d.to_radians().sin_cos()
    }
}

/// True if `degrees` is a multiple of 90 (within a millionth of a degree).
pub fn is_right_angle(degrees: f64) -> bool {
    let r = degrees.rem_euclid(90.0);
    r < 1e-6 || 90.0 - r < 1e-6
}

/// A box, `min` to `max`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: Pt,
    pub max: Pt,
}

impl Bounds {
    pub fn of(points: impl IntoIterator<Item = Pt>) -> Option<Bounds> {
        let mut it = points.into_iter();
        let first = it.next()?;
        let mut b = Bounds {
            min: first,
            max: first,
        };
        for p in it {
            b.add(p);
        }
        Some(b)
    }

    pub fn add(&mut self, p: Pt) {
        self.min = Pt::new(self.min.x.min(p.x), self.min.y.min(p.y));
        self.max = Pt::new(self.max.x.max(p.x), self.max.y.max(p.y));
    }

    pub fn expanded(&self, d: f64) -> Bounds {
        Bounds {
            min: Pt::new(self.min.x - d, self.min.y - d),
            max: Pt::new(self.max.x + d, self.max.y + d),
        }
    }

    pub fn overlaps(&self, o: &Bounds) -> bool {
        self.min.x < o.max.x && o.min.x < self.max.x && self.min.y < o.max.y && o.min.y < self.max.y
    }
}

/// The circle through three points: its centre and radius, or `None` if they are on a line.
pub fn circle_through(a: Pt, b: Pt, c: Pt) -> Option<(Pt, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let (a2, b2, c2) = (
        a.x * a.x + a.y * a.y,
        b.x * b.x + b.y * b.y,
        c.x * c.x + c.y * c.y,
    );
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let centre = Pt::new(ux, uy);
    Some((centre, centre.dist(a)))
}

/// An arc of a circle from `start` to `end` through `mid`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arc {
    pub start: Pt,
    pub end: Pt,
    pub centre: Pt,
    pub radius: f64,
    /// True if the arc runs counter-clockwise as seen on the screen (y down) from start to
    /// end.
    pub ccw: bool,
}

impl Arc {
    /// The arc KiCad stores as start, mid and end. `None` if the three are on a line.
    pub fn through(start: Pt, mid: Pt, end: Pt) -> Option<Arc> {
        let (centre, radius) = circle_through(start, mid, end)?;
        // The turn start -> mid -> end tells the direction. With y down, a positive cross
        // product is clockwise on screen.
        let cross = (mid.x - start.x) * (end.y - mid.y) - (mid.y - start.y) * (end.x - mid.x);
        Some(Arc {
            start,
            end,
            centre,
            radius,
            ccw: cross < 0.0,
        })
    }

    /// The angle of `p` about the centre, in radians, with the screen's sense (y down,
    /// counter-clockwise positive): the angle of `(dx, -dy)`.
    fn angle_of(&self, p: Pt) -> f64 {
        (-(p.y - self.centre.y)).atan2(p.x - self.centre.x)
    }

    /// The signed sweep from start to end in radians: positive if counter-clockwise, and
    /// never zero: equal ends make a full circle.
    pub fn sweep(&self) -> f64 {
        let (a0, a1) = (self.angle_of(self.start), self.angle_of(self.end));
        let tau = std::f64::consts::TAU;
        let mut s = if self.ccw {
            (a1 - a0).rem_euclid(tau)
        } else {
            -(a0 - a1).rem_euclid(tau)
        };
        if s.abs() < 1e-12 {
            s = if self.ccw { tau } else { -tau };
        }
        s
    }

    /// The point at angle `a` (radians, screen sense) on the circle.
    fn at_angle(&self, a: f64) -> Pt {
        Pt::new(
            self.centre.x + self.radius * a.cos(),
            self.centre.y - self.radius * a.sin(),
        )
    }

    /// The ends of the arc and the points of its circle (top, bottom, left, right) that its
    /// sweep passes through: points whose box is the arc's own box.
    pub fn extent_points(&self) -> Vec<Pt> {
        let sweep = self.sweep();
        let a0 = self.angle_of(self.start);
        let tau = std::f64::consts::TAU;
        // The sweep as a run of increasing angle, whichever way the arc turns.
        let (from, span) = if sweep >= 0.0 {
            (a0, sweep)
        } else {
            (a0 + sweep, -sweep)
        };
        let mut out = vec![self.start, self.end];
        for k in 0..4 {
            let phi = f64::from(k) * std::f64::consts::FRAC_PI_2;
            if (phi - from).rem_euclid(tau) <= span {
                out.push(self.at_angle(phi));
            }
        }
        out
    }

    /// The unit directions in which the arc leaves its start and its end, each into the arc:
    /// its tangents there.
    pub fn tangents(&self) -> (Pt, Pt) {
        let s = self.sweep().signum();
        let along = |a: f64, sign: f64| Pt::new(-a.sin() * sign, -a.cos() * sign);
        (
            along(self.angle_of(self.start), s),
            along(self.angle_of(self.end), -s),
        )
    }

    /// The arc as points from start to end, no further than [`ARC_ERROR`] from the arc.
    pub fn points(&self) -> Vec<Pt> {
        let sweep = self.sweep();
        let a0 = self.angle_of(self.start);
        let steps = steps_for(self.radius, sweep.abs());
        let mut out = Vec::with_capacity(steps + 1);
        out.push(self.start);
        for i in 1..steps {
            out.push(self.at_angle(a0 + sweep * i as f64 / steps as f64));
        }
        out.push(self.end);
        out
    }
}

/// How many line pieces cut an arc of `radius` and `sweep` (radians) within [`ARC_ERROR`].
pub fn steps_for(radius: f64, sweep: f64) -> usize {
    if radius <= ARC_ERROR {
        return 1;
    }
    let step = 2.0 * (1.0 - ARC_ERROR / radius).acos();
    ((sweep / step).ceil() as usize).max(1)
}

/// A circle as a closed polygon, counter-clockwise on screen, starting at angle 0.
pub fn circle_polygon(centre: Pt, radius: f64) -> Vec<Pt> {
    let n = steps_for(radius, std::f64::consts::TAU).max(8);
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            Pt::new(centre.x + radius * a.cos(), centre.y - radius * a.sin())
        })
        .collect()
}

/// Twice the signed area of a polygon. Positive when the points run clockwise on screen
/// (y down), which is the shoelace sum in these coordinates.
pub fn area2(poly: &[Pt]) -> f64 {
    let mut s = 0.0;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        s += a.x * b.y - b.x * a.y;
    }
    s
}

/// The polygon with its points in the opposite order.
pub fn reversed(poly: &[Pt]) -> Vec<Pt> {
    poly.iter().rev().copied().collect()
}

/// True if `p` is inside the polygon (even-odd rule).
pub fn contains(poly: &[Pt], p: Pt) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// The part of a convex polygon on the side of the line through `a` with direction `dir`
/// that `dir` turns left from... precisely: points `p` with `cross(dir, p - a) >= 0`.
/// Sutherland-Hodgman against one half-plane; the polygon must be convex for the result to
/// be one polygon.
pub fn clip_half_plane(poly: &[Pt], a: Pt, dir: Pt) -> Vec<Pt> {
    let side = |p: Pt| dir.x * (p.y - a.y) - dir.y * (p.x - a.x);
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sp, sq) = (side(p), side(q));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            let t = sp / (sp - sq);
            out.push(Pt::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_is_counter_clockwise_on_screen_and_exact_at_right_angles() {
        // A point to the right of the origin, turned 90 degrees, ends up above it (y down).
        assert_eq!(Pt::new(1.0, 0.0).rotate(90.0), Pt::new(0.0, -1.0));
        assert_eq!(Pt::new(1.0, 0.0).rotate(-90.0), Pt::new(0.0, 1.0));
        assert_eq!(Pt::new(2.0, 1.0).rotate(180.0), Pt::new(-2.0, -1.0));
        assert!(is_right_angle(270.0) && is_right_angle(-90.0) && !is_right_angle(45.0));
    }

    #[test]
    fn an_arc_leaves_its_ends_along_its_tangents_into_the_arc() {
        // Over the top from the right to the left: up out of the start, up out of the end.
        let a = Arc::through(Pt::new(1.0, 0.0), Pt::new(0.0, -1.0), Pt::new(-1.0, 0.0)).unwrap();
        let (s, e) = a.tangents();
        assert!(s.dist(Pt::new(0.0, -1.0)) < 1e-9, "{s:?}");
        assert!(e.dist(Pt::new(0.0, -1.0)) < 1e-9, "{e:?}");
        // Under the bottom: down out of both.
        let b = Arc::through(Pt::new(1.0, 0.0), Pt::new(0.0, 1.0), Pt::new(-1.0, 0.0)).unwrap();
        let (s, e) = b.tangents();
        assert!(s.dist(Pt::new(0.0, 1.0)) < 1e-9 && e.dist(Pt::new(0.0, 1.0)) < 1e-9);
    }

    #[test]
    fn an_arc_through_three_points_knows_its_direction_and_sweep() {
        // On screen: from the right of the centre, over the top, to the left: counter-clockwise.
        let a = Arc::through(Pt::new(1.0, 0.0), Pt::new(0.0, -1.0), Pt::new(-1.0, 0.0)).unwrap();
        assert!(a.ccw);
        assert!((a.sweep() - std::f64::consts::PI).abs() < 1e-9);
        assert!((a.radius - 1.0).abs() < 1e-9);
        let b = Arc::through(Pt::new(1.0, 0.0), Pt::new(0.0, 1.0), Pt::new(-1.0, 0.0)).unwrap();
        assert!(!b.ccw);
        assert!((b.sweep() + std::f64::consts::PI).abs() < 1e-9);
        assert!(Arc::through(Pt::new(0.0, 0.0), Pt::new(1.0, 1.0), Pt::new(2.0, 2.0)).is_none());
        // Every point of the cut-up arc is on the circle, and the ends are the ends.
        let pts = a.points();
        assert_eq!(pts.first(), Some(&a.start));
        assert_eq!(pts.last(), Some(&a.end));
        assert!(pts.iter().all(|p| (p.dist(a.centre) - 1.0).abs() < 1e-9));
        assert!(pts.len() > 8);
    }

    #[test]
    fn clipping_a_square_by_a_half_plane() {
        let sq = [
            Pt::new(0.0, 0.0),
            Pt::new(2.0, 0.0),
            Pt::new(2.0, 2.0),
            Pt::new(0.0, 2.0),
        ];
        // Keep x >= 1: direction (0, -1) has the points with larger x on its left in this
        // frame: cross(dir, p - a) = dir.x*(p.y-a.y) - dir.y*(p.x-a.x) = (p.x - 1).
        let kept = clip_half_plane(&sq, Pt::new(1.0, 0.0), Pt::new(0.0, -1.0));
        let b = Bounds::of(kept.iter().copied()).unwrap();
        assert_eq!((b.min.x, b.max.x), (1.0, 2.0));
        assert_eq!(area2(&kept).abs(), 4.0);
    }

    #[test]
    fn point_in_polygon() {
        let tri = [Pt::new(0.0, 0.0), Pt::new(4.0, 0.0), Pt::new(0.0, 4.0)];
        assert!(contains(&tri, Pt::new(1.0, 1.0)));
        assert!(!contains(&tri, Pt::new(3.0, 3.0)));
    }
}
