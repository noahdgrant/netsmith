//! Writing a generated footprint as the text of a `.kicad_mod`.
//!
//! A generated footprint is the same thing as one read from a library: the generators write
//! the file's text and [`Footprint::parse`] reads it, so the loader, the placer, the
//! fabrication writers and the `.kicad_pcb` export see no difference.

use std::fmt::Write;

use crate::footprint::{Footprint, Mount};
use crate::geometry::Pt;

/// Line widths of the layers, in mm (KiCad's library conventions).
pub const SILK_WIDTH: f64 = 0.12;
pub const FAB_WIDTH: f64 = 0.1;
pub const COURTYARD_WIDTH: f64 = 0.05;

/// A number as KiCad writes it: at most six decimals, no trailing zeros, no "-0".
pub fn num(v: f64) -> String {
    let mut s = format!("{:.6}", v);
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" { "0".to_string() } else { s }
}

/// The silkscreen, fabrication and courtyard layers, by their KiCad names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Draw {
    Silk,
    Fab,
    Courtyard,
}

impl Draw {
    fn layer(self) -> &'static str {
        match self {
            Draw::Silk => "F.SilkS",
            Draw::Fab => "F.Fab",
            Draw::Courtyard => "F.CrtYd",
        }
    }

    fn width(self) -> f64 {
        match self {
            Draw::Silk => SILK_WIDTH,
            Draw::Fab => FAB_WIDTH,
            Draw::Courtyard => COURTYARD_WIDTH,
        }
    }
}

/// The shape of a pad.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// A rectangle with rounded corners, as KiCad's generated surface-mount pads have: the
    /// corner radius is 0.25 mm but at most a quarter of the shorter side.
    RoundRect,
    Rect,
    Oval,
}

/// What a pad is made of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// Copper, solder mask and paste on the front.
    Smd,
    /// Paste only (a stencil opening that is not a pad).
    PasteOnly,
    /// Copper and mask on the front, no paste: the exposed pad's copper.
    SmdNoPaste,
    /// Copper and paste on the front, no mask opening: one opening of the stencil over a
    /// large pad, drawn as a pad of the same net.
    CopperPaste,
    /// Plated through hole, with this drill diameter.
    Through(f64),
}

/// A pad, as the generators describe it.
#[derive(Debug, Clone)]
pub struct PadSpec {
    pub number: String,
    pub kind: Kind,
    pub shape: Shape,
    pub at: Pt,
    pub size: (f64, f64),
}

impl PadSpec {
    pub fn new(number: impl ToString, kind: Kind, shape: Shape, at: Pt, size: (f64, f64)) -> Self {
        Self {
            number: number.to_string(),
            kind,
            shape,
            at,
            size,
        }
    }

    /// A rounded surface-mount pad with paste.
    pub fn smd(number: impl ToString, at: Pt, size: (f64, f64)) -> Self {
        Self::new(number, Kind::Smd, Shape::RoundRect, at, size)
    }
}

/// A footprint being written.
pub struct Drawing {
    name: String,
    library: String,
    description: String,
    tags: String,
    mount: Mount,
    body: String,
}

impl Drawing {
    pub fn new(library: &str, name: &str, mount: Mount) -> Self {
        Self {
            name: name.to_string(),
            library: library.to_string(),
            description: String::new(),
            tags: String::new(),
            mount,
            body: String::new(),
        }
    }

    pub fn describe(&mut self, description: &str, tags: &str) {
        self.description = description.to_string();
        self.tags = tags.to_string();
    }

    pub fn line(&mut self, on: Draw, a: Pt, b: Pt) {
        let _ = writeln!(
            self.body,
            "(fp_line (start {} {}) (end {} {}) (stroke (width {}) (type solid)) (layer \"{}\"))",
            num(a.x),
            num(a.y),
            num(b.x),
            num(b.y),
            num(on.width()),
            on.layer()
        );
    }

    /// The outline of a rectangle with corners `a` and `b`.
    pub fn rect(&mut self, on: Draw, a: Pt, b: Pt) {
        let _ = writeln!(
            self.body,
            "(fp_rect (start {} {}) (end {} {}) (stroke (width {}) (type solid)) (fill no) (layer \"{}\"))",
            num(a.x),
            num(a.y),
            num(b.x),
            num(b.y),
            num(on.width()),
            on.layer()
        );
    }

    /// A closed polygon, filled or only its outline.
    pub fn polygon(&mut self, on: Draw, points: &[Pt], filled: bool) {
        let pts: String = points
            .iter()
            .map(|p| format!("(xy {} {}) ", num(p.x), num(p.y)))
            .collect();
        let _ = writeln!(
            self.body,
            "(fp_poly (pts {}) (stroke (width {}) (type solid)) (fill {}) (layer \"{}\"))",
            pts.trim_end(),
            num(on.width()),
            if filled { "solid" } else { "no" },
            on.layer()
        );
    }

    /// The courtyard: a rectangle centred on the origin's frame from `min` to `max`, on
    /// KiCad's 0.01 mm grid.
    pub fn courtyard(&mut self, min: Pt, max: Pt) {
        let snap = |v: f64| super::ipc::round_to(v, 0.01);
        self.rect(
            Draw::Courtyard,
            Pt::new(snap(min.x), snap(min.y)),
            Pt::new(snap(max.x), snap(max.y)),
        );
    }

    /// The reference designator printed on the fabrication layer.
    pub fn fab_reference(&mut self, height: f64) {
        let _ = writeln!(
            self.body,
            "(fp_text user \"${{REFERENCE}}\" (at 0 0 0) (layer \"F.Fab\") (effects (font (size {h} {h}) (thickness {t}))))",
            h = num(height),
            t = num(height * 0.15)
        );
    }

    pub fn pad(&mut self, pad: &PadSpec) {
        let (kind, layers) = match pad.kind {
            Kind::Smd => ("smd", "\"F.Cu\" \"F.Mask\" \"F.Paste\""),
            Kind::PasteOnly => ("smd", "\"F.Paste\""),
            Kind::SmdNoPaste => ("smd", "\"F.Cu\" \"F.Mask\""),
            Kind::CopperPaste => ("smd", "\"F.Cu\" \"F.Paste\""),
            Kind::Through(_) => ("thru_hole", "\"*.Cu\" \"*.Mask\""),
        };
        let shape = match pad.shape {
            Shape::RoundRect => "roundrect",
            Shape::Rect => "rect",
            Shape::Oval => "oval",
        };
        let _ = write!(
            self.body,
            "(pad \"{}\" {kind} {shape} (at {} {}) (size {} {})",
            pad.number,
            num(pad.at.x),
            num(pad.at.y),
            num(pad.size.0),
            num(pad.size.1)
        );
        if let Kind::Through(drill) = pad.kind {
            let _ = write!(self.body, " (drill {})", num(drill));
        }
        let _ = write!(self.body, " (layers {layers})");
        if pad.shape == Shape::RoundRect {
            let short = pad.size.0.min(pad.size.1);
            let _ = write!(
                self.body,
                " (roundrect_rratio {})",
                num((0.25 / short).min(0.25))
            );
        }
        self.body.push_str(")\n");
    }

    /// A surface-mount pad that is a rectangle `anchor` mm in size at `at` plus a filled
    /// polygon, the corners of which are given relative to `at`: a tab that is not a
    /// rectangle (KiCad's `custom` pad).
    pub fn custom_pad(
        &mut self,
        number: impl ToString,
        at: Pt,
        anchor: (f64, f64),
        outline: &[Pt],
    ) {
        let pts: String = outline
            .iter()
            .map(|p| format!("(xy {} {}) ", num(p.x), num(p.y)))
            .collect();
        let _ = writeln!(
            self.body,
            "(pad \"{}\" smd custom (at {} {}) (size {} {}) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\") \
             (options (clearance outline) (anchor rect)) \
             (primitives (gr_poly (pts {}) (width 0) (fill yes))))",
            number.to_string(),
            num(at.x),
            num(at.y),
            num(anchor.0),
            num(anchor.1),
            pts.trim_end()
        );
    }

    /// The text of the file. The reference is put `above` the origin and the value `below`.
    pub fn text(&self, reference_y: f64, value_y: f64) -> String {
        let attr = match self.mount {
            Mount::Smd => "smd",
            Mount::ThroughHole => "through_hole",
        };
        let font = "(effects (font (size 1 1) (thickness 0.15)))";
        let hidden = "(effects (font (size 1.27 1.27) (thickness 0.15)))";
        let mut out = format!(
            "(footprint \"{name}\" (version 20241229) (generator \"netsmith\") (layer \"F.Cu\")\n\
             (descr \"{descr}\") (tags \"{tags}\")\n\
             (property \"Reference\" \"REF**\" (at 0 {ry} 0) (layer \"F.SilkS\") {font})\n\
             (property \"Value\" \"{name}\" (at 0 {vy} 0) (layer \"F.Fab\") {font})\n\
             (property \"Datasheet\" \"\" (at 0 0 0) (unlocked yes) (layer \"F.Fab\") (hide yes) {hidden})\n\
             (property \"Description\" \"\" (at 0 0 0) (unlocked yes) (layer \"F.Fab\") (hide yes) {hidden})\n\
             (attr {attr})\n",
            name = self.name,
            descr = self.description,
            tags = self.tags,
            ry = num(reference_y),
            vy = num(value_y),
        );
        out.push_str(&self.body);
        let _ = writeln!(
            out,
            "(model \"${{KICAD9_3DMODEL_DIR}}/{lib}.3dshapes/{name}.wrl\" (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 0))))",
            lib = self.library,
            name = self.name
        );
        out
    }

    /// Read the file back as a [`Footprint`]. The generators write well-formed text, so a
    /// failure is a bug in a generator and is reported as one.
    pub fn finish(self, reference_y: f64, value_y: f64) -> Footprint {
        let text = self.text(reference_y, value_y);
        match Footprint::parse(&format!("{}:{}", self.library, self.name), &text) {
            Ok(footprint) => footprint,
            Err(e) => panic!("the generator for {} wrote a bad footprint: {e}", self.name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_short() {
        assert_eq!(num(0.5), "0.5");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(1.0), "1");
        assert_eq!(num(0.1 + 0.2), "0.3");
        assert_eq!(num(-0.153641), "-0.153641");
    }

    #[test]
    fn a_drawing_reads_back_as_the_footprint_it_describes() {
        let mut d = Drawing::new("Lib", "X", Mount::Smd);
        d.pad(&PadSpec::smd(1, Pt::new(-1.0, 0.0), (1.0, 0.5)));
        d.pad(&PadSpec::smd(2, Pt::new(1.0, 0.0), (1.0, 0.5)));
        d.courtyard(Pt::new(-1.7, -0.5), Pt::new(1.7, 0.5));
        let fp = d.finish(-1.2, 1.2);
        assert_eq!(fp.name, "X");
        assert_eq!(fp.pad_numbers(), ["1", "2"]);
        assert_eq!(fp.reference_at, Pt::new(0.0, -1.2));
        assert_eq!(fp.courtyard.max, Pt::new(1.7, 0.5));
        assert_eq!(fp.pads[0].corner_ratio, 0.25);
    }
}
