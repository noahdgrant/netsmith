//! `netsmith routes import` of a blind and a buried via (issue #1015): KiCad's `micro` and
//! `blind` vias are kept with the two layers they end on, the upper first, as the routes file
//! writes them, and a through via with none.

use netsmith_check::Loader;
use netsmith_pcb::{CopperLayer, Pt, Via, ViaSpan, kicad_pcb};
use netsmith_pipeline::{Libraries, build_with};
use netsmith_solve::Catalogs;

/// The six-layer HDI board of the guide's chapter on the process, placed.
fn hdi_board() -> netsmith_pcb::Board {
    let source = include_str!("fixtures/hdi_dense.nsm");
    let built = build_with(
        "hdi.nsm",
        source,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let libraries = Libraries::sample();
    netsmith_placer::board_from_design(&built[0].design, &libraries.mapping, &libraries.footprints)
        .unwrap()
}

#[test]
fn a_blind_and_a_buried_via_are_imported_with_their_layers() {
    let mut board = hdi_board();
    let at = |x: f64| board.outline.min + Pt::new(x, 1.0);
    let via = |at: Pt, span: Option<ViaSpan>, drill: f64, diameter: f64| Via {
        net: "GND".into(),
        at,
        diameter,
        drill,
        span,
    };
    let blind = ViaSpan {
        from: CopperLayer::Inner(4),
        to: CopperLayer::Back,
    };
    let buried = ViaSpan {
        from: CopperLayer::Inner(1),
        to: CopperLayer::Inner(4),
    };
    let vias = [
        via(at(1.0), Some(blind), 0.1, 0.25),
        via(at(2.0), Some(buried), 0.2, 0.45),
        via(at(3.0), None, 0.2, 0.45),
    ];
    board.vias.extend(vias);
    let pcb = kicad_pcb(&board);
    let import = netsmith_tools::routes_import::import(&board, &pcb, None, false)
        .unwrap_or_else(|e| panic!("{e}"));
    let gnd = &import.nets["GND"];
    let layers: Vec<Vec<String>> = gnd.vias.iter().map(|v| v.layers.clone()).collect();
    assert_eq!(
        layers,
        [
            vec!["inner4".to_string(), "bottom".to_string()],
            vec!["inner1".to_string(), "inner4".to_string()],
            vec![],
        ],
        "{:?}",
        import.skipped
    );
    assert_eq!(gnd.vias[0].drill, 0.1);
}

/// The board with its process's plane nets, and an outer pour on top with settings of its own.
fn poured_board(clearance: f64) -> netsmith_pcb::Board {
    let mut board = hdi_board();
    board.pours.push(netsmith_pcb::BoardPour {
        layer: CopperLayer::Front,
        net: "GND".into(),
        stitch_pitch: None,
        thermal_gap: Some(0.4),
        spoke_width: None,
        clearance: Some(clearance),
        min_width: None,
        pads: Some(netsmith_pcb::ZonePads::Solid),
    });
    board
}

#[test]
fn the_processs_own_planes_and_pours_are_left_out_and_said() {
    let board = poured_board(0.3);
    let own = board.planes.len() + board.pours.len();
    assert!(own >= 2, "the fixture has planes and a pour: {own}");
    let pcb = kicad_pcb(&board);
    let import = netsmith_tools::routes_import::import(&board, &pcb, None, false)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(import.process_zones, own);
    assert_eq!(import.zones(), 0, "{:?}", import.nets);
    let said = import
        .skipped
        .iter()
        .filter(|s| s.contains("the process's own plane or pour"))
        .count();
    assert_eq!(said, own, "{:?}", import.skipped);
    assert!(
        import.nets.values().all(|n| n.zones.is_empty()),
        "{:?}",
        import.nets
    );
}

#[test]
fn a_zone_that_differs_from_the_processs_is_imported_as_a_hand_zone() {
    // The KiCad board has the pour with a clearance of 0.3 mm, the process states 0.4 mm.
    let pcb = kicad_pcb(&poured_board(0.3));
    let board = poured_board(0.4);
    let own = board.planes.len() + board.pours.len();
    let import = netsmith_tools::routes_import::import(&board, &pcb, None, false)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(import.process_zones, own - 1);
    assert_eq!(import.zones(), 1, "{:?}", import.nets);
    let zone = &import.nets["GND"].zones[0];
    assert_eq!(zone.layers, ["top"]);
    assert_eq!(zone.clearance, 0.3);
}
