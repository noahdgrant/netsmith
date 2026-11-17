//! The schematic as a document: the SVG the editor shows (`render_layout`, `GET
//! /api/schematic.svg`) written as a PDF, a PNG, or the SVG of one sheet (issue #1091). Each
//! is a pure function of that SVG, so the command line, `fab` and the editor give the same
//! bytes for the same drawing; `netsmith_draw` reads and writes it.
//!
//! A sheet is a module's box of a board drawn by module (`<g class="module-sheet"
//! data-module="..." data-bounds="x y w h">`), its region the box with [`SHEET_MARGIN`] round
//! it; a board drawn as one sheet (16 parts or fewer) is one sheet named for the board. The
//! PDF has a page for each sheet of each drawing it is given, in order, sized to the sheet,
//! with the sheet's name, the board's, the project's and the release in its title block. The
//! PNG is [`PNG_PER_MM`] pixels a millimetre, or fewer so that its longer side is at most
//! [`PNG_LONGEST`].

use netsmith_draw::pdf::{Document, Page, TitleBlock};
use netsmith_draw::{Picture, Rect, Rgba, raster, svg};

/// Room round a sheet's box on its page and in its picture, in millimetres.
pub const SHEET_MARGIN: f64 = 2.0;
/// The pixels a millimetre of a PNG: about 250 to the inch.
pub const PNG_PER_MM: f64 = 10.0;
/// The most pixels along the longer side of a PNG.
pub const PNG_LONGEST: usize = 8192;

/// What the schematic is written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Pdf,
    Svg,
    Png,
}

impl Format {
    /// The file's extension, which is also the word `--format` takes.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Pdf => "pdf",
            Format::Svg => "svg",
            Format::Png => "png",
        }
    }

    pub fn media_type(self) -> &'static str {
        match self {
            Format::Pdf => "application/pdf",
            Format::Svg => "image/svg+xml",
            Format::Png => "image/png",
        }
    }
}

/// What every title block says besides the sheet and the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// The project's name, when the board is in one.
    pub project: Option<String>,
    /// The release of netsmith that drew it: `netsmith 0.2.0`.
    pub release: String,
}

/// One sheet of a drawing: its name and the region of the drawing it is.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetRegion {
    pub name: String,
    pub region: Rect,
}

/// Why a schematic could not be written.
#[derive(Debug, Clone, PartialEq)]
pub enum ExportError {
    /// The drawing is not an SVG that can be read (it never fails for one netsmith drew).
    Unreadable(String),
    /// `--sheet` names no sheet of the board: the sheets it has.
    NoSheet { asked: String, sheets: Vec<String> },
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::Unreadable(why) => write!(f, "the schematic drawing: {why}"),
            ExportError::NoSheet { asked, sheets } => write!(
                f,
                "the schematic has no sheet `{asked}`; its sheets are {}",
                sheets
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl std::error::Error for ExportError {}

/// A drawing read once: its picture, its board's name and its sheets.
struct Drawing {
    picture: Picture,
    board: String,
    /// A system's own sheet (its root has the class `system`), which its title block says.
    system: bool,
    sheets: Vec<SheetRegion>,
}

fn read(svg_text: &str) -> Result<Drawing, ExportError> {
    let picture = svg::read(svg_text).map_err(|e| ExportError::Unreadable(e.to_string()))?;
    let doc =
        roxmltree::Document::parse(svg_text).map_err(|e| ExportError::Unreadable(e.to_string()))?;
    let root = doc.root_element();
    let board = root
        .attribute("data-board")
        .unwrap_or("schematic")
        .to_string();
    let system = root
        .attribute("class")
        .is_some_and(|c| c.split_whitespace().any(|c| c == "system"));
    let mut sheets: Vec<SheetRegion> = doc
        .descendants()
        .filter(|n| {
            n.is_element()
                && n.attribute("class")
                    .is_some_and(|c| c.split_whitespace().any(|c| c == "module-sheet"))
        })
        .filter_map(|n| {
            let name = n.attribute("data-module")?;
            let b: Vec<f64> = n
                .attribute("data-bounds")?
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            let [x, y, w, h] = b[..] else { return None };
            Some(SheetRegion {
                name: name.to_string(),
                region: Rect::new(x, y, w, h).inflated(SHEET_MARGIN),
            })
        })
        .collect();
    if sheets.is_empty() {
        sheets.push(SheetRegion {
            name: board.clone(),
            region: picture.view,
        });
    }
    Ok(Drawing {
        picture,
        board,
        system,
        sheets,
    })
}

/// The sheets of the drawing `svg`, in the order the PDF has them.
pub fn sheets(svg: &str) -> Result<Vec<SheetRegion>, ExportError> {
    Ok(read(svg)?.sheets)
}

impl Drawing {
    /// The region of `sheet`, or of the whole drawing when none is asked for.
    fn region(&self, sheet: Option<&str>) -> Result<Rect, ExportError> {
        match sheet {
            None => Ok(self.picture.view),
            Some(name) => self
                .sheets
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.region)
                .ok_or_else(|| ExportError::NoSheet {
                    asked: name.to_string(),
                    sheets: self.sheets.iter().map(|s| s.name.clone()).collect(),
                }),
        }
    }
}

/// The PDF of the drawings `svgs` (a board's, or a system's own sheet and its member boards'
/// one after the other): a page for each sheet of each.
pub fn pdf(title: &str, svgs: &[&str], heading: &Heading) -> Result<Vec<u8>, ExportError> {
    let drawings: Vec<Drawing> = svgs.iter().map(|s| read(s)).collect::<Result<_, _>>()?;
    Ok(write_pdf(title, &drawings, None, heading))
}

/// The PDF of one sheet of the drawing `svg`: one page.
pub fn pdf_sheet(
    title: &str,
    svg: &str,
    sheet: &str,
    heading: &Heading,
) -> Result<Vec<u8>, ExportError> {
    let drawing = read(svg)?;
    drawing.region(Some(sheet))?;
    Ok(write_pdf(title, &[drawing], Some(sheet), heading))
}

fn write_pdf(title: &str, drawings: &[Drawing], only: Option<&str>, heading: &Heading) -> Vec<u8> {
    let chosen: Vec<(&Drawing, &SheetRegion)> = drawings
        .iter()
        .flat_map(|d| d.sheets.iter().map(move |s| (d, s)))
        .filter(|(_, s)| only.is_none_or(|name| s.name == name))
        .collect();
    let total = chosen.len();
    let pages = chosen
        .iter()
        .enumerate()
        .map(|(i, (d, sheet))| {
            let what = if d.system { "System" } else { "Board" };
            let mut lines = vec![format!("{what}: {}", d.board)];
            if let Some(p) = &heading.project {
                lines.push(format!("Project: {p}"));
            }
            lines.push(format!("Drawn by {}", heading.release));
            lines.push(format!("Sheet {} of {total}", i + 1));
            Page {
                picture: &d.picture,
                view: sheet.region,
                title: TitleBlock {
                    heading: sheet.name.clone(),
                    lines,
                },
            }
        })
        .collect();
    netsmith_draw::pdf::write(&Document {
        title: title.to_string(),
        producer: heading.release.clone(),
        pages,
    })
}

/// The PNG of the drawing `svg`, or of one of its sheets.
pub fn png(svg: &str, sheet: Option<&str>) -> Result<Vec<u8>, ExportError> {
    let d = read(svg)?;
    let region = d.region(sheet)?;
    let scale = raster::scale_for(region, PNG_PER_MM, PNG_LONGEST);
    Ok(raster::png(&raster::render(
        &d.picture,
        region,
        scale,
        Rgba::WHITE,
    )))
}

/// The drawing `svg` itself, or with its view cut to one of its sheets.
pub fn svg_of(svg: &str, sheet: Option<&str>) -> Result<String, ExportError> {
    let Some(name) = sheet else {
        return Ok(svg.to_string());
    };
    let d = read(svg)?;
    let r = d.region(Some(name))?;
    let view = format!(
        "viewBox=\"{} {} {} {}\"",
        num(r.min.x),
        num(r.min.y),
        num(r.width()),
        num(r.height())
    );
    // The first `viewBox` is the root's.
    let Some(start) = svg.find("viewBox=\"") else {
        return Ok(svg.to_string());
    };
    let end = start + 9 + svg[start + 9..].find('"').unwrap_or(0) + 1;
    Ok(format!("{}{view}{}", &svg[..start], &svg[end..]))
}

fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}
