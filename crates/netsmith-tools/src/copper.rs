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

/// What a zone of a KiCad board is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneKind {
    /// Copper of a net: what a routes file holds.
    Copper,
    /// A rule area (`(keepout ...)`): no copper.
    RuleArea,
    /// A teardrop (`(attr (teardrop ...))`): copper that `fab` draws itself.
    Teardrop,
}

/// One zone of a KiCad board, with the settings that change its copper in the routes file's
/// terms (`netsmith_pcb::RouteZone`), its places in mm from the top left corner of the board's
/// `Edge.Cuts` box.
#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    pub kind: ZoneKind,
    /// The net's name as the board writes it (`""` for none).
    pub net: String,
    /// KiCad's names of its copper layers, KiCad's shorthands (`F&B.Cu`, `*.Cu`) spelt out.
    pub layers: Vec<String>,
    /// The first `(polygon`, which KiCad writes as the outline.
    pub outline: Vec<[f64; 2]>,
    /// Each further `(polygon`: a cutout.
    pub holes: Vec<Vec<[f64; 2]>>,
    pub priority: u32,
    pub clearance: f64,
    /// KiCad's `min_thickness`.
    pub min_width: f64,
    pub pads: netsmith_pcb::ZonePads,
    pub thermal_gap: f64,
    /// KiCad's `thermal_bridge_width`.
    pub spoke_width: f64,
    pub islands: netsmith_pcb::ZoneIslands,
    /// KiCad's `island_area_min`, in mm².
    pub island_min_area: Option<f64>,
    /// Filled with hatching (`(mode hatch)`), not solid.
    pub hatched: bool,
    /// Its corners rounded or cut (`(smoothing fillet)`, `(smoothing chamfer)`).
    pub smoothed: bool,
}

impl fmt::Display for Zone {
    /// `zone of net GND on F.Cu and B.Cu, priority 1, 4 corners from (0, 0), 1 hole`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let first = self.outline.first().copied().unwrap_or_default();
        write!(
            f,
            "zone of net {} on {}, priority {}, {} corners from ({}, {}), {} hole(s)",
            if self.net.is_empty() {
                "(none)"
            } else {
                &self.net
            },
            self.layers.join(" and "),
            self.priority,
            self.outline.len(),
            round(first[0]),
            round(first[1]),
            self.holes.len()
        )
    }
}

/// The zones of the KiCad board `pcb`, rule areas and teardrops among them ([`Zone::kind`]), with
/// what could not be read, each a sentence (a zone with no outline, or without a setting that
/// KiCad writes into every zone: its clearance, minimum thickness, thermal gap and spoke width).
pub fn read_zones(pcb: &str, source: &str) -> Result<(Vec<Zone>, Vec<String>), String> {
    use netsmith_pcb::{ZoneIslands, ZonePads};
    let outline = crate::reference_table::read_board(pcb, source)?.outline;
    let root = Sexp::parse(pcb).map_err(|e| format!("{source}: {e}"))?;
    let origin = (outline.min_x, outline.min_y);
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
    let copper: Vec<String> = root
        .find("layers")
        .map(|l| {
            l.args()
                .iter()
                .filter_map(|e| e.args().first()?.text())
                .filter(|n| n.ends_with(".Cu"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let number = |item: &Sexp, key: &str| -> Option<f64> {
        item.find(key)?.args().first()?.text()?.parse().ok()
    };
    let ring = |polygon: &Sexp| -> Vec<[f64; 2]> {
        polygon
            .find("pts")
            .map(|pts| {
                pts.find_all("xy")
                    .filter_map(|xy| {
                        let a = xy.args();
                        let n = |k: usize| a.get(k)?.text()?.parse::<f64>().ok();
                        Some([round(n(0)? - origin.0), round(n(1)? - origin.1)])
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut zones = Vec::new();
    let mut unread = Vec::new();
    for item in root.args().iter().filter(|i| i.head() == Some("zone")) {
        let net = match item.find("net_name").and_then(|n| n.args().first()?.text()) {
            Some(name) => name.to_string(),
            None => match item.find("net").and_then(|n| n.args().first()) {
                Some(Sexp::Str(name)) => name.clone(),
                Some(other) => other
                    .text()
                    .and_then(|t| numbered.get(t).cloned())
                    .unwrap_or_default(),
                None => String::new(),
            },
        };
        let named: Vec<String> = match item.find("layers") {
            Some(l) => l
                .args()
                .iter()
                .filter_map(Sexp::text)
                .map(str::to_string)
                .collect(),
            None => item
                .find("layer")
                .and_then(|l| l.args().first()?.text())
                .map(|l| vec![l.to_string()])
                .unwrap_or_default(),
        };
        let mut layers = Vec::new();
        for l in named {
            match l.as_str() {
                "F&B.Cu" => layers.extend(["F.Cu".to_string(), "B.Cu".to_string()]),
                "*.Cu" => layers.extend(copper.iter().cloned()),
                _ => layers.push(l),
            }
        }
        let kind = if item.find("keepout").is_some() {
            ZoneKind::RuleArea
        } else if item
            .find("attr")
            .is_some_and(|a| a.find("teardrop").is_some())
        {
            ZoneKind::Teardrop
        } else {
            ZoneKind::Copper
        };
        let mut polygons = item.find_all("polygon").map(ring);
        let Some(outline) = polygons.next().filter(|o| o.len() >= 3) else {
            unread.push(format!("a zone of net `{net}` with no outline: left out"));
            continue;
        };
        let holes: Vec<Vec<[f64; 2]>> = polygons.collect();
        let connect = item.find("connect_pads");
        let fill = item.find("fill");
        let pads = match connect.and_then(|c| c.args().first()?.text()) {
            Some("yes") => ZonePads::Solid,
            Some("no") => ZonePads::None,
            Some("thru_hole_only") => ZonePads::ThruHoleOnly,
            _ => ZonePads::Thermal,
        };
        let settings = (
            connect.and_then(|c| number(c, "clearance")),
            number(item, "min_thickness"),
            fill.and_then(|f| number(f, "thermal_gap")),
            fill.and_then(|f| number(f, "thermal_bridge_width")),
        );
        let (Some(clearance), Some(min_width), Some(thermal_gap), Some(spoke_width)) = settings
        else {
            if kind == ZoneKind::Copper {
                unread.push(format!(
                    "a zone of net `{net}` without its clearance, minimum thickness, thermal gap or spoke width: left out"
                ));
                continue;
            }
            zones.push(Zone {
                kind,
                net,
                layers,
                outline,
                holes,
                priority: 0,
                clearance: 0.0,
                min_width: 0.0,
                pads,
                thermal_gap: 0.0,
                spoke_width: 0.0,
                islands: ZoneIslands::Remove,
                island_min_area: None,
                hatched: false,
                smoothed: false,
            });
            continue;
        };
        let islands = match fill.and_then(|f| number(f, "island_removal_mode")) {
            Some(1.0) => ZoneIslands::Keep,
            Some(2.0) => ZoneIslands::RemoveBelow,
            _ => ZoneIslands::Remove,
        };
        let island_min_area = (islands == ZoneIslands::RemoveBelow).then(|| {
            fill.and_then(|f| number(f, "island_area_min"))
                .unwrap_or(0.0)
        });
        let word = |key: &str| fill.and_then(|f| f.find(key)?.args().first()?.text());
        zones.push(Zone {
            kind,
            net,
            layers,
            outline,
            holes,
            priority: number(item, "priority").map_or(0, |p| p.max(0.0) as u32),
            clearance,
            min_width,
            pads,
            thermal_gap,
            spoke_width,
            islands,
            island_min_area,
            hatched: word("mode").is_some_and(|m| m != "solid"),
            smoothed: word("smoothing").is_some_and(|m| m != "none"),
        });
    }
    Ok((zones, unread))
}

/// How two boards' copper zones differ ([`compare_zones`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZoneDiff {
    /// How many zones of the reference the other board has, with every setting the same.
    pub matched: usize,
    /// The reference's zones the other board has none of where they are.
    pub missing: Vec<Zone>,
    /// The other board's zones the reference does not have.
    pub extra: Vec<Zone>,
    /// The zones where both boards have one, on the same layers and with the same outline and
    /// holes, but whose settings differ, each a sentence naming the settings.
    pub differing: Vec<String>,
    /// The other board's net of each net of the reference, by the zones they share.
    pub nets: BTreeMap<String, String>,
    /// Where the zones of one net of either board are on two nets of the other, each a sentence.
    pub crossed: Vec<String>,
}

impl ZoneDiff {
    pub fn is_same(&self) -> bool {
        self.missing.is_empty()
            && self.extra.is_empty()
            && self.differing.is_empty()
            && self.crossed.is_empty()
    }
}

impl fmt::Display for ZoneDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "matched {} of {} zones; {} missing, {} extra, {} with other settings, {} on another net",
            self.matched,
            self.matched + self.missing.len() + self.differing.len(),
            self.missing.len(),
            self.extra.len(),
            self.differing.len(),
            self.crossed.len()
        )?;
        for z in &self.missing {
            writeln!(f, "  missing: {z}")?;
        }
        for z in &self.extra {
            writeln!(f, "  extra: {z}")?;
        }
        for d in &self.differing {
            writeln!(f, "  other settings: {d}")?;
        }
        for c in &self.crossed {
            writeln!(f, "  on another net: {c}")?;
        }
        Ok(())
    }
}

/// Whether two rings are the same to `tolerance` mm, from any corner and either way round.
fn same_ring(a: &[[f64; 2]], b: &[[f64; 2]], tolerance: f64) -> bool {
    let close = |p: [f64; 2], q: [f64; 2]| {
        (p[0] - q[0]).abs() <= tolerance && (p[1] - q[1]).abs() <= tolerance
    };
    let n = a.len();
    if n != b.len() || n == 0 {
        return n == b.len();
    }
    (0..n).any(|shift| {
        (0..n).all(|i| close(a[i], b[(i + shift) % n]))
            || (0..n).all(|i| close(a[i], b[(shift + n - i) % n]))
    })
}

/// Whether two zones stand in the same place: the same layers, outline and holes.
fn same_place(a: &Zone, b: &Zone, tolerance: f64) -> bool {
    let sorted = |z: &Zone| {
        let mut l = z.layers.clone();
        l.sort();
        l
    };
    sorted(a) == sorted(b)
        && same_ring(&a.outline, &b.outline, tolerance)
        && a.holes.len() == b.holes.len()
        && a.holes
            .iter()
            .all(|h| b.holes.iter().any(|g| same_ring(h, g, tolerance)))
}

/// The settings in which `ours` differs from the reference's `theirs`, each `name ours (theirs)`.
fn other_settings(theirs: &Zone, ours: &Zone, tolerance: f64) -> Vec<String> {
    let mut said = Vec::new();
    let mut size = |name: &str, t: f64, o: f64| {
        if (t - o).abs() > tolerance {
            said.push(format!(
                "{name} {} (the reference's {})",
                round(o),
                round(t)
            ));
        }
    };
    size("clearance", theirs.clearance, ours.clearance);
    size("min_width", theirs.min_width, ours.min_width);
    size("thermal_gap", theirs.thermal_gap, ours.thermal_gap);
    size("spoke_width", theirs.spoke_width, ours.spoke_width);
    size(
        "island_min_area",
        theirs.island_min_area.unwrap_or(0.0),
        ours.island_min_area.unwrap_or(0.0),
    );
    if theirs.priority != ours.priority {
        said.push(format!(
            "priority {} (the reference's {})",
            ours.priority, theirs.priority
        ));
    }
    if theirs.pads != ours.pads {
        said.push(format!(
            "pads {:?} (the reference's {:?})",
            ours.pads, theirs.pads
        ));
    }
    if theirs.islands != ours.islands {
        said.push(format!(
            "islands {:?} (the reference's {:?})",
            ours.islands, theirs.islands
        ));
    }
    if (theirs.hatched, theirs.smoothed) != (ours.hatched, ours.smoothed) {
        said.push(format!(
            "hatched {} and smoothed {} (the reference's {} and {})",
            ours.hatched, ours.smoothed, theirs.hatched, theirs.smoothed
        ));
    }
    said
}

/// How the copper zones of `ours` differ from those of `reference` (rule areas and teardrops
/// aside), as sets: each zone of the reference is paired with one of `ours` on the same layers
/// with the same outline and holes to `tolerance` mm (from any corner, either way round), whatever
/// the order or the nets' names; a pair whose settings differ is [`ZoneDiff::differing`]. The
/// nets are then compared by the pairs, as [`compare`] compares them.
pub fn compare_zones(reference: &[Zone], ours: &[Zone], tolerance: f64) -> ZoneDiff {
    let copper = |z: &&Zone| z.kind == ZoneKind::Copper;
    let ours: Vec<&Zone> = ours.iter().filter(copper).collect();
    let mut used = vec![false; ours.len()];
    let mut diff = ZoneDiff::default();
    let mut pairs: BTreeMap<(String, String), usize> = BTreeMap::new();
    for r in reference.iter().filter(copper) {
        let found = (0..ours.len()).find(|&i| !used[i] && same_place(r, ours[i], tolerance));
        let Some(i) = found else {
            diff.missing.push(r.clone());
            continue;
        };
        used[i] = true;
        *pairs
            .entry((r.net.clone(), ours[i].net.clone()))
            .or_default() += 1;
        let other = other_settings(r, ours[i], tolerance);
        if other.is_empty() {
            diff.matched += 1;
        } else {
            diff.differing.push(format!("{r}: {}", other.join(", ")));
        }
    }
    diff.extra = ours
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(z, _)| (*z).clone())
        .collect();
    let mut of_reference: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut of_ours: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (r, o) in pairs.keys() {
        of_reference.entry(r).or_default().push(o);
        of_ours.entry(o).or_default().push(r);
    }
    for (r, v) in &of_reference {
        if let [o] = v.as_slice() {
            diff.nets.insert(r.to_string(), o.to_string());
        } else {
            diff.crossed.push(format!(
                "the zones of the reference's net `{r}` are on nets {} of this board",
                v.join(", ")
            ));
        }
    }
    for (o, v) in &of_ours {
        if v.len() > 1 {
            diff.crossed.push(format!(
                "this board's net `{o}` has the zones of the reference's nets {}",
                v.join(", ")
            ));
        }
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZONED: &str = r#"(kicad_pcb (version 20240108)
        (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (44 "Edge.Cuts" user))
        (net 0 "") (net 1 "GND") (net 2 "VCC")
        (gr_line (start 10 10) (end 30 10) (layer "Edge.Cuts"))
        (gr_line (start 30 10) (end 30 30) (layer "Edge.Cuts"))
        (gr_line (start 30 30) (end 10 30) (layer "Edge.Cuts"))
        (gr_line (start 10 30) (end 10 10) (layer "Edge.Cuts"))
        (zone (net 1) (net_name "GND") (layers "F&B.Cu") (priority 2)
          (connect_pads (clearance 0.3)) (min_thickness 0.25)
          (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.4) (island_removal_mode 2) (island_area_min 30))
          (polygon (pts (xy 11 11) (xy 29 11) (xy 29 29) (xy 11 29)))
          (polygon (pts (xy 15 15) (xy 16 15) (xy 16 16))))
        (zone (net 2) (net_name "VCC") (layer "F.Cu")
          (connect_pads yes (clearance 0.2)) (min_thickness 0.2)
          (fill yes (thermal_gap 0.3) (thermal_bridge_width 0.3))
          (polygon (pts (xy 12 12) (xy 14 12) (xy 14 14))))
        (zone (net 0) (net_name "") (layer "F.Cu") (keepout (tracks not_allowed))
          (polygon (pts (xy 20 20) (xy 21 20) (xy 21 21)))))"#;

    #[test]
    fn a_zones_settings_and_holes_are_read_from_the_boards_corner() {
        let (zones, unread) = read_zones(ZONED, "z").unwrap();
        assert!(unread.is_empty(), "{unread:?}");
        assert_eq!(zones.len(), 3);
        let gnd = &zones[0];
        assert_eq!(gnd.kind, ZoneKind::Copper);
        assert_eq!(gnd.layers, ["F.Cu", "B.Cu"]);
        assert_eq!(gnd.outline[0], [1.0, 1.0]);
        assert_eq!(gnd.holes, [vec![[5.0, 5.0], [6.0, 5.0], [6.0, 6.0]]]);
        assert_eq!(
            (
                gnd.priority,
                gnd.clearance,
                gnd.min_width,
                gnd.thermal_gap,
                gnd.spoke_width
            ),
            (2, 0.3, 0.25, 0.5, 0.4)
        );
        assert_eq!(gnd.pads, netsmith_pcb::ZonePads::Thermal);
        assert_eq!(gnd.islands, netsmith_pcb::ZoneIslands::RemoveBelow);
        assert_eq!(gnd.island_min_area, Some(30.0));
        assert_eq!(zones[1].pads, netsmith_pcb::ZonePads::Solid);
        assert_eq!(zones[1].islands, netsmith_pcb::ZoneIslands::Remove);
        assert_eq!(zones[2].kind, ZoneKind::RuleArea);
    }

    #[test]
    fn zones_compare_as_sets_and_a_changed_setting_is_named() {
        let (reference, _) = read_zones(ZONED, "z").unwrap();
        let mut ours: Vec<Zone> = reference.iter().rev().cloned().collect();
        for z in &mut ours {
            z.net = z.net.to_lowercase();
            z.outline.rotate_left(1);
        }
        let diff = compare_zones(&reference, &ours, 1e-4);
        assert!(diff.is_same(), "{diff}");
        assert_eq!(diff.matched, 2);
        assert_eq!(diff.nets.get("GND").map(String::as_str), Some("gnd"));
        ours[2].clearance = 0.2;
        ours[1].outline[0][0] += 0.01;
        let diff = compare_zones(&reference, &ours, 1e-4);
        assert_eq!((diff.missing.len(), diff.extra.len()), (1, 1), "{diff}");
        assert!(
            diff.differing.len() == 1
                && diff.differing[0].contains("clearance 0.2 (the reference's 0.3)"),
            "{diff}"
        );
    }

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
