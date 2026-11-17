//! The pours and zones the PCB view draws under the tracks and pads (`GET /api/pcb/pours`).
//!
//! The fill is the one the Gerbers and the 3D view hold ([`netsmith_pcb::pours_of`]: the board's
//! pours and planes and the zones of the routes file, by one filler, each layer's pours less the
//! clearances round other nets' copper, the thermal reliefs, the holes and the edge band), so
//! the 2D view shows what `fab` plots. It is **not** KiCad's fill of a zone (KiCad refills with
//! its own rules; the editor never runs KiCad for the picture).
//!
//! The server sends the fill and the page draws it: one SVG path for each net on each layer, in
//! mm on the sheet. The filler cuts a curve into pieces no further than 2 µm from it, which for
//! a board with hundreds of pads is tens of thousands of points the page never needs, so each
//! outline is rounded to a micrometre and loses the points within [`TOLERANCE`] of the line
//! between their neighbours (Douglas and Peucker's rule, [`simplify`]). Orientation is kept (the
//! outlines run clockwise on the screen and the holes the other way), so the page fills a path
//! by the non-zero rule and an overlap of two pieces of one net does not punch a hole.

use std::fmt::Write as _;

use netsmith_editor::api::{PcbPourLayer, PcbPourNet, PcbPours};
use netsmith_pcb::{Area, Board, Pt};

use super::pcb::Entry;

/// How far a point may be from the line between the points kept on each side of it, in mm.
const TOLERANCE: f64 = 0.002;

/// A placement to fill: the board and the stamps of what it was placed from, copied out of the
/// entry so that the fill, which takes longer than anything else the view asks for, runs with
/// no lock of the workspace held.
pub struct Placement {
    board: Board,
    layout_stamp: String,
    routes_stamp: String,
    routed: bool,
}

impl Placement {
    pub fn of(entry: &Entry) -> Placement {
        Placement {
            board: entry.board.clone(),
            layout_stamp: entry.layout_stamp().to_string(),
            routes_stamp: entry.routes_stamp().to_string(),
            routed: entry.is_routed(),
        }
    }

    /// The fill of the board, as the scene of `name` has it placed (and routed): by copper
    /// layer, front to back, and on each layer by net. A layer with no fill is left out. A board
    /// that cannot be filled (its file would not plot either) is answered with the reason and no
    /// layers.
    pub fn pours(self, name: &str) -> PcbPours {
        let (layers, error) = match netsmith_pcb::pours_of(&self.board) {
            Ok(layers) => (
                layers
                    .into_iter()
                    .filter(|l| !l.pours.is_empty())
                    .map(|l| PcbPourLayer {
                        layer: l.layer,
                        nets: by_net(l.pours),
                    })
                    .collect(),
                None,
            ),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        PcbPours {
            board: name.to_string(),
            layout_stamp: self.layout_stamp,
            routes_stamp: self.routes_stamp,
            routed: self.routed,
            layers,
            error,
        }
    }
}

/// The pieces of one layer, joined into one path for each net, in the order a net first appears.
fn by_net(pours: Vec<netsmith_pcb::Pour>) -> Vec<PcbPourNet> {
    let mut nets: Vec<(Option<String>, String)> = Vec::new();
    for pour in pours {
        let at = match nets.iter().position(|(n, _)| *n == pour.net) {
            Some(at) => at,
            None => {
                nets.push((pour.net, String::new()));
                nets.len() - 1
            }
        };
        area(&mut nets[at].1, &pour.area);
    }
    nets.into_iter()
        .map(|(net, d)| PcbPourNet { net, d })
        .collect()
}

/// The subpaths of an area: its outline, then its holes. A ring that is no more than a line after
/// simplifying is no copper and is left out.
fn area(d: &mut String, a: &Area) {
    for ring in std::iter::once(&a.outer).chain(&a.holes) {
        let ring = simplify(ring);
        if ring.len() < 3 {
            continue;
        }
        for (i, p) in ring.iter().enumerate() {
            let _ = write!(d, "{}{} {}", if i == 0 { 'M' } else { 'L' }, p.x, p.y);
        }
        d.push('Z');
    }
}

/// A closed ring rounded to a micrometre and simplified within [`TOLERANCE`]
/// ([`netsmith_pcb::simplify_ring`], which the board's picture shares).
fn simplify(ring: &[Pt]) -> Vec<Pt> {
    netsmith_pcb::simplify_ring(ring, TOLERANCE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_pcb::distance_to_segment as off;

    /// A circle of `n` points, clockwise on the screen (y down).
    fn circle(r: f64, n: usize) -> Vec<Pt> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                Pt::new(10.0 + r * a.cos(), 5.0 + r * a.sin())
            })
            .collect()
    }

    /// Twice the signed area of a ring (positive when it runs clockwise on the screen).
    fn shoelace(ring: &[Pt]) -> f64 {
        (0..ring.len())
            .map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                a.x * b.y - b.x * a.y
            })
            .sum()
    }

    #[test]
    fn a_finely_cut_circle_loses_most_points_and_keeps_its_shape_and_direction() {
        let ring = circle(3.0, 4000);
        let kept = simplify(&ring);
        assert!(kept.len() < 400, "{} points", kept.len());
        assert!(kept.len() > 20);
        assert!(shoelace(&kept) > 0.0, "the direction is kept");
        let (was, now) = (shoelace(&ring), shoelace(&kept));
        // The area is within the strip the tolerance and the rounding allow along the edge.
        let strip = std::f64::consts::TAU * 3.0 * (TOLERANCE + 0.0005) * 2.0;
        assert!((was - now).abs() / 2.0 < strip, "{was} {now}");
        for p in &ring {
            let nearest = kept
                .iter()
                .enumerate()
                .map(|(i, a)| off(*a, kept[(i + 1) % kept.len()], *p))
                .fold(f64::MAX, f64::min);
            assert!(nearest <= TOLERANCE + 0.0008, "{nearest}");
        }
    }

    #[test]
    fn collinear_and_repeated_points_go_and_the_corners_stay() {
        let ring = [
            Pt::new(0.0, 0.0),
            Pt::new(1.0, 0.0),
            Pt::new(1.0, 0.0),
            Pt::new(2.0, 0.0),
            Pt::new(2.0, 1.0),
            Pt::new(2.0, 2.0),
            Pt::new(0.0, 2.0),
            Pt::new(0.0, 1.0),
        ];
        assert_eq!(
            simplify(&ring),
            [
                Pt::new(0.0, 0.0),
                Pt::new(2.0, 0.0),
                Pt::new(2.0, 2.0),
                Pt::new(0.0, 2.0)
            ]
        );
    }

    #[test]
    fn a_ring_that_is_a_line_is_no_copper() {
        let mut d = String::new();
        area(
            &mut d,
            &Area {
                outer: vec![Pt::new(0.0, 0.0), Pt::new(1.0, 0.0), Pt::new(2.0, 0.0)],
                holes: Vec::new(),
            },
        );
        assert_eq!(d, "");
    }

    #[test]
    fn an_area_is_an_outline_then_its_holes_with_no_negative_zero() {
        let mut d = String::new();
        area(
            &mut d,
            &Area {
                outer: vec![
                    Pt::new(-0.0001, 0.0),
                    Pt::new(4.0, 0.0),
                    Pt::new(4.0, 4.0),
                    Pt::new(0.0, 4.0),
                ],
                holes: vec![vec![
                    Pt::new(1.0, 1.0),
                    Pt::new(1.0, 2.0),
                    Pt::new(2.0, 2.0),
                    Pt::new(2.0, 1.0),
                ]],
            },
        );
        assert_eq!(d, "M0 0L4 0L4 4L0 4ZM1 1L1 2L2 2L2 1Z");
    }
}
