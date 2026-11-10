//! Footprints read from KiCad's `.kicad_mod` files.

use std::path::{Path, PathBuf};

use netsmith_kicad::install::{self, NotFound};
use netsmith_kicad::sexp::Sexp;

use crate::geometry::{Pt, Rect};

/// The environment variable that names the folder holding the `.pretty` libraries.
pub const ROOT_VARIABLE: &str = install::FOOTPRINTS_VARIABLE;

/// Space added around the pads when a footprint draws no courtyard.
const BARE_COURTYARD_MARGIN: f64 = 0.25;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FootprintError {
    /// The name is not `Library:Name`.
    BadName(String),
    NotFound {
        name: String,
        path: PathBuf,
        /// Whether the library is a project's own, listed in `kicad.toml`.
        project: bool,
    },
    /// The library is not a project's own and no KiCad install was found to look in.
    NoInstall {
        name: String,
        lookup: NotFound,
    },
    /// The library is a project's own and an installed library has the same name.
    Ambiguous {
        library: String,
        installed: PathBuf,
        project: PathBuf,
    },
    /// The library is a project's own and netsmith generates footprints of the same name.
    AmbiguousGenerated {
        library: String,
        project: PathBuf,
    },
    /// The library is a project's own and has the name of the standard library's own
    /// footprints (ADR 0087).
    AmbiguousStd {
        library: String,
        project: PathBuf,
    },
    /// The standard library's own `netsmith` has no footprint of this name.
    NoStdFootprint {
        name: String,
        known: Vec<String>,
    },
    Unreadable {
        path: PathBuf,
        reason: String,
    },
    /// The file is not a footprint netsmith can read.
    Invalid {
        name: String,
        reason: String,
    },
    /// A `gen:` footprint (ADR 0104) that its generator refuses.
    BadGenerated {
        name: String,
        reason: String,
    },
}

impl std::fmt::Display for FootprintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadName(name) => {
                write!(f, "`{name}` is not a footprint name like `Library:Name`")
            }
            Self::NotFound {
                name,
                path,
                project: false,
            } => write!(
                f,
                "footprint `{name}` not found: looked for {}; set {ROOT_VARIABLE} to the folder \
                 that holds the .pretty libraries, or list the library in kicad.toml",
                path.display()
            ),
            Self::NotFound {
                name,
                path,
                project: true,
            } => write!(
                f,
                "footprint `{name}` not found: looked for {} (the library kicad.toml lists)",
                path.display()
            ),
            Self::NoInstall { name, lookup } => write!(
                f,
                "footprint `{name}` not found: {lookup}\n(a library of the project itself is listed in kicad.toml and needs no KiCad)"
            ),
            Self::Ambiguous {
                library,
                installed,
                project,
            } => write!(
                f,
                "footprint library `{library}` is both installed ({}) and listed in kicad.toml \
                 ({}); give the project's library another name",
                installed.display(),
                project.display()
            ),
            Self::AmbiguousGenerated { library, project } => write!(
                f,
                "footprint library `{library}` is both one whose footprints netsmith generates \
                 and listed in kicad.toml ({}); give the project's library another name",
                project.display()
            ),
            Self::AmbiguousStd { library, project } => write!(
                f,
                "footprint library `{library}` is the standard library's own (std/libs/) \
                 and also listed in kicad.toml ({}); give the project's library another name",
                project.display()
            ),
            Self::NoStdFootprint { name, known } => write!(
                f,
                "footprint `{name}` not found in the standard library `{}` (std/libs/); it has {}",
                netsmith_kicad::stdlib::NAME,
                known.join(", ")
            ),
            Self::Unreadable { path, reason } => write!(f, "{}: {reason}", path.display()),
            Self::Invalid { name, reason } => write!(f, "footprint `{name}`: {reason}"),
            Self::BadGenerated { name, reason } => {
                write!(f, "generated footprint `{name}`: {reason}")
            }
        }
    }
}

impl std::error::Error for FootprintError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadKind {
    Smd,
    ThruHole,
    /// A hole with no copper.
    NpThruHole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadShape {
    Rect,
    RoundRect,
    Oval,
    Circle,
}

/// A drilled hole: its diameter, or its two sides if it is a slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drill {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pad {
    /// What a net's pin names: `1`, `A1`. Several pads of one footprint can share it.
    pub number: String,
    pub kind: PadKind,
    pub shape: PadShape,
    /// The centre, in the footprint's own frame.
    pub at: Pt,
    /// The pad's own turn in degrees, in the footprint's frame.
    pub angle: f64,
    pub width: f64,
    pub height: f64,
    pub drill: Option<Drill>,
    pub layers: Vec<String>,
    /// How round a `RoundRect` corner is, as a fraction of the shorter side.
    pub corner_ratio: f64,
    /// For a `custom` pad, the box its `primitives` span in the pad's own frame (before the
    /// pad's turn), which the anchor's `shape` and size do not cover. The primitives
    /// themselves stay in the footprint's source and are copied to the board unchanged.
    pub extent: Option<Rect>,
    /// The pad's own `(clearance ...)`, or its footprint's if the pad has none: the space
    /// the pad keeps from copper of other nets when it is larger than the board's. `None` when
    /// the file says nothing. The design rules check applies it as the router must.
    pub clearance: Option<f64>,
}

/// How a footprint is soldered to the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mount {
    /// Surface mount: pads on the face of the board.
    Smd,
    /// Leads through plated holes.
    ThroughHole,
}

/// The footprint layers netsmith draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Silk,
    Fab,
    /// `F.CrtYd`: the courtyard on the component side.
    Courtyard,
    /// `B.CrtYd`: the courtyard on the other side. Only read to know which faces the
    /// footprint draws a courtyard on ([`Footprint::courtyard_faces`]).
    BackCourtyard,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Line {
        a: Pt,
        b: Pt,
    },
    Rect {
        a: Pt,
        b: Pt,
    },
    Circle {
        centre: Pt,
        radius: f64,
    },
    /// The arc from `start` through `mid` to `end`.
    Arc {
        start: Pt,
        mid: Pt,
        end: Pt,
    },
    Poly {
        points: Vec<Pt>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Graphic {
    pub layer: Layer,
    pub shape: Shape,
    pub stroke: f64,
    pub filled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    /// `R_0402_1005Metric`, without its library.
    pub name: String,
    pub mount: Mount,
    pub pads: Vec<Pad>,
    pub graphics: Vec<Graphic>,
    /// The box the courtyard drawing spans, in the footprint's frame. A footprint with no
    /// courtyard gets its pads plus a small margin.
    pub courtyard: Rect,
    /// Which layers the footprint draws its courtyard on, `(F.CrtYd, B.CrtYd)`. A through-hole
    /// part that draws it on one only is judged on the other face by its pads (ADR 0031).
    /// Neither: the courtyard is made from the pads, and counts on both faces.
    pub courtyard_faces: (bool, bool),
    /// The part's body, in the footprint's frame: the box the drawing on the fabrication
    /// layer (`F.Fab`) spans, which a constraint on a position measures (ADR 0045). A
    /// footprint that draws nothing there has its courtyard as body ([`Footprint::body_is_courtyard`]).
    pub body: Rect,
    /// Whether `body` is the courtyard, because the footprint draws no fabrication layer.
    pub body_is_courtyard: bool,
    /// Where the library puts the reference text, in the footprint's frame.
    pub reference_at: Pt,
    /// The height of the reference text the library draws, in mm (1 if it says nothing).
    pub reference_size: f64,
    /// The boxes of the shapes the library draws on the solder mask layers, in the
    /// footprint's frame: openings in the mask that no pad makes.
    pub mask_openings: Vec<Rect>,
    /// The file as read, which the `.kicad_pcb` writer copies from.
    pub source: Sexp,
}

impl Footprint {
    /// The footprint of a silkscreen label: `text` in letters `height` tall and `thickness`
    /// thick, and a courtyard round it. It has no pads. The reference and the value are
    /// there for the board file but are hidden: the label prints its text, not a designator.
    pub fn label(text: &str, height: f64, thickness: f64) -> Footprint {
        const COURTYARD_MARGIN: f64 = 0.25;
        // The width of a line in KiCad's stroke font is about seven tenths of its height for
        // each character, and the strokes add their thickness.
        let width = text.chars().count() as f64 * height * 0.7 + thickness;
        let (hw, hh) = (
            width / 2.0 + COURTYARD_MARGIN,
            height / 2.0 + thickness / 2.0 + COURTYARD_MARGIN,
        );
        let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
        let font = format!("(effects (font (size {height} {height}) (thickness {thickness})))");
        let source = format!(
            "(footprint \"Label\" (version 20241229) (generator \"netsmith\") (layer \"F.Cu\") \
             (property \"Reference\" \"REF**\" (at 0 {up} 0) (layer \"F.SilkS\") (hide yes) {font}) \
             (property \"Value\" \"{escaped}\" (at 0 {down} 0) (layer \"F.Fab\") (hide yes) {font}) \
             (attr exclude_from_pos_files exclude_from_bom) \
             (fp_text user \"{escaped}\" (at 0 0 0) (layer \"F.SilkS\") {font}) \
             (fp_rect (start {nhw} {nhh}) (end {hw} {hh}) (stroke (width 0.05) (type solid)) (fill no) (layer \"F.CrtYd\")))",
            up = -hh - 0.5,
            down = hh + 0.5,
            nhw = -hw,
            nhh = -hh,
        );
        Footprint::parse("Label", &source).expect("a label footprint is well formed")
    }

    /// Read the text of a `.kicad_mod`. `name` is only used to say which footprint a
    /// problem is in.
    pub fn parse(name: &str, text: &str) -> Result<Footprint, FootprintError> {
        let invalid = |reason: String| FootprintError::Invalid {
            name: name.to_string(),
            reason,
        };
        let source = Sexp::parse(text).map_err(|e| invalid(e.to_string()))?;
        if source.head() != Some("footprint") {
            return Err(invalid("it is not a (footprint ...) file".to_string()));
        }
        let own_name = source
            .args()
            .first()
            .and_then(Sexp::text)
            .ok_or_else(|| invalid("it has no name".to_string()))?
            .to_string();

        let mut pads = Vec::new();
        // A pad without a clearance of its own has its footprint's, as in KiCad.
        let footprint_clearance = source
            .find("clearance")
            .and_then(|c| c.args().first())
            .and_then(Sexp::text)
            .and_then(|t| t.parse::<f64>().ok());
        for pad in source.find_all("pad") {
            let mut pad = read_pad(pad).map_err(invalid)?;
            pad.clearance = pad.clearance.or(footprint_clearance);
            pads.push(pad);
        }
        let mut graphics = Vec::new();
        for item in source.args() {
            if let Some(g) = read_graphic(item).map_err(invalid)? {
                graphics.push(g);
            }
        }

        // The file says how the part is mounted; without it, a drilled pad means through hole.
        let mount = match source
            .find("attr")
            .and_then(|a| a.args().first())
            .and_then(Sexp::text)
        {
            Some("smd") => Mount::Smd,
            Some("through_hole") => Mount::ThroughHole,
            _ if pads.iter().any(|p| p.kind != PadKind::Smd) => Mount::ThroughHole,
            _ => Mount::Smd,
        };

        let courtyard_faces = (
            graphics.iter().any(|g| g.layer == Layer::Courtyard),
            graphics.iter().any(|g| g.layer == Layer::BackCourtyard),
        );
        // The box is the front layer's if the footprint draws one, else the back layer's.
        let drawn = if courtyard_faces.0 {
            Layer::Courtyard
        } else {
            Layer::BackCourtyard
        };
        let courtyard_points = graphics
            .iter()
            .filter(|g| g.layer == drawn)
            .flat_map(|g| g.shape.extent_points());
        let courtyard = match Rect::around(courtyard_points) {
            Some(r) => r,
            None => Rect::around(pads.iter().flat_map(|p| p.bounds().corners()))
                .ok_or_else(|| invalid("it has no courtyard and no pads".to_string()))?
                .expanded(BARE_COURTYARD_MARGIN),
        };

        // A hole may reach past the drawn courtyard (a mounting hole, a pogo-pin pad), and the
        // placer keeps courtyards apart, so the courtyard takes in every hole.
        let courtyard = pads
            .iter()
            .filter_map(|p| {
                let reach = p.drill?.width.max(p.drill?.height) / 2.0;
                Some(Rect::new(p.at, p.at).expanded(reach))
            })
            .fold(courtyard, |c, hole| c.union(&hole));

        // The body: what is drawn on the fabrication layer, else the courtyard (a hole that
        // reaches past the drawing is in the courtyard, and is the part's body too).
        let fab_points: Vec<Pt> = graphics
            .iter()
            .filter(|g| g.layer == Layer::Fab)
            .flat_map(|g| g.shape.exact_extent())
            .collect();
        let (body, body_is_courtyard) = match Rect::around(fab_points) {
            Some(r) => (r, false),
            None => (courtyard, true),
        };

        let reference_at = source
            .find_all("property")
            .find(|p| p.args().first().and_then(Sexp::text) == Some("Reference"))
            .and_then(|p| p.find("at"))
            .and_then(|at| point(at, 0))
            .unwrap_or_default();

        let reference_size = source
            .find_all("property")
            .find(|p| p.args().first().and_then(Sexp::text) == Some("Reference"))
            .and_then(|p| p.find("effects")?.find("font")?.find("size"))
            .and_then(|size| size.args().first()?.text()?.parse().ok())
            .unwrap_or(1.0);

        let mask_openings = source
            .args()
            .iter()
            .filter_map(mask_opening)
            .collect::<Vec<_>>();

        Ok(Footprint {
            name: own_name,
            mount,
            pads,
            graphics,
            courtyard,
            courtyard_faces,
            body,
            body_is_courtyard,
            reference_at,
            reference_size,
            mask_openings,
            source,
        })
    }

    /// Every pad that carries this number.
    pub fn pads_numbered<'a>(&'a self, number: &'a str) -> impl Iterator<Item = &'a Pad> {
        self.pads.iter().filter(move |p| p.number == number)
    }

    /// The pad numbers in the order the file lists them, each once.
    pub fn pad_numbers(&self) -> Vec<&str> {
        let mut numbers: Vec<&str> = Vec::new();
        for p in &self.pads {
            if !numbers.contains(&p.number.as_str()) {
                numbers.push(&p.number);
            }
        }
        numbers
    }
}

impl Pad {
    /// Whether the pad has copper on the front and on the back.
    pub fn copper_layers(&self) -> (bool, bool) {
        if self.kind == PadKind::NpThruHole {
            return (false, false);
        }
        let has = |names: [&str; 2]| self.layers.iter().any(|l| names.contains(&l.as_str()));
        (has(["F.Cu", "*.Cu"]), has(["B.Cu", "*.Cu"]))
    }

    /// The box the pad's copper spans in the footprint's frame, with its own turn applied. A
    /// custom pad's box takes in its primitives.
    pub fn bounds(&self) -> Rect {
        let half = Pt::new(self.width / 2.0, self.height / 2.0);
        let anchor = Rect::new(Pt::new(-half.x, -half.y), half);
        self.extent
            .map_or(anchor, |e| anchor.union(&e))
            .rotated(self.angle)
            .translated(self.at)
    }
}

impl Shape {
    /// Points whose box is the shape's own box: an arc includes the points of its circle
    /// that its sweep passes through (the top, bottom, left or right), which three points
    /// of it can fall short of.
    pub fn exact_extent(&self) -> Vec<Pt> {
        let Shape::Arc { start, mid, end } = self else {
            return self.extent_points();
        };
        let d = 2.0
            * (start.x * (mid.y - end.y) + mid.x * (end.y - start.y) + end.x * (start.y - mid.y));
        if d.abs() < 1e-12 {
            return vec![*start, *mid, *end];
        }
        let sq = |p: &Pt| p.x * p.x + p.y * p.y;
        let (a, b, c) = (start, mid, end);
        let x = (sq(a) * (b.y - c.y) + sq(b) * (c.y - a.y) + sq(c) * (a.y - b.y)) / d;
        let y = (sq(a) * (c.x - b.x) + sq(b) * (a.x - c.x) + sq(c) * (b.x - a.x)) / d;
        let centre = Pt::new(x, y);
        let r = a.distance(centre);
        let angle = |p: &Pt| (p.y - y).atan2(p.x - x);
        let tau = std::f64::consts::TAU;
        let (a0, am, a1) = (angle(a), angle(b), angle(c));
        // The sweep from the start to the end through the middle, in the sense of increasing
        // angle if the middle is reached first that way, else from the end to the start.
        let up = (am - a0).rem_euclid(tau) <= (a1 - a0).rem_euclid(tau);
        let (from, span) = if up {
            (a0, (a1 - a0).rem_euclid(tau))
        } else {
            (a1, (a0 - a1).rem_euclid(tau))
        };
        let mut points = vec![*start, *end];
        for k in 0..4 {
            let phi = f64::from(k) * std::f64::consts::FRAC_PI_2;
            if (phi - from).rem_euclid(tau) <= span {
                points.push(Pt::new(x + r * phi.cos(), y + r * phi.sin()));
            }
        }
        points
    }

    /// Points that bound the shape. An arc is bounded by its three points, which can fall
    /// short of the bulge; courtyards do not use arcs.
    pub fn extent_points(&self) -> Vec<Pt> {
        match self {
            Shape::Line { a, b } | Shape::Rect { a, b } => vec![*a, *b],
            Shape::Circle { centre, radius } => vec![
                Pt::new(centre.x - radius, centre.y - radius),
                Pt::new(centre.x + radius, centre.y + radius),
            ],
            Shape::Arc { start, mid, end } => vec![*start, *mid, *end],
            Shape::Poly { points } => points.clone(),
        }
    }
}

fn float(item: &Sexp) -> Result<f64, String> {
    let text = item.text().ok_or("expected a number, found a list")?;
    text.parse()
        .map_err(|_| format!("`{text}` is not a number"))
}

/// The pair of numbers at `args[first..]` of a node such as `(at x y)`.
fn point(node: &Sexp, first: usize) -> Option<Pt> {
    let args = node.args();
    let x = args.get(first)?.text()?.parse().ok()?;
    let y = args.get(first + 1)?.text()?.parse().ok()?;
    Some(Pt::new(x, y))
}

fn required_point(parent: &Sexp, name: &str) -> Result<Pt, String> {
    parent
        .find(name)
        .and_then(|n| point(n, 0))
        .ok_or_else(|| format!("({} ...) has no ({name} x y)", parent.head().unwrap_or("?")))
}

fn read_pad(pad: &Sexp) -> Result<Pad, String> {
    let args = pad.args();
    let number = args
        .first()
        .and_then(Sexp::text)
        .ok_or("a pad has no number")?
        .to_string();
    let kind = match args.get(1).and_then(Sexp::text) {
        // An edge-connector pad (the Tag-Connect footprints) is copper on one face with no
        // mask or paste, which for routing is a surface-mount pad.
        Some("smd" | "connect") => PadKind::Smd,
        Some("thru_hole") => PadKind::ThruHole,
        Some("np_thru_hole") => PadKind::NpThruHole,
        other => {
            return Err(format!(
                "pad {number} has type `{}`, which is not supported",
                other.unwrap_or("")
            ));
        }
    };
    let custom = args.get(2).and_then(Sexp::text) == Some("custom");
    let shape = match args.get(2).and_then(Sexp::text) {
        // A custom pad is drawn from an anchor (a circle unless the file says rect) and its
        // primitives; the anchor is its shape here and the primitives widen its box.
        Some("custom") => match pad
            .find("options")
            .and_then(|o| o.find("anchor"))
            .and_then(|a| a.args().first())
            .and_then(Sexp::text)
        {
            Some("rect") => PadShape::Rect,
            _ => PadShape::Circle,
        },
        Some("rect") => PadShape::Rect,
        Some("roundrect") => PadShape::RoundRect,
        Some("oval") => PadShape::Oval,
        Some("circle") => PadShape::Circle,
        other => {
            return Err(format!(
                "pad {number} has shape `{}`, which is not supported",
                other.unwrap_or("")
            ));
        }
    };
    let at_node = pad
        .find("at")
        .ok_or_else(|| format!("pad {number} has no position"))?;
    let at = point(at_node, 0).ok_or_else(|| format!("pad {number} has a bad position"))?;
    let angle = match at_node.args().get(2) {
        Some(a) => float(a)?,
        None => 0.0,
    };
    let size = required_point(pad, "size").map_err(|e| format!("pad {number}: {e}"))?;
    let drill = pad.find("drill").and_then(|d| match d.args() {
        [first, ..] if first.text() == Some("oval") => Some(Drill {
            width: d.args().get(1)?.text()?.parse().ok()?,
            height: d.args().get(2)?.text()?.parse().ok()?,
        }),
        [first, ..] => {
            let d: f64 = first.text()?.parse().ok()?;
            Some(Drill {
                width: d,
                height: d,
            })
        }
        [] => None,
    });
    let layers = pad
        .find("layers")
        .map(|l| {
            l.args()
                .iter()
                .filter_map(|a| a.text().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let corner_ratio = match pad.find("roundrect_rratio") {
        Some(r) => float(r.args().first().ok_or("roundrect_rratio has no value")?)?,
        None => 0.25,
    };
    let extent = if custom {
        primitives_extent(pad).map_err(|e| format!("pad {number}: {e}"))?
    } else {
        None
    };
    Ok(Pad {
        number,
        kind,
        shape,
        at,
        angle,
        width: size.x,
        height: size.y,
        drill,
        layers,
        corner_ratio,
        extent,
        clearance: pad
            .find("clearance")
            .and_then(|c| c.args().first())
            .and_then(Sexp::text)
            .and_then(|t| t.parse().ok()),
    })
}

/// The box that a custom pad's `(primitives ...)` span, with each stroke's half width added.
/// An arc counts as its whole circle, which can only overstate the box.
fn primitives_extent(pad: &Sexp) -> Result<Option<Rect>, String> {
    let Some(block) = pad.find("primitives") else {
        return Ok(None);
    };
    let mut extent: Option<Rect> = None;
    for item in block.args() {
        let Some(head) = item.head() else {
            continue;
        };
        let points: Vec<Pt> = match head {
            "gr_line" | "gr_rect" => {
                vec![required_point(item, "start")?, required_point(item, "end")?]
            }
            "gr_circle" => {
                let centre = required_point(item, "center")?;
                let r = centre.distance(required_point(item, "end")?);
                vec![
                    Pt::new(centre.x - r, centre.y - r),
                    Pt::new(centre.x + r, centre.y + r),
                ]
            }
            "gr_arc" => {
                let (a, m, b) = (
                    required_point(item, "start")?,
                    required_point(item, "mid")?,
                    required_point(item, "end")?,
                );
                circle_through(a, m, b).unwrap_or_else(|| vec![a, m, b])
            }
            "gr_poly" | "gr_curve" => {
                let pts = item
                    .find("pts")
                    .ok_or_else(|| format!("a {head} has no (pts ...)"))?;
                pts.find_all("xy").filter_map(|xy| point(xy, 0)).collect()
            }
            other => return Err(format!("primitive `{other}` is not supported")),
        };
        let stroke = match item.find("width") {
            Some(w) => float(w.args().first().ok_or("a width has no value")?)? / 2.0,
            None => 0.0,
        };
        if let Some(r) = Rect::around(points) {
            let r = r.expanded(stroke);
            extent = Some(extent.map_or(r, |e| e.union(&r)));
        }
    }
    Ok(extent)
}

/// The two corners of the box round the circle through three points; none if they are in a
/// line.
fn circle_through(a: Pt, b: Pt, c: Pt) -> Option<Vec<Pt>> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let sq = |p: Pt| p.x * p.x + p.y * p.y;
    let x = (sq(a) * (b.y - c.y) + sq(b) * (c.y - a.y) + sq(c) * (a.y - b.y)) / d;
    let y = (sq(a) * (c.x - b.x) + sq(b) * (a.x - c.x) + sq(c) * (b.x - a.x)) / d;
    let r = a.distance(Pt::new(x, y));
    Some(vec![Pt::new(x - r, y - r), Pt::new(x + r, y + r)])
}

fn layer_of(item: &Sexp) -> Option<Layer> {
    match item.find("layer")?.args().first()?.text()? {
        "F.SilkS" => Some(Layer::Silk),
        "F.Fab" => Some(Layer::Fab),
        "F.CrtYd" => Some(Layer::Courtyard),
        "B.CrtYd" => Some(Layer::BackCourtyard),
        _ => None,
    }
}

/// The box of a shape the footprint draws on a solder mask layer.
fn mask_opening(item: &Sexp) -> Option<Rect> {
    if !matches!(
        item.head()?,
        "fp_line" | "fp_rect" | "fp_circle" | "fp_arc" | "fp_poly"
    ) {
        return None;
    }
    if !matches!(
        item.find("layer")?.args().first()?.text()?,
        "F.Mask" | "B.Mask"
    ) {
        return None;
    }
    let mut points: Vec<Pt> = ["start", "end", "mid"]
        .iter()
        .filter_map(|k| item.find(k).and_then(|n| point(n, 0)))
        .collect();
    if let Some(pts) = item.find("pts") {
        points.extend(pts.find_all("xy").filter_map(|xy| point(xy, 0)));
    }
    let bounds = Rect::around(points)?;
    // A circle is its centre and a point on it.
    Some(
        match (item.head()?, item.find("center").and_then(|n| point(n, 0))) {
            ("fp_circle", Some(c)) => {
                let radius = item.find("end").and_then(|n| point(n, 0))?.distance(c);
                Rect::new(c, c).expanded(radius)
            }
            _ => bounds,
        },
    )
}

fn read_graphic(item: &Sexp) -> Result<Option<Graphic>, String> {
    let Some(head) = item.head() else {
        return Ok(None);
    };
    if !matches!(
        head,
        "fp_line" | "fp_rect" | "fp_circle" | "fp_arc" | "fp_poly"
    ) {
        return Ok(None);
    }
    let Some(layer) = layer_of(item) else {
        return Ok(None);
    };
    let shape = match head {
        "fp_line" => Shape::Line {
            a: required_point(item, "start")?,
            b: required_point(item, "end")?,
        },
        "fp_rect" => Shape::Rect {
            a: required_point(item, "start")?,
            b: required_point(item, "end")?,
        },
        "fp_circle" => {
            let centre = required_point(item, "center")?;
            Shape::Circle {
                centre,
                radius: centre.distance(required_point(item, "end")?),
            }
        }
        "fp_arc" => Shape::Arc {
            start: required_point(item, "start")?,
            mid: required_point(item, "mid")?,
            end: required_point(item, "end")?,
        },
        _ => {
            let pts = item.find("pts").ok_or("a fp_poly has no (pts ...)")?;
            Shape::Poly {
                points: pts.find_all("xy").filter_map(|xy| point(xy, 0)).collect(),
            }
        }
    };
    // KiCad 7 and later write `(stroke (width w) ...)`; older files write `(width w)`.
    let stroke = item
        .find("stroke")
        .and_then(|s| s.find("width"))
        .or_else(|| item.find("width"))
        .and_then(|w| w.args().first())
        .map(float)
        .transpose()?
        .unwrap_or(0.0);
    let filled = item
        .find("fill")
        .and_then(|f| f.args().first())
        .and_then(Sexp::text)
        .is_some_and(|f| !matches!(f, "none" | "no"));
    Ok(Some(Graphic {
        layer,
        shape,
        stroke,
        filled,
    }))
}

/// `Name` of the id `netsmith:Name`, the standard library's own footprints (ADR 0087).
fn std_name(id: &str) -> Option<&str> {
    let (library, name) = id.split_once(':')?;
    (library == netsmith_kicad::stdlib::NAME && !name.is_empty() && !name.contains(':'))
        .then_some(name)
}

/// Where footprints come from, in this order (ADR 0043, ADR 0087):
///
/// 0. the standard library's own `netsmith` footprints (`std/libs/`), in the binary, which no
///    other library can share a name with;
///
/// 1. a project's own `.pretty` folders, by their library name;
/// 2. the generators ([`crate::generate`]), for the names they make, if the library was
///    built with [`FootprintLibrary::installed`] or [`FootprintLibrary::with_generators`]:
///    a build gives the same pads on every machine whatever KiCad has installed;
/// 3. a folder of `<Library>.pretty` folders, such as KiCad's `share/kicad/footprints`.
#[derive(Debug, Clone)]
pub struct FootprintLibrary {
    root: PathBuf,
    /// Set when `root` is a guess because no install was found
    /// (see [`FootprintLibrary::installed`]).
    no_install: Option<NotFound>,
    /// A project's libraries: name and `.pretty` folder.
    project: Vec<(String, PathBuf)>,
    /// Whether the generators are consulted before the folder.
    generators: bool,
}

impl FootprintLibrary {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            no_install: None,
            project: Vec::new(),
            generators: false,
        }
    }

    /// The generators, then the installed KiCad libraries, found as [`install`] describes:
    /// `NETSMITH_KICAD_FOOTPRINTS`, then beside `kicad-cli`, then the usual install places.
    /// A name a generator makes never reads a file; every other name does.
    ///
    /// Finding no install is not an error here: a design that uses only generated
    /// footprints and a project's own libraries needs no KiCad. [`FootprintLibrary::load`]
    /// fails, naming every place looked at, when a footprint of an installed library is
    /// asked for. [`FootprintLibrary::root`] is then the first place looked at.
    pub fn installed() -> Self {
        Self::installed_by(&install::Search::machine())
    }

    /// [`FootprintLibrary::installed`] over another machine, for tests.
    pub fn installed_by(search: &install::Search) -> Self {
        match search.find(install::Kind::Footprints) {
            Ok(found) => Self::at(found.dir),
            Err(lookup) => {
                let mut lib = Self::at(lookup.first_dir().unwrap_or(Path::new("")));
                lib.no_install = Some(lookup);
                lib
            }
        }
        .with_generators()
    }

    /// Consult the generators before the folder. [`FootprintLibrary::at`] reads only the
    /// folder, which is what a test of the file reader or of the generators against a copy
    /// of KiCad's libraries wants.
    pub fn with_generators(mut self) -> Self {
        self.generators = true;
        self
    }

    /// Add a project's library: `name` is what `Name:Footprint` ids use.
    pub fn with_library(mut self, name: &str, folder: impl Into<PathBuf>) -> Self {
        self.project.push((name.to_string(), folder.into()));
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The `.pretty` folder of a library, and whether it is the project's own.
    fn folder_of(&self, library: &str) -> Result<(PathBuf, bool), FootprintError> {
        let installed = self.root.join(format!("{library}.pretty"));
        // With no install found `root` is a guess (possibly `""`, which makes `installed` a
        // path relative to the working directory): no library is installed then, so none is
        // shadowed.
        let is_installed = self.no_install.is_none() && installed.is_dir();
        match self.project.iter().find(|(n, _)| n == library) {
            Some((_, folder)) if library == netsmith_kicad::stdlib::NAME => {
                Err(FootprintError::AmbiguousStd {
                    library: library.to_string(),
                    project: folder.clone(),
                })
            }
            Some((_, folder)) if is_installed => Err(FootprintError::Ambiguous {
                library: library.to_string(),
                installed,
                project: folder.clone(),
            }),
            Some((_, folder)) if self.generators && crate::generate::covers_library(library) => {
                Err(FootprintError::AmbiguousGenerated {
                    library: library.to_string(),
                    project: folder.clone(),
                })
            }
            Some((_, folder)) => Ok((folder.clone(), true)),
            None => Ok((installed, false)),
        }
    }

    /// Where `Library:Name` should be. For the standard library's own footprints that is the
    /// file of the repository's `std/libs/`.
    pub fn path_of(&self, name: &str) -> Result<PathBuf, FootprintError> {
        if let Some(footprint) = std_name(name) {
            return Ok(PathBuf::from(format!(
                "std/libs/{}/{footprint}.kicad_mod",
                netsmith_kicad::stdlib::FOOTPRINT_FOLDER
            )));
        }
        let (path, project) = self.locate(name)?;
        match (project, &self.no_install) {
            (false, Some(lookup)) => Err(FootprintError::NoInstall {
                name: name.to_string(),
                lookup: lookup.clone(),
            }),
            _ => Ok(path),
        }
    }

    fn locate(&self, name: &str) -> Result<(PathBuf, bool), FootprintError> {
        match name.split_once(':') {
            Some((library, footprint))
                if !library.is_empty() && !footprint.is_empty() && !footprint.contains(':') =>
            {
                let (folder, project) = self.folder_of(library)?;
                Ok((folder.join(format!("{footprint}.kicad_mod")), project))
            }
            _ => Err(FootprintError::BadName(name.to_string())),
        }
    }

    /// Read `Library:Name`.
    pub fn load(&self, name: &str) -> Result<Footprint, FootprintError> {
        // A footprint asked for by generator and parameters needs no library and no setting
        // (ADR 0104): the id carries the parameters.
        if let Some(spec) = netsmith_kicad::footprint_spec::FootprintSpec::from_id(name) {
            return crate::generate::generate_spec(&spec).map_err(|reason| {
                FootprintError::BadGenerated {
                    name: spec.spelling(),
                    reason,
                }
            });
        }
        // netsmith's own library is in the binary and answers first, whatever else is set up.
        if let Some(footprint) = std_name(name) {
            // A project's library of the same name is refused, with both named.
            if self
                .project
                .iter()
                .any(|(n, _)| n == netsmith_kicad::stdlib::NAME)
            {
                self.folder_of(netsmith_kicad::stdlib::NAME)?;
            }
            let text = netsmith_kicad::stdlib::footprint(footprint).ok_or_else(|| {
                FootprintError::NoStdFootprint {
                    name: name.to_string(),
                    known: netsmith_kicad::stdlib::footprint_names()
                        .map(str::to_string)
                        .collect(),
                }
            })?;
            return Footprint::parse(name, text);
        }
        let (path, project) = self.locate(name)?;
        if self.generators
            && !project
            && let Some(footprint) = crate::generate::generate(name)
        {
            return Ok(footprint);
        }
        // With no install found there is no installed library: `root` is a guess, possibly `""`,
        // and a path under it may be relative to the working directory. It is never looked at.
        if let (false, Some(lookup)) = (project, &self.no_install) {
            return Err(FootprintError::NoInstall {
                name: name.to_string(),
                lookup: lookup.clone(),
            });
        }
        if !path.is_file() {
            return Err(FootprintError::NotFound {
                name: name.to_string(),
                path,
                project,
            });
        }
        let text = std::fs::read_to_string(&path).map_err(|e| FootprintError::Unreadable {
            path: path.clone(),
            reason: e.to_string(),
        })?;
        Footprint::parse(name, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> FootprintLibrary {
        FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints())
    }

    const UDC: &str = "netsmith:UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA";

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// The pads of the LT8641's package as its datasheet (Rev. C, p.24, "Recommended solder pad
    /// pitch and dimensions" and the bottom view) gives them. The footprint is in the binary:
    /// no KiCad and no folder are needed.
    #[test]
    fn the_standard_library_has_the_udc_qfn_18_of_the_lt8641_with_the_datasheets_pads() {
        let fp = FootprintLibrary::at("/nonexistent").load(UDC).unwrap();
        assert_eq!(fp.mount, Mount::Smd);
        assert_eq!(
            fp.pad_numbers(),
            [
                "1", "2", "3", "4", "6", "7", "8", "9", "10", "11", "13", "14", "15", "16", "17",
                "18", "19", "20", "21", "22"
            ]
        );
        let pad = |n: &str| fp.pads.iter().find(|p| p.number == n).unwrap();
        for p in &fp.pads {
            assert_eq!(p.kind, PadKind::Smd);
            assert_eq!(p.layers, ["F.Cu", "F.Mask", "F.Paste"], "pad {}", p.number);
        }
        // The two long sides (pins 1 to 6 and 11 to 16): 0.70 long, 0.25 wide, 0.50 apart, the
        // inner edges 2.10 apart (3.50 over the outer edges). Pins 5 and 12 are not there.
        for (n, y) in [
            ("1", -1.25),
            ("2", -0.75),
            ("3", -0.25),
            ("4", 0.25),
            ("6", 1.25),
        ] {
            let (l, r) = (pad(n), pad(&format!("{}", 17 - n.parse::<u32>().unwrap())));
            assert_eq!((l.width, l.height), (0.7, 0.25), "pad {n}");
            assert_eq!((r.width, r.height), (0.7, 0.25));
            assert!(
                near(l.at.x, -1.4) && near(l.at.y, y),
                "pad {n} at {:?}",
                l.at
            );
            assert!(near(r.at.x, 1.4) && near(r.at.y, y), "pad {n}");
        }
        assert!(near(
            pad("13").at.x - pad("13").width / 2.0 - (pad("4").at.x + pad("4").width / 2.0),
            2.1
        ));
        // The two short sides (pins 7 to 10 and 17 to 20): 0.25 wide, 0.70 long, the inner edges
        // 3.10 apart (4.50 over the outer edges).
        for (n, x) in [("7", -0.75), ("8", -0.25), ("9", 0.25), ("10", 0.75)] {
            let (b, t) = (pad(n), pad(&format!("{}", 27 - n.parse::<u32>().unwrap())));
            assert_eq!((b.width, b.height), (0.25, 0.7), "pad {n}");
            assert!(near(b.at.x, x) && near(b.at.y, 1.9), "pad {n}");
            assert!(near(t.at.x, x) && near(t.at.y, -1.9), "pad {}", t.number);
        }
        assert!(near(pad("7").at.y - 0.35 - (pad("20").at.y + 0.35), 3.1));
        // The exposed pads, both SW: 0.220 wide, 0.356 and 0.400 long, 0.770 apart.
        let (p22, p21) = (pad("22"), pad("21"));
        assert_eq!((p22.width, p22.height), (0.22, 0.356));
        assert_eq!((p21.width, p21.height), (0.22, 0.4));
        assert!(near(p22.at.x, 0.0) && near(p21.at.x, 0.0));
        assert!(near(p21.at.y - p22.at.y, 0.77), "{} {}", p22.at.y, p21.at.y);
        // The top edge of pad 22 is 2.127 from the top edge of the 4.00 body.
        assert!(near(p22.at.y - p22.height / 2.0 + 2.0, 2.127));
        // The courtyard is the pads (3.50 by 4.50) and 0.25 all round.
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-2.0, -2.5), Pt::new(2.0, 2.5))
        );
    }

    /// One pad as a maker's drawing gives it: number, kind, centre, size and the hole
    /// (a slot's two sides, or one diameter twice).
    type Expected = (
        &'static str,
        PadKind,
        (f64, f64),
        (f64, f64),
        Option<(f64, f64)>,
    );

    /// A footprint of the standard library against the numbers read from the maker's drawing
    /// (`std/libs/README.md` says where each is), with no KiCad and no folder.
    fn assert_maker_footprint(
        name: &str,
        expected: &[Expected],
        courtyard: (f64, f64, f64, f64),
        body: Option<(f64, f64, f64, f64)>,
    ) {
        let fp = FootprintLibrary::at("/nonexistent")
            .load(&format!("netsmith:{name}"))
            .unwrap();
        // The receptacles and switches are `attr smd`, whatever their shell holes; the SMA jack
        // is through hole.
        let mount = if name.starts_with("SMA_") {
            Mount::ThroughHole
        } else {
            Mount::Smd
        };
        assert_eq!(fp.mount, mount, "{name}");
        assert_eq!(fp.pads.len(), expected.len(), "{name}: pad count");
        let mut left: Vec<&Pad> = fp.pads.iter().collect();
        for (number, kind, (x, y), (w, h), drill) in expected {
            let found = left.iter().position(|p| {
                p.number == *number
                    && near(p.at.x, *x)
                    && near(p.at.y, *y)
                    && near(p.width, *w)
                    && near(p.height, *h)
            });
            let Some(i) = found else {
                panic!("{name}: no pad {number} at ({x}, {y}) of {w} x {h} among {left:#?}");
            };
            let pad = left.remove(i);
            assert_eq!(pad.kind, *kind, "{name}: pad {number}");
            match (pad.drill, drill) {
                (Some(d), Some((dw, dh))) => {
                    assert!(
                        near(d.width, *dw) && near(d.height, *dh),
                        "{name}: {number}"
                    );
                }
                (None, None) => {}
                (got, want) => panic!("{name}: pad {number} drill {got:?}, wanted {want:?}"),
            }
            // A surface pad of a maker's drawing has paste, except where the drawing forbids it
            // (the Tag-Connect contact pads).
            let layers: &[&str] = match kind {
                PadKind::Smd if name.starts_with("Tag-Connect") => &["F.Cu", "F.Mask"],
                PadKind::Smd => &["F.Cu", "F.Mask", "F.Paste"],
                _ => &["*.Cu", "*.Mask"],
            };
            assert_eq!(pad.layers, layers, "{name}: pad {number}");
        }
        // The drawn courtyard, taken wider where a long slot's reach (half its length, all
        // round the pad's centre) goes past it, as `Footprint::parse` does for every hole.
        let (x0, y0, x1, y1) = courtyard;
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(x0, y0), Pt::new(x1, y1)),
            "{name}: courtyard"
        );
        let Some((x0, y0, x1, y1)) = body else {
            assert!(fp.body_is_courtyard, "{name}: no body is drawn on F.Fab");
            return;
        };
        let fab = fp.body;
        assert!(!fp.body_is_courtyard, "{name}: the body is drawn on F.Fab");
        assert!(
            near(fab.min.x, x0)
                && near(fab.min.y, y0)
                && near(fab.max.x, x1)
                && near(fab.max.y, y1),
            "{name}: body {fab:?}"
        );
    }

    use PadKind::{NpThruHole, Smd, ThruHole};

    /// A shell hole's (y, pad width, pad length, drill length).
    type ShellHole = (f64, f64, f64, f64);

    #[test]
    fn the_gct_usb4125_has_the_pads_of_its_recommended_pcb_layout() {
        // Drawing USB4125 Rev A6, sheet 1. The origin is the middle of the four shell holes
        // (3.80 apart on the long axis, 8.64 on the short one).
        let mut pads: Vec<Expected> = vec![
            ("B12", Smd, (-2.75, -1.98), (0.80, 1.20), None),
            ("B9", Smd, (-1.52, -1.98), (0.76, 1.20), None),
            ("A5", Smd, (-0.50, -1.98), (0.70, 1.20), None),
            ("B5", Smd, (0.50, -1.98), (0.70, 1.20), None),
            ("A9", Smd, (1.52, -1.98), (0.76, 1.20), None),
            ("A12", Smd, (2.75, -1.98), (0.80, 1.20), None),
        ];
        for x in [-4.32, 4.32] {
            for y in [-1.90, 1.90] {
                pads.push(("S1", ThruHole, (x, y), (1.10, 1.70), Some((0.60, 1.20))));
            }
        }
        assert_maker_footprint(
            "USB_C_Receptacle_GCT_USB4125_6P_TopMnt_Horizontal",
            &pads,
            (-5.12, -3.0, 5.12, 4.75),
            Some((-4.47, -2.58, 4.47, 4.5)),
        );
    }

    /// The 16 contacts of a USB 2.0 receptacle as the 12 pads of a layout: A1 and B12 (and the
    /// other corner and inner pairs) share a pad, so each is two pads at one place. `shell` is
    /// the x of the shell holes and `upper` and `lower` each one's (y, pad width, pad length,
    /// drill length); the drill is 0.60 wide. `hole` is the positioning hole's (y, diameter).
    fn usb16_pads(
        row: (f64, f64, f64),
        shell: (f64, ShellHole, ShellHole),
        hole: (f64, f64),
    ) -> Vec<Expected> {
        let (row_y, pad_h, wide) = row;
        let mut pads: Vec<Expected> = Vec::new();
        for (n, x) in [
            ("A1", -3.2),
            ("B12", -3.2),
            ("A4", -2.4),
            ("B9", -2.4),
            ("A9", 2.4),
            ("B4", 2.4),
            ("A12", 3.2),
            ("B1", 3.2),
        ] {
            pads.push((n, Smd, (x, row_y), (wide, pad_h), None));
        }
        for (n, x) in [
            ("B8", -1.75),
            ("A5", -1.25),
            ("B7", -0.75),
            ("A6", -0.25),
            ("A7", 0.25),
            ("B6", 0.75),
            ("A8", 1.25),
            ("B5", 1.75),
        ] {
            pads.push((n, Smd, (x, row_y), (0.30, pad_h), None));
        }
        let (shell_x, upper, lower) = shell;
        for x in [-shell_x, shell_x] {
            for (y, w, l, drill) in [upper, lower] {
                pads.push(("S1", ThruHole, (x, y), (w, l), Some((0.60, drill))));
            }
        }
        for x in [-2.89, 2.89] {
            pads.push((
                "",
                NpThruHole,
                (x, hole.0),
                (hole.1, hole.1),
                Some((hole.1, hole.1)),
            ));
        }
        pads
    }

    #[test]
    fn the_gct_usb4105_has_the_pads_of_its_recommended_pcb_layout() {
        // Drawing USB4105 Rev B4, sheet 1. The origin is the middle of the four shell holes
        // (4.18 apart); the pad's lower edge is on the line of the upper holes.
        let pads = usb16_pads(
            (-2.665, 1.15, 0.60),
            (4.32, (-2.09, 1.00, 2.10, 1.70), (2.09, 1.00, 1.80, 1.40)),
            (-1.59, 0.65),
        );
        assert_maker_footprint(
            "USB_C_Receptacle_GCT_USB4105_16P_TopMnt_Horizontal",
            &pads,
            (-5.17, -3.49, 5.17, 4.94),
            Some((-4.47, -3.24, 4.47, 4.69)),
        );
    }

    #[test]
    fn the_hro_type_c_31_m_12_has_the_pads_of_its_recommended_pcb_layout() {
        // The maker's drawing Rev A of 2020-12-08: the pad is 1.64 - 0.50 = 1.14 long; upper
        // holes with a pad of 0.90 x 2.00 and a drill 0.60 x 1.70, lower ones 0.90 x 1.70 and
        // 0.60 x 1.40.
        let pads = usb16_pads(
            (-2.66, 1.14, 0.60),
            (4.325, (-2.09, 0.90, 2.00, 1.70), (2.09, 0.90, 1.70, 1.40)),
            (-1.59, 0.60),
        );
        assert_maker_footprint(
            "USB_C_Receptacle_HRO_TYPE-C-31-M-12",
            &pads,
            (-5.175, -3.48, 5.175, 4.45),
            Some((-4.47, -3.23, 4.47, 4.2)),
        );
    }

    #[test]
    fn the_omron_b3u_1000p_has_the_pcb_pads_of_its_drawing() {
        // Cat. No. A162-E1-07 p.2: 4.2 over the outer edges, 2.6 between the inner edges, 1.7
        // long. The numbers are those of the drawing's terminal arrangement (2 left, 1 right).
        assert_maker_footprint(
            "SW_SPST_Omron_B3U-1000P",
            &[
                ("2", Smd, (-1.7, 0.0), (0.8, 1.7), None),
                ("1", Smd, (1.7, 0.0), (0.8, 1.7), None),
            ],
            (-2.35, -1.5, 2.35, 1.5),
            Some((-1.5, -1.25, 1.5, 1.25)),
        );
    }

    #[test]
    fn the_ck_rs_282g05a3_has_the_two_lands_of_its_drawing() {
        // Drawing RS-282G05A-SM RT, version A05: two lands of 1.5 x 1.5, 7.8 between centres.
        assert_maker_footprint(
            "SW_SPST_CK_RS-282G05A3-SM",
            &[
                ("1", Smd, (-3.9, 0.0), (1.5, 1.5), None),
                ("2", Smd, (3.9, 0.0), (1.5, 1.5), None),
            ],
            (-4.9, -2.05, 4.9, 2.05),
            Some((-3.0, -1.8, 3.0, 1.8)),
        );
    }

    #[test]
    fn the_amphenol_132134_has_the_holes_of_its_drawing_and_pads_derived_by_the_land_rule() {
        // Customer outline drawing of 132134, sheet 3 of 3: one hole of 1.5 in the middle and
        // four of 1.7 on a 5.08 square. The pads are derived (hole + 2 x 0.05 + 0.50, density
        // level B of the IPC-2221 land rule): 2.10 and 2.30. The pad is a through hole with no
        // paste, as `*.Cu` and `*.Mask` say.
        let mut pads: Vec<Expected> =
            vec![("1", ThruHole, (0.0, 0.0), (2.1, 2.1), Some((1.5, 1.5)))];
        for x in [-2.54, 2.54] {
            for y in [-2.54, 2.54] {
                pads.push(("2", ThruHole, (x, y), (2.3, 2.3), Some((1.7, 1.7))));
            }
        }
        assert_maker_footprint(
            "SMA_Amphenol_132134_Vertical",
            &pads,
            (-3.94, -3.94, 3.94, 3.94),
            Some((-3.5, -3.5, 3.5, 3.5)),
        );
        let fp = FootprintLibrary::at("/nonexistent")
            .load("netsmith:SMA_Amphenol_132134_Vertical")
            .unwrap();
        // The ring each pad leaves round its hole stays above what JLCPCB asks of a plated
        // through hole on a 2-layer board (0.25 mm recommended with 1 oz copper, 0.254 with
        // 2 oz; the absolute minimum is 0.18).
        for pad in &fp.pads {
            let ring = (pad.width - pad.drill.unwrap().width) / 2.0;
            assert!(ring >= 0.254, "pad {} has a ring of {ring}", pad.number);
        }
    }

    #[test]
    fn the_tag_connect_tc2030_has_the_pads_and_alignment_holes_of_its_drawing() {
        // Drawing TC2030-IDC-NL-FP Rev B, sheet 1 ("PCB top layer"): six round contact pads of
        // 0.787 at 1.27 pitch in two rows 1.27 apart, no paste, and three non-plated alignment
        // holes of 0.991: one 2.54 to the left of the middle, two 2.54 to the right and
        // 2.032 (2 x 1.016) apart. Pads 1, 3, 5 are the lower row, 2, 4, 6 the upper one.
        let mut pads: Vec<Expected> = Vec::new();
        for (n, x, y) in [
            ("1", -1.27, 0.635),
            ("2", -1.27, -0.635),
            ("3", 0.0, 0.635),
            ("4", 0.0, -0.635),
            ("5", 1.27, 0.635),
            ("6", 1.27, -0.635),
        ] {
            pads.push((n, Smd, (x, y), (0.787, 0.787), None));
        }
        for (x, y) in [(-2.54, 0.0), (2.54, -1.016), (2.54, 1.016)] {
            pads.push(("", NpThruHole, (x, y), (0.991, 0.991), Some((0.991, 0.991))));
        }
        assert_maker_footprint(
            "Tag-Connect_TC2030-IDC-NL_2x03_P1.27mm_Vertical",
            &pads,
            (-3.2855, -1.7615, 3.2855, 1.7615),
            None,
        );
    }

    #[test]
    fn a_name_the_standard_library_does_not_have_says_what_it_has() {
        let e = FootprintLibrary::at("/nonexistent")
            .load("netsmith:Nothing")
            .unwrap_err();
        assert!(matches!(e, FootprintError::NoStdFootprint { .. }), "{e}");
        let text = e.to_string();
        assert!(
            text.contains("`netsmith:Nothing`")
                && text.contains("UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA"),
            "{text}"
        );
        assert_eq!(
            FootprintLibrary::at("/nonexistent").path_of(UDC).unwrap(),
            PathBuf::from(
                "std/libs/netsmith.pretty/UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA.kicad_mod"
            )
        );
    }

    #[test]
    fn a_project_library_named_netsmith_is_refused_with_both_named() {
        let e = FootprintLibrary::at("/nonexistent")
            .with_library("netsmith", "/proj/libs/mine.pretty")
            .load(UDC)
            .unwrap_err();
        assert!(matches!(e, FootprintError::AmbiguousStd { .. }));
        let text = e.to_string();
        assert!(
            text.contains("`netsmith`")
                && text.contains("/proj/libs/mine.pretty")
                && text.contains("another name"),
            "{text}"
        );
    }

    #[test]
    fn reads_a_resistor() {
        let fp = fixtures().load("Resistor_SMD:R_0402_1005Metric").unwrap();
        assert_eq!(fp.name, "R_0402_1005Metric");
        assert_eq!(fp.pad_numbers(), ["1", "2"]);
        let p1 = &fp.pads[0];
        assert_eq!(p1.kind, PadKind::Smd);
        assert_eq!(p1.shape, PadShape::RoundRect);
        assert_eq!(p1.at, Pt::new(-0.51, 0.0));
        assert_eq!((p1.width, p1.height), (0.54, 0.64));
        assert_eq!(p1.drill, None);
        assert_eq!(p1.layers, ["F.Cu", "F.Mask", "F.Paste"]);
        assert_eq!(p1.corner_ratio, 0.25);
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-0.93, -0.47), Pt::new(0.93, 0.47))
        );
        assert_eq!(fp.reference_at, Pt::new(0.0, -1.17));
        assert_eq!(fp.mount, Mount::Smd);
    }

    #[test]
    fn reads_the_graphics_on_the_three_layers() {
        let fp = fixtures().load("Resistor_SMD:R_0402_1005Metric").unwrap();
        let on = |l| fp.graphics.iter().filter(|g| g.layer == l).count();
        assert_eq!(on(Layer::Silk), 2);
        assert_eq!(on(Layer::Courtyard), 4);
        assert_eq!(on(Layer::Fab), 4);
        let silk = fp.graphics.iter().find(|g| g.layer == Layer::Silk).unwrap();
        assert_eq!(silk.stroke, 0.12);
        assert!(!silk.filled);
    }

    #[test]
    fn reads_a_through_hole_header() {
        let fp = fixtures()
            .load("Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical")
            .unwrap();
        assert_eq!(fp.pad_numbers(), ["1", "2"]);
        assert_eq!(fp.mount, Mount::ThroughHole);
        assert_eq!(fp.pads[0].kind, PadKind::ThruHole);
        assert_eq!(fp.pads[0].shape, PadShape::Rect);
        assert_eq!(fp.pads[1].shape, PadShape::Oval);
        assert_eq!(
            fp.pads[1].drill,
            Some(Drill {
                width: 1.0,
                height: 1.0
            })
        );
        assert_eq!(fp.pads[1].at, Pt::new(0.0, 2.54));
        assert_eq!(fp.courtyard.min, Pt::new(-1.8, -1.8));
        assert_eq!(fp.courtyard.max, Pt::new(1.8, 4.35));
    }

    #[test]
    fn a_regulator_has_two_pads_numbered_two() {
        let fp = fixtures()
            .load("Package_TO_SOT_SMD:SOT-223-3_TabPin2")
            .unwrap();
        assert_eq!(fp.pad_numbers(), ["1", "2", "3"]);
        assert_eq!(fp.pads_numbered("2").count(), 2);
        assert!(
            fp.graphics
                .iter()
                .any(|g| matches!(g.shape, Shape::Poly { .. }) && g.layer == Layer::Silk)
        );
    }

    #[test]
    fn reads_arcs_circles_rects_and_old_style_widths() {
        let fp = Footprint::parse(
            "T:T",
            r#"(footprint "T"
                (fp_arc (start 1 0) (mid 0 1) (end -1 0) (width 0.1) (layer "F.SilkS"))
                (fp_circle (center 1 1) (end 1 2) (stroke (width 0.2) (type solid)) (fill solid) (layer "F.Fab"))
                (fp_rect (start -1 -1) (end 2 2) (stroke (width 0.05) (type solid)) (fill none) (layer "F.CrtYd"))
                (fp_line (start 0 0) (end 1 1) (layer "B.SilkS"))
                (pad "1" smd rect (at 0 0 90) (size 1 2) (layers "F.Cu")))"#,
        )
        .unwrap();
        assert_eq!(fp.graphics.len(), 3);
        assert_eq!(fp.graphics[0].stroke, 0.1);
        assert!(matches!(fp.graphics[0].shape, Shape::Arc { .. }));
        assert_eq!(
            fp.graphics[1].shape,
            Shape::Circle {
                centre: Pt::new(1.0, 1.0),
                radius: 1.0
            }
        );
        assert!(fp.graphics[1].filled);
        assert_eq!(fp.courtyard.max, Pt::new(2.0, 2.0));
        assert_eq!(fp.pads[0].angle, 90.0);
    }

    #[test]
    fn the_shapes_a_footprint_draws_on_the_solder_mask_are_openings_with_their_boxes() {
        let fp = Footprint::parse(
            "T:T",
            r#"(footprint "T"
                (fp_poly (pts (xy 1 1) (xy 3 1) (xy 3 2) (xy 1 2)) (stroke (width 0) (type solid)) (fill solid) (layer "F.Mask"))
                (fp_circle (center 0 5) (end 1 5) (stroke (width 0.1) (type solid)) (fill none) (layer "B.Mask"))
                (fp_line (start 0 0) (end 1 1) (layer "F.SilkS"))
                (pad "1" smd rect (at 0 0) (size 1 2) (layers "F.Cu" "F.Mask")))"#,
        )
        .unwrap();
        assert_eq!(
            fp.mask_openings,
            vec![
                Rect::new(Pt::new(1.0, 1.0), Pt::new(3.0, 2.0)),
                Rect::new(Pt::new(-1.0, 4.0), Pt::new(1.0, 6.0)),
            ]
        );
    }

    #[test]
    fn a_footprint_without_a_courtyard_gets_its_pads_and_a_margin() {
        let fp = Footprint::parse(
            "T:T",
            r#"(footprint "T" (pad "1" smd rect (at 0 0) (size 1 2) (layers "F.Cu")))"#,
        )
        .unwrap();
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-0.75, -1.25), Pt::new(0.75, 1.25))
        );
    }

    #[test]
    fn a_footprint_with_no_attr_is_through_hole_if_a_pad_is_drilled() {
        let smd = Footprint::parse(
            "T:T",
            r#"(footprint "T" (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu")))"#,
        )
        .unwrap();
        let th = Footprint::parse(
            "T:T",
            r#"(footprint "T" (pad "1" thru_hole circle (at 0 0) (size 2 2) (drill 1) (layers "*.Cu")))"#,
        )
        .unwrap();
        assert_eq!((smd.mount, th.mount), (Mount::Smd, Mount::ThroughHole));
    }

    #[test]
    fn a_connect_pad_is_a_surface_pad() {
        let fp = Footprint::parse(
            "T:T",
            r#"(footprint "T" (pad "1" connect circle (at 0 0) (size 1 1) (layers "F.Cu")))"#,
        )
        .unwrap();
        assert_eq!(fp.pads[0].kind, PadKind::Smd);
        assert_eq!(fp.mount, Mount::Smd);
    }

    #[test]
    fn a_rotated_pad_swaps_its_sides() {
        let fp = Footprint::parse(
            "T:T",
            r#"(footprint "T" (pad "1" smd rect (at 3 0 90) (size 1 2) (layers "F.Cu")))"#,
        )
        .unwrap();
        let b = fp.pads[0].bounds();
        assert_eq!((b.width(), b.height()), (2.0, 1.0));
        assert_eq!(b.centre(), Pt::new(3.0, 0.0));
    }

    #[test]
    fn a_missing_footprint_names_itself_and_where_it_looked() {
        let err = fixtures().load("Resistor_SMD:R_9999").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("Resistor_SMD:R_9999"), "{text}");
        assert!(text.contains("Resistor_SMD.pretty"), "{text}");
        assert!(text.contains("R_9999.kicad_mod"), "{text}");
        assert!(text.contains(ROOT_VARIABLE), "{text}");
    }

    #[test]
    fn no_install_is_an_error_only_when_an_installed_footprint_is_asked_for() {
        let empty = netsmith_kicad::testlib::TempDir::new("no-kicad-fp");
        let library = FootprintLibrary::installed_by(&install::Search::under(empty.path()));
        // A name a generator makes needs no install.
        assert!(library.load("Resistor_SMD:R_0402_1005Metric").is_ok());
        // A project's own library needs none either.
        let mine = netsmith_kicad::testlib::TempDir::new("mine-fp");
        let pretty = mine.path().join("mine.pretty");
        std::fs::create_dir_all(&pretty).unwrap();
        std::fs::copy(
            netsmith_kicad::testlib::sample_footprints()
                .join("Resistor_SMD.pretty/R_0402_1005Metric.kicad_mod"),
            pretty.join("Mine.kicad_mod"),
        )
        .unwrap();
        let library = library.with_library("mine", &pretty);
        assert!(library.load("mine:Mine").is_ok());
        // Another stock footprint names every place looked at and both variables.
        let text = library
            .load("Package_QFP:LQFP-64_10x10mm_P0.5mm")
            .unwrap_err()
            .to_string();
        assert!(
            text.contains("footprint `Package_QFP:LQFP-64_10x10mm_P0.5mm` not found"),
            "{text}"
        );
        assert!(
            text.contains("no KiCad footprint libraries found"),
            "{text}"
        );
        assert!(text.contains("usr/share/kicad/footprints"), "{text}");
        assert!(text.contains(install::SYMBOLS_VARIABLE), "{text}");
        assert!(text.contains(install::FOOTPRINTS_VARIABLE), "{text}");
    }

    #[test]
    fn a_project_library_is_not_shadowing_a_folder_of_the_guessed_root_when_no_kicad_is_found() {
        // With no install the root is a guess: here a folder that has a `own.pretty`, as the
        // working directory does when the root is `""`. It is no installed library.
        let dir = netsmith_kicad::testlib::TempDir::new("no-kicad-shadow-fp");
        let pretty = dir.path().join("own.pretty");
        std::fs::create_dir_all(&pretty).unwrap();
        let mut library = FootprintLibrary::at(dir.path()).with_library("own", &pretty);
        // An install that is found: the folder is installed, and the project's library clashes.
        let text = library.path_of("own:X").unwrap_err().to_string();
        assert!(text.contains("both installed"), "{text}");
        // No install found: no clash, the project's own library answers.
        let lookup = install::Search::under(dir.path())
            .with_no_kicad()
            .find(install::Kind::Footprints)
            .unwrap_err();
        library.no_install = Some(lookup);
        assert_eq!(
            library.path_of("own:X").unwrap(),
            pretty.join("X.kicad_mod")
        );
    }

    #[test]
    fn an_install_found_by_the_search_is_read() {
        let root = netsmith_kicad::testlib::TempDir::new("fake-mac-fp");
        let share = root
            .path()
            .join("Applications/KiCad/KiCad.app/Contents/SharedSupport");
        let footprints = share.join("footprints");
        std::fs::create_dir_all(&footprints).unwrap();
        copy_dir(&netsmith_kicad::testlib::sample_footprints(), &footprints);
        let library = FootprintLibrary::installed_by(&install::Search::under(root.path()));
        assert_eq!(library.root(), footprints);
    }

    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    #[test]
    fn a_generated_name_is_made_not_read_when_the_library_has_generators() {
        // The fixtures hold their own R_0402; the generators make theirs.
        let from_file = fixtures().load("Resistor_SMD:R_0402_1005Metric").unwrap();
        let made = fixtures()
            .with_generators()
            .load("Resistor_SMD:R_0402_1005Metric")
            .unwrap();
        assert_eq!(made.name, "R_0402_1005Metric");
        assert!(made.source.to_kicad_string().contains("netsmith"));
        assert!(
            !from_file
                .source
                .to_kicad_string()
                .contains("(generator \"netsmith\")")
        );
        // A name no generator makes is still read from the folder.
        let file_only = fixtures()
            .with_generators()
            .load("Package_TO_SOT_SMD:SOT-223-3_TabPin2")
            .unwrap();
        assert!(file_only.pads_numbered("2").count() == 2);
    }

    #[test]
    fn a_name_no_generator_makes_and_no_file_holds_says_where_it_looked() {
        let err = fixtures()
            .with_generators()
            .load("Resistor_SMD:R_9999")
            .unwrap_err();
        assert!(err.to_string().contains("Resistor_SMD.pretty"), "{err}");
    }

    #[test]
    fn a_project_library_named_like_a_generated_one_is_refused_with_both_named() {
        let library = fixtures()
            .with_generators()
            .with_library("Fuse", "/work/proj/libs/mine.pretty");
        let err = library.load("Fuse:Fuse_0603_1608Metric").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("`Fuse`"), "{text}");
        assert!(text.contains("netsmith generates"), "{text}");
        assert!(text.contains("/work/proj/libs/mine.pretty"), "{text}");
        // Without the generators the library is the project's own, as before.
        let plain = fixtures().with_library("Fuse", "/work/proj/libs/mine.pretty");
        assert!(matches!(
            plain.load("Fuse:Fuse_0603_1608Metric").unwrap_err(),
            FootprintError::NotFound { project: true, .. }
        ));
    }

    #[test]
    fn names_need_a_library() {
        assert_eq!(
            fixtures().load("R_0402").unwrap_err(),
            FootprintError::BadName("R_0402".into())
        );
    }

    #[test]
    fn unsupported_pad_shapes_are_refused_with_the_footprint_named() {
        let err = Footprint::parse(
            "L:X",
            r#"(footprint "X" (pad "1" smd trapezoid (at 0 0) (size 1 1)))"#,
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "footprint `L:X`: pad 1 has shape `trapezoid`, which is not supported"
        );
    }

    const CUSTOM: &str = r#"(footprint "Odd"
        (pad "1" smd custom (at 1 2 90) (size 0.5 0.4) (layers "F.Cu" "F.Mask")
            (options (clearance outline) (anchor rect))
            (primitives
                (gr_poly (pts (xy -0.25 -0.2) (xy 1.5 -0.2) (xy 1.5 0.2) (xy -0.25 0.2))
                    (width 0.1) (fill yes))
                (gr_line (start 0 0) (end 0 -1) (width 0.2))))
        (pad "2" smd rect (at 3 0) (size 1 1) (layers "F.Cu")))"#;

    #[test]
    fn a_custom_pad_keeps_its_anchor_and_its_box_takes_in_the_primitives() {
        let fp = Footprint::parse("L:Odd", CUSTOM).unwrap();
        let p = &fp.pads[0];
        assert_eq!(p.shape, PadShape::Rect);
        assert_eq!((p.width, p.height), (0.5, 0.4));
        // In the pad's frame the primitives span x -0.3..1.55 and y -1.1..0.25, wider than
        // the anchor; the pad's turn of 90 degrees and its place then apply.
        let b = p.bounds();
        let local = Rect::new(Pt::new(-0.3, -1.1), Pt::new(1.55, 0.25))
            .rotated(90.0)
            .translated(Pt::new(1.0, 2.0));
        assert!((b.min.x - local.min.x).abs() < 1e-9 && (b.max.y - local.max.y).abs() < 1e-9);
        assert!((b.max.x - local.max.x).abs() < 1e-9 && (b.min.y - local.min.y).abs() < 1e-9);
        // A pad of any other shape has no primitives and its box is its size.
        assert_eq!(fp.pads[1].extent, None);
        // The primitives stay in the source the board writer copies from.
        let first = fp.source.find_all("pad").next().unwrap();
        assert_eq!(first.find("primitives").unwrap().args().len(), 2);
        assert_eq!(fp.pads[1].bounds().max, Pt::new(3.5, 0.5));
    }

    #[test]
    fn a_custom_pad_without_an_anchor_is_a_circle_and_one_with_an_unknown_primitive_is_refused() {
        let fp = Footprint::parse(
            "L:X",
            r#"(footprint "X" (pad "1" smd custom (at 0 0) (size 1 1)))"#,
        )
        .unwrap();
        assert_eq!(fp.pads[0].shape, PadShape::Circle);
        assert_eq!(fp.pads[0].bounds().max, Pt::new(0.5, 0.5));
        let err = Footprint::parse(
            "L:X",
            r#"(footprint "X" (pad "1" smd custom (at 0 0) (size 1 1) (primitives (gr_text "x"))))"#,
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "footprint `L:X`: pad 1: primitive `gr_text` is not supported"
        );
    }

    #[test]
    fn the_courtyard_takes_in_a_hole_that_reaches_past_it() {
        let fp = Footprint::parse(
            "L:H",
            r#"(footprint "H"
                (pad "" np_thru_hole circle (at 3 0) (size 1 1) (drill 1) (layers "*.Cu"))
                (fp_rect (start -1 -1) (end 1 1) (layer "F.CrtYd")))"#,
        )
        .unwrap();
        assert_eq!(fp.courtyard.max, Pt::new(3.5, 1.0));
        assert_eq!(fp.courtyard.min, Pt::new(-1.0, -1.0));
    }

    #[test]
    fn rejects_text_that_is_not_a_footprint() {
        assert!(Footprint::parse("L:X", "(kicad_sch)").is_err());
        assert!(Footprint::parse("L:X", "(footprint").is_err());
    }
}
