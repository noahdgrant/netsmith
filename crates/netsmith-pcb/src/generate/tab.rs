//! Packages with a large tab: DPAK (TO-252), D2PAK (TO-263) and SOT-89.
//!
//! The tab is the heat path and often a pin, and its pad is not a function of the lead
//! dimensions, so the pads here are the ones the manufacturers' drawings recommend, as
//! KiCad's library gives them (the same basis as SOT-223, ADR 0043): DPAK and D2PAK from
//! Infineon's PG-TO252 and PG-TO263 drawings, SOT-89-3 from Microchip's 3L SOT-89 drawing and SOT-89-5 from Ricoh's.
//! The numbers are facts about those footprints, recorded in the tables below.

use super::draw::{Draw, Drawing, Kind, PadSpec, Shape};
use super::outline::SILK_TO_COPPER;
use crate::body3d::{Edge, Lead, Package};
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

/// SOT-89-3's package dimensions (TO-243), for the shape drawn in 3D (`Package::GullWing`), in
/// mm. Each is the nominal of the **Diodes Incorporated "SOT89 Package Information" drawing
/// (Rev. 2026-01-16, page 1, Package Outline Dimensions, whose table prints a typical value for
/// each)**, checked against **onsemi's SOT-89 3 lead case outline 528AG, issue O, 4 March 2014
/// (document 98AON82692F)** and **Union Semiconductor's SOT89-3 outline drawing (Rev. 02,
/// August 2015)**. onsemi's 528AH refers to JEDEC TO-243 variation AA; JEDEC's own drawing was
/// not read. The symbols are the Diodes table's, with the others' in brackets.
///
/// | Symbol | Meaning | Diodes | onsemi range | Union range |
/// |---|---|---|---|---|
/// | D | body length, along the row of pins | 4.50 | 4.40 to 4.60 | 4.40 to 4.60 |
/// | E | body width, across the row | 2.50 | 2.40 to 2.60 | 2.30 to 2.60 |
/// | e | pitch of the pins | 1.50 | 1.50 | 1.50 |
/// | B1 (b) | width of pins 1 and 3 outside the body | 0.48 | 0.38 to 0.47 | 0.32 to 0.54 |
/// | B (b1) | width of pin 2 outside the body | 0.56 | 0.46 to 0.55 | 0.40 to 0.62 |
/// | L | pin length outside the body (the pins lie flat on the board) | 1.05 | 0.89 to 1.20 | 0.89 to 1.20 |
/// | D1 (D2, D1) | width of the tab | 1.733 | 1.60 to 1.90 | 1.50 to 1.83 |
/// | H (H, E1) | overall length, pin tips to tab tip | 4.10 | 4.05 to 4.25 | 3.94 to 4.25 |
/// | E2 | how far the wide part of the tab runs in under the body from the tab's side | 2.20 | not given | not given |
///
/// Diodes' B1 and B are 0.01 mm over onsemi's range (and inside Union's); every other figure is
/// inside both ranges. Two figures are derived, not read: how far the **tab reaches beyond the
/// body** is H - E - L = 0.55 mm (Diodes' own H1 - E2, 2.78 - 2.20 = 0.58 mm, agrees; the
/// onsemi nominals give 0.60 mm), and the **centre lead** is joined to the wide part of the tab
/// where it stops, E - E2 = 0.30 mm from the pins' edge, under the body. Diodes also gives the
/// width of that wide part, D2 = 1.71 mm, 0.02 mm less than D1: the tab is drawn as wide as D1
/// all along. The thickness of the metal is the drawing-only constant of `body3d.rs` and not the
/// makers' c (0.35 to 0.44 mm). The fabrication outline of the footprint is the body; the pads
/// are the recorded ones (the tab's pad is as wide as D1, the same figure).
mod sot89_package {
    /// Across the row of pins (x), then along it (y).
    pub const BODY: (f64, f64) = super::SOT89_BODY;
    /// Pin pitch.
    pub const PITCH: f64 = 1.50;
    pub const OUTER_PIN_WIDTH: f64 = 0.48;
    pub const CENTRE_PIN_WIDTH: f64 = 0.56;
    /// How far a pin reaches beyond the body (L), and the length of its foot: the same.
    pub const PIN_LENGTH: f64 = 1.05;
    pub const TAB_WIDTH: f64 = 1.733;
    /// Overall, pin tips to tab tip (H).
    pub const OVERALL: f64 = 4.10;
    /// How far the wide part of the tab runs in under the body (E2).
    pub const TAB_UNDER: f64 = 2.20;
    /// How far the tab reaches beyond the body, derived: H - E - L.
    pub const TAB_REACH: f64 = OVERALL - BODY.0 - PIN_LENGTH;
    /// How far in under the body the centre lead runs to meet the tab: E - E2.
    pub const CENTRE_UNDER: f64 = BODY.0 - TAB_UNDER;
}

/// The shape of SOT-89-3: pins 1 and 3 and the narrow end of pin 2 on the left, pin 2's tab
/// under the body and out of the right.
fn sot89_3_package() -> Package {
    use sot89_package as dims;
    let pin = |at: f64, width: Option<f64>, under: Option<f64>| Lead {
        edge: Edge::Left,
        at,
        width,
        reach: None,
        under,
    };
    let leads = vec![
        pin(-dims::PITCH, None, None),
        pin(0.0, Some(dims::CENTRE_PIN_WIDTH), Some(dims::CENTRE_UNDER)),
        pin(dims::PITCH, None, None),
        Lead {
            edge: Edge::Right,
            at: 0.0,
            width: Some(dims::TAB_WIDTH),
            reach: Some(dims::TAB_REACH),
            under: Some(dims::TAB_UNDER),
        },
    ];
    Package::GullWing {
        body: dims::BODY,
        // The pins' tips; the tab states its own reach.
        span: (dims::BODY.0 + 2.0 * dims::PIN_LENGTH, dims::BODY.1),
        lead_width: dims::OUTER_PIN_WIDTH,
        lead_length: dims::PIN_LENGTH,
        leads,
        exposed: None,
    }
}

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
    let footprint = drawing.finish(-(court_y + 0.7), court_y + 0.7);
    // Only the three-pin package is shaped; SOT-89-5 stays a box.
    if s.name.starts_with("SOT-89-3") {
        super::shaped(footprint, sot89_3_package())
    } else {
        footprint
    }
}
