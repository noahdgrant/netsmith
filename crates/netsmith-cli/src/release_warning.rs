//! The warning of a project written for another minor release than this netsmith (the manifest's
//! `netsmith`, [`netsmith_fabricate::release::mismatch`], issue #1657). It is said once in a
//! run: a command that prints findings as JSON takes it into its document ([`finding`],
//! [`add_to`]), and any other run says it on stderr when it ends ([`say`], `commands::run`),
//! after the errors it may explain. Nothing is refused.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use netsmith_fabricate::project::Project;
use netsmith_fabricate::release;
use netsmith_tools::findings::{Finding, Severity};
use serde_json::Value;

/// The environment variable `netsmith mcp` gives the commands it runs when it has said a
/// project's warning itself, as it was started: its value is [`key`], so those commands do not
/// say it again, and say it when the `netsmith` key changes or a call names a file of another
/// project.
pub const SAID: &str = "NETSMITH_RELEASE_SAID";

/// The code of the finding, the same as that of the note of a project with no pin.
pub const CODE: &str = "release";

/// The warning of this run.
enum Warning {
    /// No project written for another release has been opened.
    None,
    /// One has, and nothing has said it yet.
    Pending(String),
    /// It has been said.
    Said,
}

static WARNING: Mutex<Warning> = Mutex::new(Warning::None);

/// The value of [`SAID`] for `project`: its release and its folder. `None` when its manifest
/// names none.
pub fn key(project: &Project) -> Option<String> {
    let written_for = project.manifest.netsmith.as_deref()?;
    Some(format!("{written_for} {}", folder(&project.root)))
}

/// The folder as both ends of [`SAID`] write it: a command's current folder has its links
/// followed, and `--project` may not.
fn folder(root: &Path) -> String {
    fs::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .display()
        .to_string()
}

/// A command builds or checks `project`: its warning, if it has one, is to be said.
pub fn open(project: Option<&Project>) {
    let Some(project) = project else {
        return;
    };
    let Some(warning) = release::mismatch(project.manifest.netsmith.as_deref()) else {
        return;
    };
    if std::env::var(SAID).ok() == key(project) {
        return;
    }
    let mut state = WARNING.lock().unwrap_or_else(|e| e.into_inner());
    if matches!(*state, Warning::None) {
        *state = Warning::Pending(warning);
    }
}

/// The warning not yet said, which is said by what the caller does with it.
fn take() -> Option<String> {
    let mut state = WARNING.lock().unwrap_or_else(|e| e.into_inner());
    match std::mem::replace(&mut *state, Warning::Said) {
        Warning::Pending(warning) => Some(warning),
        other => {
            *state = other;
            None
        }
    }
}

/// Says the warning on stderr, if it has not been said.
pub fn say() {
    if let Some(warning) = take() {
        eprintln!("warning: {warning}");
    }
}

/// The warning as the finding of a `--format json` document: a note of the code [`CODE`].
pub fn finding() -> Option<Finding> {
    take().map(|warning| Finding::plain(Severity::Note, CODE, warning))
}

/// Puts the warning first among the `findings` of `document`, a `--format json` document that
/// is not a [`netsmith_tools::findings::Report`], which gets the key if it has none. A run with
/// nothing to say leaves the document as it is.
pub fn add_to(document: &mut Value) {
    let Some(finding) = finding() else {
        return;
    };
    let finding = serde_json::to_value(finding).expect("a finding is plain data");
    match document["findings"].as_array_mut() {
        Some(findings) => findings.insert(0, finding),
        None => document["findings"] = Value::Array(vec![finding]),
    }
}
