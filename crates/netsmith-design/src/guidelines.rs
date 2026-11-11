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
//! copper carries the last millimetres. Vias are not checked. The router's masks reach as far as
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
//! -.buck.sw.current * .buck.sw.voltage / (.vin.voltage * .buck.efficiency)` (ADR 0025), so a 1 A,
//! 1.8 V output from 4.75 V at 85 % efficiency draws 0.459 A at its input. The quiescent current is
//! not added: the library's models have no figure for it (ADR 0025 ignores it) and one is not
//! invented here. A switching regulator whose part states no efficiency leaves the input current
//! unknown. A stated `.min_track` stays an override and is checked against the sum: 1 mm on a ground
//! that carries 3 A is a finding (at its narrowest, 0.8 mm, it carries 2.03 A at 10 K; amendment
//! 5).
//!
//! Choices of amendment 2: the larger of the two sums rather than the load side alone; a module's
//! port left out only when something inside it is on the net; the pessimism at a regulator's
//! ground left as it is; no quiescent current added.
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
//! **As built, not done.** A net whose rail states a `.min_track` gets no note when a current on it
//! is not known (the guideline names the missing carriers instead), and the note is not a guideline
//! that `--strict` can turn into an error. **Provisional:** the IPC constants below, until someone
//! with the standard compares them (it is paywalled; a person must do it, checklist in issue #10).
//! No example of `examples/` changed its widths or notes with any amendment: their currents are
//! milliamps, at the process's least track.

use netsmith_check::{Domain, InstId, InstKind, Member, Model};
use netsmith_core::{Features, NetCurrent, OUNCE_OF_COPPER, Process};

/// The temperature rise the check designs to when the board states none, in kelvin. A
/// default of this tool's and not a published figure (ADR 0036).
pub const DEFAULT_TEMPERATURE_RISE: f64 = 10.0;

/// One mil, in mm.
const MIL: f64 = 0.0254;

// The formula and its constants (ADR 0036).
//
// IPC-2221B, "Generic Standard on Printed Board Design", section 6.2 "Conductor current
// carrying capacity" (the same fit is in IPC-2221A and, as the chart IPC-2221's Figure 6-4,
// in IPC-D-275): `I = k * dT^b * A^c`, with
//   I   the current in amperes,
//   dT  the temperature rise above ambient in degrees Celsius (= kelvin),
//   A   the conductor's cross-section in square mils (width times copper thickness),
//   b = 0.44, c = 0.725,
//   k = 0.048 for a conductor on an outer layer (in air) and 0.024 on an inner layer.
// The standard is paywalled and was not read here, so the constants are PROVISIONAL until a person
// compares them with it (issue #10); the formula, the constants and their meaning were taken from
// several agreeing published derivations (Altium's IPC-2221 calculator page, the `pcb-toolkit`
// crate's `current.rs`, circuit.rocks), and checked by hand in `tests/guidelines.rs`. It is an empirical fit to old measurements of tracks in
// air, and the calculators say it holds only inside the ranges of those measurements (which
// this code did not verify against the standard): outside them the result is extrapolated.
//
// Assumed, not from the standard (the tool's choices, accepted as built, ADR 0036): the copper
// weight is the process's `stackup.copper_weight` (1 oz, 0.035 mm, for JLCPCB's two-layer outer
// copper), and the rise is the board's `.process.max_temperature_rise` or, when it states none,
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

/// The current a via with a hole of `drill` mm, plated `plating` mm thick, carries for a rise of
/// `rise` kelvin (issue #431): IPC-2221's formula over the copper of the barrel, the ring of the
/// plating round the finished hole, with the inner-layer constant, since the barrel is
/// inside the board and sheds less heat than a track in air. Calculators differ here (some take
/// the outer constant, some IPC-2152's charts); the inner one is the lower of IPC-2221's two. A
/// 0.2 mm hole plated 0.018 mm carries 0.56 A at 10 K.
pub fn via_current(drill: f64, plating: f64, rise: f64) -> f64 {
    if drill <= 0.0 || plating <= 0.0 {
        return 0.0;
    }
    // A drill is the finished hole: the plating is round it.
    let inner = drill / 2.0;
    let outer = inner + plating;
    let area = std::f64::consts::PI * (outer * outer - inner * inner) / (MIL * MIL);
    Layer::Internal.k() * rise.powf(0.44) * area.powf(0.725)
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
    net_currents: &[netsmith_solve::NetCurrent],
) -> TrackPlan {
    let mut plan = TrackPlan::default();
    let copper = process.stackup.copper_weight * OUNCE_OF_COPPER;
    if copper <= 0.0 {
        return plan;
    }
    let layer = Layer::External;
    let tolerance = process.track_width_tolerance.clamp(0.0, 0.9);
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
        let (plain, terms) = flow.worst();
        // The DC analysis solved the net: the most that flows into it over every corner of the
        // ranges, which is complete (every pin on it is modelled, so no current is unknown) and
        // not larger than the sum of every current at its largest (ADR 0076, amendment 8).
        let analysed = net_currents
            .iter()
            .find(|c| net.pins.contains(&c.pin))
            .map(|c| c.current);
        let total = analysed.unwrap_or(plain);
        if analysed.is_some() {
            unknown.clear();
        }
        // The currents that are known are a lower bound on the net's current: loads only add
        // up, so a current that is not known can make the sum larger and never smaller. A net
        // is drawn at least as wide as the known part needs, which is no guess (ADR 0036,
        // amendment 3).
        // Sized for the narrowest track the fabricator's tolerance allows (issue #431): a track
        // drawn at `needs` is finished no narrower than the width IPC-2221 asks.
        let needs = if total > 0.0 {
            ipc2221_width(total, copper, rise, layer) / (1.0 - tolerance)
        } else {
            0.0
        };
        if total > 0.0 {
            let each = via_current(process.via_drill, process.hole_plating, rise);
            let vias = if each > 0.0 {
                ((total / each) - 1e-9).ceil().max(1.0) as usize
            } else {
                1
            };
            plan.currents.push(NetCurrent {
                net: net.name.clone(),
                current: total,
                least_width: needs,
                vias,
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
            ipc2221_width(total + more, copper, rise, layer) / (1.0 - tolerance)
                <= width * (1.0 + 1e-9)
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
        // What the narrowest finished track of that width carries.
        let carries = ipc2221_current(width * (1.0 - tolerance), copper, rise, layer);
        if width >= needs {
            continue;
        }
        let what = match terms.as_slice() {
            _ if analysed.is_some() => format!(
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
                "net `{}` carries {carries_word} {} ({what}) but its tracks are {} mm wide (a rail's `.min_track`), in {} oz ({} mm) {} copper; at a temperature rise of {} K{} that width carries {} (at its narrowest, {}% under, the fabricator's tolerance) and IPC-2221 needs {} mm: give a rail on the net `.min_track &= {}mm..1m`, or allow a higher `.process.max_temperature_rise`",
                net.name,
                amps(total),
                mm(width),
                mm(process.stackup.copper_weight),
                mm(copper),
                layer.word(),
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
