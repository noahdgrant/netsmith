//! The drawings that go with pads in a package whose pins are in a left and a right column
//! (SOT, SOIC, SSOP, TSSOP, MSOP): the body on the fabrication layer with pin 1's corner
//! cut, silkscreen above and below the body and a marker by pin 1, and the courtyard.
//!
//! The silkscreen clears the copper by 0.26 mm, a silkscreen line's half width plus the
//! 0.2 mm that KiCad's library conventions keep between silkscreen and pads.

use super::draw::{Draw, Drawing, PadSpec};
use super::ipc;
use crate::footprint::{Footprint, Mount};
use crate::geometry::Pt;

/// How far silkscreen keeps from copper, centre line to pad edge.
pub(super) const SILK_TO_COPPER: f64 = 0.26;

/// The body of a two-column package: `width` across the columns (x), `length` along them (y).
#[derive(Debug, Clone, Copy)]
pub(super) struct Body {
    pub width: f64,
    pub length: f64,
}

/// A footprint from its pads (pin 1 first) and body, with `courtyard` mm round the larger of
/// the pads and the body.
pub(super) fn two_columns(
    library: &str,
    name: &str,
    description: &str,
    tags: &str,
    pads: &[PadSpec],
    body: Body,
    courtyard: f64,
) -> Footprint {
    let bounds = pad_bounds(pads);
    let court = (
        bounds.0.abs().max(bounds.2).max(body.width / 2.0) + courtyard,
        bounds.1.abs().max(bounds.3).max(body.length / 2.0) + courtyard,
    );
    two_columns_in(library, name, description, tags, pads, body, court)
}

/// The box round the pads: (min x, min y, max x, max y).
fn pad_bounds(pads: &[PadSpec]) -> (f64, f64, f64, f64) {
    let mut bounds = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for pad in pads {
        bounds.0 = bounds.0.min(pad.at.x - pad.size.0 / 2.0);
        bounds.1 = bounds.1.min(pad.at.y - pad.size.1 / 2.0);
        bounds.2 = bounds.2.max(pad.at.x + pad.size.0 / 2.0);
        bounds.3 = bounds.3.max(pad.at.y + pad.size.1 / 2.0);
    }
    bounds
}

/// Like [`two_columns`], with the courtyard given as its half width and half height instead
/// of a margin: for the packages whose courtyard is recorded, not computed.
pub(super) fn two_columns_in(
    library: &str,
    name: &str,
    description: &str,
    tags: &str,
    pads: &[PadSpec],
    body: Body,
    court: (f64, f64),
) -> Footprint {
    let mut drawing = Drawing::new(library, name, Mount::Smd);
    drawing.describe(description, tags);
    for pad in pads {
        drawing.pad(pad);
    }
    let bounds = pad_bounds(pads);
    let (hw, hl) = (body.width / 2.0, body.length / 2.0);
    drawing.courtyard(Pt::new(-court.0, -court.1), Pt::new(court.0, court.1));

    // The body, with the corner by pin 1 cut off by a quarter of the width.
    let cut = body.width / 4.0;
    drawing.polygon(
        Draw::Fab,
        &[
            Pt::new(-hw + cut, -hl),
            Pt::new(hw, -hl),
            Pt::new(hw, hl),
            Pt::new(-hw, hl),
            Pt::new(-hw, -hl + cut),
        ],
        false,
    );
    drawing.fab_reference((body.width.min(body.length) * 0.25).clamp(0.3, 1.0));

    // Silkscreen along the body's two ends, clear of the pads.
    let silk_y = (hl + 0.11).max(bounds.1.abs().max(bounds.3) + SILK_TO_COPPER);
    drawing.line(Draw::Silk, Pt::new(-hw, -silk_y), Pt::new(hw, -silk_y));
    drawing.line(Draw::Silk, Pt::new(-hw, silk_y), Pt::new(hw, silk_y));

    // A small filled triangle above pin 1, pointing at it.
    if let Some(first) = pads.first() {
        let apex = Pt::new(
            first.at.x - 0.225,
            first.at.y - first.size.1 / 2.0 - SILK_TO_COPPER,
        );
        drawing.polygon(
            Draw::Silk,
            &[
                apex,
                Pt::new(apex.x - 0.24, apex.y - 0.33),
                Pt::new(apex.x + 0.24, apex.y - 0.33),
            ],
            true,
        );
    }

    let reference_y = -(court.1 + 0.7);
    drawing.finish(reference_y, -reference_y)
}

/// The pads of a package with `per_side` pins in each of two columns, numbered down the left
/// column and up the right one, as the land `land` asks for at `pitch`.
pub(super) fn dual_pads(per_side: usize, pitch: f64, land: &ipc::Land) -> Vec<PadSpec> {
    let mut pads = Vec::with_capacity(per_side * 2);
    let top = (per_side as f64 - 1.0) / 2.0 * pitch;
    for i in 0..per_side {
        pads.push(PadSpec::smd(
            i + 1,
            Pt::new(-land.centre(), -top + i as f64 * pitch),
            (land.length(), land.width),
        ));
    }
    for i in 0..per_side {
        pads.push(PadSpec::smd(
            per_side + i + 1,
            Pt::new(land.centre(), top - i as f64 * pitch),
            (land.length(), land.width),
        ));
    }
    pads
}

/// The drawings of a package with leads on all four edges, or on the left and right ones
/// only (`n_across` is 0): the pads in `pads` (the leads, numbered left edge top to bottom,
/// bottom edge, right edge, top edge; `n_down` is the number on the left edge) have already
/// been added to `drawing`. Adds the courtyard, `courtyard` mm round the larger of the pads
/// and the `body`, the body outline with pin 1's corner cut, the silkscreen corners and a
/// marker at pin 1, and returns the footprint.
pub(super) fn quad_frame(
    mut drawing: Drawing,
    pads: &[PadSpec],
    n_down: usize,
    n_across: usize,
    body: (f64, f64),
    courtyard: f64,
) -> Footprint {
    let mut bounds = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for pad in pads {
        bounds.0 = bounds.0.min(pad.at.x - pad.size.0 / 2.0);
        bounds.1 = bounds.1.min(pad.at.y - pad.size.1 / 2.0);
        bounds.2 = bounds.2.max(pad.at.x + pad.size.0 / 2.0);
        bounds.3 = bounds.3.max(pad.at.y + pad.size.1 / 2.0);
    }
    let (hw, hh) = (body.0 / 2.0, body.1 / 2.0);
    let court = (
        bounds.0.abs().max(bounds.2).max(hw) + courtyard,
        bounds.1.abs().max(bounds.3).max(hh) + courtyard,
    );
    drawing.courtyard(Pt::new(-court.0, -court.1), Pt::new(court.0, court.1));

    // The body, with the corner by pin 1 cut off by a quarter of the shorter side.
    let cut = (body.0.min(body.1) / 4.0).min(1.0);
    drawing.polygon(
        Draw::Fab,
        &[
            Pt::new(-hw + cut, -hh),
            Pt::new(hw, -hh),
            Pt::new(hw, hh),
            Pt::new(-hw, hh),
            Pt::new(-hw, -hh + cut),
        ],
        false,
    );
    drawing.fab_reference((body.0.min(body.1) * 0.25).clamp(0.3, 1.0));

    // Silkscreen: the corners of the body, stopping short of the pads, and a marker at
    // pin 1's corner.
    let clear = SILK_TO_COPPER;
    let (sx, sy) = (hw + 0.11, hh + 0.11);
    let first_y = |pad: &PadSpec| pad.at.y.abs() + pad.size.1 / 2.0 + clear;
    let first_x = |pad: &PadSpec| pad.at.x.abs() + pad.size.0 / 2.0 + clear;
    // The nearest pad to a corner on each edge is the outermost one.
    let vertical_stop = pads.iter().take(n_down).map(first_y).fold(0.0, f64::max);
    let horizontal_stop = if n_across > 0 {
        pads[n_down..n_down + n_across]
            .iter()
            .map(first_x)
            .fold(0.0, f64::max)
    } else {
        sx - 0.5
    };
    for (cx, cy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        if sy > vertical_stop + 0.05 {
            drawing.line(
                Draw::Silk,
                Pt::new(cx * sx, cy * sy),
                Pt::new(cx * sx, cy * vertical_stop),
            );
        }
        if sx > horizontal_stop + 0.05 {
            drawing.line(
                Draw::Silk,
                Pt::new(cx * sx, cy * sy),
                Pt::new(cx * horizontal_stop, cy * sy),
            );
        }
    }
    let apex = Pt::new(-sx, -sy);
    drawing.polygon(
        Draw::Silk,
        &[
            apex,
            Pt::new(apex.x - 0.24, apex.y - 0.33),
            Pt::new(apex.x + 0.24, apex.y - 0.33),
        ],
        true,
    );

    let reference_y = -(court.1 + 0.7);
    drawing.finish(reference_y, -reference_y)
}
