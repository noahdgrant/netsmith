//! A panic in the analysis (an `expect` in the layout, say) ends that analysis with a problem
//! and leaves the server as it was (review of night 3, finding 2).
//!
//! The panics are injected through the observer the server gives the workspace: it hears every
//! progress line of an analysis, and a test's observer that panics on one makes the analysis
//! panic where that line is sent, as a bug there would.

use std::{
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use netsmith::editor::Workspace;
use netsmith::project::Target;
use netsmith_editor::{AnalysisEvent, Backend, api::Analysis};
use netsmith_testkit::TempDir;

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");

/// What the observer is told to do, and what it heard.
#[derive(Default)]
struct Probe {
    /// Panic at the first progress line of the `check` stage.
    panic_in_check: AtomicBool,
    /// Panic at the progress line of the drawing of this board.
    panic_in_drawing_of: Mutex<Option<String>>,
    events: Mutex<Vec<String>>,
    panics: Mutex<u32>,
}

impl Probe {
    fn observer(self: &Arc<Self>) -> netsmith_editor::Observer {
        let probe = Arc::clone(self);
        Arc::new(move |event| {
            let mut name = match &event {
                AnalysisEvent::Started { .. } => "started".to_string(),
                AnalysisEvent::Progress { stage, .. } => format!("progress:{stage}"),
                AnalysisEvent::Failed { .. } => "failed".to_string(),
                AnalysisEvent::Cancelled => "cancelled".to_string(),
                AnalysisEvent::Finished => "finished".to_string(),
            };
            if let AnalysisEvent::Progress { stage, board, .. } = &event {
                let wanted = match stage.as_str() {
                    "check" => probe.panic_in_check.load(Ordering::SeqCst),
                    "draw" => probe
                        .panic_in_drawing_of
                        .lock()
                        .unwrap()
                        .as_deref()
                        .is_some_and(|b| b == board),
                    _ => false,
                };
                if wanted {
                    name.push_str(" (panics)");
                    probe.events.lock().unwrap().push(name);
                    *probe.panics.lock().unwrap() += 1;
                    panic!("injected panic in the {stage} stage");
                }
            }
            probe.events.lock().unwrap().push(name);
        })
    }

    fn panics(&self) -> u32 {
        *self.panics.lock().unwrap()
    }

    fn heard(&self) -> Vec<String> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }
}

/// A project of `two_boards.nsm`, opened without its first analysis, with the probe listening.
fn project(name: &str) -> (TempDir, Workspace, Arc<Probe>) {
    let root = TempDir::new(&format!("editor-panic-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), TWO_BOARDS).unwrap();
    let editor = Workspace::open_deferred(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    let probe = Arc::new(Probe::default());
    editor.observe(probe.observer());
    (root, editor, probe)
}

/// Changes the file so that its stamp (time and length) is not the one read.
fn edit(root: &TempDir, n: usize) {
    fs::write(
        root.join("src/main.nsm"),
        format!("{TWO_BOARDS}\n{}", "// edit\n".repeat(n)),
    )
    .unwrap();
}

fn internal(analysis: &Analysis) -> Vec<&str> {
    analysis
        .problems
        .iter()
        .filter(|p| p.message.contains("internal error"))
        .map(|p| p.message.as_str())
        .collect()
}

#[test]
fn a_panic_in_the_first_analysis_ends_it_with_a_problem_and_the_server_is_ready() {
    let (root, editor, probe) = project("first");
    assert!(editor.pending().is_some(), "not analysed yet");
    probe.panic_in_check.store(true, Ordering::SeqCst);
    let analysis = editor.analysis();
    // The page is not left waiting: ready, one error that names the panic, nothing built.
    assert_eq!(editor.pending(), None);
    assert_eq!(analysis.version, 1);
    assert_eq!(analysis.errors, 1, "{:?}", analysis.problems);
    assert!(analysis.boards.is_empty());
    let message = &internal(&analysis)[0];
    assert!(
        message.contains("injected panic in the check stage")
            && message.contains("please report it"),
        "{message}"
    );
    assert!(
        analysis.problems[0]
            .rendered
            .starts_with("error: internal error")
    );
    // The watcher is told: a failure, then the end; and the version is new.
    assert_eq!(
        probe.heard(),
        ["started", "progress:check (panics)", "failed", "finished"]
    );
    // The same files are not analysed (and panicked in) again.
    assert_eq!(editor.analysis().version, 1);
    assert_eq!(probe.panics(), 1);
    // A good edit recovers.
    probe.panic_in_check.store(false, Ordering::SeqCst);
    edit(&root, 1);
    let recovered = editor.analysis();
    assert_eq!(recovered.version, 2);
    assert_eq!(recovered.errors, 0, "{:?}", recovered.problems);
    assert!(!recovered.boards.is_empty());
    assert!(internal(&recovered).is_empty());
}

#[test]
fn a_panic_in_a_later_analysis_keeps_the_last_good_reading_and_a_later_edit_recovers() {
    let (root, editor, probe) = project("later");
    let good = editor.analysis();
    assert_eq!(good.errors, 0, "{:?}", good.problems);
    assert!(good.boards.iter().all(|b| !b.stale));
    let names: Vec<_> = good.boards.iter().map(|b| b.name.clone()).collect();
    assert!(names.len() >= 2, "{names:?}");

    probe.panic_in_check.store(true, Ordering::SeqCst);
    edit(&root, 2);
    let failed = editor.analysis();
    assert_eq!(failed.version, good.version + 1);
    assert_eq!(internal(&failed).len(), 1, "{:?}", failed.problems);
    // The boards are those of the last good reading, marked stale, and still served.
    assert_eq!(
        failed
            .boards
            .iter()
            .map(|b| b.name.clone())
            .collect::<Vec<_>>(),
        names
    );
    assert!(failed.boards.iter().all(|b| b.stale));
    assert!(editor.schematic(&names[0]).is_ok());
    assert!(editor.board(&names[0]).is_ok());
    assert_eq!(editor.pending(), None);
    probe.panic_in_check.store(false, Ordering::SeqCst);

    edit(&root, 3);
    let recovered = editor.analysis();
    assert_eq!(recovered.version, good.version + 2);
    assert_eq!(recovered.errors, 0, "{:?}", recovered.problems);
    assert!(recovered.boards.iter().all(|b| !b.stale));
    assert!(internal(&recovered).is_empty());
}

#[test]
fn a_panic_in_the_drawing_of_one_board_loses_that_drawing_and_no_other_board() {
    let (_root, editor, _probe) = project("drawing");
    let baseline = editor.analysis();
    let names: Vec<_> = baseline.boards.iter().map(|b| b.name.clone()).collect();
    assert!(names.len() >= 2, "{names:?}");
    let baseline_drawn: Vec<bool> = names.iter().map(|n| editor.schematic(n).is_ok()).collect();

    // A project of its own that panics in the drawing of the first board.
    let (_other_root, other, other_probe) = project("drawing-two");
    *other_probe.panic_in_drawing_of.lock().unwrap() = Some(names[0].clone());
    let analysis = other.analysis();
    assert_eq!(other_probe.panics(), 1);
    // Every board is still there, and the problem names the one whose drawing failed.
    assert_eq!(
        analysis
            .boards
            .iter()
            .map(|b| b.name.clone())
            .collect::<Vec<_>>(),
        names
    );
    let problems = internal(&analysis);
    assert_eq!(problems.len(), 1, "{:?}", analysis.problems);
    assert!(problems[0].contains(&names[0]) && problems[0].contains("drawing"));
    let failed = analysis
        .problems
        .iter()
        .find(|p| p.message.contains("internal error"))
        .unwrap();
    assert_eq!(failed.board.as_deref(), Some(names[0].as_str()));
    let shown = other.schematic(&names[0]).expect_err("no drawing");
    assert!(
        shown.message.contains("internal error"),
        "{}",
        shown.message
    );
    // The other boards are drawn as they are without the panic (with KiCad's symbols, drawn).
    for (n, name) in names.iter().enumerate().skip(1) {
        assert_eq!(other.schematic(name).is_ok(), baseline_drawn[n], "{name}");
    }
    // The boards that did not panic have all their other views.
    assert!(other.board(&names[0]).is_ok());
    assert_eq!(other.pending(), None);
}
