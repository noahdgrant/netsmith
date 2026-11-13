//! The tests of the `netsmith` binary, one executable. Each file of this folder is a module of
//! it; a new file needs a `mod` line here, or its tests do not run. A test that does not start
//! the binary belongs to the library, in `crates/netsmith/tests`. Run one file's tests with
//! `cargo test -p netsmith-cli --test main <file>::`.

mod common;

mod board3d;
mod case_fit;
mod case_model_files;
mod cli_help;
mod compare;
mod constraints;
mod dxf;
mod easyduino_parity;
mod editor_verify;
mod emit_design;
mod guidelines;
mod lock_refresh;
mod orbit_parity;
mod order;
mod order_lock;
mod output;
mod own_part;
mod panel;
mod part_from_symbol;
mod pick_lock;
mod picks;
mod pin_export;
mod pin_swaps;
mod pins;
mod port_skeleton;
mod port_tables;
mod power_issues;
mod power_manifold_parity;
mod project;
mod refresh;
mod route;
mod rp2350_parity;
mod step_model;
mod stock_note;
mod timings;
mod view;
