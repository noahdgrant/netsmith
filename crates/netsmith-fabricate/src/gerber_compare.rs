//! The copper of the board model's picture against the copper of the Gerbers' picture (issue
//! #1142): `netsmith view pcb` draws the board netsmith holds, `netsmith view gerber` draws the
//! files `board fab` wrote as netsmith's own reader reads them back, and where the two agree the
//! writer and the reader agree with the model. It needs no KiCad.
//!
//! **One grid.** The two pictures are of one layer, in one frame (mm from the outline's top left
//! corner, y down), over one region and at one scale: `view pcb` and `view gerber` with the same
//! `--layer`, `--region` and `--scale` give pictures of the same size, pixel for pixel over the
//! same place of the board, the bottom turned over the same way (each about the middle of its
//! region). With no `--region` both draw the outline with a millimetre round it.
//!
//! **What is copper.** Each picture is read back as drawn ([`netsmith_draw::svg::read`]), and its
//! shapes are told apart by the colours the drawing gives them: a shape in a copper colour
//! ([`netsmith_pcb::PICTURE_COPPER`], [`netsmith_fab::viewer::COPPER_COLOURS`]) is copper, a
//! shape in a hole's colour cuts the copper as the hole does, and everything else (the board's
//! fill, the outline, the mask, the paste, the silkscreen, a keepout, an open connection) is left
//! out. A pour drawn see-through counts as copper as a pad does. What is left is filled white on
//! black, and a pixel is copper when at least half of it is.
//!
//! **The comparison** is the cross-check's with KiCad (ADR 0034, `netsmith_fab::crosscheck`):
//! the fraction of copper pixels the two disagree on, and the fraction with no copper of the
//! other picture within a tolerance (a square of side `2 * tolerance + 1` pixels), so that an
//! edge a pixel off, where one cuts an arc into lines, is not a difference. The pixels beyond
//! the tolerance are grouped into places (pixels that touch, corners included), each with its
//! box in the frame, as `--region` takes it, and the side that has copper there: what to draw
//! with `--region` to see it.

use netsmith_draw::{Item, Rgba, raster, svg};
use netsmith_fab::RasterImage;

/// How finely and how closely the pictures are compared.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Pixels a millimetre of both pictures.
    pub scale: f64,
    /// Within this distance, in mm, a copper pixel of one picture needs copper in the other.
    pub tolerance: f64,
    /// The most copper pixels the two disagree on, as a fraction of the copper pixels of the
    /// picture with more: an edge a pixel off, where one cuts an arc into lines.
    pub pixels: f64,
    /// The most copper pixels with nothing of the other picture within the tolerance, as the
    /// same fraction.
    pub beyond: f64,
}

impl Default for Limits {
    /// The cross-check's with KiCad: 40 pixels a millimetre, 1% of the pixels, 0.1% beyond
    /// 0.05 mm. The examples of `netsmith-cli`'s `tests/gerber_against_pcb.rs` differ on 0.21%
    /// at most and on none beyond the tolerance (2026-11-17); a pad a third of a millimetre off,
    /// or a teardrop one picture lacks, is far beyond.
    fn default() -> Limits {
        Limits {
            scale: 40.0,
            tolerance: 0.05,
            pixels: 0.01,
            beyond: 0.001,
        }
    }
}

/// Which picture has copper at a place where the other has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Only {
    /// The board model's (`view pcb`).
    Model,
    /// The Gerbers' (`view gerber`).
    Files,
}

impl std::fmt::Display for Only {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Only::Model => "only the board model",
            Only::Files => "only the Gerbers",
        })
    }
}

/// A place where one picture has copper and the other has none within the tolerance.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub only: Only,
    pub pixels: usize,
    /// Its box in the frame (mm from the outline's top left corner, y down, seen from above):
    /// `x`, `y`, `width`, `height`, as `--region` takes it.
    pub region: [f64; 4],
}

impl std::fmt::Display for Place {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let [x, y, w, h] = self.region;
        write!(
            f,
            "{}: {} pixel(s) at --region {x:.2},{y:.2},{w:.2},{h:.2}",
            self.only, self.pixels
        )
    }
}

/// How the copper of one layer compares.
#[derive(Debug, Clone, PartialEq)]
pub struct Agreement {
    /// The pictures' size in pixels.
    pub width: usize,
    pub height: usize,
    /// The copper pixels of the board model's picture and of the Gerbers'.
    pub model: usize,
    pub files: usize,
    /// The pixels one has copper on and the other not, as a fraction of the copper pixels of
    /// the picture with more.
    pub pixels: f64,
    /// The same of the pixels with no copper of the other picture within the tolerance.
    pub beyond: f64,
    /// Where those are, the largest first.
    pub places: Vec<Place>,
}

impl Agreement {
    /// Whether the two agree within `limits`.
    pub fn ok(&self, limits: &Limits) -> bool {
        self.pixels <= limits.pixels && self.beyond <= limits.beyond
    }
}

impl std::fmt::Display for Agreement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} copper pixels in the board model's picture, {} in the Gerbers'; they differ on \
             {:.3}% of them, {:.3}% beyond the tolerance",
            self.model,
            self.files,
            self.pixels * 100.0,
            self.beyond * 100.0
        )?;
        for p in &self.places {
            write!(f, "\n  {p}")?;
        }
        Ok(())
    }
}

/// Compares the copper of `model` (the SVG `view pcb` writes) with that of `files` (the SVG
/// `view gerber` writes) of one layer, drawn over the same region; `from_below` when both are
/// of the board seen from below. The region is the Gerbers' picture's (its `viewBox` is the
/// region in the frame). The error says the two are not on one grid, or that a picture cannot
/// be read.
pub fn compare(
    model: &str,
    files: &str,
    from_below: bool,
    limits: &Limits,
) -> Result<Agreement, String> {
    let colours = |list: &[&str]| -> Vec<[u8; 3]> { list.iter().filter_map(|c| rgb(c)).collect() };
    let [r, g, b] = netsmith_pcb::PICTURE_BACKGROUND;
    let model_mask = mask(
        model,
        &colours(&netsmith_pcb::PICTURE_COPPER),
        &[[r, g, b]],
        limits.scale,
    )
    .map_err(|e| format!("the board model's picture: {e}"))?;
    let (files_mask, region) = {
        let picture = svg::read(files).map_err(|e| format!("the Gerbers' picture: {e}"))?;
        let v = picture.view;
        (
            mask_of(
                picture,
                &colours(&netsmith_fab::viewer::COPPER_COLOURS),
                &colours(&netsmith_fab::viewer::HOLE_COLOURS),
                limits.scale,
            ),
            [v.min.x, v.min.y, v.width(), v.height()],
        )
    };
    if (model_mask.width, model_mask.height) != (files_mask.width, files_mask.height) {
        return Err(format!(
            "the pictures are not on one grid: the board model's is {} by {} pixels, the Gerbers' {} by {}; draw both with the same --region and --scale",
            model_mask.width, model_mask.height, files_mask.width, files_mask.height
        ));
    }
    let tolerance = (limits.tolerance * limits.scale).round() as usize;
    let near_files = files_mask.dilate(tolerance);
    let near_model = model_mask.dilate(tolerance);
    let (model_lit, files_lit) = (model_mask.lit(), files_mask.lit());
    let most = model_lit.max(files_lit).max(1) as f64;
    let differ = model_mask
        .bits
        .iter()
        .zip(&files_mask.bits)
        .filter(|(a, b)| a != b)
        .count();
    let beyond_model = only(&model_mask, &near_files);
    let beyond_files = only(&files_mask, &near_model);
    let count = |bits: &[u8]| bits.iter().filter(|b| **b != 0).count();
    let beyond = (count(&beyond_model) + count(&beyond_files)) as f64 / most;
    let grid = Grid {
        width: model_mask.width,
        height: model_mask.height,
        scale: limits.scale,
        region,
        from_below,
    };
    let mut places = grid.places(&beyond_model, Only::Model);
    places.extend(grid.places(&beyond_files, Only::Files));
    places.sort_by(|a, b| {
        b.pixels.cmp(&a.pixels).then(
            a.region
                .partial_cmp(&b.region)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    Ok(Agreement {
        width: grid.width,
        height: grid.height,
        model: model_lit,
        files: files_lit,
        pixels: differ as f64 / most,
        beyond,
        places,
    })
}

/// `#rrggbb` as red, green and blue.
fn rgb(hex: &str) -> Option<[u8; 3]> {
    let h = hex.strip_prefix('#')?;
    let at = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    Some([at(0)?, at(2)?, at(4)?])
}

/// The copper of the picture `text` as a bitmap at `scale` over its `viewBox`.
fn mask(
    text: &str,
    copper: &[[u8; 3]],
    holes: &[[u8; 3]],
    scale: f64,
) -> Result<RasterImage, String> {
    let picture = svg::read(text).map_err(|e| e.to_string())?;
    Ok(mask_of(picture, copper, holes, scale))
}

fn mask_of(
    mut picture: netsmith_draw::Picture,
    copper: &[[u8; 3]],
    holes: &[[u8; 3]],
    scale: f64,
) -> RasterImage {
    // A colour of copper is white, of a hole black, and anything else is not drawn.
    let paint = |c: Rgba| -> Option<Rgba> {
        let c = [c.r, c.g, c.b];
        if copper.contains(&c) {
            Some(Rgba::WHITE)
        } else if holes.contains(&c) {
            Some(Rgba::BLACK)
        } else {
            None
        }
    };
    picture.items.retain_mut(|item| {
        let Item::Shape(shape) = item else {
            return false;
        };
        shape.fill = shape.fill.and_then(|(c, rule)| paint(c).map(|c| (c, rule)));
        shape.stroke = shape.stroke.take().and_then(|mut s| {
            s.colour = paint(s.colour)?;
            Some(s)
        });
        shape.fill.is_some() || shape.stroke.is_some()
    });
    let image = raster::render(&picture, picture.view, scale, Rgba::BLACK);
    RasterImage {
        x0: 0.0,
        y0: 0.0,
        px_per_mm: scale,
        width: image.width,
        height: image.height,
        bits: image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|px| u8::from(px[0] >= 128))
            .collect(),
    }
}

/// The pixels of `a` with nothing of `near` (the other picture, grown by the tolerance).
fn only(a: &RasterImage, near: &RasterImage) -> Vec<u8> {
    a.bits
        .iter()
        .zip(&near.bits)
        .map(|(&x, &n)| u8::from(x != 0 && n == 0))
        .collect()
}

/// The pictures' pixels and where they are in the frame.
struct Grid {
    width: usize,
    height: usize,
    scale: f64,
    /// The region drawn, in the frame.
    region: [f64; 4],
    /// Turned over left to right about the middle of the region.
    from_below: bool,
}

impl Grid {
    /// The groups of lit pixels of `bits` that touch, corners included, as places.
    fn places(&self, bits: &[u8], only: Only) -> Vec<Place> {
        let (w, h) = (self.width, self.height);
        let mut seen = vec![false; bits.len()];
        let mut out = Vec::new();
        for start in 0..bits.len() {
            if bits[start] == 0 || seen[start] {
                continue;
            }
            seen[start] = true;
            let mut stack = vec![start];
            let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
            let mut pixels = 0;
            while let Some(i) = stack.pop() {
                let (x, y) = (i % w, i / w);
                pixels += 1;
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            continue;
                        }
                        let j = ny as usize * w + nx as usize;
                        if bits[j] != 0 && !seen[j] {
                            seen[j] = true;
                            stack.push(j);
                        }
                    }
                }
            }
            out.push(Place {
                only,
                pixels,
                region: self.region_of(x0, y0, x1, y1),
            });
        }
        out
    }

    /// The box of pixels `x0..=x1`, `y0..=y1` in the frame, seen from above.
    fn region_of(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> [f64; 4] {
        let [rx, ry, rw, _] = self.region;
        let px = 1.0 / self.scale;
        let (left, right) = (x0 as f64 * px, (x1 + 1) as f64 * px);
        // Seen from below, the picture's left edge is the region's right.
        let x = if self.from_below {
            rx + rw - right
        } else {
            rx + left
        };
        let round = |v: f64| (v * 1000.0).round() / 1000.0;
        [
            round(x),
            round(ry + y0 as f64 * px),
            round(right - left),
            round((y1 + 1 - y0) as f64 * px),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture in the manner of `view pcb`: the background, the board, one pad in the top's
    /// colour, a via and a hole in it.
    fn model(pad_x: f64) -> String {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-1 -1 12 7">
<rect x="-1" y="-1" width="12" height="7" fill="#0b0e13"/>
<path d="M0 0H10V5H0Z" fill="#121821" stroke="#8396b1" stroke-width="0.04"/>
<path d="M{pad_x} 1h2v2h-2Z" fill="#d08d50"/>
<circle cx="8" cy="2" r="0.4" fill="#a3adbc"/>
<circle cx="8" cy="2" r="0.15" fill="#0b0e13"/>
</svg>"##
        )
    }

    /// The same in the manner of `view gerber`: copper, a mask opening over it, the outline and
    /// the hole with its ring.
    fn files(pad_x: f64) -> String {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-1 -1 12 7">
<rect x="-1" y="-1" width="12" height="7" fill="#10141a"/>
<path fill="#c83434" d="M{pad_x} 1h2v2h-2Z M8.4 2A0.4 0.4 0 1 1 7.6 2A0.4 0.4 0 1 1 8.4 2Z"/>
<path fill="#d864ff" fill-opacity="0.4" d="M0.9 0.9h2.2v2.2h-2.2Z"/>
<path fill="none" stroke="#d0d2cd" stroke-width="0.1" d="M0 0H10V5H0Z"/>
<circle cx="8" cy="2" r="0.15" fill="#e6e6e6"/>
<circle cx="8" cy="2" r="0.1" fill="#10141a"/>
</svg>"##
        )
    }

    #[test]
    fn the_same_copper_agrees_whatever_else_each_picture_draws() {
        let a = compare(&model(1.0), &files(1.0), false, &Limits::default()).unwrap();
        assert_eq!((a.width, a.height), (480, 280));
        assert!(a.model > 6000 && a.files > 6000, "{a}");
        assert!(a.ok(&Limits::default()), "{a}");
        assert!(a.places.is_empty(), "{a}");
    }

    #[test]
    fn a_pad_a_third_of_a_millimetre_away_is_named_with_its_place() {
        let a = compare(&model(1.0), &files(1.333), false, &Limits::default()).unwrap();
        assert!(!a.ok(&Limits::default()), "{a}");
        let model_only = a.places.iter().find(|p| p.only == Only::Model).unwrap();
        let files_only = a.places.iter().find(|p| p.only == Only::Files).unwrap();
        // The strip the model has and the files do not is on the pad's left, from x = 1.
        let [x, y, w, h] = model_only.region;
        assert!((x - 1.0).abs() < 0.03 && (y - 1.0).abs() < 0.03, "{a}");
        assert!(w > 0.2 && w < 0.33 && (h - 2.0).abs() < 0.03, "{a}");
        assert!(files_only.region[0] > 3.0 - 0.33, "{a}");
        // Seen from below the place is still said in the frame seen from above.
        let below = compare(&model(1.0), &files(1.333), true, &Limits::default()).unwrap();
        let p = below.places.iter().find(|p| p.only == Only::Model).unwrap();
        assert!(
            (p.region[0] + p.region[2] - (10.0 - 1.0)).abs() < 0.03,
            "{below}"
        );
    }

    #[test]
    fn two_pictures_of_other_sizes_are_not_compared() {
        let other = model(1.0).replace("viewBox=\"-1 -1 12 7\"", "viewBox=\"-1 -1 12 8\"");
        let e = compare(&other, &files(1.0), false, &Limits::default()).unwrap_err();
        assert!(e.contains("not on one grid"), "{e}");
    }
}
