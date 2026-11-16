//! What `fab` hands the design rules check about the net and the height of a part (issue #761):
//! which nets are a supply or a ground, and which parts are tall. Every board here is invented.

use netsmith_pcb::{Board, place_options};
use netsmith_pipeline::{Libraries, build_with};
use netsmith_solve::Catalogs;

/// A rail with a divider on it, a test point on each of the supply, the ground and the divider's
/// middle, and three resistors: `r1` 8 mm tall, `r2` 6.4 mm tall (not above it) and `r3` with no
/// height stated.
const SOURCE: &str = "\
import std::parts::passive::Resistor
import std::parts::mechanical::TestPoint
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Widget {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .rail: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .r1: Resistor { .resistance &= 10kohm +/- 1% }
    .r2: Resistor { .resistance &= 10kohm +/- 1% }
    .r3: Resistor { .resistance &= 10kohm +/- 1% }
    .r1.height &= 8mm
    .r2.height &= 6.4mm
    .tp_vcc: TestPoint
    .tp_gnd: TestPoint
    .tp_mid: TestPoint
    .rail.pos ~> .r1 ~> .r2 ~> .rail.neg
    .rail.pos ~> .r3 ~> .rail.neg
    .tp_vcc.p1 ~ .rail.pos
    .tp_gnd.p1 ~ .rail.neg
    .tp_mid.p1 ~ .r1.b
}
";

fn board() -> Board {
    // No KiCad library: the resistor's and the test point's footprints are generated.
    let nowhere = netsmith_testkit::TempDir::new("marks-no-footprint-library");
    let libraries = Libraries::with_mapping(
        netsmith_kicad::Mapping::standard(),
        netsmith_kicad::SymbolLib::at(netsmith_testkit::sample_symbols()),
        netsmith_pcb::FootprintLibrary::installed_by(&netsmith_kicad::Search::under(nowhere.path())),
    );
    let built = build_with(
        "widget.nsm",
        SOURCE,
        &netsmith_check::Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0);
    netsmith_placer::board_from_design_placed(
        &built.design,
        &libraries.mapping,
        &libraries.footprints,
        &place_options(&built.design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_net_with_a_power_pin_is_a_supply_or_ground_net_and_the_divider_and_a_passive_only_net_are_not() {
    let mut board = board();
    // The resistors' pins are passive: no net is known as a supply yet.
    assert_eq!(netsmith_fabricate::fab::marks_of(&board).power_nets, Vec::<String>::new());
    // As if `r3.a` (on the rail's pos) and `r2.b` (on its neg) were a chip's supply and ground
    // pins, which its pin table calls `power` (ADR 0016).
    for (refdes, pad) in [("R3", "1"), ("R2", "2")] {
        let part = board.parts.iter_mut().find(|p| p.refdes == refdes).unwrap();
        part.power_pads.push(pad.to_string());
    }
    let marks = netsmith_fabricate::fab::marks_of(&board);
    let net_of = |refdes: &str| -> String {
        board
            .nets
            .iter()
            .find(|n| n.pads.iter().any(|p| p.refdes == refdes))
            .map(|n| n.name.clone())
            .unwrap_or_else(|| panic!("{refdes} is on no net"))
    };
    // The test points are numbered by their paths: `tp_gnd`, `tp_mid`, `tp_vcc`.
    let (gnd, mid, vcc) = (net_of("TP1"), net_of("TP2"), net_of("TP3"));
    assert_eq!(marks.power_nets.len(), 2, "{:?}", marks.power_nets);
    assert!(marks.power_nets.contains(&vcc), "{vcc} in {:?}", marks.power_nets);
    assert!(marks.power_nets.contains(&gnd), "{gnd} in {:?}", marks.power_nets);
    assert!(!marks.power_nets.contains(&mid), "{mid} in {:?}", marks.power_nets);
    assert_eq!(marks.test_points.len(), 3);
    assert_eq!(marks.min_power_test_point_pad, 1.0);
}

#[test]
fn a_part_is_tall_by_a_stated_height_above_6_4_mm_and_one_with_no_height_is_not() {
    let board = board();
    let marks = netsmith_fabricate::fab::marks_of(&board);
    // R1 is 8 mm; R2 is at the figure, not above it; R3 states none and is no one's guess.
    assert_eq!(marks.tall_parts, [("R1".to_string(), 8.0)]);
    assert_eq!(board.part("R3").unwrap().height, None);
    assert_eq!(marks.tall_part_height, 6.4);
    assert_eq!(marks.min_test_point_tall_body, 2.0);
}
