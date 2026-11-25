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
//!
//! # Decisions, and what was rejected (ADR 0036)
//!
//! The user's behaviour is in `docs/LANGUAGE.md` ("Track width against current"). The owner
//! accepted the check on 2026-11-02. Amendments 1 to 4 and their choices are accepted as built by
//! the owner, 2026-11-11. The IPC constants are the one provisional part (see the end of the
//! decisions).
//!
//! **Why a check at all.** A rail's `.min_track` sets the width of its nets, and the solver
//! knows the worst-case current. Nothing compared them: a 2 A rail drawn at the process's 0.1
//! mm built and went to the board house, and the track would have been a fuse. A rail that
//! states no `.min_track` is drawn at the process's least, so the commonest mistake was the
//! silent default.
//!
//! **Why a dedicated check and not a solver relation.** The relation between current and width is
//! nonlinear in both, so it is a check after the currents are solved. A net with at least two
//! terminals is checked; a net that is an inner plane or an outer pour is an area, not a track,
//! and is not checked. A finding names the net, the current range and what carries it, the width
//! and where it comes from (a rail's `.min_track`, or the process's least because no rail states
//! one), the copper weight and thickness, the rise and whether it is the default, the current
//! that width carries, and the width needed rounded up to 0.01 mm so that writing the number the
//! message asks for passes.
//!
//! **The temperature rise.** `.process.max_temperature_rise` is a `Temperature` field (a built-in
//! quantity type, kelvin) of `TwoLayer` and `FourLayer`, stated by neither. It is an upper bound
//! like `via_in_pad`: the tool uses the top of the range (`&= 20K`). When the board states none
//! the check uses [`DEFAULT_TEMPERATURE_RISE`], 10 K, a conservative and commonly used design
//! figure that a fabricator does not publish, so the process does not carry it, the message says
//! "(the default)", and `std/mfg/jlcpcb.nsm` says so where the field is declared.
//!
//! **Why IPC-2221 and not IPC-2152.** IPC-2221 is the formula in every track width calculator and
//! is easy to state and to test by hand. It is a conservative guide: its curves were fitted to
//! old measurements of tracks on thin boards and ignore the board's thickness, the planes near the
//! track and the copper around it. IPC-2152 is more accurate (it accounts for those) and can allow
//! a narrower track, but it is a family of charts and corrections, not one formula. Only
//! IPC-2221 is implemented. A track that passes is not thereby safe at the stated rise, and one
//! that fails may be fine; the finding is a prompt to decide, which is why it warns.
//!
//! **A guideline warns, with a mechanism.** ADR 0030 accepted that manufacturing-rule violations
//! block `fab` and guideline violations warn. IPC-2221 is a design guide and not a limit of
//! JLCPCB, so it is a guideline. `[project] guidelines = "warn" | "error"` (default `"warn"`) and
//! `--strict` on `build` and `fab` for one run decide. A check produces a [`Guideline`] (rule,
//! message, the instance it is about) and the pipeline decides: under `"warn"` a `warning:` line
//! from `build`, a line in `fab-notes.txt` and a warning in the editor's problems panel at the
//! rail or part; under `"error"` one error per finding at the same line. The policy travels in
//! `Catalogs` (`with_strict_guidelines`), as the default assembly of ADR 0028 does. The editor
//! takes the manifest key and has no flag.
//!
//! **Limits.** A net is checked at one width, the net's. The router may draw shorter runs narrower
//! where there is no room (a neck-down between pads, a stub at a pad's width), and the pad's own
//! copper carries the last millimetres. Vias are checked against the current by the fab rules
//! (`via_current`, `netsmith-geom`'s `drc/fab_rules.rs`), and the router adds vias where a net needs
//! more than one. The router's masks reach as far as
//! the width of the track needs: a track wider than a via once reached further than the masks
//! looked, which put 0.6 and 0.9 mm rails too close to other copper (shorts included), and every
//! line the router returns has also been measured as drawn against the other nets' copper, holes
//! and the board edge (a track nearer than the rules allow is drawn at the process's least
//! instead, and a line that still is too near is left unrouted). A pour's real width at a
//! bottleneck is not measured. No ambient temperature and no duty cycle: the worst-case current is
//! taken as continuous. Inner tracks are not drawn by the router, so the inner-layer constant is
//! unused. The ranges of the IPC fit (small currents, rises, widths) are not checked: outside them
//! the result is an extrapolation. A wide net can make a tight board harder to route, and no limit
//! caps the width.
//!
//! **Amendment 1 (issue #4): the router sizes every net.** The owner's direction on
//! issue #4 was: there are standards for track width by current, the router should work the
//! current out of the schematic, and a user who has to state a width has found a failure of the
//! router. So no syntax was added. After the currents are solved, a net with at least two
//! terminals gets the width IPC-2221 needs for its current, rounded up to 0.01 mm, whenever no
//! rail on it states a `.min_track`, every carrier on it has a known current, and the width is
//! more than the process's least. It goes into `Features::net_widths` as if a rail had stated it.
//! An explicit `.min_track` stays an override: the net is drawn at that width and the check
//! warns if IPC-2221 needs more, since the statement is the user's decision and is not raised
//! silently. An unknown current was first left at the least with a note (the solver leaves the
//! currents of a net with two or more unknown ones unsolved, issue #18); amendment 3 replaced it.
//! The formula and its source are below.
//!
//! **Amendment 2 (issue #4): the sum, by Kirchhoff's current law.** The first
//! amendment took the largest single current on a net, which is wrong wherever two things share a
//! net (a ground that two rails return through carries both). The carriers are a rail (a load takes
//! its `current` out of the net its `pos` is on and puts it back into the net its `neg` is on; a
//! supply, whose current is negative, is the reverse) and a part that passes a current (it leaves
//! the net of the first end of its `bridge` and enters the net of the second, so the nets inside a
//! `~>` chain take the chain's current). Taking the top of each range, a net's current is the sum
//! of what is taken out of it, with every load on at the same time. The sum of what is put into it
//! is the same by Kirchhoff's current law when the balance is complete; the larger of the two is
//! used, so that a pin the model leaves out (ADR 0027), or a supply stated as a budget wider than
//! its loads (ADR 0038), makes the result larger and not smaller. A ground shared by a 2 A rail and
//! a 1 A rail is a 3 A net. The message names the sum and its largest terms (up to four, then "and
//! N more") and is placed at the largest term. It is pessimistic in three ways, each a limit and
//! none corrected: loads are not correlated (two that cannot draw together are both counted at
//! their largest); no duty cycle or averaging; and a ground that the load of a regulator's output
//! and the regulator's input both return through carries the sum of the two, though the load's
//! return really goes to the regulator and only the input's goes on to the source (a linear
//! regulator's ground is counted for about twice what flows to the source). The conservative figure
//! was chosen over guessing the topology of the ground. **Each current once:** a module's port
//! stands for what is inside the module (ADR 0007), so where something modelled inside it is on the
//! net the module's port on that net is left out and the inside counts; a module with nothing
//! modelled inside on the net keeps its port. Without this the `sw` rail of a buck converter, a
//! module port and an IC rail on the same node, was counted twice. **Regulators:** the input
//! current comes from the power balance and never from the output current. A `LinearRegulator`
//! states `.vin.current == -.vout.current`, and a switching regulator's IC states `.vin.current ==
//! -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)` (ADR 0025), so a 1 A,
//! 1.8 V output from 4.75 V at 85 % efficiency draws 0.459 A at its input. The quiescent current was
//! not added at first, since the library's models had no figure for it and one was not invented;
//! it is a part's stated figure now (`LinearRegulator.quiescent_current`, ADR 0096 for a
//! switching regulator), and a part that states none adds nothing. A switching regulator whose part states no efficiency leaves the input current
//! unknown. A stated `.min_track` stays an override and is checked against the sum: 1 mm on a ground
//! that carries 3 A is a finding (at its narrowest, 0.8 mm, it carries 2.03 A at 10 K; amendment
//! 5).
//!
//! Choices of amendment 2: the larger of the two sums rather than the load side alone; a module's
//! port left out only when something inside it is on the net; the pessimism at a regulator's
//! ground left as it is; no quiescent current invented (a stated one is added).
//!
//! **Amendment 3: a net with a current that is not known
//! is sized for the known part.** Found on a real 12 V automotive board: a net carried a modem load
//! of 1.2 A that was stated and a divider resistor whose current the solver could not work out. The
//! first and second amendments left such a net at the process's least track, 0.09 mm, and then
//! warned that 0.09 mm carries 417 mA against the 1.2 A that was known: the tool drew a track it
//! knew to be too narrow, and warned about it, because it would not size from a partial sum. The
//! currents that are known are a lower bound on the net's current, because loads only add up (it
//! holds for the larger of the two sums too, since each side only gains terms), so drawing the net
//! at least as wide as the known sum needs is not a guess. The width is the least the net can have,
//! not the width it needs, and the note says so (worded `a lower bound`). It is still a note and
//! not a guideline: it does not change with `--strict`, because the person has nothing to correct
//! in a finding the tool cannot make beyond stating the current. A rail that states a `.min_track`
//! on the net has decided the width and gets no note. A net the tool sized, or left at the least
//! that the known current does not exceed, is never too narrow for what is known, so it has no
//! finding; a stated width below what the known current needs is a finding worded `carries at
//! least N A` with the sentence `the current of ... is not known, so it may be more`. The cost: a
//! board with a missing current draws wider nets than before, which can make a tight board harder
//! to route. Rejected: leaving the net at the least and saying so (a known-too-narrow track and a
//! warning that only stating a width could silence, which is the failure issue #4 set out to
//! remove); sizing for a guess at the unknown current (a guess is as likely too narrow as too
//! wide); and a finding that fails the build. The code is `plan_tracks`; the tests are in
//! `tests/guidelines.rs`.
//!
//! **Amendment 4: a current that is not known but
//! bounded.** ADR 0026's Ohm bound gives a resistor whose current nothing states an upper bound,
//! `max |Va - Vb| / R`, when both ends have a known voltage range. It is not a current, and the
//! track planner does not size for it: a bound across a supply (a fuse across 16 V is 323 A) would
//! draw the net at a width nobody needs, and amendment 3 rejected sizing for a guess. What it can
//! say is that the known part is no lower bound after all. On a net with a current that is not
//! known and no rail's `.min_track`, the note of amendment 3 is left out when every current that is
//! not known is a resistor's with an Ohm bound and the width the net is drawn at carries the known
//! sum plus every bound together, in whichever direction a bound flows. The width itself is never
//! changed by a bound. A net with any current that has no bound (a rail's, a transistor's, a
//! resistor whose ends have no range) keeps its note and its wording, and so does a net whose bound
//! needs more than the width. The finding against a stated `.min_track` is as before.
//!
//! **Amendment 5 (issue #431): the width tolerance.** A fabricator's finished track can be
//! narrower than the one drawn: JLCPCB states `.track_width_tolerance &= 0.2` (20 %) in its
//! processes. A track is sized for the narrowest finished track the process's tolerance allows, so
//! the width a net needs is IPC-2221's divided by `1 - tolerance`: 1.25 times it for JLCPCB, then
//! rounded up to 0.01 mm as before (2 A in 1 oz at 10 K needs 0.7814 mm by the formula and is drawn
//! at 0.98 mm). The check against a stated `.min_track` carries the same: the current that width
//! carries is that of the width less the tolerance (1 mm carries what 0.8 mm does), and the finding
//! says so. `ipc2221_width` and `ipc2221_current` are the plain formula and do not change.
//!
//! **Heat only, not voltage drop (issue #431, decided by the agent under the owner's delegation,
//! 2026-11-13).** A net is sized for the heat its current makes, not for a drop budget. The width
//! is chosen before the board is routed, and the drop depends on the length the router then
//! draws, so a drop rule needs the routed board: a later check, not this sizing. The drop is not
//! always small: 2 A through 50 mm of a 0.98 mm track in 1 oz copper drops about 0.05 V (1 % of
//! 5 V), and 0.5 A through 50 mm of 0.2 mm about 0.06 V (1.9 % of 3.3 V). A design where it
//! matters states a wider `.min_track` on the rail. That later check is the `track drop`
//! guideline of `fab` (issue #652, `netsmith_fabricate::track_drop`), which reads each net's rail
//! voltage from [`net_voltages`].
//!
//! **As built, not done.** A net whose rail states a `.min_track` gets no note when a current on it
//! is not known (the guideline names the missing carriers instead), and the note is not a guideline
//! that `--strict` can turn into an error. **Provisional:** the IPC constants below, until someone
//! with the standard compares them (it is paywalled; a person must do it, checklist in issue #10).
//! No example of `examples/` changed its widths or notes with any amendment: their currents are
//! milliamps, at the process's least track.

use netsmith_check::{Domain, InstId, InstKind, Member, Model, PinKind};
use netsmith_core::{CurrentFrom, Features, InnerUse, NetCurrent, OUNCE_OF_COPPER, Process};

/// The temperature rise the check designs to when the board states none, in kelvin. A
/// default of this tool's and not a published figure (ADR 0036).
pub const DEFAULT_TEMPERATURE_RISE: f64 = 10.0;

/// The part of a board's shorter side that a net's tracks may take before the board is told
/// (issue #807): a fifth. Nothing else caps a net's width, so a large current on a small
/// board makes a track that is a large part of the board. A choice of this tool's, with no
/// published source, and PROVISIONAL like the guidelines beside it: 4 mm is a fifth of a
/// 20 mm board, and a track drawn that wide carries about 5.6 A at 10 K in 1 oz copper, which
/// is where a power track begins to leave little room for anything else.
pub const WIDE_NET_FRACTION: f64 = 0.2;

// The formula, its constants and their source are in `netsmith_geom::ipc2221`, which the design
// rules check sizes a branch's copper with too (issue #1113).
pub use netsmith_geom::ipc2221::{Layer, TrackSizing, ipc2221_current, ipc2221_width, via_current};

/// The word for a layer in a message.
fn layer_word(layer: Layer) -> &'static str {
    match layer {
        Layer::External => "outer",
        Layer::Internal => "inner",
    }
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
    /// The heating (rms) current a rail states (`.rms_current`, issue #1690), in amperes: the
    /// larger end of its range in size.
    stated_rms: Option<f64>,
    /// The most its heating current can be, in amperes, and whether a stated rms current made
    /// it (see [`heats`]); `None` while its current is not known.
    heat: Option<Heat>,
}

/// What a carrier's copper heats with at most (issue #1690): its rms current, or its peak.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Heat {
    amps: f64,
    /// Whether a stated `.rms_current` brought it below the peak, on this carrier or on the
    /// ones it was worked out from.
    rms: bool,
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

/// The voltage of the rails on each net, by net name, for the check of the voltage a net's
/// routed tracks drop (issue #652): a rail's `.voltage` belongs to the net its `pos` is on, at
/// the smaller end of its range in size, and a net with several rails takes the smallest. A
/// rail the board states unused, one whose voltage is not known and one whose range holds 0 V
/// give none, and nor does the net of a `neg` (a ground has no voltage of its own).
pub fn net_voltages(model: &Model) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    for (id, inst) in model.instances() {
        if inst.kind != InstKind::Interface || model.unused_by(id).is_some() {
            continue;
        }
        let Some((_, pos)) = model.draw_of(id) else {
            continue;
        };
        let Some((lo, hi)) = signed_range(model, model.member(id, "voltage")) else {
            continue;
        };
        if lo * hi <= 0.0 {
            continue;
        }
        let volts = lo.abs().min(hi.abs());
        let Some(net) = model.nets().iter().find(|n| n.pins.contains(&pos)) else {
            continue;
        };
        match out.iter_mut().find(|(n, _)| *n == net.name) {
            Some((_, v)) => *v = v.min(volts),
            None => out.push((net.name.clone(), volts)),
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
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

    /// The largest single current a carrier on the net draws through it (a load, or a part that
    /// passes a current), with the carrier: the least the net's copper carries whatever else is
    /// on (issue #1584). A supply's range is a budget and not a draw, and is left out.
    fn largest_load(&self) -> Option<(&'a Carrier, f64)> {
        self.taken
            .iter()
            .chain(&self.given)
            .filter(|t| t.2)
            .map(|t| (t.0, t.1))
            .max_by(|a, b| a.1.total_cmp(&b.1))
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

/// A member of a group of loads on a net: its index in the group, the carriers that are it, and
/// the sum of their spans.
type GroupMember = (usize, Vec<usize>, f64);

/// The ranges of the carriers of a net, with the loads of a group of loads (issue #806) that
/// cannot be on at once at their low end: of each group with more members on the net than its
/// limit, the members with the largest spans keep their ranges and the rest draw their low end,
/// so the net carries the worst `limit` of them ([`netsmith_check::worst_of`]). A module's port
/// in a group stands for the loads inside the module. A carrier that can give current is no
/// load and keeps its range.
fn grouped_ranges(model: &netsmith_check::Model, counted: &[&Carrier]) -> Vec<Option<(f64, f64)>> {
    let mut ranges: Vec<Option<(f64, f64)>> = counted.iter().map(|c| c.range).collect();
    // For each group: each member's carriers and the sum of their ranges.
    let mut groups: Vec<(usize, Vec<GroupMember>)> = Vec::new();
    for (k, c) in counted.iter().enumerate() {
        let Some((lo, hi)) = c.range else { continue };
        if lo < 0.0 {
            continue;
        }
        let Some((g, m)) = model.load_member_of(c.inst) else {
            continue;
        };
        let members = match groups.iter_mut().find(|(id, _)| *id == g) {
            Some((_, members)) => members,
            None => {
                groups.push((g, Vec::new()));
                &mut groups.last_mut().expect("just pushed").1
            }
        };
        match members.iter_mut().find(|(id, _, _)| *id == m) {
            Some((_, carriers, span)) => {
                carriers.push(k);
                *span += hi - lo;
            }
            None => members.push((m, vec![k], hi - lo)),
        }
    }
    for (g, mut members) in groups {
        let limit = model.load_groups()[g].limit;
        if members.len() <= limit {
            continue;
        }
        members.sort_by(|a, b| b.2.total_cmp(&a.2));
        for (_, carriers, _) in members.iter().skip(limit) {
            for &k in carriers {
                ranges[k] = ranges[k].map(|(lo, _)| (lo, lo));
            }
        }
    }
    ranges
}

/// The statements of a heating (rms) current that cannot hold (issue #1690): an
/// `.rms_current` on a rail or a pin whose `.current` is not known (stated or worked out by
/// the solver), and one above the most that `.current` (or, for a load that bursts, its
/// `.peak_current`) reaches in size, since the rms of a current is never more than its peak.
/// Each fails the build.
pub fn rms_errors(model: &Model) -> Vec<String> {
    let mut errors = Vec::new();
    for (id, inst) in model.instances() {
        let rail = inst.kind == InstKind::Interface && model.draw_of(id).is_some();
        if !(rail || inst.kind == InstKind::Pin) || model.unused_by(id).is_some() {
            continue;
        }
        let Some((lo, hi)) = signed_range(model, model.member(id, "rms_current")) else {
            continue;
        };
        let rms = lo.abs().max(hi.abs());
        let path = model.path_of(id);
        match signed_range(model, model.member(id, "current")) {
            None => errors.push(format!(
                "`{path}.rms_current` is stated but `{path}.current` is not known: the rms current sizes the copper for heat beside the peak in `.current`, which the track drop measures at; state `{path}.current` (the most it carries at an instant), or leave the rms current out"
            )),
            Some((clo, chi)) => {
                // A load that bursts states its average as `.current` and its burst as
                // `.peak_current` (issue #1279): its rms lies between the two.
                let burst = signed_range(model, model.member(id, "peak_current"))
                    .map(|(lo, hi)| lo.abs().max(hi.abs()));
                let peak = clo.abs().max(chi.abs()).max(burst.unwrap_or(0.0));
                let named = if burst.is_some_and(|b| b >= chi.abs().max(clo.abs())) {
                    format!("`{path}.peak_current` reaches")
                } else {
                    format!("`{path}.current` reaches")
                };
                if rms > peak * (1.0 + 1e-9) + 1e-15 {
                    errors.push(format!(
                        "`{path}.rms_current` reaches {}, above the most {named}, {}: the rms of a current is never more than its peak; state an rms current of at most {}, or a `.current` that reaches the peak",
                        amps(rms),
                        amps(peak),
                        amps(peak),
                    ));
                }
            }
        }
    }
    errors
}

/// The most each carrier's heating current can be (issue #1690), by index into `carriers`;
/// `counted` is the carriers counted on each net.
///
/// A rail that states an `.rms_current` heats with it (never more than its peak). Any other
/// carrier heats with its peak, unless Kirchhoff's current law bounds it lower: what flows
/// through it into a net is, at every instant, what the other carriers on that net take out,
/// and the rms of a sum is never more than the sum of the rms values (the triangle inequality,
/// which holds whether the loads burst together or not), so a fuse in front of a load, or the
/// supply that feeds it, heats with no more than the sum of the heating currents of the others
/// on the net. That needs every other current on the net to be known. Each bound only lowers
/// a figure, so a pass is repeated until none changes.
fn heats(model: &Model, carriers: &[Carrier], counted: &[Vec<usize>]) -> Vec<Option<Heat>> {
    let mut heat: Vec<Option<Heat>> = carriers
        .iter()
        .map(|c| {
            let (lo, hi) = c.range?;
            let peak = lo.abs().max(hi.abs());
            Some(match c.stated_rms {
                Some(rms) if rms < peak => Heat {
                    amps: rms,
                    rms: true,
                },
                _ => Heat {
                    amps: peak,
                    rms: false,
                },
            })
        })
        .collect();
    // On each net, the carriers with one end on it: what flows through it.
    let through: Vec<Vec<usize>> = model
        .nets()
        .iter()
        .zip(counted)
        .map(|(net, ks)| {
            ks.iter()
                .copied()
                .filter(|&k| {
                    let c = &carriers[k];
                    net.pins.contains(&c.from) != c.to.is_some_and(|t| net.pins.contains(&t))
                })
                .collect()
        })
        .collect();
    for _ in 0..=carriers.len() {
        let mut changed = false;
        for ks in &through {
            if ks.len() < 2 || ks.iter().any(|&k| heat[k].is_none()) {
                continue;
            }
            for &k in ks {
                if carriers[k].stated_rms.is_some() {
                    continue;
                }
                let others = ks.iter().filter(|&&d| d != k).filter_map(|&d| heat[d]);
                let (sum, rms) = others.fold((0.0, false), |(s, r), h| (s + h.amps, r || h.rms));
                let Some(own) = heat[k] else { continue };
                if sum < own.amps * (1.0 - 1e-9) {
                    heat[k] = Some(Heat {
                        amps: sum,
                        rms: rms || own.rms,
                    });
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    heat
}

/// The heating (rms) current of a net and its words (issue #1690), where a stated
/// `.rms_current` brings it below the net's peak `total`; `None` where it does not, and the
/// copper is sized by the peak as before. It is the larger of the two sides of the net's flow,
/// each the sum of its terms' heating currents (no more than the term), and never more than
/// the peak.
fn net_rms(flow: &Flow, total: f64) -> Option<(f64, String)> {
    fn side<'a>(v: &[(&'a Carrier, f64, bool)]) -> (f64, bool, Vec<(&'a Carrier, f64)>) {
        let terms: Vec<(&Carrier, f64)> = v
            .iter()
            .filter_map(|t| Some((t.0, t.0.heat?.amps.min(t.1))))
            .collect();
        let sum: f64 = terms.iter().map(|t| t.1).sum();
        let rms = v.iter().any(|t| t.0.heat.is_some_and(|h| h.rms));
        (sum, rms, terms)
    }
    let (taken, given) = (side(&flow.taken), side(&flow.given));
    let (sum, rms, mut terms) = if (given.0 - taken.0).abs() <= 1e-9 * taken.0.max(given.0) {
        // The side with the statements is the one named.
        let stated = |v: &[(&Carrier, f64)]| v.iter().filter(|t| t.0.stated_rms.is_some()).count();
        if stated(&given.2) > stated(&taken.2) {
            given
        } else {
            taken
        }
    } else if given.0 > taken.0 {
        given
    } else {
        taken
    };
    if !rms || sum >= total * (1.0 - 1e-9) {
        return None;
    }
    terms.sort_by(|a, b| b.1.total_cmp(&a.1));
    let shown: Vec<String> = terms
        .iter()
        .take(4)
        .map(|(c, amps_of)| {
            let how = match c.heat {
                Some(h) if h.rms && c.stated_rms.is_some() => "rms",
                Some(h) if h.rms => "rms, what the others on a net it joins carry",
                _ => "its peak: it states no `.rms_current`",
            };
            format!("{} {} ({how})", c.who, amps(*amps_of))
        })
        .collect();
    let more = match terms.len() - shown.len() {
        0 => String::new(),
        n => format!(" and {n} more"),
    };
    Some((
        sum,
        format!(
            "the sum of the rms currents on it, below its peak of {}: {}{more}",
            amps(total),
            shown.join(" + ")
        ),
    ))
}

/// The longest the board's shorter side can be, in mm, when its outline states a limit on at
/// least one side: the smaller of the two largest sizes the outline allows. A board with no
/// outline, or with none of its sides limited, has none that is known (a board as big as its
/// parts need has no size yet).
fn shorter_side_at_most(features: &Features) -> Option<f64> {
    let outline = features.outline.as_ref()?;
    let side = outline.width.1.min(outline.height.1);
    (side.is_finite() && side > 0.0).then_some(side)
}

/// How copper is sized for a current on a board of `process` (ADR 0036): its outer copper, the
/// copper of an inner signal layer, the board's `.process.max_temperature_rise` or
/// [`DEFAULT_TEMPERATURE_RISE`], the fabricator's width tolerance and its via. `None` for a
/// process with no copper or no rise, which sizes nothing.
pub fn sizing_of(process: &Process) -> Option<TrackSizing> {
    let copper = process.stackup.copper_weight * OUNCE_OF_COPPER;
    let rise = process
        .max_temperature_rise
        .unwrap_or(DEFAULT_TEMPERATURE_RISE);
    if copper <= 0.0 || rise <= 0.0 {
        return None;
    }
    Some(TrackSizing {
        copper,
        inner_copper: process
            .stackup
            .inner
            .as_ref()
            .filter(|i| i.copper > 0.0 && i.layers.contains(&InnerUse::Signal))
            .map(|i| i.copper),
        rise,
        tolerance: process.track_width_tolerance.clamp(0.0, 0.9),
        via_drill: process.via_drill,
        via_plating: process.hole_plating,
    })
}

/// What the track check works out for a board (ADR 0036).
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
    /// The current of each net where some of it is known, and the least width that carries it.
    pub currents: Vec<NetCurrent>,
    /// The pins of parts that state the current of their branch (issue #1113), with the copper
    /// that current needs, by the pin's path.
    pub branches: Vec<Branch>,
    /// The pins whose current the model knows and is less than their net's (issue #1333).
    pub pin_currents: Vec<PinCurrent>,
    /// A pin whose branch is stated to carry less than the model says flows through it.
    pub errors: Vec<String>,
}

/// A pin of a part whose current the model knows, on a net with a known current, below the
/// net's (issue #1333), as `pin_currents` finds it.
#[derive(Debug, Clone, PartialEq)]
pub struct PinCurrent {
    pub pin: InstId,
    pub net: String,
    /// The most that flows through the pin, in amperes.
    pub current: f64,
    /// Its heating (rms) current, where a stated `.rms_current` brings it below `current`
    /// (issue #1690).
    pub rms: Option<f64>,
}

/// A pin of a part that states the current of its branch (issue #1113): the copper from the
/// pin's pads to the rest of its net is sized for that current and not for the net's.
#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    pub pin: InstId,
    pub net: String,
    /// The most the branch carries, in amperes: the larger end of the stated range, in size.
    pub current: f64,
    /// The width its tracks are drawn at on the outer layers, in mm: what the current needs,
    /// rounded up to the 0.01 mm a width is written in, and never less than the process's
    /// least track or a `.min_track` a rail on the net states.
    pub width: f64,
    /// The narrowest a track of it may be, on an outer layer and on an inner signal layer.
    pub least_width: f64,
    pub inner_least_width: Option<f64>,
    /// How many of the process's vias, side by side, carry it.
    pub vias: usize,
    /// Its heating (rms) current, where the pin states an `.rms_current` below its `.current`
    /// (issue #1690): the widths and the vias are sized for it.
    pub rms: Option<f64>,
}

/// The tracks of every net against the current the net carries, by IPC-2221 (ADR 0036).
///
/// The current a net carries comes from Kirchhoff's current law (ADR 0036, amendment of
/// 2026-11-04). The things that put a current into a net or take one out of it are the rails
/// (a load takes its draw out of the net its `pos` is on and puts it back into the net its
/// `neg` is on; a supply is the reverse) and the parts that pass a current (it leaves the net
/// of the first end of their `bridge` and enters the net of the second). The net carries the
/// sum of what is taken out, with every load on at the same time (of a group of loads that take
/// turns, issue #806, the worst few), or the sum of what is put in, whichever is larger: a ground net shared by a 2 A and a 1 A load carries 3 A. The
/// currents are the solver's, so a regulator's input is what its model says it draws (the
/// output current for a linear one, the power balance with the efficiency for a switching
/// one), never its output current.
///
/// Where the DC analysis solved the net, its worst case over the corners is the net's current,
/// unless one rail or part on the net draws more than that through it (a part's stated
/// `.current`, which a DC analysis may not see: a bypass diode's discharge): then that one is
/// (issue #1584). Each [`NetCurrent`] says which (`from`) and whether a current on the net is
/// not known (`lower_bound`).
///
/// A module's port stands for what is inside the module (ADR 0007), so each current is
/// counted once: at the port when nothing inside the module is on the net, otherwise inside.
/// Its tracks are as wide as the rails on
/// it ask (`.min_track`, the widest bottom, in `features.net_widths`) and never narrower than
/// the process's least. A net no rail states a width for, whose every current is known, is
/// given the width IPC-2221 needs (ADR 0036) in [`TrackPlan::widths`]. Where some current on
/// the net is not known, the known ones are a lower bound (loads only add up) and the net is
/// given the width they need, never narrower than the process's least, and is named in
/// [`TrackPlan::notes`] (ADR 0036, amendment 3). The router keeps a net with a known current on
/// the outer layers (issue #833), so the outer-layer constant and the outer copper are used. A current that is not known but
/// has an upper bound (`bounds`, Ohm's law over the voltages at the ends of a resistor) leaves
/// the width as it is and says nothing more when the width drawn already carries the known
/// current and every bound together, at both signs (ADR 0036, amendment 4). A net that is a plane or an
/// outer pour is not checked: its copper is an area and not a track.
///
/// A net whose needed width is more than [`WIDE_NET_FRACTION`] of the board's shorter side
/// gets a second finding, rule `net width` (issue #807). The side is the outline's: the
/// smaller of its width and height at their largest, so a board that may still be small
/// enough to be fine is not told; a board with no outline, or no limit on a side, is not
/// checked. The width is what IPC-2221 needs, as it is in [`TrackPlan::widths`], whether or
/// not a rail stated one; where a current on the net is not known the finding says "at least".
pub fn plan_tracks(
    model: &Model,
    process: &Process,
    features: &Features,
    bounds: &[netsmith_solve::CurrentBound],
    net_currents: &[netsmith_solve::NetCurrent],
) -> TrackPlan {
    let mut plan = TrackPlan::default();
    let Some(sizing) = sizing_of(process) else {
        return plan;
    };
    let (copper, rise, tolerance) = (sizing.copper, sizing.rise, sizing.tolerance);
    let layer = Layer::External;
    let rise_is_default = process.max_temperature_rise.is_none();
    let planes: Vec<&str> = process
        .stackup
        .inner
        .iter()
        .flat_map(|i| i.planes().map(|(_, net)| net))
        .chain(features.pours.iter().map(|p| p.net.as_str()))
        .collect();

    // What can carry a current: a rail (an interface that draws) and a part that passes one.
    let mut carriers: Vec<Carrier> = Vec::new();
    for (id, inst) in model.instances() {
        match inst.kind {
            InstKind::Interface => {
                // A rail of an interface the board states unused carries nothing (ADR 0083).
                if model.unused_by(id).is_some() {
                    continue;
                }
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
                    stated_rms: signed_range(model, model.member(id, "rms_current"))
                        .map(|(lo, hi)| lo.abs().max(hi.abs())),
                    heat: None,
                });
            }
            InstKind::Part => {
                let (Some(through), Some((a, b))) = (model.through_of(id), inst.bridge) else {
                    continue;
                };
                carriers.push(Carrier {
                    who: format!("part `{}`", model.path_of(id)),
                    inst: id,
                    holder: id,
                    is_rail: false,
                    from: a,
                    to: Some(b),
                    range: signed_range(model, Some(Member::Field(through))),
                    stated_rms: None,
                    heat: None,
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

    // The carriers counted on each net, by index into `carriers`. A module's port stands for
    // what is inside the module (ADR 0007) and the rails and parts inside it are the same
    // current seen closer up. Where something inside the module is on the net, the port is
    // left out and the inside counts; a module with nothing modelled inside on the net keeps
    // its port, which is all that is known of it.
    let counted_on: Vec<Vec<usize>> = model
        .nets()
        .iter()
        .map(|net| {
            let on_net: Vec<usize> = (0..carriers.len())
                .filter(|&k| touching(&carriers[k], net))
                .collect();
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
                    && on_net.iter().any(|&d| inside(c.holder, &carriers[d]))
            };
            on_net
                .iter()
                .copied()
                .filter(|&k| !covered(&carriers[k]))
                .collect()
        })
        .collect();
    let heat_of = heats(model, &carriers, &counted_on);
    for (c, heat) in carriers.iter_mut().zip(heat_of) {
        c.heat = heat;
    }

    for (index, net) in model.nets().iter().enumerate() {
        if net.terminals.len() < 2 || planes.contains(&net.name.as_str()) {
            continue;
        }
        let counted: Vec<&Carrier> = counted_on[index].iter().map(|&k| &carriers[k]).collect();

        let mut flow = Flow::default();
        let mut unknown: Vec<&str> = Vec::new();
        // The sum of the upper bounds of the unknown currents, or `None` when one of them has
        // none (a rail, a part that Ohm's law does not bound).
        let mut bounded: Option<f64> = Some(0.0);
        let ranges = grouped_ranges(model, &counted);
        for (c, range) in counted.iter().zip(ranges) {
            let at_from = net.pins.contains(&c.from);
            let at_to = c.to.is_some_and(|t| net.pins.contains(&t));
            // Both ends on the net: what leaves comes back, nothing flows through it.
            if at_from && at_to {
                continue;
            }
            let Some(range) = range else {
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
        let (plain, terms) = flow.worst();
        // The DC analysis solved the net: the most that flows into it over every corner of the
        // ranges, which is complete (every pin on it is modelled, so no current is unknown) and
        // not larger than the sum of every current at its largest (ADR 0076, amendment 8).
        let analysed = net_currents
            .iter()
            .find(|c| net.pins.contains(&c.pin))
            .map(|c| c.current);
        // One rail or part on the net that draws more than the analysis finds wins over it
        // (issue #1584): a bypass or clamp diode that states the current it discharges carries
        // it through the net's copper, though the DC analysis sees it blocking. The stated
        // current is the design's word, and the larger is the safer.
        let largest = flow.largest_load();
        let (total, from) = match (analysed, largest) {
            (Some(a), Some((_, hi))) if hi > a * (1.0 + 1e-9) => (hi, CurrentFrom::Carrier),
            (Some(a), _) => (a, CurrentFrom::Analysis),
            (None, _) => (plain, CurrentFrom::Sum),
        };
        if analysed.is_some() {
            unknown.clear();
        }
        // The heating (rms) current, where a stated `.rms_current` brings it below the peak
        // (issue #1690): the copper is sized for heat by it, and `total` stays the peak, which
        // the track drop reads.
        let rms = net_rms(&flow, total);
        let heat = rms.as_ref().map_or(total, |r| r.0);
        // The currents that are known are a lower bound on the net's current: loads only add
        // up, so a current that is not known can make the sum larger and never smaller. A net
        // is drawn at least as wide as the known part needs, which is no guess (ADR 0036,
        // amendment 3).
        // Sized for the narrowest track the fabricator's tolerance allows (issue #431): a track
        // drawn at `needs` is finished no narrower than the width IPC-2221 asks.
        let needs = sizing.least_width(heat);
        if total > 0.0 {
            plan.currents.push(NetCurrent {
                net: net.name.clone(),
                current: total,
                least_width: needs,
                // On an inner signal layer: the inner constant and the inner copper (issue #833).
                inner_least_width: sizing.inner_least_width(heat),
                vias: sizing.vias(heat),
                source: with_unknown(
                    &current_source(from, plain, analysed, largest, &terms),
                    &unknown,
                ),
                from,
                lower_bound: !unknown.is_empty(),
                rms: rms.as_ref().map(|r| r.0),
                rms_source: rms
                    .as_ref()
                    .map(|r| with_unknown(&r.1, &unknown))
                    .unwrap_or_default(),
            });
        }
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
            ipc2221_width(heat + more, copper, rise, layer) / (1.0 - tolerance)
                <= width * (1.0 + 1e-9)
        });
        if !unknown.is_empty() && stated.is_none() && supplied[index] && !enough {
            let known = (heat > 0.0).then_some(heat);
            plan.notes
                .push(unknown_note(&net.name, &unknown, width, known, sized));
        }
        let Some(&(worst, _)) = terms.first() else {
            continue;
        };
        if total <= 0.0 {
            continue;
        }
        if let Some(side) = shorter_side_at_most(features)
            && asks > WIDE_NET_FRACTION * side * (1.0 + 1e-9)
        {
            plan.findings.push(wide_net(
                &net.name,
                heat,
                asks,
                side,
                !unknown.is_empty(),
                worst.inst,
            ));
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
        // What the narrowest finished track of that width carries.
        let carries = ipc2221_current(width * (1.0 - tolerance), copper, rise, layer);
        if width >= needs {
            continue;
        }
        let what = match &rms {
            Some((_, words)) => words.clone(),
            None => current_source(from, plain, analysed, largest, &terms),
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
                "net `{}` carries {carries_word} {} ({what}) but its tracks are {} mm wide (a rail's `.min_track`), in {} oz ({} mm) {} copper; at a temperature rise of {} K{} that width carries {} (at its narrowest, {}% under, the fabricator's tolerance) and IPC-2221 needs {} mm: give a rail on the net `.min_track &= {}mm..`, or allow a higher `.process.max_temperature_rise`",
                net.name,
                if rms.is_some() {
                    format!("{} rms", amps(heat))
                } else {
                    amps(heat)
                },
                mm(width),
                mm(process.stackup.copper_weight),
                mm(copper),
                layer_word(layer),
                mm(rise),
                if rise_is_default { " (the default)" } else { "" },
                amps(carries),
                mm(tolerance * 100.0),
                mm(asks),
                mm(asks),
            ),
            at: Some(worst.inst),
        });
    }
    plan.widths.sort_by(|a, b| a.0.cmp(&b.0));
    branches(model, process, features, &sizing, &carriers, &mut plan);
    pin_currents(model, &carriers, &mut plan);
    plan
}

/// The pins whose own current the model knows, below the current of their net (issue #1333):
/// a feedback divider's top resistor on a rail of amps carries microamps, and the copper that
/// only feeds it needs no more. What flows through a pin is the sum of what the rails whose
/// `pos` or `neg` it is and the parts with an end of their `bridge` on it carry, each at the
/// larger end of its range in size, with the pins joined to it inside its part (an alias with
/// no pad of its own is the same copper). An `input` pin that nothing carries a current through
/// (an enable pin on a converter's input, issue #1504) carries none. Any other pin that nothing
/// carries a current through (a capacitor's, whose ripple the model does not know, a pin no
/// rail names), or that one carrier with no known current does, is left out: its current is
/// not known, and the design rules check holds the copper to it to its net's. So is
/// a pin that carries its whole net's current, which says nothing more than the net does. Only
/// the nets with a known current ([`TrackPlan::currents`]) are read: the others are not held
/// to a width. The router does not read these; a pin that states its own current
/// ([`branches`]) is drawn for it.
fn pin_currents(model: &Model, carriers: &[Carrier], plan: &mut TrackPlan) {
    for net in model.nets() {
        let Some(total) = plan
            .currents
            .iter()
            .find(|c| c.net == net.name)
            .map(|c| c.current)
        else {
            continue;
        };
        for &pin in &net.terminals {
            if model.instance(pin).kind != InstKind::Pin
                || plan.branches.iter().any(|b| b.pin == pin)
            {
                continue;
            }
            // The pin and the pins joined to it inside its part are one copper.
            let mut same = model.joined_inside(pin);
            same.push(pin);
            let mut through = carriers
                .iter()
                .filter(|c| c.pins().any(|p| same.contains(&p)))
                .map(|c| c.range.map(|(lo, hi)| lo.abs().max(hi.abs())))
                .peekable();
            if through.peek().is_none() {
                // An input that nothing carries a current through (an enable or undervoltage
                // pin, a sense or bias input) takes no current, its leakage aside (issue
                // #1504): the model counts none for it in its net's current, and the copper
                // that only feeds it is held to what the other pins past it carry.
                if same
                    .iter()
                    .all(|&p| model.pin_kind(p) == Some(PinKind::Input))
                {
                    plan.pin_currents.push(PinCurrent {
                        pin,
                        net: net.name.clone(),
                        current: 0.0,
                        rms: None,
                    });
                }
                continue;
            }
            let Some(current) = through.sum::<Option<f64>>() else {
                continue;
            };
            // Its heating current (issue #1690): the sum of the carriers', where a stated
            // `.rms_current` brings it below the peak.
            let heats: Vec<Heat> = carriers
                .iter()
                .filter(|c| c.pins().any(|p| same.contains(&p)))
                .filter_map(|c| c.heat)
                .collect();
            let heat: f64 = heats.iter().map(|h| h.amps).sum();
            let rms =
                (heats.iter().any(|h| h.rms) && heat < current * (1.0 - 1e-9)).then_some(heat);
            if current < total * (1.0 - 1e-9) {
                plan.pin_currents.push(PinCurrent {
                    pin,
                    net: net.name.clone(),
                    current,
                    rms,
                });
            }
        }
    }
}

/// Where a net's current comes from, in words (issue 1373): the one carrier that draws more than
/// the DC analysis finds (`largest`, issue #1584), the DC analysis' worst case when it solved the
/// net (`analysed`; `plain` is every current at its largest at once), or the terms that add up
/// to it, largest first.
fn current_source(
    from: CurrentFrom,
    plain: f64,
    analysed: Option<f64>,
    largest: Option<(&Carrier, f64)>,
    terms: &[(&Carrier, f64)],
) -> String {
    match terms {
        _ if from == CurrentFrom::Carrier => match (largest, analysed) {
            (Some((c, hi)), Some(a)) => format!(
                "{} carries {}, more than the {} the DC analysis finds into the net",
                c.who,
                amps(hi),
                amps(a)
            ),
            _ => String::new(),
        },
        _ if from == CurrentFrom::Analysis => format!(
            "the most the DC analysis finds into it over every corner of its ranges; every current at its largest at once would be {}",
            amps(plain)
        ),
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
    }
}

/// `what` with the currents that are not known said after it.
fn with_unknown(what: &str, unknown: &[&str]) -> String {
    if unknown.is_empty() {
        what.to_string()
    } else {
        format!(
            "{what}; the current of {} is not known, so it may be more",
            unknown.join(", ")
        )
    }
}

/// The pins of parts that state the current of their branch, sized (issue #1113), and the ones
/// whose statement is less than what the model says flows through the pin: a rail whose `pos`
/// or `neg` it is, or a part that passes a current with an end of its `bridge` on it.
///
/// A pin's `.current` says how much the copper between its pads and the rest of the net
/// carries at most, so that copper is drawn for it: a converter's bias pin, a feedback
/// divider's top, a sense resistor's tap carry almost nothing, though the net they are on is a
/// rail of amps. It is the size of the range that counts, so `1mA` and `-1mA..0A` both say 1
/// mA. It is no term of the net's balance and adds nothing to the net's current: what is
/// drawn through the pin is drawn by the rail or the part that has it, which the balance
/// already counts. A pin that states none is drawn as its net is.
fn branches(
    model: &Model,
    process: &Process,
    features: &Features,
    sizing: &TrackSizing,
    carriers: &[Carrier],
    plan: &mut TrackPlan,
) {
    for (id, inst) in model.instances() {
        if inst.kind != InstKind::Pin {
            continue;
        }
        let Some((lo, hi)) = signed_range(model, model.member(id, "current")) else {
            continue;
        };
        let Some(net) = model.nets().iter().find(|n| n.terminals.contains(&id)) else {
            continue;
        };
        let current = lo.abs().max(hi.abs());
        // The heating (rms) current of the branch (issue #1690), never above its peak: a pin
        // that states one above it is refused by `rms_errors`.
        let rms = signed_range(model, model.member(id, "rms_current"))
            .map(|(lo, hi)| lo.abs().max(hi.abs()))
            .filter(|&r| r < current);
        let heat = rms.unwrap_or(current);
        // What the model says flows through the pin: the copper cannot carry less.
        for c in carriers.iter().filter(|c| c.pins().any(|p| p == id)) {
            let Some((clo, chi)) = c.range else { continue };
            let flows = clo.abs().max(chi.abs());
            if flows > current * (1.0 + 1e-9) + 1e-15 {
                plan.errors.push(format!(
                    "`{}.current` says its branch carries at most {}, but {} carries up to {} through it: state at least {} on the pin, or leave its current out so that its copper is drawn for its net",
                    model.path_of(id),
                    amps(current),
                    c.who,
                    amps(flows),
                    amps(flows),
                ));
            }
        }
        let least_width = sizing.least_width(heat);
        let stated = features
            .net_widths
            .iter()
            .find(|(n, _)| *n == net.name)
            .map_or(0.0, |(_, w)| *w);
        let width = ((least_width * 100.0 - 1e-9).ceil() / 100.0)
            .max(process.min_track)
            .max(stated);
        plan.branches.push(Branch {
            pin: id,
            net: net.name.clone(),
            current,
            width,
            least_width,
            inner_least_width: sizing.inner_least_width(heat),
            vias: sizing.vias(heat),
            rms,
        });
    }
}

/// The finding for a net whose needed width `asks` (mm) is more than [`WIDE_NET_FRACTION`] of
/// the board's shorter side `side` (mm, at most).
fn wide_net(
    net: &str,
    current: f64,
    asks: f64,
    side: f64,
    lower_bound: bool,
    at: InstId,
) -> Guideline {
    let (needs, current) = if lower_bound {
        (
            "at least ",
            format!("at least {}, the part of it that is known", amps(current)),
        )
    } else {
        ("", amps(current))
    };
    Guideline {
        rule: "net width",
        message: format!(
            "net `{net}` needs tracks {needs}{} mm wide (IPC-2221, for {current}), which is {:.0}% of the board's shorter side, at most {} mm (the outline's largest): over {:.0}% leaves little room for anything else; make the board larger, lower the current, or give the net a plane or an outer pour",
            mm(asks),
            100.0 * asks / side,
            mm(side),
            100.0 * WIDE_NET_FRACTION,
        ),
        at: Some(at),
    }
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
