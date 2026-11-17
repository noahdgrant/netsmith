//! The drawing of a package: an SVG in the viewer's frame (mm from the outline's top left
//! corner, y down), each file a group in the colour of its function, and the PNG of that SVG.
//!
//! What each [`Show`] draws, first to last (later over earlier):
//!
//! - `top`: the top copper, the top mask's openings, the top paste, the top silkscreen, the
//!   outline and the holes; as a person sees the top.
//! - `bottom`: the same of the bottom, mirrored left to right, as a person sees the bottom when
//!   the board is turned over about its vertical axis (the editor's Side switch).
//! - `inner<n>`: that inner copper layer, the outline and the holes, seen from the top.
//! - `all`: every copper layer from the bottom up, the bottom's mask, paste and silkscreen,
//!   the top's, the files of no known function, the outline and the holes; seen from the top.
//!
//! The copper is opaque in a side's view and partly see-through with every layer; the mask's
//! openings, the paste and a file of no known function are see-through, so what is under them
//! shows. A hole is drawn as the hole it is: the background, with a thin ring, light grey for a
//! plated hole and yellow for one that is not. Every drill file is drawn in every view.
//!
//! The colours are KiCad's for its layers, so that a person who knows its board editor reads
//! them at once.

use std::fmt::Write as _;

use netsmith_draw::{Rect, Rgba, raster, svg};

use super::{Face, File, Function, Layer, Package, Read};

/// What to draw: a side as a person sees it, an inner copper layer, or every layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    Top,
    Bottom,
    /// The inner copper layer counted from the top, from 1.
    Inner(u32),
    All,
}

impl std::str::FromStr for Show {
    type Err = String;

    fn from_str(s: &str) -> Result<Show, String> {
        match s {
            "top" => Ok(Show::Top),
            "bottom" => Ok(Show::Bottom),
            "all" => Ok(Show::All),
            _ => s
                .strip_prefix("inner")
                .and_then(|n| n.parse::<u32>().ok())
                .filter(|n| *n > 0)
                .map(Show::Inner)
                .ok_or_else(|| format!("`{s}` is not top, bottom, inner<n> or all")),
        }
    }
}

impl std::fmt::Display for Show {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Show::Top => f.write_str("top"),
            Show::Bottom => f.write_str("bottom"),
            Show::Inner(n) => write!(f, "inner{n}"),
            Show::All => f.write_str("all"),
        }
    }
}

/// The SVG of a view and what it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawing {
    pub svg: String,
    /// The region shown, in the viewer's frame: x, y, width, height in mm.
    pub region: [f64; 4],
    /// The files drawn, in the order drawn.
    pub drawn: Vec<String>,
    /// The files the view would draw that could not be read: they are not in the drawing.
    pub unread: Vec<String>,
}

/// Why a view could not be drawn.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewError {
    /// The package has no file of the side or layer asked for; `has` is the views it has.
    NoLayer { asked: Show, has: Vec<Show> },
    /// No file of the view could be read, so nothing is drawn.
    Unread { asked: Show, files: Vec<String> },
    /// A region with no width or height.
    Region,
    /// The picture would have a side longer than the rasterizer draws.
    TooLarge { width: f64, height: f64 },
    /// The SVG could not be read back (a fault of this module).
    Svg(String),
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewError::NoLayer { asked, has } => {
                let has: Vec<String> = has.iter().map(Show::to_string).collect();
                write!(
                    f,
                    "the package has no file for `{asked}`; it has: {}",
                    has.join(", ")
                )
            }
            ViewError::Unread { asked, files } => write!(
                f,
                "no file of `{asked}` could be read: {}",
                files.join(", ")
            ),
            ViewError::Region => f.write_str("the region needs a width and a height above 0"),
            ViewError::TooLarge { width, height } => write!(
                f,
                "a picture of {width:.0} x {height:.0} pixels is larger than {} pixels a side; \
                 lower --scale or draw a --region",
                raster::MAX_SIDE
            ),
            ViewError::Svg(why) => write!(f, "the drawing could not be read back: {why}"),
        }
    }
}

impl std::error::Error for ViewError {}

/// The background, the colour a hole is.
const BACKGROUND: &str = "#10141a";
const BACKGROUND_RGBA: Rgba = Rgba::opaque(0x10, 0x14, 0x1a);

/// The pixels a millimetre of a PNG when no scale is given, and its longest side then.
pub const PNG_PER_MM: f64 = 20.0;
pub const PNG_LONGEST: usize = 8192;

/// The width of a hole's ring, in mm (less for a small hole).
const RING: f64 = 0.05;

/// A file's colour and opacity in a view.
fn paint(function: Function, show: Show) -> (&'static str, f64) {
    let copper = if show == Show::All { 0.7 } else { 1.0 };
    match function {
        Function::Copper(Layer::Top) => ("#c83434", copper),
        Function::Copper(Layer::Bottom) => ("#4d7fc4", copper),
        Function::Copper(Layer::Inner(n)) => {
            const INNER: [&str; 4] = ["#c2a000", "#b04ad0", "#2fb8b8", "#d07020"];
            (INNER[(n as usize - 1) % INNER.len()], copper)
        }
        Function::Mask(Face::Top) => ("#d864ff", 0.4),
        Function::Mask(Face::Bottom) => ("#02ffee", 0.4),
        Function::Paste(Face::Top) => ("#b4a0a0", 0.6),
        Function::Paste(Face::Bottom) => ("#00c2c2", 0.6),
        Function::Silk(Face::Top) => ("#f2eda1", 0.95),
        Function::Silk(Face::Bottom) => ("#e8b2a7", 0.95),
        Function::Outline => ("#d0d2cd", 1.0),
        Function::Drill => ("#e6e6e6", 1.0),
        Function::Other => ("#8fb3ff", 0.6),
    }
}

/// Where a file comes in the order of a view, or `None` when the view does not draw it.
fn rank(function: Function, show: Show) -> Option<u32> {
    let side = |face: Face| {
        matches!(
            (show, face),
            (Show::Top, Face::Top) | (Show::Bottom, Face::Bottom) | (Show::All, _)
        )
    };
    let face_rank = |face: Face, k: u32| -> u32 {
        // With every layer, the bottom's before the top's.
        let base = if show == Show::All && face == Face::Top {
            2000
        } else {
            1000
        };
        base + k
    };
    Some(match function {
        Function::Copper(layer) => match (show, layer) {
            (Show::Top, Layer::Top) | (Show::Bottom, Layer::Bottom) => 0,
            (Show::Inner(n), Layer::Inner(m)) if n == m => 0,
            (Show::All, Layer::Bottom) => 0,
            // Seen from the top: the deeper inner layers first.
            (Show::All, Layer::Inner(m)) => 1 + (999 - m.min(998)),
            (Show::All, Layer::Top) => 999,
            _ => return None,
        },
        Function::Mask(f) if side(f) => face_rank(f, 1),
        Function::Paste(f) if side(f) => face_rank(f, 2),
        Function::Silk(f) if side(f) => face_rank(f, 3),
        Function::Other if show == Show::All => 3000,
        Function::Outline => 4000,
        Function::Drill => 5000,
        _ => return None,
    })
}

/// The views a package has a file for.
pub(super) fn views(pkg: &Package) -> Vec<Show> {
    let mut out: Vec<Show> = Vec::new();
    for f in &pkg.files {
        let v = match f.function {
            Function::Copper(Layer::Top) => Show::Top,
            Function::Copper(Layer::Bottom) => Show::Bottom,
            Function::Copper(Layer::Inner(n)) => Show::Inner(n),
            Function::Mask(Face::Top) | Function::Paste(Face::Top) | Function::Silk(Face::Top) => {
                Show::Top
            }
            Function::Mask(Face::Bottom)
            | Function::Paste(Face::Bottom)
            | Function::Silk(Face::Bottom) => Show::Bottom,
            _ => continue,
        };
        if !out.contains(&v) {
            out.push(v);
        }
    }
    let key = |s: &Show| match s {
        Show::Top => 0,
        Show::Inner(n) => *n,
        Show::Bottom => u32::MAX - 1,
        Show::All => u32::MAX,
    };
    out.sort_by_key(key);
    out.push(Show::All);
    out
}

/// A number for the SVG: four decimals (0.1 µm), no trailing zeros.
fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The SVG of `show` over `region` (x, y, width, height in mm in the viewer's frame), or over
/// what the view draws, with a millimetre round it, when there is no region.
pub fn draw(pkg: &Package, show: Show, region: Option<[f64; 4]>) -> Result<Drawing, ViewError> {
    let mut chosen: Vec<(u32, usize, &File)> = pkg
        .files
        .iter()
        .enumerate()
        .filter_map(|(i, f)| rank(f.function, show).map(|r| (r, i, f)))
        .collect();
    chosen.sort_by_key(|(r, i, _)| (*r, *i));
    // A side or a layer needs a file of its own, not only the outline and the holes.
    let own = chosen
        .iter()
        .any(|(_, _, f)| !matches!(f.function, Function::Outline | Function::Drill));
    if !own {
        return Err(ViewError::NoLayer {
            asked: show,
            has: views(pkg),
        });
    }
    let unread: Vec<String> = chosen
        .iter()
        .filter(|(_, _, f)| f.read.is_err())
        .map(|(_, _, f)| f.name.clone())
        .collect();
    let files: Vec<&File> = chosen
        .iter()
        .filter(|(_, _, f)| f.read.is_ok())
        .map(|(_, _, f)| *f)
        .collect();
    let o = pkg.origin;
    let region = match region {
        Some(r) if r[2] > 0.0 && r[3] > 0.0 && r.iter().all(|v| v.is_finite()) => r,
        Some(_) => return Err(ViewError::Region),
        None => {
            let extent = files
                .iter()
                .filter_map(|f| super::file_extent(f))
                .map(|(lo, hi)| {
                    let (a, b) = (o.to_frame(lo), o.to_frame(hi));
                    ((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1)))
                })
                .reduce(|a, b| {
                    (
                        (a.0.0.min(b.0.0), a.0.1.min(b.0.1)),
                        (a.1.0.max(b.1.0), a.1.1.max(b.1.1)),
                    )
                });
            let Some((lo, hi)) = extent else {
                return Err(ViewError::Unread {
                    asked: show,
                    files: unread,
                });
            };
            [lo.0 - 1.0, lo.1 - 1.0, hi.0 - lo.0 + 2.0, hi.1 - lo.1 + 2.0]
        }
    };
    let [x0, y0, w, h] = region;
    let mirror = show == Show::Bottom;
    // A point of the files to the SVG: to the frame, then turned over for the bottom.
    let at = |p: (f64, f64)| -> (f64, f64) {
        let q = o.to_frame(p);
        if mirror { (2.0 * x0 + w - q.0, q.1) } else { q }
    };
    let mut s = String::new();
    let _ = writeln!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}mm\" height=\"{}mm\" data-show=\"{show}\">",
        num(x0),
        num(y0),
        num(w),
        num(h),
        num(w),
        num(h)
    );
    let _ = writeln!(
        s,
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{BACKGROUND}\"/>",
        num(x0),
        num(y0),
        num(w),
        num(h)
    );
    let mut drawn = Vec::new();
    for f in files {
        let (colour, opacity) = paint(f.function, show);
        let layer = f
            .function
            .layer()
            .map(|l| format!(" data-layer=\"{l}\""))
            .unwrap_or_default();
        let _ = writeln!(
            s,
            "<g class=\"layer\" data-file=\"{}\" data-function=\"{}\"{layer}>",
            escape(&f.name),
            f.function.word()
        );
        match &f.read {
            Ok(Read::Gerber(g)) => {
                let mut d = String::new();
                for a in &g.areas {
                    for ring in std::iter::once(&a.outer).chain(&a.holes) {
                        for (i, p) in ring.iter().enumerate() {
                            let (x, y) = at(*p);
                            d.push(if i == 0 { 'M' } else { ' ' });
                            d.push_str(&num(x));
                            d.push(' ');
                            d.push_str(&num(y));
                        }
                        d.push('Z');
                    }
                }
                if !d.is_empty() {
                    let _ = writeln!(
                        s,
                        "<path fill=\"{colour}\" fill-opacity=\"{opacity}\" fill-rule=\"evenodd\" d=\"{d}\"/>"
                    );
                }
            }
            Ok(Read::Drill(holes)) => {
                for hole in holes {
                    let ring = if hole.plated { "#e6e6e6" } else { "#f0c040" };
                    let r = hole.diameter / 2.0;
                    let inner = r - RING.min(hole.diameter / 4.0);
                    let (ax, ay) = at((hole.x, hole.y));
                    match hole.slot_to {
                        None => {
                            for (fill, rr) in [(ring, r), (BACKGROUND, inner)] {
                                let _ = writeln!(
                                    s,
                                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{fill}\"/>",
                                    num(ax),
                                    num(ay),
                                    num(rr)
                                );
                            }
                        }
                        Some(to) => {
                            let (bx, by) = at(to);
                            for (stroke, width) in [(ring, 2.0 * r), (BACKGROUND, 2.0 * inner)] {
                                let _ = writeln!(
                                    s,
                                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{stroke}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
                                    num(ax),
                                    num(ay),
                                    num(bx),
                                    num(by),
                                    num(width)
                                );
                            }
                        }
                    }
                }
            }
            Err(_) => {}
        }
        s.push_str("</g>\n");
        drawn.push(f.name.clone());
    }
    s.push_str("</svg>\n");
    Ok(Drawing {
        svg: s,
        region,
        drawn,
        unread,
    })
}

/// The PNG of a drawing at `scale` pixels a millimetre, or at [`PNG_PER_MM`] (fewer for a
/// picture longer than [`PNG_LONGEST`] pixels) when none is given. The same drawing and scale
/// give the same bytes.
pub fn png(drawing: &Drawing, scale: Option<f64>) -> Result<Vec<u8>, ViewError> {
    let picture = svg::read(&drawing.svg).map_err(|e| ViewError::Svg(e.to_string()))?;
    let [x, y, w, h] = drawing.region;
    let view = Rect::new(x, y, w, h);
    let scale = match scale {
        Some(s) => {
            let (pw, ph) = (w * s, h * s);
            if pw.ceil() > raster::MAX_SIDE as f64 || ph.ceil() > raster::MAX_SIDE as f64 {
                return Err(ViewError::TooLarge {
                    width: pw,
                    height: ph,
                });
            }
            s
        }
        None => raster::scale_for(view, PNG_PER_MM, PNG_LONGEST),
    };
    Ok(raster::png(&raster::render(
        &picture,
        view,
        scale,
        BACKGROUND_RGBA,
    )))
}
