//! What the power model finds wrong (issue #187) is reported where the other findings are:
//! by `netsmith build` (an overdrawn supply stops it, the rest are notes), by the analysis the
//! editor serves, and in the power tree the page draws. The page does not work any of it out.
//!
//! Each kind has an invented small board: a rail that can draw more than its supply can give, a
//! rail nothing supplies, a rail with a load whose current the solver did not settle, and a
//! converter whose efficiency it did not settle.

use netsmith_kicad::testlib::TempDir;
use std::fs;

use netsmith::editor::Workspace;
use netsmith::project::Target;
use netsmith::{Stage, outcome_with};
use netsmith_check::Loader;
use netsmith_editor::{
    Backend,
    api::{ProblemStage, Severity},
};
use netsmith_solve::Catalogs;

/// The parts of the boards: a load, and a converter that makes `5V` of what it is given. The
/// converter says what it can give (`max_current`); `Converter` also states that its output is
/// at most that, which the solver checks, and `Loose` does not, as a part that only says what
/// it can give may not.
fn parts() -> String {
    let converter = |name: &str, relation: &str| {
        format!(
            "part {name} {{
    .vin: Rail
    .vout: Rail
    .max_current: Current
    .efficiency: Number
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current * .vout.voltage / (.vin.voltage * .efficiency)
{relation}
    pins {{
        .vin.pos  power \"VIN\"  \"1\"
        .vin.neg  power \"GND\"  \"2\"
        .vout.pos power \"VOUT\" \"3\"
    }}
}}
"
        )
    };
    format!(
        "import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

part Load {{
    .power: Rail

    pins {{
        .power.pos power \"VCC\" \"1\"
        .power.neg power \"GND\" \"2\"
    }}
}}

{}
{}",
        converter("Converter", "    -.vout.current <= .max_current"),
        converter("Loose", "")
    )
}

/// A converter (`kind`) of at most 100 mA from 12 V to 5 V, and a load on its output that draws
/// `load`. `more` is more of the converter's body: its efficiency, say.
fn supplied(kind: &str, load: &str, more: &str) -> String {
    format!(
        "{}
board Bench {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .input: Rail {{
        .voltage &= 12V
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }}
    .stage: {kind} {{
        .vout.voltage &= 5V
        .max_current &= 100mA
        {more}
        .vout.pos.net &= \"V5\"
        .vout.neg.net &= \"GND\"
    }}
    .load: Load {{ .power.current &= {load} }}
    .input ~ .stage.vin
    .stage.vout ~ .load.power
}}
",
        parts()
    )
}

const SETTLED: &str = ".efficiency &= 0.8..0.9";

/// A load that nothing supplies, on a connector, so that it builds into a schematic.
fn orphan() -> String {
    "import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Bench {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .load: Rail {
        .pos.net &= \"V5\"
        .neg.net &= \"GND\"
        .current &= 0mA..10mA
        .min_track &= 1mm..1m
    }
    .out: Header1x2
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}
"
    .to_string()
}

/// Two loads whose currents are not stated, on a rail with a supply: the net has three currents,
/// and two unknown ones cannot be worked out from one.
fn unsettled_loads() -> String {
    format!(
        "{}
board Bench {{
    .supply: Rail {{
        .voltage &= 5V
        .current &= -50mA..0mA
        .pos.net &= \"V5\"
        .neg.net &= \"GND\"
    }}
    .a: Load {{ }}
    .b: Load {{ }}
    .supply ~ .a.power
    .supply ~ .b.power
}}
",
        parts()
    )
}

fn outcome(src: &str) -> Result<netsmith::Outcome, Vec<netsmith::Problem>> {
    outcome_with("bench.nsm", src, &Loader::std(), &Catalogs::jlcpcb(), None)
}

/// The notes of the power model of the one board the source builds.
fn power_notes(src: &str) -> Vec<String> {
    let outcome = outcome(src).unwrap_or_else(|p| panic!("{}", netsmith::render_problems(&p)));
    assert_eq!(outcome.boards.len(), 1);
    outcome.boards[0].power_notes()
}

#[test]
fn a_rail_that_can_draw_more_than_its_supply_gives_stops_the_build() {
    // The load takes 0 to 200 mA from a converter that says it gives 100 mA and that no relation
    // holds to it: the solver has nothing to refuse, and the power model does.
    let src = supplied("Loose", "0mA..200mA", SETTLED);
    let Err(problems) = outcome(&src) else {
        panic!("a supply that can be overdrawn must stop the build");
    };
    assert_eq!(problems.len(), 1, "{problems:#?}");
    let p = &problems[0];
    assert_eq!(p.stage, Stage::Power);
    assert_eq!(
        p.message,
        "Bench: V5 can draw up to 200 mA, more than the 100 mA its supply can give"
    );
    assert_eq!(p.board.as_deref(), Some("Bench"));
    assert_eq!(p.path.as_deref(), Some("V5"));
    // It points at what supplies the rail.
    let at = p.at.as_ref().expect("it has a place");
    assert_eq!(at.file, "bench.nsm");
    assert!(
        src.lines()
            .nth(at.line - 1)
            .unwrap()
            .contains(".stage: Loose"),
        "{at:?}"
    );
    assert!(
        p.rendered
            .starts_with("error: Bench: V5 can draw up to 200 mA"),
        "{}",
        p.rendered
    );
    assert!(p.rendered.contains("--> bench.nsm:"), "{}", p.rendered);

    // A load the supply can give builds, and the model has nothing to say.
    let notes = power_notes(&supplied("Loose", "0mA..100mA", SETTLED));
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn a_supply_the_solver_can_show_overdrawn_is_still_the_solvers_to_refuse() {
    // With the converter's own relation the solver says it, as before, and says it first.
    let Err(problems) = outcome(&supplied("Converter", "0mA..200mA", SETTLED)) else {
        panic!("the solver refuses it");
    };
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].stage, Stage::Solve);
}

#[test]
fn a_rail_nothing_supplies_is_a_note() {
    assert_eq!(
        power_notes(&orphan()),
        ["Bench: V5 has no source: nothing supplies it"]
    );
    let outcome = outcome(&orphan()).unwrap();
    let issue = &outcome.boards[0].power[0];
    assert_eq!(issue.severity, Severity::Note);
    assert_eq!(issue.subject, "V5");
    assert!(issue.at.is_some(), "{issue:?}");
}

#[test]
fn a_load_whose_current_the_solver_did_not_settle_is_a_note() {
    let notes = power_notes(&unsettled_loads());
    assert_eq!(
        notes,
        ["Bench: V5 has 2 loads with a current the solver has not settled"],
        "{notes:?}"
    );
}

#[test]
fn a_converter_whose_efficiency_the_solver_did_not_settle_is_a_note() {
    // The input current is stated, so the stage is a stage of the tree; the efficiency, which
    // would settle it, is not.
    let unsettled = supplied("Loose", "0mA..50mA", ".vin.current &= 0mA..100mA");
    let notes = power_notes(&unsettled);
    assert_eq!(
        notes,
        ["Bench: stage has an efficiency the solver has not settled"],
        "{notes:?}"
    );
    let outcome = outcome(&unsettled).unwrap();
    let issue = &outcome.boards[0].power[0];
    assert_eq!(issue.severity, Severity::Note);
    assert!(issue.at.is_some(), "{issue:?}");
    // Stated, it is settled.
    let notes = power_notes(&supplied("Loose", "0mA..50mA", SETTLED));
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn a_system_is_judged_whole_and_each_board_gets_its_own_notes() {
    // The rail of `load` is supplied by the other board through a join: the system's tree, which
    // the issues of its boards are read from, has a source for it, so the note is not made.
    let src = format!(
        "{}
board Maker {{
    .out: Rail {{
        .voltage &= 5V
        .current &= -50mA..0mA
        .pos.net &= \"V5\"
        .neg.net &= \"GND\"
    }}
}}

board User {{
    .load: Load {{
        .power.current &= 0mA..10mA
        .power.pos.net &= \"V5\"
        .power.neg.net &= \"GND\"
    }}
    .in: Rail {{
        .voltage &= 5V
        .pos.net &= \"V5\"
        .neg.net &= \"GND\"
    }}
    .in ~ .load.power
}}

system Pair {{
    .maker: Maker
    .user: User
    .maker.out ~ .user.in
}}
",
        parts()
    );
    let outcome = outcome(&src).unwrap_or_else(|p| panic!("{}", netsmith::render_problems(&p)));
    let user = outcome
        .boards
        .iter()
        .find(|b| b.label() == "Pair/user")
        .expect("the system's user board");
    assert!(
        user.power.iter().all(|i| !i.text.contains("no source")),
        "{:?}",
        user.power
    );
    // The system's tree, which the page draws for a system, says the same.
    let root = project("system", &src);
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let whole = editor.power("Pair").unwrap();
    assert!(
        whole.issues.iter().all(|i| !i.text.contains("no source")),
        "{:?}",
        whole.issues
    );
}

// ---- the command line ----------------------------------------------------------------

fn project(name: &str, src: &str) -> TempDir {
    let root = TempDir::new(&format!("power-issues-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"bench\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), src).unwrap();
    root
}

// ---- the analysis the editor serves ------------------------------------------------------

fn analysis_of(name: &str, src: &str) -> (TempDir, netsmith_editor::api::Analysis) {
    let root = project(name, src);
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    (root, analysis)
}

#[test]
fn the_analysis_carries_the_notes_as_the_power_stage_with_the_same_text_as_build() {
    let src = orphan();
    let (_root, analysis) = analysis_of("analysis-note", &src);
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let found: Vec<_> = analysis
        .problems
        .iter()
        .filter(|p| p.stage == Some(ProblemStage::Power))
        .collect();
    assert_eq!(found.len(), 1, "{:?}", analysis.problems);
    let p = found[0];
    assert_eq!(p.severity, Severity::Note);
    assert_eq!(p.message, power_notes(&src)[0]);
    assert_eq!(p.board.as_deref(), Some("Bench"));
    assert_eq!(p.path.as_deref(), Some("V5"));
    let at = p.at.as_ref().expect("it points into the source");
    assert_eq!(at.file, "src/main.nsm");
    // The count the dock shows is the count a pipeline reads.
    assert_eq!(
        analysis.notes as usize,
        analysis
            .problems
            .iter()
            .filter(|p| p.severity == Severity::Note)
            .count()
    );
}

#[test]
fn the_analysis_reports_an_overdrawn_supply_as_an_error_of_the_power_stage() {
    let (root, analysis) = analysis_of("analysis-over", &supplied("Loose", "0mA..200mA", SETTLED));
    assert!(analysis.boards.is_empty(), "the board does not build");
    assert_eq!(analysis.errors, 1, "{:?}", analysis.problems);
    let p = &analysis.problems[0];
    assert_eq!(p.severity, Severity::Error);
    assert_eq!(p.stage, Some(ProblemStage::Power));
    assert_eq!(
        p.message,
        "Bench: V5 can draw up to 200 mA, more than the 100 mA its supply can give"
    );
    assert_eq!(p.at.as_ref().unwrap().file, "src/main.nsm");
    drop(root);
}

#[test]
fn the_power_tree_carries_the_same_issues_for_the_page_to_draw() {
    let src = supplied("Loose", "0mA..50mA", ".vin.current &= 0mA..100mA");
    let root = project("tree", &src);
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    let tree = editor.power("Bench").unwrap();
    let stage = tree
        .issues
        .iter()
        .find(|i| i.subject == "stage")
        .unwrap_or_else(|| panic!("{:?}", tree.issues));
    assert_eq!(stage.severity, Severity::Note);
    assert_eq!(stage.text, "has an efficiency the solver has not settled");
    // Every issue of the tree is a problem of the analysis, word for word.
    for issue in &tree.issues {
        let message = format!("Bench: {} {}", issue.subject, issue.text);
        assert!(
            analysis
                .problems
                .iter()
                .any(|p| p.message == message && p.stage == Some(ProblemStage::Power)),
            "{message} is not in {:?}",
            analysis.problems
        );
    }
}
