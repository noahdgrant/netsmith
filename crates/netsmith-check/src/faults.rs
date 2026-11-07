//! The faults of a board's pins (ADR 0016, decisions 4 and 5).
//!
//! Most need only the board and the pin tables of its parts: a pin that must be connected
//! and is not, two pins that a part ties and the board does not, an output driving an
//! output. [`pin_faults`] reports those when the board is checked. A rule that depends on
//! the package waits for the solver if the board has not chosen the package (a catalog row
//! does): [`pin_faults_solved`] reports those, and the pad rules, which need the pads of the
//! package that was chosen.

use std::collections::{BTreeMap, HashSet};

use crate::diag::Diagnostic;
use crate::model::{InstId, InstKind, Model, Net, PinKind, PinRow, PinTable, Pinout, Requirement};
use crate::pins::article;

/// The faults that the board alone settles: every part whose package is known, and every
/// net whose parts all are.
pub fn pin_faults(model: &Model) -> Vec<Diagnostic> {
    let deferred = unknown_package(model);
    let mut run = Run::new(model, &deferred, Pass::Board);
    run.go();
    run.out.extend(crate::buses::i2c_pullups(model));
    run.out
}

/// The faults that wait for the solver: the parts whose package `before` did not know, and
/// the nets they are on, and the pad rules for every part (`solved` has the packages the
/// solver chose).
pub fn pin_faults_solved(before: &Model, solved: &Model) -> Vec<Diagnostic> {
    let deferred = unknown_package(before);
    let mut run = Run::new(solved, &deferred, Pass::Solved);
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
    out: Vec<Diagnostic>,
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
    fn new(model: &'a Model, deferred: &'a HashSet<InstId>, pass: Pass) -> Self {
        Self {
            model,
            deferred,
            pass,
            out: Vec::new(),
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
        }
        self.nets(&parts);
    }

    /// The pin tables say how a part is in the package it is in, if any does.
    fn part(&mut self, part: InstId) {
        let model = self.model;
        match model.pinout(part) {
            Pinout::NoTables | Pinout::Unknown { .. } => {}
            Pinout::NoSuchPackage { package, packages } => {
                let ty = &model.instance(part).type_name;
                let packages: Vec<String> = packages.iter().map(|p| p.to_string()).collect();
                self.report(
                    model.instance(part).at(),
                    format!(
                        "`{}` is a `{ty}` in package `{package}`, which has no pin table; its tables are for {}",
                        model.path_of(part),
                        list(&packages)
                    ),
                );
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

            let must = matches!(
                (row.kind, row.requirement),
                (_, Some(Requirement::Required)) | (PinKind::Power | PinKind::Input, None)
            );
            if must && !self.has_company(part, pin) {
                self.report(
                    inst.at(),
                    format!(
                        "`{name}` is {} that must be connected ({pads}, `{}`), but nothing else is on its net",
                        article(row.kind),
                        row.name
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
        match chosen {
            Some(f) if !functions.contains(&f) => {
                let at = self.function_site(pin, part);
                let message = if functions.is_empty() {
                    format!("`{name}` cannot be `{f}`; it has no functions to choose from")
                } else {
                    format!(
                        "`{name}` cannot be `{f}`; its functions are {}",
                        functions.join(", ")
                    )
                };
                self.report(at, message);
            }
            Some(_) => {}
            None if functions.len() > 1 && model.is_connected(pin) => {
                let scope = model.instance(part).parent.unwrap_or(model.root);
                let relative = model.path_below(pin, scope);
                self.report(
                    model.instance(part).at(),
                    format!(
                        "`{name}` has several functions ({}) and is connected: choose one with `.{relative}.function &= \"{}\"`",
                        functions.join(", "),
                        functions[0]
                    ),
                );
            }
            None => {}
        }
    }

    /// Where the board chose a function: the statement, or else the part.
    fn function_site(
        &self,
        pin: InstId,
        part: InstId,
    ) -> (crate::diag::SourceId, netsmith_lang::Span) {
        let model = self.model;
        match model.member(pin, "function") {
            Some(crate::model::Member::Field(f)) => model
                .field(f)
                .narrowed_at
                .last()
                .copied()
                .unwrap_or_else(|| model.instance(part).at()),
            _ => model.instance(part).at(),
        }
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
            if pins.len() < 2 {
                continue;
            }
            pins.sort_by_key(|&p| model.path_of(p));
            let names: Vec<String> = pins.iter().map(|&p| model.path_of(p)).collect();
            let at = self.function_site(pins[1], part);
            self.report(
                at,
                format!(
                    "function `{function}` is chosen on both {}",
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
            self.outputs(net, &mine);
            self.floating_inputs(net);
        }
    }

    /// Two outputs on one net. Pins joined inside their part are one output.
    fn outputs(&mut self, net: &Net, terminals: &[InstId]) {
        let model = self.model;
        let outputs: Vec<InstId> = terminals
            .iter()
            .copied()
            .filter(|&t| self.kind_here(t) == Some(PinKind::Output))
            .collect();
        let mut distinct: Vec<InstId> = Vec::new();
        for o in outputs {
            let group = model.joined_inside(o);
            if !distinct.iter().any(|d| group.contains(d)) {
                distinct.push(o);
            }
        }
        if distinct.len() >= 2 {
            let names: Vec<String> = distinct.iter().map(|&p| self.describe(p)).collect();
            let at = model
                .instance(model.owner_of(distinct[0]).expect("a terminal"))
                .at();
            self.report(
                at,
                format!("net `{}` has two outputs: {}", net.name, list_plain(&names)),
            );
        }
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
            Pinout::Table(table) => table
                .row(pin)
                .map(|r| r.kind_with(self.model.function_of(pin).as_deref())),
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
        let kind = row.function_kind(Some(&chosen))?;
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
        // A part with no pin table has no pad for any pin, which lowering says (a part that
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
