//! Polarized radial capacitors, through hole, two leads (`Capacitor_THT:CP_Radial_D5.0mm_P2.00mm`
//! ... `CP_Radial_D16.0mm_P7.50mm`): the packages of `std::parts::passive::Supercap` (issue 1366),
//! and of any electrolytic can of the same diameter and pitch.
//!
//! The numbers follow from the name and the lead: pad 1, the positive lead, is at the origin with
//! square (rounded) corners, pad 2 is the pitch to its right, round. The lead of a can grows with
//! its diameter, and so do the hole and the pad round it: a 0.8 mm hole in a 1.6 mm pad up to
//! 8 mm across (a 0.5 mm lead), a 1.0 mm hole in a 2.0 mm pad at 10 mm (0.6 mm), a 1.2 mm hole in a
//! 2.4 mm pad at 12.5 mm and 16 mm (0.8 mm), the lands KiCad 9's footprints of these names have
//! (`tests/generated_vs_kicad.rs` holds them to it). The courtyard is the can's circle, centred
//! between the leads, 0.25 mm out; the fabrication layer draws the can and a plus sign at the
//! positive lead, the silkscreen the can outside its outline.
//!
//! **The tapped can (issue 1399), netsmith's own.** `CP_Radial_D16.0mm_P7.50mm_Tap` is the
//! three-lead package of a supercapacitor of two cells in series that brings out the node
//! between them (`Supercap.mid`). KiCad has no footprint of this name, and its land is not read
//! from a maker's drawing: it is netsmith's own pattern, decided by the owner on issue 1399. Since
//! KiCad has no file of the name, it is not answered as a generated `Capacitor_THT` name (a board
//! opened in KiCad would name a footprint its library lacks): [`tapped_file`] writes the file of
//! netsmith's own library, `std/libs/netsmith.pretty/` (`netsmith:CP_Radial_D16.0mm_P7.50mm_Tap`),
//! and a test holds the committed file to it. The name gives the can (16 mm) and the pitch
//! (7.5 mm) between the outer leads, pad 1 (`+`) and pad 3 (`-`), as the two-lead can of that
//! name has them; the balance tap, pad 2, is midway between them on the same line, 3.75 mm from
//! each. The hole and the pad are the two-lead can's (1.2 mm in 2.4 mm), the courtyard and the
//! drawing too. A real part's leads may stand elsewhere: compare its drawing with this land
//! before a board is ordered, and give a part whose leads differ a footprint of the project's own
//! (`kicad.toml`).

use super::draw::{Draw, Drawing, Kind, PadSpec, Shape};
use crate::footprint::{Footprint, Mount};
use crate::geometry::Pt;

const LIBRARY: &str = "Capacitor_THT";

/// A can: its diameter and pitch, as the name spells them, in mm, and its land.
struct Can {
    diameter_text: &'static str,
    diameter: f64,
    pitch_text: &'static str,
    pitch: f64,
    pad: f64,
    drill: f64,
}

const CANS: [Can; 6] = [
    Can {
        diameter_text: "5.0",
        diameter: 5.0,
        pitch_text: "2.00",
        pitch: 2.0,
        pad: 1.6,
        drill: 0.8,
    },
    Can {
        diameter_text: "6.3",
        diameter: 6.3,
        pitch_text: "2.50",
        pitch: 2.5,
        pad: 1.6,
        drill: 0.8,
    },
    Can {
        diameter_text: "8.0",
        diameter: 8.0,
        pitch_text: "3.50",
        pitch: 3.5,
        pad: 1.6,
        drill: 0.8,
    },
    Can {
        diameter_text: "10.0",
        diameter: 10.0,
        pitch_text: "5.00",
        pitch: 5.0,
        pad: 2.0,
        drill: 1.0,
    },
    Can {
        diameter_text: "12.5",
        diameter: 12.5,
        pitch_text: "5.00",
        pitch: 5.0,
        pad: 2.4,
        drill: 1.2,
    },
    Can {
        diameter_text: "16.0",
        diameter: 16.0,
        pitch_text: "7.50",
        pitch: 7.5,
        pad: 2.4,
        drill: 1.2,
    },
];

impl Can {
    fn name(&self) -> String {
        format!("CP_Radial_D{}mm_P{}mm", self.diameter_text, self.pitch_text)
    }
}

/// The can that has a tapped variant (issue 1399): the 16 mm can of a two-cell supercapacitor.
const TAPPED: &Can = &CANS[5];

/// The name of the tapped variant of `can`.
fn tapped_name(can: &Can) -> String {
    format!("{}_Tap", can.name())
}

/// Every `Library:Name` this module generates.
pub(super) fn names() -> Vec<String> {
    CANS.iter()
        .map(|c| format!("{LIBRARY}:{}", c.name()))
        .collect()
}

/// The file of netsmith's own library for the tapped can: its name and the text of its
/// `.kicad_mod` (issue 1399).
pub(super) fn tapped_file() -> (String, String) {
    let (d, below) = drawing(netsmith_kicad::STDLIB_NAME, TAPPED, true);
    (tapped_name(TAPPED), d.text(-below, below))
}

/// The footprint named `library:name`, if it is one of this module's.
pub(super) fn generate(library: &str, name: &str) -> Option<Footprint> {
    if library != LIBRARY {
        return None;
    }
    let can = CANS.iter().find(|c| c.name() == name)?;
    let (d, below) = drawing(LIBRARY, can, false);
    Some(d.finish(-below, below))
}

/// The drawing of `can` in `library`: two leads, or three with the balance tap midway when
/// `tapped`; and how far below the origin the value goes (the reference as far above).
fn drawing(library: &str, can: &Can, tapped: bool) -> (Drawing, f64) {
    let name = if tapped { tapped_name(can) } else { can.name() };
    let mut d = Drawing::new(library, &name, Mount::ThroughHole);
    if tapped {
        // No 3D model: KiCad has none of this name.
        d.without_model();
    }
    if tapped {
        d.describe(
            &format!(
                "Polarized radial capacitor of two cells with a balance tap, {} mm across, {} mm \
                 pitch between the outer leads, the tap midway; netsmith's own land pattern, not \
                 from a maker's drawing",
                can.diameter_text, can.pitch_text
            ),
            "CP Radial capacitor polarized THT supercapacitor tap",
        );
    } else {
        d.describe(
            &format!(
                "Polarized radial capacitor, {} mm across, {} mm pitch, generated by netsmith",
                can.diameter_text, can.pitch_text
            ),
            "CP Radial capacitor polarized THT",
        );
    }
    let centre = Pt::new(can.pitch / 2.0, 0.0);
    let radius = can.diameter / 2.0;
    let leads: &[(u32, Shape, f64)] = if tapped {
        &[
            (1, Shape::RoundRect, 0.0),
            (2, Shape::Circle, can.pitch / 2.0),
            (3, Shape::Circle, can.pitch),
        ]
    } else {
        &[(1, Shape::RoundRect, 0.0), (2, Shape::Circle, can.pitch)]
    };
    for &(number, shape, x) in leads {
        d.pad(&PadSpec::new(
            number,
            Kind::Through(can.drill),
            shape,
            Pt::new(x, 0.0),
            (can.pad, can.pad),
        ));
    }
    d.circle_at(Draw::Courtyard, centre, radius + 0.25);
    d.circle_at(Draw::Fab, centre, radius);
    d.circle_at(Draw::Silk, centre, radius + 0.12);
    // A plus sign on the fabrication layer, left of the positive lead, inside the can.
    let plus = Pt::new(-can.pad / 2.0 - 0.6, -can.pad / 2.0 - 0.3);
    let arm = 0.25;
    d.line(
        Draw::Fab,
        Pt::new(plus.x - arm, plus.y),
        Pt::new(plus.x + arm, plus.y),
    );
    d.line(
        Draw::Fab,
        Pt::new(plus.x, plus.y - arm),
        Pt::new(plus.x, plus.y + arm),
    );
    d.fab_reference_at(centre, 1.0);
    (d, radius + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::footprint::PadKind;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn the_six_cans_have_their_pitch_hole_and_a_courtyard_round_the_can() {
        assert_eq!(names().len(), 6);
        for can in &CANS {
            let fp = generate(LIBRARY, &can.name()).unwrap();
            assert_eq!(fp.pad_numbers(), ["1", "2"], "{}", can.name());
            assert!(near(fp.pads[0].at.x, 0.0) && near(fp.pads[1].at.x, can.pitch));
            for pad in &fp.pads {
                assert_eq!(pad.kind, PadKind::ThruHole);
                assert!(near(pad.drill.unwrap().width, can.drill));
                assert!(near(pad.width, can.pad) && near(pad.height, can.pad));
            }
            let r = can.diameter / 2.0 + 0.25;
            assert!(
                near(fp.courtyard.min.x, can.pitch / 2.0 - r),
                "{}",
                can.name()
            );
            assert!(near(fp.courtyard.max.y, r), "{}", can.name());
            // A pad stays clear of its neighbour.
            assert!(can.pitch - can.pad > 0.3, "{}", can.name());
        }
        assert!(generate(LIBRARY, "CP_Radial_D5.0mm_P2.50mm").is_none());
        assert!(generate(LIBRARY, "CP_Radial_D16.0mm_P7.50mm_Tap").is_none());
        assert!(generate("Capacitor_SMD", "CP_Radial_D5.0mm_P2.00mm").is_none());
    }

    #[test]
    fn the_tapped_can_has_its_tap_midway_between_the_outer_leads() {
        let (name, text) = tapped_file();
        assert_eq!(name, "CP_Radial_D16.0mm_P7.50mm_Tap");
        let fp = Footprint::parse(&format!("netsmith:{name}"), &text).unwrap();
        assert_eq!(fp.pad_numbers(), ["1", "2", "3"]);
        let xs: Vec<f64> = fp.pads.iter().map(|p| p.at.x).collect();
        assert!(
            near(xs[0], 0.0) && near(xs[1], 3.75) && near(xs[2], 7.5),
            "{xs:?}"
        );
        for pad in &fp.pads {
            assert!(near(pad.at.y, 0.0));
            assert!(near(pad.drill.unwrap().width, 1.2));
            assert!(near(pad.width, 2.4));
        }
        // The tap's pad keeps 1.35 mm of board between it and each outer pad.
        assert!(near(xs[1] - xs[0] - fp.pads[0].width, 1.35));
        // The courtyard is the two-lead can's.
        let two = generate(LIBRARY, "CP_Radial_D16.0mm_P7.50mm").unwrap();
        assert!(near(fp.courtyard.min.x, two.courtyard.min.x));
        assert!(near(fp.courtyard.max.y, two.courtyard.max.y));
    }
}
