//! The worked LED example from the design discussion, as executable checks.
//!
//! A 5 V +/- 5% supply drives a resistor and an LED with a 1.8 V..2.2 V forward
//! voltage. The LED current must stay within 8 mA..12 mA in the worst case.

use netsmith_quantity::{Dimension, Quantity, QuantityError};

fn q(value: f64, unit: &str) -> Quantity {
    Quantity::literal(value, Some(unit)).unwrap().quantity
}

fn supply() -> Quantity {
    let lit = Quantity::literal(5.0, Some("V")).unwrap().quantity;
    let tol = Quantity::literal(5.0, Some("%")).unwrap();
    assert!(tol.relative);
    Quantity::tolerance(&lit, &tol.quantity, true).unwrap()
}

fn vf() -> Quantity {
    Quantity::range(&q(1.8, "V"), &q(2.2, "V")).unwrap()
}

/// Worst-case current for a resistor with the given nominal value and tolerance.
fn led_current(nominal_ohms: f64, tolerance_percent: f64) -> Quantity {
    let r = Quantity::tolerance(
        &q(nominal_ohms, "ohm"),
        &Quantity::literal(tolerance_percent, Some("%"))
            .unwrap()
            .quantity,
        true,
    )
    .unwrap();
    supply().sub(&vf()).unwrap().div(&r).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-30)
}

#[test]
fn literals_are_exact() {
    assert_eq!(q(8.0, "mA").lo(), 0.008);
    assert_eq!(q(100.0, "nF").lo(), 1e-7);
    assert_eq!(q(1.0, "kohm").lo(), 1000.0);
    assert_eq!(q(5.0, "%").lo(), 0.05);
}

#[test]
fn supply_and_headroom() {
    let s = supply();
    assert_eq!((s.lo(), s.hi()), (4.75, 5.25));
    let headroom = s.sub(&vf()).unwrap();
    assert!(
        close(headroom.lo(), 2.55) && close(headroom.hi(), 3.45),
        "{headroom}"
    );
    assert_eq!(headroom.dim(), Dimension::VOLTAGE);
}

#[test]
fn a_300_ohm_one_percent_resistor_meets_the_requirement() {
    let i = led_current(300.0, 1.0);
    assert_eq!(i.dim(), Dimension::CURRENT);
    assert!(
        close(i.lo(), 2.55 / 303.0) && close(i.hi(), 3.45 / 297.0),
        "{i}"
    );
    assert_eq!(i.to_string().split("..").count(), 2);
    assert!(q(8.0, "mA").lo() <= i.lo(), "{i}");
    assert!(
        Quantity::range(&q(8.0, "mA"), &q(12.0, "mA"))
            .unwrap()
            .encloses(&i)
            .unwrap()
    );
}

#[test]
fn a_220_ohm_resistor_violates_it_at_the_high_end() {
    // The tempting value from naive interval inversion (about 212 to 431 ohms).
    let i = led_current(220.0, 1.0);
    let allowed = Quantity::range(&q(8.0, "mA"), &q(12.0, "mA")).unwrap();
    assert!(!allowed.encloses(&i).unwrap());
    assert!(i.hi() > 0.0158 && i.hi() < 0.0159, "{i}");
}

#[test]
fn the_exact_window_for_an_ideal_resistor_is_287_5_to_318_75() {
    let allowed = Quantity::range(&q(8.0, "mA"), &q(12.0, "mA")).unwrap();
    let ok = |ohms: f64| allowed.encloses(&led_current(ohms, 0.0)).unwrap();
    assert!(ok(288.0) && ok(300.0) && ok(318.0));
    assert!(!ok(287.0) && !ok(319.0));
}

#[test]
fn naive_inversion_gives_a_much_wider_window() {
    let headroom = supply().sub(&vf()).unwrap();
    let r = headroom
        .div(&Quantity::range(&q(8.0, "mA"), &q(12.0, "mA")).unwrap())
        .unwrap();
    assert!(close(r.lo(), 212.5) && close(r.hi(), 431.25), "{r}");
}

#[test]
fn dimensions_are_enforced() {
    assert!(matches!(
        q(5.0, "V").add(&q(1.0, "mA")),
        Err(QuantityError::DimensionMismatch { .. })
    ));
    assert_eq!(
        q(5.0, "V").div(&q(2.0, "mA")).unwrap().dim(),
        Dimension::RESISTANCE
    );
    assert_eq!(
        q(5.0, "V").mul(&q(2.0, "mA")).unwrap().dim(),
        Dimension::POWER
    );
    assert_eq!(
        q(5.0, "V").sub(&q(1.0, "A")).map(|_| ()),
        Err(QuantityError::DimensionMismatch {
            op: "subtract",
            left: Dimension::VOLTAGE,
            right: Dimension::CURRENT,
        })
    );
}

#[test]
fn division_by_a_range_that_includes_zero_is_refused() {
    let around_zero = Quantity::range(&q(-1.0, "mA"), &q(1.0, "mA")).unwrap();
    assert_eq!(
        q(1.0, "V").div(&around_zero),
        Err(QuantityError::DivisorContainsZero)
    );
    let zero = Quantity::point(Dimension::CURRENT, 0.0).unwrap();
    assert_eq!(
        q(1.0, "V").div(&zero),
        Err(QuantityError::DivisorContainsZero)
    );
}

#[test]
fn tolerance_forms() {
    // Absolute: 3.3 V +/- 100 mV.
    let v = Quantity::tolerance(&q(3.3, "V"), &q(100.0, "mV"), false).unwrap();
    assert!(close(v.lo(), 3.2) && close(v.hi(), 3.4));
    // A tolerance of the wrong kind is an error, not a guess.
    assert!(matches!(
        Quantity::tolerance(&q(3.3, "V"), &q(5.0, "mA"), false),
        Err(QuantityError::BadTolerance { .. })
    ));
    assert!(matches!(
        Quantity::tolerance(&q(3.3, "V"), &q(5.0, "V"), true),
        Err(QuantityError::BadTolerance { .. })
    ));
}

#[test]
fn ranges_and_intersection() {
    assert_eq!(
        Quantity::range(&q(2.0, "V"), &q(1.0, "V")),
        Err(QuantityError::EmptyRange)
    );
    let a = Quantity::range(&q(1.0, "V"), &q(3.0, "V")).unwrap();
    let b = Quantity::range(&q(2.0, "V"), &q(5.0, "V")).unwrap();
    let both = a.intersect(&b).unwrap().unwrap();
    assert_eq!((both.lo(), both.hi()), (2.0, 3.0));
    let far = Quantity::range(&q(4.0, "V"), &q(5.0, "V")).unwrap();
    assert_eq!(a.intersect(&far).unwrap(), None);
    assert!(a.intersect(&q(1.0, "A")).is_err());
}

#[test]
fn computed_ranges_contain_the_exact_answer() {
    // 0.1 + 0.2 is not exactly 0.3 in binary; the widened range must still hold it.
    let sum = q(100.0, "mV").add(&q(200.0, "mV")).unwrap();
    assert!(sum.lo() <= 0.3 && 0.3 <= sum.hi());
    assert!(
        sum.lo() < sum.hi(),
        "arithmetic results are widened outward"
    );
}
