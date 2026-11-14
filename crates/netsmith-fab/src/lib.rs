//! Fabrication files for JLCPCB: Gerbers and drill files, a bill of materials, and a
//! placement file, made from a `.kicad_pcb`.
//!
//! The input is the board file KiCad reads plus the part numbers, as [`BomEntry`] values.
//! [`package`] writes
//!
//! - `<board>-gerbers.zip`: Gerbers and Excellon drill files, in a top folder `gerber/`
//! - `bom.csv`: parts JLCPCB assembles, one row per kind of part
//! - `cpl.csv`: where each of those parts sits, with JLCPCB's rotation conventions
//! - `fab-notes.txt`: what a person still has to do or check
//!
//! netsmith writes the Gerbers, the drill files and the placement file itself, from the
//! board file ([`artwork`], [`plot`], [`drill`], [`fill`], [`cpl`]), and checks the board's
//! design rules itself ([`drc`], ADR 0037); KiCad is not needed for any of it. KiCad's
//! command-line tool is used for one optional thing, the cross-check ([`crosscheck`]),
//! which plots the board and runs its design rules check and compares both with
//! netsmith's.
//!
//! Through-hole parts are not assembled unless the caller says so ([`Options::fit`]): they
//! are left out of the BOM and placement file and listed in the notes as hand-soldered. All coordinates share one origin, the board's
//! drill/place origin (`aux_axis_origin`).
//!
//! The board is read from its file with netsmith's own reader (`netsmith-sexp`), and its
//! geometry, the drawing of its layers and the design rules check are `netsmith-geom`'s, which
//! the board model and the router share. Of the other crates it uses only netsmith-kicad (to
//! find `kicad-cli`) and netsmith-core (the PNG writer).
//!
//! # Why netsmith writes its own files (ADR 0034)
//!
//! A person with the `netsmith` binary alone must be able to make the files a board house takes
//! (ADR 0021's direction, continued by ADR 0097). Until the owner's review of 2026-11-04
//! `kicad-cli` plotted the Gerbers, drilled the Excellon files and wrote the position file; now
//! this crate writes all three from the board file, and `kicad-cli` is only an optional
//! witness (the cross-check). The writers are the way: there is no flag that gives back KiCad's
//! files, and [`package`] takes no KiCad. The design rules check is netsmith's own (ADR 0037),
//! so `fab` has no "not run (KiCad not installed)" path. The pour fill is part of this
//! because a board with pours cannot be plotted without it.
//!
//! **Status of the design.** The owner accepted it on 2026-11-04 and kept it `accepted` on
//! 2026-11-11, although no package has been through JLCPCB's order preview yet. That check is
//! open, not a reason for another status: one real package must go through JLCPCB's order preview
//! and JLCPCB's own design check (issue #12; a person must do it, checklist in issue #10, and it
//! needs an order account). The person looks at the planes (the pours are written in clear
//! polarity inside a copper file, which KiCad does not do) and at the silkscreen (our font), and
//! records the board, the date, what the preview showed and any file JLCPCB refused or changed in
//! ADR 0034. Until then nothing here claims that JLCPCB accepts these files, and every package
//! JLCPCB accepted before was KiCad's. Result: none yet.
//!
//! **What is not checked, and the limits.** JLCPCB has not been given these files. They were
//! accepted by KiCad's Gerber loader only in the sense that KiCad's plot of the same board
//! agrees with them. No other Gerber viewer was available to script: the only independent readers
//! are KiCad's plot (compared, not loaded) and the rasteriser of [`raster`], which reads
//! KiCad's own files as well as ours (a test renders every file of three KiCad packages). Fill:
//! copper islands and slivers narrower than the zone's minimum thickness are not removed, a
//! spoke into another net's clearance is kept as a stub, `connect_pads` is read (thermal, solid,
//! none, through-hole only) with the per-pad override, and hatched fills, zone priorities and
//! per-zone island settings are not. A pad's `remove_unused_layers` is approximated: an inner
//! layer keeps the pad if a track of its net ends on the pad or a zone of its net covers it (KiCad
//! asks the connectivity engine). Text uses our font, so the silkscreen's letters differ from
//! KiCad's, `bold` is ignored, and text boxes, curves (`fp_curve`) and images are not plotted
//! and are named in the notes. Without a `.kicad_pro` beside the board the zone fill uses no
//! edge clearance and the zone's own hole clearance (netsmith always writes one).

#![cfg_attr(not(test), warn(unreachable_pub))]

mod board;
mod bom;
mod cpl;
mod crosscheck;
mod csv;
mod exclusive;
mod gerber;
mod kicad;
mod kicad_drc;
mod panel;
mod raster;
mod rotation;
mod score;

// The board's geometry, the drawing of its layers and the design rules check are
// `netsmith-geom`'s, which the board model and the router share (issue #611); the code here
// names them as modules, and other crates import them from `netsmith_geom`.
use netsmith_geom::{
    ConstraintOutcome, Constraints, DrcCounts, DrcError, DrcExtras, DrcFinding, DrcReport, DrcRule,
    constraint_outcome_findings, drc_check, drc_project_of,
};

use netsmith_geom::{
    Artwork, ArtworkError, GerberError, PlotRules, drill_files, plot_layer, plot_rules_of,
};

pub use board::{Board, BoardError as FabBoardError, BoardPart, Fit, Side};
pub use bom::{BomEntry, BomError, from_board as bom_from_board, parse_csv as parse_bom_csv};
pub use cpl::CplError;
pub use crosscheck::{
    Error as CrossCheckError, LayerResult as CrossCheckLayerResult, Limits as CrossCheckLimits,
    Report as CrossCheckReport, check as cross_check,
};
pub use csv::CsvError;
pub use exclusive::{Ran, run as run_exclusive};
pub use gerber::zip as gerber_zip;
pub use kicad::{CliError, KicadCli, KicadCliExe, Output};
pub use kicad_drc::{
    Comparison as KicadDrcComparison, OUT_OF_SCOPE as KICAD_DRC_OUT_OF_SCOPE,
    Reported as KicadDrcReported, compare as compare_kicad_drc, parse as parse_kicad_drc,
};
pub use panel::{
    Cell as PanelCell, CellFacts as PanelCellFacts, CellInput as PanelCellInput,
    Finding as PanelFinding, Geometry as PanelGeometry, PanelPackage, Rules as PanelBuildRules,
    Spec as PanelBuildSpec, build as build_panel, check as check_panel, geometry as panel_geometry,
    mm as panel_mm, svg as panel_svg,
};
pub use raster::{
    Canvas, Diff as RasterDiff, Hole as ExcellonHole, Image as RasterImage, Point as RasterPoint,
    RasterError, View as RasterView, capsule as raster_capsule, compare_tolerant,
    disc as raster_disc, gerber_extent, parse_excellon, render_gerber,
};
pub use score::{
    Bends as ScoreBends, Corner as ScoreCorner, Decoupling as ScoreDecoupling, PairScore,
    PourScore, Scorecard,
};

use std::path::{Path, PathBuf};

/// Choices that relax a check, which are off by default, and what the board was made to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Options {
    /// Package a board even though netsmith's design rules check reports errors (a
    /// violation, an unconnected pad or a footprint error). The notes say so.
    pub allow_drc_violations: bool,
    /// What the design rules check needs that the board file does not say (the process's
    /// mask web and the widest via it fills).
    pub drc: DrcExtras,
    /// The design's requirements on where parts go (ADR 0045), which the board file does not
    /// hold: each is measured on the board and is a finding of the check if it is not met.
    pub constraints: Option<Constraints>,
    /// Package even though some parts have no LCSC part number. Those parts are left out
    /// of the BOM and placement file and listed in the notes as unassembled.
    pub allow_missing_lcsc: bool,
    /// The manufacturing process the board was routed and checked to, as one line for
    /// the notes (see `netsmith_pcb::Board::process_note`). The board file does not carry
    /// it, so the caller that made the board says it.
    pub process: Option<String>,
    /// Lines the caller adds to the notes, after the process: what the board needs of the
    /// fabricator beyond its standard process.
    pub extras: Vec<String>,
    /// The manufacturing steps the design states for through-hole pins cut short after
    /// soldering, one line each, for the notes as `trim:` lines beside `hand-solder:`
    /// (ADR 0045 section 15).
    pub trim: Vec<String>,
    /// How the design says each part, by designator, is fitted. It overrides what the
    /// board file's attributes say; a part it does not name is read from them. A designator
    /// that is not a part of the board file (a hole or a fiducial whose footprint is left
    /// out of the bill of materials) is still listed in the notes.
    pub fit: std::collections::BTreeMap<String, Fit>,
}

#[derive(Debug)]
pub enum Error {
    Io(String),
    Board(board::BoardError),
    Bom(bom::BomError),
    /// Designators in the BOM that are not parts on the board.
    NotOnBoard(Vec<String>),
    /// Surface-mount parts on the board that the BOM does not list.
    NotInBom(Vec<String>),
    /// Parts to be assembled that have no LCSC part number: `(designators, value, footprint)`.
    MissingLcsc(Vec<(Vec<String>, String, String)>),
    Cli(CliError),
    Artwork(ArtworkError),
    /// The design rules check cannot read the board or its project file.
    DrcInput(DrcError),
    /// The design rules check found errors and the caller did not allow going on.
    Drc {
        counts: DrcCounts,
        /// The errors, one to a line.
        report: String,
    },
    /// Requirements of the design on where parts go (ADR 0045) that the board does not meet.
    /// Unlike a design rule, `allow_drc_violations` does not waive them; only making them
    /// warnings does (`constraints = "warn"`).
    Constraints {
        count: usize,
        /// The errors, one to a line.
        report: String,
    },
    Gerber(GerberError),
    Cpl(cpl::CplError),
    /// A panel that cannot be made, one line for each reason, each with the numbers.
    Panel(Vec<String>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(m) => write!(f, "{m}"),
            Self::Board(e) => write!(f, "{e}"),
            Self::Bom(e) => write!(f, "{e}"),
            Self::NotOnBoard(ds) => write!(
                f,
                "the BOM lists {} but the board has no such part (footprints excluded from the BOM do not count)",
                ds.join(", ")
            ),
            Self::NotInBom(ds) => write!(
                f,
                "the board has surface-mount parts the BOM does not list: {}",
                ds.join(", ")
            ),
            Self::MissingLcsc(entries) => {
                writeln!(
                    f,
                    "{} kind(s) of part have no LCSC part number, so JLCPCB cannot assemble them:",
                    entries.len()
                )?;
                for (ds, value, footprint) in entries {
                    writeln!(f, "  {} ({value}, {footprint})", ds.join(", "))?;
                }
                write!(
                    f,
                    "add the part numbers, or allow listing them as unassembled (Options::allow_missing_lcsc, or --allow-missing-lcsc of the netsmith-fab example; `netsmith fab` has no such flag and refuses these parts earlier, naming them: say `.assembly &= AssemblyMode::Hand` for them in the design)"
                )
            }
            Self::Cli(e) => write!(f, "{e}"),
            Self::Artwork(e) => write!(f, "{e}"),
            Self::DrcInput(e) => write!(f, "design rules check: {e}"),
            Self::Drc { counts, report } => write!(
                f,
                "the design rules check found {} violation(s), {} unconnected pad(s) and {} footprint error(s); fix them, or allow packaging anyway (Options::allow_drc_violations, --allow-drc-violations). The errors:\n\n{}",
                counts.violations,
                counts.unconnected,
                counts.footprint_errors,
                report.trim_end()
            ),
            Self::Constraints { count, report } => write!(
                f,
                "{count} requirement(s) of the design on where parts go are not met; --allow-drc-violations does not waive them, only `constraints = \"warn\"` in netsmith.toml or --constraints warn lists them as warnings instead. The errors:\n\n{}",
                report.trim_end()
            ),
            Self::Gerber(e) => write!(f, "{e}"),
            Self::Cpl(e) => write!(f, "{e}"),
            Self::Panel(lines) => write!(f, "{}", lines.join("\n")),
        }
    }
}

impl std::error::Error for Error {}

impl From<board::BoardError> for Error {
    fn from(e: board::BoardError) -> Self {
        Self::Board(e)
    }
}
impl From<bom::BomError> for Error {
    fn from(e: bom::BomError) -> Self {
        Self::Bom(e)
    }
}
impl From<ArtworkError> for Error {
    fn from(e: ArtworkError) -> Self {
        Self::Artwork(e)
    }
}
impl From<DrcError> for Error {
    fn from(e: DrcError) -> Self {
        Self::DrcInput(e)
    }
}
impl From<CliError> for Error {
    fn from(e: CliError) -> Self {
        Self::Cli(e)
    }
}
impl From<GerberError> for Error {
    fn from(e: GerberError) -> Self {
        Self::Gerber(e)
    }
}
impl From<cpl::CplError> for Error {
    fn from(e: cpl::CplError) -> Self {
        Self::Cpl(e)
    }
}

/// Which parts JLCPCB assembles and what is left for a person.
#[derive(Debug, PartialEq)]
pub struct Plan {
    /// Grouped BOM rows of the parts to assemble from JLCPCB's stock, every one with an LCSC
    /// number.
    pub bom: Vec<BomEntry>,
    /// Grouped BOM rows of the consigned parts, which the owner ships to JLCPCB (ADR 0084).
    /// They need no LCSC number, and `bom.csv` has them beside `bom`, with the manufacturer's
    /// number as the comment and the LCSC cell empty unless the part's row has a number.
    pub consigned: Vec<BomEntry>,
    /// The board's parts to place, in the BOM or among the consigned.
    pub assembled: Vec<BoardPart>,
    pub hand_solder: Vec<String>,
    pub not_fitted: Vec<String>,
    /// Parts without an LCSC number that were allowed through.
    pub unassembled: Vec<String>,
}

impl Plan {
    /// The rows of `bom.csv`: the parts from stock and the consigned parts, by first
    /// designator.
    pub fn bom_rows(&self) -> Vec<BomEntry> {
        let mut rows: Vec<BomEntry> = self.bom.iter().chain(&self.consigned).cloned().collect();
        rows.sort_by(|a, b| bom::designator_cmp(&a.designators[0], &b.designators[0]));
        rows
    }
}

/// Decides which parts go to JLCPCB. Needs no KiCad.
pub fn plan(board: &Board, entries: &[BomEntry], options: &Options) -> Result<Plan, Error> {
    let grouped = bom::group(entries)?;
    let find = |d: &str| board.parts.iter().find(|p| p.designator == d);
    let fit_of = |p: &BoardPart| options.fit.get(&p.designator).copied().unwrap_or(p.fit);

    let unknown: Vec<String> = grouped
        .iter()
        .flat_map(|e| &e.designators)
        .filter(|d| find(d).is_none())
        .cloned()
        .collect();
    if !unknown.is_empty() {
        return Err(Error::NotOnBoard(sorted(unknown)));
    }

    let listed = |d: &str| grouped.iter().any(|e| e.designators.iter().any(|x| x == d));
    let unlisted: Vec<String> = board
        .parts
        .iter()
        .filter(|p| matches!(fit_of(p), Fit::Assembled | Fit::Consigned) && !listed(&p.designator))
        .map(|p| p.designator.clone())
        .collect();
    if !unlisted.is_empty() {
        return Err(Error::NotInBom(sorted(unlisted)));
    }

    // Keep only the parts that are surface mount and populated, in the grouped rows.
    let mut bom = Vec::new();
    let mut consigned = Vec::new();
    let mut missing = Vec::new();
    let mut unassembled = Vec::new();
    for entry in &grouped {
        // A consigned part is its own row, even where a part to assemble from stock has the
        // same value, footprint and number: JLCPCB does not mix the two in one line.
        let with_fit = |fit: Fit| -> Vec<String> {
            entry
                .designators
                .iter()
                .filter(|d| find(d).is_some_and(|p| fit_of(p) == fit))
                .cloned()
                .collect()
        };
        let own = with_fit(Fit::Consigned);
        if !own.is_empty() {
            consigned.push(BomEntry {
                designators: own,
                ..entry.clone()
            });
        }
        let designators = with_fit(Fit::Assembled);
        if designators.is_empty() {
            continue;
        }
        let kept = BomEntry {
            designators,
            ..entry.clone()
        };
        if kept.lcsc.is_none() {
            missing.push((
                kept.designators.clone(),
                kept.value.clone(),
                kept.footprint.clone(),
            ));
            unassembled.extend(kept.designators);
        } else {
            bom.push(kept);
        }
    }
    if !missing.is_empty() && !options.allow_missing_lcsc {
        return Err(Error::MissingLcsc(missing));
    }

    let assembled = board
        .parts
        .iter()
        .filter(|p| {
            bom.iter()
                .chain(&consigned)
                .any(|e| e.designators.contains(&p.designator))
        })
        .cloned()
        .collect();
    let with_fit = |fit: Fit| {
        let mut named: Vec<String> = board
            .parts
            .iter()
            .filter(|p| fit_of(p) == fit)
            .map(|p| p.designator.clone())
            .chain(
                options
                    .fit
                    .iter()
                    .filter(|(_, f)| **f == fit)
                    .map(|(d, _)| d.clone()),
            )
            .collect();
        named.dedup_by(|a, b| a == b);
        named.sort();
        named.dedup();
        sorted(named)
    };
    Ok(Plan {
        bom,
        consigned,
        assembled,
        hand_solder: with_fit(Fit::HandSolder),
        not_fitted: with_fit(Fit::NotFitted),
        unassembled: sorted(unassembled),
    })
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort_by(|a, b| bom::designator_cmp(a, b));
    v
}

/// What the design rules check said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrcOutcome {
    Passed,
    /// Failed, but [`Options::allow_drc_violations`] let packaging go on.
    Waived(DrcCounts),
}

/// The finished package, ready to write.
#[derive(Debug)]
pub struct Package {
    /// The board's file name without `.kicad_pcb`.
    pub name: String,
    pub gerber_zip: Vec<u8>,
    /// File names inside the zip, after the `gerber/` folder.
    pub gerber_files: Vec<String>,
    pub bom_csv: String,
    pub cpl_csv: String,
    pub notes: String,
    pub plan: Plan,
    pub drc: DrcOutcome,
    /// Everything the design rules check found, warnings included.
    pub drc_report: DrcReport,
    /// Each requirement on where a part goes, measured: met or not, with the numbers.
    pub constraints: Vec<ConstraintOutcome>,
    /// Designators whose rotation or side the package could not verify against JLCPCB's
    /// order preview (the "unverified" lines of the notes), sorted.
    pub unverified: Vec<String>,
}

impl Package {
    pub fn zip_name(&self) -> String {
        format!("{}-gerbers.zip", self.name)
    }

    /// Writes the four files into `dir` (created if needed) and returns their paths.
    pub fn write(&self, dir: &Path) -> Result<Vec<PathBuf>, Error> {
        std::fs::create_dir_all(dir)
            .map_err(|e| Error::Io(format!("cannot create {}: {e}", dir.display())))?;
        let files: [(String, &[u8]); 4] = [
            (self.zip_name(), &self.gerber_zip),
            ("bom.csv".to_string(), self.bom_csv.as_bytes()),
            ("cpl.csv".to_string(), self.cpl_csv.as_bytes()),
            ("fab-notes.txt".to_string(), self.notes.as_bytes()),
        ];
        files
            .into_iter()
            .map(|(name, bytes)| {
                let path = dir.join(name);
                std::fs::write(&path, bytes)
                    .map_err(|e| Error::Io(format!("cannot write {}: {e}", path.display())))?;
                Ok(path)
            })
            .collect()
    }

    /// A few lines for a person to read after packaging.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "{}: {} part(s) on {} BOM line(s), {} placement(s), {} file(s) in the zip\n",
            self.name,
            self.plan.assembled.len(),
            self.plan.bom_rows().len(),
            self.plan.assembled.len(),
            self.gerber_files.len()
        );
        for line in self.notes.lines().filter(|l| !l.starts_with("fab-notes")) {
            s.push_str("  ");
            s.push_str(line);
            s.push('\n');
        }
        s
    }
}

/// Checks the board and builds the upload package for it.
///
/// Order: read the board and decide which parts are assembled (errors for part numbers
/// and BOM mismatches come first), run netsmith's design rules check (with the project file
/// beside the board), then write the Gerbers, the drill files and the placement file, all
/// with netsmith's own writers. Errors of the check stop the package unless
/// [`Options::allow_drc_violations`] says to go on. KiCad is not needed.
pub fn package(pcb: &Path, entries: &[BomEntry], options: &Options) -> Result<Package, Error> {
    let src = std::fs::read_to_string(pcb)
        .map_err(|e| Error::Io(format!("cannot read {}: {e}", pcb.display())))?;
    let board = Board::parse(&src)?;
    let name = pcb
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let plan = plan(&board, entries, options)?;

    let mut drc_report = drc_check(&src, drc_project_of(pcb).as_deref(), &options.drc)?;
    // The design's requirements on positions are measured on the same file (ADR 0045).
    let outcomes = match &options.constraints {
        Some(c) => {
            let outcomes = c.measure(&Artwork::parse(&src)?);
            drc_report
                .findings
                .extend(constraint_outcome_findings(&outcomes, c.severity));
            drc_report
                .findings
                .sort_by_key(|f| std::cmp::Reverse(f.severity));
            outcomes
        }
        None => Vec::new(),
    };
    // A requirement of the design is not a design rule: waiving the rules does not waive it.
    let unmet: Vec<String> = drc_report
        .errors()
        .filter(|f| f.rule == DrcRule::Constraint)
        .map(ToString::to_string)
        .collect();
    let drc = if drc_report.is_clean() {
        DrcOutcome::Passed
    } else if options.allow_drc_violations && unmet.is_empty() {
        DrcOutcome::Waived(drc_report.counts())
    } else if options.allow_drc_violations {
        return Err(Error::Constraints {
            count: unmet.len(),
            report: unmet.join("\n"),
        });
    } else {
        return Err(Error::Drc {
            counts: drc_report.counts(),
            report: drc_report
                .errors()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        });
    };

    let art = Artwork::parse(&src)?;
    let plotted = plot_layer(&art, &name, &plot_rules(pcb))?;
    let mut files = plotted.files;
    files.extend(drill_files(&art, &name));
    files.sort_by(|a, b| a.name.cmp(&b.name));
    let gerber_zip = gerber::zip(&files)?;

    let refs: Vec<&BoardPart> = plan.assembled.iter().collect();
    let placed = cpl::build(board.origin, &refs);

    // What the board needs of the fabricator, then what the writers did or left out.
    let extras: Vec<String> = options
        .extras
        .iter()
        .cloned()
        .chain(fill_notes(&art, &plotted.notes))
        .collect();
    let mut unverified: Vec<String> = placed
        .unverified
        .values()
        .flatten()
        .chain(&placed.bottom)
        .cloned()
        .collect();
    unverified.sort_by(|a, b| bom::designator_cmp(a, b));
    unverified.dedup();

    let notes = notes(
        &name,
        &board,
        &plan,
        &placed,
        (drc, &drc_report),
        &outcomes,
        options.process.as_deref(),
        &extras,
        &options.trim,
    );
    Ok(Package {
        gerber_files: files.into_iter().map(|f| f.name).collect(),
        bom_csv: bom::render(&plan.bom_rows()),
        cpl_csv: cpl::render(&placed.rows),
        gerber_zip,
        notes,
        plan,
        name,
        drc,
        drc_report,
        constraints: outcomes,
        unverified,
    })
}

/// The rules the plot needs from the project file beside the board (`kicad_pro`), which
/// say nothing if there is no such file.
pub(crate) fn plot_rules(pcb: &Path) -> PlotRules {
    let pro = std::fs::read_to_string(pcb.with_extension("kicad_pro")).unwrap_or_default();
    plot_rules_of(Some(&pro))
}

/// Lines for the notes about the fill of zones and anything the writers left out.
fn fill_notes(art: &Artwork, plotted: &[String]) -> Vec<String> {
    let mut lines = Vec::new();
    let filled_by_us: Vec<String> = art
        .zones
        .iter()
        .filter(|z| z.keepout.is_none())
        .flat_map(|z| {
            z.layers
                .iter()
                .filter(|l| !z.filled.iter().any(|(fl, _)| fl == *l))
                .map(move |l| format!("{} on {l}", z.net.as_deref().unwrap_or("no net")))
        })
        .collect();
    if !filled_by_us.is_empty() {
        lines.push(format!(
            "zone fill: netsmith filled {} itself (clearance round other nets, thermal reliefs on its own pads; copper islands and slivers under the minimum thickness are not removed); look at the planes and pours in JLCPCB's order preview",
            filled_by_us.join(", ")
        ));
    }
    lines.extend(plotted.iter().cloned());
    lines
}

#[allow(clippy::too_many_arguments)]
fn notes(
    name: &str,
    board: &Board,
    plan: &Plan,
    placed: &cpl::Cpl,
    (drc, report): (DrcOutcome, &DrcReport),
    constraints: &[ConstraintOutcome],
    process: Option<&str>,
    extras: &[String],
    trim: &[String],
) -> String {
    let mut lines = vec![format!("fab-notes for {name}")];
    if let Some(process) = process {
        lines.push(format!("process: {process}"));
    }
    lines.extend(extras.iter().cloned());
    if board.copper_layers.len() > 2 {
        lines.push(format!(
            "copper layers: {} ({}): the zip holds one Gerber for each; check the layer count and order in JLCPCB's order preview",
            board.copper_layers.len(),
            board.copper_layers.join(", ")
        ));
    }
    if !plan.hand_solder.is_empty() {
        lines.push(format!("hand-solder: {}", plan.hand_solder.join(", ")));
    }
    if !plan.consigned.is_empty() {
        let parts: Vec<String> = plan
            .consigned
            .iter()
            .map(|e| {
                format!(
                    "{} ({}, {} part{})",
                    e.designators.join(", "),
                    e.value,
                    e.designators.len(),
                    if e.designators.len() == 1 { "" } else { "s" }
                )
            })
            .collect();
        lines.push(format!(
            "consigned: {} (you ship these parts to JLCPCB with the order; JLCPCB places and solders them from bom.csv and cpl.csv, matches them on the order page, and cannot go on until they are in its warehouse)",
            parts.join("; ")
        ));
    }
    lines.extend(trim.iter().map(|t| format!("trim: {t}")));
    if !plan.not_fitted.is_empty() {
        lines.push(format!(
            "not fitted (no BOM line, no placement): {}",
            plan.not_fitted.join(", ")
        ));
    }
    if !plan.unassembled.is_empty() {
        lines.push(format!(
            "unassembled (no LCSC part number): {}",
            plan.unassembled.join(", ")
        ));
    }
    if !placed.unverified.is_empty() {
        let families: Vec<String> = placed
            .unverified
            .iter()
            .map(|(fp, ds)| format!("{} ({fp})", ds.join(", ")))
            .collect();
        lines.push(format!(
            "rotation unverified: check JLCPCB's order preview: {}",
            families.join("; ")
        ));
    }
    if !placed.bottom.is_empty() {
        lines.push(format!(
            "bottom side unverified (rotation and mirroring): check JLCPCB's order preview: {}",
            placed.bottom.join(", ")
        ));
    }
    lines.push(match board.origin {
        Some((x, y)) => format!("origin: drill/place origin at ({x}, {y}) mm on the board, used by every file"),
        None => "origin: the board sets no drill/place origin; coordinates are relative to the page corner, the same in every file".to_string(),
    });
    lines.push(match drc {
        DrcOutcome::Passed => "design rules check: passed".to_string(),
        DrcOutcome::Waived(c) => format!(
            "design rules check: NOT clean, packaged anyway: {} violation(s), {} unconnected pad(s), {} footprint error(s)",
            c.violations, c.unconnected, c.footprint_errors
        ),
    });
    // A package made past the check lists what it waived, each but a connection left (those are
    // in `routing-failure.txt`), so the person ordering reads what the board breaks.
    if matches!(drc, DrcOutcome::Waived(_)) {
        lines.extend(
            report
                .errors()
                .filter(|e| e.rule != DrcRule::UnconnectedItems)
                .map(|e| format!("  {e}")),
        );
    }
    if !constraints.is_empty() {
        let met = constraints.iter().filter(|c| c.satisfied).count();
        // A package is only made past an unmet one when the project asked for warnings
        // (`constraints = "warn"`); `--allow-drc-violations` never waives one.
        let unmet = constraints.len() - met;
        lines.push(if unmet == 0 {
            format!("constraints: {met} of {} satisfied", constraints.len())
        } else {
            format!(
                "constraints: {met} of {} satisfied; {unmet} not met, listed as warnings because the project or this run asked for `constraints = \"warn\"`; check the board against the case before ordering",
                constraints.len()
            )
        });
    }
    // Warnings do not stop a package, but a person should read them.
    let warnings: Vec<&DrcFinding> = report.warnings().collect();
    if !warnings.is_empty() {
        lines.push(format!("design rules warnings: {}", warnings.len()));
        lines.extend(warnings.iter().take(10).map(|w| format!("  {w}")));
        if warnings.len() > 10 {
            lines.push(format!("  ... and {} more", warnings.len() - 10));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests;
