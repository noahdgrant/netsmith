//! `netsmith dxf import`: the board outline, cutouts and mounting holes of an enclosure's DXF
//! file as source for the board's physical file (ADR 0045, step C0 piece 3, ADR 0048).
//!
//! The reader is `netsmith-dxf`. What this adds is the translation into what the language
//! has: an `Outline` (a rectangle with equal rounded corners), `Cutout` members (rounded
//! rectangles, slots and circles) and `MountingHole` parts at the holes' centres, with their
//! places, which are requirements the case fixes. They go in the `case { }` section of the
//! board's physical file: `--write` edits `<stem>.physical.nsm` at the spans of its syntax
//! tree, so its comments and the lines it does not name stay, and prints what changed. The
//! circuit source is never written. Without `--write` the text is printed, for a person (or
//! an agent) to paste.
//!
//! An outline or cutout that is not a rounded rectangle needs a polygon outline, which the
//! board model does not have (ADR 0045 step C8, issue #57): it is refused with its
//! size and edge count, and nothing is printed.

use std::fmt::Write as _;
use std::path::Path;

use crate::physical::{Edit, Section};
use netsmith_dxf::{Options, Outline, Polygon, RoundedRect, Units, read_outline};

/// What `dxf import` is asked for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Request {
    /// The outline layers; none lets the reader choose if there is only one possible.
    pub layers: Vec<String>,
    /// The layers whose circles are mounting holes.
    pub holes_layers: Vec<String>,
    /// The units, if the file does not say or is wrong.
    pub units: Option<Units>,
}

/// The mounting hole packages of `std/kicad.toml` and the diameter of their drill (mm).
const HOLES: [(&str, f64); 2] = [("M2", 2.2), ("M3", 3.2)];

/// How far a drawn hole may be from a package's drill and still be it (mm).
const HOLE_TOLERANCE: f64 = 0.05;

/// The result of an import: the text to paste into the physical file.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// What the file says in the way of the frame, the layers and the units.
    pub report: Vec<String>,
    /// The `import` lines to put at the top of the physical file.
    pub imports: String,
    /// The same, as `(module, name)` pairs.
    needed: Vec<(&'static str, &'static str)>,
    /// The statements to put in the `case { }` section: the outline, the cutouts and the
    /// holes, each with its place.
    pub members: String,
    /// The names of the members, which a later import replaces.
    pub names: Vec<String>,
    pub warnings: Vec<String>,
}

/// A length in millimetres as source: `36mm`, `12.5mm`, at most four decimals.
fn mm(v: f64) -> String {
    format!("{}mm", number(v))
}

fn number(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn round4(v: f64) -> f64 {
    let r = (v * 1e4).round() / 1e4;
    if r == 0.0 { 0.0 } else { r }
}

fn rounded_member(name: &str, kind: &str, rr: &RoundedRect, with_place: bool) -> String {
    let mut s = format!("{name}: {kind} {{\n");
    if with_place {
        let _ = writeln!(s, "    .x &= {}", mm(rr.x));
        let _ = writeln!(s, "    .y &= {}", mm(rr.y));
    }
    let _ = writeln!(s, "    .width &= {}", mm(rr.width));
    let _ = writeln!(s, "    .height &= {}", mm(rr.height));
    if rr.radius > 5e-5 {
        let _ = writeln!(s, "    .corner_radius &= {}", mm(rr.radius));
    }
    s.push_str("}\n");
    s
}

/// Read the file's text and translate it. `name` is the file's name for messages.
pub fn import(text: &str, name: &str, request: &Request) -> Result<Imported, String> {
    let outline: Outline = read_outline(
        text,
        &Options {
            outline_layers: request.layers.clone(),
            holes_layers: request.holes_layers.clone(),
            units: request.units,
            ..Options::default()
        },
    )
    .map_err(|e| format!("{name}: {e}"))?;

    // Everything that cannot be said in the language is collected, so that one run lists it.
    let mut refused: Vec<String> = Vec::new();
    let not_representable = |what: &str, p: &Polygon| {
        format!(
            "{what} is not a rectangle with equal rounded corners ({}): it needs a polygon outline, \
             which the board model does not have yet (ADR 0045 step C8, issue #57)",
            p.describe()
        )
    };
    let outer = outline.outer.as_rounded_rect();
    if outer.is_none() {
        refused.push(not_representable("the outline", &outline.outer));
    }
    let mut cutouts: Vec<RoundedRect> = Vec::new();
    for (i, c) in outline.cutouts.iter().enumerate() {
        match c.as_rounded_rect() {
            Some(rr) => cutouts.push(rr),
            None => refused.push(not_representable(&format!("cutout {}", i + 1), c)),
        }
    }
    let mut holes: Vec<(f64, f64, &'static str)> = Vec::new();
    for (i, h) in outline.holes.iter().enumerate() {
        match HOLES
            .iter()
            .find(|(_, d)| (d - h.diameter).abs() <= HOLE_TOLERANCE)
        {
            Some((package, _)) => holes.push((h.centre.0, h.centre.1, package)),
            None => refused.push(format!(
                "hole {} at ({}, {}) has a diameter of {} mm, and the mounting hole packages are {}",
                i + 1,
                number(h.centre.0),
                number(h.centre.1),
                number(h.diameter),
                HOLES
                    .iter()
                    .map(|(p, d)| format!("{p} ({d} mm)"))
                    .collect::<Vec<_>>()
                    .join(" and ")
            )),
        }
    }
    if !refused.is_empty() {
        return Err(format!(
            "{name}: cannot import:\n  - {}",
            refused.join("\n  - ")
        ));
    }
    let outer = outer.expect("checked above");

    let mut members = String::new();
    let mut names = vec!["outline".to_string()];
    members.push_str(&rounded_member(".outline", "Outline", &outer, false));
    for (i, rr) in cutouts.iter().enumerate() {
        let id = format!("cutout{}", i + 1);
        members.push('\n');
        members.push_str(&rounded_member(&format!(".{id}"), "Cutout", rr, true));
        names.push(id);
    }
    if !holes.is_empty() {
        members.push('\n');
    }
    for (i, (x, y, package)) in holes.iter().enumerate() {
        let id = format!("hole{}", i + 1);
        let _ = writeln!(
            members,
            ".{id}: MountingHole {{ .package &= \"{package}\" }}"
        );
        // The hole is where the case drills it: a requirement, stated by the centre of its body.
        let _ = writeln!(members, ".{id}.place.x &= {}", mm(round4(*x)));
        let _ = writeln!(members, ".{id}.place.y &= {}", mm(round4(*y)));
        names.push(id);
    }

    let mut imports = String::new();
    imports.push_str(if cutouts.is_empty() {
        "import std::pcb::Outline\n"
    } else {
        "import std::pcb::{Cutout, Outline}\n"
    });
    let mut needed = vec![("std::pcb", "Outline")];
    if !cutouts.is_empty() {
        needed.push(("std::pcb", "Cutout"));
    }
    if !holes.is_empty() {
        imports.push_str("import std::parts::mechanical::MountingHole\n");
        needed.push(("std::parts::mechanical", "MountingHole"));
    }

    let mut report = vec![
        format!(
            "units: {} ({})",
            outline.units.name(),
            if outline.units_from_file {
                "from the file's $INSUNITS"
            } else {
                "stated with --units"
            }
        ),
        format!("outline layer: {}", outline.outline_layers.join(", ")),
    ];
    if !outline.holes_layers.is_empty() {
        report.push(format!("holes layer: {}", outline.holes_layers.join(", ")));
    }
    report.push(format!(
        "frame: the board's origin is the outline's top left corner, the drawing's point ({}, {}) \
         (mm), with y flipped: a drawing point (x, y) is at (x - {}, {} - y) on the board",
        number(outline.origin.0),
        number(outline.origin.1),
        number(outline.origin.0),
        number(outline.origin.1),
    ));
    report.push(format!(
        "outline {} x {} mm{}; {} cutout{}; {} hole{}",
        number(outer.width),
        number(outer.height),
        if outer.radius > 5e-5 {
            format!(" with corner radius {} mm", number(outer.radius))
        } else {
            String::new()
        },
        cutouts.len(),
        if cutouts.len() == 1 { "" } else { "s" },
        holes.len(),
        if holes.len() == 1 { "" } else { "s" },
    ));
    Ok(Imported {
        report,
        imports,
        needed,
        members,
        names,
        warnings: outline.warnings,
    })
}

/// What `dxf import` prints for a person to paste.
pub fn render(imported: &Imported, board: Option<&str>, file: &str, written: bool) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "// Imported from {file} by `netsmith dxf import`.");
    for line in &imported.report {
        let _ = writeln!(out, "// {line}");
    }
    let name = board.unwrap_or("YourBoard");
    let _ = writeln!(
        out,
        "\n// in the physical file beside the board's source, `<stem>.physical.nsm` (or give `--write <file.nsm>` to put it there):"
    );
    out.push_str(&imported.imports);
    let _ = writeln!(out, "\nphysical for {name} {{\n    case {{");
    for line in imported.members.lines() {
        if line.is_empty() {
            out.push('\n');
        } else {
            let _ = writeln!(out, "        {line}");
        }
    }
    out.push_str("    }\n}\n");
    if written {
        // The diff that follows shows what was written.
        return out;
    }
    out
}

/// The names of the boards a source file declares: `board Name {` at the start of a line.
fn boards_of(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("board "))
        .filter_map(|rest| {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

/// Put the outline, cutouts and holes into the `case { }` section of the physical file
/// beside `source` (creating the file, the unit or the section if they are not there), and
/// return the board's name and what changed as a diff. The members of an earlier import
/// (`outline`, `cutout1`, `hole1`...) and the lines that state their places are replaced; every
/// other line and every comment stays. `board` may be left out when the source has one board.
/// The source is read to find the board's name and never written.
pub fn write_physical(
    source: &Path,
    board: Option<&str>,
    imported: &Imported,
) -> Result<(String, String), String> {
    let text = std::fs::read_to_string(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let boards = boards_of(&text);
    let board = match (board, boards.as_slice()) {
        (Some(b), _) if boards.iter().any(|x| x == b) => b.to_string(),
        (Some(b), _) => {
            return Err(format!(
                "{} has no board `{b}`; its boards: {}",
                source.display(),
                boards.join(", ")
            ));
        }
        (None, [one]) => one.clone(),
        (None, _) => {
            return Err(format!(
                "{} has {} boards ({}); say which with --board NAME",
                source.display(),
                boards.len(),
                boards.join(", ")
            ));
        }
    };
    let path = crate::physical::physical_path(source);
    let shown = path.display().to_string();
    let old = match std::fs::read_to_string(&path) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{shown}: {e}")),
    };
    let new = crate::physical::edit(
        old.as_deref(),
        &Edit {
            board: &board,
            section: Section::Case,
            remove: &imported.names,
            text: &imported.members,
            imports: &imported.needed,
        },
    )
    .map_err(|e| format!("{shown}: {e}"))?;
    let before = old.clone().unwrap_or_default();
    if new == before {
        return Ok((board, format!("{shown}: no change\n")));
    }
    std::fs::write(&path, &new).map_err(|e| format!("{shown}: {e}"))?;
    Ok((board, diff(&shown, &before, &new)))
}

/// A line diff of `old` to `new`: lines only in `old` start with `-`, only in `new` with `+`.
pub fn diff(name: &str, old: &str, new: &str) -> String {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    // The longest common subsequence, by dynamic programming: the files are small.
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut out = format!("--- {name} (before)\n+++ {name} (after)\n");
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            let _ = writeln!(out, " {}", a[i]);
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            let _ = writeln!(out, "+{}", b[j]);
            j += 1;
        } else {
            let _ = writeln!(out, "-{}", a[i]);
            i += 1;
        }
    }
    out
}
