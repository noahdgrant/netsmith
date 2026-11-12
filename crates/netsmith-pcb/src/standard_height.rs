//! The _Standard height_ of a package (ADR 0106): the greatest body height that the package's
//! standard allows, for a part that states no height and whose catalog row has none.
//!
//! A part's height comes from four places, the strongest first (the glossary's _Height
//! provenance_): the design states `.height`, else the `height` cell of the catalog row it was
//! picked as (ADR 0094), else the standard height of its footprint from the table below, else
//! none. A standard height is never written into the design's heights: [`crate::board::Placed`]
//! keeps the part's own height in `height`, and [`crate::board::Placed::standard_height`] gives
//! this table's figure only where that is `None`. So a stated or a catalog height always wins,
//! and every reader can tell the two apart.
//!
//! **The figure is the standard's maximum.** A height is used as an upper bound against a limit
//! (ADR 0094), so the figure is the largest the standard lets a part in that package stand: the
//! `A` dimension (seated height) of the JEDEC outline. A part in the package is often lower. No
//! nominal figure is kept: none of the drawings read prints one for `A`, only the maximum.
//!
//! **A key is a whole generated footprint name**, never a pattern: a name enters the table only
//! when a maker's drawing of that exact outline (pin count, body and pitch) was read and cites the
//! JEDEC registration. A footprint read from a file, a `gen:` footprint and every generated name
//! not listed have no standard height.
//!
//! **Sources** (read 2026-11-12). JEDEC's outline documents need a login, so each figure is from
//! a Texas Instruments package drawing that names the JEDEC registration in its notes ("Reference
//! JEDEC registration MS-012, variation AA") and prints the height as `<n> MAX` and in its title
//! (`SOIC - 1.75 mm max height`). The drawings are pages of these datasheets, downloaded from
//! `https://www.ti.com/lit/ds/symlink/<name>.pdf` and read as PDF text:
//!
//! | datasheet | literature number, revision | SHA-256 of the PDF read | drawings (page) |
//! |---|---|---|---|
//! | `lm358` | SLOS068AB, October 2024 | `8277dff7b89138c63b3582d37f378065a1dea3cae5ed1e2926ddc2779eb7e3a2` | D0008A (63), PW0008A (51), DGK0008A (56) |
//! | `tlv9062` | SBOS839N, July 2026 | `a77c6dbdedda34c1ecf26e4a5abd38a98903054b49fff9cd24d005897eabedaa` | D0014A (43), PW0014A (79), DGS0010A (50), DBV0006A (72) |
//! | `sn74hc595` | SCLS041J, October 2021 | `5a6a5aeecc79a2d4bf4e15aae681e0f99cb13b54d6524be253a6844c898eeb0d` | DW0016A (28), PW0016A (34), DB0016A (24) |
//! | `sn74hc245` | SCLS131F, August 2022 | `31514d936e53661d1c380603e5034b2958085359313986775b838d1f30fb8d3f` | DW0020A (32), PW0020A (22), DB0020A (25) |
//! | `tl431` | SLVS543S, May 2024 | `ed2e90697de4e2c553195514adf1ee59cda42863716b6ef702b8304a5b921d22` | DBZ0003A (62) |
//! | `tps7a02` | SBVS277C, September 2022 | `ed213c153f1db84762558fcac2ff9752e322151a1d44672cf554988b860cebc8` | DBV0005A (45) |
//! | `lm1117` | SNOS412Q, January 2023 | `c117f5dd97fd15e62c0dee3adbf1af60480a128a10a0f9a9c28aafb8220ec3aa` | DCY0004A (44) |
//! | `tms320f28027` | SPRS523P, February 2021 | `b73b8c499c36b0e2f0a216c86df6058129d7c596eb73b868fa45f83f11dbc16a` | PT0048A (144) |
//!
//! D0008A is drawn in inches and prints `.069 MAX [1.75]`; D0014A, in millimetres, prints
//! `1.75 MAX` for the same registration, MS-012, and the table keeps the registration's 1.75 mm.
//!
//! **Left out, so unknown:** chip resistors and capacitors (IEC 60115-8 and the EIA size codes
//! fix length and width; the thickness is each maker's, 0.45 mm to 0.55 mm for a resistor and
//! up to the length for a capacitor of high value, and the IEC tables are not free to read); QFN,
//! DFN and SON (MO-220 and MO-229 come in several profiles, V, W, U and X, from 1.00 mm down,
//! and the generated names do not say which); TQFP, VQFP, the other LQFPs, SC-70, SOT-89, DPAK,
//! D2PAK, the SMD diodes and crystals, BGA, headers (no drawing of each was read). A row is added
//! with its drawing, as these were.

/// A package standard's height for one footprint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StandardHeight {
    /// The standard, as messages name it: `JEDEC MS-012`.
    pub standard: &'static str,
    /// The greatest height above the board the standard allows, in mm.
    pub max: f64,
    /// The maker's drawing the figure was read from: `TI D0008A`.
    pub drawing: &'static str,
}

impl StandardHeight {
    /// `JEDEC MS-012, drawing TI D0008A`: the standard and the drawing, for a report.
    pub fn cited(&self) -> String {
        format!("{}, drawing {}", self.standard, self.drawing)
    }
}

const fn row(
    footprint: &'static str,
    standard: &'static str,
    max: f64,
    drawing: &'static str,
) -> (&'static str, StandardHeight) {
    (
        footprint,
        StandardHeight {
            standard,
            max,
            drawing,
        },
    )
}

/// Every footprint with a standard height, by its whole `Library:Name`.
pub const TABLE: &[(&str, StandardHeight)] = &[
    row(
        "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
        "JEDEC MS-012",
        1.75,
        "TI D0008A",
    ),
    row(
        "Package_SO:SOIC-14_3.9x8.7mm_P1.27mm",
        "JEDEC MS-012",
        1.75,
        "TI D0014A",
    ),
    row(
        "Package_SO:SOIC-16W_7.5x10.3mm_P1.27mm",
        "JEDEC MS-013",
        2.65,
        "TI DW0016A",
    ),
    row(
        "Package_SO:SOIC-20W_7.5x12.8mm_P1.27mm",
        "JEDEC MS-013",
        2.65,
        "TI DW0020A",
    ),
    row(
        "Package_SO:TSSOP-8_4.4x3mm_P0.65mm",
        "JEDEC MO-153",
        1.2,
        "TI PW0008A",
    ),
    row(
        "Package_SO:TSSOP-14_4.4x5mm_P0.65mm",
        "JEDEC MO-153",
        1.2,
        "TI PW0014A",
    ),
    row(
        "Package_SO:TSSOP-16_4.4x5mm_P0.65mm",
        "JEDEC MO-153",
        1.2,
        "TI PW0016A",
    ),
    row(
        "Package_SO:TSSOP-20_4.4x6.5mm_P0.65mm",
        "JEDEC MO-153",
        1.2,
        "TI PW0020A",
    ),
    row(
        "Package_SO:MSOP-8_3x3mm_P0.65mm",
        "JEDEC MO-187",
        1.1,
        "TI DGK0008A",
    ),
    row(
        "Package_SO:MSOP-10_3x3mm_P0.5mm",
        "JEDEC MO-187",
        1.1,
        "TI DGS0010A",
    ),
    row(
        "Package_SO:SSOP-16_5.3x6.2mm_P0.65mm",
        "JEDEC MO-150",
        2.0,
        "TI DB0016A",
    ),
    row(
        "Package_SO:SSOP-20_5.3x7.2mm_P0.65mm",
        "JEDEC MO-150",
        2.0,
        "TI DB0020A",
    ),
    row(
        "Package_TO_SOT_SMD:SOT-23",
        "JEDEC TO-236",
        1.12,
        "TI DBZ0003A",
    ),
    row(
        "Package_TO_SOT_SMD:SOT-23-3",
        "JEDEC TO-236",
        1.12,
        "TI DBZ0003A",
    ),
    row(
        "Package_TO_SOT_SMD:SOT-23-5",
        "JEDEC MO-178",
        1.45,
        "TI DBV0005A",
    ),
    row(
        "Package_TO_SOT_SMD:SOT-23-6",
        "JEDEC MO-178",
        1.45,
        "TI DBV0006A",
    ),
    row(
        "Package_TO_SOT_SMD:SOT-223",
        "JEDEC TO-261",
        1.8,
        "TI DCY0004A",
    ),
    row(
        "Package_TO_SOT_SMD:SOT-223-3_TabPin2",
        "JEDEC TO-261",
        1.8,
        "TI DCY0004A",
    ),
    row(
        "Package_QFP:LQFP-48_7x7mm_P0.5mm",
        "JEDEC MS-026",
        1.6,
        "TI PT0048A",
    ),
];

/// The standard height of the footprint `library_id` (`Package_SO:SOIC-8_3.9x4.9mm_P1.27mm`),
/// if the table has it.
pub fn of(library_id: &str) -> Option<&'static StandardHeight> {
    TABLE
        .iter()
        .find(|(name, _)| *name == library_id)
        .map(|(_, h)| h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_is_a_generated_footprint_listed_once() {
        let names = crate::generate::names();
        for (i, (name, h)) in TABLE.iter().enumerate() {
            assert!(names.iter().any(|n| n == name), "{name} is not generated");
            assert!(
                TABLE[..i].iter().all(|(n, _)| n != name),
                "{name} is listed twice"
            );
            assert!(h.max > 0.0 && h.max < 5.0, "{name}: {}", h.max);
            assert!(h.standard.starts_with("JEDEC "), "{name}");
        }
    }

    #[test]
    fn a_name_is_matched_whole_and_a_chip_or_qfn_has_none() {
        assert_eq!(
            of("Package_SO:SOIC-8_3.9x4.9mm_P1.27mm").map(|h| (h.standard, h.max)),
            Some(("JEDEC MS-012", 1.75))
        );
        assert_eq!(of("Package_SO:SOIC-8_3.9x4.9mm"), None);
        assert_eq!(of("Resistor_SMD:R_0603_1608Metric"), None);
        assert_eq!(of("Capacitor_SMD:C_0402_1005Metric"), None);
        assert_eq!(of("Package_SO:SOIC-16_3.9x9.9mm_P1.27mm"), None);
        assert!(
            crate::generate::names()
                .iter()
                .filter(|n| n.starts_with("Package_DFN_QFN:"))
                .all(|n| of(n).is_none())
        );
    }

    #[test]
    fn the_citation_names_the_standard_and_the_drawing() {
        let h = of("Package_TO_SOT_SMD:SOT-23-5").unwrap();
        assert_eq!(h.cited(), "JEDEC MO-178, drawing TI DBV0005A");
    }
}
