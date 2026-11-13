//! Draw a board as an SVG, with no help from KiCad: the outline, each footprint's fab,
//! courtyard and silkscreen drawings, pads coloured by net, reference text, and the
//! ratsnest on top, with a key of the nets underneath (or none, for a page that lists them). An inner plane is drawn as a dashed
//! border inside the outline in its net's colour, and has a line in the key.
//!
//! One millimetre is one unit of the drawing, and y grows downward, as on the board.
//!
//! Each kind of drawing sits in a group of its own, `<g class="l-...">`, so a page can show
//! and hide them: `l-outline`, `l-planes`, `l-fab`, `l-courtyard`, `l-silk`, `l-pads`,
//! `l-back-copper`, `l-front-copper`, `l-vias`, `l-ratsnest`, `l-refs` and `l-key`.
//!
//! What a page needs to pick things out is in data attributes: a part's group has `data-ref`
//! and `data-path` (its path in the design), a pad `data-pad` (its number) and, if it has a
//! net, `data-net`; a ratsnest line `data-net`, `data-from` and `data-to` (the references of
//! the parts at its ends), a keepout `data-keepout` and a cutout `data-cutout`.

use std::fmt::Write as _;

use netsmith_kicad::sexp::fmt_num;

use crate::board::{Board, CopperLayer, Placed};
use crate::footprint::{Graphic, Layer, Pad, PadShape, Shape};
use crate::geometry::Pt;
use crate::ratsnest::Ratsnest;

/// Pixels per millimetre in the size the file asks for.
const SCALE: f64 = 30.0;
/// Space round the board, and above it for the title, in mm.
const MARGIN: f64 = 3.0;
const TITLE_HEIGHT: f64 = 3.0;
const KEY_LINE: f64 = 2.0;
const KEY_TEXT: f64 = 1.2;
/// Width of one character of the monospace key text, as a fraction of its size.
const CHARACTER: f64 = 0.62;

const BACKGROUND: &str = "#0d1117";
const OUTLINE: &str = "#f2ed7d";
/// The colour of a keepout: an area where something is not allowed.
const KEEPOUT: &str = "#e8505b";
/// The colour of an area that only limits how tall a part may stand (ADR 0045, C4).
const HEIGHT_LIMIT: &str = "#e0a030";
const COURTYARD: &str = "#ff26e2";
const FAB: &str = "#5f7a99";
const SILK: &str = "#f0ead6";
const RATSNEST: &str = "#ffffff";
const UNCONNECTED: &str = "#8b949e";
const KEY: &str = "#c9d1d9";
const FRONT_COPPER: &str = "#d63c3c";
const BACK_COPPER: &str = "#3c78d6";
const VIA_COPPER: &str = "#c8c8c8";
/// Pad number text, dark so it reads on any net colour.
const PAD_TEXT: &str = "#0d1117";

/// How far apart the borders of two planes are drawn, in mm.
const PLANE_STEP: f64 = 0.35;

/// Net colours, taken in order and repeated if there are more nets than colours.
const PALETTE: [&str; 10] = [
    "#ff6b6b", "#4dd4ac", "#ffd166", "#6ea8fe", "#c792ea", "#ff9f43", "#7bed9f", "#f78fb3",
    "#74c0fc", "#d4e157",
];

fn n(v: f64) -> String {
    fmt_num(v)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Where a net is in the board's list, which is what picks its colour.
fn net_index(board: &Board, name: &str) -> usize {
    board
        .nets
        .iter()
        .position(|net| net.name == name)
        .unwrap_or(0)
}

fn net_colour(index: usize) -> &'static str {
    PALETTE[index % PALETTE.len()]
}

/// The path of an arc through three points, or `None` if they are in a line.
fn arc_path(start: Pt, mid: Pt, end: Pt) -> Option<String> {
    let d =
        2.0 * (start.x * (mid.y - end.y) + mid.x * (end.y - start.y) + end.x * (start.y - mid.y));
    if d.abs() < 1e-9 {
        return None;
    }
    let sq = |p: Pt| p.x * p.x + p.y * p.y;
    let centre = Pt::new(
        (sq(start) * (mid.y - end.y) + sq(mid) * (end.y - start.y) + sq(end) * (start.y - mid.y))
            / d,
        (sq(start) * (end.x - mid.x) + sq(mid) * (start.x - end.x) + sq(end) * (mid.x - start.x))
            / d,
    );
    let radius = centre.distance(start);
    // With y downward, a positive turn from start through mid to end runs clockwise on
    // screen, which is the direction SVG calls sweep 1.
    let cross = (mid.x - start.x) * (end.y - mid.y) - (mid.y - start.y) * (end.x - mid.x);
    let sweep = cross > 0.0;
    let angle = |p: Pt| (p.y - centre.y).atan2(p.x - centre.x);
    let tau = std::f64::consts::TAU;
    let span = if sweep {
        (angle(end) - angle(start)).rem_euclid(tau)
    } else {
        (angle(start) - angle(end)).rem_euclid(tau)
    };
    Some(format!(
        "M{} {} A{} {} 0 {} {} {} {}",
        n(start.x),
        n(start.y),
        n(radius),
        n(radius),
        u8::from(span > std::f64::consts::PI),
        u8::from(sweep),
        n(end.x),
        n(end.y)
    ))
}

fn graphic(out: &mut String, g: &Graphic, colour: &str) {
    let fill = if g.filled { colour } else { "none" };
    let width = n(g.stroke.max(0.05));
    match &g.shape {
        Shape::Line { a, b } => {
            let _ = writeln!(
                out,
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{colour}" stroke-width="{width}" stroke-linecap="round"/>"#,
                n(a.x),
                n(a.y),
                n(b.x),
                n(b.y)
            );
        }
        Shape::Rect { a, b } => {
            let _ = writeln!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{fill}" stroke="{colour}" stroke-width="{width}"/>"#,
                n(a.x.min(b.x)),
                n(a.y.min(b.y)),
                n((a.x - b.x).abs()),
                n((a.y - b.y).abs())
            );
        }
        Shape::Circle { centre, radius } => {
            let _ = writeln!(
                out,
                r#"<circle cx="{}" cy="{}" r="{}" fill="{fill}" stroke="{colour}" stroke-width="{width}"/>"#,
                n(centre.x),
                n(centre.y),
                n(*radius)
            );
        }
        Shape::Arc { start, mid, end } => {
            let path = arc_path(*start, *mid, *end).unwrap_or_else(|| {
                format!("M{} {} L{} {}", n(start.x), n(start.y), n(end.x), n(end.y))
            });
            let _ = writeln!(
                out,
                r#"<path d="{path}" fill="none" stroke="{colour}" stroke-width="{width}" stroke-linecap="round"/>"#
            );
        }
        Shape::Poly { points } => {
            let points: Vec<String> = points
                .iter()
                .map(|p| format!("{},{}", n(p.x), n(p.y)))
                .collect();
            let _ = writeln!(
                out,
                r#"<polygon points="{}" fill="{fill}" stroke="{colour}" stroke-width="{width}" stroke-linejoin="round"/>"#,
                points.join(" ")
            );
        }
    }
}

/// A pad, centred on the origin of its own group.
fn pad_shape(out: &mut String, pad: &Pad, fill: &str) {
    let (w, h) = (pad.width, pad.height);
    match pad.shape {
        PadShape::Circle => {
            let _ = writeln!(out, r#"<circle r="{}" fill="{fill}"/>"#, n(w / 2.0));
        }
        PadShape::Rect | PadShape::RoundRect | PadShape::Oval => {
            let radius = match pad.shape {
                PadShape::Rect => 0.0,
                PadShape::RoundRect => pad.corner_ratio * w.min(h),
                _ => w.min(h) / 2.0,
            };
            let _ = writeln!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{fill}"/>"#,
                n(-w / 2.0),
                n(-h / 2.0),
                n(w),
                n(h),
                n(radius)
            );
        }
    }
    if let Some(drill) = pad.drill {
        if drill.width == drill.height {
            let _ = writeln!(
                out,
                r#"<circle r="{}" fill="{BACKGROUND}"/>"#,
                n(drill.width / 2.0)
            );
        } else {
            let _ = writeln!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{BACKGROUND}"/>"#,
                n(-drill.width / 2.0),
                n(-drill.height / 2.0),
                n(drill.width),
                n(drill.height),
                n(drill.width.min(drill.height) / 2.0)
            );
        }
    }
}

/// The colour of a pad and the name of its net, if it has one.
type PadNet = (String, Option<String>);

fn part(out: &mut String, placed: &Placed, colours: &dyn Fn(&str, &str) -> PadNet) {
    let mut transform = format!("translate({} {})", n(placed.at.x), n(placed.at.y));
    if placed.rotation != 0.0 {
        let _ = write!(transform, " rotate({})", n(-placed.rotation));
    }
    if placed.mirrored() {
        // A part on the back is the footprint mirrored in its own frame, then turned.
        transform.push_str(" scale(-1 1)");
    }
    let _ = writeln!(
        out,
        r#"<g id="{}" data-ref="{}" data-path="{}" transform="{transform}">"#,
        escape(&placed.refdes),
        escape(&placed.refdes),
        escape(&placed.path)
    );
    let fp = &placed.footprint;
    for (layer, colour) in [
        (Layer::Fab, FAB),
        (Layer::Courtyard, COURTYARD),
        (Layer::Silk, SILK),
    ] {
        let _ = writeln!(out, r#"<g class="l-{}">"#, layer_class(layer));
        for g in fp.graphics.iter().filter(|g| g.layer == layer) {
            graphic(out, g, colour);
        }
        let _ = writeln!(out, "</g>");
    }
    let _ = writeln!(out, r#"<g class="l-pads">"#);
    for pad in &fp.pads {
        let (fill, net) = colours(&placed.refdes, &pad.number);
        let mut at = format!("translate({} {})", n(pad.at.x), n(pad.at.y));
        if pad.angle != 0.0 {
            let _ = write!(at, " rotate({})", n(-pad.angle));
        }
        let number = escape(&pad.number);
        let _ = match net {
            Some(net) => writeln!(
                out,
                r#"<g transform="{at}" data-pad="{number}" data-net="{}">"#,
                escape(&net)
            ),
            None => writeln!(out, r#"<g transform="{at}" data-pad="{number}">"#),
        };
        pad_shape(out, pad, &fill);
        let _ = writeln!(out, "</g>");
        let side = pad.width.min(pad.height);
        if side >= 0.5 {
            // The number is read from the front, so on the back it is mirrored back.
            let flip = if placed.mirrored() {
                format!(
                    r#" transform="translate({} 0) scale(-1 1)" "#,
                    n(2.0 * pad.at.x)
                )
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                r#"<text{flip} x="{}" y="{}" font-size="{}" fill="{PAD_TEXT}" text-anchor="middle" dominant-baseline="central" font-family="monospace">{}</text>"#,
                n(pad.at.x),
                n(pad.at.y),
                n((side * 0.6).min(1.0)),
                escape(&pad.number)
            );
        }
    }
    let _ = writeln!(out, "</g>");
    let _ = writeln!(out, "</g>");
}

/// The group a footprint's drawings of one layer are in.
fn layer_class(layer: Layer) -> &'static str {
    match layer {
        Layer::Fab => "fab",
        Layer::Courtyard | Layer::BackCourtyard => "courtyard",
        Layer::Silk => "silk",
    }
}

/// The board as the text of an SVG file, ratsnest included, with a title above it and a key of
/// the nets underneath.
pub fn render(board: &Board) -> String {
    draw(board, true)
}

/// The board alone, for a page that has its own list of nets: no title and no key. Every pad,
/// track, via and ratsnest line carries `data-net` with its net's name, and every part
/// `data-ref`, so the page can pick them out.
pub fn render_view(board: &Board) -> String {
    draw(board, false)
}

fn draw(board: &Board, with_key: bool) -> String {
    let ratsnest = Ratsnest::of(board);
    let colours = |refdes: &str, number: &str| -> (String, Option<String>) {
        let net = board.nets.iter().position(|net| {
            net.pads
                .iter()
                .any(|p| p.refdes == refdes && p.number == number)
        });
        (
            net.map_or(UNCONNECTED, net_colour).to_string(),
            net.map(|i| board.nets[i].name.clone()),
        )
    };

    let title = format!(
        "{}  {} x {} mm  {} parts  ratsnest {} lines {:.2} mm",
        board.name,
        n(board.outline.width()),
        n(board.outline.height()),
        board.parts.len(),
        ratsnest.len(),
        ratsnest.total_length()
    );
    let text_width = |chars: usize| chars as f64 * KEY_TEXT * CHARACTER;
    let key_width = board
        .nets
        .iter()
        .map(|net| {
            let pads: usize = net
                .pads
                .iter()
                .map(|p| p.to_string().chars().count() + 1)
                .sum();
            text_width(net.name.chars().count() + 2 + pads) + 2.0
        })
        .fold(text_width(title.chars().count()), f64::max);
    let o = board.outline;
    let key_top = o.max.y + MARGIN;
    let left = o.min.x - MARGIN;
    let title_height = if with_key { TITLE_HEIGHT } else { 0.0 };
    let top = o.min.y - MARGIN - title_height;
    let width = if with_key {
        (o.width() + 2.0 * MARGIN).max(key_width + 2.0 * MARGIN)
    } else {
        o.width() + 2.0 * MARGIN
    };
    let key_lines = if with_key {
        board.nets.len() + board.planes.len()
    } else {
        0
    };
    let height = if with_key {
        key_top - top + KEY_LINE * key_lines as f64 + MARGIN / 2.0
    } else {
        o.height() + 2.0 * MARGIN
    };

    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}">"#,
        n((width * SCALE).round()),
        n((height * SCALE).round()),
        n(left),
        n(top),
        n(width),
        n(height)
    );
    let _ = writeln!(
        out,
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{BACKGROUND}"/>"#,
        n(left),
        n(top),
        n(width),
        n(height)
    );
    if with_key {
        let _ = writeln!(
            out,
            r#"<text x="{}" y="{}" font-size="{}" fill="{KEY}" font-family="monospace" dominant-baseline="central">{}</text>"#,
            n(o.min.x),
            n(o.min.y - MARGIN),
            n(KEY_TEXT),
            escape(&title)
        );
    }
    let corners = |radius: f64| {
        if radius > 0.0 {
            format!(r#" rx="{}" ry="{}""#, n(radius), n(radius))
        } else {
            String::new()
        }
    };
    match &board.polygon {
        // A polygon outline (issue #57) is drawn as the run of short pieces that follows it.
        Some(polygon) => {
            let mut d = String::new();
            for (k, (a, _)) in polygon.segments().iter().enumerate() {
                let _ = write!(
                    d,
                    "{}{} {}",
                    if k == 0 { "M" } else { " L" },
                    n(a.x),
                    n(a.y)
                );
            }
            d.push_str(" Z");
            let _ = write!(
                out,
                r#"<g class="l-outline"><path data-outline="polygon" d="{d}" fill="none" stroke="{OUTLINE}" stroke-width="0.1"/>"#
            );
        }
        None => {
            let _ = write!(
                out,
                r#"<g class="l-outline"><rect x="{}" y="{}" width="{}" height="{}"{} fill="none" stroke="{OUTLINE}" stroke-width="0.1"/>"#,
                n(o.min.x),
                n(o.min.y),
                n(o.width()),
                n(o.height()),
                corners(board.corner_radius)
            );
        }
    }
    for k in board.keepout_areas() {
        let what: Vec<&str> = [
            ("tracks", k.tracks),
            ("vias", k.vias),
            ("copper", k.copper),
            ("parts", k.parts),
        ]
        .into_iter()
        .filter(|(_, forbidden)| *forbidden)
        .map(|(name, _)| name)
        .collect();
        // An area that only limits height is amber and says the limit on the sheet, so that
        // a lid rib or a bracket can be seen against the parts (ADR 0045, C4).
        let colour = if k.forbids_anything() {
            KEEPOUT
        } else {
            HEIGHT_LIMIT
        };
        let mut title = if what.is_empty() {
            String::new()
        } else {
            format!("no {} on {}", what.join(", "), k.layers.join(", "))
        };
        if let Some(limit) = k.max_height {
            if !title.is_empty() {
                title.push_str("; ");
            }
            title.push_str(&format!(
                "parts at most {} mm tall on {}",
                n(limit),
                k.layers.join(", ")
            ));
        }
        let _ = writeln!(
            out,
            r#"<rect data-keepout="{}" x="{}" y="{}" width="{}" height="{}" fill="{colour}" fill-opacity="0.12" stroke="{colour}" stroke-width="0.1" stroke-dasharray="0.8 0.5"><title>{}: {}</title></rect>"#,
            escape(&k.name),
            n(k.rect.min.x),
            n(k.rect.min.y),
            n(k.rect.width()),
            n(k.rect.height()),
            escape(&k.name),
            escape(&title)
        );
        if let Some(limit) = k.max_height {
            let _ = writeln!(
                out,
                r#"<text data-height-limit="{}" x="{}" y="{}" font-size="{}" fill="{HEIGHT_LIMIT}" font-family="monospace" dominant-baseline="hanging">max {} mm</text>"#,
                escape(&k.name),
                n(k.rect.min.x + 0.4),
                n(k.rect.min.y + 0.3),
                n(KEY_TEXT),
                n(limit)
            );
        }
    }
    for (cutout, shape) in board.cutout_shapes() {
        let _ = writeln!(
            out,
            r#"<rect data-cutout="{}" x="{}" y="{}" width="{}" height="{}"{} fill="{BACKGROUND}" stroke="{OUTLINE}" stroke-width="0.1"><title>{}</title></rect>"#,
            escape(&cutout.name),
            n(shape.rect.min.x),
            n(shape.rect.min.y),
            n(shape.rect.width()),
            n(shape.rect.height()),
            corners(shape.radius),
            escape(&cutout.name)
        );
    }
    let _ = writeln!(out, "</g>");
    let _ = writeln!(out, r#"<g class="l-planes">"#);
    for (i, plane) in board.planes.iter().enumerate() {
        // Each plane's border sits a little further in than the one above it.
        let inset = board.rules.copper_to_edge + PLANE_STEP * (i as f64 + 1.0);
        let r = o.expanded(-inset);
        let _ = writeln!(
            out,
            r#"<rect data-net="{}" data-layer="{}" x="{}" y="{}" width="{}" height="{}" fill="none" stroke="{}" stroke-width="0.15" stroke-dasharray="1 0.6" stroke-opacity="0.8"/>"#,
            escape(&plane.net),
            escape(&plane.layer),
            n(r.min.x),
            n(r.min.y),
            n(r.width()),
            n(r.height()),
            net_colour(net_index(board, &plane.net))
        );
    }
    let _ = writeln!(out, "</g>");
    for placed in &board.parts {
        part(&mut out, placed, &colours);
    }
    for layer in [CopperLayer::Back, CopperLayer::Front] {
        let (colour, class) = if layer == CopperLayer::Front {
            (FRONT_COPPER, "l-front-copper")
        } else {
            (BACK_COPPER, "l-back-copper")
        };
        let _ = writeln!(out, r#"<g class="{class}">"#);
        for t in board.tracks.iter().filter(|t| t.layer == layer) {
            if let Some(arc) = t.arc() {
                // SVG's arc: the radius, then whether it sweeps the short way (an arc at a
                // bend always does) and its sense, clockwise on the screen being 1.
                let _ = writeln!(
                    out,
                    r#"<path data-net="{}" d="M {} {} A {r} {r} 0 0 {} {} {}" fill="none" stroke="{colour}" stroke-width="{}" stroke-linecap="round" stroke-opacity="0.85"/>"#,
                    escape(&t.net),
                    n(t.from.x),
                    n(t.from.y),
                    u8::from(!arc.ccw),
                    n(t.to.x),
                    n(t.to.y),
                    n(t.width),
                    r = n(arc.radius),
                );
                continue;
            }
            let _ = writeln!(
                out,
                r#"<line data-net="{}" x1="{}" y1="{}" x2="{}" y2="{}" stroke="{colour}" stroke-width="{}" stroke-linecap="round" stroke-opacity="0.85"/>"#,
                escape(&t.net),
                n(t.from.x),
                n(t.from.y),
                n(t.to.x),
                n(t.to.y),
                n(t.width)
            );
        }
        for d in board.teardrops.iter().filter(|d| d.layer == layer) {
            let points: Vec<String> = d
                .points
                .iter()
                .map(|p| format!("{},{}", n(p.x), n(p.y)))
                .collect();
            let _ = writeln!(
                out,
                r#"<polygon data-net="{}" points="{}" fill="{colour}" fill-opacity="0.85"/>"#,
                escape(&d.net),
                points.join(" ")
            );
        }
        let _ = writeln!(out, "</g>");
    }
    let _ = writeln!(out, r#"<g class="l-vias">"#);
    for v in &board.vias {
        let _ = writeln!(
            out,
            r#"<g data-net="{}"><circle cx="{}" cy="{}" r="{}" fill="{VIA_COPPER}"/><circle cx="{}" cy="{}" r="{}" fill="{BACKGROUND}"/></g>"#,
            escape(&v.net),
            n(v.at.x),
            n(v.at.y),
            n(v.diameter / 2.0),
            n(v.at.x),
            n(v.at.y),
            n(v.drill / 2.0)
        );
    }
    let _ = writeln!(out, "</g>");
    // A line to a plane has no length to draw; the pad shows in the key and the title.
    let _ = writeln!(out, r#"<g class="l-ratsnest">"#);
    for line in ratsnest.lines.iter().filter(|l| !l.to_plane) {
        let _ = writeln!(
            out,
            r#"<line data-net="{}" data-from="{}" data-to="{}" x1="{}" y1="{}" x2="{}" y2="{}" stroke="{RATSNEST}" stroke-width="0.12" stroke-opacity="0.85"/>"#,
            escape(&line.net),
            escape(&line.from.refdes),
            escape(&line.to.refdes),
            n(line.from_at.x),
            n(line.from_at.y),
            n(line.to_at.x),
            n(line.to_at.y)
        );
    }
    let _ = writeln!(out, "</g>");
    let _ = writeln!(out, r#"<g class="l-refs">"#);
    for placed in &board.parts {
        let at = placed.to_board(placed.reference_at(&board.rules));
        let _ = writeln!(
            out,
            r#"<text x="{}" y="{}" font-size="1" fill="{SILK}" text-anchor="middle" dominant-baseline="central" font-family="monospace">{}</text>"#,
            n(at.x),
            n(at.y),
            escape(&placed.refdes)
        );
    }
    let _ = writeln!(out, "</g>");
    if with_key {
        let _ = writeln!(out, r#"<g class="l-key">"#);
        for (i, net) in board.nets.iter().enumerate() {
            let y = key_top + KEY_LINE * i as f64;
            let pads: Vec<String> = net.pads.iter().map(ToString::to_string).collect();
            let _ = writeln!(
                out,
                r#"<rect x="{}" y="{}" width="1.2" height="1.2" fill="{}"/>"#,
                n(o.min.x),
                n(y - 0.6),
                net_colour(i)
            );
            let _ = writeln!(
                out,
                r#"<text x="{}" y="{}" font-size="{}" fill="{KEY}" font-family="monospace" dominant-baseline="central">{}</text>"#,
                n(o.min.x + 2.0),
                n(y),
                n(KEY_TEXT),
                escape(&format!("{}: {}", net.name, pads.join(" ")))
            );
        }
        for (i, plane) in board.planes.iter().enumerate() {
            let y = key_top + KEY_LINE * (board.nets.len() + i) as f64;
            let _ = writeln!(
                out,
                r#"<rect x="{}" y="{}" width="1.2" height="1.2" fill="none" stroke="{}" stroke-width="0.15" stroke-dasharray="0.4 0.25"/>"#,
                n(o.min.x),
                n(y - 0.6),
                net_colour(net_index(board, &plane.net))
            );
            let _ = writeln!(
                out,
                r#"<text x="{}" y="{}" font-size="{}" fill="{KEY}" font-family="monospace" dominant-baseline="central">{}</text>"#,
                n(o.min.x + 2.0),
                n(y),
                n(KEY_TEXT),
                escape(&format!("{}: plane {}", plane.layer, plane.net))
            );
        }
        let _ = writeln!(out, "</g>");
    }
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests {

    use super::*;

    fn led() -> Board {
        crate::board::tests::led_board()
    }

    #[test]
    fn draws_every_part_pad_and_ratsnest_line() {
        let svg = render(&led());
        for id in ["D1", "J1", "R1"] {
            assert!(svg.contains(&format!(r#"<g id="{id}""#)), "{id}");
        }
        // Two pads on each of the header, the resistor and the LED.
        let pads = svg.matches(r#"<g transform="translate("#).count();
        assert_eq!(pads, 6);
        assert_eq!(svg.matches(r##"stroke="#ffffff""##).count(), 3);
        assert!(svg.starts_with("<svg "));
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn a_part_on_the_back_is_drawn_mirrored_and_one_on_the_front_is_not() {
        let mut board = led();
        board
            .parts
            .iter_mut()
            .find(|p| p.refdes == "R1")
            .unwrap()
            .side = crate::board::Side::Back;
        let svg = render(&board);
        let group = |id: &str| {
            let at = svg.find(&format!(r#"<g id="{id}""#)).unwrap();
            svg[at..].lines().next().unwrap().to_string()
        };
        assert!(group("R1").contains("scale(-1 1)"), "{}", group("R1"));
        assert!(!group("D1").contains("scale"), "{}", group("D1"));
    }

    #[test]
    fn pads_take_their_nets_colour() {
        let svg = render(&led());
        // GND is the first net and VCC the second, so both colours are in use, and the
        // net with no name collision gets the third.
        for colour in &PALETTE[..3] {
            assert!(svg.contains(colour), "{colour}");
        }
        assert!(!svg.contains(PALETTE[3]));
    }

    #[test]
    fn planes_are_dashed_borders_in_their_nets_colour_with_a_line_in_the_key() {
        let board = crate::board::tests::four_layer_board();
        let svg = render(&board);
        assert_eq!(svg.matches("stroke-dasharray=\"1 0.6\"").count(), 2);
        assert!(svg.contains("In1.Cu: plane GND") && svg.contains("In2.Cu: plane 3V3"));
        // GND is the first net and 3V3 the second.
        assert!(svg.contains(&format!(
            "stroke=\"{}\" stroke-width=\"0.15\" stroke-dasharray=\"1 0.6\"",
            PALETTE[0]
        )));
        assert!(svg.contains(&format!(
            "stroke=\"{}\" stroke-width=\"0.15\" stroke-dasharray=\"1 0.6\"",
            PALETTE[1]
        )));
        // Each plane's rectangle names its net and its copper layer, so a page can colour it
        // by layer without reading the key.
        assert!(
            svg.contains(r#"<rect data-net="GND" data-layer="In1.Cu" "#),
            "{svg}"
        );
        assert!(
            svg.contains(r#"<rect data-net="3V3" data-layer="In2.Cu" "#),
            "{svg}"
        );
        // A line to a plane has nothing to draw, so only the other net has a ratsnest line.
        assert_eq!(svg.matches("stroke=\"#ffffff\"").count(), 1);
        assert_eq!(render(&led()).matches("stroke-dasharray").count(), 0);
    }

    #[test]
    fn the_view_has_no_key_and_names_the_net_of_every_pad() {
        let board = led();
        let view = render_view(&board);
        assert!(!view.contains("l-key"), "no key group");
        assert!(!view.contains("ratsnest 3 lines"), "no title");
        assert!(render(&board).contains(r#"<g class="l-key">"#));
        // It is shorter than the page with the key: the board fills the viewBox.
        let height = |svg: &str| {
            let at = svg.find("viewBox=\"").unwrap() + 9;
            let nums: Vec<f64> = svg[at..]
                .split('"')
                .next()
                .unwrap()
                .split(' ')
                .map(|v| v.parse().unwrap())
                .collect();
            nums[3]
        };
        assert!(height(&view) < height(&render(&board)));
        assert!((height(&view) - (board.outline.height() + 6.0)).abs() < 1e-9);
        for net in &board.nets {
            let want = format!("data-net=\"{}\"", net.name);
            assert_eq!(view.matches("<g transform=\"translate").count(), 6);
            assert!(view.contains(&want), "{want}");
        }
        for layer in [
            "l-outline",
            "l-planes",
            "l-front-copper",
            "l-back-copper",
            "l-vias",
            "l-ratsnest",
            "l-refs",
            "l-pads",
        ] {
            assert!(view.contains(&format!("class=\"{layer}\"")), "{layer}");
        }
        assert!(view.contains(r#"data-ref="R1""#));
    }

    #[test]
    fn a_page_can_pick_parts_pads_and_ratsnest_lines_out() {
        let board = led();
        let view = render_view(&board);
        for part in &board.parts {
            assert!(
                view.contains(&format!(
                    r#"data-ref="{}" data-path="{}""#,
                    part.refdes, part.path
                )),
                "{}",
                part.refdes
            );
        }
        // Every pad names its number, and the ratsnest line the parts at its ends.
        assert_eq!(view.matches("data-pad=").count(), 6);
        let line = view
            .lines()
            .find(|l| l.contains("data-from="))
            .expect("a ratsnest line");
        let attr = |name: &str| {
            let at = line.find(name).unwrap() + name.len();
            line[at..].split('"').next().unwrap().to_string()
        };
        let (from, to) = (attr("data-from=\""), attr("data-to=\""));
        assert!(board.part(&from).is_some() && board.part(&to).is_some());
        assert_ne!(from, to);
    }

    #[test]
    fn a_height_limit_is_drawn_in_amber_with_its_number_and_is_no_rule_area() {
        let mut board = led();
        board.keepouts.push(crate::board::BoardKeepout {
            name: "lid_ribs".into(),
            part: None,
            at: Some(Pt::new(1.0, 1.0)),
            size: (4.0, 2.0),
            layers: vec!["top".into()],
            tracks: false,
            vias: false,
            copper: false,
            parts: false,
            max_height: Some(3.0),
            drawn: false,
        });
        let view = render_view(&board);
        assert!(
            view.contains(r#"<rect data-keepout="lid_ribs" "#)
                && view.contains(HEIGHT_LIMIT)
                && view.contains("lid_ribs: parts at most 3 mm tall on top")
                && view.contains(r#"data-height-limit="lid_ribs""#)
                && view.contains(">max 3 mm</text>"),
            "{view}"
        );
        // KiCad has no height to enforce: a zone that forbids nothing is not a rule area.
        assert!(!crate::export::kicad_pcb(&board).contains("lid_ribs"));
        // One that forbids something as well is both.
        board.keepouts[0].tracks = true;
        let view = render_view(&board);
        assert!(
            view.contains("lid_ribs: no tracks on top; parts at most 3 mm tall on top"),
            "{view}"
        );
        assert!(crate::export::kicad_pcb(&board).contains("lid_ribs"));
    }

    #[test]
    fn a_keepout_and_a_cutout_name_themselves() {
        let mut board = led();
        board.keepouts.push(crate::board::BoardKeepout {
            name: "antenna".into(),
            part: None,
            at: Some(Pt::new(1.0, 1.0)),
            size: (4.0, 2.0),
            layers: vec!["top".into()],
            tracks: true,
            vias: false,
            copper: false,
            parts: false,
            max_height: None,
            drawn: false,
        });
        board.cutouts.push(crate::board::BoardCutout {
            name: "slot".into(),
            at: Some(Pt::new(2.0, 2.0)),
            size: (1.0, 3.0),
            radius: 0.0,
        });
        let view = render_view(&board);
        assert!(view.contains(r#"<rect data-keepout="antenna" "#), "{view}");
        assert!(view.contains(r#"<rect data-cutout="slot" "#), "{view}");
    }

    #[test]
    fn a_track_and_a_via_name_their_net() {
        let mut board = led();
        let net = board.nets[0].name.clone();
        board.tracks.push(crate::board::Track {
            net: net.clone(),
            layer: CopperLayer::Front,
            from: Pt::new(0.0, 0.0),
            to: Pt::new(1.0, 0.0),
            width: 0.25,
            mid: None,
        });
        board.vias.push(crate::board::Via {
            net: net.clone(),
            at: Pt::new(2.0, 2.0),
            diameter: 0.6,
            drill: 0.3,
        });
        let view = render_view(&board);
        assert!(view.contains(&format!("<line data-net=\"{net}\" x1=\"0\"")));
        assert!(view.contains(&format!("<g data-net=\"{net}\"><circle")));
    }

    #[test]
    fn the_same_board_gives_the_same_bytes() {
        assert_eq!(render(&led()), render(&led()));
    }

    #[test]
    fn names_are_escaped() {
        let mut board = led();
        board.nets[0].name = "A&B<C>".into();
        let svg = render(&board);
        assert!(svg.contains("A&amp;B&lt;C&gt;"));
        assert!(!svg.contains("A&B"));
    }

    #[test]
    fn an_arc_runs_the_way_its_middle_point_says() {
        // From the right, up over the top (y is negative upward), to the left: anticlockwise
        // on screen, so sweep 0, and half a turn is not a large arc.
        let path = arc_path(Pt::new(1.0, 0.0), Pt::new(0.0, -1.0), Pt::new(-1.0, 0.0)).unwrap();
        assert_eq!(path, "M1 0 A1 1 0 0 0 -1 0");
        let path = arc_path(Pt::new(1.0, 0.0), Pt::new(0.0, 1.0), Pt::new(-1.0, 0.0)).unwrap();
        assert_eq!(path, "M1 0 A1 1 0 0 1 -1 0");
        assert_eq!(
            arc_path(Pt::new(0.0, 0.0), Pt::new(1.0, 1.0), Pt::new(2.0, 2.0)),
            None
        );
    }
}
