//! Diff pairs: which two nets make one, and how near their tracks run to count as coupled. The
//! router runs the pairs side by side (`netsmith_router`'s route module, issue #431), the
//! meanders match their lengths (`netsmith_pcb::meander`), and the route scorecard
//! (`netsmith_fab::score`) measures them.

/// How near, centre line to centre line, the other net of a diff pair runs for a stretch of
/// track to count as coupled, in mm.
pub const COUPLED: f64 = 0.5;
/// The suffixes that make two nets a diff pair: the net that ends in the first and the one that
/// ends in the second, with the same name before them. The router runs the same pairs side by
/// side (`netsmith_router`'s route module, issue #431).
pub const PAIR_SUFFIXES: [(&str, &str); 7] = [
    ("dp", "dm"),
    ("DP", "DM"),
    ("D+", "D-"),
    ("_P", "_N"),
    ("+", "-"),
    ("_p", "_n"),
    (".p", ".n"),
];
