//! Why a note of a build is there.

/// Why a note of a build is there: the one thing that would make it go. The tools that make a
/// note say it (a typed field, never read back from the text), and the editor's Problems dock
/// groups the notes by it, with a count for each, so that forty notes read as the few things
/// to do about them (issue #192). The set is closed: a note that fits none is `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cause {
    /// A check waits on a value that nothing states: a relation not evaluated, a bus clock
    /// or a role the design leaves out. State it.
    NotStated,
    /// A part's model leaves a value open that a check needs: a level with no value, an
    /// input current, a pin that is no `Logic`. Complete the part's definition.
    PartModel,
    /// netsmith cannot judge the pin or the net yet: an open-drain high level it cannot
    /// read, a swing it does not know, a network too large for the DC analysis.
    Unjudged,
    /// Values that depend on each other while none has a value, so the checks that use
    /// them are not evaluated. Start the circle with a range.
    Circular,
    /// A rail nothing supplies, a load whose current the solver did not settle, a
    /// converter whose efficiency it did not.
    Supply,
    /// A part that a provider stands in for, or that the catalog row says is out of stock.
    Pick,
    /// A net whose track width is a lower bound, because a current on it is not known.
    Track,
    /// Anything else the build says about what it did.
    Other,
}

impl Cause {
    /// Every cause, in the order the dock lists them.
    pub const ALL: [Cause; 8] = [
        Cause::NotStated,
        Cause::PartModel,
        Cause::Unjudged,
        Cause::Circular,
        Cause::Supply,
        Cause::Pick,
        Cause::Track,
        Cause::Other,
    ];

    /// The short name the editor's API and the page use.
    pub fn id(self) -> &'static str {
        match self {
            Cause::NotStated => "not_stated",
            Cause::PartModel => "part_model",
            Cause::Unjudged => "unjudged",
            Cause::Circular => "circular",
            Cause::Supply => "supply",
            Cause::Pick => "pick",
            Cause::Track => "track",
            Cause::Other => "other",
        }
    }
}
