//! Quad and dual flat no-lead packages: QFN and DFN (and the WSON and VSON that are DFN
//! under other names), with an exposed pad under the body.
//!
//! The leads end at the body's edge, so the pad is computed from the body's size and the
//! lead's length and width with the flat no-lead fillets of IPC-7351B ([`ipc::FLAT_NO_LEAD`]).
//! The inner edges of the pads are kept 0.2 mm from the exposed pad. The exposed pad's paste is
//! cut into a grid of openings that cover 65 % of it, which keeps the paste from floating
//! the part. The dimensions of each package are the manufacturers' drawings, which the name
//! of each row's source in KiCad's footprint scripts cites (ADR 0043).

use super::draw::{Drawing, Kind, PadSpec, Shape};
use super::ipc::{self, FLAT_NO_LEAD, Inside, Size};
use super::outline::quad_frame;
use super::params::Params;
use super::qfn_rows::MORE;
use crate::footprint::{Footprint, Mount};
use crate::geometry::Pt;

/// KiCad keeps the QFN and DFN in one library and the SON family in another.
fn library_of(p: &Package) -> &'static str {
    if let Some(library) = p.library {
        return library;
    }
    let name = p.name;
    if name.starts_with("QFN") || name.starts_with("DFN") {
        "Package_DFN_QFN"
    } else {
        "Package_SON"
    }
}

/// The least room between the inner edge of a lead's pad and the exposed pad.
const EXPOSED_PAD_CLEARANCE: f64 = 0.2;
/// The widest an opening of the paste over the exposed pad is when `gen:` does not say how
/// many: netsmith's own rule, not a datasheet's (the rows' counts are each maker's).
const DEFAULT_PASTE_OPENING: f64 = 1.5;
/// The share of the exposed pad that the paste openings cover.
const PASTE_COVERAGE: f64 = 0.65;

pub(super) struct Package {
    name: &'static str,
    /// The KiCad library of the name, where the name's start does not say.
    library: Option<&'static str>,
    /// Leads on the top and bottom edges, and on the left and right ones.
    leads_across: usize,
    leads_down: usize,
    pitch: f64,
    body: (Size, Size),
    lead_width: Size,
    lead_length: Size,
    exposed: (f64, f64),
    /// Paste openings across and down.
    paste: (usize, usize),
    heel_reduction: f64,
}

// A table row: every argument is a column.
#[allow(clippy::too_many_arguments)]
pub(super) const fn qfn(
    name: &'static str,
    leads_across: usize,
    leads_down: usize,
    pitch: f64,
    body_x: (f64, f64),
    body_y: (f64, f64),
    lead_width: (f64, f64),
    lead_length: (f64, f64),
    exposed: (f64, f64),
    paste: (usize, usize),
    heel_reduction: f64,
) -> Package {
    Package {
        name,
        library: None,
        leads_across,
        leads_down,
        pitch,
        body: (
            Size::range(body_x.0, body_x.1),
            Size::range(body_y.0, body_y.1),
        ),
        lead_width: Size::range(lead_width.0, lead_width.1),
        lead_length: Size::range(lead_length.0, lead_length.1),
        exposed,
        paste,
        heel_reduction,
    }
}

static PACKAGES: &[&[Package]] = &[&FIRST, &MORE];

/// The same row, for a name whose library its start does not say.
pub(super) const fn in_library(mut p: Package, library: &'static str) -> Package {
    p.library = Some(library);
    p
}

const FIRST: [Package; 30] = [
    qfn(
        "QFN-16-1EP_3x3mm_P0.5mm_EP1.7x1.7mm",
        4,
        4,
        0.5,
        (2.9, 3.1),
        (2.9, 3.1),
        (0.18, 0.3),
        (0.3, 0.5),
        (1.7, 1.7),
        (2, 2),
        0.05,
    ),
    qfn(
        "QFN-16-1EP_4x4mm_P0.65mm_EP2.5x2.5mm",
        4,
        4,
        0.65,
        (4.0, 4.0),
        (4.0, 4.0),
        (0.25, 0.35),
        (0.3, 0.5),
        (2.5, 2.5),
        (2, 2),
        0.0,
    ),
    qfn(
        "QFN-20-1EP_4x4mm_P0.5mm_EP2.5x2.5mm",
        5,
        5,
        0.5,
        (4.0, 4.0),
        (4.0, 4.0),
        (0.18, 0.3),
        (0.3, 0.5),
        (2.5, 2.5),
        (2, 2),
        0.0,
    ),
    qfn(
        "QFN-24-1EP_4x4mm_P0.5mm_EP2.5x2.5mm",
        6,
        6,
        0.5,
        (3.95, 4.05),
        (3.95, 4.05),
        (0.18, 0.3),
        (0.35, 0.45),
        (2.5, 2.5),
        (2, 2),
        0.0,
    ),
    qfn(
        "QFN-28-1EP_5x5mm_P0.5mm_EP3.35x3.35mm",
        7,
        7,
        0.5,
        (5.0, 5.0),
        (5.0, 5.0),
        (0.18, 0.3),
        (0.35, 0.45),
        (3.35, 3.35),
        (3, 3),
        0.0,
    ),
    qfn(
        "QFN-32-1EP_5x5mm_P0.5mm_EP3.1x3.1mm",
        8,
        8,
        0.5,
        (4.9, 5.1),
        (4.9, 5.1),
        (0.18, 0.3),
        (0.3, 0.5),
        (3.1, 3.1),
        (3, 3),
        0.0,
    ),
    qfn(
        "QFN-32-1EP_5x5mm_P0.5mm_EP3.6x3.6mm",
        8,
        8,
        0.5,
        (5.0, 5.0),
        (5.0, 5.0),
        (0.2, 0.3),
        (0.35, 0.45),
        (3.6, 3.6),
        (3, 3),
        0.0,
    ),
    qfn(
        "QFN-40-1EP_6x6mm_P0.5mm_EP4.6x4.6mm",
        10,
        10,
        0.5,
        (6.0, 6.0),
        (6.0, 6.0),
        (0.18, 0.3),
        (0.3, 0.5),
        (4.6, 4.6),
        (4, 4),
        0.0,
    ),
    qfn(
        "QFN-48-1EP_7x7mm_P0.5mm_EP5.15x5.15mm",
        12,
        12,
        0.5,
        (6.9, 7.1),
        (6.9, 7.1),
        (0.2, 0.3),
        (0.3, 0.5),
        (5.15, 5.15),
        (4, 4),
        0.0,
    ),
    qfn(
        "QFN-12-1EP_3x3mm_P0.5mm_EP1.6x1.6mm",
        3,
        3,
        0.5,
        (3.0, 3.0),
        (3.0, 3.0),
        (0.18, 0.3),
        (0.3, 0.5),
        (1.6, 1.6),
        (2, 2),
        0.0,
    ),
    qfn(
        "QFN-20-1EP_3x3mm_P0.4mm_EP1.65x1.65mm",
        5,
        5,
        0.4,
        (2.9, 3.1),
        (2.9, 3.1),
        (0.15, 0.25),
        (0.3, 0.5),
        (1.65, 1.65),
        (2, 2),
        0.0,
    ),
    qfn(
        "QFN-16-1EP_3x3mm_P0.5mm_EP1.45x1.45mm",
        4,
        4,
        0.5,
        (2.9, 3.1),
        (2.9, 3.1),
        (0.2, 0.3),
        (0.3, 0.5),
        (1.45, 1.45),
        (2, 2),
        0.0,
    ),
    qfn(
        "DFN-10-1EP_3x3mm_P0.5mm_EP1.7x2.5mm",
        0,
        5,
        0.5,
        (2.9, 3.1),
        (2.9, 3.1),
        (0.18, 0.3),
        (0.3, 0.5),
        (1.7, 2.5),
        (2, 2),
        0.0,
    ),
    qfn(
        "DFN-8-1EP_3x2mm_P0.5mm_EP1.7x1.6mm",
        0,
        4,
        0.5,
        (2.9, 3.1),
        (1.9, 2.1),
        (0.18, 0.3),
        (0.3, 0.5),
        (1.7, 1.6),
        (2, 2),
        0.0,
    ),
    qfn(
        "DFN-6-1EP_3x2mm_P0.5mm_EP1.65x1.35mm",
        0,
        3,
        0.5,
        (3.0, 3.0),
        (2.0, 2.0),
        (0.2, 0.3),
        (0.3, 0.5),
        (1.65, 1.35),
        (2, 2),
        0.0,
    ),
    qfn(
        "DFN-8-1EP_3x3mm_P0.65mm_EP1.5x2.25mm",
        0,
        4,
        0.65,
        (2.95, 3.05),
        (2.95, 3.05),
        (0.2, 0.3),
        (0.3, 0.6),
        (1.5, 2.25),
        (2, 2),
        0.0,
    ),
    qfn(
        "DFN-12-1EP_3x3mm_P0.5mm_EP1.6x2.5mm",
        0,
        6,
        0.5,
        (2.95, 3.05),
        (2.95, 3.05),
        (0.18, 0.28),
        (-0.1, 0.9),
        (1.6, 2.5),
        (2, 2),
        0.0,
    ),
    qfn(
        "DFN-8-1EP_2x2mm_P0.5mm_EP0.9x1.5mm",
        0,
        4,
        0.5,
        (1.95, 2.05),
        (1.95, 2.05),
        (0.2, 0.3),
        (0.2, 0.4),
        (0.9, 1.5),
        (2, 2),
        0.025,
    ),
    qfn(
        "DFN-8-1EP_3x3mm_P0.65mm_EP1.7x2.05mm",
        0,
        4,
        0.65,
        (3.0, 3.0),
        (3.0, 3.0),
        (0.25, 0.35),
        (0.35, 0.45),
        (1.7, 2.05),
        (2, 2),
        0.0,
    ),
    qfn(
        "DFN-10-1EP_2.6x2.6mm_P0.5mm_EP1.3x2.2mm",
        0,
        5,
        0.5,
        (2.5, 2.7),
        (2.5, 2.7),
        (0.2, 0.3),
        (0.3, 0.4),
        (1.3, 2.2),
        (2, 2),
        0.0,
    ),
    qfn(
        "QFN-20-1EP_5x5mm_P0.65mm_EP3.35x3.35mm",
        5,
        5,
        0.65,
        (5.0, 5.0),
        (5.0, 5.0),
        (0.25, 0.35),
        (0.35, 0.45),
        (3.35, 3.35),
        (2, 2),
        0.0,
    ),
    qfn(
        "QFN-24-1EP_5x5mm_P0.65mm_EP3.4x3.4mm",
        6,
        6,
        0.65,
        (4.9, 5.1),
        (4.9, 5.1),
        (0.2, 0.3),
        (0.35, 0.45),
        (3.4, 3.4),
        (2, 2),
        0.0,
    ),
    qfn(
        "QFN-28-1EP_5x5mm_P0.5mm_EP3.1x3.1mm",
        7,
        7,
        0.5,
        (5.0, 5.0),
        (5.0, 5.0),
        (0.2, 0.3),
        (0.5, 0.6),
        (3.1, 3.1),
        (3, 3),
        0.0,
    ),
    qfn(
        "QFN-44-1EP_7x7mm_P0.5mm_EP5.2x5.2mm",
        11,
        11,
        0.5,
        (6.9, 7.1),
        (6.9, 7.1),
        (0.18, 0.3),
        (0.59, 0.69),
        (5.2, 5.2),
        (4, 4),
        0.0,
    ),
    qfn(
        "WSON-10-1EP_2.5x2.5mm_P0.5mm_EP1.2x2mm",
        0,
        5,
        0.5,
        (2.4, 2.6),
        (2.4, 2.6),
        (0.2, 0.3),
        (0.35, 0.45),
        (1.2, 2.0),
        (2, 2),
        0.0,
    ),
    qfn(
        "WSON-10-1EP_2x3mm_P0.5mm_EP0.84x2.4mm",
        0,
        5,
        0.5,
        (1.9, 2.1),
        (2.9, 3.1),
        (0.2, 0.3),
        (0.25, 0.35),
        (0.84, 2.4),
        (1, 2),
        0.0,
    ),
    qfn(
        "WSON-10-1EP_4x4mm_P0.8mm_EP2.6x3mm",
        0,
        5,
        0.8,
        (3.9, 4.1),
        (3.9, 4.1),
        (0.25, 0.35),
        (0.3, 0.5),
        (2.6, 3.0),
        (2, 2),
        0.0,
    ),
    qfn(
        "HVSON-8-1EP_3x3mm_P0.65mm_EP1.6x2.4mm",
        0,
        4,
        0.65,
        (2.9, 3.1),
        (2.9, 3.1),
        (0.25, 0.35),
        (0.35, 0.45),
        (1.6, 2.4),
        (2, 2),
        0.0,
    ),
    qfn(
        "HVSON-8-1EP_4x4mm_P0.8mm_EP2.2x3.1mm",
        0,
        4,
        0.8,
        (3.9, 4.1),
        (3.9, 4.1),
        (0.3, 0.4),
        (0.4, 0.65),
        (2.2, 3.1),
        (2, 2),
        0.0,
    ),
    qfn(
        "WSON-12-1EP_3x3mm_P0.5mm_EP1.5x2.5mm",
        0,
        6,
        0.5,
        (2.9, 3.1),
        (2.9, 3.1),
        (0.2, 0.3),
        (0.3, 0.5),
        (1.5, 2.5),
        (2, 2),
        0.0,
    ),
];

fn all() -> impl Iterator<Item = &'static Package> {
    PACKAGES.iter().flat_map(|rows| rows.iter())
}

/// Every `Library:Name` this module generates.
pub fn names() -> Vec<String> {
    all()
        .map(|p| format!("{}:{}", library_of(p), p.name))
        .collect()
}

/// The footprint named `library:name`, if it is a package of this module.
pub fn generate(library: &str, name: &str) -> Option<Footprint> {
    all()
        .find(|p| p.name == name && library_of(p) == library)
        .map(|p| no_lead(p, library_of(p), p.name))
}

/// `gen:QFN(...)` and `gen:DFN(...)`: the row's columns, named. A QFN has the same number
/// of leads on each edge; a DFN has two edges of leads.
pub(super) fn from_params(
    p: &Params,
    library: &str,
    name: &str,
    dual: bool,
) -> Result<Footprint, String> {
    let pins = if dual {
        let n = p.count("pins", "the number of leads", 4, 100)?;
        if n % 2 != 0 {
            return Err(format!(
                "`pins={n}` must be even: a DFN has the same number of leads on its two edges"
            ));
        }
        n
    } else {
        let n = p.count("pins", "the number of leads", 8, 400)?;
        if n % 4 != 0 {
            return Err(format!(
                "`pins={n}` must be a multiple of 4: a QFN has the same number of leads on each of its four edges"
            ));
        }
        n
    };
    let pitch = p.length("pitch", "the distance between lead centres", 0.3, 2.0)?;
    let body = p.size_pair("body", "the body across and along", 1.0, 25.0)?;
    let exposed = p.length_pair("ep", "the exposed pad across and along", 0.2, 25.0)?;
    let lead_width = p.size("lead_width", "the width of a lead", 0.1, 1.5)?;
    let lead_length = p.size(
        "lead_length",
        "the length of a lead on the underside",
        0.1,
        2.0,
    )?;
    let paste = p
        .opt_count_pair(
            "paste",
            "the paste openings over the exposed pad, across and down",
            1,
            10,
        )?
        .unwrap_or_else(|| {
            let openings = |e: f64| ((e / DEFAULT_PASTE_OPENING).ceil() as usize).max(1);
            (openings(exposed.0), openings(exposed.1))
        });
    let heel_reduction = p
        .opt_length("heel", "the heel fillet taken off the pad", 0.0, 0.5)?
        .unwrap_or(0.0);
    let (leads_across, leads_down) = if dual {
        (0, pins / 2)
    } else {
        (pins / 4, pins / 4)
    };
    let package = Package {
        name: "",
        library: None,
        leads_across,
        leads_down,
        pitch,
        body,
        lead_width,
        lead_length,
        exposed,
        paste,
        heel_reduction,
    };
    Ok(no_lead(&package, library, name))
}

fn no_lead(p: &Package, library: &str, name: &str) -> Footprint {
    // The pads of the left and right edges are across the x axis, those of the top and
    // bottom edges across the y axis. Each axis has the land its body size gives.
    let land = |body: Size, exposed: f64| {
        let mut land = ipc::land_with_heel_reduction(
            &FLAT_NO_LEAD,
            body,
            Inside::Length(p.lead_length),
            p.lead_width,
            p.heel_reduction,
        );
        let least_gap = exposed + 2.0 * EXPOSED_PAD_CLEARANCE;
        land.inner = land.inner.max(least_gap);
        land
    };
    let across = land(p.body.0, p.exposed.0);
    let down = land(p.body.1, p.exposed.1);

    let mut pads = Vec::new();
    let mut number = 0;
    let mut add = |at: Pt, size: (f64, f64)| {
        number += 1;
        pads.push(PadSpec::smd(number, at, size));
    };
    let (n_across, n_down) = (p.leads_across, p.leads_down);
    let span = |n: usize| (n as f64 - 1.0) / 2.0 * p.pitch;
    // Left edge, top to bottom.
    for i in 0..n_down {
        let y = -span(n_down) + i as f64 * p.pitch;
        add(
            Pt::new(-across.centre(), y),
            (across.length(), across.width),
        );
    }
    // Bottom edge, left to right.
    for i in 0..n_across {
        let x = -span(n_across) + i as f64 * p.pitch;
        add(Pt::new(x, down.centre()), (down.width, down.length()));
    }
    // Right edge, bottom to top.
    for i in 0..n_down {
        let y = span(n_down) - i as f64 * p.pitch;
        add(Pt::new(across.centre(), y), (across.length(), across.width));
    }
    // Top edge, right to left.
    for i in 0..n_across {
        let x = span(n_across) - i as f64 * p.pitch;
        add(Pt::new(x, -down.centre()), (down.width, down.length()));
    }
    let leads = number;

    let mut drawing = Drawing::new(library, name, Mount::Smd);
    drawing.describe(
        &format!(
            "{} {}-pin package with exposed pad, IPC-7351B nominal density, generated by netsmith",
            if n_across == 0 {
                "Dual flat no-lead"
            } else {
                "Quad flat no-lead"
            },
            leads
        ),
        "QFN DFN",
    );
    for pad in &pads {
        drawing.pad(pad);
    }

    exposed_pad(&mut drawing, leads + 1, p.exposed, p.paste);

    let body = (p.body.0.nominal(), p.body.1.nominal());
    quad_frame(
        drawing,
        &pads,
        n_down,
        n_across,
        body,
        FLAT_NO_LEAD.courtyard,
    )
}

/// The exposed pad at the centre, numbered `number`, with the paste on it cut into
/// `paste` openings across and down that cover 65 % of it, as stencil openings that are
/// not pads. Shared with the quad flat packs that have an exposed pad.
pub(super) fn exposed_pad(
    drawing: &mut Drawing,
    number: usize,
    (ex, ey): (f64, f64),
    (px, py): (usize, usize),
) {
    drawing.pad(&PadSpec::new(
        number,
        Kind::SmdNoPaste,
        Shape::Rect,
        Pt::new(0.0, 0.0),
        (ex, ey),
    ));
    let scale = PASTE_COVERAGE.sqrt();
    let size = (
        ipc::round_to(ex * scale / px as f64, 0.01),
        ipc::round_to(ey * scale / py as f64, 0.01),
    );
    for j in 0..py {
        for i in 0..px {
            let at = Pt::new(
                (i as f64 - (px as f64 - 1.0) / 2.0) * ex / px as f64,
                (j as f64 - (py as f64 - 1.0) / 2.0) * ey / py as f64,
            );
            drawing.pad(&PadSpec::new(
                "",
                Kind::PasteOnly,
                Shape::RoundRect,
                at,
                size,
            ));
        }
    }
}
