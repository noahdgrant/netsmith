//! Placement: where each footprint goes (ADR 0033).
//!
//! A placement is derived from the circuit alone, by [`crate::anneal`], which moves and
//! turns the footprints to lower a cost made of wire length over pad centres, module
//! cohesion, decoupling distance, connectors at edges, silkscreen clearance and the
//! courtyard, outline and keepout rules. The same design, options and seed always give the
//! same board.
//!
//! The annealer starts from a simple arrangement made here: connectors in a column on the
//! left edge, then the right edge, alternating, every other part in rows between them
//! (parts of one module together), and the three blocks centred on each other. This also
//! decides whether the parts fit a given size ([`PlaceOptions::fit`]): rows that do not fit
//! are not helped by moving parts closer, so the parts are shared between the faces by area,
//! largest first, and if they still do not fit the placer says by how much.
//!
//! Parts can be on either face of the board ([`PlaceOptions::sides`]). The back is laid out
//! in the same area as the front, but a part with a hole in it (a through-hole part, a
//! mounting hole) is on both faces, so the other face keeps clear of it. Parts a layout file
//! puts somewhere ([`PlaceOptions::fixed`]) stay there and the rest are placed round them.
//! [`place_subset`] places some parts of a board that is placed already and leaves the rest.

use std::collections::{BTreeMap, VecDeque};

use crate::anneal::{self, Ended, Outcome, Problem};
use crate::board::{Board, BoardNet, Placed, ReferenceSide, Side};
use crate::constraints::Want;
use crate::geometry::{Pt, Rect};
use crate::overhang::{self, Overhang};
use crate::rules::Rules;

/// The outline's top left corner on the sheet, so the board sits inside a KiCad page.
pub const ORIGIN: Pt = Pt { x: 20.0, y: 20.0 };

/// Space kept between courtyards side by side.
const GAP_X: f64 = 1.5;
/// Space kept between courtyards above one another: room for the reference text.
const GAP_Y: f64 = 2.5;
/// Space between the connector columns and the rows of parts.
const BLOCK_GAP: f64 = 3.0;
/// A row of parts is filled up to this width, or to the side of a square with the area of
/// all the parts if that is more, so that a board with many parts grows in both directions.
const ROW_WIDTH: f64 = 30.0;

/// What is known about where the parts go, besides the design.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaceOptions {
    /// Parts that are to be on a given face, by reference designator. The placer puts the
    /// others where it likes: all on the front, unless `fit` says they must share.
    pub sides: BTreeMap<String, Side>,
    /// The size the board must be, width and height in mm. The parts are laid out inside it;
    /// if they do not fit, there is an error saying by how much. `None` sizes the board to
    /// the parts.
    pub fit: Option<(f64, f64)>,
    /// Boxes on the board, in the same coordinates as the board (its top left corner is
    /// [`ORIGIN`]), that no courtyard touches, on both faces: holes that are not parts,
    /// cutouts, keepouts that forbid parts. Without a size to fit the board is made to hold
    /// them.
    pub keepouts: Vec<Rect>,
    /// The cutouts among `keepouts`: besides the courtyards, no pad of a part is nearer to
    /// one than the process's distance from copper to an edge, as the placement check asks.
    pub cutouts: Vec<Rect>,
    /// Parts that stay where they are, by reference designator: the entries of a layout file
    /// (ADR 0032). The others are placed round them.
    pub fixed: BTreeMap<String, Fixed>,
    /// Which arrangement the placer finds: the same seed always gives the same one, another
    /// seed gives another (the layout file's `seed`). 0 is the default.
    pub seed: u64,
    /// What the design's requirements on positions leave to the placer for the parts they do
    /// not pin, by reference designator (ADR 0045). The placer keeps each part inside its
    /// want. A part they pin is in `fixed`.
    pub wants: BTreeMap<String, Want>,
    /// The relations between two parts that the design requires (ADR 0045, section 3). The
    /// annealer keeps each inside its window; the two parts move together.
    pub offsets: Vec<crate::constraints::PlanOffset>,
    /// The parts the person said may hang over the board edge, by reference designator
    /// (ADR 0045, "Built: overhang"): a part that stays in `fixed` is counted in the fit test
    /// as its courtyard clipped to the outline, and the others keep their copper inside.
    pub overhang: BTreeMap<String, Overhang>,
    /// The space kept between a courtyard and the board edge: `Rules::edge_wall` of the
    /// board's process, which `Board::from_design_placed` fills in. `None`: 2.0 mm
    /// ([`Rules::DEFAULT_EDGE_WALL`]).
    pub wall: Option<f64>,
    /// The areas that limit how tall a part may stand, in the board's coordinates (ADR 0045,
    /// "Height limits", PROVISIONAL). A part with a stated height above a zone's limit keeps
    /// its body (grown by a small margin; its courtyard if the footprint draws no fabrication
    /// outline) out of the zone on the faces the zone limits; a part with no stated
    /// height is not kept out (nobody knows whether it fits; the check reports it).
    pub height_zones: Vec<HeightZone>,
}

/// An area that limits the height of the parts standing in it, on one or both faces.
#[derive(Debug, Clone, PartialEq)]
pub struct HeightZone {
    /// The keepout it comes from.
    pub name: String,
    pub rect: Rect,
    /// The faces it limits.
    pub front: bool,
    pub back: bool,
    /// The tallest a part may stand in it, in mm.
    pub limit: f64,
}

impl HeightZone {
    /// Whether a part of `height` mm on `side` may not stand in the zone.
    pub(crate) fn rules_out(&self, height: f64, side: Side) -> bool {
        let on = if side == Side::Back {
            self.back
        } else {
            self.front
        };
        on && height > self.limit + 0.0005
    }
}

impl PlaceOptions {
    /// The space kept between a courtyard and the board edge.
    pub(crate) fn wall(&self) -> f64 {
        self.wall.unwrap_or(Rules::DEFAULT_EDGE_WALL)
    }
}

/// Where a part stays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fixed {
    /// The footprint's origin in mm from the outline's top left corner, y down.
    pub at: Pt,
    /// Degrees, counterclockwise on screen.
    pub rotation: f64,
    pub side: Side,
    /// Which side of the part its reference designator is written on.
    pub text: ReferenceSide,
}

/// What the placer needs of the board besides the parts and the options.
pub(crate) struct Context<'a> {
    pub rules: &'a Rules,
    /// The nets poured as planes on inner layers.
    pub planes: &'a [String],
    pub corner_radius: f64,
    /// Whether the board has a pour on an outer layer.
    pub pours: bool,
}

/// The parts do not fit the size the board was given.
#[derive(Debug, Clone, PartialEq)]
pub struct DoesNotFit {
    /// What the parts need, with the margin, and what the outline is.
    pub needs: (f64, f64),
    pub outline: (f64, f64),
    /// What the placer ended with when it tried the outline anyway: the rows of courtyards
    /// are only where it starts, so too little room is only known by trying. `None` where
    /// it did not try.
    pub ended: Option<Box<Ended>>,
    /// The space kept between a courtyard and the board edge, in mm.
    pub wall: f64,
    /// The parts whose courtyard is too big for the outline in every turn they may take. The
    /// rows' size in `needs` says nothing about these (the rows are not the least the parts
    /// need), so when there are any the message names them instead of giving `needs`.
    pub oversize: Vec<Oversize>,
    /// The sum of the courtyards' areas in mm2 and what both faces of the outline hold inside
    /// the margin. Where the first is the greater the parts cannot fit, whatever the placer
    /// does; where it is not, the sizes in `needs` are only where the rows start (ADR 0033,
    /// PROVISIONAL), and the message does not give them.
    pub area: (f64, f64),
    /// Every part, where the placer ended with it and in what state, in the order of the
    /// design: for the full list [`DoesNotFit::full_report`] writes.
    pub parts: Box<[PartState]>,
    /// The requirements of the physical file that are not met where the placer ended, and
    /// the lines that cannot all hold where that is proved (PROVISIONAL, ADR 0045). Filled
    /// by `Board::from_design_placed`, which knows the design; `None` before, and for a
    /// design that states no requirement.
    pub requirements: Option<Box<crate::explain::RequirementReport>>,
    /// How many times the placer tried before it gave up: the first run, then runs from the
    /// same start with a large part put in a corner first and with seeds derived from the
    /// given one (B2 and B5, PROVISIONAL). The state above is that of the closest.
    pub tries: usize,
}

/// One part's place and state at the end of a placement that did not fit.
#[derive(Debug, Clone, PartialEq)]
pub struct PartState {
    pub part: String,
    pub kind: String,
    pub side: Side,
    /// The footprint's origin in mm from the outline's top left corner, y down.
    pub at: (f64, f64),
    /// Degrees, counterclockwise on screen.
    pub rotation: f64,
    /// The courtyard's width and height as it lies.
    pub size: (f64, f64),
    /// Whether a layout file or the design's requirements pin it, so the placer did not move it.
    pub fixed: bool,
    /// What is wrong with it, as sentences with the numbers; empty if it is where it should be.
    pub state: Vec<String>,
}

/// A part that does not fit the outline in any turn it may take.
#[derive(Debug, Clone, PartialEq)]
pub struct Oversize {
    pub part: String,
    pub kind: String,
    /// Each turn tried, in degrees, with the courtyard's width and height in that turn.
    pub tried: Vec<(f64, (f64, f64))>,
    /// Whether the turn is the part's own to choose; if not, a layout file or a requirement
    /// fixed it (or limited it to the turns tried).
    pub free: bool,
}

impl std::fmt::Display for Oversize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sizes: Vec<String> = self
            .tried
            .iter()
            .map(|(angle, (w, h))| format!("{w:.1} x {h:.1} mm turned {angle} degrees"))
            .collect();
        write!(
            f,
            "{} ({}) has a courtyard of {}",
            self.part,
            self.kind,
            sizes.join(", ")
        )?;
        if !self.free {
            write!(
                f,
                " (only the turns a layout file or a requirement leave it were tried)"
            )?;
        }
        Ok(())
    }
}

impl std::fmt::Display for DoesNotFit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_refusal(f)?;
        if let Some(report) = &self.requirements {
            write!(f, "\n{}", report.render("  "))?;
        }
        Ok(())
    }
}

impl DoesNotFit {
    fn write_refusal(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.oversize.is_empty() {
            let wall = (self.wall * 100.0).round() / 100.0;
            let inside = |v: f64| (v - 2.0 * self.wall).max(0.0);
            write!(
                f,
                "the outline is {:.1} x {:.1} mm, which leaves {:.1} x {:.1} mm for a courtyard inside its {} mm margin, and a part is bigger in every turn it may take: ",
                self.outline.0,
                self.outline.1,
                inside(self.outline.0),
                inside(self.outline.1),
                wall
            )?;
            let parts: Vec<String> = self.oversize.iter().map(ToString::to_string).collect();
            write!(f, "{}", parts.join("; "))?;
            if let Some(e) = &self.ended {
                write_ended(f, e, self.wall)?;
            }
            return self.write_tries(f);
        }
        let certain = self.area.0 > self.area.1 + 1e-9;
        let Some(e) = &self.ended else {
            // Nothing was tried: all there is to say is what the rows need.
            return self.write_needs(f);
        };
        let names = e.names();
        if certain {
            // The courtyards alone are more than the two faces hold: a certain no, and the
            // size the rows need is a fair measure of by how much.
            self.write_needs(f)?;
            write!(
                f,
                "; the courtyards add up to {:.0} mm2 and both faces of the outline hold {:.0} mm2 inside its margin; ",
                self.area.0, self.area.1
            )?;
        }
        write!(
            f,
            "{} part(s) do not fit the outline of {:.1} x {:.1} mm: {}",
            names.len(),
            self.outline.0,
            self.outline.1,
            e.compact()
        )?;
        write_ended(f, e, self.wall)?;
        self.write_tries(f)
    }
}

impl DoesNotFit {
    /// How many tries were made, when there was more than one.
    fn write_tries(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.tries > 1 {
            write!(
                f,
                "\n  the placer made {} tries (the first, then {} more from other seeds derived from the given one, one of them with the largest parts put in corners first); this is the closest",
                self.tries,
                self.tries - 1
            )?;
        }
        Ok(())
    }

    /// The sentence of what the rows need, for a refusal that is certain whatever the
    /// placer does.
    fn write_needs(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (over_x, over_y) = (self.needs.0 - self.outline.0, self.needs.1 - self.outline.1);
        let mut over = Vec::new();
        if over_x > 0.0 {
            over.push(format!("{over_x:.1} mm over in x"));
        }
        if over_y > 0.0 {
            over.push(format!("{over_y:.1} mm over in y"));
        }
        write!(
            f,
            "the parts need {:.1} x {:.1} mm (courtyards and {} mm margin, on both faces) and the outline is {:.1} x {:.1} mm",
            self.needs.0,
            self.needs.1,
            (self.wall * 100.0).round() / 100.0,
            self.outline.0,
            self.outline.1,
        )?;
        if !over.is_empty() {
            write!(f, ": {}", over.join(", "))?;
        }
        Ok(())
    }

    /// Every part's final place and state, for the file `fab` writes beside the board's
    /// output when the parts do not fit (`placement-failure.txt`).
    pub fn full_report(&self) -> String {
        let mut text = format!(
            "{self}\n\nEvery part where the placer ended with it, for the outline of {:.1} x {:.1} mm (the origin of each footprint is in mm from the outline's top left corner, y down; the courtyard is as it lies; `ok` means no rule the placer keeps is broken by the part):\n",
            self.outline.0, self.outline.1
        );
        for p in &self.parts {
            let state = if p.state.is_empty() {
                "ok".to_string()
            } else {
                p.state.join("; ")
            };
            text.push_str(&format!(
                "{} ({}) {} at ({:.2}, {:.2}) turned {} courtyard {:.2} x {:.2} mm{}: {}\n",
                p.part,
                p.kind,
                if p.side == Side::Back {
                    "bottom"
                } else {
                    "top"
                },
                p.at.0,
                p.at.1,
                p.rotation,
                p.size.0,
                p.size.1,
                if p.fixed { " (not moved)" } else { "" },
                state
            ));
        }
        text
    }
}

/// What the annealer ended with, after the sentence that says the parts do not fit.
fn write_ended(f: &mut std::fmt::Formatter<'_>, e: &Ended, wall: f64) -> std::fmt::Result {
    let mut left = Vec::new();
    if e.overlaps > 0 {
        left.push(format!("{} pair(s) of courtyards overlapping", e.overlaps));
    }
    if e.outside > 0 {
        left.push(format!(
            "{} part(s) outside the outline or inside its {} mm margin",
            e.outside,
            (wall * 100.0).round() / 100.0
        ));
    }
    if e.silk > 0 {
        left.push(format!(
            "{} pair(s) with reference text or silkscreen over the other's pads",
            e.silk
        ));
    }
    if !e.relations.is_empty() {
        left.push(format!(
            "{} relation(s) between parts not held",
            e.relations.len()
        ));
    }
    if left.is_empty() {
        left.push("a rule broken that no count names".to_string());
    }
    write!(
        f,
        "; placing them in the outline anyway ended with {}",
        left.join(", ")
    )?;
    write_offenders(f, e)?;
    Ok(())
}

/// How many offenders are described one by one; the rest are only named.
const DESCRIBED: usize = 10;

/// The pairs of courtyards that overlap and the parts that are outside, the first
/// [`DESCRIBED`] of each (the worst first) with the area and the side of the outline they are
/// at, and the rest by name, so the whole list is in the message.
fn write_offenders(f: &mut std::fmt::Formatter<'_>, e: &Ended) -> std::fmt::Result {
    if !e.overlapping.is_empty() {
        write!(
            f,
            "\n  courtyards overlapping (the most overlapped first, positions from the outline's top left corner):"
        )?;
        for o in e.overlapping.iter().take(DESCRIBED) {
            write!(
                f,
                "\n    {} and {}: {} mm2 ({:.2} x {:.2} mm) at ({:.1}, {:.1}), {:.1} mm from the {} edge",
                o.a,
                o.b,
                anneal::short(o.area),
                o.size.0,
                o.size.1,
                o.at.0,
                o.at.1,
                o.nearest.1,
                o.nearest.0
            )?;
        }
        if e.overlapping.len() > DESCRIBED {
            let rest: Vec<String> = e.overlapping[DESCRIBED..]
                .iter()
                .map(|o| format!("{}/{}", o.a, o.b))
                .collect();
            write!(f, "\n    and {} more: {}", rest.len(), rest.join(", "))?;
        }
    }
    for r in &e.relations {
        write!(f, "\n  relation not held: {r}")?;
    }
    if !e.outside_parts.is_empty() {
        write!(
            f,
            "\n  parts outside the outline or inside its margin (the furthest first):"
        )?;
        for o in e.outside_parts.iter().take(DESCRIBED) {
            write!(f, "\n    {}: {}", o.part, o.why.join("; "))?;
        }
        if e.outside_parts.len() > DESCRIBED {
            let rest: Vec<&str> = e.outside_parts[DESCRIBED..]
                .iter()
                .map(|o| o.part.as_str())
                .collect();
            write!(f, "\n    and {} more: {}", rest.len(), rest.join(", "))?;
        }
    }
    Ok(())
}

/// Connectors are found by the name of the kind of part.
pub(crate) fn is_connector(kind: &str) -> bool {
    kind.starts_with("Header") || kind.starts_with("Socket") || kind.starts_with("Connector")
}

/// A box of courtyards laid out relative to its own top left corner.
struct Block {
    width: f64,
    height: f64,
    /// For each part: its index, and where its courtyard's top left corner goes.
    slots: Vec<(usize, Pt)>,
}

fn size_of(parts: &[Placed], index: usize) -> (f64, f64) {
    let c = parts[index].courtyard();
    (c.width(), c.height())
}

/// Parts one above the other, centred on a common line.
fn column(parts: &[Placed], members: &[usize]) -> Block {
    let width = members
        .iter()
        .map(|&i| size_of(parts, i).0)
        .fold(0.0, f64::max);
    let mut y = 0.0;
    let mut slots = Vec::new();
    for &i in members {
        let (w, h) = size_of(parts, i);
        slots.push((i, Pt::new((width - w) / 2.0, y)));
        y += h + GAP_Y;
    }
    Block {
        width,
        height: (y - GAP_Y).max(0.0),
        slots,
    }
}

/// The width a row of `members` is filled up to: `ROW_WIDTH`, or the side of a square with
/// the area of all the parts if that is more.
fn row_width_for(parts: &[Placed], members: &[usize]) -> f64 {
    let area: f64 = members
        .iter()
        .map(|&i| {
            let (w, h) = size_of(parts, i);
            (w + GAP_X) * (h + GAP_Y)
        })
        .sum();
    ROW_WIDTH.max(area.sqrt())
}

/// Parts side by side in rows of at most `row_width`, each part centred on its row.
fn rows(parts: &[Placed], members: &[usize], row_width: f64) -> Block {
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut used = 0.0;
    for &i in members {
        let w = size_of(parts, i).0;
        match lines.last_mut() {
            Some(line) if used + GAP_X + w <= row_width => {
                line.push(i);
                used += GAP_X + w;
            }
            _ => {
                lines.push(vec![i]);
                used = w;
            }
        }
    }
    let mut width: f64 = 0.0;
    let mut y = 0.0;
    let mut slots = Vec::new();
    for line in &lines {
        let height = line
            .iter()
            .map(|&i| size_of(parts, i).1)
            .fold(0.0, f64::max);
        let mut x = 0.0;
        for &i in line {
            let (w, h) = size_of(parts, i);
            slots.push((i, Pt::new(x, y + (height - h) / 2.0)));
            x += w + GAP_X;
        }
        width = width.max(x - GAP_X);
        y += height + GAP_Y;
    }
    Block {
        width,
        height: (y - GAP_Y).max(0.0),
        slots,
    }
}

/// Parts side by side in rows of at most `row_width` like [`rows`], but each row keeps clear
/// of the boxes in `avoid` (given relative to the block's top left corner): a part that
/// would touch one is moved along the row to its far side, and one that then does not fit in
/// the row begins the next. A part wider than the row, or a row filled with boxes, stands
/// alone on a row of its own.
fn rows_around(parts: &[Placed], members: &[usize], row_width: f64, avoid: &[Rect]) -> Block {
    let mut queue: VecDeque<usize> = members.iter().copied().collect();
    let clear = |at: Rect| {
        avoid
            .iter()
            .find(|a| {
                a.overlaps(&Rect::new(
                    Pt::new(at.min.x - GAP_X, at.min.y - GAP_Y),
                    Pt::new(at.max.x + GAP_X, at.max.y + GAP_Y),
                ))
            })
            .copied()
    };
    let mut width: f64 = 0.0;
    let mut y = 0.0;
    let mut slots = Vec::new();
    while !queue.is_empty() {
        let mut line: Vec<(usize, f64)> = Vec::new();
        let mut height: f64 = 0.0;
        let mut x = 0.0;
        while let Some(&i) = queue.front() {
            let (w, h) = size_of(parts, i);
            let span = height.max(h);
            let mut at = x;
            while let Some(hit) = clear(Rect::new(Pt::new(at, y), Pt::new(at + w, y + span))) {
                at = hit.max.x + GAP_X;
            }
            let fits = at + w <= row_width + 1e-9;
            if !fits && !line.is_empty() {
                break;
            }
            if !fits && line.is_empty() && w <= row_width {
                // Boxes fill this row: leave it empty and try further down.
                break;
            }
            queue.pop_front();
            line.push((i, at));
            height = span;
            x = at + w + GAP_X;
        }
        if line.is_empty() {
            y += GAP_Y + 1.0;
            continue;
        }
        for &(i, at) in &line {
            let (w, h) = size_of(parts, i);
            slots.push((i, Pt::new(at, y + (height - h) / 2.0)));
            width = width.max(at + w);
        }
        y += height + GAP_Y;
    }
    Block {
        width,
        height: (y - GAP_Y).max(0.0),
        slots,
    }
}

/// Round to a hundredth of a millimetre, so positions are short numbers.
fn tidy(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Whether the part has a hole through the board, which puts it on both faces.
fn is_through(part: &Placed) -> bool {
    part.has_hole()
}

/// Where the parts were put: the outline, and what the parts need of it.
struct Arrangement {
    outline: Rect,
    /// Width and height of the arrangement with its margin.
    needs: (f64, f64),
}

/// Set every part's position, taking the parts in `order`, and return the board outline.
/// The parts' sides are as they are in `parts`.
fn arrange(parts: &mut [Placed], order: &[usize], options: &PlaceOptions) -> Arrangement {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut front = Vec::new();
    let mut back = Vec::new();
    let mut connectors = 0;
    for &i in order {
        if is_connector(&parts[i].kind) {
            if connectors % 2 == 0 {
                left.push(i);
            } else {
                right.push(i);
            }
            connectors += 1;
        } else if parts[i].side == Side::Back && !is_through(&parts[i]) {
            back.push(i);
        } else {
            // A part with a hole in it takes room on the front rows whatever its side, and
            // the back rows keep clear of it.
            front.push(i);
        }
    }
    let left_block = column(parts, &left);
    let right_block = column(parts, &right);

    // The rows' width and, with a given size, where they begin on the board.
    let columns = [&left_block, &right_block]
        .iter()
        .filter(|b| !b.slots.is_empty())
        .count();
    let has_rows = !front.is_empty() || !back.is_empty();
    let blocks_count = columns + usize::from(has_rows);
    let (row_width, begin) = match options.fit {
        Some((w, _)) => {
            let used = left_block.width
                + right_block.width
                + BLOCK_GAP * blocks_count.saturating_sub(1) as f64;
            let widest = front
                .iter()
                .chain(&back)
                .map(|&i| size_of(parts, i).0)
                .fold(0.0, f64::max);
            let x = ORIGIN.x
                + options.wall()
                + if left.is_empty() {
                    0.0
                } else {
                    left_block.width + BLOCK_GAP
                };
            (
                (w - 2.0 * options.wall() - used).max(widest),
                Some(Pt::new(x, ORIGIN.y + options.wall())),
            )
        }
        None => (
            row_width_for(parts, &front).max(row_width_for(parts, &back)),
            None,
        ),
    };
    // Where the rows are is known when the size is: the boxes to keep clear of are then
    // relative to the rows' corner.
    let keepouts: Vec<Rect> = match begin {
        Some(corner) => options
            .keepouts
            .iter()
            .map(|k| k.translated(Pt::new(-corner.x, -corner.y)))
            .collect(),
        None => Vec::new(),
    };
    let front_block = if keepouts.is_empty() {
        rows(parts, &front, row_width)
    } else {
        rows_around(parts, &front, row_width, &keepouts)
    };
    let mut avoid = keepouts;
    avoid.extend(
        front_block
            .slots
            .iter()
            .filter(|(i, _)| is_through(&parts[*i]))
            .map(|&(i, corner)| {
                let (w, h) = size_of(parts, i);
                Rect::new(corner, Pt::new(corner.x + w, corner.y + h))
            }),
    );
    let back_block = if back.is_empty() {
        Block {
            width: 0.0,
            height: 0.0,
            slots: Vec::new(),
        }
    } else {
        rows_around(parts, &back, row_width, &avoid)
    };
    let middle = Block {
        width: front_block.width.max(back_block.width),
        height: front_block.height.max(back_block.height),
        slots: front_block
            .slots
            .into_iter()
            .chain(back_block.slots)
            .collect(),
    };

    // Which block is which, for placing them at the edges of a given size.
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Left,
        Rows,
        Right,
    }
    let blocks: Vec<(Kind, Block)> = [
        (Kind::Left, left_block),
        (Kind::Rows, middle),
        (Kind::Right, right_block),
    ]
    .into_iter()
    .filter(|(_, b)| !b.slots.is_empty())
    .collect();
    let width = blocks.iter().map(|(_, b)| b.width).sum::<f64>()
        + BLOCK_GAP * blocks.len().saturating_sub(1) as f64;
    let height = blocks.iter().map(|(_, b)| b.height).fold(0.0, f64::max);
    let needs = (width + 2.0 * options.wall(), height + 2.0 * options.wall());
    let (outline_width, outline_height) = match options.fit {
        Some(size) => size,
        None => (needs.0.ceil(), needs.1.ceil()),
    };
    let outline = Rect::new(
        ORIGIN,
        Pt::new(ORIGIN.x + outline_width, ORIGIN.y + outline_height),
    );

    // Without a size, the arrangement is centred in the outline, and each block on the
    // others. With one, the rows are at the top left, the connectors at the edges.
    let mut x = ORIGIN.x + (outline_width - width) / 2.0;
    for (kind, block) in &blocks {
        let centred = ORIGIN.y + (outline_height - block.height) / 2.0;
        let (block_x, top) = match (options.fit, kind) {
            (Some(_), Kind::Left) => (ORIGIN.x + options.wall(), centred),
            (Some(_), Kind::Rows) => (begin.map_or(x, |b| b.x), ORIGIN.y + options.wall()),
            (Some(_), Kind::Right) => (
                ORIGIN.x + outline_width - options.wall() - block.width,
                centred,
            ),
            (None, _) => (x, centred),
        };
        for &(i, corner) in &block.slots {
            // The footprint's courtyard where it would be with the part at the origin, which
            // is mirrored for a part on the back.
            parts[i].at = Pt::default();
            let courtyard = parts[i].courtyard();
            parts[i].at = Pt::new(
                tidy(block_x + corner.x - courtyard.min.x),
                tidy(top + corner.y - courtyard.min.y),
            );
        }
        x += block.width + BLOCK_GAP;
    }
    Arrangement { outline, needs }
}

/// The parts that are not fixed, taken in the order of their instance paths, so that the
/// rows keep the parts of a module together.
fn initial_order(parts: &[Placed], fixed: &[bool]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..parts.len()).filter(|&i| !fixed[i]).collect();
    order.sort_by(|&a, &b| parts[a].path.cmp(&parts[b].path));
    order
}

/// The box round the courtyards of the parts, and the box round them and their reference
/// text, on the board.
pub(crate) fn boxes(parts: &[Placed], rules: &Rules) -> Option<(Rect, Rect)> {
    let mut court: Option<Rect> = None;
    let mut all: Option<Rect> = None;
    for part in parts {
        let c = part.courtyard();
        court = Some(court.map_or(c, |r| r.union(&c)));
        let e = anneal::text_box(part, rules).map_or(c, |t| c.union(&t));
        all = Some(all.map_or(e, |r| r.union(&e)));
    }
    court.zip(all)
}

/// What parts need of the outline, given the box round their courtyards and the box round
/// them and their text: `wall` round the courtyards and [`anneal::TEXT_EDGE`] round the
/// text, whichever is more on each side. Also the margins on the low and high sides.
pub(crate) fn needs_of(court: Rect, all: Rect, wall: f64) -> ((f64, f64), (f64, f64), (f64, f64)) {
    let lo = (
        wall.max(court.min.x - all.min.x + anneal::TEXT_EDGE),
        wall.max(court.min.y - all.min.y + anneal::TEXT_EDGE),
    );
    let hi = (
        wall.max(all.max.x - court.max.x + anneal::TEXT_EDGE),
        wall.max(all.max.y - court.max.y + anneal::TEXT_EDGE),
    );
    (
        (court.width() + lo.0 + hi.0, court.height() + lo.1 + hi.1),
        lo,
        hi,
    )
}

/// Where the parts are, to put them back if a try fails.
fn snapshot(parts: &[Placed]) -> Vec<(Pt, f64, ReferenceSide)> {
    parts
        .iter()
        .map(|p| (p.at, p.rotation, p.reference_side))
        .collect()
}

fn restore(parts: &mut [Placed], saved: Vec<(Pt, f64, ReferenceSide)>) {
    for (p, (at, rotation, text)) in parts.iter_mut().zip(saved) {
        p.at = at;
        p.rotation = rotation;
        p.reference_side = text;
    }
}

/// The want of each part, in the order of `parts`.
/// The edges each part's requirements say it may hang over.
fn overhangs_of(parts: &[Placed], options: &PlaceOptions) -> Vec<overhang::Edges> {
    parts
        .iter()
        .map(|p| {
            options
                .overhang
                .get(&p.refdes)
                .map(Overhang::edges)
                .unwrap_or_default()
        })
        .collect()
}

fn wants_of(parts: &[Placed], options: &PlaceOptions) -> Vec<Option<Want>> {
    parts
        .iter()
        .map(|p| options.wants.get(&p.refdes).cloned())
        .collect()
}

/// Put the fixed parts where they stay.
fn apply_fixed(parts: &mut [Placed], options: &PlaceOptions) -> Vec<bool> {
    parts
        .iter_mut()
        .map(|p| match options.fixed.get(&p.refdes) {
            Some(f) => {
                p.at = ORIGIN + f.at;
                p.rotation = f.rotation;
                p.side = f.side;
                p.reference_side = f.text;
                true
            }
            None => false,
        })
        .collect()
}

/// Whether the height limits leave no room for the part on `face` (PROVISIONAL, ADR 0045
/// section 11): its height is known, and the zones that rule it out there cover the face so
/// that no place inside the outline's margin is left where its body (grown by the placer's
/// margin) is clear of them, in either turn. A part with no stated height is never forbidden
/// a face: nobody knows whether it needs one. Only a given size is judged (without one the
/// board grows to hold what is asked of it).
fn face_forbidden(part: &Placed, face: Side, options: &PlaceOptions) -> bool {
    let (Some(height), Some(fit)) = (part.height, options.fit) else {
        return false;
    };
    let blocked: Vec<Rect> = options
        .height_zones
        .iter()
        .filter(|z| z.rules_out(height, face))
        .map(|z| z.rect)
        .collect();
    if blocked.is_empty() {
        return false;
    }
    let region = Rect::new(
        Pt::new(ORIGIN.x + options.wall(), ORIGIN.y + options.wall()),
        Pt::new(
            ORIGIN.x + fit.0 - options.wall(),
            ORIGIN.y + fit.1 - options.wall(),
        ),
    );
    let margin = if part.footprint.body_is_courtyard {
        0.0
    } else {
        anneal::HEIGHT_ZONE_MARGIN
    };
    let body = part.body();
    let (w, h) = (body.width() + 2.0 * margin, body.height() + 2.0 * margin);
    // A free box, if there is one, can be slid left and up until it rests on the region's
    // edge or a zone's: so only those corners need trying.
    let mut lefts = vec![region.min.x];
    let mut tops = vec![region.min.y];
    for z in &blocked {
        lefts.push(z.max.x);
        tops.push(z.max.y);
    }
    for (w, h) in [(w, h), (h, w)] {
        for &x in &lefts {
            for &y in &tops {
                let at = Rect::new(Pt::new(x, y), Pt::new(x + w, y + h));
                let inside = at.max.x <= region.max.x + 1e-9 && at.max.y <= region.max.y + 1e-9;
                if inside
                    && blocked.iter().all(|z| {
                        at.max.x <= z.min.x + 1e-9
                            || z.max.x <= at.min.x + 1e-9
                            || at.max.y <= z.min.y + 1e-9
                            || z.max.y <= at.min.y + 1e-9
                    })
                {
                    return false;
                }
            }
        }
    }
    true
}

/// Put a part that nobody placed on the face its height allows, when the height limits rule
/// out the face it is on and not the other (B1, PROVISIONAL): the face split would otherwise
/// leave a tall part under a lid that cannot hold it while the other face is free.
fn keep_tall_parts_off_low_faces(parts: &mut [Placed], fixed: &[bool], options: &PlaceOptions) {
    for (i, part) in parts.iter_mut().enumerate() {
        if fixed[i] || options.sides.contains_key(&part.refdes) || is_through(part) {
            continue;
        }
        let other = if part.side == Side::Back {
            Side::Front
        } else {
            Side::Back
        };
        if face_forbidden(part, part.side, options) && !face_forbidden(part, other, options) {
            part.side = other;
        }
    }
}

/// Set every part's position and return the board outline. The sides of `parts` are those
/// the caller chose; with a size to fit, parts the caller did not place are moved between
/// the faces if they do not fit it on the faces they have.
///
/// The rows are only where the annealer starts. They do not decide whether the parts fit: a
/// part that is wider than the board (the rows never turn one) or that hangs over its edge
/// makes the rows too big for boards that are placed without trouble. With a size the
/// annealer is run in it whatever the rows say, first with the parts on the faces they
/// have, then, if that fails, with the free parts shared between the faces. The parts do not
/// fit only if it ends with courtyards over each other or parts outside.
pub(crate) fn arrange_best(
    parts: &mut [Placed],
    nets: &[BoardNet],
    options: &PlaceOptions,
    context: &Context,
) -> Result<Rect, DoesNotFit> {
    let fixed = apply_fixed(parts, options);
    keep_tall_parts_off_low_faces(parts, &fixed, options);
    turn_to_fit(parts, &fixed, options);
    let mut arrangement = arrange_free(parts, &fixed, options);
    let Some(fit) = options.fit else {
        return refine(
            parts,
            nets,
            options,
            context,
            &fixed,
            &arrangement,
            true,
            false,
        );
    };
    let fits = |a: &Arrangement| a.needs.0 <= fit.0 + 1e-9 && a.needs.1 <= fit.1 + 1e-9;
    if fits(&arrangement) {
        return refine(
            parts,
            nets,
            options,
            context,
            &fixed,
            &arrangement,
            true,
            false,
        );
    }
    // The rows are too big. The annealer may still place the parts in the size, on the faces
    // they are on, unless their courtyards alone cannot fit the area of a face.
    let start = snapshot(parts);
    // Share the parts that nobody placed between the faces, the largest first, each to the
    // face with the least area so far. Connectors, parts with holes and parts the layout
    // fixes stay as they are.
    let area = |p: &Placed| {
        let c = p.courtyard();
        (c.width() + GAP_X) * (c.height() + GAP_Y)
    };
    let free = |i: usize, p: &Placed| {
        !fixed[i]
            && !options.sides.contains_key(&p.refdes)
            && !is_connector(&p.kind)
            && !is_through(p)
    };
    let mut movable: Vec<usize> = (0..parts.len()).filter(|&i| free(i, &parts[i])).collect();
    // Where nothing can be shared, the first try is the only one there is, and it is tried
    // again if it ends a hair short.
    let first = if could_fit(parts, fit, options) {
        match refine(
            parts,
            nets,
            options,
            context,
            &fixed,
            &arrangement,
            false,
            movable.is_empty(),
        ) {
            Ok(outline) => return Ok(outline),
            Err(e) => Some(e),
        }
    } else {
        None
    };
    if movable.is_empty()
        && let Some(error) = first
    {
        return Err(error);
    }
    restore(parts, start);
    let mut load = [0.0f64; 2];
    for p in parts
        .iter()
        .enumerate()
        .filter(|(i, p)| !free(*i, p))
        .map(|(_, p)| p)
    {
        load[usize::from(p.side == Side::Back)] += area(p);
    }
    movable.sort_by(|&a, &b| area(&parts[b]).total_cmp(&area(&parts[a])));
    for i in movable {
        let mut face = usize::from(load[1] < load[0]);
        // A face whose height limit leaves no room for the part is not its to take, when the
        // other is (B1, PROVISIONAL).
        let faces = [Side::Front, Side::Back];
        if face_forbidden(&parts[i], faces[face], options)
            && !face_forbidden(&parts[i], faces[1 - face], options)
        {
            face = 1 - face;
        }
        load[face] += area(&parts[i]);
        parts[i].side = faces[face];
    }
    arrangement = arrange_free(parts, &fixed, options);
    let rows_fit = fits(&arrangement);
    refine(
        parts,
        nets,
        options,
        context,
        &fixed,
        &arrangement,
        rows_fit,
        true,
    )
}

/// The room for a courtyard inside an outline of `fit`: the outline less the margin on each
/// side.
fn inside(fit: (f64, f64), options: &PlaceOptions) -> (f64, f64) {
    (
        (fit.0 - 2.0 * options.wall()).max(0.0),
        (fit.1 - 2.0 * options.wall()).max(0.0),
    )
}

/// Whether a part's turn is the placer's to choose: the layout file does not lock it, its
/// requirements leave all four turns (a requirement on a position or a face does not limit
/// them; one on the rotation does, and the annealer keeps to it) and it may not hang over an
/// edge (its courtyard is allowed past it, so it is not turned to fit).
fn turn_is_free(part: &Placed, fixed: bool, options: &PlaceOptions) -> bool {
    !fixed
        && !options.overhang.contains_key(&part.refdes)
        && options
            .wants
            .get(&part.refdes)
            .is_none_or(|want| want.reach.iter().all(Option::is_some))
}

/// Turn the part whose courtyard is too big for the outline to fit in the turn it has, and
/// fits it turned a quarter (PROVISIONAL, ADR 0033): the rows, which are where the annealer
/// starts, never turn a part, and a part longer than the outline's short side starts far
/// outside it. The turn is by 90 degrees (then 270, if its requirements rule the first
/// out), so the same input gives the same board. A part with requirements on its face or
/// its position is turned like any other, to a turn they allow; one whose requirements
/// state a rotation keeps to the turns they leave, and if none of them fits, the error names
/// it. A part that fits as it is, one that fits in neither turn (the annealer tries them
/// all; the error says so, see [`oversize`]), one the layout file locks and one that may hang
/// over an edge stay as they are.
fn turn_to_fit(parts: &mut [Placed], fixed: &[bool], options: &PlaceOptions) {
    let Some(fit) = options.fit else { return };
    let (width, height) = inside(fit, options);
    for (i, part) in parts.iter_mut().enumerate() {
        if fixed[i] || options.overhang.contains_key(&part.refdes) {
            continue;
        }
        let court = part.courtyard();
        let fits = |w: f64, h: f64| w <= width + 1e-9 && h <= height + 1e-9;
        if fits(court.width(), court.height()) || !fits(court.height(), court.width()) {
            continue;
        }
        let want = options.wants.get(&part.refdes);
        let allowed = |angle: f64| {
            want.is_none_or(|w| {
                crate::constraints::quarter(angle).is_some_and(|q| w.reach[q].is_some())
            })
        };
        let turned = [90.0, 270.0]
            .into_iter()
            .map(|extra| (part.rotation + extra).rem_euclid(360.0))
            .find(|&angle| allowed(angle));
        if let Some(angle) = turned {
            part.rotation = angle;
        }
    }
}

/// Where every part is and in what state, from what the annealer ended with.
fn part_states(parts: &[Placed], fixed: &[bool], ended: &anneal::Ended) -> Vec<PartState> {
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut state: Vec<String> = ended
                .outside_parts
                .iter()
                .filter(|o| o.part == p.refdes)
                .flat_map(|o| o.why.iter().cloned())
                .collect();
            state.extend(ended.overlapping.iter().filter_map(|o| {
                let other = if o.a == p.refdes {
                    &o.b
                } else if o.b == p.refdes {
                    &o.a
                } else {
                    return None;
                };
                Some(format!(
                    "its courtyard overlaps {other}'s by {:.2} mm2 ({:.2} x {:.2} mm)",
                    o.area, o.size.0, o.size.1
                ))
            }));
            let court = p.courtyard();
            PartState {
                part: p.refdes.clone(),
                kind: p.kind.clone(),
                side: p.side,
                at: (p.at.x - ORIGIN.x, p.at.y - ORIGIN.y),
                rotation: p.rotation,
                size: (court.width(), court.height()),
                fixed: fixed[i],
                state,
            }
        })
        .collect()
}

/// The parts that do not fit `fit` in any turn they may take, with every turn tried and the
/// courtyard's size in it. A part that may hang over an edge is not one.
fn oversize(
    parts: &[Placed],
    fixed: &[bool],
    options: &PlaceOptions,
    fit: (f64, f64),
) -> Vec<Oversize> {
    let (width, height) = inside(fit, options);
    let mut found = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if options.overhang.contains_key(&part.refdes) {
            continue;
        }
        let court = part.courtyard();
        let turns: Vec<f64> = if fixed[i] {
            vec![part.rotation]
        } else if let Some(want) = options.wants.get(&part.refdes) {
            (0..4)
                .filter(|&q| want.reach[q].is_some())
                .map(|q| q as f64 * 90.0)
                .collect()
        } else {
            (0..4)
                .map(|k| (part.rotation + 90.0 * f64::from(k)).rem_euclid(360.0))
                .collect()
        };
        // A requirement's turns are absolute; the size in a turn follows from the part's own.
        let tried: Vec<(f64, (f64, f64))> = turns
            .iter()
            .map(|&angle| {
                let quarters = ((angle - part.rotation) / 90.0).round().rem_euclid(2.0);
                let size = if quarters == 0.0 {
                    (court.width(), court.height())
                } else {
                    (court.height(), court.width())
                };
                (angle, size)
            })
            .collect();
        if !tried.is_empty()
            && tried
                .iter()
                .all(|(_, (w, h))| *w > width + 1e-9 || *h > height + 1e-9)
        {
            found.push(Oversize {
                part: part.refdes.clone(),
                kind: part.kind.clone(),
                tried,
                free: turn_is_free(part, fixed[i], options),
            });
        }
    }
    found
}

/// Whether the courtyards could fit `fit` on the faces they are on: the area of those on a
/// face, with a hole on both, must not be more than what is left inside the margin. Parts
/// that may hang over the edge are not counted, for it is not known how much of them is
/// inside.
fn could_fit(parts: &[Placed], fit: (f64, f64), options: &PlaceOptions) -> bool {
    let inside = (fit.0 - 2.0 * options.wall()).max(0.0) * (fit.1 - 2.0 * options.wall()).max(0.0);
    let mut load = [0.0f64; 2];
    for p in parts
        .iter()
        .filter(|p| !options.overhang.contains_key(&p.refdes))
    {
        let c = p.courtyard();
        let area = c.width() * c.height();
        if is_through(p) {
            // On a face the footprint draws no courtyard for, the part takes only its pads
            // (`Placed::room_on`; the pads' own area is what is certainly taken).
            for (k, face) in [Side::Front, Side::Back].into_iter().enumerate() {
                load[k] += p
                    .room_on(face, 0.0, c)
                    .iter()
                    .map(|r| r.width() * r.height())
                    .sum::<f64>();
            }
        } else {
            load[usize::from(p.side == Side::Back)] += area;
        }
    }
    load[0] <= inside + 1e-9 && load[1] <= inside + 1e-9
}

/// The rows arrangement of the parts that are not fixed, with the fixed ones where they stay.
fn arrange_free(parts: &mut [Placed], fixed: &[bool], options: &PlaceOptions) -> Arrangement {
    let order = initial_order(parts, fixed);
    let mut arrangement = arrange(parts, &order, options);
    // The fixed parts are where the layout put them; the outline must hold them too.
    if let Some(needs) = fixed_needs(parts, fixed, options) {
        arrangement.needs.0 = arrangement.needs.0.max(needs.0);
        arrangement.needs.1 = arrangement.needs.1.max(needs.1);
        arrangement.outline.max.x = arrangement
            .outline
            .max
            .x
            .max(ORIGIN.x + arrangement.needs.0.ceil());
        arrangement.outline.max.y = arrangement
            .outline
            .max
            .y
            .max(ORIGIN.y + arrangement.needs.1.ceil());
    }
    arrangement
}

/// What the fixed parts need of the outline, with the margin beyond them. A fixed part that
/// the person said hangs over an edge of a given size counts only as its courtyard clipped to
/// the outline, and needs no margin on the side it hangs over.
fn fixed_needs(parts: &[Placed], fixed: &[bool], options: &PlaceOptions) -> Option<(f64, f64)> {
    let outline = options
        .fit
        .map(|(w, h)| Rect::new(ORIGIN, Pt::new(ORIGIN.x + w, ORIGIN.y + h)));
    parts
        .iter()
        .zip(fixed)
        .filter(|(_, f)| **f)
        .map(|(p, _)| {
            let court = p.courtyard();
            match (&outline, options.overhang.get(&p.refdes)) {
                (Some(outline), Some(declared)) => {
                    overhang::needs(&court, outline, declared.edges(), options.wall())
                }
                _ => (
                    court.max.x - ORIGIN.x + options.wall(),
                    court.max.y - ORIGIN.y + options.wall(),
                ),
            }
        })
        .reduce(|a, b| (a.0.max(b.0), a.1.max(b.1)))
}

/// How many more tries a placement that ended a hair short is given. The placer had no retry
/// of its own before (B5, PROVISIONAL), so this is a small fixed number.
const RETRIES: u64 = 4;
/// A run that ended with more offenders than this is not tried again: another seed does not
/// turn a crowd of overlaps into a placement.
const RETRY_WORTH: usize = 12;
/// A part is large when its courtyard is at least this share of what one face holds inside
/// the margin (B2, PROVISIONAL).
const LARGE_SHARE: f64 = 0.10;

/// A seed derived from `seed` for the `k`th try, the same on every machine.
fn derived_seed(seed: u64, k: u64) -> u64 {
    let mut z = seed.wrapping_add(k.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The parts the annealer has trouble bringing inside, largest first: free parts whose
/// courtyard is a tenth of a face or more.
fn large_parts(
    parts: &[Placed],
    fixed: &[bool],
    options: &PlaceOptions,
    region: Rect,
) -> Vec<usize> {
    let face = (region.width() - 2.0 * options.wall()).max(0.0)
        * (region.height() - 2.0 * options.wall()).max(0.0);
    let area = |p: &Placed| p.courtyard().width() * p.courtyard().height();
    let mut large: Vec<usize> = (0..parts.len())
        .filter(|&i| {
            !fixed[i]
                && !options.overhang.contains_key(&parts[i].refdes)
                && !is_connector(&parts[i].kind)
                && area(&parts[i]) >= LARGE_SHARE * face
        })
        .collect();
    large.sort_by(|&a, &b| area(&parts[b]).total_cmp(&area(&parts[a])).then(a.cmp(&b)));
    large.truncate(3);
    large
}

/// Put each large part in the corner of `region` where it clashes with nothing that stays
/// (the parts already fixed, the parts put before it, the keepouts), turned as it is, with
/// its reference text on the side toward the middle, and mark it fixed for the run. A part
/// with no clear corner is left to the annealer. Deterministic: the corners are taken in
/// the order top left, top right, bottom left, bottom right.
fn put_large_in_corners(
    parts: &mut [Placed],
    frozen: &mut [bool],
    large: &[usize],
    options: &PlaceOptions,
    region: Rect,
) {
    let wall = options.wall();
    let inner = Rect::new(
        Pt::new(region.min.x + wall, region.min.y + wall),
        Pt::new(region.max.x - wall, region.max.y - wall),
    );
    let share = |a: &Rect, b: &Rect| {
        (a.max.x.min(b.max.x) - a.min.x.max(b.min.x)).max(0.0)
            * (a.max.y.min(b.max.y) - a.min.y.max(b.min.y)).max(0.0)
    };
    for &i in large {
        parts[i].at = Pt::default();
        let court = parts[i].courtyard();
        let up = |v: f64| (v / 0.05 - 1e-9).ceil() * 0.05;
        let down = |v: f64| (v / 0.05 + 1e-9).floor() * 0.05;
        // (x, y, side of the reference text) of the origin at each corner.
        let corners = [
            (
                up(inner.min.x - court.min.x),
                up(inner.min.y - court.min.y),
                ReferenceSide::Below,
            ),
            (
                down(inner.max.x - court.max.x),
                up(inner.min.y - court.min.y),
                ReferenceSide::Below,
            ),
            (
                up(inner.min.x - court.min.x),
                down(inner.max.y - court.max.y),
                ReferenceSide::Above,
            ),
            (
                down(inner.max.x - court.max.x),
                down(inner.max.y - court.max.y),
                ReferenceSide::Above,
            ),
        ];
        for (x, y, text) in corners {
            let mut at = Pt::new(tidy(x), tidy(y));
            if let Some(want) = options.wants.get(&parts[i].refdes) {
                at = want.clamp(parts[i].rotation, at);
            }
            parts[i].at = at;
            let room = parts[i].courtyard();
            let clear = inner.contains(&room)
                && options.keepouts.iter().all(|k| share(&room, k) < 1e-6)
                && parts.iter().enumerate().all(|(j, q)| {
                    j == i
                        || !(frozen[j] || options.fixed.contains_key(&q.refdes))
                        || (q.side != parts[i].side && !is_through(q) && !is_through(&parts[i]))
                        || share(&room, &q.courtyard()) < 1e-6
                });
            if clear {
                parts[i].reference_side = text;
                frozen[i] = true;
                break;
            }
            parts[i].at = Pt::default();
        }
    }
}

/// Try a placement that ended a hair short again, up to [`RETRIES`] times: from the rows,
/// with the largest parts put in corners first (B2) and a seed derived from the given one
/// (B5), keeping the run that missed by least. Returns it and the number of runs made,
/// the first included. The same input gives the same result: nothing here reads a clock or a
/// random source.
fn retry(
    parts: &mut [Placed],
    problem: &Problem,
    rows_at: &[(Pt, f64, ReferenceSide)],
    first: Outcome,
    options: &PlaceOptions,
) -> (Outcome, usize) {
    let mut best_poses = snapshot(parts);
    let mut best = first;
    let large = large_parts(parts, problem.fixed, options, problem.region);
    let mut tries = 1;
    for k in 1..=RETRIES {
        restore(parts, rows_at.to_vec());
        let mut frozen = problem.fixed.to_vec();
        put_large_in_corners(parts, &mut frozen, &large, options, problem.region);
        let seed = if k == 1 && !large.is_empty() {
            problem.seed
        } else {
            derived_seed(problem.seed, k)
        };
        let attempt = Problem {
            fixed: &frozen,
            seed,
            ..*problem
        };
        let outcome = anneal::anneal(parts, &attempt, 1.0);
        tries += 1;
        if outcome.feasible {
            return (outcome, tries);
        }
        if outcome.ended.badness() < best.ended.badness() {
            best = outcome;
            best_poses = snapshot(parts);
        }
    }
    restore(parts, best_poses);
    (best, tries)
}

/// Anneal the parts that are not fixed inside the outline the rows made, and fit the
/// outline to the result if the board was not given a size. Falls back to the rows if the
/// annealer cannot place every part clear of the others and the rows fit; if they do not
/// (`rows_fit` is false) the parts do not fit the size, and the error says what the annealer
/// ended with.
fn refine(
    parts: &mut [Placed],
    nets: &[BoardNet],
    options: &PlaceOptions,
    context: &Context,
    fixed: &[bool],
    rows: &Arrangement,
    rows_fit: bool,
    retries: bool,
) -> Result<Rect, DoesNotFit> {
    let rows_at = snapshot(parts);
    let mut region = rows.outline;
    if let Some(size) = options.fit {
        region = Rect::new(ORIGIN, Pt::new(ORIGIN.x + size.0, ORIGIN.y + size.1));
    }
    for k in options
        .keepouts
        .iter()
        .chain(options.height_zones.iter().map(|z| &z.rect))
    {
        if options.fit.is_none() {
            region = region.union(&k.expanded(options.wall()));
        }
    }
    // Where a requirement puts a part is part of the board.
    let wants = wants_of(parts, options);
    let overhangs = overhangs_of(parts, options);
    for (part, want) in parts.iter().zip(&wants) {
        if let (Some(extent), None) = (want.as_ref().and_then(Want::extent), options.fit) {
            let size = part.courtyard();
            let reach = size.width().max(size.height());
            region = region.union(&extent.expanded(reach + options.wall()));
        }
    }
    // A relation may ask for more room than the rows leave: the far part must be able to
    // be as far from the near one as the window says.
    if options.fit.is_none() {
        let reach = options
            .offsets
            .iter()
            .flat_map(|o| [o.lo, o.hi])
            .filter(|v| v.is_finite())
            .map(f64::abs)
            .fold(0.0, f64::max);
        if reach > 0.0 {
            region = region.expanded(reach);
        }
    }
    let problem = Problem {
        nets,
        rules: context.rules,
        planes: context.planes,
        region,
        corner_radius: context.corner_radius,
        fixed,
        keepouts: &options.keepouts,
        cutouts: &options.cutouts,
        pours: context.pours,
        seed: options.seed,
        wants: &wants,
        offsets: &options.offsets,
        overhangs: &overhangs,
        outline: region,
        zones: &options.height_zones,
    };
    let mut outcome = anneal::anneal(parts, &problem, 1.0);
    let mut tries = 1;
    // A run that ends a hair short is tried again (B2 and B5, PROVISIONAL), only where the
    // answer would be a refusal and the courtyards could fit.
    if !outcome.feasible
        && retries
        && !rows_fit
        && options
            .fit
            .is_some_and(|fit| could_fit(parts, fit, options))
        && outcome.ended.names().len() <= RETRY_WORTH
    {
        (outcome, tries) = retry(parts, &problem, &rows_at, outcome, options);
    }
    if !outcome.feasible {
        if let (Some(fit), false) = (options.fit, rows_fit) {
            let room = inside(fit, options);
            let area = parts
                .iter()
                .filter(|p| !options.overhang.contains_key(&p.refdes))
                .map(|p| {
                    let c = p.courtyard();
                    c.width() * c.height()
                })
                .sum();
            return Err(DoesNotFit {
                wall: options.wall(),
                needs: rows.needs,
                outline: fit,
                oversize: oversize(parts, fixed, options, fit),
                area: (area, 2.0 * room.0 * room.1),
                parts: part_states(parts, fixed, &outcome.ended).into_boxed_slice(),
                requirements: None,
                ended: Some(Box::new(outcome.ended)),
                tries,
            });
        }
        restore(parts, rows_at);
        // The rows do not know the requirements: put the parts they govern in them.
        for (part, want) in parts.iter_mut().zip(&wants) {
            if let Some(want) = want {
                part.at = want.clamp(part.rotation, part.at);
            }
        }
        return Ok(rows.outline.union(&region));
    }
    if options.fit.is_some() {
        return Ok(region);
    }
    // Without a size the board is as big as the parts need.
    let Some((court, all)) = boxes(parts, context.rules) else {
        return Ok(region);
    };
    let (needs, lo, hi) = needs_of(court, all, options.wall());
    let size = (needs.0.ceil(), needs.1.ceil());
    let pinned = fixed.iter().any(|&f| f)
        || !options.keepouts.is_empty()
        || !options.height_zones.is_empty()
        || !options.wants.is_empty();
    if pinned {
        let width = (court.max.x + hi.0 - ORIGIN.x).max(size.0).ceil();
        let height = (court.max.y + hi.1 - ORIGIN.y).max(size.1).ceil();
        return Ok(Rect::new(
            ORIGIN,
            Pt::new(ORIGIN.x + width, ORIGIN.y + height),
        ));
    }
    // Move everything so that the parts are inside the new outline, a whole number of units.
    let shift = Pt::new(
        ((ORIGIN.x + lo.0 + (size.0 - needs.0) / 2.0 - court.min.x) / 0.05).round() * 0.05,
        ((ORIGIN.y + lo.1 + (size.1 - needs.1) / 2.0 - court.min.y) / 0.05).round() * 0.05,
    );
    for p in parts.iter_mut() {
        p.at = Pt::new(tidy(p.at.x + shift.x), tidy(p.at.y + shift.y));
    }
    Ok(Rect::new(
        ORIGIN,
        Pt::new(ORIGIN.x + size.0, ORIGIN.y + size.1),
    ))
}

/// Anneal the parts that are not fixed where they are, inside `region`, clear of `keepouts`.
/// Returns whether every part was placed clear of the others; if not, nothing is changed.
pub(crate) fn anneal_within(
    parts: &mut [Placed],
    nets: &[BoardNet],
    options: &PlaceOptions,
    context: &Context,
    region: Rect,
    keepouts: &[Rect],
) -> bool {
    let before = snapshot(parts);
    let fixed = apply_fixed(parts, options);
    let wants = wants_of(parts, options);
    let overhangs = overhangs_of(parts, options);
    let problem = Problem {
        nets,
        rules: context.rules,
        planes: context.planes,
        region,
        corner_radius: context.corner_radius,
        fixed: &fixed,
        keepouts,
        cutouts: &options.cutouts,
        pours: context.pours,
        seed: options.seed,
        wants: &wants,
        offsets: &options.offsets,
        overhangs: &overhangs,
        outline: region,
        zones: &options.height_zones,
    };
    let outcome = anneal::anneal(parts, &problem, 1.0);
    if !outcome.feasible {
        restore(parts, before);
    }
    outcome.feasible
}

/// Why some parts of a board could not be placed.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaceError {
    /// A reference designator the board does not have.
    NoPart { refdes: String },
    /// The parts do not fit the region without overlapping what is there.
    Crowded {
        region: Rect,
        parts: Vec<String>,
        /// The courtyard area of the parts and the region's free area, in mm squared.
        needs: f64,
        free: f64,
    },
}

impl std::fmt::Display for PlaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPart { refdes } => write!(f, "the board has no part `{refdes}` to place"),
            Self::Crowded {
                region,
                parts,
                needs,
                free,
            } => write!(
                f,
                "{} part(s) ({}) with {needs:.0} mm2 of courtyard do not fit the region {:.1} x {:.1} mm at ({:.1}, {:.1}), which has {free:.0} mm2 free of other parts",
                parts.len(),
                parts.join(", "),
                region.width(),
                region.height(),
                region.min.x,
                region.min.y,
            ),
        }
    }
}

impl std::error::Error for PlaceError {}

/// Place only the parts named by reference designator, inside `region` (in board
/// coordinates, mm), and leave every other part where it is: the others are obstacles, and
/// their pads pull on the nets of the parts placed. The same board, parts, region and seed
/// always give the same positions. This is what regenerating one module is made of.
///
/// Nothing is changed if the parts do not fit.
pub fn place_subset(
    board: &mut Board,
    parts: &[&str],
    region: Rect,
    seed: u64,
) -> Result<(), PlaceError> {
    for r in parts {
        if board.part(r).is_none() {
            return Err(PlaceError::NoPart {
                refdes: (*r).to_string(),
            });
        }
    }
    let fixed: Vec<bool> = board
        .parts
        .iter()
        .map(|p| !parts.contains(&p.refdes.as_str()))
        .collect();
    let mut keepouts: Vec<Rect> = board
        .keepout_areas()
        .into_iter()
        .filter(|k| k.parts)
        .map(|k| k.rect)
        .collect();
    let cutouts: Vec<Rect> = board
        .cutout_shapes()
        .into_iter()
        .map(|(_, s)| s.rect)
        .collect();
    keepouts.extend(cutouts.iter().copied());
    let planes: Vec<String> = board.planes.iter().map(|p| p.net.clone()).collect();
    let zones = board.height_zones();
    let overhangs: Vec<overhang::Edges> = board
        .parts
        .iter()
        .map(|p| {
            board
                .overhang
                .get(&p.refdes)
                .map(Overhang::edges)
                .unwrap_or_default()
        })
        .collect();
    // The region the parts may be in is the outline's part of the region.
    let region = Rect::new(
        Pt::new(
            region.min.x.max(board.outline.min.x),
            region.min.y.max(board.outline.min.y),
        ),
        Pt::new(
            region.max.x.min(board.outline.max.x),
            region.max.y.min(board.outline.max.y),
        ),
    );
    let problem = Problem {
        nets: &board.nets,
        rules: &board.rules,
        planes: &planes,
        region: board.outline,
        corner_radius: board.corner_radius,
        fixed: &fixed,
        keepouts: &keepouts,
        cutouts: &cutouts,
        pours: !board.pours.is_empty(),
        seed: seed ^ scope_of(parts),
        wants: &[],
        offsets: &[],
        overhangs: &overhangs,
        outline: board.outline,
        zones: &zones,
    };
    // The parts are kept in the region by a keepout round it.
    let mut walls = keepouts.clone();
    let o = board.outline;
    let big = 1.0e4;
    walls.extend([
        Rect::new(
            Pt::new(o.min.x - big, o.min.y - big),
            Pt::new(region.min.x, o.max.y + big),
        ),
        Rect::new(
            Pt::new(region.max.x, o.min.y - big),
            Pt::new(o.max.x + big, o.max.y + big),
        ),
        Rect::new(
            Pt::new(o.min.x - big, o.min.y - big),
            Pt::new(o.max.x + big, region.min.y),
        ),
        Rect::new(
            Pt::new(o.min.x - big, region.max.y),
            Pt::new(o.max.x + big, o.max.y + big),
        ),
    ]);
    let problem = Problem {
        keepouts: &walls,
        ..problem
    };
    let before = snapshot(&board.parts);
    let outcome = anneal::anneal(&mut board.parts, &problem, 1.0);
    if !outcome.feasible {
        restore(&mut board.parts, before);
        let moved: Vec<&Placed> = board
            .parts
            .iter()
            .filter(|p| parts.contains(&p.refdes.as_str()))
            .collect();
        let needs = moved
            .iter()
            .map(|p| p.courtyard().width() * p.courtyard().height())
            .sum();
        let overlap = |a: &Rect, b: &Rect| {
            (a.max.x.min(b.max.x) - a.min.x.max(b.min.x)).max(0.0)
                * (a.max.y.min(b.max.y) - a.min.y.max(b.min.y)).max(0.0)
        };
        let used: f64 = board
            .parts
            .iter()
            .filter(|p| !parts.contains(&p.refdes.as_str()))
            .map(|p| overlap(&p.courtyard(), &region))
            .sum();
        return Err(PlaceError::Crowded {
            region,
            parts: parts.iter().map(|s| s.to_string()).collect(),
            needs,
            free: (region.width() * region.height() - used).max(0.0),
        });
    }
    Ok(())
}

/// A number from the names of the parts being placed, so that placing one module does not
/// use the random numbers of placing another.
fn scope_of(parts: &[&str]) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for part in parts {
        for b in part.bytes().chain([0]) {
            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use netsmith_core::{Design, Net, Part, PinRef};

    use crate::board::Board;
    use crate::board::tests::{fixtures, led_design};

    use super::*;

    fn refusal(tries: usize) -> DoesNotFit {
        DoesNotFit {
            needs: (30.0, 40.0),
            outline: (35.0, 60.0),
            ended: Some(Box::new(Ended {
                outside: 1,
                outside_parts: vec![anneal::OutsideAt {
                    part: "U1".into(),
                    why: vec!["0.004 mm inside the 0.75 mm margin at the left edge".into()],
                    worst: 0.004,
                }],
                ..Ended::default()
            })),
            wall: 0.75,
            oversize: Vec::new(),
            area: (100.0, 3000.0),
            parts: Box::default(),
            requirements: None,
            tries,
        }
    }

    #[test]
    fn a_refusal_says_how_many_tries_were_made_and_shows_a_small_shortfall_with_its_digits() {
        let message = refusal(5).to_string();
        assert!(message.contains("U1 0.004"), "{message}");
        assert!(!message.contains("U1 0.00 "), "{message}");
        assert!(
            message.contains("the placer made 5 tries (the first, then 4 more"),
            "{message}"
        );
        // One try is not worth a line.
        assert!(!refusal(1).to_string().contains("tries"));
    }

    #[test]
    fn the_seeds_of_the_retries_are_derived_from_the_given_one_and_differ() {
        let seeds: Vec<u64> = (1..=RETRIES).map(|k| derived_seed(7, k)).collect();
        assert_eq!(
            seeds,
            (1..=RETRIES)
                .map(|k| derived_seed(7, k))
                .collect::<Vec<_>>()
        );
        for (i, a) in seeds.iter().enumerate() {
            assert_ne!(*a, 7);
            assert!(seeds[i + 1..].iter().all(|b| a != b));
        }
        assert_ne!(derived_seed(7, 1), derived_seed(8, 1));
    }

    fn part(refdes: &str, kind: &str, package: Option<&str>) -> Part {
        Part {
            refdes: refdes.into(),
            kind: kind.into(),
            value: "1".into(),
            package: package.map(str::to_string),
            path: String::new(),
            pins: Vec::new(),
        }
    }

    fn board_of(parts: Vec<Part>) -> Board {
        let design = Design {
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            parts,
            nets: vec![Net {
                name: "N".into(),
                pins: vec![PinRef {
                    refdes: "R1".into(),
                    pin: "1".into(),
                }],
            }],
        };
        Board::from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures()).unwrap()
    }

    /// The distance from a part's courtyard to the nearest side of the outline.
    fn edge_gap(board: &Board, refdes: &str) -> f64 {
        let c = board.part(refdes).unwrap().courtyard();
        let o = board.outline;
        (c.min.x - o.min.x)
            .min(o.max.x - c.max.x)
            .min(c.min.y - o.min.y)
            .min(o.max.y - c.max.y)
    }

    #[test]
    fn a_connector_is_at_an_edge_of_the_board() {
        let board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let gap = edge_gap(&board, "J1");
        let wall = board.rules.edge_wall();
        assert!((wall - 1e-9..wall + 3.0).contains(&gap), "{gap} mm");
        assert_eq!(board.check_placement(), vec![]);
    }

    #[test]
    fn connectors_go_to_the_edges_and_the_other_parts_between_them() {
        let board = board_of(vec![
            part("J1", "Header1x2", None),
            part("J2", "Header1x2", None),
            part("J3", "Header1x2", None),
            part("R1", "Resistor", Some("0402")),
        ]);
        for j in ["J1", "J2", "J3"] {
            let gap = edge_gap(&board, j);
            assert!(
                gap < board.rules.edge_wall() + 3.0,
                "{j} is {gap} mm from the edge"
            );
        }
        assert_eq!(board.check_placement(), vec![]);
    }

    #[test]
    fn the_outline_is_on_whole_millimetres_and_inside_the_sheet() {
        let board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        assert_eq!(board.outline.min, ORIGIN);
        assert_eq!(board.outline.width().fract(), 0.0);
        assert_eq!(board.outline.height().fract(), 0.0);
    }

    #[test]
    fn many_parts_make_a_board_that_is_wide_as_well_as_tall() {
        // 120 parts of 0805 take about 1300 mm2 with their gaps, a square of 36 mm.
        let parts = (1..=120)
            .map(|i| part(&format!("R{i}"), "Resistor", Some("0805")))
            .collect();
        let board = board_of(parts);
        let (w, h) = (board.outline.width(), board.outline.height());
        assert!(
            w > ROW_WIDTH + 2.0 * board.rules.edge_wall(),
            "{w} by {h} mm"
        );
        assert!(h < 2.0 * w, "{w} by {h} mm");
        assert_eq!(board.check_placement(), vec![]);
    }

    fn placed_with(
        parts: Vec<Part>,
        options: &PlaceOptions,
    ) -> Result<Board, crate::board::BoardError> {
        let design = Design {
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            parts,
            nets: vec![Net {
                name: "N".into(),
                pins: vec![PinRef {
                    refdes: "R1".into(),
                    pin: "1".into(),
                }],
            }],
        };
        Board::from_design_placed(
            &design,
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
            options,
        )
    }

    fn resistors(n: usize, package: &str) -> Vec<Part> {
        (1..=n)
            .map(|i| part(&format!("R{i}"), "Resistor", Some(package)))
            .collect()
    }

    fn back(refdes: &[&str]) -> PlaceOptions {
        PlaceOptions {
            sides: refdes.iter().map(|r| (r.to_string(), Side::Back)).collect(),
            ..PlaceOptions::default()
        }
    }

    #[test]
    fn parts_the_placement_puts_on_the_back_are_there_and_the_two_faces_each_have_no_overlap() {
        let board = placed_with(resistors(40, "0603"), &back(&["R1", "R2", "R3", "R4"])).unwrap();
        for r in ["R1", "R2", "R3", "R4"] {
            assert_eq!(board.part(r).unwrap().side, Side::Back, "{r}");
        }
        assert_eq!(board.part("R5").unwrap().side, Side::Front);
        assert_eq!(board.check_placement(), vec![]);
    }

    #[test]
    fn a_part_on_the_back_shares_board_area_with_the_front() {
        // 30 parts on the front alone make rows; the same 30 split in two take half the
        // height, since the faces are laid out in the same area.
        let all = placed_with(resistors(60, "0603"), &PlaceOptions::default()).unwrap();
        let names: Vec<String> = (1..=60).step_by(2).map(|i| format!("R{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let split = placed_with(resistors(60, "0603"), &back(&refs)).unwrap();
        assert!(
            split.outline.width() * split.outline.height()
                < 0.75 * all.outline.width() * all.outline.height(),
            "{:?} against {:?}",
            split.outline,
            all.outline
        );
        assert_eq!(split.check_placement(), vec![]);
    }

    #[test]
    fn courtyards_of_the_two_faces_may_overlap_but_not_over_the_pads_of_a_hole_part() {
        // Through-hole headers (also kept as connectors) among resistors, with resistors on
        // the back all round. The header draws its courtyard on F.CrtYd only, so the back
        // is judged by its pads (ADR 0031, provisional).
        let mut parts = resistors(12, "0603");
        parts.push(part("J1", "Header1x2", None));
        parts.push(part("J2", "Header1x2", None));
        let names: Vec<String> = (1..=12).map(|i| format!("R{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let board = placed_with(parts, &back(&refs)).unwrap();
        for j in ["J1", "J2"] {
            let header = board.part(j).unwrap();
            assert_eq!(header.footprint.courtyard_faces, (true, false));
            for r in &refs {
                for pad in &header.footprint.pads {
                    let near = header.pad_bounds(pad).expanded(board.rules.clearance);
                    assert!(
                        !board.part(r).unwrap().courtyard().overlaps(&near),
                        "{r} over a pad of {j}"
                    );
                }
            }
        }
        assert_eq!(board.check_placement(), vec![]);
    }

    #[test]
    fn a_given_size_is_the_outline_and_the_parts_are_inside_it() {
        let options = PlaceOptions {
            fit: Some((50.0, 40.0)),
            ..PlaceOptions::default()
        };
        let board = placed_with(resistors(20, "0603"), &options).unwrap();
        assert_eq!(
            (board.outline.width(), board.outline.height()),
            (50.0, 40.0)
        );
        assert_eq!(board.outline.min, ORIGIN);
        assert_eq!(board.check_placement(), vec![]);
    }

    #[test]
    fn parts_that_do_not_fit_the_front_share_the_faces_and_say_so_if_that_is_not_enough() {
        let mut options = PlaceOptions {
            fit: Some((30.0, 20.0)),
            ..PlaceOptions::default()
        };
        // 100 0603s do not fit 30 by 20 mm on one face, and do on two.
        let alone = placed_with(
            resistors(100, "0603"),
            &PlaceOptions {
                fit: Some((30.0, 60.0)),
                ..PlaceOptions::default()
            },
        )
        .unwrap();
        assert!(alone.parts.iter().all(|p| p.side == Side::Front));
        let board = placed_with(resistors(100, "0603"), &options).unwrap();
        let on_back = board.parts.iter().filter(|p| p.side == Side::Back).count();
        assert!(on_back > 20 && on_back < 60, "{on_back} on the back");
        assert_eq!(board.check_placement(), vec![]);
        assert_eq!(board.outline.height(), 20.0);

        // Too many even for two faces: the message names the numbers.
        options.fit = Some((15.0, 10.0));
        let message = placed_with(resistors(40, "0603"), &options)
            .unwrap_err()
            .to_string();
        // The courtyards of 40 parts are not more than the two faces hold, so it is not a
        // certain no: the message says which parts are out and by how much, and not what the
        // rows need.
        assert!(
            message.contains(" part(s) do not fit the outline of 15.0 x 10.0 mm: ")
                && message.contains("outside the outline or inside its margin, by mm past")
                && !message.contains("the parts need")
                && !message.contains("mm over in"),
            "{message}"
        );
        // Twice that is more courtyard than both faces hold: a certain no, and the message
        // gives the aggregate and the areas.
        let message = placed_with(resistors(100, "0603"), &options)
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("the parts need")
                && message.contains("the outline is 15.0 x 10.0 mm")
                && message.contains("mm over in y")
                && message.contains("the courtyards add up to ")
                && message.contains(" part(s) do not fit the outline of 15.0 x 10.0 mm: "),
            "{message}"
        );
        // A board 3 mm wide is narrower than an 0603 as it lies but not as it is turned: the
        // placer turns it (it used to refuse, because the rows never turn a part).
        options.fit = Some((3.0, 200.0));
        let board = placed_with(resistors(4, "0603"), &options).unwrap();
        assert!(
            board
                .parts
                .iter()
                .all(|p| p.rotation.rem_euclid(180.0) == 90.0)
        );
        assert_eq!(board.check_placement(), vec![]);
        // A board narrower than a part in every turn names the part and its courtyard in
        // each turn, and not the rows' size.
        options.fit = Some((1.0, 200.0));
        let message = placed_with(resistors(4, "0603"), &options)
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("R1 (Resistor) has a courtyard of")
                && message.contains("turned 90 degrees")
                && !message.contains("the parts need"),
            "{message}"
        );
    }

    #[test]
    fn a_part_too_long_for_the_outline_starts_turned_and_one_that_may_not_turn_does_not() {
        let board = placed_with(resistors(3, "0603"), &PlaceOptions::default()).unwrap();
        let mut board = board;
        board.parts.iter_mut().for_each(|p| p.rotation = 0.0);
        let mut parts = board.parts.clone();
        // R1 is 40 mm long, R2 as well but locked, R3 small.
        for p in &mut parts[..2] {
            p.footprint.courtyard = Rect::new(Pt::new(-20.0, -2.0), Pt::new(20.0, 2.0));
        }
        let mut options = PlaceOptions {
            fit: Some((20.0, 60.0)),
            ..PlaceOptions::default()
        };
        let fixed = [false, true, false];
        turn_to_fit(&mut parts, &fixed, &options);
        let turns: Vec<f64> = parts.iter().map(|p| p.rotation).collect();
        assert_eq!(turns, vec![90.0, 0.0, 0.0]);
        assert!((parts[0].courtyard().height() - 40.0).abs() < 1e-9);
        // Turned again it is not turned further, and in an outline it fits as it is nothing
        // changes.
        turn_to_fit(&mut parts, &fixed, &options);
        assert_eq!(parts[0].rotation, 90.0);
        options.fit = Some((60.0, 20.0));
        let mut again = board.parts.clone();
        again[0].footprint.courtyard = Rect::new(Pt::new(-20.0, -2.0), Pt::new(20.0, 2.0));
        turn_to_fit(&mut again, &[false; 3], &options);
        assert_eq!(again[0].rotation, 0.0);
        // A part that fits no turn is left alone: the error names it (see `oversize`).
        options.fit = Some((20.0, 30.0));
        turn_to_fit(&mut again, &[false; 3], &options);
        assert_eq!(again[0].rotation, 0.0);
        let found = oversize(&again, &[false; 3], &options, (20.0, 30.0));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].part, "R1");
        assert_eq!(found[0].tried.len(), 4);
    }

    #[test]
    fn rows_that_overflow_the_size_do_not_decide_that_the_parts_do_not_fit() {
        // The rows put 44 0603s in lines 26 mm long, which need more than 20 mm; the annealer
        // packs them closer and they all fit on the front.
        let options = PlaceOptions {
            fit: Some((30.0, 20.0)),
            ..PlaceOptions::default()
        };
        let board = placed_with(resistors(44, "0603"), &options).unwrap_or_else(|e| panic!("{e}"));
        assert!(board.parts.iter().all(|p| p.side == Side::Front));
        assert_eq!(board.check_placement(), vec![]);
        // The rows alone are too big for it.
        let mut parts = board.parts.clone();
        let rows = arrange_free(&mut parts, &[false; 44], &options);
        assert!(rows.needs.1 > 20.0, "the rows need {:?} mm", rows.needs);
    }

    #[test]
    fn a_fixed_part_that_hangs_over_counts_as_its_courtyard_clipped_to_the_outline() {
        let board = board_of(vec![
            part("J1", "Header1x2", None),
            part("R1", "Resistor", Some("0402")),
        ]);
        let mut parts: Vec<Placed> = board
            .parts
            .iter()
            .filter(|p| p.refdes == "J1")
            .cloned()
            .collect();
        // The courtyard's bottom is 3 mm past the bottom edge of a 35 x 60 mm outline.
        let c = parts[0].courtyard();
        let at = parts[0].at;
        parts[0].at = Pt::new(
            at.x + ORIGIN.x + 5.0 - c.min.x,
            at.y + ORIGIN.y + 63.0 - c.max.y,
        );
        let court = parts[0].courtyard();
        let mut options = PlaceOptions {
            fit: Some((35.0, 60.0)),
            ..PlaceOptions::default()
        };
        let fixed = [true];
        // Unstated, it needs all of its courtyard and the margin.
        let needs = fixed_needs(&parts, &fixed, &options).unwrap();
        assert!(
            (needs.1 - (63.0 + Rules::DEFAULT_EDGE_WALL)).abs() < 1e-9,
            "{needs:?}"
        );
        // Stated for the bottom edge, it needs the board to its edge and no margin there; the
        // side that is on the board keeps its margin.
        let mut edges = crate::overhang::Edges::default();
        edges.set(crate::overhang::Edge::Bottom);
        options.overhang.insert(
            "J1".into(),
            Overhang::Stated {
                edges,
                says: String::new(),
            },
        );
        let needs = fixed_needs(&parts, &fixed, &options).unwrap();
        assert!((needs.1 - 60.0).abs() < 1e-9, "{needs:?}");
        assert!(
            (needs.0 - (court.max.x - ORIGIN.x + Rules::DEFAULT_EDGE_WALL)).abs() < 1e-9,
            "{needs:?}"
        );
        // Stated for another edge, nothing is waived.
        edges = crate::overhang::Edges::default();
        edges.set(crate::overhang::Edge::Left);
        options.overhang.insert(
            "J1".into(),
            Overhang::Stated {
                edges,
                says: String::new(),
            },
        );
        let needs = fixed_needs(&parts, &fixed, &options).unwrap();
        assert!(
            (needs.1 - (60.0 + Rules::DEFAULT_EDGE_WALL)).abs() < 1e-9,
            "{needs:?}"
        );
    }

    #[test]
    fn parts_the_caller_placed_stay_where_they_were_put_when_the_rest_share_the_faces() {
        let options = PlaceOptions {
            fit: Some((30.0, 20.0)),
            sides: BTreeMap::from([("R1".to_string(), Side::Front)]),
            ..PlaceOptions::default()
        };
        let board = placed_with(resistors(40, "0603"), &options).unwrap();
        assert_eq!(board.part("R1").unwrap().side, Side::Front);
    }

    #[test]
    fn parts_that_do_not_fit_are_listed_by_name_with_the_area_and_the_side_of_the_outline() {
        let options = PlaceOptions {
            fit: Some((14.0, 12.0)),
            ..PlaceOptions::default()
        };
        let text = placed_with(resistors(60, "0805"), &options)
            .unwrap_err()
            .to_string();
        let after = |header: &str| -> Vec<&str> {
            let (_, rest) = text
                .split_once(header)
                .unwrap_or_else(|| panic!("{header}: {text}"));
            rest.lines()
                .skip(1)
                .take_while(|l| l.starts_with("    "))
                .collect()
        };
        let pairs = after("courtyards overlapping (");
        let described: Vec<&&str> = pairs.iter().filter(|l| l.contains(" mm2 (")).collect();
        assert_eq!(described.len(), DESCRIBED, "{text}");
        assert!(
            described[0].contains(" and R") && described[0].contains(" mm from the "),
            "{}",
            described[0]
        );
        // The most overlapped come first.
        let area = |l: &str| -> f64 {
            let (_, rest) = l.split_once(": ").unwrap();
            rest.split(" mm2").next().unwrap().parse().unwrap()
        };
        assert!(
            area(described[0]) >= area(described[DESCRIBED - 1]),
            "{text}"
        );
        // The rest are named, so that the message holds the whole list.
        let counted = text
            .split_once(" pair(s) of courtyards")
            .and_then(|(head, _)| head.rsplit(' ').next())
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap();
        let more = pairs.last().unwrap();
        assert!(
            more.contains(&format!("and {} more: R", counted - DESCRIBED)),
            "{more}"
        );
        let parts = after("parts outside the outline or inside its margin (");
        assert!(
            parts
                .iter()
                .filter(|l| !l.starts_with("    and "))
                .all(|l| l.contains(" mm outside the ")
                    || (l.contains(" mm inside the ") && l.contains(" mm margin at the "))),
            "{parts:?}"
        );
    }

    #[test]
    fn rows_keep_clear_of_a_keepout_when_there_is_a_size() {
        let keepout = Rect::new(Pt::new(25.0, 22.0), Pt::new(29.0, 30.0));
        let options = PlaceOptions {
            fit: Some((60.0, 40.0)),
            keepouts: vec![keepout],
            ..PlaceOptions::default()
        };
        let board = placed_with(resistors(30, "0805"), &options).unwrap();
        for p in &board.parts {
            assert!(
                !p.courtyard().overlaps(&keepout),
                "{} is over the hole",
                p.refdes
            );
        }
        assert_eq!(board.check_placement(), vec![]);
    }

    #[test]
    fn a_placement_naming_a_part_the_design_lacks_is_refused() {
        let message = placed_with(resistors(2, "0603"), &back(&["R9"]))
            .unwrap_err()
            .to_string();
        assert!(message.contains("R9"), "{message}");
    }

    #[test]
    fn a_design_with_nothing_on_the_back_is_placed_as_before() {
        // The same arrangement with and without the options is the rule that keeps the
        // examples' files as they were.
        let plain = board_of(resistors(30, "0603"));
        let with = placed_with(resistors(30, "0603"), &PlaceOptions::default()).unwrap();
        assert_eq!(plain.parts, with.parts);
        assert_eq!(plain.outline, with.outline);
    }

    #[test]
    fn a_board_with_one_part_still_places_it() {
        let board = board_of(vec![part("R1", "Resistor", Some("0402"))]);
        assert_eq!(board.check_placement(), vec![]);
    }

    // The placer derived from the circuit (ADR 0033).

    fn power_pin(name: &str, pad: &str) -> netsmith_core::PartPin {
        netsmith_core::PartPin {
            name: name.into(),
            datasheet: name.to_uppercase(),
            kind: netsmith_core::PinKind::Power,
            pads: vec![pad.into()],
            function: None,
        }
    }

    fn wire(name: &str, pins: &[(&str, &str)]) -> Net {
        Net {
            name: name.into(),
            pins: pins
                .iter()
                .map(|(r, p)| PinRef {
                    refdes: (*r).into(),
                    pin: (*p).into(),
                })
                .collect(),
        }
    }

    fn at_path(mut p: Part, path: &str) -> Part {
        p.path = path.into();
        p
    }

    fn board_from(parts: Vec<Part>, nets: Vec<Net>, options: &PlaceOptions) -> Board {
        let design = Design {
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            parts,
            nets,
        };
        Board::from_design_placed(
            &design,
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
            options,
        )
        .unwrap_or_else(|e| panic!("{e}"))
    }

    /// A chain of resistors `prefix1`..`prefixN` in the module `module`, each joined to the
    /// next, and the nets that join them.
    fn chain(prefix: &str, module: &str, n: usize) -> (Vec<Part>, Vec<Net>) {
        let parts: Vec<Part> = (1..=n)
            .map(|i| {
                at_path(
                    part(&format!("{prefix}{i}"), "Resistor", Some("0603")),
                    &format!("{module}.{prefix}{i}"),
                )
            })
            .collect();
        let nets = (1..n)
            .map(|i| {
                wire(
                    &format!("{prefix}n{i}"),
                    &[
                        (&format!("{prefix}{i}"), "2"),
                        (&format!("{prefix}{}", i + 1), "1"),
                    ],
                )
            })
            .collect();
        (parts, nets)
    }

    #[test]
    fn the_same_input_gives_the_same_board_and_another_seed_another_arrangement() {
        let (parts, nets) = chain("R", "m", 14);
        let board = |seed| {
            board_from(
                parts.clone(),
                nets.clone(),
                &PlaceOptions {
                    seed,
                    ..PlaceOptions::default()
                },
            )
        };
        assert_eq!(board(0), board(0));
        assert_eq!(board(5), board(5));
        let (a, b) = (board(0), board(5));
        assert_ne!(
            a.parts.iter().map(|p| p.at).collect::<Vec<_>>(),
            b.parts.iter().map(|p| p.at).collect::<Vec<_>>()
        );
        assert_eq!(a.check_placement(), vec![]);
        assert_eq!(b.check_placement(), vec![]);
    }

    #[test]
    fn a_chain_of_parts_is_placed_in_a_line_not_the_length_of_the_rows() {
        // Fourteen resistors in a chain: the wire is near the least it can be, a part's
        // width to the next.
        let (parts, nets) = chain("R", "m", 14);
        let board = board_from(parts, nets, &PlaceOptions::default());
        let q = crate::quality::Quality::of(&board);
        assert!(q.ratsnest < 14.0 * 3.5, "ratsnest {:.1} mm", q.ratsnest);
        assert!(q.crossings <= 1, "{} crossings", q.crossings);
    }

    #[test]
    fn parts_turn_in_right_angles_only() {
        let (parts, nets) = chain("R", "m", 20);
        let board = board_from(parts, nets, &PlaceOptions::default());
        for p in &board.parts {
            assert_eq!(p.rotation % 90.0, 0.0, "{} at {}", p.refdes, p.rotation);
        }
    }

    #[test]
    fn the_parts_of_a_module_are_together() {
        // Two modules, each a chain of ten, joined by one net: neither is spread over the
        // board.
        let (mut parts, mut nets) = chain("A", "a", 10);
        let (b_parts, b_nets) = chain("B", "b", 10);
        parts.extend(b_parts);
        nets.extend(b_nets);
        nets.push(wire("link", &[("A10", "2"), ("B1", "1")]));
        let board = board_from(parts, nets, &PlaceOptions::default());
        let hull = |prefix: &str| {
            board
                .parts
                .iter()
                .filter(|p| p.refdes.starts_with(prefix))
                .map(Placed::courtyard)
                .reduce(|a, b| a.union(&b))
                .unwrap()
        };
        let (a, b) = (hull("A"), hull("B"));
        let board_area = board.outline.width() * board.outline.height();
        assert!(
            a.width() * a.height() < 0.6 * board_area && b.width() * b.height() < 0.6 * board_area,
            "{a:?} and {b:?} on a board of {board_area} mm2"
        );
        assert_eq!(board.check_placement(), vec![]);
    }

    fn regulator(refdes: &str, path: &str) -> Part {
        let mut p = at_path(part(refdes, "LinearRegulator", Some("SOT-23")), path);
        p.pins = vec![power_pin("vin", "3"), power_pin("gnd", "1")];
        p
    }

    #[test]
    fn a_decoupling_capacitor_is_next_to_the_pins_it_decouples() {
        // A regulator with power pins, a capacitor between its supply and ground nets, and
        // thirty resistors in a chain that have nothing to do with either.
        let mut parts = vec![regulator("U1", "ldo.u")];
        parts.push(at_path(part("C1", "Capacitor", Some("0603")), "ldo.cin"));
        let (rest, mut nets) = chain("R", "other", 30);
        parts.extend(rest);
        nets.push(wire("IN", &[("U1", "3"), ("C1", "1"), ("R1", "1")]));
        nets.push(wire("GND", &[("U1", "1"), ("C1", "2"), ("R30", "2")]));
        let board = board_from(parts, nets, &PlaceOptions::default());
        let q = crate::quality::Quality::of(&board);
        assert_eq!(q.decoupling.len(), 1);
        assert_eq!(q.decoupling[0].cap, "C1");
        assert_eq!(q.decoupling[0].part, "U1");
        assert!(q.decoupling[0].distance < 3.0, "{:?}", q.decoupling);
        assert_eq!(board.check_placement(), vec![]);
    }

    #[test]
    fn a_capacitor_is_paired_with_the_part_of_its_own_module_first() {
        let mut parts = vec![regulator("U1", "a.u"), regulator("U2", "b.u")];
        parts.push(at_path(part("C1", "Capacitor", Some("0603")), "b.c"));
        parts.push(at_path(part("C2", "Capacitor", Some("0603")), "a.c"));
        let nets = vec![
            wire("IN", &[("U1", "3"), ("U2", "3"), ("C1", "1"), ("C2", "1")]),
            wire("GND", &[("U1", "1"), ("U2", "1"), ("C1", "2"), ("C2", "2")]),
        ];
        let board = board_from(parts, nets, &PlaceOptions::default());
        let pairs = crate::quality::decoupling_pairs(&board.parts, &board.nets);
        let named: Vec<(&str, &str)> = pairs
            .iter()
            .map(|p| {
                (
                    board.parts[p.cap].refdes.as_str(),
                    board.parts[p.nets[0].pins[0].0].refdes.as_str(),
                )
            })
            .collect();
        assert_eq!(named, [("C1", "U2"), ("C2", "U1")]);
    }

    #[test]
    fn a_capacitor_between_two_nets_no_part_has_power_pins_on_is_not_a_decoupling_pair() {
        let mut parts = vec![regulator("U1", "u")];
        parts.push(part("C1", "Capacitor", Some("0603")));
        let nets = vec![
            wire("IN", &[("U1", "3")]),
            wire("A", &[("C1", "1")]),
            wire("B", &[("C1", "2"), ("U1", "2")]),
        ];
        let board = board_from(parts, nets, &PlaceOptions::default());
        assert!(crate::quality::decoupling_pairs(&board.parts, &board.nets).is_empty());
    }

    #[test]
    fn parts_the_layout_fixes_stay_and_the_others_are_placed_round_them() {
        let (parts, nets) = chain("R", "m", 12);
        let mut options = PlaceOptions::default();
        options.fixed.insert(
            "R3".into(),
            Fixed {
                at: Pt::new(20.0, 15.0),
                rotation: 90.0,
                side: Side::Front,
                text: ReferenceSide::Above,
            },
        );
        options.fixed.insert(
            "R4".into(),
            Fixed {
                at: Pt::new(23.0, 15.0),
                rotation: 0.0,
                side: Side::Front,
                text: ReferenceSide::Above,
            },
        );
        let board = board_from(parts, nets, &options);
        let r3 = board.part("R3").unwrap();
        assert_eq!((r3.at, r3.rotation), (ORIGIN + Pt::new(20.0, 15.0), 90.0));
        assert_eq!(board.part("R4").unwrap().at, ORIGIN + Pt::new(23.0, 15.0));
        assert!(board.outline.contains(&r3.courtyard()));
        assert_eq!(board.check_placement(), vec![]);
    }

    #[test]
    fn a_subset_is_placed_in_its_region_and_the_rest_stays() {
        let (mut parts, mut nets) = chain("A", "a", 8);
        let (b_parts, b_nets) = chain("B", "b", 8);
        parts.extend(b_parts);
        nets.extend(b_nets);
        nets.push(wire("link", &[("A8", "2"), ("B1", "1")]));
        let board = board_from(parts, nets, &PlaceOptions::default());
        let names: Vec<&str> = (1..=8)
            .map(|i| ["B1", "B2", "B3", "B4", "B5", "B6", "B7", "B8"][i - 1])
            .collect();
        let hull = names
            .iter()
            .map(|r| board.part(r).unwrap().courtyard())
            .reduce(|a, b| a.union(&b))
            .unwrap();
        let region = hull.expanded(6.0);
        let mut again = board.clone();
        place_subset(&mut again, &names, region, 1).unwrap();
        for p in &board.parts {
            let q = again.part(&p.refdes).unwrap();
            if p.refdes.starts_with('A') {
                assert_eq!((p.at, p.rotation), (q.at, q.rotation), "{}", p.refdes);
            } else {
                assert!(
                    region.contains(&q.courtyard()),
                    "{} left the region",
                    p.refdes
                );
            }
        }
        assert_eq!(again.check_placement(), vec![]);
        // The same seed gives the same arrangement; another gives another.
        let mut twice = board.clone();
        place_subset(&mut twice, &names, region, 1).unwrap();
        assert_eq!(again, twice);
        let mut other = board.clone();
        place_subset(&mut other, &names, region, 2).unwrap();
        assert_ne!(again, other);
        assert_eq!(other.check_placement(), vec![]);
    }

    #[test]
    fn a_subset_that_does_not_fit_its_region_says_so_and_changes_nothing() {
        let (parts, nets) = chain("R", "m", 12);
        let board = board_from(parts, nets, &PlaceOptions::default());
        let names = ["R1", "R2", "R3", "R4", "R5", "R6"];
        let c = board.part("R1").unwrap().courtyard();
        let region = Rect::new(c.min, Pt::new(c.max.x + 0.5, c.max.y + 0.5));
        let mut tried = board.clone();
        let message = place_subset(&mut tried, &names, region, 0)
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("6 part(s)") && message.contains("do not fit the region"),
            "{message}"
        );
        assert_eq!(tried, board);
        let missing = place_subset(&mut tried, &["R99"], region, 0)
            .unwrap_err()
            .to_string();
        assert_eq!(missing, "the board has no part `R99` to place");
    }

    #[test]
    fn reference_text_is_kept_off_the_pads_and_the_text_of_other_parts() {
        let (parts, nets) = chain("R", "m", 30);
        let board = board_from(parts, nets, &PlaceOptions::default());
        let least = board.rules.silk_to_pad;
        for a in &board.parts {
            let text = anneal::text_box(a, &board.rules).unwrap();
            for b in board.parts.iter().filter(|b| b.refdes != a.refdes) {
                for pad in &b.footprint.pads {
                    assert!(
                        !text.overlaps(&b.pad_bounds(pad).expanded(least)),
                        "the text of {} is on a pad of {}",
                        a.refdes,
                        b.refdes
                    );
                }
                let other = anneal::text_box(b, &board.rules).unwrap();
                assert!(
                    !text.overlaps(&other),
                    "the text of {} is over the text of {}",
                    a.refdes,
                    b.refdes
                );
            }
        }
    }

    #[test]
    fn the_arrangement_of_a_small_board_is_a_fixed_one() {
        // Positions are whole units and nothing in the run depends on the machine, so these
        // are the same everywhere. A change to the cost or the moves changes them: say so in
        // the commit, and write the new ones.
        let (parts, nets) = chain("R", "m", 6);
        let board = board_from(parts, nets, &PlaceOptions::default());
        let got: Vec<(f64, f64, f64)> = board
            .parts
            .iter()
            .map(|p| (p.at.x, p.at.y, p.rotation))
            .collect();
        assert_eq!(
            got,
            vec![
                (47.4, 21.65, 180.0),
                (43.45, 22.35, 180.0),
                (40.1, 22.05, 180.0),
                (34.5, 21.65, 180.0),
                (28.7, 22.35, 180.0),
                (25.1, 22.3, 180.0)
            ]
        );
        assert_eq!(
            board.outline,
            Rect::new(Pt::new(20.0, 20.0), Pt::new(50.0, 24.0))
        );
    }
}
