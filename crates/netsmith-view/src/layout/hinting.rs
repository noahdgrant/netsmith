//! The layout file's hints, as the engine applies them (ADR 0061, stage 2).
//!
//! The file is resolved against the design before the drawing starts ([`crate::hints`]: an
//! entry for each part and note of the design, by index). This module is what the engine
//! reads: the entries in the canonical order of the design ([`Input`]), then, once the
//! working design is known (a part with several units is several boxes), a [`Plan`] in boxes.
//! Three things are applied, each where the engine can do it without drawing anything twice:
//!
//! * **A turn or a mirror** (`rotate`, `mirror`) replaces the automatic orientation of the
//!   box before anything is placed ([`Plan::orient`]); the choices the engine makes by itself
//!   for a pair satellite, a series part or a part of a spine skip a part that has one.
//! * **A move** (`dx`, `dy`, `pin`) is applied once every block is in its place and before
//!   any wire is routed ([`resolve`]): the block (the main part with the satellites that serve
//!   it, a unit of parts that stand alone, or one satellite) is carried as a whole, so the
//!   wires are routed round the result and never moved after. The slot the block had in its
//!   row stays: the hinted block is a fixed piece of the rows' arrangement, and nothing else
//!   re-flows for it.
//! * **A move of a note** is applied last, after the notes are placed (`notes.rs`).
//!
//! The file can only move things, never rewire, and an entry that cannot be honoured is a
//! finding ([`HintFinding::unusable`]) and is ignored, so that a hint can make a drawing ugly
//! and cannot make it wrong.
//!
//! **Where each hint is applied.** `rotate` and `mirror` each replace their own half of the
//! automatic orientation ("automatic" is the engine's first choice, `orientations`): `rotate`
//! alone keeps the automatic mirror. The engine's own searches skip a part that has one: a pair
//! satellite is tried in that orientation only, a series satellite and a part of a spine are not
//! turned again. A part that is turned **leaves its stack**: the parts of a stack stand one above
//! the other by their pads, which a chosen turn would no longer be (a turned LED and its resistor
//! are two pieces joined by a wire). `dx`, `dy` and `pin` carry the **block** after every block
//! has its place in the rows and **before any wire is routed**; for a satellite or a part that
//! stands alone the block is the unit of parts it stands in (a stack moves as one). A satellite's
//! own entry is applied after its block's, as an offset from where its block puts it. The block
//! keeps its **slot in the rows**: the rest of the sheet is laid out as if the block were where
//! the engine put it, so a hinted block is a fixed piece of the arrangement and nothing else
//! re-flows for it (what does shift is what the move changes in the drawing's own decisions, a
//! net whose wire grew long being named, which takes room). `pin = [x, y]` puts the block's origin
//! at `x, y` from the sheet's **reference part** where the engine put it, whatever the block's
//! own place is; the frame starts at that place, not at where a moved reference part ends. A board
//! drawn as one picture (16 parts or fewer) still has the reference part of the entry's own
//! sheet, and a sheet cut into numbered boxes has one for each box. A note's `dx`, `dy` move its
//! frame after the notes are placed, to a place free of everything else; the notes without an
//! entry are placed first, as if there were no file.
//!
//! **What stays.** The engine decides some things by trying a sheet: which nets are drawn as names
//! and which of its few `Tuning` choices is kept (the order in which the satellites of a pin are
//! placed, for one). A drawing with the file is made with the choices of the drawing without it,
//! so that a moved part does not change them for the rest of the sheet: the first drawing is the
//! one there is without the file, the second is that drawing with the file's turns, moves and
//! notes applied, and a net is named besides only when it touches a part the file moved or
//! turned and its wire grew long. A file none of whose entries is honoured gives the first
//! drawing, byte for byte.
//!
//! **A safety net, not the normal path.** `check`'s rules and the symbol overlaps must hold for
//! any file. When a drawing with the file breaks a rule that the drawing without it does not break
//! (for each kind of rule, more of it) or has two parts lie on each other, it is drawn again
//! with the entries in the order of [`keys`] (the parts by path, then the notes), keeping each one
//! that breaks none; the others are findings (`the drawing with it breaks a rule
//! (wire-through-body); it is ignored`). It costs one drawing for each entry and only a drawing
//! that breaks a rule pays it. A move that is refused leaves the entry's turn in force.
//!
//! **A part with several units** (ADR 0050): the entry moves the **primary** unit's box (the one
//! with most pins) and the other units follow, each with its block, by the same steps. `rotate`
//! and `mirror` turn the primary box alone: the other units stay as the library draws them,
//! upright.
//!
//! **The findings the engine raises**, all [`HintFinding::unusable`], end `it is ignored`: its
//! block (or the part, for a satellite) would lie on the part `<path>` at the place the file
//! gives it, or once the other entries are applied; its block would stand more than [`REACH`]
//! (127 mm) outside the drawing; the sheet has no reference part to pin it to; the note would lie
//! on a part, a wire, a text or another note, or stand outside the drawing; the drawing with it
//! breaks a rule.
//!
//! **Measured** (`crates/netsmith-view/tests/hinted.rs`): moving the block of one main part by
//! four steps, on the three boards of several sheets, moves that block (five parts on average) and
//! nothing else of its sheet in the sheet's own frame (65 of 65 other parts stay, the wires of the
//! block are routed again). Seeded random files (offsets, pins, turns, mirrors, note offsets,
//! stale entries) on every board the tests draw, with and without rails: 104 drawings, 646
//! entries, 0 rules broken, each drawing the same twice and with the design listed the other way
//! round. Known limits: a turn that is not the engine's own takes a part out of its stack, so the
//! sheet may re-flow more than for a move; and the page puts each sheet's box at its place in the
//! arrangement, so a block that was the leftmost thing of its sheet shifts the box's edge and the
//! whole sheet moves by the same steps on the page.

use std::collections::{BTreeMap, BTreeSet};

use super::canon::Canon;
use crate::geom::{Orient, Pt, Rect, snap};
use crate::hints::{FindingKind, HintFinding, PartHint, Place, ResolvedHints, Steps};

/// The grid, in millimetres.
const GRID: f64 = 1.27;

/// The furthest a hinted block may stand from the drawing the engine made, beyond its edge: a
/// move that puts a block further is a typo and not a drawing, and a drawing that wide is
/// slow to route.
pub const REACH: f64 = 127.0;

/// The room kept between the box of a hinted block and the box of any other part: a grid step,
/// so that the wires between them have a place to go.
pub const GAP: f64 = 1.27;

/// What the file says about one part, with the part as the design's canonical order has it.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub hint: PartHint,
    /// The entry as the file names it, and its line, for a finding.
    pub entry: String,
    pub line: u32,
    /// The part's path in the design: what moves are ordered by.
    pub path: String,
}

/// What the file says about one note.
#[derive(Debug, Clone)]
pub(crate) struct NoteEntry {
    pub by: Steps,
    pub entry: String,
    pub line: u32,
}

/// Entries that must not be applied: the drawing broke a rule with them, found by
/// [`super::layout_with_hints`], which draws again without them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Dropped {
    /// Indices of the design's parts.
    pub parts: BTreeSet<usize>,
    /// Indices of the design's notes.
    pub notes: BTreeSet<usize>,
}

/// One entry of the file, by index in the design's own order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Key {
    Part(usize),
    Note(usize),
}

/// Every entry, in the order [`super::layout_with_hints`] tries them when a drawing breaks a
/// rule: the parts by path, then the notes by index.
pub(crate) fn keys(hints: &ResolvedHints) -> Vec<Key> {
    let mut parts: Vec<(&str, usize)> = hints
        .parts
        .iter()
        .map(|(&i, r)| (r.path.as_str(), i))
        .collect();
    parts.sort();
    let mut out: Vec<Key> = parts.into_iter().map(|(_, i)| Key::Part(i)).collect();
    out.extend(hints.notes.keys().map(|&i| Key::Note(i)));
    out
}

/// What to leave out so that only the entries `kept` are applied.
pub(crate) fn dropped_except(hints: &ResolvedHints, kept: &BTreeSet<Key>) -> Dropped {
    Dropped {
        parts: hints
            .parts
            .keys()
            .copied()
            .filter(|&i| !kept.contains(&Key::Part(i)))
            .collect(),
        notes: hints
            .notes
            .keys()
            .copied()
            .filter(|&i| !kept.contains(&Key::Note(i)))
            .collect(),
    }
}

/// An entry as the file names it, and its line.
pub(crate) fn describe(hints: &ResolvedHints, key: Key) -> (&str, u32) {
    match key {
        Key::Part(i) => {
            let r = &hints.parts[&i];
            (&r.entry, r.line)
        }
        Key::Note(i) => {
            let r = &hints.notes[&i];
            (&r.entry, r.line)
        }
    }
}

/// The hints of one drawing in the canonical order of its design.
#[derive(Debug, Clone, Default)]
pub(crate) struct Input {
    pub parts: BTreeMap<usize, Entry>,
    pub notes: BTreeMap<usize, NoteEntry>,
    /// The sheet of each (canonical) part when one drawing holds several sheets (a small board
    /// drawn as one picture): a pin is from the reference part of its own sheet. Empty: one
    /// sheet.
    pub sheet_of: Vec<usize>,
}

impl Input {
    /// The hints for `canon`'s design: `hints` are by the original design's indices, `sheet_of`
    /// is the sheet of each of the original parts, if the drawing holds several; `dropped`
    /// entries are left out.
    pub fn new(
        canon: &Canon,
        hints: &ResolvedHints,
        sheet_of: Option<&[usize]>,
        dropped: &Dropped,
    ) -> Input {
        let to_canon = canon.canonical_parts();
        let n = to_canon.len();
        let mut input = Input::default();
        for (&p, r) in &hints.parts {
            if p >= n || dropped.parts.contains(&p) {
                continue;
            }
            input.parts.insert(
                to_canon[p],
                Entry {
                    hint: r.hint,
                    entry: r.entry.clone(),
                    line: r.line,
                    path: r.path.clone(),
                },
            );
        }
        for (&i, r) in &hints.notes {
            if dropped.notes.contains(&i) {
                continue;
            }
            input.notes.insert(
                i,
                NoteEntry {
                    by: r.hint.by,
                    entry: r.entry.clone(),
                    line: r.line,
                },
            );
        }
        if let Some(sheets) = sheet_of {
            let mut by = vec![0; n];
            for (o, &c) in to_canon.iter().enumerate() {
                by[c] = sheets.get(o).copied().unwrap_or(0);
            }
            input.sheet_of = by;
        }
        input
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty() && self.notes.is_empty()
    }
}

/// A move of a block, in boxes.
#[derive(Debug, Clone)]
pub(crate) struct Move {
    /// The box the entry is about: the primary unit of its part.
    pub part: usize,
    /// The part's other units: they follow, moved by the same steps.
    pub follow: Vec<usize>,
    pub place: Place,
    pub entry: String,
    pub line: u32,
    pub path: String,
}

/// What the engine does with the hints, by box of the working design.
#[derive(Debug, Clone, Default)]
pub(crate) struct Plan {
    /// A mirror and a number of quarter turns, each when the entry says it, for the primary
    /// unit of a part.
    turns: BTreeMap<usize, (Option<bool>, Option<u8>)>,
    pub moves: Vec<Move>,
    pub notes: BTreeMap<usize, NoteEntry>,
    /// The sheet of each box, and the box that is the reference part of each sheet.
    group: Vec<usize>,
    reference: BTreeMap<usize, usize>,
}

impl Plan {
    /// The plan for a working design of `boxes` boxes: the first `parts` are the design's parts
    /// (in canonical order) and the box `k` after them draws the part `origin[k - parts]` (a
    /// further unit). `reference` picks the reference box of a sheet among boxes.
    pub fn build(
        input: &Input,
        parts: usize,
        origin: &[usize],
        reference: impl Fn(&[usize]) -> Option<usize>,
    ) -> Plan {
        if input.is_empty() {
            return Plan::default();
        }
        // `origin` has an entry for every box (the part it draws) when the design is split, and
        // none when every part is one box.
        let part_of = |k: usize| -> usize { origin.get(k).copied().unwrap_or(k) };
        let total = if origin.is_empty() {
            parts
        } else {
            origin.len()
        };
        let mut plan = Plan {
            notes: input.notes.clone(),
            ..Plan::default()
        };
        plan.group = (0..total)
            .map(|k| input.sheet_of.get(part_of(k)).copied().unwrap_or(0))
            .collect();
        let mut sheets: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (k, &g) in plan.group.iter().enumerate() {
            sheets.entry(g).or_default().push(k);
        }
        for (g, members) in sheets {
            if let Some(r) = reference(&members) {
                plan.reference.insert(g, r);
            }
        }
        for (&c, e) in &input.parts {
            if c >= parts {
                continue;
            }
            if e.hint.rotate.is_some() || e.hint.mirror.is_some() {
                plan.turns.insert(
                    c,
                    (e.hint.mirror, e.hint.rotate.map(|d| (d / 90 % 4) as u8)),
                );
            }
            if let Some(place) = e.hint.place {
                let follow: Vec<usize> = (parts..total).filter(|&k| part_of(k) == c).collect();
                plan.moves.push(Move {
                    part: c,
                    follow,
                    place,
                    entry: e.entry.clone(),
                    line: e.line,
                    path: e.path.clone(),
                });
            }
        }
        plan
    }

    /// Whether a box has a turn or a mirror from the file.
    pub fn turned(&self, part: usize) -> bool {
        self.turns.contains_key(&part)
    }

    /// The orientation of box `part` given the one the engine chose by itself: the file's
    /// mirror and turn replace the automatic ones, each only when the entry says it.
    pub fn orient(&self, part: usize, automatic: Orient) -> Orient {
        match self.turns.get(&part) {
            None => automatic,
            Some(&(mirror, turns)) => {
                let (auto_mirror, auto_turns) = automatic.mirror_and_turns();
                Orient::turned(mirror.unwrap_or(auto_mirror), turns.unwrap_or(auto_turns))
            }
        }
    }

    /// The reference box of the sheet box `part` is on.
    fn reference_of(&self, part: usize) -> Option<usize> {
        let g = self.group.get(part).copied().unwrap_or(0);
        self.reference.get(&g).copied()
    }
}

/// What [`resolve`] reads of the drawing before the wires are routed.
pub(crate) struct Frame<'a> {
    /// Where the engine put each box (its origin), and the box it covers at the origin: the
    /// symbol, its pins and its texts.
    pub at: &'a [Pt],
    pub rect: &'a [Rect],
    /// What a finding calls each box.
    pub names: &'a [String],
    /// The boxes that move with box `part`, and whether `part` is the main part of a block (its
    /// satellites move with it) or one unit of parts (a satellite, or parts that stand alone):
    /// the block a move carries.
    pub carried: &'a dyn Fn(usize) -> (Vec<usize>, bool),
}

fn inflate(r: Rect, by: f64) -> Rect {
    Rect {
        min: Pt::new(r.min.x - by, r.min.y - by),
        max: Pt::new(r.max.x + by, r.max.y + by),
    }
}

fn overlap(a: &Rect, b: &Rect) -> bool {
    a.min.x < b.max.x - 1e-6
        && b.min.x < a.max.x - 1e-6
        && a.min.y < b.max.y - 1e-6
        && b.min.y < a.max.y - 1e-6
}

struct Item<'a> {
    mv: &'a Move,
    /// Every box the move carries.
    members: Vec<usize>,
    /// 0 for a block with a main part, 1 for a unit that stands in one.
    level: u8,
}

/// Where the moves of the file put the boxes, from where the engine put them: how far each box
/// moves, and a finding for each move that cannot be honoured.
///
/// The moves are taken in a fixed order, the pinned ones first, then the offsets, a main
/// part's block before a satellite's, each by the path of its part. A move is **not honoured**
/// when the box of one of the boxes it carries, with [`GAP`] round it, would lie on the box of
/// any other box (one that the move does not carry, and that a later move does not carry
/// either, for that one is not placed yet), or would be further than [`REACH`] from the
/// drawing: its block stays where the engine put it. So of two hinted blocks that lie on each
/// other the earlier wins, and a block can move onto the place another block is moved from.
/// The boxes that lay on each other where the engine put them are not counted.
///
/// A finding is `unusable` and names the entry and what its block would lie on.
pub(crate) fn resolve(plan: &Plan, frame: &Frame<'_>) -> (Vec<Pt>, Vec<HintFinding>) {
    let n = frame.at.len();
    let mut items: Vec<Item> = plan
        .moves
        .iter()
        .map(|mv| {
            let (mut members, block) = (frame.carried)(mv.part);
            for &b in &mv.follow {
                members.extend((frame.carried)(b).0);
            }
            members.sort_unstable();
            members.dedup();
            Item {
                mv,
                members,
                level: if block { 0 } else { 1 },
            }
        })
        .collect();
    let pinned = |it: &Item| matches!(it.mv.place, Place::Pinned(_));
    items.sort_by(|a, b| {
        (!pinned(a), a.level, &a.mv.path, a.mv.line).cmp(&(
            !pinned(b),
            b.level,
            &b.mv.path,
            b.mv.line,
        ))
    });

    // The drawing the engine made, and how far from it a block may stand.
    let mut auto: Option<Rect> = None;
    for b in 0..n {
        let r = frame.rect[b].translated(frame.at[b]);
        match &mut auto {
            Some(a) => a.union(r),
            None => auto = Some(r),
        }
    }
    let limit = auto.map(|a| inflate(a, REACH));
    let rect_at = |b: usize, shift: Pt| {
        frame.rect[b].translated(Pt::new(frame.at[b].x + shift.x, frame.at[b].y + shift.y))
    };
    let clash = |a: usize, sa: Pt, b: usize, sb: Pt| -> bool {
        overlap(
            &inflate(rect_at(a, sa), GAP / 2.0),
            &inflate(rect_at(b, sb), GAP / 2.0),
        )
    };
    let zero = Pt::new(0.0, 0.0);
    let add = |p: Pt, q: Pt| Pt::new(p.x + q.x, p.y + q.y);

    let mut forced: BTreeMap<usize, String> = BTreeMap::new();
    loop {
        let mut shifts = vec![zero; n];
        let mut accepted = vec![false; items.len()];
        let mut why: Vec<Option<String>> = vec![None; items.len()];
        for i in 0..items.len() {
            let it = &items[i];
            if let Some(reason) = forced.get(&i) {
                why[i] = Some(reason.clone());
                continue;
            }
            let what = if it.level == 0 {
                "its block"
            } else {
                "the part"
            };
            let delta = match it.mv.place {
                Place::Offset(s) => steps_mm(s),
                Place::Pinned(s) => {
                    let Some(r) = plan.reference_of(it.mv.part) else {
                        why[i] = Some("the sheet has no reference part to pin it to".to_string());
                        continue;
                    };
                    let origin = Pt::new(snap(frame.at[r].x, GRID), snap(frame.at[r].y, GRID));
                    let target = add(origin, steps_mm(s));
                    let now = add(frame.at[it.mv.part], shifts[it.mv.part]);
                    Pt::new(target.x - now.x, target.y - now.y)
                }
            };
            let mut next = shifts.clone();
            for &b in &it.members {
                next[b] = add(next[b], delta);
            }
            // What is not placed yet: the boxes a later move carries.
            let pending: BTreeSet<usize> = items[i + 1..]
                .iter()
                .flat_map(|x| x.members.iter().copied())
                .filter(|b| !it.members.contains(b))
                .collect();
            let mut failure: Option<String> = None;
            'check: for &b in &it.members {
                if let Some(limit) = limit {
                    let r = rect_at(b, next[b]);
                    if r.min.x < limit.min.x
                        || r.min.y < limit.min.y
                        || r.max.x > limit.max.x
                        || r.max.y > limit.max.y
                    {
                        failure = Some(format!(
                            "{what} would stand more than {REACH:.0} mm outside the drawing"
                        ));
                        break 'check;
                    }
                }
                for o in 0..n {
                    if it.members.contains(&o) || pending.contains(&o) {
                        continue;
                    }
                    if clash(b, next[b], o, next[o]) && !clash(b, zero, o, zero) {
                        failure = Some(format!(
                            "{what} would lie on the part `{}` at the place the file gives it",
                            frame.names[o]
                        ));
                        break 'check;
                    }
                }
            }
            match failure {
                None => {
                    shifts = next;
                    accepted[i] = true;
                }
                Some(f) => why[i] = Some(f),
            }
        }
        // The final drawing is checked as a whole: a move that was fine when it was taken can
        // lie on a box that a later move left in place (a block that was refused stays where
        // the engine put it). The latest move of the pair is refused and the moves are taken
        // again.
        let mut culprit: Option<(usize, String)> = None;
        'scan: for a in 0..n {
            for b in a + 1..n {
                if shifts[a] == shifts[b]
                    || !clash(a, shifts[a], b, shifts[b])
                    || clash(a, zero, b, zero)
                {
                    continue;
                }
                let latest = (0..items.len())
                    .filter(|&i| {
                        accepted[i]
                            && (items[i].members.contains(&a) || items[i].members.contains(&b))
                    })
                    .max();
                if let Some(i) = latest {
                    let y = if items[i].members.contains(&a) { b } else { a };
                    let what = if items[i].level == 0 {
                        "its block"
                    } else {
                        "the part"
                    };
                    culprit = Some((
                        i,
                        format!(
                            "{what} would lie on the part `{}` once the other entries are applied",
                            frame.names[y]
                        ),
                    ));
                    break 'scan;
                }
            }
        }
        match culprit {
            Some((i, reason)) => {
                forced.insert(i, reason);
            }
            None => {
                let findings = items
                    .iter()
                    .zip(&why)
                    .filter_map(|(it, w)| {
                        w.as_ref()
                            .map(|w| HintFinding::unusable(it.mv.entry.clone(), it.mv.line, w))
                    })
                    .collect();
                return (shifts, findings);
            }
        }
    }
}

fn steps_mm(s: Steps) -> Pt {
    let (x, y) = s.mm();
    Pt::new(x, y)
}

/// The finding for a note whose offset cannot be honoured.
pub(crate) fn note_finding(entry: &NoteEntry, why: &str) -> HintFinding {
    HintFinding::unusable(entry.entry.clone(), entry.line, why)
}

/// The finding for an entry that the drawing broke a rule with (found by checking the
/// drawing): it is left out and the sheet is drawn again.
pub(crate) fn rule_finding(entry: &str, line: u32, rule: &str) -> HintFinding {
    HintFinding {
        kind: FindingKind::Unusable,
        line,
        message: format!("{entry}: the drawing with it breaks a rule ({rule}); it is ignored"),
        entry: entry.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect {
            min: Pt::new(x0, y0),
            max: Pt::new(x1, y1),
        }
    }

    fn mv(part: usize, place: Place, path: &str) -> Move {
        Move {
            part,
            follow: Vec::new(),
            place,
            entry: format!("[{path}]"),
            line: 1,
            path: path.to_string(),
        }
    }

    /// Four boxes 10 mm square in a row, 30 mm apart.
    fn row() -> (Vec<Pt>, Vec<Rect>, Vec<String>) {
        (
            (0..4).map(|i| Pt::new(30.0 * i as f64, 0.0)).collect(),
            (0..4).map(|_| rect(-5.0, -5.0, 5.0, 5.0)).collect(),
            ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect(),
        )
    }

    fn near(p: Pt, x: f64, y: f64) -> bool {
        (p.x - x).abs() < 1e-6 && (p.y - y).abs() < 1e-6
    }

    fn run(plan: &Plan) -> (Vec<Pt>, Vec<HintFinding>) {
        let (at, rect, names) = row();
        let carried = |p: usize| (vec![p], true);
        resolve(
            plan,
            &Frame {
                at: &at,
                rect: &rect,
                names: &names,
                carried: &carried,
            },
        )
    }

    fn plan(moves: Vec<Move>) -> Plan {
        Plan {
            group: vec![0; 4],
            reference: BTreeMap::from([(0, 0)]),
            moves,
            ..Plan::default()
        }
    }

    #[test]
    fn an_offset_moves_the_box_by_the_steps_and_nothing_else() {
        let (shifts, found) = run(&plan(vec![mv(1, Place::Offset(Steps::new(2, -3)), "b")]));
        assert!(found.is_empty(), "{found:?}");
        assert!(near(shifts[1], 2.54, -3.81), "{:?}", shifts[1]);
        assert!(
            shifts
                .iter()
                .enumerate()
                .all(|(i, s)| i == 1 || near(*s, 0.0, 0.0))
        );
    }

    #[test]
    fn a_box_moved_onto_another_is_not_moved_and_the_finding_names_it() {
        // `b` moved 30 mm left lies on `a`.
        let (shifts, found) = run(&plan(vec![mv(1, Place::Offset(Steps::new(-24, 0)), "b")]));
        assert_eq!(shifts[1], Pt::new(0.0, 0.0));
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("`a`"), "{}", found[0].message);
        assert!(found[0].message.contains("[b]"), "{}", found[0].message);
        assert_eq!(found[0].kind, FindingKind::Unusable);
    }

    #[test]
    fn of_two_boxes_that_lie_on_each_other_the_earlier_path_wins() {
        // `b` and `c` both move to 45 mm: the place between them.
        let moves = vec![
            mv(2, Place::Offset(Steps::new(-12, 0)), "c"),
            mv(1, Place::Offset(Steps::new(12, 0)), "b"),
        ];
        let (shifts, found) = run(&plan(moves));
        assert_ne!(shifts[1], Pt::new(0.0, 0.0), "the earlier path is honoured");
        assert_eq!(shifts[2], Pt::new(0.0, 0.0));
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("[c]"), "{}", found[0].message);
        assert!(found[0].message.contains("`b`"), "{}", found[0].message);
    }

    #[test]
    fn a_pinned_move_comes_before_the_offsets() {
        // `c` is pinned to the place `b` is moved to: the pin wins although `b` is first by path.
        let moves = vec![
            mv(1, Place::Offset(Steps::new(12, 0)), "b"),
            mv(2, Place::Pinned(Steps::new(36, 0)), "c"),
        ];
        let (shifts, found) = run(&plan(moves));
        assert!(
            near(shifts[2], 45.72 - 60.0, 0.0),
            "pinned at 36 steps from `a`: {:?}",
            shifts[2]
        );
        assert_eq!(shifts[1], Pt::new(0.0, 0.0));
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("[b]"), "{}", found[0].message);
    }

    #[test]
    fn a_box_can_move_onto_the_place_another_is_moved_from() {
        // Swap `b` and `c`: both honoured.
        let moves = vec![
            mv(1, Place::Offset(Steps::new(24, 0)), "b"),
            mv(2, Place::Offset(Steps::new(-24, 0)), "c"),
        ];
        let (shifts, found) = run(&plan(moves));
        assert!(found.is_empty(), "{found:?}");
        assert!(near(shifts[1], 30.48, 0.0) && near(shifts[2], -30.48, 0.0));
    }

    #[test]
    fn a_block_beyond_the_reach_is_not_moved() {
        let (shifts, found) = run(&plan(vec![mv(3, Place::Offset(Steps::new(400, 0)), "d")]));
        assert_eq!(shifts[3], Pt::new(0.0, 0.0));
        assert!(
            found[0].message.contains("outside the drawing"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn a_unit_with_several_boxes_moves_them_by_the_same_steps() {
        let (at, rects, names) = row();
        let carried = |p: usize| (vec![p], true);
        let mut m = mv(0, Place::Offset(Steps::new(0, 8)), "a");
        m.follow = vec![3];
        let plan = plan(vec![m]);
        let (shifts, found) = resolve(
            &plan,
            &Frame {
                at: &at,
                rect: &rects,
                names: &names,
                carried: &carried,
            },
        );
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(shifts[0], shifts[3]);
        assert!(near(shifts[0], 0.0, 10.16));
    }

    #[test]
    fn a_turn_replaces_only_what_the_entry_says() {
        let mut p = Plan::default();
        p.turns.insert(0, (None, Some(1)));
        p.turns.insert(1, (Some(true), None));
        // The automatic orientation is mirrored and turned once: a new turn keeps the mirror.
        let auto = Orient::turned(true, 3);
        assert_eq!(p.orient(0, auto), Orient::turned(true, 1));
        // A new mirror keeps the automatic turn.
        assert_eq!(
            p.orient(1, Orient::turned(false, 3)),
            Orient::turned(true, 3)
        );
        assert_eq!(p.orient(2, auto), auto);
        assert!(p.turned(0) && !p.turned(2));
    }
}
