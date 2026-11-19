//! The board built again with each `alternate` fitted (issue 1312, owner decision): the checks
//! of the board run for each alternate and say which fails, and the alternate's sourcing is kept
//! for the bill of materials, which lists it as not fitted (DNP).
//!
//! An alternate is checked as the board is: the source is checked with the alternate's
//! declaration in the place of the member's (`check_fitted`), solved and designed. A mistake of
//! any stage, or a rail that can draw more than its supply gives, stops the build and is said
//! under the alternate's statement. The pick lock holds
//! every row but the member's, whose row is the alternate's to pick, and the alternate's pick is
//! not written to the lock. Then the footprint is compared: the alternate's package, when both
//! parts state one, is the member's, and each pad of the alternate is joined to the same pins of
//! the other parts as the member's same pad (a pad on no net on one part is on none on the
//! other).

use std::collections::{BTreeMap, BTreeSet};

use netsmith_check::{Alternate, Assignment, Board, Loader, Sources, check_fitted};
use netsmith_core::Design;
use netsmith_design::{Sourced, design_board};
use netsmith_solve::{Catalogs, PickLock, solve};

use crate::{Location, Problem, Stage, locate};

/// An alternate that passed every check of the board with it fitted.
#[derive(Debug, Clone)]
pub struct FittedAlternate {
    /// The path of the member it may take the place of (`u`, `radio.u`).
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

/// What the source is, for a check of it again.
pub(crate) struct Source<'a> {
    pub name: &'a str,
    pub text: &'a str,
    pub loader: &'a Loader,
    pub assignment: &'a Assignment,
}

/// Check, solve and design `board` again with each of its alternates fitted. `design` is the
/// board designed with its members as written, which each alternate's footprint is compared
/// with.
pub(crate) fn check_each(
    source: &Source<'_>,
    board: &Board,
    sources: &Sources,
    design: &Design,
    catalogs: &Catalogs,
) -> Result<Vec<FittedAlternate>, Vec<Problem>> {
    let mut fitted = Vec::new();
    let mut problems = Vec::new();
    for alternate in &board.model.alternates {
        match check_one(source, board, sources, design, catalogs, alternate) {
            Ok(one) => fitted.push(one),
            Err(mut more) => problems.append(&mut more),
        }
    }
    if problems.is_empty() {
        Ok(fitted)
    } else {
        Err(problems)
    }
}

fn check_one(
    source: &Source<'_>,
    board: &Board,
    sources: &Sources,
    design: &Design,
    catalogs: &Catalogs,
    alternate: &Alternate,
) -> Result<FittedAlternate, Vec<Problem>> {
    let at = locate(sources, alternate.source, alternate.span);
    let label = format!(
        "with `alternate .{}: {}` ({}:{}) fitted in the place of `{}`",
        leaf(&alternate.path),
        alternate.kind,
        at.file,
        at.line,
        alternate.path
    );
    let under = |mut p: Problem| {
        let said = format!("{label}: {}", p.message);
        p.rendered = p.rendered.replacen(&p.message, &said, 1);
        p.message = said;
        p.board = Some(board.name.clone());
        p
    };
    let fitted = BTreeMap::from([(alternate.path.clone(), alternate.index)]);
    let checked = check_fitted(
        source.name,
        source.text,
        source.loader,
        source.assignment,
        &board.name,
        &fitted,
    );
    if !checked.is_ok() {
        return Err(crate::problems_of(&checked)
            .into_iter()
            .map(under)
            .collect());
    }
    let Some(fitted_board) = checked.board(&board.name) else {
        return Err(vec![under(general(
            Stage::Check,
            "the board was not made".into(),
        ))]);
    };
    let catalogs = &without_lock_of(catalogs, &board.name, &alternate.path);
    let solution = crate::quietly(|| solve(fitted_board, catalogs));
    if !solution.diagnostics.is_empty() {
        return Err(solution
            .diagnostics
            .iter()
            .map(|d| under(Problem::of(Stage::Solve, d, &checked.sources)))
            .collect());
    }
    // A rail that can draw more than its supply gives with the alternate fitted.
    let issues = crate::power::board_tree(&solution.model, &checked.sources).issues;
    crate::refuse_power(&board.name, &issues, |_| true)
        .map_err(|problems| problems.into_iter().map(under).collect::<Vec<_>>())?;
    let designed = design_board(fitted_board, &solution).map_err(|errors| {
        errors
            .iter()
            .map(|e| under(general(Stage::Design, e.clone())))
            .collect::<Vec<_>>()
    })?;
    let footprint = footprint_problems(design, &designed.design, alternate);
    if !footprint.is_empty() {
        return Err(footprint
            .into_iter()
            .map(|m| {
                let mut p = general(Stage::Design, m);
                p.at = Some(at.clone());
                p.path = Some(alternate.path.clone());
                under(p)
            })
            .collect());
    }
    let index = designed
        .design
        .parts
        .iter()
        .position(|p| p.path == alternate.path);
    let primary = design.parts.iter().find(|p| p.path == alternate.path);
    let (Some(index), Some(primary)) = (index, primary) else {
        return Err(vec![under(general(
            Stage::Design,
            format!("`{}` is no part of the design", alternate.path),
        ))]);
    };
    Ok(FittedAlternate {
        path: alternate.path.clone(),
        kind: alternate.kind.clone(),
        primary: alternate.primary.clone(),
        refdes: primary.refdes.clone(),
        sourcing: designed.sourcing[index].clone(),
        at,
    })
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
fn footprint_problems(design: &Design, fitted: &Design, alternate: &Alternate) -> Vec<String> {
    let (Some(primary), Some(other)) = (
        design.parts.iter().find(|p| p.path == alternate.path),
        fitted.parts.iter().find(|p| p.path == alternate.path),
    ) else {
        return Vec::new();
    };
    let mut problems = Vec::new();
    if let (Some(a), Some(b)) = (&primary.package, &other.package)
        && a != b
    {
        problems.push(format!(
            "an alternate is fitted on the footprint of the part it takes the place of, and `{}` is a `{}` in `{a}` while `{}` is in `{b}`",
            alternate.path, alternate.primary, alternate.kind
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
                alternate.path,
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
