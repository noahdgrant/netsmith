//! KiCad's design rules check as an independent witness (ADR 0037): its JSON report read
//! into findings, and the comparison with netsmith's own report that the cross-check and the
//! tests run. Nothing here is needed to check a board.
//!
//! Two findings are the same if they are of the same rule and the items they name are at the
//! same places (within a tolerance, 0.1 mm by default): KiCad's report gives each item's
//! position (a pad's centre, a track's start, a part's origin), and netsmith reports the
//! same positions. A finding of one report that the other lacks is a difference unless it is
//! one of the documented ones: a rule only netsmith has ([`Rule::HoleToEdge`],
//! [`Rule::ViaInPad`]), or a kind of finding netsmith does not check ([`OUT_OF_SCOPE`]).

use super::{Report, Rule, Severity};
use crate::geom::Pt;

/// A finding in KiCad's report.
#[derive(Debug, Clone, PartialEq)]
pub struct Reported {
    /// KiCad's `type`, such as `clearance`.
    pub kind: String,
    pub severity: Severity,
    pub description: String,
    /// The items it names: their descriptions and where they are.
    pub items: Vec<(String, Pt)>,
}

/// What KiCad reports that netsmith does not check, by KiCad's name: a footprint compared
/// with its library, copper slivers, starved thermals, isolated copper, a zone fill that is
/// not as the zone would make it, and the like. A board that has them is not compared on
/// them. Also the kinds of finding that need KiCad's library tables or the schematic.
pub const OUT_OF_SCOPE: &[&str] = &[
    "lib_footprint_issues",
    "lib_footprint_mismatch",
    "footprint_type_mismatch",
    "footprint_filters_mismatch",
    "missing_footprint",
    "extra_footprint",
    "duplicate_footprints",
    "net_conflict",
    "unresolved_variable",
    "copper_sliver",
    "isolated_copper",
    "starved_thermal",
    "connection_width",
    "zones_intersect",
    "tracks_crossing",
    "padstack",
    "pth_inside_courtyard",
    "npth_inside_courtyard",
    "missing_courtyard",
    "through_hole_pad_without_hole",
    "mirrored_text_on_front_layer",
    "nonmirrored_text_on_back_layer",
    "text_on_edge_cuts",
    "item_on_disabled_layer",
    "microvia_drill_out_of_range",
    "too_many_vias",
    "length_out_of_range",
    "skew_out_of_range",
    "diff_pair_gap_out_of_range",
    "diff_pair_uncoupled_length_too_long",
    "via_dangling_unused",
    "assertion_failure",
    "generic_warning",
    "generic_error",
];

/// The findings in a `kicad-cli pcb drc --format json` report: `violations` and
/// `unconnected_items`, without those the person told KiCad to ignore or exclude.
pub fn parse(json: &str) -> Result<Vec<Reported>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("KiCad's report is not JSON: {e}"))?;
    let mut out = Vec::new();
    for section in ["violations", "unconnected_items"] {
        let Some(entries) = v[section].as_array() else {
            continue;
        };
        for e in entries {
            let severity = match e["severity"].as_str() {
                Some("error") => Severity::Error,
                Some("warning") => Severity::Warning,
                _ => continue,
            };
            let items = e["items"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .map(|i| {
                            (
                                i["description"].as_str().unwrap_or("").to_string(),
                                Pt::new(
                                    i["pos"]["x"].as_f64().unwrap_or(0.0),
                                    i["pos"]["y"].as_f64().unwrap_or(0.0),
                                ),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.push(Reported {
                kind: e["type"].as_str().unwrap_or("").to_string(),
                severity,
                description: e["description"].as_str().unwrap_or("").to_string(),
                items,
            });
        }
    }
    Ok(out)
}

/// The comparison of netsmith's report with KiCad's.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Comparison {
    /// Findings both have.
    pub agreed: usize,
    /// Findings of netsmith's that KiCad lacks, in a rule KiCad has.
    pub only_netsmith: Vec<String>,
    /// Findings of KiCad's that netsmith lacks, in a rule netsmith checks.
    pub only_kicad: Vec<String>,
    /// Both have it, but not at the same severity.
    pub severity: Vec<String>,
    /// Missing connections both name but between other ends: several ends are equally
    /// near, and the two checks pick differently. They are counted among those that agree.
    pub other_ends: usize,
    /// Findings of rules only netsmith has, which cannot be compared.
    pub netsmith_only_rules: usize,
    /// Findings of kinds netsmith does not check.
    pub out_of_scope: usize,
}

impl Comparison {
    /// Whether the two reports agree on everything that can be compared.
    pub fn agrees(&self) -> bool {
        self.only_netsmith.is_empty() && self.only_kicad.is_empty() && self.severity.is_empty()
    }
}

impl std::fmt::Display for Comparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.agreed == 0 && self.agrees() && self.netsmith_only_rules + self.out_of_scope == 0 {
            return writeln!(f, "design rules: neither check found anything");
        }
        writeln!(
            f,
            "design rules: {} finding(s) agree ({} between other ends of a missing connection), {} only netsmith's, {} only KiCad's, {} at another severity ({} of rules KiCad lacks, {} of kinds netsmith does not check)",
            self.agreed,
            self.other_ends,
            self.only_netsmith.len(),
            self.only_kicad.len(),
            self.severity.len(),
            self.netsmith_only_rules,
            self.out_of_scope
        )?;
        for line in &self.only_netsmith {
            writeln!(f, "  only netsmith: {line}")?;
        }
        for line in &self.only_kicad {
            writeln!(f, "  only KiCad:    {line}")?;
        }
        for line in &self.severity {
            writeln!(f, "  severity:      {line}")?;
        }
        Ok(())
    }
}

/// Whether the items at `a` and `b` are at the same places: every place of the shorter list
/// has a distinct place of the longer within `tolerance`.
fn same_places(a: &[Pt], b: &[Pt], tolerance: f64) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let mut taken = vec![false; long.len()];
    short.iter().all(|p| {
        match long
            .iter()
            .enumerate()
            .find(|(i, q)| !taken[*i] && p.dist(**q) <= tolerance)
        {
            Some((i, _)) => {
                taken[i] = true;
                true
            }
            None => false,
        }
    })
}

/// Compares the two reports. `tolerance` is how far apart (mm) two items of the same
/// finding may be reported.
pub fn compare(ours: &Report, kicad: &[Reported], tolerance: f64) -> Comparison {
    let mut c = Comparison::default();
    let mut taken = vec![false; kicad.len()];
    for f in &ours.findings {
        if matches!(f.rule, Rule::HoleToEdge | Rule::ViaInPad | Rule::Constraint) {
            c.netsmith_only_rules += 1;
            continue;
        }
        let places: Vec<Pt> = f.items.iter().map(|i| i.at).collect();
        let found = kicad.iter().enumerate().find(|(i, k)| {
            !taken[*i]
                && k.kind == f.rule.id()
                && same_places(
                    &places,
                    &k.items.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
                    tolerance,
                )
        });
        match found {
            Some((i, k)) => {
                taken[i] = true;
                c.agreed += 1;
                if k.severity != f.severity {
                    c.severity.push(format!(
                        "{}: netsmith says {}, KiCad {}: {}",
                        f.rule.id(),
                        f.severity.word(),
                        k.severity.word(),
                        f.message
                    ));
                }
            }
            None => c.only_netsmith.push(f.to_string()),
        }
    }
    // Missing connections between other ends: as many on each side is the same connections.
    let unmatched_ours: Vec<usize> = c
        .only_netsmith
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("[unconnected_items]"))
        .map(|(i, _)| i)
        .collect();
    let unmatched_theirs: Vec<usize> = kicad
        .iter()
        .enumerate()
        .filter(|(i, k)| !taken[*i] && k.kind == "unconnected_items")
        .map(|(i, _)| i)
        .collect();
    if !unmatched_ours.is_empty() && unmatched_ours.len() == unmatched_theirs.len() {
        for i in unmatched_ours.iter().rev() {
            c.only_netsmith.remove(*i);
        }
        for i in &unmatched_theirs {
            taken[*i] = true;
        }
        c.agreed += unmatched_theirs.len();
        c.other_ends += unmatched_theirs.len();
    }
    for (i, k) in kicad.iter().enumerate().filter(|(i, _)| !taken[*i]) {
        let _ = i;
        // KiCad says an open courtyard twice, as not closed and as self-intersecting.
        let twice = k.kind == "malformed_courtyard"
            && kicad.iter().enumerate().any(|(j, o)| {
                taken[j]
                    && o.kind == k.kind
                    && same_places(
                        &o.items.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
                        &k.items.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
                        tolerance,
                    )
            });
        if twice {
            continue;
        }
        if OUT_OF_SCOPE.contains(&k.kind.as_str()) {
            c.out_of_scope += 1;
            continue;
        }
        let items: Vec<String> = k
            .items
            .iter()
            .map(|(d, p)| format!("{d} at ({:.3}, {:.3})", p.x, p.y))
            .collect();
        c.only_kicad.push(format!(
            "{} [{}] {}: {}",
            k.kind,
            k.severity.word(),
            k.description,
            items.join("; ")
        ));
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drc::{Finding, Item};

    const JSON: &str = r#"{"violations": [
      {"type": "clearance", "severity": "error", "description": "Clearance violation",
       "items": [{"description": "Track [A]", "pos": {"x": 1.0, "y": 2.0}},
                 {"description": "Track [B]", "pos": {"x": 5.0, "y": 2.0}}]},
      {"type": "lib_footprint_issues", "severity": "warning", "description": "library",
       "items": [{"description": "Footprint R1", "pos": {"x": 1.0, "y": 2.0}}]},
      {"type": "hole_clearance", "severity": "ignore", "description": "ignored", "items": []}],
     "unconnected_items": []}"#;

    fn finding(rule: Rule, places: &[(f64, f64)]) -> Finding {
        Finding {
            rule,
            severity: Severity::Error,
            message: "m".into(),
            at: Pt::default(),
            layer: None,
            items: places
                .iter()
                .map(|(x, y)| Item {
                    what: "i".into(),
                    at: Pt::new(*x, *y),
                    part: None,
                    pad: None,
                    net: None,
                })
                .collect(),
        }
    }

    #[test]
    fn a_report_is_read_without_what_the_person_ignored() {
        let k = parse(JSON).unwrap();
        assert_eq!(k.len(), 2);
        assert_eq!(k[0].kind, "clearance");
        assert_eq!(k[0].items[1].1, Pt::new(5.0, 2.0));
        assert!(parse("nope").is_err());
    }

    #[test]
    fn findings_of_one_rule_at_the_same_places_agree_in_either_order() {
        let k = parse(JSON).unwrap();
        let ours = Report {
            findings: vec![finding(Rule::Clearance, &[(5.05, 2.0), (1.0, 2.04)])],
        };
        let c = compare(&ours, &k, 0.1);
        assert_eq!((c.agreed, c.out_of_scope), (1, 1));
        assert!(c.agrees(), "{c}");
    }

    #[test]
    fn a_finding_one_side_lacks_is_named_and_a_place_off_by_more_than_the_tolerance_does_not_match()
    {
        let k = parse(JSON).unwrap();
        let ours = Report {
            findings: vec![
                finding(Rule::Clearance, &[(1.5, 2.0), (5.0, 2.0)]),
                finding(Rule::ShortingItems, &[(9.0, 9.0)]),
                finding(Rule::HoleToEdge, &[(0.0, 0.0)]),
            ],
        };
        let c = compare(&ours, &k, 0.1);
        assert_eq!(c.agreed, 0);
        assert_eq!(c.only_netsmith.len(), 2);
        assert_eq!(c.only_kicad.len(), 1);
        assert_eq!(c.netsmith_only_rules, 1);
        assert!(!c.agrees());
        assert!(c.to_string().contains("only KiCad:") && c.to_string().contains("clearance"));
    }
}
