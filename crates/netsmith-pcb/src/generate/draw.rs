//! Writing a generated footprint as the text of a `.kicad_mod`.
//!
//! A generated footprint is the same thing as one read from a library: the generators write
//! the file's text and [`Footprint::parse`] reads it, so the loader, the placer, the
//! fabrication writers and the `.kicad_pcb` export see no difference.

use std::fmt::Write;

use crate::footprint::{Footprint, Mount};
use crate::geometry::Pt;

/// Line widths of the layers, in mm (KiCad's library conventions).
pub(super) const SILK_WIDTH: f64 = 0.12;
pub(super) const FAB_WIDTH: f64 = 0.1;
pub(super) const COURTYARD_WIDTH: f64 = 0.05;
pub(super) const COMMENT_WIDTH: f64 = 0.15;

/// A number as KiCad writes it: at most six decimals, no trailing zeros, no "-0".
pub(super) fn num(v: f64) -> String {
    let mut s = format!("{:.6}", v);
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" { "0".to_string() } else { s }
}

/// The silkscreen, fabrication, courtyard and comment layers, by their KiCad names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Draw {
    Silk,
    Fab,
    Courtyard,
    /// `Cmts.User`: a note for the person, such as the room a screw head takes.
    Comments,
}

impl Draw {
    fn layer(self) -> &'static str {
        match self {
            Draw::Silk => "F.SilkS",
            Draw::Fab => "F.Fab",
            Draw::Courtyard => "F.CrtYd",
            Draw::Comments => "Cmts.User",
        }
    }

    fn width(self) -> f64 {
        match self {
            Draw::Silk => SILK_WIDTH,
            Draw::Fab => FAB_WIDTH,
            Draw::Courtyard => COURTYARD_WIDTH,
            Draw::Comments => COMMENT_WIDTH,
        }
    }
}

/// The shape of a pad.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Shape {
    /// A rectangle with rounded corners, as KiCad's generated surface-mount pads have: the
    /// corner radius is 0.25 mm but at most a quarter of the shorter side.
    RoundRect,
    Rect,
    Oval,
    Circle,
}

/// What a pad is made of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Kind {
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
    /// A hole with no copper, with this drill diameter: copper and mask are kept off it on
    /// every layer.
    NonPlated(f64),
}

/// A pad, as the generators describe it.
#[derive(Debug, Clone)]
pub(super) struct PadSpec {
    pub number: String,
    pub kind: Kind,
    pub shape: Shape,
    pub at: Pt,
    pub size: (f64, f64),
    /// The pad's own `solder_mask_margin`, where it differs from the board's.
    pub mask_margin: Option<f64>,
    /// The pad's own `clearance` from copper of other nets.
    pub clearance: Option<f64>,
    /// Whether the plating is kept on every inner layer (`remove_unused_layers no`), which a
    /// plated hole that a screw bonds to wants. Only a through pad has it.
    pub keep_all_layers: bool,
}

impl PadSpec {
    pub(super) fn new(
        number: impl ToString,
        kind: Kind,
        shape: Shape,
        at: Pt,
        size: (f64, f64),
    ) -> Self {
        Self {
            number: number.to_string(),
            kind,
            shape,
            at,
            size,
            mask_margin: None,
            clearance: None,
            keep_all_layers: false,
        }
    }

    /// The mask opening is this much larger than the pad on every side.
    pub(super) fn with_mask_margin(mut self, margin: f64) -> Self {
        self.mask_margin = Some(margin);
        self
    }

    /// Copper of other nets keeps this far from the pad.
    pub(super) fn with_clearance(mut self, clearance: f64) -> Self {
        self.clearance = Some(clearance);
        self
    }

    /// Keep the plating on the inner layers that nothing connects to.
    pub(super) fn keeping_all_layers(mut self) -> Self {
        self.keep_all_layers = true;
        self
    }

    /// A rounded surface-mount pad with paste.
    pub(super) fn smd(number: impl ToString, at: Pt, size: (f64, f64)) -> Self {
        Self::new(number, Kind::Smd, Shape::RoundRect, at, size)
    }
}

/// A footprint being written.
pub(super) struct Drawing {
    name: String,
    library: String,
    description: String,
    tags: String,
    mount: Mount,
    /// What the file's `(attr ...)` says when it is not just the mounting.
    attr: Option<String>,
    /// Whether the file names a 3D model, as the library of its name would have.
    model: bool,
    body: String,
}

impl Drawing {
    pub(super) fn new(library: &str, name: &str, mount: Mount) -> Self {
        Self {
            name: name.to_string(),
            library: library.to_string(),
            description: String::new(),
            tags: String::new(),
            mount,
            attr: None,
            model: true,
            body: String::new(),
        }
    }

    /// The whole `(attr ...)` of the file, instead of the mounting alone: a part that is on
    /// no bill of materials and no placement file says `exclude_from_pos_files
    /// exclude_from_bom`.
    pub(super) fn attr(&mut self, attr: &str) {
        self.attr = Some(attr.to_string());
    }

    /// Name no 3D model: a hole, a fiducial and a test point have no body to show.
    pub(super) fn without_model(&mut self) {
        self.model = false;
    }

    pub(super) fn describe(&mut self, description: &str, tags: &str) {
        self.description = description.to_string();
        self.tags = tags.to_string();
    }

    pub(super) fn line(&mut self, on: Draw, a: Pt, b: Pt) {
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

    /// The outline of a circle round the origin.
    pub(super) fn circle(&mut self, on: Draw, radius: f64) {
        let _ = writeln!(
            self.body,
            "(fp_circle (center 0 0) (end {} 0) (stroke (width {}) (type solid)) (fill no) (layer \"{}\"))",
            num(radius),
            num(on.width()),
            on.layer()
        );
    }

    /// The outline of a rectangle with corners `a` and `b`.
    pub(super) fn rect(&mut self, on: Draw, a: Pt, b: Pt) {
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
    pub(super) fn polygon(&mut self, on: Draw, points: &[Pt], filled: bool) {
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
    pub(super) fn courtyard(&mut self, min: Pt, max: Pt) {
        let snap = |v: f64| super::ipc::round_to(v, 0.01);
        self.rect(
            Draw::Courtyard,
            Pt::new(snap(min.x), snap(min.y)),
            Pt::new(snap(max.x), snap(max.y)),
        );
    }

    /// A circular courtyard round the origin.
    pub(super) fn courtyard_circle(&mut self, radius: f64) {
        self.circle(Draw::Courtyard, radius);
    }

    /// The reference designator printed on the fabrication layer, on the origin.
    pub(super) fn fab_reference(&mut self, height: f64) {
        self.fab_reference_at(Pt::new(0.0, 0.0), height);
    }

    /// The reference designator printed on the fabrication layer at `at`.
    pub(super) fn fab_reference_at(&mut self, at: Pt, height: f64) {
        let _ = writeln!(
            self.body,
            "(fp_text user \"${{REFERENCE}}\" (at {} {} 0) (layer \"F.Fab\") (effects (font (size {h} {h}) (thickness {t}))))",
            num(at.x),
            num(at.y),
            h = num(height),
            t = num(height * 0.15)
        );
    }

    pub(super) fn pad(&mut self, pad: &PadSpec) {
        let (kind, layers) = match pad.kind {
            Kind::Smd => ("smd", "\"F.Cu\" \"F.Mask\" \"F.Paste\""),
            Kind::PasteOnly => ("smd", "\"F.Paste\""),
            Kind::SmdNoPaste => ("smd", "\"F.Cu\" \"F.Mask\""),
            Kind::CopperPaste => ("smd", "\"F.Cu\" \"F.Paste\""),
            Kind::Through(_) => ("thru_hole", "\"*.Cu\" \"*.Mask\""),
            Kind::NonPlated(_) => ("np_thru_hole", "\"*.Cu\" \"*.Mask\""),
        };
        let shape = match pad.shape {
            Shape::RoundRect => "roundrect",
            Shape::Rect => "rect",
            Shape::Oval => "oval",
            Shape::Circle => "circle",
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
        if let Kind::Through(drill) | Kind::NonPlated(drill) = pad.kind {
            let _ = write!(self.body, " (drill {})", num(drill));
        }
        let _ = write!(self.body, " (layers {layers})");
        if pad.keep_all_layers {
            self.body.push_str(" (remove_unused_layers no)");
        }
        if let Some(margin) = pad.mask_margin {
            let _ = write!(self.body, " (solder_mask_margin {})", num(margin));
        }
        if let Some(clearance) = pad.clearance {
            let _ = write!(self.body, " (clearance {})", num(clearance));
        }
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
    pub(super) fn custom_pad(
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
    pub(super) fn text(&self, reference_y: f64, value_y: f64) -> String {
        let attr = self.attr.as_deref().unwrap_or(match self.mount {
            Mount::Smd => "smd",
            Mount::ThroughHole => "through_hole",
        });
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
        if self.model {
            let _ = writeln!(
                out,
                "(model \"${{KICAD9_3DMODEL_DIR}}/{lib}.3dshapes/{name}.wrl\" (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 0))))",
                lib = self.library,
                name = self.name
            );
        } else {
            out.push_str(")\n");
        }
        out
    }

    /// Read the file back as a [`Footprint`]. The generators write well-formed text, so a
    /// failure is a bug in a generator and is reported as one.
    pub(super) fn finish(self, reference_y: f64, value_y: f64) -> Footprint {
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
