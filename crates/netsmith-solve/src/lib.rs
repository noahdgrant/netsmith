//! The solver: choose real parts for a checked board and verify, in the worst case, that
//! everything the design says still holds.

mod burst;
mod busclock;
mod busfigures;
mod catalog;
mod charge;
pub mod curve;
mod cycles;
mod dc;
mod eval;
mod expand;
mod generic;
mod jlcpcb;
mod levels;
mod lock;
mod ohm;
mod place;
mod preferred;
mod sequence;
mod solve;
mod standard;
mod supply_levels;
mod table;

pub use catalog::{
    Availability, Candidate, Catalog, Catalogs, Curved, Derating, DeratingError, Library,
    SearchBudget, SoldOut, Value,
};
pub use eval::{EvalError, Val, eval};
pub use generic::{
    Alternative, CHANGE_THE_VALUE, Generic, Need, Orderable, Sense, Series, Spec, alternatives,
    nearest, resolve,
};
pub use levels::NotEvaluated;
pub use lock::{LockedProvider, LockedRow, PickLock};
pub use place::{PlaceOffset, Window, offsets, window};
pub use preferred::Preferred;
pub use solve::{
    CurrentBound, DcSummary, HeldBy, NetCurrent, OpenCircle, Passed, PassedWhy, Pick, Seeded,
    Solution, UnconstrainedPick, Unpicked, Unresolved, UnsolvedNet, UnstatedValue, solve,
    solve_system,
};
pub use table::{Record, Supplier, Table, parse_csv};
