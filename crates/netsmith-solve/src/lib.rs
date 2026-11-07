//! The solver: choose real parts for a checked board and verify, in the worst case, that
//! everything the design says still holds.

mod busclock;
mod catalog;
mod cycles;
mod eval;
mod expand;
mod generic;
mod jlcpcb;
mod levels;
mod place;
mod solve;
mod table;

pub use catalog::{
    Availability, Candidate, Catalog, Catalogs, Derating, DeratingError, Library, Value,
};
pub use eval::{EvalError, Val, eval};
pub use generic::{
    Alternative, CHANGE_THE_VALUE, Generic, Need, Orderable, Sense, Series, Spec, alternatives,
    nearest, resolve,
};
pub use levels::NotEvaluated;
pub use place::{PlaceOffset, Window, offsets, window};
pub use solve::{
    HeldBy, OpenCircle, Pick, Seeded, Solution, UnconstrainedPick, Unresolved, UnsolvedNet,
    UnstatedValue, solve, solve_system,
};
pub use table::{Record, Supplier, Table, parse_csv};
