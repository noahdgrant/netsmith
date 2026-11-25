//! The power stage of each switching regulator of a board (issue #1585): what `board nets
//! --power-stage` lists, and the copper of other nets near its switch node that `board drc`
//! notes (`switch_node_copper`).
//!
//! How a regulator and the parts round it are recognised is `netsmith_core`'s
//! ([`netsmith_core::switching_regulators`]); this measures them on the board file's copper:
//!
//! - each **input capacitor**: the distance from its VIN pad to the IC's nearest VIN pad and
//!   from its ground pad to the IC's nearest ground pad (centre to centre), and the routed
//!   length and vias of the shortest way through the tracks of each net between them, joined as
//!   `board nets` joins a connection ([`crate::connection`]); no way where a pour or a plane
//!   carries the net, or it is open;
//! - each **support capacitor** (a bootstrap, an internal supply's): the same from the IC's
//!   pin to the capacitor's pad on its net;
//! - the **feedback** net: its routed length and vias, as `board nets` has them;
//! - **foreign copper**: each via and track of a net that is none of the IC's nets nor the
//!   nets of the parts on the switch node (the inductor's output, a catch diode's), within the
//!   keepout of the switch node's copper (its pads and tracks), edge to edge, or under the IC's
//!   body (the box of its pads), on a layer with no plane between it and that copper: no zone
//!   on a copper layer strictly between the two over the place. A via is on every layer it
//!   spans.

use netsmith_core::SwitchingRegulator;
use netsmith_geom::{
    Artwork, DrcFinding, DrcItem, DrcRule, DrcSeverity, DrcShape, Pt, TrackShape, pad_copper,
    polygon_contains, shape_gap,
};
use serde::Serialize;

use crate::connection;
use crate::net_table::{Seg, number};

/// The keepout of a switch node when the board's process states none, in mm
/// (`.process.switch_node_keepout`).
pub const DEFAULT_SWITCH_NODE_KEEPOUT: f64 = 1.0;

/// The rule's name, as `board drc`'s note and its JSON name it.
pub const RULE: &str = "switch_node_copper";

/// One regulator's power stage.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PowerStage {
    /// The IC's reference designator and its path in the design.
    pub regulator: String,
    pub path: String,
    pub vin: String,
    pub gnd: String,
    /// The switch node's net.
    pub sw: String,
    /// The keepout the foreign copper is judged against, in mm.
    pub keepout: f64,
    /// The input capacitors, the nearest first.
    pub input_caps: Vec<InputCap>,
    /// The capacitors on the IC's other pins, in the order of its pins.
    pub support: Vec<SupportRoute>,
    pub feedback: Option<Feedback>,
    /// The vias and tracks of other nets near the switch node or under the IC, nearest first.
    pub foreign: Vec<ForeignCopper>,
}

/// An input capacitor's loop. Distances and lengths in mm; `None` where no track joins the two
/// pads (a pour or a plane carries the net, or it is open).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputCap {
    pub cap: String,
    pub vin: Leg,
    pub gnd: Leg,
}

/// One side of a loop: from the IC's pad to the capacitor's.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Leg {
    /// The IC's pad (`U1.3`) and the capacitor's (`C1.1`).
    pub from: String,
    pub to: String,
    /// Centre to centre.
    pub distance: f64,
    /// Along the shortest way through the net's tracks.
    pub routed: Option<f64>,
    /// The vias at which that way changes layer.
    pub vias: Option<usize>,
}

/// A support capacitor's route from the IC's pin.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SupportRoute {
    /// The pin as its datasheet names it.
    pub pin: String,
    pub net: String,
    pub cap: String,
    /// Whether its other pad is on the switch node: a bootstrap capacitor.
    pub boot: bool,
    pub leg: Leg,
}

/// The feedback net's routed length (its connection, as `board nets` has it) and vias.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Feedback {
    pub net: String,
    pub length: f64,
    pub vias: usize,
}

/// A via or a track of another net near the switch node or under the IC.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ForeignCopper {
    /// `via` or `track`.
    pub kind: &'static str,
    pub net: String,
    /// The track's layer, or the via's layers (`F.Cu-B.Cu`).
    pub layer: String,
    /// The via's centre or the track's point nearest the switch node, in the frame of the board
    /// file, mm.
    pub at: (f64, f64),
    /// Edge to edge to the nearest switch-node copper on a layer with no plane between; `None`
    /// when there is none such (a track under the IC on a layer a plane shields from the node,
    /// but not from the body).
    pub gap: Option<f64>,
    /// Whether it is under the IC's body (the box of its pads).
    pub under_body: bool,
}

impl ForeignCopper {
    /// The words of `board drc`'s note.
    pub fn note(&self, stage: &PowerStage) -> String {
        let near = match self.gap {
            Some(g) if g <= stage.keepout + 1e-9 => format!(
                "{} mm from the switch node {} of {}",
                number(g),
                stage.sw,
                stage.regulator
            ),
            _ => format!("near the switch node {} of {}", stage.sw, stage.regulator),
        };
        let under = if self.under_body {
            format!(", under {}'s body", stage.regulator)
        } else {
            String::new()
        };
        format!(
            "{} of {} on {} at ({}, {}) is {near}{under} (keepout {} mm); move it away, or put a plane between it and the switch node",
            self.kind,
            self.net,
            self.layer,
            number(self.at.0),
            number(self.at.1),
            number(stage.keepout),
        )
    }
}

/// The `switch_node_copper` notes of `stages` (issue #1585): a finding of the design rules
/// check for each via and track of another net near a switch node or under a regulator, at its
/// place, with severity `note`, which fails nothing with any strictness.
pub fn findings(stages: &[PowerStage]) -> Vec<DrcFinding> {
    stages
        .iter()
        .flat_map(|s| {
            s.foreign.iter().map(move |f| {
                let at = Pt::new(f.at.0, f.at.1);
                let item = |what: String, net: &str, part: Option<&str>| DrcItem {
                    what,
                    at,
                    part: part.map(str::to_string),
                    pad: None,
                    net: Some(net.to_string()),
                };
                DrcFinding {
                    rule: DrcRule::SwitchNodeCopper,
                    severity: DrcSeverity::Note,
                    message: f.note(s),
                    at,
                    layer: (f.kind == "track").then(|| f.layer.clone()),
                    items: vec![
                        item(
                            format!(
                                "{} of {}",
                                if f.kind == "via" { "Via" } else { "Track" },
                                f.net
                            ),
                            &f.net,
                            None,
                        ),
                        item(
                            format!("Switch node {} of {}", s.sw, s.regulator),
                            &s.sw,
                            Some(&s.regulator),
                        ),
                    ],
                    carrying: None,
                    source: None,
                    distance: None,
                }
            })
        })
        .collect()
}

/// The power stages of `regulators` on `art`, each judged against `keepout` mm.
pub fn of(art: &Artwork, regulators: &[SwitchingRegulator], keepout: f64) -> Vec<PowerStage> {
    let segs = crate::score::lines(art);
    let rows = crate::net_table::rows(art);
    regulators
        .iter()
        .filter_map(|r| stage(art, &segs, &rows, r, keepout))
        .collect()
}

/// The indices into `art.pads` of `part`'s pads numbered in `numbers` (every pad when
/// `numbers` is `None`) on `net`.
fn pads_of(art: &Artwork, part: &str, numbers: Option<&[String]>, net: &str) -> Vec<usize> {
    (0..art.pads.len())
        .filter(|&i| {
            let p = &art.pads[i];
            p.is_terminal()
                && p.part == part
                && p.net.as_deref() == Some(net)
                && numbers.is_none_or(|n| n.contains(&p.number))
        })
        .collect()
}

/// The leg from the nearest of `from` to the nearest of `to` (indices into `art.pads`).
fn leg(art: &Artwork, segs: &[Seg], net: &str, from: &[usize], to: &[usize]) -> Option<Leg> {
    let (a, b) = from
        .iter()
        .flat_map(|&a| to.iter().map(move |&b| (a, b)))
        .min_by(|x, y| {
            let d = |(a, b): (usize, usize)| art.pads[a].at.dist(art.pads[b].at);
            d(*x).total_cmp(&d(*y))
        })?;
    let way = connection::way(art, segs, net, from, to);
    let name = |i: usize| format!("{}.{}", art.pads[i].part, art.pads[i].number);
    Some(Leg {
        from: name(a),
        to: name(b),
        distance: art.pads[a].at.dist(art.pads[b].at),
        routed: way.map(|w| w.length),
        vias: way.map(|w| w.vias),
    })
}

fn stage(
    art: &Artwork,
    segs: &[Seg],
    rows: &[crate::NetRow],
    r: &SwitchingRegulator,
    keepout: f64,
) -> Option<PowerStage> {
    let ic =
        |role: &netsmith_core::RegulatorPin| pads_of(art, &r.refdes, Some(&role.pads), &role.net);
    let (vin, gnd, sw) = (ic(&r.vin), ic(&r.gnd), ic(&r.sw));
    if sw.is_empty() {
        return None;
    }
    let mut input_caps: Vec<InputCap> = r
        .input_caps
        .iter()
        .filter_map(|c| {
            let v = pads_of(art, c, None, &r.vin.net);
            let g = pads_of(art, c, None, &r.gnd.net);
            Some(InputCap {
                cap: c.clone(),
                vin: leg(art, segs, &r.vin.net, &vin, &v)?,
                gnd: leg(art, segs, &r.gnd.net, &gnd, &g)?,
            })
        })
        .collect();
    input_caps.sort_by(|a, b| {
        (a.vin.distance + a.gnd.distance).total_cmp(&(b.vin.distance + b.gnd.distance))
    });
    let support = r
        .support
        .iter()
        .filter_map(|s| {
            let from = pads_of(art, &r.refdes, Some(&s.pads), &s.net);
            let to = pads_of(art, &s.cap, None, &s.net);
            Some(SupportRoute {
                pin: s.pin.clone(),
                net: s.net.clone(),
                cap: s.cap.clone(),
                boot: s.boot,
                leg: leg(art, segs, &s.net, &from, &to)?,
            })
        })
        .collect();
    let feedback = r.fb.as_ref().map(|fb| {
        let row = rows.iter().find(|row| row.net == fb.net);
        Feedback {
            net: fb.net.clone(),
            length: row.map_or(0.0, |row| row.length),
            vias: row.map_or(0, |row| row.vias),
        }
    });
    Some(PowerStage {
        regulator: r.refdes.clone(),
        path: r.path.clone(),
        vin: r.vin.net.clone(),
        gnd: r.gnd.net.clone(),
        sw: r.sw.net.clone(),
        keepout,
        input_caps,
        support,
        feedback,
        foreign: foreign(art, r, keepout),
    })
}

/// A piece of copper by the copper layers it is on (indices into `art.copper`).
struct Copper {
    shapes: Vec<DrcShape>,
    layers: Vec<usize>,
}

/// Whether a zone of a net on a copper layer strictly between `a` and `b` covers `at`: a plane
/// that shields the two layers from each other there.
fn plane_between(art: &Artwork, a: usize, b: usize, at: Pt) -> bool {
    let (lo, hi) = (a.min(b), a.max(b));
    art.zones
        .iter()
        .filter(|z| z.keepout.is_none() && !z.teardrop && z.net.is_some())
        .any(|z| {
            z.layers.iter().any(|l| {
                art.copper
                    .iter()
                    .position(|c| c == l)
                    .is_some_and(|i| lo < i && i < hi)
            }) && z.outlines.iter().any(|o| polygon_contains(o, at))
        })
}

/// Whether some layer of `a` and some layer of `b` have no plane between them at `at`.
fn facing(art: &Artwork, a: &[usize], b: &[usize], at: Pt) -> bool {
    a.iter()
        .any(|&x| b.iter().any(|&y| !plane_between(art, x, y, at)))
}

fn foreign(art: &Artwork, r: &SwitchingRegulator, keepout: f64) -> Vec<ForeignCopper> {
    let layer_index = |l: &str| art.copper.iter().position(|c| c == l);
    let all: Vec<usize> = (0..art.copper.len()).collect();
    // The nets of the IC and of the parts on the switch node are the stage's own.
    let own: Vec<&str> = art
        .pads
        .iter()
        .filter(|p| p.part == r.refdes || r.switch_parts.contains(&p.part))
        .filter_map(|p| p.net.as_deref())
        .collect();
    // The switch node's copper: its pads and its tracks.
    let mut node: Vec<Copper> = art
        .pads
        .iter()
        .filter(|p| p.net.as_deref() == Some(r.sw.net.as_str()))
        .map(|p| Copper {
            shapes: pad_copper(p),
            layers: all
                .iter()
                .copied()
                .filter(|&l| art.pad_copper_on(p, &art.copper[l]))
                .collect(),
        })
        .collect();
    for t in art
        .tracks
        .iter()
        .filter(|t| t.net.as_deref() == Some(r.sw.net.as_str()))
    {
        node.push(Copper {
            shapes: track_shapes(t),
            layers: layer_index(&t.layer).into_iter().collect(),
        });
    }
    // The IC's body: the box of its pads' copper, and its side's outer layer.
    let ic_pads: Vec<&netsmith_geom::ArtworkPad> = art
        .pads
        .iter()
        .filter(|p| p.part == r.refdes && p.is_terminal())
        .collect();
    let (mut lo, mut hi) = (Pt::new(f64::MAX, f64::MAX), Pt::new(f64::MIN, f64::MIN));
    for s in ic_pads.iter().flat_map(|p| pad_copper(p)) {
        for q in &s.pts {
            lo = Pt::new(lo.x.min(q.x - s.r), lo.y.min(q.y - s.r));
            hi = Pt::new(hi.x.max(q.x + s.r), hi.y.max(q.y + s.r));
        }
    }
    let body = DrcShape::polygon(vec![lo, Pt::new(hi.x, lo.y), hi, Pt::new(lo.x, hi.y)], 0.0);
    let back = art
        .footprints
        .iter()
        .find(|f| f.part == r.refdes)
        .is_some_and(|f| f.back);
    let side = if back {
        art.copper.len().saturating_sub(1)
    } else {
        0
    };
    let foreign_net = |net: &Option<String>| -> Option<String> {
        let n = net.as_deref().filter(|n| !n.is_empty())?;
        (!own.contains(&n)).then(|| n.to_string())
    };
    let judge = |kind: &'static str,
                 net: String,
                 layer: String,
                 shapes: Vec<DrcShape>,
                 layers: Vec<usize>,
                 centre: Pt|
     -> Option<ForeignCopper> {
        let mut best: Option<(f64, Pt)> = None;
        for c in &node {
            for a in &shapes {
                for b in &c.shapes {
                    let g = shape_gap(a, b);
                    if best.is_none_or(|(d, _)| g.d < d) && facing(art, &layers, &c.layers, g.a) {
                        best = Some((g.d, g.a));
                    }
                }
            }
        }
        let on_body = shapes.iter().find_map(|s| {
            let g = shape_gap(s, &body);
            (g.d <= 0.0).then_some(g.a)
        });
        let under_body = on_body.is_some_and(|at| facing(art, &layers, &[side], at));
        let near = best.is_some_and(|(d, _)| d <= keepout + 1e-9);
        (near || under_body).then(|| {
            let at = match (kind, best, on_body) {
                ("via", _, _) => centre,
                (_, Some((_, p)), _) if near => p,
                (_, _, Some(p)) => p,
                _ => centre,
            };
            ForeignCopper {
                kind,
                net,
                layer,
                at: (at.x, at.y),
                gap: best.map(|(d, _)| d),
                under_body,
            }
        })
    };
    let mut out: Vec<ForeignCopper> = Vec::new();
    for v in &art.vias {
        let Some(net) = foreign_net(&v.net) else {
            continue;
        };
        let layers: Vec<usize> = all.iter().copied().filter(|&l| v.on(l)).collect();
        let name = match (layers.first(), layers.last()) {
            (Some(&a), Some(&b)) => format!("{}-{}", art.copper[a], art.copper[b]),
            _ => String::new(),
        };
        out.extend(judge(
            "via",
            net,
            name,
            vec![DrcShape::disc(v.at, v.diameter / 2.0)],
            layers,
            v.at,
        ));
    }
    for t in &art.tracks {
        let Some(net) = foreign_net(&t.net) else {
            continue;
        };
        let centre = match &t.shape {
            TrackShape::Line(a, _) => *a,
            TrackShape::Arc(arc) => arc.points().first().copied().unwrap_or_default(),
        };
        out.extend(judge(
            "track",
            net,
            t.layer.clone(),
            track_shapes(t),
            layer_index(&t.layer).into_iter().collect(),
            centre,
        ));
    }
    out.sort_by(|a, b| {
        a.gap
            .unwrap_or(f64::MAX)
            .total_cmp(&b.gap.unwrap_or(f64::MAX))
    });
    out
}

/// A track's copper: a capsule, or an arc's chords.
fn track_shapes(t: &netsmith_geom::ArtworkTrack) -> Vec<DrcShape> {
    match &t.shape {
        TrackShape::Line(a, b) => vec![DrcShape::capsule(*a, *b, t.width / 2.0)],
        TrackShape::Arc(arc) => vec![DrcShape::stroke(arc.points(), t.width / 2.0)],
    }
}

/// The text of `board nets --power-stage`: a block for each regulator, or a line that the
/// board has none.
pub fn text(stages: &[PowerStage]) -> String {
    if stages.is_empty() {
        return "power stage: no switching regulator (a part that provides `BuckIc`)\n".to_string();
    }
    let mm = |x: Option<f64>| x.map_or("-".to_string(), number);
    let leg = |l: &Leg| match (l.routed, l.vias) {
        (Some(len), Some(v)) => format!(
            "{} to {} {} mm apart, routed {} mm with {} via(s)",
            l.from,
            l.to,
            number(l.distance),
            number(len),
            v
        ),
        _ => format!(
            "{} to {} {} mm apart, no track (a pour or a plane, or open)",
            l.from,
            l.to,
            number(l.distance)
        ),
    };
    let mut out = String::from("power stage:\n");
    for s in stages {
        out.push_str(&format!(
            "  {} ({}): VIN {}, ground {}, switch node {}\n",
            s.regulator, s.path, s.vin, s.gnd, s.sw
        ));
        if s.input_caps.is_empty() {
            out.push_str("    input: no capacitor between VIN and ground\n");
        }
        for c in &s.input_caps {
            out.push_str(&format!(
                "    input {}: VIN {}; ground {}\n",
                c.cap,
                leg(&c.vin),
                leg(&c.gnd)
            ));
        }
        for c in &s.support {
            out.push_str(&format!(
                "    {} {} on {}: {}\n",
                if c.boot { "boot" } else { "support" },
                c.cap,
                c.pin,
                leg(&c.leg)
            ));
        }
        if let Some(f) = &s.feedback {
            out.push_str(&format!(
                "    feedback {}: routed {} mm with {} via(s)\n",
                f.net,
                number(f.length),
                f.vias
            ));
        }
        if s.foreign.is_empty() {
            out.push_str(&format!(
                "    foreign copper: none within {} mm of the switch node or under the body\n",
                number(s.keepout)
            ));
        }
        for f in &s.foreign {
            out.push_str(&format!(
                "    foreign {} of {} on {} at ({}, {}): {} mm from the switch node{}\n",
                f.kind,
                f.net,
                f.layer,
                number(f.at.0),
                number(f.at.1),
                mm(f.gap),
                if f.under_body { ", under the body" } else { "" }
            ));
        }
    }
    out
}
