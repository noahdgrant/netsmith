//! A footprint taken from an Altium library (`.PcbLib`) or board (`.PcbDoc`) into a project's
//! library as a `.kicad_mod` (issue #1431, `netsmith import footprint`).
//!
//! Both files are compound (OLE) files ([`cfb`]). A library holds a storage for each footprint,
//! whose `Data` stream is the footprint's name and then its primitives; a board holds a stream
//! of each kind of primitive (`Pads6/Data`, `Tracks6/Data`, ...), each primitive naming its
//! component by its index in `Components6/Data` ([`records`]).
//!
//! What is taken, as the KiCad inputs take it:
//!
//! - the pads: the number, the shape (round, rectangle, round rectangle with its corner radius,
//!   an octagon as a rectangle with its corners cut), the size, the angle, the hole (round, or a
//!   slot) and whether it is plated, and the layers: a pad on the top or bottom layer is a
//!   surface pad there, with its paste and mask, one on the multi-layer a through-hole pad. The
//!   pad's sizes on the top layer are its sizes everywhere, as a simple pad's are; a pad's own
//!   mask and paste expansion is left to the board's rules.
//! - the tracks, arcs, fills and regions of the overlay layers (silkscreen), of the paste and
//!   mask layers and of the copper, and of two mechanical layers by the convention of Altium's
//!   IPC footprint wizard: mechanical 15 the courtyard and mechanical 13 the fabrication
//!   (assembly) drawing. A primitive on another layer (keep-out, another mechanical layer) is
//!   left out, and the notes say how many were.
//! - not the texts (the designator and comment strings), the 3D bodies or the vias: the
//!   reference becomes `REF**` and the value the footprint's name, as KiCad's libraries have them.
//!
//! A part of a board is taken off its place as a KiCad board's is: its origin and angle (the
//! component's `X`, `Y` and `ROTATION`) are taken off every point and pad, and a part on the
//! bottom face is mirrored back (x negated in the part's frame, the layers on the other face),
//! so the file is the footprint seen from the top. Altium's y is up and KiCad's down.
//!
//! The reader was written from the format as documented by the open readers of it, without a
//! file saved by Altium to check it against: its tests are footprints written record by record.

mod cfb;
mod records;

#[cfg(feature = "test-support")]
pub mod fixture;

use crate::sexp::Sexp;

use crate::footprint_import::{FootprintSource, ImportedFootprint};
use records::{Arc, Component, Cursor, Fill, Hole, NO_COMPONENT, Pad, PadShape, Point, Primitive};

/// The KiCad file version the `.kicad_mod` is written in (KiCad 9).
const VERSION: &str = "20241229";

/// Whether `bytes` are a compound file (and so the binary form of an Altium file).
pub fn is_compound_file(bytes: &[u8]) -> bool {
    bytes.starts_with(&cfb::SIGNATURE)
}

/// The footprint of an Altium file (`source` says whether it is a library, whose footprint
/// `footprint` names, or its one footprint when `None` (a library of several is refused with their
/// names, issue 1438), or a board, whose part is named), named `name` or the file's own name.
pub fn import_altium_footprint(
    bytes: &[u8],
    source: FootprintSource,
    footprint: Option<&str>,
    name: Option<&str>,
) -> Result<ImportedFootprint, String> {
    if !is_compound_file(bytes) {
        if bytes.starts_with(b"|RECORD=") || bytes.windows(8).take(64).any(|w| w == b"|RECORD=") {
            return Err(
                "an Altium file saved as ASCII: save it in Altium as binary to import it".into(),
            );
        }
        return Err("not an Altium library or board: not a compound (OLE) file".into());
    }
    let file = cfb::Compound::parse(bytes)?;
    let (source_name, primitives, place) = match source {
        FootprintSource::File => {
            let (name, primitives) = library_footprint(&file, footprint)?;
            (name, primitives, None)
        }
        FootprintSource::Board { reference } => {
            let (component, primitives) = board_part(&file, reference)?;
            (component.pattern.clone(), primitives, Some(component))
        }
    };
    let name = name.map_or_else(|| source_name.clone(), str::to_string);
    if name.is_empty() {
        return Err("the footprint has no name: give one with `--name`".into());
    }
    let place = Place::new(place.as_ref());
    let (text, notes) = kicad_mod(&name, &primitives, &place);
    Ok(ImportedFootprint {
        source_name,
        name,
        from_bottom: place.bottom,
        board_angle: place.angle,
        text,
        notes,
    })
}

/// Storages of a library that are not footprints.
const LIBRARY_STORAGES: &[&str] = &["Library", "FileVersionInfo"];

/// The footprint of a library `asked` names, or its one footprint when none is asked: its name
/// and primitives.
fn library_footprint(
    file: &cfb::Compound<'_>,
    asked: Option<&str>,
) -> Result<(String, Vec<Primitive>), String> {
    let mut found = Vec::new();
    for (storage, _, kind) in file.children(0) {
        if kind != cfb::Kind::Storage
            || LIBRARY_STORAGES
                .iter()
                .any(|s| s.eq_ignore_ascii_case(&storage))
        {
            continue;
        }
        let Some(data) = file.read(&[&storage, "Data"])? else {
            continue;
        };
        found.push((storage, data));
    }
    let mut read = Vec::new();
    for (storage, data) in &found {
        let mut c = Cursor::new(data);
        c.begin()
            .map_err(|e| format!("footprint `{storage}`: {e}"))?;
        let name = c
            .short_string()
            .map_err(|e| format!("footprint `{storage}`: {e}"))?;
        c.finish();
        read.push((name, c));
    }
    let mut names: Vec<String> = read.iter().map(|(n, _)| n.clone()).collect();
    names.sort_unstable();
    let picked = match (asked, read.as_mut_slice()) {
        (_, []) => return Err("the library holds no footprint".into()),
        (Some(asked), all) => all.iter_mut().find(|(n, _)| n == asked).ok_or_else(|| {
            format!(
                "the library has no footprint `{asked}`; its footprints are {}",
                names.join(", ")
            )
        })?,
        (None, [one]) => one,
        (None, many) => {
            return Err(format!(
                "the library holds {} footprints ({}): name the one to import with `--footprint NAME`",
                many.len(),
                names.join(", ")
            ));
        }
    };
    let (name, c) = picked;
    let primitives =
        records::primitives(c, false).map_err(|e| format!("footprint `{name}`: {e}"))?;
    Ok((name.clone(), primitives))
}

/// The streams of a board that hold primitives, and whether a stream's regions are shape-based.
const BOARD_STREAMS: &[(&str, bool)] = &[
    ("Pads6", false),
    ("Tracks6", false),
    ("Arcs6", false),
    ("Fills6", false),
    ("Regions6", false),
    ("ShapeBasedRegions6", true),
    ("Texts6", false),
    ("ComponentBodies6", false),
];

/// The component of a board whose designator is `reference`, and its primitives.
fn board_part(
    file: &cfb::Compound<'_>,
    reference: &str,
) -> Result<(Component, Vec<Primitive>), String> {
    let Some(raw) = file.read(&["Components6", "Data"])? else {
        return Err("not an Altium board: it has no `Components6` stream".into());
    };
    let components = records::components(&raw)?;
    let found: Vec<usize> = components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.designator == reference)
        .map(|(i, _)| i)
        .collect();
    let index = match found.as_slice() {
        [one] => *one,
        [] => {
            let mut refs: Vec<&str> = components
                .iter()
                .map(|c| c.designator.as_str())
                .filter(|d| !d.is_empty())
                .collect();
            refs.sort_by_key(|r| crate::footprint_import::natural(r));
            refs.dedup();
            if refs.is_empty() {
                return Err("the board has no components".into());
            }
            let shown = if refs.len() > 40 {
                format!("{}, ... ({} in all)", refs[..40].join(", "), refs.len())
            } else {
                refs.join(", ")
            };
            return Err(format!(
                "the board has no part `{reference}`; its parts are {shown}"
            ));
        }
        many => {
            return Err(format!(
                "the board has {} parts `{reference}`; a reference must name one",
                many.len()
            ));
        }
    };
    let mut primitives = Vec::new();
    for (stream, shape_based) in BOARD_STREAMS {
        let Some(raw) = file.read(&[stream, "Data"])? else {
            continue;
        };
        let mut c = Cursor::new(&raw);
        let all =
            records::primitives(&mut c, *shape_based).map_err(|e| format!("{stream}: {e}"))?;
        primitives.extend(
            all.into_iter()
                .filter(|p| component_of(p) == Some(index as u16)),
        );
    }
    let component = components[index].clone();
    if component.pattern.is_empty() {
        return Err(format!(
            "part `{reference}` names no footprint (its `PATTERN`): give one with `--name`"
        ));
    }
    Ok((component, primitives))
}

fn component_of(p: &Primitive) -> Option<u16> {
    let c = match p {
        Primitive::Pad(p) => p.component,
        Primitive::Track(t) => t.component,
        Primitive::Arc(a) => a.component,
        Primitive::Fill(f) => f.component,
        Primitive::Region(r) => r.component,
        Primitive::Other(_, c) => *c,
    };
    (c != NO_COMPONENT).then_some(c)
}

/// A component's place on its board, taken off its primitives (none for a library footprint).
struct Place {
    origin: Point,
    angle: f64,
    bottom: bool,
}

impl Place {
    fn new(component: Option<&Component>) -> Self {
        match component {
            Some(c) => Place {
                origin: c.at,
                angle: c.angle,
                bottom: c.bottom,
            },
            None => Place {
                origin: Point { x: 0.0, y: 0.0 },
                angle: 0.0,
                bottom: false,
            },
        }
    }

    /// A point of the board as KiCad's footprint frame has it: the place taken off, the mirror
    /// undone, y down.
    fn point(&self, p: Point) -> (f64, f64) {
        let (dx, dy) = (p.x - self.origin.x, p.y - self.origin.y);
        let (s, c) = (-self.angle).to_radians().sin_cos();
        let (mut x, y) = (dx * c - dy * s, dx * s + dy * c);
        if self.bottom {
            x = -x;
        }
        (round(x), round(-y))
    }

    /// A pad's angle in the footprint's frame (Altium's and KiCad's both count it
    /// counter-clockwise as seen from the top).
    fn angle(&self, board: f64) -> f64 {
        let a = board - self.angle;
        let a = if self.bottom { -a } else { a };
        let a = round(a).rem_euclid(360.0);
        if (a - 360.0).abs() < 1e-9 { 0.0 } else { a }
    }

    /// A layer of the board seen from the part's own top.
    fn layer(&self, layer: u8) -> u8 {
        if !self.bottom {
            return layer;
        }
        match layer {
            TOP => BOTTOM,
            BOTTOM => TOP,
            33 => 34,
            34 => 33,
            35 => 36,
            36 => 35,
            37 => 38,
            38 => 37,
            other => other,
        }
    }
}

/// A length to the nearest nanometre, so that a unit's 2.54 nm leaves no long tail.
fn round(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

const TOP: u8 = 1;
const BOTTOM: u8 = 32;
const MULTI: u8 = 74;
/// Mechanical layer `n` is layer byte `56 + n`.
const MECHANICAL_0: u8 = 56;
/// The mechanical layers taken, by the convention of Altium's IPC footprint wizard.
const COURTYARD_MECHANICAL: u8 = 15;
const FAB_MECHANICAL: u8 = 13;

/// The KiCad layer of a graphic on the Altium layer `layer` (`name`, a region's `V7_LAYER`, when
/// it has one), or `None` for a layer that is not taken.
fn graphic_layer(layer: u8, name: Option<&str>) -> Option<&'static str> {
    let mechanical = match name.map(str::to_ascii_uppercase) {
        Some(n) if n.starts_with("MECHANICAL") => n["MECHANICAL".len()..].parse::<u8>().ok(),
        _ => layer
            .checked_sub(MECHANICAL_0)
            .filter(|n| (1..=16).contains(n)),
    };
    if let Some(n) = mechanical {
        return match n {
            COURTYARD_MECHANICAL => Some("F.CrtYd"),
            FAB_MECHANICAL => Some("F.Fab"),
            _ => None,
        };
    }
    Some(match layer {
        TOP => "F.Cu",
        BOTTOM => "B.Cu",
        33 => "F.SilkS",
        34 => "B.SilkS",
        35 => "F.Paste",
        36 => "B.Paste",
        37 => "F.Mask",
        38 => "B.Mask",
        _ => return None,
    })
}

/// The width a filled area or a courtyard outline is drawn with.
fn outline_width(layer: &str) -> f64 {
    match layer {
        "F.CrtYd" => 0.05,
        "F.Fab" => 0.1,
        _ => 0.0,
    }
}

fn xy(head: &str, (x, y): (f64, f64)) -> Sexp {
    Sexp::node(head, vec![Sexp::num(x), Sexp::num(y)])
}

fn stroke(width: f64) -> Sexp {
    Sexp::node(
        "stroke",
        vec![
            Sexp::node("width", vec![Sexp::num(width)]),
            Sexp::node("type", vec![Sexp::atom("solid")]),
        ],
    )
}

fn layer_node(layer: &str) -> Sexp {
    Sexp::node("layer", vec![Sexp::string(layer)])
}

/// A polygon: filled on copper, paste, mask and silkscreen, an outline on the courtyard and the
/// fabrication layer.
fn poly(points: &[(f64, f64)], layer: &str) -> Sexp {
    let width = outline_width(layer);
    let fill = if width > 0.0 { "no" } else { "solid" };
    Sexp::node(
        "fp_poly",
        vec![
            Sexp::node("pts", points.iter().map(|&p| xy("xy", p)).collect()),
            stroke(width),
            Sexp::node("fill", vec![Sexp::atom(fill)]),
            layer_node(layer),
        ],
    )
}

fn arc_node(arc: &Arc, place: &Place, layer: &str) -> Sexp {
    let at = |deg: f64| {
        let (s, c) = deg.to_radians().sin_cos();
        place.point(Point {
            x: arc.center.x + arc.radius * c,
            y: arc.center.y + arc.radius * s,
        })
    };
    let sweep = (arc.end - arc.start).rem_euclid(360.0);
    if sweep < 1e-6 {
        let center = place.point(arc.center);
        return Sexp::node(
            "fp_circle",
            vec![
                xy("center", center),
                xy("end", (round(center.0 + arc.radius), center.1)),
                stroke(arc.width),
                Sexp::node("fill", vec![Sexp::atom("no")]),
                layer_node(layer),
            ],
        );
    }
    Sexp::node(
        "fp_arc",
        vec![
            xy("start", at(arc.start)),
            xy("mid", at(arc.start + sweep / 2.0)),
            xy("end", at(arc.start + sweep)),
            stroke(arc.width),
            layer_node(layer),
        ],
    )
}

fn fill_points(fill: &Fill, place: &Place) -> Vec<(f64, f64)> {
    let cx = (fill.corner1.x + fill.corner2.x) / 2.0;
    let cy = (fill.corner1.y + fill.corner2.y) / 2.0;
    let hx = (fill.corner2.x - fill.corner1.x).abs() / 2.0;
    let hy = (fill.corner2.y - fill.corner1.y).abs() / 2.0;
    let (s, c) = fill.angle.to_radians().sin_cos();
    [(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy)]
        .into_iter()
        .map(|(x, y)| {
            place.point(Point {
                x: cx + x * c - y * s,
                y: cy + x * s + y * c,
            })
        })
        .collect()
}

/// A pad as KiCad writes it; the notes say what of it was changed.
fn pad_node(pad: &Pad, place: &Place, notes: &mut Notes) -> Sexp {
    let layer = place.layer(pad.layer);
    let through = layer == MULTI || pad.hole > 0.0;
    let (w, h) = (round(pad.size.x), round(pad.size.y));
    let kind = match (through, pad.plated) {
        (false, _) => "smd",
        (true, true) => "thru_hole",
        (true, false) => "np_thru_hole",
    };
    let shape = match pad.shape {
        PadShape::Round if (w - h).abs() < 1e-9 => "circle",
        PadShape::Round => "oval",
        PadShape::Rect => "rect",
        PadShape::Octagon | PadShape::RoundRect { .. } => "roundrect",
    };
    let (x, y) = place.point(pad.at);
    let mut at = vec![Sexp::num(x), Sexp::num(y)];
    let angle = place.angle(pad.angle);
    if angle != 0.0 {
        at.push(Sexp::num(angle));
    }
    let mut items = vec![
        Sexp::string(pad.name.clone()),
        Sexp::atom(kind),
        Sexp::atom(shape),
        Sexp::node("at", at),
        Sexp::node("size", vec![Sexp::num(w), Sexp::num(h)]),
    ];
    if through {
        let d = round(pad.hole);
        let drill = match pad.hole_shape {
            Hole::Slot { length, angle } => {
                let length = round(length);
                // A slot along the pad's y is drawn as KiCad's oval drill turned with the pad.
                let across = (angle.rem_euclid(180.0) - 90.0).abs() < 45.0;
                let (dx, dy) = if across { (d, length) } else { (length, d) };
                if angle.rem_euclid(90.0).abs() > 1e-6 {
                    notes.slot_angles += 1;
                }
                vec![Sexp::atom("oval"), Sexp::num(dx), Sexp::num(dy)]
            }
            Hole::Square => {
                notes.square_holes += 1;
                vec![Sexp::num(d)]
            }
            Hole::Round => vec![Sexp::num(d)],
        };
        items.push(Sexp::node("drill", drill));
    }
    let layers: &[&str] = if through {
        &["*.Cu", "*.Mask"]
    } else if layer == BOTTOM {
        &["B.Cu", "B.Paste", "B.Mask"]
    } else {
        &["F.Cu", "F.Paste", "F.Mask"]
    };
    items.push(Sexp::node(
        "layers",
        layers.iter().map(|l| Sexp::string(*l)).collect(),
    ));
    match pad.shape {
        PadShape::RoundRect { percent } => {
            // Altium's radius is a percent of half the short side; KiCad's ratio is of the side.
            let ratio = f64::from(percent.min(100)) / 200.0;
            items.push(Sexp::node("roundrect_rratio", vec![Sexp::num(ratio)]));
        }
        PadShape::Octagon => {
            items.push(Sexp::node("roundrect_rratio", vec![Sexp::num(0.0)]));
            items.push(Sexp::node("chamfer_ratio", vec![Sexp::num(0.25)]));
            items.push(Sexp::node(
                "chamfer",
                ["top_left", "top_right", "bottom_left", "bottom_right"]
                    .into_iter()
                    .map(Sexp::atom)
                    .collect(),
            ));
        }
        _ => {}
    }
    Sexp::node("pad", items)
}

/// What the import left out or changed, for the command to print.
#[derive(Default)]
struct Notes {
    /// Graphics on layers that are not taken.
    other_layers: usize,
    texts: usize,
    bodies: usize,
    square_holes: usize,
    slot_angles: usize,
    arced_regions: usize,
    no_courtyard: bool,
}

impl Notes {
    fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut count = |n: usize, what: &str| {
            if n > 0 {
                out.push(format!("{n} {what}"));
            }
        };
        count(
            self.other_layers,
            "graphic(s) on a layer not taken (keep-out, or a mechanical layer other than 13 and 15) left out",
        );
        count(
            self.texts,
            "text(s) left out: the reference is `REF**` and the value the footprint's name",
        );
        count(self.bodies, "3D body(ies) left out");
        count(
            self.square_holes,
            "square hole(s) written as round holes of their width",
        );
        count(
            self.slot_angles,
            "slot(s) at an angle that is not a multiple of 90 degrees written along the nearer axis",
        );
        count(
            self.arced_regions,
            "region(s) with arcs written with straight edges between the arcs' ends",
        );
        if self.no_courtyard {
            out.push(
                "no courtyard: nothing is drawn on mechanical layer 15; draw one on F.CrtYd".into(),
            );
        }
        out
    }
}

/// The `.kicad_mod` text of the footprint `name` drawn by `primitives`, and the notes.
fn kicad_mod(name: &str, primitives: &[Primitive], place: &Place) -> (String, Vec<String>) {
    let mut notes = Notes::default();
    let mut graphics = Vec::new();
    let mut pads = Vec::new();
    let mut ys = Vec::new();
    let mut courtyard = false;
    for p in primitives {
        let mut keep = |layer: Option<&'static str>, node: Sexp, ys_of: &[(f64, f64)]| match layer {
            Some(l) => {
                courtyard |= l == "F.CrtYd";
                ys.extend(ys_of.iter().map(|p| p.1));
                graphics.push(node);
            }
            None => notes.other_layers += 1,
        };
        match p {
            Primitive::Pad(pad) => {
                let (_, y) = place.point(pad.at);
                ys.push(y - pad.size.y.max(pad.size.x) / 2.0);
                ys.push(y + pad.size.y.max(pad.size.x) / 2.0);
                pads.push(pad_node(pad, place, &mut notes));
            }
            Primitive::Track(t) => {
                let layer = graphic_layer(place.layer(t.layer), None);
                let (s, e) = (place.point(t.start), place.point(t.end));
                let node = Sexp::node(
                    "fp_line",
                    vec![
                        xy("start", s),
                        xy("end", e),
                        stroke(round(t.width)),
                        layer_node(layer.unwrap_or("")),
                    ],
                );
                keep(layer, node, &[s, e]);
            }
            Primitive::Arc(a) => {
                let layer = graphic_layer(place.layer(a.layer), None);
                let c = place.point(a.center);
                let r = a.radius;
                let node = arc_node(a, place, layer.unwrap_or(""));
                keep(layer, node, &[(c.0, c.1 - r), (c.0, c.1 + r)]);
            }
            Primitive::Fill(f) => {
                let layer = graphic_layer(place.layer(f.layer), None);
                let pts = fill_points(f, place);
                let node = poly(&pts, layer.unwrap_or(""));
                keep(layer, node, &pts);
            }
            Primitive::Region(r) => {
                // A cutout or a cavity is not something the footprint draws.
                if r.kind != 0 || r.outline.len() < 3 {
                    notes.other_layers += 1;
                    continue;
                }
                let layer = graphic_layer(place.layer(r.layer), r.layer_name.as_deref());
                let pts: Vec<(f64, f64)> = r.outline.iter().map(|&p| place.point(p)).collect();
                if r.had_arcs && layer.is_some() {
                    notes.arced_regions += 1;
                }
                let node = poly(&pts, layer.unwrap_or(""));
                keep(layer, node, &pts);
            }
            Primitive::Other(records::TEXT, _) => notes.texts += 1,
            Primitive::Other(records::BODY, _) => notes.bodies += 1,
            Primitive::Other(..) => {}
        }
    }
    notes.no_courtyard = !courtyard;
    let top = ys.iter().copied().fold(0.0_f64, f64::min);
    let bottom = ys.iter().copied().fold(0.0_f64, f64::max);
    let through = primitives.iter().any(
        |p| matches!(p, Primitive::Pad(pad) if place.layer(pad.layer) == MULTI || pad.hole > 0.0),
    );
    let field = |field: &str, value: &str, y: f64, layer: &str| {
        Sexp::node(
            "property",
            vec![
                Sexp::string(field),
                Sexp::string(value),
                Sexp::node("at", vec![Sexp::num(0.0), Sexp::num(round(y))]),
                layer_node(layer),
                Sexp::node(
                    "effects",
                    vec![Sexp::node(
                        "font",
                        vec![
                            Sexp::node("size", vec![Sexp::num(1.0), Sexp::num(1.0)]),
                            Sexp::node("thickness", vec![Sexp::num(0.15)]),
                        ],
                    )],
                ),
            ],
        )
    };
    let mut items = vec![
        Sexp::string(name),
        Sexp::node("version", vec![Sexp::atom(VERSION)]),
        Sexp::node("generator", vec![Sexp::string("netsmith")]),
        layer_node("F.Cu"),
        field("Reference", "REF**", top - 1.0, "F.SilkS"),
        field("Value", name, bottom + 1.0, "F.Fab"),
        Sexp::node(
            "attr",
            vec![Sexp::atom(if through { "through_hole" } else { "smd" })],
        ),
    ];
    items.extend(graphics);
    items.extend(pads);
    let text = Sexp::node("footprint", items).to_kicad_string();
    (text, notes.lines())
}
