//! Building the project's files and keeping what came of it: the boards that built, the
//! systems, and every problem.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fs,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};

use netsmith_check::{InstKind, Loader, check};
use netsmith_core::job;
use netsmith_editor::{
    AnalysisEvent, ApiError, Observer, Work,
    api::{
        Analysis, BoardDetail, BoardSummary, Location, ModuleUse, NetInfo, PanelSummary, PartInfo,
        PinInfo, Problem, ProblemStage, Severity, SystemSummary,
    },
};

use super::{Workspace, hints};
use crate::{Built, Libraries, SolvedSystem, Stage, locate, outcome_observed};

/// A board that built, with its drawing.
pub struct BoardData {
    pub file: String,
    pub built: Built,
    pub svg: Result<String, String>,
    /// The KiCad libraries of the project, for what needs a footprint: the bill of
    /// materials and the PCB. A mistake in `kicad.toml` is the error.
    pub libraries: Arc<Result<Libraries, String>>,
    /// The layout file beside the source file (ADR 0032), and the names of all the boards the
    /// source file holds, so that a table for a board that is gone is found.
    pub layout: Option<(std::path::PathBuf, Vec<String>)>,
    /// Whether a requirement on where a part goes that is not met is a warning, as the
    /// manifest's `constraints = "warn"` makes it (ADR 0045), and not an error.
    pub constraints_warn: bool,
    /// The schematic layout file of the source file (ADR 0061) and what is not used of it for
    /// this board.
    pub hints: super::hints::Hints,
}

impl BoardData {
    pub fn libraries(&self) -> Result<&Libraries, ApiError> {
        (*self.libraries)
            .as_ref()
            .map_err(|e| ApiError::unprocessable(e.clone(), "libraries_unusable", &[]))
    }
}

pub struct SystemData {
    pub file: String,
    pub solved: SolvedSystem,
}

/// A panel that built (ADR 0040).
pub struct PanelData {
    pub file: String,
    pub spec: netsmith_core::PanelSpec,
}

/// One reading of the project. A file that no longer builds keeps the boards of its last
/// good reading, marked stale.
#[derive(Default)]
pub struct Snapshot {
    pub boards: Vec<(Arc<BoardData>, bool)>,
    pub systems: Vec<(Arc<SystemData>, bool)>,
    pub panels: Vec<(Arc<PanelData>, bool)>,
    pub problems: Vec<Problem>,
}

impl Snapshot {
    pub fn summary(&self, version: u64) -> Analysis {
        let count = |severity| {
            self.problems
                .iter()
                .filter(|p| p.severity == severity)
                .count() as u32
        };
        Analysis {
            version,
            errors: count(Severity::Error),
            warnings: count(Severity::Warning),
            notes: count(Severity::Note),
            problems: self.problems.clone(),
            boards: self
                .boards
                .iter()
                .map(|(b, stale)| BoardSummary {
                    name: b.built.label(),
                    definition: b.built.design.name.clone(),
                    file: b.file.clone(),
                    system: b.built.system.clone(),
                    stale: *stale,
                    parts: b.built.design.parts.len() as u32,
                    nets: b.built.design.nets.len() as u32,
                })
                .collect(),
            systems: self
                .systems
                .iter()
                .map(|(s, _)| SystemSummary {
                    name: s.solved.name.clone(),
                    file: s.file.clone(),
                    boards: s
                        .solved
                        .boards
                        .iter()
                        .map(|(member, _, _)| format!("{}/{member}", s.solved.name))
                        .collect(),
                })
                .collect(),
            panels: self
                .panels
                .iter()
                .map(|(p, _)| PanelSummary {
                    name: p.spec.name.clone(),
                    file: p.file.clone(),
                    board: p.spec.cell.clone(),
                    columns: p.spec.columns,
                    rows: p.spec.rows,
                })
                .collect(),
            module_uses: self.module_uses(),
            elapsed_ms: None,
            cancelled: None,
            stale: None,
        }
    }

    /// The module definitions the boards use and the boards that use each, by file and line.
    fn module_uses(&self) -> Option<Vec<ModuleUse>> {
        let mut uses: Vec<ModuleUse> = Vec::new();
        for (board, _) in &self.boards {
            let trace = &board.built.trace;
            let label = board.built.label();
            for (_, inst) in trace.model.instances() {
                if inst.kind != InstKind::Module {
                    continue;
                }
                let at = api_location(locate(&trace.sources, inst.source, inst.span));
                let found = uses
                    .iter_mut()
                    .find(|u| u.file == at.file && u.line == at.line && u.name == inst.type_name);
                match found {
                    Some(u) if !u.used_by.contains(&label) => u.used_by.push(label.clone()),
                    Some(_) => {}
                    None => uses.push(ModuleUse {
                        name: inst.type_name.clone(),
                        file: at.file,
                        line: at.line,
                        used_by: vec![label.clone()],
                    }),
                }
            }
        }
        uses.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        (!uses.is_empty()).then_some(uses)
    }

    pub fn panel(&self, name: &str) -> Result<Arc<PanelData>, ApiError> {
        self.panels
            .iter()
            .map(|(p, _)| p)
            .find(|p| p.spec.name == name)
            .cloned()
            .ok_or_else(|| ApiError::not_found(format!("there is no panel `{name}`")))
    }

    pub fn board(&self, name: &str) -> Result<&BoardData, ApiError> {
        self.boards
            .iter()
            .map(|(b, _)| &**b)
            .find(|b| b.built.label() == name)
            .ok_or_else(|| ApiError::not_found(format!("there is no board `{name}`")))
    }

    /// The board, as a handle that outlives the lock the snapshot is behind.
    pub fn board_handle(&self, name: &str) -> Result<Arc<BoardData>, ApiError> {
        self.boards
            .iter()
            .map(|(b, _)| b)
            .find(|b| b.built.label() == name)
            .cloned()
            .ok_or_else(|| ApiError::not_found(format!("there is no board `{name}`")))
    }

    pub fn system(&self, name: &str) -> Option<&SystemData> {
        self.systems
            .iter()
            .map(|(s, _)| &**s)
            .find(|s| s.solved.name == name)
    }
}

fn problem(severity: Severity, p: &crate::Problem) -> Problem {
    Problem {
        severity,
        message: p.message.clone(),
        rendered: p.rendered.clone(),
        at: p.at.as_ref().map(|l| Location {
            file: l.file.clone(),
            line: l.line as u32,
            column: l.column as u32,
            end_line: l.end_line as u32,
            end_column: l.end_column as u32,
        }),
        stage: Some(match p.stage {
            Stage::Syntax => ProblemStage::Syntax,
            Stage::Check => ProblemStage::Check,
            Stage::Solve => ProblemStage::Solve,
            Stage::Lower => ProblemStage::Lower,
            Stage::Guideline => ProblemStage::Guideline,
            Stage::Power => ProblemStage::Power,
        }),
        board: p.board.clone(),
        path: p.path.clone(),
    }
}

fn general(message: String) -> Problem {
    Problem {
        severity: Severity::Error,
        rendered: format!("error: {message}\n"),
        message,
        at: None,
        stage: Some(ProblemStage::Check),
        board: None,
        path: None,
    }
}

/// The message of a panic, as the panic hook would print it.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".to_string())
}

/// Runs `work` and gives its result, or the message of the panic it ended in. The editor serves
/// a project for as long as its owner works on it: a bug in the analysis (an `expect` in the
/// layout, say) must end that one analysis with a problem the page shows, not leave the server
/// waiting for a result that never comes.
///
/// A cancellation (ADR 0062) is not a panic: it unwinds on, to whoever started the job.
pub(super) fn contained<T>(work: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).map_err(|payload| {
        if job::is_cancellation(&*payload) {
            std::panic::resume_unwind(payload)
        }
        panic_message(&*payload)
    })
}

/// The problem a panic is shown as: an error with no place, saying what panicked and that it
/// is a bug to report. Its stage is `check` (the page's types have no stage for it).
fn internal_problem(what: &str, message: &str, board: Option<String>) -> Problem {
    let message = format!(
        "internal error: {what} panicked ({message}). This is a bug in netsmith, not in the project: please report it with the file that causes it"
    );
    Problem {
        severity: Severity::Error,
        rendered: format!("error: {message}\n"),
        message,
        at: None,
        stage: Some(ProblemStage::Check),
        board,
        path: None,
    }
}

impl Snapshot {
    /// What the project is when its first analysis was cancelled: no boards, and a note that
    /// says why and what to do (ADR 0062). The server is ready with it, as it is with any
    /// reading, and an analysis of the files replaces it.
    pub fn cancelled_first() -> Snapshot {
        let message = "The first analysis was cancelled: nothing is shown until the project is analysed. Run it again, or change a file.".to_string();
        Snapshot {
            problems: vec![Problem {
                severity: Severity::Note,
                rendered: format!("note: {message}\n"),
                message,
                at: None,
                stage: Some(ProblemStage::Check),
                board: None,
                path: None,
            }],
            ..Snapshot::default()
        }
    }

    /// What the project is after an analysis that panicked: the last good reading (its boards
    /// marked stale, as for a file that does not build), or nothing on the first, and the
    /// internal error first among the problems.
    pub fn after_panic(message: &str, previous: Option<&Snapshot>) -> Snapshot {
        let mut snapshot = Snapshot::default();
        if let Some(previous) = previous {
            snapshot.boards = previous
                .boards
                .iter()
                .map(|(b, _)| (Arc::clone(b), true))
                .collect();
            snapshot.systems = previous
                .systems
                .iter()
                .map(|(s, _)| (Arc::clone(s), true))
                .collect();
            snapshot.panels = previous
                .panels
                .iter()
                .map(|(p, _)| (Arc::clone(p), true))
                .collect();
            snapshot.problems = previous.problems.clone();
        }
        snapshot.problems.insert(
            0,
            internal_problem("the analysis of the project", message, None),
        );
        snapshot
    }
}

/// The least time between two progress lines about modules.
const MODULE_PROGRESS_EVERY: std::time::Duration = std::time::Duration::from_millis(100);
/// The least time between two progress lines about what the solver has counted.
const WORK_PROGRESS_EVERY: std::time::Duration = std::time::Duration::from_millis(100);

/// The solve or lowering step being worked on, which the solver's counts are told with.
struct Where {
    stage: &'static str,
    board: String,
    done: u32,
    total: u32,
}

/// What [`job::report`] is told while a file is built: the lines of progress that carry the
/// count, at most one in each [`WORK_PROGRESS_EVERY`] (the first and the last of a step always).
fn work_sink(live: Option<Observer>, step: &Rc<RefCell<Option<Where>>>) -> Rc<dyn Fn(&job::Work)> {
    let step = Rc::clone(step);
    let told: Cell<Option<std::time::Instant>> = Cell::new(None);
    Rc::new(move |w: &job::Work| {
        let Some(live) = &live else { return };
        let step = step.borrow();
        let Some(step) = &*step else { return };
        let finished = w.total.is_some_and(|t| w.done >= t);
        let first = w.done == 0;
        if !first
            && !finished
            && told
                .get()
                .is_some_and(|at| at.elapsed() < WORK_PROGRESS_EVERY)
        {
            return;
        }
        told.set(Some(std::time::Instant::now()));
        let work = Work {
            unit: w.unit.to_string(),
            done: w.done.min(u64::from(u32::MAX)) as u32,
            total: w.total.map(|t| t.min(u64::from(u32::MAX)) as u32),
        };
        let verb = match step.stage {
            "solve" => "solving",
            "draw" => "drawing",
            _ => "lowering",
        };
        let counted = match work.total {
            Some(total) => format!("{} of {total} {}", work.done, work.unit),
            None => format!("{} {}", work.done, work.unit),
        };
        live(AnalysisEvent::Progress {
            stage: step.stage.to_string(),
            board: step.board.clone(),
            done: step.done,
            total: step.total,
            message: format!(
                "{verb} {} ({} of {}): {counted}",
                step.board,
                step.done + 1,
                step.total
            ),
            work: Some(work),
        });
    })
}

impl Workspace {
    /// Build every top-level file, with `unsaved` read in place of the files it names, and
    /// check every module on its own so a mistake in one that nothing imports shows too.
    /// `previous` supplies the boards of a file that does not build now.
    pub(super) fn analyze(
        &self,
        unsaved: &HashMap<PathBuf, String>,
        previous: Option<&Snapshot>,
        observe: &dyn Fn(AnalysisEvent),
        live: Option<Observer>,
    ) -> Snapshot {
        let mut snapshot = Snapshot::default();
        // The manifest on disk cannot be read: nothing is built from it, as for `netsmith
        // build`, and its own message is the problem, until it is mended.
        if let Some(e) = self.manifest_error() {
            snapshot.problems.push(general(e));
            return snapshot;
        }
        let target = self.target();
        let loader = match target.loader_with(unsaved) {
            Ok(loader) => loader,
            Err(e) => {
                snapshot.problems.push(general(e));
                return snapshot;
            }
        };
        let catalogs = match target.catalogs() {
            Ok(catalogs) => catalogs,
            Err(e) => {
                snapshot.problems.push(general(e));
                return snapshot;
            }
        };
        let libraries = Arc::new(Libraries::for_project(
            target.project.as_ref().map(|p| p.root.as_path()),
        ));
        let files = target.files.len() as u32;
        for (index, file) in target.files.iter().enumerate() {
            observe(AnalysisEvent::Progress {
                stage: "check".to_string(),
                board: file.name.clone(),
                done: index as u32,
                total: files,
                message: format!("checking {} ({} of {files})", file.name, index + 1),
                work: None,
            });
            // What the solver and the drawing count as they go is told with the step they are in.
            let current: Rc<RefCell<Option<Where>>> = Rc::default();
            let sink = work_sink(live.clone(), &current);
            let text = match unsaved.get(&file.path) {
                Some(text) => Ok(text.clone()),
                None => fs::read_to_string(&file.path),
            };
            let outcome = match text {
                Err(e) => Err(vec![crate::Problem {
                    message: format!("{}: {e}", file.name),
                    at: None,
                    rendered: format!("{}: {e}\n", file.name),
                    stage: Stage::Check,
                    board: None,
                    path: None,
                }]),
                Ok(text) => job::reporting(Rc::clone(&sink), || {
                    outcome_observed(
                        &file.name,
                        &text,
                        &loader,
                        &catalogs,
                        None,
                        &|step: crate::Step<'_>| {
                            let stage = if step.stage == Stage::Solve {
                                "solve"
                            } else {
                                "lower"
                            };
                            let verb = if step.stage == Stage::Solve {
                                "solving"
                            } else {
                                "lowering"
                            };
                            *current.borrow_mut() = Some(Where {
                                stage,
                                board: step.board.to_string(),
                                done: step.done as u32,
                                total: step.total as u32,
                            });
                            observe(AnalysisEvent::Progress {
                                stage: stage.to_string(),
                                board: step.board.to_string(),
                                done: step.done as u32,
                                total: step.total as u32,
                                message: format!(
                                    "{verb} {} ({} of {})",
                                    step.board,
                                    step.done + 1,
                                    step.total
                                ),
                                work: None,
                            });
                        },
                    )
                }),
            };
            match outcome {
                Ok(outcome) => {
                    // The schematic layout file beside the source file: read once, for all of
                    // its boards. A file that is not there is not wrong; one that does not read
                    // is a problem, and the drawing is the one there is without it.
                    let hints_path = netsmith_view::schematic_hints_path(&file.path);
                    let hints_name = hints::name_of(&file.name);
                    let hints_file = hints::read_in(&hints_path, unsaved);
                    let source_boards: Vec<String> = outcome
                        .boards
                        .iter()
                        .map(|b| b.design.name.clone())
                        .collect();
                    snapshot.problems.extend(hints_file.problems(
                        &hints_name,
                        &source_boards.iter().map(String::as_str).collect::<Vec<_>>(),
                    ));
                    let layout = Some((
                        crate::layout::layout_path(&file.path),
                        outcome
                            .boards
                            .iter()
                            .map(|b| b.design.name.clone())
                            .collect::<Vec<_>>(),
                    ));
                    let drawn = outcome.boards.len() as u32;
                    for (n, built) in outcome.boards.into_iter().enumerate() {
                        let label = built.label();
                        // The entries of the layout file that match this board, and the ones
                        // the drawing cannot honour.
                        let groups = module_groups(&built);
                        let (resolved, mut hint_findings) = match hints_file.hints() {
                            Some(h) => h.resolve(&built.design, &groups),
                            None => Default::default(),
                        };
                        // One board's drawing that panics is that board's problem: the rest
                        // are drawn.
                        let drawing = contained(|| {
                            observe(AnalysisEvent::Progress {
                                stage: "draw".to_string(),
                                board: label.clone(),
                                done: n as u32,
                                total: drawn,
                                message: format!("drawing {label} ({} of {drawn})", n + 1),
                                work: None,
                            });
                            *current.borrow_mut() = Some(Where {
                                stage: "draw",
                                board: label.clone(),
                                done: n as u32,
                                total: drawn,
                            });
                            // The symbols come from KiCad's libraries; without them the
                            // schematic draws boxes and says why.
                            job::reporting(Rc::clone(&sink), || {
                                (*libraries).as_ref().map_err(Clone::clone).and_then(|l| {
                                    netsmith_view::layout_sheets_hinted_findings(
                                        &built.design,
                                        &groups,
                                        &rails_of(&built),
                                        &must_connect(&built),
                                        &resolved,
                                        &l.mapping,
                                        &l.symbols,
                                    )
                                    .map(|(layout, unusable)| {
                                        (
                                            netsmith_view::render_layout(&built.design, &layout),
                                            unusable,
                                        )
                                    })
                                    .map_err(|e| e.to_string())
                                })
                            })
                        });
                        let svg = match drawing {
                            Ok(Ok((svg, unusable))) => {
                                hint_findings.extend(unusable);
                                Ok(svg)
                            }
                            Ok(Err(shown)) => Err(shown),
                            Err(message) => {
                                eprintln!(
                                    "netsmith editor: drawing the schematic of {label} panicked: {message}"
                                );
                                let problem = internal_problem(
                                    &format!("drawing the schematic of {label}"),
                                    &message,
                                    Some(label.clone()),
                                );
                                let shown = problem.message.clone();
                                snapshot.problems.push(problem);
                                Err(shown)
                            }
                        };
                        hint_findings
                            .sort_by(|a, b| (a.line, &a.message).cmp(&(b.line, &b.message)));
                        for finding in &hint_findings {
                            snapshot.problems.push(hints::finding_problem(
                                finding,
                                &hints_name,
                                hints_file.text(),
                                Some(label.clone()),
                            ));
                        }
                        let solver_notes = built.notes.len() - built.lowering_notes;
                        for (n, note) in built.notes.iter().enumerate() {
                            snapshot
                                .problems
                                .push(note_problem(&built, note, n >= solver_notes));
                        }
                        for guideline in &built.guidelines {
                            snapshot.problems.push(guideline_problem(&built, guideline));
                        }
                        for issue in &built.power {
                            snapshot.problems.push(power_problem(&built, issue));
                        }
                        snapshot.boards.push((
                            Arc::new(BoardData {
                                file: file.name.clone(),
                                built,
                                svg,
                                libraries: libraries.clone(),
                                layout: layout.clone(),
                                constraints_warn: catalogs.constraints_warn(),
                                hints: hints::Hints {
                                    path: hints_path.clone(),
                                    name: hints_name.clone(),
                                    boards: source_boards.clone(),
                                    findings: hint_findings,
                                },
                            }),
                            false,
                        ));
                    }
                    for spec in outcome.panels {
                        snapshot.panels.push((
                            Arc::new(PanelData {
                                file: file.name.clone(),
                                spec,
                            }),
                            false,
                        ));
                    }
                    for solved in outcome.systems {
                        snapshot.systems.push((
                            Arc::new(SystemData {
                                file: file.name.clone(),
                                solved,
                            }),
                            false,
                        ));
                    }
                }
                Err(problems) => {
                    snapshot
                        .problems
                        .extend(problems.iter().map(|p| problem(Severity::Error, p)));
                    if let Some(previous) = previous {
                        snapshot.boards.extend(
                            previous
                                .boards
                                .iter()
                                .filter(|(b, _)| b.file == file.name)
                                .map(|(b, _)| (Arc::clone(b), true)),
                        );
                        snapshot.systems.extend(
                            previous
                                .systems
                                .iter()
                                .filter(|(s, _)| s.file == file.name)
                                .map(|(s, _)| (Arc::clone(s), true)),
                        );
                        snapshot.panels.extend(
                            previous
                                .panels
                                .iter()
                                .filter(|(p, _)| p.file == file.name)
                                .map(|(p, _)| (Arc::clone(p), true)),
                        );
                    }
                }
            }
        }
        self.check_modules(&target, unsaved, &loader, &mut snapshot, observe);
        // A mistake in a module shows once, however many files import it.
        let mut seen = Vec::new();
        snapshot.problems.retain(|p| {
            let key = (p.severity, p.message.clone(), p.at.clone());
            let new = !seen.contains(&key);
            seen.push(key);
            new
        });
        snapshot
    }

    fn check_modules(
        &self,
        target: &crate::project::Target,
        unsaved: &HashMap<PathBuf, String>,
        loader: &Loader,
        snapshot: &mut Snapshot,
        observe: &dyn Fn(AnalysisEvent),
    ) {
        let Some(project) = &target.project else {
            return;
        };
        let Ok(modules) = project.modules() else {
            return;
        };
        let modules: Vec<_> = modules
            .into_iter()
            .filter(|module| {
                !target
                    .files
                    .iter()
                    .any(|f| same_file(&f.path, &module.path))
            })
            .collect();
        let total = modules.len() as u32;
        // Modules check in a few milliseconds each: the first is told, and then one in each
        // `MODULE_PROGRESS_EVERY`, so that fifty of them are not fifty messages at once.
        let mut told: Option<std::time::Instant> = None;
        for (index, module) in modules.into_iter().enumerate() {
            if told.is_none_or(|at| at.elapsed() >= MODULE_PROGRESS_EVERY) {
                told = Some(std::time::Instant::now());
                observe(AnalysisEvent::Progress {
                    stage: "modules".to_string(),
                    board: module.name(&project.root),
                    done: index as u32,
                    total,
                    message: format!(
                        "checking module {} ({} of {total})",
                        module.name(&project.root),
                        index + 1
                    ),
                    work: None,
                });
            }
            let text = match unsaved.get(&module.path) {
                Some(text) => text.clone(),
                None => match fs::read_to_string(&module.path) {
                    Ok(text) => text,
                    Err(_) => continue,
                },
            };
            let checked = check(&module.name(&project.root), &text, loader);
            snapshot.problems.extend(
                crate::problems_of(&checked)
                    .iter()
                    .map(|p| problem(Severity::Error, p)),
            );
        }
    }

    pub(super) fn board_detail(&self, name: &str) -> Result<BoardDetail, ApiError> {
        let state = self.fresh();
        let data = state.snapshot.board(name)?;
        let built = &data.built;
        let trace = &built.trace;
        let parts = built
            .design
            .parts
            .iter()
            .zip(&built.sourcing)
            .zip(&trace.origins)
            .map(|((part, source), &inst)| {
                let model = &trace.model;
                let at = |id| {
                    let (src, span) = model.instance(id).at();
                    locate(&trace.sources, src, span)
                };
                let mut scopes = Vec::new();
                let mut cur = model.instance(inst).parent;
                while let Some(parent) = cur.filter(|p| *p != model.root) {
                    if model.instance(parent).declared.is_some() {
                        scopes.push(api_location(at(parent)));
                    }
                    cur = model.instance(parent).parent;
                }
                PartInfo {
                    refdes: part.refdes.clone(),
                    kind: part.kind.clone(),
                    value: part.value.clone(),
                    package: part.package.clone(),
                    path: model.path_of(inst),
                    lcsc: source.lcsc.clone(),
                    assembled: source.assembled(),
                    decl: model
                        .instance(inst)
                        .declared
                        .map(|_| api_location(at(inst))),
                    scopes,
                    solved: super::solved::of(model, inst, source),
                }
            })
            .collect();
        let by_refdes: HashMap<&str, &netsmith_core::Part> = built
            .design
            .parts
            .iter()
            .map(|part| (part.refdes.as_str(), part))
            .collect();
        let nets = built
            .design
            .nets
            .iter()
            .map(|n| NetInfo {
                name: n.name.clone(),
                named: netsmith_view::layout::is_named(&n.name),
                pins: n
                    .pins
                    .iter()
                    .map(|p| {
                        // The part's pin table knows the datasheet's name for the pad.
                        let known = by_refdes.get(p.refdes.as_str()).and_then(|part| {
                            part.pins.iter().find(|pin| pin.pads.contains(&p.pin))
                        });
                        PinInfo {
                            refdes: p.refdes.clone(),
                            pin: p.pin.clone(),
                            name: known
                                .map(|k| k.datasheet.clone())
                                .filter(|name| !name.is_empty()),
                            function: known.and_then(|k| k.function.clone()),
                        }
                    })
                    .collect(),
            })
            .collect();
        Ok(BoardDetail {
            name: built.label(),
            file: data.file.clone(),
            parts,
            nets,
            notes: built.notes.clone(),
        })
    }
}

/// The layout the schematic view draws for `built`, with the inputs the editor gives it
/// (its rails, its must-connect pins, its module groups): what `render_svg_by_module_marked`
/// draws, before it is drawn. For tools that measure the drawing.
pub fn schematic_layout(
    built: &Built,
    libraries: &Libraries,
) -> Result<netsmith_view::Layout, String> {
    netsmith_view::layout_sheets_marked(
        &built.design,
        &module_groups(built),
        &rails_of(built),
        &must_connect(built),
        &libraries.mapping,
        &libraries.symbols,
    )
    .map_err(|e| e.to_string())
}

/// The power nets of a board, from the checker's model: a net a rail draws from is a supply,
/// a net the current returns on is a ground. Which nets are which is the design's to say
/// (a `Rail`'s `pos` and `neg`), not their names.
pub fn rails_of(built: &Built) -> netsmith_view::Rails {
    let mut rails = netsmith_view::Rails::default();
    for balance in built.trace.model.balances() {
        if balance.returns {
            rails.ground.insert(balance.net.clone());
        } else {
            rails.positive.insert(balance.net.clone());
        }
    }
    // A net both a return and a supply (a rail stacked on another) is drawn as a ground only
    // when it is never a supply.
    let both: Vec<String> = rails
        .positive
        .intersection(&rails.ground)
        .cloned()
        .collect();
    for net in both {
        rails.ground.remove(&net);
    }
    rails
}

/// The pins the checker says must be connected (a power or input pin that is not `optional`,
/// any pin that is `required`) that nothing else is on the net of, with the checker's message,
/// so the drawing can mark them. A board that has one does not build, so this is what a pin
/// table of a part under construction shows; it asks the checker's own question and changes
/// nothing in it.
pub fn must_connect(built: &Built) -> netsmith_view::Marks {
    use netsmith_check::{InstKind, PinKind, Pinout};
    let model = &built.trace.model;
    let refdes: std::collections::HashMap<&str, &str> = built
        .design
        .parts
        .iter()
        .map(|p| (p.path.as_str(), p.refdes.as_str()))
        .collect();
    let mut marks = netsmith_view::Marks::default();
    for (id, inst) in model.instances() {
        if inst.kind != InstKind::Part || model.board_of(id).is_none() {
            continue;
        }
        let Pinout::Table(table) = model.pinout(id) else {
            continue;
        };
        let path = model.path_of(id);
        let Some(&refdes) = refdes.get(path.as_str()) else {
            continue;
        };
        for row in &table.rows {
            let must = model.must_be_connected(row.pin, row);
            let company = model
                .net_of(row.pin)
                .is_some_and(|net| net.pins.iter().any(|&p| model.owner_of(p) != Some(id)));
            if !must || company {
                continue;
            }
            let kind = match row.kind {
                PinKind::Power => "a power pin",
                PinKind::Input => "an input",
                PinKind::Output => "an output",
                PinKind::Bidirectional => "a bidirectional pin",
                PinKind::Passive => "a passive pin",
                PinKind::NoConnect => "a no_connect pin",
                PinKind::TriState => "a tri_state output",
            };
            let pads = if row.pads.len() == 1 {
                format!("pad {}", row.pads[0])
            } else {
                format!("pads {}", row.pads.join(", "))
            };
            let message = format!(
                "`{}` is {kind} that must be connected ({pads}, `{}`), but nothing else is on its net",
                model.path_of(row.pin),
                row.name
            );
            for pad in &row.pads {
                marks
                    .must
                    .insert((refdes.to_string(), pad.clone()), message.clone());
            }
        }
    }
    marks
}

/// The parts of a board grouped by the module instance of the board each is inside (a part
/// declared on the board itself is in none), in the order the design lists the parts.
pub fn module_groups(built: &Built) -> Vec<netsmith_view::Group> {
    let model = &built.trace.model;
    let mut groups: Vec<netsmith_view::Group> = Vec::new();
    for (part, &inst) in built.trace.origins.iter().enumerate() {
        let mut top = inst;
        while let Some(parent) = model.instance(top).parent {
            if parent == model.root {
                break;
            }
            top = parent;
        }
        if top == inst || model.instance(top).parent.is_none() {
            continue;
        }
        let name = model.instance(top).name.clone();
        match groups.iter_mut().find(|g| g.name == name) {
            Some(g) => g.parts.push(part),
            None => groups.push(netsmith_view::Group {
                name,
                detail: model.instance(top).type_name.clone(),
                parts: vec![part],
            }),
        }
    }
    groups
}

pub(super) fn api_location(l: crate::Location) -> Location {
    Location {
        file: l.file,
        line: l.line as u32,
        column: l.column as u32,
        end_line: l.end_line as u32,
        end_column: l.end_column as u32,
    }
}

/// A note of the build, at the instance it is about if it names one. A part not chosen is a
/// warning, because the design has a hole; a relation not evaluated and a balance that is
/// incomplete are notes, which say what the solver left alone and are mostly expected.
fn note_problem(built: &Built, note: &str, from_lowering: bool) -> Problem {
    let model = &built.trace.model;
    let at = note_path_at(model, note).map(|inst| {
        let (src, span) = model.instance(inst).at();
        api_location(locate(&built.trace.sources, src, span))
    });
    let severity = if note.contains(": not chosen") {
        Severity::Warning
    } else {
        Severity::Note
    };
    Problem {
        severity,
        message: format!("{}: {note}", built.label()),
        rendered: format!("note: {}: {note}\n", built.label()),
        at,
        stage: Some(if from_lowering {
            ProblemStage::Lower
        } else {
            ProblemStage::Solve
        }),
        board: Some(built.label()),
        path: note_path_at(model, note)
            .map(|inst| model.path_of(inst))
            .filter(|p| !p.is_empty()),
    }
}

/// A finding against a guideline (ADR 0036) is a warning, at the rail or part that carries
/// the current. With `guidelines = "error"` the build fails and it is an error instead.
fn guideline_problem(built: &Built, guideline: &netsmith_lower::guidelines::Guideline) -> Problem {
    let model = &built.trace.model;
    let at = guideline.at.map(|inst| {
        let (src, span) = model.instance(inst).at();
        api_location(locate(&built.trace.sources, src, span))
    });
    let message = format!(
        "{}: {} guideline: {}",
        built.label(),
        guideline.rule,
        guideline.message
    );
    Problem {
        severity: Severity::Warning,
        rendered: format!("warning: {message}\n"),
        message,
        at,
        stage: Some(ProblemStage::Guideline),
        board: Some(built.label()),
        path: guideline
            .at
            .map(|inst| model.path_of(inst))
            .filter(|p| !p.is_empty()),
    }
}

/// What the power model leaves unsettled (issue #187) is a note, at the rail or converter it is
/// about. A rail that can draw more than its supply gives stops the build, so it is an error
/// of the build and not one of these.
fn power_problem(built: &Built, issue: &crate::PowerIssue) -> Problem {
    let message = format!("{}: {} {}", built.label(), issue.subject, issue.text);
    Problem {
        severity: Severity::Note,
        rendered: format!("note: {message}\n"),
        message,
        at: issue.at.clone(),
        stage: Some(ProblemStage::Power),
        board: Some(built.label()),
        path: Some(issue.subject.clone()),
    }
}

/// The instance a note is about: the first path in it that names one, or, of a path that
/// names a field (`pulls7.b.drop`), the instance that owns it.
fn note_path_at(model: &netsmith_check::Model, note: &str) -> Option<netsmith_check::InstId> {
    if let Some((path, _)) = note.split_once(": not chosen") {
        return model.inst_at(path);
    }
    let words = note
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .filter(|w| w.contains('.') && !w.starts_with('.') && !w.ends_with('.'));
    for word in words {
        let mut path = word;
        loop {
            if let Some(inst) = model.inst_at(path) {
                return Some(inst);
            }
            match path.rsplit_once('.') {
                Some((owner, _)) => path = owner,
                None => break,
            }
        }
    }
    None
}

fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (std::path::absolute(a), std::path::absolute(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
