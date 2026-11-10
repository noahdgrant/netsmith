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

use netsmith_check::Loader;
use netsmith_core::{PanelSpec, timings};
use netsmith_design::{Assembly, Sourced};
use netsmith_fab::panel::{CellFacts, CellInput, Spec};
use netsmith_fab::{BomEntry, Fit, KicadCli, Options, Package, Side, package};
use netsmith_kicad::Mapping;
use netsmith_pcb::{
    Board, RouteOptions, Trim, apply_layout, kicad_pcb, kicad_pro, place_options, route_with, svg,
};
use netsmith_solve::Catalogs;

use crate::order::{BoardFacts, PanelFacts};
use crate::resolve::resolve_generics;

use crate::{Built, Libraries, outcome_with, render_problems};

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
}

/// What `netsmith fab` says when it finds no KiCad. netsmith's own design rules check and its
/// Gerbers need none; KiCad is only the witness of `--cross-check`, which plots the boards and
/// runs KiCad's design rules check to compare with netsmith's own (ADR 0034, ADR 0037).
pub const NO_KICAD_NOTE: &str = "note: KiCad is not installed, so --cross-check (KiCad's plots and design rules check, compared with netsmith's own) is not available; netsmith's own design rules check still runs";

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
/// (ADR 0045, "Height limits", PROVISIONAL).
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
    /// no height is then an error and not a warning (ADR 0045, "Height limits", PROVISIONAL).
    /// `--allow-drc-violations` does not waive it.
    pub strict_heights: bool,
    /// The pick lock of the source (`pick_lock`): written when it does not exist, compared with
    /// the picks when it does, and with `locked` an error if they differ, before anything is
    /// placed. The solver holds parts to it through the catalogs, which the caller gives. `None`
    /// leaves the lock out of the run.
    pub lock: Option<crate::pick_lock::Use>,
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
        }
    }
}

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
    let say = |message: String| {
        if let Some(progress) = options.progress {
            progress(&message);
        }
    };
    let mut made = Vec::new();
    let outcome = outcome_with(name, text, loader, catalogs, Some(libraries))
        .map_err(|problems| render_problems(&problems))?;
    if let Some(using) = &options.lock {
        let settled = crate::pick_lock::settle(using, catalogs.pick_lock(), &outcome.lock)
            .map_err(|e| format!("{e}\n"))?;
        if let Some(file) = settled.wrote {
            say(format!("wrote {}", file.display()));
        }
        for note in settled.notes {
            say(format!("note: {note}"));
        }
    }
    let (built_boards, panels) = (outcome.boards, outcome.panels);
    let board_names: Vec<String> = built_boards.iter().map(|b| b.design.name.clone()).collect();
    // The layout file beside the source, if there is one: it says which face each part is on
    // and where things are, and a wrong entry stops fab before anything is placed.
    let layout = match &options.layout {
        Some(file) => {
            crate::layout::read(file, &board_names).map_err(|e| format!("error: {name}: {e}\n"))?
        }
        None => None,
    };
    for built in &built_boards {
        if let Some(only) = &options.only
            && !only.contains(&built.label())
        {
            continue;
        }
        let run = fab_board(
            built, out, &layout, libraries, kicad, options, catalogs, &say,
        )?;
        made.push(run.fabricated);
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
        let run = fab_board(
            built,
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

/// What the requirements prove about a board before any placing (ADR 0045 section 14,
/// PROVISIONAL): lines that cannot all hold, a courtyard past an edge nobody stated, a part
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
    match Board::precheck(
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
            p.refdes,
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
) -> Result<BoardRun, String> {
    {
        let fail = |what: String| format!("error: {}: {what}\n", built.label());
        let name = &built.design.name;

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
        let mut placement = place_options(&built.design, layout.as_ref().map(|l| &l.0));
        placement.lines_warn = catalogs.constraints_warn();
        let mut board = timings::stage("fab.place", &label, || {
            Board::from_design_placed(
                &built.design,
                &libraries.mapping,
                &libraries.footprints,
                &placement,
            )
        })
        .map_err(|e| match e {
            netsmith_pcb::BoardError::DoesNotFit(e) => fail(placement_failure(&e, out)),
            e => fail(e.to_string()),
        })?;
        if let Some((layout, file)) = layout {
            apply_layout(&mut board, layout, file).map_err(|e| fail(e.to_string()))?;
        }
        let problems: Vec<String> = board
            .check_placement()
            .iter()
            .map(ToString::to_string)
            .collect();
        // When the placer could not meet the requirements of the physical file, the faults
        // above are its symptoms: say first which lines are not met and which cannot all hold,
        // and keep the same list in `placement-failure.txt` (ADR 0045, PROVISIONAL).
        let unmet = board.explain_requirements(&built.design, &placement);
        if !problems.is_empty() {
            let mut errors = String::new();
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
            return Err(fail(requirement_failure(&report, &[], &board, out)));
        }
        for hang in board.hangs() {
            say(format!("{}: warning: {hang}", built.label()));
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
            "{}: placed on a {} by {} mm board",
            built.label(),
            board.outline.width(),
            board.outline.height()
        ));
        let routing = timings::stage("fab.route", &label, || {
            route_with(
                &mut board,
                RouteOptions {
                    time_limit: options.route_timeout,
                    progress: options.progress,
                    ..RouteOptions::default()
                },
            )
        });
        let unrouted = routing.left;
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
                report: routing_failure(&built.label(), board.outline.min, &unrouted, routing.tries),
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
            (dir.join(format!("{name}.svg")), svg::render(&board)),
        ] {
            fs::write(&file, contents).map_err(|e| fail(format!("{}: {e}", file.display())))?;
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
        extras.extend(crate::no_supplier::note(&sourcing));
        // The notes say which part hangs over which edge, how far, and who said it may; each
        // is a warning (owner decision, 2026-11-03, ADR 0045 section 10).
        extras.extend(board.hangs().into_iter().map(|h| format!("warning: {h}")));
        extras.extend(unchecked.iter().map(|u| format!("warning: {u}")));
        extras.extend(crate::heights::notes(&built.design, &built.catalog_heights));
        extras.extend(
            board
                .trims()
                .iter()
                .flat_map(Trim::unchecked)
                .map(|u| format!("warning: {u}")),
        );
        extras.extend(
            built
                .guideline_warnings()
                .into_iter()
                .map(|w| format!("warning: {w}")),
        );
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
            constraints: crate::constraints::of_design(&built.design, catalogs.constraints_warn()),
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
            let report = netsmith_fab::crosscheck::check(
                &pcb_path,
                kicad,
                &netsmith_fab::crosscheck::Limits {
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

/// What is written for a board the router left connections of.
const ROUTING_FAILURE: &str = "routing-failure.txt";

/// A board with connections the router could not make: the message of the error `fab` ends
/// in, the text of [`ROUTING_FAILURE`], and one line of the notes for each connection.
struct Incomplete {
    message: String,
    report: String,
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

/// The text of `routing-failure.txt`: every connection that has no copper, with its net and
/// its two pads and where they are, and the parts most of them end on.
fn routing_failure(
    label: &str,
    origin: netsmith_pcb::Pt,
    left: &netsmith_pcb::Ratsnest,
    tries: usize,
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
    let wrote = fs::write(&file, &incomplete.report);
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
    let panel = Spec {
        name: spec.name.clone(),
        columns: spec.columns,
        rows: spec.rows,
        rail: spec.rail,
        rules: panel_rules(&spec.rules),
    };
    let input = CellInput {
        pcb: &pcb,
        pro: pro.as_deref(),
        entries: &run.entries,
        facts: CellFacts {
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
    let pkg = netsmith_fab::panel::build(&panel, &input, &package_options)
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
        let report = netsmith_fab::crosscheck::check(
            &dir.join(format!("{}.kicad_pcb", spec.name)),
            kicad,
            &netsmith_fab::crosscheck::Limits {
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
    })
}

/// The rules of the board house for the writer, from the language's.
fn panel_rules(r: &netsmith_core::PanelRules) -> netsmith_fab::panel::Rules {
    netsmith_fab::panel::Rules {
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
/// Why a consigned part looks like this (ADR 0084, provisional). `Fab` is "the board house
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
/// `bom.csv` with an empty cell for a registered consigned part from netsmith; the notes tell
/// the person ordering to look at the matching interface (the `prepare-fab-order` step). If
/// JLCPCB documents a marker, the mark changes in this one function.
///
/// Where it is not shown: the editor's bill of materials shows a consigned part as an ordinary
/// line with no number and no price (its API has a `hand_solder` flag and no consigned one, and
/// a new field changes the generated types and the committed bundle).
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

/// What the design rules check needs that the board file does not say: the process's mask
/// web and widest filled via, and the courtyards a protruding pin excuses (ADR 0045 section 15).
pub(crate) fn drc_extras(board: &Board) -> netsmith_fab::drc::Extras {
    netsmith_fab::drc::Extras {
        mask_web: Some(board.rules.mask_bridge),
        via_in_pad: Some(board.process.via_in_pad),
        excused_courtyards: board.excused_courtyards(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_note_without_kicad_says_netsmiths_own_check_still_runs() {
        let note = super::NO_KICAD_NOTE;
        assert!(note.contains("--cross-check"), "{note}");
        assert!(
            note.contains("netsmith's own design rules check still runs"),
            "{note}"
        );
        assert!(!note.contains("none of its own"), "{note}");
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
        let text = routing_failure("Demo", netsmith_pcb::Pt::new(10.0, 20.0), &left, 4);
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
    }

    #[test]
    fn an_incomplete_board_writes_its_report_and_the_error_says_where_everything_is() {
        let dir = netsmith_kicad::testlib::TempDir::new("incomplete");
        let incomplete = Incomplete {
            message: "1 connections could not be routed (after 4 tries):\n  net A: U1.1 to R1.1"
                .into(),
            report: "INCOMPLETE: Demo\n".into(),
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
            incomplete.headline(),
            "1 connection(s) have no copper; this package is NOT ready to order, see routing-failure.txt"
        );
    }
}
