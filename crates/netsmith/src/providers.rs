//! The choice of a provider for each use of a generic (ADR 0059).
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

use std::collections::{BTreeMap, HashMap, HashSet};

use netsmith_check::{
    Assignment, Checked, Diagnostic, GenericUse, InstId, Loader, Model, Sources, check_one,
};
use netsmith_solve::{Catalogs, Solution, solve, solve_system};

use crate::Problem;

/// The most provider combinations one board or system is tried with. A trial is a whole
/// solve, so the bound is on time, as `MAX_TRIALS` of the part search is.
pub const MAX_PROVIDER_TRIALS: usize = 64;

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
}

enum Leaf {
    Pass(Pass),
    Fail(usize),
}

/// The bounds on a value of a use itself (`rf.max_frequency >= 1.61GHz`) that the solver could not
/// evaluate, in a solution where the use has more than one provider, because the provider that
/// was put in its place does not state the value (it is still unset). A bound that waits on
/// something else, a rail's `.peak` that nothing states, is the design's and not the provider's,
/// and is not this. Such a design does not fail, and the build names the
/// bound as not evaluated, but nothing has shown the provider meets it; a provider that can be
/// judged is preferred to one that cannot. One diagnostic for each bound.
pub(crate) fn unjudged(solution: &Solution) -> Vec<Diagnostic> {
    let model = &solution.model;
    let uses: Vec<InstId> = model
        .uses
        .iter()
        .filter(|u| u.candidates.len() > 1)
        // A generic that is its own provider (the standard passives, ADR 0059 stage 2) has its
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
    solution
        .deferred
        .iter()
        .filter_map(|&i| {
            let r = &model.relations()[i];
            let mut fields = Vec::new();
            r.lhs.fields(&mut fields);
            r.rhs.fields(&mut fields);
            // A value of the use that nothing states: the provider leaves it open.
            fields
                .iter()
                .any(|&f| {
                    uses.contains(&model.field(f).owner)
                        && model.field(f).domain == netsmith_check::Domain::Unset
                })
                .then(|| Diagnostic {
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
        .collect()
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
                    return Some(Leaf::Pass(Box::new((checked, solution))));
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
                Leaf::Pass(found) => Some(found),
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
    Outcome::Failed(failure(&s, checked, &uses))
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

/// No assignment works: one error at the first use that matters, naming the closest provider
/// and what each of the others fails on.
fn failure(s: &Search<'_>, checked: &Checked, uses: &[GenericUse]) -> Vec<Problem> {
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
    let message = if s.gave_up {
        let paths: Vec<String> = uses
            .iter()
            .filter(|u| !s.irrelevant.contains(&u.path))
            .map(|u| format!("`{}`", u.path))
            .collect();
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
    vec![Problem::of(&diagnostic, sources)]
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
