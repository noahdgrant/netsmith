//! Overhang: a part that hangs over the board edge on purpose (ADR 0045, "Built: overhang").
//!
//! A courtyard outside the outline is an error unless the person said the part may hang over
//! that edge. They say it in one of two ways, with no syntax of its own:
//!
//! - **A requirement** on the part's `.place` whose window for an edge of the body lies
//!   wholly at or beyond the same edge of the outline: `.plug.place.bottom >= outline.height`
//!   says the body's bottom edge is at the board's bottom edge or past it. Only the edge
//!   fields (`left`, `right`, `top`, `bottom`) of the part itself say it: a window on `x` or
//!   `y`, or one a module states for its parts, does not. [`crate::constraints::plan`] finds
//!   the edges and keeps them as [`Overhang::Stated`].
//! - **A lock** in the layout file puts the part somewhere ([`Overhang::Locked`]): every edge
//!   the locked courtyard passes is then meant.
//!
//! What the rule changes is only the courtyard against the outline. The part's courtyard is
//! counted as its [`clip`] to the outline, for the room it takes in the fit test and for
//! overlap with the courtyards of other parts, keepouts and cutouts. Copper is not waived:
//! every pad is still inside the outline and the process's copper to edge space away from it.
//! A direction the person did not state is still an error, and the finding says how far.

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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq)]
pub enum Overhang {
    /// The layout file locks the part: it hangs over wherever its courtyard is outside.
    Locked,
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
            Overhang::Locked => Edges::ALL,
            Overhang::Stated { edges, .. } => *edges,
        }
    }

    /// How the person said it, for a message.
    pub fn by(&self) -> String {
        match self {
            Overhang::Locked => "locked in the layout file".to_string(),
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
pub fn past(court: &Rect, outline: &Rect) -> Vec<(Edge, f64)> {
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
    fn a_lock_may_hang_over_every_edge_and_a_requirement_only_the_edges_it_states() {
        let mut bottom = Edges::default();
        bottom.set(Edge::Bottom);
        let stated = Overhang::Stated {
            edges: bottom,
            says: "`plug.place.bottom (a.nsm:3)` allows at least 60 mm".into(),
        };
        assert_eq!(stated.edges().list(), vec![Edge::Bottom]);
        assert_eq!(Overhang::Locked.edges().list(), Edge::ALL.to_vec());
    }
}
