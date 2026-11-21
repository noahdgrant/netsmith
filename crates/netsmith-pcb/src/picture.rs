//! A 2D picture of a board's copper (issue #1086, `netsmith view pcb`): the copper of one layer
//! or of all of them, a region of the board or the whole of it, seen from above or from below,
//! with the connections still open drawn as straight lines over it. It is an SVG; `netsmith
//! view pcb` writes it as it is or has `netsmith-draw` fill it into a PNG (ADR 0142), so the two
//! are the same drawing and the same board gives the same bytes.
//!
//! **What is drawn, from the bottom up.** The background; the board's shape as its `Edge.Cuts`
//! has it, cutouts open; the keepouts that forbid something on a layer drawn (dashed); then each
//! layer drawn, the one furthest from the viewer first: its pours and planes (translucent), its
//! teardrop fills, its pads, its tracks and the copper its footprints draw; then the vias with
//! copper on a layer drawn (the stitching vias the export will have that the routes file does
//! not, when given, in a tone of their own, issue #1489), and every drilled hole, open to the
//! background. Over the copper: the
//! courtyards and the designators when asked, the copper islands the fill took out when given
//! (hatched and outlined, dashed, in near white, each with its number: copper the pour would have
//! had and the Gerbers do not, issue #1188), the open connections (bright lines with a dot at
//! each end) and the markers of the design rules check when given (a ring with its number).
//!
//! **The copper is the Gerbers'.** Pads, tracks, pours, planes, fills and vias come from
//! [`crate::copper_of`], the geometry `fab` plots: a pour is its fill with the clearances and
//! the thermal reliefs, not its outline. A ring of a pour is simplified within 2 µm
//! ([`crate::simplify_ring`]), as the editor's pours are.
//! One exception: an arc of a net whose corners are mitered (the board's `.corner`, or a
//! `TrackPath`'s, issue #1508) is drawn as its chamfer, straight from its start to its end
//! ([`Board::chamfered`]), so a board whose style is mitered reads as mitered at every scale.
//!
//! **Colours** are the editor's dark theme (`themes.css`): the top layer copper-orange, the
//! bottom teal, each inner layer one of four tones, by its number. A colour is a layer's, never
//! a net's.
//!
//! **Seen from below** the board is mirrored left to right about the middle of its outline, as
//! the editor's Side switch shows the back: the bottom layer is nearest and drawn last, and the
//! text still reads left to right. The region is given in the board's frame either way, so the
//! numbers of a region are the same seen from above and from below.
//!
//! **Lit.** [`Lit`] names parts, nets and modules (`--highlight`); with it, what it does not
//! name is drawn faded: a pad is lit by its part or its net, a track, a via, a pour, a fill and
//! an open connection by its net, a courtyard and a designator by its part. Holes, the outline
//! and the markers are never faded. A module (`module:<path>`, issue #1129) lights its parts, and
//! each module named is drawn in its own colour ([`crate::module_colour`], the dark set): each of
//! its parts' boxes filled faintly and outlined, and a dashed box round all of them, so where a
//! module's parts are and how far they spread shows at once.
//!
//! **Sizes in pixels.** The lines that are marks, not copper (the outline, an open connection, a
//! courtyard, a marker) and the text are sized in pixels of the picture (the scale is given), so
//! that they read at every scale; the copper is drawn at its size in millimetres.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use netsmith_kicad::fmt_num;

use crate::board::{Board, CopperLayer, Placed};
use crate::copper::{Area, copper as copper_of};
use crate::footprint::{Graphic, Layer, Shape};
use crate::geometry::{Pt, Rect};
use crate::ratsnest::Ratline;
use crate::routes::layer_word;
use crate::simplify::simplify_ring;
use crate::svg::arc_path;

/// Which copper layers a picture draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layers {
    All,
    One(CopperLayer),
}

/// What a picture shows and how.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub layers: Layers,
    /// Seen from below: mirrored left to right (the module's header).
    pub from_below: bool,
    /// The region drawn, in the board's frame (mm, y down); `None` is the outline with
    /// [`MARGIN`] round it.
    pub region: Option<Rect>,
    /// Pixels a millimetre.
    pub scale: f64,
    pub courtyards: bool,
    pub designators: bool,
    /// What is lit, the rest faded; `None` fades nothing.
    pub lit: Option<Lit>,
    /// The places of the vias drawn in the stitching tone: the stitching vias the export will
    /// have that the routes file does not (issue #1489, `netsmith_router::stitching_vias`). A via
    /// of the board at one of them is drawn in [`STITCHING`], with `class="stitching"`.
    pub stitching: Vec<Pt>,
}

/// Space round the outline when no region is given, in mm.
pub const MARGIN: f64 = 1.0;

/// The colour behind the board, red, green and blue: what a rasterizer fills the picture with.
pub const BACKGROUND: [u8; 3] = [0x0b, 0x0e, 0x13];

const BG: &str = "#0b0e13";
const BOARD: &str = "#121821";
const OUTLINE: &str = "#8396b1";
const FRONT: &str = "#d08d50";
const BACK: &str = "#46a3bc";
/// The inner layers' tones, `In1.Cu` the first; a fifth inner layer takes the first again.
const INNER: [&str; 4] = ["#9a8cf0", "#5fc98f", "#e0b050", "#e07aa8"];
const VIA: &str = "#a3adbc";
/// The stitching vias not yet in the routes file (issue #1489): a cooler, darker grey than
/// [`VIA`], copper still, but marked as the export's to lay.
pub const STITCHING: &str = "#6f8fb0";
/// Every colour the picture fills copper with, as `#rrggbb`: each layer's tone and the vias'. A
/// reader of the picture tells copper from what is not by them, as the comparison of a board's
/// copper with its Gerbers' does (issue #1142, `netsmith_fabricate::gerber_compare`); a hole is
/// drawn in [`BACKGROUND`].
pub const COPPER: [&str; 8] = [
    FRONT, BACK, INNER[0], INNER[1], INNER[2], INNER[3], VIA, STITCHING,
];
const COURTYARD: &str = "#ff26e2";
const DESIGNATOR: &str = "#d3cfc1";
const KEEPOUT: &str = "#e0707a";
/// The open connections: bright, so they are seen over the copper of either face.
const OPEN: &str = "#ffd84d";
const ERROR: &str = "#ff4d4d";
/// The copper islands the fill took out: near white, a mark that is no layer's copper and reads
/// on the board's dark substrate where the copper is not.
const ISLAND: &str = "#eef1f7";
const WARNING: &str = "#ffb020";

/// How opaque a pour is: the pads and tracks over it must show.
const POUR_OPACITY: f64 = 0.35;
/// How opaque what is not lit is.
const FADED: f64 = 0.2;
/// How far a point of a pour's ring may move when the ring is simplified, in mm.
const SIMPLIFY: f64 = 0.002;

/// The parts, the nets and the modules a picture lights.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lit {
    /// The parts, by path: those named and those of the modules named.
    pub parts: BTreeSet<String>,
    pub nets: BTreeSet<String>,
    /// The modules named, in order (the order of their colours), each with the paths of its parts.
    pub modules: Vec<(String, BTreeSet<String>)>,
}

impl Lit {
    /// What `ids` name on `board`: a part by its designator or its path, a net as `net:<name>`,
    /// a module as `module:<path>` (its path in the design, as `layout summary` names it).
    /// The error names every id the board has nothing of, and lists the ids it has.
    pub fn of(board: &Board, ids: &[String]) -> Result<Lit, String> {
        let mut lit = Lit::default();
        let unknown: Vec<&String> = ids.iter().filter(|id| !lit.add(board, id)).collect();
        if unknown.is_empty() {
            return Ok(lit);
        }
        let list = |v: Vec<String>| {
            if v.is_empty() {
                "(none)".to_string()
            } else {
                v.join(", ")
            }
        };
        Err(format!(
            "--highlight names {} that the board does not have: {}\nthe parts are: {}\nthe nets: {}\nthe modules: {}",
            if unknown.len() == 1 { "an id" } else { "ids" },
            unknown
                .iter()
                .map(|u| format!("`{u}`"))
                .collect::<Vec<_>>()
                .join(", "),
            list(
                board
                    .parts
                    .iter()
                    .map(|p| format!("{} ({})", p.refdes, p.path))
                    .collect()
            ),
            list(
                board
                    .nets
                    .iter()
                    .map(|n| format!("net:{}", n.name))
                    .collect()
            ),
            list(
                crate::module_paths_of(board.parts.iter().map(|p| p.path.as_str()))
                    .into_iter()
                    .map(|m| format!("module:{m}"))
                    .collect()
            ),
        ))
    }

    /// Light what `id` names; false when the board has nothing of that name. Each kind of id is
    /// one arm, its prefix first, so a new kind (`module:<path>`, issue #1129) is one more.
    fn add(&mut self, board: &Board, id: &str) -> bool {
        if let Some(net) = id.strip_prefix("net:") {
            let found = board.nets.iter().any(|n| n.name == net);
            if found {
                self.nets.insert(net.to_string());
            }
            return found;
        }
        if let Some(module) = id.strip_prefix("module:") {
            let modules = crate::module_paths_of(board.parts.iter().map(|p| p.path.as_str()));
            if !modules.iter().any(|m| m == module) {
                return false;
            }
            let mine: BTreeSet<String> = board
                .parts
                .iter()
                .filter(|p| crate::in_module(&p.path, module))
                .map(|p| p.path.clone())
                .collect();
            if !self.modules.iter().any(|(m, _)| m == module) {
                self.parts.extend(mine.iter().cloned());
                self.modules.push((module.to_string(), mine));
            }
            return true;
        }
        match board.parts.iter().find(|p| p.path == id || p.refdes == id) {
            Some(p) => {
                self.parts.insert(p.path.clone());
                true
            }
            None => false,
        }
    }

    /// Light the nets of `lines` too (`--only-open`).
    pub fn with_nets_of(mut self, lines: &[Ratline]) -> Lit {
        self.nets.extend(lines.iter().map(|l| l.net.clone()));
        self
    }
}

/// A place the design rules check found something wrong, drawn as a ring with its label.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    /// In the board's frame.
    pub at: Pt,
    /// KiCad's name of the layer it is on (`F.Cu`, `F.Mask`): one on a copper layer is drawn
    /// with that layer only; one on no layer or on another layer is drawn with every layer.
    pub layer: Option<String>,
    /// An error rather than a warning.
    pub error: bool,
    /// What is written beside it: its number in the list printed with the picture.
    pub label: String,
}

/// A copper island the fill of a pour took out (issue #1188): drawn on its layer, hatched and
/// outlined, with its label.
#[derive(Debug, Clone, PartialEq)]
pub struct Island {
    /// KiCad's name of its copper layer (`F.Cu`): it is drawn with that layer only.
    pub layer: String,
    /// Its rings in the board's frame, the outline first and then its holes.
    pub rings: Vec<Vec<Pt>>,
    /// What is written beside it: its number in the list printed with the picture.
    pub label: String,
}

/// The region a picture of `board` shows when it is given none.
pub fn whole(board: &Board) -> Rect {
    board.outline.expanded(MARGIN)
}

/// The board's picture as the text of an SVG file: the copper of `options.layers`, the open
/// connections `open` (a line to a plane is left out: it has no length), and the `islands` the
/// fill took out and the `markers` on a layer drawn. The error says why the copper cannot be
/// drawn (the board's file would not plot either) or names a layer the board does not have.
pub fn picture(
    board: &Board,
    open: &[Ratline],
    islands: &[Island],
    markers: &[Marker],
    options: &Picture,
) -> Result<String, String> {
    let chamfered = board.chamfered();
    let board = chamfered.as_ref().unwrap_or(board);
    let copper = copper_of(board).map_err(|e| e.to_string())?;
    let names: Vec<&str> = copper.layers.iter().map(|l| l.layer.as_str()).collect();
    let shown: Vec<usize> = match options.layers {
        Layers::All => (0..names.len()).collect(),
        Layers::One(layer) => match names.iter().position(|n| *n == layer.name()) {
            Some(i) => vec![i],
            None => {
                let words: Vec<String> = names.iter().filter_map(|n| layer_word(n)).collect();
                return Err(format!(
                    "the board has no copper layer {}: its layers are {}",
                    layer_word(layer.name()).unwrap_or_else(|| layer.name().to_string()),
                    words.join(", ")
                ));
            }
        },
    };
    // The layer furthest from the viewer first: the layers run from the front to the back.
    let mut order = shown.clone();
    if !options.from_below {
        order.reverse();
    }
    let region = options.region.unwrap_or_else(|| whole(board));
    let o = board.outline;
    let f = Frame {
        flip: options.from_below.then_some(o.min.x + o.max.x),
        scale: options.scale,
        lit: options.lit.as_ref(),
    };
    let view = f.rect(region);
    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}">"#,
        (region.width() * options.scale).ceil().max(1.0),
        (region.height() * options.scale).ceil().max(1.0),
        n(view.min.x),
        n(view.min.y),
        n(view.width()),
        n(view.height())
    );
    let what = match options.layers {
        Layers::All => "all layers".to_string(),
        Layers::One(l) => layer_word(l.name()).unwrap_or_else(|| l.name().to_string()),
    };
    let _ = writeln!(
        out,
        "<title>{} copper, {what}, seen from {}</title>",
        escape(&board.name),
        if options.from_below { "below" } else { "above" }
    );
    let _ = writeln!(
        out,
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{BG}"/>"#,
        n(view.min.x),
        n(view.min.y),
        n(view.width()),
        n(view.height())
    );

    // The board.
    let mut d = String::new();
    if copper.edge.outer.len() >= 3 {
        f.area(&mut d, &copper.edge, 0.0);
    } else {
        let r = f.rect(o);
        let _ = write!(
            d,
            "M{} {}H{}V{}H{}Z",
            n(r.min.x),
            n(r.min.y),
            n(r.max.x),
            n(r.max.y),
            n(r.min.x)
        );
    }
    let _ = writeln!(
        out,
        r#"<path class="board" d="{d}" fill="{BOARD}" fill-rule="evenodd" stroke="{OUTLINE}" stroke-width="{}" stroke-linejoin="round"/>"#,
        n(f.px(1.5))
    );

    // The keepouts that forbid something on a layer drawn.
    let words: Vec<String> = shown.iter().filter_map(|&i| layer_word(names[i])).collect();
    let _ = writeln!(out, r#"<g class="keepouts">"#);
    for k in board.keepout_areas() {
        if !k.forbids_anything() || !k.layers.iter().any(|l| words.contains(l)) {
            continue;
        }
        let r = f.rect(k.rect);
        let _ = writeln!(
            out,
            r#"<rect data-keepout="{}" x="{}" y="{}" width="{}" height="{}" fill="{KEEPOUT}" fill-opacity="0.1" stroke="{KEEPOUT}" stroke-width="{}" stroke-dasharray="{} {}"/>"#,
            escape(&k.name),
            n(r.min.x),
            n(r.min.y),
            n(r.width()),
            n(r.height()),
            n(f.px(1.5)),
            n(f.px(6.0)),
            n(f.px(4.0))
        );
    }
    let _ = writeln!(out, "</g>");

    // The copper, layer by layer.
    for &i in &order {
        let layer = &copper.layers[i];
        let colour = colour_of(&layer.layer);
        let _ = writeln!(
            out,
            r#"<g class="layer" data-layer="{}">"#,
            escape(&layer.layer)
        );
        for pour in &layer.pours {
            let mut d = String::new();
            f.area(&mut d, &pour.area, SIMPLIFY);
            if d.is_empty() {
                continue;
            }
            let _ = writeln!(
                out,
                r#"<path class="pour"{} d="{d}" fill="{colour}" fill-opacity="{}"/>"#,
                net_attr(pour.net.as_deref()),
                n(POUR_OPACITY * f.alpha(f.net(pour.net.as_deref())))
            );
        }
        for fill in &layer.fills {
            let mut d = String::new();
            f.area(&mut d, &fill.area, 0.0);
            let _ = writeln!(
                out,
                r#"<path class="fill"{} d="{d}" fill="{colour}"{}/>"#,
                net_attr(fill.net.as_deref()),
                opacity("fill-opacity", f.alpha(f.net(fill.net.as_deref())))
            );
        }
        for pad in &layer.pads {
            let mut d = String::new();
            for a in &pad.areas {
                f.area(&mut d, a, 0.0);
            }
            let lit = f.net(pad.net.as_deref()) || f.part(&pad.part);
            let _ = writeln!(
                out,
                r#"<path class="pad" data-pad="{}.{}"{} d="{d}" fill="{colour}"{}/>"#,
                escape(&pad.refdes),
                escape(&pad.number),
                net_attr(pad.net.as_deref()),
                opacity("fill-opacity", f.alpha(lit))
            );
        }
        for t in &layer.tracks {
            let (a, b) = (f.p(t.from), f.p(t.to));
            let _ = writeln!(
                out,
                r#"<line{} x1="{}" y1="{}" x2="{}" y2="{}" stroke="{colour}" stroke-width="{}" stroke-linecap="round"{}/>"#,
                net_attr(t.net.as_deref()),
                n(a.x),
                n(a.y),
                n(b.x),
                n(b.y),
                n(t.width),
                opacity("stroke-opacity", f.alpha(f.net(t.net.as_deref())))
            );
        }
        if !layer.drawings.is_empty() {
            let mut d = String::new();
            for a in &layer.drawings {
                f.area(&mut d, a, 0.0);
            }
            let _ = writeln!(
                out,
                r#"<path class="drawing" d="{d}" fill="{colour}"{}/>"#,
                opacity("fill-opacity", f.alpha(false))
            );
        }
        let _ = writeln!(out, "</g>");
    }

    // The vias with copper on a layer drawn, then every hole, open to what is behind the board.
    let _ = writeln!(out, r#"<g class="vias">"#);
    for v in &copper.vias {
        let on = match v.span {
            Some((a, b)) => shown.iter().any(|&i| a <= i && i <= b),
            None => true,
        };
        if !on {
            continue;
        }
        let c = f.p(v.at);
        let stitching = options.stitching.iter().any(|p| p.distance(v.at) < 1e-6);
        let (class, fill) = if stitching {
            (r#" class="stitching""#, STITCHING)
        } else {
            ("", VIA)
        };
        let _ = writeln!(
            out,
            r#"<circle{class}{} cx="{}" cy="{}" r="{}" fill="{fill}"{}/>"#,
            net_attr(v.net.as_deref()),
            n(c.x),
            n(c.y),
            n(v.diameter / 2.0),
            opacity("fill-opacity", f.alpha(f.net(v.net.as_deref())))
        );
    }
    let _ = writeln!(out, "</g>");
    let _ = writeln!(out, r#"<g class="holes">"#);
    for h in &copper.holes {
        let a = f.p(h.at);
        match h.slot_to {
            Some(to) => {
                let b = f.p(to);
                let _ = writeln!(
                    out,
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{BG}" stroke-width="{}" stroke-linecap="round"/>"#,
                    n(a.x),
                    n(a.y),
                    n(b.x),
                    n(b.y),
                    n(h.diameter)
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    r#"<circle cx="{}" cy="{}" r="{}" fill="{BG}"/>"#,
                    n(a.x),
                    n(a.y),
                    n(h.diameter / 2.0)
                );
            }
        }
    }
    let _ = writeln!(out, "</g>");

    // The parts of the face seen: on one outer layer, those on its face; otherwise every part.
    let face = |p: &Placed| match options.layers {
        Layers::One(CopperLayer::Front) => !p.mirrored(),
        Layers::One(CopperLayer::Back) => p.mirrored(),
        _ => true,
    };
    if options.courtyards {
        let _ = writeln!(out, r#"<g class="courtyards">"#);
        for p in board.parts.iter().filter(|p| face(p)) {
            courtyard(&mut out, &f, p);
        }
        let _ = writeln!(out, "</g>");
    }
    if let Some(lit) = options.lit.as_ref().filter(|l| !l.modules.is_empty()) {
        modules(&mut out, &f, board, lit, &face);
    }

    let shown_names: Vec<&str> = shown.iter().map(|&i| names[i]).collect();
    if !islands.is_empty() {
        removed_islands(&mut out, &f, islands, &shown_names);
    }

    let _ = writeln!(out, r#"<g class="open">"#);
    for line in open.iter().filter(|l| !l.to_plane) {
        let (a, b) = (f.p(line.from_at), f.p(line.to_at));
        let alpha = opacity("opacity", f.alpha(f.net(Some(&line.net))));
        let _ = writeln!(
            out,
            r#"<g data-net="{}" data-from="{}" data-to="{}"{alpha}><line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{OPEN}" stroke-width="{}" stroke-linecap="round"/><circle cx="{}" cy="{}" r="{}" fill="{OPEN}"/><circle cx="{}" cy="{}" r="{}" fill="{OPEN}"/></g>"#,
            escape(&line.net),
            escape(&line.from.to_string()),
            escape(&line.to.to_string()),
            n(a.x),
            n(a.y),
            n(b.x),
            n(b.y),
            n(f.px(2.0)),
            n(a.x),
            n(a.y),
            n(f.px(3.0)),
            n(b.x),
            n(b.y),
            n(f.px(3.0))
        );
    }
    let _ = writeln!(out, "</g>");

    if options.designators {
        let size = f.px(11.0).max(0.5);
        let _ = writeln!(out, r#"<g class="designators">"#);
        for p in board.parts.iter().filter(|p| face(p)) {
            let Some(text) = p.silk_text() else {
                continue;
            };
            let at = f.p(p.to_board(p.reference_at(&board.rules)));
            let _ = writeln!(
                out,
                r#"<text x="{}" y="{}" font-size="{}" font-family="sans-serif" text-anchor="middle" fill="{DESIGNATOR}" stroke="{BOARD}" stroke-width="{}" paint-order="stroke"{}{}>{}</text>"#,
                n(at.x),
                // The baseline, so that the letters stand round the point.
                n(at.y + 0.35 * size),
                n(size),
                n(f.px(3.0)),
                opacity("opacity", f.alpha(f.part(&p.path))),
                // Seen from below, a turn counterclockwise from above is clockwise.
                crate::svg::turned(
                    if f.flip.is_some() {
                        -p.text_turn()
                    } else {
                        p.text_turn()
                    },
                    at
                ),
                escape(text)
            );
        }
        let _ = writeln!(out, "</g>");
    }

    let _ = writeln!(out, r#"<g class="markers">"#);
    for m in markers.iter().filter(|m| {
        m.layer
            .as_deref()
            .is_none_or(|l| !l.ends_with(".Cu") || shown_names.contains(&l))
    }) {
        let c = f.p(m.at);
        let colour = if m.error { ERROR } else { WARNING };
        let r = f.px(8.0);
        let size = f.px(12.0);
        let _ = writeln!(
            out,
            r#"<g class="marker"><circle cx="{}" cy="{}" r="{}" fill="none" stroke="{colour}" stroke-width="{}"/><circle cx="{}" cy="{}" r="{}" fill="{colour}"/><text x="{}" y="{}" font-size="{}" font-family="sans-serif" fill="{colour}" stroke="{BG}" stroke-width="{}" paint-order="stroke">{}</text></g>"#,
            n(c.x),
            n(c.y),
            n(r),
            n(f.px(2.0)),
            n(c.x),
            n(c.y),
            n(f.px(2.0)),
            n(c.x + r + f.px(2.0)),
            n(c.y - r),
            n(size),
            n(f.px(3.0)),
            escape(&m.label)
        );
    }
    let _ = writeln!(out, "</g>");
    out.push_str("</svg>\n");
    Ok(out)
}

/// From the board's frame to the picture's, and how lit a thing is.
struct Frame<'a> {
    /// Mirrored about `x = flip / 2`: the sum of the outline's left and right.
    flip: Option<f64>,
    scale: f64,
    lit: Option<&'a Lit>,
}

impl Frame<'_> {
    fn p(&self, p: Pt) -> Pt {
        match self.flip {
            Some(s) => Pt::new(s - p.x, p.y),
            None => p,
        }
    }

    fn rect(&self, r: Rect) -> Rect {
        let (a, b) = (self.p(r.min), self.p(r.max));
        Rect::new(
            Pt::new(a.x.min(b.x), a.y.min(b.y)),
            Pt::new(a.x.max(b.x), a.y.max(b.y)),
        )
    }

    /// `pixels` of the picture, in mm.
    fn px(&self, pixels: f64) -> f64 {
        pixels / self.scale
    }

    fn net(&self, net: Option<&str>) -> bool {
        self.lit
            .is_some_and(|l| net.is_some_and(|n| l.nets.contains(n)))
    }

    fn part(&self, path: &str) -> bool {
        self.lit.is_some_and(|l| l.parts.contains(path))
    }

    /// How opaque a thing is that is `lit` or not.
    fn alpha(&self, lit: bool) -> f64 {
        if self.lit.is_none() || lit {
            1.0
        } else {
            FADED
        }
    }

    /// The subpaths of an area, its holes after its outline, each ring simplified within
    /// `tolerance` (0: as it is). A ring of fewer than three points is left out.
    fn area(&self, d: &mut String, a: &Area, tolerance: f64) {
        for ring in std::iter::once(&a.outer).chain(&a.holes) {
            let ring = if tolerance > 0.0 {
                simplify_ring(ring, tolerance)
            } else {
                ring.clone()
            };
            if ring.len() < 3 {
                continue;
            }
            for (i, p) in ring.iter().enumerate() {
                let p = self.p(*p);
                let _ = write!(d, "{}{} {}", if i == 0 { 'M' } else { 'L' }, n(p.x), n(p.y));
            }
            d.push('Z');
        }
    }
}

/// The islands on a layer drawn (`shown`), each hatched at 45° and outlined, dashed, with its
/// label by its top left corner. The hatch is worked out here, not left to an SVG pattern: the
/// rasterizer draws no patterns, and the PNG must be the SVG's drawing.
fn removed_islands(out: &mut String, f: &Frame, islands: &[Island], shown: &[&str]) {
    let _ = writeln!(
        out,
        r#"<g class="islands" fill="none" stroke="{ISLAND}" stroke-linejoin="round">"#
    );
    for island in islands.iter().filter(|i| shown.contains(&i.layer.as_str())) {
        let rings: Vec<Vec<Pt>> = island
            .rings
            .iter()
            .filter(|r| r.len() >= 3)
            .map(|r| r.iter().map(|p| f.p(*p)).collect())
            .collect();
        let Some(first) = rings.first() else {
            continue;
        };
        let mut outline = String::new();
        for ring in &rings {
            for (i, p) in ring.iter().enumerate() {
                let _ = write!(
                    outline,
                    "{}{} {}",
                    if i == 0 { 'M' } else { 'L' },
                    n(p.x),
                    n(p.y)
                );
            }
            outline.push('Z');
        }
        let mut hatch = String::new();
        for (a, b) in hatch_lines(&rings, f.px(6.0)) {
            let _ = write!(hatch, "M{} {}L{} {}", n(a.x), n(a.y), n(b.x), n(b.y));
        }
        let top_left = first.iter().fold(first[0], |m, p| {
            if p.y < m.y || (p.y == m.y && p.x < m.x) {
                *p
            } else {
                m
            }
        });
        let size = f.px(12.0);
        let _ = writeln!(
            out,
            r#"<g class="island" data-layer="{}"><path d="{hatch}" stroke-width="{}" stroke-opacity="0.7"/><path d="{outline}" stroke-width="{}" stroke-dasharray="{} {}"/><text x="{}" y="{}" font-size="{}" font-family="sans-serif" fill="{ISLAND}" stroke="{BG}" stroke-width="{}" paint-order="stroke">{}</text></g>"#,
            escape(&island.layer),
            n(f.px(1.0)),
            n(f.px(1.5)),
            n(f.px(5.0)),
            n(f.px(3.0)),
            n(top_left.x + f.px(2.0)),
            n(top_left.y - f.px(3.0)),
            n(size),
            n(f.px(3.0)),
            escape(&island.label)
        );
    }
    let _ = writeln!(out, "</g>");
}

/// The pieces of the lines at 45° (down to the right on the picture), `pitch` apart, that lie in
/// the area the `rings` bound by the even-odd rule: the hatch of an island.
fn hatch_lines(rings: &[Vec<Pt>], pitch: f64) -> Vec<(Pt, Pt)> {
    // In the frame turned by 45° the lines are those of constant v = y - x; along a line, the
    // place is u = x + y.
    let uv = |p: &Pt| (p.x + p.y, p.y - p.x);
    let back = |u: f64, v: f64| Pt::new((u - v) / 2.0, (u + v) / 2.0);
    let edges: Vec<((f64, f64), (f64, f64))> = rings
        .iter()
        .flat_map(|r| (0..r.len()).map(move |i| (uv(&r[i]), uv(&r[(i + 1) % r.len()]))))
        .collect();
    let (lo, hi) = edges.iter().fold((f64::MAX, f64::MIN), |(lo, hi), (a, _)| {
        (lo.min(a.1), hi.max(a.1))
    });
    // Lines `pitch` apart are `pitch` times the root of 2 apart in v.
    let step = pitch * std::f64::consts::SQRT_2;
    if step.is_nan() || step <= 0.0 || lo > hi {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut v = (lo / step).floor() * step + step / 2.0;
    while v < hi {
        let mut cuts: Vec<f64> = edges
            .iter()
            .filter(|(a, b)| (a.1 <= v) != (b.1 <= v))
            .map(|(a, b)| a.0 + (v - a.1) / (b.1 - a.1) * (b.0 - a.0))
            .collect();
        cuts.sort_by(f64::total_cmp);
        for [a, b] in cuts.as_chunks::<2>().0 {
            out.push((back(*a, v), back(*b, v)));
        }
        v += step;
    }
    out
}

/// The modules lit, each in its colour: each part's courtyard box filled faintly and outlined,
/// and a dashed box round all of its parts (those of the face seen).
fn modules(out: &mut String, f: &Frame, board: &Board, lit: &Lit, face: &dyn Fn(&Placed) -> bool) {
    let _ = writeln!(out, r#"<g class="modules">"#);
    for (k, (module, paths)) in lit.modules.iter().enumerate() {
        let colour = crate::module_colour(k, true);
        let boxes: Vec<Rect> = board
            .parts
            .iter()
            .filter(|p| paths.contains(&p.path) && face(p))
            .map(|p| f.rect(p.courtyard()))
            .collect();
        let _ = writeln!(
            out,
            r#"<g data-module="{}" stroke="{colour}" stroke-width="{}" stroke-linejoin="round">"#,
            escape(module),
            n(f.px(2.0))
        );
        for r in &boxes {
            let _ = writeln!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{colour}" fill-opacity="0.22"/>"#,
                n(r.min.x),
                n(r.min.y),
                n(r.width()),
                n(r.height())
            );
        }
        if let Some(first) = boxes.first() {
            let all = boxes.iter().skip(1).fold(*first, |a, b| a.union(b));
            let all = all.expanded(f.px(4.0));
            let _ = writeln!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke-dasharray="{} {}"/>"#,
                n(all.min.x),
                n(all.min.y),
                n(all.width()),
                n(all.height()),
                n(f.px(6.0)),
                n(f.px(4.0))
            );
        }
        let _ = writeln!(out, "</g>");
    }
    let _ = writeln!(out, "</g>");
}

/// A part's courtyard: the drawings of its footprint's courtyard, or the boxes the placer keeps
/// other parts out of when it draws none.
fn courtyard(out: &mut String, f: &Frame, p: &Placed) {
    let stroke = format!(
        r#" fill="none" stroke="{COURTYARD}" stroke-width="{}" stroke-linejoin="round"{}"#,
        n(f.px(1.5)),
        opacity("stroke-opacity", f.alpha(f.part(&p.path)))
    );
    let at = |q: Pt| f.p(p.to_board(q));
    let points = |ps: &[Pt]| -> String {
        ps.iter()
            .map(|q| {
                let q = at(*q);
                format!("{},{}", n(q.x), n(q.y))
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let drawn: Vec<&Graphic> = p
        .footprint
        .graphics
        .iter()
        .filter(|g| g.layer == Layer::Courtyard)
        .collect();
    let _ = writeln!(
        out,
        r#"<g data-part="{}" data-ref="{}">"#,
        escape(&p.path),
        escape(&p.refdes)
    );
    if drawn.is_empty() {
        for r in p.courtyards() {
            let r = f.rect(r);
            let _ = writeln!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}"{stroke}/>"#,
                n(r.min.x),
                n(r.min.y),
                n(r.width()),
                n(r.height())
            );
        }
    }
    for g in drawn {
        let _ = match &g.shape {
            Shape::Line { a, b } => {
                let (a, b) = (at(*a), at(*b));
                writeln!(
                    out,
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}"{stroke} stroke-linecap="round"/>"#,
                    n(a.x),
                    n(a.y),
                    n(b.x),
                    n(b.y)
                )
            }
            Shape::Rect { a, b } => writeln!(
                out,
                r#"<polygon points="{}"{stroke}/>"#,
                points(&[*a, Pt::new(b.x, a.y), *b, Pt::new(a.x, b.y)])
            ),
            Shape::Circle { centre, radius } => {
                let c = at(*centre);
                writeln!(
                    out,
                    r#"<circle cx="{}" cy="{}" r="{}"{stroke}/>"#,
                    n(c.x),
                    n(c.y),
                    n(*radius)
                )
            }
            Shape::Arc { start, mid, end } => {
                let (s, m, e) = (at(*start), at(*mid), at(*end));
                let d = arc_path(s, m, e)
                    .unwrap_or_else(|| format!("M{} {} L{} {}", n(s.x), n(s.y), n(e.x), n(e.y)));
                writeln!(out, r#"<path d="{d}"{stroke} stroke-linecap="round"/>"#)
            }
            Shape::Poly { points: ps } => {
                writeln!(out, r#"<polygon points="{}"{stroke}/>"#, points(ps))
            }
        };
    }
    let _ = writeln!(out, "</g>");
}

/// The colour of a copper layer, by KiCad's name.
fn colour_of(layer: &str) -> &'static str {
    match layer {
        "F.Cu" => FRONT,
        "B.Cu" => BACK,
        _ => {
            let n: usize = layer
                .strip_prefix("In")
                .and_then(|l| l.strip_suffix(".Cu"))
                .and_then(|l| l.parse().ok())
                .unwrap_or(1);
            INNER[(n.max(1) - 1) % INNER.len()]
        }
    }
}

fn net_attr(net: Option<&str>) -> String {
    net.map_or_else(String::new, |n| format!(r#" data-net="{}""#, escape(n)))
}

/// ` name="alpha"`, or nothing when it is fully opaque.
fn opacity(name: &str, alpha: f64) -> String {
    if alpha >= 1.0 {
        String::new()
    } else {
        format!(r#" {name}="{}""#, n(alpha))
    }
}

fn n(v: f64) -> String {
    fmt_num(v)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
