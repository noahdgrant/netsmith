//! Blind and buried vias (issue #1015): on an HDI process the router changes layer through the
//! via of fewest layers that joins the two, a laser-drilled blind via from an outer layer to the
//! inner layer under it, which keeps clear of copper on its own layers only; the board file has
//! it as a micro via between those layers, and netsmith's own design rules check passes it.

use netsmith_pcb::{CopperLayer, Ratsnest, ViaSpan, kicad_pcb};
use netsmith_router::{RouteOptions, route_with};

use crate::common;
use crate::inner_layers::walled;

/// The errors of netsmith's own design rules check (ADR 0037), with no KiCad.
fn own_findings(board: &netsmith_pcb::Board) -> Vec<String> {
    let report = netsmith_geom::drc_check(
        &kicad_pcb(board),
        Some(&netsmith_pcb::kicad_pro(board)),
        &Default::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    report.errors().map(|f| f.to_string()).collect()
}

#[test]
fn a_line_walled_off_outside_crosses_under_the_wall_through_blind_vias() {
    let mut board = walled(common::hdi_six_layer_process(&[]));
    assert_eq!(
        board.signal_layers(),
        vec![CopperLayer::Inner(1), CopperLayer::Inner(4)]
    );
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert_eq!(Ratsnest::of(&board).len(), 0);
    // S crosses on the signal layer under the top, `inner1`, and changes layer through the
    // blind via from `top` to it: a 0.1 mm laser hole in 0.25 mm of copper.
    assert!(
        board
            .tracks
            .iter()
            .any(|t| t.net == "S" && t.layer == CopperLayer::Inner(1)),
        "{:?}",
        board.tracks
    );
    let blind = ViaSpan {
        from: CopperLayer::Front,
        to: CopperLayer::Inner(1),
    };
    let s_vias: Vec<_> = board.vias.iter().filter(|v| v.net == "S").collect();
    assert!(s_vias.len() >= 2, "{s_vias:?}");
    assert!(
        s_vias
            .iter()
            .all(|v| v.span == Some(blind) && v.drill == 0.1 && v.diameter == 0.25),
        "{s_vias:?}"
    );
    // The board file has each as KiCad's micro via between the two layers.
    let pcb = kicad_pcb(&board);
    assert!(
        pcb.contains("(via micro") && pcb.contains("(layers \"F.Cu\" \"In1.Cu\")"),
        "{pcb}"
    );
    assert_eq!(own_findings(&board), Vec::<String>::new());
}

#[test]
fn a_process_with_no_blind_vias_crosses_through_vias() {
    // The same board on a process that is not HDI: every via a through via, as before.
    let mut board = walled(common::four_layer_signal_process());
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert!(board.vias.iter().all(|v| v.span.is_none()));
    assert!(!kicad_pcb(&board).contains("(via micro"));
}

#[test]
fn a_walled_in_pad_is_reached_by_a_blind_via_in_the_pad_where_no_drilled_via_may_stand() {
    // Issue #1071: the HDI process fills and planarizes its blind vias by plating, so one stands
    // in a pad though the board takes no drilled via in a pad (`.via_in_pad &= 0mm`).
    let process = common::hdi_six_layer_process(&[".via_in_pad &= 0mm"]);
    let (mut board, walled) = common::land_grid_board_on(7, 12, 0.9, process);
    assert!(!board.rules.via_in_pad);
    assert!(!walled.is_empty());
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert_eq!(Ratsnest::of(&board).len(), 0);
    // Every via in a pad is the laser's blind via from the pad's face to the layer under it,
    // and every pad that was walled in has one of its own net.
    let blind = ViaSpan {
        from: CopperLayer::Front,
        to: CopperLayer::Inner(1),
    };
    let in_pads = board.vias_in_pads();
    for (k, pad) in &in_pads {
        let v = &board.vias[*k];
        assert_eq!(
            (v.span, v.diameter, v.drill),
            (Some(blind), 0.25, 0.1),
            "the via in {pad}"
        );
    }
    for pad in &walled {
        assert!(
            in_pads
                .iter()
                .any(|(k, p)| p.refdes == "U1" && &p.number == pad && board.vias[*k].net.starts_with('S')),
            "pad {pad} has no blind via of its own: {in_pads:?}"
        );
    }
    assert_eq!(own_findings(&board), Vec::<String>::new());
    // The notes say the pads have blind vias, which need no epoxy fill, and ask for no drilled
    // via in a pad.
    let extras = board.process_extras();
    assert!(
        !extras.iter().any(|e| e.starts_with("vias in pads")),
        "{extras:?}"
    );
    let line = extras
        .iter()
        .find(|e| e.starts_with("blind vias in pads"))
        .unwrap_or_else(|| panic!("{extras:?}"));
    assert!(
        line.starts_with(&format!("blind vias in pads: {} blind via(s)", in_pads.len()))
            && line.contains("U1.")
            && line.contains("fills and planarizes its blind vias by plating"),
        "{line}"
    );
}
