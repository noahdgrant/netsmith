//! Where the placer put a designator that is not on its own side, above its part, and why
//! (issue #1729).
//!
//! The placer places each designator the layout file does not (a part with no entry, or one a
//! requirement pins) from its own neighbourhood only, once the parts are placed
//! (`netsmith_placer::settle_labels`, whose header has the measures and the order): so a label
//! keeps its place while its own part and what lies round it have not changed, and a part
//! removed elsewhere moves none. [`LabelPlace`] records each designator put off its own side,
//! with what is in the way of each side tried before it, and one that is still on a pad on
//! every side; `fab` lists them in `fab-notes.txt` ([`labels_note`]) and warns of the last.

use crate::board::ReferenceSide;

/// The word for a side of a part in messages: `above`, `below`, `left` or `right`.
pub fn side_word(side: ReferenceSide) -> &'static str {
    match side {
        ReferenceSide::Above => "above",
        ReferenceSide::Below => "below",
        ReferenceSide::Left => "left",
        ReferenceSide::Right => "right",
    }
}

/// A designator the placer put on another side of its part than above, or that it could keep
/// off no pad (issue #1729).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LabelPlace {
    pub refdes: String,
    /// The part's path in the design, which a layout file keys it by.
    pub path: String,
    /// The side its text is on.
    pub side: ReferenceSide,
    /// Each side tried before `side`, in the order they are tried, with what is in the way
    /// there: `C4 pad 1`, `the board edge`, `R3's silkscreen`, `R3's designator`, `U1's
    /// courtyard`.
    pub blocked: Vec<(ReferenceSide, String)>,
    /// The pad the text is still on, or nearer than the silk to pad space, when no side keeps
    /// it off every pad.
    pub on_pad: Option<String>,
}

impl LabelPlace {
    /// `R3 below (above: C4 pad 1)`.
    pub fn short(&self) -> String {
        let why: Vec<String> = self
            .blocked
            .iter()
            .map(|(side, what)| format!("{}: {what}", side_word(*side)))
            .collect();
        if why.is_empty() {
            format!("{} {}", self.refdes, side_word(self.side))
        } else {
            format!(
                "{} {} ({})",
                self.refdes,
                side_word(self.side),
                why.join("; ")
            )
        }
    }

    /// The warning for a designator that is on a pad on every side, `None` for one that is
    /// not.
    pub fn warning(&self) -> Option<String> {
        let pad = self.on_pad.as_ref()?;
        Some(format!(
            "the designator of {} (`{}`) is on or too near a pad on every side of its part: {} it is on {pad}; place it with `netsmith layout set {} --designator DX,DY`, or print none with `.silk &= \"hide\"` in the physical file",
            self.refdes,
            self.path,
            side_word(self.side),
            self.path
        ))
    }
}

/// The line of `fab-notes.txt` for the designators placed off their own side (`labels`), with
/// what is in the way above them; `None` when there are none.
pub fn labels_note(labels: &[LabelPlace]) -> Option<String> {
    let off: Vec<String> = labels
        .iter()
        .filter(|l| l.side != ReferenceSide::Above)
        .map(LabelPlace::short)
        .collect();
    if off.is_empty() {
        return None;
    }
    Some(format!(
        "designators off their own side (above the part), each with what is in the way of the sides tried before it; a designator moves only when its part or what lies round it does: {}",
        off.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_off_its_side_says_what_is_in_the_way_and_one_on_a_pad_is_a_warning() {
        let moved = LabelPlace {
            refdes: "R3".into(),
            path: "stage.r3".into(),
            side: ReferenceSide::Below,
            blocked: vec![(ReferenceSide::Above, "C4 pad 1".into())],
            on_pad: None,
        };
        assert_eq!(moved.short(), "R3 below (above: C4 pad 1)");
        assert_eq!(moved.warning(), None);
        let note = labels_note(std::slice::from_ref(&moved)).unwrap();
        assert!(note.ends_with(": R3 below (above: C4 pad 1)"), "{note}");
        let stuck = LabelPlace {
            on_pad: Some("C4 pad 2".into()),
            side: ReferenceSide::Above,
            blocked: Vec::new(),
            ..moved
        };
        assert!(labels_note(std::slice::from_ref(&stuck)).is_none());
        let warning = stuck.warning().unwrap();
        assert!(warning.contains("above it is on C4 pad 2"), "{warning}");
        assert!(
            warning.contains("layout set stage.r3 --designator"),
            "{warning}"
        );
    }
}
