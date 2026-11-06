//! What the pin tables of a part say about one of its pins, in the package it is in
//! (ADR 0016). The faults built on these are in [`crate::faults`].

use crate::model::{
    Domain, InstId, InstKind, Member, Model, Net, PinKind, PinRow, PinTable, Pinout, RExpr,
};
use netsmith_lang::ast::CmpOp;

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
        row.map(|r| r.kind_with(self.function_of(pin).as_deref()))
    }

    /// The function the board chose for a pin: the pin's own, or else the one that a
    /// forwarding equation takes from a pin of a module (ADR 0062, stage 0).
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
        self.relations()
            .iter()
            .find_map(|r| match (&r.lhs, r.op, &r.rhs) {
                (RExpr::Field(l), CmpOp::EqEq, RExpr::Field(g)) if *l == own => {
                    let g = self.field(*g);
                    (g.name == "function" && self.instance(g.owner).kind == InstKind::Pin)
                        .then_some(g.owner)
                }
                _ => None,
            })
    }

    /// The pins whose function an equation sets from this pin's: `.mcu.p04` for `.scl`, with
    /// `.mcu.p04.function == .scl.function`.
    fn read_by(&self, pin: InstId) -> Vec<InstId> {
        let Some(Member::Field(g)) = self.member(pin, "function") else {
            return Vec::new();
        };
        self.relations()
            .iter()
            .filter_map(|r| match (&r.lhs, r.op, &r.rhs) {
                (RExpr::Field(l), CmpOp::EqEq, RExpr::Field(x)) if *x == g => {
                    let l = self.field(*l);
                    (l.name == "function" && self.instance(l.owner).kind == InstKind::Pin)
                        .then_some(l.owner)
                }
                _ => None,
            })
            .collect()
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
