//! Checks a parsed netsmith file: resolves imports and names, turns literals into
//! quantities, checks dimensions, and builds a model for the solver.

mod buses;
mod busnets;
mod checker;
mod connect;
mod diag;
mod faults;
mod lines;
mod loader;
mod lower;
mod mate;
mod model;
mod pinout;
mod pins;
mod providers;
mod row;
mod shared;

pub use busnets::{bidirectional_rows, unstated_roles};
pub use checker::physical_file_name;
pub use diag::{Diagnostic, Source, SourceId, Sources};
pub use faults::{pin_faults, pin_faults_solved};
pub use loader::Loader;
pub use model::{
    Balance, Catalog, Chain, Contact, ContactSide, Domain, Draw, EnumId, EnumInfo, Field, FieldId,
    FieldKind, GenericUse, GroundGroup, GroundNet, InstId, InstKind, Instance, Mate, Member, Model,
    Net, PartRules, Peripheral, PeripheralFunction, PinKind, PinRow, PinTable, Pinout, Provider,
    RExpr, Relation, Requirement, Role, SharedMember, SharedRes, Tie,
};

use std::collections::BTreeMap;

use checker::Checker;
use netsmith_lang::ast::DefKind;

/// Which provider stands for each use of a generic (ADR 0059): the name of a board or system
/// and, under it, the path of a use (`rf`, `main.reg`) with the id of the provider
/// (`src::rf::AcmeMicroCoax`, a [`Provider::id`]). A use that is not named takes the first
/// provider. A `BTreeMap`, so that nothing depends on a hash order.
pub type Assignment = BTreeMap<String, BTreeMap<String, String>>;

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
    check_with(name, text, loader, &Assignment::new())
}

/// [`check`], with the providers of the generics' uses chosen by `assignment` (ADR 0059).
/// Where it names none, the first provider in scope is used.
pub fn check_with(name: &str, text: &str, loader: &Loader, assignment: &Assignment) -> Checked {
    check_inner(name, text, loader, assignment, None)
}

/// [`check_with`] for one definition of the file only: what a trial of a provider needs,
/// which has no use for the others.
pub fn check_one(
    name: &str,
    text: &str,
    loader: &Loader,
    assignment: &Assignment,
    only: &str,
) -> Checked {
    check_inner(name, text, loader, assignment, Some(only))
}

fn check_inner(
    name: &str,
    text: &str,
    loader: &Loader,
    assignment: &Assignment,
    only: Option<&str>,
) -> Checked {
    let mut checker = Checker::new(loader);
    let root = checker.add_root(name, text);
    let mut boards = Vec::new();
    let mut systems = Vec::new();
    let mut panels = Vec::new();
    for (index, def) in checker.module_defs(root).iter().enumerate() {
        // An enum is a type, not a thing to build: it has nothing to elaborate.
        if def.kind == DefKind::Enum {
            continue;
        }
        if only.is_some_and(|only| def.name.name != only) {
            continue;
        }
        let model = checker.elaborate(root, index, assignment.get(&def.name.name));
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
