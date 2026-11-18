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
use crate::geom::{self, Arc, Bounds, Pt, circle_polygon};
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
    /// A simple polygon with no holes, turned as an area's outer ring runs.
    pub(crate) fn of(points: &[Pt]) -> Area {
        Area {
            outer: positive(points.to_vec(), true),
            holes: Vec::new(),
        }
    }

    /// Whether `p` is in the area (in its outer ring and in none of its holes).
    pub(crate) fn contains(&self, p: Pt) -> bool {
        geom::contains(&self.outer, p) && !self.holes.iter().any(|h| geom::contains(h, p))
    }

    /// The area in mm², the holes taken away.
    pub fn size(&self) -> f64 {
        let a = |p: &[Pt]| geom::area2(p).abs() / 2.0;
        a(&self.outer) - self.holes.iter().map(|h| a(h)).sum::<f64>()
    }

    /// The area as one ring with no holes, as KiCad keeps a zone's fill (a `filled_polygon` is
    /// a "fractured" polygon, issue #1184): each hole joined to the ring round it by a bridge
    /// of no width, two coincident edges that run from the hole's leftmost point straight to
    /// the left to the nearest edge, the holes from left to right (KiCad's own way,
    /// `SHAPE_POLY_SET::Fracture`). Read by the even-odd rule, as a Gerber region and KiCad
    /// read it, the ring is the area: its shoelace sum is the area's, and no bridge crosses an
    /// edge, since nothing of the ring lies between a hole and the nearest edge to its left,
    /// and a hole further right is wholly right of the bridge.
    pub fn fractured(&self) -> Vec<Pt> {
        let mut ring = self.outer.clone();
        let mut holes: Vec<(usize, &Vec<Pt>)> = self
            .holes
            .iter()
            .filter(|h| h.len() >= 3)
            .map(|h| {
                let left = (0..h.len())
                    .min_by(|&a, &b| h[a].x.total_cmp(&h[b].x).then(h[a].y.total_cmp(&h[b].y)))
                    .unwrap_or(0);
                (left, h)
            })
            .collect();
        holes.sort_by(|(a, ha), (b, hb)| {
            ha[*a]
                .x
                .total_cmp(&hb[*b].x)
                .then(ha[*a].y.total_cmp(&hb[*b].y))
        });
        for (left, hole) in holes {
            let p = hole[left];
            // The nearest edge the ray from `p` to the left crosses (half-open in y, so a ray
            // through a vertex crosses one of the two edges that meet there).
            let mut best: Option<(f64, usize)> = None;
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                if (a.y > p.y) == (b.y > p.y) {
                    continue;
                }
                let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
                if x <= p.x && best.is_none_or(|(bx, _)| x > bx) {
                    best = Some((x, i));
                }
            }
            // A hole always has the ring round it to its left; were rounding to miss it, the
            // nearest point of the ring stands in.
            let (at, i) = match best {
                Some((x, i)) => (Pt::new(x, p.y), i),
                None => {
                    let i = (0..ring.len())
                        .min_by(|&a, &b| ring[a].dist(p).total_cmp(&ring[b].dist(p)))
                        .unwrap_or(0);
                    (ring[i], i)
                }
            };
            let mut joined = Vec::with_capacity(ring.len() + hole.len() + 3);
            joined.extend_from_slice(&ring[..=i]);
            joined.push(at);
            joined.extend_from_slice(&hole[left..]);
            joined.extend_from_slice(&hole[..=left]);
            joined.push(at);
            joined.extend_from_slice(&ring[i + 1..]);
            ring = joined;
        }
        ring
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

/// The rings of areas: each outer ring positive and each hole negative, so that the non-zero
/// rule reads any list of them as the union of the areas.
fn rings_of(areas: &[Area]) -> Vec<Ring> {
    areas
        .iter()
        .flat_map(|a| std::iter::once(&a.outer).chain(&a.holes))
        .filter(|r| r.len() >= 3)
        .map(|r| r.iter().map(|p| [p.x, p.y]).collect())
        .collect()
}

/// What of `subject` is not in `clip`.
pub(crate) fn difference(subject: &[Area], clip: &[Area]) -> Vec<Area> {
    let (s, c) = (rings_of(subject), rings_of(clip));
    if s.is_empty() || c.is_empty() {
        return to_areas(s);
    }
    to_areas(flatten(s.overlay(
        &c,
        OverlayRule::Difference,
        FillRule::NonZero,
    )))
}

/// What of `a` is also in `b`.
pub(crate) fn intersection(a: &[Area], b: &[Area]) -> Vec<Area> {
    let (ra, rb) = (rings_of(a), rings_of(b));
    if ra.is_empty() || rb.is_empty() {
        return Vec::new();
    }
    to_areas(flatten(ra.overlay(
        &rb,
        OverlayRule::Intersect,
        FillRule::NonZero,
    )))
}

/// The union of `areas`, as areas that do not overlap.
pub(crate) fn union(areas: &[Area]) -> Vec<Area> {
    to_areas(rings_of(areas))
}

/// Whether `a` and `b` share area (more than touching).
pub(crate) fn overlap(a: &[Area], b: &[Area]) -> bool {
    let (ra, rb) = (rings_of(a), rings_of(b));
    if ra.is_empty() || rb.is_empty() {
        return false;
    }
    to_areas(flatten(ra.overlay(
        &rb,
        OverlayRule::Intersect,
        FillRule::NonZero,
    )))
    .iter()
    .any(|a| a.size() > 1e-9)
}

/// The areas grown by `by` mm: everything within `by` of them (each ring stroked with a round
/// pen `2 * by` wide, and joined to the areas).
pub(crate) fn grown(areas: &[Area], by: f64) -> Vec<Area> {
    let mut rings = rings_of(areas);
    for a in areas {
        for r in std::iter::once(&a.outer).chain(&a.holes) {
            let mut closed = r.clone();
            if let Some(&first) = r.first() {
                closed.push(first);
            }
            rings.extend(strokes(&closed, 2.0 * by));
        }
    }
    to_areas(rings)
}

/// The areas with the copper narrower than `min_width` taken out, as KiCad's fill trims a zone
/// to its `min_thickness`: each area is shrunk by half the width and the rest grown back by as
/// much, so a neck narrower than the width parts the area in two and a sliver goes.
pub(crate) fn trimmed(areas: &[Area], min_width: f64) -> Vec<Area> {
    opened(areas, min_width / 2.0, Corners::Round, Corners::Round)
}

/// How an offset draws the corners it goes round: with a round pen, or squared off at the
/// offset's distance from the corner along its bisector (Clipper's square join, which KiCad's
/// `CHAMFER_ALL_CORNERS` asks for), which reaches further than the round one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Corners {
    Round,
    Square,
}

/// The size under which a piece of shrunk copper is a speck of rounding ([`opened`]), in mm.
const SPECK: f64 = 0.001;

/// The areas shrunk by `by` with `shrink`'s corners and the rest grown back by as much with
/// `grow`'s: [`trimmed`] with round corners both ways.
pub(crate) fn opened(areas: &[Area], by: f64, shrink: Corners, grow: Corners) -> Vec<Area> {
    if by <= 0.0 || areas.is_empty() {
        return areas.to_vec();
    }
    // A speck of the shrunk copper narrower than a micrometre is what is left between the
    // pieces of the band by rounding, not copper as wide as the width: grown back it would be a
    // disc of copper where there was none.
    let core: Vec<Area> = difference(areas, &to_areas(band(areas, by, shrink)))
        .into_iter()
        .filter(|a| {
            Bounds::of(a.outer.iter().copied())
                .is_some_and(|b| (b.max.x - b.min.x).max(b.max.y - b.min.y) >= SPECK)
        })
        .collect();
    if core.is_empty() {
        return core;
    }
    let mut rings = rings_of(&core);
    rings.extend(band(&core, by, grow));
    to_areas(rings)
}

/// Everything within `by` of the rings of `areas`, on both sides, its corners drawn as
/// `corners` says.
fn band(areas: &[Area], by: f64, corners: Corners) -> Vec<Ring> {
    let mut band: Vec<Ring> = Vec::new();
    for a in areas {
        for r in std::iter::once(&a.outer).chain(&a.holes) {
            match corners {
                Corners::Round => {
                    let mut closed = r.clone();
                    if let Some(&first) = r.first() {
                        closed.push(first);
                    }
                    band.extend(strokes(&closed, 2.0 * by));
                }
                Corners::Square => band.extend(square_band(r, by)),
            }
        }
    }
    band
}

/// The band within `by` of a closed ring, as Clipper offsets it with square joins: a rectangle
/// along each side, and at each corner, on the side where the ring turns away (the outside of
/// the turn), the wedge between the two sides' rectangles squared off at `by` from the corner.
/// On the other side the rectangles overlap and need nothing more.
fn square_band(ring_pts: &[Pt], by: f64) -> Vec<Ring> {
    let n = ring_pts.len();
    let mut out = Vec::new();
    if n == 0 {
        return out;
    }
    let unit = |v: Pt| {
        let l = (v.x * v.x + v.y * v.y).sqrt();
        (l > 1e-12).then(|| v * (1.0 / l))
    };
    let dot = |a: Pt, b: Pt| a.x * b.x + a.y * b.y;
    // The normal to the right of a direction.
    let right = |d: Pt| Pt::new(d.y, -d.x);
    // Each rectangle reaches a nanometre past its side's ends, under the corners' squares, so
    // that no seam of rounding is left between them for the shrinking to keep.
    let seam = 1e-6;
    for i in 0..n {
        let (a, b) = (ring_pts[i], ring_pts[(i + 1) % n]);
        let Some(d) = unit(b - a) else { continue };
        let m = right(d) * by;
        let (a, b) = (a - d * seam, b + d * seam);
        out.extend(ring(&[a + m, b + m, b - m, a - m]));
    }
    for i in 0..n {
        let v = ring_pts[i];
        // The sides in and out of the corner, past any point repeated.
        let Some(d1) = (1..n)
            .map(|k| ring_pts[(i + n - k) % n])
            .find_map(|p| unit(v - p))
        else {
            continue;
        };
        let Some(d2) = (1..n)
            .map(|k| ring_pts[(i + k) % n])
            .find_map(|p| unit(p - v))
        else {
            continue;
        };
        let cross = d1.x * d2.y - d1.y * d2.x;
        if cross.abs() < 1e-12 && dot(d1, d2) > 0.0 {
            continue;
        }
        // The outside of the turn, and how far the square reaches along each side's offset past
        // the corner: `by * tan(turn / 4)`, the square being tangent to the round join at the
        // middle of the turn.
        let side = if cross >= 0.0 { 1.0 } else { -1.0 };
        let (na, nb) = (right(d1) * side, right(d2) * side);
        let turn = cross.abs().atan2(dot(d1, d2));
        let reach = by * (turn / 4.0).tan();
        out.extend(ring(&[
            v,
            v + na * by,
            v + na * by + d1 * reach,
            v + nb * by - d2 * reach,
            v + nb * by,
        ]));
    }
    out
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

    /// A 4 mm square with round bites out of it (circles cut into many short sides, as the fill's
    /// clearances are) and a stub 0.3 mm wide off one side.
    fn bitten() -> Vec<Area> {
        let mut p = Polygons::new();
        p.region(square(0.0, 0.0, 4.0), None, &[]);
        p.region(
            Contour::polygon(&[
                Pt::new(4.0, 1.85),
                Pt::new(4.6, 1.85),
                Pt::new(4.6, 2.15),
                Pt::new(4.0, 2.15),
            ]),
            None,
            &[],
        );
        p.polarity(false);
        for (x, y, r) in [
            (2.0, 0.0, 1.0),
            (4.0, 3.0, 0.6),
            (0.0, 2.0, 0.3),
            (1.0, 2.6, 0.4),
        ] {
            p.region(
                Contour::polygon(&geom::circle_polygon(Pt::new(x, y), r)),
                None,
                &[],
            );
        }
        p.areas()
    }

    #[test]
    fn a_square_shrink_leaves_no_speck_to_grow_into_a_disc() {
        // A bump 0.16 mm deep and 0.3 mm tall off a block, its end cut on a slant, as a spoke a
        // clearance cuts. The band's rectangles and squares once met at seams of rounding that
        // left specks of a square nanometre in the shrunk copper, each grown back into a disc
        // 0.24 mm across, past the bump's end.
        let stub = [
            (1.0, -0.5),
            (1.0, 1.0),
            (0.24, 1.0),
            (0.24, 0.4),
            (0.1618, 0.4),
            (0.1076, 0.2636),
            (0.0974, 0.2243),
            (0.08, 0.1858),
            (0.08, 0.11),
            (0.0816, 0.1),
            (0.24, 0.1),
            (0.24, -0.5),
        ];
        let pts: Vec<Pt> = stub.iter().map(|&(x, y)| Pt::new(x, y)).collect();
        let areas = vec![Area::of(&pts)];
        let total = |a: &[Area]| a.iter().map(Area::size).sum::<f64>();
        for corners in [Corners::Square, Corners::Round] {
            let o = opened(&areas, 0.122, corners, Corners::Round);
            let extra = total(&difference(&o, &grown(&areas, geom::ARC_ERROR)));
            assert!(extra < 1e-9, "{corners:?}: {extra} mm2 outside");
            assert!(
                !o.iter().any(|a| a.contains(Pt::new(0.02, 0.25))),
                "{corners:?}"
            );
        }
    }

    #[test]
    fn an_opening_never_adds_copper_and_square_corners_take_no_less() {
        let areas = bitten();
        let total = |a: &[Area]| a.iter().map(Area::size).sum::<f64>();
        for by in [0.05, 0.124, 0.2] {
            let round = opened(&areas, by, Corners::Round, Corners::Round);
            let square = opened(&areas, by, Corners::Square, Corners::Round);
            for (what, o) in [("round", &round), ("square", &square)] {
                // Nothing further out than the rounding of the curves.
                let extra = total(&difference(o, &grown(&areas, geom::ARC_ERROR)));
                assert!(extra < 1e-9, "{what} by {by}: {extra} mm2 outside");
            }
            let more = total(&difference(&square, &grown(&round, geom::ARC_ERROR)));
            assert!(
                more < 1e-9,
                "square by {by}: {more} mm2 the round keeps not"
            );
            // The stub, 0.3 mm wide, stays at 0.124 and goes at 0.2.
            let stub = |o: &[Area]| o.iter().any(|a| a.contains(Pt::new(4.4, 2.0)));
            assert_eq!(stub(&square), by < 0.15, "by {by}");
        }
    }

    /// A 10 mm square with three 2 mm holes: one at the left, one to its right on the same
    /// line (whose bridge meets the first hole), one lower down.
    fn holed() -> Area {
        let mut p = Polygons::new();
        p.region(square(0.0, 0.0, 10.0), None, &[]);
        p.polarity(false);
        for (x, y) in [(2.0, 4.0), (6.0, 4.0), (5.0, 7.5)] {
            p.region(square(x, y, 2.0), None, &[]);
        }
        let areas = p.areas();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].holes.len(), 3);
        areas.into_iter().next().unwrap()
    }

    #[test]
    fn a_fractured_area_is_one_ring_that_the_even_odd_rule_reads_as_the_area() {
        let area = holed();
        let ring = area.fractured();
        // Every point of the outline and of each hole, and two for each bridge's ends.
        assert_eq!(ring.len(), 4 + 3 * (4 + 1) + 3 * 2);
        assert!((geom::area2(&ring) / 2.0 - area.size()).abs() < 1e-9);
        assert!((area.size() - 88.0).abs() < 1e-6);
        // Read back as a Gerber region is read, it is the same copper.
        let mut back = Polygons::new();
        back.region(Contour::polygon(&ring), None, &[]);
        let read = back.areas();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].holes.len(), 3);
        assert!((read[0].size() - area.size()).abs() < 1e-6);
        for (p, copper) in [
            ((1.0, 1.0), true),
            ((3.0, 5.0), false),
            ((7.0, 5.0), false),
            ((6.0, 8.5), false),
            ((5.0, 5.0), true),
            ((9.0, 9.0), true),
        ] {
            assert_eq!(geom::contains(&ring, Pt::new(p.0, p.1)), copper, "{p:?}");
        }
        // Each bridge is level and ends on the hole's leftmost point.
        for w in ring.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a.x - b.x).abs() > 1e-12 && (a.y - b.y).abs() > 1e-12 {
                panic!("an edge that is not level or upright: {a:?} {b:?}");
            }
        }
    }

    #[test]
    fn an_area_with_no_holes_is_its_own_ring() {
        let mut p = Polygons::new();
        p.region(square(0.0, 0.0, 3.0), None, &[]);
        let area = p.areas().remove(0);
        assert_eq!(area.fractured(), area.outer);
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
    fn trimming_parts_an_area_at_a_neck_narrower_than_the_width_and_keeps_a_wide_one() {
        // Two 4 mm squares joined by a neck 0.2 mm wide, and two joined by one 0.6 mm wide.
        let dumbbell = |neck: f64| {
            let mut p = Polygons::new();
            p.region(square(0.0, 0.0, 4.0), None, &[]);
            p.region(square(6.0, 0.0, 4.0), None, &[]);
            p.region(
                Contour::polygon(&[
                    Pt::new(3.5, 2.0 - neck / 2.0),
                    Pt::new(6.5, 2.0 - neck / 2.0),
                    Pt::new(6.5, 2.0 + neck / 2.0),
                    Pt::new(3.5, 2.0 + neck / 2.0),
                ]),
                None,
                &[],
            );
            p.areas()
        };
        let thin = dumbbell(0.2);
        assert_eq!(thin.len(), 1);
        assert_eq!(trimmed(&thin, 0.25).len(), 2, "the thin neck is gone");
        let wide = dumbbell(0.6);
        let kept = trimmed(&wide, 0.25);
        assert_eq!(kept.len(), 1, "the wide neck stays");
        // The corners are rounded by the width's half, and no more is lost.
        let lost = wide[0].size() - kept[0].size();
        assert!(lost > 0.0 && lost < 0.1, "{lost}");
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
