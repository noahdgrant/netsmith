//! A placed board summed up as data (issue #773): what decides whether a placement is good,
//! in one small document that a person or an agent reads in one go, where `board3d --json` and
//! the editor's scene give the parts one by one.
//!
//! The numbers are the ones the placer and the checks use, so that a person, an agent and the
//! placer agree on what "close" means: the wire length, crossings and decoupling distances are
//! [`Quality`]'s, the decoupling pairs [`crate::decoupling_pairs`], a module's spread and its
//! intruders the placer's cohesion term (the half perimeter of the box round its parts'
//! courtyard centres, and the parts of other modules whose courtyard centre is inside that
//! box), the room a part takes on a face [`Placed::room_on`], the space kept from the edge
//! [`Rules::edge_wall`], and the findings [`Board::check_placement`]'s.
//!
//! Every place is in the layout file's frame: mm from the outline's top left corner, x to the
//! right and y down.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::board::{Board, Placed, Side};
use crate::geometry::{Pt, Rect};
use crate::quality::{Quality, crossings, decoupling_pairs};
use crate::ratsnest::Ratsnest;

/// How many of the largest free rectangles of each face are listed.
pub(crate) const FREE_RECTANGLES: usize = 5;

/// The finest side of a cell of the grid the free space is measured on, in mm. A board so large
/// that this gives more than [`MOST_CELLS`] cells on a side is measured on coarser cells.
pub(crate) const CELL: f64 = 0.25;

/// The most cells of the free-space grid along one side of the outline.
pub(crate) const MOST_CELLS: usize = 800;

/// A box in the layout file's frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SummaryBox {
    pub min: [f64; 2],
    pub max: [f64; 2],
}

/// The board as a whole.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub board: String,
    /// The outline's box, in mm.
    pub width: f64,
    pub height: f64,
    /// The space the placer keeps between a courtyard and the edge, in mm
    /// ([`Rules::edge_wall`]).
    pub edge_wall: f64,
    pub parts: usize,
    pub totals: Totals,
    /// The top face, then the bottom.
    pub faces: Vec<FaceSummary>,
    /// Each module instance with two parts or more, outer ones first, by path.
    pub modules: Vec<ModuleSummary>,
    /// Each decoupling capacitor, in board order.
    pub decoupling: Vec<DecouplingSummary>,
    /// Each net with two pads or more, by name.
    pub nets: Vec<NetSummary>,
    /// The parts near each part asked about, with [`Summary::neighbours`]; left out otherwise.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub neighbours: Vec<Neighbours>,
    /// The distance between each pair of pads asked about, with [`Summary::pad_distances`];
    /// left out otherwise.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pads: Vec<PadDistance>,
    /// What the placement check finds, as `fab` says it.
    pub findings: Vec<String>,
}

/// The board's measures of the placement as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Totals {
    /// The sum over the nets of the half perimeter of the box round the net's pads, in mm.
    pub hpwl: f64,
    /// The length of the ratsnest, in mm.
    pub ratsnest: f64,
    /// The pairs of ratsnest lines of different nets that cross.
    pub crossings: usize,
    /// The mean and the greatest distance of a decoupling capacitor from its part, in mm.
    pub decoupling_mean: f64,
    pub decoupling_max: f64,
}

/// One face of the board.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FaceSummary {
    /// `top` or `bottom`.
    pub face: &'static str,
    /// The parts on this face.
    pub parts: usize,
    /// The area a courtyard may take: inside the outline and the edge wall, outside the
    /// cutouts and the keepouts that keep parts out of this face, in mm2 (on the grid of
    /// `cell`).
    pub usable: f64,
    /// The area the parts take on this face (their courtyards, a part with a hole on both
    /// faces), counted once where they overlap, in mm2.
    pub courtyards: f64,
    /// `courtyards` over `usable`.
    pub fill: f64,
    /// The usable area no part takes, in mm2 (on the grid of `cell`).
    pub free: f64,
    /// The side of a cell of the grid `usable`, `free` and `largest_free` are measured on, in mm.
    pub cell: f64,
    /// The largest empty rectangles, largest first, none overlapping another: where a part
    /// whose courtyard fits may go on this face, as far as courtyards, cutouts, keepouts and the
    /// edge wall go (heights and holes are not counted).
    pub largest_free: Vec<SummaryBox>,
}

/// One module instance: the parts whose path starts with `path.`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModuleSummary {
    pub path: String,
    /// The parts' designators, in board order.
    pub parts: Vec<String>,
    /// The box round the parts' courtyards.
    #[serde(rename = "box")]
    pub bounds: SummaryBox,
    /// The box's area, in mm2.
    pub box_area: f64,
    /// The area the parts' courtyards take, counted once where they overlap, in mm2.
    pub courtyards: f64,
    /// The half perimeter of the box round the parts' courtyard centres, in mm: the placer's
    /// cohesion term.
    pub spread: f64,
    /// The parts of no module of this one whose courtyard centre is inside the box round the
    /// parts' courtyard centres, as the placer's cohesion term counts them, by designator.
    pub intruders: Vec<String>,
}

/// A decoupling capacitor and the part it decouples.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DecouplingSummary {
    pub cap: String,
    pub cap_path: String,
    /// The part with the nearest pin pads.
    pub part: String,
    /// The mean over the capacitor's two pads of the distance from the pad's centre to the
    /// nearest pin pad of the part on its net, in mm ([`Quality`]).
    pub distance: f64,
}

/// One net.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NetSummary {
    pub name: String,
    pub pads: usize,
    /// The half perimeter of the box round its pads, in mm.
    pub hpwl: f64,
    /// The crossings of its ratsnest lines with those of other nets.
    pub crossings: usize,
    /// Its longest ratsnest line; left out for a net all of whose lines go to its plane.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longest: Option<RatsnestLine>,
}

/// One ratsnest line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RatsnestLine {
    /// The pads, `R1.2`.
    pub from: String,
    pub to: String,
    pub length: f64,
}

/// The parts near one part.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Neighbours {
    pub part: String,
    pub path: String,
    pub face: &'static str,
    /// How far, from courtyard to courtyard, a part may be and be listed, in mm.
    pub within: f64,
    /// Nearest first.
    pub near: Vec<Neighbour>,
}

/// A part near another.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Neighbour {
    pub part: String,
    pub path: String,
    pub face: &'static str,
    /// The space between the two courtyards, in mm: 0 where they touch or overlap.
    pub gap: f64,
}

/// The distance between two pads, named `PART.PAD` (the part's path or designator, then the
/// pad's number: `stage.r1.2`, `R1.2`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PadDistance {
    /// The pads, as the designator and the number: `R1.2`.
    pub from: String,
    pub to: String,
    /// The net each pad is on, when it is on one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_net: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_net: Option<String>,
    /// From the centre of the first pad to the centre of the second, in mm.
    pub distance: f64,
    /// The way from the first pad's centre to the second's, in the layout file's frame (x to
    /// the right, y down), in mm.
    pub dx: f64,
    pub dy: f64,
    /// The space between the two pads' boxes, in mm: 0 where they touch or overlap. It does
    /// not look at the face, so two pads on opposite faces are measured as if on one.
    pub gap: f64,
}

/// A number to 0.01 (a hundredth of a mm, of a mm2, or of a fill), which keeps the document
/// small.
fn round(v: f64) -> f64 {
    let r = (v * 100.0).round() / 100.0;
    if r == 0.0 { 0.0 } else { r }
}

fn face_word(side: Side) -> &'static str {
    side.word()
}

impl Summary {
    /// The summary of a placed board.
    pub fn of(board: &Board) -> Summary {
        let origin = board.outline.min;
        let frame = |r: &Rect| SummaryBox {
            min: [round(r.min.x - origin.x), round(r.min.y - origin.y)],
            max: [round(r.max.x - origin.x), round(r.max.y - origin.y)],
        };
        let quality = Quality::of(board);
        let ratsnest = Ratsnest::of(board);
        let faces = [Side::Front, Side::Back]
            .into_iter()
            .map(|side| face(board, side, &frame))
            .collect();
        let decoupling = decoupling_pairs(&board.parts, &board.nets)
            .iter()
            .zip(&quality.decoupling)
            .map(|(pair, d)| DecouplingSummary {
                cap: d.cap.clone(),
                cap_path: board.parts[pair.cap].path.clone(),
                part: d.part.clone(),
                distance: round(d.distance),
            })
            .collect();
        let real: Vec<_> = ratsnest.lines.iter().filter(|l| !l.to_plane).collect();
        let mut nets: Vec<NetSummary> = board
            .nets
            .iter()
            .filter_map(|net| {
                let nodes = crate::ratsnest::nodes_of(&board.parts, net);
                if nodes.len() < 2 {
                    return None;
                }
                let b = Rect::around(nodes.iter().map(|n| n.centre))?;
                let own: Vec<_> = real.iter().filter(|l| l.net == net.name).collect();
                let crossings = own
                    .iter()
                    .map(|a| {
                        real.iter()
                            .filter(|b| {
                                b.net != a.net
                                    && crate::quality::segments_cross(
                                        a.from_at, a.to_at, b.from_at, b.to_at,
                                    )
                            })
                            .count()
                    })
                    .sum();
                let longest = own
                    .iter()
                    .max_by(|a, b| a.length().total_cmp(&b.length()))
                    .map(|l| RatsnestLine {
                        from: l.from.to_string(),
                        to: l.to.to_string(),
                        length: round(l.length()),
                    });
                Some(NetSummary {
                    name: net.name.clone(),
                    pads: nodes.len(),
                    hpwl: round(b.width() + b.height()),
                    crossings,
                    longest,
                })
            })
            .collect();
        nets.sort_by(|a, b| a.name.cmp(&b.name));
        Summary {
            board: board.name.clone(),
            width: round(board.outline.width()),
            height: round(board.outline.height()),
            edge_wall: round(board.rules.edge_wall()),
            parts: board.parts.len(),
            totals: Totals {
                hpwl: round(quality.hpwl),
                ratsnest: round(quality.ratsnest),
                crossings: crossings(&ratsnest.lines),
                decoupling_mean: round(quality.decoupling_mean()),
                decoupling_max: round(quality.decoupling_max()),
            },
            faces,
            modules: modules(board, &frame),
            decoupling,
            nets,
            neighbours: Vec::new(),
            pads: Vec::new(),
            findings: board
                .check_placement()
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }

    /// The summary with the parts within `within` mm (courtyard to courtyard, on either face)
    /// of each part of `parts`, which names a part by its path or its designator. A name the
    /// board has no part by is the error, with the nearest names.
    pub fn neighbours(
        mut self,
        board: &Board,
        parts: &[String],
        within: f64,
    ) -> Result<Summary, String> {
        for name in parts {
            let Some(part) = board
                .parts
                .iter()
                .find(|p| &p.path == name || &p.refdes == name)
            else {
                return Err(no_part(board, name));
            };
            let mut near: Vec<Neighbour> = board
                .parts
                .iter()
                .filter(|q| q.refdes != part.refdes)
                .filter_map(|q| {
                    let gap = courtyard_gap(part, q);
                    (gap <= within).then(|| Neighbour {
                        part: q.refdes.clone(),
                        path: q.path.clone(),
                        face: face_word(q.side),
                        gap: round(gap),
                    })
                })
                .collect();
            near.sort_by(|a, b| a.gap.total_cmp(&b.gap).then(a.part.cmp(&b.part)));
            self.neighbours.push(Neighbours {
                part: part.refdes.clone(),
                path: part.path.clone(),
                face: face_word(part.side),
                within,
                near,
            });
        }
        Ok(self)
    }

    /// The summary with the distance between each pair of pads of `pairs`, each pad named
    /// `PART.PAD`: the part by its path or its designator, then the pad's number after the last
    /// dot (`stage.r1.2`, `R1.2`). Where a footprint has several pads with the number, the
    /// nearest pair of the two is measured. A part the board has no part by, or a pad the part
    /// has none of, is the error, with the nearest names or the part's pad numbers.
    pub fn pad_distances(
        mut self,
        board: &Board,
        pairs: &[(String, String)],
    ) -> Result<Summary, String> {
        for (a, b) in pairs {
            let (ra, na, pa) = find_pads(board, a)?;
            let (rb, nb, pb) = find_pads(board, b)?;
            let mut best: Option<(f64, Pt, Pt, f64)> = None;
            for (ca, ba) in &pa {
                for (cb, bb) in &pb {
                    let d = ca.distance(*cb);
                    if best.is_none_or(|(m, ..)| d < m) {
                        best = Some((d, *ca, *cb, rect_gap(ba, bb)));
                    }
                }
            }
            let (distance, ca, cb, gap) = best.expect("find_pads gives at least one pad");
            let net_of = |refdes: &str, number: &str| {
                board
                    .nets
                    .iter()
                    .find(|n| {
                        n.pads
                            .iter()
                            .any(|p| p.refdes == refdes && p.number == number)
                    })
                    .map(|n| n.name.clone())
            };
            self.pads.push(PadDistance {
                from: format!("{ra}.{na}"),
                to: format!("{rb}.{nb}"),
                from_net: net_of(&ra, &na),
                to_net: net_of(&rb, &nb),
                distance: round(distance),
                dx: round(cb.x - ca.x),
                dy: round(cb.y - ca.y),
                gap: round(gap),
            });
        }
        Ok(self)
    }

    /// A few lines for a person.
    pub fn text(&self) -> String {
        let t = &self.totals;
        let mut text = format!(
            "{}: {} x {} mm, {} parts; wire {} mm (half perimeter), ratsnest {} mm, {} crossings, decoupling mean {} mm, max {} mm\n",
            self.board,
            self.width,
            self.height,
            self.parts,
            t.hpwl,
            t.ratsnest,
            t.crossings,
            t.decoupling_mean,
            t.decoupling_max
        );
        for f in &self.faces {
            text += &format!(
                "{}: {} parts, courtyards {} of {} mm2 ({}%), {} mm2 free",
                f.face,
                f.parts,
                f.courtyards,
                f.usable,
                round(f.fill * 100.0),
                f.free
            );
            if let Some(b) = f.largest_free.first() {
                text += &format!(
                    "; largest free rectangle {} x {} mm at ({}, {})",
                    round(b.max[0] - b.min[0]),
                    round(b.max[1] - b.min[1]),
                    b.min[0],
                    b.min[1]
                );
            }
            text.push('\n');
        }
        for m in &self.modules {
            text += &format!(
                "module {}: {} parts in {} x {} mm, spread {} mm",
                m.path,
                m.parts.len(),
                round(m.bounds.max[0] - m.bounds.min[0]),
                round(m.bounds.max[1] - m.bounds.min[1]),
                m.spread
            );
            if !m.intruders.is_empty() {
                text += &format!(", intruders {}", m.intruders.join(", "));
            }
            text.push('\n');
        }
        for d in &self.decoupling {
            text += &format!(
                "decoupling {} ({}) to {}: {} mm\n",
                d.cap, d.cap_path, d.part, d.distance
            );
        }
        for n in &self.neighbours {
            text += &format!("near {} ({}, {}):", n.part, n.path, n.face);
            if n.near.is_empty() {
                text += &format!(" nothing within {} mm", n.within);
            }
            for q in &n.near {
                text += &format!(" {} {} {} mm;", q.part, q.face, q.gap);
            }
            text.push('\n');
        }
        for p in &self.pads {
            text += &format!(
                "pads {} to {}: {} mm between centres (dx {}, dy {}), {} mm between the copper",
                p.from, p.to, p.distance, p.dx, p.dy, p.gap
            );
            let nets = match (&p.from_net, &p.to_net) {
                (Some(a), Some(b)) if a == b => format!(", both on {a}"),
                (Some(a), Some(b)) => format!(", on {a} and {b}"),
                (Some(a), None) => format!(", {} on {a}", p.from),
                (None, Some(b)) => format!(", {} on {b}", p.to),
                (None, None) => String::new(),
            };
            text += &nets;
            text.push('\n');
        }
        for f in &self.findings {
            text += &format!("finding: {f}\n");
        }
        text
    }
}

/// The part and the pads a `PART.PAD` names: the part's designator, the pad's number, and the
/// centre and box of every pad of the footprint with the number.
fn find_pads(board: &Board, name: &str) -> Result<(String, String, Vec<(Pt, Rect)>), String> {
    let Some((part, number)) = name.rsplit_once('.') else {
        return Err(format!(
            "`{name}` is not a pad: name it `PART.PAD`, the part's path or designator, a dot and the pad's number (`R1.2`)"
        ));
    };
    let Some(placed) = board
        .parts
        .iter()
        .find(|p| p.path == part || p.refdes == part)
    else {
        return Err(no_part(board, part));
    };
    let pads: Vec<(Pt, Rect)> = placed
        .footprint
        .pads_numbered(number)
        .map(|p| (placed.pad_centre(p), placed.pad_bounds(p)))
        .collect();
    if pads.is_empty() {
        return Err(format!(
            "the part `{}` has no pad `{number}`; its pads are: {}",
            placed.refdes,
            placed.footprint.pad_numbers().join(", ")
        ));
    }
    Ok((placed.refdes.clone(), number.to_string(), pads))
}

/// The error for a name the board has no part by, with the nearest names.
pub(crate) fn no_part(board: &Board, name: &str) -> String {
    let mut names: Vec<&str> = board
        .parts
        .iter()
        .flat_map(|p| [p.path.as_str(), p.refdes.as_str()])
        .collect();
    names.sort_by_key(|n| distance(n, name));
    names.dedup();
    names.truncate(3);
    format!(
        "the board `{}` has no part `{name}`; the nearest are: {}",
        board.name,
        names.join(", ")
    )
}

/// The edit distance between two names.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let here = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb))
                .min(row[j] + 1)
                .min(here + 1);
            prev = here;
        }
    }
    row[b.len()]
}

/// The space between two boxes, 0 where they touch or overlap.
fn rect_gap(a: &Rect, b: &Rect) -> f64 {
    let dx = (b.min.x - a.max.x).max(a.min.x - b.max.x).max(0.0);
    let dy = (b.min.y - a.max.y).max(a.min.y - b.max.y).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

/// The space between the courtyards of two parts, shape to shape.
fn courtyard_gap(a: &Placed, b: &Placed) -> f64 {
    let (ca, cb) = (a.courtyards(), b.courtyards());
    let ca = if ca.is_empty() {
        vec![a.courtyard()]
    } else {
        ca
    };
    let cb = if cb.is_empty() {
        vec![b.courtyard()]
    } else {
        cb
    };
    ca.iter()
        .flat_map(|x| cb.iter().map(move |y| rect_gap(x, y)))
        .fold(f64::MAX, f64::min)
}

/// The courtyard shapes of a part, or its courtyard box when the footprint lists none.
fn courts(p: &Placed) -> Vec<Rect> {
    let c = p.courtyards();
    if c.is_empty() { vec![p.courtyard()] } else { c }
}

/// The room every part takes on a face.
fn rooms(board: &Board, side: Side) -> Vec<Rect> {
    board
        .parts
        .iter()
        .flat_map(|p| p.room_on(side, board.rules.clearance, &courts(p)))
        .filter(|r| r.width() > 0.0 && r.height() > 0.0)
        .collect()
}

/// A grid of cells over the outline's box, each free or not.
struct Grid {
    origin: Pt,
    cell: f64,
    columns: usize,
    rows: usize,
    free: Vec<bool>,
}

impl Grid {
    /// The cells of `outline` that are at least `wall` inside `edge`, all free.
    fn inside(board: &Board, wall: f64) -> Grid {
        let outline = board.outline;
        let longest = outline.width().max(outline.height()).max(CELL);
        let cell = CELL.max(longest / MOST_CELLS as f64);
        let columns = (outline.width() / cell).floor().max(0.0) as usize;
        let rows = (outline.height() / cell).floor().max(0.0) as usize;
        let edge = board.edge();
        let mut free = vec![false; columns * rows];
        for r in 0..rows {
            for c in 0..columns {
                let min = Pt::new(
                    outline.min.x + c as f64 * cell,
                    outline.min.y + r as f64 * cell,
                );
                let cell_box = Rect::new(min, Pt::new(min.x + cell, min.y + cell));
                free[r * columns + c] = edge.box_depth(&cell_box) >= wall - 1e-9;
            }
        }
        Grid {
            origin: outline.min,
            cell,
            columns,
            rows,
            free,
        }
    }

    /// Mark every cell that `r` covers, even in part, as taken.
    fn take(&mut self, r: &Rect) {
        let lo = |v: f64, o: f64| ((v - o) / self.cell + 1e-9).floor().max(0.0) as usize;
        let hi = |v: f64, o: f64| ((v - o) / self.cell - 1e-9).ceil().max(0.0) as usize;
        let (c0, c1) = (
            lo(r.min.x, self.origin.x),
            hi(r.max.x, self.origin.x).min(self.columns),
        );
        let (r0, r1) = (
            lo(r.min.y, self.origin.y),
            hi(r.max.y, self.origin.y).min(self.rows),
        );
        for row in r0..r1 {
            for c in c0..c1 {
                self.free[row * self.columns + c] = false;
            }
        }
    }

    fn area(&self) -> f64 {
        self.free.iter().filter(|f| **f).count() as f64 * self.cell * self.cell
    }

    /// The largest rectangle of free cells, as (column, row, columns, rows), the first of the
    /// largest from the top left on a tie; `None` when no cell is free.
    fn largest(&self) -> Option<(usize, usize, usize, usize)> {
        let mut heights = vec![0usize; self.columns];
        let mut best: Option<(usize, (usize, usize, usize, usize))> = None;
        for row in 0..self.rows {
            for (c, h) in heights.iter_mut().enumerate() {
                *h = if self.free[row * self.columns + c] {
                    *h + 1
                } else {
                    0
                };
            }
            // The largest rectangle under the histogram of this row, by a stack.
            let mut stack: Vec<usize> = Vec::new();
            for c in 0..=self.columns {
                let h = if c < self.columns { heights[c] } else { 0 };
                while let Some(&top) = stack.last() {
                    if heights[top] <= h {
                        break;
                    }
                    stack.pop();
                    let height = heights[top];
                    let left = stack.last().map_or(0, |&l| l + 1);
                    let width = c - left;
                    let area = width * height;
                    if area > 0 && best.is_none_or(|(a, _)| area > a) {
                        best = Some((area, (left, row + 1 - height, width, height)));
                    }
                }
                stack.push(c);
            }
        }
        best.map(|(_, r)| r)
    }

    fn rect(&self, (c, r, w, h): (usize, usize, usize, usize)) -> Rect {
        let min = Pt::new(
            self.origin.x + c as f64 * self.cell,
            self.origin.y + r as f64 * self.cell,
        );
        Rect::new(
            min,
            Pt::new(min.x + w as f64 * self.cell, min.y + h as f64 * self.cell),
        )
    }
}

fn face(board: &Board, side: Side, frame: &dyn Fn(&Rect) -> SummaryBox) -> FaceSummary {
    let mut grid = Grid::inside(board, board.rules.edge_wall());
    for (_, shape) in board.cutout_shapes() {
        grid.take(&shape.rect);
    }
    for k in board.keepout_areas() {
        if k.forbids_parts_on(side) {
            grid.take(&k.rect);
        }
    }
    let usable = grid.area();
    let rooms = rooms(board, side);
    for r in &rooms {
        grid.take(r);
    }
    let free = grid.area();
    let mut largest_free = Vec::new();
    for _ in 0..FREE_RECTANGLES {
        let Some(cells) = grid.largest() else {
            break;
        };
        let r = grid.rect(cells);
        grid.take(&r);
        largest_free.push(frame(&r));
    }
    let courtyards = Rect::union_area(&rooms);
    FaceSummary {
        face: face_word(side),
        parts: board.parts.iter().filter(|p| p.side == side).count(),
        usable: round(usable),
        courtyards: round(courtyards),
        fill: if usable > 0.0 {
            round(courtyards / usable)
        } else {
            0.0
        },
        free: round(free),
        cell: grid.cell,
        largest_free,
    }
}

/// The module instances with two parts or more: every leading part of a part's path but its
/// own name.
fn modules(board: &Board, frame: &dyn Fn(&Rect) -> SummaryBox) -> Vec<ModuleSummary> {
    let mut members: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, p) in board.parts.iter().enumerate() {
        let segments: Vec<&str> = p.path.split('.').collect();
        for n in 1..segments.len() {
            members.entry(segments[..n].join(".")).or_default().push(i);
        }
    }
    members
        .into_iter()
        .filter(|(_, m)| m.len() >= 2)
        .map(|(path, m)| {
            let inside: BTreeSet<usize> = m.iter().copied().collect();
            let shapes: Vec<Rect> = m.iter().flat_map(|&i| courts(&board.parts[i])).collect();
            let bounds = shapes.iter().skip(1).fold(shapes[0], |b, r| b.union(r));
            let centres = Rect::around(m.iter().map(|&i| board.parts[i].courtyard().centre()))
                .expect("a module has parts");
            let intruders = board
                .parts
                .iter()
                .enumerate()
                .filter(|(j, _)| !inside.contains(j))
                .filter(|(_, q)| {
                    let c = q.courtyard().centre();
                    c.x > centres.min.x
                        && c.x < centres.max.x
                        && c.y > centres.min.y
                        && c.y < centres.max.y
                })
                .map(|(_, q)| q.refdes.clone())
                .collect();
            ModuleSummary {
                parts: m.iter().map(|&i| board.parts[i].refdes.clone()).collect(),
                bounds: frame(&bounds),
                box_area: round(bounds.width() * bounds.height()),
                courtyards: round(Rect::union_area(&shapes)),
                spread: round(centres.width() + centres.height()),
                intruders,
                path,
            }
        })
        .collect()
}

/// Where a part is in one placement, in the layout file's frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Place {
    pub x: f64,
    pub y: f64,
    pub turn: f64,
    pub side: &'static str,
}

/// A part that is not where it was.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Moved {
    pub path: String,
    pub refdes: String,
    pub from: Place,
    pub to: Place,
    /// How far its origin moved, in mm.
    pub distance: f64,
}

/// What changed from one placement of a board to another (issue #773).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacementComparison {
    pub board: String,
    /// The parts that moved, turned or changed face, by path.
    pub moved: Vec<Moved>,
    /// The parts of one placement that the other does not have, by path.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub only_before: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub only_after: Vec<String>,
    /// The findings of the second placement that the first does not have, and those of the
    /// first that are gone.
    pub appeared: Vec<String>,
    pub gone: Vec<String>,
    pub before: Totals,
    pub after: Totals,
}

impl PlacementComparison {
    /// What changed from `before` to `after`. A move of less than a micrometre is no move.
    pub fn between(before: &Board, after: &Board) -> PlacementComparison {
        let place = |b: &Board, p: &Placed| Place {
            x: round(p.at.x - b.outline.min.x),
            y: round(p.at.y - b.outline.min.y),
            turn: p.rotation,
            side: face_word(p.side),
        };
        // A part is the same part by its path, or by its designator where it has no path.
        let key = |p: &Placed| {
            if p.path.is_empty() {
                p.refdes.clone()
            } else {
                p.path.clone()
            }
        };
        let mut moved = Vec::new();
        let mut only_before = Vec::new();
        for p in &before.parts {
            match after.parts.iter().find(|q| key(q) == key(p)) {
                Some(q) => {
                    let (from, to) = (p.at - before.outline.min, q.at - after.outline.min);
                    let distance = from.distance(to);
                    if distance >= 1e-3 || p.rotation != q.rotation || p.side != q.side {
                        moved.push(Moved {
                            path: key(p),
                            refdes: q.refdes.clone(),
                            from: place(before, p),
                            to: place(after, q),
                            distance: round(distance),
                        });
                    }
                }
                None => only_before.push(key(p)),
            }
        }
        moved.sort_by(|a, b| a.path.cmp(&b.path));
        only_before.sort();
        let mut only_after: Vec<String> = after
            .parts
            .iter()
            .filter(|q| !before.parts.iter().any(|p| key(p) == key(q)))
            .map(key)
            .collect();
        only_after.sort();
        let findings = |b: &Board| -> Vec<String> {
            b.check_placement()
                .iter()
                .map(ToString::to_string)
                .collect()
        };
        let (old, new) = (findings(before), findings(after));
        PlacementComparison {
            board: after.name.clone(),
            moved,
            only_before,
            only_after,
            appeared: new.iter().filter(|f| !old.contains(f)).cloned().collect(),
            gone: old.iter().filter(|f| !new.contains(f)).cloned().collect(),
            before: Summary::of(before).totals,
            after: Summary::of(after).totals,
        }
    }

    /// A few lines for a person.
    pub fn text(&self) -> String {
        let mut text = format!(
            "{}: {} parts moved; wire {} -> {} mm, crossings {} -> {}, decoupling max {} -> {} mm\n",
            self.board,
            self.moved.len(),
            self.before.hpwl,
            self.after.hpwl,
            self.before.crossings,
            self.after.crossings,
            self.before.decoupling_max,
            self.after.decoupling_max
        );
        for m in &self.moved {
            text += &format!(
                "moved {} ({}): ({}, {}) {} {} -> ({}, {}) {} {}, {} mm\n",
                m.refdes,
                m.path,
                m.from.x,
                m.from.y,
                m.from.turn,
                m.from.side,
                m.to.x,
                m.to.y,
                m.to.turn,
                m.to.side,
                m.distance
            );
        }
        for p in &self.only_before {
            text += &format!("only before: {p}\n");
        }
        for p in &self.only_after {
            text += &format!("only after: {p}\n");
        }
        for f in &self.appeared {
            text += &format!("appeared: {f}\n");
        }
        for f in &self.gone {
            text += &format!("gone: {f}\n");
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::tests::led_board;

    #[test]
    fn a_grid_finds_the_largest_free_rectangle() {
        let mut board = led_board();
        board.parts.clear();
        board.cutouts.clear();
        board.keepouts.clear();
        board.polygon = None;
        board.corner_radius = 0.0;
        board.outline = Rect::new(Pt::new(10.0, 10.0), Pt::new(20.0, 16.0));
        let mut grid = Grid::inside(&board, 0.0);
        assert_eq!(grid.area(), 60.0);
        // A wall across the middle leaves the wider of the two sides largest.
        grid.take(&Rect::new(Pt::new(13.0, 10.0), Pt::new(14.0, 16.0)));
        let largest = grid.rect(grid.largest().unwrap());
        assert_eq!(largest, Rect::new(Pt::new(14.0, 10.0), Pt::new(20.0, 16.0)));
        assert_eq!(grid.area(), 54.0);
    }

    #[test]
    fn rect_gaps_are_zero_when_boxes_touch_and_diagonal_at_corners() {
        let r = |x0, y0, x1, y1| Rect::new(Pt::new(x0, y0), Pt::new(x1, y1));
        assert_eq!(
            rect_gap(&r(0.0, 0.0, 1.0, 1.0), &r(1.0, 0.0, 2.0, 1.0)),
            0.0
        );
        assert_eq!(
            rect_gap(&r(0.0, 0.0, 1.0, 1.0), &r(3.0, 0.5, 4.0, 1.0)),
            2.0
        );
        assert_eq!(
            rect_gap(&r(0.0, 0.0, 1.0, 1.0), &r(4.0, 5.0, 5.0, 6.0)),
            5.0
        );
    }

    #[test]
    fn the_totals_are_the_quality_of_the_board_and_the_faces_add_up() {
        let board = led_board();
        let quality = Quality::of(&board);
        let summary = Summary::of(&board);
        assert_eq!(summary.totals.hpwl, round(quality.hpwl));
        assert_eq!(
            summary.totals.crossings,
            crossings(&Ratsnest::of(&board).lines)
        );
        assert_eq!(summary.parts, board.parts.len());
        let top = &summary.faces[0];
        assert_eq!(top.face, "top");
        assert!(top.free <= top.usable && top.courtyards > 0.0, "{top:?}");
        // No free rectangle overlaps a courtyard on its face.
        for free in &top.largest_free {
            let r = Rect::new(
                Pt::new(free.min[0], free.min[1]) + board.outline.min,
                Pt::new(free.max[0], free.max[1]) + board.outline.min,
            )
            .expanded(-1e-6);
            for room in rooms(&board, Side::Front) {
                assert!(!r.overlaps(&room), "{r:?} over {room:?}");
            }
        }
        // Every net of the summary has two pads or more.
        assert!(summary.nets.iter().all(|n| n.pads >= 2));
    }

    #[test]
    fn a_difference_lists_the_parts_that_moved_and_how_far() {
        let before = led_board();
        let mut after = before.clone();
        after.parts[0].at = after.parts[0].at + Pt::new(3.0, 4.0);
        let d = PlacementComparison::between(&before, &after);
        assert_eq!(d.moved.len(), 1);
        assert_eq!(d.moved[0].refdes, before.parts[0].refdes);
        assert_eq!(d.moved[0].distance, 5.0);
        assert!(
            PlacementComparison::between(&before, &before)
                .moved
                .is_empty()
        );
    }

    #[test]
    fn two_pads_are_measured_centre_to_centre_and_copper_to_copper() {
        let board = led_board();
        let (a, b) = {
            let net = board.nets.iter().find(|n| n.pads.len() == 2).unwrap();
            let p = |i: usize| format!("{}.{}", net.pads[i].refdes, net.pads[i].number);
            (p(0), p(1))
        };
        let summary = Summary::of(&board)
            .pad_distances(&board, &[(a.clone(), b.clone())])
            .unwrap();
        let d = &summary.pads[0];
        assert_eq!((d.from.as_str(), d.to.as_str()), (a.as_str(), b.as_str()));
        assert_eq!(d.from_net, d.to_net);
        assert!(d.from_net.is_some());
        assert!((d.distance - d.dx.hypot(d.dy)).abs() < 0.02, "{d:?}");
        assert!(d.gap <= d.distance, "{d:?}");
        // The other way round: the same distance, the way reversed.
        let back = Summary::of(&board)
            .pad_distances(&board, &[(b, a)])
            .unwrap();
        assert_eq!(back.pads[0].distance, d.distance);
        assert_eq!(back.pads[0].dx, round(-d.dx));
        // A pad is no distance from itself.
        let same = Summary::of(&board)
            .pad_distances(&board, &[(d.from.clone(), d.from.clone())])
            .unwrap();
        assert_eq!(same.pads[0].distance, 0.0);
    }

    #[test]
    fn a_pad_is_named_by_path_or_designator_and_a_wrong_name_is_refused() {
        let board = led_board();
        let part = &board.parts[0];
        let number = part.footprint.pad_numbers()[0].to_string();
        let by_path = format!("{}.{number}", part.path);
        let by_refdes = format!("{}.{number}", part.refdes);
        let measure = |a: &str, b: &str| {
            Summary::of(&board).pad_distances(&board, &[(a.to_string(), b.to_string())])
        };
        assert_eq!(measure(&by_path, &by_refdes).unwrap().pads[0].distance, 0.0);
        let e = measure(&by_refdes, "nothing.1").unwrap_err();
        assert!(e.contains("no part `nothing`"), "{e}");
        let e = measure(&by_refdes, &format!("{}.999", part.refdes)).unwrap_err();
        assert!(
            e.contains("has no pad `999`") && e.contains("its pads are"),
            "{e}"
        );
        let e = measure(&by_refdes, "R1").unwrap_err();
        assert!(e.contains("is not a pad"), "{e}");
    }

    #[test]
    fn neighbours_are_nearest_first_and_a_wrong_name_is_refused() {
        let board = led_board();
        let first = board.parts[0].refdes.clone();
        let summary = Summary::of(&board)
            .neighbours(&board, std::slice::from_ref(&first), 100.0)
            .unwrap();
        let near = &summary.neighbours[0].near;
        assert_eq!(near.len(), board.parts.len() - 1);
        assert!(near.windows(2).all(|w| w[0].gap <= w[1].gap));
        let e = Summary::of(&board)
            .neighbours(&board, &["nothing".to_string()], 1.0)
            .unwrap_err();
        assert!(e.contains("no part `nothing`"), "{e}");
    }
}
