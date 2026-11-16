//! `.mcu.USART1 ~ .bridge.uart`: a peripheral that is an end of a bus, joined as a port (ADR
//! 0129).
//!
//! A processor's bus end is a peripheral (`peripheral USART1 as Uart { USART1_TX:output as tx
//! USART1_RX:input as rx }`, ADR 0064), not an interface, because which pin carries each line is
//! the board's choice (`.mcu.pa9.function &= USART1_TX`). `~` between such a peripheral and an
//! interface of its bus, or another such peripheral, joins the bus as `~` joins two interfaces of
//! one kind: line by line, with the crossings of the bus (a `Uart`'s `tx` to the other end's
//! `rx`). The line of the peripheral is the pin on which the board chose its function.
//!
//! The function choices are statements like any other and may come after the join, so the pins
//! are joined after every statement is read and the pin requests have their pins (ADR 0101),
//! before the nets are made: statement order never matters, and the nets are those the pin-by-pin
//! joins made. Rules, each an error at the join:
//!
//! - The other side is an interface of the peripheral's bus, or another peripheral of that bus.
//!   A pin, a part or an interface of another kind is refused. A rename map, `except`, a chain or
//!   `~>` with a peripheral is refused by the path (a peripheral is no member of its part).
//! - A line of a peripheral whose function no pin of the part has chosen is refused and names the
//!   pins that offer it, unless the role's `lines` makes the line `optional` (a master's `cs` of
//!   an `Spi`): then the line joins nothing, and the select that goes elsewhere stays the board's.
//! - A line that only the interface has (the `rts` and `cts` of a `Uart` whose peripheral lists
//!   `tx` and `rx`) joins nothing, as a line that a part's table leaves out.
//! - A peripheral has no values: nothing is equated, so the interface's `.baud` or `.frequency`
//!   is still what the parts and the board state (`equate_bus_values` runs on the nets).
//!
//! Choices made by the agent while the owner was away (2026-11-13), with the alternatives:
//!
//! - The spelling is the part's path and the peripheral's name, as a pin request names a function
//!   (`.u.UARTE1_TX`, ADR 0101). A keyword (`.mcu peripheral USART1 ~ ...`), or an interface
//!   member that the part declares for each peripheral, were not taken: the first is a second way
//!   to name a member, the second makes a part state the same bus twice.
//! - The join does not choose the functions. A function on several pins (`USART1_TX` on `PA9` or
//!   `PB6`) has no answer, and ADR 0101 makes the source or the pin file say which pin.

use netsmith_lang::Span;
use netsmith_lang::ast::Path;

use crate::checker::{Checker, Res};
use crate::diag::SourceId;
use crate::model::{InstId, InstKind, Member, Requirement};

/// One side of a join of a bus: an interface, or a peripheral of a part (its index among the
/// part's own).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BusSide {
    Iface(InstId),
    Periph(InstId, usize),
    /// A request for the end of a bus (`.dbg: .u.Uart`, #788), by its index in
    /// `Model::bus_requests`: the peripheral it gets once the pins are assigned.
    Request(usize),
}

/// A `~` with a peripheral on one side, kept until the functions are known.
#[derive(Debug, Clone)]
pub(crate) struct PeripheralJoin {
    a: BusSide,
    b: BusSide,
    /// The module or board whose body states it: the pins are named from there.
    scope: InstId,
    source: SourceId,
    span: Span,
    text: String,
}

/// The bus that one side of a join is an end of.
struct SideBus {
    def: (usize, usize),
    interface: String,
    /// How a message names the side: `the peripheral `UARTE0``, `` `.dbg` ``.
    what: String,
}

/// What a path of a join is, as far as peripherals go.
enum PeriphPath {
    /// Not a peripheral: the join goes on as before.
    No,
    Yes(InstId, usize),
    /// A request for the end of a bus.
    Request(usize),
    /// Already reported.
    Bad,
}

impl Checker<'_> {
    /// `~` between two paths of which one or both name a peripheral that is an end of a bus.
    /// False, with nothing reported, when neither does.
    pub(crate) fn join_peripheral(&mut self, left: &Path, right: &Path, span: Span) -> bool {
        let l = self.peripheral_path(left);
        if matches!(l, PeriphPath::Bad) {
            return true;
        }
        let r = self.peripheral_path(right);
        let side = |p: &PeriphPath| match p {
            PeriphPath::Yes(q, j) => Some(BusSide::Periph(*q, *j)),
            PeriphPath::Request(i) => Some(BusSide::Request(*i)),
            _ => None,
        };
        let (a, b) = match (&l, &r) {
            (PeriphPath::No, PeriphPath::No) => return false,
            (_, PeriphPath::Bad) => return true,
            (PeriphPath::No, _) => {
                let theirs = side(&r).expect("a peripheral or a request");
                let Some(end) = self.side_bus(theirs) else {
                    return true;
                };
                let Some(other) = self.bus_side(left, end) else {
                    return true;
                };
                (other, theirs)
            }
            (_, PeriphPath::No) => {
                let ours = side(&l).expect("a peripheral or a request");
                let Some(end) = self.side_bus(ours) else {
                    return true;
                };
                let Some(other) = self.bus_side(right, end) else {
                    return true;
                };
                (ours, other)
            }
            _ => {
                let (a, b) = (
                    side(&l).expect("a peripheral or a request"),
                    side(&r).expect("a peripheral or a request"),
                );
                if a == b {
                    self.error(
                        span,
                        format!(
                            "`{}` is joined to itself; `~` joins a peripheral to another end of its bus",
                            left.to_source()
                        ),
                    );
                    return true;
                }
                let (Some(ea), Some(eb)) = (self.side_bus(a), self.side_bus(b)) else {
                    return true;
                };
                if ea.def != eb.def {
                    self.error(
                        span,
                        format!(
                            "cannot connect `{}`, an end of a `{}`, to `{}`, an end of a `{}`",
                            left.to_source(),
                            ea.interface,
                            right.to_source(),
                            eb.interface
                        ),
                    );
                    return true;
                }
                (a, b)
            }
        };
        let scope = self.current();
        let source = self.cur_source();
        let text = format!("{} ~ {}", left.to_source(), right.to_source());
        self.periph_joins.push(PeripheralJoin {
            a,
            b,
            scope,
            source,
            span,
            text,
        });
        true
    }

    /// Whether a path names a peripheral of a part that is an end of a bus: its last name is a
    /// peripheral of the part the rest of the path names, and no member of it.
    fn peripheral_path(&mut self, p: &Path) -> PeriphPath {
        if let Some(index) = self.bus_request_path(p) {
            return PeriphPath::Request(index);
        }
        let Some((last, rest)) = p.segments.split_last() else {
            return PeriphPath::No;
        };
        let Some(before) = rest.last() else {
            return PeriphPath::No;
        };
        let prefix = Path {
            leading_dot: p.leading_dot,
            segments: rest.to_vec(),
            span: Span::new(p.span.start, before.span.end),
        };
        self.joining = true;
        let res = self.resolve_path(&prefix);
        self.joining = false;
        let part = match res {
            Res::Inst(i) => i,
            Res::Field(_) => return PeriphPath::No,
            Res::Poison | Res::Bad => return PeriphPath::Bad,
        };
        if self.model.instance(part).kind != InstKind::Part
            || self.model.member(part, &last.name).is_some()
        {
            return PeriphPath::No;
        }
        let Some(index) = self
            .model
            .instance(part)
            .rules
            .peripherals
            .iter()
            .position(|q| q.name == last.name)
        else {
            return PeriphPath::No;
        };
        if self.model.instance(part).rules.peripherals[index]
            .bus
            .is_none()
        {
            let ty = self.model.instance(part).type_name.clone();
            self.error(
                last.span,
                format!(
                    "`{}` is a peripheral of `{ty}` that names no bus, so `~` cannot join it as one; the part says which bus it is an end of with `peripheral {} as Uart {{ ... }}` (ADR 0064), or the board joins its pins",
                    last.name, last.name
                ),
            );
            return PeriphPath::Bad;
        }
        self.touches.push(crate::unused::Touch {
            inst: part,
            scope: self.current(),
            source: self.cur_source(),
            span: p.span,
        });
        PeriphPath::Yes(part, index)
    }

    /// The bus a side of a join is an end of: a peripheral's, or the one a request for the end
    /// of a bus asks for (from the first peripheral of its part that is an end of it). `None`
    /// when the request names no part with one, which the serving of the requests reports.
    fn side_bus(&self, side: BusSide) -> Option<SideBus> {
        let (part, index) = match side {
            BusSide::Periph(p, i) => (p, i),
            BusSide::Request(r) => {
                let part = self.bus_request_part(r)?;
                let interface = self.model.bus_requests[r].interface.clone();
                (part, *self.bus_ends(part, &interface).first()?)
            }
            BusSide::Iface(_) => return None,
        };
        let what = match side {
            BusSide::Request(r) => format!("`.{}`", self.model.bus_requests[r].name),
            _ => format!(
                "the peripheral `{}`",
                self.model.instance(part).rules.peripherals[index].name
            ),
        };
        Some(SideBus {
            def: self.periph_bus_def(part, index),
            interface: self.periph_interface(part, index),
            what,
        })
    }

    /// The other side of a join with a peripheral: an interface of the peripheral's bus.
    fn bus_side(&mut self, p: &Path, end: SideBus) -> Option<BusSide> {
        self.joining = true;
        let res = self.resolve_path(p);
        self.joining = false;
        let wanted = end.interface;
        let what = match res {
            Res::Inst(i) => i,
            Res::Field(_) => {
                self.error(
                    p.span,
                    "a value cannot be connected; connect a pin or an interface",
                );
                return None;
            }
            Res::Poison | Res::Bad => return None,
        };
        let inst = self.model.instance(what);
        if inst.kind == InstKind::Interface && inst.def == Some(end.def) {
            self.touches.push(crate::unused::Touch {
                inst: what,
                scope: self.current(),
                source: self.cur_source(),
                span: p.span,
            });
            return Some(BusSide::Iface(what));
        }
        let is = match inst.kind {
            InstKind::Interface => format!("a `{}`", inst.type_name),
            InstKind::Pin => "a pin".to_string(),
            _ => format!("a `{}`, not an interface", inst.type_name),
        };
        self.error(
            p.span,
            format!(
                "`~` joins {}, an end of a `{wanted}`, to an interface of that kind or to another such peripheral; `{}` is {is}",
                end.what,
                p.to_source()
            ),
        );
        None
    }

    fn periph_bus_def(&self, part: InstId, index: usize) -> (usize, usize) {
        self.model.instance(part).rules.peripherals[index]
            .bus
            .as_ref()
            .expect("a peripheral that is joined names its bus")
            .def
    }

    fn periph_interface(&self, part: InstId, index: usize) -> String {
        self.model.instance(part).rules.peripherals[index]
            .bus
            .as_ref()
            .expect("a peripheral that is joined names its bus")
            .interface
            .clone()
    }

    /// Join the pins of every join with a peripheral, now that every function is chosen.
    pub(crate) fn join_peripheral_ports(&mut self) {
        let joins = std::mem::take(&mut self.periph_joins);
        for join in &joins {
            // A request for the end of a bus joins as the peripheral it got; one that got none
            // was reported where it was served.
            let (Some(a), Some(b)) = (self.served_side(join.a), self.served_side(join.b)) else {
                continue;
            };
            let join = PeripheralJoin {
                a,
                b,
                ..join.clone()
            };
            self.join_bus_sides(&join);
        }
    }

    /// A side of a join as it is now that the pins are assigned: a request for the end of a bus
    /// is the peripheral it got, or `None` when it got none.
    fn served_side(&self, side: BusSide) -> Option<BusSide> {
        match side {
            BusSide::Request(r) => self
                .model
                .bus_assignments
                .iter()
                .find(|a| a.request == r)
                .map(|a| BusSide::Periph(a.part, a.peripheral)),
            other => Some(other),
        }
    }

    fn join_bus_sides(&mut self, join: &PeripheralJoin) {
        let def = match (join.a, join.b) {
            (BusSide::Periph(p, i), _) | (_, BusSide::Periph(p, i)) => self.periph_bus_def(p, i),
            _ => unreachable!("a peripheral join has a peripheral once its requests are served"),
        };
        // A bus whose `lines` had a mistake was reported where it was read.
        let Some(bus) = self.model.bus_defs.get(&def).cloned() else {
            return;
        };
        let a = self.side_lines(join.a, join, &bus);
        let b = self.side_lines(join.b, join, &bus);
        for (line, pin) in &a {
            let there = bus.partner(line).unwrap_or(line);
            if let (Some(x), Some((_, Some(y)))) = (pin, b.iter().find(|(l, _)| l == there)) {
                self.union(*x, *y);
            }
        }
    }

    /// The lines of one side, each with its pin: an interface's member, or the pin on which the
    /// board chose a peripheral's function (none when the line is optional and not chosen; a
    /// line that must be chosen and is not is reported).
    fn side_lines(
        &mut self,
        side: BusSide,
        join: &PeripheralJoin,
        bus: &crate::model::BusDef,
    ) -> Vec<(String, Option<InstId>)> {
        match side {
            BusSide::Iface(i) => {
                let mut names: Vec<String> = bus
                    .tables
                    .iter()
                    .flat_map(|t| t.rows.iter().map(|r| r.name.clone()))
                    .collect();
                names.sort();
                names.dedup();
                names
                    .into_iter()
                    .map(|n| {
                        let pin = match self.model.member(i, &n) {
                            Some(Member::Inst(p)) => Some(p),
                            _ => None,
                        };
                        (n, pin)
                    })
                    .collect()
            }
            BusSide::Request(_) => unreachable!("a request is its peripheral by now"),
            BusSide::Periph(part, index) => {
                let periph = self.model.instance(part).rules.peripherals[index].clone();
                let role = periph.bus.as_ref().and_then(|b| b.role.clone());
                let table = bus.table(role.as_deref());
                let mut out = Vec::new();
                for f in &periph.functions {
                    let Some(line) = &f.line else { continue };
                    let pin = self.chosen_pin(part, &f.name);
                    if pin.is_none() {
                        let optional = table
                            .and_then(|t| t.rows.iter().find(|r| &r.name == line))
                            .is_some_and(|r| r.requirement == Some(Requirement::Optional));
                        if !optional {
                            self.report_unchosen(join, part, &periph.name, line, &f.name);
                        }
                    }
                    out.push((line.clone(), pin));
                }
                out
            }
        }
    }

    /// The pin of a part on which the board chose a function.
    fn chosen_pin(&self, part: InstId, function: &str) -> Option<InstId> {
        let mut pins: Vec<InstId> = self
            .model
            .instances()
            .filter(|(id, i)| {
                i.kind == InstKind::Pin
                    && self.model.owner_of(*id) == Some(part)
                    && self.model.pin_facts(*id).is_some()
            })
            .map(|(id, _)| id)
            .filter(|&id| self.model.function_of(id).as_deref() == Some(function))
            .collect();
        // A function chosen on two pins is its own error; the first in path order is joined.
        pins.sort_by_key(|&p| self.model.path_of(p));
        pins.first().copied()
    }

    fn report_unchosen(
        &mut self,
        join: &PeripheralJoin,
        part: InstId,
        periph: &str,
        line: &str,
        function: &str,
    ) {
        // The pins that offer the function, and whether each has another function chosen.
        let mut offering: Vec<(String, bool)> = self
            .model
            .instances()
            .filter(|(id, i)| i.kind == InstKind::Pin && self.model.owner_of(*id) == Some(part))
            .filter_map(|(id, _)| {
                let row = self.model.pin_facts(id)?;
                self.model.offers(part, row, function).then(|| {
                    (
                        self.model.path_below(id, join.scope),
                        self.model.function_of(id).is_some(),
                    )
                })
            })
            .collect();
        offering.sort();
        let free: Vec<&String> = offering
            .iter()
            .filter(|(_, t)| !t)
            .map(|(p, _)| p)
            .collect();
        let listed = |pins: &[&String]| {
            let shown: Vec<String> = pins.iter().take(4).map(|p| format!("`{p}`")).collect();
            match pins.len() - shown.len() {
                0 => crate::lower::list_of(&shown),
                more => format!("{} and {more} more", shown.join(", ")),
            }
        };
        let part_path = self.model.path_below(part, join.scope);
        let advice = match (offering.as_slice(), free.as_slice()) {
            ([], _) => String::new(),
            ([(one, _)], _) => {
                format!(": choose it on the pin that carries it, `.{one}.function &= {function}`")
            }
            (_, [first, ..]) => format!(
                ": choose it on a pin that offers it and has no function chosen, {} (`.{first}.function &= {function}`)",
                listed(&free)
            ),
            (all, []) => {
                let all: Vec<&String> = all.iter().map(|(p, _)| p).collect();
                format!(
                    ": every pin that offers it has another function chosen, {}",
                    listed(&all)
                )
            }
        };
        self.report(
            join.source,
            join.span,
            format!(
                "`{}` joins the line `{line}` of `{periph}`, but no pin of `{part_path}` is `{function}`{advice}",
                join.text
            ),
        );
    }
}
