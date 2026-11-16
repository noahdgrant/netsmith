//! The `layout` guideline (ADR 0030, issues #829 and #994): a guideline of a part's `layout`
//! block with a number that the board does not meet. How the parts are found and measured, and
//! the messages, are in `netsmith_pcb`'s `part_layout.rs`; the placer and the router work
//! towards the guidelines, and this says when the board they end with does not meet one.
//!
//! **A guideline, as the track drop is (ADR 0036, ADR 0030).** It warns: a `warning:` line from
//! `fab`. With `guidelines = "error"` or `--strict` it is an error and `fab` stops before it
//! writes the package. A statement with no number ("close to") is worked towards and never
//! reported. A guideline that the layout file waives (`[Board.waive."part.name"]`, with a
//! reason) is said as waived, with its reason, and is neither.

use netsmith_pcb::{Board, BoardLayout};

/// The rule's name, as a guideline's message names it: `Board: layout guideline: ...`.
pub const RULE: &str = "layout";

/// The findings of a board: the ones to report, and a note for each waived one.
#[derive(Debug, Default, PartialEq)]
pub struct Findings {
    pub reported: Vec<String>,
    pub waived: Vec<String>,
}

/// A finding for each guideline with a number that the board does not meet, apart from the
/// ones `waivers` (the layout file's table of the board) waives.
pub fn findings(board: &Board, waivers: Option<&BoardLayout>) -> Findings {
    let mut out = Findings::default();
    for f in netsmith_pcb::layout_findings(board) {
        match waivers.and_then(|w| w.waiver(&f.part, &f.name)) {
            Some(reason) => out.waived.push(format!(
                "`{}.{}` is waived by the layout file ({reason}): {}",
                f.part, f.name, f.message
            )),
            None => out.reported.push(f.message),
        }
    }
    out
}
