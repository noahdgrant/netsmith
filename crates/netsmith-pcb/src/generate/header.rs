//! Pin headers and sockets, one or two rows, vertical, through hole, at 2.54 mm and 2.0 mm
//! pitch (`PinHeader_1x04_P2.54mm_Vertical`, `PinSocket_2x05_P2.00mm_Vertical`).
//!
//! The land is the one every maker of these parts recommends: a round pad of 0.7 of the
//! pitch (1.7 mm at 2.54, 1.35 mm at 2.0) round a hole that the 0.64 mm (0.5 mm) square pin
//! fits with room to spare (1.0 mm, 0.8 mm), pin 1 square. A header's second row is on the
//! side of increasing x, a socket's on the other side, because a socket is seen from the
//! mating face. The courtyard is the body plus 0.5 mm, rounded outwards to 0.05 mm.

use super::draw::{Draw, Drawing, Kind, PadSpec, Shape};
use super::params::Params;
use crate::body3d::Package;
use crate::footprint::{Footprint, Mount};
use crate::geometry::{Pt, Rect};

/// The most pins in a row that are generated.
const MOST_PER_ROW: usize = 40;

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Header,
    Socket,
}

impl Part {
    fn word(self) -> &'static str {
        match self {
            Part::Header => "PinHeader",
            Part::Socket => "PinSocket",
        }
    }
}

/// A pitch and its pad, hole and square pin.
struct Pitch {
    text: &'static str,
    mm: f64,
    pad: f64,
    drill: f64,
    /// The side of the square pin, which the hole fits with room to spare (the header and
    /// socket makers' drawings: 0.64 mm at 2.54 mm, 0.5 mm at 2.0 mm). Only the 3D view
    /// reads it.
    pin: f64,
    /// The height of a header's plastic above the board, nominal, in mm: the same for one row
    /// and two, from a maker's drawing of each (`data/jlcpcb/README.md`, "Pin headers", has
    /// the documents). Only the 3D view reads it, through [`Package::Header`]; a socket's
    /// plastic is its whole height, so a socket does not read it. It is not the part's height:
    /// that is the pin's tip above the plastic, a part's stated `.height` or its catalog row's.
    plastic: f64,
}

const PITCHES: [Pitch; 2] = [
    Pitch {
        text: "2.54",
        mm: 2.54,
        pad: 1.7,
        drill: 1.0,
        pin: 0.64,
        // 2.50 mm (general tolerance +/-0.20): JXTCONN PH2.54-1XnP-H25 (one row) and
        // A254M01-0BxP51-H25A60 (two rows), Hanxia HX PZ2.54-1x8P ZZ (one row).
        plastic: 2.5,
    },
    Pitch {
        text: "2.00",
        mm: 2.0,
        pad: 1.35,
        drill: 0.8,
        pin: 0.5,
        // 2.00 mm (+/-0.20): JXTCONN PZ2.0V-11nP-H20 (one row), XFCN PZ200V-12-XX-0870 (two).
        plastic: 2.0,
    },
];

/// The square pin and the plastic of a header (`(pin, plastic)`, in mm) of the family at
/// `pitch` mm, the figures the generated footprints carry, for a footprint read from a file
/// that [`crate::body3d`] draws as a header. `None` at a pitch with no family here.
pub fn family(pitch: f64) -> Option<(f64, f64)> {
    PITCHES
        .iter()
        .find(|k| (k.mm - pitch).abs() < 1e-9)
        .map(|k| (k.pin, k.plastic))
}

/// The pitch, in mm, a footprint name spells as `text` (`2.54`, `2.00`), if a family has it.
pub fn pitch_of(text: &str) -> Option<f64> {
    PITCHES.iter().find(|k| k.text == text).map(|k| k.mm)
}

fn library(part: Part, pitch: &Pitch) -> String {
    format!("Connector_{}_{}mm", part.word(), pitch.text)
}

fn name(part: Part, rows: usize, per_row: usize, pitch: &Pitch) -> String {
    format!(
        "{}_{rows}x{per_row:02}_P{}mm_Vertical",
        part.word(),
        pitch.text
    )
}

/// Every `Library:Name` this module generates.
pub fn names() -> Vec<String> {
    let mut out = Vec::new();
    for part in [Part::Header, Part::Socket] {
        for pitch in &PITCHES {
            for rows in [1, 2] {
                for per_row in 1..=MOST_PER_ROW {
                    out.push(format!(
                        "{}:{}",
                        library(part, pitch),
                        name(part, rows, per_row, pitch)
                    ));
                }
            }
        }
    }
    out
}

/// The footprint named `library:name`, if it is a header or a socket of this module.
pub fn generate(library_name: &str, footprint: &str) -> Option<Footprint> {
    for part in [Part::Header, Part::Socket] {
        for pitch in &PITCHES {
            if library(part, pitch) != library_name {
                continue;
            }
            let rest = footprint
                .strip_prefix(part.word())?
                .strip_prefix('_')?
                .strip_suffix(&format!("_P{}mm_Vertical", pitch.text))?;
            let (rows, per_row) = rest.split_once('x')?;
            let rows: usize = rows.parse().ok().filter(|r| [1, 2].contains(r))?;
            let per_row: usize = per_row
                .parse()
                .ok()
                .filter(|n| (1..=MOST_PER_ROW).contains(n))?;
            // The name has the row length with two digits, and no other spelling is accepted.
            if name(part, rows, per_row, pitch) != footprint {
                return None;
            }
            return Some(header(
                part,
                rows,
                per_row,
                pitch,
                &library(part, pitch),
                footprint,
            ));
        }
    }
    None
}

/// `gen:PinHeader(...)` and `gen:PinSocket(...)`: the same land as the named ones, for the
/// pitches that have one (2.54 mm and 2 mm).
pub(super) fn from_params(
    p: &Params,
    part: &str,
    library: &str,
    name: &str,
) -> Result<Footprint, String> {
    let part = if part == "PinSocket" {
        Part::Socket
    } else {
        Part::Header
    };
    let rows = p.count("rows", "the number of rows, 1 or 2", 1, 2)?;
    let per_row = p.count("pins", "the number of pins in a row", 1, MOST_PER_ROW)?;
    let pitch = p.length(
        "pitch",
        "the distance between pins, 2.54mm or 2mm",
        1.0,
        3.0,
    )?;
    let pitch = PITCHES
        .iter()
        .find(|k| (k.mm - pitch).abs() < 1e-9)
        .ok_or_else(|| {
            format!("`pitch={pitch}mm` has no land netsmith knows: 2.54mm and 2mm only")
        })?;
    Ok(header(part, rows, per_row, pitch, library, name))
}

/// Round `v` away from zero to a multiple of 0.05.
fn outward(v: f64) -> f64 {
    let steps = (v.abs() / 0.05 - 1e-9).ceil();
    v.signum() * steps * 0.05
}

fn header(
    part: Part,
    rows: usize,
    per_row: usize,
    pitch: &Pitch,
    library: &str,
    id: &str,
) -> Footprint {
    let mut drawing = Drawing::new(library, id, Mount::ThroughHole);
    drawing.describe(
        &format!(
            "Through hole {} {}x{:02}, {} mm pitch, vertical, generated by netsmith",
            match part {
                Part::Header => "pin header",
                Part::Socket => "pin socket",
            },
            rows,
            per_row,
            pitch.text
        ),
        match part {
            Part::Header => "Through hole pin header THT",
            Part::Socket => "Through hole socket THT",
        },
    );
    // Pins are numbered along the rows: 1 and 2 are side by side in a two row part.
    let direction = if part == Part::Header { 1.0 } else { -1.0 };
    let mut pins = Vec::with_capacity(rows * per_row);
    for row in 0..per_row {
        for column in 0..rows {
            let number = row * rows + column + 1;
            let shape = if number == 1 {
                Shape::Rect
            } else {
                Shape::Oval
            };
            let at = Pt::new(direction * column as f64 * pitch.mm, row as f64 * pitch.mm);
            pins.push(at);
            drawing.pad(&PadSpec::new(
                number,
                Kind::Through(pitch.drill),
                shape,
                at,
                (pitch.pad, pitch.pad),
            ));
        }
    }

    // The body: the width of the pitch per row, the length of the pitch per pin.
    let half = pitch.mm / 2.0;
    let (x0, x1) = if direction > 0.0 {
        (-half, (rows as f64 - 1.0) * pitch.mm + half)
    } else {
        (-(rows as f64 - 1.0) * pitch.mm - half, half)
    };
    let (y0, y1) = (-half, (per_row as f64 - 1.0) * pitch.mm + half);
    drawing.courtyard(
        Pt::new(-outward(-(x0 - 0.5)), -outward(-(y0 - 0.5))),
        Pt::new(outward(x1 + 0.5), outward(y1 + 0.5)),
    );

    // Pin 1's corner is cut off in the drawing of the body.
    let cut = pitch.mm / 4.0;
    drawing.polygon(
        Draw::Fab,
        &[
            Pt::new(x0 + cut, y0),
            Pt::new(x1, y0),
            Pt::new(x1, y1),
            Pt::new(x0, y1),
            Pt::new(x0, y0 + cut),
        ],
        false,
    );
    drawing.fab_reference(1.0);

    // Silkscreen round the body, clear of the pads, and a triangle that points at pin 1.
    let out = 0.06;
    drawing.rect(
        Draw::Silk,
        Pt::new(x0 - out, y0 - out),
        Pt::new(x1 + out, y1 + out),
    );
    let apex = Pt::new(x0 - out - 0.12, 0.0);
    drawing.polygon(
        Draw::Silk,
        &[
            apex,
            Pt::new(apex.x - 0.33, apex.y - 0.24),
            Pt::new(apex.x - 0.33, apex.y + 0.24),
        ],
        true,
    );

    let top = outward(y0 - 0.5);
    super::shaped(
        drawing.finish(top - 0.53, outward(y1 + 0.5) + 0.52),
        Package::Header {
            body: Rect::new(Pt::new(x0, y0), Pt::new(x1, y1)),
            pitch: pitch.mm,
            pin: pitch.pin,
            plastic: pitch.plastic,
            pins,
            socket: part == Part::Socket,
        },
    )
}
