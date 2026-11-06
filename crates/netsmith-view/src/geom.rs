//! Points, directions and orientations, all in millimetres on the sheet. The sheet's y
//! grows downward; a symbol's own y grows upward, so every symbol point is flipped when
//! it is placed.

/// A point on the sheet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// This point moved `len` millimetres in direction `dir`.
    pub fn step(self, dir: Dir, len: f64) -> Self {
        let (dx, dy) = dir.vector();
        Self::new(self.x + dx * len, self.y + dy * len)
    }
}

/// A rectangle given by its smallest and largest corners.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: Pt,
    pub max: Pt,
}

impl Rect {
    pub fn around(p: Pt) -> Self {
        Self { min: p, max: p }
    }

    pub fn include(&mut self, p: Pt) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }

    pub fn union(&mut self, other: Rect) {
        self.include(other.min);
        self.include(other.max);
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn translated(&self, by: Pt) -> Rect {
        Rect {
            min: Pt::new(self.min.x + by.x, self.min.y + by.y),
            max: Pt::new(self.max.x + by.x, self.max.y + by.y),
        }
    }
}

/// One of the four ways a wire can leave a pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    /// Unit vector on the sheet (y grows downward).
    pub fn vector(self) -> (f64, f64) {
        match self {
            Dir::Up => (0.0, -1.0),
            Dir::Down => (0.0, 1.0),
            Dir::Left => (-1.0, 0.0),
            Dir::Right => (1.0, 0.0),
        }
    }

    pub fn is_vertical(self) -> bool {
        matches!(self, Dir::Up | Dir::Down)
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
            Dir::Left => Dir::Right,
            Dir::Right => Dir::Left,
        }
    }

    /// The direction of a symbol-space vector (y up) once the sheet's y flip is applied
    /// by `orient`, or `None` when it is not along an axis.
    fn from_vector(x: f64, y: f64) -> Option<Dir> {
        const EPS: f64 = 1e-6;
        match (x.abs() < EPS, y.abs() < EPS) {
            (true, false) if y < 0.0 => Some(Dir::Up),
            (true, false) => Some(Dir::Down),
            (false, true) if x < 0.0 => Some(Dir::Left),
            (false, true) => Some(Dir::Right),
            _ => None,
        }
    }
}

/// How a symbol is turned and flipped on the sheet: a signed permutation of its axes
/// followed by the y flip, stored as the matrix `[a b; c d]` from symbol `(x, y)` to
/// sheet `(a*x + b*y, c*x + d*y)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Orient {
    a: i8,
    b: i8,
    c: i8,
    d: i8,
}

impl Orient {
    /// As drawn in the library: symbol up is sheet up.
    pub const IDENTITY: Orient = Orient::new(1, 0, 0, -1);

    const fn new(a: i8, b: i8, c: i8, d: i8) -> Self {
        Self { a, b, c, d }
    }

    /// All eight turns and mirrors, identity first so that ties keep the library's drawing.
    pub const ALL: [Orient; 8] = [
        Orient::new(1, 0, 0, -1),  // as drawn
        Orient::new(-1, 0, 0, -1), // mirrored left to right
        Orient::new(-1, 0, 0, 1),  // turned half a turn
        Orient::new(1, 0, 0, 1),   // mirrored top to bottom
        Orient::new(0, -1, -1, 0), // quarter turn counter-clockwise
        Orient::new(0, 1, 1, 0),   // quarter turn clockwise
        Orient::new(0, -1, 1, 0),
        Orient::new(0, 1, -1, 0),
    ];

    /// The orientation of a symbol flipped left to right when `mirror` (before the turn) and
    /// then turned `quarter_turns` quarter turns counterclockwise on the page, from the symbol
    /// as the library draws it ([`Orient::IDENTITY`]). This is how a layout file spells an
    /// orientation (ADR 0061): `turned(false, 0)` is the library's drawing.
    pub fn turned(mirror: bool, quarter_turns: u8) -> Orient {
        let base = if mirror {
            Orient::ALL[1]
        } else {
            Orient::IDENTITY
        };
        let mut o = base;
        for _ in 0..quarter_turns % 4 {
            // A quarter turn counterclockwise on the page (y down) takes the sheet vector
            // (x, y) to (y, -x).
            o = Orient::new(o.c, o.d, -o.a, -o.b);
        }
        o
    }

    /// The inverse of [`Orient::turned`]: whether the symbol is mirrored, and how many quarter
    /// turns counterclockwise it is turned after.
    pub fn mirror_and_turns(self) -> (bool, u8) {
        for mirror in [false, true] {
            for turns in 0..4 {
                if Orient::turned(mirror, turns) == self {
                    return (mirror, turns);
                }
            }
        }
        unreachable!("every orientation is a mirror and a number of quarter turns")
    }

    /// A symbol-space point (y up) on the sheet, relative to the symbol's origin.
    pub fn apply(self, p: (f64, f64)) -> Pt {
        let (x, y) = p;
        Pt::new(
            f64::from(self.a) * x + f64::from(self.b) * y,
            f64::from(self.c) * x + f64::from(self.d) * y,
        )
    }

    /// The sheet direction of a symbol-space direction.
    pub fn dir(self, v: (f64, f64)) -> Dir {
        let p = self.apply(v);
        Dir::from_vector(p.x, p.y).expect("symbol pins run along an axis")
    }
}

/// Round to the nearest multiple of `grid`.
pub fn snap(v: f64, grid: f64) -> f64 {
    (v / grid).round() * grid
}

/// Round up to a multiple of `grid`.
pub fn snap_up(v: f64, grid: f64) -> f64 {
    ((v / grid) - 1e-9).ceil() * grid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_flips_the_symbols_y_axis() {
        let p = Orient::IDENTITY.apply((0.0, 3.81));
        assert_eq!((p.x, p.y), (0.0, -3.81));
        assert_eq!(Orient::IDENTITY.dir((0.0, 1.0)), Dir::Up);
        assert_eq!(Orient::IDENTITY.dir((1.0, 0.0)), Dir::Right);
    }

    #[test]
    fn all_eight_orientations_differ() {
        for (i, a) in Orient::ALL.iter().enumerate() {
            for b in &Orient::ALL[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn a_quarter_turn_takes_right_to_up() {
        // Counter-clockwise on the page: a pin that faced right now faces up.
        assert_eq!(Orient::ALL[4].dir((1.0, 0.0)), Dir::Up);
        assert_eq!(Orient::ALL[4].dir((0.0, 1.0)), Dir::Left);
    }

    #[test]
    fn mirroring_swaps_left_and_right_only() {
        let m = Orient::ALL[1];
        assert_eq!(m.dir((1.0, 0.0)), Dir::Left);
        assert_eq!(m.dir((0.0, 1.0)), Dir::Up);
    }

    #[test]
    fn a_mirror_and_quarter_turns_name_each_of_the_eight_orientations() {
        assert_eq!(Orient::turned(false, 0), Orient::IDENTITY);
        assert_eq!(Orient::turned(true, 0), Orient::ALL[1]);
        assert_eq!(Orient::turned(false, 1), Orient::ALL[4]);
        assert_eq!(Orient::turned(false, 2), Orient::ALL[2]);
        let mut seen = Vec::new();
        for mirror in [false, true] {
            for turns in 0..4 {
                let o = Orient::turned(mirror, turns);
                assert!(Orient::ALL.contains(&o));
                assert!(!seen.contains(&o));
                seen.push(o);
                assert_eq!(o.mirror_and_turns(), (mirror, turns));
            }
        }
        // Four quarter turns are none.
        assert_eq!(Orient::turned(false, 5), Orient::turned(false, 1));
    }

    #[test]
    fn snapping_rounds_to_the_grid() {
        assert!((snap(1.3, 1.27) - 1.27).abs() < 1e-9);
        assert!((snap_up(1.3, 1.27) - 2.54).abs() < 1e-9);
        assert!((snap_up(2.54, 1.27) - 2.54).abs() < 1e-9);
    }
}
