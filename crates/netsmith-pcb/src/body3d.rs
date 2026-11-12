//! Package shapes: the solids a part's body is drawn as in the 3D view, made from the numbers
//! the footprint generators already hold (ADR 0105, "parametric bodies").
//!
//! A generator keeps what it knows of the package on the footprint it makes
//! ([`Footprint::package`], a [`Package`]): the body, the span over the leads, the lead width and
//! length, the exposed pad, the ball pads, the pins of a header. [`package_shape`] turns that and
//! the part's height into simple solids (boxes, extruded polygons, vertical cylinders and
//! spheroids, each with a [`Material`]) that the glTF writer and the rasterizer triangulate. A
//! footprint that no generator made (a KiCad library file, a footprint of `std/libs/`) and the
//! generated families listed below as not shaped get the box of their _Body_ ([`Footprint::body`]).
//!
//! **The height is never taken from here (ADR 0094).** The caller gives the part's known height,
//! stated or from the catalog, and the shape is built to it: the highest point of every shape is
//! that height. With no known height the shape is the flat outline of the body and no solid
//! ([`BodyShape::solids`] is empty); the scene draws it hatched. No package standard's nominal
//! height is used (that is an open decision of ADR 0105).
//!
//! **The frame** is the footprint's own: millimetres, the origin at the footprint's origin, x and
//! y as the footprint file has them (y grows downward, as in KiCad), and z up from the face of the
//! board the part sits on (0 is the copper's surface). The scene turns, moves and mirrors it.
//!
//! **Where each dimension comes from.** Every size in plan is a number of the generator: the body,
//! the span tip to tip of the leads, the lead width and the length of its foot (nominal, the middle
//! of the range), the exposed pad, the ball pads and their grid, the header's pins (0.64 mm square
//! at 2.54 mm pitch, 0.5 mm at 2.0 mm, as `generate/header.rs` records). One height is the
//! generator's too: a pin header's plastic (2.5 mm at 2.54 mm pitch, 2.0 mm at 2.0 mm, one row or
//! two, each from a maker's drawing that `data/jlcpcb/README.md` names). It is a dimension of the
//! shape, under the part's height and never in place of it: the pins go on up to the height. A
//! few details are only for the picture: no check, report or written file reads them, none is a
//! height, and each is a constant below with a doc comment that says so:
//!
//! - [`LEAD_THICKNESS`], [`LEAD_EXIT`] and [`GULL_WING_STANDOFF`]: the bend of a gull-wing lead.
//! - [`NO_LEAD_TERMINAL_THICKNESS`], [`NO_LEAD_TERMINAL_EXPOSURE`] and [`NO_LEAD_STANDOFF`]: the
//!   metal under a QFN or DFN.
//! - [`BALL_PER_PAD`] and [`BALL_HEIGHT_PER_DIAMETER`]: a BGA's balls.
//! - [`SOCKET_OPENING_DEPTH`]: a socket's openings.
//! - [`CRYSTAL_LID_THICKNESS`] and [`CRYSTAL_LID_INSET`]: a crystal's metal lid.
//! - [`PIN1_MARK_RADIUS`], [`PIN1_MARK_DEPTH`] and [`CATHODE_BAND`]: the marks of pin 1 and of a
//!   cathode.
//!
//! **Shaped:** chips (R, C, L, LED, D, fuse; ceramic body and two end terminals), the molded
//! diodes SOD-123 and SOD-323 (two gull-wing terminals at the ends and a cathode band, from the
//! sourced dimensions in `generate/diode.rs`) and SMA, SMB and SMC (the body and its cathode band
//! only; their J-bend terminals are not in the data yet), SOT-23 (-3, -5, -6), SOT-223 (three
//! pins and the wide tab, from the sourced dimensions in `generate/sot.rs`), SOT-89-3 (three
//! short leads on one side and the wide tab of pin 2 under the body and out of the other, from
//! the sourced dimensions in `generate/tab.rs`), DPAK and D2PAK of two or three lead pads (the two
//! outer gull-wing leads on one side, and the tab out of the other and flat under the body, from
//! the sourced dimensions in `generate/tab.rs`), SOIC, SSOP, TSSOP and
//! MSOP (gull-wing leads on two edges), LQFP, TQFP and VQFP (gull-wing leads on four edges, and the
//! exposed pad), QFN and DFN (terminals and the exposed pad under the body), BGA (a ball under
//! every pad), pin headers and sockets, and SMD crystals (ceramic base and metal lid). **Left as
//! the box:** SC-70, the DPAK of four or five lead pads (TO-252-4, -5), the D2PAK of four to nine
//! (TO-263-4 to -9) and SOT-89-5, whose pads are recorded and whose lead widths and spans the data
//! does not hold, and the mechanical items (holes, fiducials, test points).

use crate::footprint::Footprint;
use crate::geometry::{Pt, Rect};

/// Drawing only: the thickness of a gull-wing lead, in mm, at most a quarter of the height.
pub const LEAD_THICKNESS: f64 = 0.15;
/// Drawing only: where a gull-wing lead leaves the body, as a share of the height.
pub const LEAD_EXIT: f64 = 0.4;
/// Drawing only: the gap under the body of a leaded package, in mm, at most a tenth of the height.
pub const GULL_WING_STANDOFF: f64 = 0.1;
/// Drawing only: the thickness of a QFN's or DFN's terminals and exposed pad, in mm, at most a
/// quarter of the height.
pub const NO_LEAD_TERMINAL_THICKNESS: f64 = 0.2;
/// Drawing only: how far a QFN's terminal shows past the side of the body, in mm (it is inside
/// the pad, which reaches further).
pub const NO_LEAD_TERMINAL_EXPOSURE: f64 = 0.02;
/// Drawing only: the gap under a QFN's or DFN's plastic, so that its metal shows from below, in
/// mm, at most a tenth of the height.
pub const NO_LEAD_STANDOFF: f64 = 0.02;
/// Drawing only: a BGA ball's diameter over its pad's.
pub const BALL_PER_PAD: f64 = 1.25;
/// Drawing only: a soldered ball's height over its diameter, at most half the part's height.
pub const BALL_HEIGHT_PER_DIAMETER: f64 = 0.8;
/// Drawing only: how deep a socket's opening is drawn below its top, in mm, at most half the
/// height.
pub const SOCKET_OPENING_DEPTH: f64 = 0.05;
/// Drawing only: the thickness of a crystal's metal lid, in mm, at most a quarter of the height.
pub const CRYSTAL_LID_THICKNESS: f64 = 0.1;
/// Drawing only: how far a crystal's lid stays in from the edge of its base, in mm, at most a
/// quarter of the shorter side.
pub const CRYSTAL_LID_INSET: f64 = 0.2;
/// Drawing only: the radius of the dot that marks pin 1, as a share of the body's shorter side
/// (between 0.1 and 0.5 mm).
pub const PIN1_MARK_RADIUS: f64 = 0.08;
/// Drawing only: how deep the pin 1 dot and the cathode band are drawn below the top, in mm, at
/// most half the height.
pub const PIN1_MARK_DEPTH: f64 = 0.02;
/// Drawing only: the width of a cathode band, as a share of the body's length.
pub const CATHODE_BAND: f64 = 0.15;

/// What a generator knows of a package's shape, kept on the footprint it made, in the
/// footprint's frame. The first of `leads`, `balls` or `pins` is pin 1.
#[derive(Debug, Clone, PartialEq)]
pub enum Package {
    /// A chip with a metal terminal over each end along x: the body (length along x, width), the
    /// length of a terminal, and whether pad 1 (at -x) is a cathode.
    Chip {
        body: (f64, f64),
        terminal: f64,
        polarised: bool,
    },
    /// A molded diode (SOD, SMA), pad 1 the cathode at -x: the body, and its two terminals where
    /// the generator holds them (`None` draws the body and its band only).
    Molded {
        body: (f64, f64),
        terminals: Option<Terminals>,
    },
    /// Gull-wing leads out of the body's edges: the body (x, y), the span tip to tip across x and
    /// across y (the latter only read for leads on the top and bottom edges), and the lead's width
    /// and the length of its foot.
    GullWing {
        body: (f64, f64),
        span: (f64, f64),
        lead_width: f64,
        lead_length: f64,
        leads: Vec<Lead>,
        exposed: Option<(f64, f64)>,
    },
    /// Flat terminals under the body's edges (QFN, DFN), each `lead_length` in from the edge.
    NoLead {
        body: (f64, f64),
        lead_width: f64,
        lead_length: f64,
        leads: Vec<Lead>,
        exposed: Option<(f64, f64)>,
    },
    /// A ball under the centre of each pad, `pad` the pad's diameter.
    Bga {
        body: (f64, f64),
        pad: f64,
        balls: Vec<Pt>,
    },
    /// A pin header or socket: the plastic's outline, the pitch, the side of a square pin, the
    /// height of a header's plastic above the board (a maker's figure that the generator
    /// records; a socket's plastic is its whole height, so a socket does not read it), the pins.
    Header {
        body: Rect,
        pitch: f64,
        pin: f64,
        plastic: f64,
        pins: Vec<Pt>,
        socket: bool,
    },
    /// A ceramic crystal package with a metal lid.
    Crystal { body: (f64, f64) },
}

/// The two terminals of a molded diode, one out of each end of the body along x, in the middle
/// of its width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Terminals {
    /// The span tip to tip along x, in mm.
    pub span: f64,
    /// The width of a terminal, in mm.
    pub width: f64,
    /// The length of a terminal's foot on the board, in mm.
    pub foot: f64,
}

/// One lead of a package: the edge of the body it leaves, and its centre along that edge (y for
/// the left and right edges, x for the top and bottom ones). A lead is as wide as its package's
/// `lead_width` unless it states its own `width` (the tab of a SOT-223 is far wider than its
/// pins). A lead reaches as far as its package's `span` unless it states its own `reach` (the
/// tab of a SOT-89 stops short of its pins). A lead that states `under` is flat metal on the
/// board, from that far in under the body (a SOT-89's exposed tab) out to its tip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lead {
    pub edge: Edge,
    pub at: f64,
    pub width: Option<f64>,
    /// How far the tip is beyond the body's edge, in mm.
    pub reach: Option<f64>,
    /// How far in under the body, from its edge, the lead runs flat on the board, in mm.
    pub under: Option<f64>,
}

/// An edge of a package's body: left is -x, top is -y (the footprint's y grows downward).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Lead {
    /// The lead under the pad at `at`, on the edge of a body of `body` (x, y) it is beyond.
    pub fn at_pad(at: Pt, body: (f64, f64)) -> Lead {
        let (rx, ry) = (at.x.abs() / body.0, at.y.abs() / body.1);
        if rx >= ry {
            let edge = if at.x < 0.0 { Edge::Left } else { Edge::Right };
            Lead {
                edge,
                at: at.y,
                width: None,
                reach: None,
                under: None,
            }
        } else {
            let edge = if at.y < 0.0 { Edge::Top } else { Edge::Bottom };
            Lead {
                edge,
                at: at.x,
                width: None,
                reach: None,
                under: None,
            }
        }
    }
}

/// What a solid is made of, for its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Material {
    /// The plastic or ceramic body.
    Body,
    /// A lead, a terminal, an exposed pad, a pin or a lid.
    Metal,
    /// A mark on the top: pin 1's dot, a cathode band, a socket's opening.
    Mark,
    /// A solder ball.
    Ball,
}

/// An axis of the footprint's frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// A simple solid, easy to triangulate.
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    /// A box along the axes.
    Box { min: [f64; 3], max: [f64; 3] },
    /// A simple polygon in the plane across `axis`, extruded along it from `from` to `to`. The
    /// polygon's points are (x, y) across z, (x, z) across y and (y, z) across x.
    Prism {
        axis: Axis,
        profile: Vec<[f64; 2]>,
        from: f64,
        to: f64,
    },
    /// An upright cylinder.
    Cylinder {
        centre: Pt,
        radius: f64,
        z0: f64,
        z1: f64,
    },
    /// An ellipsoid of `radius` in plan, filling `z0` to `z1` upright.
    Spheroid {
        centre: Pt,
        radius: f64,
        z0: f64,
        z1: f64,
    },
}

/// A solid of a package shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Solid {
    pub material: Material,
    pub primitive: Primitive,
}

/// A box in the footprint's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds {
    /// The smallest box holding both.
    pub fn union(&self, o: &Bounds) -> Bounds {
        let mut out = *self;
        for i in 0..3 {
            out.min[i] = out.min[i].min(o.min[i]);
            out.max[i] = out.max[i].max(o.max[i]);
        }
        out
    }
}

impl Solid {
    /// The box the solid fills.
    pub fn bounds(&self) -> Bounds {
        match &self.primitive {
            Primitive::Box { min, max } => Bounds {
                min: *min,
                max: *max,
            },
            Primitive::Prism {
                axis,
                profile,
                from,
                to,
            } => {
                let (mut a0, mut a1, mut b0, mut b1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
                for p in profile {
                    a0 = a0.min(p[0]);
                    a1 = a1.max(p[0]);
                    b0 = b0.min(p[1]);
                    b1 = b1.max(p[1]);
                }
                let (lo, hi) = (from.min(*to), from.max(*to));
                let (min, max) = match axis {
                    Axis::Z => ([a0, b0, lo], [a1, b1, hi]),
                    Axis::Y => ([a0, lo, b0], [a1, hi, b1]),
                    Axis::X => ([lo, a0, b0], [hi, a1, b1]),
                };
                Bounds { min, max }
            }
            Primitive::Cylinder {
                centre,
                radius,
                z0,
                z1,
            }
            | Primitive::Spheroid {
                centre,
                radius,
                z0,
                z1,
            } => Bounds {
                min: [centre.x - radius, centre.y - radius, *z0],
                max: [centre.x + radius, centre.y + radius, *z1],
            },
        }
    }
}

/// A part's body as the 3D view draws it, in the footprint's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyShape {
    /// The body's outline on the board ([`Footprint::body`]), drawn hatched when there are no
    /// solids.
    pub outline: [Pt; 4],
    /// The solids, none when the height is unknown.
    pub solids: Vec<Solid>,
    /// Whether the solids are the package's shape, not the box of the body.
    pub shaped: bool,
}

impl BodyShape {
    /// The box all the solids fill, if there are any.
    pub fn bounds(&self) -> Option<Bounds> {
        self.solids
            .iter()
            .map(Solid::bounds)
            .reduce(|a, b| a.union(&b))
    }
}

/// The shape of `footprint`'s body for a part of `height` mm, or its flat outline when the
/// height is unknown (`None`) or not above zero.
pub fn package_shape(footprint: &Footprint, height: Option<f64>) -> BodyShape {
    let b = footprint.body;
    let outline = [
        b.min,
        Pt::new(b.max.x, b.min.y),
        b.max,
        Pt::new(b.min.x, b.max.y),
    ];
    let Some(h) = height.filter(|h| h.is_finite() && *h > 0.0) else {
        return BodyShape {
            outline,
            solids: Vec::new(),
            shaped: false,
        };
    };
    let (solids, shaped) = match &footprint.package {
        Some(package) => (shape(package, h), true),
        None => (
            vec![solid(
                Material::Body,
                cuboid(b.min.x, b.min.y, 0.0, b.max.x, b.max.y, h),
            )],
            false,
        ),
    };
    BodyShape {
        outline,
        solids,
        shaped,
    }
}

fn solid(material: Material, primitive: Primitive) -> Solid {
    Solid {
        material,
        primitive,
    }
}

fn cuboid(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> Primitive {
    Primitive::Box {
        min: [x0, y0, z0],
        max: [x1, y1, z1],
    }
}

/// A box of `(w, l)` centred on the origin from `z0` to `z1`.
fn centred(size: (f64, f64), z0: f64, z1: f64) -> Primitive {
    let (hx, hy) = (size.0 / 2.0, size.1 / 2.0);
    cuboid(-hx, -hy, z0, hx, hy, z1)
}

fn shape(package: &Package, h: f64) -> Vec<Solid> {
    match package {
        Package::Chip {
            body,
            terminal,
            polarised,
        } => chip(*body, *terminal, *polarised, h),
        Package::Molded { body, terminals } => molded(*body, *terminals, h),
        Package::GullWing {
            body,
            span,
            lead_width,
            lead_length,
            leads,
            exposed,
        } => {
            let mut out = gull_wing(*body, *span, *lead_width, *lead_length, leads, *exposed, h);
            if let Some(p1) = first_point(leads, *body) {
                out.push(pin1_mark(*body, p1, h));
            }
            out
        }
        Package::NoLead {
            body,
            lead_width,
            lead_length,
            leads,
            exposed,
        } => no_lead(*body, *lead_width, *lead_length, leads, *exposed, h),
        Package::Bga { body, pad, balls } => bga(*body, *pad, balls, h),
        Package::Header {
            body,
            pin,
            plastic,
            pins,
            socket,
            ..
        } => header(*body, *pin, *plastic, pins, *socket, h),
        Package::Crystal { body } => crystal(*body, h),
    }
}

/// The depth of a mark on the top of a part `h` tall.
fn mark_depth(h: f64) -> f64 {
    PIN1_MARK_DEPTH.min(h / 2.0)
}

/// A band across the top of a body near its -x end, `start` mm in from the end.
fn cathode_band(body: (f64, f64), start: f64, h: f64) -> Solid {
    let (hx, hy) = (body.0 / 2.0, body.1 / 2.0);
    let x0 = -hx + start + CATHODE_BAND * body.0 * 0.5;
    let x1 = (x0 + CATHODE_BAND * body.0).min(hx);
    solid(
        Material::Mark,
        cuboid(x0, -hy, h - mark_depth(h), x1, hy, h),
    )
}

/// The dot of pin 1 on the top of a body of `body`, in the corner nearest `pin1`.
fn pin1_mark(body: (f64, f64), pin1: Pt, h: f64) -> Solid {
    let radius = (PIN1_MARK_RADIUS * body.0.min(body.1)).clamp(0.1, 0.5);
    let radius = radius.min(body.0.min(body.1) / 4.0);
    let toward = |v: f64, half: f64| {
        if v.abs() < 1e-9 {
            0.0
        } else {
            v.signum() * (half - 2.0 * radius).max(0.0)
        }
    };
    let centre = Pt::new(toward(pin1.x, body.0 / 2.0), toward(pin1.y, body.1 / 2.0));
    solid(
        Material::Mark,
        Primitive::Cylinder {
            centre,
            radius,
            z0: h - mark_depth(h),
            z1: h,
        },
    )
}

fn chip(body: (f64, f64), terminal: f64, polarised: bool, h: f64) -> Vec<Solid> {
    let (hx, hy) = (body.0 / 2.0, body.1 / 2.0);
    let t = terminal.clamp(0.0, hx);
    let mut out = vec![
        solid(Material::Body, cuboid(-hx + t, -hy, 0.0, hx - t, hy, h)),
        solid(Material::Metal, cuboid(-hx, -hy, 0.0, -hx + t, hy, h)),
        solid(Material::Metal, cuboid(hx - t, -hy, 0.0, hx, hy, h)),
    ];
    if polarised {
        out.push(cathode_band(body, t, h));
    }
    out
}

/// The point at the position `at` along `edge`, `out` mm from the centre.
fn edge_point(edge: Edge, at: f64, out: f64) -> Pt {
    match edge {
        Edge::Left => Pt::new(-out, at),
        Edge::Right => Pt::new(out, at),
        Edge::Top => Pt::new(at, -out),
        Edge::Bottom => Pt::new(at, out),
    }
}

/// The half of the body and of the span across `edge`.
fn across(edge: Edge, of: (f64, f64)) -> f64 {
    match edge {
        Edge::Left | Edge::Right => of.0 / 2.0,
        Edge::Top | Edge::Bottom => of.1 / 2.0,
    }
}

fn first_point(leads: &[Lead], body: (f64, f64)) -> Option<Pt> {
    leads
        .first()
        .map(|l| edge_point(l.edge, l.at, across(l.edge, body)))
}

fn gull_wing(
    body: (f64, f64),
    span: (f64, f64),
    width: f64,
    foot: f64,
    leads: &[Lead],
    exposed: Option<(f64, f64)>,
    h: f64,
) -> Vec<Solid> {
    let t = LEAD_THICKNESS.min(h / 4.0);
    let standoff = GULL_WING_STANDOFF.min(h / 10.0);
    // The underside of the lead where it leaves the body.
    let e = (LEAD_EXIT * h - t / 2.0).max(standoff).min(h - t);
    let mut out = vec![solid(Material::Body, centred(body, standoff, h))];
    for lead in leads {
        let be = across(lead.edge, body);
        let tip = match lead.reach {
            Some(reach) => be + reach,
            None => across(lead.edge, span).max(be),
        };
        let foot_in = (tip - foot).max(be);
        let knee = be + (foot_in - be) / 2.0;
        // (outward, z), round the lead's side from the body's edge.
        let path: Vec<(f64, f64)> = match lead.under {
            // Flat on the board, from under the body to the tip.
            Some(under) => vec![(be - under, 0.0), (tip, 0.0), (tip, t), (be - under, t)],
            None => vec![
                (be, e),
                ((knee - t).max(be), e),
                ((foot_in - t).max(be), 0.0),
                (tip, 0.0),
                (tip, t),
                (foot_in, t),
                (knee, e + t),
                (be, e + t),
            ],
        };
        let width = lead.width.unwrap_or(width);
        let sign = match lead.edge {
            Edge::Left | Edge::Top => -1.0,
            Edge::Right | Edge::Bottom => 1.0,
        };
        let profile = path.iter().map(|&(o, z)| [sign * o, z]).collect();
        let axis = match lead.edge {
            Edge::Left | Edge::Right => Axis::Y,
            Edge::Top | Edge::Bottom => Axis::X,
        };
        out.push(solid(
            Material::Metal,
            Primitive::Prism {
                axis,
                profile,
                from: lead.at - width / 2.0,
                to: lead.at + width / 2.0,
            },
        ));
    }
    if let Some(ep) = exposed {
        out.push(solid(Material::Metal, centred(ep, 0.0, standoff.max(t))));
    }
    out
}

/// A molded diode: the body, its two gull-wing terminals where the generator holds them, and the
/// cathode band at -x.
fn molded(body: (f64, f64), terminals: Option<Terminals>, h: f64) -> Vec<Solid> {
    let mut out = match terminals {
        Some(t) => {
            let lead = |edge| Lead {
                edge,
                at: 0.0,
                width: None,
                reach: None,
                under: None,
            };
            let leads = [lead(Edge::Left), lead(Edge::Right)];
            gull_wing(body, (t.span, body.1), t.width, t.foot, &leads, None, h)
        }
        None => vec![solid(Material::Body, centred(body, 0.0, h))],
    };
    out.push(cathode_band(body, 0.0, h));
    out
}

fn no_lead(
    body: (f64, f64),
    width: f64,
    length: f64,
    leads: &[Lead],
    exposed: Option<(f64, f64)>,
    h: f64,
) -> Vec<Solid> {
    let t = NO_LEAD_TERMINAL_THICKNESS.min(h / 4.0);
    let standoff = NO_LEAD_STANDOFF.min(h / 10.0);
    let mut out = vec![solid(Material::Body, centred(body, standoff, h))];
    for lead in leads {
        let be = across(lead.edge, body);
        let width = lead.width.unwrap_or(width);
        let (inner, outer) = ((be - length).max(0.0), be + NO_LEAD_TERMINAL_EXPOSURE);
        let a = edge_point(lead.edge, lead.at - width / 2.0, inner);
        let b = edge_point(lead.edge, lead.at + width / 2.0, outer);
        out.push(solid(
            Material::Metal,
            cuboid(
                a.x.min(b.x),
                a.y.min(b.y),
                0.0,
                a.x.max(b.x),
                a.y.max(b.y),
                t,
            ),
        ));
    }
    if let Some(ep) = exposed {
        out.push(solid(Material::Metal, centred(ep, 0.0, t)));
    }
    if let Some(p1) = first_point(leads, body) {
        out.push(pin1_mark(body, p1, h));
    }
    out
}

fn bga(body: (f64, f64), pad: f64, balls: &[Pt], h: f64) -> Vec<Solid> {
    let diameter = pad * BALL_PER_PAD;
    let ball = (diameter * BALL_HEIGHT_PER_DIAMETER).min(h / 2.0);
    let mut out = vec![solid(Material::Body, centred(body, ball, h))];
    for &centre in balls {
        out.push(solid(
            Material::Ball,
            Primitive::Spheroid {
                centre,
                radius: diameter / 2.0,
                z0: 0.0,
                z1: ball,
            },
        ));
    }
    if let Some(&a1) = balls.first() {
        out.push(pin1_mark(body, a1, h));
    }
    out
}

fn header(body: Rect, pin: f64, plastic: f64, pins: &[Pt], socket: bool, h: f64) -> Vec<Solid> {
    let p = pin / 2.0;
    let mut out = Vec::new();
    if socket {
        out.push(solid(
            Material::Body,
            cuboid(body.min.x, body.min.y, 0.0, body.max.x, body.max.y, h),
        ));
        let depth = SOCKET_OPENING_DEPTH.min(h / 2.0);
        for c in pins {
            out.push(solid(
                Material::Mark,
                cuboid(c.x - p, c.y - p, h - depth, c.x + p, c.y + p, h),
            ));
        }
    } else {
        // The plastic, at most the part's height: the pins end at the height.
        let insulator = plastic.min(h);
        out.push(solid(
            Material::Body,
            cuboid(
                body.min.x, body.min.y, 0.0, body.max.x, body.max.y, insulator,
            ),
        ));
        for c in pins {
            out.push(solid(
                Material::Metal,
                cuboid(c.x - p, c.y - p, 0.0, c.x + p, c.y + p, h),
            ));
        }
    }
    out
}

fn crystal(body: (f64, f64), h: f64) -> Vec<Solid> {
    let lid = CRYSTAL_LID_THICKNESS.min(h / 4.0);
    let inset = CRYSTAL_LID_INSET.min(body.0.min(body.1) / 4.0);
    vec![
        solid(Material::Body, centred(body, 0.0, h - lid)),
        solid(
            Material::Metal,
            centred((body.0 - 2.0 * inset, body.1 - 2.0 * inset), h - lid, h),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::generate;
    use netsmith_kicad::footprint_spec::FootprintSpec;

    const EPS: f64 = 1e-9;

    fn made(id: &str) -> Footprint {
        generate(id).unwrap_or_else(|| panic!("{id} is not generated"))
    }

    fn spec(text: &str) -> Footprint {
        let spec = FootprintSpec::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        crate::generate::generate_spec(&spec).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    fn count(shape: &BodyShape, material: Material) -> usize {
        shape
            .solids
            .iter()
            .filter(|s| s.material == material)
            .count()
    }

    /// The box round the body and the copper pads.
    fn envelope(fp: &Footprint) -> Rect {
        fp.pads
            .iter()
            .filter(|p| p.layers.iter().any(|l| l.ends_with(".Cu")))
            // A custom pad's box takes in its polygon (the tab of a SOT-89).
            .fold(fp.body, |r, p| r.union(&p.bounds()))
    }

    /// Every solid inside the body and the pads in plan and between the board and the height,
    /// and the top of the shape at the height.
    fn fits(id: &str, fp: &Footprint, h: f64) -> BodyShape {
        let shape = package_shape(fp, Some(h));
        assert!(shape.shaped, "{id} is not shaped");
        let env = envelope(fp);
        for s in &shape.solids {
            let b = s.bounds();
            assert!(
                b.min[0] >= env.min.x - EPS
                    && b.max[0] <= env.max.x + EPS
                    && b.min[1] >= env.min.y - EPS
                    && b.max[1] <= env.max.y + EPS,
                "{id}: {s:?} is outside {env:?}"
            );
            assert!(
                b.min[2] >= -EPS && b.max[2] <= h + EPS,
                "{id}: {s:?} is outside 0 to {h}"
            );
            assert!((0..3).all(|i| b.min[i] <= b.max[i]), "{id}: {s:?} is empty");
        }
        let top = shape.bounds().unwrap().max[2];
        assert!((top - h).abs() < EPS, "{id}: top {top}, height {h}");
        shape
    }

    /// The metal solids of a shape, ordered along y.
    fn metal_of(shape: &BodyShape) -> Vec<Bounds> {
        let mut metal: Vec<Bounds> = shape
            .solids
            .iter()
            .filter(|s| s.material == Material::Metal)
            .map(Solid::bounds)
            .collect();
        metal.sort_by(|a, b| {
            a.min[1]
                .total_cmp(&b.min[1])
                .then(b.max[1].total_cmp(&a.max[1]))
        });
        metal
    }

    fn mark_of(shape: &BodyShape) -> Bounds {
        shape
            .solids
            .iter()
            .find(|s| s.material == Material::Mark)
            .unwrap()
            .bounds()
    }

    fn centre(b: &Bounds) -> f64 {
        (b.min[1] + b.max[1]) / 2.0
    }

    fn width(b: &Bounds) -> f64 {
        b.max[1] - b.min[1]
    }

    fn body_solid(shape: &BodyShape) -> Bounds {
        shape
            .solids
            .iter()
            .find(|s| s.material == Material::Body)
            .unwrap()
            .bounds()
    }

    #[test]
    fn a_chip_resistor_is_a_body_between_two_terminals_to_its_height() {
        let fp = made("Resistor_SMD:R_0603_1608Metric");
        let shape = fits("R_0603", &fp, 0.45);
        assert_eq!(count(&shape, Material::Metal), 2);
        assert_eq!(count(&shape, Material::Mark), 0);
        // 1.6 x 0.825 mm body, terminals 0.275 mm long (IPC-SM-782: 0.15 to 0.4).
        let all = shape.bounds().unwrap();
        assert!((all.max[0] - 0.8).abs() < EPS && (all.max[1] - 0.4125).abs() < EPS);
        let body = body_solid(&shape);
        assert!((body.max[0] - (0.8 - 0.275)).abs() < EPS, "{body:?}");
    }

    #[test]
    fn a_chip_led_has_a_cathode_band_by_pad_1() {
        let fp = made("LED_SMD:LED_0805_2012Metric");
        let shape = fits("LED_0805", &fp, 0.8);
        let band = shape
            .solids
            .iter()
            .find(|s| s.material == Material::Mark)
            .unwrap()
            .bounds();
        assert!(band.max[0] < 0.0, "the band is at the -x end: {band:?}");
    }

    #[test]
    fn an_sma_smb_or_smc_is_still_its_body_and_a_band() {
        let fp = made("Diode_SMD:D_SMA");
        let shape = fits("D_SMA", &fp, 2.3);
        assert_eq!(shape.solids.len(), 2);
        let body = body_solid(&shape);
        assert!((body.max[0] - 2.3).abs() < EPS && (body.max[1] - 1.5).abs() < EPS);
        for id in ["Diode_SMD:D_SMB", "Diode_SMD:D_SMC"] {
            let shape = fits(id, &made(id), 2.3);
            assert_eq!(count(&shape, Material::Metal), 0, "{id}");
            assert_eq!(count(&shape, Material::Mark), 1, "{id}");
        }
    }

    #[test]
    fn a_sod_diode_has_two_terminals_and_a_band_to_its_sourced_dimensions() {
        // (id, body x, body y, span, terminal width, foot): Diodes' typical values, checked
        // against onsemi's cases 425 and 477.
        for (id, bx, by, span, w, foot) in [
            ("Diode_SMD:D_SOD-123", 2.8, 1.8, 3.65, 0.57, 0.30),
            ("Diode_SMD:D_SOD-323", 1.8, 1.4, 2.50, 0.30, 0.30),
        ] {
            let shape = fits(id, &made(id), 1.2);
            assert!(shape.solids.len() > 2, "{id} is still a box and a band");
            assert_eq!(count(&shape, Material::Metal), 2, "{id}");
            assert_eq!(count(&shape, Material::Mark), 1, "{id}");
            let body = body_solid(&shape);
            assert!((body.max[0] - bx / 2.0).abs() < EPS, "{id}: {body:?}");
            assert!((body.max[1] - by / 2.0).abs() < EPS, "{id}: {body:?}");
            let metal = metal_of(&shape);
            assert_eq!(metal.len(), 2, "{id}");
            let (left, right) = if metal[0].max[0] < 0.0 {
                (&metal[0], &metal[1])
            } else {
                (&metal[1], &metal[0])
            };
            // The span tip to tip, each terminal in the middle of the width and as wide as sourced.
            assert!((right.max[0] - left.min[0] - span).abs() < EPS, "{id}");
            assert!((left.min[0] + span / 2.0).abs() < EPS, "{id}: {left:?}");
            assert!((right.max[0] - span / 2.0).abs() < EPS, "{id}: {right:?}");
            for b in [left, right] {
                assert!(
                    centre(b).abs() < EPS && (width(b) - w).abs() < EPS,
                    "{id}: {b:?}"
                );
                assert!(b.min[2].abs() < EPS, "{id}: {b:?}");
            }
            // The foot is on the board at each tip: `foot` long past the bend, or the whole reach
            // where the bend would start inside the body.
            let reach = (span - bx) / 2.0;
            for solid in shape
                .solids
                .iter()
                .filter(|s| s.material == Material::Metal)
            {
                let Primitive::Prism { profile, .. } = &solid.primitive else {
                    panic!("{id}: a terminal is a prism");
                };
                let flat: Vec<f64> = profile
                    .iter()
                    .filter(|p| p[1] == 0.0)
                    .map(|p| p[0])
                    .collect();
                let long = flat.iter().fold(f64::MIN, |a, &x| a.max(x))
                    - flat.iter().fold(f64::MAX, |a, &x| a.min(x));
                assert!(
                    (long - (foot + LEAD_THICKNESS).min(reach)).abs() < EPS,
                    "{id}: {long}"
                );
            }
            // The cathode band is at -x, across the whole width of the body.
            let band = mark_of(&shape);
            assert!(
                band.max[0] <= 0.0 && band.min[0] >= -bx / 2.0,
                "{id}: {band:?}"
            );
            assert!((width(&band) - by).abs() < EPS, "{id}: {band:?}");
            // The height is the given one and not changed by the terminals.
            assert!((shape.bounds().unwrap().max[2] - 1.2).abs() < EPS, "{id}");
        }
    }

    #[test]
    fn an_soic_has_a_gull_wing_lead_for_every_pin_reaching_its_span() {
        let fp = made("Package_SO:SOIC-8_3.9x4.9mm_P1.27mm");
        let shape = fits("SOIC-8", &fp, 1.75);
        assert_eq!(count(&shape, Material::Metal), 8);
        assert_eq!(count(&shape, Material::Mark), 1);
        let leads: Vec<Bounds> = shape
            .solids
            .iter()
            .filter(|s| s.material == Material::Metal)
            .map(Solid::bounds)
            .collect();
        // Tip to tip 6.0 mm (MS-012), from the body's edge at 1.95 mm, 0.41 mm wide at 1.27 mm.
        let reach = leads.iter().map(|b| b.max[0]).fold(f64::MIN, f64::max);
        assert!((reach - 3.0).abs() < EPS, "{reach}");
        for b in &leads {
            assert!((b.max[1] - b.min[1] - 0.41).abs() < EPS, "{b:?}");
            assert!(b.min[2].abs() < EPS, "a foot is on the board: {b:?}");
            assert!((b.min[0].abs().min(b.max[0].abs()) - 1.95).abs() < EPS);
        }
        let mut ys: Vec<f64> = leads.iter().map(|b| (b.min[1] + b.max[1]) / 2.0).collect();
        ys.sort_by(f64::total_cmp);
        assert!((ys[0] + 1.905).abs() < EPS && (ys[1] + 1.905).abs() < EPS);
        // Pin 1's dot is in the top left corner.
        let mark = shape
            .solids
            .iter()
            .find(|s| s.material == Material::Mark)
            .unwrap()
            .bounds();
        assert!(mark.max[0] < 0.0 && mark.max[1] < 0.0, "{mark:?}");
    }

    #[test]
    fn a_tssop_and_a_sot_23_have_their_leads() {
        for (id, pins, h) in [
            ("Package_SO:TSSOP-20_4.4x6.5mm_P0.65mm", 20, 1.2),
            ("Package_TO_SOT_SMD:SOT-23", 3, 1.1),
            ("Package_TO_SOT_SMD:SOT-23-6", 6, 1.45),
        ] {
            let shape = fits(id, &made(id), h);
            assert_eq!(count(&shape, Material::Metal), pins, "{id}");
        }
    }

    #[test]
    fn a_sot_223_has_three_pins_and_a_wide_tab_to_its_sourced_dimensions() {
        // Diodes Inc. "SOT223 Package Information" (2017-03-06), checked against onsemi 318E-04.
        for (id, h) in [
            ("Package_TO_SOT_SMD:SOT-223", 1.8),
            ("Package_TO_SOT_SMD:SOT-223-3_TabPin2", 1.65),
        ] {
            let fp = made(id);
            let shape = fits(id, &fp, h);
            assert!(shape.shaped, "{id} is not a box");
            assert_eq!(
                count(&shape, Material::Metal),
                4,
                "{id}: three pins, the tab"
            );
            assert_eq!(count(&shape, Material::Mark), 1, "{id}");
            // The body is 3.5 x 6.5 mm.
            let body = body_solid(&shape);
            assert!((body.max[0] - 1.75).abs() < EPS && (body.min[0] + 1.75).abs() < EPS);
            assert!((body.max[1] - 3.25).abs() < EPS && (body.min[1] + 3.25).abs() < EPS);
            let metal: Vec<Bounds> = shape
                .solids
                .iter()
                .filter(|s| s.material == Material::Metal)
                .map(Solid::bounds)
                .collect();
            // Tip to tip 7.0 mm, the feet 0.95 mm long on the board.
            let (left, right): (Vec<&Bounds>, Vec<&Bounds>) =
                metal.iter().partition(|b| b.max[0] < 0.0);
            assert_eq!((left.len(), right.len()), (3, 1), "{id}");
            for b in &left {
                assert!((b.min[0] + 3.5).abs() < EPS, "{id}: {b:?}");
                assert!((b.max[1] - b.min[1] - 0.70).abs() < EPS, "pin width {b:?}");
                assert!(b.min[2].abs() < EPS);
            }
            let ys: Vec<f64> = left.iter().map(|b| (b.min[1] + b.max[1]) / 2.0).collect();
            assert!(ys.iter().any(|y| (y + 2.3).abs() < EPS), "{ys:?}");
            assert!(ys.iter().any(|y| y.abs() < EPS), "{ys:?}");
            assert!(ys.iter().any(|y| (y - 2.3).abs() < EPS), "{ys:?}");
            let tab = right[0];
            assert!((tab.max[0] - 3.5).abs() < EPS, "{id}: {tab:?}");
            assert!((tab.max[1] - tab.min[1] - 3.00).abs() < EPS, "tab {tab:?}");
            assert!((tab.max[1] + tab.min[1]).abs() < EPS, "the tab is centred");
            // The metal under the foot is as long as the foot: 0.95 mm from the tip.
            let foot = tab.max[0] - 0.95;
            assert!(foot > 1.75, "the foot starts outside the body: {foot}");
            // Pin 1's dot is on the pins' side.
            let mark = shape
                .solids
                .iter()
                .find(|s| s.material == Material::Mark)
                .unwrap()
                .bounds();
            assert!(mark.max[0] < 0.0 && mark.max[1] < 0.0, "{mark:?}");
        }
    }

    #[test]
    fn a_sot_89_has_three_short_leads_and_a_wide_tab_to_its_sourced_dimensions() {
        // Diodes Inc. "SOT89 Package Information" (Rev. 2026-01-16), checked against onsemi 528AG.
        for id in [
            "Package_TO_SOT_SMD:SOT-89-3",
            "Package_TO_SOT_SMD:SOT-89-3_Handsoldering",
        ] {
            let fp = made(id);
            let shape = fits(id, &fp, 1.5);
            assert!(shape.shaped, "{id} is not a box");
            // Three leads on one side (pin 2's narrow end among them) and the tab.
            assert_eq!(count(&shape, Material::Metal), 4, "{id}");
            assert_eq!(count(&shape, Material::Mark), 1, "{id}");
            // The body is 2.5 x 4.5 mm.
            let body = body_solid(&shape);
            assert!((body.max[0] - 1.25).abs() < EPS && (body.min[0] + 1.25).abs() < EPS);
            assert!((body.max[1] - 2.25).abs() < EPS && (body.min[1] + 2.25).abs() < EPS);
            let mut metal: Vec<Bounds> = shape
                .solids
                .iter()
                .filter(|s| s.material == Material::Metal)
                .map(Solid::bounds)
                .collect();
            metal.sort_by(|a, b| a.min[1].total_cmp(&b.min[1]));
            let (left, right): (Vec<&Bounds>, Vec<&Bounds>) =
                metal.iter().partition(|b| b.max[0] < 0.0);
            assert_eq!((left.len(), right.len()), (3, 1), "{id}");
            // Pins at 1.5 mm pitch, 1.05 mm out of the body (tip at -2.30 mm), flat on the board.
            let centre = |b: &Bounds| (b.min[1] + b.max[1]) / 2.0;
            let width = |b: &Bounds| b.max[1] - b.min[1];
            for (b, (at, w)) in left.iter().zip([(-1.5, 0.48), (0.0, 0.56), (1.5, 0.48)]) {
                assert!((centre(b) - at).abs() < EPS, "{id}: {b:?}");
                assert!((width(b) - w).abs() < EPS, "{id}: width of {b:?}");
                assert!((b.min[0] + 2.30).abs() < EPS, "{id}: {b:?}");
                assert!(b.min[2].abs() < EPS);
            }
            // The outer pins stay outside the body; the centre lead runs 0.30 mm in under it.
            assert!((left[0].max[0] + 1.25).abs() < EPS, "{id}: {:?}", left[0]);
            assert!((left[1].max[0] + 0.95).abs() < EPS, "{id}: {:?}", left[1]);
            // The tab: 1.733 mm wide, from 0.55 mm beyond the body (tip at 1.80 mm) in under it
            // for 2.20 mm, where the centre lead takes over.
            let tab = &right[0];
            assert!((width(tab) - 1.733).abs() < EPS, "{id}: {tab:?}");
            assert!(centre(tab).abs() < EPS, "the tab is centred");
            assert!((tab.max[0] - 1.80).abs() < EPS, "{id}: {tab:?}");
            assert!((tab.min[0] + 0.95).abs() < EPS, "{id}: {tab:?}");
            assert!(tab.min[0] < body.max[0] && tab.min[0] > body.min[0]);
            assert!(tab.min[2].abs() < EPS);
            // The metal of pin 2 is one piece from tip to tip: the two parts meet.
            assert!((left[1].max[0] - tab.min[0]).abs() < EPS);
            // Tip to tip is the sourced 4.10 mm overall.
            assert!((tab.max[0] - left[0].min[0] - 4.10).abs() < EPS);
            // Pin 1's dot is on the pins' side.
            let mark = shape
                .solids
                .iter()
                .find(|s| s.material == Material::Mark)
                .unwrap()
                .bounds();
            assert!(mark.max[0] < 0.0 && mark.max[1] < 0.0, "{mark:?}");
        }
    }

    #[test]
    fn a_dpak_has_two_leads_and_a_tab_to_its_sourced_dimensions() {
        // Alpha & Omega PO-00009 (TO-252AA), checked against onsemi 369C, Vishay 95627, Diodes.
        for id in [
            "Package_TO_SOT_SMD:TO-252-2",
            "Package_TO_SOT_SMD:TO-252-3_TabPin2",
            "Package_TO_SOT_SMD:TO-252-3_TabPin4",
        ] {
            let fp = made(id);
            let shape = fits(id, &fp, 2.3);
            assert!(shape.shaped, "{id} is not a box");
            // Two leads, the tab beyond the body and the exposed tab under it; pin 1's dot.
            assert_eq!(count(&shape, Material::Metal), 4, "{id}");
            assert_eq!(count(&shape, Material::Mark), 1, "{id}");
            // The body is 6.096 mm along the leads and the footprint's 6.5 mm across.
            let body = body_solid(&shape);
            assert!((body.max[0] - 3.048).abs() < EPS && (body.min[0] + 3.048).abs() < EPS);
            assert!((body.max[1] - 3.25).abs() < EPS && (body.min[1] + 3.25).abs() < EPS);
            let metal = metal_of(&shape);
            let (left, right): (Vec<&Bounds>, Vec<&Bounds>) =
                metal.iter().partition(|b| b.max[0] < 0.0);
            assert_eq!((left.len(), right.len()), (2, 2), "{id}");
            // Leads 0.762 mm wide at 2.286 mm from the centre, reaching 2.921 mm beyond the body.
            for (b, at) in left.iter().zip([-2.286, 2.286]) {
                assert!((centre(b) - at).abs() < EPS, "{id}: {b:?}");
                assert!((width(b) - 0.762).abs() < EPS, "{id}: {b:?}");
                assert!((b.min[0] + 3.048 + 2.921).abs() < EPS, "{id}: {b:?}");
                assert!((b.max[0] + 3.048).abs() < EPS, "{id}: {b:?}");
            }
            // The tab: 5.340 mm wide, 1.016 mm beyond the body; the exposed tab 4.826 mm wide,
            // 5.249 mm in under the body from its edge.
            let (wide, narrow) = (&right[0], &right[1]);
            assert!((width(wide) - 5.340).abs() < EPS, "{id}: {wide:?}");
            assert!((wide.max[0] - 3.048 - 1.016).abs() < EPS && (wide.min[0] - 3.048).abs() < EPS);
            assert!((width(narrow) - 4.826).abs() < EPS, "{id}: {narrow:?}");
            assert!(
                (narrow.min[0] - (3.048 - 5.249)).abs() < EPS,
                "{id}: {narrow:?}"
            );
            assert!((narrow.max[0] - 3.048).abs() < EPS, "{id}: {narrow:?}");
            // Overall, tab tip to lead tips, is the sourced 10.033 mm.
            assert!((wide.max[0] - left[0].min[0] - 10.033).abs() < EPS, "{id}");
            assert!(metal.iter().all(|b| b.min[2].abs() < EPS));
            // Pin 1's dot is on the leads' side.
            let mark = mark_of(&shape);
            assert!(mark.max[0] < 0.0 && mark.max[1] < 0.0, "{id}: {mark:?}");
        }
    }

    #[test]
    fn a_d2pak_has_two_leads_and_a_tab_to_its_sourced_dimensions() {
        // Alpha & Omega PO-00005 (TO-263AB), checked against onsemi 418AJ, Vishay, Diodes.
        for id in [
            "Package_TO_SOT_SMD:TO-263-2",
            "Package_TO_SOT_SMD:TO-263-3_TabPin2",
            "Package_TO_SOT_SMD:TO-263-3_TabPin4",
        ] {
            let fp = made(id);
            let shape = fits(id, &fp, 4.5);
            assert!(shape.shaped, "{id} is not a box");
            // Two leads and the tab (one piece: beyond the body and under it as wide).
            assert_eq!(count(&shape, Material::Metal), 3, "{id}");
            assert_eq!(count(&shape, Material::Mark), 1, "{id}");
            let body = body_solid(&shape);
            assert!((body.max[0] - 4.57).abs() < EPS && (body.min[0] + 4.57).abs() < EPS);
            assert!((body.max[1] - 5.0).abs() < EPS && (body.min[1] + 5.0).abs() < EPS);
            let metal = metal_of(&shape);
            let (left, right): (Vec<&Bounds>, Vec<&Bounds>) =
                metal.iter().partition(|b| b.max[0] < 0.0);
            assert_eq!((left.len(), right.len()), (2, 1), "{id}");
            // Leads 0.81 mm wide at 2.54 mm from the centre, reaching H - D - L1 = 4.83 mm.
            for (b, at) in left.iter().zip([-2.54, 2.54]) {
                assert!((centre(b) - at).abs() < EPS, "{id}: {b:?}");
                assert!((width(b) - 0.81).abs() < EPS, "{id}: {b:?}");
                assert!((b.min[0] + 4.57 + 4.83).abs() < EPS, "{id}: {b:?}");
            }
            // The tab: 8.00 mm wide, 1.27 mm beyond the body and 6.90 mm in under it.
            let tab = &right[0];
            assert!(
                (width(tab) - 8.0).abs() < EPS && centre(tab).abs() < EPS,
                "{id}"
            );
            assert!((tab.max[0] - 4.57 - 1.27).abs() < EPS, "{id}: {tab:?}");
            assert!((tab.min[0] - (4.57 - 6.90)).abs() < EPS, "{id}: {tab:?}");
            // Overall, tab tip to lead tips, is the sourced 15.24 mm.
            assert!((tab.max[0] - left[0].min[0] - 15.24).abs() < EPS, "{id}");
            assert!(metal.iter().all(|b| b.min[2].abs() < EPS));
            let mark = mark_of(&shape);
            assert!(mark.max[0] < 0.0 && mark.max[1] < 0.0, "{id}: {mark:?}");
        }
    }

    #[test]
    fn a_dpak_or_d2pak_with_more_leads_is_still_the_box() {
        for id in [
            "Package_TO_SOT_SMD:TO-252-4",
            "Package_TO_SOT_SMD:TO-252-5_TabPin3",
            "Package_TO_SOT_SMD:TO-252-5_TabPin6",
            "Package_TO_SOT_SMD:TO-263-4",
            "Package_TO_SOT_SMD:TO-263-5_TabPin3",
            "Package_TO_SOT_SMD:TO-263-5_TabPin6",
            "Package_TO_SOT_SMD:TO-263-6",
            "Package_TO_SOT_SMD:TO-263-7_TabPin4",
            "Package_TO_SOT_SMD:TO-263-7_TabPin8",
            "Package_TO_SOT_SMD:TO-263-9_TabPin5",
            "Package_TO_SOT_SMD:TO-263-9_TabPin10",
        ] {
            let fp = made(id);
            assert!(fp.package.is_none(), "{id}");
            assert!(!package_shape(&fp, Some(2.0)).shaped, "{id}");
        }
    }

    #[test]
    fn a_sot_89_5_is_still_the_box() {
        let fp = made("Package_TO_SOT_SMD:SOT-89-5");
        assert!(fp.package.is_none());
        assert!(!package_shape(&fp, Some(1.5)).shaped);
    }

    #[test]
    fn a_qfp_has_leads_on_four_edges_and_its_exposed_pad() {
        let id = "Package_QFP:LQFP-48_7x7mm_P0.5mm";
        let shape = fits(id, &made(id), 1.6);
        assert_eq!(count(&shape, Material::Metal), 48);
        let all = shape.bounds().unwrap();
        // The span is the body plus 2.0 mm (MS-026).
        for i in 0..2 {
            assert!((all.max[i] - 4.5).abs() < EPS && (all.min[i] + 4.5).abs() < EPS);
        }
        let fp =
            spec("gen:QFP(pins=32, body=5mm, pitch=0.5mm, lead_width=0.17-0.27mm, ep=3.5x3.5mm)");
        let shape = fits("gen:QFP", &fp, 1.2);
        assert_eq!(count(&shape, Material::Metal), 33, "32 leads and the pad");
    }

    #[test]
    fn a_qfn_has_its_terminals_and_exposed_pad_under_the_body() {
        let fp = spec(
            "gen:QFN(pins=16, pitch=0.5mm, body=3x3mm, ep=1.7x1.7mm, lead_width=0.18-0.3mm, lead_length=0.3-0.5mm)",
        );
        let shape = fits("gen:QFN", &fp, 0.9);
        assert_eq!(count(&shape, Material::Metal), 17);
        let metal: Vec<Bounds> = shape
            .solids
            .iter()
            .filter(|s| s.material == Material::Metal)
            .map(Solid::bounds)
            .collect();
        let ep = metal.last().unwrap();
        assert!((ep.max[0] - 0.85).abs() < EPS && (ep.max[1] - 0.85).abs() < EPS);
        for b in &metal {
            assert!(b.min[2].abs() < EPS && b.max[2] <= 0.9 / 4.0 + EPS, "{b:?}");
        }
        // A terminal is 0.4 mm long under the body's edge and 0.24 mm wide.
        let t = metal[0];
        assert!(
            (t.min[0] + 1.5 + NO_LEAD_TERMINAL_EXPOSURE).abs() < EPS,
            "{t:?}"
        );
        assert!((t.max[0] + 1.1).abs() < EPS, "{t:?}");
        assert!((t.max[1] - t.min[1] - 0.24).abs() < EPS, "{t:?}");
        let body = body_solid(&shape);
        assert!(body.min[2] > 0.0, "the metal shows below the plastic");

        let fp = spec(
            "gen:DFN(pins=8, pitch=0.5mm, body=2x2mm, ep=0.9x1.6mm, lead_width=0.2-0.3mm, lead_length=0.25-0.45mm)",
        );
        let shape = fits("gen:DFN", &fp, 0.75);
        assert_eq!(count(&shape, Material::Metal), 9);
    }

    #[test]
    fn a_bga_has_a_ball_under_every_pad_and_its_body_above_them() {
        let id = "Package_BGA:BGA-100_11.0x11.0mm_Layout10x10_P1.0mm_Ball0.5mm_Pad0.4mm_NSMD";
        let shape = fits(id, &made(id), 1.4);
        assert_eq!(count(&shape, Material::Ball), 100);
        let ball = shape
            .solids
            .iter()
            .find(|s| s.material == Material::Ball)
            .unwrap()
            .bounds();
        assert!((ball.max[0] - ball.min[0] - 0.5).abs() < EPS, "{ball:?}");
        assert!((ball.min[0] + 4.75).abs() < EPS, "A1 is at (-4.5, -4.5)");
        let body = body_solid(&shape);
        assert!((body.min[2] - ball.max[2]).abs() < EPS);
    }

    #[test]
    fn a_pin_header_ends_its_pins_at_the_height_and_a_socket_is_its_body() {
        let id = "Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical";
        let shape = fits(id, &made(id), 8.5);
        assert_eq!(count(&shape, Material::Metal), 10);
        let body = body_solid(&shape);
        assert!((body.max[2] - 2.5).abs() < EPS, "{body:?}");
        let pin = shape
            .solids
            .iter()
            .find(|s| s.material == Material::Metal)
            .unwrap()
            .bounds();
        assert!((pin.max[0] - pin.min[0] - 0.64).abs() < EPS && (pin.max[2] - 8.5).abs() < EPS);

        let fp = spec("gen:PinSocket(rows=1, pins=4, pitch=2mm)");
        let shape = fits("gen:PinSocket", &fp, 4.3);
        assert_eq!(count(&shape, Material::Mark), 4);
        assert!((body_solid(&shape).max[2] - 4.3).abs() < EPS);
        // A header lower than its plastic: everything ends at the height.
        let shape = fits(id, &made(id), 1.5);
        assert!((body_solid(&shape).max[2] - 1.5).abs() < EPS);
    }

    #[test]
    fn a_pin_headers_plastic_is_its_familys_sourced_height() {
        // The makers' drawings that `data/jlcpcb/README.md` names: 2.50 mm at 2.54 mm pitch and
        // 2.00 mm at 2.0 mm, one row or two.
        for (id, plastic) in [
            (
                "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Vertical",
                2.5,
            ),
            (
                "Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical",
                2.5,
            ),
            (
                "Connector_PinHeader_2.00mm:PinHeader_1x04_P2.00mm_Vertical",
                2.0,
            ),
            (
                "Connector_PinHeader_2.00mm:PinHeader_2x05_P2.00mm_Vertical",
                2.0,
            ),
        ] {
            let fp = made(id);
            match &fp.package {
                Some(Package::Header {
                    plastic: p,
                    socket: false,
                    ..
                }) => assert_eq!(*p, plastic, "{id}"),
                other => panic!("{id}: {other:?}"),
            }
            // A catalog row's height is the pin's tip (8.9 mm for the 2.54 mm headers): the
            // plastic stays at its own height and the pins reach the part's.
            let shape = fits(id, &fp, 8.9);
            assert!((body_solid(&shape).max[2] - plastic).abs() < EPS, "{id}");
            assert!((shape.bounds().unwrap().max[2] - 8.9).abs() < EPS, "{id}");
        }
        let fp = spec("gen:PinHeader(rows=2, pins=3, pitch=2mm)");
        assert!(matches!(fp.package, Some(Package::Header { plastic, .. }) if plastic == 2.0));
    }

    #[test]
    fn a_crystal_is_a_ceramic_base_with_a_lid() {
        let id = "Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm";
        let shape = fits(id, &made(id), 0.8);
        assert_eq!(count(&shape, Material::Metal), 1);
        assert!((body_solid(&shape).max[2] - (0.8 - CRYSTAL_LID_THICKNESS)).abs() < EPS);
    }

    #[test]
    fn an_unknown_height_is_the_outline_only() {
        for id in [
            "Resistor_SMD:R_0402_1005Metric",
            "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
            "Package_TO_SOT_SMD:SOT-223",
        ] {
            let fp = made(id);
            for h in [None, Some(0.0), Some(-1.0), Some(f64::NAN)] {
                let shape = package_shape(&fp, h);
                assert!(shape.solids.is_empty(), "{id} {h:?}");
                assert_eq!(shape.outline[0], fp.body.min);
                assert_eq!(shape.outline[2], fp.body.max);
            }
        }
    }

    #[test]
    fn a_footprint_no_generator_shapes_is_its_body_box() {
        let fp = made("Package_TO_SOT_SMD:SC-70-8");
        assert!(fp.package.is_none());
        let shape = package_shape(&fp, Some(1.8));
        assert!(!shape.shaped);
        let b = shape.bounds().unwrap();
        assert_eq!(
            (b.min, b.max),
            (
                [fp.body.min.x, fp.body.min.y, 0.0],
                [fp.body.max.x, fp.body.max.y, 1.8]
            )
        );
        let read = Footprint::parse("X", &fp.source.to_kicad_string()).unwrap();
        assert!(read.package.is_none(), "a footprint read from a file");
    }

    #[test]
    fn every_generated_footprint_fits_and_is_the_same_twice() {
        let mut shaped = 0;
        for id in crate::generate::names() {
            let fp = made(id);
            let first = package_shape(&fp, Some(1.0));
            assert_eq!(first, package_shape(&made(id), Some(1.0)), "{id}");
            let pins = fp.pad_numbers().iter().filter(|n| !n.is_empty()).count();
            match &fp.package {
                Some(Package::GullWing { leads, exposed, .. })
                | Some(Package::NoLead { leads, exposed, .. }) => {
                    // A tab numbered like a pin (SOT-223-3_TabPin2) is a lead of its own.
                    // So is pin 2 of a SOT-89: its narrow end among the pins and its tab are two leads.
                    let tab_as_pin = usize::from(
                        id.ends_with("_TabPin2") || id.starts_with("Package_TO_SOT_SMD:SOT-89-3"),
                    );
                    let on = |e: Edge| leads.iter().filter(|l| l.edge == e).count();
                    if id.contains(":TO-252-") || id.contains(":TO-263-") {
                        // A DPAK or D2PAK has its two outer leads on the left (the centre lead is
                        // a cut stub, not drawn) and its tab, in one or two parts, on the right.
                        assert_eq!(on(Edge::Left), 2, "{id}");
                        assert!((1..=2).contains(&on(Edge::Right)), "{id}");
                        assert!(exposed.is_none(), "{id}");
                    } else {
                        assert_eq!(
                            leads.len() + usize::from(exposed.is_some()),
                            pins + tab_as_pin,
                            "{id}"
                        );
                    }
                    assert_eq!(on(Edge::Top), on(Edge::Bottom), "{id}");
                    // SOT-223 and SOT-89 have three leads on one side and the tab on the other (a DPAK's
                    // tab is one or two parts).
                    let uneven = if id.contains("SOT-223") || id.contains("SOT-89-3") {
                        2
                    } else {
                        1
                    };
                    assert!(on(Edge::Left).abs_diff(on(Edge::Right)) <= uneven, "{id}");
                }
                Some(Package::Bga { balls, .. }) => assert_eq!(balls.len(), pins, "{id}"),
                Some(Package::Header { pins: p, .. }) => assert_eq!(p.len(), pins, "{id}"),
                _ => {}
            }
            if fp.package.is_some() {
                fits(id, &fp, 1.0);
                fits(id, &fp, 0.3);
                shaped += 1;
            }
        }
        assert!(shaped > 400, "{shaped} shaped");
    }

    #[test]
    fn the_package_does_not_change_the_written_footprint() {
        let fp = made("Package_SO:SOIC-8_3.9x4.9mm_P1.27mm");
        let read = Footprint::parse(
            "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
            &fp.source.to_kicad_string(),
        )
        .unwrap();
        assert_eq!(
            Footprint {
                package: None,
                ..fp
            },
            read
        );
    }
}
