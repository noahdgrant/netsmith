//! What a board's impedance targets ask of its tracks and what the routed copper gives (issue
//! #1025): the findings where no width gives a target, and how much of each pair runs at a gap
//! that gives its impedance.

use super::*;
use crate::impedance::{TEST_TOLERANCE, against, impedance_at, target_layers};
use netsmith_core::ImpedanceTarget;

/// The step a pair's track is sampled at to see what gap its partner keeps, in mm.
const PAIR_SAMPLE: f64 = 0.05;

/// How much of a pair's routed track runs beside its partner at a gap that gives its
/// differential impedance (issue #1025), from [`Board::pair_couplings`].
#[derive(Debug, Clone, PartialEq)]
pub struct PairCoupling {
    /// The path of the target in the design.
    pub path: String,
    /// The two nets, the positive one first.
    pub p: String,
    pub n: String,
    /// The length of the positive net's track, in mm.
    pub length: f64,
    /// The length of it that has the partner's track on its layer at a gap whose differential
    /// impedance is within JLCPCB's test tolerance of the target, in mm.
    pub within: f64,
    /// The gaps, edge to edge, the partner keeps along that length: the least and the most, in
    /// mm; `None` when no length is within.
    pub gaps: Option<(f64, f64)>,
}

impl PairCoupling {
    /// The share of the positive net's track that is within, 0 to 1.
    pub fn share(&self) -> f64 {
        if self.length > 0.0 {
            self.within / self.length
        } else {
            0.0
        }
    }
}

/// The nets of a target as a finding names them: ``net `RF` (rf_z)`` or ``nets `DP` and `DM`
/// (usb_z)``.
fn named(t: &ImpedanceTarget) -> String {
    let nets: Vec<String> = t.nets.iter().map(|n| format!("`{n}`")).collect();
    if nets.len() == 1 {
        format!("net {} ({})", nets[0], t.path)
    } else {
        format!("nets {} ({})", nets.join(" and "), t.path)
    }
}

impl Board {
    /// What the board cannot give its impedance targets (issue #1025), one message each: a
    /// board with no build-up (a two-layer board, ADR 0111), where no width gives an impedance;
    /// a layer where no width gives the target (outside a formula's domain); a layer where the
    /// width that gives it is narrower than the least the net's current needs or the process
    /// makes, so the tracks are drawn wider; a pair whose spacing is under the process's
    /// clearance; and a net that two targets name, which the first sizes. Empty when every
    /// target is met.
    pub fn impedance_findings(&self) -> Vec<String> {
        let targets = &self.impedance_targets;
        let Some(solved) = target_layers(&self.process.stackup, targets) else {
            return targets
                .iter()
                .map(|t| {
                    format!(
                        "{} states {} ohm, but the board records no build-up (a two-layer board), so no width gives it; its tracks are drawn at {} mm",
                        named(t),
                        t.ohms,
                        self.track_width(&t.nets[0])
                    )
                })
                .collect();
        };
        let mut out = Vec::new();
        for (k, (t, layers)) in targets.iter().zip(solved).enumerate() {
            let what = named(t);
            if let Some((net, first)) = t.nets.iter().find_map(|n| {
                targets[..k]
                    .iter()
                    .find(|o| o.nets.contains(n))
                    .map(|o| (n, o))
            }) {
                out.push(format!(
                    "{what}: net `{net}` has a target in `{}` too, which sizes it; this one is not met",
                    first.path
                ));
                continue;
            }
            let net = &t.nets[0];
            for l in &layers {
                let Some(layer) = CopperLayer::of_name(&l.layer) else {
                    continue;
                };
                let drawn = self.track_width_on(net, layer);
                match &l.width {
                    Err(e) => out.push(format!(
                        "{what}: no width gives {} ohm on {}: {e}; its tracks there are drawn at {drawn} mm",
                        t.ohms, l.layer
                    )),
                    Ok(w) if self.target_width(net, layer).is_some_and(|t| drawn > t + 1e-9) => {
                        let floor = if self.least_width_on(net, layer)
                            > self.rules.min_track_width + 1e-9
                        {
                            "the least width its current needs"
                        } else {
                            "the narrowest track the process makes"
                        };
                        let gives = match impedance_at(
                            &self.process.stackup,
                            &l.layer,
                            drawn,
                            t.spacing,
                        ) {
                            Ok(z) => format!("{z:.1} ohm ({})", against(t.ohms, z)),
                            Err(e) => format!("no figure ({e})"),
                        };
                        out.push(format!(
                            "{what}: {} ohm needs {w:.3} mm on {}, narrower than {floor}; its tracks there are drawn at {drawn} mm, which gives {gives}",
                            t.ohms, l.layer
                        ));
                    }
                    Ok(_) => {}
                }
            }
            if let Some(s) = t.spacing
                && s < self.rules.clearance - 1e-9
            {
                out.push(format!(
                    "{what}: the pair's spacing, {s} mm, is under the process's clearance, {} mm; its tracks are kept {} mm apart, which the width does not allow for",
                    self.rules.clearance, self.rules.clearance
                ));
            }
        }
        out
    }

    /// How much of each pair with an impedance target runs at a gap that gives its impedance,
    /// once the board is routed (issue #1025): the positive net's track, sampled every 0.05 mm,
    /// against the nearest track of the partner on the same layer, whose gap, edge to edge, with
    /// the two widths drawn there, gives a differential impedance within JLCPCB's test tolerance
    /// ([`TEST_TOLERANCE`]). Empty for a board with no build-up or no pair, and for a pair with
    /// no track.
    pub fn pair_couplings(&self) -> Vec<PairCoupling> {
        let mut out = Vec::new();
        for t in &self.impedance_targets {
            let (Some(_), [p, n]) = (t.spacing, t.nets.as_slice()) else {
                continue;
            };
            let own: Vec<&Track> = self.tracks.iter().filter(|k| &k.net == p).collect();
            if own.is_empty() {
                continue;
            }
            let theirs: Vec<&Track> = self.tracks.iter().filter(|k| &k.net == n).collect();
            let (mut length, mut within) = (0.0, 0.0);
            let mut gaps: Option<(f64, f64)> = None;
            for track in own {
                let line = track.centre_line();
                for w in line.windows(2) {
                    let piece = w[0].distance(w[1]);
                    let steps = ((piece / PAIR_SAMPLE).ceil() as usize).max(1);
                    let step = piece / steps as f64;
                    for k in 0..steps {
                        let f = (k as f64 + 0.5) / steps as f64;
                        let at = Pt::new(
                            w[0].x + (w[1].x - w[0].x) * f,
                            w[0].y + (w[1].y - w[0].y) * f,
                        );
                        length += step;
                        let Some((gap, other)) = theirs
                            .iter()
                            .filter(|o| o.layer == track.layer)
                            .map(|o| (o.distance_to(at) - o.width / 2.0 - track.width / 2.0, o))
                            .min_by(|a, b| a.0.total_cmp(&b.0))
                        else {
                            continue;
                        };
                        // The pair's impedance with the narrower of the two widths there.
                        let width = track.width.min(other.width);
                        let ok = gap > 0.0
                            && impedance_at(
                                &self.process.stackup,
                                track.layer.name(),
                                width,
                                Some(gap),
                            )
                            .is_ok_and(|z| (z - t.ohms).abs() <= t.ohms * TEST_TOLERANCE);
                        if ok {
                            within += step;
                            gaps = Some(gaps.map_or((gap, gap), |(lo, hi)| {
                                (lo.min(gap), hi.max(gap))
                            }));
                        }
                    }
                }
            }
            out.push(PairCoupling {
                path: t.path.clone(),
                p: p.clone(),
                n: n.clone(),
                length,
                within,
                gaps,
            });
        }
        out
    }
}
