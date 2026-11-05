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
//! The board is read from its file with a reader of our own, so nothing here depends on the
//! other netsmith crates.

pub mod artwork;
pub mod board;
pub mod bom;
pub mod cpl;
pub mod crosscheck;
mod csv;
pub mod drc;
pub mod drill;
pub mod edge;
pub mod exclusive;
pub mod fill;
pub mod font;
pub mod gbr;
pub mod geom;
pub mod gerber;
pub mod kicad;
pub mod pads;
pub mod panel;
pub mod plot;
pub mod raster;
pub mod rotation;
pub mod sexp;

pub use board::{Board, BoardPart, Fit, Side};
pub use bom::BomEntry;
pub use kicad::{CliError, KicadCli, KicadCliExe, Output};

use std::path::{Path, PathBuf};

/// Choices that relax a check, which are off by default, and what the board was made to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Options {
    /// Package a board even though netsmith's design rules check reports errors (a
    /// violation, an unconnected pad or a footprint error). The notes say so.
    pub allow_drc_violations: bool,
    /// What the design rules check needs that the board file does not say (the process's
    /// mask web and the widest via it fills).
    pub drc: drc::Extras,
    /// The design's requirements on where parts go (ADR 0045), which the board file does not
    /// hold: each is measured on the board and is a finding of the check if it is not met.
    pub constraints: Option<drc::constraints::Constraints>,
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
    /// (ADR 0045 section 14).
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
    Artwork(artwork::ArtworkError),
    /// The design rules check cannot read the board or its project file.
    DrcInput(drc::Error),
    /// The design rules check found errors and the caller did not allow going on.
    Drc {
        counts: drc::Counts,
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
    Gerber(gerber::GerberError),
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
                    "add the part numbers, or allow listing them as unassembled (Options::allow_missing_lcsc, or --allow-missing-lcsc of the netsmith-fab example; `netsmith fab` has no such flag and refuses these parts earlier, naming them: say `.assembly &= \"hand\"` for them in the design)"
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
impl From<artwork::ArtworkError> for Error {
    fn from(e: artwork::ArtworkError) -> Self {
        Self::Artwork(e)
    }
}
impl From<drc::Error> for Error {
    fn from(e: drc::Error) -> Self {
        Self::DrcInput(e)
    }
}
impl From<CliError> for Error {
    fn from(e: CliError) -> Self {
        Self::Cli(e)
    }
}
impl From<gerber::GerberError> for Error {
    fn from(e: gerber::GerberError) -> Self {
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
    /// Grouped BOM rows of the parts to assemble, every one with an LCSC number.
    pub bom: Vec<BomEntry>,
    /// The board's parts to assemble, in the BOM.
    pub assembled: Vec<BoardPart>,
    pub hand_solder: Vec<String>,
    pub not_fitted: Vec<String>,
    /// Parts without an LCSC number that were allowed through.
    pub unassembled: Vec<String>,
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
        .filter(|p| fit_of(p) == Fit::Assembled && !listed(&p.designator))
        .map(|p| p.designator.clone())
        .collect();
    if !unlisted.is_empty() {
        return Err(Error::NotInBom(sorted(unlisted)));
    }

    // Keep only the parts that are surface mount and populated, in the grouped rows.
    let mut bom = Vec::new();
    let mut missing = Vec::new();
    let mut unassembled = Vec::new();
    for entry in &grouped {
        let designators: Vec<String> = entry
            .designators
            .iter()
            .filter(|d| find(d).is_some_and(|p| fit_of(p) == Fit::Assembled))
            .cloned()
            .collect();
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
        .filter(|p| bom.iter().any(|e| e.designators.contains(&p.designator)))
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
    Waived(drc::Counts),
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
    pub drc_report: drc::Report,
    /// Each requirement on where a part goes, measured: met or not, with the numbers.
    pub constraints: Vec<drc::constraints::Outcome>,
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
            self.plan.bom.len(),
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

    let mut drc_report = drc::check(&src, drc::project_of(pcb).as_deref(), &options.drc)?;
    // The design's requirements on positions are measured on the same file (ADR 0045).
    let outcomes = match &options.constraints {
        Some(c) => {
            let outcomes = c.measure(&artwork::Artwork::parse(&src)?);
            drc_report
                .findings
                .extend(drc::constraints::outcome_findings(&outcomes, c.severity));
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
        .filter(|f| f.rule == drc::Rule::Constraint)
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

    let art = artwork::Artwork::parse(&src)?;
    let plotted = plot::plot(&art, &name, &plot_rules(pcb))?;
    let mut files = plotted.files;
    files.extend(drill::files(&art, &name));
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
        bom_csv: bom::render(&plan.bom),
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
pub(crate) fn plot_rules(pcb: &Path) -> plot::Rules {
    let pro = std::fs::read_to_string(pcb.with_extension("kicad_pro")).unwrap_or_default();
    plot_rules_of(Some(&pro))
}

/// [`plot_rules`], from the text of the project file.
pub(crate) fn plot_rules_of(pro: Option<&str>) -> plot::Rules {
    let pro = pro.unwrap_or_default();
    let number = |key: &str| -> Option<f64> {
        let at = pro.find(&format!("\"{key}\""))?;
        let rest = pro[at..].split_once(':')?.1;
        let end = rest.find([',', '}', '\n'])?;
        rest[..end].trim().parse().ok()
    };
    plot::Rules {
        edge_clearance: number("min_copper_edge_clearance").unwrap_or(0.0),
        hole_clearance: number("min_hole_clearance"),
    }
}

/// Lines for the notes about the fill of zones and anything the writers left out.
fn fill_notes(art: &artwork::Artwork, plotted: &[String]) -> Vec<String> {
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
    (drc, report): (DrcOutcome, &drc::Report),
    constraints: &[drc::constraints::Outcome],
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
    let warnings: Vec<&drc::Finding> = report.warnings().collect();
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
