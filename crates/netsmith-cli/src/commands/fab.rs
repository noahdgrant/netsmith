use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args as ClapArgs, ValueEnum};
use netsmith_core::timings;
use netsmith_fab::{KicadCli, KicadCliExe};
use netsmith_fabricate::fab::{FabOptions, fab_detailed};
use netsmith_fabricate::order::OrderLocks;
use netsmith_fabricate::project::Target;
use netsmith_tools::findings::{BoardOut, Finding, Format, NotesMode, Report, Severity};

use super::common::{self, libraries, no_boards, sync_pins, target_offline};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "With no file, `fab` acts on the project's boards (see netsmith.toml).\n\n`fab` writes the Gerbers, drill files and placement file itself and runs netsmith's own design rules check (ADR 0037), so it does not need KiCad. An error of the check (a clearance, a short, a missing connection, a hole, the edge, a courtyard, a keepout) stops `fab`: the first line of the message says that no package was written and where the KiCad board is, with what each of two flags does. `--allow-drc-violations` packages the board anyway to order from, and `fab-notes.txt` then says the board is not clean. `--for-review` writes the files to look at instead (the KiCad board and project, `<board>-gerbers-REVIEW.zip`, the SVG, the `.glb` and `.step`, the schematic PDF and `fab-notes.txt` with the findings), each marked FOR REVIEW, NOT FOR ORDER, with no `bom.csv` and no `cpl.csv`; the run still exits 1 while the check has errors, and `project order` refuses such a package. The warnings of the check (silkscreen on a pad, a narrow mask web, a via in a pad) are listed in the notes. It writes, for each board, a Gerber zip, `bom.csv`, `cpl.csv`, `fab-notes.txt`, the `.kicad_pcb` and `.kicad_pro`, an `.svg`, and the board's 3D scene as `<board>.glb` (outside the Gerber zip; the scene of the routed board, the bytes `dev board3d --glb` writes for it; docs/BOARD3D.md), and the board as STEP, `<board>.step`, for mechanical CAD (outside the Gerber zip; the bytes `dev board3d --step` writes). Read `fab-notes.txt` before you order.\n\n`--cross-check` checks the Gerbers against the board, also with no KiCad: each copper layer of the zip, drawn as `view gerber` draws it, is compared pixel by pixel with the board model as `view pcb` draws it (40 pixels a millimetre; at most 1% of the pixels differ and 0.1% beyond 0.05 mm). A difference stops `fab` and lists each place as a `--region` box to draw with both commands; the result is in `fab-notes.txt` and, with `--format json`, a finding of code `fab`. With KiCad installed the KiCad cross-check runs as well (`cross-check-<file>.png` shows each layer that differs); without it the viewer comparison runs alone, and `fab-notes.txt` says so. A panel's cross-check is KiCad's alone.\n\nA board the router cannot finish is still written, marked INCOMPLETE, and the run ends in an error. Beside it, `routing-failure.txt` lists each connection left with its net, its two pads and their places and the parts most of them end on, and then, for each line the router left, the stage it last failed in (the first pass, the try with less room, ripping up, the negotiation, the chain of rip-ups, the rounds that rip up whole nets, or out of time) and the nets whose copper stands in its way, or, where no path crosses only other nets' copper, the pads, vias and edge that wall it in; `routing-report.json` has the same for a program, with a crowding map of the board (1 mm squares, how often the router's searches across other nets' copper passed through each), which shows where the board is fought over.\n\nWith `--format json` the progress goes nowhere unless `--progress json` prints it as JSON lines on stderr, and the notes of `fab-notes.txt` (\"before you order\") are findings with the code `fab`. A finding about requirements of the physical file that are not met has them as data in `requirements` (the requirement, what was measured, the window it allows and how far out it is), and the refusal because the parts do not fit has every overlapping pair, every part outside and every part's final place in `does_not_fit`. The lines of the physical file and locks of the layout file that cannot all hold are in `conflicts` (each with its kind, the lines, the parts with the box each courtyard lies within, and the overlap); a courtyard past an edge nobody stated and a part bigger than the outline (`past_edge`, `too_big`), proved before placing, are there too. Each line of a conflict has the window it allows (`measure`, `unit`, `min` and `max`, or `allowed_side`). With constraints as warnings, the lines that cannot all hold are findings of code `place` (the code `board build --format json` gives what is proved before placing) with their `conflicts`. The faults of the placement check where the placer ended are in `placement` (each with its kind, the parts with their courtyards, and its numbers). A lock of the layout file that breaks a requirement has `requirements` and `placement` of the board as the lock leaves it.\n\nThe height check cannot check a part of unknown height: one warning, `height_unknown`, on stderr and in `fab-notes.txt`, lists every such part that stands under a height limit or under another part's body, with the limit that would apply (for a body that states no `.standoff`, the limit its standoff would put over the part). It is a warning always: `fab` still passes. State each part's `.height` to leave it (docs/language/23-the-shape-of-a-board.md, _Height limits_)."
)]
pub struct Args {
    /// The source file; with none, the project's boards (see netsmith.toml).
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The folder to write into.
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// Seconds the router may take for a board; 0 is no limit (default 300).
    ///
    /// When the time is spent, `fab` stops and lists the connections left. The default is 300.
    #[arg(long, value_name = "SECS")]
    pub route_timeout: Option<u64>,
    /// Rounds of rip-up and re-route of whole nets for connections the router leaves; 0 is none.
    ///
    /// When a try of the router ends with connections open, each round routes every one of them
    /// through the copper of the nets in its way, removes those nets and routes them again after
    /// it; one net is ripped up at most 3 times a try. The default is 6; `rip_rounds` in
    /// netsmith.toml sets it for a project, and this wins. It runs inside `--route-timeout`.
    #[arg(long, value_name = "N")]
    pub rip_rounds: Option<usize>,
    /// Package a board that fails the design rules check (never an unmet `[constraint]`).
    ///
    /// `fab-notes.txt` then says the board is not clean. It never waives a `[constraint]` finding:
    /// a requirement on where a part goes is relaxed only by `constraints = "warn"` in
    /// netsmith.toml or `--constraints warn`.
    #[arg(long)]
    pub allow_drc_violations: bool,
    /// Write the files to look at of a board that fails the design rules check, marked FOR REVIEW.
    ///
    /// Without a flag the check stops `fab` and no package is written; `--allow-drc-violations`
    /// packages the board to order from, and this packages it to look at. It writes the KiCad
    /// board and project, `<board>-gerbers-REVIEW.zip`, the SVG, the `.glb` and `.step`, the
    /// schematic PDF and `fab-notes.txt` with the findings, each marked FOR REVIEW, NOT FOR
    /// ORDER, and no `bom.csv` and no `cpl.csv`. The run exits 1 while the check has errors, and
    /// `project order` refuses the package. It waives the findings about where parts go too, which
    /// `--allow-drc-violations` never does. A board that passes is packaged as usual. It takes
    /// neither `--allow-drc-violations` nor `--cross-check`, and `project order` does not take it.
    #[arg(long, conflicts_with_all = ["allow_drc_violations", "cross_check"])]
    pub for_review: bool,
    /// Check the Gerbers against the board, with netsmith's own viewer and with KiCad's own plot and check.
    ///
    /// Each copper layer of the Gerbers is drawn as `view gerber` draws it and compared with the board as `view pcb` draws it (the limits are in the language guide); a difference stops `fab`, lists each place as a `--region` box and is a finding of `fab-notes.txt`. This needs no KiCad. With KiCad's command-line tool installed (`$KICAD_CLI`, then `kicad-cli` or `kicad-cli.exe` on `PATH`, then Windows KiCad 9 under WSL) the files and the findings are compared with KiCad's own plot and check too; without it only the viewer comparison runs, and `fab-notes.txt` says so.
    #[arg(long)]
    pub cross_check: bool,
    /// Make a guideline finding an error for this run.
    #[arg(long)]
    pub strict: bool,
    /// Stop when the pick lock or the pin file would change; implies `--pins locked`.
    ///
    /// With `fab` it also means `--pins locked`, so a board that does not route never has its pin
    /// file rewritten (`--locked` wins over `--pins auto`).
    #[arg(long)]
    pub locked: bool,
    /// Whether an unmet requirement on where a part goes stops `fab`, over netsmith.toml.
    ///
    /// It wins over `constraints` in netsmith.toml for this run.
    #[arg(long, value_enum)]
    pub constraints: Option<Constraints>,
    /// What a board that does not route may do with its pins, over netsmith.toml (ADR 0101).
    ///
    /// `pins` in netsmith.toml sets it for a project; the default is `auto`.
    #[arg(long, value_enum)]
    pub pins: Option<Pins>,
    /// Leave the board's 3D scene (`<board>.glb`) out of the package.
    ///
    /// `glb = false` in netsmith.toml does it for a project; a stale `<board>.glb` of an earlier
    /// run is removed. A system's scene (`<System>/<System>.glb`, its boards placed against each
    /// other by their mates, ADR 0110) is left out with it.
    #[arg(long)]
    pub no_glb: bool,
    /// Leave the board as STEP (`<board>.step`) out of the package.
    ///
    /// `step = false` in netsmith.toml does it for a project; a stale `<board>.step` of an
    /// earlier run is removed. A system's STEP (`<System>/<System>.step`) is left out with it.
    #[arg(long)]
    pub no_step: bool,
    /// Leave the board's schematic (`<board>-schematic.pdf`) out of the package.
    ///
    /// `pdf = false` in netsmith.toml does it for a project; a stale `<board>-schematic.pdf` of
    /// an earlier run is removed. A system's (`<System>/<System>-schematic.pdf`) is left out with
    /// it.
    #[arg(long)]
    pub no_pdf: bool,
    /// Also write `<board>.placed.json`, the board before routing, for `netsmith dev route`.
    ///
    /// The file goes into the board's output folder, with the track margin `fab` routes with.
    /// Nothing else of the package changes, and a run without it removes the file an earlier run
    /// saved. `project order` does not take it.
    #[arg(long)]
    pub save_placed: bool,
    /// Start each route from the router's copper this machine kept, and keep it again.
    ///
    /// The editor keeps the copper it routes in `.netsmith/routes/<stem>.<board>.toml` in the
    /// project's folder, which is not committed. With this flag the copper of every net the
    /// placement left alone is kept and only the rest is routed; a kept file of another router,
    /// other routes-file copper or other design rules is not used. Without it `fab` reads and
    /// writes no kept file, so a clean checkout gives the same board. `project order` does not
    /// take it.
    #[arg(long)]
    pub reuse_routes: bool,
    /// Write the KiCad files as a self-contained project: libraries and 3D models copied in.
    ///
    /// The symbols and footprints the board uses (netsmith's own, the project's, and KiCad's
    /// library entries it references) are copied into `libs/` of the board's folder, its 3D
    /// models (a part's `.model` STEP file and the libraries' models) into `models/`, and the
    /// library tables and each footprint's `(model ...)` name them by paths inside the folder,
    /// so the folder opens in KiCad on any machine. `LICENSES.md` carries the notice that
    /// KiCad's libraries (CC-BY-SA 4.0 with a design exception) ask for. A model that cannot be
    /// found is a warning and the footprint has none. `project order` does not take it.
    #[arg(long)]
    pub portable: bool,
    /// Ask no supplier for stock or price; say in `fab-notes.txt` that they were not checked.
    ///
    /// No provider that would use the network is asked, and an answer kept earlier is used at any
    /// age. `fab-notes.txt` (and `order.txt` of an order) then says that the stock and price of
    /// the parts were not checked, so look each part up at the supplier before you order.
    #[arg(long)]
    pub offline: bool,
    /// Print the result of each board and the findings, not the stages.
    #[arg(long)]
    pub quiet: bool,
    #[command(flatten)]
    pub placing: common::Placing,
    #[command(flatten)]
    pub watching: common::Watching,
    #[command(flatten)]
    pub output: common::Output,
}

impl Args {
    /// `Some(true)` lists an unmet requirement on where a part goes instead of stopping the run,
    /// `Some(false)` stops even if the manifest says `warn`, `None` leaves it to the manifest.
    pub fn constraints_warn(&self) -> Option<bool> {
        self.constraints.map(|c| c == Constraints::Warn)
    }

    /// `Some(true)` leaves every pin and writes the proposal, `Some(false)` lets a board that does
    /// not route have its unlocked pins swapped, `None` leaves it to the manifest.
    pub fn pins_locked(&self) -> Option<bool> {
        self.pins.map(|p| p == Pins::Locked)
    }
}

/// Makes the fab package of each board.
pub fn run(args: Args) -> ExitCode {
    let format: Format = args.output.format.into();
    if let Some(code) = sync_pins(args.file.as_deref(), args.locked) {
        return code;
    }
    // `--format json` keeps stdout for its document and prints the progress only when it is
    // asked for as JSON too (issue #764).
    args.watching.tell_progress(
        "fab",
        args.quiet
            || (format == Format::Json && args.watching.progress == crate::progress::Print::Text),
    );
    let code = common::with_stage_events(|| run_chosen(&args, format));
    crate::progress::ended(
        code == ExitCode::SUCCESS,
        if code == ExitCode::SUCCESS {
            "fab: done"
        } else {
            "fab: failed (its output says why)"
        },
    );
    code
}

/// `fab` of the files or the project's boards.
fn run_chosen(args: &Args, format: Format) -> ExitCode {
    let (constraints_warn, pins_locked) = (args.constraints_warn(), args.pins_locked());
    timings::stage("fab.total", "", || {
        run_fab(
            args.file.as_deref(),
            args.out.as_deref(),
            args.route_timeout,
            args.rip_rounds,
            args.allow_drc_violations,
            args.for_review,
            args.cross_check,
            args.strict,
            args.locked,
            constraints_warn,
            pins_locked,
            args.no_glb,
            args.no_step,
            args.no_pdf,
            args.save_placed,
            args.reuse_routes,
            args.offline,
            args.portable,
            &args.placing,
            FabOutput {
                quiet: args.quiet,
                format,
                notes: args.output.notes.into(),
            },
        )
    })
}

/// Whether constraints were asked to be strict, for this target and this run's
/// `--constraints` (see [`netsmith_fabricate::fab::constraints_strict`]).
/// The project's `track_margin`, or the default.
fn track_margin(target: &Target) -> f64 {
    target
        .project
        .as_ref()
        .and_then(|p| p.manifest.track_margin)
        .unwrap_or(netsmith_fabricate::fab::DEFAULT_TRACK_MARGIN)
}

/// The rounds of rip-up the router gives a try that leaves connections: `--rip-rounds` for this
/// run, else the project's `rip_rounds`, else the router's own (issue #1088).
fn rip_rounds_of(target: &Target, flag: Option<usize>) -> usize {
    flag.unwrap_or_else(|| netsmith_fabricate::fab::rip_rounds(target.project.as_ref()))
}

/// Whether `fab` writes the board's 3D scene: `--no-glb` for this run leaves it out, else `glb`
/// in the manifest, else yes.
fn glb_wanted(target: &Target, no_glb: bool) -> bool {
    !no_glb
        && target
            .project
            .as_ref()
            .and_then(|p| p.manifest.glb)
            .unwrap_or(true)
}

/// Whether `fab` writes the board's schematic as a PDF: `--no-pdf` for this run leaves it out,
/// else `pdf` in the manifest, else yes.
fn pdf_wanted(target: &Target, no_pdf: bool) -> bool {
    !no_pdf
        && target
            .project
            .as_ref()
            .and_then(|p| p.manifest.pdf)
            .unwrap_or(true)
}

/// What the schematic PDF of `source` is drawn with: the schematic layout file beside it and
/// the project's name and revision.
fn schematic_inputs(target: &Target, source: &Path) -> netsmith_fabricate::schematic::Inputs {
    netsmith_fabricate::schematic::Inputs::of(source, target.project.as_ref())
}

/// Whether `fab` writes the board as STEP: `--no-step` for this run leaves it out, else `step`
/// in the manifest, else yes.
fn step_wanted(target: &Target, no_step: bool) -> bool {
    !no_step
        && target
            .project
            .as_ref()
            .and_then(|p| p.manifest.step)
            .unwrap_or(true)
}

fn strict_heights(target: &Target, flag: Option<bool>) -> bool {
    netsmith_fabricate::fab::constraints_strict(
        target
            .project
            .as_ref()
            .and_then(|p| p.manifest.constraints.as_deref()),
        flag,
    )
}

/// What a board that does not route may do with its pins: `--locked` forbids any change, so it is
/// `locked` whatever else is said; else `--pins` for this run, else `pins` in the manifest, else
/// `auto` (ADR 0101). `--locked` keeps the working tree as it is, the pin file included. The
/// second value is what made it `locked`, for the messages.
fn pins_mode(
    target: &Target,
    flag: Option<bool>,
    locked: bool,
) -> (
    netsmith_fabricate::fab::PinsMode,
    netsmith_fabricate::fab::LockedBy,
) {
    use netsmith_fabricate::fab::{LockedBy, PinsMode};
    if locked {
        return (PinsMode::Locked, LockedBy::LockedFlag);
    }
    match flag {
        Some(true) => (PinsMode::Locked, LockedBy::PinsFlag),
        Some(false) => (PinsMode::Auto, LockedBy::PinsFlag),
        None => match target
            .project
            .as_ref()
            .and_then(|p| p.manifest.pins.as_deref())
            .and_then(PinsMode::parse)
        {
            Some(PinsMode::Locked) => (PinsMode::Locked, LockedBy::Manifest),
            Some(PinsMode::Auto) | None => (PinsMode::Auto, LockedBy::PinsFlag),
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn run_fab(
    file: Option<&str>,
    out: Option<&Path>,
    route_timeout: Option<u64>,
    rip_rounds: Option<usize>,
    allow_drc_violations: bool,
    for_review: bool,
    cross_check: bool,
    strict: bool,
    locked: bool,
    constraints_warn: Option<bool>,
    pins_locked: Option<bool>,
    no_glb: bool,
    no_step: bool,
    no_pdf: bool,
    save_placed: bool,
    reuse_routes: bool,
    offline: bool,
    portable: bool,
    placing: &common::Placing,
    output: FabOutput,
) -> ExitCode {
    let json = output.format == Format::Json;
    let mut report = Report::new("fab");
    let (target, loader, mut catalogs) = match target_offline(file, strict, offline) {
        Ok(t) => t,
        Err(code) => return code,
    };
    // `--constraints warn|error` is for this run and wins over the manifest's `constraints`
    // (ADR 0045).
    if let Some(warn) = constraints_warn {
        catalogs = catalogs.with_constraints_warn(warn);
    }
    if no_boards(&target) {
        return ExitCode::from(2);
    }
    if let Err(code) = common::write_dependency_lock(&target) {
        return code;
    }
    let out = out.unwrap_or(&target.fab_out);
    let libraries = match timings::stage("libraries", "", || libraries(&target)) {
        Ok(l) => l,
        Err(code) => return code,
    };
    // KiCad is optional: the files are netsmith's own. It runs the design rules check when
    // it is installed, and the cross-check when asked, which without it is netsmith's viewer
    // comparison alone (issue #1158).
    let kicad = KicadCliExe::locate();
    if kicad.is_none() {
        eprintln!(
            "{}",
            netsmith_fabricate::fab::no_kicad_note(&netsmith_kicad::Search::machine())
        );
    }
    let options = FabOptions {
        route_timeout: match route_timeout {
            Some(0) => None,
            Some(secs) => Some(Duration::from_secs(secs)),
            None => FabOptions::default().route_timeout,
        },
        progress: Some(match (json, output.quiet) {
            (true, _) => keep_and_tell,
            (false, true) => quiet_and_post,
            (false, false) => crate::progress::tell,
        }),
        layout: None,
        allow_drc_violations,
        for_review,
        cross_check,
        only: None,
        strict_heights: strict_heights(&target, constraints_warn),
        lock: None,
        pins: None,
        pin_map_source: None,
        track_margin: track_margin(&target),
        rip_rounds: rip_rounds_of(&target, rip_rounds),
        glb: glb_wanted(&target, no_glb),
        step: step_wanted(&target, no_step),
        pdf: pdf_wanted(&target, no_pdf),
        schematic: None,
        revision: None,
        save_placed,
        reuse_routes,
        models: Some(netsmith_fabricate::models::Places::of(&target)),
        place_seeds: placing.seeds(&target),
        place_budget: placing.budget(&target),
        seed_writes: netsmith_fabricate::fab::SeedWrites::Write,
        offline,
        portable,
        // What an order reuses a package by (issue #841).
        reuse: netsmith_fabricate::fab::Reuse::Record,
    };
    let (pins_mode, locked_by) = pins_mode(&target, pins_locked, locked);
    let mut code = ExitCode::SUCCESS;
    // Whether a board failed its check, which may be a spelling that changed (issue #740).
    let mut unchecked = false;
    // The warnings of the pre-check before placing that are findings with data (#998).
    let mut told_as_data: Vec<String> = Vec::new();
    for source in &target.files {
        let text = match fs::read_to_string(&source.path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{}: {e}", source.name);
                return ExitCode::from(2);
            }
        };
        let (held, using) = match netsmith_pipeline::pick_lock::open(
            &source.path,
            &catalogs,
            locked,
            target.locks(source),
        ) {
            Ok(opened) => opened,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
        };
        let options = FabOptions {
            layout: Some(netsmith_fabricate::layout::layout_path(&source.path)),
            schematic: Some(schematic_inputs(&target, &source.path)),
            revision: target
                .project
                .as_ref()
                .and_then(|p| p.manifest.revision.clone()),
            lock: Some(using),
            pin_map_source: Some(source.stable_name(target.project.as_ref())),
            pins: Some(netsmith_fabricate::fab::PinsUse {
                locked_by,
                ..netsmith_fabricate::fab::PinsUse::new(
                    pins_mode,
                    netsmith_fabricate::pins::pins_path(&source.path),
                )
            }),
            ..options.clone()
        };
        match fab_detailed(
            &source.name,
            &text,
            &loader,
            out,
            held.as_ref().unwrap_or(&catalogs),
            &libraries,
            kicad.as_ref().map(|k| k as &dyn KicadCli),
            &options,
        ) {
            Err(failure) => {
                code = ExitCode::FAILURE;
                unchecked |= !failure.problems.is_empty();
                if json {
                    if failure.problems.is_empty() {
                        report.findings.push(Finding {
                            requirements: failure.requirements,
                            does_not_fit: failure.does_not_fit,
                            conflicts: failure.conflicts.into_vec(),
                            placement: failure.placement.into_vec(),
                            ..Finding::plain(
                                Severity::Error,
                                "fab",
                                failure.text.trim().trim_start_matches("error: "),
                            )
                        });
                    } else {
                        report
                            .findings
                            .extend(netsmith_tools::findings::of_problems(&failure.problems));
                    }
                } else {
                    eprint!("{}", failure.text);
                }
            }
            Ok(boards) => {
                for b in boards {
                    let lines: Vec<&str> = b
                        .notes
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .collect();
                    if json {
                        // A requirement not met that the run lists as a warning
                        // (`constraints = "warn"`) is a finding of its own, with its numbers,
                        // and not also a line of the notes.
                        let lines: Vec<&str> = lines
                            .into_iter()
                            .filter(|l| !b.requirements.iter().any(|r| l.contains(&r.text)))
                            .collect();
                        report
                            .findings
                            .extend(b.requirements.iter().map(|r| Finding {
                                board: Some(b.board.clone()),
                                requirements: vec![r.clone()],
                                ..Finding::plain(Severity::Warning, "fab", r.text.clone())
                            }));
                        // What the pre-check before placing found suspicious is a finding of
                        // its own, with the sets of lines as data, and not also a line told as
                        // the run went (#998).
                        for w in &b.place_warnings {
                            told_as_data.push(format!("{}: warning: {}", b.board, w.message));
                            report.findings.push(Finding {
                                board: Some(b.board.clone()),
                                conflicts: w.conflicts.clone(),
                                ..Finding::plain(Severity::Warning, "place", w.message.clone())
                            });
                        }
                        report.notes.total += lines.len();
                        if !lines.is_empty() {
                            *report.notes.by_cause.entry("fab".to_string()).or_default() +=
                                lines.len();
                        }
                        if output.notes == NotesMode::All {
                            report.findings.extend(lines.iter().map(|l| Finding {
                                board: Some(b.board.clone()),
                                ..Finding::plain(Severity::Note, "fab", *l)
                            }));
                        }
                        report.boards.push(BoardOut {
                            board: b.board.clone(),
                            dir: Some(b.dir.display().to_string()),
                            parts: b.origins.clone(),
                            outline_at: None,
                        });
                        continue;
                    }
                    print!("{}", b.summary);
                    println!("wrote {}", b.dir.display());
                    if !b.origins.is_empty() {
                        print!("{}", netsmith_pcb::origins_text(&b.origins));
                    }
                    match output.notes {
                        NotesMode::All if !lines.is_empty() => {
                            eprintln!("\nbefore you order ({}):\n{}", b.board, b.notes.trim_end());
                        }
                        NotesMode::Summary if !lines.is_empty() => eprintln!(
                            "note: {}: {} lines to read before you order, in {} (`--notes all` prints them)",
                            b.board,
                            lines.len(),
                            b.dir.join("fab-notes.txt").display()
                        ),
                        _ => {}
                    }
                }
            }
        }
    }
    if unchecked {
        common::release_note(&target, json.then_some(&mut report));
    }
    if json {
        // The warnings and notes that `fab` tells as it goes (a pick lock that differs, a
        // guideline, a part that hangs over an edge) are findings too.
        for line in take_kept_lines() {
            if told_as_data.contains(&line) {
                continue;
            }
            if let Some(f) = finding_of_line(&line) {
                report.findings.push(f);
            }
        }
        report.ok = code == ExitCode::SUCCESS;
        print!("{}", report.to_json());
    }
    code
}

/// How `fab` prints: `--quiet`, `--format` and `--notes` (#455).
#[derive(Clone, Copy)]
struct FabOutput {
    quiet: bool,
    format: Format,
    notes: NotesMode,
}

/// The lines `board fab --format json` keeps as it goes, to make findings of the warnings and notes
/// among them at the end. `progress` is a plain `fn`, so they are kept here.
static KEPT: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn keep_line(message: &str) {
    if let Ok(mut kept) = KEPT.lock() {
        kept.push(message.to_string());
    }
}

/// `board fab --format json`: the line kept, and told as `--progress` says (as JSON, or not at all).
fn keep_and_tell(message: &str) {
    keep_line(message);
    crate::progress::tell(message);
}

/// `board fab --quiet`: the line printed if it is a warning or a note, and posted to `--to-editor`'s
/// editor whatever it is.
fn quiet_and_post(message: &str) {
    quiet_line(message);
    crate::progress::post_only(message);
}

fn take_kept_lines() -> Vec<String> {
    KEPT.lock()
        .map(|mut k| std::mem::take(&mut *k))
        .unwrap_or_default()
}

/// `board fab --quiet`: the warnings and notes `fab` tells as it goes, and not its stages or the
/// router's passes.
fn quiet_line(message: &str) {
    if finding_of_line(message).is_some() {
        eprintln!("{message}");
    }
}

/// A line `fab` told as it went, as a finding when it is a warning or a note: `note: ...`,
/// `warning: ...`, or `<board>: warning: ...`.
fn finding_of_line(line: &str) -> Option<Finding> {
    let (board, rest) = match line.split_once(": ") {
        Some((board, rest)) if rest.starts_with("warning: ") || rest.starts_with("note: ") => {
            (Some(board.to_string()), rest)
        }
        _ => (None, line),
    };
    let (severity, code, message) = match rest.strip_prefix("warning: ") {
        Some(m) => (Severity::Warning, "fab", m),
        None => (Severity::Note, "note.other", rest.strip_prefix("note: ")?),
    };
    Some(Finding {
        board,
        ..Finding::plain(severity, code, message)
    })
}

/// `netsmith project order`: list the project's orders, or make one (ADR 0035).
#[allow(clippy::too_many_arguments)]
pub(super) fn run_order(
    request: &OrderRequest,
    out: Option<&Path>,
    route_timeout: Option<u64>,
    rip_rounds: Option<usize>,
    allow_drc_violations: bool,
    cross_check: bool,
    strict: bool,
    locked: bool,
    accept_via_in_pad: bool,
    constraints_warn: Option<bool>,
    no_glb: bool,
    no_step: bool,
    no_pdf: bool,
    offline: bool,
    placing: &common::Placing,
    format: Format,
) -> ExitCode {
    let json = format == Format::Json;
    let (target, loader, mut catalogs) = match target_offline(None, strict, offline) {
        Ok(t) => t,
        Err(code) => return code,
    };
    if let Some(warn) = constraints_warn {
        catalogs = catalogs.with_constraints_warn(warn);
    }
    let orders = target
        .project
        .as_ref()
        .map(|p| p.manifest.orders.as_slice())
        .unwrap_or_default();
    let name = match request {
        OrderRequest::List => {
            if json {
                common::print_json(&serde_json::json!({
                    "command": "order",
                    "ok": true,
                    "orders": orders.iter().map(|o| serde_json::json!({
                        "name": o.name,
                        "boards": o.boards.iter().map(|b| serde_json::json!({
                            "name": b.name,
                            "quantity": b.quantity,
                        })).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                }));
            } else {
                print!("{}", netsmith_fabricate::order::list(orders));
            }
            return ExitCode::SUCCESS;
        }
        OrderRequest::Make(name) => name,
    };
    let Some(order) = orders.iter().find(|o| o.name == *name) else {
        let names: Vec<&str> = orders.iter().map(|o| o.name.as_str()).collect();
        eprintln!(
            "error: netsmith.toml has no order `{name}`; the orders are: {}",
            if names.is_empty() {
                "(none)".to_string()
            } else {
                names.join(", ")
            }
        );
        return ExitCode::from(2);
    };
    if no_boards(&target) {
        return ExitCode::from(2);
    }
    let libraries = match libraries(&target) {
        Ok(l) => l,
        Err(code) => return code,
    };
    let kicad = KicadCliExe::locate();
    let options = FabOptions {
        route_timeout: match route_timeout {
            Some(0) => None,
            Some(secs) => Some(Duration::from_secs(secs)),
            None => FabOptions::default().route_timeout,
        },
        progress: Some(crate::progress::tell),
        layout: None,
        only: None,
        allow_drc_violations,
        // An order is never made for review.
        for_review: false,
        cross_check,
        strict_heights: strict_heights(&target, constraints_warn),
        lock: None,
        // An order is a release: it never moves a pin by itself.
        pins: None,
        pin_map_source: None,
        track_margin: track_margin(&target),
        rip_rounds: rip_rounds_of(&target, rip_rounds),
        glb: glb_wanted(&target, no_glb),
        step: step_wanted(&target, no_step),
        pdf: pdf_wanted(&target, no_pdf),
        // Each file's own, set by the order for each file.
        schematic: None,
        revision: None,
        save_placed: false,
        reuse_routes: false,
        models: Some(netsmith_fabricate::models::Places::of(&target)),
        place_seeds: placing.seeds(&target),
        place_budget: placing.budget(&target),
        // An order is a release: a seed that would change stops it (issue #458).
        seed_writes: netsmith_fabricate::fab::SeedWrites::Order,
        offline,
        portable: false,
        reuse: netsmith_fabricate::fab::Reuse::Off,
    };
    let out = out.unwrap_or(&target.fab_out);
    // A board a plain `fab` made into this folder from the same inputs is copied, not made again.
    let options = FabOptions {
        reuse: netsmith_fabricate::fab::Reuse::From(out.to_path_buf()),
        ..options
    };
    // `--accept-via-in-pad` accepts the cost of vias in pads for this run (issue #1118).
    let order = &netsmith_fabricate::order::Order {
        via_in_pad_accepted: order.via_in_pad_accepted || accept_via_in_pad,
        ..order.clone()
    };
    match netsmith_fabricate::order::fab_order(
        order,
        &target,
        &loader,
        &catalogs,
        &libraries,
        kicad.as_ref().map(|k| k as &dyn KicadCli),
        &options,
        // As `fab` on each file: the pick lock holds the parts, and is written when absent.
        if locked {
            OrderLocks::Locked
        } else {
            OrderLocks::Held
        },
        out,
    ) {
        Err(problems) if json => {
            common::print_json(&serde_json::json!({
                "command": "order",
                "ok": false,
                "order": order.name,
                "findings": [Finding::plain(
                    Severity::Error,
                    "fab",
                    problems.to_string().trim().trim_start_matches("error: "),
                )],
            }));
            ExitCode::FAILURE
        }
        Err(problems) => {
            eprint!("{problems}");
            ExitCode::FAILURE
        }
        Ok(made) if json => {
            common::print_json(&serde_json::json!({
                "command": "order",
                "ok": true,
                "order": made.name,
                "dir": made.dir.display().to_string(),
                "boards": made.boards.iter().map(|(selected, b)| serde_json::json!({
                    "board": b.board,
                    "quantity": selected.quantity,
                    "dir": b.dir.display().to_string(),
                    "reused": b.reused,
                })).collect::<Vec<_>>(),
                "files": made.files.iter().map(|f| f.display().to_string()).collect::<Vec<_>>(),
                "reused": made.reused(),
                "stages_run": made.stages_run(),
                "summary": made.summary,
            }));
            ExitCode::SUCCESS
        }
        Ok(made) => {
            for (_, b) in &made.boards {
                print!("{}", b.summary);
                println!("wrote {}", b.dir.display());
            }
            println!();
            print!("{}", made.summary);
            println!("{}", made.reuse_line());
            println!("wrote {}", made.dir.display());
            ExitCode::SUCCESS
        }
    }
}

/// `board fab --constraints`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Constraints {
    /// An unmet requirement is listed instead of stopping `fab`.
    Warn,
    /// It stops `fab`, whatever netsmith.toml says.
    Error,
}

/// `board fab --pins`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Pins {
    /// Swap interchangeable pins that are not locked, place and route again (three rounds at
    /// most) and write the pin file.
    Auto,
    /// Move none, write the swaps that would help to `pins.proposed.toml` in the output folder,
    /// and fail as before.
    Locked,
}

/// What `netsmith project order` asks for (ADR 0035).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderRequest {
    /// No name: list the project's orders.
    List,
    /// A name: make that order.
    Make(String),
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::Leaf;
    use crate::commands::common::testing::{output, parse};
    use crate::commands::fab;

    fn fab(args: &[&str]) -> fab::Args {
        let mut line = vec!["board", "fab"];
        line.extend_from_slice(args);
        match parse(&line) {
            Ok(Leaf::Fab(fab)) => fab,
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn fab_has_its_defaults_and_takes_a_file_and_an_out_directory() {
        assert_eq!(
            fab(&["a.nsm", "--out", "x"]),
            fab::Args {
                file: Some("a.nsm".into()),
                out: Some("x".into()),
                route_timeout: None,
                rip_rounds: None,
                allow_drc_violations: false,
                for_review: false,
                cross_check: false,
                strict: false,
                locked: false,
                constraints: None,
                pins: None,
                no_glb: false,
                no_step: false,
                no_pdf: false,
                save_placed: false,
                reuse_routes: false,
                offline: false,
                portable: false,
                quiet: false,
                placing: crate::commands::common::Placing {
                    place_seeds: None,
                    place_budget: None,
                },
                watching: crate::commands::common::Watching {
                    progress: crate::progress::Print::Text,
                    to_editor: None,
                },
                output: output(),
            }
        );
        assert!(parse(&["board", "fab", "a.nsm", "--port", "9"]).is_err());
        // Issue #764: the progress as JSON lines, and to a running editor.
        let f = fab(&["--progress", "json", "--to-editor", "a.nsm"]);
        assert_eq!(f.watching.progress, crate::progress::Print::Json);
        assert_eq!(f.watching.to_editor, Some(8787));
        assert_eq!(
            fab(&["--to-editor=9000", "a.nsm"]).watching.to_editor,
            Some(9000)
        );
        assert!(parse(&["board", "fab", "a.nsm", "--progress", "nothing"]).is_err());
        let f = fab(&[
            "--allow-drc-violations",
            "--cross-check",
            "--quiet",
            "--no-glb",
            "--no-step",
            "--no-pdf",
        ]);
        assert!(
            f.allow_drc_violations && f.cross_check && f.quiet && f.no_glb && f.no_step && f.no_pdf
        );
        // Issue #1189: the files of a board with findings, to look at; not with the flags that
        // package for an order or compare the Gerbers, and not for an order.
        assert!(fab(&["--for-review"]).for_review);
        assert!(!fab(&[]).for_review);
        for other in ["--allow-drc-violations", "--cross-check"] {
            let e = parse(&["board", "fab", "--for-review", other]).unwrap_err();
            assert!(e.contains("cannot be used with"), "{other}: {e}");
        }
        assert!(parse(&["project", "order", "d", "--for-review"]).is_err());
        assert!(parse(&["board", "build", "--for-review"]).is_err());
        assert!(parse(&["board", "build", "--cross-check"]).is_err());
        assert!(parse(&["board", "build", "--no-glb"]).is_err());
        assert!(parse(&["board", "build", "--no-step"]).is_err());
        assert!(parse(&["board", "build", "--no-pdf"]).is_err());
    }

    #[test]
    fn fab_and_the_editor_take_offline_and_the_other_commands_do_not() {
        assert!(!fab(&[]).offline);
        assert!(fab(&["--offline"]).offline);
        assert!(fab(&["a.nsm", "--offline"]).offline);
        assert!(matches!(
            parse(&["view", "editor", "--offline"]),
            Ok(Leaf::Editor(e)) if e.offline
        ));
        assert!(parse(&["board", "build", "--offline"]).is_err());
    }

    #[test]
    fn fab_takes_a_route_timeout_in_whole_seconds() {
        assert_eq!(fab(&["--route-timeout", "30"]).route_timeout, Some(30));
        let e = parse(&["board", "fab", "--route-timeout", "soon"]).unwrap_err();
        assert!(e.contains("--route-timeout") && e.contains("soon"), "{e}");
        assert!(parse(&["board", "fab", "--route-timeout"]).is_err());
        assert!(parse(&["board", "build", "--route-timeout", "3"]).is_err());
    }

    #[test]
    fn fab_takes_the_rounds_of_rip_up_as_a_count_and_zero_turns_them_off() {
        assert_eq!(fab(&["--rip-rounds", "2"]).rip_rounds, Some(2));
        assert_eq!(fab(&["--rip-rounds", "0"]).rip_rounds, Some(0));
        assert!(parse(&["board", "fab", "--rip-rounds", "-1"]).is_err());
        assert!(parse(&["board", "fab", "--rip-rounds", "many"]).is_err());
        assert!(parse(&["board", "build", "--rip-rounds", "3"]).is_err());
    }

    #[test]
    fn fab_takes_place_seeds_and_a_place_budget_of_seconds_or_tries() {
        use netsmith_core::job::Limit;
        use std::time::Duration;
        let f = fab(&["--place-seeds", "3", "--place-budget", "90"]);
        assert_eq!(f.placing.place_seeds, Some(3));
        assert_eq!(
            f.placing.place_budget,
            Some(Limit {
                time: Some(Duration::from_secs(90)),
                tries: None
            })
        );
        assert_eq!(
            fab(&["--place-budget", "2m+5tries"]).placing.place_budget,
            Some(Limit {
                time: Some(Duration::from_secs(120)),
                tries: Some(5)
            })
        );
        for bad in ["0", "soon", "5tries+3tries", "1.5tries"] {
            let e = parse(&["board", "fab", "--place-budget", bad]).unwrap_err();
            assert!(e.contains("--place-budget"), "{bad}: {e}");
        }
        assert!(parse(&["board", "fab", "--place-seeds", "0"]).is_err());
        assert!(parse(&["board", "fab", "--place-seeds", "33"]).is_err());
        assert!(parse(&["board", "build", "--place-budget", "9"]).is_err());
        let Ok(Leaf::Board3d(b)) = parse(&["dev", "board3d", "--place-budget", "1try"]) else {
            panic!()
        };
        assert_eq!(b.placing.place_budget.and_then(|l| l.tries), Some(1));
    }

    #[test]
    fn build_and_fab_take_strict_and_locked_and_nothing_else_does() {
        let Ok(Leaf::Build(b)) = parse(&["board", "build", "--strict", "--locked"]) else {
            panic!()
        };
        assert!(b.strict && b.locked);
        let f = fab(&["a.nsm", "--locked", "--strict"]);
        assert!(f.locked && f.strict);
        assert!(!fab(&[]).locked, "a run may change the lock by default");
        assert!(parse(&["view", "editor", "--strict"]).is_err());
        assert!(parse(&["view", "editor", "--locked"]).is_err());
        assert!(parse(&["board", "compare", "a", "b", "--locked"]).is_err());
    }

    #[test]
    fn fab_takes_pins_and_constraints_for_one_run() {
        assert_eq!(fab(&["--pins", "locked"]).pins_locked(), Some(true));
        assert_eq!(fab(&["--pins", "auto"]).pins_locked(), Some(false));
        assert_eq!(fab(&[]).pins_locked(), None, "the manifest decides");
        assert_eq!(
            fab(&["--constraints", "warn"]).constraints_warn(),
            Some(true)
        );
        assert_eq!(
            fab(&["--constraints", "error"]).constraints_warn(),
            Some(false)
        );
        assert_eq!(fab(&[]).constraints_warn(), None, "the manifest decides");
        let e = parse(&["board", "fab", "--pins", "swap"]).unwrap_err();
        assert!(
            e.contains("swap") && e.contains("auto") && e.contains("locked"),
            "{e}"
        );
        let e = parse(&["board", "fab", "--constraints", "off"]).unwrap_err();
        assert!(
            e.contains("off") && e.contains("warn") && e.contains("error"),
            "{e}"
        );
        assert!(parse(&["board", "fab", "--pins"]).is_err());
        assert!(parse(&["board", "build", "--constraints", "warn"]).is_err());
    }
}
