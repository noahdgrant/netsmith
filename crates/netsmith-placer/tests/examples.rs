//! The three example boards, from their design files to a placed board, a ratsnest and files.
//!
//! The first tests use the footprints in `fixtures/`, so they run anywhere. The last ones
//! use the installed KiCad libraries and are skipped when there are none. (The router's
//! `examples` take these boards on to routed copper.)

use netsmith_pcb::{render_svg, Board, FootprintLibrary, Ratsnest, kicad_pcb};

use crate::common;

use common::fixtures;

/// The example `example` as a board the placer laid out. The regulator example's design file is
/// made with the real catalogs, whose regulator is a SOT-23 part; these tests count the pads and
/// lines of the SOT-223 that the sample catalogs pick (`Catalogs::sample()`, as KiCad's DRC was
/// checked with), so that part's package is the sample's.
fn board(example: &str, library: &FootprintLibrary) -> Board {
    let mut design = common::example_design(example);
    for part in design.parts.iter_mut().filter(|p| p.kind == "LinearRegulator") {
        part.package = Some("SOT-223".into());
    }
    netsmith_placer::board_from_design(&design, &netsmith_kicad::Mapping::standard(), library)
        .unwrap()
}

/// Example, how many parts it has, and how many lines KiCad's DRC reports as unconnected
/// on the file written for it (checked with KiCad 9.0.0).
const EXAMPLES: [(&str, usize, usize); 3] = [
    ("led", 3, 3),
    ("status_indicator", 5, 6),
    ("linear_regulator", 5, 9),
];

#[test]
fn each_example_is_placed_without_overlaps_or_parts_off_the_board() {
    for (example, parts, _) in EXAMPLES {
        let board = board(example, &fixtures());
        assert_eq!(board.parts.len(), parts, "{example}");
        assert_eq!(board.check_placement(), vec![], "{example}");
    }
}

#[test]
fn each_examples_ratsnest_has_the_lines_kicad_counts() {
    for (example, _, lines) in EXAMPLES {
        let board = board(example, &fixtures());
        let rats = Ratsnest::of(&board);
        assert_eq!(rats.len(), lines, "{example}");
        assert!(rats.total_length() > 0.0, "{example}");
    }
}

#[test]
fn each_example_writes_the_same_files_every_time() {
    for (example, _, _) in EXAMPLES {
        let a = board(example, &fixtures());
        let b = board(example, &fixtures());
        assert_eq!(kicad_pcb(&a), kicad_pcb(&b), "{example}");
        assert_eq!(render_svg(&a), render_svg(&b), "{example}");
    }
}

#[test]
fn the_regulators_tab_and_pin_carry_the_output_net() {
    let text = kicad_pcb(&board("linear_regulator", &fixtures()));
    let u1 = text
        .split("(footprint ")
        .find(|f| f.contains("\"U1\""))
        .unwrap();
    assert_eq!(u1.matches("(net 1 \"3V3\")").count(), 2);
}

fn installed() -> Option<FootprintLibrary> {
    let library = FootprintLibrary::installed();
    if library.root().is_dir() {
        Some(library)
    } else {
        eprintln!(
            "skipped: no KiCad footprint libraries at {}",
            library.root().display()
        );
        None
    }
}

#[test]
fn with_the_installed_libraries_each_example_is_valid_and_has_the_same_ratsnest() {
    let Some(library) = installed() else { return };
    for (example, parts, lines) in EXAMPLES {
        let board = board(example, &library);
        assert_eq!(board.parts.len(), parts, "{example}");
        assert_eq!(board.check_placement(), vec![], "{example}");
        assert_eq!(Ratsnest::of(&board).len(), lines, "{example}");
    }
}

#[test]
fn every_footprint_the_mapping_names_is_in_the_installed_libraries() {
    let Some(library) = installed() else { return };
    for name in [
        "Resistor_SMD:R_0402_1005Metric",
        "Resistor_SMD:R_0603_1608Metric",
        "Resistor_SMD:R_0805_2012Metric",
        "Capacitor_SMD:C_0402_1005Metric",
        "Capacitor_SMD:C_0603_1608Metric",
        "Capacitor_SMD:C_0805_2012Metric",
        "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical",
        "Package_TO_SOT_SMD:SOT-223-3_TabPin2",
        "LED_SMD:LED_0402_1005Metric",
        "LED_SMD:LED_0603_1608Metric",
        "LED_SMD:LED_0805_2012Metric",
    ] {
        let footprint = library.load(name).unwrap_or_else(|e| panic!("{e}"));
        assert!(!footprint.pads.is_empty(), "{name}");
    }
}
