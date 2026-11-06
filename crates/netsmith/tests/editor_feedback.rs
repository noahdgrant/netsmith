//! What the editor tells the page about the analysis: the stage, board and instance of each
//! problem, the places that power nodes and stages point at, and the events an analysis of the
//! files on disk reports while it runs (ADR 0019, "Editor API additions").

use std::{
    fs,
    sync::{Arc, Mutex},
};

use netsmith::{editor::Workspace, project::Target};
use netsmith_editor::{
    AnalysisEvent, Backend,
    api::{ProblemStage, Severity},
};
use netsmith_kicad::testlib::TempDir;

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");
const LED: &str = include_str!("../../../examples/led.nsm");

/// A project with `main` as its one top-level file and a module beside it.
fn project(name: &str, main: &str) -> TempDir {
    let root = TempDir::new(&format!("editor-feedback-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), main).unwrap();
    fs::write(
        root.join("src/power.nsm"),
        "//! A module.\n\nimport std::sources::Rail\n\n/// A supply.\ninterface Supply {\n    .rail: Rail\n}\n",
    )
    .unwrap();
    root
}

fn open(root: &TempDir) -> Workspace {
    Workspace::open(Target::resolve_from(root, None).unwrap()).unwrap()
}

fn line_of(text: &str, needle: &str) -> u32 {
    text.lines().position(|l| l.contains(needle)).unwrap() as u32 + 1
}

/// What a problem is about, as the page reads it: `(stage, board, path)`.
type About = (Option<ProblemStage>, Option<String>, Option<String>);

fn about(editor: &Workspace, text: &str) -> Vec<About> {
    editor
        .check("src/main.nsm", text)
        .unwrap()
        .problems
        .into_iter()
        .map(|p| (p.stage, p.board, p.path))
        .collect()
}

#[test]
fn a_parse_error_is_syntax_and_names_no_board_and_a_checker_error_names_the_board_it_is_in() {
    let root = project("stage-text", LED);
    let editor = open(&root);
    // The text no longer parses.
    let syntax = LED.replace("board LedExample {", "board LedExample");
    assert_eq!(
        about(&editor, &syntax),
        [(Some(ProblemStage::Syntax), None, None)]
    );
    // It parses and the checker does not know the type, in the definition of the board.
    let unknown = LED.replace(".r1: Resistor {}", ".r1: Resistr {}");
    assert_eq!(
        about(&editor, &unknown),
        [(Some(ProblemStage::Check), Some("LedExample".into()), None)]
    );
}

#[test]
fn a_checker_error_written_inside_a_board_says_the_board() {
    // A contradiction the checker finds.
    let bad = LED.replace(
        "        .current &= 5mA..15mA\n",
        "        .current &= 5mA..15mA\n        .current &= 1A..2A\n",
    );
    let root = project("stage-check-board", &bad);
    let analysis = open(&root).analysis();
    let p = analysis
        .problems
        .iter()
        .find(|p| p.message.contains("contradiction on `led.current`"))
        .unwrap_or_else(|| panic!("{:?}", analysis.problems));
    assert_eq!(p.severity, Severity::Error);
    assert_eq!(
        (p.stage, p.board.as_deref(), p.path.as_deref()),
        (Some(ProblemStage::Check), Some("LedExample"), None)
    );
    assert_eq!(p.at.as_ref().unwrap().line, line_of(&bad, "1A..2A"));
    // On the wire: `stage` and `board` next to the rest, and no `path` to say nothing.
    let wire = serde_json::to_value(p).unwrap();
    assert_eq!(wire["stage"], "check");
    assert_eq!(wire["board"], "LedExample");
    assert!(wire.get("path").is_none(), "{wire}");

    // A statement of the board itself.
    let bad = LED.replace(
        "    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance\n",
        "    .supply.voltage &= 12V\n",
    );
    let root = project("stage-check-board-level", &bad);
    let analysis = open(&root).analysis();
    let p = analysis
        .problems
        .iter()
        .find(|p| p.message.contains("supply.voltage"))
        .unwrap_or_else(|| panic!("{:?}", analysis.problems));
    assert_eq!(
        (p.stage, p.board.as_deref(), p.path.as_deref()),
        (Some(ProblemStage::Check), Some("LedExample"), None)
    );
}

#[test]
fn a_checker_error_in_a_board_a_system_places_is_named_as_the_systems_member() {
    let bad = TWO_BOARDS.replace(
        "        .vout.voltage &= 3.3V +/- 5%\n",
        "        .vout.voltage &= 3.3V +/- 5%\n        .vout.voltage &= 12V\n",
    );
    let root = project("stage-check-system", &bad);
    let analysis = open(&root).analysis();
    assert!(!analysis.problems.is_empty());
    for p in &analysis.problems {
        assert_eq!(
            (p.stage, p.board.as_deref(), p.path.as_deref()),
            (Some(ProblemStage::Check), Some("TwoBoards/power"), None),
            "{p:?}"
        );
    }
}

#[test]
fn a_checker_error_in_a_module_or_a_system_belongs_to_no_one_board() {
    // `LdoStage` is a module that two boards could share: a mistake in it is nobody's.
    let bad = TWO_BOARDS.replace(
        "    .vin ~ .regulator.vin\n",
        "    .vin ~ .regulator.vin\n    .vin ~ .nowhere\n",
    );
    let root = project("stage-check-module", &bad);
    let analysis = open(&root).analysis();
    let p = analysis
        .problems
        .iter()
        .find(|p| p.message.contains("nowhere"))
        .unwrap_or_else(|| panic!("{:?}", analysis.problems));
    assert_eq!(
        (p.stage, p.board.as_deref(), p.path.as_deref()),
        (Some(ProblemStage::Check), None, None)
    );
}

#[test]
fn a_solver_error_says_its_board_and_a_system_members_is_named_as_the_summary_does() {
    // The resistor is too small for the LED: no part combination satisfies the relation.
    let bad = LED.replace(
        ".r1: Resistor {}",
        ".r1: Resistor { .resistance &= 100ohm +/- 1% }",
    );
    let root = project("stage-solve", &bad);
    let analysis = open(&root).analysis();
    let p = analysis
        .problems
        .iter()
        .find(|p| p.message.contains("no combination of candidates"))
        .unwrap_or_else(|| panic!("{:?}", analysis.problems));
    assert_eq!(p.severity, Severity::Error);
    assert_eq!(
        (p.stage, p.board.as_deref(), p.path.as_deref()),
        (Some(ProblemStage::Solve), Some("LedExample"), None)
    );
    assert_eq!(
        serde_json::to_value(p).unwrap()["stage"],
        "solve",
        "the wire word"
    );

    // The same fault in the LED board of the system: that member's, written in its definition.
    let bad = TWO_BOARDS.replace(
        ".r1: Resistor {}",
        ".r1: Resistor { .resistance &= 10ohm +/- 1% }",
    );
    let root = project("stage-solve-system", &bad);
    let analysis = open(&root).analysis();
    let p = analysis
        .problems
        .iter()
        .find(|p| p.stage == Some(ProblemStage::Solve) && p.severity == Severity::Error)
        .unwrap_or_else(|| panic!("{:?}", analysis.problems));
    assert_eq!(p.board.as_deref(), Some("TwoBoards/led"), "{p:?}");
}

const BANK: &str = "//! A rail with a bank of resistors across it.

import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

module Bank {
    .power: Rail
    .power.current &= 0mA..2mA
    .a: Resistor { .resistance &= 10kohm +/- 1% }
    .b: Resistor { .resistance &= 10kohm +/- 1% }
    .c: Resistor { .resistance &= 4.7kohm +/- 1% }
    .power.pos ~> .a ~> .b ~> .power.neg
    .power.pos ~> .c ~> .power.neg
}

board Notes {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }
    .bank: Bank
    .power_in: Header1x2
    .supply ~ .bank.power
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
}
";

#[test]
fn a_note_is_the_solvers_or_lowerings_and_says_its_board_and_instance() {
    let root = project("stage-notes", BANK);
    let analysis = open(&root).analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    // What the solver did not evaluate, at the instance it names.
    let solver = analysis
        .problems
        .iter()
        .find(|p| p.message.contains("bank.a.drop"))
        .unwrap_or_else(|| panic!("{:?}", analysis.problems));
    assert_eq!(
        (
            solver.stage,
            solver.board.as_deref(),
            solver.path.as_deref()
        ),
        (Some(ProblemStage::Solve), Some("Notes"), Some("bank.a"))
    );
    // Lowering's own note: the nets whose tracks it could not size.
    let lowering = analysis
        .problems
        .iter()
        .find(|p| p.message.contains("tracks of net `3V3` are not sized"))
        .unwrap_or_else(|| panic!("{:?}", analysis.problems));
    assert_eq!(lowering.stage, Some(ProblemStage::Lower));
    assert_eq!(lowering.board.as_deref(), Some("Notes"));
    // Every note and warning of a built board has its board.
    assert!(
        analysis
            .problems
            .iter()
            .all(|p| p.stage.is_some() && p.board.is_some()),
        "{:?}",
        analysis.problems
    );
}

#[test]
fn the_power_tree_points_a_node_at_its_supply_and_a_stage_at_its_converter() {
    let root = project("power-at", TWO_BOARDS);
    let editor = open(&root);
    let tree = editor.power("TwoBoards").unwrap();
    let node = |id: &str| tree.nodes.iter().find(|n| n.id == id).unwrap();
    // The 3.3 V net: where the regulator that gives its capacity is declared.
    let at = node("power:3V3").at.as_ref().expect("the supply's place");
    assert_eq!(at.file, "src/main.nsm");
    assert_eq!(at.line, line_of(TWO_BOARDS, ".regulator:  LinearRegulator"));
    // A net nothing limits is still somewhere: at what draws from it.
    let vin = node("power:VIN");
    assert_eq!(vin.capacity, None);
    assert_eq!(
        vin.at,
        vin.supplies
            .iter()
            .chain(&vin.loads)
            .find_map(|d| d.at.clone())
    );
    assert!(vin.at.is_some());
    // Every node that has a current has a place.
    assert!(
        tree.nodes
            .iter()
            .filter(|n| !n.loads.is_empty() || !n.supplies.is_empty())
            .all(|n| n.at.is_some()),
        "{:?}",
        tree.nodes
    );
    // The converter's declaration is the edge's.
    let edge = tree
        .edges
        .iter()
        .find(|e| e.label == "stage (LdoStage)")
        .unwrap();
    let stage = tree.stages.iter().find(|s| s.label == edge.label).unwrap();
    assert_eq!(stage.at, edge.at);
    assert_eq!(
        stage.at.as_ref().unwrap().line,
        line_of(TWO_BOARDS, ".stage: LdoStage")
    );
}

/// What the observer is told, in order.
fn recording(editor: &Workspace) -> Arc<Mutex<Vec<AnalysisEvent>>> {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    editor.observe(Arc::new(move |e| sink.lock().unwrap().push(e)));
    heard
}

/// The stages of the progress events, each once, in the order they first came.
fn stages(events: &[AnalysisEvent]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for e in events {
        if let AnalysisEvent::Progress { stage, .. } = e
            && !seen.contains(stage)
        {
            seen.push(stage.clone());
        }
    }
    seen
}

#[test]
fn a_workspace_opened_deferred_is_pending_until_its_first_analysis_which_reports_its_steps() {
    let root = project("deferred", TWO_BOARDS);
    let editor = Workspace::open_deferred(Target::resolve_from(&root, None).unwrap()).unwrap();
    let heard = recording(&editor);
    assert!(editor.pending().is_some(), "nothing was analysed yet");
    // The tree and the files do not need the analysis.
    assert_eq!(editor.project().title, "demo");
    assert!(
        editor
            .read_file("src/main.nsm")
            .unwrap()
            .text
            .contains("LdoStage")
    );
    assert!(editor.pending().is_some());
    assert!(heard.lock().unwrap().is_empty());

    let analysis = editor.analysis();
    assert!(editor.pending().is_none());
    assert_eq!(analysis.version, 1);
    assert!(analysis.elapsed_ms.is_some());
    assert_eq!(analysis.boards.len(), 2);

    let events = heard.lock().unwrap().clone();
    assert!(
        matches!(events.first(), Some(AnalysisEvent::Started { since }) if *since > 1_700_000_000_000),
        "{events:?}"
    );
    assert_eq!(events.last(), Some(&AnalysisEvent::Finished), "{events:?}");
    assert_eq!(
        stages(&events),
        ["check", "solve", "lower", "draw", "modules"],
        "{events:?}"
    );
    // The system is one thing to solve and lower (the file has no stand-alone board), and each
    // of its boards is drawn.
    let counts = |wanted: &str| -> Vec<(u32, u32)> {
        events
            .iter()
            .filter_map(|e| match e {
                // The step's own lines; what the solver counts inside a step is `work` (ADR 0062).
                AnalysisEvent::Progress {
                    stage,
                    done,
                    total,
                    work: None,
                    ..
                } if stage == wanted => Some((*done, *total)),
                _ => None,
            })
            .collect()
    };
    assert_eq!(counts("check"), [(0, 1)]);
    assert_eq!(counts("solve"), [(0, 1)]);
    assert_eq!(counts("lower"), [(0, 1)]);
    assert_eq!(counts("draw"), [(0, 2), (1, 2)]);
    assert_eq!(counts("modules"), [(0, 1)]);
    // Inside the solve step the solver says what it counts, on lines of the same step.
    let counted: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            AnalysisEvent::Progress {
                stage,
                work: Some(w),
                ..
            } if stage == "solve" => Some(w.unit.as_str()),
            _ => None,
        })
        .collect();
    assert!(counted.contains(&"parts chosen"), "{counted:?}");
    let message = events
        .iter()
        .find_map(|e| match e {
            AnalysisEvent::Progress { stage, message, .. } if stage == "solve" => Some(message),
            _ => None,
        })
        .unwrap();
    assert_eq!(message, "solving TwoBoards (1 of 1)");
}

#[test]
fn a_change_on_disk_is_analysed_again_and_announced_but_a_check_of_typed_text_is_not() {
    let root = project("again", LED);
    let editor = Workspace::open_deferred(Target::resolve_from(&root, None).unwrap()).unwrap();
    let heard = recording(&editor);
    let first = editor.analysis();
    let after_first = heard.lock().unwrap().len();
    // Typing is checked, not announced; the check says how long it took.
    let checked = editor
        .check(
            "src/main.nsm",
            &LED.replace(".r1: Resistor {}", ".r1: Resistr {}"),
        )
        .unwrap();
    assert!(checked.elapsed_ms.is_some());
    assert_eq!(heard.lock().unwrap().len(), after_first);
    // Nothing changed on disk: no analysis.
    editor.analysis();
    assert_eq!(heard.lock().unwrap().len(), after_first);
    // Another program writes the file: the next look analyses it, and says so.
    fs::write(
        root.join("src/main.nsm"),
        format!("{LED}\n// edited elsewhere\n"),
    )
    .unwrap();
    let second = editor.analysis();
    assert_eq!(second.version, first.version + 1);
    let events = heard.lock().unwrap()[after_first..].to_vec();
    assert!(
        matches!(events.first(), Some(AnalysisEvent::Started { .. }))
            && events.last() == Some(&AnalysisEvent::Finished),
        "{events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, AnalysisEvent::Started { .. }))
            .count(),
        1
    );
}

#[test]
fn a_workspace_opened_as_before_is_never_pending() {
    let root = project("eager", LED);
    let editor = open(&root);
    assert!(editor.pending().is_none());
    assert_eq!(editor.analysis().version, 1);
}

#[test]
fn the_modules_are_told_from_the_first_and_in_order_and_not_one_message_each() {
    let root = project("modules", LED);
    for n in 0..40 {
        fs::write(
            root.join(format!("src/m{n}.nsm")),
            "//! A module.\n\nimport std::sources::Rail\n\ninterface Supply {\n    .rail: Rail\n}\n",
        )
        .unwrap();
    }
    let editor = Workspace::open_deferred(Target::resolve_from(&root, None).unwrap()).unwrap();
    let heard = recording(&editor);
    editor.analysis();
    let told: Vec<(u32, u32)> = heard
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            AnalysisEvent::Progress {
                stage, done, total, ..
            } if stage == "modules" => Some((*done, *total)),
            _ => None,
        })
        .collect();
    // 41 modules (the 40 and `power.nsm`), told in order from the first, at most once in
    // 100 ms: checking them takes far less than a second, so most are not told.
    assert_eq!(told.first(), Some(&(0, 41)), "{told:?}");
    assert!(told.windows(2).all(|w| w[0].0 < w[1].0), "{told:?}");
    assert!(told.len() < 41, "{told:?}");
}
