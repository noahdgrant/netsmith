//! The waiver of a finding of a part's own footprint (issue #1196).
//!
//! Some findings come from a part's footprint and not from the board: a land pattern whose pads
//! are nearer than the process asks (`pad_to_pad` between two pads of one part), a plated ring a
//! maker drew thinner than the process leaves. The board cannot fix them. The layout file may
//! waive one, beside the waivers of guidelines, under the part's path and the rule's name, with
//! the reason, the source that the land pattern is right for the part, and the pads (or the
//! place) of the finding (`netsmith_pcb::Waiver`):
//!
//! ```toml
//! [Board.waive."reg.ic.pad_to_pad"]
//! reason = "the maker's land pattern; its pads are 0.1448 mm apart, under the 0.15 mm asked"
//! source = "maker's datasheet, Recommended land pattern, p.12"
//! pads = ["2", "3"]
//! ```
//!
//! **What a waiver covers.** A finding of its rule that is the part's own ([`Finding::own_part`]):
//! two or more items, every one of them the part's, or one item of the part under a rule that
//! judges an item alone ([`Rule::judges_an_item_alone`]: a plated ring, a drill, a courtyard). With
//! `pads`, every item is one of those pads; with `at`, the finding's place is within
//! [`PLACE_TOLERANCE`] of it. A finding with a track, a via, a zone, the edge or another part in it
//! is the board's and is never waived: the board can change it.
//!
//! **What a waived finding is.** It leaves the findings ([`Report::findings`]) for
//! [`Report::waived`], with its reason and source: it is no error and no warning, so it neither
//! stops `fab` nor fails `board drc`, and both list it as waived, `fab` in `fab-notes.txt`. A
//! waiver that covers no finding is itself one, an error ([`Rule::UnmatchedWaiver`]): the part, its
//! footprint or the process changed, and the waiver would hide a finding it was not written for.

use super::{Finding, Item, Report, Rule, Severity};
use crate::geom::Pt;

/// How far a waiver's place may be from the place of the finding it covers, in mm: the tolerance
/// to which the cross-check compares the places of findings.
pub const PLACE_TOLERANCE: f64 = 0.1;

/// A waiver of the findings of one rule of a part's own footprint ([`DrcExtras::waivers`]).
///
/// [`DrcExtras::waivers`]: super::Extras::waivers
#[derive(Debug, Clone, PartialEq)]
pub struct Waiver {
    /// The key of the layout file that states it: the part's path and the rule's name,
    /// `reg.ic.pad_to_pad`.
    pub key: String,
    /// The rule whose findings it covers.
    pub rule: Rule,
    /// The part, by its designator, as the findings name it (`U1`), and where it is (in the frame
    /// of the board file), for the finding of a waiver that covers none.
    pub part: String,
    pub part_at: Pt,
    /// The pads of the part whose findings among themselves it covers; empty when it names a place.
    pub pads: Vec<String>,
    /// The place of the finding it covers, in the frame of the board file; `None` when it names
    /// pads.
    pub at: Option<Pt>,
    /// Why the finding is accepted, and the document that says the land pattern is right.
    pub reason: String,
    pub source: String,
}

impl Waiver {
    /// Whether this waiver covers `finding`.
    pub fn covers(&self, finding: &Finding) -> bool {
        finding.rule == self.rule
            && finding.own_part() == Some(self.part.as_str())
            && (self.pads.is_empty()
                || finding
                    .items
                    .iter()
                    .all(|i| i.pad.as_ref().is_some_and(|p| self.pads.contains(p))))
            && self.at.is_none_or(|at| {
                (finding.at.x - at.x).hypot(finding.at.y - at.y) <= PLACE_TOLERANCE + 1e-9
            })
    }

    /// What it names besides the part: `pads 2, 3` or `the place (1.20, 3.40)`.
    fn what(&self) -> String {
        match self.at {
            Some(at) if self.pads.is_empty() => format!("at ({:.2}, {:.2})", at.x, at.y),
            _ => format!("pads {}", self.pads.join(", ")),
        }
    }
}

/// A finding a waiver covers, with the waiver's reason and source.
#[derive(Debug, Clone, PartialEq)]
pub struct Waived {
    pub finding: Finding,
    /// The layout file's key of the waiver: `reg.ic.pad_to_pad`.
    pub key: String,
    pub reason: String,
    pub source: String,
}

impl std::fmt::Display for Waived {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let finding = self.finding.to_string();
        // `error [rule] ...` reads `waived [rule] ...`: it is neither an error nor a warning.
        let rest = finding
            .split_once(' ')
            .map_or(finding.as_str(), |(_, rest)| rest);
        write!(
            f,
            "waived {rest}, by `{}`: {} (source: {})",
            self.key, self.reason, self.source
        )
    }
}

impl Rule {
    /// Whether the rule judges one item alone, so that a finding of it with one item of a part is
    /// the part's own: a plated ring, a drill or a slot, a courtyard, a text's size. A rule that
    /// measures an item against the board (the edge, a requirement, a fiducial's place) is not.
    pub fn judges_an_item_alone(self) -> bool {
        matches!(
            self,
            Rule::AnnularWidth
                | Rule::PlatedRing
                | Rule::DrillOutOfRange
                | Rule::NpthDrill
                | Rule::SlotWidth
                | Rule::MalformedCourtyard
                | Rule::TextHeight
                | Rule::TextThickness
        )
    }

    /// Whether a waiver of the layout file may name the rule: every rule but the finding of a
    /// waiver itself and a requirement of the design, which the physical file states and makes a
    /// warning (`constraints = "warn"`).
    pub fn may_be_waived(self) -> bool {
        !matches!(self, Rule::UnmatchedWaiver | Rule::Constraint)
    }
}

impl Finding {
    /// The part whose own footprint the finding is within, by its designator: every item is the
    /// part's, and there are two or more, or the rule judges one alone
    /// ([`Rule::judges_an_item_alone`]). `None` when the board has a part in it.
    pub fn own_part(&self) -> Option<&str> {
        let first = self.items.first()?.part.as_deref()?;
        let all = self.items.iter().all(|i| i.part.as_deref() == Some(first));
        (all && (self.items.len() >= 2 || self.rule.judges_an_item_alone())).then_some(first)
    }
}

/// Moves the findings that `waivers` cover out of `report.findings` into `report.waived`, and adds
/// an error for each waiver that covers none.
pub(super) fn apply(report: &mut Report, waivers: &[Waiver]) {
    if waivers.is_empty() {
        return;
    }
    let mut used = vec![false; waivers.len()];
    let mut kept = Vec::with_capacity(report.findings.len());
    for finding in std::mem::take(&mut report.findings) {
        match waivers.iter().position(|w| w.covers(&finding)) {
            Some(k) => {
                used[k] = true;
                let w = &waivers[k];
                report.waived.push(Waived {
                    finding,
                    key: w.key.clone(),
                    reason: w.reason.clone(),
                    source: w.source.clone(),
                });
            }
            None => kept.push(finding),
        }
    }
    for (w, _) in waivers.iter().zip(&used).filter(|(_, used)| !**used) {
        kept.push(Finding {
            rule: Rule::UnmatchedWaiver,
            severity: Severity::Error,
            message: format!(
                "the layout file waives `{}` of {} ({}), and the check finds no such finding of {}'s own footprint: the part, its footprint or the process changed, so remove the waiver or name the finding it is for",
                w.rule.id(),
                w.part,
                w.what(),
                w.part
            ),
            at: w.at.unwrap_or(w.part_at),
            layer: None,
            items: vec![Item {
                what: format!("waiver `{}`", w.key),
                at: w.part_at,
                part: Some(w.part.clone()),
                pad: None,
                net: None,
            }],
        });
    }
    kept.sort_by_key(|f| std::cmp::Reverse(f.severity));
    report.findings = kept;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(part: &str, pad: &str, at: Pt) -> Item {
        Item {
            what: format!("Pad {pad} of {part}"),
            at,
            part: Some(part.into()),
            pad: Some(pad.into()),
            net: None,
        }
    }

    fn finding(rule: Rule, severity: Severity, items: Vec<Item>) -> Finding {
        Finding {
            rule,
            severity,
            message: format!("{} found", rule.id()),
            at: items.first().map_or(Pt::new(0.0, 0.0), |i| i.at),
            layer: Some("F.Cu".into()),
            items,
        }
    }

    fn waiver(rule: Rule, part: &str, pads: &[&str], at: Option<Pt>) -> Waiver {
        Waiver {
            key: format!("{}.{}", part.to_lowercase(), rule.id()),
            rule,
            part: part.into(),
            part_at: Pt::new(10.0, 10.0),
            pads: pads.iter().map(|p| p.to_string()).collect(),
            at,
            reason: "the maker's land pattern".into(),
            source: "datasheet p.12".into(),
        }
    }

    #[test]
    fn a_finding_is_the_parts_own_when_every_item_is_the_parts() {
        let (a, b) = (Pt::new(1.0, 1.0), Pt::new(1.5, 1.0));
        let own = finding(
            Rule::PadToPad,
            Severity::Error,
            vec![pad("U1", "2", a), pad("U1", "3", b)],
        );
        assert_eq!(own.own_part(), Some("U1"));
        let two = finding(
            Rule::PadToPad,
            Severity::Error,
            vec![pad("U1", "2", a), pad("C1", "1", b)],
        );
        assert_eq!(two.own_part(), None);
        let mut track = own.clone();
        track.items[1].part = None;
        assert_eq!(track.own_part(), None, "a track is the board's");
        // One item: the part's own only under a rule that judges an item alone.
        let ring = finding(Rule::PlatedRing, Severity::Error, vec![pad("J1", "1", a)]);
        assert_eq!(ring.own_part(), Some("J1"));
        let edge = finding(
            Rule::CopperEdgeClearance,
            Severity::Error,
            vec![pad("J1", "1", a)],
        );
        assert_eq!(edge.own_part(), None);
    }

    #[test]
    fn a_waiver_moves_the_findings_it_covers_and_a_waiver_of_nothing_is_an_error() {
        let p = |x| Pt::new(x, 1.0);
        let mut report = Report {
            findings: vec![
                finding(
                    Rule::PadToPad,
                    Severity::Error,
                    vec![pad("U1", "2", p(1.0)), pad("U1", "3", p(1.5))],
                ),
                finding(
                    Rule::PadToPad,
                    Severity::Error,
                    vec![pad("U1", "3", p(1.5)), pad("U1", "4", p(2.0))],
                ),
                // Of U1 and the board's copper: not waived by U1's waiver.
                finding(
                    Rule::PadToPad,
                    Severity::Error,
                    vec![pad("U1", "2", p(1.0)), pad("C1", "1", p(0.5))],
                ),
                finding(
                    Rule::SilkOverCopper,
                    Severity::Warning,
                    vec![pad("U1", "1", p(5.0)), pad("U1", "9", p(5.0))],
                ),
            ],
            ..Report::default()
        };
        let waivers = [
            waiver(Rule::PadToPad, "U1", &["2", "3"], None),
            waiver(Rule::SilkOverCopper, "U1", &[], Some(Pt::new(5.05, 1.0))),
            waiver(Rule::PlatedRing, "U1", &["1"], None),
        ];
        apply(&mut report, &waivers);
        assert_eq!(report.waived.len(), 2, "{:#?}", report.waived);
        assert_eq!(report.waived[0].key, "u1.pad_to_pad");
        assert_eq!(report.waived[1].finding.rule, Rule::SilkOverCopper);
        let rules: Vec<Rule> = report.findings.iter().map(|f| f.rule).collect();
        assert_eq!(
            rules,
            [Rule::PadToPad, Rule::PadToPad, Rule::UnmatchedWaiver],
            "pads 3 and 4 and U1 against C1 are left, and the ring's waiver covers nothing"
        );
        let unmatched = &report.findings[2];
        assert_eq!(unmatched.severity, Severity::Error);
        assert!(
            unmatched
                .message
                .contains("waives `plated_ring` of U1 (pads 1)"),
            "{}",
            unmatched.message
        );
        assert_eq!(report.counts().violations, 3);
        let line = report.waived[0].to_string();
        assert!(
            line.starts_with("waived [pad_to_pad] pad_to_pad found at (1.000, 1.000) on F.Cu")
                && line.ends_with(
                    "Pad 3 of U1 (within U1's own footprint), by `u1.pad_to_pad`: the maker's land pattern (source: datasheet p.12)"
                ),
            "{line}"
        );
    }

    #[test]
    fn a_place_further_than_the_tolerance_covers_nothing() {
        let f = finding(
            Rule::PadToPad,
            Severity::Error,
            vec![
                pad("U1", "2", Pt::new(1.0, 1.0)),
                pad("U1", "3", Pt::new(1.5, 1.0)),
            ],
        );
        assert!(waiver(Rule::PadToPad, "U1", &[], Some(Pt::new(1.09, 1.0))).covers(&f));
        assert!(!waiver(Rule::PadToPad, "U1", &[], Some(Pt::new(1.2, 1.0))).covers(&f));
        assert!(!waiver(Rule::Clearance, "U1", &["2", "3"], None).covers(&f));
        assert!(!waiver(Rule::PadToPad, "U2", &["2", "3"], None).covers(&f));
    }
}
