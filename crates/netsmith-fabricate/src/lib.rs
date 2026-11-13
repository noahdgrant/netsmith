//! The fab driver: the placed board, its files and the order package, from a built board.
//!
//! It takes what `netsmith-pipeline` builds (`Built`) and places, routes and checks it, and writes
//! the fab package (`fab`, `order`); the files beside a source it reads (the physical file, the pin
//! file, the layout, the project manifest) are here too.

pub mod board3d;
pub mod case_fit;
pub mod fab;
mod fab_environment;
pub mod layout;
pub mod models;
pub mod no_supplier;
pub mod order;
pub mod physical;
pub mod picture;
pub mod pin_export;
pub mod pin_swap;
pub mod pins;
pub mod placed;
pub mod project;
pub mod system3d;
