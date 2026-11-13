//! The source's own writing a drawing may show.

/// What a [`Note`] is made of.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// The text of the `///` (or `//!`) doc comments of a declaration or a definition.
    Doc,
    /// A `==` relation line of the source, as it is written.
    Relation,
}

/// Where in the source a [`Note`] is: a file as it was loaded and a 1-based line.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NoteAt {
    pub file: String,
    pub line: usize,
}

/// A piece of the source's own writing that a drawing may carry onto the page: the doc comments
/// of a board, a module, a part or an interface, and the `==` relations of the design. Existing
/// source information only: nothing here is a new construct of the language, and an ordinary
/// `//` comment is never a note. A schematic draws a note as a framed text box.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Note {
    /// The path of the instance the note belongs to, from the board like the paths of
    /// [`InterfaceInstance`]: the part, module or interface a doc comment is on, or the board
    /// or module whose body states a relation (`""` for the board itself).
    pub path: String,
    pub kind: NoteKind,
    /// The words. A doc comment's lines are joined with a space and a blank `///` line makes a
    /// line break (`\n`); a relation is the text of its line, as written.
    pub text: String,
    /// Where it is written, when the checker's model says.
    pub at: Option<NoteAt>,
    /// The paths of the parts the note is about, in the order the source mentions them: for a
    /// doc comment the declaration it is on (a part itself; an interface's parts, or none for a
    /// board or a module, whose note is the sheet's); for a relation the parts whose fields it
    /// names. A note with no anchors is the sheet's.
    pub anchors: Vec<String>,
    /// For a relation that gave a field its value, what the solver settled on
    /// (`led.current = 8.6mA`), when the model has it.
    pub result: Option<String>,
}
