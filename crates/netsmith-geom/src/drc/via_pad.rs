//! `via_hole_near_pad` and `via_ring_near_pad` (issue #1579): the vias whose hole, or whose ring,
//! comes near a pad's copper. A hole on or near a pad's edge can open in the pad's solder mask
//! opening and draw solder down it; the board house's pages give no distance for it, so the check
//! does not judge it: each is a note, which fails nothing, with any strictness, and the person
//! decides. The router is not held to it.
//!
//! A pad of any net counts, on any copper layer the via is on. The distance is measured in the
//! plane of the board from the pad's copper to the hole's circle (`drill / 2` from the via's
//! centre) or the ring's (`diameter / 2`), edge to edge, negative where it reaches into the pad.
//! A hole is noted when that is less than [`super::Extras::via_hole_pad_reach`] (0 notes only
//! a hole that crosses a pad's edge, the vias `fab-notes.txt` named before issue #1579), a ring
//! when it is less than [`super::Extras::via_ring_pad_reach`] and the hole is not noted. One note
//! for each via, naming every pad it comes near, the nearest first.
//!
//! A via whose centre is on a pad's copper is a via in a pad (`via_in_pad`, and `fab-notes.txt`'s
//! line of them): the board house fills it, so it is not noted here. Nor is a laser-drilled
//! micro via of an HDI process, which the process fills by plating.

use super::checks::{EPS, Out};
use super::model::{Copper, Kind};
use super::shape::{Shape, gap};
use super::{Finding, Rule, Severity};
use crate::geom::Bounds;

/// How near a via's centre must be to a pad's copper to stand on it, in mm: on its edge counts.
const ON_PAD: f64 = 1e-6;

pub(crate) fn run(out: &mut Out<'_>) {
    let m = out.m;
    let reach =
        |rule: Rule, of: Option<f64>| of.filter(|r| *r >= 0.0 && m.rules.severity(rule).is_some());
    let hole_reach = reach(Rule::ViaHoleNearPad, m.extras.via_hole_pad_reach);
    let ring_reach = reach(Rule::ViaRingNearPad, m.extras.via_ring_pad_reach);
    if hole_reach.is_none() && ring_reach.is_none() {
        return;
    }
    let pads: Vec<&Copper> = m
        .items
        .iter()
        .filter(|c| matches!(c.kind, Kind::Pad(_)))
        .collect();
    let mut notes: Vec<Finding> = Vec::new();
    for (vi, v) in m.art.vias.iter().enumerate() {
        // An HDI process fills and planarizes its laser-drilled blind vias by plating (issue
        // #1071), as `via_in_pad` takes them.
        if v.micro && m.extras.vias.is_some() {
            continue;
        }
        let via = &m.items[m.via_item[vi]];
        let (hole_r, ring_r) = (v.drill / 2.0, v.diameter / 2.0);
        let look = (hole_r + hole_reach.unwrap_or(0.0)).max(ring_r + ring_reach.unwrap_or(0.0));
        let here = Bounds {
            min: v.at,
            max: v.at,
        }
        .expanded(look + EPS);
        let centre = Shape::disc(v.at, 0.0);
        // Each pad on a layer of the via near enough to look at, with its distance from the
        // via's centre and the first layer they share.
        let mut near: Vec<(f64, &Copper, usize)> = Vec::new();
        let mut on_pad = false;
        for c in &pads {
            let Some(&layer) = c.layers.iter().find(|l| via.layers.contains(l)) else {
                continue;
            };
            if !c.bounds.expanded(EPS).overlaps(&here) {
                continue;
            }
            let Some(d) = c
                .shapes
                .iter()
                .map(|s| gap(s, &centre).d)
                .min_by(f64::total_cmp)
            else {
                continue;
            };
            if d <= ON_PAD {
                on_pad = true;
                break;
            }
            near.push((d, c, layer));
        }
        if on_pad {
            continue;
        }
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut note = |rule: Rule, what: &str, r: f64, limit: f64| {
            let close: Vec<&(f64, &Copper, usize)> = near
                .iter()
                .filter(|(d, _, _)| d - r < limit - ON_PAD)
                .collect();
            let Some(&&(d, nearest, layer)) = close.first() else {
                return false;
            };
            let g = d - r;
            let place = if g < -ON_PAD {
                format!("reaches {} mm into", mm(-g))
            } else if g <= ON_PAD {
                "touches".to_string()
            } else {
                format!("is {} mm from", mm(g))
            };
            let more = match close.len() {
                1 => String::new(),
                2 => " (and 1 more pad)".to_string(),
                n => format!(" (and {} more pads)", n - 1),
            };
            let mut items = vec![via.item.clone()];
            items.extend(close.iter().map(|(_, c, _)| c.item.clone()));
            notes.push(Finding {
                rule,
                severity: Severity::Note,
                message: format!(
                    "the {} mm {what} of {} {place} {}{more}; {} (the process's `.via_{what}_pad_reach`, {} mm, a figure of netsmith's, not the board house's) a via's {what} can open in the pad's mask opening and draw solder: move the via off the pad, or keep it",
                    mm(2.0 * r),
                    via.item.what,
                    nearest.item.what,
                    if limit > 0.0 {
                        format!("within {} mm of a pad's copper", mm(limit))
                    } else {
                        "on a pad's copper".to_string()
                    },
                    mm(limit),
                ),
                at: v.at,
                layer: Some(m.layer_name(layer).to_string()),
                items,
                carrying: None,
                source: None,
            });
            true
        };
        let hole =
            hole_reach.is_some_and(|limit| note(Rule::ViaHoleNearPad, "hole", hole_r, limit));
        if !hole && let Some(limit) = ring_reach {
            note(Rule::ViaRingNearPad, "ring", ring_r, limit);
        }
    }
    out.findings.extend(notes);
}

/// A length in mm with at most three decimals and no trailing zeros: `0.05`, `0.2`.
fn mm(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
