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

/// The widest strip of [`Rect::turned_cover`], in mm, while there are no more strips than
/// [`TURNED_STRIPS`].
pub const TURNED_STRIP: f64 = 0.25;

/// The most strips [`Rect::turned_cover`] cuts one rectangle into: a part's courtyard turned off
/// the right angles is this many rooms at most, for the placer and the check to measure.
pub const TURNED_STRIPS: usize = 24;

/// Whether a turn is a right angle (0, 90, 180 or 270 degrees, give or take a millionth of a
/// degree), at which a box turned is a box.
pub fn is_right_angle(degrees: f64) -> bool {
    let off = degrees.rem_euclid(90.0);
    off < 1e-6 || 90.0 - off < 1e-6
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

    /// The area covered by `rects` together, in mm2: where they overlap it counts once.
    pub fn union_area(rects: &[Rect]) -> f64 {
        if let [only] = rects {
            return only.width() * only.height();
        }
        let mut xs: Vec<f64> = rects.iter().flat_map(|r| [r.min.x, r.max.x]).collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        xs.windows(2)
            .map(|w| {
                // The strip between two x values: the length of the union of the y spans of
                // the rectangles that cross it.
                let mut spans: Vec<(f64, f64)> = rects
                    .iter()
                    .filter(|r| r.min.x <= w[0] && r.max.x >= w[1])
                    .map(|r| (r.min.y, r.max.y))
                    .collect();
                spans.sort_by(|a, b| a.0.total_cmp(&b.0));
                let (mut length, mut reached) = (0.0, f64::MIN);
                for (lo, hi) in spans {
                    length += (hi - lo.max(reached)).max(0.0);
                    reached = reached.max(hi);
                }
                length * (w[1] - w[0])
            })
            .sum()
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

    /// The rectangle as it lies on a board, seen in a mirror and turned as
    /// [`Rect::rotated_for`] does, covered by boxes that hold it: at a right angle the one box
    /// that is the turned rectangle, at another angle a row of strips across the board's x
    /// (issue #996). Each strip is the box of the turned rectangle's part in it, so the strips
    /// hold the turned rectangle and reach past it only by less than a strip's width (at most
    /// [`TURNED_STRIP`] mm while there are no more than [`TURNED_STRIPS`]). The one box that
    /// holds it all (`rotated_for`) takes twice the room of a square turned 45 degrees, and far
    /// more for a long part.
    pub fn turned_cover(&self, mirrored: bool, degrees: f64) -> Vec<Rect> {
        let r = if mirrored { self.mirrored_x() } else { *self };
        let whole = r.rotated(degrees);
        if is_right_angle(degrees) || whole.width() <= 0.0 || whole.height() <= 0.0 {
            return vec![whole];
        }
        let polygon = r.corners().map(|c| c.rotate(degrees));
        let n = ((whole.width() / TURNED_STRIP).ceil() as usize).clamp(2, TURNED_STRIPS);
        let step = whole.width() / n as f64;
        (0..n)
            .filter_map(|k| {
                let x0 = whole.min.x + step * k as f64;
                let x1 = if k + 1 == n { whole.max.x } else { x0 + step };
                // The convex polygon cut by the strip: its corners inside the strip and where
                // its edges cross the strip's two sides.
                let mut ys: Vec<f64> = Vec::new();
                for (i, a) in polygon.iter().enumerate() {
                    let b = polygon[(i + 1) % 4];
                    if a.x >= x0 - 1e-12 && a.x <= x1 + 1e-12 {
                        ys.push(a.y);
                    }
                    for x in [x0, x1] {
                        let (lo, hi) = (a.x.min(b.x), a.x.max(b.x));
                        if hi - lo > 1e-12 && x >= lo && x <= hi {
                            ys.push(a.y + (b.y - a.y) * (x - a.x) / (b.x - a.x));
                        }
                    }
                }
                let y0 = ys.iter().copied().fold(f64::INFINITY, f64::min);
                let y1 = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                (y0 <= y1).then(|| Rect::new(Pt::new(x0, y0), Pt::new(x1, y1)))
            })
            .collect()
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
    fn a_rectangle_at_a_right_angle_is_covered_by_its_turned_box() {
        let r = Rect::new(Pt::new(-2.0, -1.0), Pt::new(3.0, 1.0));
        for deg in [0.0, 90.0, 180.0, 270.0, -90.0, 450.0] {
            for mirrored in [false, true] {
                assert_eq!(
                    r.turned_cover(mirrored, deg),
                    vec![r.rotated_for(mirrored, deg)]
                );
            }
        }
    }

    #[test]
    fn a_rectangle_off_the_right_angles_is_strips_that_hold_it_and_little_more() {
        // A 4 x 1 mm rectangle at 30 and 45 degrees, on each face.
        let r = Rect::new(Pt::new(-2.0, -0.5), Pt::new(2.0, 0.5));
        for deg in [30.0, 45.0, 315.0] {
            for mirrored in [false, true] {
                let cover = r.turned_cover(mirrored, deg);
                let whole = r.rotated_for(mirrored, deg);
                assert!(cover.len() >= 2 && cover.len() <= TURNED_STRIPS);
                // The strips hold every point of the turned rectangle.
                let base = if mirrored { r.mirrored_x() } else { r };
                for i in 0..=40 {
                    for j in 0..=10 {
                        let local = Pt::new(
                            base.min.x + base.width() * f64::from(i) / 40.0,
                            base.min.y + base.height() * f64::from(j) / 10.0,
                        );
                        let p = local.rotate(deg);
                        assert!(
                            cover.iter().any(|c| p.x >= c.min.x - 1e-9
                                && p.x <= c.max.x + 1e-9
                                && p.y >= c.min.y - 1e-9
                                && p.y <= c.max.y + 1e-9),
                            "{p:?} at {deg} is in no strip"
                        );
                    }
                }
                // Each strip is in the box round it, and together they take far less room.
                assert!(cover.iter().all(|c| whole.contains(c)));
                let area = Rect::union_area(&cover);
                assert!(area < whole.width() * whole.height() * 0.7, "{area} at {deg}");
                // More than the rectangle's own 4 mm2, by less than a strip along its outline.
                assert!(area > 4.0 - 1e-9 && area < 4.0 + 10.0 * TURNED_STRIP, "{area}");
            }
        }
    }

    #[test]
    fn right_angles_are_told_from_other_turns() {
        assert!(is_right_angle(0.0) && is_right_angle(270.0) && is_right_angle(-90.0));
        assert!(is_right_angle(359.999_999_9));
        assert!(!is_right_angle(45.0) && !is_right_angle(89.9));
    }

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
    fn the_union_area_counts_an_overlap_once() {
        let a = Rect::new(Pt::new(0.0, 0.0), Pt::new(2.0, 2.0));
        assert_eq!(Rect::union_area(&[a]), 4.0);
        // Apart: the sum. Overlapping by a unit square: the sum less that. One inside the
        // other: the larger.
        assert_eq!(Rect::union_area(&[a, a.translated(Pt::new(5.0, 0.0))]), 8.0);
        assert_eq!(Rect::union_area(&[a, a.translated(Pt::new(1.0, 1.0))]), 7.0);
        let inside = Rect::new(Pt::new(0.5, 0.5), Pt::new(1.0, 1.5));
        assert_eq!(Rect::union_area(&[a, inside]), 4.0);
        assert_eq!(Rect::union_area(&[]), 0.0);
    }

    #[test]
    fn contains_includes_the_edge() {
        let outer = Rect::new(Pt::new(0.0, 0.0), Pt::new(4.0, 4.0));
        assert!(outer.contains(&outer));
        assert!(!outer.contains(&outer.translated(Pt::new(0.1, 0.0))));
    }
}
