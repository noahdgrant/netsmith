//! A radio's certification against the antenna the board connects (issue #1278), checked after
//! the solve.
//!
//! A radio part states its grant as a `std::radio::Radio` with a `RadioBand` for each band nested
//! in it (the most antenna gain the grant allows there), and an antenna part states a
//! `std::radio::Antenna` with an `AntennaBand` for each band it works in (its gain there). The
//! check follows the RF path from the radio's `.rf` pin, as [`path`] says, and holds every antenna
//! whose `.feed` is on it against every band of the radio: the antenna's gain at its most, in the
//! bands of its own that overlap the radio's band, must be no more than the radio's
//! `.max_antenna_gain` there, at its least. A gain over it is an error; a band with no figure, an
//! antenna that states no gain in a band of the radio, and a path with no antenna on it (one that
//! ends at a connector, an antenna on a cable) are listed as not evaluated.
//!
//! The other conditions of the grant (the antenna's type, a reference design of the trace, the
//! exposure distance, the co-location limits) are not judged: [`verify_lines`] gives the lines of
//! `fab-notes.txt` that ask a person to verify each, and the gain a person must verify where the
//! check could not judge it (the owner's decision on issue #1278).
//!
//! Gains are in dBi, plain numbers, because the language has no unit for decibels.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use netsmith_check::{Diagnostic, Domain, InstId, InstKind, Member, Model};
use netsmith_core::Cause;
use netsmith_quantity::{Dimension, Quantity};

use crate::charge::{known, range, statement_of};
use crate::levels::NotEvaluated;

/// The type names of the standard library's interfaces.
const RADIO: &str = "Radio";
const RADIO_BAND: &str = "RadioBand";
const ANTENNA: &str = "Antenna";
const ANTENNA_BAND: &str = "AntennaBand";

/// The faults of the radios of a design against their antennas, and the checks that could not be
/// made.
pub(crate) fn check(model: &Model) -> (Vec<Diagnostic>, Vec<NotEvaluated>) {
    let mut faults = Vec::new();
    let mut notes = Vec::new();
    let nets = Nets::of(model);
    for radio in radios(model) {
        let r = model.path_of(radio);
        let not_judged = |message: String, notes: &mut Vec<NotEvaluated>| {
            notes.push(NotEvaluated {
                at: radio,
                involves: vec![radio],
                message,
                cause: Cause::NotStated,
            })
        };
        let path = match nets.path(model, radio) {
            Path::NoPin => continue,
            Path::Open => {
                not_judged(
                    format!(
                        "the antenna of the radio `{r}` is not judged against its grant: `{r}.rf` is joined to nothing; join it to the antenna's feed (`.{r}.rf ~ .antenna.feed`)"
                    ),
                    &mut notes,
                );
                continue;
            }
            Path::Found(p) => p,
        };
        if path.antennas.is_empty() {
            let why = match path.connectors.first() {
                Some(&c) => format!(
                    "the RF path from `{r}.rf` ends at the connector `{}` and reaches no antenna part, so the antenna on its cable is a person's to verify",
                    model.path_of(c)
                ),
                None => format!(
                    "the RF path from `{r}.rf` reaches no antenna part (a part with a `std::radio::Antenna` whose `.feed` is on the path)"
                ),
            };
            not_judged(
                format!("the antenna gain of the radio `{r}` is not judged: {why}"),
                &mut notes,
            );
            continue;
        }
        for band in children(model, radio, RADIO_BAND) {
            let b = model.path_of(band);
            let (Some(freq), Some(limit)) = (
                known(model, band, "band"),
                known(model, band, "max_antenna_gain"),
            ) else {
                not_judged(
                    format!(
                        "the antenna gain of the radio `{r}` in `{b}` is not judged: it states no `.band` or no `.max_antenna_gain`; state both (`.band &= 2.4GHz..2.4835GHz; .max_antenna_gain &= 2`)"
                    ),
                    &mut notes,
                );
                continue;
            };
            for &antenna in &path.antennas {
                let a = model.path_of(antenna);
                let mut most: Option<(f64, InstId)> = None;
                for ab in children(model, antenna, ANTENNA_BAND) {
                    let (Some(af), Some(gain)) =
                        (known(model, ab, "band"), known(model, ab, "gain"))
                    else {
                        continue;
                    };
                    if !overlaps(af, freq) {
                        continue;
                    }
                    if most.is_none_or(|(g, _)| gain.hi() > g) {
                        most = Some((gain.hi(), ab));
                    }
                }
                let Some((gain, ab)) = most else {
                    not_judged(
                        format!(
                            "the antenna `{a}` on the RF path of the radio `{r}` is not judged against `{b}`: it states no gain in {} (an `AntennaBand` with a `.band` that overlaps it and a `.gain`)",
                            frequencies(freq)
                        ),
                        &mut notes,
                    );
                    continue;
                };
                if gain > limit.lo() + 1e-9 * limit.lo().abs().max(1.0) {
                    let ap = model.path_of(ab);
                    let (source, span) = statement_of(model, ab, "gain");
                    faults.push(Diagnostic {
                        source,
                        span,
                        message: format!(
                            "the antenna `{a}` on the RF path of the radio `{r}` has a gain of {} dBi in {} (`{ap}.gain`), above the {} dBi the radio's grant allows there (`{b}.max_antenna_gain`){}: use an antenna with no more gain, or a radio whose grant allows it",
                            dbi(gain),
                            frequencies(freq),
                            dbi(limit.lo()),
                            through(model, &path.series),
                        ),
                    });
                }
            }
        }
    }
    (faults, notes)
}

/// The lines of `fab-notes.txt` that ask a person to verify the conditions of each radio's grant
/// the check does not judge: the antenna's type, the reference design, the exposure distance,
/// the co-location limits, and the gain where no antenna part is on the RF path.
pub fn verify_lines(model: &Model) -> Vec<String> {
    let nets = Nets::of(model);
    let mut lines = Vec::new();
    for radio in radios(model) {
        let r = model.path_of(radio);
        let grant = text(model, radio, "grant").unwrap_or_else(|| "a grant not named".into());
        let head = format!("verify: radio `{r}` ({grant}): a person must verify");
        let path = nets.path(model, radio);
        let antennas = match &path {
            Path::Found(p) => p.antennas.clone(),
            _ => Vec::new(),
        };
        if let Some(wanted) = variant(model, radio, "antenna_type") {
            let stated: Vec<String> = antennas
                .iter()
                .map(|&a| {
                    let ty = variant(model, a, "type").unwrap_or_else(|| "no type".into());
                    format!("`{}` is {ty}", model.path_of(a))
                })
                .collect();
            let board = if stated.is_empty() {
                String::new()
            } else {
                format!(" ({})", stated.join(", "))
            };
            lines.push(format!(
                "{head} that the antenna is of the type the grant was tested with, {wanted}{board}"
            ));
        }
        if let Some(design) = text(model, radio, "reference_design") {
            lines.push(format!(
                "{head} that the trace to the antenna follows the reference design the grant requires: {design}"
            ));
        }
        if let Some(d) = known(model, radio, "exposure_distance") {
            lines.push(format!(
                "{head} that the product keeps a person at least {} from the antenna (the grant's exposure condition)",
                range(Dimension::LENGTH, d)
            ));
        }
        if let Some(c) = text(model, radio, "colocation") {
            lines.push(format!(
                "{head} the limits on radios beside it (co-location): {c}"
            ));
        }
        if matches!(path, Path::NoPin) {
            continue;
        }
        if antennas.is_empty() {
            let at = match &path {
                Path::Found(p) => p
                    .connectors
                    .first()
                    .map(|&c| format!(" (connected at `{}`)", model.path_of(c))),
                _ => None,
            }
            .unwrap_or_default();
            for band in children(model, radio, RADIO_BAND) {
                if let (Some(freq), Some(limit)) = (
                    known(model, band, "band"),
                    known(model, band, "max_antenna_gain"),
                ) {
                    lines.push(format!(
                        "{head} that the antenna{at} has no more than {} dBi of gain in {}",
                        dbi(limit.lo()),
                        frequencies(freq)
                    ));
                }
            }
        }
    }
    lines
}

/// The radios of a design: the `Radio` interfaces of parts.
fn radios(model: &Model) -> Vec<InstId> {
    model
        .instances()
        .filter(|(id, i)| {
            i.kind == InstKind::Interface
                && i.type_name == RADIO
                && matches!(model.member(*id, "grant"), Some(Member::Field(_)))
        })
        .map(|(id, _)| id)
        .collect()
}

/// The interfaces of the type `type_name` nested in `inst`, in the order they were declared.
fn children(model: &Model, inst: InstId, type_name: &str) -> Vec<InstId> {
    model
        .instance(inst)
        .children
        .iter()
        .filter_map(|(_, m)| match m {
            Member::Inst(c)
                if model.instance(*c).kind == InstKind::Interface
                    && model.instance(*c).type_name == type_name =>
            {
                Some(*c)
            }
            _ => None,
        })
        .collect()
}

/// The text of a member of `String`, when it has one.
fn text(model: &Model, inst: InstId, name: &str) -> Option<String> {
    match model.member(inst, name)? {
        Member::Field(f) => match &model.field(f).domain {
            Domain::Text(t) => Some(t.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// The variant a member of an enum has, without its enum's name (`Chip` of `AntennaType::Chip`).
fn variant(model: &Model, inst: InstId, name: &str) -> Option<String> {
    text(model, inst, name).map(|v| v.rsplit("::").next().unwrap_or(&v).to_string())
}

/// Whether two ranges of frequency share more than an end (a single frequency overlaps a range
/// it lies in).
fn overlaps(a: Quantity, b: Quantity) -> bool {
    let lo = a.lo().max(b.lo());
    let hi = a.hi().min(b.hi());
    hi > lo || (hi == lo && (a.lo() == a.hi() || b.lo() == b.hi()))
}

/// A band for a message, its ends at full precision (`2.4GHz to 2.4835GHz`): a band's edge
/// is not rounded as a quantity's figure is.
fn frequencies(q: Quantity) -> String {
    let one = |hz: f64| {
        let (scale, unit) = if hz.abs() >= 1e9 {
            (1e9, "GHz")
        } else if hz.abs() >= 1e6 {
            (1e6, "MHz")
        } else if hz.abs() >= 1e3 {
            (1e3, "kHz")
        } else {
            (1.0, "Hz")
        };
        format!("{}{unit}", (hz / scale * 1e6).round() / 1e6)
    };
    if q.lo() == q.hi() {
        one(q.lo())
    } else {
        format!("{} to {}", one(q.lo()), one(q.hi()))
    }
}

/// A gain for a message: `2`, `1.5`, `-0.5`.
fn dbi(value: f64) -> String {
    let v = (value * 1e6).round() / 1e6;
    format!("{v}")
}

/// ` (through r1, c1)` for the series parts the path passed, or nothing.
fn through(model: &Model, series: &[InstId]) -> String {
    if series.is_empty() {
        return String::new();
    }
    let names: Vec<String> = series
        .iter()
        .map(|&p| format!("`{}`", model.path_of(p)))
        .collect();
    format!(", through {}", names.join(", "))
}

/// What the RF path of a radio is.
enum Path {
    /// The radio has no RF pin on a pad: its antenna is on the module.
    NoPin,
    /// The RF pin is on a net of its own.
    Open,
    Found(Found),
}

/// What the RF path reached.
struct Found {
    /// The `Antenna` interfaces whose feed is on the path.
    antennas: Vec<InstId>,
    /// The connectors on the path, where an antenna on a cable would be.
    connectors: Vec<InstId>,
    /// The parts the path went through, in the order it reached them.
    series: Vec<InstId>,
}

/// The nets of a design, read once for every radio.
struct Nets {
    /// The net of each pin.
    of: HashMap<InstId, usize>,
    /// The nets that are a rail's `pos` or `neg`: the path never enters them.
    power: HashSet<usize>,
    /// Each part's terminals, with their nets.
    terminals: HashMap<InstId, Vec<(InstId, usize)>>,
    /// The `Antenna` interface each antenna feed pin is the feed of.
    feeds: HashMap<InstId, InstId>,
}

impl Nets {
    fn of(model: &Model) -> Self {
        let mut of = HashMap::new();
        let mut terminals: HashMap<InstId, Vec<(InstId, usize)>> = HashMap::new();
        for (i, net) in model.nets().iter().enumerate() {
            for &p in &net.pins {
                of.insert(p, i);
            }
            for &t in &net.terminals {
                if let Some(part) = model.owner_of(t) {
                    terminals.entry(part).or_default().push((t, i));
                }
            }
        }
        let mut power = HashSet::new();
        let mut feeds = HashMap::new();
        for (id, inst) in model.instances() {
            if let Some((_, pos, neg)) = model.rail_of(id) {
                power.extend(of.get(&pos).copied());
                power.extend(of.get(&neg).copied());
            }
            if inst.kind == InstKind::Interface
                && inst.type_name == ANTENNA
                && let Some(Member::Inst(feed)) = model.member(id, "feed")
            {
                feeds.insert(feed, id);
            }
        }
        Nets {
            of,
            power,
            terminals,
            feeds,
        }
    }

    /// The RF path of `radio`: its `.rf` pin's net, and on through each part with two terminals
    /// to the net of its other terminal, never into a rail's net, never through the radio's own
    /// part, an antenna or a connector.
    fn path(&self, model: &Model, radio: InstId) -> Path {
        let Some(Member::Inst(rf)) = model.member(radio, "rf") else {
            return Path::NoPin;
        };
        let Some(&start) = self.of.get(&rf) else {
            return Path::NoPin;
        };
        if !model.nets()[start].terminals.contains(&rf) {
            return Path::NoPin;
        }
        if model.nets()[start].terminals.len() < 2 {
            return Path::Open;
        }
        let own = model.owner_of(radio);
        let mut found = Found {
            antennas: Vec::new(),
            connectors: Vec::new(),
            series: Vec::new(),
        };
        let mut seen_nets = BTreeSet::from([start]);
        let mut seen_parts: HashSet<InstId> = own.into_iter().collect();
        let mut queue = VecDeque::from([start]);
        while let Some(net) = queue.pop_front() {
            for &t in &model.nets()[net].terminals {
                if let Some(&antenna) = self.feeds.get(&t) {
                    if !found.antennas.contains(&antenna) {
                        found.antennas.push(antenna);
                    }
                    if let Some(part) = model.owner_of(t) {
                        seen_parts.insert(part);
                    }
                    continue;
                }
                let Some(part) = model.owner_of(t) else {
                    continue;
                };
                if !seen_parts.insert(part) {
                    continue;
                }
                let inst = model.instance(part);
                if inst.kind != InstKind::Part {
                    continue;
                }
                if inst.provides.iter().any(|g| g == "Connector")
                    || inst.standard.iter().any(|g| g == "Connector")
                {
                    found.connectors.push(part);
                    continue;
                }
                let pins = self.terminals.get(&part).map_or(&[][..], Vec::as_slice);
                if pins.len() != 2 {
                    continue;
                }
                let Some(&(_, other)) = pins.iter().find(|(p, _)| *p != t) else {
                    continue;
                };
                if self.power.contains(&other) {
                    continue;
                }
                found.series.push(part);
                if seen_nets.insert(other) {
                    queue.push_back(other);
                }
            }
        }
        Path::Found(found)
    }
}
