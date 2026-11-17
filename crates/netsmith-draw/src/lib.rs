//! Flat vector pictures and what netsmith writes them as, with no browser, no KiCad and no
//! font file (ADR 0142, issue #1091). One interface, three adapters:
//!
//! - [`svg::read`] reads the SVG netsmith writes (the schematic of `netsmith-view`, the board of
//!   `netsmith-pcb`, any drawing made the same way) into a [`Picture`]: its paths, its shapes and
//!   its text, with the styles of its `<style>` sheet, its presentation attributes and its
//!   transforms worked out. It reads the subset of SVG and CSS such drawings use (listed in the
//!   module), never runs a script, and leaves out what draws nothing (a transparent hit area).
//! - [`pdf::write`] writes pictures as a PDF, one [`pdf::Page`] for each region of a picture:
//!   vector paths, text in the PDF's standard fonts (Helvetica, Courier, Symbol), so no font is
//!   embedded and the text can be searched and copied, each page sized to its region with a
//!   title block. The same pages give the same bytes: no date, no random identifier.
//! - [`raster::render`] fills a region of a picture into pixels, anti-aliased, with the text in
//!   netsmith's stroke font (`netsmith_geom::font_strokes`), and [`raster::png`] writes them as a
//!   PNG with netsmith's own writer (`netsmith_core::png`). The same picture gives the same bytes.
//!
//! A picture is in the SVG's user units (netsmith's drawings are in millimetres), y down, as the
//! SVG has it. A page and a raster take the region to show ([`Rect`]), so one picture can be cut
//! into sheets without being read again.
//!
//! The schematic's export is built on this (`netsmith_view::export`); the board's copper picture
//! (`view pcb`, #1086) and the Gerber's (`view gerber`, #1105) read their SVG the same way.

pub mod pdf;
pub mod raster;
pub mod svg;

mod css;
mod geometry;
mod metrics;

pub use metrics::advance;

/// A point, in the picture's units, y down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub const fn new(x: f64, y: f64) -> Pt {
        Pt { x, y }
    }
}

/// An axis-aligned rectangle from `min` (top left) to `max`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: Pt,
    pub max: Pt,
}

impl Rect {
    /// The rectangle at `x`, `y` (its top left) of `width` by `height`.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            min: Pt::new(x, y),
            max: Pt::new(x + width, y + height),
        }
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    /// Grown by `by` on every side.
    pub fn inflated(&self, by: f64) -> Rect {
        Rect {
            min: Pt::new(self.min.x - by, self.min.y - by),
            max: Pt::new(self.max.x + by, self.max.y + by),
        }
    }

    /// Whether the two share any point.
    pub fn touches(&self, other: &Rect) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
    }

    /// The smallest rectangle holding both.
    pub fn union(&self, other: &Rect) -> Rect {
        Rect {
            min: Pt::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Pt::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    pub(crate) fn around(points: impl IntoIterator<Item = Pt>) -> Option<Rect> {
        let mut out: Option<Rect> = None;
        for p in points {
            let r = Rect { min: p, max: p };
            out = Some(out.map_or(r, |o| o.union(&r)));
        }
        out
    }
}

/// A colour with its opacity, each 0 to 255.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba::opaque(0, 0, 0);
    pub const WHITE: Rgba = Rgba::opaque(255, 255, 255);

    pub const fn opaque(r: u8, g: u8, b: u8) -> Rgba {
        Rgba { r, g, b, a: 255 }
    }

    /// With its opacity multiplied by `by` (0 to 1).
    pub fn faded(self, by: f64) -> Rgba {
        Rgba {
            a: (f64::from(self.a) * by.clamp(0.0, 1.0)).round() as u8,
            ..self
        }
    }
}

/// One piece of a path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    /// Start a new subpath here.
    Move(Pt),
    Line(Pt),
    /// A cubic Bézier curve: two control points and the end.
    Cubic(Pt, Pt, Pt),
    /// Back to the start of the subpath.
    Close,
}

/// A path: subpaths of lines and curves. An arc or a circle of the SVG is in it as cubic curves.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    pub segs: Vec<Seg>,
}

impl Path {
    /// Every point of the path, control points included: a box round them holds the path.
    pub fn points(&self) -> impl Iterator<Item = Pt> + '_ {
        self.segs.iter().flat_map(|s| match *s {
            Seg::Move(p) | Seg::Line(p) => vec![p],
            Seg::Cubic(a, b, c) => vec![a, b, c],
            Seg::Close => vec![],
        })
    }

    /// The subpaths as lines, each curve cut so that no point of it is further than
    /// `tolerance` from the lines, and whether each is closed.
    pub fn flatten(&self, tolerance: f64) -> Vec<(Vec<Pt>, bool)> {
        geometry::flatten(self, tolerance)
    }
}

/// How a fill decides what is inside a path that crosses itself or has holes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

/// The ends of an open line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    Butt,
    Round,
    Square,
}

/// Where two pieces of a line meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    Miter,
    Round,
    Bevel,
}

/// How a path's outline is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub colour: Rgba,
    pub width: f64,
    pub cap: Cap,
    pub join: Join,
    /// Dash and gap lengths, in turn; empty for a solid line.
    pub dash: Vec<f64>,
}

/// A filled or outlined path.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub path: Path,
    pub fill: Option<(Rgba, FillRule)>,
    pub stroke: Option<Stroke>,
}

/// Which end of a line of text its point is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// The kind of letters: proportional or monospaced, plain or bold. The PDF draws them in
/// Helvetica and Courier; the rasterizer in its one stroke font, thicker for bold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Face {
    Sans,
    SansBold,
    Mono,
    MonoBold,
}

impl Face {
    pub fn bold(self) -> bool {
        matches!(self, Face::SansBold | Face::MonoBold)
    }

    pub fn mono(self) -> bool {
        matches!(self, Face::Mono | Face::MonoBold)
    }
}

/// A piece of a line of text, with a line over it or not (KiCad's `~{...}`, an active-low name).
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub overline: bool,
}

/// One line of text, its baseline through `at`.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub at: Pt,
    /// The turn of the baseline in degrees, clockwise on the page (y down), as SVG's `rotate`.
    pub angle: f64,
    /// The size of the letters (the em), in the picture's units.
    pub size: f64,
    pub anchor: Anchor,
    pub face: Face,
    /// The colour of the letters; `None` draws only the halo.
    pub fill: Option<Rgba>,
    /// An outline drawn under the letters, so they read over the lines they cross: its colour
    /// and its width (SVG's `stroke` with `paint-order: stroke`).
    pub halo: Option<(Rgba, f64)>,
    pub runs: Vec<Run>,
}

impl Text {
    /// The whole line.
    pub fn string(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// The point where the line starts, after the anchor: `at` moved back along the baseline
    /// by the share of its width (in the PDF's letters) the anchor says.
    pub fn start(&self) -> Pt {
        let width = metrics::advance(self.face, &self.string(), self.size);
        let back = match self.anchor {
            Anchor::Start => 0.0,
            Anchor::Middle => width / 2.0,
            Anchor::End => width,
        };
        let (s, c) = self.angle.to_radians().sin_cos();
        Pt::new(self.at.x - back * c, self.at.y - back * s)
    }

    /// A box that holds the line, its halo included.
    pub fn bounds(&self) -> Rect {
        let width = metrics::advance(self.face, &self.string(), self.size);
        let start = self.start();
        let (s, c) = self.angle.to_radians().sin_cos();
        let halo = self.halo.map_or(0.0, |(_, w)| w);
        // From a descender below the baseline to an overline above the capitals.
        let (below, above) = (0.25 * self.size + halo, 1.05 * self.size + halo);
        let along = |d: f64| Pt::new(start.x + d * c, start.y + d * s);
        // "Up" from the baseline on a y-down page.
        let up = |p: Pt, h: f64| Pt::new(p.x + h * s, p.y - h * c);
        let (a, b) = (along(-halo), along(width + halo));
        Rect::around([up(a, above), up(b, above), up(a, -below), up(b, -below)])
            .expect("four points")
    }
}

/// What is drawn, in order: later items over earlier ones.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Shape(Shape),
    Text(Text),
}

impl Item {
    /// A box that holds what the item draws.
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Item::Shape(s) => {
                let half = s.stroke.as_ref().map_or(0.0, |st| st.width / 2.0);
                Rect::around(s.path.points()).map(|r| r.inflated(half))
            }
            Item::Text(t) => Some(t.bounds()),
        }
    }
}

/// A drawing: its items and the region its document says it shows (an SVG's `viewBox`).
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub view: Rect,
    pub items: Vec<Item>,
}

impl Picture {
    /// Every line of text, in drawing order.
    pub fn texts(&self) -> impl Iterator<Item = &Text> {
        self.items.iter().filter_map(|i| match i {
            Item::Text(t) => Some(t),
            Item::Shape(_) => None,
        })
    }
}
