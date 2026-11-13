//! The schematic layout file in the editor (ADR 0061, stage 3): the server reads
//! `<stem>.sch.toml` beside the top-level file, draws with it, tells what is wrong with it in the
//! problems list (stage `layout`, the file and line) and reads and writes it over
//! `/api/schematic-hints`.

use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use netsmith::editor::Workspace;
use netsmith::project::Target;
use netsmith_editor::{
    AnalysisEvent, Backend,
    api::{Analysis, HintFindingKind, Problem, ProblemStage, Severity},
};
use netsmith_testkit::TempDir;

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");
const LED: &str = "TwoBoards/led";
const POWER: &str = "TwoBoards/power";
const FILE: &str = "src/main.sch.toml";

/// A project of `two_boards.nsm`, and how many analyses of the files on disk have begun.
fn project(name: &str) -> (TempDir, Workspace, Arc<AtomicU32>) {
    let root = TempDir::new(&format!("editor-hints-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), TWO_BOARDS).unwrap();
    let editor = Workspace::open_deferred(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    let started = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&started);
    editor.observe(Arc::new(move |event| {
        if matches!(event, AnalysisEvent::Started { .. }) {
            counted.fetch_add(1, Ordering::SeqCst);
        }
    }));
    // The first analysis.
    editor.analysis();
    (root, editor, started)
}

fn layout_problems(analysis: &Analysis) -> Vec<&Problem> {
    analysis
        .problems
        .iter()
        .filter(|p| p.stage == Some(ProblemStage::Layout))
        .collect()
}

fn line_of(text: &str, needle: &str) -> u32 {
    text.lines().position(|l| l.contains(needle)).unwrap() as u32 + 1
}

/// The file's text with the resistor's block (it and the LED, which it serves) moved right by
/// 25.4 mm: honoured by the drawing of the LED board. A part that is the reference part of its
/// sheet is not used for this: the sheet is drawn from it, so moving it moves the whole drawing
/// and changes no distance in it.
const MOVED: &str = "version = 1\n\n[LedBoard.parts.r1]\ndx = 25.4\ndy = 0\n";

#[test]
fn without_a_file_there_are_no_hints_and_nothing_is_wrong() {
    let (_root, editor, _) = project("none");
    let hints = editor.schematic_hints(LED).unwrap();
    assert_eq!(hints.board, LED);
    assert_eq!(hints.file, FILE);
    assert!(!hints.exists);
    assert_eq!(hints.stamp, "none");
    assert_eq!(hints.text, "");
    assert!(hints.errors.is_empty() && hints.boards.is_empty() && hints.findings.is_empty());
    assert!(layout_problems(&editor.analysis()).is_empty());
    assert_eq!(editor.schematic_hints("Nope").unwrap_err().status, 404);
}

#[test]
fn a_file_is_read_as_it_says_with_the_entries_that_match_nothing_as_findings_and_problems() {
    let (root, editor, _) = project("read");
    let text = "\
version = 1

[LedBoard.parts.power_in]
dx = 2.54
dy = -1.27
rotate = 90

[LedBoard.parts.gone]
dx = 1.27
dy = 0

[PowerBoard.sheets.stage.parts.\"output_cap\"]
mirror = true

[PowerBoard.sheets.radio.parts.u1]
dx = 1.27
dy = 0

[Retired.parts.x]
dx = 1.27
dy = 0
";
    fs::write(root.join(FILE), text).unwrap();

    let hints = editor.schematic_hints(LED).unwrap();
    assert!(hints.exists);
    assert_eq!(hints.text, text);
    assert!(hints.errors.is_empty());
    let moved = &hints.boards["LedBoard"].parts["power_in"];
    assert_eq!(
        (moved.dx, moved.dy, moved.rotate, moved.line),
        (
            Some(2.54),
            Some(-1.27),
            Some(90),
            line_of(text, "LedBoard.parts.power_in")
        )
    );
    assert_eq!(
        hints.boards["PowerBoard"].sheets["stage"].parts["output_cap"].mirror,
        Some(true)
    );
    // The board asked for has its own findings; the table for a board the source lacks is a
    // finding for every board.
    let kinds: Vec<(HintFindingKind, u32)> =
        hints.findings.iter().map(|f| (f.kind, f.line)).collect();
    assert_eq!(
        kinds,
        [
            (
                HintFindingKind::StaleEntry,
                line_of(text, "LedBoard.parts.gone")
            ),
            (
                HintFindingKind::UnknownBoard,
                line_of(text, "Retired.parts.x")
            ),
        ],
        "{:?}",
        hints.findings
    );
    assert!(
        hints.findings[0]
            .message
            .contains("the design has no part `gone`"),
        "{}",
        hints.findings[0].message
    );
    let power = editor.schematic_hints(POWER).unwrap();
    let power_kinds: Vec<HintFindingKind> = power.findings.iter().map(|f| f.kind).collect();
    assert_eq!(
        power_kinds,
        [HintFindingKind::StaleSheet, HintFindingKind::UnknownBoard],
        "{:?}",
        power.findings
    );

    // The problems list has them as notes of stage `layout`, at the file and the line.
    let analysis = editor.analysis();
    let problems = layout_problems(&analysis);
    assert_eq!(problems.len(), 3, "{problems:#?}");
    for p in &problems {
        assert_eq!(p.severity, Severity::Note);
        let at = p.at.as_ref().expect("a finding says where");
        assert_eq!(at.file, FILE);
        assert!(
            p.rendered
                .starts_with(&format!("note: {FILE}:{}: ", at.line)),
            "{}",
            p.rendered
        );
        // The location is the header's whole line.
        let header = text.lines().nth(at.line as usize - 1).unwrap();
        assert!(header.starts_with('['), "{header}");
        assert_eq!(
            (at.column, at.end_column),
            (1, header.chars().count() as u32 + 1)
        );
    }
    let gone = problems
        .iter()
        .find(|p| p.message.contains("`gone`"))
        .expect("the finding of the entry that is gone");
    assert_eq!(gone.board.as_deref(), Some(LED));
    assert_eq!(
        gone.at.as_ref().unwrap().line,
        line_of(text, "LedBoard.parts.gone")
    );
    let unknown = problems
        .iter()
        .find(|p| p.message.contains("no board `Retired`"))
        .expect("the finding of the unknown board");
    assert_eq!(
        unknown.board, None,
        "a table for a board that is not there is the file's"
    );
    assert_eq!(
        unknown.at.as_ref().unwrap().line,
        line_of(text, "Retired.parts.x")
    );
    // Nothing of it is an error: the project builds.
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
}

#[test]
fn the_drawing_has_the_file_and_is_the_same_as_before_when_the_file_is_gone() {
    let (root, editor, _) = project("draw");
    let before = editor.schematic(LED).unwrap();
    let power = editor.schematic(POWER).unwrap();

    fs::write(root.join(FILE), MOVED).unwrap();
    let after = editor.schematic(LED).unwrap();
    assert_ne!(after, before, "the connector moved");
    assert!(
        layout_problems(&editor.analysis()).is_empty(),
        "an entry the drawing honours is no finding"
    );
    // The other board has no table: its drawing is what it was.
    assert_eq!(editor.schematic(POWER).unwrap(), power);

    // Deleting the file brings the drawing back, byte for byte.
    fs::remove_file(root.join(FILE)).unwrap();
    assert_eq!(editor.schematic(LED).unwrap(), before);
    assert!(!editor.schematic_hints(LED).unwrap().exists);
}

#[test]
fn a_file_that_does_not_read_leaves_the_drawing_as_it_is_without_one_and_says_where() {
    let (root, editor, _) = project("broken");
    let before = editor.schematic(LED).unwrap();
    let text = "version = 1\n\n[LedBoard.parts.power_in]\ndx = 2.54\ndy = 0\nspin = 3\n";
    fs::write(root.join(FILE), text).unwrap();

    assert_eq!(
        editor.schematic(LED).unwrap(),
        before,
        "the drawing has no hints"
    );
    let analysis = editor.analysis();
    let problems = layout_problems(&analysis);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    let p = problems[0];
    assert_eq!(
        p.severity,
        Severity::Warning,
        "the file is wrong, the board is not"
    );
    assert_eq!(p.board, None);
    let at = p.at.as_ref().unwrap();
    assert_eq!((at.file.as_str(), at.line), (FILE, line_of(text, "spin")));
    assert!(p.message.contains("the file is ignored"), "{}", p.message);
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    assert_eq!(analysis.boards.len(), 2, "both boards are still drawn");

    // The route says the same, and keeps the text as it is.
    let hints = editor.schematic_hints(LED).unwrap();
    assert!(hints.exists && hints.boards.is_empty());
    assert_eq!(hints.text, text);
    assert_eq!(hints.errors.len(), 1);
    assert_eq!(hints.errors[0].line, line_of(text, "spin"));

    // Mending the file brings the hints back.
    fs::write(root.join(FILE), MOVED).unwrap();
    assert_ne!(editor.schematic(LED).unwrap(), before);
    assert!(layout_problems(&editor.analysis()).is_empty());
}

#[test]
fn putting_writes_the_canonical_text_and_the_drawing_has_it() {
    let (root, editor, _) = project("put");
    let before = editor.schematic(LED).unwrap();
    // Not as the editor writes it: a comment, spaces, a number with a trailing zero.
    let body = "# my layout\nversion=1\n[LedBoard.parts.r1]\ndx = 25.40\ndy=0 # no move down\n";
    let answer = editor
        .write_schematic_hints(LED, body, Some("none"))
        .unwrap();

    let canonical = netsmith_view::hints::to_toml(&netsmith_view::hints::parse(body).unwrap());
    let on_disk = fs::read_to_string(root.join(FILE)).unwrap();
    assert_eq!(on_disk, canonical);
    assert_eq!(
        on_disk, MOVED,
        "version, a blank line, the table, the numbers as the writer spells them"
    );
    assert!(!on_disk.contains('#'), "a comment is not kept (ADR 0061)");
    // The answer is the file as it is now, as a GET says it.
    assert!(answer.exists);
    assert_eq!(answer.text, on_disk);
    assert_eq!(answer.boards["LedBoard"].parts["r1"].dx, Some(25.4));
    assert_eq!(answer, editor.schematic_hints(LED).unwrap());
    // And the drawing has it.
    assert_ne!(editor.schematic(LED).unwrap(), before);
    // Writing what the editor wrote changes nothing.
    let again = editor
        .write_schematic_hints(LED, &on_disk, Some(&answer.stamp))
        .unwrap();
    assert_eq!(fs::read_to_string(root.join(FILE)).unwrap(), on_disk);
    assert_eq!(again.text, on_disk);
}

#[test]
fn putting_text_that_does_not_read_is_refused_with_its_line_and_writes_nothing() {
    let (root, editor, started) = project("put-bad");
    let version = editor.analysis().version;
    let body = "version = 1\n\n[LedBoard.parts.power_in]\ndx = 1.3\ndy = 0\nspin = 3\n";
    let error = editor.write_schematic_hints(LED, body, None).unwrap_err();
    assert_eq!(error.status, 422);
    let detail = error.detail.as_ref().expect("a structured failure");
    assert_eq!(detail.code, "schematic_hints_invalid");
    let field = |name: &str| {
        detail
            .fields
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .unwrap_or_else(|| panic!("no field {name}: {:?}", detail.fields))
    };
    assert_eq!(field("file"), FILE);
    assert_eq!(
        field("line"),
        line_of(body, "dx = 1.3").to_string(),
        "the first fault"
    );
    assert_eq!(
        field("lines"),
        format!("{},{}", line_of(body, "dx = 1.3"), line_of(body, "spin")),
        "all of them, in the order of the file"
    );
    assert!(
        error
            .message
            .contains(&format!("{FILE}:{}: ", line_of(body, "dx = 1.3"))),
        "{}",
        error.message
    );
    // Nothing was written or analysed.
    assert!(!root.join(FILE).exists());
    assert_eq!(editor.analysis().version, version);
    assert_eq!(started.load(Ordering::SeqCst), 1);
}

#[test]
fn putting_over_a_file_that_changed_since_it_was_read_is_a_conflict() {
    let (root, editor, _) = project("put-stale");
    let first = editor.write_schematic_hints(LED, MOVED, None).unwrap();
    // Another program edits it.
    let theirs = "version = 1\n\n[LedBoard.parts.power_in]\ndx = 5.08\ndy = 2.54\n";
    fs::write(root.join(FILE), theirs).unwrap();
    let error = editor
        .write_schematic_hints(LED, "version = 1\n", Some(&first.stamp))
        .unwrap_err();
    assert_eq!(error.status, 409, "{}", error.message);
    assert_eq!(fs::read_to_string(root.join(FILE)).unwrap(), theirs);
    // Without a stamp the page says it does not mind.
    editor
        .write_schematic_hints(LED, "version = 1\n", None)
        .unwrap();
    assert_eq!(
        fs::read_to_string(root.join(FILE)).unwrap(),
        "version = 1\n"
    );
}

#[test]
fn a_put_analyses_once_and_the_watcher_finds_nothing_new() {
    let (root, editor, started) = project("once");
    let version = editor.analysis().version;
    assert_eq!(started.load(Ordering::SeqCst), 1);

    let answer = editor.write_schematic_hints(LED, MOVED, None).unwrap();
    assert_eq!(
        started.load(Ordering::SeqCst),
        2,
        "one analysis for the write"
    );
    assert_eq!(answer.version, version + 1);
    // What the server's watcher and the page's requests do next: look again.
    for _ in 0..3 {
        assert_eq!(editor.analysis().version, version + 1);
        editor.schematic(LED).unwrap();
        editor.schematic_hints(LED).unwrap();
    }
    assert_eq!(
        started.load(Ordering::SeqCst),
        2,
        "no echo of our own write"
    );

    // A change by another program is one analysis too, and so is deleting the file.
    fs::write(
        root.join(FILE),
        "version = 1\n\n[LedBoard.parts.power_in]\ndx = 3.81\ndy = 0\n",
    )
    .unwrap();
    assert_eq!(editor.analysis().version, version + 2);
    assert_eq!(editor.analysis().version, version + 2);
    fs::remove_file(root.join(FILE)).unwrap();
    assert_eq!(editor.analysis().version, version + 3);
    assert_eq!(editor.analysis().version, version + 3);
    assert_eq!(started.load(Ordering::SeqCst), 4);
}

#[test]
fn an_entry_the_drawing_cannot_honour_is_a_finding_with_the_line() {
    let (root, editor, _) = project("unusable");
    let before = editor.schematic(LED).unwrap();
    // 1270 mm is far outside the drawing (more than the 127 mm it allows): the engine refuses it,
    // and the entry is left out.
    let text = "version = 1\n\n[LedBoard.parts.power_in]\ndx = 1270\ndy = 0\n";
    fs::write(root.join(FILE), text).unwrap();
    let hints = editor.schematic_hints(LED).unwrap();
    assert!(hints.errors.is_empty(), "{:?}", hints.errors);
    let kinds: Vec<HintFindingKind> = hints.findings.iter().map(|f| f.kind).collect();
    assert_eq!(kinds, [HintFindingKind::Unusable], "{:?}", hints.findings);
    assert_eq!(
        hints.findings[0].line,
        line_of(text, "LedBoard.parts.power_in")
    );
    assert!(hints.findings[0].message.ends_with("it is ignored"));
    assert_eq!(editor.schematic(LED).unwrap(), before, "ignored");
    let analysis = editor.analysis();
    let problems = layout_problems(&analysis);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].board.as_deref(), Some(LED));
}

#[test]
fn a_check_of_typed_text_reads_the_file_too() {
    let (root, editor, started) = project("check");
    fs::write(
        root.join(FILE),
        "version = 1\n\n[LedBoard.parts.gone]\ndx = 1.27\ndy = 0\n",
    )
    .unwrap();
    editor.analysis();
    let count = started.load(Ordering::SeqCst);
    let checked = editor.check("src/main.nsm", TWO_BOARDS).unwrap();
    assert_eq!(layout_problems(&checked).len(), 1, "{:?}", checked.problems);
    assert_eq!(
        started.load(Ordering::SeqCst),
        count,
        "a check is not announced"
    );
}

// ---- the file in the tree (stage 4) ----

#[test]
fn the_tree_lists_the_file_once_it_exists_and_it_opens_and_saves_like_a_source() {
    use netsmith_editor::api::FileRole;
    let (root, editor, _) = project("tree");
    let layout_files = |editor: &Workspace| -> Vec<String> {
        editor
            .project()
            .files
            .iter()
            .filter(|f| f.role == FileRole::SchematicLayout)
            .map(|f| f.path.clone())
            .collect()
    };
    assert!(layout_files(&editor).is_empty(), "no file, no row");
    fs::write(root.join(FILE), MOVED).unwrap();
    assert_eq!(layout_files(&editor), [FILE]);
    // It is read and saved by its name, as a source is, so a problem at `src/main.sch.toml:N` opens.
    let content = editor.read_file(FILE).unwrap();
    assert_eq!(content.text, MOVED);
    assert!(!content.readonly);
    let edited = MOVED.replace("25.4", "5.08");
    editor.write_file(FILE, &edited, &content.stamp).unwrap();
    assert_eq!(fs::read_to_string(root.join(FILE)).unwrap(), edited);
    // It has no outline: it is not a source.
    assert!(
        editor
            .project()
            .files
            .iter()
            .find(|f| f.role == FileRole::SchematicLayout)
            .unwrap()
            .defs
            .is_empty()
    );
}

#[test]
fn the_file_is_removed_alone_and_with_its_source_but_never_renamed_on_its_own() {
    let (root, editor, _) = project("remove");
    fs::write(root.join(FILE), MOVED).unwrap();
    let e = editor.rename_path(FILE, "src/other.sch.toml").unwrap_err();
    assert_eq!(e.status, 400, "{}", e.message);
    assert!(e.message.contains("rename its source"), "{}", e.message);
    editor.delete_path(FILE, false).unwrap();
    assert!(!root.join(FILE).exists() && root.join("src/main.nsm").exists());
    // With its top-level source it goes too: an arrangement of a drawing that is gone.
    fs::write(root.join(FILE), MOVED).unwrap();
    editor.delete_path("src/main.nsm", true).unwrap();
    assert!(!root.join(FILE).exists());
}

#[test]
fn a_check_of_the_files_unsaved_text_reads_that_text() {
    let (root, editor, _) = project("typed");
    fs::write(root.join(FILE), MOVED).unwrap();
    editor.analysis();
    // Typed, not saved: a key the file does not have. The check says where, and the disk is not touched.
    let typed = "version = 1\n\n[LedBoard.parts.r1]\ndx = 25.4\ndy = 0\nspin = 3\n";
    let checked = editor.check(FILE, typed).unwrap();
    let found = layout_problems(&checked);
    assert_eq!(found.len(), 1, "{:?}", checked.problems);
    assert_eq!(found[0].at.as_ref().unwrap().line, line_of(typed, "spin"));
    assert!(found[0].message.contains("the file is ignored"));
    assert_eq!(fs::read_to_string(root.join(FILE)).unwrap(), MOVED);
    assert!(layout_problems(&editor.analysis()).is_empty());
}
