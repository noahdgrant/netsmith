//! `v_cut_edge` (issue 1592): the copper of a board ordered as a panel against the space the board
//! house keeps between copper and a V-cut edge.
//!
//! A board under the board house's assembly size (`assembly_size`, issue 1285) is ordered as a
//! panel, and the board's outline becomes the edge the panel is cut apart along. JLCPCB's
//! capabilities page asks "Copper clearance from V-cut board edges: ≧0.4 mm", where an edge it
//! routes asks 0.2 mm; its blog on V-cut panels measures the same 0.4 mm to the cut's centre
//! line and recommends 1.0 mm. Neither page says pours apart from tracks: "the copper layer,
//! traces/solder pads, etc." (`data/jlcpcb/README.md`). So every copper of the board counts:
//! tracks, pads, vias, teardrops, drawings on copper and the fill of each pour, as `fab` plots it
//! (the fill the board file holds, else the fill [`crate::fill`] works out), against the outer
//! outline only (a cutout is not cut by a V-cut).
//!
//! **A note** (`Severity::Note`: it fails nothing, with any strictness). The check judges one board and does not know how the panel will
//! be cut: a panel with mouse bites, or the Economic service, which makes no V-cut, keeps the
//! board's own edge clearance. So copper nearer than the figure is told what a V-cut asks and is
//! not refused. A panel the file declares (`VCutPanel`) holds the board's edge clearance to the
//! panel's figure itself, an error (`netsmith_fab::panel`). The measure is in
//! [`super::Report::v_cut`] for the notes, with or without a figure: a process that states an
//! assembly size and no V-cut figure has the distance said and not judged.

use super::checks::EPS;
use super::marks::under_assembly_size;
use super::model::Model;
use super::shape::{Shape, gap};
use super::{EdgeCopper, Finding, Rule, VCut};
use crate::fill;
use crate::plot;

/// The board's V-cut measure, if it is under the assembly size of its process.
pub(crate) fn of(m: &Model<'_>) -> Option<VCut> {
    let marks = m.extras.marks.as_ref()?;
    let under = under_assembly_size(m, marks)?;
    Some(VCut {
        width: under.width,
        height: under.height,
        assembly_size: under.least,
        nearest: nearest(m),
        least: marks.min_v_cut_edge,
        source: marks.v_cut_edge_source.clone().filter(|s| !s.is_empty()),
    })
}

/// The `v_cut_edge` finding of `v`: copper nearer the outline than the figure.
pub(crate) fn finding(m: &Model<'_>, v: &VCut) -> Option<Finding> {
    let severity = m.rules.severity(Rule::VCutEdge)?;
    let least = v.least?;
    let near = v.nearest.as_ref()?;
    if near.distance >= least - EPS {
        return None;
    }
    let from = v
        .source
        .as_deref()
        .map(|s| format!(" (source: {s})"))
        .unwrap_or_default();
    Some(Finding {
        rule: Rule::VCutEdge,
        severity,
        message: format!(
            "{} is {:.3} mm from the board edge; the board is {:.1} x {:.1} mm, under the {:.0} mm the board house assembles as a single board, so it is ordered as a panel, and the board house keeps copper {} mm from a V-cut edge{from}: keep copper that far from the edge (`.process.min_edge_clearance &= {}mm..`), or order the panel cut apart without V-cuts",
            near.what,
            near.distance,
            v.width,
            v.height,
            v.assembly_size,
            number(least),
            number(least),
        ),
        at: near.at,
        layer: near.layer.clone(),
        items: Vec::new(),
        carrying: None,
        source: v.source.clone(),
        distance: Some(super::Distance {
            required: least,
            measured: near.distance,
        }),
    })
}

/// A length in mm as the source states it: `0.4`, not `0.400`.
fn number(x: f64) -> String {
    let s = format!("{x:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The copper of the board nearest its outer outline; `None` for a board with no copper or no
/// closed outline.
pub(crate) fn nearest(m: &Model<'_>) -> Option<EdgeCopper> {
    let edge = crate::edge::outline(m.art).ok()?;
    if edge.outer.len() < 3 {
        return None;
    }
    let ring = Shape::ring(edge.outer.clone(), 0.0);
    let mut best: Option<EdgeCopper> = None;
    let mut keep = |distance: f64, what: &dyn Fn() -> String, at, layer: Option<String>| {
        if best.as_ref().is_none_or(|b| distance < b.distance - 1e-9) {
            best = Some(EdgeCopper {
                distance,
                what: what(),
                at,
                layer,
            });
        }
    };
    // Every copper item, a drawing of no net on copper among them: the board house etches it too.
    for c in &m.items {
        for s in &c.shapes {
            let g = gap(s, &ring);
            let layer = c.layers.first().map(|l| m.layer_name(*l).to_string());
            keep(g.d, &|| c.item.what.clone(), g.middle(), layer);
        }
    }
    for (layer, zone, areas) in fills(m, &edge) {
        let z = &m.art.zones[zone];
        let net = z.net.clone().unwrap_or_default();
        for a in areas {
            if a.len() < 3 {
                continue;
            }
            let g = gap(&Shape::ring(a, 0.0), &ring);
            let what = || {
                if z.name.is_empty() {
                    format!("Zone [{net}] on {layer}")
                } else {
                    format!("Zone {} [{net}] on {layer}", z.name)
                }
            };
            keep(g.d, &what, g.middle(), Some(layer.clone()));
        }
    }
    best
}

/// The outer ring of each piece of the fill of each pour with a net, by layer and the zone's
/// index: the fill the board file holds, else the one [`fill::zone_fills`] works out, as `fab`
/// plots it. A layer whose zones cannot be filled (two that overlap with one priority, which
/// `fab` refuses) has its outlines measured instead.
fn fills(
    m: &Model<'_>,
    edge: &crate::edge::Outline,
) -> Vec<(String, usize, Vec<Vec<crate::geom::Pt>>)> {
    let rules = plot::Rules {
        edge_clearance: m.rules.copper_edge,
        hole_clearance: Some(m.rules.hole_clearance),
        classes: m
            .rules
            .classes
            .iter()
            .filter(|c| c.clearance.is_some())
            .cloned()
            .collect(),
    };
    let mut out = Vec::new();
    for layer in &m.layers {
        let poured = |z: &crate::artwork::Zone| {
            z.keepout.is_none() && z.net.is_some() && !z.teardrop && z.layers.contains(layer)
        };
        if !m.art.zones.iter().any(poured) {
            continue;
        }
        let worked = fill::zone_fills(m.art, layer, edge, &rules).ok();
        for (i, z) in m.art.zones.iter().enumerate().filter(|(_, z)| poured(z)) {
            let held: Vec<Vec<crate::geom::Pt>> = z
                .filled
                .iter()
                .filter(|(l, _)| l == layer)
                .map(|(_, p)| p.clone())
                .collect();
            let rings = if !held.is_empty() {
                held
            } else {
                match &worked {
                    Some(w) => w
                        .iter()
                        .filter(|(zone, _)| *zone == i)
                        .flat_map(|(_, areas)| areas.iter().map(|a| a.outer.clone()))
                        .collect(),
                    None => z.outlines.clone(),
                }
            };
            out.push((layer.clone(), i, rings));
        }
    }
    out
}
