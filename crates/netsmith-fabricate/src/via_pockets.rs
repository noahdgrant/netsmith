//! Whether a via cuts a pour, a plane or a zone into a new island (issue #1668): a pocket of the
//! fill, on a layer the via passes, that holds pads or vias of the fill's net and that the via's
//! clearance seals off from the rest of the fill. `routes sites` and `routes check-via` say it
//! of a via not yet made, `routes add-via` and `routes move-via` of the via a change makes; the
//! design rules check refuses none of it (the pocket's pads or vias may be joined to their net
//! by other copper), so it is a note.
//!
//! **The test is local.** The board (placed, with the routes file's copper) is cut to a box
//! round the via, [`POCKET_REACH`] beyond its copper: its tracks, vias and parts within the
//! board's largest space of the box, and its fills ([`crate::routes::near::zones_of`], the
//! planes and pours as zones too) cut to the box, as the check of an edit cuts them
//! ([`crate::routes::near`]). The fills are filled with the via and without it, as `board fill
//! --islands` fills them (`netsmith_pcb::IslandsReport`, the fill the Gerbers hold), so the
//! work is two fills of the box, whatever the board's size ([`Pockets::fills`] counts them).
//!
//! **A new island.** Each region of a fill without the via holds the pads and the vias of its
//! net it joins, and the box's edge when it reaches it (it may go on outside). With the via a
//! region may part into several: the one that reaches the box's edge keeps the fill (or, when
//! none does, the one that joins the most), and each other part that joins a pad or a via is a
//! new island, said with its layer, the fill (`plane`, `pour` or `zone`), its net, a corner of
//! it, its area and the pads and vias it holds. A part that reaches the box's edge is not one:
//! it may join the rest outside the box. So a pocket that reaches further than [`POCKET_REACH`]
//! from the via is not seen here; `board fill --islands` finds it on the whole board.

use std::collections::BTreeSet;

use netsmith_pcb::{Board, BoardNet, IslandsReport, Pt, Rect, Via};

use crate::routes::near;

/// How far beyond a via's copper, in mm, a pocket it seals is looked for ([module](self)).
pub const POCKET_REACH: f64 = 3.0;

/// How near the box's edge, in mm, a region may come and be taken to reach it.
const EDGE: f64 = 0.05;

/// A via of the pocket.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PocketVia {
    /// In the routes file's frame.
    pub at: [f64; 2],
}

/// A new island a via would cut a fill into ([module](self)).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Pocket {
    /// The routes file's word for the layer (`inner1`).
    pub layer: String,
    /// `plane`, `pour` or `zone`.
    pub fill: String,
    pub net: String,
    /// A corner of it, in the routes file's frame.
    pub at: [f64; 2],
    /// Its area, mm².
    pub area: f64,
    /// The pads (`REF.PAD`) and vias of the net it holds, cut off from the rest of the fill.
    pub pads: Vec<String>,
    pub vias: Vec<PocketVia>,
}

impl Pocket {
    /// One line for a person: `cuts the inner1 plane of GND into a new island at 1.000,2.000
    /// (0.420 mm2) holding U1.3 and 1 via at 1.200,2.200`, the places moved by `by`.
    pub fn text_moved(&self, by: [f64; 2]) -> String {
        let at = |p: [f64; 2]| format!("{:.3},{:.3}", p[0] + by[0], p[1] + by[1]);
        let mut held: Vec<String> = self.pads.clone();
        match self.vias.as_slice() {
            [] => {}
            [v] => held.push(format!("1 via at {}", at(v.at))),
            vs => held.push(format!(
                "{} vias at {}",
                vs.len(),
                vs.iter().map(|v| at(v.at)).collect::<Vec<_>>().join(" ")
            )),
        }
        format!(
            "cuts the {} {} of {} into a new island at {} ({:.3} mm2) holding {}",
            self.layer,
            self.fill,
            self.net,
            at(self.at),
            self.area,
            held.join(", ")
        )
    }
}

/// The local test of [module](self): the board cut to a box, and the fills made.
pub struct Pockets {
    /// The fills made, each of the box.
    pub fills: usize,
}

impl Pockets {
    pub fn new() -> Pockets {
        Pockets { fills: 0 }
    }

    /// The new islands `via` would cut the fills of `whole` (placed, with the routes file's
    /// copper, without the via) into.
    pub fn of_via(&mut self, whole: &Board, via: &Via) -> Result<Vec<Pocket>, String> {
        let area = near::via_box(via).expanded(POCKET_REACH);
        let mut after = whole.clone();
        after.vias.push(via.clone());
        self.of_change(whole, &after, area)
    }

    /// The new islands that `after` has and `before` has not in the box round each of `at`
    /// (the places of the vias a change made), each [`POCKET_REACH`] beyond a via of `diameter`.
    pub fn of_change_at(
        &mut self,
        before: &Board,
        after: &Board,
        at: &[Pt],
        diameter: f64,
    ) -> Result<Vec<Pocket>, String> {
        let mut out: Vec<Pocket> = Vec::new();
        for &p in at {
            let area = Rect::new(p, p).expanded(diameter / 2.0 + POCKET_REACH);
            for pocket in self.of_change(before, after, area)? {
                if !out.contains(&pocket) {
                    out.push(pocket);
                }
            }
        }
        Ok(out)
    }

    /// The new islands of `after` against `before` (whole boards) in `area`.
    fn of_change(
        &mut self,
        before: &Board,
        after: &Board,
        area: Rect,
    ) -> Result<Vec<Pocket>, String> {
        let zones: Vec<_> = near::clipped(&near::zones_of(before), &[area])
            .into_iter()
            .filter(|z| !z.net.is_empty())
            .collect();
        if zones.is_empty() {
            return Ok(Vec::new());
        }
        let reach = near::reach(before);
        let cut = |whole: &Board| {
            let mut b = near::local(whole, &[], &[area.expanded(reach)]);
            b.zones = near::clipped(&near::zones_of(whole), &[area]);
            for v in &whole.vias {
                if !b.nets.iter().any(|n| n.name == v.net) {
                    b.nets.push(BoardNet {
                        name: v.net.clone(),
                        pads: Vec::new(),
                    });
                }
            }
            b
        };
        let mut islands = |b: &Board| -> Result<IslandsReport, String> {
            self.fills += 1;
            IslandsReport::of(b).map_err(|e| format!("the fill near the via: {e}"))
        };
        let was = islands(&cut(before))?;
        let now = islands(&cut(after))?;
        Ok(new_islands(before, &was, &now, area))
    }
}

impl Default for Pockets {
    fn default() -> Self {
        Pockets::new()
    }
}

/// What a region holds: its pads and vias, by name, and `OUT` when it reaches the box's edge.
const OUT: &str = "(the box's edge)";

fn held(r: &netsmith_pcb::FillRegion, area: &Rect) -> BTreeSet<String> {
    let mut s: BTreeSet<String> = r
        .pads
        .iter()
        .map(|p| format!("{}.{}", p.part, p.pad))
        .chain(r.vias.iter().map(|v| via_name(v.at)))
        .collect();
    let b = &r.bounds;
    if b.min.x <= area.min.x + EDGE
        || b.min.y <= area.min.y + EDGE
        || b.max.x >= area.max.x - EDGE
        || b.max.y >= area.max.y - EDGE
    {
        s.insert(OUT.to_string());
    }
    s
}

fn via_name(at: Pt) -> String {
    format!("via {:.4},{:.4}", at.x, at.y)
}

/// The regions of `now` that parted from a region of `was` and hold pads or vias ([module](self)).
fn new_islands(whole: &Board, was: &IslandsReport, now: &IslandsReport, area: Rect) -> Vec<Pocket> {
    let corner = whole.outline.min;
    let mut out = Vec::new();
    for f in &now.fills {
        let before: Vec<BTreeSet<String>> = was
            .fills
            .iter()
            .filter(|w| w.layer_name == f.layer_name && w.net == f.net)
            .flat_map(|w| w.regions.iter().map(|r| held(r, &area)))
            .collect();
        let known: BTreeSet<&String> = before.iter().flatten().collect();
        // Each region with what it holds that the fill held before (a via the change made is
        // new, and holds nothing of the old fill).
        let parts: Vec<(&netsmith_pcb::FillRegion, BTreeSet<String>)> = f
            .regions
            .iter()
            .map(|r| {
                let h: BTreeSet<String> = held(r, &area)
                    .into_iter()
                    .filter(|t| known.contains(t))
                    .collect();
                (r, h)
            })
            .filter(|(_, h)| !h.is_empty())
            .collect();
        for b in &before {
            // The parts of `b` that hold a pad or a via (the tokens of two regions are apart,
            // but for the box's edge).
            let of_b: Vec<&(&netsmith_pcb::FillRegion, BTreeSet<String>)> = parts
                .iter()
                .filter(|(_, h)| h.iter().any(|t| t != OUT) && h.is_subset(b))
                .collect();
            // A region that reached the box's edge keeps the fill there: each part of it that
            // does not is cut off. A region that did not keeps it in its part that joins the
            // most.
            let keeper = if b.contains(OUT) {
                None
            } else if of_b.len() < 2 {
                continue;
            } else {
                of_b.iter()
                    .enumerate()
                    .max_by(|(_, x), (_, y)| {
                        x.1.len()
                            .cmp(&y.1.len())
                            .then(x.0.area.total_cmp(&y.0.area))
                    })
                    .map(|(k, _)| k)
            };
            for (k, (r, h)) in of_b.iter().enumerate() {
                if Some(k) == keeper || h.contains(OUT) {
                    continue;
                }
                let place = |p: Pt| {
                    let p = p - corner;
                    [round(p.x), round(p.y)]
                };
                out.push(Pocket {
                    layer: f.layer.clone(),
                    fill: source_word(whole, &f.layer_name, &f.net).to_string(),
                    net: f.net.clone(),
                    at: place(r.at),
                    area: round(r.area),
                    pads: r
                        .pads
                        .iter()
                        .map(|p| format!("{}.{}", p.part, p.pad))
                        .collect(),
                    vias: r
                        .vias
                        .iter()
                        .map(|v| PocketVia { at: place(v.at) })
                        .collect(),
                });
            }
        }
    }
    out
}

/// What the fill of `net` on `layer` (KiCad's name) is on the whole board: a zone of the routes
/// file, else the plane or the pour the process states.
fn source_word(whole: &Board, layer: &str, net: &str) -> &'static str {
    if whole.replaced(layer, net)
        || whole
            .zones
            .iter()
            .any(|z| z.net == net && z.layers.iter().any(|l| l == layer))
    {
        "zone"
    } else if whole
        .planes
        .iter()
        .any(|p| p.layer == *layer && p.net == net)
    {
        "plane"
    } else if whole
        .pours
        .iter()
        .any(|p| p.layer.name() == layer && p.net == net)
    {
        "pour"
    } else {
        "zone"
    }
}

/// To 0.0001 mm.
fn round(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
