//! The crate's integration tests, one executable. Each file of this folder is a module of it
//! (a file per subject, as before); a new file needs a `mod` line here, or its tests do not run.
//! Run one file's tests with `cargo test -p netsmith --test main <file>::`, or with
//! `cargo nextest run -p netsmith <file>::`.

mod common;

mod alternatives;
mod bank;
mod board3d;
mod build;
mod catalog_add;
mod catalog_height;
mod cli_help;
mod compare;
mod conflicts;
mod consigned;
mod constraints;
mod defaults;
mod dxf;
mod easyduino;
mod easyduino_parity;
mod editor;
mod editor_cancel;
mod editor_compare;
mod editor_feedback;
mod editor_fields;
mod editor_files;
mod editor_hints;
mod editor_panic;
mod editor_pin_write;
mod editor_sidecars;
mod editor_unicode;
mod editor_verify;
mod enclosure_sensor;
mod environment;
mod erc;
mod fab;
mod fab_environment;
mod fab_no_part_number;
mod generic_parts;
mod guide;
mod guidelines;
mod height;
mod ic_symbols;
mod kicad_rows;
mod lock_refresh;
mod lt8641;
mod mosfet_drive;
mod note_causes;
mod open_circle;
mod orbit;
mod orbit_parity;
mod order;
mod order_lock;
mod output;
mod overhang;
mod own_part;
mod panel;
mod passives;
mod pick_lock;
mod picks;
mod pin_export;
mod pin_swaps;
mod pinout;
mod pins;
mod power_issues;
mod power_manifold;
mod power_manifold_ics;
mod power_manifold_parity;
mod power_manifold_parts;
mod precheck;
mod project;
mod protrusion;
mod providers;
mod providers_editor;
mod providers_fab;
mod pulldown;
mod pullup;
mod refresh;
mod relations;
mod root_spelling;
mod route;
mod row;
mod rp2350_minimal;
mod rp2350_parity;
mod schematic_invariants;
mod search_budget;
mod std_additions;
mod std_catalogs;
mod std_generics;
mod std_kinds;
mod std_logic;
mod std_models;
mod std_zener;
mod stock_note;
mod supply;
mod symbol_gallery;
mod timings;
mod two_buck;
mod unused;
mod usb_c_sink;
mod values;
mod view;

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
