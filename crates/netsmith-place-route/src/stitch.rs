//! Stitching vias: a grid of vias that joins the pour of an outer layer to the other layers
//! of its net (ADR 0020, item 2).
//!
//! They are added after routing, in the places the routed copper, the pads, the holes, the
//! board edge and the keepouts leave free. KiCad fills the pour round them. A via goes on
//! the grid point, or the nearest free point within a third of the pitch of it, and a grid
//! point with none is left without.
//!
//! The grid is at the pour's `.stitch_pitch`, read at the top of the range, every other row
//! shifted by half a pitch. A via keeps from other nets' copper the space KiCad checks, and
//! from the holes, the pads, the board edge, the cutouts and the keepouts that forbid vias.
//! Where a pad sits on a pour of its own net the pad vias that the router drops beside a pad
//! on a plane net are kept: a pad inside a ring of tracks may stand on an island that KiCad's
//! fill removes, so the pad via is sound and not tidy. Dropping it where a pour reaches the
//! pad is an optimisation to take only with KiCad's unconnected-item check as the gate (ADR
//! 0020, decided by the owner on 2026-11-02).

use netsmith_pcb::board::{Board, CopperLayer, Via};
use netsmith_pcb::geometry::{Pt, Rect};

/// The grid the vias are put on, in mm.
const STEP: f64 = 0.05;
/// Space kept beyond the rules' clearance, for rounding.
const MARGIN: f64 = 0.05;
/// The way a pour needs to a via past the pads beside it, beyond the space it keeps from them.
const POUR_WAY: f64 = 0.25;
/// The distance from a point to a box, zero inside it.
fn to_rect(p: Pt, r: &Rect) -> f64 {
    let dx = (r.min.x - p.x).max(p.x - r.max.x).max(0.0);
    let dy = (r.min.y - p.y).max(p.y - r.max.y).max(0.0);
    dx.hypot(dy)
}

/// The distance from a point to a segment.
fn to_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let d = b - a;
    let len2 = d.x * d.x + d.y * d.y;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
    };
    p.distance(Pt::new(a.x + d.x * t, a.y + d.y * t))
}

/// What a via must keep clear of, gathered once.
struct Free<'a> {
    board: &'a Board,
    net: &'a str,
    /// Boxes of every pad.
    pads: Vec<Rect>,
    holes: Vec<(Pt, Pt, f64)>,
    keepouts: Vec<Rect>,
}

impl<'a> Free<'a> {
    fn of(board: &'a Board, net: &'a str) -> Self {
        let mut pads = Vec::new();
        let mut holes = Vec::new();
        for part in &board.parts {
            for pad in &part.footprint.pads {
                if pad.copper_layers() != (false, false) {
                    pads.push(part.pad_bounds(pad));
                }
                if let Some(hole) = part.hole(pad) {
                    holes.push(hole);
                }
            }
        }
        let keepouts = board
            .keepout_areas()
            .into_iter()
            .filter(|k| k.vias)
            .map(|k| k.rect)
            .collect();
        Self {
            board,
            net,
            pads,
            holes,
            keepouts,
        }
    }

    /// Whether a via may be centred at `p`, given the vias already on the board.
    fn allows(&self, p: Pt, vias: &[Via]) -> bool {
        let rules = &self.board.rules;
        let radius = rules.via_diameter / 2.0;
        let edge = rules.copper_to_edge + radius + MARGIN;
        if self.board.edge().depth(p) < edge {
            return false;
        }
        if self
            .board
            .cutout_shapes()
            .iter()
            .any(|(_, c)| to_rect(p, &c.rect) < edge)
        {
            return false;
        }
        if self
            .keepouts
            .iter()
            .any(|k| to_rect(p, k) < radius + MARGIN)
        {
            return false;
        }
        // Copper of other nets keeps the clearance, and a pad of any net keeps room for the
        // pour to reach the via past it: a via squeezed between two pads sits in a gap that
        // the pour cannot fill, and KiCad finds it connected to nothing on that layer.
        let clearance = rules.clearance + radius + MARGIN;
        let reachable = rules.clearance.max(rules.plane_clearance) + radius + MARGIN + POUR_WAY;
        if self.pads.iter().any(|r| to_rect(p, r) < reachable) {
            return false;
        }
        // A via's hole keeps its space from the holes of pads.
        let apart = rules.hole_to_hole + rules.via_drill / 2.0 + MARGIN;
        if self
            .holes
            .iter()
            .any(|&(a, b, r)| to_segment(p, a, b) - r < apart)
        {
            return false;
        }
        if self
            .board
            .tracks
            .iter()
            .any(|t| t.net != self.net && to_segment(p, t.from, t.to) - t.width / 2.0 < clearance)
        {
            return false;
        }
        vias.iter().all(|v| {
            let centres = p.distance(v.at);
            let holes = rules.via_hole_to_hole + rules.via_drill + MARGIN;
            let copper = if v.net == self.net {
                rules.via_diameter
            } else {
                rules.via_diameter + rules.clearance + MARGIN
            };
            centres >= holes.max(copper)
        })
    }
}

/// Whether a net has another layer to be stitched to: an inner plane, or the pour of the
/// other outer layer.
fn other_layers(board: &Board, net: &str, layer: CopperLayer) -> bool {
    board.planes.iter().any(|p| p.net == net)
        || board.pours.iter().any(|p| p.net == net && p.layer != layer)
}

/// Add stitching vias to every pour that states a pitch and has another layer on its net.
pub fn stitch(board: &mut Board) {
    let pours = board.pours.clone();
    for pour in &pours {
        let Some(pitch) = pour.stitch_pitch else {
            continue;
        };
        if !other_layers(board, &pour.net, pour.layer) {
            continue;
        }
        let rules = board.rules.clone();
        let free_vias = {
            let free = Free::of(board, &pour.net);
            let o = board.outline;
            let reach = rules.copper_to_edge + rules.via_diameter / 2.0 + MARGIN;
            let search = (pitch / 3.0 / STEP).floor() as i64;
            let mut found: Vec<Via> = Vec::new();
            // Two pours on one net would put a via on the grid twice; the first one's are
            // kept by the check against the vias already there.
            let all: Vec<Via> = board.vias.clone();
            let (mut y, mut row) = (o.min.y + reach, 0);
            while y <= o.max.y - reach + 1e-9 {
                // Every other row is shifted by half a pitch, so the vias stagger.
                let mut x = o.min.x + reach + if row % 2 == 1 { pitch / 2.0 } else { 0.0 };
                while x <= o.max.x - reach + 1e-9 {
                    let near = nearest_free(&free, Pt::new(x, y), search, &all, &found);
                    if let Some(at) = near {
                        found.push(Via {
                            net: pour.net.clone(),
                            at,
                            diameter: rules.via_diameter,
                            drill: rules.via_drill,
                        });
                    }
                    x += pitch;
                }
                y += pitch;
                row += 1;
            }
            found
        };
        board.vias.extend(free_vias);
    }
}

/// The free point nearest `p` within `search` steps of the via grid, if any.
fn nearest_free(free: &Free, p: Pt, search: i64, all: &[Via], found: &[Via]) -> Option<Pt> {
    let snap = |v: f64| (v / STEP).round() * STEP;
    let centre = Pt::new(snap(p.x), snap(p.y));
    let mut best: Option<(f64, Pt)> = None;
    let vias: Vec<Via> = all.iter().chain(found).cloned().collect();
    for dy in -search..=search {
        for dx in -search..=search {
            let q = Pt::new(centre.x + dx as f64 * STEP, centre.y + dy as f64 * STEP);
            let d = q.distance(p);
            if best.is_some_and(|(b, _)| b <= d) {
                continue;
            }
            if free.allows(q, &vias) {
                best = Some((d, q));
            }
        }
    }
    best.map(|(_, q)| q)
}
