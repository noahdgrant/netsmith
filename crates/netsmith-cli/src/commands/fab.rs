use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args as ClapArgs, ValueEnum};
use netsmith::fab::{FabOptions, fab_detailed};
use netsmith::findings::{BoardOut, Finding, Format, NotesMode, Report, Severity};
use netsmith::order::OrderLocks;
use netsmith::project::Target;
use netsmith_core::timings;
use netsmith_fab::{KicadCli, KicadCliExe};

use super::common::{self, libraries, no_boards, sync_pins, target};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "With no file, `fab` acts on the project's boards (see netsmith.toml).\n\n`fab` writes the Gerbers, drill files and placement file itself and runs netsmith's own design rules check (ADR 0037), so it does not need KiCad. An error of the check (a clearance, a short, a missing connection, a hole, the edge, a courtyard, a keepout) stops `fab` unless `--allow-drc-violations` is given, and `fab-notes.txt` then says the board is not clean; its warnings (silkscreen on a pad, a narrow mask web, a via in a pad) are listed in the notes. It writes, for each board, a Gerber zip, `bom.csv`, `cpl.csv`, `fab-notes.txt`, the `.kicad_pcb` and `.kicad_pro`, an `.svg`, and the board's 3D scene as `<board>.glb` (outside the Gerber zip; the scene of the routed board, the bytes `board3d --glb` writes for it; docs/BOARD3D.md), and the board as STEP, `<board>.step`, for mechanical CAD (outside the Gerber zip; the bytes `board3d --step` writes). Read `fab-notes.txt` before you order.\n\nA board the router cannot finish is still written, marked INCOMPLETE, and the run ends in an error. Beside it, `routing-failure.txt` lists each connection left with its net, its two pads and their places and the parts most of them end on, and then, for each line the router left, the stage it last failed in (the first pass, the try with less room, ripping up, the negotiation, the chain of rip-ups, or out of time) and the nets whose copper stands in its way; `routing-report.json` has the same for a program, with a crowding map of the board (1 mm squares, how often the router's searches across other nets' copper passed through each), which shows where the board is fought over.\n\nWith `--format json` the progress goes nowhere, and the notes of `fab-notes.txt` (\"before you order\") are findings with the code `fab`."
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
    /// Package a board that fails the design rules check (never an unmet `[constraint]`).
    ///
    /// `fab-notes.txt` then says the board is not clean. It never waives a `[constraint]` finding:
    /// a requirement on where a part goes is relaxed only by `constraints = "warn"` in
    /// netsmith.toml or `--constraints warn`.
    #[arg(long)]
    pub allow_drc_violations: bool,
    /// Compare the files and the findings with KiCad's own plot and check (needs kicad-cli).
    ///
    /// It needs KiCad's command-line tool: `$KICAD_CLI`, then `kicad-cli` on `PATH`, then Windows
    /// KiCad 9 under WSL.
    #[arg(long)]
    pub cross_check: bool,
    /// Make a guideline finding an error for this run.
    #[arg(long)]
    pub strict: bool,
    /// Stop when the pick lock or the pin file would change; implies `--pins locked`.
    ///
    /// With `fab` it also means `--pins locked`, so a board that does not route never has its pin
    /// file rewritten (`--locked` wins over `--pins auto`). With `--order <name>` it applies to
    /// each source file that builds a board of the order.
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
    /// Also write `<board>.placed.json`, the board before routing, for `netsmith route`.
    ///
    /// The file goes into the board's output folder, with the track margin `fab` routes with.
    /// Nothing else of the package changes, and a run without it removes the file an earlier run
    /// saved. `--order` does not take it.
    #[arg(long, conflicts_with = "order")]
    pub save_placed: bool,
    /// Make one of the project's orders, or list them with no name (ADR 0035).
    ///
    /// An order names its boards in netsmith.toml, so `--order` takes no file. It prints text only.
    #[arg(long, value_name = "NAME", num_args = 0..=1, conflicts_with = "file")]
    pub order: Option<Option<String>>,
    /// Print the result of each board and the findings, not the stages.
    #[arg(long)]
    pub quiet: bool,
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

    /// A named order to make, or the list of them, instead of every board.
    pub fn order(&self) -> Option<OrderRequest> {
        self.order.as_ref().map(|name| match name {
            Some(name) => OrderRequest::Make(name.clone()),
            None => OrderRequest::List,
        })
    }
}

/// Makes the fab package of each board, or an order.
pub fn run(args: Args) -> ExitCode {
    let (constraints_warn, pins_locked, order) =
        (args.constraints_warn(), args.pins_locked(), args.order());
    let format: Format = args.output.format.into();
    if order.is_some() && format == Format::Json {
        eprintln!(
            "error: `fab --order` prints text only; `--format json` is for `fab` of a file or of the project's boards"
        );
        return ExitCode::from(2);
    }
    if let Some(code) = sync_pins(args.file.as_deref(), args.locked) {
        return code;
    }
    match order {
        Some(request) => run_order(
            &request,
            args.out.as_deref(),
            args.route_timeout,
            args.allow_drc_violations,
            args.cross_check,
            args.strict,
            args.locked,
            constraints_warn,
            args.no_glb,
            args.no_step,
        ),
        None => timings::stage("fab.total", "", || {
            run_fab(
                args.file.as_deref(),
                args.out.as_deref(),
                args.route_timeout,
                args.allow_drc_violations,
                args.cross_check,
                args.strict,
                args.locked,
                constraints_warn,
                pins_locked,
                args.no_glb,
                args.no_step,
                args.save_placed,
                FabOutput {
                    quiet: args.quiet,
                    format,
                    notes: args.output.notes.into(),
                },
            )
        }),
    }
}

/// Whether constraints were asked to be strict, for this target and this run's
/// `--constraints` (see [`netsmith::fab::constraints_strict`]).
/// The project's `track_margin`, or the default.
fn track_margin(target: &Target) -> f64 {
    target
        .project
        .as_ref()
        .and_then(|p| p.manifest.track_margin)
        .unwrap_or(netsmith::fab::DEFAULT_TRACK_MARGIN)
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
    netsmith::fab::constraints_strict(
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
) -> (netsmith::fab::PinsMode, netsmith::fab::LockedBy) {
    use netsmith::fab::{LockedBy, PinsMode};
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
    allow_drc_violations: bool,
    cross_check: bool,
    strict: bool,
    locked: bool,
    constraints_warn: Option<bool>,
    pins_locked: Option<bool>,
    no_glb: bool,
    no_step: bool,
    save_placed: bool,
    output: FabOutput,
) -> ExitCode {
    let json = output.format == Format::Json;
    let mut report = Report::new("fab");
    let (target, loader, mut catalogs) = match target(file, strict) {
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
    let out = out.unwrap_or(&target.fab_out);
    let libraries = match timings::stage("libraries", "", || libraries(&target)) {
        Ok(l) => l,
        Err(code) => return code,
    };
    // KiCad is optional: the files are netsmith's own. It runs the design rules check when
    // it is installed, and the cross-check when asked.
    let kicad = KicadCliExe::locate();
    if kicad.is_none() {
        if cross_check {
            match netsmith::fab::kicad_switched_off(&netsmith_kicad::Search::machine()) {
                Some(off) => eprintln!(
                    "error: --cross-check compares with KiCad's own files and needs kicad-cli, and {off}"
                ),
                None => eprintln!(
                    "error: --cross-check compares with KiCad's own files: set KICAD_CLI to its command-line tool"
                ),
            }
            return ExitCode::from(2);
        }
        eprintln!(
            "{}",
            netsmith::fab::no_kicad_note(&netsmith_kicad::Search::machine())
        );
    }
    let options = FabOptions {
        route_timeout: match route_timeout {
            Some(0) => None,
            Some(secs) => Some(Duration::from_secs(secs)),
            None => FabOptions::default().route_timeout,
        },
        progress: Some(match (json, output.quiet) {
            (true, _) => keep_line,
            (false, true) => quiet_line,
            (false, false) => |message| eprintln!("{message}"),
        }),
        layout: None,
        allow_drc_violations,
        cross_check,
        only: None,
        strict_heights: strict_heights(&target, constraints_warn),
        lock: None,
        pins: None,
        pin_map_source: None,
        track_margin: track_margin(&target),
        glb: glb_wanted(&target, no_glb),
        step: step_wanted(&target, no_step),
        save_placed,
        models: Some(netsmith::models::Places::of(&target)),
        place_seeds: netsmith::layout::place_seeds(target.project.as_ref()),
        seed_writes: netsmith::fab::SeedWrites::Write,
    };
    let (pins_mode, locked_by) = pins_mode(&target, pins_locked, locked);
    let mut code = ExitCode::SUCCESS;
    for source in &target.files {
        let text = match fs::read_to_string(&source.path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{}: {e}", source.name);
                return ExitCode::from(2);
            }
        };
        let (held, using) = match netsmith::pick_lock::open(&source.path, &catalogs, locked) {
            Ok(opened) => opened,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
        };
        let options = FabOptions {
            layout: Some(netsmith::layout::layout_path(&source.path)),
            lock: Some(using),
            pin_map_source: Some(source.stable_name(target.project.as_ref())),
            pins: Some(netsmith::fab::PinsUse {
                locked_by,
                ..netsmith::fab::PinsUse::new(pins_mode, netsmith::pins::pins_path(&source.path))
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
                if json {
                    if failure.problems.is_empty() {
                        report.findings.push(Finding::plain(
                            Severity::Error,
                            "fab",
                            failure.text.trim().trim_start_matches("error: "),
                        ));
                    } else {
                        report
                            .findings
                            .extend(netsmith::findings::of_problems(&failure.problems));
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
    if json {
        // The warnings and notes that `fab` tells as it goes (a pick lock that differs, a
        // guideline, a part that hangs over an edge) are findings too.
        for line in take_kept_lines() {
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

/// The lines `fab --format json` keeps as it goes, to make findings of the warnings and notes
/// among them at the end. `progress` is a plain `fn`, so they are kept here.
static KEPT: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn keep_line(message: &str) {
    if let Ok(mut kept) = KEPT.lock() {
        kept.push(message.to_string());
    }
}

fn take_kept_lines() -> Vec<String> {
    KEPT.lock()
        .map(|mut k| std::mem::take(&mut *k))
        .unwrap_or_default()
}

/// `fab --quiet`: the warnings and notes `fab` tells as it goes, and not its stages or the
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

/// `netsmith fab --order`: list the project's orders, or make one (ADR 0035).
#[allow(clippy::too_many_arguments)]
fn run_order(
    request: &OrderRequest,
    out: Option<&Path>,
    route_timeout: Option<u64>,
    allow_drc_violations: bool,
    cross_check: bool,
    strict: bool,
    locked: bool,
    constraints_warn: Option<bool>,
    no_glb: bool,
    no_step: bool,
) -> ExitCode {
    let (target, loader, mut catalogs) = match target(None, strict) {
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
            print!("{}", netsmith::order::list(orders));
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
        progress: Some(|message| eprintln!("{message}")),
        layout: None,
        only: None,
        allow_drc_violations,
        cross_check,
        strict_heights: strict_heights(&target, constraints_warn),
        lock: None,
        // An order is a release: it never moves a pin by itself.
        pins: None,
        pin_map_source: None,
        track_margin: track_margin(&target),
        glb: glb_wanted(&target, no_glb),
        step: step_wanted(&target, no_step),
        save_placed: false,
        models: Some(netsmith::models::Places::of(&target)),
        place_seeds: netsmith::layout::place_seeds(target.project.as_ref()),
        // An order is a release: a seed that would change stops it (issue #458).
        seed_writes: netsmith::fab::SeedWrites::Order,
    };
    let out = out.unwrap_or(&target.fab_out);
    match netsmith::order::fab_order(
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
        Err(problems) => {
            eprint!("{problems}");
            ExitCode::FAILURE
        }
        Ok(made) => {
            for (_, b) in &made.boards {
                print!("{}", b.summary);
                println!("wrote {}", b.dir.display());
            }
            println!();
            print!("{}", made.summary);
            println!("wrote {}", made.dir.display());
            ExitCode::SUCCESS
        }
    }
}

/// `fab --constraints`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Constraints {
    /// An unmet requirement is listed instead of stopping `fab`.
    Warn,
    /// It stops `fab`, whatever netsmith.toml says.
    Error,
}

/// `fab --pins`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Pins {
    /// Swap interchangeable pins that are not locked, place and route again (three rounds at
    /// most) and write the pin file.
    Auto,
    /// Move none, write the swaps that would help to `pins.proposed.toml` in the output folder,
    /// and fail as before.
    Locked,
}

/// What `netsmith fab --order` asks for (ADR 0035).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderRequest {
    /// `--order` with no name: list the project's orders.
    List,
    /// `--order <name>`: make that order.
    Make(String),
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{kind, output, parse};
    use crate::commands::{Command, fab};
    use clap::error::ErrorKind;

    fn fab(args: &[&str]) -> fab::Args {
        let mut line = vec!["fab"];
        line.extend_from_slice(args);
        match parse(&line) {
            Ok(Command::Fab(fab)) => fab,
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
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                locked: false,
                constraints: None,
                pins: None,
                no_glb: false,
                no_step: false,
                save_placed: false,
                order: None,
                quiet: false,
                output: output(),
            }
        );
        assert!(parse(&["fab", "a.nsm", "--port", "9"]).is_err());
        let f = fab(&[
            "--allow-drc-violations",
            "--cross-check",
            "--quiet",
            "--no-glb",
            "--no-step",
        ]);
        assert!(f.allow_drc_violations && f.cross_check && f.quiet && f.no_glb && f.no_step);
        assert!(parse(&["build", "--cross-check"]).is_err());
        assert!(parse(&["build", "--no-glb"]).is_err());
        assert!(parse(&["build", "--no-step"]).is_err());
    }

    #[test]
    fn fab_takes_a_route_timeout_in_whole_seconds() {
        assert_eq!(fab(&["--route-timeout", "30"]).route_timeout, Some(30));
        let e = parse(&["fab", "--route-timeout", "soon"]).unwrap_err();
        assert!(e.contains("--route-timeout") && e.contains("soon"), "{e}");
        assert!(parse(&["fab", "--route-timeout"]).is_err());
        assert!(parse(&["build", "--route-timeout", "3"]).is_err());
    }

    #[test]
    fn fab_order_takes_a_name_or_lists_the_orders_and_no_file() {
        assert_eq!(
            fab(&["--order", "devkit"]).order(),
            Some(fab::OrderRequest::Make("devkit".into()))
        );
        assert_eq!(fab(&["--order"]).order(), Some(fab::OrderRequest::List));
        assert_eq!(fab(&[]).order(), None);
        // A flag after `--order` is not its name.
        let f = fab(&["--order", "--out", "x"]);
        assert_eq!(
            (f.order(), f.out),
            (Some(fab::OrderRequest::List), Some("x".into()))
        );
        let f = fab(&["--out", "x", "--order", "d", "--route-timeout", "9"]);
        assert_eq!(f.order(), Some(fab::OrderRequest::Make("d".into())));
        assert_eq!(f.route_timeout, Some(9));
        assert_eq!(
            kind(&["fab", "a.nsm", "--order", "d"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["fab", "--order", "d", "--save-placed"]),
            ErrorKind::ArgumentConflict
        );
        assert!(fab(&["a.nsm", "--save-placed"]).save_placed);
        assert!(parse(&["build", "--order", "d"]).is_err());
    }

    #[test]
    fn build_and_fab_take_strict_and_locked_and_nothing_else_does() {
        let Ok(Command::Build(b)) = parse(&["build", "--strict", "--locked"]) else {
            panic!()
        };
        assert!(b.strict && b.locked);
        let f = fab(&["a.nsm", "--locked", "--strict"]);
        assert!(f.locked && f.strict);
        assert!(!fab(&[]).locked, "a run may change the lock by default");
        assert!(parse(&["editor", "--strict"]).is_err());
        assert!(parse(&["editor", "--locked"]).is_err());
        assert!(parse(&["compare", "a", "b", "--locked"]).is_err());
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
        let e = parse(&["fab", "--pins", "swap"]).unwrap_err();
        assert!(
            e.contains("swap") && e.contains("auto") && e.contains("locked"),
            "{e}"
        );
        let e = parse(&["fab", "--constraints", "off"]).unwrap_err();
        assert!(
            e.contains("off") && e.contains("warn") && e.contains("error"),
            "{e}"
        );
        assert!(parse(&["fab", "--pins"]).is_err());
        assert!(parse(&["build", "--constraints", "warn"]).is_err());
    }
}
