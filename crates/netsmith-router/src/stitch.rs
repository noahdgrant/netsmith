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
//! The vias the router drops beside the pads of a plane net are there while the stitching is
//! placed. Where a pad sits on a pour of its own net, its via goes after the stitching
//! (`crate::pour_pads`, issue #834) if the pour, as filled and trimmed to its minimum width,
//! joins the pad to a via it covers: a pad inside a ring of tracks may stand on an island with
//! none, and keeps its via. ADR 0020 (decided by the owner on 2026-11-02) made KiCad's
//! unconnected-item check the gate of dropping it; the router's KiCad tests run that check.
//!
//! **A fence** (issue #1275): the ground vias of a `TrackPath` with a `.fence`, along both
//! sides of each of its tracks at that distance from each other, measured from the track's
//! start. Each goes as near the track as the rules let it: its centre is put half the track's
//! width, the space between the two nets and half a via out from the track's centre line, then
//! moved to the nearest point within a third of the distance that keeps every rule a
//! stitching via keeps; a place with none, or with a via of the ground within half the distance
//! already, is left without one. The ground is the net of the
//! pour on the track's layer, else the first rail net with an inner plane; a path with neither
//! gets no fence (`fab-notes.txt` says so, `Board::fence_ground`). The vias are placed after the
//! stitching, before the teardrops and the arcs, which keep clear of them.

use netsmith_pcb::{Board, CopperLayer, Pt, Rect, Via};

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
    /// The pour's clearance from other nets' copper, where the design states one (issue
    /// #1084): a via keeps it from their pads and tracks, so the fill reaches round it.
    pour_clearance: f64,
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
        // The design's keepouts and those of the parts' footprints (issue #1291).
        let keepouts = board
            .copper_keepouts()
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
            pour_clearance: 0.0,
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
        // A pad's net is not known here, so a pad keeps the largest clearance of any net
        // (issue #758).
        let clearance = |net: &str| {
            self.board
                .clearance_between(net, self.net)
                .max(self.pour_clearance)
                + radius
                + MARGIN
        };
        let reachable = self
            .board
            .widest_clearance()
            .max(rules.plane_clearance)
            .max(self.pour_clearance)
            + radius
            + MARGIN
            + POUR_WAY;
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
            .any(|t| t.net != self.net && t.distance_to(p) - t.width / 2.0 < clearance(&t.net))
        {
            return false;
        }
        vias.iter().all(|v| {
            let centres = p.distance(v.at);
            let holes = rules.via_hole_to_hole + rules.via_drill + MARGIN;
            let copper = if v.net == self.net {
                rules.via_diameter
            } else {
                rules.via_diameter + self.board.clearance_between(&v.net, self.net) + MARGIN
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
        // A net a routes file locks gets no copper (`netsmith_pcb::Routes`).
        if board.fixed.is_locked(&pour.net) {
            continue;
        }
        if !other_layers(board, &pour.net, pour.layer) {
            continue;
        }
        let rules = board.rules.clone();
        let free_vias = {
            let free = Free {
                pour_clearance: pour.clearance.unwrap_or(0.0),
                ..Free::of(board, &pour.net)
            };
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
                            span: None,
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

/// Add the ground vias of every `TrackPath` with a fence (the module's header). Returns how many
/// were placed.
pub fn fence(board: &mut Board) -> usize {
    let paths: Vec<(Vec<String>, f64)> = board
        .track_paths
        .iter()
        .filter_map(|p| Some((p.nets.clone(), p.fence?)))
        .collect();
    let mut placed = 0;
    for (nets, pitch) in paths {
        let tracks: Vec<netsmith_pcb::Track> = board
            .tracks
            .iter()
            .filter(|t| nets.contains(&t.net))
            .cloned()
            .collect();
        for t in tracks {
            let Some(ground) = board.fence_ground(t.layer) else {
                continue;
            };
            if board.fixed.is_locked(&ground) {
                continue;
            }
            let rules = board.rules.clone();
            let out = t.width / 2.0
                + board.clearance_between(&ground, &t.net)
                + rules.via_diameter / 2.0
                + MARGIN;
            let search = ((pitch / 3.0) / STEP).floor().max(1.0) as i64;
            let line = t.centre_line();
            let mut found: Vec<Via> = Vec::new();
            {
                let free = Free::of(board, &ground);
                let all: Vec<Via> = board.vias.clone();
                for w in line.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    let length = a.distance(b);
                    if length < 1e-9 {
                        continue;
                    }
                    let u = Pt::new((b.x - a.x) / length, (b.y - a.y) / length);
                    let normal = Pt::new(-u.y, u.x);
                    let mut along = 0.0;
                    while along <= length + 1e-9 {
                        let on = Pt::new(a.x + u.x * along, a.y + u.y * along);
                        for side in [1.0, -1.0] {
                            let p =
                                Pt::new(on.x + normal.x * out * side, on.y + normal.y * out * side);
                            // A via of the ground already near (the last track's, a
                            // stitching via) stands for this one.
                            let near = all
                                .iter()
                                .chain(&found)
                                .any(|v| v.net == ground && v.at.distance(p) < pitch / 2.0);
                            if near {
                                continue;
                            }
                            if let Some(at) = nearest_free(&free, p, search, &all, &found) {
                                found.push(Via {
                                    net: ground.clone(),
                                    at,
                                    diameter: rules.via_diameter,
                                    drill: rules.via_drill,
                                    span: None,
                                });
                            }
                        }
                        along += pitch;
                    }
                }
            }
            placed += found.len();
            board.vias.extend(found);
        }
    }
    placed
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

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_pcb::{BoardPour, Track};

    /// The LED board with its parts taken away and a ground pour on the front, with a straight
    /// 8 mm track of `VCC` across its middle (the board is 15 mm by 7 mm) on a path fenced every `pitch` mm.
    fn fenced(pitch: Option<f64>, pour: bool) -> Board {
        let mut board = crate::board_from_design(
            &crate::testlib::led_design(),
            &netsmith_kicad::Mapping::standard(),
            &crate::testlib::fixtures(),
        )
        .unwrap();
        board.parts.clear();
        board.vias.clear();
        board.pours.clear();
        board.planes.clear();
        if pour {
            board.pours.push(BoardPour {
                layer: CopperLayer::Front,
                net: "GND".into(),
                stitch_pitch: None,
                thermal_gap: None,
                spoke_width: None,
                clearance: None,
                min_width: None,
                pads: None,
            });
        }
        let c = board.outline.centre();
        board.tracks.push(Track::straight(
            "VCC",
            CopperLayer::Front,
            Pt::new(c.x - 4.0, c.y),
            Pt::new(c.x + 4.0, c.y),
            0.3,
        ));
        board.track_paths.push(netsmith_core::TrackPath {
            path: "rf".into(),
            nets: vec!["VCC".into()],
            parts: Vec::new(),
            ohms: 50.0,
            budget_length: None,
            budget_loss: None,
            frequency: None,
            corner: netsmith_core::TrackCorner::Rounded,
            fence: pitch,
        });
        board
    }

    #[test]
    fn a_fence_puts_ground_vias_on_both_sides_of_the_track_at_its_pitch() {
        let mut board = fenced(Some(2.0), true);
        let placed = fence(&mut board);
        // 8 mm at 2 mm: 5 places, a via on each side of each.
        assert_eq!(placed, 10, "{:?}", board.vias);
        let t = board.tracks[0].clone();
        let rules = board.rules.clone();
        for v in &board.vias {
            assert_eq!(v.net, "GND");
            // Clear of the track by the rules' space, and no further than a third of the pitch
            // past where it was aimed.
            let edge = t.distance_to(v.at) - t.width / 2.0 - v.diameter / 2.0;
            assert!(
                edge >= board.clearance_between("GND", "VCC") - 1e-9,
                "{edge}"
            );
            assert!(
                edge <= rules.clearance + MARGIN + 2.0 / 3.0 + 1e-9,
                "{edge}"
            );
        }
        // Run again, the vias there stand for the fence: none more.
        assert_eq!(fence(&mut board), 0);
    }

    #[test]
    fn a_path_with_no_fence_or_no_ground_gets_no_vias() {
        let mut board = fenced(None, true);
        assert_eq!(fence(&mut board), 0);
        let mut board = fenced(Some(2.0), false);
        assert_eq!(fence(&mut board), 0);
        assert!(board.vias.is_empty());
        assert!(
            board.track_path_lines()[0].contains("no fence: no pour on its layer"),
            "{:?}",
            board.track_path_lines()
        );
    }
}
