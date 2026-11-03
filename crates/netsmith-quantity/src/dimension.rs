//! Physical dimensions as exponents of the SI base units.

/// Exponents of metre, kilogram, second, ampere, kelvin, mole, candela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Dimension([i8; 7]);

const BASE_SYMBOLS: [&str; 7] = ["m", "kg", "s", "A", "K", "mol", "cd"];

impl Dimension {
    pub const NONE: Self = Self([0; 7]);
    pub const LENGTH: Self = Self([1, 0, 0, 0, 0, 0, 0]);
    pub const MASS: Self = Self([0, 1, 0, 0, 0, 0, 0]);
    pub const TIME: Self = Self([0, 0, 1, 0, 0, 0, 0]);
    pub const CURRENT: Self = Self([0, 0, 0, 1, 0, 0, 0]);
    pub const TEMPERATURE: Self = Self([0, 0, 0, 0, 1, 0, 0]);
    pub const FREQUENCY: Self = Self([0, 0, -1, 0, 0, 0, 0]);
    pub const CHARGE: Self = Self([0, 0, 1, 1, 0, 0, 0]);
    pub const POWER: Self = Self([2, 1, -3, 0, 0, 0, 0]);
    pub const VOLTAGE: Self = Self([2, 1, -3, -1, 0, 0, 0]);
    pub const RESISTANCE: Self = Self([2, 1, -3, -2, 0, 0, 0]);
    pub const CONDUCTANCE: Self = Self([-2, -1, 3, 2, 0, 0, 0]);
    pub const CAPACITANCE: Self = Self([-2, -1, 4, 2, 0, 0, 0]);
    pub const INDUCTANCE: Self = Self([2, 1, -2, -2, 0, 0, 0]);

    pub fn is_dimensionless(self) -> bool {
        self == Self::NONE
    }

    /// The symbol of the named derived unit, if there is one (`V` for voltage).
    pub fn unit_symbol(self) -> Option<&'static str> {
        const NAMED: [(Dimension, &str); 11] = [
            (Dimension::LENGTH, "m"),
            (Dimension::TIME, "s"),
            (Dimension::CURRENT, "A"),
            (Dimension::TEMPERATURE, "K"),
            (Dimension::FREQUENCY, "Hz"),
            (Dimension::CHARGE, "C"),
            (Dimension::POWER, "W"),
            (Dimension::VOLTAGE, "V"),
            (Dimension::RESISTANCE, "Ω"),
            (Dimension::CAPACITANCE, "F"),
            (Dimension::INDUCTANCE, "H"),
        ];
        NAMED.iter().find(|(d, _)| *d == self).map(|(_, s)| *s)
    }
}

// Multiplying quantities adds their dimension exponents (and dividing subtracts),
// which is what clippy's `suspicious_arithmetic_impl` cannot know.
#[allow(clippy::suspicious_arithmetic_impl)]
impl std::ops::Mul for Dimension {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        let mut out = self.0;
        for (o, b) in out.iter_mut().zip(other.0) {
            *o += b;
        }
        Self(out)
    }
}

#[allow(clippy::suspicious_arithmetic_impl)]
impl std::ops::Div for Dimension {
    type Output = Self;

    fn div(self, other: Self) -> Self {
        let mut out = self.0;
        for (o, b) in out.iter_mut().zip(other.0) {
            *o -= b;
        }
        Self(out)
    }
}

impl std::fmt::Display for Dimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_dimensionless() {
            return write!(f, "a plain number");
        }
        if let Some(name) = self.unit_symbol() {
            return write!(f, "{name}");
        }
        if *self == Self::CONDUCTANCE {
            return write!(f, "S");
        }
        let parts: Vec<String> = self
            .0
            .iter()
            .zip(BASE_SYMBOLS)
            .filter(|(e, _)| **e != 0)
            .map(|(e, s)| {
                if *e == 1 {
                    s.to_string()
                } else {
                    format!("{s}^{e}")
                }
            })
            .collect();
        write!(f, "{}", parts.join("·"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_dimensions_combine() {
        assert_eq!(
            Dimension::VOLTAGE / Dimension::CURRENT,
            Dimension::RESISTANCE
        );
        assert_eq!(Dimension::VOLTAGE * Dimension::CURRENT, Dimension::POWER);
        assert_eq!(
            Dimension::RESISTANCE * Dimension::CAPACITANCE,
            Dimension::TIME
        );
        assert_eq!(
            Dimension::RESISTANCE * Dimension::CONDUCTANCE,
            Dimension::NONE
        );
        assert_eq!(
            Dimension::INDUCTANCE / Dimension::RESISTANCE,
            Dimension::TIME
        );
        assert_eq!(Dimension::CHARGE / Dimension::TIME, Dimension::CURRENT);
    }

    #[test]
    fn names_for_display() {
        assert_eq!(Dimension::VOLTAGE.to_string(), "V");
        assert_eq!(Dimension::NONE.to_string(), "a plain number");
        assert_eq!((Dimension::LENGTH * Dimension::LENGTH).to_string(), "m^2");
    }
}
