//! Building the project's files and keeping what came of it: the boards that built, the
//! systems, and every problem.

use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

use netsmith_check::{Loader, check};
use netsmith_editor::{
    ApiError,
    api::{
        Analysis, BoardDetail, BoardSummary, Location, NetInfo, PanelSummary, PartInfo, PinInfo,
        Problem, Severity, SystemSummary,
    },
};

use super::Workspace;
use crate::{Built, Libraries, SolvedSystem, locate, outcome_with};

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
}

impl BoardData {
    pub fn libraries(&self) -> Result<&Libraries, ApiError> {
        (*self.libraries)
            .as_ref()
            .map_err(|e| ApiError::internal(e.clone()))
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
        }
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
    }
}

fn general(message: String) -> Problem {
    Problem {
        severity: Severity::Error,
        rendered: format!("error: {message}\n"),
        message,
        at: None,
    }
}

impl Workspace {
    /// Build every top-level file, with `unsaved` read in place of the files it names, and
    /// check every module on its own so a mistake in one that nothing imports shows too.
    /// `previous` supplies the boards of a file that does not build now.
    pub(super) fn analyze(
        &self,
        unsaved: &HashMap<PathBuf, String>,
        previous: Option<&Snapshot>,
    ) -> Snapshot {
        let mut snapshot = Snapshot::default();
        let loader = match self.target.loader_with(unsaved) {
            Ok(loader) => loader,
            Err(e) => {
                snapshot.problems.push(general(e));
                return snapshot;
            }
        };
        let catalogs = match self.target.catalogs() {
            Ok(catalogs) => catalogs,
            Err(e) => {
                snapshot.problems.push(general(e));
                return snapshot;
            }
        };
        let libraries = Arc::new(Libraries::for_project(
            self.target.project.as_ref().map(|p| p.root.as_path()),
        ));
        for file in &self.target.files {
            let text = match unsaved.get(&file.path) {
                Some(text) => Ok(text.clone()),
                None => fs::read_to_string(&file.path),
            };
            let outcome = match text {
                Err(e) => Err(vec![crate::Problem {
                    message: format!("{}: {e}", file.name),
                    at: None,
                    rendered: format!("{}: {e}\n", file.name),
                }]),
                Ok(text) => outcome_with(&file.name, &text, &loader, &catalogs, None),
            };
            match outcome {
                Ok(outcome) => {
                    let layout = Some((
                        crate::layout::layout_path(&file.path),
                        outcome
                            .boards
                            .iter()
                            .map(|b| b.design.name.clone())
                            .collect::<Vec<_>>(),
                    ));
                    for built in outcome.boards {
                        // The symbols come from KiCad's libraries; without them the schematic
                        // draws boxes and says why.
                        let svg = (*libraries).as_ref().map_err(Clone::clone).and_then(|l| {
                            netsmith_view::render_svg_by_module_marked(
                                &built.design,
                                &module_groups(&built),
                                &rails_of(&built),
                                &must_connect(&built),
                                &l.mapping,
                                &l.symbols,
                            )
                            .map_err(|e| e.to_string())
                        });
                        for note in &built.notes {
                            snapshot.problems.push(note_problem(&built, note));
                        }
                        for guideline in &built.guidelines {
                            snapshot.problems.push(guideline_problem(&built, guideline));
                        }
                        snapshot.boards.push((
                            Arc::new(BoardData {
                                file: file.name.clone(),
                                built,
                                svg,
                                libraries: libraries.clone(),
                                layout: layout.clone(),
                                constraints_warn: catalogs.constraints_warn(),
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
        self.check_modules(unsaved, &loader, &mut snapshot);
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
        unsaved: &HashMap<PathBuf, String>,
        loader: &Loader,
        snapshot: &mut Snapshot,
    ) {
        let Some(project) = &self.target.project else {
            return;
        };
        let Ok(modules) = project.modules() else {
            return;
        };
        for module in modules {
            if self
                .target
                .files
                .iter()
                .any(|f| same_file(&f.path, &module.path))
            {
                continue;
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
                }
            })
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
                    .map(|p| PinInfo {
                        refdes: p.refdes.clone(),
                        pin: p.pin.clone(),
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

/// The power nets of a board, from the checker's model: a net a rail draws from is a supply,
/// a net the current returns on is a ground. Which nets are which is the design's to say
/// (a `Rail`'s `pos` and `neg`), not their names.
fn rails_of(built: &Built) -> netsmith_view::Rails {
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
fn must_connect(built: &Built) -> netsmith_view::Marks {
    use netsmith_check::{InstKind, PinKind, Pinout, Requirement};
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
            let must = matches!(
                (row.kind, row.requirement),
                (_, Some(Requirement::Required)) | (PinKind::Power | PinKind::Input, None)
            );
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
fn module_groups(built: &Built) -> Vec<netsmith_view::Group> {
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
fn note_problem(built: &Built, note: &str) -> Problem {
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
