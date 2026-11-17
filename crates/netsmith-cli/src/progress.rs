//! Where the progress of `fab` and `dev board3d` goes (issue #764): stderr as text for a person or as
//! JSON lines for a program (`--progress`), and, with `--to-editor`, a running editor too, whose
//! page shows a run started in a terminal in its status bar (`POST /api/progress`).
//!
//! The commands tell their progress through plain functions (`fn(&str)`), so what they are told
//! to do with it is kept here for the process: [`setup`] once, before the run.

use std::{
    io::Write,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use netsmith_editor::api::OutsideProgress;
use netsmith_fabricate::layout::StageEvent;

/// How the progress is printed on stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Print {
    /// A line for a person: `Board: placing: run 1 in 12.0 x 16.0 mm: ... [6 s]`.
    Text,
    /// One JSON object per line: `{"board","stage","unit","done","total","detail","elapsedMs",
    /// "message"}` for a line of the placer or the router, `{"message"}` for any other.
    Json,
    /// Nothing (`--quiet`, or `--format json` with text progress).
    #[value(skip)]
    Nothing,
}

struct Setup {
    print: Print,
    editor: Option<Editor>,
}

static SETUP: OnceLock<Setup> = OnceLock::new();

/// How long the editor is waited for: it is on this machine, and a run must not stall on it.
const WAIT: Duration = Duration::from_secs(2);

/// A running editor the progress is posted to.
struct Editor {
    port: u16,
    agent: ureq::Agent,
    /// The run, as the editor tells it from others: `fab 4711`.
    run: String,
    command: &'static str,
    /// Whether it stopped answering: it is then asked no more, and that was said once.
    gone: Mutex<bool>,
}

/// Says how the progress of this process is told: printed as `print`, and posted to the editor
/// on `editor`'s port, if one is given, as the run of `command`. The first call wins.
pub fn setup(print: Print, editor: Option<u16>, command: &'static str) {
    let editor = editor.map(|port| Editor {
        port,
        agent: ureq::Agent::config_builder()
            .user_agent("netsmith-progress")
            .timeout_global(Some(WAIT))
            .http_status_as_error(false)
            .proxy(None)
            .build()
            .into(),
        run: format!("{command} {}", std::process::id()),
        command,
        gone: Mutex::new(false),
    });
    let _ = SETUP.set(Setup { print, editor });
}

fn current() -> Option<&'static Setup> {
    SETUP.get()
}

/// Whether the stage lines are to be made into [`StageEvent`]s: printed as JSON or posted.
pub fn wants_events() -> bool {
    current().is_some_and(|s| s.print == Print::Json || s.editor.is_some())
}

/// A line of progress that is not the placer's or the router's (`Board: placing 12 parts`).
pub fn tell(line: &str) {
    let Some(setup) = current() else {
        eprintln!("{line}");
        return;
    };
    match setup.print {
        Print::Text => eprintln!("{line}"),
        Print::Json => print_json(&serde_json::json!({ "message": line })),
        Print::Nothing => {}
    }
    if let Some(editor) = &setup.editor {
        editor.post(OutsideProgress {
            message: line.to_string(),
            ..editor.blank(String::new())
        });
    }
}

/// Posts `line` to the editor only, for a line the command prints in its own way.
pub fn post_only(line: &str) {
    if let Some(editor) = current().and_then(|s| s.editor.as_ref()) {
        editor.post(OutsideProgress {
            message: line.to_string(),
            ..editor.blank(String::new())
        });
    }
}

/// A line of the placer or the router.
pub fn event(e: &StageEvent) {
    let Some(setup) = current() else {
        eprintln!("{}", e.message);
        return;
    };
    match setup.print {
        Print::Text => eprintln!("{}", e.message),
        Print::Json => print_json(&serde_json::to_value(e).unwrap_or_default()),
        Print::Nothing => {}
    }
    if let Some(editor) = &setup.editor {
        editor.post(OutsideProgress {
            message: e.message.clone(),
            unit: Some(e.unit.to_string()),
            done: Some(e.done),
            total: e.total,
            ..editor.blank(e.board.clone())
        });
    }
}

/// The run has ended, `ok` or not, with `message`: the editor stops showing it as running.
pub fn ended(ok: bool, message: &str) {
    if let Some(editor) = current().and_then(|s| s.editor.as_ref()) {
        editor.post(OutsideProgress {
            message: message.to_string(),
            ended: true,
            ok: Some(ok),
            ..editor.blank(String::new())
        });
    }
}

fn print_json(value: &serde_json::Value) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{value}");
}

impl Editor {
    fn blank(&self, board: String) -> OutsideProgress {
        OutsideProgress {
            run: self.run.clone(),
            command: self.command.to_string(),
            board,
            message: String::new(),
            unit: None,
            done: None,
            total: None,
            ended: false,
            ok: None,
        }
    }

    /// Posts one line. An editor that does not take it is said once on stderr and asked no
    /// more: the run goes on.
    fn post(&self, progress: OutsideProgress) {
        let mut gone = self.gone.lock().unwrap_or_else(|e| e.into_inner());
        if *gone {
            return;
        }
        let body = serde_json::to_string(&progress).unwrap_or_default();
        let url = format!("http://127.0.0.1:{}/api/progress", self.port);
        let answer = self
            .agent
            .post(&url)
            .header("X-Netsmith", "1")
            .header("Content-Type", "application/json")
            .send(&body);
        let refused = match answer {
            Ok(r) if r.status().is_success() => None,
            Ok(r) => Some(format!("it answered {}", r.status().as_u16())),
            Err(e) => Some(e.to_string()),
        };
        if let Some(why) = refused {
            *gone = true;
            eprintln!(
                "note: --to-editor: the editor on 127.0.0.1:{} did not take the progress ({why}); it is not sent there any more",
                self.port
            );
        }
    }
}
