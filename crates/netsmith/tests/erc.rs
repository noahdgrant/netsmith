//! KiCad's own electrical rules check on the schematics netsmith writes. These run
//! `kicad-cli` (found as `KicadCliExe::locate` does: `$KICAD_CLI`, `PATH`, Windows KiCad
//! under WSL) and are skipped where it is not installed; the first one also needs KiCad's
//! symbol libraries.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use netsmith::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_fab::KicadCliExe;
use netsmith_kicad::testlib::TempDir;
use netsmith_solve::Catalogs;

use crate::common;

/// The violations ERC finds in a schematic, as `type: description`, or `None` if KiCad is
/// not here.
fn erc(name: &str, schematic: &str) -> Option<Vec<String>> {
    let Some(cli) = KicadCliExe::locate_runnable() else {
        eprintln!("skipped: no kicad-cli (set KICAD_CLI, or install KiCad)");
        return None;
    };
    // Windows kicad-cli.exe is happier with a Windows-local folder than a \\wsl$ path; any
    // other kicad-cli takes a plain temporary folder.
    let base = cli.scratch_base().unwrap_or_else(|e| panic!("{e}"));
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let serial = RUNS.fetch_add(1, Ordering::SeqCst);
    let dir = TempDir::new_in(&base, &format!("erc-test-{serial}"));
    std::fs::write(dir.join(format!("{name}.kicad_sch")), schematic).ok()?;
    let report = cli
        .arg(&dir.join("erc.json"))
        .unwrap_or_else(|e| panic!("{e}"));
    let sch = cli
        .arg(&dir.join(format!("{name}.kicad_sch")))
        .unwrap_or_else(|e| panic!("{e}"));
    let ran = netsmith_fab::exclusive::run(|| {
        let mut command = Command::new(cli.exe());
        command.args([
            "sch",
            "erc",
            "--format",
            "json",
            "--severity-all",
            "-o",
            &report,
            &sch,
        ]);
        command
    })
    .unwrap_or_else(|e| panic!("kicad-cli could not be run: {e}"));
    assert!(
        ran.success(),
        "kicad-cli sch erc {sch} failed:\n{}",
        ran.report()
    );
    let text = std::fs::read_to_string(dir.join("erc.json")).ok()?;

    let report: serde_json::Value = serde_json::from_str(&text).ok()?;
    let found = report["sheets"]
        .as_array()?
        .iter()
        .flat_map(|sheet| sheet["violations"].as_array().into_iter().flatten())
        .map(|v| {
            format!(
                "{}: {}",
                v["type"].as_str().unwrap_or(""),
                v["description"].as_str().unwrap_or("")
            )
        })
        .collect();
    Some(found)
}

fn example(name: &str) -> (String, String) {
    let path = format!("{}/../../examples/{name}.nsm", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap();
    (path, text)
}

#[test]
fn every_example_passes_erc_with_kicads_own_symbols() {
    let libraries = Libraries::standard();
    if !libraries.symbols.root().is_dir() {
        eprintln!(
            "skipped: no KiCad symbols at {}",
            libraries.symbols.root().display()
        );
        return;
    }
    for name in [
        "led",
        "linear_regulator",
        "status_indicator",
        "four_layer",
        "two_boards",
    ] {
        let (path, text) = example(name);
        let built = build_with(
            &path,
            &text,
            &Loader::std(),
            &Catalogs::jlcpcb(),
            &libraries,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        for b in built {
            let Some(found) = erc(&b.design.name, &b.schematic) else {
                return;
            };
            assert_eq!(
                with_the_machines_libraries(found),
                Vec::<String>::new(),
                "{name} / {}",
                b.label()
            );
        }
    }
}

/// The violations of ERC without the ones that say a library is not in KiCad's configuration:
/// a runner that has KiCad installed but no user library table (CI) says that of every stock
/// library, and it is about the machine, not the schematic. Every other violation stays, the
/// ones about the embedded symbols included, wherever the table is there.
fn with_the_machines_libraries(found: Vec<String>) -> Vec<String> {
    found
        .into_iter()
        .filter(|v| !v.contains("The current configuration does not include the"))
        .collect()
}

#[test]
fn a_multi_unit_part_from_a_project_library_passes_erc() {
    for (name, source) in [
        ("dual", common::DUAL_BOARD),
        ("shared", common::SHARED_BOARD),
    ] {
        passes_erc(name, source);
    }
}

fn passes_erc(name: &str, source: &str) {
    let dir = TempDir::new("erc-project");
    let libraries = common::project_libraries(dir.path());
    let built = build_with(
        "t.nsm",
        source,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let Some(found) = erc(name, &built[0].schematic) else {
        return;
    };
    // KiCad here has no library table for `acme`, so it says it cannot compare the embedded
    // symbols with their libraries. That is about the libraries, not the circuit.
    assert_eq!(electrical(found), Vec::<String>::new());
}

/// The violations of ERC that are about the circuit: the ones about library tables are left
/// out, because KiCad here has no table for the invented libraries or for our sample copies
/// of its own.
fn electrical(found: Vec<String>) -> Vec<String> {
    found
        .into_iter()
        .filter(|v| !v.starts_with("lib_symbol") && !v.starts_with("footprint_link"))
        .collect()
}

fn chip_passes_erc(name: &str, source: &str) {
    let dir = TempDir::new("erc-chip");
    let libraries = common::chip_libraries(
        dir.path(),
        &common::chip_symbol(&common::CHIP_PINS),
        &common::chip_footprint(&["1", "2", "3", "4", "5", "6"]),
    );
    let built = build_with(
        "t.nsm",
        &common::chip_source_of(source),
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let Some(found) = erc(name, &built[0].schematic) else {
        return;
    };
    assert_eq!(electrical(found), Vec::<String>::new());
}

#[test]
fn pins_the_symbol_types_no_connect_pass_erc_without_a_flag_or_a_label() {
    // Two `NC` pins on a six-pin IC: neither is flagged (KiCad counts a flag on such a pin
    // as a connection) and neither is labelled.
    chip_passes_erc("chip-nc", common::CHIP_BOARD);
}

#[test]
fn a_net_with_a_single_pin_passes_erc_without_a_label() {
    // `VOUT` has the chip's output pin and nothing else on it.
    chip_passes_erc("chip-rails", common::CHIP_RAIL_BOARD);
}
