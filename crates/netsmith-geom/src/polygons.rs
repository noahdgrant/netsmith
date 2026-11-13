//! A [`Pen`] that keeps what the writers draw as polygons with holes instead of writing a
//! file: the copper of a board as data (ADR 0105), from the same drawing code as the Gerbers.
//!
//! Each object becomes polygons the way a Gerber reader sees it: a flash is its aperture's
//! shape, a stroke is the area a round pen sweeps (each straight piece a stadium), and a
//! region is its contour, filled by the even-odd rule (so the frame with a slit that
//! [`crate::fill`] clears the outside of the board with is the frame less the outline). Then
//! the objects are combined in order, as the file's polarity says: dark adds, clear takes away.
//! [`Polygons::areas`] is the result.
//!
//! Curves are cut into straight pieces no further than [`crate::geom::ARC_ERROR`] (2 µm) from the
//! true curve, inside it, so a polygon is at most that much smaller than the shape the file
//! draws. Points are in KiCad's board coordinates: mm, x to the right, y down.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;

use crate::gbr::{Ap, Contour, Pen, Seg};
use crate::geom::{self, Arc, Pt, circle_polygon};
use crate::pads::rounded_rect_polygon;

/// A polygon with holes. `outer` runs so that its shoelace sum ([`geom::area2`]) is
/// positive (clockwise on the screen, where y is down), and each hole the other way. A hole
/// holds no copper; a piece of copper inside a hole is an [`Area`] of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub outer: Vec<Pt>,
    pub holes: Vec<Vec<Pt>>,
}

impl Area {
    /// The area in mm², the holes taken away.
    pub fn size(&self) -> f64 {
        let a = |p: &[Pt]| geom::area2(p).abs() / 2.0;
        a(&self.outer) - self.holes.iter().map(|h| a(h)).sum::<f64>()
    }
}

type Ring = Vec<[f64; 2]>;

/// What was drawn in one polarity, one after the other, with the net it was drawn for.
#[derive(Debug, Clone)]
struct Run {
    dark: bool,
    /// The object's net attribute (`N,GND`), for [`Polygons::areas_by_net`].
    net: Option<String>,
    rings: Vec<Ring>,
}

/// The pen. It starts dark, as a Gerber file does.
#[derive(Debug, Clone)]
pub struct Polygons {
    dark: bool,
    runs: Vec<Run>,
}

impl Default for Polygons {
    fn default() -> Self {
        Self::new()
    }
}

impl Polygons {
    pub fn new() -> Self {
        Self {
            dark: true,
            runs: Vec::new(),
        }
    }

    /// True if nothing has been drawn.
    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|r| r.rings.is_empty())
    }

    /// Everything drawn, as the file shows it.
    pub fn areas(&self) -> Vec<Area> {
        to_areas(self.evaluate(|_| true))
    }

    /// What was drawn dark with each net attribute (`N,<net>` as [`crate::gbr::escape`] writes
    /// it; `None` for objects with none), each less everything drawn clear after it. The union
    /// of all of them is [`Polygons::areas`]. In the order the nets were first drawn.
    pub fn areas_by_net(&self) -> Vec<(Option<String>, Vec<Area>)> {
        let mut nets: Vec<Option<String>> = Vec::new();
        for r in self.runs.iter().filter(|r| r.dark) {
            if !nets.contains(&r.net) {
                nets.push(r.net.clone());
            }
        }
        nets.into_iter()
            .map(|n| {
                let areas = to_areas(self.evaluate(|r| r.net == n));
                (n, areas)
            })
            .collect()
    }

    /// The dark runs `keep` accepts, each clear run taken from what is dark before it.
    fn evaluate(&self, keep: impl Fn(&Run) -> bool) -> Vec<Ring> {
        // Dark rings are only collected (the non-zero rule joins them); each clear run is one
        // overlay, and [`to_areas`] joins what is left.
        let mut current: Vec<Ring> = Vec::new();
        for run in &self.runs {
            if run.dark {
                if keep(run) {
                    current.extend(run.rings.iter().cloned());
                }
            } else if !current.is_empty() {
                current = flatten(current.overlay(
                    &run.rings,
                    OverlayRule::Difference,
                    FillRule::NonZero,
                ));
            }
        }
        current
    }

    /// Adds rings, each already turned so its sum is positive (or a simplified shape's holes,
    /// negative), to the run for this polarity and net.
    fn put(&mut self, attrs: &[String], rings: Vec<Ring>) {
        if rings.is_empty() {
            return;
        }
        let net = if self.dark {
            attrs.iter().find(|a| a.starts_with("N,")).cloned()
        } else {
            None
        };
        match self.runs.last_mut() {
            Some(last) if last.dark == self.dark && last.net == net => last.rings.extend(rings),
            _ => self.runs.push(Run {
                dark: self.dark,
                net,
                rings,
            }),
        }
    }
}

/// The shapes of an overlay as one list of rings: each shape's outer ring and its holes.
fn flatten(shapes: Vec<Vec<Ring>>) -> Vec<Ring> {
    shapes.into_iter().flatten().collect()
}

/// Rings (outer rings positive, holes negative, as an overlay gives them) as areas: each
/// hole goes with the outer ring that holds it.
fn to_areas(rings: Vec<Ring>) -> Vec<Area> {
    // An overlay with nothing to add gives its shapes back in order, outer ring first.
    let none: Vec<Ring> = Vec::new();
    let shapes = rings.overlay(&none, OverlayRule::Union, FillRule::NonZero);
    shapes
        .into_iter()
        .filter_map(|shape| {
            let mut rings = shape.into_iter().map(|r| {
                r.into_iter()
                    .map(|[x, y]| Pt::new(x, y))
                    .collect::<Vec<Pt>>()
            });
            let outer = positive(rings.next()?, true);
            let holes = rings.map(|h| positive(h, false)).collect();
            Some(Area { outer, holes })
        })
        .collect()
}

/// The ring turned so its shoelace sum is positive (`true`) or negative.
fn positive(ring: Vec<Pt>, sign: bool) -> Vec<Pt> {
    if (geom::area2(&ring) > 0.0) == sign {
        ring
    } else {
        geom::reversed(&ring)
    }
}

/// A simple polygon as a ring with a positive sum, or nothing if it has no area.
fn ring(points: &[Pt]) -> Option<Ring> {
    if points.len() < 3 {
        return None;
    }
    let a = geom::area2(points);
    if a.abs() < 1e-12 {
        return None;
    }
    let mut r: Ring = points.iter().map(|p| [p.x, p.y]).collect();
    if a < 0.0 {
        r.reverse();
    }
    Some(r)
}

/// The area a round pen `width` wide sweeps from `a` to `b`.
fn stadium(a: Pt, b: Pt, width: f64) -> Option<Ring> {
    if width <= 0.0 {
        return None;
    }
    if a.dist(b) < 1e-12 {
        return ring(&circle_polygon(a, width / 2.0));
    }
    let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    // The turn that takes the x axis along a -> b, KiCad's way (y down).
    let angle = (-(b.y - a.y)).atan2(b.x - a.x).to_degrees();
    ring(&rounded_rect_polygon(
        mid,
        a.dist(b) + width,
        width,
        width / 2.0,
        angle,
    ))
}

/// The area a round pen sweeps along a path of straight pieces.
fn strokes(points: &[Pt], width: f64) -> Vec<Ring> {
    if points.len() == 1 {
        return stadium(points[0], points[0], width).into_iter().collect();
    }
    points
        .windows(2)
        .filter_map(|w| stadium(w[0], w[1], width))
        .collect()
}

/// An offset in the file's frame (y up) as one in the board's (y down).
fn board(at: Pt, offset: Pt) -> Pt {
    Pt::new(at.x + offset.x, at.y - offset.y)
}

/// The shape of an aperture flashed at `at`.
fn aperture(ap: &Ap, at: Pt) -> Vec<Ring> {
    match ap {
        Ap::Circle(d) => {
            if *d <= 0.0 {
                Vec::new()
            } else {
                ring(&circle_polygon(at, d / 2.0)).into_iter().collect()
            }
        }
        Ap::Rect(w, h) => ring(&rounded_rect_polygon(at, *w, *h, 0.0, 0.0))
            .into_iter()
            .collect(),
        Ap::Obround(w, h) => ring(&rounded_rect_polygon(at, *w, *h, w.min(*h) / 2.0, 0.0))
            .into_iter()
            .collect(),
        Ap::RotRect { w, h, angle } => {
            // The macro turns counter-clockwise in the file's frame, which is counter-clockwise
            // on the screen too: KiCad's sense of a positive angle.
            ring(&rounded_rect_polygon(at, *w, *h, 0.0, *angle))
                .into_iter()
                .collect()
        }
        Ap::RoundRect { radius, corners } => {
            // As the macro draws it: the four corners' polygon, a circle at each corner and a
            // stroke along each side.
            let c: Vec<Pt> = corners.iter().map(|p| board(at, *p)).collect();
            let mut out: Vec<Ring> = ring(&c).into_iter().collect();
            if *radius > 0.0 {
                for i in 0..4 {
                    out.extend(stadium(c[i], c[(i + 1) % 4], 2.0 * radius));
                }
            }
            out
        }
        Ap::HorizOval { width, a, b } => stadium(board(at, *a), board(at, *b), *width)
            .into_iter()
            .collect(),
    }
}

/// A region's contour as points, its arcs cut into straight pieces.
fn contour_points(c: &Contour) -> Vec<Pt> {
    let mut pts = vec![c.start];
    for seg in &c.segs {
        let last = *pts.last().expect("the contour has a start");
        match *seg {
            Seg::Line(p) => pts.push(p),
            Seg::Arc { to, centre, ccw } => {
                let arc = Arc {
                    start: last,
                    end: to,
                    centre,
                    radius: last.dist(centre),
                    ccw,
                };
                pts.extend(arc.points().into_iter().skip(1));
            }
        }
    }
    if pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    pts
}

impl Pen for Polygons {
    fn polarity(&mut self, dark: bool) {
        self.dark = dark;
    }

    fn flash(&mut self, ap: Ap, _function: Option<&'static str>, at: Pt, attrs: &[String]) {
        let rings = aperture(&ap, at);
        self.put(attrs, rings);
    }

    fn line(
        &mut self,
        width: f64,
        _function: Option<&'static str>,
        a: Pt,
        b: Pt,
        attrs: &[String],
    ) {
        let rings = stadium(a, b, width).into_iter().collect();
        self.put(attrs, rings);
    }

    fn arc(&mut self, width: f64, _function: Option<&'static str>, arc: &Arc, attrs: &[String]) {
        let rings = strokes(&arc.points(), width);
        self.put(attrs, rings);
    }

    fn region(&mut self, contour: Contour, _function: Option<&'static str>, attrs: &[String]) {
        let pts = contour_points(&contour);
        if pts.len() < 3 {
            return;
        }
        let r: Ring = pts.iter().map(|p| [p.x, p.y]).collect();
        // Even-odd, as a reader fills a contour that crosses or touches itself.
        let rings = flatten(vec![r].simplify_shape(FillRule::EvenOdd));
        self.put(attrs, rings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, s: f64) -> Contour {
        Contour::polygon(&[
            Pt::new(x0, y0),
            Pt::new(x0 + s, y0),
            Pt::new(x0 + s, y0 + s),
            Pt::new(x0, y0 + s),
        ])
    }

    #[test]
    fn a_clear_square_makes_a_hole_and_dark_drawn_after_it_fills_it_again() {
        let mut p = Polygons::new();
        p.region(square(0.0, 0.0, 10.0), None, &[]);
        p.polarity(false);
        p.region(square(2.0, 2.0, 4.0), None, &[]);
        let areas = p.areas();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].holes.len(), 1);
        assert!((areas[0].size() - 84.0).abs() < 1e-6);
        assert!(geom::area2(&areas[0].outer) > 0.0);
        assert!(geom::area2(&areas[0].holes[0]) < 0.0);
        p.polarity(true);
        p.region(square(3.0, 3.0, 2.0), None, &[]);
        let areas = p.areas();
        assert_eq!(
            areas.len(),
            2,
            "the island in the hole is an area of its own"
        );
        let total: f64 = areas.iter().map(Area::size).sum();
        assert!((total - 88.0).abs() < 1e-6);
    }

    #[test]
    fn a_stroke_is_a_stadium_and_a_flash_its_aperture() {
        let mut p = Polygons::new();
        p.line(1.0, None, Pt::new(0.0, 0.0), Pt::new(4.0, 0.0), &[]);
        let a = p.areas();
        let want = 4.0 + std::f64::consts::PI * 0.25;
        assert!((a[0].size() - want).abs() < 0.01, "{}", a[0].size());

        let mut p = Polygons::new();
        p.flash(Ap::Rect(2.0, 1.0), None, Pt::new(5.0, 5.0), &[]);
        assert!((p.areas()[0].size() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_round_rectangle_aperture_is_turned_into_the_boards_frame() {
        // Corners in the file's frame (y up): the box spans y 0..1 in the file, which is
        // y -1..0 on the board.
        let mut p = Polygons::new();
        let corners = [
            Pt::new(0.0, 0.0),
            Pt::new(2.0, 0.0),
            Pt::new(2.0, 1.0),
            Pt::new(0.0, 1.0),
        ];
        p.flash(
            Ap::RoundRect {
                radius: 0.1,
                corners,
            },
            None,
            Pt::new(10.0, 10.0),
            &[],
        );
        let a = &p.areas()[0];
        let ys: Vec<f64> = a.outer.iter().map(|q| q.y).collect();
        let (lo, hi) = (
            ys.iter().copied().fold(f64::MAX, f64::min),
            ys.iter().copied().fold(f64::MIN, f64::max),
        );
        assert!(
            (lo - 8.9).abs() < 1e-3 && (hi - 10.1).abs() < 1e-3,
            "{lo} {hi}"
        );
    }

    #[test]
    fn the_net_of_each_dark_object_is_kept() {
        let mut p = Polygons::new();
        p.region(square(0.0, 0.0, 4.0), None, &["N,GND".into()]);
        p.region(square(10.0, 0.0, 4.0), None, &["N,3V3".into()]);
        p.polarity(false);
        p.line(1.0, None, Pt::new(-1.0, 2.0), Pt::new(20.0, 2.0), &[]);
        let by = p.areas_by_net();
        assert_eq!(by.len(), 2);
        assert_eq!(by[0].0.as_deref(), Some("N,GND"));
        assert_eq!(by[0].1.len(), 2, "the clear line cuts the square in two");
        assert_eq!(by[1].0.as_deref(), Some("N,3V3"));
    }

    #[test]
    fn a_frame_with_a_slit_is_the_frame_less_what_it_goes_round() {
        // As the fill clears the outside of the board: a frame, then the outline the other way.
        let frame = [
            Pt::new(-1.0, -1.0),
            Pt::new(11.0, -1.0),
            Pt::new(11.0, 11.0),
            Pt::new(-1.0, 11.0),
        ];
        let outline = [
            Pt::new(0.0, 0.0),
            Pt::new(0.0, 10.0),
            Pt::new(10.0, 10.0),
            Pt::new(10.0, 0.0),
        ];
        let mut ring_pts = frame.to_vec();
        ring_pts.push(frame[0]);
        ring_pts.push(outline[0]);
        ring_pts.extend(outline.iter().skip(1));
        ring_pts.push(outline[0]);
        ring_pts.push(frame[0]);
        let mut p = Polygons::new();
        p.region(square(-1.0, -1.0, 12.0), None, &[]);
        p.polarity(false);
        p.region(Contour::polygon(&ring_pts), None, &[]);
        let a = p.areas();
        assert_eq!(a.len(), 1);
        assert!((a[0].size() - 100.0).abs() < 1e-6, "{}", a[0].size());
    }
}
