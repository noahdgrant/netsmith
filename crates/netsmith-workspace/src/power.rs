//! The power tree of a board or system of the editor's snapshot (ADR 0007, ADR 0014). The tree is
//! read from the solved model by `netsmith_pipeline::power`, which the build also asks for the
//! issues that stop it.

use netsmith_check::InstId;
use netsmith_editor::{ApiError, api::PowerTree};
use netsmith_pipeline::power::{board_tree, system_tree};

use super::analysis::Snapshot;

/// The power tree of a board by its name, or of a system by its name, which shows every
/// board in it and the joins between them.
pub fn tree(snapshot: &Snapshot, name: &str) -> Result<PowerTree, ApiError> {
    if let Some(system) = snapshot.system(name) {
        let solved = &system.solved;
        let mut tree = system_tree(&solved.model, &solved.sources, &solved_scopes(solved));
        tree.title = name.to_string();
        return Ok(tree);
    }
    let data = snapshot.board(name)?;
    let trace = &data.built.trace;
    let mut tree = board_tree(&trace.model, &trace.sources);
    tree.title = name.to_string();
    Ok(tree)
}

/// The scope of each board of a solved system: its instance and its name in the system.
pub(crate) fn solved_scopes(
    solved: &netsmith_pipeline::SolvedSystem,
) -> Vec<(InstId, Option<String>)> {
    solved
        .boards
        .iter()
        .map(|(member, _, inst)| (*inst, Some(member.clone())))
        .collect()
}
