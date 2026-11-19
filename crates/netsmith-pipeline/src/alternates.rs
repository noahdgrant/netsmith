//! The board built again with each `alternate` fitted (issue 1312, owner decision): the checks
//! of the board run for each alternate and say which fails, and the alternate's sourcing is kept
//! for the bill of materials, which lists it as not fitted (DNP).
//!
//! An alternate is checked as the board is: the source is checked with the alternate's
//! declaration in the place of the member's (`check_fitted`), solved and designed. A board that
//! a system places is checked as the system uses it (issue 1414): the system is checked and
//! solved with the alternate fitted, its mates are seated again for the system scene, its power
//! tree is read across the boards, and the member is designed from it. A mistake of any stage,
//! or a rail that can draw more than its supply gives, stops the build and is said under the
//! alternate's statement. The guideline findings of the board (an unconstrained pick, an unstated
//! value, an open circle, a bus role, those of the design stage) are made for the board with the
//! alternate fitted too (issue 1414): each that the board as written does not have is a warning
//! of the board, said under the alternate's statement, or with guidelines made errors a problem.
//! The pick lock holds every row but the member's, whose row is the alternate's to pick, and the
//! alternate's pick is not written to the lock. Then the footprint is compared: the alternate's
//! package, when both parts state one, is the member's, and each pad of the alternate is joined
//! to the same pins of the other parts as the member's same pad (a pad on no net on one part is
//! on none on the other).

use std::collections::{BTreeMap, BTreeSet};

use netsmith_check::{
    Alternate, Assignment, Board, InstId, Loader, Model, Sources, System, SystemBoard, check_fitted,
};
use netsmith_core::Design;
use netsmith_design::guidelines::Guideline;
use netsmith_design::{Designed, Sourced, design_board, design_system_board, system_mates};
use netsmith_solve::{Catalogs, PickLock, Solution, solve, solve_system};

use crate::{Location, Problem, Stage, locate};

/// An alternate that passed every check of the board with it fitted.
#[derive(Debug, Clone)]
pub struct FittedAlternate {
    /// The path of the member it may take the place of, from the board (`u`, `radio.u`).
    pub path: String,
    /// The kind of part it is (`ModuleB`) and the member's (`ModuleA`).
    pub kind: String,
    pub primary: String,
    /// The designator of the member: the footprint the alternate is fitted on.
    pub refdes: String,
    /// Where the alternate comes from, from the board designed with it fitted: what the bill of
    /// materials lists for it, marked not fitted.
    pub sourcing: Sourced,
    /// The statement.
    pub at: Location,
}

/// What the alternates of a board came to: those that passed, and the guideline findings of
/// the board with one fitted that the board as written does not have, each said under the
/// alternate's statement.
#[derive(Debug, Clone, Default)]
pub(crate) struct Rechecked {
    pub fitted: Vec<FittedAlternate>,
    pub guidelines: Vec<Guideline>,
}

/// What the source is, for a check of it again.
pub(crate) struct Source<'a> {
    pub name: &'a str,
    pub text: &'a str,
    pub loader: &'a Loader,
    pub assignment: &'a Assignment,
}

/// The board whose alternates are checked: a stand-alone board, or a member of a system.
#[derive(Clone, Copy)]
pub(crate) enum Unit<'a> {
    Board(&'a Board),
    Member {
        system: &'a System,
        member: &'a SystemBoard,
    },
}

impl Unit<'_> {
    /// The definition the source is checked and the pick lock is kept for.
    fn root(&self) -> &str {
        match self {
            Unit::Board(b) => &b.name,
            Unit::Member { system, .. } => &system.name,
        }
    }

    /// The board, as [`crate::Built::label`] has it.
    fn label(&self) -> String {
        match self {
            Unit::Board(b) => b.name.clone(),
            Unit::Member { system, member } => format!("{}/{}", system.name, member.name),
        }
    }

    /// The alternates of the board, each with its path from the board.
    fn alternates(&self) -> Vec<(&Alternate, String)> {
        match self {
            Unit::Board(b) => b
                .model
                .alternates
                .iter()
                .map(|a| (a, a.path.clone()))
                .collect(),
            Unit::Member { system, member } => {
                let prefix = format!("{}.", member.name);
                system
                    .model
                    .alternates
                    .iter()
                    .filter_map(|a| Some((a, a.path.strip_prefix(&prefix)?.to_string())))
                    .collect()
            }
        }
    }
}

/// What the board as written gave, which each alternate's board is compared with.
pub(crate) struct Primary<'a> {
    pub design: &'a Design,
    pub guidelines: &'a [Guideline],
    /// The solved model the board's findings point into (a system's whole model for a member).
    pub model: &'a Model,
}

/// Check, solve and design `unit` again with each of its alternates fitted.
pub(crate) fn check_each(
    source: &Source<'_>,
    unit: Unit<'_>,
    sources: &Sources,
    primary: &Primary<'_>,
    catalogs: &Catalogs,
) -> Result<Rechecked, Vec<Problem>> {
    let mut out = Rechecked::default();
    let mut problems = Vec::new();
    for (alternate, path) in unit.alternates() {
        match check_one(source, unit, sources, primary, catalogs, alternate, &path) {
            Ok((one, mut findings)) => {
                out.fitted.push(one);
                out.guidelines.append(&mut findings);
            }
            Err(mut more) => problems.append(&mut more),
        }
    }
    if problems.is_empty() {
        Ok(out)
    } else {
        Err(problems)
    }
}

/// The board designed with an alternate fitted, and what it was made from.
struct Made {
    solution: Solution,
    sources: Sources,
    designed: Designed,
    /// The member of the system, for a board a system places.
    member: Option<InstId>,
}

fn check_one(
    source: &Source<'_>,
    unit: Unit<'_>,
    sources: &Sources,
    primary: &Primary<'_>,
    catalogs: &Catalogs,
    alternate: &Alternate,
    path: &str,
) -> Result<(FittedAlternate, Vec<Guideline>), Vec<Problem>> {
    let at = locate(sources, alternate.source, alternate.span);
    let board_label = unit.label();
    let label = format!(
        "with `alternate .{}: {}` ({}:{}) fitted in the place of `{path}`",
        leaf(path),
        alternate.kind,
        at.file,
        at.line,
    );
    let under = |mut p: Problem| {
        let said = format!("{label}: {}", p.message);
        p.rendered = p.rendered.replacen(&p.message, &said, 1);
        p.message = said;
        p.board = Some(board_label.clone());
        p
    };
    let made = make(source, unit, catalogs, alternate)
        .map_err(|problems| problems.into_iter().map(&under).collect::<Vec<_>>())?;
    let footprint = footprint_problems(primary.design, &made.designed.design, alternate, path);
    if !footprint.is_empty() {
        return Err(footprint
            .into_iter()
            .map(|m| {
                let mut p = general(Stage::Design, m);
                p.at = Some(at.clone());
                p.path = Some(path.to_string());
                under(p)
            })
            .collect());
    }
    // The findings of the board with the alternate fitted that the board as written lacks.
    let said: BTreeSet<(&str, &str)> = primary
        .guidelines
        .iter()
        .map(|g| (g.rule, g.message.as_str()))
        .collect();
    let findings: Vec<Guideline> = findings_of(&made)
        .into_iter()
        .filter(|g| !said.contains(&(g.rule, g.message.as_str())))
        .collect();
    let findings = if catalogs.strict_guidelines() {
        let model = made.member.map_or_else(
            || made.solution.model.clone(),
            |m| made.solution.model.scoped_to(m),
        );
        crate::refuse_guidelines(&board_label, &findings, &model, &made.sources)
            .map_err(|problems| problems.into_iter().map(&under).collect::<Vec<_>>())?;
        Vec::new()
    } else {
        // Said under the statement, and about the instance of the same path on the board as
        // written, whose model the board's findings point into.
        findings
            .into_iter()
            .map(|g| Guideline {
                rule: g.rule,
                message: format!("{label}: {}", g.message),
                at: g
                    .at
                    .and_then(|i| primary.model.inst_at(&made.solution.model.path_of(i))),
            })
            .collect()
    };
    let index = made
        .designed
        .design
        .parts
        .iter()
        .position(|p| p.path == path);
    let member = primary.design.parts.iter().find(|p| p.path == path);
    let (Some(index), Some(member)) = (index, member) else {
        return Err(vec![under(general(
            Stage::Design,
            format!("`{path}` is no part of the design"),
        ))]);
    };
    Ok((
        FittedAlternate {
            path: path.to_string(),
            kind: alternate.kind.clone(),
            primary: alternate.primary.clone(),
            refdes: member.refdes.clone(),
            sourcing: made.designed.sourcing[index].clone(),
            at,
        },
        findings,
    ))
}

/// Check, solve, power-check and design the board with `alternate` fitted: a board alone, or
/// the system that places it, with its mates and its power tree across the boards.
fn make(
    source: &Source<'_>,
    unit: Unit<'_>,
    catalogs: &Catalogs,
    alternate: &Alternate,
) -> Result<Made, Vec<Problem>> {
    let fitted = BTreeMap::from([(alternate.path.clone(), alternate.index)]);
    let root = unit.root();
    let checked = check_fitted(
        source.name,
        source.text,
        source.loader,
        source.assignment,
        root,
        &fitted,
    );
    if !checked.is_ok() {
        return Err(crate::problems_of(&checked));
    }
    let not_made = || vec![general(Stage::Check, "the board was not made".into())];
    let catalogs = &without_lock_of(catalogs, root, &alternate.path);
    let label = unit.label();
    let solved = |solution: &Solution| {
        if solution.diagnostics.is_empty() {
            Ok(())
        } else {
            Err(solution
                .diagnostics
                .iter()
                .map(|d| Problem::of(Stage::Solve, d, &checked.sources))
                .collect::<Vec<_>>())
        }
    };
    let design_errors = |errors: Vec<String>| {
        errors
            .into_iter()
            .map(|e| general(Stage::Design, e))
            .collect::<Vec<_>>()
    };
    let (solution, designed, member) = match unit {
        Unit::Board(board) => {
            let board = checked.board(&board.name).ok_or_else(not_made)?;
            let solution = crate::quietly(|| solve(board, catalogs));
            solved(&solution)?;
            // A rail that can draw more than its supply gives with the alternate fitted.
            let issues = crate::power::board_tree(&solution.model, &checked.sources).issues;
            crate::refuse_power(&label, &issues, |_| true)?;
            let designed = design_board(board, &solution).map_err(design_errors)?;
            (solution, designed, None)
        }
        Unit::Member { system, member } => {
            let system = checked.system(&system.name).ok_or_else(not_made)?;
            let member = system
                .boards
                .iter()
                .find(|m| m.name == member.name)
                .ok_or_else(not_made)?;
            let solution = crate::quietly(|| solve_system(system, catalogs));
            solved(&solution)?;
            // The seats of the mates, for the system scene, with the alternate fitted.
            system_mates(system, &solution).map_err(design_errors)?;
            let scopes: Vec<(InstId, Option<String>)> = system
                .boards
                .iter()
                .map(|m| (m.inst, Some(m.name.clone())))
                .collect();
            let issues =
                crate::power::system_tree(&solution.model, &checked.sources, &scopes).issues;
            crate::refuse_power(&label, &issues, |i| {
                i.board.as_deref() == Some(member.name.as_str())
            })?;
            let designed = design_system_board(system, member, &solution).map_err(design_errors)?;
            (solution, designed, Some(member.inst))
        }
    };
    Ok(Made {
        solution,
        sources: checked.sources.clone(),
        designed,
        member,
    })
}

/// The guideline findings of the board with an alternate fitted, as the board's own are made.
fn findings_of(made: &Made) -> Vec<Guideline> {
    let (solution, sources, board) = (&made.solution, &made.sources, made.member);
    let mut findings = made.designed.guidelines.clone();
    findings.extend(crate::open_circle_findings(solution, sources, board));
    findings.extend(netsmith_catalog::values::unconstrained_pick_findings(
        solution, sources, board,
    ));
    findings.extend(netsmith_catalog::values::unstated_value_findings(
        solution, sources, board,
    ));
    findings.extend(crate::bus_roles::findings(solution, board));
    findings
}

fn general(stage: Stage, message: String) -> Problem {
    let rendered = format!("error: {message}\n");
    Problem::general(stage, message, rendered)
}

fn leaf(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

/// `catalogs`, with the pick lock's row of the part at `path` of `unit` left out.
fn without_lock_of(catalogs: &Catalogs, unit: &str, path: &str) -> Catalogs {
    let Some(lock) = catalogs.pick_lock() else {
        return catalogs.clone();
    };
    let mut lock: PickLock = lock.clone();
    let rows = lock
        .rows(unit)
        .iter()
        .filter(|r| r.path != path)
        .cloned()
        .collect();
    lock.set(unit, rows);
    catalogs.clone().with_pick_lock(lock)
}

/// What differs between the footprint of the member and the alternate's: the package, and for
/// each pad the pins of the other parts it is joined to.
fn footprint_problems(
    design: &Design,
    fitted: &Design,
    alternate: &Alternate,
    path: &str,
) -> Vec<String> {
    let (Some(primary), Some(other)) = (
        design.parts.iter().find(|p| p.path == path),
        fitted.parts.iter().find(|p| p.path == path),
    ) else {
        return Vec::new();
    };
    let mut problems = Vec::new();
    if let (Some(a), Some(b)) = (&primary.package, &other.package)
        && a != b
    {
        problems.push(format!(
            "an alternate is fitted on the footprint of the part it takes the place of, and `{}` is a `{}` in `{a}` while `{}` is in `{b}`",
            path, alternate.primary, alternate.kind
        ));
    }
    let joins_a = joins(design, &primary.refdes);
    let joins_b = joins(fitted, &other.refdes);
    let pads: BTreeSet<&String> = joins_a.keys().chain(joins_b.keys()).collect();
    let empty = BTreeSet::new();
    for pad in pads {
        let a = joins_a.get(pad).unwrap_or(&empty);
        let b = joins_b.get(pad).unwrap_or(&empty);
        if a != b {
            problems.push(format!(
                "pad {pad} of `{}` is joined to {} with `{}` fitted and to {} with `{}`: an alternate's pads carry the nets of the footprint it is fitted on",
                path,
                list(a),
                alternate.primary,
                list(b),
                alternate.kind
            ));
        }
    }
    problems
}

/// For each pad of the part `refdes` that is on a net, the pins of the other parts
/// on that net, by their parts' paths, which do not change when designators are renumbered.
fn joins(design: &Design, refdes: &str) -> BTreeMap<String, BTreeSet<String>> {
    let path_of: BTreeMap<&str, &str> = design
        .parts
        .iter()
        .map(|p| (p.refdes.as_str(), p.path.as_str()))
        .collect();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for net in &design.nets {
        let others: BTreeSet<String> = net
            .pins
            .iter()
            .filter(|p| p.refdes != refdes)
            .map(|p| {
                let part = path_of.get(p.refdes.as_str()).copied().unwrap_or(&p.refdes);
                format!("{part}.{}", p.pin)
            })
            .collect();
        for pin in net.pins.iter().filter(|p| p.refdes == refdes) {
            out.entry(pin.pin.clone())
                .or_default()
                .extend(others.iter().cloned());
        }
    }
    out
}

fn list(pins: &BTreeSet<String>) -> String {
    if pins.is_empty() {
        "nothing".to_string()
    } else {
        pins.iter()
            .map(|p| format!("`{p}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}
