//! The hold-up of a rail by a storage element (issue #1255), `std::checks::HoldUp`, checked once
//! the parts are picked.
//!
//! A board states a `HoldUp` with its `.storage` joined to the positive terminal of the storage
//! (a supercapacitor, a capacitor bank) and its `.rail` to the `pos` of the rail it holds up, and
//! the load: `.average_power`, `.peak_current` and `.duration`. The check:
//!
//! 1. **The storage**: every part with a `bias` between the storage's net and the rail's ground
//!    that has a capacitance (`total_effective_capacitance`, else `effective_capacitance`), with
//!    its ESR (`dc_esr`, else `esr`; a bank's is one capacitor's over the count) and its
//!    `leakage_current`. Parts with an ESR share the peak, their ESRs in parallel; a part that
//!    states none is left out of the parallel, which can only raise it. The storage starts at the
//!    least of its `.bias`. The rail is the one whose `pos` is on the net of `.rail` nearest the
//!    board (a module's own rails on the net, a divider's, come after), with the highest minimum.
//! 2. **The chain**: the shortest walk from the storage's net to the rail's through the `dc`
//!    elements of the parts (a diode anode to cathode, a resistor either way, a load switch, a
//!    linear and a switching regulator input to output), never through the ground.
//! 3. **The cut-off**: walked back from the rail's minimum (its `.voltage_floor`, else the least
//!    of its `.voltage`) at the peak (or the average, if it is larger): a resistor or a load switch
//!    adds the current times its resistance, a diode its forward voltage at the current, a linear
//!    regulator its `.dropout` at its output current and adds its quiescent current, a switching
//!    regulator asks the larger of its `.min_input_voltage` and its output's need plus the current
//!    through `.high_side_resistance` (or over `1 - .min_off_time * .fsw`), and draws its output's
//!    power (the current times the voltage its output needs) over the least of `.efficiency` at
//!    that input. The storage must keep that, plus its current times
//!    its ESR.
//! 4. **Energy**: what the storage gives between its starting voltage `V0` and the cut-off
//!    `Vcut`, `C * (V0^2 - Vcut^2) / 2`, against what the chain takes for `.average_power` over
//!    `.duration`: the current at the rail's minimum walked back the same way (a switching stage's
//!    at its least input, the worst case), with the leakage, as a charge `Q` given out between `V0`
//!    and `Vcut`, `Q * (V0 + Vcut) / 2`. The two agree exactly when `Q == C * (V0 - Vcut)`, so a
//!    failure says the capacitance that holds it, `Q / (V0 - Vcut)`.
//!
//! The check is made at the parts' own figures and at each corner of the board (issue #1256):
//! the capacitance and the ESR as a corner reads them, a diode's forward voltage and a regulator's
//! dropout off the row's curve file (issue #1260) at the corner's temperature and the step's
//! current. A row with no curve gives its single figure, the datasheet's maximum, at every
//! corner, as the other checks of the solver read it. A value that is not known (a corner's
//! end-of-life factor, a curve that does not reach the step) leaves that check unjudged, with a
//! note; a hold-up whose pins are on no net, whose rail is no rail or whose storage holds nothing
//! is an error.

use std::collections::{HashMap, VecDeque};

use super::Solver;
use super::corners::{Corner, Miss};
use crate::curve;
use crate::levels::NotEvaluated;
use netsmith_check::{DcKind, Diagnostic, Domain, FieldId, InstId, InstKind, Member};
use netsmith_core::Cause;
use netsmith_quantity::{Dimension, Quantity};

/// The type name of the check in the standard library.
pub(super) const HOLD_UP: &str = "HoldUp";

/// Where the check is judged.
#[derive(Clone, Copy)]
enum At<'c> {
    /// The parts' own figures.
    Figures,
    Corner(&'c Corner),
}

impl At<'_> {
    fn words(&self) -> String {
        match self {
            At::Figures => String::new(),
            At::Corner(c) => format!(" at the corner {}", c.describe()),
        }
    }
}

/// One link of the chain, from the storage towards the rail.
#[derive(Debug, Clone, Copy)]
enum Link {
    /// A `dc resistor`: the current times its resistance.
    Resistor { part: InstId, resistance: FieldId },
    /// A `dc load_switch`: the current times its on-resistance, and its own current.
    Switch {
        part: InstId,
        resistance: FieldId,
        quiescent: Option<FieldId>,
    },
    /// A `dc diode`: its forward voltage at the current.
    Diode { part: InstId, forward: FieldId },
    /// A `dc linear_regulator`: its dropout at the current, and its own current.
    Linear {
        part: InstId,
        quiescent: Option<FieldId>,
    },
    /// A `dc switching_regulator`: its least input, and its output's power over its efficiency.
    Switching {
        part: InstId,
        efficiency: FieldId,
        quiescent: Option<FieldId>,
    },
}

/// Why a value the check needs is not known.
enum Gap {
    /// A field has no value.
    Unknown(FieldId),
    /// A corner cannot read a value (as `Corners` says).
    Corner(Miss),
    /// A curve file does not reach the step.
    Beyond {
        part: InstId,
        file: String,
        why: String,
    },
    /// No part of the storage states an ESR.
    NoEsr(InstId),
    /// A value is missing that another check names (a regulator with no `.dropout`).
    Silent,
}

/// The chain walked back at one current: the voltage the storage must keep at its terminal, the
/// current it gives, and each link's share, for the message.
struct Walk {
    voltage: f64,
    current: f64,
    drops: Vec<String>,
}

/// A part of the storage.
struct Store {
    part: InstId,
    capacitance: FieldId,
    esr: Option<FieldId>,
    leakage: Option<FieldId>,
    bias: FieldId,
}

impl Solver<'_> {
    /// The faults of the hold-ups a design states, and the checks that could not be made.
    pub(super) fn hold_up(&self) -> (Vec<Diagnostic>, Vec<NotEvaluated>) {
        let mut faults = Vec::new();
        let mut notes = Vec::new();
        let nets: HashMap<InstId, usize> = self
            .model
            .nets()
            .iter()
            .enumerate()
            .flat_map(|(i, n)| n.pins.iter().map(move |&p| (p, i)))
            .collect();
        for (hold, inst) in self.model.instances() {
            if inst.kind != InstKind::Interface || inst.type_name != HOLD_UP {
                continue;
            }
            let (source, span) = inst.at();
            let path = self.model.path_of(hold);
            let mut fault = |message: String| {
                faults.push(Diagnostic {
                    source,
                    span,
                    message,
                })
            };
            let pin = |name: &str| match self.model.member(hold, name) {
                Some(Member::Inst(p)) => nets
                    .get(&p)
                    .copied()
                    .filter(|&n| self.model.nets()[n].pins.len() > 1),
                _ => None,
            };
            let (Some(storage_net), Some(rail_net)) = (pin("storage"), pin("rail")) else {
                let which = if pin("storage").is_none() {
                    "storage"
                } else {
                    "rail"
                };
                let example = if which == "storage" {
                    format!(".{path}.storage ~ .sc.p")
                } else {
                    format!(".{path}.rail ~ .out.pos")
                };
                fault(format!(
                    "`{path}.{which}` is joined to nothing: a hold-up is joined to the positive terminal of its storage and to the `pos` of the rail it holds up (`{example}`)"
                ));
                continue;
            };
            // The rail: of the rails whose `pos` is on the net, the one nearest the board (the
            // board's own, not a module's inside it), and of those the highest minimum.
            let mut rail: Option<(InstId, f64, usize, usize)> = None;
            for (id, _) in self.model.instances() {
                let Some((voltage, pos, neg)) = self.model.rail_of(id) else {
                    continue;
                };
                if nets.get(&pos) != Some(&rail_net) || id == hold {
                    continue;
                }
                let Some(v) = self.quantity(voltage) else {
                    continue;
                };
                let floor = match self.model.member(id, super::floors::FLOOR) {
                    Some(Member::Field(f)) => self.quantity(f).map(|q| q.hi()),
                    _ => None,
                };
                let least = floor.unwrap_or(v.lo());
                let Some(&ground) = nets.get(&neg) else {
                    continue;
                };
                let depth = self.model.path_of(id).matches('.').count();
                if rail.as_ref().is_none_or(|r| {
                    depth < r.3 || (depth == r.3 && least > r.1 + 1e-12 * least.abs())
                }) {
                    rail = Some((id, least, ground, depth));
                }
            }
            let Some((rail, least, ground, _)) = rail else {
                fault(format!(
                    "`{path}.rail` is on a net whose voltage no rail states: join it to the `pos` of the rail it holds up, a rail with a `.voltage` (`.{path}.rail ~ .out.pos`)"
                ));
                continue;
            };
            let rail_path = self.model.path_of(rail);
            let stores = self.storage(storage_net, ground, &nets);
            if stores.is_empty() {
                fault(format!(
                    "no part with a capacitance is between the net of `{path}.storage` and the ground of `{rail_path}`: join `{path}.storage` to the positive terminal of the supercapacitor or the bank that holds the rail up"
                ));
                continue;
            }
            let Some(chain) = self.chain(storage_net, rail_net, ground, &nets) else {
                notes.push(NotEvaluated {
                    at: hold,
                    involves: vec![hold],
                    message: format!(
                        "the hold-up `{path}` is not judged: no chain of parts runs from the net of `{path}.storage` to `{rail_path}` through what the check walks (a diode, a resistor, a load switch, a linear or a switching regulator, each by its `dc` statement); a FET that is an ideal diode is not walked"
                    ),
                    cause: Cause::NotStated,
                });
                continue;
            };
            let load = |name: &str| match self.model.member(hold, name) {
                Some(Member::Field(f)) => self.quantity(f).map(|q| q.hi()).ok_or(()),
                _ => Err(()),
            };
            let (power, peak, duration) = match (
                load("average_power"),
                load("peak_current"),
                load("duration"),
            ) {
                (Ok(p), Ok(i), Ok(t)) => (p, i, t),
                (p, i, t) => {
                    let missing: Vec<&str> = [
                        ("average_power", p.is_err()),
                        ("peak_current", i.is_err()),
                        ("duration", t.is_err()),
                    ]
                    .into_iter()
                    .filter_map(|(n, m)| m.then_some(n))
                    .collect();
                    notes.push(NotEvaluated {
                        at: hold,
                        involves: vec![hold],
                        message: format!(
                            "the hold-up `{path}` is not judged: it states no {} (`.{path}.{} &= ...`)",
                            missing
                                .iter()
                                .map(|m| format!("`.{m}`"))
                                .collect::<Vec<_>>()
                                .join(", "),
                            missing[0]
                        ),
                        cause: Cause::NotStated,
                    });
                    continue;
                }
            };
            let board = self.model.board_of(hold);
            let mut places = vec![At::Figures];
            places.extend(
                self.corners
                    .corners
                    .iter()
                    .filter(|c| Some(c.board) == board)
                    .map(At::Corner),
            );
            for at in places {
                let held = self.hold_at(
                    at, &stores, &chain, least, &rail_path, power, peak, duration,
                );
                match held {
                    Ok(None) => {}
                    Ok(Some(why)) => fault(format!(
                        "`{path}` does not hold `{rail_path}` up{}: {why}",
                        at.words()
                    )),
                    Err(gap) => {
                        if let Some(why) = self.gap_words(&gap, at) {
                            notes.push(NotEvaluated {
                                at: hold,
                                involves: vec![hold],
                                message: format!(
                                    "the hold-up `{path}` is not judged{}: {why}",
                                    at.words()
                                ),
                                cause: Cause::NotStated,
                            });
                        }
                    }
                }
            }
        }
        (faults, notes)
    }

    /// The check at one place: `None` when it holds, and why not otherwise.
    #[allow(clippy::too_many_arguments)]
    fn hold_at(
        &self,
        at: At,
        stores: &[Store],
        chain: &[Link],
        least: f64,
        rail: &str,
        power: f64,
        peak: f64,
        duration: f64,
    ) -> Result<Option<String>, Gap> {
        let mut capacitance = 0.0;
        let mut conductance = 0.0;
        let mut leakage = 0.0;
        let mut start = f64::INFINITY;
        for s in stores {
            capacitance += self.capacitance_at(s.capacitance, at)?.lo().max(0.0);
            // A part with no ESR is left out of the parallel, which can only raise it.
            let esr = match s.esr.map(|f| self.esr_at_place(f, s.part, at)) {
                Some(Ok(q)) => Some(q),
                Some(Err(Gap::Unknown(_) | Gap::Corner(Miss::Unknown))) | None => None,
                Some(Err(gap)) => return Err(gap),
            };
            if let Some(esr) = esr {
                let mut r = esr.hi();
                if let Some(count) = self.model.bank_count(s.part) {
                    r /= self.quantity(count).map_or(1.0, |q| q.lo().max(1.0));
                }
                conductance += if r > 0.0 { 1.0 / r } else { f64::INFINITY };
            }
            if let Some(l) = s.leakage {
                leakage += self.quantity(l).ok_or(Gap::Unknown(l))?.hi().max(0.0);
            }
            start = start.min(self.quantity(s.bias).ok_or(Gap::Unknown(s.bias))?.lo());
        }
        let esr = if conductance > 0.0 {
            1.0 / conductance
        } else {
            return Err(Gap::NoEsr(stores[0].part));
        };
        let average = power / least;
        let step = peak.max(average);
        let cut = self.walk(chain, least, step, at)?;
        let cutoff = cut.voltage + cut.current * esr;
        let mut drops = cut.drops;
        drops.push(format!(
            "{} across the storage's ESR of {}",
            v(cut.current * esr),
            show(Dimension::RESISTANCE, esr)
        ));
        let mean = self.walk(chain, least, average, at)?;
        let current = mean.current + leakage;
        let charge = current * duration;
        let headroom = start - cutoff;
        if headroom > 0.0 && charge <= capacitance * headroom * (1.0 + 1e-9) {
            return Ok(None);
        }
        let leak = if leakage > 0.0 {
            format!(" (and leaks {})", a(leakage))
        } else {
            String::new()
        };
        let needs = format!(
            "`{rail}` draws {} for {} ({} at its least {}), so the storage gives the chain {}{leak}",
            show(Dimension::POWER, power),
            show(Dimension::TIME, duration),
            a(average),
            v(least),
            a(mean.current),
        );
        let cut_words = format!(
            "a peak of {} keeps `{rail}` at {} only while the storage is at {} or more ({})",
            a(peak),
            v(least),
            v(cutoff),
            drops.join(", ")
        );
        if headroom <= 0.0 {
            return Ok(Some(format!(
                "{cut_words}, which is not below its starting voltage, {}",
                v(start)
            )));
        }
        Ok(Some(format!(
            "{needs}, {} over the time; the storage ({} of effective capacitance) gives only {} between its starting {} and {}, where {cut_words}: holding it takes {} of effective capacitance",
            joules(charge * (start + cutoff) / 2.0),
            show(Dimension::CAPACITANCE, capacitance),
            joules(capacitance * (start * start - cutoff * cutoff) / 2.0),
            v(start),
            v(cutoff),
            show(Dimension::CAPACITANCE, charge / headroom),
        )))
    }

    /// The chain walked back from the rail at `least` and `current`.
    fn walk(&self, chain: &[Link], least: f64, current: f64, at: At) -> Result<Walk, Gap> {
        let mut voltage = least;
        let mut current = current;
        let mut drops = Vec::new();
        for link in chain.iter().rev() {
            match *link {
                Link::Resistor { part, resistance }
                | Link::Switch {
                    part, resistance, ..
                } => {
                    let r = self.quantity(resistance).ok_or(Gap::Unknown(resistance))?;
                    let drop = current * r.hi();
                    voltage += drop;
                    drops.push(format!("{} across `{}`", v(drop), self.model.path_of(part)));
                    if let Link::Switch {
                        quiescent: Some(q), ..
                    } = *link
                    {
                        current += self.quantity(q).ok_or(Gap::Unknown(q))?.hi().max(0.0);
                    }
                }
                Link::Diode { part, forward } => {
                    let drop = self.curve_hi(part, forward, current, at)?;
                    voltage += drop;
                    drops.push(format!("{} across `{}`", v(drop), self.model.path_of(part)));
                }
                Link::Linear { part, quiescent } => {
                    let Some(Member::Field(dropout)) = self.model.member(part, "dropout") else {
                        return Err(Gap::Silent);
                    };
                    let drop = self.curve_hi(part, dropout, current, at)?;
                    voltage += drop;
                    drops.push(format!(
                        "{} of dropout in `{}`",
                        v(drop),
                        self.model.path_of(part)
                    ));
                    if let Some(q) = quiescent {
                        current += self.quantity(q).ok_or(Gap::Unknown(q))?.hi().max(0.0);
                    }
                }
                Link::Switching {
                    part,
                    efficiency,
                    quiescent,
                } => {
                    let member = |name: &str| match self.model.member(part, name) {
                        Some(Member::Field(f)) => self.quantity(f),
                        _ => None,
                    };
                    let mut input = voltage;
                    if let Some(r) = member("high_side_resistance") {
                        input += current * r.hi();
                    } else if let (Some(off), Some(f)) = (member("min_off_time"), member("fsw")) {
                        let duty = 1.0 - off.hi() * f.hi();
                        if duty > 0.0 {
                            input = voltage / duty;
                        }
                    }
                    if let Some(min) = member("min_input_voltage") {
                        input = input.max(min.hi());
                    }
                    let eta = self.quantity(efficiency).ok_or(Gap::Unknown(efficiency))?;
                    let eta = eta.lo();
                    if eta <= 0.0 || input <= 0.0 {
                        return Err(Gap::Unknown(efficiency));
                    }
                    // The power its output gives: the load's at the rail's minimum, and that
                    // with the drops after it.
                    let out_power = current * voltage;
                    drops.push(format!(
                        "`{}` needs {} at its input and draws {} there at an efficiency of {}",
                        self.model.path_of(part),
                        v(input),
                        a(out_power / (eta * input)),
                        eta
                    ));
                    current = out_power / (eta * input);
                    if let Some(q) = quiescent {
                        current += self.quantity(q).ok_or(Gap::Unknown(q))?.hi().max(0.0);
                    }
                    voltage = input;
                }
            }
        }
        drops.reverse();
        Ok(Walk {
            voltage,
            current,
            drops,
        })
    }

    /// A diode's forward voltage or a regulator's dropout at a current: off the curve the row
    /// gave it, at the corner's temperature (the board's range at the parts' own figures), or
    /// the field's figure where the row has no curve.
    fn curve_hi(&self, part: InstId, f: FieldId, current: f64, at: At) -> Result<f64, Gap> {
        if let Some((c, domain)) = self.corner_curves.get(&f)
            && self.model.field(f).domain == *domain
        {
            let mut ranges = Vec::with_capacity(c.axes.len());
            for (i, (axis, dim)) in c.axes.iter().enumerate() {
                if axis == curve::TEMPERATURE {
                    ranges.push(match at {
                        At::Corner(k) => (k.kelvin, k.kelvin),
                        At::Figures => self.environment_range(part).unwrap_or_else(|| c.span(i)),
                    });
                } else if *dim == Dimension::CURRENT {
                    ranges.push((current, current));
                } else {
                    let path = format!("{}.{axis}", self.model.path_of(part));
                    match self.model.field_at(&path).and_then(|g| self.quantity(g)) {
                        Some(q) if q.dim() == *dim => ranges.push((q.lo(), q.hi())),
                        _ => return Err(Gap::Unknown(f)),
                    }
                }
            }
            return c.over(&ranges).map(|q| q.hi()).map_err(|b| Gap::Beyond {
                part,
                file: c.file.clone(),
                why: b.to_string(),
            });
        }
        self.quantity(f).map(|q| q.hi()).ok_or(Gap::Unknown(f))
    }

    /// A capacitance at a place.
    fn capacitance_at(&self, f: FieldId, at: At) -> Result<Quantity, Gap> {
        match at {
            At::Corner(c) if self.corners.moves(f) => self.corners_value(f, c).map_err(Gap::Corner),
            _ => self.quantity(f).ok_or(Gap::Unknown(f)),
        }
    }

    /// An ESR at a place.
    fn esr_at_place(&self, f: FieldId, part: InstId, at: At) -> Result<Quantity, Gap> {
        match at {
            At::Corner(c) => self.esr_at_corner(f, part, c).map_err(Gap::Corner),
            At::Figures => self.quantity(f).ok_or(Gap::Unknown(f)),
        }
    }

    /// What a note says of a value the check could not read, or `None` for one that only a
    /// value still unknown leaves out (which another note names).
    fn gap_words(&self, gap: &Gap, at: At) -> Option<String> {
        match gap {
            Gap::Silent => None,
            Gap::NoEsr(part) => Some(format!(
                "no part of the storage states its ESR, so the step of the peak across it is not known: state the row's `dc_esr` or `esr` (`.{}.esr &= ...`)",
                self.model.path_of(*part)
            )),
            Gap::Unknown(f) => Some(format!("`{}` has no value", self.model.path_of_field(*f))),
            Gap::Beyond { part, file, why } => Some(format!(
                "the curve file {file} of `{}` does not reach the step: {why}",
                self.model.path_of(*part)
            )),
            Gap::Corner(miss) => {
                let At::Corner(corner) = at else { return None };
                self.miss_words(miss, corner)
            }
        }
    }

    /// The parts of the storage: each with a `bias` between `net` and `ground` and a
    /// capacitance.
    fn storage(&self, net: usize, ground: usize, nets: &HashMap<InstId, usize>) -> Vec<Store> {
        let mut out = Vec::new();
        for (part, _) in self.model.instances() {
            let Some((bias, a, b)) = self.model.bias_of(part) else {
                continue;
            };
            let (na, nb) = (nets.get(&a).copied(), nets.get(&b).copied());
            if !((na == Some(net) && nb == Some(ground)) || (na == Some(ground) && nb == Some(net)))
            {
                continue;
            }
            let field = |name: &str| match self.model.member(part, name) {
                Some(Member::Field(f)) => Some(f),
                _ => None,
            };
            let Some(capacitance) = field("total_effective_capacitance")
                .or_else(|| field("effective_capacitance"))
                .filter(|&f| {
                    self.model.field(f).kind
                        == netsmith_check::FieldKind::Quantity(Dimension::CAPACITANCE)
                })
            else {
                continue;
            };
            out.push(Store {
                part,
                capacitance,
                esr: field("dc_esr").or_else(|| field("esr")),
                leakage: field("leakage_current"),
                bias,
            });
        }
        out
    }

    /// The shortest chain of links from `from` to `to`, never through `ground`.
    fn chain(
        &self,
        from: usize,
        to: usize,
        ground: usize,
        nets: &HashMap<InstId, usize>,
    ) -> Option<Vec<Link>> {
        if from == to {
            return Some(Vec::new());
        }
        let net = |pin: InstId| nets.get(&pin).copied();
        let rail_net = |rail: InstId| net(self.model.rail_of(rail)?.1);
        let value = |e: &netsmith_check::DcElement, i: usize| e.values.get(i).copied();
        let mut edges: Vec<(usize, usize, Link)> = Vec::new();
        for (part, e) in self.model.dc_elements() {
            let quiescent = |required: usize| value(e, required);
            let ends = match e.kind {
                DcKind::Resistor => {
                    let (Some(a), Some(b), Some(r)) = (net(e.pins[0]), net(e.pins[1]), value(e, 0))
                    else {
                        continue;
                    };
                    let link = Link::Resistor {
                        part,
                        resistance: r,
                    };
                    edges.push((b, a, link));
                    Some((a, b, link))
                }
                DcKind::Diode => match (net(e.pins[0]), net(e.pins[1]), value(e, 0)) {
                    (Some(a), Some(k), Some(f)) => Some((a, k, Link::Diode { part, forward: f })),
                    _ => None,
                },
                DcKind::LoadSwitch => match (rail_net(e.pins[0]), rail_net(e.pins[1]), value(e, 0))
                {
                    (Some(i), Some(o), Some(r)) => Some((
                        i,
                        o,
                        Link::Switch {
                            part,
                            resistance: r,
                            quiescent: quiescent(1),
                        },
                    )),
                    _ => None,
                },
                DcKind::LinearRegulator => match (rail_net(e.pins[0]), rail_net(e.pins[1])) {
                    (Some(i), Some(o)) => Some((
                        i,
                        o,
                        Link::Linear {
                            part,
                            quiescent: quiescent(0),
                        },
                    )),
                    _ => None,
                },
                DcKind::SwitchingRegulator => {
                    match (rail_net(e.pins[0]), rail_net(e.pins[1]), value(e, 0)) {
                        (Some(i), Some(o), Some(eff)) => Some((
                            i,
                            o,
                            Link::Switching {
                                part,
                                efficiency: eff,
                                quiescent: quiescent(1),
                            },
                        )),
                        _ => None,
                    }
                }
                DcKind::Nmos | DcKind::Pmos | DcKind::Source => None,
            };
            if let Some(edge) = ends {
                edges.push(edge);
            }
        }
        let mut came: HashMap<usize, (usize, Link)> = HashMap::new();
        let mut queue = VecDeque::from([from]);
        while let Some(n) = queue.pop_front() {
            if n == to {
                break;
            }
            for &(a, b, link) in &edges {
                if a == n && b != ground && b != from && !came.contains_key(&b) {
                    came.insert(b, (a, link));
                    queue.push_back(b);
                }
            }
        }
        let mut links = Vec::new();
        let mut at = to;
        while at != from {
            let &(prev, link) = came.get(&at)?;
            links.push(link);
            at = prev;
        }
        links.reverse();
        Some(links)
    }

    /// A field's value, when it is a quantity.
    fn quantity(&self, f: FieldId) -> Option<Quantity> {
        match &self.model.field(f).domain {
            Domain::Quantity(q) => Some(*q),
            _ => None,
        }
    }
}

fn show(dim: Dimension, value: f64) -> String {
    match Quantity::point(dim, value) {
        Ok(q) => q.to_string(),
        Err(_) => format!("{value}"),
    }
}

fn v(value: f64) -> String {
    show(Dimension::VOLTAGE, value)
}

fn a(value: f64) -> String {
    show(Dimension::CURRENT, value)
}

/// An energy, in joules with a prefix (the quantities have no unit of energy).
fn joules(value: f64) -> String {
    let (scale, unit) = match value.abs() {
        x if x >= 1.0 || x == 0.0 => (1.0, "J"),
        x if x >= 1e-3 => (1e3, "mJ"),
        _ => (1e6, "uJ"),
    };
    let x = value * scale;
    let text = format!("{x:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}{unit}")
}
