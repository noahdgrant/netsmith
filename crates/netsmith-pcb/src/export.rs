//! The KiCad 9 `.kicad_pcb` writer.
//!
//! Each footprint is the library's own file with its position, reference, value and nets
//! filled in, so the pads, drawings and 3D model are exactly what KiCad's library holds.
//! Net names are written as the design has them (`VCC`), with no sheet prefix: a board
//! written directly has no sheets.

use std::collections::HashMap;

use netsmith_core::OUNCE_OF_COPPER;
use netsmith_kicad::sexp::Sexp;
use uuid::Uuid;

use crate::board::{Board, KeepoutArea, Placed, Plane};
use crate::geometry::{Pt, Rect};
use crate::place::ORIGIN;
use crate::rules::Rules;
use crate::shape::{Edge, RoundRect};

/// The file format KiCad 9.0 writes.
const VERSION: &str = "20241229";

/// The thinnest copper a plane may be left with, in mm: KiCad's default for a zone.
const ZONE_MIN_THICKNESS: f64 = 0.25;

/// The gap and the spoke width of a thermal relief, in mm: KiCad's defaults.
const THERMAL: f64 = 0.5;

/// Width of the board edge line, in mm.
const EDGE_WIDTH: f64 = 0.05;

/// Fixed namespace so UUIDs are stable across runs and machines ("netsmith" in ASCII, as in
/// the schematic writer, but a different last digit so the two never collide).
const NAMESPACE: Uuid = Uuid::from_u128(0x6e65_7473_6d69_7468_0000_0000_0000_0002);

fn uuid_for(name: &str) -> Sexp {
    Sexp::node(
        "uuid",
        vec![Sexp::string(
            Uuid::new_v5(&NAMESPACE, name.as_bytes()).to_string(),
        )],
    )
}

/// The smallest standard sheet the outline fits on, with the margin round the board that
/// `ORIGIN` leaves.
fn paper_for(outline: &Rect) -> &'static str {
    const SHEETS: [(&str, f64, f64); 4] = [
        ("A4", 297.0, 210.0),
        ("A3", 420.0, 297.0),
        ("A2", 594.0, 420.0),
        ("A1", 841.0, 594.0),
    ];
    SHEETS
        .iter()
        .find(|(_, w, h)| outline.max.x + ORIGIN.x <= *w && outline.max.y + ORIGIN.y <= *h)
        .map_or("A0", |(name, _, _)| name)
}

/// The thickness of a solder mask layer, in mm, which JLCPCB's page gives as at least this.
const MASK: f64 = 0.01;

/// A length rounded to a nanometre, so sums of millimetre values print as written.
fn round(mm: f64) -> f64 {
    (mm * 1e6).round() / 1e6
}

/// The stackup the board's process builds. Two layers are two copper layers of the
/// process's weight round one core, which makes up the rest of the thickness the board
/// chose. Four are the outer copper, a prepreg, an inner layer, a core, an inner layer, a
/// prepreg and the outer copper, each as the process states it.
fn setup(board: &Board) -> Sexp {
    let stackup = &board.process.stackup;
    let copper_mm = round(stackup.copper_weight * OUNCE_OF_COPPER);
    let layer = |name: &str, kind: &str, rest: Vec<Sexp>| {
        let mut items = vec![
            Sexp::string(name),
            Sexp::node("type", vec![Sexp::string(kind)]),
        ];
        items.extend(rest);
        Sexp::node("layer", items)
    };
    let thickness = |t: f64| Sexp::node("thickness", vec![Sexp::num(t)]);
    let text = |key: &str, v: &str| Sexp::node(key, vec![Sexp::string(v)]);
    let mask = |name: &str, kind: &str| {
        layer(
            name,
            kind,
            vec![
                text("color", "Green"),
                thickness(MASK),
                text("material", "Dry Film"),
                Sexp::node("epsilon_r", vec![Sexp::num(3.3)]),
                Sexp::node("loss_tangent", vec![Sexp::num(0.0)]),
            ],
        )
    };
    let silk = |name: &str, kind: &str| {
        layer(
            name,
            kind,
            vec![text("color", "White"), text("material", "Liquid Photo")],
        )
    };
    let copper = |name: &str, mm: f64| layer(name, "copper", vec![thickness(mm)]);
    let dielectric = |number: usize, kind: &str, mm: f64, material: &str, epsilon: f64| {
        layer(
            &format!("dielectric {number}"),
            kind,
            vec![
                thickness(mm),
                text("material", material),
                Sexp::node("epsilon_r", vec![Sexp::num(epsilon)]),
                Sexp::node("loss_tangent", vec![Sexp::num(0.02)]),
            ],
        )
    };
    let mut body = vec![
        silk("F.SilkS", "Top Silk Screen"),
        layer("F.Paste", "Top Solder Paste", vec![]),
        mask("F.Mask", "Top Solder Mask"),
        copper("F.Cu", copper_mm),
    ];
    match &stackup.inner {
        None => {
            let core_mm = round(
                stackup
                    .chosen_thickness()
                    .expect("a board is only made once its thickness is chosen")
                    - 2.0 * copper_mm
                    - 2.0 * MASK,
            );
            body.push(dielectric(1, "core", core_mm, "FR4", 4.5));
        }
        Some(inner) => {
            let (first, last) = (&board.planes[0].layer, &board.planes[1].layer);
            body.extend([
                dielectric(1, "prepreg", inner.prepreg, "7628", inner.prepreg_epsilon),
                copper(first, inner.copper),
                dielectric(2, "core", inner.core, "FR4", inner.core_epsilon),
                copper(last, inner.copper),
                dielectric(3, "prepreg", inner.prepreg, "7628", inner.prepreg_epsilon),
            ]);
        }
    }
    body.extend([
        copper("B.Cu", copper_mm),
        mask("B.Mask", "Bottom Solder Mask"),
        layer("B.Paste", "Bottom Solder Paste", vec![]),
        silk("B.SilkS", "Bottom Silk Screen"),
        text("copper_finish", "HAL lead-free"),
        Sexp::node("dielectric_constraints", vec![Sexp::atom("no")]),
    ]);
    Sexp::node(
        "setup",
        vec![
            Sexp::node("stackup", body),
            Sexp::node("pad_to_mask_clearance", vec![Sexp::num(0.0)]),
            // The least mask between two openings, which both KiCad's check and netsmith's
            // read as the web the process can make.
            Sexp::node(
                "solder_mask_min_width",
                vec![Sexp::num(board.rules.mask_bridge)],
            ),
            Sexp::node(
                "allow_soldermask_bridges_in_footprints",
                vec![Sexp::atom("no")],
            ),
            Sexp::node("tenting", vec![Sexp::atom("front"), Sexp::atom("back")]),
        ],
    )
}

/// KiCad's number for a copper layer: the front is 0, the back 2, and the inner layers
/// 4, 6, ... from the top.
fn copper_number(layer: &str) -> u32 {
    match layer.strip_prefix("In").and_then(|n| n.strip_suffix(".Cu")) {
        Some(n) => 2 + 2 * n.parse::<u32>().expect("an inner layer is named In<n>.Cu"),
        None if layer == "F.Cu" => 0,
        None => 2,
    }
}

fn layer_table(board: &Board) -> Sexp {
    let copper: Vec<(u32, &str, &str, Option<&str>)> = std::iter::once("F.Cu")
        .chain(board.planes.iter().map(|p| p.layer.as_str()))
        .chain(["B.Cu"])
        .map(|name| (copper_number(name), name, "signal", None))
        .collect();
    let rows: [(u32, &str, &str, Option<&str>); 18] = [
        (9, "F.Adhes", "user", Some("F.Adhesive")),
        (11, "B.Adhes", "user", Some("B.Adhesive")),
        (13, "F.Paste", "user", None),
        (15, "B.Paste", "user", None),
        (5, "F.SilkS", "user", Some("F.Silkscreen")),
        (7, "B.SilkS", "user", Some("B.Silkscreen")),
        (1, "F.Mask", "user", None),
        (3, "B.Mask", "user", None),
        (17, "Dwgs.User", "user", Some("User.Drawings")),
        (19, "Cmts.User", "user", Some("User.Comments")),
        (21, "Eco1.User", "user", Some("User.Eco1")),
        (23, "Eco2.User", "user", Some("User.Eco2")),
        (25, "Edge.Cuts", "user", None),
        (27, "Margin", "user", None),
        (31, "F.CrtYd", "user", Some("F.Courtyard")),
        (29, "B.CrtYd", "user", Some("B.Courtyard")),
        (35, "F.Fab", "user", None),
        (33, "B.Fab", "user", None),
    ];
    Sexp::node(
        "layers",
        copper
            .iter()
            .chain(rows.iter())
            .map(|(number, name, kind, alias)| {
                let mut items = vec![Sexp::string(*name), Sexp::atom(*kind)];
                items.extend(alias.map(Sexp::string));
                Sexp::node(&number.to_string(), items)
            })
            .collect(),
    )
}

/// The node without any `(uuid ...)`.
fn without_uuid(node: &Sexp) -> Vec<Sexp> {
    match node {
        Sexp::List(items) => items
            .iter()
            .filter(|i| i.head() != Some("uuid"))
            .cloned()
            .collect(),
        other => vec![other.clone()],
    }
}

/// The node with a `(uuid ...)` made from `key` as its last item.
fn with_uuid(node: &Sexp, key: &str) -> Sexp {
    let mut items = without_uuid(node);
    items.push(uuid_for(key));
    Sexp::List(items)
}

/// The numbers KiCad gives nets: 0 is no net, then the board's nets in order.
struct NetNumbers<'a> {
    by_pad: HashMap<(&'a str, &'a str), (usize, &'a str)>,
}

impl<'a> NetNumbers<'a> {
    fn of(board: &'a Board) -> Self {
        let mut by_pad = HashMap::new();
        for (i, net) in board.nets.iter().enumerate() {
            for pad in &net.pads {
                by_pad.insert(
                    (pad.refdes.as_str(), pad.number.as_str()),
                    (i + 1, net.name.as_str()),
                );
            }
        }
        Self { by_pad }
    }
}

fn footprint(board: &Board, part: &Placed, nets: &NetNumbers) -> Sexp {
    let key = format!("{}/{}", board.name, part.refdes);
    let mut at = vec![Sexp::num(part.at.x), Sexp::num(part.at.y)];
    if part.rotation != 0.0 {
        at.push(Sexp::num(part.rotation));
    }
    let mut items = vec![
        Sexp::atom("footprint"),
        Sexp::string(part.library_id.clone()),
        Sexp::node("layer", vec![Sexp::string(part.side.copper().name())]),
        uuid_for(&key),
        Sexp::node("at", at),
    ];
    for (index, child) in part.footprint.source.args().iter().enumerate().skip(1) {
        // Skip what only a library file has, and what was just written above.
        if matches!(
            child.head(),
            Some("version" | "generator" | "generator_version" | "layer" | "uuid" | "at")
        ) {
            continue;
        }
        let child_key = format!("{key}/{index}");
        let written = match child.head() {
            Some("property") => property(child, part, &board.rules, &child_key),
            Some("pad") => pad(child, part, nets, &child_key),
            Some(
                "fp_line" | "fp_rect" | "fp_circle" | "fp_arc" | "fp_poly" | "fp_curve" | "fp_text"
                | "fp_text_box" | "zone",
            ) => with_uuid(child, &child_key),
            _ => child.clone(),
        };
        // KiCad stores a footprint on the back already flipped: the drawings of the file as
        // seen in a mirror, and on the layers of the other face.
        items.push(if part.mirrored() && child.head() != Some("model") {
            flipped(&written, None)
        } else {
            written
        });
    }
    Sexp::List(items)
}

/// A layer name on the other face: `F.SilkS` is `B.SilkS`. Layers of both faces and the
/// inner layers stay as they are.
fn other_face(layer: &str) -> String {
    match layer.split_once('.') {
        Some(("F", rest)) => format!("B.{rest}"),
        Some(("B", rest)) => format!("F.{rest}"),
        _ => layer.to_string(),
    }
}

/// The node as it is written for a footprint on the back of the board: every x in the
/// footprint's frame is negated (`start`, `end`, `mid`, `center`, `xy`, `at`), the layers
/// are the other face's, text is mirrored, and the turn of text and of a pad (its own turn
/// together with the footprint's, as KiCad writes it, so with the footprint's already in
/// it) is not touched here. `parent` is the head of the node it is in.
fn flipped(node: &Sexp, parent: Option<&str>) -> Sexp {
    let Sexp::List(items) = node else {
        return node.clone();
    };
    let head = node.head();
    let mut out: Vec<Sexp> = Vec::with_capacity(items.len());
    for (k, item) in items.iter().enumerate() {
        out.push(match (head, item) {
            (Some("layer" | "layers"), Sexp::Str(name)) if k > 0 => Sexp::string(other_face(name)),
            (Some("start" | "end" | "mid" | "center" | "xy" | "at" | "offset"), Sexp::Atom(x))
                if k == 1 =>
            {
                match x.parse::<f64>() {
                    Ok(v) => Sexp::num(-v),
                    Err(_) => item.clone(),
                }
            }
            // Text turns the other way when it is mirrored.
            (Some("at"), Sexp::Atom(angle))
                if k == 3 && matches!(parent, Some("property" | "fp_text")) =>
            {
                match angle.parse::<f64>() {
                    Ok(v) => Sexp::num(-v),
                    Err(_) => item.clone(),
                }
            }
            (Some("chamfer"), Sexp::Atom(corner)) if k > 0 => Sexp::atom(match corner.as_str() {
                "top_left" => "top_right",
                "top_right" => "top_left",
                "bottom_left" => "bottom_right",
                "bottom_right" => "bottom_left",
                other => other,
            }),
            (Some("justify"), Sexp::Atom(a)) if k > 0 && a == "left" => Sexp::atom("right"),
            (Some("justify"), Sexp::Atom(a)) if k > 0 && a == "right" => Sexp::atom("left"),
            _ => flipped(item, head),
        });
    }
    if head == Some("effects") && node.find("justify").is_none() {
        out.push(Sexp::node("justify", vec![Sexp::atom("mirror")]));
    }
    if head == Some("justify") && !out.iter().any(|i| i.text() == Some("mirror")) {
        out.push(Sexp::atom("mirror"));
    }
    Sexp::List(out)
}

/// The reference and value are the part's; every other property is the library's.
fn property(child: &Sexp, part: &Placed, rules: &Rules, key: &str) -> Sexp {
    let mut items = without_uuid(child);
    match child.args().first().and_then(Sexp::text) {
        Some("Reference") => {
            items[2] = Sexp::string(part.refdes.clone());
            let at = part.reference_at(rules);
            for item in &mut items {
                if item.head() == Some("at") {
                    *item =
                        Sexp::node("at", vec![Sexp::num(at.x), Sexp::num(at.y), Sexp::num(0.0)]);
                } else if item.head() == Some("effects") {
                    *item = text_no_smaller(item, rules);
                }
            }
        }
        Some("Value") => items[2] = Sexp::string(part.value.clone()),
        _ => {}
    }
    items.push(uuid_for(key));
    Sexp::List(items)
}

/// The text's effects with the font no smaller than the process allows on silkscreen: a
/// library may draw its reference at 0.5 mm, which the fabricator cannot print.
fn text_no_smaller(node: &Sexp, rules: &Rules) -> Sexp {
    let Sexp::List(items) = node else {
        return node.clone();
    };
    let at_least = |item: &Sexp, least: f64| match item {
        Sexp::Atom(a) => a
            .parse::<f64>()
            .map_or_else(|_| item.clone(), |v| Sexp::num(v.max(least))),
        other => other.clone(),
    };
    Sexp::List(
        items
            .iter()
            .map(|item| match item.head() {
                Some("size") => Sexp::node(
                    "size",
                    item.args()
                        .iter()
                        .map(|a| at_least(a, rules.min_text_height))
                        .collect(),
                ),
                Some("thickness") => Sexp::node(
                    "thickness",
                    item.args()
                        .iter()
                        .map(|a| at_least(a, rules.min_text_thickness))
                        .collect(),
                ),
                _ => text_no_smaller(item, rules),
            })
            .collect(),
    )
}

/// A pad with its net, and its angle on the board (KiCad writes the pad's turn including
/// the footprint's).
fn pad(child: &Sexp, part: &Placed, nets: &NetNumbers, key: &str) -> Sexp {
    let number = child
        .args()
        .first()
        .and_then(Sexp::text)
        .expect("the footprint reader checked that a pad has a number");
    let mut items: Vec<Sexp> = without_uuid(child)
        .into_iter()
        .map(|item| {
            if item.head() != Some("at") {
                return item;
            }
            let args = item.args();
            let own: f64 = args
                .get(2)
                .and_then(Sexp::text)
                .and_then(|a| a.parse().ok())
                .unwrap_or(0.0);
            let mut at = args[..2].to_vec();
            let angle = if part.mirrored() {
                part.rotation - own
            } else {
                own + part.rotation
            };
            if angle != 0.0 {
                at.push(Sexp::num(angle));
            }
            Sexp::node("at", at)
        })
        .collect();
    if let Some((n, name)) = nets.by_pad.get(&(part.refdes.as_str(), number)) {
        items.push(Sexp::node(
            "net",
            vec![Sexp::num(*n as f64), Sexp::string(*name)],
        ));
    }
    items.push(uuid_for(key));
    Sexp::List(items)
}

/// The text of the `.kicad_pro` that goes beside the board's `.kicad_pcb`, holding the
/// board's design rules so KiCad's checks use them.
pub fn kicad_pro(board: &Board) -> String {
    board.rules.kicad_pro(&board.name, &board.net_classes())
}

/// The zone that pours a plane: the whole board inside the least space to the edge, kept
/// `plane_clearance` from copper of other nets, joined to its own pads and vias with
/// KiCad's thermal reliefs (gap and spoke width 0.5 mm, its defaults).
///
/// The file holds no fill. KiCad fills a zone when a person opens the board and presses
/// `B`; the fabrication step has KiCad fill it before it checks and plots the board.
fn zone(board: &Board, plane: &Plane, net: Sexp) -> Sexp {
    let inset = board.outline.expanded(-board.rules.copper_to_edge);
    let points: Vec<Sexp> = inset
        .corners()
        .iter()
        .map(|c| Sexp::node("xy", vec![Sexp::num(c.x), Sexp::num(c.y)]))
        .collect();
    let key = format!("{}/zone/{}", board.name, plane.layer);
    Sexp::node(
        "zone",
        vec![
            net,
            Sexp::node("net_name", vec![Sexp::string(plane.net.clone())]),
            Sexp::node("layer", vec![Sexp::string(plane.layer.clone())]),
            uuid_for(&key),
            Sexp::node("hatch", vec![Sexp::atom("edge"), Sexp::num(0.5)]),
            Sexp::node(
                "connect_pads",
                vec![Sexp::node(
                    "clearance",
                    vec![Sexp::num(board.rules.plane_clearance)],
                )],
            ),
            Sexp::node("min_thickness", vec![Sexp::num(ZONE_MIN_THICKNESS)]),
            Sexp::node("filled_areas_thickness", vec![Sexp::atom("no")]),
            Sexp::node(
                "fill",
                vec![
                    Sexp::atom("yes"),
                    Sexp::node("thermal_gap", vec![Sexp::num(THERMAL)]),
                    Sexp::node("thermal_bridge_width", vec![Sexp::num(THERMAL)]),
                ],
            ),
            Sexp::node("polygon", vec![Sexp::node("pts", points)]),
        ],
    )
}

/// A shape drawn on the board edge layer: one `gr_rect` if its corners are square, else the
/// lines and quarter circles round it. `key` makes the uuids.
fn edge_cuts(shape: &RoundRect, key: &str) -> Vec<Sexp> {
    let at = |p: Pt| vec![Sexp::num(p.x), Sexp::num(p.y)];
    let stroke = || {
        Sexp::node(
            "stroke",
            vec![
                Sexp::node("width", vec![Sexp::num(EDGE_WIDTH)]),
                Sexp::node("type", vec![Sexp::atom("default")]),
            ],
        )
    };
    let layer = || Sexp::node("layer", vec![Sexp::string("Edge.Cuts")]);
    if shape.radius == 0.0 {
        return vec![Sexp::node(
            "gr_rect",
            vec![
                Sexp::node("start", at(shape.rect.min)),
                Sexp::node("end", at(shape.rect.max)),
                stroke(),
                Sexp::node("fill", vec![Sexp::atom("no")]),
                layer(),
                uuid_for(key),
            ],
        )];
    }
    shape
        .edges()
        .into_iter()
        .enumerate()
        .map(|(k, edge)| {
            let uuid = uuid_for(&format!("{key}/{k}"));
            match edge {
                Edge::Line(a, b) => Sexp::node(
                    "gr_line",
                    vec![
                        Sexp::node("start", at(a)),
                        Sexp::node("end", at(b)),
                        stroke(),
                        layer(),
                        uuid,
                    ],
                ),
                Edge::Arc { start, mid, end } => Sexp::node(
                    "gr_arc",
                    vec![
                        Sexp::node("start", at(start)),
                        Sexp::node("mid", at(mid)),
                        Sexp::node("end", at(end)),
                        stroke(),
                        layer(),
                        uuid,
                    ],
                ),
            }
        })
        .collect()
}

/// KiCad's name for a copper layer given as a keepout's `top`, `bottom`, `inner1`...
fn copper_name(layer: &str) -> String {
    match layer {
        "top" => "F.Cu".to_string(),
        "bottom" => "B.Cu".to_string(),
        inner => format!("In{}.Cu", inner.trim_start_matches("inner")),
    }
}

/// A keepout as a KiCad rule area: a zone with no net that says what it does not allow.
/// A keepout that forbids parts forbids their pads as well.
fn rule_area(board: &Board, k: &KeepoutArea) -> Sexp {
    let flag = |name: &str, forbidden: bool| {
        Sexp::node(
            name,
            vec![Sexp::atom(if forbidden {
                "not_allowed"
            } else {
                "allowed"
            })],
        )
    };
    let points: Vec<Sexp> = k
        .rect
        .corners()
        .iter()
        .map(|c| Sexp::node("xy", vec![Sexp::num(c.x), Sexp::num(c.y)]))
        .collect();
    Sexp::node(
        "zone",
        vec![
            Sexp::node("net", vec![Sexp::num(0.0)]),
            Sexp::node("net_name", vec![Sexp::string("")]),
            Sexp::node(
                "layers",
                k.layers
                    .iter()
                    .map(|l| Sexp::string(copper_name(l)))
                    .collect(),
            ),
            uuid_for(&format!("{}/keepout/{}", board.name, k.name)),
            Sexp::node("name", vec![Sexp::string(k.name.clone())]),
            Sexp::node("hatch", vec![Sexp::atom("edge"), Sexp::num(0.5)]),
            Sexp::node(
                "connect_pads",
                vec![Sexp::node("clearance", vec![Sexp::num(0.0)])],
            ),
            Sexp::node("min_thickness", vec![Sexp::num(ZONE_MIN_THICKNESS)]),
            Sexp::node("filled_areas_thickness", vec![Sexp::atom("no")]),
            Sexp::node(
                "keepout",
                vec![
                    flag("tracks", k.tracks),
                    flag("vias", k.vias),
                    flag("pads", k.parts),
                    flag("copperpour", k.copper),
                    flag("footprints", k.parts),
                ],
            ),
            Sexp::node(
                "fill",
                vec![
                    Sexp::node("thermal_gap", vec![Sexp::num(THERMAL)]),
                    Sexp::node("thermal_bridge_width", vec![Sexp::num(THERMAL)]),
                ],
            ),
            Sexp::node("polygon", vec![Sexp::node("pts", points)]),
        ],
    )
}

/// The text of a `.kicad_pcb` for the board.
pub fn kicad_pcb(board: &Board) -> String {
    board_tree(board).to_kicad_string()
}

/// The `.kicad_pcb` of the board as a tree, before it is written as text: what
/// [`crate::copper`] reads the board's copper from, so that it is the copper of the file.
pub(crate) fn board_tree(board: &Board) -> Sexp {
    let nets = NetNumbers::of(board);

    let mut items = vec![
        Sexp::atom("kicad_pcb"),
        Sexp::node("version", vec![Sexp::atom(VERSION)]),
        Sexp::node("generator", vec![Sexp::string("netsmith")]),
        Sexp::node(
            "generator_version",
            vec![Sexp::string(env!("CARGO_PKG_VERSION"))],
        ),
        Sexp::node(
            "general",
            vec![
                Sexp::node(
                    "thickness",
                    vec![Sexp::num(
                        board
                            .process
                            .stackup
                            .chosen_thickness()
                            .expect("a board is only made once its thickness is chosen"),
                    )],
                ),
                Sexp::node("legacy_teardrops", vec![Sexp::atom("no")]),
            ],
        ),
        Sexp::node("paper", vec![Sexp::string(paper_for(&board.outline))]),
        Sexp::node(
            "title_block",
            vec![Sexp::node("title", vec![Sexp::string(board.name.clone())])],
        ),
        layer_table(board),
        setup(board),
        Sexp::node("net", vec![Sexp::num(0.0), Sexp::string("")]),
    ];
    for (i, net) in board.nets.iter().enumerate() {
        items.push(Sexp::node(
            "net",
            vec![Sexp::num((i + 1) as f64), Sexp::string(net.name.clone())],
        ));
    }
    for part in &board.parts {
        items.push(footprint(board, part, &nets));
    }
    let net_number = |name: &str| {
        let i = board
            .nets
            .iter()
            .position(|n| n.name == name)
            .expect("routed copper is on a net of the board");
        Sexp::node("net", vec![Sexp::num((i + 1) as f64)])
    };
    for (i, track) in board.tracks.iter().enumerate() {
        let at = |p: crate::geometry::Pt| vec![Sexp::num(p.x), Sexp::num(p.y)];
        items.push(Sexp::node(
            "segment",
            vec![
                Sexp::node("start", at(track.from)),
                Sexp::node("end", at(track.to)),
                Sexp::node("width", vec![Sexp::num(track.width)]),
                Sexp::node("layer", vec![Sexp::string(track.layer.name())]),
                net_number(&track.net),
                uuid_for(&format!("{}/track/{i}", board.name)),
            ],
        ));
    }
    for (i, via) in board.vias.iter().enumerate() {
        items.push(Sexp::node(
            "via",
            vec![
                Sexp::node("at", vec![Sexp::num(via.at.x), Sexp::num(via.at.y)]),
                Sexp::node("size", vec![Sexp::num(via.diameter)]),
                Sexp::node("drill", vec![Sexp::num(via.drill)]),
                Sexp::node("layers", vec![Sexp::string("F.Cu"), Sexp::string("B.Cu")]),
                net_number(&via.net),
                uuid_for(&format!("{}/via/{i}", board.name)),
            ],
        ));
    }
    for plane in &board.planes {
        items.push(zone(board, plane, net_number(&plane.net)));
    }
    // An outer pour is the same zone on an outer layer; KiCad fills it round the tracks.
    for pour in &board.pours {
        let layer = Plane {
            layer: pour.layer.name().to_string(),
            net: pour.net.clone(),
        };
        items.push(zone(board, &layer, net_number(&pour.net)));
    }
    // A keepout that only limits height is no rule area: KiCad has nothing to say about it.
    for area in board
        .keepout_areas()
        .iter()
        .filter(|a| a.forbids_anything())
    {
        items.push(rule_area(board, area));
    }
    items.extend(edge_cuts(
        &board.outline_shape(),
        &format!("{}/outline", board.name),
    ));
    for (cutout, shape) in board.cutout_shapes() {
        items.extend(edge_cuts(
            &shape,
            &format!("{}/cutout/{}", board.name, cutout.name),
        ));
    }
    Sexp::List(items)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::board::tests::{fixtures, four_layer_design, led_design};
    use crate::geometry::Pt;

    use super::*;

    fn led() -> Board {
        Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap()
    }

    fn parsed(board: &Board) -> Sexp {
        Sexp::parse(&kicad_pcb(board)).unwrap()
    }

    fn footprint_of<'a>(pcb: &'a Sexp, refdes: &str) -> &'a Sexp {
        pcb.find_all("footprint")
            .find(|f| {
                f.find_all("property").any(|p| {
                    p.args().first().and_then(Sexp::text) == Some("Reference")
                        && p.args().get(1).and_then(Sexp::text) == Some(refdes)
                })
            })
            .unwrap_or_else(|| panic!("no footprint {refdes}"))
    }

    fn pad_net<'a>(fp: &'a Sexp, number: &str) -> Option<(&'a str, &'a str)> {
        let pad = fp
            .find_all("pad")
            .find(|p| p.args().first().and_then(Sexp::text) == Some(number))?;
        let net = pad.find("net")?;
        Some((net.args()[0].text()?, net.args()[1].text()?))
    }

    #[test]
    fn writes_the_header_the_layers_and_the_net_list() {
        let pcb = parsed(&led());
        assert_eq!(pcb.head(), Some("kicad_pcb"));
        assert_eq!(
            pcb.find("version").unwrap().args()[0].text(),
            Some("20241229")
        );
        assert_eq!(
            pcb.find("generator").unwrap().args()[0].text(),
            Some("netsmith")
        );
        assert_eq!(pcb.find("paper").unwrap().args()[0].text(), Some("A4"));
        let layers = pcb.find("layers").unwrap();
        assert!(layers.find("0").is_some() && layers.find("2").is_some());
        assert!(layers.find("25").is_some(), "Edge.Cuts");
        let nets: Vec<(&str, &str)> = pcb
            .find_all("net")
            .map(|n| (n.args()[0].text().unwrap(), n.args()[1].text().unwrap()))
            .collect();
        // Net names carry no sheet prefix.
        assert_eq!(
            nets,
            [
                ("0", ""),
                ("1", "GND"),
                ("2", "VCC"),
                ("3", "net-led.anode")
            ]
        );
    }

    #[test]
    fn every_footprint_is_placed_with_its_reference_and_value() {
        let mut board = led();
        board.parts.iter_mut().for_each(|p| p.rotation = 0.0);
        let pcb = parsed(&board);
        assert_eq!(pcb.find_all("footprint").count(), 3);
        let r1 = footprint_of(&pcb, "R1");
        assert_eq!(r1.args()[0].text(), Some("Resistor_SMD:R_0402_1005Metric"));
        let at = r1.find("at").unwrap();
        let placed = board.part("R1").unwrap().at;
        assert_eq!(at.args()[0].text(), Some(fmt(placed.x).as_str()));
        assert_eq!(at.args()[1].text(), Some(fmt(placed.y).as_str()));
        assert_eq!(at.args().len(), 2, "no angle when it is zero");
        let value = r1
            .find_all("property")
            .find(|p| p.args()[0].text() == Some("Value"))
            .unwrap();
        assert_eq!(value.args()[1].text(), Some("300"));
        assert!(r1.find("model").is_some(), "the library's 3D model is kept");
    }

    #[test]
    fn a_part_on_the_back_is_written_flipped_on_the_back_layers() {
        let mut board = led();
        let (r1_at, r1_pad2) = {
            let r1 = board.part("R1").unwrap();
            (r1.at, r1.footprint.pads_numbered("2").next().unwrap().at)
        };
        board
            .parts
            .iter_mut()
            .find(|p| p.refdes == "R1")
            .unwrap()
            .side = crate::board::Side::Back;
        let pcb = parsed(&board);
        let r1 = footprint_of(&pcb, "R1");
        assert_eq!(r1.find("layer").unwrap().args()[0].text(), Some("B.Cu"));
        let at = r1.find("at").unwrap();
        assert_eq!(at.args()[0].text(), Some(fmt(r1_at.x).as_str()));
        // The pad's x is negated, and it is on the back layers.
        let pad2 = r1
            .find_all("pad")
            .find(|p| p.args()[0].text() == Some("2"))
            .unwrap();
        let pad_at = pad2.find("at").unwrap();
        assert_eq!(pad_at.args()[0].text(), Some(fmt(-r1_pad2.x).as_str()));
        let layers: Vec<&str> = pad2
            .find("layers")
            .unwrap()
            .args()
            .iter()
            .filter_map(Sexp::text)
            .collect();
        assert!(
            layers.contains(&"B.Cu") && layers.contains(&"B.Paste"),
            "{layers:?}"
        );
        assert!(!layers.iter().any(|l| l.starts_with("F.")), "{layers:?}");
        // Silkscreen text and the courtyard are on the back, and the text is mirrored.
        let reference = r1
            .find_all("property")
            .find(|p| p.args()[0].text() == Some("Reference"))
            .unwrap();
        assert_eq!(
            reference.find("layer").unwrap().args()[0].text(),
            Some("B.SilkS")
        );
        assert!(reference.find("effects").unwrap().find("justify").is_some());
        let r1_text = r1.to_kicad_string();
        assert!(
            !r1_text.contains("F.CrtYd") && !r1_text.contains("F.SilkS"),
            "{r1_text}"
        );
        // The others are still on the front.
        let d1 = footprint_of(&pcb, "D1");
        assert_eq!(d1.find("layer").unwrap().args()[0].text(), Some("F.Cu"));
    }

    #[test]
    fn a_reference_is_never_written_smaller_than_the_process_prints() {
        let mut board = led();
        // A library that draws its references at 0.5 mm with a thin stroke.
        let source = board.parts[0].footprint.source.to_kicad_string();
        board.parts[0].footprint.source = Sexp::parse(
            &source
                .replace("(size 1 1)", "(size 0.5 0.6)")
                .replace("(thickness 0.15)", "(thickness 0.08)"),
        )
        .unwrap();
        let pcb = parsed(&board);
        let refdes = board.parts[0].refdes.clone();
        let fp = footprint_of(&pcb, &refdes);
        let reference = fp
            .find_all("property")
            .find(|p| p.args()[0].text() == Some("Reference"))
            .unwrap();
        let font = reference.find("effects").unwrap().find("font").unwrap();
        let size: Vec<&str> = font
            .find("size")
            .unwrap()
            .args()
            .iter()
            .filter_map(Sexp::text)
            .collect();
        assert_eq!(size, ["1", "1"], "{}", font.to_kicad_string());
        assert_eq!(
            font.find("thickness").unwrap().args()[0].text(),
            Some("0.15")
        );
    }

    fn fmt(v: f64) -> String {
        netsmith_kicad::sexp::fmt_num(v)
    }

    #[test]
    fn each_pad_carries_its_net() {
        let pcb = parsed(&led());
        let d1 = footprint_of(&pcb, "D1");
        let j1 = footprint_of(&pcb, "J1");
        let r1 = footprint_of(&pcb, "R1");
        assert_eq!(pad_net(d1, "1"), Some(("1", "GND")));
        assert_eq!(pad_net(d1, "2"), Some(("3", "net-led.anode")));
        assert_eq!(pad_net(j1, "1"), Some(("2", "VCC")));
        assert_eq!(pad_net(j1, "2"), Some(("1", "GND")));
        assert_eq!(pad_net(r1, "1"), Some(("2", "VCC")));
        assert_eq!(pad_net(r1, "2"), Some(("3", "net-led.anode")));
    }

    #[test]
    fn pads_that_share_a_number_share_a_net_and_a_pad_on_no_net_has_none() {
        let mut design = led_design();
        design.parts.push(netsmith_core::Part {
            provides: Vec::new(),
            refdes: "U1".into(),
            kind: "LinearRegulator".into(),
            value: "LDO".into(),
            package: Some("SOT-223".into()),
            path: String::new(),
            pins: Vec::new(),
        });
        design.nets[0].pins.push(netsmith_core::PinRef {
            refdes: "U1".into(),
            pin: "2".into(),
        });
        let pcb = parsed(
            &Board::from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures())
                .unwrap(),
        );
        let u1 = footprint_of(&pcb, "U1");
        let on_net: Vec<_> = u1
            .find_all("pad")
            .filter(|p| p.args()[0].text() == Some("2"))
            .map(|p| {
                p.find("net")
                    .map(|n| n.args()[1].text().unwrap().to_string())
            })
            .collect();
        assert_eq!(on_net, [Some("GND".to_string()), Some("GND".to_string())]);
        assert_eq!(pad_net(u1, "1"), None);
    }

    #[test]
    fn the_outline_is_on_edge_cuts() {
        let board = led();
        let pcb = parsed(&board);
        let rect = pcb.find("gr_rect").unwrap();
        assert_eq!(
            rect.find("layer").unwrap().args()[0].text(),
            Some("Edge.Cuts")
        );
        let corner = |name: &str| -> Pt {
            let n = rect.find(name).unwrap();
            Pt::new(
                n.args()[0].text().unwrap().parse().unwrap(),
                n.args()[1].text().unwrap().parse().unwrap(),
            )
        };
        assert_eq!(corner("start"), board.outline.min);
        assert_eq!(corner("end"), board.outline.max);
    }

    #[test]
    fn uuids_are_valid_unique_and_stable() {
        fn collect(node: &Sexp, out: &mut Vec<String>) {
            if node.head() == Some("uuid") {
                out.push(node.args()[0].text().unwrap().to_string());
            }
            for child in node.args() {
                collect(child, out);
            }
        }
        let text = kicad_pcb(&led());
        let mut uuids = Vec::new();
        collect(&Sexp::parse(&text).unwrap(), &mut uuids);
        assert!(uuids.len() > 40);
        assert!(uuids.iter().all(|u| Uuid::parse_str(u).is_ok()));
        assert_eq!(uuids.iter().collect::<HashSet<_>>().len(), uuids.len());
        assert_eq!(text, kicad_pcb(&led()));
    }

    #[test]
    fn nothing_from_the_library_file_header_is_copied() {
        let text = kicad_pcb(&led());
        assert_eq!(text.matches("(generator ").count(), 1);
        assert_eq!(text.matches("(version ").count(), 1);
        assert!(!text.contains("REF**"));
    }

    #[test]
    fn a_rotated_part_writes_its_angle_on_the_footprint_and_its_pads() {
        let mut board = led();
        board
            .parts
            .iter_mut()
            .find(|p| p.refdes == "R1")
            .unwrap()
            .rotation = 90.0;
        let pcb = parsed(&board);
        let r1 = footprint_of(&pcb, "R1");
        assert_eq!(r1.find("at").unwrap().args()[2].text(), Some("90"));
        let pad = r1.find("pad").unwrap();
        assert_eq!(pad.find("at").unwrap().args()[2].text(), Some("90"));
    }

    fn four() -> Board {
        Board::from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap()
    }

    fn numbers(node: &Sexp, key: &str) -> Vec<f64> {
        node.find(key)
            .map(|n| {
                n.args()
                    .iter()
                    .filter_map(|a| a.text()?.parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn a_four_layer_board_lists_its_copper_layers_front_to_back() {
        let pcb = parsed(&four());
        let layers = pcb.find("layers").unwrap();
        let names: Vec<(&str, &str)> = layers
            .args()
            .iter()
            .take(4)
            .map(|l| (l.head().unwrap(), l.args()[0].text().unwrap()))
            .collect();
        assert_eq!(
            names,
            [
                ("0", "F.Cu"),
                ("4", "In1.Cu"),
                ("6", "In2.Cu"),
                ("2", "B.Cu")
            ]
        );
        // The two-layer file is as it was.
        let two = parsed(&led());
        assert!(!kicad_pcb(&led()).contains("In1.Cu"));
        assert_eq!(two.find("layers").unwrap().args().len(), 20);
    }

    #[test]
    fn a_four_layer_stackup_is_the_one_the_process_states() {
        let pcb = parsed(&four());
        let stackup = pcb.find("setup").unwrap().find("stackup").unwrap();
        let rows: Vec<(&str, Option<f64>)> = stackup
            .args()
            .iter()
            .filter(|l| l.head() == Some("layer"))
            .map(|l| {
                (
                    l.args()[0].text().unwrap(),
                    numbers(l, "thickness").first().copied(),
                )
            })
            .collect();
        let copper_and_dielectric: Vec<_> = rows
            .iter()
            .filter(|(n, _)| n.ends_with(".Cu") || n.starts_with("dielectric"))
            .collect();
        assert_eq!(
            copper_and_dielectric,
            [
                &("F.Cu", Some(0.035)),
                &("dielectric 1", Some(0.2104)),
                &("In1.Cu", Some(0.0152)),
                &("dielectric 2", Some(1.065)),
                &("In2.Cu", Some(0.0152)),
                &("dielectric 3", Some(0.2104)),
                &("B.Cu", Some(0.035)),
            ]
        );
        let total: f64 = copper_and_dielectric.iter().filter_map(|(_, t)| *t).sum();
        assert!((total - 1.5862).abs() < 1e-9, "{total}");
        let prepreg = stackup
            .args()
            .iter()
            .find(|l| l.args().first().and_then(Sexp::text) == Some("dielectric 1"))
            .unwrap();
        assert_eq!(
            prepreg.find("type").unwrap().args()[0].text(),
            Some("prepreg")
        );
        assert_eq!(numbers(prepreg, "epsilon_r"), [4.4]);
        assert_eq!(
            pcb.find("general")
                .unwrap()
                .find("thickness")
                .unwrap()
                .args()[0]
                .text(),
            Some("1.6")
        );
    }

    #[test]
    fn each_plane_is_a_zone_on_its_layer_and_net() {
        let board = four();
        let pcb = parsed(&board);
        let zones: Vec<&Sexp> = pcb.find_all("zone").collect();
        assert_eq!(zones.len(), 2);
        let summary: Vec<(String, String, String)> = zones
            .iter()
            .map(|z| {
                (
                    z.find("layer").unwrap().args()[0].text().unwrap().into(),
                    z.find("net_name").unwrap().args()[0].text().unwrap().into(),
                    z.find("net").unwrap().args()[0].text().unwrap().into(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("In1.Cu".into(), "GND".into(), "1".into()),
                ("In2.Cu".into(), "3V3".into(), "2".into())
            ]
        );
        let z = zones[0];
        // Clear of copper of other nets by the plane's clearance, thermal reliefs at KiCad's
        // defaults, and a fill that KiCad makes.
        assert_eq!(numbers(z.find("connect_pads").unwrap(), "clearance"), [0.3]);
        let fill = z.find("fill").unwrap();
        assert_eq!(fill.args()[0].text(), Some("yes"));
        assert_eq!(numbers(fill, "thermal_gap"), [0.5]);
        assert_eq!(numbers(fill, "thermal_bridge_width"), [0.5]);
        // The pour is the board inside the least space to the edge.
        let pts: Vec<(f64, f64)> = z
            .find("polygon")
            .unwrap()
            .find("pts")
            .unwrap()
            .args()
            .iter()
            .map(|p| {
                (
                    p.args()[0].text().unwrap().parse().unwrap(),
                    p.args()[1].text().unwrap().parse().unwrap(),
                )
            })
            .collect();
        let inset = board.outline.expanded(-board.rules.copper_to_edge);
        assert_eq!(pts.len(), 4);
        assert_eq!(pts[0], (inset.min.x, inset.min.y));
        assert_eq!(pts[2], (inset.max.x, inset.max.y));
    }

    #[test]
    fn a_two_layer_board_has_no_zones() {
        assert_eq!(parsed(&led()).find_all("zone").count(), 0);
    }

    #[test]
    fn a_big_board_moves_to_a_bigger_sheet() {
        let mut outline = led().outline;
        assert_eq!(paper_for(&outline), "A4");
        outline.max = Pt::new(300.0, 100.0);
        assert_eq!(paper_for(&outline), "A3");
        outline.max = Pt::new(300.0, 400.0);
        assert_eq!(paper_for(&outline), "A2");
    }
}
