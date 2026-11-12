//! The pin optimizer (ADR 0101): swaps and moves of the pins that pin requests got, so that a
//! board that does not route may.
//!
//! The pins of the requests are chosen before anything is placed (`netsmith_check::assign`), so
//! the first pin map is arbitrary with respect to routing. Once a board is placed, the nets of
//! its pads say what a different pin would mean: a request on another pin of the same part puts
//! that pin's pads on the request's net. [`improve`] tries those changes on a copy of the placed
//! board, scores each with the measures `netsmith-pcb` already has (crossings of the ratsnest,
//! then its length, from pad centres, without routing), and keeps the best one while it lowers
//! the score. It moves only what the check says may move: a request whose entry is not locked,
//! to a pin that offers its capability and that the source does not hold, among the pins of its
//! own part, so that every `shared` limit of the part still holds ([`PartSpace::allows`]).
//!
//! It is deterministic. The candidates are tried in a fixed order (parts as the check lists
//! them, requests in the byte order of their paths, pins in the byte order of their paths) and
//! a candidate replaces the best only when its score is lower, so equal scores keep the first.
//! Nothing here reads a clock but the [`Budget`], which ends the search early: it is asked before
//! every candidate is scored, because one score is quadratic in the lines of the ratsnest and a
//! round of candidates can run far past a deadline that is only looked at between rounds.
//!
//! The loop around the pipeline (place and route again with the new map, keep it when it is no
//! worse, at most [`MAX_ROUNDS`] times) is in `fab`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use netsmith_check::{EntrySource, PartSpace, PinFile};
use netsmith_core::Design;
use netsmith_pcb::board::{BoardNet, PadRef};
use netsmith_pcb::{Board, Ratsnest};
use netsmith_place_route::crossings;

/// How many times `fab` goes round (search, place, route) before it stops (owner decision of
/// 2026-11-09, Q16 of `docs/research/pin-assignment-options.md`).
pub const MAX_ROUNDS: usize = 3;

/// A cap on the changes one search applies. Every change lowers the score, so a search ends by
/// itself; this stops a pathological board from running long.
const MAX_CHANGES: usize = 200;

/// A source of time: how long since some start. The real one reads the system clock; a test gives
/// one that it moves itself, so that what a run does at the end of its budget does not depend on
/// how fast the machine is.
#[derive(Clone)]
pub struct Clock(Arc<dyn Fn() -> Duration + Send + Sync>);

impl Clock {
    /// The system clock.
    pub fn real() -> Clock {
        let base = Instant::now();
        Clock(Arc::new(move || base.elapsed()))
    }

    /// A clock that answers with `now` each time it is asked.
    pub fn from_fn(now: impl Fn() -> Duration + Send + Sync + 'static) -> Clock {
        Clock(Arc::new(now))
    }

    fn now(&self) -> Duration {
        (self.0)()
    }
}

impl std::fmt::Debug for Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Clock")
    }
}

impl Default for Clock {
    fn default() -> Self {
        Clock::real()
    }
}

/// The time the pin optimizer has: one amount, shared by the search and every round of routing
/// after it, counted from when the budget was started (the first routing failure).
#[derive(Debug, Clone)]
pub struct Budget {
    total: Option<Duration>,
    started: Duration,
    clock: Clock,
}

impl Budget {
    /// A budget of `total` (`None`: no limit) that starts now on `clock`.
    pub fn start(total: Option<Duration>, clock: &Clock) -> Budget {
        Budget {
            total,
            started: clock.now(),
            clock: clock.clone(),
        }
    }

    /// No limit, on the system clock.
    pub fn unlimited() -> Budget {
        Budget::start(None, &Clock::real())
    }

    /// What is left: `None` when there is no limit, else the time to the end, zero once spent.
    /// Each call reads the clock, so the answer is for this moment.
    pub fn left(&self) -> Option<Duration> {
        self.total
            .map(|total| total.saturating_sub(self.clock.now().saturating_sub(self.started)))
    }

    /// Whether there was a limit and it is used up.
    pub fn spent(&self) -> bool {
        self.left().is_some_and(|left| left.is_zero())
    }
}

/// How good the pin map of a placed board looks without routing: how many pairs of ratsnest
/// lines of different nets cross, then the length of the ratsnest in micrometres. Lower is
/// better, crossings first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Score {
    pub crossings: usize,
    pub ratsnest_um: u64,
}

impl Score {
    /// The score of a board as its nets are.
    pub fn of(board: &Board) -> Score {
        let ratsnest = Ratsnest::of(board);
        Score {
            crossings: crossings(&ratsnest.lines),
            ratsnest_um: (ratsnest.total_length() * 1000.0).round() as u64,
        }
    }
}

impl std::fmt::Display for Score {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} crossings, ratsnest {:.1} mm",
            self.crossings,
            self.ratsnest_um as f64 / 1000.0
        )
    }
}

/// A better pin map for a placed board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// The pin of each request that moves, by request path: `p09`.
    pub picks: BTreeMap<String, String>,
    /// What the changes were, in the order they were made, one line each.
    pub moves: Vec<String>,
    pub before: Score,
    pub after: Score,
}

impl Proposal {
    /// The pin file `file` with the moved requests on their new pins, in the table of `board`.
    /// A moved entry is the tool's choice (`source = "auto"`); its lock, if any, is untouched,
    /// because a locked entry never moves.
    pub fn applied_to(&self, file: &PinFile, board: &str) -> PinFile {
        let mut out = file.clone();
        if let Some(entries) = out.boards.get_mut(board) {
            for (key, pin) in &self.picks {
                if let Some(e) = entries.get_mut(key) {
                    e.pin = pin.clone();
                    e.source = EntrySource::Auto;
                }
            }
        }
        out
    }
}

/// A request that may move: where it is, and the net its pin is on.
struct Slot<'a> {
    space: &'a PartSpace,
    key: &'a str,
    offers: &'a [String],
    refdes: &'a str,
    /// The pads of each pin of the part that the request may have.
    pads: BTreeMap<&'a str, &'a [String]>,
    /// The index of its net in the board's nets.
    net: usize,
    movable: bool,
}

/// Search for a pin map of `spaces` (the parts of one board that have requests, from the check)
/// that scores lower than the one the board has, trying changes on a copy of `placed`, the
/// board as it was placed and before any copper. `None` when no change lowers the score.
/// The `budget` ends the search early with what it has: it is asked before the search begins and
/// before each candidate is scored.
pub fn improve(
    spaces: &[PartSpace],
    design: &Design,
    placed: &Board,
    budget: &Budget,
) -> Option<Proposal> {
    improve_counting(spaces, design, placed, budget, &mut 0)
}

/// [`improve`], and how many candidates were scored, for the tests that say the budget holds.
pub fn improve_counting(
    spaces: &[PartSpace],
    design: &Design,
    placed: &Board,
    budget: &Budget,
    scored: &mut usize,
) -> Option<Proposal> {
    let mut slots: Vec<Slot> = Vec::new();
    for space in spaces {
        let Some(part) = design.parts.iter().find(|p| p.path == space.path) else {
            continue;
        };
        for r in &space.requests {
            let pads: BTreeMap<&str, &[String]> = std::iter::once(&r.pin)
                .chain(r.offers.iter())
                .filter_map(|pin| {
                    let found = part.pins.iter().find(|p| p.name == *pin)?;
                    Some((pin.as_str(), found.pads.as_slice()))
                })
                .collect();
            // The net the request's pin is on: the one that has a pad of that pin.
            let net = pads.get(r.pin.as_str()).and_then(|own| {
                placed.nets.iter().position(|n| {
                    n.pads
                        .iter()
                        .any(|p| p.refdes == part.refdes && own.contains(&p.number))
                })
            });
            slots.push(Slot {
                space,
                key: &r.key,
                offers: &r.offers,
                refdes: &part.refdes,
                pads,
                net: net.unwrap_or(0),
                movable: net.is_some() && !r.lock,
            });
        }
    }
    let mut current: Vec<String> = spaces
        .iter()
        .flat_map(|s| s.requests.iter().map(|r| r.pin.clone()))
        .collect();
    // `slots` skipped a part the design does not have; keep the two in step.
    if current.len() != slots.len() {
        current = slots
            .iter()
            .map(|s| {
                s.space
                    .requests
                    .iter()
                    .find(|r| r.key == s.key)
                    .expect("a request of its part")
                    .pin
                    .clone()
            })
            .collect();
    }
    let original = current.clone();

    let mut scratch = placed.clone();
    let before = Score::of(&scratch);
    let mut now = before;
    let mut moves: Vec<String> = Vec::new();

    for _ in 0..MAX_CHANGES {
        let mut best: Option<(Score, Vec<(usize, String)>)> = None;
        let mut out_of_time = false;
        'candidates: for r in 0..slots.len() {
            if !slots[r].movable {
                continue;
            }
            for to in slots[r].offers {
                if *to == current[r] || !slots[r].pads.contains_key(to.as_str()) {
                    continue;
                }
                let holder = (0..slots.len())
                    .find(|&h| h != r && same_part(&slots[h], &slots[r]) && current[h] == *to);
                let change: Vec<(usize, String)> = match holder {
                    None => vec![(r, to.clone())],
                    Some(h) => {
                        // A swap, tried once, from the request that comes first.
                        if h < r
                            || !slots[h].movable
                            || !slots[h].offers.contains(&current[r])
                            || !slots[h].pads.contains_key(current[r].as_str())
                        {
                            continue;
                        }
                        vec![(r, to.clone()), (h, current[r].clone())]
                    }
                };
                if !allowed(&slots, &current, &change) {
                    continue;
                }
                // The deadline is asked before every score, not once for the round: a score is
                // quadratic in the ratsnest lines. What the round found so far is still kept.
                if budget.spent() {
                    out_of_time = true;
                    break 'candidates;
                }
                *scored += 1;
                let score = try_change(&mut scratch, &slots, &current, &change);
                if score < best.as_ref().map_or(now, |b| b.0) {
                    best = Some((score, change));
                }
            }
        }
        let Some((score, change)) = best else { break };
        moves.push(describe(&slots, &current, &change));
        for (i, pin) in change {
            set_net(&mut scratch, &slots[i], &current[i], &pin);
            current[i] = pin;
        }
        now = score;
        if out_of_time {
            break;
        }
    }
    if moves.is_empty() {
        return None;
    }
    let picks = slots
        .iter()
        .enumerate()
        .filter(|(i, _)| current[*i] != original[*i])
        .map(|(i, s)| (s.key.to_string(), current[i].clone()))
        .collect();
    Some(Proposal {
        picks,
        moves,
        before,
        after: now,
    })
}

fn same_part(a: &Slot, b: &Slot) -> bool {
    std::ptr::eq(a.space, b.space)
}

/// Whether the pins of the part, with `change` made, are a choice the check allows.
fn allowed(slots: &[Slot], current: &[String], change: &[(usize, String)]) -> bool {
    let first = &slots[change[0].0];
    let picks: Vec<(&str, &str)> = slots
        .iter()
        .enumerate()
        .filter(|(_, s)| same_part(s, first))
        .map(|(i, s)| {
            let pin = change
                .iter()
                .find(|(c, _)| *c == i)
                .map_or(current[i].as_str(), |(_, p)| p.as_str());
            (s.key, pin)
        })
        .collect();
    first.space.allows(&picks)
}

/// The score of the board with `change` made. The board is as it was left.
fn try_change(
    scratch: &mut Board,
    slots: &[Slot],
    current: &[String],
    change: &[(usize, String)],
) -> Score {
    // The pads of every request that moves leave its net first, so that a swap of two pins
    // does not take a pad away from the net it has just joined.
    let saved: Vec<(usize, Vec<PadRef>)> = change
        .iter()
        .map(|(i, _)| (slots[*i].net, scratch.nets[slots[*i].net].pads.clone()))
        .collect();
    for (i, _) in change {
        remove_pads(&mut scratch.nets[slots[*i].net], &slots[*i], &current[*i]);
    }
    for (i, pin) in change {
        add_pads(&mut scratch.nets[slots[*i].net], &slots[*i], pin);
    }
    let score = Score::of(scratch);
    for (net, pads) in saved.into_iter().rev() {
        scratch.nets[net].pads = pads;
    }
    score
}

/// Make the change of one request on the board for good.
fn set_net(board: &mut Board, slot: &Slot, from: &str, to: &str) {
    remove_pads(&mut board.nets[slot.net], slot, from);
    add_pads(&mut board.nets[slot.net], slot, to);
}

fn remove_pads(net: &mut BoardNet, slot: &Slot, pin: &str) {
    let pads = slot.pads[pin];
    net.pads
        .retain(|p| !(p.refdes == slot.refdes && pads.contains(&p.number)));
}

fn add_pads(net: &mut BoardNet, slot: &Slot, pin: &str) {
    for number in slot.pads[pin] {
        net.pads.push(PadRef {
            refdes: slot.refdes.to_string(),
            number: number.clone(),
        });
    }
}

fn describe(slots: &[Slot], current: &[String], change: &[(usize, String)]) -> String {
    match change {
        [(i, to)] => format!("`{}` moves from `{}` to `{to}`", slots[*i].key, current[*i]),
        [(i, to), (h, _)] => format!(
            "`{}` and `{}` swap: `{}` is on `{to}` and `{}` is on `{}`",
            slots[*i].key, slots[*h].key, slots[*i].key, slots[*h].key, current[*i]
        ),
        _ => String::new(),
    }
}
