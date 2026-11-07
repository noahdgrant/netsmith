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

use std::collections::{BTreeSet, HashMap, HashSet};

mod findings;
mod reads;
mod seed;
mod transfer;
pub use findings::{HeldBy, UnconstrainedPick, UnstatedValue};
pub use seed::Seeded;

use netsmith_check::{
    Balance, Board, Checkpoint, Diagnostic, Domain, Draw, FieldId, FieldKind, InstId, InstKind,
    Model, RExpr, Relation, Snapshot, SourceId, System,
};
use netsmith_lang::Span;
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

use crate::catalog::{Candidate, Catalog, Catalogs, Derating, DeratingError, Library, Value};
use crate::eval::{EvalError, Val, eval, fields_in};
use crate::generic::{Need, Sense};

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
    /// catalogs the board was solved with (ADR 0028). Lowering applies it.
    pub default_assembly: Option<String>,
    /// The logic level and absolute maximum checks that could not be made, and why
    /// (ADR 0046): never a pass.
    pub not_evaluated: Vec<crate::levels::NotEvaluated>,
    /// How many candidates the search tried in all, over every group of parts: the effort
    /// spent choosing parts. Tests use it to show that a failing design is not searched
    /// once per combination of parts that have nothing to do with the failure.
    pub trials: usize,
    /// Parts of a catalog with a value column (a resistor's resistance) that were picked while
    /// nothing limited the value, so the solver took the first row that fits (ADR 0065): a
    /// guideline finding.
    pub unconstrained: Vec<UnconstrainedPick>,
    /// Values declared in a module or board that checks which could not be evaluated read, and
    /// that nothing states or works out (ADR 0065): a guideline finding.
    pub unstated: Vec<UnstatedValue>,
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

/// The most the search will try before it stops and says so.
const MAX_TRIALS: usize = 250_000;

/// The solver's state before a group of parts was searched, to put it back.
#[derive(Clone)]
struct Mark {
    model: Snapshot,
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

/// What a search has learned so far.
#[derive(Default)]
struct Search {
    trials: usize,
    gave_up: bool,
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
}

/// The choice that came nearest to working, kept for the error message: the one that got
/// furthest through the parts, and among those the one that missed by the least.
struct Closest {
    /// How many parts had been chosen before the one that failed.
    depth: usize,
    margin: f64,
    assignment: Vec<String>,
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

struct Rejection {
    message: String,
    margin: f64,
    at: (SourceId, Span),
    /// The fields whose values the rejection and its message were worked out from.
    reads: Vec<FieldId>,
    /// The fields of the one relation or conflict that failed: if none of them is worked out
    /// from the part's count, a larger count fails the same way.
    own: Vec<FieldId>,
    /// The relation that failed (its place in the model's list), if it was one and not a
    /// conflict found while propagating.
    relation: Option<usize>,
}

/// How a search of a group's parts ended.
enum Outcome {
    /// Every part has a candidate; they are in the search's picks.
    Solved,
    /// No candidate for the part at some depth works with the choices before it. The set
    /// holds the depths of the earlier parts whose choices that failure depends on (see
    /// [`Coupling`]); a failure that does not depend on the part just above is not undone
    /// by changing that part.
    Failed(BTreeSet<usize>),
    /// The search stopped at the trial limit.
    GaveUp,
}

/// Which parts can see each other's choices. A part's choice sets its columns; propagation
/// then carries the values along the connections, the equations and the balances of
/// currents, and nowhere else. The fields those join form a class, and a value in a class
/// depends only on the choices of parts whose columns are in it. A check that reads some
/// fields (a relation, a conflict, the domains that decide which candidates fit) is
/// therefore settled by the parts that share a class with them, and by no others.
struct Coupling {
    /// For each job, the classes of its columns, and of its count if it is a bank.
    columns: Vec<HashSet<usize>>,
    /// For each job, the classes of every field inside its part.
    inside: Vec<HashSet<usize>>,
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
}

impl Coupling {
    /// The depths below `depth` of the parts whose columns share a class with `reads`.
    fn earlier(&self, jobs_at: &[usize], depth: usize, reads: &HashSet<usize>) -> BTreeSet<usize> {
        (0..depth)
            .filter(|&p| !self.columns[jobs_at[p]].is_disjoint(reads))
            .collect()
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
    solve_model(&board.model, catalogs)
}

/// Solve every board of a system together. Propagation already carries values across a
/// join (the equated fields, the balance of currents at the joined ports), and picking
/// reads whatever propagation settled, so one pass over the whole model reaches the fixed
/// point the boards need and no board is solved before the ports it depends on are known.
/// The solution's model is the system's; read one board through
/// [`Model::scoped_to`].
pub fn solve_system(system: &System, catalogs: &Catalogs) -> Solution {
    solve_model(&system.model, catalogs)
}

fn solve_model(model: &Model, catalogs: &Catalogs) -> Solution {
    let mut s = Solver {
        model: model.clone(),
        catalogs,
        diagnostics: Vec::new(),
        seen: HashSet::new(),
        picks: Vec::new(),
        unresolved: Vec::new(),
        refused_counts: HashSet::new(),
        derived: HashSet::new(),
        stated: stated_currents(model),
        sole_equation: sole_equations(model),
        claimed: claimed_by_ranges(model),
        several_equations: several_equations(model),
        equations_of: equations_of(model),
        fed: HashSet::new(),
        fed_at_start: HashSet::new(),
        trials: 0,
        seeded: Vec::new(),
        original_relations: model.relations().len(),
        extras: Vec::new(),
        surveys: Vec::new(),
        reads: Default::default(),
        stamp: vec![0; model.fields().count()],
        clock: 1,
    };
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
    s.pick_parts();
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

    let mut deferred = Vec::new();
    // A relation between the places of two parts is the placer's and the check's to
    // evaluate (ADR 0045, section 3), so it is no relation left unevaluated.
    let between_parts: HashSet<usize> = crate::place::offsets(&s.model)
        .into_iter()
        .map(|o| o.relation)
        .collect();
    // A bank whose pick failed has been reported with the count and row that came nearest. A
    // relation that mentions its `.count` would fail again on the range the count was left
    // with (a stated `.output_count` of a stage, tied by `==` to the capacitor's `.count`),
    // so it says nothing more (ADR 0052).
    let refused = std::mem::take(&mut s.refused_counts);
    s.add_transfers();
    for (i, r) in s.model.relations().to_vec().iter().enumerate() {
        // A bound that the solver added says nothing when it holds or cannot be judged (the
        // design's own bound is the line that waits), and says why when it fails.
        let own = i >= s.original_relations;
        match s.verify(r) {
            Verdict::Pass => {}
            Verdict::Unknown if own || between_parts.contains(&i) => {}
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
    s.verify_balances();
    for d in netsmith_check::pin_faults_solved(model, &s.model) {
        s.report(d.source, d.span, d.message);
    }
    let (level_faults, mut not_evaluated) = crate::levels::check(&s.model);
    not_evaluated.extend(crate::busclock::notes(&s.model));
    // A bus on which one part states its role and another does not (ADR 0064).
    for (at, message) in netsmith_check::unstated_roles(&s.model) {
        not_evaluated.push(crate::levels::NotEvaluated {
            at,
            involves: vec![at],
            message,
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
        unconstrained,
        unstated,
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
    /// The circles of values that were started from the supply's range (ADR 0053).
    seeded: Vec<Seeded>,
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
}

/// The currents in a balance that the design gave a range (`.x.current &= a..b`) and that
/// no `==` states: their range is a budget (ADR 0038). A current an equation determines is
/// not stated, whatever range it was also written with; the equation checks that.
fn stated_currents(model: &Model) -> HashMap<FieldId, Quantity> {
    let equated: HashSet<FieldId> = model
        .relations()
        .iter()
        .filter_map(|r| match (&r.op, &r.lhs) {
            (CmpOp::EqEq, RExpr::Field(f)) => Some(*f),
            _ => None,
        })
        .collect();
    model
        .balances()
        .iter()
        .flat_map(|b| b.draws.iter().map(|d| d.field))
        .filter(|f| !equated.contains(f))
        .filter_map(|f| match &model.field(f).domain {
            Domain::Quantity(q) => Some((f, *q)),
            _ => None,
        })
        .collect()
}

/// The fields a range with a value at its end narrows (ADR 0066). A part states its own draw
/// that way, and the draw is the part's to state: while an end has no value (a catalog row not
/// picked yet) the balance of the net must not work the current out from the others, because
/// the range would later be met with what the others allowed and the part's own maximum would
/// be lost in the overlap (a supply allowed 500 nA, a part that may draw 750 nA).
fn claimed_by_ranges(model: &Model) -> HashSet<FieldId> {
    model
        .relations()
        .iter()
        .filter_map(|r| match (&r.op, &r.lhs, &r.rhs) {
            (CmpOp::Narrow | CmpOp::Default, RExpr::Field(f), RExpr::Range(..)) => Some(*f),
            _ => None,
        })
        .collect()
}

fn equations_of(model: &Model) -> HashMap<FieldId, Vec<usize>> {
    let mut out: HashMap<FieldId, Vec<usize>> = HashMap::new();
    for (i, r) in model.relations().iter().enumerate() {
        if let (CmpOp::EqEq, RExpr::Field(f)) = (&r.op, &r.lhs) {
            out.entry(*f).or_default().push(i);
        }
    }
    out
}

/// The fields on the left of more than one `==` (counting a field and the fields joined to it as
/// one).
fn several_equations(model: &Model) -> HashSet<FieldId> {
    let sole = sole_equations(model);
    model
        .relations()
        .iter()
        .zip(sole)
        .filter_map(|(r, sole)| match (&r.op, &r.lhs) {
            (CmpOp::EqEq, RExpr::Field(f)) if !sole => Some(*f),
            _ => None,
        })
        .collect()
}

/// For each relation, whether it is the only `==` whose left side is its field or a field
/// that holds the same value (`~` joins interfaces and so their fields, ADR 0007).
fn sole_equations(model: &Model) -> Vec<bool> {
    let mut parent: Vec<usize> = (0..model.fields().count()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for &(a, b) in model.equalities() {
        let (ra, rb) = (find(&mut parent, a.0), find(&mut parent, b.0));
        parent[ra] = rb;
    }
    let mut count = vec![0_usize; parent.len()];
    let sides: Vec<Option<usize>> = model
        .relations()
        .iter()
        .map(|r| match (&r.op, &r.lhs) {
            (CmpOp::EqEq, RExpr::Field(f)) => Some(find(&mut parent, f.0)),
            _ => None,
        })
        .collect();
    for root in sides.iter().flatten() {
        count[*root] += 1;
    }
    sides
        .iter()
        .map(|root| root.is_some_and(|r| count[r] == 1))
        .collect()
}

fn merge(a: &Domain, b: &Domain) -> Option<Domain> {
    match (a, b) {
        (Domain::Unset, x) | (x, Domain::Unset) => Some(x.clone()),
        (Domain::Quantity(x), Domain::Quantity(y)) => {
            x.intersect(y).ok().flatten().map(Domain::Quantity)
        }
        (Domain::Text(x), Domain::Text(y)) => (x == y).then(|| a.clone()),
        _ => None,
    }
}

fn domain_of(v: Val) -> Domain {
    match v {
        Val::Q(q) => Domain::Quantity(q),
        Val::T(t) => Domain::Text(t),
    }
}

/// How far `v` sticks out of `allowed`, as a fraction of `allowed`.
fn overshoot(allowed: &Quantity, v: &Quantity) -> f64 {
    let out = (allowed.lo() - v.lo()).max(v.hi() - allowed.hi()).max(0.0);
    out / allowed.lo().abs().max(allowed.hi().abs()).max(1e-30)
}

impl Solver<'_> {
    fn report(&mut self, source: SourceId, span: Span, message: String) {
        let d = Diagnostic {
            source,
            span,
            message,
        };
        if self.seen.insert(d.clone()) {
            self.diagnostics.push(d);
        }
    }

    fn domain(&self, f: FieldId) -> Domain {
        self.model.field(f).domain.clone()
    }

    /// A failure of a relation as it is reported: with what the currents say, or, for a bound
    /// that the solver added, with the statements that make it a bound on the part (ADR 0065).
    fn relation_message(&self, index: usize, r: &Relation, message: String) -> String {
        if index >= self.original_relations {
            self.transferred_message(index, message)
        } else {
            self.with_currents(r, message)
        }
    }

    /// Set a field's domain in `propagate`, noting when.
    fn set_field(&mut self, field: FieldId, domain: Domain) {
        self.clock += 1;
        self.stamp[field.0] = self.clock;
        self.model.set_domain(field, domain);
    }

    fn propagate(&mut self) -> Vec<Conflict> {
        let mut conflicts: Vec<Conflict> = Vec::new();
        let mut said: HashSet<String> = HashSet::new();
        // The structure does not change while values narrow, so it is copied once.
        let equalities = self.model.equalities().to_vec();
        let relations = std::sync::Arc::clone(self.model.shared_relations());
        let balances = std::sync::Arc::clone(self.model.shared_balances());
        let reads = self.reads();
        // The pass in which each item was last worked through, as a time on the clock of
        // `set_field`: an item whose fields have not changed since is worked through again to
        // the same effect (it changed nothing, or it changed its own field, which is a change).
        let mut equalities_at = vec![0u64; equalities.len()];
        let mut relations_at = vec![0u64; relations.len()];
        let mut balances_at = vec![0u64; balances.len()];
        for _ in 0..64 {
            let mut changed = false;

            for (item, &(a, b)) in equalities.iter().enumerate() {
                let at = equalities_at[item];
                if at != 0 && self.stamp[a.0] <= at && self.stamp[b.0] <= at {
                    continue;
                }
                equalities_at[item] = self.clock;
                let (da, db) = (self.domain(a), self.domain(b));
                match merge(&da, &db) {
                    Some(m) => {
                        if m != da {
                            self.set_field(a, m.clone());
                            changed = true;
                        }
                        if m != db {
                            self.set_field(b, m);
                            changed = true;
                        }
                    }
                    None => {
                        let owner = self.model.instance(self.model.field(a).owner);
                        // Point at a statement that set one of the two values, if any.
                        let (source, span) = [a, b]
                            .iter()
                            .find_map(|&f| self.model.field(f).narrowed_at.last().copied())
                            .unwrap_or((owner.source, owner.span));
                        let message = format!(
                            "connected values disagree: `{}` is {} but `{}` is {}",
                            self.model.path_of_field(a),
                            self.model.show_value(a, &da),
                            self.model.path_of_field(b),
                            self.model.show_value(b, &db)
                        );
                        if said.insert(message.clone()) {
                            conflicts.push(Conflict {
                                source,
                                span,
                                message,
                                fields: vec![a, b],
                            });
                        }
                    }
                }
            }

            for (index, r) in relations.iter().enumerate() {
                if !matches!(r.op, CmpOp::EqEq | CmpOp::Narrow) {
                    continue;
                }
                let RExpr::Field(f) = &r.lhs else { continue };
                let at = relations_at[index];
                if at != 0 && reads.inputs_of(index).iter().all(|f| self.stamp[f.0] <= at) {
                    continue;
                }
                relations_at[index] = self.clock;
                let Some(v) = reads.value_of_rhs(&self.model, index, r) else {
                    continue;
                };
                let value = domain_of(v);
                let current = self.domain(*f);
                match (r.op, &current) {
                    (_, Domain::Unset) => {
                        self.set_field(*f, value);
                        changed = true;
                    }
                    (CmpOp::Narrow, _) => match merge(&current, &value) {
                        Some(m) => {
                            if m != current {
                                self.set_field(*f, m);
                                changed = true;
                            }
                        }
                        None => {
                            let message = format!(
                                "contradiction on `{}`: it is already {}, which does not overlap {}",
                                self.model.path_of_field(*f),
                                self.model.show_value(*f, &current),
                                self.model.show_value(*f, &value)
                            );
                            if said.insert(message.clone()) {
                                let mut fields = vec![*f];
                                fields_in(&r.rhs, &mut fields);
                                conflicts.push(Conflict {
                                    source: r.source,
                                    span: r.span,
                                    message,
                                    fields,
                                });
                            }
                        }
                    },
                    // An equation on a field that already has a range: the field's value
                    // is the expression's, so its set is the intersection of the two. When
                    // the expression's range lies inside the field's, the intersection is
                    // the expression's range, and the field takes it (a stated window of
                    // current becomes the current the circuit really carries, which is
                    // what a power rating must be checked against). When it does not lie
                    // inside, `verify` reports the equation with both ranges as they
                    // were, so the field is left alone here: narrowing it to the overlap
                    // would make the report name a range nobody wrote. Text and fields of
                    // another dimension are left to `verify` as well.
                    // A current a balance worked out is an enclosure of the real value,
                    // not a range anyone wrote (ADR 0038), so an equation on it and the
                    // balance both hold: it takes what the two say. Only a pair that share
                    // nothing is a fault, which `verify` reports.
                    (CmpOp::EqEq, Domain::Quantity(d)) if self.derived.contains(f) => {
                        if let Domain::Quantity(v) = &value
                            && let Ok(Some(both)) = d.intersect(v)
                            && both != *d
                        {
                            self.set_field(*f, Domain::Quantity(both));
                            changed = true;
                        }
                    }
                    (CmpOp::EqEq, Domain::Quantity(d)) if self.sole_equation[index] => {
                        if let Domain::Quantity(v) = &value
                            && v != d
                            && d.encloses(v).unwrap_or(false)
                        {
                            self.set_field(*f, value);
                            changed = true;
                        }
                    }
                    // A value that several equations state and no statement bounds (the
                    // frequency of an oscillator that the board's window and the circuit both
                    // give): the first equation that could be worked out gave it a range, and
                    // that range is stale once a pick narrows what the equation reads. The
                    // value is what each equation says, so it follows an equation whose range
                    // lies inside it. Without this the equation that came later (the window)
                    // was judged against the range of the earlier one and passed whatever the
                    // pick was (ADR 0065, "A value that two equations state"). Only for a value
                    // that a pick can change (`fed`), as in `verify`: two equations that no pick
                    // changes keep the old rule (ADR 0065, section 9.1, "Not done").
                    (CmpOp::EqEq, Domain::Quantity(d))
                        if self.several_equations.contains(f)
                            && (self.fed.contains(f) || self.fed_at_start.contains(f))
                            && self.model.field(*f).narrowed_at.is_empty() =>
                    {
                        if let Domain::Quantity(v) = &value
                            && v != d
                            && d.encloses(v).unwrap_or(false)
                        {
                            self.set_field(*f, value);
                            changed = true;
                        }
                    }
                    _ => {}
                }
            }

            // What a board draws is settled from inside it. A joined port that no board
            // can settle that way takes its value from the join, which can only say what the
            // others allow, so the joins wait until nothing else changes.
            for joins in [false, true] {
                for (item, b) in balances.iter().enumerate() {
                    let is_join = self.model.instance(b.scope).kind == InstKind::System;
                    if is_join != joins || b.returns {
                        continue;
                    }
                    let at = balances_at[item];
                    if at != 0 && b.draws.iter().all(|d| self.stamp[d.field.0] <= at) {
                        continue;
                    }
                    balances_at[item] = self.clock;
                    if let Some((field, value)) = self.solve_balance(b) {
                        match self.domain(field) {
                            Domain::Unset => {
                                self.derived.insert(field);
                            }
                            // Worked out before: it becomes what the old and the new
                            // value both say, so a value that went stale on one side is
                            // tightened and nothing the circuit allows is lost. Values
                            // that share nothing are left to `verify_balances`.
                            Domain::Quantity(old) => match self.reworked(b, &old, &value) {
                                Some(both) => {
                                    self.set_field(field, Domain::Quantity(both));
                                    changed = true;
                                    continue;
                                }
                                None => continue,
                            },
                            _ => continue,
                        }
                        self.set_field(field, Domain::Quantity(value));
                        changed = true;
                    } else if let Some(budget) = self.budget(b)
                        && budget.stated.encloses(&budget.works_out).unwrap_or(false)
                    {
                        // A stated current is a budget (ADR 0038): what the rest of the
                        // net determines lies inside it, and is what the term carries.
                        let field = budget.field;
                        let Domain::Quantity(old) = self.domain(field) else {
                            continue;
                        };
                        if let Ok(Some(both)) = old.intersect(&budget.field_value())
                            && both != old
                        {
                            self.set_field(field, Domain::Quantity(both));
                            changed = true;
                        }
                    }
                }
                if changed {
                    break;
                }
            }

            if !changed {
                break;
            }
        }
        conflicts
    }

    /// What a rail or chain adds to its net's balance: its current, negated if it is flipped.
    /// `None` while the current is unknown.
    fn contribution(&self, d: &Draw) -> Option<Quantity> {
        match &self.model.field(d.field).domain {
            Domain::Quantity(q) => Some(if d.flipped { q.neg() } else { *q }),
            _ => None,
        }
    }

    /// Whether a term is a part's through-current (the first or last part of a chain, ADR
    /// 0014) and not the current of a rail or a port.
    fn is_part_current(&self, d: &Draw) -> bool {
        let owner = self.model.field(d.field).owner;
        self.model.instance(owner).kind == InstKind::Part
    }

    /// If exactly one current on the net is unknown, its value: whatever makes the rest add up
    /// to zero. The current of a rail or a port is set this way; a part's through-current only
    /// from a complete balance (ADR 0027) of two terms (ADR 0026). From a longer balance it is
    /// verified and never set: the residual of several wide ranges says little about one part,
    /// and its own relations (Ohm's law, the LED equation, a stated value) are what determine it.
    fn solve_balance(&self, b: &Balance) -> Option<(FieldId, Quantity)> {
        if let Some(found) = self.solve_balance_but(b, None) {
            return Some(found);
        }
        // Everything is known: a current worked out earlier, from terms that have narrowed
        // since, is worked out again. The first one whose value changes is returned.
        b.draws
            .iter()
            .filter(|d| self.derived.contains(&d.field))
            .filter_map(|d| self.solve_balance_but(b, Some(d.field)))
            .find(|(f, v)| {
                matches!(self.domain(*f), Domain::Quantity(old)
                    if self.reworked(b, &old, v).is_some())
            })
    }

    /// What a current worked out before becomes when a balance works it out again as `new`,
    /// if that changes it. It is what both say. At a join between boards it is `new` only
    /// when that lies inside the old one: the join says what the others allow, and clipping
    /// a port's draw to it would hide a board that asks for more than the other gives
    /// (`verify_join`).
    fn reworked(&self, b: &Balance, old: &Quantity, new: &Quantity) -> Option<Quantity> {
        let both = if self.model.instance(b.scope).kind == InstKind::System {
            old.encloses(new).unwrap_or(false).then_some(*new)
        } else {
            old.intersect(new).ok().flatten()
        };
        both.filter(|both| both != old)
    }

    /// `solve_balance` with the current of `again`, if any, taken as unknown.
    fn solve_balance_but(
        &self,
        b: &Balance,
        again: Option<FieldId>,
    ) -> Option<(FieldId, Quantity)> {
        let mut unknown = None;
        let mut rest: Option<Quantity> = None;
        for d in &b.draws {
            let known = if Some(d.field) == again {
                None
            } else {
                self.contribution(d)
            };
            match known {
                None if unknown.is_none() => unknown = Some(d),
                None => return None,
                Some(q) => {
                    rest = match rest {
                        None => Some(q),
                        Some(sum) => Some(sum.add(&q).ok()?),
                    }
                }
            }
        }
        let u = unknown?;
        // A current that a range with values at its ends will state, and whose ends are not
        // known yet, is not worked out from the others (ADR 0066).
        if self.claimed.contains(&u.field) && self.domain(u.field) == Domain::Unset {
            return None;
        }
        // An incomplete balance (ADR 0027) has pins whose currents are not terms. What the
        // terms leave is theirs as much as a part's, so no part is set from it; a rail is, as
        // before (an enable pin on a supply net does not stop the supply being worked out),
        // but not from nothing: a lone port is not taken to be zero when something unmodelled
        // is on the net.
        if !b.is_complete() && (self.is_part_current(u) || rest.is_none()) {
            return None;
        }
        // A balance of two terms equates them (a fuse in series with a load carries the
        // load's current); a longer one subtracts, and what it leaves is not a part's. A term
        // that is exactly zero (a board's rail stated `&= 0A`: nothing else draws here) adds
        // nothing, so it is not a term: the same two currents are equal.
        let live = b
            .draws
            .iter()
            .filter(|d| !self.model.draw_is_zero(d))
            .count();
        if live > 2 && self.is_part_current(u) {
            return None;
        }
        let value = match rest {
            Some(sum) if u.flipped => sum,
            Some(sum) => sum.neg(),
            None => Quantity::point(Dimension::CURRENT, 0.0).ok()?,
        };
        Some((u.field, value))
    }

    /// The budget on a net, if it has one (ADR 0038): a complete balance with every current
    /// known, exactly one of them stated (a range the design wrote, which is an allowance)
    /// and no current that a balance worked out (such a current takes up whatever the rest
    /// leaves, so it says nothing about the stated one). What the other terms determine is
    /// what the stated one works out to, and it has to lie inside what was stated.
    fn budget(&self, b: &Balance) -> Option<Budget> {
        if !b.is_complete()
            || b.draws.len() < 2
            || self.model.instance(b.scope).kind == InstKind::System
            || b.draws.iter().any(|d| self.derived.contains(&d.field))
        {
            return None;
        }
        let known: Vec<Quantity> = b
            .draws
            .iter()
            .map(|d| self.contribution(d))
            .collect::<Option<_>>()?;
        let mut stated = b
            .draws
            .iter()
            .enumerate()
            // A current stated as exactly zero adds nothing to the net and is not an
            // allowance (a part's reference rail, ADR 0066), so it is not the stated term.
            .filter(|(_, d)| self.stated.contains_key(&d.field) && !self.model.draw_is_zero(d));
        let (index, d) = stated.next()?;
        if stated.next().is_some() {
            return None;
        }
        let mut others = known
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != index)
            .map(|(_, q)| *q);
        let first = others.next()?;
        let sum = others.try_fold(first, |sum, q| sum.add(&q).ok())?;
        let range = self.stated[&d.field];
        Some(Budget {
            field: d.field,
            index,
            flipped: d.flipped,
            stated: if d.flipped { range.neg() } else { range },
            works_out: sum.neg(),
        })
    }

    /// With every current known, the net has to be able to balance. A stated current is a
    /// budget (ADR 0038): when the rest of the net determines it, that must lie inside what
    /// was stated, and need not fill it. Otherwise the currents together have to be able to
    /// add up to zero. The currents are independent, so this is a necessary condition, not a
    /// sufficient one, and a net that fails it cannot balance. A return net is checked after the supply nets,
    /// and only when nothing has been reported: a fault shows there again, and the first
    /// message is enough.
    fn verify_balances(&mut self) {
        let mut balances = self.model.balances().to_vec();
        balances.sort_by_key(|b| b.returns);
        for b in balances {
            if b.returns && !self.diagnostics.is_empty() {
                continue;
            }
            // An incomplete balance says nothing about the parts on it (ADR 0027).
            if !b.is_complete() && b.draws.iter().any(|d| self.is_part_current(d)) {
                continue;
            }
            let known: Option<Vec<Quantity>> =
                b.draws.iter().map(|d| self.contribution(d)).collect();
            let Some(known) = known else { continue };
            if self.model.instance(b.scope).kind == InstKind::System {
                self.verify_join(&b, &known);
                continue;
            }
            // A port that only gives to what is inside, with nothing inside to take it that
            // the checker knows of (a part that is in no chain and has no rail of its own,
            // ADR 0014), is feeding what is not modelled: take what is stated.
            if known.len() == 1 && known[0].hi() <= 0.0 {
                continue;
            }
            let all: Vec<String> = b
                .draws
                .iter()
                .zip(&known)
                .map(|(d, q)| {
                    let sign = if d.flipped { "-" } else { "" };
                    format!("`{sign}{}` is {q}", self.model.path_of_field(d.field))
                })
                .collect();
            let slack = |qs: &[&Quantity]| {
                // A little slack, for the outward rounding of the sums.
                1e-9 * qs
                    .iter()
                    .flat_map(|q| [q.lo(), q.hi()])
                    .fold(1e-12_f64, |m, x| m.max(x.abs()))
            };
            // A stated current is a budget (ADR 0038): what the rest of the net determines
            // has to lie inside it. It need not fill it.
            if let Some(budget) = self.budget(&b) {
                let (stated, works_out) = (budget.stated, budget.works_out);
                let slack = slack(&[&stated, &works_out]);
                if stated.lo() - slack <= works_out.lo() && works_out.hi() <= stated.hi() + slack {
                    continue;
                }
                let d = &b.draws[budget.index];
                let name = self.model.path_of_field(d.field);
                let sign = if d.flipped { "-" } else { "" };
                let message = format!(
                    "the currents on net `{}` do not add up: `{sign}{name}` must stay within {stated}, but the others on it work out to {works_out} (all of them: {})",
                    b.net,
                    all.join(", ")
                );
                let (source, span) = self.balance_site(&b);
                self.report(source, span, message);
                continue;
            }
            // Otherwise the terms are ranges that all hold or all are worked out from one
            // another, and the balance only has to be able to reach zero.
            let total = known
                .iter()
                .skip(1)
                .try_fold(known[0], |sum, q| sum.add(q).ok());
            let Some(total) = total else { continue };
            if total.lo() - slack(&[&total]) <= 0.0 && 0.0 <= total.hi() + slack(&[&total]) {
                continue;
            }
            // The first term, against the others.
            let others = known
                .iter()
                .skip(1)
                .try_fold(None, |sum: Option<Quantity>, q| match sum {
                    None => Some(Some(*q)),
                    Some(s) => s.add(q).ok().map(Some),
                })
                .map(|sum| sum.or(Quantity::point(Dimension::CURRENT, 0.0).ok()));
            let Some(Some(others)) = others else { continue };
            let d = &b.draws[0];
            let name = self.model.path_of_field(d.field);
            let sign = if d.flipped { "-" } else { "" };
            let message = format!(
                "the currents on net `{}` do not add up: `{sign}{name}` is {}, so the others on it must add up to {}, but they can only give {others} (all of them: {})",
                b.net,
                known[0],
                known[0].neg(),
                all.join(", ")
            );
            let (source, span) = self.balance_site(&b);
            self.report(source, span, message);
        }
    }

    /// The ports a system joins, with every current known. Each port's current is what the
    /// outside draws through it (a board's rail is written that way), so they are mirror
    /// statements of one flow and must be able to add up to zero. A port that cannot draw
    /// (its current is never negative) is a supply, and its range is how much the board is
    /// built to give; what the other ports draw must lie within it. A port that draws
    /// states only what its board takes, so it is not checked against what the supply says
    /// (`.output.current &= 0mA..50mA` against a board that takes 5 mA to 15 mA is fine).
    fn verify_join(&mut self, b: &Balance, known: &[Quantity]) {
        let zero = Quantity::point(Dimension::CURRENT, 0.0).ok();
        let sum = |skip: Option<usize>| {
            known
                .iter()
                .enumerate()
                .filter(|&(j, _)| Some(j) != skip)
                .try_fold(zero, |sum, (_, q)| sum.map(|s| s.add(q).ok()))
                .flatten()
        };
        let slack = |qs: &[&Quantity]| {
            1e-9 * qs
                .iter()
                .flat_map(|q| [q.lo(), q.hi()])
                .fold(1e-12_f64, |m, x| m.max(x.abs()))
        };
        let describe = |this: &Self, i: usize| {
            let field = b.draws[i].field;
            let owner = this.model.field(field).owner;
            let board = this
                .model
                .owner_of(owner)
                .map(|inst| this.model.instance(inst).type_name.clone())
                .unwrap_or_default();
            format!(
                "`{}` (board `{board}`) is {}",
                this.model.path_of_field(field),
                known[i]
            )
        };
        let all: Vec<String> = (0..known.len()).map(|i| describe(self, i)).collect();
        let (source, span) = self.balance_site(b);

        let Some(total) = sum(None) else { return };
        if total.lo() - slack(&[&total]) > 0.0 || total.hi() + slack(&[&total]) < 0.0 {
            let message = format!(
                "the currents at the join `{}` cannot add up to zero: {}, which add up to {total}",
                b.net,
                all.join(" and ")
            );
            self.report(source, span, message);
            return;
        }
        for (i, current) in known.iter().enumerate() {
            let Some(others) = sum(Some(i)) else { continue };
            if current.lo() < 0.0 {
                continue;
            }
            let demand = others.neg();
            let slack = slack(&[current, &demand]);
            if current.lo() - slack <= demand.lo() && demand.hi() <= current.hi() + slack {
                continue;
            }
            let rest: Vec<&str> = all
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, d)| d.as_str())
                .collect();
            let message = format!(
                "the join `{}` asks more of a port than its board allows: {}, but the other ports there ({}) draw {demand}",
                b.net,
                all[i],
                rest.join(", ")
            );
            self.report(source, span, message);
        }
    }

    /// A failed requirement on a current, with what that current is made of: each net it
    /// is summed on, with the terms and their ranges, and the nets of the terms in turn (a
    /// regulator's output is the board's whole load, which is the rails and the parts on
    /// the net). Return nets repeat the same terms and are left out. A requirement that
    /// mentions no current in a balance is returned as it was.
    fn with_currents(&self, r: &Relation, message: String) -> String {
        let balances = self.model.balances();
        let reads = self.reads();
        let mut fields = Vec::new();
        fields_in(&r.lhs, &mut fields);
        fields_in(&r.rhs, &mut fields);
        let mut seen: HashSet<usize> = HashSet::new();
        let mut notes = Vec::new();
        while let Some(f) = fields.pop() {
            // The balances that sum the field, in the order the model lists them.
            for &i in reads.supply_balances_of(f) {
                let b = &balances[i];
                if !seen.insert(i) {
                    continue;
                }
                let terms: Vec<String> = b
                    .draws
                    .iter()
                    .map(|d| {
                        let sign = if d.flipped { "-" } else { "" };
                        let name = self.model.path_of_field(d.field);
                        format!("{sign}{name} ({})", self.model.field(d.field).domain)
                    })
                    .collect();
                let scope = match self.model.path_of(b.scope) {
                    path if path.is_empty() => String::new(),
                    path => format!(" in `{path}`"),
                };
                let unmodelled = if b.is_complete() {
                    String::new()
                } else {
                    let pins: Vec<String> = b
                        .unmodelled
                        .iter()
                        .map(|&p| format!("`{}`", self.model.path_of(p)))
                        .collect();
                    format!(
                        ", but the balance is incomplete: {} {} not modelled",
                        pins.join(", "),
                        if pins.len() == 1 { "is" } else { "are" }
                    )
                };
                notes.push(format!(
                    "net `{}`{scope}: {} add up to zero{unmodelled}",
                    b.net,
                    terms.join(" + ")
                ));
                fields.extend(b.draws.iter().map(|d| d.field));
            }
        }
        if notes.is_empty() {
            message
        } else {
            format!("{message} (the currents on {})", notes.join("; "))
        }
    }

    /// Where to point about a net's currents: at a statement that set one of them, or else
    /// at the board or module they are in.
    fn balance_site(&self, b: &Balance) -> (SourceId, Span) {
        b.draws
            .iter()
            .find_map(|d| self.model.field(d.field).narrowed_at.last().copied())
            .unwrap_or_else(|| {
                let scope = self.model.instance(b.scope);
                (scope.source, scope.span)
            })
    }

    /// A cell of a column that holds an enum must be one of its variants (ADR 0054): a row
    /// that names a class the enum does not have is refused, with the row and the variants.
    fn check_enum_cells(
        &self,
        catalog: &str,
        columns: &[(FieldId, String)],
        cat: Catalog,
    ) -> Result<Catalog, String> {
        for (field, key) in columns {
            let FieldKind::Enum(e) = self.model.field(*field).kind else {
                continue;
            };
            let info = self.model.enum_info(e);
            for c in &cat.candidates {
                if let Some(Value::Text(word)) = c.column(key)
                    && !info.variants.contains(word)
                {
                    let listed: Vec<String> =
                        info.variants.iter().map(|v| format!("`{v}`")).collect();
                    return Err(format!(
                        "the `{key}` of `{}` in the `{catalog}` catalog is \"{word}\", which is not a variant of `{}` ({})",
                        c.name,
                        info.name,
                        listed.join(", ")
                    ));
                }
            }
        }
        Ok(cat)
    }

    /// Whether `e` is a text field that nothing has set.
    fn is_unset_text(&self, e: &RExpr) -> bool {
        matches!(e, RExpr::Field(f)
            if matches!(self.model.field(*f).kind, FieldKind::Text | FieldKind::Enum(_))
                && self.model.field(*f).domain == Domain::Unset)
    }

    /// A requirement on a position (ADR 0045) that nothing satisfies: its bounds leave an
    /// empty window. Reported at the last statement that narrows the field.
    fn check_windows(&mut self) {
        let fields: Vec<FieldId> = self
            .model
            .fields()
            .filter(|(id, _)| self.model.is_place_field(*id))
            .map(|(id, _)| id)
            .collect();
        for field in fields {
            let w = crate::place::window(&self.model, field);
            if !w.is_empty() {
                continue;
            }
            let FieldKind::Quantity(dim) = self.model.field(field).kind else {
                continue;
            };
            let name = self.model.path_of_field(field);
            if let Some(&(source, span)) = w.sites.last() {
                self.report(source, span, w.clash(&name, dim));
            }
        }
    }

    /// Whether a relation is a bound on a `.place` field or a value tied to one: those are
    /// requirements that lowering reads (ADR 0045), not claims to verify.
    fn place_side<'r>(&self, r: &'r Relation) -> Option<(FieldId, &'r RExpr)> {
        let is_place = |e: &RExpr| match e {
            RExpr::Field(f) if self.model.is_place_field(*f) => Some(*f),
            _ => None,
        };
        match r.op {
            CmpOp::Ge | CmpOp::Le => is_place(&r.lhs)
                .map(|f| (f, &r.rhs))
                .or_else(|| is_place(&r.rhs).map(|f| (f, &r.lhs))),
            CmpOp::Narrow | CmpOp::Default => is_place(&r.lhs).map(|f| (f, &r.rhs)),
            CmpOp::EqEq => None,
        }
    }

    /// Whether the bound `r`, a `>=` or a `<=`, can hold for some value of the ranges as they
    /// are: `Fail` only when it holds for none, which no later narrowing of the ranges
    /// changes. A range that is unknown, or a comparison of anything but quantities, is
    /// `Unknown`: it is judged when it is known.
    fn verify_possible(&self, r: &Relation) -> Verdict {
        let (Ok(Val::Q(l)), Ok(Val::Q(v))) = (eval(&self.model, &r.lhs), eval(&self.model, &r.rhs))
        else {
            return Verdict::Unknown;
        };
        let (possible, word, gap) = if r.op == CmpOp::Ge {
            (l.hi() >= v.lo(), "at least", v.lo() - l.hi())
        } else {
            (l.lo() <= v.hi(), "at most", l.lo() - v.hi())
        };
        if possible {
            return Verdict::Pass;
        }
        let scale = l
            .lo()
            .abs()
            .max(l.hi().abs())
            .max(v.lo().abs())
            .max(v.hi().abs())
            .max(1e-30);
        Verdict::Fail {
            message: format!(
                "{} must be {word} {}, but {l} is always {} {v}",
                self.model.show_expr(&r.lhs),
                self.model.show_expr(&r.rhs),
                if r.op == CmpOp::Ge { "below" } else { "above" },
            ),
            margin: gap / scale,
        }
    }

    fn verify(&self, r: &Relation) -> Verdict {
        let show = |e: &RExpr| self.model.show_expr(e);
        let fail = |message: String, margin: f64| Verdict::Fail { message, margin };
        // A bound on a position narrows it. Its other side may still be waiting on a value;
        // a value a module offers (`.j.place.y &= .shell_y`, ADR 0042) that the board leaves
        // alone is no note.
        if let Some((_, other)) = self.place_side(r)
            && r.op != CmpOp::Narrow
        {
            return match eval(&self.model, other) {
                Ok(Val::Q(_)) => Verdict::Pass,
                Err(EvalError::Unknown(_)) => Verdict::Unknown,
                Ok(Val::T(_)) | Err(EvalError::Type) => {
                    fail("a position is a number".to_string(), 1.0)
                }
                Err(e) => fail(format!("{}: {e}", show(other)), 1.0),
            };
        }
        if let Some((_, RExpr::Field(g))) = self.place_side(r)
            && r.op == CmpOp::Narrow
            && self.model.field(*g).domain == Domain::Unset
        {
            return Verdict::Pass;
        }
        let eval_both = || -> Result<(Val, Val), EvalError> {
            let l = eval(&self.model, &r.lhs);
            let rr = eval(&self.model, &r.rhs);
            Ok((l?, rr?))
        };

        match r.op {
            // The checker turns every `?=` into a `&=` or drops it (ADR 0051), so the solver
            // reads `Default` as the narrowing it became.
            CmpOp::Narrow | CmpOp::Default => match eval(&self.model, &r.rhs) {
                Ok(_) => Verdict::Pass,
                // A text field nobody set narrows nothing: a value a module offers
                // (`.j.assembly &= .assembly_mode`, ADR 0042) may be left alone. A text
                // value is never worked out, so there is nothing still to wait for.
                Err(EvalError::Unknown(_)) if self.is_unset_text(&r.rhs) => Verdict::Pass,
                Err(EvalError::Unknown(_)) => Verdict::Unknown,
                Err(e) => fail(format!("{}: {e}", show(&r.rhs)), 1.0),
            },
            CmpOp::EqEq => {
                let rhs = match eval(&self.model, &r.rhs) {
                    Ok(v) => v,
                    Err(EvalError::Unknown(_)) => return Verdict::Unknown,
                    Err(e) => return fail(format!("{}: {e}", show(&r.rhs)), 1.0),
                };
                if let RExpr::Field(f) = &r.lhs {
                    let name = self.model.path_of_field(*f);
                    // A value that several equations state and no statement bounds has no
                    // allowance of its own: it is what the equations say, and a pick sets it.
                    // An equation whose right side no pick changes (a window the board stated,
                    // reached through a join or a link such as `.osc.fsw == .fsw`) is an
                    // allowance, and what the other equations work out must lie inside it. With
                    // no allowance among them, equations that a pick changes only have to share
                    // a value: both are what the circuit works out, and neither is a promise.
                    if let (Domain::Quantity(dx), Val::Q(v)) = (&self.model.field(*f).domain, &rhs)
                        && self.several_equations.contains(f)
                        && self.fed.contains(f)
                        && self.model.field(*f).narrowed_at.is_empty()
                        && !self.derived.contains(f)
                    {
                        let changes = |e: &RExpr| {
                            let mut read = Vec::new();
                            fields_in(e, &mut read);
                            read.iter().any(|g| self.fed.contains(g))
                        };
                        // What the equations that no pick changes allow, together.
                        let mut allowance: Option<Quantity> = None;
                        for &i in self.equations_of.get(f).into_iter().flatten() {
                            let other = &self.model.relations()[i];
                            if changes(&other.rhs) {
                                continue;
                            }
                            if let Ok(Val::Q(q)) = eval(&self.model, &other.rhs) {
                                allowance = match allowance {
                                    None => Some(q),
                                    Some(a) => match a.intersect(&q) {
                                        Ok(Some(both)) => Some(both),
                                        _ => Some(a),
                                    },
                                };
                            }
                        }
                        return if !changes(&r.rhs) {
                            if v.encloses(dx).unwrap_or(false) {
                                Verdict::Pass
                            } else {
                                fail(
                                    format!(
                                        "`{name}` must stay within {v}, which {} says, but it works out to {dx}",
                                        show(&r.rhs)
                                    ),
                                    overshoot(v, dx),
                                )
                            }
                        } else if let Some(a) = allowance {
                            if a.encloses(v).unwrap_or(false) {
                                Verdict::Pass
                            } else {
                                fail(
                                    format!(
                                        "`{name}` must stay within {a}, but {} works out to {v}",
                                        show(&r.rhs)
                                    ),
                                    overshoot(&a, v),
                                )
                            }
                        } else if dx.intersect(v).ok().flatten().is_some() {
                            Verdict::Pass
                        } else {
                            fail(
                                format!(
                                    "`{name}` is {dx} but {} works out to {v}; they can never be equal",
                                    show(&r.rhs)
                                ),
                                overshoot(dx, v),
                            )
                        };
                    }
                    return match (&self.model.field(*f).domain, rhs) {
                        (Domain::Unset, _) => Verdict::Unknown,
                        (Domain::Quantity(d), Val::Q(v)) => {
                            // See `propagate`: a derived current only has to share a value
                            // with the equation.
                            let derived = self.derived.contains(f);
                            if d.encloses(&v).unwrap_or(false)
                                || (derived && d.intersect(&v).ok().flatten().is_some())
                            {
                                Verdict::Pass
                            } else {
                                fail(
                                    format!(
                                        "`{name}` must stay within {d}, but {} works out to {v}",
                                        show(&r.rhs)
                                    ),
                                    overshoot(d, &v),
                                )
                            }
                        }
                        (Domain::Text(d), Val::T(v)) if *d == v => Verdict::Pass,
                        (Domain::Text(d), Val::T(v)) => {
                            let (d, v) = (
                                self.model.show_value(*f, &Domain::Text(d.clone())),
                                self.model.show_value(*f, &Domain::Text(v)),
                            );
                            let message = match (&r.rhs, self.model.field(*f).kind) {
                                // A variant written in the check: it is not an expression
                                // that works out to something else.
                                (RExpr::Text(_), FieldKind::Enum(_)) => {
                                    format!("`{name}` is {d}, but this check requires {v}")
                                }
                                _ => format!("`{name}` must be {d}, but {} is {v}", show(&r.rhs)),
                            };
                            fail(message, 1.0)
                        }
                        _ => fail(
                            format!("`{name}` and {} are different kinds of value", show(&r.rhs)),
                            1.0,
                        ),
                    };
                }
                match eval_both() {
                    Ok((Val::Q(l), Val::Q(v))) => {
                        if l.intersect(&v).ok().flatten().is_some() {
                            Verdict::Pass
                        } else {
                            fail(
                                format!(
                                    "{} is {l} but {} is {v}; they can never be equal",
                                    show(&r.lhs),
                                    show(&r.rhs)
                                ),
                                overshoot(&l, &v),
                            )
                        }
                    }
                    Err(EvalError::Unknown(_)) => Verdict::Unknown,
                    _ => Verdict::Pass,
                }
            }
            CmpOp::Ge | CmpOp::Le => match eval_both() {
                Ok((Val::Q(l), Val::Q(v))) => {
                    let (ok, word, gap) = if r.op == CmpOp::Ge {
                        (l.lo() >= v.hi(), "at least", v.hi() - l.lo())
                    } else {
                        (l.hi() <= v.lo(), "at most", l.hi() - v.lo())
                    };
                    if ok {
                        Verdict::Pass
                    } else {
                        let scale = l
                            .lo()
                            .abs()
                            .max(l.hi().abs())
                            .max(v.lo().abs())
                            .max(v.hi().abs())
                            .max(1e-30);
                        fail(
                            format!(
                                "{} must always be {word} {}, but {l} is not always {} {v}",
                                show(&r.lhs),
                                show(&r.rhs),
                                if r.op == CmpOp::Ge { "above" } else { "below" },
                            ),
                            gap / scale,
                        )
                    }
                }
                Err(EvalError::Unknown(_)) => Verdict::Unknown,
                Err(e) => fail(format!("{e}"), 1.0),
                Ok(_) => fail("a comparison needs numbers, not text".into(), 1.0),
            },
        }
    }

    // ---- picking -------------------------------------------------------------

    /// A part held to one row (`Catalogs::with_pin`) stands in for a part the design stated
    /// a value for. The design's statements about the numbers of its catalog columns (its
    /// value, a rating) are what the alternative replaces, so they are dropped before
    /// anything is propagated from them; the statements about text (the package, the
    /// colour) and every relation stay, and judge the alternative.
    fn relax_pinned(&mut self) {
        let parts: Vec<(InstId, String)> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Part && i.catalog.is_some())
            .map(|(id, _)| (id, self.model.path_of(id)))
            .filter(|(_, path)| self.catalogs.pinned(path).is_some())
            .collect();
        for (id, _) in parts {
            let fields = self
                .model
                .instance(id)
                .catalog
                .as_ref()
                .map(|c| c.fields.clone())
                .unwrap_or_default();
            for f in fields {
                if matches!(self.model.field(f).kind, FieldKind::Quantity(_)) {
                    self.model.set_domain(f, Domain::Unset);
                }
            }
        }
    }

    fn pick_parts(&mut self) {
        let mut parts: Vec<(String, InstId)> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Part && i.catalog.is_some())
            .map(|(id, _)| (self.model.path_of(id), id))
            .collect();
        parts.sort_by(|a, b| a.0.cmp(&b.0));

        let mut jobs: Vec<Job> = Vec::new();
        // What `Catalogs::resolve` gave for a catalog and its columns: the same for every part
        // that is of the type, so it is worked out once (it makes the generic parts, which
        // takes longer than the search of a part with a few rows).
        type Kinds = Vec<(String, FieldKind)>;
        type Resolved = Option<Result<crate::catalog::Catalog, String>>;
        let mut resolved_before: Vec<(String, Kinds, Resolved)> = Vec::new();
        for (path, part) in parts {
            let catalog = self
                .model
                .instance(part)
                .catalog
                .clone()
                .expect("filtered above");
            let columns: Vec<(FieldId, String)> = catalog
                .fields
                .iter()
                .map(|&f| {
                    let full = self.model.path_of_field(f);
                    let key = full
                        .strip_prefix(&format!("{path}."))
                        .unwrap_or(&full)
                        .to_string();
                    (f, key)
                })
                .collect();
            let kinds: Vec<(String, FieldKind)> = columns
                .iter()
                .map(|(f, key)| (key.clone(), self.model.field(*f).kind))
                .collect();
            let found = match resolved_before
                .iter()
                .find(|(name, k, _)| *name == catalog.name && *k == kinds)
            {
                Some((_, _, found)) => found.clone(),
                None => {
                    let found = self.catalogs.resolve(&catalog.name, &kinds);
                    resolved_before.push((catalog.name.clone(), kinds.clone(), found.clone()));
                    found
                }
            };
            let resolved = found.map(|found| {
                found.and_then(|cat| self.check_enum_cells(&catalog.name, &columns, cat))
            });
            match resolved {
                None => self.unresolved.push(Unresolved {
                    part,
                    reason: format!("there is no `{}` catalog to choose from", catalog.name),
                }),
                Some(Err(problem)) => {
                    let inst = self.model.instance(part);
                    let (source, span) = inst.at();
                    self.report(
                        source,
                        span,
                        format!(
                            "`{path}` cannot be chosen from the `{}` catalog: {problem}",
                            catalog.name
                        ),
                    );
                    self.unresolved.push(Unresolved {
                        part,
                        reason: format!("the `{}` catalog cannot be read", catalog.name),
                    });
                }
                Some(Ok(cat)) if self.unnamed_key(part, &path, &catalog, &columns, &cat) => {}
                Some(Ok(mut cat)) => {
                    // A part held to one row (`Catalogs::with_pin`) is offered only that row.
                    if let Some(row) = self.catalogs.pinned(&path) {
                        cat.candidates
                            .retain(|c| c.generic.is_none() && c.name == row);
                    }
                    let used = columns.iter().map(|(f, _)| self.mentioned(*f)).collect();
                    let count = self.model.bank_count(part);
                    jobs.push(Job {
                        part,
                        path,
                        columns,
                        used,
                        catalog: cat,
                        count,
                    });
                }
            }
        }

        self.fed = self.fed_by(&jobs);
        self.surveys = self.survey_picks(&jobs);
        let coupling = self.coupling(&jobs);
        // A group waits while a value it needs is still to come from another group's picks
        // (see `next_group`), so the groups are not always searched in path order. The picks
        // are listed in path order all the same, so the order never shows.
        // The groups, and each of them split into the parts that share no value that a pick
        // can change (ADR 0057). A component keeps its group's place in the listing of picks.
        let whole = self.group_jobs(&jobs, None);
        let mut pending: Vec<(usize, Vec<usize>)> = Vec::new();
        for component in self.group_jobs(&jobs, Some(&coupling.constant)) {
            let position = whole
                .iter()
                .position(|g| g.contains(&component[0]))
                .expect("a component is part of a group");
            pending.push((position, component));
        }
        pending.sort_by_key(|(position, group)| (*position, group[0]));
        let mut steps: Vec<Step> = Vec::new();
        // Jobs that were searched together again after a failure, once: the second search is not
        // started again for them.
        let mut joined: HashSet<usize> = HashSet::new();
        while !pending.is_empty() {
            let next = self.next_group(&jobs, &pending);
            let (position, group) = pending.remove(next);
            let mark = self.mark();
            let elsewhere: HashSet<usize> = pending
                .iter()
                .flat_map(|(_, g)| g.iter().copied())
                .collect();
            self.solve_group(&jobs, &group, &coupling, elsewhere);
            let picks = self.picks.split_off(mark.picks);
            let step = Step {
                position,
                group,
                mark,
                picks,
            };
            if self.failed(&jobs, &step) {
                self.search_again_together(
                    &jobs,
                    &coupling,
                    &mut steps,
                    step,
                    &pending,
                    &mut joined,
                );
            } else {
                steps.push(step);
            }
        }
        let mut picked: Vec<(usize, Vec<Pick>)> = steps
            .into_iter()
            .map(|step| (step.position, step.picks))
            .collect();
        picked.sort_by_key(|(position, _)| *position);
        // The picks of the components of one group are listed in the order that one search of
        // the whole group would have made them.
        let mut listed: Vec<Pick> = Vec::new();
        let mut rest = picked.into_iter().peekable();
        while let Some((position, mut picks)) = rest.next() {
            let mut parts_of_group = false;
            while let Some((_, more)) = rest.next_if(|(next, _)| *next == position) {
                picks.extend(more);
                parts_of_group = true;
            }
            if parts_of_group {
                let order = self.order_group(&jobs, &whole[position]);
                picks.sort_by_key(|p| {
                    order
                        .iter()
                        .position(|&j| jobs[j].part == p.part)
                        .unwrap_or(usize::MAX)
                });
            }
            listed.extend(picks);
        }
        self.picks.extend(listed);
    }

    /// What `search_again_together` needs to put the solver back as it was before a group.
    fn mark(&self) -> Mark {
        Mark {
            model: self.model.snapshot(),
            diagnostics: self.diagnostics.len(),
            seen: self.seen.clone(),
            picks: self.picks.len(),
            unresolved: self.unresolved.len(),
            refused_counts: self.refused_counts.clone(),
            derived: self.derived.clone(),
        }
    }

    fn restore(&mut self, mark: &Mark) {
        self.model.restore(&mark.model);
        self.diagnostics.truncate(mark.diagnostics);
        self.seen = mark.seen.clone();
        self.picks.truncate(mark.picks);
        self.unresolved.truncate(mark.unresolved);
        self.refused_counts = mark.refused_counts.clone();
        self.derived = mark.derived.clone();
    }

    /// Whether a part of the step's group has no pick.
    fn failed(&self, jobs: &[Job], step: &Step) -> bool {
        step.group.iter().any(|&j| {
            self.unresolved[step.mark.unresolved..]
                .iter()
                .any(|u| u.part == jobs[j].part)
        })
    }

    /// Whether a relation reads values that parts of both groups decide: a bound between two
    /// groups that no equation joins, such as a window that one group's value must stay
    /// inside and another group's value sets.
    fn share_a_relation(coupling: &Coupling, a: &[usize], b: &[usize]) -> bool {
        coupling
            .feeding
            .iter()
            .any(|f| a.iter().any(|j| f.contains(j)) && b.iter().any(|j| f.contains(j)))
    }

    /// A group failed, and an earlier group shares a relation with it (a bound that reads a
    /// value of each). The earlier group was searched while that relation waited for the parts
    /// that feed it, so its first fit may leave nothing for this one; and which of two such
    /// groups comes first is decided by the path order of their names. So the groups are
    /// searched again with the failed one first: the others are then judged by the relation
    /// against its picks, as it was judged against theirs. The board is the same whatever the
    /// instances are called, which was not so before. If that does not find parts for every
    /// group, the solver is put back as the first failure left it, and its message stands.
    fn search_again_together(
        &mut self,
        jobs: &[Job],
        coupling: &Coupling,
        steps: &mut Vec<Step>,
        failed: Step,
        pending: &[(usize, Vec<usize>)],
        joined: &mut HashSet<usize>,
    ) {
        let linked: Vec<usize> = (0..steps.len())
            .filter(|&i| {
                !steps[i].group.iter().any(|j| joined.contains(j))
                    && Self::share_a_relation(coupling, &steps[i].group, &failed.group)
            })
            .collect();
        if linked.is_empty() || failed.group.iter().any(|j| joined.contains(j)) {
            steps.push(failed);
            return;
        }
        let first = linked[0];
        // The groups from the first linked one on, with the failed one ahead of them: the order
        // of the second attempt. Those the failed group shares nothing with keep the order they
        // had, after the linked ones.
        let order: Vec<(usize, Vec<usize>)> =
            std::iter::once((failed.position, failed.group.clone()))
                .chain(
                    linked
                        .iter()
                        .map(|&i| (steps[i].position, steps[i].group.clone())),
                )
                .chain(
                    (first..steps.len())
                        .filter(|i| !linked.contains(i))
                        .map(|i| (steps[i].position, steps[i].group.clone())),
                )
                .collect();
        // What the first failure left, which a failed retry puts back whole: `restore` cuts the
        // lists of diagnostics and unresolved parts at the length of a mark, and the retry may
        // add its own after that.
        let left_as_failed = self.mark();
        let first_diagnostics = self.diagnostics.clone();
        let first_unresolved = self.unresolved.clone();
        self.restore(&steps[first].mark.clone());
        let mut new_steps: Vec<Step> = Vec::new();
        let mut ok = true;
        for (n, (position, group)) in order.iter().cloned().enumerate() {
            let mark = self.mark();
            let elsewhere: HashSet<usize> = pending
                .iter()
                .map(|(_, g)| g)
                .chain(order[n + 1..].iter().map(|(_, g)| g))
                .flat_map(|g| g.iter().copied())
                .collect();
            self.solve_group(jobs, &group, coupling, elsewhere);
            let picks = self.picks.split_off(mark.picks);
            let step = Step {
                position,
                group,
                mark,
                picks,
            };
            if self.failed(jobs, &step) {
                ok = false;
                break;
            }
            new_steps.push(step);
        }
        if ok {
            steps.truncate(first);
            joined.extend(order.iter().flat_map(|(_, g)| g.iter().copied()));
            steps.extend(new_steps);
        } else {
            self.restore(&left_as_failed);
            self.diagnostics = first_diagnostics;
            self.unresolved = first_unresolved;
            steps.push(failed);
        }
    }

    /// The fields that a pick can change: every field worked out, however indirectly, from the
    /// columns (and the count) of a part that is to be chosen.
    fn fed_by(&self, jobs: &[Job]) -> HashSet<FieldId> {
        self.fed_from(
            jobs.iter()
                .flat_map(|job| job.columns.iter().map(|(f, _)| *f).chain(job.count)),
        )
    }

    /// `fed_by`, before the catalogs are read: the columns (and the count) of every part that
    /// has a catalog, whether or not it will be found.
    fn fed_at_start(&self) -> HashSet<FieldId> {
        let roots: Vec<FieldId> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Part)
            .flat_map(|(id, i)| {
                i.catalog
                    .iter()
                    .flat_map(|c| c.fields.iter().copied())
                    .chain(self.model.bank_count(id))
                    .collect::<Vec<_>>()
            })
            .collect();
        self.fed_from(roots.into_iter())
    }

    fn fed_from(&self, roots: impl Iterator<Item = FieldId>) -> HashSet<FieldId> {
        let links = self.links();
        let mut fed: HashSet<FieldId> = HashSet::new();
        let mut todo: Vec<FieldId> = Vec::new();
        for f in roots {
            if fed.insert(f) {
                todo.push(f);
            }
        }
        while let Some(f) = todo.pop() {
            for &g in links.worked_out(f) {
                if fed.insert(g) {
                    todo.push(g);
                }
            }
        }
        fed
    }

    /// The group to search next: the first, in path order, that is not waiting for another
    /// group (see `waits_for`). Every relation is verified again once all parts are
    /// picked, so the order changes no verdict, only whether a value was known when it was
    /// needed.
    ///
    /// If every group waits (they wait on each other), the first goes first and fails or
    /// not as it would have: a field that no pick can set is the design's to state.
    fn next_group(&self, jobs: &[Job], pending: &[(usize, Vec<usize>)]) -> usize {
        (0..pending.len())
            .find(|&i| {
                let others: Vec<usize> = pending
                    .iter()
                    .enumerate()
                    .filter(|&(k, _)| k != i)
                    .flat_map(|(_, (_, group))| group.iter().copied())
                    .collect();
                !self.waits_for(jobs, &pending[i].1, &others)
            })
            .unwrap_or(0)
    }

    /// The parts of a group in the order they are searched: path order, except that a part
    /// that waits for another part of the group (see `waits_for`) comes after it.
    fn order_group(&self, jobs: &[Job], group: &[usize]) -> Vec<usize> {
        let mut left = group.to_vec();
        let mut ordered = Vec::with_capacity(group.len());
        while !left.is_empty() {
            let next = (0..left.len())
                .find(|&i| {
                    let others: Vec<usize> = left
                        .iter()
                        .enumerate()
                        .filter(|&(k, _)| k != i)
                        .map(|(_, &j)| j)
                        .collect();
                    !self.waits_for(jobs, &left[i..=i], &others)
                })
                .unwrap_or(0);
            ordered.push(left.remove(next));
        }
        ordered
    }

    /// Whether the parts `mine` should be searched after the parts `others`: a column of one
    /// of them needs a field that no pick has set yet (a capacitor's effective capacitance
    /// needs its bias, which a diode ahead of it on the rail sets with its forward voltage)
    /// and a column of one of the others can set that field. Only a pick decides such a
    /// value, so a part searched first would be refused for want of it, and the same board
    /// would build or not by the names of its parts.
    fn waits_for(&self, jobs: &[Job], mine: &[usize], others: &[usize]) -> bool {
        let wanted: Vec<FieldId> = mine.iter().flat_map(|&j| self.waits_on(&jobs[j])).collect();
        if wanted.is_empty() {
            return false;
        }
        let reach = self.feeders(&wanted);
        others
            .iter()
            .any(|&j| jobs[j].columns.iter().any(|(f, _)| reach.contains(f)))
    }

    /// The fields without a value that a job's columns need before a candidate can be judged:
    /// the bias of a derated column the design asks for, and what the rating a generic part
    /// leaves open is bounded by.
    fn waits_on(&self, job: &Job) -> Vec<FieldId> {
        let mut wanted: Vec<FieldId> = Vec::new();
        for (f, key) in &job.columns {
            let mut candidates = job.catalog.candidates.iter().filter_map(|c| c.column(key));
            let bias = candidates.clone().find_map(|v| match v {
                Value::Derated(d) => Some(d.against.clone()),
                _ => None,
            });
            if let Some(against) = bias
                && self.asked(*f)
            {
                let path = format!("{}.{against}", self.model.path_of(job.part));
                if let Some(bias) = self.model.field_at(&path)
                    && matches!(self.domain(bias), Domain::Unset)
                {
                    wanted.push(bias);
                }
            }
            let open = candidates.any(|v| matches!(v, Value::Open(Sense::AtLeast | Sense::AtMost)));
            if open && matches!(self.domain(*f), Domain::Unset) {
                for r in self.model.relations() {
                    let other = match (&r.op, &r.lhs, &r.rhs) {
                        (CmpOp::Ge, RExpr::Field(g), other)
                        | (CmpOp::Le, other, RExpr::Field(g))
                            if g == f =>
                        {
                            other
                        }
                        _ => continue,
                    };
                    let mut read = Vec::new();
                    fields_in(other, &mut read);
                    wanted.extend(read.into_iter().filter(|&g| {
                        matches!(self.domain(g), Domain::Unset)
                            && !self.model.is_inside(self.model.field(g).owner, job.part)
                    }));
                }
            }
        }
        wanted.sort();
        wanted.dedup();
        wanted
    }

    /// Every field without a value that something in `wanted` is worked out from, however
    /// indirectly, and `wanted` itself: what propagation would carry a pick's value along.
    /// A field that has a value is not followed, since nothing waits for it.
    fn feeders(&self, wanted: &[FieldId]) -> HashSet<FieldId> {
        let mut from: HashMap<FieldId, Vec<FieldId>> = HashMap::new();
        for &(a, b) in self.model.equalities() {
            from.entry(a).or_default().push(b);
            from.entry(b).or_default().push(a);
        }
        for r in self.model.relations() {
            if let (CmpOp::EqEq | CmpOp::Narrow, RExpr::Field(out)) = (&r.op, &r.lhs) {
                let mut reads = Vec::new();
                fields_in(&r.rhs, &mut reads);
                from.entry(*out).or_default().extend(reads);
            }
        }
        for b in self.model.balances().iter().filter(|b| !b.returns) {
            for d in &b.draws {
                from.entry(d.field)
                    .or_default()
                    .extend(b.draws.iter().map(|o| o.field).filter(|&o| o != d.field));
            }
        }
        let mut reach: HashSet<FieldId> = wanted.iter().copied().collect();
        let mut todo: Vec<FieldId> = wanted.to_vec();
        while let Some(f) = todo.pop() {
            for &g in from.get(&f).into_iter().flatten() {
                if matches!(self.domain(g), Domain::Unset) && reach.insert(g) {
                    todo.push(g);
                }
            }
        }
        reach
    }

    /// A keyed catalog (`catalog mcu by .mpn`) is not searched: the design names the row.
    /// If it has not, say which rows it could name. True if the key was missing and
    /// reported. A key that no row fits is left to the search, whose message says what the
    /// design asks for.
    fn unnamed_key(
        &mut self,
        part: InstId,
        path: &str,
        catalog: &netsmith_check::Catalog,
        columns: &[(FieldId, String)],
        cat: &Catalog,
    ) -> bool {
        let Some(key) = catalog.key else { return false };
        if !matches!(self.domain(key), Domain::Unset) {
            return false;
        }
        let key_name = columns
            .iter()
            .find(|(f, _)| *f == key)
            .map(|(_, k)| k.clone())
            .unwrap_or_default();
        let mut names: Vec<String> = Vec::new();
        for c in cat
            .candidates
            .iter()
            .filter(|c| self.fits(part, columns, c))
        {
            if let Some(Value::Text(t)) = c.column(&key_name)
                && !names.contains(t)
            {
                names.push(t.clone());
            }
        }
        if names.is_empty() {
            return false;
        }
        let inst = self.model.instance(part);
        let ty = inst.type_name.clone();
        let (source, span) = inst.at();
        self.report(
            source,
            span,
            format!(
                "`{path}` is {} `{ty}`; name the part with .{key_name}: {}",
                article(&ty),
                names.join(", ")
            ),
        );
        self.unresolved.push(Unresolved {
            part,
            reason: format!(
                "its `{}` catalog needs `.{key_name}` to say which part",
                cat.name
            ),
        });
        true
    }

    /// The classes of fields that propagation joins, and which jobs' columns and parts are in
    /// them. Only what propagation does joins fields: a connection, an equation (`==` or
    /// `&=`) and the currents of a balance. A bound (`>=`, `<=`) only judges.
    fn coupling(&self, jobs: &[Job]) -> Coupling {
        let n_fields = self.model.fields().count();
        let mut parent: Vec<usize> = (0..n_fields).collect();
        for (a, b) in self.model.equalities() {
            union(&mut parent, a.0, b.0);
        }
        for r in self.model.relations() {
            if !matches!(r.op, CmpOp::EqEq | CmpOp::Narrow) {
                continue;
            }
            let mut fields = Vec::new();
            fields_in(&r.lhs, &mut fields);
            fields_in(&r.rhs, &mut fields);
            for w in fields.windows(2) {
                union(&mut parent, w[0].0, w[1].0);
            }
        }
        for b in self.model.balances() {
            for w in b.draws.windows(2) {
                union(&mut parent, w[0].field.0, w[1].field.0);
            }
        }
        let class: Vec<usize> = (0..n_fields).map(|f| find(&mut parent, f)).collect();
        // A pick sets the columns and, for a bank, the count (ADR 0052), so a relation that
        // reads only the count (`.a.count == .b.count`) depends on the bank's choice as much
        // as one that reads a column.
        let columns = jobs
            .iter()
            .map(|job| {
                job.columns
                    .iter()
                    .map(|(f, _)| class[f.0])
                    .chain(job.count.map(|f| class[f.0]))
                    .collect()
            })
            .collect();
        let mut inside: Vec<HashSet<usize>> = vec![HashSet::new(); jobs.len()];
        for (id, field) in self.model.fields() {
            for (j, job) in jobs.iter().enumerate() {
                if self.model.is_inside(field.owner, job.part) {
                    inside[j].insert(class[id.0]);
                }
            }
        }
        // What each field is worked out from: an equation's right side for its left side, a
        // connection both ways, and the draws of a balance each from the others.
        let mut from: HashMap<FieldId, Vec<FieldId>> = HashMap::new();
        for &(a, b) in self.model.equalities() {
            from.entry(a).or_default().push(b);
            from.entry(b).or_default().push(a);
        }
        for r in self.model.relations() {
            if let (CmpOp::EqEq | CmpOp::Narrow, RExpr::Field(out)) = (&r.op, &r.lhs) {
                let mut reads = Vec::new();
                fields_in(&r.rhs, &mut reads);
                from.entry(*out).or_default().extend(reads);
            }
        }
        for b in self.model.balances().iter().filter(|b| !b.returns) {
            for d in &b.draws {
                from.entry(d.field)
                    .or_default()
                    .extend(b.draws.iter().map(|o| o.field).filter(|&o| o != d.field));
            }
        }
        // A derated column is worked out from the part's bias (ADR 0013): a row's capacitance
        // at the voltage across it. The bias is the rail's voltage, which another part's pick
        // can decide, so what reads such a column reads that too.
        for job in jobs {
            for (f, key) in &job.columns {
                let against = job
                    .catalog
                    .candidates
                    .iter()
                    .find_map(|c| match c.column(key) {
                        Some(Value::Derated(d)) => Some(d.against.clone()),
                        _ => None,
                    });
                if let Some(against) = against {
                    let path = format!("{}.{against}", self.model.path_of(job.part));
                    if let Some(bias) = self.model.field_at(&path) {
                        from.entry(*f).or_default().push(bias);
                    }
                }
            }
        }
        let mut job_of: HashMap<FieldId, usize> = HashMap::new();
        for (j, job) in jobs.iter().enumerate() {
            for (f, _) in &job.columns {
                job_of.insert(*f, j);
            }
            if let Some(count) = job.count {
                job_of.insert(count, j);
            }
        }
        let constant = self.constants(jobs);
        let feeding = self
            .model
            .relations()
            .iter()
            .map(|r| {
                let mut start = Vec::new();
                fields_in(&r.lhs, &mut start);
                fields_in(&r.rhs, &mut start);
                start.retain(|f| !constant[f.0]);
                let mut seen: HashSet<FieldId> = start.iter().copied().collect();
                let mut todo = start;
                let mut found = HashSet::new();
                while let Some(f) = todo.pop() {
                    if let Some(&j) = job_of.get(&f) {
                        found.insert(j);
                    }
                    for &g in from.get(&f).into_iter().flatten() {
                        if !constant[g.0] && seen.insert(g) {
                            todo.push(g);
                        }
                    }
                }
                found
            })
            .collect();
        Coupling {
            columns,
            inside,
            class,
            feeding,
            constant,
        }
    }

    /// Which fields have a value that no choice of parts can change: the field, with the
    /// fields joined to it by `~`, has a value now (stated, or settled by the first
    /// propagation), and none of them is worked out from anything (the left side of an
    /// equation that reads a field, a term of a balance of currents) or is a column of a
    /// part still to be chosen. A rail whose voltage the board states, and that nothing
    /// derives, is such a value: the parts on it read the same range whatever they are.
    fn constants(&self, jobs: &[Job]) -> Vec<bool> {
        let size = self.model.fields().map(|(f, _)| f.0 + 1).max().unwrap_or(0);
        let mut parent: Vec<usize> = (0..size).collect();
        for &(a, b) in self.model.equalities() {
            union(&mut parent, a.0, b.0);
        }
        let mut derivable = vec![false; size];
        let mut valued = vec![false; size];
        for r in self.model.relations() {
            if let (CmpOp::EqEq | CmpOp::Narrow, RExpr::Field(out)) = (&r.op, &r.lhs) {
                let mut reads = Vec::new();
                fields_in(&r.rhs, &mut reads);
                if !reads.is_empty() {
                    derivable[find(&mut parent, out.0)] = true;
                }
            }
        }
        for b in self.model.balances().iter().filter(|b| !b.returns) {
            for d in &b.draws {
                derivable[find(&mut parent, d.field.0)] = true;
            }
        }
        for job in jobs {
            for (f, _) in &job.columns {
                derivable[find(&mut parent, f.0)] = true;
            }
            if let Some(count) = job.count {
                derivable[find(&mut parent, count.0)] = true;
            }
        }
        for (id, field) in self.model.fields() {
            if field.domain != Domain::Unset {
                valued[find(&mut parent, id.0)] = true;
            }
        }
        (0..size)
            .map(|f| {
                let root = find(&mut parent, f);
                valued[root] && !derivable[root]
            })
            .collect()
    }

    /// Split the parts to pick into groups that share a relation. Parts in different
    /// groups cannot affect each other, so each group is searched on its own: one part
    /// with no valid choice does not undo the picks of parts unrelated to it.
    ///
    /// With `constant` (see `Solver::constants`), a field whose value no pick changes joins
    /// nothing: two parts that read the same stated rail, and have no other value in common,
    /// are two groups (ADR 0057).
    fn group_jobs(&self, jobs: &[Job], constant: Option<&[bool]>) -> Vec<Vec<usize>> {
        let fixed = |f: FieldId| constant.is_some_and(|c| c[f.0]);
        // Fields joined by a connection hold one value, so they count as one.
        let n_fields = self.model.fields().count();
        let mut fparent: Vec<usize> = (0..n_fields).collect();
        for (a, b) in self.model.equalities() {
            union(&mut fparent, a.0, b.0);
        }
        let class = |f: FieldId, fparent: &mut Vec<usize>| find(fparent, f.0);

        // The jobs that each class of values is inside of: of a field, its owner and every
        // instance around the owner is a part to pick or is not.
        let job_of: HashMap<InstId, usize> = jobs
            .iter()
            .enumerate()
            .map(|(j, job)| (job.part, j))
            .collect();
        let mut jobs_of_class: HashMap<usize, Vec<usize>> = HashMap::new();
        for (id, field) in self.model.fields() {
            if fixed(id) {
                continue;
            }
            let class = class(id, &mut fparent);
            let mut at = Some(field.owner);
            while let Some(inst) = at {
                if let Some(&j) = job_of.get(&inst) {
                    jobs_of_class.entry(class).or_default().push(j);
                }
                at = self.model.instance(inst).parent;
            }
        }
        for list in jobs_of_class.values_mut() {
            list.sort_unstable();
            list.dedup();
        }

        let mut jparent: Vec<usize> = (0..jobs.len()).collect();
        for r in self.model.relations() {
            let mut fields = Vec::new();
            fields_in(&r.lhs, &mut fields);
            fields_in(&r.rhs, &mut fields);
            fields.retain(|&f| !fixed(f));
            let mut touched: Vec<usize> = fields
                .iter()
                .filter_map(|&f| jobs_of_class.get(&class(f, &mut fparent)))
                .flatten()
                .copied()
                .collect();
            touched.sort_unstable();
            touched.dedup();
            for w in touched.windows(2) {
                union(&mut jparent, w[0], w[1]);
            }
        }

        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut index_of: std::collections::HashMap<usize, usize> = Default::default();
        for j in 0..jobs.len() {
            let root = find(&mut jparent, j);
            let g = *index_of.entry(root).or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            });
            groups[g].push(j);
        }
        groups
    }

    /// Find a choice for every part in a group that satisfies every relation together,
    /// backing up when a later part has nothing that works. The first solution in catalog
    /// order (parts in path order) is the one used.
    fn solve_group(
        &mut self,
        jobs: &[Job],
        group: &[usize],
        coupling: &Coupling,
        elsewhere: HashSet<usize>,
    ) {
        let ordered = self.order_group(jobs, group);
        let group = &ordered[..];
        let mut search = Search {
            elsewhere,
            ..Search::default()
        };
        // A part with no candidate that fits what the design already says cannot be chosen
        // whatever the others are, so that is the cause to report. Searching first would
        // blame whichever earlier part's choices failed on the way to it.
        // A part that waits for another of the group is not starved yet: what it needs
        // is set by that part's pick, and the search finds it out if it still has nothing.
        let starved = group.iter().copied().find(|&j| {
            let job = &jobs[j];
            let others: Vec<usize> = group.iter().copied().filter(|&o| o != j).collect();
            !job.catalog
                .candidates
                .iter()
                .any(|c| self.fits(job.part, &job.columns, c))
                && !self.waits_for(jobs, &[j], &others)
        });
        // A part that every candidate of fails whatever the others are is the cause: say so
        // now, and do not search the combinations of the parts before it (ADR 0057).
        let hopeless = if starved.is_none() && group.len() > 1 {
            self.hopeless(jobs, group, coupling, &search.elsewhere)
        } else {
            None
        };
        let culprit = hopeless.as_ref().map(|h| h.0);
        if let Some((_, closest, fits)) = hopeless {
            search.closest = Some(closest);
            search.first_level_fits = fits;
        }
        let solved = starved.is_none()
            && culprit.is_none()
            && matches!(
                self.search(jobs, group, 0, &mut search, coupling),
                Outcome::Solved
            );
        self.trials += search.trials;
        if solved {
            self.picks.extend(search.picks);
            return;
        }

        let names: Vec<String> = group
            .iter()
            .map(|&j| format!("`{}`", jobs[j].path))
            .collect();
        let first = &jobs[culprit.unwrap_or(group[0])];
        let part = self.model.instance(first.part);
        let (part_source, part_span) = part.at();

        if search.gave_up {
            self.report(
                part_source,
                part_span,
                format!(
                    "gave up after trying {MAX_TRIALS} combinations of candidates for {}; narrow what they need or give them fewer choices",
                    names.join(" and ")
                ),
            );
        } else if let Some(c) = search.closest.take() {
            let (source, span) = c.at;
            let message = if group.len() == 1 || culprit.is_some() {
                format!(
                    "no `{}` candidate satisfies `{}`: the closest of the {} that fit, {}, fails because {}",
                    first.catalog.name,
                    first.path,
                    search.first_level_fits,
                    c.assignment[0],
                    c.message
                )
            } else {
                format!(
                    "no combination of candidates satisfies {} together: the closest, {}, fails because {}",
                    names.join(" and "),
                    c.assignment.join(" and "),
                    c.message
                )
            };
            let notes: Vec<String> = group
                .iter()
                .flat_map(|&j| {
                    [
                        self.derating_note(&jobs[j]),
                        self.package_note(&jobs[j]),
                        self.blank_note(&jobs[j]),
                    ]
                })
                .flatten()
                .collect();
            let message = if notes.is_empty() {
                message
            } else {
                format!("{message} ({})", notes.join("; "))
            };
            self.report(source, span, message);
        } else {
            // Nothing fit what the design already says, so there was nothing to try.
            let j = starved.or(search.no_fit).unwrap_or(group[0]);
            let job = &jobs[j];
            let wants: Vec<String> = job
                .columns
                .iter()
                .map(|(f, key)| (*f, key.as_str()))
                .chain(job.count.map(|f| (f, "count")))
                .filter_map(|(f, key)| match self.domain(f) {
                    Domain::Unset => None,
                    d => Some(format!("{key} within {}", self.model.show_value(f, &d))),
                })
                .collect();
            let wants = if wants.is_empty() {
                "nothing in particular".to_string()
            } else {
                wants.join(" and ")
            };
            let notes: Vec<String> = [
                self.derating_note(job),
                self.package_note(job),
                self.blank_note(job),
            ]
            .into_iter()
            .flatten()
            .collect();
            let wants = if notes.is_empty() {
                wants
            } else {
                format!("{wants} ({})", notes.join("; "))
            };
            let inst = self.model.instance(job.part);
            let (source, span) = inst.at();
            self.report(
                source,
                span,
                format!(
                    "the `{}` catalog has nothing for `{}`, which needs {wants}",
                    job.catalog.name, job.path
                ),
            );
        }
        for &j in group {
            if let Some(count) = jobs[j].count {
                self.refused_counts.insert(count);
            }
            self.unresolved.push(Unresolved {
                part: jobs[j].part,
                reason: if group.len() == 1 {
                    "no candidate satisfies its requirements".into()
                } else {
                    "no combination of candidates satisfies these parts together".into()
                },
            });
        }
    }

    /// The part of a group that no candidate can serve, whatever the other parts are: each
    /// candidate that fits is tried alone, and every one that fits is refused by a relation
    /// whose value only this part's choice decides (no other part's column is among what it
    /// is worked out from, `Coupling::feeding`). The others cannot help it, so searching them
    /// first would only repeat the failure. With its place in the group, the candidate that
    /// came nearest, and how many fitted. A part with a bank count, and a part with any
    /// candidate that passes alone or is refused for what other parts decide, is not it.
    fn hopeless(
        &mut self,
        jobs: &[Job],
        group: &[usize],
        coupling: &Coupling,
        elsewhere: &HashSet<usize>,
    ) -> Option<(usize, Closest, usize)> {
        for &j in group {
            let job = &jobs[j];
            let fitting: Vec<&Candidate> = job
                .catalog
                .candidates
                .iter()
                .filter(|c| self.fits(job.part, &job.columns, c))
                .collect();
            if fitting.is_empty() {
                continue;
            }
            // A bank is tried at every count its range holds: a row is refused only when no
            // count of it is accepted (ADR 0052). A part with no count has the one pass.
            let counts = self.counts_of(job);
            if counts.is_empty() {
                continue;
            }
            let mut later = elsewhere.clone();
            later.extend(group.iter().copied().filter(|&o| o != j));
            let mut nearest: Option<Closest> = None;
            // A refused candidate, with the columns that the relation that refused it reads:
            // a candidate that is equal to it in those, and in every column the design uses, is
            // refused the same way. A column that only that relation reads (a bound on one
            // field, which no other field names) is not among the used ones, so the used
            // columns alone would call a row with another value there a twin.
            let mut dead: Vec<(&Candidate, Vec<&str>)> = Vec::new();
            let mut every = true;
            'candidates: for c in &fitting {
                if dead.iter().any(|(d, read)| {
                    self.interchangeable(job, d, c)
                        && read.iter().all(|k| d.column(k) == c.column(k))
                }) {
                    continue;
                }
                let mut read: Vec<&str> = Vec::new();
                for &count in &counts {
                    let checkpoint = self.model.checkpoint();
                    let bank = job.count.zip(count);
                    let outcome =
                        self.try_candidate(job.part, &job.columns, c, bank, coupling, &later);
                    self.model.rollback(checkpoint);
                    match outcome {
                        Err(r)
                            if r.relation
                                .is_some_and(|i| coupling.feeding[i].iter().all(|&k| k == j)) =>
                        {
                            read.extend(
                                job.columns
                                    .iter()
                                    .filter(|(f, _)| r.own.contains(f))
                                    .map(|(_, key)| key.as_str()),
                            );
                            if nearest.as_ref().is_none_or(|n| r.margin < n.margin) {
                                nearest = Some(Closest {
                                    depth: 0,
                                    margin: r.margin,
                                    assignment: vec![choice_name(c, count)],
                                    message: r.message,
                                    at: r.at,
                                });
                            }
                            // A refusal that reads nothing worked out from the count is the
                            // same at every count (as in `search`).
                            if let Some(cf) = job.count
                                && !coupling.classes_of(&r.own).contains(&coupling.class[cf.0])
                            {
                                break;
                            }
                        }
                        _ => {
                            every = false;
                            break 'candidates;
                        }
                    }
                }
                dead.push((c, read));
            }
            if every && let Some(closest) = nearest {
                return Some((j, closest, fitting.len()));
            }
        }
        None
    }

    /// Depth-first: choose a candidate for the part at `depth`, then the rest. Every
    /// relation that can be evaluated is checked as soon as it can, so a bad choice is
    /// dropped before the parts after it are tried.
    ///
    /// Conflict-directed backjumping: a failure says which earlier parts' choices it
    /// depends on (see [`Coupling`]). When every way of choosing the parts after this one
    /// fails for reasons that do not include this part, no other candidate for it can help,
    /// so the search goes straight back to the nearest part the failure does depend on,
    /// instead of choosing the parts in between again and reaching the same failure each
    /// time. What it skips could only fail the same way, so the first solution, and the
    /// closest failure, are those of the plain search; the remaining candidates of a skipped
    /// part are still tried (without going deeper) so that the closest failure is the same.
    fn search(
        &mut self,
        jobs: &[Job],
        group: &[usize],
        depth: usize,
        search: &mut Search,
        coupling: &Coupling,
    ) -> Outcome {
        if depth == group.len() {
            return Outcome::Solved;
        }
        let job = &jobs[group[depth]];
        let inside = &coupling.inside[group[depth]];
        let fitting: Vec<&Candidate> = job
            .catalog
            .candidates
            .iter()
            .filter(|c| self.fits(job.part, &job.columns, c))
            .collect();
        if depth == 0 {
            search.first_level_fits = fitting.len();
        }
        // Which candidates fit depends on the domains of this part's fields.
        let mut conflict = coupling.earlier(group, depth, inside);
        if fitting.is_empty() {
            search.no_fit.get_or_insert(group[depth]);
            return Outcome::Failed(conflict);
        }

        // The whole numbers of a bank's count that the design leaves open, fewest first (ADR
        // 0052); one pass with no count for a part that has none.
        let counts = self.counts_of(job);
        if counts.is_empty() {
            search.no_fit.get_or_insert(group[depth]);
            return Outcome::Failed(conflict);
        }

        // The parts still to be chosen after this one, here or in another group: a value
        // that one of them feeds is not worked out yet, and a bound on it cannot be judged.
        let mut later = search.elsewhere.clone();
        later.extend(group[depth + 1..].iter().copied());

        // Set once the parts after this one failed without depending on it: the choices of
        // the parts before it that they depend on.
        let mut jump: Option<BTreeSet<usize>> = None;
        // Choices whose every completion failed. A later one that behaves the same fails the
        // same way, so it is not tried.
        let mut failed: Vec<(&Candidate, Option<f64>)> = Vec::new();
        // Candidates rejected for a reason that no count changes (a rating too low for the
        // rail): a larger count is not tried with them.
        let mut count_free_failure = vec![false; fitting.len()];
        for (pass, &count) in counts.iter().enumerate() {
            for (index, c) in fitting.iter().enumerate() {
                let examined = pass * fitting.len() + index;
                if count_free_failure[index] {
                    continue;
                }
                if failed
                    .iter()
                    .any(|&(f, n)| n == count && self.interchangeable(job, f, c))
                {
                    continue;
                }
                if search.trials >= MAX_TRIALS {
                    search.gave_up = true;
                    return Outcome::GaveUp;
                }
                search.trials += 1;
                let checkpoint = self.model.checkpoint();
                let bank = job.count.zip(count);
                match self.try_candidate(job.part, &job.columns, c, bank, coupling, &later) {
                    Ok(_) if jump.is_some() => {
                        // The parts after this one would fail again, as they did.
                        self.model.rollback(checkpoint);
                        failed.push((c, count));
                    }
                    Ok(needs) => {
                        search
                            .chosen
                            .push(format!("{} = {}", job.path, choice_name(c, count)));
                        search.picks.push(Pick {
                            part: job.part,
                            catalog: job.catalog.name.clone(),
                            candidate: c.name.clone(),
                            label: c.label.clone(),
                            lcsc: c.lcsc.clone(),
                            mpn: c.mpn.clone(),
                            library: c.library,
                            assembly: c.assembly.clone(),
                            generic: c.generic.as_ref().map(|g| g.with_needs(&needs)),
                            rejected: examined,
                            count: count.map_or(1, |n| n as u32),
                        });
                        let outcome = self.search(jobs, group, depth + 1, search, coupling);
                        if let Outcome::Solved = outcome {
                            self.model.commit(checkpoint);
                            return outcome;
                        }
                        search.chosen.pop();
                        search.picks.pop();
                        self.model.rollback(checkpoint);
                        match outcome {
                            Outcome::Failed(mut depends_on) => {
                                if depends_on.remove(&depth) {
                                    conflict.extend(depends_on);
                                } else {
                                    jump = Some(depends_on);
                                }
                            }
                            _ => return Outcome::GaveUp,
                        }
                        failed.push((c, count));
                    }
                    Err(r) => {
                        // `try_candidate` put the model back.
                        self.model.commit(checkpoint);
                        let mut assignment = search.chosen.clone();
                        assignment.push(format!("{} = {}", job.path, choice_name(c, count)));
                        if search.closest.as_ref().is_none_or(|best| {
                            depth > best.depth || (depth == best.depth && r.margin < best.margin)
                        }) {
                            search.closest = Some(Closest {
                                depth,
                                margin: r.margin,
                                assignment: if group.len() == 1 {
                                    vec![choice_name(c, count)]
                                } else {
                                    assignment
                                },
                                message: r.message,
                                at: r.at,
                            });
                        }
                        // The rejection was worked out from these fields and the candidate's own.
                        let mut reads = coupling.classes_of(&r.reads);
                        reads.extend(inside.iter().copied());
                        conflict.extend(coupling.earlier(group, depth, &reads));
                        // A failure that reads nothing worked out from the count is the same
                        // at every count.
                        if let (Some(cf), Some(_)) = (job.count, count)
                            && !coupling.classes_of(&r.own).contains(&coupling.class[cf.0])
                        {
                            count_free_failure[index] = true;
                        }
                    }
                }
            }
        }
        Outcome::Failed(jump.unwrap_or(conflict))
    }

    /// The counts a part's search goes through, fewest first (ADR 0052): the whole numbers of
    /// its `.count` range, or the one number the design states, or a single pass with none for
    /// a part that has no count or whose count is unset (that is one part). Empty if the range
    /// holds no whole number.
    fn counts_of(&self, job: &Job) -> Vec<Option<f64>> {
        let Some(field) = job.count else {
            return vec![None];
        };
        let Domain::Quantity(q) = self.domain(field) else {
            return vec![None];
        };
        let (first, last) = (q.lo().ceil().max(1.0), q.hi().floor().min(MAX_BANK));
        let mut counts = Vec::new();
        let mut n = first;
        while n <= last {
            counts.push(Some(n));
            n += 1.0;
        }
        counts
    }

    /// Whether the design cannot tell two candidates of one part apart: they are equal in
    /// every column that a relation or a connection uses. The others (the package, the
    /// color, a capacitance nobody relates) are only matched against what the design says,
    /// which both candidates already satisfy, so they cannot change any other part's choice
    /// or any relation's verdict. A candidate that behaves like one whose every completion
    /// failed fails the same way; skipping it is what keeps a large catalog from being
    /// searched once per copy of the same electrical part.
    fn interchangeable(&self, job: &Job, a: &Candidate, b: &Candidate) -> bool {
        job.columns
            .iter()
            .zip(&job.used)
            .all(|((_, key), &used)| !used || a.column(key) == b.column(key))
    }

    /// Whether anything but the part's own constraints reads a field: a connection joins it
    /// to another field, a net sums it, or a relation also names another field. A relation
    /// that names only this field (`.effective_capacitance >= 1uF`) only judges the part that
    /// holds it.
    fn mentioned(&self, field: FieldId) -> bool {
        self.reads().shared(field)
    }

    /// What the design reads, worked out again when the relations are not those it was worked
    /// out for.
    fn reads(&self) -> std::rc::Rc<reads::Reads> {
        let mut cache = self.reads.borrow_mut();
        match cache.as_ref() {
            Some(r) if r.is_of(&self.model) => std::rc::Rc::clone(r),
            _ => {
                let r = std::rc::Rc::new(reads::Reads::of(&self.model));
                *cache = Some(std::rc::Rc::clone(&r));
                r
            }
        }
    }

    fn fits(&self, part: InstId, columns: &[(FieldId, String)], c: &Candidate) -> bool {
        if !self.package_has_the_pins(part, c) {
            return false;
        }
        columns.iter().all(|(f, key)| {
            let domain = &self.model.field(*f).domain;
            match (c.column(key), domain) {
                // A row with no value there fits no design that asks for one (a rating it
                // does not state cannot be shown to be met). A design that does not mention
                // the field has no use for the value, so the row is still offered, but only
                // for a column that a catalog may leave empty (`Standard::is_optional`): an empty
                // package or resistance is not a value that nothing needs.
                (None, _) => self.may_be_empty(part, key) && !self.asked(*f),
                (Some(Value::Open(sense)), _) => self.need_of(*f, *sense).is_some(),
                (Some(Value::Derated(d)), domain) => match self.derated(part, d) {
                    Ok(q) => match domain {
                        Domain::Quantity(want) => want.encloses(&q).unwrap_or(false),
                        Domain::Unset => true,
                        Domain::Text(_) => false,
                    },
                    // Not knowing the value matters only if the design asks for it.
                    Err(_) => !self.asked(*f),
                },
                (Some(_), Domain::Unset) => true,
                (Some(Value::Quantity(q)), Domain::Quantity(d)) => d.encloses(q).unwrap_or(false),
                (Some(Value::Text(t)), Domain::Text(d)) => t == d,
                _ => false,
            }
        })
    }

    /// Whether a candidate's package has every pin that the board connects (ADR 0016). A
    /// package that lacks one cannot be used, whatever else the row says.
    fn package_has_the_pins(&self, part: InstId, c: &Candidate) -> bool {
        match c.column("package") {
            Some(Value::Text(package)) => self.model.missing_in(part, package).is_empty(),
            _ => true,
        }
    }

    /// What to add to a message when candidates were left out for lacking a pin that the
    /// board connects.
    fn package_note(&self, job: &Job) -> Option<String> {
        let mut left_out: Vec<(String, Vec<String>)> = Vec::new();
        for c in &job.catalog.candidates {
            let Some(Value::Text(package)) = c.column("package") else {
                continue;
            };
            if left_out.iter().any(|(p, _)| p == package) {
                continue;
            }
            let lacking = self.model.missing_in(job.part, package);
            if !lacking.is_empty() {
                let names = lacking
                    .iter()
                    .map(|&p| format!("`{}`", self.model.path_below(p, job.part)))
                    .collect();
                left_out.push((package.clone(), names));
            }
        }
        if left_out.is_empty() {
            return None;
        }
        let each: Vec<String> = left_out
            .iter()
            .map(|(package, pins)| {
                format!(
                    "package `{package}` has no {}, which the board connects",
                    pins.join(", ")
                )
            })
            .collect();
        Some(format!("rows were left out because {}", each.join("; ")))
    }

    /// What to add to a message when rows were left out for an empty cell: a row with no
    /// value for a column the design asks about fits no design, so a part without a rating
    /// (`max_power`) is never picked, and the message says how many rows that is.
    fn blank_note(&self, job: &Job) -> Option<String> {
        let blank: Vec<String> = job
            .columns
            .iter()
            .filter(|(f, key)| !self.may_be_empty(job.part, key) || self.asked(*f))
            .filter_map(|(_, key)| {
                let n = job
                    .catalog
                    .candidates
                    .iter()
                    .filter(|c| c.column(key).is_none())
                    .count();
                (n > 0).then(|| format!("{n} for `{key}`"))
            })
            .collect();
        (!blank.is_empty()).then(|| {
            format!(
                "rows with an empty cell are never offered, since a part with no stated rating cannot be shown to stay within it: {}",
                blank.join(", ")
            )
        })
    }

    /// Whether a row of the part's catalog may have no value in `column` and still be offered
    /// to a design that does not mention it (ADR 0055): only the columns added after the
    /// files were written. For any other column an empty cell is a value the design needs and
    /// does not get, and the row is never offered.
    fn may_be_empty(&self, part: InstId, column: &str) -> bool {
        self.model
            .instance(part)
            .catalog
            .as_ref()
            .is_some_and(|c| crate::table::is_optional(&c.name, column))
    }

    /// Whether the design says anything about a field: it narrows it, a relation uses it, a
    /// connection joins it to another field, or a balance of currents sums it. A value that
    /// reaches the field by any of these routes is a requirement on it, and a row with no
    /// value for it cannot be shown to meet that.
    fn asked(&self, field: FieldId) -> bool {
        if !matches!(self.domain(field), Domain::Unset) {
            return true;
        }
        self.reads().read(field)
    }

    /// A candidate's value after derating, at the bias the design gives its part.
    fn derated(&self, part: InstId, d: &Derating) -> Result<Quantity, Underated> {
        let path = format!("{}.{}", self.model.path_of(part), d.against);
        let bias = match self.model.field_at(&path).map(|f| self.domain(f)) {
            Some(Domain::Quantity(q)) => q,
            _ => return Err(Underated::NoBias(path)),
        };
        d.at(&bias).map_err(Underated::Curve)
    }

    /// What to add to a message when a derated column hides candidates: the design gave no
    /// bias, or some candidates have no curve or none that reaches the bias.
    fn derating_note(&self, job: &Job) -> Option<String> {
        for (f, key) in &job.columns {
            let derated: Vec<&Derating> = job
                .catalog
                .candidates
                .iter()
                .filter(|c| c.generic.is_none())
                .filter_map(|c| match c.column(key) {
                    Some(Value::Derated(d)) => Some(d),
                    _ => None,
                })
                .collect();
            let generic =
                job.catalog.candidates.iter().any(|c| {
                    c.generic.is_some() && matches!(c.column(key), Some(Value::Derated(_)))
                });
            if (derated.is_empty() && !generic) || !self.asked(*f) {
                continue;
            }
            let (mut none, mut short) = (0, 0);
            let (mut known_to, mut wanted) = (0.0_f64, 0.0_f64);
            for d in derated {
                match self.derated(job.part, d) {
                    Err(Underated::NoBias(path)) => {
                        return Some(format!(
                            "`{path}` is not known, so `{}.{key}` cannot be worked out: place `{}` between a rail's pos and neg, whose voltage is known, or state `{path}`",
                            job.path, job.path
                        ));
                    }
                    Err(Underated::Curve(DeratingError::NoData)) => none += 1,
                    Err(Underated::Curve(DeratingError::BeyondData {
                        wanted: w,
                        known_to: k,
                    })) => {
                        short += 1;
                        wanted = w;
                        known_to = known_to.max(k);
                    }
                    Ok(_) => {}
                }
            }
            let mut why = Vec::new();
            if none > 0 {
                why.push(format!("no derating data for {none}"));
            }
            if short > 0 {
                why.push(format!(
                    "data only up to {known_to} V, but the bias reaches {wanted} V, for {short}"
                ));
            }
            if generic {
                why.push(
                    "generic parts have no recorded bias curve, and a generic ceramic capacitor is never used where the effective capacitance matters"
                        .to_string(),
                );
            }
            if !why.is_empty() {
                return Some(format!(
                    "`{}.{key}` is unknown for some candidates, so they were not used: {}",
                    job.path,
                    why.join(" and ")
                ));
            }
        }
        None
    }

    /// Try one candidate. On success its values stay in the model; on failure the model
    /// is put back exactly as it was.
    fn try_candidate(
        &mut self,
        part: InstId,
        columns: &[(FieldId, String)],
        c: &Candidate,
        bank: Option<(FieldId, f64)>,
        coupling: &Coupling,
        later: &HashSet<usize>,
    ) -> Result<Vec<(String, Need)>, Rejection> {
        let backup = self.model.checkpoint();
        let result = self.try_candidate_from(part, columns, c, bank, coupling, later, &backup);
        // A candidate that is refused leaves the model as it was.
        match result {
            Ok(_) => self.model.commit(backup),
            Err(_) => self.model.rollback(backup),
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn try_candidate_from(
        &mut self,
        part: InstId,
        columns: &[(FieldId, String)],
        c: &Candidate,
        bank: Option<(FieldId, f64)>,
        coupling: &Coupling,
        later: &HashSet<usize>,
        backup: &Checkpoint,
    ) -> Result<Vec<(String, Need)>, Rejection> {
        let mut needs: Vec<(String, Need)> = Vec::new();
        // A bank of this many of the row (ADR 0052): the count is as much the choice as the
        // row is.
        if let Some((field, n)) = bank
            && let FieldKind::Quantity(dim) = self.model.field(field).kind
            && let Ok(q) = Quantity::point(dim, n)
        {
            self.model.set_domain(field, Domain::Quantity(q));
        }
        let reject = |message: String,
                      margin: f64,
                      at: (SourceId, Span),
                      reads: Vec<FieldId>,
                      own: Vec<FieldId>,
                      relation: Option<usize>| {
            Err(Rejection {
                message,
                margin,
                at,
                reads,
                own,
                relation,
            })
        };

        // The columns a generic part leaves open (ADR 0047), in column order.
        let mut open: Vec<(FieldId, &String, Sense)> = Vec::new();
        for (f, key) in columns {
            // A row with no value for a column that the design does not ask about stays
            // unset (`fits` let it through).
            let Some(value) = c.column(key) else { continue };
            let value = match value {
                Value::Quantity(q) => Domain::Quantity(*q),
                Value::Text(t) => Domain::Text(t.clone()),
                Value::Open(sense) => {
                    open.push((*f, key, *sense));
                    continue;
                }
                Value::Derated(d) => match self.derated(part, d) {
                    Ok(q) => Domain::Quantity(q),
                    // The design does not ask for it, and `fits` let it through.
                    Err(_) => continue,
                },
            };
            self.model.set_domain(*f, value);
        }

        // A generic part takes what the design says of the columns it leaves open, and what
        // the design says can depend on the columns it does have: the ripple of a buck
        // converter's inductor needs its inductance, and the stage asks for a saturation
        // current of the load and half that ripple. So the values that follow from the marked
        // columns are worked out first, and the demand on each open column is read after
        // them (ADR 0057). A relation's demand is taken exactly, so the relation holds with
        // nothing to spare and any row that meets the need holds it too.
        if !open.is_empty()
            && let Some(conflict) = self.propagate().into_iter().next()
        {
            return reject(
                conflict.message,
                1.0,
                (conflict.source, conflict.span),
                conflict.fields.clone(),
                conflict.fields,
                None,
            );
        }
        for (f, key, sense) in open {
            let need = self.need_of(f, sense).unwrap_or(Need::Nothing);
            if let (Need::AtLeast(x) | Need::AtMost(x), FieldKind::Quantity(dim)) =
                (&need, self.model.field(f).kind)
                && let Ok(q) = Quantity::point(dim, *x)
            {
                self.model.set_domain(f, Domain::Quantity(q));
            }
            needs.push((key.clone(), need));
        }

        if let Some(conflict) = self.propagate().into_iter().next() {
            return reject(
                conflict.message,
                1.0,
                (conflict.source, conflict.span),
                conflict.fields.clone(),
                conflict.fields,
                None,
            );
        }

        // The relations this choice can decide: those that name a field of the part, and
        // those that name a field the choice made known or narrower (a value an equation works
        // out from the part's columns: a named ratio, a module's declared current, a frequency
        // two equations away from the resistor) when no part still to be chosen feeds any
        // field the relation names. A bound on such a value judges the part just as a bound
        // written on its columns does (ADR 0057); without it the pick is made without the
        // bound and only the check afterwards refuses it. While a part that feeds the value is
        // unchosen the value is a range of what that part may still be, and a bound that
        // fails on it can still hold for the part that is chosen. Such a bound is not judged,
        // but the candidate is refused if no value of the range meets it (`verify_possible`,
        // ADR 0065, 9.5).
        let changed = self.model.changed_since(backup);
        let relations = std::sync::Arc::clone(self.model.shared_relations());
        let design = self.reads();
        let touched = design.touching(part, changed.iter().copied());
        let relevant: Vec<usize> = touched
            .iter()
            .copied()
            .filter(|&index| {
                let fields = design.fields_of(index);
                let inside = fields
                    .iter()
                    .any(|&f| self.model.is_inside(self.model.field(f).owner, part));
                // A bound that the solver added reads a value another part may still decide
                // (the right side), so it waits for those parts as a bound on a worked-out
                // value does.
                if index >= self.original_relations {
                    return inside && coupling.feeding[index].is_disjoint(later);
                }
                inside
                    || (fields.iter().any(|f| changed.contains(f))
                        && coupling.feeding[index].is_disjoint(later))
            })
            .collect();
        // The bounds the choice changed that wait for a part still to be chosen. They are not
        // judged, but one that no value of the ranges as they are can meet is refused: the
        // parts chosen later only narrow the ranges, so they cannot meet it either. Without
        // this a bound on a value that another group's picks feed (the input voltage of a
        // stage, behind a filter whose drop its load decides) never reached the pick that
        // sets the value, however plainly the pick broke it.
        let waiting: Vec<usize> = touched
            .iter()
            .copied()
            .filter(|&index| {
                index < self.original_relations
                    && matches!(relations[index].op, CmpOp::Ge | CmpOp::Le)
                    && !coupling.feeding[index].is_disjoint(later)
                    && !relevant.contains(&index)
                    && design.fields_of(index).iter().any(|f| changed.contains(f))
            })
            .collect();
        // The relations are checked in order, so the one that fails is the first, and the
        // rejection depends on every relation up to it.
        let mut reads = Vec::new();
        let judged = relevant
            .iter()
            .map(|&index| (index, true))
            .chain(waiting.iter().map(|&index| (index, false)));
        for (index, whole) in judged {
            let r = &relations[index];
            let mut own = Vec::new();
            fields_in(&r.lhs, &mut own);
            fields_in(&r.rhs, &mut own);
            reads.extend(own.iter().copied());
            let verdict = if whole {
                self.verify(r)
            } else {
                self.verify_possible(r)
            };
            if let Verdict::Fail { message, margin } = verdict {
                let message = self.relation_message(index, r, message);
                return reject(message, margin, (r.source, r.span), reads, own, Some(index));
            }
        }
        Ok(needs)
    }

    /// What the design asks of a field that a generic part leaves open (ADR 0047), or `None`
    /// if it asks something the part cannot meet by taking it: a value that other values are
    /// worked out from (`Sense::Stated`) must be stated.
    ///
    /// A stated text or range is the need. A rating (`AtLeast`, `AtMost`) with no statement
    /// is the most extreme bound that a relation of the form `rating >= expression` or
    /// `rating <= expression` puts on it, if the expression has a value. If nothing bounds
    /// it the need is `Nothing`: the field stays unknown and a check that mentions it is
    /// listed as not evaluated, as for any unknown value.
    fn need_of(&self, field: FieldId, sense: Sense) -> Option<Need> {
        match self.domain(field) {
            Domain::Text(t) => return Some(Need::Text(t)),
            Domain::Quantity(q) => {
                return Some(Need::Within {
                    lo: q.lo(),
                    hi: q.hi(),
                });
            }
            Domain::Unset => {}
        }
        match sense {
            Sense::Stated => (!self.asked(field)).then_some(Need::Nothing),
            Sense::AtLeast => {
                let demand = self
                    .bounds_on(field, true)
                    .into_iter()
                    .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))));
                Some(demand.map_or(Need::Nothing, Need::AtLeast))
            }
            Sense::AtMost => {
                let allowed = self
                    .bounds_on(field, false)
                    .into_iter()
                    .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x))));
                Some(allowed.map_or(Need::Nothing, Need::AtMost))
            }
        }
    }

    /// The bounds relations put on a field by naming it alone on one side: for `above`, the
    /// highest value of the other side of each `field >= x` (the field must be at least that);
    /// otherwise the lowest value of the other side of each `field <= x`.
    fn bounds_on(&self, field: FieldId, above: bool) -> Vec<f64> {
        let mut out = Vec::new();
        for r in self.model.relations() {
            let (larger, smaller) = match r.op {
                CmpOp::Ge => (&r.lhs, &r.rhs),
                CmpOp::Le => (&r.rhs, &r.lhs),
                _ => continue,
            };
            let (mine, other) = if above {
                (larger, smaller)
            } else {
                (smaller, larger)
            };
            if matches!(mine, RExpr::Field(f) if *f == field)
                && let Ok(Val::Q(q)) = eval(&self.model, other)
            {
                out.push(if above { q.hi() } else { q.lo() });
            }
        }
        out
    }
}
