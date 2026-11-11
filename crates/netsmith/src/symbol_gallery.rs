//! A page that shows every symbol netsmith draws itself beside the KiCad symbol it replaces
//! (ADR 0097), so that a person can review the look.
//!
//! The program is `cargo run -p netsmith --example symbol_gallery -- --out <folder>`; it writes
//! one self-contained page, `gallery.html` (inline SVG, no script, no network, a light and a
//! dark theme). The committed copy is `docs/symbol-gallery.html`.
//!
//! Three kinds of symbol are shown, each with netsmith's drawing on the left and KiCad's stock
//! symbol of the id it stands for on the right, at one scale and in one frame, so the two can be
//! laid over one another by eye:
//!
//! - every symbol of [`netsmith_kicad::passives`], in the order of `passives::ids()`: the page
//!   has no list of its own, so a symbol added there is in the gallery, and the test of this
//!   module fails if it is not;
//! - a few boxes of ICs that [`netsmith_kicad::generate`] draws from the pin table of a part of
//!   `examples/` ([`EXAMPLE_ICS`]), with KiCad's symbol of the same part where its library has
//!   one;
//! - where KiCad's symbol library is not installed (found as `netsmith_kicad::install`
//!   describes), a note in the place of every stock symbol.
//!
//! Both sides are drawn by the one function [`draw_symbol`] from the symbol's S-expression, as
//! KiCad's own files and netsmith's write it, so that a difference on the page is a difference
//! in the symbols and not in two drawing routines. It draws what the two kinds of symbol use:
//! rectangles, circles, arcs, polylines, Bezier curves and texts, the fills of `none`,
//! `outline` and `background`, the pin lines with their names and numbers, and the `Reference`
//! and `Value` fields. It does not draw: the pins' graphic styles (an inverted or clocked pin
//! is a plain line), the overbar of a `~{NAME}` (the name is shown without the markup), hidden
//! pins and hidden fields (as in KiCad), a second unit or a De Morgan body style (unit 1 and
//! style 1 are drawn), a text's font beyond its size, or a colour of a fill. The look of the
//! sheet (colours, the weight of a line of width 0) is the page's, not KiCad's.

use std::fmt::Write as _;
use std::path::Path;

use netsmith_core::Part;
use netsmith_kicad::{Symbol, SymbolLib, generate, install, passives, sexp::Sexp};

/// How the page is written again, said on the page.
pub const REGENERATE: &str =
    "cargo run -p netsmith --example symbol_gallery -- --out docs --file symbol-gallery.html";

/// An IC of `examples/` drawn as a box from its pin table: where its part is, and the id of the
/// KiCad symbol that stands for it, if KiCad has one.
pub struct ExampleIc {
    /// The folder of the example under `examples/` (a project), or `""` for a file there.
    pub project: &'static str,
    /// The source file: a project's top-level file is named by its manifest, so `""` takes the
    /// first one; a single-file example names it.
    pub file: &'static str,
    /// The part's definition: `Rp2350A`.
    pub kind: &'static str,
    /// What the section is called.
    pub title: &'static str,
    /// What the section says about the placement the box shows.
    pub note: &'static str,
    /// KiCad's symbol of the same part, if its library has one: `Regulator_Linear:AP1117-15`.
    pub kicad: Option<&'static str>,
    /// Why there is none, shown in its place.
    pub no_kicad: &'static str,
}

/// The ICs of the gallery: four real pin tables, chosen to show every placement rule of
/// `generate.rs` (inputs and ground, outputs, the rest in pin order).
pub const EXAMPLE_ICS: [ExampleIc; 4] = [
    ExampleIc {
        project: "rp2350_minimal",
        file: "",
        kind: "Rp2350A",
        title: "RP2350A",
        note: "A microcontroller with many I/O and several supplies, from the pin table of examples/rp2350_minimal.",
        kicad: None,
        no_kicad: "KiCad 9's MCU_RaspberryPi library has the RP2040 and no RP2350A.",
    },
    ExampleIc {
        project: "",
        file: "linear_regulator.nsm",
        kind: "LinearRegulator",
        title: "LinearRegulator",
        note: "A three-pin regulator, from the pin table of examples/linear_regulator.nsm: power in on the left, power out on the right, ground below. KiCad's AP1117-15 is a three-pin regulator of the same kind.",
        kicad: Some("Regulator_Linear:AP1117-15"),
        no_kicad: "",
    },
    ExampleIc {
        project: "orbit",
        file: "",
        kind: "Stm32F030C8",
        title: "STM32F030C8 (LQFP-48)",
        note: "Supplies and ground top and bottom, I/O left and right, from the pin table of examples/orbit.",
        kicad: Some("MCU_ST_STM32F0:STM32F030C8Tx"),
        no_kicad: "",
    },
    ExampleIc {
        project: "easyduino_esp32s3",
        file: "",
        kind: "Esp32S3Wroom1",
        title: "ESP32-S3-WROOM-1 module",
        note: "A radio module with supplies, ground and a few dozen I/O, from the pin table of examples/easyduino_esp32s3.",
        kicad: Some("RF_Module:ESP32-S3-WROOM-1"),
        no_kicad: "",
    },
];

/// One IC section of the page: the symbol netsmith draws from a pin table.
pub struct IcSection {
    pub title: String,
    pub note: String,
    /// The symbol netsmith draws.
    pub ours: Symbol,
    pub kicad: Option<String>,
    pub no_kicad: String,
}

/// The sections of the passives: the id of each symbol `passives` draws, in its order.
pub fn passive_ids() -> Vec<&'static str> {
    passives::ids().collect()
}

/// Where KiCad's symbol library is on this machine, as the other tools find it, or none.
pub fn installed_kicad() -> Option<SymbolLib> {
    match install::locate(install::Kind::Symbols) {
        // `at` and not `installed`: the generators are left out, so that `Device:R` is KiCad's.
        Ok(found) => Some(SymbolLib::at(found.dir)),
        Err(_) => None,
    }
}

/// The boxes of [`EXAMPLE_ICS`], drawn from the pin tables of the parts in `examples`, a copy
/// of the repository's `examples/` folder. The boards are checked and solved, as `build` does,
/// and read no KiCad library.
pub fn example_ics(examples: &Path) -> Result<Vec<IcSection>, String> {
    let mut out = Vec::new();
    for ic in EXAMPLE_ICS {
        let parts = parts_of(examples, ic.project, ic.file)?;
        let part = parts
            .iter()
            .find(|p| p.kind == ic.kind && !p.pins.is_empty())
            .ok_or_else(|| format!("no part `{}` in {}{}", ic.kind, ic.project, ic.file))?;
        let ours =
            generate::symbol(part, "").ok_or_else(|| format!("`{}` has no pin table", ic.kind))?;
        out.push(IcSection {
            title: ic.title.to_string(),
            note: ic.note.to_string(),
            ours,
            kicad: ic.kicad.map(str::to_string),
            no_kicad: ic.no_kicad.to_string(),
        });
    }
    Ok(out)
}

/// The parts of the first board of a project's top-level file or of a single file, with
/// their pin tables.
fn parts_of(examples: &Path, project: &str, file: &str) -> Result<Vec<Part>, String> {
    use crate::project::Target;
    let target = if project.is_empty() {
        // A named file keeps the spelling it was given, so it is given whole.
        let path = examples.join(file);
        Target::resolve_from(Path::new(""), path.to_str())?
    } else {
        Target::resolve_from(&examples.join(project), None)?
    };
    let source = target
        .files
        .first()
        .ok_or_else(|| format!("{project}{file}: no source file"))?;
    let text = std::fs::read_to_string(&source.path)
        .map_err(|e| format!("{}: {e}", source.path.display()))?;
    let outcome = crate::outcome_with(
        &source.name,
        &text,
        &target.loader()?,
        &target.catalogs()?,
        None,
    )
    .map_err(|problems| format!("{}: {} problems", source.name, problems.len()))?;
    let boards = outcome.boards.into_iter().map(|b| b.design);
    Ok(boards.flat_map(|d| d.parts).collect())
}

// ---------------------------------------------------------------------------------------
// The drawing

/// One SVG frame: millimetres, y down (a symbol's y is turned over).
#[derive(Clone, Copy)]
struct Bounds {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Bounds {
    const EMPTY: Bounds = Bounds {
        x0: f64::INFINITY,
        y0: f64::INFINITY,
        x1: f64::NEG_INFINITY,
        y1: f64::NEG_INFINITY,
    };

    fn add(&mut self, x: f64, y: f64) {
        self.x0 = self.x0.min(x);
        self.y0 = self.y0.min(y);
        self.x1 = self.x1.max(x);
        self.y1 = self.y1.max(y);
    }

    fn union(self, o: Bounds) -> Bounds {
        Bounds {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }

    fn is_empty(self) -> bool {
        self.x0 > self.x1
    }
}

/// A symbol drawn: the SVG elements and the frame they fill.
pub struct Drawing {
    body: String,
    bounds: Bounds,
    /// How many pins are drawn.
    pub pins: usize,
    /// The extent of the shapes and pins, without the texts, in mm.
    pub extent: (f64, f64),
}

/// The width KiCad gives a line that says `0`.
const DEFAULT_WIDTH: f64 = 0.1524;
/// The text height of a field or a pin that says none: 50 mil.
const TEXT: f64 = 1.27;

fn number(node: &Sexp, index: usize) -> f64 {
    node.args()
        .get(index)
        .and_then(Sexp::text)
        .and_then(|t| t.parse().ok())
        .unwrap_or(0.0)
}

fn point(node: &Sexp, name: &str) -> (f64, f64) {
    node.find(name)
        .map_or((0.0, 0.0), |p| (number(p, 0), number(p, 1)))
}

fn stroke_width(node: &Sexp) -> f64 {
    let w = node
        .find("stroke")
        .and_then(|s| s.find("width"))
        .map_or(0.0, |w| number(w, 0));
    if w <= 0.0 { DEFAULT_WIDTH } else { w }
}

fn fill_class(node: &Sexp) -> &'static str {
    match node
        .find("fill")
        .and_then(|f| f.find("type"))
        .and_then(|t| t.args().first())
        .and_then(Sexp::text)
    {
        Some("outline") => "fo",
        Some("background") => "fb",
        _ => "fn",
    }
}

/// Whether a node says `hide`: `(hide yes)` or the bare word, which older files write.
fn hides(node: &Sexp) -> bool {
    node.args().iter().any(|a| match a {
        Sexp::Atom(w) => w == "hide",
        Sexp::List(_) => {
            a.head() == Some("hide") && a.args().first().and_then(Sexp::text) != Some("no")
        }
        Sexp::Str(_) => false,
    }) || node.find("effects").is_some_and(hides)
}

fn text_size(node: &Sexp) -> f64 {
    node.find("effects")
        .and_then(|e| e.find("font"))
        .and_then(|f| f.find("size"))
        .map_or(TEXT, |s| number(s, 0))
        .max(0.1)
}

fn justify(node: &Sexp) -> Option<&str> {
    node.find("effects")?
        .find("justify")?
        .args()
        .iter()
        .filter_map(Sexp::text)
        .find(|w| matches!(*w, "left" | "right"))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn mm(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

struct Canvas {
    body: String,
    bounds: Bounds,
    /// The frame of the shapes and pins alone.
    shapes: Bounds,
}

impl Canvas {
    fn shape_point(&mut self, x: f64, y: f64, half_width: f64) {
        self.bounds.add(x - half_width, y - half_width);
        self.bounds.add(x + half_width, y + half_width);
        self.shapes.add(x - half_width, y - half_width);
        self.shapes.add(x + half_width, y + half_width);
    }

    /// A text at (`x`, `y`) of the sheet (y down). `anchor` is `start`, `middle` or `end`
    /// along the text; `vertical` reads upward.
    fn text(
        &mut self,
        class: &str,
        text: &str,
        at: (f64, f64),
        size: f64,
        anchor: &str,
        vertical: bool,
    ) {
        if text.is_empty() {
            return;
        }
        let (x, y) = at;
        let length = text.chars().count() as f64 * size * 0.58;
        let (from, to) = match anchor {
            "start" => (0.0, length),
            "end" => (-length, 0.0),
            _ => (-length / 2.0, length / 2.0),
        };
        let half = size / 2.0;
        if vertical {
            self.bounds.add(x - half, y - to);
            self.bounds.add(x + half, y - from);
        } else {
            self.bounds.add(x + from, y - half);
            self.bounds.add(x + to, y + half);
        }
        let turn = if vertical {
            format!(" transform=\"rotate(-90 {} {})\"", mm(x), mm(y))
        } else {
            String::new()
        };
        let _ = writeln!(
            self.body,
            "<text class=\"{class}\" x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"{anchor}\"{turn}>{}</text>",
            mm(x),
            mm(y),
            mm(size),
            escape(text)
        );
    }
}

/// `~{NAME}` is `NAME` with a bar over it; the page shows the name without the markup.
fn plain_text(name: &str) -> String {
    name.replace("~{", "").replace('}', "")
}

/// The circle through three points, with the way it turns from the first to the last
/// (counterclockwise, y up) and whether that is more than a half turn: `(center, radius,
/// counterclockwise, large)`.
fn arc_through(
    s: (f64, f64),
    m: (f64, f64),
    e: (f64, f64),
) -> Option<((f64, f64), f64, bool, bool)> {
    let d = 2.0 * (s.0 * (m.1 - e.1) + m.0 * (e.1 - s.1) + e.0 * (s.1 - m.1));
    if d.abs() < 1e-9 {
        return None;
    }
    let sq = |p: (f64, f64)| p.0 * p.0 + p.1 * p.1;
    let ux = (sq(s) * (m.1 - e.1) + sq(m) * (e.1 - s.1) + sq(e) * (s.1 - m.1)) / d;
    let uy = (sq(s) * (e.0 - m.0) + sq(m) * (s.0 - e.0) + sq(e) * (m.0 - s.0)) / d;
    let radius = ((s.0 - ux).powi(2) + (s.1 - uy).powi(2)).sqrt();
    let cross = (m.0 - s.0) * (e.1 - m.1) - (m.1 - s.1) * (e.0 - m.0);
    let counterclockwise = cross > 0.0;
    let angle = |p: (f64, f64)| (p.1 - uy).atan2(p.0 - ux);
    let tau = std::f64::consts::TAU;
    let turn = if counterclockwise {
        (angle(e) - angle(s)).rem_euclid(tau)
    } else {
        (angle(s) - angle(e)).rem_euclid(tau)
    };
    Some((
        (ux, uy),
        radius,
        counterclockwise,
        turn > std::f64::consts::PI,
    ))
}

fn draw_graphics(canvas: &mut Canvas, part: &Sexp) {
    for item in part.args() {
        let width = stroke_width(item);
        let fill = fill_class(item);
        let line_class = format!("shape {fill}");
        match item.head() {
            Some("rectangle") => {
                let (a, b) = (point(item, "start"), point(item, "end"));
                let (x, y) = (a.0.min(b.0), -a.1.max(b.1));
                let (w, h) = ((a.0 - b.0).abs(), (a.1 - b.1).abs());
                let _ = writeln!(
                    canvas.body,
                    "<rect class=\"{line_class}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" stroke-width=\"{}\"/>",
                    mm(x),
                    mm(y),
                    mm(w),
                    mm(h),
                    mm(width)
                );
                canvas.shape_point(a.0, -a.1, width / 2.0);
                canvas.shape_point(b.0, -b.1, width / 2.0);
            }
            Some("circle") => {
                let c = point(item, "center");
                let r = item.find("radius").map_or(0.0, |r| number(r, 0));
                let _ = writeln!(
                    canvas.body,
                    "<circle class=\"{line_class}\" cx=\"{}\" cy=\"{}\" r=\"{}\" stroke-width=\"{}\"/>",
                    mm(c.0),
                    mm(-c.1),
                    mm(r),
                    mm(width)
                );
                canvas.shape_point(c.0 - r, -c.1 - r, width / 2.0);
                canvas.shape_point(c.0 + r, -c.1 + r, width / 2.0);
            }
            Some("polyline") => {
                let pts: Vec<(f64, f64)> = item
                    .find("pts")
                    .map(|p| {
                        p.find_all("xy")
                            .map(|xy| (number(xy, 0), number(xy, 1)))
                            .collect()
                    })
                    .unwrap_or_default();
                if pts.len() < 2 {
                    continue;
                }
                let list: Vec<String> = pts
                    .iter()
                    .map(|&(x, y)| format!("{},{}", mm(x), mm(-y)))
                    .collect();
                let _ = writeln!(
                    canvas.body,
                    "<polyline class=\"{line_class}\" points=\"{}\" stroke-width=\"{}\"/>",
                    list.join(" "),
                    mm(width)
                );
                for &(x, y) in &pts {
                    canvas.shape_point(x, -y, width / 2.0);
                }
            }
            Some("arc") => {
                let (s, m, e) = (point(item, "start"), point(item, "mid"), point(item, "end"));
                let path = match arc_through(s, m, e) {
                    Some((_, r, ccw, large)) => format!(
                        "M {} {} A {} {} 0 {} {} {} {}",
                        mm(s.0),
                        mm(-s.1),
                        mm(r),
                        mm(r),
                        u8::from(large),
                        // y is turned over: a counterclockwise turn is sweep 0 on the sheet.
                        u8::from(!ccw),
                        mm(e.0),
                        mm(-e.1)
                    ),
                    None => format!("M {} {} L {} {}", mm(s.0), mm(-s.1), mm(e.0), mm(-e.1)),
                };
                let _ = writeln!(
                    canvas.body,
                    "<path class=\"{line_class}\" d=\"{path}\" stroke-width=\"{}\"/>",
                    mm(width)
                );
                for p in [s, m, e] {
                    canvas.shape_point(p.0, -p.1, width / 2.0);
                }
            }
            Some("bezier") => {
                let pts: Vec<(f64, f64)> = item
                    .find("pts")
                    .map(|p| {
                        p.find_all("xy")
                            .map(|xy| (number(xy, 0), number(xy, 1)))
                            .collect()
                    })
                    .unwrap_or_default();
                if pts.len() != 4 {
                    continue;
                }
                let p = |i: usize| format!("{} {}", mm(pts[i].0), mm(-pts[i].1));
                let _ = writeln!(
                    canvas.body,
                    "<path class=\"{line_class}\" d=\"M {} C {}, {}, {}\" stroke-width=\"{}\"/>",
                    p(0),
                    p(1),
                    p(2),
                    p(3),
                    mm(width)
                );
                for &(x, y) in &pts {
                    canvas.shape_point(x, -y, width / 2.0);
                }
            }
            Some("text") => {
                let value = item.args().first().and_then(Sexp::text).unwrap_or("");
                let at = item
                    .find("at")
                    .map_or((0.0, 0.0), |a| (number(a, 0), number(a, 1)));
                let rotation = item.find("at").map_or(0.0, |a| number(a, 2));
                canvas.text(
                    "body-text",
                    value,
                    (at.0, -at.1),
                    text_size(item),
                    "middle",
                    rotation.rem_euclid(180.0) > 45.0,
                );
            }
            _ => {}
        }
    }
}

/// What a symbol says about how its pins show their names and numbers.
struct PinShow {
    /// How far a name is from the body's end of the pin; `None` is KiCad's 0.508 mm.
    offset: f64,
    names: bool,
    numbers: bool,
}

fn pin_show(symbol: &Sexp) -> PinShow {
    let names = symbol.find("pin_names");
    let numbers = symbol.find("pin_numbers");
    PinShow {
        offset: names
            .and_then(|n| n.find("offset"))
            .map_or(0.508, |o| number(o, 0)),
        names: !names.is_some_and(hides),
        numbers: !numbers.is_some_and(hides),
    }
}

fn draw_pin(canvas: &mut Canvas, pin: &Sexp, show: &PinShow) {
    if hides(pin) {
        return;
    }
    let at = pin.find("at");
    let (x, y) = at.map_or((0.0, 0.0), |a| (number(a, 0), number(a, 1)));
    let angle = at.map_or(0.0, |a| number(a, 2)).rem_euclid(360.0);
    let length = pin.find("length").map_or(0.0, |l| number(l, 0));
    let (sin, cos) = angle.to_radians().sin_cos();
    let (ex, ey) = (x + cos * length, y + sin * length);
    let _ = writeln!(
        canvas.body,
        "<line class=\"pin\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke-width=\"{}\"/>",
        mm(x),
        mm(-y),
        mm(ex),
        mm(-ey),
        mm(stroke_width(pin).max(DEFAULT_WIDTH))
    );
    let _ = writeln!(
        canvas.body,
        "<circle class=\"end\" cx=\"{}\" cy=\"{}\" r=\"0.2\"/>",
        mm(x),
        mm(-y)
    );
    canvas.shape_point(x, -y, 0.1);
    canvas.shape_point(ex, -ey, 0.1);
    let vertical = (angle - 90.0).abs() < 1.0 || (angle - 270.0).abs() < 1.0;
    let (mx, my) = ((x + ex) / 2.0, (y + ey) / 2.0);
    let name = pin
        .find("name")
        .and_then(|n| n.args().first())
        .and_then(Sexp::text)
        .map(plain_text)
        .filter(|n| n != "~")
        .unwrap_or_default();
    let number_text = pin
        .find("number")
        .and_then(|n| n.args().first())
        .and_then(Sexp::text)
        .unwrap_or("");
    let name_size = pin.find("name").map_or(TEXT, text_size);
    let number_size = pin.find("number").map_or(TEXT, text_size);
    let outside = show.offset <= 1e-9;
    if show.names && !name.is_empty() {
        if outside {
            // A name with no offset is above the line (left of it, for a vertical pin).
            let gap = name_size * 0.15 + stroke_width(pin) / 2.0;
            let spot = if vertical {
                (mx - gap - name_size / 2.0, -my)
            } else {
                (mx, -my - gap - name_size / 2.0)
            };
            canvas.text("pin-name", &name, spot, name_size, "middle", vertical);
        } else {
            // Inside: past the body's end of the line, by the offset.
            let (bx, by) = (ex + cos * show.offset, ey + sin * show.offset);
            let (anchor, vertical_text) = match angle as i32 {
                0 => ("start", false),
                180 => ("end", false),
                90 => ("start", true),
                270 => ("end", true),
                _ => ("start", false),
            };
            canvas.text(
                "pin-name",
                &name,
                (bx, -by),
                name_size,
                anchor,
                vertical_text,
            );
        }
    }
    if show.numbers && !number_text.is_empty() {
        let gap = number_size * 0.15 + stroke_width(pin) / 2.0;
        let (spot, anchor) = if outside {
            // Below the line (right of it, for a vertical pin).
            if vertical {
                ((mx + gap + number_size / 2.0, -my), "middle")
            } else {
                ((mx, -my + gap + number_size / 2.0), "middle")
            }
        } else if vertical {
            ((mx - gap - number_size / 2.0, -my), "middle")
        } else {
            ((mx, -my - gap - number_size / 2.0), "middle")
        };
        canvas.text(
            "pin-number",
            number_text,
            spot,
            number_size,
            anchor,
            vertical,
        );
    }
}

fn draw_field(canvas: &mut Canvas, symbol: &Sexp, name: &str) {
    let Some(prop) = symbol
        .find_all("property")
        .find(|p| p.args().first().and_then(Sexp::text) == Some(name))
    else {
        return;
    };
    if hides(prop) {
        return;
    }
    let text = prop.args().get(1).and_then(Sexp::text).unwrap_or("");
    let at = prop.find("at");
    let (x, y) = at.map_or((0.0, 0.0), |a| (number(a, 0), number(a, 1)));
    let rotation = at.map_or(0.0, |a| number(a, 2)).rem_euclid(180.0);
    let anchor = match justify(prop) {
        Some("left") => "start",
        Some("right") => "end",
        _ => "middle",
    };
    canvas.text(
        "field",
        text,
        (x, -y),
        text_size(prop),
        anchor,
        rotation > 45.0,
    );
}

/// Draw a symbol: `definition` is its `(symbol "<name>" ...)`, flattened, as
/// [`Symbol::definition`] gives it. Unit 1 and the first body style are drawn.
pub fn draw_symbol(definition: &Sexp) -> Drawing {
    let mut canvas = Canvas {
        body: String::new(),
        bounds: Bounds::EMPTY,
        shapes: Bounds::EMPTY,
    };
    let show = pin_show(definition);
    // `<name>_<unit>_<style>`: unit 0 and style 0 are drawn with every unit and style. The
    // name in front is the symbol's own or the library's, so it is read from the right.
    let parts: Vec<&Sexp> = definition
        .find_all("symbol")
        .filter(|p| {
            let id = p.args().first().and_then(Sexp::text).unwrap_or("");
            let mut it = id.rsplit('_');
            let (style, unit) = (it.next(), it.next());
            matches!(unit, Some("0" | "1")) && matches!(style, Some("0" | "1"))
        })
        .collect();
    for part in &parts {
        draw_graphics(&mut canvas, part);
    }
    let mut pins = 0;
    for part in &parts {
        for pin in part.find_all("pin") {
            if !hides(pin) {
                pins += 1;
            }
            draw_pin(&mut canvas, pin, &show);
        }
    }
    draw_field(&mut canvas, definition, "Reference");
    draw_field(&mut canvas, definition, "Value");
    let shapes = canvas.shapes;
    let extent = if shapes.is_empty() {
        (0.0, 0.0)
    } else {
        (shapes.x1 - shapes.x0, shapes.y1 - shapes.y0)
    };
    Drawing {
        body: canvas.body,
        bounds: canvas.bounds,
        pins,
        extent,
    }
}

/// The SVG of `drawing` in the frame `bounds`, with a margin, at `px_per_mm`. The size is the
/// frame's, so two symbols drawn in one frame are at one scale.
fn svg(drawing: &Drawing, frame: Bounds, px_per_mm: f64, label: &str) -> String {
    let margin = 1.5;
    let (x0, y0) = (frame.x0 - margin, frame.y0 - margin);
    let (w, h) = (
        frame.x1 - frame.x0 + 2.0 * margin,
        frame.y1 - frame.y0 + 2.0 * margin,
    );
    format!(
        "<svg class=\"symbol\" xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" \
         style=\"width:{}px\"{} role=\"img\" aria-label=\"{}\">\n{}</svg>",
        mm(x0),
        mm(y0),
        mm(w),
        mm(h),
        mm(w * px_per_mm),
        // A wide symbol (an IC with names) keeps a readable size on a phone and scrolls.
        if w > WIDE { " data-wide=\"1\"" } else { "" },
        escape(label),
        drawing.body
    )
}

// ---------------------------------------------------------------------------------------
// The page

/// The page's sections, in groups.
fn group_of(id: &str) -> &'static str {
    let library = id.split_once(':').map_or("", |(l, _)| l);
    match (library, id) {
        ("Device" | "Switch" | "Transistor_FET", _) => "Passives and discrete parts",
        (_, "Connector:TestPoint") => "Mechanical parts and the power flag",
        (l, _) if l.starts_with("Connector") => "Connectors",
        _ => "Mechanical parts and the power flag",
    }
}

const GROUPS: [&str; 3] = [
    "Passives and discrete parts",
    "Connectors",
    "Mechanical parts and the power flag",
];

const STYLE: &str = r#"
:root {
  --bg: #f6f5f1; --card: #ffffff; --ink: #1d1d1b; --muted: #5d5b55; --rule: #dcd9d0;
  --body: #840000; --fill: #ffffc2; --pin: #840000; --end: #840000; --name: #006464;
  --number: #a90000; --field: #006464; --ours: #1b6b3a; --stock: #37507a; --note-bg: #f1efe8;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --bg: #15161a; --card: #1f2127; --ink: #e8e6df; --muted: #a09d94; --rule: #353841;
    --body: #ff6f6f; --fill: #3a3720; --pin: #ff6f6f; --end: #ff6f6f; --name: #5fd0d0;
    --number: #ff9a9a; --field: #5fd0d0; --ours: #6fd394; --stock: #8fb0e6; --note-bg: #2a2d35;
  }
}
:root[data-theme="dark"] {
  --bg: #15161a; --card: #1f2127; --ink: #e8e6df; --muted: #a09d94; --rule: #353841;
  --body: #ff6f6f; --fill: #3a3720; --pin: #ff6f6f; --end: #ff6f6f; --name: #5fd0d0;
  --number: #ff9a9a; --field: #5fd0d0; --ours: #6fd394; --stock: #8fb0e6; --note-bg: #2a2d35;
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--ink);
  font: 16px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif; }
main { max-width: 1120px; margin: 0 auto; padding: 24px 16px 64px; }
h1 { font-size: 1.6rem; margin: 0 0 8px; }
h2 { font-size: 1.25rem; margin: 40px 0 12px; padding-bottom: 6px; border-bottom: 1px solid var(--rule); }
h3 { font-size: 1.05rem; margin: 0; }
p { margin: 0 0 8px; }
code { font: 0.9em ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; overflow-wrap: anywhere; }
nav { display: flex; flex-wrap: wrap; gap: 8px 16px; margin: 16px 0 0; }
nav a { color: var(--stock); }
.lede { color: var(--muted); max-width: 70ch; }
.status { background: var(--note-bg); border-radius: 6px; padding: 8px 12px; margin: 16px 0 0; }
section.sym { background: var(--card); border: 1px solid var(--rule); border-radius: 8px;
  padding: 12px 16px 16px; margin: 0 0 16px; }
section.sym header { margin-bottom: 8px; }
.ids { color: var(--muted); font-size: 0.9rem; }
.pair { display: flex; flex-wrap: wrap; gap: 16px; }
figure { margin: 0; flex: 1 1 280px; min-width: 0; }
.pair.single figure { flex-basis: 100%; }
figcaption { font-size: 0.85rem; font-weight: 600; margin: 0 0 4px; }
figure.ours figcaption { color: var(--ours); }
figure.stock figcaption { color: var(--stock); }
figcaption small { font-weight: 400; color: var(--muted); }
svg.symbol { display: block; margin: 0 auto; max-width: 100%; height: auto; border: 1px dashed var(--rule);
  border-radius: 4px; background: var(--bg); }
.shape { stroke: var(--body); stroke-linecap: round; stroke-linejoin: round; }
.shape.fn { fill: none; } .shape.fb { fill: var(--fill); } .shape.fo { fill: var(--body); }
.pin { stroke: var(--pin); stroke-linecap: round; }
.end { fill: var(--end); }
text { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; dominant-baseline: central; }
.pin-name { fill: var(--name); } .pin-number { fill: var(--number); }
.field { fill: var(--field); } .body-text { fill: var(--body); }
.missing { border: 1px dashed var(--rule); border-radius: 4px; padding: 12px; background: var(--note-bg);
  color: var(--muted); font-size: 0.9rem; min-height: 4rem; display: flex; align-items: center; }
footer { margin-top: 40px; color: var(--muted); font-size: 0.85rem; }
@media (max-width: 600px) {
  main { padding: 16px; } h1 { font-size: 1.35rem; }
  figure { overflow-x: auto; }
  svg.symbol[data-wide] { max-width: none; min-width: 560px; }
}
"#;

const NO_LIBRARY: &str = "KiCad library not found: no stock counterpart drawn";

/// The most pixels a millimetre is drawn with, and the most width a figure gets.
const MAX_SCALE: f64 = 24.0;
const MAX_WIDTH: f64 = 520.0;
const MAX_SINGLE_WIDTH: f64 = 1000.0;
/// A frame wider than this many mm is a wide symbol.
const WIDE: f64 = 28.0;

/// The scale of a frame: [`MAX_SCALE`], and less where the frame is wider than [`MAX_WIDTH`]
/// (or [`MAX_SINGLE_WIDTH`], for a symbol that has the row to itself).
fn scale_of(frame: Bounds, single: bool) -> f64 {
    let width = (frame.x1 - frame.x0 + 3.0).max(1.0);
    let most = if single { MAX_SINGLE_WIDTH } else { MAX_WIDTH };
    MAX_SCALE.min(most / width)
}

fn dimensions(d: &Drawing) -> String {
    format!(
        "{} pins, {} &times; {} mm",
        d.pins,
        mm((d.extent.0 * 10.0).round() / 10.0),
        mm((d.extent.1 * 10.0).round() / 10.0)
    )
}

enum Stock<'a> {
    /// The library says nothing: no KiCad.
    NoLibrary,
    /// KiCad has no symbol of the id.
    Absent(String),
    Symbol(&'a Symbol),
}

fn pair(ours: &Symbol, stock: Stock<'_>, ours_label: &str) -> String {
    let drawn = draw_symbol(ours.definition());
    let theirs = match &stock {
        Stock::Symbol(s) => Some(draw_symbol(s.definition())),
        _ => None,
    };
    let frame = match &theirs {
        Some(t) => drawn.bounds.union(t.bounds),
        None => drawn.bounds,
    };
    let single = theirs.is_none();
    let scale = scale_of(frame, single);
    let mut out = String::from(if single {
        "<div class=\"pair single\">\n"
    } else {
        "<div class=\"pair\">\n"
    });
    let _ = writeln!(
        out,
        "<figure class=\"ours\" data-side=\"netsmith\"><figcaption>netsmith <small>{}</small></figcaption>\n{}</figure>",
        dimensions(&drawn),
        svg(&drawn, frame, scale, ours_label)
    );
    match (&stock, theirs) {
        (Stock::Symbol(s), Some(t)) => {
            let _ = writeln!(
                out,
                "<figure class=\"stock\" data-side=\"kicad\"><figcaption>KiCad <small>{} &middot; <code>{}</code></small></figcaption>\n{}</figure>",
                dimensions(&t),
                escape(s.lib_id()),
                svg(&t, frame, scale, "KiCad's symbol")
            );
        }
        (Stock::Absent(why), _) => {
            let _ = writeln!(
                out,
                "<figure class=\"stock\" data-side=\"kicad-absent\"><figcaption>KiCad</figcaption><div class=\"missing\">{}</div></figure>",
                escape(why)
            );
        }
        _ => {
            let _ = writeln!(
                out,
                "<figure class=\"stock\" data-side=\"kicad-absent\"><figcaption>KiCad</figcaption><div class=\"missing\">{NO_LIBRARY}</div></figure>"
            );
        }
    }
    out.push_str("</div>\n");
    out
}

fn anchor_of(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// What KiCad says of an id: its symbol, or why it is not shown.
fn stock_of(kicad: Option<&SymbolLib>, id: Option<&str>, no_id: &str) -> StockOwned {
    let Some(lib) = kicad else {
        return StockOwned::NoLibrary;
    };
    let Some(id) = id else {
        return StockOwned::Absent(no_id.to_string());
    };
    match lib.symbol(id) {
        Ok(s) => StockOwned::Symbol(s),
        Err(e) => StockOwned::Absent(format!("KiCad's library has no symbol `{id}` ({e})")),
    }
}

enum StockOwned {
    NoLibrary,
    Absent(String),
    Symbol(std::sync::Arc<Symbol>),
}

impl StockOwned {
    fn borrowed(&self) -> Stock<'_> {
        match self {
            StockOwned::NoLibrary => Stock::NoLibrary,
            StockOwned::Absent(w) => Stock::Absent(w.clone()),
            StockOwned::Symbol(s) => Stock::Symbol(s),
        }
    }
}

/// The page: every symbol of `passives`, then `ics`, each beside KiCad's symbol from `kicad`
/// where there is one. It is the same text for the same inputs.
pub fn render(kicad: Option<&SymbolLib>, ics: &[IcSection]) -> String {
    let mut out = String::new();
    out.push_str(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\" />\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n\
         <meta name=\"color-scheme\" content=\"light dark\" />\n\
         <title>Symbol gallery</title>\n<style>",
    );
    out.push_str(STYLE);
    out.push_str("</style>\n</head>\n<body>\n<main>\n");
    out.push_str("<h1>netsmith symbol gallery</h1>\n");
    out.push_str(
        "<p class=\"lede\">Every symbol netsmith draws itself (ADR 0097), beside the KiCad symbol \
         it replaces, at one scale and in one frame. Left, netsmith's drawing; right, KiCad 9's \
         stock symbol of the id the mapping names. Pin numbers are red, pin names teal, the pin's \
         connection point is a dot, and the <code>Reference</code> and <code>Value</code> fields \
         are where each symbol puts them.</p>\n",
    );
    let _ = writeln!(
        out,
        "<p class=\"lede\">This page is generated: <code>{}</code>. Do not edit it.</p>",
        escape(REGENERATE)
    );
    let _ = writeln!(
        out,
        "<p class=\"status\">KiCad's symbol library: {}.</p>",
        if kicad.is_some() {
            "found when this page was written; each stock symbol is drawn from it"
        } else {
            "not found when this page was written, so no stock symbol is drawn"
        }
    );
    out.push_str("<nav aria-label=\"Groups\">\n");
    for group in GROUPS {
        let _ = writeln!(
            out,
            "<a href=\"#{}\">{}</a>",
            anchor_of(group),
            escape(group)
        );
    }
    if !ics.is_empty() {
        out.push_str("<a href=\"#ics\">ICs drawn from a pin table</a>\n");
    }
    out.push_str("</nav>\n");

    for group in GROUPS {
        let _ = writeln!(
            out,
            "<h2 id=\"{}\">{}</h2>",
            anchor_of(group),
            escape(group)
        );
        for id in passives::ids().filter(|id| group_of(id) == group) {
            let Some(ours) = passives::symbol(id) else {
                continue;
            };
            let bare = id.split_once(':').map_or(id, |(_, n)| n);
            let description = ours
                .definition()
                .find_all("property")
                .find(|p| p.args().first().and_then(Sexp::text) == Some("Description"))
                .and_then(|p| p.args().get(1).and_then(Sexp::text))
                .unwrap_or("");
            let _ = writeln!(
                out,
                "<section class=\"sym\" id=\"sym-{}\" data-symbol=\"{}\">\n<header><h3>{}</h3>\n\
                 <p class=\"ids\"><code>{}</code> is drawn as <code>{}</code>. {}</p></header>",
                anchor_of(id),
                escape(id),
                escape(bare),
                escape(id),
                escape(ours.lib_id()),
                escape(description)
            );
            let stock = stock_of(kicad, Some(id), "");
            out.push_str(&pair(
                &ours,
                stock.borrowed(),
                &format!("netsmith's {bare}"),
            ));
            out.push_str("</section>\n");
        }
    }

    if !ics.is_empty() {
        out.push_str("<h2 id=\"ics\">ICs drawn from a pin table</h2>\n");
        out.push_str(
            "<p class=\"lede\">A part with no symbol of its own is drawn as a box from its pin \
             table (<code>generate.rs</code>): power in and ground on the left, power out on the \
             right, the rest in pin order. These are the pin tables of four parts of \
             <code>examples/</code>.</p>\n",
        );
        for ic in ics {
            let _ = writeln!(
                out,
                "<section class=\"sym ic\" id=\"ic-{}\" data-ic=\"{}\">\n<header><h3>{}</h3>\n<p class=\"ids\">{}</p></header>",
                anchor_of(&ic.title),
                escape(&ic.title),
                escape(&ic.title),
                escape(&ic.note)
            );
            let stock = stock_of(kicad, ic.kicad.as_deref(), &ic.no_kicad);
            out.push_str(&pair(&ic.ours, stock.borrowed(), &ic.title));
            out.push_str("</section>\n");
        }
    }

    out.push_str(
        "<footer>The stock symbols are drawings of KiCad's symbol libraries, \
         &copy; the KiCad Library Team, CC-BY-SA 4.0 (with the exception for designs that use \
         them); netsmith's own symbols are drawn from the electrical and mechanical conventions, \
         not from KiCad's files. What the page does not draw: pin graphic styles (an inverted \
         or clocked pin is a plain line), overbars of <code>~{NAME}</code>, hidden pins and \
         fields, units after the first, and a text's font.</footer>\n",
    );
    out.push_str("</main>\n</body>\n</html>\n");
    out
}
