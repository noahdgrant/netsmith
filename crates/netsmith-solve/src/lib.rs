//! The solver: choose real parts for a checked board and verify, in the worst case, that
//! everything the design says still holds.

mod burst;
mod busclock;
mod busfigures;
mod catalog;
pub mod channel;
mod charge;
mod charging;
pub mod curve;
mod cycles;
mod dc;
mod eval;
mod expand;
mod generic;
mod jlcpcb;
mod levels;
pub mod lock;
mod mated;
pub mod migrate;
pub mod offers;
mod ohm;
mod place;
mod preferred;
pub mod radio;
mod scenario;
mod sequence;
mod sim;
mod solve;
mod standard;
mod supply_levels;
mod table;

pub use catalog::{
    Availability, Candidate, Catalog, Catalogs, Curved, Derating, DeratingError, Library,
    SearchBudget, SoldOut, Value,
};
pub use channel::{Channel, NamedChannel, PartChannel, PartChannels, Rule};
pub use eval::{EvalError, Val, eval};
pub use generic::{
    Alternative, CHANGE_THE_VALUE, Generic, Need, Orderable, Sense, Series, Spec, alternatives,
    nearest, resolve,
};
pub use jlcpcb::built_in_files;
pub use levels::NotEvaluated;
pub use lock::{Designators, Footprints, LockedFootprint, LockedProvider, LockedRow, PickLock};
pub use place::{PlaceOffset, Window, offsets, window};
pub use preferred::Preferred;
pub use solve::{
    CurrentBound, DcSummary, HandPick, HeldBy, NetCurrent, OpenCircle, OrderCodeBy, Passed,
    PassedWhy, Pick, Seeded, Solution, UnconstrainedPick, Unpicked, Unresolved, UnsolvedNet,
    UnstatedValue, solve, solve_system,
};
pub use table::{Record, Supplier, Table, parse_csv};
