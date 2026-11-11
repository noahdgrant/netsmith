//! The choice of a provider for each use of a generic (ADR 0068).
//!
//! The structure of a model depends on the provider (its relations, pins, parts), and the
//! checker builds the model before the solver runs, so a provider is not a row of a catalog.
//! It is chosen one level above `solve`: a trial is a whole elaboration (`check_one` with an
//! assignment) and a whole solve, and the first trial in which nothing fails is the pick.
//! `solve` itself, and ADR 0057's joint search inside it, are not touched.
//!
//! The order is fixed and the pick is a function of the sources alone: depth first over the
//! uses in path order, the last use varying fastest, each use's providers in the order the
//! checker lists them (`netsmith_check::Provider`). Nothing compares providers: the first
//! assignment that works is the one.
//!
//! The search. `outcome_with` checks once with the default assignment (the first provider of
//! every use), solves, and if the solution has no diagnostics that is the result, at the cost of
//! one solve, as before. Only a failure with a use that has more than one provider starts the
//! search. The trials are bounded by [`MAX_PROVIDER_TRIALS`] (64), and on reaching it the build
//! prints the diagnostics of the first solve and then `gave up after trying 64 provider
//! combinations`, naming the first six uses it did not reach and counting the rest (the
//! diagnostics first, because a design with many uses and an unrelated mistake used to end with
//! only the bound's message). Each use that has several providers is varied alone first, over its
//! other providers; a use none of whose providers changes the diagnostics (rendered text) is left
//! at its first provider for the depth-first search, and if that finds nothing and every
//! combination fits in the bound the left-out uses are searched too. So an unrelated mistake
//! costs the sum of the providers of the uses and not the product. A trial that passes while a
//! use is varied alone is kept (`Search::passes`) and taken when the search in order reaches the
//! same assignment, not solved again.
//!
//! A provider is the outer choice and the rows of its catalog the inner one, searched by ADR
//! 0057's joint search inside the trial; a provider whose catalog cannot serve the design fails
//! its trial with the catalog's own message. Rejected: a provider as a candidate row in
//! `Solver::pick_parts` (a row only supplies values for fields that already exist).
//!
//! Cost. A trial is a whole solve: for an invented board of 58 parts, release build, `check` 2
//! ms, `solve` 215 to 250 ms, so the bound is about 14 s on such a board (an example,
//! `crates/netsmith/examples/trial_cost.rs`). Not changed: every use with a second provider is
//! still varied alone, one solve each, up to the bound; varying only the uses a diagnostic
//! names would give up designs whose cause the diagnostic does not name (a rail's voltage a
//! provider sets). For a use whose first provider is the generic itself, an open value after the
//! pick is the design's (a catalog row sets every value its catalog lists), so it is not
//! "unjudged": otherwise thirty resistors with no stated current made a build take 36.9 s
//! instead of 0.5 s.
//!
//! What cannot be judged does not hold. A trial of a use whose pick is not the generic itself
//! is unjudged when a bound or equation that waits reads an unset value that lies inside the use
//! (the datasheet number of the IC inside a module provider), or a logic level or absolute
//! maximum could not be checked because a `Logic` or the part inside the use has no value for it
//! (`NotEvaluated::involves`). A provider that can be judged is preferred, and when none can
//! the first in order stands and the bound is named in the notes. What stays the design's is a
//! value inside the use that is one and the same value as one outside it (`Model::equalities`),
//! such as the `.peak` of the board's rail that the board leaves open: every provider leaves
//! it as it is, so it starts no search. The silent case of `levels.rs` (a pair of pins of which
//! neither states any level) is not counted.

use std::collections::{BTreeMap, HashMap, HashSet};

use netsmith_check::{
    Assignment, Checked, Diagnostic, FieldId, GenericUse, InstId, Loader, Model, Sources, check_one,
};
use netsmith_solve::{Catalogs, Solution, solve, solve_system};

use crate::{Problem, Stage};

/// The most provider combinations one board or system is tried with. A trial is a whole
/// solve, so the bound is on time, as `SearchBudget::trials` of the part search is.
pub const MAX_PROVIDER_TRIALS: usize = 64;

/// The most uses the message about the bound names; the others are counted.
const MAX_NAMED_USES: usize = 6;

/// What was decided for one use of a generic: which provider stands in its place, and the
/// providers tried before it that were passed over, each with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderNote {
    /// The path of the use from the board: `rf`, or `antenna.rf` inside a module.
    pub path: String,
    /// The generic's name (`CoaxReceptacle`).
    pub generic: String,
    /// The provider's name (`AcmeMicroCoax`).
    pub chosen: String,
    /// The providers that were tried and failed, in the order they were tried, with the first
    /// diagnostic of the closest trial that used each.
    pub passed_over: Vec<(String, String)>,
    /// How many providers are in scope.
    pub candidates: usize,
}

impl ProviderNote {
    /// The line a build prints, in the style of ADR 0057: what was picked and what was passed
    /// over, and why.
    pub fn line(&self) -> String {
        let mut line = format!(
            "{} is a {}: picked {}",
            self.path, self.generic, self.chosen
        );
        if !self.passed_over.is_empty() {
            let passed: Vec<String> = self
                .passed_over
                .iter()
                .map(|(name, why)| format!("{name} ({why})"))
                .collect();
            line.push_str(&format!("; passed over {}", passed.join(", ")));
        }
        line
    }
}

/// A board or a system of the file, whose uses are to be chosen.
#[derive(Clone, Copy)]
pub(crate) enum Root {
    Board,
    System,
}

/// A trial that passed: the elaboration and its solution.
pub(crate) struct Found {
    pub checked: Checked,
    pub solution: Solution,
    pub passed_over: BTreeMap<String, Vec<(String, String)>>,
}

pub(crate) enum Outcome {
    /// Nothing can be varied: no use has more than one provider in scope, or varying one at
    /// a time never changes what fails. The first solve's diagnostics stand.
    Unchanged,
    Found(Box<Found>),
    Failed(Vec<Problem>),
}

/// The index of the provider taken for each use that has more than one, with its path, in path
/// order: what a trial is known by.
type Choices = Vec<(String, usize)>;

/// A trial that did not fail: the elaboration and its solution.
type Pass = Box<(Checked, Solution)>;

/// The source a search elaborates again for each assignment.
#[derive(Clone, Copy)]
pub(crate) struct Source<'a> {
    pub name: &'a str,
    pub text: &'a str,
    pub loader: &'a Loader,
    pub catalogs: &'a Catalogs,
}

/// What one use's choices came to: the failures, in the order they were tried.
struct Record {
    /// For each use that has more than one provider, its path and the index of the provider
    /// of this trial, in path order.
    choices: Choices,
    diagnostics: Vec<Diagnostic>,
}

struct Search<'a> {
    source: Source<'a>,
    root: &'a str,
    kind: Root,
    trials: usize,
    gave_up: bool,
    /// The failures, in the order the trials ran.
    records: Vec<Record>,
    /// A trial is not run twice: its failure is kept under its choices.
    seen: HashMap<Choices, usize>,
    /// Uses that varying alone did not change anything for.
    irrelevant: HashSet<String>,
    /// Trials that did not fail but could not judge a bound on a use (`unjudged`), by their
    /// choices: the first of them is the pick when no trial is judged in full.
    weak: BTreeMap<Choices, Pass>,
    /// Trials that passed in full and have not been taken yet, by their choices. A pass found
    /// while a use is varied alone is kept here, so that the search in order, which may reach
    /// the same assignment, does not solve it again (nor count it twice against the bound).
    passes: HashMap<Choices, Pass>,
}

enum Leaf {
    /// The trial passed in full; its elaboration and solution wait in `Search::passes` under
    /// these choices.
    Pass(Choices),
    Fail(usize),
}

/// The checks that the solver could not make in a solution where a use has more than one
/// provider, because of what the provider that was put in its place states, or leaves open.
/// Such a design does not fail, and the build names each check as not evaluated, but nothing
/// has shown the provider meets it; a provider that can be judged is preferred to one that
/// cannot (ADR 0068 sections 11 and 13). What counts, for a use whose pick is not the generic
/// itself:
///
/// - a bound or an equation, among the relations that waited, that reads a value inside the
///   use that is still unset: a value of the use itself (`rf.max_frequency`), or of an
///   interface or a part inside it that nothing outside the use shares (the datasheet number
///   of the controller of a module that wraps a part, `reg.controller.max_vin`);
/// - a logic level or an absolute maximum that could not be checked because a `Logic`, or
///   the part, inside the use has no value for it (`NotEvaluated::involves`).
///
/// A bound that waits on a value that the board shares with the use (the board's rail that
/// `reg.vin` is joined to, whose `.peak` the board leaves open) is the design's and counts
/// for none. One diagnostic for each check.
pub(crate) fn unjudged(solution: &Solution) -> Vec<Diagnostic> {
    let model = &solution.model;
    let uses: Vec<InstId> = model
        .uses
        .iter()
        .filter(|u| u.candidates.len() > 1)
        // A generic that is its own provider (the standard passives, ADR 0068 stage 2) has its
        // catalog row set every value its catalog lists, and what is open after the pick is
        // what the design supplies (a current, the voltage across a part): another provider
        // would leave the same open, and trying each of them for every such part would cost a
        // whole solve a time for nothing. A provider that is written by hand has no row to
        // fill what it leaves out, so for it an open value is its own.
        .filter(|u| u.candidates.get(u.chosen).is_none_or(|c| !c.own))
        .map(|u| u.inst)
        .collect();
    if uses.is_empty() {
        return Vec::new();
    }
    let inside = |inst: InstId| uses.iter().any(|&u| model.is_inside(inst, u));
    // A value inside a use that nothing states is the provider's to state, unless it is one
    // and the same value as one outside the use, which the use's user stated or left open:
    // the rail of the board that a use's `.vin` is joined to, with its `.peak`. That is the
    // design's, and every provider leaves it as it is.
    let open_in_use = |f: FieldId| -> bool {
        let field = model.field(f);
        if field.domain != netsmith_check::Domain::Unset {
            return false;
        }
        let Some(u) = uses
            .iter()
            .copied()
            .find(|&u| model.is_inside(field.owner, u))
        else {
            return false;
        };
        if field.owner == u {
            return true;
        }
        let mut reached = vec![f];
        let mut next = 0;
        while next < reached.len() {
            let here = reached[next];
            next += 1;
            for &(a, b) in model.equalities() {
                let other = if a == here {
                    b
                } else if b == here {
                    a
                } else {
                    continue;
                };
                if reached.contains(&other) {
                    continue;
                }
                if !model.is_inside(model.field(other).owner, u) {
                    return false;
                }
                reached.push(other);
            }
        }
        true
    };
    let mut out: Vec<Diagnostic> = solution
        .deferred
        .iter()
        .filter_map(|&i| {
            let r = &model.relations()[i];
            let mut fields = Vec::new();
            r.lhs.fields(&mut fields);
            r.rhs.fields(&mut fields);
            fields.iter().any(|&f| open_in_use(f)).then(|| Diagnostic {
                source: r.source,
                span: r.span,
                message: format!(
                    "not evaluated: {} {} {}",
                    model.show_expr(&r.lhs),
                    r.op.symbol(),
                    model.show_expr(&r.rhs)
                ),
            })
        })
        .collect();
    out.extend(
        solution
            .not_evaluated
            .iter()
            .filter(|n| n.involves.iter().any(|&i| inside(i)))
            .map(|n| {
                let (source, span) = model.instance(n.at).at();
                Diagnostic {
                    source,
                    span,
                    message: n.message.clone(),
                }
            }),
    );
    out
}

impl Search<'_> {
    fn elaborate(&self, assign: &BTreeMap<String, String>) -> Checked {
        let mut assignment = Assignment::new();
        assignment.insert(self.root.to_string(), assign.clone());
        check_one(
            self.source.name,
            self.source.text,
            self.source.loader,
            &assignment,
            self.root,
        )
    }

    fn model<'c>(&self, checked: &'c Checked) -> Option<&'c Model> {
        match self.kind {
            Root::Board => checked.board(self.root).map(|b| &b.model),
            Root::System => checked.system(self.root).map(|s| &s.model),
        }
    }

    fn solve(&self, checked: &Checked) -> Option<Solution> {
        match self.kind {
            Root::Board => checked
                .board(self.root)
                .map(|b| solve(b, self.source.catalogs)),
            Root::System => checked
                .system(self.root)
                .map(|s| solve_system(s, self.source.catalogs)),
        }
    }

    /// The uses of the elaboration with more than one provider, in path order.
    fn multi(&self, checked: &Checked) -> Vec<GenericUse> {
        let mut uses: Vec<GenericUse> = self
            .model(checked)
            .map(|m| {
                m.uses
                    .iter()
                    .filter(|u| u.candidates.len() > 1)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        uses.sort_by(|a, b| a.path.cmp(&b.path));
        uses
    }

    /// Run one trial to its end: the check, and the solve if the check found nothing. The
    /// choices that are not in `assign` take the first provider.
    fn leaf(&mut self, assign: &BTreeMap<String, String>) -> Option<Leaf> {
        let checked = self.elaborate(assign);
        let choices: Choices = self
            .multi(&checked)
            .iter()
            .map(|u| (u.path.clone(), u.chosen))
            .collect();
        if let Some(&at) = self.seen.get(&choices) {
            return Some(Leaf::Fail(at));
        }
        if self.passes.contains_key(&choices) {
            return Some(Leaf::Pass(choices));
        }
        if self.trials >= MAX_PROVIDER_TRIALS {
            self.gave_up = true;
            return None;
        }
        self.trials += 1;
        count_trial();
        let diagnostics = if checked.is_ok() {
            let solution = self.solve(&checked)?;
            if solution.diagnostics.is_empty() {
                let weak = unjudged(&solution);
                if weak.is_empty() {
                    self.passes
                        .insert(choices.clone(), Box::new((checked, solution)));
                    return Some(Leaf::Pass(choices));
                }
                self.weak
                    .insert(choices.clone(), Box::new((checked, solution)));
                weak
            } else {
                solution.diagnostics
            }
        } else {
            checked.diagnostics.clone()
        };
        self.records.push(Record {
            choices: choices.clone(),
            diagnostics,
        });
        let at = self.records.len() - 1;
        self.seen.insert(choices, at);
        Some(Leaf::Fail(at))
    }

    /// Depth first over the uses that are still open, in path order, last fastest.
    fn visit(&mut self, assign: &mut BTreeMap<String, String>) -> Option<Pass> {
        let checked = self.elaborate(assign);
        let open = self
            .multi(&checked)
            .into_iter()
            .find(|u| !assign.contains_key(&u.path) && !self.irrelevant.contains(&u.path));
        let Some(open) = open else {
            return match self.leaf(assign)? {
                Leaf::Pass(choices) => self.passes.remove(&choices),
                Leaf::Fail(_) => None,
            };
        };
        for candidate in &open.candidates {
            assign.insert(open.path.clone(), candidate.id.clone());
            let found = self.visit(assign);
            assign.remove(&open.path);
            if found.is_some() {
                return found;
            }
            if self.gave_up {
                return None;
            }
        }
        None
    }
}

/// Look for an assignment of providers under which `first`, the solve of the first provider of
/// every use, does not fail. `Unchanged` when there is nothing to choose.
pub(crate) fn search(
    source: Source<'_>,
    root: &str,
    kind: Root,
    checked: &Checked,
    first: &Solution,
) -> Outcome {
    let mut s = Search {
        source,
        root,
        kind,
        trials: 0,
        gave_up: false,
        records: Vec::new(),
        seen: HashMap::new(),
        irrelevant: HashSet::new(),
        weak: BTreeMap::new(),
        passes: HashMap::new(),
    };
    let uses = s.multi(checked);
    if uses.is_empty() {
        return Outcome::Unchanged;
    }
    // The default's failure is the first record: every use at its first provider.
    let default_choices: Choices = uses.iter().map(|u| (u.path.clone(), 0)).collect();
    // The first solve may have passed without being able to judge a bound on a use.
    let first_diagnostics = if first.diagnostics.is_empty() {
        unjudged(first)
    } else {
        first.diagnostics.clone()
    };
    s.records.push(Record {
        choices: default_choices.clone(),
        diagnostics: first_diagnostics.clone(),
    });
    s.seen.insert(default_choices, 0);
    s.trials = 1;
    let shown =
        |ds: &[Diagnostic]| -> Vec<String> { ds.iter().map(|d| d.message.clone()).collect() };

    // A use is varied alone first. If none of its other providers changes what fails, the
    // failure is not the use's, and it is left at its first provider: a design with an
    // unrelated mistake is not solved once per combination of providers.
    for u in &uses {
        let mut changed = false;
        for candidate in u.candidates.iter().skip(1) {
            let mut assign = BTreeMap::new();
            assign.insert(u.path.clone(), candidate.id.clone());
            match s.leaf(&assign) {
                Some(Leaf::Pass(_)) => changed = true,
                Some(Leaf::Fail(at)) => {
                    if shown(&s.records[at].diagnostics) != shown(&first_diagnostics) {
                        changed = true;
                    }
                }
                None => {
                    changed = true;
                    break;
                }
            }
        }
        if !changed {
            s.irrelevant.insert(u.path.clone());
        }
    }
    if s.irrelevant.len() == uses.len() {
        return Outcome::Unchanged;
    }

    let mut found = s.visit(&mut BTreeMap::new());
    // The uses left at their first provider are let go only to keep the search short: when
    // the others have no answer and every combination still fits the bound, try them too.
    if found.is_none() && !s.gave_up && !s.irrelevant.is_empty() {
        let product: usize = uses
            .iter()
            .map(|u| u.candidates.len())
            .try_fold(1usize, |a, n| a.checked_mul(n))
            .unwrap_or(usize::MAX);
        if product.saturating_sub(s.trials)
            <= MAX_PROVIDER_TRIALS - s.trials.min(MAX_PROVIDER_TRIALS)
        {
            s.irrelevant.clear();
            found = s.visit(&mut BTreeMap::new());
        }
    }
    // Nothing was judged in full: the first solve stands if it did not fail, else the first
    // trial in the order that did not fail.
    if found.is_none() && first.diagnostics.is_empty() {
        return Outcome::Unchanged;
    }
    if found.is_none()
        && let Some((_, weak)) = s.weak.pop_first()
    {
        found = Some(weak);
    }
    if let Some(found) = found {
        let (checked_found, solution) = *found;
        let passed_over = passed_over(&s, &checked_found);
        return Outcome::Found(Box::new(Found {
            checked: checked_found,
            solution,
            passed_over,
        }));
    }
    Outcome::Failed(failure(&s, checked, &uses, &first_diagnostics))
}

/// For each use, the providers tried and failed before the pick, with the first diagnostic of
/// the closest trial that had it.
fn passed_over(s: &Search<'_>, picked: &Checked) -> BTreeMap<String, Vec<(String, String)>> {
    let mut out = BTreeMap::new();
    let Some(model) = s.model(picked) else {
        return out;
    };
    for u in model.uses.iter().filter(|u| u.candidates.len() > 1) {
        let mut tried = Vec::new();
        for (index, candidate) in u.candidates.iter().enumerate().take(u.chosen) {
            let best = s
                .records
                .iter()
                .filter(|r| r.choices.iter().any(|(p, c)| *p == u.path && *c == index))
                .min_by_key(|r| r.diagnostics.len());
            if let Some(why) = best.and_then(|r| r.diagnostics.first()) {
                tried.push((candidate.name.clone(), why.message.clone()));
            }
        }
        out.insert(u.path.clone(), tried);
    }
    out
}

/// No assignment works. When the search ran to its end, one error at the first use that
/// matters, naming the closest provider and what each of the others fails on. When it stopped
/// at the bound, the combinations it did not reach are not an answer, so the build says what
/// is wrong with the design as it stands, the diagnostics of the first solve, and then that
/// the search gave up (the design's own mistake is what a person has to fix first, and it was
/// hidden by the bound).
fn failure(
    s: &Search<'_>,
    checked: &Checked,
    uses: &[GenericUse],
    first: &[Diagnostic],
) -> Vec<Problem> {
    let sources: &Sources = &checked.sources;
    let model = s.model(checked).expect("the root was elaborated");
    let Some(u) = uses.iter().find(|u| !s.irrelevant.contains(&u.path)) else {
        return Vec::new();
    };
    let at = model.instance(u.inst).at();
    let describe = |index: usize| -> Option<(usize, String)> {
        s.records
            .iter()
            .filter(|r| r.choices.iter().any(|(p, c)| *p == u.path && *c == index))
            .min_by_key(|r| r.diagnostics.len())
            .and_then(|r| {
                r.diagnostics
                    .first()
                    .map(|d| (r.diagnostics.len(), d.message.clone()))
            })
    };
    let mut closest_provider = None;
    let message = if s.gave_up {
        let mut paths: Vec<String> = uses
            .iter()
            .filter(|u| !s.irrelevant.contains(&u.path))
            .map(|u| format!("`{}`", u.path))
            .collect();
        if paths.len() > MAX_NAMED_USES {
            let more = paths.len() - MAX_NAMED_USES;
            paths.truncate(MAX_NAMED_USES);
            paths.push(format!("{more} more"));
        }
        format!(
            "gave up after trying {MAX_PROVIDER_TRIALS} provider combinations for {}; give them fewer providers in scope, or name a provider as the type of one of them",
            paths.join(" and ")
        )
    } else {
        let tried: Vec<(usize, String, usize)> = u
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(i, c)| describe(i).map(|(n, _)| (i, c.name.clone(), n)))
            .collect();
        let closest = tried
            .iter()
            .min_by_key(|(i, _, n)| (*n, *i))
            .expect("at least the first provider was tried");
        let (closest_index, closest_name, _) = closest;
        closest_provider = Some(*closest_index);
        let because = describe(*closest_index).map(|(_, m)| m).unwrap_or_default();
        let mut text = format!(
            "no {} provider satisfies {}: the closest of the {} in scope, {}, fails because {}",
            u.generic,
            u.path,
            u.candidates.len(),
            closest_name,
            because
        );
        let others: Vec<String> = u
            .candidates
            .iter()
            .enumerate()
            .filter(|(i, _)| i != closest_index)
            .map(|(i, c)| match describe(i) {
                Some((_, why)) => format!("{} ({why})", c.name),
                None => format!("{} (not tried)", c.name),
            })
            .collect();
        if !others.is_empty() {
            text.push_str(&format!("; the others: {}", others.join(", ")));
        }
        text
    };
    let diagnostic = Diagnostic {
        source: at.0,
        span: at.1,
        message,
    };
    // The board a message is about: the root itself, or for a system the board whose
    // definition holds it, as `outcome_observed` names it.
    let board_of = |d: &Diagnostic| -> String {
        match s.kind {
            Root::Board => s.root.to_string(),
            Root::System => checked
                .system(s.root)
                .and_then(|system| {
                    system
                        .boards
                        .iter()
                        .find(|m| crate::inside_definition(model, m.inst, d.source, d.span))
                        .map(|m| format!("{}/{}", system.name, m.name))
                })
                .unwrap_or_else(|| s.root.to_string()),
        }
    };
    let mut problems = if s.gave_up {
        crate::diagnostic_problems(first, sources, &board_of)
    } else {
        Vec::new()
    };
    let mut problem = Problem::of(Stage::Solve, &diagnostic, sources);
    problem.board = Some(board_of(&diagnostic));
    problems.push(problem);
    if let Some(closest_index) = closest_provider {
        // The message names the first diagnostic of the closest trial. A diagnostic that every
        // trial of the search has, whichever provider stood, is the design's own (a regulator
        // the catalog cannot serve, a window a rail is outside of): the search must not hide
        // it, or a board with two providers in scope for a use would report one mistake of an
        // unrelated part and drop the rest.
        let closest = s
            .records
            .iter()
            .filter(|r| {
                r.choices
                    .iter()
                    .any(|(p, c)| *p == u.path && *c == closest_index)
            })
            .min_by_key(|r| r.diagnostics.len());
        let named = closest
            .and_then(|r| r.diagnostics.first())
            .map(|d| &d.message);
        let others: Vec<Diagnostic> = closest
            .map(|r| {
                r.diagnostics
                    .iter()
                    .filter(|d| Some(&d.message) != named)
                    .filter(|d| {
                        s.records
                            .iter()
                            .all(|r| r.diagnostics.iter().any(|e| e.message == d.message))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        problems.extend(crate::diagnostic_problems(&others, sources, &board_of));
    }
    problems
}

/// What was picked for each use of a built board, with what the search passed over. For one
/// board of a system, only its own uses, with paths from the board.
pub(crate) fn notes_for(
    model: &Model,
    board: Option<InstId>,
    passed: &BTreeMap<String, Vec<(String, String)>>,
) -> Vec<ProviderNote> {
    let mut notes: Vec<ProviderNote> = model
        .uses
        .iter()
        .filter(|u| board.is_none_or(|b| model.is_inside(u.inst, b)))
        .filter_map(|u| {
            let chosen = u.candidates.get(u.chosen)?;
            // A generic that is its own provider (the standard passives) says nothing: its
            // pick is the part, and nothing stood before it.
            if chosen.own {
                return None;
            }
            Some(ProviderNote {
                path: board.map_or_else(|| u.path.clone(), |b| model.path_below(u.inst, b)),
                generic: u.generic.clone(),
                chosen: chosen.name.clone(),
                passed_over: passed.get(&u.path).cloned().unwrap_or_default(),
                candidates: u.candidates.len(),
            })
        })
        .collect();
    notes.sort_by(|a, b| a.path.cmp(&b.path));
    notes
}

thread_local! {
    static SOLVES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many trials (a check and a solve of one assignment of providers) this thread has run
/// since it started, not counting the first solve of a build. A test reads it to show how much
/// work a failing design cost.
pub fn trials_run() -> usize {
    SOLVES.with(std::cell::Cell::get)
}

fn count_trial() {
    SOLVES.with(|c| c.set(c.get() + 1));
}
