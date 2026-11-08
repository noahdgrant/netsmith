//! Logic levels and absolute maximums (ADR 0046), checked after the rails are solved.
//!
//! Two checks, both over the nets of the board:
//!
//! - **Levels.** On a net, every output (or bidirectional pin) that is the `.line` of a
//!   `Logic` drives every input (or bidirectional pin) that is one: its `voh` must always be
//!   at least the input's `vih`, and its `vol` at most the input's `vil`, in the worst case
//!   of the two ranges.
//! - **Absolute maximum.** A pin with a `.abs_min` or `.abs_max` must have a net whose
//!   worst-case voltage, relative to the pin's ground, stays within them.
//!
//! A check that cannot be made is reported as not evaluated, never as passed: a level that
//! has no value, a ground that is not the same net for both pins, a net whose voltage
//! nothing sets. A pair of pins of which neither states any level is not looked at at all,
//! as before levels existed.

use std::cell::OnceCell;
use std::collections::{BTreeSet, HashMap};

use netsmith_check::{
    Diagnostic, Domain, FieldId, InstId, InstKind, Member, Model, Net, PinKind, RExpr,
};
use netsmith_lang::ast::{BinOp, CmpOp};
use netsmith_quantity::{Dimension, Quantity};

/// A check the solver could not make, and the part it is about.
#[derive(Debug, Clone, PartialEq)]
pub struct NotEvaluated {
    pub at: InstId,
    /// The instances the check could not be made for: the `Logic`s whose level has no value,
    /// or whose grounds differ, and the part whose absolute maximum has nothing to be read
    /// against. A provider picked for a use (ADR 0068) is judged by these: the check was not
    /// made because of what that provider states, or leaves open.
    pub involves: Vec<InstId>,
    pub message: String,
}

/// The readers of the logic interfaces and resistors of a model, for the passes that need
/// them and make no check of their own.
pub(crate) fn reader(model: &Model) -> Run<'_> {
    Run {
        model,
        faults: Vec::new(),
        notes: Vec::new(),
        seen: BTreeSet::new(),
        fractions: OnceCell::new(),
    }
}

/// The faults found and the checks that could not be made.
pub(crate) fn check(model: &Model) -> (Vec<Diagnostic>, Vec<NotEvaluated>) {
    let mut run = reader(model);
    for index in 0..model.nets().len() {
        netsmith_core::job::checkpoint();
        run.levels(index);
    }
    run.absolute_maximums();
    (run.faults, run.notes)
}

/// A pin that is the `.line` of a `Logic`, with what it says about its levels.
struct Line {
    pin: InstId,
    logic: InstId,
    part: InstId,
    kind: PinKind,
}

pub(crate) struct Run<'a> {
    model: &'a Model,
    faults: Vec<Diagnostic>,
    notes: Vec<NotEvaluated>,
    seen: BTreeSet<String>,
    /// The levels stated as a constant times a rail's voltage, found once (ADR 0082).
    fractions: OnceCell<HashMap<FieldId, (f64, FieldId)>>,
}

fn is_driver(kind: PinKind) -> bool {
    matches!(
        kind,
        PinKind::Output | PinKind::TriState | PinKind::Bidirectional
    )
}

fn is_receiver(kind: PinKind) -> bool {
    matches!(kind, PinKind::Input | PinKind::Bidirectional)
}

impl Run<'_> {
    fn note(&mut self, at: InstId, involves: Vec<InstId>, message: String) {
        if self.seen.insert(message.clone()) {
            self.notes.push(NotEvaluated {
                at,
                involves,
                message,
            });
        }
    }

    fn fault(&mut self, at: InstId, message: String) {
        if self.seen.insert(message.clone()) {
            let (source, span) = self.model.instance(at).at();
            self.faults.push(Diagnostic {
                source,
                span,
                message,
            });
        }
    }

    /// The interface that holds the levels of a pin: the `Logic` the pin is the line of, or the
    /// `Levels` of the bus interface it is a pin of (ADR 0064). Both have the reference rail
    /// and the four voltages, and everything below reads them from whichever it gets.
    pub(crate) fn logic_of(&self, pin: InstId) -> Option<InstId> {
        let inst = self.model.instance(pin);
        let parent = inst.parent?;
        if self.model.instance(parent).kind != InstKind::Interface {
            return None;
        }
        if inst.name == "line" && self.model.instance(parent).type_name == "Logic" {
            return Some(parent);
        }
        match self.model.member(parent, "levels")? {
            Member::Inst(levels) if self.model.is_levels(levels) => Some(levels),
            _ => None,
        }
    }

    /// The value of a quantity field of an instance, if it has one.
    pub(crate) fn value(&self, inst: InstId, name: &str) -> Option<(FieldId, Quantity)> {
        match self.model.member(inst, name)? {
            Member::Field(f) => match self.model.field(f).domain {
                Domain::Quantity(q) => Some((f, q)),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether a `Logic` states any of its four levels.
    fn states_levels(&self, logic: InstId) -> bool {
        ["vih", "vil", "voh", "vol"]
            .iter()
            .any(|n| self.value(logic, n).is_some())
    }

    /// The `neg` pin of a `Logic`'s reference rail: its ground.
    pub(crate) fn ground_pin(&self, logic: InstId) -> Option<InstId> {
        let Some(Member::Inst(reference)) = self.model.member(logic, "reference") else {
            return None;
        };
        match self.model.member(reference, "neg")? {
            Member::Inst(neg) => Some(neg),
            _ => None,
        }
    }

    pub(crate) fn net_index(&self, pin: InstId) -> Option<usize> {
        self.model.nets().iter().position(|n| n.pins.contains(&pin))
    }

    /// Whether two ground pins are the same ground: on one net, or on nets that mated ground
    /// contacts join (ADR 0040).
    fn same_ground(&self, a: InstId, b: InstId) -> bool {
        let (Some(x), Some(y)) = (self.net_index(a), self.net_index(b)) else {
            return false;
        };
        if x == y {
            return true;
        }
        let key = |index: usize, pin: InstId| {
            (
                self.model
                    .owner_of(pin)
                    .and_then(|o| self.model.board_of(o)),
                self.model.nets()[index].name.clone(),
            )
        };
        let (ka, kb) = (key(x, a), key(y, b));
        self.model.ground_groups().iter().any(|g| {
            let has = |k: &(Option<InstId>, String)| {
                g.nets.iter().any(|n| Some(n.board) == k.0 && n.net == k.1)
            };
            has(&ka) && has(&kb)
        })
    }

    fn name_of_net(&self, pin: InstId) -> String {
        self.net_index(pin).map_or_else(
            || "(no net)".to_string(),
            |i| self.model.nets()[i].name.clone(),
        )
    }

    // ---- levels --------------------------------------------------------------------------

    fn lines_on(&self, net: &Net) -> Vec<Line> {
        let mut lines = Vec::new();
        for &pin in &net.terminals {
            let (Some(logic), Some(part), Some(kind)) = (
                self.logic_of(pin),
                self.model.owner_of(pin),
                self.model.pin_kind(pin),
            ) else {
                continue;
            };
            lines.push(Line {
                pin,
                logic,
                part,
                kind,
            });
        }
        lines
    }

    fn levels(&mut self, index: usize) {
        let model = self.model;
        let net = &model.nets()[index];
        let lines = self.lines_on(net);
        for d in lines.iter().filter(|l| is_driver(l.kind)) {
            for r in lines.iter().filter(|l| is_receiver(l.kind)) {
                if d.pin == r.pin || model.joined_inside(d.pin).contains(&r.pin) {
                    continue;
                }
                if !self.states_levels(d.logic) && !self.states_levels(r.logic) {
                    continue;
                }
                self.pair(net, d, r);
            }
        }
    }

    fn pair(&mut self, net: &Net, d: &Line, r: &Line) {
        let model = self.model;
        let (dl, rl) = (model.path_of(d.logic), model.path_of(r.logic));
        let (Some(dg), Some(rg)) = (self.ground_pin(d.logic), self.ground_pin(r.logic)) else {
            return;
        };
        if !self.same_ground(dg, rg) {
            self.note(
                d.part,
                vec![d.logic, r.logic],
                format!(
                    "not evaluated: the logic levels of `{dl}` against `{rl}` on net `{}`: their grounds differ (`{}` is on net `{}`, `{}` on net `{}`)",
                    net.name,
                    model.path_of(dg),
                    self.name_of_net(dg),
                    model.path_of(rg),
                    self.name_of_net(rg),
                ),
            );
            return;
        }
        // A high must be read as a high: the output's lowest high against the input's
        // highest threshold. An output that states a low and no high is open-drain: its high
        // is the one its pull-up gives the line (ADR 0075).
        let voh = self.value(d.logic, "voh");
        if voh.is_none() && self.value(d.logic, "vol").is_some() {
            match self.pullups(net, dg) {
                Ok(pulls) if !pulls.is_empty() => {
                    self.high_from_pullups(net, d, r, &pulls);
                    self.low(net, d, r);
                    return;
                }
                Ok(_) => {}
                Err(why) => {
                    self.note(
                        d.part,
                        vec![d.logic, r.logic],
                        format!(
                            "not evaluated: the high level from `{dl}` to `{rl}` on net `{}`: `{dl}.voh` has no value and the pull-up that would set it cannot be read: {why}",
                            net.name
                        ),
                    );
                    self.low(net, d, r);
                    return;
                }
            }
        }
        match (voh, self.value(r.logic, "vih")) {
            (Some((fo, voh)), Some((fi, vih))) => {
                if let Some((fd, fr)) = self.correlated(fo, fi, d, r) {
                    // One supply voltage: `k_d * V >= k_r * V` for every `V` above zero is
                    // `k_d >= k_r`, and nothing looser (ADR 0082).
                    if fd.factor < fr.factor {
                        self.fault(
                            d.part,
                            format!(
                                "net `{}`: `{}` is {} x `{}` but `{}` needs {} x `{}`, the same supply voltage: a high of {} x the supply is not always read as a high, which takes {} x the supply",
                                net.name,
                                model.path_of_field(fo),
                                fd.factor,
                                model.path_of_field(fd.rail),
                                model.path_of_field(fi),
                                fr.factor,
                                model.path_of_field(fr.rail),
                                fd.factor,
                                fr.factor,
                            ),
                        );
                    }
                } else if voh.lo() < vih.hi() {
                    self.fault(
                        d.part,
                        format!(
                            "net `{}`: `{}` is {voh} but `{}` needs {vih}: a high of {} is not always read as a high, which takes {}",
                            net.name,
                            model.path_of_field(fo),
                            model.path_of_field(fi),
                            volts(voh.lo()),
                            volts(vih.hi()),
                        ),
                    );
                }
            }
            (o, i) => self.missing(
                net,
                d,
                r,
                "high",
                [(&dl, "voh", o.is_none()), (&rl, "vih", i.is_none())],
            ),
        }
        self.low(net, d, r);
    }

    /// The high of an open-drain line is what its pull-up resistors hold it at: it must be
    /// read as a high by the receiver, in the worst case of the rail's range.
    fn high_from_pullups(&mut self, net: &Net, d: &Line, r: &Line, pulls: &[PullUp]) {
        let model = self.model;
        let Some((fi, vih)) = self.value(r.logic, "vih") else {
            let rl = model.path_of(r.logic);
            self.missing(
                net,
                d,
                r,
                "high",
                [(&model.path_of(d.logic), "voh", false), (&rl, "vih", true)],
            );
            return;
        };
        // The lowest of the rails that pull the line up is its lowest high.
        let Some(low) = pulls.iter().min_by(|a, b| a.lo.total_cmp(&b.lo)) else {
            return;
        };
        let (lo, hi) = pulls
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.lo), hi.max(p.hi))
            });
        if lo < vih.hi() {
            let point = |v| Quantity::point(Dimension::VOLTAGE, v);
            let high = match (point(lo), point(hi)) {
                (Ok(a), Ok(b)) => {
                    Quantity::range(&a, &b).map_or_else(|_| volts(lo), |q| q.to_string())
                }
                _ => volts(lo),
            };
            self.fault(
                r.part,
                format!(
                    "net `{}`: the pull-up `{}` to rail `{}` holds the line at {high} (a driver that states `vol` and no `voh` leaves the high to the pull-up) but `{}` needs {vih}: a high of {} is not always read as a high, which takes {}",
                    net.name,
                    low.resistor,
                    low.rail,
                    model.path_of_field(fi),
                    volts(lo),
                    volts(vih.hi()),
                ),
            );
        }
    }

    /// A low must be read as a low: the output's highest low against the input's lowest
    /// threshold.
    fn low(&mut self, net: &Net, d: &Line, r: &Line) {
        let model = self.model;
        let (dl, rl) = (model.path_of(d.logic), model.path_of(r.logic));
        // A low must be read as a low.
        match (self.value(d.logic, "vol"), self.value(r.logic, "vil")) {
            (Some((fo, vol)), Some((fi, vil))) => {
                if let Some((fd, fr)) = self.correlated(fo, fi, d, r) {
                    // The same reduction for the low: `k_d * V <= k_r * V` is `k_d <= k_r`.
                    if fd.factor > fr.factor {
                        self.fault(
                            d.part,
                            format!(
                                "net `{}`: `{}` is {} x `{}` but `{}` allows {} x `{}`, the same supply voltage: a low of {} x the supply is not always read as a low, which takes {} x the supply",
                                net.name,
                                model.path_of_field(fo),
                                fd.factor,
                                model.path_of_field(fd.rail),
                                model.path_of_field(fi),
                                fr.factor,
                                model.path_of_field(fr.rail),
                                fd.factor,
                                fr.factor,
                            ),
                        );
                    }
                } else if vol.hi() > vil.lo() {
                    self.fault(
                        d.part,
                        format!(
                            "net `{}`: `{}` is {vol} but `{}` allows {vil}: a low of {} is not always read as a low, which takes {}",
                            net.name,
                            model.path_of_field(fo),
                            model.path_of_field(fi),
                            volts(vol.hi()),
                            volts(vil.lo()),
                        ),
                    );
                }
            }
            (o, i) => self.missing(
                net,
                d,
                r,
                "low",
                [(&dl, "vol", o.is_none()), (&rl, "vil", i.is_none())],
            ),
        }
    }

    /// The levels stated as a constant times a rail's voltage: `.voh &= 0.7 * .vdd.voltage`
    /// (or `.vdd.voltage * 0.7`, or the voltage alone). The checker keeps the statement as a
    /// relation whose right side is the expression, which is where the form survives the
    /// evaluation to a range (ADR 0082). A field is listed only when that one statement is
    /// all there is to it: no other relation mentions it and no constant narrows it.
    fn fractions(&self) -> &HashMap<FieldId, (f64, FieldId)> {
        self.fractions.get_or_init(|| {
            let model = self.model;
            let mut mentions: HashMap<FieldId, usize> = HashMap::new();
            let mut found: HashMap<FieldId, (f64, FieldId)> = HashMap::new();
            for relation in model.relations() {
                let mut fields = Vec::new();
                relation.lhs.fields(&mut fields);
                relation.rhs.fields(&mut fields);
                for f in fields {
                    *mentions.entry(f).or_insert(0) += 1;
                }
                if relation.op != CmpOp::Narrow {
                    continue;
                }
                let RExpr::Field(level) = relation.lhs else {
                    continue;
                };
                if let Some(factor) = factor_of_rail(&relation.rhs) {
                    found.insert(level, factor);
                }
            }
            found.retain(|f, _| {
                mentions.get(f) == Some(&1) && model.field(*f).narrowed_at.is_empty()
            });
            found
        })
    }

    /// The factor and the rail voltage of a level, if it is one of those [`Self::fractions`]
    /// lists and the rail is a real one: a rail whose voltage is above zero at its lowest
    /// (the reduction to the factors divides by it) and whose ground is the level's.
    fn fraction(&self, level: FieldId, logic: InstId) -> Option<Fraction> {
        let model = self.model;
        let &(factor, rail) = self.fractions().get(&level)?;
        let owner = model.field(rail).owner;
        let (voltage, pos, neg) = model.rail_of(owner)?;
        if voltage != rail || !factor.is_finite() {
            return None;
        }
        let Domain::Quantity(q) = model.field(rail).domain else {
            return None;
        };
        if q.lo() <= 0.0 || !self.same_ground(neg, self.ground_pin(logic)?) {
            return None;
        }
        Some(Fraction {
            factor,
            rail,
            pos: self.net_index(pos)?,
        })
    }

    /// The two levels of a driver and a receiver, when both are the same constant times the
    /// voltage of one supply: the rails whose voltages they name have their positive pins on
    /// one net and the same ground, so the voltage is one number for both. `None` for
    /// anything else, and the ranges are then compared as independent ones.
    fn correlated(
        &self,
        driver: FieldId,
        receiver: FieldId,
        d: &Line,
        r: &Line,
    ) -> Option<(Fraction, Fraction)> {
        let fd = self.fraction(driver, d.logic)?;
        let fr = self.fraction(receiver, r.logic)?;
        (fd.pos == fr.pos).then_some((fd, fr))
    }

    fn missing(
        &mut self,
        net: &Net,
        d: &Line,
        r: &Line,
        which: &str,
        need: [(&String, &str, bool); 2],
    ) {
        let lacking: Vec<String> = need
            .iter()
            .filter(|(_, _, none)| *none)
            .map(|(path, name, _)| format!("`{path}.{name}`"))
            .collect();
        let model = self.model;
        let lacking_logics: Vec<InstId> = [d.logic, r.logic]
            .into_iter()
            .zip(need.iter())
            .filter(|(_, (_, _, none))| *none)
            .map(|(logic, _)| logic)
            .collect();
        self.note(
            d.part,
            lacking_logics,
            format!(
                "not evaluated: the {which} level from `{}` to `{}` on net `{}`: {} {} no value",
                model.path_of(d.logic),
                model.path_of(r.logic),
                net.name,
                lacking.join(" and "),
                if lacking.len() == 1 { "has" } else { "have" },
            ),
        );
    }

    // ---- absolute maximum ----------------------------------------------------------------

    fn absolute_maximums(&mut self) {
        let model = self.model;
        let pins: Vec<InstId> = model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Pin)
            .map(|(id, _)| id)
            .collect();
        for pin in pins {
            netsmith_core::job::checkpoint();
            let min = self.value(pin, "abs_min");
            let max = self.value(pin, "abs_max");
            if min.is_none() && max.is_none() {
                continue;
            }
            let Some(index) = self.net_index(pin) else {
                continue;
            };
            if !model.nets()[index].terminals.contains(&pin) {
                continue;
            }
            let Some(owner) = model.owner_of(pin) else {
                continue;
            };
            let path = model.path_of(pin);
            let Some(logic) = self.logic_of(pin) else {
                self.note(
                    owner,
                    vec![owner],
                    format!(
                        "not evaluated: the absolute maximum of `{path}`: it is not the `line` of a `Logic` or a pin of an interface with `levels`, so its ground is not known"
                    ),
                );
                continue;
            };
            let Some(ground) = self.ground_pin(logic) else {
                continue;
            };
            let swing = match self.net_range(index, ground) {
                Ok(range) => range,
                Err(why) => {
                    self.note(
                        owner,
                        vec![owner],
                        format!(
                            "not evaluated: the absolute maximum of `{path}` against net `{}`: {why}",
                            model.nets()[index].name
                        ),
                    );
                    continue;
                }
            };
            let gnd = self.name_of_net(ground);
            if let Some((f, limit)) = max
                && swing.hi > limit.lo()
            {
                self.fault(
                    owner,
                    format!(
                        "net `{}` reaches {} above its ground `{gnd}` ({}) but `{}` is {limit}: it must not exceed {}",
                        model.nets()[index].name,
                        volts(swing.hi),
                        swing.hi_from,
                        model.path_of_field(f),
                        volts(limit.lo()),
                    ),
                );
            }
            if let Some((f, limit)) = min
                && swing.lo < limit.hi()
            {
                self.fault(
                    owner,
                    format!(
                        "net `{}` reaches {} relative to its ground `{gnd}` ({}) but `{}` is {limit}: it must not go below {}",
                        model.nets()[index].name,
                        volts(swing.lo),
                        swing.lo_from,
                        model.path_of_field(f),
                        volts(limit.hi()),
                    ),
                );
            }
        }
    }

    /// The terminal at the other end of a resistor, if `pin` is one end of a part that is
    /// one: it has a `bridge` and a `resistance`.
    pub(crate) fn resistor_far_end(&self, pin: InstId) -> Option<InstId> {
        let model = self.model;
        let part = model.owner_of(pin)?;
        let (a, b) = model.instance(part).bridge?;
        match model.member(part, "resistance")? {
            Member::Field(r)
                if model.field(r).kind
                    == netsmith_check::FieldKind::Quantity(Dimension::RESISTANCE) => {}
            _ => return None,
        }
        match pin {
            p if p == a => Some(b),
            p if p == b => Some(a),
            _ => None,
        }
    }

    /// The resistors that pull a net up to a rail: each resistor from the net to a net that
    /// has the positive pin of a rail (one step, as `net_range` reads a pull). Empty when there
    /// is none, and also when the net has a resistor to its ground as well, which makes it a
    /// divider whose voltage is not the rail's. The reason a rail cannot be read otherwise.
    fn pullups(&self, net: &Net, ground: InstId) -> Result<Vec<PullUp>, String> {
        let model = self.model;
        let mut found: Vec<PullUp> = Vec::new();
        for &t in &net.terminals {
            let Some(far) = self.resistor_far_end(t) else {
                continue;
            };
            let Some(far_net) = self.net_index(far) else {
                continue;
            };
            if self.net_index(ground) == Some(far_net) {
                return Ok(Vec::new());
            }
            let Some(part) = model.owner_of(t) else {
                continue;
            };
            let resistor = model.path_of(part);
            for &pin in &model.nets()[far_net].pins {
                let Some(parent) = model.instance(pin).parent else {
                    continue;
                };
                let Some((voltage, pos, neg)) = model.rail_of(parent) else {
                    continue;
                };
                if pin != pos {
                    continue;
                }
                let rail = model.path_of(parent);
                if !self.same_ground(neg, ground) {
                    return Err(format!(
                        "a resistor joins it to rail `{rail}`, whose ground `{}` is on net `{}`, not `{}`",
                        model.path_of(neg),
                        self.name_of_net(neg),
                        self.name_of_net(ground)
                    ));
                }
                let Domain::Quantity(q) = model.field(voltage).domain else {
                    return Err(format!(
                        "a resistor joins it to rail `{rail}`, which has no voltage"
                    ));
                };
                found.push(PullUp {
                    resistor: resistor.clone(),
                    rail,
                    lo: q.lo(),
                    hi: q.hi(),
                });
                // The rails on one net have one voltage: the first names it.
                break;
            }
        }
        Ok(found)
    }

    /// What the net at the far end of a resistor sets: ground, or a rail that has the same
    /// ground. Anything else it may be joined to is not followed.
    fn pull(&self, far: usize, ground: InstId, swing: &mut Swing) -> Result<(), String> {
        let model = self.model;
        if self.net_index(ground) == Some(far) {
            swing.add(0.0, 0.0, "a resistor to ground");
            return Ok(());
        }
        for &pin in &model.nets()[far].pins {
            let Some(parent) = model.instance(pin).parent else {
                continue;
            };
            let Some((voltage, pos, neg)) = model.rail_of(parent) else {
                continue;
            };
            if pin != pos {
                continue;
            }
            let rail = model.path_of(parent);
            if !self.same_ground(neg, ground) {
                return Err(format!(
                    "a resistor joins it to rail `{rail}`, whose ground `{}` is on net `{}`, not `{}`",
                    model.path_of(neg),
                    self.name_of_net(neg),
                    self.name_of_net(ground)
                ));
            }
            match model.field(voltage).domain {
                Domain::Quantity(q) => {
                    swing.add(q.lo(), q.hi(), &format!("a resistor to rail `{rail}`"))
                }
                _ => {
                    return Err(format!(
                        "a resistor joins it to rail `{rail}`, which has no voltage"
                    ));
                }
            }
        }
        Ok(())
    }

    /// The worst-case voltage of a net relative to a ground, and what sets each end. The
    /// reason it cannot be known otherwise.
    fn net_range(&self, index: usize, ground: InstId) -> Result<Swing, String> {
        let model = self.model;
        let net = &model.nets()[index];
        let mut swing = Swing::none();
        let mut seen_rails: Vec<InstId> = Vec::new();

        if self.net_index(ground) == Some(index) {
            swing.add(0.0, 0.0, "ground");
        }
        for &pin in &net.pins {
            let Some(parent) = model.instance(pin).parent else {
                continue;
            };
            let Some((voltage, pos, neg)) = model.rail_of(parent) else {
                continue;
            };
            if seen_rails.contains(&parent) {
                continue;
            }
            let rail = model.path_of(parent);
            if pin == pos {
                seen_rails.push(parent);
                if !self.same_ground(neg, ground) {
                    return Err(format!(
                        "rail `{rail}` is on it and its ground `{}` is on net `{}`, not `{}`",
                        model.path_of(neg),
                        self.name_of_net(neg),
                        self.name_of_net(ground)
                    ));
                }
                match model.field(voltage).domain {
                    Domain::Quantity(q) => swing.add(q.lo(), q.hi(), &format!("rail `{rail}`")),
                    _ => return Err(format!("rail `{rail}` is on it and has no voltage")),
                }
            } else if pin == neg && self.net_index(ground) != Some(index) {
                return Err(format!(
                    "the ground `{}` of rail `{rail}` is on it, and it is not the pin's ground",
                    model.path_of(neg)
                ));
            }
        }
        for &t in &net.terminals {
            // A resistor to a rail or to ground pulls the net there: a pull-up or a
            // pull-down.
            if let Some(far) = self.resistor_far_end(t)
                && let Some(far_net) = self.net_index(far)
            {
                self.pull(far_net, ground, &mut swing)?;
                continue;
            }
            let Some(kind) = model.pin_kind(t) else {
                continue;
            };
            if !is_driver(kind) {
                continue;
            }
            let path = model.path_of(t);
            let Some(logic) = self.logic_of(t) else {
                return Err(format!(
                    "`{path}` can drive it and is not the `line` of a `Logic` or a pin of an interface with `levels`, so how far it swings is not known"
                ));
            };
            let Some(g) = self.ground_pin(logic) else {
                continue;
            };
            if !self.same_ground(g, ground) {
                return Err(format!(
                    "`{path}` drives it from another ground (`{}` is on net `{}`)",
                    model.path_of(g),
                    self.name_of_net(g)
                ));
            }
            let Some(Member::Inst(reference)) = model.member(logic, "reference") else {
                continue;
            };
            match self.value(reference, "voltage") {
                Some((_, q)) => swing.add(
                    0.0,
                    q.hi().max(0.0),
                    &format!("`{path}`, a line of `{}`", model.path_of(reference)),
                ),
                None => {
                    return Err(format!(
                        "`{path}` can drive it and its reference rail `{}` has no voltage",
                        model.path_of(reference)
                    ));
                }
            }
        }
        if swing.is_none() {
            return Err(
                "nothing sets its voltage: no rail is on it and no `Logic` line drives it"
                    .to_string(),
            );
        }
        Ok(swing)
    }
}

/// A level that is `factor` times the voltage of a rail.
struct Fraction {
    factor: f64,
    /// The voltage field of the rail the expression names.
    rail: FieldId,
    /// The net the rail's positive pin is on.
    pos: usize,
}

/// `k * V`, `V * k` or `V`, where `V` is a field and `k` a number with no dimension and no
/// width: the factor and the field.
fn factor_of_rail(e: &RExpr) -> Option<(f64, FieldId)> {
    let number = |e: &RExpr| match e {
        RExpr::Const(q) if q.dim().is_dimensionless() && q.is_point() => Some(q.lo()),
        _ => None,
    };
    match e {
        RExpr::Field(v) => Some((1.0, *v)),
        RExpr::Bin(BinOp::Mul, a, b) => match (&**a, &**b) {
            (k, RExpr::Field(v)) => Some((number(k)?, *v)),
            (RExpr::Field(v), k) => Some((number(k)?, *v)),
            _ => None,
        },
        _ => None,
    }
}

/// A resistor that holds a net at a rail: the rail and the range of its voltage.
struct PullUp {
    /// The path of the resistor.
    resistor: String,
    rail: String,
    lo: f64,
    hi: f64,
}

/// The range a net can take, with what sets each end.
struct Swing {
    lo: f64,
    hi: f64,
    lo_from: String,
    hi_from: String,
    any: bool,
}

impl Swing {
    fn none() -> Self {
        Self {
            lo: 0.0,
            hi: 0.0,
            lo_from: String::new(),
            hi_from: String::new(),
            any: false,
        }
    }

    fn is_none(&self) -> bool {
        !self.any
    }

    fn add(&mut self, lo: f64, hi: f64, from: &str) {
        if !self.any || lo < self.lo {
            self.lo = lo;
            self.lo_from = from.to_string();
        }
        if !self.any || hi > self.hi {
            self.hi = hi;
            self.hi_from = from.to_string();
        }
        self.any = true;
    }
}

fn volts(v: f64) -> String {
    Quantity::point(Dimension::VOLTAGE, v).map_or_else(|_| format!("{v} V"), |q| q.to_string())
}
