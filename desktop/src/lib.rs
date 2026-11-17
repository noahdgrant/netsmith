//! The desktop shell's launcher: everything the app does that needs no window.
//!
//! The shell is a window around the editor's page (issue #799). It starts `netsmith view editor` in the
//! project folder it was opened with, asks it for a free port (`--port 0`: the system picks one,
//! so two windows never collide and nothing races between a probe and the bind), reads the
//! address the editor prints once it listens, and points the window there. Closing the window
//! stops the server. None of this needs a display, so it lives here and its tests run on a
//! machine without WebKit or GTK; the window itself is `main.rs`, behind the `window` feature.

use std::ffi::OsString;
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// The variable that names the `netsmith` program the shell starts.
pub const PROGRAM_VARIABLE: &str = "NETSMITH_BIN";

/// The project folder the shell edits: the first argument after the program's own name, taken
/// against `cwd` when it is relative, or `cwd` itself when there is none. The editor finds the
/// project's `netsmith.toml` from there as it does from a terminal.
pub fn project_folder(
    args: impl IntoIterator<Item = OsString>,
    cwd: &Path,
) -> Result<PathBuf, LaunchError> {
    let mut args = args.into_iter().skip(1);
    let folder = match args.next() {
        Some(arg) => cwd.join(arg),
        None => cwd.to_path_buf(),
    };
    if let Some(extra) = args.next() {
        return Err(LaunchError::Usage(format!(
            "one project folder at most, and {} is a second (usage: netsmith-desktop [FOLDER])",
            Path::new(&extra).display()
        )));
    }
    if !folder.is_dir() {
        return Err(LaunchError::Usage(format!(
            "{} is not a folder (usage: netsmith-desktop [FOLDER])",
            folder.display()
        )));
    }
    Ok(folder.canonicalize().unwrap_or(folder))
}

/// The `netsmith` program to start: the one `NETSMITH_BIN` names (`variable`), else a `netsmith`
/// beside the shell's own executable (`beside`, the folder it is in), else `netsmith`, which
/// the system looks up on `PATH`.
pub fn netsmith_program(variable: Option<OsString>, beside: Option<&Path>) -> PathBuf {
    if let Some(program) = variable.filter(|v| !v.is_empty()) {
        return PathBuf::from(program);
    }
    let name = format!("netsmith{}", std::env::consts::EXE_SUFFIX);
    if let Some(sibling) = beside.map(|dir| dir.join(&name))
        && sibling.is_file()
    {
        return sibling;
    }
    PathBuf::from(name)
}

/// The command that serves the editor for `folder`: `netsmith view editor --port 0`, run in the folder.
pub fn editor_command(program: &Path, folder: &Path) -> Command {
    let mut command = Command::new(program);
    command.args(["view", "editor", "--port", "0"]).current_dir(folder);
    command
}

/// The address in the line the editor prints once it listens
/// (`editing <title> at http://127.0.0.1:<port>/ (Ctrl-C to stop)`), or `None` for any other line.
pub fn ready_address(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("editing ")?;
    let start = rest.rfind(" at http://")? + " at ".len();
    let address = rest[start..].split_whitespace().next()?;
    let port = address
        .strip_prefix("http://127.0.0.1:")?
        .strip_suffix('/')?;
    port.parse::<u16>().ok()?;
    Some(address)
}

/// A running `netsmith view editor`, stopped when this is dropped.
#[derive(Debug)]
pub struct Server {
    child: Child,
    address: String,
    output: Vec<JoinHandle<()>>,
}

impl Server {
    /// Starts `command` and waits until it prints the address it serves. The server's own output
    /// goes on to this process's stdout and stderr. Fails when the program cannot be started, or
    /// when it stops before it is ready (the error carries what it wrote on stderr).
    pub fn start(mut command: Command) -> Result<Server, LaunchError> {
        let program = PathBuf::from(command.get_program());
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| LaunchError::Spawn {
                program: program.clone(),
                reason: e.to_string(),
            })?;
        let stderr = child.stderr.take().expect("stderr is piped");
        let said = Arc::new(Mutex::new(String::new()));
        let errors = {
            let said = Arc::clone(&said);
            thread::spawn(move || copy_lines(stderr, &mut std::io::stderr(), Some(&said)))
        };
        let mut stdout = BufReader::new(child.stdout.take().expect("stdout is piped"));
        let mut line = String::new();
        loop {
            line.clear();
            match stdout.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            print!("{line}");
            if let Some(address) = ready_address(line.trim_end()) {
                let address = address.to_string();
                let rest = thread::spawn(move || copy_lines(stdout, &mut std::io::stdout(), None));
                return Ok(Server {
                    child,
                    address,
                    output: vec![errors, rest],
                });
            }
        }
        // The output ended before the address: the server stopped (or closed its stdout).
        let _ = child.kill();
        let status = child.wait().ok();
        let _ = errors.join();
        let said = said
            .lock()
            .map(|s| s.trim_end().to_string())
            .unwrap_or_default();
        Err(LaunchError::Stopped {
            program,
            status,
            said,
        })
    }

    /// The address the editor serves, as it printed it (`http://127.0.0.1:<port>/`).
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The server's process id.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Stops the server and waits for it to end; returns how it ended.
    pub fn stop(mut self) -> std::io::Result<ExitStatus> {
        self.end()
    }

    fn end(&mut self) -> std::io::Result<ExitStatus> {
        // The editor keeps nothing that a stop could lose: an edit is written when it is made,
        // and the scratch folder of a comparison is swept by the next start.
        if self.child.try_wait()?.is_none() {
            let _ = self.child.kill();
        }
        let status = self.child.wait();
        for copier in self.output.drain(..) {
            let _ = copier.join();
        }
        status
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.end();
    }
}

/// Copies `from` to `to` line by line until it ends, and keeps a copy in `keep` when given.
fn copy_lines(from: impl Read, to: &mut impl Write, keep: Option<&Mutex<String>>) {
    let mut from = BufReader::new(from);
    let mut line = String::new();
    loop {
        line.clear();
        match from.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let _ = to.write_all(line.as_bytes());
        if let Some(keep) = keep
            && let Ok(mut kept) = keep.lock()
        {
            kept.push_str(&line);
        }
    }
}

/// A script that hands `problem` to the shell's own page (`loading/index.html`), which shows it
/// in place of the editor: `window.__netsmithProblem = "<problem>";`, with the text quoted for
/// JavaScript.
pub fn problem_script(problem: &str) -> String {
    let mut quoted = String::with_capacity(problem.len() + 2);
    quoted.push('"');
    for c in problem.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            // `<` keeps `</script>` out of the text; the two separators end a line in JavaScript.
            '<' | '\u{2028}' | '\u{2029}' => quoted.push_str(&format!("\\u{:04x}", c as u32)),
            c if c.is_control() => quoted.push_str(&format!("\\u{:04x}", c as u32)),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    format!("window.__netsmithProblem = {quoted};")
}

/// Why the shell could not show the editor.
#[derive(Debug)]
pub enum LaunchError {
    /// The arguments name no folder the shell can open.
    Usage(String),
    /// The `netsmith` program could not be started.
    Spawn { program: PathBuf, reason: String },
    /// `netsmith view editor` stopped before it printed its address.
    Stopped {
        program: PathBuf,
        status: Option<ExitStatus>,
        said: String,
    },
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaunchError::Usage(message) => f.write_str(message),
            LaunchError::Spawn { program, reason } => write!(
                f,
                "cannot start {}: {reason} (install netsmith, or name the program with {PROGRAM_VARIABLE})",
                program.display()
            ),
            LaunchError::Stopped {
                program,
                status,
                said,
            } => {
                write!(
                    f,
                    "`{} editor` stopped before it was ready",
                    program.display()
                )?;
                if let Some(status) = status {
                    write!(f, " ({status})")?;
                }
                if said.is_empty() {
                    Ok(())
                } else {
                    write!(f, ":\n{said}")
                }
            }
        }
    }
}

impl std::error::Error for LaunchError {}
