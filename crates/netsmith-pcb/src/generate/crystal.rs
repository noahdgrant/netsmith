//! Surface-mount crystals with two or four pads, in ceramic packages (SMD3225 and the like).
//!
//! A crystal's pads are the ones the maker's drawing recommends; they are not a function of
//! lead dimensions that IPC-7351 describes. Each row records the pads and courtyard of the
//! footprint KiCad's library has under the same name (ADR 0043: numbers, which are facts
//! about those footprints) and the datasheet that footprint cites. Only the generic sizes
//! are here; KiCad's crystals named after one maker's part (`Crystal_SMD_Abracon_...`) and
//! the through-hole ones are read from KiCad's library as before.

use super::draw::{Draw, Drawing, Kind, PadSpec, Shape};
use super::outline::SILK_TO_COPPER;
use crate::body3d::Package;
use crate::footprint::{Footprint, Mount};
use crate::geometry::Pt;

const LIBRARY: &str = "Crystal";

/// One package.
struct Crystal {
    name: &'static str,
    /// The datasheet that the pads were taken from.
    source: &'static str,
    /// Body length (across the pads) and width, in mm.
    body: (f64, f64),
    /// Whether the pads have rounded corners.
    rounded: bool,
    /// The pads as (number, x, y, width, height).
    pads: &'static [(&'static str, f64, f64, f64, f64)],
    /// The courtyard as (min x, min y, max x, max y).
    courtyard: (f64, f64, f64, f64),
}

const PACKAGES: &[Crystal] = &[
    Crystal {
        name: "Crystal_SMD_0603-2Pin_6.0x3.5mm",
        source: "http://www.petermann-technik.de/fileadmin/petermann/pdf/SMD0603-2.pdf",
        body: (6.0, 3.5),
        rounded: false,
        pads: &[("1", -2.2, 0.0, 1.9, 2.5), ("2", 2.2, 0.0, 1.9, 2.5)],
        courtyard: (-3.4, -2.0, 3.4, 2.0),
    },
    Crystal {
        name: "Crystal_SMD_0603-2Pin_6.0x3.5mm_HandSoldering",
        source: "http://www.petermann-technik.de/fileadmin/petermann/pdf/SMD0603-2.pdf",
        body: (6.0, 3.5),
        rounded: false,
        pads: &[
            ("1", -2.9125, 0.0, 3.325, 2.5),
            ("2", 2.9125, 0.0, 3.325, 2.5),
        ],
        courtyard: (-4.9, -2.0, 4.9, 2.0),
    },
    Crystal {
        name: "Crystal_SMD_0603-4Pin_6.0x3.5mm",
        source: "http://www.petermann-technik.de/fileadmin/petermann/pdf/SMD0603-4.pdf",
        body: (6.0, 3.5),
        rounded: false,
        pads: &[
            ("1", -2.2, 1.2, 1.8, 1.4),
            ("2", 2.2, 1.2, 1.8, 1.4),
            ("3", 2.2, -1.2, 1.8, 1.4),
            ("4", -2.2, -1.2, 1.8, 1.4),
        ],
        courtyard: (-3.4, -2.2, 3.4, 2.2),
    },
    Crystal {
        name: "Crystal_SMD_0603-4Pin_6.0x3.5mm_HandSoldering",
        source: "http://www.petermann-technik.de/fileadmin/petermann/pdf/SMD0603-4.pdf",
        body: (6.0, 3.5),
        rounded: false,
        pads: &[
            ("1", -2.65, 1.55, 2.7, 2.1),
            ("2", 2.65, 1.55, 2.7, 2.1),
            ("3", 2.65, -1.55, 2.7, 2.1),
            ("4", -2.65, -1.55, 2.7, 2.1),
        ],
        courtyard: (-4.3, -2.9, 4.3, 2.9),
    },
    Crystal {
        name: "Crystal_SMD_1210-4Pin_1.2x1.0mm",
        source: "https://ecsxtal.com/store/pdf/ECX-1048.pdf",
        body: (1.2, 1.0),
        rounded: true,
        pads: &[
            ("1", -0.335, -0.41, 0.47, 0.52),
            ("2", -0.335, 0.41, 0.47, 0.52),
            ("3", 0.335, 0.41, 0.47, 0.52),
            ("4", 0.335, -0.41, 0.47, 0.52),
        ],
        courtyard: (-0.82, -0.92, 0.82, 0.92),
    },
    Crystal {
        name: "Crystal_SMD_2012-2Pin_2.0x1.2mm",
        source: "http://txccrystal.com/images/pdf/9ht11.pdf",
        body: (2.0, 1.2),
        rounded: false,
        pads: &[("1", -0.7, 0.0, 0.6, 1.1), ("2", 0.7, 0.0, 0.6, 1.1)],
        courtyard: (-1.3, -0.9, 1.3, 0.9),
    },
    Crystal {
        name: "Crystal_SMD_2012-2Pin_2.0x1.2mm_HandSoldering",
        source: "http://txccrystal.com/images/pdf/9ht11.pdf",
        body: (2.0, 1.2),
        rounded: false,
        pads: &[("1", -0.925, 0.0, 1.05, 1.1), ("2", 0.925, 0.0, 1.05, 1.1)],
        courtyard: (-1.7, -0.9, 1.7, 0.9),
    },
    Crystal {
        name: "Crystal_SMD_2016-4Pin_2.0x1.6mm",
        source: "http://www.q-crystal.com/upload/5/2015552223166229.pdf",
        body: (2.0, 1.6),
        rounded: false,
        pads: &[
            ("1", -0.7, 0.55, 0.9, 0.8),
            ("2", 0.7, 0.55, 0.9, 0.8),
            ("3", 0.7, -0.55, 0.9, 0.8),
            ("4", -0.7, -0.55, 0.9, 0.8),
        ],
        courtyard: (-1.4, -1.3, 1.4, 1.3),
    },
    Crystal {
        name: "Crystal_SMD_2520-4Pin_2.5x2.0mm",
        source: "http://www.newxtal.com/UploadFiles/Images/2012-11-12-09-29-09-776.pdf",
        body: (2.5, 2.0),
        rounded: false,
        pads: &[
            ("1", -0.875, 0.7, 1.15, 1.0),
            ("2", 0.875, 0.7, 1.15, 1.0),
            ("3", 0.875, -0.7, 1.15, 1.0),
            ("4", -0.875, -0.7, 1.15, 1.0),
        ],
        courtyard: (-1.7, -1.5, 1.7, 1.5),
    },
    Crystal {
        name: "Crystal_SMD_3215-2Pin_3.2x1.5mm",
        source: "https://support.epson.biz/td/api/doc_check.php?dl=brief_FC-135R_en.pdf",
        body: (3.2, 1.5),
        rounded: false,
        pads: &[("1", 1.25, 0.0, 1.0, 1.8), ("2", -1.25, 0.0, 1.0, 1.8)],
        courtyard: (-2.0, -1.15, 2.0, 1.15),
    },
    Crystal {
        name: "Crystal_SMD_3225-4Pin_3.2x2.5mm",
        source: "http://www.txccrystal.com/images/pdf/7m-accuracy.pdf",
        body: (3.2, 2.5),
        rounded: false,
        pads: &[
            ("1", -1.1, 0.85, 1.4, 1.2),
            ("2", 1.1, 0.85, 1.4, 1.2),
            ("3", 1.1, -0.85, 1.4, 1.2),
            ("4", -1.1, -0.85, 1.4, 1.2),
        ],
        courtyard: (-2.1, -1.7, 2.1, 1.7),
    },
    Crystal {
        name: "Crystal_SMD_3225-4Pin_3.2x2.5mm_HandSoldering",
        source: "http://www.txccrystal.com/images/pdf/7m-accuracy.pdf",
        body: (3.2, 2.5),
        rounded: false,
        pads: &[
            ("1", -1.45, 1.15, 2.1, 1.8),
            ("2", 1.45, 1.15, 2.1, 1.8),
            ("3", 1.45, -1.15, 2.1, 1.8),
            ("4", -1.45, -1.15, 2.1, 1.8),
        ],
        courtyard: (-2.8, -2.3, 2.8, 2.3),
    },
    Crystal {
        name: "Crystal_SMD_5032-2Pin_5.0x3.2mm",
        source: "http://www.icbase.com/File/PDF/HKC/HKC00061008.pdf",
        body: (5.0, 3.2),
        rounded: false,
        pads: &[("1", -1.85, 0.0, 2.0, 2.4), ("2", 1.85, 0.0, 2.0, 2.4)],
        courtyard: (-3.1, -1.9, 3.1, 1.9),
    },
    Crystal {
        name: "Crystal_SMD_5032-2Pin_5.0x3.2mm_HandSoldering",
        source: "http://www.icbase.com/File/PDF/HKC/HKC00061008.pdf",
        body: (5.0, 3.2),
        rounded: false,
        pads: &[("1", -2.6, 0.0, 3.5, 2.4), ("2", 2.6, 0.0, 3.5, 2.4)],
        courtyard: (-4.6, -1.9, 4.6, 1.9),
    },
    Crystal {
        name: "Crystal_SMD_5032-4Pin_5.0x3.2mm",
        source: "http://www.icbase.com/File/PDF/HKC/HKC00061008.pdf",
        body: (5.0, 3.2),
        rounded: false,
        pads: &[
            ("1", -1.65, 1.0, 1.6, 1.3),
            ("2", 1.65, 1.0, 1.6, 1.3),
            ("3", 1.65, -1.0, 1.6, 1.3),
            ("4", -1.65, -1.0, 1.6, 1.3),
        ],
        courtyard: (-2.8, -1.9, 2.8, 1.9),
    },
    Crystal {
        name: "Crystal_SMD_7050-2Pin_7.0x5.0mm",
        source: "https://www.foxonline.com/pdfs/FQ7050.pdf",
        body: (7.0, 5.0),
        rounded: false,
        pads: &[("1", -2.6, 0.0, 2.8, 3.0), ("2", 2.6, 0.0, 2.8, 3.0)],
        courtyard: (-4.3, -2.8, 4.3, 2.8),
    },
    Crystal {
        name: "Crystal_SMD_7050-2Pin_7.0x5.0mm_HandSoldering",
        source: "https://www.foxonline.com/pdfs/FQ7050.pdf",
        body: (7.0, 5.0),
        rounded: false,
        pads: &[("1", -3.65, 0.0, 4.9, 3.0), ("2", 3.65, 0.0, 4.9, 3.0)],
        courtyard: (-6.4, -2.8, 6.4, 2.8),
    },
    Crystal {
        name: "Crystal_SMD_7050-4Pin_7.0x5.0mm",
        source: "https://www.foxonline.com/pdfs/FQ7050.pdf",
        body: (7.0, 5.0),
        rounded: false,
        pads: &[
            ("1", -2.95, 1.35, 2.1, 1.7),
            ("2", 2.95, 1.35, 2.1, 1.7),
            ("3", 2.95, -1.35, 2.1, 1.7),
            ("4", -2.95, -1.35, 2.1, 1.7),
        ],
        courtyard: (-4.3, -2.8, 4.3, 2.8),
    },
];

/// Every `Library:Name` this module generates.
pub fn names() -> Vec<String> {
    PACKAGES
        .iter()
        .map(|c| format!("{LIBRARY}:{}", c.name))
        .collect()
}

/// The footprint named `library:name`, if it is a package of this module.
pub fn generate(library: &str, name: &str) -> Option<Footprint> {
    if library != LIBRARY {
        return None;
    }
    PACKAGES.iter().find(|c| c.name == name).map(crystal)
}

fn crystal(c: &Crystal) -> Footprint {
    let mut drawing = Drawing::new(LIBRARY, c.name, Mount::Smd);
    drawing.describe(
        &format!(
            "SMD crystal, {} pins, {} x {} mm body, pads as recorded from {}, generated by netsmith",
            c.pads.len(),
            c.body.0,
            c.body.1,
            c.source
        ),
        "SMD SMT crystal",
    );
    let shape = if c.rounded {
        Shape::RoundRect
    } else {
        Shape::Rect
    };
    // The innermost edge of the pads along x.
    let mut inner_x = f64::MAX;
    for &(number, x, y, w, h) in c.pads {
        drawing.pad(&PadSpec::new(
            number,
            Kind::Smd,
            shape,
            Pt::new(x, y),
            (w, h),
        ));
        inner_x = inner_x.min(x.abs() - w / 2.0);
    }
    let (min_x, min_y, max_x, max_y) = c.courtyard;
    drawing.courtyard(Pt::new(min_x, min_y), Pt::new(max_x, max_y));

    let (hx, hy) = (c.body.0 / 2.0, c.body.1 / 2.0);
    // The body, with the corner by pin 1 cut off.
    let cut = (hy / 2.0).min(0.5);
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
    drawing.fab_reference((c.body.1 * 0.25).clamp(0.3, 1.0));

    // Silkscreen along the body between the pads, where there is room for it.
    let reach = inner_x - SILK_TO_COPPER;
    let silk_y = hy + 0.11;
    if reach > 0.1 {
        for y in [-silk_y, silk_y] {
            drawing.line(Draw::Silk, Pt::new(-reach, y), Pt::new(reach, y));
        }
    }
    // A small triangle beside pad 1, on the side away from the body, pointing at it.
    if let Some(&(_, x, y, w, _)) = c.pads.first() {
        let side = if x < 0.0 { -1.0 } else { 1.0 };
        let apex = Pt::new(x + side * (w / 2.0 + SILK_TO_COPPER), y);
        drawing.polygon(
            Draw::Silk,
            &[
                apex,
                Pt::new(apex.x + side * 0.33, apex.y - 0.24),
                Pt::new(apex.x + side * 0.33, apex.y + 0.24),
            ],
            true,
        );
    }
    let top = -min_y.abs().max(max_y);
    super::shaped(
        drawing.finish(top - 0.7, -top + 0.7),
        Package::Crystal { body: c.body },
    )
}
