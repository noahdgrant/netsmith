//! Escape room (issue #997): the nets that leave each side of a busy part against the
//! corridor in front of that side.
//!
//! A busy part's pads that join a net that is not a plane and reaches another part leave it on
//! its face. A pad on the rim of the box round the part's pads leaves through the side it is on
//! (a pad further in leaves inward or by a via, and is not counted). In front of each side is a
//! band [`RING`] deep. Where nothing reaches into the band a track goes straight out. Where the
//! room of another part on the same face, or the board's edge less the copper to edge space,
//! reaches into it, the tracks of the pads behind it must run along the gap to its ends: a run of
//! the side blocked at a gap `g` holds as many tracks as fit in `g` (with a clearance to each
//! side), times two (they leave by both ends). The escape shortfall of a run is the pads behind
//! it beyond what it holds, and of a part the sum over its runs, a count of nets. The cost is
//! the shortfall of every busy part times `Problem::escape_room`.
//!
//! A capacitor that decouples the part is not counted against it, as for the escape space: its
//! place is by the pins it decouples.

use super::*;

/// How near a pad's edge must be to a side of the box round the pads to be on its rim, in mm.
const RIM: f64 = 0.05;

/// What a busy part's pads ask of the room round it, in one turn, relative to its origin.
#[derive(Clone)]
pub(super) struct Rim {
    /// The box round the part's pads.
    pads: Rect,
    /// For each side (`-x`, `-y`, `+x`, `+y`), where along it the pads that leave through it
    /// are: their centre's y on the two sides across x, their x on the other two.
    leaving: [Vec<f64>; 4],
}

impl Rim {
    /// The rim of a part as it lies at angle `angle` about its origin; `leaves` says which of
    /// its pads (by their index in the footprint) leave it. `None` if no pad leaves.
    pub(super) fn of(part: &mut Placed, angle: f64, leaves: &[bool]) -> Option<Rim> {
        let (at, rotation) = (part.at, part.rotation);
        part.at = Pt::default();
        part.rotation = angle;
        let boxes: Vec<(Rect, bool)> = part
            .footprint
            .pads
            .iter()
            .enumerate()
            .filter(|(_, p)| part.pad_layers(p) != (false, false))
            .map(|(k, p)| (part.pad_bounds(p), leaves.get(k).copied().unwrap_or(false)))
            .collect();
        part.at = at;
        part.rotation = rotation;
        let pads = boxes.iter().map(|(b, _)| *b).reduce(|a, b| a.union(&b))?;
        let mut leaving: [Vec<f64>; 4] = Default::default();
        for (b, _) in boxes.iter().filter(|(_, l)| *l) {
            let c = b.centre();
            let sides = [
                (b.min.x - pads.min.x <= RIM, c.y),
                (b.min.y - pads.min.y <= RIM, c.x),
                (pads.max.x - b.max.x <= RIM, c.y),
                (pads.max.y - b.max.y <= RIM, c.x),
            ];
            // A pad in a corner is on two sides: it counts on the first.
            if let Some(s) = sides.iter().position(|(on, _)| *on) {
                leaving[s].push(sides[s].1);
            }
        }
        leaving
            .iter()
            .any(|l| !l.is_empty())
            .then_some(Rim { pads, leaving })
    }
}

/// The band in front of side `side` of `pads`, [`RING`] deep.
fn band(pads: &Rect, side: usize) -> Rect {
    let (lo, hi) = (pads.min, pads.max);
    match side {
        0 => Rect::new(Pt::new(lo.x - RING, lo.y), Pt::new(lo.x, hi.y)),
        1 => Rect::new(Pt::new(lo.x, lo.y - RING), Pt::new(hi.x, lo.y)),
        2 => Rect::new(Pt::new(hi.x, lo.y), Pt::new(hi.x + RING, hi.y)),
        _ => Rect::new(Pt::new(lo.x, hi.y), Pt::new(hi.x, hi.y + RING)),
    }
}

/// Where box `r` lies along side `side` of `pads`, and how far out from the side its near
/// edge is.
fn across(pads: &Rect, side: usize, r: &Rect) -> (f64, f64, f64) {
    match side {
        0 => (r.min.y, r.max.y, pads.min.x - r.max.x),
        1 => (r.min.x, r.max.x, pads.min.y - r.max.y),
        2 => (r.min.y, r.max.y, r.min.x - pads.max.x),
        _ => (r.min.x, r.max.x, r.min.y - pads.max.y),
    }
}

/// The escape shortfall of one side whose pads leave at `leaving` along it, from `lo` to `hi`,
/// with `walls` (from, to, gap) in front of it. `fits` is how many tracks a gap holds.
fn side_shortfall(
    leaving: &[f64],
    lo: f64,
    hi: f64,
    walls: &[(f64, f64, f64)],
    fits: impl Fn(f64) -> f64,
) -> f64 {
    if walls.is_empty() {
        return 0.0;
    }
    let mut cuts: Vec<f64> = vec![lo, hi];
    for &(a, z, _) in walls {
        cuts.extend([a, z].into_iter().filter(|&v| v > lo && v < hi));
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    let mut short = 0.0;
    let mut close = |run: (f64, f64), end: f64, last: bool| {
        let behind = leaving
            .iter()
            .filter(|&&v| v >= run.0 && (v < end || (last && v <= end)))
            .count() as f64;
        short += (behind - 2.0 * fits(run.1)).max(0.0);
    };
    // A run of the side blocked at less than the band's depth: where it starts, its least gap.
    let mut run: Option<(f64, f64)> = None;
    for k in 0..cuts.len() - 1 {
        let (a, z) = (cuts[k], cuts[k + 1]);
        let mid = (a + z) / 2.0;
        let gap = walls
            .iter()
            .filter(|&&(f, t, _)| f <= mid && mid <= t)
            .map(|&(_, _, g)| g)
            .fold(RING, f64::min);
        if gap < RING {
            run = Some(match run {
                Some((start, least)) => (start, least.min(gap)),
                None => (a, gap),
            });
        } else if let Some(r) = run.take() {
            close(r, a, false);
        }
    }
    if let Some(r) = run {
        close(r, hi, true);
    }
    short
}

impl<'a> Engine<'a> {
    /// Busy part `b`'s rim as it lies now.
    fn rim(&self, b: usize) -> Option<&Rim> {
        let rims = &self.models[b].rims;
        let k = (self.shape_index(b) / 4).min(rims.len().checked_sub(1)?);
        rims[k].as_ref()
    }

    /// The box round busy part `b`'s pads and the bands in front of it, as it lies now.
    fn rim_reach(&self, b: usize) -> Option<Rect> {
        Some(self.rim(b)?.pads.translated(self.pos(b)).expanded(RING))
    }

    /// Busy part `b`'s escape shortfall as the parts are now, and the parts whose room reaches
    /// into a band in front of it.
    pub(super) fn rim_cost(&self, b: usize) -> (f64, Vec<usize>) {
        let Some(rim) = self.rim(b) else {
            return (0.0, Vec::new());
        };
        let pads = rim.pads.translated(self.pos(b));
        let reach = pads.expanded(RING);
        let rules = self.p.rules;
        let pitch = (rules.track_width + rules.clearance).max(0.05);
        let fits = |g: f64| ((g - rules.clearance) / pitch).floor().max(0.0);
        let n = self.models.len();
        let face = self.models[b].side;
        let others: Vec<(usize, Rect)> = self
            .grid
            .near(b, &reach)
            .into_iter()
            .filter(|&j| j != b && !self.near_ok[b * n + j])
            .flat_map(|j| {
                self.rooms_on(j, face)
                    .into_iter()
                    .map(move |(r, _)| (j, r))
            })
            .filter(|(_, r)| r.overlaps(&reach))
            .collect();
        let region = self.p.region;
        let to_edge = [
            pads.min.x - region.min.x,
            pads.min.y - region.min.y,
            region.max.x - pads.max.x,
            region.max.y - pads.max.y,
        ];
        let mut blockers: Vec<usize> = Vec::new();
        let mut short = 0.0;
        for side in 0..4 {
            let leaving = &rim.leaving[side];
            if leaving.is_empty() {
                continue;
            }
            let (lo, hi, _) = across(&pads, side, &pads);
            let in_front = band(&pads, side);
            let mut walls: Vec<(f64, f64, f64)> = Vec::new();
            let wall = to_edge[side] - rules.copper_to_edge;
            if wall < RING {
                walls.push((lo, hi, wall.max(0.0)));
            }
            for (j, r) in &others {
                if !r.overlaps(&in_front) {
                    continue;
                }
                let (a, z, gap) = across(&pads, side, r);
                walls.push((a.max(lo), z.min(hi), gap.max(0.0)));
                if !blockers.contains(j) {
                    blockers.push(*j);
                }
            }
            short += side_shortfall(leaving, lo, hi, &walls, fits);
        }
        (short, blockers)
    }

    /// Whether a move of the parts of `set` can change the shortfall of busy part `e` (an
    /// index of `escapers`): it is one of them, one was in a band in front of it, or one's
    /// reach is there now.
    fn rim_touched(&self, e: usize, set: &[usize]) -> bool {
        let b = self.escapers[e];
        if set.contains(&b) {
            return true;
        }
        let kept = &self.escape_kept[e].1;
        let reach = self.rim_reach(b);
        set.iter()
            .any(|j| kept.contains(j) || reach.is_some_and(|r| self.reach_abs[*j].overlaps(&r)))
    }

    /// The escape shortfall of every busy part, with the parts of `set` as they are now and
    /// the others' shortfalls as last kept.
    pub(super) fn escape_with(&self, set: &[usize]) -> f64 {
        (0..self.escapers.len())
            .map(|e| {
                if self.rim_touched(e, set) {
                    self.rim_cost(self.escapers[e]).0
                } else {
                    self.escape_kept[e].0
                }
            })
            .sum()
    }

    /// Keep the shortfalls that a move of the parts of `set` can change.
    pub(super) fn refresh_escape(&mut self, set: &[usize]) {
        for e in 0..self.escapers.len() {
            if self.rim_touched(e, set) {
                self.escape_kept[e] = self.rim_cost(self.escapers[e]);
            }
        }
    }

    /// Keep every busy part's shortfall.
    pub(super) fn refresh_escape_all(&mut self) {
        for e in 0..self.escapers.len() {
            self.escape_kept[e] = self.rim_cost(self.escapers[e]);
        }
    }
}

#[cfg(test)]
impl<'a> Engine<'a> {
    /// The escape shortfall of every busy part as last kept.
    pub(super) fn escape_kept_total(&self) -> f64 {
        self.escape_kept.iter().map(|k| k.0).sum()
    }

    /// The escape shortfall of every busy part counted again from nothing.
    pub(super) fn escape_counted_afresh(&self) -> f64 {
        self.escapers.iter().map(|&b| self.rim_cost(b).0).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fits(g: f64) -> f64 {
        ((g - 0.2) / 0.4).floor().max(0.0)
    }

    fn ten() -> Vec<f64> {
        (0..10).map(f64::from).collect()
    }

    #[test]
    fn a_side_with_nothing_in_front_of_it_lets_every_pad_leave() {
        assert_eq!(side_shortfall(&ten(), -0.5, 9.5, &[], fits), 0.0);
    }

    #[test]
    fn a_side_against_the_edge_lets_out_as_many_as_fit_along_the_gap_both_ways() {
        // 1.05 mm holds two tracks at 0.4 mm with 0.2 mm of clearance: four leave.
        assert_eq!(
            side_shortfall(&ten(), -0.5, 9.5, &[(-0.5, 9.5, 1.05)], fits),
            6.0
        );
        // At the band's depth nothing blocks.
        assert_eq!(
            side_shortfall(&ten(), -0.5, 9.5, &[(-0.5, 9.5, RING)], fits),
            0.0
        );
    }

    #[test]
    fn a_part_in_front_of_a_side_blocks_only_the_pads_behind_it() {
        // Pads 2 to 7 behind a part 0.3 mm away, where nothing fits.
        assert_eq!(
            side_shortfall(&ten(), -0.5, 9.5, &[(1.8, 7.2, 0.3)], fits),
            6.0
        );
        // Two parts beside each other make one run, held by the nearer.
        assert_eq!(
            side_shortfall(&ten(), -0.5, 9.5, &[(1.8, 4.5, 1.0), (4.5, 7.2, 0.7)], fits),
            4.0
        );
    }
}
