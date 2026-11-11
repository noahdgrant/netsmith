//! Notes: the source's own writing (the doc comments of the board, a module, a part or an
//! interface, and the `==` relations of the design, [`netsmith_core::Note`]) drawn on the sheet
//! as framed text boxes, as an engineer writes a note beside the part it is about.
//!
//! A note is a rounded frame in a light note tone with wrapped text: a doc comment in the page's
//! sans serif, a relation in a monospace face with a small heading (`Relation`) and, when the
//! solver settled the field it gives a value to, the result under it. The text is wrapped at
//! word boundaries to about 80 mm (the box is as wide as its longest line, 25 mm at least), cut at
//! [`MAX_CHARS`] letters with an ellipsis (the whole text goes in the tooltip).
//!
//! A note is a box of its own and **joins nothing**: no wire, no leader line. It is placed
//! once the sheet is drawn and every wire routed, in free space:
//!
//! 1. A note of a part (a doc comment on a part, an interface or a module declaration with
//!    parts, or a relation of one part) stands beside the part, on the side with room, nearest
//!    first and a side no pin leaves by before one a pin leaves by (`beside`). A relation of
//!    several parts stands in free space near the middle of the parts it names (`around`).
//! 2. A note of the sheet (the board's or a module's doc comment, a relation that names no
//!    part) stands in a strip at the top left, above the drawing (under the sheet's title).
//! 3. A note that finds no free place near its parts goes to the margin: a column to the right
//!    of the drawing, or a strip under it, whichever leaves the sheet nearer the shape the
//!    sheets aim for. A sheet with more than [`MANY`] notes puts all of them in one column at
//!    the right margin.
//!
//! "Free" is measured against everything the check measures (`crate::sheetview`): the symbols'
//! boxes, the texts, the wires and the thick lines of buses, power symbols, flags, dots, X marks
//! and the notes already placed, with a clearance, so the drawing's wires are never moved for a
//! note and a note never lies on one. A sheet with no notes is left exactly as it was.
//!
//! **Why (ADR 0060).** An engineer's hand-drawn schematic carries its reasons: a framed box with
//! the calculation that gave a resistor its value, a line that says where a circuit was taken
//! from, a warning that two connectors must be 22 mm apart. netsmith's source has the same
//! writing (the `///` above a part, a module or an interface, and the `==` lines such as
//! `.led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance`) and the schematic
//! dropped it. This is a drawing convention and not a change of the language: the checker's model
//! already kept every doc comment, and each relation statement keeps its own text.
//!
//! **What becomes a note** (`netsmith-design`, `notes_of`, in the source's order, from
//! `Instance::docs`, `Instance::def_docs` and `Relation::written`): the doc comments of the board
//! and of each module that has a part, of each part declaration (a note about that part) and of
//! each interface declaration with parts on its nets (about those parts, or the sheet's when more
//! than three); and each `==` relation of a board or a module body, as written, about the parts
//! whose fields it names. A relation of rails only (`.cards.voltage == .supply.voltage * 0.9`) is
//! about the parts on the nets of the pins of each interface it names, as the doc comment of an
//! interface is: none when there are more than three, and with the return net left out (ground has
//! every part, so counting it would make every such relation the sheet's). **Not drawn:** an
//! ordinary `//` comment (the lexer drops it before the model); the docs of the standard library
//! and the relations inside it or inside any part's own definition (the part's model, not the
//! design); a relation of a physical file (it says where parts go on the board); the `//!` of a
//! file; the docs of an interface or a module that nothing drawn is in (a manufacturing process);
//! a relation `>=` or `<=`.
//!
//! **Where, in numbers.** A note of a part stands beside it, a side no pin leaves by before one a
//! pin leaves by, nearest first and sliding along the side. A note with no free place within
//! [`NEAR`] (40 mm) of its part, or [`AROUND`] (50 mm) of the middle of its parts, goes to a
//! margin, a column to its right (level with its part as far as the notes above allow) or a strip
//! under the drawing, whichever leaves the sheet nearer its aim of 1.6 wide for each high. A note
//! is a rounded frame, 25 to about 90 mm wide, text wrapped at word boundaries by the page's
//! letter widths. The tie-breaks are fixed (distance, then position), so a drawing is the same
//! every time.
//!
//! **What `check` and the metrics do.** A note is no conductor: it is left out of the netlist the
//! check rebuilds. Two rules are new: `note-overlap` (a note's frame lies on a symbol, text, wire,
//! thick line of a bus, power symbol, flag, dot, X or another note) and `note-text-overflow` (a
//! line of text, measured as it is drawn, escapes its frame). The metrics count `notes`,
//! `note_area` and `note_overlaps` (target 0); their columns are printed only for a board that
//! has a note.
//!
//! **In the SVG.** `<g class="note note-doc|note-relation" data-note="<path>" data-n="<index>"
//! data-ref="<reference of the first part on this sheet>" data-anchors="<reference> ...">` and
//! `data-at="<file>:<line>"`, a `note-frame` rect, a `note-heading` text and one selectable
//! `note-text` text with a tspan for each line; a default look in the embedded style, written
//! only into a drawing that has a note. `data-anchors` has the references of all the parts of the
//! design's note (`Note::anchors`), each once, also those on another sheet, empty for the note of a
//! sheet. The page lights a note with any of its parts and the parts of a note that has the focus
//! from this list. Nets are not marked: a note lighting the nets it is about would need a rule for
//! which net of a rail it is, and what a note of ground lights.
//!
//! **Rejected.** Reconstructing a relation from the solver's expression tree (constants and paths
//! lose the way the author wrote them); reading `//` comments for "TODO" or "NOTE:" lines (not
//! source information the language states); a leader line from a note to its part (the owner
//! wants none: nearness says it); notes as obstacles to the router before routing (it moves wires,
//! and every existing drawing would change; room is found in what is drawn, and the margin takes
//! the rest).
//!
//! **Cost.** A sheet with notes is bigger by the margin or the strip. A note beside a part may
//! stand where a later change of the drawing would want room.
//!
//! **Choices, accepted as built by the owner, 2026-11-11.** The status of ADR 0060 was proposed:
//! the owner decided on 2026-11-05 that `///` doc comments and `==` relations may be drawn as
//! notes, and the rules for what is drawn, where and how many were left to review. A relation of
//! a library module (a `BuckStage` feedback relation) is not drawn, and neither is a `>=` or `<=`
//! relation, which states a claim to verify and not what the circuit is. A note lights no net
//! (above). A module's doc comment does not say which module it is when a small board draws
//! modules as one sheet: a heading with the module's path on the note of a module that is not on a
//! sheet of its own needs the drawing to know whether it holds one module or several, which no
//! layout entry point passes down, so it is a change of the layout's interface; not planned
//! until a board needs it.

use std::collections::HashMap;

use netsmith_core::{Design, Note, NoteKind};

use super::hinting;
use super::{Anchor, BORDER, GRID, Layout, PlacedNote, Text};
use crate::geom::{Dir, Pt, Rect, snap, snap_up};
use crate::glyph::text_em;
use crate::hints::HintFinding;
use crate::sheetview::{bus_segments, overlaps, symbol_box, texts, views};

/// The height of a doc comment's letters, as net names are.
pub const NOTE_TEXT: f64 = 1.27;
/// The height of a relation's letters (monospace, 0.6 of it wide).
pub const MONO_TEXT: f64 = 1.2;
/// The height of the heading of a relation note.
pub const HEADING_TEXT: f64 = 0.95;
/// The distance between the baselines of two lines.
pub const LINE: f64 = 1.9;
/// Room between the frame and its text.
pub const PAD: f64 = 1.6;
/// How wide a monospace letter is drawn, as a fraction of its height.
pub const MONO_EM: f64 = 0.6;
/// The longest a line of text may be, in millimetres, when it is wrapped.
const MAX_INNER: f64 = 80.0;
/// The narrowest a note's frame is.
const MIN_WIDTH: f64 = 25.4;
/// A note's text is cut after this many letters (the tooltip has all of it).
pub const MAX_CHARS: usize = 600;
/// A sheet with more notes than this puts them all in one column at its right margin.
pub const MANY: usize = 8;
/// Wrapping measures a line this much wider than it is measured by, so a font a little wider
/// than the measure still fits.
const WRAP_SLACK: f64 = 1.06;
/// Room kept between a note and anything else.
const CLEAR: f64 = 1.27;
/// Room between the drawing and the margin column or strip, and between notes in them.
const MARGIN_GAP: f64 = 5.08;
const NOTE_GAP: f64 = 2.54;
/// How far from a part a note may stand, beside it, and from the middle of its parts.
const NEAR: f64 = 40.0;
const AROUND: f64 = 50.0;
/// The shape a sheet aims for, width over height (`sheets::TARGET_ASPECT`).
const TARGET_ASPECT: f64 = 1.6;
/// What a side costs, in millimetres of distance, when a pin leaves the part by it.
const PIN_SIDE: f64 = 30.0;
/// The top left rounding of the frame.
pub const RADIUS: f64 = 1.0;

/// How wide `text` is drawn in a note: a monospace letter is [`MONO_EM`] of the height, other
/// text is measured by the class of each letter ([`text_em`]).
pub fn line_width(text: &str, size: f64, mono: bool) -> f64 {
    if mono {
        text.chars().count() as f64 * size * MONO_EM
    } else {
        text_em(text) * size
    }
}

/// What a note says, wrapped, and how big its frame is.
pub(crate) struct Body {
    pub heading: Option<String>,
    pub lines: Vec<String>,
    pub mono: bool,
    pub width: f64,
    pub height: f64,
    /// The whole text, when the lines are cut.
    pub full: Option<String>,
}

impl Body {
    pub fn size(&self) -> f64 {
        if self.mono { MONO_TEXT } else { NOTE_TEXT }
    }

    fn heading_room(&self) -> f64 {
        if self.heading.is_some() { 2.3 } else { 0.0 }
    }
}

/// The text cut to [`MAX_CHARS`] letters, with an ellipsis, when it is longer.
pub fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(MAX_CHARS - 1).collect();
    format!("{}…", head.trim_end())
}

/// `text` in lines no wider than `limit` (a paragraph break is a line break), at word
/// boundaries; a word wider than a line is cut where it overflows.
fn wrap(text: &str, size: f64, mono: bool, limit: f64) -> Vec<String> {
    let wide = |s: &str| line_width(s, size, mono) * WRAP_SLACK;
    let mut lines: Vec<String> = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word.to_string();
            loop {
                let candidate = if line.is_empty() {
                    word.clone()
                } else {
                    format!("{line} {word}")
                };
                if wide(&candidate) <= limit {
                    line = candidate;
                    break;
                }
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    continue;
                }
                // The word alone is too wide: cut it where it overflows.
                let mut cut = String::new();
                let mut rest = String::new();
                for c in word.chars() {
                    if rest.is_empty() && wide(&format!("{cut}{c}")) <= limit {
                        cut.push(c);
                    } else {
                        rest.push(c);
                    }
                }
                if cut.is_empty() {
                    // A limit narrower than one letter: take the letter anyway.
                    let mut chars = word.chars();
                    cut = chars.next().map(String::from).unwrap_or_default();
                    rest = chars.collect();
                }
                lines.push(cut);
                if rest.is_empty() {
                    break;
                }
                word = rest;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}

/// The lines and the frame of one note.
pub(crate) fn body(note: &Note) -> Body {
    let mono = note.kind == NoteKind::Relation;
    let size = if mono { MONO_TEXT } else { NOTE_TEXT };
    let mut whole = note.text.clone();
    if let Some(result) = &note.result {
        whole.push('\n');
        whole.push_str(&format!("→ {result}"));
    }
    let cut = truncate(&whole);
    let full = (cut != whole).then(|| whole.clone());
    let lines = wrap(&cut, size, mono, MAX_INNER);
    let heading = (note.kind == NoteKind::Relation).then(|| "Relation".to_string());
    let widest = lines
        .iter()
        .map(|l| line_width(l, size, mono))
        .chain(
            heading
                .iter()
                .map(|h| line_width(h, HEADING_TEXT, false) + 2.0 * PAD),
        )
        .fold(0.0, f64::max);
    let width = snap_up(widest + 2.0 * PAD, GRID).max(snap_up(MIN_WIDTH, GRID));
    let mut b = Body {
        heading,
        lines,
        mono,
        width,
        height: 0.0,
        full,
    };
    let rows = b.lines.len().max(1) as f64;
    b.height = snap_up(
        2.0 * PAD + b.heading_room() + (rows - 1.0) * LINE + size * 1.1,
        GRID,
    );
    b
}

/// The placed note for `body` with its frame's top left corner at `at`.
fn make(index: usize, note: &Note, body: Body, at: Pt, anchor: Option<usize>) -> PlacedNote {
    let size = body.size();
    let frame = Rect {
        min: at,
        max: Pt::new(at.x + body.width, at.y + body.height),
    };
    let heading = body.heading.as_ref().map(|h| Text {
        text: h.clone(),
        at: Pt::new(at.x + PAD, at.y + PAD + HEADING_TEXT * 0.78),
        anchor: Anchor::Start,
        vertical: false,
    });
    let first = at.y + PAD + body.heading_room() + size * 0.78;
    let lines = body
        .lines
        .iter()
        .enumerate()
        .map(|(i, line)| Text {
            text: line.clone(),
            at: Pt::new(at.x + PAD, first + i as f64 * LINE),
            anchor: Anchor::Start,
            vertical: false,
        })
        .collect();
    PlacedNote {
        note: index,
        kind: note.kind,
        path: note.path.clone(),
        anchor,
        frame,
        heading,
        lines,
        size,
        mono: body.mono,
        full: body.full,
    }
}

/// The boxes everything else of the layout takes, with the clearance a note keeps from it.
fn obstacles(l: &Layout) -> Vec<Rect> {
    fn grow(r: Rect, by: f64) -> Rect {
        Rect {
            min: Pt::new(r.min.x - by, r.min.y - by),
            max: Pt::new(r.max.x + by, r.max.y + by),
        }
    }
    let mut out: Vec<Rect> = Vec::new();
    for p in &l.parts {
        out.push(grow(p.extent, CLEAR));
        out.push(grow(symbol_box(p), CLEAR));
    }
    for v in views(l) {
        for t in texts(l, &v) {
            out.push(grow(t, 0.8));
        }
        for s in v.segments.iter().copied().chain(bus_segments(l, &v)) {
            out.push(grow(s.rect(), 1.0));
        }
        for &i in &v.symbols {
            out.push(grow(l.symbols[i].extent(), 0.8));
        }
        for &i in &v.ports {
            out.push(grow(l.ports[i].extent(), 0.8));
        }
        for &i in &v.junctions {
            out.push(grow(Rect::around(l.junctions[i].at), 1.0));
        }
        for &i in &v.buses {
            if let Some(f) = &l.buses[i].flag {
                out.push(grow(f.extent(), 0.8));
            }
            out.push(grow(
                Rect {
                    min: l.buses[i].tick.0,
                    max: l.buses[i].tick.1,
                },
                1.0,
            ));
        }
    }
    out
}

/// Which way a pin leaves each box of `parts`, as `[up, down, left, right]`.
fn pin_sides(l: &Layout, parts: &[usize]) -> [bool; 4] {
    let mut sides = [false; 4];
    for p in l.parts.iter().filter(|p| parts.contains(&p.part)) {
        for pin in &p.pins {
            let k = match pin.escape {
                Dir::Up => 0,
                Dir::Down => 1,
                Dir::Left => 2,
                Dir::Right => 3,
            };
            sides[k] = true;
        }
    }
    sides
}

/// The box of the boxes that draw part `part` (a part with several units has more than one).
fn part_box(l: &Layout, part: usize) -> Option<Rect> {
    let mut found: Option<Rect> = None;
    for p in l.parts.iter().filter(|p| p.part == part) {
        match &mut found {
            Some(r) => r.union(p.extent),
            None => found = Some(p.extent),
        }
    }
    found
}

fn centre(r: Rect) -> Pt {
    Pt::new((r.min.x + r.max.x) / 2.0, (r.min.y + r.max.y) / 2.0)
}

fn at_top_left(at: Pt, w: f64, h: f64) -> Rect {
    Rect {
        min: at,
        max: Pt::new(at.x + w, at.y + h),
    }
}

/// Whether `r` is free: inside `region`, and on nothing of `taken`.
fn free(r: Rect, region: Rect, taken: &[Rect]) -> bool {
    r.min.x >= region.min.x - 1e-9
        && r.min.y >= region.min.y - 1e-9
        && r.max.x <= region.max.x + 1e-9
        && r.max.y <= region.max.y + 1e-9
        && !taken.iter().any(|t| overlaps(&r, t, 0.0))
}

fn inflate(r: Rect, by: f64) -> Rect {
    Rect {
        min: Pt::new(r.min.x - by, r.min.y - by),
        max: Pt::new(r.max.x + by, r.max.y + by),
    }
}

/// Where a box of `w` by `h` can stand beside `a`, the nearest first: on each side, at a gap
/// that grows by a step, sliding along the side. A side a pin leaves by costs [`PIN_SIDE`].
fn beside(a: Rect, w: f64, h: f64, pins: [bool; 4]) -> Vec<(f64, Rect)> {
    let mut out = Vec::new();
    let mut gap = NOTE_GAP;
    while gap <= NEAR {
        for (side, pinned) in pins.iter().enumerate() {
            let penalty = if *pinned { PIN_SIDE } else { 0.0 };
            let vertical_side = side >= 2; // left and right: slide up and down
            let (lo, hi) = if vertical_side {
                (a.min.y - h / 4.0, a.max.y + h / 4.0)
            } else {
                (a.min.x - w / 4.0, a.max.x + w / 4.0)
            };
            let mid = if vertical_side {
                (a.min.y + a.max.y) / 2.0
            } else {
                (a.min.x + a.max.x) / 2.0
            };
            let mut slide = lo;
            while slide <= hi + 1e-9 {
                let along = slide;
                let at = match side {
                    // above
                    0 => Pt::new(
                        snap(along - w / 2.0, GRID),
                        snap_up(a.min.y - gap - h, GRID),
                    ),
                    // below
                    1 => Pt::new(snap(along - w / 2.0, GRID), snap_up(a.max.y + gap, GRID)),
                    // left
                    2 => Pt::new(
                        snap_up(a.min.x - gap - w, GRID),
                        snap(along - h / 2.0, GRID),
                    ),
                    // right
                    _ => Pt::new(snap_up(a.max.x + gap, GRID), snap(along - h / 2.0, GRID)),
                };
                let score = gap + 0.35 * (along - mid).abs() + penalty;
                out.push((score, at_top_left(at, w, h)));
                slide += GRID;
            }
        }
        gap += NOTE_GAP;
    }
    out.sort_by(|x, y| {
        x.0.total_cmp(&y.0)
            .then(x.1.min.y.total_cmp(&y.1.min.y))
            .then(x.1.min.x.total_cmp(&y.1.min.x))
    });
    out
}

/// Where a box of `w` by `h` can stand round `c`, the nearest first.
fn around(c: Pt, w: f64, h: f64) -> Vec<(f64, Rect)> {
    let mut out = Vec::new();
    let steps = (AROUND / NOTE_GAP).ceil() as i32;
    for j in -steps..=steps {
        for i in -steps..=steps {
            let (dx, dy) = (f64::from(i) * NOTE_GAP, f64::from(j) * NOTE_GAP);
            let d = dx.hypot(dy);
            if d > AROUND {
                continue;
            }
            let at = Pt::new(
                snap(c.x + dx - w / 2.0, GRID),
                snap(c.y + dy - h / 2.0, GRID),
            );
            out.push((d, at_top_left(at, w, h)));
        }
    }
    out.sort_by(|x, y| {
        x.0.total_cmp(&y.0)
            .then(x.1.min.y.total_cmp(&y.1.min.y))
            .then(x.1.min.x.total_cmp(&y.1.min.x))
    });
    out
}

/// Draws the notes of `design` on `layout`, the sheet it has drawn: see the module's doc. A
/// design with no notes leaves the layout as it is.
///
/// `hints` move notes from where they were put (ADR 0061): each note, in the order of the
/// design, is carried by its offset to the place that is free of everything else (the parts,
/// texts, wires, symbols and the other notes, with the clearance a note keeps), as the notes
/// are placed; one that would lie on something, or stand more than [`hinting::REACH`] outside
/// the drawing, stays where it was put and is a finding. The rest of the notes are placed
/// first, as if there were no file, so a moved note moves nothing else.
pub(super) fn place(
    design: &Design,
    layout: &mut Layout,
    hints: &std::collections::BTreeMap<usize, hinting::NoteEntry>,
) -> Vec<HintFinding> {
    if design.notes.is_empty() {
        return Vec::new();
    }
    let bodies: Vec<Body> = design.notes.iter().map(body).collect();
    let by_path: HashMap<&str, usize> = design
        .parts
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.path.is_empty())
        .map(|(i, p)| (p.path.as_str(), i))
        .collect();
    // The parts each note is about, that are on this sheet.
    let anchors: Vec<Vec<usize>> = design
        .notes
        .iter()
        .map(|n| {
            let mut v: Vec<usize> = n
                .anchors
                .iter()
                .filter_map(|a| by_path.get(a.as_str()).copied())
                .filter(|&p| part_box(layout, p).is_some())
                .collect();
            v.dedup();
            v
        })
        .collect();

    let mut placed: Vec<PlacedNote> = Vec::new();
    let mut taken = obstacles(layout);
    let drawing = taken.clone();
    let region = Rect {
        min: Pt::new(layout.bounds.min.x + GRID, layout.bounds.min.y + GRID),
        max: Pt::new(layout.bounds.max.x - GRID, layout.bounds.max.y - GRID),
    };
    // What is left for the margins: the sheet's own notes (a strip above) and the notes that
    // found no place near their parts (a column or a strip).
    let mut strip: Vec<usize> = Vec::new();
    let mut margin: Vec<usize> = Vec::new();
    let many = design.notes.len() > MANY;
    for (i, note) in design.notes.iter().enumerate() {
        let parts = &anchors[i];
        if parts.is_empty() {
            if many {
                margin.push(i)
            } else {
                strip.push(i)
            }
            continue;
        }
        if many {
            margin.push(i);
            continue;
        }
        let b = &bodies[i];
        let boxes: Vec<Rect> = parts.iter().filter_map(|&p| part_box(layout, p)).collect();
        let candidates = if boxes.len() == 1 {
            beside(boxes[0], b.width, b.height, pin_sides(layout, parts))
        } else {
            let (mut x, mut y) = (0.0, 0.0);
            for r in &boxes {
                x += centre(*r).x;
                y += centre(*r).y;
            }
            let n = boxes.len() as f64;
            around(Pt::new(x / n, y / n), b.width, b.height)
        };
        let spot = candidates
            .into_iter()
            .find(|(_, r)| free(*r, region, &taken))
            .map(|(_, r)| r);
        match spot {
            Some(r) => {
                let p = make(i, note, body_of(&bodies, i), r.min, parts.first().copied());
                taken.push(inflate(p.frame, CLEAR));
                placed.push(p);
            }
            None => margin.push(i),
        }
    }

    // The margin: a column to the right of the drawing, or a strip under it, whichever leaves
    // the sheet nearer the shape the sheets aim for.
    let content = Rect {
        min: Pt::new(layout.bounds.min.x + BORDER, layout.bounds.min.y + BORDER),
        max: Pt::new(layout.bounds.max.x - BORDER, layout.bounds.max.y - BORDER),
    };
    let mut bounds = layout.bounds;
    if !margin.is_empty() {
        let size = |i: usize| (bodies[i].width, bodies[i].height);
        // A column at the right, the notes in the order of the parts they are about from the
        // top, each as level with its part as the notes above it leave room for.
        let wanted = |i: usize| -> f64 {
            let boxes: Vec<Rect> = anchors[i]
                .iter()
                .filter_map(|&p| part_box(layout, p))
                .collect();
            if boxes.is_empty() {
                content.min.y
            } else {
                let mean = boxes.iter().map(|r| centre(*r).y).sum::<f64>() / boxes.len() as f64;
                mean - size(i).1 / 2.0
            }
        };
        let mut by_height = margin.clone();
        by_height.sort_by(|&a, &b| wanted(a).total_cmp(&wanted(b)).then(a.cmp(&b)));
        let x0 = snap_up(content.max.x + MARGIN_GAP, GRID);
        let mut column: Vec<(usize, Pt)> = Vec::new();
        let mut y = snap_up(content.min.y, GRID);
        let mut widest = 0.0f64;
        for &i in &by_height {
            let (w, h) = size(i);
            let top = snap_up(y.max(wanted(i)), GRID);
            column.push((i, Pt::new(x0, top)));
            y = snap_up(top + h + NOTE_GAP, GRID);
            widest = widest.max(w);
        }
        let column_bounds = {
            let mut b = bounds;
            b.max.x = b.max.x.max(snap(x0 + widest + BORDER, GRID));
            b.max.y = b.max.y.max(snap(y - NOTE_GAP + BORDER, GRID));
            b
        };
        // A strip under it, filled from the left to the drawing's width.
        let y0 = snap_up(content.max.y + MARGIN_GAP, GRID);
        let limit = (content.width()).max(margin.iter().map(|&i| size(i).0).fold(0.0, f64::max));
        let mut row: Vec<(usize, Pt)> = Vec::new();
        let (mut x, mut yy, mut tall) = (snap_up(content.min.x, GRID), y0, 0.0f64);
        for &i in &margin {
            let (w, h) = size(i);
            if x > content.min.x + 0.5 && x + w > content.min.x + limit {
                x = snap_up(content.min.x, GRID);
                yy = snap_up(yy + tall + NOTE_GAP, GRID);
                tall = 0.0;
            }
            row.push((i, Pt::new(x, yy)));
            x = snap_up(x + w + NOTE_GAP, GRID);
            tall = tall.max(h);
        }
        let strip_bounds = {
            let mut b = bounds;
            let right = row
                .iter()
                .map(|&(i, at)| at.x + size(i).0)
                .fold(0.0, f64::max);
            b.max.x = b.max.x.max(snap(right + BORDER, GRID));
            b.max.y = b.max.y.max(snap(yy + tall + BORDER, GRID));
            b
        };
        let miss = |b: Rect| (b.width() / b.height().max(1.0) / TARGET_ASPECT).ln().abs();
        let use_column = many || miss(column_bounds) <= miss(strip_bounds);
        let (chosen, nb) = if use_column {
            (column, column_bounds)
        } else {
            (row, strip_bounds)
        };
        bounds = nb;
        for (i, at) in chosen {
            let p = make(
                i,
                &design.notes[i],
                body_of(&bodies, i),
                at,
                anchors[i].first().copied(),
            );
            placed.push(p);
        }
    }

    // The sheet's own notes: in a strip above the drawing, at the left, under the title.
    if !strip.is_empty() {
        let limit = content
            .width()
            .max(strip.iter().map(|&i| bodies[i].width).fold(0.0, f64::max));
        let mut rows: Vec<Vec<(usize, f64)>> = vec![Vec::new()];
        let mut x = 0.0;
        for &i in &strip {
            let w = bodies[i].width;
            if x > 0.0 && x + w > limit {
                rows.push(Vec::new());
                x = 0.0;
            }
            rows.last_mut().expect("a row").push((i, x));
            x += w + NOTE_GAP;
        }
        let heights: Vec<f64> = rows
            .iter()
            .map(|r| r.iter().map(|&(i, _)| bodies[i].height).fold(0.0, f64::max))
            .collect();
        let total: f64 = heights.iter().sum::<f64>() + NOTE_GAP * (rows.len() - 1) as f64;
        let bottom = snap(content.min.y - MARGIN_GAP, GRID);
        let top = snap_up(bottom - total, GRID);
        let mut y = top;
        let mut right = 0.0f64;
        for (r, row) in rows.iter().enumerate() {
            for &(i, dx) in row {
                let at = Pt::new(snap_up(content.min.x + dx, GRID), y);
                placed.push(make(i, &design.notes[i], body_of(&bodies, i), at, None));
                right = right.max(at.x + bodies[i].width);
            }
            y = snap_up(y + heights[r] + NOTE_GAP, GRID);
        }
        bounds.min.y = bounds.min.y.min(snap(top - BORDER, GRID));
        bounds.max.x = bounds.max.x.max(snap(right + BORDER, GRID));
    }

    placed.sort_by_key(|p| p.note);
    let mut findings = Vec::new();
    for (&i, hint) in hints {
        let Some(at) = placed.iter().position(|p| p.note == i) else {
            continue;
        };
        let (dx, dy) = hint.by.mm();
        if dx == 0.0 && dy == 0.0 {
            continue;
        }
        let by = Pt::new(dx, dy);
        let frame = placed[at].frame.translated(by);
        let reach = inflate(bounds, hinting::REACH);
        let outside = frame.min.x < reach.min.x
            || frame.min.y < reach.min.y
            || frame.max.x > reach.max.x
            || frame.max.y > reach.max.y;
        let lies_on = drawing.iter().any(|t| overlaps(&frame, t, 0.0))
            || placed
                .iter()
                .enumerate()
                .any(|(j, o)| j != at && overlaps(&frame, &inflate(o.frame, CLEAR), 0.0));
        if outside {
            findings.push(hinting::note_finding(
                hint,
                &format!(
                    "the note would stand more than {:.0} mm outside the drawing",
                    hinting::REACH
                ),
            ));
        } else if lies_on {
            findings.push(hinting::note_finding(
                hint,
                "the note would lie on a part, a wire, a text or another note at the place the file gives it",
            ));
        } else {
            shift_note(&mut placed[at], by);
            bounds.union(inflate(frame, BORDER));
        }
    }
    layout.bounds = bounds;
    layout.notes = placed;
    findings
}

/// Moves a placed note, its frame and its text, by `by`.
fn shift_note(note: &mut PlacedNote, by: Pt) {
    note.frame = note.frame.translated(by);
    for t in note.heading.iter_mut().chain(note.lines.iter_mut()) {
        t.at = Pt::new(t.at.x + by.x, t.at.y + by.y);
    }
}

/// A copy of the body of note `i` (a body is moved into the note it makes).
fn body_of(bodies: &[Body], i: usize) -> Body {
    let b = &bodies[i];
    Body {
        heading: b.heading.clone(),
        lines: b.lines.clone(),
        mono: b.mono,
        width: b.width,
        height: b.height,
        full: b.full.clone(),
    }
}

/// The box a text of a note covers as it is drawn, for the check: from its start across the
/// width the text has in its face, a letter's height up from the baseline and a third below it.
pub(crate) fn text_box(t: &Text, size: f64, mono: bool) -> Rect {
    let w = line_width(&t.text, size, mono);
    Rect {
        min: Pt::new(t.at.x, t.at.y - size * 0.8),
        max: Pt::new(t.at.x + w, t.at.y + size * 0.3),
    }
}
