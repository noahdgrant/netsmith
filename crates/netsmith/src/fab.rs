//! `netsmith fab`: from source text to the files a board house wants.
//!
//! For each board it builds the design, places and routes a PCB, writes the board and its
//! design rules, and packages Gerbers, a drill file, a bill of materials and a placement
//! file for JLCPCB. It refuses, with the reason, rather than write files that would not
//! make a working board: an unplaceable or unroutable board, a design rules violation, or an
//! assembled part with no part number. The Gerbers, drill files, placement file and design
//! rules check are netsmith's own (ADR 0034, ADR 0037); KiCad is only the optional
//! cross-check.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use netsmith_check::Loader;
use netsmith_core::PanelSpec;
use netsmith_fab::panel::{CellFacts, CellInput, Spec};
use netsmith_fab::{BomEntry, Fit, KicadCli, Options, Package, Side, package};
use netsmith_kicad::Mapping;
use netsmith_lower::{Assembly, Sourced};
use netsmith_pcb::{
    Board, RouteOptions, apply_layout, kicad_pcb, kicad_pro, place_options, route_with, svg,
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
        let (sourcing, unresolved) = resolve_generics(&built.sourcing, &built.rechecks, catalogs);
        let unresolved: Vec<String> = unresolved
            .iter()
            .filter(|u| {
                built
                    .sourcing
                    .iter()
                    .any(|s| s.refdes == u.refdes && s.assembled())
            })
            .map(|u| fail(u.line()))
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

        say(format!(
            "{}: placing {} parts",
            built.label(),
            built.design.parts.len()
        ));
        let mut board = Board::from_design_placed(
            &built.design,
            &libraries.mapping,
            &libraries.footprints,
            &place_options(&built.design, layout.as_ref().map(|l| &l.0)),
        )
        .map_err(|e| fail(e.to_string()))?;
        if let Some((layout, file)) = layout {
            apply_layout(&mut board, layout, file).map_err(|e| fail(e.to_string()))?;
        }
        let problems: Vec<String> = board
            .check_placement()
            .iter()
            .map(ToString::to_string)
            .collect();
        if !problems.is_empty() {
            return Err(problems.iter().map(|p| fail(p.clone())).collect());
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
        say(format!(
            "{}: placed on a {} by {} mm board",
            built.label(),
            board.outline.width(),
            board.outline.height()
        ));
        let routing = route_with(
            &mut board,
            RouteOptions {
                time_limit: options.route_timeout,
                progress: options.progress,
                ..RouteOptions::default()
            },
        );
        let unrouted = routing.left;
        if !unrouted.is_empty() {
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
            return Err(fail(msg));
        }
        say(format!("{}: routed, writing files", built.label()));

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

        // The project's own libraries, so KiCad finds the footprints.
        for (table, text) in netsmith_kicad::library_tables(libraries.mapping.libraries(), &dir) {
            let file = dir.join(table);
            fs::write(&file, text).map_err(|e| fail(format!("{}: {e}", file.display())))?;
        }

        let entries = bom_entries(&sourcing, &libraries.mapping).map_err(fail)?;
        // Guidelines that the design goes against (ADR 0036) are for the person ordering, so
        // they are in `fab-notes.txt`.
        let mut extras = board.process_extras();
        extras.extend(crate::no_supplier::note(&sourcing));
        // The notes say which part hangs over which edge, how far, and who said it may; each
        // is a warning (owner decision, 2026-11-03, ADR 0045 section 10).
        extras.extend(board.hangs().into_iter().map(|h| format!("warning: {h}")));
        extras.extend(unchecked.iter().map(|u| format!("warning: {u}")));
        extras.extend(
            built
                .guideline_warnings()
                .into_iter()
                .map(|w| format!("warning: {w}")),
        );
        let package_options = Options {
            allow_drc_violations: options.allow_drc_violations,
            drc: netsmith_fab::drc::Extras {
                mask_web: Some(board.rules.mask_bridge),
                via_in_pad: Some(board.process.via_in_pad),
            },
            process: Some(board.process_note()),
            extras,
            fit: fit_of(&built.sourcing),
            // The design's requirements on where parts go (ADR 0045): an error that stops
            // the package, or a warning with `constraints = "warn"`.
            constraints: crate::constraints::of_design(&built.design, catalogs.constraints_warn()),
            ..Options::default()
        };
        let pkg =
            package(&pcb_path, &entries, &package_options).map_err(|e| fail(e.to_string()))?;
        let mut files = vec![pcb_path.clone()];
        files.extend(pkg.write(&dir).map_err(|e| fail(e.to_string()))?);
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
        assembled_parts: pkg.plan.assembled.len(),
        bom: pkg.plan.bom.clone(),
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
            };
            (s.refdes.clone(), fit)
        })
        .collect()
}

/// The bill of materials: one entry per distinct part, with every place it is used. Parts
/// the board house does not place are left out. A part it should place but that has no
/// part number is kept without one, so the packaging step can refuse it by name.
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
        match entries
            .iter_mut()
            .find(|e| e.value == s.value && e.footprint == footprint && e.lcsc == s.lcsc)
        {
            Some(entry) => entry.designators.push(s.refdes.clone()),
            None => entries.push(BomEntry {
                designators: vec![s.refdes.clone()],
                value: s.value.clone(),
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

#[cfg(test)]
mod tests {
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
}
