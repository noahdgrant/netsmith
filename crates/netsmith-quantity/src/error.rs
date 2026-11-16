use crate::dimension::Dimension;

#[derive(Debug, Clone, PartialEq)]
pub enum QuantityError {
    UnknownUnit(String),
    DimensionMismatch {
        op: &'static str,
        left: Dimension,
        right: Dimension,
    },
    /// The divisor's range includes zero, so the quotient is unbounded.
    DivisorContainsZero,
    /// A range whose low end is above its high end.
    EmptyRange,
    NotFinite,
    /// A range with an open end (`1mm..`, `..5mm`) where a number with two ends is needed:
    /// a tolerance, or an end that is itself open.
    OpenEnd {
        op: &'static str,
    },
    /// `x +/- t` where `t` is neither a percentage nor the same dimension as `x`.
    BadTolerance {
        center: Dimension,
        tolerance: Dimension,
    },
}

impl std::fmt::Display for QuantityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownUnit(u) => write!(f, "unknown unit `{u}`"),
            Self::DimensionMismatch { op, left, right } => {
                write!(f, "cannot {op} {left} and {right}")
            }
            Self::DivisorContainsZero => write!(f, "the divisor's range includes zero"),
            Self::EmptyRange => write!(f, "the low end of the range is above the high end"),
            Self::NotFinite => write!(f, "the value is not a finite number"),
            Self::OpenEnd { op } => {
                write!(f, "cannot {op} a range with an open end (`lo..` or `..hi`)")
            }
            Self::BadTolerance { center, tolerance } => write!(
                f,
                "a tolerance on {center} must be a percentage or in the same unit, not {tolerance}"
            ),
        }
    }
}

impl std::error::Error for QuantityError {}
