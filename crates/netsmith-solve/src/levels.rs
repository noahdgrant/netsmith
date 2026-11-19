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
//!
//! The absolute maximum of a pin is held against the net's worst-case voltage relative to the
//! pin's ground, which is the hull of the ground itself, each rail whose `pos` is on the net, each
//! `Logic` line that drives it (between its reference rail's pins) and each rail or ground a
//! resistor joins it to (ADR 0046). A level is the datasheet's guaranteed number, so `voh` and
//! `vih` are minimums and `vol` and `vil` maximums, and they are ranges when the rail is. The
//! checks that cannot be made are the ones listed above, and also a net that a driver which is
//! not a `Logic` line drives, and a rail on another ground. A limit is stated only in a pin row
//! (ADR 0126), which names the pin's ground (ADR 0124): the `Logic`'s or the bus's, the row's
//! `from`, or the one rail ground of the part's table. The voltage of a net is not followed through anything but resistors, so a
//! net that a plain output drives is not evaluated, and a level check does not know the load (the
//! part states the load of its numbers in its comment).
//!
//! An open-drain line takes its high from its pull-up (ADR 0075, no language change). Two parts on
//! an `I2c` bus state `.vil`, `.vih` and `.vol` and no `.voh`, as their datasheets do, because an
//! open-drain pin does not set its high; with no `voh` every ordered pair on each line was a note,
//! and the check that mattered, that the pull-up's rail is a high for every reader, was never made:
//! a bus pulled up to 1.8 V with a 3.3 V reader passed in silence. In `pair`, a driver that states
//! `vol` and no `voh`, on a net that has a pull-up, has as its high the voltage range of the
//! pull-up's rail, and each receiver's `vih` is compared with it as with a `voh`. A pull-up is a
//! resistor (a part with a `bridge` and a `resistance`) from the net to a net that holds the
//! positive pin of a rail (`I2cPullups`, `PullUp` and a plain `Resistor` are all recognised, by the
//! structure that `net_range` reads); several pull-ups give the hull of their rails. A driver that
//! states `voh` is judged by it whatever is on the net; a driver that states no `vol` says nothing
//! and its high and low are notes; a net with a resistor to its own ground as well is a divider and
//! keeps the note; a rail on another ground or with no voltage is a note that names it; a chain of
//! resistors is not followed (ADR 0044's I2C rule follows a chain), so a pull-up behind a series
//! resistor or a switch keeps the note (as ADR 0075 wrote it; the DC analysis's solved circuit of
//! the line is read first since, `high_from_analysis`). Not taken: stating a `voh` on the bus from `I2cPullups` or
//! `PullUp` (a module cannot set a member of a part inside another module, `docs/DESIGN-RULES.md`;
//! `~` between two bus interfaces joins no levels, ADR 0064; and it fixes the high to the module's
//! own rail when a board may have a second pull-up); a pin kind or marker for open-drain (KiCad's
//! `open_collector`, ADR 0064 question 10); ignoring a stated `voh` when a pull-up is on the net (a
//! reset line with a resistor is a normal circuit). The cost of the inference: "states `vol` and no
//! `voh`" is how an open-drain pin looks in a datasheet, but also how a push-pull part with an
//! incomplete model looks, and such a part on a net with a pull-up has its high taken from the
//! pull-up and not reported as missing, where the note was the only sign that the model is
//! incomplete. The rule needs `vol`, a pull-up, and a rail with a voltage.
//!
//! Decided by the owner, 2026-11-11: the inference stays. The TLV7022's two outputs on one net
//! then broke the net rules, and the kind `open_drain` was added (#460, the owner's decision of
//! 2026-11-12): an `open_drain` pin always takes its high from its pull-up, whatever it states,
//! and the inference stays for the other kinds, so the I2C pins of std (`bidirectional`) keep
//! their checks. In `net_range` an `open_drain` pin only pulls its net to its ground.
//!
//! Not built from ADR 0046: more parts with datasheet levels (issue #64), and levels across a
//! series part from a driver the DC analysis does not model into a loaded net (below); the voltage of a net is
//! followed only through resistors, for its absolute maximum, where the DC analysis did not solve
//! it. Open-drain lines have levels since ADR 0075,
//! above. A level translator that is a part with a `Logic` line or a bus on each side
//! (`LevelTranslator`) is judged on each side's net, as any two parts are.
//!
//! What the levels check cannot judge is named (#850), never left out in silence, when a line on
//! the net states the levels the other end would be compared with:
//!
//! - A pin that can drive the net and has no levels (an `output`, `tri_state`, `bidirectional`
//!   or `open_drain` pin that is not the `.line` of a `Logic` or a pin of an interface or a part
//!   with `levels`), with a receiver that states `vih` or `vil`: its levels are not known. A plain
//!   `open_drain` pin leaves the high to the circuit around the line as an `open_drain` line
//!   does, so its high is judged from the DC analysis or the pull-up, against the receiver's
//!   ground, and only its low is a note.
//! - A pin that reads the net and has no levels (an `input` or `bidirectional` one), with a
//!   driver that states `voh`, `vol` or a swing: the levels it needs are not known. A gate of a
//!   transistor and an enable pin of a regulator are such pins, and their own relations judge
//!   them where the part states any.
//! - A receiver that states levels on a net that nothing on it drives, joined by a part with a
//!   `bridge` to a net that something drives (a series resistor, a divider's top, a diode, the
//!   channel of a MOSFET that translates a level). A `Logic` driver of the far net that the DC
//!   analysis models (its levels state a test current, or it states `roh` and `rol`) is judged:
//!   the analysis solves its island once with the driver held high and once held low
//!   (`NetReads::driven`), and the receiver's `vih` and `vil` are held against the range of its
//!   net's voltage in each, over every corner and every state of the rest of the island, so a
//!   load the island models (a pull-down, a pull-up, an input's leakage) is in the number. An
//!   open-drain driver (#1045) has as its high the net's voltage with every driver of the two
//!   nets letting go. A driver the analysis does not model, behind a resistor into a net that
//!   nothing else is on (its other pins are `input`s that state no leakage), is judged as a pair
//!   on the far net (#1045): no current flows through the resistor, so the two nets are at one
//!   voltage. Such a driver behind a two-resistor divider is judged at the divider's levels
//!   (#1270, below). Any other driver, an island the analysis did not solve, and a receiver whose ground
//!   is not the island's are a note that names the part and the net the level comes from. A part
//!   that only pulls the net to a rail or to ground has no driver on its far side and is not
//!   named.
//! - A receiver that states levels on a net whose own drivers can all let go of it (none is an
//!   `output`), joined by a part with a `bridge` to a net with a driver that states levels: the
//!   far driver's level crosses the part when the near drivers let go, as the low of one bus
//!   crosses the MOSFET of a translator into the other bus. Each side is judged on its own lines.
//!   What crosses is judged as above, with every driver of the receiver's net held letting go of
//!   it (#1045), where the analysis models the far driver; for the rest, one note for each part
//!   and net names what crosses. A net with an `output` is held by it.
//! - A MOSFET between the two nets (#1045). The analysis takes a MOSFET as a switch, on or off by
//!   its gate, and finds states that only its body diode rules out: the far side pulls the drain
//!   of a translator low, and the MOSFET stays off with the source at its pull-up. A part that
//!   states `.body_diode_voltage` (the datasheet's VSD, the most forward voltage of the diode
//!   from the source to the drain of an N-channel part) has the diode in the island of these
//!   solves, and is judged with it. One that does not is solved again with its channel held on:
//!   a level that fails with the channel on is a fault, one that holds only with it on is a note
//!   that names the body diode, never a pass.
//!
//! A part states the levels of its plain pins once (issue #1190). A datasheet gives one table of
//! levels for the I/O pins of a supply, and a part whose pins were plain had no levels at all, so
//! every net it shared with a `Logic` line was a note; giving each pin a `Logic` of its own only to
//! repeat the same four statements was the only way out. A part that declares `.levels: Levels`
//! (the interface a bus carries as `.levels`, ADR 0064) gives those levels to each of its plain
//! pins whose row's kind is one this check reads (`input`, `output`, `bidirectional`, `tri_state`,
//! `open_drain`): `Model::levels_of` returns the part's `levels` for such a pin, as it returns a
//! bus's `levels` for a pin of the bus, so the pair, the absolute maximum (measured from the ground
//! of `.levels.reference`, and `from` is refused in the row) and the DC analysis read it as they
//! read a bus pin. A `power`, `passive` or `no_connect` pin keeps no levels and the ground of its
//! row. A pin of an interface (a `Logic` line, a bus pin) has its interface's levels and never the
//! part's, so a pin with a datasheet row of its own (a reset input) is a `Logic` and keeps them,
//! whole: a `Logic` that states only `vih` does not take `voh` from the part. Not taken (the
//! owner's choice, asked in the pull request): a word in the pin row naming the `Levels` it takes
//! (`levels .io`), which would allow two groups on two supplies in one part (today one of them is
//! an interface with `levels`, or `Logic` lines), and a block that lists the pins of a group.
//!
//! An input level stated in rows by the supply (#1305, #1307) is written from its rows before
//! this check runs (`crate::supply_levels`), and read here as any level; a level its rows give
//! no value is the note that it has none, with the reason (`Run::unread_why`).
//!
//! A control input strapped to a rail or to ground is at a known level (#1304). A board ties a
//! mode pin of a transceiver straight to its supply or to ground, and the pin was judged as a net
//! that nothing drives: every passive on the ground whose far end something drives (a pull-down
//! on another line) was a series part the level came through, and each strapped pin gave a note
//! for each of them. A net that holds the `pos` of a rail is held at the rail's voltage range,
//! and one that holds a `neg` (and no `pos`) at 0 V (`holder`): a receiver on it that states
//! `vih` or `vil` is judged by that range against its `vih` (a rail) or its `vil` (a ground),
//! measured from its own ground, and no level comes into the net through a series part
//! (`series_sources` and `crossings` skip it), so nothing names its passives. The resistors on
//! such a net do not move it for an absolute maximum (`net_range`): a resistor from the supply to
//! ground left the ground at the supply's voltage. A level the pin does not state, a rail with no
//! voltage, and a rail or a ground that is not the pin's ground are notes. Not taken: judging a
//! pin on a rail against `vil` as well (a strap to a rail means a high), and a rail that is the
//! output of a part that may be off (a load switch), which is held at its range as any rail is.
//!
//! A level through a two-resistor divider is followed (#1270; the owner's decision: two-resistor
//! dividers only). A board shifts a host's 3.3 V output into a 1.8 V input with a series resistor
//! and a resistor to ground, and the driver, which states no test current, was a note. Where the
//! DC analysis does not model a `Logic` driver behind a series resistor, and the receiver's net is
//! a divider's middle (`divider`: nothing drives or holds it, its only resistors are the series
//! one and one to a rail or a ground on the receiver's ground, and every other pin on it is an
//! `input` that states no leakage), the driver is a source at its own levels, behind its `roh` or
//! `rol` where it states one, and the net is `e + (v - e) * rb / (ra + rb)`, monotonic in each of
//! the driver's level, the rail's voltage and the two resistances, so its range is taken at their
//! corners (`Divider::range`). The lowest high is held against the receiver's `vih`, the highest
//! low against its `vil`, with the fault naming both resistors, the net the level comes from and
//! the rail or ground. The net's absolute maximum is the divided swing of the driver's net with no
//! source resistance (`divided_range`), where `net_range` saw only the resistor to the rail or
//! ground. Every other network keeps the note: a third resistor, an open-drain driver (its high
//! is its circuit's), a series part that is not a resistor, a resistance with no value, a rail on
//! another ground or with no voltage, a driver on another ground.
//!
//! A pair of which neither end states a level is still not looked at. Not taken (#850, #1045):
//! judging a receiver against a driver the DC analysis does not model behind a series part into
//! a net that something draws from, from the driver's levels alone (only the analysis knows the
//! drop); the body diode in the analysis's other solves (what it writes for a part, the voltage
//! of a net for an absolute maximum), which no level needs; a forward voltage the analysis assumes for
//! a body diode that the part does not state (a datasheet gives it at a test current, and no
//! number is sound for every part); a note for a plain pin on a net whose lines state no level,
//! which would name every gate that a line drives on a board that states no levels at all.
//!
//! Levels stated as fractions of one rail are compared at one supply voltage (ADR 0082). The
//! levels are compared as two intervals, and an interval counts a value once for each time it is
//! written (ADR 0026, ADR 0076): `0.7 * .vdd.voltage` on a 3.3 V +/- 5 % rail is 2.194 V..2.426 V
//! wherever it appears. A microphone that states `.out.voh &= 0.7 * .vdd.voltage` and a processor
//! that states `.in.vih &= 0.7 * .vdd.voltage`, on one rail, were refused, though at any actual
//! supply voltage the two are the same number. So a level that is, as written, a number times the
//! voltage of a rail (`k * V`, `V * k`, or `V`) is compared with another level of that form as a
//! fraction of one supply voltage, when both are stated by one `&=` statement of that form and by
//! nothing else, the `V` of each is the `voltage` of a rail whose `neg` is the level's ground and
//! whose lowest voltage is above zero, and the `pos` pins of the two rails are on one net. Then
//! `k_driver * V >= k_receiver * V` for every `V` is `k_driver >= k_receiver`, and a high is a
//! fault when `k_driver < k_receiver`, a low when `k_driver > k_receiver`; the message names the
//! factors and the rail voltages. Anything else (a sum, a clamp, a different net, a level a number
//! also narrows, a rail that may be zero) is compared as two independent ranges. The form is
//! recovered from the model's `Relation`s (`Run::fractions`), because the field's domain does not
//! remember it; two rails have one voltage when their `pos` pins are on one net and the ground is
//! the same, read from the nets and not from the voltage fields. Not taken: comparing at the
//! corners of the rail's range (exact for affine forms, but it needs a slope and an offset for each
//! level, a decision for forms that are not affine, and the corner of a rail with a range on each
//! side); leaving the check and having the board add margin (a series resistor), which moves a
//! fault of the model into the circuit; matching the text of the two statements (the structure is
//! enough). Limits: division (`.vdd.voltage / 2`) is not read as a fraction, because the factor
//! would be a rounded `1 / k` (write `0.5 * .vdd.voltage`); a factor that is a range is not read as
//! one; two rails that a board gives the same range on two nets are two voltages.
//!
//! Accepted as built by the owner, 2026-11-11. Not built: the rule for affine forms (a fraction of
//! the supply with an offset, `.vdd.voltage - 0.4V`), which a corner comparison decides exactly,
//! and the same treatment of the pull-up path above for a receiver at a fraction of that rail; not
//! planned until a board needs it.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use netsmith_check::{
    Diagnostic, Domain, FieldId, InstId, InstKind, Member, Model, Net, PinKind, RExpr,
};
use netsmith_core::Cause;
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
    /// Why it was not made (see [`Cause`]).
    pub cause: Cause,
}

/// What the DC analysis says of the nets it solved, for the levels check (ADR 0076, amendment 8).
#[derive(Debug, Default, Clone)]
pub(crate) struct NetReads {
    /// By net (its place in `Model::nets`): the net of the ground the voltage is measured from,
    /// and the range the line's voltage has when every pin on it that is open drain lets go
    /// of it: the high of an open-drain line, whatever its circuit makes of the pull-up.
    pub high: HashMap<usize, (usize, f64, f64)>,
    /// By net: the net of the ground the voltage is measured from, and the range of the node's
    /// voltage over every corner and state of the island the analysis solved it in (amendment 12).
    /// Only the free nets of an island that has no pin taken to be open drain by inference: a held
    /// net's voltage is its source's, which the check reads from the rail.
    pub volts: HashMap<usize, (usize, f64, f64)>,
    /// By the pin of a `Logic` line and a net of its island that a series part joins it to
    /// (#850, #1045): the net of the ground, and the range of the net's voltage with the pin
    /// held driving high and held driving low and every driver of the net itself letting go of
    /// it, over every corner and every state of the rest of the island. Only for the pins and
    /// nets [`series_drivers`] lists.
    pub driven: HashMap<(InstId, usize), Driven>,
}

/// The voltage of a net with one logic pin held high and held low (see [`NetReads::driven`]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Driven {
    pub ground: usize,
    /// With the pin driving high, or, for a pin that cannot (open drain, `idle`), with every
    /// driver of its net and of this one letting go: the high it leaves to the circuit.
    pub high: (f64, f64),
    pub low: (f64, f64),
    pub idle: bool,
    /// For each MOSFET between the two nets, the high and the low again with its channel held
    /// on: the analysis does not model a MOSFET's body diode, which turns the channel of a
    /// translator on when the far side pulls its drain low (#1045).
    pub on: Vec<HeldOn>,
}

/// A MOSFET, and the high and the low of [`Driven`] with its channel held on (none where that
/// state has no consistent combination).
pub(crate) type HeldOn = (InstId, Option<(f64, f64)>, Option<(f64, f64)>);

/// The readers of the logic interfaces and resistors of a model, for the passes that need
/// them and make no check of their own.
pub(crate) fn reader(model: &Model) -> Run<'_> {
    Run {
        model,
        reads: None,
        unread: None,
        faults: Vec::new(),
        notes: Vec::new(),
        seen: BTreeSet::new(),
        fractions: OnceCell::new(),
    }
}

/// The pins whose level reaches a receiver through a series part (#850, #1045), with the net of
/// the receiver and the parts between: each driver of a net that a part with a `bridge` joins to
/// a net on which a receiver states `vih` or `vil` and that nothing drives, or whose own drivers
/// can all let go of it. The DC analysis solves their islands once with each held high and once
/// held low, and the receiver's net let go of, for the levels check to judge the receiver by.
pub(crate) fn series_drivers(model: &Model) -> BTreeMap<(InstId, usize), BTreeSet<InstId>> {
    let run = reader(model);
    let mut out: BTreeMap<(InstId, usize), BTreeSet<InstId>> = BTreeMap::new();
    for index in 0..model.nets().len() {
        let lines = run.lines_on(&model.nets()[index]);
        let sources = run.series_sources(index, &lines);
        for (part, far) in sources.into_iter().chain(run.crossings(index, &lines)) {
            for &t in &model.nets()[far].terminals {
                if model.unused_by(t).is_none()
                    && model.owner_of(t) != Some(part)
                    && model.pin_kind(t).is_some_and(is_driver)
                {
                    out.entry((t, index)).or_default().insert(part);
                }
            }
        }
    }
    out
}

/// The faults found and the checks that could not be made.
/// `unread` says, for each level stated in rows by the supply that its rows give no value, why
/// (issue #1305, `crate::supply_levels`), for the note that it has none.
pub(crate) fn check(
    model: &Model,
    reads: &NetReads,
    unread: &HashMap<FieldId, String>,
) -> (Vec<Diagnostic>, Vec<NotEvaluated>) {
    let mut run = reader(model);
    run.reads = Some(reads);
    run.unread = Some(unread);
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
    /// What the DC analysis solved, when the check has it.
    reads: Option<&'a NetReads>,
    /// Why the rows by the supply of a level give it no value (issue #1305).
    unread: Option<&'a HashMap<FieldId, String>>,
    faults: Vec<Diagnostic>,
    notes: Vec<NotEvaluated>,
    seen: BTreeSet<String>,
    /// The levels stated as a constant times a rail's voltage, found once (ADR 0082).
    fractions: OnceCell<HashMap<FieldId, (f64, FieldId)>>,
}

fn is_driver(kind: PinKind) -> bool {
    matches!(
        kind,
        PinKind::Output | PinKind::TriState | PinKind::OpenDrain | PinKind::Bidirectional
    )
}

/// Why a driver leaves the high of its line to the circuit around it, for the messages: its
/// kind says so (#460), or it states a low and no high (ADR 0075).
fn open_drain_why(d: &Line) -> &'static str {
    if d.kind == PinKind::OpenDrain {
        "an `open_drain` pin"
    } else {
        "a driver that states `vol` and no `voh`"
    }
}

fn is_receiver(kind: PinKind) -> bool {
    matches!(kind, PinKind::Input | PinKind::Bidirectional)
}

impl Run<'_> {
    fn note(&mut self, at: InstId, involves: Vec<InstId>, cause: Cause, message: String) {
        if self.seen.insert(message.clone()) {
            self.notes.push(NotEvaluated {
                at,
                involves,
                message,
                cause,
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

    /// The interface that holds the levels of a pin: the `Logic` the pin is the line of, the
    /// `Levels` of the bus interface it is a pin of (ADR 0064), or the `Levels` of its part for
    /// a plain pin of a signal kind (issue #1190, [`Model::levels_of`]). All have the reference
    /// rail and the four voltages, and everything below reads them from whichever it gets.
    pub(crate) fn logic_of(&self, pin: InstId) -> Option<InstId> {
        self.model.levels_of(pin)
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

    /// The unloaded output level `voh` or `vol` of a line: the one it states, or, when it states
    /// none and states a swing (ADR 0081), the top (`voh`) or the bottom (`vol`) of the swing,
    /// read as a single value. The third part says it was derived from the swing, for the
    /// message to say so. A stated level always wins.
    fn output_level(&self, logic: InstId, name: &str) -> Option<(FieldId, Quantity, bool)> {
        if let Some((f, q)) = self.value(logic, name) {
            return Some((f, q, false));
        }
        let (f, swing) = self.value(logic, "swing")?;
        let at = match name {
            "voh" => swing.hi(),
            "vol" => swing.lo(),
            _ => return None,
        };
        Some((
            f,
            Quantity::interval(Dimension::VOLTAGE, at, at).ok()?,
            true,
        ))
    }

    /// How a message names an output level: its field when it is stated, else the level and the
    /// swing that it comes from.
    fn level_text(&self, logic: InstId, name: &str, field: FieldId, derived: bool) -> String {
        let model = self.model;
        if !derived {
            return format!("`{}`", model.path_of_field(field));
        }
        let end = if name == "voh" { "top" } else { "bottom" };
        format!(
            "`{}.{name}` (not stated: the {end} of its swing, `{}`)",
            model.path_of(logic),
            model.path_of_field(field)
        )
    }

    /// Why a level stated in rows by the supply has no value, as the end of a note (`: ...`),
    /// or nothing (issue #1305).
    fn unread_why(&self, logic: InstId, name: &str) -> String {
        match self.model.member(logic, name) {
            Some(Member::Field(f)) => self
                .unread
                .and_then(|u| u.get(&f))
                .map_or_else(String::new, |why| format!(": {why}")),
            _ => String::new(),
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
            // The board states the interface is not used (ADR 0083): its levels are not compared.
            if self.model.unused_by(pin).is_some() {
                continue;
            }
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
        self.strapped(index, &lines);
        self.plain_pins(index, &lines);
        self.through_series(index, &lines);
    }

    /// What holds a net at a voltage of its own (#1304): the positive pin of a rail, or a ground
    /// (the negative pin of a rail). The rail named is the one with the shortest path, which is
    /// the board's where the board states one; a net with both a rail and a ground on it (a
    /// rail of 0 V, or a short) takes the rail. None when no rail is on the net.
    fn holder(&self, index: usize) -> Option<Holder> {
        let model = self.model;
        let mut rail: Option<(String, InstId, FieldId, InstId)> = None;
        let mut ground: Option<InstId> = None;
        for &pin in &model.nets()[index].pins {
            let Some(parent) = model.instance(pin).parent else {
                continue;
            };
            let Some((voltage, pos, neg)) = model.rail_of(parent) else {
                continue;
            };
            if pin == pos {
                let path = model.path_of(parent);
                let shorter = rail.as_ref().is_none_or(|(p, ..)| {
                    (path.matches('.').count(), &path) < (p.matches('.').count(), p)
                });
                if shorter {
                    rail = Some((path, parent, voltage, neg));
                }
            } else if pin == neg && ground.is_none() {
                ground = Some(neg);
            }
        }
        match (rail, ground) {
            (Some((path, _, voltage, neg)), _) => Some(Holder::Rail { path, voltage, neg }),
            (None, Some(neg)) => Some(Holder::Ground { neg }),
            (None, None) => None,
        }
    }

    /// An input strapped to a rail or to ground (#1304): a receiver that states `vih` or `vil`
    /// on a net that holds the positive pin of a rail is held at the rail's voltage range, and
    /// one on a ground at 0 V, measured from its own ground, with no part between. A rail is a
    /// high, judged against `vih`, and a ground a low, judged against `vil`; a level the pin
    /// does not state, a rail with no voltage and a rail or a ground that is not the pin's
    /// ground are notes. Such a net is held, so no level comes into it through a series part,
    /// whatever the parts on it join it to (`series_sources`, `crossings`).
    fn strapped(&mut self, index: usize, lines: &[Line]) {
        let model = self.model;
        let net = &model.nets()[index];
        let Some(holder) = self.holder(index) else {
            return;
        };
        let receivers: Vec<&Line> = lines
            .iter()
            .filter(|l| is_receiver(l.kind) && self.states_input_levels(l.logic))
            .collect();
        for r in receivers {
            let Some(rg) = self.ground_pin(r.logic) else {
                continue;
            };
            let rl = model.path_of(r.logic);
            let (neg, what) = match &holder {
                Holder::Rail { path, neg, .. } => (*neg, format!("rail `{path}`")),
                Holder::Ground { neg } => (*neg, "a ground".to_string()),
            };
            if !self.same_ground(neg, rg) {
                self.note(
                    r.part,
                    vec![r.logic],
                    Cause::Unjudged,
                    format!(
                        "not evaluated: the logic levels into `{rl}` on net `{}`: {what} holds it, and its ground `{}` is on net `{}`, not `{}`",
                        net.name,
                        model.path_of(neg),
                        self.name_of_net(neg),
                        self.name_of_net(rg)
                    ),
                );
                continue;
            }
            match &holder {
                Holder::Rail { path, voltage, .. } => {
                    let Domain::Quantity(q) = model.field(*voltage).domain else {
                        self.note(
                            r.part,
                            vec![r.logic],
                            Cause::Unjudged,
                            format!(
                                "not evaluated: the high level into `{rl}` on net `{}`: rail `{path}` holds it and has no voltage",
                                net.name
                            ),
                        );
                        continue;
                    };
                    let Some(vih) = self.value(r.logic, "vih") else {
                        self.note(
                            r.part,
                            vec![r.logic],
                            Cause::PartModel,
                            format!(
                                "not evaluated: the high level into `{rl}` on net `{}`: rail `{path}` holds it at {}, and `{rl}.vih` has no value{}",
                                net.name,
                                range_text((q.lo(), q.hi())),
                                self.unread_why(r.logic, "vih"),
                            ),
                        );
                        continue;
                    };
                    self.held_high(net, r, vih, (q.lo(), q.hi()), |high| {
                        format!("rail `{path}` holds the line at {high}")
                    });
                }
                Holder::Ground { .. } => {
                    let Some(vil) = self.value(r.logic, "vil") else {
                        self.note(
                            r.part,
                            vec![r.logic],
                            Cause::PartModel,
                            format!(
                                "not evaluated: the low level into `{rl}` on net `{}`: it is a ground, and `{rl}.vil` has no value{}",
                                net.name,
                                self.unread_why(r.logic, "vil"),
                            ),
                        );
                        continue;
                    };
                    self.held_low(net, r, vil, (0.0, 0.0), |low| {
                        format!("the ground holds the line at {low}")
                    });
                }
            }
        }
    }

    /// Whether a line states a level a receiver is judged by (`vih` or `vil`).
    fn states_input_levels(&self, logic: InstId) -> bool {
        ["vih", "vil"]
            .iter()
            .any(|n| self.value(logic, n).is_some())
    }

    /// Whether a line states a level a driver is judged by (`voh`, `vol` or a swing).
    fn states_output_levels(&self, logic: InstId) -> bool {
        ["voh", "vol", "swing"]
            .iter()
            .any(|n| self.value(logic, n).is_some())
    }

    /// The pins on a net that have a kind and no levels: not the `.line` of a `Logic` or a pin
    /// of an interface or a part with `levels`, and not on an interface the board states is
    /// unused.
    fn plain_on(&self, net: &Net) -> Vec<(InstId, InstId, PinKind)> {
        let model = self.model;
        net.terminals
            .iter()
            .filter(|&&pin| model.unused_by(pin).is_none() && self.logic_of(pin).is_none())
            .filter_map(|&pin| Some((pin, model.owner_of(pin)?, model.pin_kind(pin)?)))
            .collect()
    }

    /// A pin with no levels on a net with a line that states them (#850): a plain output that
    /// drives a receiver, or a plain input that a driver drives. The pair is not judged, and
    /// the note names the pin. A plain `open_drain` pin leaves the high to the circuit around
    /// the line, as an `open_drain` line does, so its high is judged from that and only its
    /// low is not.
    fn plain_pins(&mut self, index: usize, lines: &[Line]) {
        let model = self.model;
        let net = &model.nets()[index];
        let plain = self.plain_on(net);
        let unknown =
            "is not the `line` of a `Logic` or a pin of an interface or a part with `levels`";
        for &(pin, part, kind) in &plain {
            let p = model.path_of(pin);
            let inside = model.joined_inside(pin);
            let apart = |l: &Line| l.pin != pin && !inside.contains(&l.pin);
            if is_driver(kind) {
                for r in lines.iter().filter(|l| is_receiver(l.kind) && apart(l)) {
                    if !self.states_input_levels(r.logic) {
                        continue;
                    }
                    let rl = model.path_of(r.logic);
                    if kind == PinKind::OpenDrain {
                        self.plain_open_drain_high(index, &p, part, r);
                        self.note(
                            part,
                            vec![part],
                            Cause::PartModel,
                            format!(
                                "not evaluated: the low level from `{p}` to `{rl}` on net `{}`: `{p}` {unknown}, so its `vol` is not known",
                                net.name
                            ),
                        );
                        continue;
                    }
                    self.note(
                        part,
                        vec![part],
                        Cause::PartModel,
                        format!(
                            "not evaluated: the logic levels from `{p}` to `{rl}` on net `{}`: `{p}` can drive it and {unknown}, so its levels are not known",
                            net.name
                        ),
                    );
                }
            }
            if is_receiver(kind) {
                for d in lines.iter().filter(|l| is_driver(l.kind) && apart(l)) {
                    if !self.states_output_levels(d.logic) {
                        continue;
                    }
                    self.note(
                        part,
                        vec![part],
                        Cause::PartModel,
                        format!(
                            "not evaluated: the logic levels from `{}` to `{p}` on net `{}`: `{p}` reads it and {unknown}, so the levels it needs are not known",
                            model.path_of(d.logic),
                            net.name
                        ),
                    );
                }
            }
        }
    }

    /// The high of a line that a plain `open_drain` pin pulls low: what the DC analysis holds
    /// it at, else its pull-ups, measured from the receiver's ground, against the receiver's
    /// `vih`. A high that cannot be read is a note.
    fn plain_open_drain_high(&mut self, index: usize, p: &str, part: InstId, r: &Line) {
        let model = self.model;
        let net = &model.nets()[index];
        let (Some(vih), Some(rg)) = (self.value(r.logic, "vih"), self.ground_pin(r.logic)) else {
            return;
        };
        let why = "an `open_drain` pin";
        if let Some(&(ground, lo, hi)) = self.reads.and_then(|reads| reads.high.get(&index))
            && self.net_index(rg) == Some(ground)
        {
            self.held_high(net, r, vih, (lo, hi), |high| {
                format!(
                    "the DC analysis holds the line at {high} with every driver of it high impedance ({why} leaves the high to its circuit)"
                )
            });
            return;
        }
        let why_not = match self.pullups(net, rg) {
            Ok(pulls) if !pulls.is_empty() => {
                self.pulled_high(net, r, vih, &pulls, why);
                return;
            }
            Ok(_) => "no resistor on it pulls it up to a rail".to_string(),
            Err(why) => why,
        };
        self.note(
            part,
            vec![part],
            Cause::Unjudged,
            format!(
                "not evaluated: the high level from `{p}` to `{}` on net `{}`: `{p}` is an `open_drain` pin and the pull-up that would set its high cannot be read: {why_not}",
                model.path_of(r.logic),
                net.name
            ),
        );
    }

    /// Whether something on a net can drive it: a pin of a driving kind, not on an interface the
    /// board states is unused.
    fn drives(&self, net: &Net) -> bool {
        let model = self.model;
        net.terminals
            .iter()
            .any(|&t| model.unused_by(t).is_none() && model.pin_kind(t).is_some_and(is_driver))
    }

    /// The series parts a receiver's level comes through (#850): when nothing on the net drives
    /// it and a receiver on it states `vih` or `vil`, each part with a `bridge` from the net to
    /// another net that something drives, with that net.
    fn series_sources(&self, index: usize, lines: &[Line]) -> Vec<(InstId, usize)> {
        let model = self.model;
        let net = &model.nets()[index];
        if self.drives(net)
            || self.holder(index).is_some()
            || !lines
                .iter()
                .any(|l| is_receiver(l.kind) && self.states_input_levels(l.logic))
        {
            return Vec::new();
        }
        let mut out = Vec::new();
        for &t in &net.terminals {
            let Some((part, far)) = self.far_end(t, index) else {
                continue;
            };
            if self.drives(&model.nets()[far]) && !out.contains(&(part, far)) {
                out.push((part, far));
            }
        }
        out
    }

    /// The part of a pin, when it is a part with a `bridge` and the pin is one end of it, and
    /// the net of the other end, when that is another net.
    fn far_end(&self, pin: InstId, index: usize) -> Option<(InstId, usize)> {
        let model = self.model;
        let part = model.owner_of(pin)?;
        let (a, b) = model.instance(part).bridge?;
        let far = match pin {
            t if t == a => b,
            t if t == b => a,
            _ => return None,
        };
        let far = self.net_index(far).filter(|&f| f != index)?;
        Some((part, far))
    }

    /// A receiver that states levels on a net that nothing on it drives, joined through a
    /// series part (a part with a `bridge`: a resistor, a diode, a transistor's channel) to a
    /// net that something drives (#850). Where the DC analysis solved the receiver's net with a
    /// `Logic` driver of the far net held high and held low ([`NetReads::driven`]), the receiver
    /// is judged by the voltage it gives the net, measured from the receiver's ground. A driver
    /// the analysis does not model, behind a resistor into a net that nothing else is on, is
    /// judged as a pair on the far net (#1045): no current flows through the resistor, so the
    /// two nets are at one voltage. For any other driver the note names the part and the net the
    /// level comes from. A net that has a driver of its own is judged on its own lines, as
    /// above, and a part that only pulls the net to a rail or to ground has no driver on its far
    /// side.
    fn through_series(&mut self, index: usize, lines: &[Line]) {
        let model = self.model;
        let net = &model.nets()[index];
        let receivers: Vec<&Line> = lines
            .iter()
            .filter(|l| is_receiver(l.kind) && self.states_input_levels(l.logic))
            .collect();
        for (part, far) in self.series_sources(index, lines) {
            let from = &model.nets()[far];
            let far_lines = self.lines_on(from);
            let drivers: Vec<InstId> = from
                .terminals
                .iter()
                .copied()
                .filter(|&t| model.unused_by(t).is_none())
                .filter(|&t| model.pin_kind(t).is_some_and(is_driver))
                .collect();
            let unloaded = self.unloaded_through(index, part);
            for r in &receivers {
                if r.part == part {
                    continue;
                }
                for &t in &drivers {
                    if let Some(d) = far_lines.iter().find(|l| l.pin == t) {
                        if self.judged_across(net, d, r, part, from, index, false) {
                            continue;
                        }
                        if unloaded {
                            self.pair(from, d, r);
                            continue;
                        }
                        if let Some(div) = self
                            .ground_pin(r.logic)
                            .and_then(|g| self.divider(index, part, g))
                            && self.through_divider(net, d, r, from, &div)
                        {
                            continue;
                        }
                    }
                    self.note(
                        r.part,
                        Vec::new(),
                        Cause::Unjudged,
                        format!(
                            "not evaluated: the logic levels into `{}` on net `{}`: nothing on the net drives it, and its level comes through `{}` from net `{}`, which the levels check does not follow",
                            model.path_of(r.logic),
                            net.name,
                            model.path_of(part),
                            from.name
                        ),
                    );
                }
            }
        }
        self.across_series(index, lines);
    }

    /// Whether a net is one that nothing draws from but its receivers, joined to the net of its
    /// level by a resistor (#1045): every pin on it is the resistor's or an `input` that states
    /// no leakage (or a leakage of nothing), so no current flows through the resistor and the
    /// net is at the voltage of the other end.
    fn unloaded_through(&self, index: usize, part: InstId) -> bool {
        let model = self.model;
        let net = &model.nets()[index];
        let resistor = net
            .terminals
            .iter()
            .any(|&t| model.owner_of(t) == Some(part) && self.resistor_far_end(t).is_some());
        resistor
            && net.terminals.iter().all(|&t| {
                if model.owner_of(t) == Some(part) {
                    return true;
                }
                model.pin_kind(t) == Some(PinKind::Input)
                    && self.logic_of(t).is_some_and(|logic| {
                        self.value(logic, "ii").is_none_or(|(_, q)| q.hi() <= 0.0)
                    })
            })
    }

    /// Whether a pin draws nothing from its net: an `input` that states no leakage, or a
    /// leakage of nothing (#1045, #1270).
    fn draws_nothing(&self, pin: InstId) -> bool {
        self.model.pin_kind(pin) == Some(PinKind::Input)
            && self
                .logic_of(pin)
                .is_none_or(|logic| self.value(logic, "ii").is_none_or(|(_, q)| q.hi() <= 0.0))
    }

    /// The two-resistor divider a net is the middle of (#1270): `series`, a resistor from a net
    /// that something drives, and one other resistor from the net to a rail or a ground whose
    /// ground is `ground`, and on the net nothing else but pins that draw nothing. None for any
    /// other network, a resistance with no value, and a rail with no voltage.
    fn divider(&self, index: usize, series: InstId, ground: InstId) -> Option<Divider> {
        let model = self.model;
        let net = &model.nets()[index];
        if self.drives(net) || self.holder(index).is_some() {
            return None;
        }
        let mut series_seen = false;
        let mut shunt: Option<(InstId, usize)> = None;
        for &t in &net.terminals {
            if model.unused_by(t).is_some() {
                continue;
            }
            let far = self.resistor_far_end(t).and_then(|f| self.net_index(f));
            let part = model.owner_of(t)?;
            match far {
                Some(_) if part == series => series_seen = true,
                Some(far) if far != index && shunt.is_none() => shunt = Some((part, far)),
                Some(_) => return None,
                None if part == series => return None,
                None if self.draws_nothing(t) => {}
                None => return None,
            }
        }
        let (shunt, far) = shunt.filter(|_| series_seen)?;
        let end = match self.holder(far)? {
            Holder::Ground { neg } if self.same_ground(neg, ground) => Rail {
                range: (0.0, 0.0),
                name: format!("ground `{}`", model.nets()[far].name),
            },
            Holder::Rail { path, voltage, neg } if self.same_ground(neg, ground) => {
                if self.net_index(ground) == Some(far) {
                    return None;
                }
                let Domain::Quantity(q) = model.field(voltage).domain else {
                    return None;
                };
                Rail {
                    range: (q.lo(), q.hi()),
                    name: format!("rail `{path}`"),
                }
            }
            _ => return None,
        };
        let ohms = |part: InstId| {
            let (_, q) = self.value(part, "resistance")?;
            (q.lo() > 0.0 && q.hi().is_finite()).then_some((q.lo(), q.hi()))
        };
        Some(Divider {
            series,
            shunt,
            rs: ohms(series)?,
            rsh: ohms(shunt)?,
            end,
        })
    }

    /// The divider of a net with no driver of its own, behind the one resistor whose far net
    /// something drives, for its absolute maximum (#1270): the divider and the net it is fed from.
    fn divider_of(&self, index: usize, ground: InstId) -> Option<(Divider, usize)> {
        let model = self.model;
        let mut fed: Option<(InstId, usize)> = None;
        for &t in &model.nets()[index].terminals {
            if self.resistor_far_end(t).is_none() {
                continue;
            }
            let Some((part, far)) = self.far_end(t, index) else {
                continue;
            };
            if self.drives(&model.nets()[far]) && self.holder(far).is_none() {
                if fed.is_some_and(|f| f != (part, far)) {
                    return None;
                }
                fed = Some((part, far));
            }
        }
        let (part, far) = fed?;
        Some((self.divider(index, part, ground)?, far))
    }

    /// A receiver behind a two-resistor divider from a `Logic` driver the DC analysis does not
    /// model (#1270): the driver is a source at its own levels, behind its `roh` or `rol` when it
    /// states one, and the receiver's `vih` is held against the lowest voltage the divider gives
    /// with the driver high, its `vil` against the highest with the driver low, over the
    /// tolerances of both resistors and the range of the rail. An open-drain driver keeps the
    /// note (false), as does a driver on another ground.
    fn through_divider(
        &mut self,
        net: &Net,
        d: &Line,
        r: &Line,
        from: &Net,
        div: &Divider,
    ) -> bool {
        let model = self.model;
        if d.kind == PinKind::OpenDrain
            || (self.value(d.logic, "voh").is_none() && self.value(d.logic, "vol").is_some())
        {
            return false;
        }
        let (Some(dg), Some(rg)) = (self.ground_pin(d.logic), self.ground_pin(r.logic)) else {
            return false;
        };
        if !self.same_ground(dg, rg) {
            return false;
        }
        let (dl, rl) = (model.path_of(d.logic), model.path_of(r.logic));
        let source = |name: &str| {
            self.value(d.logic, name)
                .map_or((0.0, 0.0), |(_, q)| (q.lo().max(0.0), q.hi()))
        };
        let (roh, rol) = (source("roh"), source("rol"));
        let words = format!(
            "the divider of `{}` from net `{}` and `{}` to {}",
            model.path_of(div.series),
            from.name,
            model.path_of(div.shunt),
            div.end.name
        );
        match (
            self.output_level(d.logic, "voh"),
            self.value(r.logic, "vih"),
        ) {
            (Some((_, voh, _)), Some(vih)) => {
                let range = div.range((voh.lo(), voh.hi()), roh);
                self.held_high(net, r, vih, range, |v| {
                    format!("with `{dl}` driving high, {words} holds the line at {v}")
                });
            }
            (o, i) => self.missing(
                net,
                d,
                r,
                "high",
                [(&dl, "voh", o.is_none()), (&rl, "vih", i.is_none())],
            ),
        }
        match (
            self.output_level(d.logic, "vol"),
            self.value(r.logic, "vil"),
        ) {
            (Some((_, vol, _)), Some(vil)) => {
                let range = div.range((vol.lo(), vol.hi()), rol);
                self.held_low(net, r, vil, range, |v| {
                    format!("with `{dl}` driving low, {words} holds the line at {v}")
                });
            }
            (o, i) => self.missing(
                net,
                d,
                r,
                "low",
                [(&dl, "vol", o.is_none()), (&rl, "vil", i.is_none())],
            ),
        }
        true
    }

    /// A receiver judged by the voltage the DC analysis gives its net with a driver behind a
    /// series part held high and held low, and, with `let_go`, every driver of the receiver's
    /// own net letting go of it (#1045): the lowest high against its `vih`, the highest low
    /// against its `vil`. A level the receiver does not state is a note, as for a pair. False
    /// when the analysis has no such reading (the check is the caller's).
    #[allow(clippy::too_many_arguments)]
    fn judged_across(
        &mut self,
        net: &Net,
        d: &Line,
        r: &Line,
        part: InstId,
        from: &Net,
        index: usize,
        let_go: bool,
    ) -> bool {
        let model = self.model;
        let ground = self.ground_pin(r.logic).and_then(|g| self.net_index(g));
        let Some(driven) = self
            .reads
            .and_then(|reads| reads.driven.get(&(d.pin, index)))
            .filter(|x| Some(x.ground) == ground)
            .cloned()
        else {
            return false;
        };
        let (dl, rl) = (model.path_of(d.logic), model.path_of(r.logic));
        let across = Across {
            part,
            through: format!("through `{}` from net `{}`", model.path_of(part), from.name),
        };
        let near = if let_go {
            format!(" and every driver of net `{}` letting go of it", net.name)
        } else {
            String::new()
        };
        let on = driven.on.iter().find(|o| o.0 == part);
        let (on_high, on_low) = (on.and_then(|o| o.1), on.and_then(|o| o.2));
        match self.value(r.logic, "vih") {
            Some(vih) => {
                let state = if driven.idle {
                    let nets = if let_go {
                        format!("nets `{}` and `{}`", from.name, net.name)
                    } else {
                        format!("net `{}`", from.name)
                    };
                    format!(
                        "with every driver of {nets} letting go ({} leaves the high to its circuit)",
                        open_drain_why(d)
                    )
                } else {
                    format!("with `{dl}` driving high{near}")
                };
                self.judge_level(net, r, vih, true, (driven.high, on_high), &state, &across);
            }
            None => self.missing(net, d, r, "high", [(&dl, "voh", false), (&rl, "vih", true)]),
        }
        match self.value(r.logic, "vil") {
            Some(vil) => {
                let state = format!("with `{dl}` driving low{near}");
                self.judge_level(net, r, vil, false, (driven.low, on_low), &state, &across);
            }
            None => self.missing(net, d, r, "low", [(&dl, "vol", false), (&rl, "vil", true)]),
        }
        true
    }

    /// One level of a receiver behind a series part against the range the DC analysis holds its
    /// net at in one state (`full`), and, for a MOSFET between, the range with its channel held
    /// on (`on`). A range the receiver reads holds. When only the range with the MOSFET on is
    /// read, the states that fail are ones in which the analysis finds it off, which a body
    /// diode the analysis does not model may rule out: a note, not a fault (#1045).
    #[allow(clippy::too_many_arguments)]
    fn judge_level(
        &mut self,
        net: &Net,
        r: &Line,
        (fi, limit): (FieldId, Quantity),
        high: bool,
        (full, on): ((f64, f64), Option<(f64, f64)>),
        state: &str,
        across: &Across,
    ) {
        let model = self.model;
        let holds = |(lo, hi): (f64, f64)| {
            if high {
                lo >= limit.hi()
            } else {
                hi <= limit.lo()
            }
        };
        if holds(full) {
            return;
        }
        let through = &across.through;
        let p = model.path_of(across.part);
        let report = |me: &mut Self, range: (f64, f64), words: String| {
            let holder =
                |v: &str| format!("{words}, the DC analysis holds the line at {v} {through}");
            if high {
                me.held_high(net, r, (fi, limit), range, holder);
            } else {
                me.held_low(net, r, (fi, limit), range, holder);
            }
        };
        match on {
            Some(on) if holds(on) => {
                let level = if high { "high" } else { "low" };
                self.note(
                    r.part,
                    vec![across.part],
                    Cause::Unjudged,
                    format!(
                        "not evaluated: the {level} level into `{}` on net `{}` {through} {state}: with `{p}` on, the DC analysis holds the line at {}, which `{}` reads as a {level}, but in some states it finds `{p}` off and the line at {}; whether `{p}` turns on there rests on its body diode, which the analysis models only when `{p}.body_diode_voltage` states its forward voltage",
                        model.path_of(r.logic),
                        net.name,
                        range_text(on),
                        model.path_of_field(fi),
                        range_text(full),
                    ),
                );
            }
            Some(on) => report(self, on, format!("{state} and `{p}` on")),
            None => report(self, full, state.to_string()),
        }
    }

    /// The parts a level crosses into a net whose own drivers can all let go of it (#850): when
    /// a pin on the net can drive it, none is an `output`, and a receiver on it states `vih` or
    /// `vil`, each part with a `bridge` from the net to another net, with that net.
    fn crossings(&self, index: usize, lines: &[Line]) -> Vec<(InstId, usize)> {
        let model = self.model;
        let net = &model.nets()[index];
        let own: Vec<PinKind> = net
            .terminals
            .iter()
            .filter(|&&t| model.unused_by(t).is_none())
            .filter_map(|&t| model.pin_kind(t))
            .filter(|&k| is_driver(k))
            .collect();
        if own.is_empty()
            || own.contains(&PinKind::Output)
            || self.holder(index).is_some()
            || !lines
                .iter()
                .any(|l| is_receiver(l.kind) && self.states_input_levels(l.logic))
        {
            return Vec::new();
        }
        let mut parts: Vec<(InstId, usize)> = Vec::new();
        for &t in &net.terminals {
            if let Some(found) = self.far_end(t, index)
                && !parts.contains(&found)
            {
                parts.push(found);
            }
        }
        parts
    }

    /// A level that crosses a series part into a net whose own drivers can all let go of it
    /// (#850, #1045): the low that one side of a MOSFET translator between two buses pulls the
    /// other side to, or the level of a bidirectional pin behind a series resistor. Each side's
    /// own lines are judged on that side. What comes across is judged where the DC analysis
    /// solved the two nets with the far driver held high and held low and every driver of the
    /// near net letting go ([`NetReads::driven`]); for the rest the note names the part, the
    /// drivers on the far side and the receivers on the near side. A net with a push-pull
    /// `output` on it is held by that output, and is not looked at.
    fn across_series(&mut self, index: usize, lines: &[Line]) {
        let model = self.model;
        let net = &model.nets()[index];
        let receivers: Vec<&Line> = lines
            .iter()
            .filter(|l| is_receiver(l.kind) && self.states_input_levels(l.logic))
            .collect();
        for (part, far) in self.crossings(index, lines) {
            let from = &model.nets()[far];
            let far_lines = self.lines_on(from);
            let mut drivers: Vec<InstId> = Vec::new();
            let mut readers: Vec<InstId> = Vec::new();
            let far_drivers: Vec<&Line> = far_lines
                .iter()
                .filter(|d| is_driver(d.kind) && d.part != part)
                .filter(|d| self.states_output_levels(d.logic))
                .collect();
            for d in far_drivers {
                for r in receivers.iter().filter(|r| r.part != part) {
                    if !self.judged_across(net, d, r, part, from, index, true) {
                        drivers.push(d.logic);
                        readers.push(r.logic);
                    }
                }
            }
            for v in [&mut drivers, &mut readers] {
                v.sort_unstable();
                v.dedup();
            }
            if drivers.is_empty() || readers.is_empty() {
                continue;
            }
            let names = |ls: &[InstId]| {
                ls.iter()
                    .map(|&l| format!("`{}`", model.path_of(l)))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            self.note(
                part,
                Vec::new(),
                Cause::Unjudged,
                format!(
                    "not evaluated: the logic levels that cross `{}` from net `{}` into net `{}`: every driver of net `{}` can let go of it, and then the level that {} drive{} reaches {} through `{}`, which the levels check does not follow",
                    model.path_of(part),
                    from.name,
                    net.name,
                    net.name,
                    names(&drivers),
                    if drivers.len() == 1 { "s" } else { "" },
                    names(&readers),
                    model.path_of(part),
                ),
            );
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
                Cause::Unjudged,
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
        // highest threshold. An `open_drain` pin, or an output that states a low and no high,
        // is open drain: its high is the one its pull-up gives the line (ADR 0075, #460).
        let voh = self.output_level(d.logic, "voh");
        if d.kind == PinKind::OpenDrain
            || (self.value(d.logic, "voh").is_none() && self.value(d.logic, "vol").is_some())
        {
            // The DC analysis solved the line's circuit: its voltage with every open-drain
            // driver let go is the high, whatever sits between the rail and the line.
            if let Some(&(ground, lo, hi)) = self
                .net_index(d.pin)
                .and_then(|n| self.reads.and_then(|r| r.high.get(&n)))
                && self.net_index(dg) == Some(ground)
            {
                self.high_from_analysis(net, d, r, (lo, hi));
                self.low(net, d, r);
                return;
            }
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
                        Cause::Unjudged,
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
            (Some((fo, voh, derived)), Some((fi, vih))) => {
                let out = self.level_text(d.logic, "voh", fo, derived);
                if let Some((fd, fr)) = self.correlated(fo, fi, d, r).filter(|_| !derived) {
                    // One supply voltage: `k_d * V >= k_r * V` for every `V` above zero is
                    // `k_d >= k_r`, and nothing looser (ADR 0082).
                    if fd.factor < fr.factor {
                        self.fault(
                            d.part,
                            format!(
                                "net `{}`: {out} is {} x `{}` but `{}` needs {} x `{}`, the same supply voltage: a high of {} x the supply is not always read as a high, which takes {} x the supply",
                                net.name,
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
                            "net `{}`: {out} is {voh} but `{}` needs {vih}: a high of {} is not always read as a high, which takes {}",
                            net.name,
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

    /// The high of an open-drain line as the DC analysis solved it: the voltage of the net with
    /// every driver of it high impedance, in the worst case of the ranges of the rails and the
    /// parts between them and the line. It must be read as a high by the receiver.
    fn high_from_analysis(&mut self, net: &Net, d: &Line, r: &Line, (lo, hi): (f64, f64)) {
        let model = self.model;
        let Some(vih) = self.value(r.logic, "vih") else {
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
        let why = open_drain_why(d);
        self.held_high(net, r, vih, (lo, hi), |high| {
            format!(
                "the DC analysis holds the line at {high} with every driver of it high impedance ({why} leaves the high to its circuit)"
            )
        });
    }

    /// The high of an open-drain line is what its pull-up resistors hold it at: it must be
    /// read as a high by the receiver, in the worst case of the rail's range.
    fn high_from_pullups(&mut self, net: &Net, d: &Line, r: &Line, pulls: &[PullUp]) {
        let model = self.model;
        let Some(vih) = self.value(r.logic, "vih") else {
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
        self.pulled_high(net, r, vih, pulls, open_drain_why(d));
    }

    /// The pull-ups' high against a receiver's `vih`: the lowest of the rails that pull the
    /// line up is its lowest high.
    fn pulled_high(
        &mut self,
        net: &Net,
        r: &Line,
        vih: (FieldId, Quantity),
        pulls: &[PullUp],
        why: &str,
    ) {
        let Some(low) = pulls.iter().min_by(|a, b| a.lo.total_cmp(&b.lo)) else {
            return;
        };
        let (lo, hi) = pulls
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.lo), hi.max(p.hi))
            });
        let (resistor, rail) = (low.resistor.clone(), low.rail.clone());
        self.held_high(net, r, vih, (lo, hi), |high| {
            format!(
                "the pull-up `{resistor}` to rail `{rail}` holds the line at {high} ({why} leaves the high to the pull-up)"
            )
        });
    }

    /// The fault of a high that the circuit around an open-drain line holds it at, `lo` to
    /// `hi`, when it is not always read as a high by the receiver. `holder` says what holds
    /// it there, given the range as text.
    fn held_high(
        &mut self,
        net: &Net,
        r: &Line,
        (fi, vih): (FieldId, Quantity),
        (lo, hi): (f64, f64),
        holder: impl FnOnce(&str) -> String,
    ) {
        if lo >= vih.hi() {
            return;
        }
        let point = |v| Quantity::point(Dimension::VOLTAGE, v);
        let high = match (point(lo), point(hi)) {
            (Ok(a), Ok(b)) => Quantity::range(&a, &b).map_or_else(|_| volts(lo), |q| q.to_string()),
            _ => volts(lo),
        };
        let message = format!(
            "net `{}`: {} but `{}` needs {vih}: a high of {} is not always read as a high, which takes {}",
            net.name,
            holder(&high),
            self.model.path_of_field(fi),
            volts(lo),
            volts(vih.hi()),
        );
        self.fault(r.part, message);
    }

    /// The fault of a low that the circuit holds a line at, `lo` to `hi`, when it is not always
    /// read as a low by the receiver. `holder` says what holds it there, given the range as text.
    fn held_low(
        &mut self,
        net: &Net,
        r: &Line,
        (fi, vil): (FieldId, Quantity),
        (lo, hi): (f64, f64),
        holder: impl FnOnce(&str) -> String,
    ) {
        if hi <= vil.lo() {
            return;
        }
        let point = |v| Quantity::point(Dimension::VOLTAGE, v);
        let low = match (point(lo), point(hi)) {
            (Ok(a), Ok(b)) => Quantity::range(&a, &b).map_or_else(|_| volts(hi), |q| q.to_string()),
            _ => volts(hi),
        };
        let message = format!(
            "net `{}`: {} but `{}` allows {vil}: a low of {} is not always read as a low, which takes {}",
            net.name,
            holder(&low),
            self.model.path_of_field(fi),
            volts(hi),
            volts(vil.lo()),
        );
        self.fault(r.part, message);
    }

    /// A low must be read as a low: the output's highest low against the input's lowest
    /// threshold.
    fn low(&mut self, net: &Net, d: &Line, r: &Line) {
        let model = self.model;
        let (dl, rl) = (model.path_of(d.logic), model.path_of(r.logic));
        // A low must be read as a low.
        match (
            self.output_level(d.logic, "vol"),
            self.value(r.logic, "vil"),
        ) {
            (Some((fo, vol, derived)), Some((fi, vil))) => {
                let out = self.level_text(d.logic, "vol", fo, derived);
                if let Some((fd, fr)) = self.correlated(fo, fi, d, r).filter(|_| !derived) {
                    // The same reduction for the low: `k_d * V <= k_r * V` is `k_d <= k_r`.
                    if fd.factor > fr.factor {
                        self.fault(
                            d.part,
                            format!(
                                "net `{}`: {out} is {} x `{}` but `{}` allows {} x `{}`, the same supply voltage: a low of {} x the supply is not always read as a low, which takes {} x the supply",
                                net.name,
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
                            "net `{}`: {out} is {vol} but `{}` allows {vil}: a low of {} is not always read as a low, which takes {}",
                            net.name,
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
        let why: String = [d.logic, r.logic]
            .into_iter()
            .zip(need.iter())
            .filter(|(_, (_, _, none))| *none)
            .map(|(logic, (_, name, _))| self.unread_why(logic, name))
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
            Cause::PartModel,
            format!(
                "not evaluated: the {which} level from `{}` to `{}` on net `{}`: {} {} no value{why}",
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
            if model.unused_by(pin).is_some() {
                continue;
            }
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
            // A pin that nothing outside its own part is joined to (an optional pin left to
            // float) has no voltage to judge, and a note that nothing sets it says nothing.
            if model.nets()[index]
                .pins
                .iter()
                .all(|&p| model.is_inside(p, owner))
            {
                continue;
            }
            let path = model.path_of(pin);
            // The ground is the `Logic`'s or the bus's, else the one the pin row names or
            // takes from its table (ADR 0124).
            let ground = match self.logic_of(pin) {
                Some(logic) => self.ground_pin(logic),
                None => match model.limit_reference(pin) {
                    Some(reference) => Some(reference),
                    None => {
                        self.note(
                            owner,
                            vec![owner],
                            Cause::PartModel,
                            format!(
                                "not evaluated: the absolute maximum of `{path}`: it is not the `line` of a `Logic` or a pin of an interface or a part with `levels`, and its pin row names no ground, so its ground is not known"
                            ),
                        );
                        continue;
                    }
                },
            };
            let Some(ground) = ground else {
                continue;
            };
            // A net the DC analysis solved has the voltage the circuit around it makes of it
            // (amendment 12), which is the worst case of the whole network and not of one step of
            // it: a divider's middle is not the rail it hangs from.
            let swing = match self
                .node_range(index, ground)
                .map_or_else(|| self.divided_range(index, ground), Ok)
            {
                Ok(range) => range,
                Err(why) => {
                    self.note(
                        owner,
                        vec![owner],
                        Cause::Unjudged,
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

    /// The range the DC analysis found for a net, measured from the same ground as the pin.
    fn node_range(&self, index: usize, ground: InstId) -> Option<Swing> {
        let &(g, lo, hi) = self.reads?.volts.get(&index)?;
        if self.net_index(ground) != Some(g) {
            return None;
        }
        let from = "the DC analysis of the network it is in";
        let mut swing = Swing::none();
        swing.add(lo, hi, from);
        Some(swing)
    }

    /// The worst-case voltage of a net that is the middle of a two-resistor divider (#1270): the
    /// divider of the swing of the net it is fed from, with the driver taken as a source of no
    /// resistance, over the tolerances of the resistors and the range of the rail. Any other net
    /// is `net_range`'s.
    fn divided_range(&self, index: usize, ground: InstId) -> Result<Swing, String> {
        let Some((div, far)) = self.divider_of(index, ground) else {
            return self.net_range(index, ground);
        };
        let fed = self.net_range(far, ground)?;
        let (lo, hi) = div.range((fed.lo, fed.hi), (0.0, 0.0));
        let model = self.model;
        let mut swing = Swing::none();
        swing.add(
            lo,
            hi,
            &format!(
                "the divider of `{}` from net `{}` and `{}` to {}",
                model.path_of(div.series),
                model.nets()[far].name,
                model.path_of(div.shunt),
                div.end.name
            ),
        );
        Ok(swing)
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
        // A net that a rail or a ground holds is at that voltage, whatever resistors join it to
        // other rails (#1304): a resistor from a rail to ground does not lift the ground.
        let held = self.holder(index).is_some();
        for &t in &net.terminals {
            // A resistor to a rail or to ground pulls the net there: a pull-up or a
            // pull-down.
            if let Some(far) = self.resistor_far_end(t)
                && let Some(far_net) = self.net_index(far)
            {
                if !held {
                    self.pull(far_net, ground, &mut swing)?;
                }
                continue;
            }
            let Some(kind) = model.pin_kind(t) else {
                continue;
            };
            if !is_driver(kind) {
                continue;
            }
            let path = model.path_of(t);
            // An open-drain pin only pulls the line to its ground; its high is the pull-up's,
            // which the resistors above add (#460).
            if kind == PinKind::OpenDrain {
                swing.add(
                    0.0,
                    0.0,
                    &format!("`{path}`, an open_drain pin, which only pulls the line low"),
                );
                continue;
            }
            let Some(logic) = self.logic_of(t) else {
                return Err(format!(
                    "`{path}` can drive it and is not the `line` of a `Logic` or a pin of an interface or a part with `levels`, so how far it swings is not known"
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
            // A line that states its own swing (ADR 0081) is held to it, as written and
            // relative to its ground; one that does not swings from the ground to the top of
            // its reference rail.
            if let Some((_, q)) = self.value(logic, "swing") {
                swing.add(
                    q.lo(),
                    q.hi(),
                    &format!("`{path}`, a line that states a swing of {q}"),
                );
                continue;
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

/// What holds a net at a voltage of its own (#1304).
enum Holder {
    /// The positive pin of a rail: its path, its voltage field and its ground pin.
    Rail {
        path: String,
        voltage: FieldId,
        neg: InstId,
    },
    /// The negative pin of a rail, and no positive one.
    Ground { neg: InstId },
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

/// A two-resistor divider (#1270): a resistor from a driven net into the middle, and one from the
/// middle to a rail or a ground.
struct Divider {
    series: InstId,
    shunt: InstId,
    /// The range of each resistance, in ohms.
    rs: (f64, f64),
    rsh: (f64, f64),
    /// What the shunt resistor's far end is held at.
    end: Rail,
}

/// The voltage a divider's shunt resistor ends at, and the words for it.
struct Rail {
    range: (f64, f64),
    name: String,
}

impl Divider {
    /// The range of the middle with the far end of the series resistor at `fed` behind a source
    /// resistance of `source` ohms: `v = e + (f - e) * rsh / (rs + source + rsh)` is monotonic in
    /// each of the four, so its extremes are at the corners of their ranges.
    fn range(&self, fed: (f64, f64), source: (f64, f64)) -> (f64, f64) {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for f in [fed.0, fed.1] {
            for e in [self.end.range.0, self.end.range.1] {
                for rs in [self.rs.0 + source.0, self.rs.1 + source.1] {
                    for rsh in [self.rsh.0, self.rsh.1] {
                        let v = e + (f - e) * rsh / (rs + rsh);
                        lo = lo.min(v);
                        hi = hi.max(v);
                    }
                }
            }
        }
        (lo, hi)
    }
}

/// The series part a level comes through, and the words for it.
struct Across {
    part: InstId,
    through: String,
}

/// A range of voltages as a message gives it (`1.477V..1.75V`), or one voltage.
fn range_text((lo, hi): (f64, f64)) -> String {
    let point = |v| Quantity::point(Dimension::VOLTAGE, v);
    match (point(lo), point(hi)) {
        (Ok(a), Ok(b)) => Quantity::range(&a, &b).map_or_else(|_| volts(lo), |q| q.to_string()),
        _ => volts(lo),
    }
}

fn volts(v: f64) -> String {
    Quantity::point(Dimension::VOLTAGE, v).map_or_else(|_| format!("{v} V"), |q| q.to_string())
}
