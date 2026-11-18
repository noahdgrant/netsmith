//! Where a change of `routes tidy` can be seen by the design rules check (issue #1225): a grid of
//! the board's items built once, and the neighbourhood of a change, out of which the check of a
//! group of changes takes the items it checks.
//!
//! **The neighbourhood.** Every rule of the check that a track or a via takes part in judges it
//! against copper, holes, mask openings or pads no further from it than the board's largest
//! space ([`reach`]): a clearance of copper, of a hole or of a net class, the space between
//! pads, mask and holes, a zone's clearance, and how near a pad a track may neck down. The
//! neighbourhood of a change is the box round the copper it removes and adds, grown by that
//! space; a finding the change makes or takes away has its place in it. The check of a group
//! takes every item whose box meets the neighbourhood of one of its changes grown by the space
//! once more (so that what touches an item of the neighbourhood is there too), and compares the
//! findings with a place in a neighbourhood, with and without the changes. What lies further
//! off is the same on both boards, and is not looked at.
//!
//! **Zones.** Each zone, the planes and the pours too (as the zones the routes file would hold
//! for them, `netsmith_pcb::process_zones`), is cut to the boxes of the items the check takes
//! ([`clusters`], [`clip`]), so that a check fills the zones of its neighbourhoods and not of the
//! board: the fill near a change is kept from the same copper as the whole board's.
//!
//! A rule whose finding can be far from the copper that makes it (a net's pieces, a fill's
//! islands, the current a branch of a net carries) is the whole board's: the check of the
//! whole board after each pass ([`super`]) sees what the check of a neighbourhood cannot.

use netsmith_pcb::{Board, Pt, Rect, Track, Via};

/// The side of a square of the grid, in mm, at the least.
const CELL: f64 = 2.0;

/// The largest space the design rules check holds a track or a via to, in mm: the
/// neighbourhood's margin (the module's header).
pub fn reach(board: &Board) -> f64 {
    let r = &board.rules;
    let nets = board.net_clearances.iter().map(|(_, c)| *c);
    let zones = board.zones.iter().map(|z| z.clearance);
    [
        r.clearance,
        r.hole_clearance,
        r.plated_hole_clearance,
        r.pad_to_pad,
        r.mask_to_track,
        r.hole_to_hole,
        r.via_hole_to_hole,
        r.mask_bridge,
        r.plane_clearance,
        r.silk_to_pad,
        netsmith_router::route::NECK_REACH,
    ]
    .into_iter()
    .chain(nets)
    .chain(zones)
    .filter(|v| v.is_finite())
    .fold(0.0, f64::max)
        + 0.01
}

/// The box round a track's copper.
pub fn track_box(t: &Track) -> Rect {
    let r = Rect::around(t.centre_line()).unwrap_or(Rect::new(t.from, t.to));
    r.expanded(t.width / 2.0)
}

/// The box round a via's copper.
pub fn via_box(v: &Via) -> Rect {
    Rect::new(v.at, v.at).expanded(v.diameter / 2.0)
}

/// The box round a part: its courtyard, body, pads and mask openings.
pub fn part_box(part: &netsmith_pcb::Placed) -> Rect {
    let mut r = part.courtyard().union(&part.body());
    for p in &part.footprint.pads {
        r = r.union(&part.pad_bounds(p));
    }
    for m in part.mask_openings() {
        r = r.union(&m);
    }
    r
}

/// Whether two boxes share a point.
pub fn meets(a: &Rect, b: &Rect) -> bool {
    a.min.x <= b.max.x && b.min.x <= a.max.x && a.min.y <= b.max.y && b.min.y <= a.max.y
}

/// Whether `p` lies in `r`.
pub fn holds(r: &Rect, p: Pt) -> bool {
    r.min.x <= p.x && p.x <= r.max.x && r.min.y <= p.y && p.y <= r.max.y
}

/// `rects` merged, two that meet into the box round both, until no two meet.
pub fn clusters(mut rects: Vec<Rect>) -> Vec<Rect> {
    loop {
        let mut merged = false;
        let mut out: Vec<Rect> = Vec::new();
        for r in rects {
            match out.iter_mut().find(|o| meets(o, &r)) {
                Some(o) => {
                    *o = o.union(&r);
                    merged = true;
                }
                None => out.push(r),
            }
        }
        rects = out;
        if !merged {
            return rects;
        }
    }
}

/// The part of the polygon `ring` inside `r` (Sutherland and Hodgman's clipping, one side of
/// the box at a time): a ring of fewer than three points when none is.
pub fn clip(ring: &[Pt], r: &Rect) -> Vec<Pt> {
    let at_x = |a: Pt, b: Pt, x: f64| Pt::new(x, a.y + (b.y - a.y) * (x - a.x) / (b.x - a.x));
    let at_y = |a: Pt, b: Pt, y: f64| Pt::new(a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y), y);
    let mut points: Vec<Pt> = ring.to_vec();
    for side in 0..4 {
        if points.is_empty() {
            break;
        }
        let inside = |p: Pt| match side {
            0 => p.x >= r.min.x,
            1 => p.x <= r.max.x,
            2 => p.y >= r.min.y,
            _ => p.y <= r.max.y,
        };
        let cross = |a: Pt, b: Pt| match side {
            0 => at_x(a, b, r.min.x),
            1 => at_x(a, b, r.max.x),
            2 => at_y(a, b, r.min.y),
            _ => at_y(a, b, r.max.y),
        };
        let mut out = Vec::with_capacity(points.len() + 4);
        for i in 0..points.len() {
            let (a, b) = (points[(i + points.len() - 1) % points.len()], points[i]);
            match (inside(a), inside(b)) {
                (true, true) => out.push(b),
                (true, false) => out.push(cross(a, b)),
                (false, true) => {
                    out.push(cross(a, b));
                    out.push(b);
                }
                (false, false) => {}
            }
        }
        out.dedup_by(|a, b| a.distance(*b) < 1e-9);
        points = out;
    }
    if points.len() > 1 && points[0].distance(points[points.len() - 1]) < 1e-9 {
        points.pop();
    }
    points
}

/// The zones of `zones` cut to each box of `boxes` (boxes that do not meet, [`clusters`]):
/// a zone for each box a zone's outline reaches into, with its holes cut so too.
pub fn clipped(zones: &[netsmith_pcb::BoardZone], boxes: &[Rect]) -> Vec<netsmith_pcb::BoardZone> {
    let mut out = Vec::new();
    for z in zones {
        let Some(round) = Rect::around(z.outline.iter().copied()) else {
            continue;
        };
        for b in boxes.iter().filter(|b| meets(b, &round)) {
            let outline = clip(&z.outline, b);
            if outline.len() < 3 {
                continue;
            }
            let mut zone = z.clone();
            zone.outline = outline;
            zone.holes = z
                .holes
                .iter()
                .map(|h| clip(h, b))
                .filter(|h| h.len() >= 3)
                .collect();
            out.push(zone);
        }
    }
    out
}

/// What the grid holds: a track of the copper, a via of it, or a part of the board, by its
/// place in its list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Item {
    Wire(usize),
    Hole(usize),
    Part(usize),
}

/// A uniform grid over the board, each square listing the items whose box meets it. An item
/// added later is put in with [`Grid::put`]; one removed stays listed, and the caller tests what
/// it finds against the copper as it is (a list may name an item twice, or one no longer
/// there).
pub struct Grid {
    origin: Pt,
    cell: f64,
    columns: usize,
    rows: usize,
    cells: Vec<Vec<Item>>,
}

impl Grid {
    /// An empty grid over `area`, in squares of `cell` mm at the least and no more than 512 a
    /// side.
    pub fn new(area: Rect, cell: f64) -> Grid {
        let cell = cell
            .max(CELL)
            .max(area.width() / 512.0)
            .max(area.height() / 512.0);
        let columns = ((area.width() / cell).ceil() as usize).max(1);
        let rows = ((area.height() / cell).ceil() as usize).max(1);
        Grid {
            origin: area.min,
            cell,
            columns,
            rows,
            cells: vec![Vec::new(); columns * rows],
        }
    }

    fn span(&self, r: &Rect) -> (usize, usize, usize, usize) {
        let at = |v: f64, n: usize| ((v / self.cell).floor().max(0.0) as usize).min(n - 1);
        (
            at(r.min.x - self.origin.x, self.columns),
            at(r.max.x - self.origin.x, self.columns),
            at(r.min.y - self.origin.y, self.rows),
            at(r.max.y - self.origin.y, self.rows),
        )
    }

    /// Lists `item` in every square its box `r` meets (a box off the grid in the squares of
    /// its edge).
    pub fn put(&mut self, item: Item, r: &Rect) {
        let (x0, x1, y0, y1) = self.span(r);
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.cells[y * self.columns + x].push(item);
            }
        }
    }

    /// The items listed in the squares `r` meets, each once, in order.
    pub fn near(&self, r: &Rect) -> Vec<Item> {
        let (x0, x1, y0, y1) = self.span(r);
        let mut out: Vec<Item> = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                out.extend_from_slice(&self.cells[y * self.columns + x]);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ring_is_cut_to_a_box_and_boxes_that_meet_are_one() {
        let square =
            |a: f64, b: f64| vec![Pt::new(a, a), Pt::new(b, a), Pt::new(b, b), Pt::new(a, b)];
        let r = Rect::new(Pt::new(2.0, 2.0), Pt::new(4.0, 3.0));
        let cut = clip(&square(0.0, 10.0), &r);
        assert_eq!(Rect::around(cut.iter().copied()), Some(r), "{cut:?}");
        assert_eq!(cut.len(), 4, "{cut:?}");
        // A ring off the box is nothing; one inside is itself.
        assert!(clip(&square(5.0, 6.0), &r).len() < 3);
        assert_eq!(clip(&square(2.5, 2.9), &r), square(2.5, 2.9));
        let boxes = clusters(vec![
            Rect::new(Pt::new(0.0, 0.0), Pt::new(1.0, 1.0)),
            Rect::new(Pt::new(5.0, 5.0), Pt::new(6.0, 6.0)),
            Rect::new(Pt::new(0.5, 0.5), Pt::new(2.0, 2.0)),
            Rect::new(Pt::new(1.5, 1.5), Pt::new(5.5, 5.5)),
        ]);
        assert_eq!(boxes, vec![Rect::new(Pt::new(0.0, 0.0), Pt::new(6.0, 6.0))]);
    }

    #[test]
    fn the_grid_finds_what_meets_a_box_and_nothing_far() {
        let mut g = Grid::new(Rect::new(Pt::new(0.0, 0.0), Pt::new(100.0, 50.0)), 2.0);
        g.put(
            Item::Wire(0),
            &Rect::new(Pt::new(1.0, 1.0), Pt::new(3.0, 1.5)),
        );
        g.put(
            Item::Hole(4),
            &Rect::new(Pt::new(60.0, 40.0), Pt::new(61.0, 41.0)),
        );
        // A box off the grid is listed at its edge.
        g.put(
            Item::Part(2),
            &Rect::new(Pt::new(-5.0, -5.0), Pt::new(-4.0, -4.0)),
        );
        let near = g.near(&Rect::new(Pt::new(0.0, 0.0), Pt::new(2.5, 2.5)));
        assert_eq!(near, vec![Item::Wire(0), Item::Part(2)]);
        assert_eq!(
            g.near(&Rect::new(Pt::new(59.0, 39.0), Pt::new(60.5, 40.5))),
            vec![Item::Hole(4)]
        );
        assert!(
            g.near(&Rect::new(Pt::new(30.0, 20.0), Pt::new(31.0, 21.0)))
                .is_empty()
        );
    }
}
