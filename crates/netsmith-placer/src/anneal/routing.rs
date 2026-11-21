//! The placement objective for routing (issue #1534, step 4 of #1517): what a placement is
//! scored by besides its wire, so that the router has less to untangle.
//!
//! It is off unless the caller asks for it (`PlaceOptions::routing_objective`, 0 by default,
//! `netsmith_pcb::ROUTING_OBJECTIVE` the weight it was measured at), so every placement made
//! before it is made again as it was. With weight `w` it adds three things:
//!
//! - **The power chain, in the order its current flows.** A net whose current the design knows
//!   (`Features::net_currents`) and is [`CHAIN_CURRENT`] or more, that is no plane and has at
//!   most `MAX_LINE_PADS` pads, weighs `1 + w * I` times as much (`I` in amperes, at most
//!   [`MOST_AMPS`]). The input power chain (connector, protection, regulator, inductor, output
//!   capacitors) is a run of such nets, each joining one stage to the next, so the stages are
//!   drawn together one after another: a placement that folds the chain back, or leaves a
//!   stage away from the next, pays for the heavy nets twice. When the faces are shared
//!   (`fit` and parts that do not fit one face, `place::arrange_shared`), the parts of the
//!   chain go together to the face the first of them takes, as long as it has the room, so the
//!   chain's tracks need no via. A return that is a plane is not a net of the chain; one
//!   that is not (a small board's ground) is, and takes the parts it joins with it.
//! - **A part beside the pins it joins** (a memory beside its controller). A part that shares
//!   [`BUS_NETS`] or more nets of two or three pads with one other part is on a bus of that
//!   part (the part with fewer pads is the satellite, the other the host). Each net of the bus
//!   weighs `1 + w * BUS_WEIGHT` times as much, and the corridor, the box round the pads of
//!   both parts on the bus's nets, is kept clear: each mm a third part's room on the
//!   satellite's face reaches into it costs `w * W_CORRIDOR`. A part on one of the bus's nets
//!   (a pull-up) and a capacitor that decouples either part are not counted: their place is
//!   by those pins.
//! - **Nothing under a dense module's escape.** A fine-pitch part's escape ring (the box round
//!   its pads grown by `RING`, 1.5 mm) is kept clear on its own face already, as a hard rule;
//!   on the other face a part may stand under it, where the fanout vias of the ring's tracks
//!   go through the board. With the objective each mm a part on the other face reaches under
//!   the ring costs `w * W_UNDER`. A capacitor that decouples the part is not counted: under
//!   the pins it decouples is its place.
//!
//! The weights are chosen, not tuned: the corridor and the ring weigh what a decoupling
//! capacitor's pull to its pins does (`W_DECOUPLING`, 6 a mm), which a pull of many nets may
//! still beat, and nothing is a hard rule, so a board that placed before still places.
//!
//! **Measured** (`cargo test -p netsmith-fabricate --test main
//! placement_for_routing::measured -- --ignored --nocapture`): each board placed with and
//! without the objective and routed, the router's objective of `board nets` (vias, layer
//! changes, detour, nets over twice their straight length) and the counts of this objective.
//! Debug build, seed 0 of each example, 2026-11-21 (`under` the parts under a fine-pitch part's
//! ring, `corridor` the parts in a bus's corridor, `chain` the half perimeters of the chain's
//! nets in mm; the router left no line on any board):
//!
//! | board | under | corridor | chain | vias | layer changes | detour | over twice |
//! |---|---|---|---|---|---|---|---|
//! | led, status_indicator, four_layer, six_layer | 0 to 0 | 0 to 0 | unchanged | unchanged | unchanged | unchanged | 0 to 0 |
//! | linear_regulator | 0 to 0 | 0 to 0 | 22.0 to 15.8 | 0 to 0 | 0 to 0 | 1.051 to 1.042 | 0 to 0 |
//! | generated 1 | 4 to 0 | 0 to 0 | 17.0 to 15.2 | 12 to 6 | 12 to 6 | 1.180 to 1.116 | 1 to 0 |
//! | generated 2 | 4 to 1 | 1 to 0 | 16.0 to 14.6 | 8 to 8 | 8 to 8 | 1.086 to 1.073 | 0 to 0 |
//! | generated 3 | 4 to 0 | 1 to 0 | 18.1 to 15.9 | 8 to 10 | 8 to 10 | 1.094 to 1.124 | 0 to 0 |
//!
//! The examples with none of the three (no current of 0.1 A, no bus, one face) are placed as
//! before: the run draws the same moves. On the generated boards (`routing_board` of the
//! placer's tests) the placement does what it is for on every seed, and the router's objective
//! is better on two of three and worse on one, so the objective stays off by default until a
//! wider measure shows it is clearly better. How a person turns it on is still open.
//!
//! The counts the tests assert
//! (`crates/netsmith-placer/tests/routing_objective.rs`) come from
//! [`crate::routing_counts`], which measures a placed board the same way.

use super::*;

/// The least current, in amperes, from which a net is a power net of the chain.
pub(crate) const CHAIN_CURRENT: f64 = 0.1;
/// The most current, in amperes, that adds to a power net's weight.
const MOST_AMPS: f64 = 3.0;
/// The fewest nets a part shares with one other part to be on a bus of it.
pub(crate) const BUS_NETS: usize = 4;
/// The most pads a net of a bus has: the two parts', and one more (a pull-up).
pub(crate) const BUS_PADS: usize = 3;
/// What each net of a bus weighs on top of a plain net, times the objective's weight.
pub(super) const BUS_WEIGHT: f64 = 1.0;
/// Each mm a third part's room reaches into a bus's corridor.
const W_CORRIDOR: f64 = W_DECOUPLING;
/// Each mm a part on the other face reaches under a fine-pitch part's escape ring.
const W_UNDER: f64 = W_DECOUPLING;

/// A part on a bus of another, as the annealer keeps it.
pub(super) struct Bus {
    /// The satellite (the part with fewer pads) and its host.
    pub(super) part: usize,
    pub(super) host: usize,
    /// The pads of the two on the bus's nets, (part, pad).
    pub(super) pads: Vec<(usize, usize)>,
    /// For each part, whether it has a pad on one of the bus's nets.
    pub(super) on: Vec<bool>,
}

/// How many times a net of `current` amperes weighs at objective weight `w`: 1 for a net
/// below [`CHAIN_CURRENT`].
pub(super) fn power_factor(current: f64, w: f64) -> f64 {
    if current < CHAIN_CURRENT {
        1.0
    } else {
        1.0 + w * current.min(MOST_AMPS)
    }
}

/// The buses of the board: for each two parts that share at least [`BUS_NETS`] nets of two
/// or three pads (no plane), the one with fewer pads on a bus of the other, with the indices of
/// the nets. `pads_of` is each part's number of pads.
pub(crate) fn find_buses(
    nets: &[(bool, Vec<(usize, usize)>)],
    pads_of: &[usize],
) -> Vec<(usize, usize, Vec<usize>)> {
    let mut shared: std::collections::BTreeMap<(usize, usize), Vec<usize>> = Default::default();
    for (id, (plane, pads)) in nets.iter().enumerate() {
        if *plane || pads.len() > BUS_PADS {
            continue;
        }
        let mut parts: Vec<usize> = pads.iter().map(|&(i, _)| i).collect();
        parts.sort_unstable();
        parts.dedup();
        for (k, &a) in parts.iter().enumerate() {
            for &b in &parts[k + 1..] {
                shared.entry((a, b)).or_default().push(id);
            }
        }
    }
    shared
        .into_iter()
        .filter(|(_, ids)| ids.len() >= BUS_NETS)
        .map(|((a, b), ids)| {
            if pads_of[a] <= pads_of[b] {
                (a, b, ids)
            } else {
                (b, a, ids)
            }
        })
        .collect()
}

/// The power nets of the chain, by index into `nets`: a current of [`CHAIN_CURRENT`] or more,
/// no plane, and at most `MAX_LINE_PADS` pads.
pub(crate) fn chain_nets(
    nets: &[BoardNet],
    currents: &[(String, f64)],
    planes: &[String],
) -> Vec<usize> {
    nets.iter()
        .enumerate()
        .filter(|(_, net)| {
            !planes.contains(&net.name)
                && net.pads.len() >= 2
                && net.pads.len() <= MAX_LINE_PADS
                && currents
                    .iter()
                    .any(|(n, c)| *n == net.name && *c >= CHAIN_CURRENT)
        })
        .map(|(k, _)| k)
        .collect()
}

/// For each part, whether it has a pad on a net of the power chain ([`chain_nets`]).
pub(crate) fn chain_parts(
    parts: &[Placed],
    nets: &[BoardNet],
    currents: &[(String, f64)],
    planes: &[String],
) -> Vec<bool> {
    let mut on = vec![false; parts.len()];
    for k in chain_nets(nets, currents, planes) {
        for pad in &nets[k].pads {
            if let Some(i) = parts.iter().position(|p| p.refdes == pad.refdes) {
                on[i] = true;
            }
        }
    }
    on
}

impl<'a> Engine<'a> {
    /// The box round the pads of bus `b`'s two parts on its nets, where they are now.
    pub(super) fn corridor(&self, b: usize) -> Rect {
        let bus = &self.buses[b];
        let mut it = bus.pads.iter().map(|&(i, k)| self.pad_at(i, k));
        let first = it.next().unwrap_or_default();
        it.fold(Rect::new(first, first), |r, p| r.union(&Rect::new(p, p)))
    }

    /// How far part `j`'s room on the satellite's face reaches into bus `b`'s corridor, in mm:
    /// 0 for the bus's own parts, a part on one of its nets, a capacitor that decouples either
    /// part, and while the two parts are on different faces.
    pub(super) fn in_corridor(&self, b: usize, j: usize) -> f64 {
        let bus = &self.buses[b];
        let n = self.models.len();
        let (part, host) = (bus.part, bus.host);
        if bus.on[j]
            || self.near_ok[part * n + j]
            || self.near_ok[host * n + j]
            || self.models[part].side != self.models[host].side
        {
            return 0.0;
        }
        let corridor = self.corridor(b);
        self.rooms_on(j, self.models[part].side)
            .iter()
            .map(|(room, _)| penetration(room, &corridor, 0.0))
            .fold(0.0, f64::max)
    }

    /// What bus `b`'s corridor costs: every third part in it.
    pub(super) fn bus_cost(&self, b: usize) -> f64 {
        let depth: f64 = (0..self.models.len()).map(|j| self.in_corridor(b, j)).sum();
        self.w_routing * W_CORRIDOR * depth
    }

    /// How far part `j`, on the other face from fine-pitch part `c`, reaches under `c`'s escape
    /// ring, in mm. A part with a hole is measured on `c`'s face already (`ring_depth`), and a
    /// capacitor that decouples `c` is not counted.
    pub(super) fn under_depth(&self, c: usize, j: usize) -> f64 {
        let n = self.models.len();
        let (mc, mj) = (&self.models[c], &self.models[j]);
        if c == j || mc.side == mj.side || mj.hole || self.near_ok[c * n + j] {
            return 0.0;
        }
        let Some(ring) = self.shape(c).ring else {
            return 0.0;
        };
        let ring = ring.translated(self.pos(c));
        self.courts(j)
            .map(|court| penetration(&court, &ring, 0.0))
            .fold(0.0, f64::max)
    }

    /// What parts `i` and `j` cost by standing under each other's escape ring.
    pub(super) fn under_cost(&self, i: usize, j: usize) -> f64 {
        self.w_routing * W_UNDER * (self.under_depth(i, j) + self.under_depth(j, i))
    }

    /// The cost of the buses that involve a part of `set`, and of each part of `set` in the
    /// corridor of any other bus; each bus is counted once (`bus_mark`).
    pub(super) fn buses_with(&mut self, set: &[usize], stamp: u32) -> f64 {
        let mut cost = 0.0;
        for &i in set {
            for k in 0..self.buses_of[i].len() {
                let b = self.buses_of[i][k];
                if self.bus_mark[b] != stamp {
                    self.bus_mark[b] = stamp;
                    cost += self.bus_cost(b);
                }
            }
        }
        for &i in set {
            for b in 0..self.buses.len() {
                if self.bus_mark[b] != stamp {
                    cost += self.w_routing * W_CORRIDOR * self.in_corridor(b, i);
                }
            }
        }
        cost
    }
}
