//! The `table` of a `kicad.toml` row: the numbers a datasheet's package drawing gives, which the
//! row's footprint must agree with (issue #1281, decided by the owner on 2026-11-19).
//!
//! ```toml
//! [[part]]
//! kind = "Driver"
//! package = "QFN-48"
//! footprint = "own:Driver"
//! table = "pads=49, pitch=0.5mm, body=7x7mm"
//! ```
//!
//! The text is written as the parameters of a `gen:` footprint are (ADR 0104): `key=value`
//! items, a length with its unit, a size one length or the least and the greatest, a pair across
//! and then along. Three keys, each optional, at least one given:
//!
//! - `pads`: how many pads the footprint has, counted by number (several pads of one number, the
//!   pieces of an exposed pad, are one; an unnumbered pad, a paste opening, is none). A 48-pin
//!   QFN with an exposed pad states `pads=49`.
//! - `pitch`: the least distance between the centres of two pads of different numbers that are
//!   in one row or one column (the same y, or the same x). A SOT-23-3, whose two pads on one side
//!   are 1.9 mm apart, states `pitch=1.9mm`.
//! - `body`: the size of the body the footprint draws on its fabrication layer (`F.Fab`), across
//!   (x in the footprint's frame) and then along (y), `3.9x4.9mm`, or a size each, `6.9-7.1x6.9-7.1mm`.
//!
//! This module reads the text; `netsmith_pcb::validate_table` measures the footprint. A count is
//! matched exactly, a pitch to [`PITCH_TOLERANCE`] (0.635 mm and 0.65 mm are told apart), and a
//! body of one length to [`BODY_TOLERANCE`] each way.

use crate::FootprintSpec;

/// How far a measured pitch may be from the stated one, in mm. Small enough to tell 25 mil
/// (0.635 mm) from 0.65 mm, large enough for the rounding of a drawn footprint.
pub const PITCH_TOLERANCE: f64 = 0.005;

/// How far a measured body may be from a stated size of one length, in mm, each way. A drawing
/// on `F.Fab` is the nominal body to its line's centre; a stated range is matched as it stands.
pub const BODY_TOLERANCE: f64 = 0.05;

/// A size in mm: the least and the greatest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableSize {
    pub min: f64,
    pub max: f64,
}

// The numbers are finite (the reader refuses any other), so equality is an equivalence.
impl Eq for TableSize {}

impl TableSize {
    /// Whether `measured` is this size: inside the range, or within [`BODY_TOLERANCE`] of a size
    /// of one length.
    pub fn holds(&self, measured: f64) -> bool {
        let slack = if self.min == self.max {
            BODY_TOLERANCE
        } else {
            0.0
        };
        measured >= self.min - slack - 1e-9 && measured <= self.max + slack + 1e-9
    }
}

impl std::fmt::Display for TableSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.min == self.max {
            write!(f, "{}", self.min)
        } else {
            write!(f, "{}-{}", self.min, self.max)
        }
    }
}

/// The table of a row, read.
#[derive(Debug, Clone, PartialEq)]
pub struct FootprintTable {
    /// The table as the row wrote it, for messages.
    pub text: String,
    pub pads: Option<usize>,
    pub pitch: Option<f64>,
    /// Across, then along.
    pub body: Option<(TableSize, TableSize)>,
}

impl Eq for FootprintTable {}

const KEYS: &[&str] = &["pads", "pitch", "body"];

fn number(text: &str) -> Option<f64> {
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
}

fn size(text: &str) -> Option<TableSize> {
    let (min, max) = match text.split_once('-') {
        Some((a, b)) => (number(a)?, number(b)?),
        None => {
            let v = number(text)?;
            (v, v)
        }
    };
    (min <= max).then_some(TableSize { min, max })
}

impl FootprintTable {
    /// Read `pads=49, pitch=0.5mm, body=7x7mm`. The message says what is wrong with the text,
    /// not where it is (the caller names the row and the line).
    pub fn parse(text: &str) -> Result<FootprintTable, String> {
        let spec = FootprintSpec::parse(&format!("gen:Table({text})"))?;
        if let Some(key) = spec.params.keys().find(|k| !KEYS.contains(&k.as_str())) {
            return Err(format!(
                "`{key}` is not a column of a table; its columns are `pads`, `pitch` and `body`"
            ));
        }
        if spec.params.is_empty() {
            return Err(
                "the table states nothing; give `pads`, `pitch` or `body`, as in `pads=8, pitch=1.27mm`"
                    .to_string(),
            );
        }
        let get = |key: &str| spec.params.get(key).map(String::as_str);
        let pads = get("pads")
            .map(|v| {
                v.parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| format!("`pads={v}` is not a whole number above 0"))
            })
            .transpose()?;
        let pitch = get("pitch")
            .map(|v| {
                v.strip_suffix("mm")
                    .and_then(number)
                    .ok_or_else(|| format!("`pitch={v}` is not a length in mm, written `0.5mm`"))
            })
            .transpose()?;
        let body = get("body")
            .map(|v| {
                let bad = || {
                    format!(
                        "`body={v}` is not two sizes in mm, across and then along, written `7x7mm` or `6.9-7.1x6.9-7.1mm`"
                    )
                };
                let inner = v.strip_suffix("mm").ok_or_else(bad)?;
                let (a, b) = inner.split_once('x').ok_or_else(bad)?;
                Ok::<_, String>((size(a).ok_or_else(bad)?, size(b).ok_or_else(bad)?))
            })
            .transpose()?;
        Ok(FootprintTable {
            text: text.trim().to_string(),
            pads,
            pitch,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_reads_its_three_columns() {
        let t = FootprintTable::parse("pads=49, pitch=0.50mm, body=6.9-7.1x7mm").unwrap();
        assert_eq!(t.pads, Some(49));
        assert_eq!(t.pitch, Some(0.5));
        let (across, along) = t.body.unwrap();
        assert_eq!((across.min, across.max), (6.9, 7.1));
        assert_eq!((along.min, along.max), (7.0, 7.0));
        assert!(along.holds(7.04) && !along.holds(7.06));
        assert!(across.holds(7.1) && !across.holds(7.11));
        let only = FootprintTable::parse("pads=3").unwrap();
        assert_eq!((only.pitch, only.body), (None, None));
    }

    #[test]
    fn a_table_refuses_what_it_cannot_read() {
        for (text, says) in [
            ("", "states nothing"),
            ("pins=48", "`pins` is not a column of a table"),
            ("pads=0", "`pads=0` is not a whole number"),
            ("pitch=0.5", "`pitch=0.5` is not a length in mm"),
            ("body=7mm", "`body=7mm` is not two sizes"),
            ("body=7.1-6.9x7mm", "`body=7.1-6.9x7mm` is not two sizes"),
            ("pads=8, pads=9", "`pads` is given twice"),
        ] {
            let e = FootprintTable::parse(text).unwrap_err();
            assert!(e.contains(says), "{text:?}: {e}");
        }
    }
}
