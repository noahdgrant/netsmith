//! Overhang: a part that hangs over the board edge on purpose (ADR 0045, "Built: overhang").
//!
//! A courtyard outside the outline is an error unless the person said the part may hang over
//! that edge. They say it with a requirement, with no syntax of its own, on the part's
//! `.place` whose window for an edge of the body lies wholly at or beyond the same edge of
//! the outline: `.plug.place.bottom >= outline.height` says the body's bottom edge is at the
//! board's bottom edge or past it. Only the edge fields (`left`, `right`, `top`, `bottom`)
//! of the part itself say it: a window on `x` or `y`, or one a module states for its parts,
//! does not, and neither does a lock in the layout file (owner decision, 2026-11-03: a locked
//! part must state every edge it is past, like any other part).
//! [`crate::constraints::plan`] finds the edges and keeps them as [`Overhang::Stated`].
//!
//! What the rule changes is only the courtyard against the outline. The part's courtyard is
//! counted as its [`clip`] to the outline, for the room it takes in the fit test and for
//! overlap with the courtyards of other parts, keepouts and cutouts. Copper is not waived:
//! every pad is still inside the outline and the process's copper to edge space away from it.
//! A direction the person did not state is still an error, and the finding says how far.
//!
//! **The rule in full** (ADR 0045 section 10; every choice below is the owner's, decided on
//! 2026-11-03 and 2026-11-04).
//!
//! - *The fit.* A part that stays (a lock, or a requirement that leaves one place) counts in the
//!   placer's fit test as its courtyard clipped to the outline, with the process's margin
//!   (once a fixed 2.0 mm, now derived, ADR 0033) kept on the sides that are on the board and
//!   none on a side it hangs over ([`needs`]). A part that is placed (a window) keeps its pads,
//!   not its courtyard, inside: the annealer adds a hard cost for a pad corner nearer the
//!   outline than `copper_to_edge`.
//! - *The check* (`Board::check_placement`). No `OutsideOutline` for the courtyard past a
//!   stated edge; overlap is measured on the courtyard clipped to the outline; a pad outside
//!   the outline or nearer than `copper_to_edge` is still `PadNearEdge`; a courtyard past an
//!   edge nobody stated is still `OutsideOutline`, and the finding names the part, each edge,
//!   how far, and the line that would allow it (`.plug.place.right >= outline.width`).
//!   `Board::hangs()` lists what is past a stated edge, and `fab` prints each as a warning and
//!   writes it to `fab-notes.txt`.
//! - *The body's centre.* The placer refuses a requirement that leaves the body's centre
//!   wholly outside a fixed outline, except along an axis, in a direction, where the part's own
//!   edge requirement states the overhang (`stated_edges`). A window on `.x` or `.y` keeps the
//!   refusal. Accepted as built by the owner, 2026-11-11.
//! - *Not checked.* The body (`F.Fab`) need not stay inside, and the part of the courtyard
//!   outside the outline, the silkscreen and the text are not measured (`silk_edge_clearance`
//!   stays a warning, as in KiCad). How far a part hangs is whatever its window says.
//! - *Two stages in the placer, not a veto.* The rows are the annealer's start state and do
//!   not decide that the parts do not fit. With a size to fit, the annealer runs on the faces
//!   the parts are on, then the free parts are shared between the faces, largest first, and it
//!   runs again; the parts do not fit only if the last run ends with courtyards over each other
//!   or parts outside. Two parts that hang over the same edge may overlap beyond it, as the
//!   check allows (`Engine::room` clips the courtyard of a part that states an overhang).
//!
//! **The nine decisions of section 10** (all decided, 2026-11-03 and 2026-11-04; issue #11 is
//! closed). The ADR's text gave each as a question with the owner's answer, word for word:
//! (a) "Whether a window on `.x` or `.y` should state an overhang (now it does not: only the
//! edge fields do). **Decided 2026-11-03: only the edge fields.**" (b) "Whether a locked part
//! should state every edge it is past (now it does not need to). **Decided 2026-11-03: it
//! must.**" (c) "The courtyard wall, which was a fixed 2.0 mm and is now derived from the
//! process (ADR 0033): a stated overhang keeps it on the other sides. **Decided 2026-11-04:
//! accepted, as in ADR 0033.**" (d) "The hole-part two-face rule changed (ADR 0031): a footprint
//! that draws its courtyard on one face's layer only is judged on the other face by its pads;
//! the courtyard rules are otherwise unchanged. **Decided 2026-11-04: accepted, as in ADR
//! 0031.**" (e) "Whether `fab` should list a stated overhang as a warning, not a note.
//! **Decided 2026-11-03: a warning.**" (f) "The limit on how far a part may hang. **Decided
//! 2026-11-03: no limit.**" (g) "Silkscreen over the edge stays a warning. **Decided
//! 2026-11-03: it stays a warning.**" (h) "A module's `.place` does not state an overhang for
//! its parts. **Decided 2026-11-04: it stays refused.**" (a module's author does not know the
//! case; a module that needs one offers a value, as ADR 0042 describes, and the board states
//! the requirement). (i) "Two parts that hang over the same edge may overlap beyond it in the
//! check, but the annealer still keeps their whole courtyards apart (stricter than the check).
//! **Decided 2026-11-04: the annealer is changed to agree with the check.**" Whether a module's
//! physical file may state an overhang is the module layer of ADR 0048, which is not built
//! (issue #52).

use crate::geometry::{Pt, Rect};

/// A side of the board outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    pub const ALL: [Edge; 4] = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom];

    pub fn word(self) -> &'static str {
        match self {
            Edge::Left => "left",
            Edge::Right => "right",
            Edge::Top => "top",
            Edge::Bottom => "bottom",
        }
    }
}

/// A set of edges of the outline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl Edges {
    pub const ALL: Edges = Edges {
        left: true,
        right: true,
        top: true,
        bottom: true,
    };

    pub fn has(self, edge: Edge) -> bool {
        match edge {
            Edge::Left => self.left,
            Edge::Right => self.right,
            Edge::Top => self.top,
            Edge::Bottom => self.bottom,
        }
    }

    pub fn set(&mut self, edge: Edge) {
        match edge {
            Edge::Left => self.left = true,
            Edge::Right => self.right = true,
            Edge::Top => self.top = true,
            Edge::Bottom => self.bottom = true,
        }
    }

    pub fn any(self) -> bool {
        self.left || self.right || self.top || self.bottom
    }

    /// The edges in the set, in the order left, right, top, bottom.
    pub fn list(self) -> Vec<Edge> {
        Edge::ALL.into_iter().filter(|&e| self.has(e)).collect()
    }
}

/// What the person stated about a part hanging over the edge.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Overhang {
    /// Requirements on the part's `.place` that put an edge of its body at or past the same
    /// edge of the outline.
    Stated {
        edges: Edges,
        /// The requirements, as `` `plug.place.bottom (widget.nsm:14)` allows at least 60 mm ``,
        /// for a message.
        says: String,
    },
}

impl Overhang {
    /// The edges the part may hang over.
    pub fn edges(&self) -> Edges {
        match self {
            Overhang::Stated { edges, .. } => *edges,
        }
    }

    /// How the person said it, for a message.
    pub fn by(&self) -> String {
        match self {
            Overhang::Stated { says, .. } => says.clone(),
        }
    }
}

/// The part of `court` inside `outline`: where the room it takes is on the board. A courtyard
/// wholly outside is a line or a point on the outline's nearest side, which takes no room.
pub fn clip(court: &Rect, outline: &Rect) -> Rect {
    let into = |p: Pt| {
        Pt::new(
            p.x.clamp(outline.min.x, outline.max.x),
            p.y.clamp(outline.min.y, outline.max.y),
        )
    };
    Rect::new(into(court.min), into(court.max))
}

/// How far `court` is past each edge of `outline`, in mm, for the edges it is past.
pub(crate) fn past(court: &Rect, outline: &Rect) -> Vec<(Edge, f64)> {
    [
        (Edge::Left, outline.min.x - court.min.x),
        (Edge::Right, court.max.x - outline.max.x),
        (Edge::Top, outline.min.y - court.min.y),
        (Edge::Bottom, court.max.y - outline.max.y),
    ]
    .into_iter()
    .filter(|&(_, by)| by > 1e-6)
    .collect()
}

/// What a part needs of an outline of the size `fit`, in mm from its top left corner: the far
/// side of the part's clip to the outline and the margin `margin` beyond it, except on a
/// side where the part is meant to hang over, where it reaches the edge and needs no margin.
/// `court` is in the outline's own frame.
pub fn needs(court: &Rect, outline: &Rect, may: Edges, margin: f64) -> (f64, f64) {
    let c = clip(court, outline);
    let over_x = may.right && court.max.x > outline.max.x + 1e-6;
    let over_y = may.bottom && court.max.y > outline.max.y + 1e-6;
    (
        c.max.x - outline.min.x + if over_x { 0.0 } else { margin },
        c.max.y - outline.min.y + if over_y { 0.0 } else { margin },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect::new(Pt::new(x0, y0), Pt::new(x1, y1))
    }

    const BOARD: fn() -> Rect = || rect(0.0, 0.0, 35.0, 60.0);

    #[test]
    fn a_courtyard_inside_is_its_own_clip_and_is_past_no_edge() {
        let c = rect(5.0, 5.0, 20.0, 30.0);
        assert_eq!(clip(&c, &BOARD()), c);
        assert!(past(&c, &BOARD()).is_empty());
    }

    #[test]
    fn a_courtyard_hanging_over_the_bottom_is_clipped_there_and_past_by_the_rest() {
        // 42 mm along y, 28 along x, 20 mm of it on the board: 80 per cent is past the edge.
        let c = rect(3.0, 40.0, 31.0, 82.0);
        assert_eq!(clip(&c, &BOARD()), rect(3.0, 40.0, 31.0, 60.0));
        assert_eq!(past(&c, &BOARD()), vec![(Edge::Bottom, 22.0)]);
    }

    #[test]
    fn a_courtyard_past_two_edges_is_past_both_by_their_own_numbers() {
        let c = rect(-3.5, 30.0, 38.5, 70.0);
        assert_eq!(
            past(&c, &BOARD()),
            vec![(Edge::Left, 3.5), (Edge::Right, 3.5), (Edge::Bottom, 10.0)]
        );
        assert_eq!(clip(&c, &BOARD()), rect(0.0, 30.0, 35.0, 60.0));
    }

    #[test]
    fn a_courtyard_wholly_outside_takes_no_room() {
        let c = rect(10.0, 70.0, 20.0, 80.0);
        let clipped = clip(&c, &BOARD());
        assert_eq!(clipped.width() * clipped.height(), 0.0);
        assert!(!clipped.overlaps(&rect(0.0, 0.0, 35.0, 60.0)));
    }

    #[test]
    fn touching_the_edge_is_not_past_it() {
        let c = rect(0.0, 10.0, 35.0, 60.0);
        assert!(past(&c, &BOARD()).is_empty());
    }

    #[test]
    fn a_part_that_may_hang_over_needs_the_clip_and_no_margin_on_that_side() {
        let c = rect(3.0, 40.0, 31.0, 82.0);
        let may = Edges {
            bottom: true,
            ..Edges::default()
        };
        // 20 mm of the courtyard is on the board, to its bottom edge: no margin below, and the
        // right side is inside the board with its margin as ever.
        assert_eq!(needs(&c, &BOARD(), may, 2.0), (33.0, 60.0));
        // Hanging over a side the person did not state costs the margin there.
        assert_eq!(needs(&c, &BOARD(), Edges::default(), 2.0), (33.0, 62.0));
    }

    #[test]
    fn a_requirement_allows_only_the_edges_it_states() {
        let mut bottom = Edges::default();
        bottom.set(Edge::Bottom);
        let stated = Overhang::Stated {
            edges: bottom,
            says: "`plug.place.bottom (a.nsm:3)` allows at least 60 mm".into(),
        };
        assert_eq!(stated.edges().list(), vec![Edge::Bottom]);
    }
}
