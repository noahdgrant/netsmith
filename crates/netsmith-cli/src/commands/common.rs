//! What several commands share: `--format` and `--notes`, the names of `--view` and
//! `--highlight`, and the steps that find and build what a command acts on.

use std::fs;
use std::process::ExitCode;

use clap::{Args as ClapArgs, ValueEnum};
use netsmith::Libraries;
use netsmith::project::Target;
use netsmith_check::Loader;
use netsmith_core::timings;
use netsmith_solve::Catalogs;

/// The port `netsmith editor` listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 8787;

/// `--format` and `--notes`, of `build`, `check` and `fab`.
#[derive(Debug, PartialEq, ClapArgs)]
pub struct Output {
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` prints one JSON document on stdout and nothing else there: `command`, `ok`, the
    /// `boards` with the folders written, the `findings`, and the count of the `notes` for each
    /// cause. Each finding has a `severity` (`error`, `warning` or `note`), a `code`, and the
    /// `board`, the member `path`, the `file`, `line`, `column` and `message` it has. The code is
    /// the stage that found an error or a warning (`syntax`, `check`, `solve`, `design`,
    /// `guideline`, `power`, `lock`, `place`, `fab`), or `note.` and the cause of a note
    /// (`note.not_stated`, `note.part_model`, `note.unjudged`, `note.circular`, `note.supply`,
    /// `note.pick`, `note.track`, `note.other`). With `--notes summary` or `none` the notes are
    /// left out of the findings; their counts are always there.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
    /// Print each note, a count of the notes for each cause, or none.
    ///
    /// `summary` prints a board's notes as one line with a count for each cause (`note: TwoBuck:
    /// 7 notes: 6 not_stated, 1 unjudged`), `none` leaves them out, and `all`, the default,
    /// prints each. With `--format json` the notes are findings only with `all`; their counts are
    /// always there.
    #[arg(long, value_enum, default_value = "all")]
    pub notes: Notes,
}

/// `--format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// For a person.
    Text,
    /// One JSON document on stdout.
    Json,
}

impl From<Format> for netsmith::findings::Format {
    fn from(format: Format) -> Self {
        match format {
            Format::Text => netsmith::findings::Format::Text,
            Format::Json => netsmith::findings::Format::Json,
        }
    }
}

/// `--notes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Notes {
    /// Each note.
    All,
    /// A count for each cause.
    Summary,
    /// No note.
    None,
}

impl From<Notes> for netsmith::findings::NotesMode {
    fn from(notes: Notes) -> Self {
        match notes {
            Notes::All => netsmith::findings::NotesMode::All,
            Notes::Summary => netsmith::findings::NotesMode::Summary,
            Notes::None => netsmith::findings::NotesMode::None,
        }
    }
}

/// Write the pin file beside each source that has pin requests, when a run changes it (ADR 0101),
/// and say what was written and what is stale. `--locked` writes nothing and stops instead. The
/// exit code of the command when it stops, `None` to go on.
pub(super) fn sync_pins(file: Option<&str>, locked: bool) -> Option<ExitCode> {
    // Only what the check needs: the catalogs are loaded, and timed, by the command itself.
    let Ok(target) = Target::resolve(file) else {
        // The command says it again.
        return None;
    };
    let Ok(loader) = target.loader() else {
        return None;
    };
    for source in &target.files {
        let Ok(text) = fs::read_to_string(&source.path) else {
            return None;
        };
        match netsmith::pins::sync(&source.name, &source.path, &text, &loader, locked) {
            Ok(synced) => {
                for note in synced.notes {
                    eprintln!("note: {note}");
                }
            }
            Err(message) => {
                eprint!("{message}");
                return Some(ExitCode::FAILURE);
            }
        }
    }
    None
}

/// What a command acts on: the named file or the project's top-level files, what their
/// imports can reach, and the catalogs to pick parts from. A problem is printed and is the
/// exit code.
pub(super) fn target(
    file: Option<&str>,
    strict: bool,
) -> Result<(Target, Loader, Catalogs), ExitCode> {
    let fail = |e: String| {
        eprintln!("{e}");
        ExitCode::from(2)
    };
    let target = Target::resolve(file).map_err(fail)?;
    let loader = target.loader().map_err(fail)?;
    let mut catalogs = timings::stage("catalogs", "", || target.catalogs()).map_err(fail)?;
    // `--strict` makes guideline findings errors for this run; the manifest's
    // `guidelines = "error"` already has (ADR 0036).
    if strict {
        catalogs = catalogs.with_strict_guidelines(true);
    }
    Ok((target, loader, catalogs))
}

/// The KiCad libraries of the target's project (ADR 0021). A mistake in `kicad.toml` is
/// printed and is the exit code.
pub(super) fn libraries(target: &Target) -> Result<Libraries, ExitCode> {
    let root = target.project.as_ref().map(|p| p.root.as_path());
    Libraries::for_project(root).map_err(|e| {
        eprintln!("{e}");
        ExitCode::from(2)
    })
}

/// Whether a project with no file to act on was found, which it says. A library-only
/// project lists no `top_level` file (ADR 0011).
pub(super) fn no_boards(target: &Target) -> bool {
    let empty = target.files.is_empty();
    if empty {
        eprintln!(
            "error: this project has no boards: `top_level` in netsmith.toml is empty; name a .nsm file to build it"
        );
    }
    empty
}

/// What `board3d` and `render` do first: the one board `file` and `--board` pick, built and
/// placed as `fab` places it (the pick lock and the layout file beside its source are read,
/// nothing is written), and `then` of it. A project of several top-level files takes the first
/// that has the board `--board` names. The error is printed and is the exit code: 1 when no file
/// builds the board or places it, 2 when the command cannot start.
pub(super) fn with_placed_board<T>(
    file: Option<&str>,
    board: Option<&str>,
    then: impl Fn(&netsmith::board3d::Placed) -> Result<T, String>,
) -> Result<T, ExitCode> {
    let (target, loader, catalogs) = target(file, false)?;
    if no_boards(&target) {
        return Err(ExitCode::from(2));
    }
    if target.files.len() > 1 && board.is_none() {
        eprintln!(
            "error: the project has {} top-level files; name one, or a board with `--board <name>`",
            target.files.len()
        );
        return Err(ExitCode::from(2));
    }
    let libraries = timings::stage("libraries", "", || libraries(&target))?;
    let mut errors = String::new();
    for source in &target.files {
        let text = fs::read_to_string(&source.path).map_err(|e| {
            eprintln!("{}: {e}", source.name);
            ExitCode::from(2)
        })?;
        // The pick lock holds the parts it names to their rows, as for `fab`; it is not written.
        let (held, _) = netsmith::pick_lock::open(&source.path, &catalogs, false).map_err(|e| {
            eprintln!("{e}");
            ExitCode::from(2)
        })?;
        let placed = netsmith::board3d::Placed {
            name: &source.name,
            text: &text,
            loader: &loader,
            catalogs: held.as_ref().unwrap_or(&catalogs),
            libraries: &libraries,
            layout: netsmith::layout::layout_path(&source.path),
            board,
        };
        match then(&placed) {
            Ok(done) => return Ok(done),
            Err(e) => errors.push_str(&e),
        }
    }
    eprint!("{errors}");
    Err(ExitCode::FAILURE)
}

/// `--view` of `render` and `view`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ViewName {
    /// Straight at the top face.
    Top,
    /// Straight at the bottom face.
    Bottom,
    /// Along the board's front edge.
    Front,
    /// Along the board's back edge.
    Back,
    /// Along the board's left edge.
    Left,
    /// Along the board's right edge.
    Right,
    /// From a corner, above.
    Iso,
    /// From a corner, below.
    IsoBottom,
}

impl ViewName {
    pub(super) fn raster(self) -> netsmith_3d::raster3d::View {
        use netsmith_3d::raster3d::View;
        match self {
            ViewName::Top => View::Top,
            ViewName::Bottom => View::Bottom,
            ViewName::Front => View::Front,
            ViewName::Back => View::Back,
            ViewName::Left => View::Left,
            ViewName::Right => View::Right,
            ViewName::Iso => View::Iso,
            ViewName::IsoBottom => View::IsoBottom,
        }
    }

    pub(super) fn editor(self) -> netsmith_editor::api::RenderView {
        use netsmith_editor::api::RenderView;
        match self {
            ViewName::Top => RenderView::Top,
            ViewName::Bottom => RenderView::Bottom,
            ViewName::Front => RenderView::Front,
            ViewName::Back => RenderView::Back,
            ViewName::Left => RenderView::Left,
            ViewName::Right => RenderView::Right,
            ViewName::Iso => RenderView::Iso,
            ViewName::IsoBottom => RenderView::IsoBottom,
        }
    }
}

/// The ids of one `--highlight` or `--select`, split at commas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ids(pub Vec<String>);

pub(super) fn parse_ids(given: &str) -> Result<Ids, String> {
    let ids: Vec<String> = given
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    if ids.len() > 1 && ids.iter().any(|id| id == "none") {
        return Err("`none` clears it, so no id goes with it".to_string());
    }
    Ok(Ids(ids))
}

/// What the tests of every command share: the command line as main.rs has it.
#[cfg(test)]
pub(crate) mod testing {
    use clap::Parser;
    use clap::error::ErrorKind;

    use crate::commands::{Command, common};

    /// The command line as main.rs has it.
    #[derive(Debug, Parser)]
    #[command(name = "netsmith", version)]
    pub(crate) struct Cli {
        #[command(subcommand)]
        command: Command,
    }

    pub(crate) fn parse_args(args: &[&str]) -> Result<Command, clap::Error> {
        Cli::try_parse_from(std::iter::once(&"netsmith").chain(args)).map(|cli| cli.command)
    }

    pub(crate) fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args).map_err(|e| e.to_string())
    }

    pub(crate) fn kind(args: &[&str]) -> ErrorKind {
        parse_args(args).unwrap_err().kind()
    }

    pub(crate) fn output() -> common::Output {
        common::Output {
            format: common::Format::Text,
            notes: common::Notes::All,
        }
    }
}
