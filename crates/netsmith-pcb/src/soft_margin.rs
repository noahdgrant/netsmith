//! A soft margin that a placed board does not keep (issue 1310): [`Board::soft_margins_broken`].
//!
//! `soft margin 3mm round .u1` in a physical file asks other parts to stay 3 mm from the
//! courtyard of `u1`. The placer pays for a courtyard inside the margin and keeps out of it where
//! it can; what it could not keep is listed here, for `fab` and `layout check` to say as a
//! warning (an error when strict).

use std::fmt;

use serde::Serialize;

use crate::board::{Board, member};

/// A part whose courtyard is nearer a part with a soft margin than the margin asks.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SoftMarginBroken {
    /// The part the margin is round.
    pub refdes: String,
    pub path: String,
    /// The part inside the margin.
    pub other: String,
    pub other_path: String,
    /// From courtyard to courtyard, and the margin, in mm.
    pub gap: f64,
    pub margin: f64,
    /// The line of the physical file that states the margin; empty when it is not known.
    pub source: String,
}

impl fmt::Display for SoftMarginBroken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is {} mm from {}, courtyard to courtyard, and the soft margin round {} asks for {} mm",
            member(&self.other_path, &self.other),
            crate::mm_text(self.gap),
            member(&self.path, &self.refdes),
            member(&self.path, &self.refdes),
            crate::mm_text(self.margin),
        )?;
        if !self.source.is_empty() {
            write!(f, " ({})", self.source)?;
        }
        write!(
            f,
            "; move {} away, or give the margin less",
            member(&self.other_path, &self.other)
        )
    }
}

impl Board {
    /// The pairs of parts that break a soft margin where the parts are now: a part with a soft
    /// margin and each other part whose courtyard is nearer than it. In the order of the parts
    /// that state a margin, then of the parts inside it. Reads the placement only.
    pub fn soft_margins_broken(&self) -> Vec<SoftMarginBroken> {
        let mut found = Vec::new();
        for (i, part) in self.parts.iter().enumerate() {
            if part.soft_margin <= 0.0 {
                continue;
            }
            for (j, other) in self.parts.iter().enumerate() {
                if i == j {
                    continue;
                }
                if let Some((gap, margin)) = part.soft_short(other) {
                    found.push(SoftMarginBroken {
                        refdes: part.refdes.clone(),
                        path: part.path.clone(),
                        other: other.refdes.clone(),
                        other_path: other.path.clone(),
                        gap,
                        margin,
                        source: part.soft_margin_source.clone(),
                    });
                }
            }
        }
        found
    }
}
