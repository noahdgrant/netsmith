//! `degC`, the offset unit: a literal is kelvin underneath, and a difference stays kelvin.

use netsmith_quantity::{Dimension, Quantity, QuantityError};

fn lit(value: f64, unit: &str) -> Quantity {
    Quantity::literal(value, Some(unit)).unwrap().quantity
}

#[test]
fn zero_degc_is_273_15_kelvin() {
    let q = lit(0.0, "degC");
    assert_eq!(
        (q.dim(), q.lo(), q.hi()),
        (Dimension::TEMPERATURE, 273.15, 273.15)
    );
    assert_eq!(lit(25.0, "degC").lo(), 298.15);
    // The nearest double to 233.15, not the one a float sum leaves.
    assert_eq!(lit(-40.0, "degC").lo(), 233.15);
    assert_eq!(lit(125.0, "degC").lo(), 398.15);
    assert_eq!(lit(85.0, "degC").lo(), lit(358.15, "K").lo());
}

#[test]
fn a_range_in_degc_is_a_range_in_kelvin() {
    let r = Quantity::range(&lit(-40.0, "degC"), &lit(125.0, "degC")).unwrap();
    assert_eq!((r.lo(), r.hi()), (233.15, 398.15));
}

#[test]
fn a_temperature_plus_a_rise_is_a_temperature_and_a_difference_is_kelvin() {
    // `10degC + 5K` is 15 degC: the 5 K is a difference.
    let sum = lit(10.0, "degC").add(&lit(5.0, "K")).unwrap();
    assert!((sum.lo() - lit(15.0, "degC").lo()).abs() < 1e-9);
    // `20degC - 10degC` is 10 K: the difference of two temperatures.
    let diff = lit(20.0, "degC").sub(&lit(10.0, "degC")).unwrap();
    assert_eq!(diff.dim(), Dimension::TEMPERATURE);
    assert!((diff.lo() - 10.0).abs() < 1e-9 && (diff.hi() - 10.0).abs() < 1e-9);
    assert_eq!(diff.to_string(), "10K");
}

#[test]
fn degc_takes_no_prefix_and_c_is_still_charge() {
    assert!(matches!(
        Quantity::literal(1.0, Some("mdegC")),
        Err(QuantityError::UnknownUnit(_))
    ));
    assert_eq!(lit(5.0, "nC").dim(), Dimension::CHARGE);
    assert_eq!(lit(2.0, "C").dim(), Dimension::CHARGE);
    assert_eq!(lit(2.0, "C").lo(), 2.0);
}

#[test]
fn degc_is_not_a_relative_unit() {
    assert!(!Quantity::literal(5.0, Some("degC")).unwrap().relative);
}
