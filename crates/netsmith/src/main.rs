//! `netsmith --version`, `--help`, `init`, `build`, `fab`, `editor`, `board3d`, `render`, `view` and `compare`.

use std::{
    env, fs,
    net::{Ipv4Addr, TcpListener},
    path::Path,
    process::ExitCode,
    sync::Arc,
    time::Duration,
};

use netsmith::cli::{Command, LockMode, OrderRequest, RefreshRequest, parse_args};
use netsmith::compare::{Request, Verdict, compare_files};
use netsmith::editor::Workspace;
use netsmith::fab::{FabOptions, fab_with_options};
use netsmith::init::init;
use netsmith::order::OrderLocks;
use netsmith::project::{Project, Target};
use netsmith::supply::Supply;
use netsmith::{Libraries, outcome_with, render_problems};
use netsmith_check::Loader;
use netsmith_core::timings;
use netsmith_fab::{KicadCli, KicadCliExe};
use netsmith_solve::Catalogs;

fn main() -> ExitCode {
    match parse_args(env::args().skip(1)) {
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
        Ok(Command::Version) => {
            println!("netsmith {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Help) => {
            print!("{}", netsmith::cli::help());
            ExitCode::SUCCESS
        }
        Ok(Command::Init { name }) => run_init(name.as_deref()),
        Ok(Command::Build {
            file,
            out,
            strict,
            locked,
        }) => timings::stage("build.total", "", || {
            // The pin file first: a request must have its pin before anything is built, and
            // `--locked` stops here before the pick lock or any file is written.
            if let Some(code) = sync_pins(file.as_deref(), locked) {
                return code;
            }
            run_build(file.as_deref(), out.as_deref(), strict, locked)
        }),
        Ok(Command::Fab {
            file,
            out,
            route_timeout,
            allow_drc_violations,
            cross_check,
            strict,
            locked,
            constraints_warn,
            pins_locked,
            no_glb,
            order,
        }) => {
            if let Some(code) = sync_pins(file.as_deref(), locked) {
                return code;
            }
            match order {
                Some(request) => run_order(
                    &request,
                    out.as_deref(),
                    route_timeout,
                    allow_drc_violations,
                    cross_check,
                    strict,
                    locked,
                    constraints_warn,
                    no_glb,
                ),
                None => timings::stage("fab.total", "", || {
                    run_fab(
                        file.as_deref(),
                        out.as_deref(),
                        route_timeout,
                        allow_drc_violations,
                        cross_check,
                        strict,
                        locked,
                        constraints_warn,
                        pins_locked,
                        no_glb,
                    )
                }),
            }
        }
        Ok(Command::Lock { file, mode }) => run_lock(file.as_deref(), mode),
        Ok(Command::Editor { file, port }) => run_editor(file.as_deref(), port),
        Ok(Command::Board3d {
            file,
            board,
            json,
            glb,
        }) => run_board3d(file.as_deref(), board.as_deref(), json, glb.as_deref()),
        Ok(Command::Render {
            file,
            board,
            out,
            request,
        }) => run_render(file.as_deref(), board.as_deref(), out.as_deref(), &request),
        Ok(Command::View(request)) => run_view(&request),
        Ok(Command::CatalogRefresh(request)) => run_catalog_refresh(&request),
        Ok(Command::CatalogAge { paths, only, days }) => run_catalog_age(&paths, &only, days),
        Ok(Command::CatalogAdd {
            lcsc,
            to,
            offline,
            no_cache,
        }) => run_catalog_add(&lcsc, to.as_deref(), offline, no_cache),
        Ok(Command::DxfImport {
            file,
            layers,
            holes_layers,
            units,
            board,
            write,
        }) => run_dxf_import(
            &file,
            &netsmith::dxf::Request {
                layers,
                holes_layers,
                units,
            },
            board.as_deref(),
            write.as_deref(),
        ),
        Ok(Command::KicadLibsImport {
            sym_lib_table,
            fp_lib_table,
        }) => run_kicad_libs_import(&sym_lib_table, fp_lib_table.as_deref()),
        Ok(Command::Compare {
            a,
            b,
            board,
            map,
            by_refdes,
            ignore_unconnected,
            loose_values,
        }) => run_compare(&Request {
            a: &a,
            b: &b,
            board: board.as_deref(),
            map: map.as_deref(),
            by_refdes,
            ignore_unconnected,
            loose_values,
        }),
    }
}

/// `netsmith dxf import`: the source to paste goes to stdout, the report and warnings to
/// stderr. With `--write`, the places go into the layout file and the diff is printed.
fn run_dxf_import(
    file: &Path,
    request: &netsmith::dxf::Request,
    board: Option<&str>,
    write: Option<&Path>,
) -> ExitCode {
    use netsmith::dxf;
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    };
    let bytes = match fs::read(file) {
        Ok(b) => b,
        Err(e) => return fail(format!("{}: {e}", file.display())),
    };
    // Old files are often in a Windows code page: the bytes that are not UTF-8 are only in
    // text that is not read.
    let text = String::from_utf8_lossy(&bytes);
    let name = file.display().to_string();
    let imported = match dxf::import(&text, &name, request) {
        Ok(i) => i,
        Err(e) => return fail(e),
    };
    for w in &imported.warnings {
        eprintln!("warning: {w}");
    }
    let mut written = None;
    if let Some(source) = write {
        match dxf::write_physical(source, board, &imported) {
            Ok((b, diff)) => written = Some((b, diff)),
            Err(e) => return fail(e),
        }
    }
    let board_name = written.as_ref().map(|(b, _)| b.as_str()).or(board);
    print!(
        "{}",
        dxf::render(&imported, board_name, &name, written.is_some())
    );
    if let Some((_, diff)) = written {
        println!("\n// physical file written; what changed:");
        print!("{diff}");
    }
    ExitCode::SUCCESS
}

/// `netsmith catalog refresh`: the exit code is 1 if a row is a CONFLICT or MISSING, and 2 if
/// the run had to stop (an endpoint refused, a file could not be read) and wrote nothing.
/// `netsmith catalog add` (#454): exit 0 when the row is written (or the record printed), 1
/// when LCSC has no record or the file already has the number, 2 when the run could not be made.
fn run_catalog_add(lcsc: &str, to: Option<&Path>, offline: bool, no_cache: bool) -> ExitCode {
    use netsmith::refresh::{self, AddError, Network};
    let mut network = Network {
        offline,
        ..Network::default()
    };
    if no_cache {
        network.cache_dir = None;
    }
    let shown = to.map(|p| p.display().to_string());
    match refresh::add(lcsc, to.zip(shown.as_deref()), network) {
        Ok(added) => {
            print!("{}", refresh::render_added(&added));
            ExitCode::SUCCESS
        }
        Err(AddError::Refused(why)) => {
            eprintln!("error: {why}");
            ExitCode::FAILURE
        }
        Err(AddError::Stopped(why)) => {
            eprintln!("error: {why}");
            ExitCode::from(2)
        }
    }
}

fn run_catalog_refresh(request: &RefreshRequest) -> ExitCode {
    use netsmith::refresh::{self, Network};
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    };
    let cwd = match env::current_dir() {
        Ok(d) => d,
        Err(e) => return fail(e.to_string()),
    };
    let targets = match refresh::resolve_targets(&cwd, &request.paths) {
        Ok(t) => t,
        Err(e) => return fail(e),
    };
    let mut network = Network {
        offline: request.offline,
        ..Network::default()
    };
    if let Some(ms) = request.delay_ms {
        network.delay = Duration::from_millis(ms);
    }
    if let Some(hours) = request.cache_ttl_hours {
        network.ttl = Duration::from_secs(hours.saturating_mul(3600));
    }
    if request.no_cache {
        network.cache_dir = None;
    }
    let options = refresh::Options {
        dry_run: request.dry_run,
        only: request.only.clone(),
        lcsc: request.lcsc.clone(),
        network,
    };
    // One line on stderr for each row that is looked up.
    let say = |m: &str| eprintln!("{m}");
    let report = match refresh::refresh(&targets, &options, &say) {
        Ok(r) => r,
        Err(e) => {
            eprint!("{}", refresh::NOTICE);
            return fail(e);
        }
    };
    print!("{}", refresh::render_diff(&report));
    print!("{}", refresh::render_summary(&report));
    if report.has_findings() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run_catalog_age(paths: &[std::path::PathBuf], only: &[String], days: u32) -> ExitCode {
    use netsmith::refresh;
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    };
    let cwd = match env::current_dir() {
        Ok(d) => d,
        Err(e) => return fail(e.to_string()),
    };
    let today = refresh::date_of(refresh::now_secs());
    // The supply provider of the project that contains the directory, if there is one.
    let supply =
        match Project::find(&cwd).and_then(|p| p.map_or(Ok(Supply::none()), |p| p.supply())) {
            Ok(supply) => supply,
            Err(e) => return fail(e),
        };
    match refresh::resolve_targets(&cwd, paths)
        .and_then(|t| refresh::age(&t, only, days, &today, &supply))
    {
        Ok(report) => {
            print!("{}", refresh::render_age(&report));
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn run_init(name: Option<&str>) -> ExitCode {
    match init(Path::new("."), name) {
        Ok(report) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Write the pin file beside each source that has pin requests, when a run changes it (ADR 0101),
/// and say what was written and what is stale. `--locked` writes nothing and stops instead. The
/// exit code of the command when it stops, `None` to go on.
fn sync_pins(file: Option<&str>, locked: bool) -> Option<ExitCode> {
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
fn target(file: Option<&str>, strict: bool) -> Result<(Target, Loader, Catalogs), ExitCode> {
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
fn libraries(target: &Target) -> Result<Libraries, ExitCode> {
    let root = target.project.as_ref().map(|p| p.root.as_path());
    Libraries::for_project(root).map_err(|e| {
        eprintln!("{e}");
        ExitCode::from(2)
    })
}

fn run_kicad_libs_import(sym: &str, fp: Option<&str>) -> ExitCode {
    let read = |path: &str| fs::read_to_string(path).map_err(|e| format!("{path}: {e}"));
    let stanzas = read(sym).and_then(|sym_text| {
        let fp_text = fp.map(read).transpose()?;
        netsmith_kicad::library_stanzas(Some(&sym_text), fp_text.as_deref())
    });
    match stanzas {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

/// Whether a project with no file to act on was found, which it says. A library-only
/// project lists no `top_level` file (ADR 0011).
fn no_boards(target: &Target) -> bool {
    let empty = target.files.is_empty();
    if empty {
        eprintln!(
            "error: this project has no boards: `top_level` in netsmith.toml is empty; name a .nsm file to build it"
        );
    }
    empty
}

/// `netsmith lock --update` and `--check`: the pick lock of each source file, solved again with
/// no holding. The diff goes to stdout; what the lock held that no longer passes, the reason a
/// check fails and every error go to stderr. Exit 0 when nothing is wrong (a lock was written,
/// or is current), 1 when `--check` finds a lock that is not current, 2 when it cannot tell.
fn run_lock(file: Option<&str>, mode: LockMode) -> ExitCode {
    let (target, loader, catalogs) = match target(file, false) {
        Ok(t) => t,
        Err(code) => return code,
    };
    if no_boards(&target) {
        return ExitCode::from(2);
    }
    let mut stale = false;
    for source in &target.files {
        let text = match fs::read_to_string(&source.path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{}: {e}", source.name);
                return ExitCode::from(2);
            }
        };
        let found = match netsmith::pick_lock::refresh(
            &source.path,
            &source.name,
            &text,
            &loader,
            &catalogs,
        ) {
            Ok(found) => found,
            Err(e) => {
                eprintln!("{}", e.trim_end());
                return ExitCode::from(2);
            }
        };
        let name = found.file.display();
        let lines: String = found.diff.iter().map(|l| format!("  {l}\n")).collect();
        if !found.reported.is_empty() {
            let outcome = match mode {
                LockMode::Update => "replaced with the rows above",
                LockMode::Check => {
                    "`lock --update` replaces them with the rows the solver picks now"
                }
            };
            eprintln!(
                "note: the rows {name} holds that no longer pass, in the solver's words ({outcome}):\n{}",
                found.reported.trim_end()
            );
        }
        // The entries of the picks file whose part is gone (#462): the file is a person's, so a
        // refresh leaves it alone, and `--check` fails on them.
        if !found.stale.is_empty() {
            let picks = netsmith::picks::picks_path(&source.path);
            let lines: String = found.stale.iter().map(|l| format!("  {l}\n")).collect();
            match mode {
                LockMode::Update => eprint!(
                    "note: {} names parts the design no longer picks a row for; remove their entries:\n{lines}",
                    picks.display()
                ),
                LockMode::Check => {
                    eprint!(
                        "error: {} names parts the design no longer picks a row for; remove their entries:\n{lines}",
                        picks.display()
                    );
                    stale = true;
                }
            }
        }
        match mode {
            LockMode::Update => {
                if let Err(e) = found.write() {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
                if !found.is_current() {
                    let verb = if found.existed { "updated" } else { "wrote" };
                    println!("{verb} {name}");
                    print!("{lines}");
                }
            }
            LockMode::Check if !found.existed && !found.is_current() => {
                eprintln!(
                    "error: {name} does not exist: run `netsmith lock --update` to write it, and commit it"
                );
                stale = true;
            }
            LockMode::Check if !found.is_current() => {
                println!("{name} is not current");
                print!("{lines}");
                eprintln!(
                    "error: {name} is not current: run `netsmith lock --update` to refresh it, and commit it"
                );
                stale = true;
            }
            LockMode::Check => {}
        }
    }
    if stale {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run_build(file: Option<&str>, out: Option<&Path>, strict: bool, locked: bool) -> ExitCode {
    let (target, loader, catalogs) = match target(file, strict) {
        Ok(t) => t,
        Err(code) => return code,
    };
    let libraries = match timings::stage("libraries", "", || libraries(&target)) {
        Ok(l) => l,
        Err(code) => return code,
    };
    if no_boards(&target) {
        return ExitCode::from(2);
    }
    let out = out.unwrap_or(&target.build_out);
    let mut code = ExitCode::SUCCESS;
    for source in &target.files {
        let text = match fs::read_to_string(&source.path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{}: {e}", source.name);
                return ExitCode::from(2);
            }
        };

        // The pick lock beside the source holds the parts it names to their rows.
        let (held, using) = match netsmith::pick_lock::open(&source.path, &catalogs, locked) {
            Ok(opened) => opened,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
        };
        let catalogs = held.as_ref().unwrap_or(&catalogs);
        let built = outcome_with(&source.name, &text, &loader, catalogs, Some(&libraries))
            .map_err(|problems| render_problems(&problems));
        let settled = built.and_then(|outcome| {
            netsmith::pick_lock::settle(&using, catalogs.pick_lock(), &outcome.lock)
                .map(|settled| (outcome.boards, settled))
                .map_err(|e| format!("{e}\n"))
        });
        match settled {
            Err(problems) => {
                eprint!("{problems}");
                code = ExitCode::FAILURE;
            }
            Ok((boards, settled)) => {
                if let Some(file) = &settled.wrote {
                    println!("wrote {}", file.display());
                }
                for note in &settled.notes {
                    eprintln!("note: {note}");
                }
                // What the requirements prove cannot be placed is said now, with no placing
                // (ADR 0045 section 14); the layout file's locks count.
                let names: Vec<String> = boards.iter().map(|b| b.design.name.clone()).collect();
                let layout =
                    netsmith::layout::read(&netsmith::layout::layout_path(&source.path), &names)
                        .ok()
                        .flatten();
                let mut unplaceable = false;
                for b in &boards {
                    let found = netsmith::fab::precheck_board(
                        b,
                        layout.as_ref().map(|l| &l.0),
                        &libraries,
                        catalogs.constraints_warn(),
                    );
                    for warning in &found.warnings {
                        eprintln!("warning: {}: {warning}", found.board);
                    }
                    for error in &found.errors {
                        eprintln!("error: {}: {error}", found.board);
                        unplaceable = true;
                    }
                }
                if unplaceable {
                    code = ExitCode::FAILURE;
                    continue;
                }
                for b in boards {
                    let dir = match b.write(out) {
                        Ok(dir) => dir,
                        Err(e) => {
                            eprintln!("{}: {e}", b.dir(out).display());
                            return ExitCode::from(2);
                        }
                    };
                    // The project's own libraries and the standard library's own files that
                    // the board uses, so KiCad finds the symbols.
                    if let Err(e) = libraries.write_tables(&dir, &b.design) {
                        eprintln!("{}: {e}", dir.display());
                        return ExitCode::from(2);
                    }
                    print!("{}", b.report);
                    for note in &b.notes {
                        eprintln!("note: {note}");
                    }
                    for warning in b.guideline_warnings() {
                        eprintln!("warning: {warning}");
                    }
                    for note in b.power_notes() {
                        eprintln!("note: {note}");
                    }
                    println!("wrote {}", dir.display());
                }
            }
        }
    }
    code
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
) -> ExitCode {
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
        progress: Some(|message| eprintln!("{message}")),
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
        match fab_with_options(
            &source.name,
            &text,
            &loader,
            out,
            held.as_ref().unwrap_or(&catalogs),
            &libraries,
            kicad.as_ref().map(|k| k as &dyn KicadCli),
            &options,
        ) {
            Err(problems) => {
                eprint!("{problems}");
                code = ExitCode::FAILURE;
            }
            Ok(boards) => {
                for b in boards {
                    print!("{}", b.summary);
                    println!("wrote {}", b.dir.display());
                    if !b.notes.trim().is_empty() {
                        eprintln!("\nbefore you order ({}):\n{}", b.board, b.notes.trim_end());
                    }
                }
            }
        }
    }
    code
}

/// What `board3d` and `render` do first: the one board `file` and `--board` pick, built and
/// placed as `fab` places it (the pick lock and the layout file beside its source are read,
/// nothing is written), and `then` of it. A project of several top-level files takes the first
/// that has the board `--board` names. The error is printed and is the exit code: 1 when no file
/// builds the board or places it, 2 when the command cannot start.
fn with_placed_board<T>(
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

/// `netsmith board3d`: one board placed as `fab` places it (the pick lock and the layout file
/// beside its source are read, nothing is written) and its 3D scene printed: the JSON report on
/// stdout with `--json`, the scene as a `.glb` file with `--glb` (and no summary), a summary with
/// neither. Exit 0 when the report is printed (and the file written), 1 when the board does not
/// build or place, 2 when the command cannot start or the file cannot be written.
fn run_board3d(
    file: Option<&str>,
    board: Option<&str>,
    json: bool,
    glb: Option<&Path>,
) -> ExitCode {
    let made = with_placed_board(file, board, |placed| placed.run());
    let (report, scene) = match made {
        Ok(made) => made,
        Err(code) => return code,
    };
    if let Some(out) = glb {
        let bytes = netsmith_pcb::gltf::glb(&scene, &report.board.name);
        if let Err(e) = fs::write(out, bytes) {
            eprintln!("error: {}: {e}", out.display());
            return ExitCode::from(2);
        }
    }
    if json {
        print!("{}", report.json());
    } else if glb.is_none() {
        print!("{}", report.summary());
    }
    ExitCode::SUCCESS
}

/// `netsmith render`: one board placed as `board3d` places it, drawn by netsmith's own
/// rasterizer and written as a PNG (`<board>-<view>.png` unless `--out` says a file); the path
/// written is printed. Exit 0 when the picture is written, 1 when the board does not build or
/// place, 2 when the command cannot start, an id of `--highlight` is not the board's, or the file
/// cannot be written.
fn run_render(
    file: Option<&str>,
    board: Option<&str>,
    out: Option<&Path>,
    request: &netsmith::render::Request,
) -> ExitCode {
    let (scene, label) = match with_placed_board(file, board, |placed| placed.scene_and_label()) {
        Ok(made) => made,
        Err(code) => return code,
    };
    let png = match netsmith::render::draw(&scene, request) {
        Ok(png) => png,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let out = out.map_or_else(
        || netsmith::render::default_out(&label, request.view),
        Path::to_path_buf,
    );
    if let Err(e) = fs::write(&out, png) {
        eprintln!("error: {}: {e}", out.display());
        return ExitCode::from(2);
    }
    println!("{}", out.display());
    ExitCode::SUCCESS
}

/// `netsmith view`: read or change the 3D view of a running editor's page. A refusal (the
/// editor's own message), no editor and no page open all exit 2.
fn run_view(request: &netsmith::view_client::Request) -> ExitCode {
    match netsmith::view_client::run(request) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
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

/// Exit 0 when the designs match, 1 when they differ, 2 when they could not be compared or
/// the comparison could not be decided.
fn run_compare(request: &Request) -> ExitCode {
    let kicad = KicadCliExe::locate();
    match compare_files(request, kicad.as_ref().map(|k| k as &dyn KicadCli)) {
        Ok(done) => match done.verdict {
            Verdict::Match => {
                print!("{}", done.text);
                ExitCode::SUCCESS
            }
            Verdict::Different => {
                print!("{}", done.text);
                ExitCode::FAILURE
            }
            Verdict::Undecided => {
                eprint!("{}", done.text);
                ExitCode::from(2)
            }
        },
        Err(e) => {
            eprintln!("error: {}", e.trim_end());
            ExitCode::from(2)
        }
    }
}

fn run_editor(file: Option<&str>, port: u16) -> ExitCode {
    if !netsmith_editor::bundle_is_present() {
        eprintln!("{}", netsmith_editor::missing_bundle_message());
        return ExitCode::from(2);
    }
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot listen on 127.0.0.1:{port}: {e} (try another with --port)");
            return ExitCode::from(2);
        }
    };
    let port = listener.local_addr().map_or(port, |a| a.port());
    let target = match Target::resolve(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    // Not analysed yet: the server listens first and the analysis runs behind it, so a page
    // that connects during it is told so (ADR 0019).
    let workspace = match Workspace::open_deferred(target) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    // A comparison's scratch folder is removed when it ends; one that a killed editor (or a
    // Ctrl-C that the system ended at once) left behind is removed now.
    let swept = netsmith::editor::sweep_scratch();
    if swept > 0 {
        eprintln!("removed {swept} leftover scratch folder(s) of earlier comparisons");
    }
    println!(
        "editing {} at http://127.0.0.1:{port}/ (Ctrl-C to stop)",
        workspace.title()
    );
    match netsmith_editor::serve(listener, Arc::new(workspace)) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("the editor stopped: {e}");
            ExitCode::from(2)
        }
    }
}
