//! Guidelines: what a design does against advice that a person decides about, as opposed to
//! a limit of the fabricator (ADR 0036, ADR 0030).
//!
//! The one guideline so far is the width of a track against the current it carries,
//! from IPC-2221. A violation is a [`Guideline`] finding. It warns by default; a project
//! that sets `guidelines = "error"` in `netsmith.toml`, or a run with `--strict`, refuses
//! to build on one.
//!
//! The same module sizes the tracks: a net whose current the solver has worked out and
//! that no rail gives a `.min_track` is drawn at the width IPC-2221 needs for that current
//! (ADR 0036), and a net whose current is not known is sized for the part that is known and named in a note.

use netsmith_check::{Domain, InstId, InstKind, Member, Model};
use netsmith_core::{Features, OUNCE_OF_COPPER, Process};

/// The temperature rise the check designs to when the board states none, in kelvin. A
/// default of this tool's and not a published figure (ADR 0036).
pub const DEFAULT_TEMPERATURE_RISE: f64 = 10.0;

/// One mil, in mm.
const MIL: f64 = 0.0254;

// The formula and its constants (ADR 0036, ADR 0036).
//
// IPC-2221B, "Generic Standard on Printed Board Design", section 6.2 "Conductor current
// carrying capacity" (the same fit is in IPC-2221A and, as the chart IPC-2221's Figure 6-4,
// in IPC-D-275): `I = k * dT^b * A^c`, with
//   I   the current in amperes,
//   dT  the temperature rise above ambient in degrees Celsius (= kelvin),
//   A   the conductor's cross-section in square mils (width times copper thickness),
//   b = 0.44, c = 0.725,
//   k = 0.048 for a conductor on an outer layer (in air) and 0.024 on an inner layer.
// The standard is paywalled and was not read here; the formula, the constants and their
// meaning were taken from several agreeing published derivations (Altium's IPC-2221
// calculator page, the `pcb-toolkit` crate's `current.rs`, circuit.rocks), and checked by
// hand in `tests/guidelines.rs`. It is an empirical fit to old measurements of tracks in
// air, and the calculators say it holds only inside the ranges of those measurements (which
// this code did not verify against the standard): outside them the result is extrapolated.
//
// Assumed, not from the standard, and PROVISIONAL (ADR 0036): the copper weight is the
// process's `stackup.copper_weight` (1 oz, 0.035 mm, for JLCPCB's two-layer outer copper),
// and the rise is the board's `.process.max_temperature_rise` or, when it states none,
// [`DEFAULT_TEMPERATURE_RISE`]. Tracks are on outer layers, so `K_EXTERNAL`.

/// The IPC-2221 constant `k` for a track on an outer layer, in air.
const K_EXTERNAL: f64 = 0.048;
/// The IPC-2221 constant `k` for a track on an inner layer.
const K_INTERNAL: f64 = 0.024;

/// Where a track is: outer layers shed heat to the air, inner layers do not, so IPC-2221
/// allows an inner track half the current.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    External,
    Internal,
}

impl Layer {
    fn k(self) -> f64 {
        match self {
            Self::External => K_EXTERNAL,
            Self::Internal => K_INTERNAL,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Self::External => "outer",
            Self::Internal => "inner",
        }
    }
}

/// The current a track of `width` mm in copper `copper` mm thick carries for a temperature
/// rise of `rise` kelvin, in amperes: IPC-2221's `I = k * dT^0.44 * A^0.725`, with the
/// cross-section `A` in square mils.
pub fn ipc2221_current(width: f64, copper: f64, rise: f64, layer: Layer) -> f64 {
    let area = (width / MIL) * (copper / MIL);
    layer.k() * rise.powf(0.44) * area.powf(0.725)
}

/// The narrowest track in mm, in copper `copper` mm thick, that carries `current` amperes
/// for a rise of `rise` kelvin: the formula of [`ipc2221_current`] solved for the area.
pub fn ipc2221_width(current: f64, copper: f64, rise: f64, layer: Layer) -> f64 {
    let area = (current / (layer.k() * rise.powf(0.44))).powf(1.0 / 0.725);
    area / (copper / MIL) * MIL
}

/// What a design does against a guideline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guideline {
    /// The guideline: `IPC-2221`.
    pub rule: &'static str,
    /// What is wrong, with the numbers.
    pub message: String,
    /// What the finding is about, if it is one thing in the source: the rail or the part
    /// that carries the current.
    pub at: Option<InstId>,
}

/// A length in mm for a message: three decimals at most (a copper of 0.035 mm), no trailing
/// zeros.
fn mm(x: f64) -> String {
    let text = format!("{x:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A current for a message, to three significant digits: `500 mA`, `2 A`, `745 mA`.
fn amps(x: f64) -> String {
    let (value, unit) = if x < 1.0 { (x * 1e3, "mA") } else { (x, "A") };
    let decimals = if value == 0.0 {
        0
    } else {
        (2 - value.abs().log10().floor() as i32).max(0) as usize
    };
    let text = format!("{value:.decimals$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        &text
    };
    format!("{text} {unit}")
}

/// A current that something carries between two pins of a net.
struct Carrier {
    /// What carries it: `rail output`, `part r1`.
    who: String,
    inst: InstId,
    /// The part, module or board that holds it: a rail's owner, or the part itself. What
    /// holds a current decides whether a module's port is left out (see [`plan_tracks`]).
    holder: InstId,
    /// Whether it is a rail (and not a part passing a current).
    is_rail: bool,
    /// The pin the current leaves its net by when it is positive: a rail's `pos` (it draws
    /// from the net there), the first end of a part's `bridge`.
    from: InstId,
    /// The pin it comes back into its net by: a rail's `neg`, the second end of a bridge.
    to: Option<InstId>,
    /// The range of the signed current in amperes, or `None` while the solver has not worked
    /// the current out.
    range: Option<(f64, f64)>,
}

impl Carrier {
    fn pins(&self) -> impl Iterator<Item = InstId> {
        std::iter::once(self.from).chain(self.to)
    }
}

/// The range of a current field, signed, in amperes.
fn signed_range(model: &Model, member: Option<Member>) -> Option<(f64, f64)> {
    let Some(Member::Field(f)) = member else {
        return None;
    };
    let Domain::Quantity(q) = &model.field(f).domain else {
        return None;
    };
    let (lo, hi) = (q.lo(), q.hi());
    if !lo.is_finite() || !hi.is_finite() {
        return None;
    }
    // Adding zero turns a negative zero into a zero.
    Some((lo + 0.0, hi + 0.0))
}

/// What flows through a net by Kirchhoff's current law: what the carriers on it take out of
/// it and what they put into it, each at its largest, every one at the same time. A term is
/// a carrier, its largest amount, and whether that is the carrier drawing (a load) and not
/// supplying.
#[derive(Default)]
struct Flow<'a> {
    taken: Vec<(&'a Carrier, f64, bool)>,
    given: Vec<(&'a Carrier, f64, bool)>,
}

impl<'a> Flow<'a> {
    /// A carrier's current `lo..hi` leaves the net (when positive) or enters it (when
    /// negative): the carrier's `from` pin is on the net.
    fn leaves(&mut self, c: &'a Carrier, (lo, hi): (f64, f64)) {
        if hi > 0.0 {
            self.taken.push((c, hi, true));
        }
        if lo < 0.0 {
            self.given.push((c, -lo, false));
        }
    }

    /// The carrier's `to` pin is on the net: what is positive enters it.
    fn enters(&mut self, c: &'a Carrier, (lo, hi): (f64, f64)) {
        if hi > 0.0 {
            self.given.push((c, hi, true));
        }
        if lo < 0.0 {
            self.taken.push((c, -lo, false));
        }
    }

    /// The larger of the two sums and the terms of that side, largest first. On a net whose
    /// every current is modelled the sides are equal (the law), and the side of the loads is
    /// the one named; where a stated supply is a budget wider than its loads, or a pin is
    /// not modelled, the larger is the safer.
    fn worst(&self) -> (f64, Vec<(&'a Carrier, f64)>) {
        let sum = |v: &[(&Carrier, f64, bool)]| v.iter().map(|t| t.1).sum::<f64>();
        let loads =
            |v: &[(&Carrier, f64, bool)]| v.iter().filter(|t| t.2).map(|t| t.1).sum::<f64>();
        let (t, g) = (sum(&self.taken), sum(&self.given));
        let side = if (g - t).abs() <= 1e-9 * t.max(g) {
            if loads(&self.given) > loads(&self.taken) {
                &self.given
            } else {
                &self.taken
            }
        } else if g > t {
            &self.given
        } else {
            &self.taken
        };
        let mut terms: Vec<(&Carrier, f64)> = side.iter().map(|t| (t.0, t.1)).collect();
        terms.sort_by(|a, b| b.1.total_cmp(&a.1));
        (sum(side), terms)
    }
}

/// What the track check works out for a board (ADR 0036, ADR 0036).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackPlan {
    /// The width in mm that IPC-2221 needs, by net, for the current that is known on each net
    /// that no rail gave a `.min_track` and that needs more than the process's least track.
    /// The router draws the net at this width. Sorted by net name.
    pub widths: Vec<(String, f64)>,
    /// The nets drawn narrower than IPC-2221 needs for the current that is known: a rail's
    /// `.min_track` that is too narrow.
    pub findings: Vec<Guideline>,
    /// The nets with a current that is not known, whose tracks are a lower bound, by name.
    pub notes: Vec<String>,
}

/// The tracks of every net against the current the net carries, by IPC-2221 (ADR 0036).
///
/// The current a net carries comes from Kirchhoff's current law (ADR 0036, amendment of
/// 2026-11-04). The things that put a current into a net or take one out of it are the rails
/// (a load takes its draw out of the net its `pos` is on and puts it back into the net its
/// `neg` is on; a supply is the reverse) and the parts that pass a current (it leaves the net
/// of the first end of their `bridge` and enters the net of the second). The net carries the
/// sum of what is taken out, with every load on at the same time, or the sum of what is put
/// in, whichever is larger: a ground net shared by a 2 A and a 1 A load carries 3 A. The
/// currents are the solver's, so a regulator's input is what its model says it draws (the
/// output current for a linear one, the power balance with the efficiency for a switching
/// one), never its output current.
///
/// A module's port stands for what is inside the module (ADR 0007), so each current is
/// counted once: at the port when nothing inside the module is on the net, otherwise inside.
/// Its tracks are as wide as the rails on
/// it ask (`.min_track`, the widest bottom, in `features.net_widths`) and never narrower than
/// the process's least. A net no rail states a width for, whose every current is known, is
/// given the width IPC-2221 needs (ADR 0036) in [`TrackPlan::widths`]. Where some current on
/// the net is not known, the known ones are a lower bound (loads only add up) and the net is
/// given the width they need, never narrower than the process's least, and is named in
/// [`TrackPlan::notes`] (ADR 0036, amendment 3). The router draws on the outer layers only,
/// so the outer-layer constant and the outer copper are used. A current that is not known but
/// has an upper bound (`bounds`, Ohm's law over the voltages at the ends of a resistor) leaves
/// the width as it is and says nothing more when the width drawn already carries the known
/// current and every bound together, at both signs (ADR 0036, amendment 4). A net that is a plane or an
/// outer pour is not checked: its copper is an area and not a track.
pub fn plan_tracks(
    model: &Model,
    process: &Process,
    features: &Features,
    bounds: &[netsmith_solve::CurrentBound],
) -> TrackPlan {
    let mut plan = TrackPlan::default();
    let copper = process.stackup.copper_weight * OUNCE_OF_COPPER;
    if copper <= 0.0 {
        return plan;
    }
    let layer = Layer::External;
    let (rise, rise_is_default) = match process.max_temperature_rise {
        Some(r) => (r, false),
        None => (DEFAULT_TEMPERATURE_RISE, true),
    };
    if rise <= 0.0 {
        return plan;
    }
    let planes: Vec<&str> = process
        .stackup
        .inner
        .iter()
        .flat_map(|i| i.planes.iter().map(String::as_str))
        .chain(features.pours.iter().map(|p| p.net.as_str()))
        .collect();

    // What can carry a current: a rail (an interface that draws) and a part that passes one.
    let mut carriers: Vec<Carrier> = Vec::new();
    for (id, inst) in model.instances() {
        match inst.kind {
            InstKind::Interface => {
                let Some((_, pos)) = model.draw_of(id) else {
                    continue;
                };
                let Some(holder) = model.owner_of(id) else {
                    continue;
                };
                let neg = match model.member(id, "neg") {
                    Some(Member::Inst(neg)) => Some(neg),
                    _ => None,
                };
                carriers.push(Carrier {
                    who: format!("rail `{}`", model.path_of(id)),
                    inst: id,
                    holder,
                    is_rail: true,
                    from: pos,
                    to: neg,
                    range: signed_range(model, model.member(id, "current")),
                });
            }
            InstKind::Part => {
                let (Some(_), Some((a, b))) = (model.through_of(id), inst.bridge) else {
                    continue;
                };
                carriers.push(Carrier {
                    who: format!("part `{}`", model.path_of(id)),
                    inst: id,
                    holder: id,
                    is_rail: false,
                    from: a,
                    to: Some(b),
                    range: signed_range(model, model.member(id, "current")),
                });
            }
            _ => {}
        }
    }

    // The nets a supply reaches: those with a rail's pin, and, through the parts that pass
    // a current, the nets on the other side of them. Only these are worth a note when their
    // current is not known; a signal net carries no power to size.
    let touching = |c: &Carrier, net: &netsmith_check::Net| c.pins().any(|p| net.pins.contains(&p));
    let mut supplied: Vec<bool> = model
        .nets()
        .iter()
        .map(|net| carriers.iter().any(|c| c.is_rail && touching(c, net)))
        .collect();
    loop {
        let mut grew = false;
        for c in carriers.iter().filter(|c| !c.is_rail) {
            let touched: Vec<usize> = (0..supplied.len())
                .filter(|&i| touching(c, &model.nets()[i]))
                .collect();
            if touched.iter().any(|&i| supplied[i]) {
                for i in touched {
                    grew |= !supplied[i];
                    supplied[i] = true;
                }
            }
        }
        if !grew {
            break;
        }
    }

    for (index, net) in model.nets().iter().enumerate() {
        if net.terminals.len() < 2 || planes.contains(&net.name.as_str()) {
            continue;
        }
        let on_net: Vec<&Carrier> = carriers.iter().filter(|c| touching(c, net)).collect();
        // A module's port stands for what is inside the module (ADR 0007) and the rails and
        // parts inside it are the same current seen closer up. Where something inside the
        // module is on the net, the port is left out and the inside counts; a module with
        // nothing modelled inside on the net keeps its port, which is all that is known of it.
        let inside = |module: InstId, d: &Carrier| {
            let mut up = model.instance(d.holder).parent;
            while let Some(p) = up {
                if p == module {
                    return d.holder != module;
                }
                up = model.instance(p).parent;
            }
            false
        };
        let covered = |c: &Carrier| {
            c.is_rail
                && model.instance(c.holder).kind == InstKind::Module
                && on_net.iter().any(|d| inside(c.holder, d))
        };
        let counted: Vec<&Carrier> = on_net.iter().copied().filter(|c| !covered(c)).collect();

        let mut flow = Flow::default();
        let mut unknown: Vec<&str> = Vec::new();
        // The sum of the upper bounds of the unknown currents, or `None` when one of them has
        // none (a rail, a part that Ohm's law does not bound).
        let mut bounded: Option<f64> = Some(0.0);
        for c in &counted {
            let at_from = net.pins.contains(&c.from);
            let at_to = c.to.is_some_and(|t| net.pins.contains(&t));
            // Both ends on the net: what leaves comes back, nothing flows through it.
            if at_from && at_to {
                continue;
            }
            let Some(range) = c.range else {
                unknown.push(c.who.as_str());
                let bound = bounds.iter().find(|b| !c.is_rail && b.part == c.inst);
                bounded = bounded.zip(bound).map(|(sum, b)| sum + b.current);
                continue;
            };
            if at_from {
                flow.leaves(c, range);
            } else {
                flow.enters(c, range);
            }
        }
        let stated = features
            .net_widths
            .iter()
            .find(|(n, _)| *n == net.name)
            .map(|(_, w)| *w);
        let (total, terms) = flow.worst();
        // The currents that are known are a lower bound on the net's current: loads only add
        // up, so a current that is not known can make the sum larger and never smaller. A net
        // is drawn at least as wide as the known part needs, which is no guess (ADR 0036,
        // amendment 3).
        let needs = if total > 0.0 {
            ipc2221_width(total, copper, rise, layer)
        } else {
            0.0
        };
        // Rounded up to the 0.01 mm that a width is written in, so that asking for what the
        // message says passes.
        let asks = (needs * 100.0 - 1e-9).ceil() / 100.0;
        let sized = stated.is_none() && asks > process.min_track;
        // A current that is not known may be larger than the ones that are: say so, with the
        // width the net got. A rail that states a `.min_track` has decided the width.
        // Every current that is not known has an upper bound, and the width drawn carries the
        // known current and all of them at once, in whichever direction they flow: the
        // tracks are no lower bound, they are enough.
        let width = if sized { asks } else { process.min_track };
        let enough = bounded.is_some_and(|more| {
            ipc2221_width(total + more, copper, rise, layer) <= width * (1.0 + 1e-9)
        });
        if !unknown.is_empty() && stated.is_none() && supplied[index] && !enough {
            let known = (total > 0.0).then_some(total);
            plan.notes
                .push(unknown_note(&net.name, &unknown, width, known, sized));
        }
        let Some(&(worst, _)) = terms.first() else {
            continue;
        };
        if total <= 0.0 {
            continue;
        }
        if sized {
            plan.widths.push((net.name.clone(), asks));
            continue;
        }
        // Only a width the board stated can be below what the net needs here: a net no rail
        // states a width for is at least as wide as the known current needs, or at the
        // process's least, which that current does not exceed.
        let Some(stated) = stated else {
            continue;
        };
        let width = stated.max(process.min_track);
        let carries = ipc2221_current(width, copper, rise, layer);
        if width >= needs {
            continue;
        }
        let what = match terms.as_slice() {
            [(c, hi)] => match c.range {
                Some((lo, _)) if lo >= 0.0 && lo != *hi => {
                    format!("{}: {} to {}", c.who, amps(lo), amps(*hi))
                }
                _ => format!("{}: {}", c.who, amps(*hi)),
            },
            many => {
                let shown: Vec<String> = many
                    .iter()
                    .take(4)
                    .map(|(c, hi)| format!("{} {}", c.who, amps(*hi)))
                    .collect();
                let more = match many.len() - shown.len() {
                    0 => String::new(),
                    n => format!(" and {n} more"),
                };
                format!(
                    "the sum of the currents on it with every load on at once: {}{more}",
                    shown.join(" + ")
                )
            }
        };
        let (carries_word, what) = if unknown.is_empty() {
            ("up to", what)
        } else {
            (
                "at least",
                format!(
                    "{what}; the current of {} is not known, so it may be more",
                    unknown.join(", ")
                ),
            )
        };
        plan.findings.push(Guideline {
            rule: "IPC-2221",
            message: format!(
                "net `{}` carries {carries_word} {} ({what}) but its tracks are {} mm wide (a rail's `.min_track`), in {} oz ({} mm) {} copper; at a temperature rise of {} K{} that width carries {} and IPC-2221 needs {} mm: give a rail on the net `.min_track &= {}mm..1m`, or allow a higher `.process.max_temperature_rise`",
                net.name,
                amps(total),
                mm(width),
                mm(process.stackup.copper_weight),
                mm(copper),
                layer.word(),
                mm(rise),
                if rise_is_default { " (the default)" } else { "" },
                amps(carries),
                mm(asks),
                mm(asks),
            ),
            at: Some(worst.inst),
        });
    }
    plan.widths.sort_by(|a, b| a.0.cmp(&b.0));
    plan
}

/// The note for a net with a current that is not known. The tracks are a lower bound: they
/// are drawn for the part of the current that is known (`known`, in amperes, when there is
/// any) and are at least the process's least `least`; `sized` says the known current is what
/// decided `width`. The carriers come first so that the editor, which puts a note at the first
/// path it finds in the text, puts it at one of them and not at a generated net name.
fn unknown_note(
    net: &str,
    unknown: &[&str],
    width: f64,
    known: Option<f64>,
    sized: bool,
) -> String {
    let bound = match (known, sized) {
        (Some(known), true) => format!(
            "{} mm, a lower bound: sized for the {} that is known, not for the whole current",
            mm(width),
            amps(known)
        ),
        (Some(known), false) => format!(
            "the process's least, {} mm, a lower bound: it carries the {} that is known, not the whole current",
            mm(width),
            amps(known)
        ),
        (None, _) => format!(
            "the process's least, {} mm, a lower bound: no current on the net is known",
            mm(width)
        ),
    };
    format!(
        "the current of {} is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `{net}` are {bound}; state that current, or give a rail on the net a `.min_track`",
        unknown.join(", "),
    )
}
