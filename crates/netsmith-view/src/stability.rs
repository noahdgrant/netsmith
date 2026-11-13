//! How much a sheet moves when its design changes a little, measured.
//!
//! A person who edits a board reads the same drawing again after each edit and builds a map of
//! it in their head; an edit that moves everything destroys the map. This module applies the
//! small edits a person makes ([`Kind`]) to a board, draws it before and after, and says, for
//! each sheet, what stayed where it was ([`Movement`]):
//!
//! * the parts present in both, how many did not move (to 0.01 mm) and the furthest one moved;
//! * the wire segments of the old drawing that are, point for point, in the new one;
//! * the net labels and flags that are the same;
//! * whether the sheet's rows or its aspect band changed.
//!
//! Each is measured twice: in the **anchor frame**, where both drawings are translated so that
//! the same reference part is at the origin (the part present in both that is largest, ties by
//! its path: a drawing that grew on its low side is not "moved" by that), and in the **raw
//! frame**, the coordinates the layout has. Nothing here draws or changes a drawing: it is
//! std-only code that reads layouts, for tests and for `schematic_metrics --stability`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use netsmith_core::{Design, Net, Part, PinRef};
use netsmith_kicad::{Mapping, SymbolLib};

use crate::geom::{Orient, Pt};
use crate::layout::{Layout, Marks, PlacedPart, Rails};
use crate::sheets::{Group, layout_sheets_marked};
use crate::sheetview::{SheetView, views};

/// Everything a drawing is made from.
#[derive(Debug, Clone)]
pub struct Board {
    pub design: Design,
    pub groups: Vec<Group>,
    pub rails: Rails,
    pub marks: Marks,
}

impl Board {
    pub fn draw(&self, mapping: &Mapping, lib: &SymbolLib) -> Layout {
        layout_sheets_marked(
            &self.design,
            &self.groups,
            &self.rails,
            &self.marks,
            mapping,
            lib,
        )
        .expect("a board that is drawn once can be drawn again after an edit")
    }
}

/// The kinds of small edit a person makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// (a) Give a signal net another name.
    RenameNet,
    /// (b) Add a decoupling capacitor between a supply and a ground, beside a pin of the
    /// sheet's largest part that is on the supply.
    AddDecouplingCap,
    /// (c) Add a pull-up from a signal pin of the sheet's largest part to a supply.
    AddPullUp,
    /// (d) Take a small part out.
    DeletePart,
    /// (e) Change a value to another of the same length (no effect on the drawing).
    ChangeValue,
    /// (f) Add a part to another module (this sheet must not change).
    AddToOtherModule,
    /// (g) Add a part that is on no net to this sheet.
    AddUnrelatedPart,
    /// (h) Give a supply rail another name (a rail is drawn with a symbol at each pin).
    RenameRail,
}

impl Kind {
    pub const ALL: [Kind; 8] = [
        Kind::RenameNet,
        Kind::AddDecouplingCap,
        Kind::AddPullUp,
        Kind::DeletePart,
        Kind::ChangeValue,
        Kind::AddToOtherModule,
        Kind::AddUnrelatedPart,
        Kind::RenameRail,
    ];

    /// The letter of the edit in reports.
    pub fn letter(self) -> char {
        match self {
            Kind::RenameNet => 'a',
            Kind::AddDecouplingCap => 'b',
            Kind::AddPullUp => 'c',
            Kind::DeletePart => 'd',
            Kind::ChangeValue => 'e',
            Kind::AddToOtherModule => 'f',
            Kind::AddUnrelatedPart => 'g',
            Kind::RenameRail => 'h',
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::RenameNet => "rename a net",
            Kind::AddDecouplingCap => "add a decoupling capacitor",
            Kind::AddPullUp => "add a pull-up",
            Kind::DeletePart => "delete a part",
            Kind::ChangeValue => "change a value",
            Kind::AddToOtherModule => "add a part to another module",
            Kind::AddUnrelatedPart => "add an unrelated part",
            Kind::RenameRail => "rename a supply rail",
        }
    }

    /// Whether the edit leaves the measured sheet's drawing alone by design: a value of the
    /// same length, a part of another module.
    pub fn changes_nothing_here(self) -> bool {
        matches!(self, Kind::ChangeValue | Kind::AddToOtherModule)
    }
}

/// One concrete edit: which sheet it is for and what it does.
#[derive(Debug, Clone)]
pub struct Edit {
    pub kind: Kind,
    /// The sheet that is measured.
    pub sheet: String,
    /// What it does, for a report.
    pub label: String,
    action: Action,
}

#[derive(Debug, Clone)]
enum Action {
    Rename {
        from: String,
        to: String,
    },
    /// A new part with its pins on nets, put after the part with refdes `after` in the design
    /// and in its group.
    Add {
        part: Part,
        nets: Vec<(String, String)>,
        after: String,
    },
    Delete(String),
    Value {
        refdes: String,
        to: String,
    },
}

/// The board after an edit, with the keys of the parts it added or took out (which no
/// comparison counts) and the net renames.
#[derive(Debug, Clone)]
pub struct Edited {
    pub board: Board,
    pub touched: Vec<String>,
    pub renames: BTreeMap<String, String>,
}

/// What identifies a part across edits: its path (renumbering designators does not change
/// it), else its reference.
pub fn part_key(part: &Part) -> String {
    if part.path.is_empty() {
        part.refdes.clone()
    } else {
        part.path.clone()
    }
}

fn box_key(design: &Design, placed: &PlacedPart) -> String {
    let base = part_key(&design.parts[placed.part]);
    match placed.glyph.unit {
        Some(u) => format!("{base}#{u}"),
        None => base,
    }
}

/// The design parts that are on a sheet, by the sheet's view of a drawing.
fn sheet_parts(layout: &Layout, v: &SheetView) -> Vec<usize> {
    let set: BTreeSet<usize> = v.parts.iter().map(|&b| layout.parts[b].part).collect();
    set.into_iter().collect()
}

/// How many pins of nets each part has: its size to the planner.
fn net_pins(design: &Design) -> Vec<usize> {
    let index: BTreeMap<&str, usize> = design
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();
    let mut count = vec![0; design.parts.len()];
    for net in &design.nets {
        for pin in &net.pins {
            if let Some(&i) = index.get(pin.refdes.as_str()) {
                count[i] += 1;
            }
        }
    }
    count
}

/// The names of the sheets of a drawing, in order.
pub fn sheet_names(layout: &Layout) -> Vec<String> {
    views(layout).into_iter().map(|v| v.name).collect()
}

/// An edit of `kind` for the sheet called `sheet` of the drawing `layout` of `board`, chosen
/// by a rule that reads only the design (the sheet's largest part, its busiest signal net,
/// the middle one of its two-pin parts), or none when the sheet has nothing to apply it to.
pub fn plan(board: &Board, layout: &Layout, sheet: &str, kind: Kind) -> Option<Edit> {
    let all = views(layout);
    let here = all.iter().find(|v| v.name == sheet)?;
    let on_sheet = sheet_parts(layout, here);
    let design = &board.design;
    let pins = net_pins(design);
    // Larger first, ties by path.
    let mut by_size = on_sheet.clone();
    by_size.sort_by(|&a, &b| {
        pins[b]
            .cmp(&pins[a])
            .then_with(|| part_key(&design.parts[a]).cmp(&part_key(&design.parts[b])))
    });
    let main = *by_size.first()?;
    let main_refdes = design.parts[main].refdes.clone();
    let on =
        |parts: &[usize], refdes: &str| parts.iter().any(|&p| design.parts[p].refdes == refdes);
    let is_rail =
        |name: &str| board.rails.positive.contains(name) || board.rails.ground.contains(name);
    let pins_in =
        |parts: &[usize], net: &Net| net.pins.iter().filter(|p| on(parts, &p.refdes)).count();
    let positive = |net: &Net| board.rails.positive.contains(&net.name);
    let ground = |net: &Net| board.rails.ground.contains(&net.name);
    let module_prefix = |p: &Part| match p.path.rfind('.') {
        Some(i) => format!("{}.", &p.path[..i]),
        None => String::new(),
    };
    let new_part = |kind: &str, refdes: &str, value: &str, near: &Part, name: &str| Part {
        provides: Vec::new(),
        class: None,
        refdes: refdes.into(),
        kind: kind.into(),
        value: value.into(),
        package: Some("0402".into()),
        path: format!("{}{name}", module_prefix(near)),
        pins: Vec::new(),
    };
    let sheet = sheet.to_string();
    let main_nets: Vec<&Net> = design
        .nets
        .iter()
        .filter(|n| n.pins.iter().any(|p| p.refdes == main_refdes))
        .collect();
    match kind {
        Kind::RenameNet | Kind::RenameRail => {
            let rail = kind == Kind::RenameRail;
            let mut nets: Vec<&Net> = design
                .nets
                .iter()
                .filter(|n| {
                    pins_in(&on_sheet, n) >= 1
                        && crate::layout::is_named(&n.name)
                        && if rail { positive(n) } else { !is_rail(&n.name) }
                })
                .collect();
            // The most pins, then the name.
            nets.sort_by(|a, b| {
                pins_in(&on_sheet, b)
                    .cmp(&pins_in(&on_sheet, a))
                    .then(a.name.cmp(&b.name))
            });
            let net = nets.first()?;
            // The same length, and sorting last: a rename must not make the drawing depend on
            // the order of the names.
            let mut to: Vec<char> = net.name.chars().collect();
            let at = to.iter().position(|c| c.is_alphanumeric())?;
            to[at] = if to[at] == 'Z' { 'Y' } else { 'Z' };
            let to: String = to.into_iter().collect();
            Some(Edit {
                kind,
                sheet,
                label: format!("rename net {} to {to}", net.name),
                action: Action::Rename {
                    from: net.name.clone(),
                    to,
                },
            })
        }
        Kind::AddDecouplingCap => {
            let supply = main_nets.iter().find(|n| positive(n))?;
            let gnd = design
                .nets
                .iter()
                .filter(|n| ground(n) && pins_in(&on_sheet, n) >= 1)
                .min_by(|a, b| a.name.cmp(&b.name))?;
            let part = new_part(
                "Capacitor",
                "C909",
                "100n",
                &design.parts[main],
                "new_decoupling",
            );
            Some(Edit {
                kind,
                sheet,
                label: format!(
                    "add {} between {} and {} beside {main_refdes}",
                    part.refdes, supply.name, gnd.name
                ),
                action: Action::Add {
                    nets: vec![
                        (supply.name.clone(), "1".into()),
                        (gnd.name.clone(), "2".into()),
                    ],
                    part,
                    after: main_refdes,
                },
            })
        }
        Kind::AddPullUp => {
            let supply = design
                .nets
                .iter()
                .filter(|n| positive(n) && pins_in(&on_sheet, n) >= 1)
                .min_by(|a, b| a.name.cmp(&b.name))?;
            let mut signals: Vec<&&Net> = main_nets
                .iter()
                .filter(|n| !is_rail(&n.name) && crate::layout::is_named(&n.name))
                .collect();
            signals.sort_by(|a, b| a.name.cmp(&b.name));
            let signal = signals.first()?;
            let part = new_part("Resistor", "R908", "10k", &design.parts[main], "new_pullup");
            Some(Edit {
                kind,
                sheet,
                label: format!(
                    "add {} from {} to {} beside {main_refdes}",
                    part.refdes, signal.name, supply.name
                ),
                action: Action::Add {
                    nets: vec![
                        (signal.name.clone(), "1".into()),
                        (supply.name.clone(), "2".into()),
                    ],
                    part,
                    after: main_refdes,
                },
            })
        }
        Kind::DeletePart => {
            let mut small: Vec<usize> = on_sheet
                .iter()
                .copied()
                .filter(|&p| pins[p] == 2 && p != main)
                .collect();
            small.sort_by_key(|&p| part_key(&design.parts[p]));
            let gone = *small.get(small.len() / 2)?;
            Some(Edit {
                kind,
                sheet,
                label: format!("delete {}", design.parts[gone].refdes),
                action: Action::Delete(design.parts[gone].refdes.clone()),
            })
        }
        Kind::ChangeValue => {
            let mut valued: Vec<usize> = on_sheet
                .iter()
                .copied()
                .filter(|&p| design.parts[p].value.chars().any(|c| c.is_ascii_digit()))
                .collect();
            valued.sort_by_key(|&p| part_key(&design.parts[p]));
            let p = *valued.first()?;
            let mut to: Vec<char> = design.parts[p].value.chars().collect();
            let at = to.iter().position(char::is_ascii_digit)?;
            let d = to[at].to_digit(10)?;
            to[at] = char::from_digit((d + 1) % 10, 10)?;
            let to: String = to.into_iter().collect();
            Some(Edit {
                kind,
                sheet,
                label: format!("change {} to {to}", design.parts[p].refdes),
                action: Action::Value {
                    refdes: design.parts[p].refdes.clone(),
                    to,
                },
            })
        }
        Kind::AddToOtherModule => {
            let other = all
                .iter()
                .find(|v| v.name != sheet && !v.parts.is_empty())?;
            let there = sheet_parts(layout, other);
            let near = *there.first()?;
            let part = new_part(
                "Capacitor",
                "C909",
                "100n",
                &design.parts[near],
                "new_elsewhere",
            );
            let supply = design
                .nets
                .iter()
                .find(|n| positive(n) && pins_in(&there, n) >= 1);
            let gnd = design
                .nets
                .iter()
                .find(|n| ground(n) && pins_in(&there, n) >= 1);
            let nets = match (supply, gnd) {
                (Some(s), Some(g)) => {
                    vec![(s.name.clone(), "1".into()), (g.name.clone(), "2".into())]
                }
                _ => Vec::new(),
            };
            Some(Edit {
                kind,
                label: format!("add {} to sheet {}", part.refdes, other.name),
                sheet,
                action: Action::Add {
                    after: design.parts[near].refdes.clone(),
                    nets,
                    part,
                },
            })
        }
        Kind::AddUnrelatedPart => {
            // After the last part of the sheet, with a path that sorts last.
            let last = *on_sheet.iter().max()?;
            let part = new_part(
                "Resistor",
                "R999",
                "1k",
                &design.parts[last],
                "zz_unrelated",
            );
            Some(Edit {
                kind,
                label: format!("add {} on no net", part.refdes),
                sheet,
                action: Action::Add {
                    after: design.parts[last].refdes.clone(),
                    nets: Vec::new(),
                    part,
                },
            })
        }
    }
}

/// `board` after `edit`.
pub fn apply(board: &Board, edit: &Edit) -> Edited {
    let mut board = board.clone();
    let mut touched = Vec::new();
    let mut renames = BTreeMap::new();
    match &edit.action {
        Action::Rename { from, to } => {
            if let Some(n) = board.design.nets.iter_mut().find(|n| &n.name == from) {
                n.name = to.clone();
            }
            for set in [&mut board.rails.ground, &mut board.rails.positive] {
                if set.remove(from) {
                    set.insert(to.clone());
                }
            }
            for i in &mut board.design.interfaces {
                for m in &mut i.members {
                    if &m.net == from {
                        m.net = to.clone();
                    }
                }
            }
            renames.insert(from.clone(), to.clone());
        }
        Action::Add { part, nets, after } => {
            let after = board
                .design
                .parts
                .iter()
                .position(|p| &p.refdes == after)
                .expect("the part the new one goes after exists");
            let at = after + 1;
            board.design.parts.insert(at, part.clone());
            for g in &mut board.groups {
                for p in &mut g.parts {
                    if *p >= at {
                        *p += 1;
                    }
                }
            }
            if let Some(g) = board.groups.iter_mut().find(|g| g.parts.contains(&after)) {
                let pos = g.parts.iter().position(|&p| p == after).unwrap_or(0);
                g.parts.insert(pos + 1, at);
            }
            for (name, pad) in nets {
                if let Some(n) = board.design.nets.iter_mut().find(|n| &n.name == name) {
                    n.pins.push(PinRef {
                        refdes: part.refdes.clone(),
                        pin: pad.clone(),
                    });
                }
            }
            touched.push(part_key(part));
        }
        Action::Delete(refdes) => {
            let gone = board
                .design
                .parts
                .iter()
                .position(|p| &p.refdes == refdes)
                .expect("the part to delete exists");
            touched.push(part_key(&board.design.parts[gone]));
            board.design.parts.remove(gone);
            for net in &mut board.design.nets {
                net.pins.retain(|p| &p.refdes != refdes);
            }
            board.design.nets.retain(|n| !n.pins.is_empty());
            for g in &mut board.groups {
                g.parts.retain(|&p| p != gone);
                for p in &mut g.parts {
                    if *p > gone {
                        *p -= 1;
                    }
                }
            }
            board.groups.retain(|g| !g.parts.is_empty());
        }
        Action::Value { refdes, to } => {
            if let Some(p) = board.design.parts.iter_mut().find(|p| &p.refdes == refdes) {
                p.value = to.clone();
            }
        }
    }
    Edited {
        board,
        touched,
        renames,
    }
}

type Tag = (char, String, String, [i64; 2], u8);

/// One sheet of one drawing as sets of exact things, so two drawings can be compared.
struct SheetDrawing {
    parts: BTreeMap<String, PartAt>,
    /// Net name, then a segment between two points in hundredths of a millimetre.
    segments: BTreeSet<(String, [i64; 4])>,
    /// Labels and flags: kind, net name, text, point, direction.
    tags: BTreeSet<Tag>,
    frame: [f64; 4],
    rows: usize,
}

#[derive(Clone, Copy)]
struct PartAt {
    at: Pt,
    orient: Orient,
    /// Size to choose the reference part by: pins, then the area of the symbol.
    size: (usize, i64),
}

fn hundredths(v: f64) -> i64 {
    (v * 100.0).round() as i64
}

fn extract(design: &Design, layout: &Layout, name: &str) -> Option<SheetDrawing> {
    let v = views(layout).into_iter().find(|v| v.name == name)?;
    let mut parts = BTreeMap::new();
    for &b in &v.parts {
        let placed = &layout.parts[b];
        let r = placed.glyph.bounds();
        parts.insert(
            box_key(design, placed),
            PartAt {
                at: placed.at,
                orient: placed.orient,
                size: (
                    placed.pins.len(),
                    hundredths((r.max.x - r.min.x) * (r.max.y - r.min.y)),
                ),
            },
        );
    }
    let net_name = |n: usize| design.nets[n].name.clone();
    let mut segments = BTreeSet::new();
    for s in &v.segments {
        let (a, b) = (
            [hundredths(s.a.x), hundredths(s.a.y)],
            [hundredths(s.b.x), hundredths(s.b.y)],
        );
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        segments.insert((net_name(s.net), [lo[0], lo[1], hi[0], hi[1]]));
    }
    let mut tags = BTreeSet::new();
    for &i in &v.labels {
        let l = &layout.labels[i];
        tags.insert((
            'L',
            net_name(l.net),
            l.text.text.clone(),
            [hundredths(l.at.x), hundredths(l.at.y)],
            l.dir as u8,
        ));
    }
    for &i in &v.ports {
        let p = &layout.ports[i];
        tags.insert((
            'F',
            net_name(p.net),
            p.label.as_ref().map(|t| t.text.clone()).unwrap_or_default(),
            [hundredths(p.at.x), hundredths(p.at.y)],
            p.dir as u8,
        ));
    }
    let rows = layout
        .sheets
        .iter()
        .find(|s| s.name == name)
        .map_or(layout.rows, |s| s.rows);
    Some(SheetDrawing {
        parts,
        segments,
        tags,
        frame: [v.frame.min.x, v.frame.min.y, v.frame.max.x, v.frame.max.y],
        rows,
    })
}

/// What stayed, in one frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frame {
    /// Parts in both drawings, other than the edited one.
    pub parts: usize,
    /// Of them, those that did not move (0.01 mm) or turn.
    pub unmoved: usize,
    /// The furthest one moved, in mm.
    pub max_move: f64,
    /// Wire segments of the old drawing, and those that are in the new one as they were.
    pub segments: usize,
    pub segments_same: usize,
    /// The same for net labels and flags.
    pub tags: usize,
    pub tags_same: usize,
    /// How far the origin of the frame moved, in mm: in the raw frame the corner of the
    /// sheet's box; in the anchor frame the reference part, which is 0 by construction.
    pub origin: f64,
}

impl Frame {
    pub fn unmoved_fraction(&self) -> f64 {
        frac(self.unmoved, self.parts)
    }

    pub fn segments_fraction(&self) -> f64 {
        frac(self.segments_same, self.segments)
    }

    pub fn tags_fraction(&self) -> f64 {
        frac(self.tags_same, self.tags)
    }

    /// Nothing counted differs.
    pub fn is_identical(&self) -> bool {
        self.unmoved == self.parts
            && self.segments_same == self.segments
            && self.tags_same == self.tags
    }
}

fn frac(a: usize, b: usize) -> f64 {
    if b == 0 { 1.0 } else { a as f64 / b as f64 }
}

/// What changed in one sheet for one edit.
#[derive(Debug, Clone, PartialEq)]
pub struct Movement {
    pub sheet: String,
    /// Parts of the sheet before the edit.
    pub size: usize,
    pub anchor: Frame,
    pub raw: Frame,
    pub rows: (usize, usize),
    /// The sheet's aspect (width over height) before and after.
    pub aspect: (f64, f64),
    /// The reference part, by key.
    pub reference: String,
    /// The reference part's origin before and after, and the sheet's box (min x, min y, max x,
    /// max y) before and after: what a picture of the two drawings over each other needs.
    pub reference_at: [(f64, f64); 2],
    /// Each part that moved in the anchor frame, by key, and by how much (x, y in mm).
    pub moves: Vec<(String, (f64, f64))>,
    pub frames: [[f64; 4]; 2],
}

/// The band a sheet's aspect is in: a change of band is what a person sees as a different
/// kind of page.
pub fn aspect_band(aspect: f64) -> usize {
    [0.6, 0.9, 1.3, 1.8, 2.5]
        .iter()
        .filter(|&&t| aspect >= t)
        .count()
}

impl Movement {
    pub fn rows_changed(&self) -> bool {
        self.rows.0 != self.rows.1
    }

    pub fn band_changed(&self) -> bool {
        aspect_band(self.aspect.0) != aspect_band(self.aspect.1)
    }
}

fn is_touched(key: &str, touched: &[String]) -> bool {
    let base = key.split('#').next().unwrap_or(key);
    touched.iter().any(|t| t == base)
}

/// Compares sheet `name` of two drawings. `touched` are the keys of the parts the edit added
/// or took out and `renames` maps old net names to new.
pub fn compare(
    before: (&Layout, &Design),
    after: (&Layout, &Design),
    name: &str,
    touched: &[String],
    renames: &BTreeMap<String, String>,
) -> Option<Movement> {
    let a = extract(before.1, before.0, name)?;
    let b = extract(after.1, after.0, name)?;
    // The reference part: present in both, largest, ties by key.
    let reference = a
        .parts
        .iter()
        .filter(|(k, _)| b.parts.contains_key(*k) && !is_touched(k, touched))
        .max_by(|(ka, pa), (kb, pb)| pa.size.cmp(&pb.size).then_with(|| kb.cmp(ka)))
        .map(|(k, _)| k.clone())?;
    let (ra, rb) = (a.parts[&reference].at, b.parts[&reference].at);
    let shift = |x: i64, y: i64, by: Pt| {
        [
            hundredths(x as f64 / 100.0 - by.x),
            hundredths(y as f64 / 100.0 - by.y),
        ]
    };
    let measure = |by_a: Pt, by_b: Pt, origin: f64| -> Frame {
        let mut f = Frame {
            origin,
            ..Frame::default()
        };
        for (k, pa) in &a.parts {
            let Some(pb) = b.parts.get(k) else { continue };
            if is_touched(k, touched) {
                continue;
            }
            f.parts += 1;
            let d = (
                (pb.at.x - by_b.x) - (pa.at.x - by_a.x),
                (pb.at.y - by_b.y) - (pa.at.y - by_a.y),
            );
            let dist = (d.0 * d.0 + d.1 * d.1).sqrt();
            f.max_move = f.max_move.max(dist);
            if dist < 0.01 && pa.orient == pb.orient {
                f.unmoved += 1;
            }
        }
        let moved = |s: &(String, [i64; 4]), by: Pt| {
            let p = shift(s.1[0], s.1[1], by);
            let q = shift(s.1[2], s.1[3], by);
            let (lo, hi) = if p <= q { (p, q) } else { (q, p) };
            (s.0.clone(), [lo[0], lo[1], hi[0], hi[1]])
        };
        let old: BTreeSet<_> = a
            .segments
            .iter()
            .map(|s| {
                let (n, c) = moved(s, by_a);
                (renames.get(&n).cloned().unwrap_or(n), c)
            })
            .collect();
        let new: BTreeSet<_> = b.segments.iter().map(|s| moved(s, by_b)).collect();
        f.segments = old.len();
        f.segments_same = old.intersection(&new).count();
        let tag = |t: &Tag, by: Pt, rename: bool| {
            let at = shift(t.3[0], t.3[1], by);
            let swap = |s: &String| {
                if rename {
                    renames.get(s).cloned().unwrap_or_else(|| s.clone())
                } else {
                    s.clone()
                }
            };
            (t.0, swap(&t.1), swap(&t.2), at, t.4)
        };
        let old: BTreeSet<_> = a.tags.iter().map(|t| tag(t, by_a, true)).collect();
        let new: BTreeSet<_> = b.tags.iter().map(|t| tag(t, by_b, false)).collect();
        f.tags = old.len();
        f.tags_same = old.intersection(&new).count();
        f
    };
    let moves: Vec<(String, (f64, f64))> = a
        .parts
        .iter()
        .filter_map(|(k, pa)| {
            let pb = b.parts.get(k)?;
            let d = (
                (pb.at.x - rb.x) - (pa.at.x - ra.x),
                (pb.at.y - rb.y) - (pa.at.y - ra.y),
            );
            (!is_touched(k, touched) && (d.0.abs() > 0.01 || d.1.abs() > 0.01))
                .then(|| (k.clone(), d))
        })
        .collect();
    let zero = Pt::new(0.0, 0.0);
    let origin = ((b.frame[0] - a.frame[0]).powi(2) + (b.frame[1] - a.frame[1]).powi(2)).sqrt();
    let raw = measure(zero, zero, origin);
    let anchor = measure(ra, rb, 0.0);
    let aspect = |f: &[f64; 4]| (f[2] - f[0]) / (f[3] - f[1]).max(1.0);
    Some(Movement {
        sheet: name.to_string(),
        size: a.parts.len(),
        reference_at: [(ra.x, ra.y), (rb.x, rb.y)],
        moves,
        frames: [a.frame, b.frame],
        anchor,
        raw,
        rows: (a.rows, b.rows),
        aspect: (aspect(&a.frame), aspect(&b.frame)),
        reference,
    })
}

/// One line of the measurement: a sheet, an edit and what moved.
#[derive(Debug, Clone)]
pub struct Row {
    pub board: String,
    pub kind: Kind,
    pub label: String,
    pub movement: Movement,
}

/// Every edit of [`Kind`] on every sheet of `board` (or only the sheets named, when `only` is
/// not empty), each drawn and compared with the board as it is.
pub fn measure(
    name: &str,
    board: &Board,
    mapping: &Mapping,
    lib: &SymbolLib,
    only: &[String],
) -> Vec<Row> {
    let base = board.draw(mapping, lib);
    let mut out = Vec::new();
    for sheet in sheet_names(&base) {
        if !only.is_empty() && !only.contains(&sheet) {
            continue;
        }
        for kind in Kind::ALL {
            let Some(edit) = plan(board, &base, &sheet, kind) else {
                continue;
            };
            let edited = apply(board, &edit);
            let after = edited.board.draw(mapping, lib);
            let Some(movement) = compare(
                (&base, &board.design),
                (&after, &edited.board.design),
                &sheet,
                &edited.touched,
                &edited.renames,
            ) else {
                continue;
            };
            out.push(Row {
                board: name.to_string(),
                kind,
                label: edit.label,
                movement,
            });
        }
    }
    out
}

/// The rows as a table, one line each, and then for each kind of edit a summary over the
/// sheets of at least `min_parts` parts whose rows did not change.
pub fn table(rows: &[Row], min_parts: usize) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{:<24} {:<2} {:>3} | anchor: {:>5} {:>7} {:>5} {:>6} | raw: {:>5} {:>7} {:>5} {:>6} {:>7} | rows band  edit",
        "sheet",
        "ed",
        "n",
        "unmov",
        "max mm",
        "segs",
        "labels",
        "unmov",
        "max mm",
        "segs",
        "labels",
        "origin"
    );
    let pct = |f: f64| format!("{:.0}%", 100.0 * f);
    for r in rows {
        let m = &r.movement;
        let _ = writeln!(
            s,
            "{:<24} {:<2} {:>3} | anchor: {:>5} {:>7.2} {:>5} {:>6} | raw: {:>5} {:>7.2} {:>5} {:>6} {:>7.2} | {}>{} {}  {}",
            format!("{}/{}", r.board, m.sheet)
                .chars()
                .take(24)
                .collect::<String>(),
            r.kind.letter(),
            m.size,
            pct(m.anchor.unmoved_fraction()),
            m.anchor.max_move,
            pct(m.anchor.segments_fraction()),
            pct(m.anchor.tags_fraction()),
            pct(m.raw.unmoved_fraction()),
            m.raw.max_move,
            pct(m.raw.segments_fraction()),
            pct(m.raw.tags_fraction()),
            m.raw.origin,
            m.rows.0,
            m.rows.1,
            if m.band_changed() { "band" } else { "-   " },
            r.label,
        );
    }
    s.push_str(&summary(rows, min_parts));
    s
}

/// For each kind of edit, over the sheets of `min_parts` parts or more whose row count did not
/// change: how many there are, the mean and the least share of parts unmoved and of segments
/// kept, in the anchor frame and in the raw frame.
pub fn summary(rows: &[Row], min_parts: usize) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "\nsummary over sheets of {min_parts}+ parts, rows unchanged (mean / least):"
    );
    let _ = writeln!(
        s,
        "{:<32} {:>3} | {:>13} {:>13} | {:>13} {:>13} | rows changed",
        "edit", "n", "anchor parts", "anchor segs", "raw parts", "raw segs"
    );
    for kind in Kind::ALL {
        let used: Vec<&Movement> = rows
            .iter()
            .filter(|r| r.kind == kind && r.movement.size >= min_parts)
            .map(|r| &r.movement)
            .collect();
        if used.is_empty() {
            continue;
        }
        let steady: Vec<&&Movement> = used.iter().filter(|m| !m.rows_changed()).collect();
        let stat = |f: &dyn Fn(&Movement) -> f64| -> String {
            let v: Vec<f64> = steady.iter().map(|m| f(m)).collect();
            if v.is_empty() {
                return "-".into();
            }
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            let least = v.iter().copied().fold(1.0, f64::min);
            format!("{:.0}% / {:.0}%", 100.0 * mean, 100.0 * least)
        };
        let _ = writeln!(
            s,
            "({}) {:<28} {:>3} | {:>13} {:>13} | {:>13} {:>13} | {}",
            kind.letter(),
            kind.name(),
            steady.len(),
            stat(&|m| m.anchor.unmoved_fraction()),
            stat(&|m| m.anchor.segments_fraction()),
            stat(&|m| m.raw.unmoved_fraction()),
            stat(&|m| m.raw.segments_fraction()),
            used.len() - steady.len(),
        );
    }
    s
}
