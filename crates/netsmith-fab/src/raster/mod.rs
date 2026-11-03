//! A self-contained Gerber (RS-274X, X2) and Excellon reader with a scanline
//! rasteriser and an image comparison, written to cross-check netsmith's own Gerber and
//! drill writers against KiCad's: render both files to bitmaps and compare the bitmaps,
//! which ignores differences in how the same copper is written.
//!
//! - [`render_gerber`] draws a Gerber file as the dark and clear composite it describes;
//!   [`gerber_extent`] finds the box a file draws in, to choose one [`View`] for two files.
//! - [`parse_excellon`] reads a drill file into [`Hole`]s.
//! - [`compare`], [`compare_tolerant`] and [`diff_image`] compare two [`Image`]s drawn
//!   on the same grid; [`Image::to_png`] writes one for a person to look at.
//!
//! Nothing here depends on a crate outside `std`.

mod excellon;
mod geom;
mod gerber;
mod macros;
mod png;

pub use excellon::{Hole, parse_excellon};
pub use gerber::{gerber_extent, render_gerber};

/// Why a file could not be read or two images could not be compared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterError {
    /// The 1-based line of the offending command in the file, or 0 when the error is not
    /// about a line.
    pub line: usize,
    /// What went wrong; for an unsupported command it names the command.
    pub message: String,
}

impl RasterError {
    pub(crate) fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for RasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            f.write_str(&self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for RasterError {}

/// A rectangle in millimetres in Gerber orientation (X right, Y up).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// The lower-left corner.
    pub min: (f64, f64),
    /// The upper-right corner.
    pub max: (f64, f64),
}

impl View {
    /// The smallest view that holds both `self` and `other`.
    pub fn union(&self, other: &View) -> View {
        View {
            min: (self.min.0.min(other.min.0), self.min.1.min(other.min.1)),
            max: (self.max.0.max(other.max.0), self.max.1.max(other.max.1)),
        }
    }

    /// The view grown by `margin` millimetres on every side.
    pub fn grow(&self, margin: f64) -> View {
        View {
            min: (self.min.0 - margin, self.min.1 - margin),
            max: (self.max.0 + margin, self.max.1 + margin),
        }
    }
}

/// The largest image, in pixels, that [`render_gerber`] will allocate.
const MAX_PIXELS: usize = 1 << 28;

/// A black and white bitmap of a region of the board.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    /// X of the lower-left corner, in millimetres.
    pub x0: f64,
    /// Y of the lower-left corner, in millimetres.
    pub y0: f64,
    /// Pixels per millimetre.
    pub px_per_mm: f64,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// One byte per pixel, 0 or 1 (1 is lit), row after row; row 0 is the top (the
    /// largest Y).
    pub bits: Vec<u8>,
}

impl Image {
    /// An unlit image covering `view`.
    pub(crate) fn blank(view: &View, px_per_mm: f64) -> Result<Image, RasterError> {
        let (w, h) = (
            (view.max.0 - view.min.0) * px_per_mm,
            (view.max.1 - view.min.1) * px_per_mm,
        );
        let ok = px_per_mm.is_finite()
            && px_per_mm > 0.0
            && w.is_finite()
            && h.is_finite()
            && w > 0.0
            && h > 0.0
            && view.min.0.is_finite()
            && view.min.1.is_finite();
        if !ok {
            return Err(RasterError::new(
                0,
                "the view must have a positive size and the scale must be positive",
            ));
        }
        if w * h > MAX_PIXELS as f64 {
            return Err(RasterError::new(
                0,
                format!("an image of {w:.0} x {h:.0} pixels is too large"),
            ));
        }
        let (width, height) = (w.ceil() as usize, h.ceil() as usize);
        Ok(Image {
            x0: view.min.0,
            y0: view.min.1,
            px_per_mm,
            width,
            height,
            bits: vec![0; width * height],
        })
    }

    /// The number of lit pixels.
    pub fn lit(&self) -> usize {
        self.bits.iter().filter(|&&b| b != 0).count()
    }

    /// The image as a grayscale PNG (lit is white, unlit black), with uncompressed
    /// deflate blocks, for looking at while debugging.
    pub fn to_png(&self) -> Vec<u8> {
        let pixels: Vec<u8> = self
            .bits
            .iter()
            .map(|&b| if b != 0 { 255 } else { 0 })
            .collect();
        png::gray(self.width, self.height, &pixels)
    }

    /// The image with every lit pixel grown to the square of side `2 * radius_px + 1`
    /// centred on it (a Chebyshev distance of at most `radius_px`).
    pub fn dilate(&self, radius_px: usize) -> Image {
        let (w, h, r) = (self.width, self.height, radius_px);
        if r == 0 || w == 0 || h == 0 {
            return self.clone();
        }
        // Along each row: a pixel is lit if a lit pixel is within r columns.
        let mut across = vec![0u8; w * h];
        let mut prefix = vec![0u32; w + 1];
        for y in 0..h {
            let row = &self.bits[y * w..(y + 1) * w];
            for x in 0..w {
                prefix[x + 1] = prefix[x] + u32::from(row[x] != 0);
            }
            for x in 0..w {
                let (lo, hi) = (x.saturating_sub(r), (x + r + 1).min(w));
                across[y * w + x] = u8::from(prefix[hi] > prefix[lo]);
            }
        }
        // Down each column: a sliding window of 2r+1 rows, counted per column.
        let mut out = vec![0u8; w * h];
        let mut counts = vec![0u32; w];
        for row in across.chunks(w).take(r.min(h - 1) + 1) {
            for (c, &b) in counts.iter_mut().zip(row) {
                *c += u32::from(b);
            }
        }
        for y in 0..h {
            if y > 0 {
                if y + r < h {
                    for (c, &b) in counts.iter_mut().zip(&across[(y + r) * w..(y + r + 1) * w]) {
                        *c += u32::from(b);
                    }
                }
                if y > r {
                    let old = y - r - 1;
                    for (c, &b) in counts.iter_mut().zip(&across[old * w..(old + 1) * w]) {
                        *c -= u32::from(b);
                    }
                }
            }
            for (o, &c) in out[y * w..(y + 1) * w].iter_mut().zip(&counts) {
                *o = u8::from(c > 0);
            }
        }
        Image {
            bits: out,
            ..self.clone()
        }
    }

    fn same_grid(&self, other: &Image) -> bool {
        self.width == other.width
            && self.height == other.height
            && (self.px_per_mm - other.px_per_mm).abs() < 1e-9
            && (self.x0 - other.x0).abs() < 1e-9
            && (self.y0 - other.y0).abs() < 1e-9
    }
}

/// How two images differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diff {
    /// Lit pixels in the first image.
    pub lit_a: usize,
    /// Lit pixels in the second image.
    pub lit_b: usize,
    /// Pixels lit in the first image only (for [`compare_tolerant`], with no lit pixel of
    /// the second within the tolerance).
    pub only_a: usize,
    /// Pixels lit in the second image only (same tolerance rule).
    pub only_b: usize,
}

impl Diff {
    /// `(only_a + only_b) / max(lit_a, lit_b, 1)`: 0 for identical images, about 2 for
    /// disjoint ones of equal size.
    pub fn fraction(&self) -> f64 {
        (self.only_a + self.only_b) as f64 / self.lit_a.max(self.lit_b).max(1) as f64
    }
}

fn check_grid(a: &Image, b: &Image) -> Result<(), RasterError> {
    if a.same_grid(b) {
        Ok(())
    } else {
        Err(RasterError::new(
            0,
            format!(
                "the images are not on the same grid: {}x{} px at ({}, {}) mm and {}x{} px at ({}, {}) mm",
                a.width, a.height, a.x0, a.y0, b.width, b.height, b.x0, b.y0
            ),
        ))
    }
}

/// Counts the pixels lit in `a` and unlit in `mask`.
fn count_outside(a: &Image, mask: &Image) -> usize {
    a.bits
        .iter()
        .zip(&mask.bits)
        .filter(|&(&x, &m)| x != 0 && m == 0)
        .count()
}

/// Compares two images pixel by pixel. They must have the same size, origin and scale.
pub fn compare(a: &Image, b: &Image) -> Result<Diff, RasterError> {
    compare_tolerant(a, b, 0)
}

/// Like [`compare`], but a lit pixel of `a` counts as `only_a` only if no pixel of `b`
/// within `tolerance_px` is lit, and symmetrically for `b`. Distance is Chebyshev (the
/// square of side `2 * tolerance_px + 1` around the pixel; see [`Image::dilate`]). This
/// absorbs the sub-pixel differences between two correct renderings of one shape.
pub fn compare_tolerant(a: &Image, b: &Image, tolerance_px: usize) -> Result<Diff, RasterError> {
    check_grid(a, b)?;
    let (only_a, only_b) = if tolerance_px == 0 {
        (count_outside(a, b), count_outside(b, a))
    } else {
        (
            count_outside(a, &b.dilate(tolerance_px)),
            count_outside(b, &a.dilate(tolerance_px)),
        )
    };
    Ok(Diff {
        lit_a: a.lit(),
        lit_b: b.lit(),
        only_a,
        only_b,
    })
}

/// Gray level of a pixel lit in both images in [`diff_image`].
pub const DIFF_BOTH: u8 = 128;
/// Gray level of a pixel lit in the first image only in [`diff_image`].
pub const DIFF_ONLY_A: u8 = 255;
/// Gray level of a pixel lit in the second image only in [`diff_image`].
pub const DIFF_ONLY_B: u8 = 64;

/// A PNG showing where two images differ: mid gray ([`DIFF_BOTH`], 128) where both are
/// lit, white ([`DIFF_ONLY_A`], 255) where only `a` is, dark gray ([`DIFF_ONLY_B`], 64)
/// where only `b` is, and black where neither is. The PNG has the size of `a`; where `b`
/// does not cover a pixel of `a` (the sizes differ) `b` counts as unlit there.
pub fn diff_image(a: &Image, b: &Image) -> Vec<u8> {
    let mut pixels = vec![0u8; a.width * a.height];
    for y in 0..a.height {
        for x in 0..a.width {
            let la = a.bits[y * a.width + x] != 0;
            let lb = x < b.width && y < b.height && b.bits[y * b.width + x] != 0;
            pixels[y * a.width + x] = match (la, lb) {
                (true, true) => DIFF_BOTH,
                (true, false) => DIFF_ONLY_A,
                (false, true) => DIFF_ONLY_B,
                (false, false) => 0,
            };
        }
    }
    png::gray(a.width, a.height, &pixels)
}

#[cfg(test)]
mod tests {
    use super::png::{adler32, crc32};
    use super::*;

    fn image(w: usize, h: usize, lit: &[(usize, usize)]) -> Image {
        let mut bits = vec![0; w * h];
        for &(x, y) in lit {
            bits[y * w + x] = 1;
        }
        Image {
            x0: 0.0,
            y0: 0.0,
            px_per_mm: 1.0,
            width: w,
            height: h,
            bits,
        }
    }

    /// Checks every chunk's CRC, inflates the stored blocks, checks the Adler-32 and
    /// returns (width, height, pixels).
    fn decode_png(png: &[u8]) -> (usize, usize, Vec<u8>) {
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let mut at = 8;
        let (mut w, mut h) = (0, 0);
        let mut idat = Vec::new();
        let mut kinds = Vec::new();
        while at < png.len() {
            let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
            let kind = &png[at + 4..at + 8];
            let data = &png[at + 8..at + 8 + len];
            let crc = u32::from_be_bytes(png[at + 8 + len..at + 12 + len].try_into().unwrap());
            assert_eq!(crc, crc32(&png[at + 4..at + 8 + len]), "CRC of {kind:?}");
            kinds.push(kind.to_vec());
            match kind {
                b"IHDR" => {
                    w = u32::from_be_bytes(data[..4].try_into().unwrap()) as usize;
                    h = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
                    assert_eq!(&data[8..], &[8, 0, 0, 0, 0]);
                }
                b"IDAT" => idat.extend_from_slice(data),
                _ => {}
            }
            at += 12 + len;
        }
        assert_eq!(at, png.len());
        assert_eq!(
            kinds,
            vec![b"IHDR".to_vec(), b"IDAT".to_vec(), b"IEND".to_vec()]
        );
        assert_eq!(idat[0] & 0x0F, 8, "deflate");
        assert_eq!(
            (u32::from(idat[0]) * 256 + u32::from(idat[1])) % 31,
            0,
            "zlib header check"
        );
        let mut raw = Vec::new();
        let mut p = 2;
        loop {
            let last = idat[p] & 1 == 1;
            assert_eq!(idat[p] & 6, 0, "stored block");
            let len = usize::from(u16::from_le_bytes([idat[p + 1], idat[p + 2]]));
            let nlen = u16::from_le_bytes([idat[p + 3], idat[p + 4]]);
            assert_eq!(nlen, !(len as u16));
            raw.extend_from_slice(&idat[p + 5..p + 5 + len]);
            p += 5 + len;
            if last {
                break;
            }
        }
        let adler = u32::from_be_bytes(idat[p..p + 4].try_into().unwrap());
        assert_eq!(adler, adler32(&raw));
        assert_eq!(p + 4, idat.len());
        assert_eq!(raw.len(), (w + 1) * h);
        let mut pixels = Vec::new();
        for row in raw.chunks(w + 1) {
            assert_eq!(row[0], 0);
            pixels.extend_from_slice(&row[1..]);
        }
        (w, h, pixels)
    }

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(&[]), 1);
    }

    #[test]
    fn png_round_trips() {
        let img = image(5, 3, &[(0, 0), (4, 2), (2, 1)]);
        let (w, h, px) = decode_png(&img.to_png());
        assert_eq!((w, h), (5, 3));
        let want: Vec<u8> = img.bits.iter().map(|&b| b * 255).collect();
        assert_eq!(px, want);
    }

    #[test]
    fn png_larger_than_one_stored_block() {
        // 300 x 300 pixels is 90 300 bytes of scanlines: two stored blocks.
        let lit: Vec<(usize, usize)> = (0..300).map(|i| (i, (i * 7) % 300)).collect();
        let img = image(300, 300, &lit);
        let (w, h, px) = decode_png(&img.to_png());
        assert_eq!((w, h), (300, 300));
        assert_eq!(px.iter().filter(|&&p| p == 255).count(), 300);
        assert_eq!(px[7 * 300 + 1], 255);
    }

    #[test]
    fn dilate_grows_by_a_square() {
        let img = image(9, 9, &[(4, 4)]);
        assert_eq!(img.dilate(0), img);
        let d = img.dilate(2);
        assert_eq!(d.lit(), 25);
        assert!(d.bits[2 * 9 + 2] == 1 && d.bits[6 * 9 + 6] == 1 && d.bits[2 * 9 + 1] == 0);
        // Near the border and with a radius larger than the image.
        let corner = image(4, 4, &[(0, 0)]).dilate(1);
        assert_eq!(corner.lit(), 4);
        assert_eq!(image(4, 4, &[(1, 2)]).dilate(10).lit(), 16);
        assert_eq!(image(4, 4, &[]).dilate(3).lit(), 0);
    }

    #[test]
    fn compare_counts_differences() {
        let a = image(4, 4, &[(0, 0), (1, 1), (2, 2)]);
        let b = image(4, 4, &[(1, 1), (2, 2), (3, 3), (3, 2)]);
        let d = compare(&a, &b).unwrap();
        assert_eq!(
            d,
            Diff {
                lit_a: 3,
                lit_b: 4,
                only_a: 1,
                only_b: 2
            }
        );
        assert_eq!(d.fraction(), 0.75);
        assert_eq!(compare(&a, &a).unwrap().fraction(), 0.0);
        let empty = image(4, 4, &[]);
        assert_eq!(compare(&empty, &empty).unwrap().fraction(), 0.0);
        assert_eq!(compare(&a, &empty).unwrap().fraction(), 1.0);
    }

    #[test]
    fn compare_rejects_different_grids() {
        let a = image(4, 4, &[]);
        assert!(compare(&a, &image(4, 5, &[])).is_err());
        let mut shifted = image(4, 4, &[]);
        shifted.x0 = 1.0;
        let e = compare(&a, &shifted).unwrap_err();
        assert!(e.to_string().contains("same grid"));
        let mut scaled = image(4, 4, &[]);
        scaled.px_per_mm = 2.0;
        assert!(compare_tolerant(&a, &scaled, 1).is_err());
    }

    #[test]
    fn tolerant_compare_absorbs_a_one_pixel_shift() {
        let a = image(8, 8, &[(2, 2), (3, 2), (4, 2)]);
        let b = image(8, 8, &[(3, 3), (4, 3), (5, 3)]);
        assert_eq!(compare(&a, &b).unwrap().only_a, 3);
        let t = compare_tolerant(&a, &b, 1).unwrap();
        assert_eq!((t.only_a, t.only_b, t.lit_a, t.lit_b), (0, 0, 3, 3));
        let far = image(8, 8, &[(7, 7)]);
        let t = compare_tolerant(&a, &far, 2).unwrap();
        assert_eq!((t.only_a, t.only_b), (3, 1));
        assert_eq!(
            compare_tolerant(&a, &b, 0).unwrap(),
            compare(&a, &b).unwrap()
        );
    }

    #[test]
    fn diff_image_levels() {
        let a = image(2, 2, &[(0, 0), (1, 0)]);
        let b = image(2, 2, &[(1, 0), (1, 1)]);
        let (w, h, px) = decode_png(&diff_image(&a, &b));
        assert_eq!((w, h), (2, 2));
        assert_eq!(px, vec![DIFF_ONLY_A, DIFF_BOTH, 0, DIFF_ONLY_B]);
        const { assert!(DIFF_ONLY_A != DIFF_BOTH && DIFF_BOTH != DIFF_ONLY_B && DIFF_ONLY_B != 0) };
        // A smaller b counts as unlit where it has no pixel.
        let small = image(1, 1, &[(0, 0)]);
        let (_, _, px) = decode_png(&diff_image(&a, &small));
        assert_eq!(px, vec![DIFF_BOTH, DIFF_ONLY_A, 0, 0]);
    }

    #[test]
    fn views_combine() {
        let a = View {
            min: (0.0, 1.0),
            max: (2.0, 3.0),
        };
        let b = View {
            min: (-1.0, 2.0),
            max: (1.0, 5.0),
        };
        assert_eq!(
            a.union(&b),
            View {
                min: (-1.0, 1.0),
                max: (2.0, 5.0)
            }
        );
        assert_eq!(
            a.grow(0.5),
            View {
                min: (-0.5, 0.5),
                max: (2.5, 3.5)
            }
        );
    }

    #[test]
    fn two_spellings_of_one_shape_compare_equal() {
        // A rectangular pad flashed, and the same pad as a region.
        let head = "%FSLAX24Y24*%\n%MOMM*%\n";
        let flash = format!("{head}%ADD10R,3.0X2.0*%\nD10*\nX50000Y50000D03*\nM02*\n");
        let region = format!(
            "{head}G36*\nX35000Y40000D02*\nG01*\nX65000Y40000D01*\nX65000Y60000D01*\nX35000Y60000D01*\nX35000Y40000D01*\nG37*\nM02*\n"
        );
        let view = View {
            min: (0.0, 0.0),
            max: (10.0, 10.0),
        };
        let a = render_gerber(&flash, &view, 20.0).unwrap();
        let b = render_gerber(&region, &view, 20.0).unwrap();
        assert_eq!(compare(&a, &b).unwrap().fraction(), 0.0);
        // Or as a stroke of a thinner aperture: nearly the same, exactly under tolerance.
        let stroke =
            format!("{head}%ADD10R,3.0X0.5*%\nD10*\nX50000Y42500D02*\nX50000Y57500D01*\nM02*\n");
        let c = render_gerber(&stroke, &view, 20.0).unwrap();
        assert_eq!(compare(&a, &c).unwrap().fraction(), 0.0);
    }

    #[test]
    fn kicad_reference_files_load_and_draw() {
        let root = std::path::Path::new("/tmp/ref-gerber");
        if !root.is_dir() {
            return;
        }
        let mut files = 0;
        let mut dirs: Vec<_> = std::fs::read_dir(root)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        dirs.sort();
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(dir.join("gerber")) else {
                continue;
            };
            let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            paths.sort();
            for path in paths {
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                let text = std::fs::read_to_string(&path).unwrap();
                if name.ends_with(".drl") {
                    let holes = parse_excellon(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
                    assert!(holes.iter().all(|h| h.diameter > 0.0), "{name}");
                    files += 1;
                } else if name.ends_with(".gbr") {
                    let view = match gerber_extent(&text) {
                        Ok(v) => v.grow(0.5),
                        Err(_) => View {
                            min: (0.0, 0.0),
                            max: (10.0, 10.0),
                        },
                    };
                    let t = std::time::Instant::now();
                    let img =
                        render_gerber(&text, &view, 20.0).unwrap_or_else(|e| panic!("{name}: {e}"));
                    eprintln!(
                        "{name}: {}x{} px, {} lit, {:?}",
                        img.width,
                        img.height,
                        img.lit(),
                        t.elapsed()
                    );
                    let must_draw = ["Cu", "Mask", "Paste", "Edge"]
                        .iter()
                        .any(|k| name.contains(k));
                    if must_draw {
                        assert!(img.lit() > 0, "{name} draws nothing");
                    }
                    files += 1;
                }
            }
        }
        assert!(files > 0);
    }
}
