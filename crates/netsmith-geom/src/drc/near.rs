//! `near_rule_area` (issue #1591): the parts, tracks, vias and pours that stand near a rule area
//! that forbids copper or parts, a module's antenna keep-out above all, where copper and parts can
//! detune the antenna. The check does not judge them: each is a note, which fails nothing, with
//! any strictness, and the person decides.
//!
//! An area is a keep-out zone of the board or of a footprint (the `rule_areas` of `board rules`)
//! that forbids anything (`tracks`, `vias`, `pads`, `copperpour`, `footprints`). What is listed
//! is what lies within [`super::Extras::rule_area_reach`] of its outline, measured in the plane
//! of the board, edge to edge, on any copper layer: under it on a layer it is not on (the
//! opposite face, an inner layer), inside it on a layer where it does not forbid that kind, and
//! beside it. What [`super::Rule::ItemsNotAllowed`] already flags (inside, on a layer of the
//! area, of a kind it forbids) is not listed again. A footprint's own area leaves the footprint
//! and its pads out: the module's own copper is its maker's to judge.
//!
//! A part is its courtyard's box (else its pads' box), one note for the part; a via is one
//! note each; the tracks of one net on one layer are one note, at the nearest; a pour is one
//! note for each layer, by the fill the file holds or else its outline (no fill is worked out
//! here, so a pour the fill keeps off an area that forbids pours is listed at the area's edge).

use super::checks::{EPS, Out, area_name, courtyard_box};
use super::model::Kind;
use super::shape::{Shape, gap, within_hole};
use super::{Finding, Item, Rule, Severity};
use crate::geom::{Bounds, Pt};
use crate::polygons::Area;

pub(crate) fn run(out: &mut Out<'_>) {
    let m = out.m;
    let Some(reach) = m.extras.rule_area_reach else {
        return;
    };
    if reach < 0.0 || m.rules.severity(Rule::NearRuleArea).is_none() {
        return;
    }
    let mut notes: Vec<Finding> = Vec::new();
    for z in &m.art.zones {
        let Some(k) = &z.keepout else { continue };
        let forbids: Vec<&str> = [
            (k.tracks, "tracks"),
            (k.vias, "vias"),
            (k.pads, "pads"),
            (k.copperpour, "copperpour"),
            (k.footprints, "footprints"),
        ]
        .into_iter()
        .filter_map(|(f, w)| f.then_some(w))
        .collect();
        if forbids.is_empty() {
            continue;
        }
        let areas = crate::fill::outline_area(z);
        let Some(bounds) = Bounds::of(z.outlines.iter().flatten().copied()) else {
            continue;
        };
        let near = bounds.expanded(reach + EPS);
        let owner = (!z.owner.is_empty()).then_some(z.owner.as_str());
        let on: Vec<usize> = z
            .layers
            .iter()
            .filter_map(|l| m.layers.iter().position(|x| x == l))
            .collect();
        let about = format!(
            "the rule area {} (no {} on {})",
            area_name(z),
            forbids.join(", "),
            z.layers.join(", ")
        );
        let mut note = |what: &str,
                        d: f64,
                        on_area: bool,
                        layer: &str,
                        at: Pt,
                        items: Vec<Item>| {
            let place = if d > 0.0 {
                format!("{} mm from {about}", mm(d))
            } else if on_area {
                format!("inside {about}")
            } else {
                format!("under {about}, on {layer}, a layer it is not on")
            };
            notes.push(Finding {
                rule: Rule::NearRuleArea,
                severity: Severity::Note,
                message: format!(
                    "{what} is {place}; within {} mm of the area copper and parts can detune an antenna: move it or keep it",
                    mm(reach)
                ),
                at,
                layer: Some(layer.to_string()),
                items,
                carrying: None,
                source: None,
                distance: None,
            });
        };

        // The parts, by their courtyard's box, else their pads'.
        for fp in m
            .art
            .footprints
            .iter()
            .filter(|f| Some(f.part.as_str()) != owner)
        {
            let side = if fp.back { "B.Cu" } else { "F.Cu" };
            let yard = courtyard_box(m.art, fp).or_else(|| {
                let mut pads = m
                    .items
                    .iter()
                    .filter(|c| {
                        matches!(c.kind, Kind::Pad(_)) && c.item.part.as_deref() == Some(&fp.part)
                    })
                    .map(|c| c.bounds);
                let first = pads.next()?;
                Some(pads.fold(first, |mut b, p| {
                    b.add(p.min);
                    b.add(p.max);
                    b
                }))
            });
            let Some(yard) = yard.filter(|y| y.overlaps(&near)) else {
                continue;
            };
            let corners = vec![
                yard.min,
                Pt::new(yard.max.x, yard.min.y),
                yard.max,
                Pt::new(yard.min.x, yard.max.y),
            ];
            let Some((d, _)) = distance(&Shape::polygon(corners, 0.0), &areas) else {
                continue;
            };
            let on_area = z.layers.iter().any(|l| l == side);
            if d > reach + EPS || (d == 0.0 && on_area && k.footprints) {
                continue;
            }
            let item = Item {
                what: format!("Footprint {}", fp.part),
                at: fp.at,
                part: Some(fp.part.clone()),
                pad: None,
                net: None,
            };
            note(
                &format!("Part {} on {side}", fp.part),
                d,
                on_area,
                side,
                fp.at,
                vec![item],
            );
        }

        // The vias one by one, the tracks of a net on a layer together, at the nearest.
        let mut tracks: Vec<(Option<String>, usize, f64, usize, usize)> = Vec::new();
        for (i, c) in m.items.iter().enumerate() {
            let (forbidden, via) = match c.kind {
                Kind::Via(_) => (k.vias, true),
                Kind::Track(_) | Kind::Teardrop(_) => (k.tracks, false),
                Kind::Pad(_) | Kind::Graphic(_) => continue,
            };
            if !c.bounds.overlaps(&near) {
                continue;
            }
            let Some(d) = c
                .shapes
                .iter()
                .filter_map(|s| distance(s, &areas))
                .map(|(d, _)| d)
                .min_by(f64::total_cmp)
            else {
                continue;
            };
            let on_area = c.layers.iter().any(|l| on.contains(l));
            if d > reach + EPS || (d == 0.0 && on_area && forbidden) {
                continue;
            }
            let layer = c
                .layers
                .iter()
                .copied()
                .find(|l| on.contains(l))
                .or_else(|| c.layers.first().copied())
                .unwrap_or(0);
            if via {
                note(
                    &c.item.what,
                    d,
                    on_area,
                    m.layer_name(layer),
                    c.item.at,
                    vec![c.item.clone()],
                );
                continue;
            }
            match tracks.iter_mut().find(|t| t.0 == c.net && t.1 == layer) {
                Some(t) => {
                    t.4 += 1;
                    if d < t.2 {
                        t.2 = d;
                        t.3 = i;
                    }
                }
                None => tracks.push((c.net.clone(), layer, d, i, 1)),
            }
        }
        for (_, layer, d, i, count) in tracks {
            let c = &m.items[i];
            let what = match count {
                1 => c.item.what.clone(),
                n => format!("{} (the nearest of {n} on its net and layer)", c.item.what),
            };
            note(
                &what,
                d,
                on.contains(&layer),
                m.layer_name(layer),
                c.item.at,
                vec![c.item.clone()],
            );
        }

        // The pours, one note for each layer.
        for p in &m.pours {
            let Some((d, at)) = p
                .polygons
                .iter()
                .filter(|poly| poly.len() >= 3)
                .filter_map(|poly| distance(&Shape::polygon(poly.clone(), 0.0), &areas))
                .min_by(|a, b| a.0.total_cmp(&b.0))
            else {
                continue;
            };
            if d > reach + EPS {
                continue;
            }
            let layer = m.layer_name(p.layer);
            let net = p.net.clone().unwrap_or_default();
            let what = format!("Zone [{net}] on {layer}");
            let item = Item {
                what: what.clone(),
                at,
                part: None,
                pad: None,
                net: p.net.clone(),
            };
            note(
                &format!("A pour of {net} on {layer}"),
                d,
                on.contains(&p.layer),
                layer,
                at,
                vec![item],
            );
        }
    }
    out.findings.extend(notes);
}

/// The least distance in the plane from `s` to any of `areas`, edge to edge, and the point of
/// `s` nearest to it: 0 when `s` reaches into one. A shape wholly inside a hole of an area (a
/// cutout) is measured to the hole's edge.
fn distance(s: &Shape, areas: &[Area]) -> Option<(f64, Pt)> {
    areas
        .iter()
        .filter(|a| a.outer.len() >= 3)
        .map(|a| {
            let g = match a.holes.iter().find(|h| within_hole(s, h)) {
                Some(h) => gap(s, &Shape::ring(h.clone(), 0.0)),
                None => gap(s, &Shape::polygon(a.outer.clone(), 0.0)),
            };
            (g.d, g.a)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

/// A length in mm with at most two decimals and no trailing zeros: `1`, `0.85`.
fn mm(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
