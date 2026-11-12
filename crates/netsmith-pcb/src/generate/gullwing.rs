//! Gull-wing small-outline packages: SOIC, SSOP, TSSOP and MSOP, two columns of leads.
//!
//! The lead dimensions are the JEDEC outlines (MS-012 and MS-013 for SOIC, MO-150 for
//! SSOP, MO-153 for TSSOP, MO-187 for MSOP) as the manufacturers' drawings and KiCad's
//! footprint scripts record them; the pads follow IPC-7351B at nominal density ([`ipc`]).

use super::ipc::{self, GULL_WING, GULL_WING_FINE, Inside, Size};
use super::outline::{Body, dual_pads, two_columns};
use super::params::Params;
use crate::footprint::Footprint;

/// One package.
#[derive(Clone, Copy)]
struct Package {
    name: &'static str,
    description: &'static str,
    pins: usize,
    pitch: f64,
    /// Across the columns, then along them.
    body: (f64, f64),
    /// Tip to tip of the leads of the two columns.
    overall: Size,
    lead_width: Size,
    lead_length: Size,
    /// Whether the fine-pitch fillets apply although the pitch is above 0.625 mm, where
    /// the standard asks for them: KiCad's MSOP-8 uses them, the plain ones would leave
    /// its pads 0.15 mm apart.
    fine: bool,
}

// A table row: every argument is a column.
#[allow(clippy::too_many_arguments)]
const fn pkg(
    name: &'static str,
    description: &'static str,
    pins: usize,
    pitch: f64,
    body: (f64, f64),
    overall: (f64, f64),
    lead_width: (f64, f64),
    lead_length: (f64, f64),
    fine: bool,
) -> Package {
    Package {
        name,
        description,
        pins,
        pitch,
        body,
        overall: Size::range(overall.0, overall.1),
        lead_width: Size::range(lead_width.0, lead_width.1),
        lead_length: Size::range(lead_length.0, lead_length.1),
        fine,
    }
}

/// JEDEC MS-012 (narrow body SOIC): 3.9 mm body, 6.0 mm +- 0.2 over the leads.
const MS012: ((f64, f64), (f64, f64), (f64, f64)) = ((5.8, 6.2), (0.31, 0.51), (0.4, 1.27));
/// JEDEC MS-013 (wide body SOIC): 7.5 mm body, 10.3 mm +- 0.33 over the leads.
const MS013: ((f64, f64), (f64, f64), (f64, f64)) = ((9.97, 10.63), (0.31, 0.51), (0.4, 1.27));
/// JEDEC MO-153 (TSSOP): 4.4 mm body, 6.4 mm over the leads.
const MO153: ((f64, f64), (f64, f64), (f64, f64)) = ((6.4, 6.4), (0.17, 0.30), (0.45, 0.75));
/// JEDEC MO-150 (SSOP): 5.3 mm body, 7.8 mm +- 0.4 over the leads.
const MO150: ((f64, f64), (f64, f64), (f64, f64)) = ((7.4, 8.2), (0.22, 0.38), (0.55, 0.95));

const SOIC: &str = "Small outline IC, gull-wing leads, 1.27 mm pitch";
const TSSOP: &str = "Thin shrink small outline package, 0.65 mm pitch";
const SSOP: &str = "Shrink small outline package, 0.65 mm pitch";

const PACKAGES: &[Package] = &[
    pkg(
        "SOIC-8_3.9x4.9mm_P1.27mm",
        SOIC,
        8,
        1.27,
        (3.9, 4.9),
        MS012.0,
        MS012.1,
        MS012.2,
        false,
    ),
    pkg(
        "SOIC-14_3.9x8.7mm_P1.27mm",
        SOIC,
        14,
        1.27,
        (3.9, 8.65),
        MS012.0,
        MS012.1,
        MS012.2,
        false,
    ),
    pkg(
        "SOIC-16_3.9x9.9mm_P1.27mm",
        SOIC,
        16,
        1.27,
        (3.9, 9.9),
        MS012.0,
        MS012.1,
        MS012.2,
        false,
    ),
    pkg(
        "SOIC-14W_7.5x9mm_P1.27mm",
        SOIC,
        14,
        1.27,
        (7.5, 9.0),
        MS013.0,
        MS013.1,
        MS013.2,
        false,
    ),
    pkg(
        "SOIC-16W_7.5x10.3mm_P1.27mm",
        SOIC,
        16,
        1.27,
        (7.5, 10.3),
        MS013.0,
        MS013.1,
        MS013.2,
        false,
    ),
    pkg(
        "SOIC-16W_7.5x12.8mm_P1.27mm",
        SOIC,
        16,
        1.27,
        (7.5, 12.8),
        MS013.0,
        MS013.1,
        MS013.2,
        false,
    ),
    pkg(
        "SOIC-18W_7.5x11.6mm_P1.27mm",
        SOIC,
        18,
        1.27,
        (7.5, 11.55),
        MS013.0,
        MS013.1,
        MS013.2,
        false,
    ),
    pkg(
        "SOIC-20W_7.5x12.8mm_P1.27mm",
        SOIC,
        20,
        1.27,
        (7.5, 12.8),
        MS013.0,
        MS013.1,
        MS013.2,
        false,
    ),
    pkg(
        "SOIC-24W_7.5x15.4mm_P1.27mm",
        SOIC,
        24,
        1.27,
        (7.5, 15.4),
        MS013.0,
        MS013.1,
        MS013.2,
        false,
    ),
    pkg(
        "SOIC-28W_7.5x17.9mm_P1.27mm",
        SOIC,
        28,
        1.27,
        (7.5, 17.9),
        MS013.0,
        MS013.1,
        MS013.2,
        false,
    ),
    pkg(
        "TSSOP-8_4.4x3mm_P0.65mm",
        TSSOP,
        8,
        0.65,
        (4.4, 3.0),
        MO153.0,
        MO153.1,
        MO153.2,
        false,
    ),
    pkg(
        "TSSOP-14_4.4x5mm_P0.65mm",
        TSSOP,
        14,
        0.65,
        (4.4, 5.0),
        MO153.0,
        MO153.1,
        MO153.2,
        false,
    ),
    pkg(
        "TSSOP-16_4.4x5mm_P0.65mm",
        TSSOP,
        16,
        0.65,
        (4.4, 5.0),
        MO153.0,
        MO153.1,
        MO153.2,
        false,
    ),
    pkg(
        "TSSOP-20_4.4x6.5mm_P0.65mm",
        TSSOP,
        20,
        0.65,
        (4.4, 6.5),
        MO153.0,
        MO153.1,
        MO153.2,
        false,
    ),
    pkg(
        "TSSOP-24_4.4x7.8mm_P0.65mm",
        TSSOP,
        24,
        0.65,
        (4.4, 7.8),
        MO153.0,
        MO153.1,
        MO153.2,
        false,
    ),
    pkg(
        "TSSOP-28_4.4x9.7mm_P0.65mm",
        TSSOP,
        28,
        0.65,
        (4.4, 9.7),
        MO153.0,
        MO153.1,
        MO153.2,
        false,
    ),
    pkg(
        "MSOP-8_3x3mm_P0.65mm",
        "Mini small outline package, 0.65 mm pitch",
        8,
        0.65,
        (3.0, 3.0),
        (4.65, 5.15),
        (0.22, 0.38),
        (0.4, 0.8),
        true,
    ),
    pkg(
        "MSOP-10_3x3mm_P0.5mm",
        "Mini small outline package, 0.5 mm pitch",
        10,
        0.5,
        (3.0, 3.0),
        (4.65, 5.15),
        (0.17, 0.33),
        (0.4, 0.8),
        false,
    ),
    pkg(
        "SSOP-16_5.3x6.2mm_P0.65mm",
        SSOP,
        16,
        0.65,
        (5.3, 6.2),
        (7.6, 7.9),
        (0.25, 0.38),
        (0.63, 1.03),
        false,
    ),
    pkg(
        "SSOP-20_5.3x7.2mm_P0.65mm",
        SSOP,
        20,
        0.65,
        (5.3, 7.2),
        MO150.0,
        MO150.1,
        MO150.2,
        false,
    ),
];

/// The TSSOP rows beyond the first few: (name, pins, pitch, body width, body length).
///
/// The span over the leads is 6.4 mm for the 4.4 mm body, 8.1 mm for the 6.1 mm body and
/// 10.0 mm for the 8.0 mm body; the leads are 0.45 to 0.75 mm long and, by pitch, 0.17 to
/// 0.30 mm (0.65 mm), 0.17 to 0.27 mm (0.5 mm) and 0.13 to 0.23 mm (0.4 mm) wide. Only part
/// of that is read from a drawing: TI's SN74LVC16245A datasheet (SCES062Q) gives 6.20 to
/// 6.60 mm over the leads of a 4.4 mm body with 0.13 to 0.23 mm leads at 0.4 mm pitch (page
/// 21, MPDS006C) and 7.90 to 8.30 mm over a 6.1 mm body with 0.17 to 0.27 mm leads at 0.5 mm
/// pitch (page 25, MTSS003D, "within JEDEC MO-153"). The other values (the 8.0 mm body's
/// 10.0 mm, the 0.65 mm pitch width, the 6.1 mm body at 0.65 and 0.4 mm pitch) were fitted
/// to reproduce KiCad 9.0.0's pads and are not read from a datasheet; JEDEC MO-153 itself
/// was not read (ADR 0043).
#[rustfmt::skip]
const TSSOP_ROWS: &[(&str, usize, f64, f64, f64)] = &[
    ("TSSOP-100_6.1x20.8mm_P0.4mm", 100, 0.4, 6.1, 20.8),
    ("TSSOP-14_4.4x3.6mm_P0.4mm", 14, 0.4, 4.4, 3.6),
    ("TSSOP-16_4.4x3.6mm_P0.4mm", 16, 0.4, 4.4, 3.6),
    ("TSSOP-20_4.4x5mm_P0.4mm", 20, 0.4, 4.4, 5.0),
    ("TSSOP-20_4.4x5mm_P0.5mm", 20, 0.5, 4.4, 5.0),
    ("TSSOP-24_4.4x5mm_P0.4mm", 24, 0.4, 4.4, 5.0),
    ("TSSOP-24_4.4x6.5mm_P0.5mm", 24, 0.5, 4.4, 6.5),
    ("TSSOP-24_6.1x7.8mm_P0.65mm", 24, 0.65, 6.1, 7.8),
    ("TSSOP-28_4.4x7.8mm_P0.5mm", 28, 0.5, 4.4, 7.8),
    ("TSSOP-28_6.1x7.8mm_P0.5mm", 28, 0.5, 6.1, 7.8),
    ("TSSOP-28_6.1x9.7mm_P0.65mm", 28, 0.65, 6.1, 9.7),
    ("TSSOP-28_8x9.7mm_P0.65mm", 28, 0.65, 8.0, 9.7),
    ("TSSOP-30_4.4x7.8mm_P0.5mm", 30, 0.5, 4.4, 7.8),
    ("TSSOP-30_6.1x9.7mm_P0.65mm", 30, 0.65, 6.1, 9.7),
    ("TSSOP-32_4.4x6.5mm_P0.4mm", 32, 0.4, 4.4, 6.5),
    ("TSSOP-32_6.1x11mm_P0.65mm", 32, 0.65, 6.1, 11.0),
    ("TSSOP-32_8x11mm_P0.65mm", 32, 0.65, 8.0, 11.0),
    ("TSSOP-36_4.4x7.8mm_P0.4mm", 36, 0.4, 4.4, 7.8),
    ("TSSOP-36_4.4x9.7mm_P0.5mm", 36, 0.5, 4.4, 9.7),
    ("TSSOP-36_6.1x12.5mm_P0.65mm", 36, 0.65, 6.1, 12.5),
    ("TSSOP-36_6.1x7.8mm_P0.4mm", 36, 0.4, 6.1, 7.8),
    ("TSSOP-36_6.1x9.7mm_P0.5mm", 36, 0.5, 6.1, 9.7),
    ("TSSOP-36_8x12.5mm_P0.65mm", 36, 0.65, 8.0, 12.5),
    ("TSSOP-36_8x9.7mm_P0.5mm", 36, 0.5, 8.0, 9.7),
    ("TSSOP-38_4.4x9.7mm_P0.5mm", 38, 0.5, 4.4, 9.7),
    ("TSSOP-38_6.1x12.5mm_P0.65mm", 38, 0.65, 6.1, 12.5),
    ("TSSOP-40_6.1x11mm_P0.5mm", 40, 0.5, 6.1, 11.0),
    ("TSSOP-40_6.1x14mm_P0.65mm", 40, 0.65, 6.1, 14.0),
    ("TSSOP-40_8x11mm_P0.5mm", 40, 0.5, 8.0, 11.0),
    ("TSSOP-40_8x14mm_P0.65mm", 40, 0.65, 8.0, 14.0),
    ("TSSOP-44_4.4x11mm_P0.5mm", 44, 0.5, 4.4, 11.0),
    ("TSSOP-44_6.1x11mm_P0.5mm", 44, 0.5, 6.1, 11.0),
    ("TSSOP-48_4.4x9.7mm_P0.4mm", 48, 0.4, 4.4, 9.7),
    ("TSSOP-48_6.1x12.5mm_P0.5mm", 48, 0.5, 6.1, 12.5),
    ("TSSOP-48_6.1x9.7mm_P0.4mm", 48, 0.4, 6.1, 9.7),
    ("TSSOP-48_8x12.5mm_P0.5mm", 48, 0.5, 8.0, 12.5),
    ("TSSOP-48_8x9.7mm_P0.4mm", 48, 0.4, 8.0, 9.7),
    ("TSSOP-50_4.4x12.5mm_P0.5mm", 50, 0.5, 4.4, 12.5),
    ("TSSOP-52_6.1x11mm_P0.4mm", 52, 0.4, 6.1, 11.0),
    ("TSSOP-52_8x11mm_P0.4mm", 52, 0.4, 8.0, 11.0),
    ("TSSOP-56_4.4x11.3mm_P0.4mm", 56, 0.4, 4.4, 11.3),
    ("TSSOP-56_6.1x12.5mm_P0.4mm", 56, 0.4, 6.1, 12.5),
    ("TSSOP-56_6.1x14mm_P0.5mm", 56, 0.5, 6.1, 14.0),
    ("TSSOP-56_8x12.5mm_P0.4mm", 56, 0.4, 8.0, 12.5),
    ("TSSOP-56_8x14mm_P0.5mm", 56, 0.5, 8.0, 14.0),
    ("TSSOP-60_8x12.5mm_P0.4mm", 60, 0.4, 8.0, 12.5),
    ("TSSOP-64_6.1x14mm_P0.4mm", 64, 0.4, 6.1, 14.0),
    ("TSSOP-64_6.1x17mm_P0.5mm", 64, 0.5, 6.1, 17.0),
    ("TSSOP-64_8x14mm_P0.4mm", 64, 0.4, 8.0, 14.0),
    ("TSSOP-68_8x14mm_P0.4mm", 68, 0.4, 8.0, 14.0),
    ("TSSOP-80_6.1x17mm_P0.4mm", 80, 0.4, 6.1, 17.0),
];

fn tssop_row(
    &(name, pins, pitch, body_width, body_length): &(&'static str, usize, f64, f64, f64),
) -> Package {
    let overall = if body_width < 5.0 {
        6.4
    } else if body_width < 7.0 {
        8.1
    } else {
        10.0
    };
    let (width, description) = if pitch > 0.6 {
        ((0.17, 0.30), TSSOP)
    } else if pitch > 0.45 {
        (
            (0.17, 0.27),
            "Thin shrink small outline package, 0.5 mm pitch",
        )
    } else {
        (
            (0.13, 0.23),
            "Thin shrink small outline package, 0.4 mm pitch",
        )
    };
    pkg(
        name,
        description,
        pins,
        pitch,
        (body_width, body_length),
        (overall, overall),
        width,
        MO153.2,
        false,
    )
}

/// Every package of this module.
fn packages() -> impl Iterator<Item = Package> {
    PACKAGES
        .iter()
        .copied()
        .chain(TSSOP_ROWS.iter().map(tssop_row))
}

/// Every `Library:Name` this module generates.
pub fn names() -> Vec<String> {
    packages()
        .map(|p| format!("Package_SO:{}", p.name))
        .collect()
}

/// The footprint named `library:name`, if it is a package of this module.
pub fn generate(library: &str, name: &str) -> Option<Footprint> {
    if library != "Package_SO" {
        return None;
    }
    let p = packages().find(|p| p.name == name)?;
    Some(gull_wing(&p, "Package_SO", p.name))
}

/// `gen:SO(...)`: two columns of gull-wing leads, as the SOIC, SSOP, TSSOP and MSOP rows
/// are made, with every dimension of a row a parameter. The fine-pitch fillets are used at
/// 0.625 mm and below.
pub(super) fn from_params(p: &Params, library: &str, name: &str) -> Result<Footprint, String> {
    let pins = p.count("pins", "the number of leads", 2, 100)?;
    if pins % 2 != 0 {
        return Err(format!(
            "`pins={pins}` must be even: a small outline package has the same number of leads in its two columns"
        ));
    }
    let package = Package {
        name: "",
        description: "Small outline package, gull-wing leads",
        pins,
        pitch: p.length("pitch", "the distance between lead centres", 0.3, 2.54)?,
        body: p.length_pair(
            "body",
            "the body across the columns and along them",
            1.0,
            40.0,
        )?,
        overall: p.size(
            "span",
            "the distance from tip to tip of the leads of the two columns",
            1.0,
            20.0,
        )?,
        lead_width: p.size("lead_width", "the width of a lead", 0.1, 1.0)?,
        lead_length: p.size("lead_length", "the length of the foot of a lead", 0.1, 2.0)?,
        fine: false,
    };
    Ok(gull_wing(&package, library, name))
}

fn gull_wing(p: &Package, library: &str, name: &str) -> Footprint {
    let class = if p.fine || p.pitch <= 0.625 {
        &GULL_WING_FINE
    } else {
        &GULL_WING
    };
    let land = ipc::land(
        class,
        p.overall,
        Inside::Length(p.lead_length),
        p.lead_width,
    );
    let pads = dual_pads(p.pins / 2, p.pitch, &land);
    let package = crate::body3d::Package::GullWing {
        body: p.body,
        span: (p.overall.nominal(), p.body.1),
        lead_width: p.lead_width.nominal(),
        lead_length: p.lead_length.nominal(),
        leads: super::leads_of(&pads, p.body),
        exposed: None,
    };
    let footprint = two_columns(
        library,
        name,
        &format!(
            "{}, {} pins, IPC-7351B nominal density, generated by netsmith",
            p.description, p.pins
        ),
        "SO",
        &pads,
        Body {
            width: p.body.0,
            length: p.body.1,
        },
        class.courtyard,
    );
    super::shaped(footprint, package)
}
