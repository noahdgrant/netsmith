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
//! keep it alone: the statement finds no part or several, the net has a pour, a plane or a zone
//! (which joins every copper of the net it reaches), or the routed copper of the sense pin joins
//! another pad of the net besides the sensed pad. The findings are guidelines of `fab`, as the other
//! statements' are, and a layout file may waive one.

use netsmith_core::{LayoutGuideline, LayoutRule};

use crate::board::{Board, BoardNet, PadRef, Placed};
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

/// Whether the net joins its copper by a pour, a plane or a zone of a routes file: a sense
/// track of it would be joined to the rest of the net wherever the pour reaches it.
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
    let tracks: Vec<_> = board.tracks.iter().filter(|x| x.net == net.name).collect();
    let vias: Vec<_> = board.vias.iter().filter(|x| x.net == net.name).collect();
    let reach = reach_without(board.copper_count(), &nodes, (&tracks, &vias), &from, cut);
    Some(SenseCopper {
        joins: reach.pads.iter().map(|&k| nodes[k].pad.clone()).collect(),
        tracks: reach.tracks,
        vias: reach.vias,
        reached: reach.touches_cut,
    })
}

/// The copper joined to a sense pin's pads when the sensed pad is taken away.
#[derive(Debug, Clone, PartialEq)]
pub struct SenseCopper {
    /// The other pads of the net it joins: none, when the pin is routed alone.
    pub joins: Vec<PadRef>,
    /// Its tracks and vias.
    pub tracks: usize,
    pub vias: usize,
    /// Whether a track or a via of it reaches the sensed pad.
    pub reached: bool,
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
        } else if kelvin.sensed.is_none() {
            continue;
        } else if net_is_poured(board, net) {
            refdes.push(parts[kelvin.found[0]].refdes.clone());
            format!(
                "{head}, and net `{net}` has a pour, a plane or a zone, which joins every copper of the net it reaches, so the pin cannot be routed alone {tail}: take the pour off the net, or route the sense track by hand and waive the guideline in the layout file"
            )
        } else if !routed {
            continue;
        } else {
            let Some(copper) = sense_copper(board, &kelvin) else {
                continue;
            };
            refdes.push(parts[kelvin.found[0]].refdes.clone());
            if !copper.joins.is_empty() {
                let pads: Vec<String> = copper.joins.iter().map(ToString::to_string).collect();
                format!(
                    "{head}, and its copper ({} tracks, {} vias) joins {} besides the pad it senses, so load current may flow in it {tail}: give the router room between the pin and the other pads of net `{net}`, or route the sense track by hand",
                    copper.tracks,
                    copper.vias,
                    pads.join(", ")
                )
            } else if (copper.tracks > 0 || copper.vias > 0) && !copper.reached {
                format!(
                    "{head}, and its copper ({} tracks, {} vias) does not reach the pad it senses {tail}: route the sense track to {from}",
                    copper.tracks, copper.vias
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
