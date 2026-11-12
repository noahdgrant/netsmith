//! Diodes in the small molded packages SOD-123, SOD-323 and DO-214 (SMA, SMB, SMC).
//!
//! These land patterns are not computed from lead tolerances: the packages are molded bodies
//! with flat terminals under them, and manufacturers publish a recommended pattern for each.
//! The pad sizes and spacing below are those patterns as KiCad's library records them
//! (ADR 0043); the courtyard is 0.25 mm (nominal density) round the larger of the pads and
//! the body. Pad 1 is the cathode.
//!
//! **The shape drawn in 3D** (`Package::Molded`): the SOD-123 and SOD-323 have two gull-wing
//! terminals and a cathode band, from the sourced dimensions of [`SOD_123`] and [`SOD_323`]
//! below. The SMA, SMB and SMC have no terminals in the data yet and are drawn as the body and
//! its band.

use super::draw::{Draw, Drawing, PadSpec};
use crate::body3d::Terminals;
use crate::footprint::{Footprint, Mount};
use crate::geometry::Pt;

struct Package {
    name: &'static str,
    description: &'static str,
    /// Distance of each pad's centre from the middle.
    centre: f64,
    /// Length (along the part), then width of a pad. KiCad's file has the SMC pad turned a
    /// quarter, with the sides the other way round.
    pad: (f64, f64),
    /// Length, then width of the body.
    body: (f64, f64),
    /// The terminals drawn in 3D, where a source gives them.
    terminals: Option<Terminals>,
}

/// SOD-123's terminals, for the shape drawn in 3D, in mm. The nominals are the **Diodes
/// Incorporated "SOD123 Package Information" drawing (Rev. 2017-03-16, page 1, Package Outline
/// Dimensions, whose table prints a typical value for each)**, checked against **onsemi's SOD-123
/// 2-lead case 425, issue H, 29 February 2024 (document 98ASB42927B)**. The symbols are the
/// Diodes table's, with onsemi's in brackets where they differ. JEDEC's own drawing was not read.
///
/// | Symbol | Meaning | Diodes (range) | onsemi (range) |
/// |---|---|---|---|
/// | E (E) | body length, along the terminals | 2.65 (2.55 to 2.85) | 2.69 (2.54 to 2.84) |
/// | D (D) | body width | 1.55 (1.40 to 1.70) | 1.60 (1.40 to 1.80) |
/// | He (H) | span over the terminals, tip to tip | 3.65 (3.55 to 3.85) | 3.68 (3.56 to 3.86) |
/// | b (b) | width of a terminal | 0.57 (0.52 to 0.62) | 0.61 (0.51 to 0.71) |
/// | L (L) | the terminal's foot on the board | 0.30 (0.25 to 0.40) | 0.25 (reference) |
///
/// The span, width and foot are Diodes' typical values, all inside onsemi's ranges. The body is
/// the footprint's own, 2.8 x 1.8 mm: its length is inside both makers' range, its width is
/// onsemi's maximum and 0.10 mm over Diodes', and a terminal's reach beyond it, (3.65 - 2.8) / 2 =
/// 0.425 mm, is derived and not read. The bend's thickness and height are the drawing-only
/// constants of `body3d.rs`, and not the makers' c (0.10 to 0.15 mm).
const SOD_123: Terminals = Terminals {
    span: 3.65,
    width: 0.57,
    foot: 0.30,
};

/// SOD-323's terminals, for the shape drawn in 3D, in mm. The nominals are the **Diodes
/// Incorporated "SOD323 Package Information" drawing (Rev. 2017-03-16, page 1, Package Outline
/// Dimensions, whose table prints a typical value for each)**, checked against **onsemi's SOD-323
/// 1.70x1.25x0.85 case 477, issue K, 11 March 2024 (document 98ASB17533C)**. The symbols are the
/// Diodes table's, with onsemi's in brackets where they differ. JEDEC's own drawing was not read.
///
/// | Symbol | Meaning | Diodes (range) | onsemi (range) |
/// |---|---|---|---|
/// | E (D) | body length, along the terminals | 1.70 (1.60 to 1.80) | 1.70 (1.60 to 1.80) |
/// | D (E) | body width | 1.30 (1.20 to 1.40) | 1.25 (1.15 to 1.35) |
/// | He (H) | span over the terminals, tip to tip | 2.50 (2.30 to 2.70) | 2.50 (2.30 to 2.70) |
/// | b (b) | width of a terminal | 0.30 (0.25 to 0.35) | 0.32 (0.25 to 0.40) |
/// | L (L1) | the terminal's foot on the board | 0.30 (0.20 to 0.40) | 0.40 (reference) |
///
/// The span, width and foot are Diodes' typical values, all inside onsemi's ranges. The body is
/// the footprint's own, 1.8 x 1.4 mm: its length is the top of both makers' range, its width the
/// top of Diodes' and 0.05 mm over onsemi's, and a terminal's reach beyond it, (2.5 - 1.8) / 2 =
/// 0.35 mm, is derived and not read. The bend's thickness and height are the drawing-only
/// constants of `body3d.rs`, and not the makers' c (0.09 to 0.18 mm).
const SOD_323: Terminals = Terminals {
    span: 2.50,
    width: 0.30,
    foot: 0.30,
};

const PACKAGES: [Package; 5] = [
    Package {
        name: "D_SOD-123",
        description: "Diode, SOD-123",
        centre: 1.65,
        pad: (0.9, 1.2),
        body: (2.8, 1.8),
        terminals: Some(SOD_123),
    },
    Package {
        name: "D_SOD-323",
        description: "Diode, SOD-323",
        centre: 1.05,
        pad: (0.6, 0.45),
        body: (1.8, 1.4),
        terminals: Some(SOD_323),
    },
    Package {
        name: "D_SMA",
        description: "Diode, SMA (DO-214AC)",
        centre: 2.0,
        pad: (2.5, 1.8),
        body: (4.6, 3.0),
        terminals: None,
    },
    Package {
        name: "D_SMB",
        description: "Diode, SMB (DO-214AA)",
        centre: 2.15,
        pad: (2.5, 2.3),
        body: (4.6, 4.0),
        terminals: None,
    },
    Package {
        name: "D_SMC",
        description: "Diode, SMC (DO-214AB)",
        centre: 3.4,
        pad: (2.5, 3.3),
        body: (7.1, 6.2),
        terminals: None,
    },
];

const LIBRARY: &str = "Diode_SMD";
const COURTYARD: f64 = 0.25;

/// Every `Library:Name` this module generates.
pub fn names() -> Vec<String> {
    PACKAGES
        .iter()
        .map(|p| format!("{LIBRARY}:{}", p.name))
        .collect()
}

/// The footprint named `library:name`, if it is a package of this module.
pub fn generate(library: &str, name: &str) -> Option<Footprint> {
    if library != LIBRARY {
        return None;
    }
    PACKAGES.iter().find(|p| p.name == name).map(diode)
}

fn diode(p: &Package) -> Footprint {
    let mut drawing = Drawing::new(LIBRARY, p.name, Mount::Smd);
    drawing.describe(
        &format!(
            "{}, land pattern as the manufacturers recommend, generated by netsmith",
            p.description
        ),
        "diode",
    );
    for (number, side) in [(1, -1.0), (2, 1.0)] {
        drawing.pad(&PadSpec::smd(number, Pt::new(side * p.centre, 0.0), p.pad));
    }
    let pad_outer = p.centre + p.pad.0 / 2.0;
    let (hx, hy) = (p.body.0 / 2.0, p.body.1 / 2.0);
    let court = (
        pad_outer.max(hx) + COURTYARD,
        (p.pad.1 / 2.0).max(hy) + COURTYARD,
    );
    drawing.courtyard(Pt::new(-court.0, -court.1), Pt::new(court.0, court.1));

    // The body with the cathode corner cut.
    let cut = (hy * 0.4).min(0.5);
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
    drawing.fab_reference((p.body.1 * 0.4).clamp(0.4, 1.0));

    // A bracket round the cathode pad, clear of the copper: the part is polarised.
    let x = pad_outer + super::outline::SILK_TO_COPPER;
    let y = hy.max(p.pad.1 / 2.0 + super::outline::SILK_TO_COPPER) + 0.15;
    drawing.line(Draw::Silk, Pt::new(-x, -y), Pt::new(-x, y));
    drawing.line(Draw::Silk, Pt::new(-x, y), Pt::new(p.centre, y));
    drawing.line(Draw::Silk, Pt::new(p.centre, -y), Pt::new(-x, -y));

    let reference_y = -(court.1 + 0.7);
    super::shaped(
        drawing.finish(reference_y, -reference_y),
        crate::body3d::Package::Molded {
            body: p.body,
            terminals: p.terminals,
        },
    )
}
