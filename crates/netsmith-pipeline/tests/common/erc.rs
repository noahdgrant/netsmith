//! KiCad's own electrical rules check of a schematic netsmith wrote, for the tests that run it: this
//! crate's `erc`, `lt8641` and `passives`, and the board tests of netsmith-tools, which include
//! this file. It runs `kicad-cli` (found as `KicadCliExe::locate_runnable` does: `$KICAD_CLI`,
//! `PATH`, Windows KiCad under WSL) and the check is skipped where it is not installed.

#![allow(dead_code)]

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use netsmith_fab::KicadCliExe;
use netsmith_testkit::TempDir;

/// The violations ERC finds in a schematic of one sheet, as `type: description`, or `None` if
/// KiCad is not here.
pub(crate) fn erc(name: &str, schematic: &str) -> Option<Vec<String>> {
    erc_sheets(name, schematic, &[])
}

/// [`erc`] of a built board's schematic: its root sheet and the sheet of each module file.
pub(crate) fn erc_built(built: &netsmith_pipeline::Built) -> Option<Vec<String>> {
    erc_sheets(&built.design.name, &built.schematic, &built.sheets)
}

/// [`erc`] of a root sheet and the files of its other sheets, `(file name, text)`.
pub(crate) fn erc_sheets(
    name: &str,
    schematic: &str,
    sheets: &[(String, String)],
) -> Option<Vec<String>> {
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
    for (file, text) in sheets {
        std::fs::write(dir.join(file), text).ok()?;
    }
    // The project file `build` writes beside the schematic, with netsmith's ERC pin map (#461).
    std::fs::write(
        dir.join(format!("{name}.kicad_pro")),
        netsmith_kicad::project_file(name),
    )
    .ok()?;
    let report = cli
        .arg(&dir.join("erc.json"))
        .unwrap_or_else(|e| panic!("{e}"));
    let sch = cli
        .arg(&dir.join(format!("{name}.kicad_sch")))
        .unwrap_or_else(|e| panic!("{e}"));
    let ran = netsmith_fab::run_exclusive(|| {
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

/// The violations of ERC without the ones that say a library is not in KiCad's configuration:
/// a runner that has KiCad installed but no user library table (CI) says that of every stock
/// library, and it is about the machine, not the schematic. Every other violation stays, the
/// ones about the embedded symbols included, wherever the table is there.
pub(crate) fn with_the_machines_libraries(found: Vec<String>) -> Vec<String> {
    found
        .into_iter()
        .filter(|v| !v.contains("The current configuration does not include the"))
        .collect()
}
