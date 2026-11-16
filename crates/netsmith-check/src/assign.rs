//! Pin requests and their assignment (ADR 0101).
//!
//! A board asks for a pin by capability, `.status: .u.GPIO`, and does not name the pin. This
//! module gives each request a pin of the part, before the nets are made, so that every later
//! check sees the board as if it had been written with the pin: the request's pin is joined to
//! the part's pin, and the function is chosen on it (`.u.p02.function &= GPIO`).
//!
//! The choice is a search, and it is deterministic. A request that has an entry in the pin file
//! that still holds keeps it. The others are searched in the byte order of their paths, each on
//! the pins that offer its capability in the byte order of the pins' paths, taking the first
//! solution of a depth-first search: the least pin map in that order. Nothing depends on the
//! order of rows in a part's table or on a hash. A pin that the source names (it is connected,
//! or its function is chosen) and a pin of another entry are never given away.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use netsmith_lang::Span;
use netsmith_lang::ast::{Decl, DefKind};

use crate::checker::Checker;
use crate::lower::closest;
use crate::model::{
    BusAssignment, InstId, InstKind, Member, Model, PinAssignment, PinRequest, PinRow, Pinout,
};
use crate::pinfile::{BusEntry, EntrySource, PinEntry, PinFile};

/// How many steps of the search one part may take before it says so and gives up.
const SEARCH_STEPS: usize = 200_000;

/// What one board's (or system's) elaboration decided about pins: the entries its pin file
/// table should hold now, and which requests it has.
#[derive(Debug, Clone, Default)]
pub struct RootPins {
    /// The definition's name, the key of its table in the pin file.
    pub board: String,
    /// The entry of each request that has a pin, by the request's path.
    pub entries: BTreeMap<String, PinEntry>,
    /// The entry of each request for the end of a bus that has its peripheral, by the request's
    /// path (#788).
    pub buses: BTreeMap<String, BusEntry>,
    /// The path of every request of the definition, whether it got a pin or not.
    pub requests: BTreeSet<String>,
    /// What the pin optimizer may move, for each part that has requests.
    pub spaces: Vec<PartSpace>,
    /// The requests that got no pin, in the byte order of their paths, with what the check said.
    pub unserved: Vec<Unserved>,
}

/// A request that got no pin, and why: the message the check reported at the request (or at
/// the part or the function it names). Empty when the check said nothing of this request in
/// particular (the part's own check says why its table does not resolve).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unserved {
    /// The request's path, the key of its entry in the pin file.
    pub key: String,
    /// The part's path in the board, as the request names it (`u`, `hub.mcu`), whether or not
    /// such a part exists.
    pub part: String,
    /// The function the request asks for.
    pub capability: String,
    pub message: String,
}

/// What a run decided about the pin file: the file as it should be now, and how it differs from
/// the one that was read.
#[derive(Debug, Clone, Default)]
pub struct PinsOutcome {
    /// The pin file after this run: every entry that holds, and the ones the run made.
    pub file: PinFile,
    /// The file that was read, if there was one.
    pub existing: Option<PinFile>,
    /// What the run changes in the file, one line each: a request that got a pin, an entry that
    /// moved, an entry that goes. Empty when the file is as it should be.
    pub changes: Vec<String>,
    /// What the file says that is stale and does not stop the run, with its place: an entry that
    /// no longer holds and is picked again, an entry of a request that is gone.
    pub findings: Vec<String>,
    /// For each board or system of the source that has requests, by definition name: what the
    /// pin optimizer may move in each of its parts (see [`PartSpace`]).
    pub spaces: BTreeMap<String, Vec<PartSpace>>,
    /// For each board or system of the source, by definition name: the requests that got no
    /// pin, each with the message the check gave. A run with any of them fails, so this is
    /// what a person sees beside the problems.
    pub unserved: BTreeMap<String, Vec<Unserved>>,
}

/// What one part leaves open to the pin optimizer (ADR 0101): the requests that ask it for a
/// pin, which pins each may have, and what a choice of pins must keep. It is plain data, so
/// that a later stage that knows the layout can try choices without elaborating the board
/// again, and it holds the same rules as the assignment: a pin that the source holds never
/// moves, and a choice of pins keeps every `shared` limit of the part.
#[derive(Debug, Clone)]
pub struct PartSpace {
    /// The part's path in the board (`u`), as `Part::path` of the design has it.
    pub path: String,
    /// The requests of the part, in the byte order of their keys.
    pub requests: Vec<SpaceRequest>,
    /// The pins the source holds (it connects them or chooses their function), by path in the
    /// part: no request may have one.
    pub taken: BTreeSet<String>,
    shares: Vec<ShareData>,
    connected: BTreeSet<InstId>,
    /// The functions the source chose, by pin.
    fixed: Vec<(InstId, String)>,
    pins: BTreeMap<String, InstId>,
}

/// A request of a [`PartSpace`] and the pin it has now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceRequest {
    /// The request's path, the key of its entry in the pin file.
    pub key: String,
    /// The function it asks for.
    pub capability: String,
    /// The pin it has, by path in the part (`p02`).
    pub pin: String,
    /// A locked entry never moves.
    pub lock: bool,
    /// Every pin of the part that offers the capability and that the source does not hold, in
    /// the byte order of the paths.
    pub offers: Vec<String>,
    /// The channel of the capability on each pin of `offers` whose row names one (`p03` has
    /// `AIN1` for `ANALOG`, `fn ANALOG class AIN1 of ANALOG`, #787), by pin path.
    pub channels: BTreeMap<String, String>,
    /// For a line of a request for the end of a bus (`dbg.tx` of `.dbg: .u.Uart`, #788): the
    /// request and the peripheral it got. Such a line moves only with the whole request, so the
    /// optimizer and the editor's moves leave it where it is.
    pub bus: Option<BusLine>,
}

/// The request for the end of a bus that a [`SpaceRequest`] is a line of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusLine {
    /// The request's path, the key of its entry in the pin file (`dbg`).
    pub request: String,
    /// The peripheral it got (`UARTE1`).
    pub peripheral: String,
    /// The line (`tx`).
    pub line: String,
}

impl SpaceRequest {
    /// The function the pin file and the firmware exports record for the request on `pin`:
    /// the channel of the capability there, when the row names one, else the capability.
    pub fn function_on(&self, pin: &str) -> &str {
        self.channels.get(pin).unwrap_or(&self.capability)
    }
}

impl PartSpace {
    /// Whether a choice of pins is allowed: each request of `picks` (a key and a pin path) has a
    /// pin that offers what it asks for and the source does not hold, no pin is used twice, and
    /// no `shared` resource of the part is used past its limit.
    pub fn allows(&self, picks: &[(&str, &str)]) -> bool {
        let mut chosen = self.fixed.clone();
        let mut used: BTreeSet<&str> = BTreeSet::new();
        for (key, pin) in picks {
            let Some(r) = self.requests.iter().find(|r| r.key == *key) else {
                return false;
            };
            if !r.offers.iter().any(|o| o == pin) || !used.insert(pin) {
                return false;
            }
            chosen.push((self.pins[*pin], r.capability.clone()));
        }
        shares_conflict_in(&self.shares, &self.connected, &chosen).is_none()
    }
}

impl PinsOutcome {
    /// Whether the file on disk would change: it is missing and there are entries, or what it
    /// says differs.
    pub fn changed(&self) -> bool {
        !self.changes.is_empty()
    }
}

/// Who holds a pin that a request cannot have.
#[derive(Debug, Clone)]
enum Holder {
    /// The source: the pin is connected or has its function chosen.
    Source(String),
    /// An entry of the pin file that holds.
    Entry {
        key: String,
        locked: bool,
        place: Option<String>,
    },
    /// A request that is served in this run, which comes first in path order.
    New(String),
}

impl Holder {
    fn text(&self) -> String {
        match self {
            Self::Source(d) => format!("the source ({d})"),
            Self::Entry { key, locked, place } => {
                let mut what = Vec::new();
                if *locked {
                    what.push("locked".to_string());
                }
                if let Some(p) = place {
                    what.push(p.clone());
                }
                if what.is_empty() {
                    format!("`{key}`")
                } else {
                    format!("`{key}` ({})", what.join(", "))
                }
            }
            Self::New(key) => format!("`{key}` (new)"),
        }
    }
}

/// A request that resolved to a part and a capability the part offers.
struct Asked {
    index: usize,
    key: String,
    part: InstId,
    capability: String,
}

/// What is known about one part while its requests are served.
struct PartCtx {
    part: InstId,
    path: String,
    ty: String,
    /// The pins of the part's table that the request can have, with their path in the part.
    rows: Vec<(InstId, String)>,
    /// The pins the source connects (a `shared` pin member is in use by that).
    connected: BTreeSet<InstId>,
}

impl Checker<'_> {
    /// Record a request for a pin that the declaration `d` makes in `block`.
    pub(crate) fn record_request(&mut self, block: InstId, pin: InstId, d: &Decl) {
        if self.in_kind(DefKind::Part)
            || self.in_kind(DefKind::Interface)
            || self.in_kind(DefKind::Generic)
        {
            let what = if self.in_kind(DefKind::Part) {
                "a part"
            } else if self.in_kind(DefKind::Interface) {
                "an interface"
            } else {
                "a generic"
            };
            self.error(
                d.span,
                format!(
                    "a pin request goes in a board or a module, where it asks a part declared beside it for a pin; {what} has no such part"
                ),
            );
            return;
        }
        let (capability, target) =
            d.ty.segments
                .split_last()
                .expect("a request has a part and a capability");
        let first = target.first().expect("a request has a part").span;
        let last = target.last().expect("a request has a part").span;
        let source = self.cur_source();
        self.model.requests.push(PinRequest {
            pin,
            block,
            target: target.iter().map(|s| s.name.clone()).collect(),
            capability: capability.name.clone(),
            source,
            span: d.span,
            target_span: Span::new(first.start, last.end),
            capability_span: capability.span,
        });
    }

    /// Give every request of the model a pin. `name` is the definition's name; `keep` says
    /// whether its table of the pin file counts (a board or a system has one, a module read on
    /// its own has not). Joins the request's pin to the part's pin and chooses the function on
    /// it. Mistakes are reported; the pins that could be given are given.
    pub(crate) fn assign_pins(&mut self, name: &str, keep: bool) {
        let requests = self.model.requests.clone();
        let mut root = RootPins {
            board: name.to_string(),
            ..RootPins::default()
        };
        let old: BTreeMap<String, PinEntry> = if keep {
            self.pin_input
                .as_ref()
                .and_then(|p| p.file.board(name).cloned())
                .unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        let old_buses: BTreeMap<String, BusEntry> = if keep {
            self.pin_input
                .as_ref()
                .and_then(|p| p.file.buses_of(name).cloned())
                .unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        if requests.is_empty() && self.model.bus_requests.is_empty() {
            if keep {
                self.pin_roots.push(root);
            }
            return;
        }

        // Which pins the source connects, from the groups the `~` statements made so far.
        let connected = self.connected_pins();

        // Resolve each request to its part, then serve the requests of one part together.
        let mut asked: Vec<Asked> = Vec::new();
        let mut order: Vec<usize> = (0..requests.len()).collect();
        order.sort_by_key(|&i| self.model.path_of(requests[i].pin));
        for i in order {
            let key = self.model.path_of(requests[i].pin);
            root.requests.insert(key.clone());
            if let Some((part, capability)) = self.resolve_request(&requests[i], &key) {
                asked.push(Asked {
                    index: i,
                    key,
                    part,
                    capability,
                });
            }
        }
        // The requests for the end of a bus (#788), the same way.
        let mut buses: Vec<Asked> = Vec::new();
        for i in 0..self.model.bus_requests.len() {
            let key = self.bus_key(i);
            root.requests.insert(key.clone());
            if let Some(part) = self.resolve_bus_request(i, &key) {
                buses.push(Asked {
                    index: i,
                    key,
                    part,
                    capability: self.model.bus_requests[i].interface.clone(),
                });
            }
        }
        buses.sort_by(|a, b| a.key.cmp(&b.key));
        let mut parts: Vec<InstId> = asked.iter().chain(&buses).map(|a| a.part).collect();
        parts.sort_by_key(|&p| self.model.path_of(p));
        parts.dedup();
        let olds = (&old, &old_buses);
        for part in parts {
            let mine: Vec<&Asked> = asked.iter().filter(|a| a.part == part).collect();
            let bus_mine: Vec<&Asked> = buses.iter().filter(|a| a.part == part).collect();
            self.serve_part(
                part, &mine, &bus_mine, &requests, olds, &connected, &mut root,
            );
        }
        root.unserved = self.unserved_of(&requests, &root);
        if keep {
            self.pin_roots.push(root);
        }
    }

    /// The requests that have no entry in `root` after they were served, each with the message
    /// the check reported at its declaration, at the part it names or at its function.
    fn unserved_of(&self, requests: &[PinRequest], root: &RootPins) -> Vec<Unserved> {
        let model = &self.model;
        let block_path = |r: &PinRequest| model.path_of(r.block);
        let mut out: Vec<Unserved> = requests
            .iter()
            .filter(|r| !root.entries.contains_key(&model.path_of(r.pin)))
            .map(|r| {
                let at = [r.span, r.target_span, r.capability_span];
                let message = self
                    .diags
                    .iter()
                    .find(|d| d.source == r.source && at.contains(&d.span))
                    .map(|d| d.message.clone())
                    .unwrap_or_default();
                let block = block_path(r);
                let target = r.target.join(".");
                Unserved {
                    key: model.path_of(r.pin),
                    part: if block.is_empty() {
                        target
                    } else {
                        format!("{block}.{target}")
                    },
                    capability: r.capability.clone(),
                    message,
                }
            })
            .collect();
        for (i, r) in model.bus_requests.iter().enumerate() {
            let key = self.bus_key(i);
            if root.buses.contains_key(&key) {
                continue;
            }
            let at = [r.span, r.target_span, r.capability_span];
            let message = self
                .diags
                .iter()
                .find(|d| d.source == r.source && at.contains(&d.span))
                .map(|d| d.message.clone())
                .unwrap_or_default();
            let block = model.path_of(r.block);
            let target = r.target.join(".");
            out.push(Unserved {
                key,
                part: if block.is_empty() {
                    target
                } else {
                    format!("{block}.{target}")
                },
                capability: r.interface.clone(),
                message,
            });
        }
        out.sort_by(|a, b| a.key.cmp(&b.key));
        out
    }

    /// The path of a request for the end of a bus, the key of its entry: its block's path and
    /// its name (`dbg`, `hub.dbg`).
    fn bus_key(&self, index: usize) -> String {
        let r = &self.model.bus_requests[index];
        let block = self.model.path_of(r.block);
        if block.is_empty() {
            r.name.clone()
        } else {
            format!("{block}.{}", r.name)
        }
    }

    /// The part a request for the end of a bus asks, if the request is sound: the part exists,
    /// has a pin table, and some peripheral of it is an end of the bus.
    fn resolve_bus_request(&mut self, index: usize, key: &str) -> Option<InstId> {
        let r = self.model.bus_requests[index].clone();
        let as_pin = PinRequest {
            pin: r.block,
            block: r.block,
            target: r.target.clone(),
            capability: r.interface.clone(),
            source: r.source,
            span: r.span,
            target_span: r.target_span,
            capability_span: r.capability_span,
        };
        let part = self.resolve_part(&as_pin, key)?;
        if self.bus_ends(part, &r.interface).is_empty() {
            let ty = self.model.instance(part).type_name.clone();
            let message = format!(
                "`{key}` asks `{}` for the end of a `{}`, but no peripheral of `{ty}` is one",
                r.target.join("."),
                r.interface
            );
            self.report(r.source, r.capability_span, message);
            return None;
        }
        Some(part)
    }

    /// The part a request asks and the capability it asks for, if the request is sound:
    /// the part exists, has a pin table, and some pin of it offers the capability.
    fn resolve_request(&mut self, req: &PinRequest, key: &str) -> Option<(InstId, String)> {
        let part = self.resolve_part(req, key)?;
        let model = &self.model;
        let ty = model.instance(part).type_name.clone();
        let Pinout::Table(table) = model.pinout(part) else {
            return None;
        };
        if table
            .rows
            .iter()
            .any(|r| model.offers(part, r, &req.capability))
        {
            return Some((part, req.capability.clone()));
        }
        let message = self.nothing_offers(req, part, table.rows.as_slice(), key, &ty);
        self.report(req.source, req.capability_span, message);
        None
    }

    /// The part a request asks, if it exists and has a pin table for the board's package.
    fn resolve_part(&mut self, req: &PinRequest, key: &str) -> Option<InstId> {
        let model = &self.model;
        let target = req.target.join(".");
        let mut inst = req.block;
        for name in &req.target {
            match model.member(inst, name) {
                Some(Member::Inst(child)) => inst = child,
                _ => {
                    let here = model.instance(inst);
                    let known: Vec<String> = here
                        .children
                        .iter()
                        .filter(|(_, m)| {
                            matches!(m, Member::Inst(i) if model.instance(*i).kind == InstKind::Part)
                        })
                        .map(|(n, _)| n.clone())
                        .collect();
                    let mut message = format!(
                        "`{key}` asks `{target}` for {}, but `{name}` is not a part declared here",
                        req.capability
                    );
                    match closest(name, &known) {
                        Some(near) => message.push_str(&format!("; did you mean `{near}`?")),
                        None if !known.is_empty() => {
                            let known: Vec<String> =
                                known.iter().map(|n| format!("`{n}`")).collect();
                            message
                                .push_str(&format!("; the parts here are {}", list_plain(&known)));
                        }
                        None => {}
                    }
                    self.report(req.source, req.target_span, message);
                    return None;
                }
            }
        }
        let part = inst;
        let (kind, ty) = {
            let i = model.instance(part);
            (i.kind, i.type_name.clone())
        };
        if kind != InstKind::Part {
            let message = format!(
                "`{key}` asks `{target}` for {}, but `{target}` is {} `{ty}`, not a part; a request asks a part that has a pin table",
                req.capability,
                kind.word()
            );
            self.report(req.source, req.target_span, message);
            return None;
        }
        match model.pinout(part) {
            Pinout::Table(_) => Some(part),
            Pinout::NoTables => {
                let message = format!(
                    "`{key}` asks `{target}` for {}, but `{ty}` has no pin table, so there is no pin of it to give",
                    req.capability
                );
                self.report(req.source, req.target_span, message);
                None
            }
            Pinout::Unknown { packages } => {
                let packages: Vec<String> = packages.iter().map(|p| format!("`{p}`")).collect();
                let message = format!(
                    "`{key}` asks `{target}` for {}, but the pins of `{ty}` depend on its package, which the board does not state; state it (`.{target}.package &= ...`), one of {}",
                    req.capability,
                    list_plain(&packages)
                );
                self.report(req.source, req.target_span, message);
                None
            }
            // The part's own check says that the package has no table.
            Pinout::NoSuchPackage { .. } => None,
        }
    }

    /// The message for a capability that no pin of the part offers.
    fn nothing_offers(
        &self,
        req: &PinRequest,
        part: InstId,
        rows: &[PinRow],
        key: &str,
        ty: &str,
    ) -> String {
        let model = &self.model;
        let target = req.target.join(".");
        let cap = &req.capability;
        let inst = model.instance(part);
        // The functions the rows list, then the lines of the peripherals that are on them.
        let mut listed: Vec<String> = Vec::new();
        for r in rows {
            for f in &r.functions {
                if !listed.contains(&f.name) {
                    listed.push(f.name.clone());
                }
            }
        }
        for r in rows {
            for p in model.class_peripherals(part, r) {
                for f in &p.functions {
                    if !listed.contains(&f.name) {
                        listed.push(f.name.clone());
                    }
                }
            }
        }
        let head = format!(
            "`{key}` asks for {cap} on `{target}` (`{ty}`), and no pin of `{ty}` offers it"
        );
        if let Some(p) = inst.rules.peripherals.iter().find(|p| p.name == *cap) {
            let own: Vec<String> = p.functions.iter().map(|f| f.name.clone()).collect();
            return format!(
                "{head}; `{cap}` is a peripheral, not a function: ask for one of its functions, {}",
                plain_list(&own)
            );
        }
        // A request for the end of a bus of a part that has no such end (#788).
        if self.modules.iter().any(|m| {
            m.defs
                .iter()
                .any(|d| d.kind == DefKind::Interface && d.name.name == *cap)
        }) {
            return format!(
                "`{key}` asks `{target}` (`{ty}`) for the end of a `{cap}`, but no peripheral of `{ty}` is one; a part says that a peripheral is an end of a bus with `peripheral NAME as {cap} {{ ... }}` (ADR 0064)"
            );
        }
        let mut message = head;
        match closest(cap, &listed) {
            Some(near) => message.push_str(&format!("; did you mean `{near}`? ")),
            None => message.push_str("; "),
        }
        if listed.is_empty() {
            message.push_str("its rows list no functions");
        } else {
            message.push_str(&format!(
                "the functions its rows and peripherals offer are {}",
                plain_list(&listed)
            ));
        }
        message
    }

    /// The pins that something else joins: they share a group of the `~` statements with a
    /// pin that is not joined to them inside their part, or ask for a net by name. A pin of a
    /// part is in use of the board by this, as `Model::is_connected` has it once the nets exist.
    fn connected_pins(&mut self) -> BTreeMap<InstId, String> {
        let pins: Vec<InstId> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Pin)
            .map(|(id, _)| id)
            .collect();
        let mut groups: HashMap<usize, Vec<InstId>> = HashMap::new();
        for &pin in &pins {
            let root = self.find(pin.0);
            groups.entry(root).or_default().push(pin);
        }
        let mut found = BTreeMap::new();
        for &pin in &pins {
            let root = self.find(pin.0);
            let inside = self.model.joined_inside(pin);
            let mut others: Vec<String> = groups[&root]
                .iter()
                .filter(|&&p| p != pin && !inside.contains(&p))
                .map(|&p| self.model.path_of(p))
                .collect();
            others.sort();
            if let Some(first) = others.first() {
                found.insert(pin, format!("connected to `{first}`"));
            } else if let Some(net) = self.requested_name(pin) {
                found.insert(pin, format!("on the net `{net}`"));
            }
        }
        found
    }

    /// Serve the requests of one part: keep the entries that hold, search for the rest. The
    /// entries of requests for a pin that hold are kept first; then the requests for the end of a
    /// bus are served (#788), as they need several pins of one peripheral; then the other
    /// requests for a pin, round them.
    #[allow(clippy::too_many_arguments)]
    fn serve_part(
        &mut self,
        part: InstId,
        mine: &[&Asked],
        bus_mine: &[&Asked],
        requests: &[PinRequest],
        (old, old_buses): (&BTreeMap<String, PinEntry>, &BTreeMap<String, BusEntry>),
        connected: &BTreeMap<InstId, String>,
        root: &mut RootPins,
    ) {
        let Pinout::Table(table) = self.model.pinout(part) else {
            return;
        };
        let rows_all: Vec<PinRow> = table.rows.clone();
        let ctx = {
            let model = &self.model;
            let mut rows: Vec<(InstId, String)> = rows_all
                .iter()
                .map(|r| (r.pin, model.path_below(r.pin, part)))
                .collect();
            rows.sort_by_key(|a| model.path_of(a.0));
            PartCtx {
                part,
                path: model.path_of(part),
                ty: model.instance(part).type_name.clone(),
                rows,
                connected: connected.keys().copied().collect(),
            }
        };

        // What the source already holds: a pin that is connected or has its function chosen.
        let mut occupied: BTreeMap<InstId, Holder> = BTreeMap::new();
        let mut fixed: Vec<(InstId, String)> = Vec::new();
        {
            let model = &self.model;
            for (pin, _) in &ctx.rows {
                let function = model.function_of(*pin);
                if let Some(f) = &function {
                    fixed.push((*pin, f.clone()));
                }
                let mut how: Vec<String> = Vec::new();
                if let Some(c) = connected.get(pin) {
                    how.push(c.clone());
                }
                if let Some(f) = &function {
                    let at = model
                        .function_stated_at(*pin)
                        .and_then(|(s, sp)| self.sources.place(s, sp))
                        .map(|p| format!(" at {p}"))
                        .unwrap_or_default();
                    how.push(format!("function {f} chosen{at}"));
                }
                let how = (!how.is_empty()).then(|| how.join(", "));
                if let Some(how) = how {
                    occupied.insert(*pin, Holder::Source(how));
                }
            }
        }
        let source_fixed = fixed.clone();
        let source_taken: BTreeSet<InstId> = occupied.keys().copied().collect();
        // The entries that still hold are kept; the others are told and picked again.
        let pin_source = self.pin_input.as_ref().map(|p| p.source);
        let mut kept: Vec<(&Asked, PinAssignment)> = Vec::new();
        let mut todo: Vec<&Asked> = Vec::new();
        // Locked entries are served first, in key order, then the others: a lock wins a pin over
        // an unlocked entry whose key sorts earlier, which is then moved or dropped with a note.
        // (The sort is stable, so each group stays in key order.)
        let mut in_order: Vec<&Asked> = mine.to_vec();
        in_order.sort_by_key(|a| !old.get(&a.key).is_some_and(|e| e.lock));
        for a in in_order {
            let Some(entry) = old.get(&a.key) else {
                todo.push(a);
                continue;
            };
            let verdict = self.entry_holds(&ctx, a, entry, &rows_all, &occupied, &fixed);
            match verdict {
                Ok(pin) => {
                    occupied.insert(
                        pin,
                        Holder::Entry {
                            key: a.key.clone(),
                            locked: entry.lock,
                            place: self.entry_place(entry),
                        },
                    );
                    fixed.push((pin, a.capability.clone()));
                    kept.push((
                        a,
                        PinAssignment {
                            request: requests[a.index].pin,
                            part,
                            pin,
                            function: a.capability.clone(),
                            key: a.key.clone(),
                            lock: entry.lock,
                            source: entry.source,
                        },
                    ));
                }
                Err(reason) => {
                    let place = self.entry_place(entry);
                    let at = place.as_ref().map(|p| format!("{p}: ")).unwrap_or_default();
                    let where_ = format!("`{}.{}`", ctx.path, entry.pin);
                    if entry.lock {
                        let message = format!(
                            "`{}` is locked to {where_} in the pin file, but {reason}; change the source, or unlock the entry so that a pin is picked again",
                            a.key
                        );
                        if let (Some(src), Some(span)) = (pin_source, entry.at) {
                            self.report(src, span, message);
                        } else {
                            self.report(requests[a.index].source, requests[a.index].span, message);
                        }
                    } else {
                        self.pin_findings.push(format!(
                            "{at}`{}` was {where_}, but {reason}; a pin is picked again",
                            a.key
                        ));
                        todo.push(a);
                    }
                }
            }
        }
        todo.sort_by(|a, b| a.key.cmp(&b.key));

        // The requests for the end of a bus, round the entries that hold.
        let pending: Vec<String> = todo.iter().map(|a| a.capability.clone()).collect();
        let served_buses = self.serve_buses(
            &ctx,
            bus_mine,
            old_buses,
            &rows_all,
            (&mut occupied, &mut fixed),
            &kept,
            &pending,
        );

        // The search, for what is left.
        let mut picked: Vec<(&Asked, InstId)> = Vec::new();
        let mut errors: Vec<(usize, String)> = Vec::new();
        loop {
            if todo.is_empty() {
                break;
            }
            match self.search(&ctx, &todo, &occupied, &fixed, &rows_all, &kept) {
                Search::Found(pins) => {
                    for (a, pin) in todo.iter().zip(pins) {
                        picked.push((a, pin));
                    }
                    break;
                }
                Search::Failed { index, message } => {
                    errors.push((todo[index].index, message));
                    todo.remove(index);
                }
            }
        }
        for (index, message) in errors {
            let r = &requests[index];
            self.report(r.source, r.span, message);
        }

        // Apply: the function is chosen on the part's pin, the request's pin joins it.
        let mut applied: Vec<PinAssignment> = kept.into_iter().map(|(_, p)| p).collect();
        for (a, pin) in picked {
            applied.push(PinAssignment {
                request: requests[a.index].pin,
                part,
                pin,
                function: a.capability.clone(),
                key: a.key.clone(),
                lock: false,
                source: EntrySource::Auto,
            });
        }
        applied.sort_by(|a, b| a.key.cmp(&b.key));
        {
            let model = &self.model;
            let mut requests_of_part: Vec<SpaceRequest> = applied
                .iter()
                .map(|p| {
                    let mut channels = BTreeMap::new();
                    let offers = ctx
                        .rows
                        .iter()
                        .filter(|(pin, _)| !source_taken.contains(pin))
                        .filter(|(pin, path)| {
                            let row = rows_all.iter().find(|r| r.pin == *pin).expect("a row");
                            if let Some(channel) = row.channel_of(&p.function) {
                                channels.insert(path.clone(), channel.to_string());
                            }
                            model.offers(part, row, &p.function)
                        })
                        .map(|(_, path)| path.clone())
                        .collect();
                    SpaceRequest {
                        key: p.key.clone(),
                        capability: p.function.clone(),
                        pin: model.path_below(p.pin, part),
                        lock: p.lock,
                        offers,
                        channels,
                        bus: None,
                    }
                })
                .collect();
            // Each line of a request for the end of a bus, which moves only with its request.
            for b in &served_buses {
                let peripheral = model.instance(part).rules.peripherals[b.peripheral]
                    .name
                    .clone();
                for (line, pin, function) in &b.lines {
                    let offers = ctx
                        .rows
                        .iter()
                        .filter(|(p, _)| !source_taken.contains(p))
                        .filter(|(p, _)| {
                            let row = rows_all.iter().find(|r| r.pin == *p).expect("a row");
                            model.offers(part, row, function)
                        })
                        .map(|(_, path)| path.clone())
                        .collect();
                    requests_of_part.push(SpaceRequest {
                        key: format!("{}.{line}", b.key),
                        capability: function.clone(),
                        pin: model.path_below(*pin, part),
                        lock: b.lock,
                        offers,
                        channels: BTreeMap::new(),
                        bus: Some(BusLine {
                            request: b.key.clone(),
                            peripheral: peripheral.clone(),
                            line: line.clone(),
                        }),
                    });
                }
            }
            requests_of_part.sort_by(|a, b| a.key.cmp(&b.key));
            if !requests_of_part.is_empty() {
                root.spaces.push(PartSpace {
                    path: ctx.path.clone(),
                    requests: requests_of_part,
                    taken: source_taken
                        .iter()
                        .map(|p| model.path_below(*p, part))
                        .collect(),
                    shares: share_data(model, part),
                    connected: ctx.connected.clone(),
                    fixed: source_fixed,
                    pins: ctx
                        .rows
                        .iter()
                        .map(|(id, path)| (path.clone(), *id))
                        .collect(),
                });
            }
        }
        for p in applied {
            let r = requests
                .iter()
                .find(|r| r.pin == p.request)
                .expect("a request of the part");
            if let Some(Member::Field(f)) = self.model.member(p.pin, "function") {
                let field = &mut self.model.fields[f.0];
                field.domain = crate::model::Domain::Text(p.function.clone());
                field.narrowed_at.push((r.source, r.span));
            }
            let (ra, rb) = (self.find(p.request.0), self.find(p.pin.0));
            if ra != rb {
                self.uf.insert(ra, rb);
            }
            // The entry records the channel of a class where the row names one (#787).
            let function = rows_all
                .iter()
                .find(|row| row.pin == p.pin)
                .and_then(|row| row.channel_of(&p.function))
                .unwrap_or(&p.function)
                .to_string();
            root.entries.insert(
                p.key.clone(),
                PinEntry {
                    pin: self.model.path_below(p.pin, part),
                    function,
                    lock: p.lock,
                    source: p.source,
                    at: None,
                },
            );
            self.model.pin_assignments.push(p);
        }
        // Apply the requests for the end of a bus: the function of each line is chosen on its
        // pin. The request joins as the peripheral when the joins of peripherals are made.
        for b in served_buses {
            let r = self.model.bus_requests[b.request].clone();
            let mut pins = BTreeMap::new();
            for (line, pin, function) in &b.lines {
                if let Some(Member::Field(f)) = self.model.member(*pin, "function") {
                    let field = &mut self.model.fields[f.0];
                    field.domain = crate::model::Domain::Text(function.clone());
                    field.narrowed_at.push((r.source, r.span));
                }
                pins.insert(line.clone(), self.model.path_below(*pin, part));
                self.model.pin_assignments.push(PinAssignment {
                    request: *pin,
                    part,
                    pin: *pin,
                    function: function.clone(),
                    key: format!("{}.{line}", b.key),
                    lock: b.lock,
                    source: b.source,
                });
            }
            root.buses.insert(
                b.key.clone(),
                BusEntry {
                    peripheral: self.model.instance(part).rules.peripherals[b.peripheral]
                        .name
                        .clone(),
                    pins,
                    lock: b.lock,
                    source: b.source,
                    at: None,
                },
            );
            self.model.bus_assignments.push(b);
        }
        self.model.pin_assignments.sort_by(|a, b| a.key.cmp(&b.key));
        self.model.bus_assignments.sort_by(|a, b| a.key.cmp(&b.key));
    }

    /// Serve the requests for the end of a bus of one part (#788): an entry that holds is
    /// kept, the others get the first peripheral, in the byte order of the names, that is an end
    /// of the bus and for whose lines the search finds pins. A peripheral that something else
    /// uses (a function of it is chosen on a pin, held by an entry, or asked for by a request for
    /// a pin that is still to be served) is never given. `pending` holds what those requests ask.
    #[allow(clippy::too_many_arguments)]
    fn serve_buses(
        &mut self,
        ctx: &PartCtx,
        bus_mine: &[&Asked],
        old: &BTreeMap<String, BusEntry>,
        rows: &[PinRow],
        (occupied, fixed): (&mut BTreeMap<InstId, Holder>, &mut Vec<(InstId, String)>),
        kept: &[(&Asked, PinAssignment)],
        pending: &[String],
    ) -> Vec<BusAssignment> {
        let mut out: Vec<BusAssignment> = Vec::new();
        let mut todo: Vec<&Asked> = Vec::new();
        let mut in_order: Vec<&Asked> = bus_mine.to_vec();
        in_order.sort_by_key(|a| !old.get(&a.key).is_some_and(|e| e.lock));
        let pin_source = self.pin_input.as_ref().map(|p| p.source);
        let take = |occupied: &mut BTreeMap<InstId, Holder>,
                    fixed: &mut Vec<(InstId, String)>,
                    lines: &[(String, InstId, String)],
                    holder: &Holder| {
            for (_, pin, function) in lines {
                occupied.insert(*pin, holder.clone());
                fixed.push((*pin, function.clone()));
            }
        };
        for a in in_order {
            let Some(entry) = old.get(&a.key) else {
                todo.push(a);
                continue;
            };
            match self.bus_entry_holds(ctx, a, entry, rows, occupied, fixed, pending) {
                Ok((peripheral, lines)) => {
                    let holder = Holder::Entry {
                        key: a.key.clone(),
                        locked: entry.lock,
                        place: self.bus_entry_place(entry),
                    };
                    take(occupied, fixed, &lines, &holder);
                    out.push(BusAssignment {
                        request: a.index,
                        key: a.key.clone(),
                        part: ctx.part,
                        peripheral,
                        lines,
                        lock: entry.lock,
                        source: entry.source,
                    });
                }
                Err(reason) => {
                    let r = &self.model.bus_requests[a.index];
                    let (r_source, r_span) = (r.source, r.span);
                    if entry.lock {
                        let message = format!(
                            "`{}` is locked to `{}` of `{}` in the pin file, but {reason}; change the source, or unlock the entry so that a peripheral is picked again",
                            a.key, entry.peripheral, ctx.path
                        );
                        match (pin_source, entry.at) {
                            (Some(src), Some(span)) => self.report(src, span, message),
                            _ => self.report(r_source, r_span, message),
                        }
                    } else {
                        let at = self
                            .bus_entry_place(entry)
                            .map(|p| format!("{p}: "))
                            .unwrap_or_default();
                        self.pin_findings.push(format!(
                            "{at}`{}` was `{}` of `{}`, but {reason}; a peripheral is picked again",
                            a.key, entry.peripheral, ctx.path
                        ));
                        todo.push(a);
                    }
                }
            }
        }
        todo.sort_by(|a, b| a.key.cmp(&b.key));
        for a in todo {
            let interface = a.capability.clone();
            let ends = self.bus_ends(ctx.part, &interface);
            let mut why: Vec<String> = Vec::new();
            let mut served = None;
            for index in ends {
                let name = self.model.instance(ctx.part).rules.peripherals[index]
                    .name
                    .clone();
                if let Some(used) = self.peripheral_in_use(ctx.part, index, fixed, pending) {
                    why.push(format!("`{name}` is in use ({used})"));
                    continue;
                }
                let lines = self.bus_lines(ctx.part, index);
                if lines.is_empty() {
                    why.push(format!("`{name}` names no line of the bus"));
                    continue;
                }
                let asked: Vec<Asked> = lines
                    .iter()
                    .map(|(line, function)| Asked {
                        index: a.index,
                        key: format!("{}.{line}", a.key),
                        part: ctx.part,
                        capability: function.clone(),
                    })
                    .collect();
                let refs: Vec<&Asked> = asked.iter().collect();
                match self.search(ctx, &refs, occupied, fixed, rows, kept) {
                    Search::Found(pins) => {
                        let picked: Vec<(String, InstId, String)> = lines
                            .iter()
                            .zip(pins)
                            .map(|((line, function), pin)| (line.clone(), pin, function.clone()))
                            .collect();
                        served = Some((index, picked));
                        break;
                    }
                    Search::Failed { message, .. } => why.push(format!("`{name}`: {message}")),
                }
            }
            match served {
                Some((peripheral, lines)) => {
                    take(occupied, fixed, &lines, &Holder::New(a.key.clone()));
                    out.push(BusAssignment {
                        request: a.index,
                        key: a.key.clone(),
                        part: ctx.part,
                        peripheral,
                        lines,
                        lock: false,
                        source: EntrySource::Auto,
                    });
                }
                None => {
                    let r = &self.model.bus_requests[a.index];
                    let (source, span) = (r.source, r.span);
                    let message = format!(
                        "`{}` asks `{}` for the end of a `{interface}`, and no peripheral of `{}` that is one can have a pin for each of its lines: {}",
                        a.key,
                        ctx.path,
                        ctx.ty,
                        why.join("; ")
                    );
                    self.report(source, span, message);
                }
            }
        }
        out.sort_by(|a, b| a.key.cmp(&b.key));
        out
    }

    /// Why a peripheral of the part cannot be given to a request for the end of its bus, if it
    /// cannot: a function of it is chosen on a pin (by the source or an entry that holds), or a
    /// request for a pin still to be served asks for one.
    fn peripheral_in_use(
        &self,
        part: InstId,
        index: usize,
        fixed: &[(InstId, String)],
        pending: &[String],
    ) -> Option<String> {
        let periph = &self.model.instance(part).rules.peripherals[index];
        for f in &periph.functions {
            if let Some((pin, _)) = fixed.iter().find(|(_, g)| *g == f.name) {
                return Some(format!("{} is on `{}`", f.name, self.model.path_of(*pin)));
            }
            if pending.contains(&f.name) {
                return Some(format!("a request asks for {}", f.name));
            }
        }
        None
    }

    /// The place of an entry of a request for the end of a bus in the pin file.
    fn bus_entry_place(&self, entry: &BusEntry) -> Option<String> {
        let src = self.pin_input.as_ref()?.source;
        self.sources.place(src, entry.at?)
    }

    /// Whether the entry of a request for the end of a bus still holds: its peripheral is an
    /// end of the bus that nothing else uses, the entry names the pin of each of its lines and
    /// no other, and each pin is in the part, offers the line's function, is held by nothing
    /// else, and keeps the part's `shared` limits. The peripheral and the lines when it does.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn bus_entry_holds(
        &self,
        ctx: &PartCtx,
        a: &Asked,
        entry: &BusEntry,
        rows: &[PinRow],
        occupied: &BTreeMap<InstId, Holder>,
        fixed: &[(InstId, String)],
        pending: &[String],
    ) -> Result<(usize, Vec<(String, InstId, String)>), String> {
        let model = &self.model;
        let ends = self.bus_ends(ctx.part, &a.capability);
        let peripherals = &model.instance(ctx.part).rules.peripherals;
        let Some(index) = ends
            .iter()
            .copied()
            .find(|&i| peripherals[i].name == entry.peripheral)
        else {
            let names: Vec<String> = ends
                .iter()
                .map(|&i| format!("`{}`", peripherals[i].name))
                .collect();
            return Err(format!(
                "`{}` (`{}`) has no peripheral `{}` that is an end of a `{}`; its ends are {}",
                ctx.path,
                ctx.ty,
                entry.peripheral,
                a.capability,
                list_plain(&names)
            ));
        };
        if let Some(used) = self.peripheral_in_use(ctx.part, index, fixed, pending) {
            return Err(format!("`{}` is in use ({used})", entry.peripheral));
        }
        let lines = self.bus_lines(ctx.part, index);
        let want: Vec<&String> = lines.iter().map(|(l, _)| l).collect();
        let have: Vec<&String> = entry.pins.keys().collect();
        if want != have {
            let show = |v: &[&String]| {
                let v: Vec<String> = v.iter().map(|l| format!("`{l}`")).collect();
                list_plain(&v)
            };
            return Err(format!(
                "the entry has the lines {} and `{}` has {}",
                show(&have),
                entry.peripheral,
                show(&want)
            ));
        }
        let mut out = Vec::new();
        let mut chosen = fixed.to_vec();
        for (line, function) in &lines {
            let at = &entry.pins[line];
            let Some((pin, _)) = ctx.rows.iter().find(|(_, p)| p == at) else {
                return Err(format!("`{}` (`{}`) has no pin `{at}`", ctx.path, ctx.ty));
            };
            let row = rows.iter().find(|r| r.pin == *pin).expect("a pin of a row");
            if !model.offers(ctx.part, row, function) {
                return Err(format!("`{}.{at}` does not offer {function}", ctx.path));
            }
            if let Some(holder) = occupied.get(pin) {
                return Err(format!("`{}.{at}` is held by {}", ctx.path, holder.text()));
            }
            if out.iter().any(|(_, p, _)| p == pin) {
                return Err(format!("`{}.{at}` is the pin of two lines", ctx.path));
            }
            chosen.push((*pin, function.clone()));
            out.push((line.clone(), *pin, function.clone()));
        }
        if let Some(c) = shares_conflict(model, ctx, &chosen) {
            return Err(c.sentence_with(&holders_of(model, occupied, None)));
        }
        Ok((index, out))
    }

    /// Put together what the elaborations decided: the new pin file, what differs from the old
    /// one, and the entries that are stale. `complete` says that every definition of the file was
    /// elaborated, so that an entry of a request that is not there is known to be stale.
    pub(crate) fn finish_pins(&mut self, complete: bool) -> PinsOutcome {
        let roots = std::mem::take(&mut self.pin_roots);
        let existing = self.pin_input.as_ref().map(|p| p.file.clone());
        let src = self.pin_input.as_ref().map(|p| p.source);
        let mut file = PinFile::default();
        let mut spaces: BTreeMap<String, Vec<PartSpace>> = BTreeMap::new();
        let mut unserved: BTreeMap<String, Vec<Unserved>> = BTreeMap::new();
        for r in &roots {
            if !r.unserved.is_empty() {
                unserved.insert(r.board.clone(), r.unserved.clone());
            }
            if !r.entries.is_empty() {
                file.boards.insert(r.board.clone(), r.entries.clone());
            }
            if !r.buses.is_empty() {
                file.buses.insert(r.board.clone(), r.buses.clone());
            }
            if !r.spaces.is_empty() {
                spaces.insert(r.board.clone(), r.spaces.clone());
            }
        }
        let mut findings = std::mem::take(&mut self.pin_findings);
        if let (true, Some(old)) = (complete, &existing) {
            // Each stale entry: its key, its lock, where it is, and what it held.
            let mut stale: Vec<(&String, &String, bool, Option<Span>, String)> = Vec::new();
            for (board, entries) in &old.boards {
                for (key, e) in entries {
                    stale.push((board, key, e.lock, e.at, format!("pin `{}`", e.pin)));
                }
            }
            for (board, entries) in &old.buses {
                for (key, e) in entries {
                    stale.push((board, key, e.lock, e.at, format!("`{}`", e.peripheral)));
                }
            }
            stale.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
            for (board, key, lock, at, held) in stale {
                let root = roots.iter().find(|r| r.board == *board);
                {
                    if root.is_some_and(|r| r.requests.contains(key)) {
                        continue;
                    }
                    let why = if root.is_none() {
                        format!("the source has no board or system `{board}`")
                    } else {
                        format!("`{board}` has no request `{key}`")
                    };
                    let place = src.zip(at).and_then(|(s, at)| self.sources.place(s, at));
                    if lock {
                        let message = format!(
                            "`{key}` is locked to {held} in the table of `{board}`, but {why}; delete the entry to let the file drop it"
                        );
                        if let Some((s, at)) = src.zip(at) {
                            self.report(s, at, message);
                        }
                    } else {
                        let at = place.map(|p| format!("{p}: ")).unwrap_or_default();
                        findings.push(format!(
                            "{at}the entry `{key}` ({held}) is stale: {why}; the file drops it"
                        ));
                    }
                }
            }
        }
        let changes = pin_changes(existing.as_ref(), &file);
        PinsOutcome {
            file,
            existing,
            changes,
            findings,
            spaces,
            unserved,
        }
    }

    /// The place of an entry in the pin file, `main.pins.toml:3`.
    fn entry_place(&self, entry: &PinEntry) -> Option<String> {
        let src = self.pin_input.as_ref()?.source;
        self.sources.place(src, entry.at?)
    }

    /// Whether an entry of the pin file still holds for its request: the pin is in the part,
    /// it offers what the request asks for, nothing else holds it, and the part's `shared`
    /// resources still allow it. The pin when it does, the reason when it does not.
    fn entry_holds(
        &self,
        ctx: &PartCtx,
        a: &Asked,
        entry: &PinEntry,
        rows: &[PinRow],
        occupied: &BTreeMap<InstId, Holder>,
        fixed: &[(InstId, String)],
    ) -> Result<InstId, String> {
        let model = &self.model;
        let Some((pin, _)) = ctx.rows.iter().find(|(_, p)| *p == entry.pin) else {
            let have: Vec<String> = ctx.rows.iter().map(|(_, p)| p.clone()).collect();
            return Err(format!(
                "`{}` (`{}`) has no pin `{}`; its pins are {}",
                ctx.path,
                ctx.ty,
                entry.pin,
                short_list(&have)
            ));
        };
        let row = rows.iter().find(|r| r.pin == *pin).expect("a pin of a row");
        // An entry of a request for a class records the channel of its pin (`AIN1` for
        // `ANALOG`, #787); one moved to another pin of the class still asks for the class.
        let asks_capability = entry.function == a.capability
            || rows.iter().any(|r| {
                r.functions
                    .iter()
                    .any(|f| f.name == entry.function && f.of.as_deref() == Some(&a.capability))
            });
        if !asks_capability {
            return Err(format!(
                "the entry is for {} and the request asks for {}",
                entry.function, a.capability
            ));
        }
        if !model.offers(ctx.part, row, &a.capability) {
            return Err(format!(
                "`{}.{}` does not offer {}",
                ctx.path, entry.pin, a.capability
            ));
        }
        if let Some(holder) = occupied.get(pin) {
            return Err(format!(
                "`{}.{}` is held by {}",
                ctx.path,
                entry.pin,
                holder.text()
            ));
        }
        let mut chosen = fixed.to_vec();
        chosen.push((*pin, a.capability.clone()));
        if let Some(c) = shares_conflict(model, ctx, &chosen) {
            return Err(c.sentence_with(&holders_of(model, occupied, Some(*pin))));
        }
        Ok(*pin)
    }

    /// Find pins for `todo`, in order: the first solution of a depth-first search over the
    /// pins that offer each capability. On failure, say which request cannot be served, given
    /// the ones before it, and why.
    fn search(
        &self,
        ctx: &PartCtx,
        todo: &[&Asked],
        occupied: &BTreeMap<InstId, Holder>,
        fixed: &[(InstId, String)],
        rows: &[PinRow],
        kept: &[(&Asked, PinAssignment)],
    ) -> Search {
        let model = &self.model;
        let domains: Vec<Vec<InstId>> = todo
            .iter()
            .map(|a| {
                ctx.rows
                    .iter()
                    .filter(|(pin, _)| !occupied.contains_key(pin))
                    .filter(|(pin, _)| {
                        let row = rows.iter().find(|r| r.pin == *pin).expect("a row");
                        model.offers(ctx.part, row, &a.capability)
                    })
                    .map(|(pin, _)| *pin)
                    .collect()
            })
            .collect();
        let functions: Vec<&str> = todo.iter().map(|a| a.capability.as_str()).collect();

        // The longest run of the requests, from the first, that can be served.
        let mut budget = SEARCH_STEPS;
        let mut n = 0;
        let mut last: Option<Vec<InstId>> = None;
        while n < todo.len() {
            match solve(
                model,
                ctx,
                fixed,
                &domains[..=n],
                &functions[..=n],
                &mut budget,
            ) {
                Outcome::Found(pins) => {
                    last = Some(pins);
                    n += 1;
                }
                Outcome::None => break,
                Outcome::GaveUp => {
                    let a = todo[n];
                    return Search::Failed {
                        index: n,
                        message: format!(
                            "`{}` asks for {} on `{}`, and the search for a choice of pins that keeps `shared` within its limits gave up after {SEARCH_STEPS} steps; state some of the pins",
                            a.key, a.capability, ctx.path
                        ),
                    };
                }
            }
        }
        if n == todo.len() {
            return Search::Found(last.unwrap_or_default());
        }

        // The request `n` cannot be served after the ones before it. Say why.
        let a = todo[n];
        let ahead: Vec<InstId> = last.clone().unwrap_or_default();
        let mut holders = occupied.clone();
        for (b, pin) in todo.iter().zip(&ahead) {
            holders.insert(*pin, Holder::New(b.key.clone()));
        }
        let all_offering: Vec<InstId> = ctx
            .rows
            .iter()
            .filter(|(pin, _)| {
                let row = rows.iter().find(|r| r.pin == *pin).expect("a row");
                model.offers(ctx.part, row, &a.capability)
            })
            .map(|(pin, _)| *pin)
            .collect();
        let free: Vec<InstId> = all_offering
            .iter()
            .copied()
            .filter(|p| !holders.contains_key(p))
            .collect();
        let head = format!("`{}` asks for {} on `{}`", a.key, a.capability, ctx.path);
        let path_of = |p: &InstId| format!("`{}`", model.path_of(*p));
        let message = if free.is_empty() {
            // Every pin that offers it is held: by the source, an entry or a request before it.
            let taken: Vec<String> = all_offering
                .iter()
                .map(|p| format!("{} by {}", path_of(p), holders[p].text()))
                .collect();
            let mut m = format!(
                "{head}, and every pin that offers it is taken: {}",
                taken.join(", ")
            );
            if let Some(mv) = self.a_move(ctx, a, &holders, rows, kept, fixed, &all_offering) {
                m.push_str(&format!("; {mv}"));
            }
            m
        } else {
            // Free pins remain, so `shared` is what refuses.
            let mut chosen = fixed.to_vec();
            chosen.extend(
                ahead
                    .iter()
                    .zip(&functions)
                    .map(|(p, f)| (*p, f.to_string())),
            );
            chosen.push((free[0], a.capability.clone()));
            let by = holders_of(model, &holders, Some(free[0]));
            match shares_conflict(model, ctx, &chosen) {
                Some(c) => format!(
                    "{head}, and no free pin can serve it: {}",
                    c.sentence_with(&by)
                ),
                None => format!(
                    "{head}, and no choice of pins for the requests of `{}` keeps its shared resources within their limits",
                    ctx.path
                ),
            }
        };
        Search::Failed { index: n, message }
    }

    /// The one move of an unlocked entry that would free a pin the request needs, if there is
    /// one: the entry goes to another free pin that offers what it asks for.
    #[allow(clippy::too_many_arguments)]
    fn a_move(
        &self,
        ctx: &PartCtx,
        a: &Asked,
        holders: &BTreeMap<InstId, Holder>,
        rows: &[PinRow],
        kept: &[(&Asked, PinAssignment)],
        fixed: &[(InstId, String)],
        offering: &[InstId],
    ) -> Option<String> {
        let model = &self.model;
        for &want in offering {
            let Some(Holder::Entry {
                key, locked: false, ..
            }) = holders.get(&want)
            else {
                continue;
            };
            let Some((_, held)) = kept.iter().find(|(b, _)| b.key == *key) else {
                continue;
            };
            let free = ctx.rows.iter().find(|(pin, _)| {
                let row = rows.iter().find(|r| r.pin == *pin).expect("a row");
                !holders.contains_key(pin) && model.offers(ctx.part, row, &held.function) && {
                    let mut chosen: Vec<(InstId, String)> =
                        fixed.iter().filter(|(p, _)| *p != want).cloned().collect();
                    chosen.push((*pin, held.function.clone()));
                    chosen.push((want, a.capability.clone()));
                    shares_conflict(model, ctx, &chosen).is_none()
                }
            });
            if let Some((pin, _)) = free {
                return Some(format!(
                    "moving `{key}` to `{}` would free `{}`; run with the entry removed from the pin file, or move it there, and run again",
                    model.path_of(*pin),
                    model.path_of(want)
                ));
            }
        }
        None
    }
}

/// What the new file changes in the old one, one line each, in the order of the keys.
fn pin_changes(old: Option<&PinFile>, new: &PinFile) -> Vec<String> {
    let mut keys: BTreeSet<(&str, &str)> = BTreeSet::new();
    for f in old.into_iter().chain(Some(new)) {
        for (board, entries) in &f.boards {
            for key in entries.keys() {
                keys.insert((board, key));
            }
        }
    }
    let get = |f: Option<&'_ PinFile>, board: &str, key: &str| -> Option<PinEntry> {
        f?.board(board)?.get(key).cloned()
    };
    let mut out: Vec<((&str, &str), String)> = Vec::new();
    for (board, key) in keys {
        let (a, b) = (get(old, board, key), get(Some(new), board, key));
        match (a, b) {
            (None, Some(b)) => out.push((
                (board, key),
                format!("{board}.{key}: new, pin `{}` ({})", b.pin, b.function),
            )),
            (Some(a), None) => out.push((
                (board, key),
                format!("{board}.{key}: removed, it was pin `{}`", a.pin),
            )),
            (Some(a), Some(b)) if a != b => {
                if a.pin != b.pin {
                    out.push((
                        (board, key),
                        format!("{board}.{key}: pin `{}` becomes `{}`", a.pin, b.pin),
                    ));
                } else {
                    out.push(((board, key), format!("{board}.{key}: its entry changes")));
                }
            }
            _ => {}
        }
    }
    // The entries of requests for the end of a bus (#788).
    let mut keys: BTreeSet<(&str, &str)> = BTreeSet::new();
    for f in old.into_iter().chain(Some(new)) {
        for (board, entries) in &f.buses {
            for key in entries.keys() {
                keys.insert((board, key));
            }
        }
    }
    let get = |f: Option<&'_ PinFile>, board: &str, key: &str| -> Option<BusEntry> {
        f?.buses_of(board)?.get(key).cloned()
    };
    let lines = |e: &BusEntry| {
        let each: Vec<String> = e
            .pins
            .iter()
            .map(|(line, pin)| format!("{line} `{pin}`"))
            .collect();
        each.join(", ")
    };
    for (board, key) in keys {
        match (get(old, board, key), get(Some(new), board, key)) {
            (None, Some(b)) => out.push((
                (board, key),
                format!("{board}.{key}: new, `{}` with {}", b.peripheral, lines(&b)),
            )),
            (Some(a), None) => out.push((
                (board, key),
                format!("{board}.{key}: removed, it was `{}`", a.peripheral),
            )),
            (Some(a), Some(b)) if a != b => {
                if a.peripheral != b.peripheral || a.pins != b.pins {
                    out.push((
                        (board, key),
                        format!(
                            "{board}.{key}: `{}` with {} becomes `{}` with {}",
                            a.peripheral,
                            lines(&a),
                            b.peripheral,
                            lines(&b)
                        ),
                    ));
                } else {
                    out.push(((board, key), format!("{board}.{key}: its entry changes")));
                }
            }
            _ => {}
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    if old.is_none() && new.is_empty() {
        out.clear();
    }
    out.into_iter().map(|(_, line)| line).collect()
}

/// How a search ended.
enum Search {
    /// A pin for each request, in the order of the requests.
    Found(Vec<InstId>),
    /// The request at `index` cannot be served.
    Failed { index: usize, message: String },
}

enum Outcome {
    Found(Vec<InstId>),
    None,
    GaveUp,
}

/// A depth-first search for distinct pins, one from each domain, so that the shared resources
/// of the part hold. The first solution in the order of the domains, which are in path order.
fn solve(
    model: &Model,
    ctx: &PartCtx,
    fixed: &[(InstId, String)],
    domains: &[Vec<InstId>],
    functions: &[&str],
    budget: &mut usize,
) -> Outcome {
    if !matchable(domains) {
        return Outcome::None;
    }
    let mut picks: Vec<InstId> = Vec::new();
    match descend(model, ctx, fixed, domains, functions, &mut picks, budget) {
        Some(true) => Outcome::Found(picks),
        Some(false) => Outcome::None,
        None => Outcome::GaveUp,
    }
}

/// `Some(true)` with `picks` filled, `Some(false)` when no choice works, `None` when the
/// budget ran out.
fn descend(
    model: &Model,
    ctx: &PartCtx,
    fixed: &[(InstId, String)],
    domains: &[Vec<InstId>],
    functions: &[&str],
    picks: &mut Vec<InstId>,
    budget: &mut usize,
) -> Option<bool> {
    let i = picks.len();
    if i == domains.len() {
        return Some(true);
    }
    for &pin in &domains[i] {
        if picks.contains(&pin) {
            continue;
        }
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let mut chosen = fixed.to_vec();
        chosen.extend(
            picks
                .iter()
                .zip(functions)
                .map(|(p, f)| (*p, f.to_string())),
        );
        chosen.push((pin, functions[i].to_string()));
        if shares_conflict(model, ctx, &chosen).is_some() {
            continue;
        }
        picks.push(pin);
        match descend(model, ctx, fixed, domains, functions, picks, budget)? {
            true => return Some(true),
            false => {
                picks.pop();
            }
        }
    }
    Some(false)
}

/// Whether every domain can have a pin of its own: a bipartite matching, so that a search
/// that cannot succeed (more requests than pins) stops at once and does not try every order.
fn matchable(domains: &[Vec<InstId>]) -> bool {
    fn augment(
        domains: &[Vec<InstId>],
        i: usize,
        seen: &mut BTreeSet<InstId>,
        owner: &mut BTreeMap<InstId, usize>,
    ) -> bool {
        for &pin in &domains[i] {
            if !seen.insert(pin) {
                continue;
            }
            let free = match owner.get(&pin) {
                None => true,
                Some(&j) => augment(domains, j, seen, owner),
            };
            if free {
                owner.insert(pin, i);
                return true;
            }
        }
        false
    }
    let mut owner: BTreeMap<InstId, usize> = BTreeMap::new();
    (0..domains.len()).all(|i| augment(domains, i, &mut BTreeSet::new(), &mut owner))
}

/// A shared resource of the part that more members use than it allows.
struct Conflict {
    share: String,
    limit: usize,
    /// Every member allowed, then the members in use with the pins that use them.
    members: Vec<String>,
    in_use: Vec<(String, Vec<(InstId, String)>)>,
}

impl Conflict {
    fn sentence_with(&self, holders: &BTreeMap<InstId, String>) -> String {
        let uses: Vec<String> = self
            .in_use
            .iter()
            .map(|(member, pins)| {
                let each: Vec<String> = pins
                    .iter()
                    .map(|(pin, f)| match holders.get(pin) {
                        Some(h) => format!("{h} as {f}"),
                        None => format!("a pin as {f}"),
                    })
                    .collect();
                format!("{member} is used on {}", each.join(" and "))
            })
            .collect();
        format!(
            "`{}` allows {} of {} at a time: {}",
            self.share,
            self.limit,
            plain_list(&self.members),
            uses.join("; ")
        )
    }
}

/// How a message names each pin of the part that holds something, and the pin the request
/// would take: `u.p00 (`console`)` for a pin an entry or a request holds, `u.p01 (this request)`
/// for the one being tried.
fn holders_of(
    model: &Model,
    holders: &BTreeMap<InstId, Holder>,
    candidate: Option<InstId>,
) -> BTreeMap<InstId, String> {
    let mut out: BTreeMap<InstId, String> = holders
        .iter()
        .map(|(pin, h)| {
            let who = match h {
                Holder::Source(_) => String::new(),
                Holder::Entry { key, .. } | Holder::New(key) => format!(" (`{key}`)"),
            };
            (*pin, format!("`{}`{who}", model.path_of(*pin)))
        })
        .collect();
    if let Some(pin) = candidate {
        out.insert(pin, format!("`{}` (this request)", model.path_of(pin)));
    }
    out
}

/// A shared resource of a part, as the search reads it.
#[derive(Debug, Clone)]
struct ShareData {
    name: String,
    limit: usize,
    members: Vec<MemberData>,
}

#[derive(Debug, Clone)]
struct MemberData {
    name: String,
    /// The pin, for a member that is a pin.
    pin: Option<InstId>,
    /// The functions of the peripheral, for a member that is one.
    peripheral: Option<Vec<String>>,
}

/// The shared resources of a part, by name, with their members.
fn share_data(model: &Model, part: InstId) -> Vec<ShareData> {
    let inst = model.instance(part);
    let mut shares: Vec<&crate::model::SharedRes> = inst.rules.shares.iter().collect();
    shares.sort_by(|a, b| a.name.cmp(&b.name));
    shares.dedup_by(|a, b| a.name == b.name);
    shares
        .into_iter()
        .map(|s| ShareData {
            name: s.name.clone(),
            limit: s.limit as usize,
            members: s
                .members
                .iter()
                .map(|m| MemberData {
                    name: m.name.clone(),
                    pin: m.pin,
                    peripheral: inst
                        .rules
                        .peripherals
                        .iter()
                        .find(|p| p.name == m.name)
                        .map(|p| p.functions.iter().map(|g| g.name.clone()).collect()),
                })
                .collect(),
        })
        .collect()
}

/// The first shared resource of the part that the chosen functions would use over its limit.
/// A function that a peripheral lists puts the peripheral in use, a function that a member
/// names puts that member in use, and a member that is a pin is in use when a function is
/// chosen on it or the source connects it.
fn shares_conflict(model: &Model, ctx: &PartCtx, chosen: &[(InstId, String)]) -> Option<Conflict> {
    shares_conflict_in(&share_data(model, ctx.part), &ctx.connected, chosen)
}

fn shares_conflict_in(
    shares: &[ShareData],
    connected: &BTreeSet<InstId>,
    chosen: &[(InstId, String)],
) -> Option<Conflict> {
    for s in shares {
        let mut in_use: Vec<(String, Vec<(InstId, String)>)> = Vec::new();
        for m in &s.members {
            let uses: Vec<(InstId, String)> = if let Some(pin) = m.pin {
                match chosen.iter().find(|(p, _)| *p == pin) {
                    Some((p, f)) => vec![(*p, f.clone())],
                    None if connected.contains(&pin) => vec![(pin, "connected".to_string())],
                    None => Vec::new(),
                }
            } else if let Some(functions) = &m.peripheral {
                chosen
                    .iter()
                    .filter(|(_, f)| functions.iter().any(|g| g == f))
                    .cloned()
                    .collect()
            } else {
                chosen
                    .iter()
                    .filter(|(_, f)| *f == m.name)
                    .cloned()
                    .collect()
            };
            if !uses.is_empty() && !in_use.iter().any(|(n, _)| *n == m.name) {
                in_use.push((m.name.clone(), uses));
            }
        }
        if in_use.len() > s.limit {
            return Some(Conflict {
                share: s.name.clone(),
                limit: s.limit,
                members: s.members.iter().map(|m| m.name.clone()).collect(),
                in_use,
            });
        }
    }
    None
}

/// Names for a message: `a, b and c`, no quotes.
fn plain_list(names: &[String]) -> String {
    list_plain(names)
}

fn list_plain(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// Up to six names, then `...`.
fn short_list(names: &[String]) -> String {
    if names.len() <= 6 {
        list_plain(names)
    } else {
        format!("{}, ...", names[..6].join(", "))
    }
}
