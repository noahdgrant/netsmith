//! What a bus interface's `lines` say about a board's nets (ADR 0064).
//!
//! All the rules read the nets after they are made, on the pins of parts (a port of a module is
//! only a way through), so no rule depends on the order of statements:
//!
//! 1. the table of a part against the role it states (message 8);
//! 2. one line on one net: `mosi` is never on `miso`, a `tx` is never on a `tx` (messages 1, 3);
//! 3. the limit of interfaces of one role on one bus (message 2);
//! 4. `single`: a net carries a select line of one slave (message 7).
//!
//! A *bus* is the set of interfaces of one definition that share a net. A net that one of the
//! rules reported is skipped by the rules of ADR 0044 ("two outputs", "only inputs"), so one
//! fault is said once.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::diag::{Diagnostic, SourceId};
use crate::model::{
    BusDef, Domain, FieldId, FieldKind, InstId, InstKind, Member, Model, Net, PinKind, PinRow,
    Requirement,
};
use crate::pins::article;

type DefRef = (usize, usize);

/// What the bus rules found: the messages, and the nets they are about.
#[derive(Default)]
pub(crate) struct BusFindings {
    pub out: Vec<Diagnostic>,
    /// The names of the nets that a rule reported.
    pub nets: HashSet<String>,
}

/// A pin of a bus interface of a part: which line of which bus end it is.
#[derive(Clone)]
struct BusPin {
    pin: InstId,
    /// The interface instance, the end of the bus.
    end: InstId,
    def: DefRef,
    line: String,
    /// The variant the interface states (`Slave`), if it states one.
    role: Option<String>,
}

impl Model {
    /// The `lines` of the interface an instance is made from, if it has any.
    pub(crate) fn bus_def_of(&self, iface: InstId) -> Option<(DefRef, &BusDef)> {
        let def = self.instance(iface).def?;
        self.bus_defs.get(&def).map(|b| (def, b))
    }

    /// The variant of an interface's `.role`, if it has one and it is stated.
    pub(crate) fn role_of(&self, iface: InstId) -> Option<String> {
        let Some(Member::Field(f)) = self.member(iface, "role") else {
            return None;
        };
        match (&self.field(f).kind, &self.field(f).domain) {
            (FieldKind::Enum(_), Domain::Text(variant)) => Some(variant.clone()),
            _ => None,
        }
    }

    /// Whether `field` is the `.role` of a bus interface. What a part is on the bus is not a
    /// value that two ends share, so `~` does not equate it (as it does not equate a current or
    /// a logic level).
    pub fn is_role(&self, field: FieldId) -> bool {
        let f = self.field(field);
        f.name == "role"
            && matches!(f.kind, FieldKind::Enum(_))
            && self.instance(f.owner).kind == InstKind::Interface
            && self
                .bus_def_of(f.owner)
                .is_some_and(|(_, b)| b.role_enum.is_some())
    }

    /// The bus pin a terminal is, if it is the line of an interface that has `lines`.
    fn bus_pin(&self, pin: InstId) -> Option<BusPin> {
        let end = self.instance(pin).parent?;
        if self.instance(end).kind != InstKind::Interface {
            return None;
        }
        let (def, bus) = self.bus_def_of(end)?;
        let line = self.instance(pin).name.clone();
        bus.tables
            .iter()
            .flat_map(|t| t.rows.iter())
            .any(|r| r.name == line)
            .then(|| BusPin {
                pin,
                end,
                def,
                role: self.role_of(end),
                line,
            })
    }

    /// The row of a role's table for a bus pin, with the role's name as it is written.
    fn line_row(&self, bp: &BusPin) -> Option<(&crate::model::LineRow, String, &BusDef)> {
        let bus = self.bus_defs.get(&bp.def)?;
        let table = bus.table(bp.role.as_deref())?;
        let row = table.rows.iter().find(|r| r.name == bp.line)?;
        let role = match (&bp.role, &bus.role_enum) {
            (Some(v), Some(e)) => format!("{e}::{v}"),
            _ => bus.name.clone(),
        };
        Some((row, role, bus))
    }

    /// Whether a pin of a part must be connected: its row says so, else its role says so, else
    /// its kind does (a power or input pin). A role's `required` is judged only for an interface
    /// that has any line connected, so a bus a part declares and the board never uses is left
    /// alone (ADR 0064).
    pub fn must_be_connected(&self, pin: InstId, row: &PinRow) -> bool {
        self.must_be_connected_by(pin, row).0
    }

    /// As [`Model::must_be_connected`], and the clause of the message that names the role when
    /// the role is what says so.
    pub(crate) fn must_be_connected_by(&self, pin: InstId, row: &PinRow) -> (bool, Option<String>) {
        match row.requirement {
            Some(Requirement::Required) => return (true, None),
            Some(Requirement::Optional) => return (false, None),
            None => {}
        }
        if let Some(bp) = self.bus_pin(pin)
            && let Some((line, role, bus)) = self.line_row(&bp)
            && let Some(requirement) = line.requirement
        {
            let used = self.instance(bp.end).children.iter().any(|(_, m)| {
                matches!(m, Member::Inst(p) if self.instance(*p).kind == InstKind::Pin && self.is_connected(*p))
            });
            return match requirement {
                Requirement::Optional => (false, None),
                Requirement::Required => (
                    used,
                    Some(format!("`{role}` of `{}` requires it, and", bus.name)),
                ),
            };
        }
        (matches!(row.kind, PinKind::Power | PinKind::Input), None)
    }

    /// Whether a bus pin of a part that states its role is on the net: the rules of a shared
    /// line then give the advice about `tri_state`.
    pub(crate) fn net_has_stated_role(&self, net: &Net) -> bool {
        net.terminals
            .iter()
            .filter_map(|&t| self.bus_pin(t))
            .any(|bp| bp.role.is_some())
    }
}

/// `two`, `three`, then the digits.
fn count_word(n: usize) -> String {
    match n {
        2 => "two".to_string(),
        3 => "three".to_string(),
        _ => n.to_string(),
    }
}

/// `` `a`, `b` and `c` ``
fn and_list(items: &[String]) -> String {
    crate::lower::list_of(&items.iter().map(|i| format!("`{i}`")).collect::<Vec<_>>())
}

/// A role's name for a plural in a message: `Slave` is `slaves`.
fn plural(variant: &str) -> String {
    format!("{}s", variant.to_lowercase())
}

struct Rules<'a> {
    model: &'a Model,
    found: BusFindings,
}

/// The rules of the bus lines on the nets of a board.
pub(crate) fn check(model: &Model) -> BusFindings {
    let mut rules = Rules {
        model,
        found: BusFindings::default(),
    };
    rules.tables_against_roles();
    let per_net = collect(model);
    for (net, pins) in &per_net {
        rules.one_line_on_a_net(net, pins);
        rules.single_lines(net, pins);
    }
    rules.limits(&per_net);
    rules.found
}

/// The bus pins of every net of a board, in the order of the nets and, on a net, of the paths.
fn collect(model: &Model) -> Vec<(&Net, Vec<BusPin>)> {
    let mut per_net: Vec<(&Net, Vec<BusPin>)> = Vec::new();
    for net in model.nets() {
        let mut pins: Vec<BusPin> = net
            .terminals
            .iter()
            .filter(|&&t| {
                model
                    .owner_of(t)
                    .is_some_and(|p| model.board_of(p).is_some())
            })
            .filter_map(|&t| model.bus_pin(t))
            .collect();
        if pins.is_empty() {
            continue;
        }
        pins.sort_by_key(|p| model.path_of(p.pin));
        per_net.push((net, pins));
    }
    per_net
}

/// The interfaces that share a net, in groups: each is a bus. The ends of a group are of one
/// definition, in path order, and the groups are in the order of their first end.
fn components(model: &Model, per_net: &[(&Net, Vec<BusPin>)]) -> Vec<Vec<InstId>> {
    let mut parent: BTreeMap<InstId, InstId> = BTreeMap::new();
    fn find(parent: &mut BTreeMap<InstId, InstId>, x: InstId) -> InstId {
        let p = *parent.entry(x).or_insert(x);
        if p == x {
            return x;
        }
        let root = find(parent, p);
        parent.insert(x, root);
        root
    }
    for (_, pins) in per_net {
        let mut first: BTreeMap<DefRef, InstId> = BTreeMap::new();
        for p in pins {
            find(&mut parent, p.end);
            match first.get(&p.def) {
                Some(&f) => {
                    let (a, b) = (find(&mut parent, f), find(&mut parent, p.end));
                    if a != b {
                        parent.insert(a.max(b), a.min(b));
                    }
                }
                None => {
                    first.insert(p.def, p.end);
                }
            }
        }
    }
    let mut by_root: BTreeMap<InstId, Vec<InstId>> = BTreeMap::new();
    let ends: Vec<InstId> = parent.keys().copied().collect();
    for e in ends {
        let root = find(&mut parent, e);
        by_root.entry(root).or_default().push(e);
    }
    let mut groups: Vec<Vec<InstId>> = by_root.into_values().collect();
    for g in &mut groups {
        g.sort_by_key(|&e| model.path_of(e));
    }
    groups.sort_by_key(|g| model.path_of(g[0]));
    groups
}

/// The values that the interfaces of a bus share, as the nets show it: the same fields that `~`
/// between two interfaces equates (a clock, a mode), for the interfaces whose pins the board
/// joined one at a time. Each interface is paired with the first of its bus, so the pairs do
/// not depend on the order of the statements (ADR 0064, stage 3).
pub(crate) fn shared_values(model: &Model) -> Vec<(FieldId, FieldId)> {
    let per_net = collect(model);
    let mut pairs = Vec::new();
    for ends in components(model, &per_net) {
        let first = ends[0];
        for &other in &ends[1..] {
            for (name, member) in &model.instance(first).children {
                let (Member::Field(x), Some(Member::Field(y))) =
                    (member, model.member(other, name))
                else {
                    continue;
                };
                if model.is_flow(*x) || model.is_level(*x) || model.is_role(*x) {
                    continue;
                }
                pairs.push((*x, y));
            }
        }
    }
    pairs
}

/// A bus as the clock rules see it: the interfaces of one definition that share a net.
pub struct BusGroup {
    /// The interface's name: `Spi`.
    pub name: String,
    /// The interfaces, in path order.
    pub ends: Vec<InstId>,
    /// The member that holds the clock (`frequency`, `baud`), if the interface has one.
    pub clock: Option<String>,
}

/// The buses of a board: each interface that has `lines` and shares a net with another of its
/// definition, and each that is alone (a bus of one).
pub fn bus_groups(model: &Model) -> Vec<BusGroup> {
    let per_net = collect(model);
    components(model, &per_net)
        .into_iter()
        .filter_map(|ends| {
            let (_, bus) = model.bus_def_of(ends[0])?;
            Some(BusGroup {
                name: bus.name.clone(),
                clock: bus.clock.clone(),
                ends,
            })
        })
        .collect()
}

impl Rules<'_> {
    fn report(&mut self, at: (SourceId, netsmith_lang::Span), message: String) {
        let d = Diagnostic {
            source: at.0,
            span: at.1,
            message,
        };
        if !self.found.out.contains(&d) {
            self.found.out.push(d);
        }
    }

    /// Where a message about a bus pin points: the part that has it, as the other pin rules do.
    fn at_part(&self, pin: InstId) -> (SourceId, netsmith_lang::Span) {
        let owner = self.model.owner_of(pin).expect("a bus pin has a part");
        self.model.instance(owner).at()
    }

    fn type_of(&self, pin: InstId) -> String {
        self.model
            .owner_of(pin)
            .map_or(String::new(), |o| self.model.instance(o).type_name.clone())
    }

    /// Rule 1: the kind of each row of a part that states its role against what the role makes
    /// of the line. The same kind agrees; so does `bidirectional` (a pin that can drive and
    /// receive is not contradicted: a guideline asks for the honest kind), `passive` (a
    /// connector's pin), `no_connect`, a `tri_state` output for an `output`, and, for a line the
    /// role makes `bidirectional`, any signal kind.
    fn tables_against_roles(&mut self) {
        let model = self.model;
        let mut pins: Vec<InstId> = model
            .instances()
            .filter(|(id, i)| {
                i.kind == InstKind::Pin && model.is_terminal(*id) && model.board_of(*id).is_some()
            })
            .map(|(id, _)| id)
            .collect();
        pins.sort_by_key(|&p| model.path_of(p));
        for pin in pins {
            let Some(bp) = model.bus_pin(pin) else {
                continue;
            };
            if bp.role.is_none() {
                continue;
            }
            let Some((line, role, bus)) = model.line_row(&bp) else {
                continue;
            };
            let (Some(kind), Some(row)) = (model.pin_kind(pin), model.pin_facts(pin)) else {
                continue;
            };
            if agrees(line.kind, kind) {
                continue;
            }
            let name = model.path_of(pin);
            let message = format!(
                "`{name}` is {} in the table of `{}`, but `{role}` makes `{}` {} of `{}`; either the datasheet's `{}` pin is not {}, or the role is the other one",
                article(kind),
                self.type_of(pin),
                bp.line,
                article(line.kind),
                bus.name,
                row.name,
                article(kind),
            );
            let at = self.at_part(pin);
            self.report(at, message);
            // What the wrong kind does to its net is the same fault.
            if let Some(net) = model.net_of(pin) {
                self.found.nets.insert(net.name.clone());
            }
        }
    }

    /// Rule 2: all the bus pins on a net are one line, or two that cross.
    fn one_line_on_a_net(&mut self, net: &Net, pins: &[BusPin]) {
        let model = self.model;
        let mut by_def: BTreeMap<DefRef, Vec<&BusPin>> = BTreeMap::new();
        for p in pins {
            by_def.entry(p.def).or_default().push(p);
        }
        let at = self.at_part(pins[0].pin);
        if by_def.len() >= 2 {
            let parts: Vec<String> = by_def
                .iter()
                .map(|(def, ps)| {
                    let bus = &model.bus_defs[def];
                    let names: Vec<String> = ps.iter().map(|p| model.path_of(p.pin)).collect();
                    format!("`{}` ({})", bus.name, and_list(&names))
                })
                .collect();
            self.report(
                at,
                format!(
                    "net `{}` joins lines of different interfaces: {}; each interface joins its own lines to the same lines of another one",
                    net.name,
                    crate::lower::list_of(&parts)
                ),
            );
            self.found.nets.insert(net.name.clone());
            return;
        }
        let (def, ps) = by_def.iter().next().expect("a net with bus pins");
        let bus = &model.bus_defs[def];
        let mut lines: BTreeMap<&str, Vec<&BusPin>> = BTreeMap::new();
        for p in ps {
            lines.entry(p.line.as_str()).or_default().push(p);
        }
        let names_of =
            |ps: &[&BusPin]| -> Vec<String> { ps.iter().map(|p| model.path_of(p.pin)).collect() };
        match lines.len() {
            1 => {
                let (line, ps) = lines.iter().next().expect("one line");
                let Some(partner) = bus.partner(line) else {
                    return;
                };
                let ends: BTreeSet<InstId> = ps.iter().map(|p| p.end).collect();
                if ends.len() < 2 {
                    return;
                }
                let (first, second) = (ps[0], ps[ps.len() - 1]);
                let pairs: Vec<String> = bus
                    .crossings()
                    .iter()
                    .map(|(a, b)| format!("`{a}` to `{b}`"))
                    .collect();
                let (e1, e2) = (
                    model.path_of(first.end),
                    model.path_of(
                        ps.iter()
                            .find(|p| p.end != first.end)
                            .map_or(second.end, |p| p.end),
                    ),
                );
                self.report(
                    at,
                    format!(
                        "net `{}` joins `{line}` to `{line}` ({}): `{line}` crosses `{partner}` in a `{}`, so it is joined to the other end's `{partner}`; write `.{} ~ .{e2}.{partner}`, or join the two interfaces with `.{e1} ~ .{e2}`, which crosses {}",
                        net.name,
                        names_of(ps).iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", "),
                        bus.name,
                        model.path_of(first.pin),
                        crate::lower::list_of(&pairs)
                    ),
                );
                self.found.nets.insert(net.name.clone());
            }
            2 => {
                let mut it = lines.keys();
                let (a, b) = (*it.next().expect("two"), *it.next().expect("two"));
                if bus.partner(a) == Some(b) {
                    return;
                }
                self.different_lines(net, bus, &lines, at);
            }
            _ => self.different_lines(net, bus, &lines, at),
        }
    }

    fn different_lines(
        &mut self,
        net: &Net,
        bus: &BusDef,
        lines: &BTreeMap<&str, Vec<&BusPin>>,
        at: (SourceId, netsmith_lang::Span),
    ) {
        let model = self.model;
        // The line the net is of is the one with most pins, the first by name among equals.
        let major = lines
            .iter()
            .max_by_key(|(name, ps)| (ps.len(), std::cmp::Reverse(**name)))
            .map(|(name, _)| *name)
            .expect("a line");
        let shown: Vec<String> = lines
            .iter()
            .map(|(name, ps)| {
                let paths: Vec<String> = ps
                    .iter()
                    .map(|p| format!("`{}`", model.path_of(p.pin)))
                    .collect();
                format!("`{name}` ({})", paths.join(", "))
            })
            .collect();
        let minor: Vec<String> = lines
            .iter()
            .filter(|(name, _)| **name != major)
            .flat_map(|(_, ps)| ps.iter().map(|p| model.path_of(p.pin)))
            .collect();
        let crossing = {
            let pairs: Vec<String> = bus
                .crossings()
                .iter()
                .map(|(a, b)| format!("`{a}` to `{b}`"))
                .collect();
            if pairs.is_empty() {
                String::new()
            } else {
                format!(", except {}, which cross", crate::lower::list_of(&pairs))
            }
        };
        let joined = lines
            .keys()
            .map(|l| format!("`{l}` to `{l}`"))
            .collect::<Vec<_>>();
        self.report(
            at,
            format!(
                "net `{}` joins {} different lines of a `{}`: {}; a `{}` joins {}{crossing}, and {} {} on the net of `{major}`",
                net.name,
                count_word(lines.len()),
                bus.name,
                crate::lower::list_of(&shown),
                bus.name,
                crate::lower::list_of(&joined),
                and_list(&minor),
                if minor.len() == 1 { "is" } else { "are" },
            ),
        );
        self.found.nets.insert(net.name.clone());
    }

    /// Rule 4: a net carries a `single` line of one end of a role.
    fn single_lines(&mut self, net: &Net, pins: &[BusPin]) {
        let model = self.model;
        let mut groups: BTreeMap<(DefRef, String, String), Vec<&BusPin>> = BTreeMap::new();
        for p in pins {
            let Some(role) = &p.role else { continue };
            let Some((row, _, _)) = model.line_row(p) else {
                continue;
            };
            if row.single {
                groups
                    .entry((p.def, role.clone(), p.line.clone()))
                    .or_default()
                    .push(p);
            }
        }
        for ((def, role, line), ps) in groups {
            let ends: BTreeSet<InstId> = ps.iter().map(|p| p.end).collect();
            if ends.len() < 2 {
                continue;
            }
            let bus = &model.bus_defs[&def];
            let text = format!("{}::{role}", bus.role_enum.as_deref().unwrap_or(""));
            let names: Vec<String> = ps.iter().map(|p| model.path_of(p.pin)).collect();
            let at = self.at_part(ps[0].pin);
            let last = names.last().expect("a pin");
            self.report(
                at,
                format!(
                    "net `{}` carries the `{line}` of {} {}, {}: `{}` gives each {} a `{line}` of its own (`{line}` is `single` for `{text}`); join `.{last}` to a pin of its own",
                    net.name,
                    count_word(ends.len()),
                    plural(&role),
                    and_list(&names),
                    bus.name,
                    role.to_lowercase(),
                ),
            );
            self.found.nets.insert(net.name.clone());
        }
    }

    /// Rule 3: the interfaces of one role on one bus against its `limit`. The bus is the
    /// interfaces of one definition that share a net.
    fn limits(&mut self, per_net: &[(&Net, Vec<BusPin>)]) {
        let model = self.model;
        for members in components(model, per_net) {
            let Some((def, bus)) = model.bus_def_of(members[0]) else {
                continue;
            };
            // Count the ends by role: the role's variant, or None for a bus with one role.
            for table in &bus.tables {
                let Some(limit) = table.limit else { continue };
                let same: Vec<InstId> = members
                    .iter()
                    .copied()
                    .filter(|&e| model.role_of(e) == table.role)
                    .collect();
                if same.len() as u32 <= limit {
                    continue;
                }
                let nets: Vec<&Net> = per_net
                    .iter()
                    .filter(|(_, ps)| ps.iter().any(|p| p.def == def && members.contains(&p.end)))
                    .map(|(n, _)| *n)
                    .collect();
                self.over_limit(bus, table, limit, &same, &members, &nets, per_net);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn over_limit(
        &mut self,
        bus: &BusDef,
        table: &crate::model::RoleLines,
        limit: u32,
        same: &[InstId],
        members: &[InstId],
        nets: &[&Net],
        per_net: &[(&Net, Vec<BusPin>)],
    ) {
        let model = self.model;
        let ends = and_list(&same.iter().map(|&e| model.path_of(e)).collect::<Vec<_>>());
        // The net where two of the offending ends meet: the first one, with the pins on it.
        let through = per_net
            .iter()
            .find(|(_, ps)| ps.iter().filter(|p| same.contains(&p.end)).count() >= 2)
            .map(|(net, ps)| {
                let names: Vec<String> = ps.iter().map(|p| model.path_of(p.pin)).collect();
                format!("`{}` ({})", net.name, and_list(&names))
            })
            .unwrap_or_default();
        // The ends past the limit, in path order: the ones to move.
        let extra = and_list(
            &same[limit as usize..]
                .iter()
                .map(|&e| model.path_of(e))
                .collect::<Vec<_>>(),
        );
        let own = if same.len() as u32 - limit == 1 {
            "a bus of its own"
        } else {
            "a bus of its own each"
        };
        let all = if same.len() == 2 { "both" } else { "all" };
        let (head, others) = match (&table.role, &bus.role_enum) {
            (Some(v), Some(e)) => {
                let mut by_role: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for &end in members.iter().filter(|e| !same.contains(e)) {
                    if let Some(r) = model.role_of(end) {
                        by_role.entry(r).or_default().push(model.path_of(end));
                    }
                }
                let rest: String = by_role
                    .iter()
                    .map(|(role, ends)| {
                        format!(
                            "; the {} on it {} {}",
                            if ends.len() == 1 {
                                role.to_lowercase()
                            } else {
                                plural(role)
                            },
                            if ends.len() == 1 { "is" } else { "are" },
                            and_list(ends)
                        )
                    })
                    .collect();
                (
                    format!(
                        "{ends} are {all} `{e}::{v}` on one `{}` bus, and `{}` allows {limit} `{v}` on a bus",
                        bus.name, bus.name
                    ),
                    rest,
                )
            }
            _ => (
                format!(
                    "{ends} are {} ends of one `{}` bus, and a `{}` has {limit} {} on a bus",
                    same.len(),
                    bus.name,
                    bus.name,
                    if limit == 1 { "end" } else { "ends" }
                ),
                String::new(),
            ),
        };
        let at_end = same[(limit as usize).min(same.len() - 1)];
        let at = model
            .owner_of(at_end)
            .map_or_else(|| model.instance(at_end).at(), |o| model.instance(o).at());
        self.report(
            at,
            format!("{head}: they are on one bus through {through}{others}; give {extra} {own}"),
        );
        for n in nets {
            self.found.nets.insert(n.name.clone());
        }
    }
}

/// Whether the kind a part's row gives a line agrees with the kind its role gives it.
fn agrees(role: PinKind, row: PinKind) -> bool {
    use PinKind::*;
    row == role
        || matches!(row, Bidirectional | Passive | NoConnect)
        || matches!((role, row), (Output, TriState) | (TriState, Output))
        || (role == Bidirectional && matches!(row, Input | Output | TriState))
}

/// Rule 10, the advice: a `bidirectional` row on a line whose role gives a plain direction is
/// accepted, and the part is asked to say `output`, `input` or `tri_state` as its datasheet says.
/// One finding for each such pin, with the part it is a pin of.
pub fn bidirectional_rows(model: &Model) -> Vec<(InstId, String)> {
    let mut pins: Vec<InstId> = model
        .instances()
        .filter(|(id, i)| {
            i.kind == InstKind::Pin && model.is_terminal(*id) && model.board_of(*id).is_some()
        })
        .map(|(id, _)| id)
        .collect();
    pins.sort_by_key(|&p| model.path_of(p));
    let mut out = Vec::new();
    for pin in pins {
        let Some(bp) = model.bus_pin(pin) else {
            continue;
        };
        if bp.role.is_none() {
            continue;
        }
        let Some((line, role, _)) = model.line_row(&bp) else {
            continue;
        };
        if model.pin_kind(pin) != Some(PinKind::Bidirectional)
            || !matches!(
                line.kind,
                PinKind::Input | PinKind::Output | PinKind::TriState
            )
        {
            continue;
        }
        let Some(part) = model.owner_of(pin) else {
            continue;
        };
        let what = match line.kind {
            PinKind::Input => "an input: write `input`",
            PinKind::Output => "an output: write `output` or `tri_state`",
            _ => "a `tri_state` output: write `tri_state`",
        };
        out.push((
            part,
            format!(
                "`{}` is `bidirectional` in the table of `{}`, and `{role}` makes `{}` {what}, as the datasheet says, unless the datasheet says the pin also {}",
                model.path_of(pin),
                model.instance(part).type_name,
                bp.line,
                if line.kind == PinKind::Input { "drives" } else { "receives" },
            ),
        ));
    }
    out
}

/// Rule 7, the note: a bus on which some end states its role and another does not, so the
/// limit and the table of the second are not judged. One note for each such end.
pub fn unstated_roles(model: &Model) -> Vec<(InstId, String)> {
    let per_net = collect(model);
    let mut out = Vec::new();
    for ends in components(model, &per_net) {
        let Some((_, bus)) = model.bus_def_of(ends[0]) else {
            continue;
        };
        if bus.role_enum.is_none()
            || ends.len() < 2
            || !ends.iter().any(|&e| model.role_of(e).is_some())
        {
            continue;
        }
        for e in ends.iter().copied().filter(|&e| model.role_of(e).is_none()) {
            let owner = model.owner_of(e).unwrap_or(e);
            out.push((
                owner,
                format!(
                    "not evaluated: the role of `{}` is not stated, so the limit and the table of `{}` are not judged",
                    model.path_of(e),
                    model.path_of(owner)
                ),
            ));
        }
    }
    out
}
