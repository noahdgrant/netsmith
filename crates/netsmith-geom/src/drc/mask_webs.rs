//! One line for a part whose own pads are too close for any solder mask web (issue #1685).
//!
//! A fine-pitch part (a 0.4 mm pitch QFN, a connector of narrow pins) has neighbouring pads
//! nearer than the least web the process makes (`.min_mask_bridge`), so the board house leaves
//! no mask between them and makes one opening over the row: a gang opening, the only outcome.
//! `solder_mask_bridge` reports each such pair, a finding for every neighbour on every run, and a
//! real finding is lost among them. With [`super::Extras::mask_web_summary`] (`fab`'s extras, so
//! `board fab`, `board drc` and `board check-footprint`) these pairs become one finding per part
//! and mask layer.
//!
//! **When the process cannot make a web.** A pair of a part's own pads (both items the part's)
//! is a breach of `solder_mask_bridge` and the pads' copper, edge to edge, is nearer than the
//! least web plus the board's own mask margin on each side (`web + 2 × margin`, KiCad's
//! `pad_to_mask_clearance`, 0 on a board netsmith writes): even an opening the board's size round
//! each pad leaves less mask than the process makes, and no footprint setting short of a
//! solder-mask-defined pad changes that. A pair whose copper leaves room for the web, so that
//! a larger margin of the footprint or a pad is what narrows it, is the footprint's choice and
//! can be fixed.
//!
//! **When a part is summarised.** Every breach pair of the part's own pads, on either mask
//! layer, is one the process cannot make, and there are at least two on the layer. A part with
//! one pair it could make keeps every finding pair by pair, and so does a part with a single
//! pair. A web between pads of two parts is the board's and is never summarised; a note within
//! a probe's margin ([`super::Extras::margin`]) is not either.
//!
//! **The finding.** The rule `solder_mask_bridge`, the severity of the pairs it replaces (the
//! highest of them), the place of the narrowest pair, the layer, and as its items each pad of the
//! pairs once (so a JSON reader lists the pads); its distance is the web and the narrowest
//! opening. The message gives the count of pairs and the spacing of their openings. It is within
//! the part's own footprint ([`super::Finding::own_part`]).
//!
//! **Waivers and strictness.** The layout file's waivers are applied first, to each pair as
//! before, so a waiver written for some pairs still covers them; the pairs it leaves are
//! summarised. A waiver of the rule for the part with no pads covers every pair, and nothing is
//! left to summarise. The summary has the severity of the findings it replaces, so a run that
//! fails on a warning fails on it as it failed on them; only the count of findings is smaller.
//! KiCad's check, and netsmith's in the cross-check, report pair by pair: the cross-check runs
//! without the summary.

use super::model::{Model, outline_shapes};
use super::shape::gap;
use super::{Distance, Finding, Item, Report, Rule};
use crate::pads;

/// Replaces, in `report.findings`, the `solder_mask_bridge` pairs of each part whose every pair is
/// one the process cannot make by one finding for the part and layer.
pub(super) fn summarise(m: &Model<'_>, report: &mut Report) {
    let web = m.rules.mask_web;
    if web <= 0.0 {
        return;
    }
    let needed = web + 2.0 * m.art.mask_margin;
    let candidate = |f: &Finding| {
        f.rule == Rule::SolderMaskBridge
            && f.severity != super::Severity::Note
            && f.items.len() == 2
            && f.own_part().is_some()
    };
    // For each part: whether every pair is unmakeable.
    let mut verdicts: Vec<(String, bool)> = Vec::new();
    for f in report.findings.iter().filter(|f| candidate(f)) {
        let part = f.own_part().unwrap_or_default().to_string();
        let unmakeable = copper_gap(m, &f.items).is_some_and(|d| d < needed - 1e-6);
        match verdicts.iter_mut().find(|(p, _)| *p == part) {
            Some(v) => v.1 &= unmakeable,
            None => verdicts.push((part, unmakeable)),
        }
    }
    let summarised = |f: &Finding| {
        candidate(f)
            && verdicts
                .iter()
                .any(|(p, all)| *all && f.own_part() == Some(p.as_str()))
    };
    // The groups, by part and layer, with at least two pairs.
    let mut groups: Vec<(String, Option<String>, Vec<usize>)> = Vec::new();
    for (i, f) in report.findings.iter().enumerate() {
        if !summarised(f) {
            continue;
        }
        let part = f.own_part().unwrap_or_default().to_string();
        match groups
            .iter_mut()
            .find(|(p, l, _)| *p == part && *l == f.layer)
        {
            Some(g) => g.2.push(i),
            None => groups.push((part, f.layer.clone(), vec![i])),
        }
    }
    groups.retain(|g| g.2.len() >= 2);
    if groups.is_empty() {
        return;
    }
    let mut replaced = vec![None; report.findings.len()];
    for (part, layer, members) in &groups {
        let summary = summary_of(part, layer.as_deref(), web, members, &report.findings);
        replaced[members[0]] = Some(Some(summary));
        for &i in &members[1..] {
            replaced[i] = Some(None);
        }
    }
    let old = std::mem::take(&mut report.findings);
    for (f, r) in old.into_iter().zip(replaced) {
        match r {
            None => report.findings.push(f),
            Some(Some(s)) => report.findings.push(s),
            Some(None) => {}
        }
    }
}

/// The space between the copper of the two pads `items` names, edge to edge.
fn copper_gap(m: &Model<'_>, items: &[Item]) -> Option<f64> {
    let pad = |i: &Item| {
        m.art.pads.iter().find(|p| {
            Some(&p.part) == i.part.as_ref()
                && Some(&p.number) == i.pad.as_ref()
                && p.at.dist(i.at) < 1e-6
        })
    };
    let (a, b) = (pad(&items[0])?, pad(&items[1])?);
    let (sa, sb) = (
        outline_shapes(&pads::outline(a)),
        outline_shapes(&pads::outline(b)),
    );
    sa.iter()
        .flat_map(|x| sb.iter().map(move |y| gap(x, y).d))
        .min_by(f64::total_cmp)
}

/// The one finding for `part` on `layer` in place of the pairs `members` of `findings`.
fn summary_of(
    part: &str,
    layer: Option<&str>,
    web: f64,
    members: &[usize],
    findings: &[Finding],
) -> Finding {
    let pairs: Vec<&Finding> = members.iter().map(|&i| &findings[i]).collect();
    let narrowest = pairs
        .iter()
        .min_by(|a, b| measured(a).total_cmp(&measured(b)))
        .expect("a group has pairs");
    let mut spacings: Vec<String> = pairs
        .iter()
        .map(|f| format!("{:.4} mm", measured(f)))
        .collect();
    spacings.sort();
    spacings.dedup();
    let mut items: Vec<Item> = Vec::new();
    for i in pairs.iter().flat_map(|f| f.items.iter()) {
        if !items
            .iter()
            .any(|k| k.pad == i.pad && k.at.dist(i.at) < 1e-6)
        {
            items.push(i.clone());
        }
    }
    let spacing = match spacings.as_slice() {
        [one] => format!("{one} apart"),
        many => format!("{} apart", many.join(", ")),
    };
    Finding {
        rule: Rule::SolderMaskBridge,
        severity: pairs
            .iter()
            .map(|f| f.severity)
            .max()
            .unwrap_or(narrowest.severity),
        message: format!(
            "{} pairs of {part}'s own pads have solder mask openings {spacing}, under the process's least web of {web:.4} mm; their copper is nearer than the web, so the board house leaves no mask between them and makes one opening over them (a gang opening)",
            pairs.len()
        ),
        at: narrowest.at,
        layer: layer.map(str::to_string),
        items,
        carrying: None,
        source: None,
        distance: Some(Distance {
            required: web,
            measured: measured(narrowest),
        }),
    }
}

fn measured(f: &Finding) -> f64 {
    f.distance.map_or(0.0, |d| d.measured)
}
