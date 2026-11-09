//! Footprints that netsmith generates itself (ADR 0043).
//!
//! A generator turns the parameters of a package into a [`Footprint`], the same model the
//! loader makes from a `.kicad_mod`, using the land pattern formulas of IPC-7351B at
//! nominal density ([`ipc`]). Each generator answers to the names KiCad's libraries give
//! its footprints, so a mapping row such as `Resistor_SMD:R_0402_1005Metric` finds it, and
//! [`FootprintLibrary`](crate::FootprintLibrary) consults the generators before any file.
//! Every generated footprint is checked against KiCad's own of the same name by a test
//! (`tests/generated_vs_kicad.rs`) when KiCad's libraries are installed.

mod bga;
mod chip;
mod crystal;
mod diode;
mod draw;
mod gullwing;
mod header;
pub mod ipc;
mod outline;
mod qfn;
mod qfn_rows;
mod qfp;
mod sot;
mod tab;

use std::sync::OnceLock;

use crate::footprint::Footprint;

/// The footprint that `Library:Name` names, if a generator makes it.
pub fn generate(id: &str) -> Option<Footprint> {
    let (library, name) = id.split_once(':')?;
    chip::generate(library, name)
        .or_else(|| gullwing::generate(library, name))
        .or_else(|| sot::generate(library, name))
        .or_else(|| tab::generate(library, name))
        .or_else(|| diode::generate(library, name))
        .or_else(|| qfn::generate(library, name))
        .or_else(|| qfp::generate(library, name))
        .or_else(|| header::generate(library, name))
        .or_else(|| crystal::generate(library, name))
        .or_else(|| bga::generate(library, name))
}

/// Every `Library:Name` that [`generate`] answers, in a fixed order.
pub fn names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut all = chip::names();
        all.extend(gullwing::names());
        all.extend(sot::names());
        all.extend(tab::names());
        all.extend(diode::names());
        all.extend(qfn::names());
        all.extend(qfp::names());
        all.extend(header::names());
        all.extend(crystal::names());
        all.extend(bga::names());
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
            // A QFP of a maker's own drawing, which is not the MS-026 pattern.
            "Package_QFP:LQFP-64_10x10mm_P0.5mm",
            "Package_QFP:TQFP-47_7x7mm_P0.5mm",
            // Wolfspeed's pad numbering (the tab is pin 1), not the Infineon pattern.
            "Package_TO_SOT_SMD:TO-263-2_TabPin1",
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
    fn the_tssop_rows_have_pads_1_475_mm_long_at_the_span_of_their_body_width() {
        // (name, pads, centre of the left column, pad width) from the MO-153 spans:
        // 6.4 mm over the leads of a 4.4 mm body, 8.1 mm of a 6.1 mm one, 10.0 mm of an 8 mm one.
        for (name, pads, x, width) in [
            ("TSSOP-20_4.4x5mm_P0.5mm", 20, 2.8625, 0.3),
            ("TSSOP-24_6.1x7.8mm_P0.65mm", 24, 3.7125, 0.4),
            ("TSSOP-48_6.1x9.7mm_P0.4mm", 48, 3.7125, 0.25),
            ("TSSOP-56_8x14mm_P0.5mm", 56, 4.6625, 0.3),
        ] {
            let fp = generate(&format!("Package_SO:{name}")).unwrap();
            assert_eq!(fp.pads.len(), pads, "{name}");
            let first = &fp.pads[0];
            assert!(near(first.at.x, -x), "{name}: {}", first.at.x);
            assert!(
                near(first.width, 1.475) && near(first.height, width),
                "{name}"
            );
            // Pin 1 is at the top of the left column, the last pin at the top of the right.
            let last = &fp.pads[pads - 1];
            assert!(near(last.at.x, x) && near(last.at.y, first.at.y), "{name}");
        }
        let fp = generate("Package_SO:TSSOP-48_6.1x9.7mm_P0.4mm").unwrap();
        let pitch = fp.pads[1].at.y - fp.pads[0].at.y;
        assert!(near(pitch, 0.4), "{pitch}");
    }

    #[test]
    fn the_wide_soic_rows_share_the_ms_013_pads() {
        let fp = generate("Package_SO:SOIC-14W_7.5x9mm_P1.27mm").unwrap();
        assert_eq!(fp.pads.len(), 14);
        assert!(near(fp.pads[0].at.x, -4.65) && near(fp.pads[0].at.y, -3.81));
        assert!(near(fp.pads[0].width, 2.05) && near(fp.pads[0].height, 0.6));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-5.93, -4.75), Pt::new(5.93, 4.75))
        );
        assert_eq!(
            generate("Package_SO:SOIC-16W_7.5x12.8mm_P1.27mm")
                .unwrap()
                .pads
                .len(),
            16
        );
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
    fn a_row_whose_name_does_not_say_its_library_is_found_only_in_that_library() {
        let id = "Package_CSP:LFCSP-16-1EP_3x3mm_P0.5mm_EP1.7x1.7mm";
        let fp = generate(id).unwrap();
        assert_eq!(fp.pads.len(), 16 + 1 + 4);
        assert!(names().iter().any(|n| n == id));
        assert!(generate("Package_DFN_QFN:LFCSP-16-1EP_3x3mm_P0.5mm_EP1.7x1.7mm").is_none());
        // A maker's part named by its drawing, in the library its start does not say.
        assert!(generate("Package_SON:Texas_X2SON-4_1x1mm_P0.65mm").is_none());
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

    #[test]
    fn a_tqfp_48_has_twelve_pads_on_each_edge_numbered_counterclockwise_from_the_left_top() {
        let fp = generate("Package_QFP:TQFP-48_7x7mm_P0.5mm").unwrap();
        assert_eq!(fp.pads.len(), 48);
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        // JEDEC MS-026: 9.0 mm over the leads, 0.6 mm leads: pads 1.475 x 0.3 mm centred
        // 4.1625 mm from the middle, the courtyard 0.25 mm outside the pads.
        assert!(near(at("1").x, -4.1625) && near(at("1").y, -2.75));
        assert!(near(at("12").y, 2.75) && near(at("13").x, -2.75) && near(at("13").y, 4.1625));
        assert!(near(at("25").x, 4.1625) && near(at("25").y, 2.75));
        assert!(near(at("37").x, 2.75) && near(at("37").y, -4.1625));
        assert!(near(fp.pads[0].width, 1.475) && near(fp.pads[0].height, 0.3));
        // The pads of the top and bottom edges are turned a quarter.
        assert!(near(fp.pads[12].width, 0.3) && near(fp.pads[12].height, 1.475));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-5.15, -5.15), Pt::new(5.15, 5.15))
        );
    }

    #[test]
    fn a_qfp_with_an_exposed_pad_has_it_numbered_after_the_leads_with_its_paste_in_a_grid() {
        let fp = generate("Package_QFP:LQFP-48-1EP_7x7mm_P0.5mm_EP3.6x3.6mm").unwrap();
        // 48 leads, the pad, and nine paste openings of 0.97 mm.
        assert_eq!(fp.pads.len(), 48 + 1 + 9);
        let ep = fp.pads_numbered("49").next().unwrap();
        assert!(near(ep.width, 3.6) && near(ep.height, 3.6));
        let paste: Vec<_> = fp.pads.iter().filter(|p| p.number.is_empty()).collect();
        assert!(
            paste
                .iter()
                .all(|p| near(p.width, 0.97) && near(p.height, 0.97))
        );
        // The leads are those of the plain package of the same name.
        let plain = generate("Package_QFP:LQFP-48_7x7mm_P0.5mm").unwrap();
        assert_eq!(fp.pads[0], plain.pads[0]);
        assert_eq!(fp.courtyard, plain.courtyard);
    }

    #[test]
    fn a_bga_has_circular_pads_on_the_grid_lettered_from_the_top_left_without_i_o_q_s_x_z() {
        let fp = generate("Package_BGA:BGA-144_13.0x13.0mm_Layout12x12_P1.0mm").unwrap();
        assert_eq!(fp.pads.len(), 144);
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        assert!(near(at("A1").x, -5.5) && near(at("A1").y, -5.5));
        // Row letters: A B C D E F G H J K L M; the twelfth row is M.
        assert!(near(at("M12").x, 5.5) && near(at("M12").y, 5.5));
        assert!(fp.pads_numbered("I1").next().is_none());
        assert_eq!(fp.pads[0].shape, PadShape::Circle);
        assert!(near(fp.pads[0].width, 0.5));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-7.5, -7.5), Pt::new(7.5, 7.5))
        );
        // After Y the rows continue AA, AB, ...
        let big = generate("Package_BGA:BGA-1156_35.0x35.0mm_Layout34x34_P1.0mm").unwrap();
        assert!(big.pads_numbered("AA1").next().is_some());
        assert!(big.pads_numbered("AN34").next().is_some());
        assert!(big.pads_numbered("AO1").next().is_none());
    }

    #[test]
    fn a_dpak_has_its_tab_as_one_pad_with_four_paste_openings_of_the_same_number() {
        let fp = generate("Package_TO_SOT_SMD:TO-252-3_TabPin2").unwrap();
        // Three leads, and the tab: pad 2 is the middle lead and the tab, which is a copper
        // pad with the mask opening and four copper pads that carry the paste.
        assert_eq!(fp.pad_numbers(), ["1", "2", "3"]);
        assert_eq!(fp.pads_numbered("2").count(), 1 + 1 + 4);
        let tab = fp
            .pads_numbered("2")
            .find(|p| p.layers == ["F.Cu", "F.Mask"])
            .unwrap();
        assert!(near(tab.at.x, 1.26) && near(tab.width, 6.4) && near(tab.height, 5.8));
        let paste: Vec<_> = fp
            .pads_numbered("2")
            .filter(|p| p.layers == ["F.Cu", "F.Paste"])
            .collect();
        assert_eq!(paste.len(), 4);
        assert!(
            paste
                .iter()
                .all(|p| near(p.width, 3.05) && near(p.height, 2.75))
        );
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-6.39, -3.5), Pt::new(4.71, 3.5))
        );
        // The same tab on a package with the tab as its fourth pin, or its sixth.
        assert_eq!(
            generate("Package_TO_SOT_SMD:TO-252-5_TabPin6")
                .unwrap()
                .pads_numbered("6")
                .count(),
            5
        );
        let four_pin = generate("Package_TO_SOT_SMD:TO-252-4").unwrap();
        let mut four = four_pin.pad_numbers();
        four.sort();
        assert_eq!(four, ["1", "2", "3", "4", "5"]);
    }

    #[test]
    fn a_d2pak_has_the_dpak_structure_on_the_pg_to263_pads() {
        let fp = generate("Package_TO_SOT_SMD:TO-263-5_TabPin3").unwrap();
        // Five leads, the third sharing its number with the tab (copper, and four paste
        // openings of the same number).
        assert_eq!(fp.pads_numbered("3").count(), 1 + 1 + 4);
        let tab = fp
            .pads_numbered("3")
            .find(|p| p.layers == ["F.Cu", "F.Mask"] && p.width > 5.0)
            .unwrap();
        assert!(near(tab.at.x, 1.5) && near(tab.width, 9.4) && near(tab.height, 10.8));
        let lead = fp.pads_numbered("1").next().unwrap();
        assert!(near(lead.at.x, -7.65) && near(lead.at.y, -3.4));
        assert!(near(lead.width, 4.6) && near(lead.height, 1.1));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-10.2, -5.65), Pt::new(6.45, 5.65))
        );
        // The nine-pin parts' stencil openings are not pads: no number, no copper.
        let nine = generate("Package_TO_SOT_SMD:TO-263-9_TabPin10").unwrap();
        let openings: Vec<_> = nine.pads.iter().filter(|p| p.number.is_empty()).collect();
        assert_eq!(openings.len(), 4);
        assert!(openings.iter().all(|p| p.layers == ["F.Paste"]));
    }

    #[test]
    fn a_sot_89_has_a_tab_pad_that_reaches_beyond_its_anchor() {
        let fp = generate("Package_TO_SOT_SMD:SOT-89-3").unwrap();
        let mut numbers = fp.pad_numbers();
        numbers.sort();
        assert_eq!(numbers, ["1", "2", "3"]);
        let tab = fp.pads_numbered("2").next().unwrap();
        // The anchor is 1.475 mm wide at x = -1.8625; the polygon runs on to x = 2.0.
        assert!(near(tab.width, 1.475) && near(tab.at.x, -1.8625));
        assert!(near(tab.bounds().max.x, 2.0), "{:?}", tab.bounds());
        assert!(near(tab.bounds().min.x, -2.6), "{:?}", tab.bounds());
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-2.85, -2.5), Pt::new(2.25, 2.5))
        );
    }

    #[test]
    fn a_smd_crystal_has_the_recorded_pads_and_the_courtyard_of_its_drawing() {
        let fp = generate("Crystal:Crystal_SMD_3215-2Pin_3.2x1.5mm").unwrap();
        assert_eq!(fp.pad_numbers(), ["1", "2"]);
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        // Pad 1 is on the right in this drawing (Epson FC-135).
        assert!(near(at("1").x, 1.25) && near(at("2").x, -1.25));
        assert!(near(fp.pads[0].width, 1.0) && near(fp.pads[0].height, 1.8));
        assert_eq!(fp.pads[0].shape, PadShape::Rect);
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-2.0, -1.15), Pt::new(2.0, 1.15))
        );
        let four = generate("Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm").unwrap();
        assert_eq!(four.pads.len(), 4);
    }
}
