//! `netsmith routes fanout REF` (issue #1640): the escape of a fine-pitch part (a BGA, an LGA, the
//! lands of a QFN) fanned out as a group: a via site assigned to each pad that needs to escape,
//! the assignment negotiated across the group, and each pad routed to its via, written to the
//! routes file as ordinary copper.
//!
//! **Which pads escape.** The part's surface-mount pads on a net that leaves the part (a pad of
//! another part is on it) and that is not on the outer ring of its pads, and that no copper of the
//! routes file reaches already: a track of its net on the pad's face with an end in the pad, or a
//! via of its net in it (those are said as joined already). A pad is on the outer ring when it is
//! the first or the last of its row or its column of pads (in the footprint's own frame, centres
//! within 0.01 mm): those reach the copper outside the part on its face with no via. `--net`
//! narrows the pads to those of the nets named; `--pads` names the pads instead (any surface-mount
//! pad of the part on a net, the outer ring too); `--plan` names them with their sites.
//!
//! **The sites.** The legal sites between the part's pads of [`crate::via_sites`] (judged for the
//! process's through via of no net of the board, so a free site takes a via of any of its
//! neighbours' nets), less those copper of the routes file of another net than the pad's holds;
//! or, with `--sites`, the places given, each judged by [`crate::via_check`], a place it finds an
//! error with listed with what blocks it and not used. A pad may take a site no further from its
//! centre than [`REACH`] pitches (the part's least distance between two pad centres). Two vias of
//! different nets must keep the board's clearance between their copper and its via hole to hole
//! space between their holes ([`apart`]).
//!
//! **The assignment, negotiated.** The pads and the sites are matched at the least total distance
//! from pad to site ([`assign`], the Hungarian method on the pads of the group at once, never one
//! pad at a time); where two sites the matching gives to pads of different nets are too near each
//! other, the site of the pad further from it is barred to that pad and the group is matched
//! again. Then every pad assigned is routed to its via in one run of the router for the group, the
//! router's negotiation among their lines ([`netsmith_router::one::route_nets_weighed`]): each
//! net's pads in the group and their vias are its only pads for that run, a plated pad of the
//! via's size and drill on its layers standing in for each via ([`crate::route_net`]'s
//! `--via`), every other net held as it is. A net whose every pad reached its via has escaped,
//! and its vias and the router's copper go to its record, unlocked; the pads of a net left are
//! barred from the site they had and matched again in the next round with the sites left, at most
//! [`ROUNDS`] rounds. A plan is followed as it is: no pad of it is matched again.
//!
//! **Never refused silently.** A pad that cannot escape is listed with what blocks it: no legal
//! site within reach (or none left free: the sites near it held by copper of another net, or
//! given to other pads), what walls its line in (the router's report: pads, vias, the copper of
//! other nets, the edge), or the design rules check's refusal of its copper. A plan's site the
//! check finds an error with, or one too near another planned site of another net, is a refusal
//! that names what blocks it, and nothing is written.
//!
//! **Checked as an edit.** The records that change are put in the file together as one edit
//! ([`crate::routes::edit_all`]), so a result that adds an error of the design rules check is
//! refused; then each net's record is tried alone, in turn, and a net whose copper is refused is
//! listed with the refusal while the others are written.
//!
//! **Bounded work, counted.** At most [`ROUNDS`] runs of the router, each with its own bounds
//! (counts); the checks of the sites, one check for each place given or planned, and the checks
//! of the edit ([`Fanout::checks`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use netsmith_pcb::{Board, CopperLayer, NetRoutes, PadRef, Pt, RouteVia, Routes, Via};
use netsmith_router::one;

use crate::route_net::through::stand_in;
use crate::route_net::{Wall, add, um, walls};
use crate::routes::{Edit, Refusal, edit_all_counted, via_offer, with_routes};
use crate::{via_check, via_sites};

/// The most rounds of matching and routing ([module](self)).
pub const ROUNDS: usize = 3;

/// How far from a pad's centre, in pitches, a site it may take lies: [`via_sites::NEIGHBOUR`], the
/// sites of the squares round it and those between it and its neighbours.
pub const REACH: f64 = via_sites::NEIGHBOUR;

/// A line of a plan (`--plan FILE`): a pad and the place of its via, in the routes file's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    /// The pad's number (`B2`), or `REF.PAD` (`U1.B2`).
    pub pad: String,
    pub at: [f64; 2],
    /// The line of the file, from 1.
    pub line: usize,
}

/// The lines of a plan file: `PAD = X,Y` (or `REF.PAD = X,Y`) for each pad, a place in mm in the
/// routes file's frame; blank lines and what follows a `#` are ignored.
pub fn parse_plan(text: &str) -> Result<Vec<Planned>, String> {
    let mut out = Vec::new();
    for (k, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let bad = || {
            format!(
                "line {} of the plan, `{}`, is not `PAD = X,Y` (`B2 = 12.4,8.8`)",
                k + 1,
                raw.trim()
            )
        };
        let (pad, place) = line.split_once('=').ok_or_else(bad)?;
        let (x, y) = place.split_once(',').ok_or_else(bad)?;
        let (x, y) = (
            x.trim().parse::<f64>().map_err(|_| bad())?,
            y.trim().parse::<f64>().map_err(|_| bad())?,
        );
        let pad = pad.trim();
        if pad.is_empty() || !x.is_finite() || !y.is_finite() {
            return Err(bad());
        }
        out.push(Planned {
            pad: pad.to_string(),
            at: [x, y],
            line: k + 1,
        });
    }
    Ok(out)
}

/// What to fan out ([module](self)).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FanoutAsk {
    /// The part, by its reference.
    pub part: String,
    /// Only the pads of these nets (`--net`).
    pub nets: Vec<String>,
    /// These pads, by number or `REF.PAD` (`--pads`), in place of the pads that need to escape.
    pub pads: Vec<String>,
    /// The plan (`--plan`): the pads and their sites.
    pub plan: Option<Vec<Planned>>,
    /// The sites allowed (`--sites`), in the routes file's frame; empty for the part's sites.
    pub sites: Vec<[f64; 2]>,
    /// The weights of the router's quality objective; `None` is the router's own costs.
    pub weights: Option<netsmith_router::Weights>,
}

/// A pad fanned out to its via.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Escape {
    /// `REF.PAD`.
    pub pad: String,
    pub net: String,
    /// The via's place, in the routes file's frame.
    pub at: [f64; 2],
    /// The round it escaped in, from 1.
    pub round: usize,
}

/// A pad that could not be fanned out, and what blocks it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Left {
    /// `REF.PAD`.
    pub pad: String,
    pub net: String,
    /// The site it had last, when it had one.
    pub at: Option<[f64; 2]>,
    /// What blocks it, said for a person.
    pub blocker: String,
    /// What walls its line in, when the router found no way to its via.
    pub walls: Vec<Wall>,
}

/// A place of `--sites` that the check finds an error with.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SiteRefused {
    pub at: [f64; 2],
    pub blocker: String,
}

/// What `routes fanout` did, or would do ([module](self)).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Fanout {
    pub board: String,
    pub part: String,
    pub via: via_sites::ViaSize,
    /// The part's least distance between two pad centres, mm.
    pub pitch: f64,
    /// The pads that needed to escape (those joined already not counted).
    pub needed: usize,
    /// The pads fanned out, in the footprint's order.
    pub escaped: Vec<Escape>,
    /// The pads the routes file's copper joins already, `REF.PAD`.
    pub joined: Vec<String>,
    /// The pads that could not be fanned out, in the footprint's order.
    pub left: Vec<Left>,
    /// The sites the pads were matched with.
    pub sites: usize,
    /// The places of `--sites` the check refused.
    pub sites_refused: Vec<SiteRefused>,
    /// The rounds of matching and routing, and the runs of the router.
    pub rounds: usize,
    pub runs: usize,
    /// The design rules checks made.
    pub checks: usize,
    /// The routes file with the change, when there is one.
    #[serde(skip)]
    pub routes: Option<Routes>,
}

/// A pad of the part that may escape.
#[derive(Debug, Clone)]
struct Pad {
    number: String,
    net: String,
    at: Pt,
    local: Pt,
    face: CopperLayer,
    radius: f64,
}

impl Pad {
    fn name(&self, refdes: &str) -> String {
        format!("{refdes}.{}", self.number)
    }
}

/// A site the pads are matched with: its place on the board, and the nets whose copper of the
/// routes file holds it.
#[derive(Debug, Clone)]
struct Site {
    at: Pt,
    held: BTreeSet<String>,
}

/// Fan out the part `ask.part` of `board` (placed, with no routes on it) with the copper of
/// `routes`: the [module](self)'s header. The routes are not changed; [`Fanout::routes`] is the
/// file with the change. Refused when the board has no such part, net or pad, or a plan's site is
/// illegal.
pub fn fanout(board: &Board, routes: &Routes, ask: &FanoutAsk) -> Result<Fanout, Refusal> {
    let refuse = |e: String| Refusal::Refused(e);
    let refdes = ask.part.as_str();
    let part = board
        .part(refdes)
        .ok_or_else(|| refuse(format!("the board {} has no part {refdes}", board.name)))?;
    for n in &ask.nets {
        if !board.nets.iter().any(|b| b.name == *n) {
            return Err(refuse(format!("the board {} has no net `{n}`", board.name)));
        }
    }
    let offer = via_offer(board, None)?;
    let net_of: HashMap<&str, &netsmith_pcb::BoardNet> = board
        .nets
        .iter()
        .flat_map(|n| {
            n.pads
                .iter()
                .filter(|p| p.refdes == refdes)
                .map(move |p| (p.number.as_str(), n))
        })
        .collect();
    // The part's surface-mount pads on a net.
    let pads: Vec<Pad> = part
        .footprint
        .pads
        .iter()
        .filter(|p| p.drill.is_none() && !p.number.is_empty())
        .filter_map(|p| {
            let (front, back) = part.pad_layers(p);
            if front == back {
                return None;
            }
            let net = net_of.get(p.number.as_str())?;
            Some(Pad {
                number: p.number.clone(),
                net: net.name.clone(),
                at: part.pad_centre(p),
                local: p.at,
                face: if front {
                    CopperLayer::Front
                } else {
                    CopperLayer::Back
                },
                radius: p.width.min(p.height) / 2.0,
            })
        })
        .collect();
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
    let pitch = if pitch.is_finite() { pitch } else { 0.0 };
    let find = |name: &str| {
        let number = name
            .strip_prefix(&format!("{refdes}."))
            .unwrap_or(name)
            .to_string();
        pads.iter().position(|p| p.number == number).ok_or_else(|| {
            refuse(format!(
                "`{name}` is not a surface-mount pad of {refdes} on a net"
            ))
        })
    };

    // The pads asked for, with their planned sites.
    let mut planned: BTreeMap<usize, Pt> = BTreeMap::new();
    let chosen: Vec<usize> = if let Some(plan) = &ask.plan {
        for p in plan {
            let k = find(&p.pad).map_err(|e| match e {
                Refusal::Refused(e) => refuse(format!("line {} of the plan: {e}", p.line)),
                e => e,
            })?;
            if planned
                .insert(k, Pt::new(p.at[0], p.at[1]) + board.outline.min)
                .is_some()
            {
                return Err(refuse(format!(
                    "line {} of the plan names {} a second time",
                    p.line,
                    pads[k].name(refdes)
                )));
            }
        }
        planned.keys().copied().collect()
    } else if !ask.pads.is_empty() {
        let mut v = Vec::new();
        for name in &ask.pads {
            let k = find(name)?;
            if !v.contains(&k) {
                v.push(k);
            }
        }
        v.sort();
        v
    } else {
        let outer = outer_ring(&pads);
        (0..pads.len())
            .filter(|&k| !outer[k])
            .filter(|&k| {
                net_of[pads[k].number.as_str()]
                    .pads
                    .iter()
                    .any(|p| p.refdes != refdes)
            })
            .collect()
    };
    let chosen: Vec<usize> = chosen
        .into_iter()
        .filter(|&k| ask.nets.is_empty() || ask.nets.contains(&pads[k].net))
        .collect();

    let (routed, _) = with_routes(board, routes)?;
    let mut out = Fanout {
        board: board.name.clone(),
        part: refdes.to_string(),
        via: via_sites::ViaSize {
            diameter: offer.diameter,
            drill: offer.drill,
        },
        pitch: um(pitch),
        needed: 0,
        escaped: Vec::new(),
        joined: Vec::new(),
        left: Vec::new(),
        sites: 0,
        sites_refused: Vec::new(),
        rounds: 0,
        runs: 0,
        checks: 0,
        routes: None,
    };
    let file_place = |p: Pt| {
        let q = p - board.outline.min;
        [um(q.x) + 0.0, um(q.y) + 0.0]
    };
    let place_said = |p: Pt| {
        let [x, y] = file_place(p);
        format!("({x}, {y})")
    };

    // The pads joined already, and the nets whose records take no copper.
    let mut todo: Vec<usize> = Vec::new();
    let table = routes.boards.get(&board.name);
    for k in chosen {
        let p = &pads[k];
        let reached = routed.tracks.iter().any(|t| {
            t.net == p.net
                && t.layer == p.face
                && (t.from.distance(p.at) <= p.radius || t.to.distance(p.at) <= p.radius)
        }) || routed
            .vias
            .iter()
            .any(|v| v.net == p.net && v.at.distance(p.at) <= p.radius);
        if reached {
            out.joined.push(p.name(refdes));
            continue;
        }
        out.needed += 1;
        if let Some(record) = table.and_then(|t| t.nets.get(&p.net)) {
            let why = if record.lock {
                Some(format!(
                    "net {} is locked in the routes file (`netsmith routes unlock --net {}` first)",
                    p.net, p.net
                ))
            } else if Some(&record.pads) != netsmith_pcb::pad_hash(board, &p.net).as_ref() {
                Some(format!(
                    "net {}'s record was written for pads that have moved since (`netsmith routes lock --net {}` writes it again)",
                    p.net, p.net
                ))
            } else {
                None
            };
            if let Some(blocker) = why {
                out.left.push(Left {
                    pad: p.name(refdes),
                    net: p.net.clone(),
                    at: None,
                    blocker,
                    walls: Vec::new(),
                });
                continue;
            }
        }
        todo.push(k);
    }

    // Two vias of different nets far enough apart.
    let rules = &board.rules;
    let apart = |a: Pt, b: Pt| {
        let d = a.distance(b);
        d + 1e-6 >= offer.diameter + rules.clearance
            && d + 1e-6 >= offer.drill + rules.via_hole_to_hole.max(rules.hole_to_hole)
    };

    // The plan's sites, each judged.
    if !planned.is_empty() {
        let mut said: Vec<(usize, Pt)> = Vec::new();
        for (&k, &at) in &planned {
            if !todo.contains(&k) {
                continue;
            }
            let p = &pads[k];
            let check = via_check::check_via(
                board,
                routes,
                file_place(at),
                Some(&p.net),
                None,
                None,
                None,
            )?;
            out.checks += check.checks;
            if !check.legal {
                return Err(refuse(format!(
                    "the plan's site of {} at {} is refused: {}; nothing is written",
                    p.name(refdes),
                    place_said(at),
                    clash_said(&check.breaks)
                )));
            }
            if let Some((j, _)) = said
                .iter()
                .find(|(j, b)| pads[*j].net != p.net && !apart(at, *b))
            {
                return Err(refuse(format!(
                    "the plan's site of {} at {} is too near the site of {} (net {}) at {}: two vias of different nets keep {:.3} mm between centres; nothing is written",
                    p.name(refdes),
                    place_said(at),
                    pads[*j].name(refdes),
                    pads[*j].net,
                    place_said(planned[j]),
                    (offer.diameter + rules.clearance)
                        .max(offer.drill + rules.via_hole_to_hole.max(rules.hole_to_hole))
                )));
            }
            said.push((k, at));
        }
    }

    // The sites the pads are matched with.
    let mut pool: Vec<Site> = Vec::new();
    if planned.is_empty() && !todo.is_empty() {
        if ask.sites.is_empty() {
            let found = via_sites::sites(board, routes, refdes, None)?;
            out.checks += found.checks;
            pool = found
                .between
                .iter()
                .map(|s| Site {
                    at: Pt::new(s.at[0], s.at[1]) + board.outline.min,
                    held: s.held_by.iter().map(|h| h.net.clone()).collect(),
                })
                .collect();
        } else {
            for &at in &ask.sites {
                let check = via_check::check_via(board, routes, at, None, None, None, None)?;
                out.checks += check.checks;
                let p = Pt::new(at[0], at[1]) + board.outline.min;
                if check.legal {
                    if !pool.iter().any(|s| s.at.distance(p) < 1e-4) {
                        pool.push(Site {
                            at: p,
                            held: BTreeSet::new(),
                        });
                    }
                } else {
                    out.sites_refused.push(SiteRefused {
                        at: file_place(p),
                        blocker: clash_said(&check.breaks),
                    });
                }
            }
        }
        out.sites = pool.len();
    }

    // The rounds of matching and routing.
    let reach = REACH * pitch + 1e-6;
    let mut state = routes.clone();
    let mut barred: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut placed: Vec<(Pt, String)> = Vec::new();
    let mut last: BTreeMap<usize, (Option<Pt>, String, Vec<Wall>)> = BTreeMap::new();
    let mut escaped: BTreeMap<usize, (Pt, usize)> = BTreeMap::new();
    for round in 1..=ROUNDS {
        if todo.is_empty() {
            break;
        }
        // Each pad's site this round.
        let mut given: Vec<(usize, Pt)> = Vec::new();
        if !planned.is_empty() {
            if round > 1 {
                break;
            }
            given = todo.iter().map(|&k| (k, planned[&k])).collect();
        } else {
            let allowed = |i: usize, j: usize, barred: &BTreeSet<(usize, usize)>| {
                let (p, s) = (&pads[todo[i]], &pool[j]);
                s.at.distance(p.at) <= reach
                    && s.held.iter().all(|n| *n == p.net)
                    && !barred.contains(&(todo[i], j))
                    && placed.iter().all(|(at, net)| {
                        (*net == p.net && at.distance(s.at) > 1e-4) || apart(*at, s.at)
                    })
            };
            let mut matched;
            let mut guard = 0;
            loop {
                let cost: Vec<Vec<Option<f64>>> = (0..todo.len())
                    .map(|i| {
                        (0..pool.len())
                            .map(|j| {
                                allowed(i, j, &barred)
                                    .then(|| pool[j].at.distance(pads[todo[i]].at))
                            })
                            .collect()
                    })
                    .collect();
                matched = assign(&cost);
                // A pair too near, of different nets: the site barred to the pad further from it.
                let mut clash = None;
                'pairs: for (a, sa) in matched.iter().enumerate() {
                    let Some(sa) = sa else { continue };
                    for (b, sb) in matched.iter().enumerate().skip(a + 1) {
                        let Some(sb) = sb else { continue };
                        if pads[todo[a]].net != pads[todo[b]].net
                            && !apart(pool[*sa].at, pool[*sb].at)
                        {
                            let da = pool[*sa].at.distance(pads[todo[a]].at);
                            let db = pool[*sb].at.distance(pads[todo[b]].at);
                            clash = Some(if da >= db { (a, *sa) } else { (b, *sb) });
                            break 'pairs;
                        }
                    }
                }
                guard += 1;
                match clash {
                    Some((i, j)) if guard <= pool.len() * todo.len() => {
                        barred.insert((todo[i], j));
                    }
                    _ => break,
                }
            }
            let mut no_site = Vec::new();
            for (i, s) in matched.iter().enumerate() {
                let k = todo[i];
                match s {
                    Some(j) => given.push((k, pool[*j].at)),
                    None => {
                        // A pad a round before left keeps what blocked it then.
                        no_site.push(k);
                        if last.contains_key(&k) {
                            continue;
                        }
                        let p = &pads[k];
                        let near: Vec<&Site> = pool
                            .iter()
                            .filter(|s| s.at.distance(p.at) <= reach)
                            .collect();
                        let held_here = |s: &&&Site| s.held.iter().any(|n| *n != p.net);
                        let held: BTreeSet<String> = near
                            .iter()
                            .flat_map(|s| s.held.iter().filter(|n| **n != p.net).cloned())
                            .collect();
                        let n_held = near.iter().filter(held_here).count();
                        let why = if near.is_empty() {
                            format!(
                                "no legal via site within {:.3} mm of it ({REACH} pitches{}): `netsmith routes sites {refdes}` lists the sites and what refused the rest",
                                reach,
                                if ask.sites.is_empty() {
                                    ""
                                } else {
                                    ", of the places of `--sites`"
                                }
                            )
                        } else if !held.is_empty() {
                            let rest = near.len() - n_held;
                            format!(
                                "of the {} site(s) within reach, {n_held} {} held by the copper of {}{}",
                                near.len(),
                                if n_held == 1 { "is" } else { "are" },
                                nets_said(&held),
                                if rest == 0 {
                                    String::new()
                                } else {
                                    format!(
                                        " and {rest} given to other pads or too near their vias"
                                    )
                                }
                            )
                        } else {
                            format!(
                                "the {} site(s) within reach are given to other pads, or too near their vias",
                                near.len()
                            )
                        };
                        last.insert(k, (None, why, Vec::new()));
                    }
                }
            }
            todo.retain(|k| !no_site.contains(k));
        }
        if given.is_empty() {
            break;
        }
        out.rounds = round;

        // One run of the router for the group, each via stood in for by a plated pad.
        let (mut b, _) = with_routes(board, &state)?;
        let mut group: BTreeMap<String, Vec<PadRef>> = BTreeMap::new();
        let mut stand: Vec<(String, usize, Pt)> = Vec::new();
        for (n, &(k, at)) in given.iter().enumerate() {
            let p = &pads[k];
            let via = Via {
                net: p.net.clone(),
                at,
                diameter: offer.diameter,
                drill: offer.drill,
                span: None,
            };
            let s = stand_in(&b, &via, n);
            stand.push((format!("{}.1", s.refdes), k, at));
            let refs = group.entry(p.net.clone()).or_default();
            refs.push(PadRef {
                refdes: refdes.to_string(),
                number: p.number.clone(),
            });
            refs.push(PadRef {
                refdes: s.refdes.clone(),
                number: "1".to_string(),
            });
            b.parts.push(s);
        }
        for net in b.nets.iter_mut() {
            if let Some(refs) = group.get(&net.name) {
                net.pads = refs.clone();
            }
        }
        let nets: BTreeSet<String> = group.keys().cloned().collect();
        out.runs += 1;
        let way = one::route_nets_weighed(&b, &nets, ask.weights);
        let mut next = Vec::new();
        for net in &nets {
            let mine: Vec<&(usize, Pt)> =
                given.iter().filter(|(k, _)| pads[*k].net == *net).collect();
            if way.left.iter().all(|l| l.net != *net) {
                add(board, &mut state, net, &way);
                for &&(k, at) in &mine {
                    add_via(board, &mut state, net, at, offer);
                    placed.push((at, net.clone()));
                    escaped.insert(k, (at, round));
                    last.remove(&k);
                }
                continue;
            }
            let why: Vec<_> = way
                .why
                .iter()
                .filter(|r| r.line.net == *net)
                .cloned()
                .collect();
            let mut found = walls(board, &why);
            for w in &mut found {
                for (name, _, at) in &stand {
                    if w.line.contains(name.as_str()) {
                        w.line = w
                            .line
                            .replace(name.as_str(), &format!("the via {}", place_said(*at)));
                    }
                    w.pads.retain(|p| p != name);
                }
            }
            let blocker = walls_said(&found);
            for &&(k, at) in &mine {
                if let Some(j) = pool.iter().position(|s| s.at.distance(at) < 1e-6) {
                    barred.insert((k, j));
                }
                last.insert(k, (Some(at), blocker.clone(), found.clone()));
                next.push(k);
            }
        }
        todo.retain(|k| next.contains(k));
    }

    // The change, checked as one edit; failing that, each net's record alone.
    let record = |r: &Routes, net: &str| {
        r.boards
            .get(&board.name)
            .and_then(|t| t.nets.get(net))
            .cloned()
    };
    let changed: BTreeSet<String> = escaped.keys().map(|&k| pads[k].net.clone()).collect();
    let edits: Vec<(String, Edit)> = changed
        .iter()
        .filter(|n| record(routes, n) != record(&state, n))
        .map(|n| {
            (
                n.clone(),
                Edit::Restore {
                    net: n.clone(),
                    before: record(routes, n),
                    after: record(&state, n),
                },
            )
        })
        .collect();
    if !edits.is_empty() {
        let mut checked = routes.clone();
        let all: Vec<Edit> = edits.iter().map(|(_, e)| e.clone()).collect();
        let (result, counts) = edit_all_counted(board, &mut checked, &all);
        out.checks += counts.checks;
        match result {
            Ok(_) => out.routes = Some(checked),
            Err(Refusal::Failed(e)) => return Err(Refusal::Failed(e)),
            Err(Refusal::Refused(_)) => {
                let mut kept = routes.clone();
                let mut any = false;
                for (net, edit) in &edits {
                    let mut next = kept.clone();
                    let (result, counts) =
                        edit_all_counted(board, &mut next, std::slice::from_ref(edit));
                    out.checks += counts.checks;
                    match result {
                        Ok(_) => {
                            kept = next;
                            any = true;
                        }
                        Err(Refusal::Failed(e)) => return Err(Refusal::Failed(e)),
                        Err(Refusal::Refused(e)) => {
                            let ks: Vec<usize> = escaped
                                .keys()
                                .copied()
                                .filter(|&k| pads[k].net == *net)
                                .collect();
                            for k in ks {
                                let (at, _) = escaped.remove(&k).expect("escaped");
                                last.insert(
                                    k,
                                    (
                                        Some(at),
                                        format!("the design rules check refuses its copper: {e}"),
                                        Vec::new(),
                                    ),
                                );
                            }
                        }
                    }
                }
                if any {
                    out.routes = Some(kept);
                }
            }
        }
    }
    for (k, (at, round)) in &escaped {
        out.escaped.push(Escape {
            pad: pads[*k].name(refdes),
            net: pads[*k].net.clone(),
            at: file_place(*at),
            round: *round,
        });
    }
    for (k, (at, blocker, walls)) in last {
        out.left.push(Left {
            pad: pads[k].name(refdes),
            net: pads[k].net.clone(),
            at: at.map(file_place),
            blocker,
            walls,
        });
    }
    let order: HashMap<String, usize> = pads
        .iter()
        .enumerate()
        .map(|(k, p)| (p.name(refdes), k))
        .collect();
    out.left
        .sort_by_key(|l| order.get(&l.pad).copied().unwrap_or(usize::MAX));
    Ok(out)
}

/// Which pads are on the outer ring of the part's pads: the first or the last of their row or
/// their column, in the footprint's own frame ([module](self)).
fn outer_ring(pads: &[Pad]) -> Vec<bool> {
    const SAME: f64 = 0.01;
    pads.iter()
        .map(|p| {
            let row = pads.iter().filter(|q| (q.local.y - p.local.y).abs() < SAME);
            let (lo, hi) = row.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| {
                (lo.min(q.local.x), hi.max(q.local.x))
            });
            let column = pads.iter().filter(|q| (q.local.x - p.local.x).abs() < SAME);
            let (top, bottom) = column.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| {
                (lo.min(q.local.y), hi.max(q.local.y))
            });
            p.local.x <= lo + SAME
                || p.local.x >= hi - SAME
                || p.local.y <= top + SAME
                || p.local.y >= bottom - SAME
        })
        .collect()
}

/// The via of the process at `at` (the board's places) added to `net`'s record, unlocked.
fn add_via(board: &Board, routes: &mut Routes, net: &str, at: Pt, offer: netsmith_pcb::ViaOffer) {
    let record = routes
        .boards
        .entry(board.name.clone())
        .or_default()
        .nets
        .entry(net.to_string())
        .or_insert_with(|| NetRoutes {
            pads: netsmith_pcb::pad_hash(board, net).unwrap_or_default(),
            ..NetRoutes::default()
        });
    let mut v = RouteVia::of(
        board,
        &Via {
            net: net.to_string(),
            at,
            diameter: offer.diameter,
            drill: offer.drill,
            span: None,
        },
    );
    v.at = [um(v.at[0]), um(v.at[1])];
    record.vias.push(v);
}

/// The pads matched with sites at the least total cost: `cost[i][j]` is what pad `i` pays for site
/// `j`, `None` where it may not take it. Each pad gets a site or none (`None`), no site two pads;
/// as many pads as can be get one, then the least total. The Hungarian method on the square of
/// the pads and the sites with one "no site" for each pad, which costs more than any site.
pub fn assign(cost: &[Vec<Option<f64>>]) -> Vec<Option<usize>> {
    let n = cost.len();
    if n == 0 {
        return Vec::new();
    }
    let m_sites = cost[0].len();
    let most = cost
        .iter()
        .flatten()
        .flatten()
        .fold(0.0_f64, |a, &b| a.max(b));
    // No site costs more than every site together; a site barred more than that again.
    let none = (most + 1.0) * (n as f64 + 1.0);
    let barred = none * (n as f64 + 1.0) * 2.0;
    let m = m_sites + n;
    let at = |i: usize, j: usize| -> f64 {
        if j < m_sites {
            cost[i][j].unwrap_or(barred)
        } else {
            none
        }
    };
    // e-maxx's Hungarian method, rows 1..=n, columns 1..=m, n <= m.
    let mut u = vec![0.0; n + 1];
    let mut v = vec![0.0; m + 1];
    let mut p = vec![0usize; m + 1];
    let mut way = vec![0usize; m + 1];
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = vec![f64::INFINITY; m + 1];
        let mut used = vec![false; m + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = f64::INFINITY;
            let mut j1 = 0;
            for j in 1..=m {
                if !used[j] {
                    let cur = at(i0 - 1, j - 1) - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=m {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut out = vec![None; n];
    for j in 1..=m {
        if p[j] != 0 && j <= m_sites && cost[p[j] - 1][j - 1].is_some() {
            out[p[j] - 1] = Some(j - 1);
        }
    }
    out
}

/// The first rule a check of a via finds broken, said: the rule, the thing in the way, its net
/// and the shortfall.
fn clash_said(breaks: &[via_check::Clash]) -> String {
    let Some(c) = breaks.first() else {
        return "the design rules check finds an error with a via there".to_string();
    };
    let mut s = format!(
        "{} with {}",
        c.rule,
        if c.what.is_empty() { &c.item } else { &c.what }
    );
    if let Some(n) = &c.net
        && !c.what.contains(n.as_str())
    {
        s.push_str(&format!(" of net {n}"));
    }
    if let Some(short) = c.shortfall {
        s.push_str(&format!(", short by {short:.4} mm"));
    }
    s
}

/// What walls a line in, said ([`Wall`]).
fn walls_said(found: &[Wall]) -> String {
    let mut who: Vec<String> = Vec::new();
    for w in found {
        if !w.pads.is_empty() {
            who.push(format!("pad(s) {}", w.pads.join(", ")));
        }
        if !w.vias.is_empty() {
            who.push(format!("via(s) of {}", w.vias.join(", ")));
        }
        if !w.nets.is_empty() {
            let set: BTreeSet<String> = w.nets.iter().cloned().collect();
            who.push(format!("the copper of {}", nets_said(&set)));
        }
        if w.edge {
            who.push("the board's edge".to_string());
        }
    }
    who.dedup();
    if who.is_empty() {
        "the router found no way to its via".to_string()
    } else {
        format!("no way to its via: walled in by {}", who.join("; "))
    }
}

/// `net A`, `nets A and B`, `nets A, B and C`.
fn nets_said(set: &BTreeSet<String>) -> String {
    let v: Vec<&str> = set.iter().map(String::as_str).collect();
    match v.as_slice() {
        [] => "no net".to_string(),
        [one] => format!("net {one}"),
        [rest @ .., last] => format!("nets {} and {last}", rest.join(", ")),
    }
}

impl Fanout {
    /// The text for a person.
    pub fn text(&self) -> String {
        self.text_moved([0.0, 0.0], "mm from the outline's top left corner, y down")
    }

    /// [`Fanout::text`] with its places moved by `by`, mm, into another frame than the routes
    /// file's, which `frame` says (after `places in`).
    pub fn text_moved(&self, by: [f64; 2], frame: &str) -> String {
        use std::fmt::Write as _;
        let at = |p: [f64; 2]| format!("{:.3},{:.3}", p[0] + by[0], p[1] + by[1]);
        let mut s = String::new();
        let _ = writeln!(
            s,
            "{} [{}]: {} of {} pad(s) that need to escape fanned out to a {:.3} mm via ({:.3} mm drill), pitch {:.3} mm, {} round(s), {} run(s) of the router",
            self.part,
            self.board,
            self.escaped.len(),
            self.needed,
            self.via.diameter,
            self.via.drill,
            self.pitch,
            self.rounds,
            self.runs
        );
        for e in &self.escaped {
            let _ = writeln!(s, "  {} [{}] to the via at {}", e.pad, e.net, at(e.at));
        }
        if !self.joined.is_empty() {
            let _ = writeln!(
                s,
                "joined already by the routes file's copper: {}",
                self.joined.join(", ")
            );
        }
        if !self.left.is_empty() {
            let _ = writeln!(s, "{} pad(s) not fanned out:", self.left.len());
            for l in &self.left {
                let site = match l.at {
                    Some(p) => format!(" (site {})", at(p)),
                    None => String::new(),
                };
                let _ = writeln!(s, "  {} [{}]{site}: {}", l.pad, l.net, l.blocker);
            }
        }
        for r in &self.sites_refused {
            let _ = writeln!(
                s,
                "the site at {} of `--sites` is refused: {}",
                at(r.at),
                r.blocker
            );
        }
        let matched = if self.sites == 0 {
            String::new()
        } else {
            format!("{} site(s) matched with, ", self.sites)
        };
        let _ = writeln!(
            s,
            "places in {frame} ({matched}{} design rules check(s))",
            self.checks
        );
        s
    }
}
