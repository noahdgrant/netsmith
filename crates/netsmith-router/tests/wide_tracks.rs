//! A net that states a wide `.min_track` (a rail that carries current, ADR 0036) is routed at
//! that width, and what the router calls routed passes netsmith's own design rules check
//! (ADR 0037). Before this, a track wider than a via was masked too narrowly round other
//! copper, and a wide stub out of a fine-pitch pad touched the pad beside it: 68 errors on a
//! 70-part board with rails of 0.6 and 0.9 mm, 16 on 49 parts, shorts included.
//!
//! The boards are the seeded ones of `synthetic.rs`, with widths given to some of their nets.

use crate::common;

use netsmith_geom::{DrcExtras, DrcRule, drc_check};

use netsmith_pcb::{Board, BoardKeepout, Pt};
use netsmith_router::{RouteOptions, route_with};

use common::{hard_boards, synthetic_board};

/// The errors netsmith's check finds on `board`, as `fab` runs it.
fn errors(board: &Board) -> Vec<String> {
    let report = drc_check(
        &netsmith_pcb::kicad_pcb(board),
        Some(&netsmith_pcb::kicad_pro(board)),
        &Default::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    report.errors().map(|f| f.to_string()).collect()
}

/// Give four signal nets each 0.6 mm, 0.9 mm and 0.36 mm, a handful of high-current rails.
fn with_rails(mut board: Board) -> Board {
    let signal: Vec<String> = board
        .nets
        .iter()
        .filter(|n| n.name.starts_with('S'))
        .map(|n| n.name.clone())
        .collect();
    for (k, width) in [0.6, 0.9, 0.36].into_iter().enumerate() {
        for name in signal.iter().skip(k * 3).step_by(9).take(4) {
            board.net_widths.push((name.clone(), width));
        }
    }
    board
}

#[test]
fn boards_with_rails_of_0_6_and_0_9_mm_route_completely_and_pass_the_check() {
    // (seed, small parts, regulators, pads per side of the package, nets of the package,
    // other nets): 49 parts, 73 parts and 70 parts with a package of 80 pads.
    for (seed, shape) in [
        (4, (40, 8, 8, 20, 20)),
        (2, (60, 12, 12, 40, 30)),
        (1, (52, 17, 20, 72, 36)),
        (7, (52, 17, 20, 72, 36)),
    ] {
        let (chips, regs, per_side, ic, parts) = shape;
        let mut board = with_rails(synthetic_board(seed, chips, regs, per_side, ic, parts));
        let routing = route_with(&mut board, RouteOptions::default());
        let name = format!("seed {seed}, {} parts", board.parts.len());
        assert!(routing.left.is_empty(), "{name}: {:?}", routing.left.lines);
        assert_eq!(errors(&board), Vec::<String>::new(), "{name}");
        // The rails really were drawn wide, not all necked down.
        let wide = |w: f64| board.tracks.iter().filter(|t| t.width >= w - 1e-9).count();
        assert!(
            wide(0.6) > 5 && wide(0.9) > 5,
            "{name}: {} / {}",
            wide(0.6),
            wide(0.9)
        );
    }
}

#[test]
fn a_wide_rail_on_the_planes_nets_passes_the_check_too() {
    let mut board = synthetic_board(1, 40, 8, 8, 20, 20);
    board.net_widths.push(("3V3".into(), 0.63));
    board.net_widths.push(("GND".into(), 0.86));
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert_eq!(errors(&board), Vec::<String>::new());
}

#[test]
fn a_board_that_cannot_be_routed_says_so_and_what_it_drew_passes_the_check() {
    // Eight parts, and a wall of keepout between the left and the right half that no track
    // may cross on either face: the nets that join the halves cannot be routed.
    let mut board = hard_boards(1).remove(0);
    board.keepouts.push(BoardKeepout {
        name: "wall".into(),
        part: None,
        at: Some(Pt::new(8.2, 0.0)),
        size: (0.6, 20.0),
        layers: vec!["top".into(), "bottom".into()],
        tracks: true,
        vias: true,
        copper: false,
        parts: false,
        max_height: None,
        drawn: false,
    });
    for net in board
        .nets
        .iter()
        .map(|n| n.name.clone())
        .collect::<Vec<_>>()
    {
        board.net_widths.push((net, 0.6));
    }
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(!routing.left.is_empty(), "the wall cannot be crossed");
    assert!(
        routing.left.lines.iter().all(|l| !l.net.is_empty()),
        "each line left is named"
    );
    let found = errors(&board);
    assert!(
        found
            .iter()
            .all(|e| !e.contains("[clearance]") && !e.contains("[shorting_items]")),
        "{found:?}"
    );
}

/// The same rails, sized by their current: the width is the least their current needs.
fn with_currents(mut board: Board) -> Board {
    board = with_rails(board);
    board.net_currents = board
        .net_widths
        .iter()
        .map(|(net, width)| netsmith_core::NetCurrent {
            net: net.clone(),
            current: 1.0,
            least_width: *width,
            vias: 1,
        })
        .collect();
    board
}

#[test]
fn a_rail_sized_by_its_current_is_necked_down_only_near_its_own_pads() {
    for (seed, shape) in [(4, (40, 8, 8, 20, 20)), (1, (52, 17, 20, 72, 36))] {
        let (chips, regs, per_side, ic, parts) = shape;
        let mut board = with_currents(synthetic_board(seed, chips, regs, per_side, ic, parts));
        let routing = route_with(&mut board, RouteOptions::default());
        let name = format!("seed {seed}, {} parts", board.parts.len());
        assert!(routing.left.is_empty(), "{name}: {:?}", routing.left.lines);
        let reach = netsmith_router::route::NECK_REACH;
        for t in &board.tracks {
            let least = board.least_width(&t.net);
            if t.width >= least - 1e-9 {
                continue;
            }
            // Near the pads of a part on the net: the box round their centres, grown by the
            // reach.
            let boxes: Vec<netsmith_pcb::Rect> = board
                .nets
                .iter()
                .filter(|n| n.name == t.net)
                .flat_map(|n| n.pads.iter())
                .filter_map(|p| board.neck_box(&p.refdes))
                .map(|b| b.expanded(reach + 1e-6))
                .collect();
            let near = |p: Pt| {
                boxes
                    .iter()
                    .any(|b| b.contains(&netsmith_pcb::Rect::new(p, p)))
            };
            assert!(
                near(t.from) && near(t.to),
                "{name}: {} is {} mm wide, under its {least} mm, away from its pads: {t:?}",
                t.net,
                t.width
            );
        }
        // The design rules check, told the least widths, agrees.
        let extras = DrcExtras {
            least_widths: board.least_widths(),
            neck_reach: reach,
            ..DrcExtras::default()
        };
        let report = drc_check(
            &netsmith_pcb::kicad_pcb(&board),
            Some(&netsmith_pcb::kicad_pro(&board)),
            &extras,
        )
        .unwrap();
        assert_eq!(report.count(DrcRule::TrackCurrent), 0, "{name}\n{report}");
    }
}

#[test]
fn a_rail_whose_current_one_via_does_not_carry_changes_layer_through_vias_side_by_side() {
    let mut board = with_currents(synthetic_board(4, 40, 8, 8, 20, 20));
    for c in &mut board.net_currents {
        c.vias = 3;
    }
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    let rails: Vec<String> = board.net_currents.iter().map(|c| c.net.clone()).collect();
    let on_rails = board.vias.iter().filter(|v| rails.contains(&v.net)).count();
    assert!(on_rails >= 3, "the rails change layer: {on_rails} vias");
    assert_eq!(
        netsmith_router::parallel::short_clusters(&board),
        Vec::new(),
        "every via of a rail stands with two more"
    );
    let extras = DrcExtras {
        via_counts: board.via_counts(),
        ..DrcExtras::default()
    };
    let report = drc_check(
        &netsmith_pcb::kicad_pcb(&board),
        Some(&netsmith_pcb::kicad_pro(&board)),
        &extras,
    )
    .unwrap();
    assert_eq!(report.count(DrcRule::ViaCurrent), 0, "{report}");
    assert_eq!(errors(&board), Vec::<String>::new());
    // And the check counts: with four needed, the clusters of three are short.
    let more = DrcExtras {
        via_counts: board
            .via_counts()
            .into_iter()
            .map(|(n, _)| (n, 4))
            .collect(),
        ..DrcExtras::default()
    };
    let report = drc_check(
        &netsmith_pcb::kicad_pcb(&board),
        Some(&netsmith_pcb::kicad_pro(&board)),
        &more,
    )
    .unwrap();
    assert!(report.count(DrcRule::ViaCurrent) >= 1, "{report}");
}
