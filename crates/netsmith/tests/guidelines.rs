//! Guideline findings (ADR 0036, ADR 0030): a warning by default, an error with
//! `guidelines = "error"` in `netsmith.toml` or `--strict` for one run, shown by the
//! command line, the fab notes and the editor.

use netsmith_kicad::testlib::TempDir;
use std::{fs, path::Path, process::Command};

use netsmith::editor::Workspace;
use netsmith::project::{Manifest, Target};
use netsmith::{Libraries, build_with, outcome_with};
use netsmith_check::Loader;
use netsmith_editor::{Backend, api::Severity};
use netsmith_solve::Catalogs;

/// A 2 A supply and load on a `5V` / `GND` pair, with the rail drawn `width` wide.
fn board(width: &str) -> String {
    format!(
        "import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Heater {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .load: Rail {{
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= 0A..2A
        .min_track &= {width}..1m
    }}
    .source: Rail {{
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= -2A..0A
    }}
    .supply: Header1x2
    .out: Header1x2
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}}
"
    )
}

fn dir(name: &str) -> TempDir {
    let dir = TempDir::new(&format!("guidelines-test-{name}"));
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn project(name: &str, manifest_line: &str, width: &str) -> TempDir {
    let root = dir(name);
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"heater\"\n{manifest_line}\n"),
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), board(width)).unwrap();
    root
}

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env(
            "NETSMITH_KICAD_SYMBOLS",
            netsmith_kicad::testlib::sample_symbols(),
        )
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_kicad::testlib::sample_footprints(),
        )
        .output()
        .unwrap()
}

fn build(src: &str, catalogs: &Catalogs) -> Result<Vec<netsmith::Built>, String> {
    build_with(
        "heater.nsm",
        src,
        &Loader::std(),
        catalogs,
        &Libraries::sample(),
    )
}

#[test]
fn a_guideline_finding_warns_and_the_board_builds() {
    let boards = build(&board("0.2mm"), &Catalogs::jlcpcb()).unwrap();
    let warnings = boards[0].guideline_warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings[0].starts_with("Heater: IPC-2221 guideline: net `5V` carries up to 2 A")
            && warnings[0].contains("0.2 mm wide")
            && warnings[0].contains("IPC-2221 needs 0.79 mm"),
        "{}",
        warnings[0]
    );
    // A wide enough rail has none.
    let boards = build(&board("1mm"), &Catalogs::jlcpcb()).unwrap();
    assert!(boards[0].guidelines.is_empty());
}

#[test]
fn strict_guidelines_refuse_the_build_with_the_numbers_and_the_line() {
    let strict = Catalogs::jlcpcb().with_strict_guidelines(true);
    let Err(e) = build(&board("0.2mm"), &strict) else {
        panic!("a strict build of a finding must fail");
    };
    assert!(
        e.contains("error: Heater: IPC-2221 guideline: net `5V` carries up to 2 A")
            && e.contains("that width carries 745 mA and IPC-2221 needs 0.79 mm")
            && e.contains("--> heater.nsm:"),
        "{e}"
    );
    // Both nets are told, each once.
    assert_eq!(e.matches("guideline: net").count(), 2, "{e}");
    assert!(build(&board("1mm"), &strict).is_ok());
}

#[test]
fn the_manifest_key_is_warn_or_error() {
    let with = |word: &str| {
        Manifest::parse(&format!(
            "[project]\nname = \"a\"\nguidelines = \"{word}\"\n"
        ))
    };
    assert_eq!(with("warn").unwrap().guidelines.as_deref(), Some("warn"));
    assert_eq!(with("error").unwrap().guidelines.as_deref(), Some("error"));
    let e = with("strict").unwrap_err();
    assert!(
        e.contains("guidelines = \"strict\" is not one of \"warn\" or \"error\""),
        "{e}"
    );
    assert_eq!(
        Manifest::parse("[project]\nname = \"a\"\n")
            .unwrap()
            .guidelines,
        None
    );
    assert!(!Manifest::new("a").render().contains("guidelines"));
    let m = Manifest {
        guidelines: Some("error".into()),
        ..Manifest::new("a")
    };
    assert_eq!(Manifest::parse(&m.render()), Ok(m));
}

#[test]
fn the_manifest_policy_reaches_the_catalogs_every_command_uses() {
    for (line, strict) in [
        ("", false),
        ("guidelines = \"warn\"", false),
        ("guidelines = \"error\"", true),
    ] {
        let root = project("policy", line, "0.2mm");
        let catalogs = Target::resolve_from(&root, None)
            .unwrap()
            .catalogs()
            .unwrap();
        assert_eq!(catalogs.strict_guidelines(), strict, "{line}");
    }
}

#[test]
fn the_command_line_warns_by_default_and_refuses_with_strict_or_the_manifest_key() {
    let root = project("cli", "", "0.2mm");
    let run = netsmith(&root, &["build"]);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{said}");
    assert!(
        said.contains("warning: Heater: IPC-2221 guideline: net `5V` carries up to 2 A"),
        "{said}"
    );

    // `--strict` is for one run: the next run without it warns again.
    let run = netsmith(&root, &["build", "--strict"]);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success(), "{said}");
    assert!(
        said.contains("error: Heater: IPC-2221 guideline: net `5V`"),
        "{said}"
    );
    assert!(netsmith(&root, &["build"]).status.success());

    let root = project("cli-error", "guidelines = \"error\"", "0.2mm");
    let run = netsmith(&root, &["build"]);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("IPC-2221 guideline"));

    // A board that meets the guideline builds under either.
    let root = project("cli-ok", "guidelines = \"error\"", "1mm");
    let run = netsmith(&root, &["build"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(!String::from_utf8_lossy(&run.stderr).contains("guideline"));
}

#[test]
fn fab_puts_the_finding_in_the_notes_for_the_person_ordering() {
    let root = project("fab", "", "0.2mm");
    let run = netsmith(&root, &["fab", "--allow-drc-violations"]);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(
        run.status.success(),
        "{said}\n{}",
        String::from_utf8_lossy(&run.stdout)
    );
    let notes = fs::read_to_string(root.join("fab/Heater/fab-notes.txt")).unwrap();
    assert!(
        notes.contains("warning: Heater: IPC-2221 guideline: net `5V` carries up to 2 A"),
        "{notes}"
    );
    // With `--strict` nothing is made.
    let run = netsmith(
        &root,
        &[
            "fab",
            "--allow-drc-violations",
            "--strict",
            "--out",
            "strict",
        ],
    );
    assert!(!run.status.success());
    assert!(!root.join("strict").exists());
}

#[test]
fn the_editor_shows_the_finding_as_a_warning_at_the_rail_and_an_error_if_the_project_says() {
    let root = project("editor", "", "0.2mm");
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let found: Vec<_> = analysis
        .problems
        .iter()
        .filter(|p| p.message.contains("IPC-2221 guideline"))
        .collect();
    assert_eq!(found.len(), 2, "{:?}", analysis.problems);
    assert!(found.iter().all(|p| p.severity == Severity::Warning));
    assert_eq!(analysis.warnings, 2);
    // The page is told the stage, and the board and the rail the message is about.
    for p in &found {
        assert_eq!(p.stage, Some(netsmith_editor::api::ProblemStage::Guideline));
        assert_eq!(p.board.as_deref(), Some("Heater"), "{p:?}");
        assert!(p.path.is_some(), "{p:?}");
    }
    let at = found[0].at.as_ref().expect("it points into the source");
    assert_eq!(at.file, "src/main.nsm");
    let text = fs::read_to_string(root.join("src/main.nsm")).unwrap();
    assert!(
        text.lines()
            .nth(at.line as usize - 1)
            .unwrap()
            .contains(".load: Rail"),
        "{at:?}"
    );

    // The same board under `guidelines = "error"` does not build, and the findings are errors.
    let root = project("editor-error", "guidelines = \"error\"", "0.2mm");
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    assert!(analysis.boards.is_empty());
    assert_eq!(analysis.errors, 2, "{:?}", analysis.problems);
    assert!(
        analysis
            .problems
            .iter()
            .all(|p| p.severity == Severity::Error
                && p.message.contains("IPC-2221 guideline")
                && p.stage == Some(netsmith_editor::api::ProblemStage::Guideline))
    );
}

#[test]
fn a_board_with_the_finding_still_builds_through_outcome_for_tools_that_only_read() {
    let outcome = outcome_with(
        "heater.nsm",
        &board("0.2mm"),
        &Loader::std(),
        &Catalogs::jlcpcb(),
        None,
    )
    .unwrap_or_else(|p| panic!("{}", netsmith::render_problems(&p)));
    assert_eq!(outcome.boards[0].guidelines.len(), 2);
}

// ---- the router sizes every net (issue #4, ADR 0036 amendment) -----------------------

/// The heater with no `.min_track` anywhere, so that the width is the router's to work out.
fn unstated_project(name: &str, manifest_line: &str) -> TempDir {
    let root = dir(name);
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"heater\"\n{manifest_line}\n"),
    )
    .unwrap();
    let src = board("0.2mm").replace("        .min_track &= 0.2mm..1m\n", "");
    assert!(!src.contains("min_track"));
    fs::write(root.join("src/main.nsm"), src).unwrap();
    root
}

#[test]
fn a_board_that_states_no_width_is_not_warned_and_is_routed_at_the_width_the_current_needs() {
    // Under `--strict` too: there is nothing to refuse.
    let root = unstated_project("sized", "");
    let run = netsmith(&root, &["fab", "--allow-drc-violations", "--strict"]);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{said}");
    assert!(!said.contains("guideline"), "{said}");
    let notes = fs::read_to_string(root.join("fab/Heater/fab-notes.txt")).unwrap();
    assert!(!notes.contains("IPC-2221"), "{notes}");
    // The board file draws 5V and GND at 0.79 mm (2 A needs 0.7814 mm in 1 oz at 10 K).
    let pcb = fs::read_to_string(root.join("fab/Heater/Heater.kicad_pcb")).unwrap();
    assert!(
        pcb.contains("(width 0.79)"),
        "no track of 0.79 mm in the board"
    );
    let pro = fs::read_to_string(root.join("fab/Heater/Heater.kicad_pro")).unwrap();
    assert!(pro.contains("Track_0.79mm"), "a net class of 0.79 mm");
}

#[test]
fn the_editor_shows_no_warning_for_a_net_the_router_sized() {
    let root = unstated_project("editor-sized", "");
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let a = editor.analysis();
    assert_eq!(a.errors, 0, "{:?}", a.problems);
    assert!(
        !a.problems.iter().any(|p| p.message.contains("IPC-2221")),
        "{:?}",
        a.problems
    );
}
