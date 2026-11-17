//! A board gets the same placement and copper whatever its designators are (issue #1048): the
//! design stage orders a net's pins by the paths of their parts, not by designator, so the
//! placer's nets and the router's lines do not change when the parts are relettered or
//! renumbered. The two boards here differ only in the letters the board gives three parts of
//! its own, which moves them before and after the resistors and the header in designator order.

use crate::guard;

use netsmith_core::Design;
use netsmith_pcb::Board;
use netsmith_router::{RouteOptions, route_with};

const SOURCE: &str = r#"
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::mfg::jlcpcb::TwoLayer

part Chip {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
}

board Renamed {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .input: Header1x2
    .first: Chip
    .second: Chip
    .third: Chip
    .pull: Resistor {
        .resistance &= 10kohm +/- 1%
    }
    .series: Resistor {
        .resistance &= 1kohm +/- 1%
    }
LETTERS
    .input.p1 ~ .first.a
    .input.p1 ~ .third.a
    .input.p1 ~ .pull.a
    .first.b ~ .second.a
    .second.b ~ .series.a
    .series.b ~ .third.b
    .pull.b ~ .input.p2
    .second.b ~ .input.p2
}
"#;

/// The design of the board, with `letters` (statements of the board) in it.
fn design(letters: &str) -> Design {
    let source = SOURCE.replace("LETTERS", letters);
    let checked = netsmith_check::check("t.nsm", &source, &netsmith_check::Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = &checked.boards[0];
    let solution = netsmith_solve::solve(board, &netsmith_solve::Catalogs::jlcpcb());
    assert!(
        solution.diagnostics.is_empty(),
        "{:?}",
        solution.diagnostics
    );
    netsmith_design::design_board(board, &solution)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .design
}

/// The board placed and routed.
fn built(design: &Design) -> Board {
    let mapping = netsmith_kicad::Mapping::parse(
        "[[part]]\nkind = \"Chip\"\npackage = \"0603\"\ndefault = true\nsymbol = \"Device:R\"\nfootprint = \"Resistor_SMD:R_0603_1608Metric\"\n",
        "designators",
        std::path::Path::new("."),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let mut board =
        netsmith_placer::board_from_design(design, &mapping, &crate::common::fixtures())
            .unwrap_or_else(|e| panic!("{e}"));
    let routing = route_with(
        &mut board,
        RouteOptions {
            time_limit: Some(guard::GUARD),
            ..RouteOptions::default()
        },
    );
    guard::not_hung(&routing);
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    board
}

/// Each net's pins as the paths of their parts and the pads, in the design's order.
fn nets_by_path(design: &Design) -> Vec<(String, Vec<(String, String)>)> {
    let path = |refdes: &str| {
        design
            .parts
            .iter()
            .find(|p| p.refdes == refdes)
            .unwrap()
            .path
            .clone()
    };
    design
        .nets
        .iter()
        .map(|n| {
            let pins = n
                .pins
                .iter()
                .map(|p| (path(&p.refdes), p.pin.clone()))
                .collect();
            (n.name.clone(), pins)
        })
        .collect()
}

/// Where each part is, by path, and the copper, in an order of their own.
fn placement_and_copper(board: &Board) -> (Vec<String>, Vec<String>) {
    let mut places: Vec<String> = board
        .parts
        .iter()
        .map(|p| format!("{} {:?} {} {:?}", p.path, p.at, p.rotation, p.side))
        .collect();
    places.sort();
    let mut copper: Vec<String> = board
        .tracks
        .iter()
        .map(|t| format!("{t:?}"))
        .chain(board.vias.iter().map(|v| format!("{v:?}")))
        .collect();
    copper.sort();
    (places, copper)
}

#[test]
fn relettering_the_parts_changes_no_net_placement_or_copper() {
    let plain = design("");
    let lettered = design(
        "    .first.designator &= \"Z\"\n    .second.designator &= \"A\"\n    .third.designator &= \"B\"\n",
    );
    let refdes = |d: &Design| d.parts.iter().map(|p| p.refdes.clone()).collect::<Vec<_>>();
    assert_ne!(refdes(&plain), refdes(&lettered), "the designators differ");
    assert_eq!(nets_by_path(&plain), nets_by_path(&lettered));
    let (a, b) = (built(&plain), built(&lettered));
    assert!(!a.tracks.is_empty());
    let (places_a, copper_a) = placement_and_copper(&a);
    let (places_b, copper_b) = placement_and_copper(&b);
    assert_eq!(places_a, places_b, "the parts are placed the same");
    assert!(copper_a == copper_b, "the copper is the same");
}
