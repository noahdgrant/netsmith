//! What the tests of the binary share: the parity gate of a ported board, which runs `netsmith
//! fab` (the shared project helpers are netsmith's `tests/common`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use netsmith::parity::{DesignatorMap, Tolerances, check_text};
use netsmith_kicad::testlib;

/// What a gate on the parity of a ported board says: where the project is, and what to expect
/// of it.
pub struct ParityGate<'a> {
    /// The project's folder (`examples/<name>`).
    pub project: PathBuf,
    /// The name of the test's temporary folder.
    pub tag: &'a str,
    /// The parts that `reference/parts.toml` names, each of which the fab board must have.
    pub parts: usize,
    /// The number of must-match items the checker must judge `ok`, at least: the count the
    /// board's README states, so that a lost item is noticed (a gate may be raised, never lowered
    /// without the README saying why).
    pub min_ok: usize,
    /// The seconds `fab` may route (`--route-timeout`).
    pub route_seconds: &'a str,
    /// How the committed overlay is made again, for the message of a stale one.
    pub regenerate: &'a str,
    /// The must-match items (by their `id`, as the report prints it) that the board is known not
    /// to meet and that its README says why. Each must fail, and no other may: a fix is noticed
    /// as much as a loss. The boards that match everything have none.
    pub known_failures: &'a [&'a str],
}

/// Builds the project with `netsmith fab --locked`, with no KiCad, and gates on the parity
/// checker (`reference/placements.toml`, the original's parts paired with the fab board's by
/// `reference/parts.toml` and each footprint's `netsmith_path`): every must-match item
/// must be `ok` on the board `fab` writes, complete or INCOMPLETE (a run that the route timeout
/// stopped fails with `INCOMPLETE:`, which is allowed; any other failure of `fab` fails). The
/// committed `reference/parity-overlay.svg` must be the picture of that board: the placer is
/// deterministic, and the router has no say in where the parts are.
pub fn fab_matches_the_reference(gate: &ParityGate) {
    let reference = gate.project.join("reference");
    let parts = netsmith::port::PortParts::read(&reference.join("parts.toml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let out = testlib::TempDir::unused(gate.tag);
    // `--locked` ends the run when the pick lock is not current and never writes it, so the test
    // leaves the project's folder as it was; it also means `--pins locked`.
    let output = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args([
            "fab",
            "--locked",
            "--route-timeout",
            gate.route_seconds,
            "--out",
        ])
        .arg(&*out)
        .current_dir(&gate.project)
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
    // fails with `INCOMPLETE`, which is allowed: nothing else.
    assert!(
        output.status.success() || text.contains("INCOMPLETE:"),
        "netsmith fab failed, and not for an unfinished route:\n{text}"
    );
    let pcb = find_pcb(&out)
        .unwrap_or_else(|| panic!("fab wrote no .kicad_pcb in {}:\n{text}", out.display()));
    let pcb = fs::read_to_string(&pcb).unwrap();
    // The fab board's footprints hold their member paths (`netsmith_path`), and `parts.toml`
    // names the original's part of each: every part of the original is paired.
    let map = DesignatorMap::from_parts(&parts, &pcb).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        (parts.len(), map.len()),
        (gate.parts, gate.parts),
        "reference/parts.toml names every part of the reference board ({}), and the fab board \
         has each of them",
        gate.parts
    );
    let table = fs::read_to_string(reference.join("placements.toml")).unwrap();
    let report =
        check_text(&table, &pcb, &map, &Tolerances::default()).unwrap_or_else(|e| panic!("{e}"));
    let (ok, failed) = report.must_match_counts();
    let failing: Vec<&str> = report
        .failures()
        .filter(|i| i.must_match)
        .map(|i| i.id.as_str())
        .collect();
    assert!(
        failing == gate.known_failures,
        "the must-match items that fail are {failing:?}, the gate knows {:?}:\n{}",
        gate.known_failures,
        report.text()
    );
    assert!(
        ok >= gate.min_ok && failed == gate.known_failures.len(),
        "{ok} must-match items ok:\n{}",
        report.text()
    );

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
         against {}); the first differing lines:\n{}\n{}\n\n{}",
        committed.lines().count(),
        fresh.lines().count(),
        differing.join("\n"),
        gate.regenerate,
        report.text()
    );
}

/// Every `.kicad_pcb` under `dir`, sorted, so that the answer does not depend on the order the
/// file system lists a folder in.
fn find_pcbs(dir: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            find_pcbs(&path, found);
        } else if path.extension().is_some_and(|e| e == "kicad_pcb") {
            found.push(path);
        }
    }
}

/// The one `.kicad_pcb` that `fab` wrote under `dir`: `None` when there is none, and a panic
/// that lists them when there are several (the gate would not know which board to judge).
fn find_pcb(dir: &Path) -> Option<PathBuf> {
    let mut found = Vec::new();
    find_pcbs(dir, &mut found);
    assert!(
        found.len() <= 1,
        "fab wrote {} boards, and the gate judges exactly one: {found:?}",
        found.len()
    );
    found.pop()
}

/// The project helpers of netsmith's own tests, shared with these.
#[path = "../../../netsmith/tests/common/mod.rs"]
mod shared;
pub use shared::*;
