//! Sine, cosine and tangent from additions, multiplications and divisions alone.
//!
//! The standard library's `f64::sin` and friends call the platform's maths library, whose last
//! bit differs between Linux, macOS and Windows. A circle's points and a camera's field of view
//! feed every pixel of a render, so they are computed here instead: a reduction to
//! `[-pi/4, pi/4]` and a Taylor polynomial, in plain `f64` operations, which IEEE 754 rounds the
//! same way everywhere (Rust never fuses a multiply and an add on its own), for |x| up to about
//! a million radians (Cody and Waite's reduction). The result is within
//! a few units in the last place of the true value, which is far below what a picture shows.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_6, PI};

const PIO2_HI: f64 = 1.570_796_326_734_125_6;
const PIO2_LO: f64 = 6.077_100_506_506_192e-11;

/// The sine and cosine of `x` in `[-pi/4, pi/4]`.
fn kernel(x: f64) -> (f64, f64) {
    let x2 = x * x;
    // Taylor series to x^17 and x^18: the next term is below 1e-19 on this interval.
    let mut s = 1.0;
    let mut c = 1.0;
    let mut k = 18.0;
    while k > 1.0 {
        c = 1.0 - c * x2 / (k * (k - 1.0));
        s = 1.0 - s * x2 / ((k + 1.0) * k);
        k -= 2.0;
    }
    (x * s, c)
}

/// The sine and cosine of `x` (radians), the same bits on every machine.
pub(crate) fn sin_cos(x: f64) -> (f64, f64) {
    if !x.is_finite() {
        return (f64::NAN, f64::NAN);
    }
    // x = q * pi/2 + r with |r| <= pi/4 (and a little more for a huge x, where nothing is exact).
    let q = (x / FRAC_PI_2).round();
    // Cody and Waite: pi/2 in two parts, the first with 33 bits so that q times it is exact.
    let r = (x - q * PIO2_HI) - q * PIO2_LO;
    let (s, c) = kernel(r);
    match (q.rem_euclid(4.0)) as u8 {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

/// The sine and cosine of `turns` whole turns: exact at the quarter turns, where `sin_cos` of
/// the radians would leave a 1e-16 instead of a 0.
pub(crate) fn sin_cos_turns(turns: f64) -> (f64, f64) {
    let q = (turns * 4.0).round();
    let r = (turns * 4.0 - q) * FRAC_PI_2;
    let (s, c) = if r == 0.0 { (0.0, 1.0) } else { kernel(r) };
    match (q.rem_euclid(4.0)) as u8 {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

/// The tangent of `x` (radians).
pub(crate) fn tan(x: f64) -> f64 {
    let (s, c) = sin_cos(x);
    s / c
}

/// The arc tangent of `x` in `[0, tan(pi/12)]`: a Taylor series to x^31 (the next term is below
/// 1e-19 there).
fn atan_kernel(x: f64) -> f64 {
    let x2 = x * x;
    let mut s = 0.0;
    let mut k = 31.0;
    while k > 0.0 {
        s = 1.0 / k - x2 * s;
        k -= 2.0;
    }
    x * s
}

/// The arc tangent of `x` (radians), the same bits on every machine.
pub(crate) fn atan(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let (sign, a) = if x < 0.0 { (-1.0, -x) } else { (1.0, x) };
    // atan(a) = pi/2 - atan(1/a) brings a into [0, 1]; then atan(a) = pi/6 + atan((a - 1/sqrt 3)
    // / (1 + a/sqrt 3)) brings it to within tan(pi/12) of 0.
    let (base, a) = if a > 1.0 {
        (FRAC_PI_2, 1.0 / a)
    } else {
        (0.0, a)
    };
    const TAN_PI_12: f64 = 0.267_949_192_431_122_7;
    const INV_SQRT3: f64 = 0.577_350_269_189_625_8;
    let t = if a > TAN_PI_12 {
        FRAC_PI_6 + atan_kernel((a - INV_SQRT3) / (1.0 + a * INV_SQRT3))
    } else {
        atan_kernel(a)
    };
    let r = if base > 0.0 { base - t } else { t };
    sign * r
}

/// The angle of the point `(x, y)` from the x axis, in `(-pi, pi]`, as `f64::atan2` gives it,
/// the same bits on every machine. `(0, 0)` gives 0.
pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    if x == 0.0 && y == 0.0 {
        return 0.0;
    }
    if y.abs() <= x.abs() {
        let t = atan(y / x);
        if x > 0.0 {
            t
        } else if y >= 0.0 {
            t + PI
        } else {
            t - PI
        }
    } else {
        let t = -atan(x / y);
        if y > 0.0 {
            t + FRAC_PI_2
        } else {
            t - FRAC_PI_2
        }
    }
}

/// Degrees to radians.
pub(crate) fn radians(degrees: f64) -> f64 {
    degrees * (PI / 180.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agrees_with_the_platform_to_a_few_ulps() {
        for i in -2000..=2000 {
            let x = f64::from(i) * 0.0123;
            let (s, c) = sin_cos(x);
            assert!((s - x.sin()).abs() < 4e-16, "sin {x}");
            assert!((c - x.cos()).abs() < 4e-16, "cos {x}");
        }
        assert!((tan(radians(30.0)) - 30f64.to_radians().tan()).abs() < 1e-15);
    }

    #[test]
    fn atan2_agrees_with_the_platform_to_a_few_ulps() {
        for i in -60..=60 {
            for j in -60..=60 {
                let (y, x) = (f64::from(i) * 0.37, f64::from(j) * 0.29);
                if x == 0.0 && y == 0.0 {
                    continue;
                }
                assert!((atan2(y, x) - y.atan2(x)).abs() < 1e-15, "atan2 {y} {x}");
            }
        }
        for x in [1e-9, 0.1, 0.2679, 0.268, 0.5, 1.0, 1.5, 7.0, 1e9] {
            assert!((atan(x) - x.atan()).abs() < 4e-16, "atan {x}");
            assert_eq!(atan(-x), -atan(x));
        }
        assert_eq!(atan2(0.0, 0.0), 0.0);
        assert_eq!(atan2(0.0, -1.0), PI);
    }

    #[test]
    fn quarter_turns_are_exact() {
        assert_eq!(sin_cos_turns(0.0), (0.0, 1.0));
        assert_eq!(sin_cos_turns(0.25), (1.0, -0.0));
        assert_eq!(sin_cos_turns(0.5), (-0.0, -1.0));
        assert_eq!(sin_cos_turns(0.75), (-1.0, 0.0));
        let (s, c) = sin_cos_turns(1.0 / 8.0);
        assert!((s - c).abs() < 3e-16 && (s - std::f64::consts::FRAC_1_SQRT_2).abs() < 3e-16);
    }
}
