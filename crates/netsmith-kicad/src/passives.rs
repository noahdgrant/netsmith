//! The symbols of the passives and discrete parts of the standard mapping, drawn by netsmith
//! (ADR 0097, step 2): a resistor, a capacitor, an inductor, a ferrite bead, a fuse, a crystal,
//! an LED and four diodes, a switch and the N and P FETs. A mapping row that names one of
//! these ids (`Device:R`, `Switch:SW_SPST`, `Transistor_FET:Q_NMOS_GSD`) is answered here, from
//! the binary, on every machine and whatever KiCad has installed, as the footprints of
//! `netsmith_pcb::generate` are (ADR 0043). No mapping file changes. The symbol that answers
//! has the id `netsmith:<Name>` ([`answer_id`]) and goes into the board's own
//! `netsmith.kicad_sym` like the symbol of an IC drawn from its pin table.
//!
//! # What is fixed and what is the look
//!
//! A symbol has two parts, and they are kept apart on purpose.
//!
//! - **The geometry** every other part of netsmith measures: the number, name and electrical
//!   type of each pin, where it is and which way it runs, its length, the position of the
//!   `Reference` and `Value` fields, whether the pin numbers and names are shown, and the
//!   extent of the body (the layout engine routes round it, and puts the texts beside it).
//!   They are the written constants of [`SPECS`] and of the glyph functions' extents, equal
//!   to those of the symbol with the same id in KiCad 9's stock libraries (the test
//!   `passives_agree_with_the_installed_kicad_symbols` of
//!   `crates/netsmith/tests/passives.rs` proves it where KiCad is installed). A pin that moved by a grid step
//!   would move every wire the schematic view draws to it.
//! - **The look**: what is drawn inside that extent. It is the glyph function of one symbol
//!   ([`resistor`], [`capacitor`], ...), which returns its [`Shape`]s, and the three line
//!   weights ([`THIN`], [`BODY`], [`HEAVY`]) and fills at the top of the file. To change how a
//!   resistor looks, change [`resistor`] and nothing else. The shapes are drawn from the
//!   electrical conventions (IEC 60617, IEEE 315) and the grid, not from KiCad's symbol files,
//!   which are licensed CC-BY-SA 4.0 (ADR 0097). A body is a rectangle with the background
//!   fill and a line of [`BODY`] weight, as the box of an IC is (`generate.rs`).
//!
//! The same id gives the same bytes: nothing here reads a clock, a map's order or the machine.

use crate::sexp::Sexp;
use crate::symbols::{PinType, Symbol};

/// A point of a symbol, mm, y up.
type P = (f64, f64);

/// The default line weight of the sheet, for leads and thin features.
const THIN: f64 = 0.0;
/// The weight of a body outline.
const BODY: f64 = 0.254;
/// The weight of a capacitor's plates and of a crystal's.
const HEAVY: f64 = 0.508;

/// The fill of a closed shape.
#[derive(Clone, Copy)]
enum Fill {
    /// Not filled: the lines behind show.
    None,
    /// Filled with the sheet's background, as an IC's box is.
    Background,
    /// Filled with the line's colour.
    Solid,
}

impl Fill {
    fn word(self) -> &'static str {
        match self {
            Fill::None => "none",
            Fill::Background => "background",
            Fill::Solid => "outline",
        }
    }
}

/// One drawn shape of a glyph.
enum Shape {
    Line {
        pts: Vec<P>,
        width: f64,
        fill: Fill,
    },
    Rect {
        a: P,
        b: P,
        width: f64,
        fill: Fill,
    },
    Circle {
        center: P,
        radius: f64,
        width: f64,
        fill: Fill,
    },
    Arc {
        start: P,
        mid: P,
        end: P,
        width: f64,
    },
}

fn line(pts: &[P], width: f64) -> Shape {
    Shape::Line {
        pts: pts.to_vec(),
        width,
        fill: Fill::None,
    }
}

fn polygon(pts: &[P], width: f64, fill: Fill) -> Shape {
    let mut closed = pts.to_vec();
    closed.push(pts[0]);
    Shape::Line {
        pts: closed,
        width,
        fill,
    }
}

fn boxed(a: P, b: P) -> Shape {
    Shape::Rect {
        a,
        b,
        width: BODY,
        fill: Fill::Background,
    }
}

fn dot(center: P) -> Shape {
    Shape::Circle {
        center,
        radius: 0.254,
        width: THIN,
        fill: Fill::Solid,
    }
}

fn stroke(width: f64) -> Sexp {
    Sexp::node(
        "stroke",
        vec![
            Sexp::node("width", vec![Sexp::num(width)]),
            Sexp::node("type", vec![Sexp::atom("default")]),
        ],
    )
}

fn fill(kind: Fill) -> Sexp {
    Sexp::node(
        "fill",
        vec![Sexp::node("type", vec![Sexp::atom(kind.word())])],
    )
}

fn xy(p: P) -> Sexp {
    Sexp::node("xy", vec![Sexp::num(p.0), Sexp::num(p.1)])
}

fn point(name: &str, p: P) -> Sexp {
    Sexp::node(name, vec![Sexp::num(p.0), Sexp::num(p.1)])
}

impl Shape {
    fn node(&self) -> Sexp {
        match self {
            Shape::Line {
                pts,
                width,
                fill: f,
            } => Sexp::node(
                "polyline",
                vec![
                    Sexp::node("pts", pts.iter().map(|&p| xy(p)).collect()),
                    stroke(*width),
                    fill(*f),
                ],
            ),
            Shape::Rect {
                a,
                b,
                width,
                fill: f,
            } => Sexp::node(
                "rectangle",
                vec![
                    point("start", *a),
                    point("end", *b),
                    stroke(*width),
                    fill(*f),
                ],
            ),
            Shape::Circle {
                center,
                radius,
                width,
                fill: f,
            } => Sexp::node(
                "circle",
                vec![
                    point("center", *center),
                    Sexp::node("radius", vec![Sexp::num(*radius)]),
                    stroke(*width),
                    fill(*f),
                ],
            ),
            Shape::Arc {
                start,
                mid,
                end,
                width,
            } => Sexp::node(
                "arc",
                vec![
                    point("start", *start),
                    point("mid", *mid),
                    point("end", *end),
                    stroke(*width),
                    fill(Fill::None),
                ],
            ),
        }
    }
}

// ---------------------------------------------------------------------------------------
// The glyphs: the look of each symbol. Everything a glyph draws stays inside the extent
// its `Spec` documents, and leaves the pins' connection points free.

/// A resistor: a rectangle (IEC 60617). Extent x ±1.016, y ±2.54.
fn resistor() -> Vec<Shape> {
    vec![boxed((-1.016, 2.54), (1.016, -2.54))]
}

/// A capacitor: two parallel plates, each as long as the plate is wide and a little thick.
/// The plates' outer faces are where the pins end (a pin ends at y ±1.016, a plate is
/// [`HEAVY`] thick). Extent x ±2.032, y ±0.762.
fn capacitor() -> Vec<Shape> {
    let plate = 1.016 - HEAVY / 2.0;
    vec![
        line(&[(-2.032, plate), (2.032, plate)], HEAVY),
        line(&[(-2.032, -plate), (2.032, -plate)], HEAVY),
    ]
}

/// An inductor: four loops of a coil, each a half circle of 1.27 mm, bulging to the right.
/// Extent x 0 to 0.635, y ±2.54.
fn inductor() -> Vec<Shape> {
    (0..4)
        .map(|i| {
            let top = 2.54 - 1.27 * f64::from(i);
            Shape::Arc {
                start: (0.0, top),
                mid: (0.635, top - 0.635),
                end: (0.0, top - 1.27),
                width: THIN,
            }
        })
        .collect()
}

/// The two arrows of light of an LED, pointing away from the diode to the lower left. An arrow
/// is a shaft and a filled head whose right angle is the tip. Extent x from -4.572, y from
/// -2.286.
fn light_arrows() -> Vec<Shape> {
    let mut out = Vec::new();
    for tip in [(-4.572, -2.286), (-3.302, -2.286)] {
        out.push(line(
            &[
                (tip.0 + 1.524, tip.1 + 1.524),
                (tip.0 + 0.381, tip.1 + 0.381),
            ],
            THIN,
        ));
        out.push(polygon(
            &[tip, (tip.0 + 0.762, tip.1), (tip.0, tip.1 + 0.762)],
            THIN,
            Fill::Solid,
        ));
    }
    out
}

/// The triangle of a diode, pointing left to the cathode, and the bar it points at.
/// Extent x ±1.27, y ±1.27.
fn diode_body() -> Vec<Shape> {
    vec![
        polygon(
            &[(1.27, 1.27), (1.27, -1.27), (-1.27, 0.0)],
            BODY,
            Fill::None,
        ),
        line(&[(-1.27, 1.27), (-1.27, -1.27)], BODY),
    ]
}

/// A diode: a triangle and a bar. Extent x ±1.27, y ±1.27.
fn diode() -> Vec<Shape> {
    diode_body()
}

/// An LED: a diode and two arrows of light.
fn led() -> Vec<Shape> {
    let mut out = diode_body();
    out.extend(light_arrows());
    out
}

/// A Schottky diode: the bar's ends are bent in opposite directions, like an S. Extent
/// x -1.905 to 1.27, y ±1.27.
fn schottky() -> Vec<Shape> {
    vec![
        polygon(
            &[(1.27, 1.27), (1.27, -1.27), (-1.27, 0.0)],
            BODY,
            Fill::None,
        ),
        line(
            &[
                (-1.905, 0.762),
                (-1.905, 1.27),
                (-1.27, 1.27),
                (-1.27, -1.27),
                (-0.635, -1.27),
                (-0.635, -0.762),
            ],
            BODY,
        ),
    ]
}

/// A Zener diode: the bar has one end bent. Extent x ±1.27, y ±1.27.
fn zener() -> Vec<Shape> {
    vec![
        polygon(
            &[(1.27, 1.27), (1.27, -1.27), (-1.27, 0.0)],
            BODY,
            Fill::None,
        ),
        line(&[(-1.27, -1.27), (-1.27, 1.27), (-0.635, 1.27)], BODY),
    ]
}

/// A bidirectional TVS diode: two triangles tip to tip with a bent bar across their tips (the
/// ends of the bar bend the opposite ways, like a Z). The triangles' bases are where KiCad's
/// 5.08 mm body ends, past the pins' ends (x ±1.27), so the pins' lines run into them.
/// Extent x ±2.54, y ±1.27.
fn tvs() -> Vec<Shape> {
    vec![
        polygon(
            &[(-2.54, 1.27), (-2.54, -1.27), (0.0, 0.0)],
            BODY,
            Fill::None,
        ),
        polygon(&[(2.54, 1.27), (2.54, -1.27), (0.0, 0.0)], BODY, Fill::None),
        line(
            &[(0.635, 1.27), (0.0, 1.27), (0.0, -1.27), (-0.635, -1.27)],
            BODY,
        ),
    ]
}

/// A ferrite bead: a filled rectangle across the line, turned 30 degrees, as the line's
/// impedance is drawn in IEEE 315. Half as long as the pins' ends are apart (2.54), 1.143 wide
/// each side. Extent x ±2.771, y ±2.260.
fn ferrite_bead() -> Vec<Shape> {
    let (half_length, half_width) = (2.54, 1.143);
    let (sin, cos) = (0.5, 3f64.sqrt() / 2.0);
    // The long side runs from upper left to lower right.
    let corner =
        |along: f64, across: f64| (along * cos + across * sin, -along * sin + across * cos);
    vec![polygon(
        &[
            corner(-half_length, half_width),
            corner(half_length, half_width),
            corner(half_length, -half_width),
            corner(-half_length, -half_width),
        ],
        THIN,
        Fill::Solid,
    )]
}

/// A fuse: a rectangle with the line of the element through it. Extent x ±0.762, y ±2.54.
fn fuse() -> Vec<Shape> {
    vec![
        boxed((-0.762, 2.54), (0.762, -2.54)),
        line(&[(0.0, 2.54), (0.0, -2.54)], THIN),
    ]
}

/// A crystal: a rectangle between two plates, which stand out beyond it, and a lead to each.
/// Extent x ±2.54, y ±2.54.
fn crystal() -> Vec<Shape> {
    vec![
        line(&[(-2.54, 0.0), (-1.905, 0.0)], THIN),
        line(&[(-1.905, -2.54), (-1.905, 2.54)], HEAVY),
        boxed((-1.143, 1.778), (1.143, -1.778)),
        line(&[(1.905, -2.54), (1.905, 2.54)], HEAVY),
        line(&[(2.54, 0.0), (1.905, 0.0)], THIN),
    ]
}

/// A switch: two contacts and a blade, open. A contact is a circle that ends at its pin's end
/// (a pin ends at x ±2.54). Extent x ±2.54, y -0.508 to 1.778.
fn switch() -> Vec<Shape> {
    let contact = |x: f64| Shape::Circle {
        center: (x, 0.0),
        radius: 0.508,
        width: THIN,
        fill: Fill::None,
    };
    vec![
        contact(-2.032),
        line(&[(-1.524, 0.0), (1.524, 1.778)], THIN),
        contact(2.032),
    ]
}

/// An enhancement MOSFET, in a circle: the gate and its insulated plate, the channel in three
/// pieces, the drain above and the source below, the body tied to the source, and an arrow on
/// the body that points in for an N channel and out for a P channel. Extent x -2.54 to 4.445,
/// y ±2.794.
fn mosfet(n_channel: bool) -> Vec<Shape> {
    let channel = 0.762;
    let arrow = if n_channel {
        polygon(
            &[(1.016, 0.0), (2.032, 0.381), (2.032, -0.381)],
            THIN,
            Fill::Solid,
        )
    } else {
        polygon(
            &[(2.286, 0.0), (1.27, 0.381), (1.27, -0.381)],
            THIN,
            Fill::Solid,
        )
    };
    vec![
        Shape::Circle {
            center: (1.651, 0.0),
            radius: 2.794,
            width: BODY,
            fill: Fill::None,
        },
        line(&[(-2.54, 0.0), (0.254, 0.0)], THIN),
        line(&[(0.254, 1.905), (0.254, -1.905)], BODY),
        line(&[(channel, 2.286), (channel, 1.27)], BODY),
        line(&[(channel, 0.508), (channel, -0.508)], BODY),
        line(&[(channel, -1.27), (channel, -2.286)], BODY),
        line(&[(2.54, 2.54), (2.54, 1.778), (channel, 1.778)], THIN),
        line(&[(2.54, -2.54), (2.54, 0.0), (channel, 0.0)], THIN),
        line(&[(2.54, -1.778), (channel, -1.778)], THIN),
        dot((2.54, 1.778)),
        dot((2.54, -1.778)),
        arrow,
    ]
}

fn nmos() -> Vec<Shape> {
    mosfet(true)
}

fn pmos() -> Vec<Shape> {
    mosfet(false)
}

// ---------------------------------------------------------------------------------------
// The geometry: what the rest of netsmith measures.

/// One pin of a symbol.
struct PinSpec {
    kind: PinType,
    /// Where the pin meets the wire, and the way its line runs toward the body (degrees, 0 is
    /// right, 90 is up).
    at: (f64, f64, f64),
    length: f64,
    name: &'static str,
    number: &'static str,
}

const fn pin(
    kind: PinType,
    at: (f64, f64, f64),
    length: f64,
    name: &'static str,
    number: &'static str,
) -> PinSpec {
    PinSpec {
        kind,
        at,
        length,
        name,
        number,
    }
}

/// Two passive pins at the top and bottom of a vertical part (a line runs down from the top
/// and up from the bottom).
const fn vertical(length: f64, names: [&'static str; 2]) -> [PinSpec; 2] {
    [
        pin(PinType::Passive, (0.0, 3.81, 270.0), length, names[0], "1"),
        pin(PinType::Passive, (0.0, -3.81, 90.0), length, names[1], "2"),
    ]
}

/// Two passive pins at the left and right of a horizontal part.
const fn horizontal(at: f64, length: f64, names: [&'static str; 2]) -> [PinSpec; 2] {
    [
        pin(PinType::Passive, (-at, 0.0, 0.0), length, names[0], "1"),
        pin(PinType::Passive, (at, 0.0, 180.0), length, names[1], "2"),
    ]
}

/// How a symbol shows the names of its pins.
#[derive(Clone, Copy)]
struct Names {
    offset: f64,
    hidden: bool,
}

/// Everything about a symbol but its look.
struct Spec {
    /// `Library:Name`, as the mapping names it.
    id: &'static str,
    /// The letters of the `Reference` field.
    reference: &'static str,
    description: &'static str,
    /// `(x, y, rotation)` of the `Reference` and `Value` fields, from the origin.
    reference_at: (f64, f64, f64),
    value_at: (f64, f64, f64),
    /// Whether the pin numbers are hidden.
    numbers_hidden: bool,
    names: Names,
    pins: &'static [PinSpec],
    /// The look.
    glyph: fn() -> Vec<Shape>,
}

const SHOWN: bool = false;
const HIDDEN: bool = true;

/// Every symbol this module draws, in a fixed order. The numbers are those of KiCad 9's stock
/// symbol of the same id, measured with `kicad-cli`'s libraries, and the test
/// `passives_agree_with_the_installed_kicad_symbols` (`crates/netsmith/tests/passives.rs`)
/// proves each against the installed library.
const SPECS: &[Spec] = &[
    Spec {
        id: "Device:R",
        reference: "R",
        description: "Resistor",
        reference_at: (2.032, 0.0, 90.0),
        value_at: (0.0, 0.0, 90.0),
        numbers_hidden: HIDDEN,
        names: Names {
            offset: 0.0,
            hidden: SHOWN,
        },
        pins: &vertical(1.27, ["~", "~"]),
        glyph: resistor,
    },
    Spec {
        id: "Device:C",
        reference: "C",
        description: "Capacitor, not polarized",
        reference_at: (0.635, 2.54, 0.0),
        value_at: (0.635, -2.54, 0.0),
        numbers_hidden: HIDDEN,
        names: Names {
            offset: 0.254,
            hidden: SHOWN,
        },
        pins: &vertical(2.794, ["~", "~"]),
        glyph: capacitor,
    },
    Spec {
        id: "Device:L",
        reference: "L",
        description: "Inductor",
        reference_at: (-1.27, 0.0, 90.0),
        value_at: (1.905, 0.0, 90.0),
        numbers_hidden: HIDDEN,
        names: Names {
            offset: 1.016,
            hidden: HIDDEN,
        },
        pins: &vertical(1.27, ["1", "2"]),
        glyph: inductor,
    },
    Spec {
        id: "Device:LED",
        reference: "D",
        description: "Light emitting diode",
        reference_at: (0.0, 2.54, 0.0),
        value_at: (0.0, -2.54, 0.0),
        numbers_hidden: HIDDEN,
        names: DIODE_NAMES,
        pins: &horizontal(3.81, 2.54, ["K", "A"]),
        glyph: led,
    },
    Spec {
        id: "Device:D",
        reference: "D",
        description: "Diode",
        reference_at: (0.0, 2.54, 0.0),
        value_at: (0.0, -2.54, 0.0),
        numbers_hidden: HIDDEN,
        names: DIODE_NAMES,
        pins: &horizontal(3.81, 2.54, ["K", "A"]),
        glyph: diode,
    },
    Spec {
        id: "Device:D_Schottky",
        reference: "D",
        description: "Schottky diode",
        reference_at: (0.0, 2.54, 0.0),
        value_at: (0.0, -2.54, 0.0),
        numbers_hidden: HIDDEN,
        names: DIODE_NAMES,
        pins: &horizontal(3.81, 2.54, ["K", "A"]),
        glyph: schottky,
    },
    Spec {
        id: "Device:D_Zener",
        reference: "D",
        description: "Zener diode",
        reference_at: (0.0, 2.54, 0.0),
        value_at: (0.0, -2.54, 0.0),
        numbers_hidden: HIDDEN,
        names: DIODE_NAMES,
        pins: &horizontal(3.81, 2.54, ["K", "A"]),
        glyph: zener,
    },
    Spec {
        id: "Device:D_TVS",
        reference: "D",
        description: "Bidirectional transient voltage suppression diode",
        reference_at: (0.0, 2.54, 0.0),
        value_at: (0.0, -2.54, 0.0),
        numbers_hidden: HIDDEN,
        names: DIODE_NAMES,
        pins: &horizontal(3.81, 2.54, ["A1", "A2"]),
        glyph: tvs,
    },
    Spec {
        id: "Device:FerriteBead",
        reference: "FB",
        description: "Ferrite bead",
        reference_at: (-3.81, 0.635, 90.0),
        value_at: (3.81, 0.0, 90.0),
        numbers_hidden: HIDDEN,
        names: Names {
            offset: 0.0,
            hidden: SHOWN,
        },
        pins: &vertical(2.54, ["~", "~"]),
        glyph: ferrite_bead,
    },
    Spec {
        id: "Device:Fuse",
        reference: "F",
        description: "Fuse",
        reference_at: (2.032, 0.0, 90.0),
        value_at: (-1.905, 0.0, 90.0),
        numbers_hidden: HIDDEN,
        names: Names {
            offset: 0.0,
            hidden: SHOWN,
        },
        pins: &vertical(1.27, ["~", "~"]),
        glyph: fuse,
    },
    Spec {
        id: "Device:Crystal",
        reference: "Y",
        description: "Two pin crystal",
        reference_at: (0.0, 3.81, 0.0),
        value_at: (0.0, -3.81, 0.0),
        numbers_hidden: HIDDEN,
        names: DIODE_NAMES,
        pins: &horizontal(3.81, 1.27, ["1", "2"]),
        glyph: crystal,
    },
    Spec {
        id: "Switch:SW_SPST",
        reference: "SW",
        description: "Single pole single throw switch",
        reference_at: (0.0, 3.175, 0.0),
        value_at: (0.0, -2.54, 0.0),
        numbers_hidden: SHOWN,
        names: Names {
            offset: 0.0,
            hidden: HIDDEN,
        },
        pins: &horizontal(5.08, 2.54, ["A", "B"]),
        glyph: switch,
    },
    Spec {
        id: "Transistor_FET:Q_NMOS_GSD",
        reference: "Q",
        description: "N-channel MOSFET, gate, source, drain",
        reference_at: (5.08, 1.905, 0.0),
        value_at: (5.08, 0.0, 0.0),
        numbers_hidden: SHOWN,
        names: Names {
            offset: 0.0,
            hidden: HIDDEN,
        },
        pins: &FET_PINS,
        glyph: nmos,
    },
    Spec {
        id: "Transistor_FET:Q_PMOS_GSD",
        reference: "Q",
        description: "P-channel MOSFET, gate, source, drain",
        reference_at: (5.08, 1.905, 0.0),
        value_at: (5.08, 0.0, 0.0),
        numbers_hidden: SHOWN,
        names: Names {
            offset: 0.0,
            hidden: HIDDEN,
        },
        pins: &FET_PINS,
        glyph: pmos,
    },
];

/// The names of a diode's, an LED's and a crystal's pins are not shown (they are `K` and `A`).
const DIODE_NAMES: Names = Names {
    offset: 1.016,
    hidden: HIDDEN,
};

/// The pins of a FET drawn with the gate on the left, the drain above and the source below:
/// pad 1 is the gate (an input), pad 3 the drain and pad 2 the source, which are passive.
const FET_PINS: [PinSpec; 3] = [
    pin(PinType::Input, (-5.08, 0.0, 0.0), 2.54, "G", "1"),
    pin(PinType::Passive, (2.54, 5.08, 270.0), 2.54, "D", "3"),
    pin(PinType::Passive, (2.54, -5.08, 90.0), 2.54, "S", "2"),
];

/// The ids of the symbols made here, in a fixed order.
pub fn ids() -> impl Iterator<Item = &'static str> {
    SPECS.iter().map(|s| s.id)
}

/// Whether some id of `library` is made here.
pub fn covers_library(library: &str) -> bool {
    SPECS
        .iter()
        .any(|s| s.id.split_once(':').is_some_and(|(l, _)| l == library))
}

fn effects(hide: bool) -> Sexp {
    let mut items = vec![Sexp::node(
        "font",
        vec![Sexp::node("size", vec![Sexp::num(1.27), Sexp::num(1.27)])],
    )];
    if hide {
        items.push(Sexp::node("hide", vec![Sexp::atom("yes")]));
    }
    Sexp::node("effects", items)
}

fn property(name: &str, value: &str, at: (f64, f64, f64), hide: bool) -> Sexp {
    Sexp::node(
        "property",
        vec![
            Sexp::string(name),
            Sexp::string(value),
            Sexp::node(
                "at",
                vec![Sexp::num(at.0), Sexp::num(at.1), Sexp::num(at.2)],
            ),
            effects(hide),
        ],
    )
}

fn pin_node(spec: &PinSpec) -> Sexp {
    Sexp::node(
        "pin",
        vec![
            Sexp::atom(spec.kind.name()),
            Sexp::atom("line"),
            Sexp::node(
                "at",
                vec![
                    Sexp::num(spec.at.0),
                    Sexp::num(spec.at.1),
                    Sexp::num(spec.at.2),
                ],
            ),
            Sexp::node("length", vec![Sexp::num(spec.length)]),
            Sexp::node("name", vec![Sexp::string(spec.name), effects(false)]),
            Sexp::node("number", vec![Sexp::string(spec.number), effects(false)]),
        ],
    )
}

/// `(symbol "<name>" ...)` as a library file holds it, with the glyph in unit 0 (drawn with
/// every unit) and the pins in unit 1.
fn definition(spec: &Spec) -> Sexp {
    let name = spec.id.split_once(':').map_or(spec.id, |(_, n)| n);
    let mut items = vec![Sexp::string(name)];
    let mut flag = |list: &str, hidden: bool, offset: Option<f64>| {
        let mut inner = Vec::new();
        if let Some(offset) = offset {
            inner.push(Sexp::node("offset", vec![Sexp::num(offset)]));
        }
        if hidden {
            inner.push(Sexp::node("hide", vec![Sexp::atom("yes")]));
        }
        if !inner.is_empty() {
            items.push(Sexp::node(list, inner));
        }
    };
    flag("pin_numbers", spec.numbers_hidden, None);
    flag("pin_names", spec.names.hidden, Some(spec.names.offset));
    items.extend([
        Sexp::node("exclude_from_sim", vec![Sexp::atom("no")]),
        Sexp::node("in_bom", vec![Sexp::atom("yes")]),
        Sexp::node("on_board", vec![Sexp::atom("yes")]),
        property("Reference", spec.reference, spec.reference_at, false),
        property("Value", name, spec.value_at, false),
        property("Footprint", "", (0.0, 0.0, 0.0), true),
        property("Datasheet", "~", (0.0, 0.0, 0.0), true),
        property("Description", spec.description, (0.0, 0.0, 0.0), true),
    ]);
    let mut drawing = vec![Sexp::string(format!("{name}_0_1"))];
    drawing.extend((spec.glyph)().iter().map(Shape::node));
    items.push(Sexp::node("symbol", drawing));
    let mut pins = vec![Sexp::string(format!("{name}_1_1"))];
    pins.extend(spec.pins.iter().map(pin_node));
    items.push(Sexp::node("symbol", pins));
    Sexp::node("symbol", items)
}

/// The id of the symbol that answers the KiCad id `lib_id` (`Device:R` is `netsmith:R`), if
/// this module draws it. The symbol is a netsmith one and not KiCad's `Device:R`: a schematic
/// that says `Device:R` while it embeds another drawing gets a `lib_symbol_mismatch` warning
/// from KiCad on any machine that has the stock library, and a library of the board's own
/// (`netsmith.kicad_sym`, beside the schematic and in its library table) has none.
pub fn answer_id(lib_id: &str) -> Option<String> {
    let spec = SPECS.iter().find(|s| s.id == lib_id)?;
    let name = spec.id.split_once(':').map_or(spec.id, |(_, n)| n);
    Some(format!("{}:{name}", crate::stdlib::NAME))
}

/// The symbol that answers the KiCad id `lib_id` (`Device:R`): its own id is
/// [`answer_id`].
pub fn symbol(lib_id: &str) -> Option<Symbol> {
    let spec = SPECS.iter().find(|s| s.id == lib_id)?;
    let own = answer_id(lib_id)?;
    let name = lib_id.split_once(':').map_or(lib_id, |(_, n)| n);
    Some(
        Symbol::from_definition(&own, name, definition(spec))
            .expect("a symbol drawn here is one that can be read"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_id_is_drawn_twice_the_same_and_reads_back() {
        let mut seen = std::collections::HashSet::new();
        for id in ids() {
            assert!(seen.insert(id), "{id} is listed twice");
            let first = symbol(id).unwrap_or_else(|| panic!("{id} is listed but not drawn"));
            let name = id.split_once(':').unwrap().1;
            assert_eq!(first.lib_id(), format!("netsmith:{name}"));
            assert_eq!(answer_id(id).as_deref(), Some(first.lib_id()));
            assert_eq!(first.units(), 1, "{id}");
            assert_eq!(
                first.definition().to_kicad_string(),
                symbol(id).unwrap().definition().to_kicad_string(),
                "{id} is not deterministic"
            );
        }
        assert_eq!(seen.len(), 14);
        assert!(symbol("Device:Nothing").is_none());
        assert!(covers_library("Device") && covers_library("Switch"));
        assert!(!covers_library("Connector"));
    }

    #[test]
    fn a_pin_is_where_its_line_leaves_the_body_on_the_grid() {
        // Every pin point and length is on a 0.127 mm step (5 mil), the grid of the stock
        // symbols, and every pin of one symbol has its own number.
        for id in ids() {
            let symbol = symbol(id).unwrap();
            let mut numbers = std::collections::HashSet::new();
            for pin in symbol.pins() {
                assert!(
                    numbers.insert(pin.number.clone()),
                    "{id}: pin {}",
                    pin.number
                );
                for v in [pin.x, pin.y, pin.length] {
                    let steps = v / 0.127;
                    assert!((steps - steps.round()).abs() < 1e-6, "{id}: {v}");
                }
            }
        }
    }

    #[test]
    fn a_pin_line_ends_inside_the_part_not_beyond_its_centre() {
        // The line runs from the connection point toward the body: a part's two pins face
        // each other and their lines do not cross.
        for id in ids().filter(|i| !i.starts_with("Transistor_FET")) {
            let symbol = symbol(id).unwrap();
            let end = |n: &str| {
                let p = symbol.pin(n).unwrap();
                let (s, c) = p.angle.to_radians().sin_cos();
                (p.x + c * p.length, p.y + s * p.length)
            };
            let (a, b) = (end("1"), end("2"));
            assert!(
                (a.0.abs() < 1e-9 && b.0.abs() < 1e-9 && a.1 > b.1)
                    || (a.1.abs() < 1e-9 && b.1.abs() < 1e-9 && a.0 < b.0),
                "{id}: pin lines end at {a:?} and {b:?}"
            );
        }
    }
}
