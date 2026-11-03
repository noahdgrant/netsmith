//! Footprints that netsmith generates itself (ADR 0043).
//!
//! A generator turns the parameters of a package into a [`Footprint`], the same model the
//! loader makes from a `.kicad_mod`, using the land pattern formulas of IPC-7351B at
//! nominal density ([`ipc`]). Each generator answers to the names KiCad's libraries give
//! its footprints, so a mapping row such as `Resistor_SMD:R_0402_1005Metric` finds it, and
//! [`FootprintLibrary`](crate::FootprintLibrary) consults the generators before any file.
//! Every generated footprint is checked against KiCad's own of the same name by a test
//! (`tests/generated_vs_kicad.rs`) when KiCad's libraries are installed.

mod chip;
mod diode;
mod draw;
mod gullwing;
mod header;
pub mod ipc;
mod outline;
mod qfn;
mod sot;

use std::sync::OnceLock;

use crate::footprint::Footprint;

/// The footprint that `Library:Name` names, if a generator makes it.
pub fn generate(id: &str) -> Option<Footprint> {
    let (library, name) = id.split_once(':')?;
    chip::generate(library, name)
        .or_else(|| gullwing::generate(library, name))
        .or_else(|| sot::generate(library, name))
        .or_else(|| diode::generate(library, name))
        .or_else(|| qfn::generate(library, name))
        .or_else(|| header::generate(library, name))
}

/// Every `Library:Name` that [`generate`] answers, in a fixed order.
pub fn names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut all = chip::names();
        all.extend(gullwing::names());
        all.extend(sot::names());
        all.extend(diode::names());
        all.extend(qfn::names());
        all.extend(header::names());
        all.sort();
        all.dedup();
        all
    })
}

/// Whether some generated name belongs to the library `library`.
pub fn covers_library(library: &str) -> bool {
    names()
        .iter()
        .any(|n| n.split_once(':').is_some_and(|(l, _)| l == library))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::footprint::{Layer, Mount, PadKind, PadShape};
    use crate::geometry::{Pt, Rect};

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn every_listed_name_is_generated_under_that_name_and_twice_the_same() {
        assert!(names().len() > 400, "{} names", names().len());
        let mut seen = std::collections::HashSet::new();
        for id in names() {
            assert!(seen.insert(id.as_str()), "{id} is listed twice");
            let first = generate(id).unwrap_or_else(|| panic!("{id} is listed but not made"));
            let (_, name) = id.split_once(':').unwrap();
            assert_eq!(first.name, name);
            assert_eq!(generate(id).unwrap(), first, "{id} is not deterministic");
            assert!(!first.pads.is_empty(), "{id} has no pads");
            assert!(first.courtyard.width() > 0.0, "{id} has no courtyard");
        }
    }

    #[test]
    fn a_name_that_is_not_generated_is_none() {
        for id in [
            "Resistor_SMD:R_9999_9999Metric",
            "Resistor_SMD:C_0402_1005Metric",
            "Nothing:R_0402_1005Metric",
            "R_0402_1005Metric",
            "Package_SO:SOIC-9_3.9x4.9mm_P1.27mm",
            "Connector_PinHeader_2.54mm:PinHeader_1x2_P2.54mm_Vertical",
            "Connector_PinHeader_2.54mm:PinHeader_1x41_P2.54mm_Vertical",
            "Connector_PinHeader_2.54mm:PinHeader_3x02_P2.54mm_Vertical",
            "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.00mm_Vertical",
        ] {
            assert!(generate(id).is_none(), "{id}");
        }
    }

    #[test]
    fn the_0402_resistor_has_the_pads_ipc_7351b_gives_a_body_of_1_0_to_1_1_by_0_48_to_0_6() {
        let fp = generate("Resistor_SMD:R_0402_1005Metric").unwrap();
        assert_eq!(fp.mount, Mount::Smd);
        assert_eq!(fp.pad_numbers(), ["1", "2"]);
        let (a, b) = (&fp.pads[0], &fp.pads[1]);
        assert!(near(a.at.x, -0.51) && near(b.at.x, 0.51) && near(a.at.y, 0.0));
        assert!(near(a.width, 0.54) && near(a.height, 0.64));
        assert_eq!((a.kind, a.shape), (PadKind::Smd, PadShape::RoundRect));
        assert_eq!(a.layers, ["F.Cu", "F.Mask", "F.Paste"]);
        // A quarter of the shorter side, as KiCad's generated pads have it.
        assert!(near(a.corner_ratio, 0.25));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-0.93, -0.47), Pt::new(0.93, 0.47))
        );
        assert!(near(fp.reference_at.y, -1.17));
    }

    #[test]
    fn a_larger_chip_has_a_corner_radius_of_at_most_a_quarter_millimetre() {
        let fp = generate("Resistor_SMD:R_1206_3216Metric").unwrap();
        let pad = &fp.pads[0];
        assert!((pad.corner_ratio * pad.width.min(pad.height) - 0.25).abs() < 1e-4);
    }

    #[test]
    fn the_0201_has_stencil_openings_that_are_not_pads() {
        let fp = generate("Capacitor_SMD:C_0201_0603Metric").unwrap();
        assert_eq!(fp.pads.len(), 4);
        let paste: Vec<_> = fp.pads.iter().filter(|p| p.number.is_empty()).collect();
        assert_eq!(paste.len(), 2);
        assert!(paste.iter().all(|p| p.layers == ["F.Paste"]));
        // Pulled in from the copper pads, and not as wide.
        let copper = &fp.pads.iter().find(|p| p.number == "1").unwrap();
        assert!(paste[0].width < copper.width && paste[0].height < copper.height);
    }

    #[test]
    fn a_led_and_a_diode_mark_the_cathode_side_on_the_silkscreen() {
        for id in ["LED_SMD:LED_0603_1608Metric", "Diode_SMD:D_SOD-123"] {
            let fp = generate(id).unwrap();
            let silk: Vec<_> = fp
                .graphics
                .iter()
                .filter(|g| g.layer == Layer::Silk)
                .collect();
            assert!(!silk.is_empty(), "{id}");
            // Everything on the silkscreen is on the left of pad 1's outer edge, or between
            // the pads: none of it sits on pad 2's side beyond the body.
            let pad1 = &fp.pads.iter().find(|p| p.number == "1").unwrap();
            let outer = pad1.at.x - pad1.width / 2.0;
            assert!(
                silk.iter()
                    .flat_map(|g| g.shape.extent_points())
                    .any(|p| p.x < outer),
                "{id}: no silkscreen by pad 1"
            );
        }
    }

    #[test]
    fn sot_23_has_three_pads_on_the_ipc_pattern() {
        let fp = generate("Package_TO_SOT_SMD:SOT-23").unwrap();
        assert_eq!(fp.pad_numbers(), ["1", "2", "3"]);
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        assert!(near(at("1").x, -0.9375) && near(at("1").y, -0.95));
        assert!(near(at("2").y, 0.95));
        assert!(near(at("3").x, 0.9375) && near(at("3").y, 0.0));
        assert!(near(fp.pads[0].width, 1.475) && near(fp.pads[0].height, 0.6));
    }

    #[test]
    fn sot_23_5_and_6_number_down_the_left_and_up_the_right() {
        let five = generate("Package_TO_SOT_SMD:SOT-23-5").unwrap();
        let six = generate("Package_TO_SOT_SMD:SOT-23-6").unwrap();
        assert_eq!(five.pad_numbers(), ["1", "2", "3", "4", "5"]);
        assert_eq!(six.pad_numbers(), ["1", "2", "3", "4", "5", "6"]);
        let y = |fp: &Footprint, n: &str| fp.pads_numbered(n).next().unwrap().at.y;
        assert!(y(&six, "1") < y(&six, "3") && y(&six, "4") > y(&six, "6"));
        // The five pin part has no middle pad on the right.
        assert!(five.pads_numbered("5").all(|p| near(p.at.y, -0.95)));
    }

    #[test]
    fn sot_223_has_a_tab_pad_numbered_as_its_name_says() {
        let tab2 = generate("Package_TO_SOT_SMD:SOT-223-3_TabPin2").unwrap();
        assert_eq!(tab2.pad_numbers(), ["1", "2", "3"]);
        assert_eq!(tab2.pads_numbered("2").count(), 2);
        let tab = tab2.pads.iter().find(|p| p.at.x > 0.0).unwrap();
        assert_eq!(tab.number, "2");
        assert!(near(tab.height, 3.8));
        let four = generate("Package_TO_SOT_SMD:SOT-223").unwrap();
        assert_eq!(four.pad_numbers(), ["1", "2", "3", "4"]);
    }

    #[test]
    fn soic_8_has_its_pads_numbered_counterclockwise_from_the_top_left() {
        let fp = generate("Package_SO:SOIC-8_3.9x4.9mm_P1.27mm").unwrap();
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        assert!(near(at("1").x, -2.475) && near(at("1").y, -1.905));
        assert!(near(at("4").y, 1.905) && near(at("5").x, 2.475) && near(at("5").y, 1.905));
        assert!(near(at("8").y, -1.905));
        assert!(near(fp.pads[0].width, 1.95) && near(fp.pads[0].height, 0.6));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-3.7, -2.7), Pt::new(3.7, 2.7))
        );
    }

    #[test]
    fn a_qfn_has_an_exposed_pad_numbered_after_the_leads_and_paste_openings_without_a_number() {
        let fp = generate("Package_DFN_QFN:QFN-16-1EP_3x3mm_P0.5mm_EP1.7x1.7mm").unwrap();
        assert_eq!(fp.pads.len(), 16 + 1 + 4);
        let ep = fp.pads_numbered("17").next().unwrap();
        assert_eq!(ep.shape, PadShape::Rect);
        assert_eq!(ep.layers, ["F.Cu", "F.Mask"]);
        assert!(near(ep.width, 1.7) && near(ep.height, 1.7));
        // 65 % of the exposed pad is paste, in four openings.
        let paste: f64 = fp
            .pads
            .iter()
            .filter(|p| p.number.is_empty())
            .map(|p| p.width * p.height)
            .sum();
        assert!((paste / (1.7 * 1.7) - 0.65).abs() < 0.02, "{paste}");
        // Pin 1 is the top of the left edge; pin 5 the left of the bottom edge.
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        assert!(at("1").x < 0.0 && at("1").y < 0.0 && at("4").y > 0.0);
        assert!(at("5").y > 0.0 && at("5").x < 0.0 && at("8").x > 0.0);
        assert!(at("9").x > 0.0 && at("13").y < 0.0 && at("16").x < 0.0);
    }

    #[test]
    fn the_leads_of_a_qfn_keep_off_a_large_exposed_pad() {
        // With a 1.9 mm exposed pad the inner edges of the leads are 0.2 mm from it.
        let fp = generate("Package_DFN_QFN:QFN-16-1EP_3x3mm_P0.5mm_EP1.7x1.7mm").unwrap();
        let lead = fp.pads_numbered("1").next().unwrap();
        let inner = lead.at.x.abs() - lead.width / 2.0;
        assert!(inner >= 1.7 / 2.0 + 0.2 - 1e-9, "{inner}");
    }

    #[test]
    fn a_dfn_has_pads_on_two_edges_only() {
        let fp = generate("Package_DFN_QFN:DFN-8-1EP_3x2mm_P0.5mm_EP1.7x1.6mm").unwrap();
        let leads: Vec<_> = fp
            .pads
            .iter()
            .filter(|p| p.number.parse::<u32>().is_ok_and(|n| n <= 8))
            .collect();
        assert_eq!(leads.len(), 8);
        assert!(leads.iter().all(|p| p.at.x.abs() > 1.0));
    }

    #[test]
    fn a_pin_header_is_through_hole_with_a_square_first_pad() {
        let fp = generate("Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical").unwrap();
        assert_eq!(fp.mount, Mount::ThroughHole);
        assert_eq!(fp.pad_numbers(), ["1", "2"]);
        assert_eq!(fp.pads[0].shape, PadShape::Rect);
        assert_eq!(fp.pads[1].shape, PadShape::Oval);
        assert!(near(fp.pads[1].at.y, 2.54) && near(fp.pads[1].width, 1.7));
        assert!(near(fp.pads[0].drill.unwrap().width, 1.0));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-1.8, -1.8), Pt::new(1.8, 4.35))
        );
    }

    #[test]
    fn the_second_row_of_a_header_is_to_the_right_and_of_a_socket_to_the_left() {
        let header =
            generate("Connector_PinHeader_2.00mm:PinHeader_2x03_P2.00mm_Vertical").unwrap();
        let socket =
            generate("Connector_PinSocket_2.00mm:PinSocket_2x03_P2.00mm_Vertical").unwrap();
        let at = |fp: &Footprint, n: &str| fp.pads_numbered(n).next().unwrap().at;
        assert_eq!(at(&header, "2"), Pt::new(2.0, 0.0));
        assert_eq!(at(&socket, "2"), Pt::new(-2.0, 0.0));
        assert_eq!(at(&header, "5"), Pt::new(0.0, 4.0));
        assert!(near(header.pads[1].drill.unwrap().width, 0.8));
        assert!(near(header.pads[1].width, 1.35));
    }
}
