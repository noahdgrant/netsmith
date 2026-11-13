//! The symbols of the standard mapping that netsmith draws itself (ADR 0097, step 2): the
//! passives and discrete parts (a resistor, a capacitor, an inductor, a ferrite bead, a fuse, a
//! crystal, an LED and four diodes, a switch and the N and P FETs), the connectors (the generic
//! pin headers and sockets, the USB-C receptacles and the coaxial connector), the mechanical
//! parts (a test point, a mounting hole, a plated hole and a fiducial) and the power flag.
//! A mapping row that names one of these ids (`Device:R`, `Connector_Generic:Conn_01x02`,
//! `Transistor_FET:Q_NMOS_GSD`) is answered here, from the binary, on every machine and whatever
//! KiCad has installed, as the footprints of `netsmith_pcb::generated_footprint` are (ADR 0043). No
//! mapping file changes. The symbol that answers has the id `netsmith:<Name>` ([`answer_id`])
//! and goes into the board's own `netsmith.kicad_sym` like the symbol of an IC drawn from its
//! pin table. The module keeps the name of the symbols it drew first.
//!
//! These parts are drawn as fixed glyphs, not as the box of an IC (`generate.rs`), because the
//! layout of the schematic reads their pins where KiCad's symbols put them: a two-row header
//! has its odd pins left and its even pins right, a USB-C receptacle has its power on the right
//! and its ground below, a test point has one pin at its foot. The hand-built designs of the
//! drawing tests also have no pin table, and a part with no pin table cannot be drawn as a box.
//!
//! # What is fixed and what is the look
//!
//! A symbol has two parts, and they are kept apart on purpose.
//!
//! - **The geometry** every other part of netsmith measures: the number, name and electrical
//!   type of each pin, where it is and which way it runs, its length, the position of the
//!   `Reference` and `Value` fields, whether the pin numbers and names are shown, and the
//!   extent of the body (the layout engine routes round it, and puts the texts beside it).
//!   They are the written constants of the specs (`passive_specs`, `connector_specs`,
//!   `mechanical_specs`) and of the glyph functions' extents, equal to those of the symbol with
//!   the same id in KiCad 9's stock libraries (the test
//!   `generated_symbols_agree_with_the_installed_kicad_symbols` of
//!   `crates/netsmith-pipeline/tests/passives.rs` proves it where KiCad is installed). A pin that moved
//!   by a grid step would move every wire the schematic view draws to it.
//! - **The look**: what is drawn inside that extent. It is the glyph function of one symbol
//!   ([`resistor`], [`capacitor`], [`usb_c`], ...), which returns its [`Shape`]s, and the three
//!   line weights ([`THIN`], [`BODY`], [`HEAVY`]) and fills at the top of the file. To change
//!   how a resistor looks, change [`resistor`] and nothing else. The shapes are drawn from the
//!   electrical and mechanical conventions (IEC 60617, IEEE 315, the USB trident) and the grid,
//!   not from KiCad's symbol files, which are licensed CC-BY-SA 4.0 (ADR 0097). A body is a
//!   rectangle with the background fill and a line of [`BODY`] weight, as the box of an IC is
//!   (`generate.rs`).
//!
//! The same id gives the same bytes: nothing here reads a clock, a map's order or the machine.

use std::sync::OnceLock;

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
    // The body diode, in parallel with the channel from source to drain: the same side the
    // channel arrow points, so that it conducts from the source toward the drain in an N-channel
    // part and the other way in a P-channel one. A bar and a triangle on a rail 0.762 mm outside
    // the leads' column.
    let rail = 3.302;
    let (bar, apex, base) = if n_channel {
        (0.381, 0.381, -0.254)
    } else {
        (-0.381, -0.381, 0.254)
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
        line(&[(2.54, 2.54), (2.54, 1.778)], THIN),
        line(
            &[
                (channel, -1.778),
                (rail, -1.778),
                (rail, 1.778),
                (channel, 1.778),
            ],
            THIN,
        ),
        line(&[(2.54, -2.54), (2.54, 0.0), (channel, 0.0)], THIN),
        dot((2.54, 1.778)),
        dot((2.54, -1.778)),
        arrow,
        line(&[(rail - 0.381, bar), (rail + 0.381, bar)], THIN),
        polygon(
            &[(rail, apex), (rail - 0.381, base), (rail + 0.381, base)],
            THIN,
            Fill::None,
        ),
    ]
}

fn nmos() -> Vec<Shape> {
    mosfet(true)
}

fn pmos() -> Vec<Shape> {
    mosfet(false)
}

/// The pitch of a header's pins, 0.1 inch, mm.
const PITCH: f64 = 2.54;

/// The `y` of the first pin of a header with `rows` pins in a column: the middle of the column
/// falls on the origin, a grid step above it for a column of an even length.
fn header_top(rows: usize) -> f64 {
    ((rows - 1) / 2) as f64 * PITCH
}

/// A pin header of `columns` columns (1 or 2) and `rows` pins in a column: a box, with a bar
/// at each pin for its contact. A second column is on the right, 5.08 mm from the first, and
/// its bars lie against its own edge. Extent x -1.27 to 1.27 (3.81 with two columns), y one grid
/// step beyond the first and last pin.
fn header(rows: usize, columns: usize) -> Vec<Shape> {
    let top = header_top(rows);
    let right = if columns == 2 { 3.81 } else { 1.27 };
    let bottom = top - (rows - 1) as f64 * PITCH;
    let mut out = vec![boxed((-1.27, top + 1.27), (right, bottom - 1.27))];
    for row in 0..rows {
        let y = top - row as f64 * PITCH;
        out.push(line(&[(-1.27, y), (-0.254, y)], BODY));
        if columns == 2 {
            out.push(line(&[(right, y), (right - 1.016, y)], BODY));
        }
    }
    out
}

/// A pin socket of `rows` contacts in one column: at each pin a lead and an open arc, the
/// receptacle that a pin enters (IEC 60617, a socket is the "(" that a plug's pin goes into).
/// Extent x -1.27 to 0, y half a contact beyond the first and last pin.
fn socket(rows: usize) -> Vec<Shape> {
    let top = header_top(rows);
    let mut out = Vec::new();
    for row in 0..rows {
        let y = top - row as f64 * PITCH;
        out.push(line(&[(-1.27, y), (-0.508, y)], THIN));
        out.push(Shape::Arc {
            start: (0.0, y - 0.508),
            mid: (-0.508, y),
            end: (0.0, y + 0.508),
            width: THIN,
        });
    }
    out
}

/// A USB Type-C receptacle in a box `half_height` high (a receptacle with more contacts has
/// more pins down its side): the oval mouth of the receptacle with its tongue, on the left,
/// and the USB trident on the right: a stem with an arrowhead, a base disc, and a fork that
/// ends in a disc and one that ends in a square. Extent x ±10.16, y ±`half_height`.
fn usb_c(half_height: f64) -> Vec<Shape> {
    let mut out = vec![boxed((-10.16, half_height), (10.16, -half_height))];
    // The mouth: two sides and two half circles of 1.27 mm radius.
    out.push(line(&[(-8.255, 2.54), (-8.255, -2.54)], BODY));
    out.push(line(&[(-5.715, 2.54), (-5.715, -2.54)], BODY));
    out.push(Shape::Arc {
        start: (-8.255, 2.54),
        mid: (-6.985, 3.81),
        end: (-5.715, 2.54),
        width: BODY,
    });
    out.push(Shape::Arc {
        start: (-5.715, -2.54),
        mid: (-6.985, -3.81),
        end: (-8.255, -2.54),
        width: BODY,
    });
    out.push(Shape::Rect {
        a: (-7.239, 1.905),
        b: (-6.731, -1.905),
        width: THIN,
        fill: Fill::Solid,
    });
    // The trident: stem, base disc, arrowhead, and the two forks.
    out.push(line(&[(1.27, -3.81), (1.27, 5.08)], BODY));
    out.push(Shape::Circle {
        center: (1.27, -3.81),
        radius: 1.016,
        width: THIN,
        fill: Fill::Solid,
    });
    out.push(polygon(
        &[(0.508, 5.08), (1.27, 6.604), (2.032, 5.08)],
        THIN,
        Fill::Solid,
    ));
    out.push(line(
        &[(1.27, -1.27), (-0.254, 0.254), (-0.254, 1.27)],
        BODY,
    ));
    out.push(Shape::Circle {
        center: (-0.254, 1.778),
        radius: 0.508,
        width: THIN,
        fill: Fill::Solid,
    });
    out.push(line(&[(1.27, 0.0), (2.794, 1.524), (2.794, 2.286)], BODY));
    out.push(Shape::Rect {
        a: (2.413, 2.286),
        b: (3.175, 3.048),
        width: THIN,
        fill: Fill::Solid,
    });
    out
}

/// A coaxial connector: the centre conductor as a small ring, which the signal's lead (from the
/// left) reaches, inside a shield drawn as two arcs that open to the left where the lead
/// enters, and the shield's own lead down. Extent x -2.54 to 1.778, y -2.54 to 1.778.
fn coaxial() -> Vec<Shape> {
    let radius: f64 = 1.778;
    // The gap of the shield on the left is as wide as the centre conductor's ring.
    let gap: f64 = 0.508;
    let edge = (radius * radius - gap * gap).sqrt();
    vec![
        line(&[(-2.54, 0.0), (-0.508, 0.0)], THIN),
        Shape::Circle {
            center: (0.0, 0.0),
            radius: 0.508,
            width: BODY,
            fill: Fill::None,
        },
        Shape::Arc {
            start: (-edge, gap),
            mid: (0.0, radius),
            end: (radius, 0.0),
            width: BODY,
        },
        Shape::Arc {
            start: (radius, 0.0),
            mid: (0.0, -radius),
            end: (-edge, -gap),
            width: BODY,
        },
        line(&[(0.0, -2.54), (0.0, -radius)], THIN),
    ]
}

/// A test point: a small open circle on the end of its pin. Extent x ±0.762, y 2.54 to 4.064.
fn test_point() -> Vec<Shape> {
    vec![Shape::Circle {
        center: (0.0, 3.302),
        radius: 0.762,
        width: BODY,
        fill: Fill::None,
    }]
}

/// A mounting hole: a ring, the drill. Extent x and y ±1.27.
fn mounting_hole() -> Vec<Shape> {
    vec![Shape::Circle {
        center: (0.0, 0.0),
        radius: 1.27,
        width: BODY,
        fill: Fill::None,
    }]
}

/// A plated mounting hole: a ring with the plating's ring inside it, above its pin. Extent x
/// ±1.27, y 0 to 2.54.
fn plated_hole() -> Vec<Shape> {
    vec![
        Shape::Circle {
            center: (0.0, 1.27),
            radius: 1.27,
            width: BODY,
            fill: Fill::None,
        },
        Shape::Circle {
            center: (0.0, 1.27),
            radius: 0.635,
            width: THIN,
            fill: Fill::None,
        },
    ]
}

/// A fiducial: a copper dot in a ring, the clearing round it. Extent x and y ±1.27.
fn fiducial() -> Vec<Shape> {
    vec![
        Shape::Circle {
            center: (0.0, 0.0),
            radius: 1.27,
            width: BODY,
            fill: Fill::Background,
        },
        Shape::Circle {
            center: (0.0, 0.0),
            radius: 0.508,
            width: THIN,
            fill: Fill::Solid,
        },
    ]
}

/// A power flag: a stem from the pin and a diamond on it, the flag that says "power comes from
/// here". Extent x ±1.016, y 0 to 2.54.
fn power_flag() -> Vec<Shape> {
    vec![
        line(&[(0.0, 0.0), (0.0, 1.27)], THIN),
        polygon(
            &[(0.0, 1.27), (-1.016, 1.905), (0.0, 2.54), (1.016, 1.905)],
            THIN,
            Fill::None,
        ),
    ]
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
    name: String,
    number: String,
    /// A further pad on the point of another pin, which KiCad's symbols hide.
    stacked: bool,
}

fn pin(
    kind: PinType,
    at: (f64, f64, f64),
    length: f64,
    name: &str,
    number: &str,
    stacked: bool,
) -> PinSpec {
    PinSpec {
        kind,
        at,
        length,
        name: name.to_string(),
        number: number.to_string(),
        stacked,
    }
}

/// Two passive pins at the top and bottom of a vertical part (a line runs down from the top
/// and up from the bottom).
fn vertical(length: f64, names: [&str; 2]) -> Vec<PinSpec> {
    let passive = PinType::Passive;
    vec![
        pin(passive, (0.0, 3.81, 270.0), length, names[0], "1", false),
        pin(passive, (0.0, -3.81, 90.0), length, names[1], "2", false),
    ]
}

/// Two passive pins at the left and right of a horizontal part.
fn horizontal(at: f64, length: f64, names: [&str; 2]) -> Vec<PinSpec> {
    let passive = PinType::Passive;
    vec![
        pin(passive, (-at, 0.0, 0.0), length, names[0], "1", false),
        pin(passive, (at, 0.0, 180.0), length, names[1], "2", false),
    ]
}

/// How a symbol shows the names of its pins. No offset is a symbol that says nothing of it.
#[derive(Clone, Copy)]
struct Names {
    offset: Option<f64>,
    hidden: bool,
}

/// Everything about a symbol but its look.
struct Spec {
    /// `Library:Name`, as the mapping names it.
    id: String,
    /// The letters of the `Reference` field.
    reference: &'static str,
    /// Whether the `Reference` field is not shown (a flag's is not).
    reference_hidden: bool,
    description: String,
    /// `(x, y, rotation)` of the `Reference` and `Value` fields, from the origin.
    reference_at: (f64, f64, f64),
    value_at: (f64, f64, f64),
    /// Whether the pin numbers are hidden.
    numbers_hidden: bool,
    names: Names,
    pins: Vec<PinSpec>,
    /// The part of the definition the pins are in: unit and style, `1_1`, or `0_0` for a pin
    /// that every unit shares.
    pin_part: &'static str,
    /// Whether KiCad calls it a power symbol, which a flag is.
    power: bool,
    /// Whether a bill of materials lists a part drawn with it: not a hole or a fiducial, which
    /// nobody buys or fits (KiCad's own symbols say the same).
    in_bom: bool,
    /// Whether the `Reference` and `Value` fields are left-justified (the text starts at its
    /// position, as KiCad's FET symbols have it, so that it stands clear of the body).
    fields_left: bool,
    /// The look.
    glyph: Vec<Shape>,
}

const SHOWN: bool = false;
const HIDDEN: bool = true;

/// What a spec is when it says nothing of a field: pin numbers shown, names offset 1.016 mm.
const BASE: Spec = Spec {
    id: String::new(),
    reference: "",
    reference_hidden: false,
    description: String::new(),
    reference_at: (0.0, 0.0, 0.0),
    value_at: (0.0, 0.0, 0.0),
    numbers_hidden: SHOWN,
    names: Names {
        offset: Some(1.016),
        hidden: SHOWN,
    },
    pins: Vec::new(),
    pin_part: "1_1",
    power: false,
    in_bom: true,
    fields_left: false,
    glyph: Vec::new(),
};

/// The passives and discrete parts. The numbers are those of KiCad 9's stock symbol of the same
/// id, measured with `kicad-cli`'s libraries, and the test
/// `generated_symbols_agree_with_the_installed_kicad_symbols`
/// (`crates/netsmith-pipeline/tests/passives.rs`) proves each against the installed library.
fn passive_specs() -> Vec<Spec> {
    vec![
        Spec {
            id: "Device:R".into(),
            reference: "R",
            description: "Resistor".into(),
            reference_at: (2.032, 0.0, 90.0),
            value_at: (0.0, 0.0, 90.0),
            numbers_hidden: HIDDEN,
            names: Names {
                offset: Some(0.0),
                hidden: SHOWN,
            },
            pins: vertical(1.27, ["~", "~"]),
            glyph: resistor(),
            ..BASE
        },
        Spec {
            id: "Device:C".into(),
            reference: "C",
            description: "Capacitor, not polarized".into(),
            reference_at: (0.635, 2.54, 0.0),
            value_at: (0.635, -2.54, 0.0),
            numbers_hidden: HIDDEN,
            names: Names {
                offset: Some(0.254),
                hidden: SHOWN,
            },
            pins: vertical(2.794, ["~", "~"]),
            glyph: capacitor(),
            ..BASE
        },
        Spec {
            id: "Device:L".into(),
            reference: "L",
            description: "Inductor".into(),
            reference_at: (-1.27, 0.0, 90.0),
            value_at: (1.905, 0.0, 90.0),
            numbers_hidden: HIDDEN,
            names: Names {
                offset: Some(1.016),
                hidden: HIDDEN,
            },
            pins: vertical(1.27, ["1", "2"]),
            glyph: inductor(),
            ..BASE
        },
        Spec {
            id: "Device:LED".into(),
            reference: "D",
            description: "Light emitting diode".into(),
            reference_at: (0.0, 2.54, 0.0),
            value_at: (0.0, -2.54, 0.0),
            numbers_hidden: HIDDEN,
            names: DIODE_NAMES,
            pins: horizontal(3.81, 2.54, ["K", "A"]),
            glyph: led(),
            ..BASE
        },
        Spec {
            id: "Device:D".into(),
            reference: "D",
            description: "Diode".into(),
            reference_at: (0.0, 2.54, 0.0),
            value_at: (0.0, -2.54, 0.0),
            numbers_hidden: HIDDEN,
            names: DIODE_NAMES,
            pins: horizontal(3.81, 2.54, ["K", "A"]),
            glyph: diode(),
            ..BASE
        },
        Spec {
            id: "Device:D_Schottky".into(),
            reference: "D",
            description: "Schottky diode".into(),
            reference_at: (0.0, 2.54, 0.0),
            value_at: (0.0, -2.54, 0.0),
            numbers_hidden: HIDDEN,
            names: DIODE_NAMES,
            pins: horizontal(3.81, 2.54, ["K", "A"]),
            glyph: schottky(),
            ..BASE
        },
        Spec {
            id: "Device:D_Zener".into(),
            reference: "D",
            description: "Zener diode".into(),
            reference_at: (0.0, 2.54, 0.0),
            value_at: (0.0, -2.54, 0.0),
            numbers_hidden: HIDDEN,
            names: DIODE_NAMES,
            pins: horizontal(3.81, 2.54, ["K", "A"]),
            glyph: zener(),
            ..BASE
        },
        Spec {
            id: "Device:D_TVS".into(),
            reference: "D",
            description: "Bidirectional transient voltage suppression diode".into(),
            reference_at: (0.0, 2.54, 0.0),
            value_at: (0.0, -2.54, 0.0),
            numbers_hidden: HIDDEN,
            names: DIODE_NAMES,
            pins: horizontal(3.81, 2.54, ["A1", "A2"]),
            glyph: tvs(),
            ..BASE
        },
        Spec {
            id: "Device:FerriteBead".into(),
            reference: "FB",
            description: "Ferrite bead".into(),
            reference_at: (-3.81, 0.635, 90.0),
            value_at: (3.81, 0.0, 90.0),
            numbers_hidden: HIDDEN,
            names: Names {
                offset: Some(0.0),
                hidden: SHOWN,
            },
            pins: vertical(2.54, ["~", "~"]),
            glyph: ferrite_bead(),
            ..BASE
        },
        Spec {
            id: "Device:Fuse".into(),
            reference: "F",
            description: "Fuse".into(),
            reference_at: (2.032, 0.0, 90.0),
            value_at: (-1.905, 0.0, 90.0),
            numbers_hidden: HIDDEN,
            names: Names {
                offset: Some(0.0),
                hidden: SHOWN,
            },
            pins: vertical(1.27, ["~", "~"]),
            glyph: fuse(),
            ..BASE
        },
        Spec {
            id: "Device:Crystal".into(),
            reference: "Y",
            description: "Two pin crystal".into(),
            reference_at: (0.0, 3.81, 0.0),
            value_at: (0.0, -3.81, 0.0),
            numbers_hidden: HIDDEN,
            names: DIODE_NAMES,
            pins: horizontal(3.81, 1.27, ["1", "2"]),
            glyph: crystal(),
            ..BASE
        },
        Spec {
            id: "Switch:SW_SPST".into(),
            reference: "SW",
            description: "Single pole single throw switch".into(),
            reference_at: (0.0, 3.175, 0.0),
            value_at: (0.0, -2.54, 0.0),
            numbers_hidden: SHOWN,
            names: Names {
                offset: Some(0.0),
                hidden: HIDDEN,
            },
            pins: horizontal(5.08, 2.54, ["A", "B"]),
            glyph: switch(),
            ..BASE
        },
        Spec {
            id: "Transistor_FET:Q_NMOS_GSD".into(),
            reference: "Q",
            description: "N-channel MOSFET, gate, source, drain".into(),
            reference_at: (5.08, 1.905, 0.0),
            value_at: (5.08, 0.0, 0.0),
            numbers_hidden: SHOWN,
            names: Names {
                offset: Some(0.0),
                hidden: HIDDEN,
            },
            pins: fet_pins(),
            fields_left: true,
            glyph: nmos(),
            ..BASE
        },
        Spec {
            id: "Transistor_FET:Q_PMOS_GSD".into(),
            reference: "Q",
            description: "P-channel MOSFET, gate, source, drain".into(),
            reference_at: (5.08, 1.905, 0.0),
            value_at: (5.08, 0.0, 0.0),
            numbers_hidden: SHOWN,
            names: Names {
                offset: Some(0.0),
                hidden: HIDDEN,
            },
            pins: fet_pins(),
            fields_left: true,
            glyph: pmos(),
            ..BASE
        },
    ]
}

/// The names of a diode's, an LED's and a crystal's pins are not shown (they are `K` and `A`).
const DIODE_NAMES: Names = Names {
    offset: Some(1.016),
    hidden: HIDDEN,
};

/// The pins of a FET drawn with the gate on the left, the drain above and the source below:
/// pad 1 is the gate (an input), pad 3 the drain and pad 2 the source, which are passive.
fn fet_pins() -> Vec<PinSpec> {
    vec![
        pin(PinType::Input, (-5.08, 0.0, 0.0), 2.54, "G", "1", false),
        pin(PinType::Passive, (2.54, 5.08, 270.0), 2.54, "D", "3", false),
        pin(PinType::Passive, (2.54, -5.08, 90.0), 2.54, "S", "2", false),
    ]
}

/// The pins of a header of `columns` columns of `rows` pins, 2.54 mm apart, the first at the
/// top of the left column. A pin is `Pin_<number>`, as KiCad's generic connectors name it. The
/// left column has the odd numbers (running right, into the body) and the right column the
/// even ones (running left).
fn header_pins(rows: usize, columns: usize) -> Vec<PinSpec> {
    let top = header_top(rows);
    let mut pins = Vec::new();
    for column in 0..columns {
        for row in 0..rows {
            let number = (row * columns + column + 1).to_string();
            let y = top - row as f64 * PITCH;
            let at = if column == 0 {
                (-5.08, y, 0.0)
            } else {
                (7.62, y, 180.0)
            };
            pins.push(pin(
                PinType::Passive,
                at,
                3.81,
                &format!("Pin_{number}"),
                &number,
                false,
            ));
        }
    }
    pins
}

/// A header or a socket of `columns` columns of `rows` pins in `library`, with KiCad's name
/// (`Conn_01x04`, `Conn_01x04_Socket`, `Conn_02x05_Odd_Even`).
fn header_spec(library: &str, rows: usize, columns: usize, socket_form: bool) -> Spec {
    let top = header_top(rows);
    let bottom = top - (rows - 1) as f64 * PITCH - 1.27;
    let suffix = match (columns, socket_form) {
        (_, true) => "_Socket",
        (2, false) => "_Odd_Even",
        _ => "",
    };
    let middle = if columns == 2 { 1.27 } else { 0.0 };
    Spec {
        id: format!("{library}:Conn_{columns:02}x{rows:02}{suffix}"),
        reference: "J",
        description: format!(
            "{}, {} of {rows}",
            if socket_form {
                "Pin socket"
            } else {
                "Pin header"
            },
            if columns == 1 {
                "one row".to_string()
            } else {
                format!("{columns} rows")
            },
        ),
        reference_at: (middle, top + PITCH, 0.0),
        value_at: (middle, bottom - 1.27, 0.0),
        names: Names {
            offset: Some(1.016),
            hidden: HIDDEN,
        },
        pins: header_pins(rows, columns),
        glyph: if socket_form {
            socket(rows)
        } else {
            header(rows, columns)
        },
        ..BASE
    }
}

/// The connectors: the generic headers and sockets the standard mapping names, the two USB-C
/// receptacles and the coaxial connector.
fn connector_specs() -> Vec<Spec> {
    let mut specs = Vec::new();
    for rows in [2, 4, 8, 10] {
        specs.push(header_spec("Connector_Generic", rows, 1, false));
    }
    for rows in [3, 5, 10] {
        specs.push(header_spec("Connector_Generic", rows, 2, false));
    }
    for rows in [2, 4, 8, 10] {
        specs.push(header_spec("Connector", rows, 1, true));
    }
    let bi = PinType::Bidirectional;
    let passive = PinType::Passive;
    specs.push(Spec {
        id: "Connector:USB_C_Receptacle_PowerOnly_6P".into(),
        reference: "J",
        description: "USB Type-C receptacle, power only, 6 contacts".into(),
        reference_at: (0.0, 16.51, 0.0),
        value_at: (0.0, 13.97, 0.0),
        pins: vec![
            pin(passive, (-7.62, -17.78, 90.0), 5.08, "SHIELD", "S1", false),
            pin(passive, (0.0, -17.78, 90.0), 5.08, "GND", "A12", false),
            pin(passive, (0.0, -17.78, 90.0), 5.08, "GND", "B12", true),
            pin(passive, (15.24, 7.62, 180.0), 5.08, "VBUS", "A9", false),
            pin(passive, (15.24, 7.62, 180.0), 5.08, "VBUS", "B9", true),
            pin(bi, (15.24, -5.08, 180.0), 5.08, "CC1", "A5", false),
            pin(bi, (15.24, -7.62, 180.0), 5.08, "CC2", "B5", false),
        ],
        glyph: usb_c(12.7),
        ..BASE
    });
    specs.push(Spec {
        id: "Connector:USB_C_Receptacle_USB2.0_16P".into(),
        reference: "J",
        description: "USB Type-C receptacle, USB 2.0, 16 contacts".into(),
        reference_at: (0.0, 22.225, 0.0),
        value_at: (0.0, 19.685, 0.0),
        pins: vec![
            pin(passive, (-7.62, -22.86, 90.0), 5.08, "SHIELD", "S1", false),
            pin(passive, (0.0, -22.86, 90.0), 5.08, "GND", "A1", false),
            pin(passive, (0.0, -22.86, 90.0), 5.08, "GND", "A12", true),
            pin(passive, (0.0, -22.86, 90.0), 5.08, "GND", "B1", true),
            pin(passive, (0.0, -22.86, 90.0), 5.08, "GND", "B12", true),
            pin(passive, (15.24, 15.24, 180.0), 5.08, "VBUS", "A4", false),
            pin(passive, (15.24, 15.24, 180.0), 5.08, "VBUS", "A9", true),
            pin(passive, (15.24, 15.24, 180.0), 5.08, "VBUS", "B4", true),
            pin(passive, (15.24, 15.24, 180.0), 5.08, "VBUS", "B9", true),
            pin(bi, (15.24, 10.16, 180.0), 5.08, "CC1", "A5", false),
            pin(bi, (15.24, 7.62, 180.0), 5.08, "CC2", "B5", false),
            pin(bi, (15.24, 2.54, 180.0), 5.08, "D-", "A7", false),
            pin(bi, (15.24, 0.0, 180.0), 5.08, "D-", "B7", false),
            pin(bi, (15.24, -2.54, 180.0), 5.08, "D+", "A6", false),
            pin(bi, (15.24, -5.08, 180.0), 5.08, "D+", "B6", false),
            pin(bi, (15.24, -12.7, 180.0), 5.08, "SBU1", "A8", false),
            pin(bi, (15.24, -15.24, 180.0), 5.08, "SBU2", "B8", false),
        ],
        glyph: usb_c(17.78),
        ..BASE
    });
    specs.push(Spec {
        id: "Connector:Conn_Coaxial".into(),
        reference: "J",
        description: "Coaxial connector".into(),
        reference_at: (0.254, 3.048, 0.0),
        value_at: (2.921, 0.0, 90.0),
        names: Names {
            offset: Some(1.016),
            hidden: HIDDEN,
        },
        pins: vec![
            pin(passive, (-5.08, 0.0, 0.0), 2.54, "In", "1", false),
            pin(passive, (0.0, -5.08, 90.0), 2.54, "Ext", "2", false),
        ],
        glyph: coaxial(),
        ..BASE
    });
    specs
}

/// The mechanical parts and the power flag.
fn mechanical_specs() -> Vec<Spec> {
    let passive = PinType::Passive;
    vec![
        Spec {
            id: "Connector:TestPoint".into(),
            reference: "TP",
            description: "Test point".into(),
            reference_at: (0.0, 6.858, 0.0),
            value_at: (0.0, 5.08, 0.0),
            numbers_hidden: HIDDEN,
            names: Names {
                offset: Some(0.762),
                hidden: HIDDEN,
            },
            pins: vec![pin(passive, (0.0, 0.0, 90.0), 2.54, "1", "1", false)],
            glyph: test_point(),
            ..BASE
        },
        Spec {
            id: "Mechanical:MountingHole".into(),
            reference: "H",
            description: "Mounting hole".into(),
            reference_at: (0.0, 5.08, 0.0),
            value_at: (0.0, 3.175, 0.0),
            glyph: mounting_hole(),
            in_bom: false,
            ..BASE
        },
        Spec {
            id: "Mechanical:MountingHole_Pad".into(),
            reference: "H",
            description: "Mounting hole with a connection to its plating".into(),
            reference_at: (0.0, 6.35, 0.0),
            value_at: (0.0, 4.445, 0.0),
            numbers_hidden: HIDDEN,
            names: Names {
                offset: Some(1.016),
                hidden: HIDDEN,
            },
            pins: vec![pin(
                PinType::Input,
                (0.0, -2.54, 90.0),
                2.54,
                "1",
                "1",
                false,
            )],
            glyph: plated_hole(),
            in_bom: false,
            ..BASE
        },
        Spec {
            id: "Mechanical:Fiducial".into(),
            reference: "FID",
            description: "Fiducial mark for the pick and place machine".into(),
            reference_at: (0.0, 5.08, 0.0),
            value_at: (0.0, 3.175, 0.0),
            names: Names {
                offset: None,
                hidden: SHOWN,
            },
            glyph: fiducial(),
            in_bom: false,
            ..BASE
        },
        Spec {
            id: crate::mapping::POWER_FLAG_SYMBOL.into(),
            reference: "#FLG",
            reference_hidden: true,
            description: "Tells the electrical rules check where power comes from".into(),
            reference_at: (0.0, 1.905, 0.0),
            value_at: (0.0, 3.81, 0.0),
            numbers_hidden: HIDDEN,
            names: Names {
                offset: Some(0.0),
                hidden: HIDDEN,
            },
            pins: vec![pin(
                PinType::PowerOut,
                (0.0, 0.0, 90.0),
                0.0,
                "~",
                "1",
                false,
            )],
            pin_part: "0_0",
            power: true,
            glyph: power_flag(),
            // As KiCad's own flag says; the flag the exporter places is `#FLG`, which no bill lists.
            in_bom: true,
            fields_left: false,
        },
    ]
}

/// Every symbol this module draws, in a fixed order.
fn specs() -> &'static [Spec] {
    static SPECS: OnceLock<Vec<Spec>> = OnceLock::new();
    SPECS.get_or_init(|| {
        let mut all = passive_specs();
        all.extend(connector_specs());
        all.extend(mechanical_specs());
        all
    })
}

/// The ids of the symbols made here, in a fixed order.
pub fn ids() -> impl Iterator<Item = &'static str> {
    specs().iter().map(|s| s.id.as_str())
}

/// Whether some id of `library` is made here.
#[cfg(test)]
pub(crate) fn covers_library(library: &str) -> bool {
    specs()
        .iter()
        .any(|s| s.id.split_once(':').is_some_and(|(l, _)| l == library))
}

fn effects(hide: bool, left: bool) -> Sexp {
    let mut items = vec![Sexp::node(
        "font",
        vec![Sexp::node("size", vec![Sexp::num(1.27), Sexp::num(1.27)])],
    )];
    if left {
        items.push(Sexp::node("justify", vec![Sexp::atom("left")]));
    }
    if hide {
        items.push(Sexp::node("hide", vec![Sexp::atom("yes")]));
    }
    Sexp::node("effects", items)
}

fn property(name: &str, value: &str, at: (f64, f64, f64), hide: bool, left: bool) -> Sexp {
    Sexp::node(
        "property",
        vec![
            Sexp::string(name),
            Sexp::string(value),
            Sexp::node(
                "at",
                vec![Sexp::num(at.0), Sexp::num(at.1), Sexp::num(at.2)],
            ),
            effects(hide, left),
        ],
    )
}

fn pin_node(spec: &PinSpec) -> Sexp {
    let mut items = vec![
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
    ];
    if spec.stacked {
        items.push(Sexp::node("hide", vec![Sexp::atom("yes")]));
    }
    items.push(Sexp::node(
        "name",
        vec![Sexp::string(&spec.name), effects(false, false)],
    ));
    items.push(Sexp::node(
        "number",
        vec![Sexp::string(&spec.number), effects(false, false)],
    ));
    Sexp::node("pin", items)
}

/// The name of the symbol without its library: `R` for `Device:R`.
fn bare(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, n)| n)
}

/// `(symbol "<name>" ...)` as a library file holds it, with the glyph in unit 0 (drawn with
/// every unit) and the pins in unit 1, or in the part the spec says.
fn definition(spec: &Spec) -> Sexp {
    let name = bare(&spec.id);
    let mut items = vec![Sexp::string(name)];
    if spec.power {
        items.push(Sexp::node("power", Vec::new()));
    }
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
    flag("pin_names", spec.names.hidden, spec.names.offset);
    items.extend([
        Sexp::node("exclude_from_sim", vec![Sexp::atom("no")]),
        Sexp::node("in_bom", vec![Sexp::yes_no(spec.in_bom)]),
        Sexp::node("on_board", vec![Sexp::atom("yes")]),
        property(
            "Reference",
            spec.reference,
            spec.reference_at,
            spec.reference_hidden,
            spec.fields_left,
        ),
        property("Value", name, spec.value_at, false, spec.fields_left),
        property("Footprint", "", (0.0, 0.0, 0.0), true, false),
        property("Datasheet", "~", (0.0, 0.0, 0.0), true, false),
        property(
            "Description",
            &spec.description,
            (0.0, 0.0, 0.0),
            true,
            false,
        ),
    ]);
    let mut drawing = vec![Sexp::string(format!("{name}_0_1"))];
    drawing.extend(spec.glyph.iter().map(Shape::node));
    items.push(Sexp::node("symbol", drawing));
    // A part with no pin (a mounting hole, a fiducial) has no part for them.
    if !spec.pins.is_empty() {
        let mut pins = vec![Sexp::string(format!("{name}_{}", spec.pin_part))];
        pins.extend(spec.pins.iter().map(pin_node));
        items.push(Sexp::node("symbol", pins));
    }
    Sexp::node("symbol", items)
}

/// The id of the symbol that answers the KiCad id `lib_id` (`Device:R` is `netsmith:R`), if
/// this module draws it. The symbol is a netsmith one and not KiCad's `Device:R`: a schematic
/// that says `Device:R` while it embeds another drawing gets a `lib_symbol_mismatch` warning
/// from KiCad on any machine that has the stock library, and a library of the board's own
/// (`netsmith.kicad_sym`, beside the schematic and in its library table) has none.
pub fn answer_id(lib_id: &str) -> Option<String> {
    let spec = specs().iter().find(|s| s.id == lib_id)?;
    Some(format!("{}:{}", crate::stdlib::NAME, bare(&spec.id)))
}

/// The symbol that answers the KiCad id `lib_id` (`Device:R`): its own id is
/// [`answer_id`].
pub fn symbol(lib_id: &str) -> Option<Symbol> {
    let spec = specs().iter().find(|s| s.id == lib_id)?;
    let own = answer_id(lib_id)?;
    Some(
        Symbol::from_definition(&own, bare(lib_id), definition(spec))
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
        // 14 passives and discrete parts, 7 headers, 4 sockets, 2 USB-C receptacles, the coaxial
        // connector, 4 mechanical parts and the power flag.
        assert_eq!(seen.len(), 33);
        // Two ids never answer with one symbol.
        let answers: std::collections::HashSet<String> =
            ids().map(|id| answer_id(id).unwrap()).collect();
        assert_eq!(answers.len(), seen.len());
        assert!(symbol("Device:Nothing").is_none());
        assert!(covers_library("Device") && covers_library("Switch"));
        assert!(covers_library("Connector") && covers_library("power"));
        assert!(!covers_library("Regulator_Linear"));
        assert!(symbol("Regulator_Linear:AP1117-15").is_none());
    }

    #[test]
    fn only_the_holes_and_the_fiducial_are_left_out_of_a_bill_of_materials() {
        // KiCad's own symbols say the same (`crates/netsmith-pipeline/tests/passives.rs` compares them):
        // a test point and a power flag are `in_bom yes`, and the flag is no part because its
        // placed instance is not (`#FLG` and `in_bom no` in the schematic).
        let out: Vec<&str> = ids().filter(|id| !symbol(id).unwrap().in_bom()).collect();
        assert_eq!(
            out,
            [
                "Mechanical:MountingHole",
                "Mechanical:MountingHole_Pad",
                "Mechanical:Fiducial"
            ]
        );
        for id in ids() {
            assert!(symbol(id).unwrap().on_board(), "{id}");
        }
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
        for id in ids().filter(|i| i.starts_with("Device:") || i.starts_with("Switch:")) {
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
