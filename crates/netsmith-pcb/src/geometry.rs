//! Points and rectangles, in millimetres with y growing downward, as KiCad has them.

/// A point. Angles anywhere in this crate follow KiCad: degrees, and a positive angle turns
/// counterclockwise as seen on screen.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(self, o: Pt) -> f64 {
        (self.x - o.x).hypot(self.y - o.y)
    }

    /// Turn about the origin. Right angles are exact.
    pub fn rotate(self, degrees: f64) -> Pt {
        let (cos, sin) = cos_sin(degrees);
        Pt::new(self.x * cos + self.y * sin, -self.x * sin + self.y * cos)
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

fn cos_sin(degrees: f64) -> (f64, f64) {
    let d = degrees.rem_euclid(360.0);
    if d == 0.0 {
        (1.0, 0.0)
    } else if d == 90.0 {
        (0.0, 1.0)
    } else if d == 180.0 {
        (-1.0, 0.0)
    } else if d == 270.0 {
        (0.0, -1.0)
    } else {
        let r = d.to_radians();
        (r.cos(), r.sin())
    }
}

/// An axis-aligned rectangle with `min` at the top left.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub min: Pt,
    pub max: Pt,
}

impl Rect {
    pub fn new(min: Pt, max: Pt) -> Self {
        Self { min, max }
    }

    /// The smallest rectangle holding every point, or `None` if there are none.
    pub fn around(points: impl IntoIterator<Item = Pt>) -> Option<Rect> {
        let mut points = points.into_iter();
        let first = points.next()?;
        let mut r = Rect::new(first, first);
        for p in points {
            r.min = Pt::new(r.min.x.min(p.x), r.min.y.min(p.y));
            r.max = Pt::new(r.max.x.max(p.x), r.max.y.max(p.y));
        }
        Some(r)
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn centre(&self) -> Pt {
        Pt::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
        )
    }

    pub fn corners(&self) -> [Pt; 4] {
        [
            self.min,
            Pt::new(self.max.x, self.min.y),
            self.max,
            Pt::new(self.min.x, self.max.y),
        ]
    }

    pub fn expanded(&self, by: f64) -> Rect {
        Rect::new(
            Pt::new(self.min.x - by, self.min.y - by),
            Pt::new(self.max.x + by, self.max.y + by),
        )
    }

    pub fn translated(&self, by: Pt) -> Rect {
        Rect::new(self.min + by, self.max + by)
    }

    pub fn union(&self, o: &Rect) -> Rect {
        Rect::new(
            Pt::new(self.min.x.min(o.min.x), self.min.y.min(o.min.y)),
            Pt::new(self.max.x.max(o.max.x), self.max.y.max(o.max.y)),
        )
    }

    /// The box that holds this one after it is turned about the origin.
    pub fn rotated(&self, degrees: f64) -> Rect {
        Rect::around(self.corners().map(|c| c.rotate(degrees))).expect("a rectangle has corners")
    }

    /// The box seen in a mirror along the y axis: x is negated.
    pub fn mirrored_x(&self) -> Rect {
        Rect::new(
            Pt::new(-self.max.x, self.min.y),
            Pt::new(-self.min.x, self.max.y),
        )
    }

    /// The box as it lies on a board: seen in a mirror along the y axis if `mirrored`, then
    /// turned about the origin.
    pub fn rotated_for(&self, mirrored: bool, degrees: f64) -> Rect {
        let r = if mirrored { self.mirrored_x() } else { *self };
        r.rotated(degrees)
    }

    /// True if the two share area. Rectangles that only touch do not overlap.
    pub fn overlaps(&self, o: &Rect) -> bool {
        const TOLERANCE: f64 = 1e-6;
        self.min.x < o.max.x - TOLERANCE
            && o.min.x < self.max.x - TOLERANCE
            && self.min.y < o.max.y - TOLERANCE
            && o.min.y < self.max.y - TOLERANCE
    }

    pub fn contains(&self, o: &Rect) -> bool {
        const TOLERANCE: f64 = 1e-6;
        o.min.x >= self.min.x - TOLERANCE
            && o.min.y >= self.min.y - TOLERANCE
            && o.max.x <= self.max.x + TOLERANCE
            && o.max.y <= self.max.y + TOLERANCE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_angle_rotations_are_exact() {
        // A quarter turn counterclockwise on screen sends right to up (y grows downward).
        assert_eq!(Pt::new(1.0, 0.0).rotate(90.0), Pt::new(0.0, -1.0));
        assert_eq!(Pt::new(1.0, 2.0).rotate(180.0), Pt::new(-1.0, -2.0));
        assert_eq!(Pt::new(1.0, 0.0).rotate(270.0), Pt::new(0.0, 1.0));
        assert_eq!(Pt::new(1.0, 0.0).rotate(-90.0), Pt::new(0.0, 1.0));
        assert_eq!(Pt::new(3.0, 4.0).rotate(360.0), Pt::new(3.0, 4.0));
    }

    #[test]
    fn rotating_a_rectangle_swaps_its_sides() {
        let r = Rect::new(Pt::new(-2.0, -1.0), Pt::new(2.0, 1.0)).rotated(90.0);
        assert_eq!((r.width(), r.height()), (2.0, 4.0));
    }

    #[test]
    fn touching_rectangles_do_not_overlap() {
        let a = Rect::new(Pt::new(0.0, 0.0), Pt::new(1.0, 1.0));
        let touching = a.translated(Pt::new(1.0, 0.0));
        let over = a.translated(Pt::new(0.5, 0.5));
        assert!(!a.overlaps(&touching));
        assert!(a.overlaps(&over));
    }

    #[test]
    fn contains_includes_the_edge() {
        let outer = Rect::new(Pt::new(0.0, 0.0), Pt::new(4.0, 4.0));
        assert!(outer.contains(&outer));
        assert!(!outer.contains(&outer.translated(Pt::new(0.1, 0.0))));
    }
}
