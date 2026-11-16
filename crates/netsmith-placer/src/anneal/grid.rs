//! Which parts can cost something together: a grid of square cells over the board, each
//! listing the parts whose reach ([`Shape::reach`]) touches it (issue #766).
//!
//! Two parts whose reaches do not overlap cost nothing together by their own boxes
//! ([`Engine::pair_of_parts`]), so the cost of a part is the sum over the parts its reach
//! meets, and the parts that carry a keepout or an escape ring, which reach past their
//! reach. Every other pair adds exactly nothing, so a sum over the parts the grid gives, in
//! the same order, is the sum over every part: the placement is the same with or without the
//! grid. Without it each pose a part was tried at in [`Engine::legalise_by`] cost a pass over
//! every part, and a crowded board of 420 parts took minutes.

use std::collections::HashMap;

use super::*;

/// The side of a cell, in mm: about the reach of a small part, so that one is in a few cells.
const CELL: f64 = 4.0;

/// A part whose reach covers more cells than this is listed apart and met by every part,
/// so that a board of a few very large parts does not fill the map with cells.
const MOST_CELLS: i64 = 1024;

/// The cells a box touches: the first and last column and row.
type Span = (i64, i64, i64, i64);

fn span(r: &Rect) -> Span {
    let cell = |v: f64| (v / CELL).floor() as i64;
    (cell(r.min.x), cell(r.min.y), cell(r.max.x), cell(r.max.y))
}

fn cells(s: Span) -> i64 {
    (s.2 - s.0 + 1).saturating_mul(s.3 - s.1 + 1)
}

pub(super) struct Grid {
    cells: HashMap<(i64, i64), Vec<usize>>,
    /// Where each part is listed: its span, `None` if nowhere yet or among the wide ones.
    at: Vec<Option<Span>>,
    /// The parts too wide for the cells, and those that carry a keepout or an escape ring:
    /// every part is measured against them. In ascending order.
    always: Vec<usize>,
    /// Which parts are in `always` because of what they carry (not their size).
    carrier: Vec<bool>,
}

impl Grid {
    /// An empty grid for parts, `carrier` saying which carry something that reaches past
    /// their reach.
    pub(super) fn new(carrier: Vec<bool>) -> Grid {
        let always = (0..carrier.len()).filter(|&i| carrier[i]).collect();
        Grid {
            cells: HashMap::new(),
            at: vec![None; carrier.len()],
            always,
            carrier,
        }
    }

    /// Lists part `i` where its reach `r` is now.
    pub(super) fn set(&mut self, i: usize, r: &Rect) {
        if self.carrier[i] {
            return;
        }
        let new = span(r);
        let wide = cells(new) > MOST_CELLS;
        let old = self.at[i];
        if old == Some(new) {
            return;
        }
        let was_wide = old.is_none() && self.always.binary_search(&i).is_ok();
        if let Some(s) = old {
            for x in s.0..=s.2 {
                for y in s.1..=s.3 {
                    if let Some(list) = self.cells.get_mut(&(x, y)) {
                        if let Some(k) = list.iter().position(|&j| j == i) {
                            list.swap_remove(k);
                        }
                        if list.is_empty() {
                            self.cells.remove(&(x, y));
                        }
                    }
                }
            }
        }
        if wide {
            self.at[i] = None;
            if !was_wide && let Err(k) = self.always.binary_search(&i) {
                self.always.insert(k, i);
            }
            return;
        }
        if was_wide && let Ok(k) = self.always.binary_search(&i) {
            self.always.remove(k);
        }
        for x in new.0..=new.2 {
            for y in new.1..=new.3 {
                self.cells.entry((x, y)).or_default().push(i);
            }
        }
        self.at[i] = Some(new);
    }

    /// The parts that may cost something with part `i`, whose reach is `r`, in ascending
    /// order: every part if `i` carries something or is wide, else the parts listed in the
    /// cells `r` touches and those met by every part. It may include `i`.
    pub(super) fn near(&self, i: usize, r: &Rect) -> Vec<usize> {
        let s = span(r);
        if self.carrier[i] || cells(s) > MOST_CELLS {
            return (0..self.at.len()).collect();
        }
        let mut found = self.always.clone();
        for x in s.0..=s.2 {
            for y in s.1..=s.3 {
                if let Some(list) = self.cells.get(&(x, y)) {
                    found.extend_from_slice(list);
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        found
    }
}

impl<'a> Engine<'a> {
    /// Sets part `i`'s reach on the board from where it is, and lists it there.
    pub(super) fn set_reach(&mut self, i: usize) {
        let r = self.shape(i).reach.translated(self.pos(i));
        self.reach_abs[i] = r;
        self.grid.set(i, &r);
    }

    /// The parts that may cost something with part `i` where it is now ([`Grid::near`]).
    pub(super) fn near(&self, i: usize) -> Vec<usize> {
        self.grid.near(i, &self.reach_abs[i])
    }
}
