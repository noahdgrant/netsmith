//! Regenerating one module (issue #825): the module's parts are placed again near where they
//! were, every other part stays, and the nets of the parts placed again are named.

use std::collections::BTreeSet;

use netsmith_pcb::Board;
use netsmith_placer::{RegenerateError, regenerate};

use crate::common;

fn regulator() -> Board {
    common::example_board("linear_regulator", &common::fixtures())
}

fn place_of(board: &Board, refdes: &str) -> (f64, f64, f64) {
    let p = board.part(refdes).unwrap();
    (p.at.x, p.at.y, p.rotation)
}

#[test]
fn a_module_is_placed_again_and_the_rest_stays() {
    let board = regulator();
    let stage: Vec<String> = board
        .parts
        .iter()
        .filter(|p| p.path.starts_with("stage."))
        .map(|p| p.refdes.clone())
        .collect();
    assert_eq!(stage.len(), 3);
    // Seeds until one moves a part: a seed may find the arrangement that is there.
    let (seed, again, done) = (1..20)
        .find_map(|seed| {
            let mut again = board.clone();
            let done = regenerate(&mut again, "stage", &BTreeSet::new(), seed, 3.0).unwrap();
            (again.parts != board.parts).then_some((seed, again, done))
        })
        .expect("some seed moves a part of the module");
    assert_eq!(done.parts, stage);
    assert!(done.held.is_empty());
    for p in &board.parts {
        let q = again.part(&p.refdes).unwrap();
        if stage.contains(&p.refdes) {
            assert!(done.region.contains(&q.courtyard()), "{} left", p.refdes);
        } else {
            assert_eq!(place_of(&board, &p.refdes), place_of(&again, &p.refdes));
        }
    }
    assert_eq!(again.check_placement(), vec![]);
    // Every net of a part placed again is named, and only those.
    for net in &board.nets {
        let touches = net.pads.iter().any(|pad| stage.contains(&pad.refdes));
        assert_eq!(done.nets.contains(&net.name), touches, "{}", net.name);
    }
    // The same seed gives the same board.
    let mut twice = board.clone();
    regenerate(&mut twice, "stage", &BTreeSet::new(), seed, 3.0).unwrap();
    assert_eq!(twice, again);
}

#[test]
fn a_held_part_of_the_module_stays() {
    let board = regulator();
    let held_one = board
        .parts
        .iter()
        .find(|p| p.path == "stage.regulator")
        .unwrap()
        .refdes
        .clone();
    let held: BTreeSet<String> = [held_one.clone()].into();
    for seed in 0..4 {
        let mut again = board.clone();
        let done = regenerate(&mut again, "stage", &held, seed, 3.0).unwrap();
        assert_eq!(done.held, vec![held_one.clone()]);
        assert!(!done.parts.contains(&held_one));
        assert_eq!(place_of(&board, &held_one), place_of(&again, &held_one));
    }
}

#[test]
fn a_module_the_board_lacks_or_holds_whole_is_refused_and_changes_nothing() {
    let board = regulator();
    let mut tried = board.clone();
    let missing = regenerate(&mut tried, "stag", &BTreeSet::new(), 0, 3.0).unwrap_err();
    assert_eq!(
        missing.to_string(),
        "the board has no part in `stag`; its modules include `stage`"
    );
    let all: BTreeSet<String> = board
        .parts
        .iter()
        .filter(|p| p.path.starts_with("stage."))
        .map(|p| p.refdes.clone())
        .collect();
    let held = regenerate(&mut tried, "stage", &all, 0, 3.0).unwrap_err();
    assert!(matches!(held, RegenerateError::AllHeld { .. }), "{held}");
    assert_eq!(tried, board);
}
