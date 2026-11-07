//! The guideline about a bus role (ADR 0064): a `bidirectional` row on a line whose role gives
//! a direction is accepted, and the part is asked to say `input`, `output` or `tri_state`, as its
//! datasheet does. It is a finding of the kind a project may make blocking (`guidelines =
//! "error"`, `--strict`), because it is advice: the part may be right.

use netsmith_check::{InstId, Model};
use netsmith_lower::guidelines::Guideline;
use netsmith_solve::Solution;

/// One finding for each `bidirectional` row of a part that states its role and has a line the
/// role gives a direction. For one board of a system, only those inside it.
pub(crate) fn bidirectional_findings(solution: &Solution, board: Option<InstId>) -> Vec<Guideline> {
    let scoped;
    let model: &Model = match board {
        Some(b) => {
            scoped = solution.model.scoped_to(b);
            &scoped
        }
        None => &solution.model,
    };
    netsmith_check::bidirectional_rows(model)
        .into_iter()
        .map(|(part, message)| Guideline {
            rule: "bus role",
            message,
            at: Some(part),
        })
        .collect()
}
