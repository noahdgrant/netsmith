//! Quad flat packages with gull-wing leads on four edges: LQFP, TQFP and VQFP.
//!
//! The outline is JEDEC MS-026 (thin and low profile quad flat packs): the span over the
//! leads is the body plus 2.0 mm (a basic dimension, so without a tolerance), a lead is
//! 0.45 to 0.75 mm long, and its width depends on the pitch. The pads follow IPC-7351B at
//! nominal density ([`ipc`]), the fine-pitch fillet goals from 0.625 mm down.
//!
//! Only the packages whose KiCad footprint is this pattern are generated. KiCad also has
//! QFPs made from one maker's drawing each (NXP's SOT outlines, ON Semiconductor's,
//! Microchip's hand-drawn pads): those have other lead spans and lengths, and so other pads,
//! and are read from KiCad's library as before (ADR 0043).

use super::draw::{Drawing, PadSpec};
use super::ipc::{self, GULL_WING, GULL_WING_FINE, Inside, Size};
use super::outline::quad_frame;
use crate::footprint::{Footprint, Mount};
use crate::geometry::Pt;

const LIBRARY: &str = "Package_QFP";

/// Leads outside the body: the span over the leads is the body plus this, in mm (MS-026).
const LEAD_REACH: f64 = 2.0;
/// The length of one lead, in mm (MS-026).
const LEAD_LENGTH: Size = Size::range(0.45, 0.75);

/// One package: a square body with the same number of leads on each edge.
struct Package {
    /// The kind in the footprint's name, and in its description.
    kind: &'static str,
    pins: usize,
    /// The side of the (square) body, in mm.
    body: f64,
    pitch: f64,
    lead_width: Size,
}

// The lead widths that reproduce KiCad 9.0's pads; the datasheets of the packages differ
// from one maker to the next within MS-026's ranges.
const W_0_4: Size = Size::range(0.13, 0.23);
const W_0_5: Size = Size::range(0.17, 0.27);
const W_0_65_A: Size = Size::range(0.17, 0.27);
const W_0_65_B: Size = Size::range(0.22, 0.33);
const W_0_8: Size = Size::range(0.30, 0.45);

const fn pkg(kind: &'static str, pins: usize, body: f64, pitch: f64, lead_width: Size) -> Package {
    Package {
        kind,
        pins,
        body,
        pitch,
        lead_width,
    }
}

const PACKAGES: &[Package] = &[
    pkg("LQFP", 44, 10.0, 0.8, W_0_8),
    pkg("LQFP", 48, 7.0, 0.5, W_0_5),
    pkg("LQFP", 52, 10.0, 0.65, W_0_65_A),
    pkg("LQFP", 80, 10.0, 0.4, W_0_4),
    pkg("LQFP", 128, 14.0, 0.4, W_0_4),
    pkg("LQFP", 144, 20.0, 0.5, W_0_5),
    pkg("LQFP", 216, 24.0, 0.4, W_0_4),
    pkg("TQFP", 48, 7.0, 0.5, W_0_5),
    pkg("TQFP", 64, 10.0, 0.5, W_0_5),
    pkg("TQFP", 100, 14.0, 0.5, W_0_5),
    pkg("TQFP", 144, 20.0, 0.5, W_0_5),
    pkg("TQFP", 176, 24.0, 0.5, W_0_5),
    pkg("VQFP", 80, 14.0, 0.65, W_0_65_B),
    pkg("VQFP", 100, 14.0, 0.5, W_0_5),
    pkg("VQFP", 128, 14.0, 0.4, W_0_4),
    pkg("VQFP", 176, 20.0, 0.4, W_0_4),
];

/// The name KiCad gives the footprint: `TQFP-48_7x7mm_P0.5mm`.
fn name_of(p: &Package) -> String {
    format!(
        "{}-{}_{}x{}mm_P{}mm",
        p.kind,
        p.pins,
        name_number(p.body),
        name_number(p.body),
        name_number(p.pitch)
    )
}

/// A number as KiCad writes it in a name: `7`, `0.5`, `0.65`.
fn name_number(v: f64) -> String {
    super::draw::num(v)
}

/// Every `Library:Name` this module generates.
pub fn names() -> Vec<String> {
    PACKAGES
        .iter()
        .map(|p| format!("{LIBRARY}:{}", name_of(p)))
        .collect()
}

/// The footprint named `library:name`, if it is a package of this module.
pub fn generate(library: &str, name: &str) -> Option<Footprint> {
    if library != LIBRARY {
        return None;
    }
    let p = PACKAGES.iter().find(|p| name_of(p) == name)?;
    Some(quad_flat_pack(p, name))
}

fn quad_flat_pack(p: &Package, name: &str) -> Footprint {
    let class = if p.pitch <= 0.625 {
        &GULL_WING_FINE
    } else {
        &GULL_WING
    };
    let overall = p.body + LEAD_REACH;
    let land = ipc::land(
        class,
        Size::range(overall, overall),
        Inside::Length(LEAD_LENGTH),
        p.lead_width,
    );
    let per_side = p.pins / 4;
    let span = (per_side as f64 - 1.0) / 2.0 * p.pitch;
    let mut pads = Vec::with_capacity(p.pins);
    let mut number = 0;
    let mut add = |at: Pt, size: (f64, f64)| {
        number += 1;
        pads.push(PadSpec::smd(number, at, size));
    };
    let (long, wide) = (land.length(), land.width);
    // Left edge top to bottom, bottom edge left to right, right edge bottom to top, top
    // edge right to left: counterclockwise from the top of the left edge.
    for i in 0..per_side {
        add(
            Pt::new(-land.centre(), -span + i as f64 * p.pitch),
            (long, wide),
        );
    }
    for i in 0..per_side {
        add(
            Pt::new(-span + i as f64 * p.pitch, land.centre()),
            (wide, long),
        );
    }
    for i in 0..per_side {
        add(
            Pt::new(land.centre(), span - i as f64 * p.pitch),
            (long, wide),
        );
    }
    for i in 0..per_side {
        add(
            Pt::new(span - i as f64 * p.pitch, -land.centre()),
            (wide, long),
        );
    }

    let mut drawing = Drawing::new(LIBRARY, name, Mount::Smd);
    drawing.describe(
        &format!(
            "{}, {} pins, {} mm body, {} mm pitch (JEDEC MS-026), IPC-7351B nominal density, generated by netsmith",
            p.kind,
            p.pins,
            name_number(p.body),
            name_number(p.pitch)
        ),
        &format!("{} QFP", p.kind),
    );
    for pad in &pads {
        drawing.pad(pad);
    }
    quad_frame(
        drawing,
        &pads,
        per_side,
        per_side,
        (p.body, p.body),
        class.courtyard,
    )
}
