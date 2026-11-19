//! Pads that other parts wall in, found where the parts are, before anything is routed (issue
//! #1313): [`Board::walled_pads`].
//!
//! The router says `walled in by pad(s) ...` of a line it could not route
//! (`netsmith_router`'s `walls`), after it has tried. Here the same fault is looked for on the
//! placed board, from the pads alone, so that `fab` and `layout check` name the parts to move
//! before a route is tried.
//!
//! A surface-mount pad on a net that reaches another part is measured on its copper layer, in a
//! window of [`ESCAPE`] (1.5 mm, the router's escape lanes) round it, on a grid of [`CELL`]. A
//! cell is closed to a track's centre where it is nearer another net's pad (a pad of the same
//! part too, and a pad with no net) than the process's clearance and half the router's track,
//! or nearer the outline than the copper to edge space and half a track. The open cells the pad
//! reaches are flooded from the pad. The pad has a way out when the flood reaches the edge of
//! the window, or a cell outside the pad where one of the router's vias fits (no other net's pad
//! nearer than the clearance and the via's radius, inside the edge's space). A pad with no way
//! out is walled in, and the pads that close the cells next to the flood, and the edge, are
//! what wall it in. A pad is said only when a pad of another part is in its wall: a pad its own
//! footprint walls in is the footprint's, not the placement's.
//!
//! Not measured, so not said: a through-hole pad (it reaches every layer), a pad where the
//! process allows a via in it and a via fits on it, the other face under a via, keepouts and
//! cutouts, and copper the routes file fixes. The grid can call a gap within a cell of the
//! limit open or closed. It is a warning: the router may still find a way the window does not
//! hold.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Serialize;

use crate::board::{Board, member};
use crate::geometry::{Pt, Rect};

/// How far round a pad its way out is looked for, in mm: the length of the router's escape
/// lanes (`LANE_LENGTH` in `netsmith_router::route`).
pub(crate) const ESCAPE: f64 = 1.5;
/// The side of a cell of the grid, in mm.
pub(crate) const CELL: f64 = 0.025;

/// A part whose pads wall in another part's pad, with the numbers of those pads.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WallPart {
    pub refdes: String,
    pub path: String,
    pub pads: Vec<String>,
}

/// A pad with no way out on its layer ([`Board::walled_pads`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WalledPad {
    /// The part the pad is on.
    pub refdes: String,
    pub path: String,
    /// The pad's number and its net.
    pub pad: String,
    pub net: String,
    /// The parts whose pads close its way out, the pad's own part last when it is one of them.
    pub by: Vec<WallPart>,
    /// Whether the board's edge closes it too.
    pub edge: bool,
    /// The track and the space it was measured with, in mm.
    pub track: f64,
    pub clearance: f64,
}

impl fmt::Display for WalledPad {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut by: Vec<String> = self
            .by
            .iter()
            .map(|w| {
                format!(
                    "{} (pad{} {})",
                    member(&w.path, &w.refdes),
                    if w.pads.len() == 1 { "" } else { "s" },
                    w.pads.join(", ")
                )
            })
            .collect();
        if self.edge {
            by.push("the board's edge".to_string());
        }
        let others: Vec<&str> = self
            .by
            .iter()
            .filter(|w| w.refdes != self.refdes)
            .map(|w| w.refdes.as_str())
            .collect();
        write!(
            f,
            "pad {} of {} (net `{}`) is walled in by {}: no {} mm track leaves it {} mm from other copper and no via fits beside it, within {} mm; move {} away from it, or give the parts a `.routing_margin`",
            self.pad,
            member(&self.path, &self.refdes),
            self.net,
            by.join(", "),
            crate::mm_text(self.track),
            crate::mm_text(self.clearance),
            crate::mm_text(ESCAPE),
            others.join(", ")
        )
    }
}

/// What closes a cell: the index of a pad among the obstacles, or the edge.
#[derive(Clone, Copy, PartialEq)]
enum Closed {
    Open,
    Pad(usize),
    Edge,
}

impl Board {
    /// The surface-mount pads that other parts wall in where the parts are now, by the
    /// module's rule, in the order of the parts and their pads. Empty when every pad has a
    /// way out. Reads the placement only; places and routes nothing.
    pub fn walled_pads(&self) -> Vec<WalledPad> {
        let rules = &self.rules;
        let half_track = rules.track_width / 2.0;
        let via_r = rules.via_diameter / 2.0;
        // The net of each pad, by part and number.
        let mut net_of: BTreeMap<(&str, &str), &str> = BTreeMap::new();
        let mut parts_of: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for n in &self.nets {
            for p in &n.pads {
                net_of.insert((p.refdes.as_str(), p.number.as_str()), n.name.as_str());
                parts_of
                    .entry(n.name.as_str())
                    .or_default()
                    .insert(p.refdes.as_str());
            }
        }
        // Every pad with copper: its part, number, net, box, and the layers it is on.
        struct Copper<'a> {
            part: usize,
            number: &'a str,
            net: Option<&'a str>,
            bounds: Rect,
            front: bool,
            back: bool,
            hole: bool,
        }
        let mut copper: Vec<Copper> = Vec::new();
        for (i, part) in self.parts.iter().enumerate() {
            for pad in &part.footprint.pads {
                let (front, back) = part.pad_layers(pad);
                if !front && !back {
                    continue;
                }
                copper.push(Copper {
                    part: i,
                    number: pad.number.as_str(),
                    net: net_of
                        .get(&(part.refdes.as_str(), pad.number.as_str()))
                        .copied(),
                    bounds: part.pad_bounds(pad),
                    front,
                    back,
                    hole: pad.drill.is_some(),
                });
            }
        }
        let outline = self.outline;
        let mut found = Vec::new();
        for (k, pad) in copper.iter().enumerate() {
            netsmith_core::job::checkpoint();
            let Some(net) = pad.net else { continue };
            if pad.hole || pad.front == pad.back {
                continue;
            }
            let refdes = self.parts[pad.part].refdes.as_str();
            // A pad whose net ends on its own part alone has no line to leave by.
            if parts_of
                .get(net)
                .is_none_or(|parts| parts.iter().all(|p| *p == refdes))
            {
                continue;
            }
            if rules.via_in_pad
                && pad.bounds.width() >= rules.via_diameter
                && pad.bounds.height() >= rules.via_diameter
            {
                continue;
            }
            let window = pad.bounds.expanded(ESCAPE);
            let nx = (window.width() / CELL).ceil() as usize;
            let ny = (window.height() / CELL).ceil() as usize;
            let centre = |ix: usize, iy: usize| {
                Pt::new(
                    window.min.x + (ix as f64 + 0.5) * CELL,
                    window.min.y + (iy as f64 + 0.5) * CELL,
                )
            };
            let inside =
                |r: &Rect, p: Pt| p.x > r.min.x && p.x < r.max.x && p.y > r.min.y && p.y < r.max.y;
            let mut track = vec![Closed::Open; nx * ny];
            let mut via = vec![false; nx * ny];
            // The other nets' pads on the pad's layer, near enough to close a cell.
            let reach = window.expanded(rules.clearance + via_r);
            let obstacles: Vec<usize> = copper
                .iter()
                .enumerate()
                .filter(|(j, q)| {
                    *j != k
                        && (q.net.is_none() || q.net != pad.net)
                        && ((pad.front && q.front) || (pad.back && q.back))
                        && q.bounds.overlaps(&reach)
                })
                .map(|(j, _)| j)
                .collect();
            let edge_track =
                Rect::new(outline.min, outline.max).expanded(-(rules.copper_to_edge + half_track));
            let edge_via =
                Rect::new(outline.min, outline.max).expanded(-(rules.copper_to_edge + via_r));
            for iy in 0..ny {
                for ix in 0..nx {
                    let c = centre(ix, iy);
                    let at = iy * nx + ix;
                    if !inside(&edge_track, c) {
                        track[at] = Closed::Edge;
                    }
                    if !inside(&edge_via, c) {
                        via[at] = true;
                    }
                }
            }
            // The cells whose centres are inside `r`, as ranges of the grid.
            let cells = |r: &Rect| {
                let lo =
                    |v: f64, min: f64| (((v - min) / CELL - 0.5).floor() + 1.0).max(0.0) as usize;
                let hi = |v: f64, min: f64, n: usize| {
                    ((((v - min) / CELL - 0.5).ceil()).max(0.0) as usize).min(n)
                };
                (
                    lo(r.min.x, window.min.x)..hi(r.max.x, window.min.x, nx),
                    lo(r.min.y, window.min.y)..hi(r.max.y, window.min.y, ny),
                )
            };
            for &j in &obstacles {
                let b = &copper[j].bounds;
                let (xs, ys) = cells(&b.expanded(rules.clearance + half_track));
                for iy in ys {
                    for ix in xs.clone() {
                        let at = iy * nx + ix;
                        if track[at] == Closed::Open {
                            track[at] = Closed::Pad(j);
                        }
                    }
                }
                let (xs, ys) = cells(&b.expanded(rules.clearance + via_r));
                for iy in ys {
                    for ix in xs.clone() {
                        via[iy * nx + ix] = true;
                    }
                }
            }
            // Flood from the pad's own cells, which a track starts in whatever is near.
            let own_via = pad.bounds.expanded(via_r);
            let mut seen = vec![false; nx * ny];
            let mut queue: Vec<usize> = Vec::new();
            for iy in 0..ny {
                for ix in 0..nx {
                    if inside(&pad.bounds, centre(ix, iy)) {
                        let at = iy * nx + ix;
                        seen[at] = true;
                        queue.push(at);
                    }
                }
            }
            let mut out = false;
            let mut wall: BTreeSet<usize> = BTreeSet::new();
            let mut edge = false;
            while let Some(at) = queue.pop() {
                let (ix, iy) = (at % nx, at / nx);
                if ix == 0 || iy == 0 || ix + 1 == nx || iy + 1 == ny {
                    out = true;
                    break;
                }
                if !via[at] && !inside(&own_via, centre(ix, iy)) {
                    out = true;
                    break;
                }
                for next in [at - 1, at + 1, at - nx, at + nx] {
                    if seen[next] {
                        continue;
                    }
                    match track[next] {
                        Closed::Open => {
                            seen[next] = true;
                            queue.push(next);
                        }
                        Closed::Pad(j) => {
                            wall.insert(j);
                        }
                        Closed::Edge => edge = true,
                    }
                }
            }
            if out {
                continue;
            }
            let mut by: BTreeMap<usize, BTreeSet<&str>> = BTreeMap::new();
            for j in wall {
                by.entry(copper[j].part)
                    .or_default()
                    .insert(copper[j].number);
            }
            if by.keys().all(|&p| p == pad.part) {
                continue;
            }
            let own = self.parts[pad.part].clone();
            let mut by: Vec<WallPart> = by
                .into_iter()
                .map(|(p, pads)| WallPart {
                    refdes: self.parts[p].refdes.clone(),
                    path: self.parts[p].path.clone(),
                    pads: pads.into_iter().map(str::to_string).collect(),
                })
                .collect();
            // The pad's own part last: the others are the ones to move.
            by.sort_by_key(|w| w.refdes == own.refdes);
            found.push(WalledPad {
                refdes: own.refdes.clone(),
                path: own.path.clone(),
                pad: pad.number.to_string(),
                net: net.to_string(),
                by,
                edge,
                track: rules.track_width,
                clearance: rules.clearance,
            });
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::tests::led_board;

    /// The LED board with four one-pad copies of the part `refdes` (pads with no net) put
    /// round pad `number` of that part, left of it, above, below and right of it, `gap` mm of
    /// space from the pad's copper to theirs.
    fn walled(refdes: &str, number: &str, gap: f64) -> (Board, Rect) {
        let mut board = led_board();
        // A process with no via in a pad: a via in it is the router's last way out.
        board.rules.via_in_pad = false;
        let i = board.parts.iter().position(|p| p.refdes == refdes).unwrap();
        let part = board.parts[i].clone();
        let pad = part
            .footprint
            .pads
            .iter()
            .find(|p| p.number == number)
            .unwrap();
        let target = part.pad_bounds(pad);
        for (n, dir) in [(-1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (1.0, 0.0)]
            .into_iter()
            .enumerate()
        {
            // The copy's pad 1 is put beside the target, `gap` from it along `dir`.
            let mut copy = part.clone();
            copy.refdes = format!("X{n}");
            copy.path = format!("wall{n}");
            // Its first pad alone, so that the rest of it is nowhere near the target.
            copy.footprint.pads.truncate(1);
            let first = copy.pad_bounds(&copy.footprint.pads[0]);
            let shift = Pt::new(
                dir.0 * (target.width() / 2.0 + first.width() / 2.0 + gap),
                dir.1 * (target.height() / 2.0 + first.height() / 2.0 + gap),
            );
            copy.at = copy.at + (target.centre() + shift - first.centre());
            board.parts.push(copy);
        }
        (board, target)
    }

    #[test]
    fn the_placed_led_board_has_no_pad_walled_in() {
        assert_eq!(led_board().walled_pads(), vec![]);
    }

    #[test]
    fn a_pad_with_other_parts_pressed_round_it_is_walled_in_and_names_them() {
        let board = led_board();
        // A part with two pads on nets that reach other parts.
        let r = board
            .parts
            .iter()
            .find(|p| p.refdes.starts_with('R'))
            .unwrap()
            .refdes
            .clone();
        let gap = board.rules.clearance + 0.01;
        let (board, _) = walled(&r, "1", gap);
        let walled = board.walled_pads();
        let found = walled
            .iter()
            .find(|w| w.refdes == r && w.pad == "1")
            .unwrap_or_else(|| panic!("pad 1 of {r} is walled in: {walled:?}"));
        let names: Vec<&str> = found.by.iter().map(|w| w.refdes.as_str()).collect();
        assert!(names.contains(&"X0"), "{names:?}");
        let text = found.to_string();
        assert!(text.contains(&format!("pad 1 of {r}")), "{text}");
        assert!(text.contains("X0"), "{text}");
        assert!(text.contains("`.routing_margin`"), "{text}");
    }

    #[test]
    fn a_walled_pad_a_via_in_it_reaches_is_not_said_where_the_process_allows_one() {
        let board = led_board();
        let gap = board.rules.clearance + 0.01;
        let (mut board, _) = walled("R1", "1", gap);
        assert!(board.walled_pads().iter().any(|w| w.refdes == "R1"));
        board.rules.via_in_pad = true;
        assert!(!board.walled_pads().iter().any(|w| w.refdes == "R1"));
    }

    #[test]
    fn the_same_parts_with_room_for_a_track_between_them_wall_nothing_in() {
        let board = led_board();
        let r = board
            .parts
            .iter()
            .find(|p| p.refdes.starts_with('R'))
            .unwrap()
            .refdes
            .clone();
        let rules = &board.rules;
        let gap = 2.0 * rules.clearance + rules.track_width + 4.0 * CELL;
        let (board, _) = walled(&r, "1", gap);
        assert!(
            !board
                .walled_pads()
                .iter()
                .any(|w| w.refdes == r && w.pad == "1"),
            "{:?}",
            board.walled_pads()
        );
    }
}
