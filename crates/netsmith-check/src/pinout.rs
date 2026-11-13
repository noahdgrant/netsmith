//! What the pin tables of a part say about one of its pins, in the package it is in
//! (ADR 0016). The faults built on these are in [`crate::faults`].

use crate::diag::SourceId;
use crate::model::{
    Domain, InstId, Member, Model, Net, Peripheral, PeripheralFunction, PinKind, PinRow, PinTable,
    Pinout,
};
use netsmith_lang::Span;

impl Model {
    /// The package a part is in, if its `package` field holds one.
    pub fn package_of(&self, part: InstId) -> Option<String> {
        match self.member(part, "package")? {
            Member::Field(f) => match &self.field(f).domain {
                Domain::Text(t) => Some(t.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Which pin table applies to a part.
    pub fn pinout(&self, part: InstId) -> Pinout<'_> {
        let tables = &self.instance(part).tables;
        let Some(first) = tables.first() else {
            return Pinout::NoTables;
        };
        if first.package.is_none() {
            return Pinout::Table(first);
        }
        let packages: Vec<&str> = tables.iter().filter_map(|t| t.package.as_deref()).collect();
        match self.package_of(part) {
            None => Pinout::Unknown { packages },
            Some(package) => match tables
                .iter()
                .find(|t| t.package.as_deref() == Some(package.as_str()))
            {
                Some(table) => Pinout::Table(table),
                None => Pinout::NoSuchPackage { package, packages },
            },
        }
    }

    /// The table of a part for one package: the unnamed table is for every package.
    pub fn table_for(&self, part: InstId, package: &str) -> Option<&PinTable> {
        self.instance(part)
            .tables
            .iter()
            .find(|t| t.package.as_deref().is_none_or(|p| p == package))
    }

    /// The row for a pin in the pinout that applies to its part, if there is one.
    pub fn pin_row(&self, pin: InstId) -> Option<&PinRow> {
        let part = self.owner_of(pin)?;
        match self.pinout(part) {
            Pinout::Table(table) => table.row(pin),
            _ => None,
        }
    }

    /// The first row any table of the part has for a pin. A pin's kind, name and
    /// `optional` or `required` are the same in every table that has it, so this is its
    /// description whatever the package.
    pub fn pin_facts(&self, pin: InstId) -> Option<&PinRow> {
        let part = self.owner_of(pin)?;
        self.instance(part).tables.iter().find_map(|t| t.row(pin))
    }

    /// The pin an absolute maximum of the pin's row is measured from (ADR 0124), if its row
    /// states one with a reference of its own.
    pub fn limit_reference(&self, pin: InstId) -> Option<InstId> {
        self.pin_facts(pin)?.abs_max.as_ref()?.reference
    }

    /// Whether the pin's ground is that of an interface it belongs to: it is the `.line` of a
    /// `Logic`, or a pin of an interface with `levels` (a bus), as the levels check reads it.
    pub fn grounded_by_levels(&self, pin: InstId) -> bool {
        let inst = self.instance(pin);
        let Some(parent) = inst.parent else {
            return false;
        };
        let p = self.instance(parent);
        if p.kind != crate::model::InstKind::Interface {
            return false;
        }
        if inst.name == "line" && p.type_name == "Logic" {
            return true;
        }
        matches!(self.member(parent, "levels"), Some(Member::Inst(l)) if self.is_levels(l))
    }

    /// Whether the pin is the ground of a rail: the `neg` of an interface that also has a
    /// `pos` pin.
    pub fn is_rail_ground(&self, pin: InstId) -> bool {
        let inst = self.instance(pin);
        let Some(parent) = inst.parent else {
            return false;
        };
        inst.name == "neg"
            && self.instance(parent).kind == crate::model::InstKind::Interface
            && matches!(self.member(parent, "pos"), Some(Member::Inst(p)) if self.instance(p).kind == crate::model::InstKind::Pin)
    }

    /// The packages whose table has a row for the pin.
    pub fn packages_with(&self, pin: InstId) -> Vec<&str> {
        let Some(part) = self.owner_of(pin) else {
            return Vec::new();
        };
        self.instance(part)
            .tables
            .iter()
            .filter(|t| t.row(pin).is_some())
            .filter_map(|t| t.package.as_deref())
            .collect()
    }

    /// The pins that the part's own definition joins to this one, directly or through
    /// others. The pin itself is not in the list.
    pub fn joined_inside(&self, pin: InstId) -> Vec<InstId> {
        let mut found = vec![pin];
        let mut i = 0;
        while i < found.len() {
            let at = found[i];
            for &(a, b) in &self.part_joins {
                for (from, to) in [(a, b), (b, a)] {
                    if from == at && !found.contains(&to) {
                        found.push(to);
                    }
                }
            }
            i += 1;
        }
        found.remove(0);
        found
    }

    /// Whether a pin has no row of its own but is joined inside its part to a pin that
    /// has one, so that it needs no pad of its own (ADR 0016, decision 5).
    pub fn is_alias(&self, pin: InstId) -> bool {
        self.pin_facts(pin).is_none()
            && self
                .joined_inside(pin)
                .into_iter()
                .any(|p| self.pin_facts(p).is_some())
    }

    /// The net a pin is on, if it is on one with anything else or by a name.
    pub fn net_of(&self, pin: InstId) -> Option<&Net> {
        self.nets().iter().find(|n| n.pins.contains(&pin))
    }

    /// The pins on a pin's net that are neither it nor joined to it inside its part.
    pub fn others_on_net(&self, pin: InstId) -> Vec<InstId> {
        let Some(net) = self.net_of(pin) else {
            return Vec::new();
        };
        let mine = self.joined_inside(pin);
        net.pins
            .iter()
            .copied()
            .filter(|&p| p != pin && !mine.contains(&p))
            .collect()
    }

    /// Whether the board does something with a pin: it is on a net with a pin that is not
    /// its own, or the board names its net.
    pub fn is_connected(&self, pin: InstId) -> bool {
        !self.others_on_net(pin).is_empty() || self.net_of(pin).is_some_and(|n| n.explicit)
    }

    /// The pins of a part that the board connects and that a package does not have. A
    /// package without a table has none of them.
    pub fn missing_in(&self, part: InstId, package: &str) -> Vec<InstId> {
        // One table for every package has every pin there is.
        if self
            .instance(part)
            .tables
            .iter()
            .all(|t| t.package.is_none())
        {
            return Vec::new();
        }
        let Some(table) = self.table_for(part, package) else {
            return Vec::new();
        };
        let mut pins: Vec<InstId> = self
            .instance(part)
            .tables
            .iter()
            .flat_map(|t| t.rows.iter().map(|r| r.pin))
            .filter(|&p| table.row(p).is_none() && self.is_connected(p))
            .collect();
        pins.sort_by_key(|&p| self.path_of(p));
        pins.dedup();
        pins
    }

    /// The kind of a pin for the rules of a net: the kind of the function the board chose if
    /// that function has one of its own (ADR 0046), else the pin's. From the table that
    /// applies to its part, or from any table when none single one does yet. A part with no
    /// pin table has none.
    pub fn pin_kind(&self, pin: InstId) -> Option<PinKind> {
        let part = self.owner_of(pin)?;
        let row = match self.pinout(part) {
            Pinout::Table(table) => table.row(pin),
            _ => self.pin_facts(pin),
        };
        row.map(|r| self.kind_with(part, r, self.function_of(pin).as_deref()))
    }

    /// The kind of a pin for the rules of a net, given the function the board chose for it:
    /// the function's own kind if it has one, else the pin's (ADR 0046). A function that a
    /// peripheral of the part lists takes its kind from the peripheral (ADR 0071).
    ///
    /// Why a function has its own kind (ADR 0046). A pin has one kind in a `pins` table, and a pin
    /// that several peripherals share can be a different direction as each of them: a GPIO is
    /// bidirectional, a UART transmitter an output, a receiver an input. ADR 0044 could not check
    /// "a function chosen that conflicts with the pin" because the table had one kind and a list
    /// of names, and guessing from names such as `TX` and `RX` would be a convention and not data.
    /// A function may carry its own kind after a colon with no spaces (`fn GPIO:bidirectional
    /// UART1_TX:output`); the pin's kind stays the default for the pin and for every function
    /// written without a colon. The industry usage is KiCad's alternate pin definitions
    /// (`(alternate "UART1_TX" output line)`), each with its own electrical type; ADR 0016 left
    /// them unchecked against a pin's functions and still does, because the vendor's names rarely
    /// match a part's. The net rules (`outputs` and `floating_inputs` in `faults.rs`) judge a pin
    /// by this kind, so the check is the existing pair of net rules and not a new one. The symbol,
    /// the KiCad electrical type check (ADR 0016, decision 8), the rule that an input must be
    /// connected and the other pin rules still use the pin's own kind. A function kind that
    /// repeats the pin's own is allowed, because a datasheet table lists every function's type. A
    /// function's kind in one package is not compared with its kind in another; a pin's own kind
    /// still must be the same in every package. No part in `std/` names functions, so none states
    /// a function kind.
    pub fn kind_with(&self, part: InstId, row: &PinRow, chosen: Option<&str>) -> PinKind {
        self.function_kind(part, row, chosen).unwrap_or(row.kind)
    }

    /// The function's own kind, when the board chose a function that has one: from the
    /// peripheral that lists it, or else from the row.
    pub fn function_kind(
        &self,
        part: InstId,
        row: &PinRow,
        chosen: Option<&str>,
    ) -> Option<PinKind> {
        let name = chosen?;
        self.peripheral_of(part, name)
            .and_then(|(_, f)| f.kind)
            .or_else(|| row.function_kind(Some(name)))
    }

    /// The peripherals that are `on` a function the row lists: every pin with such a row offers
    /// their functions as well (ADR 0071). Computed from the part's peripherals when it is
    /// asked, because a `peripheral` may be stated after the table.
    pub fn class_peripherals(&self, part: InstId, row: &PinRow) -> Vec<&Peripheral> {
        let mut found: Vec<&Peripheral> = Vec::new();
        for p in &self.instance(part).rules.peripherals {
            let on_row = p
                .class
                .as_ref()
                .is_some_and(|c| row.functions.iter().any(|f| f.name == *c));
            if on_row && !found.iter().any(|q| q.name == p.name) {
                found.push(p);
            }
        }
        found
    }

    /// Whether the function is a class of the part: the board may choose it on any number of
    /// pins. It is one when a peripheral is `on` it, or when a row marks it (`fn GPIO class`).
    /// The two spellings mean the same, and this is the one place that says so.
    pub fn is_class(&self, part: InstId, function: &str) -> bool {
        let inst = self.instance(part);
        inst.rules
            .peripherals
            .iter()
            .any(|p| p.class.as_deref() == Some(function))
            || inst.tables.iter().any(|t| {
                t.rows
                    .iter()
                    .any(|r| r.functions.iter().any(|f| f.class && f.name == function))
            })
    }

    /// Whether a pin with this row can be the function: the row lists it, or a peripheral on a
    /// function the row lists does.
    pub fn offers(&self, part: InstId, row: &PinRow, function: &str) -> bool {
        row.functions.iter().any(|f| f.name == function)
            || self
                .class_peripherals(part, row)
                .iter()
                .any(|p| p.functions.iter().any(|f| f.name == function))
    }

    /// How many functions a pin with this row can be.
    pub fn offered_count(&self, part: InstId, row: &PinRow) -> usize {
        let mut names: Vec<&str> = row.functions.iter().map(|f| f.name.as_str()).collect();
        for p in self.class_peripherals(part, row) {
            for f in &p.functions {
                if !names.contains(&f.name.as_str()) {
                    names.push(&f.name);
                }
            }
        }
        names.len()
    }

    /// The peripheral of a part that lists a function, and the function as it lists it.
    pub fn peripheral_of(
        &self,
        part: InstId,
        function: &str,
    ) -> Option<(&Peripheral, &PeripheralFunction)> {
        self.instance(part).rules.peripherals.iter().find_map(|p| {
            p.functions
                .iter()
                .find(|f| f.name == function)
                .map(|f| (p, f))
        })
    }

    /// The statement that chose a pin's function: on the pin, or on the pin of a module that
    /// forwards to it.
    pub fn function_stated_at(&self, pin: InstId) -> Option<(SourceId, Span)> {
        self.function_chain(pin)
            .into_iter()
            .find_map(|p| match self.member(p, "function") {
                Some(Member::Field(f)) => self.field(f).narrowed_at.last().copied(),
                _ => None,
            })
    }

    /// The function the board chose for a pin: the pin's own, or else the one that a
    /// forwarding equation takes from a pin of a module (ADR 0071, stage 0).
    ///
    /// Stage 0 of ADR 0071 (ADR 0063): a function stated on a pin that no part reads is an error.
    /// `function_of` follows an `==` whose left side is the pin's `function` and whose right side
    /// is another pin's `function`, to a depth of six (a module in a module). The checks of the
    /// part then apply to the forwarded function: it must be on the pin, a function with a kind of
    /// its own counts on the net, and a function chosen on two pins is refused. `pin_faults`
    /// reports a stated function on a pin with no row in any table of its part and that no
    /// equation passes on to a pin that has one; for a pin of a module the message names the
    /// equation to write, and its hint names the part's pin when exactly one pin with functions
    /// is on the same net inside the module. The message of a function the pin does not offer
    /// points at the board's statement through the forwarding, and the message of a connected
    /// pin that is not chosen names the module's pin that the board writes.
    ///
    /// Two choices, accepted as built by the owner, 2026-11-11. When nothing passes the choice
    /// on, the part's own pin still says it has several functions to choose from: that message is
    /// right (nothing chose one) and was there before, so both errors are printed and the new one
    /// says where the choice went. And an equation with the exported pin on the left
    /// (`.scl.function == .mcu.p04.function`) is not a forwarding: the solver works a value from
    /// the right side to the left, and the two would not agree about which pin is chosen. It is the
    /// same error, with the same hint. The `peripheral`, `shared`, `on` and `limit` of later
    /// stages of ADR 0071 are not what this function is about.
    pub fn function_of(&self, pin: InstId) -> Option<String> {
        self.function_chain(pin)
            .into_iter()
            .find_map(|p| self.own_function(p))
    }

    /// The text a pin's own `function` field holds.
    fn own_function(&self, pin: InstId) -> Option<String> {
        match self.member(pin, "function")? {
            Member::Field(f) => match &self.field(f).domain {
                Domain::Text(t) => Some(t.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// The pin an equation `.mcu.p04.function == .scl.function` takes this pin's function
    /// from: the pin whose `function` is alone on the right of an `==` whose left side is this
    /// pin's `function`. A module forwards the choice of its board to a pin of a part this way.
    fn forwarded_from(&self, pin: InstId) -> Option<InstId> {
        let Member::Field(own) = self.member(pin, "function")? else {
            return None;
        };
        self.forwards().from.get(&own).copied()
    }

    /// The pins whose function an equation sets from this pin's: `.mcu.p04` for `.scl`, with
    /// `.mcu.p04.function == .scl.function`.
    fn read_by(&self, pin: InstId) -> Vec<InstId> {
        let Some(Member::Field(g)) = self.member(pin, "function") else {
            return Vec::new();
        };
        self.forwards().read_by.get(&g).cloned().unwrap_or_default()
    }

    /// The pin, then the pins its function is taken from, one after the other, to a depth
    /// of a few (a module in a module).
    pub fn function_chain(&self, pin: InstId) -> Vec<InstId> {
        let mut chain = vec![pin];
        while chain.len() <= FORWARD_DEPTH {
            match self.forwarded_from(*chain.last().expect("a pin")) {
                Some(next) if !chain.contains(&next) => chain.push(next),
                _ => break,
            }
        }
        chain
    }

    /// Whether the function stated on a pin reaches a pin that a part's table describes:
    /// the pin is a row, or an equation passes its function on to a pin that reaches one.
    pub fn function_is_read(&self, pin: InstId) -> bool {
        self.function_read_within(pin, FORWARD_DEPTH)
    }

    fn function_read_within(&self, pin: InstId, depth: usize) -> bool {
        self.pin_facts(pin).is_some()
            || (depth > 0
                && self
                    .read_by(pin)
                    .into_iter()
                    .any(|q| q != pin && self.function_read_within(q, depth - 1)))
    }
}

/// How many modules deep a function choice is followed from the part's pin out.
const FORWARD_DEPTH: usize = 6;
