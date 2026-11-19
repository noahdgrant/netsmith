//! The process of each board of a system is the board's own (issue #1326): two boards that
//! state the same generic process differently are two processes, and a contradiction about a
//! process is reported only inside one board.

use netsmith_check::Loader;
use netsmith_pipeline::{Outcome, Problem, outcome_with, providers};
use netsmith_solve::Catalogs;

/// A board with a header and a process: `process` is the type and what it states.
fn board(name: &str, process: &str) -> String {
    format!(
        "board {name} {{\n    .process: {process}\n    .process.inner1.use &= \"signal\"\n    .process.inner2.use &= \"signal\"\n    .input: Rail {{ .voltage &= 5V +/- 5%; .pos.net &= \"VIN\" }}\n    .j1: Header1x2\n    .input.pos ~ .j1.p1\n    .input.neg ~ .j1.p2\n}}\n"
    )
}

/// A system of one board for each process, the boards `B0`, `B1`... as the members `b0`,
/// `b1`...
fn system(processes: &[&str]) -> String {
    let mut text = String::from(
        "import std::mfg::jlcpcb::FourLayer\nimport std::parts::connector::Header1x2\nimport std::sources::Rail\n\n",
    );
    for (i, process) in processes.iter().enumerate() {
        text.push_str(&board(&format!("B{i}"), process));
        text.push('\n');
    }
    text.push_str("system Stack {\n");
    for i in 0..processes.len() {
        text.push_str(&format!("    .b{i}: B{i}\n"));
    }
    text.push_str("}\n");
    text
}

/// A FourLayer process of this thickness.
fn thick(t: &str) -> String {
    format!("FourLayer {{ .stackup.thickness &= {t} }}")
}

fn build(source: &str) -> Result<Outcome, Vec<Problem>> {
    outcome_with(
        "main.nsm",
        source,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        None,
    )
}

fn messages(problems: &[Problem]) -> Vec<String> {
    problems.iter().map(|p| p.message.clone()).collect()
}

/// The provider the process of the board `name` of the system was made with. A board whose
/// process is the first provider in scope, with nothing passed over, has no note.
fn build_up(o: &Outcome, name: &str) -> String {
    let b = o
        .boards
        .iter()
        .find(|b| b.system.is_some() && b.board == name)
        .unwrap_or_else(|| panic!("no board `{name}` of the system"));
    b.providers
        .iter()
        .find(|p| p.path == "process")
        .map_or_else(|| "JlcFourLayerA04161H7628".into(), |p| p.chosen.clone())
}

#[test]
fn two_boards_of_a_system_state_one_generic_process_at_two_thicknesses() {
    let o = build(&system(&[&thick("1.6mm"), &thick("1.2mm")]))
        .unwrap_or_else(|e| panic!("{:#?}", messages(&e)));
    assert!(build_up(&o, "b0").contains("04161H"), "{}", build_up(&o, "b0"));
    assert!(build_up(&o, "b1").contains("04121H"), "{}", build_up(&o, "b1"));
}

#[test]
fn each_board_of_a_system_of_four_has_the_thickness_it_states() {
    let thicknesses = ["1.2mm", "2.0mm", "0.8mm", "1.0mm"];
    let processes: Vec<String> = thicknesses.iter().map(|t| thick(t)).collect();
    let processes: Vec<&str> = processes.iter().map(String::as_str).collect();
    let before = providers::trials_run();
    let o = build(&system(&processes)).unwrap_or_else(|e| panic!("{:#?}", messages(&e)));
    // Each board's build-up is settled at the check, board by board: one trial solves the
    // system, where a search over the product of the four boards' providers gave up at 64.
    assert_eq!(providers::trials_run() - before, 1);
    for (i, code) in ["04121H", "04201H", "04081H", "04101H"].iter().enumerate() {
        let picked = build_up(&o, &format!("b{i}"));
        assert!(picked.contains(code), "b{i} is {picked}, not {code}");
    }
}

#[test]
fn a_thickness_no_provider_has_is_reported_inside_its_board_alone() {
    let e = build(&system(&[&thick("1.6mm"), &thick("1.3mm")]))
        .err()
        .expect("no build-up is 1.3 mm thick");
    let boards: Vec<Option<String>> = e.iter().map(|p| p.board.clone()).collect();
    assert!(
        boards.iter().all(|b| b.as_deref() == Some("Stack/b1")),
        "{boards:?}: {:#?}",
        messages(&e)
    );
    let m = messages(&e).join("\n");
    assert!(m.contains("b1.process"), "{m}");
    assert!(!m.contains("b0.process"), "{m}");
}
