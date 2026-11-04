//! Packages with a large tab: DPAK (TO-252), D2PAK (TO-263) and SOT-89.
//!
//! The tab is the heat path and often a pin, and its pad is not a function of the lead
//! dimensions, so the pads here are the ones the manufacturers' drawings recommend, as
//! KiCad's library gives them (the same basis as SOT-223, ADR 0043): DPAK and D2PAK from
//! Infineon's PG-TO252 and PG-TO263 drawings, SOT-89-3 from Microchip's 3L SOT-89 drawing and SOT-89-5 from Ricoh's.
//! The numbers are facts about those footprints, recorded in the tables below.

use super::draw::{Draw, Drawing, Kind, PadSpec, Shape};
use super::outline::SILK_TO_COPPER;
use crate::footprint::{Footprint, Mount};
use crate::geometry::Pt;

const LIBRARY: &str = "Package_TO_SOT_SMD";

/// The courtyard's distance from the copper and the body, in mm.
const COURTYARD: f64 = 0.25;

/// The land pattern shared by the variants of one tab package (DPAK, D2PAK): where the
/// leads' pads are, the tab's pad and the four paste openings on it, and the body.
struct TabPackage {
    description: &'static str,
    tags: &'static str,
    /// Centre of the leads' pads, and their length.
    lead_x: f64,
    lead_length: f64,
    /// The tab pad in copper and mask: centre and size.
    tab: (f64, f64, f64, f64),
    /// Four openings of the paste on the tab: x of the two columns, y of the two rows, size.
    paste_x: [f64; 2],
    paste_y: f64,
    paste: (f64, f64),
    /// The body: along the leads, across, and how far the tab's stub is drawn in from the
    /// body's edges.
    body: (f64, f64),
    stub_inset: f64,
}

/// DPAK (PG-TO252).
const TO252: TabPackage = TabPackage {
    description: "TO-252/DPAK SMD package (Infineon PG-TO252)",
    tags: "TO-252 DPAK TO_SOT_SMD",
    lead_x: -5.04,
    lead_length: 2.2,
    tab: (1.26, 0.0, 6.4, 5.8),
    paste_x: [-0.415, 2.935],
    paste_y: 1.525,
    paste: (3.05, 2.75),
    body: (6.22, 6.5),
    stub_inset: 0.55,
};

/// D2PAK (PG-TO263): the body is 9.25 mm along the leads and 10 mm across.
const TO263: TabPackage = TabPackage {
    description: "TO-263/D2PAK/DDPAK SMD package (Infineon PG-TO263)",
    tags: "TO-263 D2PAK DDPAK TO_SOT_SMD",
    lead_x: -7.65,
    lead_length: 4.6,
    tab: (1.5, 0.0, 9.4, 10.8),
    paste_x: [-0.925, 3.925],
    paste_y: 2.775,
    paste: (4.55, 5.25),
    body: (9.25, 10.0),
    stub_inset: 0.0,
};

/// A variant of a tab package: the leads in a column at the left as (number, y), the
/// number of the tab, and the height of a lead's pad.
struct Dpak {
    name: &'static str,
    package: &'static TabPackage,
    leads: &'static [(&'static str, f64)],
    tab: &'static str,
    lead_height: f64,
    /// Whether the four paste openings are copper pads of the tab's number (KiCad's DPAK
    /// and most D2PAKs) or stencil openings that are not pads (its nine-pin D2PAKs).
    paste_numbered: bool,
}

const THREE: [(&str, f64); 3] = [("1", -2.28), ("2", 0.0), ("3", 2.28)];
const FIVE: [(&str, f64); 5] = [
    ("1", -2.28),
    ("2", -1.14),
    ("3", 0.0),
    ("4", 1.14),
    ("5", 2.28),
];

const D2_THREE: [(&str, f64); 3] = [("1", -2.54), ("2", 0.0), ("3", 2.54)];
const D2_FIVE: [(&str, f64); 5] = [("1", -3.4), ("2", -1.7), ("3", 0.0), ("4", 1.7), ("5", 3.4)];
const D2_SEVEN: [(&str, f64); 7] = [
    ("1", -3.81),
    ("2", -2.54),
    ("3", -1.27),
    ("4", 0.0),
    ("5", 1.27),
    ("6", 2.54),
    ("7", 3.81),
];
const D2_SIX: [(&str, f64); 6] = [
    ("1", -3.81),
    ("2", -2.54),
    ("3", -1.27),
    ("5", 1.27),
    ("6", 2.54),
    ("7", 3.81),
];
const D2_NINE: [(&str, f64); 9] = [
    ("1", -3.88),
    ("2", -2.91),
    ("3", -1.94),
    ("4", -0.97),
    ("5", 0.0),
    ("6", 0.97),
    ("7", 1.94),
    ("8", 2.91),
    ("9", 3.88),
];

const fn to252(
    name: &'static str,
    leads: &'static [(&'static str, f64)],
    tab: &'static str,
    lead_height: f64,
) -> Dpak {
    Dpak {
        name,
        package: &TO252,
        leads,
        tab,
        lead_height,
        paste_numbered: true,
    }
}

const fn to263(
    name: &'static str,
    leads: &'static [(&'static str, f64)],
    tab: &'static str,
    lead_height: f64,
    paste_numbered: bool,
) -> Dpak {
    Dpak {
        name,
        package: &TO263,
        leads,
        tab,
        lead_height,
        paste_numbered,
    }
}

const DPAK: &[Dpak] = &[
    to252("TO-252-2", &[("1", -2.28), ("3", 2.28)], "2", 1.2),
    to252("TO-252-3_TabPin2", &THREE, "2", 1.2),
    to252("TO-252-3_TabPin4", &THREE, "4", 1.2),
    to252(
        "TO-252-4",
        &[("1", -2.28), ("2", -1.14), ("4", 1.14), ("5", 2.28)],
        "3",
        0.8,
    ),
    to252("TO-252-5_TabPin3", &FIVE, "3", 0.8),
    to252("TO-252-5_TabPin6", &FIVE, "6", 0.8),
    to263("TO-263-2", &[("1", -2.54), ("3", 2.54)], "2", 1.1, true),
    to263("TO-263-3_TabPin2", &D2_THREE, "2", 1.1, true),
    to263("TO-263-3_TabPin4", &D2_THREE, "4", 1.1, true),
    to263(
        "TO-263-4",
        &[("1", -3.4), ("2", -1.7), ("4", 1.7), ("5", 3.4)],
        "3",
        1.1,
        true,
    ),
    to263("TO-263-5_TabPin3", &D2_FIVE, "3", 1.1, true),
    to263("TO-263-5_TabPin6", &D2_FIVE, "6", 1.1, true),
    to263("TO-263-6", &D2_SIX, "4", 0.8, true),
    to263("TO-263-7_TabPin4", &D2_SEVEN, "4", 0.8, true),
    to263("TO-263-7_TabPin8", &D2_SEVEN, "8", 0.8, true),
    to263("TO-263-9_TabPin10", &D2_NINE, "10", 0.61, false),
    to263("TO-263-9_TabPin5", &D2_NINE, "5", 0.61, false),
];

/// A SOT-89 variant.
struct Sot89 {
    name: &'static str,
    description: &'static str,
    /// The pads of the leads as (number, x, y).
    leads: &'static [(&'static str, f64, f64)],
    lead_size: (f64, f64),
    /// The tab's centre, the anchor rectangle of its pad, and the polygon that makes the
    /// rest of the pad, relative to the centre.
    tab_at: f64,
    tab_anchor: (f64, f64),
    tab_polygon: &'static [(f64, f64)],
}

const SOT89_3_TAB: [(f64, f64); 4] = [
    (3.8625, 0.8665),
    (0.7375, 0.8665),
    (0.7375, -0.8665),
    (3.8625, -0.8665),
];
const SOT89_3_HAND_TAB: [(f64, f64); 4] = [
    (5.3625, 0.8665),
    (1.2375, 0.8665),
    (1.2375, -0.8665),
    (5.3625, -0.8665),
];
/// The tab of SOT-89-5: a bar 1 mm high with the narrow neck 0.8 mm wide that reaches up
/// and down, 2.6 mm to each side of the centre.
const SOT89_5_TAB: [(f64, f64); 12] = [
    (0.9, -0.5),
    (2.6, -0.5),
    (2.6, 0.5),
    (0.9, 0.5),
    (0.4, 1.0),
    (-0.4, 1.0),
    (-0.9, 0.5),
    (-2.6, 0.5),
    (-2.6, -0.5),
    (-0.9, -0.5),
    (-0.4, -1.0),
    (0.4, -1.0),
];
const SOT89_5_HAND_TAB: [(f64, f64); 12] = [
    (0.9, -0.5),
    (3.6, -0.5),
    (3.6, 0.5),
    (0.9, 0.5),
    (0.4, 1.0),
    (-0.4, 1.0),
    (-0.9, 0.5),
    (-3.6, 0.5),
    (-3.6, -0.5),
    (-0.9, -0.5),
    (-0.4, -1.0),
    (0.4, -1.0),
];

const SOT89: &[Sot89] = &[
    Sot89 {
        name: "SOT-89-3",
        description: "SOT-89-3, Microchip 3L SOT-89 (C04-029C)",
        leads: &[("1", -1.95, -1.5), ("3", -1.95, 1.5)],
        lead_size: (1.3, 0.9),
        tab_at: -1.8625,
        tab_anchor: (1.475, 0.9),
        tab_polygon: &SOT89_3_TAB,
    },
    Sot89 {
        name: "SOT-89-3_Handsoldering",
        description: "SOT-89-3, Microchip 3L SOT-89 (C04-029C), pads for hand soldering",
        leads: &[("1", -2.45, -1.5), ("3", -2.45, 1.5)],
        lead_size: (2.3, 0.9),
        tab_at: -2.3625,
        tab_anchor: (2.475, 0.9),
        tab_polygon: &SOT89_3_HAND_TAB,
    },
    Sot89 {
        name: "SOT-89-5",
        description: "SOT-89-5, Ricoh SOT-89-5 outline",
        leads: &[
            ("1", -1.85, -1.5),
            ("3", -1.85, 1.5),
            ("4", 1.85, 1.5),
            ("5", 1.85, -1.5),
        ],
        lead_size: (1.5, 0.7),
        tab_at: 0.0,
        tab_anchor: (0.8, 2.0),
        tab_polygon: &SOT89_5_TAB,
    },
    Sot89 {
        name: "SOT-89-5_Handsoldering",
        description: "SOT-89-5, Ricoh SOT-89-5 outline, pads for hand soldering",
        leads: &[
            ("1", -2.35, -1.5),
            ("3", -2.35, 1.5),
            ("4", 2.35, 1.5),
            ("5", 2.35, -1.5),
        ],
        lead_size: (2.5, 0.7),
        tab_at: 0.0,
        tab_anchor: (0.8, 2.0),
        tab_polygon: &SOT89_5_HAND_TAB,
    },
];

/// SOT-89's body: 2.5 mm across, 4.5 mm along the pins.
const SOT89_BODY: (f64, f64) = (2.5, 4.5);

/// Every `Library:Name` this module generates.
pub fn names() -> Vec<String> {
    DPAK.iter()
        .map(|d| d.name)
        .chain(SOT89.iter().map(|s| s.name))
        .map(|n| format!("{LIBRARY}:{n}"))
        .collect()
}

/// The footprint named `library:name`, if it is a package of this module.
pub fn generate(library: &str, name: &str) -> Option<Footprint> {
    if library != LIBRARY {
        return None;
    }
    if let Some(d) = DPAK.iter().find(|d| d.name == name) {
        return Some(dpak(d));
    }
    SOT89.iter().find(|s| s.name == name).map(sot89)
}

fn dpak(d: &Dpak) -> Footprint {
    let pkg = d.package;
    let mut drawing = Drawing::new(LIBRARY, d.name, Mount::Smd);
    drawing.describe(
        &format!("{}, recorded pads, generated by netsmith", pkg.description),
        pkg.tags,
    );
    for &(number, y) in d.leads {
        drawing.pad(&PadSpec::smd(
            number,
            Pt::new(pkg.lead_x, y),
            (pkg.lead_length, d.lead_height),
        ));
    }
    let (tx, ty, tw, th) = pkg.tab;
    drawing.pad(&PadSpec::new(
        d.tab,
        Kind::SmdNoPaste,
        Shape::RoundRect,
        Pt::new(tx, ty),
        (tw, th),
    ));
    let (paste_number, paste_kind) = if d.paste_numbered {
        (d.tab, Kind::CopperPaste)
    } else {
        ("", Kind::PasteOnly)
    };
    for x in pkg.paste_x {
        for y in [-pkg.paste_y, pkg.paste_y] {
            drawing.pad(&PadSpec::new(
                paste_number,
                paste_kind,
                Shape::RoundRect,
                Pt::new(x, y),
                pkg.paste,
            ));
        }
    }

    let left = pkg.lead_x - pkg.lead_length / 2.0;
    let right = tx + tw / 2.0;
    let (hx, hy) = (pkg.body.0 / 2.0, pkg.body.1 / 2.0);
    // The courtyard clears the body and the tab, whichever reaches further across.
    let court_y = hy.max(th / 2.0) + COURTYARD;
    drawing.courtyard(
        Pt::new(left - COURTYARD, -court_y),
        Pt::new(right + COURTYARD, court_y),
    );

    // The body with the corner by pin 1 cut off, the tab's stub on the right.
    let inset = pkg.stub_inset;
    drawing.polygon(
        Draw::Fab,
        &[
            Pt::new(-hx + 1.0, -hy),
            Pt::new(hx, -hy),
            Pt::new(hx, -hy + inset),
            Pt::new(hx + 1.0, -hy + inset),
            Pt::new(hx + 1.0, hy - inset),
            Pt::new(hx, hy - inset),
            Pt::new(hx, hy),
            Pt::new(-hx, hy),
            Pt::new(-hx, -hy + 1.0),
        ],
        false,
    );
    drawing.fab_reference(1.0);
    // The silkscreen along the body stops short of the tab's copper where the body is
    // not wider than the tab.
    let silk_y = hy + 0.2;
    let silk_to = if silk_y < th / 2.0 + SILK_TO_COPPER {
        tx - tw / 2.0 - SILK_TO_COPPER
    } else {
        hx
    };
    drawing.line(
        Draw::Silk,
        Pt::new(-hx - 0.2, -silk_y),
        Pt::new(silk_to, -silk_y),
    );
    drawing.line(
        Draw::Silk,
        Pt::new(-hx - 0.2, silk_y),
        Pt::new(silk_to, silk_y),
    );
    let first = d.leads[0].1 - d.lead_height / 2.0 - SILK_TO_COPPER;
    marker(&mut drawing, Pt::new(pkg.lead_x, first));
    drawing.finish(-(court_y + 0.7), court_y + 0.7)
}

/// A filled triangle whose apex is at `apex`, pointing down at pin 1.
fn marker(drawing: &mut Drawing, apex: Pt) {
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

fn sot89(s: &Sot89) -> Footprint {
    let mut drawing = Drawing::new(LIBRARY, s.name, Mount::Smd);
    drawing.describe(
        &format!("{}, recorded pads, generated by netsmith", s.description),
        "SOT-89 TO_SOT_SMD",
    );
    let mut left = f64::MAX;
    let mut right = f64::MIN;
    for &(number, x, y) in s.leads {
        drawing.pad(&PadSpec::smd(number, Pt::new(x, y), s.lead_size));
        left = left.min(x - s.lead_size.0 / 2.0);
        right = right.max(x + s.lead_size.0 / 2.0);
    }
    let tab: Vec<Pt> = s.tab_polygon.iter().map(|&(x, y)| Pt::new(x, y)).collect();
    drawing.custom_pad("2", Pt::new(s.tab_at, 0.0), s.tab_anchor, &tab);
    left = left.min(s.tab_at - s.tab_anchor.0 / 2.0);
    right = right.max(s.tab_at + s.tab_anchor.0 / 2.0);
    for p in &tab {
        left = left.min(s.tab_at + p.x);
        right = right.max(s.tab_at + p.x);
    }

    let (hx, hy) = (SOT89_BODY.0 / 2.0, SOT89_BODY.1 / 2.0);
    let court_y = hy + COURTYARD;
    drawing.courtyard(
        Pt::new(left - COURTYARD, -court_y),
        Pt::new(right + COURTYARD, court_y),
    );
    drawing.polygon(
        Draw::Fab,
        &[
            Pt::new(-hx + 1.0, -hy),
            Pt::new(hx, -hy),
            Pt::new(hx, hy),
            Pt::new(-hx, hy),
            Pt::new(-hx, -hy + 1.0),
        ],
        false,
    );
    drawing.fab_reference(0.8);
    let silk_y = hy + 0.11;
    drawing.line(Draw::Silk, Pt::new(-hx, -silk_y), Pt::new(hx, -silk_y));
    drawing.line(Draw::Silk, Pt::new(-hx, silk_y), Pt::new(hx, silk_y));
    let (_, x, y) = s.leads[0];
    marker(
        &mut drawing,
        Pt::new(x, y - s.lead_size.1 / 2.0 - SILK_TO_COPPER),
    );
    drawing.finish(-(court_y + 0.7), court_y + 0.7)
}
