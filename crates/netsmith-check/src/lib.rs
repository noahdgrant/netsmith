//! Checks a parsed netsmith file: resolves imports and names, turns literals into
//! quantities, checks dimensions, and builds a model for the solver.

mod buses;
mod checker;
mod connect;
mod diag;
mod faults;
mod loader;
mod lower;
mod mate;
mod model;
mod pinout;
mod pins;

pub use checker::physical_file_name;
pub use diag::{Diagnostic, Source, SourceId, Sources};
pub use faults::{pin_faults, pin_faults_solved};
pub use loader::Loader;
pub use model::{
    Balance, Catalog, Chain, Contact, ContactSide, Domain, Draw, Field, FieldId, FieldKind,
    GroundGroup, GroundNet, InstId, InstKind, Instance, Mate, Member, Model, Net, PinKind, PinRow,
    PinTable, Pinout, RExpr, Relation, Requirement, Role, Tie,
};

use checker::Checker;
use netsmith_lang::ast::DefKind;

/// One elaborated `board`.
#[derive(Debug, Clone)]
pub struct Board {
    pub name: String,
    pub model: Model,
}

/// One `system`: several boards and the joins between their ports, in one model. The
/// boards keep their own nets (a net never crosses boards); the joins equate the values of
/// ports and balance their currents.
#[derive(Debug, Clone)]
pub struct System {
    pub name: String,
    pub model: Model,
    /// The boards, in the order the system declares them.
    pub boards: Vec<SystemBoard>,
}

/// A board as a member of a system: `.main: PowerBoard`.
#[derive(Debug, Clone)]
pub struct SystemBoard {
    /// The member name in the system (`main`).
    pub name: String,
    /// The board definition it was made from (`PowerBoard`).
    pub type_name: String,
    pub inst: InstId,
}

/// One `panel`: the board it repeats and the panel rules of its fabricator, in one model.
/// The board is not narrowed or joined to anything: a panel repeats it as it is.
#[derive(Debug, Clone)]
pub struct Panel {
    pub name: String,
    pub model: Model,
    /// The board definition it repeats (`.cell: Sensor` gives `Sensor`), if the panel names
    /// one.
    pub cell: Option<String>,
}

pub struct Checked {
    pub boards: Vec<Board>,
    pub systems: Vec<System>,
    pub panels: Vec<Panel>,
    pub diagnostics: Vec<Diagnostic>,
    pub sources: Sources,
}

impl Checked {
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn board(&self, name: &str) -> Option<&Board> {
        self.boards.iter().find(|b| b.name == name)
    }

    pub fn system(&self, name: &str) -> Option<&System> {
        self.systems.iter().find(|s| s.name == name)
    }

    /// The boards that are not part of any system. A board that a system places is made
    /// as that system's, with the values its joins give it, not on its own.
    pub fn standalone_boards(&self) -> impl Iterator<Item = &Board> {
        self.boards.iter().filter(|b| {
            !self
                .systems
                .iter()
                .any(|s| s.boards.iter().any(|m| m.type_name == b.name))
        })
    }

    /// Every diagnostic, formatted with its source line.
    pub fn render(&self) -> String {
        self.diagnostics
            .iter()
            .map(|d| d.render(&self.sources))
            .collect()
    }
}

/// Check one source file. Every definition in it is elaborated: boards become
/// [`Board`]s, and parts, interfaces and modules are checked on their own so mistakes
/// in an unused definition still show up.
pub fn check(name: &str, text: &str, loader: &Loader) -> Checked {
    let mut checker = Checker::new(loader);
    let root = checker.add_root(name, text);
    let mut boards = Vec::new();
    let mut systems = Vec::new();
    let mut panels = Vec::new();
    for (index, def) in checker.module_defs(root).iter().enumerate() {
        let model = checker.elaborate(root, index);
        if matches!(def.kind, DefKind::Board | DefKind::System) {
            checker.extend(pin_faults(&model));
        }
        match def.kind {
            DefKind::Board => boards.push(Board {
                name: def.name.name.clone(),
                model,
            }),
            DefKind::System => {
                let members = model
                    .instance(model.root)
                    .children
                    .iter()
                    .filter_map(|(name, m)| match m {
                        Member::Inst(inst) if model.instance(*inst).kind == InstKind::Board => {
                            Some(SystemBoard {
                                name: name.clone(),
                                type_name: model.instance(*inst).type_name.clone(),
                                inst: *inst,
                            })
                        }
                        _ => None,
                    })
                    .collect();
                systems.push(System {
                    name: def.name.name.clone(),
                    model,
                    boards: members,
                });
            }
            DefKind::Panel => {
                let cell = model
                    .instance(model.root)
                    .children
                    .iter()
                    .find_map(|(_, m)| match m {
                        Member::Inst(inst) if model.instance(*inst).kind == InstKind::Board => {
                            Some(model.instance(*inst).type_name.clone())
                        }
                        _ => None,
                    });
                panels.push(Panel {
                    name: def.name.name.clone(),
                    model,
                    cell,
                });
            }
            _ => {}
        }
    }
    let (diagnostics, sources) = checker.into_diagnostics();
    Checked {
        boards,
        systems,
        panels,
        diagnostics,
        sources,
    }
}
