//! `netsmith board pins` (issue #1092): the pin file beside a source (`<stem>.pins.toml`, ADR
//! 0101) from the command line, as the editor writes it. `lock`, `unlock` and `move` call what
//! the editor's `POST /api/pins/lock` and `POST /api/pins/move` call (`netsmith_workspace`'s
//! pin writer), so the bytes written and the refusals are the same; `apply` takes the
//! proposal of `board fab --pins locked` through the same rules, and `show` prints what the
//! editor's pin map shows. The write changes the `lock`, `pin`, `function` and `source` lines
//! of one entry and no other line.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand};
use netsmith_check::PinFile;
use netsmith_editor::api::{Analysis, BoardSummary, PinLock, PinMove, PinRequestInfo, Severity};
use netsmith_editor::{ApiError, Backend};
use netsmith_fabricate::project::Target as Source;
use netsmith_workspace::Workspace;
use serde_json::{Value, json};

use super::common::Format;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "A board asks a part for a pin by what the pin must offer (`.status: .u.GPIO`) and the pin file beside the source keeps the answer (ADR 0101). These commands read and change that file as the editor's pin map does, with the same checks: the project is analysed first, and a choice the check would not keep is refused, with the reason and with nothing written. A write changes the `lock`, `pin`, `function` and `source` lines of one request's entry and leaves every other line, its comments and its order as they were.\n\n`show` lists the requests of the boards that have any, each with its part, function, pin and pad, whether it is locked, who chose the pin (`source`: `hand` or `auto`), where it came from (`from`: `file`, or `run` when `board build` would write it), the message of the check when it got none, and the pins it may be moved to. `lock` and `unlock` set the `lock` line of a request's entry (a request with no entry yet is written with the pin it has). `move` puts a request on another pin that offers its function and writes `source = \"hand\"`; a refused move says why and lists the pins the request may take. `apply` takes the pins of `pins.proposed.toml`, which `board fab --pins locked` writes (default: `pins.proposed.toml` in the project's fab folder), for the board's requests that it moves, under the rules of a move applied to the whole map, so a swap of two pins is not refused for the order it is written in; it moves no locked request, and it writes nothing when the proposal changes nothing.\n\nWith no `--board`, `show` lists every board that has requests, and the other commands act on the one board that has the request (or, for `apply`, the one that has requests): name it with `--board` when several do. `--format json` prints one JSON document on stdout and nothing else there: `command`, `ok`, and what was done (`boards` for `show`; `board`, `request`, `file` and the `entry` written for `lock`, `unlock` and `move`; `board`, `file` and `moved` for `apply`); a refusal has `ok: false`, an `error` with its `status`, `code` and `message`, and for a refused `move` `mayTake`, the free pins that offer the function.\n\nThe exit code is 0 when it did what it was asked, 1 when the choice was refused, and 2 when it could not run (a source that does not build, a request or board that is not there, a pin file that changed on disk, a usage mistake)."
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

/// The board a command acts on.
#[derive(Debug, PartialEq, ClapArgs)]
pub struct Target {
    /// The source file; with none, the project's boards (see netsmith.toml).
    #[arg(value_name = "FILE.NSM", long)]
    pub file: Option<String>,
    /// The board, by name, when more than one has pin requests.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Print the pin requests of the boards, each with its pin and the pins it may take.
    Show(Target),
    /// Lock the pin a request has, so that a run never moves it.
    Lock(RequestArgs),
    /// Unlock a request, so that a run may move it again.
    Unlock(RequestArgs),
    /// Put a request on another pin that offers its function; refused with the pins it may take.
    Move(MoveArgs),
    /// Take the pins of `pins.proposed.toml` (written by `board fab --pins locked`) for a board.
    Apply(ApplyArgs),
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct RequestArgs {
    /// The request, by its path in the board (`status`): the key of its entry in the pin file.
    #[arg(value_name = "REQUEST")]
    pub request: String,
    #[command(flatten)]
    pub target: Target,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct MoveArgs {
    /// The request, by its path in the board (`status`).
    #[arg(value_name = "REQUEST")]
    pub request: String,
    /// The pin to put it on, by its path in the part (`p03`), not its pad.
    #[arg(value_name = "PIN")]
    pub pin: String,
    #[command(flatten)]
    pub target: Target,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ApplyArgs {
    /// The proposal to take; with none, `pins.proposed.toml` in the project's fab folder.
    #[arg(long, value_name = "PROPOSAL.TOML")]
    pub proposal: Option<PathBuf>,
    #[command(flatten)]
    pub target: Target,
}

pub fn run(args: Args) -> ExitCode {
    let (name, target) = match &args.command {
        Command::Show(t) => ("show", t),
        Command::Lock(a) => ("lock", &a.target),
        Command::Unlock(a) => ("unlock", &a.target),
        Command::Move(a) => ("move", &a.target),
        Command::Apply(a) => ("apply", &a.target),
    };
    let command = format!("board pins {name}");
    let json = target.format == Format::Json;
    match act(&args.command, target, &command) {
        Ok(done) => {
            if json {
                println!("{}", done.json);
            } else {
                print!("{}", done.text);
            }
            ExitCode::SUCCESS
        }
        Err(failed) => {
            if json {
                println!("{}", failed.json(&command));
            } else {
                eprint!("{}", failed.text);
            }
            ExitCode::from(failed.code)
        }
    }
}

/// What a command did, as the text a person reads and the JSON document.
struct Done {
    text: String,
    json: Value,
}

/// Why a command did not do what it was asked: the exit code (1 for a choice the rules refuse,
/// 2 when it could not run), the text for a person, and what the JSON document adds.
struct Failed {
    code: u8,
    text: String,
    status: u16,
    error_code: String,
    message: String,
    extra: Vec<(&'static str, Value)>,
}

impl Failed {
    fn usage(message: impl Into<String>) -> Failed {
        let message = message.into();
        Failed {
            code: 2,
            text: format!("error: {message}\n"),
            status: 0,
            error_code: "usage".into(),
            message,
            extra: Vec::new(),
        }
    }

    fn api(e: ApiError) -> Failed {
        let error_code = e
            .detail
            .as_ref()
            .map_or_else(|| "error".to_string(), |d| d.code.clone());
        Failed {
            code: if e.status == 422 { 1 } else { 2 },
            text: format!("error: {}\n", e.message),
            status: e.status,
            error_code,
            message: e.message,
            extra: Vec::new(),
        }
    }

    fn json(&self, command: &str) -> Value {
        let mut out = json!({
            "command": command,
            "ok": false,
            "error": { "status": self.status, "code": self.error_code, "message": self.message },
        });
        for (key, value) in &self.extra {
            out[*key] = value.clone();
        }
        out
    }
}

/// One board with pin requests: its summary, and each request with the part it asks.
struct Pinned {
    summary: BoardSummary,
    requests: Vec<(String, PinRequestInfo)>,
}

fn act(command: &Command, target: &Target, name: &str) -> Result<Done, Failed> {
    let source = super::common::resolve(target.file.as_deref()).map_err(Failed::usage)?;
    if source.files.is_empty() {
        return Err(Failed::usage(
            "this project has no boards: `top_level` in netsmith.toml is empty; name a .nsm file",
        ));
    }
    let workspace = Workspace::open(source.clone()).map_err(Failed::usage)?;
    let boards = pinned(&workspace, target.board.as_deref())?;
    match command {
        Command::Show(_) => Ok(show(&boards, name)),
        Command::Lock(a) | Command::Unlock(a) => {
            let locked = matches!(command, Command::Lock(_));
            let board = one_with(boards, Some(&a.request), target)?;
            let wrote = workspace
                .pin_lock(&PinLock {
                    board: board.summary.name.clone(),
                    request: a.request.clone(),
                    locked,
                })
                .map_err(Failed::api)?;
            wrote_entry(&source, &board, &a.request, &wrote.file, name)
        }
        Command::Move(a) => {
            let board = one_with(boards, Some(&a.request), target)?;
            let moved = workspace.pin_move(&PinMove {
                board: board.summary.name.clone(),
                request: a.request.clone(),
                pin: a.pin.clone(),
            });
            match moved {
                Ok(wrote) => wrote_entry(&source, &board, &a.request, &wrote.file, name),
                Err(e) => {
                    let mut failed = Failed::api(e);
                    let (text, may_take) = may_take(&board, &a.request);
                    failed.text.push_str(&text);
                    failed.extra.push(("board", json!(board.summary.name)));
                    failed.extra.push(("request", json!(a.request)));
                    failed.extra.push(("mayTake", may_take));
                    Err(failed)
                }
            }
        }
        Command::Apply(a) => {
            let board = one_with(boards, None, target)?;
            let path = a
                .proposal
                .clone()
                .unwrap_or_else(|| source.fab_out.join("pins.proposed.toml"));
            let proposal = fs::read_to_string(&path).map_err(|e| {
                Failed::usage(format!(
                    "{}: {e} (`netsmith board fab --pins locked` writes it when it proposes a swap)",
                    path.display()
                ))
            })?;
            let applied = workspace
                .apply_pins(&board.summary.name, &proposal)
                .map_err(Failed::api)?;
            let mut text = String::new();
            if applied.moved.is_empty() {
                text.push_str(&format!(
                    "{} already has every pin of the proposal: nothing to apply\n",
                    applied.file
                ));
            } else {
                text.push_str(&format!("wrote {}\n", applied.file));
                for line in &applied.moved {
                    text.push_str(&format!("  {}: {line}\n", board.summary.name));
                }
            }
            Ok(Done {
                text,
                json: json!({
                    "command": name,
                    "ok": true,
                    "board": board.summary.name,
                    "file": applied.file,
                    "moved": applied.moved,
                }),
            })
        }
    }
}

/// The boards of the analysis that have pin requests (all of them when `named`, the board a
/// person named: then the board is kept even with none), with each request and its part.
fn pinned(workspace: &Workspace, named: Option<&str>) -> Result<Vec<Pinned>, Failed> {
    let analysis: Analysis = workspace.analysis();
    let summaries: Vec<&BoardSummary> = analysis
        .boards
        .iter()
        .filter(|b| match named {
            Some(n) => b.name == n || b.definition == n || b.name.rsplit('/').next() == Some(n),
            None => true,
        })
        .collect();
    if summaries.is_empty() {
        let mut text = match named {
            Some(n) => format!("there is no board `{n}`"),
            None => "no board builds".to_string(),
        };
        if !analysis.boards.is_empty() {
            let names: Vec<&str> = analysis.boards.iter().map(|b| b.name.as_str()).collect();
            text.push_str(&format!(" (the boards are {})", names.join(", ")));
        }
        for p in analysis
            .problems
            .iter()
            .filter(|p| p.severity == Severity::Error)
        {
            text.push_str(&format!("\n{}", p.rendered.trim_end()));
        }
        return Err(Failed::usage(text));
    }
    let mut out = Vec::new();
    for summary in summaries {
        let detail = workspace.board(&summary.name).map_err(Failed::api)?;
        let requests: Vec<(String, PinRequestInfo)> = detail
            .parts
            .iter()
            .flat_map(|part| {
                part.pin_requests
                    .iter()
                    .flatten()
                    .map(|r| (part.path.clone(), r.clone()))
            })
            .collect();
        if named.is_some() || !requests.is_empty() {
            out.push(Pinned {
                summary: summary.clone(),
                requests,
            });
        }
    }
    Ok(out)
}

/// The one board a write acts on: the one that has `request`, or with none the one that has
/// requests.
fn one_with(
    boards: Vec<Pinned>,
    request: Option<&String>,
    target: &Target,
) -> Result<Pinned, Failed> {
    let mut found: Vec<Pinned> = boards
        .into_iter()
        .filter(|b| match request {
            Some(r) => b.requests.iter().any(|(_, q)| q.request == *r),
            None => !b.requests.is_empty(),
        })
        .collect();
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(Failed::usage(match request {
            Some(r) => format!(
                "{} has no pin request `{r}`; `netsmith board pins show` lists them",
                match &target.board {
                    Some(b) => format!("board `{b}`"),
                    None => "no board".to_string(),
                }
            ),
            None => "no board has a pin request".to_string(),
        })),
        _ => {
            let names: Vec<&str> = found.iter().map(|b| b.summary.name.as_str()).collect();
            Err(Failed::usage(format!(
                "{} boards have {}: name one with `--board` ({})",
                names.len(),
                request.map_or("pin requests".to_string(), |r| format!("a request `{r}`")),
                names.join(", ")
            )))
        }
    }
}

/// The pin file's entry of `request` as it is on disk after a write, as the success message.
fn wrote_entry(
    source: &Source,
    board: &Pinned,
    request: &str,
    file: &str,
    command: &str,
) -> Result<Done, Failed> {
    let path = source
        .files
        .iter()
        .find(|f| f.name == board.summary.file)
        .map(|f| netsmith_fabricate::pins::pins_path(&f.path))
        .ok_or_else(|| {
            Failed::usage(format!(
                "{} is not a file of the project",
                board.summary.file
            ))
        })?;
    let text =
        fs::read_to_string(&path).map_err(|e| Failed::usage(format!("{}: {e}", path.display())))?;
    let parsed = PinFile::parse(&text)
        .map_err(|_| Failed::usage(format!("{file} does not read after the write")))?;
    let entry = parsed
        .board(&board.summary.definition)
        .and_then(|b| b.get(request))
        .ok_or_else(|| {
            Failed::usage(format!(
                "{file} has no entry for `{request}` after the write"
            ))
        })?;
    let lock = if entry.lock { ", locked" } else { "" };
    Ok(Done {
        text: format!(
            "wrote {file}: {} {request} is on {} for {} ({}){lock}\n",
            board.summary.name,
            entry.pin,
            entry.function,
            entry.source.word()
        ),
        json: json!({
            "command": command,
            "ok": true,
            "board": board.summary.name,
            "request": request,
            "file": file,
            "entry": {
                "pin": entry.pin,
                "function": entry.function,
                "lock": entry.lock,
                "source": entry.source.word(),
            },
        }),
    })
}

/// The pins a refused move could have used: the free ones that offer the function, which the
/// request does not have, as text and as JSON.
fn may_take(board: &Pinned, request: &str) -> (String, Value) {
    let Some((_, info)) = board.requests.iter().find(|(_, q)| q.request == request) else {
        return (String::new(), json!([]));
    };
    let free: Vec<_> = info
        .offers
        .iter()
        .filter(|o| o.held_by.is_none() && Some(&o.pin) != info.pin.as_ref())
        .collect();
    let text = if free.is_empty() {
        format!(
            "`{request}` ({}) has no other free pin to take\n",
            info.function
        )
    } else {
        let pins: Vec<String> = free.iter().map(|o| pin_and_pad(&o.pin, &o.pad)).collect();
        format!(
            "`{request}` ({}) may take: {}\n",
            info.function,
            pins.join(", ")
        )
    };
    let json = free
        .iter()
        .map(|o| {
            let mut v = json!({ "pin": o.pin });
            if let Some(pad) = &o.pad {
                v["pad"] = json!(pad);
            }
            v
        })
        .collect();
    (text, Value::Array(json))
}

fn pin_and_pad(pin: &str, pad: &Option<String>) -> String {
    match pad {
        Some(pad) => format!("{pin} (pad {pad})"),
        None => pin.to_string(),
    }
}

fn show(boards: &[Pinned], command: &str) -> Done {
    let mut text = String::new();
    let mut documents = Vec::new();
    for board in boards {
        let file = netsmith_fabricate::pins::file_name(&board.summary.file);
        text.push_str(&format!("{} ({file})", board.summary.name));
        if board.summary.stale {
            text.push_str(": the source does not build, so this is the last good board");
        }
        text.push('\n');
        if board.requests.is_empty() {
            text.push_str("  no pin requests\n");
        }
        let mut requests = Vec::new();
        for (part, r) in &board.requests {
            let pin = match &r.pin {
                Some(pin) => pin_and_pad(pin, &r.pad),
                None => "no pin".to_string(),
            };
            text.push_str(&format!(
                "  {} on {part}: {} at {pin}",
                r.request, r.function
            ));
            if r.locked {
                text.push_str(", locked");
            }
            if let Some(entry) = &r.entry {
                text.push_str(&format!(", source {entry}"));
            }
            if let Some(from) = &r.from {
                text.push_str(&format!(", from {from}"));
            }
            text.push('\n');
            if let Some(message) = &r.message {
                text.push_str(&format!("    {message}\n"));
            }
            if !r.offers.is_empty() {
                let pins: Vec<String> = r
                    .offers
                    .iter()
                    .filter(|o| Some(&o.pin) != r.pin.as_ref())
                    .map(|o| match &o.held_by {
                        Some(by) => format!("{} (held by `{by}`)", o.pin),
                        None => pin_and_pad(&o.pin, &o.pad),
                    })
                    .collect();
                if !pins.is_empty() {
                    text.push_str(&format!("    other pins: {}\n", pins.join(", ")));
                }
            }
            let mut value = serde_json::to_value(r).expect("plain data");
            value["part"] = json!(part);
            requests.push(value);
        }
        documents.push(json!({
            "board": board.summary.name,
            "file": file,
            "stale": board.summary.stale,
            "requests": requests,
        }));
    }
    Done {
        text,
        json: json!({ "command": command, "ok": true, "boards": documents }),
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::parse_args;

    fn parses(words: &[&str]) -> bool {
        let mut all = vec!["board", "pins"];
        all.extend(words);
        parse_args(&all).is_ok()
    }

    #[test]
    fn every_verb_parses_with_its_words() {
        assert!(parses(&["show"]));
        assert!(parses(&[
            "show", "--file", "a.nsm", "--board", "B", "--format", "json"
        ]));
        assert!(parses(&["lock", "status"]));
        assert!(parses(&["unlock", "status", "--board", "B"]));
        assert!(parses(&["move", "status", "p03", "--format", "json"]));
        assert!(parses(&["apply", "--proposal", "fab/pins.proposed.toml"]));
    }

    #[test]
    fn a_verb_needs_its_words() {
        assert!(!parses(&[]));
        assert!(!parses(&["lock"]));
        assert!(!parses(&["move", "status"]));
        assert!(!parses(&["show", "extra"]));
    }
}
