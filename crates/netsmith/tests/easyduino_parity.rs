//! The gate of the Easyduino ESP32-S3 port: the board `netsmith fab` writes for the project in
//! `examples/easyduino_esp32s3` must match Easyduino's where the owner said it must (the
//! outline, the holes and the parts of `reference/must_match.toml`), as judged by the parity
//! checker (`netsmith::parity`, which `rp2350_parity.rs` tests against synthetic boards).
//! Everything else of the board is reported by the checker and not gated.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use netsmith::parity::{DesignatorMap, Tolerances, check_text};
use netsmith_kicad::testlib::TempDir;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/easyduino_esp32s3")
}

/// How long the router may run before `fab` writes the board marked INCOMPLETE.
const ROUTE_SECONDS: &str = "20";

/// Builds the example with `netsmith fab`, with no KiCad (the project needs none, ADR 0097) and
/// the router stopped after [`ROUTE_SECONDS`], and gates on the checker: every must-match item
/// must be `ok` on the board it writes, complete or INCOMPLETE. Any other failure of `fab` (no
/// board written, a requirement not met, a design that does not build) fails the test.
#[test]
fn easyduino_pcb_matches_the_original_where_it_must() {
    let reference = project().join("reference");
    let map =
        DesignatorMap::parse(&fs::read_to_string(reference.join("designators.toml")).unwrap())
            .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        map.len(),
        35,
        "reference/designators.toml pairs every part of Easyduino's board (35)"
    );
    let out = TempDir::unused("easyduino-parity-fab");
    // `--locked` ends the run when the pick lock is not current and never writes it, so the test
    // leaves the project's folder as it was; it also means `--pins locked`.
    let output = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["fab", "--locked", "--route-timeout", ROUTE_SECONDS, "--out"])
        .arg(&*out)
        .current_dir(project())
        .env("NETSMITH_NO_KICAD", "1")
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // A run that routed everything succeeds; one that the timeout stopped writes the board and
    // fails with `INCOMPLETE`, which this test allows: nothing else.
    assert!(
        output.status.success() || text.contains("INCOMPLETE:"),
        "netsmith fab failed, and not for an unfinished route:\n{text}"
    );
    let pcb = find_pcb(&out)
        .unwrap_or_else(|| panic!("fab wrote no .kicad_pcb in {}:\n{text}", out.display()));
    let table = fs::read_to_string(reference.join("placements.toml")).unwrap();
    let report = check_text(
        &table,
        &fs::read_to_string(&pcb).unwrap(),
        &map,
        &Tolerances::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(report.passed(), "{}", report.text());
    let (ok, failed) = report.must_match_counts();
    // The outline, the four holes, J1, J2, SW1, SW2 and U2 with every pad of each.
    assert!(
        ok > 100 && failed == 0,
        "{ok} must-match items ok:\n{}",
        report.text()
    );

    // The committed overlay is the picture of this run (the placer is deterministic: the same
    // source, physical file, layout file and seed give the same board, and the router has no say
    // in where the parts are).
    let committed = fs::read_to_string(reference.join("parity-overlay.svg")).unwrap();
    let fresh = report.overlay_svg();
    let differing: Vec<String> = committed
        .lines()
        .zip(fresh.lines())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .take(12)
        .map(|(n, (a, b))| format!("line {}:\n  committed {a}\n  fresh     {b}", n + 1))
        .collect();
    assert!(
        committed == fresh,
        "reference/parity-overlay.svg is not the overlay of the board `fab` writes ({} lines \
         against {}); the first differing lines:\n{}\nfrom examples/easyduino_esp32s3 run \
         `netsmith fab --locked`, then\n  cargo run -p netsmith --example parity -- \
         fab/EasyduinoEsp32s3/EasyduinoEsp32s3.kicad_pcb --table reference/placements.toml \
         --map reference/designators.toml --svg reference/parity-overlay.svg\n\n{}",
        committed.lines().count(),
        fresh.lines().count(),
        differing.join("\n"),
        report.text()
    );
}

fn find_pcb(dir: &Path) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_pcb(&path) {
                return Some(found);
            }
        } else if path.extension().is_some_and(|e| e == "kicad_pcb") {
            return Some(path);
        }
    }
    None
}
