//! The land pattern maths of IPC-7351B, at nominal density.
//!
//! The standard is a paid document, so this file works from the formulas as they are
//! published in manufacturers' application notes and in the open generator scripts of the
//! KiCad library project (kicad-footprint-generator, GPL-3.0-or-later; ADR 0043 records
//! what was read and that no code was copied). The fillet goals are the numbers of
//! IPC-7351B tables 3-2 to 3-15, median ("nominal", Level B) column, which those sources
//! tabulate.
//!
//! A lead is described by three toleranced sizes:
//!
//! - `L`, the outside span of the leads (for a chip: its length; for a gull wing: tip to tip),
//! - `T`, the length of one lead (for a chip: the terminal) or, where a datasheet gives
//!   it instead, `S`, the span between the inner lead edges,
//! - `W`, the lead width.
//!
//! and the pad follows from the largest outside span `Z`, the smallest inner gap `G` and
//! the largest width `X`:
//!
//! ```text
//! Zmax = Lmin + 2 Jt + sqrt(CL^2 + F^2 + P^2)
//! Gmin = Smax - 2 Jh - sqrt(CS^2 + F^2 + P^2)
//! Xmax = Wmin + 2 Js + sqrt(CW^2 + F^2 + P^2)
//! ```
//!
//! where `C` is the tolerance range of the size, `F` the fabrication tolerance of the
//! board (0.1 mm), `P` the placement tolerance (0.05 mm), and `Jt`, `Jh`, `Js` the toe,
//! heel and side fillet goals. When `S` is not given, `Snom = Lnom - 2 Tnom` and
//! `CS = sqrt(CL^2 + 2 CT^2)` (the root sum of squares, as the standard has it), and
//! `Smax = Snom + CS / 2`. Each of `Z`, `G`, `X` is then rounded to a grid.

/// The fabrication tolerance `F` of the board the land is for, in mm.
pub(crate) const FABRICATION_TOLERANCE: f64 = 0.1;
/// The placement tolerance `P` of the pick-and-place machine, in mm.
pub(crate) const PLACEMENT_TOLERANCE: f64 = 0.05;

/// A size with its limits, in mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Size {
    pub min: f64,
    pub max: f64,
}

impl Size {
    pub(crate) const fn range(min: f64, max: f64) -> Size {
        Size { min, max }
    }

    pub(crate) fn nominal(self) -> f64 {
        (self.min + self.max) / 2.0
    }

    /// The tolerance range `C`.
    pub(crate) fn spread(self) -> f64 {
        self.max - self.min
    }
}

/// Fillet goals and roundings of one class of lead (a row of an IPC-7351B table).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Class {
    pub toe: f64,
    pub heel: f64,
    pub side: f64,
    /// The excess of the courtyard over the pads.
    pub courtyard: f64,
    /// The grid `Z`, `G` and `X` are rounded to.
    pub round: f64,
}

/// Rectangular chips below 0603 (1608 metric): table 3-2.
pub(crate) const CHIP_SMALL: Class = Class {
    toe: 0.2,
    heel: 0.0,
    side: 0.0,
    courtyard: 0.15,
    round: 0.02,
};

/// Rectangular chips from 0603 up: table 3-2.
pub(crate) const CHIP: Class = Class {
    toe: 0.35,
    heel: 0.0,
    side: 0.0,
    courtyard: 0.25,
    round: 0.05,
};

/// Gull-wing leads at a pitch above 0.625 mm: table 3-3.
pub(crate) const GULL_WING: Class = Class {
    toe: 0.35,
    heel: 0.35,
    side: 0.03,
    courtyard: 0.25,
    round: 0.05,
};

/// Gull-wing leads at a pitch of 0.625 mm or less: table 3-3.
pub(crate) const GULL_WING_FINE: Class = Class {
    toe: 0.35,
    heel: 0.35,
    side: -0.02,
    courtyard: 0.25,
    round: 0.05,
};

/// Flat leads that end at the package edge (QFN, DFN, SON): table 3-15.
pub(crate) const FLAT_NO_LEAD: Class = Class {
    toe: 0.3,
    heel: 0.0,
    side: -0.04,
    courtyard: 0.25,
    round: 0.05,
};

/// How a lead is described besides its outside span and width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Inside {
    /// The length of one lead.
    Length(Size),
    /// The span between the inner edges of the leads.
    Span(Size),
}

/// The pad that one pair of opposite leads asks for, in mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Land {
    /// Edge to edge across the pair, outer edges (`Z`).
    pub outer: f64,
    /// Between the inner edges (`G`).
    pub inner: f64,
    /// Across the lead (`X`).
    pub width: f64,
}

impl Land {
    /// The length of one pad, in the direction of the lead.
    pub(crate) fn length(&self) -> f64 {
        (self.outer - self.inner) / 2.0
    }

    /// Distance of the pad's centre from the middle of the pair.
    pub(crate) fn centre(&self) -> f64 {
        (self.outer + self.inner) / 4.0
    }
}

/// Round to a multiple of `base`, halves to even (as the scripts that make KiCad's own
/// footprints do, so equal inputs give equal pads).
pub(crate) fn round_to(value: f64, base: f64) -> f64 {
    (value / base).round_ties_even() * base
}

/// The land of a pair of leads.
pub(crate) fn land(class: &Class, outside: Size, inside: Inside, width: Size) -> Land {
    land_with_heel_reduction(class, outside, inside, width, 0.0)
}

/// The land of a pair of leads, with the heel fillet goal cut by `reduction` on each lead
/// (a part whose exposed pad needs more room).
pub(crate) fn land_with_heel_reduction(
    class: &Class,
    outside: Size,
    inside: Inside,
    width: Size,
    reduction: f64,
) -> Land {
    let noise = FABRICATION_TOLERANCE.powi(2) + PLACEMENT_TOLERANCE.powi(2);
    let (s_nominal, s_spread) = match inside {
        Inside::Span(s) => (s.nominal(), s.spread()),
        Inside::Length(t) => (
            outside.nominal() - 2.0 * t.nominal(),
            (outside.spread().powi(2) + 2.0 * t.spread().powi(2)).sqrt(),
        ),
    };
    let s_max = s_nominal + s_spread / 2.0;
    let z = outside.min + 2.0 * class.toe + (outside.spread().powi(2) + noise).sqrt();
    let g = s_max - 2.0 * class.heel + 2.0 * reduction - (s_spread.powi(2) + noise).sqrt();
    let x = width.min + 2.0 * class.side + (width.spread().powi(2) + noise).sqrt();
    let up = |v: f64| ((v / class.round) + 0.5 + 1e-9).floor() * class.round;
    Land {
        outer: up(z),
        inner: up(g),
        width: up(x),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_0603_chip_resistor_gets_the_pad_of_the_published_dimensions() {
        // Body 1.5 to 1.7 mm long, 0.7 to 0.95 wide, terminals 0.15 to 0.4: the numbers of
        // IPC-SM-782 for a 0603 resistor.
        let land = land(
            &CHIP,
            Size::range(1.5, 1.7),
            Inside::Length(Size::range(0.15, 0.4)),
            Size::range(0.7, 0.95),
        );
        assert!((land.outer - 2.45).abs() < 1e-9, "{land:?}");
        assert!((land.inner - 0.85).abs() < 1e-9, "{land:?}");
        assert!((land.width - 0.95).abs() < 1e-9, "{land:?}");
        assert!((land.length() - 0.8).abs() < 1e-9);
        assert!((land.centre() - 0.825).abs() < 1e-9);
    }

    #[test]
    fn a_given_inner_span_is_used_as_it_is() {
        let land = land(
            &CHIP,
            Size::range(1.5, 1.7),
            Inside::Span(Size::range(0.7, 1.11)),
            Size::range(0.7, 0.95),
        );
        assert!((land.inner - 0.7).abs() < 1e-9, "{land:?}");
    }

    #[test]
    fn rounding_goes_to_the_grid_with_halves_to_even() {
        assert!((round_to(1.55, 0.02) - 1.56).abs() < 1e-9);
        assert!((round_to(0.974, 0.05) - 0.95).abs() < 1e-9);
    }
}
