//! Kelvin sense connections (issue #1148): a pin that a part's `kelvin` statement names is a
//! sense pin, and the pad of the part its selector finds on the pin's net is the pad it senses
//! (the sensed pad). The pin is routed from that pad, alone: no other copper of the net joins the sense track on the way, so no
//! load current flows in it.
//!
//! ```text
//! layout {
//!     source "..."
//!     kelvin sense_p bridging .in_p .in_n to .in_p
//! }
//! ```
//!
//! [`kelvin_senses`] finds, for each such statement on a placed board, the parts the selector
//! finds and, when it finds one, the sensed pad: that part's pad on the net of the sense pin (of
//! several, the one nearest the sense pin's pads). The router routes the sense pin from it
//! (`netsmith_router`'s `kelvin` module), and [`kelvin_findings`] says when the board does not
//! keep it alone: the statement finds no part or several, or the routed copper of the sense pin
//! joins another pad of the net besides the sensed pad. The findings are guidelines of `fab`, as
//! the other statements' are, and a layout file may waive one.
//!
//! On a net with a pour, a plane or a zone, which joins every copper of its net it reaches, the
//! board file carves the pour round the sense copper ([`sense_carves`], issue #1174), so the sense
//! copper touches the rest of the net only inside the sensed pad there too.

use netsmith_core::{LayoutGuideline, LayoutRule};

use crate::board::{Board, BoardNet, CopperLayer, PadRef, Placed};
use crate::copper::Area;
use crate::geometry::{Pt, Rect};
use crate::part_layout::{LayoutFinding, Nets, selects};
use crate::ratsnest::{nodes_of, reach_without};

/// A `kelvin` statement of a part of the board, with what it finds.
#[derive(Debug, Clone, PartialEq)]
pub struct KelvinSense {
    /// The index of the part that states it, and of the statement in its [`Placed::layout`].
    pub part: usize,
    pub rule: usize,
    /// The net of the sense pin, by index of the board's nets; `None` for a pin on no net.
    pub net: Option<usize>,
    /// The numbers of the sense pin's pads.
    pub sense: Vec<String>,
    /// The parts the selector finds, by index.
    pub found: Vec<usize>,
    /// The sensed pad, when the selector finds one part with a pad on the sense pin's net.
    pub sensed: Option<SensedPad>,
}

/// The pad a sense pin is routed from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SensedPad {
    /// The index of the part, and of the pad in its footprint's pads.
    pub part: usize,
    pub pad: usize,
}

impl KelvinSense {
    /// The statement, as the part states it.
    pub fn rule<'a>(&self, parts: &'a [Placed]) -> &'a LayoutGuideline {
        &parts[self.part].layout[self.rule].rule
    }

    /// The sensed pad as a net names it: its part's designator and its pad's number.
    pub fn sensed_ref(&self, parts: &[Placed]) -> Option<PadRef> {
        let sensed = self.sensed?;
        let part = &parts[sensed.part];
        Some(PadRef {
            refdes: part.refdes.clone(),
            number: part.footprint.pads[sensed.pad].number.clone(),
        })
    }
}

/// Every `kelvin` statement of the board's parts, in the order of the parts that state them
/// and of their statements.
pub fn kelvin_senses(parts: &[Placed], nets: &[BoardNet]) -> Vec<KelvinSense> {
    if parts.iter().all(|p| p.layout.is_empty()) {
        return Vec::new();
    }
    let index = Nets::new(parts, nets);
    let mut out = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        for (r, g) in part.layout.iter().enumerate() {
            let LayoutRule::Kelvin { select, .. } = &g.rule.rule else {
                continue;
            };
            let sense = g.pads.last().cloned().unwrap_or_default();
            let net = index.of_pads(i, &sense).first().copied();
            let selector = &g.pads[..g.pads.len() - 1];
            let sides: Vec<Vec<usize>> = selector.iter().map(|p| index.of_pads(i, p)).collect();
            let found: Vec<usize> = (0..parts.len())
                .filter(|&j| j != i && selects(select, &sides, &parts[j], j, &index))
                .collect();
            let sensed = match (net, found.as_slice()) {
                (Some(net), &[j]) => {
                    let at: Vec<_> = sense
                        .iter()
                        .flat_map(|n| part.footprint.pads_numbered(n))
                        .map(|p| part.pad_centre(p))
                        .collect();
                    let other = &parts[j];
                    other
                        .footprint
                        .pads
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| index.of(j, &p.number) == Some(net))
                        .map(|(k, p)| {
                            let c = other.pad_centre(p);
                            let d = at.iter().map(|a| a.distance(c)).fold(f64::MAX, f64::min);
                            (k, d)
                        })
                        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                        .map(|(pad, _)| SensedPad { part: j, pad })
                }
                _ => None,
            };
            out.push(KelvinSense {
                part: i,
                rule: r,
                net,
                sense,
                found,
                sensed,
            });
        }
    }
    out
}

/// Whether the net joins its copper by a pour, a plane or a zone of a routes file: the board file
/// carves each round the sense copper of a Kelvin connection on the net ([`sense_carves`]).
pub fn net_is_poured(board: &Board, net: &str) -> bool {
    board.pours.iter().any(|p| p.net == net)
        || board.planes.iter().any(|p| p.net == net)
        || board.zones.iter().any(|z| z.net == net)
}

/// What the routed copper of a sense pin joins: the reach of its pads with the sensed pad taken
/// away ([`SenseCopper`]). `None` for a statement with no sensed pad or no net.
pub fn sense_copper(board: &Board, kelvin: &KelvinSense) -> Option<SenseCopper> {
    let (net, t) = (&board.nets[kelvin.net?], kelvin.sensed?);
    let parts = &board.parts;
    let nodes = nodes_of(parts, net);
    let sensing = &parts[kelvin.part];
    let sensed = &parts[t.part];
    let sensed_centre = sensed.pad_centre(&sensed.footprint.pads[t.pad]);
    let cut = nodes
        .iter()
        .position(|n| n.pad.refdes == sensed.refdes && n.centre == sensed_centre)?;
    let from: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.pad.refdes == sensing.refdes && kelvin.sense.contains(&n.pad.number))
        .map(|(k, _)| k)
        .collect();
    let (track_at, tracks): (Vec<usize>, Vec<_>) = board
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, x)| x.net == net.name)
        .unzip();
    let (via_at, vias): (Vec<usize>, Vec<_>) = board
        .vias
        .iter()
        .enumerate()
        .filter(|(_, x)| x.net == net.name)
        .unzip();
    let reach = reach_without(board.copper_count(), &nodes, (&tracks, &vias), &from, cut);
    Some(SenseCopper {
        pads: from.iter().map(|&k| nodes[k].pad.clone()).collect(),
        joins: reach.pads.iter().map(|&k| nodes[k].pad.clone()).collect(),
        tracks: reach.tracks.iter().map(|&k| track_at[k]).collect(),
        vias: reach.vias.iter().map(|&k| via_at[k]).collect(),
        reached: reach.touches_cut,
    })
}

/// The copper joined to a sense pin's pads when the sensed pad is taken away.
#[derive(Debug, Clone, PartialEq)]
pub struct SenseCopper {
    /// The sense pin's pads.
    pub pads: Vec<PadRef>,
    /// The other pads of the net it joins: none, when the pin is routed alone.
    pub joins: Vec<PadRef>,
    /// Its tracks and vias, by index of the board's.
    pub tracks: Vec<usize>,
    pub vias: Vec<usize>,
    /// Whether a track or a via of it reaches the sensed pad.
    pub reached: bool,
}

/// Where the board file keeps the pours, planes and zones of a sense pin's net off its sense
/// copper (issue #1174): a rule area that forbids pours on each layer the net is poured on, round
/// the sense copper on that layer ([`sense_carves`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SenseCarve {
    /// The sense pin's net.
    pub net: String,
    /// The sense pin, as the rule areas are named: `U1.1`.
    pub pin: String,
    /// The copper it keeps the pour off.
    pub copper: SenseCopper,
    /// Each layer the net is poured on that the sense copper has copper on, with the area the
    /// pour may not fill there: the sense copper grown by the pour's clearance.
    pub layers: Vec<(CopperLayer, Vec<Area>)>,
}

/// The carve round each sense pin's copper on a net with a pour, a plane or a zone, so that the
/// sense copper touches the rest of its net only inside the sensed pad (issue #1174).
///
/// A pour joins every copper of its net it reaches, and a sense track on its layer would be
/// joined to the rest of the net along its whole length. So the board file holds a rule area
/// that forbids pours (KiCad's keepout with `copperpour not_allowed`, which KiCad's refill keeps
/// to as netsmith's fill does) round the sense copper on each layer the net is poured on: round
/// each of its tracks on the layer and their teardrops, each of its vias (a via has copper on every layer it joins,
/// a plane's too), and the sense pin's pads with copper on the layer, each grown by the larger of
/// the pour's clearance and the net's. The pour then keeps from the sense copper the space it
/// keeps from another net's, and the sensed pad, which the area does not cover, stays joined to
/// the pour round the rest of it.
///
/// Nothing is carved for a statement with no sensed pad, or whose copper joins another pad of
/// the net besides the sensed one (the copper is not alone, and `kelvin_findings` says so): the
/// pour joins it as before. A sense pin not routed yet is carved too, round its pads, so the
/// ratsnest keeps its line to the sensed pad.
pub fn sense_carves(board: &Board) -> Vec<SenseCarve> {
    if board.pours.is_empty() && board.planes.is_empty() && board.zones.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for kelvin in kelvin_senses(&board.parts, &board.nets) {
        let Some(n) = kelvin.net else { continue };
        let net = &board.nets[n].name;
        let poured = poured_layers(board, net);
        if poured.is_empty() {
            continue;
        }
        let Some(copper) = sense_copper(board, &kelvin) else {
            continue;
        };
        if !copper.joins.is_empty() {
            continue;
        }
        let part = &board.parts[kelvin.part];
        let mut layers = Vec::new();
        for (layer, space) in poured {
            let areas = carve_on(board, net, &copper, layer, space);
            if !areas.is_empty() {
                layers.push((layer, areas));
            }
        }
        out.push(SenseCarve {
            net: net.clone(),
            pin: format!("{}.{}", part.refdes, kelvin.sense.join(".")),
            copper,
            layers,
        });
    }
    out
}

/// The layers a net is poured on (its outer pours and inner planes on layers with no zone of a
/// routes file of the net, and the layers of its zones), each with the space the pour keeps from copper of
/// other nets there, never less than the net's own clearance.
fn poured_layers(board: &Board, net: &str) -> Vec<(CopperLayer, f64)> {
    let mut out: Vec<(CopperLayer, f64)> = Vec::new();
    let mut add = |layer: CopperLayer, space: f64| match out.iter_mut().find(|(l, _)| *l == layer) {
        Some(e) => e.1 = e.1.max(space),
        None => out.push((layer, space)),
    };
    for p in &board.pours {
        if p.net == net && !board.replaced(p.layer.name(), net) {
            add(p.layer, p.clearance.unwrap_or(board.rules.plane_clearance));
        }
    }
    for p in &board.planes {
        if p.net == net
            && !board.replaced(&p.layer, net)
            && let Some(layer) = CopperLayer::of_name(&p.layer)
        {
            add(layer, board.rules.plane_clearance);
        }
    }
    for z in board.zones.iter().filter(|z| z.net == net) {
        for layer in z.layers.iter().filter_map(|l| CopperLayer::of_name(l)) {
            add(layer, z.clearance);
        }
    }
    let least = board.clearance(net);
    for e in &mut out {
        e.1 = e.1.max(least);
    }
    out
}

/// The sense copper of `net` on `layer` (its tracks and their teardrops, its vias and the sense
/// pin's pads), grown by `space` and by the error of the polygons that stand for its round ends,
/// as one set of areas.
fn carve_on(
    board: &Board,
    net: &str,
    copper: &SenseCopper,
    layer: CopperLayer,
    space: f64,
) -> Vec<Area> {
    use netsmith_geom::{Arc, Contour, Pen, Polygons, Pt as G};
    let g = |p: Pt| G { x: p.x, y: p.y };
    let grow = space + netsmith_geom::ARC_ERROR;
    let mut pen = Polygons::new();
    pen.polarity(true);
    let mut drawn = false;
    for &i in &copper.tracks {
        let t = &board.tracks[i];
        if t.layer != layer {
            continue;
        }
        let width = t.width + 2.0 * grow;
        match t.mid.and_then(|m| Arc::through(g(t.from), g(m), g(t.to))) {
            Some(arc) => pen.arc(width, None, &arc, &[]),
            None => pen.line(width, None, g(t.from), g(t.to), &[]),
        }
        drawn = true;
    }
    let layers = board.copper_count();
    for &i in &copper.vias {
        let v = &board.vias[i];
        if v.on(layer, layers) {
            pen.region(Contour::circle(g(v.at), v.diameter / 2.0 + grow), None, &[]);
            drawn = true;
        }
    }
    // A polygon and a round pen `grow` wide on each side round its edge: it grown by `grow`.
    let grown = |pen: &mut Polygons, ring: &[G]| {
        pen.region(Contour::polygon(ring), None, &[]);
        for k in 0..ring.len() {
            pen.line(2.0 * grow, None, ring[k], ring[(k + 1) % ring.len()], &[]);
        }
    };
    // The teardrops of the sense tracks: each lies on its track's end, where it meets a pad.
    for drop in board
        .teardrops
        .iter()
        .filter(|d| d.layer == layer && d.net == *net)
    {
        let on_sense = copper.tracks.iter().any(|&i| {
            let t = &board.tracks[i];
            t.layer == layer
                && drop
                    .points
                    .iter()
                    .any(|&p| t.distance_to(p) <= t.width / 2.0 + 1e-6)
        });
        if on_sense && drop.points.len() >= 3 {
            let ring: Vec<G> = drop.points.iter().map(|&p| g(p)).collect();
            grown(&mut pen, &ring);
            drawn = true;
        }
    }
    for pad_ref in &copper.pads {
        let Some(part) = board.part(&pad_ref.refdes) else {
            continue;
        };
        for pad in part.footprint.pads_numbered(&pad_ref.number) {
            let (front, back) = part.pad_layers(pad);
            let on = match layer {
                CopperLayer::Front => front,
                CopperLayer::Back => back,
                CopperLayer::Inner(_) => front && back,
            };
            if !on {
                continue;
            }
            // The box of the pad's copper in its own frame, turned with the pad and the part,
            // and a stroke `space` wide on each side round it: the pad grown by `space`.
            let half = Pt::new(pad.width / 2.0, pad.height / 2.0);
            let anchor = Rect::new(Pt::new(-half.x, -half.y), half);
            let own = pad.extent.map_or(anchor, |e| anchor.union(&e));
            let corners: Vec<G> = own
                .corners()
                .iter()
                .map(|c| g(part.to_board(pad.at + c.rotate(pad.angle))))
                .collect();
            grown(&mut pen, &corners);
            drawn = true;
        }
    }
    if !drawn {
        return Vec::new();
    }
    crate::copper::areas(&pen)
}

/// A sentence for each `kelvin` statement the board does not keep: no part found, several, a
/// net that is poured, or routed copper that joins the sense pin to more than the sensed pad. A board
/// with no copper is judged on what it finds alone.
pub fn kelvin_findings(board: &Board) -> Vec<LayoutFinding> {
    let parts = &board.parts;
    let routed = !board.tracks.is_empty() || !board.vias.is_empty();
    let mut out = Vec::new();
    for kelvin in kelvin_senses(parts, &board.nets) {
        let Some(n) = kelvin.net else { continue };
        let g = kelvin.rule(parts);
        let LayoutRule::Kelvin { to, .. } = &g.rule else {
            continue;
        };
        let part = &parts[kelvin.part];
        let net = &board.nets[n].name;
        let head = format!(
            "`{}` ({}) asks for `{}`: its pin `.{to}` (pads {}) routed alone (a Kelvin connection)",
            part.path,
            part.refdes,
            g.name,
            kelvin.sense.join(", ")
        );
        let tail = format!("({}; {})", g.source, g.site);
        let mut refdes = vec![part.refdes.clone()];
        let from = kelvin
            .sensed_ref(parts)
            .map(|t| t.to_string())
            .unwrap_or_else(|| "the pad it senses on the part it finds".to_string());
        let head = format!("{head} from {from}");
        let message = if kelvin.found.is_empty() {
            format!(
                "{head}, and it finds no part on net `{net}`, so the pin senses no pad {tail}: join the part it senses (the shunt) to the pin's net"
            )
        } else if kelvin.found.len() > 1 {
            let names: Vec<&str> = kelvin
                .found
                .iter()
                .map(|&j| parts[j].refdes.as_str())
                .collect();
            refdes.extend(names.iter().map(|s| s.to_string()));
            format!(
                "{head}, and it finds {} parts ({}), so the pad it senses is not one pad {tail}: leave one part on net `{net}` that the statement finds",
                kelvin.found.len(),
                names.join(", ")
            )
        } else if kelvin.sensed.is_none() || !routed {
            continue;
        } else {
            let Some(copper) = sense_copper(board, &kelvin) else {
                continue;
            };
            refdes.push(parts[kelvin.found[0]].refdes.clone());
            // A pour of the net is carved round copper that is alone, and joins the rest.
            let poured = if net_is_poured(board, net) {
                format!(", and the pour of net `{net}` joins it too")
            } else {
                String::new()
            };
            if !copper.joins.is_empty() {
                let pads: Vec<String> = copper.joins.iter().map(ToString::to_string).collect();
                format!(
                    "{head}, and its copper ({} tracks, {} vias) joins {} besides the pad it senses, so load current may flow in it{poured} {tail}: give the router room between the pin and the other pads of net `{net}`, or route the sense track by hand",
                    copper.tracks.len(),
                    copper.vias.len(),
                    pads.join(", ")
                )
            } else if (!copper.tracks.is_empty() || !copper.vias.is_empty()) && !copper.reached {
                format!(
                    "{head}, and its copper ({} tracks, {} vias) does not reach the pad it senses {tail}: route the sense track to {from}",
                    copper.tracks.len(),
                    copper.vias.len()
                )
            } else {
                continue;
            }
        };
        out.push(LayoutFinding {
            part: g.part.clone(),
            name: g.name.clone(),
            message,
            refdes,
        });
    }
    out
}
