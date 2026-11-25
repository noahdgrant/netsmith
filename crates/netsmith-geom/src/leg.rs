//! The length of a leg of a net's copper measured to the centres of its pads (issue #1670): the
//! copper outside the pad, plus the straight distance from where the leg enters the pad to the
//! pad's centre. Copper inside the pad does not count, so the same copper outside a pad gives the
//! same length wherever the leg enters it: a track that stops at the pad's edge and one drawn on
//! to its centre, or one that bends inside it, measure the same. `board nets` measures a net's
//! connection by it (and so a diff pair's skew and a length group's spread), and the router's
//! meanders match a pair and a group by it, so the two agree.

use crate::drc::end_joins;
use crate::drc::shape::Shape;
use crate::geom::Pt;

/// A point is in a pad when it lies on its copper by this much, in mm: the rule the length trace
/// joins a track's end to a pad by.
const IN: f64 = 1e-3;
/// The samples taken along each straight piece of a leg to find where it enters a pad, before the
/// crossing is narrowed down.
const SAMPLES: usize = 32;
/// The halvings that narrow down a crossing: the place is then within the piece's length over
/// `2^STEPS`, below a nanometre for any piece on a board.
const STEPS: usize = 40;

/// A pad a leg's end is measured to: its copper (`crate::pad_copper`) and its centre.
#[derive(Debug, Clone, Copy)]
pub struct LegPad<'a> {
    pub copper: &'a [Shape],
    pub centre: Pt,
}

impl LegPad<'_> {
    /// Whether `p` is in the pad.
    pub fn holds(&self, p: Pt) -> bool {
        end_joins(p, IN, self.copper)
    }
}

/// The length of the leg `line` (its centre line, straight pieces end to end) with its start in
/// the pad `start` and its end in `end`, either `None` for an end in no pad, measured to the
/// pads' centres, in mm. Where the leg enters the end's pad is the first point of it, from the
/// start, in that pad, and so, from its end back, for the start's pad; a leg whose end is joined
/// to its pad but whose centre line never reaches it (a round end that runs into the pad's edge)
/// enters at its end. A leg wholly in one pad, or in two pads that overlap, measures the straight
/// distance between their centres: none for one pad.
pub fn leg_length(line: &[Pt], start: Option<LegPad<'_>>, end: Option<LegPad<'_>>) -> f64 {
    let along: Vec<f64> = std::iter::once(0.0)
        .chain(line.windows(2).scan(0.0, |s, w| {
            *s += w[0].dist(w[1]);
            Some(*s)
        }))
        .collect();
    let total = along.last().copied().unwrap_or_default();
    if line.len() < 2 {
        return match (start, end) {
            (Some(a), Some(b)) => a.centre.dist(b.centre),
            _ => 0.0,
        };
    }
    let at = |s: f64| point_at(line, &along, s);
    // The leg's ends, as distances along it: where it enters each pad.
    let from = start.map_or(0.0, |pad| {
        entry(&along, total, |s| pad.holds(at(s)), true).unwrap_or(0.0)
    });
    let to = end.map_or(total, |pad| {
        entry(&along, total, |s| pad.holds(at(s)), false).unwrap_or(total)
    });
    if let (Some(a), Some(b)) = (start, end)
        && from >= to
    {
        return a.centre.dist(b.centre);
    }
    let into = |pad: Option<LegPad<'_>>, s: f64| pad.map_or(0.0, |p| at(s).dist(p.centre));
    into(start, from) + (to - from).max(0.0) + into(end, to)
}

/// The point at the distance `s` along `line`, whose pieces end at the distances `along`.
fn point_at(line: &[Pt], along: &[f64], s: f64) -> Pt {
    let k = along
        .windows(2)
        .position(|w| s <= w[1])
        .unwrap_or(along.len().saturating_sub(2));
    let (a, b) = (line[k], line[k + 1]);
    let len = along[k + 1] - along[k];
    let t = if len > 0.0 {
        ((s - along[k]) / len).clamp(0.0, 1.0)
    } else {
        0.0
    };
    a + (b - a) * t
}

/// The distance along a leg of `total` length where it first comes into what `inside` holds,
/// scanning from its start (`backwards` false) or from its end; `None` when no sample is inside.
/// Each piece (between the distances `along`) is sampled [`SAMPLES`] times and the crossing
/// halved [`STEPS`] times.
fn entry(along: &[f64], total: f64, inside: impl Fn(f64) -> bool, backwards: bool) -> Option<f64> {
    let mut samples: Vec<f64> = along
        .windows(2)
        .flat_map(|w| (0..SAMPLES).map(move |k| w[0] + (w[1] - w[0]) * k as f64 / SAMPLES as f64))
        .chain([total])
        .collect();
    if backwards {
        samples.reverse();
    }
    let first = samples.iter().position(|&s| inside(s))?;
    if first == 0 {
        return Some(samples[0]);
    }
    // Narrow down the crossing between the last sample outside and the first inside.
    let (mut out, mut inn) = (samples[first - 1], samples[first]);
    for _ in 0..STEPS {
        let mid = (out + inn) / 2.0;
        if inside(mid) {
            inn = mid;
        } else {
            out = mid;
        }
    }
    Some(inn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(c: Pt, half: f64) -> Vec<Shape> {
        vec![Shape::polygon(
            vec![
                Pt::new(c.x - half, c.y - half),
                Pt::new(c.x + half, c.y - half),
                Pt::new(c.x + half, c.y + half),
                Pt::new(c.x - half, c.y + half),
            ],
            0.0,
        )]
    }

    #[test]
    fn a_leg_to_the_pads_edge_and_one_to_its_centre_measure_the_same() {
        let copper = square(Pt::new(10.0, 0.0), 0.5);
        let pad = LegPad {
            copper: &copper,
            centre: Pt::new(10.0, 0.0),
        };
        let to_centre = leg_length(&[Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)], None, Some(pad));
        let to_edge = leg_length(&[Pt::new(0.0, 0.0), Pt::new(9.6, 0.0)], None, Some(pad));
        assert!((to_centre - 10.0).abs() < 1e-6, "{to_centre}");
        assert!((to_edge - 10.0).abs() < 1e-6, "{to_edge}");
        // Entering at the pad's corner side: 9.5 outside, then straight to the centre.
        let bent = leg_length(
            &[Pt::new(0.0, 0.4), Pt::new(9.5, 0.4), Pt::new(9.9, -0.3)],
            None,
            Some(pad),
        );
        let want = 9.5 + Pt::new(9.5, 0.4).dist(Pt::new(10.0, 0.0));
        // The pad holds a point a micrometre off its edge, so the entry is that much early.
        assert!((bent - want).abs() < 2e-3, "{bent} against {want}");
    }

    #[test]
    fn a_leg_wholly_in_its_pad_measures_nothing() {
        let copper = square(Pt::new(0.0, 0.0), 0.5);
        let pad = LegPad {
            copper: &copper,
            centre: Pt::new(0.0, 0.0),
        };
        let inside = leg_length(
            &[Pt::new(-0.4, 0.0), Pt::new(0.0, 0.2)],
            Some(pad),
            Some(pad),
        );
        assert_eq!(inside, 0.0);
    }
}
