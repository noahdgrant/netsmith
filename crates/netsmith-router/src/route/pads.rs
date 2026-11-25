//! Where a line meets a pad: the cells of a pad, the escape lanes and the classes of the lines.

use super::*;

/// The pad a ratsnest line ends on, and the part it is of.
pub(super) fn pad_of<'a>(
    board: &'a Board,
    pad: &'a netsmith_pcb::PadRef,
    at: Pt,
) -> (&'a netsmith_pcb::Placed, &'a netsmith_pcb::Pad) {
    let part = board
        .part(&pad.refdes)
        .expect("a ratsnest line names a part");
    let found = part
        .footprint
        .pads_numbered(&pad.number)
        .find(|p| part.pad_centre(p) == at)
        .expect("a ratsnest line ends on a pad centre");
    (part, found)
}

/// The copper layers a pad of this footprint has, and its centre. A through-hole pad is on
/// every one of the router's `layers`, the inner signal layers too (issue #833).
pub(super) fn pad_spots(
    board: &Board,
    pad: &netsmith_pcb::PadRef,
    at: Pt,
    grid: &Grid,
    layers: usize,
) -> Vec<Spot> {
    let (part, found) = pad_of(board, pad, at);
    let (front, back) = part.pad_layers(found);
    let (i, j) = grid.cell(at);
    let through = front && back && found.drill.is_some();
    [(front, FRONT), (back, BACK)]
        .iter()
        .filter(|(has, _)| *has)
        .map(|&(_, layer)| (i, j, layer))
        .chain((2..layers).filter(|_| through).map(|layer| (i, j, layer)))
        .collect()
}

/// A pad a via may stand in: a surface-mount pad of a plain shape that is wide enough for
/// the via's copper, when the process fills and caps vias that size. On an HDI process the via
/// to a layer is the one [`Stack::via_between`] draws: a blind via to the layer under the pad's
/// face, which the process fills and planarizes by plating whatever `.via_in_pad` says, so it
/// stands in a pad as narrow as its own copper (issue #1071); a through via only as on any
/// process.
#[derive(Clone, Copy, Debug)]
pub(super) struct InPad {
    /// Where the via goes: the pad's centre.
    pub(super) at: Pt,
    /// The pad's box, which the via stays off all the keep-outs of other pads.
    pub(super) bounds: Rect,
    /// The pad's cell.
    pub(super) cell: (usize, usize),
    /// The pad's layer of the stack, [`FRONT`] or [`BACK`].
    pub(super) face: usize,
    /// The layers a path through the via goes on from: the far outer layer and each inner
    /// signal layer the line may use (issue #833) that a via in the pad leads to.
    pub(super) layers: Layers,
}

impl InPad {
    /// The cells a path through the via in the pad goes on from: the far outer layer's, then
    /// those of the inner signal layers, from the top.
    pub(super) fn spots(&self) -> impl Iterator<Item = Spot> + '_ {
        let (i, j) = self.cell;
        (0..32).filter(|&l| self.layers[l]).map(move |l| (i, j, l))
    }

    /// Whether a path that begins or ends at `spot` goes through the via in the pad.
    pub(super) fn is_spot(&self, spot: Spot) -> bool {
        self.spots().any(|s| s == spot)
    }
}

/// The via in the pad at `at` a line may go through to the first `layers` layers of `stack`,
/// `None` where it may not stand in the pad ([`InPad`]).
pub(super) fn in_pad(
    board: &Board,
    stack: &Stack,
    pad: &netsmith_pcb::PadRef,
    at: Pt,
    grid: &Grid,
    layers: usize,
) -> Option<InPad> {
    let rules = &board.rules;
    let (part, found) = pad_of(board, pad, at);
    let (front, back) = part.pad_layers(found);
    if front == back || found.extent.is_some() || found.drill.is_some() {
        return None;
    }
    let face = if front { FRONT } else { BACK };
    let width = found.width.min(found.height);
    let reach = (0..layers)
        .filter(|&l| l != face)
        .filter(|&l| {
            let (_, offer) = stack.offer_between(board, face, l);
            (offer.laser || rules.via_in_pad) && width >= offer.diameter
        })
        .fold(Layers::outer(false, false), Layers::with);
    if reach.is_empty() {
        return None;
    }
    Some(InPad {
        at,
        bounds: part.pad_bounds(found),
        cell: grid.cell(at),
        face,
        layers: reach,
    })
}

/// How deep a pad stands in a fine-pitch grid of pads, in pitches from the outside of the
/// part's pads: 0 for a pad on the outer ring, and for every pad of a part that is not a
/// fine-pitch grid (fewer than eight surface-mount pads, or none within the room a via and the
/// rules' spaces need between pads). Such a pad can leave only through the gaps between its
/// neighbours, and the deeper it is the fewer ways out it has.
pub(super) fn pad_depth(board: &Board, pad: &netsmith_pcb::PadRef, at: Pt) -> f64 {
    let rules = &board.rules;
    let (part, found) = pad_of(board, pad, at);
    let (front, back) = part.pad_layers(found);
    let on_face = |p: &netsmith_pcb::Pad| {
        let (f, b) = part.pad_layers(p);
        (front && f) || (back && b)
    };
    let pads: Vec<&netsmith_pcb::Pad> = part
        .footprint
        .pads
        .iter()
        .filter(|p| p.drill.is_none() && on_face(p))
        .collect();
    if pads.len() < 8 || found.drill.is_some() {
        return 0.0;
    }
    let here = part.pad_bounds(found);
    let gap = |a: &Rect, b: &Rect| {
        let dx = (a.min.x - b.max.x).max(b.min.x - a.max.x).max(0.0);
        let dy = (a.min.y - b.max.y).max(b.min.y - a.max.y).max(0.0);
        dx.hypot(dy)
    };
    let nearest = pads
        .iter()
        .filter(|p| part.pad_centre(p) != at)
        .map(|p| {
            (
                gap(&here, &part.pad_bounds(p)),
                part.pad_centre(p).distance(at),
            )
        })
        .fold((f64::MAX, f64::MAX), |m, g| (m.0.min(g.0), m.1.min(g.1)));
    if nearest.0 >= rules.via_diameter + 2.0 * rules.clearance {
        return 0.0;
    }
    let mut span = here;
    for p in &pads {
        let b = part.pad_bounds(p);
        span = Rect::new(
            Pt::new(span.min.x.min(b.min.x), span.min.y.min(b.min.y)),
            Pt::new(span.max.x.max(b.max.x), span.max.y.max(b.max.y)),
        );
    }
    let inside = (at.x - span.min.x)
        .min(span.max.x - at.x)
        .min(at.y - span.min.y)
        .min(span.max.y - at.y);
    (inside / nearest.1).round().max(0.0)
}

/// The escape lanes of the pads of fine-pitch parts (issue #431, see the module's header): for
/// each pad on the outside of such a part, the cells straight out from its edge, away from the
/// part, on the pad's layer, [`LANE_LENGTH`] long and as wide as the pad. A track of another
/// net pays [`LANE_TRACK`] a step to use one. A via of any net pays [`LANE_VIA`] to stand where
/// its copper would close a lane, except a via of the lane's own net at the lane's end, where
/// its line leaves the part's ring. A net uses the lanes of the other net of its diff pair as
/// its own. It is a cost, not a wall: a line that has no other way crosses a lane, and the
/// negotiation and rip-ups can take one back.
pub(super) struct Lanes {
    pub(super) nx: usize,
    /// For each layer and cell of the whole grid, the nets whose lanes take it, as a net's
    /// index plus one, 0 for none. A cell counts [`LANE_SLOTS`] nets at most; another's lane
    /// does not take it.
    pub(super) track: Vec<Vec<[u32; LANE_SLOTS]>>,
    /// The cells where a via would close a lane.
    pub(super) via_block: Vec<bool>,
    /// The cells at the end of a lane where its own net's via is free, by net as `track`.
    pub(super) via_end: Vec<[u32; LANE_SLOTS]>,
    /// The other net of each net's diff pair ([`diff_pair_partners`]).
    pub(super) partner: Vec<Option<usize>>,
}

/// Whether a lane goes on from its via on the inner signal layers, the cells as far again past
/// its end, which another net's track pays [`LANE_TRACK`] a step to use (issue #1015): measured,
/// not kept (`crate::plan`'s header: no board or line changed on 36 runs of the dense board).
pub(super) const INNER_LANES: bool = false;

/// How many nets' lanes one cell counts.
pub(super) const LANE_SLOTS: usize = 4;

/// The cells of a window that a line's track, or via, pays to use: [`Lanes::charged`].
pub(super) struct LaneCost {
    pub(super) track: Vec<Vec<bool>>,
    pub(super) via: Vec<bool>,
}

impl Lanes {
    /// No lanes yet, over `grid` on `layers` layers of the router's stack.
    pub(super) fn over(grid: &Grid, nets: usize, layers: usize) -> Lanes {
        let cells = grid.nx * grid.ny;
        Lanes {
            nx: grid.nx,
            track: vec![vec![[0; LANE_SLOTS]; cells]; layers],
            via_block: vec![false; cells],
            via_end: vec![[0; LANE_SLOTS]; cells],
            partner: vec![None; nets],
        }
    }

    pub(super) fn take(slot: &mut [u32; LANE_SLOTS], net: usize) {
        let mark = net as u32 + 1;
        if slot.contains(&mark) {
            return;
        }
        if let Some(free) = slot.iter_mut().find(|s| **s == 0) {
            *free = mark;
        }
    }

    /// The lane of a pad of `net`: the cells whose centres are in `track`, on the layers in
    /// `layers`; for a via, the cells within `via_block` of it, and the cells within `via_end`
    /// of `end`, where its own net's via is free.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn claim(
        &mut self,
        grid: &Grid,
        net: usize,
        layers: [bool; 2],
        track: &Rect,
        end: Pt,
        via_block: f64,
        via_end: f64,
    ) {
        self.claim_inner(grid, net, layers, track, end, (via_block, via_end), None);
    }

    /// [`Lanes::claim`], and on every inner layer of the stack the cells whose centres are in
    /// `inner`: the lane's way on from its via (issue #1015).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn claim_inner(
        &mut self,
        grid: &Grid,
        net: usize,
        layers: [bool; 2],
        track: &Rect,
        end: Pt,
        (via_block, via_end): (f64, f64),
        inner: Option<&Rect>,
    ) {
        let whole = Window::full(grid);
        if let Some(inner) = inner {
            for (i, j) in grid.cells_near(&whole, inner, 0.0) {
                let p = grid.centre(i, j);
                if inner.contains(&Rect::new(p, p)) {
                    let at = j * self.nx + i;
                    for layer in 2..self.track.len() {
                        Self::take(&mut self.track[layer][at], net);
                    }
                }
            }
        }
        for (i, j) in grid.cells_near(&whole, track, via_block) {
            let p = grid.centre(i, j);
            let at = j * self.nx + i;
            if track.contains(&Rect::new(p, p)) {
                for layer in [FRONT, BACK] {
                    if layers[layer] {
                        Self::take(&mut self.track[layer][at], net);
                    }
                }
            }
            if Form::Rect(*track).distance(p) <= via_block + 1e-9 {
                self.via_block[at] = true;
            }
        }
        for (i, j) in grid.cells_near(&whole, &Rect::new(end, end), via_end) {
            if grid.centre(i, j).distance(end) <= via_end + 1e-9 {
                Self::take(&mut self.via_end[j * self.nx + i], net);
            }
        }
    }

    /// The cells of `window` that a line of `net` pays to use. `None` when the window has none.
    pub(super) fn charged(&self, window: Window, net: usize) -> Option<LaneCost> {
        let marks = [Some(net), self.partner.get(net).copied().flatten()];
        let own = |slot: &[u32; LANE_SLOTS]| {
            marks
                .iter()
                .flatten()
                .any(|&n| slot.contains(&(n as u32 + 1)))
        };
        let mut any = false;
        let mut track = vec![Vec::new(); self.track.len()];
        let mut via = Vec::with_capacity(window.cells());
        for j in window.j0..window.j0 + window.ny {
            for i in window.i0..window.i0 + window.nx {
                let at = j * self.nx + i;
                for layer in 0..self.track.len() {
                    let slot = &self.track[layer][at];
                    let charged = slot[0] != 0 && !own(slot);
                    any |= charged;
                    track[layer].push(charged);
                }
                let end = &self.via_end[at];
                let charged = !own(end) && (self.via_block[at] || end[0] != 0);
                any |= charged;
                via.push(charged);
            }
        }
        any.then_some(LaneCost { track, via })
    }
}

/// Whether a part gets escape lanes ([`Lanes`]): at least 16 surface-mount terminal pads (a
/// paste aperture is none), two of them
/// 0.55 mm apart or nearer (pads at one centre, the pieces of an exposed pad, are not counted
/// apart), facing at least three sides. A connector's single row is not ringed by other parts.
pub(super) fn fine_pitch(part: &netsmith_pcb::Placed, faces: usize) -> bool {
    let pads: Vec<&netsmith_pcb::Pad> = part
        .footprint
        .pads
        .iter()
        .filter(|p| p.drill.is_none() && p.is_terminal())
        .collect();
    pads.len() >= 16
        && faces >= 3
        && pads.iter().enumerate().any(|(a, p)| {
            pads.iter().skip(a + 1).any(|q| {
                let d = part.pad_centre(p).distance(part.pad_centre(q));
                d > 1e-6 && d <= 0.55 + 1e-9
            })
        })
}

/// The escape lanes of every fine-pitch part of the board ([`Lanes`]), `None` if it has none.
pub(super) fn escape_lanes(board: &Board, grid: &Grid) -> Option<Lanes> {
    let net_of: HashMap<(&str, &str), usize> = board
        .nets
        .iter()
        .enumerate()
        .flat_map(|(n, net)| {
            net.pads
                .iter()
                .map(move |p| ((p.refdes.as_str(), p.number.as_str()), n))
        })
        .collect();
    let rules = &board.rules;
    let inner_layers = board.signal_layers().len();
    let mut lanes = Lanes::over(grid, board.nets.len(), 2 + inner_layers);
    lanes.partner = diff_pair_partners(board);
    // A via closes a lane where its copper and the rules' space come within half the least
    // track of the lane's middle.
    let via_block = rules.via_diameter / 2.0 + rules.clearance + rules.min_track_width / 2.0;
    let reach = LANE_LENGTH;
    let mut any = false;
    for part in &board.parts {
        let pads: Vec<&netsmith_pcb::Pad> = part
            .footprint
            .pads
            .iter()
            .filter(|p| p.drill.is_none())
            .collect();
        let Some(span) = pads.iter().map(|p| part.pad_bounds(p)).reduce(|a, b| {
            Rect::new(
                Pt::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
                Pt::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
            )
        }) else {
            continue;
        };
        let mut claims = Vec::new();
        for pad in pads {
            let Some(&net) = net_of.get(&(part.refdes.as_str(), pad.number.as_str())) else {
                continue;
            };
            let b = part.pad_bounds(pad);
            let c = b.centre();
            // The sides of the part's pads that this pad is on; a pad on none (an exposed pad,
            // an inner row) has no lane. Of two, the one that points away from the part's
            // origin.
            let sides = [
                ((-1.0, 0.0), b.min.x - span.min.x),
                ((1.0, 0.0), span.max.x - b.max.x),
                ((0.0, -1.0), b.min.y - span.min.y),
                ((0.0, 1.0), span.max.y - b.max.y),
            ];
            let away = |(dx, dy): (f64, f64)| dx * (c.x - part.at.x) + dy * (c.y - part.at.y);
            let Some(&((dx, dy), _)) = sides
                .iter()
                .filter(|(_, gap)| *gap <= 1e-6)
                .max_by(|a, b| away(a.0).total_cmp(&away(b.0)))
            else {
                continue;
            };
            let (track, end) = if dx < 0.0 {
                let r = Rect::new(Pt::new(b.min.x - reach, b.min.y), Pt::new(b.min.x, b.max.y));
                (r, Pt::new(r.min.x, c.y))
            } else if dx > 0.0 {
                let r = Rect::new(Pt::new(b.max.x, b.min.y), Pt::new(b.max.x + reach, b.max.y));
                (r, Pt::new(r.max.x, c.y))
            } else if dy < 0.0 {
                let r = Rect::new(Pt::new(b.min.x, b.min.y - reach), Pt::new(b.max.x, b.min.y));
                (r, Pt::new(c.x, r.min.y))
            } else {
                let r = Rect::new(Pt::new(b.min.x, b.max.y), Pt::new(b.max.x, b.max.y + reach));
                (r, Pt::new(c.x, r.max.y))
            };
            // The lane's way on from its via on the inner signal layers (issue #1015): as wide
            // as the pad, from the lane's end as far again.
            let along = Pt::new(dx * reach, dy * reach);
            let far = Pt::new(end.x + along.x, end.y + along.y);
            let half = Pt::new(
                if dx == 0.0 { b.width() / 2.0 } else { 0.0 },
                if dy == 0.0 { b.height() / 2.0 } else { 0.0 },
            );
            let inner = Rect::new(
                Pt::new(end.x.min(far.x) - half.x, end.y.min(far.y) - half.y),
                Pt::new(end.x.max(far.x) + half.x, end.y.max(far.y) + half.y),
            );
            let (front, back) = part.pad_layers(pad);
            claims.push(((dx as i8, dy as i8), net, [front, back], track, end, inner));
        }
        let mut faces: Vec<(i8, i8)> = claims.iter().map(|c| c.0).collect();
        faces.sort_unstable();
        faces.dedup();
        if !fine_pitch(part, faces.len()) {
            continue;
        }
        for (_, net, layers, track, end, inner) in claims {
            let via_end = rules.via_diameter / 2.0;
            let inner = (INNER_LANES && inner_layers > 0).then_some(&inner);
            lanes.claim_inner(grid, net, layers, &track, end, (via_block, via_end), inner);
            any = true;
        }
    }
    any.then_some(lanes)
}

/// For each net, the other net of its diff pair, if it has one ([`Board::diff_partners`]): two
/// nets whose names differ only in a suffix pair the route scorecard knows
/// (`netsmith_geom::PAIR_SUFFIXES`), the two nets of a pair with an impedance target (a
/// `PairImpedance`, issue #1025), or two nets the design states are a pair (a `DiffPair`, or the
/// lines of a std bus such as `Can`, issue #1193).
pub(super) fn diff_pair_partners(board: &Board) -> Vec<Option<usize>> {
    board.diff_partners()
}

/// How important a line is (issue #416), most important first: the order the classes are
/// routed in from the second try on ([`LineClasses`]). Every class is derived from what the
/// board knows, with nothing written in the source. The copper of a decoupling, pair or clock
/// line is not ripped up for a line of a later class ([`Class::protected`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Class {
    /// A net of a stated impedance of one track, an RF net (issue #1274): routed first, and
    /// kept on one layer with no via unless the physical file says `route any_layer`.
    Rf,
    /// A pad that must reach its net's plane, or a pad deep in a fine-pitch grid of pads: the
    /// fewest ways out.
    Escape,
    /// A decoupling capacitor's pad to the power pin it decouples ([`crate::quality`]).
    Decoupling,
    /// A net of a diff pair ([`diff_pair_partners`]).
    Pair,
    /// A net of a crystal (a part of the kind, or that provides the generic, `Crystal`), or
    /// whose name says it is a clock (`clk`, `xin`, `xout`, `xtal`).
    Clock,
    /// A net that carries at least [`POWER_CURRENT`].
    Power,
    Rest,
}

impl Class {
    /// Whether a line of this class keeps its copper against a line of a later class: a
    /// decoupling, pair or clock line does. An escape or power line keeps its place in the order
    /// only (decided by the agent under the owner's delegation of 2026-11-13): protecting the
    /// escapes of a QFN and every power net left RP2350 unrouted, and a power line ripped up is
    /// routed again at its full width.
    pub(super) fn protected(self) -> bool {
        matches!(
            self,
            Class::Rf | Class::Decoupling | Class::Pair | Class::Clock
        )
    }
}

/// The least current of a net that is routed as power, in amperes.
pub(super) const POWER_CURRENT: f64 = 0.1;

/// The class of each line ([`Class`]); `depth` is each line's [`pad_depth`]. With `pairs`
/// false a net of a diff pair is classed as if it were no pair (the try that routes a pair as
/// two lines, [`Router::unpair`], issue #1295).
pub(super) fn classes(board: &Board, lines: &[Ratline], depth: &[f64], pairs: bool) -> Vec<Class> {
    let partners = if pairs {
        diff_pair_partners(board)
    } else {
        vec![None; board.nets.len()]
    };
    let index: HashMap<&str, usize> = board
        .nets
        .iter()
        .enumerate()
        .map(|(n, net)| (net.name.as_str(), n))
        .collect();
    let pair = |net: &str| index.get(net).is_some_and(|&n| partners[n].is_some());
    let crystal =
        |part: &netsmith_pcb::Placed| part.declared_kind == Some(netsmith_core::PartKind::Crystal);
    let clock_nets: Vec<&str> = board
        .nets
        .iter()
        .filter(|n| {
            let lower = n.name.to_lowercase();
            let named = ["clk", "xin", "xout", "xtal"]
                .iter()
                .any(|w| lower.contains(w));
            let on_crystal = n
                .pads
                .iter()
                .any(|p| board.part(&p.refdes).is_some_and(crystal));
            // A crystal's ground pins are not its clock.
            let plane = board.planes.iter().any(|pl| pl.net == n.name)
                || board.pours.iter().any(|po| po.net == n.name);
            !plane && (named || on_crystal)
        })
        .map(|n| n.name.as_str())
        .collect();
    let power = |net: &str| {
        board
            .net_currents
            .iter()
            .any(|c| c.net == net && c.current >= POWER_CURRENT)
    };
    let decoupling = netsmith_pcb::decoupling_pairs(&board.parts, &board.nets);
    let decouples = |l: &Ratline| crate::decouple::decouples(board, &decoupling, l);
    let rf = board.rf_nets();
    lines
        .iter()
        .zip(depth)
        .map(|(l, &d)| {
            if !l.to_plane && rf.contains(&l.net) {
                Class::Rf
            } else if l.to_plane || d > 0.0 {
                Class::Escape
            } else if decouples(l) {
                Class::Decoupling
            } else if pair(&l.net) {
                Class::Pair
            } else if clock_nets.contains(&l.net.as_str()) {
                Class::Clock
            } else if power(&l.net) {
                Class::Power
            } else {
                Class::Rest
            }
        })
        .collect()
}
