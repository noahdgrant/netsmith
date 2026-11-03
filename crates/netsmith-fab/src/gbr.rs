//! A Gerber (RS-274X, with the X2 attributes) file under construction: apertures that are
//! numbered by first use, objects that carry their attributes, regions with arcs, and the
//! header KiCad writes (format 4.6 in millimetres), so a board house reads the file the same
//! way. Coordinates go in as KiCad's board coordinates; the file's origin is the drill/place
//! origin and y is turned upward, as KiCad plots.

use crate::geom::{Arc, Pt};
use std::fmt::Write as _;

/// An aperture. Offsets inside a macro are in the file's frame (y up) from the flash point.
#[derive(Debug, Clone, PartialEq)]
pub enum Ap {
    Circle(f64),
    Rect(f64, f64),
    Obround(f64, f64),
    /// A rectangle with rounded corners: the radius and the four corners of the inner
    /// rectangle, turned already.
    RoundRect {
        radius: f64,
        corners: [Pt; 4],
    },
    /// A rectangle `w` by `h` turned by `angle` degrees counter-clockwise.
    RotRect {
        w: f64,
        h: f64,
        angle: f64,
    },
    /// A thick line with round ends between two points (an oval, turned).
    HorizOval {
        width: f64,
        a: Pt,
        b: Pt,
    },
}

const ROUND_RECT_MACRO: &str = "%AMRoundRect*\n0 Rectangle with rounded corners*\n0 $1 Rounding radius*\n0 $2 $3 $4 $5 $6 $7 $8 $9 X,Y pos of 4 corners*\n0 Add a 4 corners polygon primitive as box body*\n4,1,4,$2,$3,$4,$5,$6,$7,$8,$9,$2,$3,0*\n0 Add four circle primitives for the rounded corners*\n1,1,$1+$1,$2,$3*\n1,1,$1+$1,$4,$5*\n1,1,$1+$1,$6,$7*\n1,1,$1+$1,$8,$9*\n0 Add four rect primitives between the rounded corners*\n20,1,$1+$1,$2,$3,$4,$5,0*\n20,1,$1+$1,$4,$5,$6,$7,0*\n20,1,$1+$1,$6,$7,$8,$9,0*\n20,1,$1+$1,$8,$9,$2,$3,0*%";
const ROT_RECT_MACRO: &str = "%AMRotRect*\n0 Rectangle, with rotation*\n0 The origin of the aperture is its center*\n0 $1 length*\n0 $2 width*\n0 $3 Rotation angle, in degrees counterclockwise*\n0 Add horizontal line*\n21,1,$1,$2,0,0,$3*%";
const HORIZ_OVAL_MACRO: &str = "%AMHorizOval*\n0 Thick line with rounded ends*\n0 $1 width*\n0 $2 $3 position (X,Y) of the first rounded end (center of the circle)*\n0 $4 $5 position (X,Y) of the second rounded end (center of the circle)*\n0 Add line between two ends*\n20,1,$1,$2,$3,$4,$5,0*\n0 Add two circle primitives to create the rounded ends*\n1,1,$1,$2,$3*\n1,1,$1,$4,$5*%";

/// A number as a Gerber parameter: up to six decimals, no trailing zeros but at least the
/// six KiCad prints, so that equal apertures print equal.
fn dec(v: f64) -> String {
    let v = if v.abs() < 5e-7 { 0.0 } else { v };
    format!("{v:.6}")
}

impl Ap {
    fn macro_text(&self) -> Option<&'static str> {
        match self {
            Ap::RoundRect { .. } => Some(ROUND_RECT_MACRO),
            Ap::RotRect { .. } => Some(ROT_RECT_MACRO),
            Ap::HorizOval { .. } => Some(HORIZ_OVAL_MACRO),
            _ => None,
        }
    }

    /// The text after `%ADDnn`.
    fn definition(&self) -> String {
        match self {
            Ap::Circle(d) => format!("C,{}", dec(*d)),
            Ap::Rect(w, h) => format!("R,{}X{}", dec(*w), dec(*h)),
            Ap::Obround(w, h) => format!("O,{}X{}", dec(*w), dec(*h)),
            Ap::RoundRect { radius, corners } => {
                let mut s = format!("RoundRect,{}", dec(*radius));
                for c in corners {
                    let _ = write!(s, "X{}X{}", dec(c.x), dec(c.y));
                }
                s.push_str("X0");
                s
            }
            Ap::RotRect { w, h, angle } => {
                format!("RotRect,{}X{}X{}", dec(*w), dec(*h), dec(*angle))
            }
            Ap::HorizOval { width, a, b } => format!(
                "HorizOval,{}X{}X{}X{}X{}X0",
                dec(*width),
                dec(a.x),
                dec(a.y),
                dec(b.x),
                dec(b.y)
            ),
        }
    }
}

/// A piece of the outline of a region.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Line(Pt),
    /// An arc to `to` about `centre`, counter-clockwise on the screen if `ccw`.
    Arc {
        to: Pt,
        centre: Pt,
        ccw: bool,
    },
}

/// A closed outline: from `start` along the segments, back to the start.
#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    pub start: Pt,
    pub segs: Vec<Seg>,
}

impl Contour {
    /// A polygon given by its corners.
    pub fn polygon(points: &[Pt]) -> Contour {
        Contour {
            start: points.first().copied().unwrap_or_default(),
            segs: points.iter().skip(1).map(|p| Seg::Line(*p)).collect(),
        }
    }

    /// A circle as two half arcs.
    pub fn circle(centre: Pt, radius: f64) -> Contour {
        let (a, b) = (
            Pt::new(centre.x + radius, centre.y),
            Pt::new(centre.x - radius, centre.y),
        );
        Contour {
            start: a,
            segs: vec![
                Seg::Arc {
                    to: b,
                    centre,
                    ccw: true,
                },
                Seg::Arc {
                    to: a,
                    centre,
                    ccw: true,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Cmd {
    Polarity(bool),
    Flash { ap: usize, at: Pt },
    Line { ap: usize, a: Pt, b: Pt },
    Arc { ap: usize, arc: Arc },
    Region(Contour),
}

#[derive(Debug, Clone, PartialEq)]
struct Item {
    cmd: Cmd,
    /// `TO` attribute lines (without `%TO.` and `*%`) that hold for this object.
    attrs: Vec<String>,
    /// The `TA.AperFunction` of a region.
    function: Option<&'static str>,
}

/// One Gerber file.
#[derive(Debug, Clone)]
pub struct Gerber {
    origin: Pt,
    aps: Vec<(Ap, Option<&'static str>)>,
    items: Vec<Item>,
}

/// A value of an attribute field, escaped as the format wants: the characters that end a
/// field or a command, and anything outside ASCII, become `\uXXXX`.
pub fn escape(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        if matches!(c, ',' | '*' | '%' | '\\') || !(' '..='~').contains(&c) {
            let mut buf = [0u16; 2];
            for unit in c.encode_utf16(&mut buf) {
                let _ = write!(out, "\\u{unit:04X}");
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl Gerber {
    /// A file whose origin (0, 0) is at `origin` of the board.
    pub fn new(origin: Pt) -> Self {
        Self {
            origin,
            aps: Vec::new(),
            items: Vec::new(),
        }
    }

    /// True if nothing has been drawn.
    pub fn is_empty(&self) -> bool {
        !self
            .items
            .iter()
            .any(|i| !matches!(i.cmd, Cmd::Polarity(_)))
    }

    /// The point in the file's frame.
    fn at(&self, p: Pt) -> Pt {
        Pt::new(p.x - self.origin.x, -(p.y - self.origin.y))
    }

    fn ap(&mut self, ap: Ap, function: Option<&'static str>) -> usize {
        match self
            .aps
            .iter()
            .position(|(a, f)| *a == ap && *f == function)
        {
            Some(i) => i,
            None => {
                self.aps.push((ap, function));
                self.aps.len() - 1
            }
        }
    }

    /// Switches to dark (`true`) or clear polarity for what follows.
    pub fn polarity(&mut self, dark: bool) {
        self.items.push(Item {
            cmd: Cmd::Polarity(dark),
            attrs: Vec::new(),
            function: None,
        });
    }

    pub fn flash(&mut self, ap: Ap, function: Option<&'static str>, at: Pt, attrs: &[String]) {
        let ap = self.ap(ap, function);
        self.push(Cmd::Flash { ap, at }, attrs, None);
    }

    /// A stroke with a round pen of `width`.
    pub fn line(
        &mut self,
        width: f64,
        function: Option<&'static str>,
        a: Pt,
        b: Pt,
        attrs: &[String],
    ) {
        let ap = self.ap(Ap::Circle(width), function);
        self.push(Cmd::Line { ap, a, b }, attrs, None);
    }

    pub fn arc(&mut self, width: f64, function: Option<&'static str>, arc: &Arc, attrs: &[String]) {
        let ap = self.ap(Ap::Circle(width), function);
        self.push(Cmd::Arc { ap, arc: *arc }, attrs, None);
    }

    pub fn region(&mut self, contour: Contour, function: Option<&'static str>, attrs: &[String]) {
        self.push(Cmd::Region(contour), attrs, function);
    }

    fn push(&mut self, cmd: Cmd, attrs: &[String], function: Option<&'static str>) {
        self.items.push(Item {
            cmd,
            attrs: attrs.to_vec(),
            function,
        });
    }

    /// The file. `function` is the `TF.FileFunction` value (`Copper,L1,Top`), `negative` is
    /// the file polarity, and `project` names the board.
    pub fn text(&self, function: &str, negative: bool, project: &str) -> String {
        let mut o = String::new();
        let _ = writeln!(
            o,
            "%TF.GenerationSoftware,netsmith,netsmith,{}*%",
            env!("CARGO_PKG_VERSION")
        );
        let _ = writeln!(
            o,
            "%TF.ProjectId,{},{},rev?*%",
            escape(project),
            project_id(project)
        );
        o.push_str("%TF.SameCoordinates,Original*%\n");
        let _ = writeln!(o, "%TF.FileFunction,{function}*%");
        if !function.starts_with("Profile") {
            let _ = writeln!(
                o,
                "%TF.FilePolarity,{}*%",
                if negative { "Negative" } else { "Positive" }
            );
        }
        o.push_str("%FSLAX46Y46*%\n");
        o.push_str("G04 Gerber Fmt 4.6, Leading zero omitted, Abs format (unit mm)*\n");
        o.push_str("G04 Created by netsmith*\n");
        o.push_str("%MOMM*%\n%LPD*%\nG01*\nG04 APERTURE LIST*\n");
        let mut macros: Vec<&'static str> = Vec::new();
        for (ap, _) in &self.aps {
            if let Some(m) = ap.macro_text()
                && !macros.contains(&m)
            {
                macros.push(m);
            }
        }
        if !macros.is_empty() {
            o.push_str("G04 Aperture macros list*\n");
            for m in &macros {
                o.push_str(m);
                o.push('\n');
            }
            o.push_str("G04 Aperture macros list end*\n");
        }
        for (i, (ap, function)) in self.aps.iter().enumerate() {
            if let Some(f) = function {
                let _ = writeln!(o, "%TA.AperFunction,{f}*%");
            }
            let _ = writeln!(o, "%ADD{}{}*%", 10 + i, ap.definition());
            if function.is_some() {
                o.push_str("%TD*%\n");
            }
        }
        o.push_str("G04 APERTURE END LIST*\n");

        let mut w = Writer {
            gerber: self,
            out: o,
            ap: None,
            attrs: Vec::new(),
            function: None,
            dark: true,
            at: None,
            mode: Mode::Linear,
        };
        for item in &self.items {
            w.item(item);
        }
        w.close_attrs();
        w.out.push_str("M02*\n");
        w.out
    }
}

/// A fixed-looking id for a project, from its name (the format wants a UUID-shaped id).
fn project_id(name: &str) -> String {
    let mut h: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for b in name.bytes() {
        h ^= u128::from(b);
        h = h.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    let x = format!("{h:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &x[0..8],
        &x[8..12],
        &x[12..16],
        &x[16..20],
        &x[20..32]
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Linear,
    Cw,
    Ccw,
}

struct Writer<'a> {
    gerber: &'a Gerber,
    out: String,
    ap: Option<usize>,
    attrs: Vec<String>,
    function: Option<&'static str>,
    dark: bool,
    /// Where the pen is, in file units (integers of a millionth of a millimetre).
    at: Option<(i64, i64)>,
    mode: Mode,
}

fn units(v: f64) -> i64 {
    (v * 1e6).round() as i64
}

impl Writer<'_> {
    fn item(&mut self, item: &Item) {
        if let Cmd::Polarity(dark) = item.cmd {
            if dark != self.dark {
                self.out
                    .push_str(if dark { "%LPD*%\n" } else { "%LPC*%\n" });
                self.dark = dark;
            }
            return;
        }
        if item.attrs != self.attrs {
            self.close_attrs();
            for a in &item.attrs {
                let _ = writeln!(self.out, "%TO.{a}*%");
            }
            self.attrs.clone_from(&item.attrs);
        }
        if item.function != self.function {
            if self.function.is_some() {
                self.out.push_str("%TD.AperFunction*%\n");
            }
            if let Some(f) = item.function {
                let _ = writeln!(self.out, "%TA.AperFunction,{f}*%");
            }
            self.function = item.function;
        }
        match &item.cmd {
            Cmd::Polarity(_) => {}
            Cmd::Flash { ap, at } => {
                self.select(*ap);
                let p = self.coord(*at);
                let _ = writeln!(self.out, "X{}Y{}D03*", p.0, p.1);
                self.at = Some(p);
            }
            Cmd::Line { ap, a, b } => {
                self.select(*ap);
                self.mode(Mode::Linear);
                self.move_to(*a);
                let p = self.coord(*b);
                let _ = writeln!(self.out, "X{}Y{}D01*", p.0, p.1);
                self.at = Some(p);
            }
            Cmd::Arc { ap, arc } => {
                self.select(*ap);
                self.move_to(arc.start);
                self.arc_to(arc.end, arc.centre, arc.ccw);
            }
            Cmd::Region(contour) => {
                self.mode(Mode::Linear);
                self.out.push_str("G36*\n");
                self.move_to(contour.start);
                for seg in &contour.segs {
                    match *seg {
                        Seg::Line(to) => {
                            self.mode(Mode::Linear);
                            let p = self.coord(to);
                            let _ = writeln!(self.out, "X{}Y{}D01*", p.0, p.1);
                            self.at = Some(p);
                        }
                        Seg::Arc { to, centre, ccw } => self.arc_to(to, centre, ccw),
                    }
                }
                // Close back to the start if the contour does not end there.
                let start = self.coord(contour.start);
                if self.at != Some(start) {
                    self.mode(Mode::Linear);
                    let _ = writeln!(self.out, "X{}Y{}D01*", start.0, start.1);
                    self.at = Some(start);
                }
                self.out.push_str("G37*\n");
                self.at = None;
            }
        }
    }

    fn close_attrs(&mut self) {
        if !self.attrs.is_empty() {
            self.out.push_str("%TD*%\n");
            self.attrs.clear();
            self.function = None;
        } else if self.function.is_some() {
            self.out.push_str("%TD.AperFunction*%\n");
            self.function = None;
        }
    }

    fn select(&mut self, ap: usize) {
        if self.ap != Some(ap) {
            let _ = writeln!(self.out, "D{}*", 10 + ap);
            self.ap = Some(ap);
        }
    }

    fn coord(&self, p: Pt) -> (i64, i64) {
        let q = self.gerber.at(p);
        (units(q.x), units(q.y))
    }

    fn mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.out.push_str(match mode {
                Mode::Linear => "G01*\n",
                Mode::Cw => "G02*\n",
                Mode::Ccw => "G03*\n",
            });
            self.mode = mode;
        }
    }

    fn move_to(&mut self, p: Pt) {
        let c = self.coord(p);
        if self.at != Some(c) {
            let _ = writeln!(self.out, "X{}Y{}D02*", c.0, c.1);
            self.at = Some(c);
        }
    }

    /// An arc from where the pen is, in multi-quadrant mode, which needs no `G75` again
    /// after the first (written with the header's `G01`; here before each arc, which is
    /// harmless and keeps the file easy to read).
    fn arc_to(&mut self, to: Pt, centre: Pt, ccw: bool) {
        let (from, end) = (self.at.unwrap_or((0, 0)), self.coord(to));
        let c = self.coord(centre);
        self.out.push_str("G75*\n");
        self.mode(if ccw { Mode::Ccw } else { Mode::Cw });
        let _ = writeln!(
            self.out,
            "X{}Y{}I{}J{}D01*",
            end.0,
            end.1,
            c.0 - from.0,
            c.1 - from.1
        );
        self.at = Some(end);
        // `G01` again, so that a stroke or region after it is straight.
        self.mode(Mode::Linear);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flash_is_written_in_the_files_frame_with_the_origin_taken_off_and_y_up() {
        let mut g = Gerber::new(Pt::new(100.0, 200.0));
        g.flash(
            Ap::Rect(1.7, 1.7),
            Some("ComponentPad"),
            Pt::new(124.29, 224.23),
            &["P,J1,1".into(), "N,GND".into()],
        );
        let t = g.text("Copper,L1,Top", false, "Board");
        assert!(t.contains("%TA.AperFunction,ComponentPad*%\n%ADD10R,1.700000X1.700000*%\n%TD*%"));
        assert!(
            t.contains("%TO.P,J1,1*%\n%TO.N,GND*%\nD10*\nX24290000Y-24230000D03*\n%TD*%\nM02*"),
            "{t}"
        );
        assert!(t.contains(
            "%TF.FileFunction,Copper,L1,Top*%\n%TF.FilePolarity,Positive*%\n%FSLAX46Y46*%"
        ));
        assert!(t.contains("%MOMM*%"));
    }

    #[test]
    fn apertures_are_numbered_by_first_use_and_shared() {
        let mut g = Gerber::new(Pt::default());
        g.line(
            0.2,
            Some("Conductor"),
            Pt::new(0.0, 0.0),
            Pt::new(1.0, 0.0),
            &[],
        );
        g.flash(Ap::Circle(0.5), None, Pt::new(1.0, 1.0), &[]);
        g.line(
            0.2,
            Some("Conductor"),
            Pt::new(1.0, 0.0),
            Pt::new(1.0, 1.0),
            &[],
        );
        let t = g.text("Copper,L1,Top", false, "B");
        assert_eq!(t.matches("%ADD").count(), 2);
        assert!(t.contains("%ADD10C,0.200000*%") && t.contains("%ADD11C,0.500000*%"));
        // The pen is moved only when it is not where the stroke starts.
        assert!(t.contains("X0Y0D02*\nX1000000Y0D01*\n"));
        assert!(
            t.contains("D11*\nX1000000Y-1000000D03*\nD10*\nX1000000Y0D02*\nX1000000Y-1000000D01*")
        );
    }

    #[test]
    fn a_macro_is_defined_once_before_the_apertures_that_use_it() {
        let mut g = Gerber::new(Pt::default());
        let c = [
            Pt::new(-0.2, -0.3),
            Pt::new(0.2, -0.3),
            Pt::new(0.2, 0.3),
            Pt::new(-0.2, 0.3),
        ];
        g.flash(
            Ap::RoundRect {
                radius: 0.1,
                corners: c,
            },
            None,
            Pt::new(1.0, 1.0),
            &[],
        );
        g.flash(
            Ap::RotRect {
                w: 1.0,
                h: 2.0,
                angle: 30.0,
            },
            None,
            Pt::new(2.0, 1.0),
            &[],
        );
        let t = g.text("Copper,L1,Top", false, "B");
        assert_eq!(t.matches("%AMRoundRect*").count(), 1);
        assert!(
            t.find("%AMRoundRect*").unwrap()
                < t.find("%ADD10RoundRect,0.100000X-0.200000X-0.300000")
                    .unwrap()
        );
        assert!(t.contains("%ADD11RotRect,1.000000X2.000000X30.000000*%"));
    }

    #[test]
    fn a_region_with_an_arc_and_clear_polarity() {
        let mut g = Gerber::new(Pt::default());
        g.region(
            Contour {
                start: Pt::new(0.0, 0.0),
                segs: vec![
                    Seg::Line(Pt::new(2.0, 0.0)),
                    Seg::Arc {
                        to: Pt::new(0.0, 0.0),
                        centre: Pt::new(1.0, 0.0),
                        ccw: true,
                    },
                ],
            },
            Some("Conductor"),
            &["N,GND".into()],
        );
        g.polarity(false);
        g.flash(Ap::Circle(0.4), None, Pt::new(1.0, 0.5), &[]);
        let t = g.text("Copper,L1,Top", false, "B");
        assert!(t.contains("%TO.N,GND*%\n%TA.AperFunction,Conductor*%\nG36*\nX0Y0D02*\nX2000000Y0D01*\nG75*\nG03*\nX0Y0I-1000000J0D01*\nG01*\nG37*\n%LPC*%"), "{t}");
        assert!(t.contains("%LPC*%"));
    }

    #[test]
    fn attribute_values_are_escaped() {
        assert_eq!(escape("Net-(R1-Pad1)"), "Net-(R1-Pad1)");
        assert_eq!(escape("a,b*c%d"), "a\\u002Cb\\u002Ac\\u0025d");
        assert_eq!(escape("\u{b5}F"), "\\u00B5F");
    }

    #[test]
    fn the_project_id_looks_like_a_uuid_and_is_stable() {
        let a = project_id("LedExample");
        assert_eq!(a, project_id("LedExample"));
        assert_ne!(a, project_id("Other"));
        let parts: Vec<usize> = a.split('-').map(str::len).collect();
        assert_eq!(parts, [8, 4, 4, 4, 12]);
    }
}
