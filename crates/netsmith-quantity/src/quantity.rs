//! A dimensioned interval `[lo, hi]` in SI base units.
//!
//! Every arithmetic result is rounded outward, and only where the operation rounded: an
//! end that the operation computes exactly (a sum of two doubles that fits, a product with
//! zero, a quotient with no remainder) stays where it is, and an end that was rounded moves
//! one unit in the last place towards the true value's side, so a computed range always
//! contains the true one. The rounding of one operation is found exactly (TwoSum for a
//! sum, a fused multiply-add for a product and a quotient), not guessed. That makes a
//! requirement check conservative: it can reject a design that touches a limit only by a
//! rounding error, never accept one that misses it, and a range that ends exactly at a
//! limit (`16V - 0A * 100mΩ` against `..16V`) is not pushed past it by a widening that was
//! not needed (ADR 0056). Values built from literals are not widened.
//!
//! An end can be open: `lo..` has no upper limit (the high end is positive infinity) and
//! `..hi` no lower one (negative infinity). Only [`Quantity::at_least`] and
//! [`Quantity::at_most`] make one, and the arithmetic on a range with an open end keeps the
//! end open (`[1, inf) + 2 = [3, inf)`, `0 * [1, inf) = 0`); an arithmetic result that
//! overflows, with both operands closed, is still an error. A consumer that needs a number
//! of such a range asks [`Quantity::is_bounded`] first: `mid` of one is not finite.

use crate::dimension::Dimension;
use crate::error::QuantityError;
use crate::unit::{CELSIUS_ZERO_K, parse_unit};

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
        // `+ 0.0` turns a negative zero into a zero, so a range never prints as `-0V`.
        Ok(Self {
            dim,
            lo: lo + 0.0,
            hi: hi + 0.0,
        })
    }

    /// The result of an operation on ranges, at least one of which has an open end: an end
    /// may be infinite there, and only there. A range with no open end is made by
    /// [`Quantity::interval`], which refuses an overflow.
    fn interval_open(dim: Dimension, lo: f64, hi: f64) -> Res<Self> {
        // The only infinite ends a range has: negative at the low end, positive at the high.
        if lo.is_nan() || hi.is_nan() || lo == f64::INFINITY || hi == f64::NEG_INFINITY {
            return Err(QuantityError::NotFinite);
        }
        if lo > hi {
            return Err(QuantityError::EmptyRange);
        }
        Ok(Self {
            dim,
            lo: lo + 0.0,
            hi: hi + 0.0,
        })
    }

    /// The result of an operation: checked as a range with an open end when an operand has
    /// one, as a closed range (no infinite end) when none has.
    fn result(dim: Dimension, lo: f64, hi: f64, operands: &[&Self]) -> Res<Self> {
        if operands.iter().all(|q| q.is_bounded()) {
            Self::interval(dim, lo, hi)
        } else {
            Self::interval_open(dim, lo, hi)
        }
    }

    /// `lo..`: every value from the low end of `lo` up, with no upper limit.
    pub fn at_least(lo: &Self) -> Res<Self> {
        if !lo.lo.is_finite() {
            return Err(QuantityError::OpenEnd {
                op: "start a range",
            });
        }
        Self::interval_open(lo.dim, lo.lo, f64::INFINITY)
    }

    /// `..hi`: every value up to the high end of `hi`, with no lower limit.
    pub fn at_most(hi: &Self) -> Res<Self> {
        if !hi.hi.is_finite() {
            return Err(QuantityError::OpenEnd { op: "end a range" });
        }
        Self::interval_open(hi.dim, f64::NEG_INFINITY, hi.hi)
    }

    /// Whether both ends are numbers: no end is open.
    pub fn is_bounded(&self) -> bool {
        self.lo.is_finite() && self.hi.is_finite()
    }

    /// Whether the range has no upper limit (`lo..`).
    pub fn is_open_above(&self) -> bool {
        self.hi == f64::INFINITY
    }

    /// Whether the range has no lower limit (`..hi`).
    pub fn is_open_below(&self) -> bool {
        self.lo == f64::NEG_INFINITY
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
        if unit.offset {
            // An offset unit: the number on the Celsius scale, in kelvin. The sum is rounded
            // to a nanokelvin so that `-40degC` is the double nearest 233.15 and not
            // 233.14999999999998.
            let kelvin: f64 = format!("{:.9}", value + CELSIUS_ZERO_K)
                .parse()
                .map_err(|_| QuantityError::NotFinite)?;
            return Ok(Literal {
                quantity: Self::point(unit.dim, kelvin)?,
                relative: false,
            });
        }
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
        Self::result(lo.dim, lo.lo, hi.hi, &[lo, hi])
    }

    /// `center +/- tol`. A relative tolerance (`5%`) scales the center; an absolute
    /// one (`10mV`) must have the center's dimension.
    pub fn tolerance(center: &Self, tol: &Self, tol_relative: bool) -> Res<Self> {
        if !center.is_bounded() || !tol.is_bounded() {
            return Err(QuantityError::OpenEnd {
                op: "give a tolerance to",
            });
        }
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

    /// The mean of the ends: not finite for a range with an open end, which has no centre.
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
        Self::result(
            self.dim,
            add_ends(self.lo, other.lo).0,
            add_ends(self.hi, other.hi).1,
            &[self, other],
        )
    }

    pub fn sub(&self, other: &Self) -> Res<Self> {
        same_dim("subtract", self, other)?;
        Self::result(
            self.dim,
            add_ends(self.lo, -other.hi).0,
            add_ends(self.hi, -other.lo).1,
            &[self, other],
        )
    }

    pub fn mul(&self, other: &Self) -> Res<Self> {
        let products = [
            mul_ends(self.lo, other.lo),
            mul_ends(self.lo, other.hi),
            mul_ends(self.hi, other.lo),
            mul_ends(self.hi, other.hi),
        ];
        let (lo, hi) = extremes(&products);
        Self::result(self.dim * other.dim, lo, hi, &[self, other])
    }

    pub fn div(&self, other: &Self) -> Res<Self> {
        if other.lo <= 0.0 && other.hi >= 0.0 {
            return Err(QuantityError::DivisorContainsZero);
        }
        let quotients = [
            div_ends(self.lo, other.lo),
            div_ends(self.lo, other.hi),
            div_ends(self.hi, other.lo),
            div_ends(self.hi, other.hi),
        ];
        let (lo, hi) = extremes(&quotients);
        Self::result(self.dim / other.dim, lo, hi, &[self, other])
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

/// The lowest lower end and the highest upper end of four `(lower, upper)` pairs.
fn extremes(values: &[(f64, f64); 4]) -> (f64, f64) {
    let lo = values.iter().map(|v| v.0).fold(f64::INFINITY, f64::min);
    let hi = values.iter().map(|v| v.1).fold(f64::NEG_INFINITY, f64::max);
    (lo, hi)
}

/// An operand (or a result) outside this range of magnitudes is widened by a full unit and
/// not examined, because the error of a product or a quotient could be lost to underflow
/// there and its sign could not be read. Zero is always examined.
const SAFE_LOW: f64 = 1e-100;
const SAFE_HIGH: f64 = 1e100;

fn plain(x: f64) -> bool {
    let a = x.abs();
    a == 0.0 || (SAFE_LOW..=SAFE_HIGH).contains(&a)
}

/// Where the real result lies, given the rounded one and the sign of the rounding error
/// (`true result - rounded`): the rounded result is the end that the error does not
/// push past, and the other end moves one unit.
fn directed(rounded: f64, error_sign: f64) -> (f64, f64) {
    if error_sign < 0.0 {
        (down(rounded), rounded)
    } else if error_sign > 0.0 {
        (rounded, up(rounded))
    } else {
        (rounded, rounded)
    }
}

/// `a + b` rounded down and rounded up, found exactly (Knuth's TwoSum).
fn add_ends(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    if !s.is_finite() || !a.is_finite() || !b.is_finite() {
        return (s, s);
    }
    let bb = s - a;
    let error = (a - (s - bb)) + (b - bb);
    directed(s, error)
}

/// `a * b` rounded down and rounded up. A product with zero is exact.
fn mul_ends(a: f64, b: f64) -> (f64, f64) {
    // Zero times an open end is zero: the values at the end are finite however far it goes.
    if a == 0.0 || b == 0.0 {
        return (0.0, 0.0);
    }
    let p = a * b;
    if !p.is_finite() {
        return (p, p);
    }
    if !(plain(a) && plain(b)) || p.abs() < SAFE_LOW {
        return (down(p), up(p));
    }
    directed(p, a.mul_add(b, -p))
}

/// `a / b` rounded down and rounded up (`b` is not zero). A quotient of zero is exact.
fn div_ends(a: f64, b: f64) -> (f64, f64) {
    // A number over an open end is zero, and an open end over an open end is anything of the
    // sign the quotient has, from zero to infinity.
    if b.is_infinite() {
        if a.is_finite() {
            return (0.0, 0.0);
        }
        return if (a > 0.0) == (b > 0.0) {
            (0.0, f64::INFINITY)
        } else {
            (f64::NEG_INFINITY, 0.0)
        };
    }
    let q = a / b;
    if !q.is_finite() || a == 0.0 {
        return (q, q);
    }
    if !(plain(a) && plain(b)) || q.abs() < SAFE_LOW {
        return (down(q), up(q));
    }
    // `a - q * b` is the remainder, exactly; the quotient's error has its sign over `b`'s.
    let remainder = (-q).mul_add(b, a);
    directed(q, remainder * b.signum())
}

fn down(x: f64) -> f64 {
    if x.is_finite() { x.next_down() } else { x }
}

fn up(x: f64) -> f64 {
    if x.is_finite() { x.next_up() } else { x }
}
