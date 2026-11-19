//! `netsmith routes join-pad` (issue #1293): a short track, or a track and a via, from a pad into
//! the fill of its net, for a pad the fill leaves unjoined (a thermal relief whose spokes the
//! clearance of other nets cuts, or starves to one), written to the routes file as the other
//! `routes` commands write it.
//!
//! **Which pad needs it.** The fill is the one `board fill` reports ([`FillReport`], with the
//! routes file's copper on the board): a pad a fill of its net lists is joined when the fill
//! joins it (a spoke or a track, not starved) or another fill of its net does; a pad no fill
//! lists is joined when the design rules check finds it connected (no `unconnected_items`
//! names it). A joined pad is said, and nothing is written.
//!
//! **Where it goes.** A track on a layer of the pad that a fill of its net is on, from the pad's
//! centre straight out in one of [`DIRECTIONS`] directions, to the first place, a step of
//! [`STEP`] at a time and no further than [`MOST_REACH`], where its round end (the net's own
//! width on the layer) lies wholly on that fill's copper (the outline `board fill` gives). The
//! shortest such track is the direction with the most room; the others follow, shortest first.
//! When no track fits, a track from the pad to a through via (the process's), the via's place on
//! the fill of its net on another layer, clear of the pad by the board's clearance.
//!
//! **Checked.** Each try is an edit of the routes file ([`crate::routes::edit_all`]): refused,
//! as `routes add-track` refuses it, when the design rules check finds a new error with it (a
//! clearance to another net's copper, a short). A try the check takes is kept only when the pad
//! is joined after it, by the rule above (with a via, when the check finds the pad connected).
//! At most [`MOST_TRIES`] tries are made, and when none is kept the refusal says how many were
//! tried and why the shortest was refused. The track and via are unlocked.

use netsmith_pcb::{Board, CopperLayer, FillPad, FillReport, PadRef, Pt, Routes};

use crate::routes::{Edit, Refusal, edit_all, via_offer, with_routes};

/// How many directions a track is tried in: every 22.5 degrees.
pub const DIRECTIONS: usize = 16;
/// How far from the pad's centre a track or via may go, in mm.
pub const MOST_REACH: f64 = 5.0;
/// The step along a direction, in mm.
pub const STEP: f64 = 0.025;
/// The most tries checked by the design rules check.
pub const MOST_TRIES: usize = 24;

/// The track (and via) that joins the pad, in the routes file's frame (mm from the outline's top
/// left corner, y down).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Join {
    /// The layer of the track.
    pub layer: String,
    pub from: [f64; 2],
    pub to: [f64; 2],
    pub width: f64,
    pub length: f64,
    /// The direction from the pad, in degrees counterclockwise on screen from the right (+x).
    pub direction: f64,
    /// The via at `to`, when the fill is on another layer, and that layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_layer: Option<String>,
}

/// A try that was not kept, and why.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Tried {
    pub layer: String,
    pub direction: f64,
    pub length: f64,
    pub via: bool,
    pub why: String,
}

/// What `routes join-pad` did ([module](self)).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Joined {
    pub board: String,
    pub pad: String,
    pub net: String,
    /// How the fill had the pad: `unjoinable on top: ...`, `starved on top: 1 of 4 spokes`.
    pub before: String,
    /// The pad was joined already: nothing to do.
    pub already: bool,
    pub join: Option<Join>,
    /// The tries refused or that left the pad unjoined, in the order tried.
    pub tried: Vec<Tried>,
    /// Why nothing joins it, when nothing does.
    pub refused: Option<String>,
    /// What the edit said, when it was made.
    pub said: Option<String>,
    /// The routes file with the join, when there is one.
    #[serde(skip)]
    pub routes: Option<Routes>,
}

impl Joined {
    /// The text for a person.
    pub fn text(&self) -> String {
        let mut out = format!("{} [{}]: {}\n", self.pad, self.net, self.before);
        if self.already {
            out.push_str("it is joined already: nothing to add\n");
        }
        if let Some(j) = &self.join {
            let at = |p: [f64; 2]| format!("{:.3},{:.3}", p[0], p[1]);
            out.push_str(&format!(
                "a {:.3} mm track on {} from {} to {} ({:.3} mm, {:.1} degrees)",
                j.width,
                j.layer,
                at(j.from),
                at(j.to),
                j.length,
                j.direction
            ));
            match (&j.via, &j.fill_layer) {
                (Some(v), Some(l)) => {
                    out.push_str(&format!(" and a via at {} into the fill on {l}\n", at(*v)))
                }
                _ => out.push_str(&format!(" into the fill on {}\n", j.layer)),
            }
            if !self.tried.is_empty() {
                out.push_str(&format!(
                    "{} shorter try(s) refused first\n",
                    self.tried.len()
                ));
            }
        }
        out
    }
}

/// The pad `name` (`REF.PAD`) of `board` (placed, with no routes on it) joined to the fill of its
/// net by a short track or via added to `routes`, as the [module](self) says. Refused when the
/// board has no such pad, it is on no net, or its net has no fill.
pub fn join_pad(board: &Board, routes: &Routes, name: &str) -> Result<Joined, Refusal> {
    let refuse = |e: String| Refusal::Refused(e);
    let (refdes, number) = name
        .split_once('.')
        .ok_or_else(|| refuse(format!("`{name}` is not a pad, `REF.PAD` (`C1.2`)")))?;
    let part = board
        .parts
        .iter()
        .find(|p| p.refdes == refdes)
        .ok_or_else(|| refuse(format!("the board {} has no part {refdes}", board.name)))?;
    let pad = part.footprint.pads_numbered(number).next().ok_or_else(|| {
        refuse(format!(
            "{refdes} has no pad {number}: its pads are {}",
            part.footprint.pad_numbers().join(", ")
        ))
    })?;
    let at = PadRef {
        refdes: refdes.to_string(),
        number: number.to_string(),
    };
    let net = board
        .nets
        .iter()
        .find(|n| n.pads.contains(&at))
        .map(|n| n.name.clone())
        .ok_or_else(|| refuse(format!("pad {name} is on no net: no fill joins it")))?;

    let (routed, _) = with_routes(board, routes)?;
    let report = FillReport::of(&routed).map_err(|e| Refusal::Failed(e.to_string()))?;
    let fills: Vec<&netsmith_pcb::FillArea> =
        report.fills.iter().filter(|f| f.net == net).collect();
    if fills.is_empty() {
        return Err(refuse(format!(
            "net {net} has no pour, plane or zone: there is no fill to join {name} to (`netsmith routes route {net}` routes it)"
        )));
    }
    let before = state(&report, &routed, &net, refdes, number)?;
    let mut joined = Joined {
        board: board.name.clone(),
        pad: name.to_string(),
        net: net.clone(),
        before: before.text.clone(),
        already: before.joined,
        join: None,
        tried: Vec::new(),
        refused: None,
        said: None,
        routes: None,
    };
    if before.joined {
        return Ok(joined);
    }

    // The pad's layers a track can be drawn on.
    let centre = part.pad_centre(pad);
    let bounds = part.pad_bounds(pad);
    let (front, back) = part.pad_layers(pad);
    let mut layers: Vec<CopperLayer> = Vec::new();
    if front {
        layers.push(CopperLayer::Front);
    }
    if back {
        layers.push(CopperLayer::Back);
    }
    if front && back {
        layers.extend(board.signal_layers().into_iter().filter(|l| !l.is_outer()));
    }
    let room = |layer: &str, p: Pt, r: f64| {
        fills.iter().filter(|f| f.layer == layer).any(|f| {
            f.outline
                .iter()
                .any(|poly| poly.contains(p) && poly.distance_to_edge(p) >= r)
        })
    };
    let ways: Vec<(f64, Pt)> = (0..DIRECTIONS)
        .map(|k| {
            let a = k as f64 * 360.0 / DIRECTIONS as f64;
            let r = a.to_radians();
            (a, Pt::new(r.cos(), -r.sin()))
        })
        .collect();
    let steps = (MOST_REACH / STEP).round() as usize;
    // (length, layer, direction, unit, via's fill layer).
    let mut tracks: Vec<(f64, CopperLayer, f64, Pt, Option<String>)> = Vec::new();
    for &layer in &layers {
        let width = board.track_width_on(&net, layer);
        for &(a, u) in &ways {
            if let Some(t) = (1..=steps)
                .map(|i| i as f64 * STEP)
                .find(|t| room(layer.word(), along(centre, u, *t), width / 2.0))
            {
                tracks.push((t, layer, a, u, None));
            }
        }
    }
    tracks.sort_by(|a, b| a.0.total_cmp(&b.0));
    let via = via_offer(board, None)?;
    let mut vias: Vec<(f64, CopperLayer, f64, Pt, Option<String>)> = Vec::new();
    let others: Vec<&str> = {
        let mut o: Vec<&str> = fills
            .iter()
            .map(|f| f.layer.as_str())
            .filter(|l| !layers.iter().any(|x| x.word() == *l))
            .collect();
        o.dedup();
        o
    };
    if let Some(&layer) = layers.first() {
        for &(a, u) in &ways {
            let clear = exit(&bounds, centre, u) + via.diameter / 2.0 + board.rules.clearance;
            let found = (0..=steps)
                .map(|i| clear + i as f64 * STEP)
                .take_while(|t| *t <= MOST_REACH)
                .find_map(|t| {
                    let p = along(centre, u, t);
                    others
                        .iter()
                        .find(|l| room(l, p, via.diameter / 2.0))
                        .map(|l| (t, l.to_string()))
                });
            if let Some((t, l)) = found {
                vias.push((t, layer, a, u, Some(l)));
            }
        }
    }
    vias.sort_by(|a, b| a.0.total_cmp(&b.0));

    let origin = board.outline.min;
    let here = |p: Pt| {
        let p = p - origin;
        let um = |v: f64| (v * 1e6).round() / 1e6;
        [um(p.x), um(p.y)]
    };
    for (t, layer, a, u, fill_layer) in tracks.into_iter().chain(vias).take(MOST_TRIES) {
        let end = along(centre, u, t);
        let mut edits = vec![Edit::AddTrack {
            net: net.clone(),
            layer,
            width: None,
            from: here(centre),
            mid: None,
            to: here(end),
            lock: false,
        }];
        if fill_layer.is_some() {
            edits.push(Edit::AddVia {
                net: net.clone(),
                at: here(end),
                diameter: None,
                drill: None,
                layers: None,
                lock: false,
            });
        }
        let mut next = routes.clone();
        let tried = |why: String| Tried {
            layer: layer.word().to_string(),
            direction: a,
            length: (t * 1e6).round() / 1e6,
            via: fill_layer.is_some(),
            why,
        };
        match edit_all(board, &mut next, &edits) {
            Err(Refusal::Refused(e)) => {
                joined.tried.push(tried(e));
                continue;
            }
            Err(e) => return Err(e),
            Ok(said) => {
                let (after, _) = with_routes(board, &next)?;
                let report = FillReport::of(&after).map_err(|e| Refusal::Failed(e.to_string()))?;
                let now = state(&report, &after, &net, refdes, number)?;
                let done = if fill_layer.is_some() {
                    !now.unconnected
                } else {
                    now.joined
                };
                if !done {
                    joined.tried.push(tried(format!(
                        "the pad is still not joined with it: {}",
                        now.text
                    )));
                    continue;
                }
                joined.join = Some(Join {
                    layer: layer.word().to_string(),
                    from: here(centre),
                    to: here(end),
                    width: board.track_width_on(&net, layer),
                    length: (t * 1e6).round() / 1e6,
                    direction: a,
                    via: fill_layer.as_ref().map(|_| here(end)),
                    fill_layer,
                });
                joined.said = Some(said);
                joined.routes = Some(next);
                return Ok(joined);
            }
        }
    }
    let nearest = match joined.tried.first() {
        Some(t) => format!(
            "; the shortest, {:.3} mm on {} at {:.1} degrees{}, was refused: {}",
            t.length,
            t.layer,
            t.direction,
            if t.via { " with a via" } else { "" },
            t.why
        ),
        None => String::new(),
    };
    joined.refused = Some(format!(
        "no track of net {net} from {name} reaches the fill of its net within {MOST_REACH} mm, nor a track and a via: {} tried{nearest}; move the part, give it room, or join it by hand (`netsmith routes add-track`)",
        joined.tried.len()
    ));
    Ok(joined)
}

/// How the fill has the pad.
struct State {
    joined: bool,
    /// The design rules check finds the pad unconnected.
    unconnected: bool,
    text: String,
}

fn state(
    report: &FillReport,
    board: &Board,
    net: &str,
    part: &str,
    pad: &str,
) -> Result<State, Refusal> {
    let listed: Vec<(&str, &FillPad)> = report
        .fills
        .iter()
        .filter(|f| f.net == net)
        .flat_map(|f| {
            f.pads
                .iter()
                .filter(|p| p.part == part && p.pad == pad)
                .map(move |p| (f.layer.as_str(), p))
        })
        .collect();
    let checked = netsmith_geom::drc_check(
        &netsmith_pcb::kicad_pcb(board),
        Some(&netsmith_pcb::kicad_pro(board)),
        &crate::fab::drc_extras(board),
    )
    .map_err(|e| Refusal::Failed(format!("the design rules check: {e}")))?;
    let unconnected = checked.findings.iter().any(|f| {
        f.rule == netsmith_geom::DrcRule::UnconnectedItems
            && f.items
                .iter()
                .any(|i| i.part.as_deref() == Some(part) && i.pad.as_deref() == Some(pad))
    });
    let good = |p: &FillPad| (p.joined && !p.starved) || p.joined_elsewhere;
    if let Some((layer, _)) = listed.iter().find(|(_, p)| good(p)) {
        return Ok(State {
            joined: true,
            unconnected,
            text: format!("the fill on {layer} joins it"),
        });
    }
    let text = match listed.first() {
        Some((layer, p)) => match (&p.unjoinable, p.starved) {
            (Some(why), _) => format!("unjoinable on {layer}: {why}"),
            (None, true) => format!(
                "starved on {layer}: {} of {} spokes, {} track(s)",
                p.joined_spokes.unwrap_or(0),
                p.spokes.unwrap_or(0),
                p.tracks
            ),
            (None, false) => format!("the fill on {layer} does not reach it"),
        },
        None if unconnected => {
            "no fill of its net covers its centre, and the design rules check finds it unconnected"
                .to_string()
        }
        None => {
            "no fill of its net covers its centre, and its copper joins it to its net".to_string()
        }
    };
    Ok(State {
        joined: listed.is_empty() && !unconnected,
        unconnected,
        text,
    })
}

/// How far from `centre` the ray along `u` leaves the box `r`.
fn exit(r: &netsmith_pcb::Rect, centre: Pt, u: Pt) -> f64 {
    let leave = |c: f64, d: f64, lo: f64, hi: f64| {
        if d > 1e-12 {
            (hi - c) / d
        } else if d < -1e-12 {
            (lo - c) / d
        } else {
            f64::INFINITY
        }
    };
    leave(centre.x, u.x, r.min.x, r.max.x)
        .min(leave(centre.y, u.y, r.min.y, r.max.y))
        .max(0.0)
}

/// The place `t` mm from `centre` along the unit `u`.
fn along(centre: Pt, u: Pt, t: f64) -> Pt {
    Pt::new(centre.x + u.x * t, centre.y + u.y * t)
}
