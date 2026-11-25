//! The DC resistance of a net's copper between two pads or groups of pads (issue #1703), the drop
//! at a current and the heat of each piece of copper the current passes, as `netsmith board
//! resistance` prints them.
//!
//! The copper is the net's connection graph (`crate::connection`, issues #1487, #1582): its
//! tracks split where another track, a via or a pad joins them, joined where the design rules
//! check joins them, each leg into a pad measured to the pad's centre (issue #1670,
//! [`netsmith_geom::leg_length`]). Here each via keeps a point on each copper layer, joined
//! through its barrel. The graph is a network of resistors, solved by its node voltages
//! ([`solve`]), so the current divides between parallel ways as copper divides it.
//!
//! # What is modelled (decided by the agent under the owner's delegation, issue #1703)
//!
//! - **A track** is `rho * length / (width * thickness)`, the resistance the track drop measures
//!   it at (`netsmith_pcb::COPPER_RESISTIVITY`, annealed copper at 20 °C, at the drawn width):
//!   in the outer copper on an outer layer and in the inner copper on an inner one; an arc by its
//!   chords. Its length is the leg's (issue #1670): the copper inside a pad is the pad's.
//! - **A via** is its barrel between the copper layers, `rho * height / (pi * ((r + t)^2 - r^2))`
//!   for its finished hole of radius `r` plated `t` mm thick (the process's hole plating), the
//!   height the board's stackup puts between the two layers (or its thickness shared evenly
//!   between them when the board file has no stackup).
//! - **A pad** has no resistance: every copper end in it is joined at its centre, on any layer,
//!   as a net's length joins them. A group of pads (`R1`, every pad of the part on the net, or
//!   several named) is one terminal: the current enters or leaves by all its pads at one voltage.
//! - **Not modelled**: a pour, a plane or a zone of the routes file (its copper is an area, and
//!   no line of it is a track: the figure is of the net's tracks and vias alone, an upper bound
//!   where a pour also joins the pads, and [`Resistance::left_out`] names it), a teardrop, the
//!   resistance a track's own heat adds and the fabricator's width tolerance (the drop is the
//!   drawn copper's, as the track drop's is).
//! - **The heat** of each piece at its share of the current is IPC-2221's (ADR 0036,
//!   [`netsmith_geom::ipc2221`]), the rule of `track_current` and `via_current`: a track's rise
//!   at the narrowest finished track the tolerance allows and the width it needs at the board's
//!   allowed rise ([`TrackSizing::least_width_on`]), a via's rise and the current one carries at
//!   that rise, by its own hole.

use netsmith_geom::ipc2221::TrackSizing;
use netsmith_geom::{Artwork, ArtworkPad, ArtworkVia, DrcShape, pad_copper};

use crate::connection::{self, SAME, Union, Vias};
use crate::net_table::Seg;

/// The resistivity of annealed copper at 20 °C, in ohm millimetres: the track drop's
/// (`netsmith_pcb::COPPER_RESISTIVITY`).
pub const COPPER_RESISTIVITY: f64 = 1.72e-5;

/// The least share of the current a piece carries to be on the paths the current takes: listed,
/// and counted for the narrowest track and the vias. A choice of this tool's (issue #1703): a
/// loop of copper off the way carries a trickle that says nothing of the way.
pub const LISTED_SHARE: f64 = 0.01;

/// How near the node voltages must come to the solution: the residual over the current put in.
const TOLERANCE: f64 = 1e-12;

/// The copper of a board, as the resistance reads it: the thickness of its outer and inner
/// layers and of the plating of its holes, in mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Copper {
    pub outer: f64,
    pub inner: f64,
    pub plating: f64,
}

/// What [`of`] measures: the net, its two terminals (each a list of pads, `R1.1`, or parts,
/// `R1`, every pad of the part on the net), the current (amperes, when it is known) and how the
/// board sizes copper for a current (`None` when it sizes none: then no heat is said).
#[derive(Debug, Clone)]
pub struct Ask<'a> {
    pub net: &'a str,
    pub from: &'a [String],
    pub to: &'a [String],
    pub current: Option<f64>,
    /// The current that heats the copper, in amperes, where it is less than `current`: the
    /// net's rms current (issue #1690), which the heat is judged at while the drop stays at
    /// `current`.
    pub heating: Option<f64>,
    pub copper: Copper,
    pub sizing: Option<TrackSizing>,
}

/// What a piece of copper is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Track,
    Via,
}

/// One piece of copper the current passes: a track between two joins, or a via.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Segment {
    pub kind: Kind,
    /// A track's layer; a via's top and bottom layers, joined by `-`.
    pub layer: String,
    /// A track's ends, the way the current flows; a via's place twice.
    pub from: (f64, f64),
    pub to: (f64, f64),
    /// A track's drawn width, or a via's finished hole, in mm.
    pub width: f64,
    /// A track's length, or a via's barrel between the layers the current passes, in mm.
    pub length: f64,
    /// Its resistance, in ohms.
    pub resistance: f64,
    /// The share of the current it carries.
    pub share: f64,
    /// The current, in amperes, and what it drops, in volts, when the current is known; its
    /// rise is at its share of the heating current where the net has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drop: Option<f64>,
    /// Its temperature rise by IPC-2221, in kelvin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rise: Option<f64>,
    /// A track's least width for its current at the allowed rise, in mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs: Option<f64>,
    /// The most current a via carries at the allowed rise, in amperes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carries: Option<f64>,
    /// Whether it rises more than the board allows: the track is narrower than it needs, or the
    /// via carries more than it can.
    pub over: bool,
}

/// The narrowest track the current passes.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Narrowest {
    pub width: f64,
    pub layer: String,
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub share: f64,
}

/// The resistance of a net's copper between two terminals, and what the current does in it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Resistance {
    pub net: String,
    /// The pads of each terminal, as `R1.1`.
    pub from: Vec<String>,
    pub to: Vec<String>,
    /// In ohms.
    pub resistance: f64,
    /// The current, in amperes, and the voltage it drops, in volts, when it is known.
    pub current: Option<f64>,
    pub drop: Option<f64>,
    /// The rms current the heat is judged at, in amperes, where it is less than `current`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heating: Option<f64>,
    /// The temperature rise the board allows, in kelvin: what `over` is judged against.
    pub allowed_rise: Option<f64>,
    /// The narrowest track on the paths the current takes, if it passes any.
    pub narrowest: Option<Narrowest>,
    /// The vias on the paths the current takes.
    pub vias: usize,
    /// The pieces that carry [`LISTED_SHARE`] of the current or more, the hottest first (the
    /// largest share first where the heat is not known).
    pub segments: Vec<Segment>,
    /// The copper of the net the resistance leaves out: each pour, plane or zone, by its layers.
    pub left_out: Vec<String>,
    /// The work of the solve: its iterations.
    pub iterations: usize,
}

/// The pads of `art` on `net` that `names` name: `R1.1` the pads numbered 1 of R1, `R1` every
/// pad of R1 on the net. An error names one that names none.
fn terminal(pads: &[&ArtworkPad], net: &str, names: &[String]) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for name in names {
        let (part, number) = match name.split_once('.') {
            Some((p, n)) => (p, Some(n)),
            None => (name.as_str(), None),
        };
        let found: Vec<usize> = (0..pads.len())
            .filter(|&d| pads[d].part == part && number.is_none_or(|n| pads[d].number == n))
            .collect();
        if found.is_empty() {
            return Err(format!("{name} is no pad of net {net}"));
        }
        for d in found {
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    Ok(out)
}

/// The height of each copper layer of `art` in the board, in mm from its top: from its stackup,
/// or its thickness (1.6 mm when the file gives none) shared evenly between the layers.
fn heights(art: &Artwork) -> Vec<f64> {
    let n = art.copper.len();
    let mut at = Vec::new();
    let mut z = 0.0;
    for l in &art.stackup {
        if l.kind == "copper" {
            at.push(z + l.thickness / 2.0);
        }
        z += l.thickness;
    }
    if at.len() == n && n > 1 {
        return at;
    }
    let thickness = art.thickness.unwrap_or(1.6);
    (0..n)
        .map(|l| thickness * l as f64 / (n.max(2) - 1) as f64)
        .collect()
}

/// The area of copper of a via's barrel with a finished hole of `drill` mm plated `plating` mm
/// thick, in mm².
fn barrel_area(drill: f64, plating: f64) -> f64 {
    let r = drill / 2.0;
    std::f64::consts::PI * ((r + plating).powi(2) - r * r)
}

/// An edge of the network: two nodes, a resistance and what it is.
struct Edge {
    a: usize,
    b: usize,
    ohms: f64,
    what: What,
}

enum What {
    /// The piece of the graph of that index, and its length.
    Piece(usize, f64),
    /// The barrel of the via of that index between two layers.
    Barrel(usize, usize, usize),
}

/// The resistance of `ask.net`'s copper between `ask.from` and `ask.to`, as the header says.
/// An error when a terminal names no pad of the net, when the two share a pad, or when no copper
/// of the net joins them.
pub fn of(art: &Artwork, ask: &Ask<'_>) -> Result<Resistance, String> {
    let net = ask.net;
    let segs: Vec<Seg> = crate::score::lines(art)
        .into_iter()
        .filter(|s| s.0 == net)
        .collect();
    let segs: Vec<&Seg> = segs.iter().collect();
    let pads: Vec<&ArtworkPad> = art
        .pads
        .iter()
        .filter(|p| p.is_terminal() && p.net.as_deref() == Some(net))
        .collect();
    if pads.is_empty() {
        return Err(format!("the board has no pad on net {net}"));
    }
    let a = terminal(&pads, net, ask.from)?;
    let b = terminal(&pads, net, ask.to)?;
    if let Some(&d) = a.iter().find(|d| b.contains(d)) {
        return Err(format!(
            "{}.{} is in both ends: they must be two pads, or groups, apart",
            pads[d].part, pads[d].number
        ));
    }
    let vias: Vec<&ArtworkVia> = art
        .vias
        .iter()
        .filter(|v| v.net.as_deref() == Some(net))
        .collect();
    let copper: Vec<Vec<DrcShape>> = pads.iter().map(|p| pad_copper(p)).collect();
    // The joins only a round end makes are found on the graph whose vias are whole, as a net's
    // length finds them; then each via keeps a point on each layer.
    let joined = connection::graph(art, &segs, &vias, &pads, &copper, &[], Vias::Joined);
    let reach = connection::reaches(art, &segs, &vias, &copper, &joined);
    let g = connection::graph(art, &segs, &vias, &pads, &copper, &reach, Vias::ByLayer);
    let layers = art.copper.len();
    let ends = 2 * g.pieces.len();
    let pad0 = ends + vias.len();
    let lay0 = pad0 + pads.len();
    let layer_index = |name: &str| art.copper.iter().position(|c| c == name);
    let is_outer = |name: &str| layer_index(name).is_none_or(|l| l == 0 || l + 1 == layers);
    let thickness = |name: &str| match is_outer(name) {
        true => ask.copper.outer,
        false => ask.copper.inner,
    };
    // The edges, and the joins of no resistance (a piece of no length, a via of no copper).
    let mut edges: Vec<Edge> = Vec::new();
    let mut union = Union::new(g.node.len());
    for (k, &root) in g.node.iter().enumerate() {
        union.join(k, root);
    }
    for (e, s) in g.pieces.iter().enumerate() {
        let (u, v) = (g.node[2 * e], g.node[2 * e + 1]);
        if u == v {
            continue;
        }
        if s.4 <= 0.0 || thickness(&s.1) <= 0.0 {
            continue;
        }
        let length = connection::piece_length(&g, e, pad0, &pads, &copper);
        let ohms = COPPER_RESISTIVITY * length / (s.4 * thickness(&s.1));
        if ohms > 0.0 && ohms.is_finite() {
            edges.push(Edge {
                a: u,
                b: v,
                ohms,
                what: What::Piece(e, length),
            });
        } else if length <= SAME {
            union.join(u, v);
        }
    }
    let z = heights(art);
    let area_of = |v: &ArtworkVia| barrel_area(v.drill, ask.copper.plating);
    for (i, via) in vias.iter().enumerate() {
        let on: Vec<usize> = (0..layers).filter(|&l| via.on(l)).collect();
        for w in on.windows(2) {
            let (top, bottom) = (lay0 + i * layers + w[0], lay0 + i * layers + w[1]);
            let height = (z[w[1]] - z[w[0]]).abs();
            let area = area_of(via);
            if area > 0.0 && height > 0.0 {
                edges.push(Edge {
                    a: g.node[top],
                    b: g.node[bottom],
                    ohms: COPPER_RESISTIVITY * height / area,
                    what: What::Barrel(i, w[0], w[1]),
                });
            } else {
                union.join(g.node[top], g.node[bottom]);
            }
        }
    }
    // Each terminal is one node.
    for w in a.windows(2) {
        union.join(g.node[pad0 + w[0]], g.node[pad0 + w[1]]);
    }
    for w in b.windows(2) {
        union.join(g.node[pad0 + w[0]], g.node[pad0 + w[1]]);
    }
    let (from, to) = (union.find(pad0 + a[0]), union.find(pad0 + b[0]));
    let name = |d: &usize| format!("{}.{}", pads[*d].part, pads[*d].number);
    let names = |set: &[usize]| set.iter().map(name).collect::<Vec<_>>();
    if from == to {
        return Err(format!(
            "{} and {} are joined by pad copper alone: no track or via is between them",
            names(&a).join(","),
            names(&b).join(",")
        ));
    }
    let mut edges: Vec<Edge> = edges
        .into_iter()
        .map(|e| Edge {
            a: union.find(e.a),
            b: union.find(e.b),
            ..e
        })
        .filter(|e| e.a != e.b)
        .collect();
    // The nodes, numbered.
    let mut index = vec![usize::MAX; g.node.len()];
    let mut n = 0;
    for e in &edges {
        for k in [e.a, e.b] {
            if index[k] == usize::MAX {
                index[k] = n;
                n += 1;
            }
        }
    }
    for e in &mut edges {
        (e.a, e.b) = (index[e.a], index[e.b]);
    }
    let (from, to) = (index[from], index[to]);
    if from == usize::MAX || to == usize::MAX {
        return Err(format!(
            "no track or via of net {net} joins {} to {}",
            names(&a).join(","),
            names(&b).join(",")
        ));
    }
    let solved = solve(n, &edges, from, to).ok_or_else(|| {
        format!(
            "no track or via of net {net} joins {} to {}",
            names(&a).join(","),
            names(&b).join(",")
        )
    })?;
    let resistance = solved.volts[from];
    let rise_allowed = ask.sizing.map(|s| s.rise);
    // Each piece at its share, a via at the most any piece of its barrel carries.
    let mut segments: Vec<Segment> = Vec::new();
    let mut barrels: Vec<Option<(f64, usize, usize, f64)>> = vec![None; vias.len()];
    for e in &edges {
        let flow = (solved.volts[e.a] - solved.volts[e.b]) / e.ohms;
        let share = flow.abs();
        match e.what {
            What::Piece(p, length) => {
                // An edge's `a` is its piece's start: the current flows from it when positive.
                let s = &g.pieces[p];
                let (f, t) = if flow >= 0.0 { (s.2, s.3) } else { (s.3, s.2) };
                segments.push(Segment {
                    kind: Kind::Track,
                    layer: s.1.clone(),
                    from: (f.x, f.y),
                    to: (t.x, t.y),
                    width: s.4,
                    length,
                    resistance: e.ohms,
                    share,
                    current: None,
                    drop: None,
                    rise: None,
                    needs: None,
                    carries: None,
                    over: false,
                });
            }
            What::Barrel(v, top, bottom) => {
                let (ohms, lo, hi, most) = barrels[v].get_or_insert((0.0, top, bottom, 0.0));
                *lo = (*lo).min(top);
                *hi = (*hi).max(bottom);
                *ohms += e.ohms;
                *most = most.max(share);
            }
        }
    }
    for (v, b) in barrels.iter().enumerate() {
        let Some((ohms, top, bottom, share)) = *b else {
            continue;
        };
        let via = vias[v];
        segments.push(Segment {
            kind: Kind::Via,
            layer: format!("{}-{}", art.copper[top], art.copper[bottom]),
            from: (via.at.x, via.at.y),
            to: (via.at.x, via.at.y),
            width: via.drill,
            length: (z[bottom] - z[top]).abs(),
            resistance: ohms,
            share,
            current: None,
            drop: None,
            rise: None,
            needs: None,
            carries: None,
            over: false,
        });
    }
    segments.retain(|s| s.share >= LISTED_SHARE);
    for s in &mut segments {
        let Some(amps) = ask.current else { continue };
        let i = amps * s.share;
        s.current = Some(i);
        s.drop = Some(i * s.resistance);
        let Some(sizing) = ask.sizing else { continue };
        let i = ask.heating.map_or(i, |h| h * s.share);
        match s.kind {
            Kind::Track => {
                let inner = s.layer.starts_with("In");
                let rise = sizing.track_rise(i, s.width, inner);
                let needs = sizing.least_width_on(i, inner);
                s.rise = Some(rise);
                s.needs = Some(needs);
                s.over = s.width < needs - 1e-9;
            }
            Kind::Via => {
                let carries = sizing.via_capacity(s.width);
                s.rise = Some(netsmith_geom::ipc2221::via_rise(
                    i,
                    s.width,
                    sizing.via_plating,
                ));
                s.carries = Some(carries);
                s.over = i > carries * (1.0 + 1e-9);
            }
        }
    }
    segments.sort_by(|x, y| {
        let rise = |s: &Segment| s.rise.unwrap_or(0.0);
        rise(y)
            .total_cmp(&rise(x))
            .then(y.share.total_cmp(&x.share))
    });
    let narrowest = segments
        .iter()
        .filter(|s| s.kind == Kind::Track)
        .min_by(|x, y| {
            x.width
                .total_cmp(&y.width)
                .then(y.share.total_cmp(&x.share))
        })
        .map(|s| Narrowest {
            width: s.width,
            layer: s.layer.clone(),
            from: s.from,
            to: s.to,
            share: s.share,
        });
    let mut left_out: Vec<String> = art
        .zones
        .iter()
        .filter(|z| z.net.as_deref() == Some(net) && !z.outlines.is_empty())
        .map(|z| z.layers.join(","))
        .collect();
    left_out.sort();
    left_out.dedup();
    Ok(Resistance {
        net: net.to_string(),
        from: names(&a),
        to: names(&b),
        resistance,
        current: ask.current,
        drop: ask.current.map(|i| i * resistance),
        heating: ask.heating,
        allowed_rise: rise_allowed,
        narrowest,
        vias: segments.iter().filter(|s| s.kind == Kind::Via).count(),
        segments,
        left_out,
        iterations: solved.iterations,
    })
}

/// The node voltages of a network.
pub struct Solved {
    /// Each node's voltage with 1 A put in at the source and taken out at the sink (0 V), so the
    /// source's is the resistance between them; a node joined to neither is 0.
    pub volts: Vec<f64>,
    /// The iterations the solve took: its work.
    pub iterations: usize,
}

/// The node voltages of the network of `n` nodes and `edges` with 1 A put in at `source` and
/// taken out at `sink`, by conjugate gradients on its conductance matrix with the sink at 0 V
/// (Jacobi preconditioned). Only the nodes the source's copper joins are solved. `None` when the
/// sink is not among them.
fn solve(n: usize, edges: &[Edge], source: usize, sink: usize) -> Option<Solved> {
    // The nodes joined to the source.
    let mut next: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for e in edges {
        let g = 1.0 / e.ohms;
        next[e.a].push((e.b, g));
        next[e.b].push((e.a, g));
    }
    let mut reached = vec![false; n];
    let mut stack = vec![source];
    reached[source] = true;
    while let Some(u) = stack.pop() {
        for &(v, _) in &next[u] {
            if !reached[v] {
                reached[v] = true;
                stack.push(v);
            }
        }
    }
    if !reached[sink] {
        return None;
    }
    // The unknowns: every node reached but the sink.
    let free = |k: usize| reached[k] && k != sink;
    let diagonal: Vec<f64> = (0..n)
        .map(|k| next[k].iter().map(|&(_, g)| g).sum::<f64>())
        .collect();
    let apply = |x: &[f64], out: &mut [f64]| {
        for k in 0..n {
            out[k] = if free(k) {
                diagonal[k] * x[k]
                    - next[k]
                        .iter()
                        .filter(|&&(j, _)| free(j))
                        .map(|&(j, g)| g * x[j])
                        .sum::<f64>()
            } else {
                0.0
            };
        }
    };
    let dot = |x: &[f64], y: &[f64]| x.iter().zip(y).map(|(a, b)| a * b).sum::<f64>();
    let mut x = vec![0.0; n];
    let mut r = vec![0.0; n];
    r[source] = 1.0;
    let precondition = |r: &[f64]| -> Vec<f64> {
        (0..n)
            .map(|k| {
                if free(k) && diagonal[k] > 0.0 {
                    r[k] / diagonal[k]
                } else {
                    0.0
                }
            })
            .collect()
    };
    let mut z = precondition(&r);
    let mut p = z.clone();
    let mut rz = dot(&r, &z);
    let mut q = vec![0.0; n];
    let mut iterations = 0;
    let most = 20 * n + 100;
    while dot(&r, &r).sqrt() > TOLERANCE && iterations < most {
        iterations += 1;
        apply(&p, &mut q);
        let pq = dot(&p, &q);
        if pq <= 0.0 {
            break;
        }
        let alpha = rz / pq;
        for k in 0..n {
            x[k] += alpha * p[k];
            r[k] -= alpha * q[k];
        }
        z = precondition(&r);
        let next_rz = dot(&r, &z);
        let beta = next_rz / rz;
        rz = next_rz;
        for k in 0..n {
            p[k] = z[k] + beta * p[k];
        }
    }
    Some(Solved {
        volts: x,
        iterations,
    })
}

/// A number of mm to three decimals, as the tables say it.
fn mm(x: f64) -> String {
    format!("{x:.3}")
}

/// A resistance for a person: in mΩ below an ohm.
fn ohms(r: f64) -> String {
    if r < 1.0 {
        format!("{:.3} m\u{3a9}", r * 1e3)
    } else {
        format!("{r:.4} \u{3a9}")
    }
}

/// A current for a person, in A.
fn amps(i: f64) -> String {
    format!("{i:.3} A")
}

impl Resistance {
    /// The text `board resistance` prints, its places moved by `(dx, dy)` mm from the board
    /// file's frame (`--frame`).
    pub fn text_moved(&self, (dx, dy): (f64, f64)) -> String {
        use std::fmt::Write as _;
        let at = |p: (f64, f64)| format!("({}, {})", mm(p.0 + dx), mm(p.1 + dy));
        let mut out = String::new();
        let _ = writeln!(
            out,
            "net {} from {} to {}: {}",
            self.net,
            self.from.join(","),
            self.to.join(","),
            ohms(self.resistance)
        );
        match (self.current, self.drop) {
            (Some(i), Some(v)) => {
                let _ = writeln!(out, "  at {}: {:.3} mV", amps(i), v * 1e3);
                if let Some(h) = self.heating {
                    let _ = writeln!(out, "  the heat at the net's rms current, {}", amps(h));
                }
            }
            _ => {
                let _ = writeln!(
                    out,
                    "  the net's current is not known: give --amps for the drop and the heat"
                );
            }
        }
        match &self.narrowest {
            Some(n) => {
                let _ = writeln!(
                    out,
                    "  narrowest track: {} mm on {}, {} to {}, {:.1} % of the current",
                    mm(n.width),
                    n.layer,
                    at(n.from),
                    at(n.to),
                    n.share * 100.0
                );
            }
            None => {
                let _ = writeln!(out, "  no track carries the current");
            }
        }
        let _ = writeln!(out, "  vias the current passes: {}", self.vias);
        for l in &self.left_out {
            let _ = writeln!(
                out,
                "  not modelled: the net's copper area (a pour, a plane or a zone) on {l}: the figure is of its tracks and vias alone, an upper bound where the area joins the ends"
            );
        }
        let _ = writeln!(
            out,
            "  the copper the current passes ({} % of it or more), {}:",
            LISTED_SHARE * 100.0,
            match self.allowed_rise {
                Some(r) if self.current.is_some() =>
                    format!("the hottest first (IPC-2221, {r} K allowed)"),
                _ => "the largest share first".to_string(),
            }
        );
        for s in &self.segments {
            let place = match s.kind {
                Kind::Track => format!(
                    "track {} to {} on {}, {} mm wide, {} mm long",
                    at(s.from),
                    at(s.to),
                    s.layer,
                    mm(s.width),
                    mm(s.length)
                ),
                Kind::Via => format!(
                    "via at {} {}, a {} mm hole",
                    at(s.from),
                    s.layer,
                    mm(s.width)
                ),
            };
            let mut line = format!(
                "    {place}: {}, {:.1} %",
                ohms(s.resistance),
                s.share * 100.0
            );
            if let (Some(i), Some(v)) = (s.current, s.drop) {
                let _ = write!(line, ", {}, {:.3} mV", amps(i), v * 1e3);
            }
            if let Some(r) = s.rise {
                let _ = write!(line, ", rises {r:.2} K");
            }
            if let Some(w) = s.needs {
                let _ = write!(line, ", needs {} mm", mm(w));
            }
            if let Some(c) = s.carries {
                let _ = write!(line, ", carries {}", amps(c));
            }
            if s.over {
                line.push_str(", over");
            }
            let _ = writeln!(out, "{line}");
        }
        out
    }
}
