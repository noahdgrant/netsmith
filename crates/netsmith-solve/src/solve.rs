//! Choosing real parts and checking that the design works in the worst case.
//!
//! The solver does four things, in order:
//!
//! 1. **Propagate.** Connected interfaces hold one value, so equal fields are narrowed
//!    to what they all allow. A field defined by an equation whose inputs are known takes
//!    that value. At each net the currents of the rails, and of the chains of parts that
//!    start there, add up to zero, so when only one of them is unknown it is the negative
//!    of the rest.
//! 2. **Pick.** For each part with a catalog and an unset catalog column, take candidates
//!    in catalog order, skip those that do not fit what the design already says, and use
//!    the first one for which every relation that mentions the part holds.
//! 3. **Verify.** Check every relation again with the picks in place.
//! 4. **Report.** Whatever could not be evaluated is deferred, and whatever fails is
//!    reported with the numbers.
//!
//! A relation holds in the worst case: a requirement on an outcome must contain the whole
//! range the inputs can produce, not just a typical value. Parts that share a relation are
//! picked together by a depth-first search over their candidates (parts in path order,
//! candidates in catalog order): a choice that makes a later part impossible is undone and
//! the next candidate is tried, up to a fixed number of trials. A failure remembers which
//! earlier parts it depends on, and the search goes back to the nearest of them rather than
//! choosing the parts in between again (conflict-directed backjumping).
//!
//! # Why the pick is judged as it is (ADR 0057, ADR 0065)
//!
//! Each rule below came from a real board for which the solver chose parts without something
//! the design had said, or gave up without saying why. The guide ("Catalogs and how the solver
//! picks parts") says what a user sees; this says why, and what was left out. The check after
//! the picks is always the judge: a rule here only turns a candidate away earlier, never lets
//! one through.
//!
//! **A bound on a value worked out from a part takes part in the pick.** `try_candidate` judged
//! only the relations that name a field inside the part, so `.ratio >= 0.05`, with
//! `.ratio == .bottom.resistance / (.top.resistance + .bottom.resistance)`, was judged once
//! after every part was picked, and the pair that was picked failed it. Now a candidate is also
//! judged by a relation that names a value its choice made known or narrower, if no part still
//! to be chosen feeds a field the relation names. What feeds a relation (`Coupling::feeding`,
//! made once per solve) is found from the fields it names, through the left side of every
//! equation to what it reads, through every connection, through the other terms of a balance
//! of currents, and from a derated column (a capacitor's `effective_capacitance`) to its
//! `.bias`, to the columns of the parts. The last condition matters: while the other half of a
//! divider is not chosen, `.ratio` is a range of what it may still be, and a bound that fails on
//! it can hold for the part that is chosen. Rejected: judging at once, which refuses pairs that
//! the second part would have saved (a design that built could fail), and moving the bound onto
//! the resistors by rewriting the model, which changes the message and cannot follow an
//! equation through two named values.
//!
//! **A bound that waits still turns a candidate away when no value can meet it**
//! (`verify_possible`, ADR 0065 9.5): `lhs <= rhs` with the lowest value of `lhs` above the
//! highest of `rhs`, and the mirror for `>=`. Later picks only narrow ranges, so a bound that
//! holds for no value of a range holds for none of the narrower one. A range with no value yet
//! is not judged. Found when a buck stage's quiescent-current bound was refused after the
//! divider had been picked at its smallest sizes, because the stage's input voltage was fed by
//! parts of groups that come later. **Two tiers** (9.6, the owner's decision): a candidate
//! that `verify_possible` lets through and `verify` does not (the bound holds for some values
//! of the range, not all) is kept for a second tier, tried only when the first has nothing that
//! works. Both tiers go in catalog order, inside each count of a bank (`Accepted::whole`).
//!
//! **A value that two equations state is what the equations say, not an allowance** (9.1).
//! `.osc.fsw` was the left side of the module's `.fsw == .fsw_nominal * .fsw_tolerance` and of
//! the timer's `.osc.fsw == .fsw`, with a window stated on `.timer.fsw`. With a range on the
//! member that sets the resistor, the first equation could be worked out before any pick and
//! gave the field a wide range that no pick ever narrowed; the window was then never compared
//! with the pick, and the result depended on whether one unrelated range line was there. A
//! field with several equations, no statement that narrowed it, and a value a pick can change
//! (`several_equations`, `Solver::fed`) follows an equation whose range lies inside it, and
//! `verify` judges each of its equations against the others: an equation no pick changes (the
//! window) is the allowance, and what the others work out must lie inside it. With no pick able
//! to change any of them the old rule stands (a correction of 2026-11-06 made both halves test
//! that). Rejected: narrowing to the intersection of the equations everywhere, which would
//! report a range nobody wrote and pass partly overlapping ranges.
//!
//! **A generic inductor is held to what the stage asks of it.** Its saturation current is "the
//! least that any relation demands", but the relations were read before the part's marked
//! inductance had set the ripple, so `current + ripple / 2 <= saturation_current` could not be
//! evaluated and the generic took only its own `current <= saturation_current`. Now the values
//! that follow from a generic candidate's marked columns are worked out first (one
//! propagation) and the demand is read after them. The part takes the demand exactly, so the
//! stage's check holds with nothing to spare, and the demand goes with it to `resolve`, which
//! refuses the row that does not meet it. A stage whose load is over every row gets a generic
//! that no row stands for, and `fab` refuses it by name: a generic has no rating, so the demand
//! is a requirement on the row, as for a capacitor's voltage. Not done: the inductor's group does
//! not wait for its frequency resistor's group when the resistor's pick fails (`next_group`
//! orders groups by the picks a column needs, not by every value a check reads), because that
//! can change which solution is found first for designs that build.
//!
//! **A part nothing can serve is named at once, and parts that read only the same stated value
//! are searched apart.** The joint search gave up after 250000 combinations without a reason when
//! one part had no candidate whatever the others were: on an invented chain of ten dividers (20
//! resistors tied by one total and a bound that no resistor of the last stage meets) it took
//! 71.8 s and named all 20; it now ends in 0.4 s naming the one. Before a group of more than one
//! part is searched, `hopeless` tries each part's candidates alone. A part is the cause when
//! every candidate that fits is refused by a relation whose value only this part's choice
//! decides (the parts that feed it are this part alone) and none passes. A bank is tried at
//! every count its range holds (`counts_of`); a refusal that reads nothing worked out from the
//! count is the same at every count, so it costs one trial. A part refused for what other parts
//! decide, or by a conflict found while propagating, is not counted, and the search runs as it
//! did. Rejected: trying each part alone with every other part open and calling any failure a
//! cause, because a relation that fails on the wide range of a part not yet chosen can hold for
//! the part that is. Groups are made from the values two parts share, but a value no pick can
//! change (a field with a value, none of the fields joined to it worked out from anything,
//! none a column: `constants`) joins nothing, so two parts that read the same stated rail are
//! two groups and a failing group does not undo the picks of the other. Rejected: splitting
//! by classes of joined values only, since one equation between two parts joins their classes
//! and a power chain is held together by such equations. Not done: a bank that no count can
//! serve because of a value another part decides (a filter that sets the rail's voltage) is
//! still searched as before and can give up.
//!
//! **A failure names the picks it came from.** A failure used to depend on every part whose
//! column was in the class of values it read, and one equation over many parts (a total, a net
//! every part draws from) put them all in one class, so nothing was skipped and the message
//! named them all. `Coupling::fed_by` follows the edges of `feeding` the other way, from the
//! fields a rejection was worked out from to the columns they come from, and `Failure::core`
//! keeps the part, the earlier picks it depends on, the parts below it, and the parts still to
//! be chosen that feed what the relation read. A part that the search jumped over is not in it.
//! The message names the core when it is fewer parts than the group; `gave up` is the safety
//! net for a failure made of every part. The identity of a successful build is unchanged: the
//! first solution in catalog order. Soundness stands on the same dependence as `feeding`; the
//! gaps are a value worked out backwards (propagation does not do so) and a hidden read.
//! Rejected: recording a nogood for each failure (a check added to every trial), finding the
//! smallest set again by trying subsets of the picks (a failure on fewer picks does not imply
//! the failure on more), and naming every part that fed the failure (it hides the fault).
//!
//! **A bank of capacitors is searched by count, fewest first** (ADR 0052). A part whose `.count`
//! is a range is tried with every catalog row at one capacitor, then at two, and so on
//! (`counts_of`); the first count and row for which every relation that mentions the part holds
//! win, so a part one capacitor can serve is picked as before and the fewest parts make the
//! smallest bill of materials. Rejected: the row first and the count second (catalog order is the
//! whole policy, ADR 0047, and a bank of one must be what the stage did before), and each row at
//! every count before the next (it would pick five of a small row over two of a later, larger
//! one). To keep the search short, a candidate that failed a relation no count can change (a
//! rating below the rail's peak: the failing relation reads nothing worked out from the count)
//! is not tried at the next count, so a rating no row meets costs one trial for each row. The
//! most a count may be is `MAX_BANK` (64), whatever range a design states; the stage's own
//! range is 1 to 8, a round number above what a datasheet asks of a 5 V rail with the catalog's
//! parts, and one number in `std/parts/regulator.nsm`. When no count meets the requirement the
//! message names the nearest as `2 x <row>`; a relation that mentions a field of a part the pick
//! has refused (the output ripple reads the count) is not
//! judged again on the value the part was left with, because the part's own message says what
//! failed. The count is part of what a pick sets for the backjump (`Coupling.columns`): without
//! it a failure that read an earlier bank's count (a board value that is the sum of two banks'
//! counts) was recorded as depending on no earlier part, and the earlier bank's other counts
//! were skipped, which refused a board that has a solution. Skipping less is the sound
//! direction. A stated `.count` is a requirement: the pick tries counts up to the first
//! that meets it and equals it, and says what it tried if none does. Not modelled: the ripple
//! current a capacitor takes (the catalog has no ripple rating), so it is not divided between
//! the capacitors of a bank.
//!
//! **A search that runs away is counted in work as well as trials** (issue #177).
//! `SearchBudget::trials` (250000) was no limit for a big board, because a trial runs the
//! propagation twice and a pass reads every equation, relation and balance: 250000 trials of
//! eight parts took 1.8 s alone and 36 s with 1000 independent parts beside them (a trial of
//! about 7 microseconds against 145). `Solver::work` adds what each pass reads, one for each
//! relation a trial is judged against, one for each candidate a part's turn looks at, and one
//! for each earlier failed choice a candidate is compared with. A search stops at
//! `SearchBudget::work` (1000000000, about 20 s of a release build on that board), whichever
//! limit it reaches first. Both are counts, so the answer is the same on every machine; a
//! board of a dozen relations stays far from the work limit. The budget is an option of the
//! catalogs (`Catalogs::with_search_budget`), so a test lowers it; there is no manifest key
//! and no flag. Rejected: a time limit (the result would depend on the machine, and its test
//! would be flaky), a lower trial limit (a small board may need many trials), a limit over
//! every group together (many small groups do many trials in all, none a runaway). The number
//! was chosen from the invented board's speed, not from a real board's trial cost, and the
//! owner accepted it on 2026-11-11; if a real search stops sooner than a person waits, it comes
//! down. The hopeless scan and `search_again_together` have a budget each, so a group that
//! gives up and is searched again with another gets a second budget. A failing group does not
//! say which part to state a value for, because the search does not know it (not built; issue
//! #65).
//!
//! **The order of the groups must not decide the board** (9.3). Groups are picked in path
//! order, and a bound that reads values of two groups waits for the parts that feed it, so the
//! group that came first was picked without it, and its first fit could leave the second
//! nothing: named `.ref` the reference divider sorted first and the board built, named `.zref`
//! the tap divider did and the build failed with a message about the reference's parts. (The
//! report that instance names alone changed a result could not be reproduced; this is the
//! dependence on a name that was found.) `Mark` keeps the state before each group, and
//! `search_again_together` goes back to before the first earlier group that shares a relation
//! with a failed one and searches again with the failed group first. Parts that were found are
//! never searched again, so a board that built keeps its picks. Limits (not built; issue #65):
//! only the order "failed group first" is tried, not every order and not a joint search (a joint search of four
//! resistors from the 700 rows of the real catalog had not ended after two minutes in a debug
//! build). A `Mark` holds a `Bookmark` of the model, a place in its journal, not a copy of every
//! domain: one copy for each group made the memory grow with the square of the board (#569). A failed retry
//! puts back the diagnostics and the unresolved parts as well as the model and the picks (a
//! correction of 2026-11-06).
//!
//! `cycles.rs` has the open circles (ADR 0057) and `findings.rs` the unconstrained pick (ADR
//! 0065); `transfer.rs` has the bound that reaches a part (ADR 0065, stage 0); `narrow.rs` has
//! the skipping of candidates by blocks (issue #184), checked by `Catalogs::with_narrowing_audit`.
//!
//! # Where the code is
//!
//! This file has the public types, the state of a search (`Job`, `Mark`, `Search`, `Coupling`)
//! and `solve_model`, which runs the four steps. The steps are in the modules beside it:
//! `propagate.rs` (step 1), `candidates.rs` (judging one candidate), `search.rs` (the search of
//! one group), `coupling.rs` (which parts see each other's choices, and the groups), `pick.rs`
//! (step 2: the order of the groups and the second search), `hold.rs` (a part held to a lock or
//! a stated choice), `verify.rs` (step 3), `equations.rs` (what the model's equations say before
//! the solver starts), and `dcpass.rs`, `findings.rs`, `narrow.rs`, `reads.rs`, `seed.rs` and
//! `transfer.rs` as named above.
//!
//! # Where to read ADR 0057
//!
//! The first change of ADR 0057 changes which part is picked for some designs. The owner
//! accepted ADR 0057 as built on 2026-11-11, with its work limit. Where to read it:
//! `try_candidate` (the `changed` set and the condition that no part still to be chosen feeds the
//! relation: "the one place where a design that built could stop building"), `constants` (what
//! counts as a value no pick changes: a mistake there splits parts that are coupled), `hopeless`
//! (the condition for a part being the cause), `narrow.rs` and `search_group` (what a block's
//! refusal stands on, and the second search of a group that is not solved), and `SearchBudget`
//! in `catalog.rs` (the 1000000000 limit and what counts as a unit).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

mod candidates;
mod coupling;
mod dcpass;
mod equations;
mod findings;
mod hold;
mod narrow;
mod pick;
mod propagate;
mod reads;
mod search;
mod seed;
mod transfer;
mod verify;
pub use dcpass::DcSummary;
pub use findings::{HeldBy, UnconstrainedPick, UnstatedValue};
pub use seed::Seeded;

use netsmith_check::{
    Board, Bookmark, Diagnostic, FieldId, InstId, Model, RExpr, Relation, SourceId, System,
};
use netsmith_core::job;
use netsmith_lang::Span;
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::Quantity;

use crate::catalog::{Candidate, Catalog, Catalogs, DeratingError, Library};
use crate::eval::fields_in;
use crate::generic::Need;
use crate::lock::{ChosenRow, LockedRow};

use equations::{
    claimed_by_ranges, equations_of, several_equations, sole_equations, stated_currents,
    stated_late,
};

/// The result of solving one board.
#[derive(Debug)]
pub struct Solution {
    /// The board's model with every value the solver could settle filled in.
    pub model: Model,
    pub picks: Vec<Pick>,
    /// Parts with a catalog that were not picked, and why.
    pub unresolved: Vec<Unresolved>,
    /// Indices into `model.relations()` that could not be evaluated: something they
    /// depend on has no value.
    pub deferred: Vec<usize>,
    /// Groups of fields that depend on each other in a cycle while none has a value, so
    /// that nothing in them or after them could be worked out. Only a note says so: a range
    /// stated on one of the fields would start the solver.
    pub circular: Vec<Vec<FieldId>>,
    /// Circles of values (closed or not) with a check that reads them and could not be
    /// made, after the solver's own start (ADR 0053): the build says so as a finding, not
    /// only as a note (ADR 0057).
    pub open_circles: Vec<OpenCircle>,
    /// Circles of values that the solver started itself, from the range of a value the board
    /// states, because the part's own range holds no other value that its equations allow
    /// (ADR 0053).
    pub seeded: Vec<Seeded>,
    pub diagnostics: Vec<Diagnostic>,
    /// The project's default for who assembles a part (`fab`, `hand` or `none`), from the
    /// catalogs the board was solved with (ADR 0028). The design stage applies it.
    pub default_assembly: Option<String>,
    /// The logic level and absolute maximum checks that could not be made, and why
    /// (ADR 0046): never a pass.
    pub not_evaluated: Vec<crate::levels::NotEvaluated>,
    /// How many candidates the search tried in all, over every group of parts: the effort
    /// spent choosing parts. Tests use it to show that a failing design is not searched
    /// once per combination of parts that have nothing to do with the failure.
    pub trials: usize,
    /// The work those trials were, in the units of `SearchBudget::work`: what the budget of a
    /// search is counted in, because a trial on a large board is not the same effort as one on
    /// a small board.
    pub work: u64,
    /// Parts of a catalog with a value column (a resistor's resistance) that were picked while
    /// nothing limited the value, so the solver took the first row that fits (ADR 0065): a
    /// guideline finding.
    pub unconstrained: Vec<UnconstrainedPick>,
    /// Values declared in a module or board that checks which could not be evaluated read, and
    /// that nothing states or works out (ADR 0065): a guideline finding.
    pub unstated: Vec<UnstatedValue>,
    /// The parts whose current has no value and that Ohm's law bounds, with the bound (see
    /// [`CurrentBound`]). The model does not hold it as the part's current.
    pub current_bounds: Vec<CurrentBound>,
    /// What the DC analysis did (ADR 0076): how many islands it solved, left alone, and how many
    /// fields of parts it wrote.
    pub dc: DcSummary,
    /// What the DC analysis found into the nets it solved (ADR 0076, amendment 8).
    pub net_currents: Vec<NetCurrent>,
}

/// The most current that flows into a net, over every corner of the ranges of the DC analysis of
/// the island it is in: at each corner the sum of what the elements put into the net, and the
/// largest of those over the corners. Smaller than the sum of every current at its largest
/// where the currents follow one voltage, never larger, and complete for the net: every pin on
/// it is one that the analysis models.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetCurrent {
    /// A terminal of the net, which names it.
    pub pin: InstId,
    /// The largest current in amperes.
    pub current: f64,
}

/// An upper bound on the size of a part's current, from Ohm's law over the voltages that can
/// be at its two ends (the Ohm bound, pass-only: ADR 0026, amendment). The part's current stays
/// without a value; this says only that it is no larger than `current`, in either direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurrentBound {
    pub part: InstId,
    /// The largest `|I|` in amperes.
    pub current: f64,
}

/// A circle of values that stayed open, with the checks it left unmade.
#[derive(Debug, Clone)]
pub struct OpenCircle {
    /// The fields of the circle, in field order.
    pub fields: Vec<FieldId>,
    /// Whether a range stated on one of `fields` starts it. If not, `waiting` lists the values
    /// outside it that nothing states and that something in it reads.
    pub closed: bool,
    pub waiting: Vec<FieldId>,
    /// Indices into `model.relations()`: the bounds (`>=`, `<=`) that read a field of the
    /// circle, or a value worked out from one, and were not evaluated.
    pub checks: Vec<usize>,
    /// Empty for a circle of equations. For a group that closes through the currents of a net
    /// (a balance of more than two currents, which the solver does not work out a part's
    /// current from): each such net. A range on one of the currents starts the group, as one
    /// on a voltage in it does.
    pub nets: Vec<UnsolvedNet>,
}

/// A net whose currents add up to zero (Kirchhoff's current law) and whose parts' currents the
/// solver does not work out from the others (ADR 0026, issue #18).
#[derive(Debug, Clone)]
pub struct UnsolvedNet {
    /// The net's name.
    pub net: String,
    /// The board or module whose balance it is.
    pub scope: InstId,
    /// Every current on the net that has no value, in field order.
    pub unknown: Vec<FieldId>,
    /// Those of them that are a part's through-current: what a stated range would start.
    pub unsolved: Vec<FieldId>,
}

#[derive(Debug, Clone)]
pub struct Pick {
    pub part: InstId,
    pub catalog: String,
    pub candidate: String,
    /// The value for a schematic: `300`.
    pub label: String,
    /// The supplier's and manufacturer's part numbers and JLCPCB's library, if the
    /// candidate is a real part.
    pub lcsc: Option<String>,
    pub mpn: Option<String>,
    pub library: Option<Library>,
    /// The row's default for who assembles the part (ADR 0024).
    pub assembly: Option<String>,
    /// The row's `height` in mm, if it has one (ADR 0094): the part's height unless the design
    /// states it.
    pub height: Option<f64>,
    /// The row's `temp_min` and `temp_max` in kelvin, if it has them (ADR 0099): the part's
    /// range unless the design states one.
    pub temp_min: Option<f64>,
    pub temp_max: Option<f64>,
    /// The row's `qualification` cell, `Aec` or `None` (ADR 0099): the part's qualification
    /// unless the design states one. `Option::None` is a row that says nothing.
    pub qualification: Option<String>,
    /// The cited range of the row's dielectric class, when it names a class that has one: it
    /// judges an end that neither the design nor the row states, and is the class's and not a
    /// datasheet's (`netsmith_check::environment::ClassRange`).
    pub temp_class: Option<&'static netsmith_check::environment::ClassRange>,
    /// Set if the candidate is a generic part (ADR 0047): it has no part number yet, and
    /// [`crate::resolve`] finds the row that stands for it.
    pub generic: Option<crate::Generic>,
    /// How many candidates fit the design's constraints but failed a relation first.
    pub rejected: usize,
    /// How many identical parts of this candidate stand in parallel (a bank, ADR 0052): the
    /// value of the part's `.count` when the design leaves it to the solver or states it, and
    /// 1 for a part that has none.
    pub count: u32,
}

#[derive(Debug, Clone)]
pub struct Unresolved {
    pub part: InstId,
    pub reason: String,
}

enum Verdict {
    Pass,
    Unknown,
    Fail { message: String, margin: f64 },
}

/// Why a candidate's effective value is unknown.
enum Underated {
    /// The design gave the part no bias voltage (the path of the field).
    NoBias(String),
    Curve(DeratingError),
}

/// How many names the give-up message lists before it counts the rest.
const GIVE_UP_NAMES: usize = 12;

/// What a search that ran out of its budget says (`SearchBudget`): how many combinations it
/// tried, which parts it was choosing together, and what to change. The parts are searched
/// together because a bound reads values of several of them, and the search tried the
/// combinations in turn because no failure pointed at fewer parts than all.
fn give_up_message(tried: usize, names: &[String], out_of_work: bool) -> String {
    let shown = names.len().min(GIVE_UP_NAMES);
    let mut parts = names[..shown].join(" and ");
    if names.len() > shown {
        parts.push_str(&format!(" and {} more", names.len() - shown));
    }
    let limit = if out_of_work {
        " (the search stopped for the work it had done, which grows with the size of the board, and not for the number of combinations it had tried)"
    } else {
        ""
    };
    format!(
        "gave up after trying {tried} combinations of candidates for {parts}{limit}; a bound reads values of all of them, so no part could be blamed alone. State a value for one of them, narrow a window that a bound on them reads, or split the group so that fewer parts share a bound"
    )
}

/// The solver's state before a group of parts was searched, to put it back.
#[derive(Clone)]
struct Mark {
    model: Bookmark,
    diagnostics: usize,
    seen: HashSet<Diagnostic>,
    picks: usize,
    unresolved: usize,
    refused_counts: HashSet<FieldId>,
    derived: HashSet<FieldId>,
}

/// A group of parts that was searched, where it stands in the listing of picks, and the state
/// before it.
struct Step {
    position: usize,
    group: Vec<usize>,
    mark: Mark,
    picks: Vec<Pick>,
}

/// One part that needs a real part chosen for it.
struct Job {
    part: InstId,
    path: String,
    columns: Vec<(FieldId, String)>,
    /// For each column, whether a relation or connection uses its field. A column nothing
    /// uses cannot change what any other part or relation sees.
    used: Vec<bool>,
    catalog: Catalog,
    /// The part's `.count` field, if it has one (a bank, ADR 0052). When the design gives it a
    /// range, the search chooses the whole number in it together with the row.
    count: Option<FieldId>,
}

/// `a` or `an` for a type name in a message. A name like `Mcu32` is read letter by letter,
/// so it takes `an`.
fn article(word: &str) -> &'static str {
    let mut chars = word.chars();
    let spelled = matches!(
        chars.next(),
        Some('F' | 'H' | 'L' | 'M' | 'N' | 'R' | 'S' | 'X')
    ) && chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() && !"aeiou".contains(c));
    if word.starts_with(['A', 'E', 'I', 'O', 'U']) || spelled {
        "an"
    } else {
        "a"
    }
}

/// The most identical parts a bank may be asked for, whatever range a design gives its count.
const MAX_BANK: f64 = 64.0;

/// A choice as a message names it: the row, and for a bank how many of it.
fn choice_name(c: &Candidate, count: Option<f64>) -> String {
    match count {
        Some(n) => format!("{n} x {}", c.name),
        None => c.name.clone(),
    }
}

/// How often the scan of the fields that finds which parts share a relation says how far it is.
const FIELDS_REPORT_EVERY: usize = 512;

/// What a search has learned so far.
#[derive(Default)]
struct Search {
    trials: usize,
    /// `Solver::work` when the search began: the work of this search is what came after.
    work_from: u64,
    gave_up: bool,
    /// Whether it gave up for the work it had done and not for the candidates it had tried.
    out_of_work: bool,
    /// How many candidates fit the first part's constraints, for the message.
    first_level_fits: usize,
    /// The part, if any, that had nothing fitting what the design already says.
    no_fit: Option<usize>,
    /// The choices currently made, as `path = candidate`.
    chosen: Vec<String>,
    picks: Vec<Pick>,
    closest: Option<Closest>,
    /// The jobs of the groups not searched yet.
    elsewhere: HashSet<usize>,
    /// The most parts that were chosen at once: it only grows, and is what progress reports
    /// (a search that backs up has not un-chosen what it got through).
    deepest: usize,
    /// Whether a part's candidates are narrowed before they are tried (`narrow.rs`).
    narrow: bool,
    /// How many parts' turns had candidates to skip, how many candidates the narrowing skipped,
    /// and the work of the questions it asked to find them.
    pruned: usize,
    skipped: usize,
    probe_work: u64,
}

/// The choice that came nearest to working, kept for the error message: the one that got
/// furthest through the parts, and among those the one that missed by the least.
#[derive(Clone)]
struct Closest {
    /// How many parts had been chosen before the one that failed.
    depth: usize,
    margin: f64,
    assignment: Vec<String>,
    /// The depth in the group of the part each entry of `assignment` is for.
    depths: Vec<usize>,
    message: String,
    at: (SourceId, Span),
}

struct Conflict {
    source: SourceId,
    span: Span,
    message: String,
    /// The fields whose values the conflict was found in.
    fields: Vec<FieldId>,
}

/// A candidate that was accepted, with what it needs of its open columns.
struct Accepted {
    needs: Vec<(String, Need)>,
    /// Whether every bound that waits for a part still to be chosen (`verify_possible`) also
    /// holds over the whole range its values have now. A candidate for which one holds only
    /// for some values is accepted only when no candidate has them all (ADR 0065, 9.6).
    whole: bool,
}

struct Rejection {
    message: String,
    margin: f64,
    at: (SourceId, Span),
    /// The fields of the one relation or conflict that failed: if none of them is worked out
    /// from the part's count, a larger count fails the same way.
    own: Vec<FieldId>,
    /// The relation that failed (its place in the model's list), if it was one and not a
    /// conflict found while propagating.
    relation: Option<usize>,
    /// Whether the DC analysis of the group's network was part of the trial and read numbers
    /// that are not listed in `own` (a pin's levels): the value it wrote is then worked out from
    /// every pick of the group. The fields it did read are in `own`.
    network: bool,
}

/// Why a part, or the parts after it, could not be chosen.
struct Failure {
    /// The depths of the earlier parts whose choices the failure depends on.
    depends: BTreeSet<usize>,
    /// The depths of the parts the failure involves, however deep: this part if it is one, the
    /// earlier parts it depends on, and the parts after it that the failure came from. The
    /// smallest set of picks that cannot be met together, as far as the search has learned.
    core: BTreeSet<usize>,
    /// The rejection nearest to working among those the failure is made of.
    closest: Option<Closest>,
}

/// How a search of a group's parts ended.
enum Outcome {
    /// Every part has a candidate; they are in the search's picks.
    Solved,
    /// No candidate for the part at some depth works with the choices before it. The set
    /// holds the depths of the earlier parts whose choices that failure depends on (see
    /// [`Coupling`]); a failure that does not depend on the part just above is not undone
    /// by changing that part.
    Failed(Failure),
    /// The search stopped at the trial limit.
    GaveUp,
}

/// Which parts can see each other's choices. A part's choice sets its columns; propagation
/// then carries the values along the connections (both ways), the equations (from the right
/// side to the left) and the balances of currents (each term from the others), and nowhere
/// else. A value therefore depends on the choices of the parts whose columns it is worked out
/// from, which `Coupling::fed_by` finds by following those edges, and a check that reads some
/// fields (a relation, a conflict, the domains that decide which candidates fit) is settled by
/// those parts and by no others. The class of a field (the fields joined to it by any of the
/// edges, in either direction) is wider and only used where a rejection is compared with the
/// count of a bank.
struct Coupling {
    /// The class of each field.
    class: Vec<usize>,
    /// For each relation (by its place in the model's list), the jobs whose columns the
    /// fields it names are worked out from, however indirectly: the parts whose pick the
    /// relation's value waits for. A relation that none of the parts still to be chosen feeds
    /// can be judged by the choice that completes what it reads.
    feeding: Vec<HashSet<usize>>,
    /// For each field, whether its value is the same whatever parts are chosen (see
    /// `Solver::constants`).
    constant: Vec<bool>,
    /// What each field is worked out from (see `Solver::coupling`), and the job whose pick sets
    /// each column: the parts a field's value depends on are found by following them.
    from: HashMap<FieldId, Vec<FieldId>>,
    job_of: HashMap<FieldId, usize>,
    /// The jobs each field was found to be fed by.
    fed_by_field: std::cell::RefCell<HashMap<FieldId, std::rc::Rc<HashSet<usize>>>>,
}

/// Keep in `best` the rejection that came nearest to working: the one that got furthest
/// through the parts, and among those the one that missed by the least.
fn nearer(best: &mut Option<Closest>, other: Option<&Closest>) {
    if let Some(c) = other
        && best
            .as_ref()
            .is_none_or(|b| c.depth > b.depth || (c.depth == b.depth && c.margin < b.margin))
    {
        *best = Some(c.clone());
    }
}

impl Coupling {
    /// The jobs whose pick sets the value of one of `fields`, however indirectly: the parts
    /// the value is worked out from. Narrower than the classes, which join everything that
    /// shares an equation or a net; this follows the direction in which values are worked out.
    fn fed_by(&self, fields: &[FieldId]) -> HashSet<usize> {
        let mut out = HashSet::new();
        for &f in fields {
            let cached = self.fed_by_field.borrow().get(&f).cloned();
            let found = cached.unwrap_or_else(|| {
                let mut found = HashSet::new();
                if !self.constant[f.0] {
                    let mut seen: HashSet<FieldId> = HashSet::from([f]);
                    let mut todo = vec![f];
                    while let Some(g) = todo.pop() {
                        if let Some(&j) = self.job_of.get(&g) {
                            found.insert(j);
                        }
                        for &h in self.from.get(&g).into_iter().flatten() {
                            if !self.constant[h.0] && seen.insert(h) {
                                todo.push(h);
                            }
                        }
                    }
                }
                let found = std::rc::Rc::new(found);
                self.fed_by_field.borrow_mut().insert(f, found.clone());
                found
            });
            out.extend(found.iter().copied());
        }
        out
    }

    /// The depths below `depth` of the parts in `jobs`.
    fn earlier_jobs(
        &self,
        jobs_at: &[usize],
        depth: usize,
        jobs: &HashSet<usize>,
    ) -> BTreeSet<usize> {
        (0..depth).filter(|&p| jobs.contains(&jobs_at[p])).collect()
    }

    fn classes_of(&self, fields: &[FieldId]) -> HashSet<usize> {
        fields.iter().map(|f| self.class[f.0]).collect()
    }
}

fn find(parent: &mut [usize], x: usize) -> usize {
    if parent[x] != x {
        let root = find(parent, parent[x]);
        parent[x] = root;
    }
    parent[x]
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra] = rb;
    }
}

pub fn solve(board: &Board, catalogs: &Catalogs) -> Solution {
    solve_model(&board.model, catalogs, &board.name)
}

/// Solve every board of a system together. Propagation already carries values across a
/// join (the equated fields, the balance of currents at the joined ports), and picking
/// reads whatever propagation settled, so one pass over the whole model reaches the fixed
/// point the boards need and no board is solved before the ports it depends on are known.
/// The solution's model is the system's; read one board through
/// [`Model::scoped_to`].
pub fn solve_system(system: &System, catalogs: &Catalogs) -> Solution {
    solve_model(&system.model, catalogs, &system.name)
}

fn solve_model(model: &Model, catalogs: &Catalogs, unit: &str) -> Solution {
    let mut s = Solver {
        model: model.clone(),
        catalogs,
        locked: catalogs
            .pick_lock()
            .map(|lock| {
                lock.rows(unit)
                    .iter()
                    .map(|r| (r.path.clone(), r.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        chosen: catalogs
            .picks()
            .map(|picks| {
                picks
                    .rows(unit)
                    .iter()
                    .map(|r| (r.path.clone(), r.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        diagnostics: Vec::new(),
        seen: HashSet::new(),
        picks: Vec::new(),
        unresolved: Vec::new(),
        refused_counts: HashSet::new(),
        derived: HashSet::new(),
        stated: stated_currents(model),
        stated_late: stated_late(model),
        sole_equation: sole_equations(model),
        claimed: claimed_by_ranges(model),
        several_equations: several_equations(model),
        equations_of: equations_of(model),
        fed: HashSet::new(),
        fed_at_start: HashSet::new(),
        trials: 0,
        work: 0,
        seeded: Vec::new(),
        dc_ends: Default::default(),
        supply_draws: Vec::new(),
        budget_failed: HashSet::new(),
        dc_disagreements: Vec::new(),
        original_relations: model.relations().len(),
        extras: Vec::new(),
        surveys: Vec::new(),
        reads: Default::default(),
        stamp: vec![0; model.fields().count()],
        clock: 1,
        chosen_before: 0,
        to_choose: 0,
        network: Default::default(),
        dc_index: None,
    };
    job::checkpoint();
    s.relax_pinned();
    s.fed_at_start = s.fed_at_start();
    for c in s.propagate() {
        s.report(c.source, c.span, c.message);
    }
    // A circle of values that no range starts is started from the supply's (ADR 0053), before
    // the picks that need its values, and again after them for a circle that a pick closed.
    if s.seed_circles() {
        for c in s.propagate() {
            s.report(c.source, c.span, c.message);
        }
    }
    // The bounds that the design's own bounds entail for the fields held to a value that still
    // has none (ADR 0065) are in the model while parts are picked and checked, and out of it
    // otherwise. A member that has a value by now is judged on it.
    s.find_transfers();
    s.add_transfers();
    netsmith_core::timings::stage("solve.pick", "", || s.pick_parts());
    s.drop_transfers();
    for c in s.propagate() {
        s.report(c.source, c.span, c.message);
    }
    if s.seed_circles() {
        for c in s.propagate() {
            s.report(c.source, c.span, c.message);
        }
    }
    s.check_windows();
    // The picked parts' network is solved, and what it gives is written where nothing states
    // a value (ADR 0076); the relations that waited are then judged with numbers.
    let (dc, dc_notes, net_reads, net_currents) =
        netsmith_core::timings::stage("solve.dc", "", || s.dc_pass());
    for c in s.propagate() {
        s.report(c.source, c.span, c.message);
    }

    let mut deferred = Vec::new();
    // A relation between the places of two parts is the placer's and the check's to
    // evaluate (ADR 0045, section 3), so it is no relation left unevaluated.
    let between_parts: HashSet<usize> = crate::place::offsets(&s.model)
        .into_iter()
        .map(|o| o.relation)
        .collect();
    // A bank whose pick failed has been reported with the count and row that came nearest. A
    // relation that mentions its `.count` would fail again on the range the count was left
    // with (the output ripple of a stage reads it), so it says nothing more (ADR 0052).
    let refused = std::mem::take(&mut s.refused_counts);
    // The relation a `~>` chain across a rail gives its current (ADR 0113) waits when the rail
    // has no voltage, and so, often, do the parts' own checks that read the current. When one of
    // them waits and says so, the chain's relation is not said as well.
    let chain_currents: HashMap<_, HashSet<FieldId>> = s
        .model
        .chains()
        .iter()
        .map(|c| {
            let currents = c
                .parts
                .iter()
                .filter_map(|&p| s.model.through_of(p))
                .collect();
            ((c.source, c.span), currents)
        })
        .collect();
    let mut chains_waiting: Vec<usize> = Vec::new();
    s.add_transfers();
    for (i, r) in s.model.relations().to_vec().iter().enumerate() {
        job::checkpoint();
        // A bound that the solver added says nothing when it holds or cannot be judged (the
        // design's own bound is the line that waits), and says why when it fails.
        let own = i >= s.original_relations;
        match s.verify(r) {
            Verdict::Pass => {}
            Verdict::Unknown if own || between_parts.contains(&i) => {}
            Verdict::Unknown if chain_currents.contains_key(&(r.source, r.span)) => {
                chains_waiting.push(i);
            }
            Verdict::Unknown => deferred.push(i),
            Verdict::Fail { .. } if relation_mentions_any(r, &refused) => {}
            Verdict::Fail { message, .. } => {
                // A bound on the clock of a bus is said in the words of the bus (ADR 0064).
                if let Some((part, text)) = crate::busclock::failure(&s.model, r) {
                    let (source, span) = s.model.instance(part).at();
                    s.report(source, span, text);
                    continue;
                }
                let message = s.relation_message(i, r, message);
                s.report(r.source, r.span, message)
            }
        }
    }
    s.drop_transfers();
    for i in chains_waiting {
        let r = &s.model.relations()[i];
        let currents = &chain_currents[&(r.source, r.span)];
        let said = deferred
            .iter()
            .any(|&j| relation_mentions_any(&s.model.relations()[j], currents));
        if !said {
            deferred.push(i);
        }
    }
    deferred.sort_unstable();
    // A check that waits on a current that is not known is made when Ohm's law bounds the
    // current and the bound proves it (never otherwise).
    let (deferred, current_bounds) = s.prove_by_ohm(deferred);
    s.verify_balances();
    let supply_found = s.dc_check_supplies();
    s.dc_report_disagreements(supply_found);
    for d in netsmith_check::pin_faults_solved(model, &s.model) {
        s.report(d.source, d.span, d.message);
    }
    let (level_faults, mut not_evaluated) = crate::levels::check(&s.model, &net_reads);
    not_evaluated.extend(crate::busclock::notes(&s.model));
    not_evaluated.extend(crate::busfigures::notes(&s.model, &deferred));
    not_evaluated.extend(dc_notes);
    // A bus on which one part states its role and another does not (ADR 0064).
    for (at, message) in netsmith_check::unstated_roles(&s.model) {
        not_evaluated.push(crate::levels::NotEvaluated {
            at,
            involves: vec![at],
            message,
            cause: netsmith_core::Cause::NotStated,
        });
    }
    for d in level_faults {
        s.report(d.source, d.span, d.message);
    }
    let circular = if deferred.is_empty() {
        Vec::new()
    } else {
        crate::cycles::find(&s.model)
    };
    let open_circles = if deferred.is_empty() {
        Vec::new()
    } else {
        open_circles(&s.model, &deferred)
    };
    let unconstrained = s.unconstrained_picks(&s.surveys, &open_circles);
    let held: HashSet<FieldId> = unconstrained
        .iter()
        .filter_map(|u| u.held_by.as_ref().map(|h| h.member))
        .collect();
    let unstated = s.unstated_values(&deferred, &open_circles, &held);
    Solution {
        model: s.model,
        picks: s.picks,
        unresolved: s.unresolved,
        deferred,
        circular,
        open_circles,
        seeded: s.seeded,
        diagnostics: s.diagnostics,
        default_assembly: catalogs.default_assembly().map(str::to_string),
        not_evaluated,
        trials: s.trials,
        work: s.work,
        unconstrained,
        unstated,
        current_bounds,
        dc,
        net_currents,
    }
}

/// The circles of values that stayed open and have a bound that reads them (or a value
/// worked out from them) among the relations that were not evaluated.
fn open_circles(model: &Model, deferred: &[usize]) -> Vec<OpenCircle> {
    let relations = model.relations();
    let mut out: Vec<OpenCircle> = Vec::new();
    let found = |circles: Vec<crate::cycles::Circle>, out: &mut Vec<OpenCircle>| {
        for circle in circles {
            // The values worked out from the circle's: a field that an equation sets from them.
            let mut reach: HashSet<FieldId> = circle.fields.iter().copied().collect();
            loop {
                let before = reach.len();
                for r in relations {
                    if let (CmpOp::EqEq | CmpOp::Narrow, RExpr::Field(f)) = (&r.op, &r.lhs) {
                        let mut reads = Vec::new();
                        fields_in(&r.rhs, &mut reads);
                        if reads.iter().any(|g| reach.contains(g)) {
                            reach.insert(*f);
                        }
                    }
                }
                if reach.len() == before {
                    break;
                }
            }
            let checks: Vec<usize> = deferred
                .iter()
                .copied()
                // One that a circle already named is not named again.
                .filter(|i| !out.iter().any(|o| o.checks.contains(i)))
                .filter(|&i| {
                    let r = &relations[i];
                    let mut fields = Vec::new();
                    fields_in(&r.lhs, &mut fields);
                    fields_in(&r.rhs, &mut fields);
                    matches!(r.op, CmpOp::Ge | CmpOp::Le)
                        && fields.iter().any(|f| reach.contains(f))
                })
                .collect();
            if !checks.is_empty() {
                let nets = circle
                    .through
                    .into_iter()
                    .map(|t| {
                        let b = &model.balances()[t.balance];
                        UnsolvedNet {
                            net: b.net.clone(),
                            scope: b.scope,
                            unknown: t.unknown,
                            unsolved: t.unsolved,
                        }
                    })
                    .collect();
                out.push(OpenCircle {
                    fields: circle.fields,
                    closed: circle.closed,
                    waiting: circle.waiting,
                    checks,
                    nets,
                });
            }
        }
    };
    found(crate::cycles::circles(model), &mut out);
    // The groups that close through the currents of a net come after, and name only the checks
    // that no circle of equations has named (issue #18).
    found(crate::cycles::balance_circles(model), &mut out);
    out
}

/// Whether a relation mentions any of the fields.
fn relation_mentions_any(r: &Relation, fields: &HashSet<FieldId>) -> bool {
    let mut mentioned = Vec::new();
    fields_in(&r.lhs, &mut mentioned);
    fields_in(&r.rhs, &mut mentioned);
    mentioned.iter().any(|f| fields.contains(f))
}

/// A stated current and what the rest of its net determines it to be (ADR 0038). Both are
/// in the balance's terms: negated when the current is flipped.
struct Budget {
    field: FieldId,
    index: usize,
    flipped: bool,
    stated: Quantity,
    works_out: Quantity,
}

impl Budget {
    /// What the current is, as the field holds it.
    fn field_value(&self) -> Quantity {
        if self.flipped {
            self.works_out.neg()
        } else {
            self.works_out
        }
    }
}

struct Solver<'a> {
    model: Model,
    catalogs: &'a Catalogs,
    /// The rows a pick lock holds the parts of this board or system to, by path (see
    /// `PickLock`). A part held to a row by `Catalogs::with_pin` is not held by the lock.
    locked: BTreeMap<String, LockedRow>,
    /// The rows a picks file holds the parts of this board or system to, by path (#462). A
    /// chosen row comes before the lock's for its part.
    chosen: BTreeMap<String, ChosenRow>,
    diagnostics: Vec<Diagnostic>,
    seen: HashSet<Diagnostic>,
    picks: Vec<Pick>,
    unresolved: Vec<Unresolved>,
    /// The `.count` of each bank whose search for a row failed (ADR 0052).
    refused_counts: HashSet<FieldId>,
    /// Currents that propagation worked out from a balance (they were not stated). Such a
    /// current is worked out again when the terms it came from narrow, so it never goes
    /// stale: a value the design stated is not touched.
    derived: HashSet<FieldId>,
    /// The currents in a balance that the design stated with a range and that no equation
    /// determines, with the range as written (ADR 0038): each is a budget, and what the
    /// circuit determines must lie inside it.
    stated: HashMap<FieldId, Quantity>,
    /// The same, for a current whose range has a value at an end (ADR 0066): it states its
    /// range when the ends have a value, and is then a budget as the same range written with
    /// numbers is (`stated_range`).
    stated_late: HashSet<FieldId>,
    /// Per relation: whether it is an equation on a field that no other equation states
    /// (through the field or any field joined to it). Only such an equation narrows a field
    /// that has a range; see `propagate`.
    sole_equation: Vec<bool>,
    /// The fields that a range with a value at an end states (`f &= lo .. hi`, ADR 0066): they
    /// are not worked out from a balance while the statement waits for its ends.
    claimed: HashSet<FieldId>,
    /// The fields that more than one equation states (the left side of an equation that is not
    /// a sole one): each equation says what the value is, none says what it may be.
    several_equations: HashSet<FieldId>,
    /// The equations (by their place in the model's list) that have each field alone on the left.
    equations_of: HashMap<FieldId, Vec<usize>>,
    /// The fields that a pick can change: worked out, however indirectly, from the columns of a
    /// part still to be chosen. Empty until the parts are picked.
    fed: HashSet<FieldId>,
    /// The same, worked out before the parts are looked at, from the columns of every part that
    /// has a catalog: `fed` is empty while the first propagation runs, and the rule for several
    /// equations that `propagate` applies only to fields a pick can change must hold then too.
    fed_at_start: HashSet<FieldId>,
    /// Candidates tried so far, over every group.
    trials: usize,
    /// The work done so far, over every group (`SearchBudget::work`): a unit for each equation,
    /// relation and balance that a pass of `propagate` reads, for each relation a trial is
    /// judged against, and for each candidate that a part's turn looks at.
    work: u64,
    /// The circles of values that were started from the supply's range (ADR 0053).
    seeded: Vec<Seeded>,
    /// The fields the DC analysis wrote, with the states at the ends of what it wrote
    /// (ADR 0076): a failed relation that reads one says them.
    dc_ends: dcpass::DcEndsMap,
    /// What each solved island draws from a supply that states its `.current` (amendment 13).
    supply_draws: dcpass::SupplyDraws,
    /// The stated currents that a balance has reported as over their budget.
    budget_failed: HashSet<FieldId>,
    /// The currents a balance worked out that the DC analysis finds in a range that shares no
    /// value with it, said once the supplies' budgets have been judged (amendment 15).
    dc_disagreements: Vec<(SourceId, Span, String)>,
    /// How many relations the design wrote: the ones after them are the solver's own
    /// (`extras`) while they are in the model.
    original_relations: usize,
    /// The bounds that the design's bounds entail (ADR 0065), with why.
    extras: Vec<(Relation, transfer::Transfer)>,
    /// What the picks of parts with a value column might be limited by, found before the picks.
    surveys: Vec<findings::Survey>,
    /// What the design reads, for the relations the model has (`reads`).
    reads: std::cell::RefCell<Option<std::rc::Rc<reads::Reads>>>,
    /// When `propagate` last changed each field, on a clock that counts its changes.
    stamp: Vec<u64>,
    clock: u64,
    /// How many parts the groups before this one have chosen, and how many there are to
    /// choose in all: what the progress of picking is counted in (see `job::report`).
    chosen_before: usize,
    to_choose: usize,
    /// The pick-time analysis of the group being searched (ADR 0076, amendment 10).
    network: dcpass::NetworkTrial,
    /// What the pick-time analysis reads of the model's nets and instances, which the picks do
    /// not change: made at the first analysis and kept for the solve (issue #568).
    dc_index: Option<crate::dc::Index>,
}
