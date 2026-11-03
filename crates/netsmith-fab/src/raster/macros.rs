//! Aperture macros (`%AM`): expressions with variables, and the primitives they draw.

use super::RasterError;
use super::geom::{Prim, Pt, Rule, Tess, disc, ngon, rotate};
use std::collections::HashMap;

/// The body of a macro: its blocks with the line each began on.
#[derive(Clone, Debug)]
pub(crate) struct MacroDef {
    pub blocks: Vec<(usize, String)>,
}

struct Eval<'a> {
    s: &'a [u8],
    i: usize,
    vars: &'a HashMap<u32, f64>,
    line: usize,
}

impl Eval<'_> {
    fn err(&self, what: &str) -> RasterError {
        RasterError::new(
            self.line,
            format!(
                "bad macro expression `{}`: {what}",
                String::from_utf8_lossy(self.s)
            ),
        )
    }

    fn skip(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip();
        self.s.get(self.i).copied()
    }

    /// `term (('+' | '-') term)*`
    fn expr(&mut self) -> Result<f64, RasterError> {
        let mut v = self.term()?;
        while let Some(c) = self.peek() {
            match c {
                b'+' => {
                    self.i += 1;
                    v += self.term()?;
                }
                b'-' => {
                    self.i += 1;
                    v -= self.term()?;
                }
                _ => break,
            }
        }
        Ok(v)
    }

    /// `factor (('x' | 'X' | '/') factor)*`
    fn term(&mut self) -> Result<f64, RasterError> {
        let mut v = self.factor()?;
        while let Some(c) = self.peek() {
            match c {
                b'x' | b'X' => {
                    self.i += 1;
                    v *= self.factor()?;
                }
                b'/' => {
                    self.i += 1;
                    let d = self.factor()?;
                    if d == 0.0 {
                        return Err(self.err("division by zero"));
                    }
                    v /= d;
                }
                _ => break,
            }
        }
        Ok(v)
    }

    fn factor(&mut self) -> Result<f64, RasterError> {
        match self.peek() {
            Some(b'+') => {
                self.i += 1;
                self.factor()
            }
            Some(b'-') => {
                self.i += 1;
                Ok(-self.factor()?)
            }
            Some(b'(') => {
                self.i += 1;
                let v = self.expr()?;
                if self.peek() != Some(b')') {
                    return Err(self.err("missing `)`"));
                }
                self.i += 1;
                Ok(v)
            }
            Some(b'$') => {
                self.i += 1;
                let start = self.i;
                while self.s.get(self.i).is_some_and(u8::is_ascii_digit) {
                    self.i += 1;
                }
                let n: u32 = std::str::from_utf8(&self.s[start..self.i])
                    .ok()
                    .and_then(|t| t.parse().ok())
                    .ok_or_else(|| self.err("`$` without a number"))?;
                Ok(self.vars.get(&n).copied().unwrap_or(0.0))
            }
            Some(c) if c.is_ascii_digit() || c == b'.' => {
                let start = self.i;
                while self
                    .s
                    .get(self.i)
                    .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
                {
                    self.i += 1;
                }
                std::str::from_utf8(&self.s[start..self.i])
                    .ok()
                    .and_then(|t| t.parse().ok())
                    .ok_or_else(|| self.err("bad number"))
            }
            _ => Err(self.err("expected a number, a variable or `(`")),
        }
    }
}

/// Evaluates `text` with the variables `vars`.
fn eval(text: &str, vars: &HashMap<u32, f64>, line: usize) -> Result<f64, RasterError> {
    let mut e = Eval {
        s: text.as_bytes(),
        i: 0,
        vars,
        line,
    };
    let v = e.expr()?;
    if e.peek().is_some() {
        return Err(e.err("unexpected characters after the expression"));
    }
    if !v.is_finite() {
        return Err(e.err("not a finite number"));
    }
    Ok(v)
}

/// A rectangle with its sides along `u` from `p1` to `p2`, `w` wide, counter-clockwise.
fn bar(p1: Pt, p2: Pt, w: f64) -> Vec<Pt> {
    let (dx, dy) = (p2.0 - p1.0, p2.1 - p1.1);
    let len = dx.hypot(dy);
    let (ux, uy) = if len == 0.0 {
        (1.0, 0.0)
    } else {
        (dx / len, dy / len)
    };
    let (nx, ny) = (-uy * w / 2.0, ux * w / 2.0);
    vec![
        (p1.0 - nx, p1.1 - ny),
        (p2.0 - nx, p2.1 - ny),
        (p2.0 + nx, p2.1 + ny),
        (p1.0 + nx, p1.1 + ny),
    ]
}

/// The four ring pieces of a thermal primitive, centred on the origin, before rotation.
fn thermal(outer: f64, inner: f64, gap: f64, tess: Tess, unit: f64) -> Vec<Vec<Pt>> {
    let (ro, ri, g) = (outer / 2.0, (inner / 2.0).max(0.0), gap / 2.0);
    let mut pieces = Vec::new();
    if ro <= g || ro <= ri {
        return pieces;
    }
    // First quadrant: x >= g, y >= g, between the two circles.
    let (first, last) = ((g / ro).asin(), (g / ro).acos());
    let sweep = last - first;
    let n = tess.arc_n(ro * unit, sweep);
    let mut quadrant: Vec<Pt> = (0..=n)
        .map(|k| {
            let a = first + sweep * k as f64 / n as f64;
            (ro * a.cos(), ro * a.sin())
        })
        .collect();
    if ri > g {
        let (lo, hi) = ((g / ri).asin(), (g / ri).acos());
        let m = tess.arc_n(ri * unit, hi - lo);
        for k in 0..=m {
            let a = hi - (hi - lo) * k as f64 / m as f64;
            quadrant.push((ri * a.cos(), ri * a.sin()));
        }
    } else {
        quadrant.push((g, g));
    }
    for k in 0..4 {
        pieces.push(
            quadrant
                .iter()
                .map(|&p| rotate(p, 90.0 * f64::from(k)))
                .collect(),
        );
    }
    pieces
}

/// Evaluates the macro `def` for one aperture whose parameters are `params` (`$1`, `$2`,
/// ...), in file units, and returns its primitives in millimetres (`unit` is the number of
/// millimetres in a file unit).
pub(crate) fn instantiate(
    def: &MacroDef,
    params: &[f64],
    unit: f64,
    tess: Tess,
) -> Result<Vec<Prim>, RasterError> {
    let mut vars: HashMap<u32, f64> = params
        .iter()
        .enumerate()
        .map(|(i, p)| (i as u32 + 1, *p))
        .collect();
    let mut out = Vec::new();
    for (line, block) in &def.blocks {
        let line = *line;
        let b = block.trim();
        if b.is_empty() {
            continue;
        }
        // Code 0 is a comment, and its text may contain anything.
        if b.starts_with('0') && !b[1..].starts_with(|c: char| c.is_ascii_digit() || c == '.') {
            continue;
        }
        if let Some(rest) = b.strip_prefix('$') {
            let (lhs, rhs) = rest
                .split_once('=')
                .ok_or_else(|| RasterError::new(line, format!("macro block `{b}` has no `=`")))?;
            let n: u32 = lhs.trim().parse().map_err(|_| {
                RasterError::new(line, format!("bad variable `${}` in macro", lhs.trim()))
            })?;
            let v = eval(rhs, &vars, line)?;
            vars.insert(n, v);
            continue;
        }
        let (code_text, rest) = b.split_once(',').unwrap_or((b, ""));
        let code: u32 = code_text
            .trim()
            .parse()
            .map_err(|_| RasterError::new(line, format!("unsupported macro statement `{b}`")))?;
        let p = rest
            .split(',')
            .filter(|t| !t.trim().is_empty())
            .map(|t| eval(t, &vars, line))
            .collect::<Result<Vec<f64>, _>>()?;
        let need = |n: usize| -> Result<(), RasterError> {
            if p.len() < n {
                Err(RasterError::new(
                    line,
                    format!(
                        "macro primitive {code} needs {n} parameters, `{b}` has {}",
                        p.len()
                    ),
                ))
            } else {
                Ok(())
            }
        };
        let at = |i: usize| p.get(i).copied().unwrap_or(0.0);
        let on = at(0) != 0.0;
        let polys: Vec<Vec<Pt>> = match code {
            1 => {
                need(4)?;
                let c = rotate((at(2), at(3)), at(4));
                let r = at(1) / 2.0;
                if r <= 0.0 {
                    continue;
                }
                vec![disc(c, r, tess.circle_n(r * unit))]
            }
            2 | 20 => {
                need(6)?;
                let poly = bar((at(2), at(3)), (at(4), at(5)), at(1));
                vec![poly.into_iter().map(|q| rotate(q, at(6))).collect()]
            }
            21 => {
                need(5)?;
                let (w, h, cx, cy) = (at(1), at(2), at(3), at(4));
                let poly = bar((cx - w / 2.0, cy), (cx + w / 2.0, cy), h);
                vec![poly.into_iter().map(|q| rotate(q, at(5))).collect()]
            }
            22 => {
                need(5)?;
                let (w, h, x, y) = (at(1), at(2), at(3), at(4));
                let poly = bar((x, y + h / 2.0), (x + w, y + h / 2.0), h);
                vec![poly.into_iter().map(|q| rotate(q, at(5))).collect()]
            }
            4 => {
                need(3)?;
                let n = at(1);
                if n < 2.0 {
                    return Err(RasterError::new(
                        line,
                        format!("macro outline `{b}` needs at least 3 vertices"),
                    ));
                }
                let n = n as usize;
                need(4 + 2 * n)?;
                let pts = (0..=n)
                    .map(|k| rotate((at(2 + 2 * k), at(3 + 2 * k)), at(4 + 2 * n)))
                    .collect();
                vec![pts]
            }
            5 => {
                need(5)?;
                let n = at(1) as usize;
                if !(3..=1000).contains(&n) {
                    return Err(RasterError::new(
                        line,
                        format!("macro polygon `{b}` needs 3 to 1000 vertices"),
                    ));
                }
                let r = at(4) / 2.0;
                let pts = ngon((at(2), at(3)), r, n, 0.0);
                vec![pts.into_iter().map(|q| rotate(q, at(5))).collect()]
            }
            7 => {
                need(5)?;
                let c = (at(0), at(1));
                thermal(at(2), at(3), at(4), tess, unit)
                    .into_iter()
                    .map(|piece| {
                        piece
                            .into_iter()
                            .map(|q| rotate((q.0 + c.0, q.1 + c.1), at(5)))
                            .collect()
                    })
                    .collect()
            }
            _ => {
                return Err(RasterError::new(
                    line,
                    format!("unsupported macro primitive code {code} in `{b}`"),
                ));
            }
        };
        let scaled = polys
            .into_iter()
            .map(|poly| poly.into_iter().map(|q| (q.0 * unit, q.1 * unit)).collect())
            .collect();
        // Primitive 7 has no exposure parameter: it is always on.
        let on = code == 7 || on;
        out.push(Prim {
            polys: scaled,
            rule: Rule::NonZero,
            on,
        });
    }
    Ok(out)
}
