//! The alternatives of an unresolved generic part (ADR 0047): `build` plans them without
//! solving the board again, and the one who shows them (the bill of materials, `fab`) makes the
//! solves, once. Every part, number and file here is invented.

use std::cell::Cell;
use std::fs;
use std::rc::Rc;

use netsmith::editor::Workspace;
use netsmith::project::{Project, Target};
use netsmith::refresh::Network;
use netsmith::{Built, outcome_with};
use netsmith_editor::Backend;
use netsmith_kicad::testlib::TempDir;
use netsmith_solve::Catalogs;

/// A resistor in series with an LED on 5 V.
const LED: &str = include_str!("../../../examples/led.nsm");

/// An invented resistor only this project has: the board's 249 ohm has no row, so the part is
/// unresolved, and the nearest rows of the standard catalog are its three alternatives.
const OWN_CATALOG: &str = "lcsc,mpn,resistance,max_power,package,library,source\n\
     C9999999,ZZR-316,316ohm +/- 1%,250mW,0402,Extended,https://example.invalid/zzr\n";

const UNIT: &str = "alternative parts re-solved";

fn project(label: &str, resistance: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"own\"\nseries = \"E96\"\ncatalogs = [\"catalogs/resistor.csv\"]\n",
    )
    .unwrap();
    fs::write(root.join("catalogs/resistor.csv"), OWN_CATALOG).unwrap();
    let source = LED.replace(
        ".r1: Resistor {}",
        &format!(".r1: Resistor {{ .resistance &= {resistance} }}"),
    );
    fs::write(root.join("src/main.nsm"), source).unwrap();
    root
}

/// The boards of the project, built with no KiCad library, and how many alternatives were
/// solved while it was built.
fn build(root: &TempDir) -> (Vec<Built>, u64, Catalogs) {
    let project = Project::load(root).unwrap();
    let catalogs = project.catalogs().unwrap();
    let loader = project.loader().unwrap();
    let source = fs::read_to_string(root.join("src/main.nsm")).unwrap();
    let solved = Rc::new(Cell::new(0));
    let seen = solved.clone();
    let outcome = netsmith_core::job::reporting(
        Rc::new(move |w| {
            if w.unit == UNIT {
                seen.set(seen.get().max(w.done));
            }
        }),
        || outcome_with("main.nsm", &source, &loader, &catalogs, None),
    );
    (outcome.unwrap().boards, solved.get(), catalogs)
}

#[test]
fn build_does_not_solve_the_board_again_for_alternatives() {
    let root = project("alternatives-build", "249ohm +/- 1%");
    let (boards, solved, _) = build(&root);
    assert_eq!(solved, 0);
    let rechecks = &boards[0].rechecks;
    assert_eq!(rechecks.planned(), 3, "the three nearest rows");
    assert!(!rechecks.is_solved());
}

#[test]
fn a_board_with_no_unresolved_part_plans_nothing() {
    let root = project("alternatives-none", "316ohm +/- 1%");
    let (boards, solved, catalogs) = build(&root);
    assert_eq!(solved, 0);
    assert_eq!(boards[0].rechecks.planned(), 0);
    assert!(boards[0].rechecks.get(&catalogs).is_empty());
}

#[test]
fn the_first_to_ask_makes_the_solves_and_a_second_reads_them() {
    let root = project("alternatives-get", "249ohm +/- 1%");
    let (boards, _, catalogs) = build(&root);
    let rechecks = &boards[0].rechecks;
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    let first = netsmith_core::job::reporting(
        Rc::new(move |w| {
            if w.unit == UNIT {
                seen.set(seen.get() + 1);
            }
        }),
        || rechecks.get(&catalogs).to_vec(),
    );
    assert_eq!(count.get(), 3, "one re-solve for each alternative");
    assert!(rechecks.is_solved());
    let names: Vec<&str> = first.iter().map(|r| r.alternative.as_str()).collect();
    assert_eq!(
        names,
        [
            "240Ω 1% 0402 C25094",
            "270Ω 1% 0402 C25099",
            "220Ω 1% 0402 C25091"
        ]
    );
    assert!(first.iter().all(|r| r.refdes == "R1" && r.outcome.is_ok()));
    // A second call reads what the first made.
    let second = netsmith_core::job::reporting(
        Rc::new(|w| assert_ne!(w.unit, UNIT, "solved again")),
        || rechecks.get(&catalogs).to_vec(),
    );
    assert_eq!(first, second);
}

#[test]
fn the_bill_of_materials_still_says_how_each_alternative_checks() {
    let root = project("alternatives-bom", "249ohm +/- 1%");
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap()
        .with_network(Network {
            now: Some(1_793_620_800),
            offline: true,
            cache_dir: None,
            ..Network::default()
        });
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let bom = editor.bom(&analysis.boards[0].name).unwrap();
    let r = bom.lines.iter().find(|l| l.kind == "Resistor").unwrap();
    let why = r.unresolved.as_deref().expect("an unresolved part");
    assert_eq!(why, EXPECTED, "{why}");
}

/// What the bill of materials said before the solves were made on demand, word for word.
const EXPECTED: &str = "unresolved: no row of the `resistor` catalog can be ordered for 249 ohm 1% 0402 62.5 mW (E96). Nearest in the catalog: 240Ω 1% 0402 C25094 (-3.6%, Extended), 270Ω 1% 0402 C25099 (+8.4%, Extended), 220Ω 1% 0402 C25091 (-11.6%, Basic). Checked again with each alternative in this part's place, its stated value left out: 240Ω 1% 0402 C25094 passes every check; 270Ω 1% 0402 C25099 passes every check; 220Ω 1% 0402 C25091 passes every check. Change the value in the design and build again: the solver then checks every relation again with it.";
