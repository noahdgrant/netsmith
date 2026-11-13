//! The grid, the windows and the masks: what a cell is open to, for one line.

use super::*;

/// Something that copper must keep clear of.
#[derive(Clone, Copy)]
pub(crate) enum Form {
    Rect(Rect),
    /// A track: its two ends, and half its width.
    Segment(Pt, Pt, f64),
    Circle(Pt, f64),
}

impl Form {
    /// How far a point is from the form's edge, zero inside it.
    pub(crate) fn distance(&self, p: Pt) -> f64 {
        match self {
            Form::Rect(r) => {
                let dx = (r.min.x - p.x).max(p.x - r.max.x).max(0.0);
                let dy = (r.min.y - p.y).max(p.y - r.max.y).max(0.0);
                dx.hypot(dy)
            }
            Form::Segment(a, b, half) => {
                let ab = *b - *a;
                let len2 = ab.x * ab.x + ab.y * ab.y;
                let t = if len2 == 0.0 {
                    0.0
                } else {
                    (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
                };
                let nearest = Pt::new(a.x + ab.x * t, a.y + ab.y * t);
                (p.distance(nearest) - half).max(0.0)
            }
            Form::Circle(c, radius) => (p.distance(*c) - radius).max(0.0),
        }
    }

    /// How far a track's centre line from `a` to `b` is from the form's edge, zero if it
    /// touches or crosses the form. Exact, not on the grid.
    pub(crate) fn distance_to_line(&self, a: Pt, b: Pt) -> f64 {
        match self {
            Form::Rect(r) => {
                let inside =
                    |p: Pt| r.min.x <= p.x && p.x <= r.max.x && r.min.y <= p.y && p.y <= r.max.y;
                if inside(a) || inside(b) {
                    return 0.0;
                }
                let corners = [
                    r.min,
                    Pt::new(r.max.x, r.min.y),
                    r.max,
                    Pt::new(r.min.x, r.max.y),
                ];
                (0..4)
                    .map(|k| line_gap(a, b, corners[k], corners[(k + 1) % 4]))
                    .fold(f64::MAX, f64::min)
            }
            Form::Segment(c, d, half) => (line_gap(a, b, *c, *d) - half).max(0.0),
            Form::Circle(c, radius) => (point_to_line(*c, a, b) - radius).max(0.0),
        }
    }

    pub(crate) fn bounds(&self) -> Rect {
        match self {
            Form::Rect(r) => *r,
            Form::Segment(a, b, half) => Rect::new(
                Pt::new(a.x.min(b.x), a.y.min(b.y)),
                Pt::new(a.x.max(b.x), a.y.max(b.y)),
            )
            .expanded(*half),
            Form::Circle(c, r) => Rect::new(*c, *c).expanded(*r),
        }
    }
}

/// How far a point is from the segment from `a` to `b`.
pub(super) fn point_to_line(p: Pt, a: Pt, b: Pt) -> f64 {
    let ab = b - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
    };
    p.distance(Pt::new(a.x + ab.x * t, a.y + ab.y * t))
}

/// The least distance between the segments `a`-`b` and `c`-`d`, zero if they cross.
pub(super) fn line_gap(a: Pt, b: Pt, c: Pt, d: Pt) -> f64 {
    let side = |p: Pt, q: Pt, r: Pt| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (side(a, b, c), side(a, b, d));
    let (d3, d4) = (side(c, d, a), side(c, d, b));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    [
        point_to_line(a, c, d),
        point_to_line(b, c, d),
        point_to_line(c, a, b),
        point_to_line(d, a, b),
    ]
    .into_iter()
    .fold(f64::MAX, f64::min)
}

pub(crate) struct Obstacle {
    /// The net it carries, as an index into the board's nets.
    pub(crate) net: Option<usize>,
    pub(crate) layers: [bool; 2],
    pub(crate) form: Form,
    pub(crate) clearance: f64,
}

pub(super) struct Grid {
    pub(super) origin: Pt,
    pub(super) nx: usize,
    pub(super) ny: usize,
}

impl Grid {
    pub(super) fn over(outline: &Rect) -> Grid {
        Grid {
            origin: outline.min,
            nx: (outline.width() / PITCH).floor() as usize + 1,
            ny: (outline.height() / PITCH).floor() as usize + 1,
        }
    }

    pub(super) fn centre(&self, i: usize, j: usize) -> Pt {
        Pt::new(
            self.origin.x + i as f64 * PITCH,
            self.origin.y + j as f64 * PITCH,
        )
    }

    /// The cell whose centre is nearest.
    pub(super) fn cell(&self, p: Pt) -> (usize, usize) {
        let i = ((p.x - self.origin.x) / PITCH).round().max(0.0) as usize;
        let j = ((p.y - self.origin.y) / PITCH).round().max(0.0) as usize;
        (i.min(self.nx - 1), j.min(self.ny - 1))
    }

    /// The cells of `window` within `reach` of the box.
    pub(super) fn cells_near(
        &self,
        window: &Window,
        bounds: &Rect,
        reach: f64,
    ) -> impl Iterator<Item = (usize, usize)> + use<> {
        let (i0, j0) = self.cell(add_scalar(bounds.min, -reach));
        let (i1, j1) = self.cell(add_scalar(bounds.max, reach));
        let (i0, j0) = (i0.max(window.i0), j0.max(window.j0));
        let (i1, j1) = (
            i1.min(window.i0 + window.nx - 1),
            j1.min(window.j0 + window.ny - 1),
        );
        (j0..=j1).flat_map(move |j| (i0..=i1).map(move |i| (i, j)))
    }
}

/// A rectangle of cells: the part of the grid a search is confined to. Most lines join pads
/// that are near each other, so most are routed in a small window; one that cannot be is
/// tried again in a larger.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Window {
    pub(super) i0: usize,
    pub(super) j0: usize,
    pub(super) nx: usize,
    pub(super) ny: usize,
}

impl Window {
    pub(super) fn full(grid: &Grid) -> Window {
        Window {
            i0: 0,
            j0: 0,
            nx: grid.nx,
            ny: grid.ny,
        }
    }

    /// The cells within `margin` mm of the box that spans these spots.
    pub(super) fn around(grid: &Grid, spots: &[Spot], margin: f64) -> Window {
        let cells = (margin / PITCH).ceil() as usize;
        let (mut i0, mut j0, mut i1, mut j1) = (usize::MAX, usize::MAX, 0, 0);
        for &(i, j, _) in spots {
            (i0, j0, i1, j1) = (i0.min(i), j0.min(j), i1.max(i), j1.max(j));
        }
        let (i0, j0) = (i0.saturating_sub(cells), j0.saturating_sub(cells));
        let (i1, j1) = ((i1 + cells).min(grid.nx - 1), (j1 + cells).min(grid.ny - 1));
        Window {
            i0,
            j0,
            nx: i1 - i0 + 1,
            ny: j1 - j0 + 1,
        }
    }

    pub(super) fn cells(&self) -> usize {
        self.nx * self.ny
    }

    pub(super) fn contains(&self, i: usize, j: usize) -> bool {
        (self.i0..self.i0 + self.nx).contains(&i) && (self.j0..self.j0 + self.ny).contains(&j)
    }

    /// The cell's place in a vector over the window.
    pub(super) fn index(&self, i: usize, j: usize) -> usize {
        (j - self.j0) * self.nx + (i - self.i0)
    }

    pub(super) fn covers(&self, grid: &Grid) -> bool {
        *self == Window::full(grid)
    }
}

/// `p` moved by `by` along both axes.
pub(super) fn add_scalar(p: Pt, by: f64) -> Pt {
    Pt::new(p.x + by, p.y + by)
}

/// Which cells of a window a track, or a via, of one net may not use.
pub(super) struct Masks {
    pub(super) window: Window,
    pub(super) track: [Vec<bool>; 2],
    pub(super) via: Vec<bool>,
    /// The cells a track at its full width may not use, when `track` is for the narrowest
    /// track the process makes: a track through a cell of this is necked down there. `None`
    /// when `track` is for the full width.
    pub(super) wide: Option<[Vec<bool>; 2]>,
    /// Of the necked cells, those near the line's own pads where only the process's least track
    /// fits: elsewhere a necked track is the width its current needs. `None` when the two are
    /// one width.
    pub(super) pinch: Option<[Vec<bool>; 2]>,
    /// Where a via could stand before [`Router::room_for_vias`] kept only the cells with room
    /// for a cluster: where the others of a cluster stand. `None` when it did not run.
    pub(super) via_alone: Option<Vec<bool>>,
    /// The cells inside a pad of the line's own net, by layer, where a path may turn by more
    /// than 45 degrees: the corner is inside the pad's copper (issue #415). Empty when unset.
    pub(super) free_turn: [Vec<bool>; 2],
    /// How often lines have fought over each cell while the router negotiated, which makes
    /// a track through it dearer. Empty when it is not negotiating.
    pub(super) history: [Vec<u8>; 2],
    /// Cells that other nets' routed copper keeps tracks and vias from, which a search for
    /// the nets least in the way may cross at a price.
    pub(super) soft: Option<Soft>,
    /// What a bend of 45 degrees costs ([`Costs::turn`]).
    pub(super) turn: i64,
    /// The cells of other nets' escape lanes, which a track or a via of this line pays
    /// [`LANE_TRACK`] or [`LANE_VIA`] to use. `None` when there are none.
    pub(super) lane: Option<LaneCost>,
    /// What a track pays, beyond a step, on each cell by layer for the coarse plan
    /// ([`crate::plan`]): nothing in the line's corridor, and outside it the crowding of the
    /// cell's tile on that layer. `None` when the try follows no plan or nothing costs.
    pub(super) off_plan: Option<[Vec<i64>; 2]>,
    /// For a net of a diff pair whose partner is routed, the cells beside the partner's tracks
    /// (within [`PAIR_NEAR`] of their centre lines), by layer: a step onto one costs [`FOLLOW`]
    /// of its length, so the net runs beside its partner (issue #431, S9). Empty when unset.
    pub(super) follow: [Vec<bool>; 2],
    /// The share of the octile distance the search's estimate is, as a fraction: whole, or
    /// [`FOLLOW`] where a step may cost that little.
    pub(super) estimate: (i64, i64),
    /// What a step costs beyond its length on each layer ([`POUR`] on another net's pour), and
    /// a via beyond [`VIA`] ([`POUR_VIA`]), in the pass off the pours; zero elsewhere.
    pub(super) pour: ([i64; 2], i64),
}

pub(super) struct Soft {
    pub(super) track: [Vec<bool>; 2],
    pub(super) via: Vec<bool>,
}

impl Masks {
    /// Nothing in the way anywhere in the window.
    pub(super) fn open(window: Window) -> Masks {
        Masks {
            window,
            track: [vec![false; window.cells()], vec![false; window.cells()]],
            via: vec![false; window.cells()],
            wide: None,
            pinch: None,
            via_alone: None,
            free_turn: [Vec::new(), Vec::new()],
            history: [Vec::new(), Vec::new()],
            soft: None,
            turn: TURN,
            lane: None,
            off_plan: None,
            follow: [Vec::new(), Vec::new()],
            estimate: (1, 1),
            pour: ([0, 0], 0),
        }
    }

    pub(super) fn at(&self, i: usize, j: usize) -> usize {
        self.window.index(i, j)
    }

    /// Whether a path may turn by more than 45 degrees on the cell: it is inside a pad of the
    /// line's net.
    pub(super) fn turns_freely(&self, layer: usize, i: usize, j: usize) -> bool {
        self.free_turn[layer]
            .get(self.at(i, j))
            .copied()
            .unwrap_or(false)
    }

    /// Whether a track on the cell has to be necked down.
    pub(super) fn necked(&self, layer: usize, i: usize, j: usize) -> bool {
        self.wide.as_ref().is_some_and(|w| w[layer][self.at(i, j)])
    }

    /// How narrow a track on the cell is: 0 at full width, 1 necked to the width its current
    /// needs, 2 pinched to the process's least.
    pub(super) fn neck(&self, layer: usize, i: usize, j: usize) -> u8 {
        if self.pinch.as_ref().is_some_and(|p| p[layer][self.at(i, j)]) {
            2
        } else {
            u8::from(self.necked(layer, i, j))
        }
    }

    /// What it costs, beyond a step, to put a track on a cell that other nets' routed copper
    /// keeps tracks from.
    pub(super) fn soft_cost(&self, layer: usize, i: usize, j: usize) -> i64 {
        match &self.soft {
            Some(soft) if soft.track[layer][self.at(i, j)] => SOFT,
            _ => 0,
        }
    }

    /// The same for a via.
    pub(super) fn soft_via_cost(&self, i: usize, j: usize) -> i64 {
        match &self.soft {
            Some(soft) if soft.via[self.at(i, j)] => SOFT_VIA,
            _ => 0,
        }
    }

    /// What a step of `length` costs onto a cell: less by [`FOLLOW`] on a cell beside a
    /// pair's partner.
    pub(super) fn follow_cost(&self, layer: usize, i: usize, j: usize, length: i64) -> i64 {
        match self.follow[layer].get(self.at(i, j)) {
            Some(true) => length * FOLLOW.0 / FOLLOW.1,
            _ => length,
        }
    }

    /// What it costs, beyond a step, to put a track on a cell of another net's escape lane.
    pub(super) fn lane_cost(&self, layer: usize, i: usize, j: usize) -> i64 {
        match &self.lane {
            Some(lane) if lane.track[layer][self.at(i, j)] => LANE_TRACK,
            _ => 0,
        }
    }

    /// The same for a via.
    pub(super) fn lane_via_cost(&self, i: usize, j: usize) -> i64 {
        match &self.lane {
            Some(lane) if lane.via[self.at(i, j)] => LANE_VIA,
            _ => 0,
        }
    }

    /// What it costs, beyond a step, to put a track on a cell for the coarse plan.
    pub(super) fn off_plan_cost(&self, layer: usize, i: usize, j: usize) -> i64 {
        self.off_plan
            .as_ref()
            .map_or(0, |cost| cost[layer][self.at(i, j)])
    }

    /// What it costs, beyond a step, to put a track on the cell.
    pub(super) fn history_cost(&self, layer: usize, i: usize, j: usize) -> i64 {
        self.history[layer]
            .get(self.at(i, j))
            .map_or(0, |&h| i64::from(h) * HISTORY_COST)
    }
}

/// How far from an obstacle a mask keeps a track and a via: the room beyond the rules, and
/// the width the track is drawn at.
#[derive(Clone, Copy)]
pub(super) struct Reach {
    pub(super) room: f64,
    pub(super) width: f64,
}

/// Everything copper must keep clear of, apart from what has been routed.
pub(crate) struct Obstacles {
    pub(crate) list: Vec<Obstacle>,
    /// Where no via may go whatever its net, and how far from the box: a via does not sit
    /// on a pad, and keeps further from the holes of through-hole pads.
    pub(super) via_keepouts: Vec<(Form, f64)>,
}

/// How many obstacles keep each cell of a window from tracks on each layer, and from vias.
/// A cell is open when its count is zero. Counting is what lets one net's own pads be taken
/// away again from counts made for all of them.
pub(super) struct Counts {
    pub(super) window: Window,
    pub(super) track: [Vec<u16>; 2],
    pub(super) via: Vec<u16>,
}

impl Counts {
    pub(super) fn new(window: Window) -> Counts {
        Counts {
            window,
            track: [vec![0; window.cells()], vec![0; window.cells()]],
            via: vec![0; window.cells()],
        }
    }

    /// Add one (`up`) or take away one obstacle's reach: the cells within the rules' space
    /// of it, plus the room, for a track of `width` and for a via.
    pub(super) fn stamp(
        &mut self,
        grid: &Grid,
        rules: &netsmith_pcb::Rules,
        o: &Obstacle,
        reach: Reach,
        up: bool,
    ) {
        let Reach { room, width } = reach;
        let window = self.window;
        let track_reach = o.clearance + width / 2.0 + room;
        let via_reach = o.clearance + rules.via_diameter / 2.0 + room;
        let bounds = o.form.bounds();
        let far = track_reach.max(via_reach);
        // Most obstacles are nowhere near a small window.
        let corner = |i: usize, j: usize| grid.centre(i, j);
        let seen = Rect::new(
            corner(window.i0, window.j0),
            corner(window.i0 + window.nx - 1, window.j0 + window.ny - 1),
        );
        if !seen.expanded(PITCH).overlaps(&bounds.expanded(far)) {
            return;
        }
        // Look as far as either goes: a track wider than a via reaches further than it.
        for (i, j) in grid.cells_near(&window, &bounds, far) {
            let d = o.form.distance(grid.centre(i, j));
            let at = window.index(i, j);
            let bump = |c: &mut u16| {
                if up {
                    *c += 1;
                } else {
                    *c -= 1;
                }
            };
            for layer in [FRONT, BACK] {
                if o.layers[layer] && d < track_reach {
                    bump(&mut self.track[layer][at]);
                }
            }
            if d < via_reach {
                bump(&mut self.via[at]);
            }
        }
    }

    pub(super) fn stamp_keepout(&mut self, grid: &Grid, form: &Form, reach: f64) {
        for (i, j) in grid.cells_near(&self.window, &form.bounds(), reach) {
            if form.distance(grid.centre(i, j)) < reach {
                let at = self.window.index(i, j);
                self.via[at] += 1;
            }
        }
    }
}

/// What the copper that never moves keeps cells from, over the whole board, for one room
/// and track width: the board's edge, the pads, the holes and the keepouts.
pub(super) struct Base {
    pub(super) counts: Counts,
}

impl Base {
    pub(super) fn new(board: &Board, grid: &Grid, fixed: &Obstacles, reach: Reach) -> Base {
        let mut counts = Counts::new(Window::full(grid));
        let (outline, rules) = (&board.outline, &board.rules);
        // The grid lies over the outline's bounding box; a cell outside a polygon outline
        // (in a notch, past an arc) is closed to everything, as a cell off the box is.
        let polygon = board.polygon.as_ref();
        for j in 0..grid.ny {
            for i in 0..grid.nx {
                let p = grid.centre(i, j);
                if !outline.contains(&Rect::new(p, p)) || polygon.is_some_and(|s| !s.contains(p)) {
                    let at = counts.window.index(i, j);
                    counts.track[FRONT][at] += 1;
                    counts.track[BACK][at] += 1;
                    counts.via[at] += 1;
                }
            }
        }
        for o in &fixed.list {
            counts.stamp(grid, rules, o, reach, true);
        }
        for (form, keep) in &fixed.via_keepouts {
            counts.stamp_keepout(grid, form, *keep);
        }
        Base { counts }
    }
}

/// The cells of `window` that a track, or a via, of `net` may not use: what never moves
/// (`base`, less the net's own pads, `own`), the copper routed so far of other nets
/// (`routed`), and the keep-outs of the holes of routed vias (`keepouts`).
#[allow(clippy::too_many_arguments)]
pub(super) fn masks_for(
    grid: &Grid,
    window: Window,
    board: &Board,
    base: &Base,
    own: &[&Obstacle],
    routed: &[&Obstacle],
    keepouts: &[(Form, f64)],
    net: usize,
    reach: Reach,
) -> Masks {
    let rules = &board.rules;
    let mut counts = Counts::new(window);
    let from = &base.counts;
    for layer in [FRONT, BACK] {
        copy_window(&from.track[layer], &mut counts.track[layer], grid, window);
    }
    copy_window(&from.via, &mut counts.via, grid, window);
    for o in own {
        counts.stamp(grid, rules, o, reach, false);
    }
    for o in routed.iter().filter(|o| o.net != Some(net)) {
        counts.stamp(grid, rules, o, reach, true);
    }
    for (form, keep) in keepouts {
        counts.stamp_keepout(grid, form, *keep);
    }
    let mut m = Masks::open(window);
    for layer in [FRONT, BACK] {
        m.track[layer] = counts.track[layer].iter().map(|&c| c > 0).collect();
    }
    m.via = counts.via.iter().map(|&c| c > 0).collect();
    m
}

/// The rows of a window out of counts over the whole grid.
pub(super) fn copy_window(whole: &[u16], out: &mut [u16], grid: &Grid, window: Window) {
    for j in 0..window.ny {
        let from = (window.j0 + j) * grid.nx + window.i0;
        out[j * window.nx..(j + 1) * window.nx].copy_from_slice(&whole[from..from + window.nx]);
    }
}
