//! Pictures as a PDF: one page for each [`Page`], each sized to its region of its picture
//! with a margin and a title block under it, the region's items drawn as vector paths and text.
//!
//! The text is in the PDF's standard fonts, which every reader has, so none is embedded:
//! Helvetica and Helvetica-Bold (Courier and Courier-Bold for [`Face::Mono`]) with the Windows
//! Latin encoding, and Symbol for the Greek letters (`Ω`, `μ`); a letter none of them has is
//! written `?`. Text can be searched and copied. A colour that is not opaque is drawn through a
//! graphics state with its opacity. Each page draws only the items that reach its region, and
//! clips them to it.
//!
//! The file is PDF 1.4, its content streams compressed with netsmith's own deflate
//! (`netsmith_core::png::deflate`). It holds no date and no identifier: the same pages give the
//! same bytes.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::metrics;
use crate::{Anchor, Cap, Face, FillRule, Item, Join, Picture, Pt, Rect, Rgba, Seg, Shape, Text};

/// The white space round a page's drawing, in millimetres.
pub const MARGIN: f64 = 10.0;
/// The width of the title block, in millimetres.
pub const TITLE_WIDTH: f64 = 80.0;
/// The space between the drawing and the title block.
const GAP: f64 = 5.0;
/// The height of the title block's heading and of each of its other lines.
const HEADING: f64 = 5.5;
const LINE: f64 = 3.6;
/// Points to a millimetre.
const PT: f64 = 72.0 / 25.4;

/// The words of a page's title block: its heading (the sheet's name) in bold, then a line
/// for each of the rest.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleBlock {
    pub heading: String,
    pub lines: Vec<String>,
}

/// One page: the region `view` of `picture` (its units taken as millimetres), drawn at full
/// size, and its title block.
#[derive(Debug, Clone, PartialEq)]
pub struct Page<'a> {
    pub picture: &'a Picture,
    pub view: Rect,
    pub title: TitleBlock,
}

impl Page<'_> {
    /// The page's width and height, in millimetres: the region, the margin round it, and the
    /// title block under it.
    pub fn size(&self) -> (f64, f64) {
        let width = self.view.width().max(TITLE_WIDTH) + 2.0 * MARGIN;
        let height = self.view.height() + 2.0 * MARGIN + GAP + self.title_height();
        (width, height)
    }

    fn title_height(&self) -> f64 {
        2.0 + HEADING + LINE * self.title.lines.len() as f64 + 1.0
    }
}

/// A document: its title (the reader's window shows it), what made it, and its pages.
#[derive(Debug, Clone, PartialEq)]
pub struct Document<'a> {
    pub title: String,
    pub producer: String,
    pub pages: Vec<Page<'a>>,
}

/// The bytes of `doc` as a PDF file.
pub fn write(doc: &Document) -> Vec<u8> {
    let mut states: BTreeSet<(u8, u8)> = BTreeSet::new();
    let contents: Vec<(f64, f64, String)> = doc
        .pages
        .iter()
        .map(|page| {
            let (w, h) = page.size();
            let mut c = Content {
                out: String::new(),
                state: (255, 255),
                states: &mut states,
            };
            c.page(page, h);
            (w, h, c.out)
        })
        .collect();

    let mut file = Writer::default();
    file.raw(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");
    let pages_at = 2;
    let resources_at = 4;
    let first_page = 5 + FONTS.len();
    let kids: Vec<String> = (0..contents.len())
        .map(|i| format!("{} 0 R", first_page + 2 * i))
        .collect();
    file.object(1, &format!("<< /Type /Catalog /Pages {pages_at} 0 R >>"));
    file.object(
        pages_at,
        &format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            contents.len()
        ),
    );
    file.object(
        3,
        &format!(
            "<< /Title {} /Producer {} >>",
            text_string(&doc.title),
            text_string(&doc.producer)
        ),
    );
    let fonts: Vec<String> = FONTS
        .iter()
        .enumerate()
        .map(|(i, (name, _, _))| format!("/{name} {} 0 R", 5 + i))
        .collect();
    let gs: Vec<String> = states
        .iter()
        .map(|&(f, s)| {
            format!(
                "/{} << /Type /ExtGState /ca {} /CA {} >>",
                state_name((f, s)),
                n(f64::from(f) / 255.0),
                n(f64::from(s) / 255.0)
            )
        })
        .collect();
    file.object(
        resources_at,
        &format!(
            "<< /ProcSet [/PDF /Text] /Font << {} >> /ExtGState << {} >> >>",
            fonts.join(" "),
            gs.join(" ")
        ),
    );
    for (i, (_, base, encoding)) in FONTS.iter().enumerate() {
        let encoding = if *encoding {
            " /Encoding /WinAnsiEncoding"
        } else {
            ""
        };
        file.object(
            5 + i,
            &format!("<< /Type /Font /Subtype /Type1 /BaseFont /{base}{encoding} >>"),
        );
    }
    for (i, (w, h, content)) in contents.iter().enumerate() {
        let page = first_page + 2 * i;
        file.object(
            page,
            &format!(
                "<< /Type /Page /Parent {pages_at} 0 R /MediaBox [0 0 {} {}] /Resources {resources_at} 0 R /Contents {} 0 R >>",
                n(w * PT),
                n(h * PT),
                page + 1
            ),
        );
        let mut z = vec![0x78, 0x01];
        z.extend_from_slice(&netsmith_core::png::deflate(content.as_bytes()));
        z.extend_from_slice(&netsmith_core::png::adler32(content.as_bytes()).to_be_bytes());
        file.stream(
            page + 1,
            &format!("<< /Length {} /Filter /FlateDecode >>", z.len()),
            &z,
        );
    }
    file.finish(1, 3)
}

/// The fonts every page may use: the resource name, the font, and whether it takes the
/// Windows Latin encoding.
const FONTS: [(&str, &str, bool); 5] = [
    ("F1", "Helvetica", true),
    ("F2", "Helvetica-Bold", true),
    ("F3", "Courier", true),
    ("F4", "Courier-Bold", true),
    ("F5", "Symbol", false),
];

fn font_of(face: Face) -> &'static str {
    match face {
        Face::Sans => "F1",
        Face::SansBold => "F2",
        Face::Mono => "F3",
        Face::MonoBold => "F4",
    }
}

fn state_name((fill, stroke): (u8, u8)) -> String {
    format!("A{fill}x{stroke}")
}

#[derive(Default)]
struct Writer {
    bytes: Vec<u8>,
    /// Where each object starts, by its number.
    offsets: Vec<(usize, usize)>,
}

impl Writer {
    fn raw(&mut self, b: &[u8]) {
        self.bytes.extend_from_slice(b);
    }

    fn object(&mut self, number: usize, body: &str) {
        self.offsets.push((number, self.bytes.len()));
        self.raw(format!("{number} 0 obj\n{body}\nendobj\n").as_bytes());
    }

    fn stream(&mut self, number: usize, dict: &str, data: &[u8]) {
        self.offsets.push((number, self.bytes.len()));
        self.raw(format!("{number} 0 obj\n{dict}\nstream\n").as_bytes());
        self.raw(data);
        self.raw(b"\nendstream\nendobj\n");
    }

    fn finish(mut self, root: usize, info: usize) -> Vec<u8> {
        self.offsets.sort();
        let size = self.offsets.last().map_or(0, |(n, _)| n + 1);
        let xref = self.bytes.len();
        let mut table = format!("xref\n0 {size}\n0000000000 65535 f \n");
        for number in 1..size {
            let at = self
                .offsets
                .iter()
                .find(|(n, _)| *n == number)
                .map_or(0, |(_, at)| *at);
            let _ = writeln!(table, "{at:010} 00000 n ");
        }
        let _ = write!(
            table,
            "trailer\n<< /Size {size} /Root {root} 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        );
        self.raw(table.as_bytes());
        self.bytes
    }
}

/// A page's content stream as it is written.
struct Content<'a> {
    out: String,
    /// The opacity of fills and strokes now set.
    state: (u8, u8),
    states: &'a mut BTreeSet<(u8, u8)>,
}

impl Content<'_> {
    fn page(&mut self, page: &Page, height: f64) {
        let view = page.view;
        // Millimetres, y down, from the page's top left corner.
        let _ = writeln!(self.out, "{} 0 0 {} 0 {} cm", n(PT), n(-PT), n(height * PT));
        // The drawing: the region moved to the margin and clipped to it.
        let _ = writeln!(
            self.out,
            "q 1 0 0 1 {} {} cm {} {} {} {} re W n",
            n(MARGIN - view.min.x),
            n(MARGIN - view.min.y),
            n(view.min.x),
            n(view.min.y),
            n(view.width()),
            n(view.height())
        );
        for item in &page.picture.items {
            if item.bounds().is_none_or(|b| !b.touches(&view)) {
                continue;
            }
            match item {
                Item::Shape(s) => self.shape(s),
                Item::Text(t) => self.text(t),
            }
        }
        let _ = writeln!(self.out, "Q");
        self.set_state((255, 255));
        self.frame(page);
    }

    /// The thin line round the drawing and the title block with its words.
    fn frame(&mut self, page: &Page) {
        let (w, _) = page.size();
        let ink = Rgba::opaque(60, 60, 60);
        let _ = writeln!(
            self.out,
            "{} RG 0.25 w 0 J 0 j [] 0 d {} {} {} {} re S",
            colour(ink),
            n(MARGIN),
            n(MARGIN),
            n(page.view.width()),
            n(page.view.height())
        );
        let (x, y) = (w - MARGIN - TITLE_WIDTH, MARGIN + page.view.height() + GAP);
        let th = page.title_height();
        let _ = writeln!(
            self.out,
            "0.35 w {} {} {} {} re S",
            n(x),
            n(y),
            n(TITLE_WIDTH),
            n(th)
        );
        let _ = writeln!(
            self.out,
            "{} {} m {} {} l S",
            n(x),
            n(y + 2.0 + HEADING),
            n(x + TITLE_WIDTH),
            n(y + 2.0 + HEADING)
        );
        let mut line = |text: &str, at: f64, size: f64, face: Face| {
            self.text(&Text {
                at: Pt::new(x + 3.0, at),
                angle: 0.0,
                size,
                anchor: Anchor::Start,
                face,
                fill: Some(Rgba::BLACK),
                halo: None,
                runs: vec![crate::Run {
                    text: fit(text, face, size, TITLE_WIDTH - 6.0),
                    overline: false,
                }],
            });
        };
        line(
            &page.title.heading,
            y + 2.0 + HEADING - 1.8,
            3.4,
            Face::SansBold,
        );
        for (i, text) in page.title.lines.iter().enumerate() {
            line(
                text,
                y + 2.0 + HEADING + LINE * (i as f64 + 1.0) - 1.0,
                2.4,
                Face::Sans,
            );
        }
    }

    fn set_state(&mut self, state: (u8, u8)) {
        if self.state != state {
            self.states.insert(state);
            let _ = writeln!(self.out, "/{} gs", state_name(state));
            self.state = state;
        }
    }

    fn shape(&mut self, shape: &Shape) {
        let fill_alpha = shape.fill.map_or(255, |(c, _)| c.a);
        let stroke_alpha = shape.stroke.as_ref().map_or(255, |s| s.colour.a);
        self.set_state((fill_alpha, stroke_alpha));
        if let Some((c, _)) = shape.fill {
            let _ = write!(self.out, "{} rg ", colour(c));
        }
        if let Some(s) = &shape.stroke {
            let cap = match s.cap {
                Cap::Butt => 0,
                Cap::Round => 1,
                Cap::Square => 2,
            };
            let join = match s.join {
                Join::Miter => 0,
                Join::Round => 1,
                Join::Bevel => 2,
            };
            let dash: Vec<String> = s.dash.iter().map(|d| n(*d)).collect();
            let _ = write!(
                self.out,
                "{} RG {} w {cap} J {join} j [{}] 0 d ",
                colour(s.colour),
                n(s.width),
                dash.join(" ")
            );
        }
        for seg in &shape.path.segs {
            match *seg {
                Seg::Move(p) => {
                    let _ = write!(self.out, "{} {} m ", n(p.x), n(p.y));
                }
                Seg::Line(p) => {
                    let _ = write!(self.out, "{} {} l ", n(p.x), n(p.y));
                }
                Seg::Cubic(a, b, c) => {
                    let _ = write!(
                        self.out,
                        "{} {} {} {} {} {} c ",
                        n(a.x),
                        n(a.y),
                        n(b.x),
                        n(b.y),
                        n(c.x),
                        n(c.y)
                    );
                }
                Seg::Close => self.out.push_str("h "),
            }
        }
        let even = shape.fill.is_some_and(|(_, r)| r == FillRule::EvenOdd);
        let op = match (shape.fill.is_some(), shape.stroke.is_some(), even) {
            (true, true, false) => "B",
            (true, true, true) => "B*",
            (true, false, false) => "f",
            (true, false, true) => "f*",
            (false, true, _) => "S",
            (false, false, _) => "n",
        };
        self.out.push_str(op);
        self.out.push('\n');
    }

    fn text(&mut self, text: &Text) {
        let start = text.start();
        let (s, c) = text.angle.to_radians().sin_cos();
        // The text matrix: along the baseline, and up (y is down on this page).
        let matrix = format!(
            "{} {} {} {} {} {} Tm",
            n(c),
            n(s),
            n(s),
            n(-c),
            n(start.x),
            n(start.y)
        );
        let pieces = encode_runs(text);
        let show = |out: &mut String| {
            let _ = writeln!(out, "BT {matrix}");
            let mut font = "";
            for (f, bytes) in &pieces {
                let f = if *f == "F5" { "F5" } else { font_of(text.face) };
                if f != font {
                    let _ = write!(out, "/{f} {} Tf ", n(text.size));
                    font = f;
                }
                let _ = writeln!(out, "{} Tj", literal(bytes));
            }
            out.push_str("ET\n");
        };
        if let Some((halo, width)) = text.halo {
            self.set_state((255, halo.a));
            let _ = write!(
                self.out,
                "{} RG {} w 1 J 1 j [] 0 d 1 Tr ",
                colour(halo),
                n(width)
            );
            show(&mut self.out);
        }
        if let Some(fill) = text.fill {
            self.set_state((fill.a, fill.a));
            let _ = write!(self.out, "{} rg 0 Tr ", colour(fill));
            show(&mut self.out);
            // The lines over the runs that have one.
            let mut along = 0.0;
            for run in &text.runs {
                let width = metrics::advance(text.face, &run.text, text.size);
                if run.overline {
                    let up = 0.8 * text.size;
                    let at = |d: f64| Pt::new(start.x + d * c + up * s, start.y + d * s - up * c);
                    let (a, b) = (at(along), at(along + width));
                    let _ = writeln!(
                        self.out,
                        "{} RG {} w 0 J [] 0 d {} {} m {} {} l S",
                        colour(fill),
                        n(0.07 * text.size),
                        n(a.x),
                        n(a.y),
                        n(b.x),
                        n(b.y)
                    );
                }
                along += width;
            }
        }
    }
}

/// `text` cut with an ellipsis so that it is at most `width` long.
fn fit(text: &str, face: Face, size: f64, width: f64) -> String {
    if metrics::advance(face, text, size) <= width {
        return text.to_string();
    }
    let mut out: String = text.to_string();
    while !out.is_empty() && metrics::advance(face, &format!("{out}..."), size) > width {
        out.pop();
    }
    format!("{out}...")
}

/// The pieces of a line in the encoding of the font that has their letters: `""` for the
/// line's own font (Windows Latin), `"F5"` for Symbol.
fn encode_runs(text: &Text) -> Vec<(&'static str, Vec<u8>)> {
    let mut out: Vec<(&'static str, Vec<u8>)> = Vec::new();
    for c in text.runs.iter().flat_map(|r| r.text.chars()) {
        let (font, byte) = match win_ansi(c) {
            Some(b) => ("", b),
            None => match symbol(c) {
                Some(b) => ("F5", b),
                None => ("", b'?'),
            },
        };
        match out.last_mut() {
            Some((f, bytes)) if *f == font => bytes.push(byte),
            _ => out.push((font, vec![byte])),
        }
    }
    out
}

/// `c` in the Windows Latin encoding, if it has it.
fn win_ansi(c: char) -> Option<u8> {
    let u = c as u32;
    match u {
        0x20..=0x7e | 0xa0..=0xff => Some(u as u8),
        _ => Some(match c {
            '€' => 0x80,
            '‚' => 0x82,
            'ƒ' => 0x83,
            '„' => 0x84,
            '…' => 0x85,
            '†' => 0x86,
            '‡' => 0x87,
            'ˆ' => 0x88,
            '‰' => 0x89,
            'Š' => 0x8a,
            '‹' => 0x8b,
            'Œ' => 0x8c,
            'Ž' => 0x8e,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            '˜' => 0x98,
            '™' => 0x99,
            'š' => 0x9a,
            '›' => 0x9b,
            'œ' => 0x9c,
            'ž' => 0x9e,
            'Ÿ' => 0x9f,
            _ => return None,
        }),
    }
}

/// `c` in the Symbol font's encoding: the Greek letters, the ohm sign, and a few signs.
fn symbol(c: char) -> Option<u8> {
    const GREEK_UPPER: &str = "ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡ ΣΤΥΦΧΨΩ";
    const SYMBOL_UPPER: &[u8] = b"ABGDEZHQIKLMNXOPR STUFCYW";
    const GREEK_LOWER: &str = "αβγδεζηθικλμνξοπρςστυφχψω";
    const SYMBOL_LOWER: &[u8] = b"abgdezhqiklmnxoprVstufcyw";
    if c == 'Ω' || c == '\u{2126}' {
        return Some(b'W');
    }
    if let Some(i) = GREEK_UPPER.chars().position(|g| g == c && g != ' ') {
        return Some(SYMBOL_UPPER[i]);
    }
    if let Some(i) = GREEK_LOWER.chars().position(|g| g == c) {
        return Some(SYMBOL_LOWER[i]);
    }
    Some(match c {
        '≤' => 0xa3,
        '≥' => 0xb3,
        '∞' => 0xa5,
        '≈' => 0xbb,
        '≠' => 0xb9,
        '√' => 0xd6,
        '→' => 0xae,
        '←' => 0xac,
        '↑' => 0xad,
        '↓' => 0xaf,
        '∑' => 0xe5,
        _ => return None,
    })
}

/// Bytes as a PDF literal string.
fn literal(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + 2);
    out.push('(');
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(b as char);
            }
            0x20..=0x7e => out.push(b as char),
            _ => {
                let _ = write!(out, "\\{b:03o}");
            }
        }
    }
    out.push(')');
    out
}

/// A string of the document's information: plain ASCII as a literal, anything else as UTF-16
/// with its byte order mark, as the PDF specification has text strings.
fn text_string(text: &str) -> String {
    if text.chars().all(|c| (' '..='~').contains(&c)) {
        return literal(text.as_bytes());
    }
    let mut out = String::from("<FEFF");
    for unit in text.encode_utf16() {
        let _ = write!(out, "{unit:04X}");
    }
    out.push('>');
    out
}

fn colour(c: Rgba) -> String {
    format!(
        "{} {} {}",
        n(f64::from(c.r) / 255.0),
        n(f64::from(c.g) / 255.0),
        n(f64::from(c.b) / 255.0)
    )
}

/// A number with at most four decimals and no trailing zeros.
fn n(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    match s {
        "-0" | "" => "0".to_string(),
        s => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short() {
        assert_eq!(n(1.0), "1");
        assert_eq!(n(-0.00001), "0");
        assert_eq!(n(2.54), "2.54");
        assert_eq!(n(1.23456), "1.2346");
    }

    #[test]
    fn letters_fall_back_to_symbol() {
        assert_eq!(symbol('Ω'), Some(b'W'));
        assert_eq!(symbol('μ'), Some(b'm'));
        assert_eq!(win_ansi('µ'), Some(0xb5));
        assert_eq!(literal(b"a(b)\\\xb5"), "(a\\(b\\)\\\\\\265)");
    }
}
