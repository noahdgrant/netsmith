//! The power-up order of a board (`sequence { .a.vout; then 6ms; .b.vout }`, issue #1267),
//! checked after the values are solved.
//!
//! Each event gets a window of time from power-on, from the earliest it can happen to the latest,
//! and an order `later` at least `d` after `earlier` holds when the earliest of `later` is at
//! least `d` after the latest of `earlier`. The margin is the difference; a negative margin is a
//! fault that names the two events, the times and how much is missing. A window is worked out
//! from what the board is:
//!
//! - **A supply.** A net that holds the `pos` of a rail and that no regulator drives is up at
//!   power-on (a connector's supply, a board's own input rail). A net that a MOSFET switches is
//!   not modelled (a load switch's gate drive), and an order that needs it is not evaluated.
//! - **A regulator's output.** A part with a `dc linear_regulator` or `dc switching_regulator`
//!   element drives the `pos` net of its output rail, and the nets an inductor, a ferrite bead or
//!   a fuse joins to it. It starts when its input is up and its enable pin `.en` (when it has one)
//!   is high, then waits its `.enable_delay` and rises for its `.soft_start_time`, each read as a
//!   range from its least to its most. A part that states no `.enable_delay` has the generic's
//!   default (none); a value with no value (a switching IC's `.soft_start_time`, which is
//!   optional) adds nothing to the earliest time and leaves the latest one not known. An enable
//!   pin that nothing on the board joins follows the input (an internal pull-up).
//! - **A load switch's output.** A part with a `dc load_switch` element (`std::parts::switch`)
//!   drives its output as a regulator does, from its input and the enable its element names.
//!   Its turn-on time is not modelled: it adds nothing to the earliest time, and an order that
//!   needs the latest time is not evaluated. An enable that nothing joins holds it off.
//! - **A pin going high.** A pin on a supply or a regulator's output goes high as the rail rises:
//!   from the start of its rise to the time it is up. A pin on a net pulled up by one resistor
//!   from a rail, with a resistor to ground (a divider) or none and capacitors to ground, crosses
//!   the threshold `Vth` of its part (`.enable_threshold`) after
//!   `R * C * ln(V / (V - Vth))`, with `R` and `V` the Thevenin resistance and voltage of the
//!   resistors, each end of the window at the worst of the ranges: the earliest from the start of
//!   the rail's rise, the latest from the time it is up. A part that states no threshold leaves
//!   the latest time not known. A net that an output pin drives (a GPIO), or that is pulled up
//!   from two nets, is not modelled.
//!
//! An order that cannot be judged (a window with no latest time where it is needed, a pin that
//! nothing drives) is listed as not evaluated, with what to state.

use std::collections::HashMap;

use netsmith_check::{
    DcKind, Diagnostic, Domain, EventKind, InstId, InstKind, Member, Model, PinKind, SequenceEvent,
};
use netsmith_core::Cause;
use netsmith_quantity::{Dimension, Quantity};

use crate::levels::NotEvaluated;

/// The faults of the board's power-up order, and the orders that could not be judged.
pub(crate) fn check(model: &Model) -> (Vec<Diagnostic>, Vec<NotEvaluated>) {
    let mut faults = Vec::new();
    let mut notes = Vec::new();
    if model.sequence().is_empty() {
        return (faults, notes);
    }
    let mut timing = Timing::new(model);
    for order in model.sequence() {
        let earlier = timing.event(&order.earlier);
        let later = timing.event(&order.later);
        let earlier_e = model.path_of(order.earlier.inst);
        let later_e = model.path_of(order.later.inst);
        let not_judged = |why: String| NotEvaluated {
            at: order.later.inst,
            involves: vec![order.earlier.inst, order.later.inst],
            message: format!(
                "the `sequence` order of `{later_e}` after `{earlier_e}` is not judged: {why}"
            ),
            cause: Cause::NotStated,
        };
        let (earlier, later) = match (earlier, later) {
            (Ok(e), Ok(l)) => (e, l),
            (Err(why), _) | (_, Err(why)) => {
                notes.push(not_judged(why));
                continue;
            }
        };
        if !earlier.hi.is_finite() {
            let why = earlier
                .open
                .clone()
                .unwrap_or_else(|| "its latest time is not known".into());
            notes.push(not_judged(format!(
                "the latest time `{earlier_e}` {} is not known: {why}",
                verb(&order.earlier)
            )));
            continue;
        }
        let gap = later.lo - earlier.hi;
        let margin = gap - order.delay;
        if margin >= -1e-9 * order.delay.max(1e-6) {
            continue;
        }
        let need = if order.delay > 0.0 {
            format!("at least {} after", show(order.delay))
        } else {
            "at or after".to_string()
        };
        let may = if gap >= 0.0 {
            format!("only {} after it", show(gap))
        } else {
            format!("{} before it", show(-gap))
        };
        faults.push(Diagnostic {
            source: order.source,
            span: order.span,
            message: format!(
                "the `sequence` says `{later_e}` comes {need} `{earlier_e}`, but it may come {may}: `{earlier_e}` {ev} by {eh} at the latest ({ew}) and `{later_e}` {lv} from {ll} at the earliest ({lw}); the margin is {m}, {short} short",
                ev = verb(&order.earlier),
                lv = verb(&order.later),
                eh = show(earlier.hi),
                ll = show(later.lo),
                ew = earlier.how,
                lw = later.how,
                m = show(margin),
                short = show(-margin),
            ),
        });
    }
    (faults, notes)
}

fn verb(event: &SequenceEvent) -> &'static str {
    match event.kind {
        EventKind::Rail => "is up",
        EventKind::Pin => "goes high",
    }
}

/// When an event happens, in seconds from power-on: from `lo` at the earliest to `hi` at the
/// latest (infinite when it is not known, and `open` says why). `start` is the earliest a rail
/// may begin to rise, which an RC on it starts charging from.
#[derive(Debug, Clone)]
struct When {
    start: f64,
    lo: f64,
    hi: f64,
    open: Option<String>,
    /// What the window comes from, for a message.
    how: String,
}

impl When {
    fn at_power_on(how: String) -> Self {
        Self {
            start: 0.0,
            lo: 0.0,
            hi: 0.0,
            open: None,
            how,
        }
    }

    /// When a pin on this rail goes high: from the start of its rise to the time it is up.
    fn crossing(mut self) -> Self {
        self.lo = self.start;
        self
    }
}

/// A part that drives the `pos` net of its output rail.
struct Regulator {
    part: InstId,
    vin_net: usize,
    out_net: usize,
    /// The enable pin of a `dc load_switch` element; a regulator's is its member `.en`.
    switch_enable: Option<InstId>,
}

struct Timing<'a> {
    model: &'a Model,
    /// The net of each pin, by its place in `Model::nets`.
    net_of: HashMap<InstId, usize>,
    /// For each net, the first net of the nets an inductor, a bead or a fuse joins it to.
    group: Vec<usize>,
    regulators: Vec<Regulator>,
    /// For each net, whether it holds the `pos` of a rail, and the rail.
    supply: Vec<Option<InstId>>,
    /// For each net, whether it holds the `neg` of a rail and no `pos`.
    ground: Vec<bool>,
    /// The regulators being worked out, to stop at a loop.
    visiting: Vec<InstId>,
}

impl<'a> Timing<'a> {
    fn new(model: &'a Model) -> Self {
        let nets = model.nets();
        let mut net_of = HashMap::new();
        for (k, net) in nets.iter().enumerate() {
            for pin in &net.pins {
                net_of.insert(*pin, k);
            }
        }
        let mut supply = vec![None; nets.len()];
        let mut neg = vec![false; nets.len()];
        for (id, _) in model.instances() {
            if let Some((_, pos, n)) = model.rail_of(id) {
                if let Some(&k) = net_of.get(&pos) {
                    supply[k].get_or_insert(id);
                }
                if let Some(&k) = net_of.get(&n) {
                    neg[k] = true;
                }
            }
        }
        let ground = (0..nets.len())
            .map(|k| neg[k] && supply[k].is_none())
            .collect();
        let mut group: Vec<usize> = (0..nets.len()).collect();
        fn root(group: &mut [usize], k: usize) -> usize {
            let mut k = k;
            while group[k] != k {
                group[k] = group[group[k]];
                k = group[k];
            }
            k
        }
        let mut regulators = Vec::new();
        for (part, element) in model.dc_elements() {
            match element.kind {
                DcKind::Resistor => {
                    let standard = &model.instance(part).standard;
                    if !standard
                        .iter()
                        .any(|s| matches!(s.as_str(), "Inductor" | "FerriteBead" | "Fuse"))
                    {
                        continue;
                    }
                    let (Some(&a), Some(&b)) = (
                        element.pins.first().and_then(|p| net_of.get(p)),
                        element.pins.get(1).and_then(|p| net_of.get(p)),
                    ) else {
                        continue;
                    };
                    let (ra, rb) = (root(&mut group, a), root(&mut group, b));
                    group[ra.max(rb)] = ra.min(rb);
                }
                DcKind::LinearRegulator | DcKind::SwitchingRegulator | DcKind::LoadSwitch => {
                    let pos = |rail: Option<&InstId>| {
                        rail.and_then(|r| model.rail_of(*r))
                            .and_then(|(_, pos, _)| net_of.get(&pos).copied())
                    };
                    if let (Some(vin_net), Some(out_net)) =
                        (pos(element.pins.first()), pos(element.pins.get(1)))
                    {
                        regulators.push(Regulator {
                            part,
                            vin_net,
                            out_net,
                            switch_enable: (element.kind == DcKind::LoadSwitch)
                                .then(|| element.pins.get(2).copied())
                                .flatten(),
                        });
                    }
                }
                _ => {}
            }
        }
        let group = (0..nets.len()).map(|k| root(&mut group, k)).collect();
        Self {
            model,
            net_of,
            group,
            regulators,
            supply,
            ground,
            visiting: Vec::new(),
        }
    }

    fn event(&mut self, event: &SequenceEvent) -> Result<When, String> {
        let pin = match event.kind {
            EventKind::Rail => {
                let (_, pos, _) = self.model.rail_of(event.inst).ok_or("it is not a rail")?;
                pos
            }
            EventKind::Pin => event.inst,
        };
        let net = *self
            .net_of
            .get(&pin)
            .ok_or_else(|| format!("`{}` is on no net", event.written))?;
        match event.kind {
            EventKind::Rail => self.rail(net),
            EventKind::Pin => {
                let threshold = self.threshold_of(pin);
                self.pin(net, pin, threshold)
            }
        }
    }

    fn name(&self, net: usize) -> &str {
        &self.model.nets()[net].name
    }

    fn in_group(&self, net: usize) -> impl Iterator<Item = usize> + '_ {
        let g = self.group[net];
        (0..self.group.len()).filter(move |k| self.group[*k] == g)
    }

    /// When the rail on `net` is up: a regulator's output, or a supply.
    fn rail(&mut self, net: usize) -> Result<When, String> {
        let g = self.group[net];
        if let Some(k) = self
            .regulators
            .iter()
            .position(|r| self.group[r.out_net] == g)
        {
            return self.regulator(k);
        }
        let members: Vec<usize> = self.in_group(net).collect();
        if !members.iter().any(|k| self.supply[*k].is_some()) {
            return Err(format!("`{}` holds no rail", self.name(net)));
        }
        // A rail that a MOSFET switches comes up when its gate says, which is not modelled.
        for (part, element) in self.model.dc_elements() {
            if matches!(element.kind, DcKind::Nmos | DcKind::Pmos)
                && element.pins[1..]
                    .iter()
                    .any(|p| self.net_of.get(p).is_some_and(|n| members.contains(n)))
            {
                return Err(format!(
                    "`{}` is switched by `{}`, and the time a switch turns on is not modelled",
                    self.name(net),
                    self.model.path_of(part)
                ));
            }
        }
        Ok(When::at_power_on(format!(
            "`{}` is a supply, up at power-on",
            self.name(net)
        )))
    }

    /// When the regulator `k`'s output is up.
    fn regulator(&mut self, k: usize) -> Result<When, String> {
        let part = self.regulators[k].part;
        let path = self.model.path_of(part);
        if self.visiting.contains(&part) {
            return Err(format!("the start of `{path}` waits on its own output"));
        }
        self.visiting.push(part);
        let result = self.regulator_inner(k, &path);
        self.visiting.pop();
        result
    }

    fn regulator_inner(&mut self, k: usize, path: &str) -> Result<When, String> {
        let (part, vin_net) = (self.regulators[k].part, self.regulators[k].vin_net);
        let switch_enable = self.regulators[k].switch_enable;
        let vin = self.rail(vin_net)?;
        let mut open = vin.open.clone();
        // It starts once its input is rising and its enable is high.
        let (mut start, mut start_hi) = (vin.start, vin.hi);
        let mut why = format!("`{path}` starts when its input is up");
        let en = switch_enable.or_else(|| match self.model.member(part, "en") {
            Some(Member::Inst(en)) if self.model.instance(en).kind == InstKind::Pin => Some(en),
            _ => None,
        });
        let joined = |en: InstId| {
            self.net_of
                .get(&en)
                .filter(|n| self.model.nets()[**n].terminals.iter().any(|t| *t != en))
                .copied()
        };
        if let Some(en) = switch_enable
            && joined(en).is_none()
        {
            // A load switch's enable that nothing joins holds it off (`std::parts::switch`).
            return Err(format!(
                "the enable `{}` of the load switch `{path}` is joined to nothing, so it never turns on",
                self.model.path_of(en)
            ));
        }
        if let Some(en) = en
            && let Some(en_net) = joined(en)
        {
            let threshold = self.threshold_of(en);
            let e = self.pin(en_net, en, threshold)?;
            start = start.max(e.lo);
            start_hi = start_hi.max(e.hi);
            open = open.or(e.open);
            why = format!(
                "`{path}` starts when its input is up and `{}` is high",
                self.model.path_of(en)
            );
        }
        if switch_enable.is_some() {
            // The turn-on time of a load switch is not modelled: it adds nothing to the
            // earliest time and leaves the latest one not known.
            return Ok(When {
                start,
                lo: start,
                hi: f64::INFINITY,
                open: open.or(Some(format!(
                    "the turn-on time of the load switch `{path}` is not modelled"
                ))),
                how: format!("{why}, then turns on in a time that is not modelled"),
            });
        }
        let delay = self.range(part, "enable_delay");
        let soft = self.range(part, "soft_start_time");
        for (name, r) in [("enable_delay", &delay), ("soft_start_time", &soft)] {
            if r.is_none() && open.is_none() {
                open = Some(format!("`{path}` states no `.{name}`"));
            }
        }
        let (d_lo, d_hi) = delay.unwrap_or((0.0, f64::INFINITY));
        let (s_lo, s_hi) = soft.unwrap_or((0.0, f64::INFINITY));
        Ok(When {
            start: start + d_lo,
            lo: start + d_lo + s_lo,
            hi: start_hi + d_hi + s_hi,
            open,
            how: format!(
                "{why}, then waits {} and rises for {}",
                span(d_lo, d_hi),
                span(s_lo, s_hi)
            ),
        })
    }

    /// When `pin`, on `net`, goes high: crossing `threshold` (of its part) as the net rises.
    fn pin(
        &mut self,
        net: usize,
        pin: InstId,
        threshold: Option<Quantity>,
    ) -> Result<When, String> {
        let pin_path = self.model.path_of(pin);
        if self.ground[net] {
            return Err(format!("`{pin_path}` is on ground, which never goes high"));
        }
        let g = self.group[net];
        if self.regulators.iter().any(|r| self.group[r.out_net] == g)
            || self.in_group(net).any(|k| self.supply[k].is_some())
        {
            return Ok(self.rail(net)?.crossing());
        }
        let model = self.model;
        let mut pullups: Vec<(InstId, usize)> = Vec::new();
        let mut pulldowns: Vec<InstId> = Vec::new();
        let mut caps: Vec<InstId> = Vec::new();
        for &t in &model.nets()[net].terminals {
            if t == pin {
                continue;
            }
            let Some(part) = model.owner_of(t) else {
                continue;
            };
            let standard = &model.instance(part).standard;
            let far = |a: InstId, b: InstId| {
                let other = if a == t { b } else { a };
                self.net_of.get(&other).copied()
            };
            if standard.iter().any(|s| s == "Resistor")
                && let Some(dc) = model.instance(part).dc.first()
                && dc.pins.len() == 2
            {
                match far(dc.pins[0], dc.pins[1]) {
                    Some(n) if self.ground[n] => pulldowns.push(part),
                    Some(n) => pullups.push((part, n)),
                    None => {}
                }
                continue;
            }
            if standard
                .iter()
                .any(|s| s == "Capacitor" || s == "CapacitorBank")
                && let Some((_, a, b)) = model.bias_of(part)
            {
                match far(a, b) {
                    Some(n) if self.ground[n] => caps.push(part),
                    _ => {
                        return Err(format!(
                            "`{}` joins `{pin_path}` to a net that is not ground, which is not modelled",
                            model.path_of(part)
                        ));
                    }
                }
                continue;
            }
            let kind = model
                .instance(part)
                .tables
                .iter()
                .find_map(|table| table.row(t))
                .map(|row| row.kind);
            if matches!(
                kind,
                Some(
                    PinKind::Output
                        | PinKind::Bidirectional
                        | PinKind::TriState
                        | PinKind::OpenDrain
                )
            ) {
                return Err(format!(
                    "`{pin_path}` is driven by `{}`, and when it goes high is not known",
                    model.path_of(t)
                ));
            }
        }
        let (resistor, from) = match pullups.as_slice() {
            [] => {
                return Err(format!(
                    "nothing pulls `{pin_path}` up: join it to a rail, or through a resistor"
                ));
            }
            [one] => *one,
            _ => {
                return Err(format!(
                    "`{pin_path}` is pulled up from more than one net, which is not modelled"
                ));
            }
        };
        let supply = self.rail(from)?;
        let voltage = self
            .in_group(from)
            .find_map(|k| self.supply[k])
            .and_then(|rail| model.rail_of(rail))
            .and_then(|(v, _, _)| quantity(model, v))
            .ok_or_else(|| format!("the voltage of `{}` is not known", self.name(from)))?;
        let value = |part: InstId, name: &str| -> Result<(f64, f64), String> {
            known(model, part, name)
                .map(|q| (q.lo(), q.hi()))
                .ok_or_else(|| format!("`{}.{name}` has no value", model.path_of(part)))
        };
        let top = value(resistor, "resistance")?;
        // The resistors to ground in parallel, as their least and most.
        let bottom = if pulldowns.is_empty() {
            None
        } else {
            let mut g = (0.0, 0.0);
            for &r in &pulldowns {
                let (lo, hi) = value(r, "resistance")?;
                g = (g.0 + 1.0 / hi, g.1 + 1.0 / lo);
            }
            Some((1.0 / g.0, 1.0 / g.1))
        };
        // The Thevenin voltage and resistance, each at its least and most.
        let (v_lo, v_hi, r_lo, r_hi) = match bottom {
            None => (voltage.lo(), voltage.hi(), top.0, top.1),
            Some((b_lo, b_hi)) => (
                voltage.lo() * b_lo / (top.1 + b_lo),
                voltage.hi() * b_hi / (top.0 + b_hi),
                top.0 * b_lo / (top.0 + b_lo),
                top.1 * b_hi / (top.1 + b_hi),
            ),
        };
        let mut c = (0.0, 0.0);
        for &cap in &caps {
            let q = known(model, cap, "effective_capacitance")
                .or_else(|| known(model, cap, "capacitance"))
                .ok_or_else(|| format!("`{}.capacitance` has no value", model.path_of(cap)))?;
            c = (c.0 + q.lo().max(0.0), c.1 + q.hi().max(0.0));
        }
        let rc = if caps.is_empty() {
            format!("`{pin_path}` is pulled up by `{}`", model.path_of(resistor))
        } else {
            format!(
                "`{pin_path}` is pulled up by `{}` and held by {} to ground, a time constant of {}",
                model.path_of(resistor),
                show_c(c.0, c.1),
                span(r_lo * c.0, r_hi * c.1)
            )
        };
        let charge = |tau: f64, v: f64, th: f64| tau * (v / (v - th)).ln();
        let (lo, hi, open) = match threshold {
            Some(th) => {
                if v_hi <= th.lo() {
                    return Err(format!(
                        "`{pin_path}` is pulled to {} at the most, which never reaches its threshold of {}",
                        Quantity::point(Dimension::VOLTAGE, v_hi)
                            .map_or(String::new(), |q| q.to_string()),
                        th
                    ));
                }
                let lo = supply.start + charge(r_lo * c.0, v_hi, th.lo());
                if v_lo > th.hi() {
                    (
                        lo,
                        supply.hi + charge(r_hi * c.1, v_lo, th.hi()),
                        supply.open.clone(),
                    )
                } else {
                    (
                        lo,
                        f64::INFINITY,
                        Some(format!(
                            "`{pin_path}` is pulled to {} at the least, which may not reach its threshold of {}",
                            Quantity::point(Dimension::VOLTAGE, v_lo)
                                .map_or(String::new(), |q| q.to_string()),
                            th
                        )),
                    )
                }
            }
            None if caps.is_empty() => (supply.start, supply.hi, supply.open.clone()),
            None => (
                supply.start,
                f64::INFINITY,
                Some(format!(
                    "the part of `{pin_path}` states no `.enable_threshold`, so the time the RC takes to reach it is not known"
                )),
            ),
        };
        Ok(When {
            start: lo,
            lo,
            hi,
            open,
            how: rc,
        })
    }

    /// The threshold of the part a pin belongs to (`.enable_threshold`), or of the first part on
    /// its net that states one, for a pin of a module's port.
    fn threshold_of(&self, pin: InstId) -> Option<Quantity> {
        let model = self.model;
        let of = |p: InstId| {
            model
                .owner_of(p)
                .filter(|o| model.instance(*o).kind == InstKind::Part)
                .and_then(|o| known(model, o, "enable_threshold"))
        };
        of(pin).or_else(|| {
            let net = self.net_of.get(&pin)?;
            model.nets()[*net].terminals.iter().find_map(|t| of(*t))
        })
    }

    /// A time member of a part as its least and most, or `None` when it has no value.
    fn range(&self, part: InstId, name: &str) -> Option<(f64, f64)> {
        known(self.model, part, name)
            .filter(|q| q.dim() == Dimension::TIME)
            .map(|q| (q.lo().max(0.0), q.hi()))
    }
}

fn quantity(model: &Model, field: netsmith_check::FieldId) -> Option<Quantity> {
    match &model.field(field).domain {
        Domain::Quantity(q) => Some(*q),
        _ => None,
    }
}

/// The value of a member of an instance, when it is a quantity that has one.
fn known(model: &Model, inst: InstId, name: &str) -> Option<Quantity> {
    let Some(Member::Field(f)) = model.member(inst, name) else {
        return None;
    };
    quantity(model, f)
}

fn show(seconds: f64) -> String {
    match Quantity::point(Dimension::TIME, seconds) {
        Ok(q) => q.to_string(),
        Err(_) => format!("{seconds} s"),
    }
}

/// A range of times, `1 ms`, `1 ms to 2 ms` or `0 s at the least` when its top is not known.
fn span(lo: f64, hi: f64) -> String {
    if !hi.is_finite() {
        format!("{} or more", show(lo))
    } else if (hi - lo).abs() <= 1e-12 * hi.abs().max(1e-12) {
        show(lo)
    } else {
        format!("{} to {}", show(lo), show(hi))
    }
}

fn show_c(lo: f64, hi: f64) -> String {
    let one = |v: f64| {
        Quantity::point(Dimension::CAPACITANCE, v).map_or(format!("{v} F"), |q| q.to_string())
    };
    if (hi - lo).abs() <= 1e-12 * hi.abs().max(1e-18) {
        one(lo)
    } else {
        format!("{} to {}", one(lo), one(hi))
    }
}
