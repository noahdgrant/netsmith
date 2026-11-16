//! A logo's vector file as outlines to fill: an SVG's shapes or a DXF's closed loops
//! (`std::parts::mechanical::Logo`).
//!
//! [`read_logo`] returns the outlines in millimetres, scaled so that the longer side of their
//! box is the size asked for, with the box's centre at the origin, x to the right and y
//! downward (the board's frame; a DXF has y up, so it is turned over).
//!
//! **SVG.** The shapes of `path`, `polygon`, `rect`, `circle` and `ellipse` elements are read, as
//! filled shapes. A path's curves (`C`, `S`, `Q`, `T`, `A`) are cut into short straight pieces,
//! fine enough that the cut shows nowhere on a mark of a few millimetres. What is refused, with
//! the element's name and place in the message: a `transform` (on the element or on a group that
//! holds it: apply it in the drawing program), a shape that has no fill (`fill="none"`, a
//! stroke is not drawn: convert the stroke to a path), and an element that draws something the
//! reader does not read (`text`, `image`, `use`, `line`, `polyline`, `foreignObject`: convert text
//! to paths, and a line to a shape). Units, colours, opacity and the fill rule are not read: the
//! drawing is scaled to its size, every shape is filled, and a shape inside another is a hole in
//! it (the even-odd rule). A shape that overlaps another without lying inside it is plotted
//! as it is, so two overlapping filled shapes are filled where they overlap, too. Anything in
//! `defs`, `clipPath`, `mask`, `symbol`, `pattern` and `marker`, and any element that is
//! `display:none`, is not drawn and not read.
//!
//! **DXF.** The closed loops that its lines, arcs, circles, splines and polylines make, on every
//! layer, as [`crate::read_outline`] reads a board's (the same tolerances: ends within 0.001 mm
//! are one point). A file with no `$INSUNITS` is taken to be in millimetres; it does not matter,
//! as the drawing is scaled. A loop with a gap is an error that names the gap.
//!
//! **A hole.** A shape inside another is a hole. Neither a KiCad `fp_poly` nor a Gerber region
//! holds one, so the hole is joined to the outline round it by a cut of no width (two
//! overlapping edges), as KiCad's own import of a drawing does, and the result is one polygon.

use crate::geom::P;

/// How many straight pieces a cubic or quadratic Bézier is cut into: a chord of a curve of
/// a logo's mark is far under a hundredth of its length from the curve.
const CURVE_PIECES: usize = 32;

/// Read the file `name` (its extension says SVG or DXF) whose text is `text`, as outlines scaled
/// to `size`. An error is a sentence that names the file.
pub fn read_logo(name: &str, text: &str, size: f64) -> Result<Vec<Vec<(f64, f64)>>, String> {
    let lower = name.to_ascii_lowercase();
    let loops = if lower.ends_with(".svg") {
        svg_loops(text).map_err(|e| format!("`{name}`: {e}"))?
    } else if lower.ends_with(".dxf") {
        dxf_loops(text).map_err(|e| format!("`{name}`: {e}"))?
    } else {
        return Err(format!(
            "`{name}` is neither an SVG nor a DXF file (it should end in `.svg` or `.dxf`): a logo is drawn from a vector file, and a raster image is not read"
        ));
    };
    let loops: Vec<Vec<P>> = loops
        .into_iter()
        .map(dedup)
        .filter(|l| l.len() >= 3 && area(l).abs() > 0.0)
        .collect();
    if loops.is_empty() {
        return Err(format!(
            "`{name}` has no closed shape to fill: draw the mark as filled shapes (a closed path, a polygon, a rectangle or a circle)"
        ));
    }
    let nested = nest(loops);
    let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for &(x, y) in nested.iter().flatten() {
        lo = (lo.0.min(x), lo.1.min(y));
        hi = (hi.0.max(x), hi.1.max(y));
    }
    let longer = (hi.0 - lo.0).max(hi.1 - lo.1);
    if !longer.is_finite() || longer <= 0.0 {
        return Err(format!("`{name}` draws a mark with no extent"));
    }
    let (scale, centre) = (size / longer, ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0));
    Ok(nested
        .into_iter()
        .map(|l| {
            l.into_iter()
                .map(|(x, y)| ((x - centre.0) * scale, (y - centre.1) * scale))
                .collect()
        })
        .collect())
}

fn dedup(mut l: Vec<P>) -> Vec<P> {
    l.dedup();
    while l.len() > 1 && l.first() == l.last() {
        l.pop();
    }
    l
}

/// The signed area of a ring (the shoelace sum).
fn area(l: &[P]) -> f64 {
    let n = l.len();
    (0..n)
        .map(|i| {
            let (a, b) = (l[i], l[(i + 1) % n]);
            (a.0 * b.1 - b.0 * a.1) / 2.0
        })
        .sum()
}

fn inside(p: P, ring: &[P]) -> bool {
    crate::geom::inside(p, ring)
}

/// The loops as outlines with their holes joined in: a loop inside an odd number of others is a
/// hole of the smallest loop round it.
fn nest(loops: Vec<Vec<P>>) -> Vec<Vec<P>> {
    let n = loops.len();
    // The loops that hold each loop's first point that is not on another loop's edge: its first
    // point is enough for shapes that nest or stand apart.
    let holds = |i: usize| -> Vec<usize> {
        (0..n)
            .filter(|&j| j != i && inside(loops[i][0], &loops[j]))
            .collect()
    };
    let around: Vec<Vec<usize>> = (0..n).map(holds).collect();
    let mut holes_of: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut outers = Vec::new();
    for i in 0..n {
        if around[i].len().is_multiple_of(2) {
            outers.push(i);
        } else if let Some(&parent) = around[i]
            .iter()
            .filter(|&&j| around[j].len().is_multiple_of(2))
            .min_by(|&&a, &&b| {
                area(&loops[a])
                    .abs()
                    .partial_cmp(&area(&loops[b]).abs())
                    .expect("finite")
            })
        {
            holes_of[parent].push(i);
        }
    }
    outers
        .into_iter()
        .map(|o| {
            let mut ring = loops[o].clone();
            let sign = area(&ring).signum();
            let mut holes: Vec<Vec<P>> = holes_of[o].iter().map(|&h| loops[h].clone()).collect();
            // Every ring of the shape, to keep a cut from crossing one.
            for hole in &mut holes {
                // A hole runs the other way round, so that the rule that fills by winding leaves
                // it empty too.
                if area(hole).signum() == sign {
                    hole.reverse();
                }
            }
            while !holes.is_empty() {
                // The hole nearest the ring first, cut in by the shortest cut that crosses no
                // edge.
                let mut best: Option<(f64, usize, usize, usize)> = None;
                for (h, hole) in holes.iter().enumerate() {
                    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
                    for (j, hp) in hole.iter().enumerate() {
                        for (i, rp) in ring.iter().enumerate() {
                            pairs.push(((hp.0 - rp.0).hypot(hp.1 - rp.1), j, i));
                        }
                    }
                    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("finite"));
                    for &(d, j, i) in pairs.iter().take(64) {
                        let clear = crosses_none(ring[i], hole[j], &ring, Some(i))
                            && holes.iter().enumerate().all(|(k, other)| {
                                crosses_none(ring[i], hole[j], other, (k == h).then_some(j))
                            });
                        if clear {
                            if best.is_none_or(|b| d < b.0) {
                                best = Some((d, h, j, i));
                            }
                            break;
                        }
                    }
                }
                let (_, h, j, i) = best.unwrap_or_else(|| {
                    // No cut is clear: take the nearest pair, which is still a polygon that
                    // draws the hole.
                    let hole = &holes[0];
                    (0.0, 0, 0, nearest(&ring, hole[0]))
                });
                let hole = holes.remove(h);
                let mut joined = Vec::with_capacity(ring.len() + hole.len() + 2);
                joined.extend_from_slice(&ring[..=i]);
                for k in 0..=hole.len() {
                    joined.push(hole[(j + k) % hole.len()]);
                }
                joined.push(ring[i]);
                joined.extend_from_slice(&ring[i + 1..]);
                ring = joined;
            }
            ring
        })
        .collect()
}

fn nearest(ring: &[P], p: P) -> usize {
    (0..ring.len())
        .min_by(|&a, &b| {
            let d = |q: P| (q.0 - p.0).hypot(q.1 - p.1);
            d(ring[a]).partial_cmp(&d(ring[b])).expect("finite")
        })
        .unwrap_or(0)
}

/// Whether the cut `a`-`b` crosses no edge of `ring` (the edges at `skip`, the ring's vertex the
/// cut starts from, are left out; touching at an end is not crossing).
fn crosses_none(a: P, b: P, ring: &[P], skip: Option<usize>) -> bool {
    let n = ring.len();
    (0..n).all(|k| {
        if skip.is_some_and(|s| k == s || (k + 1) % n == s) {
            return true;
        }
        !properly_cross(a, b, ring[k], ring[(k + 1) % n])
    })
}

fn properly_cross(a: P, b: P, c: P, d: P) -> bool {
    let cross = |o: P, p: P, q: P| (p.0 - o.0) * (q.1 - o.1) - (p.1 - o.1) * (q.0 - o.0);
    let (d1, d2, d3, d4) = (
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    );
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

// ---------------------------------------------------------------------------------------------
// DXF

fn dxf_loops(text: &str) -> Result<Vec<Vec<P>>, String> {
    crate::read_loops(text).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------------
// SVG

struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
    /// `<name`, `</name`, or `<name ... />`.
    kind: TagKind,
    /// The line it starts on.
    line: usize,
}

#[derive(PartialEq, Clone, Copy)]
enum TagKind {
    Open,
    Close,
    Empty,
}

impl Tag {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// A presentation property, from its attribute or the `style` attribute.
    fn property(&self, name: &str) -> Option<String> {
        if let Some(style) = self.attr("style") {
            for decl in style.split(';') {
                if let Some((k, v)) = decl.split_once(':')
                    && k.trim() == name
                {
                    return Some(v.trim().to_string());
                }
            }
        }
        self.attr(name).map(|v| v.trim().to_string())
    }

    fn place(&self) -> String {
        format!("`<{}>` at line {}", self.name, self.line)
    }
}

fn tags(text: &str) -> Result<Vec<Tag>, String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let mut counted = 0;
    while let Some(off) = text[i..].find('<') {
        i += off;
        line += text[counted..i].matches('\n').count();
        counted = i;
        let rest = &text[i..];
        let skip = |end: &str| -> Result<usize, String> {
            rest.find(end)
                .map(|p| p + end.len())
                .ok_or_else(|| format!("line {line}: a `{}` is never closed", &rest[..2]))
        };
        if rest.starts_with("<!--") {
            i += skip("-->")?;
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            i += skip("]]>")?;
            continue;
        }
        if rest.starts_with("<?") {
            i += skip("?>")?;
            continue;
        }
        if rest.starts_with("<!") {
            i += skip(">")?;
            continue;
        }
        let mut j = i + 1;
        let closing = b.get(j) == Some(&b'/');
        if closing {
            j += 1;
        }
        let start = j;
        while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'/' && b[j] != b'>' {
            j += 1;
        }
        let name = text[start..j].to_string();
        let mut attrs = Vec::new();
        let mut empty = false;
        loop {
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            match b.get(j) {
                None => return Err(format!("line {line}: the tag `<{name}` is never closed")),
                Some(b'>') => {
                    j += 1;
                    break;
                }
                Some(b'/') => {
                    empty = true;
                    j += 1;
                }
                Some(_) => {
                    let start = j;
                    while j < b.len()
                        && b[j] != b'='
                        && !b[j].is_ascii_whitespace()
                        && b[j] != b'>'
                        && b[j] != b'/'
                    {
                        j += 1;
                    }
                    let key = text[start..j].to_string();
                    while j < b.len() && b[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if b.get(j) != Some(&b'=') {
                        attrs.push((key, String::new()));
                        continue;
                    }
                    j += 1;
                    while j < b.len() && b[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    let Some(&quote) = b.get(j).filter(|q| **q == b'"' || **q == b'\'') else {
                        return Err(format!(
                            "line {line}: the attribute `{key}` of `<{name}>` has a value with no quotes"
                        ));
                    };
                    j += 1;
                    let vstart = j;
                    while j < b.len() && b[j] != quote {
                        j += 1;
                    }
                    if j >= b.len() {
                        return Err(format!(
                            "line {line}: the attribute `{key}` of `<{name}>` is never closed"
                        ));
                    }
                    attrs.push((key, text[vstart..j].to_string()));
                    j += 1;
                }
            }
        }
        out.push(Tag {
            name,
            attrs,
            kind: if closing {
                TagKind::Close
            } else if empty {
                TagKind::Empty
            } else {
                TagKind::Open
            },
            line,
        });
        i = j;
    }
    Ok(out)
}

/// Elements whose content is not drawn.
const NOT_DRAWN: [&str; 6] = ["defs", "clipPath", "mask", "symbol", "pattern", "marker"];
/// Elements that draw something this reader does not read.
const UNREAD: [&str; 7] = [
    "text",
    "image",
    "use",
    "line",
    "polyline",
    "foreignObject",
    "switch",
];

fn svg_loops(text: &str) -> Result<Vec<Vec<P>>, String> {
    let tags = tags(text)?;
    if !tags.iter().any(|t| t.name == "svg") {
        return Err("it is not an SVG file (no `<svg>` element)".to_string());
    }
    let mut loops: Vec<Vec<P>> = Vec::new();
    // The open elements: their names, and whether each hides what is in it.
    let mut stack: Vec<(String, bool)> = Vec::new();
    for tag in &tags {
        if tag.kind == TagKind::Close {
            while let Some((name, _)) = stack.pop() {
                if name == tag.name {
                    break;
                }
            }
            continue;
        }
        let hidden_by_parent = stack.iter().any(|(_, h)| *h);
        let hides = NOT_DRAWN.contains(&tag.name.as_str())
            || tag.property("display").as_deref() == Some("none");
        let skipped = hidden_by_parent || hides;
        if tag.kind == TagKind::Open {
            stack.push((tag.name.clone(), hides || hidden_by_parent));
        }
        if skipped || tag.name.contains(':') {
            continue;
        }
        if tag.name != "svg" && tag.attr("transform").is_some() {
            return Err(format!(
                "{} has a `transform`, which is not read: apply it in the drawing program (Inkscape: Object, Apply Transform; or ungroup and move) and save a plain SVG",
                tag.place()
            ));
        }
        if UNREAD.contains(&tag.name.as_str()) {
            return Err(format!(
                "{} draws something that is not read: {}",
                tag.place(),
                match tag.name.as_str() {
                    "text" => "convert the text to paths",
                    "image" => "a raster image is not drawn; trace it to a vector shape",
                    "line" | "polyline" =>
                        "a line is a stroke, and a stroke is not drawn: convert it to a closed shape",
                    "use" => "unlink the clone so that the shape is in the file itself",
                    _ => "draw the mark as filled shapes",
                }
            ));
        }
        if !matches!(
            tag.name.as_str(),
            "path" | "polygon" | "rect" | "circle" | "ellipse"
        ) {
            continue;
        }
        if tag.property("fill").as_deref() == Some("none") {
            return Err(format!(
                "{} has no fill, so it is a stroke, and a stroke is not drawn: convert the stroke to a path (Inkscape: Path, Stroke to Path)",
                tag.place()
            ));
        }
        let mut path = match tag.name.as_str() {
            "path" => {
                let d = tag
                    .attr("d")
                    .ok_or_else(|| format!("{} has no `d`", tag.place()))?;
                path_loops(d).map_err(|e| format!("{}: {e}", tag.place()))?
            }
            "polygon" => {
                let points = tag
                    .attr("points")
                    .ok_or_else(|| format!("{} has no `points`", tag.place()))?;
                let nums = numbers(points).map_err(|e| format!("{}: {e}", tag.place()))?;
                if nums.len() % 2 != 0 {
                    return Err(format!(
                        "{} has {} numbers in `points`, an odd count",
                        tag.place(),
                        nums.len()
                    ));
                }
                vec![nums.chunks(2).map(|c| (c[0], c[1])).collect()]
            }
            "rect" => {
                let n = |k: &str| -> Result<f64, String> {
                    match tag.attr(k) {
                        Some(v) => length(v).map_err(|e| format!("{} `{k}`: {e}", tag.place())),
                        None => Ok(0.0),
                    }
                };
                let (x, y, w, h) = (n("x")?, n("y")?, n("width")?, n("height")?);
                let (mut rx, mut ry) = (n("rx")?, n("ry")?);
                if tag.attr("rx").is_some() && tag.attr("ry").is_none() {
                    ry = rx;
                } else if tag.attr("ry").is_some() && tag.attr("rx").is_none() {
                    rx = ry;
                }
                rx = rx.min(w / 2.0);
                ry = ry.min(h / 2.0);
                if rx > 0.0 && ry > 0.0 {
                    let d = format!(
                        "M{} {} H{} A{rx} {ry} 0 0 1 {} {} V{} A{rx} {ry} 0 0 1 {} {} H{} A{rx} {ry} 0 0 1 {} {} V{} A{rx} {ry} 0 0 1 {} {}Z",
                        x + rx,
                        y,
                        x + w - rx,
                        x + w,
                        y + ry,
                        y + h - ry,
                        x + w - rx,
                        y + h,
                        x + rx,
                        x,
                        y + h - ry,
                        y + ry,
                        x + rx,
                        y
                    );
                    path_loops(&d).map_err(|e| format!("{}: {e}", tag.place()))?
                } else {
                    vec![vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]]
                }
            }
            _ => {
                let n = |k: &str| -> Result<f64, String> {
                    match tag.attr(k) {
                        Some(v) => length(v).map_err(|e| format!("{} `{k}`: {e}", tag.place())),
                        None => Ok(0.0),
                    }
                };
                let (cx, cy) = (n("cx")?, n("cy")?);
                let (rx, ry) = if tag.name == "circle" {
                    (n("r")?, n("r")?)
                } else {
                    (n("rx")?, n("ry")?)
                };
                if rx <= 0.0 || ry <= 0.0 {
                    continue;
                }
                let d = format!(
                    "M{} {cy} A{rx} {ry} 0 1 1 {} {cy} A{rx} {ry} 0 1 1 {} {cy}Z",
                    cx + rx,
                    cx - rx,
                    cx + rx
                );
                path_loops(&d).map_err(|e| format!("{}: {e}", tag.place()))?
            }
        };
        loops.append(&mut path);
    }
    Ok(loops)
}

fn length(v: &str) -> Result<f64, String> {
    let v = v.trim();
    let end = v
        .find(|c: char| c.is_ascii_alphabetic() && c != 'e' && c != 'E' || c == '%')
        .unwrap_or(v.len());
    v[..end]
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("`{v}` is not a number"))
}

fn numbers(s: &str) -> Result<Vec<f64>, String> {
    let mut sc = Scanner::new(s);
    let mut out = Vec::new();
    loop {
        sc.skip_separators();
        if sc.done() {
            return Ok(out);
        }
        out.push(sc.number()?);
    }
}

struct Scanner<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Scanner<'a> {
    fn new(s: &'a str) -> Self {
        Scanner {
            b: s.as_bytes(),
            i: 0,
        }
    }

    fn done(&self) -> bool {
        self.i >= self.b.len()
    }

    fn skip_separators(&mut self) {
        while self.i < self.b.len()
            && (self.b[self.i].is_ascii_whitespace() || self.b[self.i] == b',')
        {
            self.i += 1;
        }
    }

    fn number(&mut self) -> Result<f64, String> {
        self.skip_separators();
        let start = self.i;
        let digits = |sc: &mut Self| {
            let s = sc.i;
            while sc.i < sc.b.len() && sc.b[sc.i].is_ascii_digit() {
                sc.i += 1;
            }
            sc.i > s
        };
        if matches!(self.b.get(self.i), Some(b'+' | b'-')) {
            self.i += 1;
        }
        let whole = digits(self);
        let mut frac = false;
        if self.b.get(self.i) == Some(&b'.') {
            self.i += 1;
            frac = digits(self);
        }
        if !whole && !frac {
            return Err(format!(
                "expected a number at `{}`",
                String::from_utf8_lossy(&self.b[start..self.b.len().min(start + 12)])
            ));
        }
        if matches!(self.b.get(self.i), Some(b'e' | b'E'))
            && matches!(self.b.get(self.i + 1), Some(b'0'..=b'9' | b'+' | b'-'))
        {
            self.i += 2;
            digits(self);
        }
        std::str::from_utf8(&self.b[start..self.i])
            .expect("ascii")
            .parse::<f64>()
            .map_err(|_| "a number is out of range".to_string())
    }

    /// An arc's flag: one digit, 0 or 1, that may be written with no separator after it.
    fn flag(&mut self) -> Result<bool, String> {
        self.skip_separators();
        match self.b.get(self.i) {
            Some(b'0') => {
                self.i += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.i += 1;
                Ok(true)
            }
            _ => Err("an arc's flag is not 0 or 1".to_string()),
        }
    }
}

/// The subpaths of a path's `d`, each a ring of points.
fn path_loops(d: &str) -> Result<Vec<Vec<P>>, String> {
    let mut sc = Scanner::new(d);
    let mut loops: Vec<Vec<P>> = Vec::new();
    let mut ring: Vec<P> = Vec::new();
    let (mut cur, mut start) = ((0.0, 0.0), (0.0, 0.0));
    // The control point of the last curve, for `S` and `T`.
    let mut last_c: Option<P> = None;
    let mut last_q: Option<P> = None;
    let mut cmd = b' ';
    let mut first = true;
    loop {
        sc.skip_separators();
        if sc.done() {
            break;
        }
        let c = sc.b[sc.i];
        if c.is_ascii_alphabetic() {
            cmd = c;
            sc.i += 1;
            if first && !matches!(cmd, b'M' | b'm') {
                return Err("a path must start with `M`".to_string());
            }
            if matches!(cmd, b'Z' | b'z') {
                if !ring.is_empty() {
                    loops.push(std::mem::take(&mut ring));
                }
                cur = start;
                last_c = None;
                last_q = None;
                continue;
            }
        } else if cmd == b' ' {
            return Err("a path must start with `M`".to_string());
        } else if matches!(cmd, b'Z' | b'z') {
            return Err("numbers after `Z`".to_string());
        }
        first = false;
        let rel = cmd.is_ascii_lowercase();
        let off = |p: P, cur: P| if rel { (cur.0 + p.0, cur.1 + p.1) } else { p };
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let p = off((sc.number()?, sc.number()?), cur);
                if !ring.is_empty() {
                    loops.push(std::mem::take(&mut ring));
                }
                ring.push(p);
                cur = p;
                start = p;
                // Further pairs after an `M` are `L`.
                cmd = if rel { b'l' } else { b'L' };
                last_c = None;
                last_q = None;
            }
            b'L' => {
                let p = off((sc.number()?, sc.number()?), cur);
                ring.push(p);
                cur = p;
                last_c = None;
                last_q = None;
            }
            b'H' => {
                let x = sc.number()?;
                cur = (if rel { cur.0 + x } else { x }, cur.1);
                ring.push(cur);
                last_c = None;
                last_q = None;
            }
            b'V' => {
                let y = sc.number()?;
                cur = (cur.0, if rel { cur.1 + y } else { y });
                ring.push(cur);
                last_c = None;
                last_q = None;
            }
            b'C' => {
                let c1 = off((sc.number()?, sc.number()?), cur);
                let c2 = off((sc.number()?, sc.number()?), cur);
                let p = off((sc.number()?, sc.number()?), cur);
                cubic(&mut ring, cur, c1, c2, p);
                last_c = Some(c2);
                last_q = None;
                cur = p;
            }
            b'S' => {
                let c1 = last_c.map_or(cur, |c| (2.0 * cur.0 - c.0, 2.0 * cur.1 - c.1));
                let c2 = off((sc.number()?, sc.number()?), cur);
                let p = off((sc.number()?, sc.number()?), cur);
                cubic(&mut ring, cur, c1, c2, p);
                last_c = Some(c2);
                last_q = None;
                cur = p;
            }
            b'Q' => {
                let c = off((sc.number()?, sc.number()?), cur);
                let p = off((sc.number()?, sc.number()?), cur);
                quadratic(&mut ring, cur, c, p);
                last_q = Some(c);
                last_c = None;
                cur = p;
            }
            b'T' => {
                let c = last_q.map_or(cur, |c| (2.0 * cur.0 - c.0, 2.0 * cur.1 - c.1));
                let p = off((sc.number()?, sc.number()?), cur);
                quadratic(&mut ring, cur, c, p);
                last_q = Some(c);
                last_c = None;
                cur = p;
            }
            b'A' => {
                let (rx, ry, rot) = (sc.number()?, sc.number()?, sc.number()?);
                let large = sc.flag()?;
                let sweep = sc.flag()?;
                let p = off((sc.number()?, sc.number()?), cur);
                arc(&mut ring, cur, rx, ry, rot, large, sweep, p);
                last_c = None;
                last_q = None;
                cur = p;
            }
            other => {
                return Err(format!("`{}` is not a path command", other as char));
            }
        }
    }
    if !ring.is_empty() {
        loops.push(ring);
    }
    Ok(loops)
}

fn cubic(ring: &mut Vec<P>, p0: P, p1: P, p2: P, p3: P) {
    let n = CURVE_PIECES;
    for k in 1..=n {
        let t = k as f64 / n as f64;
        let u = 1.0 - t;
        ring.push((
            u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0,
            u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1,
        ));
    }
}

fn quadratic(ring: &mut Vec<P>, p0: P, p1: P, p2: P) {
    let n = CURVE_PIECES;
    for k in 1..=n {
        let t = k as f64 / n as f64;
        let u = 1.0 - t;
        ring.push((
            u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
            u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
        ));
    }
}

/// SVG's elliptical arc (appendix B.2.4 of the SVG specification: endpoint to centre form).
#[allow(clippy::too_many_arguments)]
fn arc(
    ring: &mut Vec<P>,
    from: P,
    rx: f64,
    ry: f64,
    rot_deg: f64,
    large: bool,
    sweep: bool,
    to: P,
) {
    if from == to {
        return;
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        ring.push(to);
        return;
    }
    let phi = rot_deg.to_radians();
    let (cos, sin) = (phi.cos(), phi.sin());
    let (dx, dy) = ((from.0 - to.0) / 2.0, (from.1 - to.1) / 2.0);
    let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = if den == 0.0 {
        0.0
    } else {
        (num / den).max(0.0).sqrt()
    };
    if large == sweep {
        coef = -coef;
    }
    let (cxp, cyp) = (coef * rx * y1 / ry, -coef * ry * x1 / rx);
    let (cx, cy) = (
        cos * cxp - sin * cyp + (from.0 + to.0) / 2.0,
        sin * cxp + cos * cyp + (from.1 + to.1) / 2.0,
    );
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let theta1 = angle(1.0, 0.0, (x1 - cxp) / rx, (y1 - cyp) / ry);
    let mut delta = angle(
        (x1 - cxp) / rx,
        (y1 - cyp) / ry,
        (-x1 - cxp) / rx,
        (-y1 - cyp) / ry,
    );
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    let n = ((delta.abs() / (std::f64::consts::PI / 36.0)).ceil() as usize).max(2);
    for k in 1..n {
        let t = theta1 + delta * k as f64 / n as f64;
        let (ex, ey) = (rx * t.cos(), ry * t.sin());
        ring.push((cos * ex - sin * ey + cx, sin * ex + cos * ey + cy));
    }
    ring.push(to);
}
