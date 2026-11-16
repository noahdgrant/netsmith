//! The `layout` guideline (ADR 0030, issue #829): a part that a `near` statement of a part's
//! `layout` block finds, further from the pin than the number the datasheet gives. How the parts
//! are found and measured, and the message, are in `netsmith_pcb`'s `part_layout.rs`; the placer
//! pulls the parts there, and this says when the placement it ends with does not meet the number.
//!
//! **A guideline, as the track drop is (ADR 0036, ADR 0030).** It warns: a `warning:` line from
//! `fab`. With `guidelines = "error"` or `--strict` it is an error and `fab` stops before it
//! writes the package. A `near` with no number ("close to") is pulled and never reported.

use netsmith_pcb::Board;

/// The rule's name, as a guideline's message names it: `Board: layout guideline: ...`.
pub const RULE: &str = "layout";

/// A finding for each part a `near` statement with a number finds further away than it.
pub fn findings(board: &Board) -> Vec<String> {
    netsmith_pcb::near_findings(&board.parts, &board.nets)
}
