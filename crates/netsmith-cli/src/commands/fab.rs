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
            match netsmith::fab::kicad_switched_off(&netsmith_kicad::install::Search::machine()) {
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
            netsmith::fab::no_kicad_note(&netsmith_kicad::install::Search::machine())
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
        save_placed,
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
                        });
                        continue;
                    }
                    print!("{}", b.summary);
                    println!("wrote {}", b.dir.display());
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
        save_placed: false,
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

/// Make the fab package (Gerbers, drill, bom.csv, cpl.csv, fab-notes.txt, the 3D scene as .glb)
/// or an order.
///
/// With no file, `fab` acts on the project's boards (see netsmith.toml).
///
/// `fab` writes the Gerbers, drill files and placement file itself and runs netsmith's own
/// design rules check (ADR 0037), so it does not need KiCad. An error of the check (a
/// clearance, a short, a missing connection, a hole, the edge, a courtyard, a keepout) stops
/// `fab` unless `--allow-drc-violations` is given, and `fab-notes.txt` then says the board is
/// not clean; its warnings (silkscreen on a pad, a narrow mask web, a via in a pad) are listed
/// in the notes. It writes, for each board, a Gerber zip, `bom.csv`, `cpl.csv`,
/// `fab-notes.txt`, the `.kicad_pcb` and `.kicad_pro`, an `.svg`, and the board's 3D scene as
/// `<board>.glb` (outside the Gerber zip; the scene of the routed board, the bytes
/// `board3d --glb` writes for it; docs/BOARD3D.md). Read `fab-notes.txt` before you order.
///
/// A board the router cannot finish is still written, marked INCOMPLETE, and the run ends in an
/// error. Beside it, `routing-failure.txt` lists each connection left with its net, its two pads
/// and their places and the parts most of them end on, and then, for each line the router left,
/// the stage it last failed in (the first pass, the try with less room, ripping up, the
/// negotiation, the chain of rip-ups, or out of time) and the nets whose copper stands in its
/// way; `routing-report.json` has the same for a program, with a crowding map of the board
/// (1 mm squares, how often the router's searches across other nets' copper passed through
/// each), which shows where the board is fought over.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct FabArgs {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The folder to write into; the default is the target's.
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// Seconds the router may take for a board (0 for no limit).
    ///
    /// When the time is spent, `fab` stops and lists the connections left. The default is 300.
    #[arg(long, value_name = "SECS")]
    pub route_timeout: Option<u64>,
    /// Package a board that fails the design rules check.
    ///
    /// `fab-notes.txt` then says the board is not clean. It never waives a `[constraint]`
    /// finding: a requirement on where a part goes is relaxed only by `constraints = "warn"` in
    /// netsmith.toml or `--constraints warn`.
    #[arg(long)]
    pub allow_drc_violations: bool,
    /// Compare the files and the findings with KiCad's own plot and check of the board.
    ///
    /// It needs KiCad's command-line tool: `$KICAD_CLI`, then `kicad-cli` on `PATH`, then
    /// Windows KiCad 9 under WSL.
    #[arg(long)]
    pub cross_check: bool,
    /// Make a guideline finding an error for this run, as with `build`.
    #[arg(long)]
    pub strict: bool,
    /// Neither the pick lock nor the pin file may change, as with `build`.
    ///
    /// With `fab` it also means `--pins locked`, so a board that does not route never has its
    /// pin file rewritten (`--locked` wins over `--pins auto`). With `--order <name>` it applies
    /// to each source file that builds a board of the order.
    #[arg(long)]
    pub locked: bool,
    /// Whether a requirement on where a part goes that is not met stops `fab` (ADR 0045).
    ///
    /// It wins over `constraints` in netsmith.toml for this run.
    #[arg(long, value_enum)]
    pub constraints: Option<Constraints>,
    /// What a board that does not route may do with the pins its requests got (ADR 0101).
    ///
    /// `pins` in netsmith.toml sets it for a project; the default is `auto`.
    #[arg(long, value_enum)]
    pub pins: Option<Pins>,
    /// Leave the board's 3D scene (`<board>.glb`) out of the package for this run.
    ///
    /// `glb = false` in netsmith.toml does it for a project; a stale `<board>.glb` of an
    /// earlier run is removed.
    #[arg(long)]
    pub no_glb: bool,
    /// Also write `<board>.placed.json`, the board as placed before it is routed, for `route`.
    ///
    /// The file goes into the board's output folder, with the track margin `fab` routes with.
    /// Nothing else of the package changes, and a run without it removes the file an earlier run
    /// saved. `--order` does not take it.
    #[arg(long, conflicts_with = "order")]
    pub save_placed: bool,
    /// Make one of the project's orders; with no name, list them (ADR 0035).
    ///
    /// An order names its boards in netsmith.toml, so `--order` takes no file. It prints text
    /// only.
    #[arg(long, value_name = "NAME", num_args = 0..=1, conflicts_with = "file")]
    pub order: Option<Option<String>>,
    /// Print the result of each board and the findings, not the stages and the router's passes.
    #[arg(long)]
    pub quiet: bool,
    /// Text for a person, or one JSON document on stdout, as with `build`.
    ///
    /// With `json` the progress goes nowhere, and the notes of `fab-notes.txt` ("before you
    /// order") are findings with the code `fab`.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
    /// Each note, a count for each cause, or none, as with `build`.
    #[arg(long, value_enum, default_value = "all")]
    pub notes: NotesMode,
}

impl FabArgs {
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
