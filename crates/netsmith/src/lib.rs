//! The whole pipeline, from source text to a KiCad schematic.

mod bus_roles;
pub mod cli;
pub mod compare;
pub mod constraints;
pub mod dxf;
pub mod editor;
pub mod fab;
pub mod init;
pub mod layout;
pub mod libraries;
pub mod no_supplier;
pub mod order;
pub mod physical;
pub mod project;
pub mod providers;
pub mod refresh;
pub mod resolve;
pub mod supply;
mod values;

use std::path::{Path, PathBuf};

use netsmith_check::{
    Checked, Domain, FieldId, FieldKind, InstId, InstKind, Loader, Model, SourceId, Sources, check,
};
use netsmith_core::{Design, PanelSpec};
use netsmith_kicad::{Mapping, Pinout, emit_schematic, project_file};
use netsmith_lang::{ParseError, Span};
use netsmith_lower::guidelines::Guideline;
use netsmith_lower::{Assembly, Lowered, Sourced, lower, lower_panel, lower_system_board};
use netsmith_pcb::{Subject, validate_pinout};
use netsmith_solve::{Catalogs, Solution, solve, solve_system};

pub use libraries::Libraries;
pub use providers::{MAX_PROVIDER_TRIALS, ProviderNote};

/// One board, built.
pub struct Built {
    /// The system this board is part of, if it was placed in one.
    pub system: Option<String>,
    /// The board's name: its definition's, or for a board in a system its member name in
    /// the system (`main`).
    pub board: String,
    pub design: Design,
    /// Where each part of the design comes from, in the design's order: its LCSC number,
    /// manufacturer part number and JLCPCB library, and whether the board house places it.
    pub sourcing: Vec<Sourced>,
    /// For each generic part that no catalog row stands for, the board's checks run again
    /// with each nearest alternative in its place (ADR 0047). Nothing is solved when the
    /// board is built: `rechecks.get(catalogs)` makes the solves, for the one who shows the result.
    /// Empty if every generic part has a row.
    pub rechecks: resolve::Rechecks,
    /// The text of the `.kicad_sch`.
    pub schematic: String,
    /// The text of the `.kicad_pro`.
    pub project: String,
    /// What was decided, for a person to read.
    pub report: String,
    /// Things worth knowing that did not stop the build: what the solver decided and left
    /// alone, then the notes of lowering (the last `lowering_notes` of them).
    pub notes: Vec<String>,
    /// For each use of a generic on this board: the provider that stands in its place and the
    /// providers passed over (ADR 0068). Empty for a board that uses no generic.
    pub providers: Vec<ProviderNote>,
    /// How many of the last `notes` come from lowering and not from the solver.
    pub lowering_notes: usize,
    /// What the design does against a guideline (ADR 0036): a warning that did not stop
    /// the build. With `guidelines = "error"` or `--strict` the build stops instead and
    /// this is empty.
    pub guidelines: Vec<Guideline>,
    /// What the build knew about the source, for a tool that points back into it.
    pub trace: Trace,
}

/// What a board was built from: the solved model, the sources its spans point into, and
/// the instance each part of the design was made from.
#[derive(Debug, Clone)]
pub struct Trace {
    /// The solved model. For a board of a system it is scoped to the board.
    pub model: Model,
    pub sources: Sources,
    /// The instance of each part of `design`, in the design's order.
    pub origins: Vec<InstId>,
}

/// A system, solved: its whole model, with each board a member.
#[derive(Debug, Clone)]
pub struct SolvedSystem {
    pub name: String,
    pub model: Model,
    pub sources: Sources,
    /// The members: `(name in the system, board definition, instance)`.
    pub boards: Vec<(String, String, InstId)>,
}

/// Everything one source file builds.
pub struct Outcome {
    pub boards: Vec<Built>,
    pub systems: Vec<SolvedSystem>,
    /// The panels (ADR 0040). Each repeats a stand-alone board of `boards`.
    pub panels: Vec<PanelSpec>,
}

/// Where a mistake is: 1-based lines and columns (in characters) of its first and last
/// character, in the file as the diagnostics name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

/// The stage of the pipeline that found a mistake (ADR 0019, "A diagnostic is a value").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The text does not parse.
    Syntax,
    /// The checker: names, types, connections, pin tables; a board file with nothing to build.
    Check,
    /// The solver.
    Solve,
    /// Lowering to parts and nets, and the checks of the schematic and footprints against it.
    Lower,
    /// A guideline that the project makes an error (ADR 0036).
    Guideline,
}

/// One mistake that stops a build: the message, where it is if it is in the source, and
/// the text the command line prints for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub message: String,
    pub at: Option<Location>,
    pub rendered: String,
    /// The stage that found it.
    pub stage: Stage,
    /// The board it is about, as [`Built::label`] has it, if it is about one.
    pub board: Option<String>,
    /// The instance it is about, from the board, if it is about one.
    pub path: Option<String>,
}

impl Problem {
    /// A mistake of a board or system as a whole, with no line of its own.
    fn general(stage: Stage, message: String, rendered: String) -> Self {
        Self {
            message,
            at: None,
            rendered,
            stage,
            board: None,
            path: None,
        }
    }

    fn of(stage: Stage, d: &netsmith_check::Diagnostic, sources: &Sources) -> Self {
        Self {
            message: d.message.clone(),
            at: Some(locate(sources, d.source, d.span)),
            rendered: d.render(sources),
            stage,
            board: None,
            path: None,
        }
    }

    /// Say which board it is about.
    fn on_board(mut self, label: &str) -> Self {
        self.board = Some(label.to_string());
        self
    }
}

/// Whether `span` of `source` is inside the definition that made `inst`.
fn inside_definition(model: &Model, inst: InstId, source: SourceId, span: Span) -> bool {
    let inst = model.instance(inst);
    inst.source == source && inst.span.start <= span.start && span.end <= inst.span.end
}

/// The board a checker mistake is about, when it is written inside the definition of one
/// (not in a module or a system, which several boards share), as the summary names it. A board
/// that a system places is named as the system's member.
fn board_of_diagnostic(checked: &Checked, d: &netsmith_check::Diagnostic) -> Option<String> {
    let board = checked
        .boards
        .iter()
        .find(|b| inside_definition(&b.model, b.model.root, d.source, d.span))?;
    if checked.standalone_boards().any(|b| b.name == board.name) {
        Some(board.name.clone())
    } else {
        let mut placed = checked.systems.iter().flat_map(|s| {
            s.boards
                .iter()
                .filter(|m| m.type_name == board.name)
                .map(move |m| format!("{}/{}", s.name, m.name))
        });
        let first = placed.next()?;
        // A board a system places twice is two boards: the mistake is in both, in neither.
        placed.next().is_none().then_some(first)
    }
}

/// Where `span` of source `source` is, in lines and columns.
pub fn locate(sources: &Sources, source: SourceId, span: Span) -> Location {
    let source = sources.get(source);
    let position =
        |offset: usize| ParseError::new(Span::new(offset, offset), "").line_col(&source.text);
    let (line, column) = position(span.start);
    let (end_line, end_column) = position(span.end.max(span.start));
    Location {
        file: source.name.clone(),
        line,
        column,
        end_line,
        end_column,
    }
}

/// The problems of a checked file, one for each diagnostic.
pub fn problems_of(checked: &Checked) -> Vec<Problem> {
    checked
        .diagnostics
        .iter()
        .map(|d| {
            let stage = if checked.syntax.contains(d) {
                Stage::Syntax
            } else {
                Stage::Check
            };
            let mut problem = Problem::of(stage, d, &checked.sources);
            if stage == Stage::Check {
                problem.board = board_of_diagnostic(checked, d);
            }
            problem
        })
        .collect()
}

/// What the command line prints for a list of problems.
pub fn render_problems(problems: &[Problem]) -> String {
    problems.iter().map(|p| p.rendered.as_str()).collect()
}

impl Built {
    /// `LedExample`, or `Demo/main` for the board `main` of the system `Demo`.
    pub fn label(&self) -> String {
        match &self.system {
            Some(system) => format!("{system}/{}", self.board),
            None => self.board.clone(),
        }
    }

    /// The guideline findings as the command line prints them, one line each.
    pub fn guideline_warnings(&self) -> Vec<String> {
        let label = self.label();
        self.guidelines
            .iter()
            .map(|g| format!("{label}: {} guideline: {}", g.rule, g.message))
            .collect()
    }

    /// Where this board's files go under `out`: a directory of its own, inside the system's
    /// directory for a board of a system.
    pub fn dir(&self, out: &Path) -> PathBuf {
        match &self.system {
            Some(system) => out.join(system).join(&self.board),
            None => out.join(&self.board),
        }
    }

    /// Write the KiCad schematic and project file into this board's directory under `out`,
    /// and return the directory.
    pub fn write(&self, out: &Path) -> std::io::Result<PathBuf> {
        let dir = self.dir(out);
        std::fs::create_dir_all(&dir)?;
        let name = &self.design.name;
        std::fs::write(dir.join(format!("{name}.kicad_sch")), &self.schematic)?;
        std::fs::write(dir.join(format!("{name}.kicad_pro")), &self.project)?;
        Ok(dir)
    }
}

/// Check, solve, lower and export every board in one source file, and every board of every
/// system in it, reading the installed KiCad libraries. A board that a system places is
/// built as that system's, not again alone. On failure the error is every problem,
/// rendered with its source line.
pub fn build(name: &str, text: &str, catalogs: &Catalogs) -> Result<Vec<Built>, String> {
    build_with(name, text, &Loader::std(), catalogs, &Libraries::standard())
}

/// [`build`], with imports resolved by `loader`: the standard library and, for a project,
/// its `src/` modules; and with the KiCad libraries given.
pub fn build_with(
    name: &str,
    text: &str,
    loader: &Loader,
    catalogs: &Catalogs,
    libraries: &Libraries,
) -> Result<Vec<Built>, String> {
    outcome_with(name, text, loader, catalogs, Some(libraries))
        .map(|o| o.boards)
        .map_err(|problems| render_problems(&problems))
}

/// [`build_with`], keeping each problem as a value and the solved systems as well as the
/// boards: what an editor needs to show the source and the design side by side.
///
/// With `libraries` of `None` no KiCad library is read, as `check` and `solve` read none:
/// each board's `design`, `sourcing`, `notes` and `trace` are complete, and its
/// `schematic`, `project` and `report` are empty. That is what the editor and `compare`
/// need, and they work on a machine with no KiCad.
pub fn outcome_with(
    name: &str,
    text: &str,
    loader: &Loader,
    catalogs: &Catalogs,
    libraries: Option<&Libraries>,
) -> Result<Outcome, Vec<Problem>> {
    outcome_observed(name, text, loader, catalogs, libraries, &|_| {})
}

/// What [`outcome_observed`] is about to do, for a caller that shows how far it has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step<'a> {
    /// `Solve` or `Lower`.
    pub stage: Stage,
    /// The board or system it is about to work on (a board of a system is lowered under the
    /// system's name).
    pub board: &'a str,
    /// How many of the file's boards and systems are finished, and how many there are.
    pub done: usize,
    pub total: usize,
}

/// [`outcome_with`], telling `observe` before it solves and before it lowers each board and
/// system of the file.
pub fn outcome_observed(
    name: &str,
    text: &str,
    loader: &Loader,
    catalogs: &Catalogs,
    libraries: Option<&Libraries>,
    observe: &dyn Fn(Step<'_>),
) -> Result<Outcome, Vec<Problem>> {
    let checked = check(name, text, loader);
    if !checked.is_ok() {
        return Err(problems_of(&checked));
    }
    if checked.boards.is_empty() && checked.systems.is_empty() && checked.panels.is_empty() {
        let message = format!("{name} has no `board` to build");
        return Err(vec![Problem::general(
            Stage::Check,
            message.clone(),
            format!("{message}\n"),
        )]);
    }

    let mut built = Vec::new();
    let mut solved = Vec::new();
    let total = checked.standalone_boards().count() + checked.systems.len();
    for (done, board) in checked.standalone_boards().enumerate() {
        let step = |stage| Step {
            stage,
            board: &board.name,
            done,
            total,
        };
        observe(step(Stage::Solve));
        let first = solve(board, catalogs);
        // A failure with a use that has several providers is tried with the others, and so is
        // a bound on such a use that could not be judged (ADR 0068).
        let mut picked = None;
        if !first.diagnostics.is_empty() || !providers::unjudged(&first).is_empty() {
            match providers::search(
                providers::Source {
                    name,
                    text,
                    loader,
                    catalogs,
                },
                &board.name,
                providers::Root::Board,
                &checked,
                &first,
            ) {
                providers::Outcome::Unchanged if !first.diagnostics.is_empty() => {
                    return Err(diagnostic_problems(
                        &first.diagnostics,
                        &checked.sources,
                        &|_| board.name.clone(),
                    ));
                }
                providers::Outcome::Unchanged => {}
                providers::Outcome::Failed(problems) => return Err(problems),
                providers::Outcome::Found(found) => picked = Some(*found),
            }
        }
        let (solution, passed) = match picked.as_ref() {
            Some(p) => (&p.solution, p.passed_over.clone()),
            None => (&first, Default::default()),
        };
        let board = picked
            .as_ref()
            .and_then(|p| p.checked.board(&board.name))
            .unwrap_or(board);
        let sources = picked
            .as_ref()
            .map_or(&checked.sources, |p| &p.checked.sources);
        observe(step(Stage::Lower));
        let mut lowered = lower(board, solution)
            .map_err(|errors| lowering_problems(Some(&board.name), &board.name, &errors))?;
        lowered
            .guidelines
            .extend(open_circle_findings(solution, sources, None));
        lowered
            .guidelines
            .extend(values::unconstrained_pick_findings(solution, sources, None));
        lowered
            .guidelines
            .extend(values::unstated_value_findings(solution, sources, None));
        lowered
            .guidelines
            .extend(bus_roles::findings(solution, None));
        if catalogs.strict_guidelines() {
            refuse_guidelines(&board.name, &lowered.guidelines, &solution.model, sources)?;
        }
        let provided = providers::notes_for(&solution.model, None, &passed);
        let mut notes = provider_lines(&provided);
        notes.extend(self::notes(solution, None));
        let mut one = export(
            None,
            &board.name,
            lowered,
            notes,
            &solution.model,
            sources,
            libraries,
        )?;
        one.providers = provided;
        one.rechecks = resolve::Rechecks::plan(solution, &one.design.parts, catalogs, || {
            let board = board.clone();
            Box::new(move |pinned: &Catalogs| quietly(|| solve(&board, pinned)))
        });
        built.push(one);
    }
    let standalone = total - checked.systems.len();
    for (n, system) in checked.systems.iter().enumerate() {
        let step = |stage| Step {
            stage,
            board: &system.name,
            done: standalone + n,
            total,
        };
        if system.boards.is_empty() {
            let message = format!("system {} has no boards", system.name);
            return Err(vec![Problem::general(
                Stage::Check,
                message.clone(),
                format!("error: {message}\n"),
            )]);
        }
        observe(step(Stage::Solve));
        let first = solve_system(system, catalogs);
        let mut picked = None;
        if !first.diagnostics.is_empty() || !providers::unjudged(&first).is_empty() {
            match providers::search(
                providers::Source {
                    name,
                    text,
                    loader,
                    catalogs,
                },
                &system.name,
                providers::Root::System,
                &checked,
                &first,
            ) {
                providers::Outcome::Unchanged if !first.diagnostics.is_empty() => {
                    // A message written inside the definition of a member's board is about that
                    // board; any other is about the system.
                    return Err(diagnostic_problems(
                        &first.diagnostics,
                        &checked.sources,
                        &|d| {
                            let member = system.boards.iter().find(|m| {
                                inside_definition(&first.model, m.inst, d.source, d.span)
                            });
                            match member {
                                Some(m) => format!("{}/{}", system.name, m.name),
                                None => system.name.clone(),
                            }
                        },
                    ));
                }
                providers::Outcome::Unchanged => {}
                providers::Outcome::Failed(problems) => return Err(problems),
                providers::Outcome::Found(found) => picked = Some(*found),
            }
        }
        let (solution, passed) = match picked.as_ref() {
            Some(p) => (&p.solution, p.passed_over.clone()),
            None => (&first, Default::default()),
        };
        let system = picked
            .as_ref()
            .and_then(|p| p.checked.system(&system.name))
            .unwrap_or(system);
        let sources = picked
            .as_ref()
            .map_or(&checked.sources, |p| &p.checked.sources);
        observe(step(Stage::Lower));
        for member in &system.boards {
            let label = format!("{}/{}", system.name, member.name);
            let mut lowered = lower_system_board(system, member, solution)
                .map_err(|errors| lowering_problems(Some(&label), &label, &errors))?;
            lowered
                .guidelines
                .extend(open_circle_findings(solution, sources, Some(member.inst)));
            lowered
                .guidelines
                .extend(values::unconstrained_pick_findings(
                    solution,
                    sources,
                    Some(member.inst),
                ));
            lowered.guidelines.extend(values::unstated_value_findings(
                solution,
                sources,
                Some(member.inst),
            ));
            lowered
                .guidelines
                .extend(bus_roles::findings(solution, Some(member.inst)));
            let scoped = solution.model.scoped_to(member.inst);
            if catalogs.strict_guidelines() {
                refuse_guidelines(&label, &lowered.guidelines, &scoped, sources)?;
            }
            let provided = providers::notes_for(&solution.model, Some(member.inst), &passed);
            let mut notes = provider_lines(&provided);
            notes.extend(self::notes(solution, Some(member.inst)));
            let mut one = export(
                Some(&system.name),
                &member.name,
                lowered,
                notes,
                &scoped,
                sources,
                libraries,
            )?;
            one.providers = provided;
            one.rechecks = resolve::Rechecks::plan(solution, &one.design.parts, catalogs, || {
                let system = system.clone();
                Box::new(move |pinned: &Catalogs| quietly(|| solve_system(&system, pinned)))
            });
            built.push(one);
        }
        solved.push(SolvedSystem {
            name: system.name.clone(),
            model: solution.model.clone(),
            sources: sources.clone(),
            boards: system
                .boards
                .iter()
                .map(|m| (m.name.clone(), m.type_name.clone(), m.inst))
                .collect(),
        });
    }
    let mut panels = Vec::new();
    for panel in &checked.panels {
        let spec = lower_panel(panel)
            .map_err(|errors| lowering_problems(None, &format!("panel {}", panel.name), &errors))?;
        if !built
            .iter()
            .any(|b| b.system.is_none() && b.board == spec.cell)
        {
            let standalone: Vec<&str> = built
                .iter()
                .filter(|b| b.system.is_none())
                .map(|b| b.board.as_str())
                .collect();
            let message = format!(
                "panel {}: `{}` is not a stand-alone board of this file, and a panel repeats one (the stand-alone boards here: {}); a board that a system places is made as the system's, and a board is defined in the file of its panel",
                spec.name,
                spec.cell,
                if standalone.is_empty() {
                    "none".to_string()
                } else {
                    standalone.join(", ")
                }
            );
            return Err(vec![Problem::general(
                Stage::Check,
                message.clone(),
                format!("error: {message}\n"),
            )]);
        }
        panels.push(spec);
    }
    Ok(Outcome {
        boards: built,
        systems: solved,
        panels,
    })
}

/// Runs a solve that is a check of an alternative part, not the board's own: it counts nothing
/// of what it chooses, so that the count of the board's solve is not told again from zero
/// (`Rechecks::get` says how many alternatives it has solved).
fn quietly<T>(work: impl FnOnce() -> T) -> T {
    netsmith_core::job::reporting(std::rc::Rc::new(|_| {}), work)
}

/// With guidelines made errors (ADR 0036), a finding stops the build: one problem for each,
/// at the rail or part that carries the current.
fn refuse_guidelines(
    label: &str,
    findings: &[Guideline],
    model: &Model,
    sources: &Sources,
) -> Result<(), Vec<Problem>> {
    if findings.is_empty() {
        return Ok(());
    }
    Err(findings
        .iter()
        .map(|g| {
            let message = format!("{label}: {} guideline: {}", g.rule, g.message);
            let mut problem = match g.at {
                Some(inst) => {
                    let (source, span) = model.instance(inst).at();
                    let mut problem = Problem::of(
                        Stage::Guideline,
                        &netsmith_check::Diagnostic {
                            source,
                            span,
                            message,
                        },
                        sources,
                    );
                    problem.path = Some(model.path_of(inst)).filter(|p| !p.is_empty());
                    problem
                }
                None => Problem::general(
                    Stage::Guideline,
                    message.clone(),
                    format!("error: {message}\n"),
                ),
            };
            problem.board = Some(label.to_string());
            problem
        })
        .collect())
}

/// The solver's mistakes. `board_of` says which board a mistake is about.
fn diagnostic_problems(
    diagnostics: &[netsmith_check::Diagnostic],
    sources: &Sources,
    board_of: &dyn Fn(&netsmith_check::Diagnostic) -> String,
) -> Vec<Problem> {
    diagnostics
        .iter()
        .map(|d| {
            let mut problem = Problem::of(Stage::Solve, d, sources);
            problem.board = Some(board_of(d));
            problem
        })
        .collect()
}

/// Mistakes of lowering `subject`: a board (`board` names it), or a panel.
fn lowering_problems(board: Option<&str>, subject: &str, errors: &[String]) -> Vec<Problem> {
    errors
        .iter()
        .map(|e| {
            let mut problem = Problem::general(
                Stage::Lower,
                format!("{subject}: {e}"),
                format!("error: {subject}: {e}\n"),
            );
            problem.board = board.map(str::to_string);
            problem
        })
        .collect()
}

/// What was not decided: parts not chosen and relations not evaluated. For one board of a
/// system, only those inside it.
fn notes(solution: &Solution, board: Option<InstId>) -> Vec<String> {
    let whole = &solution.model;
    // Paths are shown from the board, as they are when it is built alone.
    let model = &board.map_or_else(|| whole.clone(), |b| whole.scoped_to(b));
    let inside = |inst: InstId| board.is_none_or(|b| whole.is_inside(inst, b));
    solution
        .unresolved
        .iter()
        .filter(|u| inside(u.part))
        .map(|u| format!("{}: not chosen: {}", model.path_of(u.part), u.reason))
        .chain(whole.balances().iter().filter_map(|b| {
            // An incomplete balance (ADR 0027) is worth a line only where it kept the
            // current of a part from being set.
            let unset: Vec<String> = b
                .draws
                .iter()
                .filter(|d| {
                    let owner = whole.field(d.field).owner;
                    whole.instance(owner).kind == InstKind::Part
                        && !matches!(whole.field(d.field).domain, Domain::Quantity(_))
                })
                .map(|d| format!("`{}`", model.path_of_field(d.field)))
                .collect();
            if b.is_complete() || b.returns || unset.is_empty() || !inside(b.scope) {
                return None;
            }
            let pins: Vec<String> = b
                .unmodelled
                .iter()
                .map(|&p| format!("`{}`", model.path_of(p)))
                .collect();
            let scope = match model.path_of(b.scope) {
                path if path.is_empty() => String::new(),
                path => format!(" in `{path}`"),
            };
            Some(format!(
                "the balance on net `{}`{scope} is incomplete: {} {} not modelled, so {} {} not set from it",
                b.net,
                pins.join(", "),
                if pins.len() == 1 { "is" } else { "are" },
                unset.join(", "),
                if unset.len() == 1 { "is" } else { "are" },
            ))
        }))
        .chain(
            solution
                .not_evaluated
                .iter()
                .filter(|n| inside(n.at))
                .map(|n| n.message.clone()),
        )
        .chain(solution.deferred.iter().filter_map(|&i| {
            let r = &whole.relations()[i];
            let mut fields = Vec::new();
            r.lhs.fields(&mut fields);
            r.rhs.fields(&mut fields);
            fields
                .iter()
                .any(|&f| inside(whole.field(f).owner))
                .then(|| {
                    format!(
                        "not evaluated: {} {} {}",
                        model.show_expr(&r.lhs),
                        r.op.symbol(),
                        model.show_expr(&r.rhs)
                    )
                })
        }))
        .chain(
            solution
                .circular
                .iter()
                .filter(|group| group.iter().any(|&f| inside(whole.field(f).owner)))
                .map(|group| circular_note(model, group)),
        )
        .chain(
            solution
                .seeded
                .iter()
                .filter(|seed| inside(whole.field(seed.field).owner))
                .map(|seed| started_note(model, seed)),
        )
        .collect()
}

/// A line of the notes for each use of a generic that a provider was picked for. A generic
/// that is its own provider (the standard passives, one day) says nothing: its pick is the
/// part.
fn provider_lines(notes: &[ProviderNote]) -> Vec<String> {
    notes
        .iter()
        .filter(|n| n.chosen != n.generic)
        .map(ProviderNote::line)
        .collect()
}

/// The checks that a circle of values left unmade (see `Solution::open_circles`, ADR 0057),
/// as findings of the kind a project may make blocking (`guidelines = "error"`, `--strict`):
/// the build passes with a warning by default, because the checks are the only thing that is
/// missing and the design may be right, and stops with `--strict`. For one board of a system,
/// only those inside it.
fn open_circle_findings(
    solution: &Solution,
    sources: &Sources,
    board: Option<InstId>,
) -> Vec<Guideline> {
    let whole = &solution.model;
    let model = &board.map_or_else(|| whole.clone(), |b| whole.scoped_to(b));
    let inside = |inst: InstId| board.is_none_or(|b| whole.is_inside(inst, b));
    solution
        .open_circles
        .iter()
        .filter(|c| c.fields.iter().any(|&f| inside(whole.field(f).owner)))
        .map(|c| {
            let quote = |fields: &[FieldId]| -> Vec<String> {
                let mut paths: Vec<String> = fields
                    .iter()
                    .map(|&f| format!("`{}`", model.path_of_field(f)))
                    .collect();
                paths.sort();
                paths
            };
            let mut shown = quote(&c.fields);
            let more = shown.len().saturating_sub(CIRCULAR_SHOWN);
            shown.truncate(CIRCULAR_SHOWN);
            let more = match more {
                0 => String::new(),
                n => format!(" and {n} more"),
            };
            // The design's own checks come before the library's (a part's rating): the line
            // shows only the first few.
            let mut order = c.checks.clone();
            order.sort_by_key(|&i| sources.get(whole.relations()[i].source).name.starts_with("std/"));
            let checks: Vec<String> = order
                .iter()
                .map(|&i| {
                    let r = &whole.relations()[i];
                    format!(
                        "`{} {} {}`",
                        model.show_expr(&r.lhs),
                        r.op.symbol(),
                        model.show_expr(&r.rhs)
                    )
                })
                .collect();
            let listed = checks.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
            let rest = match checks.len().saturating_sub(3) {
                0 => String::new(),
                n => format!(" and {n} more"),
            };
            let start = start_of(whole, &c.fields);
            let waits = |c: &netsmith_solve::OpenCircle| -> Option<(String, InstId)> {
                if c.closed {
                    return None;
                }
                let waiting = quote(&c.waiting);
                Some((
                    format!(
                        "a range on them alone would not start them, because they also read {}, which nothing states: state {} first",
                        waiting.join(", "),
                        if waiting.len() == 1 { "it" } else { "them" }
                    ),
                    c.waiting
                        .first()
                        .map_or(whole.field(start).owner, |&f| whole.field(f).owner),
                ))
            };
            let (what_to_state, at) = if let Some(first) = c.nets.first() {
                // A group that closes through the currents of a net (issue #18).
                let nets: Vec<String> = c
                    .nets
                    .iter()
                    .map(|n| {
                        format!(
                            "net `{}` carries {} currents ({}), none of which has a value",
                            n.net,
                            n.unknown.len(),
                            quote(&n.unknown).join(", ")
                        )
                    })
                    .collect();
                let current = first.unsolved.first().copied().unwrap_or(c.fields[0]);
                let voltage = (matches!(
                    whole.field(start).kind,
                    FieldKind::Quantity(d) if d == netsmith_quantity::Dimension::VOLTAGE
                ))
                .then(|| format!(", or on a voltage in the circle, for example `{}`", model.path_of_field(start)));
                // A rail or port of the net that nothing is said to draw from: the shortest path.
                let outside = first
                    .unknown
                    .iter()
                    .copied()
                    .filter(|&f| {
                        whole.instance(whole.field(f).owner).kind != InstKind::Part
                    })
                    .min_by_key(|&f| {
                        let path = model.path_of_field(f);
                        (path.matches('.').count(), path)
                    });
                let nothing_else = outside
                    .map(|f| {
                        format!(
                            "; if nothing else draws from the net, `{} &= 0A` makes it a circle of equations, which a voltage starts",
                            model.path_of_field(f)
                        )
                    })
                    .unwrap_or_default();
                let mut text = format!(
                    "{}, and the solver works a part's current out only from a net of two currents (ADR 0026), so what the checks wait on is never set: state a range on `{}` (the part's current: the most and the least that the net's loads draw through it){}{nothing_else}; the checks are then made against what you state, and nothing compares it with the net's other currents",
                    nets.join("; "),
                    model.path_of_field(current),
                    voltage.unwrap_or_default()
                );
                let mut at = whole.field(current).owner;
                if let Some((more, owner)) = waits(c) {
                    text = format!("{text}; and {more}");
                    at = owner;
                }
                (text, at)
            } else if let Some(waiting) = waits(c) {
                waiting
            } else {
                (
                    format!(
                        "state a range on one of them with `&=`, for example `{}`, that holds every value it can take: the checks are then made against it, and a range that is too small makes them fail",
                        model.path_of_field(start)
                    ),
                    whole.field(start).owner,
                )
            };
            Guideline {
                rule: "open circle",
                message: format!(
                    "these values depend on each other and none has a value: {}{more}; so the checks {listed}{rest} are not evaluated, and the parts they would judge are chosen without them; {what_to_state}",
                    shown.join(", ")
                ),
                at: Some(at),
            }
        })
        .collect()
}

/// The field of a circle to suggest a range for: one that an equation sets from a value of
/// the same kind that has a range already (the load's voltage from the supply's, the first cut
/// of ADR 0053), else the first voltage, else the first field.
fn start_of(model: &Model, fields: &[FieldId]) -> FieldId {
    let kind_of = |f: FieldId| match model.field(f).kind {
        FieldKind::Quantity(d) => Some(d),
        _ => None,
    };
    let from_a_range = fields.iter().copied().find(|&f| {
        model.relations().iter().any(|r| {
            let (netsmith_lang::ast::CmpOp::EqEq, netsmith_check::RExpr::Field(out)) =
                (&r.op, &r.lhs)
            else {
                return false;
            };
            let mut reads = Vec::new();
            r.rhs.fields(&mut reads);
            *out == f
                && reads.iter().any(|&g| {
                    g != f
                        && kind_of(g) == kind_of(f)
                        && matches!(model.field(g).domain, Domain::Quantity(_))
                })
        })
    });
    from_a_range
        .or_else(|| {
            fields
                .iter()
                .copied()
                .find(|&f| kind_of(f) == Some(netsmith_quantity::Dimension::VOLTAGE))
        })
        .unwrap_or(fields[0])
}

/// A circle of values that the solver started itself (see `Solution::seeded`, ADR 0053).
fn started_note(model: &Model, seed: &netsmith_solve::Seeded) -> String {
    let from: Vec<String> = seed
        .from
        .iter()
        .map(|&f| format!("`{}`", model.path_of_field(f)))
        .collect();
    format!(
        "`{}` depends on itself through its own equations and nothing stated a range for it, so the solver started it from {} and worked out {}, the range its equations map into itself; no other value inside {}, the range its part works in, satisfies them (state a range on it with `&=` to replace this)",
        model.path_of_field(seed.field),
        from.join(", "),
        seed.range,
        seed.within
    )
}

/// The fields a group names before the note says how many more there are.
const CIRCULAR_SHOWN: usize = 10;

/// Values that wait on each other (see `Solution::circular`).
fn circular_note(model: &Model, group: &[FieldId]) -> String {
    let mut paths: Vec<String> = group
        .iter()
        .map(|&f| format!("`{}`", model.path_of_field(f)))
        .collect();
    paths.sort();
    let more = paths.len().saturating_sub(CIRCULAR_SHOWN);
    paths.truncate(CIRCULAR_SHOWN);
    let more = match more {
        0 => String::new(),
        n => format!(" and {n} more"),
    };
    format!(
        "these values depend on each other and none has a value, so the checks that use them are not evaluated: {}{more}; state a range on one of them with `&=` to start the solver, or state the range the part works in (`>=` and `<=` on the voltage) so that the solver can start it",
        paths.join(", ")
    )
}

/// The schematic and report of a lowered board.
fn export(
    system: Option<&str>,
    board: &str,
    lowered: Lowered,
    mut notes: Vec<String>,
    model: &Model,
    sources: &Sources,
    libraries: Option<&Libraries>,
) -> Result<Built, Vec<Problem>> {
    // The nets whose tracks were not sized from their current (ADR 0048).
    notes.extend(lowered.notes.iter().cloned());
    let lowering_notes = lowered.notes.len();
    let label = match system {
        Some(system) => format!("{system}/{board}"),
        None => board.to_string(),
    };
    let mut design = lowered.design;
    constraints::name_sources(&mut design, &lowered.constraint_sites, sources);
    constraints::name_offset_sources(&mut design, &lowered.offset_sites, sources);
    constraints::name_protrusion_sources(&mut design, &lowered.protrusion_sites, sources);
    let mut sourcing = lowered.sourcing;
    let (schematic, project) = match libraries {
        None => (String::new(), String::new()),
        Some(libraries) => {
            // A through-hole footprint is soldered by hand, whatever kind of part it is,
            // unless the design or the catalog says who assembles it (ADR 0024); the
            // footprint file says which it is. The footprint and the symbol must also
            // agree with the part's pin table (ADR 0016, decision 8).
            let mut disagreements = Vec::new();
            for (s, part) in sourcing.iter_mut().zip(&design.parts) {
                let Ok(mapped) = libraries.mapping.map(&s.kind, s.package.as_deref()) else {
                    // The schematic names the part the mapping lacks.
                    continue;
                };
                let footprint = libraries.footprints.load(&mapped.footprint).map_err(|e| {
                    vec![
                        Problem::general(
                            Stage::Lower,
                            format!("{label}: {}: {e}", s.refdes),
                            format!("error: {label}: {}: {e}\n", s.refdes),
                        )
                        .on_board(&label),
                    ]
                })?;
                if footprint.mount == netsmith_pcb::Mount::ThroughHole && !s.assembly_stated {
                    s.assembly = Assembly::Hand;
                }
                // A symbol that cannot be read is named by the schematic below.
                if let (Some(pinout), Ok(symbol)) =
                    (Pinout::of(part), libraries.symbols.symbol(&mapped.lib_id))
                {
                    let subject = Subject {
                        refdes: &s.refdes,
                        kind: &s.kind,
                        package: s.package.as_deref(),
                    };
                    disagreements.extend(validate_pinout(
                        &subject, &pinout, &mapped, &symbol, &footprint,
                    ));
                }
            }
            if !disagreements.is_empty() {
                return Err(disagreements
                    .into_iter()
                    .map(|m| {
                        Problem::general(
                            Stage::Lower,
                            format!("{label}: {m}"),
                            format!("error: {label}: {m}\n"),
                        )
                        .on_board(&label)
                    })
                    .collect());
            }
            // Silkscreen text is not in the circuit: it has no symbol.
            let drawn = Design {
                parts: design
                    .parts
                    .iter()
                    .filter(|p| p.kind != "Label")
                    .cloned()
                    .collect(),
                ..design.clone()
            };
            let schematic = emit_schematic(&drawn, &libraries.mapping, &libraries.symbols)
                .map_err(|e| {
                    vec![
                        Problem::general(
                            Stage::Lower,
                            format!("{label}: {e}"),
                            format!("error: {label}: {e}\n"),
                        )
                        .on_board(&label),
                    ]
                })?;
            (schematic, project_file(&design.name))
        }
    };
    // The parts the board house does not fit, for the rule that a part over protruding pins is
    // fitted by hand after the pins are cut (ADR 0045 section 15).
    design.features.hand_fitted = design
        .parts
        .iter()
        .zip(&sourcing)
        .filter(|(_, s)| s.assembly != Assembly::Fab)
        .map(|(p, _)| p.path.clone())
        .collect();
    let heading = match system {
        Some(_) => format!("{label} ({})", design.name),
        None => label,
    };
    Ok(Built {
        system: system.map(str::to_string),
        board: board.to_string(),
        report: match libraries {
            Some(libraries) => report(&heading, &design, &sourcing, &libraries.mapping),
            None => String::new(),
        },
        project,
        design,
        sourcing,
        rechecks: resolve::Rechecks::none(),
        schematic,
        notes,
        providers: Vec::new(),
        lowering_notes,
        guidelines: lowered.guidelines,
        trace: Trace {
            model: model.clone(),
            sources: sources.clone(),
            origins: lowered.origins,
        },
    })
}

fn report(heading: &str, design: &Design, sourcing: &[Sourced], mapping: &Mapping) -> String {
    let mut out = format!("board {heading}\n");
    for (p, source) in design.parts.iter().zip(sourcing) {
        let footprint = if p.kind == "Label" {
            "silkscreen text".to_string()
        } else {
            mapping
                .map(&p.kind, p.package.as_deref())
                .map(|m| m.footprint)
                .unwrap_or_else(|e| format!("({e})"))
        };
        let supplier = match (&source.lcsc, source.library) {
            _ if source.assembly == Assembly::Hand => "hand-soldered".to_string(),
            _ if source.assembly == Assembly::None => "not fitted".to_string(),
            (Some(lcsc), Some(library)) => format!("{lcsc} {library}"),
            (Some(lcsc), None) => lcsc.clone(),
            (None, _) if source.generic.is_some() => {
                "generic, resolved to a part when the bill of materials or fab is made".to_string()
            }
            (None, _) => "no LCSC number".to_string(),
        };
        out.push_str(&format!(
            "  {:<4} {:<15} {:<15} {footprint:<58} {supplier}\n",
            p.refdes, p.kind, p.value
        ));
    }
    for n in &design.nets {
        let pins: Vec<String> = n
            .pins
            .iter()
            .map(|p| format!("{}.{}", p.refdes, p.pin))
            .collect();
        out.push_str(&format!("  net {}: {}\n", n.name, pins.join(", ")));
    }
    out
}
