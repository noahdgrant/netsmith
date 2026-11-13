//! Cancelling the analysis, a build and a fab (ADR 0062): what a cancelled job leaves behind
//! (nothing), what the page is told, and that the next analysis is as if it had not happened.
//!
//! The cancellations are made from inside the job, through the observer the server gives the
//! workspace (it hears every line of an analysis) or the `say` a job reports to, so that each
//! test cancels at a known place and none depends on a timer.

use std::{
    fs,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use netsmith::editor::Workspace;
use netsmith::project::Target;
use netsmith_editor::{
    AnalysisEvent, Backend, Cancel, Say,
    api::{Analysis, Severity},
};
use netsmith_pcb::FootprintLibrary;
use netsmith_testkit::TempDir;

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");

#[path = "../../netsmith-check/tests/common/stages.rs"]
mod stages;

/// What the observer is told to do, and what it heard.
#[derive(Default)]
struct Probe {
    workspace: Mutex<Weak<Workspace>>,
    /// Cancel the analysis at the first progress line of this stage.
    cancel_at: Mutex<Option<String>>,
    cancelled_at: Mutex<Vec<Instant>>,
    events: Mutex<Vec<String>>,
    /// The counts of work the solver reported: `unit done/total`.
    work: Mutex<Vec<String>>,
    /// Cancel the analysis at the first count of `parts chosen` that reaches this many: the
    /// search is under way.
    cancel_at_chosen: Mutex<Option<u64>>,
    /// The counts of `parts chosen` the solver reported after that cancel, and the one it was
    /// asked at.
    chosen_after_cancel: Mutex<Vec<u64>>,
    chosen_at_cancel: Mutex<Option<u64>>,
}

impl Probe {
    fn observer(self: &Arc<Self>) -> netsmith_editor::Observer {
        let probe = Arc::clone(self);
        Arc::new(move |event| {
            let name = match &event {
                AnalysisEvent::Started { .. } => "started".to_string(),
                AnalysisEvent::Progress { stage, .. } => format!("progress:{stage}"),
                AnalysisEvent::Failed { .. } => "failed".to_string(),
                AnalysisEvent::Cancelled => "cancelled".to_string(),
                AnalysisEvent::Finished => "finished".to_string(),
            };
            probe.events.lock().unwrap().push(name);
            if let AnalysisEvent::Progress { stage, work, .. } = &event {
                if let Some(w) = work {
                    if w.unit == "parts chosen" {
                        probe.hear_chosen(u64::from(w.done));
                    }
                    probe.work.lock().unwrap().push(format!(
                        "{} {}/{}",
                        w.unit,
                        w.done,
                        w.total.map_or("?".to_string(), |t| t.to_string())
                    ));
                }
                let mut at = probe.cancel_at.lock().unwrap();
                if at.as_deref() == Some(stage.as_str()) {
                    *at = None;
                    probe.cancelled_at.lock().unwrap().push(Instant::now());
                    let workspace = probe.workspace.lock().unwrap().upgrade();
                    let said = workspace.expect("the workspace is there").cancel_analysis();
                    assert!(said, "an analysis is running, and says so");
                }
            }
        })
    }

    /// A count of parts chosen: the cancel is asked at the first that reaches the count the test
    /// set, and every count after it is kept.
    fn hear_chosen(&self, done: u64) {
        let mut at = self.cancel_at_chosen.lock().unwrap();
        if let Some(wanted) = *at
            && done >= wanted
        {
            *at = None;
            *self.chosen_at_cancel.lock().unwrap() = Some(done);
            let workspace = self.workspace.lock().unwrap().upgrade();
            let said = workspace.expect("the workspace is there").cancel_analysis();
            assert!(said, "an analysis is running, and says so");
        } else if self.chosen_at_cancel.lock().unwrap().is_some() {
            self.chosen_after_cancel.lock().unwrap().push(done);
        }
    }

    fn cancel_at(&self, stage: &str) {
        *self.cancel_at.lock().unwrap() = Some(stage.to_string());
    }

    fn heard(&self) -> Vec<String> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }
}

fn open(root: &TempDir, probe: &Arc<Probe>) -> Arc<Workspace> {
    let editor = Arc::new(
        Workspace::open_deferred(Target::resolve_from(root, None).unwrap())
            .unwrap_or_else(|e| panic!("{e}")),
    );
    *probe.workspace.lock().unwrap() = Arc::downgrade(&editor);
    editor.observe(probe.observer());
    editor
}

/// A project of `two_boards.nsm`, opened without its first analysis.
fn project(name: &str) -> (TempDir, Arc<Workspace>, Arc<Probe>) {
    let root = TempDir::new(&format!("editor-cancel-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), TWO_BOARDS).unwrap();
    let probe = Arc::new(Probe::default());
    let editor = open(&root, &probe);
    (root, editor, probe)
}

/// Changes the file so that its stamp (time and length) is not the one read, and so does its program's place: a plain
/// comment that leaves every token where it was is no change to the analysis, but each added line moves the end of the file.
fn edit(root: &TempDir, n: usize) {
    fs::write(
        root.join("src/main.nsm"),
        format!("{TWO_BOARDS}\n{}", "// edit\n".repeat(n)),
    )
    .unwrap();
}

/// Everything the page can read of the reading, as text.
fn reading(editor: &Workspace) -> (String, String, String, String) {
    let analysis = editor.analysis();
    (
        serde_json::to_string(&analysis).unwrap(),
        editor.schematic("TwoBoards/led").unwrap(),
        serde_json::to_string(&editor.board("TwoBoards/led").unwrap()).unwrap(),
        serde_json::to_string(&editor.power("TwoBoards/power").unwrap()).unwrap(),
    )
}

#[test]
fn a_cancelled_analysis_publishes_nothing() {
    let (root, editor, probe) = project("nothing");
    let first = editor.analysis();
    assert_eq!((first.version, first.errors, first.cancelled), (1, 0, None));
    assert_eq!(first.boards.len(), 2);
    let before = reading(&editor);
    probe.heard();

    // A file changes, and the analysis of it is cancelled while it solves.
    edit(&root, 1);
    probe.cancel_at("solve");
    let after = editor.analysis();
    assert_eq!(
        probe.heard(),
        [
            "started",
            "progress:check",
            "progress:solve",
            "cancelled",
            "finished"
        ],
        "no `failed`: a cancellation is not a panic"
    );
    // The reading is the one before, to the byte; only the note that it was cancelled is new.
    assert_eq!(after.version, 1, "the version did not move");
    assert_eq!(after.cancelled, Some(true));
    assert_eq!(
        Analysis {
            cancelled: None,
            stale: None,
            ..after.clone()
        },
        first
    );
    let now = reading(&editor);
    let strip = |s: &str| s.replace(",\"cancelled\":true", "");
    assert_eq!(strip(&now.0), strip(&before.0));
    assert_eq!(&now.1, &before.1, "the drawing");
    assert_eq!(&now.2, &before.2, "the board");
    assert_eq!(&now.3, &before.3, "the power tree");
    assert_eq!(editor.pending(), None);

    // The same files are not analysed again by the watcher's next look (or any request's).
    for _ in 0..3 {
        assert_eq!(editor.analysis().version, 1);
    }
    assert_eq!(probe.heard(), Vec::<String>::new(), "nothing began again");

    // The next change to a file analyses, and the reading is of it.
    edit(&root, 2);
    let next = editor.analysis();
    assert_eq!(next.version, 2);
    assert_eq!((next.errors, next.cancelled), (0, None));
    assert_eq!(probe.heard().last().map(String::as_str), Some("finished"));
}

#[test]
fn run_again_analyses_the_files_a_cancelled_analysis_left() {
    let (root, editor, probe) = project("again");
    editor.analysis();
    edit(&root, 1);
    probe.cancel_at("check");
    assert_eq!(editor.analysis().cancelled, Some(true));
    probe.heard();
    // Nothing is running, so there is nothing to cancel.
    assert!(!editor.cancel_analysis());
    let ran = editor.run_analysis();
    assert_eq!((ran.version, ran.cancelled), (2, None));
    assert!(probe.heard().contains(&"progress:solve".to_string()));
    // And with nothing cancelled, "run again" is the plain analysis: files that were analysed
    // are not analysed again.
    probe.heard();
    assert_eq!(editor.run_analysis().version, 2);
    assert_eq!(probe.heard(), Vec::<String>::new());
}

#[test]
fn a_cancelled_first_analysis_leaves_the_server_ready() {
    let (root, editor, probe) = project("first");
    assert!(editor.pending().is_some(), "not analysed yet");
    probe.cancel_at("solve");
    let analysis = editor.analysis();
    // Not pending for ever: ready, with no boards and one note that says why and what to do.
    assert_eq!(editor.pending(), None);
    assert_eq!(analysis.version, 1);
    assert_eq!(analysis.cancelled, Some(true));
    assert!(analysis.boards.is_empty());
    assert_eq!(
        (analysis.errors, analysis.warnings, analysis.notes),
        (0, 0, 1)
    );
    let note = &analysis.problems[0];
    assert_eq!(note.severity, Severity::Note);
    assert!(
        note.message.starts_with("The first analysis was cancelled")
            && note.message.contains("Run it again"),
        "{}",
        note.message
    );
    assert_eq!(
        probe.heard(),
        [
            "started",
            "progress:check",
            "progress:solve",
            "cancelled",
            "finished"
        ]
    );
    // The views answer: there is no board, which is a plain 404.
    assert_eq!(editor.schematic("TwoBoards/led").unwrap_err().status, 404);
    // It is not analysed again behind the page's back.
    assert_eq!(editor.analysis().version, 1);
    assert_eq!(probe.heard(), Vec::<String>::new());
    // A change to a file, or "run again", analyses.
    let ran = editor.run_analysis();
    assert_eq!((ran.version, ran.cancelled, ran.boards.len()), (2, None, 2));
    edit(&root, 3);
    assert_eq!(editor.analysis().version, 3);
}

#[test]
fn an_analysis_in_a_state_nobody_asked_to_cancel_is_not_stopped_by_another_cancel() {
    let (_root, editor, probe) = project("idle");
    // Nothing runs: the answer says so, and the analysis that follows is whole.
    assert!(!editor.cancel_analysis());
    assert_eq!(editor.analysis().version, 1);
    assert!(!probe.heard().contains(&"cancelled".to_string()));
}

/// The size the cancel test uses: a dev build solves it in a few seconds, so there are many
/// parts left to choose when the cancel is asked. (With the solver crates optimised in the dev
/// profile, 16 stages solve in well under half a second, too fast to be cancelled.)
const STAGES: usize = 64;

/// A cancel in the middle of the search is heard at the next trial: the solver goes on to no
/// part that it would not have chosen anyway, and ends cancelled.
///
/// This counts work and times nothing, so that a busy runner cannot fail it: the cancel is asked
/// from inside the search, at a count of parts chosen (the same place whatever the load), and
/// what is checked is how many parts the solver reports having chosen after it. Every report is
/// made by a trial that has passed a checkpoint, so a solver that hears the cancel at its next
/// checkpoint reports none (the one allowed is the end of the group it was in, which is reported
/// without a trial); one that does not hear it goes on to the end and reports the rest, up to
/// the total. A slow runner only spaces the reports out, and the progress lines are throttled (one
/// in a tenth of a second), so they can only be fewer, never more, than the work done after the
/// cancel. For the same reason this sees a cancel that goes on for more than a tenth of a second
/// or so, not one that is a few trials late: the exact count, which no line is dropped from, is
/// `a_solve_is_stopped_while_it_searches_before_it_chooses_another_part` in `netsmith-solve`.
#[test]
fn a_cancel_during_a_long_solve_is_heard_before_the_solver_chooses_another_part() {
    let root = TempDir::new("editor-cancel-latency");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"big\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), stages::source(STAGES)).unwrap();
    let probe = Arc::new(Probe::default());
    let editor = open(&root, &probe);
    // The tenth part chosen: well into the search, with most of the parts still to choose.
    *probe.cancel_at_chosen.lock().unwrap() = Some(10);
    let analysis = editor.analysis();
    let asked_at = probe
        .chosen_at_cancel
        .lock()
        .unwrap()
        .expect("the search got to its tenth part");
    let after = probe.chosen_after_cancel.lock().unwrap().clone();
    let total: u64 = probe
        .work
        .lock()
        .unwrap()
        .iter()
        .find(|w| w.starts_with("parts chosen 0/"))
        .and_then(|w| w.split('/').nth(1)?.parse().ok())
        .expect("the solver said how many parts it chooses");
    eprintln!("cancelled at {asked_at} of {total} parts chosen; reported after it: {after:?}");
    assert_eq!(
        analysis.cancelled,
        Some(true),
        "the solve ended before it was asked to"
    );
    assert!(
        asked_at * 4 < total,
        "asked at {asked_at} of {total}: the search was to be far from its end"
    );
    assert!(
        after.len() <= 1 && after.iter().all(|&done| done <= asked_at + 1),
        "the solver went on to choose parts after it was asked to stop: {after:?} (asked at {asked_at} of {total})"
    );
}

#[test]
fn the_solver_reports_the_parts_it_has_chosen_with_the_total_and_at_most_ten_lines_a_second() {
    let root = TempDir::new("editor-cancel-work");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"w\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), stages::source(3)).unwrap();
    let probe = Arc::new(Probe::default());
    let editor = open(&root, &probe);
    let started = Instant::now();
    let analysis = editor.analysis();
    let took = started.elapsed();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let work = probe.work.lock().unwrap().clone();
    let parts: Vec<&String> = work
        .iter()
        .filter(|w| w.starts_with("parts chosen"))
        .collect();
    assert!(parts.len() >= 2, "{work:?}");
    assert!(parts[0].starts_with("parts chosen 0/"), "{parts:?}");
    let total = parts[0].split('/').nth(1).unwrap();
    assert!(
        parts.iter().all(|w| w.ends_with(&format!("/{total}"))),
        "{parts:?}"
    );
    assert!(
        parts
            .last()
            .unwrap()
            .starts_with(&format!("parts chosen {total}/")),
        "it ends at the total: {parts:?}"
    );
    // Throttled: the first and the last of a step always, the rest one in a tenth of a second.
    let allowed = 4 + (took.as_millis() / 100) as usize;
    assert!(
        parts.len() <= allowed,
        "{} lines in {took:?} (at most {allowed}): {parts:?}",
        parts.len()
    );
}

// ---- build and fab --------------------------------------------------------------------------

/// Every file under `dir` (none if it is not there).
fn files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// A `say` that cancels the job at its first line and counts its lines.
fn cancelling(token: &Cancel) -> (Say, Arc<Mutex<usize>>) {
    let lines = Arc::new(Mutex::new(0usize));
    let say: Say = {
        let (token, lines) = (token.clone(), Arc::clone(&lines));
        Arc::new(move |_: &str| {
            *lines.lock().unwrap() += 1;
            token.cancel();
        })
    };
    (say, lines)
}

#[test]
fn build_cancels_before_it_writes_a_schematic_and_a_later_build_is_whole() {
    if netsmith_kicad::install::skip_without_kicad_libraries() {
        return;
    }
    let (root, editor, _probe) = project("build");
    editor.analysis();
    let token = Cancel::new();
    let (say, lines) = cancelling(&token);
    // The first thing a build says is that it is reading the project; it ends in the checks.
    let error = editor.build(&say, &token).expect_err("cancelled");
    assert!(error.is_cancelled(), "{error:?}");
    assert_eq!(error.status, 499);
    assert!(*lines.lock().unwrap() >= 1);
    assert!(!root.join("out").exists(), "nothing was written");
    // The editor goes on: the same build, not cancelled, writes both schematics.
    let quiet: Say = Arc::new(|_| {});
    let report = editor.build(&quiet, &Cancel::new()).unwrap();
    assert!(report.ok, "{}", report.output);
    assert_eq!(report.written.len(), 2);
}

#[test]
fn fab_cancels_before_it_writes_a_package_and_keeps_nothing() {
    if !FootprintLibrary::installed().root().exists() {
        return;
    }
    let (root, editor, _probe) = project("fab");
    editor.analysis();
    let token = Cancel::new();
    let routed = Arc::new(AtomicBool::new(false));
    // Cancelled at the first stage `fab` tells (it is placing): the placer or the router is
    // what stops.
    let say: Say = {
        let (token, routed) = (token.clone(), Arc::clone(&routed));
        Arc::new(move |line: &str| {
            routed.store(true, Ordering::SeqCst);
            if line.contains("placing") {
                token.cancel();
            }
        })
    };
    let error = editor
        .fab("TwoBoards/led", false, &say, &token)
        .expect_err("cancelled");
    assert!(error.is_cancelled(), "{error:?}");
    assert!(routed.load(Ordering::SeqCst), "it had begun");
    assert_eq!(
        files_under(&root.join("fab")),
        Vec::<std::path::PathBuf>::new(),
        "no file of the board it was on"
    );
    // Nothing is offered for download.
    assert_eq!(
        editor.download("TwoBoards/led", "bom").unwrap_err().status,
        404
    );
    // The same fab, not cancelled, makes the package.
    let quiet: Say = Arc::new(|_| {});
    let report = editor
        .fab("TwoBoards/led", false, &quiet, &Cancel::new())
        .unwrap();
    assert!(report.ok, "{}", report.output);
    assert!(editor.download("TwoBoards/led", "bom").is_ok());
}

#[test]
fn a_token_cancelled_before_a_job_starts_ends_it_before_it_does_anything() {
    let (root, editor, _probe) = project("early");
    editor.analysis();
    let token = Cancel::new();
    token.cancel();
    let (say, lines) = cancelling(&Cancel::new());
    for error in [
        editor.build(&say, &token).expect_err("build"),
        editor
            .fab("TwoBoards/led", false, &say, &token)
            .expect_err("fab"),
    ] {
        assert!(error.is_cancelled(), "{error:?}");
    }
    assert!(!root.join("out").exists() && !root.join("fab").exists());
    // What the job says may be the one line that it began.
    assert!(*lines.lock().unwrap() <= 2);
}

#[test]
fn with_no_analysis_on_change_a_file_is_read_when_the_page_asks() {
    let (root, editor, probe) = project("manual");
    assert_eq!(editor.analysis().version, 1);
    editor.set_auto_analysis(false);
    probe.heard();
    edit(&root, 1);
    // The change is not analysed, however often the watcher asks; the reading says it is behind.
    for _ in 0..3 {
        let a = editor.analysis();
        assert_eq!((a.version, a.stale), (1, Some(true)));
    }
    assert_eq!(probe.heard(), Vec::<String>::new());
    // The page asks: the file is read, and the reading is whole again.
    let ran = editor.run_analysis();
    assert_eq!((ran.version, ran.stale), (2, None));
    assert!(probe.heard().contains(&"progress:solve".to_string()));
    // With nothing changed, asking again does nothing.
    assert_eq!(editor.run_analysis().version, 2);
    // Switched back on, a change is analysed at once.
    editor.set_auto_analysis(true);
    // (A change of the program: one more line of the same comment would be no change at all.)
    fs::write(
        root.join("src/main.nsm"),
        format!("{TWO_BOARDS}\nboard Another {{\n}}\n"),
    )
    .unwrap();
    assert_eq!(
        (editor.analysis().version, editor.analysis().stale),
        (3, None)
    );
}

#[test]
fn asking_for_an_analysis_with_nothing_changed_does_not_run_the_next_change() {
    let (root, editor, probe) = project("spent");
    editor.analysis();
    editor.set_auto_analysis(false);
    assert_eq!(editor.run_analysis().version, 1);
    probe.heard();
    edit(&root, 1);
    assert_eq!(editor.analysis().stale, Some(true));
    assert_eq!(probe.heard(), Vec::<String>::new());
}

/// Writes `text` to the project's file and returns the reading's version after the server has looked.
fn write_and_read(root: &TempDir, editor: &Workspace, text: &str) -> u64 {
    fs::write(root.join("src/main.nsm"), text).unwrap();
    editor.analysis().version
}

#[test]
fn a_change_to_a_plain_comment_is_not_analysed_but_a_change_to_the_program_is() {
    let (root, editor, probe) = project("comment");
    assert_eq!(editor.analysis().version, 1);
    probe.heard();
    // A plain comment at the end of an existing line, longer and shorter: the reading stays.
    let with_comment = TWO_BOARDS.replacen(
        "import std::sources::Rail\n",
        "import std::sources::Rail // a note\n",
        1,
    );
    assert_ne!(with_comment, TWO_BOARDS);
    assert_eq!(write_and_read(&root, &editor, &with_comment), 1);
    let longer = with_comment.replace("// a note", "// a much longer note than before");
    assert_eq!(write_and_read(&root, &editor, &longer), 1);
    assert_eq!(probe.heard(), Vec::<String>::new(), "no analysis ran");
    // The status of the reading is as it was: not stale, not cancelled.
    let a = editor.analysis();
    assert_eq!((a.stale, a.cancelled), (None, None));
    // A line added moves everything below it, so that is analysed.
    let added = format!("// a new line\n{TWO_BOARDS}");
    assert_eq!(write_and_read(&root, &editor, &added), 2);
    assert!(probe.heard().contains(&"progress:solve".to_string()));
    // A doc comment is part of the design (it is a note on the schematic): analysed.
    let doc = format!("//! a doc\n{TWO_BOARDS}");
    assert_eq!(write_and_read(&root, &editor, &doc), 3);
    let doc2 = format!("//! another doc\n{TWO_BOARDS}");
    assert_eq!(write_and_read(&root, &editor, &doc2), 4);
    // So is a change to the program.
    let code = format!("{TWO_BOARDS}\nboard Extra {{\n}}\n");
    assert_eq!(write_and_read(&root, &editor, &code), 5);
}

#[test]
fn a_check_of_typed_text_that_differs_only_in_a_comment_is_the_reading_of_the_files() {
    let (_root, editor, probe) = project("typed");
    let read = editor.analysis();
    probe.heard();
    let typed = TWO_BOARDS.replacen(
        "import std::sources::Rail\n",
        "import std::sources::Rail // typed in the editor\n",
        1,
    );
    let checked = editor.check("src/main.nsm", &typed).unwrap();
    assert_eq!(
        (checked.errors, checked.version),
        (read.errors, read.version)
    );
    assert_eq!(probe.heard(), Vec::<String>::new());
    // A typed mistake is a check as ever.
    let wrong = TWO_BOARDS.replace("LedBoard", "LedBoard2 {} board Wrong {} // ");
    let checked = editor.check("src/main.nsm", &wrong);
    assert!(checked.is_ok());
}
