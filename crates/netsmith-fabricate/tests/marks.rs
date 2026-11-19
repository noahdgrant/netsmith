//! What `fab` hands the design rules check about the net and the height of a part (issue #761):
//! which nets are a supply or a ground, and which parts are tall. Every board here is invented.

use netsmith_pcb::{Board, place_options};
use netsmith_pipeline::{Libraries, build_with};
use netsmith_solve::Catalogs;

/// The shipped catalogs and an invented resistor row of 31.7 kohm (a value the shipped catalog
/// has no row of) whose `height` cell is empty: a part picked as it has no height, which is what
/// these tests are about. Every chip row of the shipped catalogs has a height (issue #1421).
fn catalogs() -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    c.add_file("resistor", "catalogs/resistor.csv", NO_HEIGHT_ROW)
        .unwrap();
    c
}

/// The invented row (an invented part number too), with no `height`.
const NO_HEIGHT_ROW: &str = "\
lcsc,mpn,resistance,max_power,package,height,library,source
C900317,ACME-R317,31.7kohm +/- 1%,62.5mW,0402,,Basic,https://example.invalid/acme.pdf
";

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
    .r3: Resistor { .resistance &= 31.7kohm +/- 1% }
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
        &catalogs(),
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

/// The net of the first pad of `refdes`.
fn net_of(board: &Board, refdes: &str) -> String {
    board
        .nets
        .iter()
        .find(|n| n.pads.iter().any(|p| p.refdes == refdes))
        .map(|n| n.name.clone())
        .unwrap_or_else(|| panic!("{refdes} is on no net"))
}

#[test]
fn the_nets_of_a_rail_are_supply_and_ground_nets_though_only_passive_parts_are_on_them() {
    let board = board();
    // The resistors' pins are passive, and the rail's two nets have nothing but resistors and
    // test points on them: the design's rails (issue #1006) are what makes them supply nets.
    assert!(board.parts.iter().all(|p| p.power_pads.is_empty()));
    let marks = netsmith_fabricate::fab::marks_of(&board);
    // The test points are numbered by their paths: `tp_gnd`, `tp_mid`, `tp_vcc`.
    let (gnd, mid, vcc) = (net_of(&board, "TP1"), net_of(&board, "TP2"), net_of(&board, "TP3"));
    assert_eq!(board.rail_nets, ["GND", "VCC"]);
    assert_eq!(marks.power_nets.len(), 2, "{:?}", marks.power_nets);
    assert!(marks.power_nets.contains(&vcc), "{vcc} in {:?}", marks.power_nets);
    assert!(marks.power_nets.contains(&gnd), "{gnd} in {:?}", marks.power_nets);
    // The divider's middle is on no rail.
    assert!(!marks.power_nets.contains(&mid), "{mid} in {:?}", marks.power_nets);
    assert_eq!(marks.test_points.len(), 3);
    assert_eq!(marks.min_power_test_point_pad, 1.0);
}

#[test]
fn a_net_with_a_power_pin_is_a_supply_or_ground_net_without_a_rail_too() {
    let mut board = board();
    // A board that is not given the design's rails (one read from a file): the resistors' pins
    // are passive and no net is known as a supply.
    board.rail_nets.clear();
    assert_eq!(netsmith_fabricate::fab::marks_of(&board).power_nets, Vec::<String>::new());
    // As if `r3.a` (on the rail's pos) and `r2.b` (on its neg) were a chip's supply and ground
    // pins, which its pin table calls `power` (ADR 0016).
    for (refdes, pad) in [("R3", "1"), ("R2", "2")] {
        let part = board.parts.iter_mut().find(|p| p.refdes == refdes).unwrap();
        part.power_pads.push(pad.to_string());
    }
    let marks = netsmith_fabricate::fab::marks_of(&board);
    let (gnd, mid, vcc) = (net_of(&board, "TP1"), net_of(&board, "TP2"), net_of(&board, "TP3"));
    assert_eq!(marks.power_nets.len(), 2, "{:?}", marks.power_nets);
    assert!(marks.power_nets.contains(&vcc), "{vcc} in {:?}", marks.power_nets);
    assert!(marks.power_nets.contains(&gnd), "{gnd} in {:?}", marks.power_nets);
    assert!(!marks.power_nets.contains(&mid), "{mid} in {:?}", marks.power_nets);
}

#[test]
fn the_finish_and_the_round_board_figures_come_from_the_process() {
    let board = board();
    let marks = netsmith_fabricate::fab::marks_of(&board);
    assert_eq!(marks.round_angle, 90.0);
    assert_eq!(marks.fine_pitch, 0.4);
    // TwoLayer's default finish is lead-free HASL; ENIG and OSP are flat.
    assert_eq!(marks.uneven_finish.as_deref(), Some("HASL lead-free"));
    let mut enig = board.clone();
    enig.process.finish = "ENIG".into();
    assert_eq!(netsmith_fabricate::fab::marks_of(&enig).uneven_finish, None);
    enig.process.finish = "HASL leaded".into();
    assert_eq!(
        netsmith_fabricate::fab::marks_of(&enig).uneven_finish.as_deref(),
        Some("HASL leaded")
    );
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

#[test]
fn the_least_assembly_board_comes_from_the_process_and_a_board_may_raise_it() {
    // JLCPCB's assembly capabilities page (issue 1285): 10 mm for one face, 70 mm for both.
    let board = board();
    let marks = netsmith_fabricate::fab::marks_of(&board);
    assert_eq!(marks.min_single_sided_assembly_size, Some(10.0));
    assert_eq!(marks.min_double_sided_assembly_size, Some(70.0));
    // The figures' source is the process's (issue 1393): the page, the services and the day.
    let source = marks.assembly_size_source.as_deref().expect("a source");
    assert!(
        source.contains("https://jlcpcb.com/capabilities/pcb-assembly-capabilities")
            && source.contains("Standard PCBA")
            && source.contains("read 2026-11-19"),
        "{source}"
    );
    // A process that states none (the model's `None`) is not judged.
    let mut none = board.clone();
    none.process.min_single_sided_assembly_size = None;
    none.process.min_double_sided_assembly_size = None;
    none.process.assembly_size_source = None;
    let marks = netsmith_fabricate::fab::marks_of(&none);
    assert_eq!(marks.min_single_sided_assembly_size, None);
    assert_eq!(marks.min_double_sided_assembly_size, None);
    assert_eq!(marks.assembly_size_source, None);
}
