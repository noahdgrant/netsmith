//! A piece of a part's silkscreen as the annealer measures it: the line a stroke draws, not
//! the box round it.

use super::*;

/// How far a chord may lie from the arc or circle it stands for, in mm.
const SAGITTA: f64 = 0.01;

/// A stroked line or a filled area of the silkscreen, in a part's frame as it lies on the
/// board, relative to the part's origin. Drawing a pin 1 circle or an arc round a body as a
/// box would count every pad inside it as under the silkscreen; this measures the line.
#[derive(Debug, Clone)]
pub(super) struct Piece {
    /// The line the stroke follows; the outline of an area, whose last point is its first.
    points: Vec<Pt>,
    /// Half the stroke's width: how far the silkscreen reaches from `points`.
    half: f64,
    /// Whether the inside of the outline is silkscreen too (a filled shape, or text).
    solid: bool,
    /// The box round the piece, stroke included.
    bounds: Rect,
}

impl Piece {
    /// The stroke of a drawing along `points`, `width` wide; `None` if there is no point.
    pub(super) fn stroke(points: Vec<Pt>, width: f64, solid: bool) -> Option<Piece> {
        let half = width / 2.0;
        let bounds = Rect::around(points.iter().copied())?.expanded(half);
        Some(Piece {
            points,
            half,
            solid,
            bounds,
        })
    }

    /// A solid box (text).
    pub(super) fn solid_box(r: Rect) -> Piece {
        let [a, b, c, d] = r.corners();
        Piece {
            points: vec![a, b, c, d, a],
            half: 0.0,
            solid: true,
            bounds: r,
        }
    }

    /// The box round the piece.
    pub(super) fn bounds(&self) -> Rect {
        self.bounds
    }

    /// How far the piece, with its origin at `at`, is in `rect` or nearer to it than `gap`:
    /// the gap that is missing, and more if the two touch; `0` when they are `gap` apart or
    /// more. Distances are straight, not along the axes.
    pub(super) fn depth(&self, at: Pt, rect: &Rect, gap: f64) -> f64 {
        let local = rect.translated(Pt::new(-at.x, -at.y));
        if !self.bounds.expanded(gap).overlaps(&local) {
            return 0.0;
        }
        (gap + self.half - self.distance(&local)).max(0.0)
    }

    /// How much of `gap` the piece, with its origin at `at`, is short of an edge: `depth`
    /// says how far inside the edge a point of the board is (negative outside it, and
    /// concave, as the depth inside a convex outline is). The piece's nearest point to the
    /// edge is one of the points of its line, so these are what is measured; its stroke
    /// reaches `half` further. `0` when the piece is `gap` or more inside.
    pub(super) fn edge_depth(&self, at: Pt, gap: f64, depth: impl Fn(Pt) -> f64) -> f64 {
        let near = |p: Pt| depth(Pt::new(p.x + at.x, p.y + at.y));
        // The box's corners tell most pieces from the edge at once: where the depth is
        // concave, the least of it over the box is at a corner.
        if self.bounds.corners().into_iter().all(|c| near(c) >= gap) {
            return 0.0;
        }
        let inside = self
            .points
            .iter()
            .map(|p| near(*p))
            .fold(f64::MAX, f64::min);
        (gap + self.half - inside).max(0.0)
    }

    /// The distance from the piece's line (or area, if solid) to `r`, 0 where they meet.
    fn distance(&self, r: &Rect) -> f64 {
        if self.solid && self.points.len() > 3 && r.corners().iter().any(|c| self.encloses(*c)) {
            return 0.0;
        }
        match self.points.as_slice() {
            [only] => rect_distance(&Rect::new(*only, *only), r),
            points => points
                .windows(2)
                .map(|w| segment_to_rect(w[0], w[1], r))
                .fold(f64::MAX, f64::min),
        }
    }

    /// Whether `p` is inside the outline (even-odd).
    fn encloses(&self, p: Pt) -> bool {
        let mut inside = false;
        for w in self.points.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                inside = !inside;
            }
        }
        inside
    }
}

/// The distance from the segment `a`-`b` to the box `r`, 0 if they meet.
fn segment_to_rect(a: Pt, b: Pt, r: &Rect) -> f64 {
    if crosses(a, b, r) {
        return 0.0;
    }
    let to_segment = |p: Pt| {
        let d = b - a;
        let len2 = d.x * d.x + d.y * d.y;
        let t = if len2 == 0.0 {
            0.0
        } else {
            (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
        };
        p.distance(Pt::new(a.x + t * d.x, a.y + t * d.y))
    };
    let ends = rect_distance(&Rect::new(a, a), r).min(rect_distance(&Rect::new(b, b), r));
    r.corners().into_iter().map(to_segment).fold(ends, f64::min)
}

/// Whether the segment `a`-`b` has a point in the box `r` (Liang and Barsky's clipping).
fn crosses(a: Pt, b: Pt, r: &Rect) -> bool {
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    let d = b - a;
    for (p, q) in [
        (-d.x, a.x - r.min.x),
        (d.x, r.max.x - a.x),
        (-d.y, a.y - r.min.y),
        (d.y, r.max.y - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                lo = lo.max(t);
            } else {
                hi = hi.min(t);
            }
        }
    }
    lo <= hi
}

/// The pieces of a part's silkscreen on the board with the part turned to its angle at the
/// origin: each drawing on the silkscreen layer as its line (an arc and a circle as chords
/// within [`SAGITTA`] of the curve), a filled rectangle, circle or polygon as an area, and the
/// text the footprint prints besides the designator as boxes.
pub(super) fn pieces(part: &Placed, rules: &Rules) -> Vec<Piece> {
    let mut found: Vec<Piece> = part
        .footprint
        .graphics
        .iter()
        .filter(|g| g.layer == netsmith_pcb::Layer::Silk)
        .filter_map(|g| {
            let points = g
                .shape
                .outline(SAGITTA)
                .into_iter()
                .map(|p| part.to_board(p))
                .collect();
            Piece::stroke(points, g.stroke, g.filled && g.shape.is_closed())
        })
        .collect();
    // A label's own text is the part: its courtyard takes the room.
    if part.kind != "Label" {
        for t in &part.footprint.silk_texts {
            let width = t.text.chars().count() as f64 * t.height * 0.95
                + t.thickness.max(rules.min_text_thickness);
            let (w, h) = if (t.angle.rem_euclid(180.0) - 90.0).abs() < 1e-6 {
                (t.height, width)
            } else {
                (width, t.height)
            };
            let c = part.to_board(t.at);
            found.push(Piece::solid_box(
                Rect::new(
                    Pt::new(c.x - w / 2.0, c.y - h / 2.0),
                    Pt::new(c.x + w / 2.0, c.y + h / 2.0),
                )
                .expanded(TEXT_PAD),
            ));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_pcb::Shape;

    fn piece(shape: &Shape, stroke: f64, filled: bool) -> Piece {
        Piece::stroke(shape.outline(SAGITTA), stroke, filled && shape.is_closed()).unwrap()
    }

    fn pad(x: f64, y: f64) -> Rect {
        Rect::new(Pt::new(x - 0.25, y - 0.25), Pt::new(x + 0.25, y + 0.25))
    }

    #[test]
    fn a_pad_inside_a_circle_drawn_as_a_line_is_not_under_the_silkscreen() {
        let ring = piece(
            &Shape::Circle {
                centre: Pt::new(0.0, 0.0),
                radius: 2.0,
            },
            0.12,
            false,
        );
        assert_eq!(ring.depth(Pt::default(), &pad(0.0, 0.0), 0.15), 0.0);
        // On the line it is.
        assert!(ring.depth(Pt::default(), &pad(2.0, 0.0), 0.15) > 0.15);
        // Near it, within the space a pad keeps from the silkscreen, but not touching.
        let near = ring.depth(Pt::default(), &pad(2.0 - 0.25 - 0.06 - 0.1, 0.0), 0.15);
        assert!(near > 0.0 && near < 0.15, "{near}");
    }

    #[test]
    fn a_filled_circle_is_silkscreen_all_over() {
        let disc = piece(
            &Shape::Circle {
                centre: Pt::new(0.0, 0.0),
                radius: 2.0,
            },
            0.0,
            true,
        );
        assert!(disc.depth(Pt::default(), &pad(0.0, 0.0), 0.15) >= 0.15);
        assert_eq!(disc.depth(Pt::default(), &pad(4.0, 0.0), 0.15), 0.0);
    }

    #[test]
    fn an_arc_follows_its_bulge_and_not_its_three_points() {
        // A half circle from (-2, 0) through (0, -2) to (2, 0): a pad at its top is on it.
        let arc = piece(
            &Shape::Arc {
                start: Pt::new(-2.0, 0.0),
                mid: Pt::new(0.0, -2.0),
                end: Pt::new(2.0, 0.0),
            },
            0.12,
            false,
        );
        assert!(arc.depth(Pt::default(), &pad(0.0, -2.0), 0.15) > 0.15);
        // Inside the half circle there is nothing, and neither below it.
        assert_eq!(arc.depth(Pt::default(), &pad(0.0, -0.5), 0.15), 0.0);
        assert_eq!(arc.depth(Pt::default(), &pad(0.0, 1.0), 0.15), 0.0);
        // The bulge of the arc passes through (1.414, -1.414); a box round the three points
        // would not tell this from the middle of the arc's own box.
        assert!(arc.depth(Pt::default(), &pad(1.414, -1.414), 0.15) > 0.15);
    }

    #[test]
    fn a_diagonal_line_leaves_the_corner_of_its_box_free() {
        let line = piece(
            &Shape::Line {
                a: Pt::new(0.0, 0.0),
                b: Pt::new(4.0, 4.0),
            },
            0.12,
            false,
        );
        assert!(line.depth(Pt::default(), &pad(2.0, 2.0), 0.15) > 0.15);
        assert_eq!(line.depth(Pt::default(), &pad(3.5, 0.5), 0.15), 0.0);
    }

    #[test]
    fn a_piece_is_measured_where_its_part_stands() {
        let line = piece(
            &Shape::Line {
                a: Pt::new(0.0, 0.0),
                b: Pt::new(2.0, 0.0),
            },
            0.12,
            false,
        );
        assert!(line.depth(Pt::new(10.0, 5.0), &pad(11.0, 5.0), 0.15) > 0.15);
        assert_eq!(line.depth(Pt::new(10.0, 5.0), &pad(1.0, 0.0), 0.15), 0.0);
    }

    #[test]
    fn a_piece_is_short_of_an_edge_by_what_its_stroke_and_the_gap_ask_beyond_its_line() {
        // The inside of a 10 mm square, as the annealer measures it along the sides.
        let inside = |p: Pt| p.x.min(p.y).min(10.0 - p.x).min(10.0 - p.y);
        let ring = piece(
            &Shape::Circle {
                centre: Pt::new(0.0, 0.0),
                radius: 1.0,
            },
            0.2,
            false,
        );
        // Stood at x = 1, the circle's left point is on the side: the stroke reaches 0.1 mm
        // past its line, and 0.15 mm is asked.
        // (The circle is chords within 0.01 mm of the curve, so its nearest vertex may be that
        // much short of the curve's.)
        let v = ring.edge_depth(Pt::new(1.0, 5.0), 0.15, inside);
        assert!((v - 0.25).abs() <= SAGITTA + 1e-9, "{v}");
        // Far enough in, nothing; and the part's origin moves the piece.
        assert_eq!(ring.edge_depth(Pt::new(5.0, 5.0), 0.15, inside), 0.0);
        assert_eq!(ring.edge_depth(Pt::new(1.27, 5.0), 0.15, inside), 0.0);
        assert!(ring.edge_depth(Pt::new(1.2, 5.0), 0.15, inside) > 0.0);
    }

    #[test]
    fn a_part_has_a_piece_for_each_drawing_and_a_box_for_the_text_it_prints_besides_its_reference()
    {
        let board = crate::board::board_from_design(
            &crate::testlib::led_design(),
            &netsmith_kicad::Mapping::standard(),
            &crate::testlib::fixtures(),
        )
        .unwrap();
        let rules = board.rules.clone();
        let mut part = board.parts[0].clone();
        part.at = Pt::default();
        part.rotation = 0.0;
        let drawn = part
            .footprint
            .graphics
            .iter()
            .filter(|g| g.layer == netsmith_pcb::Layer::Silk)
            .count();
        part.footprint.silk_texts.clear();
        assert_eq!(pieces(&part, &rules).len(), drawn);
        part.footprint.silk_texts.push(netsmith_pcb::SilkText {
            text: "VALUE".into(),
            at: Pt::new(0.0, 3.0),
            angle: 0.0,
            height: 1.0,
            thickness: 0.15,
        });
        let found = pieces(&part, &rules);
        assert_eq!(found.len(), drawn + 1);
        let text = found.last().unwrap().bounds();
        assert!((text.centre().y - 3.0).abs() < 1e-9);
        assert!(text.width() > text.height());
        // Turned a quarter, the text stays as wide: its place turns, its letters do not.
        part.rotation = 90.0;
        let turned = pieces(&part, &rules).pop().unwrap().bounds();
        assert!((turned.width() - text.width()).abs() < 1e-9);
        assert!((turned.centre().x - 3.0).abs() < 1e-9 || (turned.centre().x + 3.0).abs() < 1e-9);
    }
}
