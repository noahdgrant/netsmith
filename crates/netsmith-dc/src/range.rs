//! A closed interval of reals: the stated range of a value, or the range a result takes.

use std::fmt;

/// `lo <= hi`. A value with no spread (`lo == hi`) is a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub lo: f64,
    pub hi: f64,
}

impl Range {
    /// The range of the two ends in either order.
    pub fn new(a: f64, b: f64) -> Range {
        Range {
            lo: a.min(b),
            hi: a.max(b),
        }
    }

    pub fn point(v: f64) -> Range {
        Range { lo: v, hi: v }
    }

    /// `nominal` with a symmetric fractional tolerance (`0.01` is 1 %).
    pub fn tol(nominal: f64, fraction: f64) -> Range {
        Range::new(nominal * (1.0 - fraction), nominal * (1.0 + fraction))
    }

    /// What nothing has been added to: `include` makes it a point.
    pub const EMPTY: Range = Range {
        lo: f64::INFINITY,
        hi: f64::NEG_INFINITY,
    };

    pub fn is_empty(self) -> bool {
        self.lo > self.hi
    }

    pub fn is_point(self) -> bool {
        self.lo == self.hi
    }

    pub fn mid(self) -> f64 {
        0.5 * (self.lo + self.hi)
    }

    pub fn width(self) -> f64 {
        self.hi - self.lo
    }

    /// The values a corner enumeration tries: one for a point, the two ends otherwise.
    pub fn corners(self) -> Vec<f64> {
        if self.is_point() {
            vec![self.lo]
        } else {
            vec![self.lo, self.hi]
        }
    }

    pub fn include(&mut self, v: f64) {
        self.lo = self.lo.min(v);
        self.hi = self.hi.max(v);
    }

    /// Whether `v` is inside, allowing `slack` (absolute) at both ends.
    pub fn contains(self, v: f64, slack: f64) -> bool {
        v >= self.lo - slack && v <= self.hi + slack
    }

    /// The largest magnitude in the range.
    pub fn abs_max(self) -> f64 {
        self.lo.abs().max(self.hi.abs())
    }
}

/// A number in engineering notation with an SI prefix and four significant figures at most:
/// `15.84u`, `3.3`, `-10.1m`.
pub fn si(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return format!("{v}");
    }
    let prefixes = [
        (1e9, "G"),
        (1e6, "M"),
        (1e3, "k"),
        (1.0, ""),
        (1e-3, "m"),
        (1e-6, "u"),
        (1e-9, "n"),
        (1e-12, "p"),
    ];
    let a = v.abs();
    let (scale, prefix) = prefixes
        .iter()
        .copied()
        .find(|&(s, _)| a >= s * 0.9995)
        .unwrap_or((1e-12, "p"));
    let mut text = format!("{:.4}", v / scale);
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    format!("{text}{prefix}")
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            write!(f, "(none)")
        } else if self.is_point() {
            write!(f, "{}", si(self.lo))
        } else {
            write!(f, "{}..{}", si(self.lo), si(self.hi))
        }
    }
}
