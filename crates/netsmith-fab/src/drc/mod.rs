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
//!
//! # Why the check is here, what it reads, and what it was checked against (ADR 0037)
//!
//! The owner accepted these choices on 2026-11-04. ADR 0034 made the fabrication files netsmith's
//! own and left the check to `kicad-cli`: without KiCad `fab` refused to package, or packaged with
//! a note that nothing had been checked. This check is now the gate in `fab`, `fab --order` and
//! the editor's Fab button. KiCad's check stays as a witness (`fab --cross-check`, the tests) and
//! as a second opinion in the editor (the button "Also ask KiCad", through `Project.kicad`).
//!
//! **Where it lives: here, over the `.kicad_pcb` text and the `.kicad_pro` beside it, and not in
//! `netsmith-pcb` over its `Board`.** Three reasons. The package, the Gerbers and the
//! cross-check already work from the file, through one reader ([`crate::artwork`]), so the check
//! sees exactly what the board house will make, and a board ported from KiCad is checked like one
//! netsmith wrote. [`crate::package`] calls the check directly, and a separate crate would have to
//! sit above this one (it needs `artwork`, the font and the outline), so `package` could not call
//! it. And KiCad reads the same two files, so the two checks can be compared on identical input.
//! The editor writes the file in memory and checks that. The `Board` model's own
//! `check_placement` (parts, courtyards as boxes, before anything is routed) stays and is not
//! replaced.
//!
//! **The numbers are the project's:** `min_clearance`, `min_track_width`, `min_via_diameter`,
//! `min_via_annular_width`, `min_through_hole_diameter`, `min_hole_to_hole`, `min_hole_clearance`,
//! `min_copper_edge_clearance`, `min_silk_clearance`, the text sizes, each net class's clearance
//! with its net patterns and priority, and each rule's severity. A number the file lacks is KiCad
//! 9's default. The board file's `solder_mask_min_width` is the least mask web (the exporter
//! writes the process's `mask_bridge` there, so KiCad reads it too), and `fab` also passes the
//! process's `via_in_pad` as an extra, because no KiCad file holds it. The project's
//! `rule_severities` decide what stops `fab`: the exporter writes `silk_over_copper`,
//! `silk_overlap`, `silk_edge_clearance` and `solder_mask_bridge` as warnings (a fabricator clips
//! silkscreen over a mask opening and keeps a web only where it can; neither stops a board being
//! made) and everything else as errors. The owner confirmed on 2026-11-04 that those four stay
//! warnings.
//!
//! **Findings are data.** [`check`] returns a `Report` of findings: the rule (named as KiCad
//! names it), the severity, a message with the numbers (`clearance is 0.1000 mm between SMD pad 1
//! [A] of S1 and Via [E]...; the least is 0.1270 mm`), the place in mm and the layer, and the items
//! (what they are, where KiCad would say they are, the part and pad, the net). The package's notes
//! say `design rules check: passed` or `... NOT clean, packaged anyway: N violation(s), M
//! unconnected pad(s), K footprint error(s)` and add `design rules warnings: N`. The editor's
//! Findings tab lists the findings of a routed board with source `drc`; it leaves out the missing
//! connections (the ratsnest's, listed as `unrouted`) and overlapping courtyards (the placement
//! check's). `--allow-drc-violations` lets a package through with errors, and never waives a
//! placement requirement: a `constraint` finding stops `fab` unless `constraints = "warn"` or
//! `--constraints warn` says otherwise (ADR 0045 section 8).
//!
//! **Rules that are not KiCad's** (the cross-check counts them and does not compare them):
//! `hole_to_edge` (a hole with no copper against the edge, by the same least as copper; KiCad has
//! no rule), `via_in_pad` (a via in a pad wider than the process fills), `constraint` (a
//! requirement on where a part is, in [`constraints`]; it needs the design, so `fab` and the
//! editor add it to the findings, as an error or, with `constraints = "warn"`, a warning), and,
//! from the placement check and shown in the editor, `too_tall` (a part with a stated `.height`
//! under a lower height limit of a keepout, ADR 0045 section 11) and `height_unchecked` (a part
//! with no `.height` under a height limit, a warning unless constraints are strict). The other
//! rules are KiCad's by name: clearance, `shorting_items`, `tracks_crossing`, `unconnected_items`
//! (the pieces of each net joined by the shortest connections between them),
//! `track_width`, `via_diameter`, `annular_width`, `drill_out_of_range`, `hole_to_hole`,
//! `hole_clearance`, `copper_edge_clearance`, `courtyards_overlap`, `malformed_courtyard`,
//! `silk_over_copper`, `silk_overlap`, `silk_edge_clearance`, `text_height`, `text_thickness`,
//! `solder_mask_bridge`, `items_not_allowed` (a part's own rule area does not forbid the part),
//! `track_dangling`, `via_dangling` and `invalid_outline`. Geometry is exact for what a board is
//! made of: every shape is a path grown by a radius (a track, a round pad, an oval, a rounded
//! rectangle's polygon) or a solid polygon (custom pads are their anchor and primitives), the gap
//! between two shapes is the least distance between their paths less both radii, and a uniform
//! grid finds the pairs that can be near. A measurement may fall short of its limit by half a
//! micrometre and pass, as in KiCad.
//!
//! **What KiCad taught by comparison, now mirrored.** A pad's angle in the file is absolute and a
//! missing angle is 0 (`artwork` took the footprint's turn; this changed the Gerbers of a board
//! that omits it, as KiCad's do). Two tracks of different nets that cross are `tracks_crossing`,
//! not a short. KiCad reports each layer of a via's clearance but a pair of pads once, as does
//! netsmith. A zero-length missing connection is not drawn, and an open courtyard is reported
//! twice (not closed, self-intersecting). Text is compared with any silkscreen but two drawings
//! only if they do not belong to the same part or both to the board. Mask openings of pads with no
//! net count against pads with one, and openings that only touch are not a bridge when the process
//! asks for no web. A track's place is its start and an arc's its centre. A missing connection's
//! ends are the nearest anchors, and where several are equally near KiCad takes the one on a
//! shared layer, then a track, then the longer (the two checks still pick differently at times,
//! and the comparison counts a missing connection that names other ends as agreeing).
//!
//! **The pour.** A pour joins every pad, via and track of its net that touches its outline on its
//! layer; a zone or a pad set to no connection does not join the pad. A pad the real fill would cut
//! off (a spoke into a keepout, an island) is not seen: that is KiCad's `isolated_copper`,
//! `starved_thermal` and `copper_sliver`, which netsmith does not check, so the gate can pass a
//! board KiCad flags in those categories. A library footprint's rule areas are not enforced, as
//! KiCad does not enforce them: only the board's own are (a keepout a part carries in the source is
//! the board's, ADR 0029). A footprint can state `solder_mask_margin`, `solder_paste_margin`,
//! `solder_paste_ratio`, `clearance` and `zone_connect` for all its pads; they are read, and a
//! pad's own wins. A courtyard drawn with lines of no length is closed by the other lines.
//!
//! **What was checked against KiCad's check.** The cross-check compares findings by rule and by the
//! places of the items (0.1 mm), in both directions, and fails on a finding either side lacks. A
//! rule only netsmith has and a kind of finding netsmith does not check (`OUT_OF_SCOPE` in
//! [`kicad`]: library mismatches, slivers, starved thermals, isolated copper) are counted and not
//! compared. A corpus of 28 broken boards (`tests/drc_corpus.rs`, generated by `tests/common`):
//! 127 findings agree, none differ. The synthetic feature board (every pad shape, turned
//! footprints, slots, arcs, planes, a pour): 20 findings, all agree. The tests of `netsmith-pcb`
//! that ran KiCad's check (`kicad_drc.rs`: the examples routed and unrouted, 33 hard boards, the
//! 70-part synthetic board, land grids with vias in pads, holes, custom pads, keepouts, cutouts,
//! rounded outlines, parts on both faces, wide tracks, four-layer planes) now run this one on the
//! same files: 37 boards, 163 findings agree, none differ (the 193 findings of KiCad's that are
//! left out are about its library tables, because the tests' footprints are not KiCad's). Random
//! boards (`netsmith-pcb`'s `tests/drc.rs`): 100 seeded boards routed by netsmith have no error and
//! the same number of missing connections as the ratsnest; a board of 70 parts, 711 tracks and 161
//! vias is checked in 0.2 s in a debug build. A copy of a real project's board routed by netsmith
//! (nothing of it is in the repository, so no figures from it are kept): no error, and KiCad agrees
//! on the warnings except three (two silkscreen texts that our font puts a hair nearer a pad, and
//! a mask opening drawn as a graphic on `F.Mask`, which is not checked).
//!
//! **What is not checked.** Impedance and controlled-impedance rules, acid traps and copper
//! slivers, islands and starved thermals of a pour, a pour's fill, differential pairs and
//! lengths, custom rules (`.kicad_dru`), blind, buried and micro vias, stackup rules,
//! unrouted-by-design nets, footprint-against-library and schematic parity (KiCad's
//! `lib_footprint_*`, `extra_footprint`), and the electrical rules check (the solver does most of
//! it). Silkscreen text uses netsmith's own font, so a text that only just clears a pad can be
//! judged differently. JLCPCB's own check has not been run on a package: the cross-check says
//! netsmith agrees with KiCad, not that a fabricator will accept the board.

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
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Extras {
    /// The least solder mask between two openings of pads of different nets, if the board's
    /// setup does not say (`solder_mask_min_width`).
    pub mask_web: Option<f64>,
    /// The widest via the process fills and caps, so that it may sit in a pad; a wider one
    /// in a pad is a warning. `None` is not checked.
    pub via_in_pad: Option<f64>,
    /// Pairs of parts, by designator, whose courtyards may overlap because one stands over
    /// the other's pins, which the design cuts short after soldering and which the rule of
    /// `netsmith_pcb::protrusion` excuses (ADR 0045 section 15). The check does not judge that
    /// rule; the caller that has the design does, and says which pairs it excused.
    pub excused_courtyards: Vec<(String, String)>,
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
