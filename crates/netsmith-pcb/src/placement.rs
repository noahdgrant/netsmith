//! What the placer is told and what it answers with (ADR 0033, ADR 0045): the options a
//! caller and the layout file give it ([`PlaceOptions`]), the parts it must leave where they
//! are ([`Fixed`]), the zones of height limits, the refusal when the parts do not fit
//! ([`DoesNotFit`]) with the state the placer ended in ([`Ended`]), and the check of the parts
//! too big for the outline in every turn, which is made before any placing as well
//! ([`oversize_before_placing`]).
//!
//! The placer itself is the crate `netsmith-place-route`. These are here, beside the board
//! model, because the board, its requirements, the layout file and the errors are made of them
//! (issue #518).

use std::collections::BTreeMap;

use crate::board::{Placed, ReferenceSide, Side};
use crate::constraints::Want;
use crate::geometry::{Pt, Rect};
use crate::overhang::Overhang;
use crate::rules::Rules;

/// The outline's top left corner on the sheet, so the board sits inside a KiCad page.
pub const ORIGIN: Pt = Pt { x: 20.0, y: 20.0 };

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
    /// board's process, which `Board::load` fills in. `None`: 2.0 mm
    /// ([`Rules::DEFAULT_EDGE_WALL`]).
    pub wall: Option<f64>,
    /// The areas that limit how tall a part may stand, in the board's coordinates (ADR 0045,
    /// "Height limits"). A part with a stated height above a zone's limit keeps
    /// its body (grown by a small margin; its courtyard if the footprint draws no fabrication
    /// outline) out of the zone on the faces the zone limits; a part with no stated
    /// height is not kept out (nobody knows whether it fits; the check reports it).
    pub height_zones: Vec<HeightZone>,
    /// The keepouts that a part carries (ADR 0029, ADR 0045 section 11): each moves and turns
    /// with its part, so the annealer keeps the other parts out of it where the part is, as
    /// the check does (`Board::check_placement`).
    pub carried: Vec<CarriedZone>,
    /// Constraints are warnings for this run (`--constraints warn`, ADR 0045): lines that the
    /// check before placing proves cannot all hold are listed as warnings and the placer runs
    /// as ever, instead of stopping the board. A courtyard past an edge nobody stated, or a
    /// part bigger than the outline, stays an error: it is not a requirement.
    pub lines_warn: bool,
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

/// A keepout that a part carries: its box is in the part's own frame (the footprint's), and
/// turns, mirrors and moves with the part. It keeps other parts out the way a keepout for
/// parts does (`parts`) and limits their height the way a height zone does (`limit`).
#[derive(Debug, Clone, PartialEq)]
pub struct CarriedZone {
    /// The keepout it comes from.
    pub name: String,
    /// The path of the part that carries it.
    pub part: String,
    /// The box in the part's own frame.
    pub local: Rect,
    /// Whether it forbids parts.
    pub parts: bool,
    /// The faces its height limit applies to.
    pub front: bool,
    pub back: bool,
    /// The tallest a part may stand in it, in mm, if it limits height.
    pub limit: Option<f64>,
}

impl CarriedZone {
    /// Whether a part of `height` mm on `side` may not stand in it for its height: the
    /// limit is below the height, on a face the limit applies to. A part with no stated
    /// height is not ruled out (nobody knows whether it fits; the check reports it).
    pub fn limits(&self, height: Option<f64>, side: Side) -> bool {
        let (Some(limit), Some(height)) = (self.limit, height) else {
            return false;
        };
        let on = if side == Side::Back {
            self.back
        } else {
            self.front
        };
        on && height > limit + 0.0005
    }
}

impl HeightZone {
    /// Whether a part of `height` mm on `side` may not stand in the zone.
    pub fn rules_out(&self, height: f64, side: Side) -> bool {
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
    pub fn wall(&self) -> f64 {
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
    /// the lines that cannot all hold where that is proved (ADR 0045). Filled
    /// by `netsmith_place_route::board_from_design_placed`, which knows the design; `None` before, and for a
    /// design that states no requirement.
    pub requirements: Option<Box<crate::explain::RequirementReport>>,
}

/// One part's place and state at the end of a placement that did not fit.
#[derive(Debug, Clone, PartialEq)]
pub struct PartState {
    pub part: String,
    /// The member's path in the design (empty for a part that has none).
    pub path: String,
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
    /// The part's path in the design.
    pub path: String,
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
            crate::board::member(&self.path, &self.part),
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
        let tries = self.ended.as_ref().map_or(0, |e| e.tries);
        if tries > 1 {
            write!(
                f,
                "\n  the placer made {tries} tries (the first, then {} more from other seeds derived from the given one, one of them with the largest parts put in corners first); this is the closest",
                tries - 1
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

    /// What blocks the placement where the placer ended, one sentence each with the members by
    /// path (the designator beside) and by how much: the courtyards that overlap with the area
    /// they share, the parts outside what is kept clear with the distance, the relations not
    /// held and the requirements not met. For a board that no seed places (issue #458).
    pub fn blockers(&self) -> Vec<String> {
        let name = |refdes: &str| {
            let path = self
                .parts
                .iter()
                .find(|p| p.part == refdes)
                .map_or("", |p| p.path.as_str());
            crate::board::member(path, refdes)
        };
        let mut lines = Vec::new();
        for o in &self.oversize {
            lines.push(format!(
                "{} is bigger than the outline in every turn it may take",
                name(&o.part)
            ));
        }
        if let Some(e) = &self.ended {
            for o in &e.overlapping {
                lines.push(format!(
                    "the courtyards of {} and {} overlap by {:.2} mm2",
                    name(&o.a),
                    name(&o.b),
                    o.area
                ));
            }
            for o in &e.outside_parts {
                if o.why.is_empty() {
                    lines.push(format!("{} is outside what is kept clear", name(&o.part)));
                } else {
                    lines.push(format!("{}: {}", name(&o.part), o.why.join("; ")));
                }
            }
            lines.extend(e.relations.iter().cloned());
        }
        if let Some(report) = &self.requirements {
            lines.extend(report.unmet.iter().cloned());
        }
        lines
    }

    /// How badly the placer missed, for choosing the closest of several tries: the number of
    /// parts and relations that break a rule, then the sum of the distances and areas by which
    /// they do. A refusal the placer did not try to place has the worst.
    pub fn badness(&self) -> (usize, f64) {
        match &self.ended {
            Some(e) if self.oversize.is_empty() => e.badness(),
            _ => (usize::MAX, f64::INFINITY),
        }
    }

    /// Whether another seed could place the parts: the placer tried the outline and no part
    /// is bigger than it in every turn (the courtyards' area alone is no certain answer, since
    /// the faces may be shared).
    pub fn seed_may_help(&self) -> bool {
        self.ended.is_some() && self.oversize.is_empty() && self.area.0 <= self.area.1 + 1e-9
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
                crate::board::member(&p.path, &p.part),
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
pub const DESCRIBED: usize = 10;

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
                e.member(&o.a),
                e.member(&o.b),
                short(o.area),
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
                .map(|o| format!("{} and {}", e.member(&o.a), e.member(&o.b)))
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
            write!(f, "\n    {}: {}", e.member(&o.part), o.why.join("; "))?;
        }
        if e.outside_parts.len() > DESCRIBED {
            let rest: Vec<String> = e.outside_parts[DESCRIBED..]
                .iter()
                .map(|o| e.member(&o.part))
                .collect();
            write!(f, "\n    and {} more: {}", rest.len(), rest.join(", "))?;
        }
    }
    Ok(())
}

/// The room for a courtyard inside an outline of `fit`: the outline less the margin on each
/// side.
pub fn inside(fit: (f64, f64), options: &PlaceOptions) -> (f64, f64) {
    (
        (fit.0 - 2.0 * options.wall()).max(0.0),
        (fit.1 - 2.0 * options.wall()).max(0.0),
    )
}

/// Whether a part's turn is the placer's to choose: the layout file does not lock it, its
/// requirements leave all four turns (a requirement on a position or a face does not limit
/// them; one on the rotation does, and the annealer keeps to it) and it may not hang over an
/// edge (its courtyard is allowed past it, so it is not turned to fit).
pub fn turn_is_free(part: &Placed, fixed: bool, options: &PlaceOptions) -> bool {
    !fixed
        && !options.overhang.contains_key(&part.refdes)
        && options
            .wants
            .get(&part.refdes)
            .is_none_or(|want| want.reach.iter().all(Option::is_some))
}

/// The parts that do not fit `fit` in any turn they may take, with every turn tried and the
/// courtyard's size in it. A part that may hang over an edge is not one.
pub fn oversize(
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
                path: part.path.clone(),
                kind: part.kind.clone(),
                tried,
                free: turn_is_free(part, fixed[i], options),
            });
        }
    }
    found
}

/// [`oversize`] for the parts as loaded, before any placing (ADR 0045 section 14):
/// the parts that are too big for an outline of `fit` in every turn their requirements and the
/// layout file leave them. `options` must already hold the plan of the requirements (the pins,
/// the wants and the overhangs). The mechanical parts are left out: they go to the corners.
pub fn oversize_before_placing(
    parts: &[Placed],
    options: &PlaceOptions,
    fit: (f64, f64),
) -> Vec<Oversize> {
    let mut parts: Vec<Placed> = parts
        .iter()
        .filter(|p| !crate::placement::is_mechanical(&p.kind))
        .cloned()
        .collect();
    let fixed: Vec<bool> = parts
        .iter_mut()
        .map(|p| match options.fixed.get(&p.refdes) {
            Some(f) => {
                p.rotation = f.rotation;
                p.side = f.side;
                true
            }
            None => false,
        })
        .collect();
    oversize(&parts, &fixed, options, fit)
}

/// How many hard rules the final state of a run breaks, and which parts break them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ended {
    /// Pairs of parts, on the same copper, whose courtyards overlap.
    pub overlaps: usize,
    /// Parts that are outside the outline or nearer its edge than the margin, or in a keepout.
    pub outside: usize,
    /// Pairs of parts whose reference text or silkscreen lie over the other's pads.
    pub silk: usize,
    /// The overlapping pairs by name, the most overlapped first.
    pub overlapping: Vec<OverlapAt>,
    /// The parts that are outside, by name, the furthest first.
    pub outside_parts: Vec<OutsideAt>,
    /// The relations between two parts that do not hold, each as a sentence with the numbers
    /// (ADR 0045, section 3).
    pub relations: Vec<String>,
    /// How many times the placer tried before it gave up: the first run, then runs from the
    /// same start with a large part put in a corner first and with seeds derived from the
    /// given one (B2 and B5, PROVISIONAL). The rest of this is the closest run's.
    pub tries: usize,
    /// The path in the design of each part named here, by reference designator, for the
    /// messages: a member is named by its path with the designator beside.
    pub paths: std::collections::BTreeMap<String, String>,
}

impl Ended {
    /// How badly the run missed, for choosing among several: the number of parts and
    /// relations that break a rule, then the sum of the distances and areas by which they do.
    pub fn badness(&self) -> (usize, f64) {
        let count = self.names().len() + self.relations.len() + self.silk;
        let amount = self.outside_parts.iter().map(|o| o.worst).sum::<f64>()
            + self.overlapping.iter().map(|o| o.area).sum::<f64>();
        (count, amount)
    }

    /// A part named here as a message names it: `` `stage.c1` (C1) ``, or the designator alone
    /// when its path is not known.
    pub fn member(&self, refdes: &str) -> String {
        crate::board::member(self.paths.get(refdes).map_or("", String::as_str), refdes)
    }

    /// The parts that break a rule when the run ended: those outside first, the furthest
    /// first, then those only in an overlapping pair; each once.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let others = self
            .overlapping
            .iter()
            .flat_map(|o| [o.a.as_str(), o.b.as_str()]);
        for name in self
            .outside_parts
            .iter()
            .map(|o| o.part.as_str())
            .chain(others)
        {
            if !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
        names
    }

    /// Every part that is out, with how far, and every overlapping pair with its area, in
    /// two lists on one line, the worst first: `outside the outline or inside its margin, by
    /// mm past what is kept clear (the margin from the edge, a keepout, a pad's distance):
    /// R21 0.85, R3 0.80; courtyards overlapping, by mm2: R1 and R2 10.00` (each part by its
    /// path, with the designator beside, when the path is known).
    pub fn compact(&self) -> String {
        let mut lists = Vec::new();
        if !self.outside_parts.is_empty() {
            let items: Vec<String> = self
                .outside_parts
                .iter()
                .map(|o| {
                    if o.worst > 1e-6 {
                        format!("{} {}", self.member(&o.part), short(o.worst))
                    } else {
                        // Nothing past a line: the reference text, or a requirement.
                        format!("{} (text or a requirement)", self.member(&o.part))
                    }
                })
                .collect();
            lists.push(format!(
                "outside the outline or inside its margin, by mm past what is kept clear: {}",
                items.join(", ")
            ));
        }
        if !self.overlapping.is_empty() {
            let items: Vec<String> = self
                .overlapping
                .iter()
                .map(|o| {
                    format!(
                        "{} and {} {}",
                        self.member(&o.a),
                        self.member(&o.b),
                        short(o.area)
                    )
                })
                .collect();
            lists.push(format!(
                "courtyards overlapping, by mm2: {}",
                items.join(", ")
            ));
        }
        if lists.is_empty() {
            "a rule broken that no count names".to_string()
        } else {
            lists.join("; ")
        }
    }
}

/// Two courtyards that overlap when a run ended.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlapAt {
    pub a: String,
    pub b: String,
    /// The area they share, in mm2, and its size along x and along y.
    pub area: f64,
    pub size: (f64, f64),
    /// Where the overlap is, in the outline's frame (its top left corner is 0, 0), and the
    /// side of the outline it is nearest, with the distance to it.
    pub at: (f64, f64),
    pub nearest: (&'static str, f64),
}

/// A part that is outside the outline, inside its margin, or in a keepout when a run ended.
#[derive(Debug, Clone, PartialEq)]
pub struct OutsideAt {
    pub part: String,
    /// What is wrong with each direction, as a sentence: `0.80 mm outside the left edge`.
    pub why: Vec<String>,
    /// The worst of them in mm, for the order.
    pub worst: f64,
}

/// A distance in mm as it is shown for a shortfall: two decimals, but more where two would
/// round a real amount to `0.00` (a part 0.004 mm past a line is past it).
pub fn short(v: f64) -> String {
    if v >= 0.0095 {
        format!("{v:.2}")
    } else if v >= 0.00095 {
        format!("{v:.3}")
    } else if v >= 0.00005 {
        format!("{v:.4}")
    } else {
        "<0.0001".to_string()
    }
}

/// Whether a kind of part is a mechanical item that belongs in the corners of a fixed
/// outline: a hole or a fiducial. A test point is on a net and is placed with the circuit.
pub fn is_mechanical(kind: &str) -> bool {
    matches!(kind, "MountingHole" | "PlatedHole" | "Fiducial")
}
