//! A load switch in the power tree (issue 1269): its output rail is supplied, the switch is an
//! edge from its input's rail to its output's, and the input's rail draws the output's load. A
//! switch whose enable cannot turn it on supplies nothing, and the note of its output says why.
//! The IC is invented.

use netsmith_check::Loader;
use netsmith_editor::api::EdgeKind;
use netsmith_solve::Catalogs;

const SRC: &str = r#"
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::parts::switch::LoadSwitch
import std::mfg::jlcpcb::TwoLayer
import std::sources::Rail

/// Zsw, an invented load switch IC.
part Zsw provides LoadSwitch {
    .on_resistance &= 40mohm..80mohm
    .max_current &= 2A
    .quiescent_current &= 0A..10uA
    pins {
        .in.pos  power "VIN"  "1"
        .in.neg  power "GND"  "2"
        .enable  input "ON"   "3"
        .out.pos power "VOUT" "4"
    }
}

board Bench {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2

    .sw: Zsw
    .supply ~ .sw.in
    .sw.enable ~ .supply.pos
    .sw.out.pos.net &= "VSW"
    .load: Resistor { .resistance &= 1kohm +/- 1% }
    .sw.out.pos ~> .load ~> .sw.out.neg
}
"#;

fn built(src: &str) -> netsmith_pipeline::Outcome {
    netsmith_pipeline::outcome_with("bench.nsm", src, &Loader::std(), &Catalogs::jlcpcb(), None)
        .unwrap_or_else(|p| panic!("{}", netsmith_pipeline::render_problems(&p)))
}

#[test]
fn a_load_switch_that_can_turn_on_supplies_its_rail_and_loads_its_input() {
    let outcome = built(SRC);
    let board = &outcome.boards[0];
    let tree = netsmith_pipeline::power::board_tree(&board.trace.model, &board.trace.sources);
    assert!(
        tree.edges.iter().any(|e| e.from == "VIN"
            && e.to == "VSW"
            && e.kind == EdgeKind::Converts
            && e.label == "sw (Zsw)"),
        "{:?}",
        tree.edges
    );
    let vsw = tree.nodes.iter().find(|n| n.net == "VSW").unwrap();
    assert_eq!(vsw.supplies.len(), 1, "{vsw:?}");
    assert_eq!(vsw.capacity.as_ref().map(|c| c.hi), Some(2.0), "{vsw:?}");
    // The load is the resistor's, about 5 V over 1 kohm, and the input's rail draws it.
    let vin = tree.nodes.iter().find(|n| n.net == "VIN").unwrap();
    let draw = vin.loads.iter().find(|d| d.who == "sw.in").unwrap();
    let range = draw.range.as_ref().unwrap();
    assert!(range.lo > 0.0045 && range.hi < 0.0054, "{range:?}");
    let notes = board.power_notes();
    assert!(notes.iter().all(|n| !n.contains("has no source")), "{notes:?}");
}

#[test]
fn a_load_switch_whose_enable_is_grounded_supplies_nothing_and_says_why() {
    let src = SRC.replace(".sw.enable ~ .supply.pos", ".sw.enable ~ .supply.neg");
    let outcome = built(&src);
    let board = &outcome.boards[0];
    let tree = netsmith_pipeline::power::board_tree(&board.trace.model, &board.trace.sources);
    assert!(
        !tree.edges.iter().any(|e| e.to == "VSW"),
        "{:?}",
        tree.edges
    );
    let notes = board.power_notes();
    assert!(
        notes.iter().any(|n| n.contains(
            "VSW has no source: the load switch `sw` is off, since its enable `sw.enable` is held low"
        )),
        "{notes:?}"
    );
}
