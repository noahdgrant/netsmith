//! A picture as pixels: every shape filled with anti-aliased edges (the share of each pixel a
//! shape covers, sampled on [`ROWS`] lines a pixel and measured exactly along each), every line
//! drawn as the polygons of its outline with round joins, and text in netsmith's stroke font
//! (`netsmith_geom::font_strokes`), its halo first. The same picture and region give the same
//! pixels: the work is in a fixed order and nothing is random.
//!
//! The letters are not the PDF's: the stroke font is a single-line font drawn for silkscreen,
//! about as wide as Helvetica. A picture to read is what it is for.

use netsmith_geom::{ArtworkText, HAlign, VAlign};

use crate::geometry::{dashes, outline};
use crate::{Anchor, FillRule, Item, Picture, Pt, Rect, Rgba, Shape, Text};

/// The lines a pixel row is sampled on.
pub const ROWS: usize = 4;

/// The most pixels along a side of a picture.
pub const MAX_SIDE: usize = 16_384;

/// Pixels, four bytes each (red, green, blue, opacity), row after row from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// The region `view` of `picture` at `px_per_mm` pixels to a unit of the picture (a millimetre
/// in netsmith's drawings), on `background`. A side is at least one pixel and at most
/// [`MAX_SIDE`].
pub fn render(picture: &Picture, view: Rect, px_per_mm: f64, background: Rgba) -> Image {
    let side = |len: f64| ((len * px_per_mm).ceil() as usize).clamp(1, MAX_SIDE);
    let (width, height) = (side(view.width()), side(view.height()));
    let mut canvas = Canvas {
        width,
        height,
        rgb: [background.r, background.g, background.b]
            .repeat(width * height)
            .into_iter()
            .map(f64::from)
            .collect(),
        view,
        scale: px_per_mm,
    };
    // Half a pixel round the region, so a shape just outside it still touches its edge.
    let seen = view.inflated(1.0 / px_per_mm);
    for item in &picture.items {
        if item.bounds().is_none_or(|b| !b.touches(&seen)) {
            continue;
        }
        match item {
            Item::Shape(shape) => canvas.shape(shape),
            Item::Text(text) => canvas.text(text),
        }
    }
    let mut rgba = Vec::with_capacity(width * height * 4);
    for px in canvas.rgb.as_chunks::<3>().0 {
        rgba.extend(px.iter().map(|v| v.round().clamp(0.0, 255.0) as u8));
        rgba.push(255);
    }
    Image {
        width,
        height,
        rgba,
    }
}

/// `image` as a PNG, with netsmith's own writer.
pub fn png(image: &Image) -> Vec<u8> {
    netsmith_core::png::rgba(image.width, image.height, &image.rgba)
}

/// A pixel scale for `view` that gives `per_mm` pixels a unit, or less so that the longer side
/// is at most `longest` pixels.
pub fn scale_for(view: Rect, per_mm: f64, longest: usize) -> f64 {
    let long = view.width().max(view.height()).max(1e-9);
    per_mm.min(longest as f64 / long)
}

struct Canvas {
    width: usize,
    height: usize,
    /// Three channels a pixel, 0 to 255, kept unrounded until the end.
    rgb: Vec<f64>,
    view: Rect,
    scale: f64,
}

impl Canvas {
    fn px(&self, p: Pt) -> Pt {
        Pt::new(
            (p.x - self.view.min.x) * self.scale,
            (p.y - self.view.min.y) * self.scale,
        )
    }

    /// The tolerance of curves and discs, in the picture's units: a fifth of a pixel.
    fn tolerance(&self) -> f64 {
        0.2 / self.scale
    }

    fn shape(&mut self, shape: &Shape) {
        let tol = self.tolerance();
        let subpaths = shape.path.flatten(tol);
        if let Some((colour, rule)) = shape.fill {
            let polys: Vec<Vec<Pt>> = subpaths
                .iter()
                .filter(|(p, _)| p.len() > 2)
                .map(|(p, _)| p.iter().map(|q| self.px(*q)).collect())
                .collect();
            self.fill(&polys, rule, colour);
        }
        if let Some(stroke) = &shape.stroke {
            let mut polys = Vec::new();
            for (points, closed) in &subpaths {
                let mut line = points.clone();
                if *closed && line.len() > 1 {
                    line.push(line[0]);
                }
                let pieces = if stroke.dash.is_empty() {
                    vec![(line, *closed)]
                } else {
                    dashes(&line, &stroke.dash)
                        .into_iter()
                        .map(|d| (d, false))
                        .collect()
                };
                for (piece, closed) in pieces {
                    polys.extend(outline(&piece, closed, stroke.width, stroke.cap, tol));
                }
            }
            let polys: Vec<Vec<Pt>> = polys
                .into_iter()
                .map(|p| p.into_iter().map(|q| self.px(q)).collect())
                .collect();
            self.fill(&polys, FillRule::NonZero, stroke.colour);
        }
    }

    fn text(&mut self, text: &Text) {
        let size = text.size;
        let string: String = text.string();
        let glyphs = ArtworkText {
            layer: String::new(),
            text: string.clone(),
            at: netsmith_geom::Pt::new(text.at.x, text.at.y),
            angle: -text.angle,
            size: (0.72 * size, 0.72 * size),
            thickness: 0.0,
            mirror: false,
            italic: false,
            h: match text.anchor {
                Anchor::Start => HAlign::Left,
                Anchor::Middle => HAlign::Centre,
                Anchor::End => HAlign::Right,
            },
            v: VAlign::Bottom,
            owner: String::new(),
        };
        let (paths, _) = netsmith_geom::font_strokes(&glyphs);
        let lines: Vec<Vec<Pt>> = paths
            .iter()
            .map(|p| p.iter().map(|q| Pt::new(q.x, q.y)).collect())
            .collect();
        // The overlines, from where each run starts and ends along the line in the font's own
        // widths.
        let mut over: Vec<Vec<Pt>> = Vec::new();
        if text.runs.iter().any(|r| r.overline) {
            let width_of = |s: &str| {
                let probe = ArtworkText {
                    text: s.to_string(),
                    at: netsmith_geom::Pt::new(0.0, 0.0),
                    angle: 0.0,
                    h: HAlign::Left,
                    ..glyphs.clone()
                };
                let (paths, _) = netsmith_geom::font_strokes(&probe);
                paths.iter().flatten().map(|p| p.x).fold(0.0f64, f64::max)
            };
            let total = width_of(&string);
            let start = match text.anchor {
                Anchor::Start => 0.0,
                Anchor::Middle => -total / 2.0,
                Anchor::End => -total,
            };
            let (s, c) = text.angle.to_radians().sin_cos();
            let at = |along: f64, up: f64| {
                Pt::new(
                    text.at.x + along * c + up * s,
                    text.at.y + along * s - up * c,
                )
            };
            let mut before = String::new();
            for run in &text.runs {
                let from = width_of(&before);
                before.push_str(&run.text);
                if run.overline {
                    let to = width_of(&before);
                    over.push(vec![
                        at(start + from, 0.85 * size),
                        at(start + to, 0.85 * size),
                    ]);
                }
            }
        }
        let weight = if text.face.bold() { 0.14 } else { 0.09 } * size;
        let tol = self.tolerance();
        let draw = |canvas: &mut Canvas, width: f64, colour: Rgba| {
            let mut polys = Vec::new();
            for line in lines.iter().chain(&over) {
                polys.extend(outline(line, false, width, crate::Cap::Round, tol));
            }
            let polys: Vec<Vec<Pt>> = polys
                .into_iter()
                .map(|p| p.into_iter().map(|q| canvas.px(q)).collect())
                .collect();
            canvas.fill(&polys, FillRule::NonZero, colour);
        };
        if let Some((colour, width)) = text.halo {
            draw(self, weight + width, colour);
        }
        if let Some(colour) = text.fill {
            draw(self, weight, colour);
        }
    }

    /// Fill `polys` (in pixels) under `rule` with `colour`, each pixel by the share of it they
    /// cover.
    fn fill(&mut self, polys: &[Vec<Pt>], rule: FillRule, colour: Rgba) {
        if colour.a == 0 {
            return;
        }
        let mut edges: Vec<Edge> = Vec::new();
        let (mut x_lo, mut x_hi, mut y_lo, mut y_hi) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for poly in polys {
            for (i, a) in poly.iter().enumerate() {
                let b = poly[(i + 1) % poly.len()];
                x_lo = x_lo.min(a.x);
                x_hi = x_hi.max(a.x);
                y_lo = y_lo.min(a.y);
                y_hi = y_hi.max(a.y);
                if a.y == b.y {
                    continue;
                }
                let (top, bottom, dir) = if a.y < b.y { (*a, b, 1) } else { (b, *a, -1) };
                edges.push(Edge {
                    y0: top.y,
                    y1: bottom.y,
                    x0: top.x,
                    slope: (bottom.x - top.x) / (bottom.y - top.y),
                    dir,
                });
            }
        }
        if edges.is_empty() {
            return;
        }
        let row_lo = y_lo.floor().max(0.0) as usize;
        let row_hi = (y_hi.ceil().max(0.0) as usize).min(self.height);
        let col_lo = x_lo.floor().max(0.0) as usize;
        let col_hi = (x_hi.ceil().max(0.0) as usize).min(self.width);
        if row_lo >= row_hi || col_lo >= col_hi {
            return;
        }
        edges.sort_by(|a, b| a.y0.total_cmp(&b.y0));
        let span = col_hi - col_lo;
        let mut cover = vec![0.0f64; span + 1];
        let mut active: Vec<usize> = Vec::new();
        let mut next = 0;
        let mut crossings: Vec<(f64, i32)> = Vec::new();
        let alpha = f64::from(colour.a) / 255.0;
        let paint = [
            f64::from(colour.r),
            f64::from(colour.g),
            f64::from(colour.b),
        ];
        for row in row_lo..row_hi {
            cover.iter_mut().for_each(|c| *c = 0.0);
            let mut touched = false;
            for s in 0..ROWS {
                let y = row as f64 + (s as f64 + 0.5) / ROWS as f64;
                while next < edges.len() && edges[next].y0 <= y {
                    active.push(next);
                    next += 1;
                }
                active.retain(|&e| edges[e].y1 > y);
                crossings.clear();
                for &e in &active {
                    let edge = &edges[e];
                    if edge.y0 <= y {
                        crossings.push((edge.x0 + (y - edge.y0) * edge.slope, edge.dir));
                    }
                }
                if crossings.is_empty() {
                    continue;
                }
                crossings.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let mut winding = 0;
                for pair in 0..crossings.len() {
                    let (x, dir) = crossings[pair];
                    winding += dir;
                    let inside = match rule {
                        FillRule::NonZero => winding != 0,
                        FillRule::EvenOdd => winding % 2 != 0,
                    };
                    if inside && let Some(&(end, _)) = crossings.get(pair + 1) {
                        add_span(&mut cover, x - col_lo as f64, end - col_lo as f64, span);
                        touched = true;
                    }
                }
            }
            if !touched {
                continue;
            }
            for (i, c) in cover.iter().take(span).enumerate() {
                if *c <= 0.0 {
                    continue;
                }
                let a = alpha * (c / ROWS as f64).min(1.0);
                let at = (row * self.width + col_lo + i) * 3;
                for (k, v) in paint.iter().enumerate() {
                    let px = &mut self.rgb[at + k];
                    *px += (v - *px) * a;
                }
            }
        }
    }
}

struct Edge {
    y0: f64,
    y1: f64,
    x0: f64,
    slope: f64,
    dir: i32,
}

/// Add the share of each pixel from `a` to `b` (columns of `cover`) covers.
fn add_span(cover: &mut [f64], a: f64, b: f64, span: usize) {
    let (a, b) = (a.max(0.0), b.min(span as f64));
    if b <= a {
        return;
    }
    let (ia, ib) = (a.floor() as usize, b.floor() as usize);
    if ia == ib {
        cover[ia] += b - a;
        return;
    }
    cover[ia] += ia as f64 + 1.0 - a;
    for c in &mut cover[ia + 1..ib] {
        *c += 1.0;
    }
    if ib < cover.len() {
        cover[ib] += b - ib as f64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Path, Seg};

    #[test]
    fn a_square_covers_its_pixels_and_half_of_its_edge() {
        let square = Shape {
            path: Path {
                segs: vec![
                    Seg::Move(Pt::new(1.0, 1.0)),
                    Seg::Line(Pt::new(3.5, 1.0)),
                    Seg::Line(Pt::new(3.5, 3.0)),
                    Seg::Line(Pt::new(1.0, 3.0)),
                    Seg::Close,
                ],
            },
            fill: Some((Rgba::BLACK, FillRule::NonZero)),
            stroke: None,
        };
        let picture = Picture {
            view: Rect::new(0.0, 0.0, 5.0, 4.0),
            items: vec![Item::Shape(square)],
        };
        let img = render(&picture, picture.view, 1.0, Rgba::WHITE);
        let at = |x: usize, y: usize| img.rgba[(y * img.width + x) * 4];
        assert_eq!((img.width, img.height), (5, 4));
        assert_eq!(at(0, 0), 255);
        assert_eq!(at(1, 1), 0);
        assert_eq!(at(2, 2), 0);
        // Half of the column at x = 3 is covered.
        assert_eq!(at(3, 1), 128);
        assert_eq!(at(4, 2), 255);
    }
}
