//! `examples/two_buck/`: a 12 V board with two buck stages, a linear post-regulator, input
//! protection and the parts around them (59 parts), made to time the solver on a board with
//! regulators in it (`docs/solver-timings.md`). The test builds it with no KiCad, solves it and
//! reads what the DC analysis settled. It asserts no time: a wall-clock number in a test fails
//! on a busy machine and says nothing about the code (the timings are measured with
//! `NETSMITH_TIMINGS=1`, by hand).

use std::fs;
use std::path::PathBuf;

use netsmith::project::Target;
use netsmith::{Libraries, build_with};
use netsmith_check::{Domain, check};
use netsmith_kicad::install::Search;
use netsmith_quantity::Quantity;
use netsmith_solve::solve;
use netsmith_testkit::TempDir;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/two_buck")
}

fn source() -> (String, String) {
    let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(target.files.len(), 1, "one board file");
    let file = &target.files[0];
    let text = fs::read_to_string(&file.path).unwrap();
    (file.name.clone(), text)
}

#[test]
fn the_board_builds_with_no_kicad_and_has_about_sixty_parts() {
    let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
    let (name, text) = source();
    // A machine with no KiCad install: the IC is drawn from its pin table and every footprint
    // is a generator's (ADR 0097).
    let nowhere = TempDir::new("two-buck-no-kicad");
    let libraries = Libraries::for_project_by(Some(&project()), &Search::under(nowhere.path()))
        .unwrap_or_else(|e| panic!("{e}"));
    let built = build_with(
        &name,
        &text,
        &target.loader().unwrap(),
        &target.catalogs().unwrap(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(built.len(), 1);
    let board = &built[0];
    assert_eq!(board.design.name, "TwoBuck");
    let parts = board.design.parts.len();
    assert!((55..=65).contains(&parts), "{parts} parts");
    assert!(!board.schematic.is_empty());
    // Every generic part found a row; only the invented IC has none (two of them).
    let unsourced: Vec<_> = board
        .sourcing
        .iter()
        .filter(|s| s.lcsc.is_none() && s.kind != "Header1x2" && s.kind != "TestPoint")
        .map(|s| s.kind.as_str())
        .collect();
    assert_eq!(unsourced, ["BenchBuck", "BenchBuck"], "{unsourced:?}");
}

fn range(solution: &netsmith_solve::Solution, path: &str) -> Quantity {
    match solution.model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

#[test]
fn it_solves_and_the_dc_analysis_settles_what_each_regulator_draws() {
    let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
    let (name, text) = source();
    let loader = target.loader().unwrap();
    let checked = check(&name, &text, &loader);
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().expect("one board");
    let solution = solve(board, &target.catalogs().unwrap());
    // No error finding: every generic picked a row, no rating failed, and the DC analysis
    // raised nothing.
    let said: String = solution
        .diagnostics
        .iter()
        .map(|d| d.render(&checked.sources))
        .collect();
    assert!(solution.diagnostics.is_empty(), "{said}");
    assert!(solution.unresolved.is_empty(), "{:?}", solution.unresolved);

    // The DC analysis reached the regulators: each IC's input current is a range with a draw
    // in it, and the output of each stage is held inside what the board asked for.
    for stage in ["buck_5v", "buck_3v3"] {
        let drawn = range(&solution, &format!("{stage}.stage.ic.vin.current"));
        assert!(drawn.hi() > 0.0 && drawn.hi() < 3.0, "{stage}: {drawn}");
    }
    let five = range(&solution, "buck_5v.vout.voltage");
    assert!(
        five.lo() >= 4.75 - 1e-9 && five.hi() <= 5.25 + 1e-9,
        "{five}"
    );
    let three = range(&solution, "buck_3v3.vout.voltage");
    assert!(
        three.lo() >= 3.135 - 1e-9 && three.hi() <= 3.465 + 1e-9,
        "{three}"
    );
}
