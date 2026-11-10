//! Unit suffixes such as `mA`, `kohm`, `µF` and `%`.

use crate::dimension::Dimension;
use crate::error::QuantityError;

/// A unit as `10^exp10` of the SI base-unit combination for `dim`.
///
/// Every supported unit and prefix is a power of ten, so literals convert with a
/// single correctly rounded decimal parse (`8mA` is exactly `0.008`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unit {
    pub exp10: i32,
    pub dim: Dimension,
    /// `%` and `ppm`: a fraction of the value they modify, not a unit of their own.
    pub relative: bool,
    /// `degC`: an offset unit, whose zero is not the zero of its dimension. A literal in it is
    /// the number plus 273.15, in kelvin (`0degC` is `273.15K`), and a sign written directly on
    /// the literal belongs to the number, so `-40degC` is 233.15 K and not minus 313.15 K.
    pub offset: bool,
}

/// The kelvin value of `0degC`.
pub const CELSIUS_ZERO_K: f64 = 273.15;

struct Base {
    symbol: &'static str,
    exp10: i32,
    dim: Dimension,
    prefixable: bool,
}

const fn base(symbol: &'static str, exp10: i32, dim: Dimension, prefixable: bool) -> Base {
    Base {
        symbol,
        exp10,
        dim,
        prefixable,
    }
}

const BASES: [Base; 12] = [
    base("m", 0, Dimension::LENGTH, true),
    // The SI base unit is the kilogram, so a gram is 10^-3 of it.
    base("g", -3, Dimension::MASS, true),
    base("s", 0, Dimension::TIME, true),
    base("A", 0, Dimension::CURRENT, true),
    base("K", 0, Dimension::TEMPERATURE, false),
    base("V", 0, Dimension::VOLTAGE, true),
    base("ohm", 0, Dimension::RESISTANCE, true),
    base("Ω", 0, Dimension::RESISTANCE, true),
    base("W", 0, Dimension::POWER, true),
    base("F", 0, Dimension::CAPACITANCE, true),
    base("H", 0, Dimension::INDUCTANCE, true),
    base("Hz", 0, Dimension::FREQUENCY, true),
];

const EXTRA: [Base; 3] = [
    base("S", 0, Dimension::CONDUCTANCE, true),
    // The coulomb. A temperature in Celsius is `degC` (ADR 0102), so that `C` stays charge.
    base("C", 0, Dimension::CHARGE, true),
    base("degC", 0, Dimension::TEMPERATURE, false),
];

const PREFIXES: [(&str, i32); 11] = [
    ("f", -15),
    ("p", -12),
    ("n", -9),
    ("u", -6),
    ("µ", -6), // micro sign
    ("μ", -6), // Greek small mu
    ("m", -3),
    ("k", 3),
    ("M", 6),
    ("G", 9),
    ("T", 12),
];

fn lookup(symbol: &str) -> Option<&'static Base> {
    BASES
        .iter()
        .chain(EXTRA.iter())
        .find(|b| b.symbol == symbol)
}

pub fn parse_unit(text: &str) -> Result<Unit, QuantityError> {
    match text {
        "%" => {
            return Ok(Unit {
                exp10: -2,
                dim: Dimension::NONE,
                relative: true,
                offset: false,
            });
        }
        "ppm" => {
            return Ok(Unit {
                exp10: -6,
                dim: Dimension::NONE,
                relative: true,
                offset: false,
            });
        }
        _ => {}
    }
    // A whole-symbol match wins, so `m` is the metre and `Hz` is not `H` + `z`.
    if let Some(b) = lookup(text) {
        return Ok(Unit {
            exp10: b.exp10,
            dim: b.dim,
            relative: false,
            offset: b.symbol == "degC",
        });
    }
    for (prefix, exp) in PREFIXES {
        if let Some(rest) = text.strip_prefix(prefix)
            && let Some(b) = lookup(rest)
            && b.prefixable
        {
            return Ok(Unit {
                exp10: b.exp10 + exp,
                dim: b.dim,
                relative: false,
                offset: false,
            });
        }
    }
    Err(QuantityError::UnknownUnit(text.to_string()))
}

/// Whether `text` is an offset unit (`degC`): a sign written on a literal in it is the literal's.
pub fn is_offset_unit(text: &str) -> bool {
    parse_unit(text).is_ok_and(|u| u.offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(s: &str) -> (i32, Dimension) {
        let u = parse_unit(s).unwrap();
        (u.exp10, u.dim)
    }

    #[test]
    fn prefixes_scale_by_powers_of_ten() {
        assert_eq!(unit("V"), (0, Dimension::VOLTAGE));
        assert_eq!(unit("mV"), (-3, Dimension::VOLTAGE));
        assert_eq!(unit("mA"), (-3, Dimension::CURRENT));
        assert_eq!(unit("uF"), (-6, Dimension::CAPACITANCE));
        assert_eq!(unit("µF"), (-6, Dimension::CAPACITANCE));
        assert_eq!(unit("nF"), (-9, Dimension::CAPACITANCE));
        assert_eq!(unit("pF"), (-12, Dimension::CAPACITANCE));
        assert_eq!(unit("kHz"), (3, Dimension::FREQUENCY));
        assert_eq!(unit("MHz"), (6, Dimension::FREQUENCY));
    }

    #[test]
    fn resistance_spellings() {
        assert_eq!(unit("ohm"), (0, Dimension::RESISTANCE));
        assert_eq!(unit("Ω"), (0, Dimension::RESISTANCE));
        assert_eq!(unit("kohm"), (3, Dimension::RESISTANCE));
        assert_eq!(unit("kΩ"), (3, Dimension::RESISTANCE));
        assert_eq!(
            unit("mohm"),
            (-3, Dimension::RESISTANCE),
            "lowercase m is milli"
        );
        assert_eq!(
            unit("Mohm"),
            (6, Dimension::RESISTANCE),
            "uppercase M is mega"
        );
    }

    #[test]
    fn whole_symbols_beat_prefixes() {
        assert_eq!(unit("m"), (0, Dimension::LENGTH));
        assert_eq!(unit("mm"), (-3, Dimension::LENGTH));
        assert_eq!(unit("s"), (0, Dimension::TIME));
        assert_eq!(unit("ms"), (-3, Dimension::TIME));
        assert_eq!(unit("Hz"), (0, Dimension::FREQUENCY));
        assert_eq!(unit("H"), (0, Dimension::INDUCTANCE));
        assert_eq!(unit("mH"), (-3, Dimension::INDUCTANCE));
        assert_eq!(unit("S"), (0, Dimension::CONDUCTANCE));
    }

    #[test]
    fn grams_are_a_thousandth_of_the_base_unit() {
        assert_eq!(unit("g"), (-3, Dimension::MASS));
        assert_eq!(unit("kg"), (0, Dimension::MASS));
        assert_eq!(unit("mg"), (-6, Dimension::MASS));
    }

    #[test]
    fn percentages_are_relative_and_dimensionless() {
        let p = parse_unit("%").unwrap();
        assert!(p.relative && p.dim.is_dimensionless() && p.exp10 == -2);
        assert_eq!(parse_unit("ppm").unwrap().exp10, -6);
        assert!(!parse_unit("V").unwrap().relative);
    }

    #[test]
    fn degc_is_the_one_offset_unit_and_c_stays_the_coulomb() {
        let c = parse_unit("degC").unwrap();
        assert!(c.offset && c.dim == Dimension::TEMPERATURE && c.exp10 == 0);
        assert!(!parse_unit("K").unwrap().offset);
        assert_eq!(unit("C"), (0, Dimension::CHARGE));
        assert_eq!(unit("nC"), (-9, Dimension::CHARGE));
        assert!(!parse_unit("C").unwrap().offset);
    }

    #[test]
    fn rejects_what_it_does_not_know() {
        assert!(matches!(
            parse_unit("furlong"),
            Err(QuantityError::UnknownUnit(_))
        ));
        assert!(parse_unit("kK").is_err(), "kelvin takes no prefix here");
        assert!(parse_unit("mdegC").is_err(), "degC takes no prefix");
        assert!(parse_unit("degc").is_err(), "units are case sensitive");
        assert!(parse_unit("v").is_err(), "units are case sensitive");
        assert!(parse_unit("").is_err());
    }
}
