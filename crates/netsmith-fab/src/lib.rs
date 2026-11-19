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
//! [`viewer`] reads a package back, netsmith's or another tool's, and draws it or sums it up
//! (`netsmith view gerber`, issue #1105), with the reader of [`raster`].
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
//! copper narrower than the zone's minimum thickness, a spoke whose end is not on the fill and
//! a copper island are removed as KiCad removes them, with the zone's island setting
//! (`netsmith_geom::fill` says where it differs), `connect_pads` is read (thermal, solid, none,
//! through-hole only) with the per-pad override, zone priorities are followed, and hatched fills
//! are not supported. A pad's `remove_unused_layers` is approximated: an inner
//! layer keeps the pad if a track of its net ends on the pad or a zone of its net covers it (KiCad
//! asks the connectivity engine). Text uses our font, so the silkscreen's letters differ from
//! KiCad's, `bold` is ignored, and images are not plotted and are named in the notes. Text boxes
//! (`fp_text_box`, `gr_text_box`: the border and the text, wrapped at the words) and curves
//! (`fp_curve`, `gr_curve`: straight pieces of a cubic Bézier) are plotted; a character the
//! font lacks is a box named in the notes (the font has the Latin-1 letters, Greek letters and
//! signs of a board). Without a `.kicad_pro` beside the board the zone fill uses no
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
mod net_routes;
mod net_table;
mod panel;
mod raster;
mod rotation;
mod score;
pub mod viewer;

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
    Reported as KicadDrcReported, compare as compare_kicad_drc,
    compare_with_rules_file as compare_kicad_drc_with_rules_file, drop_dangling_vias_of_clusters,
    in_rules_file as in_kicad_dru, parse as parse_kicad_drc,
};
pub use net_routes::{LONG_EXCESS, LONG_RATIO, MOST_VIAS, NetRoute, NetRoutes, VIAS_LEAST};
pub use net_table::{
    GroupNet, GroupRow, NetRow, NetTable, PairLimit, PairRow, SAMPLE as PAIR_SAMPLE, VIA_PAIR_GAP,
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
    /// Package a board that the check finds errors in, for a person to look at and not to order
    /// (`board fab --for-review`, issue #1189): the errors of the design rules check and the
    /// requirements on where parts go that are not met are waived, the notes and the Gerber zip's
    /// name say "FOR REVIEW, NOT FOR ORDER", and [`Package::write`] writes no `bom.csv` and no
    /// `cpl.csv`. A board that passes is packaged as usual, unless [`Options::waived`] has a
    /// finding. [`package`] still returns the package: the caller ends the run in an error while
    /// the check has errors or a finding was waived.
    pub for_review: bool,
    /// The findings the caller waived for review before the package (`board fab --for-review`,
    /// issue #1212): a part over a keepout's height limit, a part in a keepout, a collision
    /// with a case model, one line each with its rule, the part, the numbers and the place
    /// (`error [too_tall] ...`). With [`Options::for_review`], a package with any is made for
    /// review even when the design rules check passes, and the notes list them under the
    /// `FOR REVIEW` heading with the errors of the check.
    pub waived: Vec<String>,
    /// The pictures of the copper the caller writes beside a package made for review, by file
    /// name (issue #1213), which the `FOR REVIEW` heading of the notes names.
    pub pictures: Vec<String>,
    /// What the design rules check needs that the board file does not say (the process's
    /// mask web and the widest via it fills).
    pub drc: DrcExtras,
    /// The design's requirements on where parts go (ADR 0045), which the board file does not
    /// hold: each is measured on the board and is a finding of the check if it is not met.
    pub constraints: Option<Constraints>,
    /// The board's diff pairs, the positive net first, with the skew each may have in mm, which
    /// the board file does not hold (`netsmith_pcb::Board::diff_pairs_with_skew`): [`check`]
    /// notes each pair whose skew is over its limit (issue #1223, #1234).
    pub pairs: Vec<PairLimit>,
    /// The board's groups of nets matched in length (issue #1234): [`check`] notes each group
    /// whose spread is over its tolerance.
    pub length_groups: Vec<netsmith_core::LengthGroup>,
    /// Package even though some parts have no LCSC part number. Those parts are left out
    /// of the BOM and placement file and listed in the notes as unassembled.
    pub allow_missing_lcsc: bool,
    /// The manufacturing process the board was routed and checked to, as one line for
    /// the notes (see `netsmith_pcb::Board::process_note`). The board file does not carry
    /// it, so the caller that made the board says it.
    pub process: Option<String>,
    /// The project's revision (issue #1134): the first line of the notes says it, `fab-notes
    /// for <name>, revision <revision>`.
    pub revision: Option<String>,
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
    /// The parts, by designator, that are not fitted and that `bom.csv` lists as `DNP` lines
    /// (`.fitted &= false`, issue #1273): their entries are kept, with no LCSC number, and
    /// they are on no placement file. A part that is not fitted and is not named here (a hole,
    /// a fiducial, a footprint KiCad marks `dnp`) leaves the bill as before.
    pub dnp: std::collections::BTreeSet<String>,
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
        /// The requirements of the design on where parts go among them, as measured, for a
        /// caller that reports them as data (issue #816).
        unmet: Vec<ConstraintOutcome>,
    },
    /// Requirements of the design on where parts go (ADR 0045) that the board does not meet.
    /// Unlike a design rule, `allow_drc_violations` does not waive them; only making them
    /// warnings does (`constraints = "warn"`).
    Constraints {
        count: usize,
        /// The errors, one to a line.
        report: String,
        /// The same requirements as measured: each with the value, the window and how far out.
        unmet: Vec<ConstraintOutcome>,
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
                    "add the part numbers, or allow listing them as unassembled (Options::allow_missing_lcsc, or --allow-missing-lcsc of the netsmith-fab example; `netsmith board fab` has no such flag and refuses these parts earlier, naming them: say `.assembly &= AssemblyMode::Hand` for them in the design)"
                )
            }
            Self::Cli(e) => write!(f, "{e}"),
            Self::Artwork(e) => write!(f, "{e}"),
            Self::DrcInput(e) => write!(f, "design rules check: {e}"),
            Self::Drc { counts, report, .. } => write!(
                f,
                "{}\n{} violation(s), {} unconnected pad(s) and {} footprint error(s); fix them, or package the board anyway (Options::allow_drc_violations) or for review (Options::for_review). The errors:\n\n{}",
                drc_stop_line(counts, None),
                counts.violations,
                counts.unconnected,
                counts.footprint_errors,
                report.trim_end()
            ),
            Self::Constraints { count, report, .. } => write!(
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

/// The first line of the message when the design rules check stops packaging (issue #1189): that
/// it did, that no package was written (and where the KiCad board is, when the caller wrote it),
/// and what each of the two flags does.
pub fn drc_stop_line(counts: &DrcCounts, board_in: Option<&Path>) -> String {
    let errors = counts.violations + counts.unconnected + counts.footprint_errors;
    let written = match board_in {
        Some(dir) => format!(" (the KiCad board is in {})", dir.display()),
        None => String::new(),
    };
    format!(
        "design rules check: {errors} error{}: no package written{written}; --allow-drc-violations packages the board anyway and says in fab-notes.txt that it is not clean; --for-review writes the files to look at",
        if errors == 1 { "" } else { "s" }
    )
}

/// What the notes and the files of a package made for review say they are.
pub const FOR_REVIEW: &str = "FOR REVIEW, NOT FOR ORDER";

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
    /// The parts nobody fits that `bom.csv` does not list: holes, fiducials, test points.
    pub not_fitted: Vec<String>,
    /// Grouped BOM rows of the parts that are not fitted and that the BOM lists (`DNP`, issue
    /// #1273): a footprint whose value is chosen on the bench. They are in `bom.csv` with
    /// their comment (`DNP (tune 1pF..10pF)`) and no LCSC number, and in no placement file.
    pub dnp: Vec<BomEntry>,
    /// Parts without an LCSC number that were allowed through.
    pub unassembled: Vec<String>,
}

impl Plan {
    /// The rows of `bom.csv`: the parts from stock and the consigned parts, by first
    /// designator.
    pub fn bom_rows(&self) -> Vec<BomEntry> {
        let mut rows: Vec<BomEntry> = self
            .bom
            .iter()
            .chain(&self.consigned)
            .chain(&self.dnp)
            .cloned()
            .collect();
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
    let mut dnp = Vec::new();
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
        // A part that is not fitted and that the design lists is a `DNP` line, with no
        // number to buy.
        let unfitted: Vec<String> = with_fit(Fit::NotFitted)
            .into_iter()
            .filter(|d| options.dnp.contains(d))
            .collect();
        if !unfitted.is_empty() {
            dnp.push(BomEntry {
                designators: unfitted,
                lcsc: None,
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
    let in_dnp = |d: &String| dnp.iter().any(|e: &BomEntry| e.designators.contains(d));
    let not_fitted = with_fit(Fit::NotFitted)
        .into_iter()
        .filter(|d| !in_dnp(d))
        .collect();
    Ok(Plan {
        bom,
        consigned,
        assembled,
        hand_solder: with_fit(Fit::HandSolder),
        not_fitted,
        dnp,
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
    /// Failed, and [`Options::for_review`] made the package to look at: it is marked
    /// [`FOR_REVIEW`] and has no BOM and no placement file.
    ForReview(DrcCounts),
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
    /// The nets routed long and the nets with the most vias, as the notes list them (issue
    /// #1194).
    pub routes: NetRoutes,
}

impl Package {
    /// Whether this package was made for review and not for order ([`Options::for_review`]).
    pub fn for_review(&self) -> bool {
        matches!(self.drc, DrcOutcome::ForReview(_))
    }

    /// `<board>-gerbers.zip`, and `<board>-gerbers-REVIEW.zip` for a package made for review.
    pub fn zip_name(&self) -> String {
        if self.for_review() {
            format!("{}-gerbers-REVIEW.zip", self.name)
        } else {
            format!("{}-gerbers.zip", self.name)
        }
    }

    /// Writes the files into `dir` (created if needed) and returns their paths: the Gerber zip,
    /// `bom.csv`, `cpl.csv` and `fab-notes.txt`; for a package made for review the zip and the
    /// notes alone, and the files of an earlier package of the board that an order could use
    /// (`bom.csv`, `cpl.csv`, `<board>-gerbers.zip`) are removed, so that the folder holds nothing
    /// that is not marked.
    pub fn write(&self, dir: &Path) -> Result<Vec<PathBuf>, Error> {
        std::fs::create_dir_all(dir)
            .map_err(|e| Error::Io(format!("cannot create {}: {e}", dir.display())))?;
        let mut files: Vec<(String, &[u8])> = vec![(self.zip_name(), &self.gerber_zip)];
        // The other kind of package of this board leaves nothing behind that an order could take.
        let stale = if self.for_review() {
            vec![
                "bom.csv".to_string(),
                "cpl.csv".to_string(),
                format!("{}-gerbers.zip", self.name),
            ]
        } else {
            vec![format!("{}-gerbers-REVIEW.zip", self.name)]
        };
        for stale in stale {
            match std::fs::remove_file(dir.join(&stale)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(Error::Io(format!("cannot remove {stale}: {e}")));
                }
                _ => {}
            }
        }
        if !self.for_review() {
            files.push(("bom.csv".to_string(), self.bom_csv.as_bytes()));
            files.push(("cpl.csv".to_string(), self.cpl_csv.as_bytes()));
        }
        files.push(("fab-notes.txt".to_string(), self.notes.as_bytes()));
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

/// What [`check`] found: the design rules check's findings, the design's requirements on
/// positions among them (each not met is a finding), and each requirement as measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    /// Everything the check found, errors first, warnings included.
    pub report: DrcReport,
    /// Each requirement on where a part goes, measured: met or not, with the numbers.
    pub outcomes: Vec<ConstraintOutcome>,
    /// The nets routed long and the nets with the most vias (issue #1194): notes, not findings.
    pub routes: NetRoutes,
    /// The diff pairs of [`Options::pairs`] whose skew is over their limit (issue #1223): notes,
    /// not findings.
    pub skewed: Vec<PairRow>,
    /// The groups of [`Options::length_groups`] whose spread is over their tolerance (issue
    /// #1234): notes, not findings.
    pub spread: Vec<GroupRow>,
}

/// The design rules check of the board file `src` (the text of a `.kicad_pcb`, with the
/// `.kicad_pro` text in `project`) as [`package`] runs it, and the design's requirements on
/// positions (ADR 0045) measured on the same text. `netsmith board drc` calls it alone.
pub fn check(src: &str, project: Option<&str>, options: &Options) -> Result<Checked, Error> {
    let mut report = drc_check(src, project, &options.drc)?;
    let art = Artwork::parse(src)?;
    let routes = NetRoutes::of(&art);
    let table = NetTable::of(&art, &options.pairs, &options.length_groups);
    let skewed = table.over().cloned().collect();
    let spread = table.groups_over().cloned().collect();
    let outcomes = match &options.constraints {
        Some(c) => {
            let outcomes = c.measure(&art);
            report
                .findings
                .extend(constraint_outcome_findings(&outcomes, c.severity));
            report
                .findings
                .sort_by_key(|f| std::cmp::Reverse(f.severity));
            outcomes
        }
        None => Vec::new(),
    };
    Ok(Checked {
        report,
        outcomes,
        routes,
        skewed,
        spread,
    })
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

    let Checked {
        report: drc_report,
        outcomes,
        routes,
        skewed: _,
        spread: _,
    } = check(&src, drc_project_of(pcb).as_deref(), options)?;
    // A requirement of the design is not a design rule: waiving the rules does not waive it.
    let unmet: Vec<String> = drc_report
        .errors()
        .filter(|f| f.rule == DrcRule::Constraint)
        .map(ToString::to_string)
        .collect();
    // The same as measured, for the callers that say them as data: the ones the check found
    // errors (`constraint_outcome_findings` makes a finding of each not met, of the severity
    // the caller chose).
    let unmet_outcomes: Vec<ConstraintOutcome> = match &options.constraints {
        Some(c) if c.severity == netsmith_geom::DrcSeverity::Error => {
            outcomes.iter().filter(|o| !o.satisfied).cloned().collect()
        }
        _ => Vec::new(),
    };
    // A package for review is one whatever the check says when the caller waived a finding
    // before it (issue #1212): such a board is never packaged to order from.
    let drc = if options.for_review && (!drc_report.is_clean() || !options.waived.is_empty()) {
        DrcOutcome::ForReview(drc_report.counts())
    } else if drc_report.is_clean() {
        DrcOutcome::Passed
    } else if options.allow_drc_violations && unmet.is_empty() {
        DrcOutcome::Waived(drc_report.counts())
    } else if options.allow_drc_violations {
        return Err(Error::Constraints {
            count: unmet.len(),
            report: unmet.join("\n"),
            unmet: unmet_outcomes,
        });
    } else {
        return Err(Error::Drc {
            counts: drc_report.counts(),
            report: drc_report
                .errors()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            unmet: unmet_outcomes,
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
        .chain(routes.lines())
        .collect();
    let mut unverified: Vec<String> = placed
        .unverified
        .values()
        .flatten()
        .chain(&placed.bottom)
        .chain(placed.turned.iter().map(|(d, _)| d))
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
        (options.process.as_deref(), options.revision.as_deref()),
        &extras,
        &options.trim,
        (&options.waived, &options.pictures),
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
        routes,
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
            "zone fill: netsmith filled {} itself (clearance round other nets, thermal reliefs on its own pads; copper under the minimum thickness, spokes that reach nothing and copper islands removed); look at the planes and pours in JLCPCB's order preview",
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
    (process, revision): (Option<&str>, Option<&str>),
    extras: &[String],
    trim: &[String],
    (waived, pictures): (&[String], &[String]),
) -> String {
    let mut lines = vec![match revision {
        Some(revision) => format!("fab-notes for {name}, revision {revision}"),
        None => format!("fab-notes for {name}"),
    }];
    if matches!(drc, DrcOutcome::ForReview(_)) {
        lines.extend(review_heading(report, waived, pictures));
    }
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
    if !plan.dnp.is_empty() {
        let parts: Vec<String> = plan
            .dnp
            .iter()
            .map(|e| {
                let value = e.value.strip_prefix("DNP").unwrap_or(&e.value).trim();
                format!("{} {value}", e.designators.join(", "))
            })
            .collect();
        lines.push(format!(
            "DNP (a DNP line of bom.csv with no LCSC number, no placement; the footprint is on the board, and the value is chosen on the bench): {}; check on JLCPCB's order page that none of them is placed",
            parts.join("; ")
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
    if !placed.turned.is_empty() {
        let turns: Vec<String> = placed
            .turned
            .iter()
            .map(|(d, turn)| format!("{d} ({} degrees)", cpl::number(*turn)))
            .collect();
        lines.push(format!(
            "turn off the right angles unverified (JLCPCB's correction for such a turn is not measured; cpl.csv has KiCad's turn plus the footprint's offset): check JLCPCB's order preview: {}",
            turns.join(", ")
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
        DrcOutcome::ForReview(c) if c == DrcCounts::default() => format!(
            "design rules check: no errors, and the package is {FOR_REVIEW} for the findings under FOR REVIEW above"
        ),
        DrcOutcome::ForReview(c) => format!(
            "design rules check: NOT clean, {FOR_REVIEW}: {} violation(s), {} unconnected pad(s), {} footprint error(s), each listed under FOR REVIEW above",
            c.violations, c.unconnected, c.footprint_errors
        ),
    });
    // A package made past the check lists what it waived, each but a connection left (those are
    // in `routing-failure.txt`), so the person ordering reads what the board breaks. A package for
    // review lists them at the top instead.
    if matches!(drc, DrcOutcome::Waived(_)) {
        lines.extend(
            report
                .errors()
                .filter(|e| e.rule != DrcRule::UnconnectedItems)
                .map(|e| format!("  {e}")),
        );
    }
    // A finding of a part's own footprint that the layout file waives (issue #1196) is neither an
    // error nor a warning, and every one is listed, with its reason and source, for a reviewer.
    if !report.waived.is_empty() {
        lines.push(format!(
            "design rules waived: {} finding(s) of a part's own footprint, by the layout file; check each against its source",
            report.waived.len()
        ));
        lines.extend(report.waived.iter().map(|w| format!("  {w}")));
    }
    if !constraints.is_empty() {
        let met = constraints.iter().filter(|c| c.satisfied).count();
        // A package is only made past an unmet one when the project asked for warnings
        // (`constraints = "warn"`); `--allow-drc-violations` never waives one.
        let unmet = constraints.len() - met;
        lines.push(if unmet == 0 {
            format!("constraints: {met} of {} satisfied", constraints.len())
        } else if matches!(drc, DrcOutcome::ForReview(_)) {
            format!(
                "constraints: {met} of {} satisfied; {unmet} not met (each listed under FOR REVIEW above): {FOR_REVIEW}",
                constraints.len()
            )
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

/// The `FOR REVIEW` heading of the notes of a package made for review (issues #1189, #1212,
/// #1213): what the package is and is not, the pictures written beside it, and every finding
/// `--for-review` waived, one to a line: those the caller waived before the package (`waived`),
/// then the errors of the design rules check, the requirements on where parts go among them.
fn review_heading(report: &DrcReport, waived: &[String], pictures: &[String]) -> Vec<String> {
    let found: Vec<String> = waived
        .iter()
        .cloned()
        .chain(report.errors().map(ToString::to_string))
        .collect();
    let pictures = if pictures.is_empty() {
        String::new()
    } else {
        format!(
            "; the pictures of the copper, each layer as `view pcb --layer <layer> --drc --islands` draws it: {}",
            pictures.join(", ")
        )
    };
    let mut lines = vec![format!(
        "{FOR_REVIEW}: `--for-review` waived {} finding{}, each listed below, so this package is made to look at, not to order; it has no bom.csv and no cpl.csv, and `project order` refuses it{pictures}",
        found.len(),
        if found.len() == 1 { "" } else { "s" }
    )];
    lines.extend(found.into_iter().map(|f| format!("  {f}")));
    lines
}

#[cfg(test)]
mod tests;
