//! A diff pair routed as one object (issue #1223): every pair, stated or named, is pulled to one
//! pitch (its stated spacing, or its nets' clearance), its two lines run coupled at that gap, its
//! lengths are matched by a meander, and where it changes layer its two vias stand side by side.
//! Measured by the table `netsmith board nets` prints (`netsmith_fab::NetTable`), whose numbers
//! are the router's own: its skew is the skew the router says it left.

use netsmith_core::NetPair;
use netsmith_fab::{NetTable, PairRow};
use netsmith_geom::Artwork;
use netsmith_pcb::{Board, BoardKeepout, kicad_pcb};
use netsmith_router::meander::SKEW_TARGET;
use netsmith_router::{RouteOptions, Routing, route_with};

use crate::impedance_targets::pair_board;

/// The least share of a pair's first net that runs coupled on these boards: the rest is where
/// the lines part for their pads and the meander's bumps. On the skewed board the meander is a
/// third of the shorter net (about 6 mm of 17), and the pair runs coupled for 60 % of it.
const COUPLED_SHARE: f64 = 0.55;

/// `A` and `B` of [`pair_board`], stated a pair with no spacing (a `DiffPair`).
fn stated(apart: f64, out: f64) -> Board {
    let mut board = pair_board(apart, out);
    board.pairs = vec![NetPair {
        path: "pair".into(),
        p: "A".into(),
        n: "B".into(),
    }];
    board
}

/// Route `board` and measure its pair as `board nets` does.
fn route_and_measure(board: &mut Board) -> (Routing, PairRow) {
    let routing = route_with(board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    let art = Artwork::parse(&kicad_pcb(board)).unwrap();
    let table = NetTable::of(&art, &board.diff_pairs(), SKEW_TARGET);
    assert_eq!(table.pairs.len(), 1, "{table:?}");
    let pair = table.pairs[0].clone();
    println!("{}", table.text());
    (routing, pair)
}

/// The findings of netsmith's design rules check of the board that are a stub: a track end or a
/// via joined to nothing more of its net.
fn stubs(board: &Board) -> Vec<String> {
    let report = netsmith_geom::drc_check(
        &kicad_pcb(board),
        Some(&netsmith_pcb::kicad_pro(board)),
        &Default::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    report
        .findings
        .iter()
        .filter(|f| {
            matches!(
                f.rule,
                netsmith_geom::DrcRule::TrackDangling | netsmith_geom::DrcRule::ViaDangling
            )
        })
        .map(|f| f.to_string())
        .collect()
}

/// The table's skew is the one the router says it left, and its vias are the board's.
fn agrees(board: &Board, routing: &Routing, pair: &PairRow) {
    let matched = routing
        .pairs
        .iter()
        .find(|m| m.p == pair.p && m.n == pair.n)
        .unwrap_or_else(|| panic!("{:?}", routing.pairs));
    assert!(
        (matched.after - pair.skew).abs() < 0.01,
        "the router left {} mm, the table says {} mm",
        matched.after,
        pair.skew
    );
    let vias = |net: &str| board.vias.iter().filter(|v| v.net == net).count();
    assert_eq!((pair.vias_p, pair.vias_n), (vias(&pair.p), vias(&pair.n)));
    assert_eq!(stubs(board), Vec::<String>::new());
}

#[test]
fn a_stated_pair_whose_pads_are_apart_runs_coupled_at_one_gap_with_its_lengths_matched() {
    // `B`'s parts 3 mm below `A`'s and 2 mm further out to each side: the pair funnels from its
    // pads to its pitch and runs coupled between, and the meander takes up the 4 mm.
    let mut board = stated(3.0, 2.0);
    let (routing, pair) = route_and_measure(&mut board);
    agrees(&board, &routing, &pair);
    assert!(pair.skew <= SKEW_TARGET && !pair.over, "{pair:?}");
    assert!(pair.share >= COUPLED_SHARE, "{:.0} % coupled", pair.share * 100.0);
    // At its pitch: the least gap where the pair runs coupled is its nets' clearance with the
    // router's margin (0.02 mm), rounded up to its grid (0.1 mm), never under the clearance. (The
    // largest is where the two lines part for their pads or the meander.)
    let clearance = board.clearance("A");
    let (least, _) = pair.gaps.unwrap();
    assert!(
        least >= clearance - 1e-6 && least <= clearance + 0.12 + 1e-6,
        "the least gap {least} mm, the clearance {clearance} mm"
    );
}

#[test]
fn the_same_nets_stated_no_pair_run_apart() {
    let mut board = pair_board(3.0, 2.0);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    let art = Artwork::parse(&kicad_pcb(&board)).unwrap();
    let pairs = [("A".to_string(), "B".to_string())];
    let table = NetTable::of(&art, &pairs, SKEW_TARGET);
    println!("{}", table.text());
    assert!(table.pairs[0].share < COUPLED_SHARE, "{:?}", table.pairs[0]);
}

#[test]
fn a_pair_walled_off_on_the_top_changes_layer_through_vias_side_by_side() {
    // A wall across the board on the top: the pair goes under it, and each via of one net has a
    // via of the other beside it.
    let mut board = stated(1.2, 0.0);
    board.keepouts.push(BoardKeepout {
        name: "wall".into(),
        part: None,
        at: Some(netsmith_pcb::Pt::new(10.7, 0.0)),
        size: (0.6, 8.0),
        layers: vec!["top".into()],
        tracks: true,
        vias: false,
        copper: false,
        parts: false,
        max_height: None,
    });
    let (routing, pair) = route_and_measure(&mut board);
    agrees(&board, &routing, &pair);
    assert!(pair.vias_p >= 2 && pair.vias_p == pair.vias_n, "{pair:?}");
    assert_eq!(pair.via_pairs, pair.vias_p, "{pair:?}");
    assert!(pair.skew <= SKEW_TARGET, "{pair:?}");
    assert!(pair.share >= COUPLED_SHARE, "{:.0} % coupled", pair.share * 100.0);
}
