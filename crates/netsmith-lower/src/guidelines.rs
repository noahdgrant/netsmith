//! Guidelines: what a design does against advice that a person decides about, as opposed to
//! a limit of the fabricator (ADR 0036, ADR 0030).
//!
//! The one guideline so far is the width of a track against the current it carries,
//! from IPC-2221. A violation is a [`Guideline`] finding. It warns by default; a project
//! that sets `guidelines = "error"` in `netsmith.toml`, or a run with `--strict`, refuses
//! to build on one.

use netsmith_check::{Domain, InstId, InstKind, Member, Model};
use netsmith_core::{Features, OUNCE_OF_COPPER, Process};

/// The temperature rise the check designs to when the board states none, in kelvin. A
/// default of this tool's and not a published figure (ADR 0036).
pub const DEFAULT_TEMPERATURE_RISE: f64 = 10.0;

/// One mil, in mm.
const MIL: f64 = 0.0254;

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
    lo: f64,
    hi: f64,
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

/// The tracks of every net against the current the net carries, by IPC-2221 (ADR 0036).
///
/// A net carries the largest of the currents of the things on it: the rails with a pin on
/// it, and the parts whose bridge ends on it, which is how the nets inside a chain of
/// series parts take the chain's current. Its tracks are as wide as the rails on it ask
/// (`.min_track`, the widest bottom) and never narrower than the process's least. The
/// router draws on the outer layers only, so the outer-layer constant and the outer copper
/// are used. A net that is a plane or an outer pour is not checked: its copper is an area
/// and not a track.
pub fn track_widths(model: &Model, process: &Process, features: &Features) -> Vec<Guideline> {
    let copper = process.stackup.copper_weight * OUNCE_OF_COPPER;
    if copper <= 0.0 {
        return Vec::new();
    }
    let layer = Layer::External;
    let (rise, rise_is_default) = match process.max_temperature_rise {
        Some(r) => (r, false),
        None => (DEFAULT_TEMPERATURE_RISE, true),
    };
    if rise <= 0.0 {
        return Vec::new();
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
                let Some((lo, hi)) = magnitude(model, model.member(id, "current")) else {
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
                        lo,
                        hi,
                    },
                ));
            }
            InstKind::Part => {
                let (Some(_), Some((a, b))) = (model.through_of(id), inst.bridge) else {
                    continue;
                };
                let Some((lo, hi)) = magnitude(model, model.member(id, "current")) else {
                    continue;
                };
                carriers.push((
                    vec![a, b],
                    Carrier {
                        who: format!("part `{}`", model.path_of(id)),
                        inst: id,
                        lo,
                        hi,
                    },
                ));
            }
            _ => {}
        }
    }

    let mut found = Vec::new();
    for net in model.nets() {
        if net.terminals.len() < 2 || planes.contains(&net.name.as_str()) {
            continue;
        }
        // The largest current on the net; the first of equals, so the order is the model's.
        let mut worst: Option<&Carrier> = None;
        for (pins, c) in &carriers {
            if pins.iter().any(|p| net.pins.contains(p)) && worst.is_none_or(|w| c.hi > w.hi) {
                worst = Some(c);
            }
        }
        let Some(worst) = worst else {
            continue;
        };
        if worst.hi <= 0.0 {
            continue;
        }
        let stated = features
            .net_widths
            .iter()
            .find(|(n, _)| *n == net.name)
            .map(|(_, w)| *w);
        let width = stated.map_or(process.min_track, |w| w.max(process.min_track));
        let carries = ipc2221_current(width, copper, rise, layer);
        let needs = ipc2221_width(worst.hi, copper, rise, layer);
        if width >= needs {
            continue;
        }
        // Rounded up to the 0.01 mm that a width is written in, so that asking for what the
        // message says passes.
        let asks = (needs * 100.0 - 1e-9).ceil() / 100.0;
        let source = match stated {
            Some(_) => "a rail's `.min_track`",
            None => "the process's least, because no rail on it states a `.min_track`",
        };
        let range = if worst.lo == worst.hi {
            amps(worst.hi)
        } else {
            format!("{} to {}", amps(worst.lo), amps(worst.hi))
        };
        found.push(Guideline {
            rule: "IPC-2221",
            message: format!(
                "net `{}` carries up to {} ({}: {range}) but its tracks are {} mm wide ({source}), in {} oz ({} mm) {} copper; at a temperature rise of {} K{} that width carries {} and IPC-2221 needs {} mm: give a rail on the net `.min_track &= {}mm..1m`, or allow a higher `.process.max_temperature_rise`",
                net.name,
                amps(worst.hi),
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
    found
}
