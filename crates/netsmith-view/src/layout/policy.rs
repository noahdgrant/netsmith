//! Which nets are drawn as wires and which as names.
//!
//! A person wires what is near and local, a decoupling capacitor to its pin, a divider, the
//! spine of a regulator, and writes a name where a wire would run across the page or across
//! other wires. This is that rule, as numbers, applied to a first drawing of the sheet in
//! which every net is a wire:
//!
//! 1. Only a net of **more than one group of parts** may be named. A group is a main part
//!    (an IC, a connector) with the small parts that serve its pins: a net inside one group
//!    (a decoupling capacitor on its pin, a pull-up, a divider, a bootstrap capacitor, the
//!    feedback of a regulator) is only wires. A rail, a ground and a net that goes to
//!    another sheet (which is always named, by a flag) are not chosen here. A net with a
//!    generated name is named by its main part's first pin, `J2.3`, as KiCad does.
//! 2. A net whose wire is longer than [`LONG_STEPS`] grid steps is named.
//! 3. A net that fans out to three groups or more, and whose wire is longer than
//!    [`FAN_OUT_STEPS`], is named.
//! 4. While a wire of a net crosses more than [`MAX_CROSSINGS`] wires of other nets, the net
//!    with the most crossings (then the longest, then the first by name) is named, and the
//!    crossings are counted again without it.
//!
//! The nets are chosen from one drawing and drawn again; [`PASSES`] bounds how many times.
//! The choice is a pure function of the drawing, so the same design gives the same names.

use std::collections::BTreeMap;

use super::engine::Drawn;
use super::*;
use crate::sheetview::{Rel, Seg, relate, simplify};

/// A wire longer than this, in grid steps (61 mm), is named instead.
pub(super) const LONG_STEPS: f64 = 48.0;
/// A net that joins three groups or more is named when its wire is longer than this.
pub(super) const FAN_OUT_STEPS: f64 = 30.0;
/// A net may cross this many wires of other nets and stay a wire.
pub(super) const MAX_CROSSINGS: usize = 1;
/// How many times a sheet is drawn again with more nets named.
pub(super) const PASSES: usize = 3;

/// The nets to draw as names after seeing `drawn`, which has `labelled` named already: a
/// superset of `labelled`.
pub(super) fn choose(ctx: &Ctx, drawn: &Drawn, labelled: &[bool]) -> Vec<bool> {
    let nets = ctx.design.nets.len();
    let mut out = labelled.to_vec();

    // The wire of every net, as straight runs.
    let mut segs: Vec<Seg> = Vec::new();
    for w in &drawn.layout.wires {
        for pair in simplify(&w.points).windows(2) {
            segs.push(Seg {
                net: w.net,
                a: pair[0],
                b: pair[1],
            });
        }
    }
    let mut length = vec![0.0; nets];
    for s in &segs {
        length[s.net] += s.length() / GRID;
    }

    let candidate = |n: usize| {
        !ctx.external[n] && !ctx.ground[n] && ctx.power[n].is_none() && drawn.clusters[n] >= 2
    };
    let candidates: Vec<usize> = (0..nets).filter(|&n| candidate(n) && !out[n]).collect();

    for &n in &candidates {
        if length[n] > LONG_STEPS || (drawn.clusters[n] >= 3 && length[n] > FAN_OUT_STEPS) {
            out[n] = true;
        }
    }

    // Crossings of the nets still wired, the worst named first.
    loop {
        let mut crossings: BTreeMap<usize, usize> = BTreeMap::new();
        for (i, s) in segs.iter().enumerate() {
            if out[s.net] {
                continue;
            }
            for t in &segs[i + 1..] {
                if s.net == t.net || out[t.net] {
                    continue;
                }
                if matches!(relate(s, t), Rel::Cross(_)) {
                    for n in [s.net, t.net] {
                        if candidate(n) {
                            *crossings.entry(n).or_default() += 1;
                        }
                    }
                }
            }
        }
        let worst = crossings
            .into_iter()
            .filter(|&(n, c)| c > MAX_CROSSINGS && !out[n])
            .max_by(|&(a, ca), &(b, cb)| {
                ca.cmp(&cb)
                    .then(length[a].total_cmp(&length[b]))
                    .then_with(|| ctx.design.nets[b].name.cmp(&ctx.design.nets[a].name))
            })
            .map(|(n, _)| n);
        match worst {
            Some(n) => out[n] = true,
            None => break,
        }
    }
    out
}
