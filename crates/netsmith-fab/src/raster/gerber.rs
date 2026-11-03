//! The RS-274X (Gerber X2) reader and the rasteriser behind [`render_gerber`].

use super::geom::{
    Item, Map, Prim, Pt, Rule, Tess, capsule, convex_hull, disc, fill, ngon, rotate,
};
use super::macros::{MacroDef, instantiate};
use super::{Image, RasterError, View};
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

/// How the digits of a coordinate are read, from `%FS`.
#[derive(Clone, Copy, Debug)]
struct Format {
    xi: usize,
    xd: usize,
    yi: usize,
    yd: usize,
    /// Trailing zeros are omitted (`T`); otherwise leading zeros are (`L`).
    trailing_omitted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Interp {
    Linear,
    Cw,
    Ccw,
}

/// `%LM`, `%LR`, `%LS`: applied to the aperture as it is flashed or drawn, in the order
/// mirror, scale, rotate.
#[derive(Clone, Copy, Debug)]
struct Transform {
    mirror_x: bool,
    mirror_y: bool,
    rot: f64,
    scale: f64,
}

impl Transform {
    const IDENTITY: Self = Self {
        mirror_x: false,
        mirror_y: false,
        rot: 0.0,
        scale: 1.0,
    };

    fn apply(&self, p: Pt) -> Pt {
        let x = if self.mirror_x { -p.0 } else { p.0 };
        let y = if self.mirror_y { -p.1 } else { p.1 };
        rotate((x * self.scale, y * self.scale), self.rot)
    }
}

/// The convex shape a stroke sweeps.
#[derive(Clone, Debug)]
enum Brush {
    Circle(f64),
    Convex(Vec<Pt>),
    None,
}

#[derive(Clone, Debug)]
struct Aperture {
    /// What a flash draws, around the aperture's origin, in millimetres.
    prims: Vec<Prim>,
    brush: Brush,
}

struct Region {
    contours: Vec<Vec<Pt>>,
    current: Vec<Pt>,
}

struct Parser<'a> {
    sink: &'a mut dyn FnMut(&Item),
    tess: Tess,
    format: Option<Format>,
    /// Millimetres per file unit.
    unit: Option<f64>,
    apertures: HashMap<u32, Aperture>,
    macros: HashMap<String, MacroDef>,
    current: Option<u32>,
    interp: Interp,
    multi: bool,
    pos: Pt,
    last_op: Option<u32>,
    dark: bool,
    transform: Transform,
    region: Option<Region>,
    done: bool,
}

fn parse_f64(t: &str, line: usize) -> Result<f64, RasterError> {
    t.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| RasterError::new(line, format!("`{t}` is not a number")))
}

/// Splits `G01X10Y-20D01` into letter and digits pairs.
fn words(block: &str, line: usize) -> Result<Vec<(char, &str)>, RasterError> {
    let mut out = Vec::new();
    let mut chars = block.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if !matches!(c, 'G' | 'X' | 'Y' | 'I' | 'J' | 'D' | 'M') {
            return Err(RasterError::new(
                line,
                format!("unsupported command `{block}` (unknown word `{c}`)"),
            ));
        }
        let start = i + 1;
        let mut end = start;
        while let Some(&(j, d)) = chars.peek() {
            if d.is_ascii_digit() || matches!(d, '+' | '-' | '.') {
                end = j + d.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        out.push((c, &block[start..end]));
    }
    Ok(out)
}

fn int(t: &str, what: &str, line: usize) -> Result<u32, RasterError> {
    t.parse()
        .map_err(|_| RasterError::new(line, format!("`{what}{t}` is not a valid command")))
}

impl<'a> Parser<'a> {
    fn new(sink: &'a mut dyn FnMut(&Item), ppm: f64) -> Self {
        Self {
            sink,
            tess: Tess { ppm },
            format: None,
            unit: None,
            apertures: HashMap::new(),
            macros: HashMap::new(),
            current: None,
            interp: Interp::Linear,
            multi: true,
            pos: (0.0, 0.0),
            last_op: None,
            dark: true,
            transform: Transform::IDENTITY,
            region: None,
            done: false,
        }
    }

    fn run(&mut self, text: &str) -> Result<(), RasterError> {
        let mut i = 0;
        let mut line = 1;
        while i < text.len() && !self.done {
            let rest = &text[i..];
            let c = rest.as_bytes()[0];
            match c {
                b'\n' => {
                    line += 1;
                    i += 1;
                }
                b' ' | b'\t' | b'\r' | b'*' => i += 1,
                b'%' => {
                    let body_start = i + 1;
                    let len = text[body_start..].find('%').ok_or_else(|| {
                        RasterError::new(line, "`%` command is not closed by a second `%`")
                    })?;
                    let body = &text[body_start..body_start + len];
                    self.extended(body, line)?;
                    line += body.matches('\n').count();
                    i = body_start + len + 1;
                }
                _ => {
                    let len = rest.find('*').ok_or_else(|| {
                        RasterError::new(line, "the last block has no closing `*`")
                    })?;
                    let raw = &rest[..len];
                    let block: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
                    self.block(&block, line)?;
                    line += raw.matches('\n').count();
                    i += len + 1;
                }
            }
        }
        if self.region.is_some() {
            return Err(RasterError::new(
                line,
                "region (G36) is never closed by G37",
            ));
        }
        Ok(())
    }

    fn need_unit(&self, line: usize) -> Result<f64, RasterError> {
        self.unit.ok_or_else(|| {
            RasterError::new(
                line,
                "units are not set (`%MOMM*%` or `%MOIN*%`) before use",
            )
        })
    }

    // ---- extended commands ----

    fn extended(&mut self, body: &str, line: usize) -> Result<(), RasterError> {
        // The line of each block: count newlines before it.
        let mut blocks: Vec<(usize, String)> = Vec::new();
        let mut l = line;
        for raw in body.split('*') {
            let lead = raw.len() - raw.trim_start().len();
            let at = l + raw[..lead].matches('\n').count();
            l += raw.matches('\n').count();
            let b: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
            if !b.is_empty() {
                blocks.push((at, b));
            }
        }
        let Some((first_line, first)) = blocks.first().cloned() else {
            return Ok(());
        };
        let code: String = first.chars().take(2).collect();
        match code.as_str() {
            "TF" | "TA" | "TO" | "TD" | "LN" => Ok(()),
            "FS" => self.set_format(&first, first_line),
            "MO" => match &first[2..] {
                "MM" => {
                    self.unit = Some(1.0);
                    Ok(())
                }
                "IN" => {
                    self.unit = Some(25.4);
                    Ok(())
                }
                other => Err(RasterError::new(
                    first_line,
                    format!("`%MO{other}` is not MM or IN"),
                )),
            },
            "AM" => {
                let name = first[2..].to_string();
                let blocks = blocks.into_iter().skip(1).collect();
                self.macros.insert(name, MacroDef { blocks });
                Ok(())
            }
            "AD" => self.define_aperture(&first, first_line),
            "LP" => match &first[2..] {
                "D" => {
                    self.dark = true;
                    Ok(())
                }
                "C" => {
                    self.dark = false;
                    Ok(())
                }
                other => Err(RasterError::new(
                    first_line,
                    format!("`%LP{other}` is not D or C"),
                )),
            },
            "LM" => {
                let (mx, my) = match &first[2..] {
                    "N" => (false, false),
                    "X" => (true, false),
                    "Y" => (false, true),
                    "XY" => (true, true),
                    other => {
                        return Err(RasterError::new(
                            first_line,
                            format!("`%LM{other}` is not N, X, Y or XY"),
                        ));
                    }
                };
                self.transform.mirror_x = mx;
                self.transform.mirror_y = my;
                Ok(())
            }
            "LR" => {
                self.transform.rot = parse_f64(&first[2..], first_line)?;
                Ok(())
            }
            "LS" => {
                self.transform.scale = parse_f64(&first[2..], first_line)?;
                Ok(())
            }
            "IP" => match &first[2..] {
                "POS" => Ok(()),
                other => Err(RasterError::new(
                    first_line,
                    format!("unsupported command `%IP{other}` (only IPPOS)"),
                )),
            },
            _ => Err(RasterError::new(
                first_line,
                format!("unsupported command `%{first}`"),
            )),
        }
    }

    fn set_format(&mut self, cmd: &str, line: usize) -> Result<(), RasterError> {
        let bad = || RasterError::new(line, format!("unsupported format `%{cmd}`"));
        let b = cmd.as_bytes();
        // FS [L|T|D] [A|I] X.. Y..
        let mut k = 2;
        let trailing_omitted = match b.get(k) {
            Some(b'L') => false,
            Some(b'T') => true,
            Some(b'D') => false,
            _ => return Err(bad()),
        };
        k += 1;
        match b.get(k) {
            Some(b'A') => {}
            Some(b'I') => {
                return Err(RasterError::new(
                    line,
                    format!("unsupported command `%{cmd}` (incremental coordinates)"),
                ));
            }
            _ => return Err(bad()),
        }
        k += 1;
        let digits = |k: &mut usize, axis: u8| -> Option<(usize, usize)> {
            if b.get(*k) != Some(&axis) {
                return None;
            }
            let i = usize::from(b.get(*k + 1)?.checked_sub(b'0').filter(|d| *d <= 9)?);
            let d = usize::from(b.get(*k + 2)?.checked_sub(b'0').filter(|d| *d <= 9)?);
            *k += 3;
            Some((i, d))
        };
        let (xi, xd) = digits(&mut k, b'X').ok_or_else(bad)?;
        let (yi, yd) = digits(&mut k, b'Y').ok_or_else(bad)?;
        if k != b.len() {
            return Err(bad());
        }
        self.format = Some(Format {
            xi,
            xd,
            yi,
            yd,
            trailing_omitted,
        });
        Ok(())
    }

    fn define_aperture(&mut self, cmd: &str, line: usize) -> Result<(), RasterError> {
        let unit = self.need_unit(line)?;
        let rest = cmd[2..]
            .strip_prefix('D')
            .ok_or_else(|| RasterError::new(line, format!("bad aperture definition `%{cmd}`")))?;
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        let n: u32 = rest[..digits]
            .parse()
            .ok()
            .filter(|n| *n >= 10)
            .ok_or_else(|| RasterError::new(line, format!("bad aperture number in `%{cmd}`")))?;
        let spec = &rest[digits..];
        let (name, params) = spec.split_once(',').unwrap_or((spec, ""));
        let values = params
            .split('X')
            .filter(|t| !t.is_empty())
            .map(|t| parse_f64(t, line))
            .collect::<Result<Vec<f64>, _>>()?;
        let ap = match name {
            "C" | "R" | "O" | "P" => self.standard(name, &values, unit, line)?,
            _ => {
                let def = self.macros.get(name).ok_or_else(|| {
                    RasterError::new(line, format!("aperture D{n} uses undefined macro `{name}`"))
                })?;
                let prims = instantiate(def, &values, unit, self.tess)?;
                Aperture {
                    prims,
                    brush: Brush::None,
                }
            }
        };
        self.apertures.insert(n, ap);
        Ok(())
    }

    /// An aperture of the standard kind `C`, `R`, `O` or `P`; `v` is in file units.
    fn standard(
        &self,
        kind: &str,
        v: &[f64],
        unit: f64,
        line: usize,
    ) -> Result<Aperture, RasterError> {
        let tess = self.tess;
        let at = |i: usize| v.get(i).copied();
        let need = |n: usize| -> Result<(), RasterError> {
            if v.len() < n {
                Err(RasterError::new(
                    line,
                    format!("aperture `{kind}` needs {n} parameters, has {}", v.len()),
                ))
            } else {
                Ok(())
            }
        };
        let (outline, brush, hole_at): (Vec<Pt>, Brush, usize) = match kind {
            "C" => {
                need(1)?;
                let r = v[0] * unit / 2.0;
                if r < 0.0 {
                    return Err(RasterError::new(line, "negative circle diameter"));
                }
                let n = tess.circle_n(r);
                (disc((0.0, 0.0), r, n), Brush::Circle(r), 1)
            }
            "R" => {
                need(2)?;
                let (hw, hh) = (v[0] * unit / 2.0, v[1] * unit / 2.0);
                let poly = vec![(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];
                (poly.clone(), Brush::Convex(poly), 2)
            }
            "O" => {
                need(2)?;
                let (w, h) = (v[0] * unit, v[1] * unit);
                let poly = if w >= h {
                    let d = (w - h) / 2.0;
                    capsule((-d, 0.0), (d, 0.0), h / 2.0, tess.circle_n(h / 2.0))
                } else {
                    let d = (h - w) / 2.0;
                    capsule((0.0, -d), (0.0, d), w / 2.0, tess.circle_n(w / 2.0))
                };
                (poly.clone(), Brush::Convex(poly), 2)
            }
            _ => {
                need(2)?;
                let sides = v[1];
                if !(3.0..=12.0).contains(&sides) {
                    return Err(RasterError::new(
                        line,
                        "a polygon aperture has 3 to 12 vertices",
                    ));
                }
                let poly = ngon(
                    (0.0, 0.0),
                    v[0] * unit / 2.0,
                    sides as usize,
                    at(2).unwrap_or(0.0).to_radians(),
                );
                (poly.clone(), Brush::Convex(poly), 3)
            }
        };
        let mut prims = vec![Prim {
            polys: vec![outline],
            rule: Rule::NonZero,
            on: true,
        }];
        let hole: Option<Vec<Pt>> = match (at(hole_at), at(hole_at + 1)) {
            (Some(d), None) if d > 0.0 => {
                let r = d * unit / 2.0;
                Some(disc((0.0, 0.0), r, tess.circle_n(r)))
            }
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => {
                let (hw, hh) = (w * unit / 2.0, h * unit / 2.0);
                Some(vec![(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)])
            }
            _ => None,
        };
        if let Some(h) = hole {
            prims.push(Prim {
                polys: vec![h],
                rule: Rule::NonZero,
                on: false,
            });
        }
        Ok(Aperture { prims, brush })
    }

    // ---- data blocks ----

    fn block(&mut self, block: &str, line: usize) -> Result<(), RasterError> {
        let comment = block.starts_with("G04")
            || (block.starts_with("G4") && !block[2..].starts_with(|c: char| c.is_ascii_digit()));
        if comment {
            return Ok(());
        }
        let (mut x, mut y, mut i, mut j, mut d) = (None, None, None, None, None);
        for (letter, v) in words(block, line)? {
            match letter {
                'G' => self.g_code(int(v, "G", line)?, line)?,
                'M' => match int(v, "M", line)? {
                    0 | 2 => self.done = true,
                    1 => {}
                    n => {
                        return Err(RasterError::new(
                            line,
                            format!("unsupported command `M{n:02}`"),
                        ));
                    }
                },
                'X' => x = Some(v),
                'Y' => y = Some(v),
                'I' => i = Some(v),
                'J' => j = Some(v),
                _ => d = Some(int(v, "D", line)?),
            }
        }
        let has_coords = x.is_some() || y.is_some() || i.is_some() || j.is_some();
        let op = match d {
            Some(n) if n >= 10 => {
                if !self.apertures.contains_key(&n) {
                    return Err(RasterError::new(
                        line,
                        format!("aperture D{n} is not defined"),
                    ));
                }
                self.current = Some(n);
                None
            }
            Some(n) => {
                self.last_op = Some(n);
                Some(n)
            }
            None if has_coords => Some(self.last_op.ok_or_else(|| {
                RasterError::new(
                    line,
                    format!("coordinates without D01, D02 or D03 in `{block}`"),
                )
            })?),
            None => None,
        };
        let Some(op) = op else {
            return Ok(());
        };
        let fmt = self
            .format
            .ok_or_else(|| RasterError::new(line, "coordinates before `%FS`"))?;
        let unit = self.need_unit(line)?;
        let num = |t: Option<&str>, ints: usize, decs: usize| -> Result<Option<f64>, RasterError> {
            t.map(|t| coordinate(t, ints, decs, fmt.trailing_omitted, line).map(|v| v * unit))
                .transpose()
        };
        let to = (
            num(x, fmt.xi, fmt.xd)?.unwrap_or(self.pos.0),
            num(y, fmt.yi, fmt.yd)?.unwrap_or(self.pos.1),
        );
        let ij = (
            num(i, fmt.xi, fmt.xd)?.unwrap_or(0.0),
            num(j, fmt.yi, fmt.yd)?.unwrap_or(0.0),
        );
        match op {
            1 => self.draw(to, ij, line)?,
            2 => self.move_to(),
            3 => self.flash(to, line)?,
            n => {
                return Err(RasterError::new(
                    line,
                    format!("unsupported command `D{n:02}`"),
                ));
            }
        }
        self.pos = to;
        Ok(())
    }

    fn g_code(&mut self, n: u32, line: usize) -> Result<(), RasterError> {
        match n {
            1 => self.interp = Interp::Linear,
            2 => self.interp = Interp::Cw,
            3 => self.interp = Interp::Ccw,
            36 => {
                if self.region.is_some() {
                    return Err(RasterError::new(line, "G36 inside a region"));
                }
                self.region = Some(Region {
                    contours: Vec::new(),
                    current: Vec::new(),
                });
            }
            37 => {
                let Some(mut r) = self.region.take() else {
                    return Err(RasterError::new(line, "G37 without G36"));
                };
                close_contour(&mut r);
                let polys: Vec<Vec<Pt>> = r.contours;
                if !polys.is_empty() {
                    let item = Item {
                        dark: self.dark,
                        prims: vec![Prim {
                            polys,
                            rule: Rule::EvenOdd,
                            on: true,
                        }],
                    };
                    (self.sink)(&item);
                }
            }
            54 | 55 | 90 => {}
            70 => self.unit = Some(25.4),
            71 => self.unit = Some(1.0),
            74 => self.multi = false,
            75 => self.multi = true,
            _ => {
                return Err(RasterError::new(
                    line,
                    format!("unsupported command `G{n:02}`"),
                ));
            }
        }
        Ok(())
    }

    /// `D02`: ends the contour being built, if inside a region.
    fn move_to(&mut self) {
        if let Some(r) = &mut self.region {
            close_contour(r);
        }
    }

    fn flash(&mut self, at: Pt, line: usize) -> Result<(), RasterError> {
        if self.region.is_some() {
            return Err(RasterError::new(line, "D03 (flash) inside a region"));
        }
        let ap = self.aperture(line)?;
        let t = self.transform;
        let prims = ap
            .prims
            .iter()
            .map(|p| Prim {
                polys: p
                    .polys
                    .iter()
                    .map(|poly| {
                        poly.iter()
                            .map(|&q| {
                                let q = t.apply(q);
                                (q.0 + at.0, q.1 + at.1)
                            })
                            .collect()
                    })
                    .collect(),
                rule: p.rule,
                on: p.on,
            })
            .collect();
        (self.sink)(&Item {
            dark: self.dark,
            prims,
        });
        Ok(())
    }

    fn aperture(&self, line: usize) -> Result<&Aperture, RasterError> {
        let n = self.current.ok_or_else(|| {
            RasterError::new(line, "drawing or flashing with no aperture selected")
        })?;
        self.apertures
            .get(&n)
            .ok_or_else(|| RasterError::new(line, format!("aperture D{n} is not defined")))
    }

    fn draw(&mut self, to: Pt, ij: (f64, f64), line: usize) -> Result<(), RasterError> {
        let from = self.pos;
        let path = if self.interp == Interp::Linear {
            vec![to]
        } else {
            self.arc(from, to, ij)
        };
        if let Some(r) = &mut self.region {
            if r.current.is_empty() {
                r.current.push(from);
            }
            r.current.extend(path);
            return Ok(());
        }
        let ap = self.aperture(line)?;
        let t = self.transform;
        let mut chain = vec![from];
        chain.extend(path);
        let polys: Vec<Vec<Pt>> = match &ap.brush {
            Brush::Circle(r) => {
                let r = r * t.scale.abs();
                if r <= 0.0 {
                    return Ok(());
                }
                let n = self.tess.circle_n(r);
                chain
                    .windows(2)
                    .map(|w| capsule(w[0], w[1], r, n))
                    .collect()
            }
            Brush::Convex(poly) => {
                let poly: Vec<Pt> = poly.iter().map(|&q| t.apply(q)).collect();
                chain
                    .windows(2)
                    .map(|w| {
                        let mut pts = Vec::with_capacity(poly.len() * 2);
                        pts.extend(poly.iter().map(|q| (q.0 + w[0].0, q.1 + w[0].1)));
                        pts.extend(poly.iter().map(|q| (q.0 + w[1].0, q.1 + w[1].1)));
                        convex_hull(pts)
                    })
                    .collect()
            }
            Brush::None => {
                return Err(RasterError::new(
                    line,
                    "unsupported command `D01` with a macro aperture (only C, R, O and P can be drawn with)",
                ));
            }
        };
        (self.sink)(&Item {
            dark: self.dark,
            prims: vec![Prim {
                polys,
                rule: Rule::NonZero,
                on: true,
            }],
        });
        Ok(())
    }

    /// The points of the arc after `from`, ending exactly at `to`.
    fn arc(&self, from: Pt, to: Pt, ij: (f64, f64)) -> Vec<Pt> {
        let cw = self.interp == Interp::Cw;
        let sweep_for = |c: Pt, full: bool| -> f64 {
            let a0 = (from.1 - c.1).atan2(from.0 - c.0);
            let a1 = (to.1 - c.1).atan2(to.0 - c.0);
            let mut d = a1 - a0;
            if cw {
                if d >= 0.0 {
                    d -= TAU;
                }
                if full && d.abs() < 1e-12 {
                    d = -TAU;
                }
            } else {
                if d <= 0.0 {
                    d += TAU;
                }
                if full && d.abs() < 1e-12 {
                    d = TAU;
                }
            }
            d
        };
        let same = (from.0 - to.0).abs() < 1e-12 && (from.1 - to.1).abs() < 1e-12;
        let (centre, sweep) = if self.multi {
            let c = (from.0 + ij.0, from.1 + ij.1);
            let s = if same {
                if cw { -TAU } else { TAU }
            } else {
                sweep_for(c, false)
            };
            (c, s)
        } else if same {
            return vec![to];
        } else {
            // Single quadrant: I and J are unsigned; try the four signs and keep the
            // centre that is equally far from both ends on an arc of at most 90 degrees.
            let (i, j) = (ij.0.abs(), ij.1.abs());
            let mut best: Option<(f64, Pt, f64)> = None;
            for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                let c = (from.0 + sx * i, from.1 + sy * j);
                let s = sweep_for(c, false);
                let mismatch =
                    ((from.0 - c.0).hypot(from.1 - c.1) - (to.0 - c.0).hypot(to.1 - c.1)).abs();
                let score = mismatch + if s.abs() > PI / 2.0 + 1e-6 { 1e6 } else { 0.0 };
                if best.is_none_or(|b| score < b.0) {
                    best = Some((score, c, s));
                }
            }
            let (_, c, s) = best.unwrap_or((0.0, from, 0.0));
            (c, s)
        };
        let r0 = (from.0 - centre.0).hypot(from.1 - centre.1);
        let r1 = (to.0 - centre.0).hypot(to.1 - centre.1);
        if r0.max(r1) < 1e-12 {
            return vec![to];
        }
        let a0 = (from.1 - centre.1).atan2(from.0 - centre.0);
        let n = self.tess.arc_n(r0.max(r1), sweep);
        let mut out = Vec::with_capacity(n);
        for k in 1..n {
            let t = k as f64 / n as f64;
            let a = a0 + sweep * t;
            let r = r0 + (r1 - r0) * t;
            out.push((centre.0 + r * a.cos(), centre.1 + r * a.sin()));
        }
        out.push(to);
        out
    }
}

/// Ends the contour being built: it is kept if it has an area.
fn close_contour(r: &mut Region) {
    let c = std::mem::take(&mut r.current);
    if c.len() >= 3 {
        r.contours.push(c);
    }
}

/// Reads one coordinate. Digits are scaled by the format; a number with a decimal point is
/// taken as written.
fn coordinate(
    t: &str,
    ints: usize,
    decs: usize,
    trailing_omitted: bool,
    line: usize,
) -> Result<f64, RasterError> {
    if t.contains('.') {
        return parse_f64(t, line);
    }
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if digits.is_empty() {
        return Err(RasterError::new(line, format!("`{t}` is not a coordinate")));
    }
    let mut digits = digits.to_string();
    if trailing_omitted {
        while digits.len() < ints + decs {
            digits.push('0');
        }
    }
    let v = parse_f64(&digits, line)? / 10f64.powi(decs as i32);
    Ok(if neg { -v } else { v })
}

fn parse(text: &str, ppm: f64, sink: &mut dyn FnMut(&Item)) -> Result<(), RasterError> {
    Parser::new(sink, ppm).run(text)
}

/// The bounding box, in millimetres, of everything the file draws: strokes and flashes
/// including the aperture's own size, and regions. Clear-polarity items count too.
///
/// A file that draws nothing has no extent and gives an error, so a caller that wants a
/// common view for several files should skip such files.
pub fn gerber_extent(text: &str) -> Result<View, RasterError> {
    let (mut lo, mut hi) = (
        (f64::INFINITY, f64::INFINITY),
        (f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    // Fine tessellation, so that the extremes of arcs are not missed.
    parse(text, 400.0, &mut |item| {
        for p in item.prims.iter().filter(|p| p.on) {
            for q in p.polys.iter().flatten() {
                lo = (lo.0.min(q.0), lo.1.min(q.1));
                hi = (hi.0.max(q.0), hi.1.max(q.1));
            }
        }
    })?;
    if lo.0 > hi.0 {
        return Err(RasterError::new(
            0,
            "the file draws nothing, so it has no extent",
        ));
    }
    Ok(View { min: lo, max: hi })
}

/// Draws `item` on `img`.
fn draw_item(img: &mut Image, map: Map, item: &Item) {
    let (w, h) = (img.width, img.height);
    let value = u8::from(item.dark);
    if let [p] = item.prims.as_slice()
        && p.on
    {
        fill(&p.polys, map, p.rule, w, h, |row, a, b| {
            img.bits[row * w + a..row * w + b].fill(value);
        });
        return;
    }
    // Compose the primitives on a scratch grid over the item's bounding box first.
    let (mut lo, mut hi) = (
        (f64::INFINITY, f64::INFINITY),
        (f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    for q in item.prims.iter().flat_map(|p| p.polys.iter().flatten()) {
        let q = map.apply(*q);
        lo = (lo.0.min(q.0), lo.1.min(q.1));
        hi = (hi.0.max(q.0), hi.1.max(q.1));
    }
    if !(lo.0.is_finite() && hi.0.is_finite() && lo.1.is_finite() && hi.1.is_finite()) {
        return;
    }
    let bx0 = lo.0.floor().clamp(0.0, w as f64) as usize;
    let by0 = lo.1.floor().clamp(0.0, h as f64) as usize;
    let bx1 = hi.0.ceil().clamp(0.0, w as f64) as usize;
    let by1 = hi.1.ceil().clamp(0.0, h as f64) as usize;
    if bx0 >= bx1 || by0 >= by1 {
        return;
    }
    let (sw, sh) = (bx1 - bx0, by1 - by0);
    let local = Map {
        scale: map.scale,
        ox: map.ox - bx0 as f64,
        oy: map.oy - by0 as f64,
    };
    let mut scratch = vec![0u8; sw * sh];
    for p in &item.prims {
        let v = u8::from(p.on);
        fill(&p.polys, local, p.rule, sw, sh, |row, a, b| {
            scratch[row * sw + a..row * sw + b].fill(v);
        });
    }
    for row in 0..sh {
        for col in 0..sw {
            if scratch[row * sw + col] != 0 {
                img.bits[(by0 + row) * w + bx0 + col] = value;
            }
        }
    }
}

/// Renders the final dark/clear composite of a Gerber file: the copper or ink of the
/// layer. `view` is the area to draw, in millimetres with Y up, at `px_per_mm` pixels per
/// millimetre; the image is `ceil(width * px_per_mm)` by `ceil(height * px_per_mm)` pixels
/// and its lower-left corner is `view.min`.
///
/// A pixel is lit if its centre is inside the shape (a stroke, a flash or a region), so a
/// stroke narrower than a pixel may light no pixels or one row of them. Items are applied
/// in file order: a dark item lights pixels, a clear item (`%LPC*%`) unlights them. The
/// file's `%TF.FilePolarity` and image polarity are ignored; the caller knows what the
/// layer means.
///
/// Supported: `%FS` (absolute, any digits, leading or trailing zeros omitted), `%MO`,
/// `G70`/`G71`, `%AD` for `C`, `R`, `O`, `P` (with a round or rectangular hole) and for
/// macros, `%AM` with primitives 1, 2, 4, 5, 7, 20, 21 and 22, variables and the
/// arithmetic `+ - x / ( )`, `G01`/`G02`/`G03`, `G74`/`G75`, `D01`/`D02`/`D03`,
/// `G36`/`G37` regions (filled even-odd, arcs allowed), `%LP`, `%LM`, `%LR`, `%LS`, `G04`
/// and the attribute commands `%TF %TA %TO %TD`, which are ignored.
///
/// A stroke is the Minkowski sum of the segment or arc and the aperture, so a rectangular
/// aperture is swept as the shape it is; a macro aperture cannot be drawn with. Anything
/// else (`%SR`, `%AB`, macro primitive 6, incremental coordinates, ...) is an error that
/// names the command and its line.
pub fn render_gerber(text: &str, view: &View, px_per_mm: f64) -> Result<Image, RasterError> {
    let mut img = Image::blank(view, px_per_mm)?;
    let ytop = img.y0 + img.height as f64 / px_per_mm;
    let map = Map {
        scale: px_per_mm,
        ox: -img.x0 * px_per_mm,
        oy: ytop * px_per_mm,
    };
    parse(text, px_per_mm, &mut |item| draw_item(&mut img, map, item))?;
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Format 2.4 in millimetres, so `X50000` is 5 mm.
    const HEAD: &str = "%FSLAX24Y24*%\n%MOMM*%\n";

    fn view10() -> View {
        View {
            min: (0.0, 0.0),
            max: (10.0, 10.0),
        }
    }

    fn render(body: &str, ppm: f64) -> Image {
        let src = format!("{HEAD}{body}\nM02*\n");
        render_gerber(&src, &view10(), ppm).unwrap()
    }

    /// Whether the pixel containing (x, y) mm is lit.
    fn at(img: &Image, x: f64, y: f64) -> bool {
        let col = ((x - img.x0) * img.px_per_mm).floor() as usize;
        let row = img.height - 1 - ((y - img.y0) * img.px_per_mm).floor() as usize;
        img.bits[row * img.width + col] != 0
    }

    fn near(got: usize, want: f64, tol: f64) {
        assert!(
            (got as f64 - want).abs() <= tol,
            "got {got} lit pixels, wanted {want} +- {tol}"
        );
    }

    #[test]
    fn flash_circle() {
        let img = render("%ADD10C,2.0*%\nD10*\nX50000Y50000D03*", 10.0);
        near(img.lit(), PI * 100.0, 5.0);
        assert!(at(&img, 5.0, 5.0) && at(&img, 5.9, 5.0) && !at(&img, 6.2, 5.0));
        assert!(!at(&img, 0.5, 0.5));
    }

    #[test]
    fn flash_rectangle_is_exact_on_the_grid() {
        let img = render("%ADD10R,2.0X1.0*%\nD10*\nX50000Y50000D03*", 10.0);
        assert_eq!(img.lit(), 200);
        assert!(
            at(&img, 4.1, 4.6) && at(&img, 5.9, 5.4) && !at(&img, 6.1, 5.0) && !at(&img, 5.0, 5.6)
        );
    }

    #[test]
    fn flash_obround() {
        let img = render("%ADD10O,4.0X2.0*%\nD10*\nX50000Y50000D03*", 10.0);
        near(img.lit(), (4.0 - 2.0) * 2.0 * 100.0 + PI * 100.0, 8.0);
        let tall = render("%ADD10O,2.0X4.0*%\nD10*\nX50000Y50000D03*", 10.0);
        assert_eq!(tall.lit() / 10, img.lit() / 10);
        assert!(at(&tall, 5.0, 6.9) && !at(&tall, 6.9, 5.0));
    }

    #[test]
    fn flash_polygon_with_rotation() {
        // A square with its vertices on the axes: a diamond of diagonal 4, area 8.
        let img = render("%ADD10P,4.0X4X0*%\nD10*\nX50000Y50000D03*", 10.0);
        near(img.lit(), 800.0, 25.0);
        assert!(at(&img, 6.7, 5.0) && !at(&img, 6.5, 6.5));
        // Rotated by 45 degrees it is an axis-aligned square of side 2 sqrt 2.
        let sq = render("%ADD10P,4.0X4X45*%\nD10*\nX50000Y50000D03*", 10.0);
        near(sq.lit(), 800.0, 25.0);
        assert!(at(&sq, 6.3, 6.3) && !at(&sq, 6.9, 5.0));
    }

    #[test]
    fn aperture_holes() {
        let round = render("%ADD10C,4.0X1.0*%\nD10*\nX50000Y50000D03*", 10.0);
        near(round.lit(), PI * (4.0 - 0.25) * 100.0, 15.0);
        assert!(!at(&round, 5.0, 5.0) && at(&round, 6.0, 5.0));
        let square = render("%ADD10R,4.0X4.0X2.0X1.0*%\nD10*\nX50000Y50000D03*", 10.0);
        assert_eq!(square.lit(), 1600 - 200);
        assert!(!at(&square, 5.5, 5.2) && at(&square, 5.0, 6.5));
    }

    #[test]
    fn macro_with_an_expression() {
        let img = render(
            "%AMDOT*\n0 a comment, with a comma*\n1,1,$1+$1,0,0*%\n%ADD10DOT,1.0*%\nD10*\nX50000Y50000D03*",
            10.0,
        );
        near(img.lit(), PI * 100.0, 5.0);
    }

    #[test]
    fn macro_variables_and_parentheses() {
        let assign = render(
            "%AMDOT*\n$3=$1x2*\n1,1,$3,0,0*%\n%ADD10DOT,1.0*%\nD10*\nX50000Y50000D03*",
            10.0,
        );
        near(assign.lit(), PI * 100.0, 5.0);
        let nested = render(
            "%AMDOT*\n$2=($1+1)/2x2-1*\n1,1,$2,0,0*%\n%ADD10DOT,1.0*%\nD10*\nX50000Y50000D03*",
            10.0,
        );
        // $2 = (1 + 1) / 2 x 2 - 1 = 1: a circle of diameter 1.
        near(nested.lit(), PI * 25.0, 4.0);
    }

    #[test]
    fn kicad_round_rectangle_macro() {
        let src = "%AMRoundRect*\n0 Rectangle with rounded corners*\n0 $1 Rounding radius*\n0 $2 $3 $4 $5 $6 $7 $8 $9 X,Y pos of 4 corners*\n0 Add a 4 corners polygon primitive as box body*\n4,1,4,$2,$3,$4,$5,$6,$7,$8,$9,$2,$3,0*\n0 Add four circle primitives for the rounded corners*\n1,1,$1+$1,$2,$3*\n1,1,$1+$1,$4,$5*\n1,1,$1+$1,$6,$7*\n1,1,$1+$1,$8,$9*\n0 Add four rect primitives between the rounded corners*\n20,1,$1+$1,$2,$3,$4,$5,0*\n20,1,$1+$1,$4,$5,$6,$7,0*\n20,1,$1+$1,$6,$7,$8,$9,0*\n20,1,$1+$1,$8,$9,$2,$3,0*%\n%ADD10RoundRect,0.25X-0.75X-0.25X0.75X-0.25X0.75X0.25X-0.75X0.25X0*%\nD10*\nX50000Y50000D03*";
        let img = render(src, 20.0);
        near(img.lit(), (2.0 - (4.0 - PI) * 0.0625) * 400.0, 10.0);
        // The corner is rounded: the square corner is dark, the middle of the edge is lit.
        assert!(!at(&img, 5.99, 5.49) && at(&img, 5.8, 5.4) && at(&img, 5.0, 5.49));
    }

    #[test]
    fn macro_primitive_rotation() {
        // A 4 x 1 centre line rotated by 90 degrees stands upright.
        let img = render(
            "%AMBAR*\n21,1,4,1,0,0,90*%\n%ADD10BAR*%\nD10*\nX50000Y50000D03*",
            10.0,
        );
        assert_eq!(img.lit(), 400);
        assert!(at(&img, 5.0, 6.9) && !at(&img, 6.9, 5.0));
        // The rotation of a circle moves its centre about the macro origin.
        let c = render(
            "%AMDOT*\n1,1,1,2,0,90*%\n%ADD10DOT*%\nD10*\nX50000Y50000D03*",
            10.0,
        );
        assert!(at(&c, 5.0, 7.0) && !at(&c, 7.0, 5.0));
    }

    #[test]
    fn macro_exposure_off_cuts_the_macro_only() {
        let src = "%AMRING*\n1,1,4,0,0*\n1,0,2,0,0*%\n%ADD10RING*%\nD10*\nX50000Y50000D03*";
        let img = render(src, 10.0);
        near(img.lit(), PI * 3.0 * 100.0, 8.0);
        assert!(!at(&img, 5.0, 5.0) && at(&img, 6.5, 5.0));
        // Earlier copper outside the macro's own shapes is not cut by its hole.
        let under = render(
            &format!(
                "%ADD11R,10.0X10.0*%\nD11*\nX50000Y50000D03*\n%LPC*%\n{}",
                &src[src.find("%AM").unwrap()..]
            ),
            10.0,
        );
        assert!(under.lit() > 9000);
    }

    #[test]
    fn macro_outline_and_polygon_and_thermal() {
        let tri = render(
            "%AMTRI*\n4,1,3,0,0,4,0,0,4,0,0,0*%\n%ADD10TRI*%\nD10*\nX10000Y10000D03*",
            10.0,
        );
        near(tri.lit(), 800.0, 25.0);
        let hex = render(
            "%AMHEX*\n5,1,6,0,0,4,0*%\n%ADD10HEX*%\nD10*\nX50000Y50000D03*",
            10.0,
        );
        near(hex.lit(), 3.0 * 3f64.sqrt() / 2.0 * 4.0 * 100.0, 25.0);
        let th = render(
            "%AMTH*\n7,0,0,4,2,0.5,0*%\n%ADD10TH*%\nD10*\nX50000Y50000D03*",
            10.0,
        );
        assert!(
            !at(&th, 5.0, 5.0) && at(&th, 6.2, 6.2) && !at(&th, 6.5, 5.0) && !at(&th, 5.0, 6.5)
        );
        assert!(at(&th, 3.8, 3.8) && !at(&th, 3.5, 5.0) && !at(&th, 7.5, 7.5));
    }

    #[test]
    fn stroke_with_round_caps() {
        let img = render(
            "%ADD10C,1.0*%\nD10*\nX20000Y50000D02*\nX80000Y50000D01*",
            10.0,
        );
        near(img.lit(), 6.0 * 10.0 * 10.0 + PI * 25.0, 8.0);
        assert!(
            at(&img, 1.6, 5.0) && !at(&img, 1.4, 5.0) && at(&img, 8.4, 5.0) && !at(&img, 8.6, 5.0)
        );
        assert!(!at(&img, 5.0, 5.6));
    }

    #[test]
    fn stroke_with_rectangular_aperture_is_swept() {
        let img = render(
            "%ADD10R,1.0X1.0*%\nD10*\nX20000Y50000D02*\nX80000Y50000D01*",
            10.0,
        );
        assert_eq!(img.lit(), 700);
        let diag = render(
            "%ADD10R,1.0X1.0*%\nD10*\nX20000Y20000D02*\nX80000Y80000D01*",
            10.0,
        );
        // Swept square: 6 sqrt 2 x 1 plus the square's own corners: 6 x 1 + 6 x 1 + 1.
        near(diag.lit(), 1300.0, 12.0);
        assert!(at(&diag, 2.4, 1.6) && !at(&diag, 2.0, 8.0));
    }

    #[test]
    fn stroke_with_obround_aperture_and_modal_operation() {
        let img = render(
            "%ADD10O,2.0X1.0*%\nD10*\nX20000Y50000D02*\nX80000Y50000D01*\nX80000Y30000*",
            10.0,
        );
        assert!(at(&img, 8.0, 3.2) && at(&img, 8.0, 5.0) && !at(&img, 2.0, 3.0));
    }

    #[test]
    fn region_with_a_keyhole_slit() {
        let sq = "G36*\nX20000Y20000D02*\nG01*\nX80000Y20000D01*\nX80000Y80000D01*\nX20000Y80000D01*\nX20000Y20000D01*\nG37*";
        assert_eq!(render(sq, 10.0).lit(), 3600);
        let slit = "G36*\nX20000Y20000D02*\nG01*\nX80000Y20000D01*\nX80000Y80000D01*\nX50000Y80000D01*\nX50000Y50000D01*\nX50000Y80000D01*\nX20000Y80000D01*\nG37*";
        assert_eq!(render(slit, 10.0).lit(), 3600);
    }

    #[test]
    fn region_with_a_hole_contour() {
        let src = "G36*\nX20000Y20000D02*\nG01*\nX80000Y20000D01*\nX80000Y80000D01*\nX20000Y80000D01*\nX20000Y20000D01*\nX40000Y40000D02*\nX60000Y40000D01*\nX60000Y60000D01*\nX40000Y60000D01*\nX40000Y40000D01*\nG37*";
        assert_eq!(render(src, 10.0).lit(), 3600 - 400);
    }

    #[test]
    fn arc_stroke() {
        // Clockwise from (2, 5) to (8, 5) about (5, 5): over the top.
        let src = "%ADD10C,0.2*%\nD10*\nG75*\nX20000Y50000D02*\nG02*\nX80000Y50000I30000J0D01*";
        let img = render(src, 20.0);
        near(img.lit(), PI * 3.0 * 0.2 * 400.0 + PI * 0.01 * 400.0, 25.0);
        assert!(at(&img, 5.0, 8.0) && !at(&img, 5.0, 2.0) && !at(&img, 5.0, 5.0));
        let ccw = render(&src.replace("G02*", "G03*"), 20.0);
        assert!(at(&ccw, 5.0, 2.0) && !at(&ccw, 5.0, 8.0));
    }

    #[test]
    fn full_circle_stroke() {
        let src = "%ADD10C,0.2*%\nD10*\nG75*\nX20000Y50000D02*\nG03*\nX20000Y50000I30000J0D01*";
        let img = render(src, 20.0);
        near(img.lit(), TAU * 3.0 * 0.2 * 400.0, 30.0);
        assert!(at(&img, 5.0, 8.0) && at(&img, 5.0, 2.0) && !at(&img, 5.0, 5.0));
    }

    #[test]
    fn arc_region_is_a_half_disc() {
        let src = "G36*\nG75*\nX20000Y50000D02*\nG02*\nX80000Y50000I30000J0D01*\nG01*\nX20000Y50000D01*\nG37*";
        let img = render(src, 20.0);
        near(img.lit(), PI * 9.0 / 2.0 * 400.0, 30.0);
        assert!(at(&img, 5.0, 7.5) && !at(&img, 5.0, 4.5));
    }

    #[test]
    fn single_quadrant_arc() {
        // A quarter circle about (5, 5) from (8, 5) to (5, 8), counter-clockwise, with
        // unsigned offsets.
        let src = "%ADD10C,0.2*%\nD10*\nG74*\nX80000Y50000D02*\nG03*\nX50000Y80000I30000J0D01*";
        let img = render(src, 20.0);
        near(img.lit(), PI / 2.0 * 3.0 * 0.2 * 400.0, 25.0);
        assert!(
            at(&img, 5.0 + 3.0 * 0.5f64.sqrt(), 5.0 + 3.0 * 0.5f64.sqrt()) && !at(&img, 2.2, 2.2)
        );
    }

    #[test]
    fn arcs_stay_within_a_quarter_pixel_of_the_circle() {
        let mut points = Vec::new();
        parse(
            &format!("{HEAD}%ADD10C,0.01*%\nD10*\nG75*\nX20000Y50000D02*\nG02*\nX80000Y50000I30000J0D01*\nM02*"),
            10.0,
            &mut |item| points.extend(item.prims[0].polys.iter().flatten().copied()),
        )
        .unwrap();
        // Chord midpoints are the worst case: every vertex of the thin stroke is within
        // r +- 0.005 mm of the centre line, and the line's chords sag at most 0.025 mm.
        let worst = points
            .iter()
            .map(|p| ((p.0 - 5.0).hypot(p.1 - 5.0) - 3.0).abs())
            .fold(0.0, f64::max);
        assert!(worst <= 0.005 + 1e-9, "{worst}");
    }

    #[test]
    fn clear_polarity_cuts_a_hole() {
        let src = "%ADD10R,6.0X6.0*%\n%ADD11C,2.0*%\nD10*\nX50000Y50000D03*\n%LPC*%\nD11*\nX50000Y50000D03*";
        let img = render(src, 10.0);
        near(img.lit(), 3600.0 - PI * 100.0, 12.0);
        assert!(!at(&img, 5.0, 5.0) && at(&img, 3.0, 3.0));
        // Dark again over the hole.
        let again = render(
            "%ADD10R,6.0X6.0*%\n%ADD11C,2.0*%\n%ADD12C,1.0*%\nD10*\nX50000Y50000D03*\n%LPC*%\nD11*\nX50000Y50000D03*\n%LPD*%\nD12*\nX50000Y50000D03*",
            10.0,
        );
        assert!(at(&again, 5.0, 5.0) && !at(&again, 5.8, 5.0));
    }

    #[test]
    fn clear_region_cuts_a_stroke() {
        let src = "%ADD10C,2.0*%\nD10*\nX10000Y50000D02*\nX90000Y50000D01*\n%LPC*%\nG36*\nX40000Y30000D02*\nG01*\nX60000Y30000D01*\nX60000Y70000D01*\nX40000Y70000D01*\nG37*";
        let img = render(src, 10.0);
        assert!(!at(&img, 5.0, 5.0) && at(&img, 3.0, 5.0) && at(&img, 7.0, 5.0));
    }

    #[test]
    fn aperture_transforms() {
        let rect = "%ADD10R,4.0X2.0*%\nD10*\nX50000Y50000D03*";
        let rot = render(&format!("%LR90*%\n{rect}"), 10.0);
        assert!(at(&rot, 5.0, 6.9) && !at(&rot, 6.9, 5.0));
        let scaled = render(&format!("%LS0.5*%\n{rect}"), 10.0);
        assert_eq!(scaled.lit(), 200);
        let mirrored = render(
            "%ADD10R,2.0X2.0*%\n%AMOFF*\n21,1,2,2,2,0,0*%\n%ADD11OFF*%\n%LMX*%\nD11*\nX50000Y50000D03*",
            10.0,
        );
        assert!(at(&mirrored, 3.0, 5.0) && !at(&mirrored, 7.0, 5.0));
        let reset = render(
            "%LR45*%\n%LR0*%\n%LS1*%\n%LMN*%\n%ADD10R,2.0X1.0*%\nD10*\nX50000Y50000D03*",
            10.0,
        );
        assert_eq!(reset.lit(), 200);
    }

    #[test]
    fn trailing_zero_omission_and_inches() {
        let src = "%FSTAX13Y13*%\n%MOMM*%\n%ADD10R,2.0X2.0*%\nD10*\nX5Y5D03*\nM02*";
        let img = render_gerber(src, &view10(), 10.0).unwrap();
        assert_eq!(img.lit(), 400);
        assert!(at(&img, 5.0, 5.0));
        // 0.1 inch is 2.54 mm; 0.2 inch is 5.08 mm.
        let inch = "%FSLAX24Y24*%\n%MOIN*%\n%ADD10C,0.1*%\nD10*\nX2000Y2000D03*\nM02*";
        let img = render_gerber(inch, &view10(), 10.0).unwrap();
        near(img.lit(), PI * 1.27 * 1.27 * 100.0, 6.0);
        assert!(at(&img, 5.08, 5.08) && !at(&img, 5.08 + 1.4, 5.08));
        let ext = gerber_extent(inch).unwrap();
        assert!(
            (ext.min.0 - (5.08 - 1.27)).abs() < 1e-3 && (ext.max.1 - (5.08 + 1.27)).abs() < 1e-3
        );
    }

    #[test]
    fn decimal_coordinates_and_deprecated_codes() {
        let src = "%FSLAX24Y24*%\n%MOMM*%\n%ADD10R,2.0X2.0*%\nG54D10*\nX5.0Y5.0D03*\nM02*";
        assert_eq!(render_gerber(src, &view10(), 10.0).unwrap().lit(), 400);
        let inch = "%FSLAX24Y24*%\nG70*\n%ADD10R,0.1X0.1*%\nD10*\nX1000Y1000D03*\nM02*";
        near(
            render_gerber(inch, &view10(), 10.0).unwrap().lit(),
            645.0,
            40.0,
        );
    }

    #[test]
    fn extent_includes_apertures_and_arcs() {
        let src = format!(
            "{HEAD}%ADD10C,1.0*%\n%ADD11R,2.0X4.0*%\nD10*\nX20000Y30000D02*\nX20000Y60000D01*\nD11*\nX70000Y10000D03*\nM02*"
        );
        let v = gerber_extent(&src).unwrap();
        assert!(
            (v.min.0 - 1.5).abs() < 1e-3 && (v.min.1 - -1.0).abs() < 1e-3,
            "{v:?}"
        );
        assert!(
            (v.max.0 - 8.0).abs() < 1e-3 && (v.max.1 - 6.5).abs() < 1e-3,
            "{v:?}"
        );
        let arc = format!(
            "{HEAD}%ADD10C,0.2*%\nD10*\nG75*\nX20000Y50000D02*\nG02*\nX80000Y50000I30000J0D01*\nM02*"
        );
        let v = gerber_extent(&arc).unwrap();
        assert!(
            (v.max.1 - 8.1).abs() < 0.01 && (v.min.0 - 1.9).abs() < 1e-3,
            "{v:?}"
        );
        assert!(gerber_extent(&format!("{HEAD}M02*")).is_err());
    }

    #[test]
    fn attributes_and_comments_are_ignored() {
        let src = "%TF.FileFunction,Copper,L1,Top*%\n%TF.FilePolarity,Positive*%\nG04 a comment with D03 and X1Y1*\n%FSLAX24Y24*%\n%MOMM*%\n%LPD*%\nG01*\n%TA.AperFunction,SMDPad,CuDef*%\n%ADD10R,2.0X2.0*%\n%TD*%\nD10*\n%TO.P,R1,1*%\nX50000Y50000D03*\n%TD*%\nM02*\n";
        assert_eq!(render_gerber(src, &view10(), 10.0).unwrap().lit(), 400);
    }

    #[test]
    fn unsupported_commands_name_the_command_and_line() {
        let cases = [
            ("%SRX2Y2I1J1*%\n", "%SR", 4),
            ("G99*\n", "G99", 4),
            ("%ABD12*%\n", "%AB", 4),
            (
                "%AMBAD*\n6,0,0,1,0.1,0.1,2,0.1,0.1,0*%\n%ADD10BAD*%\n",
                "primitive code 6",
                5,
            ),
            ("%IPNEG*%\n", "%IPNEG", 4),
            ("M05*\n", "M05", 4),
            ("N12*\n", "N12", 4),
        ];
        for (body, name, line) in cases {
            let src = format!("{HEAD}G01*\n{body}M02*\n");
            let e = render_gerber(&src, &view10(), 10.0).unwrap_err();
            assert!(e.message.contains(name), "{e}");
            assert_eq!(e.line, line, "{e}");
            assert!(e.to_string().starts_with(&format!("line {line}: ")));
        }
        // The line is counted through multi-line macros and comments.
        let src = format!("{HEAD}%AMA*\n1,1,1,0,0*%\nG04 x*\nG99*\n");
        assert_eq!(render_gerber(&src, &view10(), 10.0).unwrap_err().line, 6);
    }

    #[test]
    fn other_errors() {
        let e = |src: &str| render_gerber(src, &view10(), 10.0).unwrap_err().to_string();
        assert!(e("%FSLAX24Y24*%\nX1Y1D03*\n").contains("units"));
        assert!(e(&format!("{HEAD}X1Y1D03*\n")).contains("aperture"));
        assert!(e(&format!("{HEAD}D10*\n")).contains("not defined"));
        assert!(e(&format!("{HEAD}%ADD10C,1*%\nD10*\nG36*\nX1Y1D03*\n")).contains("region"));
        assert!(e(&format!("{HEAD}G36*\nX1Y1D02*\n")).contains("G37"));
        assert!(e(&format!("{HEAD}G37*\n")).contains("G36"));
        assert!(e(&format!("{HEAD}%ADD10NOPE,1*%\n")).contains("undefined macro"));
        assert!(
            e(&format!("{HEAD}%AMM*\n1,1,$1/0,0,0*%\n%ADD10M,1*%\n")).contains("division by zero")
        );
        assert!(e(&format!("{HEAD}%AMM*\n1,1,(1+,0,0*%\n%ADD10M,1*%\n")).contains("expression"));
        assert!(
            e(&format!(
                "{HEAD}%AMM*\n1,1,1,0,0*%\n%ADD10M*%\nD10*\nX1Y1D02*\nX2Y2D01*\n"
            ))
            .contains("macro aperture")
        );
        assert!(e("%FSLIX24Y24*%\n%MOMM*%\n").contains("incremental"));
        assert!(e(&format!("{HEAD}G04 never closed")).contains("closing"));
        let bad_view = View {
            min: (0.0, 0.0),
            max: (0.0, 5.0),
        };
        assert!(render_gerber("", &bad_view, 10.0).is_err());
        assert!(render_gerber("", &view10(), 0.0).is_err());
        assert!(
            render_gerber(
                "",
                &View {
                    min: (0.0, 0.0),
                    max: (1e6, 1e6)
                },
                10.0
            )
            .is_err()
        );
    }

    #[test]
    fn things_outside_the_view_are_clipped() {
        let src = "%ADD10C,4.0*%\nD10*\nX0Y0D03*\nX200000Y200000D03*\nX50000Y-200000D03*";
        let img = render(src, 10.0);
        near(img.lit(), PI * 4.0 * 100.0 / 4.0, 8.0);
    }

    #[test]
    fn m02_stops_reading() {
        let src = "%ADD10R,2.0X2.0*%\nD10*\nX50000Y50000D03*\nM02*\nX20000Y20000D03*\nG99*";
        assert_eq!(render(src, 10.0).lit(), 400);
    }

    #[test]
    fn rendering_speed_of_a_board_sized_image() {
        // 100 x 100 mm at 20 px/mm (format 2.4: 10000 is 1 mm): a pour with a grid of
        // holes, a row of pads and 400 tracks.
        let mut body = String::from(
            "%ADD10C,0.25*%\n%ADD11R,1.5X1.5*%\n%ADD12C,1.0*%\nG36*\nX10000Y10000D02*\nG01*\nX990000Y10000D01*\nX990000Y990000D01*\nX10000Y990000D01*\nG37*\n%LPC*%\nD12*\n",
        );
        for i in 0..40 {
            for j in 0..40 {
                body.push_str(&format!(
                    "X{}Y{}D03*\n",
                    15_000 + i * 23_000,
                    15_000 + j * 23_000
                ));
            }
        }
        body.push_str("%LPD*%\nD11*\n");
        for i in 0..40 {
            body.push_str(&format!("X{}Y15000D03*\n", 15_000 + i * 23_000));
        }
        body.push_str("D10*\n");
        for i in 0..400 {
            let x = 10_000 + i * 2_400;
            body.push_str(&format!("X{x}Y10000D02*\nX{x}Y980000D01*\n"));
        }
        let src = format!("{HEAD}{body}M02*\n");
        let view = View {
            min: (0.0, 0.0),
            max: (100.0, 100.0),
        };
        let t = std::time::Instant::now();
        let img = render_gerber(&src, &view, 20.0).unwrap();
        eprintln!(
            "100 x 100 mm at 20 px/mm: {:?}, {} lit pixels",
            t.elapsed(),
            img.lit()
        );
        assert!(img.lit() > 3_000_000);
        assert!(at(&img, 50.0, 50.0) && !at(&img, 0.3, 0.3));
    }
}
