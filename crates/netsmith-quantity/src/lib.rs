//! Dimensioned quantities and worst-case interval arithmetic for netsmith.

mod dimension;
mod display;
mod error;
mod quantity;
mod unit;

pub use dimension::Dimension;
pub use error::QuantityError;
pub use quantity::{Literal, Quantity};
pub use unit::{CELSIUS_ZERO_K, Unit, is_offset_unit, parse_unit};

// throwaway: exercises the narrow CI path (#618), not to merge
