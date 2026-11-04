//! The whole pipeline, from source text to a KiCad schematic.

pub mod cli;
pub mod compare;
pub mod constraints;
pub mod dxf;
pub mod editor;
pub mod fab;
pub mod init;
pub mod layout;
pub mod libraries;
pub mod order;
pub mod physical;
pub mod project;
pub mod refresh;
pub mod resolve;
pub mod supply;

use std::path::{Path, PathBuf};

use netsmith_check::{Checked, Domain, InstId, InstKind, Loader, Model, SourceId, Sources, check};
use netsmith_core::{Design, PanelSpec};
use netsmith_kicad::{Mapping, Pinout, emit_schematic, project_file};
use netsmith_lang::{ParseError, Span};
use netsmith_lower::guidelines::Guideline;
use netsmith_lower::{Assembly, Lowered, Sourced, lower, lower_panel, lower_system_board};
use netsmith_pcb::{Subject, validate_pinout};
use netsmith_solve::{Catalogs, Solution, solve, solve_system};

pub use libraries::Libraries;

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
    /// with each nearest alternative in its place (ADR 0047). Empty if every generic part
    /// has a row.
    pub rechecks: Vec<resolve::Recheck>,
    /// The text of the `.kicad_sch`.
    pub schematic: String,
    /// The text of the `.kicad_pro`.
    pub project: String,
    /// What was decided, for a person to read.
    pub report: String,
    /// Things worth knowing that did not stop the build.
    pub notes: Vec<String>,
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

/// One mistake that stops a build: the message, where it is if it is in the source, and
/// the text the command line prints for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub message: String,
    pub at: Option<Location>,
    pub rendered: String,
}

impl Problem {
    /// A mistake of a board or system as a whole, with no line of its own.
    fn general(message: String, rendered: String) -> Self {
        Self {
            message,
            at: None,
            rendered,
        }
    }

    fn of(d: &netsmith_check::Diagnostic, sources: &Sources) -> Self {
        Self {
            message: d.message.clone(),
            at: Some(locate(sources, d.source, d.span)),
            rendered: d.render(sources),
        }
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
        .map(|d| Problem::of(d, &checked.sources))
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
    let checked = check(name, text, loader);
    if !checked.is_ok() {
        return Err(problems_of(&checked));
    }
    if checked.boards.is_empty() && checked.systems.is_empty() && checked.panels.is_empty() {
        let message = format!("{name} has no `board` to build");
        return Err(vec![Problem::general(
            message.clone(),
            format!("{message}\n"),
        )]);
    }

    let mut built = Vec::new();
    let mut solved = Vec::new();
    for board in checked.standalone_boards() {
        let solution = solve(board, catalogs);
        if !solution.diagnostics.is_empty() {
            return Err(diagnostic_problems(&solution.diagnostics, &checked.sources));
        }
        let lowered =
            lower(board, &solution).map_err(|errors| lowering_problems(&board.name, &errors))?;
        if catalogs.strict_guidelines() {
            refuse_guidelines(
                &board.name,
                &lowered.guidelines,
                &solution.model,
                &checked.sources,
            )?;
        }
        let notes = notes(&solution, None);
        let mut one = export(
            None,
            &board.name,
            lowered,
            notes,
            &solution.model,
            &checked.sources,
            libraries,
        )?;
        one.rechecks = resolve::recheck_alternatives(
            &solution,
            &one.design.parts,
            catalogs,
            &|pinned: &Catalogs| solve(board, pinned),
        );
        built.push(one);
    }
    for system in &checked.systems {
        if system.boards.is_empty() {
            let message = format!("system {} has no boards", system.name);
            return Err(vec![Problem::general(
                message.clone(),
                format!("error: {message}\n"),
            )]);
        }
        let solution = solve_system(system, catalogs);
        if !solution.diagnostics.is_empty() {
            return Err(diagnostic_problems(&solution.diagnostics, &checked.sources));
        }
        for member in &system.boards {
            let label = format!("{}/{}", system.name, member.name);
            let lowered = lower_system_board(system, member, &solution)
                .map_err(|errors| lowering_problems(&label, &errors))?;
            let scoped = solution.model.scoped_to(member.inst);
            if catalogs.strict_guidelines() {
                refuse_guidelines(&label, &lowered.guidelines, &scoped, &checked.sources)?;
            }
            let notes = notes(&solution, Some(member.inst));
            let mut one = export(
                Some(&system.name),
                &member.name,
                lowered,
                notes,
                &scoped,
                &checked.sources,
                libraries,
            )?;
            one.rechecks = resolve::recheck_alternatives(
                &solution,
                &one.design.parts,
                catalogs,
                &|pinned: &Catalogs| solve_system(system, pinned),
            );
            built.push(one);
        }
        solved.push(SolvedSystem {
            name: system.name.clone(),
            model: solution.model.clone(),
            sources: checked.sources.clone(),
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
            .map_err(|errors| lowering_problems(&format!("panel {}", panel.name), &errors))?;
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
            match g.at {
                Some(inst) => {
                    let (source, span) = model.instance(inst).at();
                    Problem::of(
                        &netsmith_check::Diagnostic {
                            source,
                            span,
                            message,
                        },
                        sources,
                    )
                }
                None => Problem::general(message.clone(), format!("error: {message}\n")),
            }
        })
        .collect())
}

fn diagnostic_problems(
    diagnostics: &[netsmith_check::Diagnostic],
    sources: &Sources,
) -> Vec<Problem> {
    diagnostics
        .iter()
        .map(|d| Problem::of(d, sources))
        .collect()
}

fn lowering_problems(board: &str, errors: &[String]) -> Vec<Problem> {
    errors
        .iter()
        .map(|e| Problem::general(format!("{board}: {e}"), format!("error: {board}: {e}\n")))
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
        .collect()
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
    let label = match system {
        Some(system) => format!("{system}/{board}"),
        None => board.to_string(),
    };
    let mut design = lowered.design;
    constraints::name_sources(&mut design, &lowered.constraint_sites, sources);
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
                    vec![Problem::general(
                        format!("{label}: {}: {e}", s.refdes),
                        format!("error: {label}: {}: {e}\n", s.refdes),
                    )]
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
                        Problem::general(format!("{label}: {m}"), format!("error: {label}: {m}\n"))
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
                    vec![Problem::general(
                        format!("{label}: {e}"),
                        format!("error: {label}: {e}\n"),
                    )]
                })?;
            (schematic, project_file(&design.name))
        }
    };
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
        rechecks: Vec::new(),
        schematic,
        notes,
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
