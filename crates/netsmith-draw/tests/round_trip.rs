//! A small SVG in the style netsmith writes, read and written as PDF and PNG.

use std::io::Read as _;

use netsmith_draw::pdf::{Document, Page, TitleBlock};
use netsmith_draw::{Face, Item, Rect, Rgba, raster, svg};

const SAMPLE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" class="d" viewBox="0 0 40 20">
<style>
.d{--ink:#1d4f5e;--paper:#fbfaf3}
.d .sheet{fill:var(--paper)}
.d text{fill:var(--ink);stroke:var(--paper);stroke-width:.5;paint-order:stroke}
.d .hit{fill:none;stroke:transparent}
.d .line{fill:none;stroke:#0a7a3c;stroke-linecap:round}
.d .bold{font-weight:700}
.d .frame{fill:#0a7a3c0a;stroke:#6f9a86;stroke-width:1.2;stroke-dasharray:6 4;vector-effect:non-scaling-stroke}
.d:hover .line{stroke:red}
</style>
<title>sample</title>
<rect class="sheet" x="0" y="0" width="40" height="20"/>
<rect class="frame" x="1" y="1" width="38" height="18" rx="1"/>
<g class="wire"><polyline class="hit" stroke-width="2" points="2,10 30,10"/><polyline class="line" stroke-width="0.25" points="2,10 30,10 30,15"/></g>
<path class="line" d="M 5 5 A 2 2 0 0 1 9 5"/>
<circle cx="30" cy="10" r="0.45" fill="#0a7a3c"/>
<text class="bold" font-size="1.27" text-anchor="middle" x="10" y="8">R1</text>
<text font-size="1.27" transform="rotate(-90 20 15)" x="20" y="15">GND (Ω)</text>
<text font-size="1" x="3" y="18">~<tspan text-decoration="overline">RST</tspan></text>
<text font-size="1" x="25" y="4" xml:space="preserve"><tspan x="25" y="4">line one</tspan><tspan x="25" y="5.5">line two</tspan></text>
</svg>"##;

#[test]
fn the_reader_resolves_styles_and_leaves_out_what_draws_nothing() {
    let p = svg::read(SAMPLE).expect("reads");
    assert_eq!(p.view, Rect::new(0.0, 0.0, 40.0, 20.0));
    let texts: Vec<String> = p.texts().map(|t| t.string()).collect();
    assert_eq!(texts, ["R1", "GND (Ω)", "~RST", "line one", "line two"]);
    let r1 = p.texts().next().unwrap();
    assert_eq!(r1.face, Face::SansBold);
    assert_eq!(r1.fill, Some(Rgba::opaque(0x1d, 0x4f, 0x5e)));
    assert_eq!(r1.halo, Some((Rgba::opaque(0xfb, 0xfa, 0xf3), 0.5)));
    let gnd = p.texts().nth(1).unwrap();
    assert!((gnd.angle + 90.0).abs() < 1e-9);
    let rst = p.texts().nth(2).unwrap();
    assert!(rst.runs.iter().any(|r| r.overline && r.text == "RST"));
    // The hit area is transparent and left out: the sheet, the frame, one wire, the arc and
    // the dot are the shapes.
    let shapes: Vec<_> = p
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Shape(s) => Some(s),
            Item::Text(_) => None,
        })
        .collect();
    assert_eq!(shapes.len(), 5);
    let frame = shapes[1];
    assert_eq!(frame.fill.unwrap().0.a, 10);
    let stroke = frame.stroke.as_ref().unwrap();
    assert!((stroke.width - 1.2 * 25.4 / 96.0).abs() < 1e-9);
    assert_eq!(stroke.dash.len(), 2);
    // The `:hover` rule is not the drawing's.
    assert_eq!(
        shapes[2].stroke.as_ref().unwrap().colour,
        Rgba::opaque(0x0a, 0x7a, 0x3c)
    );
}

fn inflate(z: &[u8]) -> String {
    let mut out = String::new();
    flate2::read::ZlibDecoder::new(z)
        .read_to_string(&mut out)
        .expect("inflates");
    out
}

/// The content streams of a PDF, inflated.
pub fn contents(pdf: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = pdf;
    while let Some(at) = find(rest, b"/FlateDecode >>\nstream\n") {
        let start = at + b"/FlateDecode >>\nstream\n".len();
        let end = start + find(&rest[start..], b"\nendstream").expect("an end");
        out.push(inflate(&rest[start..end]));
        rest = &rest[end..];
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[test]
fn a_pdf_has_a_page_for_each_region_and_its_text() {
    let p = svg::read(SAMPLE).expect("reads");
    let doc = Document {
        title: "sample".to_string(),
        producer: "netsmith test".to_string(),
        pages: vec![
            Page {
                picture: &p,
                view: p.view,
                title: TitleBlock {
                    heading: "all".to_string(),
                    lines: vec!["Board".to_string()],
                },
            },
            Page {
                picture: &p,
                view: Rect::new(0.0, 0.0, 15.0, 10.0),
                title: TitleBlock {
                    heading: "corner".to_string(),
                    lines: vec![],
                },
            },
        ],
    };
    let bytes = netsmith_draw::pdf::write(&doc);
    assert!(bytes.starts_with(b"%PDF-1.4"));
    assert!(bytes.ends_with(b"%%EOF\n"));
    assert_eq!(bytes, netsmith_draw::pdf::write(&doc), "the same bytes");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/Count 2"));
    assert!(text.contains("/BaseFont /Helvetica-Bold"));
    let pages = contents(&bytes);
    assert_eq!(pages.len(), 2);
    assert!(pages[0].contains("(R1) Tj"));
    assert!(pages[0].contains("(GND \\() Tj") && pages[0].contains("/F5"));
    assert!(pages[0].contains("(line two) Tj"));
    assert!(pages[0].contains("(all) Tj"));
    // The corner page draws R1 and not the text that is outside it.
    assert!(pages[1].contains("(R1) Tj"));
    assert!(!pages[1].contains("(line two) Tj"));
    // The second page is as wide as the title block (80 mm) with its margins.
    assert!(text.contains("/MediaBox [0 0 283.4646 "), "{text}");
}

#[test]
fn a_png_is_the_same_each_time_and_draws_the_wire() {
    let p = svg::read(SAMPLE).expect("reads");
    let img = raster::render(&p, p.view, 10.0, Rgba::WHITE);
    assert_eq!((img.width, img.height), (400, 200));
    // The wire at y = 10, x = 15 is green; the paper at (39.7, 0.3), outside the frame, is the sheet's colour.
    let at = |x: usize, y: usize| {
        let i = (y * img.width + x) * 4;
        [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2]]
    };
    let wire = at(150, 100);
    assert!(wire[1] > wire[0] + 40, "{wire:?}");
    assert_eq!(at(397, 3), [0xfb, 0xfa, 0xf3]);
    let png = raster::png(&img);
    assert!(png.starts_with(b"\x89PNG"));
    assert_eq!(png, raster::png(&raster::render(&p, p.view, 10.0, Rgba::WHITE)));
}
