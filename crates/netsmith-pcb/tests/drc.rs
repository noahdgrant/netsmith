//! netsmith's own design rules check on the boards the placer and the router make (ADR 0037):
//! boards routed from many seeds must have no error, the check must see the same missing
//! connections as the ratsnest, a 70-part board must be checked in a couple of seconds, and
//! where KiCad is installed its check must agree with ours on a sample of them.

use netsmith_kicad::testlib::TempDir;
use std::time::Instant;

use netsmith_fab::drc::{self, Extras, Report, kicad};
use netsmith_fab::{KicadCli, KicadCliExe};
use netsmith_pcb::{Board, Ratsnest, RouteOptions, kicad_pcb, kicad_pro, route, route_with};

use crate::common;

use common::{hard_boards, synthetic_board};

/// The check as `fab` runs it on the files it writes for `board`.
fn ours(board: &Board) -> Report {
    let extras = Extras {
        mask_web: Some(board.rules.mask_bridge),
        via_in_pad: Some(board.process.via_in_pad),
        excused_courtyards: board.excused_courtyards(),
    };
    drc::check(&kicad_pcb(board), Some(&kicad_pro(board)), &extras)
        .unwrap_or_else(|e| panic!("{}: {e}", board.name))
}

/// KiCad's check of the same files compared with ours, or `None` if KiCad is not here.
fn against_kicad(name: &str, board: &Board, mine: &Report) -> Option<kicad::Comparison> {
    let cli = KicadCliExe::locate_runnable()?;
    let dir = TempDir::new(&format!("pcb-drc-{name}"));
    let path = dir.join(format!("{name}.kicad_pcb"));
    std::fs::write(&path, kicad_pcb(board)).ok()?;
    std::fs::write(path.with_extension("kicad_pro"), kicad_pro(board)).ok()?;
    let json = cli
        .drc_json(&path)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    Some(kicad::compare(mine, &kicad::parse(&json).unwrap(), 0.1))
}

#[test]
fn a_hundred_boards_from_a_hundred_seeds_route_to_no_error_and_the_count_of_missing_connections_is_the_ratsnests()
 {
    let boards = hard_boards(100);
    let mut routed = 0;
    for (n, mut board) in boards.into_iter().enumerate() {
        let left = route(&mut board);
        // Before routing, the missing connections are the ratsnest's; after, those it left.
        let mine = ours(&board);
        assert_eq!(
            mine.count(drc::Rule::UnconnectedItems),
            Ratsnest::of(&board).len(),
            "board {n}: {mine}"
        );
        if left.is_empty() {
            routed += 1;
            let errors: Vec<_> = mine.errors().map(ToString::to_string).collect();
            assert_eq!(errors, Vec::<String>::new(), "board {n}");
        }
    }
    assert!(routed >= 90, "only {routed} of 100 routed");
}

#[test]
fn unrouted_boards_have_exactly_the_ratsnests_missing_connections_and_nothing_else_wrong() {
    for mut board in hard_boards(10) {
        let mine = ours(&board);
        assert_eq!(
            mine.count(drc::Rule::UnconnectedItems),
            Ratsnest::of(&board).len()
        );
        assert!(
            mine.errors().all(|f| f.rule == drc::Rule::UnconnectedItems),
            "{mine}"
        );
        let _ = route(&mut board);
    }
}

#[test]
fn kicad_agrees_on_a_sample_of_routed_random_boards() {
    let boards = hard_boards(100);
    let sample: Vec<(usize, Board)> = boards
        .into_iter()
        .enumerate()
        .filter(|(n, _)| n % 7 == 0)
        .map(|(n, mut b)| {
            let _ = route(&mut b);
            (n, b)
        })
        .collect();
    let (mut agreed, mut failures) = (0, Vec::new());
    let results: Vec<_> = std::thread::scope(|s| {
        sample
            .iter()
            .map(|(n, b)| {
                s.spawn(move || {
                    let mine = ours(b);
                    against_kicad(&format!("rand{n}"), b, &mine).map(|c| (*n, mine, c))
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect()
    });
    for (n, mine, c) in results.into_iter().flatten() {
        agreed += c.agreed;
        if !c.agrees() {
            failures.push(format!("board {n}: {c}{mine}"));
        }
    }
    eprintln!(
        "random boards: {} compared with KiCad, {agreed} findings agree",
        sample.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn boards_from_the_synthetic_generator_route_clean_and_agree_with_kicad() {
    let mut failures = Vec::new();
    for seed in 1..=4u64 {
        let mut board = synthetic_board(seed, 10, 3, 8, 10, 4);
        let routing = route_with(&mut board, RouteOptions::default());
        assert!(
            routing.left.is_empty(),
            "seed {seed}: {:?}",
            routing.left.lines
        );
        let mine = ours(&board);
        let errors: Vec<_> = mine.errors().map(ToString::to_string).collect();
        assert_eq!(errors, Vec::<String>::new(), "seed {seed}");
        if let Some(c) = against_kicad(&format!("synth{seed}"), &board, &mine)
            && !c.agrees()
        {
            failures.push(format!("seed {seed}: {c}{mine}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_board_of_seventy_parts_is_checked_in_a_couple_of_seconds() {
    let mut board = synthetic_board(7, 52, 17, 20, 72, 36);
    assert_eq!(board.parts.len(), 70);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty());
    let text = kicad_pcb(&board);
    let pro = kicad_pro(&board);
    let started = Instant::now();
    let report = drc::check(&text, Some(&pro), &Extras::default()).unwrap();
    let took = started.elapsed();
    eprintln!(
        "70 parts, {} tracks, {} vias: checked in {took:?}, {} finding(s)",
        board.tracks.len(),
        board.vias.len(),
        report.findings.len()
    );
    assert!(report.is_clean(), "{report}");
    // Generous for an unoptimised build; a release build takes a fraction of a second.
    let limit = if cfg!(debug_assertions) { 20.0 } else { 2.0 };
    assert!(took.as_secs_f64() < limit, "took {took:?}");
}
