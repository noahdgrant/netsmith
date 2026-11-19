//! A land pattern taken from a reference board's Gerber, drill and placement files (issue 1432,
//! `netsmith import footprint` given a folder or a zip of them): the last resort, when a vendor
//! gives no KiCad board or footprint file (`netsmith_kicad::import_footprint` takes those).
//!
//! The files are read with netsmith's own Gerber and Excellon readers (the Gerber viewer's,
//! `netsmith_fab::viewer`), and the part is found by its reference designator in the placement
//! file (a column of designators, the centre's x and y, the rotation and the face, found by
//! name: JLCPCB's `Designator,Mid X,Mid Y,Layer,Rotation`, KiCad's `.pos` file in CSV or in
//! columns, the `Center-X(mm)` and `Ref X` of other tools; a value may carry `mm` or `mil`).
//!
//! What a Gerber file holds is a picture, so the footprint is rebuilt, and a part of it is a
//! guess. What is rebuilt and what is guessed is said in [`Imported::rebuilt`] and
//! [`Imported::guessed`], and the command prints both:
//!
//! - **The part's pads.** When the copper file has the X2 object attributes (`%TO.P`, the pin of
//!   a part, or `%TO.C`, the part), the pads are the flashes it names, with their numbers. When it
//!   has not, each flash of the part's face's outer copper goes to the part whose placement is
//!   nearest (a guess: a big part's corner pad may be nearer a small neighbour), leaving out vias
//!   and fiducials (by X2 `%TA.AperFunction` when there is one, and a copper flash on a plated
//!   hole with no mask opening otherwise), and the pads are numbered counter-clockwise from the
//!   top left, the pads at the middle last (a guess: check pin 1 on the datasheet).
//! - **A pad's shape and size** are its aperture's: a circle, a rectangle, an obround (KiCad's
//!   `oval`), a rounded rectangle from KiCad's `RoundRect` macro, a turned rectangle from its
//!   `RotRect`, any other macro of one polygon a `custom` pad of that polygon, and a macro of
//!   more a rectangle round it (a guess). A regular polygon is a circle of its diameter (a guess).
//! - **The holes** are the drill file's: a plated hole on a pad makes it a through-hole pad of
//!   that drill (a slot an oval drill), an unplated one nearest the part and within 2 mm of its
//!   pads a non-plated hole.
//! - **Mask and paste.** A surface pad has the mask where the mask file opens over it and the
//!   paste where the paste file has some; with no mask or no paste file, both are assumed (a
//!   guess).
//! - **The place.** The part's centre and rotation from the placement file are taken off (the
//!   placement file and the Gerber files are taken to share their origin, as KiCad and netsmith
//!   write them), and a part on the bottom is mirrored back to be seen from the top. A rotation
//!   an assembler's file gives may hold the assembler's own offset (JLCPCB's), so the footprint
//!   may come out turned: a guess, said when the part is not at 0°.
//! - **Not rebuilt.** The courtyard is a rectangle 0.25 mm round the pads (IPC-7351's nominal
//!   excess, a guess) and the fabrication layer has the reference and the value only: the body,
//!   the silkscreen and a 3D model are not in the files.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use netsmith_fab::ExcellonHole;
use netsmith_fab::viewer::{self, ApertureShape, Face, Flash, Function, Layer, Read};

/// How far the courtyard is from the pads, in mm (IPC-7351's nominal density).
const COURTYARD_EXCESS: f64 = 0.25;
/// How far from the part's pads an unplated hole may be to be the part's, in mm.
const HOLE_REACH: f64 = 2.0;
/// Two points closer than this, in mm, are one place (a hole on a pad, a mask flash on a pad).
const SAME_PLACE: f64 = 0.01;

/// A footprint rebuilt from Gerber files.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// The name it is written under.
    pub name: String,
    /// The `.kicad_mod` text.
    pub text: String,
    /// Whether the part was on the bottom face (and is mirrored back to the top).
    pub from_bottom: bool,
    /// What was taken from the files, one line each.
    pub rebuilt: Vec<String>,
    /// What was guessed, one line each, for a person to check against the datasheet.
    pub guessed: Vec<String>,
}

/// The files of a folder (and of the zips in it) or of a zip, by name: what [`import`] reads.
pub fn gather(path: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let shown = path.display();
    if path.is_dir() {
        let mut found = Vec::new();
        walk(path, "", &mut found).map_err(|e| format!("{shown}: {e}"))?;
        let mut inner = Vec::new();
        for (name, bytes) in &found {
            if name.to_ascii_lowercase().ends_with(".zip") {
                for (m, b) in viewer::unzip(bytes).map_err(|e| format!("{name}: {e}"))? {
                    inner.push((format!("{name}/{m}"), b));
                }
            }
        }
        found.retain(|(n, _)| !n.to_ascii_lowercase().ends_with(".zip"));
        found.extend(inner);
        return Ok(found);
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{shown}: {e}"))?;
    if !bytes.starts_with(b"PK") {
        return Err(format!("{shown}: not a zip"));
    }
    viewer::unzip(&bytes).map_err(|e| format!("{shown}: {e}"))
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = format!("{prefix}{}", e.file_name().to_string_lossy());
        if e.file_type()?.is_dir() {
            walk(&e.path(), &format!("{name}/"), out)?;
        } else if e.metadata()?.len() <= 64 << 20 {
            out.push((name, std::fs::read(e.path())?));
        }
    }
    Ok(())
}

/// Whether the files hold a Gerber file (so a folder is a Gerber package, not a `.pretty`).
pub fn has_gerber(files: &[(String, Vec<u8>)]) -> bool {
    files
        .iter()
        .any(|(_, b)| viewer::sniff(b) == Some(viewer::Format::Gerber))
}

/// One row of a placement file.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub designator: String,
    /// The centre, in mm, y up.
    pub x: f64,
    pub y: f64,
    /// Degrees counter-clockwise.
    pub rotation: f64,
    pub bottom: bool,
    /// The footprint's name, when the file has a `Package` or `Footprint` column.
    pub package: Option<String>,
}

/// The rows of a placement file, or `None` when `text` is not one. Columns are found by name in
/// the first line that names a designator and an x and a y.
pub fn parse_placement(text: &str) -> Option<Vec<Placement>> {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let comma = line.contains(',');
        let header_text = line.trim_start_matches('#');
        let header = split(header_text, comma);
        let Some(cols) = Columns::find(&header) else {
            continue;
        };
        let mil = line.to_ascii_lowercase().contains("mil");
        let mut rows = Vec::new();
        for row in &lines[i + 1..] {
            if row.trim().is_empty() || row.trim_start().starts_with('#') {
                continue;
            }
            let cells = split(row, comma);
            let cell = |c: usize| cells.get(c).map(String::as_str).unwrap_or("");
            let number = |c: usize| length(cell(c), mil);
            let (Some(x), Some(y)) = (number(cols.x), number(cols.y)) else {
                continue;
            };
            let rotation = cols
                .rotation
                .and_then(|c| cell(c).trim_end_matches(['°']).parse().ok())
                .unwrap_or(0.0);
            let bottom = cols.side.is_some_and(|c| {
                matches!(
                    cell(c).to_ascii_lowercase().as_str(),
                    "bottom" | "bot" | "b" | "back" | "bottomlayer" | "bottom layer"
                )
            });
            rows.push(Placement {
                designator: cell(cols.designator).to_string(),
                x,
                y,
                rotation,
                bottom,
                package: cols
                    .package
                    .map(|c| cell(c).to_string())
                    .filter(|p| !p.is_empty()),
            });
        }
        return Some(rows);
    }
    None
}

/// A length of a placement file in mm: `12.5`, `12.5mm`, `492mil` (or mils by the header).
fn length(text: &str, mil: bool) -> Option<f64> {
    let t = text.trim().to_ascii_lowercase();
    if let Some(v) = t.strip_suffix("mil") {
        return v.trim().parse::<f64>().ok().map(|v| v * 0.0254);
    }
    let v: f64 = t.strip_suffix("mm").unwrap_or(&t).trim().parse().ok()?;
    Some(if mil { v * 0.0254 } else { v })
}

/// The cells of a line: by commas (quotes kept together) or by blanks.
fn split(line: &str, comma: bool) -> Vec<String> {
    if !comma {
        return line.split_whitespace().map(str::to_string).collect();
    }
    let mut out = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cell).trim().to_string()),
            _ => cell.push(c),
        }
    }
    out.push(cell.trim().to_string());
    out
}

struct Columns {
    designator: usize,
    x: usize,
    y: usize,
    rotation: Option<usize>,
    side: Option<usize>,
    package: Option<usize>,
}

impl Columns {
    fn find(header: &[String]) -> Option<Columns> {
        // A name without case, blanks, punctuation and its unit (`Center-X(mm)` is `centerx`).
        let names: Vec<String> = header
            .iter()
            .map(|h| {
                let h = h.split('(').next().unwrap_or("").to_ascii_lowercase();
                h.chars().filter(char::is_ascii_alphanumeric).collect()
            })
            .collect();
        let find = |words: &[&str]| words.iter().find_map(|w| names.iter().position(|n| n == w));
        let axis = |a: char| {
            let words: Vec<String> = ["mid", "center", "centre", "pos", "location", "ref", ""]
                .iter()
                .map(|p| format!("{p}{a}"))
                .collect();
            let refs: Vec<&str> = words.iter().map(String::as_str).collect();
            find(&refs)
        };
        Some(Columns {
            designator: find(&["designator", "ref", "refdes", "reference", "designators"])?,
            x: axis('x')?,
            y: axis('y')?,
            rotation: find(&["rotation", "rot", "angle"]),
            side: find(&["layer", "side", "tb"]),
            package: find(&["package", "footprint"]),
        })
    }
}

/// A pad rebuilt, in the footprint's frame (mm, y down, as KiCad has it).
#[derive(Debug, Clone)]
struct Pad {
    number: String,
    kind: Kind,
    shape: Shape,
    at: (f64, f64),
    size: (f64, f64),
    angle: f64,
    mask: bool,
    paste: bool,
    /// The corner points of what it covers, for the courtyard.
    corners: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Smd,
    /// A plated hole: its diameter, and the slot's length and direction in the frame.
    Through(Drill),
    NonPlated(Drill),
}

#[derive(Debug, Clone, PartialEq)]
enum Drill {
    Round(f64),
    /// `(drill oval w h)`.
    Oval(f64, f64),
}

#[derive(Debug, Clone, PartialEq)]
enum Shape {
    Circle,
    Rect,
    Oval,
    RoundRect(f64),
    /// A polygon, relative to the pad's place.
    Custom(Vec<(f64, f64)>),
}

/// The footprint of the part `reference` in `files` (a folder's or a zip's, [`gather`]), named
/// `name` (or the placement file's `Package`; neither is an error). The error says what the
/// files lack: no Gerber file, no placement file with the part, no copper flash near it.
pub fn import(
    files: Vec<(String, Vec<u8>)>,
    reference: &str,
    name: Option<&str>,
) -> Result<Imported, String> {
    let (gerbers, others): (Vec<_>, Vec<_>) = files
        .into_iter()
        .partition(|(_, b)| viewer::sniff(b).is_some());
    if gerbers.is_empty() {
        return Err("no Gerber or drill file in it".into());
    }
    let mut placement = None;
    let mut seen_files = Vec::new();
    for (file, bytes) in &others {
        let Some(rows) = parse_placement(&String::from_utf8_lossy(bytes)) else {
            continue;
        };
        seen_files.push(file.clone());
        if rows.iter().any(|r| r.designator == reference) {
            placement = Some((file.clone(), rows));
            break;
        }
    }
    let Some((placement_file, rows)) = placement else {
        return Err(if seen_files.is_empty() {
            "no placement file (a CSV of designators and their x, y and rotation) in it: add the reference board's placement file beside its Gerber files".into()
        } else {
            format!(
                "the placement file{} ({}) {} no part `{reference}`",
                if seen_files.len() == 1 { "" } else { "s" },
                seen_files.join(", "),
                if seen_files.len() == 1 { "has" } else { "have" }
            )
        });
    };
    let part = rows
        .iter()
        .find(|r| r.designator == reference)
        .cloned()
        .expect("found above");
    let name = match name {
        Some(n) => n.to_string(),
        None => part.package.clone().ok_or_else(|| {
            format!(
                "{placement_file} gives no footprint name for `{reference}`: name it with `--name`"
            )
        })?,
    };

    let pkg = viewer::read("package", gerbers);
    let (layer, face) = if part.bottom {
        (Layer::Bottom, Face::Bottom)
    } else {
        (Layer::Top, Face::Top)
    };
    let side = if part.bottom { "bottom" } else { "top" };
    let mut rebuilt = vec![format!(
        "the place of {reference} from {placement_file}: ({}, {}) mm, {}°, {side}",
        num(part.x),
        num(part.y),
        num(part.rotation)
    )];
    let mut guessed = Vec::new();

    let flashes_of = |function: Function| -> Vec<&Flash> {
        pkg.files
            .iter()
            .filter(|f| f.function == function)
            .filter_map(|f| match &f.read {
                Ok(Read::Gerber(g)) => Some(g.facts.flashed.iter()),
                _ => None,
            })
            .flatten()
            .filter(|f| f.dark)
            .collect()
    };
    let areas_of = |function: Function| -> Option<Vec<&viewer::Area>> {
        let files: Vec<_> = pkg
            .files
            .iter()
            .filter(|f| f.function == function)
            .filter_map(|f| match &f.read {
                Ok(Read::Gerber(g)) => Some(g),
                _ => None,
            })
            .collect();
        (!files.is_empty()).then(|| files.iter().flat_map(|g| g.areas.iter()).collect())
    };
    let copper_files: Vec<&str> = pkg
        .files
        .iter()
        .filter(|f| f.function == Function::Copper(layer))
        .map(|f| f.name.as_str())
        .collect();
    if copper_files.is_empty() {
        return Err(format!(
            "no {side} copper file among the Gerber files (found by its X2 file function or its name)"
        ));
    }
    let copper = flashes_of(Function::Copper(layer));
    let mask = areas_of(Function::Mask(face));
    let paste = areas_of(Function::Paste(face));
    let holes: Vec<&ExcellonHole> = pkg
        .files
        .iter()
        .filter_map(|f| match &f.read {
            Ok(Read::Drill(h)) => Some(h.iter()),
            _ => None,
        })
        .flatten()
        .collect();
    let on_hole = |p: (f64, f64), plated: bool| {
        holes
            .iter()
            .find(|h| h.plated == plated && dist((h.x, h.y), p) < SAME_PLACE)
            .copied()
    };
    let covered = |areas: &Option<Vec<&viewer::Area>>, p: (f64, f64)| {
        areas.as_ref().map(|a| a.iter().any(|a| inside(a, p)))
    };

    // The part's flashes: named by X2, or the nearest placement's.
    let named: Vec<&Flash> = copper
        .iter()
        .copied()
        .filter(|f| {
            f.pin.as_ref().is_some_and(|(r, _)| r == reference)
                || f.component.as_deref() == Some(reference)
        })
        .collect();
    let by_x2 = !named.is_empty();
    let chosen: Vec<&Flash> = if by_x2 {
        rebuilt.push(format!(
            "the pads of {reference}: the flashes {} names {reference}'s by its X2 attributes",
            copper_files.join(", ")
        ));
        named
    } else {
        let same_side: Vec<&Placement> = rows.iter().filter(|r| r.bottom == part.bottom).collect();
        let mut vias = 0;
        let picked: Vec<&Flash> = copper
            .iter()
            .copied()
            .filter(|f| {
                let function = f.function.as_deref().unwrap_or("");
                if function.starts_with("ViaPad") || function.starts_with("FiducialPad") {
                    return false;
                }
                if f.function.is_none()
                    && on_hole(f.at, true).is_some()
                    && covered(&mask, f.at) == Some(false)
                {
                    vias += 1;
                    return false;
                }
                true
            })
            .filter(|f| {
                let nearest = same_side
                    .iter()
                    .min_by(|a, b| dist((a.x, a.y), f.at).total_cmp(&dist((b.x, b.y), f.at)));
                nearest.is_some_and(|n| n.designator == reference)
            })
            .collect();
        guessed.push(format!(
            "the pads of {reference}: the flashes of {} nearer its centre than any other part's of {placement_file} (the copper has no X2 attributes naming them); a big part's pad may have gone to a neighbour, or a part the placement file leaves out given its pads to {reference}{}",
            copper_files.join(", "),
            if vias > 0 {
                format!(
                    "; {vias} copper flash{} on a plated hole with no mask opening taken for via{}",
                    if vias == 1 { "" } else { "es" },
                    if vias == 1 { "" } else { "s" }
                )
            } else {
                String::new()
            }
        ));
        picked
    };
    if chosen.is_empty() {
        return Err(format!(
            "no copper flash of {} is nearer {reference} than another part: the placement file and the Gerber files may not share an origin",
            copper_files.join(", ")
        ));
    }

    // From the board's frame (mm, y up) to the footprint's (mm, y down), seen from the top.
    let rot = part.rotation;
    let centre = (part.x, part.y);
    let local_up = |p: (f64, f64)| {
        let d = rotate((p.0 - centre.0, p.1 - centre.1), -rot);
        if part.bottom { (-d.0, d.1) } else { d }
    };
    let to_frame = |p: (f64, f64)| {
        let q = local_up(p);
        (q.0, -q.1)
    };
    // A vector of the flash (relative to its point) into the frame.
    let vector = |v: (f64, f64)| {
        let d = rotate(v, -rot);
        let d = if part.bottom { (-d.0, d.1) } else { d };
        (d.0, -d.1)
    };
    let local_angle = |board: f64| {
        let a = if part.bottom {
            rot - board
        } else {
            board - rot
        };
        a.rem_euclid(360.0)
    };

    let mut pads = Vec::new();
    let mut shape_guesses = BTreeMap::<String, usize>::new();
    let mut mask_paste_counts = (0, 0);
    let mut offset_drills = 0;
    for f in &chosen {
        let at = to_frame(f.at);
        let (mut shape, mut angle, mut offset) = (Shape::Rect, 0.0, (0.0, 0.0));
        let mut size: (f64, f64);
        match &f.shape {
            ApertureShape::Circle { diameter } => {
                shape = Shape::Circle;
                size = (*diameter, *diameter);
            }
            ApertureShape::Rect { width, height } => {
                size = (*width, *height);
                angle = local_angle(f.rotation);
            }
            ApertureShape::Obround { width, height } => {
                shape = Shape::Oval;
                size = (*width, *height);
                angle = local_angle(f.rotation);
            }
            ApertureShape::Polygon { diameter, .. } => {
                shape = Shape::Circle;
                size = (*diameter, *diameter);
                *shape_guesses
                    .entry("a regular polygon's aperture made a circle of its diameter".into())
                    .or_default() += 1;
            }
            ApertureShape::Macro { name, params } => {
                let pts: Vec<(f64, f64)> = f.outline.iter().map(|&v| vector(v)).collect();
                let (lo, hi) = bbox(&pts);
                let mid = ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0);
                if name == "RotRect" && params.len() >= 3 {
                    size = (params[0] * f.unit, params[1] * f.unit);
                    angle = local_angle(params[2] + f.rotation);
                } else if name == "RoundRect" && params.len() >= 9 {
                    // KiCad's macro: the radius, then the centres of the four corners' circles;
                    // its size is theirs and the radius round them (exact, where the drawn
                    // circles' points are a little outside).
                    let r = params[0] * f.unit;
                    let corners: Vec<(f64, f64)> = params[1..9]
                        .chunks(2)
                        .map(|c| vector((c[0] * f.unit, c[1] * f.unit)))
                        .collect();
                    let (clo, chi) = bbox(&corners);
                    size = (chi.0 - clo.0 + 2.0 * r, chi.1 - clo.1 + 2.0 * r);
                    offset = ((clo.0 + chi.0) / 2.0, (clo.1 + chi.1) / 2.0);
                    let short = size.0.min(size.1);
                    shape = Shape::RoundRect(if short > 0.0 {
                        (r / short).min(0.5)
                    } else {
                        0.25
                    });
                } else if let Some(poly) = single_polygon(f) {
                    let poly: Vec<(f64, f64)> = poly.iter().map(|&v| vector(v)).collect();
                    shape = Shape::Custom(poly);
                    size = (0.0, 0.0);
                } else {
                    size = (hi.0 - lo.0, hi.1 - lo.1);
                    offset = mid;
                    *shape_guesses
                        .entry(format!(
                            "the aperture macro `{name}`, of several shapes, made a rectangle round it"
                        ))
                        .or_default() += 1;
                }
            }
        }
        // A rectangle or an oval is the same turned a half turn; a quarter turn swaps its sides.
        if matches!(shape, Shape::Rect | Shape::Oval | Shape::RoundRect(_)) {
            let a = angle.rem_euclid(180.0);
            if a.abs() < 1e-6 || (a - 180.0).abs() < 1e-6 {
                angle = 0.0;
            } else if (a - 90.0).abs() < 1e-6 {
                size = (size.1, size.0);
                angle = 0.0;
            } else {
                angle = a;
            }
        } else if !matches!(shape, Shape::Custom(_)) {
            angle = 0.0;
        }
        let at = (at.0 + offset.0, at.1 + offset.1);
        let kind = match on_hole_near(&holes, f.at, true, size) {
            Some(h) => {
                if dist((h.x, h.y), f.at) > SAME_PLACE {
                    offset_drills += 1;
                }
                Kind::Through(drill_of(h, &vector))
            }
            None => Kind::Smd,
        };
        let (m, p) = (covered(&mask, f.at), covered(&paste, f.at));
        let mask_on = m.unwrap_or(true);
        let paste_on = kind == Kind::Smd && p.unwrap_or(true);
        mask_paste_counts.0 += usize::from(m.is_none());
        mask_paste_counts.1 += usize::from(p.is_none() && kind == Kind::Smd);
        let corners = match &shape {
            Shape::Custom(poly) => poly.iter().map(|q| (q.0 + at.0, q.1 + at.1)).collect(),
            _ => {
                let (hw, hh) = (size.0 / 2.0, size.1 / 2.0);
                [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)]
                    .iter()
                    // KiCad turns a pad counter-clockwise as seen, which in its y-down frame is
                    // the negative angle of the maths.
                    .map(|&c| {
                        let r = rotate(c, -angle);
                        (r.0 + at.0, r.1 + at.1)
                    })
                    .collect()
            }
        };
        pads.push(Pad {
            number: f.pin.as_ref().map(|(_, n)| n.clone()).unwrap_or_default(),
            kind,
            shape,
            at,
            size,
            angle,
            mask: mask_on,
            paste: paste_on,
            corners,
        });
    }

    // Unplated holes of the part: the nearest placement is the part's and they are near its pads.
    let (lo, hi) = bbox(
        &pads
            .iter()
            .flat_map(|p| p.corners.clone())
            .collect::<Vec<_>>(),
    );
    let same_side: Vec<&Placement> = rows.iter().filter(|r| r.bottom == part.bottom).collect();
    let mut npth = 0;
    for h in holes.iter().filter(|h| !h.plated) {
        let at = to_frame((h.x, h.y));
        let near = at.0 >= lo.0 - HOLE_REACH
            && at.0 <= hi.0 + HOLE_REACH
            && at.1 >= lo.1 - HOLE_REACH
            && at.1 <= hi.1 + HOLE_REACH;
        let nearest = same_side
            .iter()
            .min_by(|a, b| dist((a.x, a.y), (h.x, h.y)).total_cmp(&dist((b.x, b.y), (h.x, h.y))));
        if !near || !nearest.is_some_and(|n| n.designator == reference) {
            continue;
        }
        let drill = drill_of(h, &vector);
        let size = match drill {
            Drill::Round(d) => (d, d),
            Drill::Oval(w, hh) => (w, hh),
        };
        npth += 1;
        pads.push(Pad {
            number: String::new(),
            kind: Kind::NonPlated(drill),
            shape: if size.0 == size.1 {
                Shape::Circle
            } else {
                Shape::Oval
            },
            at,
            size,
            angle: 0.0,
            mask: true,
            paste: false,
            corners: vec![
                (at.0 - size.0 / 2.0, at.1 - size.1 / 2.0),
                (at.0 + size.0 / 2.0, at.1 + size.1 / 2.0),
            ],
        });
    }

    let copper_pads = pads
        .iter()
        .filter(|p| !matches!(p.kind, Kind::NonPlated(_)));
    let through = copper_pads
        .clone()
        .filter(|p| matches!(p.kind, Kind::Through(_)))
        .count();
    let total = copper_pads.count();
    rebuilt.push(format!(
        "{total} pad{} with their apertures' shapes and sizes{}",
        if total == 1 { "" } else { "s" },
        if through > 0 {
            format!(", {through} on a plated hole of the drill file")
        } else {
            String::new()
        }
    ));
    if npth > 0 {
        rebuilt.push(format!(
            "{npth} unplated hole{} of the drill file within {HOLE_REACH} mm of the pads",
            if npth == 1 { "" } else { "s" }
        ));
    }
    if offset_drills > 0 {
        guessed.push(format!(
            "{offset_drills} pad{} whose hole is off its middle drawn with the hole in the middle",
            if offset_drills == 1 { "" } else { "s" }
        ));
    }
    for (what, n) in shape_guesses {
        guessed.push(format!("{what} ({n} pad{})", if n == 1 { "" } else { "s" }));
    }
    if mask_paste_counts.0 > 0 {
        guessed.push(format!("no {side} mask file: every pad has a mask opening"));
    } else {
        rebuilt.push("the mask openings from the mask file".into());
    }
    if mask_paste_counts.1 > 0 {
        guessed.push(format!("no {side} paste file: every surface pad has paste"));
    } else if pads.iter().any(|p| p.kind == Kind::Smd) {
        rebuilt.push("the paste from the paste file".into());
    }

    // Numbers: the files' (X2 `%TO.P`), or counter-clockwise from the top left.
    let numbered = pads.iter().filter(|p| !p.number.is_empty()).count();
    if numbered < total {
        number_pads(&mut pads);
        guessed.push(format!(
            "the pad numbers, counted counter-clockwise from the top left with the pads at the middle last{}: check pin 1 against the datasheet",
            if numbered > 0 {
                format!(" ({numbered} of {total} were named by the files, and are kept)")
            } else {
                String::new()
            }
        ));
    } else {
        rebuilt.push("the pad numbers from the copper file's X2 attributes".into());
    }
    if rot.rem_euclid(360.0).abs() > 1e-9 {
        guessed.push(format!(
            "the footprint's angle: the placement file's {}° is taken off; an assembler's file may add its own offset, so check that pin 1 is where the datasheet draws it",
            num(rot)
        ));
    }
    if part.bottom {
        guessed.push(
            "the bottom face: the pads are mirrored left to right to be seen from the top".into(),
        );
    }
    guessed.push(format!(
        "the courtyard: a rectangle {COURTYARD_EXCESS} mm round the pads (the files have no courtyard)"
    ));
    rebuilt.push(
        "not rebuilt: the body on the fabrication layer, the silkscreen and a 3D model (the files do not say them)".into(),
    );

    let text = write(&name, &pads, reference);
    Ok(Imported {
        name,
        text,
        from_bottom: part.bottom,
        rebuilt,
        guessed,
    })
}

/// The plated hole under a pad: at its point, or within its size.
fn on_hole_near<'a>(
    holes: &[&'a ExcellonHole],
    p: (f64, f64),
    plated: bool,
    size: (f64, f64),
) -> Option<&'a ExcellonHole> {
    let reach = (size.0.min(size.1) / 2.0).max(SAME_PLACE);
    holes
        .iter()
        .filter(|h| h.plated == plated)
        .map(|h| (dist((h.x, h.y), p), *h))
        .filter(|(d, _)| *d < reach)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, h)| h)
}

/// A hole's drill in the frame: a round one, or a slot's oval along its length.
fn drill_of(h: &ExcellonHole, vector: &dyn Fn((f64, f64)) -> (f64, f64)) -> Drill {
    match h.slot_to {
        None => Drill::Round(h.diameter),
        Some(to) => {
            let v = vector((to.0 - h.x, to.1 - h.y));
            let long = (v.0 * v.0 + v.1 * v.1).sqrt() + h.diameter;
            if v.0.abs() >= v.1.abs() {
                Drill::Oval(long, h.diameter)
            } else {
                Drill::Oval(h.diameter, long)
            }
        }
    }
}

/// The polygon of a macro flash of one polygon, relative to its point.
fn single_polygon(f: &Flash) -> Option<Vec<(f64, f64)>> {
    // The outline is every point of the shapes drawn; a macro of one outline is a polygon whose
    // points are not repeated except to close it.
    let mut pts = f.outline.clone();
    if pts.len() < 3 {
        return None;
    }
    if pts.first() == pts.last() {
        pts.pop();
    }
    let area = pts
        .iter()
        .zip(pts.iter().cycle().skip(1))
        .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
        .sum::<f64>();
    let unique = pts.len()
        == pts
            .iter()
            .map(|p| ((p.0 * 1e4).round() as i64, (p.1 * 1e4).round() as i64))
            .collect::<std::collections::BTreeSet<_>>()
            .len();
    (unique && area.abs() > 1e-9 && pts.len() <= 64).then_some(pts)
}

/// Numbers the pads that have none, counter-clockwise from the top left as seen, those at the
/// middle last, after the numbers the files gave.
fn number_pads(pads: &mut [Pad]) {
    let given: std::collections::BTreeSet<String> = pads
        .iter()
        .filter(|p| !p.number.is_empty())
        .map(|p| p.number.clone())
        .collect();
    let mut order: Vec<usize> = (0..pads.len())
        .filter(|&i| pads[i].number.is_empty() && !matches!(pads[i].kind, Kind::NonPlated(_)))
        .collect();
    let reach = order
        .iter()
        .map(|&i| (pads[i].at.0.powi(2) + pads[i].at.1.powi(2)).sqrt())
        .fold(0.0, f64::max);
    let key = |p: &Pad| {
        let r = (p.at.0.powi(2) + p.at.1.powi(2)).sqrt();
        let middle = r <= reach * 0.05;
        // Seen with y up, counter-clockwise from the top left (135°).
        let theta = (-p.at.1).atan2(p.at.0).to_degrees();
        ((theta - 135.0).rem_euclid(360.0), middle)
    };
    order.sort_by(|&a, &b| {
        let (ka, ma) = key(&pads[a]);
        let (kb, mb) = key(&pads[b]);
        ma.cmp(&mb).then(ka.total_cmp(&kb))
    });
    let mut n = 0;
    for i in order {
        loop {
            n += 1;
            if !given.contains(&n.to_string()) {
                break;
            }
        }
        pads[i].number = n.to_string();
    }
}

/// The `.kicad_mod` text.
fn write(name: &str, pads: &[Pad], reference: &str) -> String {
    let (lo, hi) = bbox(
        &pads
            .iter()
            .flat_map(|p| p.corners.clone())
            .collect::<Vec<_>>(),
    );
    let snap = |v: f64| (v * 100.0).round() / 100.0;
    let (cx0, cy0, cx1, cy1) = (
        snap(lo.0 - COURTYARD_EXCESS),
        snap(lo.1 - COURTYARD_EXCESS),
        snap(hi.0 + COURTYARD_EXCESS),
        snap(hi.1 + COURTYARD_EXCESS),
    );
    let through = pads.iter().any(|p| matches!(p.kind, Kind::Through(_)));
    let mut s = String::new();
    let _ = writeln!(s, "(footprint {:?}", name);
    s.push_str("  (version 20240108)\n  (generator \"netsmith\")\n  (layer \"F.Cu\")\n");
    let _ = writeln!(
        s,
        "  (descr \"Rebuilt by netsmith from the Gerber, drill and placement files of a reference board ({reference}); check it against the datasheet\")"
    );
    let _ = writeln!(
        s,
        "  (attr {})",
        if through { "through_hole" } else { "smd" }
    );
    let _ = writeln!(
        s,
        "  (fp_text reference \"REF**\" (at 0 {}) (layer \"F.SilkS\") (effects (font (size 1 1) (thickness 0.15))))",
        num(cy0 - 1.0)
    );
    let _ = writeln!(
        s,
        "  (fp_text value {:?} (at 0 {}) (layer \"F.Fab\") (effects (font (size 1 1) (thickness 0.15))))",
        name,
        num(cy1 + 1.0)
    );
    s.push_str("  (fp_text user \"${REFERENCE}\" (at 0 0) (layer \"F.Fab\") (effects (font (size 0.5 0.5) (thickness 0.08))))\n");
    let _ = writeln!(
        s,
        "  (fp_rect (start {} {}) (end {} {}) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\"))",
        num(cx0),
        num(cy0),
        num(cx1),
        num(cy1)
    );
    for p in pads {
        let (kind, layers) = match &p.kind {
            Kind::Smd => {
                let mut l = vec!["\"F.Cu\""];
                if p.mask {
                    l.push("\"F.Mask\"");
                }
                if p.paste {
                    l.push("\"F.Paste\"");
                }
                ("smd", l.join(" "))
            }
            Kind::Through(_) => ("thru_hole", "\"*.Cu\" \"*.Mask\"".to_string()),
            Kind::NonPlated(_) => ("np_thru_hole", "\"*.Cu\" \"*.Mask\"".to_string()),
        };
        let shape = match &p.shape {
            Shape::Circle => "circle",
            Shape::Rect => "rect",
            Shape::Oval => "oval",
            Shape::RoundRect(_) => "roundrect",
            Shape::Custom(_) => "custom",
        };
        let at = if p.angle.abs() > 1e-9 {
            format!("{} {} {}", num(p.at.0), num(p.at.1), num(p.angle))
        } else {
            format!("{} {}", num(p.at.0), num(p.at.1))
        };
        let size = match &p.shape {
            // The anchor of a custom pad: a small rectangle inside its polygon.
            Shape::Custom(_) => (0.01, 0.01),
            _ => p.size,
        };
        let _ = write!(
            s,
            "  (pad {:?} {kind} {shape} (at {at}) (size {} {})",
            p.number,
            num(size.0),
            num(size.1)
        );
        if let Kind::Through(d) | Kind::NonPlated(d) = &p.kind {
            match d {
                Drill::Round(d) => {
                    let _ = write!(s, " (drill {})", num(*d));
                }
                Drill::Oval(w, h) => {
                    let _ = write!(s, " (drill oval {} {})", num(*w), num(*h));
                }
            }
        }
        let _ = write!(s, " (layers {layers})");
        if let Shape::RoundRect(r) = p.shape {
            let _ = write!(s, " (roundrect_rratio {})", num(r));
        }
        if let Shape::Custom(poly) = &p.shape {
            let pts: Vec<String> = poly
                .iter()
                .map(|q| format!("(xy {} {})", num(q.0), num(q.1)))
                .collect();
            let _ = write!(
                s,
                " (options (clearance outline) (anchor rect)) (primitives (gr_poly (pts {}) (width 0) (fill yes)))",
                pts.join(" ")
            );
        }
        s.push_str(")\n");
    }
    s.push_str(")\n");
    s
}

/// A number as KiCad writes it: at most four decimals, no trailing zeros, no `-0`.
fn num(v: f64) -> String {
    let mut s = format!("{v:.4}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" { "0".into() } else { s }
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// `p` turned `deg` degrees counter-clockwise (y up).
fn rotate(p: (f64, f64), deg: f64) -> (f64, f64) {
    let (s, c) = deg.to_radians().sin_cos();
    (p.0 * c - p.1 * s, p.0 * s + p.1 * c)
}

fn bbox(pts: &[(f64, f64)]) -> ((f64, f64), (f64, f64)) {
    pts.iter().fold(
        (
            (f64::INFINITY, f64::INFINITY),
            (f64::NEG_INFINITY, f64::NEG_INFINITY),
        ),
        |(lo, hi), p| {
            (
                (lo.0.min(p.0), lo.1.min(p.1)),
                (hi.0.max(p.0), hi.1.max(p.1)),
            )
        },
    )
}

/// Whether `p` is inside `area` (its outer edge, not in a hole).
fn inside(area: &viewer::Area, p: (f64, f64)) -> bool {
    in_ring(&area.outer, p) && !area.holes.iter().any(|h| in_ring(h, p))
}

fn in_ring(ring: &[(f64, f64)], p: (f64, f64)) -> bool {
    let mut odd = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1) {
            odd = !odd;
        }
    }
    odd
}
