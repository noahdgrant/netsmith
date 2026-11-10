//! A footprint asked for by generator name and parameters (`gen:QFN(pins=48, ...)`, ADR 0104).
//!
//! The parameters of a generator are the numbers its tables hold for each package, named: the
//! pads of `gen:QFN(...)` are those of the table row with the same numbers, because the same
//! code makes them (a test compares them with [`super::generate`] for several ids of each
//! generator). A generator reads its parameters through [`Params`], which turns a missing,
//! unknown, unreadable or out-of-range one into a message that names it and says what it is.
//! [`check_pads`] then refuses a result whose pads overlap or are not the size of a pad.
//!
//! Lengths are in millimetres and carry the unit: `pitch=0.5mm`. A size has limits, `0.2-0.3mm`,
//! or one number for a size with no tolerance, `0.25mm`. A pair is `7x7mm`, across and then
//! along, the unit once at the end; a count pair is `3x3`. The numbers are as `netsmith_kicad`'s
//! `FootprintSpec` spells them.

use std::collections::BTreeMap;

use netsmith_kicad::footprint_spec::FootprintSpec;

use super::ipc::Size;
use crate::footprint::{Footprint, PadKind};

/// The parameters of one `gen:` footprint, for the generator that reads them.
pub(super) struct Params<'a> {
    generator: &'a str,
    values: &'a BTreeMap<String, String>,
}

/// A length in mm: `0.5mm` is 0.5. The unit is required.
fn mm(text: &str) -> Option<&str> {
    text.strip_suffix("mm")
}

fn number(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|v| v.is_finite())
}

impl<'a> Params<'a> {
    /// The parameters of `spec`, which must be among `known`: one that is not is named, with
    /// the ones the generator has.
    pub(super) fn new(spec: &'a FootprintSpec, known: &[&str]) -> Result<Params<'a>, String> {
        if let Some(key) = spec.params.keys().find(|k| !known.contains(&k.as_str())) {
            return Err(format!(
                "`{key}` is not a parameter of `{}`; its parameters are {}",
                spec.generator,
                known
                    .iter()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        Ok(Params {
            generator: &spec.generator,
            values: &spec.params,
        })
    }

    fn raw(&self, key: &str) -> Option<&'a str> {
        self.values.get(key).map(String::as_str)
    }

    fn missing(&self, key: &str, what: &str) -> String {
        format!(
            "`{key}` is missing: {what} (`gen:{}(..., {key}=...)`)",
            self.generator
        )
    }

    fn unreadable(key: &str, value: &str, expected: &str) -> String {
        format!("`{key}={value}` is not {expected}")
    }

    fn out_of_range(key: &str, value: &str, what: &str, lo: f64, hi: f64, unit: &str) -> String {
        format!("`{key}={value}` is out of range: {what} is from {lo}{unit} to {hi}{unit}")
    }

    /// A whole number from `lo` to `hi`.
    fn one_count(key: &str, text: &str, what: &str, lo: usize, hi: usize) -> Result<usize, String> {
        let n: usize = text
            .parse()
            .map_err(|_| Self::unreadable(key, text, "a whole number"))?;
        if (lo..=hi).contains(&n) {
            Ok(n)
        } else {
            Err(Self::out_of_range(
                key, text, what, lo as f64, hi as f64, "",
            ))
        }
    }

    /// A length of `lo` to `hi` mm, with its unit.
    fn one_length(key: &str, text: &str, what: &str, lo: f64, hi: f64) -> Result<f64, String> {
        let v = mm(text)
            .and_then(number)
            .ok_or_else(|| Self::unreadable(key, text, "a length in mm, written `0.5mm`"))?;
        if v >= lo && v <= hi {
            Ok(v)
        } else {
            Err(Self::out_of_range(key, text, what, lo, hi, "mm"))
        }
    }

    /// `a` or `a-b`, each of `lo` to `hi`, without the unit.
    fn one_size(
        key: &str,
        whole: &str,
        text: &str,
        what: &str,
        lo: f64,
        hi: f64,
    ) -> Result<Size, String> {
        let bad = || Self::unreadable(key, whole, "a size in mm, `0.25mm` or `0.2-0.3mm`");
        let (min, max) = match text.split_once('-') {
            Some((a, b)) => (number(a).ok_or_else(bad)?, number(b).ok_or_else(bad)?),
            None => {
                let v = number(text).ok_or_else(bad)?;
                (v, v)
            }
        };
        if min > max {
            return Err(format!(
                "`{key}={whole}` has its least size {min}mm above its greatest {max}mm"
            ));
        }
        if min < lo || max > hi {
            return Err(Self::out_of_range(key, whole, what, lo, hi, "mm"));
        }
        Ok(Size::range(min, max))
    }

    /// A whole number, required.
    pub(super) fn count(
        &self,
        key: &str,
        what: &str,
        lo: usize,
        hi: usize,
    ) -> Result<usize, String> {
        let text = self.raw(key).ok_or_else(|| self.missing(key, what))?;
        Self::one_count(key, text, what, lo, hi)
    }

    /// A length in mm, required.
    pub(super) fn length(&self, key: &str, what: &str, lo: f64, hi: f64) -> Result<f64, String> {
        let text = self.raw(key).ok_or_else(|| self.missing(key, what))?;
        Self::one_length(key, text, what, lo, hi)
    }

    /// A length in mm, or `None` when the key is not given.
    pub(super) fn opt_length(
        &self,
        key: &str,
        what: &str,
        lo: f64,
        hi: f64,
    ) -> Result<Option<f64>, String> {
        self.raw(key)
            .map(|t| Self::one_length(key, t, what, lo, hi))
            .transpose()
    }

    /// A size in mm: one number or `min-max`, required.
    pub(super) fn size(&self, key: &str, what: &str, lo: f64, hi: f64) -> Result<Size, String> {
        self.opt_size(key, what, lo, hi)?
            .ok_or_else(|| self.missing(key, what))
    }

    pub(super) fn opt_size(
        &self,
        key: &str,
        what: &str,
        lo: f64,
        hi: f64,
    ) -> Result<Option<Size>, String> {
        let Some(text) = self.raw(key) else {
            return Ok(None);
        };
        let inner = mm(text)
            .ok_or_else(|| Self::unreadable(key, text, "a size in mm, `0.25mm` or `0.2-0.3mm`"))?;
        Self::one_size(key, text, inner, what, lo, hi).map(Some)
    }

    /// `AxB` in mm, required: the two lengths, across and then along.
    pub(super) fn length_pair(
        &self,
        key: &str,
        what: &str,
        lo: f64,
        hi: f64,
    ) -> Result<(f64, f64), String> {
        let text = self.raw(key).ok_or_else(|| self.missing(key, what))?;
        let bad = || Self::unreadable(key, text, "two lengths in mm, written `7x7mm`");
        let inner = mm(text).ok_or_else(bad)?;
        let (a, b) = inner.split_once('x').ok_or_else(bad)?;
        let (a, b) = (number(a).ok_or_else(bad)?, number(b).ok_or_else(bad)?);
        if [a, b].iter().all(|v| *v >= lo && *v <= hi) {
            Ok((a, b))
        } else {
            Err(Self::out_of_range(key, text, what, lo, hi, "mm"))
        }
    }

    /// `AxB` in mm, or `None` when the key is not given.
    pub(super) fn opt_length_pair(
        &self,
        key: &str,
        what: &str,
        lo: f64,
        hi: f64,
    ) -> Result<Option<(f64, f64)>, String> {
        if self.raw(key).is_none() {
            return Ok(None);
        }
        self.length_pair(key, what, lo, hi).map(Some)
    }

    /// Whether the key is given.
    pub(super) fn has(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// `AxB` in mm where each is a size, required: `7x7mm` or `4.9-5.1x4.9-5.1mm`.
    pub(super) fn size_pair(
        &self,
        key: &str,
        what: &str,
        lo: f64,
        hi: f64,
    ) -> Result<(Size, Size), String> {
        let text = self.raw(key).ok_or_else(|| self.missing(key, what))?;
        let bad = || {
            Self::unreadable(
                key,
                text,
                "two sizes in mm, written `7x7mm` or `6.9-7.1x6.9-7.1mm`",
            )
        };
        let inner = mm(text).ok_or_else(bad)?;
        let (a, b) = inner.split_once('x').ok_or_else(bad)?;
        Ok((
            Self::one_size(key, text, a, what, lo, hi)?,
            Self::one_size(key, text, b, what, lo, hi)?,
        ))
    }

    /// `AxB` as two whole numbers, or `None` when the key is not given.
    pub(super) fn opt_count_pair(
        &self,
        key: &str,
        what: &str,
        lo: usize,
        hi: usize,
    ) -> Result<Option<(usize, usize)>, String> {
        let Some(text) = self.raw(key) else {
            return Ok(None);
        };
        let (a, b) = text
            .split_once('x')
            .ok_or_else(|| Self::unreadable(key, text, "two whole numbers, written `3x3`"))?;
        Ok(Some((
            Self::one_count(key, a, what, lo, hi)?,
            Self::one_count(key, b, what, lo, hi)?,
        )))
    }

    /// `AxB` as two whole numbers, required.
    pub(super) fn count_pair(
        &self,
        key: &str,
        what: &str,
        lo: usize,
        hi: usize,
    ) -> Result<(usize, usize), String> {
        self.opt_count_pair(key, what, lo, hi)?
            .ok_or_else(|| self.missing(key, what))
    }
}

/// Refuse a footprint whose pads cannot be a land pattern: a pad with no size, or two pads of
/// different numbers that overlap (a lead width that is wider than the pitch leaves, a body
/// that is too small for the leads). A wrong pad is a wrong board.
pub(super) fn check_pads(footprint: &Footprint) -> Result<(), String> {
    let copper: Vec<_> = footprint
        .pads
        .iter()
        .filter(|p| !p.number.is_empty() && p.kind != PadKind::NpThruHole)
        .collect();
    for pad in &copper {
        if pad.width <= 0.0 || pad.height <= 0.0 {
            return Err(format!(
                "pad {} comes out with no size ({} x {} mm); the dimensions do not make a land pattern",
                pad.number, pad.width, pad.height
            ));
        }
    }
    for (i, a) in copper.iter().enumerate() {
        for b in &copper[i + 1..] {
            if a.number == b.number {
                continue;
            }
            let overlap = |p: f64, q: f64, wp: f64, wq: f64| (p - q).abs() < (wp + wq) / 2.0 - 1e-9;
            if overlap(a.at.x, b.at.x, a.width, b.width)
                && overlap(a.at.y, b.at.y, a.height, b.height)
            {
                return Err(format!(
                    "pads {} and {} overlap (they are {} x {} mm and {} x {} mm); the pitch, \
                     the lead width and the body do not fit together",
                    a.number, b.number, a.width, a.height, b.width, b.height
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{generate, generate_spec};
    use super::*;

    fn made(spec: &str) -> Footprint {
        let spec = FootprintSpec::parse(spec).unwrap();
        generate_spec(&spec).unwrap_or_else(|e| panic!("{}: {e}", spec.spelling()))
    }

    fn refused(spec: &str) -> String {
        let spec = FootprintSpec::parse(spec).unwrap();
        generate_spec(&spec).expect_err(&spec.spelling())
    }

    /// The `gen:` footprint has the pads, the courtyard and the body of the id's footprint
    /// (the name and the library are the only differences): the same numbers by name, made by
    /// the same code.
    fn agrees(spec: &str, id: &str) {
        let by_name = made(spec);
        let by_id = generate(id).unwrap_or_else(|| panic!("{id} is not generated"));
        assert!(!by_id.pads.is_empty());
        assert_eq!(by_name.pads, by_id.pads, "{spec} against {id}");
        assert_eq!(by_name.courtyard, by_id.courtyard, "{spec} against {id}");
        assert_eq!(by_name.body, by_id.body, "{spec} against {id}");
        assert_eq!(by_name.mount, by_id.mount, "{spec} against {id}");
        // The name is the id's, and the id reads back to the same footprint.
        let spec = FootprintSpec::parse(spec).unwrap();
        assert_eq!(by_name.name, spec.id().strip_prefix("gen:").unwrap());
        assert_eq!(generate(&spec.id()).unwrap(), by_name);
    }

    #[test]
    fn a_qfn_has_the_pads_of_the_row_with_the_same_numbers() {
        agrees(
            "gen:QFN(pins=28, pitch=0.5mm, body=5x5mm, ep=3.35x3.35mm, lead_width=0.18-0.3mm, lead_length=0.35-0.45mm, paste=3x3)",
            "Package_DFN_QFN:QFN-28-1EP_5x5mm_P0.5mm_EP3.35x3.35mm",
        );
        agrees(
            "gen:QFN(pins=16, pitch=0.65mm, body=4x4mm, ep=2.5x2.5mm, lead_width=0.25-0.35mm, lead_length=0.3-0.5mm, paste=2x2)",
            "Package_DFN_QFN:QFN-16-1EP_4x4mm_P0.65mm_EP2.5x2.5mm",
        );
        // A body with its tolerance, and a heel fillet taken off.
        agrees(
            "gen:QFN(pins=16, pitch=0.5mm, body=2.9-3.1x2.9-3.1mm, ep=1.7x1.7mm, lead_width=0.18-0.3mm, lead_length=0.3-0.5mm, paste=2x2, heel=0.05mm)",
            "Package_DFN_QFN:QFN-16-1EP_3x3mm_P0.5mm_EP1.7x1.7mm",
        );
        // What a QFN of the table has: 28 leads, numbered round from the left, and the exposed pad.
        let fp = made(
            "gen:QFN(pins=28, pitch=0.5mm, body=5x5mm, ep=3.35x3.35mm, lead_width=0.18-0.3mm, lead_length=0.35-0.45mm)",
        );
        let numbered: Vec<_> = fp
            .pad_numbers()
            .into_iter()
            .filter(|n| !n.is_empty())
            .collect();
        assert_eq!(numbered.len(), 29);
        assert_eq!(numbered.last(), Some(&"29"));
    }

    #[test]
    fn a_qfn_with_no_paste_count_gets_openings_no_wider_than_one_and_a_half_millimetres() {
        let fp = made(
            "gen:QFN(pins=48, pitch=0.5mm, body=7x7mm, ep=5.2x5.2mm, lead_width=0.2-0.3mm, lead_length=0.35-0.45mm)",
        );
        let paste = fp.pads.iter().filter(|p| p.number.is_empty()).count();
        // 5.2 mm over 1.5 mm is four openings a side.
        assert_eq!(paste, 16);
    }

    #[test]
    fn a_dfn_has_the_pads_of_the_row_with_the_same_numbers() {
        agrees(
            "gen:DFN(pins=10, pitch=0.5mm, body=3x3mm, ep=1.58x2.35mm, lead_width=0.18-0.3mm, lead_length=0.3-0.5mm, paste=2x2)",
            "Package_DFN_QFN:DFN-10-1EP_3x3mm_P0.5mm_EP1.58x2.35mm",
        );
        // Leads on two edges only: ten pads and the exposed pad.
        let fp = made(
            "gen:DFN(pins=10, pitch=0.5mm, body=3x3mm, ep=1.58x2.35mm, lead_width=0.18-0.3mm, lead_length=0.3-0.5mm)",
        );
        let numbered: Vec<_> = fp
            .pad_numbers()
            .into_iter()
            .filter(|n| !n.is_empty())
            .collect();
        assert_eq!(numbered.len(), 11);
    }

    #[test]
    fn a_qfp_has_the_pads_of_the_row_with_the_same_numbers() {
        agrees(
            "gen:QFP(pins=48, body=7mm, pitch=0.5mm)",
            "Package_QFP:TQFP-48_7x7mm_P0.5mm",
        );
        agrees(
            "gen:QFP(pins=48, body=7mm, pitch=0.5mm, ep=5x5mm, paste=4x4)",
            "Package_QFP:TQFP-48-1EP_7x7mm_P0.5mm_EP5x5mm",
        );
        // The lead width of a pitch can be given, and the 0.65 mm pitch's second width is one.
        agrees(
            "gen:QFP(pins=44, body=10mm, pitch=0.8mm, lead_width=0.3-0.45mm)",
            "Package_QFP:LQFP-44_10x10mm_P0.8mm",
        );
        agrees(
            "gen:QFP(pins=80, body=14mm, pitch=0.65mm, lead_width=0.22-0.33mm)",
            "Package_QFP:VQFP-80_14x14mm_P0.65mm",
        );
        assert!(refused("gen:QFP(pins=64, body=10mm, pitch=0.7mm)").contains("`lead_width`"));
    }

    #[test]
    fn a_small_outline_package_has_the_pads_of_the_row_with_the_same_numbers() {
        agrees(
            "gen:SO(pins=8, pitch=1.27mm, body=3.9x4.9mm, span=5.8-6.2mm, lead_width=0.31-0.51mm, lead_length=0.4-1.27mm)",
            "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
        );
        agrees(
            "gen:SO(pins=16, pitch=1.27mm, body=7.5x10.3mm, span=9.97-10.63mm, lead_width=0.31-0.51mm, lead_length=0.4-1.27mm)",
            "Package_SO:SOIC-16W_7.5x10.3mm_P1.27mm",
        );
        // Fine pitch: the fillets of the smaller pitches follow from the pitch.
        agrees(
            "gen:SO(pins=20, pitch=0.5mm, body=4.4x5mm, span=6.4mm, lead_width=0.17-0.27mm, lead_length=0.45-0.75mm)",
            "Package_SO:TSSOP-20_4.4x5mm_P0.5mm",
        );
    }

    #[test]
    fn a_sot_23_has_the_pads_of_the_row_with_the_same_numbers() {
        agrees("gen:SOT23(pins=5)", "Package_TO_SOT_SMD:SOT-23-5");
        agrees("gen:SOT23(pins=6)", "Package_TO_SOT_SMD:SOT-23-6");
        agrees("gen:SOT23(pins=3)", "Package_TO_SOT_SMD:SOT-23-3");
        // The TO-236 outline: the same layout, 2.1 to 2.64 mm over the leads, a narrower body.
        agrees(
            "gen:SOT23(pins=3, span=2.1-2.64mm, body=1.3x2.9mm)",
            "Package_TO_SOT_SMD:SOT-23",
        );
    }

    #[test]
    fn a_header_and_a_socket_have_the_pads_of_the_named_ones() {
        agrees(
            "gen:PinHeader(rows=1, pins=4, pitch=2.54mm)",
            "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Vertical",
        );
        agrees(
            "gen:PinHeader(rows=2, pins=20, pitch=2.54mm)",
            "Connector_PinHeader_2.54mm:PinHeader_2x20_P2.54mm_Vertical",
        );
        agrees(
            "gen:PinSocket(rows=2, pins=5, pitch=2.0mm)",
            "Connector_PinSocket_2.00mm:PinSocket_2x05_P2.00mm_Vertical",
        );
        let header = made("gen:PinHeader(rows=1, pins=4, pitch=2.54mm)");
        let socket = made("gen:PinSocket(rows=1, pins=4, pitch=2.54mm)");
        assert_ne!(header.name, socket.name);
    }

    #[test]
    fn a_bga_has_the_pads_of_the_row_with_the_same_numbers() {
        agrees(
            "gen:BGA(layout=11x11, pitch=0.8mm, body=10x10mm, pad=0.32mm)",
            "Package_BGA:XBGA-121_10x10mm_Layout11x11_P0.8mm",
        );
        agrees(
            "gen:BGA(layout=10x10, pitch=1mm, body=11x11mm, pad=0.4mm)",
            "Package_BGA:BGA-100_11.0x11.0mm_Layout10x10_P1.0mm_Ball0.5mm_Pad0.4mm_NSMD",
        );
    }

    #[test]
    fn the_name_is_the_same_for_the_same_numbers_and_different_for_others() {
        let a = made("gen:SOT23(pins=5)");
        let b = made("gen:SOT23( pins=05 )");
        let c = made("gen:SOT23(pins=5, span=2.9mm)");
        assert_eq!(a.name, b.name);
        assert_ne!(a.name, c.name);
        assert_eq!(a.name, "SOT23_pins5");
        assert_eq!(c.name, "SOT23_pins5_span2.9mm");
    }

    #[test]
    fn a_generator_that_is_unknown_is_named_with_the_ones_there_are() {
        let e = refused("gen:QFNN(pins=16)");
        assert!(e.contains("no generator `QFNN`"), "{e}");
        for name in super::super::GENERATORS {
            assert!(e.contains(name), "{e}");
        }
    }

    #[test]
    fn a_parameter_that_is_missing_unknown_unreadable_or_out_of_range_is_named() {
        let full = "pins=16, pitch=0.5mm, body=4x4mm, ep=2.5x2.5mm, lead_width=0.2-0.3mm, lead_length=0.3-0.5mm";
        made(&format!("gen:QFN({full})"));
        let e = refused("gen:QFN(pins=16, pitch=0.5mm)");
        assert!(e.contains("`body` is missing"), "{e}");
        let e = refused(&format!("gen:QFN({full}, colour=3)"));
        assert!(e.contains("`colour` is not a parameter of `QFN`"), "{e}");
        assert!(e.contains("`lead_length`"), "{e}");
        let e = refused(&format!(
            "gen:QFN({})",
            full.replace("pitch=0.5mm", "pitch=0.5")
        ));
        assert!(e.contains("`pitch=0.5` is not a length in mm"), "{e}");
        let e = refused(&format!("gen:QFN({})", full.replace("pins=16", "pins=2")));
        assert!(e.contains("`pins=2` is out of range"), "{e}");
        let e = refused(&format!("gen:QFN({})", full.replace("pins=16", "pins=18")));
        assert!(e.contains("multiple of 4"), "{e}");
        let e = refused(&format!(
            "gen:QFN({})",
            full.replace("pitch=0.5mm", "pitch=9mm")
        ));
        assert!(e.contains("`pitch=9mm` is out of range"), "{e}");
        let e = refused(&format!(
            "gen:QFN({})",
            full.replace("lead_width=0.2-0.3mm", "lead_width=0.3-0.2mm")
        ));
        assert!(e.contains("least size"), "{e}");
        let e = refused("gen:PinHeader(rows=3, pins=4, pitch=2.54mm)");
        assert!(e.contains("`rows=3` is out of range"), "{e}");
        let e = refused("gen:PinHeader(rows=1, pins=4, pitch=1.27mm)");
        assert!(e.contains("2.54mm and 2mm only"), "{e}");
        let e = refused("gen:SOT23(pins=4)");
        assert!(e.contains("3, 5 or 6"), "{e}");
        let e = refused("gen:BGA(layout=4x4, pitch=0.8mm, body=1x1mm, pad=0.3mm)");
        assert!(e.contains("smaller than the 4x4 balls need"), "{e}");
    }

    #[test]
    fn pads_that_overlap_are_refused() {
        // 0.5 mm pitch with leads 0.6 mm wide.
        let e = refused(
            "gen:QFN(pins=16, pitch=0.5mm, body=4x4mm, ep=2.5x2.5mm, lead_width=0.6-0.7mm, lead_length=0.3-0.5mm)",
        );
        assert!(e.contains("overlap"), "{e}");
    }
}
