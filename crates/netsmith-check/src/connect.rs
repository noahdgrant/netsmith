//! Connections, bridges, catalogs, and the nets they produce.

use netsmith_lang::Span;
use netsmith_lang::ast::{self, CmpOp, ConnectOp};

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
            Res::Inst(i) => match self.model.instance(i).kind {
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
            },
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
                            self.connect_interfaces(a, b, path.span, true);
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
        for (name, member) in children {
            match (member, self.model.member(b, &name)) {
                (Member::Inst(x), Some(Member::Inst(y))) => match self.model.instance(x).kind {
                    InstKind::Pin if join_pins => self.union(x, y),
                    InstKind::Interface => {
                        self.connect_interfaces(x, y, at, join_pins);
                    }
                    _ => {}
                },
                // A current is not shared by a connection: a rail can feed several loads.
                // The currents are summed at the net instead (`Balance`).
                (Member::Field(x), Some(Member::Field(y)))
                    if !self.model.is_flow(x) && !self.model.is_level(x) =>
                {
                    self.model.equalities.push((x, y))
                }
                _ => {}
            }
        }
        true
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
        for path in &c.fields {
            match self.resolve_path(path) {
                Res::Field(f) => fields.push(f),
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
        self.model.instances[cur.0].catalog = Some(Catalog {
            name: c.name.name.clone(),
            fields,
            key,
        });
    }

    /// The net name a pin asked for, if any.
    fn requested_name(&self, pin: InstId) -> Option<String> {
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
    }

    /// Give each part with a `bias` (a capacitor) the voltage of the rail it sits across.
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
    fn derive_bias(&mut self) {
        let parts: Vec<(InstId, _, InstId, InstId)> = self
            .model
            .instances()
            .filter_map(|(id, _)| {
                let (bias, a, b) = self.model.bias_of(id)?;
                Some((id, bias, a, b))
            })
            .collect();
        let rails: Vec<_> = self
            .model
            .instances()
            .filter_map(|(id, _)| Some((id, self.model.rail_of(id)?)))
            .collect();
        for (part, bias, a, b) in parts {
            let (na, nb) = (self.find(a.0), self.find(b.0));
            if na == nb {
                continue;
            }
            let scope = self.model.instance(part).parent;
            let mut across: Vec<(String, _)> = Vec::new();
            for &(rail, (voltage, pos, neg)) in &rails {
                let Some(owner) = self.model.owner_of(rail) else {
                    continue;
                };
                if Some(owner) != scope && self.model.instance(owner).parent != scope {
                    continue;
                }
                let (np, nn) = (self.find(pos.0), self.find(neg.0));
                if (np, nn) == (na, nb) || (np, nn) == (nb, na) {
                    across.push((self.model.path_of(rail), voltage));
                }
            }
            across.sort();
            let Some((_, voltage)) = across.first() else {
                continue;
            };
            // Point at the statement that gave the bias, if the design gave one: that is
            // where a contradiction is.
            let (source, span) = match self.model.field(bias).narrowed_at.last() {
                Some(&at) => at,
                None => self.model.instance(part).at(),
            };
            self.model.relations.push(Relation {
                lhs: RExpr::Field(bias),
                op: CmpOp::EqEq,
                rhs: RExpr::Field(*voltage),
                source,
                span,
            });
        }
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
        self.model.balances = balances;
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
