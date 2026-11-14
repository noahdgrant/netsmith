//! `netsmith fab`: from source text to the files a board house wants.
//!
//! For each board it builds the design, places and routes a PCB, writes the board and its
//! design rules, and packages Gerbers, a drill file, a bill of materials and a placement
//! file for JLCPCB. It refuses, with the reason, rather than write files that would not
//! make a working board: an unplaceable board, a design rules violation, or an assembled part
//! with no part number. A board the router could not finish is written marked INCOMPLETE
//! (`routing-failure.txt`, a first line of `fab-notes.txt`) so that its routed part can be
//! looked at, and the run still fails. The Gerbers, drill files, placement file and design
//! rules check are netsmith's own (ADR 0034, ADR 0037); KiCad is only the optional
//! cross-check.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use netsmith_check::{Loader, PartSpace, PinFile};
use netsmith_core::{PanelSpec, timings};
use netsmith_design::{Assembly, Sourced};
use netsmith_fab::{
    BomEntry, Fit, KicadCli, Options, Package, PanelBuildSpec, PanelCellFacts, PanelCellInput,
    Side, package,
};
use netsmith_kicad::Mapping;
use netsmith_pcb::{Board, Trim, kicad_pcb, kicad_pro, place_options, render_svg};
use netsmith_router::{RouteOptions, route_with};
use netsmith_solve::Catalogs;

use crate::order::{BoardFacts, PanelFacts};
use crate::pin_swap::{self, Score};
use netsmith_pipeline::resolve::resolve_generics;

use netsmith_pipeline::{Built, Libraries, outcome_with, render_problems};

/// One board, made.
#[derive(Debug)]
pub struct Fabricated {
    pub board: String,
    /// Where its files were written.
    pub dir: PathBuf,
    /// The Gerber and drill files inside the zip.
    pub gerber_files: Vec<String>,
    /// What was decided, for a person to read.
    pub summary: String,
    /// Things to check before ordering.
    pub notes: String,
    pub files: Vec<PathBuf>,
    /// The numbers an order's summary is made of.
    pub facts: BoardFacts,
    /// Where each part's footprint origin is and how far it is from its body's centre (#457);
    /// none for a panel.
    pub origins: Vec<netsmith_pcb::PartOrigin>,
}

/// What `netsmith fab` says when it finds no KiCad, and why there is none: KiCad is switched off
/// (`NETSMITH_NO_KICAD`, which hides an install that may be there) or it was not found. netsmith's
/// own design rules check and its Gerbers need none; KiCad is only the witness of
/// `--cross-check`, which plots the boards and runs KiCad's design rules check to compare with
/// netsmith's own (ADR 0034, ADR 0037). `scripts/check-no-kicad.sh` greps for either lead
/// ("KiCad is switched off", "KiCad is not installed") as the proof that KiCad was hidden.
pub fn no_kicad_note(search: &netsmith_kicad::Search) -> String {
    let why = if search.is_off() {
        format!(
            "KiCad is switched off ({} is set)",
            netsmith_kicad::NO_KICAD_VARIABLE
        )
    } else {
        "KiCad is not installed".to_string()
    };
    format!(
        "note: {why}, so --cross-check (KiCad's plots and design rules check, compared with netsmith's own) is not available; netsmith's own design rules check still runs"
    )
}

/// Why there is no `kicad-cli` when KiCad is switched off (`NETSMITH_NO_KICAD`), for the message of
/// a command that needs one: the search then finds none whatever `KICAD_CLI` says, so a message
/// that says "set KICAD_CLI" would send a person to a setting that changes nothing. `None` when
/// KiCad is not switched off.
pub fn kicad_switched_off(search: &netsmith_kicad::Search) -> Option<String> {
    search.is_off().then(|| {
        format!(
            "{} is set, which hides every KiCad program and install (kicad-cli too, whatever {} says): unset it to use KiCad",
            netsmith_kicad::NO_KICAD_VARIABLE,
            netsmith_kicad::CLI_VARIABLE
        )
    })
}

/// Build, place, route and package every board in one source file. A system's boards each
/// get a package of their own, in `<out>/<System>/<board>/`. On failure the error is every
/// problem found.
pub fn fab(
    name: &str,
    text: &str,
    out: &Path,
    catalogs: &Catalogs,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
) -> Result<Vec<Fabricated>, String> {
    fab_with(name, text, &Loader::std(), out, catalogs, libraries, kicad)
}

/// How long the router may work on a board unless told otherwise.
pub const DEFAULT_ROUTE_TIMEOUT: Duration = Duration::from_secs(300);

/// Whether constraints were asked to be strict: `--constraints error` for this run
/// (`flag == Some(false)`, `Some(true)` being `warn`), else `constraints = "error"` in the
/// manifest. Unset is not strict here, though a violated requirement is an error either way
/// (ADR 0045, "Height limits").
pub fn constraints_strict(manifest: Option<&str>, flag: Option<bool>) -> bool {
    match flag {
        Some(warn) => !warn,
        None => manifest == Some("error"),
    }
}

/// How `fab` goes about it.
#[derive(Debug, Clone)]
pub struct FabOptions {
    /// After this long routing a board, stop and report the connections left. `None` is no
    /// limit.
    pub route_timeout: Option<Duration>,
    /// Told which stage `fab` is in and how far the router has got, so that a long run is
    /// not silent. The command line prints these to stderr.
    pub progress: Option<fn(&str)>,
    /// The layout file of the source (ADR 0020 item 7), which is put on each board after it
    /// is placed. Optional: a file that does not exist changes nothing.
    pub layout: Option<PathBuf>,
    /// Package a board although netsmith's design rules check finds errors. The notes say
    /// so. Nothing else needs this: the check runs without KiCad.
    pub allow_drc_violations: bool,
    /// After the files are written, plot each board with KiCad too and compare the two sets
    /// (ADR 0034), and run KiCad's design rules check and compare its findings with
    /// netsmith's (ADR 0037). Needs KiCad; a difference is an error.
    pub cross_check: bool,
    /// Make only the boards with these labels (`Unit`, `DevKit/carrier`) and skip the rest of
    /// the file. `None` makes every board. An order uses it (ADR 0035).
    pub only: Option<Vec<String>>,
    /// Constraints were asked to be strict (`fab --constraints error`, or `constraints =
    /// "error"` in the manifest, and not the default): a part under a height limit that states
    /// no height is then an error and not a warning (ADR 0045, "Height limits").
    /// `--allow-drc-violations` does not waive it.
    pub strict_heights: bool,
    /// The pick lock of the source (`pick_lock`): written when it does not exist, compared with
    /// the picks when it does, and with `locked` an error if they differ, before anything is
    /// placed. The solver holds parts to it through the catalogs, which the caller gives. `None`
    /// leaves the lock out of the run.
    pub lock: Option<netsmith_pipeline::pick_lock::Use>,
    /// What `fab` does with the pins that pin requests got when a board does not route (ADR 0101,
    /// `pin_swap`). `None` leaves the pins alone: a board that does not route is INCOMPLETE as
    /// before. The command line gives `Some` unless an order is made.
    pub pins: Option<PinsUse>,
    /// The name of the source file as the pin map for firmware names it (`<board>.pins.h` and
    /// `.pins.json`, which every board with pin requests gets in its output folder): the path
    /// from the project's root. `None` names the file as `fab` was given it.
    pub pin_map_source: Option<String>,
    /// How many times the process's least track the router draws a signal at, where there is
    /// room (`track_margin` in `netsmith.toml`): a board it leaves unfinished at that width is
    /// routed again at the least, so the margin is kept where it fits and given up where it does
    /// not (issue #431). 1 is the least.
    pub track_margin: f64,
    /// Write the board's 3D scene as `<board>.glb` into the board's package folder, beside the
    /// pin map and `fab-notes.txt` and outside the Gerber zip: the scene of the board as routed,
    /// as `board3d --glb` writes it (docs/BOARD3D.md). The default is `true`; the command line
    /// gives `false` for `--no-glb` or `glb = false` in `netsmith.toml`. When it is `false`, a
    /// `<board>.glb` that an earlier run left in the folder is removed, as the pin map's files
    /// are, so that the folder never holds a scene of another layout.
    pub glb: bool,
    /// Write the board as placed, just before it is routed, as `<board>.placed.json` into the
    /// board's output folder (`fab --save-placed`), for `netsmith route` to route again with no
    /// placing. The default is `false`.
    pub save_placed: bool,
    /// Write the board as STEP, `<board>.step`, into the board's package folder beside the
    /// `.glb` and outside the Gerber zip, as `board3d --step` writes it (ADR 0107,
    /// docs/BOARD3D.md). The default is `true`; the command line gives `false` for `--no-step` or
    /// `step = false` in `netsmith.toml`. When it is `false`, a `<board>.step` that an earlier run
    /// left in the folder is removed, as the `.glb` is.
    pub step: bool,
    /// Where the files of the target are, to find the STEP models its parts are drawn from
    /// (ADR 0108) and its case models (ADR 0109): a path outside the project stops `fab`, a
    /// file that is not there is a warning (a case model's, an error with `strict_heights`).
    /// `None` finds none, and every part is drawn with its generated shape.
    pub models: Option<crate::models::Places>,
    /// How many seeds the placer tries for a board, from the layout file's (`place_seeds` in
    /// `netsmith.toml`, issue #458; [`crate::layout::DEFAULT_PLACE_SEEDS`] when it does not
    /// say).
    pub place_seeds: u32,
    /// Whether a seed that places the board and is not the layout file's may be written to the
    /// file (issue #458): `fab` writes it; an order and the editor's release never change a
    /// choice by themselves, and stop instead.
    pub seed_writes: SeedWrites,
}

/// What a run may do with a seed that places the board and is not the layout file's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SeedWrites {
    /// Write it to the layout file (`fab`).
    #[default]
    Write,
    /// Stop with an error that says the seed: an order (`fab --order`) is a release.
    Order,
    /// Stop with an error that says the seed: the editor's release is a release.
    EditorRelease,
}

/// Whether `fab` may move pins by itself, and where the pin file is.
#[derive(Debug, Clone)]
pub struct PinsUse {
    pub mode: PinsMode,
    /// The pin file beside the source (`<stem>.pins.toml`), which `auto` rewrites.
    pub file: PathBuf,
    /// The clock the pin optimizer's time budget runs on: the system's, unless a test moves its
    /// own.
    pub clock: pin_swap::Clock,
    /// What made the mode `locked`, for the messages: they name what the person gave and never
    /// advise dropping a flag they did not pass. [`LockedBy::PinsFlag`] unless told.
    pub locked_by: LockedBy,
}

/// What a person gave that makes [`PinsMode::Locked`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LockedBy {
    /// `--pins locked`.
    #[default]
    PinsFlag,
    /// `--locked`, which implies it.
    LockedFlag,
    /// `pins = "locked"` in `netsmith.toml`.
    Manifest,
}

impl LockedBy {
    /// What was given, as the messages say it.
    fn given(self) -> &'static str {
        match self {
            LockedBy::PinsFlag => "--pins locked",
            LockedBy::LockedFlag => "--locked",
            LockedBy::Manifest => "`pins = \"locked\"` in netsmith.toml",
        }
    }

    /// How to let the pins move: drop what was given, or the flag that wins over it.
    fn way_out(self) -> &'static str {
        match self {
            LockedBy::PinsFlag => "run without --pins locked",
            LockedBy::LockedFlag => "run without --locked",
            LockedBy::Manifest => {
                "run with --pins auto (or set `pins = \"auto\"` in netsmith.toml)"
            }
        }
    }
}

impl PinsUse {
    /// The mode and the file, on the system clock.
    pub fn new(mode: PinsMode, file: PathBuf) -> PinsUse {
        PinsUse {
            mode,
            file,
            clock: pin_swap::Clock::real(),
            locked_by: LockedBy::default(),
        }
    }
}

/// `--pins`: what a routing failure may do to the pin map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinsMode {
    /// Swap interchangeable pins, place and route again, and write the pin file.
    Auto,
    /// Change nothing. Write the swaps that would help to `pins.proposed.toml` in the output
    /// folder, and fail as before.
    Locked,
}

impl PinsMode {
    /// `"auto"` or `"locked"`, as `--pins` and the manifest spell it.
    pub fn parse(word: &str) -> Option<PinsMode> {
        match word {
            "auto" => Some(PinsMode::Auto),
            "locked" => Some(PinsMode::Locked),
            _ => None,
        }
    }
}

impl Default for FabOptions {
    fn default() -> Self {
        Self {
            route_timeout: Some(DEFAULT_ROUTE_TIMEOUT),
            progress: None,
            layout: None,
            allow_drc_violations: false,
            cross_check: false,
            only: None,
            strict_heights: false,
            lock: None,
            pins: None,
            pin_map_source: None,
            track_margin: DEFAULT_TRACK_MARGIN,
            glb: true,
            save_placed: false,
            step: true,
            models: None,
            place_seeds: crate::layout::DEFAULT_PLACE_SEEDS,
            seed_writes: SeedWrites::Write,
        }
    }
}

/// How many times the process's least track a signal is drawn at where there is room, when the
/// project does not say (`track_margin` in `netsmith.toml`, issue #431): JLCPCB's page says of its
/// 0.1 mm least "stay well above if possible", and 1.5 times it is 0.15 mm.
pub const DEFAULT_TRACK_MARGIN: f64 = 1.5;

/// [`fab`], with imports resolved by `loader`.
pub fn fab_with(
    name: &str,
    text: &str,
    loader: &Loader,
    out: &Path,
    catalogs: &Catalogs,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
) -> Result<Vec<Fabricated>, String> {
    fab_with_options(
        name,
        text,
        loader,
        out,
        catalogs,
        libraries,
        kicad,
        &FabOptions::default(),
    )
}

/// [`fab_with`], with a choice of the router's time limit and a way to follow progress.
#[allow(clippy::too_many_arguments)]
pub fn fab_with_options(
    name: &str,
    text: &str,
    loader: &Loader,
    out: &Path,
    catalogs: &Catalogs,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
    options: &FabOptions,
) -> Result<Vec<Fabricated>, String> {
    fab_detailed(name, text, loader, out, catalogs, libraries, kicad, options).map_err(|f| f.text)
}

/// Why a fab run stopped: the text the command line prints, and the problems of the build when
/// it was the build that stopped it (for `--format json`, #455). A failure of a later step (the
/// placer, the router, the design rules check, a file) has the text alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FabFailure {
    pub text: String,
    pub problems: Vec<netsmith_pipeline::Problem>,
}

impl From<String> for FabFailure {
    fn from(text: String) -> Self {
        FabFailure {
            text,
            problems: Vec::new(),
        }
    }
}

/// [`fab_with_options`], with the problems of a build that stops it kept as data.
#[allow(clippy::too_many_arguments)]
pub fn fab_detailed(
    name: &str,
    text: &str,
    loader: &Loader,
    out: &Path,
    catalogs: &Catalogs,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
    options: &FabOptions,
) -> Result<Vec<Fabricated>, FabFailure> {
    let say = |message: String| {
        if let Some(progress) = options.progress {
            progress(&message);
        }
    };
    let mut made = Vec::new();
    let outcome =
        outcome_with(name, text, loader, catalogs, Some(libraries)).map_err(|problems| {
            FabFailure {
                text: render_problems(&problems),
                problems,
            }
        })?;
    if let Some(using) = &options.lock {
        let settled =
            netsmith_pipeline::pick_lock::settle(using, catalogs.pick_lock(), &outcome.lock)
                .map_err(|e| format!("{e}\n"))?;
        if let Some(file) = settled.wrote {
            say(format!("wrote {}", file.display()));
        }
        for note in settled.notes {
            say(format!("note: {note}"));
        }
    }
    let mut pin_state = PinState::new(&outcome.pins);
    let (mut built_boards, panels, systems) = (outcome.boards, outcome.panels, outcome.systems);
    // The STEP models parts are drawn from and the case models, found on disk (ADR 0108,
    // ADR 0109).
    if let Some(places) = &options.models {
        let mut errors = String::new();
        for built in &mut built_boards {
            let label = built.label();
            let found = crate::models::locate(&mut built.design, places);
            for warning in found.warnings {
                say(format!("warning: {label}: {warning}"));
            }
            for error in found.errors {
                errors.push_str(&format!("error: {label}: {error}\n"));
            }
            // A case model that is not there leaves the fit unchecked (ADR 0109): an error
            // when constraints are strict, as a part of unknown height under a limit is.
            for unchecked in found.unchecked {
                if options.strict_heights {
                    errors.push_str(&format!(
                        "error: {label}: {unchecked}; this is an error because constraints are strict (`--constraints error` or `constraints = \"error\"`), and `--allow-drc-violations` does not waive it: add the file, or run with `--constraints warn`\n"
                    ));
                } else {
                    say(format!("warning: {label}: {unchecked}"));
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors.into());
        }
    }
    let board_names: Vec<String> = built_boards.iter().map(|b| b.design.name.clone()).collect();
    // The layout file beside the source, if there is one: it says which face each part is on
    // and where things are, and a wrong entry stops fab before anything is placed.
    let layout = match &options.layout {
        Some(file) => {
            crate::layout::read(file, &board_names).map_err(|e| format!("error: {name}: {e}\n"))?
        }
        None => None,
    };
    // The routes file beside it is read now too, so a wrong one stops fab before anything is
    // placed; each board puts its table on after it is placed.
    if let Some(file) = &options.layout {
        crate::routes::read(&crate::routes::beside_layout(file), &board_names)
            .map_err(|e| format!("error: {name}: {e}\n"))?;
    }
    // Two requests that are one signal in the C header of the pin map are an error, said now
    // with every other check of the source and before any board is placed or written.
    let mut collisions = String::new();
    for built in &built_boards {
        let source = options.pin_map_source.as_deref().unwrap_or(name);
        let spaces = pin_state.spaces_of(built);
        if let Err(message) = crate::pin_export::render(&built.design, spaces, source) {
            collisions.push_str(&format!("{message}\n"));
        }
    }
    if !collisions.is_empty() {
        return Err(collisions.into());
    }
    // Each board as made, routed, by label: a system's scene is made of its members'.
    let mut routed: BTreeMap<String, Board> = BTreeMap::new();
    for built in &built_boards {
        if let Some(only) = &options.only
            && !only.contains(&built.label())
        {
            continue;
        }
        let run = fab_board_pinned(
            &Pass { name, text, loader },
            built,
            &mut pin_state,
            out,
            &layout,
            libraries,
            kicad,
            options,
            catalogs,
            &say,
        )?;
        routed.insert(built.label(), run.board);
        made.push(run.fabricated);
    }
    // The system scene of each system whose boards were all made (ADR 0110): checked across
    // boards, and written beside them.
    for system in &systems {
        let members = crate::system3d::members(system, &built_boards);
        let boards: Option<Vec<(&Built, &Board)>> = members
            .iter()
            .map(|b| routed.get(&b.label()).map(|board| (*b, board)))
            .collect();
        let Some(boards) = boards else { continue };
        fab_system(system, &boards, out, options, &say)?;
    }
    // The panels of the file (ADR 0040): each repeats a board of it, which is made again, in
    // the panel's own folder.
    for panel in &panels {
        if let Some(only) = &options.only
            && !only.contains(&panel.name)
        {
            continue;
        }
        let fail = |what: String| format!("error: panel {}: {what}\n", panel.name);
        let built = built_boards
            .iter()
            .find(|b| b.system.is_none() && b.board == panel.cell)
            .ok_or_else(|| fail(format!("it repeats `{}`, which is not built", panel.cell)))?;
        let dir = out.join(&panel.name);
        say(format!(
            "{}: making {}, its board, first",
            panel.name, panel.cell
        ));
        let run = fab_board_pinned(
            &Pass { name, text, loader },
            built,
            &mut pin_state,
            &dir.join("cell"),
            &layout,
            libraries,
            kicad,
            options,
            catalogs,
            &say,
        )?;
        say(format!(
            "{}: {} x {} cells of {}",
            panel.name, panel.columns, panel.rows, panel.cell
        ));
        made.push(fab_panel(panel, &run, &dir, kicad, options, &say)?);
    }
    Ok(made)
}

/// What the requirements prove about a board before any placing (ADR 0045 section 14): lines
/// that cannot all hold, a courtyard past an edge nobody stated, a part
/// bigger than the outline. It takes milliseconds and starts no placer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrecheckReport {
    /// The board's label.
    pub board: String,
    /// What is proved, each as a message with the numbers and the lines.
    pub errors: Vec<String>,
    /// What is only suspicious.
    pub warnings: Vec<String>,
}

/// The pre-check of one built board with the layout file's locks, if any. A board the
/// libraries cannot give footprints for (no KiCad install, a part with no mapping) has nothing
/// proved here: `build` has never needed them, and `fab` reports that when it places.
pub fn precheck_board(
    built: &Built,
    layout: Option<&netsmith_pcb::Layout>,
    libraries: &Libraries,
    constraints_warn: bool,
) -> PrecheckReport {
    let mut report = PrecheckReport {
        board: built.label(),
        ..PrecheckReport::default()
    };
    let mut placement = place_options(&built.design, layout);
    placement.lines_warn = constraints_warn;
    match netsmith_placer::precheck(
        &built.design,
        &libraries.mapping,
        &libraries.footprints,
        &placement,
    ) {
        Ok(found) => {
            report.errors = found.errors();
            report.warnings = found.warnings;
        }
        Err(e @ netsmith_pcb::BoardError::Constraints(_)) => report.errors = vec![e.to_string()],
        Err(_) => {}
    }
    report
}

/// The file in the board's fab folder that lists every part's final place and state when
/// the parts do not fit (PROVISIONAL, ADR 0033).
const PLACEMENT_FAILURE: &str = "placement-failure.txt";

/// The refusal's message, with the full list written to `placement-failure.txt` in `out` and
/// named; if it cannot be written, the message says so and carries the list.
fn placement_failure(e: &netsmith_pcb::DoesNotFit, out: &Path) -> String {
    let file = out.join(PLACEMENT_FAILURE);
    let written =
        std::fs::create_dir_all(out).and_then(|()| std::fs::write(&file, e.full_report()));
    match written {
        Ok(()) => format!(
            "{e}\n  every part's final place and state ({} parts) is in {}",
            e.parts.len(),
            file.display()
        ),
        Err(err) => format!(
            "{e}\n  (the list of every part's final place and state could not be written to {}: {err})",
            file.display()
        ),
    }
}

/// The requirements of the physical file that the arrangement the placer ended on does not
/// meet, as the message of the first error that `fab` prints for it, with the list also
/// written to `placement-failure.txt` in `out` with the faults and every part's place.
fn requirement_failure(
    report: &netsmith_pcb::RequirementReport,
    problems: &[String],
    board: &Board,
    out: &Path,
) -> String {
    let mut text = format!("{report}\n");
    if !problems.is_empty() {
        text.push_str("\nThe faults the placement check found where the placer ended:\n");
        for p in problems {
            text.push_str(&format!("  {p}\n"));
        }
    }
    text.push_str(
        "\nEvery part where the placer ended (the origin of each footprint is in mm from the outline's top left corner, y down):\n",
    );
    for p in &board.parts {
        text.push_str(&format!(
            "{} ({}) {} at ({:.2}, {:.2}) turned {}\n",
            netsmith_pcb::board_member(&p.path, &p.refdes),
            p.kind,
            p.side.word(),
            p.at.x - board.outline.min.x,
            p.at.y - board.outline.min.y,
            p.rotation
        ));
    }
    let file = out.join(PLACEMENT_FAILURE);
    match std::fs::create_dir_all(out).and_then(|()| std::fs::write(&file, text)) {
        Ok(()) => format!(
            "{report}\n  every part's final place{} is in {}",
            if problems.is_empty() {
                ""
            } else {
                " and the faults that follow"
            },
            file.display()
        ),
        Err(err) => format!(
            "{report}\n  (the list could not be written to {}: {err})",
            file.display()
        ),
    }
}

/// What a seed after the layout file's that placed the board does (issue #458): it is said,
/// with the seed it replaces (stale, if the file states it) and what blocked each seed tried,
/// and written to the layout file as the board's seed, so that the next run starts from it.
/// With `--locked` nothing may be written, so a seed that changed is an error, as a pick lock
/// that differs is. A run with no layout file to name (`FabOptions::layout`) only says it.
fn settle_seed(
    (label, board): (&str, &str),
    seeds: &crate::layout::Seeds,
    layout: &LayoutFile,
    options: &FabOptions,
    say: &dyn Fn(String),
) -> Result<(), String> {
    let fail = |what: String| format!("error: {label}: {what}\n");
    let file = options
        .layout
        .as_ref()
        .map(|f| f.display().to_string())
        .or_else(|| layout.as_ref().map(|(_, name)| name.clone()));
    let name = file
        .clone()
        .unwrap_or_else(|| "the layout file".to_string());
    let Some(note) = seeds.note(&name) else {
        return Ok(());
    };
    let locked = options.lock.as_ref().is_some_and(|l| l.locked);
    let release = match options.seed_writes {
        SeedWrites::Write => None,
        SeedWrites::Order => Some("an order"),
        SeedWrites::EditorRelease => Some("a release from the editor"),
    };
    if let (Some(what), Some(_)) = (release, &options.layout) {
        return Err(fail(format!(
            "{note}, and {what} is a release, which never changes a choice by itself: run `netsmith fab` on the source to write `seed = {}` to {name}, review it and commit it, then make the release again",
            seeds.used
        )));
    }
    match (&options.layout, locked) {
        (Some(_), true) => Err(fail(format!(
            "{note}, and --locked says {name} must not be written: run without --locked to write `seed = {}` to it, and commit it",
            seeds.used
        ))),
        (Some(path), false) => {
            crate::layout::write_seed(path, board, seeds.used).map_err(fail)?;
            say(format!("{label}: note: {note}, and is written to {name}"));
            Ok(())
        }
        (None, _) => {
            say(format!("{label}: note: {note}"));
            Ok(())
        }
    }
}

/// One board, made: placed, routed, written and packaged, with the work's results kept for
/// a panel that repeats it.
struct BoardRun {
    fabricated: Fabricated,
    board: Board,
    package: Package,
    entries: Vec<BomEntry>,
    fit: BTreeMap<String, Fit>,
    pcb: PathBuf,
    options: Options,
}

type LayoutFile = Option<(netsmith_pcb::Layout, String)>;

#[allow(clippy::too_many_arguments)]
fn fab_board(
    built: &Built,
    out: &Path,
    layout: &LayoutFile,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
    options: &FabOptions,
    catalogs: &Catalogs,
    say: &dyn Fn(String),
    stuck: &mut Option<Stuck>,
    pin_notes: &[String],
    pin_map_of: &PinMapOf,
) -> Result<BoardRun, String> {
    {
        let fail = |what: String| format!("error: {}: {what}\n", built.label());
        let name = &built.design.name;
        // The pin map for firmware is of the pins THIS pass was made with, which are the pins
        // of the board it writes, so a pass whose files are kept or put back (see
        // `fab_board_pinned`) leaves the map of its own board beside it. A signal name that
        // collides in C is an error here, before anything is placed.
        let pin_map = pin_map_of.render(built)?;

        // A generic part (ADR 0047) becomes the row that stands for it now; one that no row
        // stands for cannot be ordered, and is refused by name with the nearest rows.
        let label = built.label();
        let (sourcing, unresolved) = timings::stage("fab.resolve", &label, || {
            resolve_generics(&built.sourcing, catalogs)
        });
        let unresolved: Vec<String> = unresolved
            .iter()
            .filter(|u| {
                built
                    .sourcing
                    .iter()
                    .any(|s| s.refdes == u.refdes && s.needs_part_number())
            })
            .map(|u| fail(u.line(&built.rechecks, catalogs)))
            .collect();
        // Every part to assemble needs a supplier part number. All that lack one are named
        // together, now and not after the routing, with what to write to say that the board
        // house is not to assemble them.
        let unnumbered = crate::no_supplier::find(built, &sourcing);
        if !unnumbered.is_empty() || !unresolved.is_empty() {
            let mut errors = unresolved.concat();
            if !unnumbered.is_empty() {
                errors.push_str(&fail(crate::no_supplier::message(
                    &built.board,
                    &unnumbered,
                )));
            }
            return Err(errors);
        }

        // The generic parts are judged against the board's environment with the rows they were
        // resolved to: a row outside it would have been ordered, so nothing is placed (ADR 0099).
        let environment = crate::fab_environment::judge_resolved(built, &sourcing)
            .map_err(|errors| errors.into_iter().map(fail).collect::<String>())?;

        // What the requirements prove cannot be placed is said now, before the placer starts
        // (it can take a minute on a tight board).
        let precheck = timings::stage("fab.precheck", &label, || {
            precheck_board(
                built,
                layout.as_ref().map(|l| &l.0),
                libraries,
                catalogs.constraints_warn(),
            )
        });
        for warning in &precheck.warnings {
            say(format!("{}: warning: {warning}", built.label()));
        }
        if !precheck.errors.is_empty() {
            return Err(precheck.errors.into_iter().map(fail).collect());
        }
        say(format!(
            "{}: placing {} parts",
            built.label(),
            built.design.parts.len()
        ));
        let placing = timings::stage("fab.place", &label, || {
            crate::layout::place(
                &built.design,
                layout.as_ref(),
                libraries,
                catalogs.constraints_warn(),
                options.place_seeds,
            )
        })
        .map_err(|e| match e {
            crate::layout::PlaceFailure::Board(netsmith_pcb::BoardError::DoesNotFit(e)) => {
                fail(placement_failure(&e, out))
            }
            crate::layout::PlaceFailure::Unplaced { error, seeds } => fail(format!(
                "{}\n{}",
                seeds.report(),
                placement_failure(&error, out)
            )),
            crate::layout::PlaceFailure::Board(e) => fail(e.to_string()),
            crate::layout::PlaceFailure::Layout(e) => fail(e),
        })?;
        let crate::layout::Placing {
            mut board,
            options: placement,
            seeds,
        } = placing;
        settle_seed(
            (label.as_str(), name.as_str()),
            &seeds,
            layout,
            options,
            say,
        )?;
        let problems: Vec<String> = board
            .check_placement()
            .iter()
            .map(ToString::to_string)
            .collect();
        // When the placer could not meet the requirements of the physical file, the faults
        // above are its symptoms: say first which lines are not met and which cannot all hold,
        // and keep the same list in `placement-failure.txt` (ADR 0045).
        let unmet = board.explain_requirements(&built.design, &placement);
        // No seed placed the board: say so first, with what blocks the closest try, which is
        // the board whose faults follow (issue #458).
        let no_seed = (!seeds.placed).then(|| fail(seeds.report()));
        if !problems.is_empty() {
            let mut errors = no_seed.clone().unwrap_or_default();
            if let Some(report) = &unmet {
                errors.push_str(&fail(requirement_failure(report, &problems, &board, out)));
            }
            errors.extend(problems.iter().map(|p| fail(p.clone())));
            return Err(errors);
        }
        // The placer's own arrangement, with no layout file to move parts after it, that does
        // not meet a requirement would be found by the check after routing; say so now, with
        // the same list, unless constraints are only warnings for this run. (With a layout
        // file the positions are the file's, and the check after routing measures those.)
        if layout.is_none()
            && let Some(report) = unmet.filter(|r| !r.unmet.is_empty())
            && !catalogs.constraints_warn()
        {
            return Err(
                no_seed.unwrap_or_default() + &fail(requirement_failure(&report, &[], &board, out))
            );
        }
        for hang in board.hangs() {
            say(format!("{}: warning: {hang}", built.label()));
        }
        // Two parts where an imported KiCad board has them may share courtyard area: that
        // board was made with its own footprints (`Board::imported`).
        for overlap in board.imported_overlaps() {
            say(format!(
                "{}: warning: {overlap}: both stand where the imported KiCad board has them (`source = \"import\"` in the layout file), which was made with its own footprints",
                built.label()
            ));
        }
        // A part under a height limit that states no height is not a pass (ADR 0045,
        // "Height limits"): it is named, here and in the notes.
        let unchecked = board.heights_unchecked();
        if options.strict_heights && !unchecked.is_empty() {
            return Err(unchecked
                .iter()
                .map(|u| {
                    fail(format!(
                        "{u}; this is an error because constraints are strict (`--constraints error` or `constraints = \"error\"`), and `--allow-drc-violations` does not waive it: state the height, or run with `--constraints warn`"
                    ))
                })
                .collect());
        }
        for u in &unchecked {
            say(format!("{}: warning: {u}", built.label()));
        }
        // A pass that rests on a package standard's height is said to be one (ADR 0106).
        for pass in board.standard_passes() {
            say(format!("{}: note: {pass}", built.label()));
        }
        // The board's fit in its case models (ADR 0109), measured on the placement before
        // routing: a collision stops `fab` whatever the flags say, an unchecked fit does when
        // constraints are strict, the rest are warnings. A file that is not there was said by
        // `models::locate` above; it is in the notes too.
        let fit = if built.design.features.case_models.is_empty() {
            netsmith_3d::CaseScene::default()
        } else {
            timings::stage("fab.case", &label, || {
                crate::board3d::scene(built, &board).case
            })
        };
        let refused: Vec<String> = fit
            .findings
            .iter()
            .filter(|f| f.severity(options.strict_heights) == "error")
            .map(|f| fail(case_refusal(f)))
            .collect();
        if !refused.is_empty() {
            return Err(refused.concat());
        }
        for f in fit
            .findings
            .iter()
            .filter(|f| !matches!(f.why, netsmith_3d::CaseWhy::Missing { .. }))
        {
            say(format!(
                "{}: warning: {}: {f}",
                built.label(),
                f.kind.word()
            ));
        }
        // A trim is a step a person does with a cutter. The statement stays a warning for as
        // long as it stands (ADR 0045 section 15).
        for trim in board.trims() {
            say(format!(
                "{}: warning: the design states a trim: {}",
                built.label(),
                trim.note()
            ));
            for u in trim.unchecked() {
                say(format!("{}: warning: {u}", built.label()));
            }
        }
        say(format!(
            "{}: placed on a {} by {} mm board with seed {}",
            built.label(),
            board.outline.width(),
            board.outline.height(),
            seeds.used
        ));
        // The copper the routes file fixes, before the router (ADR 0030); what of it does not
        // match the board any more is said, not used.
        for note in
            crate::routes::apply_beside(&mut board, options.layout.as_deref()).map_err(fail)?
        {
            say(format!("{}: warning: {note}", built.label()));
        }
        // The board as placed, for the pin optimizer if the router leaves connections.
        let placed = options.pins.is_some().then(|| board.clone());
        // `--save-placed`: the board as placed, for `netsmith route`; without it, one an earlier
        // run left is removed, so the folder never holds a placed board of another layout.
        if let Some(file) = crate::placed::save_or_remove(
            &built.dir(out),
            &board,
            options.track_margin,
            options.save_placed,
        )
        .map_err(fail)?
        {
            say(format!(
                "{}: saved the placed board to {}",
                built.label(),
                file.display()
            ));
        }
        let routing = timings::stage("fab.route", &label, || {
            route_with_margin(
                &mut board,
                options.route_timeout,
                RouteOptions {
                    progress: options.progress,
                    ..RouteOptions::default()
                },
                options.track_margin,
                say,
            )
        });
        // A diff pair whose meanders could not match it says the skew it keeps (issue #431, S11).
        for p in &routing.pairs {
            if p.after > netsmith_router::meander::SKEW_TARGET {
                say(format!(
                    "{}: diff pair {}/{} keeps a skew of {:.2} mm (from {:.2} mm): no straight run of its shorter net has room for more meander",
                    built.label(),
                    p.p,
                    p.n,
                    p.after,
                    p.before
                ));
            }
        }
        let unrouted = routing.left;
        if let (false, Some(placed)) = (unrouted.is_empty(), placed) {
            *stuck = Some(Stuck {
                left: unrouted.len(),
                crossings: Score::of(&placed).crossings,
                placed,
            });
        }
        // A board with connections left is still written, so that the owner can open the part
        // that is routed in the PCB view, but it is marked INCOMPLETE and the run ends in an
        // error (see `write_incomplete`).
        let incomplete = (!unrouted.is_empty()).then(|| {
            let mut msg = format!("{} connections could not be routed", unrouted.len());
            match (routing.stopped, options.route_timeout) {
                (true, Some(limit)) => msg.push_str(&format!(
                    " (stopped after the route timeout of {} s, on try {} of the router; raise it with --route-timeout)",
                    limit.as_secs(),
                    routing.tries
                )),
                _ => msg.push_str(&format!(
                    " (after {} tries with the lines in different orders, ripping up the nets in the way)",
                    routing.tries
                )),
            }
            msg.push(':');
            for line in &unrouted.lines {
                msg.push_str(&format!("\n  {line}"));
            }
            Incomplete {
                message: msg,
                report: routing_failure(
                    &built.label(),
                    board.outline.min,
                    &unrouted,
                    routing.tries,
                    &routing.why,
                ),
                json: routing_report(&routing.why, &routing.crowding),
                lines: unrouted
                    .lines
                    .iter()
                    .map(|line| format!("ERROR: not routed: {line}"))
                    .collect(),
            }
        });
        if let Some(incomplete) = &incomplete {
            say(format!(
                "{}: INCOMPLETE: {} connections are not routed; writing the board and the Gerbers anyway, marked incomplete",
                built.label(),
                incomplete.lines.len()
            ));
        } else {
            say(format!("{}: routed, writing files", built.label()));
        }

        let dir = built.dir(out);
        let pcb_path = dir.join(format!("{name}.kicad_pcb"));
        fs::create_dir_all(&dir).map_err(|e| fail(format!("{}: {e}", dir.display())))?;
        for (file, contents) in [
            (pcb_path.clone(), kicad_pcb(&board)),
            (dir.join(format!("{name}.kicad_pro")), kicad_pro(&board)),
            (dir.join(format!("{name}.svg")), render_svg(&board)),
        ] {
            fs::write(&file, contents).map_err(|e| fail(format!("{}: {e}", file.display())))?;
        }

        // The pin map for firmware, beside the board: written for an incomplete board too,
        // whose pins are the ones it was routed with. A board with no request has none, and
        // the two files an earlier run left are removed.
        let pin_files = crate::pin_export::write(&dir, name, pin_map.as_ref())
            .map_err(|e| fail(format!("{}: {e}", dir.display())))?;

        // The board's 3D scene, beside the other files that are not for the board house. A failure
        // to write it is a note, never a failed fab: the package does not need it.
        // One scene for both, its parts drawn from their STEP models (ADR 0108).
        let scene = (options.glb || options.step).then(|| crate::board3d::scene(built, &board));
        for w in scene.iter().flat_map(|s| &s.warnings) {
            say(format!("{}: warning: {w}", built.label()));
        }
        let (glb_file, glb_note) = write_glb(&dir, name, built, scene.as_ref(), options.glb);
        if let Some(note) = &glb_note {
            say(format!("{}: warning: {note}", built.label()));
        }

        // The board as STEP, the same way: a failure is a note, never a failed fab.
        let (step_file, step_note) = write_step(&dir, name, built, scene.as_ref(), options.step);
        if let Some(note) = &step_note {
            say(format!("{}: warning: {note}", built.label()));
        }

        // The project's own libraries and the standard library's own files that the board
        // uses, so KiCad finds the footprints.
        libraries
            .write_tables(&dir, &built.design)
            .map_err(|e| fail(format!("{}: {e}", dir.display())))?;

        let entries = bom_entries(&sourcing, &libraries.mapping).map_err(fail)?;
        // Guidelines that the design goes against (ADR 0036) are for the person ordering, so
        // they are in `fab-notes.txt`.
        let mut extras = board.process_extras();
        extras.splice(0..0, pin_notes.iter().cloned());
        extras.extend(glb_note.map(|n| format!("warning: {n}")));
        extras.extend(step_note.map(|n| format!("warning: {n}")));
        if let Some(scene) = &scene {
            extras.extend(scene.warnings.iter().map(|w| format!("warning: {w}")));
            extras.extend(model_notes(scene));
        }
        extras.extend(crate::no_supplier::note(&sourcing));
        // The notes say which part hangs over which edge, how far, and who said it may; each
        // is a warning (owner decision, 2026-11-03, ADR 0045 section 10).
        extras.extend(board.hangs().into_iter().map(|h| format!("warning: {h}")));
        extras.extend(unchecked.iter().map(|u| format!("warning: {u}")));
        extras.extend(netsmith_pipeline::heights::notes(
            &built.design,
            &built.catalog_heights,
            &board,
        ));
        extras.extend(crate::case_fit::notes(&fit));
        extras.extend(
            board
                .trims()
                .iter()
                .flat_map(Trim::unchecked)
                .map(|u| format!("warning: {u}")),
        );
        extras.extend(
            built
                .guideline_warnings_except(&environment.replaced)
                .into_iter()
                .chain(environment.findings)
                .map(|w| format!("warning: {w}")),
        );
        // What the check of the environment did and did not do, for a board that has one.
        extras.extend(environment.notes);
        // The notes lead with what is wrong, so that nobody orders an incomplete board.
        if let Some(incomplete) = &incomplete {
            extras.insert(0, format!("ERROR: INCOMPLETE: {}", incomplete.headline()));
            for (k, line) in incomplete.lines.iter().take(10).enumerate() {
                extras.insert(1 + k, line.clone());
            }
            if incomplete.lines.len() > 10 {
                extras.insert(
                    11,
                    format!(
                        "ERROR: ... and {} more not routed, all in {ROUTING_FAILURE}",
                        incomplete.lines.len() - 10
                    ),
                );
            }
        }
        let package_options = Options {
            // The check finds the unrouted connections as unconnected pads; the package is made
            // anyway, for the person to look at, and the run still fails.
            allow_drc_violations: options.allow_drc_violations || incomplete.is_some(),
            drc: drc_extras(&board),
            trim: board.trims().iter().map(Trim::note).collect(),
            process: Some(board.process_note()),
            extras,
            fit: fit_of(&built.sourcing),
            // The design's requirements on where parts go (ADR 0045): an error that stops
            // the package, or a warning with `constraints = "warn"`.
            constraints: netsmith_pipeline::constraints::of_design(
                &built.design,
                catalogs.constraints_warn(),
            ),
            ..Options::default()
        };
        if let Some(incomplete) = incomplete {
            return Err(fail(write_incomplete(
                &incomplete,
                &dir,
                package(&pcb_path, &entries, &package_options),
            )));
        }
        // Reads the written board back, runs the design rules check, plots the Gerbers and
        // drill files and makes the BOM and the placement file.
        let pkg = timings::stage("fab.package", &label, || {
            package(&pcb_path, &entries, &package_options)
        })
        .map_err(|e| fail(e.to_string()))?;
        let mut files = vec![pcb_path.clone()];
        files.extend(pin_files);
        files.extend(glb_file);
        files.extend(step_file);
        files.extend(
            timings::stage("fab.write", &label, || pkg.write(&dir))
                .map_err(|e| fail(e.to_string()))?,
        );
        let mut notes = pkg.notes.clone();
        if options.cross_check {
            let kicad = kicad.ok_or_else(|| {
                fail(
                    "--cross-check compares with KiCad's own files, and KiCad is not installed"
                        .to_string(),
                )
            })?;
            say(format!("{}: cross-checking against KiCad", built.label()));
            let report = netsmith_fab::cross_check(
                &pcb_path,
                kicad,
                &netsmith_fab::CrossCheckLimits {
                    pictures: true,
                    ..Default::default()
                },
            )
            .map_err(|e| fail(e.to_string()))?;
            // The pictures of the differences, for a person to look at.
            for layer in &report.layers {
                if let Some(png) = layer.diff_png.as_ref().filter(|_| !layer.ok) {
                    let file = dir.join(format!("cross-check-{}.png", layer.file));
                    fs::write(&file, png).map_err(|e| fail(format!("{}: {e}", file.display())))?;
                    files.push(file);
                }
            }
            if !report.ok() {
                return Err(fail(format!("{report}")));
            }
            notes.push_str(&format!("{report}"));
        }

        let fabricated = Fabricated {
            board: built.label(),
            dir,
            gerber_files: pkg.gerber_files.clone(),
            summary: pkg.summary(),
            notes,
            files,
            facts: BoardFacts::of(&board, &pkg),
            origins: board.origins(),
        };
        Ok(BoardRun {
            fabricated,
            fit: package_options.fit.clone(),
            options: package_options,
            board,
            package: pkg,
            entries,
            pcb: pcb_path,
        })
    }
}

/// The system scene of `system` from its members as made (ADR 0110): its findings across
/// boards, and `<out>/<System>/<System>.glb` and `.step` with the switches of a board's. A
/// collision of known heights and a misaligned mate stop the run, and `--allow-drc-violations`
/// does not waive them; a collision through a package standard's height, a part of unknown height
/// facing another board and a member drawn apart are warnings, errors with strict constraints.
fn fab_system(
    system: &netsmith_pipeline::SolvedSystem,
    members: &[(&Built, &Board)],
    out: &Path,
    options: &FabOptions,
    say: &dyn Fn(String),
) -> Result<(), String> {
    let name = &system.name;
    let scene = timings::stage("fab.system", name, || {
        crate::system3d::scene(system, members)
    });
    let strict = options.strict_heights;
    let mut errors = String::new();
    for f in &scene.findings {
        if f.code.is_error(strict) {
            let why = if f.code.error() {
                "it stops `fab`, and `--allow-drc-violations` does not waive it"
            } else {
                "it is an error because constraints are strict (`--constraints error` or `constraints = \"error\"`); run with `--constraints warn` to list it instead"
            };
            errors.push_str(&format!(
                "error: {name}: {}: {}; {why}\n",
                f.code.word(),
                f.message
            ));
        } else {
            say(format!("{name}: warning: {}: {}", f.code.word(), f.message));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let dir = out.join(name);
    fs::create_dir_all(&dir).map_err(|e| format!("error: {name}: {}: {e}\n", dir.display()))?;
    for (write, extension, what) in [
        (options.glb, "glb", "3D scene"),
        (options.step, "step", "STEP file"),
    ] {
        let file = dir.join(format!("{name}.{extension}"));
        if !write {
            if let Err(e) = fs::remove_file(&file)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                say(format!(
                    "{name}: warning: {}: {e}; an earlier run's {what} stays in the package",
                    file.display()
                ));
            }
            continue;
        }
        let bytes = if extension == "glb" {
            netsmith_3d::system_glb(&scene)
        } else {
            netsmith_3d::system_step(&scene).into_bytes()
        };
        match fs::write(&file, bytes) {
            Ok(()) => say(format!(
                "{name}: wrote the system's {what} {}",
                file.display()
            )),
            Err(e) => say(format!(
                "{name}: warning: the system's {what} {} was not written ({e}); the boards' packages are complete without it",
                file.display()
            )),
        }
    }
    Ok(())
}

/// The board's 3D scene as `<name>.glb` in `dir` (or, with `write` false, the removal of one an
/// earlier run left): the bytes `board3d --glb` writes for `board`, which is the board as fab
/// placed and routed it, with its copper (the editor's `/api/pcb/3d` draws the same). Returns the
/// file written and a note for the person ordering when it could not be written.
fn write_glb(
    dir: &Path,
    name: &str,
    built: &Built,
    scene: Option<&netsmith_3d::Scene>,
    write: bool,
) -> (Option<PathBuf>, Option<String>) {
    let file = dir.join(format!("{name}.glb"));
    if !write {
        return match fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => (
                None,
                Some(format!(
                    "{}: {e}; an earlier run's 3D scene stays in the package",
                    file.display()
                )),
            ),
            _ => (None, None),
        };
    }
    let Some(scene) = scene.filter(|_| write) else {
        return (None, None);
    };
    match fs::write(&file, netsmith_3d::glb(scene, &built.label())) {
        Ok(()) => (Some(file), None),
        Err(e) => (
            None,
            Some(format!(
                "the 3D scene {} was not written ({e}); the package is complete without it",
                file.display()
            )),
        ),
    }
}

/// The board as `<name>.step` in `dir` (or, with `write` false, the removal of one an earlier
/// run left): the bytes `board3d --step` writes for `board`, the board as fab placed and routed
/// it; the STEP holds no copper, so they are those of `board3d --step` for the same placement.
/// Returns the file written and a note for the person ordering when it could not be written.
fn write_step(
    dir: &Path,
    name: &str,
    built: &Built,
    scene: Option<&netsmith_3d::Scene>,
    write: bool,
) -> (Option<PathBuf>, Option<String>) {
    let file = dir.join(format!("{name}.step"));
    if !write {
        return match fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => (
                None,
                Some(format!(
                    "{}: {e}; an earlier run's STEP file stays in the package",
                    file.display()
                )),
            ),
            _ => (None, None),
        };
    }
    let Some(scene) = scene.filter(|_| write) else {
        return (None, None);
    };
    match fs::write(
        &file,
        netsmith_3d::scene_step(scene, &built.label()).into_bytes(),
    ) {
        Ok(()) => (Some(file), None),
        Err(e) => (
            None,
            Some(format!(
                "the STEP file {} was not written ({e}); the package is complete without it",
                file.display()
            )),
        ),
    }
}

/// The lines of `fab-notes.txt` that name the parts drawn from a STEP model (ADR 0108 "Cost"):
/// the `.glb` and the `.step` hold the model, which its maker's license may keep from being
/// shared.
fn model_notes(scene: &netsmith_3d::Scene) -> Vec<String> {
    let drawn: Vec<String> = scene
        .bodies
        .iter()
        .filter_map(|b| {
            let m = b.model.as_ref()?;
            Some(format!(
                "{} (`{}`) from `{}` (sha256 {})",
                b.refdes, b.path, m.file, m.sha256
            ))
        })
        .collect();
    if drawn.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "note: drawn from a STEP model in the .glb and the .step (never measured: heights are the parts' own): {}; share those two files only as the models' licenses allow",
        drawn.join(", ")
    )]
}

/// Why a finding of the case fit check stops `fab` (ADR 0109), and what to do.
fn case_refusal(f: &netsmith_3d::FitFinding) -> String {
    use netsmith_3d::{CaseWeight, CaseWhy};
    let kind = f.kind.word();
    match (f.weight(), &f.why) {
        (CaseWeight::Error, CaseWhy::Unreadable { .. }) => format!(
            "{kind}: {f}; a case model netsmith cannot read stops `fab`, and `--allow-drc-violations` does not waive it"
        ),
        (CaseWeight::Error, _) => format!(
            "{kind}: {f}; a collision with the case stops `fab`, and neither `--allow-drc-violations` nor `--constraints warn` waives it: move or lower the part, or correct the case model's placement (`.x`, `.y`, `.z`, `.rx`, `.ry`, `.rz`)"
        ),
        _ => format!(
            "{kind}: {f}; this is an error because constraints are strict (`--constraints error` or `constraints = \"error\"`), and `--allow-drc-violations` does not waive it: state the height, close the shell, or run with `--constraints warn`"
        ),
    }
}

/// A board the router left connections of: the board as it was placed, for the pin optimizer,
/// and what it left.
struct Stuck {
    placed: Board,
    left: usize,
    crossings: usize,
}

/// What `fab_board_pinned` needs to elaborate the source again with another pin map.
struct Pass<'a> {
    name: &'a str,
    text: &'a str,
    loader: &'a Loader,
}

/// The pin map of the source as the run stands: what the check gave each board's requests, what
/// the optimizer has changed, and what the pin optimizer may move.
struct PinState {
    /// The pin file as the run began, and as it is now.
    original: PinFile,
    file: PinFile,
    spaces: BTreeMap<String, Vec<PartSpace>>,
    /// The proposal of `--pins locked`, with every board's moves.
    proposed: Option<PinFile>,
}

impl PinState {
    /// What the check decided about the requests of the parts of `built`, as the run began: none
    /// for a board of a system, whose requests are the system's.
    fn spaces_of(&self, built: &Built) -> &[PartSpace] {
        match self.spaces.get(&built.board) {
            Some(spaces) if built.system.is_none() => spaces,
            _ => &[],
        }
    }

    fn new(pins: &netsmith_check::PinsOutcome) -> Self {
        Self {
            original: pins.file.clone(),
            file: pins.file.clone(),
            spaces: pins.spaces.clone(),
            proposed: None,
        }
    }
}

/// What a pass of `fab_board` writes the pin map for firmware from: the pins of the requests of
/// the board that pass made, and the source file's name for the files.
struct PinMapOf<'a> {
    spaces: &'a [PartSpace],
    source: &'a str,
}

impl PinMapOf<'_> {
    /// The pin map of `built` (`None`: no request, or a board of a system).
    fn render(&self, built: &Built) -> Result<Option<crate::pin_export::PinMap>, String> {
        if built.system.is_some() {
            return Ok(None);
        }
        crate::pin_export::render(&built.design, self.spaces, self.source)
            .map_err(|message| format!("{message}\n"))
    }
}

/// The requests whose pin differs between two maps, one line each: `Board.g01: p00 to p09`.
fn pin_changes(old: &PinFile, new: &PinFile) -> Vec<String> {
    let mut out = Vec::new();
    for (board, entries) in &new.boards {
        for (key, e) in entries {
            if let Some(was) = old.board(board).and_then(|b| b.get(key))
                && was.pin != e.pin
            {
                out.push(format!("{board}.{key}: {} to {}", was.pin, e.pin));
            }
        }
    }
    out
}

/// [`fab_board`], and when the router leaves connections and the board has pin requests, the
/// pin optimizer (`pin_swap`) and a new pass of stages 1 to 5 with the pins it proposes
/// (ADR 0101; owner decisions of 2026-11-09, Q11 to Q13 and Q16 of
/// `docs/research/pin-assignment-options.md`).
///
/// With `--pins auto` each round searches the placed board for a better pin map, elaborates the
/// source with it and places and routes again, at most `pin_swap::MAX_ROUNDS` times. A round's
/// map is kept when it routes, or when it leaves no more connections and no more crossings than
/// the map before it; otherwise the map before stands, and the board is made again with it so
/// that the files are its own. The pin file is written with the map that stands. The rounds
/// share one time budget, the route timeout, counted from the first failure.
///
/// With `--pins locked` nothing is changed: the search runs once on the placed board, its
/// proposal is written to `pins.proposed.toml` in `out`, and the run fails as before.
///
/// A board with no requests, a board that routes, and a board of a system (its requests are in
/// the system's table too) are made by [`fab_board`] alone.
#[allow(clippy::too_many_arguments)]
fn fab_board_pinned(
    pass: &Pass,
    built: &Built,
    state: &mut PinState,
    out: &Path,
    layout: &LayoutFile,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
    options: &FabOptions,
    catalogs: &Catalogs,
    say: &dyn Fn(String),
) -> Result<BoardRun, String> {
    let mut first_stuck = None;
    let source = options.pin_map_source.as_deref().unwrap_or(pass.name);
    let first = fab_board(
        built,
        out,
        layout,
        libraries,
        kicad,
        options,
        catalogs,
        say,
        &mut first_stuck,
        &[],
        &PinMapOf {
            spaces: state.spaces_of(built),
            source,
        },
    );
    let (Some(pins), Err(message), Some(first_stuck)) = (&options.pins, &first, first_stuck) else {
        return first;
    };
    let board = built.board.clone();
    let label = built.label();
    let spaces = match state.spaces.get(&board) {
        Some(s) if built.system.is_none() => s.clone(),
        _ => return first,
    };
    // One budget for the search and every route after it, counted from the first failure.
    let budget = pin_swap::Budget::start(options.route_timeout, &pins.clock);
    let design = &built.design;

    if pins.mode == PinsMode::Locked {
        let proposal = pin_swap::improve(&spaces, design, &first_stuck.placed, &budget);
        let mut message = message.clone();
        match proposal {
            None => message.push_str(&format!(
                "  pins: {given}, so no pin moves; no change of the unlocked pins of {board} lowers its crossings or its ratsnest\n",
                given = pins.locked_by.given()
            )),
            Some(proposal) => {
                let base = state.proposed.clone().unwrap_or_else(|| state.file.clone());
                let file = proposal.applied_to(&base, &board);
                let target = out.join(PROPOSED_PINS);
                let wrote = fs::create_dir_all(out).and_then(|()| {
                    fs::write(
                        &target,
                        format!(
                            "# Proposed by `netsmith fab` ({}): placed and not routed, so not yet known to route. Copy it over the pin file to take it.\n{}",
                            pins.locked_by.given(),
                            file.to_text()
                        ),
                    )
                });
                message.push_str(&format!(
                    "  pins: {}, so no pin moves. Moving the unlocked pins of {board} would lower the ratsnest from {} to {} (placed and not routed):\n",
                    pins.locked_by.given(),
                    proposal.before,
                    proposal.after
                ));
                for line in &proposal.moves {
                    message.push_str(&format!("    {line}\n"));
                }
                match wrote {
                    Ok(()) => message.push_str(&format!(
                        "  the proposed pin map is in {}; copy it over the pin file and run again, or {}\n",
                        target.display(),
                        pins.locked_by.way_out()
                    )),
                    Err(e) => message.push_str(&format!("  ({} could not be written: {e})\n", target.display())),
                }
                state.proposed = Some(file);
            }
        }
        return Err(message);
    }

    // `--pins auto`: rounds of search, place and route.
    let pin_file_name = pass
        .loader
        .pin_file(pass.name)
        .map(|(file, _)| file.to_string())
        .unwrap_or_else(|| crate::pins::file_name(pass.name));
    // How many connections the files on disk leave: the first pass's, then each later pass's.
    let mut on_disk_left = Some(first_stuck.left);
    let mut best_stuck = first_stuck;
    let mut best_message = message.clone();
    let mut best_map = state.file.clone();
    let mut best_spaces = spaces;
    let mut best_built: Option<Built> = None;
    let mut best_is_last = true;
    let mut rounds = 0;
    for round in 1..=pin_swap::MAX_ROUNDS {
        let time_is_spent = || {
            say(format!(
                "{label}: pins: the time budget of the route timeout is spent, no further round"
            ));
        };
        if budget.spent() {
            time_is_spent();
            break;
        }
        let current = best_built.as_ref().unwrap_or(built);
        let Some(proposal) =
            pin_swap::improve(&best_spaces, &current.design, &best_stuck.placed, &budget)
        else {
            say(format!(
                "{label}: pins: round {round}: no change of the unlocked pins lowers the crossings or the ratsnest, stopping"
            ));
            break;
        };
        // The search used time of the budget: what is left is read now, after it, and the route
        // below gets that and no more. A round that has no time to route is not made, and the
        // files on disk stay the last pass's, which are the best so far.
        if budget.spent() {
            time_is_spent();
            break;
        }
        say(format!(
            "{label}: pins: round {round}: {} moves; {} to {}; placing and routing again",
            proposal.moves.len(),
            proposal.before,
            proposal.after
        ));
        let map = proposal.applied_to(&best_map, &board);
        let mut loader = pass.loader.clone();
        loader.attach_pins(pass.name, &pin_file_name, &map.to_text());
        let Ok(again) = netsmith_pipeline::outcome_with(
            pass.name,
            pass.text,
            &loader,
            catalogs,
            Some(libraries),
        ) else {
            say(format!(
                "{label}: pins: round {round}: the board does not build with the new pins, stopping"
            ));
            best_is_last = true;
            break;
        };
        let spaces_again = again.pins.spaces.get(&board).cloned().unwrap_or_default();
        let Some(next) = again.boards.into_iter().find(|b| b.label() == label) else {
            break;
        };
        let notes = pin_notes(&state.original, &map);
        // The elaboration took time too: the route gets what is left now.
        let route_timeout = match budget.left() {
            Some(left) if left.is_zero() => {
                time_is_spent();
                break;
            }
            left => left,
        };
        let attempt_options = FabOptions {
            route_timeout,
            ..options.clone()
        };
        rounds = round;
        let mut stuck = None;
        match fab_board(
            &next,
            out,
            layout,
            libraries,
            kicad,
            &attempt_options,
            catalogs,
            say,
            &mut stuck,
            &notes,
            &PinMapOf {
                spaces: &spaces_again,
                source,
            },
        ) {
            Ok(run) => {
                say(format!("{label}: pins: round {round}: the board routes"));
                finish_pins(state, pins, &map, say);
                return Ok(run);
            }
            Err(m) => {
                // This pass wrote the files on disk now: how many connections they leave.
                on_disk_left = stuck.as_ref().map(|s| s.left);
                match stuck {
                    Some(s) if s.left <= best_stuck.left && s.crossings <= best_stuck.crossings => {
                        say(format!(
                            "{label}: pins: round {round}: {} connections left (was {}), {} crossings (was {}); keeping the new pins",
                            s.left, best_stuck.left, s.crossings, best_stuck.crossings
                        ));
                        best_stuck = s;
                        best_message = m;
                        best_map = map;
                        best_spaces = spaces_again;
                        best_built = Some(next);
                        best_is_last = true;
                    }
                    _ => {
                        say(format!(
                            "{label}: pins: round {round}: no better than before, keeping the earlier pins"
                        ));
                        best_is_last = false;
                        break;
                    }
                }
            }
        }
    }
    // The files on disk are the last pass's. When the map that stands is an earlier one, make
    // the board again with it, unless that does worse than the files already there.
    let mut result = best_message;
    if !best_is_last {
        let current = best_built.as_ref().unwrap_or(built);
        let notes = pin_notes(&state.original, &best_map);
        let mut again = None;
        let remake_options = FabOptions {
            route_timeout: remake_timeout(&budget, options.route_timeout),
            ..options.clone()
        };
        let dir = current.dir(out);
        let aside = set_aside(&dir);
        let remade = fab_board(
            current,
            out,
            layout,
            libraries,
            kicad,
            &remake_options,
            catalogs,
            say,
            &mut again,
            &notes,
            &PinMapOf {
                spaces: &best_spaces,
                source,
            },
        );
        let remake_left = match (&remade, &again) {
            (Ok(_), _) => Some(0),
            (Err(_), Some(stuck)) => Some(stuck.left),
            // No unrouted connection was counted: either the pass stopped before it routed, and
            // wrote nothing, or it routed everything and failed after (the design rules check, say),
            // and wrote the folder.
            (Err(_), None) => dir.is_dir().then_some(0),
        };
        let keep = aside.is_some() && keep_earlier_files(remake_left, on_disk_left);
        if keep && let Some(aside) = &aside {
            let back = set_back(&dir, aside);
            say(format!(
                "{label}: pins: the board made again with the pins that stand left {} connection(s), the files before it {}: keeping the files before it",
                remake_left.map_or_else(|| "no board".to_string(), |n| n.to_string()),
                on_disk_left.map_or_else(|| "?".to_string(), |n| n.to_string()),
            ));
            result.push_str(&format!(
                "  pins: the files in {} are those of the last round of pins, which is not the pin map written ({}); do not order from them\n",
                dir.display(),
                why_earlier_files_stay(remake_left)
            ));
            if let Err(e) = back {
                result.push_str(&format!(
                    "  ({} could not be restored: {e})\n",
                    dir.display()
                ));
            }
        } else {
            if let Some(aside) = &aside {
                let _ = fs::remove_dir_all(aside);
            }
            if let Err(m) = remade {
                result = m;
            }
        }
    }
    finish_pins(state, pins, &best_map, say);
    if rounds > 0 {
        result.push_str(&format!(
            "  pins: tried {rounds} round(s) of swapping the unlocked pins; the board still has {} connection(s) left\n",
            best_stuck.left
        ));
    }
    Err(result)
}

/// The shortest route the board is made again with when the budget is spent ([`remake_timeout`]).
pub(crate) const REMAKE_FLOOR: Duration = Duration::from_secs(10);

/// The route timeout of the pass that makes the board again with the pin map that stands, after a
/// round that was no better (the files on disk are that round's). It gets what is left of the
/// budget, so that the run as a whole keeps to the route timeout. When less than [`REMAKE_FLOOR`]
/// is left (nothing, or a second: a router cut off after a second leaves most of the copper
/// out) it gets the floor instead (never more than the route timeout itself): the files must be
/// the ones of the pin file that is written, and the board is marked incomplete either way, so
/// the run is longer than the route timeout by at most the floor. A remake that still routes worse
/// than the files already written does not replace them ([`keep_earlier_files`]).
pub(crate) fn remake_timeout(
    budget: &pin_swap::Budget,
    route_timeout: Option<Duration>,
) -> Option<Duration> {
    match (budget.left(), route_timeout) {
        (Some(left), Some(total)) if left < REMAKE_FLOOR => Some(REMAKE_FLOOR.min(total)),
        (left, _) => left,
    }
}

/// Whether the files of the pass before the remake stay, because the remake did not do better
/// than they do: `remake_left` is the connections the remake left (`None`: it made no board),
/// `on_disk_left` those of the files on disk (`None`: not known, so the remake is not held
/// against them). A remake that leaves more connections than the files on disk, or that made no
/// board, never replaces them.
fn keep_earlier_files(remake_left: Option<usize>, on_disk_left: Option<usize>) -> bool {
    match (remake_left, on_disk_left) {
        (Some(remake), Some(on_disk)) => remake > on_disk,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

/// What to say of a remake whose board did not replace the files before it: it made no board
/// (`None`: it failed before it routed, in placement, say) or it left more connections.
fn why_earlier_files_stay(remake_left: Option<usize>) -> &'static str {
    match remake_left {
        Some(_) => "making the board again with it left more connections",
        None => "making the board again with it made no board",
    }
}

/// Move the folder `dir` of a board's files aside, so that a pass can write it afresh and the
/// old files come back if that pass does worse ([`set_back`]). `None` when there is no folder, or
/// it could not be moved (then the pass writes over it, as every pass does).
fn set_aside(dir: &Path) -> Option<PathBuf> {
    let name = dir.file_name()?.to_string_lossy().into_owned();
    let aside = dir.with_file_name(format!(".{name}.before-remake"));
    let _ = fs::remove_dir_all(&aside);
    dir.is_dir()
        .then(|| fs::rename(dir, &aside).ok().map(|()| aside))
        .flatten()
}

/// Put the folder moved aside by [`set_aside`] back where it was, in place of what a pass wrote.
fn set_back(dir: &Path, aside: &Path) -> std::io::Result<()> {
    let _ = fs::remove_dir_all(dir);
    fs::rename(aside, dir)
}

/// The proposed pin map of `--pins locked`, in the output folder.
const PROPOSED_PINS: &str = "pins.proposed.toml";

/// The line of `fab-notes.txt` that says the pin map is not the one the run began with: a changed
/// pin is a change of the firmware.
fn pin_notes(old: &PinFile, new: &PinFile) -> Vec<String> {
    let changes = pin_changes(old, new);
    if changes.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "pin map changed by this run, so the firmware's pin map changes: {}",
        changes.join("; ")
    )]
}

/// Make `map` the pin map the run ends with, and under `--pins auto` write the pin file when it
/// differs from the one the run began with.
fn finish_pins(state: &mut PinState, pins: &PinsUse, map: &PinFile, say: &dyn Fn(String)) {
    state.file = map.clone();
    if state.file == state.original {
        return;
    }
    match fs::write(&pins.file, state.file.to_text()) {
        Ok(()) => {
            say(format!("wrote {}", pins.file.display()));
            for line in pin_changes(&state.original, &state.file) {
                say(format!("  {line}"));
            }
        }
        Err(e) => say(format!("error: {}: {e}", pins.file.display())),
    }
}

/// What is written for a board the router left connections of.
pub const ROUTING_FAILURE: &str = "routing-failure.txt";
/// Beside it, for a program: why each line is left and the crowding map, as JSON.
pub const ROUTING_REPORT: &str = "routing-report.json";

/// A board with connections the router could not make: the message of the error `fab` ends
/// in, the text of [`ROUTING_FAILURE`], and one line of the notes for each connection.
struct Incomplete {
    message: String,
    report: String,
    /// The text of [`ROUTING_REPORT`].
    json: String,
    lines: Vec<String>,
}

impl Incomplete {
    /// The first line of the notes.
    fn headline(&self) -> String {
        format!(
            "{} connection(s) have no copper; this package is NOT ready to order, see {ROUTING_FAILURE}",
            self.lines.len()
        )
    }
}

/// The text of [`ROUTING_REPORT`]: why each line is left and the crowding map (see
/// [`netsmith_router::Routing`]), in the board's own coordinates.
pub fn routing_report(
    why: &[netsmith_router::LineReport],
    crowding: &netsmith_router::Crowding,
) -> String {
    let report = serde_json::json!({
        "why": why,
        "crowding": crowding,
    });
    serde_json::to_string_pretty(&report).unwrap_or_default() + "\n"
}

/// The text of `routing-failure.txt`: every connection that has no copper, with its net and
/// its two pads and where they are, the parts most of them end on, and for each line the
/// router left the stage it last failed in and the nets in its way.
pub fn routing_failure(
    label: &str,
    origin: netsmith_pcb::Pt,
    left: &netsmith_pcb::Ratsnest,
    tries: usize,
    why: &[netsmith_router::LineReport],
) -> String {
    let at = |p: netsmith_pcb::Pt| (p.x - origin.x, p.y - origin.y);
    let mut text = format!(
        "INCOMPLETE: {label} is not fully routed. {} connection(s) have no copper after {tries} tries of the router, so this board does not work. The board file and the Gerbers beside this file are only for looking at the part that is routed; do not order them.\n\nEach line: the net, its two pads, and where they are in mm from the outline's top left corner, y down.\n\n",
        left.len()
    );
    let mut ends: BTreeMap<&str, usize> = BTreeMap::new();
    for line in &left.lines {
        let (from, to) = (at(line.from_at), at(line.to_at));
        if line.to_plane {
            text.push_str(&format!(
                "net {}: {} to the plane, at ({:.2}, {:.2})\n",
                line.net, line.from, from.0, from.1
            ));
        } else {
            text.push_str(&format!(
                "net {}: {} at ({:.2}, {:.2}) to {} at ({:.2}, {:.2})\n",
                line.net, line.from, from.0, from.1, line.to, to.0, to.1
            ));
        }
        let mut parts = vec![line.from.refdes.as_str()];
        if !line.to_plane && line.to.refdes != line.from.refdes {
            parts.push(line.to.refdes.as_str());
        }
        for part in parts {
            *ends.entry(part).or_default() += 1;
        }
    }
    let mut ends: Vec<(&str, usize)> = ends.into_iter().collect();
    ends.sort_by_key(|&(part, n)| (std::cmp::Reverse(n), part));
    text.push_str("\nThe parts the unrouted connections end on, most first (a part that most of them end on has pads that are walled in by their neighbours or by the copper of other nets):\n");
    for (part, n) in ends.iter().take(8) {
        text.push_str(&format!("  {part}: {n}\n"));
    }
    if !why.is_empty() {
        text.push_str(&format!(
            "\nWhy each line is left, by the router's own lines (which can pair the same pads otherwise than above): the stage of the router it last failed in, and the nets whose copper stands in its way (the fewest that a path allowed to cross other nets' copper meets). {ROUTING_REPORT} has the same, and a map of where the board was fought over, in the board's coordinates:\n"
        ));
        for w in why {
            let nets = if w.blockers.is_empty() {
                "no net's copper (the pads, holes or edge alone, or no time to look)".to_string()
            } else {
                format!("net(s) {}", w.blockers.join(", "))
            };
            text.push_str(&format!("  {}: {}; in its way: {nets}\n", w.line, w.stage));
        }
    }
    text
}

/// Write what an incomplete board has: `routing-failure.txt`, and the Gerbers, the bill of
/// materials, the placement file and the notes if the package could be made (the notes lead
/// with the error). Returns the message `fab` fails with.
fn write_incomplete(
    incomplete: &Incomplete,
    dir: &Path,
    package: Result<Package, netsmith_fab::Error>,
) -> String {
    let mut message = incomplete.message.clone();
    let file = dir.join(ROUTING_FAILURE);
    let wrote = fs::write(&file, &incomplete.report)
        .and_then(|()| fs::write(dir.join(ROUTING_REPORT), &incomplete.json));
    message.push_str(&format!(
        "\n  INCOMPLETE: the board file is in {}",
        dir.display()
    ));
    match package.map(|pkg| pkg.write(dir).map(|_| pkg)) {
        Ok(Ok(pkg)) => message.push_str(&format!(
            " with the Gerbers ({}) and fab-notes.txt; they are NOT ready to order",
            pkg.zip_name()
        )),
        Ok(Err(e)) => message.push_str(&format!("; the Gerbers could not be written: {e}")),
        Err(e) => message.push_str(&format!("; the Gerbers were not made: {e}")),
    }
    match wrote {
        Ok(()) => message.push_str(&format!(
            "\n  every unrouted connection with its net and pads is in {}",
            file.display()
        )),
        Err(e) => message.push_str(&format!(
            "\n  ({} could not be written: {e})",
            file.display()
        )),
    }
    message
}

/// The panel of `spec`, made from the board `run` made: the panel board file, its Gerbers,
/// drill files, bill of materials and placement file, in `dir` beside the board's own
/// package in `dir/cell/`.
fn fab_panel(
    spec: &PanelSpec,
    run: &BoardRun,
    dir: &Path,
    kicad: Option<&dyn KicadCli>,
    options: &FabOptions,
    say: &dyn Fn(String),
) -> Result<Fabricated, String> {
    let fail = |what: String| format!("error: panel {}: {what}\n", spec.name);
    let pcb =
        fs::read_to_string(&run.pcb).map_err(|e| fail(format!("{}: {e}", run.pcb.display())))?;
    let pro = fs::read_to_string(run.pcb.with_extension("kicad_pro")).ok();
    let assembled = &run.package.plan.assembled;
    let panel = PanelBuildSpec {
        name: spec.name.clone(),
        columns: spec.columns,
        rows: spec.rows,
        rail: spec.rail,
        rules: panel_rules(&spec.rules),
    };
    let input = PanelCellInput {
        pcb: &pcb,
        pro: pro.as_deref(),
        entries: &run.entries,
        facts: PanelCellFacts {
            board: spec.cell.clone(),
            thickness: run.board.process.stackup.chosen_thickness(),
            edge_clearance: run.board.process.min_edge_clearance,
            assembled: !assembled.is_empty(),
            bottom_assembled: assembled.iter().any(|p| p.side == Side::Bottom),
        },
    };
    let package_options = Options {
        fit: run.fit.clone(),
        ..run.options.clone()
    };
    let pkg = netsmith_fab::build_panel(&panel, &input, &package_options)
        .map_err(|e| fail(e.to_string()))?;
    let mut files = pkg.write(dir).map_err(|e| fail(e.to_string()))?;
    let mut notes = pkg.notes.clone();
    if options.cross_check {
        let kicad = kicad.ok_or_else(|| {
            fail(
                "--cross-check compares with KiCad's own files, and KiCad is not installed"
                    .to_string(),
            )
        })?;
        say(format!("{}: cross-checking against KiCad", spec.name));
        let report = netsmith_fab::cross_check(
            &dir.join(format!("{}.kicad_pcb", spec.name)),
            kicad,
            &netsmith_fab::CrossCheckLimits {
                pictures: true,
                ..Default::default()
            },
        )
        .map_err(|e| fail(e.to_string()))?;
        for layer in &report.layers {
            if let Some(png) = layer.diff_png.as_ref().filter(|_| !layer.ok) {
                let file = dir.join(format!("cross-check-{}.png", layer.file));
                fs::write(&file, png).map_err(|e| fail(format!("{}: {e}", file.display())))?;
                files.push(file);
            }
        }
        if !report.ok() {
            return Err(fail(format!("{report}")));
        }
        notes.push_str(&format!("{report}"));
    }
    let process = &run.board.process;
    let facts = BoardFacts {
        layers: process.stackup.layers,
        width_mm: pkg.geometry.width,
        height_mm: pkg.geometry.height,
        thickness_mm: process.stackup.chosen_thickness(),
        assembled_parts: crate::order::stock_parts(&pkg.plan),
        bom: pkg.plan.bom.clone(),
        consigned: pkg.plan.consigned.clone(),
        hand_solder: pkg.plan.hand_solder.clone(),
        not_fitted: pkg.plan.not_fitted.clone(),
        unassembled: pkg.plan.unassembled.clone(),
        unverified: pkg.unverified.clone(),
        panel: Some(PanelFacts {
            board: spec.cell.clone(),
            columns: spec.columns,
            rows: spec.rows,
            cell_width_mm: pkg.geometry.cell_width,
            cell_height_mm: pkg.geometry.cell_height,
            rail_mm: spec.rail,
        }),
    };
    Ok(Fabricated {
        board: spec.name.clone(),
        dir: dir.to_path_buf(),
        gerber_files: pkg.gerber_files.clone(),
        summary: pkg.summary(),
        notes,
        files,
        facts,
        origins: Vec::new(),
    })
}

/// The rules of the board house for the writer, from the language's.
fn panel_rules(r: &netsmith_core::PanelRules) -> netsmith_fab::PanelBuildRules {
    netsmith_fab::PanelBuildRules {
        fabricator: r.fabricator.clone(),
        source: r.source.clone(),
        min_rail: r.min_rail,
        min_size: r.min_size,
        max_size: r.max_size,
        max_assembled_size: r.max_assembled_size,
        max_cuts: r.max_cuts,
        min_cut_spacing: r.min_cut_spacing,
        advised_cut_spacing: r.advised_cut_spacing,
        min_edge_clearance: r.min_edge_clearance,
        advised_edge_clearance: r.advised_edge_clearance,
        min_thickness: r.min_thickness,
        tooling_hole: r.tooling_hole,
        fiducial: r.fiducial,
        fiducial_offset: r.fiducial_offset,
    }
}

/// What the design says about how each part is fitted, for the packager, which would
/// otherwise read it from the footprint's attributes.
fn fit_of(sourcing: &[Sourced]) -> BTreeMap<String, Fit> {
    sourcing
        .iter()
        .map(|s| {
            let fit = match s.assembly {
                Assembly::Fab => Fit::Assembled,
                Assembly::Hand => Fit::HandSolder,
                Assembly::None => Fit::NotFitted,
                Assembly::Consigned => Fit::Consigned,
            };
            (s.refdes.clone(), fit)
        })
        .collect()
}

/// The bill of materials: one entry per distinct part, with every place it is used. Parts
/// the board house does not place are left out; a consigned part is kept, with its
/// manufacturer's number as the comment and no part number unless its row has one. A part it
/// should place but that has no part number is kept without one, so the packaging step can
/// refuse it by name.
///
/// Why a consigned part looks like this (ADR 0084; the owner accepted its seven
/// choices as built on 2026-11-11). `Fab` is "the board house
/// buys it" and `Consigned` is "the board house places it, and the owner supplies it"; they
/// share everything but the supplier. Both are in `cpl.csv` and in this bill. `Fab` needs an
/// `lcsc` (`fab` refuses without); `Consigned` does not, and its comment is the manufacturer's
/// number, the LCSC cell empty unless the catalog row has a number. The `fab-notes.txt` of a
/// consigned part has a `consigned:` line with each part's manufacturer's number and quantity.
/// The stock note (#202) and the refusal of an unresolved generic (#208) never apply to it: no
/// row stands for a part the owner supplies, so a consigned generic stays generic and no LCSC
/// number is invented for it. A consigned part and a part from stock are never one line, even
/// with equal value, footprint and number, because JLCPCB does not mix the two in one line.
///
/// What the board house reads, read on 2026-11-08 as pages by an agent (netsmith itself asks no
/// supplier, ADR 0039): JLCPCB's article "How to consign parts to JLCPCB" (the owner registers
/// the components on JLCPCB's site, ships them, and "the component seller must place PCBA orders
/// to use the registered components" within the free storage period) says nothing of the BOM or
/// the CPL. "Common BOM and CPL matching issues and explanations" and "Component matching
/// guidelines for PCBA orders": consigned components, like pre-ordered and globally sourced
/// ones, are chosen by the customer in the order page's component matching interface; JLCPCB
/// goes on only when all of them are in its warehouse; a row it cannot match is "No Part
/// Selected" and is matched by hand by the manufacturer's number or a C-code. JLCPCB's BOM guide
/// lists designator, footprint, comment and LCSC part number and no column or word that marks a
/// row as consigned (a different assembler, PCBasic, asks for the row to be marked; that is no
/// JLCPCB rule). So the format has no marker, and the choice was between an empty LCSC cell and
/// a made-up column or word. An unknown column is at best ignored and at worst fails the
/// upload; a word in the LCSC cell fails the check of `C` plus digits. The row therefore keeps
/// designator, footprint and comment, and the marking is in `fab-notes.txt` and in `order.txt`
/// of an order, not in `bom.csv`. Not tested: JLCPCB's order page. Nobody has uploaded a
/// `bom.csv` with an empty cell for a registered consigned part from netsmith (a person must do
/// this; checklist in issue #10); the notes tell the person ordering to look at the matching
/// interface (the `prepare-fab-order` step). If
/// JLCPCB documents a marker, the mark changes in this one function.
///
/// Where it is not shown: the editor's bill of materials shows a consigned part as an ordinary
/// line with no number and no price (its API has a `hand_solder` flag and no consigned one, and
/// a new field changes the generated types; not built, issue #19).
pub fn bom_entries(sourcing: &[Sourced], mapping: &Mapping) -> Result<Vec<BomEntry>, String> {
    let mut entries: Vec<BomEntry> = Vec::new();
    for s in sourcing.iter().filter(|s| s.assembled()) {
        let footprint = mapping
            .map(&s.kind, s.package.as_deref())
            .map_err(|e| format!("{}: {e}", s.refdes))?
            .footprint;
        // The board names a footprint without its library: `R_0402_1005Metric`.
        let footprint = footprint
            .rsplit(':')
            .next()
            .unwrap_or(&footprint)
            .to_string();
        // The comment of a consigned part is the manufacturer's number: JLCPCB has no column that
        // marks a part as consigned, so the board house reads what the owner shipped from it
        // (ADR 0084).
        let value = match (s.assembly, &s.mpn) {
            (Assembly::Consigned, Some(mpn)) => mpn.clone(),
            _ => s.value.clone(),
        };
        match entries
            .iter_mut()
            .find(|e| e.value == value && e.footprint == footprint && e.lcsc == s.lcsc)
        {
            Some(entry) => entry.designators.push(s.refdes.clone()),
            None => entries.push(BomEntry {
                designators: vec![s.refdes.clone()],
                value,
                footprint,
                lcsc: s.lcsc.clone(),
            }),
        }
    }
    for e in &mut entries {
        e.designators.sort_by_key(|d| designator_key(d));
    }
    entries.sort_by_key(|e| designator_key(&e.designators[0]));
    Ok(entries)
}

/// `C2` before `C10`: the letters, then the number.
fn designator_key(designator: &str) -> (String, u32) {
    let split = designator
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(designator.len());
    let (letters, digits) = designator.split_at(split);
    (letters.to_string(), digits.parse().unwrap_or(0))
}

/// Route `board` with signals at the project's track margin where there is room (issue #431):
/// the router tries the margin first, in one order of the lines and a quarter of the route
/// time, and a board it leaves unfinished is routed again from the same placement at the
/// process's least track, in what is left. The short trial keeps a board that has no room for
/// the margin from spending most of its time on it first.
/// `netsmith route` routes a saved placed board with it too, so the two route alike. `base`
/// gives the router's other options (`fab` gives the defaults, `route` may set the costs, the
/// tries and the snapshots); its time limit and retries are set here.
pub fn route_with_margin(
    board: &mut Board,
    route_timeout: Option<Duration>,
    base: RouteOptions,
    track_margin: f64,
    say: &dyn Fn(String),
) -> netsmith_router::Routing {
    let route = |board: &mut Board, time: Option<Duration>, full: bool| {
        route_with(
            board,
            RouteOptions {
                retry_orders: full,
                time_limit: time,
                ..base
            },
        )
    };
    let least = board.rules.min_track_width;
    let wide = (least * track_margin * 100.0).round() / 100.0;
    if wide <= board.rules.track_width + 1e-9 {
        return route(board, route_timeout, true);
    }
    let started = std::time::Instant::now();
    let mut trial = board.clone();
    trial.rules.track_width = wide;
    let routing = route(&mut trial, route_timeout.map(|t| t / 4), false);
    if routing.left.is_empty() {
        *board = trial;
        return routing;
    }
    say(format!(
        "{}: {} connection(s) left with signals at {wide} mm; routing again at the least track, {least} mm",
        board.name,
        routing.left.len()
    ));
    let left = route_timeout.map(|t| t.saturating_sub(started.elapsed()));
    route(board, left, true)
}

/// What the design rules check needs that the board file does not say: the process's mask
/// web and widest filled via, the rules a project file cannot say and the least width of the
/// nets that carry current (issue #431), and the
/// courtyards a protruding pin excuses (ADR 0045 section 15).
pub fn drc_extras(board: &Board) -> netsmith_geom::DrcExtras {
    netsmith_geom::DrcExtras {
        mask_web: Some(board.rules.mask_bridge),
        via_in_pad: Some(board.process.via_in_pad),
        excused_courtyards: board.excused_courtyards(),
        hole_to_track: Some(board.rules.hole_clearance),
        plated_hole_clearance: Some(board.rules.plated_hole_clearance),
        pad_to_pad: Some(board.rules.pad_to_pad),
        pad_hole_to_hole: Some(board.rules.hole_to_hole),
        mask_to_track: Some(board.rules.mask_to_track),
        plated_slot: Some(board.rules.plated_slot),
        npth_slot: Some(board.rules.npth_slot),
        npth_drill: Some(board.rules.npth_drill),
        pad_ring: Some(board.rules.min_annular_ring),
        least_widths: board.least_widths(),
        neck_reach: netsmith_router::route::NECK_REACH,
        via_counts: board.via_counts(),
        track_angle: true,
    }
}

#[cfg(test)]
mod tests {

    fn moved_seed(stated: bool) -> crate::layout::Seeds {
        crate::layout::Seeds {
            first: 3,
            stated,
            limit: 8,
            used: 4,
            placed: true,
            missed: vec![(
                3,
                "the courtyards of `usb` (J2) and `c37` (C37) overlap by 0.42 mm2".into(),
            )],
            blockers: Vec::new(),
        }
    }

    #[test]
    fn a_seed_that_moved_is_written_to_the_layout_file_and_said() {
        let dir = netsmith_testkit::TempDir::new("settle-seed");
        let file = dir.path().join("main.pcb.toml");
        std::fs::write(&file, "version = 1\n\n[Board]\nseed = 3\n").unwrap();
        let options = FabOptions {
            layout: Some(file.clone()),
            ..FabOptions::default()
        };
        let said = std::cell::RefCell::new(Vec::new());
        settle_seed(
            ("Board", "Board"),
            &moved_seed(true),
            &None,
            &options,
            &|m| said.borrow_mut().push(m),
        )
        .unwrap();
        let name = file.display();
        assert_eq!(
            said.into_inner(),
            vec![format!(
                "Board: note: the seed 3 that {name} states no longer places the board, so it is stale: the placer tried seed 3 (the courtyards of `usb` (J2) and `c37` (C37) overlap by 0.42 mm2); seed 4 places it, and is written to {name}"
            )]
        );
        let layout =
            netsmith_pcb::Layout::parse(&std::fs::read_to_string(&file).unwrap(), "f").unwrap();
        assert_eq!(layout.boards["Board"].seed, Some(4));
    }

    #[test]
    fn an_order_or_the_editors_release_stops_on_a_seed_that_moved_and_writes_nothing() {
        let dir = netsmith_testkit::TempDir::new("settle-seed-order");
        let file = dir.path().join("main.pcb.toml");
        std::fs::write(&file, "version = 1\n\n[Board]\nseed = 3\n").unwrap();
        let name = file.display();
        for (writes, what) in [
            (SeedWrites::Order, "an order"),
            (SeedWrites::EditorRelease, "a release from the editor"),
        ] {
            let options = FabOptions {
                layout: Some(file.clone()),
                seed_writes: writes,
                ..FabOptions::default()
            };
            let e = settle_seed(
                ("Board", "Board"),
                &moved_seed(true),
                &None,
                &options,
                &|_| {},
            )
            .unwrap_err();
            assert_eq!(
                e,
                format!(
                    "error: Board: the seed 3 that {name} states no longer places the board, so it is stale: the placer tried seed 3 (the courtyards of `usb` (J2) and `c37` (C37) overlap by 0.42 mm2); seed 4 places it, and {what} is a release, which never changes a choice by itself: run `netsmith fab` on the source to write `seed = 4` to {name}, review it and commit it, then make the release again\n"
                )
            );
            assert_eq!(
                std::fs::read_to_string(&file).unwrap(),
                "version = 1\n\n[Board]\nseed = 3\n"
            );
        }
    }

    #[test]
    fn with_locked_a_seed_that_moved_is_an_error_and_nothing_is_written() {
        let dir = netsmith_testkit::TempDir::new("settle-seed-locked");
        let file = dir.path().join("main.pcb.toml");
        let options = FabOptions {
            layout: Some(file.clone()),
            lock: Some(netsmith_pipeline::pick_lock::Use {
                file: dir.path().join("main.lock.toml"),
                locked: true,
            }),
            ..FabOptions::default()
        };
        let e = settle_seed(
            ("Board", "Board"),
            &moved_seed(false),
            &None,
            &options,
            &|_| {},
        )
        .unwrap_err();
        let name = file.display();
        assert_eq!(
            e,
            format!(
                "error: Board: seed 3 (the default: {name} states no seed) does not place the board: the placer tried seed 3 (the courtyards of `usb` (J2) and `c37` (C37) overlap by 0.42 mm2); seed 4 places it, and --locked says {name} must not be written: run without --locked to write `seed = 4` to it, and commit it\n"
            )
        );
        assert!(!file.exists());
    }

    #[test]
    fn the_remake_gets_what_is_left_of_the_budget_or_a_stated_floor() {
        use crate::pin_swap::{Budget, Clock};
        use std::sync::atomic::{AtomicU64, Ordering};
        let secs = Duration::from_secs;
        // A clock that has moved `elapsed` seconds since the budget started.
        let at = |total: Option<Duration>, elapsed: u64| {
            let now = std::sync::Arc::new(AtomicU64::new(0));
            let clock = {
                let now = now.clone();
                Clock::from_fn(move || Duration::from_secs(now.load(Ordering::SeqCst)))
            };
            let budget = Budget::start(total, &clock);
            now.store(elapsed, Ordering::SeqCst);
            budget
        };
        // Time left: the remake gets that, not the whole timeout again.
        assert_eq!(
            remake_timeout(&at(Some(secs(120)), 100), Some(secs(120))),
            Some(secs(20))
        );
        // Spent: the floor, but never more than the timeout itself.
        assert_eq!(
            remake_timeout(&at(Some(secs(120)), 130), Some(secs(120))),
            Some(REMAKE_FLOOR)
        );
        assert_eq!(
            remake_timeout(&at(Some(secs(3)), 9), Some(secs(3))),
            Some(secs(3))
        );
        // Nearly spent: a second of 300 s left is no time to route in, and the router cut off
        // after it leaves the board worse than the pass before. The floor applies below it too.
        assert_eq!(
            remake_timeout(&at(Some(secs(300)), 299), Some(secs(300))),
            Some(REMAKE_FLOOR)
        );
        assert_eq!(
            remake_timeout(&at(Some(secs(300)), 291), Some(secs(300))),
            Some(REMAKE_FLOOR),
            "9 s left is below the floor"
        );
        assert_eq!(
            remake_timeout(&at(Some(secs(300)), 290), Some(secs(300))),
            Some(secs(10)),
            "exactly the floor is what is left"
        );
        assert_eq!(
            remake_timeout(&at(Some(secs(300)), 280), Some(secs(300))),
            Some(secs(20))
        );
        assert_eq!(
            remake_timeout(&at(Some(secs(3)), 2), Some(secs(3))),
            Some(secs(3)),
            "never more than the timeout itself"
        );
        // No limit stays none.
        assert_eq!(remake_timeout(&at(None, 500), None), None);
    }

    #[test]
    fn a_remake_that_leaves_more_connections_does_not_replace_the_files_before_it() {
        // Fewer or as many connections left: the remake is as good, and the pin file's map
        // matches the files.
        assert!(!keep_earlier_files(Some(2), Some(3)));
        assert!(!keep_earlier_files(Some(3), Some(3)));
        assert!(!keep_earlier_files(Some(0), Some(3)));
        // More: the files before it stay.
        assert!(keep_earlier_files(Some(4), Some(3)));
        // No board made: nothing to put in their place.
        assert!(keep_earlier_files(None, Some(3)));
        // What is said of it depends on which happened.
        assert!(why_earlier_files_stay(Some(4)).contains("left more connections"));
        assert!(why_earlier_files_stay(None).contains("made no board"));
        assert!(!why_earlier_files_stay(None).contains("left more connections"));
        assert!(keep_earlier_files(None, None));
        // The files before are not known: the remake is not held against them.
        assert!(!keep_earlier_files(Some(9), None));
    }

    #[test]
    fn the_files_set_aside_come_back_whole() {
        let root = netsmith_testkit::TempDir::new("fab-set-aside");
        let dir = root.join("Board");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Board.kicad_pcb"), "earlier").unwrap();
        fs::write(dir.join("kept-only-here.txt"), "earlier").unwrap();
        let aside = set_aside(&dir).expect("a folder is moved aside");
        assert!(!dir.exists());
        // A pass writes the folder afresh, and less of it.
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Board.kicad_pcb"), "worse").unwrap();
        set_back(&dir, &aside).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("Board.kicad_pcb")).unwrap(),
            "earlier"
        );
        assert!(dir.join("kept-only-here.txt").is_file());
        assert!(!aside.exists());
        // No folder, nothing to set aside.
        assert!(set_aside(&root.join("Nothing")).is_none());
    }
    #[test]
    fn the_note_without_kicad_says_netsmiths_own_check_still_runs() {
        let note = super::no_kicad_note(&netsmith_kicad::Search::under("/nowhere"));
        assert!(note.contains("KiCad is not installed"), "{note}");
        assert!(note.contains("--cross-check"), "{note}");
        assert!(
            note.contains("netsmith's own design rules check still runs"),
            "{note}"
        );
        assert!(!note.contains("none of its own"), "{note}");
    }

    #[test]
    fn the_note_says_when_kicad_is_switched_off_and_not_that_it_is_not_installed() {
        let note = super::no_kicad_note(&netsmith_kicad::Search::under("/nowhere").with_no_kicad());
        assert!(
            note.contains("KiCad is switched off (NETSMITH_NO_KICAD is set)"),
            "{note}"
        );
        assert!(!note.contains("not installed"), "{note}");
        assert!(
            note.contains("netsmith's own design rules check still runs"),
            "{note}"
        );
    }

    use super::*;

    fn sourced(
        refdes: &str,
        kind: &str,
        value: &str,
        package: Option<&str>,
        lcsc: Option<&str>,
        assembly: Assembly,
    ) -> Sourced {
        Sourced {
            refdes: refdes.into(),
            kind: kind.into(),
            value: value.into(),
            package: package.map(Into::into),
            lcsc: lcsc.map(Into::into),
            mpn: None,
            library: None,
            assembly,
            assembly_stated: false,
            catalog_height: None,
            generic: None,
            resolved_row: None,
        }
    }

    #[test]
    fn identical_parts_share_one_line_with_designators_in_order() {
        let entries = bom_entries(
            &[
                sourced(
                    "C10",
                    "Capacitor",
                    "100n",
                    Some("0402"),
                    Some("C1525"),
                    Assembly::Fab,
                ),
                sourced(
                    "R1",
                    "Resistor",
                    "330",
                    Some("0402"),
                    Some("C25104"),
                    Assembly::Fab,
                ),
                sourced(
                    "C2",
                    "Capacitor",
                    "100n",
                    Some("0402"),
                    Some("C1525"),
                    Assembly::Fab,
                ),
            ],
            &Mapping::standard(),
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].designators, ["C2", "C10"]);
        assert_eq!(
            entries[0].footprint, "C_0402_1005Metric",
            "the library prefix is dropped"
        );
        assert_eq!(entries[0].lcsc.as_deref(), Some("C1525"));
        assert_eq!(entries[1].designators, ["R1"]);
    }

    #[test]
    fn parts_that_are_not_assembled_stay_out() {
        let entries = bom_entries(
            &[
                sourced("J1", "Header1x2", "Conn_01x02", None, None, Assembly::Hand),
                sourced(
                    "R1",
                    "Resistor",
                    "330",
                    Some("0402"),
                    Some("C25104"),
                    Assembly::Fab,
                ),
            ],
            &Mapping::standard(),
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].designators, ["R1"]);
    }

    #[test]
    fn an_assembled_part_without_a_number_is_kept_so_it_can_be_refused_by_name() {
        let entries = bom_entries(
            &[sourced("D1", "Led", "LED", None, None, Assembly::Fab)],
            &Mapping::standard(),
        )
        .unwrap();
        assert_eq!(entries[0].lcsc, None);
    }

    #[test]
    fn the_same_value_with_different_part_numbers_stays_two_lines() {
        let entries = bom_entries(
            &[
                sourced(
                    "R1",
                    "Resistor",
                    "1k",
                    Some("0402"),
                    Some("C11702"),
                    Assembly::Fab,
                ),
                sourced(
                    "R2",
                    "Resistor",
                    "1k",
                    Some("0402"),
                    Some("C99999"),
                    Assembly::Fab,
                ),
            ],
            &Mapping::standard(),
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
    }
    fn line(
        net: &str,
        from: (&str, &str, f64, f64),
        to: (&str, &str, f64, f64),
    ) -> netsmith_pcb::Ratline {
        let pad = |(refdes, number, _, _): (&str, &str, f64, f64)| netsmith_pcb::PadRef {
            refdes: refdes.into(),
            number: number.into(),
        };
        netsmith_pcb::Ratline {
            net: net.into(),
            from: pad(from),
            to: pad(to),
            from_at: netsmith_pcb::Pt::new(from.2, from.3),
            to_at: netsmith_pcb::Pt::new(to.2, to.3),
            to_plane: false,
        }
    }

    #[test]
    fn the_routing_failure_lists_each_connection_with_its_net_pads_and_places() {
        let left = netsmith_pcb::Ratsnest {
            lines: vec![
                line("SIG_A", ("U1", "A3", 15.0, 25.5), ("R1", "1", 12.0, 21.0)),
                line("SIG_B", ("U1", "C1", 16.1, 25.5), ("J1", "2", 30.0, 5.0)),
            ],
        };
        let text = routing_failure("Demo", netsmith_pcb::Pt::new(10.0, 20.0), &left, 4, &[]);
        assert!(
            text.starts_with(
                "INCOMPLETE: Demo is not fully routed. 2 connection(s) have no copper after 4 tries"
            ),
            "{text}"
        );
        assert!(
            text.contains("net SIG_A: U1.A3 at (5.00, 5.50) to R1.1 at (2.00, 1.00)\n"),
            "{text}"
        );
        assert!(
            text.contains("net SIG_B: U1.C1 at (6.10, 5.50) to J1.2 at (20.00, -15.00)\n"),
            "{text}"
        );
        assert!(
            text.contains("  U1: 2\n") && text.contains("  R1: 1\n"),
            "{text}"
        );
        assert!(!text.contains("Why each line is left"), "{text}");
    }

    #[test]
    fn the_routing_failure_says_where_each_line_failed_and_what_is_in_its_way() {
        let left = netsmith_pcb::Ratsnest {
            lines: vec![
                line("SIG_A", ("U1", "A3", 15.0, 25.5), ("R1", "1", 12.0, 21.0)),
                line("SIG_B", ("U1", "C1", 16.1, 25.5), ("J1", "2", 30.0, 5.0)),
            ],
        };
        let why = vec![
            netsmith_router::LineReport {
                line: left.lines[0].clone(),
                stage: netsmith_router::Stage::Chain,
                blockers: vec!["GND".into(), "SIG_C".into()],
            },
            netsmith_router::LineReport {
                line: left.lines[1].clone(),
                stage: netsmith_router::Stage::OutOfTime,
                blockers: vec![],
            },
        ];
        let text = routing_failure("Demo", netsmith_pcb::Pt::new(10.0, 20.0), &left, 4, &why);
        // What readers match comes first, as it was.
        assert_eq!(
            text,
            routing_failure("Demo", netsmith_pcb::Pt::new(10.0, 20.0), &left, 4, &[])
                + &text[text.find("\nWhy each line is left").unwrap()..],
        );
        assert_eq!(text.matches("\nnet ").count(), 2, "{text}");
        assert!(
            text.contains(
                "  net SIG_A: U1.A3 to R1.1: failed in the chain of rip-ups; in its way: net(s) GND, SIG_C\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("  net SIG_B: U1.C1 to J1.2: not finished, the time ran out; in its way: no net's copper"),
            "{text}"
        );
        assert!(text.contains("routing-report.json"), "{text}");
    }

    #[test]
    fn an_incomplete_board_writes_its_report_and_the_error_says_where_everything_is() {
        let dir = netsmith_testkit::TempDir::new("incomplete");
        let incomplete = Incomplete {
            message: "1 connections could not be routed (after 4 tries):\n  net A: U1.1 to R1.1"
                .into(),
            report: "INCOMPLETE: Demo\n".into(),
            json: "{}\n".into(),
            lines: vec!["ERROR: not routed: net A: U1.1 to R1.1".into()],
        };
        // The package could not be made (for instance a requirement is unmet): the report is
        // still written and the error says why there are no Gerbers.
        let message = write_incomplete(
            &incomplete,
            &dir,
            Err(netsmith_fab::Error::Io("no package".into())),
        );
        assert!(
            message.starts_with("1 connections could not be routed"),
            "{message}"
        );
        assert!(
            message.contains("INCOMPLETE: the board file is in")
                && message.contains("the Gerbers were not made: no package")
                && message.contains("routing-failure.txt"),
            "{message}"
        );
        assert_eq!(
            fs::read_to_string(dir.join("routing-failure.txt")).unwrap(),
            "INCOMPLETE: Demo\n"
        );
        assert_eq!(
            fs::read_to_string(dir.join("routing-report.json")).unwrap(),
            "{}\n"
        );
        assert_eq!(
            incomplete.headline(),
            "1 connection(s) have no copper; this package is NOT ready to order, see routing-failure.txt"
        );
    }

    /// The fields of a process that are not a rule the router and the design rules check keep,
    /// each with the reason (issue #431).
    const NOT_A_COPPER_RULE: [(&str, &str); 18] = [
        (
            "hole_plating",
            "the track width check rates a via for current by it (issue #431); the counts it gives are the router's",
        ),
        (
            "track_width_tolerance",
            "the track width check sizes for the narrowest finished track (issue #431); the widths it gives are the router's",
        ),
        (
            "min_plane_hole_clearance",
            "kept by the plane's fill, which the check does not judge (ADR 0037)",
        ),
        ("inner1", "a plane or a signal layer, which the board says"),
        ("inner2", "a plane or a signal layer, which the board says"),
        ("inner3", "a plane or a signal layer, which the board says"),
        ("inner4", "a plane or a signal layer, which the board says"),
        ("inner5", "a plane or a signal layer, which the board says"),
        ("inner6", "a plane or a signal layer, which the board says"),
        ("inner7", "a plane or a signal layer, which the board says"),
        ("inner8", "a plane or a signal layer, which the board says"),
        ("fabricator", "a name, for the notes"),
        ("source", "where the numbers are published, for the notes"),
        (
            "stackup",
            "the build of the board: the writers' and the current check's",
        ),
        ("top", "a pour, which the board names the net of"),
        ("bottom", "a pour, which the board names the net of"),
        (
            "max_temperature_rise",
            "the track width check's rise (ADR 0036), not a distance",
        ),
        (
            "min_silk_width",
            "checked as the text's stroke (`min_text_thickness`) only; a drawn line is the footprint's",
        ),
    ];

    /// The fields declared at the top level of `interface <name>` (or `generic <name>`) in
    /// `std/mfg/jlcpcb.nsm` or `std/mfg/acme.nsm`, with the value its `&=` or `?=` line gives,
    /// if any, or the line of `interface <provider> provides <name>` (ADR 0113).
    fn process_fields(name: &str, provider: Option<&str>) -> Vec<(String, Option<String>)> {
        let (_, text) = std_module(name);
        let block = |header: &str| -> &str {
            let start = text.find(header).expect("the process is in the file");
            let body = &text[start..];
            &body[..body.find("\n}").expect("the definition ends")]
        };
        let body = if text.contains(&format!("generic {name} {{")) {
            block(&format!("generic {name} {{"))
        } else {
            block(&format!("interface {name} {{"))
        };
        let own = provider.map(|p| block(&format!("interface {p} provides {name} {{")));
        let mut fields: Vec<(String, Option<String>)> = Vec::new();
        for line in body.lines().chain(own.into_iter().flat_map(str::lines)) {
            let Some(rest) = line.strip_prefix("    .") else {
                continue;
            };
            let word: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let after = rest[word.len()..].trim_start();
            if after.starts_with(':') {
                fields.push((word, None));
            } else if let Some(value) = after
                .strip_prefix("&=")
                .or_else(|| after.strip_prefix("?="))
                && let Some(f) = fields.iter_mut().find(|f| f.0 == word)
            {
                f.1 = Some(value.trim().to_string());
            }
        }
        fields
    }

    /// The std module that defines the process `name`, and its text.
    fn std_module(name: &str) -> (&'static str, &'static str) {
        if matches!(name, "EightLayer" | "TenLayer") {
            ("acme", include_str!("../../../std/mfg/acme.nsm"))
        } else {
            ("jlcpcb", include_str!("../../../std/mfg/jlcpcb.nsm"))
        }
    }

    /// The process of a board that states each of `narrowings` inside `.process { }`. A name
    /// with a weight, `TwoLayer@2`, is `TwoLayer` at that copper weight (ADR 0113).
    fn process(name: &str, narrowings: &[String]) -> netsmith_core::Process {
        let mut lines: Vec<String> = narrowings.to_vec();
        let name = match name.split_once('@') {
            Some((name, weight)) => {
                lines.push(format!(".stackup.copper_weight &= {weight}"));
                name
            }
            None => name,
        };
        let inner = match name {
            "FourLayer" => 2,
            "SixLayer" => 4,
            "EightLayer" => 6,
            "TenLayer" => 8,
            _ => 0,
        };
        let nets = if inner > 0 {
            lines.push(".inner1.net &= \"GND\"".into());
            lines.push(".inner2.net &= \"3V3\"".into());
            for n in 3..=inner {
                lines.push(format!(".inner{n}.use &= \"signal\""));
            }
            "        .a.net &= \"GND\"\n        .b.net &= \"3V3\"\n"
        } else {
            lines.push(".stackup.thickness &= 1.6mm".into());
            ""
        };
        let body: String = lines.iter().map(|l| format!("        {l}\n")).collect();
        let (module, _) = std_module(name);
        let source = format!(
            "import std::mfg::{module}::{name}\nimport std::parts::passive::Resistor\nboard T {{\n    .process: {name} {{\n{body}    }}\n    .r: Resistor {{\n{nets}    }}\n}}\n"
        );
        netsmith_pipeline::build_with(
            "t.nsm",
            &source,
            &netsmith_check::Loader::std(),
            &netsmith_solve::Catalogs::jlcpcb(),
            &netsmith_pipeline::Libraries::sample(),
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .remove(0)
        .design
        .process
        .expect("the board has a process")
    }

    /// What the router keeps and what the design rules check is given for a process: the
    /// router's rules, and the project file with the extras `fab` passes.
    fn what_is_kept(p: &netsmith_core::Process) -> Result<(String, String), String> {
        let rules = netsmith_pcb::Rules::for_process(p)?;
        let board_extras = netsmith_geom::DrcExtras {
            mask_web: Some(rules.mask_bridge),
            via_in_pad: Some(p.via_in_pad),
            hole_to_track: Some(rules.hole_clearance),
            plated_hole_clearance: Some(rules.plated_hole_clearance),
            pad_to_pad: Some(rules.pad_to_pad),
            pad_hole_to_hole: Some(rules.hole_to_hole),
            mask_to_track: Some(rules.mask_to_track),
            plated_slot: Some(rules.plated_slot),
            npth_slot: Some(rules.npth_slot),
            npth_drill: Some(rules.npth_drill),
            pad_ring: Some(rules.min_annular_ring),
            ..Default::default()
        };
        let check = format!("{}\n{board_extras:?}", rules.kicad_pro("T", &[]));
        Ok((format!("{rules:?}"), check))
    }

    /// Every rule a process states reaches both the router and the design rules check: a
    /// little more of it changes what each keeps, or makes the router refuse the process (a
    /// rule of the via, which the via the process offers must meet). A field that is not such
    /// a rule is listed with its reason (issue #431).
    #[test]
    fn every_rule_a_process_states_reaches_the_router_and_the_check() {
        // Each provider of a generic process, by the weight that picks it (ADR 0113).
        for (name, provider) in [
            ("TwoLayer", Some("JlcTwoLayer1oz")),
            ("TwoLayer@2", Some("JlcTwoLayer2oz")),
            ("FourLayer", None),
            ("SixLayer", None),
            ("EightLayer", None),
            ("TenLayer", None),
        ] {
            let base = what_is_kept(&process(name, &[])).unwrap();
            let generic = name.split('@').next().expect("a name");
            for (field, value) in process_fields(generic, provider) {
                if NOT_A_COPPER_RULE.iter().any(|(f, _)| *f == field) {
                    continue;
                }
                let value = value.unwrap_or_else(|| panic!("{name}.{field} states no value"));
                // A lower bound is raised by 0.01 mm, an upper bound lowered, a value moved.
                let number: f64 = value
                    .trim_start_matches("0mm..")
                    .split("mm")
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_else(|| panic!("{name}.{field}: cannot read {value}"));
                let narrowed = if value.ends_with("..1m") {
                    format!(".{field} &= {}mm..1m", number + 0.01)
                } else if value.starts_with("0mm..") {
                    format!(".{field} &= 0mm")
                } else {
                    format!(".{field} &= {}mm", number + 0.05)
                };
                let after = what_is_kept(&process(name, std::slice::from_ref(&narrowed)));
                match after {
                    Err(_) => {}
                    Ok((router, check)) => {
                        assert_ne!(
                            router, base.0,
                            "{name}: `{narrowed}` does not change the router's rules"
                        );
                        assert_ne!(
                            check, base.1,
                            "{name}: `{narrowed}` does not reach the design rules check"
                        );
                    }
                }
            }
        }
    }
}
