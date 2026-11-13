//! Two-terminal chips: resistors, capacitors, inductors, LEDs, diodes and fuses in the
//! sizes 0201 to 2512 (`R_0402_1005Metric` and its kin).
//!
//! The pads come from the IPC-7351B formulas ([`super::ipc`]) at nominal density; the body
//! and terminal dimensions are the envelopes of IPC-SM-782 and of the manufacturers'
//! datasheets as KiCad's footprint scripts record them (ADR 0043 gives the sources).

use super::draw::{Draw, Drawing, Kind, PadSpec, Shape};
use super::ipc::{self, CHIP, CHIP_SMALL, Inside, Size};
use crate::body3d::Package;
use crate::footprint::{Footprint, Mount};
use crate::geometry::Pt;

/// One size of chip: the body, and the terminals or the terminal spacing.
#[derive(Clone, Copy)]
struct Dimensions {
    /// `0402`
    imperial: &'static str,
    /// `1005`
    metric: &'static str,
    length: Size,
    width: Size,
    ends: Inside,
}

const fn t(min: f64, max: f64) -> Inside {
    Inside::Length(Size::range(min, max))
}

const fn s(min: f64, max: f64) -> Inside {
    Inside::Span(Size::range(min, max))
}

const fn dims(
    imperial: &'static str,
    metric: &'static str,
    length: (f64, f64),
    width: (f64, f64),
    ends: Inside,
) -> Dimensions {
    Dimensions {
        imperial,
        metric,
        length: Size::range(length.0, length.1),
        width: Size::range(width.0, width.1),
        ends,
    }
}

/// Chip resistors (IPC-SM-782, page 72).
const RESISTORS: [Dimensions; 8] = [
    dims("0201", "0603", (0.55, 0.65), (0.25, 0.35), t(0.1, 0.2)),
    dims("0402", "1005", (1.0, 1.1), (0.48, 0.6), t(0.1, 0.3)),
    dims("0603", "1608", (1.5, 1.7), (0.7, 0.95), t(0.15, 0.4)),
    dims("0805", "2012", (1.85, 2.15), (1.1, 1.4), t(0.15, 0.65)),
    dims("1206", "3216", (3.05, 3.35), (1.45, 1.75), t(0.25, 0.75)),
    dims("1210", "3225", (3.05, 3.35), (2.34, 2.64), t(0.25, 0.75)),
    dims("2010", "5025", (4.85, 5.15), (2.35, 2.65), t(0.35, 0.85)),
    dims("2512", "6332", (6.15, 6.45), (3.05, 3.35), t(0.35, 0.85)),
];

/// Chip capacitors (IPC-SM-782, page 76; the terminals of the 0805 are shorter than the
/// standard's 0.75 mm, to keep the pads from growing wide over a tight gap).
const CAPACITORS: [Dimensions; 6] = [
    dims("0201", "0603", (0.55, 0.65), (0.25, 0.35), t(0.1, 0.2)),
    dims("0402", "1005", (0.9, 1.1), (0.4, 0.6), t(0.1, 0.3)),
    dims("0603", "1608", (1.45, 1.75), (0.65, 0.95), t(0.2, 0.5)),
    dims("0805", "2012", (1.8, 2.2), (1.05, 1.45), t(0.25, 0.55)),
    dims("1206", "3216", (3.0, 3.4), (1.4, 1.8), t(0.25, 0.75)),
    dims("1210", "3225", (3.0, 3.4), (2.3, 2.7), t(0.25, 0.75)),
];

/// Inductors, ferrite beads, LEDs, diodes and fuses: the datasheets give the spacing of the
/// terminals instead of their length.
const GENERAL: [Dimensions; 8] = [
    dims("0201", "0603", (0.55, 0.65), (0.25, 0.35), t(0.1, 0.2)),
    dims("0402", "1005", (1.0, 1.1), (0.48, 0.6), s(0.4, 0.7)),
    dims("0603", "1608", (1.5, 1.7), (0.7, 0.95), s(0.7, 1.11)),
    dims("0805", "2012", (1.86, 2.15), (1.12, 1.38), s(0.89, 1.55)),
    dims("1206", "3216", (3.05, 3.35), (1.45, 1.75), s(1.55, 2.32)),
    dims("1210", "3225", (3.05, 3.35), (2.34, 2.64), s(1.55, 2.32)),
    dims("2010", "5025", (4.85, 5.15), (2.35, 2.65), s(3.15, 3.92)),
    dims("2512", "6332", (6.15, 6.45), (3.05, 3.35), s(4.45, 5.22)),
];

/// Inductors differ from the general table in two sizes, where IPC-SM-782 (page 80) has
/// bodies of its own.
const INDUCTORS: [Dimensions; 8] = [
    GENERAL[0],
    GENERAL[1],
    GENERAL[2],
    dims("0805", "2012", (1.7, 2.3), (0.6, 1.2), t(0.1, 0.3)),
    dims("1206", "3216", (2.9, 3.5), (1.3, 1.9), t(0.2, 0.5)),
    GENERAL[5],
    GENERAL[6],
    GENERAL[7],
];

/// What kind of part a chip footprint is for.
struct Family {
    library: &'static str,
    prefix: &'static str,
    describes: &'static str,
    tags: &'static str,
    table: &'static [Dimensions],
    /// The sizes KiCad's library has, which are the ones that are checked against it.
    sizes: &'static [&'static str],
    /// Whether pad 1 is the cathode, marked on the silkscreen.
    polarised: bool,
}

const ALL: [&str; 8] = [
    "0201", "0402", "0603", "0805", "1206", "1210", "2010", "2512",
];

const FAMILIES: [Family; 6] = [
    Family {
        library: "Resistor_SMD",
        prefix: "R",
        describes: "Resistor",
        tags: "resistor",
        table: &RESISTORS,
        sizes: &ALL,
        polarised: false,
    },
    Family {
        library: "Capacitor_SMD",
        prefix: "C",
        describes: "Capacitor",
        tags: "capacitor",
        table: &CAPACITORS,
        sizes: &["0201", "0402", "0603", "0805", "1206", "1210"],
        polarised: false,
    },
    Family {
        library: "Inductor_SMD",
        prefix: "L",
        describes: "Inductor",
        tags: "inductor",
        table: &INDUCTORS,
        sizes: &ALL,
        polarised: false,
    },
    Family {
        library: "LED_SMD",
        prefix: "LED",
        describes: "LED",
        tags: "LED",
        table: &GENERAL,
        sizes: &ALL,
        polarised: true,
    },
    Family {
        library: "Diode_SMD",
        prefix: "D",
        describes: "Diode",
        tags: "diode",
        table: &GENERAL,
        sizes: &ALL,
        polarised: true,
    },
    Family {
        library: "Fuse",
        prefix: "Fuse",
        describes: "Fuse",
        tags: "fuse",
        table: &GENERAL,
        sizes: &["0402", "0603", "0805", "1206", "1210", "2010", "2512"],
        polarised: false,
    },
];

/// Every `Library:Name` this module generates.
pub(super) fn names() -> Vec<String> {
    let mut out = Vec::new();
    for family in &FAMILIES {
        for size in family.sizes {
            if let Some(d) = family.table.iter().find(|d| d.imperial == *size) {
                out.push(format!(
                    "{}:{}_{}_{}Metric",
                    family.library, family.prefix, d.imperial, d.metric
                ));
            }
        }
    }
    out
}

/// The footprint named `library:name`, if it is a chip of this module.
pub(super) fn generate(library: &str, name: &str) -> Option<Footprint> {
    let family = FAMILIES.iter().find(|f| f.library == library)?;
    let rest = name.strip_prefix(family.prefix)?.strip_prefix('_')?;
    let dimensions = family
        .table
        .iter()
        .find(|d| format!("{}_{}Metric", d.imperial, d.metric) == rest)?;
    if !family.sizes.contains(&dimensions.imperial) {
        return None;
    }
    Some(chip(family, dimensions, name))
}

fn chip(family: &Family, d: &Dimensions, name: &str) -> Footprint {
    // Below 0603 the standard asks for smaller fillets.
    let small = d.imperial < "0603";
    let class = if small { &CHIP_SMALL } else { &CHIP };
    let land = ipc::land(class, d.length, d.ends, d.width);
    let (centre, length, width) = (land.centre(), land.length(), land.width);
    let body = (d.length.nominal(), d.width.nominal());

    let mut drawing = Drawing::new(family.library, name, Mount::Smd);
    drawing.describe(
        &format!(
            "{} SMD {} ({} Metric), square (rectangular) end terminal, IPC-7351B nominal density, generated by netsmith",
            family.describes, d.imperial, d.metric
        ),
        family.tags,
    );

    // The two copper pads, and for the smallest sizes the stencil openings, which are
    // smaller than the pads so that the paste does not float the part off its pad.
    for (number, side) in [(1, -1.0), (2, 1.0)] {
        let at = Pt::new(side * centre, 0.0);
        if small && d.imperial == "0201" {
            // Each end of the opening is pulled in by a tenth of the pad's length, the inner
            // end by another 0.05 mm, and the opening is a tenth narrower.
            let (inner, outer) = (centre - length / 2.0, centre + length / 2.0);
            let (inner, outer) = (inner + 0.1 * length + 0.05, outer - 0.1 * length);
            drawing.pad(&PadSpec::new(
                "",
                Kind::PasteOnly,
                Shape::RoundRect,
                Pt::new(side * (inner + outer) / 2.0, 0.0),
                (outer - inner, width * 0.9),
            ));
            drawing.pad(&PadSpec::new(
                number,
                Kind::SmdNoPaste,
                Shape::RoundRect,
                at,
                (length, width),
            ));
        } else {
            drawing.pad(&PadSpec::smd(number, at, (length, width)));
        }
    }

    let pad_outer = centre + length / 2.0;
    let court = (
        pad_outer.max(body.0 / 2.0) + class.courtyard,
        (width / 2.0).max(body.1 / 2.0) + class.courtyard,
    );
    drawing.courtyard(Pt::new(-court.0, -court.1), Pt::new(court.0, court.1));

    // The body on the fabrication layer; a polarised part has its cathode corner cut.
    let (hx, hy) = (body.0 / 2.0, body.1 / 2.0);
    if family.polarised {
        let cut = (body.1 * 0.375).min(0.3);
        drawing.polygon(
            Draw::Fab,
            &[
                Pt::new(-hx + cut, -hy),
                Pt::new(hx, -hy),
                Pt::new(hx, hy),
                Pt::new(-hx, hy),
                Pt::new(-hx, -hy + cut),
            ],
            false,
        );
    } else {
        drawing.rect(Draw::Fab, Pt::new(-hx, -hy), Pt::new(hx, hy));
    }
    drawing.fab_reference((body.1 * 0.5).clamp(0.25, 0.8));

    // Silkscreen: a pair of strokes between the pads, or for a polarised part a bracket
    // round the cathode pad, clear of the copper.
    let half_gap = centre - length / 2.0;
    if family.polarised {
        let (x, y) = (pad_outer + 0.26, width / 2.0 + 0.26);
        drawing.line(Draw::Silk, Pt::new(-x, -y), Pt::new(-x, y));
        drawing.line(Draw::Silk, Pt::new(-x, y), Pt::new(body.0 / 2.0, y));
        drawing.line(Draw::Silk, Pt::new(body.0 / 2.0, -y), Pt::new(-x, -y));
    } else if !small {
        let y = width / 2.0 + 0.1;
        let x = half_gap - 0.1;
        if x > 0.05 {
            drawing.line(Draw::Silk, Pt::new(-x, -y), Pt::new(x, -y));
            drawing.line(Draw::Silk, Pt::new(-x, y), Pt::new(x, y));
        }
    } else if d.imperial == "0402" {
        let y = width / 2.0 + 0.06;
        let x = half_gap - 0.1;
        drawing.line(Draw::Silk, Pt::new(-x, -y), Pt::new(x, -y));
        drawing.line(Draw::Silk, Pt::new(-x, y), Pt::new(x, y));
    }

    let terminal = match d.ends {
        Inside::Length(t) => t.nominal(),
        Inside::Span(s) => (d.length.nominal() - s.nominal()) / 2.0,
    };
    let reference_y = -(court.1 + 0.7);
    super::shaped(
        drawing.finish(reference_y, -reference_y),
        Package::Chip {
            body,
            terminal,
            polarised: family.polarised,
        },
    )
}
