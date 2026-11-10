//! The crate's integration tests, one executable. Each file of this folder is a module of it
//! (a file per subject, as before); a new file needs a `mod` line here, or its tests do not run.
//! Run one file's tests with `cargo test -p netsmith-solve --test main <file>::`, or with
//! `cargo nextest run -p netsmith-solve <file>::`.

mod backjump;
mod bank;
mod buck_ripple;
mod bus_clock;
mod bus_levels;
mod bus_rise_time;
mod bus_sink_current;
mod cancel;
mod catalog_files;
mod conflict_core;
mod correlated_levels;
mod cycles;
mod data;
mod dc_abs_max;
mod dc_analysis;
mod dc_buck;
mod dc_budget;
mod dc_chain;
mod dc_islands;
mod dc_loads;
mod dc_open_drain;
mod dc_regulators;
mod dc_search;
mod dc_silent;
mod dc_staged;
mod dc_stated_rail;
mod dc_switches;
mod defaults;
mod derived_bounds;
mod empty_cells;
mod end_functions;
mod enums;
mod generic;
mod generic_parts;
mod instance_names;
mod joint_bank;
mod joint_search;
mod levels;
mod mate;
mod member_range_ends;
mod narrowing;
mod ohm_bound;
mod optional_columns;
mod pick_order;
mod pick_tiers;
mod pins;
mod rail_alias;
mod range_ends;
mod range_forms_agree;
mod scaling;
mod search_budget;
mod seed;
mod solve;
mod swing_levels;
mod system;
mod temperature_coefficient;
mod transferred_bounds;
mod translator_levels;
mod two_equations;
mod two_unknowns;
mod unconstrained;
mod unconstrained_gaps;
mod unused;
mod waiting_bounds;

/// A file in this folder without a `mod` line above is not part of the executable: its tests would
/// never run, and nothing else would say so.
#[test]
fn every_test_file_is_declared_above() {
    let declared = include_str!("main.rs");
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "rs")
            && let Some(name) = path.file_stem().and_then(|s| s.to_str())
            && name != "main"
            && !declared.lines().any(|l| l == format!("mod {name};"))
        {
            missing.push(name.to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "add `mod` lines to main.rs for: {missing:?}"
    );
}
