//! The Gerber viewer (issue #1105): reads the files of a fab package, the zip or the folder,
//! netsmith's or another tool's, with netsmith's own reader (no KiCad, ADR 0097), and draws
//! them as an SVG or a PNG, or sums them up. `netsmith view gerber` is its command line; the
//! editor's Gerber view (issue #1107) makes the same calls through its backend
//! (`netsmith-workspace`'s `gerber.rs`), so the two draw the same SVG.
//!
//! The interface:
//!
//! - [`open`] reads a package from a path (a `.zip`, or a folder and the folders in it; a
//!   folder with no Gerber or drill file of its own has its zips read, so `fab`'s output folder
//!   opens as its `<board>-gerbers.zip`), [`read_bytes`] from the bytes of a zip or a file, and
//!   [`read`] from files already in memory. Every
//!   file is sniffed: RS-274X is read by [`crate::raster`]'s Gerber reader, Excellon by its
//!   drill reader, and anything else is listed in [`Package::skipped`].
//! - A [`File`] has its [`Function`] (copper of a layer, mask, paste or silkscreen of a face,
//!   the outline, the drill, or other), taken from its X2 `%TF.FileFunction` where it has one
//!   and from its name otherwise (KiCad's `-F_Cu`, Protel's `.GTL`; [`identify`]), and what was
//!   read: its shapes, or why it could not be. A file the reader does not fully understand is
//!   not drawn at all: its error names the command and the line ([`crate::RasterError`]).
//! - [`Package::summary`] is the JSON summary: layers found, extents, apertures, objects,
//!   holes by size and the files not read ([`Summary`]).
//! - [`draw`] makes the SVG of a [`Show`] (a side as a person sees it, an inner layer, or all
//!   of them) and a region, and [`png`] fills that SVG into pixels with `netsmith-draw`, so the
//!   SVG and the PNG are one drawing.
//!
//! **The frame.** Gerber and drill files are in mm (or inches, converted) with y up, from an
//! origin of the writer's choosing. The viewer's frame is the one the layout and routes files
//! use: millimetres from the top left corner of the outline, y down. The corner is the box of
//! the outline file's centrelines (its strokes' paths, not their outer edges); with no outline
//! file it is the corner of everything drawn. Extents and regions are in this frame, and the
//! [`Origin`] says where its zero is in the files' coordinates.
//!
//! **Polarity.** The objects of a file are combined in order as its polarity says (`%LPD` adds,
//! `%LPC` takes away), exactly, with polygon operations ([`shapes`]), so a layer drawn over
//! another does not hide it where it was cleared. A file's `%TF.FilePolarity` is reported and
//! not applied: a mask file draws its openings whichever it says.
//!
//! Curves are cut into straight pieces no more than 2.5 µm from the true curve (the reader's
//! tessellation at [`READ_PX_PER_MM`]).

mod draw;
mod identify;
mod shapes;
mod summary;

use std::io::Read as _;
use std::path::Path;

pub use crate::raster::Facts;
pub use draw::{Drawing, PNG_LONGEST, PNG_PER_MM, Show, ViewError, draw, png};
pub use identify::{Face, Function, Layer, identify};
pub use shapes::Area;
pub use summary::{Board, Extent, FileSummary, HoleSize, OriginSummary, Skipped, Summary, Unread};

use crate::raster::{self, Hole, RasterError};

/// The fineness the curves are read at: a curve is cut so that no point of it is more than a
/// quarter of a pixel at this scale from the true curve (2.5 µm).
pub const READ_PX_PER_MM: f64 = 100.0;

/// The largest file read; a larger one is skipped.
const MAX_FILE: u64 = 64 << 20;

/// A package as read.
#[derive(Debug, Clone)]
pub struct Package {
    /// The zip's or the folder's name.
    pub name: String,
    /// The Gerber and drill files, by name.
    pub files: Vec<File>,
    /// The files that are neither Gerber nor Excellon (a bill of materials, a readme), with why.
    pub skipped: Vec<(String, String)>,
    /// Where the frame's zero is in the files' coordinates.
    pub origin: Origin,
}

/// Where the viewer's frame starts, in the files' own coordinates (mm, y up).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Origin {
    pub x: f64,
    pub y: f64,
    /// The outline's centreline box gave it (true), or everything drawn (false).
    pub from_outline: bool,
    /// The outline's centreline box, width and height, when there is an outline.
    pub board: Option<(f64, f64)>,
}

impl Origin {
    /// A point of the files (mm, y up) in the viewer's frame (mm from the corner, y down).
    pub fn to_frame(&self, p: (f64, f64)) -> (f64, f64) {
        (p.0 - self.x, self.y - p.1)
    }
}

/// One Gerber or drill file.
#[derive(Debug, Clone)]
pub struct File {
    /// Its path inside the package, `/`-separated (`gerber/led-F_Cu.gbr`).
    pub name: String,
    pub format: Format,
    pub function: Function,
    /// How the function was found: `x2` (the file's attribute) or `name`; `None` when it was
    /// not (the function is [`Function::Other`]).
    pub identified_by: Option<&'static str>,
    pub read: Result<Read, RasterError>,
}

/// What a file holds.
#[derive(Debug, Clone)]
pub enum Read {
    Gerber(Gerber),
    Drill(Vec<Hole>),
}

/// A Gerber file read: its shapes after the polarities are combined, in the files' frame.
#[derive(Debug, Clone)]
pub struct Gerber {
    pub areas: Vec<Area>,
    pub facts: Facts,
}

/// The format a file was read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Gerber,
    Excellon,
}

/// Why a package could not be opened at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenError(pub String);

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OpenError {}

/// Reads the package at `path`: a zip, or a folder (see the module's header).
pub fn open(path: &Path) -> Result<Package, OpenError> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let err = |e: &dyn std::fmt::Display| OpenError(format!("{}: {e}", path.display()));
    let meta = std::fs::metadata(path).map_err(|e| err(&e))?;
    let files = if meta.is_dir() {
        let mut found = Vec::new();
        walk(path, "", &mut found).map_err(|e| err(&e))?;
        found.sort_by(|a, b| a.0.cmp(&b.0));
        let own = found.iter().any(|(_, bytes)| sniff(bytes).is_some());
        if own {
            found
        } else {
            // `fab`'s folder: the package is in its zips.
            let mut inner = Vec::new();
            for (n, bytes) in &found {
                if n.to_ascii_lowercase().ends_with(".zip") {
                    for (m, b) in unzip(bytes).map_err(|e| err(&format!("{n}: {e}")))? {
                        inner.push((format!("{n}/{m}"), b));
                    }
                }
            }
            if inner.is_empty() { found } else { inner }
        }
    } else {
        let bytes = std::fs::read(path).map_err(|e| err(&e))?;
        return read_bytes(&name, bytes).map_err(|e| err(&e));
    };
    Ok(read(&name, files))
}

/// Reads a package from the bytes of a `.zip`, or of one Gerber or drill file (a package of one),
/// named `name`: what [`open`] does with a file, for bytes that are not on disk (the editor's
/// Gerber view, issue #1107).
pub fn read_bytes(name: &str, bytes: Vec<u8>) -> Result<Package, OpenError> {
    let files = if bytes.starts_with(b"PK") {
        unzip(&bytes).map_err(OpenError)?
    } else {
        vec![(name.to_string(), bytes)]
    };
    Ok(read(name, files))
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = format!("{prefix}{}", e.file_name().to_string_lossy());
        let kind = e.file_type()?;
        if kind.is_dir() {
            walk(&e.path(), &format!("{name}/"), out)?;
        } else if e.metadata()?.len() <= MAX_FILE {
            out.push((name, std::fs::read(e.path())?));
        }
    }
    Ok(())
}

/// The files of a zip, by name, its folders and macOS's `__MACOSX` copies left out.
fn unzip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if entry.is_dir() || name.starts_with("__MACOSX/") || entry.size() > MAX_FILE {
            continue;
        }
        let mut b = Vec::new();
        entry.read_to_end(&mut b).map_err(|e| e.to_string())?;
        out.push((name, b));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// What a file is: Excellon when its first line that is not a comment is `M48`, Gerber when
/// its start has the format statement (`%FS`, which every Gerber gives before its first
/// coordinate); otherwise neither. A file whose start is not plain text (a PDF, a 3D model) is
/// neither, whatever bytes it holds.
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    let head = &bytes[..bytes.len().min(8192)];
    let text = head
        .iter()
        .all(|b| b.is_ascii_graphic() || b.is_ascii_whitespace() || *b >= 0x80);
    if !text || head.is_empty() {
        return None;
    }
    let head = String::from_utf8_lossy(head);
    let first = head
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with(';'));
    if first.is_some_and(|l| l.starts_with("M48")) {
        return Some(Format::Excellon);
    }
    head.contains("%FS").then_some(Format::Gerber)
}

/// Reads a package from its files (name, bytes), in the order given.
pub fn read(name: &str, files: Vec<(String, Vec<u8>)>) -> Package {
    let mut read_files = Vec::new();
    let mut skipped = Vec::new();
    for (file, bytes) in files {
        let Some(format) = sniff(&bytes) else {
            skipped.push((file, "not a Gerber or Excellon file".to_string()));
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let (read, x2) = match format {
            Format::Gerber => {
                let mut items = Vec::new();
                match raster::read_gerber(&text, READ_PX_PER_MM, &mut |item| {
                    items.push(item.clone())
                }) {
                    Ok(facts) => {
                        let x2 = facts.file_function.clone();
                        let areas = shapes::combine(&items);
                        (Ok(Read::Gerber(Gerber { areas, facts })), x2)
                    }
                    Err(e) => (Err(e), None),
                }
            }
            Format::Excellon => (raster::parse_excellon(&text).map(Read::Drill), None),
        };
        let (function, by) = match format {
            Format::Excellon => (Function::Drill, Some("format")),
            Format::Gerber => identify(x2.as_deref(), &file),
        };
        read_files.push(File {
            name: file,
            format,
            function,
            identified_by: by,
            read,
        });
    }
    let origin = origin_of(&read_files);
    Package {
        name: name.to_string(),
        files: read_files,
        skipped,
        origin,
    }
}

/// The top left corner of the outline's centrelines, or of everything drawn.
fn origin_of(files: &[File]) -> Origin {
    let outline = files
        .iter()
        .filter(|f| f.function == Function::Outline)
        .find_map(|f| match &f.read {
            Ok(Read::Gerber(g)) => g.facts.centre,
            _ => None,
        });
    if let Some((lo, hi)) = outline {
        return Origin {
            x: lo.0,
            y: hi.1,
            from_outline: true,
            board: Some((hi.0 - lo.0, hi.1 - lo.1)),
        };
    }
    let all = files.iter().filter_map(file_extent).reduce(|a, b| {
        (
            (a.0.0.min(b.0.0), a.0.1.min(b.0.1)),
            (a.1.0.max(b.1.0), a.1.1.max(b.1.1)),
        )
    });
    let (x, y) = all.map_or((0.0, 0.0), |(lo, hi)| (lo.0, hi.1));
    Origin {
        x,
        y,
        from_outline: false,
        board: None,
    }
}

/// The box of what a file draws, in the files' frame (lower left, upper right); `None` for a
/// file that draws nothing or was not read.
pub fn file_extent(file: &File) -> Option<((f64, f64), (f64, f64))> {
    let points: Box<dyn Iterator<Item = ((f64, f64), f64)>> = match file.read.as_ref().ok()? {
        Read::Gerber(g) => Box::new(
            g.areas
                .iter()
                .flat_map(|a| a.outer.iter().map(|p| (*p, 0.0))),
        ),
        Read::Drill(holes) => Box::new(holes.iter().flat_map(|h| {
            let r = h.diameter / 2.0;
            std::iter::once(((h.x, h.y), r)).chain(h.slot_to.map(|to| (to, r)))
        })),
    };
    points.fold(None, |acc, (p, r)| {
        let (lo, hi) = ((p.0 - r, p.1 - r), (p.0 + r, p.1 + r));
        Some(match acc {
            None => (lo, hi),
            Some((a, b)) => (
                (a.0.min(lo.0), a.1.min(lo.1)),
                (b.0.max(hi.0), b.1.max(hi.1)),
            ),
        })
    })
}

impl Package {
    /// The summary of every file ([`Summary`]).
    pub fn summary(&self) -> Summary {
        summary::of(self)
    }

    /// The files that could not be read, with why.
    pub fn unread(&self) -> impl Iterator<Item = (&File, &RasterError)> {
        self.files
            .iter()
            .filter_map(|f| f.read.as_ref().err().map(|e| (f, e)))
    }
}
