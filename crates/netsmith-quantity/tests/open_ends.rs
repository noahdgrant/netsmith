//! A range with an open end (`lo..`, `..hi`): what arithmetic, intersection and comparison
//! do with it. Every value here is invented.

use netsmith_quantity::{Dimension, Quantity, QuantityError};

fn q(value: f64, unit: &str) -> Quantity {
    Quantity::literal(value, Some(unit)).unwrap().quantity
}

fn from(value: f64, unit: &str) -> Quantity {
    Quantity::at_least(&q(value, unit)).unwrap()
}

fn until(value: f64, unit: &str) -> Quantity {
    Quantity::at_most(&q(value, unit)).unwrap()
}

#[test]
fn an_open_end_is_the_infinity_on_its_side() {
    let low = from(1.6, "mm");
    assert_eq!(low.lo(), 1.6e-3);
    assert!(low.is_open_above() && !low.is_open_below() && !low.is_bounded());
    let high = until(5.0, "mm");
    assert_eq!(high.hi(), 5e-3);
    assert!(high.is_open_below() && !high.is_open_above() && !high.is_bounded());
    assert!(q(1.0, "mm").is_bounded());
    assert!(!low.mid().is_finite());
}

#[test]
fn the_end_of_a_range_is_the_end_of_what_is_written() {
    // `lo..` takes the lowest end of a range written for `lo`, as `lo..hi` does.
    let lo = Quantity::range(&q(1.0, "V"), &q(2.0, "V")).unwrap();
    assert_eq!(Quantity::at_least(&lo).unwrap().lo(), 1.0);
    assert_eq!(Quantity::at_most(&lo).unwrap().hi(), 2.0);
    // An end that is itself open has no number to take.
    assert_eq!(
        Quantity::at_least(&until(2.0, "V")),
        Err(QuantityError::OpenEnd {
            op: "start a range"
        })
    );
    assert_eq!(
        Quantity::at_most(&from(2.0, "V")),
        Err(QuantityError::OpenEnd { op: "end a range" })
    );
}

#[test]
fn arithmetic_keeps_an_open_end_open() {
    let a = from(1.0, "V");
    let sum = a.add(&q(2.0, "V")).unwrap();
    assert_eq!((sum.lo(), sum.is_open_above()), (3.0, true));
    let diff = q(5.0, "V").sub(&a).unwrap();
    assert_eq!((diff.hi(), diff.is_open_below()), (4.0, true));
    let both = a.add(&a).unwrap();
    assert_eq!((both.lo(), both.is_open_above()), (2.0, true));
    let neg = a.neg();
    assert_eq!((neg.hi(), neg.is_open_below()), (-1.0, true));
    let scaled = a.mul(&q(2.0, "A")).unwrap();
    assert_eq!((scaled.lo(), scaled.is_open_above()), (2.0, true));
    assert_eq!(scaled.dim(), Dimension::VOLTAGE * Dimension::CURRENT);
    let over = q(6.0, "V").div(&a).unwrap();
    assert_eq!((over.lo(), over.hi()), (0.0, 6.0), "6 V over 1 V or more");
}

#[test]
fn zero_times_an_open_end_is_zero() {
    let product = q(0.0, "A").mul(&from(1.0, "V")).unwrap();
    assert_eq!((product.lo(), product.hi()), (0.0, 0.0));
    let quotient = q(0.0, "V").div(&from(1.0, "A")).unwrap();
    assert_eq!((quotient.lo(), quotient.hi()), (0.0, 0.0));
}

#[test]
fn an_open_end_over_an_open_end_is_anything_of_the_quotients_sign() {
    let same = from(1.0, "V").div(&from(1.0, "A")).unwrap();
    assert_eq!((same.lo(), same.is_open_above()), (0.0, true));
    let other = from(1.0, "V").div(&until(-1.0, "A")).unwrap();
    assert_eq!((other.is_open_below(), other.hi()), (true, 0.0));
}

#[test]
fn a_divisor_that_reaches_zero_from_below_is_refused() {
    assert_eq!(
        q(1.0, "V").div(&until(1.0, "A")),
        Err(QuantityError::DivisorContainsZero)
    );
}

#[test]
fn a_closed_overflow_is_still_an_error() {
    let huge = q(1e200, "V");
    assert_eq!(huge.mul(&huge), Err(QuantityError::NotFinite));
}

#[test]
fn a_tolerance_needs_two_closed_ends() {
    let err = Quantity::tolerance(&from(1.0, "V"), &q(5.0, "%"), true);
    assert!(matches!(err, Err(QuantityError::OpenEnd { .. })), "{err:?}");
}

#[test]
fn narrowing_and_comparison_read_the_open_end() {
    let limit = from(0.1, "mm");
    let tighter = limit.intersect(&from(0.15, "mm")).unwrap().unwrap();
    assert_eq!((tighter.lo(), tighter.is_open_above()), (0.15e-3, true));
    let band = limit.intersect(&until(1.0, "mm")).unwrap().unwrap();
    assert_eq!((band.lo(), band.hi()), (0.1e-3, 1e-3));
    assert_eq!(until(0.05, "mm").intersect(&limit).unwrap(), None);
    assert!(limit.encloses(&q(3.0, "mm")).unwrap());
    assert!(!limit.encloses(&q(0.05, "mm")).unwrap());
    assert!(!q(3.0, "mm").encloses(&limit).unwrap());
    assert!(limit.encloses(&from(0.2, "mm")).unwrap());
}

#[test]
fn a_range_from_an_open_end_is_open() {
    let r = Quantity::range(&q(1.0, "V"), &from(3.0, "V")).unwrap();
    assert_eq!((r.lo(), r.is_open_above()), (1.0, true));
}
