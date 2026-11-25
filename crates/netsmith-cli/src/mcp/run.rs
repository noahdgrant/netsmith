//! A tool call runs its command as a child process of this same `netsmith` binary, in the
//! server's folder, as a person runs it in a terminal (issue #1106). The command's stdout is the
//! result, its stderr lines are its progress (ADR 0062: long work says how far it has got), and
//! its exit code says whether the call was an error. A child process, not a call in this one:
//! the commands print to stdout, which here is the protocol's channel, and keep their setup for
//! the process (`crate::progress`), so a second command in the same process would be told the
//! first one's. A cancel ends the child process, as Ctrl+C does in a terminal: a job inside the
//! editor is asked to end at its next checkpoint (ADR 0062), but the command line has no cancel
//! of its own but Ctrl+C, so what a cancelled call leaves is what an interrupted command leaves.
//!
//! A tool call that is to act on a running editor's page (issue #1108) is this same call with the
//! editor's port in its arguments ([`super::attach`]); a command that would run in the editor's
//! own process would be another way of running it, chosen in [`super::Server::execute`], and
//! [`run`] is the one this server has.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// One running command, which a cancel stops.
#[derive(Default)]
pub struct Job {
    cancelled: AtomicBool,
    child: Mutex<Option<Child>>,
}

impl Job {
    /// Stops the command: its process is killed, now if it has started, or as it starts.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(child) = self
            .child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            let _ = child.kill();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// What a command did.
#[derive(Debug)]
pub struct Ran {
    /// Its exit code; `None` when it was killed (a cancel) or ended by a signal.
    pub code: Option<i32>,
    pub stdout: String,
    /// The lines of stderr that are not progress lines of `--progress json`.
    pub stderr: String,
}

/// Runs `program` with `argv` in `dir` for `job`, telling `progress` each line it prints on
/// stderr: the message of a JSON line of `--progress json`, or the line as it is. `input` is
/// what the command reads on stdin (a document given to the call itself, issue #1472), or
/// nothing, and the variable `env` set when there is one. Returns when the command has ended,
/// also when it was cancelled.
pub fn run(
    program: &Path,
    argv: &[String],
    input: Option<&str>,
    dir: &Path,
    env: Option<(&str, &str)>,
    job: &Job,
    mut progress: impl FnMut(&str),
) -> std::io::Result<Ran> {
    let mut child = Command::new(program)
        .args(argv)
        .current_dir(dir)
        .envs(env)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Written from a thread of its own, so a document larger than the pipe holds does not wait
    // on a command that is itself waiting to be read.
    let writer = match (input, child.stdin.take()) {
        (Some(text), Some(mut stdin)) => {
            let text = text.to_string();
            Some(std::thread::spawn(move || {
                let _ = std::io::Write::write_all(&mut stdin, text.as_bytes());
            }))
        }
        _ => None,
    };
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    {
        let mut slot = job.child.lock().unwrap_or_else(|e| e.into_inner());
        *slot = Some(child);
        // A cancel that came before the child was kept here has not killed it.
        if job.is_cancelled()
            && let Some(child) = slot.as_mut()
        {
            let _ = child.kill();
        }
    }
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let mut kept = String::new();
    for line in BufReader::new(stderr).split(b'\n') {
        let Ok(line) = line else { break };
        let line = String::from_utf8_lossy(&line);
        let line = line.trim_end_matches('\r');
        match progress_message(line) {
            Some(message) => progress(&message),
            None => {
                progress(line);
                kept.push_str(line);
                kept.push('\n');
            }
        }
    }
    let stdout = reader.join().unwrap_or_default();
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    let status = job
        .child
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .expect("the child is kept until it ends")
        .wait()?;
    Ok(Ran {
        code: status.code(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: kept,
    })
}

/// The message of a line of `--progress json` (`{"message": ..., ...}`), with the count of its
/// unit when it has one: `Board: placing ... (3 of 8 seeds)` is told as the line says it.
fn progress_message(line: &str) -> Option<String> {
    if !line.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    value.get("message")?.as_str().map(str::to_string)
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, mpsc};

    use super::*;

    /// A program that says one line on stderr and then waits until it is killed.
    fn waits(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("waits");
        std::fs::write(&path, "#!/bin/sh\necho started >&2\nexec sleep 100000\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn a_cancel_kills_the_command_and_the_run_ends() {
        let dir = netsmith_testkit::TempDir::new("mcp-cancel");
        let program = waits(&dir);
        let job = Arc::new(Job::default());
        let (tx, rx) = mpsc::channel();
        let running = {
            let job = job.clone();
            let dir = dir.to_path_buf();
            std::thread::spawn(move || {
                run(&program, &[], None, &dir, None, &job, |line| {
                    let _ = tx.send(line.to_string());
                })
            })
        };
        // The command has started when it says so.
        assert_eq!(rx.recv().unwrap(), "started");
        job.cancel();
        let ran = running.join().unwrap().unwrap();
        assert_eq!(ran.code, None, "{ran:?}");
        assert!(job.is_cancelled());
    }

    #[test]
    fn a_json_progress_line_is_told_by_its_message_and_not_kept() {
        assert_eq!(
            progress_message(r#"{"board":"B","message":"B: placing"}"#).as_deref(),
            Some("B: placing")
        );
        assert_eq!(progress_message("error: no file"), None);
    }
}
