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
//!
//! **The process's rules in the `.kicad_dru`** (issue #1185). A finding KiCad makes of a rule
//! that `fab` writes into the rules file beside the board (`netsmith_geom::drc_kicad_dru`) names
//! it, `(rule 'netsmith pad_to_pad' ...)`, and [`parse`] gives it netsmith's rule for its kind,
//! keeping KiCad's own in [`Reported::base`]. [`compare_with_rules_file`] compares those rules
//! too, when both checks read the file; [`compare`] does not (the rules are netsmith's, as
//! without the file). Either way a finding of netsmith's under KiCad's base rule at the same
//! places agrees with it: a pad pair 0.08 mm apart breaks both `clearance` (0.1 mm) and
//! `pad_to_pad` (0.15 mm) in netsmith's check, and KiCad reports it twice, once for each (the
//! rule of the `.kicad_dru` for the pads' space is a `physical_clearance`, which sits beside the
//! project's `clearance`; a finding under a base rule KiCad names also agrees, for the files that
//! hold another kind of rule there).
//!
//! **Pads of one footprint** (issue #1229). KiCad's check measures the space between two pads of
//! one footprint as it does that between two parts' pads, so the rule of the `.kicad_dru` reports
//! both; its `clearance` constraint is silent where a pad or its footprint states a clearance of
//! its own (a library's), and the rule is a `physical_clearance` so that it is not (see
//! `netsmith_geom::drc_kicad_dru`). KiCad's report of two pads that touch is the short's,
//! counted in [`Comparison::measured_otherwise`].
//!
//! **Measured otherwise** (issue #1186). A finding of a silkscreen rule one check alone makes of
//! a pair that netsmith's report says the font decides (`DrcReport::font_dependent`: a text in
//! netsmith's font near the least, which KiCad draws in its own) is counted in
//! [`Comparison::measured_otherwise`], not as a difference; so is netsmith's finding of copper or
//! silkscreen near a circle on `Edge.Cuts` or `Margin`, which KiCad 9 finds only where the two
//! cross. Of several sides of the edge within the least, KiCad names the first it finds and
//! netsmith the nearest, so an edge finding that agrees on everything but the edge agrees.

use netsmith_geom::{DrcReport, DrcRule, DrcSeverity, Pt};

/// A finding in KiCad's report.
#[derive(Debug, Clone, PartialEq)]
pub struct Reported {
    /// The rule: KiCad's `type`, such as `clearance`, or for a finding of a rule of the
    /// `.kicad_dru` that netsmith writes, netsmith's rule that it holds (`pad_to_pad`).
    pub kind: String,
    /// KiCad's `type`, whatever the rule.
    pub base: String,
    pub severity: DrcSeverity,
    pub description: String,
    /// The items it names: their descriptions and where they are.
    pub items: Vec<(String, Pt)>,
}

/// What KiCad reports that netsmith does not check, or measures otherwise, by KiCad's name: a footprint compared
/// with its library, copper slivers, a zone fill that is not as the zone would make it, and the
/// like. A board that has them is not compared on them. Also the kinds of finding that need
/// KiCad's library tables or the schematic, and starved thermals and isolated copper, which both
/// checks report but each of its own fill (netsmith's, and KiCad's refill of the zones before
/// its check), so neither side's are compared here; netsmith makes the spokes KiCad makes
/// (issue #1115), and `tests/thermal_spokes.rs` compares the starved thermals of a board.
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
                Some("error") => DrcSeverity::Error,
                Some("warning") => DrcSeverity::Warning,
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
            let base = e["type"].as_str().unwrap_or("").to_string();
            let description = e["description"].as_str().unwrap_or("").to_string();
            out.push(Reported {
                kind: rule_of_rules_file(&description).unwrap_or_else(|| base.clone()),
                base,
                severity,
                description,
                items,
            });
        }
    }
    Ok(out)
}

/// The rule of netsmith's that a finding of KiCad's names when a rule of the `.kicad_dru`
/// netsmith writes made it: `Clearance violation (rule 'netsmith pad_to_pad' clearance 0.1500
/// mm; actual 0.1448 mm)` is `pad_to_pad`. `None` for a finding of any other rule.
fn rule_of_rules_file(description: &str) -> Option<String> {
    let named = description.split("(rule '").nth(1)?;
    let name = named
        .split('\'')
        .next()?
        .strip_prefix(netsmith_geom::DRC_RULES_FILE_PREFIX)?;
    let id = name.split(' ').next()?;
    DrcRule::from_id(id).map(|r| r.id().to_string())
}

/// Whether a rule of netsmith's is one the `.kicad_dru` that netsmith writes holds, so that
/// KiCad checks it when the file is beside the board.
pub fn in_rules_file(rule: DrcRule) -> bool {
    matches!(
        rule,
        DrcRule::PadToPad
            | DrcRule::HoleToTrack
            | DrcRule::PadHoleToHole
            | DrcRule::NpthDrill
            | DrcRule::SlotWidth
            | DrcRule::PlatedRing
    )
}

/// Takes out of KiCad's findings each `via_dangling` of a via whose copper overlaps another via
/// of its net, where netsmith's check finds the via joined, and says how many (issue #1185). KiCad
/// counts a via or a through-hole pad that a via touches on its top layer alone, so the vias of a
/// cluster side by side (the router's, for a current one via does not carry, issue #431) are
/// dangling to KiCad although their copper runs together on every layer and each carries its
/// share; netsmith's rule counts the layers the touching copper is on.
pub fn drop_dangling_vias_of_clusters(
    kicad: &mut Vec<Reported>,
    art: &netsmith_geom::Artwork,
    ours: &DrcReport,
) -> usize {
    let near = |a: Pt, b: Pt| a.dist(b) <= 1e-3;
    let in_cluster = |at: Pt| {
        art.vias.iter().any(|v| {
            near(v.at, at)
                && v.net.is_some()
                && art.vias.iter().any(|w| {
                    !near(w.at, v.at)
                        && w.net == v.net
                        && w.at.dist(v.at) < (w.diameter + v.diameter) / 2.0
                })
        })
    };
    let dangling_to_us = |at: Pt| {
        ours.findings
            .iter()
            .any(|f| f.rule == DrcRule::ViaDangling && f.items.iter().any(|i| near(i.at, at)))
    };
    let before = kicad.len();
    kicad.retain(|k| {
        !(k.kind == "via_dangling"
            && k.items
                .first()
                .is_some_and(|(_, at)| in_cluster(*at) && !dangling_to_us(*at)))
    });
    before - kicad.len()
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
    /// Findings of kinds netsmith does not check, and of KiCad's that it counts otherwise
    /// ([`drop_dangling_vias_of_clusters`]).
    pub out_of_scope: usize,
    /// Findings one check alone makes where the two measure otherwise by design (issue #1186):
    /// a silkscreen rule's pair with a text the font decides (netsmith's report lists them,
    /// `DrcReport::font_dependent`: KiCad draws KiCad's font, the package prints netsmith's),
    /// and netsmith's finding of copper or silkscreen near a circle on `Edge.Cuts` or `Margin`,
    /// which KiCad 9 finds only where the two cross.
    pub measured_otherwise: usize,
}

impl Comparison {
    /// Whether the two reports agree on everything that can be compared.
    pub fn agrees(&self) -> bool {
        self.only_netsmith.is_empty() && self.only_kicad.is_empty() && self.severity.is_empty()
    }
}

impl std::fmt::Display for Comparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.agreed == 0
            && self.agrees()
            && self.netsmith_only_rules + self.out_of_scope + self.measured_otherwise == 0
        {
            return writeln!(f, "design rules: neither check found anything");
        }
        writeln!(
            f,
            "design rules: {} finding(s) agree ({} between other ends of a missing connection), {} only netsmith's, {} only KiCad's, {} at another severity ({} of rules KiCad lacks, {} of kinds netsmith does not check, {} measured otherwise: a text in another font, a round cutout)",
            self.agreed,
            self.other_ends,
            self.only_netsmith.len(),
            self.only_kicad.len(),
            self.severity.len(),
            self.netsmith_only_rules,
            self.out_of_scope,
            self.measured_otherwise
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
/// finding may be reported. The rules netsmith writes into the `.kicad_dru` are not compared
/// (see [`compare_with_rules_file`]).
pub fn compare(ours: &DrcReport, kicad: &[Reported], tolerance: f64) -> Comparison {
    compare_rules(ours, kicad, tolerance, false)
}

/// [`compare`], with the rules of the `.kicad_dru` netsmith writes compared too: for two checks
/// that both read that file beside the board (issue #1185).
pub fn compare_with_rules_file(ours: &DrcReport, kicad: &[Reported], tolerance: f64) -> Comparison {
    compare_rules(ours, kicad, tolerance, true)
}

/// The rules KiCad's check judges against the board edge, which name one edge of several.
fn edge_rule(rule: &str) -> bool {
    matches!(rule, "copper_edge_clearance" | "silk_edge_clearance")
}

/// Whether an item, as either report names it, is a drawing of the edge.
fn is_edge(what: &str) -> bool {
    what.ends_with(" on Edge.Cuts") || what.ends_with(" on Margin")
}

/// Whether a finding of a silkscreen rule at these places is one of netsmith's report's pairs
/// that the font decides (issue #1186).
fn font_decides(ours: &DrcReport, rule: &str, places: &[Pt], tolerance: f64) -> bool {
    matches!(
        rule,
        "silk_over_copper" | "silk_overlap" | "silk_edge_clearance"
    ) && ours
        .font_dependent
        .iter()
        .any(|d| d.rule.id() == rule && same_places(places, &d.places, tolerance))
}

fn compare_rules(
    ours: &DrcReport,
    kicad: &[Reported],
    tolerance: f64,
    rules_file: bool,
) -> Comparison {
    let mut c = Comparison::default();
    let mut taken = vec![false; kicad.len()];
    let places_of = |k: &Reported| k.items.iter().map(|(_, p)| *p).collect::<Vec<_>>();
    let compared = |rule: DrcRule| !rule.is_netsmiths() || (rules_file && in_rules_file(rule));
    for f in &ours.findings {
        if !compared(f.rule) {
            c.netsmith_only_rules += 1;
            continue;
        }
        // Out of scope, among them a pour judged by its fill: each check judges its own fill.
        if OUT_OF_SCOPE.contains(&f.rule.id()) {
            c.out_of_scope += 1;
            continue;
        }
        let places: Vec<Pt> = f.items.iter().map(|i| i.at).collect();
        let found = kicad
            .iter()
            .enumerate()
            .find(|(i, k)| {
                !taken[*i]
                    && k.kind == f.rule.id()
                    && same_places(
                        &places,
                        &k.items.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
                        tolerance,
                    )
            })
            // Of several sides of the edge within the least (at a corner), KiCad names the first
            // it finds; netsmith the nearest. The other item is the finding.
            .or_else(|| {
                let off_edge = |what: &str| !is_edge(what);
                let others: Vec<Pt> = f
                    .items
                    .iter()
                    .filter(|i| off_edge(&i.what))
                    .map(|i| i.at)
                    .collect();
                (edge_rule(f.rule.id()) && others.len() < places.len())
                    .then(|| {
                        kicad.iter().enumerate().find(|(i, k)| {
                            !taken[*i]
                                && k.kind == f.rule.id()
                                && same_places(
                                    &others,
                                    &k.items
                                        .iter()
                                        .filter(|(d, _)| off_edge(d))
                                        .map(|(_, p)| *p)
                                        .collect::<Vec<_>>(),
                                    tolerance,
                                )
                        })
                    })
                    .flatten()
            });
        // KiCad's rule of the `.kicad_dru` replaced its own for these items and reports them
        // once, under netsmith's stricter rule.
        let absorbed = found.is_none()
            && kicad.iter().any(|k| {
                k.base == f.rule.id()
                    && k.kind != k.base
                    && same_places(&places, &places_of(k), tolerance)
            });
        if absorbed {
            c.agreed += 1;
            continue;
        }
        if found.is_none()
            && (font_decides(ours, f.rule.id(), &places, tolerance)
                || (edge_rule(f.rule.id())
                    && f.items
                        .iter()
                        .any(|i| i.what.starts_with("Circle on ") && is_edge(&i.what))))
        {
            c.measured_otherwise += 1;
            continue;
        }
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
        // The rule of the pads' space is a `physical_clearance` in the `.kicad_dru`, which KiCad
        // also reports of two pads that touch (a distance of 0): netsmith's report has that pair
        // as the short, `shorting_items`, alone (issue #1229).
        if k.kind == "pad_to_pad"
            && ours.findings.iter().any(|f| {
                f.rule == DrcRule::ShortingItems
                    && same_places(
                        &places_of(k),
                        &f.items.iter().map(|i| i.at).collect::<Vec<_>>(),
                        tolerance,
                    )
            })
        {
            c.measured_otherwise += 1;
            continue;
        }
        if OUT_OF_SCOPE.contains(&k.kind.as_str()) {
            c.out_of_scope += 1;
            continue;
        }
        if DrcRule::from_id(&k.kind).is_some_and(|r| !compared(r)) {
            c.netsmith_only_rules += 1;
            continue;
        }
        if font_decides(ours, &k.kind, &places_of(k), tolerance) {
            c.measured_otherwise += 1;
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
    use netsmith_geom::{DrcFinding, DrcItem as Item};

    const JSON: &str = r#"{"violations": [
      {"type": "clearance", "severity": "error", "description": "Clearance violation",
       "items": [{"description": "Track [A]", "pos": {"x": 1.0, "y": 2.0}},
                 {"description": "Track [B]", "pos": {"x": 5.0, "y": 2.0}}]},
      {"type": "lib_footprint_issues", "severity": "warning", "description": "library",
       "items": [{"description": "Footprint R1", "pos": {"x": 1.0, "y": 2.0}}]},
      {"type": "hole_clearance", "severity": "ignore", "description": "ignored", "items": []}],
     "unconnected_items": []}"#;

    fn finding(rule: DrcRule, places: &[(f64, f64)]) -> DrcFinding {
        DrcFinding {
            rule,
            severity: DrcSeverity::Error,
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
        let ours = DrcReport {
            font_dependent: Vec::new(),
            findings: vec![finding(DrcRule::Clearance, &[(5.05, 2.0), (1.0, 2.04)])],
            waived: Vec::new(),
        };
        let c = compare(&ours, &k, 0.1);
        assert_eq!((c.agreed, c.out_of_scope), (1, 1));
        assert!(c.agrees(), "{c}");
    }

    #[test]
    fn a_finding_one_side_lacks_is_named_and_a_place_off_by_more_than_the_tolerance_does_not_match()
    {
        let k = parse(JSON).unwrap();
        let ours = DrcReport {
            font_dependent: Vec::new(),
            findings: vec![
                finding(DrcRule::Clearance, &[(1.5, 2.0), (5.0, 2.0)]),
                finding(DrcRule::ShortingItems, &[(9.0, 9.0)]),
                finding(DrcRule::HoleToEdge, &[(0.0, 0.0)]),
            ],
            waived: Vec::new(),
        };
        let c = compare(&ours, &k, 0.1);
        assert_eq!(c.agreed, 0);
        assert_eq!(c.only_netsmith.len(), 2);
        assert_eq!(c.only_kicad.len(), 1);
        assert_eq!(c.netsmith_only_rules, 1);
        assert!(!c.agrees());
        assert!(c.to_string().contains("only KiCad:") && c.to_string().contains("clearance"));
    }

    /// KiCad's report of two pads 0.08 mm apart and two 0.1448 mm apart, with the rules file.
    const RULES_FILE_JSON: &str = r#"{"violations": [
      {"type": "clearance", "severity": "error",
       "description": "Clearance violation (rule 'netsmith pad_to_pad' clearance 0.1500 mm; actual 0.0800 mm)",
       "items": [{"description": "Pad 1", "pos": {"x": 1.0, "y": 2.0}},
                 {"description": "Pad 2", "pos": {"x": 1.38, "y": 2.0}}]},
      {"type": "clearance", "severity": "error",
       "description": "Clearance violation (rule 'netsmith pad_to_pad' clearance 0.1500 mm; actual 0.1448 mm)",
       "items": [{"description": "Pad 1", "pos": {"x": 5.0, "y": 2.0}},
                 {"description": "Pad 2", "pos": {"x": 5.4448, "y": 2.0}}]},
      {"type": "clearance", "severity": "error",
       "description": "Clearance violation (rule 'netsmith clearance Clearance_1mm' clearance 1.0000 mm; actual 0.5000 mm)",
       "items": [{"description": "Pad 1", "pos": {"x": 9.0, "y": 2.0}},
                 {"description": "Pad 2", "pos": {"x": 9.8, "y": 2.0}}]}],
     "unconnected_items": []}"#;

    #[test]
    fn a_finding_of_a_rule_of_the_rules_file_is_read_as_netsmiths_rule_with_kicads_kept() {
        let k = parse(RULES_FILE_JSON).unwrap();
        assert_eq!(
            k.iter()
                .map(|r| (r.kind.as_str(), r.base.as_str()))
                .collect::<Vec<_>>(),
            [
                ("pad_to_pad", "clearance"),
                ("pad_to_pad", "clearance"),
                // A net class's rule of the file is KiCad's clearance.
                ("clearance", "clearance"),
            ]
        );
        assert_eq!(
            rule_of_rules_file("Clearance violation (netclass 'Default' ...)"),
            None
        );
        assert_eq!(rule_of_rules_file("(rule 'mine' clearance 0.2 mm)"), None);
    }

    #[test]
    fn with_the_rules_file_both_checks_rules_are_compared_and_the_stricter_one_absorbs_the_projects()
     {
        let k = parse(RULES_FILE_JSON).unwrap();
        let ours = DrcReport {
            font_dependent: Vec::new(),
            findings: vec![
                finding(DrcRule::Clearance, &[(1.0, 2.0), (1.38, 2.0)]),
                finding(DrcRule::PadToPad, &[(1.0, 2.0), (1.38, 2.0)]),
                finding(DrcRule::PadToPad, &[(5.0, 2.0), (5.4448, 2.0)]),
                finding(DrcRule::Clearance, &[(9.0, 2.0), (9.8, 2.0)]),
            ],
            waived: Vec::new(),
        };
        let c = compare_with_rules_file(&ours, &k, 0.01);
        assert!(c.agrees(), "{c}");
        assert_eq!((c.agreed, c.netsmith_only_rules), (4, 0));
        // Without the file on both sides the rules are netsmith's and not compared; the clearance
        // the stricter rule replaced still agrees.
        let c = compare(&ours, &k, 0.01);
        assert!(c.agrees(), "{c}");
        assert_eq!((c.agreed, c.netsmith_only_rules), (2, 4));
        // A finding of the file's rules that one side lacks is a difference.
        let short = DrcReport {
            font_dependent: Vec::new(),
            findings: ours.findings[..2].to_vec(),
            waived: Vec::new(),
        };
        let c = compare_with_rules_file(&short, &k, 0.01);
        assert_eq!(c.only_kicad.len(), 2, "{c}");
    }

    fn reported(kind: &str, items: &[(&str, (f64, f64))]) -> Reported {
        Reported {
            kind: kind.into(),
            base: kind.into(),
            severity: DrcSeverity::Warning,
            description: "d".into(),
            items: items
                .iter()
                .map(|(d, (x, y))| (d.to_string(), Pt::new(*x, *y)))
                .collect(),
        }
    }

    fn named(rule: DrcRule, items: &[(&str, (f64, f64))]) -> DrcFinding {
        DrcFinding {
            severity: DrcSeverity::Warning,
            items: items
                .iter()
                .map(|(what, (x, y))| Item {
                    what: what.to_string(),
                    at: Pt::new(*x, *y),
                    part: None,
                    pad: None,
                    net: None,
                })
                .collect(),
            ..finding(rule, &[])
        }
    }

    #[test]
    fn a_silkscreen_finding_the_font_decides_is_measured_otherwise_and_one_it_does_not_is_a_difference()
     {
        // KiCad's font crosses a line that netsmith's text clears: the pair is in netsmith's list.
        let kicad = vec![reported(
            "silk_overlap",
            &[("PCB text 'R1'", (10.0, 10.0)), ("Segment", (10.81, 9.0))],
        )];
        let ours = DrcReport {
            findings: vec![],
            font_dependent: vec![netsmith_geom::DrcFontDependent {
                rule: DrcRule::SilkOverlap,
                places: vec![Pt::new(10.0, 10.0), Pt::new(10.81, 9.0)],
            }],
            waived: Vec::new(),
        };
        let c = compare(&ours, &kicad, 0.01);
        assert!(c.agrees() && c.measured_otherwise == 1, "{c}");
        // The same finding of another rule, or with no such pair, is a difference.
        let other = vec![reported(
            "silk_edge_clearance",
            &[("PCB text 'R1'", (10.0, 10.0)), ("Segment", (10.81, 9.0))],
        )];
        assert_eq!(compare(&ours, &other, 0.01).only_kicad.len(), 1);
        let bare = DrcReport::default();
        assert_eq!(compare(&bare, &kicad, 0.01).only_kicad.len(), 1);
        // netsmith's finding of such a pair that KiCad lacks is measured otherwise too.
        let ours = DrcReport {
            findings: vec![named(
                DrcRule::SilkOverlap,
                &[("Text 'R1'", (10.0, 10.0)), ("Segment", (10.81, 9.0))],
            )],
            ..ours
        };
        let c = compare(&ours, &[], 0.01);
        assert!(c.agrees() && c.measured_otherwise == 1, "{c}");
    }

    #[test]
    fn a_finding_at_a_round_edge_netsmith_alone_makes_is_measured_otherwise_and_a_corner_matches_by_the_other_item()
     {
        let ours = DrcReport {
            findings: vec![named(
                DrcRule::SilkEdgeClearance,
                &[
                    ("Circle on Edge.Cuts", (5.0, 5.0)),
                    ("Segment on F.SilkS", (3.0, 6.1)),
                ],
            )],
            ..DrcReport::default()
        };
        let c = compare(&ours, &[], 0.01);
        assert!(c.agrees() && c.measured_otherwise == 1, "{c}");
        // A segment of the edge is not excused.
        let segment = DrcReport {
            findings: vec![named(
                DrcRule::SilkEdgeClearance,
                &[
                    ("Segment on Edge.Cuts", (0.0, 0.0)),
                    ("Segment on F.SilkS", (0.1, 0.1)),
                ],
            )],
            ..DrcReport::default()
        };
        assert_eq!(compare(&segment, &[], 0.01).only_netsmith.len(), 1);
        // At a corner KiCad names the other side: the silkscreen is the finding.
        let kicad = vec![reported(
            "silk_edge_clearance",
            &[
                ("Segment on Edge.Cuts", (40.0, 0.0)),
                ("Segment on F.Silkscreen", (0.1, 0.1)),
            ],
        )];
        let c = compare(&segment, &kicad, 0.01);
        assert!(c.agrees() && c.agreed == 1, "{c}");
    }
}
