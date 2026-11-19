//! A footprint taken from an Eagle library (`.lbr`) or board (`.brd`) into a project's library
//! (issue #1430, `netsmith import footprint`), written as a `.kicad_mod` as a KiCad input is
//! ([`crate::import_footprint`]).
//!
//! Both files are XML. A library holds its packages under `drawing/library/packages`; a board
//! holds a copy of each library it uses under `drawing/board/libraries`, and places each part as
//! an `element` that names its library and its package. A board keeps the package as the library
//! draws it, seen from the top, whatever the element's place, angle or face, so nothing of the
//! board has to be undone: the element only says which package to take.
//!
//! What a package draws becomes:
//!
//! - an `smd` a pad of the face its layer is on (1 the top, 16 the bottom), `rect`, or
//!   `roundrect` when it has a `roundness` (Eagle's percentage of half the shorter side, so
//!   KiCad's ratio is half of it), with mask and paste unless `stop` or `cream` is `no`;
//! - a `pad` a through-hole pad on every copper layer: `square` is `rect`, `round` is `circle`,
//!   `long` is an `oval` twice as long as it is wide, `offset` the same with its hole at one end,
//!   and `octagon` is written `circle` (netsmith reads no octagon), said in the notes. A pad with
//!   no `diameter` gets Eagle's default ring, a quarter of the drill held between 0.254 mm and
//!   0.508 mm;
//! - a `hole` a pad with no number and no copper (`np_thru_hole`);
//! - a `wire` a line, or an arc when it has a `curve`; a `circle` a circle (filled when its width
//!   is 0); a `rectangle` a filled polygon (turned by its `rot`; on the courtyard, an outline);
//!   a `polygon` a polygon, its curved edges drawn straight (said in the notes);
//! - the texts `>NAME` and `>VALUE` the footprint's reference (`REF**`) and value (its name), and
//!   another text a text of its own.
//!
//! The layers are Eagle's own numbers: tPlace and bPlace (21, 22) the silkscreen, tDocu and bDocu
//! (51, 52) the fabrication layer, tStop and bStop (29, 30) the mask, tCream and bCream (31, 32)
//! the paste, Top and Bottom (1, 16) copper. Eagle has no courtyard layer of its own: a layer the
//! file names `tCourtyard`, `tCrtYd` or the like (any name with `courtyard` or `crtyd` in it; a
//! name that starts with `b` the back's) is the courtyard. A drawing on any other layer (the
//! keepouts, the dimension, the names and values beside their texts, a user layer) is left out,
//! and the notes count them by layer.
//!
//! Eagle's y grows up and KiCad's down, so every y is negated; an angle counter-clockwise on the
//! screen is the same number in both.

use roxmltree::{Document, Node};

use crate::footprint_import::ImportedFootprint;
use crate::sexp::Sexp;

/// Where the package comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EagleSource<'a> {
    /// A `.lbr` library: its one package.
    Library,
    /// A `.brd` board: the package of the element with this name (its reference designator).
    Board { reference: &'a str },
}

/// The package of `text` (an Eagle library or board, as `source` says), named `name` (or the
/// package's own name when `None`), as a `.kicad_mod`. The error says what the file lacks.
pub fn import_eagle_footprint(
    text: &str,
    source: EagleSource,
    name: Option<&str>,
) -> Result<ImportedFootprint, String> {
    // Eagle writes `<!DOCTYPE eagle SYSTEM "eagle.dtd">`, which names a DTD and declares nothing.
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc =
        Document::parse_with_options(text, options).map_err(|e| format!("not an XML file: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "eagle" {
        return Err(format!(
            "not an Eagle file: it starts with `<{}>`, not `<eagle>`",
            root.tag_name().name()
        ));
    }
    let drawing = child(root, "drawing").ok_or("the Eagle file has no <drawing>")?;
    let layers = Layers::of(drawing);
    let (package, board_angle, from_bottom) = match source {
        EagleSource::Library => {
            let library = child(drawing, "library").ok_or(
                "not an Eagle library: it has no <library> (a board, `.brd`, takes `--ref REF`)",
            )?;
            (library_package(library)?, 0.0, false)
        }
        EagleSource::Board { reference } => {
            let board = child(drawing, "board").ok_or("not an Eagle board: it has no <board>")?;
            let (package, rot) = board_package(board, reference)?;
            (package, rot.angle, rot.mirror)
        }
    };
    let source_name = package.attribute("name").unwrap_or_default().to_string();
    let name = name.map_or_else(|| source_name.clone(), str::to_string);
    if name.is_empty() {
        return Err("the package has no name: give one with `--name`".into());
    }
    let mut out = Writer {
        layers: &layers,
        notes: Notes::default(),
    };
    let text = out.footprint(package, &name);
    Ok(ImportedFootprint {
        source_name,
        name,
        // The board keeps the package as the library draws it: there is nothing to turn back.
        from_bottom: false,
        board_angle,
        text,
        notes: {
            let mut notes = out.notes.finish();
            if from_bottom {
                notes.insert(
                    0,
                    "the part is on the bottom face of the board; Eagle keeps its package drawn from the top".into(),
                );
            }
            notes
        },
    })
}

fn child<'a, 'i>(node: Node<'a, 'i>, tag: &str) -> Option<Node<'a, 'i>> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == tag)
}

fn children<'a, 'i>(node: Node<'a, 'i>, tag: &'a str) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children()
        .filter(move |c| c.is_element() && c.tag_name().name() == tag)
}

fn packages<'a, 'i>(library: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    child(library, "packages")
        .map(|p| children(p, "package").collect())
        .unwrap_or_default()
}

/// The one package of a library.
fn library_package<'a, 'i>(library: Node<'a, 'i>) -> Result<Node<'a, 'i>, String> {
    let all = packages(library);
    match all.as_slice() {
        [one] => Ok(*one),
        [] => Err("the library has no packages".into()),
        many => Err(format!(
            "the library has {} packages ({}), and the command takes a library of one: export the package to a library of its own in Eagle, or take it from a board with `--ref`",
            many.len(),
            many.iter()
                .filter_map(|p| p.attribute("name"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The package of the board's element `reference`, and the element's turn on the board.
fn board_package<'a, 'i>(
    board: Node<'a, 'i>,
    reference: &str,
) -> Result<(Node<'a, 'i>, Rot), String> {
    let elements: Vec<Node> = child(board, "elements")
        .map(|e| children(e, "element").collect())
        .unwrap_or_default();
    let found: Vec<&Node> = elements
        .iter()
        .filter(|e| e.attribute("name") == Some(reference))
        .collect();
    let element = match found.as_slice() {
        [one] => **one,
        [] => {
            let mut refs: Vec<&str> = elements
                .iter()
                .filter_map(|e| e.attribute("name"))
                .collect();
            if refs.is_empty() {
                return Err("the board has no parts".into());
            }
            refs.sort_by_key(|r| crate::footprint_import::natural(r));
            refs.dedup();
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
    let library_name = element.attribute("library").unwrap_or_default();
    let library_urn = element.attribute("library_urn");
    let package_name = element.attribute("package").unwrap_or_default();
    let library = child(board, "libraries")
        .into_iter()
        .flat_map(|l| children(l, "library"))
        .find(|l| {
            l.attribute("name") == Some(library_name)
                && (library_urn.is_none() || l.attribute("urn") == library_urn)
        })
        .ok_or_else(|| {
            format!("the board has no library `{library_name}`, which its part `{reference}` names")
        })?;
    let package = packages(library)
        .into_iter()
        .find(|p| p.attribute("name") == Some(package_name))
        .ok_or_else(|| {
            format!(
                "the board's library `{library_name}` has no package `{package_name}`, which its part `{reference}` names"
            )
        })?;
    Ok((package, Rot::parse(element.attribute("rot"))))
}

/// An Eagle turn, `R90`, `MR180`, `SR45`: the angle counter-clockwise, and a mirror.
#[derive(Debug, Clone, Copy, Default)]
struct Rot {
    angle: f64,
    mirror: bool,
}

impl Rot {
    fn parse(text: Option<&str>) -> Rot {
        let Some(text) = text else {
            return Rot::default();
        };
        let letters: String = text
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        Rot {
            angle: text[letters.len()..].parse().unwrap_or(0.0),
            mirror: letters.contains('M'),
        }
    }
}

/// The names of the file's layers, by number.
struct Layers(Vec<(u32, String)>);

impl Layers {
    fn of(drawing: Node) -> Layers {
        Layers(
            child(drawing, "layers")
                .into_iter()
                .flat_map(|l| children(l, "layer"))
                .filter_map(|l| {
                    Some((
                        l.attribute("number")?.parse().ok()?,
                        l.attribute("name")?.to_string(),
                    ))
                })
                .collect(),
        )
    }

    fn name(&self, number: u32) -> String {
        self.0
            .iter()
            .find(|(n, _)| *n == number)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| match number {
                21 => "tPlace".into(),
                22 => "bPlace".into(),
                25 => "tNames".into(),
                27 => "tValues".into(),
                39 => "tKeepout".into(),
                51 => "tDocu".into(),
                52 => "bDocu".into(),
                _ => format!("layer {number}"),
            })
    }

    /// The KiCad layer an Eagle layer's drawings go on, if any.
    fn kicad(&self, number: u32) -> Option<&'static str> {
        Some(match number {
            1 => "F.Cu",
            16 => "B.Cu",
            21 => "F.SilkS",
            22 => "B.SilkS",
            29 => "F.Mask",
            30 => "B.Mask",
            31 => "F.Paste",
            32 => "B.Paste",
            51 => "F.Fab",
            52 => "B.Fab",
            _ => {
                let name = self.name(number).to_lowercase();
                if !(name.contains("courtyard") || name.contains("crtyd")) {
                    return None;
                }
                if name.starts_with('b') {
                    "B.CrtYd"
                } else {
                    "F.CrtYd"
                }
            }
        })
    }
}

/// What the import changed or left out, for the person to check.
#[derive(Default)]
struct Notes {
    /// Drawings left out, by the Eagle layer's name, in the order met.
    left_out: Vec<(String, usize)>,
    octagons: usize,
    curved_polygons: usize,
}

impl Notes {
    fn leave_out(&mut self, layer: String) {
        match self.left_out.iter_mut().find(|(l, _)| *l == layer) {
            Some((_, n)) => *n += 1,
            None => self.left_out.push((layer, 1)),
        }
    }

    fn finish(self) -> Vec<String> {
        let mut out = Vec::new();
        let plural = |n: usize, one: &str, many: &str| {
            if n == 1 {
                format!("1 {one}")
            } else {
                format!("{n} {many}")
            }
        };
        if self.octagons > 0 {
            out.push(format!(
                "{} written round: netsmith reads no octagonal pad",
                plural(self.octagons, "octagonal pad", "octagonal pads")
            ));
        }
        if self.curved_polygons > 0 {
            out.push(format!(
                "{} drawn with straight edges",
                plural(
                    self.curved_polygons,
                    "polygon with curved edges",
                    "polygons with curved edges"
                )
            ));
        }
        for (layer, n) in self.left_out {
            out.push(format!(
                "left out {} on Eagle's layer {layer}, which a KiCad footprint has no layer for",
                plural(n, "drawing", "drawings")
            ));
        }
        out
    }
}

struct Writer<'a> {
    layers: &'a Layers,
    notes: Notes,
}

/// An attribute as a number, 0 when absent or not one.
fn num(node: Node, key: &str) -> f64 {
    node.attribute(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// A length for the file: to a nanometre, as KiCad stores it, and never `-0`.
fn len(v: f64) -> Sexp {
    let r = (v * 1e6).round() / 1e6;
    Sexp::num(if r == 0.0 { 0.0 } else { r })
}

/// `(head x y)` of an Eagle point, its y negated.
fn point(head: &str, x: f64, y: f64) -> Sexp {
    Sexp::node(head, vec![len(x), len(-y)])
}

/// `(at x y angle)`, with no angle when it is 0.
fn at(x: f64, y: f64, angle: f64) -> Sexp {
    let mut items = vec![len(x), len(-y)];
    let angle = angle.rem_euclid(360.0);
    if angle.abs() > 1e-9 && (angle - 360.0).abs() > 1e-9 {
        items.push(len(angle));
    }
    Sexp::node("at", items)
}

fn stroke(width: f64) -> Sexp {
    Sexp::node(
        "stroke",
        vec![
            Sexp::node("width", vec![len(width)]),
            Sexp::node("type", vec![Sexp::atom("solid")]),
        ],
    )
}

fn fill(solid: bool) -> Sexp {
    Sexp::node(
        "fill",
        vec![Sexp::atom(if solid { "solid" } else { "none" })],
    )
}

fn layer(name: &str) -> Sexp {
    Sexp::node("layer", vec![Sexp::string(name)])
}

fn font(size: f64, ratio: f64) -> Sexp {
    Sexp::node(
        "effects",
        vec![Sexp::node(
            "font",
            vec![
                Sexp::node("size", vec![len(size), len(size)]),
                Sexp::node("thickness", vec![len(size * ratio / 100.0)]),
            ],
        )],
    )
}

/// The mid point of the arc from `a` to `b` that sweeps `curve` degrees counter-clockwise (in
/// Eagle's frame, y up).
fn arc_mid(a: (f64, f64), b: (f64, f64), curve: f64) -> (f64, f64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let chord = dx.hypot(dy);
    let half = curve.to_radians() / 2.0;
    // The centre is to the left of a to b for a sweep counter-clockwise, at this distance from
    // the chord's middle.
    let h = (chord / 2.0) / half.tan();
    let (nx, ny) = (-dy / chord, dx / chord);
    let c = ((a.0 + b.0) / 2.0 + nx * h, (a.1 + b.1) / 2.0 + ny * h);
    let (sin, cos) = half.sin_cos();
    let (rx, ry) = (a.0 - c.0, a.1 - c.1);
    (c.0 + rx * cos - ry * sin, c.1 + rx * sin + ry * cos)
}

impl Writer<'_> {
    fn footprint(&mut self, package: Node, name: &str) -> String {
        let mut items = vec![
            Sexp::atom("footprint"),
            Sexp::string(name),
            Sexp::node("version", vec![Sexp::atom("20241229")]),
            Sexp::node("generator", vec![Sexp::string("netsmith")]),
            layer("F.Cu"),
        ];
        if let Some(d) = child(package, "description").and_then(|d| d.text()) {
            let d = d.split_whitespace().collect::<Vec<_>>().join(" ");
            if !d.is_empty() {
                items.push(Sexp::node("descr", vec![Sexp::string(d)]));
            }
        }
        let texts: Vec<Node> = children(package, "text").collect();
        let field = |which: &str| {
            texts.iter().find(|t| {
                t.text()
                    .is_some_and(|s| s.trim().eq_ignore_ascii_case(which))
            })
        };
        items.push(self.field(
            "Reference",
            "REF**",
            field(">NAME").copied(),
            "F.SilkS",
            -2.0,
        ));
        items.push(self.field("Value", name, field(">VALUE").copied(), "F.Fab", 2.0));
        let drilled = package
            .children()
            .any(|c| c.is_element() && c.tag_name().name() == "pad");
        items.push(Sexp::node(
            "attr",
            vec![Sexp::atom(if drilled { "through_hole" } else { "smd" })],
        ));
        let mut pads = Vec::new();
        for node in package.children().filter(Node::is_element) {
            match node.tag_name().name() {
                "smd" => pads.push(self.smd(node)),
                "pad" => pads.push(self.pad(node)),
                "hole" => pads.push(hole(node)),
                "wire" | "circle" | "rectangle" | "polygon" => {
                    if let Some(g) = self.graphic(node) {
                        items.push(g);
                    }
                }
                "text" => {
                    let is_field = node.text().is_some_and(|s| {
                        let s = s.trim();
                        s.eq_ignore_ascii_case(">NAME") || s.eq_ignore_ascii_case(">VALUE")
                    });
                    if !is_field && let Some(t) = self.text(node) {
                        items.push(t);
                    }
                }
                _ => {}
            }
        }
        items.extend(pads);
        Sexp::List(items).to_kicad_string()
    }

    /// The reference or the value, where the package's `>NAME` or `>VALUE` text is, else above
    /// or below the origin.
    fn field(&self, field: &str, value: &str, text: Option<Node>, on: &str, y: f64) -> Sexp {
        let (place, size, ratio) = match text {
            Some(t) => {
                let rot = Rot::parse(t.attribute("rot"));
                (
                    at(num(t, "x"), num(t, "y"), rot.angle),
                    t.attribute("size")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(1.0),
                    t.attribute("ratio")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(8.0),
                )
            }
            None => (at(0.0, -y, 0.0), 1.0, 15.0),
        };
        let layer_name = match text.and_then(|t| t.attribute("layer")) {
            Some("26") | Some("22") if on == "F.SilkS" => "B.SilkS",
            Some("28") | Some("52") if on == "F.Fab" => "B.Fab",
            _ => on,
        };
        let mut effects = font(size, ratio);
        if text.is_some()
            && let Sexp::List(items) = &mut effects
        {
            items.push(Sexp::node(
                "justify",
                vec![Sexp::atom("left"), Sexp::atom("bottom")],
            ));
        }
        Sexp::node(
            "property",
            vec![
                Sexp::string(field),
                Sexp::string(value),
                place,
                layer(layer_name),
                effects,
            ],
        )
    }

    fn smd(&mut self, node: Node) -> Sexp {
        let rot = Rot::parse(node.attribute("rot"));
        let (w, h) = (num(node, "dx"), num(node, "dy"));
        let roundness = num(node, "roundness").clamp(0.0, 100.0);
        let face = if node.attribute("layer") == Some("16") {
            "B"
        } else {
            "F"
        };
        let mut layers = vec![Sexp::string(format!("{face}.Cu"))];
        if node.attribute("stop") != Some("no") {
            layers.push(Sexp::string(format!("{face}.Mask")));
        }
        if node.attribute("cream") != Some("no") {
            layers.push(Sexp::string(format!("{face}.Paste")));
        }
        let mut items = vec![
            Sexp::string(node.attribute("name").unwrap_or_default()),
            Sexp::atom("smd"),
            Sexp::atom(if roundness > 0.0 { "roundrect" } else { "rect" }),
            at(num(node, "x"), num(node, "y"), rot.angle),
            Sexp::node("size", vec![len(w), len(h)]),
            Sexp::node("layers", layers),
        ];
        if roundness > 0.0 {
            items.push(Sexp::node("roundrect_rratio", vec![len(roundness / 200.0)]));
        }
        Sexp::node("pad", items)
    }

    fn pad(&mut self, node: Node) -> Sexp {
        let rot = Rot::parse(node.attribute("rot"));
        let drill = num(node, "drill");
        let given = num(node, "diameter");
        let d = if given > 0.0 {
            given
        } else {
            drill + 2.0 * (drill * 0.25).clamp(0.254, 0.508)
        };
        let shape = node.attribute("shape").unwrap_or("round");
        let (kind, size, offset) = match shape {
            "square" => ("rect", (d, d), 0.0),
            "long" => ("oval", (2.0 * d, d), 0.0),
            "offset" => ("oval", (2.0 * d, d), d / 2.0),
            "octagon" => {
                self.notes.octagons += 1;
                ("circle", (d, d), 0.0)
            }
            _ => ("circle", (d, d), 0.0),
        };
        // Eagle's offset pad reaches out from its hole along +x; KiCad keeps the hole at the
        // pad's place, so the pad moves and the drill is offset back.
        let (x, y) = (num(node, "x"), num(node, "y"));
        let (s, c) = rot.angle.to_radians().sin_cos();
        let (px, py) = (x + offset * c, y + offset * s);
        let mut drill_items = vec![len(drill)];
        if offset != 0.0 {
            drill_items.push(Sexp::node("offset", vec![len(-offset), len(0.0)]));
        }
        let mut layers = vec![Sexp::string("*.Cu")];
        if node.attribute("stop") != Some("no") {
            layers.push(Sexp::string("*.Mask"));
        }
        Sexp::node(
            "pad",
            vec![
                Sexp::string(node.attribute("name").unwrap_or_default()),
                Sexp::atom("thru_hole"),
                Sexp::atom(kind),
                at(px, py, rot.angle),
                Sexp::node("size", vec![len(size.0), len(size.1)]),
                Sexp::node("drill", drill_items),
                Sexp::node("layers", layers),
            ],
        )
    }

    fn graphic(&mut self, node: Node) -> Option<Sexp> {
        let number: u32 = node.attribute("layer")?.parse().ok()?;
        let Some(on) = self.layers.kicad(number) else {
            self.notes.leave_out(self.layers.name(number));
            return None;
        };
        let width = num(node, "width");
        Some(match node.tag_name().name() {
            "wire" => {
                let a = (num(node, "x1"), num(node, "y1"));
                let b = (num(node, "x2"), num(node, "y2"));
                let curve = num(node, "curve");
                if curve.abs() > 1e-9 && a != b {
                    let m = arc_mid(a, b, curve);
                    Sexp::node(
                        "fp_arc",
                        vec![
                            point("start", a.0, a.1),
                            point("mid", m.0, m.1),
                            point("end", b.0, b.1),
                            stroke(width),
                            layer(on),
                        ],
                    )
                } else {
                    Sexp::node(
                        "fp_line",
                        vec![
                            point("start", a.0, a.1),
                            point("end", b.0, b.1),
                            stroke(width),
                            layer(on),
                        ],
                    )
                }
            }
            "circle" => {
                let (x, y, r) = (num(node, "x"), num(node, "y"), num(node, "radius"));
                Sexp::node(
                    "fp_circle",
                    vec![
                        point("center", x, y),
                        point("end", x + r, y),
                        stroke(width),
                        fill(width == 0.0),
                        layer(on),
                    ],
                )
            }
            "rectangle" => {
                let (x1, y1, x2, y2) = (
                    num(node, "x1"),
                    num(node, "y1"),
                    num(node, "x2"),
                    num(node, "y2"),
                );
                let (cx, cy) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
                let (s, c) = Rot::parse(node.attribute("rot"))
                    .angle
                    .to_radians()
                    .sin_cos();
                let corners = [(x1, y1), (x2, y1), (x2, y2), (x1, y2)].map(|(x, y)| {
                    let (dx, dy) = (x - cx, y - cy);
                    point("xy", cx + dx * c - dy * s, cy + dx * s + dy * c)
                });
                Sexp::node(
                    "fp_poly",
                    vec![
                        Sexp::node("pts", corners.to_vec()),
                        // A courtyard is an outline; any other rectangle of Eagle's is filled.
                        stroke(if on.ends_with("CrtYd") { 0.05 } else { 0.0 }),
                        fill(!on.ends_with("CrtYd")),
                        layer(on),
                    ],
                )
            }
            _ => {
                let vertices: Vec<Node> = children(node, "vertex").collect();
                if vertices.len() < 3 {
                    return None;
                }
                if vertices.iter().any(|v| num(*v, "curve") != 0.0) {
                    self.notes.curved_polygons += 1;
                }
                Sexp::node(
                    "fp_poly",
                    vec![
                        Sexp::node(
                            "pts",
                            vertices
                                .iter()
                                .map(|v| point("xy", num(*v, "x"), num(*v, "y")))
                                .collect(),
                        ),
                        stroke(width),
                        fill(true),
                        layer(on),
                    ],
                )
            }
        })
    }

    fn text(&mut self, node: Node) -> Option<Sexp> {
        let number: u32 = node.attribute("layer")?.parse().ok()?;
        let Some(on) = self.layers.kicad(number) else {
            self.notes.leave_out(self.layers.name(number));
            return None;
        };
        let content = node.text()?.trim();
        let rot = Rot::parse(node.attribute("rot"));
        let mut effects = font(
            node.attribute("size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0),
            node.attribute("ratio")
                .and_then(|s| s.parse().ok())
                .unwrap_or(8.0),
        );
        if let Sexp::List(items) = &mut effects {
            items.push(Sexp::node(
                "justify",
                vec![Sexp::atom("left"), Sexp::atom("bottom")],
            ));
        }
        Some(Sexp::node(
            "fp_text",
            vec![
                Sexp::atom("user"),
                Sexp::string(content),
                at(num(node, "x"), num(node, "y"), rot.angle),
                layer(on),
                effects,
            ],
        ))
    }
}

/// A hole with no copper: a pad with no number.
fn hole(node: Node) -> Sexp {
    let d = num(node, "drill");
    Sexp::node(
        "pad",
        vec![
            Sexp::string(""),
            Sexp::atom("np_thru_hole"),
            Sexp::atom("circle"),
            at(num(node, "x"), num(node, "y"), 0.0),
            Sexp::node("size", vec![len(d), len(d)]),
            Sexp::node("drill", vec![len(d)]),
            Sexp::node("layers", vec![Sexp::string("*.Cu"), Sexp::string("*.Mask")]),
        ],
    )
}
