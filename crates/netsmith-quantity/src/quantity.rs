//! A dimensioned interval `[lo, hi]` in SI base units.
//!
//! Every arithmetic result is rounded outward by one unit in the last place, so a
//! computed range always contains the true one. That makes a requirement check
//! conservative: it can reject a design that exactly touches a limit, never accept
//! one that misses it. Values built from literals are not widened.

use crate::dimension::Dimension;
use crate::error::QuantityError;
use crate::unit::parse_unit;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantity {
    dim: Dimension,
    lo: f64,
    hi: f64,
}

/// A parsed literal. `relative` is set for `%` and `ppm`, which only make sense as
/// the tolerance of another value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Literal {
    pub quantity: Quantity,
    pub relative: bool,
}

type Res<T> = Result<T, QuantityError>;

impl Quantity {
    pub fn interval(dim: Dimension, lo: f64, hi: f64) -> Res<Self> {
        if !lo.is_finite() || !hi.is_finite() {
            return Err(QuantityError::NotFinite);
        }
        if lo > hi {
            return Err(QuantityError::EmptyRange);
        }
        Ok(Self { dim, lo, hi })
    }

    pub fn point(dim: Dimension, value: f64) -> Res<Self> {
        Self::interval(dim, value, value)
    }

    /// A number with an optional unit suffix, as written in source: `1.8`, `8mA`, `5%`.
    pub fn literal(value: f64, unit: Option<&str>) -> Res<Literal> {
        let Some(unit) = unit else {
            return Ok(Literal {
                quantity: Self::point(Dimension::NONE, value)?,
                relative: false,
            });
        };
        let unit = parse_unit(unit)?;
        // `value` printed shortest-round-trip, then scaled by a power of ten in one
        // decimal parse, so `8mA` is exactly the double nearest 0.008.
        let scaled: f64 = format!("{value}e{}", unit.exp10)
            .parse()
            .map_err(|_| QuantityError::NotFinite)?;
        Ok(Literal {
            quantity: Self::point(unit.dim, scaled)?,
            relative: unit.relative,
        })
    }

    /// `lo..hi`, the smallest range holding both ends.
    pub fn range(lo: &Self, hi: &Self) -> Res<Self> {
        same_dim("form a range from", lo, hi)?;
        Self::interval(lo.dim, lo.lo, hi.hi)
    }

    /// `center +/- tol`. A relative tolerance (`5%`) scales the center; an absolute
    /// one (`10mV`) must have the center's dimension.
    pub fn tolerance(center: &Self, tol: &Self, tol_relative: bool) -> Res<Self> {
        if tol_relative {
            if !tol.dim.is_dimensionless() {
                return Err(QuantityError::BadTolerance {
                    center: center.dim,
                    tolerance: tol.dim,
                });
            }
            let t = tol.lo.abs().max(tol.hi.abs());
            if center.is_point() {
                let half = center.lo.abs() * t;
                return Self::interval(center.dim, center.lo - half, center.lo + half);
            }
            let factor = Self::interval(Dimension::NONE, 1.0 - t, 1.0 + t)?;
            return center.mul(&factor);
        }
        if tol.dim != center.dim {
            return Err(QuantityError::BadTolerance {
                center: center.dim,
                tolerance: tol.dim,
            });
        }
        let t = tol.lo.abs().max(tol.hi.abs());
        if center.is_point() {
            return Self::interval(center.dim, center.lo - t, center.lo + t);
        }
        Self::interval(center.dim, down(center.lo - t), up(center.hi + t))
    }

    pub fn dim(&self) -> Dimension {
        self.dim
    }

    pub fn lo(&self) -> f64 {
        self.lo
    }

    pub fn hi(&self) -> f64 {
        self.hi
    }

    pub fn mid(&self) -> f64 {
        self.lo / 2.0 + self.hi / 2.0
    }

    pub fn is_point(&self) -> bool {
        self.lo == self.hi
    }

    pub fn neg(&self) -> Self {
        Self {
            dim: self.dim,
            lo: -self.hi,
            hi: -self.lo,
        }
    }

    pub fn add(&self, other: &Self) -> Res<Self> {
        same_dim("add", self, other)?;
        Self::widened(self.dim, self.lo + other.lo, self.hi + other.hi)
    }

    pub fn sub(&self, other: &Self) -> Res<Self> {
        same_dim("subtract", self, other)?;
        Self::widened(self.dim, self.lo - other.hi, self.hi - other.lo)
    }

    pub fn mul(&self, other: &Self) -> Res<Self> {
        let products = [
            self.lo * other.lo,
            self.lo * other.hi,
            self.hi * other.lo,
            self.hi * other.hi,
        ];
        let (lo, hi) = extremes(&products);
        Self::widened(self.dim * other.dim, lo, hi)
    }

    pub fn div(&self, other: &Self) -> Res<Self> {
        if other.lo <= 0.0 && other.hi >= 0.0 {
            return Err(QuantityError::DivisorContainsZero);
        }
        let quotients = [
            self.lo / other.lo,
            self.lo / other.hi,
            self.hi / other.lo,
            self.hi / other.hi,
        ];
        let (lo, hi) = extremes(&quotients);
        Self::widened(self.dim / other.dim, lo, hi)
    }

    /// The values both ranges allow, or `None` if they do not overlap.
    pub fn intersect(&self, other: &Self) -> Res<Option<Self>> {
        same_dim("intersect", self, other)?;
        let (lo, hi) = (self.lo.max(other.lo), self.hi.min(other.hi));
        Ok((lo <= hi).then_some(Self {
            dim: self.dim,
            lo,
            hi,
        }))
    }

    /// Whether every value of `other` lies inside `self`.
    pub fn encloses(&self, other: &Self) -> Res<bool> {
        same_dim("compare", self, other)?;
        Ok(self.lo <= other.lo && other.hi <= self.hi)
    }

    fn widened(dim: Dimension, lo: f64, hi: f64) -> Res<Self> {
        Self::interval(dim, down(lo), up(hi))
    }
}

fn same_dim(op: &'static str, a: &Quantity, b: &Quantity) -> Res<()> {
    if a.dim == b.dim {
        Ok(())
    } else {
        Err(QuantityError::DimensionMismatch {
            op,
            left: a.dim,
            right: b.dim,
        })
    }
}

fn extremes(values: &[f64; 4]) -> (f64, f64) {
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (lo, hi)
}

fn down(x: f64) -> f64 {
    if x.is_finite() { x.next_down() } else { x }
}

fn up(x: f64) -> f64 {
    if x.is_finite() { x.next_up() } else { x }
}
