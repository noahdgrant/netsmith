//! `netsmith routes sites` (issue #1628): the places between the pads of a part where a via of
//! the process fits without breaking a design rule, and which net's copper holds each, for the
//! escape of a fine-pitch part (a BGA, an LGA, the lands of a QFN).
//!
//! **Where it looks.** The sites tried are the midpoints of the part's pairs of neighbouring
//! pads: two pads whose centres lie no further apart than [`NEIGHBOUR`] times the part's
//! least distance between two pad centres (its pitch). On a grid that is the place between two
//! pads of a row or a column and the centre of each square of four pads (the midpoint of its
//! diagonals); a place two pairs share is tried once. The pads of the part themselves are tried
//! apart, a via at each surface-mount pad's centre: a via in a pad, which the process must fill
//! and cap (`.via_in_pad`), so those sites are listed on their own.
//!
//! **What judges a site.** The via is the process's through via ([`crate::routes::via_offer`]),
//! and a site is judged by the design rules check that judges `routes add-via`
//! ([`crate::routes`]): the check of the board near the part with the via, against the same
//! check without it. A site is legal when the check finds no new error with the via on the
//! placed board with no routes copper: its clearance to the pads of other nets and to the edge,
//! its hole to other holes, the drill and ring minimums, the mask, the keepouts and rule areas,
//! and the process's own limits, as `board drc` has them. The via is of `--net` when one is
//! asked for (its own pads are then no other net's copper), or of a net no pad is on, so a free
//! site fits a via of any net of its neighbours. A site in a pad is of the pad's net (or
//! `--net`'s, whose pads alone are tried); its `via_in_pad` finding (the process fills no via
//! that wide in a pad) does not refuse it, and is said as `filled: false`.
//!
//! **Held.** A legal site is held when the check finds a new error with the via on the board
//! with the routes file's copper: a track or a via of another net (of any net, with no
//! `--net`) is too near. The tracks and vias the findings name are its holders, each with the
//! rule and the space required and measured of its finding, as `routes check-via` gives them
//! ([`crate::via_check::clash`], issue #1669). A pour, a plane or a zone holds no site: its
//! fill is made round a via, as the check makes it. The copper the router lays when `fab` runs
//! is not on the board: the sites are those of the routes file.
//!
//! **Pockets.** Each listed site says the new islands a via there would cut a pour, a plane or a
//! zone into ([`crate::via_pockets`], issue #1668): a note, which [`Sites::zone_safe`] sets
//! apart.
//!
//! The neighbours of a site are the pads of the board whose centres lie nearest it (within
//! 0.01 mm of the nearest), with their nets and the distance from centre to centre. Each check
//! is of the board cut to the part's box grown by twice the board's largest space
//! ([`crate::routes::near`]), so the work is the part's, not the board's ([`Sites::checks`]).

use std::collections::HashMap;

use netsmith_pcb::{Board, Pt, Routes, Via};

use crate::routes::{Refusal, near, via_offer, with_routes};

/// How far apart, in pitches, two pads may be and still be neighbours whose midpoint is a site:
/// more than the diagonal of a square grid (1.414), less than two pitches.
pub const NEIGHBOUR: f64 = 1.5;

/// The net of the probe via when no `--net` is asked for: a name no design gives a net.
const PROBE: &str = "(via site)";

/// A pad next to a site.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Neighbour {
    /// `REF.PAD`.
    pub pad: String,
    /// Its net, `None` for a pad on no net.
    pub net: Option<String>,
    /// From the site to the pad's centre, mm.
    pub distance: f64,
}

/// Copper of the routes file that holds a site, with the rule it breaks with the via there and
/// by how much: the figures of the check's finding, as `routes check-via` lists them
/// ([`crate::via_check::Clash`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Holder {
    /// Its net.
    pub net: String,
    /// `track` or `via`.
    pub what: String,
    /// The thing as the check names it (`Track [GND] on F.Cu, length 1.000 mm`).
    pub item: String,
    /// Where the check says it is (a track's start, a via's centre), in the routes file's frame.
    pub at: [f64; 2],
    /// The rule the via there breaks (`clearance`, `hole_clearance`, ...).
    pub rule: String,
    /// The space the rule asks for and the space measured, mm, and `required - measured`.
    pub required: Option<f64>,
    pub measured: Option<f64>,
    pub shortfall: Option<f64>,
}

/// A site between pads.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Site {
    /// In the routes file's frame (mm from the outline's top left corner, y down).
    pub at: [f64; 2],
    pub pads: Vec<Neighbour>,
    /// No copper of the routes file holds it.
    pub free: bool,
    /// What holds it, when it is held, the largest shortfall first.
    pub held_by: Vec<Holder>,
    /// The new islands a via here would cut a pour, a plane or a zone into ([`crate::via_pockets`]).
    pub pockets: Vec<crate::via_pockets::Pocket>,
}

/// A site in a pad.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct InPad {
    /// `REF.PAD`.
    pub pad: String,
    pub net: Option<String>,
    pub at: [f64; 2],
    /// The process fills and caps a via of this size in a pad: no `via_in_pad` finding.
    pub filled: bool,
    /// The `via_in_pad` finding, when the process does not fill it.
    pub why: Option<String>,
    pub free: bool,
    pub held_by: Vec<Holder>,
    pub pockets: Vec<crate::via_pockets::Pocket>,
}

/// How many sites one rule refused.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RefusedBy {
    pub rule: String,
    pub sites: usize,
}

/// The via the sites are judged for.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct ViaSize {
    pub diameter: f64,
    pub drill: f64,
}

/// What `routes sites` found ([module](self)).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Sites {
    pub board: String,
    pub part: String,
    /// The net the via is of, when one was asked for.
    pub net: Option<String>,
    pub via: ViaSize,
    /// The legal sites between the pads, from the top, then from the left.
    pub between: Vec<Site>,
    /// The legal sites in the pads, in the footprint's order.
    pub in_pad: Vec<InPad>,
    /// The sites between the pads tried.
    pub tried: usize,
    /// The sites (between pads and in pads) the check refused, by the first rule of each.
    pub refused: Vec<RefusedBy>,
    /// Whether the sites whose via would cut a fill into a new island are set apart
    /// ([`Sites::zone_safe`]).
    pub zone_safe: bool,
    /// The sites set apart, between the pads and in them, when `zone_safe`.
    pub cuts_zone: Vec<Site>,
    pub cuts_zone_in_pad: Vec<InPad>,
    /// The design rules checks made.
    pub checks: usize,
    /// The fills of the box round a site made to find the pockets ([`crate::via_pockets`]).
    pub fills: usize,
}

impl Sites {
    /// The text for a person.
    pub fn text(&self) -> String {
        self.text_moved([0.0, 0.0], "mm from the outline's top left corner, y down")
    }

    /// [`Sites::text`] with the places moved by `by`, mm, into another frame than the routes
    /// file's, which `frame` says (after `places in`).
    pub fn text_moved(&self, by: [f64; 2], frame: &str) -> String {
        let at = |p: [f64; 2]| format!("{:.3},{:.3}", p[0] + by[0], p[1] + by[1]);
        let net = |n: &Option<String>| n.clone().unwrap_or_else(|| "no net".to_string());
        let held = |h: &[Holder]| {
            h.iter()
                .map(|h| {
                    let numbers = match (h.required, h.measured, h.shortfall) {
                        (Some(r), Some(m), Some(s)) => {
                            format!(": needs {r:.4} mm, has {m:.4} mm, short by {s:.4} mm")
                        }
                        _ => String::new(),
                    };
                    format!(
                        "a {} of {} at {} ({}{numbers})",
                        h.what,
                        h.net,
                        at(h.at),
                        h.rule
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let pockets = |out: &mut String, ps: &[crate::via_pockets::Pocket]| {
            for p in ps {
                out.push_str(&format!("    a via here {}\n", p.text_moved(by)));
            }
        };
        let between_line = |out: &mut String, s: &Site| {
            let pads: Vec<String> = s
                .pads
                .iter()
                .map(|p| format!("{} [{}]", p.pad, net(&p.net)))
                .collect();
            out.push_str(&format!(
                "  {} {}  between {}",
                if s.free { "free" } else { "held" },
                at(s.at),
                pads.join(", ")
            ));
            if !s.free {
                out.push_str(&format!("  by {}", held(&s.held_by)));
            }
            if !s.pockets.is_empty() {
                out.push_str("  cuts a fill");
            }
            out.push('\n');
            pockets(out, &s.pockets);
        };
        let in_pad_line = |out: &mut String, s: &InPad| {
            out.push_str(&format!(
                "  {} {}  in {} [{}]  {}",
                if s.free { "free" } else { "held" },
                at(s.at),
                s.pad,
                net(&s.net),
                if s.filled {
                    "filled by the process"
                } else {
                    "not filled by the process"
                }
            ));
            if !s.free {
                out.push_str(&format!("  by {}", held(&s.held_by)));
            }
            if !s.pockets.is_empty() {
                out.push_str("  cuts a fill");
            }
            out.push('\n');
            pockets(out, &s.pockets);
        };
        let mut out = format!(
            "{} [{}]: {} of {} site(s) between its pads take a {:.3} mm via ({:.3} mm drill){}\n",
            self.part,
            self.board,
            self.between.len(),
            self.tried,
            self.via.diameter,
            self.via.drill,
            match &self.net {
                Some(n) => format!(" of {n}"),
                None => String::new(),
            }
        );
        for s in &self.between {
            between_line(&mut out, s);
        }
        if !self.in_pad.is_empty() {
            out.push_str(&format!(
                "{} site(s) in its pads (a via in a pad, which the process must fill and cap, `.via_in_pad`):\n",
                self.in_pad.len()
            ));
            for s in &self.in_pad {
                in_pad_line(&mut out, s);
            }
        }
        if self.zone_safe {
            let apart = self.cuts_zone.len() + self.cuts_zone_in_pad.len();
            if apart == 0 {
                out.push_str("no site's via cuts a fill into a new island (`--zone-safe`)\n");
            } else {
                out.push_str(&format!(
                    "{apart} site(s) set apart by `--zone-safe`: a via there cuts a fill into a new island\n"
                ));
                for s in &self.cuts_zone {
                    between_line(&mut out, s);
                }
                for s in &self.cuts_zone_in_pad {
                    in_pad_line(&mut out, s);
                }
            }
        }
        if !self.refused.is_empty() {
            let by: Vec<String> = self
                .refused
                .iter()
                .map(|r| format!("{} {}", r.sites, r.rule))
                .collect();
            out.push_str(&format!(
                "refused by the design rules check: {}\n",
                by.join(", ")
            ));
        }
        out.push_str(&format!(
            "places in {frame} ({} design rules check(s), {} fill(s))\n",
            self.checks, self.fills
        ));
        out
    }

    /// The sites with `zone_safe` set: those whose via would cut a fill into a new island
    /// moved out of `between` and `in_pad` into `cuts_zone` and `cuts_zone_in_pad`.
    pub fn zone_safe(mut self) -> Sites {
        let (cut, kept): (Vec<Site>, Vec<Site>) = std::mem::take(&mut self.between)
            .into_iter()
            .partition(|s| !s.pockets.is_empty());
        self.between = kept;
        self.cuts_zone = cut;
        let (cut, kept): (Vec<InPad>, Vec<InPad>) = std::mem::take(&mut self.in_pad)
            .into_iter()
            .partition(|s| !s.pockets.is_empty());
        self.in_pad = kept;
        self.cuts_zone_in_pad = cut;
        self.zone_safe = true;
        self
    }
}

/// The sites of the part `refdes` of `board` (placed, with no routes on it) with the copper of
/// `routes`, for a via of `net` (or of no net of the board), as the [module](self) says.
/// Refused when the board has no such part or net.
pub fn sites(
    board: &Board,
    routes: &Routes,
    refdes: &str,
    net: Option<&str>,
) -> Result<Sites, Refusal> {
    let refuse = |e: String| Refusal::Refused(e);
    let part = board
        .part(refdes)
        .ok_or_else(|| refuse(format!("the board {} has no part {refdes}", board.name)))?;
    if let Some(n) = net
        && !board.nets.iter().any(|b| b.name == n)
    {
        return Err(refuse(format!("the board {} has no net `{n}`", board.name)));
    }
    let offer = via_offer(board, None)?;
    let via_net = net.unwrap_or(PROBE).to_string();
    let net_of: HashMap<(&str, &str), &str> = board
        .nets
        .iter()
        .flat_map(|n| {
            n.pads
                .iter()
                .map(move |p| ((p.refdes.as_str(), p.number.as_str()), n.name.as_str()))
        })
        .collect();

    // The boards the checks are made of: near the part, with no zones (a fill is made round a
    // via), with and without the routes file's copper.
    let (routed, _) = with_routes(board, routes)?;
    let reach = near::reach(board).max(near::reach(&routed));
    let hood = near::part_box(part).expanded(2.0 * reach);
    let cut = |whole: &Board| {
        let mut b = near::local(whole, &[], &[hood.expanded(reach)]);
        b.zones.clear();
        if net.is_none() {
            b.nets.push(netsmith_pcb::BoardNet {
                name: PROBE.to_string(),
                pads: Vec::new(),
            });
        }
        b
    };
    let bare = cut(board);
    let copper = cut(&routed);
    let mut checks = 0;
    let errors =
        |b: &Board, checks: &mut usize| -> Result<Vec<netsmith_geom::DrcFinding>, String> {
            *checks += 1;
            crate::routes::drc_findings_with(
                b,
                &netsmith_pcb::kicad_pro(board),
                &crate::fab::drc_extras(board),
                false,
            )
        };
    let bare_before = errors(&bare, &mut checks)?;
    let copper_before = errors(&copper, &mut checks)?;
    // The findings `with` the via at `at` has that `before` does not, near the via.
    let new_near = |base: &Board,
                    before: &[netsmith_geom::DrcFinding],
                    at: Pt,
                    of: &str,
                    checks: &mut usize|
     -> Result<Vec<netsmith_geom::DrcFinding>, String> {
        let mut b = base.clone();
        b.vias.push(Via {
            net: of.to_string(),
            at,
            diameter: offer.diameter,
            drill: offer.drill,
            span: None,
        });
        let mut old: Vec<String> = before.iter().map(ToString::to_string).collect();
        let mut new = Vec::new();
        for f in errors(&b, checks)? {
            let place = Pt::new(f.at.x, f.at.y) + board.outline.min;
            if place.distance(at) > offer.diameter + reach {
                continue;
            }
            match old.iter().position(|o| *o == f.to_string()) {
                Some(k) => {
                    old.swap_remove(k);
                }
                None => new.push(f),
            }
        }
        Ok(new)
    };
    // The tracks and vias of other nets that the findings of the via at `at` name, each with
    // the rule and the figures of its finding as `routes check-via` has them.
    let holders = |found: &[netsmith_geom::DrcFinding], of: &str, at: Pt| {
        let mut out: Vec<Holder> = Vec::new();
        for f in found {
            let c = crate::via_check::clash(f, at);
            for i in f.items.iter().filter(|i| i.part.is_none()) {
                let what = if i.what.starts_with("Track") || i.what.starts_with("Arc") {
                    "track"
                } else if i.what.starts_with("Via") {
                    "via"
                } else {
                    continue;
                };
                let held_net = i.net.clone().unwrap_or_default();
                if held_net == of {
                    continue;
                }
                let p = Pt::new(i.at.x, i.at.y) - board.outline.min;
                let h = Holder {
                    net: held_net,
                    what: what.to_string(),
                    item: i.what.clone(),
                    at: [round(p.x), round(p.y)],
                    rule: c.rule.clone(),
                    required: c.required,
                    measured: c.measured,
                    shortfall: c.shortfall,
                };
                match out
                    .iter_mut()
                    .find(|o| o.item == h.item && o.at == h.at && o.rule == h.rule)
                {
                    Some(o) if o.shortfall < h.shortfall => *o = h,
                    Some(_) => {}
                    None => out.push(h),
                }
            }
        }
        out.sort_by(|a, b| {
            b.shortfall
                .unwrap_or(f64::INFINITY)
                .total_cmp(&a.shortfall.unwrap_or(f64::INFINITY))
                .then(a.net.cmp(&b.net))
                .then(a.item.cmp(&b.item))
        });
        out
    };
    let mut pockets = crate::via_pockets::Pockets::new();
    let probe = |at: Pt, of: &str| Via {
        net: of.to_string(),
        at,
        diameter: offer.diameter,
        drill: offer.drill,
        span: None,
    };
    let mut refused: Vec<RefusedBy> = Vec::new();
    let mut refuse_by = |found: &[netsmith_geom::DrcFinding]| {
        let rule = found[0].rule.id().to_string();
        match refused.iter_mut().find(|r| r.rule == rule) {
            Some(r) => r.sites += 1,
            None => refused.push(RefusedBy { rule, sites: 1 }),
        }
    };
    let file_place = |p: Pt| {
        let p = p - board.outline.min;
        [p.x, p.y]
    };

    // The pads of the board near the part: the neighbours of a site.
    let pads: Vec<(String, Option<String>, Pt)> = board
        .parts
        .iter()
        .filter(|p| near::meets(&near::part_box(p), &hood))
        .flat_map(|p| {
            let net_of = &net_of;
            p.footprint.pads.iter().map(move |pad| {
                (
                    format!("{}.{}", p.refdes, pad.number),
                    net_of
                        .get(&(p.refdes.as_str(), pad.number.as_str()))
                        .map(|n| n.to_string()),
                    p.pad_centre(pad),
                )
            })
        })
        .collect();
    let neighbours = |at: Pt| {
        let nearest = pads
            .iter()
            .map(|(_, _, c)| c.distance(at))
            .fold(f64::INFINITY, f64::min);
        pads.iter()
            .filter(|(_, _, c)| c.distance(at) <= nearest + 0.01)
            .map(|(pad, net, c)| Neighbour {
                pad: pad.clone(),
                net: net.clone(),
                distance: round(c.distance(at)),
            })
            .collect::<Vec<_>>()
    };

    // The sites between the part's pads.
    let centres: Vec<Pt> = part
        .footprint
        .pads
        .iter()
        .map(|p| part.pad_centre(p))
        .collect();
    let pitch = centres
        .iter()
        .enumerate()
        .flat_map(|(i, a)| centres[i + 1..].iter().map(move |b| a.distance(*b)))
        .filter(|d| *d > 1e-6)
        .fold(f64::INFINITY, f64::min);
    let mut candidates: Vec<Pt> = Vec::new();
    for (i, a) in centres.iter().enumerate() {
        for b in &centres[i + 1..] {
            let d = a.distance(*b);
            if d > 1e-6 && d <= NEIGHBOUR * pitch + 1e-6 {
                let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                if !candidates.iter().any(|c| c.distance(mid) < 1e-4) {
                    candidates.push(mid);
                }
            }
        }
    }
    candidates.sort_by(|a, b| {
        (round(a.y), round(a.x))
            .partial_cmp(&(round(b.y), round(b.x)))
            .expect("places are numbers")
    });
    let mut between = Vec::new();
    for &at in &candidates {
        let found = new_near(&bare, &bare_before, at, &via_net, &mut checks)?;
        if !found.is_empty() {
            refuse_by(&found);
            continue;
        }
        let found = new_near(&copper, &copper_before, at, &via_net, &mut checks)?;
        let held_by = holders(&found, &via_net, at);
        between.push(Site {
            at: file_place(at),
            pads: neighbours(at),
            free: found.is_empty(),
            held_by,
            pockets: pockets.of_via(&routed, &probe(at, &via_net))?,
        });
    }

    // The sites in the part's pads.
    let mut in_pad = Vec::new();
    for pad in &part.footprint.pads {
        let (front, back) = part.pad_layers(pad);
        if pad.drill.is_some() || front == back || pad.number.is_empty() {
            continue;
        }
        let pad_net = net_of
            .get(&(part.refdes.as_str(), pad.number.as_str()))
            .map(|n| n.to_string());
        let of = match (net, &pad_net) {
            (Some(n), Some(p)) if n == p => n.to_string(),
            (Some(_), _) => continue,
            (None, Some(p)) => p.clone(),
            (None, None) => PROBE.to_string(),
        };
        let at = part.pad_centre(pad);
        let found = new_near(&bare, &bare_before, at, &of, &mut checks)?;
        let (in_pad_rule, others): (Vec<_>, Vec<_>) = found
            .into_iter()
            .partition(|f| f.rule == netsmith_geom::DrcRule::ViaInPad);
        if !others.is_empty() {
            refuse_by(&others);
            continue;
        }
        let held = new_near(&copper, &copper_before, at, &of, &mut checks)?;
        let held: Vec<_> = held
            .into_iter()
            .filter(|f| f.rule != netsmith_geom::DrcRule::ViaInPad)
            .collect();
        in_pad.push(InPad {
            pad: format!("{}.{}", part.refdes, pad.number),
            net: pad_net,
            at: file_place(at),
            filled: in_pad_rule.is_empty(),
            why: in_pad_rule.first().map(|f| f.message.clone()),
            free: held.is_empty(),
            held_by: holders(&held, &of, at),
            pockets: pockets.of_via(&routed, &probe(at, &of))?,
        });
    }
    refused.sort_by(|a, b| b.sites.cmp(&a.sites).then(a.rule.cmp(&b.rule)));
    Ok(Sites {
        board: board.name.clone(),
        part: refdes.to_string(),
        net: net.map(str::to_string),
        via: ViaSize {
            diameter: offer.diameter,
            drill: offer.drill,
        },
        between,
        in_pad,
        tried: candidates.len(),
        refused,
        zone_safe: false,
        cuts_zone: Vec::new(),
        cuts_zone_in_pad: Vec::new(),
        checks,
        fills: pockets.fills,
    })
}

/// To 0.0001 mm.
fn round(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
