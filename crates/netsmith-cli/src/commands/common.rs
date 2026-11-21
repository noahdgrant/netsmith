//! What several commands share: `--format` and `--notes`, the names of `--view` and
//! `--highlight`, and the steps that find and build what a command acts on.

use std::fs;
use std::process::ExitCode;

use clap::{Args as ClapArgs, ValueEnum};
use netsmith_check::Loader;
use netsmith_core::timings;
use netsmith_fabricate::project::Target;
use netsmith_pipeline::Libraries;
use netsmith_solve::Catalogs;
use netsmith_tools::findings::{Finding, Report, Severity};

/// The port `netsmith view editor` listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 8787;

/// `--place-seeds` and `--place-budget`, of `fab` and `dev board3d` (issues #764 and #766).
#[derive(Debug, PartialEq, ClapArgs)]
pub struct Placing {
    /// How many seeds the placer tries for a board, over `place_seeds` in netsmith.toml.
    ///
    /// From 1 (the layout file's seed only) to 32; the default is 8. Each seed is a whole
    /// placement, and a seed that ends a few parts short is run again up to four more times, so
    /// a board no seed places costs about this many times five runs.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=32))]
    pub place_seeds: Option<u32>,
    /// How long the placer may look for a board's placement, over `place_budget` in netsmith.toml.
    ///
    /// Seconds (`120`, `120s`, `2m`), tries (`5tries`; a try is one run of the placer in a fixed
    /// outline), or both (`120s+5tries`), whichever is spent first; no limit by default. When it
    /// is spent the placer ends the run it is in, begins no other seed and gives the closest try,
    /// with what is still out: `placed nothing in 120 s (...): the placer tried 2 of 8 seeds ...,
    /// and the closest is seed 1, where this blocks it:` and the parts by name. A budget of tries
    /// gives the same answer on every machine; a budget of time does not.
    #[arg(long, value_name = "BUDGET", value_parser = netsmith_fabricate::layout::parse_place_budget)]
    pub place_budget: Option<netsmith_core::job::Limit>,
}

impl Placing {
    /// The seeds to try for a board of `target`: `--place-seeds`, else the manifest's.
    pub fn seeds(&self, target: &Target) -> u32 {
        self.place_seeds
            .unwrap_or_else(|| netsmith_fabricate::layout::place_seeds(target.project.as_ref()))
    }

    /// The place budget for a board of `target`: `--place-budget`, else the manifest's.
    pub fn budget(&self, target: &Target) -> netsmith_core::job::Limit {
        netsmith_fabricate::layout::place_budget(target.project.as_ref(), self.place_budget)
    }
}

/// The weights of the router's quality objective (issue #1517), of `board fab`, `routes route`
/// and `dev route`: with none, the router routes at its own costs, as it always has. Each one
/// wins over the key of the same name in `netsmith.toml` (`length_weight`, `via_weight`,
/// `layer_change_weight`, issue #1536), which `project order` and the editor's routing read too.
#[derive(Debug, PartialEq, ClapArgs)]
pub struct Weighing {
    /// What a millimetre of a signal net's track costs the router, as a share of its own (1).
    ///
    /// From 0.1 to 10. Any of the three weights turns on the router's polish pass, which routes
    /// each signal line again on a finished board and keeps the cheaper copper, and its reroute
    /// pass, which lays the nets that detour most again with the nets in their way, the worst
    /// first, and keeps that only when the total gets shorter; with none the
    /// router routes as before. A signal net is one no plane, pour or zone fills and whose
    /// current needs no vias side by side.
    #[arg(long, value_name = "SHARE", value_parser = parse_weight)]
    pub length_weight: Option<f64>,
    /// What a via of a signal net costs the router, in mm of track (its own is 15).
    ///
    /// Raise it for fewer vias at the price of longer tracks; 0 to 1000.
    #[arg(long, value_name = "MM", value_parser = parse_weight)]
    pub via_weight: Option<f64>,
    /// What a change of layer of a signal net costs the router beyond its via, in mm of track
    /// (its own is 0).
    ///
    /// Each via of a line is one change of layer, so the router pays the two together; `board
    /// nets` counts them apart. 0 to 1000.
    #[arg(long, value_name = "MM", value_parser = parse_weight)]
    pub layer_change_weight: Option<f64>,
}

impl Weighing {
    /// The weights of a run: the flags stated, else the project's keys, the router's own for
    /// the ones neither states; `None` when none is stated (`netsmith_fabricate::fab::weights`).
    pub fn weights(
        &self,
        project: Option<&netsmith_fabricate::project::Project>,
    ) -> Result<Option<netsmith_router::Weights>, String> {
        netsmith_fabricate::fab::weights(
            project,
            (
                self.length_weight,
                self.via_weight,
                self.layer_change_weight,
            ),
        )
    }
}

/// A weight: a number that is not negative.
fn parse_weight(s: &str) -> Result<f64, String> {
    let v: f64 = s.parse().map_err(|_| format!("`{s}` is not a number"))?;
    if !v.is_finite() || v < 0.0 {
        return Err(format!("a weight is a number of 0 or more, not `{s}`"));
    }
    Ok(v)
}

/// `--progress` and `--to-editor`, of `fab` and `dev board3d` (issue #764).
#[derive(Debug, PartialEq, ClapArgs)]
pub struct Watching {
    /// How the progress is printed on stderr: lines for a person, or one JSON object per line.
    ///
    /// `json` prints each line of the placer and the router as `{"board", "stage", "unit",
    /// "done", "total", "detail", "elapsedMs", "message"}` (`stage` is `placing` or `routing`,
    /// `total` and `detail` only when there are some) and any other line of progress as
    /// `{"message"}`; errors and warnings stay text. With `board fab --format json` the progress is
    /// printed only with `--progress json`; `--quiet` prints none.
    #[arg(long, value_enum, value_name = "FORMAT", default_value = "text")]
    pub progress: crate::progress::Print,
    /// Send the progress to the editor running on this port too (`--to-editor=9000`; 8787 with no port), which
    /// shows the run in its status bar.
    ///
    /// Each line is posted to the editor's `/api/progress` as it is told, also with `--quiet`, and
    /// the last says how the run ended. An editor that does not take a line is said once on
    /// stderr and sent nothing more; the run goes on.
    #[arg(long, value_name = "PORT", num_args = 0..=1, require_equals = true, default_missing_value = "8787", value_parser = clap::value_parser!(u16).range(1..))]
    pub to_editor: Option<u16>,
}

impl Watching {
    /// Says where the progress of this run of `command` goes ([`crate::progress::setup`]):
    /// printed as `--progress` says unless `quiet` (`--quiet`, or `board fab --format json` with text
    /// progress), and posted to `--to-editor`'s editor.
    pub fn tell_progress(&self, command: &'static str, quiet: bool) {
        let print = if quiet {
            crate::progress::Print::Nothing
        } else {
            self.progress
        };
        crate::progress::setup(print, self.to_editor, command);
    }
}

/// Runs `work` with the placer's and the router's lines made into events, when they are to be
/// printed as JSON or posted to an editor ([`crate::progress`]).
pub fn with_stage_events<R>(work: impl FnOnce() -> R) -> R {
    if crate::progress::wants_events() {
        netsmith_fabricate::layout::stage_events(std::rc::Rc::new(crate::progress::event), work)
    } else {
        work()
    }
}

/// Prints a line of the placer's progress to stderr, as `--progress` says.
fn say_on_stderr(line: &str) {
    crate::progress::tell(line);
}

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Format {
    /// For a person.
    #[default]
    Text,
    /// One JSON document on stdout.
    Json,
}

impl From<Format> for netsmith_tools::findings::Format {
    fn from(format: Format) -> Self {
        match format {
            Format::Text => netsmith_tools::findings::Format::Text,
            Format::Json => netsmith_tools::findings::Format::Json,
        }
    }
}

/// Prints `document` as the one JSON document of a `--format json` run: pretty, with a newline.
pub fn print_json(document: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(document).expect("a document is plain data")
    );
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

impl From<Notes> for netsmith_tools::findings::NotesMode {
    fn from(notes: Notes) -> Self {
        match notes {
            Notes::All => netsmith_tools::findings::NotesMode::All,
            Notes::Summary => netsmith_tools::findings::NotesMode::Summary,
            Notes::None => netsmith_tools::findings::NotesMode::None,
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
        match netsmith_fabricate::pins::sync(&source.name, &source.path, &text, &loader, locked) {
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
    target_offline(file, strict, false)
}

/// [`target`] for a run that may be `--offline`: the project's supply then asks no provider that
/// would use the network (issue #803).
pub(super) fn target_offline(
    file: Option<&str>,
    strict: bool,
    offline: bool,
) -> Result<(Target, Loader, Catalogs), ExitCode> {
    let fail = |e: String| {
        eprintln!("{e}");
        ExitCode::from(2)
    };
    let target = Target::resolve(file).map_err(fail)?.with_offline(offline);
    let loader = target.loader().map_err(fail)?;
    let mut catalogs = timings::stage("catalogs", "", || target.catalogs()).map_err(fail)?;
    // `--strict` makes guideline findings errors for this run; the manifest's
    // `guidelines = "error"` already has (ADR 0036).
    if strict {
        catalogs = catalogs.with_strict_guidelines(true);
    }
    Ok((target, loader, catalogs))
}

/// After a run in which a board failed its check: when the target's project is written for
/// another netsmith release, or says none, the note that an error may be a spelling that changed
/// and where the changes are listed (`netsmith_fabricate::release`, issue #740). Printed as text,
/// or kept as a finding for `--format json`. A file outside a project gets no note.
pub(super) fn release_note(target: &Target, report: Option<&mut Report>) {
    let Some(project) = &target.project else {
        return;
    };
    let Some(note) = netsmith_fabricate::release::note(project.manifest.netsmith.as_deref()) else {
        return;
    };
    match report {
        Some(report) => report
            .findings
            .push(Finding::plain(Severity::Note, "release", note)),
        None => eprintln!("note: {note}"),
    }
}

/// The KiCad libraries of the target's project (ADR 0021). A mistake in `kicad.toml` is
/// printed and is the exit code.
pub(super) fn libraries(target: &Target) -> Result<Libraries, ExitCode> {
    target.libraries().map_err(|e| {
        eprintln!("{e}");
        ExitCode::from(2)
    })
}

/// `build` and `fab`: write the project's dependency lock when it reads a dependency by `git`
/// and has none (issue #1012), as the pick lock is written (ADR 0100); an existing one is never
/// rewritten. Said on stderr, so `--format json` keeps stdout to itself. A failure is printed
/// and is the exit code.
pub(super) fn write_dependency_lock(target: &Target) -> Result<(), ExitCode> {
    let Some(project) = &target.project else {
        return Ok(());
    };
    match netsmith_fabricate::dependency_lock::write_if_missing(project) {
        Ok(Some(_)) => {
            eprintln!(
                "wrote {}, the commit of each dependency by git; commit it",
                netsmith_fabricate::dependency_lock::LOCK_FILE,
            );
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(e) => {
            eprintln!("{e}");
            Err(ExitCode::from(2))
        }
    }
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

/// What `dev board3d` and `view 3d` do first: the one board `file` and `--board` pick, built and
/// placed as `fab` places it (the pick lock and the layout file beside its source are read,
/// nothing is written), and `then` of it. A project of several top-level files takes the first
/// that has the board `--board` names. The error is printed and is the exit code: 1 when no file
/// builds the board or places it, 2 when the command cannot start. With `placing` (`dev board3d`)
/// its seeds and budget are used and the placer's progress is printed to stderr; with none, the
/// manifest's, and nothing is printed.
pub(super) fn with_placed_board<T>(
    file: Option<&str>,
    board: Option<&str>,
    placing: Option<&Placing>,
    then: impl Fn(&netsmith_fabricate::board3d::Placed) -> Result<T, String>,
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
    let places = netsmith_fabricate::models::Places::of(&target);
    let mut errors = String::new();
    for source in &target.files {
        let text = fs::read_to_string(&source.path).map_err(|e| {
            eprintln!("{}: {e}", source.name);
            ExitCode::from(2)
        })?;
        // The pick lock holds the parts it names to their rows, as for `fab`; it is not written.
        let (held, _) = netsmith_pipeline::pick_lock::open(
            &source.path,
            &catalogs,
            false,
            target.locks(source),
        )
        .map_err(|e| {
            eprintln!("{e}");
            ExitCode::from(2)
        })?;
        let placed = netsmith_fabricate::board3d::Placed {
            name: &source.name,
            source: &source.path,
            text: &text,
            loader: &loader,
            catalogs: held.as_ref().unwrap_or(&catalogs),
            libraries: &libraries,
            layout: netsmith_fabricate::layout::layout_path(&source.path),
            board,
            seeds: match placing {
                Some(placing) => netsmith_fabricate::layout::Seeding {
                    seeds: placing.seeds(&target),
                    budget: placing.budget(&target),
                    progress: Some(say_on_stderr),
                },
                None => netsmith_fabricate::layout::Seeding {
                    seeds: netsmith_fabricate::layout::place_seeds(target.project.as_ref()),
                    budget: netsmith_fabricate::layout::place_budget(target.project.as_ref(), None),
                    progress: None,
                },
            },
            models: Some(places.clone()),
        };
        match then(&placed) {
            Ok(done) => return Ok(done),
            Err(e) => errors.push_str(&e),
        }
    }
    eprint!("{errors}");
    Err(ExitCode::FAILURE)
}

/// `--view` of `view 3d` and `view editor-3d`.
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
    pub(super) fn raster(self) -> netsmith_3d::View {
        use netsmith_3d::View;
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

/// `--case` of `view 3d` and `view editor-3d`: how a case model (ADR 0109) is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CaseName {
    /// Not drawn: the board alone.
    Hidden,
    /// Drawn faint, so the board shows inside it.
    SeeThrough,
    /// Drawn opaque: the case hides what is inside it.
    Solid,
}

impl CaseName {
    pub(super) fn render(self) -> netsmith_3d::RenderCaseMode {
        use netsmith_3d::RenderCaseMode as CaseMode;
        match self {
            CaseName::Hidden => CaseMode::Hidden,
            CaseName::SeeThrough => CaseMode::SeeThrough,
            CaseName::Solid => CaseMode::Solid,
        }
    }

    pub(super) fn editor(self) -> netsmith_editor::api::CaseMode {
        use netsmith_editor::api::CaseMode;
        match self {
            CaseName::Hidden => CaseMode::Hidden,
            CaseName::SeeThrough => CaseMode::SeeThrough,
            CaseName::Solid => CaseMode::Solid,
        }
    }
}

/// One `--case` of `view 3d` and `view editor-3d`: a mode, or `hidden:<name>[,<name>]`, which leaves those
/// case models out and draws the rest as the mode says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseArg {
    Mode(CaseName),
    Hidden(Vec<String>),
}

/// The value parser of `--case`.
pub fn parse_case(given: &str) -> Result<CaseArg, String> {
    if let Some(names) = given.strip_prefix("hidden:") {
        let names: Vec<String> = names.split(',').map(|n| n.trim().to_string()).collect();
        if names.iter().any(String::is_empty) {
            return Err(
                "`hidden:` takes case model names, comma-separated: `hidden:<name>[,<name>]`"
                    .to_string(),
            );
        }
        return Ok(CaseArg::Hidden(names));
    }
    CaseName::from_str(given, true).map(CaseArg::Mode).map_err(|_| {
        format!(
            "`{given}` is not a case mode: use hidden, see-through or solid, or `hidden:<name>[,<name>]` for some case models only"
        )
    })
}

/// What a `--case` list says: the last mode given (if any), and every name of a `hidden:`.
pub fn fold_case(args: &[CaseArg]) -> (Option<CaseName>, Vec<String>) {
    let mut mode = None;
    let mut hidden: Vec<String> = Vec::new();
    for a in args {
        match a {
            CaseArg::Mode(m) => mode = Some(*m),
            CaseArg::Hidden(names) => {
                for n in names {
                    if !hidden.contains(n) {
                        hidden.push(n.clone());
                    }
                }
            }
        }
    }
    (mode, hidden)
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

    use crate::commands::{
        Board, Command, Dev, Import, Project, View, board3d, bom, build, case, catalog, check,
        common, compare, copper, drc, dxf, editor, explain, fab, fill, fmt, footprint, gerber,
        init, kicad_libs, layout, lint, lock, mcp, migrate_offers, nets, order, panel, parity,
        part, pcb, pins, port, power, project_show, render, route, routes, rules, schematic,
        schematic_layout, show, swap, system, view,
    };

    /// The command line as main.rs has it.
    #[derive(Debug, Parser)]
    #[command(name = "netsmith", version)]
    pub(crate) struct Cli {
        #[command(subcommand)]
        command: Command,
    }

    /// Every command that runs, by its own name, whatever group it is in: what the tests of a
    /// command's arguments compare a parse with.
    #[derive(Debug, PartialEq)]
    pub(crate) enum Leaf {
        Init(init::Args),
        Refresh(init::RefreshArgs),
        Lock(lock::Args),
        Lint(lint::Args),
        Fmt(fmt::Args),
        MigrateOffers(migrate_offers::Args),
        Order(order::Args),
        ProjectShow(project_show::Args),
        Check(check::Args),
        Build(build::Args),
        Fab(fab::Args),
        Drc(drc::Args),
        Rules(rules::Args),
        Explain(explain::Args),
        Compare(compare::Args),
        Pins(pins::Args),
        Swap(swap::Args),
        Show(show::Args),
        Bom(bom::Args),
        Power(power::Args),
        System(system::Args),
        Panel(panel::Args),
        Copper(copper::Args),
        Fill(fill::Args),
        Nets(nets::Args),
        Layout(layout::Args),
        Routes(routes::Args),
        SchematicLayout(schematic_layout::Args),
        Render(render::Args),
        Editor(editor::Args),
        View(view::Args),
        Schematic(schematic::Args),
        Pcb(pcb::Args),
        Gerber(gerber::Args),
        Catalog(catalog::Args),
        Dxf(dxf::ImportArgs),
        KicadLibs(kicad_libs::Args),
        Footprint(footprint::Args),
        Part(part::Args),
        Port(port::Args),
        Case(case::Args),
        Route(route::Args),
        Parity(parity::Args),
        Board3d(board3d::Args),
        Mcp(mcp::Args),
    }

    impl From<Command> for Leaf {
        fn from(command: Command) -> Leaf {
            match command {
                Command::Project(c) => match c {
                    Project::Init(a) => Leaf::Init(a),
                    Project::Refresh(a) => Leaf::Refresh(a),
                    Project::Lock(a) => Leaf::Lock(a),
                    Project::Lint(a) => Leaf::Lint(a),
                    Project::Fmt(a) => Leaf::Fmt(a),
                    Project::MigrateOffers(a) => Leaf::MigrateOffers(a),
                    Project::Order(a) => Leaf::Order(a),
                    Project::Show(a) => Leaf::ProjectShow(a),
                },
                Command::Board(c) => match c {
                    Board::Check(a) => Leaf::Check(a),
                    Board::Build(a) => Leaf::Build(a),
                    Board::Fab(a) => Leaf::Fab(a),
                    Board::Drc(a) => Leaf::Drc(a),
                    Board::Rules(a) => Leaf::Rules(a),
                    Board::Explain(a) => Leaf::Explain(a),
                    Board::Compare(a) => Leaf::Compare(a),
                    Board::Pins(a) => Leaf::Pins(a),
                    Board::Swap(a) => Leaf::Swap(a),
                    Board::Show(a) => Leaf::Show(a),
                    Board::Bom(a) => Leaf::Bom(a),
                    Board::Power(a) => Leaf::Power(a),
                    Board::System(a) => Leaf::System(a),
                    Board::Panel(a) => Leaf::Panel(a),
                    Board::Copper(a) => Leaf::Copper(a),
                    Board::Fill(a) => Leaf::Fill(a),
                    Board::Nets(a) => Leaf::Nets(a),
                },
                Command::Layout(a) => Leaf::Layout(a),
                Command::Routes(a) => Leaf::Routes(a),
                Command::Schematic(a) => Leaf::SchematicLayout(a),
                Command::View(c) => match c {
                    View::ThreeD(a) => Leaf::Render(a),
                    View::Editor(a) => Leaf::Editor(a),
                    View::Editor3d(a) => Leaf::View(a),
                    View::Schematic(a) => Leaf::Schematic(a),
                    View::Pcb(a) => Leaf::Pcb(a),
                    View::Gerber(a) => Leaf::Gerber(a),
                },
                Command::Catalog(a) => Leaf::Catalog(a),
                Command::Import(c) => match c {
                    Import::Dxf(a) => Leaf::Dxf(a),
                    Import::KicadLibs(a) => Leaf::KicadLibs(a),
                    Import::Footprint(a) => Leaf::Footprint(a),
                    Import::Part(a) => Leaf::Part(a),
                    Import::Port(a) => Leaf::Port(a),
                    Import::Case(a) => Leaf::Case(a),
                },
                Command::Dev(c) => match c {
                    Dev::Route(a) => Leaf::Route(a),
                    Dev::Parity(a) => Leaf::Parity(a),
                    Dev::Board3d(a) => Leaf::Board3d(a),
                },
                Command::Mcp(a) => Leaf::Mcp(a),
            }
        }
    }

    pub(crate) fn parse_args(args: &[&str]) -> Result<Command, clap::Error> {
        Cli::try_parse_from(std::iter::once(&"netsmith").chain(args)).map(|cli| cli.command)
    }

    /// The command `args` name, as its own [`Leaf`], or clap's error as text.
    pub(crate) fn parse(args: &[&str]) -> Result<Leaf, String> {
        parse_args(args).map(Leaf::from).map_err(|e| e.to_string())
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
