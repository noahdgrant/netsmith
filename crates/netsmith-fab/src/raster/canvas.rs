//! A colour picture drawn by the same scanline fill as the Gerber rasteriser, for pictures an
//! agent opens (a PNG it can read, where an SVG it cannot): the router's stages and its
//! crowding map (`netsmith route --stages`). Coordinates are millimetres in a board's own
//! frame, x to the right and **y down**, the frame of `netsmith_pcb::Board`.
//!
//! A shape is filled where its polygons' winding number is not zero, so overlapping polygons
//! of one fill are their union and each pixel is blended once. A pixel is inside when its
//! centre is: no anti-aliasing, so the same shapes give the same bytes.

use netsmith_core::png;

use super::geom::{Map, Rule, fill};

/// A point in millimetres, y down.
pub type Point = (f64, f64);

/// An RGB picture of a rectangle of a board.
#[derive(Debug, Clone, PartialEq)]
pub struct Canvas {
    /// The board point at the top left corner of the picture, in millimetres.
    pub origin: Point,
    pub px_per_mm: f64,
    pub width: usize,
    pub height: usize,
    /// Three bytes a pixel, row after row from the top.
    pub rgb: Vec<u8>,
}

impl Canvas {
    /// A picture of `width` by `height` pixels in one colour, its top left at `origin`.
    pub fn new(
        origin: Point,
        px_per_mm: f64,
        width: usize,
        height: usize,
        background: [u8; 3],
    ) -> Canvas {
        let mut rgb = Vec::with_capacity(width * height * 3);
        for _ in 0..width * height {
            rgb.extend_from_slice(&background);
        }
        Canvas {
            origin,
            px_per_mm,
            width,
            height,
            rgb,
        }
    }

    fn map(&self) -> Map {
        // `Map` takes y up: the points go in with y negated.
        Map {
            scale: self.px_per_mm,
            ox: -self.origin.0 * self.px_per_mm,
            oy: -self.origin.1 * self.px_per_mm,
        }
    }

    /// Fill the union of `polys` with `colour`, `alpha` of 255 covering what is under it.
    pub fn fill(&mut self, polys: &[Vec<Point>], colour: [u8; 3], alpha: u8) {
        let flipped: Vec<Vec<(f64, f64)>> = polys
            .iter()
            .map(|p| p.iter().map(|&(x, y)| (x, -y)).collect())
            .collect();
        let (w, h, map) = (self.width, self.height, self.map());
        let rgb = &mut self.rgb;
        let a = u32::from(alpha);
        fill(&flipped, map, Rule::NonZero, w, h, |row, from, to| {
            for px in rgb[(row * w + from) * 3..(row * w + to) * 3]
                .as_chunks_mut::<3>()
                .0
            {
                for (c, &new) in px.iter_mut().zip(&colour) {
                    *c = ((u32::from(*c) * (255 - a) + u32::from(new) * a + 127) / 255) as u8;
                }
            }
        });
    }

    /// Fill the rectangle from `min` to `max`.
    pub fn rect(&mut self, min: Point, max: Point, colour: [u8; 3], alpha: u8) {
        let ring = vec![min, (max.0, min.1), max, (min.0, max.1)];
        self.fill(&[ring], colour, alpha);
    }

    /// Draw each path as a line of `width` millimetres with round ends and joins.
    pub fn strokes(&mut self, paths: &[Vec<Point>], width: f64, colour: [u8; 3], alpha: u8) {
        let mut polys = Vec::new();
        for path in paths {
            for pair in path.windows(2) {
                polys.push(capsule(pair[0], pair[1], width / 2.0));
            }
            if let [p] = path.as_slice() {
                polys.push(disc(*p, width / 2.0));
            }
        }
        self.fill(&polys, colour, alpha);
    }

    /// The picture as an RGBA PNG, every pixel opaque.
    pub fn to_png(&self) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(self.width * self.height * 4);
        for px in self.rgb.as_chunks::<3>().0 {
            rgba.extend_from_slice(px);
            rgba.push(255);
        }
        png::rgba(self.width, self.height, &rgba)
    }
}

/// The vertices of a circle in a picture: enough at the sizes these pictures draw.
const ROUND: usize = 16;

/// A circle of radius `r` about `c`, as a polygon.
pub fn disc(c: Point, r: f64) -> Vec<Point> {
    (0..ROUND)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / ROUND as f64;
            (c.0 + r * a.cos(), c.1 + r * a.sin())
        })
        .collect()
}

/// The points within `r` of the segment from `a` to `b`, as a polygon.
pub fn capsule(a: Point, b: Point, r: f64) -> Vec<Point> {
    let theta = (b.1 - a.1).atan2(b.0 - a.0);
    let half = ROUND / 2;
    let mut out = Vec::with_capacity(2 * half + 2);
    for (centre, from) in [
        (b, theta - std::f64::consts::FRAC_PI_2),
        (a, theta + std::f64::consts::FRAC_PI_2),
    ] {
        for k in 0..=half {
            let t = from + std::f64::consts::PI * k as f64 / half as f64;
            out.push((centre.0 + r * t.cos(), centre.1 + r * t.sin()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filled_rectangle_covers_the_pixels_whose_centres_are_inside_y_down() {
        let mut c = Canvas::new((10.0, 20.0), 2.0, 20, 10, [0, 0, 0]);
        // Board x 11..12, y 21..22: pixels 2..4 across, 2..4 down.
        c.rect((11.0, 21.0), (12.0, 22.0), [255, 0, 0], 255);
        let at = |col: usize, row: usize| &c.rgb[(row * 20 + col) * 3..(row * 20 + col) * 3 + 3];
        assert_eq!(at(2, 2), [255, 0, 0]);
        assert_eq!(at(3, 3), [255, 0, 0]);
        assert_eq!(at(4, 2), [0, 0, 0]);
        assert_eq!(at(2, 4), [0, 0, 0]);
        assert_eq!(at(1, 1), [0, 0, 0]);
        let lit = c
            .rgb
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|p| p[0] == 255)
            .count();
        assert_eq!(lit, 4);
    }

    #[test]
    fn half_alpha_blends_and_overlapping_shapes_of_one_fill_blend_once() {
        let mut c = Canvas::new((0.0, 0.0), 1.0, 4, 4, [0, 0, 200]);
        let square = vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)];
        c.fill(&[square.clone(), square], [200, 0, 0], 128);
        assert_eq!(&c.rgb[..3], &[100, 0, 100]);
        let png = c.to_png();
        assert_eq!(&png[1..4], b"PNG");
        assert_eq!(png, c.to_png());
    }
}
