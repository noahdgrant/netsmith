//! A net whose rail states a `.min_clearance` (issue #758) keeps that space from copper of every
//! other net: the router keeps it, the `.kicad_pro` has a net class that states it, and
//! netsmith's own design rules check, which reads the class, finds the routed board clean.
//!
//! The board is a seeded one of `synthetic.rs`, with a clearance given to some of its nets.

use crate::common;

use netsmith_geom::{DrcRules, drc_check};
use netsmith_pcb::Board;
use netsmith_router::{RouteOptions, route_with};

use common::synthetic_board;

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

/// The least distance between copper of `net`'s tracks and the tracks of other nets on the
/// same layer, in mm.
fn least_gap(board: &Board, net: &str) -> f64 {
    let mut least = f64::INFINITY;
    for a in board.tracks.iter().filter(|t| t.net == net) {
        for b in board
            .tracks
            .iter()
            .filter(|t| t.net != net && t.layer == a.layer)
        {
            let centres = [
                a.distance_to(b.from),
                a.distance_to(b.to),
                b.distance_to(a.from),
                b.distance_to(a.to),
            ]
            .into_iter()
            .fold(f64::INFINITY, f64::min);
            least = least.min(centres - a.width / 2.0 - b.width / 2.0);
        }
    }
    least
}

#[test]
fn nets_kept_further_route_completely_keep_their_clearance_and_pass_the_check() {
    let mut board = synthetic_board(4, 40, 8, 8, 20, 20);
    let kept: Vec<String> = board
        .nets
        .iter()
        .filter(|n| n.name.starts_with('S'))
        .step_by(4)
        .take(4)
        .map(|n| n.name.clone())
        .collect();
    assert_eq!(kept.len(), 4);
    for net in &kept {
        board.net_clearances.push((net.clone(), 0.4));
    }
    assert!(board.rules.clearance < 0.4);
    let pro = netsmith_pcb::kicad_pro(&board);
    assert!(pro.contains(r#""name": "Clearance_0.4mm""#), "{pro}");
    let rules = DrcRules::from_project(&pro).unwrap();
    assert_eq!(rules.net_clearance(Some(&kept[0])), 0.4);
    // The witness: routed as if no net were kept further, the same board breaks the space.
    let mut blind = board.clone();
    blind.net_clearances.clear();
    route_with(&mut blind, RouteOptions::default());
    blind.net_clearances = board.net_clearances.clone();
    let broken = errors(&blind);
    assert!(
        broken.iter().any(|e| e.contains("[clearance]")),
        "the clearance changed nothing: {broken:?}"
    );
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert_eq!(errors(&board), Vec::<String>::new());
    for net in &kept {
        let gap = least_gap(&board, net);
        assert!(gap >= 0.4 - 1e-6, "{net}: {gap} mm from another net's track");
    }
}
