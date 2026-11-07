//! A range ends where the arithmetic says it ends: only an operation that rounded moves an
//! end, and then it moves it past the true value (ADR 0056).
//!
//! The first test is the board that found the fault: a supply of 9 V to 16 V behind a
//! series resistor whose drop starts at exactly 0 V. The drop used to be widened below
//! zero, and the supply less that drop was then widened above 16 V, so `7V..16V` did not
//! enclose it. Every value here is invented.

use netsmith_quantity::{Dimension, Quantity};

fn q(lo: f64, hi: f64, unit: &str) -> Quantity {
    let lo = Quantity::literal(lo, Some(unit)).unwrap().quantity;
    let hi = Quantity::literal(hi, Some(unit)).unwrap().quantity;
    Quantity::range(&lo, &hi).unwrap()
}

#[test]
fn a_range_that_ends_at_the_supplys_maximum_stays_inside_it() {
    let supply = q(9.0, 16.0, "V");
    let current = q(0.0, 1.0, "A");
    let resistance = q(0.099, 0.101, "ohm");
    let drop = current.mul(&resistance).unwrap();
    assert_eq!(drop.lo(), 0.0, "a product with zero is exactly zero");
    let after = supply.sub(&drop).unwrap();
    assert_eq!(after.hi(), 16.0, "16 V less a drop of at least 0 V");
    assert!(q(7.0, 16.0, "V").encloses(&after).unwrap());
    // The lower end is rounded, and rounded down.
    let exact_low = 9.0 - 0.101;
    assert!(after.lo() <= exact_low && after.lo() > exact_low - 1e-12);
}

#[test]
fn a_range_that_really_goes_past_the_limit_is_still_outside() {
    // The drop can be slightly negative (a source of current in the other direction).
    let supply = q(9.0, 16.0, "V");
    let drop = q(-1e-9, 1.0, "V");
    let after = supply.sub(&drop).unwrap();
    assert!(after.hi() > 16.0);
    assert!(!q(7.0, 16.0, "V").encloses(&after).unwrap());
    // And a tiny rounding on a value that is not exact never lets it pass a limit.
    let a = q(0.1, 0.1, "V");
    let b = q(0.2, 0.2, "V");
    let sum = a.add(&b).unwrap();
    assert!(sum.lo() < sum.hi(), "0.1 + 0.2 is not exact in doubles");
    assert!(sum.lo() <= 0.3 && 0.3 <= sum.hi());
    let exact = q(0.5, 0.5, "V").add(&q(0.25, 0.25, "V")).unwrap();
    assert!(exact.is_point() && exact.lo() == 0.75);
}

#[test]
fn a_rounded_product_or_quotient_is_enclosed_and_an_exact_one_is_a_point() {
    let ratio = |lo: f64, hi: f64| Quantity::interval(Dimension::NONE, lo, hi).unwrap();
    let third = ratio(1.0, 1.0).div(&ratio(3.0, 3.0)).unwrap();
    assert!(third.lo() <= 1.0 / 3.0 && 1.0 / 3.0 <= third.hi());
    assert!(third.lo() < third.hi());
    // 3 * (1/3) is not 1 exactly, and the range must say it may be a hair either side.
    let back = third.mul(&ratio(3.0, 3.0)).unwrap();
    assert!(back.lo() <= 1.0 && 1.0 <= back.hi());
    let quarter = ratio(1.0, 1.0).div(&ratio(4.0, 4.0)).unwrap();
    assert!(quarter.is_point() && quarter.lo() == 0.25);
    let zero = ratio(0.0, 0.0).div(&ratio(3.0, 5.0)).unwrap();
    assert!(zero.is_point() && zero.lo() == 0.0 && !zero.lo().is_sign_negative());
}

/// A product of two integers below 2^40 is below 2^80: an integer that a double holds only
/// to 53 bits. The range must hold the exact product, which `u128` has.
#[test]
fn a_product_range_holds_the_exact_product() {
    let mut x: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x >> 24) | 1
    };
    for _ in 0..2000 {
        let (a, b) = (next(), next());
        let (fa, fb) = (a as f64, b as f64);
        let p = Quantity::interval(Dimension::NONE, fa, fa)
            .unwrap()
            .mul(&Quantity::interval(Dimension::NONE, fb, fb).unwrap())
            .unwrap();
        let exact = u128::from(a) * u128::from(b);
        assert!(
            p.lo() as u128 <= exact && exact <= p.hi() as u128,
            "{a} * {b}"
        );
        if p.is_point() {
            assert_eq!(p.lo() as u128, exact, "{a} * {b} said exact");
        }
    }
}
