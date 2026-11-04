//! Ball grid arrays on a regular grid: KiCad's `Package_BGA` footprints named
//! `...-N_WxHmm_LayoutCxR_Ppitchmm` whose balls fill the whole grid.
//!
//! The pads are circles on the pitch, centred on the body, numbered by row letter and column
//! number from the top left (A1): the letters skip I, O, Q, S, X and Z as JEDEC does, and
//! continue with AA, AB, ... after Y. The courtyard is 1 mm outside the body. The diameter of
//! a ball's pad is a number from each package's drawing (the pad diameter in KiCad's name where
//! it has one) and is recorded in the table, as there is no rule for it: packages of one
//! pitch have pads from 0.2 to 0.5 mm. Only the footprints that this reproduces to 0.01 mm
//! are generated; a grid with balls left out, rows or columns with two pitches, and bodies
//! whose courtyard KiCad draws by another rule are read from KiCad's library (ADR 0043).

use super::draw::{Draw, Drawing, Kind, PadSpec, Shape};
use super::ipc::round_to;
use crate::footprint::{Footprint, Mount};
use crate::geometry::Pt;

const LIBRARY: &str = "Package_BGA";
/// The courtyard's distance from the body, in mm.
const COURTYARD: f64 = 1.0;

struct Bga {
    name: &'static str,
    columns: usize,
    rows: usize,
    pitch: f64,
    /// The body across the columns (x) and along them (y).
    body: (f64, f64),
    /// The diameter of a ball's pad.
    pad: f64,
}

const fn bga(
    name: &'static str,
    columns: usize,
    rows: usize,
    pitch: f64,
    body: (f64, f64),
    pad: f64,
) -> Bga {
    Bga {
        name,
        columns,
        rows,
        pitch,
        body,
        pad,
    }
}

#[rustfmt::skip]
const PACKAGES: &[Bga] = &[
    bga("Analog_BGA-165_11.9x16mm_Layout11x15_P1.0mm", 11, 15, 1.0, (11.9, 16.0), 0.5),
    bga("Analog_BGA-209_9.5x16mm_Layout11x19_P0.8mm", 11, 19, 0.8, (9.5, 16.0), 0.4),
    bga("Analog_BGA-28_4x6.25mm_Layout4x7_P0.8mm", 4, 7, 0.8, (4.0, 6.25), 0.4),
    bga("Analog_BGA-49_6.25x6.25mm_Layout7x7_P0.8mm", 7, 7, 0.8, (6.25, 6.25), 0.4),
    bga("Analog_BGA-77_9x15mm_Layout7x11_P1.27mm", 7, 11, 1.27, (9.0, 15.0), 0.63),
    bga("BGA-100_11.0x11.0mm_Layout10x10_P1.0mm_Ball0.5mm_Pad0.4mm_NSMD", 10, 10, 1.0, (11.0, 11.0), 0.4),
    bga("BGA-1156_35.0x35.0mm_Layout34x34_P1.0mm", 34, 34, 1.0, (35.0, 35.0), 0.5),
    bga("BGA-121_9.0x9.0mm_Layout11x11_P0.8mm_Ball0.4mm_Pad0.35mm_NSMD", 11, 11, 0.8, (9.0, 9.0), 0.35),
    bga("BGA-144_13.0x13.0mm_Layout12x12_P1.0mm", 12, 12, 1.0, (13.0, 13.0), 0.5),
    bga("BGA-169_11.0x11.0mm_Layout13x13_P0.8mm_Ball0.5mm_Pad0.4mm_NSMD", 13, 13, 0.8, (11.0, 11.0), 0.4),
    bga("BGA-16_1.92x1.92mm_Layout4x4_P0.5mm", 4, 4, 0.5, (1.92, 1.92), 0.25),
    bga("BGA-196_15x15mm_Layout14x14_P1.0mm", 14, 14, 1.0, (15.0, 15.0), 0.45),
    bga("BGA-256_14.0x14.0mm_Layout16x16_P0.8mm_Ball0.45mm_Pad0.32mm_NSMD", 16, 16, 0.8, (14.0, 14.0), 0.32),
    bga("BGA-256_17.0x17.0mm_Layout16x16_P1.0mm_Ball0.5mm_Pad0.4mm_NSMD", 16, 16, 1.0, (17.0, 17.0), 0.4),
    bga("BGA-324_15.0x15.0mm_Layout18x18_P0.8mm_Ball0.5mm_Pad0.4mm_NSMD", 18, 18, 0.8, (15.0, 15.0), 0.4),
    bga("BGA-324_15x15mm_Layout18x18_P0.8mm", 18, 18, 0.8, (15.0, 15.0), 0.4),
    bga("BGA-324_19.0x19.0mm_Layout18x18_P1.0mm_Ball0.5mm_Pad0.4mm_NSMD", 18, 18, 1.0, (19.0, 19.0), 0.4),
    bga("BGA-400_21.0x21.0mm_Layout20x20_P1.0mm", 20, 20, 1.0, (21.0, 21.0), 0.5),
    bga("BGA-484_23.0x23.0mm_Layout22x22_P1.0mm", 22, 22, 1.0, (23.0, 23.0), 0.5),
    bga("BGA-48_8.0x9.0mm_Layout6x8_P0.8mm", 6, 8, 0.8, (8.0, 9.0), 0.4),
    bga("BGA-529_19x19mm_Layout23x23_P0.8mm", 23, 23, 0.8, (19.0, 19.0), 0.45),
    bga("BGA-625_21.0x21.0mm_Layout25x25_P0.8mm", 25, 25, 0.8, (21.0, 21.0), 0.4),
    bga("BGA-676_27.0x27.0mm_Layout26x26_P1.0mm_Ball0.6mm_Pad0.5mm_NSMD", 26, 26, 1.0, (27.0, 27.0), 0.5),
    bga("BGA-81_4.496x4.377mm_Layout9x9_P0.4mm_Ball0.25mm_Pad0.2mm_NSMD", 9, 9, 0.4, (4.496, 4.377), 0.2),
    bga("BGA-9_1.6x1.6mm_Layout3x3_P0.5mm", 3, 3, 0.5, (1.6, 1.6), 0.25),
    bga("FB-BGA-484_23.0x23.0mm_Layout22x22_P1.0mm", 22, 22, 1.0, (23.0, 23.0), 0.4),
    bga("Infineon_TFBGA-48_6x10mm_Layout6x8_P0.75mm", 6, 8, 0.75, (6.0, 10.0), 0.25),
    bga("LFBGA-100_10x10mm_Layout10x10_P0.8mm", 10, 10, 0.8, (10.0, 10.0), 0.5),
    bga("LFBGA-144_10x10mm_Layout12x12_P0.8mm", 12, 12, 0.8, (10.0, 10.0), 0.4),
    bga("LFBGA-289_14x14mm_Layout17x17_P0.8mm", 17, 17, 0.8, (14.0, 14.0), 0.32),
    bga("LFBGA-400_16x16mm_Layout20x20_P0.8mm", 20, 20, 0.8, (16.0, 16.0), 0.35),
    bga("LFBGA-484_18x18mm_Layout22x22_P0.8mm", 22, 22, 0.8, (18.0, 18.0), 0.35),
    bga("MAPBGA-289_14x14mm_Layout17x17_P0.8mm", 17, 17, 0.8, (14.0, 14.0), 0.35),
    bga("Maxim_WLP-12_2.008x1.608mm_Layout4x3_P0.4mm", 4, 3, 0.4, (2.008, 1.608), 0.25),
    bga("Microchip_TFBGA-196_11x11mm_Layout14x14_P0.75mm_SMD", 14, 14, 0.75, (11.0, 11.0), 0.35),
    bga("NXP_VFBGA-42_2.6x3mm_Layout6x7_P0.4mm", 6, 7, 0.4, (2.6, 3.0), 0.24),
    bga("ST_TFBGA-169_7x7mm_Layout13x13_P0.5mm", 13, 13, 0.5, (7.0, 7.0), 0.3),
    bga("ST_TFBGA-225_13x13mm_Layout15x15_P0.8mm", 15, 15, 0.8, (13.0, 13.0), 0.4),
    bga("ST_UFBGA-121_6x6mm_Layout11x11_P0.5mm", 11, 11, 0.5, (6.0, 6.0), 0.225),
    bga("ST_UFBGA-81_5x5mm_Layout9x9_P0.5mm", 9, 9, 0.5, (5.0, 5.0), 0.25),
    bga("ST_uTFBGA-36_3.6x3.6mm_Layout6x6_P0.5mm", 6, 6, 0.5, (3.6, 3.6), 0.25),
    bga("TFBGA-100_5.5x5.5mm_Layout10x10_P0.5mm", 10, 10, 0.5, (5.5, 5.5), 0.2),
    bga("TFBGA-100_8x8mm_Layout10x10_P0.8mm", 10, 10, 0.8, (8.0, 8.0), 0.4),
    bga("TFBGA-100_9.0x9.0mm_Layout10x10_P0.8mm", 10, 10, 0.8, (9.0, 9.0), 0.35),
    bga("TFBGA-121_10x10mm_Layout11x11_P0.8mm", 11, 11, 0.8, (10.0, 10.0), 0.4),
    bga("TFBGA-169_9x9mm_Layout13x13_P0.65mm", 13, 13, 0.65, (9.0, 9.0), 0.2),
    bga("TFBGA-225_10x10mm_Layout15x15_P0.65mm", 15, 15, 0.65, (10.0, 10.0), 0.33),
    bga("TFBGA-256_13x13mm_Layout16x16_P0.8mm", 16, 16, 0.8, (13.0, 13.0), 0.35),
    bga("TFBGA-289_9x9mm_Layout17x17_P0.5mm", 17, 17, 0.5, (9.0, 9.0), 0.23),
    bga("TFBGA-324_12x12mm_Layout18x18_P0.65mm", 18, 18, 0.65, (12.0, 12.0), 0.25),
    bga("TFBGA-361_13x13mm_Layout19x19_P0.65mm", 19, 19, 0.65, (13.0, 13.0), 0.33),
    bga("TFBGA-48_6x10mm_Layout6x8_P0.75mm", 6, 8, 0.75, (6.0, 10.0), 0.4),
    bga("TFBGA-49_3x3mm_Layout7x7_P0.4mm", 7, 7, 0.4, (3.0, 3.0), 0.2),
    bga("TFBGA-576_16x16mm_Layout24x24_P0.65mm", 24, 24, 0.65, (16.0, 16.0), 0.25),
    bga("TFBGA-64_5x5mm_Layout8x8_P0.5mm", 8, 8, 0.5, (5.0, 5.0), 0.28),
    bga("TFBGA-81_5x5mm_Layout9x9_P0.5mm", 9, 9, 0.5, (5.0, 5.0), 0.2),
    bga("Texas_BGA-289_15x15mm_Layout17x17_P0.8mm", 17, 17, 0.8, (15.0, 15.0), 0.4),
    bga("Texas_DSBGA-28_1.9x3mm_Layout4x7_P0.4mm", 4, 7, 0.4, (1.9, 3.0), 0.225),
    bga("Texas_DSBGA-49_3.33x3.488mm_Layout7x7_P0.4mm", 7, 7, 0.4, (3.33, 3.488), 0.225),
    bga("Texas_DSBGA-64_3.415x3.535mm_Layout8x8_P0.4mm", 8, 8, 0.4, (3.415, 3.535), 0.225),
    bga("Texas_DSBGA-6_0.704x1.054mm_Layout2x3_P0.35mm", 2, 3, 0.35, (0.704, 1.054), 0.2),
    bga("Texas_DSBGA-6_0.95x1.488mm_Layout2x3_P0.4mm", 2, 3, 0.4, (0.95, 1.488), 0.25),
    bga("Texas_DSBGA-8_0.705x1.468mm_Layout2x4_P0.4mm", 2, 4, 0.4, (0.705, 1.468), 0.2),
    bga("Texas_DSBGA-9_1.62x1.58mm_Layout3x3_P0.5mm", 3, 3, 0.5, (1.62, 1.58), 0.245),
    bga("Texas_PicoStar_BGA-4_0.758x0.758mm_Layout2x2_P0.4mm", 2, 2, 0.4, (0.758, 0.758), 0.23),
    bga("Texas_YFP0020_DSBGA-20_1.588x1.988mm_Layout4x5_P0.4mm", 4, 5, 0.4, (1.588, 1.988), 0.23),
    bga("UCBGA-36_2.5x2.5mm_Layout6x6_P0.4mm", 6, 6, 0.4, (2.5, 2.5), 0.27),
    bga("UCBGA-49_3x3mm_Layout7x7_P0.4mm", 7, 7, 0.4, (3.0, 3.0), 0.27),
    bga("UCBGA-81_4x4mm_Layout9x9_P0.4mm", 9, 9, 0.4, (4.0, 4.0), 0.27),
    bga("UFBGA-144_10x10mm_Layout12x12_P0.8mm", 12, 12, 0.8, (10.0, 10.0), 0.4),
    bga("UFBGA-144_7x7mm_Layout12x12_P0.5mm", 12, 12, 0.5, (7.0, 7.0), 0.28),
    bga("UFBGA-169_7x7mm_Layout13x13_P0.5mm", 13, 13, 0.5, (7.0, 7.0), 0.27),
    bga("UFBGA-64_5x5mm_Layout8x8_P0.5mm", 8, 8, 0.5, (5.0, 5.0), 0.28),
    bga("VFBGA-100_7.0x7.0mm_Layout10x10_P0.65mm", 10, 10, 0.65, (7.0, 7.0), 0.3),
    bga("VFBGA-49_5.0x5.0mm_Layout7x7_P0.65mm", 7, 7, 0.65, (5.0, 5.0), 0.3),
    bga("WLP-4_0.728x0.728mm_Layout2x2_P0.35mm", 2, 2, 0.35, (0.728, 0.728), 0.2),
    bga("WLP-9_1.468x1.448mm_Layout3x3_P0.4mm", 3, 3, 0.4, (1.468, 1.448), 0.27),
    bga("XBGA-121_10x10mm_Layout11x11_P0.8mm", 11, 11, 0.8, (10.0, 10.0), 0.32),
    bga("XFBGA-64_5.0x5.0mm_Layout8x8_P0.5mm", 8, 8, 0.5, (5.0, 5.0), 0.35),
];

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
    PACKAGES.iter().find(|p| p.name == name).map(grid)
}

/// The letters of a ball row, from the top: A..Y without I, O, Q, S, X, Z, then AA, AB, ...
fn row_label(index: usize) -> String {
    const LETTERS: &[u8] = b"ABCDEFGHJKLMNPRTUVWY";
    let n = LETTERS.len();
    if index < n {
        (LETTERS[index] as char).to_string()
    } else {
        format!(
            "{}{}",
            LETTERS[index / n - 1] as char,
            LETTERS[index % n] as char
        )
    }
}

fn grid(p: &Bga) -> Footprint {
    let mut drawing = Drawing::new(LIBRARY, p.name, Mount::Smd);
    drawing.describe(
        &format!(
            "BGA, {} balls on a {}x{} grid, {} mm pitch, {} x {} mm body, generated by netsmith",
            p.columns * p.rows,
            p.columns,
            p.rows,
            super::draw::num(p.pitch),
            super::draw::num(p.body.0),
            super::draw::num(p.body.1)
        ),
        "BGA",
    );
    let span = |n: usize| (n as f64 - 1.0) / 2.0 * p.pitch;
    for row in 0..p.rows {
        for column in 0..p.columns {
            drawing.pad(&PadSpec::new(
                format!("{}{}", row_label(row), column + 1),
                Kind::Smd,
                Shape::Circle,
                Pt::new(
                    -span(p.columns) + column as f64 * p.pitch,
                    -span(p.rows) + row as f64 * p.pitch,
                ),
                (p.pad, p.pad),
            ));
        }
    }

    let (hx, hy) = (p.body.0 / 2.0, p.body.1 / 2.0);
    let court = (
        round_to(hx + COURTYARD, 0.01),
        round_to(hy + COURTYARD, 0.01),
    );
    drawing.courtyard(Pt::new(-court.0, -court.1), Pt::new(court.0, court.1));

    // The body with the corner by ball A1 cut off.
    let cut = (p.body.0.min(p.body.1) / 4.0).min(1.0);
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
    drawing.fab_reference((p.body.0.min(p.body.1) * 0.25).clamp(0.3, 1.0));

    // The silkscreen is the body's outline, with a marker outside the corner of A1.
    let (sx, sy) = (hx + 0.11, hy + 0.11);
    let notch = cut.min(sx - 0.2);
    for (a, b) in [
        (Pt::new(-sx + notch, -sy), Pt::new(sx, -sy)),
        (Pt::new(sx, -sy), Pt::new(sx, sy)),
        (Pt::new(sx, sy), Pt::new(-sx, sy)),
        (Pt::new(-sx, sy), Pt::new(-sx, -sy + notch)),
    ] {
        drawing.line(Draw::Silk, a, b);
    }
    drawing.line(
        Draw::Silk,
        Pt::new(-sx + notch, -sy),
        Pt::new(-sx, -sy + notch),
    );
    drawing.finish(-(court.1 + 0.7), court.1 + 0.7)
}
