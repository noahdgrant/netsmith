//! Room for routing round a part (issue #1313): a board states `.routing_margin`, every part
//! that states none keeps it, a part's own beats it, and the placer keeps the sum of two parts'
//! margins between their courtyards. A margin is one value, zero or more. Every board here is
//! invented: three resistors on a rail, on a 30 by 20 mm board.

use netsmith_check::Loader;
use netsmith_pcb::Board;
use netsmith_pipeline::{Built, Libraries, build_with};
use netsmith_solve::Catalogs;

/// The board, with `extra` in the board's block.
fn source(extra: &str) -> String {
    format!(
        "import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Widget {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .rail: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }}
    .r1: Resistor {{ .resistance &= 15kohm +/- 1% }}
    .r2: Resistor {{ .resistance &= 15kohm +/- 1% }}
    .r3: Resistor {{ .resistance &= 15kohm +/- 1% }}
    .rail.pos ~> .r1 ~> .rail.neg
    .rail.pos ~> .r2 ~> .rail.neg
    .rail.pos ~> .r3 ~> .rail.neg
{extra}}}
"
    )
}

/// The outline and `extra`.
fn physical(extra: &str) -> String {
    format!(
        "import std::pcb::Outline

physical for Widget {{
    case {{
        .outline: Outline {{
            .width &= 30mm
            .height &= 20mm
        }}
    }}
{extra}}}
"
    )
}

fn build(source_extra: &str, physical_extra: &str) -> Result<Built, String> {
    let loader =
        Loader::std().with_physical("widget.nsm", "widget.physical.nsm", &physical(physical_extra));
    build_with(
        "widget.nsm",
        &source(source_extra),
        &loader,
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .map(|mut b| b.remove(0))
}

fn refused(source_extra: &str) -> String {
    match build(source_extra, "") {
        Ok(_) => panic!("`{source_extra}` builds"),
        Err(e) => e,
    }
}

fn margins(b: &Built) -> Vec<(&str, f64)> {
    b.design
        .features
        .routing_margins
        .iter()
        .map(|m| (m.path.as_str(), m.margin))
        .collect()
}

fn placed(b: &Built) -> Board {
    let libraries = Libraries::sample();
    netsmith_placer::board_from_design_placed(
        &b.design,
        &libraries.mapping,
        &libraries.footprints,
        &netsmith_pcb::place_options(&b.design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_board_with_no_margin_has_none() {
    let b = build("", "").unwrap();
    assert_eq!(margins(&b), []);
    assert!(placed(&b).parts.iter().all(|p| p.routing_margin == 0.0));
}

#[test]
fn every_part_keeps_the_boards_margin_and_a_parts_own_beats_it() {
    let b = build(
        "    .routing_margin &= 0.6mm\n    .r2.routing_margin &= 1mm\n    .r3.routing_margin &= 0mm\n",
        "",
    )
    .unwrap();
    assert_eq!(margins(&b), [("r1", 0.6), ("r2", 1.0)]);
    let board = placed(&b);
    let margin = |r: &str| board.parts.iter().find(|p| p.path == r).unwrap().routing_margin;
    assert_eq!((margin("r1"), margin("r2"), margin("r3")), (0.6, 1.0, 0.0));
    // The placer kept the sum of the two margins between every two courtyards.
    for (i, a) in board.parts.iter().enumerate() {
        for other in &board.parts[i + 1..] {
            assert_eq!(a.margin_short(other), None, "{} and {}", a.refdes, other.refdes);
        }
    }
    assert_eq!(board.check_placement(), vec![]);
}

#[test]
fn the_physical_file_states_a_margin_too() {
    let b = build("", "    .routing_margin &= 0.4mm\n    .r1.routing_margin &= 0.8mm\n").unwrap();
    assert_eq!(margins(&b), [("r1", 0.8), ("r2", 0.4), ("r3", 0.4)]);
}

#[test]
fn a_negative_margin_and_a_range_are_refused_with_the_path_and_the_number() {
    let e = refused("    .r1.routing_margin &= -0.2mm\n");
    assert!(
        e.contains("`r1.routing_margin` is -0.2 mm, and a margin for routing is zero or more"),
        "{e}"
    );
    let e = refused("    .routing_margin &= 0.2mm..0.5mm\n");
    assert!(
        e.contains("`routing_margin` is ") && e.contains("a range, and a margin for routing is one value: say it with `&= 0.2mm`"),
        "{e}"
    );
}
