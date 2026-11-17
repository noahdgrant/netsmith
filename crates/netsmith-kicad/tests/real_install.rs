//! The lookup of KiCad's libraries against the real folders of the machine the tests run on
//! (issue #791), not against an invented install tree as the unit tests of `install.rs` do.
//!
//! On a machine with no KiCad these tests say so and pass (`test`, and a developer's laptop). The
//! `kicad-lookup` job of CI installs KiCad 9 on Ubuntu, macOS and Windows runners, sets nothing
//! the lookup reads (no `NETSMITH_KICAD_*` variable, no `KICAD_CLI`, KiCad's own folder not on
//! `PATH`) and sets `NETSMITH_REQUIRE_KICAD_LIBRARIES`: then a library that is not found, or that
//! is found through a variable, fails the test instead of skipping it.

use netsmith_kicad::{InstallKind, Search, symbol_in, symbol_names_in};

/// Set (to anything but empty) by the CI job that has installed KiCad.
const REQUIRE: &str = "NETSMITH_REQUIRE_KICAD_LIBRARIES";

fn required() -> bool {
    std::env::var_os(REQUIRE).is_some_and(|v| !v.is_empty())
}

/// The folder of `kind` this machine's lookup finds, or `None` after saying why the test skips.
/// Where the libraries are required, finding none (or one a variable pointed at) fails the test.
fn found(kind: InstallKind) -> Option<std::path::PathBuf> {
    match Search::machine().find(kind) {
        Ok(found) => {
            println!(
                "{} folder: {} (found: {:?})",
                kind.variable(),
                found.dir.display(),
                found.source
            );
            if required() {
                assert_ne!(
                    format!("{:?}", found.source),
                    "Variable",
                    "{REQUIRE} is set, so the lookup must find {} itself, with {} unset",
                    kind.variable(),
                    kind.variable()
                );
            }
            Some(found.dir)
        }
        Err(not_found) => {
            assert!(
                !required(),
                "{REQUIRE} is set, but the lookup finds no KiCad library on this machine: {not_found}"
            );
            eprintln!("skipped: no KiCad symbol and footprint libraries are installed");
            None
        }
    }
}

#[test]
fn the_lookup_finds_the_stock_symbol_folder_of_this_machine() {
    let Some(dir) = found(InstallKind::Symbols) else {
        return;
    };
    assert!(dir.is_dir(), "{} is no folder", dir.display());
    let device = dir.join("Device.kicad_sym");
    assert!(
        device.is_file(),
        "the stock Device library is not in {}",
        dir.display()
    );
    // The real file, read by the crate's own reader (line endings and all): the resistor has two
    // passive pins.
    let text = std::fs::read_to_string(&device).unwrap();
    let names = symbol_names_in(&text, "Device.kicad_sym").unwrap();
    assert!(
        names.iter().any(|n| n == "R"),
        "no symbol R in the stock Device library ({} symbols)",
        names.len()
    );
    let r = symbol_in(&text, "Device.kicad_sym", "R").unwrap();
    assert_eq!(r.pin_numbers(), ["1", "2"]);
}

#[test]
fn the_lookup_finds_the_stock_footprint_folder_of_this_machine() {
    let Some(dir) = found(InstallKind::Footprints) else {
        return;
    };
    assert!(dir.is_dir(), "{} is no folder", dir.display());
    let resistors = dir.join("Resistor_SMD.pretty");
    assert!(
        resistors.is_dir(),
        "the stock Resistor_SMD library is not in {}",
        dir.display()
    );
    let footprint = resistors.join("R_0603_1608Metric.kicad_mod");
    assert!(
        footprint.is_file(),
        "{} is not in the library",
        footprint.display()
    );
}
