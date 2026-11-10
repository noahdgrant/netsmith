//! SOT-23 with three, five or six pins, SOT-223, and the SC-70 family (SOT-323, SOT-343,
//! SOT-353, SOT-363, SC-70-8).
//!
//! The SOT-23 family follows IPC-7351B like the other gull-wing packages, from the JEDEC
//! outlines TO-236 (SOT-23) and MO-178 (SOT-23-3, -5, -6). SOT-223 has a tab pad whose size is
//! not a function of the lead dimensions; its pads are the ones the manufacturers' drawings
//! recommend, as KiCad's library gives them (ADR 0043).
//!
//! The SC-70 family (EIAJ SC-70, also SOT-323 and, with more leads, SOT-343, -353, -363 and
//! SC-70-8; a body about 1.25 x 2.0 mm, 0.65 mm pitch, 0.5 mm for the eight-lead one) is
//! recorded the same way: KiCad 9.0 draws its pads by hand, with one 1.35 x 2.2 mm body for
//! every row and a courtyard that is not a margin round the pads. The pads and the courtyard
//! are the numbers of those footprints (facts about them, as for SOT-223); a row for
//! reflow and one for hand soldering of each. The makers' drawings were not read.

use super::draw::PadSpec;
use super::ipc::{self, GULL_WING, Inside, Size};
use super::outline::{Body, two_columns, two_columns_in};
use super::params::Params;
use crate::footprint::Footprint;
use crate::geometry::Pt;

/// The pitch of the pins of every SOT-23 here, in mm.
const PITCH: f64 = 0.95;

/// A SOT-23 variant.
struct Sot23 {
    name: &'static str,
    description: &'static str,
    /// Tip to tip over the leads of the two columns.
    overall: Size,
    lead_length: Size,
    lead_width: Size,
    /// Across the columns, then along them.
    body: (f64, f64),
    /// The pins as (number, column, row), the column -1 (left) or 1, the row 0 to 2 from the
    /// top.
    pins: &'static [(u8, i8, u8)],
}

const SOT_23: [Sot23; 4] = [
    Sot23 {
        name: "SOT-23",
        description: "SOT, 3 Pin (JEDEC TO-236 Var AB)",
        overall: Size::range(2.10, 2.64),
        lead_length: Size::range(0.3, 0.6),
        lead_width: SOT_LEAD_WIDTH,
        body: (1.3, 2.9),
        pins: &[(1, -1, 0), (2, -1, 2), (3, 1, 1)],
    },
    Sot23 {
        name: "SOT-23-3",
        description: "SOT, 3 Pin (JEDEC MO-178 inferred 3-pin variant)",
        overall: Size::range(2.8, 2.8),
        lead_length: Size::range(0.3, 0.6),
        lead_width: SOT_LEAD_WIDTH,
        body: (1.6, 2.9),
        pins: &[(1, -1, 0), (2, -1, 2), (3, 1, 1)],
    },
    Sot23 {
        name: "SOT-23-5",
        description: "SOT, 5 Pin (JEDEC MO-178 Var AA)",
        overall: Size::range(2.8, 2.8),
        lead_length: Size::range(0.3, 0.6),
        lead_width: SOT_LEAD_WIDTH,
        body: (1.6, 2.9),
        pins: &[(1, -1, 0), (2, -1, 1), (3, -1, 2), (4, 1, 2), (5, 1, 0)],
    },
    Sot23 {
        name: "SOT-23-6",
        description: "SOT, 6 Pin (JEDEC MO-178 Var AB)",
        overall: Size::range(2.8, 2.8),
        lead_length: Size::range(0.3, 0.6),
        lead_width: SOT_LEAD_WIDTH,
        body: (1.6, 2.9),
        pins: &[
            (1, -1, 0),
            (2, -1, 1),
            (3, -1, 2),
            (4, 1, 2),
            (5, 1, 1),
            (6, 1, 0),
        ],
    },
];

const SOT_LEAD_WIDTH: Size = Size::range(0.3, 0.5);

/// The SOT-223 variants: (name, pad of the tab is numbered, pins on the narrow side).
const SOT_223: [(&str, &str); 2] = [("SOT-223-3_TabPin2", "2"), ("SOT-223", "4")];

const LIBRARY: &str = "Package_TO_SOT_SMD";

/// Every `Library:Name` this module generates.
pub fn names() -> Vec<String> {
    SOT_23
        .iter()
        .map(|s| s.name)
        .chain(SOT_223.iter().map(|s| s.0))
        .chain(SC_70.iter().map(|s| s.name))
        .map(|n| format!("{LIBRARY}:{n}"))
        .collect()
}

/// The footprint named `library:name`, if it is a package of this module.
pub fn generate(library: &str, name: &str) -> Option<Footprint> {
    if library != LIBRARY {
        return None;
    }
    if let Some(s) = SOT_23.iter().find(|s| s.name == name) {
        return Some(sot23(s, LIBRARY, s.name));
    }
    if let Some(s) = SC_70.iter().find(|s| s.name == name) {
        return Some(sc70_footprint(s));
    }
    let (_, tab) = SOT_223.iter().find(|s| s.0 == name)?;
    Some(sot223(name, tab))
}

/// `gen:SOT23(...)`: a SOT-23 with three, five or six leads. What a parameter leaves out is
/// the SOT-23-3, -5 and -6 row's own (JEDEC MO-178): 2.8 mm over the leads, feet 0.3 to 0.6 mm
/// long and 0.3 to 0.5 mm wide, a 1.6 x 2.9 mm body, the pitch 0.95 mm.
pub(super) fn from_params(p: &Params, library: &str, name: &str) -> Result<Footprint, String> {
    let pins = p.count("pins", "the number of leads, 3, 5 or 6", 3, 6)?;
    let row = match pins {
        3 => &SOT_23[1],
        5 => &SOT_23[2],
        6 => &SOT_23[3],
        _ => {
            return Err(format!(
                "`pins={pins}` is not a SOT-23 with a pad pattern netsmith knows: 3, 5 or 6"
            ));
        }
    };
    let package = Sot23 {
        name: "",
        description: row.description,
        overall: p
            .opt_size(
                "span",
                "the distance from tip to tip of the leads of the two columns",
                1.0,
                10.0,
            )?
            .unwrap_or(row.overall),
        lead_length: p
            .opt_size("lead_length", "the length of the foot of a lead", 0.1, 2.0)?
            .unwrap_or(row.lead_length),
        lead_width: p
            .opt_size("lead_width", "the width of a lead", 0.1, 1.0)?
            .unwrap_or(row.lead_width),
        body: p
            .opt_length_pair(
                "body",
                "the body across the columns and along them",
                0.5,
                10.0,
            )?
            .unwrap_or(row.body),
        pins: row.pins,
    };
    Ok(sot23(&package, library, name))
}

fn sot23(s: &Sot23, library: &str, name: &str) -> Footprint {
    let land = ipc::land(
        &GULL_WING,
        s.overall,
        Inside::Length(s.lead_length),
        s.lead_width,
    );
    let pads: Vec<PadSpec> = s
        .pins
        .iter()
        .map(|&(number, column, row)| {
            PadSpec::smd(
                number,
                Pt::new(
                    f64::from(column) * land.centre(),
                    (f64::from(row) - 1.0) * PITCH,
                ),
                (land.length(), land.width),
            )
        })
        .collect();
    two_columns(
        library,
        name,
        &format!(
            "{}, IPC-7351B nominal density, generated by netsmith",
            s.description
        ),
        "SOT TO_SOT_SMD",
        &pads,
        Body {
            width: s.body.0,
            length: s.body.1,
        },
        GULL_WING.courtyard,
    )
}

/// Three 2.0 x 1.5 mm pads 2.3 mm apart on the left; the tab, 2.0 x 3.8 mm, on the right.
fn sot223(name: &str, tab: &str) -> Footprint {
    let mut pads = Vec::new();
    for (i, y) in [-2.3, 0.0, 2.3].into_iter().enumerate() {
        pads.push(PadSpec::smd(i + 1, Pt::new(-3.15, y), (2.0, 1.5)));
    }
    pads.push(PadSpec::smd(tab, Pt::new(3.15, 0.0), (2.0, 3.8)));
    two_columns(
        LIBRARY,
        name,
        "SOT-223, 3 pins and a tab, JEDEC TO-261, generated by netsmith",
        "SOT-223 TO_SOT_SMD",
        &pads,
        Body {
            width: 3.7,
            length: 6.7,
        },
        0.25,
    )
}

/// An SC-70 variant, its pads recorded.
struct Sc70 {
    name: &'static str,
    description: &'static str,
    /// The number of pins, in two columns.
    pins: usize,
    /// The pitch along the columns.
    pitch: f64,
    /// The pins of a three-pin package: pin 3 is alone on the right, between the two rows
    /// of pins 1 and 2.
    three: bool,
    /// The centre of the pads of each column from the middle, and the pad across the
    /// columns (x) and along them (y).
    column: f64,
    pad: (f64, f64),
    /// Half width and half height of the courtyard.
    courtyard: (f64, f64),
}

const SC_70_BODY: Body = Body {
    width: 1.35,
    length: 2.2,
};

/// A row: the reflow pads, then the pads for hand soldering.
const fn row(
    name: &'static str,
    description: &'static str,
    pins: usize,
    pitch: f64,
    column: f64,
    pad: (f64, f64),
    courtyard: (f64, f64),
) -> Sc70 {
    Sc70 {
        name,
        description,
        pins,
        pitch,
        three: pins == 3,
        column,
        pad,
        courtyard,
    }
}

const SC_70: [Sc70; 10] = [
    row(
        "SOT-323_SC-70",
        "SOT-323, SC-70",
        3,
        1.3,
        1.0,
        (0.7, 0.45),
        (1.7, 1.3),
    ),
    row(
        "SOT-323_SC-70_Handsoldering",
        "SOT-323, SC-70, pads for hand soldering",
        3,
        1.3,
        1.33,
        (1.5, 0.45),
        (2.4, 1.3),
    ),
    row(
        "SOT-343_SC-70-4",
        "SOT-343, SC-70-4",
        4,
        1.3,
        0.95,
        (0.65, 0.4),
        (1.6, 1.4),
    ),
    row(
        "SOT-343_SC-70-4_Handsoldering",
        "SOT-343, SC-70-4, pads for hand soldering",
        4,
        1.3,
        1.33,
        (1.5, 0.4),
        (2.4, 1.4),
    ),
    row(
        "SOT-353_SC-70-5",
        "SOT-353, SC-70-5",
        5,
        0.65,
        0.95,
        (0.65, 0.4),
        (1.6, 1.4),
    ),
    row(
        "SOT-353_SC-70-5_Handsoldering",
        "SOT-353, SC-70-5, pads for hand soldering",
        5,
        0.65,
        1.33,
        (1.5, 0.4),
        (2.4, 1.4),
    ),
    row(
        "SOT-363_SC-70-6",
        "SOT-363, SC-70-6, SC-88",
        6,
        0.65,
        0.95,
        (0.65, 0.4),
        (1.6, 1.4),
    ),
    row(
        "SOT-363_SC-70-6_Handsoldering",
        "SOT-363, SC-70-6, SC-88, pads for hand soldering",
        6,
        0.65,
        1.33,
        (1.5, 0.4),
        (2.4, 1.4),
    ),
    row("SC-70-8", "SC70-8", 8, 0.5, 0.9, (1.0, 0.3), (1.7, 1.3)),
    row(
        "SC-70-8_Handsoldering",
        "SC70-8, pads for hand soldering",
        8,
        0.5,
        1.15,
        (1.5, 0.3),
        (2.2, 1.3),
    ),
];

/// The pads of an SC-70 package: down the left column, then up the right one; the
/// three-pin package has pins 1 and 2 on the left and pin 3 on the right in the middle.
fn sc70_footprint(s: &Sc70) -> Footprint {
    let mut pads = Vec::new();
    if s.three {
        let y = s.pitch / 2.0;
        for (n, x, at_y) in [(1, -s.column, -y), (2, -s.column, y), (3, s.column, 0.0)] {
            pads.push(PadSpec::smd(n, Pt::new(x, at_y), s.pad));
        }
    } else {
        // Five pins: three on the left, two on the right at every other place of the grid.
        let left = if s.pins == 5 { 3 } else { s.pins / 2 };
        let right = s.pins - left;
        let right_pitch = if s.pins == 5 { 2.0 * s.pitch } else { s.pitch };
        let top = (left as f64 - 1.0) / 2.0 * s.pitch;
        for i in 0..left {
            pads.push(PadSpec::smd(
                i + 1,
                Pt::new(-s.column, -top + i as f64 * s.pitch),
                s.pad,
            ));
        }
        let top = (right as f64 - 1.0) / 2.0 * right_pitch;
        for i in 0..right {
            pads.push(PadSpec::smd(
                left + i + 1,
                Pt::new(s.column, top - i as f64 * right_pitch),
                s.pad,
            ));
        }
    }
    two_columns_in(
        LIBRARY,
        s.name,
        &format!("{}, pads as recorded, generated by netsmith", s.description),
        "SC-70 SOT TO_SOT_SMD",
        &pads,
        SC_70_BODY,
        s.courtyard,
    )
}
