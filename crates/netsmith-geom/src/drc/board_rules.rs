//! The rules a board is checked to, as data (`netsmith board rules`, issue #1513): for a tool
//! that draws copper by them, so that it keeps the spaces the check asks and not the spaces it
//! guessed from the findings.
//!
//! [`BoardRules::of`] reads only what the check reads ([`super::read`]: the project's numbers,
//! the board setup's mask web, the caller's [`Extras`]), through the same functions the rules
//! call ([`Rules::net_clearance`], [`Rules::pair_clearance`], [`Extras::least_width`],
//! [`Extras::hole_to_track_of`], [`Artwork::pad_mask_margin`]), so a number here is the number a
//! finding is judged against. Lengths are in mm; places are in the frame of the board file, y
//! down, as a finding's place is.

use super::{Extras, Rules};
use crate::artwork::{Artwork, Zone};
use crate::geom::Pt;
use serde::Serialize;

/// Every rule of the check that a tool drawing copper needs (issue #1513).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoardRules {
    /// The least clearance between copper of two nets on the board, and the clearance between
    /// the nets of each pair of classes (the larger of the two classes').
    pub clearance: Clearances,
    /// The net classes, the default one first.
    pub classes: Vec<ClassRules>,
    /// Each net of the board, by name.
    pub nets: Vec<NetRules>,
    /// The widths of tracks: the least of the board, and how near one of its net's pads a track
    /// may be narrower than its current asks (the neck-down out of a fine-pitch pad).
    pub widths: Widths,
    /// The solder mask: the margin of each footprint's openings and the spaces round them.
    pub mask: Mask,
    /// The spaces round holes and the sizes of holes.
    pub holes: Holes,
    /// The least space between surface-mount pads of different nets; `None` is not checked.
    pub pad_to_pad: Option<f64>,
    /// The least space from copper, and from a hole, to the board edge.
    pub copper_to_edge: f64,
    pub vias: Vias,
    /// The rule areas of the board and of its footprints, with the kinds they forbid.
    pub rule_areas: Vec<RuleArea>,
    pub layers: Layers,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Clearances {
    /// The board's least: no class keeps less.
    pub least: f64,
    /// The clearance between the nets of two classes, each pair once (a class with itself too).
    pub pairs: Vec<ClassPair>,
    /// A pad's own clearance, where it is larger than the pair's, counts on top of it.
    pub pads: Vec<PadClearance>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassPair {
    pub a: String,
    pub b: String,
    pub clearance: f64,
}

/// A pad that states a clearance of its own.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PadClearance {
    pub part: String,
    pub pad: String,
    pub clearance: f64,
}

/// A net class: the clearance its nets keep and what its copper is drawn at.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassRules {
    /// `Default` for the class of every net no other class names.
    pub name: String,
    /// The clearance its nets keep, never less than the board's least.
    pub clearance: f64,
    /// Lower wins where two classes name a net; `None` for the default class.
    pub priority: Option<i64>,
    /// The net name patterns it holds (`*` is any run of characters).
    pub patterns: Vec<String>,
    pub track_width: Option<f64>,
    pub via_diameter: Option<f64>,
    pub via_drill: Option<f64>,
    pub diff_pair_gap: Option<f64>,
    pub diff_pair_width: Option<f64>,
}

/// What one net's copper is held to.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NetRules {
    pub net: String,
    pub class: String,
    /// The clearance it keeps from copper of a net of its own class or a class that keeps less.
    pub clearance: f64,
    /// The width its class draws its tracks at, the width it wants.
    pub width: Option<f64>,
    /// The least width of its tracks on an outer layer and on an inner one: the board's least,
    /// or what its current needs (`track_current`) where that is more, away from its pads.
    pub least_width: f64,
    pub least_width_inner: f64,
    /// The width that gives its impedance target, by layer (`track_impedance`, a warning).
    pub impedance_widths: Vec<LayerWidth>,
    /// How many vias side by side it needs where it changes layer (`via_current`).
    pub vias: usize,
    /// The copper and hole its class draws its vias at.
    pub via_diameter: Option<f64>,
    pub via_drill: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LayerWidth {
    pub layer: String,
    pub width: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Widths {
    pub least: f64,
    pub neck_reach: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Mask {
    /// How far an opening reaches past a pad's copper where neither the pad nor its footprint
    /// says.
    pub margin: f64,
    /// The least mask between the openings of pads of different nets (`solder_mask_bridge`).
    pub web: f64,
    /// Whether the mask between the pads of one footprint may run together.
    pub bridges_in_footprints: bool,
    /// The least space from a track to the opening of a pad of another net (`mask_to_track`);
    /// `None` is not checked.
    pub to_track: Option<f64>,
    /// Each footprint's margin, and its pads whose margin is not the footprint's.
    pub footprints: Vec<FootprintMask>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FootprintMask {
    pub part: String,
    pub margin: f64,
    pub pads: Vec<PadMask>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PadMask {
    pub pad: String,
    pub margin: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Holes {
    /// The least space from a hole to copper of another net (`hole_clearance`).
    pub to_copper: f64,
    /// The least space from a via's or a non-plated hole to a track, and from a plated pad's
    /// hole to a track (`hole_to_track`); `None` is not checked.
    pub to_track: Option<f64>,
    pub plated_pad_to_track: Option<f64>,
    /// The least space between two vias' holes (`hole_to_hole`), and between two holes when one
    /// is a pad's (`pad_hole_to_hole`).
    pub hole_to_hole: f64,
    pub pad_hole_to_hole: Option<f64>,
    /// The least drilled hole (`drill_out_of_range`), non-plated hole, plated and non-plated
    /// slot, and copper round a plated pad's hole.
    pub least_drill: f64,
    pub least_npth_drill: Option<f64>,
    pub least_plated_slot: Option<f64>,
    pub least_npth_slot: Option<f64>,
    pub least_pad_ring: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Vias {
    /// The least copper of a via, its least hole and the least copper round its hole.
    pub least_diameter: f64,
    pub least_drill: f64,
    pub least_ring: f64,
    /// A micro via's least copper and hole, where the process makes them.
    pub least_microvia_diameter: Option<f64>,
    pub least_microvia_drill: Option<f64>,
    /// The copper and hole the default class draws its vias at.
    pub diameter: Option<f64>,
    pub drill: Option<f64>,
    /// The widest via the process fills and caps, so that it may sit in a pad.
    pub in_pad: Option<f64>,
    /// The most space between the copper of two vias of one net at which they stand side by
    /// side as one group for `via_current` (issue #1505).
    pub group_gap: f64,
    /// How near a pad's copper a via's hole and its ring come, edge to edge, before the check
    /// notes them (`via_hole_near_pad`, `via_ring_near_pad`, issue #1579); `None` notes nothing.
    pub hole_pad_reach: Option<f64>,
    pub ring_pad_reach: Option<f64>,
    /// The blind and buried vias the process makes, by their first and last copper layer.
    pub spans: Vec<ViaSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ViaSpan {
    pub from: String,
    pub to: String,
    pub laser: bool,
}

/// A rule area: its outline and holes in the frame of the board file, the copper layers it is
/// on, and the kinds it forbids there (`tracks`, `vias`, `pads`, `copperpour`, `footprints`).
/// What only touches its edge is not in it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuleArea {
    pub name: String,
    /// The part whose footprint carries it; `None` for the board's. A footprint's area does not
    /// forbid the footprint itself.
    pub part: Option<String>,
    pub layers: Vec<String>,
    pub outlines: Vec<Vec<[f64; 2]>>,
    pub holes: Vec<Vec<[f64; 2]>>,
    pub forbids: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Layers {
    /// The copper layers, front to back.
    pub copper: Vec<String>,
    /// The board's thickness and its stackup, top to bottom, as the file states them.
    pub thickness: Option<f64>,
    pub stackup: Vec<StackupLayer>,
    /// The zones that pour a net (the planes and pours), with the space they keep.
    pub pours: Vec<Pour>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StackupLayer {
    pub name: String,
    pub kind: String,
    pub thickness: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pour {
    pub net: String,
    pub name: String,
    pub layers: Vec<String>,
    pub clearance: f64,
    pub min_thickness: f64,
    pub priority: u32,
}

fn points(pts: &[Pt]) -> Vec<[f64; 2]> {
    pts.iter().map(|p| [p.x, p.y]).collect()
}

fn forbids(z: &Zone) -> Vec<&'static str> {
    let Some(k) = &z.keepout else {
        return Vec::new();
    };
    [
        (k.tracks, "tracks"),
        (k.vias, "vias"),
        (k.pads, "pads"),
        (k.copperpour, "copperpour"),
        (k.footprints, "footprints"),
    ]
    .into_iter()
    .filter_map(|(no, kind)| no.then_some(kind))
    .collect()
}

impl BoardRules {
    /// The rules `art` is checked to with `rules` and `extras`, as [`super::check_artwork`]
    /// holds it to them; `bridges_in_footprints` as the board's setup says.
    pub fn of(
        art: &Artwork,
        rules: &Rules,
        extras: &Extras,
        bridges_in_footprints: bool,
    ) -> BoardRules {
        const DEFAULT: &str = "Default";
        let mut classes = vec![ClassRules {
            name: DEFAULT.to_string(),
            clearance: rules.class_clearance(None),
            priority: None,
            patterns: Vec::new(),
            track_width: rules.default_sizes.track_width,
            via_diameter: rules.default_sizes.via_diameter,
            via_drill: rules.default_sizes.via_drill,
            diff_pair_gap: rules.default_sizes.diff_pair_gap,
            diff_pair_width: rules.default_sizes.diff_pair_width,
        }];
        classes.extend(rules.classes.iter().map(|c| ClassRules {
            name: c.name.clone(),
            clearance: rules.class_clearance(Some(c)),
            priority: Some(c.priority),
            patterns: c.patterns.clone(),
            track_width: c.sizes.track_width,
            via_diameter: c.sizes.via_diameter,
            via_drill: c.sizes.via_drill,
            diff_pair_gap: c.sizes.diff_pair_gap,
            diff_pair_width: c.sizes.diff_pair_width,
        }));
        let mut pairs = Vec::new();
        for (i, a) in classes.iter().enumerate() {
            for b in &classes[i..] {
                pairs.push(ClassPair {
                    a: a.name.clone(),
                    b: b.name.clone(),
                    clearance: a.clearance.max(b.clearance),
                });
            }
        }
        let pads = art
            .pads
            .iter()
            .filter_map(|p| {
                let c = p.clearance.filter(|&c| c > 0.0)?;
                Some(PadClearance {
                    part: p.part.clone(),
                    pad: p.number.clone(),
                    clearance: c,
                })
            })
            .collect();

        // Every net the board's copper names, once, by name.
        let mut names: Vec<&str> = art
            .pads
            .iter()
            .filter_map(|p| p.net.as_deref())
            .chain(art.vias.iter().filter_map(|v| v.net.as_deref()))
            .chain(art.tracks.iter().filter_map(|t| t.net.as_deref()))
            .chain(
                art.zones
                    .iter()
                    .filter(|z| z.keepout.is_none())
                    .filter_map(|z| z.net.as_deref()),
            )
            .filter(|n| !n.is_empty())
            .collect();
        names.sort_unstable();
        names.dedup();
        let nets = names
            .into_iter()
            .map(|net| {
                let sizes = rules.net_sizes(Some(net));
                let least = |inner: bool| {
                    extras
                        .least_width(net, inner)
                        .unwrap_or(0.0)
                        .max(rules.track_width)
                };
                NetRules {
                    net: net.to_string(),
                    class: rules
                        .class_of(Some(net))
                        .map_or(DEFAULT, |c| c.name.as_str())
                        .to_string(),
                    clearance: rules.net_clearance(Some(net)),
                    width: sizes.track_width,
                    least_width: least(false),
                    least_width_inner: least(true),
                    impedance_widths: extras
                        .target_widths
                        .iter()
                        .filter(|(n, _, _)| n == net)
                        .map(|(_, layer, width)| LayerWidth {
                            layer: layer.clone(),
                            width: *width,
                        })
                        .collect(),
                    vias: extras
                        .via_counts
                        .iter()
                        .find(|(n, _)| n == net)
                        .map_or(1, |&(_, k)| k),
                    via_diameter: sizes.via_diameter,
                    via_drill: sizes.via_drill,
                }
            })
            .collect();

        // Each footprint's margin: its pads' footprint margin, else the board's; and the pads
        // whose own margin differs from it.
        let mut footprints: Vec<FootprintMask> = Vec::new();
        for p in &art.pads {
            let at = match footprints.iter().position(|f| f.part == p.part) {
                Some(at) => at,
                None => {
                    footprints.push(FootprintMask {
                        part: p.part.clone(),
                        margin: p.footprint_mask_margin.unwrap_or(art.mask_margin),
                        pads: Vec::new(),
                    });
                    footprints.len() - 1
                }
            };
            let margin = art.pad_mask_margin(p);
            let f = &mut footprints[at];
            if (margin - f.margin).abs() > 1e-9 && !f.pads.iter().any(|q| q.pad == p.number) {
                f.pads.push(PadMask {
                    pad: p.number.clone(),
                    margin,
                });
            }
        }
        footprints.sort_by(|a, b| a.part.cmp(&b.part));

        let layer_name = |i: usize| art.copper.get(i).cloned().unwrap_or_default();
        let spans = extras
            .vias
            .iter()
            .flat_map(|v| &v.spans)
            .map(|&(a, b, laser)| ViaSpan {
                from: layer_name(a),
                to: layer_name(b),
                laser,
            })
            .collect();

        let copper_layers = |z: &Zone| -> Vec<String> {
            z.layers
                .iter()
                .filter(|l| l.ends_with(".Cu"))
                .cloned()
                .collect()
        };
        let rule_areas = art
            .zones
            .iter()
            .filter(|z| z.keepout.is_some())
            .map(|z| RuleArea {
                name: z.name.clone(),
                part: (!z.owner.is_empty()).then(|| z.owner.clone()),
                layers: copper_layers(z),
                outlines: z.outlines.iter().map(|o| points(o)).collect(),
                holes: z.holes.iter().map(|h| points(h)).collect(),
                forbids: forbids(z),
            })
            .collect();
        let pours = art
            .zones
            .iter()
            .filter(|z| z.keepout.is_none() && !z.teardrop)
            .filter_map(|z| {
                Some(Pour {
                    net: z.net.clone().filter(|n| !n.is_empty())?,
                    name: z.name.clone(),
                    layers: copper_layers(z),
                    clearance: z.clearance,
                    min_thickness: z.min_thickness,
                    priority: z.priority,
                })
            })
            .collect();

        BoardRules {
            clearance: Clearances {
                least: rules.clearance,
                pairs,
                pads,
            },
            classes,
            nets,
            widths: Widths {
                least: rules.track_width,
                neck_reach: extras.neck_reach,
            },
            mask: Mask {
                margin: art.mask_margin,
                web: rules.mask_web,
                bridges_in_footprints,
                to_track: extras.mask_to_track,
                footprints,
            },
            holes: Holes {
                to_copper: rules.hole_clearance,
                to_track: extras.hole_to_track_of(false),
                plated_pad_to_track: extras.hole_to_track_of(true),
                hole_to_hole: rules.hole_to_hole,
                pad_hole_to_hole: extras.pad_hole_to_hole,
                least_drill: rules.through_hole,
                least_npth_drill: extras.npth_drill,
                least_plated_slot: extras.plated_slot,
                least_npth_slot: extras.npth_slot,
                least_pad_ring: extras.pad_ring,
            },
            pad_to_pad: extras.pad_to_pad,
            copper_to_edge: rules.copper_edge,
            vias: Vias {
                least_diameter: rules.via_diameter,
                least_drill: rules.through_hole,
                least_ring: rules.annular_width,
                least_microvia_diameter: rules.microvia_diameter,
                least_microvia_drill: rules.microvia_drill,
                diameter: rules.default_sizes.via_diameter,
                drill: rules.default_sizes.via_drill,
                in_pad: extras.via_in_pad,
                group_gap: extras.via_group_gap,
                hole_pad_reach: extras.via_hole_pad_reach,
                ring_pad_reach: extras.via_ring_pad_reach,
                spans,
            },
            rule_areas,
            layers: Layers {
                copper: art.copper.clone(),
                thickness: art.thickness,
                stackup: art
                    .stackup
                    .iter()
                    .map(|l| StackupLayer {
                        name: l.name.clone(),
                        kind: l.kind.clone(),
                        thickness: l.thickness,
                    })
                    .collect(),
                pours,
            },
        }
    }

    /// The rules as text for a person: a line for each kind of rule, then the classes, the
    /// nets, the footprints with a margin of their own, the rule areas and the pours.
    pub fn text(&self) -> String {
        self.text_moved(Pt::new(0.0, 0.0))
    }

    /// [`BoardRules::text`] with its places moved by `by`, mm (a rule area's first corner).
    pub fn text_moved(&self, by: Pt) -> String {
        use std::fmt::Write;
        let mm =
            |v: Option<f64>| v.map_or_else(|| "not checked".to_string(), |v| format!("{v:.4} mm"));
        let mut s = String::new();
        let mut line = |text: String| {
            s.push_str(&text);
            s.push('\n');
        };
        line(format!("clearance: {:.4} mm least", self.clearance.least));
        line(format!(
            "track width: {:.4} mm least (a track may neck down to it within {:.4} mm of the pad centres of a part on its net, not of the pads' edges)",
            self.widths.least, self.widths.neck_reach
        ));
        line(format!(
            "copper to edge: {:.4} mm; pad to pad (surface-mount, other nets): {}",
            self.copper_to_edge,
            mm(self.pad_to_pad)
        ));
        let h = &self.holes;
        line(format!(
            "holes: to copper {:.4} mm, via or non-plated hole to track {}, plated pad hole to track {}, hole to hole {:.4} mm, pad hole to hole {}",
            h.to_copper,
            mm(h.to_track),
            mm(h.plated_pad_to_track),
            h.hole_to_hole,
            mm(h.pad_hole_to_hole)
        ));
        line(format!(
            "hole sizes: drill {:.4} mm least, non-plated {}, plated slot {}, non-plated slot {}, pad ring {}",
            h.least_drill,
            mm(h.least_npth_drill),
            mm(h.least_plated_slot),
            mm(h.least_npth_slot),
            mm(h.least_pad_ring)
        ));
        let v = &self.vias;
        line(format!(
            "vias: {} / {} drawn, {:.4} mm / {:.4} mm least, ring {:.4} mm, in a pad up to {}, side by side within {:.4} mm, noted near a pad within {} (hole) and {} (ring)",
            mm(v.diameter),
            mm(v.drill),
            v.least_diameter,
            v.least_drill,
            v.least_ring,
            mm(v.in_pad),
            v.group_gap,
            mm(v.hole_pad_reach),
            mm(v.ring_pad_reach)
        ));
        for span in &v.spans {
            line(format!(
                "  span {} to {}{}",
                span.from,
                span.to,
                if span.laser { " (laser)" } else { "" }
            ));
        }
        let m = &self.mask;
        line(format!(
            "mask: margin {:.4} mm, web {:.4} mm{}, to track {}",
            m.margin,
            m.web,
            if m.bridges_in_footprints {
                " (bridges in footprints allowed)"
            } else {
                ""
            },
            mm(m.to_track)
        ));
        for f in m
            .footprints
            .iter()
            .filter(|f| (f.margin - m.margin).abs() > 1e-9 || !f.pads.is_empty())
        {
            let mut text = format!("  {}: margin {:.4} mm", f.part, f.margin);
            for p in &f.pads {
                let _ = write!(text, ", pad {} {:.4} mm", p.pad, p.margin);
            }
            line(text);
        }
        line("classes:".to_string());
        for c in &self.classes {
            let mut text = format!("  {}: clearance {:.4} mm", c.name, c.clearance);
            if let Some(w) = c.track_width {
                let _ = write!(text, ", track {w:.4} mm");
            }
            if let (Some(d), Some(h)) = (c.via_diameter, c.via_drill) {
                let _ = write!(text, ", via {d:.4} / {h:.4} mm");
            }
            if let (Some(g), Some(w)) = (c.diff_pair_gap, c.diff_pair_width) {
                let _ = write!(text, ", pair {w:.4} mm at {g:.4} mm gap");
            }
            if !c.patterns.is_empty() {
                let _ = write!(text, " ({})", c.patterns.join(", "));
            }
            line(text);
        }
        if self.classes.len() > 1 {
            line("clearance between classes:".to_string());
            for p in self.clearance.pairs.iter().filter(|p| p.a != p.b) {
                line(format!("  {} - {}: {:.4} mm", p.a, p.b, p.clearance));
            }
        }
        for p in &self.clearance.pads {
            line(format!(
                "  pad {} of {}: clearance {:.4} mm of its own",
                p.pad, p.part, p.clearance
            ));
        }
        line("nets:".to_string());
        for n in &self.nets {
            let mut text = format!(
                "  {} ({}): clearance {:.4} mm, width {}, least {:.4} mm",
                n.net,
                n.class,
                n.clearance,
                mm(n.width),
                n.least_width
            );
            if (n.least_width_inner - n.least_width).abs() > 1e-9 {
                let _ = write!(text, " ({:.4} mm inner)", n.least_width_inner);
            }
            for w in &n.impedance_widths {
                let _ = write!(text, ", {:.4} mm on {} for its impedance", w.width, w.layer);
            }
            if n.vias > 1 {
                let _ = write!(text, ", {} vias side by side", n.vias);
            }
            line(text);
        }
        if !self.rule_areas.is_empty() {
            line("rule areas:".to_string());
            for a in &self.rule_areas {
                let name = if a.name.is_empty() {
                    "(no name)"
                } else {
                    &a.name
                };
                let whose = a
                    .part
                    .as_ref()
                    .map(|p| format!(" of {p}"))
                    .unwrap_or_default();
                let at = a
                    .outlines
                    .first()
                    .and_then(|o| o.first())
                    .map(|p| format!(" from ({:.3}, {:.3})", p[0] + by.x, p[1] + by.y))
                    .unwrap_or_default();
                line(format!(
                    "  {name}{whose} on {}{at}: no {}",
                    a.layers.join(", "),
                    a.forbids.join(", ")
                ));
            }
        }
        let l = &self.layers;
        line(format!(
            "layers: {}{}",
            l.copper.join(", "),
            l.thickness
                .map(|t| format!(", {t:.3} mm thick"))
                .unwrap_or_default()
        ));
        for p in &l.pours {
            line(format!(
                "  pour {} on {}: clearance {:.4} mm, least {:.4} mm",
                p.net,
                p.layers.join(", "),
                p.clearance,
                p.min_thickness
            ));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Extras, Rule, check, rules_of};

    /// A project with a least clearance of 0.1 mm, a default class of 0.15 mm and a `Power`
    /// class of 0.3 mm for the nets `V*`, drawn 0.5 mm wide.
    const PRO: &str = r#"{
      "board": {"design_settings": {"rules": {"min_clearance": 0.1, "min_track_width": 0.15}}},
      "net_settings": {
        "classes": [
          {"name": "Default", "clearance": 0.15, "priority": 2147483647, "track_width": 0.2, "via_diameter": 0.6, "via_drill": 0.3},
          {"name": "Power", "clearance": 0.3, "priority": 0, "track_width": 0.5}],
        "netclass_patterns": [{"netclass": "Power", "pattern": "V*"}]}}"#;

    /// A 20 mm board with a VIN pad of R1 at (10, 10), 1 mm square, whose footprint widens its
    /// mask opening by 0.3 mm, a rule area of U1 that forbids tracks and vias, and `items`.
    fn pcb(items: &str) -> String {
        format!(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (net 0 "") (net 1 "VIN") (net 2 "SIG")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            (footprint "L:R" (layer "F.Cu") (at 10 10) (solder_mask_margin 0.3)
              (property "Reference" "R1")
              (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu" "F.Mask") (net 1 "VIN")))
            (footprint "L:U" (layer "F.Cu") (at 15 15)
              (property "Reference" "U1")
              (zone (net 0) (net_name "") (layer "F.Cu") (name "ANT") (hatch edge 0.5)
                (keepout (tracks not_allowed) (vias not_allowed) (pads allowed) (copperpour allowed) (footprints allowed))
                (polygon (pts (xy 14 14) (xy 18 14) (xy 18 18) (xy 14 18)))))
            {items})"#
        )
    }

    fn extras() -> Extras {
        Extras {
            mask_to_track: Some(0.1),
            least_widths: vec![("VIN".into(), 0.4)],
            hole_to_track: Some(0.25),
            ..Extras::default()
        }
    }

    fn track(net: u32, (x0, y0): (f64, f64), (x1, y1): (f64, f64), width: f64) -> String {
        format!(
            r#"(segment (start {x0} {y0}) (end {x1} {y1}) (width {width}) (layer "F.Cu") (net {net}))"#
        )
    }

    /// The findings of a rule on the board with `items`.
    fn found(items: &str, rule: Rule) -> usize {
        check(&pcb(items), Some(PRO), &extras())
            .unwrap()
            .count(rule)
    }

    #[test]
    fn the_document_says_the_classes_nets_mask_and_rule_areas() {
        let r = rules_of(&pcb(""), Some(PRO), &extras()).unwrap();
        assert_eq!(r.clearance.least, 0.1);
        let names: Vec<(&str, f64)> = r
            .classes
            .iter()
            .map(|c| (c.name.as_str(), c.clearance))
            .collect();
        assert_eq!(names, [("Default", 0.15), ("Power", 0.3)]);
        let pair = |a: &str, b: &str| {
            r.clearance
                .pairs
                .iter()
                .find(|p| p.a == a && p.b == b)
                .map(|p| p.clearance)
        };
        assert_eq!(pair("Default", "Power"), Some(0.3));
        assert_eq!(pair("Default", "Default"), Some(0.15));
        let vin = r.nets.iter().find(|n| n.net == "VIN").unwrap();
        assert_eq!(
            (
                vin.class.as_str(),
                vin.clearance,
                vin.width,
                vin.least_width
            ),
            ("Power", 0.3, Some(0.5), 0.4)
        );
        assert_eq!(vin.via_diameter, Some(0.6), "the default class's via");
        let r1 = r.mask.footprints.iter().find(|f| f.part == "R1").unwrap();
        assert_eq!((r1.margin, r1.pads.len()), (0.3, 0));
        assert_eq!(r.mask.to_track, Some(0.1));
        assert_eq!(r.rule_areas.len(), 1);
        let area = &r.rule_areas[0];
        assert_eq!(area.part.as_deref(), Some("U1"));
        assert_eq!(area.forbids, ["tracks", "vias"]);
        assert_eq!(area.outlines[0][0], [14.0, 14.0]);
        assert_eq!(r.layers.copper, ["F.Cu", "B.Cu"]);
    }

    #[test]
    fn the_check_fails_copper_just_inside_each_number_and_passes_it_just_outside() {
        let r = rules_of(&pcb(""), Some(PRO), &extras()).unwrap();
        // The clearance between a VIN and a SIG track, edge to edge.
        let gap = r
            .clearance
            .pairs
            .iter()
            .find(|p| p.a == "Default" && p.b == "Power")
            .unwrap()
            .clearance;
        let pair = |g: f64| {
            let y = 3.0 + 0.2 + g;
            format!(
                "{} {}",
                track(1, (2.0, 3.0), (8.0, 3.0), 0.2),
                track(2, (2.0, y), (8.0, y), 0.2)
            )
        };
        assert_eq!(found(&pair(gap - 0.01), Rule::Clearance), 1);
        assert_eq!(found(&pair(gap + 0.01), Rule::Clearance), 0);

        // A SIG track beside R1's pad: the opening is the footprint's margin wider, and the
        // track keeps the mask-to-track space from it.
        let r1 = r.mask.footprints.iter().find(|f| f.part == "R1").unwrap();
        let reach = r1.margin + r.mask.to_track.unwrap();
        let beside = |d: f64| {
            let x = 10.5 + d + 0.1;
            track(2, (x, 9.0), (x, 11.0), 0.2)
        };
        assert_eq!(found(&beside(reach - 0.01), Rule::MaskToTrack), 1);
        assert_eq!(found(&beside(reach + 0.01), Rule::MaskToTrack), 0);

        // A VIN track away from its pads, narrower and wider than its least width.
        let vin = r.nets.iter().find(|n| n.net == "VIN").unwrap();
        let wide = |w: f64| track(1, (2.0, 3.0), (8.0, 3.0), w);
        assert_eq!(found(&wide(vin.least_width - 0.01), Rule::TrackCurrent), 1);
        assert_eq!(found(&wide(vin.least_width + 0.01), Rule::TrackCurrent), 0);

        // A SIG track into the rule area, and one beside it.
        let area = &r.rule_areas[0];
        let [x, y] = area.outlines[0][0];
        let at = |dx: f64| track(2, (x + dx, y + 1.0), (x + dx, y + 2.0), 0.2);
        assert_eq!(found(&at(0.5), Rule::ItemsNotAllowed), 1);
        assert_eq!(found(&at(-0.5), Rule::ItemsNotAllowed), 0);
    }
}
