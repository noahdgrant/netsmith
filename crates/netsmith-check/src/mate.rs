//! Connector mating in a system (ADR 0040).
//!
//! `~` between the `.mating` of two connectors says that the connectors are mated: contact N
//! of one meets contact N of the other. The system names no pins and no nets. After the nets
//! of every board are known, [`Checker::analyse_mates`] reads what each board has behind each
//! contact (its role), checks that the two sides can meet, derives the port joins the mate
//! stands for, finds the grounds it joins and adds the requirement that a contact carries the
//! current through it.

use std::collections::{BTreeMap, HashMap};

use netsmith_lang::Span;
use netsmith_lang::ast::{BinOp, CmpOp};
use netsmith_quantity::{Dimension, Quantity};

use crate::checker::Checker;
use crate::model::{
    Contact, ContactSide, Domain, FieldId, GroundGroup, GroundNet, InstId, InstKind, Mate,
    MateDecl, Member, Model, PinKind, RExpr, Relation, Role,
};

/// What the board behind one contact has on its net: the role, and the pins that say it.
struct Side {
    pin: InstId,
    role: Role,
    net: Option<String>,
    /// The root of the net in the union-find, to see which contacts share one.
    root: Option<usize>,
    outputs: Vec<InstId>,
    inputs: Vec<InstId>,
    bidirectional: Vec<InstId>,
    tri_state: Vec<InstId>,
}

impl Side {
    fn drives(&self) -> bool {
        !self.outputs.is_empty() || !self.bidirectional.is_empty() || !self.tri_state.is_empty()
    }
}

/// One pin path of a port with the contacts of the mate it is on, on both boards.
type Carried = (String, Vec<usize>);

/// A net of a board, to find the nets that mated ground contacts join.
type BoardNet = (InstId, String);

/// How two sides of a contact meet.
enum Meeting {
    Fine,
    /// The two cannot be joined: what is wrong, in words.
    Refused(String),
}

impl Checker<'_> {
    /// Whether an interface is the `.mating` of a connector part.
    pub(crate) fn is_mating(&self, iface: InstId) -> bool {
        let inst = self.model.instance(iface);
        inst.kind == InstKind::Interface
            && inst.type_name == "Mating"
            && inst
                .parent
                .is_some_and(|p| self.model.instance(p).kind == InstKind::Part)
    }

    /// `.carrier.slot_a.mating ~ .sensor.j1.mating`: record that two connectors are mated.
    /// Everything that needs the nets is checked later ([`Checker::analyse_mates`]).
    pub(crate) fn mate(
        &mut self,
        a: InstId,
        b: InstId,
        at_a: &netsmith_lang::ast::Path,
        at_b: &netsmith_lang::ast::Path,
    ) {
        if !(self.is_mating(a) && self.is_mating(b)) {
            let bad = if self.is_mating(a) { at_b } else { at_a };
            self.error(
                bad.span,
                format!(
                    "`{}` is not the `.mating` of a connector; a connector's `.mating` mates only with another connector's",
                    bad.to_source()
                ),
            );
            return;
        }
        let part = |m: InstId| self.model.instance(m).parent.expect("a mating has a part");
        let (pa, pb) = (part(a), part(b));
        let span = Span::new(at_a.span.start, at_b.span.end);
        if self.model.board_of(pa) == self.model.board_of(pb) {
            self.error(
                span,
                format!(
                    "`{}` and `{}` are on the same board; a system mates connectors of different boards",
                    at_a.to_source(),
                    at_b.to_source()
                ),
            );
            return;
        }
        for (p, at) in [(pa, at_a), (pb, at_b)] {
            if self.model.mate_decls.iter().any(|d| d.a == p || d.b == p) {
                self.error(
                    at.span,
                    format!(
                        "`{}` is already mated; a connector mates with one other",
                        at.to_source().trim_end_matches(".mating")
                    ),
                );
                return;
            }
        }
        let source = self.cur_source();
        self.model.mate_decls.push(MateDecl {
            a: pa,
            b: pb,
            source,
            span,
        });
    }

    // ---- reading a connector --------------------------------------------------------

    /// `.carrier.slot_a`: the path of a part from the system.
    fn shown(&self, part: InstId) -> String {
        format!("`.{}`", self.model.path_of(part))
    }

    fn text(&self, part: InstId, name: &str) -> Option<String> {
        let mating = self.model.mating_of(part)?;
        match self.model.member(mating, name)? {
            Member::Field(f) => match &self.model.field(f).domain {
                Domain::Text(t) => Some(t.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    fn quantity(&self, part: InstId, name: &str) -> Option<Quantity> {
        let mating = self.model.mating_of(part)?;
        match self.model.member(mating, name)? {
            Member::Field(f) => match &self.model.field(f).domain {
                Domain::Quantity(q) => Some(*q),
                _ => None,
            },
            _ => None,
        }
    }

    fn field(&self, part: InstId, name: &str) -> Option<FieldId> {
        let mating = self.model.mating_of(part)?;
        match self.model.member(mating, name)? {
            Member::Field(f) => Some(f),
            _ => None,
        }
    }

    /// The number of contacts of a connector, if it states a whole number.
    fn count(&self, part: InstId, name: &str) -> Option<usize> {
        let q = self.quantity(part, name)?;
        (q.is_point() && q.mid() >= 0.0 && q.mid().fract() == 0.0).then(|| q.mid() as usize)
    }

    // ---- the analysis -----------------------------------------------------------------

    /// Read every mate against the nets of the boards, once the nets are known: the checks of
    /// the connectors, of the contacts, the ports joined by them, the grounds, and the
    /// current each contact carries. Then refuse the port joins the author wrote that a mate
    /// already makes.
    pub(crate) fn analyse_mates(&mut self) {
        let decls = self.model.mate_decls.clone();
        let mut mates = Vec::new();
        let mut read = Vec::new();
        for decl in &decls {
            let (mate, sides) = self.read_mate(decl);
            mates.push(mate);
            read.push(sides);
        }
        // The current through a mate can be read only when every mate's ports are known: a port
        // joined to several is a hub, and what a mate carries is what the other side draws.
        let mut joins_of: HashMap<InstId, usize> = HashMap::new();
        for mate in &mates {
            for &(p, q) in &mate.ports {
                *joins_of.entry(p).or_default() += 1;
                *joins_of.entry(q).or_default() += 1;
            }
        }
        for ((decl, mate), sides) in decls.iter().zip(&mates).zip(&read) {
            if let Some(sides) = sides {
                self.require_current(decl, sides, &mate.ports, &joins_of);
            }
        }
        self.model.mates = mates;
        self.refuse_duplicate_joins();
        self.find_ground_groups();
    }

    fn fail(&mut self, decl: &MateDecl, message: String) {
        self.report(decl.source, decl.span, message);
    }

    /// The mate with what is behind its contacts, and the two sides of each contact when the
    /// mate is sound enough to check the current of.
    fn read_mate(&mut self, decl: &MateDecl) -> (Mate, Option<Vec<(Side, Side)>>) {
        let mut mate = Mate {
            a: decl.a,
            b: decl.b,
            source: decl.source,
            span: decl.span,
            contacts: Vec::new(),
            ports: Vec::new(),
            reversible: None,
        };
        let Some(positions) = self.connectors_mate(decl) else {
            return (mate, None);
        };

        // Contact N of one is contact N of the other.
        let mut sides: Vec<(Side, Side)> = Vec::new();
        for n in 1..=positions {
            let (Some(pa), Some(pb)) = (self.contact_pin(decl.a, n), self.contact_pin(decl.b, n))
            else {
                for part in [decl.a, decl.b] {
                    if self.contact_pin(part, n).is_none() {
                        let message = format!(
                            "{} has no pin `p{n}`, so it has no contact {n}; a connector's contact N is its pin `pN`",
                            self.shown(part)
                        );
                        self.fail(decl, message);
                    }
                }
                return (mate, None);
            };
            sides.push((self.side(pa), self.side(pb)));
        }

        let mut clean = true;
        for (i, (a, b)) in sides.iter().enumerate() {
            if let Meeting::Refused(why) = self.meet(a, b) {
                clean = false;
                let message = self.contact_message(decl, i + 1, positions, a, b, &why);
                self.fail(decl, message);
            }
        }

        mate.contacts = sides
            .iter()
            .enumerate()
            .map(|(i, (a, b))| Contact {
                number: i + 1,
                a: ContactSide {
                    pin: a.pin,
                    role: a.role,
                    net: a.net.clone(),
                },
                b: ContactSide {
                    pin: b.pin,
                    role: b.role,
                    net: b.net.clone(),
                },
            })
            .collect();

        if clean {
            self.carries_ground(decl, &sides);
            mate.reversible = self.reversible(decl, &sides);
            mate.ports = self.derive_ports(decl, &sides);
            self.equate_unported_rails(decl, &sides, &mate.ports);
        }
        (mate, clean.then_some(sides))
    }

    /// The pin of a connector for contact `n`.
    fn contact_pin(&self, part: InstId, n: usize) -> Option<InstId> {
        match self.model.member(part, &format!("p{n}"))? {
            Member::Inst(p) if self.model.instance(p).kind == InstKind::Pin => Some(p),
            _ => None,
        }
    }

    /// Whether the two connectors can mate at all: kind, gender, pitch, rows and positions.
    /// Reports what does not, and gives the number of contacts when they do.
    fn connectors_mate(&mut self, decl: &MateDecl) -> Option<usize> {
        let (a, b) = (decl.a, decl.b);
        let mut ok = true;
        for part in [a, b] {
            for name in ["kind", "gender", "pitch", "rows", "positions"] {
                let said = match name {
                    "kind" | "gender" => self.text(part, name).is_some(),
                    "positions" | "rows" => self.count(part, name).is_some(),
                    _ => self.quantity(part, name).is_some(),
                };
                if !said {
                    let message = format!(
                        "{} (a `{}`) does not state its `.mating.{name}`, so what it mates with cannot be checked",
                        self.shown(part),
                        self.model.instance(part).type_name
                    );
                    self.fail(decl, message);
                    ok = false;
                }
            }
        }
        if !ok {
            return None;
        }
        let ty = |p: InstId| self.model.instance(p).type_name.clone();
        let (sa, sb) = (self.shown(a), self.shown(b));
        let (ka, kb) = (
            self.text(a, "kind").expect("checked"),
            self.text(b, "kind").expect("checked"),
        );
        if ka != kb {
            self.fail(
                decl,
                format!(
                    "{sa} is a `{ka}` connector and {sb} is a `{kb}` one; connectors of different kinds do not mate"
                ),
            );
            return None;
        }
        if ka != "pin_header" {
            self.fail(
                decl,
                format!(
                    "{sa} and {sb} are `{ka}` connectors, and only `pin_header` mates are modelled"
                ),
            );
            return None;
        }
        let (ga, gb) = (
            self.text(a, "gender").expect("checked"),
            self.text(b, "gender").expect("checked"),
        );
        for (part, g) in [(a, &ga), (b, &gb)] {
            if g != "header" && g != "socket" {
                let message = format!(
                    "{} has the gender `{g}`; a `pin_header` is a `header` or a `socket`",
                    self.shown(part)
                );
                self.fail(decl, message);
                return None;
            }
        }
        if ga == gb {
            self.fail(
                decl,
                format!(
                    "{sa} and {sb} are both {ga}s ({} and {}); a {ga} mates with a {}",
                    ty(a),
                    ty(b),
                    if ga == "header" { "socket" } else { "header" }
                ),
            );
            return None;
        }
        let (pa, pb) = (self.count(a, "positions")?, self.count(b, "positions")?);
        if pa != pb {
            self.fail(
                decl,
                format!(
                    "{sa} ({}) has {pa} positions and {sb} ({}) has {pb}; a mate joins contact N to contact N, so both need the same number",
                    ty(a),
                    ty(b)
                ),
            );
            return None;
        }
        let (ra, rb) = (self.count(a, "rows")?, self.count(b, "rows")?);
        if ra != rb {
            self.fail(
                decl,
                format!(
                    "{sa} ({}) has {ra} rows and {sb} ({}) has {rb}",
                    ty(a),
                    ty(b)
                ),
            );
            return None;
        }
        let (la, lb) = (
            self.quantity(a, "pitch").expect("checked"),
            self.quantity(b, "pitch").expect("checked"),
        );
        if la != lb {
            self.fail(
                decl,
                format!(
                    "{sa} ({}) has a pitch of {la} and {sb} ({}) has {lb}; they do not fit together",
                    ty(a),
                    ty(b)
                ),
            );
            return None;
        }
        Some(pa)
    }

    /// What is behind a contact: the net of the connector's pin in its board and what it is.
    fn side(&mut self, pin: InstId) -> Side {
        let part = self.model.owner_of(pin);
        let mut side = Side {
            pin,
            role: Role::Open,
            net: None,
            root: None,
            outputs: Vec::new(),
            inputs: Vec::new(),
            bidirectional: Vec::new(),
            tri_state: Vec::new(),
        };
        let Some(net) = self.model.net_of(pin).cloned() else {
            return side;
        };
        side.net = Some(net.name.clone());
        side.root = Some(self.find(pin.0));
        let (mut pos, mut neg) = (false, false);
        for &p in &net.pins {
            if let Some(iface) = self.model.instance(p).parent
                && let Some((_, rpos, rneg)) = self.model.rail_of(iface)
            {
                pos |= p == rpos;
                neg |= p == rneg;
            }
            if net.terminals.contains(&p) && self.model.owner_of(p) != part {
                match self.model.pin_facts(p).map(|r| r.kind) {
                    Some(PinKind::Output) => side.outputs.push(p),
                    Some(PinKind::Input) => side.inputs.push(p),
                    Some(PinKind::Bidirectional) => side.bidirectional.push(p),
                    Some(PinKind::TriState) => side.tri_state.push(p),
                    _ => {}
                }
            }
        }
        side.role = if pos {
            Role::Power
        } else if neg {
            Role::Ground
        } else if side.drives() || !side.inputs.is_empty() {
            Role::Signal
        } else {
            Role::Open
        };
        side
    }

    /// Whether two sides can meet, and why not.
    fn meet(&self, a: &Side, b: &Side) -> Meeting {
        use Role::*;
        let refuse = |why: &str| Meeting::Refused(why.to_string());
        match (a.role, b.role) {
            (Ground, Ground) | (Power, Power) | (Open, Open) => Meeting::Fine,
            (Ground | Power, Open) | (Open, Ground | Power) => Meeting::Fine,
            (Ground, Power) | (Power, Ground) => {
                refuse("is ground on one board and power on the other")
            }
            (Ground | Power, Signal) | (Signal, Ground | Power) => {
                let other = if a.role == Signal { b.role } else { a.role };
                refuse(&format!(
                    "is a signal on one board and {} on the other",
                    other.word()
                ))
            }
            (Signal, Signal) | (Signal, Open) | (Open, Signal) => {
                if !a.outputs.is_empty() && !b.outputs.is_empty() {
                    return refuse(
                        "is an output on both boards, and two outputs cannot share a net",
                    );
                }
                if (!a.outputs.is_empty() && !b.tri_state.is_empty())
                    || (!a.tri_state.is_empty() && !b.outputs.is_empty())
                {
                    return refuse(
                        "is an output on one board and a tri_state output on the other: a line that several parts drive needs every driver to be tri_state",
                    );
                }
                let driven = a.drives() || b.drives();
                let listens = !a.inputs.is_empty() || !b.inputs.is_empty();
                if listens && !driven {
                    return refuse("has an input and nothing that drives it on either board");
                }
                Meeting::Fine
            }
        }
    }

    /// One line for a contact that does not work: both sides, with their nets.
    fn contact_message(
        &self,
        decl: &MateDecl,
        n: usize,
        positions: usize,
        a: &Side,
        b: &Side,
        why: &str,
    ) -> String {
        let describe = |part: InstId, s: &Side| {
            let net = s
                .net
                .as_ref()
                .map_or("on no net".to_string(), |n| format!("on net `{n}`"));
            let pins = |v: &[InstId], what: &str| {
                if v.is_empty() {
                    String::new()
                } else {
                    format!(
                        " ({what} {})",
                        v.iter()
                            .map(|&p| format!("`{}`", self.model.path_of(p)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            };
            let detail = match s.role {
                Role::Signal => format!(
                    "{}{}{}{}",
                    pins(&s.outputs, "output"),
                    pins(&s.tri_state, "tri_state"),
                    pins(&s.inputs, "input"),
                    pins(&s.bidirectional, "bidirectional")
                ),
                _ => String::new(),
            };
            format!(
                "{} pin {n} is {net}, {}{detail}",
                self.shown(part),
                s.role.word()
            )
        };
        let mut message = format!(
            "contact {n} of the mated connectors {why}: {}; {}",
            describe(decl.a, a),
            describe(decl.b, b)
        );
        if n != positions + 1 - n {
            message.push_str(&format!(
                "; a mate joins contact N to contact N, so if one is meant to be turned end for end, its pin {n} would meet pin {}",
                positions + 1 - n
            ));
        }
        message
    }

    /// A mate that carries power or signals must carry ground too: at least one contact
    /// that is ground on both boards.
    fn carries_ground(&mut self, decl: &MateDecl, sides: &[(Side, Side)]) {
        let carries = sides.iter().any(|(a, b)| {
            matches!(a.role, Role::Power | Role::Signal)
                || matches!(b.role, Role::Power | Role::Signal)
        });
        let grounded = sides
            .iter()
            .any(|(a, b)| a.role == Role::Ground && b.role == Role::Ground);
        if carries && !grounded {
            let first = sides
                .iter()
                .position(|(a, b)| {
                    matches!(a.role, Role::Power | Role::Signal)
                        || matches!(b.role, Role::Power | Role::Signal)
                })
                .expect("it carries");
            let message = format!(
                "the mate of {} and {} carries power or signals (contact {} is {} on {} and {} on {}) but no ground: no contact is ground on both boards, so the two boards share no ground",
                self.shown(decl.a),
                self.shown(decl.b),
                first + 1,
                sides[first].0.role.word(),
                self.shown(decl.a),
                sides[first].1.role.word(),
                self.shown(decl.b),
            );
            self.fail(decl, message);
        }
    }

    /// Whether the mate would also work if one connector were turned end for end (contact N
    /// meeting contact `positions + 1 - N`), and what would swap. A keyed connector goes in
    /// one way only.
    fn reversible(&mut self, decl: &MateDecl, sides: &[(Side, Side)]) -> Option<String> {
        let n = sides.len();
        if n < 2 {
            return None;
        }
        let keyed = |part: InstId| {
            self.text(part, "keyed")
                .is_some_and(|k| !k.is_empty() && k != "none")
        };
        if keyed(decl.a) || keyed(decl.b) {
            return None;
        }
        let turned_ok = (0..n).all(|i| {
            let (a, b) = (&sides[i].0, &sides[n - 1 - i].1);
            matches!(self.meet(a, b), Meeting::Fine) && self.voltages_meet(a, b)
        });
        if !turned_ok {
            return None;
        }
        // The contacts whose partner changes: the first pair that would swap.
        let swapped = (0..n).find(|&i| {
            i < n - 1 - i
                && (sides[i].0.net != sides[n - 1 - i].0.net
                    || sides[i].1.net != sides[n - 1 - i].1.net)
        })?;
        let (i, j) = (swapped, n - 1 - swapped);
        let net = |s: &Side| s.net.clone().unwrap_or_else(|| "no net".to_string());
        Some(format!(
            "the pinout of the mate of {} and {} also works with one connector turned end for end, and neither is keyed: contact {} ({} on {}, {} on {}) and contact {} ({} on {}, {} on {}) would swap places and every contact would still be accepted; only a keyed connector (`.mating.keyed`) goes in one way",
            self.shown(decl.a),
            self.shown(decl.b),
            i + 1,
            net(&sides[i].0),
            self.shown(decl.a),
            net(&sides[i].1),
            self.shown(decl.b),
            j + 1,
            net(&sides[j].0),
            self.shown(decl.a),
            net(&sides[j].1),
            self.shown(decl.b),
        ))
    }

    /// Whether two power contacts can be at one voltage, by what the boards state: rails whose
    /// voltages are known and do not overlap cannot be joined.
    fn voltages_meet(&self, a: &Side, b: &Side) -> bool {
        if a.role != Role::Power || b.role != Role::Power {
            return true;
        }
        let stated = |s: &Side| match self.model.field(self.rail_voltage(s)?).domain {
            Domain::Quantity(q) => Some(q),
            _ => None,
        };
        match (stated(a), stated(b)) {
            (Some(x), Some(y)) => matches!(x.intersect(&y), Ok(Some(_))),
            _ => true,
        }
    }

    // ---- the ports a mate joins -----------------------------------------------------------

    /// The pins of a port with the path of each below the port.
    fn port_pins(&self, port: InstId) -> Vec<(String, InstId)> {
        let mut out = Vec::new();
        let mut stack = vec![(String::new(), port)];
        while let Some((prefix, at)) = stack.pop() {
            for (name, m) in &self.model.instance(at).children {
                if let Member::Inst(i) = m {
                    let path = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{prefix}.{name}")
                    };
                    match self.model.instance(*i).kind {
                        InstKind::Pin => out.push((path, *i)),
                        InstKind::Interface => stack.push((path, *i)),
                        _ => {}
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// The ports (interfaces with pins) a board declares directly.
    fn ports_of(&self, board: InstId) -> Vec<InstId> {
        self.model
            .instance(board)
            .children
            .iter()
            .filter_map(|(_, m)| match m {
                Member::Inst(i)
                    if self.model.instance(*i).kind == InstKind::Interface
                        && !self.port_pins(*i).is_empty() =>
                {
                    Some(*i)
                }
                _ => None,
            })
            .collect()
    }

    /// The contacts of `connector` that are on the same net as `pin`.
    fn contacts_on(&mut self, connector: InstId, pin: InstId, positions: usize) -> Vec<usize> {
        let root = self.find(pin.0);
        let Some(net) = self.model.net_of(pin) else {
            return Vec::new();
        };
        if net.pins.is_empty() {
            return Vec::new();
        }
        (1..=positions)
            .filter(|&n| {
                self.contact_pin(connector, n)
                    .is_some_and(|c| self.find(c.0) == root)
            })
            .collect()
    }

    /// The pins of two ports that a mate carries, as `(path, contacts)`: the pin of `p` and
    /// the pin of `q` of one path are on the same contact. `None` if a pin has no such
    /// partner and `whole` is set.
    fn carried(
        &mut self,
        decl: &MateDecl,
        positions: usize,
        p: InstId,
        q: InstId,
    ) -> Option<(Vec<Carried>, bool)> {
        // `p` is on the board of `a` and `q` on the board of `b`, or the reverse.
        let on = |m: &Model, port: InstId, part: InstId| m.board_of(port) == m.board_of(part);
        let (cp, cq) = if on(&self.model, p, decl.a) && on(&self.model, q, decl.b) {
            (decl.a, decl.b)
        } else if on(&self.model, p, decl.b) && on(&self.model, q, decl.a) {
            (decl.b, decl.a)
        } else {
            return None;
        };
        let (pp, pq) = (self.port_pins(p), self.port_pins(q));
        let mut found = Vec::new();
        let mut whole = !pp.is_empty() && pp.len() == pq.len();
        for (path, a) in &pp {
            let Some((_, b)) = pq.iter().find(|(qp, _)| qp == path) else {
                whole = false;
                continue;
            };
            let (ca, cb) = (
                self.contacts_on(cp, *a, positions),
                self.contacts_on(cq, *b, positions),
            );
            let shared: Vec<usize> = ca.iter().copied().filter(|n| cb.contains(n)).collect();
            if shared.is_empty() {
                whole = false;
            } else {
                found.push((path.clone(), shared));
            }
        }
        Some((found, whole))
    }

    /// A port of one board whose pins are all on contacts that the other board's port, of the
    /// same kind, has its pins on, is joined to it by the mate, as `~` between the two
    /// would: the values are equated and the currents are balanced.
    fn derive_ports(&mut self, decl: &MateDecl, sides: &[(Side, Side)]) -> Vec<(InstId, InstId)> {
        let positions = sides.len();
        let (Some(ba), Some(bb)) = (self.model.board_of(decl.a), self.model.board_of(decl.b))
        else {
            return Vec::new();
        };
        let mut pairs: Vec<(InstId, InstId)> = Vec::new();
        for p in self.ports_of(ba) {
            for q in self.ports_of(bb) {
                if self.model.instance(p).def != self.model.instance(q).def {
                    continue;
                }
                if let Some((_, true)) = self.carried(decl, positions, p, q) {
                    pairs.push((p, q));
                }
            }
        }
        // Two ports of one board on the same contacts (an input and an output on one net) leave
        // the mate unable to say which one the other board's port is joined to.
        let mut joined = Vec::new();
        for &(p, q) in &pairs {
            let on_p: Vec<InstId> = pairs.iter().filter(|x| x.1 == q).map(|x| x.0).collect();
            let on_q: Vec<InstId> = pairs.iter().filter(|x| x.0 == p).map(|x| x.1).collect();
            if on_p.len() > 1 || on_q.len() > 1 {
                let (many, one) = if on_p.len() > 1 {
                    (&on_p, q)
                } else {
                    (&on_q, p)
                };
                let mut names: Vec<String> = many
                    .iter()
                    .map(|&m| format!("`.{}`", self.model.path_of(m)))
                    .collect();
                names.sort();
                let message = format!(
                    "{} are all on the contacts of the mate of {} and {} that `.{}` is on, so the mate cannot say which of them is joined to it; give the board one port for that net",
                    names.join(", "),
                    self.shown(decl.a),
                    self.shown(decl.b),
                    self.model.path_of(one)
                );
                self.fail(decl, message);
                continue;
            }
            let span = decl.span;
            self.connect_interfaces(p, q, span, false);
            self.model.joins.push((p, q));
            joined.push((p, q));
        }
        joined
    }

    /// Power contacts whose rails are not ports of a board (a regulator's output wired
    /// straight to a connector) are not joined as ports, but their voltages must still
    /// agree: the same equality a port join would make, on the rails the contacts sit on.
    fn equate_unported_rails(
        &mut self,
        _decl: &MateDecl,
        sides: &[(Side, Side)],
        ports: &[(InstId, InstId)],
    ) {
        let covered = |this: &Self, pin: InstId| {
            ports.iter().any(|&(p, q)| {
                [p, q].iter().any(|&port| {
                    this.port_pins(port)
                        .iter()
                        .any(|&(_, x)| this.model.net_of(x).is_some_and(|n| n.pins.contains(&pin)))
                })
            })
        };
        for (a, b) in sides {
            if a.role != Role::Power || b.role != Role::Power {
                continue;
            }
            if covered(self, a.pin) && covered(self, b.pin) {
                continue;
            }
            let (Some(va), Some(vb)) = (self.rail_voltage(a), self.rail_voltage(b)) else {
                continue;
            };
            if !self.model.equalities.contains(&(va, vb)) {
                self.model.equalities.push((va, vb));
            }
        }
    }

    /// The voltage field of the rail whose `pos` is on a side's net, the first by path; a
    /// board's own port before one inside a part or module.
    fn rail_voltage(&self, side: &Side) -> Option<FieldId> {
        let net = self.model.net_of(side.pin)?;
        let mut rails: Vec<(bool, String, FieldId)> = Vec::new();
        for &p in &net.pins {
            let Some(iface) = self.model.instance(p).parent else {
                continue;
            };
            if let Some((voltage, pos, _)) = self.model.rail_of(iface)
                && p == pos
            {
                let on_board = self
                    .model
                    .instance(iface)
                    .parent
                    .is_some_and(|o| self.model.instance(o).kind == InstKind::Board);
                rails.push((!on_board, self.model.path_of(iface), voltage));
            }
        }
        rails.sort();
        rails.first().map(|r| r.2)
    }

    // ---- what a contact carries ------------------------------------------------------------

    /// The requirement that the contacts of a mate carry the current through them: each
    /// connector's `pin_current` times the contacts that carry a rail's `pos` (or the ground
    /// that the rails of the mate return through) is at least the current of the rail, in
    /// either direction. A connector with no rating leaves it unevaluated.
    fn require_current(
        &mut self,
        decl: &MateDecl,
        sides: &[(Side, Side)],
        ports: &[(InstId, InstId)],
        joins_of: &HashMap<InstId, usize>,
    ) {
        // A pair of joined ports is one flow, written from both ends. What the mate carries is
        // the current of the port that is joined to fewer others: a plug-in's own draw, not the
        // carrier's whole output, which the other plug-ins share.
        let mut wants: Vec<(usize, Vec<FieldId>)> = Vec::new();
        // The rails that return through one ground (keyed by its net on the first board) add up.
        let mut by_ground: BTreeMap<usize, (usize, Vec<FieldId>)> = BTreeMap::new();
        for &(p, q) in ports {
            let own = if joins_of[&q] < joins_of[&p] { q } else { p };
            let mine = |s: &(Side, Side)| if own == q { s.1.root } else { s.0.root };
            let Some((current, _)) = self.model.draw_of(own) else {
                continue;
            };
            let (Some(Member::Inst(pos)), Some(Member::Inst(neg))) =
                (self.model.member(own, "pos"), self.model.member(own, "neg"))
            else {
                continue;
            };
            let (rpos, rneg) = (self.find(pos.0), self.find(neg.0));
            let power = sides
                .iter()
                .filter(|s| {
                    mine(s) == Some(rpos) && s.0.role == Role::Power && s.1.role == Role::Power
                })
                .count();
            if power > 0 {
                wants.push((power, vec![current]));
            }
            let ground: Vec<&(Side, Side)> = sides
                .iter()
                .filter(|s| {
                    mine(s) == Some(rneg) && s.0.role == Role::Ground && s.1.role == Role::Ground
                })
                .collect();
            if let Some(key) = ground.first().and_then(|s| s.0.root) {
                by_ground
                    .entry(key)
                    .or_insert_with(|| (ground.len(), Vec::new()))
                    .1
                    .push(current);
            }
        }
        wants.extend(by_ground.into_values());
        wants.sort();
        wants.dedup();

        let number = |n: usize| RExpr::Const(Quantity::point(Dimension::NONE, n as f64).unwrap());
        for part in [decl.a, decl.b] {
            let Some(rating) = self.field(part, "pin_current") else {
                continue;
            };
            for (contacts, fields) in &wants {
                let carried = fields
                    .iter()
                    .map(|&f| RExpr::Field(f))
                    .reduce(|x, y| RExpr::Bin(BinOp::Add, Box::new(x), Box::new(y)))
                    .expect("a rail has a current");
                let capacity = RExpr::Bin(
                    BinOp::Mul,
                    Box::new(number(*contacts)),
                    Box::new(RExpr::Field(rating)),
                );
                for current in [carried.clone(), RExpr::Neg(Box::new(carried))] {
                    self.model.relations.push(Relation {
                        lhs: capacity.clone(),
                        op: CmpOp::Ge,
                        rhs: current,
                        source: decl.source,
                        span: decl.span,
                    });
                }
            }
        }
    }

    // ---- joins the author wrote, and grounds --------------------------------------------------

    /// A port join that a mate already makes is a second way to say it: refuse it.
    fn refuse_duplicate_joins(&mut self) {
        let explicit = self.model.explicit_joins.clone();
        let mates = self.model.mates.clone();
        for join in explicit {
            for (index, mate) in mates.iter().enumerate() {
                let decl = MateDecl {
                    a: mate.a,
                    b: mate.b,
                    source: mate.source,
                    span: mate.span,
                };
                let positions = mate.contacts.len();
                if positions == 0 {
                    continue;
                }
                let Some((found, _)) = self.carried(&decl, positions, join.a, join.b) else {
                    continue;
                };
                if found.is_empty() {
                    continue;
                }
                let mut contacts: Vec<usize> =
                    found.iter().flat_map(|(_, c)| c.iter().copied()).collect();
                contacts.sort();
                contacts.dedup();
                let list = contacts
                    .iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let message = format!(
                    "`.{}` and `.{}` are already joined by the mate of {} and {} at contact{} {list}; the mate derives this join, so remove this one (a port join is for ports that no connector carries)",
                    self.model.path_of(join.a),
                    self.model.path_of(join.b),
                    self.shown(mate.a),
                    self.shown(mate.b),
                    if contacts.len() == 1 { "" } else { "s" },
                );
                let _ = index;
                self.report(join.source, join.span, message);
                break;
            }
        }
    }

    /// Which boards share a ground through the mates: the nets joined by a contact that is
    /// ground on both sides.
    fn find_ground_groups(&mut self) {
        let mut parent: HashMap<(InstId, String), (InstId, String)> = HashMap::new();
        fn root(
            parent: &HashMap<(InstId, String), (InstId, String)>,
            mut x: (InstId, String),
        ) -> (InstId, String) {
            while let Some(p) = parent.get(&x) {
                x = p.clone();
            }
            x
        }
        let mut per_mate: Vec<Vec<(BoardNet, BoardNet)>> = Vec::new();
        let mates = self.model.mates.clone();
        for mate in &mates {
            let mut pairs = Vec::new();
            let (ba, bb) = (self.model.board_of(mate.a), self.model.board_of(mate.b));
            for c in &mate.contacts {
                if let (Role::Ground, Role::Ground, Some(na), Some(nb), Some(ba), Some(bb)) =
                    (c.a.role, c.b.role, &c.a.net, &c.b.net, ba, bb)
                {
                    let (x, y) = ((ba, na.clone()), (bb, nb.clone()));
                    let (rx, ry) = (root(&parent, x.clone()), root(&parent, y.clone()));
                    if rx != ry {
                        parent.insert(rx, ry);
                    }
                    pairs.push((x, y));
                }
            }
            per_mate.push(pairs);
        }
        let mut groups: BTreeMap<(usize, String), GroundGroup> = BTreeMap::new();
        for (index, pairs) in per_mate.iter().enumerate() {
            for (x, y) in pairs {
                let r = root(&parent, x.clone());
                let key = (r.0.0, r.1.clone());
                let group = groups.entry(key).or_insert_with(|| GroundGroup {
                    nets: Vec::new(),
                    contacts: Vec::new(),
                });
                for (board, net) in [x, y] {
                    let n = GroundNet {
                        board: *board,
                        net: net.clone(),
                    };
                    if !group.nets.contains(&n) {
                        group.nets.push(n);
                    }
                }
                match group.contacts.iter_mut().find(|(m, _)| *m == index) {
                    Some((_, count)) => *count += 1,
                    None => group.contacts.push((index, 1)),
                }
            }
        }
        let mut out: Vec<GroundGroup> = groups.into_values().collect();
        for g in &mut out {
            g.nets
                .sort_by_key(|n| (self.model.path_of(n.board), n.net.clone()));
        }
        out.sort_by_key(|g| {
            g.nets
                .first()
                .map(|n| (self.model.path_of(n.board), n.net.clone()))
        });
        self.model.ground_groups = out;
    }
}
