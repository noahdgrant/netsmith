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
//! Nothing is guessed: a shape or a field this reader cannot read is an error that names it, and
//! a table that lacks a part, a `must_match` or a count of the outline's elements is no table.

use std::fmt::Write as _;

use netsmith_kicad::sexp::Sexp;

/// The parts a person decided must match the reference exactly (a checker fails on them): the
/// RP2350A Minimal board's, which a table has when no [`MustMatch`] list is given.
pub const MUST_MATCH: [&str; 10] = [
    "J1", "J2", "J3", "J4", "SW1", "SW2", "MH1", "MH2", "MH3", "MH4",
];

/// The reference designators a board's table marks `must_match = true`. The outline and every
/// hole are always marked; a part is marked when its designator is in the list. A board's own
/// list is a TOML file of the example (`reference/must_match.toml`) with one key, `parts`:
///
/// ```toml
/// parts = ["J1", "SW1", "H1"]
/// ```
///
/// The default list is [`MUST_MATCH`], so the table of a board without a list is as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MustMatch {
    parts: Vec<String>,
    /// `false` for the default list: the header then keeps the text it always had.
    custom: bool,
}

impl Default for MustMatch {
    fn default() -> MustMatch {
        MustMatch {
            parts: MUST_MATCH.iter().map(|s| s.to_string()).collect(),
            custom: false,
        }
    }
}

impl MustMatch {
    /// The list of a `must_match.toml`. A key it does not know, a designator written twice and a
    /// list that is empty are errors: a typo must not drop a part from the must-match set.
    pub fn from_toml(text: &str) -> Result<MustMatch, String> {
        let table: toml::Table = text
            .parse()
            .map_err(|e| format!("the must-match list: {e}"))?;
        if let Some(key) = table.keys().find(|k| k.as_str() != "parts") {
            return Err(format!(
                "the must-match list has the key {key}; only `parts` is read"
            ));
        }
        let list = table
            .get("parts")
            .and_then(toml::Value::as_array)
            .ok_or("the must-match list has no `parts = [...]`")?;
        let mut parts: Vec<String> = Vec::new();
        for item in list {
            let name = item
                .as_str()
                .ok_or("the must-match list: a part is not a text")?;
            if parts.iter().any(|p| p == name) {
                return Err(format!("the must-match list names {name} twice"));
            }
            parts.push(name.to_string());
        }
        if parts.is_empty() {
            return Err("the must-match list is empty".to_string());
        }
        Ok(MustMatch {
            parts,
            custom: true,
        })
    }

    pub fn contains(&self, reference: &str) -> bool {
        self.parts.iter().any(|p| p == reference)
    }
}

/// The table of `pcb` (the text of a `.kicad_pcb`); `source` is the file's name, written in the
/// header. The must-match parts are the RP2350A Minimal board's ([`MUST_MATCH`]); another
/// board's list goes to [`placements_toml_with`].
pub fn placements_toml(pcb: &str, source: &str) -> Result<String, String> {
    placements_toml_with(pcb, source, &MustMatch::default())
}

/// The table of `pcb` with the must-match parts of `must` (see [`MustMatch`]). A designator of
/// the list that the board does not have is an error (the default list is not held against the
/// board).
pub fn placements_toml_with(pcb: &str, source: &str, must: &MustMatch) -> Result<String, String> {
    let root = Sexp::parse(pcb).map_err(|e| format!("{source}: {e}"))?;
    if root.head() != Some("kicad_pcb") {
        return Err(format!("{source}: not a kicad_pcb file"));
    }
    let Board {
        outline,
        holes,
        parts,
    } = board_of(&root, source)?;
    // The default list is a board's own (the RP2350A Minimal's) and is not held against another
    // file; a list a board states must name only parts that it has.
    if must.custom
        && let Some(missing) = must
            .parts
            .iter()
            .find(|m| !parts.iter().any(|p| &p.reference == *m))
    {
        return Err(format!(
            "{source}: the must-match list names {missing}, which the board does not have"
        ));
    }

    let mut out = String::new();
    out.push_str(HEADER);
    out.push_str(if must.custom {
        MUST_MATCH_LIST_PARAGRAPH
    } else {
        MUST_MATCH_DEFAULT_PARAGRAPH
    });
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

    for hole in &holes {
        let _ = writeln!(out, "\n[[hole]]");
        let _ = writeln!(out, "must_match = true");
        let _ = writeln!(out, "reference = {}", quote(&hole.reference));
        let _ = writeln!(out, "x = {}", num(hole.x));
        let _ = writeln!(out, "y = {}", num(hole.y));
        let _ = writeln!(out, "drill = {}", num(hole.drill));
        let _ = writeln!(out, "size = [{}, {}]", num(hole.size.0), num(hole.size.1));
        if let Some(net) = &hole.net {
            let _ = writeln!(out, "net = {}", quote(net));
        }
        if hole.plated {
            let _ = writeln!(out, "plated = true");
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
        let _ = writeln!(out, "must_match = {}", must.contains(&part.reference));
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
#   points and rounded to 0.1 micrometre. [[hole]]: a non-plated pad (np_thru_hole), or the
#   drilled pad of a plated MountingHole footprint, its place on the board, its drill diameter,
#   its pad's size [width, height] and its net (left out for none). [[part]]: every footprint,
#   sorted by reference: its name, layer, position and rotation (0 when the file has none). A pad's x
#   and y are in the footprint's own frame (before its rotation); `angle` is the pad's own
#   angle as the file writes it (it includes the footprint's rotation) and is left out when the
#   file has none; `size` is [width, height]; `drill` is the diameter of a round hole, and
#   `drill_slot` the [width, height] of an oval one; `extent` (a custom pad only) is the box
#   [min_x, min_y, max_x, max_y] that its polygons, lines, arcs and circles span, in the pad's
#   own frame and with each stroke's half width added; `layers` are the pad's copper layers as
#   the file writes them (a footprint on the back has them on the other face already); `net` is
#   the net's name and is left out for a pad no net names.
";

const MUST_MATCH_DEFAULT_PARAGRAPH: &str = "\
# must_match = true marks what a copy of the board must reproduce (the outline, the holes,
#   J1 to J4, SW1, SW2); false is reported, not required.
";

const MUST_MATCH_LIST_PARAGRAPH: &str = "\
# must_match = true marks what a copy of the board must reproduce (the outline, the holes and the
#   parts of the board's must-match list); false is reported, not required.
# A [[hole]] with plated = true is the drilled pad (the one with the largest drill) of a
#   MountingHole footprint of KiCad's library, which draws its hole as a plated pad.
";

/// What the table holds, as numbers: read from a `.kicad_pcb` by [`read_board`], or from a table
/// (`placements.toml`) by [`Board::from_table`]. The parity checker compares two of them.
#[derive(Debug, Clone)]
pub struct Board {
    pub outline: Outline,
    /// The holes: the non-plated ones (`np_thru_hole` pads with a round drill) and the drilled
    /// pad of each plated `MountingHole` footprint, by place on the board.
    pub holes: Vec<Hole>,
    /// Every footprint, sorted by reference (letters, then the number).
    pub parts: Vec<Part>,
}

/// A hole: the reference of its footprint, its place on the board, its drill, its pad's size
/// and the pad's net.
#[derive(Debug, Clone)]
pub struct Hole {
    pub reference: String,
    pub x: f64,
    pub y: f64,
    pub drill: f64,
    /// The drilled pad's width and height (the pad of a plated mounting hole is larger than its
    /// drill; a non-plated one is usually the drill's size).
    pub size: (f64, f64),
    /// The pad's net, if the file gives it one.
    pub net: Option<String>,
    /// `true` for the drilled pad of a `MountingHole` footprint that draws its hole plated
    /// (`thru_hole`), as KiCad's `MountingHole_*_Pad*` footprints do; `false` for an
    /// `np_thru_hole` pad.
    pub plated: bool,
}

/// The board as a `.kicad_pcb` states it.
pub fn read_board(pcb: &str, source: &str) -> Result<Board, String> {
    let root = Sexp::parse(pcb).map_err(|e| format!("{source}: {e}"))?;
    if root.head() != Some("kicad_pcb") {
        return Err(format!("{source}: not a kicad_pcb file"));
    }
    board_of(&root, source)
}

fn board_of(root: &Sexp, source: &str) -> Result<Board, String> {
    let outline = outline(root)?;
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
    let mut holes = Vec::new();
    for part in &parts {
        for pad in part.pads.iter().filter(|p| p.kind == "np_thru_hole") {
            let Some(drill) = pad.drill else { continue };
            let (x, y) = part.board_point(pad.x, pad.y);
            holes.push(Hole {
                reference: part.reference.clone(),
                x,
                y,
                drill,
                size: pad.size.unwrap_or((drill, drill)),
                net: pad.net.clone(),
                plated: false,
            });
        }
        if let Some(hole) = plated_mounting_hole(part) {
            holes.push(hole);
        }
    }
    Ok(Board {
        outline,
        holes,
        parts,
    })
}

/// The hole of a `MountingHole` footprint of KiCad's library that has no `np_thru_hole` pad: the
/// `MountingHole_*_Pad` family draws it as a plated `thru_hole` pad (with a ring of vias around
/// it, all numbered like it), so the hole is the pad with the largest drill, and the vias are
/// not holes of their own. A footprint of another name, or with a non-plated pad, is not read
/// this way: nothing is guessed.
fn plated_mounting_hole(part: &Part) -> Option<Hole> {
    let library_name = part.footprint.rsplit(':').next().unwrap_or(&part.footprint);
    if !library_name.starts_with("MountingHole")
        || part.pads.iter().any(|p| p.kind == "np_thru_hole")
    {
        return None;
    }
    let pad = part
        .pads
        .iter()
        .filter(|p| p.kind == "thru_hole" && p.drill.is_some())
        .max_by(|a, b| {
            a.drill
                .partial_cmp(&b.drill)
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
    let (x, y) = part.board_point(pad.x, pad.y);
    Some(Hole {
        reference: part.reference.clone(),
        x,
        y,
        drill: pad.drill?,
        size: pad.size.unwrap_or((pad.drill?, pad.drill?)),
        net: pad.net.clone(),
        plated: true,
    })
}

impl Board {
    /// The board of a table as `placements_toml` prints it.
    pub fn from_table(text: &str) -> Result<Board, String> {
        let table: toml::Table = text.parse().map_err(|e| format!("the table: {e}"))?;
        let outline = table
            .get("outline")
            .and_then(toml::Value::as_table)
            .ok_or("the table has no [outline]")?;
        let count = |key: &str| -> Result<usize, String> {
            outline
                .get(key)
                .and_then(toml::Value::as_integer)
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| format!("the table's [outline] has no whole number {key}"))
        };
        let arcs = count("arcs")?;
        let lines = count("lines")?;
        let radii = if let Some(r) = outline.get("corner_radius") {
            vec![value(r, "outline corner_radius")?; arcs]
        } else if let Some(list) = outline.get("arc_radii").and_then(toml::Value::as_array) {
            list.iter()
                .map(|r| value(r, "outline arc_radii"))
                .collect::<Result<_, _>>()?
        } else if arcs == 0 {
            Vec::new()
        } else {
            return Err(
                "the table's [outline] has arcs but no corner_radius or arc_radii".to_string(),
            );
        };
        if radii.len() != arcs {
            return Err(format!(
                "the table's [outline] has {arcs} arcs and {} radii",
                radii.len()
            ));
        }
        let outline = Outline {
            min_x: field(outline, "min_x", "outline")?,
            min_y: field(outline, "min_y", "outline")?,
            max_x: field(outline, "max_x", "outline")?,
            max_y: field(outline, "max_y", "outline")?,
            lines,
            radii,
        };
        let mut holes = Vec::new();
        for h in array_of(&table, "hole") {
            let size = match h
                .get("size")
                .and_then(toml::Value::as_array)
                .map(Vec::as_slice)
            {
                Some([w, hh]) => (value(w, "a [[hole]] size")?, value(hh, "a [[hole]] size")?),
                _ => return Err("a [[hole]] has no size = [width, height]".to_string()),
            };
            holes.push(Hole {
                reference: text_field(h, "reference", "a [[hole]]")?,
                x: field(h, "x", "a [[hole]]")?,
                y: field(h, "y", "a [[hole]]")?,
                drill: field(h, "drill", "a [[hole]]")?,
                size,
                net: h
                    .get("net")
                    .and_then(toml::Value::as_str)
                    .map(str::to_string),
                plated: h.get("plated").and_then(toml::Value::as_bool) == Some(true),
            });
        }
        let mut parts = Vec::new();
        for p in array_of(&table, "part") {
            let reference = text_field(p, "reference", "a [[part]]")?;
            let at = format!("part {reference}");
            let mut pads = Vec::new();
            for pad in p
                .get("pads")
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
            {
                let pad = pad
                    .as_table()
                    .ok_or_else(|| format!("{at}: a pad is not a table"))?;
                let number = text_field(pad, "number", &at)?;
                let pair = |key: &str| -> Result<Option<(f64, f64)>, String> {
                    let Some(v) = pad.get(key) else {
                        return Ok(None);
                    };
                    match v.as_array().map(Vec::as_slice) {
                        Some([w, h]) => Ok(Some((value(w, key)?, value(h, key)?))),
                        _ => Err(format!("{at} pad {number}: {key} is not [w, h]")),
                    }
                };
                pads.push(Pad {
                    kind: text_field(pad, "type", &at)?,
                    shape: text_field(pad, "shape", &at)?,
                    x: field(pad, "x", &at)?,
                    y: field(pad, "y", &at)?,
                    angle: pad.get("angle").map(|v| value(v, "angle")).transpose()?,
                    size: pair("size")?,
                    drill: pad.get("drill").map(|v| value(v, "drill")).transpose()?,
                    drill_slot: pair("drill_slot")?,
                    extent: match pad.get("extent") {
                        None => None,
                        Some(v) => match v.as_array().map(Vec::as_slice) {
                            Some([a, b, c, d]) => Some([
                                value(a, "extent")?,
                                value(b, "extent")?,
                                value(c, "extent")?,
                                value(d, "extent")?,
                            ]),
                            _ => {
                                return Err(format!(
                                    "{at} pad {number}: extent is not [min_x, min_y, max_x, max_y]"
                                ));
                            }
                        },
                    },
                    layers: pad
                        .get("layers")
                        .and_then(toml::Value::as_array)
                        .and_then(|l| {
                            l.iter()
                                .map(|v| v.as_str().map(str::to_string))
                                .collect::<Option<Vec<_>>>()
                        })
                        .ok_or_else(|| {
                            format!(
                                "{at} pad {number}: no layers (a list of names): a table lacks it"
                            )
                        })?,
                    net: pad
                        .get("net")
                        .and_then(toml::Value::as_str)
                        .map(str::to_string),
                    number,
                });
            }
            parts.push(Part {
                footprint: text_field(p, "footprint", &at)?,
                layer: text_field(p, "layer", &at)?,
                x: field(p, "x", &at)?,
                y: field(p, "y", &at)?,
                rotation: field(p, "rotation", &at)?,
                must_match: p
                    .get("must_match")
                    .and_then(toml::Value::as_bool)
                    .ok_or_else(|| {
                        format!("{at} has no must_match (true or false): a table lacks it")
                    })?,
                reference,
                pads,
            });
        }
        if parts.is_empty() {
            return Err("the table has no [[part]]: it is cut short".to_string());
        }
        Ok(Board {
            outline,
            holes,
            parts,
        })
    }
}

fn array_of<'a>(table: &'a toml::Table, key: &str) -> impl Iterator<Item = &'a toml::Table> {
    table
        .get(key)
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_table)
}

fn value(v: &toml::Value, what: &str) -> Result<f64, String> {
    match v {
        toml::Value::Float(f) => Ok(*f),
        toml::Value::Integer(i) => Ok(*i as f64),
        _ => Err(format!("{what} is not a number")),
    }
}

fn field(t: &toml::Table, key: &str, whose: &str) -> Result<f64, String> {
    value(
        t.get(key).ok_or_else(|| format!("{whose} has no {key}"))?,
        &format!("{whose}: {key}"),
    )
}

fn text_field(t: &toml::Table, key: &str, whose: &str) -> Result<String, String> {
    t.get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{whose} has no text {key}"))
}

#[derive(Debug, Clone)]
pub struct Outline {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub lines: usize,
    /// The radius of each arc, rounded to 0.1 micrometre.
    pub radii: Vec<f64>,
}

impl Outline {
    /// The one radius of every arc, when they agree.
    pub fn corner_radius(&self) -> Option<f64> {
        let first = *self.radii.first()?;
        self.radii
            .iter()
            .all(|r| (r - first).abs() < 1e-3)
            .then_some(first)
    }
}

/// A footprint's place on the board, for the shapes it draws: `(at x y [angle])`.
#[derive(Clone, Copy)]
struct Frame<'a> {
    reference: &'a str,
    x: f64,
    y: f64,
    rotation: f64,
}

impl Frame<'_> {
    fn board_point(&self, x: f64, y: f64) -> (f64, f64) {
        place(self.x, self.y, self.rotation, x, y)
    }
}

/// What the `Edge.Cuts` shapes add up to, point by point.
struct Edge {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    lines: usize,
    radii: Vec<f64>,
}

impl Edge {
    fn take(&mut self, (x, y): (f64, f64)) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    /// One shape on `Edge.Cuts`: a board's `gr_*` or a footprint's `fp_*` (`owner`). A footprint's
    /// shape is in the footprint's own frame: its points are placed on the board first.
    fn add(&mut self, item: &Sexp, head: &str, owner: Option<Frame>) -> Result<(), String> {
        let at = |p: (f64, f64)| owner.map_or(p, |f| f.board_point(p.0, p.1));
        let kind = &head[3..];
        match kind {
            "line" => {
                self.lines += 1;
                for key in ["start", "end"] {
                    self.take(at(point(item, key)?));
                }
            }
            "rect" => {
                self.lines += 4;
                let (a, b) = (point(item, "start")?, point(item, "end")?);
                for corner in [a, (b.0, a.1), b, (a.0, b.1)] {
                    self.take(at(corner));
                }
            }
            "poly" => {
                let pts = item
                    .find("pts")
                    .ok_or_else(|| format!("a {head} has no (pts ...)"))?;
                let mut count = 0;
                for node in pts.args() {
                    let ([x, y], Some("xy")) = (
                        numbers(node)[..].try_into().unwrap_or([f64::NAN; 2]),
                        node.head(),
                    ) else {
                        return Err(format!(
                            "a {head} has a {} in its (pts ...): only (xy x y) points are read",
                            node.head().unwrap_or("?")
                        ));
                    };
                    count += 1;
                    self.take(at((x, y)));
                }
                if count < 3 {
                    return Err(format!("a {head} has {count} points: it is no outline"));
                }
                // A polygon is closed: as many edges as points.
                self.lines += count;
            }
            "arc" => {
                let (start, mid, end) = (
                    at(point(item, "start")?),
                    at(point(item, "mid")?),
                    at(point(item, "end")?),
                );
                let arc = Arc::through(start, mid, end)?;
                self.radii.push(arc.radius);
                self.take(start);
                self.take(end);
                for p in arc.extremes() {
                    self.take(p);
                }
            }
            _ => {
                let whose = owner.map_or(String::new(), |f| format!(" of {}", f.reference));
                return Err(format!(
                    "an Edge.Cuts {head}{whose} is not read (lines, rectangles, polygons and \
                     arcs are)"
                ));
            }
        }
        Ok(())
    }
}

/// The outline: every shape on `Edge.Cuts` that the board draws (`gr_*`) or a footprint draws
/// (`fp_*`, placed on the board by the footprint's position and rotation: a development-board
/// footprint can carry the outline of the board it stands for).
fn outline(root: &Sexp) -> Result<Outline, String> {
    let mut edge = Edge {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
        lines: 0,
        radii: Vec::new(),
    };
    let on_edge = |item: &Sexp| item.find("layer").and_then(first_text) == Some("Edge.Cuts");
    for item in root.args() {
        match item.head() {
            Some(head) if head.starts_with("gr_") && on_edge(item) => {
                edge.add(item, head, None)?;
            }
            Some("footprint") => {
                let frame = frame_of(item)?;
                for shape in item.args() {
                    match shape.head() {
                        Some(head) if head.starts_with("fp_") && on_edge(shape) => {
                            let owner = Frame {
                                reference: &frame.0,
                                x: frame.1,
                                y: frame.2,
                                rotation: frame.3,
                            };
                            edge.add(shape, head, Some(owner))?;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    if !edge.min_x.is_finite() {
        return Err("the board has no Edge.Cuts lines or arcs".to_string());
    }
    // An arc is stored by its three points, written to five decimals: its radius and the extent
    // of its curve are worked out from them, so they are rounded to four (0.1 micrometre).
    let round = |v: f64| (v * 1e4).round() / 1e4;
    Ok(Outline {
        min_x: round(edge.min_x),
        min_y: round(edge.min_y),
        max_x: round(edge.max_x),
        max_y: round(edge.max_y),
        lines: edge.lines,
        radii: edge.radii.into_iter().map(round).collect(),
    })
}

/// A footprint's reference, position and rotation.
fn frame_of(footprint: &Sexp) -> Result<(String, f64, f64, f64), String> {
    let name = footprint.args().first().and_then(Sexp::text).unwrap_or("");
    let reference = footprint
        .find_all("property")
        .find(|p| p.args().first().and_then(Sexp::text) == Some("Reference"))
        .and_then(|p| p.args().get(1))
        // KiCad 8 and later: `(property "Reference" "R1" ...)`. KiCad 7 and earlier (format
        // 20221018 and before): `(fp_text reference "R1" ...)`.
        .or_else(|| {
            footprint
                .find_all("fp_text")
                .find(|t| t.args().first().and_then(Sexp::text) == Some("reference"))
                .and_then(|t| t.args().get(1))
        })
        .and_then(Sexp::text)
        .ok_or_else(|| {
            format!("the footprint {name} has no Reference (neither a property nor an fp_text)")
        })?
        .to_string();
    let at = footprint
        .find("at")
        .ok_or_else(|| format!("{reference}: no (at ...)"))?;
    match numbers(at)[..] {
        [x, y] => Ok((reference, x, y, 0.0)),
        [x, y, r] => Ok((reference, x, y, r)),
        _ => Err(format!("{reference}: (at ...) is not x y [angle]")),
    }
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

#[derive(Debug, Clone)]
pub struct Part {
    pub reference: String,
    pub footprint: String,
    pub layer: String,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    /// Set by a table; a board read from a `.kicad_pcb` has `false` (see [`MUST_MATCH`]).
    pub must_match: bool,
    pub pads: Vec<Pad>,
}

impl Part {
    /// A point of the footprint's own frame, on the board (KiCad's rotation, y down).
    pub fn board_point(&self, x: f64, y: f64) -> (f64, f64) {
        place(self.x, self.y, self.rotation, x, y)
    }
}

/// The board point of a point `(x, y)` of a footprint at `(at_x, at_y)` turned by `rotation`.
/// A footprint on the back keeps its pads and shapes as the file writes them: KiCad stores them
/// already mirrored, so the same turn applies.
fn place(at_x: f64, at_y: f64, rotation: f64, x: f64, y: f64) -> (f64, f64) {
    let (s, c) = rotation.to_radians().sin_cos();
    // Whole turns are exact: a part turned 90 or 180 degrees keeps its figures round.
    let (s, c) = (snap(s), snap(c));
    (at_x + x * c + y * s, at_y + y * c - x * s)
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

#[derive(Debug, Clone)]
pub struct Pad {
    pub number: String,
    /// `smd`, `thru_hole`, `np_thru_hole` or `connect`.
    pub kind: String,
    pub shape: String,
    /// The centre in the footprint's own frame (before its rotation).
    pub x: f64,
    pub y: f64,
    pub angle: Option<f64>,
    pub size: Option<(f64, f64)>,
    pub drill: Option<f64>,
    pub drill_slot: Option<(f64, f64)>,
    /// A custom pad's box `[min_x, min_y, max_x, max_y]` over its primitives, in the pad's own
    /// frame (before its angle), rounded to 0.1 micrometre.
    pub extent: Option<[f64; 4]>,
    /// The copper layers of the pad's `(layers ...)` as the file writes them (`F.Cu`, `B.Cu`,
    /// `*.Cu`, ...), in its order; the mask, paste and other layers are left out. A footprint on
    /// the back has its pads' layers already moved to the other face.
    pub layers: Vec<String>,
    pub net: Option<String>,
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
        if let Some(e) = self.extent {
            let _ = write!(
                s,
                ", extent = [{}, {}, {}, {}]",
                num(e[0]),
                num(e[1]),
                num(e[2]),
                num(e[3])
            );
        }
        let layers: Vec<String> = self.layers.iter().map(|l| quote(l)).collect();
        let _ = write!(s, ", layers = [{}]", layers.join(", "));
        if let Some(net) = &self.net {
            let _ = write!(s, ", net = {}", quote(net));
        }
        s.push_str(" }");
        s
    }
}

fn part(footprint: &Sexp) -> Result<Part, String> {
    let name = footprint.args().first().and_then(Sexp::text).unwrap_or("");
    let (reference, x, y, rotation) = frame_of(footprint)?;
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
        must_match: false,
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
        .filter(|n| !n.is_empty())
        .map(str::to_string);
    let layers = pad
        .find("layers")
        .map(|l| {
            l.args()
                .iter()
                .filter_map(Sexp::text)
                .filter(|l| l.ends_with(".Cu"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let extent = if shape == "custom" {
        primitives_extent(pad).map_err(|e| format!("pad {number}: {e}"))?
    } else {
        None
    };
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
        extent,
        layers,
        net,
    })
}

/// The box `[min_x, min_y, max_x, max_y]` over a custom pad's `(primitives ...)`, each stroke's
/// half width added. A curve is not read (its box is not its points'), and nothing is guessed.
fn primitives_extent(pad: &Sexp) -> Result<Option<[f64; 4]>, String> {
    let Some(block) = pad.find("primitives") else {
        return Ok(None);
    };
    let mut e = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut take = |x: f64, y: f64, grow: f64| {
        e[0] = e[0].min(x - grow);
        e[1] = e[1].min(y - grow);
        e[2] = e[2].max(x + grow);
        e[3] = e[3].max(y + grow);
    };
    for item in block.args() {
        let Some(head) = item.head() else { continue };
        let grow = numbers_of(item, "width").first().copied().unwrap_or(0.0) / 2.0;
        match head {
            "gr_line" | "gr_rect" => {
                for key in ["start", "end"] {
                    let (x, y) = point(item, key)?;
                    take(x, y, grow);
                }
            }
            "gr_circle" => {
                let (cx, cy) = point(item, "center")?;
                let (ex, ey) = point(item, "end")?;
                let r = (ex - cx).hypot(ey - cy);
                take(cx - r, cy - r, grow);
                take(cx + r, cy + r, grow);
            }
            "gr_arc" => {
                let (start, mid, end) = (
                    point(item, "start")?,
                    point(item, "mid")?,
                    point(item, "end")?,
                );
                let arc = Arc::through(start, mid, end)?;
                take(start.0, start.1, grow);
                take(end.0, end.1, grow);
                for (x, y) in arc.extremes() {
                    take(x, y, grow);
                }
            }
            "gr_poly" => {
                let pts = item
                    .find("pts")
                    .ok_or("a gr_poly has no (pts ...)".to_string())?;
                for xy in pts.find_all("xy") {
                    let [x, y] = numbers(xy)[..] else {
                        return Err("a gr_poly point is not (xy x y)".to_string());
                    };
                    take(x, y, grow);
                }
            }
            other => {
                return Err(format!(
                    "the primitive {other} of a custom pad is not read (lines, rectangles, \
                     circles, arcs and polygons are)"
                ));
            }
        }
    }
    if !e[0].is_finite() {
        return Ok(None);
    }
    let round = |v: f64| (v * 1e4).round() / 1e4;
    Ok(Some(e.map(round)))
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
        _ => Err(format!("a drawn item has no ({key} x y)")),
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
pub fn natural_key(reference: &str) -> (String, u64, String) {
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
