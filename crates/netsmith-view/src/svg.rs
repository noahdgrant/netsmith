//! Draw a laid-out design as an SVG.
//!
//! Every wire, junction, label, pin and part carries `data-net`, `data-ref` or `data-pin`
//! so the page can highlight them. Coordinates are millimetres on the sheet, so the
//! drawing scales with the viewer. A part also says where its symbol's origin is
//! (`data-origin`, `x y`), how it is turned (`data-orient`, `mirror turns`: `0 1` is a quarter
//! turn counterclockwise, as the layout file's `rotate = 90` and `mirror`, ADR 0061) and, for
//! the reference part of its sheet, `data-anchor="1"`.

use std::fmt::Write as _;

use netsmith_core::{Design, DesignError};
use netsmith_kicad::{Mapping, SymbolLib};

use crate::geom::Dir;
use crate::geom::{Orient, Pt};
use crate::glyph::{Fill, GlyphPin, P, Shape};
use crate::layout::{
    Anchor, Layout, MarkKind, Marks, PinMark, PlacedPart, Port, PowerKind, PowerSymbol, Rails,
    Text, layout, layout_rails,
};
use crate::sheets::{Group, TITLE_SIZE, layout_sheets, layout_sheets_marked, layout_sheets_rails};

/// Wires and symbol lines are never drawn thinner than this.
const MIN_LINE: f64 = 0.2;
const WIRE_WIDTH: f64 = 0.25;
const JUNCTION_RADIUS: f64 = 0.45;
const HIT_WIDTH: f64 = 2.0;

#[derive(Debug, PartialEq, Eq)]
pub enum ViewError {
    /// The design is not consistent, so there is nothing sensible to draw.
    Design(DesignError),
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Design(e) => write!(f, "cannot draw the design: {e}"),
        }
    }
}

impl std::error::Error for ViewError {}

impl From<DesignError> for ViewError {
    fn from(e: DesignError) -> Self {
        Self::Design(e)
    }
}

const STYLE: &str = "\
.schematic{--sheet:#fbfaf3;--wire:#0a7a3c;--body:#7b2020;--fill:#fff6bd;--pin:#7b2020;--text:#1d4f5e;--net:#075c2b;--hit:#e8590c;--muted:#8a8f98;--bad:#d92d20;font-family:'Segoe UI',Roboto,Helvetica,Arial,sans-serif}
.schematic .sheet{fill:var(--sheet)}
.schematic .module-frame{fill:#0a7a3c0a;stroke:#6f9a86;stroke-width:1.2;stroke-dasharray:6 4;vector-effect:non-scaling-stroke}
.schematic .module-title{fill:#2f5f4a;font-weight:700}
.schematic .module-sheet:hover .module-frame{fill:#0a7a3c18}
.schematic text{fill:var(--text);stroke:var(--sheet);stroke-width:.5;stroke-linejoin:round;paint-order:stroke}
.schematic .nc line,.schematic .nc circle{fill:none;stroke:var(--muted);stroke-width:.25;stroke-linecap:round}
.schematic .nc.must line,.schematic .nc.must circle{stroke:var(--bad);stroke-width:.35}
.schematic .nc .hit{stroke:transparent;stroke-width:2.4;pointer-events:all}
.schematic .port .flag{fill:none;stroke:var(--wire);stroke-width:.25;stroke-linejoin:round}
.schematic .port text{stroke:none}
.schematic .net-label{fill:var(--net);font-weight:600}
.schematic .bus .line{stroke-width:.9}
.schematic .bus-tick{stroke:var(--wire);stroke-width:.3;stroke-linecap:round}
.schematic .bus-flag .flag{fill:none;stroke:var(--wire);stroke-width:.5;stroke-linejoin:round}
.schematic .bus-label{fill:var(--net);font-weight:600}
.schematic .bus-count{fill:var(--net);font-weight:600}
.schematic .bus-members{font-weight:400}
.schematic.has-on .bus-group:not(.on){opacity:.22}
.schematic .bus-group.on .bus .line{stroke:var(--hit);stroke-width:.9}
.schematic .bus-group.on .bus-tick,.schematic .bus-group.on .bus-flag .flag{stroke:var(--hit)}
.schematic .ref{font-weight:700}
.schematic .pin-name,.schematic .pin-number{fill:var(--pin)}
.schematic .line{fill:none;stroke:var(--wire);stroke-linecap:round;stroke-linejoin:round}
.schematic .hit{fill:none;stroke:transparent;pointer-events:stroke}
.schematic .junction{fill:var(--wire)}
.schematic .pin-line{stroke:var(--pin);stroke-linecap:round}
.schematic .pin-end{fill:transparent;stroke:none}
.schematic .s{stroke:var(--body);stroke-linecap:round;stroke-linejoin:round}
.schematic .f-none{fill:none}
.schematic .f-outline{fill:var(--body)}
.schematic .f-background{fill:var(--fill)}
.schematic .missing .s{stroke-dasharray:1 .6}
.schematic.has-on .wire:not(.on),.schematic.has-on .power:not(.on),.schematic.has-on .port:not(.on),.schematic.has-on .junction:not(.on),.schematic.has-on .net-label:not(.on),.schematic.has-on .part:not(.on):not(.touches){opacity:.22}
.schematic .on .line,.schematic .junction.on,.schematic .on .pin-line{stroke:var(--hit);stroke-width:.5}
.schematic .junction.on{fill:var(--hit)}
.schematic text.on,.schematic .on text{fill:var(--hit)}
.schematic .part.on .s{stroke:var(--hit)}
.schematic .pin.on .pin-line{stroke:var(--hit);stroke-width:.5}
";

/// What a note is drawn with by default; the page maps the tokens (`--note-fill`, `--note-stroke`,
/// `--note-text`, `--note-stroke-text`) to its own. Written only into a drawing that has notes,
/// so a drawing without any is the same bytes as before notes existed.
const NOTE_STYLE: &str = "\
.schematic .note-frame{fill:var(--note-fill,#fff9db);stroke:var(--note-stroke,#d4b95e);stroke-width:.25}\n\
.schematic .note text{fill:var(--note-text,#4a4120);stroke:none;font-weight:400}\n\
.schematic .note .note-heading{fill:var(--note-stroke-text,#8a6d1b);font-weight:700;letter-spacing:.02em}\n\
.schematic .note-relation .note-text{font-family:ui-monospace,'SF Mono',Menlo,Consolas,'Liberation Mono',monospace}\n\
.schematic.has-on .note:not(.on){opacity:.22}\n\
.schematic .note.on .note-frame{stroke:var(--hit);stroke-width:.5}\n";

/// Draw `design` as one SVG document.
pub fn render_svg(
    design: &Design,
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Result<String, ViewError> {
    let l = layout(design, mapping, lib)?;
    Ok(draw(design, &l))
}

/// Draw `design` with its parts in boxes by `groups` (see [`crate::sheets`]).
pub fn render_svg_by_module(
    design: &Design,
    groups: &[Group],
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Result<String, ViewError> {
    let l = layout_sheets(design, groups, mapping, lib)?;
    Ok(draw(design, &l))
}

/// [`render_svg_by_module`] with the rails known: their pins get power symbols.
pub fn render_svg_by_module_with(
    design: &Design,
    groups: &[Group],
    rails: &Rails,
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Result<String, ViewError> {
    let l = layout_sheets_rails(design, groups, rails, mapping, lib)?;
    Ok(draw(design, &l))
}

/// [`render_svg_by_module_with`] with the pins that must be connected told.
pub fn render_svg_by_module_marked(
    design: &Design,
    groups: &[Group],
    rails: &Rails,
    marks: &Marks,
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Result<String, ViewError> {
    let l = layout_sheets_marked(design, groups, rails, marks, mapping, lib)?;
    Ok(draw(design, &l))
}

/// [`render_svg`] with the rails known.
pub fn render_svg_with(
    design: &Design,
    rails: &Rails,
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Result<String, ViewError> {
    let l = layout_rails(design, rails, mapping, lib)?;
    Ok(draw(design, &l))
}

/// Draw a layout already made (by [`crate::layout_sheets_hinted`], say) as one SVG document,
/// as [`render_svg_by_module_marked`] draws its own.
pub fn render_layout(design: &Design, l: &Layout) -> String {
    draw(design, l)
}

fn draw(design: &Design, l: &Layout) -> String {
    let b = l.bounds;
    let view = format!(
        "{} {} {} {}",
        n(b.min.x),
        n(b.min.y),
        n(b.width()),
        n(b.height())
    );
    let mut out = String::new();
    let w = &mut out;
    let _ = writeln!(
        w,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"schematic\" viewBox=\"{view}\" data-board=\"{}\" data-bounds=\"{view}\">",
        esc(&design.name)
    );
    let _ = writeln!(w, "<title>{}</title>", esc(&design.name));
    let notes_style = if l.notes.is_empty() { "" } else { NOTE_STYLE };
    let _ = writeln!(w, "<style>\n{STYLE}{notes_style}</style>");
    let _ = writeln!(
        w,
        "<rect class=\"sheet\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
        n(b.min.x),
        n(b.min.y),
        n(b.width()),
        n(b.height())
    );

    if !l.sheets.is_empty() {
        let _ = writeln!(w, "<g class=\"sheets\">");
        for sheet in &l.sheets {
            let f = sheet.frame;
            let _ = writeln!(
                w,
                "<g class=\"module-sheet\" data-module=\"{}\" data-bounds=\"{} {} {} {}\"><rect class=\"module-frame\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                esc(&sheet.name),
                n(f.min.x),
                n(f.min.y),
                n(f.width()),
                n(f.height()),
                n(f.min.x),
                n(f.min.y),
                n(f.width()),
                n(f.height())
            );
            text_element(w, &sheet.title, "module-title", TITLE_SIZE, "");
            let _ = writeln!(w, "</g>");
        }
        let _ = writeln!(w, "</g>");
    }

    let _ = writeln!(w, "<g class=\"parts\">");
    for p in &l.parts {
        draw_part(w, design, p);
    }
    let _ = writeln!(w, "</g>");

    let _ = writeln!(w, "<g class=\"wires\">");
    for wire in &l.wires {
        let points = wire
            .points
            .iter()
            .map(|p| format!("{},{}", n(p.x), n(p.y)))
            .collect::<Vec<_>>()
            .join(" ");
        let entry = l.buses.iter().any(|b| {
            b.entries.iter().any(|e| {
                e.net == wire.net
                    && wire
                        .points
                        .last()
                        .is_some_and(|p| (p.x - e.at.x).abs() < 1e-6 && (p.y - e.at.y).abs() < 1e-6)
            })
        });
        let class = if entry { "wire bus-entry" } else { "wire" };
        let _ = writeln!(
            w,
            "<g class=\"{class}\" data-net=\"{}\"><polyline class=\"hit\" stroke-width=\"{HIT_WIDTH}\" points=\"{points}\"/><polyline class=\"line\" stroke-width=\"{WIRE_WIDTH}\" points=\"{points}\"/></g>",
            esc(&design.nets[wire.net].name)
        );
    }
    let _ = writeln!(w, "</g>");

    let _ = writeln!(w, "<g class=\"buses\">");
    for bus in &l.buses {
        draw_bus(w, design, bus);
    }
    let _ = writeln!(w, "</g>");

    let _ = writeln!(w, "<g class=\"junctions\">");
    for j in &l.junctions {
        let _ = writeln!(
            w,
            "<circle class=\"junction\" data-net=\"{}\" cx=\"{}\" cy=\"{}\" r=\"{JUNCTION_RADIUS}\"/>",
            esc(&design.nets[j.net].name),
            n(j.at.x),
            n(j.at.y)
        );
    }
    let _ = writeln!(w, "</g>");

    let _ = writeln!(w, "<g class=\"power-symbols\">");
    for symbol in &l.symbols {
        draw_power(w, design, symbol);
    }
    let _ = writeln!(w, "</g>");

    let _ = writeln!(w, "<g class=\"ports\">");
    for port in &l.ports {
        draw_port(w, design, port);
    }
    let _ = writeln!(w, "</g>");

    let _ = writeln!(w, "<g class=\"unconnected\">");
    for mark in &l.marks {
        draw_mark(w, design, mark);
    }
    let _ = writeln!(w, "</g>");

    if !l.notes.is_empty() {
        let _ = writeln!(w, "<g class=\"notes\">");
        for note in &l.notes {
            draw_note(w, design, note);
        }
        let _ = writeln!(w, "</g>");
    }

    let _ = writeln!(w, "<g class=\"labels\">");
    for label in &l.labels {
        let name = &design.nets[label.net].name;
        text_element_titled(
            w,
            &label.text,
            "net-label",
            1.27,
            &format!(" data-net=\"{}\"", esc(name)),
            (label.text.text != *name).then_some(name.as_str()),
        );
    }
    let _ = writeln!(w, "</g>");
    let _ = writeln!(w, "</svg>");
    out
}

/// A note: a rounded frame and its text as one `<text>` with a `<tspan>` for each line, so it
/// can be selected. `data-note` is the path of what it belongs to (`""` for the board),
/// `data-n` the note's index in the design's notes, `data-ref` the reference of the part it is
/// about when it is about one, `data-at` the `file:line` it is written at; the tooltip has the
/// whole text when the drawn text is cut.
fn draw_note(w: &mut String, design: &Design, note: &crate::layout::PlacedNote) {
    use netsmith_core::NoteKind;
    let kind = match note.kind {
        NoteKind::Doc => "note-doc",
        NoteKind::Relation => "note-relation",
    };
    let source = &design.notes[note.note];
    let mut attrs = format!(
        " data-note=\"{}\" data-n=\"{}\"",
        esc(&note.path),
        note.note
    );
    if let Some(a) = note.anchor {
        let _ = write!(attrs, " data-ref=\"{}\"", esc(&design.parts[a].refdes));
    }
    if let Some(at) = &source.at {
        let _ = write!(attrs, " data-at=\"{}:{}\"", esc(&at.file), at.line);
    }
    let _ = writeln!(w, "<g class=\"note {kind}\"{attrs}>");
    if let Some(full) = &note.full {
        let _ = writeln!(w, "<title>{}</title>", esc(full));
    }
    let f = note.frame;
    let _ = writeln!(
        w,
        "<rect class=\"note-frame\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" ry=\"{}\"/>",
        n(f.min.x),
        n(f.min.y),
        n(f.width()),
        n(f.height()),
        n(crate::layout::RADIUS),
        n(crate::layout::RADIUS)
    );
    if let Some(h) = &note.heading {
        let _ = writeln!(
            w,
            "<text class=\"note-heading\" font-size=\"{}\" x=\"{}\" y=\"{}\">{}</text>",
            n(crate::layout::HEADING_TEXT),
            n(h.at.x),
            n(h.at.y),
            esc(&h.text)
        );
    }
    if let Some(first) = note.lines.first() {
        let _ = write!(
            w,
            "<text class=\"note-text\" font-size=\"{}\" x=\"{}\" y=\"{}\" xml:space=\"preserve\">",
            n(note.size),
            n(first.at.x),
            n(first.at.y)
        );
        for line in &note.lines {
            let _ = write!(
                w,
                "<tspan x=\"{}\" y=\"{}\">{}</tspan>",
                n(line.at.x),
                n(line.at.y),
                esc(&line.text)
            );
        }
        let _ = writeln!(w, "</text>");
    }
    let _ = writeln!(w, "</g>");
}

/// A bus: the thick line, the tick with the count of members, the name and the list of members,
/// and the flag where the members go on to another module. `data-bus` is the interface
/// instance's path, `data-type` its definition, `data-nets` the members' nets (a space between
/// them) and `data-members` the members as JSON, `[["tx","MODEM_TX"],...]`, in the order of the
/// pins: what a page that follows a bus reads (names may contain spaces, so it reads this one).
fn draw_bus(w: &mut String, design: &Design, bus: &crate::layout::Bus) {
    let nets: Vec<String> = bus
        .members
        .iter()
        .map(|m| design.nets[m.net].name.clone())
        .collect();
    let members: Vec<String> = bus
        .members
        .iter()
        .map(|m| {
            format!(
                "[{},{}]",
                json_string(&m.name),
                json_string(&design.nets[m.net].name)
            )
        })
        .collect();
    let _ = writeln!(
        w,
        "<g class=\"bus-group\" data-bus=\"{}\" data-type=\"{}\" data-nets=\"{}\" data-members=\"{}\"><title>{} ({}): {} members</title>",
        esc(&bus.path),
        esc(&bus.type_name),
        esc(&nets.join(" ")),
        esc(&format!("[{}]", members.join(","))),
        esc(&bus.path),
        esc(&bus.type_name),
        bus.members.len()
    );
    let d: String = bus
        .lines
        .iter()
        .map(|line| {
            line.iter()
                .enumerate()
                .map(|(i, p)| format!("{}{} {}", if i == 0 { "M" } else { "L" }, n(p.x), n(p.y)))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let _ = writeln!(
        w,
        "<g class=\"bus\" data-bus=\"{}\"><path class=\"hit\" stroke-width=\"{HIT_WIDTH}\" d=\"{d}\"/><path class=\"line\" d=\"{d}\"/></g>",
        esc(&bus.path)
    );
    let _ = writeln!(
        w,
        "<line class=\"bus-tick\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>",
        n(bus.tick.0.x),
        n(bus.tick.0.y),
        n(bus.tick.1.x),
        n(bus.tick.1.y)
    );
    let attrs = format!(" data-bus=\"{}\"", esc(&bus.path));
    text_element(w, &bus.count, "bus-count", 1.27, &attrs);
    text_element(w, &bus.label, "bus-label", 1.27, &attrs);
    text_element(w, &bus.list, "bus-label bus-members", 1.27, &attrs);
    if let Some(o) = &bus.other {
        text_element(w, o, "bus-label bus-members", 1.27, &attrs);
    }
    if let Some(f) = &bus.flag {
        let (dx, dy) = f.dir.vector();
        let at = |a: f64, b: f64| {
            format!(
                "{},{}",
                n(f.at.x + dx * a - dy * b),
                n(f.at.y + dy * a + dx * b)
            )
        };
        let half = Port::HALF;
        let points = [
            at(0.0, 0.0),
            at(half, -half),
            at(f.length, -half),
            at(f.length, half),
            at(half, half),
        ]
        .join(" ");
        let _ = writeln!(
            w,
            "<g class=\"bus-flag\" data-bus=\"{}\"><title>{} (goes to another module)</title><polygon class=\"flag\" points=\"{points}\"/>",
            esc(&bus.path),
            esc(&bus.path)
        );
        text_element(w, &f.label, "bus-label", Port::TEXT, &attrs);
        let _ = writeln!(w, "</g>");
    }
    let _ = writeln!(w, "</g>");
}

/// A flag where a net goes to another module, in the shape of KiCad's hierarchical label: it
/// points at the wire.
fn draw_port(w: &mut String, design: &Design, port: &Port) {
    let name = &design.nets[port.net].name;
    let (dx, dy) = port.dir.vector();
    let at = |a: f64, b: f64| {
        format!(
            "{},{}",
            n(port.at.x + dx * a - dy * b),
            n(port.at.y + dy * a + dx * b)
        )
    };
    let l = port.length;
    let points = [
        at(0.0, 0.0),
        at(Port::HALF, -Port::HALF),
        at(l, -Port::HALF),
        at(l, Port::HALF),
        at(Port::HALF, Port::HALF),
    ]
    .join(" ");
    let _ = writeln!(
        w,
        "<g class=\"port\" data-net=\"{}\"><title>{} (goes to another module)</title><polygon class=\"flag\" points=\"{points}\"/>",
        esc(name),
        esc(name)
    );
    if let Some(label) = &port.label {
        let shown = (label.text != *name).then_some(name.as_str());
        text_element_titled(w, label, "net-label", Port::TEXT, "", shown);
    }
    let _ = writeln!(w, "</g>");
}

/// The X KiCad draws at a pin that connects to nothing; a pin that must be connected has a
/// red ring as well, and the checker's message.
fn draw_mark(w: &mut String, design: &Design, m: &PinMark) {
    let part = &design.parts[m.part];
    let (class, tip) = match &m.kind {
        MarkKind::Open => (
            "nc",
            format!(
                "not connected: {} {} ({})",
                part.refdes, m.pin_name, m.pin_kind
            ),
        ),
        MarkKind::Must(message) => (
            "nc must",
            format!("{} {}: {message}", part.refdes, m.pin_name),
        ),
    };
    let (x, y) = (m.at.x, m.at.y);
    let r = crate::sheetview::MARK_HALF;
    let _ = writeln!(
        w,
        "<g class=\"{class}\" data-unconnected=\"{}.{}\"><title>{}</title><circle class=\"hit\" cx=\"{}\" cy=\"{}\" r=\"1.2\"/><line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/><line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>{}</g>",
        esc(&part.refdes),
        esc(&m.pad),
        esc(&tip),
        n(x),
        n(y),
        n(x - r),
        n(y - r),
        n(x + r),
        n(y + r),
        n(x - r),
        n(y + r),
        n(x + r),
        n(y - r),
        if matches!(m.kind, MarkKind::Must(_)) {
            format!("<circle cx=\"{}\" cy=\"{}\" r=\"1.4\"/>", n(x), n(y))
        } else {
            String::new()
        }
    );
}

/// A power symbol in KiCad's manner: a ground is bars that shrink away from the wire, a
/// supply is a stem with an arrow head; the name is beside it.
fn draw_power(w: &mut String, design: &Design, s: &PowerSymbol) {
    let name = &design.nets[s.net].name;
    let (class, what) = match s.kind {
        PowerKind::Ground => ("power ground", "ground"),
        PowerKind::Supply => ("power supply", "supply"),
    };
    let _ = writeln!(
        w,
        "<g class=\"{class}\" data-net=\"{}\"><title>{} ({what})</title>",
        esc(name),
        esc(name)
    );
    let (x, y) = (s.at.x, s.at.y);
    let dy = if s.dir == Dir::Down { 1.0 } else { -1.0 };
    let line = |w: &mut String, points: &[(f64, f64)]| {
        let points = points
            .iter()
            .map(|(px, py)| format!("{},{}", n(*px), n(*py)))
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            w,
            "<polyline class=\"line\" stroke-width=\"{WIRE_WIDTH}\" points=\"{points}\"/>"
        );
    };
    match s.kind {
        PowerKind::Ground => {
            for (k, half) in [1.5, 1.0, 0.5].into_iter().enumerate() {
                let by = y + dy * 0.9 * k as f64;
                line(w, &[(x - half, by), (x + half, by)]);
            }
        }
        PowerKind::Supply => {
            let tip = y + dy * 1.8;
            line(w, &[(x, y), (x, tip)]);
            line(
                w,
                &[
                    (x - 0.9, tip - dy * 0.9),
                    (x, tip),
                    (x + 0.9, tip - dy * 0.9),
                ],
            );
        }
    }
    if let Some(label) = &s.label {
        let shown = (label.text != *name).then_some(name.as_str());
        text_element_titled(w, label, "net-label", 1.27, "", shown);
    }
    let _ = writeln!(w, "</g>");
}

fn draw_part(w: &mut String, design: &Design, p: &PlacedPart) {
    let part = &design.parts[p.part];
    let class = if p.glyph.symbol.is_some() {
        "part"
    } else {
        "part missing"
    };
    let unit = p
        .glyph
        .unit
        .map(crate::layout::unit_letter)
        .unwrap_or_default();
    let unit_attr = if unit.is_empty() {
        String::new()
    } else {
        format!(" data-unit=\"{unit}\"")
    };
    // Where the symbol's origin is, how it is turned and whether it is the sheet's reference
    // part: what a page that moves a part writes into the layout file (ADR 0061) needs.
    let (mirror, turns) = p.orient.mirror_and_turns();
    let anchor = if p.anchor { " data-anchor=\"1\"" } else { "" };
    let _ = writeln!(
        w,
        "<g class=\"{class}\" data-ref=\"{}\" data-kind=\"{}\"{unit_attr} data-origin=\"{} {}\" data-orient=\"{} {turns}\"{anchor}>",
        esc(&part.refdes),
        esc(&part.kind),
        n(p.at.x),
        n(p.at.y),
        u8::from(mirror)
    );
    let mut summary = format!("{} {} {}", part.refdes, part.kind, part.value);
    if let Some(package) = &part.package {
        let _ = write!(summary, " ({package})");
    }
    if !unit.is_empty() {
        let _ = write!(summary, ", unit {unit}");
    }
    if let Some(why) = &p.glyph.missing {
        let _ = write!(summary, "\nNo symbol: {why}");
    }
    let _ = writeln!(w, "<title>{}</title>", esc(&summary));

    let at = |q: P| -> Pt {
        let r = p.orient.apply(q);
        Pt::new(p.at.x + r.x, p.at.y + r.y)
    };
    for shape in &p.glyph.shapes {
        draw_shape(w, shape, &at);
    }
    for (pin, placed) in p.glyph.pins.iter().zip(&p.pins) {
        if pin.hidden {
            continue;
        }
        let net = placed
            .net
            .map(|i| format!(" data-net=\"{}\"", esc(&design.nets[i].name)))
            .unwrap_or_default();
        let _ = writeln!(
            w,
            "<g class=\"pin\" data-ref=\"{}\" data-pin=\"{}\"{net}>",
            esc(&part.refdes),
            esc(&placed.id)
        );
        draw_pin(w, pin, placed.at, p, &p.orient);
        let _ = writeln!(w, "</g>");
    }
    text_element(w, &p.reference, "ref", 1.27, "");
    if let Some(v) = &p.value {
        text_element(w, v, "value", 1.27, "");
    }
    let _ = writeln!(w, "</g>");
}

fn draw_pin(w: &mut String, pin: &GlyphPin, at: Pt, part: &PlacedPart, orient: &Orient) {
    let toward = orient.dir(pin.toward_body);
    let inner = at.step(toward, pin.length);
    let _ = writeln!(
        w,
        "<line class=\"pin-line\" stroke-width=\"{MIN_LINE}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>",
        n(at.x),
        n(at.y),
        n(inner.x),
        n(inner.y)
    );
    let _ = writeln!(
        w,
        "<circle class=\"pin-end\" cx=\"{}\" cy=\"{}\" r=\"1.1\"/>",
        n(at.x),
        n(at.y)
    );
    let (number, name) = part.glyph.pin_texts(pin, at, toward);
    let size = part.glyph.pin_text;
    if let Some(t) = number {
        text_element(w, &t, "pin-number", size, "");
    }
    if let Some(t) = name {
        text_element(w, &t, "pin-name", size, "");
    }
}

fn draw_shape(w: &mut String, shape: &Shape, at: &dyn Fn(P) -> Pt) {
    let class = |fill: Fill| match fill {
        Fill::None => "s f-none",
        Fill::Outline => "s f-outline",
        Fill::Background => "s f-background",
    };
    let sw = |width: f64| n(width.max(MIN_LINE));
    match shape {
        Shape::Rect { a, b, width, fill } => {
            let (p, q) = (at(*a), at(*b));
            let _ = writeln!(
                w,
                "<rect class=\"{}\" stroke-width=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                class(*fill),
                sw(*width),
                n(p.x.min(q.x)),
                n(p.y.min(q.y)),
                n((p.x - q.x).abs()),
                n((p.y - q.y).abs())
            );
        }
        Shape::Poly { pts, width, fill } => {
            let points = pts
                .iter()
                .map(|&q| {
                    let p = at(q);
                    format!("{},{}", n(p.x), n(p.y))
                })
                .collect::<Vec<_>>()
                .join(" ");
            let _ = writeln!(
                w,
                "<polyline class=\"{}\" stroke-width=\"{}\" points=\"{points}\"/>",
                class(*fill),
                sw(*width)
            );
        }
        Shape::Circle {
            center,
            radius,
            width,
            fill,
        } => {
            let c = at(*center);
            let _ = writeln!(
                w,
                "<circle class=\"{}\" stroke-width=\"{}\" cx=\"{}\" cy=\"{}\" r=\"{}\"/>",
                class(*fill),
                sw(*width),
                n(c.x),
                n(c.y),
                n(*radius)
            );
        }
        Shape::Arc {
            start,
            mid,
            end,
            width,
            fill,
        } => {
            let _ = writeln!(
                w,
                "<path class=\"{}\" stroke-width=\"{}\" d=\"{}\"/>",
                class(*fill),
                sw(*width),
                arc_path(at(*start), at(*mid), at(*end))
            );
        }
        Shape::Text {
            at: pos,
            text,
            size,
        } => {
            let p = at(*pos);
            let t = Text {
                text: text.clone(),
                at: Pt::new(p.x, p.y + size * 0.35),
                anchor: Anchor::Middle,
                vertical: false,
            };
            text_element(w, &t, "pin-name", *size, "");
        }
    }
}

/// An SVG path for the circular arc from `start` through `mid` to `end`, all on the sheet.
fn arc_path(start: Pt, mid: Pt, end: Pt) -> String {
    let cross = (mid.x - start.x) * (end.y - mid.y) - (mid.y - start.y) * (end.x - mid.x);
    let line = || {
        format!(
            "M {} {} L {} {}",
            n(start.x),
            n(start.y),
            n(end.x),
            n(end.y)
        )
    };
    if cross.abs() < 1e-9 {
        return line();
    }
    // Circle through three points.
    let d =
        2.0 * (start.x * (mid.y - end.y) + mid.x * (end.y - start.y) + end.x * (start.y - mid.y));
    let sq = |p: Pt| p.x * p.x + p.y * p.y;
    let cx =
        (sq(start) * (mid.y - end.y) + sq(mid) * (end.y - start.y) + sq(end) * (start.y - mid.y))
            / d;
    let cy =
        (sq(start) * (end.x - mid.x) + sq(mid) * (start.x - end.x) + sq(end) * (mid.x - start.x))
            / d;
    let r = ((start.x - cx).powi(2) + (start.y - cy).powi(2)).sqrt();
    // On the sheet y grows downward, so a positive cross product turns clockwise, which
    // SVG calls the positive sweep.
    let sweep = cross > 0.0;
    let angle = |p: Pt| (p.y - cy).atan2(p.x - cx);
    let tau = std::f64::consts::TAU;
    let swept = if sweep {
        (angle(end) - angle(start)).rem_euclid(tau)
    } else {
        (angle(start) - angle(end)).rem_euclid(tau)
    };
    format!(
        "M {} {} A {} {} 0 {} {} {} {}",
        n(start.x),
        n(start.y),
        n(r),
        n(r),
        u8::from(swept > std::f64::consts::PI),
        u8::from(sweep),
        n(end.x),
        n(end.y)
    )
}

fn text_element(w: &mut String, t: &Text, class: &str, size: f64, attrs: &str) {
    text_element_titled(w, t, class, size, attrs, None);
}

/// Text, with `title` (the whole of a name that was cut) as its tooltip.
fn text_element_titled(
    w: &mut String,
    t: &Text,
    class: &str,
    size: f64,
    attrs: &str,
    title: Option<&str>,
) {
    let anchor = match t.anchor {
        Anchor::Start => "start",
        Anchor::Middle => "middle",
        Anchor::End => "end",
    };
    let _ = writeln!(
        w,
        "<text class=\"{class}\"{attrs}{} font-size=\"{}\" text-anchor=\"{anchor}\" x=\"{}\" y=\"{}\">{}{}</text>",
        if t.vertical {
            format!(" transform=\"rotate(-90 {} {})\"", n(t.at.x), n(t.at.y))
        } else {
            String::new()
        },
        n(size),
        n(t.at.x),
        n(t.at.y),
        title.map_or(String::new(), |full| format!(
            "<title>{}</title>",
            esc(full)
        )),
        overbars(&t.text)
    );
}

/// `text` escaped, with each `~{...}` (KiCad's overbar, for an active-low name) written as
/// its letters under a line.
fn overbars(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("~{") {
        let Some(len) = rest[start + 2..].find('}') else {
            break;
        };
        out.push_str(&esc(&rest[..start]));
        let inner = &rest[start + 2..start + 2 + len];
        out.push_str(&format!(
            "<tspan text-decoration=\"overline\">{}</tspan>",
            esc(inner)
        ));
        rest = &rest[start + 2 + len + 1..];
    }
    out.push_str(&esc(rest));
    out
}

/// A number with at most three decimals and no trailing zeros, never `-0`.
fn n(v: f64) -> String {
    let mut s = format!("{v:.3}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" { "0".to_string() } else { s }
}

/// `s` as a JSON string.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short_and_never_negative_zero() {
        assert_eq!(n(1.0), "1");
        assert_eq!(n(-0.0001), "0");
        assert_eq!(n(2.5400001), "2.54");
        assert_eq!(n(-3.81), "-3.81");
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(esc("a<b & \"c\""), "a&lt;b &amp; &quot;c&quot;");
    }

    #[test]
    fn a_half_circle_is_an_arc_with_the_right_sweep() {
        // Left to right over the top on a y-down sheet turns clockwise.
        let d = arc_path(Pt::new(-1.0, 0.0), Pt::new(0.0, -1.0), Pt::new(1.0, 0.0));
        assert_eq!(d, "M -1 0 A 1 1 0 0 1 1 0");
        // Over the bottom turns the other way.
        let d = arc_path(Pt::new(-1.0, 0.0), Pt::new(0.0, 1.0), Pt::new(1.0, 0.0));
        assert_eq!(d, "M -1 0 A 1 1 0 0 0 1 0");
    }

    #[test]
    fn three_points_in_a_line_are_a_line() {
        let d = arc_path(Pt::new(0.0, 0.0), Pt::new(1.0, 0.0), Pt::new(2.0, 0.0));
        assert_eq!(d, "M 0 0 L 2 0");
    }
}
