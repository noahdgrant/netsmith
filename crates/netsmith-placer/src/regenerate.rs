//! Regenerate one module (ADR 0030, issue #825): place the parts of one module instance again
//! with every other part held where it is, and say which nets lost their copper.
//!
//! **Scope.** The module's parts are those whose path is the module's path or starts with it
//! and a `.` or a `[` (`stage` has `stage.input_cap` and `stage.bank[0]`, not `stage2.r1`). A
//! part of the module that the caller holds (the layout file locks it) or that the design's
//! requirements pin ([`Board::pinned`]) stays where it is, as every part outside the module
//! does; the others are placed again.
//!
//! **Region.** They are placed by [`place_subset`] inside the box round their courtyards as
//! they are now, grown by `margin` on each side and cut to the outline, so a regenerated module
//! stays where it was on the board and only its arrangement changes. The parts held, the
//! keepouts and the cutouts are obstacles; the pads of the parts held still pull on the nets.
//! If the parts do not fit, nothing is changed and the error says by how much.
//!
//! **Seed.** The same board, module, parts held, seed and margin always give the same
//! arrangement, and another seed gives another; the caller picks the seed (the editor offers the
//! next one each time it is asked again).
//!
//! **Copper.** The nets with a pad on a part placed again are named
//! ([`Regenerated::nets`]): their copper no longer reaches their pads. The router's
//! `carry_copper` keeps the copper of every other net (of a board routed before the change)
//! and routes only what is left.

use std::collections::BTreeSet;

use netsmith_pcb::{Board, Pt, Rect};

use crate::place::{PlaceError, place_subset};

/// How far round a module's parts the region of a regeneration reaches, in mm, unless the
/// caller says otherwise.
pub const DEFAULT_MARGIN: f64 = 3.0;

/// What [`regenerate`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct Regenerated {
    /// The parts placed again, by reference designator, in the board's order.
    pub parts: Vec<String>,
    /// The parts of the module left where they were (held by the caller, or pinned by the
    /// requirements), by reference designator.
    pub held: Vec<String>,
    /// Where the parts were placed, in board coordinates (mm), cut to the outline.
    pub region: Rect,
    /// The nets with a pad on a part placed again, by name, sorted: their copper is ripped up.
    pub nets: Vec<String>,
}

/// Why a module could not be regenerated. Nothing is changed.
#[derive(Debug, Clone, PartialEq)]
pub enum RegenerateError {
    /// No part of the board is in the module; `nearest` are module paths the board has.
    NoModule {
        module: String,
        nearest: Vec<String>,
    },
    /// Every part of the module is held.
    AllHeld { module: String, held: Vec<String> },
    /// The parts do not fit the region.
    Place(PlaceError),
}

impl std::fmt::Display for RegenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegenerateError::NoModule { module, nearest } if nearest.is_empty() => {
                write!(f, "the board has no part in `{module}`")
            }
            RegenerateError::NoModule { module, nearest } => write!(
                f,
                "the board has no part in `{module}`; its modules include {}",
                nearest
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            RegenerateError::AllHeld { module, held } => write!(
                f,
                "every part of `{module}` is held where it is ({}): unlock one in the layout file to let it move",
                held.join(", ")
            ),
            RegenerateError::Place(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RegenerateError {}

/// Whether the part at `path` is in the module at `module` (or is that part).
pub fn in_module(path: &str, module: &str) -> bool {
    netsmith_pcb::in_module(path, module)
}

/// The module paths of the board's parts: every prefix of a part's path that ends before a `.`
/// or a `[`, sorted, each once.
pub fn module_paths(board: &Board) -> Vec<String> {
    netsmith_pcb::module_paths_of(board.parts.iter().map(|p| p.path.as_str()))
}

/// Place the parts of the module `module` again, inside the box round them grown by `margin`
/// mm, with every other part, every part in `held` (by reference designator) and every pinned
/// part left where it is. See the module's header.
pub fn regenerate(
    board: &mut Board,
    module: &str,
    held: &BTreeSet<String>,
    seed: u64,
    margin: f64,
) -> Result<Regenerated, RegenerateError> {
    let members: Vec<&netsmith_pcb::Placed> = board
        .parts
        .iter()
        .filter(|p| in_module(&p.path, module))
        .collect();
    if members.is_empty() {
        let paths = module_paths(board);
        let nearest = nearest(&paths, module, 5);
        return Err(RegenerateError::NoModule {
            module: module.to_string(),
            nearest,
        });
    }
    let (kept, moved): (Vec<_>, Vec<_>) = members
        .into_iter()
        .partition(|p| held.contains(&p.refdes) || board.pinned.contains(&p.refdes));
    let held: Vec<String> = kept.iter().map(|p| p.refdes.clone()).collect();
    if moved.is_empty() {
        return Err(RegenerateError::AllHeld {
            module: module.to_string(),
            held,
        });
    }
    let hull = moved
        .iter()
        .map(|p| p.courtyard())
        .reduce(|a, b| a.union(&b))
        .expect("at least one part");
    let grown = hull.expanded(margin.max(0.0));
    let o = board.outline;
    let region = Rect::new(
        Pt::new(grown.min.x.max(o.min.x), grown.min.y.max(o.min.y)),
        Pt::new(grown.max.x.min(o.max.x), grown.max.y.min(o.max.y)),
    );
    let parts: Vec<String> = moved.iter().map(|p| p.refdes.clone()).collect();
    let names: Vec<&str> = parts.iter().map(String::as_str).collect();
    let nets: BTreeSet<String> = board
        .nets
        .iter()
        .filter(|n| {
            n.pads
                .iter()
                .any(|pad| names.contains(&pad.refdes.as_str()))
        })
        .map(|n| n.name.clone())
        .collect();
    place_subset(board, &names, region, seed).map_err(RegenerateError::Place)?;
    Ok(Regenerated {
        parts,
        held,
        region,
        nets: nets.into_iter().collect(),
    })
}

/// Up to `count` of `names` nearest `wanted`: those that share its first word first, then by
/// the number of letters to change.
fn nearest(names: &[String], wanted: &str, count: usize) -> Vec<String> {
    let mut scored: Vec<(usize, &String)> =
        names.iter().map(|n| (distance(n, wanted), n)).collect();
    scored.sort();
    scored
        .into_iter()
        .take(count)
        .map(|(_, n)| n.clone())
        .collect()
}

/// The edit distance of two names, by characters.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut last = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let here = row[j + 1];
            row[j + 1] = (row[j + 1] + 1)
                .min(row[j] + 1)
                .min(last + usize::from(ca != *cb));
            last = here;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_in_a_module_by_its_whole_words() {
        assert!(in_module("stage.input_cap", "stage"));
        assert!(in_module("stage.bank[0]", "stage.bank"));
        assert!(in_module("bank[2]", "bank"));
        assert!(in_module("stage", "stage"));
        assert!(!in_module("stage2.r1", "stage"));
        assert!(!in_module("stage.input_cap", ""));
    }

    #[test]
    fn the_nearest_names_come_first() {
        let names = [
            "stage".to_string(),
            "power".to_string(),
            "status".to_string(),
        ];
        assert_eq!(nearest(&names, "stag", 2), vec!["stage", "status"]);
    }
}
