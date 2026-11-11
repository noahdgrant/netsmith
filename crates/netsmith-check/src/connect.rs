//! Connections, bridges, catalogs, and the nets they produce.

use netsmith_lang::Span;
use netsmith_lang::ast::{self, BinOp, CmpOp, ConnectOp};
use netsmith_quantity::{Dimension, Quantity};

use crate::checker::{Checker, Res};
use std::collections::{HashMap, HashSet};

use crate::model::{
    Balance, Catalog, Chain, Draw, ExplicitJoin, FieldId, InstId, InstKind, Member, Net, RExpr,
    Relation,
};

#[derive(Clone, Copy)]
enum End {
    Pin(InstId),
    Iface(InstId),
    Part(InstId),
}

struct ChainBuilder {
    start: InstId,
    parts: Vec<InstId>,
    end: InstId,
}

impl Checker<'_> {
    pub(crate) fn find(&mut self, x: usize) -> usize {
        let parent = *self.uf.get(&x).unwrap_or(&x);
        if parent == x {
            return x;
        }
        let root = self.find(parent);
        self.uf.insert(x, root);
        root
    }

    fn union(&mut self, a: InstId, b: InstId) {
        let (ra, rb) = (self.find(a.0), self.find(b.0));
        if ra != rb {
            self.uf.insert(ra, rb);
        }
        if self.in_part() {
            self.model.part_joins.push((a, b));
        }
    }

    /// Whether the statement being read is in a part's own definition, where a connection
    /// joins pins of the one part.
    fn in_part(&self) -> bool {
        self.scope.last().is_some_and(|&s| {
            let mut cur = Some(s);
            while let Some(c) = cur {
                match self.model.instance(c).kind {
                    InstKind::Part => return true,
                    InstKind::Interface | InstKind::Pin => cur = self.model.instance(c).parent,
                    _ => return false,
                }
            }
            false
        })
    }

    fn endpoint(&mut self, p: &ast::Path) -> Option<End> {
        match self.resolve_path(p) {
            Res::Inst(i) => {
                let touch = crate::unused::Touch {
                    inst: i,
                    scope: self.current(),
                    source: self.cur_source(),
                    span: p.span,
                };
                self.touches.push(touch);
                self.endpoint_of(p, i)
            }
            Res::Field(_) => {
                self.error(
                    p.span,
                    "a value cannot be connected; connect a pin or an interface",
                );
                None
            }
            Res::Poison | Res::Bad => None,
        }
    }

    fn endpoint_of(&mut self, p: &ast::Path, i: InstId) -> Option<End> {
        match self.model.instance(i).kind {
            InstKind::Pin => Some(End::Pin(i)),
            InstKind::Interface => Some(End::Iface(i)),
            InstKind::Part => Some(End::Part(i)),
            InstKind::Module | InstKind::Board | InstKind::System | InstKind::Panel => {
                self.error(
                    p.span,
                    "connect a pin or an interface of this, not the whole module or board",
                );
                None
            }
        }
    }

    fn bridge_of(&mut self, part: InstId, at: &ast::Path) -> Option<(InstId, InstId)> {
        let bridge = self.model.instance(part).bridge;
        if bridge.is_none() {
            let ty = self.model.instance(part).type_name.clone();
            self.error(
                at.span,
                format!(
                    "`{}` cannot be used in a `~>` chain: `{ty}` has no `bridge`",
                    at.to_source()
                ),
            );
        }
        bridge
    }

    /// `~` in a system: `.main.output ~ .io.input` says that two boards' ports meet.
    /// Only the board names nets, so nothing is put on a net; the ports' values are
    /// equated and their currents balanced.
    fn join(&mut self, c: &ast::Connect) {
        let mut prev = &c.first;
        let mut prev_end = self.endpoint(prev);
        for (op, path) in &c.rest {
            let next = self.endpoint(path);
            if *op == ConnectOp::Series {
                self.error(
                    path.span,
                    "a system joins ports with `~`; `~>` runs through parts, which only a board has",
                );
                return;
            }
            let (Some(a), Some(b)) = (prev_end, next) else {
                return;
            };
            let (End::Iface(a), End::Iface(b)) = (a, b) else {
                self.error(
                    path.span,
                    "a system joins two ports of the same kind; a pin is not a port",
                );
                return;
            };
            if self.is_mating(a) || self.is_mating(b) {
                self.mate(a, b, prev, path);
            } else {
                self.join_ports(a, b, prev, path);
            }
            prev = path;
            prev_end = next;
        }
    }

    fn join_ports(&mut self, a: InstId, b: InstId, at_a: &ast::Path, at_b: &ast::Path) {
        for (port, at) in [(a, at_a), (b, at_b)] {
            let on_board = self
                .model
                .instance(port)
                .parent
                .is_some_and(|p| self.model.instance(p).kind == InstKind::Board);
            if !on_board {
                self.error(
                    at.span,
                    format!(
                        "`{}` is not a port: a port is an interface a board declares directly",
                        at.to_source()
                    ),
                );
                return;
            }
        }
        if self.model.instance(a).parent == self.model.instance(b).parent {
            self.error(
                at_b.span,
                format!(
                    "`{}` and `{}` are on the same board; a system joins ports of different boards",
                    at_a.to_source(),
                    at_b.to_source()
                ),
            );
            return;
        }
        if self.connect_interfaces(a, b, at_b.span, false) {
            self.model.joins.push((a, b));
            let source = self.cur_source();
            self.model.explicit_joins.push(ExplicitJoin {
                a,
                b,
                source,
                span: Span::new(at_a.span.start, at_b.span.end),
            });
        }
    }

    pub fn connect(&mut self, c: &ast::Connect) {
        if self.in_system() {
            return self.join(c);
        }
        if self.in_panel() {
            self.error(
                c.span,
                "a panel joins nothing: it repeats one board in a grid, and the boards stay apart",
            );
            return;
        }
        let Some(first) = self.endpoint(&c.first) else {
            return;
        };
        let mut chain: Option<ChainBuilder> = None;

        // The open end that the next operand attaches to.
        let mut prev = match first {
            End::Part(part) => {
                let Some((from, to)) = self.bridge_of(part, &c.first) else {
                    return;
                };
                chain = Some(ChainBuilder {
                    start: from,
                    parts: vec![part],
                    end: to,
                });
                End::Pin(to)
            }
            other => other,
        };

        for (op, path) in &c.rest {
            let Some(next) = self.endpoint(path) else {
                return;
            };
            match op {
                ConnectOp::Same => {
                    self.finish_chain(&mut chain);
                    match (prev, next) {
                        (End::Pin(a), End::Pin(b)) => self.union(a, b),
                        (End::Iface(a), End::Iface(b)) => {
                            if self.connect_interfaces(a, b, path.span, true) {
                                self.note_rail_alias(a, b);
                            }
                        }
                        _ => {
                            self.error(
                                path.span,
                                "`~` joins two pins or two interfaces of the same kind",
                            );
                            return;
                        }
                    }
                    prev = next;
                }
                ConnectOp::Series => {
                    let End::Pin(a) = prev else {
                        self.error(
                            path.span,
                            "`~>` runs from a pin through bridges, not from an interface",
                        );
                        return;
                    };
                    let builder = chain.get_or_insert(ChainBuilder {
                        start: a,
                        parts: Vec::new(),
                        end: a,
                    });
                    match next {
                        End::Pin(b) => {
                            self.union(a, b);
                            builder.end = b;
                            prev = End::Pin(b);
                        }
                        End::Part(part) => {
                            let Some((from, to)) = self.bridge_of(part, path) else {
                                return;
                            };
                            self.union(a, from);
                            let builder = chain.as_mut().expect("chain was just created");
                            builder.parts.push(part);
                            builder.end = to;
                            prev = End::Pin(to);
                        }
                        End::Iface(_) => {
                            self.error(
                                path.span,
                                "`~>` ends at a pin or passes through a part, not an interface",
                            );
                            return;
                        }
                    }
                }
            }
        }
        self.finish_chain(&mut chain);
    }

    fn finish_chain(&mut self, chain: &mut Option<ChainBuilder>) {
        if let Some(b) = chain.take()
            && !b.parts.is_empty()
        {
            self.model.chains.push(Chain {
                start: b.start,
                parts: b.parts,
                end: b.end,
            });
        }
    }

    /// Remember a `~` in a module that joins an interface of the module itself to an interface
    /// of a part inside it, if the two are rails (they have a `current` and a `pos`). Whether
    /// the module's is then an alias of the part's is decided with the balances (ADR 0077).
    fn note_rail_alias(&mut self, a: InstId, b: InstId) {
        let Some(&module) = self.scope.last() else {
            return;
        };
        if self.model.instance(module).kind != InstKind::Module {
            return;
        }
        for (own, inner) in [(a, b), (b, a)] {
            let in_part = self.model.instance(inner).parent.is_some_and(|p| {
                self.model.instance(p).kind == InstKind::Part
                    && self.model.instance(p).parent == Some(module)
            });
            if self.model.instance(own).parent == Some(module)
                && in_part
                && self.model.draw_of(own).is_some()
                && self.model.draw_of(inner).is_some()
            {
                self.model.rail_aliases.push((own, inner));
            }
        }
    }

    /// Equate the values of two interfaces of the same kind and, if `join_pins`, put their
    /// pins on the same nets. False if they are not of the same kind.
    pub(crate) fn connect_interfaces(
        &mut self,
        a: InstId,
        b: InstId,
        at: Span,
        join_pins: bool,
    ) -> bool {
        let (ia, ib) = (self.model.instance(a), self.model.instance(b));
        if ia.def != ib.def {
            let (ta, tb) = (ia.type_name.clone(), ib.type_name.clone());
            self.error(at, format!("cannot connect a `{ta}` to a `{tb}`"));
            return false;
        }
        let children = self.model.instance(a).children.clone();
        // The lines that cross (a `Uart`'s `tx` and `rx`) are joined to the other end's partner
        // line, not to their own name (ADR 0064).
        let crossed: Vec<(String, String)> = match self.model.bus_def_of(a) {
            Some((_, bus)) => bus
                .crossings()
                .into_iter()
                .flat_map(|(x, y)| {
                    [
                        (x.to_string(), y.to_string()),
                        (y.to_string(), x.to_string()),
                    ]
                })
                .collect(),
            None => Vec::new(),
        };
        for (name, member) in children {
            let there = crossed
                .iter()
                .find(|(own, _)| *own == name)
                .map_or(name.as_str(), |(_, other)| other.as_str());
            match (member, self.model.member(b, there)) {
                (Member::Inst(x), Some(Member::Inst(y))) => match self.model.instance(x).kind {
                    // Two pins that no part's table describes join nothing: a device without
                    // flow control gives no pin to `rts` and `cts`, and two of them make no
                    // net with copper on it.
                    InstKind::Pin if join_pins && !(self.undescribed(x) && self.undescribed(y)) => {
                        self.union(x, y)
                    }
                    // The levels of a bus interface are its part's own, and their reference
                    // rail is that part's supply: joining them would join two supplies.
                    InstKind::Interface if self.model.is_levels(x) => {}
                    InstKind::Interface => {
                        self.connect_interfaces(x, y, at, join_pins);
                    }
                    _ => {}
                },
                // A current is not shared by a connection: a rail can feed several loads.
                // The currents are summed at the net instead (`Balance`).
                (Member::Field(x), Some(Member::Field(y)))
                    if !self.model.is_flow(x)
                        && !self.model.is_level(x)
                        && !self.model.is_pin_figure(x)
                        && !self.model.is_role(x) =>
                {
                    self.model.equalities.push((x, y))
                }
                _ => {}
            }
        }
        true
    }

    /// Whether a pin is one that its part leaves out: the part has a pin table, and no table
    /// has a row for the pin.
    fn undescribed(&self, pin: InstId) -> bool {
        self.model.owner_of(pin).is_some_and(|part| {
            let inst = self.model.instance(part);
            inst.kind == InstKind::Part
                && !inst.tables.is_empty()
                && self.model.pin_facts(pin).is_none()
        })
    }

    fn pin_of(&mut self, p: &ast::Path) -> Option<InstId> {
        match self.resolve_path(p) {
            Res::Inst(i) if self.model.instance(i).kind == InstKind::Pin => Some(i),
            Res::Inst(_) | Res::Field(_) => {
                self.error(p.span, format!("`{}` is not a pin", p.to_source()));
                None
            }
            Res::Poison | Res::Bad => None,
        }
    }

    pub fn bridge(&mut self, b: &ast::Bridge) {
        let cur = self.scope.last().copied().expect("scope is open");
        if self.model.instance(cur).kind != InstKind::Part {
            self.error(b.span, "`bridge` can only appear inside a part");
            return;
        }
        let (from, to) = (self.pin_of(&b.from), self.pin_of(&b.to));
        let (Some(from), Some(to)) = (from, to) else {
            return;
        };
        if from == to {
            self.error(b.span, "a bridge needs two different terminals");
        } else if self.model.instance(cur).bridge.is_some() {
            self.error(b.span, "this part already has a bridge");
        } else {
            self.model.instances[cur.0].bridge = Some((from, to));
        }
    }

    pub fn catalog(&mut self, c: &ast::Catalog) {
        let cur = self.scope.last().copied().expect("scope is open");
        if self.model.instance(cur).kind != InstKind::Part {
            self.error(c.span, "`catalog` can only appear inside a part");
            return;
        }
        let mut fields = Vec::new();
        let mut marked = Vec::new();
        for (path, optional) in c.fields.iter().zip(&c.optional) {
            match self.resolve_path(path) {
                Res::Field(f) => {
                    fields.push(f);
                    if optional.is_some() {
                        marked.push((f, path));
                    }
                }
                Res::Inst(_) => self.error(
                    path.span,
                    format!(
                        "`{}` is not a plain value, so a catalog cannot list it",
                        path.to_source()
                    ),
                ),
                Res::Poison | Res::Bad => {}
            }
        }
        let key = c
            .key
            .as_ref()
            .and_then(|path| match self.resolve_path(path) {
                Res::Field(f) if fields.contains(&f) => Some(f),
                Res::Field(_) => {
                    self.error(
                        path.span,
                        format!(
                            "the key `{}` of a catalog must be one of its columns",
                            path.to_source()
                        ),
                    );
                    None
                }
                Res::Inst(_) => {
                    self.error(
                        path.span,
                        format!(
                            "`{}` is not a plain value, so it cannot be a key",
                            path.to_source()
                        ),
                    );
                    None
                }
                Res::Poison | Res::Bad => None,
            });
        // The key names the one row the design chooses: it has a value in every row.
        let mut optional = Vec::new();
        for (f, path) in marked {
            if key == Some(f) {
                self.error(
                    path.span,
                    format!(
                        "`{}` is the key of `catalog {}` and names the row, so it cannot be `optional`: a row without it could not be chosen",
                        path.to_source(),
                        c.name.name
                    ),
                );
            } else if !optional.contains(&f) {
                optional.push(f);
            }
        }
        self.model.instances[cur.0].catalog = Some(Catalog {
            name: c.name.name.clone(),
            fields,
            key,
            optional,
        });
    }

    /// The net name a pin asked for, if any.
    pub(crate) fn requested_name(&self, pin: InstId) -> Option<String> {
        let Member::Field(f) = self.model.member(pin, "net")? else {
            return None;
        };
        match &self.model.field(f).domain {
            crate::model::Domain::Text(n) => Some(n.clone()),
            _ => None,
        }
    }

    /// Group connected pins into nets, agree on a name for each, and name the rest.
    ///
    /// Pins that ask for the same name are one net, as in any schematic tool. A net no
    /// pin named is called `net-` plus its smallest terminal path (or pin path, if it has
    /// no terminals). The smallest path, not the first declared, so renaming does not
    /// depend on the order the source lists things in.
    pub fn finalize_nets(&mut self) {
        let pins: Vec<InstId> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Pin)
            .map(|(id, _)| id)
            .collect();

        // A net name is board-wide, so two boards of a system may both have a `GND`.
        let mut first_named: std::collections::HashMap<(Option<InstId>, String), InstId> =
            Default::default();
        for &pin in &pins {
            if let Some(name) = self.requested_name(pin) {
                match first_named.get(&(self.model.board_of(pin), name.clone())) {
                    Some(&first) => self.union(first, pin),
                    None => {
                        first_named.insert((self.model.board_of(pin), name), pin);
                    }
                }
            }
        }

        let mut order: Vec<usize> = Vec::new();
        let mut groups: std::collections::HashMap<usize, Vec<InstId>> = Default::default();
        for pin in pins {
            let root = self.find(pin.0);
            groups.entry(root).or_insert_with(|| {
                order.push(root);
                Vec::new()
            });
            groups.get_mut(&root).expect("just inserted").push(pin);
        }

        // (pins, the name they agreed on)
        let mut nets: Vec<(Vec<InstId>, Option<String>)> = Vec::new();
        for root in order {
            let pins = groups.remove(&root).expect("grouped above");
            let mut name: Option<(String, InstId)> = None;
            for &pin in &pins {
                let Some(n) = self.requested_name(pin) else {
                    continue;
                };
                match &name {
                    None => name = Some((n, pin)),
                    Some((existing, first)) if *existing != n => {
                        let Some(Member::Field(f)) = self.model.member(pin, "net") else {
                            continue;
                        };
                        let (src, span) = *self
                            .model
                            .field(f)
                            .narrowed_at
                            .last()
                            .expect("a set name has a source");
                        let (a, b) = (self.model.path_of(*first), self.model.path_of(pin));
                        let existing = existing.clone();
                        self.report(
                            src,
                            span,
                            format!("these pins are connected but name their net differently: `{existing}` on `{a}`, `{n}` on `{b}`"),
                        );
                    }
                    Some(_) => {}
                }
            }
            if pins.len() >= 2 || name.is_some() {
                nets.push((pins, name.map(|(n, _)| n)));
            }
        }

        let mut used: std::collections::HashSet<(Option<InstId>, String)> = nets
            .iter()
            .filter_map(|(pins, n)| Some((self.model.board_of(pins[0]), n.clone()?)))
            .collect();
        let mut built = Vec::new();
        for (pins, name) in nets {
            let board = self.model.board_of(pins[0]);
            let below = board.unwrap_or(self.model.root);
            let terminals: Vec<InstId> = pins
                .iter()
                .copied()
                .filter(|&p| self.model.is_terminal(p))
                .collect();
            let (name, explicit) = match name {
                Some(n) => (n, true),
                None => {
                    let paths = if terminals.is_empty() {
                        &pins
                    } else {
                        &terminals
                    };
                    let smallest = paths
                        .iter()
                        .map(|&p| self.model.path_below(p, below))
                        .min()
                        .expect("a net has a pin");
                    let base = format!("net-{smallest}");
                    let mut candidate = base.clone();
                    let mut n = 2;
                    while !used.insert((board, candidate.clone())) {
                        candidate = format!("{base}_{n}");
                        n += 1;
                    }
                    (candidate, false)
                }
            };
            built.push(Net {
                pins,
                terminals,
                name,
                explicit,
            });
        }
        built.sort_by(|a, b| a.name.cmp(&b.name));
        self.model.nets = built;
        self.chain_currents();
        self.analyse_mates();
        self.balance_currents();
        self.derive_bias();
        self.derive_chain_values();
        self.equate_bus_values();
        self.derive_bus_figures();
        self.derive_temperature_shift();
    }

    /// Give each `I2cPullups` the figures of the parts on its bus (ADR 0102).
    ///
    /// The bus is found by the nets: the `I2c` interfaces that share a net with the pull-up's
    /// `.bus.sda` or `.bus.scl` (`bus_beside`). Two relations are made from them, both before the
    /// optional members are skipped, so a part that states no figure drops its own relation and
    /// nothing else (ADR 0074); the solver names that part (`netsmith-solve`, `busfigures.rs`).
    ///
    /// - `bus_capacitance == the .capacitance of each interface + .trace_capacitance`. The
    ///   board's estimate of the traces is the addend that says the board wants the sum: a sum of
    ///   the pins alone would pass a bound the traces break, so with none stated nothing is
    ///   derived. A bus with no `I2c` interface, or one end that is a peripheral (it has no
    ///   figure to read), gets none.
    /// - for each interface, `resistance >= (supply.voltage - levels.vol) / sink_current` of
    ///   that interface: its own `IOL` with its own `VOL`. The worst case is the greatest of
    ///   these, which no single pair of the bus's figures stands for (the least current and the
    ///   greatest level can be two parts), so each part has its own relation and `min` is not
    ///   needed (ADR 0098 keeps it from giving a value).
    ///
    /// The pull-up's own `.bus_capacitance`, `.sink_current` and `.low_level` stay what a board
    /// may state, and a board that states one is judged on its own figure: the derivation of
    /// that figure is not made. Both would say one fault twice, and an equation against a
    /// narrower range the board states reads as a figure that cannot be proved, not as a narrowing.
    fn derive_bus_figures(&mut self) {
        let pullups: Vec<InstId> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Module && i.type_name == "I2cPullups")
            .map(|(id, _)| id)
            .collect();
        for pullup in pullups {
            let Some(Member::Inst(bus)) = self.model.member(pullup, "bus") else {
                continue;
            };
            let pins: Vec<InstId> = ["sda", "scl"]
                .iter()
                .filter_map(|n| match self.model.member(bus, n) {
                    Some(Member::Inst(p)) => Some(p),
                    _ => None,
                })
                .collect();
            let ends = crate::busnets::bus_beside(&self.model, "I2c", &pins);
            let field = |m: &Self, path: &str| m.model.field_in(pullup, path);
            let at = self.model.instance(pullup).at();
            if ends.peripherals.is_empty() && !ends.interfaces.is_empty() {
                let figures: Vec<FieldId> = ends
                    .interfaces
                    .iter()
                    .filter_map(|&i| self.model.field_in(i, "capacitance"))
                    .collect();
                if let (Some(total), Some(trace), true) = (
                    field(self, "bus_capacitance"),
                    field(self, "trace_capacitance"),
                    figures.len() == ends.interfaces.len(),
                ) && !self.board_states(total)
                {
                    let sum = figures
                        .iter()
                        .chain(std::iter::once(&trace))
                        .map(|&f| RExpr::Field(f))
                        .reduce(|sum, f| RExpr::Bin(BinOp::Add, Box::new(sum), Box::new(f)))
                        .expect("a sum has the traces");
                    self.derived(total, sum, at);
                }
            }
            let (Some(resistance), Some(supply)) =
                (field(self, "resistance"), field(self, "supply.voltage"))
            else {
                continue;
            };
            // A board that states the figure itself (`.sink_current` or `.low_level`) has the
            // bound of its own, and it stands in place of the parts' (see above).
            let by_hand = ["sink_current", "low_level"]
                .iter()
                .filter_map(|p| field(self, p))
                .any(|f| self.board_states(f));
            if by_hand {
                continue;
            }
            for &iface in &ends.interfaces {
                let (Some(vol), Some(sink)) = (
                    self.model.field_in(iface, "levels.vol"),
                    self.model.field_in(iface, "sink_current"),
                ) else {
                    continue;
                };
                let least = RExpr::Bin(
                    BinOp::Div,
                    Box::new(RExpr::Bin(
                        BinOp::Sub,
                        Box::new(RExpr::Field(supply)),
                        Box::new(RExpr::Field(vol)),
                    )),
                    Box::new(RExpr::Field(sink)),
                );
                std::sync::Arc::make_mut(&mut self.model.relations).push(Relation {
                    lhs: RExpr::Field(resistance),
                    op: CmpOp::Ge,
                    rhs: least,
                    source: at.0,
                    span: at.1,
                    written: None,
                    own: false,
                });
            }
        }
    }

    /// Give a part that has a `temperature_shift` and a `voltage_over_temperature` (the `Zener`)
    /// the shift of the board's environment: the board's `.environment.temperature` less the
    /// 25 C that a datasheet states its figures at, so -40 C to 125 C is `-65K..100K`.
    ///
    /// It is derived only when all of these hold, and otherwise nothing changes for the part:
    ///
    /// - the board states an environment temperature (an `&=`, or the `?=` of a named
    ///   environment);
    /// - the design states no `.temperature_shift` of its own: one it states overrides the
    ///   derivation, as for the figures of a bus (`derive_bus_figures`);
    /// - the design reads the part's `.voltage_over_temperature` in a relation of its own
    ///   (`.z.voltage_over_temperature <= 18V`). The shift makes the library's relation, and
    ///   with it the row's `temperature_coefficient`, asked of every row (ADR 0078), so a board
    ///   that never bounds the voltage over temperature must not lose the rows that state no
    ///   coefficient to an environment it stated for another reason.
    ///
    /// The value is the relation `temperature_shift == environment.temperature - 25 C`, from the
    /// range's two ends, so the worst case of the library's relation is the hot corner at the
    /// environment's hot end and the cold corner at its cold end.
    fn derive_temperature_shift(&mut self) {
        let parts: Vec<(InstId, FieldId, FieldId, FieldId)> = self
            .model
            .instances()
            .filter_map(|(id, _)| {
                let (shift, voltage) = self.model.temperature_shift_of(id)?;
                let environment = self.model.environment_temperature_of(id)?;
                Some((id, shift, voltage, environment))
            })
            .collect();
        for (part, shift, voltage, environment) in parts {
            if self.board_states(shift) || !self.board_states(environment) {
                continue;
            }
            let read = self.model.relations.iter().any(|r| {
                let mut fields = Vec::new();
                r.lhs.fields(&mut fields);
                r.rhs.fields(&mut fields);
                !r.own && fields.contains(&voltage)
            });
            if !read {
                continue;
            }
            let Ok(reference) = Quantity::point(
                Dimension::TEMPERATURE,
                crate::environment::DATASHEET_TEMPERATURE,
            ) else {
                continue;
            };
            let value = RExpr::Bin(
                BinOp::Sub,
                Box::new(RExpr::Field(environment)),
                Box::new(RExpr::Const(reference)),
            );
            let at = self.model.instance(part).at();
            self.derived(shift, value, at);
        }
    }

    /// Whether a statement of the design gives `field` a value or a bound: an `&=` or a `?=`,
    /// or an equation or a bound against numbers alone (`optional::states`).
    fn board_states(&self, field: FieldId) -> bool {
        self.stated.contains(&field)
            || !self.model.field(field).narrowed_at.is_empty()
            || self
                .model
                .relations
                .iter()
                .any(|r| crate::optional::states(r.op, &r.lhs, &r.rhs) == Some(field))
    }

    /// The relation `field == value` that the checker derives, pointed at `at`.
    fn derived(&mut self, field: FieldId, value: RExpr, at: (crate::diag::SourceId, Span)) {
        std::sync::Arc::make_mut(&mut self.model.relations).push(Relation {
            lhs: RExpr::Field(field),
            op: CmpOp::EqEq,
            rhs: value,
            source: at.0,
            span: at.1,
            written: None,
            own: false,
        });
    }

    /// The values that the interfaces of one bus share (its clock, its mode) are equated when the
    /// nets show them to be on one bus, as `~` between two interfaces equates them. The pairs
    /// that a `~` has made already are not made twice (ADR 0064, stage 3).
    fn equate_bus_values(&mut self) {
        for (x, y) in crate::busnets::shared_values(&self.model) {
            let known = self
                .model
                .equalities
                .iter()
                .any(|&(a, b)| (a, b) == (x, y) || (a, b) == (y, x));
            if !known {
                self.model.equalities.push((x, y));
            }
        }
    }

    /// Give each part with a `bias` (a capacitor) the voltage of the rail it sits across, and
    /// each part with a `bridge_voltage` (a diode) that voltage with its sign.
    ///
    /// A part is across a rail when its two bridge terminals are on the nets of the rail's
    /// `pos` and `neg`, in either order: the sign of a bias does not matter to a ceramic
    /// capacitor. Only the rails the part's own scope can see count, as in the balances: the
    /// rails of the module or board that holds the part, and the ports of what that holds.
    /// A rail inside another module is out of reach (ADR 0004), and does not need to be
    /// named: the voltage reaches the port by the `~` that joins them.
    ///
    /// The bias is one relation, `bias = rail.voltage`, so a bias the design also states
    /// is checked against it by the solver, which reports a contradiction. Rails on the
    /// same two nets are joined at the pins, so they are one voltage; the first by path
    /// stands for them, which keeps the choice independent of statement order. A part across
    /// no rail keeps its bias unset, and the solver says so if the design needs it.
    ///
    /// A part that is not symmetric has a `bridge_voltage` and not a `bias` of this kind
    /// (ADR 0088): the voltage from the first terminal of its bridge to the second. It is
    /// the rail's voltage when the first terminal is on the rail's `pos`, and its negative
    /// when it is on the `neg`, so a diode wired the wrong way round across a rail is the
    /// other sign and the part's own relations see it. Such a part's `bias` is the part's to
    /// define from it, and is not derived here.
    fn derive_bias(&mut self) {
        // The part, the field to give, the two terminals, and whether the sign counts.
        let parts: Vec<(InstId, FieldId, InstId, InstId, bool)> = self
            .model
            .instances()
            .filter_map(|(id, _)| {
                if let Some((v, a, b)) = self.model.bridge_voltage_of(id) {
                    return Some((id, v, a, b, true));
                }
                let (bias, a, b) = self.model.bias_of(id)?;
                Some((id, bias, a, b, false))
            })
            .collect();
        let rails: Vec<_> = self
            .model
            .instances()
            .filter_map(|(id, _)| Some((id, self.model.rail_of(id)?)))
            .collect();
        for (part, field, a, b, signed) in parts {
            let (na, nb) = (self.find(a.0), self.find(b.0));
            if na == nb {
                continue;
            }
            let scope = self.model.instance(part).parent;
            // The rail's path, its voltage, and whether the first terminal is on its `neg`.
            let mut across: Vec<(String, _, bool)> = Vec::new();
            for &(rail, (voltage, pos, neg)) in &rails {
                let Some(owner) = self.model.owner_of(rail) else {
                    continue;
                };
                if Some(owner) != scope && self.model.instance(owner).parent != scope {
                    continue;
                }
                let (np, nn) = (self.find(pos.0), self.find(neg.0));
                if (np, nn) == (na, nb) {
                    across.push((self.model.path_of(rail), voltage, false));
                } else if (np, nn) == (nb, na) {
                    across.push((self.model.path_of(rail), voltage, true));
                }
            }
            across.sort_by(|x, y| x.0.cmp(&y.0));
            let Some(&(_, voltage, flipped)) = across.first() else {
                continue;
            };
            // Point at the statement that gave the value, if the design gave one: that is
            // where a contradiction is.
            let (source, span) = match self.model.field(field).narrowed_at.last() {
                Some(&at) => at,
                None => self.model.instance(part).at(),
            };
            let rail_voltage = RExpr::Field(voltage);
            let rhs = if signed && flipped {
                RExpr::Neg(Box::new(rail_voltage))
            } else {
                rail_voltage
            };
            std::sync::Arc::make_mut(&mut self.model.relations).push(Relation {
                lhs: RExpr::Field(field),
                op: CmpOp::EqEq,
                rhs,
                source,
                span,
                written: None,
                own: false,
            });
        }
    }

    /// Give the parts of each `~>` chain the values of the chain: the `peak` of the rail the
    /// chain starts on to each part that has a `supply_peak` (a fuse), and the other two
    /// values of ADR 0091 (see below). ADR 0086.
    ///
    /// The chain carries the value, as it carries the current (ADR 0014): a part in series
    /// is not across the rail and has no `bias`, but it sees the rail's highest voltage
    /// through its first net. The rail is one whose `pos` is on the chain's start net, among
    /// those the part's own scope can see (as for `derive_bias`). The rails the scope
    /// declares itself come before the rails of what it holds, and then the first by path
    /// stands, which keeps the choice independent of statement order. Every part of the
    /// chain gets the same value: a part further along sees at most the start's peak, less
    /// what the parts before it drop, and the drop is not subtracted. A chain that starts on
    /// no rail's `pos` (a signal, a rail's `neg`) gives its parts nothing, and the fault, if
    /// the design needs the value, is the solver's, as for a bias.
    ///
    /// The value is one relation, `supply_peak = rail.peak`, so one the design states too is
    /// checked against it, and a rail that states no peak leaves the checks on it listed as
    /// not evaluated.
    ///
    /// The rail's `reverse_peak`, the highest voltage it reaches the wrong way round, goes to
    /// the first part of the chain that has a `supply_reverse_peak`, and to no other: that
    /// part is the one that meets it (a diode blocks it, a TVS diode conducts), and what is
    /// behind it is protected by it, or the part's own check names it. The relation is
    /// `supply_reverse_peak = rail.reverse_peak`.
    ///
    /// A part with a `supply_fault_current` is given the `limit_current` of the nearest part
    /// in front of it in the chain that has one (a fuse's rating): the most that can get
    /// through to it, whatever is behind it. It needs no rail, so a chain that starts on a
    /// signal has it. A part further in front that has a `limit_current` is not looked at: the
    /// nearest is at least the smallest of them, so it is an upper bound.
    fn derive_chain_values(&mut self) {
        let rails: Vec<(InstId, InstId)> = self
            .model
            .instances()
            .filter_map(|(id, _)| {
                let (_, pos, _) = self.model.rail_of(id)?;
                self.model.peak_of(id)?;
                Some((id, pos))
            })
            .collect();
        for chain in self.model.chains.clone() {
            let Some(scope) = self.model.instance(chain.parts[0]).parent else {
                continue;
            };
            let start = self.find(chain.start.0);
            let mut starts: Vec<(bool, String, InstId)> = Vec::new();
            for &(rail, pos) in &rails {
                let Some(owner) = self.model.owner_of(rail) else {
                    continue;
                };
                let own = owner == scope;
                if !own && self.model.instance(owner).parent != Some(scope) {
                    continue;
                }
                if self.find(pos.0) == start {
                    starts.push((!own, self.model.path_of(rail), rail));
                }
            }
            starts.sort();
            let rail = starts.first().map(|&(_, _, rail)| rail);
            let peak = rail.and_then(|r| self.model.peak_of(r));
            let reverse = rail.and_then(|r| self.model.reverse_peak_of(r));
            let mut reverse_met = false;
            let mut limit: Option<FieldId> = None;
            for &part in &chain.parts {
                if let (Some(field), Some(peak)) = (self.model.supply_peak_of(part), peak) {
                    self.relate_chain_value(part, field, peak);
                }
                if let Some(field) = self.model.supply_reverse_peak_of(part) {
                    if let (Some(reverse), false) = (reverse, reverse_met) {
                        self.relate_chain_value(part, field, reverse);
                    }
                    reverse_met = true;
                }
                if let (Some(field), Some(limit)) =
                    (self.model.supply_fault_current_of(part), limit)
                {
                    self.relate_chain_value(part, field, limit);
                }
                if let Some(given) = self.model.limit_current_of(part) {
                    limit = Some(given);
                }
            }
        }
    }

    /// The relation `field = value` that a chain gives a part, pointed at the statement that
    /// gave the part's value if the design gave one, or else at the part.
    fn relate_chain_value(&mut self, part: InstId, field: FieldId, value: FieldId) {
        let (source, span) = match self.model.field(field).narrowed_at.last() {
            Some(&at) => at,
            None => self.model.instance(part).at(),
        };
        std::sync::Arc::make_mut(&mut self.model.relations).push(Relation {
            lhs: RExpr::Field(field),
            op: CmpOp::EqEq,
            rhs: RExpr::Field(value),
            source,
            span,
            written: None,
            own: false,
        });
    }

    /// For each net (by union-find root), how many terminals of parts are on it.
    fn terminal_counts(&mut self) -> HashMap<usize, usize> {
        let mut counts = HashMap::new();
        for net in self.model.nets.clone() {
            if let Some(pin) = net.pins.first() {
                counts.insert(self.find(pin.0), net.terminals.len());
            }
        }
        counts
    }

    /// Whether a junction in a chain, between one part and the next, has a third terminal
    /// on it that can take some of the current.
    fn is_tapped(&mut self, counts: &HashMap<usize, usize>, parts: &[InstId]) -> bool {
        parts.windows(2).any(|pair| {
            let (_, out) = self
                .model
                .instance(pair[0])
                .bridge
                .expect("a chain part bridges");
            counts.get(&self.find(out.0)) != Some(&2)
        })
    }

    /// Equate the through-currents of neighbouring parts in a chain: a series chain passes
    /// one current. Only where the two parts are the only terminals on the net between
    /// them; a third terminal there can take some of it, and then nothing says the two are
    /// equal. A part with no through-current (a capacitor) is skipped.
    fn chain_currents(&mut self) {
        let counts = self.terminal_counts();
        for chain in self.model.chains.clone() {
            for pair in chain.parts.windows(2) {
                let (Some(a), Some(b)) = (
                    self.model.through_of(pair[0]),
                    self.model.through_of(pair[1]),
                ) else {
                    continue;
                };
                if !self.is_tapped(&counts, pair) {
                    self.model.equalities.push((a, b));
                }
            }
        }
    }

    /// Add up the currents at each net, one scope at a time (ADR 0007, ADR 0014).
    ///
    /// A rail belongs to the part, module or board it is declared in, and takes part in
    /// two scopes. In the scope that holds its owner it counts as it is: it draws what
    /// the owner draws. In its owner's own scope, as a module's port, it is the supply of
    /// what is inside, so it counts flipped. A board's rails stand for the outside world
    /// and count as they are, so a load is written positive and a supply is negative.
    /// Rails deeper than the scope's children are left to the scopes of their own
    /// modules, which is why a module's port hides what is inside it. What a rail draws
    /// through `pos` comes back at `neg`, so there it counts with the opposite sign.
    ///
    /// A chain that starts on a net a rail draws from draws the through-current of its
    /// first part there, and gives back the through-current of its last part where it
    /// ends. Each end reads its own part. A chain that starts elsewhere (a signal pin) is
    /// left out, both ends, because what supplies it is not modelled and a half would not
    /// balance.
    fn balance_currents(&mut self) {
        let net_names: HashMap<usize, String> = self
            .model
            .nets
            .clone()
            .iter()
            .filter_map(|n| Some((self.find(n.pins.first()?.0), n.name.clone())))
            .collect();

        // (scope, net root) -> what is drawn there.
        #[derive(Default)]
        struct Group {
            draws: Vec<(InstId, Draw)>,
            /// A rail draws from this net through its `pos`.
            supply: bool,
            /// A rail's current comes back to this net through its `neg`.
            returns: bool,
            /// The part, module or board each rail belongs to: what its current stands for.
            owners: Vec<InstId>,
            /// The pins that are terms, or that a term stands for (a chain's first and last
            /// part pins).
            counted: HashSet<InstId>,
        }
        let mut groups: HashMap<(InstId, usize), Group> = HashMap::new();
        let rails: Vec<InstId> = self
            .model
            .instances()
            .filter(|(id, _)| self.model.draw_of(*id).is_some())
            .map(|(id, _)| id)
            .collect();
        for rail in rails {
            let (field, pos) = self.model.draw_of(rail).expect("filtered above");
            let Some(owner) = self.model.owner_of(rail) else {
                continue;
            };
            let neg = match self.model.member(rail, "neg") {
                Some(Member::Inst(neg)) => Some(neg),
                _ => None,
            };
            let (root, neg_root) = (self.find(pos.0), neg.map(|n| (n, self.find(n.0))));
            let mut add = |scope: InstId, flipped: bool| {
                let g = groups.entry((scope, root)).or_default();
                g.draws.push((pos, Draw { field, flipped }));
                g.supply = true;
                g.owners.push(owner);
                g.counted.insert(pos);
                if let Some((neg, neg_root)) = neg_root {
                    let g = groups.entry((scope, neg_root)).or_default();
                    let flipped = !flipped;
                    g.draws.push((neg, Draw { field, flipped }));
                    g.returns = true;
                    g.owners.push(owner);
                    g.counted.insert(neg);
                }
            };
            // A board's ports are balanced where they are joined (below), not by the net
            // they sit on: in a system their nets are never joined.
            if let Some(scope) = self.model.instance(owner).parent
                && self.model.instance(scope).kind != InstKind::System
            {
                add(scope, false);
            }
            match self.model.instance(owner).kind {
                InstKind::Module => add(owner, true),
                InstKind::Board => add(owner, false),
                _ => {}
            }
        }

        // A chain is whole when it can be counted at both ends: every part passes a
        // current, nothing taps the junctions, and it ends on a net a rail is on. Otherwise
        // only its start is drawn, and the scope's return nets are not checked, because
        // what comes back there is not all accounted for.
        let counts = self.terminal_counts();
        let mut partial: HashSet<InstId> = HashSet::new();
        for chain in self.model.chains.clone() {
            let throughs: Vec<Option<FieldId>> = chain
                .parts
                .iter()
                .map(|&p| self.model.through_of(p))
                .collect();
            let Some(scope) = self.model.instance(chain.parts[0]).parent else {
                continue;
            };
            let start = self.find(chain.start.0);
            let end = self.find(chain.end.0);
            if throughs.iter().all(Option::is_none)
                || !groups.get(&(scope, start)).is_some_and(|g| g.supply)
            {
                continue;
            }
            let whole = throughs.iter().all(Option::is_some)
                && !self.is_tapped(&counts, &chain.parts)
                && groups
                    .get(&(scope, end))
                    .is_some_and(|g| g.supply || g.returns);
            if let Some(field) = throughs[0] {
                let draw = Draw {
                    field,
                    flipped: false,
                };
                let first = self.model.instance(chain.parts[0]).bridge;
                let g = groups.entry((scope, start)).or_default();
                g.draws.push((chain.start, draw));
                g.counted.insert(chain.start);
                g.counted.extend(first.map(|(a, _)| a));
            }
            if whole {
                let field =
                    throughs[throughs.len() - 1].expect("whole means every part passes one");
                let draw = Draw {
                    field,
                    flipped: true,
                };
                let last = self
                    .model
                    .instance(chain.parts[chain.parts.len() - 1])
                    .bridge;
                let g = groups.entry((scope, end)).or_default();
                g.draws.push((chain.end, draw));
                g.counted.insert(chain.end);
                g.counted.extend(last.map(|(_, b)| b));
            } else {
                partial.insert(scope);
            }
        }

        // A module's rail joined to the rail of the one part inside it that draws from the net
        // is that part's current (ADR 0077): the module's balance there is the two of them, and
        // the reference rails of the part's logic lines, which carry nothing, are not terms.
        for (own, inner) in self.rail_aliases() {
            let (Some((own_field, own_pos)), Some((inner_field, _))) =
                (self.model.draw_of(own), self.model.draw_of(inner))
            else {
                continue;
            };
            let module = self
                .model
                .instance(own)
                .parent
                .expect("a module's own rail");
            let root = self.find(own_pos.0);
            if let Some(group) = groups.get_mut(&(module, root))
                && group
                    .draws
                    .iter()
                    .all(|(_, d)| self.carries_nothing_else(d, own_field, inner_field))
            {
                group
                    .draws
                    .retain(|(_, d)| d.field == own_field || d.field == inner_field);
            }
        }

        let mut balances = Vec::new();
        for ((scope, root), mut group) in groups {
            // A return net with one draw on it is a rail whose `neg` goes nowhere in
            // particular; a net where nothing draws through a `pos` and nothing is
            // returned is only a chain end.
            if !group.supply
                && (!group.returns || group.draws.len() < 2 || partial.contains(&scope))
            {
                continue;
            }
            group
                .draws
                .sort_by_key(|(_, d)| self.model.path_of_field(d.field));
            let net = net_names
                .get(&root)
                .cloned()
                .unwrap_or_else(|| self.model.path_of(group.draws[0].0));
            let unmodelled = self.unmodelled_pins(scope, root, &group.owners, &group.counted);
            balances.push(Balance {
                net,
                scope,
                draws: group.draws.into_iter().map(|(_, d)| d).collect(),
                returns: !group.supply,
                unmodelled,
            });
        }
        balances.extend(self.balance_joins());
        balances.sort_by_key(|b| (self.model.path_of(b.scope), b.net.clone()));
        self.model.balances = std::sync::Arc::new(balances);
    }

    /// The pairs a module's rail and the rail of a part inside it that the module's `~`
    /// statements join, in which each rail is joined to one other only: a module rail joined to
    /// two parts' rails is what they draw together, and a part's rail joined to two of the
    /// module's is the supply of both (ADR 0077).
    ///
    /// This is the one exception to ADR 0007, which says `~` between rails equates every field
    /// except `current`, because a rail can feed several loads. A module that wraps a part (an
    /// input monitor with its bypass capacitor, a flash with its decoupling) joins its own rail to
    /// the part's with `~`. Without the exception the part's statement of what it draws
    /// (`.vs.current &= 0A..1.1mA`, ADR 0066) did not reach the module's rail unless the module
    /// said so in an equation, the same line in every wrapper (`.vdd.current ==
    /// .sensor.vs.current`). The module's balance (its rail, flipped, and the part's rail) is two
    /// terms and works out the module's rail, but only while the net inside the module has no
    /// other unknown current. A part whose logic line names its supply as the line's reference
    /// puts a second rail on the net with no statement of what it draws, and the balance had two
    /// unknowns: a sheet that adds up the draws of its modules had its check not evaluated, and
    /// the supply had no current (issue 189).
    ///
    /// The alias is made in the balance of the net, in the module's scope, and not as an equation
    /// between the two `current` fields, so a budget stated at the module's rail is judged as ADR
    /// 0038 says: `.monitor.vdd.current &= 0A..500uA` against a part that claims 0 to 1.1 mA is
    /// refused, with the numbers, as for a part that is not in a module. The rail of a logic line
    /// or of a bus's levels (a `reference`, ADR 0066) is not a term of it, and a current that is
    /// exactly zero is none either. It does not apply when the module's rail is joined to the
    /// rails of two parts or a part's rail to two of the module's; when a chain starts on the net
    /// (a pull-up from `.vdd.pos`); when another part has a rail on the net joined to the sensor's
    /// rail and not to the module's, or another rail of the module that is not a reference; when
    /// the part is not directly inside the module, or the inner member is a module's port and not
    /// a part's rail; or when the module is a board, whose rails stand for the outside.
    ///
    /// What was not taken:
    ///
    /// - An equation between the two `current` fields, pushed by `~`. The solver merges two
    ///   connected values to the range they share, so a budget stated at the module's rail would
    ///   narrow the part's claim to fit and the refusal of ADR 0066 and ADR 0038 would be lost for
    ///   every wrapped part: a supply allowed 0.5 mA of a part that may draw 1.1 mA would build
    ///   without a word.
    /// - The language saying that a module's interface rail is an alias of the part's,
    ///   everywhere. The module could no longer put another load on its own rail (a pull-up, a
    ///   second part, a pass-through rail), and it changes what the interface of a module means
    ///   for every module, which is broader than the fault.
    /// - Leaving it and writing the equation in each wrapper: the same line in every wrapper for
    ///   a fact the `~` already states, and a wrapper that forgets it gives a sheet that cannot
    ///   be solved, with a message that names the sheet's fields and not the module's missing
    ///   line.
    /// - Equating in every module's balance, whatever else is on the net: a wrapper with a
    ///   pull-up on its rail would be told that its rail draws what the part draws and nothing
    ///   more, and fail a balance that is right.
    ///
    /// Left open, for the owner: a module that wraps another module (`.vdd ~ .inner.vdd`) is not
    /// covered. The inner rail is a port, whose current the inner module's own balance sets from
    /// its part, and a module's balance of two terms already equates the two when nothing else is
    /// unknown; a reference rail on that net still stops it. The decision named a part inside;
    /// extending it to an inner module's port is the owner's. A wrapper's bus interface
    /// (`.spi.levels.reference ~ .vcc`, issue 188) is a member of the module and joined to the
    /// module's rail, not to a part's, so the rule does not see it: the reference of a part's own
    /// levels is left out of the balance, the module's own is not, and the module states
    /// `.current &= 0A` for it. The same applies to the `neg` pins: the return nets are checked
    /// and never solved from (ADR 0014).
    fn rail_aliases(&self) -> Vec<(InstId, InstId)> {
        let pairs = &self.model.rail_aliases;
        let mut counts: HashMap<InstId, usize> = HashMap::new();
        for &(own, inner) in pairs {
            *counts.entry(own).or_default() += 1;
            *counts.entry(inner).or_default() += 1;
        }
        let mut pairs: Vec<(InstId, InstId)> = pairs
            .iter()
            .copied()
            .filter(|(own, inner)| counts[own] == 1 && counts[inner] == 1)
            .collect();
        pairs.sort_by_key(|&(own, _)| self.model.path_of(own));
        pairs
    }

    /// Whether a term on the net of an alias leaves the alias true: the two rails themselves,
    /// a current that is exactly zero, or the reference rail of a logic line or a bus's levels,
    /// which names a supply for a level and carries nothing itself (ADR 0066). Any other draw
    /// on the net inside the module (a chain that starts on it, another part's rail, another
    /// rail of the module that others draw from) is a share of the module's current that the
    /// part's does not have.
    fn carries_nothing_else(&self, d: &Draw, own: FieldId, inner: FieldId) -> bool {
        if d.field == own || d.field == inner || self.model.draw_is_zero(d) {
            return true;
        }
        let rail = self.model.field(d.field).owner;
        self.model.instance(rail).parent.is_some_and(|p| {
            self.model.instance(p).kind == InstKind::Interface
                && (self.model.is_levels(p) || self.model.instance(p).type_name == "Logic")
        })
    }

    /// The pins on a net, seen from one scope, that its balance does not account for
    /// (ADR 0027). A balance is meaningful only when it is complete: a term that is the
    /// only one the checker knows of, among pins whose currents it does not, says nothing
    /// about itself. Each terminal on the net is accounted for when it is
    ///
    /// - a term, or the first or last part pin of a counted chain;
    /// - a pin that draws nothing by its part's model (see `Model::pin_draws_nothing`): any
    ///   pin of a part that draws nothing, or an `input` or `no_connect` pin of a part
    ///   whose supply is a rail;
    /// - inside a module of the scope that has a rail on the net, which stands for what is
    ///   inside it (ADR 0007);
    /// - outside the scope, when the scope has a rail of its own on the net, which stands
    ///   for the outside.
    ///
    /// Anything else is returned, by path order.
    fn unmodelled_pins(
        &mut self,
        scope: InstId,
        root: usize,
        owners: &[InstId],
        counted: &HashSet<InstId>,
    ) -> Vec<InstId> {
        let terminals: Vec<InstId> = self
            .model
            .nets
            .clone()
            .iter()
            .find(|n| n.pins.first().is_some_and(|p| self.find(p.0) == root))
            .map(|n| n.terminals.clone())
            .unwrap_or_default();
        let mut out: Vec<InstId> = terminals
            .into_iter()
            .filter(|&t| {
                if counted.contains(&t) {
                    return false;
                }
                if !self.model.is_inside(t, scope) {
                    return !owners.contains(&scope);
                }
                let Some(part) = self.model.owner_of(t) else {
                    return true;
                };
                // The child of the scope that holds the part: the part itself, or the
                // module whose rail stands for it.
                let mut child = part;
                while let Some(parent) = self.model.instance(child).parent {
                    if parent == scope {
                        break;
                    }
                    child = parent;
                }
                if child != part {
                    return !owners.contains(&child);
                }
                !self.model.pin_draws_nothing(t)
            })
            .collect();
        out.sort_by_key(|&p| self.model.path_of(p));
        out
    }

    /// What flows through each group of ports that a system joins. The ports are mirror
    /// statements of one flow, each written as its board writes it (a rail's current is
    /// what the outside draws through it), so they count as they are and add up to zero.
    fn balance_joins(&self) -> Vec<Balance> {
        let mut parent: HashMap<InstId, InstId> = HashMap::new();
        fn root(parent: &HashMap<InstId, InstId>, mut x: InstId) -> InstId {
            while let Some(&p) = parent.get(&x) {
                x = p;
            }
            x
        }
        let joins = self.model.joins();
        for &(a, b) in joins {
            let (ra, rb) = (root(&parent, a), root(&parent, b));
            if ra != rb {
                parent.insert(ra, rb);
            }
        }
        let mut groups: HashMap<InstId, Vec<InstId>> = HashMap::new();
        for &(a, b) in joins {
            for port in [a, b] {
                let ports = groups.entry(root(&parent, port)).or_default();
                if !ports.contains(&port) {
                    ports.push(port);
                }
            }
        }
        let mut balances = Vec::new();
        for (_, mut ports) in groups {
            ports.sort_by_key(|&p| self.model.path_of(p));
            let draws: Vec<Draw> = ports
                .iter()
                .filter_map(|&p| self.model.draw_of(p))
                .map(|(field, _)| Draw {
                    field,
                    flipped: false,
                })
                .collect();
            let Some(scope) = self
                .model
                .instance(ports[0])
                .parent
                .and_then(|board| self.model.instance(board).parent)
            else {
                continue;
            };
            if draws.len() < 2 {
                continue;
            }
            let net = ports
                .iter()
                .map(|&p| self.model.path_of(p))
                .collect::<Vec<_>>()
                .join(" ~ ");
            balances.push(Balance {
                net,
                scope,
                draws,
                returns: false,
                unmodelled: Vec::new(),
            });
        }
        balances
    }
}
