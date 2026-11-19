//! Land grid arrays whose pads are a ring round the edge of the body, with a grid of pads
//! inside the ring or none (issue 1423): the sensors, modules and power stages whose package
//! drawing gives a few numbers (the pads on each edge, their pitch and size, where the rows are,
//! the body, and the inner grid with its pitch and pad).
//!
//! The pads are the drawing's recommended land pattern as it stands: an LGA has no leads to wet,
//! so no IPC fillet is added, and the size given is the copper, the paste and the mask opening.
//! The ring is numbered as a QFN is, counterclockwise from the top of the left edge: the left
//! edge top to bottom, the bottom edge left to right, the right edge bottom to top, the top edge
//! right to left. The inner grid follows, row by row from the top left. Each pad is a rectangle
//! with rounded corners (a quarter of the shorter side, at most 0.25 mm). The courtyard is
//! 0.25 mm round the larger of the pads and the body; the body is drawn on `F.Fab` with pin 1's
//! corner cut, so a row's `table` (issue 1281) measures it.
//!
//! The named rows are the `LayoutBorder` footprints of KiCad 9's `Package_LGA` library that this
//! rule reproduces (`tests/generated_vs_kicad.rs` holds them to KiCad's, pad for pad and in the
//! courtyard, to 0.01 mm): their numbers are read from KiCad 9.0.0's files (the same in 9.0.9).
//! KiCad's other LGAs are not made: square-cornered pads, pads turned 90 degrees in the file, a
//! courtyard of another margin (`LGA-14_2x2mm_P0.35mm`), rows off the pitch (the Rohm MLGA010),
//! or a ring with corner pads of their own and a split grid (the nRF9160 module). KiCad has no
//! footprint of a ring with a full inner grid, so the grid is held by the unit tests below.

use super::draw::{Drawing, PadSpec};
use super::outline::quad_frame;
use super::params::Params;
use crate::body3d::Package;
use crate::footprint::{Footprint, Mount};
use crate::geometry::Pt;

const LIBRARY: &str = "Package_LGA";
/// The courtyard's distance from the pads and the body, in mm.
const COURTYARD: f64 = 0.25;

/// The pads inside the ring: how many across and down, their pitch across and down, and the
/// size of one.
#[derive(Clone, Copy)]
struct Grid {
    count: (usize, usize),
    pitch: (f64, f64),
    pad: (f64, f64),
}

struct Lga {
    name: &'static str,
    /// Pads on each of the top and bottom edges, and on each of the left and right ones.
    edge: (usize, usize),
    pitch: f64,
    /// A ring pad's width along its edge and its length towards the middle.
    pad: (f64, f64),
    /// The distance between the centres of the left and right columns, and of the top and
    /// bottom rows.
    span: (f64, f64),
    body: (f64, f64),
    grid: Option<Grid>,
}

#[rustfmt::skip]
const PACKAGES: &[Lga] = &[
    Lga { name: "LGA-14_3x2.5mm_P0.5mm_LayoutBorder3x4y", edge: (3, 4), pitch: 0.5, pad: (0.35, 0.625), span: (2.325, 1.825), body: (3.0, 2.5), grid: None },
    Lga { name: "LGA-14_3x5mm_P0.8mm_LayoutBorder1x6y", edge: (1, 6), pitch: 0.8, pad: (0.6, 0.95), span: (2.0, 4.0), body: (3.0, 5.0), grid: None },
    Lga { name: "LGA-16_3x3mm_P0.5mm_LayoutBorder3x5y", edge: (3, 5), pitch: 0.5, pad: (0.35, 0.55), span: (2.45, 2.45), body: (3.0, 3.0), grid: None },
    Lga { name: "LGA-16_4x4mm_P0.65mm_LayoutBorder4x4y", edge: (4, 4), pitch: 0.65, pad: (0.4, 0.55), span: (3.4, 3.4), body: (4.0, 4.0), grid: None },
    Lga { name: "ST_HLGA-10_2.5x2.5mm_P0.6mm_LayoutBorder3x2y", edge: (3, 2), pitch: 0.6, pad: (0.4, 0.55), span: (1.85, 1.85), body: (2.5, 2.5), grid: None },
    Lga { name: "ST_HLGA-10_2x2mm_P0.5mm_LayoutBorder3x2y", edge: (3, 2), pitch: 0.5, pad: (0.35, 0.425), span: (1.525, 1.525), body: (2.0, 2.0), grid: None },
];

/// Every `Library:Name` this module generates.
pub(super) fn names() -> Vec<String> {
    PACKAGES
        .iter()
        .map(|p| format!("{LIBRARY}:{}", p.name))
        .collect()
}

/// The footprint named `library:name`, if it is a package of this module.
pub(super) fn generate(library: &str, name: &str) -> Option<Footprint> {
    if library != LIBRARY {
        return None;
    }
    PACKAGES
        .iter()
        .find(|p| p.name == name)
        .map(|p| land_grid(p, LIBRARY, p.name))
}

/// `gen:LGA(...)`: a ring of pads round the body, and a grid inside it when `grid` is given.
pub(super) fn from_params(p: &Params, library: &str, name: &str) -> Result<Footprint, String> {
    let edge = p.count_pair(
        "edge",
        "the pads on each top and bottom edge, and on each left and right edge",
        0,
        100,
    )?;
    if edge.1 == 0 {
        return Err(format!(
            "`edge={}x0` has no pads on the left and right edges; a package with two rows of pads \
             has them on the left and right, `edge=0x{}`",
            edge.0, edge.0
        ));
    }
    let pitch = p.length(
        "pitch",
        "the distance between the centres of ring pads",
        0.2,
        3.0,
    )?;
    let pad = p.length_pair(
        "pad",
        "a ring pad's width along its edge and its length towards the middle",
        0.05,
        5.0,
    )?;
    let body = p.length_pair("body", "the body across and along", 0.5, 60.0)?;
    let span = p
        .opt_length_pair(
            "span",
            "the distance between the centres of the left and right columns, and of the top and bottom rows",
            0.1,
            60.0,
        )?
        .unwrap_or((body.0 - pad.1, body.1 - pad.1));
    if pad.0 >= pitch {
        return Err(format!(
            "`pad={}x{}mm` is not narrower than the pitch of {pitch}mm: neighbouring pads would touch",
            pad.0, pad.1
        ));
    }
    let grid = match p.opt_count_pair("grid", "the pads inside the ring, across and down", 1, 60)? {
        None => {
            for key in ["grid_pitch", "grid_pad"] {
                if p.has(key) {
                    return Err(format!(
                        "`{key}` is given without `grid`: say how many pads the inner grid has, `grid=3x3`"
                    ));
                }
            }
            None
        }
        Some(count) => {
            let pitch = p.length_or_pair(
                "grid_pitch",
                "the distance between the centres of grid pads, across and down",
                0.2,
                10.0,
            )?;
            let pad = p.length_pair("grid_pad", "a grid pad across and along", 0.05, 10.0)?;
            Some(Grid { count, pitch, pad })
        }
    };
    let package = Lga {
        name: "",
        edge,
        pitch,
        pad,
        span,
        body,
        grid,
    };
    if let Some(grid) = grid {
        inside_ring(&package, grid)?;
    }
    Ok(land_grid(&package, library, name))
}

/// Refuse a grid that reaches past the ring's inner edges: the grid is the pads inside it.
fn inside_ring(p: &Lga, grid: Grid) -> Result<(), String> {
    let reach = |n: usize, pitch: f64, pad: f64| (n as f64 - 1.0) * pitch / 2.0 + pad / 2.0;
    let half = (
        reach(grid.count.0, grid.pitch.0, grid.pad.0),
        reach(grid.count.1, grid.pitch.1, grid.pad.1),
    );
    let inner_x = p.span.0 / 2.0 - p.pad.1 / 2.0;
    let inner_y = if p.edge.0 > 0 {
        p.span.1 / 2.0 - p.pad.1 / 2.0
    } else {
        p.body.1 / 2.0
    };
    if half.0 >= inner_x || half.1 >= inner_y {
        return Err(format!(
            "the grid of {}x{} pads is {}x{}mm, which does not fit inside the ring ({}x{}mm \
             between the inner edges of its pads)",
            grid.count.0,
            grid.count.1,
            round(2.0 * half.0),
            round(2.0 * half.1),
            round(2.0 * inner_x),
            round(2.0 * inner_y)
        ));
    }
    Ok(())
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// The centres of `n` pads `pitch` apart, centred on 0, first to last.
fn row(n: usize, pitch: f64) -> impl DoubleEndedIterator<Item = f64> {
    (0..n).map(move |i| (i as f64 - (n as f64 - 1.0) / 2.0) * pitch)
}

fn land_grid(p: &Lga, library: &str, name: &str) -> Footprint {
    let (across, down) = p.edge;
    let (width, length) = p.pad;
    let (cx, cy) = (p.span.0 / 2.0, p.span.1 / 2.0);
    let mut ring = Vec::new();
    let mut number = 0;
    let mut add = |at: Pt, size: (f64, f64)| {
        number += 1;
        ring.push(PadSpec::smd(number, at, size));
    };
    // Left edge, top to bottom.
    for y in row(down, p.pitch) {
        add(Pt::new(-cx, y), (length, width));
    }
    // Bottom edge, left to right.
    for x in row(across, p.pitch) {
        add(Pt::new(x, cy), (width, length));
    }
    // Right edge, bottom to top.
    for y in row(down, p.pitch).rev() {
        add(Pt::new(cx, y), (length, width));
    }
    // Top edge, right to left.
    for x in row(across, p.pitch).rev() {
        add(Pt::new(x, -cy), (width, length));
    }
    let mut inner = Vec::new();
    if let Some(grid) = p.grid {
        for y in row(grid.count.1, grid.pitch.1) {
            for x in row(grid.count.0, grid.pitch.0) {
                number += 1;
                inner.push(PadSpec::smd(number, Pt::new(x, y), grid.pad));
            }
        }
    }

    let mut drawing = Drawing::new(library, name, Mount::Smd);
    drawing.describe(
        &format!(
            "Land grid array, {} pads round the edge{}, the land pattern of the package drawing, generated by netsmith",
            ring.len(),
            if inner.is_empty() {
                String::new()
            } else {
                format!(" and {} in a grid inside", inner.len())
            }
        ),
        "LGA",
    );
    for pad in ring.iter().chain(&inner) {
        drawing.pad(pad);
    }
    let package = Package::NoLead {
        body: p.body,
        lead_width: width,
        lead_length: length,
        leads: super::leads_of(&ring, p.body),
        exposed: None,
    };
    let footprint = quad_frame(drawing, &ring, down, across, p.body, COURTYARD);
    super::shaped(footprint, package)
}

#[cfg(test)]
mod tests {
    use super::super::generate_spec;
    use super::*;
    use netsmith_kicad::FootprintSpec;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn made(spec: &str) -> Result<Footprint, String> {
        generate_spec(&FootprintSpec::parse(spec).unwrap())
    }

    #[test]
    fn the_ring_is_numbered_counterclockwise_from_the_top_of_the_left_edge() {
        let fp = generate(LIBRARY, "LGA-14_3x2.5mm_P0.5mm_LayoutBorder3x4y").unwrap();
        assert_eq!(fp.pads.len(), 14);
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        // The numbers of KiCad 9's footprint of this name.
        assert!(near(at("1").x, -1.1625) && near(at("1").y, -0.75));
        assert!(near(at("4").y, 0.75));
        assert!(near(at("5").x, -0.5) && near(at("5").y, 0.9125));
        assert!(near(at("8").x, 1.1625) && near(at("8").y, 0.75));
        assert!(near(at("12").x, 0.5) && near(at("12").y, -0.9125));
        assert!(near(at("14").x, -0.5));
        let left = fp.pads_numbered("1").next().unwrap();
        assert!(near(left.width, 0.625) && near(left.height, 0.35));
        let bottom = fp.pads_numbered("5").next().unwrap();
        assert!(near(bottom.width, 0.35) && near(bottom.height, 0.625));
        assert!((fp.courtyard.max.x - 1.75).abs() < 1e-9);
        assert!((fp.courtyard.max.y - 1.5).abs() < 1e-9);
    }

    #[test]
    fn a_named_row_and_gen_with_its_numbers_are_the_same_pads() {
        let by_name = generate(LIBRARY, "LGA-16_4x4mm_P0.65mm_LayoutBorder4x4y").unwrap();
        let by_params =
            made("gen:LGA(edge=4x4, pitch=0.65mm, pad=0.4x0.55mm, span=3.4x3.4mm, body=4x4mm)")
                .unwrap();
        assert_eq!(by_name.pads, by_params.pads);
        assert_eq!(by_name.courtyard, by_params.courtyard);
        // Without `span` the pads' outer ends are on the body's edge.
        let flush = made("gen:LGA(edge=4x4, pitch=0.65mm, pad=0.4x0.55mm, body=4x4mm)").unwrap();
        let pad = flush.pads_numbered("1").next().unwrap();
        assert!(near(pad.at.x - pad.width / 2.0, -2.0));
    }

    #[test]
    fn the_grid_is_numbered_after_the_ring_row_by_row_from_the_top_left() {
        let fp = made(
            "gen:LGA(edge=8x8, pitch=0.5mm, pad=0.25x0.5mm, body=5x5mm, grid=3x2, grid_pitch=1x1.2mm, grid_pad=0.6x0.5mm)",
        )
        .unwrap();
        assert_eq!(fp.pads.len(), 32 + 6);
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        assert!(near(at("33").x, -1.0) && near(at("33").y, -0.6));
        assert!(near(at("35").x, 1.0) && near(at("35").y, -0.6));
        assert!(near(at("36").x, -1.0) && near(at("36").y, 0.6));
        assert!(near(at("38").x, 1.0) && near(at("38").y, 0.6));
        let grid = fp.pads_numbered("38").next().unwrap();
        assert!(near(grid.width, 0.6) && near(grid.height, 0.5));
        // A grid pitch of one length is the pitch both ways.
        let square = made(
            "gen:LGA(edge=8x8, pitch=0.5mm, pad=0.25x0.5mm, body=5x5mm, grid=2x2, grid_pitch=1mm, grid_pad=0.5x0.5mm)",
        )
        .unwrap();
        let at = |n: &str| square.pads_numbered(n).next().unwrap().at;
        assert!(near(at("36").x, 0.5) && near(at("36").y, 0.5));
    }

    #[test]
    fn a_grid_that_does_not_fit_inside_the_ring_and_pads_too_wide_for_the_pitch_are_refused() {
        let e = made(
            "gen:LGA(edge=8x8, pitch=0.5mm, pad=0.25x0.5mm, body=5x5mm, grid=5x5, grid_pitch=1mm, grid_pad=0.5x0.5mm)",
        )
        .unwrap_err();
        assert!(e.contains("does not fit inside the ring"), "{e}");
        let e = made("gen:LGA(edge=4x4, pitch=0.5mm, pad=0.5x0.5mm, body=3x3mm)").unwrap_err();
        assert!(e.contains("not narrower than the pitch"), "{e}");
        let e =
            made("gen:LGA(edge=4x4, pitch=0.5mm, pad=0.25x0.5mm, body=3x3mm, grid_pad=0.5x0.5mm)")
                .unwrap_err();
        assert!(e.contains("without `grid`"), "{e}");
        let e = made("gen:LGA(edge=4x0, pitch=0.5mm, pad=0.25x0.5mm, body=3x3mm)").unwrap_err();
        assert!(e.contains("`edge=0x4`"), "{e}");
        let e = made("gen:LGA(edge=4x4, pitch=0.5mm, pad=0.25x0.5mm, body=3x3mm, grid=2x2, grid_pad=0.5x0.5mm)")
            .unwrap_err();
        assert!(e.contains("`grid_pitch` is missing"), "{e}");
        // Corner pads that meet.
        let e = made("gen:LGA(edge=6x6, pitch=0.5mm, pad=0.25x0.6mm, body=3x3mm)").unwrap_err();
        assert!(e.contains("overlap"), "{e}");
    }

    #[test]
    fn two_rows_only_are_on_the_left_and_right() {
        let fp = made("gen:LGA(edge=0x4, pitch=1.27mm, pad=0.6x1.2mm, body=5x5mm)").unwrap();
        assert_eq!(fp.pads.len(), 8);
        assert!(fp.pads.iter().all(|p| near(p.at.x.abs(), 1.9)));
    }
}
