//! The solver: choose real parts for a checked board and verify, in the worst case, that
//! everything the design says still holds.

mod catalog;
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
pub use generic::{Alternative, Generic, Need, Orderable, Sense, Series, Spec, nearest, resolve};
pub use levels::NotEvaluated;
pub use place::{Window, window};
pub use solve::{Pick, Solution, Unresolved, solve, solve_system};
pub use table::{Record, Supplier, Table, parse_csv};
