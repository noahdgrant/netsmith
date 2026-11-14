//! The copper of a KiCad board, its tracks, arcs and vias, read once for the import of a routes
//! file (`routes_import`) and for the comparison of two boards' copper (issue #599: a board
//! `fab` draws from a routes file imported from a KiCad board has that board's copper).
//!
//! Places are in mm from the top left corner of the board's `Edge.Cuts` box, as the parity
//! checker and the routes file have them (`reference_table::read_board`), so two boards whose
//! outlines stand at different places on the sheet compare.

use std::collections::BTreeMap;
use std::fmt;

use netsmith_kicad::Sexp;

/// What a shape is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Segment,
    Arc,
    Via,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Segment => "segment",
            Kind::Arc => "arc",
            Kind::Via => "via",
        }
    }
}

/// One track, arc or via of a KiCad board.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub kind: Kind,
    /// The net's name as the board writes it (`""` for none).
    pub net: String,
    /// The copper layer of a track or arc (`F.Cu`); of a via, its layers joined by `-`
    /// (`F.Cu-B.Cu`).
    pub layer: String,
    /// A track's or arc's start, a via's centre.
    pub from: [f64; 2],
    /// An arc's middle point.
    pub mid: Option<[f64; 2]>,
    /// A track's or arc's end; a via's centre again.
    pub to: [f64; 2],
    /// A track's width, a via's diameter.
    pub width: f64,
    /// A via's drill.
    pub drill: Option<f64>,
}

impl fmt::Display for Shape {
    /// `segment of net GND on F.Cu, 0.2 mm, (1, 2) to (3, 4)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pt = |p: [f64; 2]| format!("({}, {})", round(p[0]), round(p[1]));
        write!(
            f,
            "{} of net {} on {}, {} mm",
            self.kind.word(),
            if self.net.is_empty() {
                "(none)"
            } else {
                &self.net
            },
            self.layer,
            round(self.width)
        )?;
        match self.kind {
            Kind::Via => write!(
                f,
                ", drill {} mm, at {}",
                round(self.drill.unwrap_or(0.0)),
                pt(self.from)
            ),
            _ => {
                write!(f, ", {}", pt(self.from))?;
                if let Some(mid) = self.mid {
                    write!(f, " through {}", pt(mid))?;
                }
                write!(f, " to {}", pt(self.to))
            }
        }
    }
}

impl Shape {
    /// How long a track or arc is along its centre line, in mm; 0 for a via.
    pub fn length(&self) -> f64 {
        let d = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]);
        match (self.kind, self.mid) {
            (Kind::Via, _) => 0.0,
            (Kind::Arc, Some(m)) => {
                // The circle through the three points; each chord's angle at the centre is
                // twice the arcsine of half the chord over the radius.
                let (a, b, c) = (d(self.from, m), d(m, self.to), d(self.from, self.to));
                let cross = (m[0] - self.from[0]) * (self.to[1] - self.from[1])
                    - (m[1] - self.from[1]) * (self.to[0] - self.from[0]);
                if cross.abs() < 1e-12 {
                    return c;
                }
                let radius = a * b * c / (2.0 * cross.abs());
                let angle = |chord: f64| 2.0 * (chord / (2.0 * radius)).clamp(-1.0, 1.0).asin();
                radius * (angle(a) + angle(b))
            }
            _ => d(self.from, self.to),
        }
    }
}

/// To 1 nm, so a place reads as the board wrote it (11.6, not 11.600000000000001).
fn round(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// The copper of the KiCad board `pcb` (KiCad 9's numbered nets or KiCad 10's named ones), in
/// mm from the top left corner of its `Edge.Cuts` box, with what could not be read, each a
/// sentence (a shape with no start, end, width, place or size).
pub fn read(pcb: &str, source: &str) -> Result<(Vec<Shape>, Vec<String>), String> {
    let outline = crate::reference_table::read_board(pcb, source)?.outline;
    let root = Sexp::parse(pcb).map_err(|e| format!("{source}: {e}"))?;
    let origin = (outline.min_x, outline.min_y);
    let at = |x: f64, y: f64| [round(x - origin.0), round(y - origin.1)];
    // The KiCad 9 net table, number to name; KiCad 10 names the net on each item.
    let numbered: BTreeMap<String, String> = root
        .find_all("net")
        .filter_map(|n| {
            let args = n.args();
            Some((
                args.first()?.text()?.to_string(),
                args.get(1)?.text()?.to_string(),
            ))
        })
        .collect();
    let net_name = |item: &Sexp| -> Option<String> {
        let arg = item.find("net")?.args().first()?;
        match arg {
            Sexp::Str(name) => Some(name.clone()),
            other => numbered.get(other.text()?).cloned(),
        }
    };
    let point = |item: &Sexp, key: &str| -> Option<[f64; 2]> {
        let a = item.find(key)?.args();
        let n = |k: usize| a.get(k)?.text()?.parse::<f64>().ok();
        Some(at(n(0)?, n(1)?))
    };
    let number = |item: &Sexp, key: &str| -> Option<f64> {
        item.find(key)?.args().first()?.text()?.parse().ok()
    };
    let mut shapes = Vec::new();
    let mut unread = Vec::new();
    for item in root.args() {
        let kind = match item.head() {
            Some("segment") => Kind::Segment,
            Some("arc") => Kind::Arc,
            Some("via") => Kind::Via,
            _ => continue,
        };
        let net = net_name(item).unwrap_or_default();
        if kind == Kind::Via {
            let layers: Vec<&str> = item
                .find("layers")
                .map(|l| l.args().iter().filter_map(Sexp::text).collect())
                .unwrap_or_default();
            let (Some(centre), Some(width), Some(drill)) = (
                point(item, "at"),
                number(item, "size"),
                number(item, "drill"),
            ) else {
                unread.push(format!(
                    "a via of net `{net}` with no place, size or drill: left out"
                ));
                continue;
            };
            shapes.push(Shape {
                kind,
                net,
                layer: layers.join("-"),
                from: centre,
                mid: None,
                to: centre,
                width,
                drill: Some(drill),
            });
            continue;
        }
        let layer = item
            .find("layer")
            .and_then(|l| l.args().first()?.text())
            .unwrap_or_default()
            .to_string();
        let (Some(from), Some(to), Some(width)) = (
            point(item, "start"),
            point(item, "end"),
            number(item, "width"),
        ) else {
            unread.push(format!(
                "a {} of net `{net}` with no start, end or width: left out",
                kind.word()
            ));
            continue;
        };
        shapes.push(Shape {
            kind,
            net,
            layer,
            from,
            mid: if kind == Kind::Arc {
                point(item, "mid")
            } else {
                None
            },
            to,
            width,
            drill: None,
        });
    }
    Ok((shapes, unread))
}

/// How two boards' copper differs ([`compare`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Diff {
    /// How many shapes of the reference the other board has, by kind.
    pub matched: BTreeMap<Kind, usize>,
    /// The reference's shapes the other board does not have.
    pub missing: Vec<Shape>,
    /// The other board's shapes the reference does not have.
    pub extra: Vec<Shape>,
    /// The other board's net of each net of the reference, by the shapes they share.
    pub nets: BTreeMap<String, String>,
    /// Where the shapes of one net of either board are on two nets of the other, each a
    /// sentence: copper drawn on another net than the reference's.
    pub crossed: Vec<String>,
}

impl Diff {
    /// Whether every shape of each board is the other's, on one net each way.
    pub fn is_same(&self) -> bool {
        self.missing.is_empty() && self.extra.is_empty() && self.crossed.is_empty()
    }

    /// The shapes of the reference, by kind, matched or not.
    pub fn reference_count(&self, kind: Kind) -> usize {
        self.matched.get(&kind).copied().unwrap_or(0)
            + self.missing.iter().filter(|s| s.kind == kind).count()
    }
}

impl fmt::Display for Diff {
    /// One line of counts, then each missing, extra and crossed item on a line of its own.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = |k: Kind| self.matched.get(&k).copied().unwrap_or(0);
        writeln!(
            f,
            "matched {} of {} segments, {} of {} arcs and {} of {} vias; {} missing, {} extra, {} on another net",
            n(Kind::Segment),
            self.reference_count(Kind::Segment),
            n(Kind::Arc),
            self.reference_count(Kind::Arc),
            n(Kind::Via),
            self.reference_count(Kind::Via),
            self.missing.len(),
            self.extra.len(),
            self.crossed.len()
        )?;
        for s in &self.missing {
            writeln!(f, "  missing: {s}")?;
        }
        for s in &self.extra {
            writeln!(f, "  extra: {s}")?;
        }
        for c in &self.crossed {
            writeln!(f, "  on another net: {c}")?;
        }
        Ok(())
    }
}

/// Whether `a` of the reference and `b` of the other board are the same shape: the same kind,
/// layer and size, and the same ends (either way round; an arc through the same middle) to
/// `tolerance` mm.
fn same(a: &Shape, b: &Shape, tolerance: f64) -> bool {
    let close = |p: [f64; 2], q: [f64; 2]| {
        (p[0] - q[0]).abs() <= tolerance && (p[1] - q[1]).abs() <= tolerance
    };
    let ends = (close(a.from, b.from) && close(a.to, b.to))
        || (close(a.from, b.to) && close(a.to, b.from));
    let mid = match (a.mid, b.mid) {
        (Some(p), Some(q)) => close(p, q),
        (None, None) => true,
        _ => false,
    };
    let drill = match (a.drill, b.drill) {
        (Some(p), Some(q)) => (p - q).abs() <= tolerance,
        (None, None) => true,
        _ => false,
    };
    a.kind == b.kind
        && a.layer == b.layer
        && (a.width - b.width).abs() <= tolerance
        && drill
        && ends
        && mid
}

/// How the copper of `ours` differs from that of `reference`, as sets: each shape of the
/// reference is paired with one shape of `ours` that is the same to `tolerance` mm (kind, layer,
/// width or size and drill, ends either way round, an arc's middle), whatever the order or the
/// nets' names. The nets are then compared by the pairs: a net of the reference whose shapes are
/// on two nets of `ours`, or a net of `ours` that has the shapes of two of the reference's, is
/// [`Diff::crossed`].
pub fn compare(reference: &[Shape], ours: &[Shape], tolerance: f64) -> Diff {
    let mut diff = Diff::default();
    // The other board's shapes by kind and layer, each paired at most once.
    let mut pool: BTreeMap<(Kind, &str), Vec<(usize, &Shape)>> = BTreeMap::new();
    for (i, s) in ours.iter().enumerate() {
        pool.entry((s.kind, s.layer.as_str()))
            .or_default()
            .push((i, s));
    }
    let mut used = vec![false; ours.len()];
    let mut pairs: BTreeMap<(String, String), usize> = BTreeMap::new();
    for r in reference {
        let found = pool
            .get(&(r.kind, r.layer.as_str()))
            .and_then(|c| c.iter().find(|(i, s)| !used[*i] && same(r, s, tolerance)));
        match found {
            Some(&(i, s)) => {
                used[i] = true;
                *diff.matched.entry(r.kind).or_default() += 1;
                *pairs.entry((r.net.clone(), s.net.clone())).or_default() += 1;
            }
            None => diff.missing.push(r.clone()),
        }
    }
    diff.extra = ours
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(s, _)| s.clone())
        .collect();
    let mut of_reference: BTreeMap<&str, Vec<(&str, usize)>> = BTreeMap::new();
    let mut of_ours: BTreeMap<&str, Vec<(&str, usize)>> = BTreeMap::new();
    for ((r, o), n) in &pairs {
        of_reference.entry(r).or_default().push((o, *n));
        of_ours.entry(o).or_default().push((r, *n));
    }
    let list = |v: &[(&str, usize)]| {
        v.iter()
            .map(|(n, c)| format!("`{n}` ({c})"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    for (r, v) in &of_reference {
        if let [(o, _)] = v.as_slice() {
            diff.nets.insert(r.to_string(), o.to_string());
        } else {
            diff.crossed.push(format!(
                "the copper of the reference's net `{r}` is on {} nets of this board: {}",
                v.len(),
                list(v)
            ));
        }
    }
    for (o, v) in &of_ours {
        if v.len() > 1 {
            diff.crossed.push(format!(
                "this board's net `{o}` has the copper of {} nets of the reference: {}",
                v.len(),
                list(v)
            ));
        }
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(net: &str, from: [f64; 2], to: [f64; 2]) -> Shape {
        Shape {
            kind: Kind::Segment,
            net: net.into(),
            layer: "F.Cu".into(),
            from,
            mid: None,
            to,
            width: 0.2,
            drill: None,
        }
    }

    #[test]
    fn a_half_circle_is_as_long_as_half_its_circumference() {
        let arc = Shape {
            kind: Kind::Arc,
            mid: Some([1.0, 1.0]),
            to: [2.0, 0.0],
            ..segment("a", [0.0, 0.0], [0.0, 0.0])
        };
        assert!(
            (arc.length() - std::f64::consts::PI).abs() < 1e-9,
            "{}",
            arc.length()
        );
        assert!((segment("a", [0.0, 0.0], [3.0, 4.0]).length() - 5.0).abs() < 1e-12);
    }

    #[test]
    fn copper_in_another_order_and_with_other_net_names_is_the_same() {
        let reference = [
            segment("/A", [0.0, 0.0], [1.0, 0.0]),
            segment("/B", [0.0, 1.0], [1.0, 1.0]),
        ];
        let ours = [
            segment("b", [1.0, 1.0], [0.0, 1.00005]),
            segment("a", [0.0, 0.0], [1.0, 0.0]),
        ];
        let diff = compare(&reference, &ours, 1e-4);
        assert!(diff.is_same(), "{diff}");
        assert_eq!(diff.nets.get("/A").map(String::as_str), Some("a"));
        assert_eq!(diff.matched.get(&Kind::Segment), Some(&2));
    }

    #[test]
    fn a_shape_moved_widened_or_on_another_net_is_named() {
        let reference = [
            segment("/A", [0.0, 0.0], [1.0, 0.0]),
            segment("/A", [1.0, 0.0], [2.0, 0.0]),
            segment("/B", [0.0, 1.0], [1.0, 1.0]),
        ];
        let mut wide = segment("a", [1.0, 0.0], [2.0, 0.0]);
        wide.width = 0.25;
        let ours = [
            segment("a", [0.0, 0.0], [1.0, 0.001]),
            wide,
            segment("a", [0.0, 1.0], [1.0, 1.0]),
        ];
        let diff = compare(&reference, &ours, 1e-4);
        assert_eq!(diff.missing.len(), 2, "{diff}");
        assert_eq!(diff.extra.len(), 2, "{diff}");
        assert_eq!(diff.crossed.len(), 0, "{diff}");
        let text = diff.to_string();
        assert!(
            text.contains("missing: segment of net /A on F.Cu, 0.2 mm, (0, 0) to (1, 0)"),
            "{text}"
        );
        // Both nets of the reference on one of ours.
        let ours = [
            segment("a", [0.0, 0.0], [1.0, 0.0]),
            segment("a", [1.0, 0.0], [2.0, 0.0]),
            segment("a", [0.0, 1.0], [1.0, 1.0]),
        ];
        let diff = compare(&reference, &ours, 1e-4);
        assert_eq!(diff.crossed.len(), 1, "{diff}");
        assert!(diff.crossed[0].contains("`/A` (2), `/B` (1)"), "{diff}");
        assert!(!diff.is_same());
    }
}
