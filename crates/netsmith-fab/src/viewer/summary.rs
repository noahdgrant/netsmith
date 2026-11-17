//! The summary of a package: what `netsmith view gerber --format json` prints.

use serde::Serialize;

use super::{Format, Package, Read, file_extent};

/// A box in the viewer's frame (mm from the outline's top left corner, y down).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Extent {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Where the frame's zero is in the files' coordinates (mm, y up), and what gave it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OriginSummary {
    pub x: f64,
    pub y: f64,
    /// `outline`: the top left corner of the outline file's centrelines; `extent`: of
    /// everything drawn, when there is no outline file.
    pub from: &'static str,
}

/// The board's size, the box of the outline's centrelines.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Board {
    pub width: f64,
    pub height: f64,
}

/// One Gerber or drill file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileSummary {
    /// Its path in the package.
    pub file: String,
    /// `gerber` or `excellon`.
    pub format: &'static str,
    /// `copper`, `mask`, `paste`, `silk`, `outline`, `drill` or `other`.
    pub function: &'static str,
    /// The `--layer` that draws it as its own: `top`, `bottom`, `inner<n>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// What said its function: `x2` (its attribute), `name`, `format` (a drill file); none when
    /// nothing did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identified_by: Option<&'static str>,
    /// Its X2 `%TF.FileFunction`, as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_function: Option<String>,
    /// Its X2 `%TF.FilePolarity` (`Negative` for a mask written as its openings), reported and
    /// not applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polarity: Option<String>,
    /// Whether it was read; a file not read is in `unread` with why, and is not drawn.
    pub read: bool,
    /// The box of what it draws (apertures and holes included); none for a file that draws
    /// nothing or was not read.
    pub extent: Option<Extent>,
    /// A Gerber's apertures (`%AD`), flashes, strokes, regions and the objects of these drawn
    /// clear (`%LPC`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apertures: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flashes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strokes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regions: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clear: Option<usize>,
    /// The area a Gerber covers, in mm², what it clears taken away.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<f64>,
    /// A drill file's round holes and slots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slots: Option<usize>,
}

/// The holes of one size, of every drill file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HoleSize {
    /// The tool's diameter in mm (to the micrometre).
    pub diameter: f64,
    pub plated: bool,
    pub holes: usize,
    pub slots: usize,
}

/// A file that could not be read, and the first thing in it the reader did not understand.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unread {
    pub file: String,
    /// The line of the command, from 1; none when the fault is not of a line.
    pub line: Option<usize>,
    pub message: String,
}

/// A file of the package that is neither Gerber nor Excellon.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Skipped {
    pub file: String,
    pub why: String,
}

/// The summary of a package.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub package: String,
    pub origin: OriginSummary,
    /// The outline's size; none with no outline file.
    pub board: Option<Board>,
    /// The box of everything drawn, in the viewer's frame.
    pub extent: Option<Extent>,
    /// The views `--layer` can draw: `top`, `inner1`..., `bottom`, `all`.
    pub views: Vec<String>,
    pub layers: Vec<FileSummary>,
    /// Holes by size, smallest first, plated before not.
    pub holes: Vec<HoleSize>,
    pub unread: Vec<Unread>,
    pub skipped: Vec<Skipped>,
}

fn round(v: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    let r = (v * k).round() / k;
    if r == 0.0 { 0.0 } else { r }
}

fn extent_of(lo: (f64, f64), hi: (f64, f64), pkg: &Package) -> Extent {
    let (a, b) = (pkg.origin.to_frame(lo), pkg.origin.to_frame(hi));
    let (x, y) = (a.0.min(b.0), a.1.min(b.1));
    Extent {
        x: round(x, 4),
        y: round(y, 4),
        width: round((a.0 - b.0).abs(), 4),
        height: round((a.1 - b.1).abs(), 4),
    }
}

pub(super) fn of(pkg: &Package) -> Summary {
    let mut layers = Vec::new();
    let mut unread = Vec::new();
    let mut sizes: Vec<HoleSize> = Vec::new();
    let mut all: Option<((f64, f64), (f64, f64))> = None;
    for f in &pkg.files {
        let extent = file_extent(f);
        if let Some((lo, hi)) = extent {
            all = Some(match all {
                None => (lo, hi),
                Some((a, b)) => (
                    (a.0.min(lo.0), a.1.min(lo.1)),
                    (b.0.max(hi.0), b.1.max(hi.1)),
                ),
            });
        }
        let mut s = FileSummary {
            file: f.name.clone(),
            format: match f.format {
                Format::Gerber => "gerber",
                Format::Excellon => "excellon",
            },
            function: f.function.word(),
            layer: f.function.layer(),
            identified_by: f.identified_by,
            file_function: None,
            polarity: None,
            read: f.read.is_ok(),
            extent: extent.map(|(lo, hi)| extent_of(lo, hi, pkg)),
            apertures: None,
            flashes: None,
            strokes: None,
            regions: None,
            clear: None,
            area: None,
            holes: None,
            slots: None,
        };
        match &f.read {
            Ok(Read::Gerber(g)) => {
                s.file_function = g.facts.file_function.clone();
                s.polarity = g.facts.file_polarity.clone();
                s.apertures = Some(g.facts.apertures);
                s.flashes = Some(g.facts.flashes);
                s.strokes = Some(g.facts.strokes);
                s.regions = Some(g.facts.regions);
                s.clear = Some(g.facts.clear);
                s.area = Some(round(g.areas.iter().map(|a| a.size()).sum(), 3));
            }
            Ok(Read::Drill(holes)) => {
                let slots = holes.iter().filter(|h| h.slot_to.is_some()).count();
                s.holes = Some(holes.len() - slots);
                s.slots = Some(slots);
                for h in holes {
                    let d = round(h.diameter, 3);
                    let slot = usize::from(h.slot_to.is_some());
                    match sizes
                        .iter_mut()
                        .find(|z| z.diameter == d && z.plated == h.plated)
                    {
                        Some(z) => {
                            z.holes += 1 - slot;
                            z.slots += slot;
                        }
                        None => sizes.push(HoleSize {
                            diameter: d,
                            plated: h.plated,
                            holes: 1 - slot,
                            slots: slot,
                        }),
                    }
                }
            }
            Err(e) => unread.push(Unread {
                file: f.name.clone(),
                line: (e.line > 0).then_some(e.line),
                message: e.message.clone(),
            }),
        }
        layers.push(s);
    }
    sizes.sort_by(|a, b| {
        a.diameter
            .total_cmp(&b.diameter)
            .then(b.plated.cmp(&a.plated))
    });
    let o = pkg.origin;
    Summary {
        package: pkg.name.clone(),
        origin: OriginSummary {
            x: round(o.x, 4),
            y: round(o.y, 4),
            from: if o.from_outline { "outline" } else { "extent" },
        },
        board: o.board.map(|(w, h)| Board {
            width: round(w, 4),
            height: round(h, 4),
        }),
        extent: all.map(|(lo, hi)| extent_of(lo, hi, pkg)),
        views: super::draw::views(pkg)
            .iter()
            .map(ToString::to_string)
            .collect(),
        layers,
        holes: sizes,
        unread,
        skipped: pkg
            .skipped
            .iter()
            .map(|(file, why)| Skipped {
                file: file.clone(),
                why: why.clone(),
            })
            .collect(),
    }
}
