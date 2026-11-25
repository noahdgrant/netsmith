//! What `board bom`, `board power`, `board system` and `board panel` share (issue #1096): each
//! prints one of the editor's read-only views of the solved design, as text or as the very JSON
//! the editor's `GET /api/bom`, `/api/power`, `/api/system` and `/api/panel` answer. The views
//! are made by `netsmith_workspace::Workspace` through `netsmith_editor::Backend`, the code the
//! editor's server calls, so the command line and the editor cannot differ; nothing here makes a
//! view, it opens the workspace, finds the name the view is asked for and prints the answer.

use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_editor::api::{Analysis, Severity};
use netsmith_editor::{ApiError, Backend};
use netsmith_workspace::Workspace;

use super::common::Format;

/// `--format`, of the four views.
#[derive(Debug, PartialEq, ClapArgs)]
pub struct Printing {
    /// Text for a person, or the editor's own JSON for the view on stdout.
    ///
    /// `json` prints one JSON document and nothing else on stdout: the answer of the editor's
    /// route for the view, the same document (`GET /api/bom?board=`, `/api/power?board=`,
    /// `/api/system?name=`, `/api/panel?name=`), with the field names of the editor's page
    /// (`totalUsd`, `capacityFrom`, `cellWidth`).
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

/// The workspace over the file (or the project's boards), analysed once: what the editor has
/// when it opens the same target. A problem is printed and is the exit code.
pub(super) fn open(file: Option<&str>, offline: bool) -> Result<(Workspace, Analysis), ExitCode> {
    let fail = |e: String| {
        eprintln!("{e}");
        ExitCode::from(2)
    };
    let target = super::common::resolve(file)
        .map_err(fail)?
        .with_offline(offline);
    if super::common::no_boards(&target) {
        return Err(ExitCode::from(2));
    }
    let workspace = Workspace::open(target).map_err(fail)?;
    let analysis = workspace.analysis();
    Ok((workspace, analysis))
}

/// What a view asks the workspace for: the name of a board (`bom`, `power`), or of a system or a
/// board (`system`), or of a panel (`panel`).
pub(super) enum Kind {
    /// A board, as `--board` names it.
    Board,
    /// A system, else a board, as `--name` names it.
    System,
    /// A panel, as `--name` names it.
    Panel,
}

/// The name to ask for: what `asked` says, matched to a name of the analysis as the editor names
/// it (`Board`, or `System/Board` for a board of a system, which `Board` alone also names when no
/// other board has that name), or the only one there is when nothing is asked. A name that
/// cannot be chosen is printed with the choices, and is the exit code 2.
pub(super) fn choose(
    analysis: &Analysis,
    kind: &Kind,
    asked: Option<&str>,
) -> Result<String, ExitCode> {
    let boards: Vec<&str> = analysis.boards.iter().map(|b| b.name.as_str()).collect();
    let systems: Vec<&str> = analysis.systems.iter().map(|s| s.name.as_str()).collect();
    let panels: Vec<&str> = analysis.panels.iter().map(|p| p.name.as_str()).collect();
    let (what, flag, choices): (&str, &str, Vec<&str>) = match kind {
        Kind::Board => ("board", "--board", boards.clone()),
        Kind::Panel => ("panel", "--name", panels),
        // A system is shown by its name; a board of the project too, which the editor shows as
        // its members and nets. With a name left out: the only system, or else the only board.
        Kind::System => (
            "system or board",
            "--name",
            systems.iter().chain(boards.iter()).copied().collect(),
        ),
    };
    let fail = |message: String| {
        eprintln!("error: {message}");
        for problem in analysis
            .problems
            .iter()
            .filter(|p| p.severity == Severity::Error)
        {
            eprint!("{}", problem.rendered);
        }
        ExitCode::from(2)
    };
    let list = |names: &[&str]| names.join(", ");
    match asked {
        Some(name) => {
            if choices.contains(&name) {
                return Ok(name.to_string());
            }
            // `Board` for `System/Board`, when only one board has that name.
            let suffix = format!("/{name}");
            let mut by_suffix = choices.iter().filter(|c| c.ends_with(&suffix));
            match (by_suffix.next(), by_suffix.next()) {
                (Some(one), None) => Ok((*one).to_string()),
                _ => Err(fail(format!(
                    "no {what} named `{name}`{}",
                    if choices.is_empty() {
                        String::new()
                    } else {
                        format!("; the {what}s are {}", list(&choices))
                    }
                ))),
            }
        }
        None => {
            let default = match kind {
                Kind::System if systems.len() == 1 => Some(systems[0]),
                Kind::System if systems.is_empty() && boards.len() == 1 => Some(boards[0]),
                Kind::System => None,
                _ if choices.len() == 1 => Some(choices[0]),
                _ => None,
            };
            match default {
                Some(name) => Ok(name.to_string()),
                None if choices.is_empty() => Err(fail(format!(
                    "the source has no {what} to show (a mistake in it, or none declared)"
                ))),
                None => Err(fail(format!(
                    "{} {what}s: name one with `{flag} <name>`: {}",
                    choices.len(),
                    list(&choices)
                ))),
            }
        }
    }
}

/// Prints a view's answer as `format` says: the JSON of `view` as the editor sends it, or the
/// `text` made of it. An error of the workspace is printed as the editor says it, with the
/// error problems of the source, and is the exit code 1.
pub(super) fn print<T>(
    answer: Result<T, ApiError>,
    analysis: &Analysis,
    format: Format,
    (json, text): (
        impl FnOnce(&T) -> serde_json::Result<String>,
        impl FnOnce(&T) -> String,
    ),
) -> ExitCode {
    match answer {
        Ok(view) => {
            match format {
                Format::Json => match json(&view) {
                    Ok(json) => println!("{json}"),
                    Err(e) => {
                        eprintln!("error: {e}");
                        return ExitCode::from(2);
                    }
                },
                Format::Text => print!("{}", text(&view)),
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e.message);
            for problem in analysis
                .problems
                .iter()
                .filter(|p| p.severity == Severity::Error)
            {
                eprint!("{}", problem.rendered);
            }
            ExitCode::FAILURE
        }
    }
}

/// Rows as aligned columns, the first row the header; a cell is as wide as its widest, the last
/// column is not padded.
pub(super) fn table(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|c| {
            rows.iter()
                .filter_map(|r| r.get(c))
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    for row in rows {
        let mut line = String::new();
        for (c, cell) in row.iter().enumerate() {
            if c + 1 == row.len() {
                line.push_str(cell);
            } else {
                line.push_str(cell);
                line.push_str(&" ".repeat(widths[c] - cell.chars().count() + 2));
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}
