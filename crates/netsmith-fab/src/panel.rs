//! Panels (ADR 0040): one board repeated in a grid, cut apart by V-cuts.
//!
//! A panel is made from the finished `.kicad_pcb` of its board, so every Gerber, drill and
//! placement number is the board's own, moved. [`board_file`] writes the panel as a
//! `.kicad_pcb` of its own, in which each cell is the board's items translated by the cell's
//! offset (its nets and reference designators renamed so that each cell stands alone, its
//! outline replaced by one outline for the panel), the V-cut lines are drawn on `Edge.Cuts`
//! and marked "V-CUT" as JLCPCB asks, and the rails carry the tooling holes and fiducials.
//! [`build`] then runs the same writers as [`crate::package`] on that file, so KiCad can
//! plot the panel board too and the cross-check compares the two.
//!
//! Every limit comes from [`Rules`], which the language's `std::mfg::jlcpcb::VCutPanel`
//! states with its sources; no board house's number is written in this file. The one number
//! that is not a rule of the board house is the mask opening of a fiducial, which is the
//! project's own fiducial footprint (`Fiducial_1mm_Mask2mm`: an opening twice the copper).

use std::collections::BTreeMap;

use crate::artwork::Artwork;
use crate::board::{Board, BoardPart, Fit};
use crate::bom::{self, BomEntry};
use crate::sexp::{self, Sexp};
use crate::{DrcOutcome, Error, Options, Plan, cpl, drc, drill, edge, gerber, plot};

/// The mask opening round a fiducial, as a multiple of its copper, as the project's
/// fiducial footprint draws it.
const FIDUCIAL_OPENING: f64 = 2.0;
/// The size of the "V-CUT" mark, in mm, and where it sits beyond the outline.
const MARK_SIZE: f64 = 1.0;
const MARK_GAP: f64 = 1.5;

/// What the board house needs of a V-cut panel (`std::mfg::jlcpcb::VCutPanel`). Lengths are
/// in mm.
#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    pub fabricator: String,
    /// Where the numbers are published.
    pub source: String,
    pub min_rail: f64,
    pub min_size: f64,
    pub max_size: f64,
    pub max_assembled_size: f64,
    pub max_cuts: u32,
    pub min_cut_spacing: f64,
    pub advised_cut_spacing: f64,
    pub min_edge_clearance: f64,
    pub advised_edge_clearance: f64,
    pub min_thickness: f64,
    pub tooling_hole: f64,
    pub fiducial: f64,
    pub fiducial_offset: f64,
}

/// A panel as its `panel` definition states it.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub name: String,
    pub columns: u32,
    pub rows: u32,
    /// The rail on all four sides, 0 for none.
    pub rail: f64,
    pub rules: Rules,
}

impl Spec {
    fn cells(&self) -> usize {
        self.columns as usize * self.rows as usize
    }
}

/// What the checks need to know of the board that is repeated.
#[derive(Debug, Clone, PartialEq)]
pub struct CellFacts {
    /// The board's name.
    pub board: String,
    /// The finished thickness in mm, if the board has chosen one.
    pub thickness: Option<f64>,
    /// The least space the board keeps between copper and its edge, which its design rules
    /// check enforced.
    pub edge_clearance: f64,
    /// Whether the board house assembles any part of the board.
    pub assembled: bool,
    /// Whether any of those parts is on the back.
    pub bottom_assembled: bool,
}

/// A board and everything the panel is made from.
#[derive(Debug, Clone)]
pub struct CellInput<'a> {
    /// The text of the board's `.kicad_pcb`.
    pub pcb: &'a str,
    /// The text of the `.kicad_pro` beside it.
    pub pro: Option<&'a str>,
    /// The board's bill of materials: the parts the board house assembles.
    pub entries: &'a [BomEntry],
    pub facts: CellFacts,
}

/// One board of the panel.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// Counted along the rows from the top left, from 1.
    pub index: usize,
    pub column: u32,
    pub row: u32,
    /// How far the board's items move to reach this cell.
    pub dx: f64,
    pub dy: f64,
}

/// Where everything is, in the coordinates of the board file.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    /// The top left corner of the panel and its size.
    pub x0: f64,
    pub y0: f64,
    pub width: f64,
    pub height: f64,
    /// The size of one board.
    pub cell_width: f64,
    pub cell_height: f64,
    /// The top left corner of the board's outline in its own file.
    pub cell_x0: f64,
    pub cell_y0: f64,
    pub cells: Vec<Cell>,
    /// The x of every vertical V-cut line and the y of every horizontal one.
    pub cuts_x: Vec<f64>,
    pub cuts_y: Vec<f64>,
}

/// What the panel checks found.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub error: bool,
    pub message: String,
}

/// `50`, `12.5`: a length as short as it reads.
pub fn mm(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0" } else { s }.to_string()
}

fn number(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0" } else { s }.to_string()
}

/// The board's outline: its top left corner and size. A V-cut runs straight across the whole
/// panel, so the outline must be one rectangle with square corners.
pub fn cell_outline(pcb: &str) -> Result<(f64, f64, f64, f64), Error> {
    let art = Artwork::parse(pcb)?;
    let outline = edge::outline(&art).map_err(|e| Error::Panel(vec![e]))?;
    let b = crate::geom::Bounds::of(outline.outer.iter().copied()).expect("an outline has points");
    let corners = outline.outer.len() == 4
        && outline.outer.iter().all(|p| {
            ((p.x - b.min.x).abs() < 1e-6 || (p.x - b.max.x).abs() < 1e-6)
                && ((p.y - b.min.y).abs() < 1e-6 || (p.y - b.max.y).abs() < 1e-6)
        });
    if !corners {
        return Err(Error::Panel(vec![format!(
            "the board's outline is not a rectangle with square corners ({} points on Edge.Cuts): a V-cut runs straight across the whole panel, so every board in it must be a rectangle; set the corner radius to 0",
            outline.outer.len()
        )]));
    }
    Ok((b.min.x, b.min.y, b.max.x - b.min.x, b.max.y - b.min.y))
}

/// Where the cells, the rails and the cut lines are, for a board whose outline starts at
/// `(x0, y0)` and is `width` by `height`. Cells are side by side with no space between them.
pub fn geometry(spec: &Spec, outline: (f64, f64, f64, f64)) -> Geometry {
    let (cx0, cy0, w, h) = outline;
    let rail = spec.rail;
    let mut cells = Vec::new();
    for row in 0..spec.rows {
        for column in 0..spec.columns {
            cells.push(Cell {
                index: cells.len() + 1,
                column,
                row,
                dx: rail + f64::from(column) * w,
                dy: rail + f64::from(row) * h,
            });
        }
    }
    let width = 2.0 * rail + f64::from(spec.columns) * w;
    let height = 2.0 * rail + f64::from(spec.rows) * h;
    let lines = |start: f64, step: f64, count: u32| -> Vec<f64> {
        let mut v = Vec::new();
        if rail > 0.0 {
            v.push(start + rail);
        }
        v.extend((1..count).map(|i| start + rail + f64::from(i) * step));
        if rail > 0.0 {
            v.push(start + rail + f64::from(count) * step);
        }
        v
    };
    Geometry {
        x0: cx0,
        y0: cy0,
        width,
        height,
        cell_width: w,
        cell_height: h,
        cell_x0: cx0,
        cell_y0: cy0,
        cells,
        cuts_x: lines(cx0, w, spec.columns),
        cuts_y: lines(cy0, h, spec.rows),
    }
}

/// The panel-level checks, each message with the numbers in it and the board house's page
/// the number is from. The board's own checks have run before (ADR 0037).
pub fn check(spec: &Spec, g: &Geometry, facts: &CellFacts) -> Vec<Finding> {
    let r = &spec.rules;
    let name = &spec.name;
    let who = &r.fabricator;
    let src = &r.source;
    let mut out = Vec::new();
    let mut error = |message: String| {
        out.push(Finding {
            error: true,
            message: format!("panel {name}: {message} ({who}, {src})"),
        })
    };
    if spec.rail > 0.0 && spec.rail < r.min_rail {
        error(format!(
            "the rail is {} mm wide and a rail that carries tooling holes and fiducials is at least {} mm",
            mm(spec.rail),
            mm(r.min_rail)
        ));
    }
    if spec.cells() == 1 && spec.rail == 0.0 {
        error(
            "one board and no rail has nothing to cut apart: add columns or rows, or a rail"
                .to_string(),
        );
    }
    for (side, size) in [("wide", g.width), ("high", g.height)] {
        if size < r.min_size {
            error(format!(
                "the panel is {} mm {side} and a V-cut panel is at least {} mm each way",
                mm(size),
                mm(r.min_size)
            ));
        }
        if size > r.max_size {
            error(format!(
                "the panel is {} mm {side} and a V-cut panel is at most {} mm each way",
                mm(size),
                mm(r.max_size)
            ));
        } else if facts.assembled && size > r.max_assembled_size {
            error(format!(
                "the panel is {} mm {side} and a panel that is assembled is at most {} mm each way",
                mm(size),
                mm(r.max_assembled_size)
            ));
        }
    }
    for (what, lines) in [("vertical", &g.cuts_x), ("horizontal", &g.cuts_y)] {
        if lines.len() > r.max_cuts as usize {
            error(format!(
                "it needs {} {what} V-cut lines and there may be {} along one side",
                lines.len(),
                r.max_cuts
            ));
        }
    }
    let mut spacings = vec![
        ("board width", g.cell_width),
        ("board height", g.cell_height),
    ];
    if spec.rail > 0.0 {
        spacings.push(("rail", spec.rail));
    }
    for (what, gap) in spacings {
        if gap < r.min_cut_spacing {
            error(format!(
                "the {what} is {} mm, which puts two V-cut lines {} mm apart and they are at least {} mm apart",
                mm(gap),
                mm(gap),
                mm(r.min_cut_spacing)
            ));
        }
    }
    match facts.thickness {
        Some(t) if t < r.min_thickness => error(format!(
            "{} is {} mm thick and a V-cut board is at least {} mm thick",
            facts.board,
            mm(t),
            mm(r.min_thickness)
        )),
        None => error(format!(
            "{} leaves its thickness open and a V-cut board is at least {} mm thick: state it, for example `.process.stackup.thickness &= 1.6mm`",
            facts.board,
            mm(r.min_thickness)
        )),
        Some(_) => {}
    }
    if facts.edge_clearance + 1e-9 < r.min_edge_clearance {
        error(format!(
            "{} keeps copper {} mm from its edge and copper is at least {} mm from a V-cut edge: narrow the board, `.process.min_edge_clearance &= {}mm..1m`",
            facts.board,
            mm(facts.edge_clearance),
            mm(r.min_edge_clearance),
            mm(r.min_edge_clearance)
        ));
    }
    if facts.assembled && spec.rail == 0.0 {
        error(format!(
            "{} has parts the board house assembles and a panel that is assembled needs tooling holes and fiducials on its rails: add `.rail &= {}mm`",
            facts.board,
            mm(r.min_rail)
        ));
    }
    // Cells cannot overlap, as they are laid out side by side; the check is of the layout
    // itself, so that a change to it cannot go unnoticed.
    for (i, a) in g.cells.iter().enumerate() {
        for b in &g.cells[i + 1..] {
            let apart = (a.dx - b.dx).abs() >= g.cell_width - 1e-9
                || (a.dy - b.dy).abs() >= g.cell_height - 1e-9;
            if !apart {
                out.push(Finding {
                    error: true,
                    message: format!(
                        "panel {name}: cells {} and {} overlap ({} mm apart across, {} mm down, boards {} by {} mm)",
                        a.index,
                        b.index,
                        mm((a.dx - b.dx).abs()),
                        mm((a.dy - b.dy).abs()),
                        mm(g.cell_width),
                        mm(g.cell_height)
                    ),
                });
            }
        }
    }
    // Advice, not a refusal.
    for (what, gap, advised) in [
        ("board width", g.cell_width, r.advised_cut_spacing),
        ("board height", g.cell_height, r.advised_cut_spacing),
    ] {
        if gap >= r.min_cut_spacing && gap < advised {
            out.push(Finding {
                error: false,
                message: format!(
                    "panel {name}: the {what} is {} mm and {who} advises V-cut lines {} mm apart",
                    mm(gap),
                    mm(advised)
                ),
            });
        }
    }
    if facts.edge_clearance + 1e-9 >= r.min_edge_clearance
        && facts.edge_clearance + 1e-9 < r.advised_edge_clearance
    {
        out.push(Finding {
            error: false,
            message: format!(
                "panel {name}: copper is {} mm from the board edge and {who} advises {} mm from a V-cut",
                mm(facts.edge_clearance),
                mm(r.advised_edge_clearance)
            ),
        });
    }
    out
}

/// A name for item number `n` of `seed`, in the shape of a UUID, that is the same every time.
fn uuid(seed: &str, n: usize) -> String {
    fn fnv(bytes: &[u8], basis: u64) -> u64 {
        bytes.iter().fold(basis, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }
    let text = format!("{seed}/{n}");
    let a = fnv(text.as_bytes(), 0xcbf2_9ce4_8422_2325);
    let b = fnv(text.as_bytes(), 0x8422_2325_cbf2_9ce4);
    let hex = format!("{a:016x}{b:016x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// How one cell's items are carried over: moved, with nets and designators of their own.
struct Carry {
    dx: f64,
    dy: f64,
    cell: usize,
    /// Net numbers of the board run 1..=`nets`; cell `c` uses `(c - 1) * nets + n`.
    nets: i64,
    uuids: usize,
}

impl Carry {
    fn net_id(&self, n: i64) -> i64 {
        if n <= 0 {
            0
        } else {
            (self.cell as i64 - 1) * self.nets + n
        }
    }

    fn net_name(&self, name: &str) -> String {
        if name.is_empty() {
            String::new()
        } else {
            format!("cell{}/{name}", self.cell)
        }
    }

    fn moved(&mut self, e: &Sexp, inside: bool) -> Sexp {
        let Sexp::List(items) = e else {
            return e.clone();
        };
        let head = e.head();
        match head {
            Some("uuid") if items.len() == 2 => {
                self.uuids += 1;
                let old = items[1].text().unwrap_or("");
                Sexp::node(
                    "uuid",
                    vec![Sexp::string(uuid(&format!("{old}/{}", self.cell), 0))],
                )
            }
            Some("net") if items.len() >= 2 => {
                let id: i64 = items[1].text().and_then(|t| t.parse().ok()).unwrap_or(0);
                let mut out = vec![Sexp::atom("net"), Sexp::atom(self.net_id(id).to_string())];
                if let Some(name) = items.get(2).and_then(Sexp::text) {
                    out.push(Sexp::string(self.net_name(name)));
                }
                Sexp::List(out)
            }
            Some("net_name") if items.len() == 2 => Sexp::node(
                "net_name",
                vec![Sexp::string(
                    self.net_name(items[1].text().unwrap_or_default()),
                )],
            ),
            Some("property")
                if items.get(1).and_then(Sexp::text) == Some("Reference") && items.len() > 2 =>
            {
                let mut out: Vec<Sexp> = items.iter().map(|i| self.moved(i, inside)).collect();
                if let Some(r) = items[2].text() {
                    out[2] = Sexp::string(suffixed(r, self.cell));
                }
                Sexp::List(out)
            }
            Some("footprint") => {
                let mut out: Vec<Sexp> = Vec::new();
                for i in items {
                    match i.head() {
                        Some("at") => out.push(self.shift(i)),
                        Some("property") if self.silk_reference(i) => {
                            // The reference designator is made unique for the panel and
                            // hidden; the silkscreen keeps the text the board has.
                            let mut hidden = self.moved(i, true);
                            if let Sexp::List(h) = &mut hidden {
                                h.push(item("(hide yes)"));
                            }
                            out.push(hidden);
                            out.push(self.silk_text(i));
                        }
                        _ => out.push(self.moved(i, true)),
                    }
                }
                Sexp::List(out)
            }
            Some("start" | "end" | "mid" | "center" | "xy" | "at") if !inside => self.shift(e),
            _ => Sexp::List(items.iter().map(|i| self.moved(i, inside)).collect()),
        }
    }

    /// Whether `property` is a reference designator that is printed on the silkscreen.
    fn silk_reference(&self, property: &Sexp) -> bool {
        property.items().get(1).and_then(Sexp::text) == Some("Reference")
            && property.child("hide").is_none()
            && property
                .child("effects")
                .is_none_or(|e| e.child("hide").is_none())
            && layer_of(property).is_some_and(|l| l.ends_with("SilkS"))
    }

    /// The text the silkscreen shows for the reference `property`, as plain text.
    fn silk_text(&mut self, property: &Sexp) -> Sexp {
        let mut out = vec![
            Sexp::atom("fp_text"),
            Sexp::atom("user"),
            property.items().get(2).cloned().unwrap_or(Sexp::string("")),
        ];
        for name in ["at", "layer", "effects"] {
            if let Some(c) = property.child(name) {
                out.push(c.clone());
            }
        }
        self.uuids += 1;
        out.push(Sexp::node(
            "uuid",
            vec![Sexp::string(uuid(
                &format!("silk{}", self.cell),
                self.uuids + property.items().len(),
            ))],
        ));
        Sexp::List(out)
    }

    /// `(at x y ...)`, `(xy x y)`: the first two numbers moved.
    fn shift(&self, e: &Sexp) -> Sexp {
        let Sexp::List(items) = e else {
            return e.clone();
        };
        let mut out = items.clone();
        if let (Some(x), Some(y)) = (
            items
                .get(1)
                .and_then(Sexp::text)
                .and_then(|t| t.parse::<f64>().ok()),
            items
                .get(2)
                .and_then(Sexp::text)
                .and_then(|t| t.parse::<f64>().ok()),
        ) {
            out[1] = Sexp::atom(number(x + self.dx));
            out[2] = Sexp::atom(number(y + self.dy));
        }
        Sexp::List(out)
    }
}

/// `R1` of cell 3 is `R1_3`.
pub fn suffixed(designator: &str, cell: usize) -> String {
    format!("{designator}_{cell}")
}

/// The designator of the board a panel designator is of: `R1_3` is `R1`.
pub fn unsuffixed(designator: &str) -> &str {
    designator.rsplit_once('_').map_or(designator, |(d, _)| d)
}

fn item(text: &str) -> Sexp {
    sexp::parse(text).expect("a generated item is well formed")
}

fn at(x: f64, y: f64) -> String {
    format!("(at {} {})", number(x), number(y))
}

/// A hidden text of a footprint made for the panel.
fn hidden(kind: &str, text: &str, layer: &str) -> String {
    format!(
        "(property \"{kind}\" \"{text}\" (at 0 0 0) (layer \"{layer}\") (hide yes) (effects (font (size 1 1) (thickness 0.15))))"
    )
}

/// A fiducial on the front or the back.
fn fiducial(n: usize, x: f64, y: f64, diameter: f64, back: bool) -> Sexp {
    let (cu, mask, fab) = if back {
        ("B.Cu", "B.Mask", "B.Fab")
    } else {
        ("F.Cu", "F.Mask", "F.Fab")
    };
    let margin = diameter * (FIDUCIAL_OPENING - 1.0) / 2.0;
    item(&format!(
        "(footprint \"Fiducial:Fiducial_1mm_Mask2mm\" (layer \"{cu}\") (uuid \"{}\") {} {} {} (attr smd exclude_from_pos_files exclude_from_bom) (pad \"\" smd circle (at 0 0) (size {d} {d}) (layers \"{cu}\" \"{mask}\") (solder_mask_margin {m}) (clearance {m})))",
        uuid("panel-fiducial", n * 2 + usize::from(back)),
        at(x, y),
        hidden(
            "Reference",
            &format!("FID{}{}", n, if back { "B" } else { "" }),
            fab
        ),
        hidden("Value", "Fiducial", fab),
        d = number(diameter),
        m = number(margin),
    ))
}

/// A tooling hole: a drill with no copper and no net.
fn tooling_hole(n: usize, x: f64, y: f64, diameter: f64) -> Sexp {
    item(&format!(
        "(footprint \"MountingHole:MountingHole_2mm\" (layer \"F.Cu\") (uuid \"{}\") {} {} {} (attr exclude_from_pos_files exclude_from_bom) (pad \"\" np_thru_hole circle (at 0 0) (size {d} {d}) (drill {d}) (layers \"*.Cu\" \"*.Mask\")))",
        uuid("panel-tooling-hole", n),
        at(x, y),
        hidden("Reference", &format!("TH{n}"), "F.Fab"),
        hidden("Value", "ToolingHole", "F.Fab"),
        d = number(diameter),
    ))
}

/// The marks the rails carry: four tooling holes in the corners, centred in the rails, and
/// three fiducials a fixed distance from the panel's left and right edges, placed so that
/// the panel looks different turned round. `(x, y, diameter)` of each.
pub struct Marks {
    pub tooling_holes: Vec<(f64, f64, f64)>,
    pub fiducials: Vec<(f64, f64, f64)>,
}

pub fn marks(spec: &Spec, g: &Geometry) -> Marks {
    if spec.rail <= 0.0 {
        return Marks {
            tooling_holes: Vec::new(),
            fiducials: Vec::new(),
        };
    }
    let r = &spec.rules;
    let c = spec.rail / 2.0;
    let (x1, y1) = (g.x0 + g.width, g.y0 + g.height);
    let tooling_holes = [
        (g.x0 + c, g.y0 + c),
        (x1 - c, g.y0 + c),
        (g.x0 + c, y1 - c),
        (x1 - c, y1 - c),
    ]
    .iter()
    .map(|(x, y)| (*x, *y, r.tooling_hole))
    .collect();
    let along = 3.0 * spec.rail;
    let o = r.fiducial_offset;
    let fiducials = [
        (g.x0 + o, g.y0 + along),
        (g.x0 + o, y1 - along),
        (x1 - o, y1 - along),
    ]
    .iter()
    .map(|(x, y)| (*x, *y, r.fiducial))
    .collect();
    Marks {
        tooling_holes,
        fiducials,
    }
}

/// The panel as the text of a `.kicad_pcb`.
pub fn board_file(
    spec: &Spec,
    g: &Geometry,
    pcb: &str,
    back_fiducials: bool,
) -> Result<String, Error> {
    let root = sexp::parse(pcb).map_err(|e| Error::Panel(vec![e.to_string()]))?;
    if root.head() != Some("kicad_pcb") {
        return Err(Error::Panel(vec![
            "the board file is not a KiCad board".to_string(),
        ]));
    }
    let nets: Vec<(i64, String)> = root
        .children("net")
        .filter_map(|n| {
            let id: i64 = n.items().get(1)?.text()?.parse().ok()?;
            Some((id, n.items().get(2)?.text()?.to_string()))
        })
        .filter(|(id, _)| *id > 0)
        .collect();
    let net_count = nets.iter().map(|(id, _)| *id).max().unwrap_or(0);

    // The outline of the board: the rectangle that is replaced by the panel's.
    let (cx0, cy0, cw, ch) = (g.cell_x0, g.cell_y0, g.cell_width, g.cell_height);
    let is_outline = |e: &Sexp| -> bool {
        if e.head() != Some("gr_rect") || layer_of(e) != Some("Edge.Cuts") {
            return false;
        }
        let pt = |name: &str| -> Option<(f64, f64)> {
            let c = e.child(name)?;
            Some((
                c.items().get(1)?.text()?.parse().ok()?,
                c.items().get(2)?.text()?.parse().ok()?,
            ))
        };
        match (pt("start"), pt("end")) {
            (Some(a), Some(b)) => {
                let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
                let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
                (x0 - cx0).abs() < 1e-6
                    && (y0 - cy0).abs() < 1e-6
                    && (x1 - x0 - cw).abs() < 1e-6
                    && (y1 - y0 - ch).abs() < 1e-6
            }
            _ => false,
        }
    };
    let stroke = root
        .children("gr_rect")
        .find(|e| is_outline(e))
        .and_then(|e| e.child("stroke"))
        .and_then(|s| s.child("width"))
        .and_then(|w| w.items().get(1)?.text()?.parse::<f64>().ok());
    let Some(stroke) = stroke else {
        return Err(Error::Panel(vec![
            "the board's outline is not one `gr_rect` on Edge.Cuts; a panel is made from a board whose outline is a plain rectangle".to_string(),
        ]));
    };

    const HEADER: [&str; 8] = [
        "version",
        "generator",
        "generator_version",
        "general",
        "paper",
        "title_block",
        "layers",
        "setup",
    ];
    let mut out: Vec<Sexp> = vec![Sexp::atom("kicad_pcb")];
    for e in root.items().iter().skip(1) {
        match e.head() {
            Some("title_block") => out.push(item(&format!(
                "(title_block (title \"{}\"))",
                spec.name.replace('"', "")
            ))),
            Some("setup") => {
                // The drill and place origin moves with the first cell, so it keeps its place
                // on the board.
                let mut setup = e.clone();
                if let Sexp::List(items) = &mut setup {
                    for i in items.iter_mut() {
                        if i.head() == Some("aux_axis_origin") {
                            let shifted = Carry {
                                dx: spec.rail,
                                dy: spec.rail,
                                cell: 1,
                                nets: net_count,
                                uuids: 0,
                            }
                            .shift(i);
                            *i = shifted;
                        }
                    }
                }
                out.push(setup);
            }
            Some(h) if HEADER.contains(&h) => out.push(e.clone()),
            _ => {}
        }
    }
    out.push(item("(net 0 \"\")"));
    for cell in 1..=spec.cells() {
        let carry = Carry {
            dx: 0.0,
            dy: 0.0,
            cell,
            nets: net_count,
            uuids: 0,
        };
        for (id, name) in &nets {
            out.push(Sexp::node(
                "net",
                vec![
                    Sexp::atom(carry.net_id(*id).to_string()),
                    Sexp::string(carry.net_name(name)),
                ],
            ));
        }
    }
    for cell in &g.cells {
        let mut carry = Carry {
            dx: cell.dx,
            dy: cell.dy,
            cell: cell.index,
            nets: net_count,
            uuids: 0,
        };
        for e in root.items().iter().skip(1) {
            let Some(head) = e.head() else { continue };
            match head {
                h if HEADER.contains(&h) || h == "net" => {}
                "embedded_fonts" => {}
                "footprint" | "segment" | "via" | "arc" | "zone" | "gr_line" | "gr_rect"
                | "gr_circle" | "gr_arc" | "gr_poly" | "gr_curve" | "gr_text" => {
                    if is_outline(e) {
                        continue;
                    }
                    out.push(carry.moved(e, false));
                }
                other => {
                    return Err(Error::Panel(vec![format!(
                        "the board has a `{other}` and a panel cannot place that yet; the items it carries over are footprints, tracks, vias, zones and drawings"
                    )]));
                }
            }
        }
    }

    // The panel's own outline, the cut lines and their marks.
    let (x1, y1) = (g.x0 + g.width, g.y0 + g.height);
    let mut n = 0usize;
    let mut next = || {
        n += 1;
        uuid("panel", n)
    };
    out.push(item(&format!(
        "(gr_rect (start {} {}) (end {} {}) (stroke (width {}) (type default)) (fill no) (layer \"Edge.Cuts\") (uuid \"{}\"))",
        number(g.x0),
        number(g.y0),
        number(x1),
        number(y1),
        number(stroke),
        next()
    )));
    let line = |a: (f64, f64), b: (f64, f64), id: String| {
        item(&format!(
            "(gr_line (start {} {}) (end {} {}) (stroke (width {}) (type default)) (layer \"Edge.Cuts\") (uuid \"{id}\"))",
            number(a.0),
            number(a.1),
            number(b.0),
            number(b.1),
            number(stroke)
        ))
    };
    let mark = |x: f64, y: f64, id: String| {
        item(&format!(
            "(gr_text \"V-CUT\" (at {} {} 0) (layer \"Edge.Cuts\") (uuid \"{id}\") (effects (font (size {s} {s}) (thickness 0.15))))",
            number(x),
            number(y),
            s = number(MARK_SIZE)
        ))
    };
    for (i, x) in g.cuts_x.iter().enumerate() {
        out.push(line((*x, g.y0), (*x, y1), next()));
        let y = if i % 2 == 0 {
            g.y0 - MARK_GAP
        } else {
            y1 + MARK_GAP
        };
        out.push(mark(*x, y, next()));
    }
    for (i, y) in g.cuts_y.iter().enumerate() {
        out.push(line((g.x0, *y), (x1, *y), next()));
        let x = if i % 2 == 0 {
            g.x0 - 3.0 * MARK_GAP
        } else {
            x1 + 3.0 * MARK_GAP
        };
        out.push(mark(x, *y, next()));
    }
    let m = marks(spec, g);
    for (i, (x, y, d)) in m.tooling_holes.iter().enumerate() {
        out.push(tooling_hole(i + 1, *x, *y, *d));
    }
    for (i, (x, y, d)) in m.fiducials.iter().enumerate() {
        out.push(fiducial(i + 1, *x, *y, *d, false));
        if back_fiducials {
            out.push(fiducial(i + 1, *x, *y, *d, true));
        }
    }

    let mut text = String::from("(kicad_pcb");
    for e in out.iter().skip(1) {
        text.push_str("\n\t");
        e.write(&mut text);
    }
    text.push_str("\n)\n");
    Ok(text)
}

fn layer_of(e: &Sexp) -> Option<&str> {
    e.child("layer")?.items().get(1)?.text()
}

/// The panel drawn: boards outlined and named, cut lines, rails, tooling holes and fiducials.
pub fn svg(spec: &Spec, g: &Geometry, board: &str) -> String {
    let pad = 6.0;
    let (w, h) = (g.width + 2.0 * pad, g.height + 2.0 * pad);
    let x = |v: f64| number(v - g.x0 + pad);
    let y = |v: f64| number(v - g.y0 + pad);
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}mm\" height=\"{}mm\">\n",
        number(w),
        number(h),
        number(w),
        number(h)
    );
    s.push_str(&format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#e9e3d0\" stroke=\"#444\" stroke-width=\"0.3\"/>\n",
        x(g.x0),
        y(g.y0),
        number(g.width),
        number(g.height)
    ));
    let font = (g.cell_width.min(g.cell_height) / 8.0).clamp(1.0, 4.0);
    for c in &g.cells {
        let (cx, cy) = (g.cell_x0 + c.dx, g.cell_y0 + c.dy);
        s.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#2f7d4f\" fill-opacity=\"0.35\" stroke=\"#1b4d2f\" stroke-width=\"0.3\"/>\n<text x=\"{}\" y=\"{}\" font-size=\"{}\" font-family=\"sans-serif\" text-anchor=\"middle\">{} {}</text>\n",
            x(cx),
            y(cy),
            number(g.cell_width),
            number(g.cell_height),
            x(cx + g.cell_width / 2.0),
            y(cy + g.cell_height / 2.0 + font / 3.0),
            number(font),
            xml(board),
            c.index
        ));
    }
    for cx in &g.cuts_x {
        s.push_str(&format!(
            "<line x1=\"{0}\" y1=\"{1}\" x2=\"{0}\" y2=\"{2}\" stroke=\"#c0392b\" stroke-width=\"0.25\" stroke-dasharray=\"2 1\"/>\n",
            x(*cx),
            y(g.y0),
            y(g.y0 + g.height)
        ));
    }
    for cy in &g.cuts_y {
        s.push_str(&format!(
            "<line x1=\"{1}\" y1=\"{0}\" x2=\"{2}\" y2=\"{0}\" stroke=\"#c0392b\" stroke-width=\"0.25\" stroke-dasharray=\"2 1\"/>\n",
            y(*cy),
            x(g.x0),
            x(g.x0 + g.width)
        ));
    }
    let m = marks(spec, g);
    for (px, py, d) in &m.tooling_holes {
        s.push_str(&format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#fff\" stroke=\"#444\" stroke-width=\"0.2\"/>\n",
            x(*px),
            y(*py),
            number(d / 2.0)
        ));
    }
    for (px, py, d) in &m.fiducials {
        s.push_str(&format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#b8860b\"/>\n",
            x(*px),
            y(*py),
            number(d / 2.0)
        ));
    }
    s.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" font-size=\"2.5\" font-family=\"sans-serif\">{} {} x {} V-cut, {} x {} mm</text>\n</svg>\n",
        number(pad),
        number(pad - 2.0),
        xml(&spec.name),
        spec.columns,
        spec.rows,
        mm(g.width),
        mm(g.height)
    ));
    s
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}

/// The finished panel, ready to write.
#[derive(Debug)]
pub struct PanelPackage {
    /// The panel's name.
    pub name: String,
    /// The board it repeats.
    pub board: String,
    pub spec: Spec,
    pub geometry: Geometry,
    /// The panel as a `.kicad_pcb`, and the `.kicad_pro` it is checked and plotted to.
    pub pcb: String,
    pub pro: Option<String>,
    pub svg: String,
    pub gerber_zip: Vec<u8>,
    pub gerber_files: Vec<String>,
    pub bom_csv: String,
    pub cpl_csv: String,
    pub notes: String,
    /// The parts of the whole panel, with a designator for each cell.
    pub plan: Plan,
    pub findings: Vec<Finding>,
    pub drc: DrcOutcome,
    pub drc_report: drc::Report,
    /// The designators of the board (not of the panel) whose rotation or side could not be
    /// verified.
    pub unverified: Vec<String>,
}

impl PanelPackage {
    pub fn zip_name(&self) -> String {
        format!("{}-gerbers.zip", self.name)
    }

    /// Writes the files into `dir` (created if needed) and returns their paths.
    pub fn write(&self, dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, Error> {
        std::fs::create_dir_all(dir)
            .map_err(|e| Error::Io(format!("cannot create {}: {e}", dir.display())))?;
        let mut files: Vec<(String, &[u8])> = vec![
            (self.zip_name(), &self.gerber_zip),
            ("bom.csv".to_string(), self.bom_csv.as_bytes()),
            ("cpl.csv".to_string(), self.cpl_csv.as_bytes()),
            ("fab-notes.txt".to_string(), self.notes.as_bytes()),
            (format!("{}.kicad_pcb", self.name), self.pcb.as_bytes()),
            ("panel.svg".to_string(), self.svg.as_bytes()),
        ];
        if let Some(pro) = &self.pro {
            files.push((format!("{}.kicad_pro", self.name), pro.as_bytes()));
        }
        files
            .into_iter()
            .map(|(name, bytes)| {
                let path = dir.join(name);
                std::fs::write(&path, bytes)
                    .map_err(|e| Error::Io(format!("cannot write {}: {e}", path.display())))?;
                Ok(path)
            })
            .collect()
    }

    /// A few lines for a person to read after packaging.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "{}: {} x {} cells of {}, {} part(s) on {} BOM line(s), {} placement(s), {} file(s) in the zip\n",
            self.name,
            self.spec.columns,
            self.spec.rows,
            self.board,
            self.plan.assembled.len(),
            self.plan.bom.len(),
            self.plan.assembled.len(),
            self.gerber_files.len()
        );
        for line in self.notes.lines().filter(|l| !l.starts_with("fab-notes")) {
            s.push_str("  ");
            s.push_str(line);
            s.push('\n');
        }
        s
    }
}

/// Builds the panel of `spec` from the board in `cell`: checks it, writes it as a board file,
/// runs netsmith's design rules check on that file, and writes the Gerbers, drill files,
/// bill of materials and placement file with the writers [`crate::package`] uses. KiCad is
/// not needed.
pub fn build(spec: &Spec, cell: &CellInput, options: &Options) -> Result<PanelPackage, Error> {
    let outline = cell_outline(cell.pcb)?;
    let g = geometry(spec, outline);
    let findings = check(spec, &g, &cell.facts);
    let refused: Vec<String> = findings
        .iter()
        .filter(|f| f.error)
        .map(|f| f.message.clone())
        .collect();
    if !refused.is_empty() {
        return Err(Error::Panel(refused));
    }
    let pcb = board_file(spec, &g, cell.pcb, cell.facts.bottom_assembled)?;
    let name = spec.name.clone();

    let drc_report = drc::check(&pcb, cell.pro, &options.drc)?;
    let drc = if drc_report.is_clean() {
        DrcOutcome::Passed
    } else if options.allow_drc_violations {
        DrcOutcome::Waived(drc_report.counts())
    } else {
        return Err(Error::Drc {
            counts: drc_report.counts(),
            report: drc_report
                .errors()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        });
    };

    let art = Artwork::parse(&pcb)?;
    let rules = crate::plot_rules_of(cell.pro);
    let plotted = plot::plot(&art, &name, &rules)?;
    let mut files = plotted.files;
    files.extend(drill::files(&art, &name));
    files.sort_by(|a, b| a.name.cmp(&b.name));
    let gerber_zip = gerber::zip(&files)?;

    // The parts of every cell, a designator each.
    let board = Board::parse(&pcb)?;
    let cells = spec.cells();
    let entries: Vec<BomEntry> = cell
        .entries
        .iter()
        .map(|e| BomEntry {
            designators: (1..=cells)
                .flat_map(|k| e.designators.iter().map(move |d| suffixed(d, k)))
                .collect(),
            ..e.clone()
        })
        .collect();
    let fit: BTreeMap<String, Fit> = (1..=cells)
        .flat_map(|k| options.fit.iter().map(move |(d, f)| (suffixed(d, k), *f)))
        .collect();
    let panel_options = Options {
        fit,
        ..options.clone()
    };
    let plan = crate::plan(&board, &entries, &panel_options)?;
    let refs: Vec<&BoardPart> = plan.assembled.iter().collect();
    let placed = cpl::build(board.origin, &refs);

    let mut unverified: Vec<String> = placed
        .unverified
        .values()
        .flatten()
        .chain(&placed.bottom)
        .map(|d| unsuffixed(d).to_string())
        .collect();
    unverified.sort_by(|a, b| bom::designator_cmp(a, b));
    unverified.dedup();

    let mut extras = panel_notes(spec, &g, cell, &findings);
    extras.extend(options.extras.iter().cloned());
    extras.extend(crate::fill_notes(&art, &plotted.notes));
    let notes = crate::notes(
        &name,
        &board,
        &plan,
        &placed,
        (drc, &drc_report),
        // The cell's requirements on where parts go (ADR 0045) are measured on the cell's own
        // board, by its package; in the panel's frame their numbers would be shifted.
        &[],
        options.process.as_deref(),
        &extras,
        &options.trim,
    );
    Ok(PanelPackage {
        gerber_files: files.into_iter().map(|f| f.name).collect(),
        bom_csv: bom::render(&plan.bom_rows()),
        cpl_csv: cpl::render(&placed.rows),
        gerber_zip,
        notes,
        svg: svg(spec, &g, &cell.facts.board),
        board: cell.facts.board.clone(),
        spec: spec.clone(),
        geometry: g,
        pcb,
        pro: cell.pro.map(str::to_string),
        plan,
        findings,
        drc,
        drc_report,
        unverified,
        name,
    })
}

/// The lines of `fab-notes.txt` that say what the panel is and what to tell the board house.
fn panel_notes(spec: &Spec, g: &Geometry, cell: &CellInput, findings: &[Finding]) -> Vec<String> {
    let r = &spec.rules;
    let board = &cell.facts.board;
    let cells = spec.cells();
    let mut lines = vec![
        format!(
            "panel: {} columns x {} rows of {board} ({cells} boards of {} x {} mm), {}, {} x {} mm in all, boards side by side with no space between them",
            spec.columns,
            spec.rows,
            mm(g.cell_width),
            mm(g.cell_height),
            if spec.rail > 0.0 {
                format!("{} mm rails on all four sides", mm(spec.rail))
            } else {
                "no rails".to_string()
            },
            mm(g.width),
            mm(g.height)
        ),
        format!(
            "V-cut: {} vertical and {} horizontal line(s), each across the whole panel, drawn on Edge.Cuts with the mark \"V-CUT\" beside it outside the outline, as {} asks (same layer as the board outline, marked V-CUT: https://jlcpcb.com/blog/v-cut-panelization-standards); x = {} mm, y = {} mm",
            g.cuts_x.len(),
            g.cuts_y.len(),
            r.fabricator,
            list(&g.cuts_x),
            list(&g.cuts_y)
        ),
        format!(
            "tell {}: order it as a panel made by the customer (the Gerbers are the whole panel, 1 design in it; {} allows 10: https://jlcpcb.com/help/article/instructions-for-ordering), V-cut, with the cut lines as drawn; V-cut is not available in economic assembly, only in standard assembly (https://jlcpcb.com/help/article/pcb-assembly-faqs)",
            r.fabricator, r.fabricator
        ),
    ];
    let m = marks(spec, g);
    if m.tooling_holes.is_empty() {
        lines.push("rails: none, so no tooling holes and no fiducials".to_string());
    } else {
        lines.push(format!(
            "rails: {} tooling holes of {} mm, centred in the rails, and {} fiducials of {} mm centred {} mm from the panel edges on the {}, as {} asks of a panel it assembles",
            m.tooling_holes.len(),
            mm(r.tooling_hole),
            m.fiducials.len(),
            mm(r.fiducial),
            mm(r.fiducial_offset),
            if cell.facts.bottom_assembled {
                "front and the back"
            } else {
                "front"
            },
            r.fabricator
        ));
    }
    lines.push(format!(
        "UNVERIFIED designators: bom.csv and cpl.csv name the parts of cell k as R1_k (cells are counted along the rows from the top left; the silkscreen keeps the board's own names). {} publishes no rule for the designators of a panel made by the customer, and has not accepted a netsmith panel. If it refuses them, the board's own bom.csv and cpl.csv are in cell/{board}/, which are what ordering the single board and letting {} make the panel (which adds its own rails and fiducials) takes",
        r.fabricator, r.fabricator
    ));
    lines.extend(
        findings
            .iter()
            .filter(|f| !f.error)
            .map(|f| format!("warning: {}", f.message)),
    );
    lines
}

fn list(v: &[f64]) -> String {
    if v.is_empty() {
        "none".to_string()
    } else {
        v.iter().map(|x| mm(*x)).collect::<Vec<_>>().join(", ")
    }
}

#[cfg(test)]
mod tests;
