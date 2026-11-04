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
//! (ADR 0036), and a net whose current is not known is named in a note instead.

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

/// A current that something carries: its magnitude, as the range it can have.
struct Carrier {
    /// What carries it: `rail output`, `part r1`.
    who: String,
    inst: InstId,
    /// Whether it is a rail (and not a part passing a current).
    is_rail: bool,
    /// The magnitude range, or `None` while the solver has not worked the current out.
    range: Option<(f64, f64)>,
}

/// The magnitude range of a current field, in amperes.
fn magnitude(model: &Model, member: Option<Member>) -> Option<(f64, f64)> {
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
    Some(if lo >= 0.0 {
        (lo + 0.0, hi)
    } else if hi <= 0.0 {
        (-hi + 0.0, -lo)
    } else {
        (0.0, hi.max(-lo))
    })
}

/// What the track check works out for a board (ADR 0036, ADR 0036).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackPlan {
    /// The width in mm that IPC-2221 needs, by net, for each net whose current is known,
    /// that no rail gave a `.min_track` and that needs more than the process's least track.
    /// The router draws the net at this width. Sorted by net name.
    pub widths: Vec<(String, f64)>,
    /// The nets drawn narrower than IPC-2221 needs: a rail's `.min_track` that is too
    /// narrow, or a net whose current is not fully known.
    pub findings: Vec<Guideline>,
    /// The nets that were not sized from their current because it is not known, by name.
    pub notes: Vec<String>,
}

/// The tracks of every net against the current the net carries, by IPC-2221 (ADR 0036).
///
/// A net carries the largest of the currents of the things on it: the rails with a pin on
/// it, and the parts whose bridge ends on it, which is how the nets inside a chain of
/// series parts take the chain's current. Its tracks are as wide as the rails on it ask
/// (`.min_track`, the widest bottom, in `features.net_widths`) and never narrower than the
/// process's least. A net no rail states a width for, whose every current is known, is
/// given the width IPC-2221 needs (ADR 0036) in [`TrackPlan::widths`]; a net where some
/// current on it is not known is not sized (a guess would be as likely too narrow as too
/// wide) and is named in [`TrackPlan::notes`]. The router draws on the outer layers only,
/// so the outer-layer constant and the outer copper are used. A net that is a plane or an
/// outer pour is not checked: its copper is an area and not a track.
pub fn plan_tracks(model: &Model, process: &Process, features: &Features) -> TrackPlan {
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
    let mut carriers: Vec<(Vec<InstId>, Carrier)> = Vec::new();
    for (id, inst) in model.instances() {
        match inst.kind {
            InstKind::Interface => {
                let Some((_, pos)) = model.draw_of(id) else {
                    continue;
                };
                let mut pins = vec![pos];
                if let Some(Member::Inst(neg)) = model.member(id, "neg") {
                    pins.push(neg);
                }
                carriers.push((
                    pins,
                    Carrier {
                        who: format!("rail `{}`", model.path_of(id)),
                        inst: id,
                        is_rail: true,
                        range: magnitude(model, model.member(id, "current")),
                    },
                ));
            }
            InstKind::Part => {
                let (Some(_), Some((a, b))) = (model.through_of(id), inst.bridge) else {
                    continue;
                };
                carriers.push((
                    vec![a, b],
                    Carrier {
                        who: format!("part `{}`", model.path_of(id)),
                        inst: id,
                        is_rail: false,
                        range: magnitude(model, model.member(id, "current")),
                    },
                ));
            }
            _ => {}
        }
    }

    // The nets a supply reaches: those with a rail's pin, and, through the parts that pass
    // a current, the nets on the other side of them. Only these are worth a note when their
    // current is not known; a signal net carries no power to size.
    let mut supplied: Vec<bool> = model
        .nets()
        .iter()
        .map(|net| {
            carriers
                .iter()
                .any(|(pins, c)| c.is_rail && pins.iter().any(|p| net.pins.contains(p)))
        })
        .collect();
    loop {
        let mut grew = false;
        for (pins, _) in carriers.iter().filter(|(_, c)| !c.is_rail) {
            let touched: Vec<usize> = (0..supplied.len())
                .filter(|&i| pins.iter().any(|p| model.nets()[i].pins.contains(p)))
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
        let on_net: Vec<&Carrier> = carriers
            .iter()
            .filter(|(pins, _)| pins.iter().any(|p| net.pins.contains(p)))
            .map(|(_, c)| c)
            .collect();
        // The largest current on the net; the first of equals, so the order is the model's.
        let mut worst: Option<(&Carrier, f64, f64)> = None;
        for c in &on_net {
            if let Some((lo, hi)) = c.range
                && worst.is_none_or(|(_, _, h)| hi > h)
            {
                worst = Some((c, lo, hi));
            }
        }
        let unknown: Vec<&str> = on_net
            .iter()
            .filter(|c| c.range.is_none())
            .map(|c| c.who.as_str())
            .collect();
        let stated = features
            .net_widths
            .iter()
            .find(|(n, _)| *n == net.name)
            .map(|(_, w)| *w);
        let Some((worst, lo, hi)) = worst else {
            if !unknown.is_empty() && stated.is_none() && supplied[index] {
                plan.notes
                    .push(unknown_note(&net.name, &unknown, process.min_track));
            }
            continue;
        };
        // A current that is not known may be larger than the ones that are: do not size
        // the net from the known ones, and say so (the check below still holds the net to
        // what is known).
        if !unknown.is_empty() && stated.is_none() && supplied[index] {
            plan.notes
                .push(unknown_note(&net.name, &unknown, process.min_track));
        }
        if hi <= 0.0 {
            continue;
        }
        let needs = ipc2221_width(hi, copper, rise, layer);
        // Rounded up to the 0.01 mm that a width is written in, so that asking for what the
        // message says passes.
        let asks = (needs * 100.0 - 1e-9).ceil() / 100.0;
        if stated.is_none() && unknown.is_empty() && asks > process.min_track {
            plan.widths.push((net.name.clone(), asks));
            continue;
        }
        let width = stated.map_or(process.min_track, |w| w.max(process.min_track));
        let carries = ipc2221_current(width, copper, rise, layer);
        if width >= needs {
            continue;
        }
        let source = match stated {
            Some(_) => "a rail's `.min_track`",
            None => {
                "the process's least, because no rail on it states a `.min_track` and a current on it is not known"
            }
        };
        let range = if lo == hi {
            amps(hi)
        } else {
            format!("{} to {}", amps(lo), amps(hi))
        };
        plan.findings.push(Guideline {
            rule: "IPC-2221",
            message: format!(
                "net `{}` carries up to {} ({}: {range}) but its tracks are {} mm wide ({source}), in {} oz ({} mm) {} copper; at a temperature rise of {} K{} that width carries {} and IPC-2221 needs {} mm: give a rail on the net `.min_track &= {}mm..1m`, or allow a higher `.process.max_temperature_rise`",
                net.name,
                amps(hi),
                worst.who,
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

/// The note for a net whose tracks were not sized from its current. The carriers come first
/// so that the editor, which puts a note at the first path it finds in the text, puts it at
/// one of them and not at a generated net name.
fn unknown_note(net: &str, unknown: &[&str], least: f64) -> String {
    format!(
        "the current of {} is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `{net}` are not sized from their current and are the process's least, {} mm; state that current, or give a rail on the net a `.min_track`",
        unknown.join(", "),
        mm(least),
    )
}
