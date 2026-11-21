//! A board with a `std::bus::Can` (issue #1193): the bus's two lines are a diff pair whatever
//! their nets are called, so the router routes them side by side with their lengths matched and
//! their vias alike, and the placer puts the bus's termination by the bus's connector. Placed and
//! routed as `fab` does, with no KiCad.

use std::path::Path;
use std::time::Duration;

use netsmith_check::Loader;
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_pcb::{Board, FootprintLibrary, place_options};
use netsmith_pipeline::{Libraries, build_with};
use netsmith_router::{RouteOptions, route_with};
use netsmith_solve::Catalogs;

/// An invented transceiver and an invented bus connector, a header for the logic side, and the
/// termination of the bus's end.
const NODE: &str = r#"import std::bus::{Can, CanTermination}
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x4

/// An invented CAN transceiver.
#[assembly(hand)]
part Xcvr {
    .designator &= "U"
    .can: Can
    .can.bitrate <= 1MHz
    .package: Package
    .package &= "SOT-23-5"
    pins "SOT-23-5" {
        .txd      input         "TXD"  "1"
        .gnd      power         "GND"  "2"
        .vcc      power         "VCC"  "3"
        .can.canl bidirectional "CANL" "4"
        .can.canh bidirectional "CANH" "5"
    }
}

/// An invented connector of the bus.
#[kind(connector)]
part BusPort {
    .designator &= "J"
    .can: Can
    .package: Package
    .package &= "2.54mm"
    pins "2.54mm" {
        .can.canh passive "H" "1"
        .can.canl passive "L" "2"
    }
}

board Node {
    .process: TwoLayer
    .process.stackup.thickness &= 1.6mm
    .logic: Header1x4
    .bus: BusPort
    .xcvr: Xcvr
    .term: CanTermination
    .xcvr.can.bitrate &= 500kHz
    .xcvr.can ~ .bus.can
    .term.bus ~ .bus.can
    .logic.p1 ~ .xcvr.vcc
    .logic.p2 ~ .xcvr.gnd
    .logic.p3 ~ .xcvr.txd
}
"#;

/// The two invented parts' rows, as a project's `kicad.toml` gives them.
const MAPPING: &str = r#"[[part]]
kind = "Xcvr"
package = "SOT-23-5"
footprint = "gen:SOT23(pins=5)"

[[part]]
kind = "BusPort"
package = "2.54mm"
symbol = "Connector_Generic:Conn_01x02"
footprint = "gen:PinHeader(pins=2, pitch=2.54mm, rows=1)"

[part.pin_names]
H = "Pin_1"
L = "Pin_2"
"#;

fn libraries() -> Libraries {
    let mapping = Mapping::standard()
        .merged(Mapping::parse(MAPPING, "kicad.toml", Path::new(".")).unwrap())
        .unwrap();
    Libraries::with_mapping(
        mapping,
        SymbolLib::at(netsmith_testkit::sample_symbols()),
        FootprintLibrary::at(netsmith_testkit::sample_footprints()),
    )
}

/// The board placed as `fab` places it.
fn placed() -> Board {
    let libraries = libraries();
    let built = build_with(
        "node.nsm",
        NODE,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0);
    let pairs = &built.design.features.pairs;
    assert_eq!(pairs.len(), 1, "{pairs:?}");
    netsmith_placer::board_from_design_placed(
        &built.design,
        &libraries.mapping,
        &libraries.footprints,
        &place_options(&built.design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// The least distance from a pad of `a` to a pad of `b`, centre to centre, in mm.
fn pad_distance(board: &Board, a: &str, b: &str) -> f64 {
    let (a, b) = (board.part(a).unwrap(), board.part(b).unwrap());
    let mut least = f64::MAX;
    for p in &a.footprint.pads {
        for q in &b.footprint.pads {
            least = least.min(a.pad_centre(p).distance(b.pad_centre(q)));
        }
    }
    least
}

/// The length of a net's tracks and its vias.
fn length_and_vias(board: &Board, net: &str) -> (f64, usize) {
    let length = board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .map(|t| t.length())
        .sum();
    (length, board.vias.iter().filter(|v| v.net == net).count())
}

#[test]
fn the_lines_of_a_can_bus_are_routed_as_a_pair_and_its_termination_is_by_the_connector() {
    let mut board = placed();
    // The nets are named by the interface, `net-bus.can.canh` and `net-bus.can.canl`: no suffix
    // pair of the router's says they are a pair, the interface does.
    let pairs = board.diff_pairs();
    assert_eq!(pairs.len(), 1, "{pairs:?}");
    let (h, l) = pairs[0].clone();
    assert!(h.ends_with("canh") && l.ends_with("canl"), "{h}, {l}");
    // The termination is found across the pair and pulled to the connector.
    let found = netsmith_pcb::termination_pairs(&board.parts, &board.nets, &pairs);
    assert_eq!(found.len(), 1, "{found:?}");
    let term = &board.parts[found[0].cap];
    assert_eq!(term.path, "term.resistor");
    let (to_connector, to_transceiver) = (
        pad_distance(&board, &term.refdes, "J1"),
        pad_distance(&board, &term.refdes, "U1"),
    );
    println!("the termination is {to_connector:.2} mm from J1 and {to_transceiver:.2} mm from U1");
    assert!(to_connector < to_transceiver, "{to_connector:.2} mm from J1, {to_transceiver:.2} mm from U1");
    let routing = route_with(
        &mut board,
        RouteOptions {
            time_limit: Some(Duration::from_secs(120)),
            ..RouteOptions::default()
        },
    );
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    let matched = routing
        .pairs
        .iter()
        .find(|m| m.p == h && m.n == l)
        .unwrap_or_else(|| panic!("{:?}", routing.pairs));
    println!("{matched:?}");
    // The bus's own skew limit, from its standard (`std::bus::Can.skew`, issue #1234).
    assert_eq!(matched.limit, CAN_SKEW, "{matched:?}");
    assert!(matched.after <= matched.limit, "{matched:?}");
    let ((lh, vh), (ll, vl)) = (length_and_vias(&board, &h), length_and_vias(&board, &l));
    println!("{h}: {lh:.2} mm, {vh} vias; {l}: {ll:.2} mm, {vl} vias");
    assert_eq!(vh, vl);
    // Neither line is longer than the board is round.
    let round = 2.0 * (board.outline.width() + board.outline.height());
    assert!(lh < round && ll < round, "{lh:.2} and {ll:.2} mm on a board {round:.2} mm round");
}

/// The board placed as `fab` places it, then spread along a board 72 mm long: the connector at
/// the left end with the termination beside it, the transceiver and its header 60 mm away at the
/// right end.
fn spread() -> Board {
    let mut board = placed();
    board.outline = netsmith_pcb::Rect::new(
        netsmith_pcb::Pt::new(0.0, 0.0),
        netsmith_pcb::Pt::new(72.0, 16.0),
    );
    board.polygon = None;
    let term = board
        .parts
        .iter()
        .position(|p| p.path == "term.resistor")
        .unwrap();
    for (i, part) in board.parts.iter_mut().enumerate() {
        part.at = match part.refdes.as_str() {
            "J1" => netsmith_pcb::Pt::new(5.0, 8.0),
            "U1" => netsmith_pcb::Pt::new(65.0, 8.0),
            _ if i == term => netsmith_pcb::Pt::new(9.0, 8.0),
            _ => netsmith_pcb::Pt::new(69.5, 4.0),
        };
    }
    board
}

/// The skew `std::bus::Can` offers its lines, in mm (issue #1234).
const CAN_SKEW: f64 = 2.5;

/// The least share of the CAN pair's first line that runs coupled on the spread board.
const COUPLED_SHARE: f64 = 0.8;

#[test]
fn a_can_pair_60_mm_long_is_routed_as_one_coupled_with_its_lengths_matched() {
    // Before issue #1223 the two lines ran 0.9 mm apart, centre to centre (not coupled), with a
    // skew of 2.8 mm that no meander took up.
    let mut board = spread();
    let (h, l) = board.diff_pairs()[0].clone();
    let routing = route_with(
        &mut board,
        RouteOptions {
            time_limit: Some(Duration::from_secs(120)),
            ..RouteOptions::default()
        },
    );
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    let table = netsmith_fabricate::net_table::of_board(&board).unwrap();
    print!("{}", table.text());
    let pair = &table.pairs[0];
    assert_eq!((pair.p.as_str(), pair.n.as_str()), (h.as_str(), l.as_str()));
    // The table's numbers are the router's.
    let matched = routing.pairs.iter().find(|m| m.p == h).unwrap();
    assert!((matched.after - pair.skew).abs() < 0.01, "{matched:?} {pair:?}");
    // The pair's lengths are its nets' connections; the tracks add up to each net's copper,
    // which also has what the length leaves out (issue #1511: a dead stub at a pad).
    let ((lh, vh), (ll, vl)) = (length_and_vias(&board, &h), length_and_vias(&board, &l));
    let row = |net: &str| table.nets.iter().find(|r| r.net == net).unwrap();
    let (rh, rl) = (row(&h), row(&l));
    assert!(
        (rh.length - pair.length_p).abs() < 0.01 && (rl.length - pair.length_n).abs() < 0.01,
        "{pair:?}"
    );
    assert!(
        (lh - rh.copper).abs() < 0.01 && (ll - rl.copper).abs() < 0.01,
        "{lh:.3} and {ll:.3} mm of track, {:.3} and {:.3} mm of copper",
        rh.copper,
        rl.copper
    );
    assert_eq!((vh, vl), (pair.vias_p, pair.vias_n));
    assert!(pair.length_p > 55.0, "{pair:?}");
    assert_eq!(pair.limit, CAN_SKEW, "{pair:?}");
    assert!(!pair.over && pair.skew <= CAN_SKEW, "{pair:?}");
    assert!(pair.share >= COUPLED_SHARE, "{:.0} % coupled", pair.share * 100.0);
    assert_eq!(pair.via_pairs, pair.vias_p.max(pair.vias_n), "{pair:?}");
}
