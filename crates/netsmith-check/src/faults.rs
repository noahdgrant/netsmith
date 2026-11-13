//! The faults of a board's pins (ADR 0016, decisions 4 and 5).
//!
//! Most need only the board and the pin tables of its parts: a pin that must be connected
//! and is not, two pins that a part ties and the board does not, an output driving an
//! output. [`pin_faults`] reports those when the board is checked. A rule that depends on
//! the package waits for the solver if the board has not chosen the package (a catalog row
//! does): [`pin_faults_solved`] reports those, and the pad rules, which need the pads of the
//! package that was chosen.
//!
//! # The faults of decision 4, with their messages
//!
//! | Fault | Rule | Message |
//! |---|---|---|
//! | Required pin unconnected | `power` and `input` pins must have another part's pin on their net unless the row says `optional`; `output`, `bidirectional` and `passive` pins may float unless it says `required` | `` `u.en` is an input that must be connected (pad 6, `EN`), but nothing else is on its net `` |
//! | Tied pin not tied | `tie .a ~ .b` in a part means the board must put the two on one net | `` `u.sense` must be on the same net as `u.vout.pos` (`Ldo9` ties them), but nothing is connected to `u.sense` ``, or, when it is on another net, `` ... but `u.sense` is on net `VIN` with ... `` |
//! | No-connect pin connected | a `no_connect` pin must have nothing else on its net | `` `u.nc` is a no_connect pin (pad 5, `NC`) but it is on net `3V3` with ... `` |
//! | Two outputs on one net | two `output` pins on one net; pins the part joins inside are one; an `output` and a `bidirectional` pin are allowed | ``net `X` has two outputs: `a.tx`, `b.tx` `` |
//! | Output on a rail | an `output` pin on the net of a rail's `pos` or `neg` (#461); an output that is itself a rail's pin, or a rail of its own part, does not count | `` `d.out` (`Zdrv`) is an output on net `GND`, the net of rail `gnd` (`gnd.neg`): ... `` |
//! | Function not chosen | a connected pin with more than one function needs `.function &=` from the board | `` `u.pa9` has several functions (GPIO, UART1_TX, SPI1_MOSI) and is connected: choose one with `.u.pa9.function &= "GPIO"` `` |
//! | Function not offered | `.function &=` names a function the pin does not list | `` `u.pa9` cannot be `I2C1_SDA`; its functions are GPIO, UART1_TX, SPI1_MOSI `` |
//! | Function used twice | one function on two pins of one part (requests for a function that rows list on several pins each get their own pin) | `` function `UART1_TX` is chosen on both `u.pa9` and `u.pb6` `` |
//! | Pad given to two pins | two pins on one pad that the part does not join (decision 5, after the solve) | `` pad 3 is given to `sense` and `en` `` |
//!
//! "Another part's pin" means a terminal whose owning part is not this one, so a net made only
//! of pins joined inside the part counts as unconnected. A net counts as company for a `power`
//! or `input` pin if it has a pin that is not the part's own, including a rail the board
//! declares: a rail that nothing on the board drives is supplied from outside (ADR 0007). A
//! module's port the board leaves open is not seen. Messages point at the board's declaration
//! of the part (`.u: Ldo9`) and name the pinout line. An intentionally unused `optional` pin
//! needs no statement; leaving a `required` pin open is refused (if the datasheet allows it,
//! the pin is not `required`). `optional` and `required` are refused where they change nothing
//! (`optional` on an output, `required` on an input). Two power sources on one net are not
//! checked: whether a power pin gives or takes is the sign of the rail's current, which is
//! known only after the solve (a known limit).

use std::collections::{BTreeMap, HashSet};

use crate::diag::Diagnostic;
use crate::model::{InstId, InstKind, Model, Net, PinKind, PinRow, PinTable, Pinout};
use crate::pins::article;

/// The faults that the board alone settles: every part whose package is known, and every
/// net whose parts all are.
pub fn pin_faults(model: &Model) -> Vec<Diagnostic> {
    pin_faults_and_conflicts(model).0
}

/// [`pin_faults`], and those of them that are inside a use with several providers, which
/// another provider may not have (`Model::provider_conflicts`, ADR 0113).
pub(crate) fn pin_faults_and_conflicts(model: &Model) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
    let deferred = unknown_package(model);
    let bus = crate::busnets::check(model);
    let mut run = Run::new(model, &deferred, Pass::Board, bus.nets);
    run.go();
    run.out.extend(crate::buses::i2c_pullups(model));
    run.out.extend(bus.out);
    (run.out, run.conflicts)
}

/// The faults that wait for the solver: the parts whose package `before` did not know, and
/// the nets they are on, and the pad rules for every part (`solved` has the packages the
/// solver chose).
pub fn pin_faults_solved(before: &Model, solved: &Model) -> Vec<Diagnostic> {
    let deferred = unknown_package(before);
    let bus = crate::busnets::check(solved);
    let mut run = Run::new(solved, &deferred, Pass::Solved, bus.nets);
    run.go();
    run.out
}

/// The parts of a board with pin tables for packages and no package yet.
fn unknown_package(model: &Model) -> HashSet<InstId> {
    model
        .instances()
        .filter(|(id, i)| {
            i.kind == InstKind::Part
                && model.board_of(*id).is_some()
                && matches!(model.pinout(*id), Pinout::Unknown { .. })
        })
        .map(|(id, _)| id)
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Board,
    Solved,
}

struct Run<'a> {
    model: &'a Model,
    deferred: &'a HashSet<InstId>,
    pass: Pass,
    /// The nets a bus rule has reported (ADR 0064): the rules of a net skip them.
    bus_nets: std::collections::HashSet<String>,
    out: Vec<Diagnostic>,
    /// The faults among `out` that another provider of a use may not have (ADR 0113).
    conflicts: Vec<Diagnostic>,
}

/// Items that are already written for a message, joined.
fn list_plain(items: &[String]) -> String {
    items.join(", ")
}

fn list(items: &[String]) -> String {
    items
        .iter()
        .map(|i| format!("`{i}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl<'a> Run<'a> {
    fn new(
        model: &'a Model,
        deferred: &'a HashSet<InstId>,
        pass: Pass,
        bus_nets: std::collections::HashSet<String>,
    ) -> Self {
        Self {
            model,
            deferred,
            pass,
            bus_nets,
            out: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    fn report(&mut self, at: (crate::diag::SourceId, netsmith_lang::Span), message: String) {
        let d = Diagnostic {
            source: at.0,
            span: at.1,
            message,
        };
        if !self.out.contains(&d) {
            self.out.push(d);
        }
    }

    /// Whether this pass owns the rules for a part.
    fn mine(&self, part: InstId) -> bool {
        self.deferred.contains(&part) == (self.pass == Pass::Solved)
    }

    fn go(&mut self) {
        let model = self.model;
        let mut parts: Vec<(String, InstId)> = model
            .instances()
            .filter(|(id, i)| i.kind == InstKind::Part && model.board_of(*id).is_some())
            .map(|(id, _)| (model.path_of(id), id))
            .collect();
        parts.sort();
        for &(_, part) in &parts {
            netsmith_core::job::checkpoint();
            if self.mine(part) {
                self.part(part);
            }
            if self.pass == Pass::Solved {
                self.pads(part);
            }
            if self.pass == Pass::Board {
                self.shared(part);
            }
        }
        self.nets(&parts);
        if self.pass == Pass::Board {
            self.stray_functions();
        }
    }

    /// A function stated on a pin that no part reads has no effect (ADR 0071, stage 0): a
    /// module's own pin whose choice nothing passes on, or a pin of a part that its table does
    /// not describe. It is an error, because a statement that is accepted and does nothing
    /// sends the reader to the wrong place.
    fn stray_functions(&mut self) {
        let model = self.model;
        let mut pins: Vec<(String, InstId)> = model
            .instances()
            .filter(|(id, i)| i.kind == InstKind::Pin && model.board_of(*id).is_some())
            .map(|(id, _)| (model.path_of(id), id))
            .collect();
        pins.sort();
        for (path, pin) in pins {
            let Some(crate::model::Member::Field(field)) = model.member(pin, "function") else {
                continue;
            };
            let Some(&at) = model.field(field).narrowed_at.last() else {
                continue;
            };
            if model.function_is_read(pin) {
                continue;
            }
            let Some(owner) = model.owner_of(pin) else {
                continue;
            };
            let inst = model.instance(owner);
            let name = format!("`{path}`");
            let message = if inst.kind == InstKind::Part {
                format!(
                    "`{path}.function` is stated, but {name} has no row in the pin table of `{}`, so it has no effect; the function of a pin is chosen on a pin of the table",
                    inst.type_name
                )
            } else {
                let ty = &inst.type_name;
                let own = model.path_below(pin, owner);
                let hint = self.forwarding_hint(owner, pin, &own);
                format!(
                    "`{path}.function` is stated, but {name} is a pin of {} `{ty}` and nothing passes it to a pin of a part, so it has no effect; inside `{ty}`, write {hint}",
                    inst.kind.word()
                )
            };
            self.report(at, message);
        }
    }

    /// The equation a module writes to pass the function of its own pin to a pin of a part:
    /// with the part's pin named when exactly one pin with functions is on the same net inside
    /// the module, else with its place left to fill.
    fn forwarding_hint(&self, module: InstId, pin: InstId, own: &str) -> String {
        let model = self.model;
        let candidates: Vec<InstId> = model
            .net_of(pin)
            .map(|n| {
                n.pins
                    .iter()
                    .copied()
                    .filter(|&p| {
                        p != pin
                            && model.owner_of(p).is_some_and(|o| {
                                model.is_inside(o, module)
                                    && o != module
                                    && model
                                        .pin_facts(p)
                                        .is_some_and(|r| model.offered_count(o, r) > 0)
                            })
                    })
                    .collect()
            })
            .unwrap_or_default();
        match candidates.as_slice() {
            [one] => format!(
                "`.{}.function == .{own}.function`",
                model.path_below(*one, module)
            ),
            _ => format!("`.<part>.<pin>.function == .{own}.function`"),
        }
    }

    /// What a part's `shared` resources allow (ADR 0071). It needs no package, so it is judged
    /// in the board pass for every part, deferred or not.
    fn shared(&mut self, part: InstId) {
        for (at, message) in crate::shared::overuse(self.model, part) {
            self.report(at, message);
        }
    }

    /// The pin tables say how a part is in the package it is in, if any does.
    fn part(&mut self, part: InstId) {
        let model = self.model;
        match model.pinout(part) {
            Pinout::NoTables | Pinout::Unknown { .. } => {}
            Pinout::NoSuchPackage { package, packages } => {
                let ty = &model.instance(part).type_name;
                let packages: Vec<String> = packages.iter().map(|p| p.to_string()).collect();
                let at = model.instance(part).at();
                let message = format!(
                    "`{}` is a `{ty}` in package `{package}`, which has no pin table; its tables are for {}",
                    model.path_of(part),
                    list(&packages)
                );
                // Another provider of the use may have a table for it (ADR 0113).
                if model.in_varied_use(part) {
                    self.conflicts.push(Diagnostic {
                        source: at.0,
                        span: at.1,
                        message: message.clone(),
                    });
                }
                self.report(at, message);
            }
            Pinout::Table(table) => {
                self.pins(part, table);
                self.ties(part, table);
                self.functions_used_twice(part, table);
            }
        }
    }

    fn pins(&mut self, part: InstId, table: &PinTable) {
        let model = self.model;
        let inst = model.instance(part);
        let path = model.path_of(part);
        let mut all: Vec<InstId> = inst
            .tables
            .iter()
            .flat_map(|t| t.rows.iter().map(|r| r.pin))
            .collect();
        all.sort_by_key(|&p| model.path_of(p));
        all.dedup();

        for pin in all {
            let name = model.path_of(pin);
            let Some(row) = table.row(pin) else {
                // A pin the package does not have, which the board uses.
                if model.is_connected(pin) {
                    let facts = model.pin_facts(pin).expect("it is in some table");
                    let package = table.package.clone().unwrap_or_default();
                    let have: Vec<String> = model
                        .packages_with(pin)
                        .iter()
                        .map(|p| p.to_string())
                        .collect();
                    self.report(
                        inst.at(),
                        format!(
                            "`{name}` (`{}`) does not exist in package `{package}` of `{}`, but the board connects it; the packages that have it: {}",
                            facts.name,
                            inst.type_name,
                            list(&have)
                        ),
                    );
                }
                continue;
            };
            let pads = pads_text(row);

            if row.kind == PinKind::NoConnect {
                let others = model.others_on_net(pin);
                if !others.is_empty() {
                    let net = model.net_of(pin).expect("it has others");
                    let mut names: Vec<String> = others.iter().map(|&p| model.path_of(p)).collect();
                    names.sort();
                    self.report(
                        inst.at(),
                        format!(
                            "`{name}` is a no_connect pin ({pads}, `{}`) but it is on net `{}` with {}",
                            row.name,
                            net.name,
                            list(&names)
                        ),
                    );
                }
            }

            let (must, by_role) = model.must_be_connected_by(pin, row);
            if must && !self.has_company(part, pin) && model.unused_by(pin).is_none() {
                let because = by_role.unwrap_or_default();
                self.report(
                    inst.at(),
                    format!(
                        "`{name}` is {} that must be connected ({pads}, `{}`){}{because} nothing else is on its net",
                        article(row.kind),
                        row.name,
                        if because.is_empty() { ", but" } else { ": " },
                    ),
                );
            }

            self.function(part, pin, row, &path);
        }
    }

    /// Whether a pin shares its net with a pin that is not the part's own. A rail or a
    /// module's port counts: it is where a board brings the connection in.
    fn has_company(&self, part: InstId, pin: InstId) -> bool {
        let model = self.model;
        model
            .net_of(pin)
            .is_some_and(|net| net.pins.iter().any(|&p| model.owner_of(p) != Some(part)))
    }

    fn function(&mut self, part: InstId, pin: InstId, row: &PinRow, _path: &str) {
        let model = self.model;
        let name = model.path_of(pin);
        let chosen = model.function_of(pin);
        let functions: Vec<String> = row.function_names();
        let classed = model.class_peripherals(part, row);
        match chosen {
            Some(f) if !model.offers(part, row, &f) => {
                let at = self.function_site(pin, part);
                let message = if model.instance(part).rules.peripherals.is_empty() {
                    if functions.is_empty() {
                        format!("`{name}` cannot be `{f}`; it has no functions to choose from")
                    } else {
                        format!(
                            "`{name}` cannot be `{f}`; its functions are {}",
                            functions.join(", ")
                        )
                    }
                } else {
                    self.not_offered(part, pin, row, &f)
                };
                self.report(at, message);
            }
            Some(_) => {}
            None if model.offered_count(part, row) > 1 && model.is_connected(pin) => {
                // A pin that a module forwards is chosen where the board can write it: on the
                // module's own pin, by the path the board knows.
                let chain = model.function_chain(pin);
                let outer = *chain.last().expect("a pin");
                let (at, scope, pin) = match model.owner_of(outer).filter(|_| outer != pin) {
                    Some(module) => (
                        model.instance(module).at(),
                        model.instance(module).parent.unwrap_or(model.root),
                        outer,
                    ),
                    None => (
                        model.instance(part).at(),
                        model.instance(part).parent.unwrap_or(model.root),
                        pin,
                    ),
                };
                let relative = model.path_below(pin, scope);
                let shown = if classed.is_empty() {
                    functions.join(", ")
                } else {
                    format!(
                        "{} and the lines of the peripherals on those pins",
                        functions.join(", ")
                    )
                };
                self.report(
                    at,
                    format!(
                        "`{name}` has several functions ({shown}) and is connected: choose one with `.{relative}.function &= \"{}\"`",
                        functions[0]
                    ),
                );
            }
            None => {}
        }
    }

    /// The message for a function that a pin does not offer, in a part that has peripherals:
    /// where the function is offered when a peripheral lists it, the nearest function when no
    /// pin does, and what this pin offers. A pin whose peripherals are on a class has too many
    /// functions to list, so it names the peripherals.
    fn not_offered(&self, part: InstId, pin: InstId, row: &PinRow, function: &str) -> String {
        let model = self.model;
        let name = model.path_of(pin);
        let functions = row.function_names();
        let classed = model.class_peripherals(part, row);
        let mut message = format!("`{name}` cannot be `{function}`");
        if let Some((peripheral, _)) = model.peripheral_of(part, function) {
            let mut pins: Vec<String> = model
                .instance(part)
                .tables
                .iter()
                .flat_map(|t| t.rows.iter())
                .filter(|r| r.functions.iter().any(|f| f.name == function))
                .map(|r| format!("`{}`", model.path_of(r.pin)))
                .collect();
            pins.sort();
            pins.dedup();
            let on = pins.join(", ");
            message.push_str(&match (&peripheral.class, pins.is_empty()) {
                (Some(class), true) => {
                    format!("; that function is offered on every pin that lists `{class}`")
                }
                (Some(class), false) => format!(
                    "; that function is offered on every pin that lists `{class}` and on {on}"
                ),
                (None, _) => format!("; that function is offered only on {on}"),
            });
            message.push_str("; ");
        } else {
            let mut known: Vec<String> = model
                .instance(part)
                .tables
                .iter()
                .flat_map(|t| t.rows.iter())
                .flat_map(|r| r.functions.iter().map(|f| f.name.clone()))
                .chain(
                    model
                        .instance(part)
                        .rules
                        .peripherals
                        .iter()
                        .flat_map(|p| p.functions.iter().map(|f| f.name.clone())),
                )
                .collect();
            known.sort();
            known.dedup();
            match crate::lower::closest(function, &known) {
                Some(near) => message.push_str(&format!("; did you mean `{near}`? ")),
                None => message.push_str("; "),
            }
        }
        if classed.is_empty() {
            if functions.is_empty() {
                message.push_str("it has no functions to choose from");
            } else {
                message.push_str(&format!("its functions are {}", functions.join(", ")));
            }
        } else {
            let lines: Vec<String> = classed.iter().map(|p| p.name.clone()).collect();
            message.push_str(&format!(
                "`{name}` offers {} and the lines of the peripherals on those pins: {}",
                functions.join(", "),
                lines.join(", ")
            ));
        }
        message
    }

    /// Where the board chose a function: the statement, on the pin or on the pin of a module
    /// that forwards to it, or else the part.
    fn function_site(
        &self,
        pin: InstId,
        part: InstId,
    ) -> (crate::diag::SourceId, netsmith_lang::Span) {
        let model = self.model;
        model
            .function_chain(pin)
            .into_iter()
            .find_map(|p| match model.member(p, "function") {
                Some(crate::model::Member::Field(f)) => model.field(f).narrowed_at.last().copied(),
                _ => None,
            })
            .unwrap_or_else(|| model.instance(part).at())
    }

    fn functions_used_twice(&mut self, part: InstId, table: &PinTable) {
        let model = self.model;
        let mut chosen: BTreeMap<String, Vec<InstId>> = BTreeMap::new();
        for row in &table.rows {
            if let Some(f) = model.function_of(row.pin) {
                chosen.entry(f).or_default().push(row.pin);
            }
        }
        for (function, mut pins) in chosen {
            // A class names what a pin can be, not one use of it: `GPIO` on two pins is two
            // pins that are plain GPIOs (ADR 0101). A peripheral that is `on` a function and
            // a row's `class` marker both make one.
            if model.is_class(part, &function) {
                continue;
            }
            // A function that rows list on several pins (`ANALOG`) may be asked for by several
            // requests, and each gets a pin of its own: a request's pin is not counted. A line
            // of a peripheral (`UARTE1_TX`) is on one pin at a time, so it stays counted. Two
            // pins the source chose by hand are counted either way.
            if model.peripheral_of(part, &function).is_none() {
                pins.retain(|&p| !model.pin_assignments.iter().any(|a| a.pin == p));
            }
            if pins.len() < 2 {
                continue;
            }
            pins.sort_by_key(|&p| model.path_of(p));
            let names: Vec<String> = pins.iter().map(|&p| model.path_of(p)).collect();
            let at = self.function_site(pins[1], part);
            // Pins that requests got say so: a request cannot know that its function is taken.
            let asked: Vec<String> = pins
                .iter()
                .filter_map(|&p| model.pin_assignments.iter().find(|a| a.pin == p))
                .map(|a| format!("`{}`", a.key))
                .collect();
            let by = if asked.len() == pins.len() {
                format!(
                    "; the requests {} ask for a line of a peripheral, which one pin has",
                    asked.join(" and ")
                )
            } else {
                String::new()
            };
            self.report(
                at,
                format!(
                    "function `{function}` is chosen on both {}{by}",
                    names
                        .iter()
                        .map(|n| format!("`{n}`"))
                        .collect::<Vec<_>>()
                        .join(" and ")
                ),
            );
        }
    }

    fn ties(&mut self, part: InstId, table: &PinTable) {
        let model = self.model;
        let inst = model.instance(part);
        for tie in &inst.ties {
            // A pin that the package does not have is not tied there.
            let missing = |p: InstId| table.row(p).is_none() && model.pin_facts(p).is_some();
            if missing(tie.a) || missing(tie.b) {
                continue;
            }
            let (na, nb) = (model.net_of(tie.a), model.net_of(tie.b));
            let joined = matches!((na, nb), (Some(x), Some(y)) if std::ptr::eq(x, y));
            if joined {
                continue;
            }
            let (a, b) = (model.path_of(tie.a), model.path_of(tie.b));
            let where_a = match na {
                None => format!("nothing is connected to `{a}`"),
                Some(n) => {
                    let mut others: Vec<String> = n
                        .pins
                        .iter()
                        .filter(|&&p| p != tie.a)
                        .map(|&p| model.path_of(p))
                        .collect();
                    others.sort();
                    format!("`{a}` is on net `{}` with {}", n.name, list(&others))
                }
            };
            self.report(
                (tie.source, tie.span),
                format!(
                    "`{a}` must be on the same net as `{b}` (`{}` ties them), but {where_a}",
                    inst.type_name
                ),
            );
        }
    }

    /// The rules of a whole net: two outputs on it, and inputs with nothing to drive them.
    fn nets(&mut self, parts: &[(String, InstId)]) {
        let model = self.model;
        for net in model.nets() {
            netsmith_core::job::checkpoint();
            let mut mine: Vec<InstId> = Vec::new();
            let mut any_mine = false;
            for &t in &net.terminals {
                let Some(part) = model.owner_of(t) else {
                    continue;
                };
                if !parts.iter().any(|(_, p)| *p == part) {
                    continue;
                }
                any_mine |= self.deferred.contains(&part);
                mine.push(t);
            }
            // A net with a part still waiting for its package is judged after the solve.
            if any_mine != (self.pass == Pass::Solved) {
                continue;
            }
            // A net that a bus rule has reported is said once (ADR 0064).
            if self.bus_nets.contains(&net.name) {
                continue;
            }
            self.outputs(net, &mine);
            self.open_drains(net, &mine);
            self.outputs_on_rails(net, &mine);
            self.floating_inputs(net);
        }
    }

    /// Two drivers on one net: pins whose kind is `output` or `tri_state`. Pins joined inside
    /// their part are one driver. Drivers that are all `tri_state` share the line (at most
    /// one is selected at a time, which firmware and the select line carry); two `output` pins
    /// are two outputs, and an `output` among `tri_state` pins drives while the others are
    /// selected (ADR 0064). A `bidirectional` pin is not counted, as before.
    fn outputs(&mut self, net: &Net, terminals: &[InstId]) {
        let model = self.model;
        let drivers: Vec<(InstId, PinKind)> = terminals
            .iter()
            .filter_map(|&t| match self.kind_here(t) {
                Some(k @ (PinKind::Output | PinKind::TriState)) => Some((t, k)),
                _ => None,
            })
            .collect();
        let mut distinct: Vec<(InstId, PinKind)> = Vec::new();
        for (o, kind) in drivers {
            let group = model.joined_inside(o);
            if !distinct.iter().any(|(d, _)| group.contains(d)) {
                distinct.push((o, kind));
            }
        }
        if distinct.len() < 2 {
            return;
        }
        let plain: Vec<InstId> = distinct
            .iter()
            .filter(|(_, k)| *k == PinKind::Output)
            .map(|&(p, _)| p)
            .collect();
        let at = model
            .instance(model.owner_of(distinct[0].0).expect("a terminal"))
            .at();
        if plain.is_empty() {
            return;
        }
        if plain.len() == distinct.len() {
            let names: Vec<String> = distinct.iter().map(|&(p, _)| self.describe(p)).collect();
            let mut message = format!("net `{}` has two outputs: {}", net.name, list_plain(&names));
            if model.net_has_stated_role(net) {
                message.push_str(
                    "; a line that several parts share needs every driver to be `tri_state`",
                );
            }
            self.report(at, message);
            return;
        }
        // An `output` among `tri_state` pins: the output drives while the others are selected.
        let says = |p: InstId, kind: PinKind| {
            let ty = model
                .owner_of(p)
                .map_or(String::new(), |o| model.instance(o).type_name.clone());
            let what = if kind == PinKind::TriState {
                "a `tri_state` output"
            } else {
                "an output"
            };
            format!("`{}` (`{ty}`, {what})", model.path_of(p))
        };
        let all: Vec<String> = distinct.iter().map(|&(p, k)| says(p, k)).collect();
        let plain_names: Vec<String> = plain
            .iter()
            .map(|&p| format!("`{}`", model.path_of(p)))
            .collect();
        let mut types: Vec<String> = plain
            .iter()
            .filter_map(|&p| model.owner_of(p))
            .map(|o| format!("`{}`", model.instance(o).type_name))
            .collect();
        types.sort();
        types.dedup();
        let (be, copula) = if plain.len() == 1 {
            ("is", "it")
        } else {
            ("are", "they")
        };
        self.report(
            at,
            format!(
                "net `{}` has {} drivers and {} {be} not `tri_state`: {}; {copula} would drive the net while another driver on it is selected; the datasheet of {} must say that the pin is high impedance while its part is not selected (then write `tri_state` for it, with the page), or give {} a net of its own",
                net.name,
                distinct.len(),
                plain_names.join(", "),
                all.join(" and "),
                types.join(", "),
                plain_names.join(", "),
            ),
        );
    }

    /// The rules of a net with `open_drain` pins (#460, the owner's decisions of 2026-11-12).
    /// Several of them share the line, since each only pulls it low or lets it go. An `output`
    /// or a `tri_state` output on the same net drives it high while an open-drain pin pulls it
    /// low, which is refused. And the line needs a pull-up: a resistor from the net to a net
    /// that holds the positive pin of a rail, or a pin that can drive it high (`output`,
    /// `tri_state` or `bidirectional`, such as a GPIO behind a series resistor). Pins joined
    /// inside their part are one pin.
    fn open_drains(&mut self, net: &Net, terminals: &[InstId]) {
        let model = self.model;
        let mut drains: Vec<InstId> = Vec::new();
        let mut pushers: Vec<(InstId, PinKind)> = Vec::new();
        for &t in terminals {
            let kind = self.kind_here(t);
            let into: &mut Vec<InstId> = match kind {
                Some(PinKind::OpenDrain) => &mut drains,
                Some(PinKind::Output | PinKind::TriState) => {
                    let group = model.joined_inside(t);
                    if !pushers.iter().any(|(p, _)| group.contains(p)) {
                        pushers.push((t, kind.expect("matched")));
                    }
                    continue;
                }
                _ => continue,
            };
            let group = model.joined_inside(t);
            if !into.iter().any(|p| group.contains(p)) {
                into.push(t);
            }
        }
        if drains.is_empty() {
            return;
        }
        let at = model
            .instance(model.owner_of(drains[0]).expect("a terminal"))
            .at();
        let drain_names: Vec<String> = drains.iter().map(|&p| self.describe(p)).collect();
        let (drain_word, lets) = if drains.len() == 1 {
            ("an open_drain output", "it lets")
        } else {
            ("open_drain outputs", "they let")
        };
        if !pushers.is_empty() {
            let pushed: Vec<String> = pushers
                .iter()
                .map(|&(p, kind)| {
                    let ty = model
                        .owner_of(p)
                        .map_or(String::new(), |o| model.instance(o).type_name.clone());
                    format!("`{}` (`{ty}`, {})", model.path_of(p), article(kind))
                })
                .collect();
            let names: Vec<String> = pushers
                .iter()
                .map(|&(p, _)| format!("`{}`", model.path_of(p)))
                .collect();
            let (drives, them, own) = if pushers.len() == 1 {
                ("it drives", "it", "its")
            } else {
                ("they drive", "them", "their")
            };
            self.report(
                at,
                format!(
                    "net `{}` has {drain_word} ({}) and a push-pull driver: {}; {drives} the line high while an open_drain pin pulls it low; give {} a net of {own} own, or write `open_drain` for {them} if the datasheet says the pin is open drain",
                    net.name,
                    list_plain(&drain_names),
                    pushed.join(" and "),
                    names.join(", "),
                ),
            );
        }
        if !terminals.iter().any(|&t| self.pulls_up(net, t)) {
            self.report(
                at,
                format!(
                    "net `{}` has {drain_word} ({}) and no pull-up: nothing takes the line high when {lets} go of it; join a resistor from the net to a rail's `pos`, or to a pin that drives the line high",
                    net.name,
                    list_plain(&drain_names),
                ),
            );
        }
    }

    /// A push-pull `output` pin on the net of a rail, its positive or its negative pin (#461, the
    /// owner's decision of 2026-11-12): the output drives the other level against the rail, which
    /// is a short. Any other kind may be tied to a rail (a GPIO to ground is an ordinary circuit).
    /// An output that is itself a pin of a rail is a supply the part gives (a charger's or a hub's
    /// VBUS, which `unused.rs` counts as a source), and a rail of the output's own part is the
    /// part's business: neither counts.
    fn outputs_on_rails(&mut self, net: &Net, terminals: &[InstId]) {
        let model = self.model;
        for &t in terminals {
            if self.kind_here(t) != Some(PinKind::Output) {
                continue;
            }
            let own_rail = model
                .instance(t)
                .parent
                .and_then(|i| model.rail_of(i))
                .is_some_and(|(_, pos, neg)| t == pos || t == neg);
            if own_rail {
                continue;
            }
            let owner = model.owner_of(t);
            let rail = net.pins.iter().find_map(|&p| {
                let iface = model.instance(p).parent?;
                let (_, pos, neg) = model.rail_of(iface)?;
                if p != pos && p != neg {
                    return None;
                }
                if owner.is_some() && model.owner_of(iface) == owner {
                    return None;
                }
                Some((iface, p))
            });
            let Some((rail, pin)) = rail else {
                continue;
            };
            let at = model.instance(owner.expect("a terminal has a part")).at();
            self.report(
                at,
                format!(
                    "{} is an output on net `{}`, the net of rail `{}` (`{}`): a push-pull output drives the rail to its other level, which is a short; give it a net of its own, or, if its datasheet says the pin may be tied, write the kind it says (`bidirectional`, `open_drain` or `passive`)",
                    self.describe_kind(t),
                    net.name,
                    model.path_of(rail),
                    model.path_of(pin),
                ),
            );
        }
    }

    /// `` `u.out` (`Part`) ``: a pin and the type of its part, for a message.
    fn describe_kind(&self, pin: InstId) -> String {
        let model = self.model;
        let ty = model
            .owner_of(pin)
            .map_or(String::new(), |o| model.instance(o).type_name.clone());
        format!("`{}` (`{ty}`)", model.path_of(pin))
    }

    /// Whether `pin` of `net` is one end of a resistor whose other end is on a net that holds
    /// the positive pin of a rail or a pin that can drive high (`output`, `tri_state`,
    /// `bidirectional`) of another part: a pull-up of the net (one step, as the levels check
    /// reads a pull).
    fn pulls_up(&self, net: &Net, pin: InstId) -> bool {
        let model = self.model;
        let Some(resistor) = model.owner_of(pin) else {
            return false;
        };
        let Some((a, b)) = model.instance(resistor).bridge else {
            return false;
        };
        let is_resistor = matches!(
            model.member(resistor, "resistance"),
            Some(crate::model::Member::Field(r))
                if model.field(r).kind
                    == crate::model::FieldKind::Quantity(netsmith_quantity::Dimension::RESISTANCE)
        );
        let far = match pin {
            p if p == a => b,
            p if p == b => a,
            _ => return false,
        };
        if !is_resistor || net.pins.contains(&far) {
            return false;
        }
        let Some(far_net) = model.nets().iter().find(|n| n.pins.contains(&far)) else {
            return false;
        };
        far_net.pins.iter().any(|&p| {
            let rail_pos = model
                .instance(p)
                .parent
                .and_then(|i| model.rail_of(i))
                .is_some_and(|(_, pos, _)| pos == p);
            let drives = model.owner_of(p).is_some_and(|o| o != resistor)
                && matches!(
                    self.kind_here(p),
                    Some(PinKind::Output | PinKind::TriState | PinKind::Bidirectional)
                );
            rail_pos || drives
        })
    }

    /// A net whose pins are all inputs has nothing that drives it (a UART's `rx` wired to
    /// the other `rx`). An output, a bidirectional pin, a power pin, a passive pin (a
    /// pull resistor, a connector) or a rail the board declares is a driver, and so is any
    /// pin the checker knows nothing about: the net is judged only when every pin on it is
    /// known to be an input. A net that only one part's own pins are on is the business of
    /// the rule that an input must be connected.
    fn floating_inputs(&mut self, net: &Net) {
        let model = self.model;
        if net.terminals.is_empty() {
            return;
        }
        for &t in &net.terminals {
            let known_input = model
                .owner_of(t)
                .is_some_and(|part| self.kind_in(part, t) == Some(PinKind::Input));
            if !known_input {
                return;
            }
        }
        // The pins that are not a part's: a rail the board declares supplies the net, and a
        // port of the board is brought in from outside it. A port of a module is only a
        // way through.
        for &p in net.pins.iter().filter(|p| !net.terminals.contains(p)) {
            let supplied = model
                .instance(p)
                .parent
                .is_some_and(|i| model.rail_of(i).is_some());
            let board_port = model
                .owner_of(p)
                .is_some_and(|o| model.instance(o).kind == InstKind::Board);
            if supplied || board_port {
                return;
            }
        }
        let mut owners: Vec<InstId> = net
            .terminals
            .iter()
            .filter_map(|&t| model.owner_of(t))
            .collect();
        owners.sort();
        owners.dedup();
        if owners.len() < 2 && net.pins.len() == net.terminals.len() {
            return;
        }
        let mut names: Vec<String> = net.terminals.iter().map(|&p| self.describe(p)).collect();
        names.sort();
        let chosen = net
            .terminals
            .iter()
            .any(|&p| self.function_kind_note(p).is_some());
        let at = model.instance(owners[0]).at();
        self.report(
            at,
            format!(
                "net `{}` has only inputs ({}): nothing drives it, so it floats; join an output, a bidirectional pin or a rail to it, or a pull resistor{}",
                net.name,
                list_plain(&names),
                if chosen {
                    ", or choose another function for a pin whose function makes it an input"
                } else {
                    ""
                }
            ),
        );
    }

    /// A pin's kind for the rules of a net, in the pin table that applies to its part: the
    /// kind of the function the board chose if that function has one of its own (ADR 0046),
    /// else the pin's. A part with no table, or whose package has no row for the pin, has
    /// none.
    fn kind_in(&self, part: InstId, pin: InstId) -> Option<PinKind> {
        match self.model.pinout(part) {
            Pinout::Table(table) => table.row(pin).map(|r| {
                self.model
                    .kind_with(part, r, self.model.function_of(pin).as_deref())
            }),
            _ => None,
        }
    }

    /// The kind of a pin for the rules of a net: from any table when the part has no
    /// single applicable one yet (the pin's kind is the same in every package).
    fn kind_here(&self, pin: InstId) -> Option<PinKind> {
        self.model.pin_kind(pin)
    }

    /// The chosen function of a pin and its kind, when the function has a kind of its own
    /// that decides how the net sees the pin.
    fn function_kind_note(&self, pin: InstId) -> Option<(String, PinKind)> {
        let model = self.model;
        let part = model.owner_of(pin)?;
        let chosen = model.function_of(pin)?;
        let row = match model.pinout(part) {
            Pinout::Table(table) => table.row(pin),
            _ => model.pin_facts(pin),
        }?;
        let kind = model.function_kind(part, row, Some(&chosen))?;
        Some((chosen, kind))
    }

    /// How a message names a pin on a net: its path, and when its function decides its
    /// kind, the part's kind, the function and that kind.
    fn describe(&self, pin: InstId) -> String {
        let path = self.model.path_of(pin);
        match (self.function_kind_note(pin), self.model.owner_of(pin)) {
            (Some((function, kind)), Some(part)) => format!(
                "`{path}` (`{}` function `{function}`, {})",
                self.model.instance(part).type_name,
                article(kind)
            ),
            _ => format!("`{path}`"),
        }
    }

    /// A pad belongs to one pin, unless the part joins the pins that share it. This is a
    /// fault of the part's table, so it names the part's definition, not where it is used.
    fn shared_pads(&mut self, part: InstId, table: &PinTable) {
        let model = self.model;
        let ty = &model.instance(part).type_name;
        let mut by_pad: BTreeMap<&str, Vec<&PinRow>> = BTreeMap::new();
        for row in &table.rows {
            for pad in &row.pads {
                by_pad.entry(pad).or_default().push(row);
            }
        }
        let package = table
            .package
            .as_ref()
            .map(|p| format!(" (package `{p}`)"))
            .unwrap_or_default();
        for (pad, rows) in by_pad {
            for (i, a) in rows.iter().enumerate() {
                let Some(b) = rows[i + 1..]
                    .iter()
                    .find(|b| !model.joined_inside(a.pin).contains(&b.pin))
                else {
                    continue;
                };
                let shown = |r: &PinRow| model.path_below(r.pin, part);
                self.report(
                    (table.source, b.span),
                    format!(
                        "pad {pad} of `{ty}`{package} is given to both `{}` and `{}`; two pins share a pad only if the part joins them",
                        shown(a),
                        shown(b)
                    ),
                );
                break;
            }
        }
    }

    /// The pad rules (decision 5): a pad belongs to one pin, and a pin on a net has a pad
    /// of its own or an alias for one.
    fn pads(&mut self, part: InstId) {
        let model = self.model;
        let inst = model.instance(part);
        let path = model.path_of(part);
        // A part with no pin table has no pad for any pin, which the design stage says (a part that
        // is not drawn from a datasheet has none to give). The tables are for packages and
        // none was chosen, or none fits: said elsewhere.
        let Pinout::Table(table) = model.pinout(part) else {
            return;
        };

        self.shared_pads(part, table);

        // Pins that are on a net and have no pad and no alias.
        let mut padless: Vec<String> = Vec::new();
        for (id, pin) in model.instances() {
            if pin.kind != InstKind::Pin
                || !model.is_terminal(id)
                || model.owner_of(id) != Some(part)
                || model.net_of(id).is_none()
                || model.pin_facts(id).is_some()
                || model.is_alias(id)
            {
                continue;
            }
            padless.push(model.path_below(id, part));
        }
        if !padless.is_empty() {
            padless.sort();
            self.report(
                inst.at(),
                format!(
                    "`{path}` is a `{}` and its pins ({}) have no pads, so it cannot be placed on a symbol; add them to a `pins` table of `{}`, which says which pad each pin is",
                    inst.type_name,
                    padless
                        .iter()
                        .map(|p| format!("`{p}`"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    inst.type_name
                ),
            );
        }
    }
}

fn pads_text(row: &PinRow) -> String {
    match row.pads.as_slice() {
        [one] => format!("pad {one}"),
        many => format!("pads {}", many.join(", ")),
    }
}
