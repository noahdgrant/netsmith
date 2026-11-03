//! netsmith's own design rules check (ADR 0037): the gate in front of packaging, and the
//! findings the editor lists, over the board file the writers read. KiCad is not needed;
//! its check ([`kicad`]) is an independent witness that the cross-check compares with.
//!
//! The check reads the same [`Artwork`] the Gerber writers draw from, and the project file
//! beside the board for the numbers (`min_clearance`, `min_track_width`, net classes...), so a
//! board ported from KiCad is checked as KiCad would, and a board netsmith wrote is checked
//! to the manufacturing process it was routed for. A finding is data: a rule (named as KiCad
//! names it, so the two reports compare), a severity, a message with the numbers, a place
//! and the items involved.
//!
//! Zone fills are not computed here. The fill netsmith makes keeps the clearance, hole
//! clearance and edge clearance of its zone by construction (the fill is "everything but
//! the clearances"), so the copper of a pour is never checked against other copper; a fill
//! the file already holds is not checked either. A pour connects the pads, vias and tracks
//! of its net that lie inside its outline.

mod checks;
mod connect;
pub mod constraints;
pub mod kicad;
mod model;
mod rules;
pub mod shape;

use crate::artwork::Artwork;
use crate::geom::Pt;
pub use rules::{Class, Rules};

/// How bad a finding is. A rule the project file says to ignore produces none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
    /// Only in a project file: the person chose not to check this rule.
    Ignore,
}

impl Severity {
    pub fn parse(s: &str) -> Option<Severity> {
        match s {
            "error" => Some(Severity::Error),
            "warning" => Some(Severity::Warning),
            "ignore" => Some(Severity::Ignore),
            _ => None,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Ignore => "ignore",
        }
    }
}

/// A rule of the check. The ids are KiCad's names for the same rule, so that the two
/// checks' reports can be compared; [`Rule::HoleToEdge`] and [`Rule::ViaInPad`] are
/// netsmith's own, which KiCad has no rule for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    /// Copper of two nets nearer than the clearance: track, pad and via against each other.
    Clearance,
    /// Copper of two nets that touch.
    ShortingItems,
    /// Two tracks of different nets that cross.
    TracksCrossing,
    /// Pads, vias and tracks of one net that no copper joins.
    UnconnectedItems,
    /// A hole nearer than the hole clearance to copper of another net.
    HoleClearance,
    /// Two holes nearer than the least space between holes.
    HoleToHole,
    /// A drilled hole smaller than the process drills.
    DrillOutOfRange,
    /// A via or plated hole with less copper round it than the process leaves.
    AnnularWidth,
    TrackWidth,
    ViaDiameter,
    /// Copper nearer the board edge than the process allows.
    CopperEdgeClearance,
    /// A hole nearer the board edge than the process allows (netsmith's: KiCad has no rule
    /// for a hole with no copper).
    HoleToEdge,
    CourtyardsOverlap,
    MalformedCourtyard,
    /// Silkscreen on a pad's mask opening, where the fabricator clips it.
    SilkOverCopper,
    /// Silkscreen of two parts nearer than the silkscreen clearance.
    SilkOverlap,
    SilkEdgeClearance,
    TextHeight,
    TextThickness,
    /// Two pads of different nets whose mask openings are nearer than the mask web the
    /// process can make.
    SolderMaskBridge,
    /// A track, via, pad or part inside a rule area that forbids it.
    ItemsNotAllowed,
    TrackDangling,
    ViaDangling,
    /// The board outline is not one closed path, or a cutout is not inside it.
    InvalidOutline,
    /// A via sunk into a pad, which the process fills only up to a diameter (netsmith's).
    ViaInPad,
    /// A requirement of the design on where a part goes that the board does not meet
    /// (ADR 0045; netsmith's, and not in the board file: see [`constraints`]).
    Constraint,
}

impl Rule {
    pub const ALL: [Rule; 26] = [
        Rule::Clearance,
        Rule::ShortingItems,
        Rule::TracksCrossing,
        Rule::UnconnectedItems,
        Rule::HoleClearance,
        Rule::HoleToHole,
        Rule::DrillOutOfRange,
        Rule::AnnularWidth,
        Rule::TrackWidth,
        Rule::ViaDiameter,
        Rule::CopperEdgeClearance,
        Rule::HoleToEdge,
        Rule::CourtyardsOverlap,
        Rule::MalformedCourtyard,
        Rule::SilkOverCopper,
        Rule::SilkOverlap,
        Rule::SilkEdgeClearance,
        Rule::TextHeight,
        Rule::TextThickness,
        Rule::SolderMaskBridge,
        Rule::ItemsNotAllowed,
        Rule::TrackDangling,
        Rule::ViaDangling,
        Rule::InvalidOutline,
        Rule::ViaInPad,
        Rule::Constraint,
    ];

    /// KiCad's name for the rule (its `type` in a JSON report).
    pub fn id(self) -> &'static str {
        match self {
            Rule::Clearance => "clearance",
            Rule::ShortingItems => "shorting_items",
            Rule::TracksCrossing => "tracks_crossing",
            Rule::UnconnectedItems => "unconnected_items",
            Rule::HoleClearance => "hole_clearance",
            Rule::HoleToHole => "hole_to_hole",
            Rule::DrillOutOfRange => "drill_out_of_range",
            Rule::AnnularWidth => "annular_width",
            Rule::TrackWidth => "track_width",
            Rule::ViaDiameter => "via_diameter",
            Rule::CopperEdgeClearance => "copper_edge_clearance",
            Rule::HoleToEdge => "hole_to_edge",
            Rule::CourtyardsOverlap => "courtyards_overlap",
            Rule::MalformedCourtyard => "malformed_courtyard",
            Rule::SilkOverCopper => "silk_over_copper",
            Rule::SilkOverlap => "silk_overlap",
            Rule::SilkEdgeClearance => "silk_edge_clearance",
            Rule::TextHeight => "text_height",
            Rule::TextThickness => "text_thickness",
            Rule::SolderMaskBridge => "solder_mask_bridge",
            Rule::ItemsNotAllowed => "items_not_allowed",
            Rule::TrackDangling => "track_dangling",
            Rule::ViaDangling => "via_dangling",
            Rule::InvalidOutline => "invalid_outline",
            Rule::ViaInPad => "via_in_pad",
            Rule::Constraint => "constraint",
        }
    }

    pub fn from_id(id: &str) -> Option<Rule> {
        Rule::ALL.into_iter().find(|r| r.id() == id)
    }

    /// How a broken rule is reported when the project file does not say. KiCad's own
    /// defaults, except that what only the silkscreen suffers is a warning.
    pub fn default_severity(self) -> Option<Severity> {
        Some(match self {
            Rule::SilkOverCopper
            | Rule::SilkOverlap
            | Rule::SilkEdgeClearance
            | Rule::TextHeight
            | Rule::TextThickness
            | Rule::TrackDangling
            | Rule::ViaDangling
            | Rule::ViaInPad => Severity::Warning,
            _ => Severity::Error,
        })
    }
}

/// One of the things a finding is about.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// What it is, as a person would say it: `Pad 1 [VIN] of C1 on F.Cu`.
    pub what: String,
    /// Where KiCad would say it is (a pad's or via's centre, a track's start, a part's
    /// origin).
    pub at: Pt,
    /// The part it belongs to, and the pad.
    pub part: Option<String>,
    pub pad: Option<String>,
    pub net: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub rule: Rule,
    pub severity: Severity,
    /// What is wrong, with the numbers: `clearance 0.100 mm is less than the 0.127 mm...`.
    pub message: String,
    /// Where the problem is, in the board's frame (mm, y down).
    pub at: Pt,
    pub layer: Option<String>,
    pub items: Vec<Item>,
}

impl Finding {
    /// The parts the finding names, in order, once each.
    pub fn parts(&self) -> Vec<String> {
        let mut parts: Vec<String> = Vec::new();
        for p in self.items.iter().filter_map(|i| i.part.clone()) {
            if !parts.contains(&p) {
                parts.push(p);
            }
        }
        parts
    }
}

impl Finding {
    /// The message and, after it, the items the finding is about.
    pub fn with_items(&self) -> String {
        if self.items.is_empty() {
            return self.message.clone();
        }
        let items: Vec<&str> = self.items.iter().map(|i| i.what.as_str()).collect();
        format!("{}: {}", self.message, items.join("; "))
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} [{}] {} at ({:.3}, {:.3})",
            self.severity.word(),
            self.rule.id(),
            self.message,
            self.at.x,
            self.at.y
        )?;
        if let Some(l) = &self.layer {
            write!(f, " on {l}")?;
        }
        if !self.items.is_empty() {
            let items: Vec<&str> = self.items.iter().map(|i| i.what.as_str()).collect();
            write!(f, ": {}", items.join("; "))?;
        }
        Ok(())
    }
}

/// The counts a package's notes give: the wording of KiCad's report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Errors that are not a missing connection or a malformed footprint.
    pub violations: u32,
    /// Connections that are missing.
    pub unconnected: u32,
    /// Footprints with a courtyard that is not a closed shape.
    pub footprint_errors: u32,
}

impl Counts {
    pub fn total(self) -> u32 {
        self.violations + self.unconnected + self.footprint_errors
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    /// Errors first, then warnings, each in the order the rules ran.
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn errors(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
    }

    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for f in self.errors() {
            match f.rule {
                Rule::UnconnectedItems => c.unconnected += 1,
                Rule::MalformedCourtyard => c.footprint_errors += 1,
                _ => c.violations += 1,
            }
        }
        c
    }

    pub fn is_clean(&self) -> bool {
        self.errors().next().is_none()
    }

    /// How many findings of a rule there are, of any severity.
    pub fn count(&self, rule: Rule) -> usize {
        self.findings.iter().filter(|f| f.rule == rule).count()
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for finding in &self.findings {
            writeln!(f, "{finding}")?;
        }
        Ok(())
    }
}

/// What the check needs to know that the board file and its project do not say. All of it
/// comes from the manufacturing process the board was made for.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Extras {
    /// The least solder mask between two openings of pads of different nets, if the board's
    /// setup does not say (`solder_mask_min_width`).
    pub mask_web: Option<f64>,
    /// The widest via the process fills and caps, so that it may sit in a pad; a wider one
    /// in a pad is a warning. `None` is not checked.
    pub via_in_pad: Option<f64>,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    Board(crate::artwork::ArtworkError),
    Project(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Board(e) => write!(f, "{e}"),
            Error::Project(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<crate::artwork::ArtworkError> for Error {
    fn from(e: crate::artwork::ArtworkError) -> Self {
        Error::Board(e)
    }
}

/// Checks the board in `pcb` (the text of a `.kicad_pcb`) to the rules in `project` (the
/// text of the `.kicad_pro` beside it; KiCad's defaults without one).
pub fn check(pcb: &str, project: Option<&str>, extras: &Extras) -> Result<Report, Error> {
    let art = Artwork::parse(pcb)?;
    let mut rules = match project {
        Some(p) => Rules::from_project(p).map_err(Error::Project)?,
        None => Rules::default(),
    };
    // The board's own setup states the mask web the process can make; the caller may too.
    let setup_web = crate::sexp::parse(pcb)
        .ok()
        .and_then(|root| {
            root.child("setup")?
                .child("solder_mask_min_width")?
                .items()
                .get(1)?
                .text()?
                .parse::<f64>()
                .ok()
        })
        .or(extras.mask_web);
    rules.mask_web = setup_web.unwrap_or(0.0);
    let bridges_in_footprints = crate::sexp::parse(pcb)
        .ok()
        .and_then(|root| {
            root.child("setup")?
                .child("allow_soldermask_bridges_in_footprints")?
                .items()
                .get(1)?
                .text()
                .map(|t| t == "yes")
        })
        .unwrap_or(false);
    Ok(check_artwork(&art, &rules, extras, bridges_in_footprints))
}

/// Checks a board already read, to rules already known.
pub fn check_artwork(
    art: &Artwork,
    rules: &Rules,
    extras: &Extras,
    bridges_in_footprints: bool,
) -> Report {
    let model = model::Model::new(art, rules, extras, bridges_in_footprints);
    let mut findings = Vec::new();
    checks::run(&model, &mut findings);
    connect::run(&model, &mut findings);
    // Errors before warnings, otherwise in the order found.
    findings.sort_by_key(|f| std::cmp::Reverse(f.severity));
    Report { findings }
}

/// The `.kicad_pro` beside a board file, if there is one.
pub fn project_of(pcb: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(pcb.with_extension("kicad_pro")).ok()
}
