//! A big board as sheets: one box for each module (and for the parts that are in none), each
//! laid out on its own, set out in a grid. A net that joins parts on more than one sheet is
//! drawn as a stub with its name at each of its pins, never as a wire across the page; ground
//! is drawn on every sheet that has it.
//!
//! A small board is not split (it stays one drawing, as [`crate::layout::layout`] makes it),
//! and a sheet with more than [`MAX_SHEET_PARTS`] parts is cut into numbered sheets along the
//! order a breadth-first walk of its connections meets the parts, so no sheet is a long thin
//! strip. The result is a pure function of the design and the groups.

use std::collections::{BTreeSet, HashMap};

use netsmith_core::{Design, DesignError, Net, PinRef};
use netsmith_kicad::{Mapping, SymbolLib};

use crate::geom::{Pt, Rect, snap_up};
use crate::layout::{
    Anchor, Layout, NetLabel, PlacedPart, PowerSymbol, Rails, TEXT_SIZE, Text, Wire,
    is_ground_name, layout_rails, layout_with, text_width,
};

/// A board with this many parts or fewer is drawn as one sheet.
pub const SHEET_THRESHOLD: usize = 16;

/// The most parts on one sheet; a bigger group is cut.
pub const MAX_SHEET_PARTS: usize = 20;

/// The widest a net may be, in pins, and still join two parts in the walk that orders a cut.
const WALK_NET_PINS: usize = 12;

const GRID: f64 = 1.27;
/// Room for the title above a sheet's drawing.
const TITLE_HEIGHT: f64 = 8.89;
/// The height of the title's letters, in millimetres, so it reads when the whole board is in view.
pub const TITLE_SIZE: f64 = 3.6;
/// Space between sheets.
const SHEET_GAP: f64 = 12.7;
/// The shape the grid aims for: width over height.
const TARGET_ASPECT: f64 = 1.6;

/// Parts that belong together, usually one module instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The module instance's name: `stage`.
    pub name: String,
    /// What it is, for a second line under the name: the module's definition.
    pub detail: String,
    /// Indices into `design.parts`.
    pub parts: Vec<usize>,
}

/// One box of a board drawn by module.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    /// The group's name; a cut sheet adds its number: `stage (2/3)`.
    pub name: String,
    pub detail: String,
    /// Indices into `design.parts`.
    pub parts: Vec<usize>,
    /// The box, title included.
    pub frame: Rect,
    pub title: Text,
}

/// Lays `design` out by `groups`. The parts no group names make a sheet of their own, named
/// as the design is. A board of [`SHEET_THRESHOLD`] parts or fewer, and a board that is one
/// sheet of [`MAX_SHEET_PARTS`] or fewer, is laid out as one drawing with no sheets.
pub fn layout_sheets(
    design: &Design,
    groups: &[Group],
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Result<Layout, DesignError> {
    layout_sheets_rails(design, groups, &Rails::default(), mapping, lib)
}

/// [`layout_sheets`] with the rails known: their pins get power symbols.
pub fn layout_sheets_rails(
    design: &Design,
    groups: &[Group],
    rails: &Rails,
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Result<Layout, DesignError> {
    design.validate()?;
    let clusters = clusters(design, groups);
    let single = clusters.len() <= 1 && design.parts.len() <= MAX_SHEET_PARTS;
    if design.parts.len() <= SHEET_THRESHOLD || single {
        return layout_rails(design, rails, mapping, lib);
    }
    let clusters = cut(design, clusters);

    let mut owner = vec![0; design.parts.len()];
    for (c, cluster) in clusters.iter().enumerate() {
        for &p in &cluster.parts {
            owner[p] = c;
        }
    }
    let part_index: HashMap<&str, usize> = design
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();

    let mut drawn: Vec<Drawn> = Vec::new();
    for cluster in &clusters {
        // This sheet's view of the design: its parts, and of each net the pins that are here.
        let in_sheet: HashMap<usize, usize> = cluster
            .parts
            .iter()
            .enumerate()
            .map(|(local, &p)| (p, local))
            .collect();
        let mut nets = Vec::new();
        let mut net_of_global = Vec::new();
        let mut external = Vec::new();
        for (n, net) in design.nets.iter().enumerate() {
            let pins: Vec<PinRef> = net
                .pins
                .iter()
                .filter(|pin| in_sheet.contains_key(&part_index[pin.refdes.as_str()]))
                .cloned()
                .collect();
            if pins.is_empty() {
                continue;
            }
            let elsewhere = net
                .pins
                .iter()
                .any(|pin| owner[part_index[pin.refdes.as_str()]] != owner[cluster.parts[0]]);
            external.push(elsewhere && !is_ground_name(&net.name));
            nets.push(Net {
                name: net.name.clone(),
                pins,
            });
            net_of_global.push(n);
        }
        let sub = Design {
            name: cluster.name.clone(),
            parts: cluster
                .parts
                .iter()
                .map(|&p| design.parts[p].clone())
                .collect(),
            nets,
            process: None,
            features: Default::default(),
        };
        let layout = layout_with(&sub, mapping, lib, &external, rails, rails.ground.len() > 1)?;
        drawn.push(Drawn {
            layout,
            parts: cluster.parts.clone(),
            nets: net_of_global,
        });
    }

    // Each sheet's box: its drawing with a strip for the title above.
    let titles: Vec<Text> = clusters
        .iter()
        .map(|c| Text {
            text: if c.detail.is_empty() {
                c.name.clone()
            } else {
                format!("{}: {}", c.name, c.detail)
            },
            at: Pt::new(0.0, 0.0),
            anchor: Anchor::Start,
            vertical: false,
        })
        .collect();
    let frames: Vec<Rect> = drawn
        .iter()
        .zip(&titles)
        .map(|(d, title)| {
            let mut frame = d.layout.bounds;
            frame.min.y -= TITLE_HEIGHT;
            frame.max.x = frame
                .max
                .x
                .max(frame.min.x + text_width(&title.text) * TITLE_SIZE / TEXT_SIZE + 4.0 * GRID);
            frame
        })
        .collect();
    let offsets = arrange(&frames);

    let mut parts: Vec<Option<PlacedPart>> = vec![None; design.parts.len()];
    let mut symbols = Vec::new();
    let mut wires = Vec::new();
    let mut junctions = Vec::new();
    let mut labels = Vec::new();
    let mut sheets = Vec::new();
    let mut bounds: Option<Rect> = None;
    for (((d, cluster), (frame, title)), by) in drawn
        .into_iter()
        .zip(&clusters)
        .zip(frames.iter().zip(titles))
        .zip(offsets)
    {
        let frame = frame.translated(by);
        bounds = Some(bounds.map_or(frame, |mut b| {
            b.union(frame);
            b
        }));
        for (local, mut placed) in d.layout.parts.into_iter().enumerate() {
            let global = d.parts[local];
            move_part(&mut placed, by);
            placed.part = global;
            for pin in &mut placed.pins {
                pin.net = pin.net.map(|n| d.nets[n]);
            }
            parts[global] = Some(placed);
        }
        for w in d.layout.wires {
            wires.push(Wire {
                net: d.nets[w.net],
                points: w.points.into_iter().map(|p| shift(p, by)).collect(),
            });
        }
        for sym in d.layout.symbols {
            symbols.push(PowerSymbol {
                net: d.nets[sym.net],
                at: shift(sym.at, by),
                label: sym.label.map(|mut l| {
                    l.at = shift(l.at, by);
                    l
                }),
                ..sym
            });
        }
        for j in d.layout.junctions {
            junctions.push(crate::layout::Junction {
                net: d.nets[j.net],
                at: shift(j.at, by),
            });
        }
        for l in d.layout.labels {
            let mut text = l.text;
            text.at = shift(text.at, by);
            labels.push(NetLabel {
                net: d.nets[l.net],
                text,
            });
        }
        sheets.push(Sheet {
            name: cluster.name.clone(),
            detail: cluster.detail.clone(),
            parts: cluster.parts.clone(),
            frame,
            title: Text {
                at: Pt::new(
                    frame.min.x + GRID * 2.0,
                    frame.min.y + TITLE_HEIGHT - GRID * 2.0,
                ),
                ..title
            },
        });
    }
    Ok(Layout {
        parts: parts
            .into_iter()
            .map(|p| p.expect("every part is on a sheet"))
            .collect(),
        symbols,
        sheets,
        wires,
        junctions,
        labels,
        bounds: bounds.unwrap_or(Rect::around(Pt::new(0.0, 0.0))),
    })
}

struct Drawn {
    layout: Layout,
    /// Sub-design part index to design part index.
    parts: Vec<usize>,
    /// Sub-design net index to design net index.
    nets: Vec<usize>,
}

fn shift(p: Pt, by: Pt) -> Pt {
    Pt::new(p.x + by.x, p.y + by.y)
}

fn move_part(part: &mut PlacedPart, by: Pt) {
    part.at = shift(part.at, by);
    part.extent = part.extent.translated(by);
    for pin in &mut part.pins {
        pin.at = shift(pin.at, by);
    }
    part.reference.at = shift(part.reference.at, by);
    if let Some(v) = &mut part.value {
        v.at = shift(v.at, by);
    }
}

/// The groups that have parts, then the parts in no group (a part named twice stays in the
/// first group that names it).
fn clusters(design: &Design, groups: &[Group]) -> Vec<Group> {
    let mut taken = vec![false; design.parts.len()];
    let mut out = Vec::new();
    for g in groups {
        let parts: Vec<usize> = g
            .parts
            .iter()
            .copied()
            .filter(|&p| p < taken.len() && !std::mem::replace(&mut taken[p], true))
            .collect();
        if !parts.is_empty() {
            out.push(Group { parts, ..g.clone() });
        }
    }
    let rest: Vec<usize> = (0..taken.len()).filter(|&p| !taken[p]).collect();
    if !rest.is_empty() {
        out.push(Group {
            name: design.name.clone(),
            detail: String::new(),
            parts: rest,
        });
    }
    out
}

/// Cuts each group of more than [`MAX_SHEET_PARTS`] parts into equal pieces along a walk of
/// its connections.
fn cut(design: &Design, clusters: Vec<Group>) -> Vec<Group> {
    let mut out = Vec::new();
    for g in clusters {
        if g.parts.len() <= MAX_SHEET_PARTS {
            out.push(g);
            continue;
        }
        let order = walk(design, &g.parts);
        let pieces = order.len().div_ceil(MAX_SHEET_PARTS);
        let size = order.len().div_ceil(pieces);
        for (i, chunk) in order.chunks(size).enumerate() {
            out.push(Group {
                name: format!("{} ({}/{})", g.name, i + 1, pieces),
                detail: g.detail.clone(),
                parts: chunk.to_vec(),
            });
        }
    }
    out
}

/// The parts of a group in the order a breadth-first walk meets them: a connector first if
/// there is one, then the lowest index; ground and wide nets do not join parts.
fn walk(design: &Design, parts: &[usize]) -> Vec<usize> {
    let index: HashMap<&str, usize> = design
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();
    let member: BTreeSet<usize> = parts.iter().copied().collect();
    let mut next: HashMap<usize, BTreeSet<usize>> = HashMap::new();
    for net in &design.nets {
        if is_ground_name(&net.name) || net.pins.len() > WALK_NET_PINS {
            continue;
        }
        let touching: BTreeSet<usize> = net
            .pins
            .iter()
            .map(|pin| index[pin.refdes.as_str()])
            .filter(|p| member.contains(p))
            .collect();
        for &a in &touching {
            next.entry(a)
                .or_default()
                .extend(touching.iter().filter(|&&b| b != a));
        }
    }
    let is_connector = |p: usize| crate::layout::is_connector(&design.parts[p].refdes);
    let mut roots: Vec<usize> = member.iter().copied().collect();
    roots.sort_by_key(|&p| (!is_connector(p), p));
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for root in roots {
        if !seen.insert(root) {
            continue;
        }
        let mut queue = vec![root];
        let mut i = 0;
        while i < queue.len() {
            let here = queue[i];
            i += 1;
            for &n in next.get(&here).into_iter().flatten() {
                if seen.insert(n) {
                    queue.push(n);
                }
            }
        }
        order.extend(queue);
    }
    order
}

/// Where each frame's top-left corner goes: rows filled from the left until a row would pass
/// the width that makes the whole roughly [`TARGET_ASPECT`] wide, each frame on the grid.
/// Returns the translation to apply to each frame.
fn arrange(frames: &[Rect]) -> Vec<Pt> {
    let area: f64 = frames
        .iter()
        .map(|f| (f.width() + SHEET_GAP) * (f.height() + SHEET_GAP))
        .sum();
    let widest = frames.iter().map(Rect::width).fold(0.0, f64::max);
    let target = (area * TARGET_ASPECT).sqrt().max(widest);

    let mut offsets = Vec::with_capacity(frames.len());
    let (mut x, mut y, mut row_height) = (0.0, 0.0, 0.0f64);
    for f in frames {
        if x > 0.0 && x + f.width() > target {
            x = 0.0;
            y += row_height + SHEET_GAP;
            row_height = 0.0;
        }
        offsets.push(Pt::new(
            snap_up(x - f.min.x, GRID),
            snap_up(y - f.min.y, GRID),
        ));
        x += f.width() + SHEET_GAP;
        row_height = row_height.max(f.height());
    }
    offsets
}
