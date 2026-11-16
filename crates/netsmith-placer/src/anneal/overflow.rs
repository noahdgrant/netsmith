//! Track overflow (issue #820): the track the nets ask of the board, bin by bin, beyond the
//! track a bin holds.
//!
//! The board's region is cut into square bins. Each net whose ratsnest lines are counted (not
//! a plane, at most [`MAX_LINE_PADS`] pads) wants its half perimeter of track, spread evenly
//! over the box round its pad centres grown by half a track pitch (the rectangular uniform
//! wire density estimate). Each pad takes the room its copper and half the clearance round
//! it cover. A bin holds track on each signal layer (the two outer layers and every inner layer
//! that is not a plane) at one track pitch (the process's track width and clearance) over [`USABLE`] of its area. What a bin is asked for beyond what it
//! holds is its overflow; the cost is the sum of the overflows, in mm of track, times
//! `Problem::overflow` (`PlaceOptions::track_overflow`), which is 0, and the term left out, unless
//! a caller asks for it.
//!
//! **Measured** (`cargo test -p netsmith-router --test main track_overflow::measured --
//! --ignored --nocapture`), as the lines one pass of the router leaves (no other orders, no
//! rip-up, no negotiation: a fixed amount of work), seeds 0 to 5 unless said:
//!
//! | board | without | at 1 | at 4 |
//! |---|---|---|---|
//! | the tight board (99 parts on both faces, 27 x 25 mm) | 138 | 163 | 155 |
//! | the mixed board (101 parts, 35 x 60 mm) | 86 | 75 | 80 |
//!
//! (Seeds 0 to 7 of the tight board: 202 without, 206 at 1.) On the tight board the term at 1
//! halves the overflow (38.7 to 19.5 mm on seed 0, 37.8 to 22.7 on seed 1) and the router
//! leaves no fewer lines, so the overflow of the bins is not what leaves them (the router's
//! tests of that board, `tight.rs`, find them on the module of 80 fine-pitch pads). With the
//! nets alone and no pads the overflow is a few mm and the lines left are more (56 against 41
//! on seeds 0 and 1). From one seed to the next a board's count moves by five lines or more:
//! the mixed board's gain is inside that, the tight board loses at both weights, and the wire
//! grows by about a tenth at 4. So the weight stays 0 until a board shows a gain (issue #820).
//!
//! Lengths are kept as whole micrometres, so that a change added and taken away again leaves
//! the same numbers, and the run stays a function of its input.

use super::*;

/// The least side of a bin, in mm.
const BIN: f64 = 1.0;
/// At most this many bins along a side of the region: a large board has larger bins.
const MAX_BINS: f64 = 64.0;
/// The share of a bin's area that track may take: the rest is the space round vias, bends and
/// the ends of tracks.
const USABLE: f64 = 0.5;
/// Lengths are kept in micrometres.
const MICRONS: f64 = 1000.0;

/// What one net or one part asks of the bins: (bin, length in micrometres).
pub(super) type Use = Vec<(u32, i64)>;

/// The bins and what is asked of each, as the parts are now (kept up to date by
/// [`Engine::refresh`]).
pub(super) struct Bins {
    origin: Pt,
    bin: f64,
    cols: usize,
    rows: usize,
    /// The track a bin holds, in micrometres.
    room: i64,
    /// The track pitch: the process's track width and clearance, in mm.
    pitch: f64,
    /// What each bin is asked for, in micrometres.
    asked: Vec<i64>,
    /// What each net and each part asks, as the parts are now.
    nets: Vec<Use>,
    parts: Vec<Use>,
    /// The sum of the bins' overflows, in micrometres.
    over: i64,
    /// A change being measured: what it adds to each bin, and the bins it touches.
    change: Vec<i64>,
    touched: Vec<u32>,
    marked: Vec<bool>,
}

impl Bins {
    /// Bins over `region` that hold track on `layers` layers (the outer two and the inner
    /// signal layers; never fewer than one).
    pub(super) fn new(
        region: &Rect,
        rules: &Rules,
        layers: usize,
        nets: usize,
        parts: usize,
    ) -> Bins {
        let side = region.width().max(region.height()).max(0.0);
        let bin = BIN.max(side / MAX_BINS);
        let cols = ((region.width() / bin).ceil() as usize).max(1);
        let rows = ((region.height() / bin).ceil() as usize).max(1);
        let pitch = (rules.track_width + rules.clearance).max(0.05);
        let room = (layers.max(1) as f64 * USABLE * bin * bin / pitch * MICRONS).round() as i64;
        Bins {
            origin: region.min,
            bin,
            cols,
            rows,
            room,
            pitch,
            asked: vec![0; cols * rows],
            nets: vec![Vec::new(); nets],
            parts: vec![Vec::new(); parts],
            over: 0,
            change: vec![0; cols * rows],
            touched: Vec::new(),
            marked: vec![false; cols * rows],
        }
    }

    /// `length` mm of track spread evenly over the box `b`, as it falls in the bins; what falls
    /// outside the region is not counted.
    fn spread(&self, b: &Rect, length: f64, out: &mut Use) {
        let area = b.width() * b.height();
        if area <= 0.0 || length <= 0.0 {
            return;
        }
        let density = length / area;
        let index = |v: f64, lo: f64, n: usize| -> Option<usize> {
            let k = ((v - lo) / self.bin).floor();
            (k >= 0.0).then(|| (k as usize).min(n - 1))
        };
        let (x0, x1) = (b.min.x - self.origin.x, b.max.x - self.origin.x);
        let (y0, y1) = (b.min.y - self.origin.y, b.max.y - self.origin.y);
        if x1 <= 0.0 || y1 <= 0.0 {
            return;
        }
        let first_x = index(b.min.x, self.origin.x, self.cols).unwrap_or(0);
        let first_y = index(b.min.y, self.origin.y, self.rows).unwrap_or(0);
        for row in first_y..self.rows {
            let (lo, hi) = (row as f64 * self.bin, (row + 1) as f64 * self.bin);
            if lo >= y1 {
                break;
            }
            let h = hi.min(y1) - lo.max(y0);
            if h <= 0.0 {
                continue;
            }
            for col in first_x..self.cols {
                let (lo, hi) = (col as f64 * self.bin, (col + 1) as f64 * self.bin);
                if lo >= x1 {
                    break;
                }
                let w = hi.min(x1) - lo.max(x0);
                if w <= 0.0 {
                    continue;
                }
                let amount = (density * w * h * MICRONS).round() as i64;
                if amount > 0 {
                    out.push(((row * self.cols + col) as u32, amount));
                }
            }
        }
    }

    fn overflow(&self, asked: i64) -> i64 {
        (asked - self.room).max(0)
    }

    /// Adds `sign` times `what` to the change being measured.
    fn mark(&mut self, what: &[(u32, i64)], sign: i64) {
        for &(b, v) in what {
            let b = b as usize;
            if !self.marked[b] {
                self.marked[b] = true;
                self.touched.push(b as u32);
            }
            self.change[b] += sign * v;
        }
    }

    /// How much the overflow grows by the change marked, and clears it.
    fn settle(&mut self) -> i64 {
        let mut grows = 0;
        for &b in &self.touched {
            let b = b as usize;
            let now = self.asked[b];
            grows += self.overflow(now + self.change[b]) - self.overflow(now);
            self.change[b] = 0;
            self.marked[b] = false;
        }
        self.touched.clear();
        grows
    }

    /// Replaces what is asked by `old` with `new` in the bins.
    fn commit(&mut self, old: &[(u32, i64)], new: &[(u32, i64)]) {
        for (&(b, v), sign) in old
            .iter()
            .map(|x| (x, -1))
            .chain(new.iter().map(|x| (x, 1)))
        {
            let b = b as usize;
            let before = self.overflow(self.asked[b]);
            self.asked[b] += sign * v;
            self.over += self.overflow(self.asked[b]) - before;
        }
    }
}

impl<'a> Engine<'a> {
    /// What net `n` asks of the bins as the parts are now: its half perimeter, over the box
    /// round its pad centres grown by half a track pitch. Nothing for a net whose lines are not
    /// counted.
    pub(super) fn net_use(&self, n: usize) -> Use {
        let mut out = Vec::new();
        if !self.nets[n].lines {
            return out;
        }
        let Some(b) = Rect::around(self.nets[n].pads.iter().map(|&(i, k)| self.pad_at(i, k)))
        else {
            return out;
        };
        let length = b.width() + b.height();
        self.bins
            .spread(&b.expanded(self.bins.pitch / 2.0), length, &mut out);
        out
    }

    /// The room part `i`'s pads take as the part is now: each pad's copper and half the
    /// clearance round it, as track of one pitch.
    pub(super) fn pad_use(&self, i: usize) -> Use {
        let mut out = Vec::new();
        let at = self.pos(i);
        let grow = self.p.rules.clearance / 2.0;
        for pad in &self.shape(i).pad_boxes {
            let b = pad.translated(at).expanded(grow);
            let length = b.width() * b.height() / self.bins.pitch;
            self.bins.spread(&b, length, &mut out);
        }
        out
    }

    /// The overflow of the bins, in mm of track, with the nets `nets` and the parts `set` as
    /// they are now and everything else as last kept.
    pub(super) fn overflow_with(&mut self, nets: &[usize], set: &[usize]) -> f64 {
        for &n in nets {
            let now = self.net_use(n);
            let kept = std::mem::take(&mut self.bins.nets[n]);
            self.bins.mark(&kept, -1);
            self.bins.mark(&now, 1);
            self.bins.nets[n] = kept;
        }
        for &i in set {
            let now = self.pad_use(i);
            let kept = std::mem::take(&mut self.bins.parts[i]);
            self.bins.mark(&kept, -1);
            self.bins.mark(&now, 1);
            self.bins.parts[i] = kept;
        }
        let grows = self.bins.settle();
        (self.bins.over + grows) as f64 / MICRONS
    }

    /// Keep what net `n` asks of the bins as the parts are now.
    pub(super) fn refresh_bins_net(&mut self, n: usize) {
        let now = self.net_use(n);
        let kept = std::mem::take(&mut self.bins.nets[n]);
        self.bins.commit(&kept, &now);
        self.bins.nets[n] = now;
    }

    /// Keep what part `i`'s pads take of the bins as the part is now.
    pub(super) fn refresh_bins_part(&mut self, i: usize) {
        let now = self.pad_use(i);
        let kept = std::mem::take(&mut self.bins.parts[i]);
        self.bins.commit(&kept, &now);
        self.bins.parts[i] = now;
    }
}

#[cfg(test)]
impl<'a> Engine<'a> {
    /// The overflow of the bins as last kept, in mm of track.
    pub(super) fn overflow(&self) -> f64 {
        self.bins.over as f64 / MICRONS
    }

    /// The overflow of the bins counted again from nothing, as the parts are now, in mm of
    /// track.
    pub(super) fn overflow_counted_afresh(&self) -> f64 {
        let mut asked = vec![0i64; self.bins.asked.len()];
        let uses = (0..self.nets.len())
            .map(|n| self.net_use(n))
            .chain((0..self.models.len()).map(|i| self.pad_use(i)));
        for u in uses {
            for (b, v) in u {
                asked[b as usize] += v;
            }
        }
        asked.iter().map(|&d| self.bins.overflow(d)).sum::<i64>() as f64 / MICRONS
    }
}
