//! Tests of the search, the lanes and the windows.

use super::*;

/// A grid of 3 mm by 1 mm with nothing in the way.
pub(super) fn open_grid() -> (Grid, Masks) {
    let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(3.0, 1.0)));
    let masks = Masks::open(Window::full(&grid));
    (grid, masks)
}

/// A search that is never stopped.
fn find(masks: &Masks, from: &[Spot], goal: Goal) -> Option<Vec<Spot>> {
    search(masks, from, goal, Extras::default(), &|| false).ok()
}

/// A search for the nets in a line's way, as the router makes one: in four directions.
pub(super) fn find_soft(masks: &Masks, from: &[Spot], goal: Goal) -> Option<Vec<Spot>> {
    let extras = Extras {
        coarse: true,
        ..Extras::default()
    };
    search(masks, from, goal, extras, &|| false).ok()
}

/// How many rounds a negotiation that starts with `start` lines left runs, when its rounds
/// leave `rounds` lines: [`Router::negotiate`]'s stopping, replayed.
fn rounds_run(start: usize, rounds: &[usize]) -> usize {
    let mut best = start;
    for (round, &left) in rounds.iter().enumerate().take(NEGOTIATION_ROUNDS) {
        best = best.min(left);
        if left == 0 || spreads(best, left) {
            return round + 1;
        }
    }
    rounds.len().min(NEGOTIATION_ROUNDS)
}

#[test]
fn a_negotiation_that_improves_after_rounds_that_do_not_still_gets_the_round_that_does() {
    // Measured traces (lines left at the start, then after each round) of negotiations whose
    // best state came after rounds that left as many lines as it started with or more: Orbit
    // at the least track and at 0.15 mm (every turn cost from 24 to 27), and seeds of the
    // tight and mixed boards.
    let traces: [(usize, &[usize], usize); 7] = [
        (1, &[3, 2, 1, 1, 0], 5),
        (1, &[2, 2, 2, 1, 1, 0], 6),
        (1, &[1, 2, 2, 2, 0], 5),
        (7, &[10, 6, 5, 7, 7, 7], 3),
        (13, &[16, 17, 10, 10, 16, 16], 3),
        (11, &[17, 13, 14, 10, 13, 11], 4),
        (5, &[9, 7, 7, 4, 6, 7], 4),
    ];
    for (start, rounds, improves) in traces {
        assert!(
            rounds_run(start, rounds) >= improves,
            "{start} then {rounds:?} must run to round {improves}"
        );
    }
}

#[test]
fn a_negotiation_whose_rounds_spread_the_lines_left_stops() {
    // RP2350 at both track margins: every round leaves several times the lines it started
    // with, and none ever leaves fewer.
    assert_eq!(rounds_run(2, &[9, 16, 15, 16, 13, 19]), 1);
    assert_eq!(rounds_run(3, &[12, 11, 13, 15, 16, 15]), 1);
    assert_eq!(rounds_run(4, &[17, 16, 17, 13, 12, 19]), 1);
    assert_eq!(rounds_run(4, &[7, 12, 18, 11, 13, 16]), 2);
}

#[test]
fn a_straight_path_stays_on_its_layer() {
    let (grid, masks) = open_grid();
    let path = find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
    assert_eq!(path.len(), 31);
    assert!(path.iter().all(|&(_, j, layer)| j == 5 && layer == FRONT));
    let runs = runs(&grid, &path.iter().map(|s| (s.0, s.1)).collect::<Vec<_>>());
    assert_eq!(runs.len(), 1, "one straight track");
    assert_eq!(runs[0], (Pt::new(0.0, 0.5), Pt::new(3.0, 0.5)));
}

#[test]
fn a_wall_on_the_front_sends_the_path_through_the_back() {
    let (grid, mut masks) = open_grid();
    for j in 0..grid.ny {
        let at = masks.at(15, j);
        masks.track[FRONT][at] = true;
    }
    let path = find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
    assert!(path.iter().any(|s| s.2 == BACK));
    let changes = path.windows(2).filter(|w| w[0].2 != w[1].2).count();
    assert_eq!(changes, 2, "down to the back and up again");
}

/// Masks of the 3 by 1 mm grid with cells closed at random (a fixed sequence), about one
/// in `one_in` for tracks on each layer and for vias.
fn random_masks(seed: u64, one_in: u64) -> (Grid, Masks) {
    let (grid, mut masks) = open_grid();
    let mut x = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut next = || {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (x >> 33).is_multiple_of(one_in)
    };
    for c in 0..masks.window.cells() {
        masks.track[FRONT][c] = next();
        masks.track[BACK][c] = next();
        masks.via[c] = next();
    }
    (grid, masks)
}

#[test]
fn the_flood_finds_a_way_exactly_when_the_search_finds_a_path() {
    let modes = [(true, true), (true, false), (false, false)];
    let (mut joined, mut apart) = (0, 0);
    for seed in 0..120 {
        let (_, masks) = random_masks(seed, 2 + seed % 3);
        let from = [(0, (seed % 10) as usize, FRONT)];
        let to = [(29, ((seed * 7) % 10) as usize, (seed % 2) as usize)];
        for goal in [Goal::Spots(&to), Goal::Via] {
            for (right_angles, coarse) in modes {
                let extras = Extras {
                    right_angles,
                    coarse,
                    ..Extras::default()
                };
                let found = search(&masks, &from, goal, extras, &|| false).is_ok();
                let flood = reachable(&masks, &from, goal, extras, &|| false);
                assert_eq!(flood, Ok(found), "seed {seed}, {right_angles} {coarse}");
                if found {
                    joined += 1;
                } else {
                    apart += 1;
                }
            }
        }
    }
    assert!(joined > 50 && apart > 50, "{joined} joined, {apart} apart");
}

#[test]
fn a_pairs_net_goes_out_of_its_way_to_run_beside_its_partner() {
    // On a grid of 10 mm by 1.5 mm, from row 2 at the left to row 2 at the right. The
    // partner's cells are row 6, from column 10 to 90: a step there costs half, and with the
    // estimate cut to match, the path leaves its straight way to run on them.
    let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(10.0, 1.5)));
    let mut masks = Masks::open(Window::full(&grid));
    let mut beside = vec![false; masks.window.cells()];
    for i in 10..=90 {
        beside[masks.at(i, 6)] = true;
    }
    masks.follow = [beside, vec![false; masks.window.cells()]];
    masks.estimate = FOLLOW;
    let path = find(&masks, &[(0, 2, FRONT)], Goal::Spots(&[(100, 2, FRONT)])).unwrap();
    let on = path.iter().filter(|s| s.1 == 6).count();
    assert!(on >= 70, "{on} cells beside the partner");
    // With nothing to follow, it runs straight.
    let masks = Masks::open(Window::full(&grid));
    let path = find(&masks, &[(0, 2, FRONT)], Goal::Spots(&[(100, 2, FRONT)])).unwrap();
    assert!(path.iter().all(|s| s.1 == 2));
}

#[test]
fn a_pairs_new_copper_is_kept_only_if_more_is_coupled_at_no_more_length_or_vias() {
    let before = (10.0, 2, 0.30);
    assert!(keeps_pair(before, (10.0, 2, 0.60)));
    assert!(keeps_pair(before, (10.0 + PAIR_SLACK, 1, 0.60)));
    assert!(!keeps_pair(before, (10.5, 2, 0.60)), "longer");
    assert!(!keeps_pair(before, (9.0, 3, 0.60)), "a via more");
    assert!(!keeps_pair(before, (9.0, 2, 0.305)), "no more coupled");
}

#[test]
fn a_wall_on_both_layers_cannot_be_crossed() {
    let (grid, mut masks) = open_grid();
    for j in 0..grid.ny {
        let at = masks.at(15, j);
        masks.track[FRONT][at] = true;
        let at = masks.at(15, j);
        masks.track[BACK][at] = true;
    }
    assert_eq!(
        find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])),
        None
    );
}

#[test]
fn a_via_is_not_put_where_the_mask_forbids_one() {
    let (grid, mut masks) = open_grid();
    for j in 0..grid.ny {
        let at = masks.at(15, j);
        masks.track[FRONT][at] = true;
    }
    for i in 0..grid.nx {
        for j in 0..grid.ny {
            let at = masks.at(i, j);
            masks.via[at] = true;
        }
    }
    assert_eq!(
        find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])),
        None
    );
}

#[test]
fn a_run_through_cells_that_need_a_narrow_track_is_a_run_of_its_own() {
    let (grid, _) = open_grid();
    let cells: Vec<(usize, usize)> = (0..10).map(|i| (i, 5)).collect();
    let mut neck = vec![0u8; 10];
    for n in &mut neck[4..7] {
        *n = 1;
    }
    let runs = runs_with(&grid, &cells, &neck);
    // Wide up to the first necked cell, narrow through it, wide again after.
    let widths: Vec<u8> = runs.iter().map(|r| r.2).collect();
    assert_eq!(widths, [0, 1, 0], "{runs:?}");
    assert_eq!(runs[0].1, runs[1].0);
    assert_eq!(runs[1].1, runs[2].0);
    assert_eq!(runs[0].0, Pt::new(0.0, 0.5));
    assert_eq!(runs[2].1, Pt::new(0.9, 0.5));
}

#[test]
fn turns_cost_more_than_straight_runs() {
    let (grid, masks) = open_grid();
    // Same cost in steps either way round, so the path with one corner wins.
    let path = find(&masks, &[(0, 0, FRONT)], Goal::Spots(&[(20, 8, FRONT)])).unwrap();
    let cells: Vec<_> = path.iter().map(|s| (s.0, s.1)).collect();
    assert_eq!(runs(&grid, &cells).len(), 2);
}

#[test]
fn the_turn_cost_of_the_masks_is_what_a_bend_costs() {
    // A wall on the front with a gap at its top: round it takes four bends, through the
    // back two vias.
    let (_, mut masks) = open_grid();
    for j in 0..9 {
        let at = masks.at(15, j);
        masks.track[FRONT][at] = true;
    }
    let ends = ([(0, 4, FRONT)], [(30, 4, FRONT)]);
    let layers = |masks: &Masks| -> Vec<usize> {
        find(masks, &ends.0, Goal::Spots(&ends.1))
            .unwrap()
            .iter()
            .map(|s| s.2)
            .collect()
    };
    assert_eq!(masks.turn, TURN);
    assert!(
        !layers(&masks).contains(&BACK),
        "bends are cheaper than vias"
    );
    masks.turn = 1000;
    assert!(
        layers(&masks).contains(&BACK),
        "vias are cheaper than bends"
    );
}

/// A grid of 3 mm by 1 mm with one escape lane on the front, of net 7: the cells of row 5
/// from column 10 to 20, and a via's place at its end.
fn one_lane() -> (Grid, Lanes) {
    let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(3.0, 1.0)));
    let mut lanes = Lanes::over(&grid, 8);
    let track = Rect::new(Pt::new(1.0, 0.5), Pt::new(2.0, 0.5));
    lanes.claim(&grid, 7, [true, false], &track, Pt::new(2.0, 0.5), 0.2, 0.2);
    (grid, lanes)
}

#[test]
fn an_escape_lane_is_free_for_its_own_net_and_dear_for_another() {
    let (grid, lanes) = one_lane();
    let window = Window::full(&grid);
    let straight = |net: usize| {
        let mut masks = Masks::open(window);
        masks.lane = lanes.charged(window, net);
        let path = find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
        path.iter().all(|&(_, j, layer)| j == 5 && layer == FRONT)
    };
    assert!(straight(7), "the lane's own net goes straight through it");
    assert!(!straight(3), "another net goes round the lane");
    // A via of another net at the lane's end costs more than one off the lane.
    let mut masks = Masks::open(window);
    masks.lane = lanes.charged(window, 3);
    assert!(masks.lane_via_cost(20, 5) > 0);
    assert!(masks.lane_cost(FRONT, 15, 5) > 0);
    assert_eq!(
        masks.lane_cost(BACK, 15, 5),
        0,
        "the lane is on the pad's layer only"
    );
    assert_eq!(masks.lane_via_cost(20, 9), 0);
    // The lane's own net puts its via at the lane's end for nothing, but pays as any other
    // for one that closes the lane before its end.
    masks.lane = lanes.charged(window, 7);
    assert_eq!(masks.lane_via_cost(20, 5), 0);
    assert!(masks.lane_via_cost(14, 5) > 0);
    assert!(
        masks.lane_via_cost(14, 7) > 0,
        "beside the lane, its copper would close it"
    );
}

#[test]
fn the_other_net_of_a_diff_pair_uses_the_lane_as_its_own() {
    let (grid, mut lanes) = one_lane();
    lanes.partner[7] = Some(3);
    lanes.partner[3] = Some(7);
    let window = Window::full(&grid);
    let mut masks = Masks::open(window);
    masks.lane = lanes.charged(window, 3);
    assert_eq!(masks.lane_cost(FRONT, 15, 5), 0);
    masks.lane = lanes.charged(window, 2);
    assert!(masks.lane_cost(FRONT, 15, 5) > 0);
}

#[test]
fn another_net_still_crosses_a_lane_when_nothing_else_is_free() {
    let (grid, lanes) = one_lane();
    let window = Window::full(&grid);
    let mut masks = Masks::open(window);
    masks.lane = lanes.charged(window, 3);
    // Everything but row 5 is walled, and no via may stand anywhere.
    for j in 0..grid.ny {
        for i in 0..grid.nx {
            let at = masks.at(i, j);
            masks.via[at] = true;
            masks.track[BACK][at] = true;
            if j != 5 {
                masks.track[FRONT][at] = true;
            }
        }
    }
    let path = find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)]));
    assert_eq!(path.map(|p| p.len()), Some(31), "a cost, not a wall");
}

#[test]
fn a_path_to_a_plane_ends_at_the_nearest_cell_a_via_may_use() {
    let (grid, mut masks) = open_grid();
    // No via within three cells of the start, along either layer.
    for i in 0..3 {
        for j in 0..grid.ny {
            let at = masks.at(i, j);
            masks.via[at] = true;
        }
    }
    let path = find(&masks, &[(0, 5, FRONT)], Goal::Via).unwrap();
    assert_eq!(path.last(), Some(&(3, 5, FRONT)));
    // With a via nowhere, there is no way to the plane.
    masks.via.iter_mut().for_each(|v| *v = true);
    assert_eq!(find(&masks, &[(0, 5, FRONT)], Goal::Via), None);
}

#[test]
fn a_window_round_the_ends_of_a_line_is_clipped_to_the_grid() {
    let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(10.0, 5.0)));
    let w = Window::around(&grid, &[(20, 25, FRONT), (30, 25, BACK)], 1.0);
    assert_eq!(
        w,
        Window {
            i0: 10,
            j0: 15,
            nx: 31,
            ny: 21
        }
    );
    assert!(w.contains(10, 15) && w.contains(40, 35) && !w.contains(41, 25));
    let edge = Window::around(&grid, &[(1, 1, FRONT)], 2.0);
    assert_eq!((edge.i0, edge.j0), (0, 0));
    let all = Window::around(&grid, &[(50, 25, FRONT)], 100.0);
    assert!(all.covers(&grid));
}

#[test]
fn a_search_in_a_window_stays_inside_it() {
    let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(3.0, 1.0)));
    let mut masks = Masks::open(Window {
        i0: 10,
        j0: 0,
        nx: 15,
        ny: grid.ny,
    });
    // A wall on both layers across the window, with a gap only outside it is no gap.
    for j in 0..grid.ny {
        let at = masks.at(20, j);
        masks.track[FRONT][at] = true;
        masks.track[BACK][at] = true;
    }
    assert_eq!(
        find(&masks, &[(10, 5, FRONT)], Goal::Spots(&[(24, 5, FRONT)])),
        None
    );
}

#[test]
fn a_pad_walled_in_is_closed_and_one_hemmed_in_by_the_window_is_cramped() {
    let (_, mut masks) = open_grid();
    let ring = |masks: &mut Masks, (ci, cj): (usize, usize)| {
        for (i, j) in [(ci - 1, cj), (ci + 1, cj), (ci, cj - 1), (ci, cj + 1)] {
            let at = masks.at(i, j);
            masks.track[FRONT][at] = true;
            masks.track[BACK][at] = true;
        }
    };
    ring(&mut masks, (15, 5));
    let from = [(15, 5, FRONT)];
    let to = [(28, 5, FRONT)];
    assert_eq!(
        link(&masks, &from, Goal::Spots(&to), Extras::default()),
        Link::Closed
    );
    // From the other end the answer is the same.
    assert_eq!(
        link(&masks, &to, Goal::Spots(&from), Extras::default()),
        Link::Closed
    );
    // Open it again and the ends are joined.
    let (_, open) = open_grid();
    assert_eq!(
        link(&open, &from, Goal::Spots(&to), Extras::default()),
        Link::Joined
    );

    // A window with a wall in it, from edge to edge: each side is hemmed in by the wall
    // and the window, which a larger window might not.
    let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(3.0, 1.0)));
    let mut cut = Masks::open(Window {
        i0: 5,
        j0: 0,
        nx: 20,
        ny: grid.ny,
    });
    for j in 0..grid.ny {
        let at = cut.at(15, j);
        cut.track[FRONT][at] = true;
        cut.track[BACK][at] = true;
    }
    assert_eq!(
        link(
            &cut,
            &[(10, 5, FRONT)],
            Goal::Spots(&[(20, 5, FRONT)]),
            Extras::default()
        ),
        Link::Cramped
    );
}

#[test]
fn a_link_to_a_plane_needs_a_place_for_a_via() {
    let (_, mut masks) = open_grid();
    assert_eq!(
        link(&masks, &[(0, 5, FRONT)], Goal::Via, Extras::default()),
        Link::Joined
    );
    masks.via.iter_mut().for_each(|v| *v = true);
    assert_ne!(
        link(&masks, &[(0, 5, FRONT)], Goal::Via, Extras::default()),
        Link::Joined
    );
}

/// The open grid with the cell `(ci, cj)` walled in on the front: its four neighbours
/// are closed to tracks there, and no via may be put anywhere (the way out of a pad
/// that has no room beside it).
fn walled_front(ci: usize, cj: usize) -> Masks {
    let (_, mut masks) = open_grid();
    masks.via.iter_mut().for_each(|v| *v = true);
    for (i, j) in [(ci - 1, cj), (ci + 1, cj), (ci, cj - 1), (ci, cj + 1)] {
        let at = masks.at(i, j);
        masks.track[FRONT][at] = true;
    }
    masks
}

fn search_with(masks: &Masks, from: &[Spot], to: &[Spot], extras: Extras) -> Option<Vec<Spot>> {
    search(masks, from, Goal::Spots(to), extras, &|| false).ok()
}

#[test]
fn a_pad_walled_in_on_its_layer_is_left_through_a_via_in_the_pad_if_it_may_have_one() {
    let masks = walled_front(10, 5);
    let from = [(10, 5, FRONT)];
    let to = [(28, 5, BACK)];
    assert_eq!(search_with(&masks, &from, &to, Extras::default()), None);
    let extras = Extras {
        from: &[(10, 5, BACK)],
        to: &[],
        ..Extras::default()
    };
    let path = search_with(&masks, &from, &to, extras).unwrap();
    // The path starts on the back, in the pad's cell, and comes up at the far end.
    assert_eq!(path.first(), Some(&(10, 5, BACK)));
    assert_eq!(path.last(), Some(&(28, 5, BACK)));
    assert_eq!(link(&masks, &from, Goal::Spots(&to), extras), Link::Joined);
    assert_eq!(
        link(&masks, &from, Goal::Spots(&to), Extras::default()),
        Link::Closed
    );
}

#[test]
fn the_far_end_of_a_path_may_be_a_via_in_a_pad_too() {
    let masks = walled_front(20, 5);
    let from = [(2, 5, BACK)];
    let to = [(20, 5, FRONT)];
    assert_eq!(search_with(&masks, &from, &to, Extras::default()), None);
    let extras = Extras {
        from: &[],
        to: &[(20, 5, BACK)],
        ..Extras::default()
    };
    let path = search_with(&masks, &from, &to, extras).unwrap();
    assert_eq!(path.last(), Some(&(20, 5, BACK)));
    assert_eq!(path.first(), Some(&(2, 5, BACK)));
}

#[test]
fn a_via_in_a_pad_is_used_only_when_nothing_cheaper_reaches_the_pad() {
    // Nothing in the way: the straight path on the front costs 180 and a via in the pad
    // costs far more, so the path is not through it.
    let (_, masks) = open_grid();
    let extras = Extras {
        from: &[(2, 5, BACK)],
        to: &[(28, 5, BACK)],
        ..Extras::default()
    };
    let path = search_with(&masks, &[(2, 5, FRONT)], &[(28, 5, FRONT)], extras).unwrap();
    assert!(path.iter().all(|s| s.2 == FRONT), "{path:?}");
}

#[test]
fn a_search_that_is_told_to_stop_gives_up_and_says_so() {
    // No place for a via anywhere, and so much to look at that the search looks at the
    // clock before it has seen it all.
    let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(40.0, 40.0)));
    let mut all_via = Masks::open(Window::full(&grid));
    all_via.via.iter_mut().for_each(|v| *v = true);
    assert_eq!(
        search(
            &all_via,
            &[(0, 0, FRONT)],
            Goal::Via,
            Extras::default(),
            &|| true
        ),
        Err(Miss::Stopped)
    );
    assert_eq!(
        search(
            &all_via,
            &[(0, 0, FRONT)],
            Goal::Via,
            Extras::default(),
            &|| false
        ),
        Err(Miss::Blocked)
    );
}
