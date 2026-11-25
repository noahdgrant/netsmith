//! What a command found, as data for an agent and as a short summary for a person (#455).
//!
//! A finding has a severity (`error`, `warning` or `note`), a code, the board and the member
//! path it is about when it is about one, the place in the source when it has one, and the
//! message the command line prints. The code is what the build knows about the finding (the
//! owner's decision of 2026-11-12): for an error or a warning, the stage that found it
//! (`syntax`, `check`, `solve`, `design`, `guideline`, `power`, or `fab` for the rest of a fab
//! run); for a note, `note.` and its cause (`note.not_stated`, `note.part_model`, ...; see
//! `netsmith_core::Cause`). There are no codes per rule.
//!
//! `--format json` prints one [`Report`] for the whole run. `--notes summary` prints the notes
//! of a board as a count for each cause, and `--notes none` leaves them out; `all`, the default,
//! prints each.

use std::collections::BTreeMap;

use netsmith_core::Cause;
use serde::Serialize;

use netsmith_pipeline::{Built, Problem, Stage};

/// How a command prints the notes of a board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotesMode {
    /// Every note, one line each.
    #[default]
    All,
    /// A count for each cause.
    Summary,
    /// No note.
    None,
}

impl NotesMode {
    pub fn parse(word: &str) -> Option<NotesMode> {
        match word {
            "all" => Some(NotesMode::All),
            "summary" => Some(NotesMode::Summary),
            "none" => Some(NotesMode::None),
            _ => None,
        }
    }
}

/// How a command prints what it found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    /// For a person: the lines it has always printed.
    #[default]
    Text,
    /// One JSON document on standard output, and nothing else there.
    Json,
}

impl Format {
    pub fn parse(word: &str) -> Option<Format> {
        match word {
            "text" => Some(Format::Text),
            "json" => Some(Format::Json),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
    /// A finding of the design rules check of a part's own footprint that a waiver of the layout
    /// file covers (issue #1196): neither an error nor a warning, with the waiver's `reason` and
    /// `source`.
    Waived,
}

/// One thing a command found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,
    /// The member path in the source it is about (`stage.c1`), when it is about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    pub message: String,
    /// The requirements of the physical file on where parts go that the finding is about, each
    /// with what was measured, the window it allows and how far out it is (#816).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<netsmith_pcb::RequirementMiss>,
    /// For a refusal because the parts do not fit: every pair of courtyards that overlap, every
    /// part outside what is kept clear and bigger than the outline, and every part's final
    /// place (#817).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub does_not_fit: Option<Box<netsmith_pcb::FitReport>>,
    /// The sets of lines of the physical file and locks of the layout file that cannot all
    /// hold, each with its kind, the lines, the parts with the box each courtyard lies within,
    /// and the overlap (#779).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<netsmith_pcb::Conflict>,
    /// The faults the placement check found where the placer ended, each with its kind, the
    /// parts with their courtyards, and its numbers (#779).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub placement: Vec<netsmith_pcb::PlacementFault>,
    /// For a finding of the design rules check (`board drc`, issue #1095): the rule that found it
    /// (`clearance`, `short_circuit`, `unconnected_items`, ...). The `code` is the stage, `drc`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// For a finding of the design rules check: where the problem is, in mm in the frame of the
    /// board file (y down), and the layer it is on when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
    /// The nets the finding names (each once, in the order of the things it is about), for a
    /// finding of the design rules check.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub nets: Vec<String>,
    /// The parts (designators) the finding names, each once, for a finding of the design rules
    /// check.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<String>,
    /// What waived the finding, for one a run went past: `for-review` for what `board fab
    /// --for-review` waived (issue #1212). The finding is still an error of the run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waived: Option<String>,
    /// For a finding of the design rules check that is within one part's own footprint (every
    /// item it names is the part's, issue #1196): the part, by its designator. Such a finding is
    /// the footprint's, which the board cannot fix, and the layout file may waive it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footprint: Option<String>,
    /// For a finding within one part's own footprint: the numbers of its pads that the finding
    /// names, each once. A `solder_mask_bridge` finding for a part whose pads no mask web fits
    /// between (issue #1685) stands for many pairs, and lists every pad of them here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pads: Vec<String>,
    /// For a waived finding of the design rules check: the layout file's key of the waiver
    /// (`reg.ic.pad_to_pad`), its reason and its source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Where a source comes from: for a waived finding, the waiver's; for an `assembly_size`
    /// finding, where the board house's least size comes from (issue 1393).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// For a `track_current` or `via_current` finding (issue 1373): the copper, the current it
    /// was held to and what that needs. Its fields are the finding's own in the JSON.
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub carrying: Option<Carrying>,
}

/// The numbers of a `track_current` or `via_current` finding (issue 1373), so that a person can
/// choose between wider copper and a lower current.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Carrying {
    /// The current assumed, in amperes: the net's, or, for copper that only feeds branches, what
    /// they draw. Left out when it is not known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<f64>,
    /// Where the current comes from, in words: which loads, or the pins that state it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub current_from: String,
    /// A track's width, in mm, and the width its current needs on its layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needed_width: Option<f64>,
    /// The vias side by side a cluster has, and how many its current needs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vias: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needed_vias: Option<usize>,
    /// What the sizing is based on: the copper's thickness in mm, the temperature rise in kelvin
    /// and the fabricator's width tolerance (0.2 is 20 %).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copper: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rise: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
}

impl From<&netsmith_geom::DrcCarrying> for Carrying {
    fn from(c: &netsmith_geom::DrcCarrying) -> Carrying {
        Carrying {
            current: c.current,
            current_from: c.current_from.clone(),
            width: c.width,
            needed_width: c.needed_width,
            vias: c.vias,
            needed_vias: c.needed_vias,
            copper: c.copper,
            rise: c.rise,
            tolerance: c.tolerance,
        }
    }
}

/// Where a finding of the design rules check is.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Place {
    pub x: f64,
    pub y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
}

impl Finding {
    /// A finding with only a severity, a code and a message.
    pub fn plain(severity: Severity, code: &str, message: impl Into<String>) -> Finding {
        Finding {
            severity,
            code: code.to_string(),
            board: None,
            path: None,
            file: None,
            line: None,
            column: None,
            message: message.into(),
            requirements: Vec::new(),
            does_not_fit: None,
            conflicts: Vec::new(),
            placement: Vec::new(),
            rule: None,
            place: None,
            nets: Vec::new(),
            parts: Vec::new(),
            waived: None,
            footprint: None,
            pads: Vec::new(),
            waiver: None,
            reason: None,
            source: None,
            carrying: None,
        }
    }

    fn on(mut self, board: &str) -> Finding {
        self.board = Some(board.to_string());
        self
    }
}

/// The findings of netsmith's own design rules check (ADR 0037) as `board drc` and `fab` have
/// them: the code `drc`, the severity (a warning of the check is a warning, an error an error),
/// the rule, where it is, the nets and the parts it names, the part whose own footprint it is
/// within (`footprint`), and the message with the things it is about; after them the findings a
/// waiver of the layout file covers, `waived`, each with the waiver's key, reason and source
/// (issue #1196). `board` names the board a design's check was made on.
pub fn of_drc(report: &netsmith_geom::DrcReport, board: Option<&str>) -> Vec<Finding> {
    report
        .findings
        .iter()
        .map(|f| of_drc_finding(f, board))
        .chain(report.waived.iter().map(|w| of_drc_waived(w, board)))
        .collect()
}

/// One finding of the design rules check as [`of_drc`] has it.
pub fn of_drc_finding(f: &netsmith_geom::DrcFinding, board: Option<&str>) -> Finding {
    let mut nets: Vec<String> = Vec::new();
    for n in f.items.iter().filter_map(|i| i.net.clone()) {
        if !nets.contains(&n) {
            nets.push(n);
        }
    }
    Finding {
        board: board.map(str::to_string),
        rule: Some(f.rule.id().to_string()),
        place: Some(Place {
            x: f.at.x,
            y: f.at.y,
            layer: f.layer.clone(),
        }),
        nets,
        parts: f.parts(),
        footprint: f.own_part().map(str::to_string),
        pads: own_pads(f),
        carrying: f.carrying.as_ref().map(Carrying::from),
        source: f.source.clone(),
        ..Finding::plain(
            match f.severity {
                netsmith_geom::DrcSeverity::Error => Severity::Error,
                // `near_rule_area` (issue #1591): a note, which fails nothing.
                netsmith_geom::DrcSeverity::Note => Severity::Note,
                _ => Severity::Warning,
            },
            "drc",
            f.with_items(),
        )
    }
}

/// The numbers of the pads a finding within one part's own footprint names, each once; none for
/// another finding.
fn own_pads(f: &netsmith_geom::DrcFinding) -> Vec<String> {
    let mut pads: Vec<String> = Vec::new();
    if f.own_part().is_some() {
        for p in f.items.iter().filter_map(|i| i.pad.clone()) {
            if !pads.contains(&p) {
                pads.push(p);
            }
        }
    }
    pads
}

/// A finding of the design rules check that a waiver of the layout file covers, as [`of_drc`]
/// has it (issue #1196): the severity `waived`, with the waiver's key, reason and source. It is
/// not `--for-review`'s `waived` (issue #1212), which marks an error the run went past.
pub fn of_drc_waived(w: &netsmith_geom::DrcWaived, board: Option<&str>) -> Finding {
    Finding {
        severity: Severity::Waived,
        waiver: Some(w.key.clone()),
        reason: Some(w.reason.clone()),
        source: Some(w.source.clone()),
        ..of_drc_finding(&w.finding, board)
    }
}

/// The word of a stage, as the code of an error it found.
pub fn stage_code(stage: Stage) -> &'static str {
    match stage {
        Stage::Syntax => "syntax",
        Stage::Check => "check",
        Stage::Solve => "solve",
        Stage::Design => "design",
        Stage::Guideline => "guideline",
        Stage::Power => "power",
    }
}

/// The errors that stopped a build.
pub fn of_problems(problems: &[Problem]) -> Vec<Finding> {
    problems
        .iter()
        .map(|p| Finding {
            code: stage_code(p.stage).to_string(),
            board: p.board.clone(),
            path: p.path.clone(),
            file: p.at.as_ref().map(|a| a.file.clone()),
            line: p.at.as_ref().map(|a| a.line),
            column: p.at.as_ref().map(|a| a.column),
            message: p.message.clone(),
            ..Finding::plain(Severity::Error, "", "")
        })
        .collect()
}

/// The warnings of a board that built: what it does against a guideline.
pub fn warnings(built: &Built) -> Vec<Finding> {
    let label = built.label();
    built
        .guidelines
        .iter()
        .map(|g| Finding {
            path: g.at.map(|at| built.trace.model.path_of(at)),
            ..Finding::plain(
                Severity::Warning,
                stage_code(Stage::Guideline),
                format!("{} guideline: {}", g.rule, g.message),
            )
            .on(&label)
        })
        .collect()
}

/// The notes of a board that built, each with its cause: the solver's and the design stage's,
/// then the power model's (a rail nothing supplies is `supply`).
pub fn notes(built: &Built) -> Vec<(Cause, Finding)> {
    let label = built.label();
    let mut out: Vec<(Cause, Finding)> = built
        .notes
        .iter()
        .enumerate()
        .map(|(i, note)| {
            let cause = built.note_causes.get(i).copied().unwrap_or(Cause::Other);
            (cause, note_finding(cause, note).on(&label))
        })
        .collect();
    for line in built.power_notes() {
        out.push((Cause::Supply, note_finding(Cause::Supply, &line).on(&label)));
    }
    out
}

fn note_finding(cause: Cause, message: &str) -> Finding {
    Finding::plain(Severity::Note, &format!("note.{}", cause.id()), message)
}

/// How many notes there are of each cause, in the order of [`Cause::ALL`].
pub fn counts<'a>(causes: impl IntoIterator<Item = &'a Cause>) -> Vec<(Cause, usize)> {
    let mut by: BTreeMap<&'static str, usize> = BTreeMap::new();
    for c in causes {
        *by.entry(c.id()).or_default() += 1;
    }
    Cause::ALL
        .iter()
        .filter_map(|c| by.get(c.id()).map(|&n| (*c, n)))
        .collect()
}

/// The one line `--notes summary` prints for a board's notes: `note: LedExample: 74 notes:
/// 31 part_model, 18 not_stated, ... (`--notes all` lists them)`. `None` when it has none.
pub fn summary_line(board: &str, notes: &[(Cause, Finding)]) -> Option<String> {
    if notes.is_empty() {
        return None;
    }
    let parts: Vec<String> = counts(notes.iter().map(|(c, _)| c))
        .iter()
        .map(|(c, n)| format!("{n} {}", c.id()))
        .collect();
    Some(format!(
        "note: {board}: {} note{}: {} (`--notes all` lists them)",
        notes.len(),
        if notes.len() == 1 { "" } else { "s" },
        parts.join(", ")
    ))
}

/// A board a command built or made, for the report.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoardOut {
    pub board: String,
    /// The folder it wrote, if it wrote one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// For `fab`: where each part's footprint origin is and how far it is from its body's
    /// centre (#457).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<netsmith_pcb::PartOrigin>,
    /// For `board drc` of a design's board: the outline's top left corner in the frame of the
    /// places of its findings, so that a place less this is in mm from the outline's corner (y
    /// down), the frame of the layout file and the routes file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline_at: Option<Place>,
    /// For `fab` and `board drc`: the nets the routing made long against their straight
    /// length, and the nets with the most vias (issue #1194), each with its length, its
    /// straight length, the ratio and its vias. Left out when there are none.
    #[serde(skip_serializing_if = "netsmith_fab::NetRoutes::is_empty")]
    pub routes: netsmith_fab::NetRoutes,
    /// For `board drc` of a design's board: the diff pairs whose skew is over their limit
    /// (issue #1223, #1234), each with the numbers `board nets` gives a pair. Left out when there
    /// are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pair_skews: Vec<netsmith_fab::PairRow>,
    /// For `board drc` of a design's board: the groups of nets matched in length whose spread
    /// is over their tolerance (issue #1234), each with the numbers `board nets` gives a group.
    /// Left out when there are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub group_spreads: Vec<netsmith_fab::GroupRow>,
    /// For `board drc` of a design's board: the `TrackPath`s whose tracks are longer than
    /// their length budget (issue #1275) or lose more than their loss budget (issue #1405).
    /// Left out when there are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub long_paths: Vec<netsmith_fab::PathRow>,
    /// For `board drc` of a design's board: the quality of its routed copper (issue #1481), the
    /// totals and each net with a defect, as `board nets` has it. Left out when there is no
    /// defect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<netsmith_fabricate::net_quality::Quality>,
    /// For `board drc` of a design's board: the power nets whose routed tracks drop more than
    /// the track-drop budget (issue #1540), as `board nets` has them. Left out when there are
    /// none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub track_drops: Vec<netsmith_fabricate::track_drop::NetDrop>,
}

/// The count of the notes of the run, for each cause, whether or not they are listed.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct NoteCounts {
    pub total: usize,
    pub by_cause: BTreeMap<String, usize>,
}

/// What `--format json` prints: one document for the whole run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub command: &'static str,
    /// The frame of the document's places (issue #1588): `outline` (mm from the outline's top
    /// left corner, y down) for `board check`, `build` and `fab`, whose places are the boxes of
    /// the placement; left out where the command adds it as it prints (`board drc`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<&'static str>,
    /// Whether the run did what it was asked; its exit status is 0 then.
    pub ok: bool,
    pub boards: Vec<BoardOut>,
    pub findings: Vec<Finding>,
    pub notes: NoteCounts,
}

impl Report {
    pub fn new(command: &'static str) -> Report {
        Report {
            command,
            frame: None,
            ok: true,
            boards: Vec::new(),
            findings: Vec::new(),
            notes: NoteCounts::default(),
        }
    }

    /// Adds the warnings and the notes of a board that built, the notes as `mode` says: listed
    /// with `all`, counted always.
    pub fn add_built(&mut self, built: &Built, mode: NotesMode) {
        self.findings.extend(warnings(built));
        let notes = notes(built);
        for (cause, _) in &notes {
            self.notes.total += 1;
            *self
                .notes
                .by_cause
                .entry(cause.id().to_string())
                .or_default() += 1;
        }
        if mode == NotesMode::All {
            self.findings.extend(notes.into_iter().map(|(_, f)| f));
        }
    }

    /// The document, with a newline.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("a report is plain data");
        text.push('\n');
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_counts_the_notes_by_cause_in_the_docks_order() {
        let note = |c: Cause| (c, note_finding(c, "x"));
        let notes = vec![
            note(Cause::NotStated),
            note(Cause::PartModel),
            note(Cause::PartModel),
            note(Cause::Supply),
        ];
        assert_eq!(
            summary_line("B", &notes).unwrap(),
            "note: B: 4 notes: 1 not_stated, 2 part_model, 1 supply (`--notes all` lists them)"
        );
        assert_eq!(summary_line("B", &[]), None);
        assert_eq!(
            summary_line("B", &notes[..1]).unwrap(),
            "note: B: 1 note: 1 not_stated (`--notes all` lists them)"
        );
    }

    #[test]
    fn a_finding_leaves_out_what_it_does_not_have() {
        let f = Finding::plain(Severity::Note, "note.other", "hello");
        assert_eq!(
            serde_json::to_string(&f).unwrap(),
            r#"{"severity":"note","code":"note.other","message":"hello"}"#
        );
    }

    /// Issue 1393: the source of the figure an `assembly_size` finding is judged against is the
    /// finding's `source` in the JSON, and a finding with none has no such field.
    #[test]
    fn an_assembly_size_finding_has_its_source_as_a_field() {
        use netsmith_geom::{DrcFinding, DrcRule, DrcSeverity};
        let finding = |source| DrcFinding {
            rule: DrcRule::AssemblySize,
            severity: DrcSeverity::Warning,
            message: "m".to_string(),
            at: Default::default(),
            layer: Some("Edge.Cuts".to_string()),
            items: Vec::new(),
            carrying: None,
            source,
            distance: None,
        };
        let with = serde_json::to_value(of_drc_finding(
            &finding(Some("a page, read 2026-11-19".to_string())),
            None,
        ))
        .unwrap();
        assert_eq!(with["rule"], "assembly_size");
        assert_eq!(with["source"], "a page, read 2026-11-19");
        let without = serde_json::to_value(of_drc_finding(&finding(None), None)).unwrap();
        assert!(without.get("source").is_none(), "{without}");
    }

    /// Issue 1373: a `track_current` finding has the width, the width needed and the current as
    /// fields of its own, and a finding of another rule has none of them.
    #[test]
    fn a_current_finding_has_its_numbers_as_fields() {
        use netsmith_geom::{DrcCarrying, DrcFinding, DrcRule, DrcSeverity};
        let finding = |carrying| DrcFinding {
            rule: DrcRule::TrackCurrent,
            severity: DrcSeverity::Error,
            message: "m".to_string(),
            at: Default::default(),
            layer: Some("F.Cu".to_string()),
            items: Vec::new(),
            carrying,
            source: None,
            distance: None,
        };
        let with = finding(Some(DrcCarrying {
            current: Some(2.5),
            current_from: "rail `a` 2.5 A".to_string(),
            width: Some(0.2),
            needed_width: Some(0.55),
            copper: Some(0.035),
            rise: Some(10.0),
            tolerance: Some(0.2),
            ..DrcCarrying::default()
        }));
        let json = serde_json::to_value(of_drc_finding(&with, None)).unwrap();
        assert_eq!(json["rule"], "track_current");
        assert_eq!(json["width"], 0.2);
        assert_eq!(json["needed_width"], 0.55);
        assert_eq!(json["current"], 2.5);
        assert_eq!(json["current_from"], "rail `a` 2.5 A");
        assert_eq!(
            (json["copper"].clone(), json["rise"].clone()),
            (0.035.into(), 10.0.into())
        );
        assert!(json.get("vias").is_none(), "{json}");
        let without = serde_json::to_value(of_drc_finding(&finding(None), None)).unwrap();
        for key in ["width", "needed_width", "current", "current_from"] {
            assert!(without.get(key).is_none(), "{key}: {without}");
        }
    }
}
