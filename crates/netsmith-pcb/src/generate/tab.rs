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
    /// The sourced dimensions of the shape drawn in 3D.
    shape: &'static TabShape,
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
    shape: &DPAK_SHAPE,
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
    shape: &D2PAK_SHAPE,
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

/// The package dimensions of DPAK (TO-252) and D2PAK (TO-263) with two or three lead pads, for the
/// shape drawn in 3D, in mm. The 3D view reads these and not the land pattern above, whose
/// lead pads are wider than the leads (ADR 0043). Each row's nominal is **Alpha & Omega's
/// "TO252 PACKAGE OUTLINE" (PO-00009, version W) and "TO263(D2PAK) PACKAGE OUTLINE" (PO-00005,
/// version N)**, the one maker whose table prints a nominal for each figure. Both refer to JEDEC
/// TO-252 (AA) and TO-263 AB; JEDEC's own drawings were not read. Each is checked against the
/// ranges of **onsemi's DPAK-3 case 369C, issue L (document 98AON10527D, 24 August 2026)** and
/// **D2PAK-3 case 418AJ, issue F (98AON56370E, 11 March 2021)**, **Vishay's D-PAK (TO-252AA) "M"
/// (document 95627, 24 June 2016)** and **D2PAK 2L (TO-263AB 2L)**, and **Diodes Incorporated's
/// TO252-5 (Rev. 2017-04-04) and TO263AB (D2PAK) (2016-11-15)** package outlines. The symbols are
/// AOS's, with the others' in brackets where they differ.
///
/// DPAK:
///
/// | Symbol | Meaning | AOS | onsemi range | Vishay range | Diodes range |
/// |---|---|---|---|---|---|
/// | D | body length, along the leads | 6.096 | 5.97 to 6.22 | 5.97 to 6.22 | 6.00 to 6.20 |
/// | E | body width, across | 6.604 | 6.35 to 6.73 | 6.35 to 6.73 | 6.45 to 6.70 |
/// | e | pitch of the leads | 2.286 | 2.29 | 2.29 | not given (1.27, TO252-5) |
/// | b | width of a lead | 0.762 | 0.63 to 0.89 | 0.64 to 0.89 | 0.51 to 0.71 (TO252-5's finer leads) |
/// | L1 | how far a lead reaches beyond the body (a reference figure) | 2.921 | 2.90 | 2.74 | 2.74 |
/// | L | the foot of a lead | 1.520 | 1.40 to 1.78 | 1.40 to 1.78 | 1.40 to 1.78 |
/// | H | overall, tab tip to lead tips | 10.033 | 9.40 to 10.41 | 9.40 to 10.41 | 9.40 to 10.41 |
/// | L3 | how far the tab reaches beyond the body | 1.016 | 0.89 to 1.27 | 0.89 to 1.27 | 0.88 to 1.27 |
/// | b2 (b3) | width of the tab beyond the body | 5.340 | 4.57 to 5.46 | 4.95 to 5.46 | 5.21 to 5.46 |
/// | D1 | length of the exposed tab under the body | 5.249 | not given | at least 5.21 | at least 5.21 |
/// | E1 | width of the exposed tab under the body | 4.826 | not given | at least 4.32 | at least 4.32 |
///
/// D2PAK:
///
/// | Symbol | Meaning | AOS | onsemi range | Vishay range | Diodes range |
/// |---|---|---|---|---|---|
/// | D | body length, along the leads | 9.14 | 8.38 to 9.65 | 8.51 to 9.65 | 8.39 to 9.65 |
/// | E | body width, across | 10.03 | 9.65 to 10.67 | 9.65 to 10.67 | 9.66 to 10.66 |
/// | e | pitch of the leads | 2.54 | 2.54 | 2.54 | 2.54 |
/// | b | width of a lead | 0.81 | 0.51 to 0.99 | 0.51 to 0.99 | 0.51 to 0.99 |
/// | L | the foot of a lead | 2.54 | 1.78 to 2.79 | 1.78 to 2.79 | 1.78 to 2.79 |
/// | H | overall, tab tip to lead tips | 15.24 | 14.60 to 15.88 | 14.61 to 15.88 | 14.61 to 15.87 |
/// | L1 | how far the tab reaches beyond the body | 1.27 | at most 1.68 | at most 1.65 | at most 1.67 |
/// | L4 | how far a lead reaches beyond the body | not given | not given | 4.78 to 5.28 | not given |
/// | E1 | width of the tab | 8.00 | at least 6.22 | 7.90 to 8.80 | 6.23 to 8.23 |
/// | D1 | length of the exposed tab under the body | 8.00 | at least 6.60 | 6.86 to 8.00 | 6.55 to 6.95 |
///
/// Three figures are derived, not read. A D2PAK **lead's reach** beyond the body is H - D - L1 =
/// 4.83 mm (inside Vishay's L4). The D2PAK's **exposed tab length D1** is 6.90 mm: AOS's 8.00 mm
/// is outside Diodes' range, and 6.90 mm, the middle of 6.86 to 6.95 mm, is the one stretch
/// that every maker's range holds. The DPAK's reach 2.921 mm is AOS's own reference figure, and
/// is also H - D - L3.
///
/// What is not drawn: the **centre lead**. Every outline here shows pin 2 of a three-pad
/// package as the tab and the lead as a cut stub with no dimension (the TO-252-3 and TO-263-3
/// variants), so only the two outer leads are drawn; the TO-252-2 and TO-263-2 have two. The
/// metal's thickness is the drawing-only constant of `body3d.rs` and not the makers' c; the
/// chamfers of the tab, the tab's hole and the ears of the DPAK's tab are not drawn. The body
/// across (y) is the footprint's own, 6.5 mm (DPAK) and 10.0 mm (D2PAK): both are inside every
/// maker's range of E, while AOS's nominals would overhang the footprint's body outline by
/// 0.10 and 0.03 mm. The pads are the recorded ones (ADR 0043) and not these dimensions.
///
/// **Variants with no source stay boxes**: the 4- and 5-lead DPAK (TO-252-4 and -5, whose pads
/// are at a 1.14 mm pitch where Diodes' TO252-5 outline has 1.27 mm) and the 4- to 9-lead D2PAK
/// (TO-263-4 to -9, whose pitches of 1.7, 1.27 and 0.97 mm no outline read here gives).
struct TabShape {
    /// Along the leads (x), then across (y).
    body: (f64, f64),
    pitch: f64,
    lead_width: f64,
    /// How far a lead reaches beyond the body, and the length of its foot.
    lead_reach: f64,
    lead_foot: f64,
    /// The width of the tab beyond the body and how far it reaches.
    tab_width: f64,
    tab_reach: f64,
    /// The exposed tab under the body: its width and how far in from the body's edge it runs.
    under_width: f64,
    under: f64,
}

const DPAK_SHAPE: TabShape = TabShape {
    // Across: the footprint's own body, `TO252.body.1`.
    body: (6.096, 6.5),
    pitch: 2.286,
    lead_width: 0.762,
    lead_reach: 2.921,
    lead_foot: 1.520,
    tab_width: 5.340,
    tab_reach: 1.016,
    under_width: 4.826,
    under: 5.249,
};

const D2PAK_SHAPE: TabShape = TabShape {
    // Across: the footprint's own body, `TO263.body.1`.
    body: (9.14, 10.0),
    pitch: 2.54,
    lead_width: 0.81,
    // H - D - L1: 15.24 - 9.14 - 1.27.
    lead_reach: 15.24 - 9.14 - 1.27,
    lead_foot: 2.54,
    tab_width: 8.00,
    tab_reach: 1.27,
    under_width: 8.00,
    under: 6.90,
};

/// The shape of a DPAK or D2PAK: the two outer leads on the left, and the tab out of the right
/// side and flat under the body.
fn tab_package(s: &TabShape) -> Package {
    let lead = |at: f64| Lead {
        edge: Edge::Left,
        at,
        width: None,
        reach: None,
        under: None,
    };
    let tab = |width: f64, reach: f64, under: f64| Lead {
        edge: Edge::Right,
        at: 0.0,
        width: Some(width),
        reach: Some(reach),
        under: Some(under),
    };
    let mut leads = vec![lead(-s.pitch), lead(s.pitch)];
    if (s.tab_width - s.under_width).abs() < 1e-9 {
        leads.push(tab(s.tab_width, s.tab_reach, s.under));
    } else {
        // The part beyond the body is wider than the exposed part under it.
        leads.push(tab(s.tab_width, s.tab_reach, 0.0));
        leads.push(tab(s.under_width, 0.0, s.under));
    }
    Package::GullWing {
        body: s.body,
        span: (s.body.0 + 2.0 * s.lead_reach, s.body.1),
        lead_width: s.lead_width,
        lead_length: s.lead_foot,
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
    let footprint = drawing.finish(-(court_y + 0.7), court_y + 0.7);
    // Only the packages of two or three lead pads are shaped; the others stay boxes.
    match (d.leads.len() <= 3).then_some(pkg.shape) {
        Some(shape) => super::shaped(footprint, tab_package(shape)),
        None => footprint,
    }
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
