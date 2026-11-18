//! A diff pair and copper already on the board (issue #1295): a pair whose corridor has a routed
//! line of another net in it goes round it and still runs coupled, one net of a pair routed
//! beside its partner's copper (`netsmith routes route` of the second line, its partner fixed)
//! runs at the pair's pitch, and a board whose pairs leave connections gets one more try with
//! each pair routed as two separate lines. Measured by `netsmith board nets`'s table and by the
//! router's own counts, never by time.

use std::collections::BTreeSet;

use netsmith_core::NetPair;
use netsmith_fab::{NetTable, PairRow};
use netsmith_geom::Artwork;
use netsmith_pcb::{Board, CopperLayer, Pt, Track, kicad_pcb};
use netsmith_router::meander::SKEW_TARGET;
use netsmith_router::{RouteOptions, route_with};

use crate::impedance_targets::pair_board;

/// The least share of the pair that runs coupled on these boards: the rest is where the lines
/// part for their pads (as `pair_object`'s).
const COUPLED_SHARE: f64 = 0.55;

/// `A` and `B` of [`pair_board`] 1.2 mm apart, stated a pair with no spacing (a `DiffPair`).
fn stated() -> Board {
    let mut board = pair_board(1.2, 0.0);
    board.pairs = vec![NetPair {
        path: "pair".into(),
        p: "A".into(),
        n: "B".into(),
        skew: None,
    }];
    board
}

/// The pair `A`/`B` of `board` as `netsmith board nets` measures it.
fn measure(board: &Board) -> PairRow {
    let art = Artwork::parse(&kicad_pcb(board)).unwrap();
    let pairs = [("A".to_string(), "B".to_string(), SKEW_TARGET)];
    let table = NetTable::of(&art, &pairs, &[]);
    println!("{}", table.text());
    table.pairs[0].clone()
}

/// A routed line of net `N` on every layer from x = 8 to 16 mm at `y`, as fixed copper (a
/// routes file's).
fn routed_line(board: &mut Board, y: f64) {
    for layer in [
        CopperLayer::Front,
        CopperLayer::Inner(1),
        CopperLayer::Inner(2),
        CopperLayer::Back,
    ] {
        board.tracks.push(Track {
            net: "N".into(),
            layer,
            from: Pt::new(8.0, y),
            to: Pt::new(16.0, y),
            width: 0.2,
            mid: None,
        });
    }
    board.fixed.tracks = board.tracks.len();
}

/// The board with only the copper of `net` from `routed`, fixed (a routes file's).
fn with_fixed(board: &Board, routed: &Board, net: &str) -> Board {
    let mut b = board.clone();
    b.tracks
        .extend(routed.tracks.iter().filter(|t| t.net == net).cloned());
    b.vias
        .extend(routed.vias.iter().filter(|v| v.net == net).cloned());
    b.fixed.tracks = b.tracks.len();
    b.fixed.vias = b.vias.len();
    b
}

#[test]
fn a_pair_whose_corridor_has_a_routed_line_in_it_goes_round_it_and_runs_coupled() {
    // `N`'s line runs between the pair's pads on every layer: the pair goes round it on one
    // side, at its pitch, its lengths matched.
    let mut board = stated();
    routed_line(&mut board, 6.6);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    assert_eq!(routing.separate, None);
    let pair = measure(&board);
    assert!(pair.skew <= SKEW_TARGET, "{pair:?}");
    assert!(pair.share >= COUPLED_SHARE, "{:.0} % coupled", pair.share * 100.0);
}

#[test]
fn a_pair_routed_alone_round_a_routed_line_runs_coupled() {
    // The pair's two nets alone routed (`route_nets`, every other net held) with `N`'s line
    // fixed: the pair goes round it, coupled.
    let mut board = stated();
    routed_line(&mut board, 6.6);
    let one =
        netsmith_router::one::route_nets(&board, &BTreeSet::from(["A".into(), "B".into()]));
    assert!(one.complete(), "{:?}", one.left);
    board.tracks.extend(one.tracks);
    board.vias.extend(one.vias);
    let pair = measure(&board);
    assert!(pair.share >= COUPLED_SHARE, "{:.0} % coupled", pair.share * 100.0);
}

#[test]
fn the_second_line_of_a_pair_routed_beside_the_first_runs_at_the_pairs_pitch() {
    // `A` routed as part of the pair and kept (fixed), `B` ripped and routed again alone, as
    // `netsmith routes route B` does: before issue #1295 `B` ran straight, 1.2 mm off `A`, 0 %
    // coupled with a skew of 0.69 mm.
    let board = stated();
    let mut first = board.clone();
    let routing = route_with(&mut first, RouteOptions::default());
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
    let mut board = with_fixed(&board, &first, "A");
    let one = netsmith_router::one::route_nets(&board, &BTreeSet::from(["B".into()]));
    assert!(one.complete(), "{:?}", one.left);
    board.tracks.extend(one.tracks);
    board.vias.extend(one.vias);
    let pair = measure(&board);
    assert!(pair.share >= COUPLED_SHARE, "{:.0} % coupled", pair.share * 100.0);
    // At the pair's pitch: never nearer than the clearance, and within the router's grid of it.
    let clearance = board.clearance("B");
    let (least, _) = pair.gaps.unwrap();
    assert!(
        least >= clearance - 1e-6 && least <= clearance + 0.12 + 1e-6,
        "the least gap {least} mm, the clearance {clearance} mm"
    );
    // `B` grew to about `A`'s length, which a meander would have made it anyway: the pair is
    // within its skew with no meander (`routes route` draws none; `fab` does).
    assert!(pair.skew <= SKEW_TARGET, "{pair:?}");
}

/// A closed ring of `GND` copper on every layer round `R4`'s pad of `N` (as fixed copper),
/// which leaves `N` a line the router cannot route whatever the pair does.
fn walled_in(board: &mut Board) {
    let (x0, x1, y0, y1) = (18.05, 18.95, 6.75, 7.65);
    let corners = [
        Pt::new(x0, y0),
        Pt::new(x1, y0),
        Pt::new(x1, y1),
        Pt::new(x0, y1),
    ];
    for layer in [
        CopperLayer::Front,
        CopperLayer::Inner(1),
        CopperLayer::Inner(2),
        CopperLayer::Back,
    ] {
        for k in 0..4 {
            board.tracks.push(Track {
                net: "GND".into(),
                layer,
                from: corners[k],
                to: corners[(k + 1) % 4],
                width: 0.15,
                mid: None,
            });
        }
    }
    board.fixed.tracks = board.tracks.len();
}

#[test]
fn a_board_whose_pairs_leave_lines_gets_one_more_try_with_them_as_two_lines() {
    // The same board with and without the pair stated: the stated one is routed once more with
    // the pair as two lines, and keeps that try only if it leaves fewer lines (here it cannot).
    let options = || RouteOptions {
        rip_rounds: 0,
        ..RouteOptions::default()
    };
    let mut plain = pair_board(1.2, 0.0);
    walled_in(&mut plain);
    let plain = route_with(&mut plain, options());
    let mut paired = stated();
    walled_in(&mut paired);
    let paired = route_with(&mut paired, options());
    assert!(!plain.left.is_empty() && !paired.left.is_empty());
    assert_eq!(paired.tries, plain.tries + 1, "one try more");
    assert_eq!(paired.separate, None, "kept only if it leaves fewer lines");
    assert_eq!(paired.left.lines.len(), plain.left.lines.len());
    // With no classes a pair is routed as two lines already: no try more.
    let off = || RouteOptions {
        classes: netsmith_router::LineClasses::Off,
        ..options()
    };
    let mut plain_off = pair_board(1.2, 0.0);
    walled_in(&mut plain_off);
    let mut paired_off = stated();
    walled_in(&mut paired_off);
    assert_eq!(
        route_with(&mut paired_off, off()).tries,
        route_with(&mut plain_off, off()).tries
    );
}
