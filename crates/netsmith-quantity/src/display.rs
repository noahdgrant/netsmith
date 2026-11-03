//! Engineering-notation text for quantities (`8mA`, `2.55V..3.45V`, `300Ω`).

use crate::quantity::Quantity;

const PREFIXES: [(i32, &str); 9] = [
    (-15, "f"),
    (-12, "p"),
    (-9, "n"),
    (-6, "u"),
    (-3, "m"),
    (0, ""),
    (3, "k"),
    (6, "M"),
    (9, "G"),
];

/// Significant digits shown: enough to read, few enough to be a number a person says.
const SIGNIFICANT: i32 = 4;

/// The most digits a range goes to when fewer would round it much wider than it is.
const WIDEST: i32 = 9;

/// Below this (the smallest prefix, femto) a value is what interval arithmetic leaves of a
/// cancellation, not a quantity: it is shown as `0`.
const NEGLIGIBLE: f64 = 1e-15;

/// Which way a value is rounded to the digits shown.
#[derive(Clone, Copy)]
enum Round {
    Nearest,
    /// Toward minus infinity: the low end of a range, so the text never claims a narrower
    /// range than the quantity is.
    Down,
    Up,
}

impl std::fmt::Display for Quantity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let symbol = self.dim().unit_symbol();
        let render = |v: f64, round: Round, digits: i32| match symbol {
            Some(s) => with_prefix(v, s, round, digits),
            None if self.dim().is_dimensionless() => trim(round_to(v, round, digits), digits),
            None => format!(
                "{} {}",
                trim(round_to(v, round, digits), digits),
                self.dim()
            ),
        };
        // Outward rounding leaves computed "points" a hair wide; show them as points.
        let scale = self.lo().abs().max(self.hi().abs());
        // What interval arithmetic leaves of a cancellation (`137mA - 137mA` is a few
        // thousandths of a femtoampere, or less) is zero.
        let tidy = |v: f64| {
            if v.abs() < NEGLIGIBLE || v.abs() < scale * 1e-12 {
                0.0
            } else {
                v
            }
        };
        if self.hi() - self.lo() <= scale * 1e-12 {
            let mid = if self.mid().abs() < NEGLIGIBLE {
                0.0
            } else {
                self.mid()
            };
            write!(f, "{}", render(mid, Round::Nearest, SIGNIFICANT))
        } else {
            let (lo, hi) = (tidy(self.lo()), tidy(self.hi()));
            // Digits are added until rounding outward has not made the range much wider than
            // it is (`1.000001..1.000004` is not `1..1.001`).
            let mut digits = SIGNIFICANT;
            while digits < WIDEST
                && round_to(hi, Round::Up, digits) - round_to(lo, Round::Down, digits)
                    > (hi - lo) * 1.25
            {
                digits += 1;
            }
            write!(
                f,
                "{}..{}",
                render(lo, Round::Down, digits),
                render(hi, Round::Up, digits)
            )
        }
    }
}

/// `value` rounded to `digits` significant digits, in the direction `round`.
fn round_to(value: f64, round: Round, digits: i32) -> f64 {
    if value == 0.0 || !value.is_finite() {
        return 0.0 * value;
    }
    let magnitude = value.abs().log10().floor() as i32 + 1;
    let factor = 10f64.powi(digits - magnitude);
    let scaled = value * factor;
    let nearest = scaled.round();
    // `3.58` times 100 is `358.00000000000006` or `357.99999999999994`: a value that is a
    // whole number of steps, to within what a float can say, is one.
    let steps = if (scaled - nearest).abs() < 1e-6 * nearest.abs().max(1.0) {
        nearest
    } else {
        match round {
            Round::Nearest => nearest,
            Round::Down => scaled.floor(),
            Round::Up => scaled.ceil(),
        }
    };
    let out = steps / factor;
    if out == 0.0 { 0.0 } else { out }
}

fn with_prefix(value: f64, symbol: &str, round: Round, digits: i32) -> String {
    let value = round_to(value, round, digits);
    if value == 0.0 {
        return format!("0{symbol}");
    }
    let exp3 = ((value.abs().log10() / 3.0).floor() as i32 * 3).clamp(-15, 9);
    let (_, prefix) = PREFIXES
        .iter()
        .find(|(e, _)| *e == exp3)
        .copied()
        .unwrap_or((0, ""));
    let mantissa = value / 10f64.powi(exp3);
    format!("{}{prefix}{symbol}", trim(mantissa, digits))
}

/// `significant` significant digits, without trailing zeros.
fn trim(value: f64, significant: i32) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let magnitude = value.abs().log10().floor() as i32 + 1;
    let decimals = (significant - magnitude).max(0) as usize;
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use crate::Quantity;

    fn q(value: f64, unit: &str) -> Quantity {
        Quantity::literal(value, Some(unit)).unwrap().quantity
    }

    #[test]
    fn points_use_engineering_prefixes() {
        assert_eq!(q(8.0, "mA").to_string(), "8mA");
        assert_eq!(q(300.0, "ohm").to_string(), "300Ω");
        assert_eq!(q(1.0, "kohm").to_string(), "1kΩ");
        assert_eq!(q(100.0, "nF").to_string(), "100nF");
        assert_eq!(q(3.3, "V").to_string(), "3.3V");
        assert_eq!(q(0.0, "V").to_string(), "0V");
        assert_eq!(q(-2.5, "mV").to_string(), "-2.5mV");
    }

    #[test]
    fn ranges_show_both_ends() {
        let r = Quantity::range(&q(1.8, "V"), &q(2.2, "V")).unwrap();
        assert_eq!(r.to_string(), "1.8V..2.2V");
    }

    #[test]
    fn computed_points_do_not_show_rounding_noise() {
        let v = q(5.0, "V").sub(&q(2.0, "V")).unwrap();
        assert_eq!(v.to_string(), "3V");
    }

    #[test]
    fn dimensionless_and_unnamed_dimensions() {
        let n = Quantity::literal(2.5, None).unwrap().quantity;
        assert_eq!(n.to_string(), "2.5");
        let area = q(3.0, "m").mul(&q(2.0, "m")).unwrap();
        assert_eq!(area.to_string(), "6 m^2");
    }

    fn interval(dim: crate::Dimension, lo: f64, hi: f64) -> Quantity {
        Quantity::interval(dim, lo, hi).unwrap()
    }

    #[test]
    fn what_cancellation_leaves_is_zero() {
        use crate::Dimension;
        // The sums of a power tree leave a few thousandths of a femtoampere, or a number
        // so small it has hundreds of zeros; both are zero.
        for tiny in [7.41098e-300_f64, 3.3e-19, 1e-16, -2e-17] {
            let range = interval(Dimension::CURRENT, -0.137, tiny.max(-0.137 + 1e-9));
            assert_eq!(range.to_string(), "-137mA..0A", "{tiny}");
            let range = interval(Dimension::CURRENT, tiny.min(0.0), 0.137);
            assert_eq!(range.to_string(), "0A..137mA", "{tiny}");
        }
        assert_eq!(interval(Dimension::CURRENT, 1e-17, 1e-17).to_string(), "0A");
        // A real small value is still shown, down to the smallest prefix.
        assert_eq!(q(0.5, "pF").to_string(), "500fF");
        assert_eq!(q(2.0, "fF").to_string(), "2fF");
        // And a small end of a range that is not a residual (1 mA against 1 uA).
        assert_eq!(
            interval(Dimension::CURRENT, 1e-6, 1e-3).to_string(),
            "1uA..1mA"
        );
    }

    #[test]
    fn numbers_have_four_significant_digits_and_ranges_round_outward() {
        use crate::Dimension;
        let rail = interval(Dimension::VOLTAGE, 3.58464, 3.76509);
        assert_eq!(rail.to_string(), "3.584V..3.766V");
        assert_eq!(
            interval(Dimension::VOLTAGE, 3.135, 3.465).to_string(),
            "3.135V..3.465V"
        );
        // Rounding up past a prefix changes the prefix.
        assert_eq!(
            interval(Dimension::CURRENT, 0.5, 0.99996).to_string(),
            "500mA..1A"
        );
        assert_eq!(q(0.123456, "A").to_string(), "123.5mA");
        assert_eq!(q(12345.678, "ohm").to_string(), "12.35kΩ");
        // A range narrower than four digits shows the digits that tell its ends apart.
        let narrow = interval(Dimension::VOLTAGE, 1.000001, 1.000004);
        assert_eq!(narrow.to_string(), "1.000001V..1.000004V");
    }
}
