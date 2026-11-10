//! A reference table of facts read from a KiCad `.kicad_pcb`: the board outline, the mounting
//! holes and, for every footprint, where it is and what its pads are.
//!
//! It exists for the RP2350A Minimal port (`examples/rp2350_minimal/`), whose parity checker
//! compares a generated board with Raspberry Pi's. The program is
//! `cargo run -p netsmith --example reference_table -- <file.kicad_pcb>`; it prints the table
//! (TOML) to standard output. Every number in the table is a number the file states, except
//! the outline's bounding box and corner radius, which are computed from the `Edge.Cuts`
//! lines and arcs, and the position of a hole, which is its pad's place on the board.
//!
//! Nothing is guessed: a shape or a field this reader cannot read is an error that names it.

use std::fmt::Write as _;

use netsmith_kicad::sexp::Sexp;

/// The parts a person decided must match the reference exactly (a checker fails on them).
pub const MUST_MATCH: [&str; 10] = [
    "J1", "J2", "J3", "J4", "SW1", "SW2", "MH1", "MH2", "MH3", "MH4",
];

/// The table of `pcb` (the text of a `.kicad_pcb`); `source` is the file's name, written in the
/// header.
pub fn placements_toml(pcb: &str, source: &str) -> Result<String, String> {
    let root = Sexp::parse(pcb).map_err(|e| format!("{source}: {e}"))?;
    if root.head() != Some("kicad_pcb") {
        return Err(format!("{source}: not a kicad_pcb file"));
    }
    let outline = outline(&root)?;
    let mut parts = root
        .find_all("footprint")
        .map(part)
        .collect::<Result<Vec<_>, _>>()?;
    parts.sort_by_key(|p| natural_key(&p.reference));
    for pair in parts.windows(2) {
        if pair[0].reference == pair[1].reference {
            return Err(format!(
                "{source}: two footprints are called {}",
                pair[0].reference
            ));
        }
    }

    let mut out = String::new();
    out.push_str(HEADER);
    let _ = writeln!(out, "\n[source]");
    let _ = writeln!(out, "file = {}", quote(source));
    for (key, value) in [
        ("version", root.find("version").and_then(first_text)),
        ("generator", root.find("generator").and_then(first_text)),
        (
            "generator_version",
            root.find("generator_version").and_then(first_text),
        ),
    ] {
        if let Some(value) = value {
            let _ = writeln!(out, "{key} = {}", quote(value));
        }
    }
    if let Some(title) = root.find("title_block") {
        for key in ["title", "date", "rev"] {
            if let Some(value) = title.find(key).and_then(first_text) {
                let _ = writeln!(out, "{key} = {}", quote(value));
            }
        }
    }
    let _ = writeln!(out, "footprints = {}", parts.len());

    let _ = writeln!(out, "\n[outline]");
    let _ = writeln!(out, "must_match = true");
    let _ = writeln!(out, "min_x = {}", num(outline.min_x));
    let _ = writeln!(out, "min_y = {}", num(outline.min_y));
    let _ = writeln!(out, "max_x = {}", num(outline.max_x));
    let _ = writeln!(out, "max_y = {}", num(outline.max_y));
    let _ = writeln!(out, "width = {}", num(outline.max_x - outline.min_x));
    let _ = writeln!(out, "height = {}", num(outline.max_y - outline.min_y));
    let _ = writeln!(out, "lines = {}", outline.lines);
    let _ = writeln!(out, "arcs = {}", outline.radii.len());
    match outline.corner_radius() {
        Some(r) => {
            let _ = writeln!(out, "corner_radius = {}", num(r));
        }
        None if outline.radii.is_empty() => {}
        None => {
            let radii: Vec<String> = outline.radii.iter().map(|r| num(*r)).collect();
            let _ = writeln!(
                out,
                "# the arcs differ in radius, so there is no corner_radius"
            );
            let _ = writeln!(out, "arc_radii = [{}]", radii.join(", "));
        }
    }

    for part in &parts {
        for pad in part.pads.iter().filter(|p| p.kind == "np_thru_hole") {
            let Some(drill) = pad.drill else { continue };
            let (x, y) = part.board_point(pad.x, pad.y);
            let _ = writeln!(out, "\n[[hole]]");
            let _ = writeln!(out, "must_match = true");
            let _ = writeln!(out, "reference = {}", quote(&part.reference));
            let _ = writeln!(out, "x = {}", num(x));
            let _ = writeln!(out, "y = {}", num(y));
            let _ = writeln!(out, "drill = {}", num(drill));
        }
    }

    for part in &parts {
        let _ = writeln!(out, "\n[[part]]");
        let _ = writeln!(out, "reference = {}", quote(&part.reference));
        let _ = writeln!(out, "footprint = {}", quote(&part.footprint));
        let _ = writeln!(out, "layer = {}", quote(&part.layer));
        let _ = writeln!(out, "x = {}", num(part.x));
        let _ = writeln!(out, "y = {}", num(part.y));
        let _ = writeln!(out, "rotation = {}", num(part.rotation));
        let _ = writeln!(
            out,
            "must_match = {}",
            MUST_MATCH.contains(&part.reference.as_str())
        );
        if part.pads.is_empty() {
            let _ = writeln!(out, "pads = []");
            continue;
        }
        let _ = writeln!(out, "pads = [");
        for pad in &part.pads {
            let _ = writeln!(out, "    {},", pad.inline());
        }
        let _ = writeln!(out, "]");
    }
    Ok(out)
}

const HEADER: &str = "\
# Reference placements, read from a KiCad board file by
#   cargo run -p netsmith --example reference_table -- <file.kicad_pcb>
# Do not edit: run the program again. A test checks that this file is its output.
#
# Units are millimetres and degrees, as in the KiCad file. The origin is the file's own page
# origin, x grows to the right and y grows DOWN, as in KiCad's board editor. A rotation is
# KiCad's: positive turns counter-clockwise as seen on the screen, so with y down a point
# (x, y) of the footprint lands at (x cos a + y sin a, y cos a - x sin a) from its position.
#
# [outline]: the bounding box of the Edge.Cuts lines and arcs (the arcs' extent included) and,
#   when every arc has one radius, the corner radius; both are worked out from the drawn
#   points and rounded to 0.1 micrometre. [[hole]]: a non-plated pad (np_thru_hole),
#   its place on the board and its drill diameter. [[part]]: every footprint, sorted by
#   reference: its name, layer, position and rotation (0 when the file has none). A pad's x
#   and y are in the footprint's own frame (before its rotation); `angle` is the pad's own
#   angle as the file writes it (it includes the footprint's rotation) and is left out when the
#   file has none; `size` is [width, height]; `drill` is the diameter of a round hole, and
#   `drill_slot` the [width, height] of an oval one; `net` is the net's name and is left out
#   for a pad no net names.
# must_match = true marks what a copy of the board must reproduce (the outline, the holes,
#   J1 to J4, SW1, SW2); false is reported, not required.
";

struct Outline {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    lines: usize,
    radii: Vec<f64>,
}

impl Outline {
    fn corner_radius(&self) -> Option<f64> {
        let first = *self.radii.first()?;
        self.radii
            .iter()
            .all(|r| (r - first).abs() < 1e-3)
            .then_some(first)
    }
}

fn outline(root: &Sexp) -> Result<Outline, String> {
    let mut o = Outline {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
        lines: 0,
        radii: Vec::new(),
    };
    let mut take = |x: f64, y: f64| {
        o.min_x = o.min_x.min(x);
        o.min_y = o.min_y.min(y);
        o.max_x = o.max_x.max(x);
        o.max_y = o.max_y.max(y);
    };
    let mut lines = 0;
    let mut radii = Vec::new();
    for item in root.args() {
        let Some(head) = item.head() else { continue };
        if !head.starts_with("gr_") {
            continue;
        }
        let on_edge = item.find("layer").and_then(first_text) == Some("Edge.Cuts");
        if !on_edge {
            continue;
        }
        match head {
            "gr_line" => {
                lines += 1;
                for key in ["start", "end"] {
                    let (x, y) = point(item, key)?;
                    take(x, y);
                }
            }
            "gr_rect" => {
                lines += 4;
                for key in ["start", "end"] {
                    let (x, y) = point(item, key)?;
                    take(x, y);
                }
            }
            "gr_arc" => {
                let (start, mid, end) = (
                    point(item, "start")?,
                    point(item, "mid")?,
                    point(item, "end")?,
                );
                let arc = Arc::through(start, mid, end)?;
                radii.push(arc.radius);
                take(start.0, start.1);
                take(end.0, end.1);
                for (x, y) in arc.extremes() {
                    take(x, y);
                }
            }
            other => {
                return Err(format!(
                    "an Edge.Cuts {other} is not read (lines, rectangles and arcs are)"
                ));
            }
        }
    }
    if !o.min_x.is_finite() {
        return Err("the board has no Edge.Cuts lines or arcs".to_string());
    }
    // An arc is stored by its three points, written to five decimals: its radius and the extent
    // of its curve are worked out from them, so they are rounded to four (0.1 micrometre).
    let round = |v: f64| (v * 1e4).round() / 1e4;
    o.min_x = round(o.min_x);
    o.min_y = round(o.min_y);
    o.max_x = round(o.max_x);
    o.max_y = round(o.max_y);
    o.lines = lines;
    o.radii = radii.into_iter().map(round).collect();
    Ok(o)
}

/// An arc through three points: its centre and radius, the angle it starts at, and how far and
/// which way it turns (angles in the file's own coordinates, y down).
struct Arc {
    cx: f64,
    cy: f64,
    radius: f64,
    from: f64,
    sweep: f64,
    dir: f64,
}

impl Arc {
    fn through(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> Result<Arc, String> {
        let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
        if d.abs() < 1e-12 {
            return Err("an Edge.Cuts arc has its three points on a line".to_string());
        }
        let sq = |p: (f64, f64)| p.0 * p.0 + p.1 * p.1;
        let cx = (sq(a) * (b.1 - c.1) + sq(b) * (c.1 - a.1) + sq(c) * (a.1 - b.1)) / d;
        let cy = (sq(a) * (c.0 - b.0) + sq(b) * (a.0 - c.0) + sq(c) * (b.0 - a.0)) / d;
        let radius = ((a.0 - cx).powi(2) + (a.1 - cy).powi(2)).sqrt();
        let angle = |p: (f64, f64)| (p.1 - cy).atan2(p.0 - cx);
        let cross = (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0);
        let dir = if cross > 0.0 { 1.0 } else { -1.0 };
        let tau = std::f64::consts::TAU;
        let sweep = ((angle(c) - angle(a)) * dir).rem_euclid(tau);
        Ok(Arc {
            cx,
            cy,
            radius,
            from: angle(a),
            sweep,
            dir,
        })
    }

    /// The points of the circle at 0, 90, 180 and 270 degrees that lie on the arc.
    fn extremes(&self) -> Vec<(f64, f64)> {
        let tau = std::f64::consts::TAU;
        (0..4)
            .map(|k| f64::from(k) * std::f64::consts::FRAC_PI_2)
            .filter(|t| ((t - self.from) * self.dir).rem_euclid(tau) <= self.sweep + 1e-9)
            .map(|t| {
                (
                    self.cx + self.radius * t.cos(),
                    self.cy + self.radius * t.sin(),
                )
            })
            .collect()
    }
}

struct Part {
    reference: String,
    footprint: String,
    layer: String,
    x: f64,
    y: f64,
    rotation: f64,
    pads: Vec<Pad>,
}

impl Part {
    /// A point of the footprint's own frame, on the board (KiCad's rotation, y down).
    fn board_point(&self, x: f64, y: f64) -> (f64, f64) {
        let (s, c) = self.rotation.to_radians().sin_cos();
        // Whole turns are exact: a part turned 90 or 180 degrees keeps its figures round.
        let (s, c) = (snap(s), snap(c));
        (self.x + x * c + y * s, self.y + y * c - x * s)
    }
}

fn snap(v: f64) -> f64 {
    if v.abs() < 1e-12 {
        0.0
    } else if (v - 1.0).abs() < 1e-12 {
        1.0
    } else if (v + 1.0).abs() < 1e-12 {
        -1.0
    } else {
        v
    }
}

struct Pad {
    number: String,
    kind: String,
    shape: String,
    x: f64,
    y: f64,
    angle: Option<f64>,
    size: Option<(f64, f64)>,
    drill: Option<f64>,
    drill_slot: Option<(f64, f64)>,
    net: Option<String>,
}

impl Pad {
    fn inline(&self) -> String {
        let mut s = format!(
            "{{ number = {}, type = {}, shape = {}, x = {}, y = {}",
            quote(&self.number),
            quote(&self.kind),
            quote(&self.shape),
            num(self.x),
            num(self.y)
        );
        if let Some(a) = self.angle {
            let _ = write!(s, ", angle = {}", num(a));
        }
        if let Some((w, h)) = self.size {
            let _ = write!(s, ", size = [{}, {}]", num(w), num(h));
        }
        if let Some(d) = self.drill {
            let _ = write!(s, ", drill = {}", num(d));
        }
        if let Some((w, h)) = self.drill_slot {
            let _ = write!(s, ", drill_slot = [{}, {}]", num(w), num(h));
        }
        if let Some(net) = &self.net {
            let _ = write!(s, ", net = {}", quote(net));
        }
        s.push_str(" }");
        s
    }
}

fn part(footprint: &Sexp) -> Result<Part, String> {
    let name = footprint.args().first().and_then(Sexp::text).unwrap_or("");
    let reference = footprint
        .find_all("property")
        .find(|p| p.args().first().and_then(Sexp::text) == Some("Reference"))
        .and_then(|p| p.args().get(1))
        .and_then(Sexp::text)
        .ok_or_else(|| format!("the footprint {name} has no Reference property"))?
        .to_string();
    let at = footprint
        .find("at")
        .ok_or_else(|| format!("{reference}: no (at ...)"))?;
    let nums = numbers(at);
    let (x, y, rotation) = match nums[..] {
        [x, y] => (x, y, 0.0),
        [x, y, r] => (x, y, r),
        _ => return Err(format!("{reference}: (at ...) is not x y [angle]")),
    };
    let layer = footprint
        .find("layer")
        .and_then(first_text)
        .ok_or_else(|| format!("{reference}: no layer"))?
        .to_string();
    let pads = footprint
        .find_all("pad")
        .map(|p| pad(p).map_err(|e| format!("{reference}: {e}")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Part {
        reference,
        footprint: name.to_string(),
        layer,
        x,
        y,
        rotation,
        pads,
    })
}

fn pad(pad: &Sexp) -> Result<Pad, String> {
    let args = pad.args();
    let text = |i: usize| args.get(i).and_then(Sexp::text).map(str::to_string);
    let number = text(0).ok_or("a pad has no number")?;
    let kind = text(1).ok_or_else(|| format!("pad {number} has no type"))?;
    let shape = text(2).ok_or_else(|| format!("pad {number} has no shape"))?;
    let at = numbers(
        pad.find("at")
            .ok_or_else(|| format!("pad {number} has no (at ...)"))?,
    );
    let (x, y, angle) = match at[..] {
        [x, y] => (x, y, None),
        [x, y, a] => (x, y, Some(a)),
        _ => return Err(format!("pad {number}: (at ...) is not x y [angle]")),
    };
    let size = match numbers_of(pad, "size")[..] {
        [] => None,
        [w, h] => Some((w, h)),
        _ => return Err(format!("pad {number}: (size ...) is not w h")),
    };
    let (mut drill, mut drill_slot) = (None, None);
    if let Some(d) = pad.find("drill") {
        let oval = d.args().first().and_then(Sexp::text) == Some("oval");
        match (oval, &numbers(d)[..]) {
            (false, [d]) => drill = Some(*d),
            (true, [w, h]) => drill_slot = Some((*w, *h)),
            _ => return Err(format!("pad {number}: this (drill ...) is not read")),
        }
    }
    let net = pad
        .find("net")
        .and_then(|n| n.args().last())
        .and_then(Sexp::text)
        .map(str::to_string);
    Ok(Pad {
        number,
        kind,
        shape,
        x,
        y,
        angle,
        size,
        drill,
        drill_slot,
        net,
    })
}

fn first_text(node: &Sexp) -> Option<&str> {
    node.args().first().and_then(Sexp::text)
}

fn numbers(node: &Sexp) -> Vec<f64> {
    node.args()
        .iter()
        .filter_map(|a| a.text().and_then(|t| t.parse().ok()))
        .collect()
}

fn numbers_of(node: &Sexp, key: &str) -> Vec<f64> {
    node.find(key).map(numbers).unwrap_or_default()
}

fn point(node: &Sexp, key: &str) -> Result<(f64, f64), String> {
    match numbers_of(node, key)[..] {
        [x, y] => Ok((x, y)),
        _ => Err(format!("an Edge.Cuts item has no ({key} x y)")),
    }
}

/// A figure as the table writes it: six decimals at most, no trailing zeros.
fn num(n: f64) -> String {
    let mut s = format!("{n:.6}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" { "0".to_string() } else { s }
}

fn quote(text: &str) -> String {
    let mut s = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\t' => s.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(s, "\\u{:04x}", c as u32);
            }
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

/// `C10` after `C9`, and `J1` after `C99`: the letters, then the number.
fn natural_key(reference: &str) -> (String, u64, String) {
    let split = reference
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(reference.len());
    let (letters, rest) = reference.split_at(split);
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (
        letters.to_string(),
        digits.parse().unwrap_or(0),
        rest[digits.len()..].to_string(),
    )
}
