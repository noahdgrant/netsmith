//! Read the SVG netsmith writes into a [`Picture`].
//!
//! What is read: `<svg>` (its `viewBox`, else its `width` and `height`), `<g>`, `<rect>` (with
//! `rx`, `ry`), `<circle>`, `<ellipse>`, `<line>`, `<polyline>`, `<polygon>`, `<path>` (every
//! command of `d`, arcs as curves) and `<text>` with its `<tspan>`s (a `tspan` with its own `x`
//! or `y` starts a new line; one with `text-decoration="overline"` is a run with a line over it).
//! Their style comes, weakest first, from the parent (for the properties that inherit), the
//! presentation attributes, the rules of every `<style>` (see `css`) and the `style`
//! attribute; `var(--name, fallback)` is resolved from the custom properties in scope. Read
//! properties: `fill`, `fill-opacity`, `fill-rule`, `stroke`, `stroke-width`, `stroke-opacity`,
//! `stroke-linecap`, `stroke-linejoin`, `stroke-dasharray`, `opacity` (a group's multiplies its
//! children's), `display: none`, `visibility: hidden`, `vector-effect: non-scaling-stroke` (its
//! width read as CSS pixels at 96 to the inch, a user unit being a millimetre), `font-size`,
//! `font-weight` (600 and up is bold), `font-family` (a monospaced family is [`Face::Mono`]),
//! `text-anchor` and `paint-order` (a text's stroke is its halo). A `transform` of `matrix`,
//! `translate`, `scale`, `rotate` (about a point too), `skewX` or `skewY` is applied to the
//! points; a text keeps the turn and scale of its transform.
//!
//! Left out: `<title>`, `<desc>`, `<style>` (read as the sheet), `<defs>` and what refers to it
//! (`<use>`, clipping, masks, gradients, markers), `<image>`, `<foreignObject>`, and anything
//! that draws nothing (no fill and no stroke, or fully transparent).

use std::collections::HashMap;

use crate::css::{self, Element, Sheet};
use crate::geometry::{self, Affine};
use crate::{
    Anchor, Cap, Face, FillRule, Item, Join, Path, Picture, Pt, Rect, Rgba, Run, Seg, Shape,
    Stroke, Text,
};

/// Why an SVG could not be read.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadError(pub String);

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the SVG cannot be read: {}", self.0)
    }
}

impl std::error::Error for ReadError {}

/// The properties a child takes from its parent when it does not say.
const INHERITED: [&str; 15] = [
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-dasharray",
    "font-size",
    "font-weight",
    "font-family",
    "text-anchor",
    "paint-order",
    "visibility",
];

/// The properties also read from an attribute of the same name.
const PRESENTATION: [&str; 19] = [
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-dasharray",
    "font-size",
    "font-weight",
    "font-family",
    "text-anchor",
    "paint-order",
    "visibility",
    "opacity",
    "display",
    "vector-effect",
    "text-decoration",
];

/// What a CSS pixel is in millimetres.
const PX_MM: f64 = 25.4 / 96.0;

/// Read `text`, an SVG document.
pub fn read(text: &str) -> Result<Picture, ReadError> {
    let doc = roxmltree::Document::parse(text).map_err(|e| ReadError(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err(ReadError(format!(
            "its root is <{}>, not <svg>",
            root.tag_name().name()
        )));
    }
    let mut sheet = Sheet::default();
    for node in root.descendants() {
        if node.is_element() && node.tag_name().name() == "style" {
            let css: String = node
                .children()
                .filter_map(|c| c.text())
                .collect::<Vec<_>>()
                .join("");
            sheet.add(&css);
        }
    }
    let mut reader = Reader {
        sheet,
        items: Vec::new(),
    };
    let mut path = Vec::new();
    reader.visit(root, &HashMap::new(), 1.0, Affine::IDENTITY, &mut path);
    let view = view_box(root).unwrap_or_else(|| {
        reader
            .items
            .iter()
            .filter_map(Item::bounds)
            .reduce(|a, b| a.union(&b))
            .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0))
    });
    Ok(Picture {
        view,
        items: reader.items,
    })
}

fn view_box(root: roxmltree::Node) -> Option<Rect> {
    if let Some(v) = root.attribute("viewBox") {
        let n = numbers(v);
        if n.len() == 4 {
            return Some(Rect::new(n[0], n[1], n[2], n[3]));
        }
    }
    let w = root.attribute("width").and_then(length)?;
    let h = root.attribute("height").and_then(length)?;
    Some(Rect::new(0.0, 0.0, w, h))
}

struct Reader {
    sheet: Sheet,
    items: Vec<Item>,
}

/// An element's style: every property it has, its own and the ones it inherited.
type Props = HashMap<String, String>;

impl Reader {
    fn visit<'a>(
        &mut self,
        node: roxmltree::Node<'a, '_>,
        parent: &Props,
        opacity: f64,
        transform: Affine,
        path: &mut Vec<Element<'a>>,
    ) {
        let tag = node.tag_name().name();
        if matches!(
            tag,
            "title"
                | "desc"
                | "style"
                | "defs"
                | "use"
                | "clipPath"
                | "mask"
                | "symbol"
                | "marker"
                | "linearGradient"
                | "radialGradient"
                | "pattern"
                | "image"
                | "foreignObject"
                | "metadata"
                | "script"
        ) {
            return;
        }
        path.push(Element {
            tag,
            classes: node
                .attribute("class")
                .unwrap_or("")
                .split_whitespace()
                .map(str::to_string)
                .collect(),
        });
        let props = self.style(node, parent, path);
        let display_none = get(&props, "display").as_deref() == Some("none");
        if !display_none {
            let transform = node
                .attribute("transform")
                .map_or(transform, |t| parse_transform(t).then(transform));
            let opacity = opacity
                * get(&props, "opacity")
                    .and_then(|o| number(&o))
                    .unwrap_or(1.0);
            let hidden = matches!(
                get(&props, "visibility").as_deref(),
                Some("hidden" | "collapse")
            );
            match tag {
                "svg" | "g" | "a" | "switch" => {
                    for child in node.children().filter(|c| c.is_element()) {
                        self.visit(child, &props, opacity, transform, path);
                    }
                }
                "text" => {
                    if !hidden {
                        self.text(node, &props, opacity, transform, path);
                    }
                }
                _ => {
                    if !hidden && let Some(shape_path) = shape_path(node) {
                        self.shape(&shape_path, &props, opacity, transform);
                    }
                }
            }
        }
        path.pop();
    }

    /// The style of `node`: its parent's inherited properties, then its presentation
    /// attributes, the sheet's rules and its `style` attribute.
    fn style(&self, node: roxmltree::Node, parent: &Props, path: &[Element]) -> Props {
        let mut props: Props = parent
            .iter()
            .filter(|(k, _)| k.starts_with("--") || INHERITED.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for name in PRESENTATION {
            if let Some(v) = node.attribute(name) {
                props.insert(name.to_string(), v.trim().to_string());
            }
        }
        for (k, v) in self.sheet.matching(path) {
            props.insert(k, v);
        }
        if let Some(style) = node.attribute("style") {
            for (k, v) in css::declarations(style) {
                props.insert(k, v);
            }
        }
        // `inherit` takes the parent's; the rest of CSS's keywords are not used here.
        let inherits: Vec<String> = props
            .iter()
            .filter(|(_, v)| v.as_str() == "inherit")
            .map(|(k, _)| k.clone())
            .collect();
        for k in inherits {
            match parent.get(&k) {
                Some(v) => props.insert(k, v.clone()),
                None => props.remove(&k),
            };
        }
        props
    }

    fn shape(&mut self, path: &Path, props: &Props, opacity: f64, transform: Affine) {
        let fill = paint(props, "fill", Some(Rgba::BLACK))
            .map(|c| c.faded(opacity_of(props, "fill-opacity") * opacity))
            .filter(|c| c.a > 0)
            .map(|c| {
                let rule = match get(props, "fill-rule").as_deref() {
                    Some("evenodd") => FillRule::EvenOdd,
                    _ => FillRule::NonZero,
                };
                (c, rule)
            });
        let stroke = self.stroke(props, opacity, transform);
        if fill.is_none() && stroke.is_none() {
            return;
        }
        self.items.push(Item::Shape(Shape {
            path: transform.path(path),
            fill,
            stroke,
        }));
    }

    fn stroke(&self, props: &Props, opacity: f64, transform: Affine) -> Option<Stroke> {
        let colour =
            paint(props, "stroke", None)?.faded(opacity_of(props, "stroke-opacity") * opacity);
        let width = get(props, "stroke-width")
            .and_then(|w| length(&w))
            .unwrap_or(1.0);
        let fixed = get(props, "vector-effect").as_deref() == Some("non-scaling-stroke");
        let scale = if fixed {
            PX_MM
        } else {
            transform.scale_factor()
        };
        let width = width * scale;
        if colour.a == 0 || width <= 0.0 {
            return None;
        }
        let cap = match get(props, "stroke-linecap").as_deref() {
            Some("round") => Cap::Round,
            Some("square") => Cap::Square,
            _ => Cap::Butt,
        };
        let join = match get(props, "stroke-linejoin").as_deref() {
            Some("round") => Join::Round,
            Some("bevel") => Join::Bevel,
            _ => Join::Miter,
        };
        let mut dash: Vec<f64> = match get(props, "stroke-dasharray") {
            Some(d) if d != "none" => numbers(&d).iter().map(|v| v * scale).collect(),
            _ => Vec::new(),
        };
        if dash.len() % 2 == 1 {
            dash.extend(dash.clone());
        }
        if dash.iter().all(|d| *d <= 0.0) {
            dash.clear();
        }
        Some(Stroke {
            colour,
            width,
            cap,
            join,
            dash,
        })
    }

    fn text<'a>(
        &mut self,
        node: roxmltree::Node<'a, '_>,
        props: &Props,
        opacity: f64,
        transform: Affine,
        path: &mut Vec<Element<'a>>,
    ) {
        let preserve =
            node.attribute(("http://www.w3.org/XML/1998/namespace", "space")) == Some("preserve");
        let first = |name: &str| {
            node.attribute(name)
                .and_then(|v| numbers(v).first().copied())
        };
        let mut lines: Vec<Line> = vec![Line {
            at: Pt::new(first("x").unwrap_or(0.0), first("y").unwrap_or(0.0)),
            props: props.clone(),
            runs: Vec::new(),
        }];
        for child in node.children() {
            if let Some(t) = child.text().filter(|_| child.is_text()) {
                push_run(&mut lines, t, false);
            } else if child.is_element() && child.tag_name().name() == "tspan" {
                path.push(Element {
                    tag: "tspan",
                    classes: child
                        .attribute("class")
                        .unwrap_or("")
                        .split_whitespace()
                        .map(str::to_string)
                        .collect(),
                });
                let tprops = self.style(child, props, path);
                path.pop();
                let x = child
                    .attribute("x")
                    .and_then(|v| numbers(v).first().copied());
                let y = child
                    .attribute("y")
                    .and_then(|v| numbers(v).first().copied());
                if x.is_some() || y.is_some() {
                    let last = lines.last().expect("a line").at;
                    lines.push(Line {
                        at: Pt::new(x.unwrap_or(last.x), y.unwrap_or(last.y)),
                        props: tprops.clone(),
                        runs: Vec::new(),
                    });
                }
                let overline = get(&tprops, "text-decoration")
                    .is_some_and(|d| d.split_whitespace().any(|w| w == "overline"));
                let content: String = child
                    .descendants()
                    .filter(|d| d.is_text())
                    .filter_map(|d| d.text())
                    .collect();
                push_run(&mut lines, &content, overline);
            }
        }
        for line in lines {
            let mut runs = line.runs;
            tidy(&mut runs, preserve);
            if runs.iter().all(|r| r.text.is_empty()) {
                continue;
            }
            let p = &line.props;
            let size = get(p, "font-size").and_then(|s| length(&s)).unwrap_or(16.0)
                * transform.scale_factor();
            let anchor = match get(p, "text-anchor").as_deref() {
                Some("middle") => Anchor::Middle,
                Some("end") => Anchor::End,
                _ => Anchor::Start,
            };
            let bold = get(p, "font-weight").is_some_and(|w| match w.as_str() {
                "bold" | "bolder" => true,
                w => number(w).is_some_and(|n| n >= 600.0),
            });
            let mono = get(p, "font-family").is_some_and(|f| {
                let f = f.to_ascii_lowercase();
                ["mono", "courier", "consolas", "menlo"]
                    .iter()
                    .any(|m| f.contains(m))
            });
            let face = match (mono, bold) {
                (false, false) => Face::Sans,
                (false, true) => Face::SansBold,
                (true, false) => Face::Mono,
                (true, true) => Face::MonoBold,
            };
            let fill = paint(p, "fill", Some(Rgba::BLACK))
                .map(|c| c.faded(opacity_of(p, "fill-opacity") * opacity))
                .filter(|c| c.a > 0);
            let halo = self
                .stroke(p, opacity, transform)
                .map(|s| (s.colour, s.width));
            if fill.is_none() && halo.is_none() {
                continue;
            }
            self.items.push(Item::Text(Text {
                at: transform.apply(line.at),
                angle: transform.angle(),
                size,
                anchor,
                face,
                fill,
                halo,
                runs,
            }));
        }
    }
}

struct Line {
    at: Pt,
    props: Props,
    runs: Vec<Run>,
}

fn push_run(lines: &mut [Line], text: &str, overline: bool) {
    let line = lines.last_mut().expect("a line");
    line.runs.push(Run {
        text: text.to_string(),
        overline,
    });
}

/// The white space of a line's runs as SVG draws it: with `xml:space="preserve"` each newline
/// and tab is a space; without, they go, runs of spaces are one, and the line is trimmed.
fn tidy(runs: &mut Vec<Run>, preserve: bool) {
    for run in runs.iter_mut() {
        if preserve {
            run.text = run.text.replace(['\n', '\r', '\t'], " ");
        } else {
            let mut out = String::with_capacity(run.text.len());
            let mut space = false;
            for c in run.text.chars() {
                if c == '\n' || c == '\r' {
                    continue;
                }
                if c == ' ' || c == '\t' {
                    if !space {
                        out.push(' ');
                    }
                    space = true;
                } else {
                    out.push(c);
                    space = false;
                }
            }
            run.text = out;
        }
    }
    if !preserve {
        if let Some(first) = runs.first_mut() {
            first.text = first.text.trim_start().to_string();
        }
        if let Some(last) = runs.last_mut() {
            last.text = last.text.trim_end().to_string();
        }
    }
    runs.retain(|r| !r.text.is_empty());
}

/// A property's value with each `var(--name, fallback)` replaced.
fn get(props: &Props, name: &str) -> Option<String> {
    props.get(name).map(|v| resolve(props, v, 0))
}

fn resolve(props: &Props, value: &str, depth: usize) -> String {
    let Some(start) = value.find("var(") else {
        return value.to_string();
    };
    if depth > 16 {
        return value.to_string();
    }
    // The matching parenthesis.
    let inner_start = start + 4;
    let mut level = 1;
    let mut end = None;
    for (i, c) in value[inner_start..].char_indices() {
        match c {
            '(' => level += 1,
            ')' => {
                level -= 1;
                if level == 0 {
                    end = Some(inner_start + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(end) = end else {
        return value.to_string();
    };
    let inner = &value[inner_start..end];
    let (name, fallback) = match inner.split_once(',') {
        Some((n, f)) => (n.trim(), Some(f.trim())),
        None => (inner.trim(), None),
    };
    let replaced = match props.get(name) {
        Some(v) => resolve(props, v, depth + 1),
        None => fallback.map_or(String::new(), |f| resolve(props, f, depth + 1)),
    };
    let rest = resolve(props, &value[end + 1..], depth + 1);
    format!("{}{}{}", &value[..start], replaced, rest)
}

fn opacity_of(props: &Props, name: &str) -> f64 {
    get(props, name)
        .and_then(|v| {
            let v = v.trim();
            match v.strip_suffix('%') {
                Some(p) => number(p).map(|n| n / 100.0),
                None => number(v),
            }
        })
        .unwrap_or(1.0)
        .clamp(0.0, 1.0)
}

/// The colour `name` paints with: `None` for `none` or `transparent`, `default` when it says
/// nothing or a colour that is not read.
fn paint(props: &Props, name: &str, default: Option<Rgba>) -> Option<Rgba> {
    match get(props, name) {
        None => default,
        Some(v) => match colour(&v) {
            Some(c) => c,
            None => default,
        },
    }
}

/// A CSS colour: `Some(None)` is no paint, `None` a colour that is not read.
pub(crate) fn colour(value: &str) -> Option<Option<Rgba>> {
    let v = value.trim().to_ascii_lowercase();
    if v == "none" || v == "transparent" {
        return Some(None);
    }
    if let Some(hex) = v.strip_prefix('#') {
        let digit = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|d| d * 17);
        let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        return match hex.len() {
            3 => Some(Some(Rgba::opaque(digit(0)?, digit(1)?, digit(2)?))),
            4 => Some(Some(Rgba {
                r: digit(0)?,
                g: digit(1)?,
                b: digit(2)?,
                a: digit(3)?,
            })),
            6 => Some(Some(Rgba::opaque(pair(0)?, pair(2)?, pair(4)?))),
            8 => Some(Some(Rgba {
                r: pair(0)?,
                g: pair(2)?,
                b: pair(4)?,
                a: pair(6)?,
            })),
            _ => None,
        };
    }
    if let Some(args) = v
        .strip_prefix("rgba(")
        .or_else(|| v.strip_prefix("rgb("))
        .and_then(|a| a.strip_suffix(')'))
    {
        let parts: Vec<&str> = args
            .split([',', ' ', '/'])
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() < 3 {
            return None;
        }
        let channel = |p: &str| -> Option<u8> {
            match p.strip_suffix('%') {
                Some(pc) => Some((number(pc)? * 2.55).round().clamp(0.0, 255.0) as u8),
                None => Some(number(p)?.round().clamp(0.0, 255.0) as u8),
            }
        };
        let alpha = match parts.get(3) {
            Some(a) => match a.strip_suffix('%') {
                Some(pc) => number(pc)? / 100.0,
                None => number(a)?,
            },
            None => 1.0,
        };
        return Some(Some(Rgba {
            r: channel(parts[0])?,
            g: channel(parts[1])?,
            b: channel(parts[2])?,
            a: (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
        }));
    }
    let named = match v.as_str() {
        "black" => Rgba::opaque(0, 0, 0),
        "white" => Rgba::opaque(255, 255, 255),
        "red" => Rgba::opaque(255, 0, 0),
        "green" => Rgba::opaque(0, 128, 0),
        "blue" => Rgba::opaque(0, 0, 255),
        "yellow" => Rgba::opaque(255, 255, 0),
        "orange" => Rgba::opaque(255, 165, 0),
        "gray" | "grey" => Rgba::opaque(128, 128, 128),
        "silver" => Rgba::opaque(192, 192, 192),
        "magenta" | "fuchsia" => Rgba::opaque(255, 0, 255),
        "cyan" | "aqua" => Rgba::opaque(0, 255, 255),
        _ => return None,
    };
    Some(Some(named))
}

/// The geometry of a shape element, in its own coordinates.
fn shape_path(node: roxmltree::Node) -> Option<Path> {
    let num = |name: &str| node.attribute(name).and_then(length).unwrap_or(0.0);
    match node.tag_name().name() {
        "rect" => {
            let (w, h) = (num("width"), num("height"));
            if w <= 0.0 || h <= 0.0 {
                return None;
            }
            let rx = node.attribute("rx").and_then(length);
            let ry = node.attribute("ry").and_then(length);
            let (rx, ry) = (rx.or(ry).unwrap_or(0.0), ry.or(rx).unwrap_or(0.0));
            Some(geometry::rounded_rect(num("x"), num("y"), w, h, rx, ry))
        }
        "circle" => {
            let r = num("r");
            (r > 0.0).then(|| geometry::ellipse(Pt::new(num("cx"), num("cy")), r, r))
        }
        "ellipse" => {
            let (rx, ry) = (num("rx"), num("ry"));
            (rx > 0.0 && ry > 0.0).then(|| geometry::ellipse(Pt::new(num("cx"), num("cy")), rx, ry))
        }
        "line" => Some(Path {
            segs: vec![
                Seg::Move(Pt::new(num("x1"), num("y1"))),
                Seg::Line(Pt::new(num("x2"), num("y2"))),
            ],
        }),
        tag @ ("polyline" | "polygon") => {
            let n = numbers(node.attribute("points").unwrap_or(""));
            let points: Vec<Pt> = n
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| Pt::new(c[0], c[1]))
                .collect();
            let (first, rest) = points.split_first()?;
            let mut segs = vec![Seg::Move(*first)];
            segs.extend(rest.iter().map(|p| Seg::Line(*p)));
            if tag == "polygon" {
                segs.push(Seg::Close);
            }
            Some(Path { segs })
        }
        "path" => {
            let p = path_data(node.attribute("d").unwrap_or(""));
            (!p.segs.is_empty()).then_some(p)
        }
        _ => None,
    }
}

/// The commands of a path's `d`.
pub(crate) fn path_data(d: &str) -> Path {
    let tokens = tokens(d);
    let mut segs = Vec::new();
    let (mut i, mut cmd) = (0usize, ' ');
    let mut cur = Pt::new(0.0, 0.0);
    let mut start = cur;
    // The last control point, for the smooth curves.
    let mut last_cubic: Option<Pt> = None;
    let mut last_quad: Option<Pt> = None;
    let num = |i: &mut usize| -> Option<f64> {
        match tokens.get(*i) {
            Some(Token::Num(n)) => {
                *i += 1;
                Some(*n)
            }
            _ => None,
        }
    };
    let flag = |i: &mut usize| -> Option<bool> {
        match tokens.get(*i) {
            Some(Token::Num(n)) => {
                *i += 1;
                Some(*n != 0.0)
            }
            _ => None,
        }
    };
    while i < tokens.len() {
        if let Token::Cmd(c) = tokens[i] {
            cmd = c;
            i += 1;
            if c == 'Z' || c == 'z' {
                segs.push(Seg::Close);
                cur = start;
                last_cubic = None;
                last_quad = None;
                continue;
            }
        } else if cmd == ' ' {
            break;
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { cur } else { Pt::new(0.0, 0.0) };
        let at = |x: f64, y: f64| Pt::new(base.x + x, base.y + y);
        let before = i;
        let (mut cubic, mut quad) = (None, None);
        match cmd.to_ascii_uppercase() {
            'M' => {
                let (Some(x), Some(y)) = (num(&mut i), num(&mut i)) else {
                    break;
                };
                cur = at(x, y);
                start = cur;
                segs.push(Seg::Move(cur));
                // Pairs after the first are lines.
                cmd = if rel { 'l' } else { 'L' };
            }
            'L' => {
                let (Some(x), Some(y)) = (num(&mut i), num(&mut i)) else {
                    break;
                };
                cur = at(x, y);
                segs.push(Seg::Line(cur));
            }
            'H' => {
                let Some(x) = num(&mut i) else { break };
                cur = Pt::new(if rel { cur.x + x } else { x }, cur.y);
                segs.push(Seg::Line(cur));
            }
            'V' => {
                let Some(y) = num(&mut i) else { break };
                cur = Pt::new(cur.x, if rel { cur.y + y } else { y });
                segs.push(Seg::Line(cur));
            }
            'C' => {
                let v: Option<Vec<f64>> = (0..6).map(|_| num(&mut i)).collect();
                let Some(v) = v else { break };
                let (a, b, c) = (at(v[0], v[1]), at(v[2], v[3]), at(v[4], v[5]));
                segs.push(Seg::Cubic(a, b, c));
                cur = c;
                cubic = Some(b);
            }
            'S' => {
                let v: Option<Vec<f64>> = (0..4).map(|_| num(&mut i)).collect();
                let Some(v) = v else { break };
                let a = last_cubic.map_or(cur, |p| Pt::new(2.0 * cur.x - p.x, 2.0 * cur.y - p.y));
                let (b, c) = (at(v[0], v[1]), at(v[2], v[3]));
                segs.push(Seg::Cubic(a, b, c));
                cur = c;
                cubic = Some(b);
            }
            'Q' | 'T' => {
                let smooth = cmd.eq_ignore_ascii_case(&'T');
                let (q, end) = if smooth {
                    let (Some(x), Some(y)) = (num(&mut i), num(&mut i)) else {
                        break;
                    };
                    let q =
                        last_quad.map_or(cur, |p| Pt::new(2.0 * cur.x - p.x, 2.0 * cur.y - p.y));
                    (q, at(x, y))
                } else {
                    let v: Option<Vec<f64>> = (0..4).map(|_| num(&mut i)).collect();
                    let Some(v) = v else { break };
                    (at(v[0], v[1]), at(v[2], v[3]))
                };
                let a = Pt::new(
                    cur.x + 2.0 / 3.0 * (q.x - cur.x),
                    cur.y + 2.0 / 3.0 * (q.y - cur.y),
                );
                let b = Pt::new(
                    end.x + 2.0 / 3.0 * (q.x - end.x),
                    end.y + 2.0 / 3.0 * (q.y - end.y),
                );
                segs.push(Seg::Cubic(a, b, end));
                cur = end;
                quad = Some(q);
            }
            'A' => {
                let (Some(rx), Some(ry), Some(rot), Some(large), Some(sweep), Some(x), Some(y)) = (
                    num(&mut i),
                    num(&mut i),
                    num(&mut i),
                    flag(&mut i),
                    flag(&mut i),
                    num(&mut i),
                    num(&mut i),
                ) else {
                    break;
                };
                let end = at(x, y);
                segs.extend(geometry::svg_arc(cur, rx, ry, rot, large, sweep, end));
                cur = end;
            }
            _ => break,
        }
        last_cubic = cubic;
        last_quad = quad;
        if i == before {
            break;
        }
    }
    Path { segs }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Cmd(char),
    Num(f64),
}

fn tokens(d: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let b = d.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            out.push(Token::Cmd(c));
            i += 1;
        } else if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() {
            let start = i;
            i += 1;
            let mut dot = c == '.';
            while i < b.len() {
                let d = b[i] as char;
                if d.is_ascii_digit() {
                    i += 1;
                } else if d == '.' && !dot {
                    dot = true;
                    i += 1;
                } else if (d == 'e' || d == 'E')
                    && i + 1 < b.len()
                    && (b[i + 1].is_ascii_digit() || b[i + 1] == b'-' || b[i + 1] == b'+')
                {
                    i += 2;
                } else {
                    break;
                }
            }
            match d[start..i].parse::<f64>() {
                Ok(n) => out.push(Token::Num(n)),
                Err(_) => break,
            }
        } else {
            i += 1;
        }
    }
    out
}

/// The numbers of a list (`"1,2 3"`, `"1 -2"`).
pub(crate) fn numbers(text: &str) -> Vec<f64> {
    tokens(text)
        .into_iter()
        .filter_map(|t| match t {
            Token::Num(n) => Some(n),
            Token::Cmd(_) => None,
        })
        .collect()
}

fn number(text: &str) -> Option<f64> {
    text.trim().parse().ok()
}

/// A length: its number, whatever unit follows (`px`, a user unit, is the only one used).
fn length(text: &str) -> Option<f64> {
    let t = text.trim();
    let end = t
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
        .unwrap_or(t.len());
    // `1e` would be a number cut short; a unit starting with `e` (`em`, `ex`) is not used.
    t[..end].parse().ok()
}

fn parse_transform(text: &str) -> Affine {
    let mut out = Affine::IDENTITY;
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let v = numbers(&rest[open + 1..open + close]);
        rest = &rest[open + close + 1..];
        let m = match (name, v.as_slice()) {
            ("matrix", [a, b, c, d, e, f]) => Affine([*a, *b, *c, *d, *e, *f]),
            ("translate", [x]) => Affine::translate(*x, 0.0),
            ("translate", [x, y]) => Affine::translate(*x, *y),
            ("scale", [s]) => Affine::scale(*s, *s),
            ("scale", [x, y]) => Affine::scale(*x, *y),
            ("rotate", [a]) => Affine::rotate(*a),
            ("rotate", [a, x, y]) => Affine::translate(-x, -y)
                .then(Affine::rotate(*a))
                .then(Affine::translate(*x, *y)),
            ("skewX", [a]) => Affine([1.0, 0.0, a.to_radians().tan(), 1.0, 0.0, 0.0]),
            ("skewY", [a]) => Affine([1.0, a.to_radians().tan(), 0.0, 1.0, 0.0, 0.0]),
            _ => Affine::IDENTITY,
        };
        // The transforms of a list apply right to left: the last moves a point first.
        out = m.then(out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rotation_about_a_point_keeps_the_point() {
        let t = parse_transform("rotate(-90 10 5)");
        let p = t.apply(Pt::new(10.0, 5.0));
        assert!((p.x - 10.0).abs() < 1e-12 && (p.y - 5.0).abs() < 1e-12);
        assert!((t.angle() + 90.0).abs() < 1e-9);
        // Translate then scale: a point is scaled first.
        let t = parse_transform("translate(10,0) scale(2)");
        assert_eq!(t.apply(Pt::new(1.0, 1.0)), Pt::new(12.0, 2.0));
    }

    #[test]
    fn colours() {
        assert_eq!(
            colour("#0a7a3c0a"),
            Some(Some(Rgba {
                r: 10,
                g: 122,
                b: 60,
                a: 10
            }))
        );
        assert_eq!(colour("#fff"), Some(Some(Rgba::WHITE)));
        assert_eq!(colour("transparent"), Some(None));
        assert_eq!(
            colour("rgba(255, 0, 0, 0.5)"),
            Some(Some(Rgba {
                r: 255,
                g: 0,
                b: 0,
                a: 128
            }))
        );
        assert_eq!(colour("currentColor"), None);
    }

    #[test]
    fn path_commands() {
        let p = path_data("M1 2 L3 4 h2 v-1 Z m1,1 l1-1");
        assert_eq!(
            p.segs,
            vec![
                Seg::Move(Pt::new(1.0, 2.0)),
                Seg::Line(Pt::new(3.0, 4.0)),
                Seg::Line(Pt::new(5.0, 4.0)),
                Seg::Line(Pt::new(5.0, 3.0)),
                Seg::Close,
                Seg::Move(Pt::new(2.0, 3.0)),
                Seg::Line(Pt::new(3.0, 2.0)),
            ]
        );
        assert_eq!(numbers("1e-3,2.5.5"), vec![0.001, 2.5, 0.5]);
    }
}
