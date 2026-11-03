//! The board outline: the lines, arcs, circles and polygons on `Edge.Cuts` joined into
//! closed paths. The path with the most area is the board's edge; the others are cutouts.

use crate::artwork::{Artwork, Shape};
use crate::geom::{Pt, area2, circle_polygon};

/// Ends this close are the same point, in mm (KiCad joins outline pieces within a few
/// micrometres).
const JOIN: f64 = 0.01;

#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    /// The edge of the board, as a polygon (arcs cut into short lines).
    pub outer: Vec<Pt>,
    pub cutouts: Vec<Vec<Pt>>,
}

/// Joins the loose `pieces` end to end into closed paths, and adds them to `closed`. A path
/// that does not close is an error naming where it starts and ends.
pub(crate) fn join_paths(
    pieces: Vec<Vec<Pt>>,
    mut closed: Vec<Vec<Pt>>,
) -> Result<Vec<Vec<Pt>>, (Pt, Pt)> {
    // A piece of no length (a library draws them) joins nothing and is nothing.
    let pieces: Vec<Vec<Pt>> = pieces
        .into_iter()
        .filter(|p| p.iter().any(|q| q.dist(p[0]) > 1e-9))
        .collect();
    let mut used = vec![false; pieces.len()];
    for first in 0..pieces.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let mut path = pieces[first].clone();
        loop {
            let (start, end) = (path[0], path[path.len() - 1]);
            if path.len() > 2 && start.dist(end) <= JOIN {
                path.pop();
                closed.push(path);
                break;
            }
            let next = (0..pieces.len()).find(|&i| {
                !used[i]
                    && (pieces[i][0].dist(end) <= JOIN
                        || pieces[i][pieces[i].len() - 1].dist(end) <= JOIN)
            });
            match next {
                Some(i) => {
                    used[i] = true;
                    let mut piece = pieces[i].clone();
                    if piece[0].dist(end) > JOIN {
                        piece.reverse();
                    }
                    path.extend(piece.into_iter().skip(1));
                }
                None => return Err((start, end)),
            }
        }
    }
    Ok(closed)
}

/// Whether `p` is on the edge of the closed path `path`, within [`JOIN`].
fn on_boundary(path: &[Pt], p: Pt) -> bool {
    (0..path.len()).any(|i| {
        let (a, b) = (path[i], path[(i + 1) % path.len()]);
        let ab = Pt::new(b.x - a.x, b.y - a.y);
        let len2 = ab.x * ab.x + ab.y * ab.y;
        let t = if len2 == 0.0 {
            0.0
        } else {
            (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
        };
        p.dist(Pt::new(a.x + t * ab.x, a.y + t * ab.y)) <= JOIN
    })
}

fn is_cut_line(piece: &[Pt], closed: &[Vec<Pt>]) -> bool {
    piece.len() == 2
        && piece
            .iter()
            .all(|end| closed.iter().any(|c| on_boundary(c, *end)))
}

/// The V-cut lines drawn on `Edge.Cuts`: straight lines with both ends on a closed path of the
/// outline, which are not part of it.
pub fn cut_lines(art: &Artwork) -> Vec<(Pt, Pt)> {
    let closed: Vec<Vec<Pt>> = art
        .graphics
        .iter()
        .filter(|g| g.layer == "Edge.Cuts")
        .filter_map(|g| match &g.shape {
            Shape::Poly(p) => Some(p.clone()),
            Shape::Circle { centre, radius } => Some(circle_polygon(*centre, *radius)),
            _ => None,
        })
        .collect();
    art.graphics
        .iter()
        .filter(|g| g.layer == "Edge.Cuts")
        .filter_map(|g| match &g.shape {
            Shape::Line(a, b) if is_cut_line(&[*a, *b], &closed) => Some((*a, *b)),
            _ => None,
        })
        .collect()
}

/// The outline of the board, or what is wrong with it.
pub fn outline(art: &Artwork) -> Result<Outline, String> {
    let mut closed: Vec<Vec<Pt>> = Vec::new();
    let mut pieces: Vec<Vec<Pt>> = Vec::new();
    for g in art.graphics.iter().filter(|g| g.layer == "Edge.Cuts") {
        match &g.shape {
            Shape::Line(a, b) => pieces.push(vec![*a, *b]),
            Shape::Arc(arc) => pieces.push(arc.points()),
            Shape::Circle { centre, radius } => closed.push(circle_polygon(*centre, *radius)),
            Shape::Poly(p) => closed.push(p.clone()),
        }
    }
    // A V-cut panel draws its cut lines on `Edge.Cuts`, as JLCPCB asks (ADR 0040). A straight
    // line with both ends on the edge of a closed path is such a line, not a piece of an
    // outline, and it joins nothing.
    let pieces: Vec<Vec<Pt>> = pieces
        .into_iter()
        .filter(|p| !is_cut_line(p, &closed))
        .collect();
    let mut closed = join_paths(pieces, closed).map_err(|(start, end)| {
        format!(
            "the board outline on Edge.Cuts is not closed: a path from ({}, {}) ends at ({}, {}) with nothing joining it within {JOIN} mm",
            start.x, start.y, end.x, end.y
        )
    })?;
    if closed.is_empty() {
        return Err("the board has no outline: nothing is drawn on Edge.Cuts".to_string());
    }
    let (at, _) = closed
        .iter()
        .enumerate()
        .map(|(i, p)| (i, area2(p).abs()))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0, 0.0));
    let outer = closed.remove(at);
    Ok(Outline {
        outer,
        cutouts: closed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artwork::Artwork;

    fn board(items: &str) -> Artwork {
        Artwork::parse(&format!(
            "(kicad_pcb (layers (0 \"F.Cu\" signal) (2 \"B.Cu\" signal)) {items})"
        ))
        .unwrap()
    }

    fn line(a: (f64, f64), b: (f64, f64)) -> String {
        format!(
            "(gr_line (start {} {}) (end {} {}) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\"))",
            a.0, a.1, b.0, b.1
        )
    }

    #[test]
    fn pieces_of_no_length_join_nothing_and_do_not_stop_a_loop_from_closing() {
        let p = |x: f64, y: f64| Pt::new(x, y);
        let square = vec![
            vec![p(0.0, 0.0), p(2.0, 0.0)],
            vec![p(2.0, 0.0), p(2.0, 0.0)],
            vec![p(2.0, 0.0), p(2.0, 2.0)],
            vec![p(2.0, 2.0), p(0.0, 2.0)],
            vec![p(0.0, 2.0), p(0.0, 0.0)],
            vec![p(1.0, 1.0), p(1.0, 1.0)],
        ];
        let closed = join_paths(square, Vec::new()).unwrap();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].len(), 4);
        let open = join_paths(vec![vec![p(0.0, 0.0), p(2.0, 0.0)]], Vec::new());
        assert_eq!(open, Err((p(0.0, 0.0), p(2.0, 0.0))));
    }

    #[test]
    fn loose_lines_in_any_order_and_direction_make_a_closed_outline_and_a_cutout() {
        let items = [
            line((0.0, 0.0), (10.0, 0.0)),
            line((10.0, 10.0), (10.0, 0.0)),
            line((10.0, 10.0), (0.0, 10.0)),
            line((0.0, 10.0), (0.0, 0.0)),
            "(gr_circle (center 5 5) (end 6 5) (stroke (width 0.05) (type default)) (fill no) (layer \"Edge.Cuts\"))".to_string(),
        ]
        .join("\n");
        let o = outline(&board(&items)).unwrap();
        assert_eq!(o.outer.len(), 4);
        assert_eq!(o.cutouts.len(), 1);
        assert!((area2(&o.outer).abs() - 200.0).abs() < 1e-9);
    }

    #[test]
    fn a_cut_line_across_a_closed_outline_is_not_part_of_it() {
        let items = format!(
            "(gr_rect (start 0 0) (end 10 10) (stroke (width 0.05) (type default)) (fill no) (layer \"Edge.Cuts\")){}",
            line((5.0, 0.0), (5.0, 10.0))
        );
        let o = outline(&board(&items)).unwrap();
        assert_eq!(o.outer.len(), 4);
        assert!(o.cutouts.is_empty());
        // A line that ends inside the board is still an outline that is not closed.
        let items = format!(
            "(gr_rect (start 0 0) (end 10 10) (stroke (width 0.05) (type default)) (fill no) (layer \"Edge.Cuts\")){}",
            line((5.0, 0.0), (5.0, 7.0))
        );
        assert!(outline(&board(&items)).unwrap_err().contains("not closed"));
    }

    #[test]
    fn an_open_outline_names_where_it_breaks() {
        let items = [
            line((0.0, 0.0), (10.0, 0.0)),
            line((10.0, 0.0), (10.0, 10.0)),
        ]
        .join("\n");
        let err = outline(&board(&items)).unwrap_err();
        assert!(
            err.contains("not closed") && err.contains("(0, 0)") && err.contains("(10, 10)"),
            "{err}"
        );
    }

    #[test]
    fn a_board_with_no_outline_is_refused() {
        let err = outline(&board("")).unwrap_err();
        assert!(err.contains("no outline"), "{err}");
    }

    #[test]
    fn a_rounded_rectangle_made_of_lines_and_arcs_closes() {
        let arc = |s: (f64, f64), m: (f64, f64), e: (f64, f64)| {
            format!(
                "(gr_arc (start {} {}) (mid {} {}) (end {} {}) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\"))",
                s.0, s.1, m.0, m.1, e.0, e.1
            )
        };
        let k = 1.0 - std::f64::consts::FRAC_1_SQRT_2;
        let items = [
            line((1.0, 0.0), (9.0, 0.0)),
            arc((9.0, 0.0), (10.0 - k, k), (10.0, 1.0)),
            line((10.0, 1.0), (10.0, 9.0)),
            arc((10.0, 9.0), (10.0 - k, 10.0 - k), (9.0, 10.0)),
            line((9.0, 10.0), (1.0, 10.0)),
            arc((1.0, 10.0), (k, 10.0 - k), (0.0, 9.0)),
            line((0.0, 9.0), (0.0, 1.0)),
            arc((0.0, 1.0), (k, k), (1.0, 0.0)),
        ]
        .join("\n");
        let o = outline(&board(&items)).unwrap();
        assert!(o.outer.len() > 8);
        assert!(area2(&o.outer).abs() / 2.0 < 100.0 && area2(&o.outer).abs() / 2.0 > 95.0);
    }
}
