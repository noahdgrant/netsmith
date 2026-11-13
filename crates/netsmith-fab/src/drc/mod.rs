//! netsmith's own design rules check is `netsmith_geom::drc` (ADR 0037), re-exported here, with
//! KiCad's check beside it as the independent witness the cross-check compares with
//! ([`kicad`]).

pub use netsmith_geom::drc::*;

pub mod kicad;
