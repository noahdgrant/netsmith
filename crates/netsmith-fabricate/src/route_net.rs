//! `netsmith routes route` (issue #1195): one net, or one connection of it, routed by the router
//! round the copper the routes file has, the nets whose copper stood in its way named, and, when
//! asked, that copper removed and those nets routed again after it.
//!
//! **What is routed.** The board as placed with the routes file's copper on it, every other net
//! locked ([`netsmith_router::one::route_nets`]): the router adds copper to the one net and goes
//! round all the rest. A connection, `NET:REF.PAD-REF.PAD`, is the lines that join those two pads
//! alone: the net's other pads count as no net's while it is routed ([`Ask::connection`]), so the
//! way neither uses nor runs over them, and a net that has a plane joins each pad to it. The
//! copper goes to the routes file as unlocked shapes of the net's record.
//!
//! **What stood in the way.** When the net finds no way, it is routed again with the unlocked
//! tracks and vias of every net that may be ripped taken away: a net whose record is not locked,
//! is current (its pads have not moved) and has such a shape; zones, locked shapes and the
//! copper of locked nets stay. The nets whose copper that way crosses
//! ([`netsmith_router::walk::Ground::crossed`], the design rules check's own spaces) stood in
//! the way. Each of them is then given back its copper in turn, by name, and kept back when the
//! net still routes: the nets named are a set none of which can be left out, though not always
//! the smallest. When even that finds no way, what walls the net in is said instead (the
//! router's report: pads, vias, the copper of locked nets, the edge).
//!
//! **Rip and route again** (`--rip NET,...`): only the nets named may be ripped. Their copper is
//! removed, the net is routed through the room it left, and each ripped net is routed again
//! after it, by name. A ripped net that finds no way may rip in turn, from the nets named, never
//! the net asked for. **Bounds, all counts**, those of the router's rounds of rip-up (issue
//! #1088): at most [`netsmith_router::RIP_ROUNDS`] rounds of routing ripped nets again,
//! [`netsmith_router::RIP_MOST`] rips of one net, and [`MOST_RUNS`] runs of the router in all.
//! A ripped net left with more connections open than it had is a refusal: nothing is written.
//!
//! **Checked as an edit.** The records that change are put in the file together as one edit
//! ([`crate::routes::edit_all`]), so the design rules check is applied as `routes add-track`
//! applies it: a result that adds a finding is refused and the file is as it was.
//!
//! **The narrowest place.** The narrowest place of the new way, and the widest track of the net it has
//! room for there ([`netsmith_router::one::narrowest`]).

use std::collections::{BTreeMap, BTreeSet};

use netsmith_pcb::{Board, NetRoutes, PadRef, Ratline, Ratsnest, RouteTrack, RouteVia, Routes};
use netsmith_router::one::{self, One};
use netsmith_router::route::LineReport;
use netsmith_router::walk::Ground;
use netsmith_router::{RIP_MOST, RIP_ROUNDS};

use crate::routes::{Edit, Refusal, edit_all, with_routes};

/// The most times one `routes route` runs the router (each run routes one net): what bounds the
/// search for the nets in the way and the rounds of ripped nets together.
pub const MOST_RUNS: usize = 64;

/// What to route: a net, or one connection of it, and the nets that may be ripped.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub net: String,
    /// The two pads of the net to join, when only that connection is routed.
    pub connection: Option<[PadRef; 2]>,
    /// The nets whose unlocked copper may be removed and routed again (`--rip`).
    pub rip: BTreeSet<String>,
}

impl Ask {
    /// The ask for `target` (`NET`, or `NET:REF.PAD-REF.PAD`) and `rip` on `board`; refused when
    /// the board has no such net or pads, or `rip` names the net itself.
    pub fn of(board: &Board, target: &str, rip: &[String]) -> Result<Ask, Refusal> {
        let has = |n: &str| board.nets.iter().any(|x| x.name == n);
        let no_net =
            |n: &str| Refusal::Refused(format!("the board {} has no net `{n}`", board.name));
        let (net, connection) = if has(target) {
            (target.to_string(), None)
        } else {
            let (net, pads) = target.rsplit_once(':').ok_or_else(|| no_net(target))?;
            if !has(net) {
                return Err(no_net(net));
            }
            (net.to_string(), Some(connection_of(board, net, pads)?))
        };
        let mut set = BTreeSet::new();
        for r in rip {
            if !has(r) {
                return Err(no_net(r));
            }
            if *r == net {
                return Err(Refusal::Refused(format!(
                    "`--rip` names net {net}, the net to route: it rips the nets in its way"
                )));
            }
            set.insert(r.clone());
        }
        Ok(Ask {
            net,
            connection,
            rip: set,
        })
    }
}

/// The two pads `pads` (`REF.PAD-REF.PAD`) of `net`.
fn connection_of(board: &Board, net: &str, pads: &str) -> Result<[PadRef; 2], Refusal> {
    let of_net = &board
        .nets
        .iter()
        .find(|n| n.name == net)
        .expect("the net is the board's")
        .pads;
    let pad = |text: &str| {
        let (refdes, number) = text.split_once('.')?;
        let p = PadRef {
            refdes: refdes.to_string(),
            number: number.to_string(),
        };
        of_net.contains(&p).then_some(p)
    };
    for (i, _) in pads.match_indices('-') {
        if let (Some(a), Some(b)) = (pad(&pads[..i]), pad(&pads[i + 1..]))
            && a != b
        {
            return Ok([a, b]);
        }
    }
    let names: Vec<String> = of_net.iter().map(ToString::to_string).collect();
    Err(Refusal::Refused(format!(
        "`{pads}` is not two pads of net {net}, `REF.PAD-REF.PAD`: its pads are {}",
        names.join(", ")
    )))
}

/// What `routes route` did, or would do: the module's header.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Routed {
    pub board: String,
    pub net: String,
    /// The connection routed (`R1.2 to D1.1`), or `None` for the whole net.
    pub connection: Option<String>,
    /// The net, or the connection, was joined already: nothing to route.
    pub joined: bool,
    /// The nets whose copper stood in the way, when the net had no way round the copper there.
    pub in_the_way: Vec<String>,
    /// The nets ripped up and routed again.
    pub ripped: Vec<String>,
    /// Each record that changes, the net asked for first.
    pub nets: Vec<NetChange>,
    /// The narrowest place of the new way of the net asked for.
    pub narrowest: Option<Narrowest>,
    /// What walls the net in where it has no way even without the copper it may rip.
    pub walls: Vec<Wall>,
    /// Why nothing is (or would be) written; `None` when the change is made.
    pub refused: Option<String>,
    /// The runs of the router, and the rounds of routing ripped nets again.
    pub runs: usize,
    pub rounds: usize,
    /// The routes file with the change, when it is made.
    #[serde(skip)]
    pub routes: Option<Routes>,
}

/// One record that changes ([`Routed::nets`]), in the routes file's places.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NetChange {
    pub net: String,
    /// The unlocked tracks and vias of the record taken away.
    pub removed_tracks: usize,
    pub removed_vias: usize,
    /// The copper the router laid.
    pub tracks: Vec<RouteTrack>,
    pub vias: Vec<RouteVia>,
    /// The connections of the net still open after the change.
    pub left: Vec<String>,
}

/// The narrowest place of a way, in the routes file's places ([`one::Narrowest`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Narrowest {
    pub layer: String,
    pub at: [f64; 2],
    /// The width of the track there.
    pub width: f64,
    /// The widest track of the net that has room there, every space of the rules kept.
    pub fits: f64,
}

/// What walls in a line of the net ([`LineReport`]), in the routes file's places.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Wall {
    pub line: String,
    /// The nets of the copper in the wall.
    pub nets: Vec<String>,
    pub pads: Vec<String>,
    /// The vias in the wall, `net X at (x, y)`.
    pub vias: Vec<String>,
    pub edge: bool,
}

/// Route `ask` on `board` (placed, with no routes on it) round the copper of `routes`: the
/// module's header. The routes are not changed; [`Routed::routes`] is the file with the change
/// when it is made. An error is a board or file that could not be read.
pub fn route_net(board: &Board, routes: &Routes, ask: &Ask) -> Result<Routed, String> {
    let mut run = Runs {
        board,
        ask,
        runs: 0,
    };
    let mut out = Routed {
        board: board.name.clone(),
        net: ask.net.clone(),
        connection: ask.connection.as_ref().map(|[a, b]| format!("{a} to {b}")),
        joined: false,
        in_the_way: Vec::new(),
        ripped: Vec::new(),
        nets: Vec::new(),
        narrowest: None,
        walls: Vec::new(),
        refused: None,
        runs: 0,
        rounds: 0,
        routes: None,
    };
    let refused = run.plan(routes, &mut out).map_err(|e| e.to_string())?;
    out.runs = run.runs;
    match refused {
        Err(why) => out.refused = Some(why),
        Ok(None) => {}
        Ok(Some(state)) => finish(board, routes, &state, &mut out)?,
    }
    Ok(out)
}

/// The routes of `board` with `state` checked as one edit, and what changed.
fn finish(board: &Board, routes: &Routes, state: &Routes, out: &mut Routed) -> Result<(), String> {
    let record = |r: &Routes, net: &str| {
        r.boards
            .get(&board.name)
            .and_then(|t| t.nets.get(net))
            .cloned()
    };
    let mut nets: Vec<String> = vec![out.net.clone()];
    nets.extend(out.ripped.iter().cloned());
    let edits: Vec<Edit> = nets
        .iter()
        .filter(|n| record(routes, n) != record(state, n))
        .map(|n| Edit::Restore {
            net: n.clone(),
            before: record(routes, n),
            after: record(state, n),
        })
        .collect();
    let (real, _) = with_routes(board, state)?;
    let open = Ratsnest::of(&real);
    for net in &nets {
        let (before, after) = (record(routes, net), record(state, net));
        let (before, after) = (before.unwrap_or_default(), after.unwrap_or_default());
        let tracks: Vec<RouteTrack> = after
            .tracks
            .iter()
            .filter(|t| !before.tracks.contains(t))
            .cloned()
            .collect();
        let vias: Vec<RouteVia> = after
            .vias
            .iter()
            .filter(|v| !before.vias.contains(v))
            .cloned()
            .collect();
        out.nets.push(NetChange {
            net: net.clone(),
            removed_tracks: before
                .tracks
                .iter()
                .filter(|t| !after.tracks.contains(t))
                .count(),
            removed_vias: before
                .vias
                .iter()
                .filter(|v| !after.vias.contains(v))
                .count(),
            tracks,
            vias,
            left: open
                .lines
                .iter()
                .filter(|l| l.net == *net)
                .map(ToString::to_string)
                .collect(),
        });
    }
    let new: Vec<netsmith_pcb::Track> = out.nets[0]
        .tracks
        .iter()
        .map(|t| t.on(board, &out.net))
        .collect();
    out.narrowest = one::narrowest(&real, &out.net, &new).map(|c| {
        let at = c.at - board.outline.min;
        Narrowest {
            layer: c.layer.word().to_string(),
            at: [um(at.x), um(at.y)],
            width: c.width,
            fits: (c.fits * 1000.0).round() / 1000.0,
        }
    });
    if edits.is_empty() {
        return Ok(());
    }
    let mut checked = routes.clone();
    match edit_all(board, &mut checked, &edits) {
        Ok(_) => out.routes = Some(checked),
        Err(Refusal::Refused(e)) => out.refused = Some(e),
        Err(Refusal::Failed(e)) => return Err(e),
    }
    Ok(())
}

/// A place to the micrometre, as the routes file keeps it.
fn um(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// Where a net finds a way once some nets' copper is taken away ([`Runs::in_the_way`]).
enum Found {
    /// The nets whose copper the way crosses, and the way.
    Way(BTreeSet<String>, One),
    /// No way even so: the last run, if one was made.
    NoWay(Option<One>),
}

/// The runs of the router for one ask, counted.
struct Runs<'a> {
    board: &'a Board,
    ask: &'a Ask,
    runs: usize,
}

impl Runs<'_> {
    /// The routes with the change (`Ok(Some)`), nothing to do (`Ok(None)`), or why it is refused
    /// (`Err`), filling in what `out` says on the way. The outer error is a failure to read.
    #[allow(clippy::type_complexity)]
    fn plan(
        &mut self,
        routes: &Routes,
        out: &mut Routed,
    ) -> Result<Result<Option<Routes>, String>, Refusal> {
        let net = self.ask.net.as_str();
        let table = routes.boards.get(&self.board.name);
        if let Some(record) = table.and_then(|t| t.nets.get(net)) {
            if record.lock {
                return Ok(Err(format!(
                    "net {net} is locked in the routes file: the router adds no copper to it (`netsmith routes unlock --net {net}` first)"
                )));
            }
            if Some(&record.pads) != netsmith_pcb::pad_hash(self.board, net).as_ref() {
                return Ok(Err(format!(
                    "net {net}'s record was written for pads that have moved since: `netsmith routes lock --net {net}` writes it again, or `netsmith routes delete --net {net}` removes it"
                )));
            }
        }
        let (real, _) = with_routes(self.board, routes)?;
        if self.open(&real, net, true).is_empty() {
            out.joined = true;
            return Ok(Ok(None));
        }
        let first = self.route(routes, net, true)?;
        let mut state = routes.clone();
        if first.complete() {
            add(self.board, &mut state, net, &first);
            return Ok(Ok(Some(state)));
        }
        let every = |exclude: &BTreeSet<String>| candidates(self.board, routes, exclude, None);
        let only_net = BTreeSet::from([net.to_string()]);
        if self.ask.rip.is_empty() {
            return Ok(
                match self.in_the_way(routes, net, true, &every(&only_net))? {
                    Found::Way(set, _) => {
                        out.in_the_way = set.iter().cloned().collect();
                        Err(format!(
                            "net {net} has no way round the copper there: the copper of {} stands in its way; `--rip {}` removes it and routes it again after",
                            nets_said(&set),
                            list(&set)
                        ))
                    }
                    Found::NoWay(last) => {
                        out.walls = walls(self.board, &last.unwrap_or(first).why);
                        Err(no_way(net))
                    }
                },
            );
        }
        let allowed = candidates(self.board, routes, &only_net, Some(&self.ask.rip));
        let (set, way) = match self.in_the_way(routes, net, true, &allowed)? {
            Found::Way(set, way) => (set, way),
            Found::NoWay(_) => {
                return Ok(
                    match self.in_the_way(routes, net, true, &every(&only_net))? {
                        Found::Way(set, _) => {
                            out.in_the_way = set.iter().cloned().collect();
                            let more: BTreeSet<String> =
                                set.difference(&allowed).cloned().collect();
                            Err(format!(
                                "net {net} has no way with only the copper of the nets `--rip` names taken away: the copper of {} stands in its way, and {} may not be ripped (`--rip {}`)",
                                nets_said(&set),
                                nets_said(&more),
                                list(&set)
                            ))
                        }
                        Found::NoWay(last) => {
                            out.walls = walls(self.board, &last.unwrap_or(first).why);
                            Err(no_way(net))
                        }
                    },
                );
            }
        };
        out.in_the_way = set.iter().cloned().collect();
        state = strip(self.board, &state, &set);
        add(self.board, &mut state, net, &way);
        // The ripped nets routed again, round after round.
        let mut rips: BTreeMap<String, usize> = set.iter().map(|n| (n.clone(), 1)).collect();
        let mut queue: Vec<String> = set.into_iter().collect();
        for round in 1..=RIP_ROUNDS {
            if queue.is_empty() || self.runs >= MOST_RUNS {
                break;
            }
            out.rounds = round;
            let mut next: BTreeSet<String> = BTreeSet::new();
            for m in std::mem::take(&mut queue) {
                if self.runs >= MOST_RUNS {
                    break;
                }
                let again = self.route(&state, &m, false)?;
                if again.complete() {
                    add(self.board, &mut state, &m, &again);
                    continue;
                }
                // It may rip in turn: the nets named, not the net asked for, not itself, and
                // not one ripped as often as it may be.
                let mut exclude = only_net.clone();
                exclude.insert(m.clone());
                exclude.extend(
                    rips.iter()
                        .filter(|&(_, &n)| n >= RIP_MOST)
                        .map(|(k, _)| k.clone()),
                );
                let cands = candidates(self.board, &state, &exclude, Some(&self.ask.rip));
                match self.in_the_way(&state, &m, false, &cands)? {
                    Found::Way(more, way) => {
                        state = strip(self.board, &state, &more);
                        add(self.board, &mut state, &m, &way);
                        for n in more {
                            *rips.entry(n.clone()).or_default() += 1;
                            next.insert(n);
                        }
                    }
                    Found::NoWay(_) => add(self.board, &mut state, &m, &again),
                }
            }
            queue = next.into_iter().collect();
        }
        out.ripped = rips.keys().cloned().collect();
        // A ripped net is left no more open than it was.
        let (after, _) = with_routes(self.board, &state)?;
        let (was, now) = (Ratsnest::of(&real), Ratsnest::of(&after));
        let count = |r: &Ratsnest, n: &str| r.lines.iter().filter(|l| l.net == n).count();
        for m in &out.ripped {
            let (a, b) = (count(&was, m), count(&now, m));
            if b > a {
                return Ok(Err(format!(
                    "net {m}, ripped up for net {net}, could not be routed again: {b} of its connections would be open where {a} were, so nothing is written"
                )));
            }
        }
        if !self.open(&after, net, true).is_empty() {
            return Ok(Err(no_way(net)));
        }
        Ok(Ok(Some(state)))
    }

    /// The connections of `net` open on `board` (the asked connection's alone, when `cut`).
    fn open(&self, board: &Board, net: &str, cut: bool) -> Vec<Ratline> {
        let mut b = board.clone();
        if cut {
            self.cut(&mut b, net);
        }
        Ratsnest::of(&b)
            .lines
            .into_iter()
            .filter(|l| l.net == net)
            .collect()
    }

    /// The asked connection's net with only its two pads (the module's header).
    fn cut(&self, board: &mut Board, net: &str) {
        if let Some(pads) = &self.ask.connection
            && net == self.ask.net
            && let Some(n) = board.nets.iter_mut().find(|n| n.name == net)
        {
            n.pads.retain(|p| pads.contains(p));
        }
    }

    /// One run of the router: `net` round the copper of `routes` (cut to the asked connection
    /// when `cut`).
    fn route(&mut self, routes: &Routes, net: &str, cut: bool) -> Result<One, Refusal> {
        self.runs += 1;
        let (mut b, _) = with_routes(self.board, routes)?;
        if cut {
            self.cut(&mut b, net);
        }
        Ok(one::route_nets(&b, &BTreeSet::from([net.to_string()])))
    }

    /// The nets of `cands` whose copper stands in the way of `net` (the module's header).
    fn in_the_way(
        &mut self,
        routes: &Routes,
        net: &str,
        cut: bool,
        cands: &BTreeSet<String>,
    ) -> Result<Found, Refusal> {
        if cands.is_empty() || self.runs >= MOST_RUNS {
            return Ok(Found::NoWay(None));
        }
        let way = self.route(&strip(self.board, routes, cands), net, cut)?;
        if !way.complete() {
            return Ok(Found::NoWay(Some(way)));
        }
        let mut set = self.crossed(routes, cands, net, &way)?;
        let mut best = way;
        for m in set.clone() {
            if self.runs >= MOST_RUNS {
                break;
            }
            if !set.contains(&m) {
                continue;
            }
            let mut fewer = set.clone();
            fewer.remove(&m);
            let way = self.route(&strip(self.board, routes, &fewer), net, cut)?;
            if way.complete() {
                set = self.crossed(routes, &fewer, net, &way)?;
                best = way;
            }
        }
        Ok(Found::Way(set, best))
    }

    /// The nets of `taken` (whose unlocked copper `routes` has and the way was routed without)
    /// whose copper the way of `net` crosses.
    fn crossed(
        &self,
        routes: &Routes,
        taken: &BTreeSet<String>,
        net: &str,
        way: &One,
    ) -> Result<BTreeSet<String>, Refusal> {
        let board = self.board;
        let (bare, _) = with_routes(board, &strip(board, routes, taken))?;
        let ground = Ground::of(&bare)?;
        let (mut tracks, mut vias) = (Vec::new(), Vec::new());
        if let Some(table) = routes.boards.get(&board.name) {
            for n in taken {
                if let Some(r) = table.nets.get(n) {
                    tracks.extend(r.tracks.iter().filter(|t| !t.lock).map(|t| t.on(board, n)));
                    vias.extend(r.vias.iter().filter(|v| !v.lock).map(|v| v.on(board, n)));
                }
            }
        }
        let mut nets = BTreeSet::new();
        let mut note = |c: netsmith_router::walk::Crossed| {
            nets.extend(c.tracks.iter().map(|&i| tracks[i].net.clone()));
            nets.extend(c.vias.iter().map(|&i| vias[i].net.clone()));
        };
        for t in &way.tracks {
            note(ground.crossed(net, t.layer, t.width, &t.centre_line(), &tracks, &vias));
        }
        for v in &way.vias {
            for layer in bare.via_layers(v) {
                note(ground.crossed(net, layer, v.diameter, &[v.at], &tracks, &vias));
            }
        }
        Ok(nets)
    }
}

/// The nets of `routes` whose copper may be ripped (the module's header), less `exclude`, and
/// only those of `only` when given.
fn candidates(
    board: &Board,
    routes: &Routes,
    exclude: &BTreeSet<String>,
    only: Option<&BTreeSet<String>>,
) -> BTreeSet<String> {
    let Some(table) = routes.boards.get(&board.name) else {
        return BTreeSet::new();
    };
    table
        .nets
        .iter()
        .filter(|(n, _)| !exclude.contains(*n) && only.is_none_or(|o| o.contains(*n)))
        .filter(|(n, r)| {
            !r.lock
                && Some(&r.pads) == netsmith_pcb::pad_hash(board, n).as_ref()
                && (r.tracks.iter().any(|t| !t.lock) || r.vias.iter().any(|v| !v.lock))
        })
        .map(|(n, _)| n.clone())
        .collect()
}

/// `routes` with the unlocked tracks and vias of `nets` taken away.
fn strip(board: &Board, routes: &Routes, nets: &BTreeSet<String>) -> Routes {
    let mut out = routes.clone();
    if let Some(table) = out.boards.get_mut(&board.name) {
        for n in nets {
            if let Some(r) = table.nets.get_mut(n) {
                r.tracks.retain(|t| t.lock);
                r.vias.retain(|v| v.lock);
                if r.shapes() == 0 && !r.lock {
                    table.nets.remove(n);
                }
            }
        }
        if table.nets.is_empty() {
            out.boards.remove(&board.name);
        }
    }
    out
}

/// The copper of `way` added to `net`'s record in `routes`, as unlocked shapes.
fn add(board: &Board, routes: &mut Routes, net: &str, way: &One) {
    if way.tracks.is_empty() && way.vias.is_empty() {
        return;
    }
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
    let p = |[x, y]: [f64; 2]| [um(x), um(y)];
    for t in way.tracks.iter().filter(|t| t.net == net) {
        let mut r = RouteTrack::of(board, t);
        (r.from, r.to, r.mid) = (p(r.from), p(r.to), r.mid.map(p));
        record.tracks.push(r);
    }
    for v in way.vias.iter().filter(|v| v.net == net) {
        let mut r = RouteVia::of(board, v);
        r.at = p(r.at);
        record.vias.push(r);
    }
}

/// What walls in each line of `why`, in the routes file's places.
fn walls(board: &Board, why: &[LineReport]) -> Vec<Wall> {
    why.iter()
        .map(|r| Wall {
            line: r.line.to_string(),
            nets: r.blockers.clone(),
            pads: r.pads.iter().map(ToString::to_string).collect(),
            vias: r
                .vias
                .iter()
                .map(|v| {
                    let at = v.at - board.outline.min;
                    format!("net {} at ({}, {})", v.net, um(at.x), um(at.y))
                })
                .collect(),
            edge: r.edge,
        })
        .collect()
}

fn no_way(net: &str) -> String {
    format!(
        "net {net} has no way, even with the copper of every net that may be ripped taken away: what walls it in is locked copper, pads or the edge"
    )
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

fn list(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(",")
}

impl Routed {
    /// The text for a person.
    pub fn text(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let what = match &self.connection {
            Some(c) => format!("net {} ({c})", self.net),
            None => format!("net {}", self.net),
        };
        if self.joined {
            let _ = writeln!(s, "{what} is joined already: nothing to route");
            return s;
        }
        if !self.in_the_way.is_empty() {
            let set: BTreeSet<String> = self.in_the_way.iter().cloned().collect();
            let _ = writeln!(s, "in the way of {what}: the copper of {}", nets_said(&set));
        }
        for w in &self.walls {
            let mut by: Vec<String> = Vec::new();
            if !w.pads.is_empty() {
                by.push(format!("pad(s) {}", w.pads.join(", ")));
            }
            if !w.vias.is_empty() {
                by.push(format!("via(s) of {}", w.vias.join(", ")));
            }
            if !w.nets.is_empty() {
                let set: BTreeSet<String> = w.nets.iter().cloned().collect();
                by.push(format!("the copper of {}", nets_said(&set)));
            }
            if w.edge {
                by.push("the board's edge".to_string());
            }
            let by = if by.is_empty() {
                "nothing it can name (a keepout?)".to_string()
            } else {
                by.join("; ")
            };
            let _ = writeln!(s, "{}: walled in by {by}", w.line);
        }
        if self.refused.is_none() {
            for (k, n) in self.nets.iter().enumerate() {
                let layers: BTreeSet<&str> = n.tracks.iter().map(|t| t.layer.word()).collect();
                let on = if layers.is_empty() {
                    String::new()
                } else {
                    format!(
                        " on {}",
                        layers.into_iter().collect::<Vec<_>>().join(" and ")
                    )
                };
                let removed = if n.removed_tracks + n.removed_vias > 0 {
                    format!(
                        "ripped up ({} track(s) and {} via(s) removed), ",
                        n.removed_tracks, n.removed_vias
                    )
                } else {
                    String::new()
                };
                let head = if k == 0 {
                    what.clone()
                } else {
                    format!("net {}", n.net)
                };
                let _ = writeln!(
                    s,
                    "{head}: {removed}routed with {} track(s) and {} via(s){on}{}",
                    n.tracks.len(),
                    n.vias.len(),
                    if n.left.is_empty() {
                        String::new()
                    } else {
                        format!(", {} connection(s) still open", n.left.len())
                    }
                );
            }
            if let Some(c) = &self.narrowest {
                let most = c.fits >= c.width + one::MOST_ROOM - 1e-9;
                let _ = writeln!(
                    s,
                    "the narrowest place of its way, at ({}, {}) on {}, has room for a track of {}{} mm (it is {} mm)",
                    c.at[0],
                    c.at[1],
                    c.layer,
                    if most { "at least " } else { "" },
                    c.fits,
                    c.width
                );
            }
        }
        let _ = writeln!(
            s,
            "{} run(s) of the router, {} round(s) of routing ripped nets again",
            self.runs, self.rounds
        );
        s
    }
}
