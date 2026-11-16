//! `.dbg: .u.Uart`: a request for the end of a bus of a part (#788).
//!
//! A pin request asks a part for a pin by a function (`.tx: .u.UARTE1_TX`, ADR 0101). A request
//! whose last name is the bus interface of a peripheral of the part (`peripheral UARTE0 on GPIO
//! as Uart { ... }`, ADR 0064) asks for the end of that bus instead: the tool picks one of the
//! part's peripherals that are ends of it and a pin for each of its lines, keeping every line on
//! that one peripheral and every `shared` limit of the part (the serving is in `assign.rs`). The
//! request is no pin. It stands for the peripheral it got, and joins as a peripheral does (ADR
//! 0129): `.dbg ~ .console.uart` joins line by line, with the crossings of the bus.
//!
//! Choices made by the agent while the owner was away (2026-11-15), with the alternatives:
//!
//! - Which of the two a request is, is decided by the part: a request is for the end of a bus
//!   when a peripheral of the part it names states `as` that name. A part that states no such
//!   peripheral makes the request one for a function, which no pin offers, and the message says
//!   that the part has no end of that bus. No new spelling was taken (`.dbg: .u.bus Uart`): the
//!   type rule of ADR 0101 gets a second case, as `docs/research/pin-assignment-options.md`
//!   (1.2, I2) has it.
//! - The request is a path in a join and nowhere else, as a peripheral is: `.dbg.tx` is refused.
//!   An interface member (`.dbg` as a `Uart` whose pins are the chosen ones) would let a board
//!   write `.dbg.tx`, but would state the bus a second time and give the request values (`baud`)
//!   that a peripheral has not.
//! - The request gets a pin for every line the peripheral lists, `optional` ones too (a
//!   master's `miso` and `cs` of an `Spi`), as the issue says ("a pin for each of its lines"). A
//!   board whose select reaches the slave another way chooses the functions by hand and joins the
//!   peripheral (ADR 0129), where an unchosen `optional` line joins nothing.

use netsmith_lang::Span;
use netsmith_lang::ast::{Decl, DefKind, Item, Path};

use crate::checker::{Binding, Checker};
use crate::model::{BusRequest, InstId, InstKind, Member};

impl Checker<'_> {
    /// Whether the request `d`, declared in `block` among `items`, asks for the end of a bus:
    /// the part it names has a peripheral that is an end of a bus of that name. The part may be
    /// declared after the request (statement order never matters), so a part that is not a
    /// member yet is read from its declaration among `items`.
    pub(crate) fn asks_for_bus_end(&mut self, block: InstId, d: &Decl, items: &[Item]) -> bool {
        let Some((wanted, target)) = d.ty.segments.split_last() else {
            return false;
        };
        let mut inst = block;
        let mut resolved = true;
        for seg in target {
            match self.model.member(inst, &seg.name) {
                Some(Member::Inst(child)) => inst = child,
                _ => {
                    resolved = false;
                    break;
                }
            }
        }
        if resolved {
            let i = self.model.instance(inst);
            return i.kind == InstKind::Part
                && i.rules
                    .peripherals
                    .iter()
                    .any(|p| p.bus.as_ref().is_some_and(|b| b.interface == wanted.name));
        }
        // A part declared later in the block: read its definition.
        let [only] = target else { return false };
        let Some(decl) = items.iter().find_map(|item| match item {
            Item::Decl(e)
                if e.name.segments.len() == 1
                    && e.name.segments[0].name == only.name
                    && !e.ty.of_member
                    && e.ty.segments.len() == 1 =>
            {
                Some(e)
            }
            _ => None,
        }) else {
            return false;
        };
        let ty = &decl.ty.segments[0].name;
        let def = match self.modules[self.cur_module].scope.get(ty).copied() {
            Some(Binding::Def(m, i)) => Some((m, i)),
            Some(_) => None,
            None => self.prelude_def(ty),
        };
        let Some((m, i)) = def else { return false };
        let def = &self.modules[m].defs[i];
        def.kind == DefKind::Part
            && def.items.iter().any(|item| {
                matches!(item, Item::Peripheral(p) if p.bus.as_ref().is_some_and(|b| {
                    b.interface.segments.last().is_some_and(|s| s.name == wanted.name)
                }))
            })
    }

    /// Record the request for the end of a bus that `d` declares in `block`.
    pub(crate) fn record_bus_request(&mut self, block: InstId, d: &Decl) {
        let (wanted, target) =
            d.ty.segments
                .split_last()
                .expect("a request has a part and a capability");
        let first = target.first().expect("a request has a part").span;
        let last = target.last().expect("a request has a part").span;
        let source = self.cur_source();
        self.model.bus_requests.push(BusRequest {
            block,
            name: d.name.segments[0].name.clone(),
            target: target.iter().map(|s| s.name.clone()).collect(),
            interface: wanted.name.clone(),
            source,
            span: d.span,
            target_span: Span::new(first.start, last.end),
            capability_span: wanted.span,
        });
    }

    /// The request for the end of a bus that `owner` declares as `name`, if there is one.
    pub(crate) fn bus_request_at(&self, owner: InstId, name: &str) -> Option<usize> {
        self.model
            .bus_requests
            .iter()
            .position(|r| r.block == owner && r.name == name)
    }

    /// The request for the end of a bus that a path of a join names (`.dbg`, `.hub.dbg`), if it
    /// names one. Reports nothing.
    pub(crate) fn bus_request_path(&self, p: &Path) -> Option<usize> {
        if !p.leading_dot {
            return None;
        }
        let (last, rest) = p.segments.split_last()?;
        let mut inst = self.current();
        for seg in rest {
            match self.model.member(inst, &seg.name) {
                Some(Member::Inst(child)) => inst = child,
                _ => return None,
            }
        }
        self.bus_request_at(inst, &last.name)
    }

    /// The message for a request for the end of a bus that a path uses as something else.
    pub(crate) fn bus_request_misused(&self, index: usize) -> String {
        let r = &self.model.bus_requests[index];
        format!(
            "`.{}` asks `{}` for the end of a `{}`, which is joined alone with `~` to an interface of that bus (`.{} ~ .other.bus`, as a peripheral is, ADR 0129); it has no members, and the pin file says which pins it got",
            r.name,
            r.target.join("."),
            r.interface,
            r.name
        )
    }

    /// The part a request for the end of a bus names, when it is one, else `None` with nothing
    /// reported (the serving reports it).
    pub(crate) fn bus_request_part(&self, index: usize) -> Option<InstId> {
        let r = &self.model.bus_requests[index];
        let mut inst = r.block;
        for name in &r.target {
            match self.model.member(inst, name) {
                Some(Member::Inst(child)) => inst = child,
                _ => return None,
            }
        }
        (self.model.instance(inst).kind == InstKind::Part).then_some(inst)
    }

    /// The peripherals of `part` that are ends of the bus `interface`, by index, in the byte
    /// order of their names.
    pub(crate) fn bus_ends(&self, part: InstId, interface: &str) -> Vec<usize> {
        let peripherals = &self.model.instance(part).rules.peripherals;
        let mut out: Vec<usize> = (0..peripherals.len())
            .filter(|&i| {
                peripherals[i]
                    .bus
                    .as_ref()
                    .is_some_and(|b| b.interface == interface)
            })
            .collect();
        out.sort_by(|&a, &b| peripherals[a].name.cmp(&peripherals[b].name));
        out
    }

    /// The lines of a peripheral that a request for its bus gets a pin for, in the byte order of
    /// the lines: each function that names a line, with that line.
    pub(crate) fn bus_lines(&self, part: InstId, index: usize) -> Vec<(String, String)> {
        let periph = &self.model.instance(part).rules.peripherals[index];
        let mut out: Vec<(String, String)> = periph
            .functions
            .iter()
            .filter_map(|f| Some((f.line.clone()?, f.name.clone())))
            .collect();
        out.sort();
        out
    }
}
