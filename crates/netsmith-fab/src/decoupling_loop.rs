//! The decoupling loop of each capacitor of a board (issue #1730): what `board nets
//! --decoupling` lists, geometry only, so a move of a capacitor can be compared before and
//! after. No inductance is estimated.
//!
//! Which capacitors and which pins is the caller's ([`Decoupled`]): the board's decoupling pairs
//! (`netsmith_pcb::decoupling_pairs`, bulk capacitors of a rail among them), each with its supply
//! net and its return net. This measures, on the board file's copper:
//!
//! - the **pin it serves**: of the decoupled parts' pins on the supply net, the one whose pad's
//!   centre is nearest the capacitor's supply pad; and the **return pin**: of that part's pins on
//!   the return net, the one nearest the capacitor's return pad;
//! - each **leg** (supply, from the capacitor's pad to the pin it serves; return, from the pin
//!   back to the capacitor's other pad): the distance centre to centre, and the routed length,
//!   the narrowest track and the vias of the shortest way through the tracks of the net between
//!   the two pads, joined as `board nets` joins a connection ([`crate::connection`]). A pour or a
//!   plane carries current but has no path: a leg that only a pour or a plane joins (or that is
//!   open) has no way, and the loop says it is not complete;
//! - the **loop**: the two ways' lengths and vias added, and the narrowest of both;
//! - the **return copper**: of the copper layers with a zone of the return net over the
//!   capacitor's return pad's centre, the nearest to the capacitor's own outer layer, and the
//!   height between the two through the stackup (copper centre to copper centre, as `board
//!   resistance` measures a via's barrel; 0 for a pour on the capacitor's own layer).

use netsmith_geom::{Artwork, polygon_contains};
use serde::Serialize;

use crate::connection;
use crate::net_table::{Seg, number};

/// A capacitor of a decoupling pair, as the caller recognised it.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoupled {
    /// The capacitor's reference designator and its path in the design.
    pub cap: String,
    pub path: String,
    pub supply: Side,
    /// The net the current goes back on: the pair's net with a pour or a plane, else the one with
    /// more pads.
    pub ret: Side,
}

/// One net of a decoupling pair.
#[derive(Debug, Clone, PartialEq)]
pub struct Side {
    pub net: String,
    /// The capacitor's pad on the net.
    pub cap_pad: String,
    /// The decoupled parts' pads on the net, as (reference designator, pad number).
    pub pins: Vec<(String, String)>,
}

/// The loop of one capacitor. Lengths and distances in mm.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DecouplingLoop {
    pub cap: String,
    pub path: String,
    /// The part it decouples (that of the pin it serves).
    pub part: String,
    /// The capacitor's centre (the middle of its two pads), in the board file's frame.
    pub at: (f64, f64),
    pub supply: LoopLeg,
    /// `None` where the part has no pin on the return net.
    #[serde(rename = "return")]
    pub ret: Option<LoopLeg>,
    #[serde(rename = "loop")]
    pub total: LoopTotal,
    /// The nearest copper of the return net under the capacitor; `None` where none is.
    pub return_copper: Option<ReturnCopper>,
}

/// One leg of a loop.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoopLeg {
    pub net: String,
    /// The two pads (`C1.1`, `U1.4`): the capacitor's and the pin's on the supply leg, the pin's
    /// and the capacitor's on the return leg.
    pub from: String,
    pub to: String,
    /// Centre to centre.
    pub distance: f64,
    /// Along the shortest way through the net's tracks; `None` where no track joins the two
    /// pads (a pour or a plane carries the net, or it is open).
    pub routed: Option<f64>,
    /// The way's narrowest track; `None` with no way, or a way of no track.
    pub narrowest: Option<f64>,
    /// The vias at which the way changes layer.
    pub vias: Option<usize>,
}

/// Both legs together.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoopTotal {
    /// The routed lengths of the legs that have a way, added.
    pub length: f64,
    pub narrowest: Option<f64>,
    pub vias: usize,
    /// Whether both legs have a way through tracks: else `length` and `vias` are of the legs
    /// that do.
    pub complete: bool,
}

/// The nearest zone of the return net under the capacitor.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReturnCopper {
    pub net: String,
    pub layer: String,
    /// The height from the capacitor's layer through the stackup, copper centre to copper
    /// centre: 0 for a pour on its own layer.
    pub distance: f64,
}

/// The loops of `caps` on `art`, in the order of `caps`; a capacitor with no pad of its supply
/// net, or none of its parts' pins on it, on the board is left out.
pub fn of(art: &Artwork, caps: &[Decoupled]) -> Vec<DecouplingLoop> {
    let segs = crate::score::lines(art);
    let heights = crate::resistance::heights(art);
    caps.iter()
        .filter_map(|c| one(art, &segs, &heights, c))
        .collect()
}

/// The index into `art.pads` of the terminal pad `number` of `part` on `net`.
fn pad(art: &Artwork, part: &str, number: &str, net: &str) -> Option<usize> {
    (0..art.pads.len()).find(|&i| {
        let p = &art.pads[i];
        p.is_terminal() && p.part == part && p.number == number && p.net.as_deref() == Some(net)
    })
}

/// The nearest of `pins` (as (refdes, number) on `net`) to the pad `to`, as its index.
fn nearest(art: &Artwork, net: &str, pins: &[(String, String)], to: usize) -> Option<usize> {
    pins.iter()
        .filter_map(|(r, n)| pad(art, r, n, net))
        .min_by(|&a, &b| {
            let d = |i: usize| art.pads[i].at.dist(art.pads[to].at);
            d(a).total_cmp(&d(b))
        })
}

fn leg(art: &Artwork, segs: &[Seg], net: &str, from: usize, to: usize) -> LoopLeg {
    let way = connection::way(art, segs, net, &[from], &[to]);
    let name = |i: usize| format!("{}.{}", art.pads[i].part, art.pads[i].number);
    LoopLeg {
        net: net.to_string(),
        from: name(from),
        to: name(to),
        distance: art.pads[from].at.dist(art.pads[to].at),
        routed: way.map(|w| w.length),
        narrowest: way.and_then(|w| w.narrowest),
        vias: way.map(|w| w.vias),
    }
}

fn one(art: &Artwork, segs: &[Seg], heights: &[f64], c: &Decoupled) -> Option<DecouplingLoop> {
    let cap_supply = pad(art, &c.cap, &c.supply.cap_pad, &c.supply.net)?;
    let pin = nearest(art, &c.supply.net, &c.supply.pins, cap_supply)?;
    let part = art.pads[pin].part.clone();
    let supply = leg(art, segs, &c.supply.net, cap_supply, pin);
    let cap_return = pad(art, &c.cap, &c.ret.cap_pad, &c.ret.net);
    let own: Vec<(String, String)> = c
        .ret
        .pins
        .iter()
        .filter(|(r, _)| *r == part)
        .cloned()
        .collect();
    let ret = cap_return.and_then(|cr| {
        let back = nearest(art, &c.ret.net, &own, cr)?;
        Some(leg(art, segs, &c.ret.net, back, cr))
    });
    let legs: Vec<&LoopLeg> = std::iter::once(&supply).chain(ret.as_ref()).collect();
    let total = LoopTotal {
        length: legs.iter().filter_map(|l| l.routed).sum(),
        narrowest: legs
            .iter()
            .filter_map(|l| l.narrowest)
            .min_by(f64::total_cmp),
        vias: legs.iter().filter_map(|l| l.vias).sum(),
        complete: ret.is_some() && legs.iter().all(|l| l.routed.is_some()),
    };
    let centre = match cap_return {
        Some(cr) => {
            let (a, b) = (art.pads[cap_supply].at, art.pads[cr].at);
            ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
        }
        None => (art.pads[cap_supply].at.x, art.pads[cap_supply].at.y),
    };
    let return_copper =
        cap_return.and_then(|cr| return_copper(art, heights, &c.cap, &c.ret.net, cr));
    Some(DecouplingLoop {
        cap: c.cap.clone(),
        path: c.path.clone(),
        part,
        at: centre,
        supply,
        ret,
        total,
        return_copper,
    })
}

/// The nearest layer to the capacitor's own with a zone of `net` over the pad `under`.
fn return_copper(
    art: &Artwork,
    heights: &[f64],
    cap: &str,
    net: &str,
    under: usize,
) -> Option<ReturnCopper> {
    let back = art
        .footprints
        .iter()
        .find(|f| f.part == cap)
        .is_some_and(|f| f.back);
    let side = if back {
        art.copper.len().saturating_sub(1)
    } else {
        0
    };
    let at = art.pads[under].at;
    let height = |l: usize| match (heights.get(l), heights.get(side)) {
        (Some(a), Some(b)) => (a - b).abs(),
        _ => f64::MAX,
    };
    art.zones
        .iter()
        .filter(|z| z.keepout.is_none() && !z.teardrop && z.net.as_deref() == Some(net))
        .filter(|z| z.outlines.iter().any(|o| polygon_contains(o, at)))
        .flat_map(|z| z.layers.iter())
        .filter_map(|l| art.copper.iter().position(|c| c == l))
        .min_by(|&a, &b| height(a).total_cmp(&height(b)).then(a.cmp(&b)))
        .map(|l| ReturnCopper {
            net: net.to_string(),
            layer: art.copper[l].clone(),
            distance: height(l),
        })
}

/// The text of `board nets --decoupling`, its places moved by `(dx, dy)` mm.
pub fn text(loops: &[DecouplingLoop], (dx, dy): (f64, f64)) -> String {
    if loops.is_empty() {
        return "decoupling loops: no decoupling capacitor\n".to_string();
    }
    let mut out = String::from(
        "decoupling loops (geometry only; a pour or a plane carries current but has no path, so a leg it joins has no track):\n",
    );
    let leg = |l: &LoopLeg| match (l.routed, l.vias) {
        (Some(len), Some(v)) => format!(
            "{} to {} on {}, {} mm apart, routed {} mm, narrowest {}, {} via(s)",
            l.from,
            l.to,
            l.net,
            number(l.distance),
            number(len),
            l.narrowest
                .map_or("-".to_string(), |w| format!("{} mm", number(w))),
            v
        ),
        _ => format!(
            "{} to {} on {}, {} mm apart, no track (a pour or a plane, or open)",
            l.from,
            l.to,
            l.net,
            number(l.distance)
        ),
    };
    for c in loops {
        out.push_str(&format!(
            "  {} ({}) at ({}, {}) decouples {}:\n",
            c.cap,
            c.path,
            number(c.at.0 + dx),
            number(c.at.1 + dy),
            c.part
        ));
        out.push_str(&format!("    supply: {}\n", leg(&c.supply)));
        match &c.ret {
            Some(r) => out.push_str(&format!("    return: {}\n", leg(r))),
            None => out.push_str(&format!("    return: {} has no pin on it\n", c.part)),
        }
        let t = &c.total;
        out.push_str(&format!(
            "    loop: routed {} mm{}, narrowest {}, {} via(s)\n",
            number(t.length),
            if t.complete {
                ""
            } else {
                " (not complete: a leg has no track)"
            },
            t.narrowest
                .map_or("-".to_string(), |w| format!("{} mm", number(w))),
            t.vias
        ));
        match &c.return_copper {
            Some(p) => out.push_str(&format!(
                "    return copper: {} on {}, {} mm from the capacitor's layer\n",
                p.net,
                p.layer,
                number(p.distance)
            )),
            None => out.push_str("    return copper: no zone of the return net under it\n"),
        }
    }
    out
}

/// A loop's figures that `--compare` sets side by side.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, serde::Deserialize)]
pub struct Figures {
    pub length: f64,
    pub narrowest: Option<f64>,
    pub vias: usize,
    pub complete: bool,
    /// The return copper's distance.
    pub return_copper: Option<f64>,
}

impl DecouplingLoop {
    pub fn figures(&self) -> Figures {
        Figures {
            length: self.total.length,
            narrowest: self.total.narrowest,
            vias: self.total.vias,
            complete: self.total.complete,
            return_copper: self.return_copper.as_ref().map(|p| p.distance),
        }
    }
}

/// One capacitor's loop before and after.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ComparedLoop {
    pub cap: String,
    /// `None` where the capacitor is not in that state.
    pub before: Option<Figures>,
    pub after: Option<Figures>,
    /// After less before, of the length (mm) and the vias, where both are known.
    pub length_change: Option<f64>,
    pub vias_change: Option<i64>,
}

/// The loops of `after` and those of `before` (by capacitor), capacitor by capacitor: those of
/// `after` in its order, then those only `before` has.
pub fn compare(before: &[(String, Figures)], after: &[DecouplingLoop]) -> Vec<ComparedLoop> {
    let mut out: Vec<ComparedLoop> = after
        .iter()
        .map(|a| {
            let f = a.figures();
            let b = before.iter().find(|(c, _)| *c == a.cap).map(|(_, f)| *f);
            ComparedLoop {
                cap: a.cap.clone(),
                before: b,
                after: Some(f),
                length_change: b.map(|b| f.length - b.length),
                vias_change: b.map(|b| f.vias as i64 - b.vias as i64),
            }
        })
        .collect();
    for (c, b) in before {
        if !after.iter().any(|a| a.cap == *c) {
            out.push(ComparedLoop {
                cap: c.clone(),
                before: Some(*b),
                after: None,
                length_change: None,
                vias_change: None,
            });
        }
    }
    out
}

/// The text of `--decoupling --compare`: a line for each capacitor.
pub fn compare_text(against: &str, rows: &[ComparedLoop]) -> String {
    let mut out = format!("decoupling loops compared with {against} (before -> after):\n");
    let fig =
        |f: Option<Figures>, pick: &dyn Fn(Figures) -> String| f.map_or("-".to_string(), pick);
    let mm = |x: Option<f64>| x.map_or("-".to_string(), number);
    for r in rows {
        let signed = |x: f64| {
            if x > 0.0 {
                format!("+{}", number(x))
            } else {
                number(x)
            }
        };
        out.push_str(&format!(
            "  {}: loop {} -> {} mm ({}), vias {} -> {} ({}), narrowest {} -> {} mm, return copper {} -> {} mm\n",
            r.cap,
            fig(r.before, &|f| format!(
                "{}{}",
                number(f.length),
                if f.complete { "" } else { "*" }
            )),
            fig(r.after, &|f| format!(
                "{}{}",
                number(f.length),
                if f.complete { "" } else { "*" }
            )),
            r.length_change.map_or("-".to_string(), signed),
            fig(r.before, &|f| f.vias.to_string()),
            fig(r.after, &|f| f.vias.to_string()),
            r.vias_change.map_or("-".to_string(), |v| signed(v as f64)),
            fig(r.before, &|f| mm(f.narrowest)),
            fig(r.after, &|f| mm(f.narrowest)),
            fig(r.before, &|f| mm(f.return_copper)),
            fig(r.after, &|f| mm(f.return_copper)),
        ));
    }
    if rows
        .iter()
        .any(|r| [r.before, r.after].iter().flatten().any(|f| !f.complete))
    {
        out.push_str(
            "  (* a loop not complete: a leg has no track, so its length is of the other)\n",
        );
    }
    if rows.is_empty() {
        out.push_str("  no decoupling capacitor\n");
    }
    out
}
