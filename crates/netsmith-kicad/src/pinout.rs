//! What a part says about its pins, for the KiCad side: the symbol check and the viewer's
//! fallback box. The facts are the lowered part's own ([`PartPin`]); this wraps them with
//! the lookups the two need.

use netsmith_core::{Part, PartPin};

/// The pins of a part in the package it was given.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pinout {
    pub pins: Vec<PartPin>,
}

impl Pinout {
    /// The pinout of a lowered part, or `None` for a part that has no pin table.
    pub fn of(part: &Part) -> Option<Pinout> {
        (!part.pins.is_empty()).then(|| Pinout {
            pins: part.pins.clone(),
        })
    }

    /// The pin on `pad`.
    pub fn pin_on(&self, pad: &str) -> Option<&PartPin> {
        self.pins.iter().find(|p| p.pads.iter().any(|q| q == pad))
    }

    /// Every pad, in the pinout's order.
    pub fn pads(&self) -> impl Iterator<Item = &str> {
        self.pins
            .iter()
            .flat_map(|p| p.pads.iter().map(String::as_str))
    }
}
