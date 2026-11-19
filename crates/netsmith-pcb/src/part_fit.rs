//! Where one part fits with every other part where it is (issue #780): [`Board::fit_part`],
//! which `netsmith layout fit` prints.
//!
//! For each turn and face the part may take, the part is tried on a grid of origins over the
//! outline (every `step` mm from the outline's top left corner), nearest a point first, and
//! each place is measured as the placer and `fab` measure it: the placement check of that part
//! ([`Board::placement_problems_of`]: courtyards, holes, keepouts, height limits, the edge and
//! cutouts) and the requirements of the physical file about it (its own windows, its modules'
//! and the relations it is an end of). The first place where nothing is found is the pose's
//! answer. A pose with no such place gives the closest try instead (the fewest findings, then
//! the nearest), with what is in the way there.
//!
//! The turns are the four right angles from the part's turn (the placer turns by right angles
//! only); the faces are both, unless a requirement states the face or the caller names one. A
//! turn or face a requirement rules out is not tried. Nothing is moved: the board is the
//! caller's, and a place it finds goes into the layout file with `netsmith layout set --lock`
//! (locked, so that `fab`'s placer places the other parts round it, issue #1224).
//!
//! The measure is the placement check's own: [`Board::placement_problems_of`] is
//! [`Board::check_placement`] measured on one part, the check `fab`, `layout check`, `layout
//! regenerate` and the editor run, so a place it calls free is one they accept.
//!
//! Places are footprint origins in the layout file's frame (mm from the outline's top left
//! corner, y down), and distances are from the point to the centre of the courtyard.

use serde::Serialize;

use crate::board::{Board, Placed, Side};
use crate::conflict::PlacementFault;
use crate::constraints::requirements;
use crate::explain::unmet;
use crate::fit_report::PartRef;
use crate::geometry::Pt;
use crate::miss::RequirementMiss;
use netsmith_core::{Design, PlaceField};

/// The grid of a fit query unless the caller says otherwise, in mm.
pub const FIT_STEP: f64 = 0.5;

/// What the distances of a fit query are measured from.
#[derive(Debug, Clone, PartialEq)]
pub enum FitNear {
    /// Where the part is now.
    Itself,
    /// Another part, by path or designator: the centre of its courtyard.
    Part(String),
    /// A point in the layout file's frame, mm from the outline's top left corner, y down.
    Point(f64, f64),
}

impl FitNear {
    /// What `text` says to be near: `X,Y` (mm, from the outline's top left corner, y down) is a
    /// point and anything else a part, by path or designator (a part's path has no comma).
    /// `None` is [`FitNear::Itself`]. The one reading of `--near` for `netsmith layout fit` and
    /// of `near` for the editor's `GET /api/pcb/fit`.
    pub fn parse(text: Option<&str>) -> Result<FitNear, String> {
        let Some(text) = text else {
            return Ok(FitNear::Itself);
        };
        let Some((x, y)) = text.split_once(',') else {
            return Ok(FitNear::Part(text.to_string()));
        };
        let number = |v: &str| {
            v.trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("`{v}` is not a number"))
        };
        Ok(FitNear::Point(number(x)?, number(y)?))
    }
}

/// The point the distances are from, in the layout file's frame, and the part it is the centre
/// of, if any.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FitTarget {
    pub x: f64,
    pub y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<PartRef>,
    /// Whether it is where the part is now (no `--near`).
    #[serde(skip)]
    pub itself: bool,
}

/// One place of the part and what is found there.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FitPlace {
    /// The turn in degrees, counterclockwise on screen.
    pub turn: f64,
    /// `top` or `bottom`.
    pub side: &'static str,
    /// The footprint's origin, as the layout file's `at` has it.
    pub at: [f64; 2],
    /// From the target to the centre of the courtyard, in mm.
    pub distance: f64,
    /// Whether nothing is found here.
    pub fits: bool,
    /// The placement check's findings about the part here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub placement: Vec<PlacementFault>,
    /// The requirements about the part it does not meet here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<RequirementMiss>,
    /// The parts whose courtyards are nearer this one's than their routing margins ask
    /// (`.routing_margin`, issue #1313).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub margins: Vec<MarginShort>,
    /// The sentences of `placement`, `requirements` and `margins`, in that order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<String>,
}

/// A part nearer the fitted part than their routing margins ask for (issue #1313).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MarginShort {
    pub part: PartRef,
    /// From courtyard to courtyard, in mm (the larger of the spaces along x and along y).
    pub gap: f64,
    /// The sum of the two parts' routing margins, in mm.
    pub need: f64,
    pub text: String,
}

/// What [`Board::fit_part`] found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartFit {
    pub board: String,
    pub part: PartRef,
    pub near: FitTarget,
    /// The grid, in mm.
    pub step: f64,
    /// Where the part is now, and what is found there.
    pub now: FitPlace,
    /// For each turn and face tried: the nearest place that fits, or the closest try. The poses
    /// with a place come first, nearest first, then those without.
    pub poses: Vec<FitPlace>,
}

impl PartFit {
    /// Whether some pose has a place.
    pub fn fits(&self) -> bool {
        self.poses.iter().any(|p| p.fits)
    }

    /// A few lines for a person; each place that fits with the `netsmith layout set` that
    /// writes it, locked: the place is free with every other part where it is, and `fab`'s
    /// placer places round a locked part, where an unlocked entry is put after placing and
    /// may find a part the placer put there (issue #1224).
    pub fn text(&self) -> String {
        let near = match &self.near.part {
            Some(p) => format!("{} ({})", p.refdes, p.path),
            None if self.near.itself => "where it is".to_string(),
            None => format!("{},{}", num(self.near.x), num(self.near.y)),
        };
        let mut out = format!(
            "{} ({}) of {}, nearest {near}, on a grid of {} mm:\n",
            self.part.refdes,
            self.part.path,
            self.board,
            num(self.step)
        );
        out += &format!("  now {}", pose(&self.now));
        if self.now.fits {
            out += ": fits\n";
        } else {
            out += ":\n";
            for m in &self.now.messages {
                out += &format!("    {m}\n");
            }
        }
        for p in &self.poses {
            if p.fits {
                out += &format!(
                    "  fits {}, {} mm away: netsmith layout set {} --at {},{} --turn {} --side {} --lock\n",
                    pose(p),
                    num(p.distance),
                    self.part.path,
                    num(p.at[0]),
                    num(p.at[1]),
                    num(p.turn),
                    p.side
                );
            } else {
                out += &format!(
                    "  no place turned {} on the {}; the closest try, {},{}, has:\n",
                    num(p.turn),
                    p.side,
                    num(p.at[0]),
                    num(p.at[1])
                );
                for m in &p.messages {
                    out += &format!("    {m}\n");
                }
            }
        }
        if !self.fits() {
            out += "no place fits: move or unlock a part in the way, or give the board more room\n";
        }
        out
    }
}

fn pose(p: &FitPlace) -> String {
    format!(
        "{},{} {}° {}",
        num(p.at[0]),
        num(p.at[1]),
        num(p.turn),
        p.side
    )
}

/// A figure to two decimals, with no `-0` and no trailing zeros.
fn num(v: f64) -> String {
    let r = round(v);
    let s = format!("{r:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn round(v: f64) -> f64 {
    let r = (v * 100.0).round() / 100.0;
    if r == 0.0 { 0.0 } else { r }
}

impl Board {
    /// Where the part `name` (a path or a designator) fits with every other part where it is,
    /// for each turn and face it may take, nearest `near` first, on a grid of `step` mm
    /// (the module's header). `face` keeps to one face. The design is the board's, for its
    /// requirements. Refused: a part the board does not have, and a step that is not above 0.
    pub fn fit_part(
        &self,
        design: &Design,
        name: &str,
        near: &FitNear,
        step: f64,
        face: Option<Side>,
    ) -> Result<PartFit, String> {
        if !(step.is_finite() && step > 0.0) {
            return Err(format!("the grid step must be above 0 mm, not {step}"));
        }
        let index = self.find_part(name)?;
        let part = &self.parts[index];
        let origin = self.outline.min;
        let target = match near {
            FitNear::Itself => {
                let c = part.courtyard().centre();
                FitTarget {
                    x: round(c.x - origin.x),
                    y: round(c.y - origin.y),
                    part: None,
                    itself: true,
                }
            }
            FitNear::Part(other) => {
                let other = &self.parts[self.find_part(other)?];
                let c = other.courtyard().centre();
                FitTarget {
                    x: round(c.x - origin.x),
                    y: round(c.y - origin.y),
                    part: Some(PartRef {
                        refdes: other.refdes.clone(),
                        path: other.path.clone(),
                    }),
                    itself: false,
                }
            }
            FitNear::Point(x, y) => FitTarget {
                x: *x,
                y: *y,
                part: None,
                itself: false,
            },
        };
        let point = origin + Pt::new(target.x, target.y);
        let mut board = self.clone();
        let now = board.measure(design, index, point);

        // The faces and turns the requirements leave.
        let (reqs, side_req) = requirements(design, &part.path);
        let faces: Vec<Side> = match (face, side_req.as_ref().map(|(s, _)| s.as_str())) {
            (Some(f), _) => vec![f],
            (None, Some("bottom")) => vec![Side::Back],
            (None, Some(_)) => vec![Side::Front],
            (None, None) => vec![Side::Front, Side::Back],
        };
        let turn_ok = |turn: f64| {
            reqs.iter()
                .filter(|r| r.field == PlaceField::Rotation)
                .all(|r| {
                    let t = turn.rem_euclid(360.0);
                    [t - 360.0, t, t + 360.0]
                        .iter()
                        .any(|t| *t >= r.lo - 1e-6 && *t <= r.hi + 1e-6)
                })
        };
        let base = part.rotation.rem_euclid(90.0);
        let mut poses = Vec::new();
        for side in faces {
            for q in 0..4 {
                let turn = round(base + 90.0 * f64::from(q));
                if !turn_ok(turn) {
                    continue;
                }
                poses.push(board.fit_pose(design, index, point, step, turn, side));
            }
        }
        poses.sort_by(|a, b| {
            b.fits
                .cmp(&a.fits)
                .then(a.distance.total_cmp(&b.distance))
                .then(a.turn.total_cmp(&b.turn))
        });
        Ok(PartFit {
            board: self.name.clone(),
            part: PartRef {
                refdes: part.refdes.clone(),
                path: part.path.clone(),
            },
            near: target,
            step,
            now,
            poses,
        })
    }

    /// The index of the part `name`, a path or a designator, or the refusal with the nearest
    /// names.
    fn find_part(&self, name: &str) -> Result<usize, String> {
        self.parts
            .iter()
            .position(|p| p.path == name || p.refdes == name)
            .ok_or_else(|| crate::summary::no_part(self, name))
    }

    /// What is found about the part at `index` where it is, its distance measured from
    /// `point`.
    fn measure(&self, design: &Design, index: usize, point: Pt) -> FitPlace {
        let part = &self.parts[index];
        let placement: Vec<PlacementFault> = self
            .placement_problems_of(&part.refdes)
            .iter()
            .map(|p| self.fault_of(p))
            .collect();
        let requirements: Vec<RequirementMiss> = unmet(design, &self.parts, Some(&part.path))
            .into_iter()
            .map(|(_, m)| m)
            .collect();
        let margins = self.margins_of(index);
        let messages = placement
            .iter()
            .map(|f| f.text.clone())
            .chain(requirements.iter().map(|m| m.text.clone()))
            .chain(margins.iter().map(|m| m.text.clone()))
            .collect();
        let at = part.at - self.outline.min;
        FitPlace {
            turn: round(part.rotation),
            side: part.side.word(),
            at: [round(at.x), round(at.y)],
            distance: round(distance(part, point)),
            fits: placement.is_empty() && requirements.is_empty() && margins.is_empty(),
            placement,
            requirements,
            margins,
            messages,
        }
    }

    /// The parts nearer the part at `index` than their routing margins ask for, as
    /// [`Placed::margin_short`] measures them: the placer's rule, which `fab`'s placement check
    /// does not hold (issue #1313).
    fn margins_of(&self, index: usize) -> Vec<MarginShort> {
        let part = &self.parts[index];
        self.parts
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != index)
            .filter_map(|(_, other)| {
                let (gap, need) = part.margin_short(other)?;
                Some(MarginShort {
                    part: PartRef {
                        refdes: other.refdes.clone(),
                        path: other.path.clone(),
                    },
                    gap: round(gap),
                    need: round(need),
                    text: format!(
                        "{} is {} mm from {}, courtyard to courtyard, and their routing margins ask for {} mm ({} mm and {} mm, `.routing_margin`)",
                        crate::board::member(&part.path, &part.refdes),
                        num(gap),
                        crate::board::member(&other.path, &other.refdes),
                        num(need),
                        num(part.routing_margin),
                        num(other.routing_margin)
                    ),
                })
            })
            .collect()
    }

    /// The nearest place of the part at `index` turned `turn` on `side` where nothing is found,
    /// or the closest try. The part is left where it was.
    fn fit_pose(
        &mut self,
        design: &Design,
        index: usize,
        point: Pt,
        step: f64,
        turn: f64,
        side: Side,
    ) -> FitPlace {
        let was = (
            self.parts[index].at,
            self.parts[index].rotation,
            self.parts[index].side,
        );
        let o = self.outline;
        {
            let p = &mut self.parts[index];
            p.rotation = turn;
            p.side = side;
            p.at = o.min;
        }
        // The origins that keep the courtyard inside the outline (or, for a part that may hang
        // over an edge, within its own size of it), on the grid from the outline's corner.
        let court = self.parts[index].courtyard();
        let (lo, hi) = (court.min - o.min, court.max - o.min);
        let hang = if self.overhang.contains_key(&self.parts[index].refdes) {
            Pt::new(court.width(), court.height())
        } else {
            Pt::new(0.0, 0.0)
        };
        let axis = |min: f64, max: f64| -> Vec<f64> {
            let first = (min / step - 1e-9).ceil() as i64;
            let last = (max / step + 1e-9).floor() as i64;
            (first..=last).map(|k| k as f64 * step).collect()
        };
        let xs = axis(-lo.x - hang.x, o.width() - hi.x + hang.x);
        let ys = axis(-lo.y - hang.y, o.height() - hi.y + hang.y);
        let centre = court.centre() - o.min;
        let mut grid: Vec<(f64, Pt)> = xs
            .iter()
            .flat_map(|&x| ys.iter().map(move |&y| Pt::new(x, y)))
            .map(|at| {
                let c = o.min + at + centre;
                ((c.x - point.x).hypot(c.y - point.y), at)
            })
            .collect();
        if grid.is_empty() {
            // Bigger than the outline: the one try is at its centre.
            let at = Pt::new(o.width() / 2.0, o.height() / 2.0) - centre;
            grid.push((0.0, at));
        }
        grid.sort_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then(a.1.x.total_cmp(&b.1.x))
                .then(a.1.y.total_cmp(&b.1.y))
        });
        let mut best: Option<(usize, f64, Pt)> = None;
        let mut found = None;
        for &(d, at) in &grid {
            self.parts[index].at = o.min + at;
            let refdes = &self.parts[index].refdes;
            let faults = self.placement_problems_of(refdes).len()
                + unmet(design, &self.parts, Some(&self.parts[index].path)).len()
                + self.margins_of(index).len();
            if faults == 0 {
                found = Some(at);
                break;
            }
            if best.is_none_or(|(n, _, _)| faults < n) {
                best = Some((faults, d, at));
            }
        }
        let at = found.or(best.map(|b| b.2)).expect("one try at least");
        self.parts[index].at = o.min + at;
        let place = self.measure(design, index, point);
        let p = &mut self.parts[index];
        (p.at, p.rotation, p.side) = was;
        place
    }
}

/// From `point` to the centre of the part's courtyard, in mm.
fn distance(part: &Placed, point: Pt) -> f64 {
    let c = part.courtyard().centre();
    (c.x - point.x).hypot(c.y - point.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::tests::{led_board, led_design};
    use crate::geometry::Rect;

    /// The LED board with a routing margin of `margin` mm round every part (issue #1313).
    fn with_margins(margin: f64) -> Board {
        let mut board = led_board();
        for p in &mut board.parts {
            p.routing_margin = margin;
        }
        board
    }

    #[test]
    fn two_parts_nearer_than_their_routing_margins_are_short_and_say_by_how_much() {
        let board = with_margins(0.75);
        let d1 = board.parts.iter().find(|p| p.refdes == "D1").unwrap();
        let r1 = board.parts.iter().find(|p| p.refdes == "R1").unwrap();
        // 1.09 mm apart, courtyard to courtyard, and 1.5 mm asked for.
        let (gap, need) = d1.margin_short(r1).unwrap();
        assert!((gap - 1.09).abs() < 1e-6, "{gap}");
        assert!((need - 1.5).abs() < 1e-9, "{need}");
        assert_eq!(r1.margin_short(d1), d1.margin_short(r1));
        // With no margin, or margins they keep, nothing is short.
        assert_eq!(
            led_board().parts[0].margin_short(&led_board().parts[2]),
            None
        );
        let board = with_margins(0.5);
        let d1 = board.parts.iter().find(|p| p.refdes == "D1").unwrap();
        let r1 = board.parts.iter().find(|p| p.refdes == "R1").unwrap();
        assert_eq!(d1.margin_short(r1), None);
    }

    #[test]
    fn a_fit_keeps_the_routing_margins_and_says_which_part_is_too_near_where_it_is() {
        let board = with_margins(0.75);
        let design = led_design();
        let fit = board
            .fit_part(&design, "D1", &FitNear::Itself, 0.5, Some(Side::Front))
            .unwrap();
        assert!(!fit.now.fits, "{}", fit.text());
        assert_eq!(fit.now.margins.len(), 1, "{:?}", fit.now.margins);
        assert_eq!(fit.now.margins[0].part.refdes, "R1");
        assert!(
            fit.text()
                .contains("D1 is 1.09 mm from R1, courtyard to courtyard, and their routing margins ask for 1.5 mm"),
            "{}",
            fit.text()
        );
        // The place it gives keeps 1.5 mm from every other part.
        let first = fit.poses.iter().find(|p| p.fits).expect("a place");
        let mut moved = board.clone();
        let d1 = moved.parts.iter_mut().find(|p| p.refdes == "D1").unwrap();
        d1.at = moved.outline.min + Pt::new(first.at[0], first.at[1]);
        d1.rotation = first.turn;
        let d1 = moved.parts.iter().find(|p| p.refdes == "D1").unwrap();
        for other in moved.parts.iter().filter(|p| p.refdes != "D1") {
            assert_eq!(d1.margin_short(other), None, "{}", other.refdes);
        }
    }

    #[test]
    fn a_part_that_fits_where_it_is_is_said_to_fit_there_first() {
        let board = led_board();
        let design = led_design();
        let part = board.parts[0].clone();
        let fit = board
            .fit_part(
                &design,
                &part.refdes,
                &FitNear::Itself,
                0.5,
                Some(Side::Front),
            )
            .unwrap();
        assert!(fit.now.fits, "{}", fit.text());
        assert!(fit.fits(), "{}", fit.text());
        assert!(fit.poses.iter().all(|p| p.side == "top"));
        // The pose it has now fits within one step of where it is.
        let same = fit
            .poses
            .iter()
            .find(|p| p.turn == round(part.rotation))
            .unwrap();
        assert!(
            same.fits && same.distance <= 0.5 * 2f64.sqrt() + 1e-6,
            "{same:?}"
        );
        // The board is as it was.
        assert_eq!(board.parts[0], part);
    }

    #[test]
    fn a_place_that_fits_has_no_finding_and_one_on_another_part_does() {
        let board = led_board();
        let design = led_design();
        let (a, b) = (board.parts[0].clone(), board.parts[1].clone());
        // Near the other part: the place found is clear of it.
        let fit = board
            .fit_part(
                &design,
                &a.path,
                &FitNear::Part(b.refdes.clone()),
                0.25,
                None,
            )
            .unwrap();
        let first = fit.poses.first().unwrap();
        assert!(first.fits, "{}", fit.text());
        let mut moved = board.clone();
        let p = moved
            .parts
            .iter_mut()
            .find(|p| p.refdes == a.refdes)
            .unwrap();
        p.at = moved.outline.min + Pt::new(first.at[0], first.at[1]);
        p.rotation = first.turn;
        p.side = if first.side == "bottom" {
            Side::Back
        } else {
            Side::Front
        };
        assert_eq!(moved.placement_problems_of(&a.refdes), vec![]);
        // On top of it, the check names both.
        let p = moved
            .parts
            .iter_mut()
            .find(|p| p.refdes == a.refdes)
            .unwrap();
        p.at = b.at;
        p.side = b.side;
        let problems = moved.placement_problems_of(&a.refdes);
        assert!(
            problems
                .iter()
                .any(|p| p.parts().contains(&b.refdes.as_str())),
            "{problems:?}"
        );
        // And the one part's problems are the whole check's that name it.
        let whole: Vec<_> = moved
            .check_placement()
            .into_iter()
            .filter(|p| p.parts().contains(&a.refdes.as_str()))
            .collect();
        assert_eq!(problems, whole);
    }

    #[test]
    fn near_is_a_point_with_a_comma_a_part_without_and_where_it_is_with_none() {
        assert_eq!(FitNear::parse(None), Ok(FitNear::Itself));
        assert_eq!(
            FitNear::parse(Some("stage.u1")),
            Ok(FitNear::Part("stage.u1".into()))
        );
        assert_eq!(
            FitNear::parse(Some("-1.5, 2")),
            Ok(FitNear::Point(-1.5, 2.0))
        );
        assert!(
            FitNear::parse(Some("1,x"))
                .unwrap_err()
                .contains("`x` is not a number")
        );
        assert!(FitNear::parse(Some("1,NaN")).is_err());
    }

    #[test]
    fn a_part_the_board_lacks_and_a_bad_step_are_refused() {
        let board = led_board();
        let design = led_design();
        let e = board
            .fit_part(&design, "nothing", &FitNear::Itself, 0.5, None)
            .unwrap_err();
        assert!(e.contains("no part `nothing`"), "{e}");
        let first = board.parts[0].refdes.clone();
        assert!(
            board
                .fit_part(&design, &first, &FitNear::Itself, 0.0, None)
                .is_err()
        );
    }

    #[test]
    fn a_part_bigger_than_the_outline_fits_nowhere_and_says_what_is_in_the_way() {
        let mut board = led_board();
        let design = led_design();
        let first = board.parts[0].refdes.clone();
        let court = board.parts[0].courtyard();
        board.outline = Rect::new(
            board.outline.min,
            board.outline.min + Pt::new(court.width() / 2.0, court.height() / 2.0),
        );
        let fit = board
            .fit_part(&design, &first, &FitNear::Itself, 0.5, Some(Side::Front))
            .unwrap();
        assert!(!fit.fits(), "{}", fit.text());
        assert!(
            fit.poses
                .iter()
                .all(|p| p.placement.iter().any(|f| f.kind == "outside_outline")),
            "{}",
            fit.text()
        );
        assert!(fit.text().contains("no place fits"), "{}", fit.text());
    }
}
